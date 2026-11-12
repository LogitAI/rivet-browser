//! Makes the supported-build row of a new browser build from the row of a prior build.
//!
//! Chromium's source changes little between two builds of a milestone, but every address moves.
//! So each address of the prior row is named through the prior build's public symbols, the same
//! decorated names are looked up in the new build's symbols, and the instruction bytes that guard
//! each hook are read again from the new module. Values that are not addresses (member offsets,
//! table slots, object sizes) are read again from the new module where `row_recipe` has a recipe
//! for them, and a recipe that fails stops the row. The rest are carried over. They have no public
//! symbols, so a row that crosses a milestone lists each of them and is never published without a
//! person's review, and every row must still pass on the real browser.
//!
//! A row is plain data here, in the form the runtime writes: a `usize` is a `0x` string, and bytes
//! are bare hex. A value this code cannot explain stops the row: it goes to review rather than to
//! people's browsers, and the runtime's own prologue checks stand behind that.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::{json, Value};

use crate::chrome_build::{hex, PeFile};
use crate::row_resolve;

/// The lowest value taken for an address. Offsets and slots lie below it.
const LOWEST_ADDRESS: u64 = 0x10_0000;
/// The address a build records for what it does not have.
const ABSENT: u64 = 0x7FFF_0000;
/// How far below an address its naming symbol may lie: a table slot or an instruction in a function.
const NEAREST_SYMBOL: u32 = 0x2000;

/// One symbol: its address in the module and its decorated name.
pub type Symbol = (u32, String);

/// Removes what the linker adds to a decorated name, which differs from build to build.
pub fn stable_name(name: &str) -> &str {
    let name = name.trim_end_matches('\0');
    let name = match name.strip_prefix(".weak.") {
        Some(rest) => rest.split(".default.").next().unwrap_or(rest),
        None => name,
    };
    name.split(".llvm.").next().unwrap_or(name)
}

/// What the generator found, for the person or job that decides what to do with the row.
#[derive(Debug, Default, Serialize)]
pub struct PortReport {
    pub browser: String,
    pub version: String,
    pub prior_version: String,
    pub addresses: usize,
    pub mapped: usize,
    /// Why this row must not be published without a person. Empty means none was found.
    pub review: Vec<String>,
    /// Addresses with several possible new places, settled by an independent check.
    pub settled: Vec<String>,
    /// Addresses inside a symbol rather than at its start, moved by the same distance.
    pub interior: Vec<String>,
    /// Addresses found through private symbols, since no public one covers them.
    pub private: Vec<String>,
    pub bytes_changed: Vec<String>,
    pub slots_moved: Vec<String>,
    /// Values that are not addresses, read again from the new module by their recipes.
    pub derived: Vec<String>,
    /// Values that are not addresses and have no recipe, so they were carried over.
    pub carried: Vec<String>,
    /// Optional parts the new build could not supply. The runtime does without them.
    pub left_out: Vec<String>,
    /// The new build had no symbols, so every address was looked for by the prior row's fingerprints.
    pub made_without_pdb: bool,
    /// Addresses a fingerprint found in exactly one place of the new build.
    pub fingerprinted: Vec<String>,
    /// Values that are Rivet's settings rather than facts about the build, kept from the prior row.
    pub settings: Vec<String>,
}

fn is_address(value: u64) -> bool {
    (LOWEST_ADDRESS..ABSENT).contains(&value)
}

fn read_address(value: &Value) -> Option<u64> {
    let digits = value.as_str()?.strip_prefix("0x")?;
    u64::from_str_radix(digits, 16).ok()
}

fn write_address(value: u64) -> Value {
    Value::String(format!("{value:#x}"))
}

/// Visits every `0x` value of a row with its path.
fn walk_addresses(path: &str, value: &mut Value, visit: &mut dyn FnMut(&str, &mut Value)) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                walk_addresses(&format!("{path}.{key}"), item, visit);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                walk_addresses(&format!("{path}[{index}]"), item, visit);
            }
        }
        Value::String(text) if text.starts_with("0x") => visit(path, value),
        _ => {}
    }
}

/// The addresses a row holds.
pub fn addresses_of(row: &Value) -> BTreeSet<u32> {
    let mut found = BTreeSet::new();
    let mut copy = row.clone();
    walk_addresses("", &mut copy, &mut |_, value| {
        if let Some(address) = read_address(value).filter(|value| is_address(*value)) {
            found.insert(address as u32);
        }
    });
    found
}

/// Names each address: all the names of the nearest symbol at or below it, which are many where
/// the linker folded identical functions, and how far above that symbol it lies.
pub fn name_addresses(
    targets: &BTreeSet<u32>,
    sorted_symbols: &[Symbol],
) -> BTreeMap<u32, (u32, Vec<String>)> {
    let mut named = BTreeMap::new();
    for target in targets {
        let end = sorted_symbols.partition_point(|(address, _)| address <= target);
        let Some((nearest, _)) = end.checked_sub(1).map(|at| &sorted_symbols[at]) else {
            continue;
        };
        if target - nearest > NEAREST_SYMBOL {
            continue;
        }
        let start = sorted_symbols.partition_point(|(address, _)| address < nearest);
        let names = sorted_symbols[start..end]
            .iter()
            .map(|(_, name)| stable_name(name).to_owned())
            .collect();
        named.insert(*target, (target - nearest, names));
    }
    named
}

/// The names to keep from the new build's symbols.
pub fn wanted_names(named: &BTreeMap<u32, (u32, Vec<String>)>) -> HashSet<String> {
    named
        .values()
        .flat_map(|(_, names)| names.iter().cloned())
        .collect()
}

/// Each place an address may have moved to, with how many of its names say so, most votes first.
fn candidates(
    names: &[String],
    distance: u32,
    new: &HashMap<String, Vec<u32>>,
) -> Vec<(u32, usize)> {
    let mut votes: BTreeMap<u32, usize> = BTreeMap::new();
    for name in names {
        for address in new.get(name).map_or(&[][..], Vec::as_slice) {
            *votes.entry(address + distance).or_default() += 1;
        }
    }
    let mut votes: Vec<_> = votes.into_iter().collect();
    votes.sort_by_key(|(address, count)| (std::cmp::Reverse(*count), *address));
    votes
}

/// A function's instructions, read as far as this needs them.
pub mod x64 {
    /// One decoded instruction: its length, where its RIP-relative displacement or relative branch
    /// target is, where a displacement from a register is, as in `[rcx + 0x558]`, and where its
    /// immediate value is. Each place is `(offset, size)` within the instruction.
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct Instruction {
        pub length: usize,
        pub relative: Option<(usize, usize)>,
        pub displacement: Option<(usize, usize)>,
        pub immediate: Option<(usize, usize)>,
    }

    impl Instruction {
        fn plain(length: usize) -> Self {
            Self {
                length,
                relative: None,
                displacement: None,
                immediate: None,
            }
        }

        /// An instruction whose last `size` bytes, from `at`, are an immediate value.
        fn with_immediate(at: usize, size: usize) -> Self {
            Self {
                length: at + size,
                relative: None,
                displacement: None,
                immediate: Some((at, size)),
            }
        }

        /// A branch whose last `size` bytes, from `at`, are the distance to its target.
        fn branch(at: usize, size: usize) -> Self {
            Self {
                length: at + size,
                relative: Some((at, size)),
                displacement: None,
                immediate: None,
            }
        }
    }

    fn modrm(bytes: &[u8], at: usize, immediate: usize) -> Option<Instruction> {
        let byte = *bytes.get(at)?;
        let (mode, rm) = (byte >> 6, byte & 7);
        let mut length = at + 1;
        let mut relative = None;
        let mut displacement = None;
        if mode != 3 && rm == 4 {
            let sib = *bytes.get(length)?;
            length += 1;
            if mode == 0 && sib & 7 == 5 {
                displacement = Some((length, 4));
                length += 4;
            }
        }
        match mode {
            0 if rm == 5 => {
                relative = Some((length, 4));
                length += 4;
            }
            1 => {
                displacement = Some((length, 1));
                length += 1;
            }
            2 => {
                displacement = Some((length, 4));
                length += 4;
            }
            _ => {}
        }
        Some(Instruction {
            length: length + immediate,
            relative,
            displacement,
            immediate: (immediate > 0).then_some((length, immediate)),
        })
    }

    /// Decodes the instruction at the start of `bytes`, or `None` for one this does not know. The
    /// list covers what function starts are made of, and anything else goes to a person.
    pub fn decode(bytes: &[u8]) -> Option<Instruction> {
        decode_with(bytes, false)
    }

    /// Decodes as `decode` does, and also the instructions met inside a function body: lock and
    /// segment prefixes, string and x87 instructions, VEX encoded vector instructions and more of
    /// the two and three byte opcodes. It reads bodies to follow calls and operands, and never
    /// judges a guard, so what a hook may copy stays as `decode` says.
    pub fn decode_body(bytes: &[u8]) -> Option<Instruction> {
        decode_with(bytes, true)
    }

    fn decode_with(bytes: &[u8], body: bool) -> Option<Instruction> {
        let mut at = 0;
        let mut wide_operand = false;
        let mut rex_w = false;
        loop {
            match *bytes.get(at)? {
                0x66 => wide_operand = true,
                0xF2 | 0xF3 => {}
                0xF0 | 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 if body => {}
                _ => break,
            }
            at += 1;
        }
        if bytes.get(at)? & 0xF0 == 0x40 {
            rex_w = bytes[at] & 8 != 0;
            at += 1;
        }
        let opcode = *bytes.get(at)?;
        at += 1;
        let word = if wide_operand { 2 } else { 4 };
        match opcode {
            0x50..=0x5F | 0x90 | 0xC3 | 0xCC | 0x99 => Some(Instruction::plain(at)),
            0x98 => Some(Instruction::plain(at)),
            0x00..=0x03
            | 0x08..=0x0B
            | 0x10..=0x13
            | 0x18..=0x1B
            | 0x20..=0x23
            | 0x28..=0x2B
            | 0x30..=0x33
            | 0x38..=0x3B
            | 0x63
            | 0x84..=0x8B
            | 0x8D
            | 0x8F
            | 0xD0..=0xD3
            | 0xFE
            | 0xFF => modrm(bytes, at, 0),
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C | 0x6A | 0xA8 | 0xB0..=0xB7 => {
                Some(Instruction::with_immediate(at, 1))
            }
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D | 0x68 | 0xA9 => {
                Some(Instruction::with_immediate(at, word))
            }
            0xB8..=0xBF => Some(Instruction::with_immediate(
                at,
                if rex_w { 8 } else { word },
            )),
            0x80 | 0x83 | 0xC0 | 0xC1 | 0xC6 | 0x6B => modrm(bytes, at, 1),
            0x81 | 0xC7 | 0x69 => modrm(bytes, at, word),
            0xF6 => {
                let immediate = usize::from((bytes.get(at)? >> 3) & 7 < 2);
                modrm(bytes, at, immediate)
            }
            0xF7 => {
                let immediate = if (bytes.get(at)? >> 3) & 7 < 2 {
                    word
                } else {
                    0
                };
                modrm(bytes, at, immediate)
            }
            0x70..=0x7F | 0xEB => Some(Instruction::branch(at, 1)),
            0xE8 | 0xE9 => Some(Instruction::branch(at, 4)),
            0x0F => {
                let second = *bytes.get(at)?;
                at += 1;
                match second {
                    0x80..=0x8F => Some(Instruction::branch(at, 4)),
                    0x10..=0x17
                    | 0x1E
                    | 0x1F
                    | 0x28..=0x2F
                    | 0x40..=0x6F
                    | 0x7E
                    | 0x7F
                    | 0x90..=0x9F
                    | 0xAF
                    | 0xB6
                    | 0xB7
                    | 0xBE
                    | 0xBF
                    | 0xD6
                    | 0xEF => modrm(bytes, at, 0),
                    _ if body => body_two_byte(bytes, at, second),
                    _ => None,
                }
            }
            _ if body => body_one_byte(bytes, at, opcode),
            _ => None,
        }
    }

    /// One byte opcodes that only a function body holds. `at` is just past the opcode.
    fn body_one_byte(bytes: &[u8], at: usize, opcode: u8) -> Option<Instruction> {
        match opcode {
            0x91..=0x97 | 0x9C..=0x9F | 0xA4..=0xA7 | 0xAA..=0xAF | 0xC9 | 0xF4 | 0xF5 => {
                Some(Instruction::plain(at))
            }
            0xF8..=0xFD => Some(Instruction::plain(at)),
            0xC2 => Some(Instruction::with_immediate(at, 2)),
            0xCD => Some(Instruction::with_immediate(at, 1)),
            0xD8..=0xDF => modrm(bytes, at, 0),
            0xE0..=0xE3 => Some(Instruction::branch(at, 1)),
            // Two byte VEX: one payload byte, then an opcode of the 0F map.
            0xC5 => {
                let opcode = *bytes.get(at + 1)?;
                modrm(bytes, at + 2, vex_immediate(1, opcode))
            }
            // Three byte VEX: the map in the first payload byte, then one more payload byte.
            0xC4 => {
                let map = bytes.get(at)? & 0x1F;
                let opcode = *bytes.get(at + 2)?;
                modrm(bytes, at + 3, vex_immediate(map, opcode))
            }
            _ => None,
        }
    }

    /// Whether a VEX instruction of `map` ends in an immediate byte.
    fn vex_immediate(map: u8, opcode: u8) -> usize {
        match map {
            3 => 1,
            1 => usize::from(matches!(opcode, 0x70..=0x73 | 0xC2 | 0xC4..=0xC6)),
            _ => 0,
        }
    }

    /// Two byte opcodes that only a function body holds. `at` is just past the second byte.
    fn body_two_byte(bytes: &[u8], at: usize, second: u8) -> Option<Instruction> {
        match second {
            0x0B | 0x31 | 0x77 | 0xA2 | 0xC8..=0xCF => Some(Instruction::plain(at)),
            0x0D
            | 0x18..=0x1D
            | 0x74..=0x76
            | 0x7C
            | 0x7D
            | 0xA3
            | 0xA5
            | 0xAB
            | 0xAD
            | 0xAE
            | 0xB0
            | 0xB1
            | 0xB3
            | 0xB8
            | 0xBB
            | 0xBC
            | 0xBD
            | 0xC0
            | 0xC1
            | 0xC3
            | 0xC7
            | 0xD0..=0xD5
            | 0xD7..=0xEE
            | 0xF0..=0xFE => modrm(bytes, at, 0),
            0x70..=0x73 | 0xA4 | 0xAC | 0xBA | 0xC2 | 0xC4..=0xC6 => modrm(bytes, at, 1),
            0x38 => modrm(bytes, at + 1, 0),
            0x3A => modrm(bytes, at + 1, 1),
            _ => None,
        }
    }

    /// The instruction boundaries within `bytes`, and whether all of them could be read up to `length`.
    pub fn boundaries(bytes: &[u8], length: usize) -> (Vec<usize>, bool) {
        let mut at = 0;
        let mut seen = vec![];
        while at < length {
            match decode(&bytes[at..]) {
                Some(instruction) => {
                    at += instruction.length;
                    seen.push(at);
                }
                None => return (seen, false),
            }
        }
        (seen, true)
    }

    /// The bytes with every relative value blanked, which is what stays the same when only the
    /// distance to other code moved. `None` when an instruction cannot be read.
    pub fn shape(bytes: &[u8]) -> Option<Vec<u8>> {
        let mut out = bytes.to_vec();
        let mut at = 0;
        while at < bytes.len() {
            let instruction = decode_tail(&bytes[at..])?;
            if let Some((offset, size)) = instruction.relative {
                for index in at + offset..(at + offset + size).min(out.len()) {
                    out[index] = 0;
                }
            }
            at += instruction.length;
        }
        Some(out)
    }

    /// Decodes an instruction that may be cut off by the end of a recorded prologue.
    fn decode_tail(bytes: &[u8]) -> Option<Instruction> {
        let mut padded = bytes.to_vec();
        padded.resize(bytes.len() + 16, 0);
        decode(&padded)
    }

    /// Whether any instruction in the bytes depends on where it sits.
    pub fn has_relative(bytes: &[u8]) -> Option<bool> {
        let mut at = 0;
        while at < bytes.len() {
            let instruction = decode_tail(&bytes[at..])?;
            if instruction.relative.is_some() {
                return Some(true);
            }
            at += instruction.length;
        }
        Some(false)
    }

    /// An instruction's bytes with its relative value, displacement and immediate cleared: what
    /// stays the same when code moves and members, slots and constants change.
    pub fn skeleton(bytes: &[u8], instruction: &Instruction) -> Vec<u8> {
        let mut out = bytes[..instruction.length.min(bytes.len())].to_vec();
        let fields = [
            instruction.relative,
            instruction.displacement,
            instruction.immediate,
        ];
        for (offset, size) in fields.into_iter().flatten() {
            for index in offset..(offset + size).min(out.len()) {
                out[index] = 0;
            }
        }
        out
    }
}

/// A byte field with no address field of the same name, and where it was read from.
fn byte_field_address(key: &str) -> Option<(&'static str, u64)> {
    Some(match key {
        "region_minimum_prologue" => ("region_minimum_size", 0),
        "bounds_bytes" => ("bounds_in_screen", 0),
        "page_info_bytes" => ("page_info_path", 0),
        "hwnd_for_widget_head" => ("hwnd_for_widget", 0),
        "hwnd_for_widget_tail" => ("hwnd_for_widget", 9),
        _ => return None,
    })
}

/// Fields of bytes that are not read from the module at one address: they name things.
const NOT_CODE: &[&str] = &["http_icon_name", "nav_icon_names"];

/// Data, rather than instructions, so its shape is not read as code.
const DATA_BYTES: &[&str] = &["page_info_bytes"];

/// The value at a path such as `toolbar.paint_slot`.
fn at_path<'a>(row: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(row, |value, key| &value[key])
}

/// `(vtable, slot, function)`: a function the runtime replaces in a table, and so a check the new
/// module can answer on its own. A function listed twice settles a folded name.
const VTABLE_SLOTS: &[(&str, &str, &str)] = &[
    (
        "toolbar.browser_widget_vtable",
        "toolbar.paint_slot",
        "toolbar.paint",
    ),
    (
        "toolbar.browser_widget_vtable",
        "toolbar.layout_slot",
        "toolbar.layout",
    ),
    (
        "toolbar.plain_widget_vtable",
        "toolbar.paint_slot",
        "toolbar.paint",
    ),
    (
        "toolbar.plain_widget_vtable",
        "toolbar.layout_slot",
        "toolbar.layout",
    ),
    (
        "toolbar.browser_delegate_vtable",
        "toolbar.active_web_contents_slot",
        "toolbar.active_web_contents",
    ),
    (
        "toolbar.web_contents_vtable",
        "toolbar.web_contents_title_slot",
        "toolbar.web_contents_title",
    ),
    (
        "side_panel.layout_vtable",
        "side_panel.proposed_slot",
        "side_panel.proposed",
    ),
    (
        "toolbar.infobar_vtable",
        "toolbar.infobar_layout_slot",
        "toolbar.infobar_layout",
    ),
    (
        "layout.frame_vtable",
        "layout.on_paint_slot",
        "layout.on_paint",
    ),
    (
        "layout.frame_vtable",
        "layout.top_inset_slot",
        "layout.top_inset",
    ),
    ("layout.frame_vtable", "layout.layout_slot", "layout.layout"),
    (
        "layout.caption_vtable",
        "layout.caption_size_slot",
        "layout.caption_size",
    ),
    (
        "layout.caption_vtable",
        "layout.caption_paint_slot",
        "layout.caption_paint",
    ),
    (
        "location_bar.vtable",
        "location_bar.slot",
        "location_bar.function",
    ),
    (
        "location_bar.vtable",
        "location_bar.preferred_slot",
        "location_bar.preferred",
    ),
];

fn usize_at(row: &Value, path: &str) -> Option<u64> {
    read_address(at_path(row, path))
}

/// Everything the new build supplies.
pub struct NewBuild<'a> {
    pub version: &'a str,
    pub pe: &'a PeFile<'a>,
    pub symbols: &'a HashMap<String, Vec<u32>>,
    /// Where each address with no public name went, found through private symbols.
    pub private: &'a BTreeMap<u32, u32>,
}

/// Settles an address the names disagree on. Folded functions share names across builds, so the
/// names alone can point at a function the new build no longer folds with the wanted one. The
/// instructions that row checks for it say which of the places is meant, and a table slot that
/// must hold it says the same.
fn settle(target: u32, choices: &[(u32, usize)], build: &Value, pe: &PeFile) -> Option<u32> {
    let mut remaining: Vec<u32> = choices.iter().map(|(address, _)| *address).collect();
    let mut guards = vec![];
    collect_guards(build, &mut guards);
    for (address, bytes) in guards {
        if address != target {
            continue;
        }
        let wanted = x64::shape(&bytes)?;
        remaining.retain(|place| {
            pe.read(*place, bytes.len())
                .and_then(x64::shape)
                .is_some_and(|shape| shape == wanted)
        });
    }
    if remaining.len() == 1 {
        return Some(remaining[0]);
    }
    // The linker folds functions with the same code under one name, and a new build can fold them
    // into two groups. Short functions read to their end show that any of the places does the same.
    let code: Vec<_> = remaining
        .iter()
        .map(|place| whole_function(pe, *place))
        .collect();
    let first = code.first()?.as_ref()?;
    code.iter()
        .all(|other| other.as_ref() == Some(first))
        .then(|| remaining[0])
}

/// The bytes of a function short enough to read to its end: its first `ret` with no branch before.
pub(crate) fn whole_function(pe: &PeFile, address: u32) -> Option<Vec<u8>> {
    let bytes = pe.read(address, 48)?;
    let mut at = 0;
    while at < 32 {
        let instruction = x64::decode(&bytes[at..])?;
        if instruction.relative.is_some() && bytes[at] != 0x48 && bytes[at] != 0x8B {
            return None;
        }
        at += instruction.length;
        if bytes[at - 1] == 0xC3 {
            return x64::shape(&bytes[..at]);
        }
    }
    None
}

/// Every `(address, bytes)` guard of a row.
fn collect_guards(value: &Value, guards: &mut Vec<(u32, Vec<u8>)>) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                if let Some(bytes) = item.as_str().filter(|_| is_byte_field(key)).and_then(hex) {
                    let base = byte_field_address(key)
                        .map(|(base, _)| base.to_owned())
                        .unwrap_or_else(|| {
                            key.trim_end_matches("_prologue")
                                .trim_end_matches("_bytes")
                                .to_owned()
                        });
                    if let Some(address) = object
                        .get(&base)
                        .and_then(read_address)
                        .filter(|value| is_address(*value))
                    {
                        guards.push((address as u32, bytes));
                    }
                }
                collect_guards(item, guards);
            }
        }
        Value::Array(items) => {
            if let [address, Value::String(bytes)] = items.as_slice() {
                if let (Some(address), Some(bytes)) = (read_address(address), hex(bytes)) {
                    guards.push((address as u32, bytes));
                    return;
                }
            }
            items.iter().for_each(|item| collect_guards(item, guards));
        }
        _ => {}
    }
}

fn is_byte_field(key: &str) -> bool {
    key.ends_with("_prologue") || key.ends_with("_bytes") || byte_field_address(key).is_some()
}

/// The bytes a hook overwrites with its jump.
const COPIED_AT_LEAST: usize = 12;

/// What the runtime may do with a guard, judged from the prior row, which a person verified.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GuardKind {
    /// Long enough for a hook, ending on an instruction, and moving nothing that depends on where
    /// it sits apart from a last stack cookie load, which the hook relocates. The hook runs a copy
    /// of these bytes, so a new function start must have the same properties.
    Copyable,
    /// Holds a position dependent instruction before its end, and a row may count bytes to it.
    Fixed,
    /// Too short or cut inside an instruction: it is only ever compared.
    Compared,
}

pub fn guard_kind(bytes: &[u8]) -> GuardKind {
    let mut padded = bytes.to_vec();
    padded.resize(bytes.len() + 16, 0);
    let mut at = 0;
    let mut relative_before_end = false;
    while at < bytes.len() {
        let Some(instruction) = x64::decode(&padded[at..]) else {
            return GuardKind::Compared;
        };
        if let Some((offset, size)) = instruction.relative {
            let inside = at + offset + size <= bytes.len();
            let is_last_cookie = at + 7 == bytes.len() && bytes[at..at + 3] == [0x48, 0x8B, 0x05];
            relative_before_end |= inside && !is_last_cookie;
            if !inside {
                return GuardKind::Compared;
            }
        }
        at += instruction.length;
    }
    match (
        relative_before_end,
        at == bytes.len() && bytes.len() >= COPIED_AT_LEAST,
    ) {
        (true, _) => GuardKind::Fixed,
        (false, true) => GuardKind::Copyable,
        (false, false) => GuardKind::Compared,
    }
}

/// Reads a guard again from the new module. A hook runs a copy of what it overwrites, so a guard
/// that is copied must stay cut on an instruction, or the copy would crash the browser.
fn read_guard(
    label: &str,
    address: u64,
    old: &[u8],
    is_code: bool,
    pe: &PeFile,
    report: &mut PortReport,
) -> Option<Vec<u8>> {
    let Ok(address) = u32::try_from(address) else {
        report
            .review
            .push(format!("{label}: address {address:#x} out of range"));
        return None;
    };
    let Some(mut new) = pe.read(address, old.len()).map(<[u8]>::to_vec) else {
        report
            .review
            .push(format!("{label}: {address:#x} lies outside the module"));
        return None;
    };
    if is_code {
        match guard_kind(old) {
            GuardKind::Compared => {}
            GuardKind::Fixed => {
                if x64::shape(old) != x64::shape(&new) {
                    report.review.push(format!(
                        "{label}: a row counts bytes into this function start, which changed: {} was {}",
                        hex_text(&new),
                        hex_text(old)
                    ));
                }
            }
            GuardKind::Copyable => {
                let window = pe.read(address, old.len() + 16).unwrap_or(&new);
                let (seen, complete) = x64::boundaries(window, old.len());
                if complete && !seen.contains(&old.len()) {
                    // The jump needs twelve bytes, so a shorter guard grows to the next instruction.
                    let keep = seen
                        .iter()
                        .rev()
                        .find(|end| **end < old.len() && **end >= COPIED_AT_LEAST)
                        .copied();
                    let length = keep.unwrap_or_else(|| {
                        *seen
                            .iter()
                            .find(|end| **end > old.len())
                            .unwrap_or(&old.len())
                    });
                    new = pe.read(address, length).map_or(new, <[u8]>::to_vec);
                }
                if guard_kind(&new) != GuardKind::Copyable {
                    report.review.push(format!(
                        "{label}: a hook could not copy this function start: {} was {}",
                        hex_text(&new),
                        hex_text(old)
                    ));
                }
            }
        }
    }
    if new != old {
        report
            .bytes_changed
            .push(format!("{label}: {} -> {}", hex_text(old), hex_text(&new)));
    }
    Some(new)
}

fn hex_text(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reads every guard of a row object again from the new module.
fn refresh_guards(path: &str, value: &mut Value, pe: &PeFile, report: &mut PortReport) {
    match value {
        Value::Object(object) => {
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                let label = format!("{path}.{key}");
                let is_field = is_byte_field(&key) && !NOT_CODE.contains(&key.as_str());
                let old = object[&key].as_str().filter(|_| is_field).and_then(hex);
                if let Some(old) = old.filter(|old| !old.is_empty() && old.as_slice() != [0]) {
                    let (base, distance) = match byte_field_address(&key) {
                        Some((base, distance)) => (base.to_owned(), distance),
                        None => (
                            key.trim_end_matches("_prologue")
                                .trim_end_matches("_bytes")
                                .to_owned(),
                            0,
                        ),
                    };
                    let Some(address) = object.get(&base).and_then(read_address) else {
                        report
                            .review
                            .push(format!("{label}: no address field {base}"));
                        continue;
                    };
                    if !is_address(address) {
                        continue;
                    }
                    let is_code = !DATA_BYTES.contains(&key.as_str());
                    if let Some(new) =
                        read_guard(&label, address + distance, &old, is_code, pe, report)
                    {
                        object.insert(key, Value::String(hex_text(&new)));
                    }
                    continue;
                }
                refresh_guards(
                    &label,
                    object.get_mut(&key).expect("listed key"),
                    pe,
                    report,
                );
            }
        }
        Value::Array(items) => {
            if let [address, Value::String(bytes)] = items.as_mut_slice() {
                if let (Some(address), Some(old)) = (read_address(address), hex(bytes)) {
                    if is_address(address) {
                        if let Some(new) = read_guard(path, address, &old, true, pe, report) {
                            *bytes = hex_text(&new);
                        }
                    }
                    return;
                }
            }
            for (index, item) in items.iter_mut().enumerate() {
                refresh_guards(&format!("{path}[{index}]"), item, pe, report);
            }
        }
        _ => {}
    }
}

/// Checks the rule that a table slot holds its function, moving a slot that moved. The other fields
/// of a class can move too, and nothing in the module tells, so a moved slot goes to review.
fn check_vtable_slots(build: &mut Value, pe: &PeFile, report: &mut PortReport) {
    for (vtable_path, slot_path, function_path) in VTABLE_SLOTS {
        let (Some(vtable), Some(function)) =
            (usize_at(build, vtable_path), usize_at(build, function_path))
        else {
            continue;
        };
        let Some(slot) = usize_at(build, slot_path) else {
            continue;
        };
        if !is_address(vtable) || !is_address(function) {
            continue;
        }
        if pe.pointer_rva(vtable as u32 + slot as u32 * 8) == Some(function as u32) {
            continue;
        }
        match pe.vtable_slot(vtable as u32, function as u32) {
            Some(moved) => {
                report
                    .slots_moved
                    .push(format!("{slot_path}: {slot} -> {moved}"));
                report
                    .review
                    .push(format!("{slot_path} moved from {slot} to {moved}"));
                let (parent, key) = slot_path.split_once('.').expect("a path has a parent");
                build[parent][key] = write_address(u64::from(moved));
            }
            None => report.review.push(format!(
                "{vtable_path} holds no {function_path} {function:#x}"
            )),
        }
    }
}

/// `ContentsContainerView::SetRoundedCorners` starts `movss; ucomiss; jne changed; jp parity`. The
/// row records where both go, so they are read from the instructions rather than moved by distance.
fn read_rounded_corners(build: &mut Value, pe: &PeFile, report: &mut PortReport) {
    // A row that keeps no destinations does not hook this function.
    if !usize_at(build, "split.set_rounded_corners_changed").is_some_and(is_address) {
        return;
    }
    let Some(start) =
        usize_at(build, "split.set_rounded_corners").filter(|value| is_address(*value))
    else {
        return;
    };
    let Some(bytes) = pe.read(start as u32, 13) else {
        return;
    };
    if bytes[..4] != [0xF3, 0x0F, 0x10, 0x02] || bytes[4..6] != [0x0F, 0x2E] || bytes[11] != 0x75 {
        report.review.push(format!(
            "split.set_rounded_corners starts differently: {}",
            hex_text(bytes)
        ));
        return;
    }
    let changed = start + 13 + u64::from(bytes[12]);
    build["split"]["set_rounded_corners_changed"] = write_address(changed);
    build["split"]["set_rounded_corners_parity"] = write_address(start + 13);
}

/// Addresses read from instructions above, which no symbol names.
const READ_FROM_CODE: &[&str] = &[
    "split.set_rounded_corners_changed",
    "split.set_rounded_corners_parity",
];

fn read_from_code_targets(build: &Value) -> Vec<u32> {
    READ_FROM_CODE
        .iter()
        .filter_map(|path| usize_at(build, path))
        .map(|value| value as u32)
        .collect()
}

/// The code addresses a row holds that no public symbol starts at, which need private symbols.
pub fn unnamed_code(
    prior: &Value,
    named: &BTreeMap<u32, (u32, Vec<String>)>,
    pe_of_new: &PeFile,
) -> Vec<u32> {
    let read = read_from_code_targets(&prior["build"]);
    let mut out = vec![];
    for (target, (distance, _)) in named {
        // The prior module is gone by now. Code and data keep their sections from build to build.
        if *distance != 0 && !read.contains(target) && pe_of_new.is_code(*target) {
            out.push(*target);
        }
    }
    out
}

/// Makes the new row. `prior` is `{frame, build}`, as the runtime writes it.
pub fn port_row(
    prior: &Value,
    named: &BTreeMap<u32, (u32, Vec<String>)>,
    new: &NewBuild,
) -> (Value, PortReport) {
    let (row, mut report) = begin_row(prior, new.version, true);
    let mut moved: BTreeMap<u32, u32> = BTreeMap::new();
    let targets = addresses_of(&row);
    report.addresses = targets.len();
    port_by_name(prior, &row, named, new, &targets, &mut moved, &mut report);
    finish_row(row, &moved, new.pe, report)
}

/// A copy of the prior row with the new version, and frame addresses in the `0x` form. `carries`
/// says the values with no recipe stay as the prior row has them, so a new milestone stops the
/// row; without it they are read from the new build later, each with its own review line.
fn begin_row(prior: &Value, version: &str, carries: bool) -> (Value, PortReport) {
    let mut row = prior.clone();
    if let Some(object) = row.as_object_mut() {
        // The mark tells how the prior row was made, not this one.
        object.remove(WITHOUT_PDB);
    }
    let prior_version = prior["frame"]["version"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let mut report = PortReport {
        browser: prior["build"]["browser"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        version: version.to_owned(),
        prior_version: prior_version.clone(),
        ..PortReport::default()
    };
    let parts: Vec<u16> = version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    if parts.len() != 4 {
        report
            .review
            .push(format!("{} is not a four part version", version));
    }
    report.carried = crate::row_recipe::without_recipe(&prior["build"]);
    report.settings = crate::row_recipe::settings(&prior["build"]);
    if carries && prior_version.split('.').next() != version.split('.').next() {
        report.review.push(format!(
            "the milestone changed from {prior_version} to {version}: {} values have no recipe and were carried over unchecked: {}",
            report.carried.len(),
            report.carried.join(", ")
        ));
    }
    row["build"]["version"] = json!(parts);
    row["frame"]["version"] = json!(version);

    let frame_as_addresses = frame_addresses(&row["frame"]);
    row["frame_addresses"] = frame_as_addresses;
    (row, report)
}

/// Finds each address of a row through the public names of both builds.
fn port_by_name(
    prior: &Value,
    row: &Value,
    named: &BTreeMap<u32, (u32, Vec<String>)>,
    new: &NewBuild,
    targets: &BTreeSet<u32>,
    moved: &mut BTreeMap<u32, u32>,
    report: &mut PortReport,
) {
    for target in targets {
        let Some((distance, names)) = named.get(target) else {
            continue;
        };
        // No public name covers this function: the distance from the nearest one means nothing,
        // since another build lays those functions out differently.
        if *distance != 0
            && new.pe.is_code(*target)
            && !read_from_code_targets(&prior["build"]).contains(target)
        {
            match new.private.get(target) {
                Some(place) => {
                    moved.insert(*target, *place);
                    report.private.push(format!("{target:#x} -> {place:#x}"));
                }
                None => report.review.push(format!(
                    "{target:#x} lies in a function with no name in both builds"
                )),
            }
            continue;
        }
        let choices = candidates(names, *distance, new.symbols);
        let place = match choices.as_slice() {
            [] => None,
            [(only, _)] => Some(*only),
            many => {
                let settled = settle(*target, many, &row["build"], new.pe);
                let place = settled
                    .or_else(|| by_slot(*target, many, &row["build"], new.pe, named, new.symbols));
                match place {
                    Some(place) => report
                        .settled
                        .push(format!("{target:#x} -> {place:#x} of {many:x?}")),
                    None => {
                        // Most folded names still fold together, so the largest group is the likely
                        // place, and a person looks at it.
                        report.review.push(format!(
                            "{target:#x} has several places: {many:x?}, first of {:?}",
                            &names[..names.len().min(3)]
                        ));
                    }
                }
                place.or(Some(many[0].0))
            }
        };
        if let Some(place) = place {
            if *distance != 0 {
                report.interior.push(format!(
                    "{target:#x} = {}+{distance:#x} -> {place:#x}",
                    names[0]
                ));
            }
            moved.insert(*target, place);
        }
    }
}

/// Writes the new places into a row and reads every guard and table slot back from the new module.
fn finish_row(
    mut row: Value,
    moved: &BTreeMap<u32, u32>,
    pe: &PeFile,
    mut report: PortReport,
) -> (Value, PortReport) {
    report.mapped = moved.len();

    let frame_values = row
        .as_object_mut()
        .and_then(|object| object.remove("frame_addresses"))
        .unwrap_or_default();
    let mut unmapped = BTreeSet::new();
    let mut rewrite = |path: &str, value: &mut Value| {
        if let Some(address) = read_address(value).filter(|value| is_address(*value)) {
            if READ_FROM_CODE.iter().any(|field| path.ends_with(field)) {
                return;
            }
            match moved.get(&(address as u32)) {
                Some(place) => *value = write_address(u64::from(*place)),
                None => {
                    unmapped.insert(format!("{path} {address:#x}"));
                }
            }
        }
    };
    walk_addresses("", &mut row["build"], &mut rewrite);
    let mut frame_values = frame_values;
    walk_addresses(".frame", &mut frame_values, &mut rewrite);
    report.review.extend(
        unmapped
            .into_iter()
            .map(|field| format!("no new place for {field}")),
    );
    apply_frame_addresses(&mut row["frame"], &frame_values);

    // Everything read back from the new module.
    read_rounded_corners(&mut row["build"], pe, &mut report);
    refresh_guards("build", &mut row["build"], pe, &mut report);
    refresh_frame_guard(&mut row["frame"], pe, &mut report);
    check_vtable_slots(&mut row["build"], pe, &mut report);
    check_frame_slot(&mut row["frame"], pe, &mut report);
    // Last, since the recipes read the addresses of the new build.
    let derived = crate::row_recipe::derive_row(&mut row["build"], pe);
    report.derived.extend(derived.read);
    report.review.extend(
        derived
            .failed
            .into_iter()
            .map(|reason| format!("a value could not be read from the new build: {reason}")),
    );
    (row, report)
}

/// How many bytes of a function a fingerprint holds at most.
const FINGERPRINT_BYTES: usize = 32;
/// A shorter fingerprint matches too many places to mean anything.
const FINGERPRINT_LEAST: usize = 16;
/// The first bytes of a fingerprint, which are searched for, must be plain instruction bytes.
const ANCHOR: usize = 4;
/// How far into a class table a fingerprint looks for a slot it can name.
const SLOTS_SEEN: u32 = 0x80;

/// The addresses of a row that are found again in a new build. Those read from code are skipped.
fn fingerprint_targets(row: &Value) -> BTreeSet<u32> {
    let mut row = row.clone();
    row["frame_addresses"] = frame_addresses(&row["frame"]);
    let mut found = BTreeSet::new();
    let mut collect = |path: &str, value: &mut Value| {
        if READ_FROM_CODE.iter().any(|field| path.ends_with(field)) {
            return;
        }
        if let Some(address) = read_address(value).filter(|value| is_address(*value)) {
            found.insert(address as u32);
        }
    };
    walk_addresses("", &mut row["build"], &mut collect);
    walk_addresses(".frame", &mut row["frame_addresses"], &mut collect);
    found
}

/// The start of a function with its displacements cleared, cut at an instruction end.
pub(crate) fn code_print(pe: &PeFile, address: u32) -> Option<Vec<u8>> {
    if !pe.is_code(address) {
        return None;
    }
    let window = pe.read(address, FINGERPRINT_BYTES + 16)?;
    // Zeros decode as `add [rax], al` and padding as `int3`: the start of a class table or of the
    // gap between functions, never of a function.
    if window.starts_with(&[0x00, 0x00]) || window.first() == Some(&0xCC) {
        return None;
    }
    let (ends, _) = x64::boundaries(window, FINGERPRINT_BYTES);
    let length = ends
        .iter()
        .rev()
        .find(|end| **end <= FINGERPRINT_BYTES)
        .copied()?;
    if length < FINGERPRINT_LEAST {
        return None;
    }
    let bytes = &window[..length];
    let shape = x64::shape(bytes)?;
    // The anchor is searched for byte by byte, so it may not hold a displacement.
    (shape[..ANCHOR] == bytes[..ANCHOR]).then_some(shape)
}

/// Records, in the row, how the runtime can find each of its addresses in a build with no row.
/// A function is known by its first instructions without their displacements. A class table is
/// known by a slot that holds one of those functions. An address that is neither cannot be found,
/// so then the row gets no fingerprints at all.
pub fn add_fingerprints(row: &mut Value, pe: &PeFile) -> Result<(), String> {
    let (prints, missing) = fingerprints_of(row, pe);
    if !missing.is_empty() {
        return Err(format!("no fingerprint for {}", missing.join(", ")));
    }
    row["fingerprints"] = Value::Object(prints);
    Ok(())
}

/// The fingerprint of each address of a row that has one, read from the row's own module, and the
/// addresses that have none. A row made from them leaves those addresses unvouched.
pub fn fingerprints_of(row: &Value, pe: &PeFile) -> (serde_json::Map<String, Value>, Vec<String>) {
    let targets = fingerprint_targets(row);
    let mut prints = serde_json::Map::new();
    let mut missing = Vec::new();
    let mut functions = BTreeMap::new();
    for target in &targets {
        if let Some(shape) = code_print(pe, *target) {
            functions.insert(*target, hex_text(&shape));
        }
    }
    for target in &targets {
        let key = format!("{target:#x}");
        if let Some(shape) = functions.get(target) {
            prints.insert(key, Value::String(shape.clone()));
            continue;
        }
        let slot = (0..SLOTS_SEEN).find_map(|slot| {
            let holds = pe.pointer_rva(target.checked_add(slot * 8)?)?;
            functions.contains_key(&holds).then_some((slot, holds))
        });
        match slot {
            Some((slot, holds)) => {
                prints.insert(key, json!({"slot": slot, "holds": format!("{holds:#x}")}));
            }
            None => missing.push(key),
        }
    }
    (prints, missing)
}

/// Every place each function fingerprint matches, in one pass over the code.
pub(crate) fn find_prints(pe: &PeFile, prints: &BTreeMap<u32, Vec<u8>>) -> BTreeMap<u32, Vec<u32>> {
    let mut by_anchor: HashMap<[u8; ANCHOR], Vec<u32>> = HashMap::new();
    let mut first = [false; 256];
    for (target, shape) in prints {
        let anchor: [u8; ANCHOR] = shape[..ANCHOR].try_into().expect("checked length");
        first[anchor[0] as usize] = true;
        by_anchor.entry(anchor).or_default().push(*target);
    }
    let mut found: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (base, bytes, is_code) in pe.sections() {
        if !is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(ANCHOR) {
            if !first[bytes[at] as usize] {
                continue;
            }
            let anchor: [u8; ANCHOR] = bytes[at..at + ANCHOR].try_into().expect("in range");
            let Some(targets) = by_anchor.get(&anchor) else {
                continue;
            };
            for target in targets {
                let shape = &prints[target];
                let Some(candidate) = bytes.get(at..at + shape.len()) else {
                    continue;
                };
                if x64::shape(candidate).as_deref() == Some(shape.as_slice()) {
                    found.entry(*target).or_default().push(base + at as u32);
                }
            }
        }
    }
    found
}

/// Every class table whose given slot holds a function, by a pointer search over the module.
pub(crate) fn find_table(pe: &PeFile, slot: u32, function: u32) -> Vec<u32> {
    let value = (pe.image_base() + u64::from(function)).to_le_bytes();
    let mut found = Vec::new();
    for (base, bytes, _) in pe.sections() {
        for at in (0..bytes.len().saturating_sub(7)).step_by(8) {
            if bytes[at..at + 8] == value {
                if let Some(table) = (base + at as u32).checked_sub(slot * 8) {
                    found.push(table);
                }
            }
        }
    }
    found
}

/// Makes the row of a build that has no row from the fingerprints of a verified row of the same
/// milestone, without symbols. Each address must match exactly one place, no two addresses may
/// share one, and every function start and table slot is then checked again in the new module the
/// way a ported row is. Any doubt is an error with its reasons, and then the runtime changes
/// nothing in the browser.
pub fn find_row(prior: &Value, version: &str, pe: &PeFile) -> Result<Value, Vec<String>> {
    let Some(stored) = prior["fingerprints"].as_object() else {
        return Err(vec!["the row has no fingerprints".to_owned()]);
    };
    let (row, report) = find_candidate(prior, stored, None, version, pe);
    if !report.review.is_empty() {
        return Err(report.review);
    }
    Ok(row)
}

/// Makes a candidate row for a build whose symbols are not published, from `stored`, the
/// fingerprints of the prior row's addresses in the prior build. An address is taken where its
/// fingerprint matches exactly one place. Offsets, slots and sizes with a recipe are read again
/// from the new module exactly as for a row made from symbols.
///
/// `prior_pe` is the prior row's own module, when the caller has it. A fingerprint taken at a
/// function start its exception table lists then counts only places the new module's exception
/// table also lists, since the same bytes inside another function are not the function. Each
/// address the fingerprints leave open is looked for by `row_resolve`, which takes a place only
/// where two independent methods agree, and each value with no recipe is read from the
/// instructions that use it. Whatever stays open keeps a review line and its prior value, so such
/// a row is never published as it is.
pub fn find_candidate(
    prior: &Value,
    stored: &serde_json::Map<String, Value>,
    prior_pe: Option<&PeFile>,
    version: &str,
    pe: &PeFile,
) -> (Value, PortReport) {
    let (row, mut report) = begin_row(prior, version, prior_pe.is_none());
    report.made_without_pdb = true;
    let prior_starts = prior_pe.map(PeFile::function_starts).unwrap_or_default();
    let targets = fingerprint_targets(prior);
    report.addresses = targets.len();
    let mut functions = BTreeMap::new();
    let mut tables = BTreeMap::new();
    // Each address its fingerprint did not pin down, with its review line, waits for the methods
    // that read the prior build next to the new one.
    let mut wanted: BTreeMap<u32, (String, row_resolve::Wanted)> = BTreeMap::new();
    let want = |kind, candidates: Option<Vec<u32>>| row_resolve::Wanted { kind, candidates };
    for target in &targets {
        match stored.get(&format!("{target:#x}")) {
            Some(Value::String(text)) => match hex(text).filter(|shape| shape.len() >= ANCHOR) {
                Some(shape) => {
                    functions.insert(*target, shape);
                }
                None => report
                    .review
                    .push(format!("{target:#x}: unreadable fingerprint")),
            },
            Some(Value::Object(table)) => {
                let slot = table["slot"]
                    .as_u64()
                    .and_then(|slot| u32::try_from(slot).ok());
                let holds = read_address(&table["holds"]).map(|holds| holds as u32);
                match (slot, holds) {
                    (Some(slot), Some(holds)) => {
                        tables.insert(*target, (slot, holds));
                    }
                    _ => report
                        .review
                        .push(format!("{target:#x}: unreadable table fingerprint")),
                }
            }
            _ => {
                let line = format!("{target:#x}: no fingerprint");
                match prior_pe {
                    Some(prior_pe) => {
                        let kind = if prior_pe.is_code(*target) {
                            row_resolve::Kind::Function
                        } else {
                            row_resolve::Kind::Table
                        };
                        wanted.insert(*target, (line, want(kind, None)));
                    }
                    None => report.review.push(line),
                }
            }
        }
    }
    let found = find_prints(pe, &functions);
    let new_starts = if prior_starts.is_empty() {
        BTreeSet::new()
    } else {
        pe.function_starts()
    };
    let mut moved = BTreeMap::new();
    for target in functions.keys() {
        let all = found.get(target).map_or(&[][..], Vec::as_slice);
        let places: Vec<u32> = all
            .iter()
            .copied()
            .filter(|place| {
                // The linker starts whole functions on 16 bytes, and the exception table also
                // lists the parts it moves out of a function, which start anywhere.
                !prior_starts.contains(target)
                    || (new_starts.contains(place) && (target % 16 != 0 || *place % 16 == 0))
            })
            .collect();
        let function = row_resolve::Kind::Function;
        match places.as_slice() {
            [only] => {
                moved.insert(*target, *only);
                // Says which matches the exception tables checked, for the person reading the report.
                let checked = if prior_starts.contains(target) {
                    " at a function start"
                } else {
                    ""
                };
                report
                    .fingerprinted
                    .push(format!("{target:#x} -> {only:#x}{checked}"));
            }
            [] if !all.is_empty() => {
                let line = format!(
                    "{target:#x} matches no function start, of {} places",
                    all.len()
                );
                wanted.insert(*target, (line, want(function, None)));
            }
            [] => {
                let line = format!("{target:#x} matches no place");
                wanted.insert(*target, (line, want(function, None)));
            }
            many => {
                let line = format!("{target:#x} matches {} places", many.len());
                wanted.insert(*target, (line, want(function, Some(many.to_vec()))));
            }
        }
    }
    for (target, (slot, holds)) in &tables {
        let table = row_resolve::Kind::Table;
        let Some(function) = moved.get(holds).copied() else {
            let line = format!("{target:#x}: the function in slot {slot} was not found");
            wanted.insert(*target, (line, want(table, None)));
            continue;
        };
        match find_table(pe, *slot, function).as_slice() {
            [only] => {
                moved.insert(*target, *only);
                report
                    .fingerprinted
                    .push(format!("{target:#x} -> {only:#x}"));
            }
            many => {
                let line = format!(
                    "{target:#x}: {} tables hold {function:#x} in slot {slot}",
                    many.len()
                );
                let candidates = (!many.is_empty()).then(|| many.to_vec());
                wanted.insert(*target, (line, want(table, candidates)));
            }
        }
    }
    let mut followed = BTreeMap::new();
    match prior_pe {
        Some(prior_pe) if !wanted.is_empty() => {
            let table_targets: BTreeSet<u32> = tables
                .keys()
                .copied()
                .chain(
                    wanted
                        .iter()
                        .filter(|(_, (_, want))| want.kind == row_resolve::Kind::Table)
                        .map(|(target, _)| *target),
                )
                .collect();
            let asked: BTreeMap<u32, row_resolve::Wanted> = wanted
                .iter()
                .map(|(target, (_, want))| (*target, want.clone()))
                .collect();
            let resolution = row_resolve::resolve(prior_pe, pe, &moved, &asked, &table_targets);
            for (target, (line, _)) in &wanted {
                match resolution.places.get(target) {
                    Some(place) => {
                        moved.insert(*target, *place);
                    }
                    None => {
                        let reason = resolution
                            .reasons
                            .get(target)
                            .map_or("no method was run", String::as_str);
                        report.review.push(format!("{line}; not settled: {reason}"));
                    }
                }
            }
            report.settled.extend(resolution.settled);
            followed = resolution.followed;
        }
        _ => report
            .review
            .extend(wanted.into_values().map(|(line, _)| line)),
    }
    let places: BTreeSet<u32> = moved.values().copied().collect();
    if places.len() != moved.len() {
        report
            .review
            .push("two addresses found the same place".to_owned());
    }
    let (mut row, mut report) = finish_row(row, &moved, pe, report);
    if let Some(object) = row.as_object_mut() {
        // They describe the prior build, so they would mislead in this row.
        object.remove("fingerprints");
    }
    if let Some(prior_pe) = prior_pe {
        read_carried(prior, &mut row, &moved, &followed, prior_pe, pe, &mut report);
    }
    (row, report)
}

/// The value at a path such as `layout.frame_ids[2]` or `toolbar.dev.colours[3][0]`.
fn value_at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for part in path.split('.') {
        let (key, indexes) = part.split_at(part.find('[').unwrap_or(part.len()));
        current = current.get(key)?;
        for index in indexes.split(']').filter(|index| !index.is_empty()) {
            current = current.get(index.trim_start_matches('[').parse::<usize>().ok()?)?;
        }
    }
    Some(current)
}

fn value_at_mut<'a>(value: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    let mut current = value;
    for part in path.split('.') {
        let (key, indexes) = part.split_at(part.find('[').unwrap_or(part.len()));
        current = current.get_mut(key)?;
        for index in indexes.split(']').filter(|index| !index.is_empty()) {
            current = current.get_mut(index.trim_start_matches('[').parse::<usize>().ok()?)?;
        }
    }
    Some(current)
}

/// Reads each value the recipes leave, a member offset, slot, size or id, from the instructions
/// that use it in the prior build's functions and the same instructions in the new build. A value
/// that cannot be read keeps a review line with the reason and stays listed as carried, so the row
/// is never published with a value of the prior build.
fn read_carried(
    prior: &Value,
    row: &mut Value,
    moved: &BTreeMap<u32, u32>,
    followed: &BTreeMap<u32, u32>,
    prior_pe: &PeFile,
    pe: &PeFile,
    report: &mut PortReport,
) {
    let mut values = Vec::new();
    let mut unreadable = Vec::new();
    for path in std::mem::take(&mut report.carried) {
        let items: Vec<(String, &Value)> = match value_at(&prior["build"], &path) {
            Some(Value::Array(items)) => items
                .iter()
                .enumerate()
                .map(|(index, item)| (format!("{path}[{index}]"), item))
                .collect(),
            Some(item) => vec![(path.clone(), item)],
            None => vec![],
        };
        if items.is_empty() {
            unreadable.push(path);
        }
        for (path, item) in items {
            match crate::row_recipe::number(item) {
                Some(value) => values.push((path, value)),
                None => unreadable.push(path),
            }
        }
    }
    // Each part of the row reads its values from its own functions.
    let mut sections: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
    for (section, part) in prior["build"].as_object().into_iter().flatten() {
        let mut part = part.clone();
        let mut pairs = Vec::new();
        walk_addresses("", &mut part, &mut |_, value| {
            if let Some(address) = read_address(value).filter(|value| is_address(*value)) {
                let address = address as u32;
                if let Some(place) = moved.get(&address).filter(|_| prior_pe.is_code(address)) {
                    pairs.push((address, *place));
                }
            }
        });
        pairs.sort_unstable();
        pairs.dedup();
        if !pairs.is_empty() {
            sections.insert(section.clone(), pairs);
        }
    }
    for reading in row_resolve::read_values(prior_pe, pe, &sections, followed, &values) {
        match reading.value {
            Ok((value, how)) => {
                if let Some(place) = value_at_mut(&mut row["build"], &reading.path) {
                    *place = if place.is_string() {
                        write_address(value)
                    } else {
                        json!(value)
                    };
                }
                report.derived.push(if value == reading.prior {
                    format!("{}: {value:#x}, {how}", reading.path)
                } else {
                    format!("{}: {:#x} -> {value:#x}, {how}", reading.path, reading.prior)
                });
            }
            Err(reason) => {
                report.review.push(format!(
                    "{} was not read from the new build: {reason}",
                    reading.path
                ));
                report.carried.push(reading.path);
            }
        }
    }
    for path in unreadable {
        report
            .review
            .push(format!("{path} is not a number this generator can read"));
        report.carried.push(path);
    }
}

/// The key of a row made without its build's symbols. It holds what the row job needs to make the
/// row again from the symbols once they are published, and to compare the two. The runtime reads
/// only `frame` and `build`, so the mark changes nothing in a browser.
pub const WITHOUT_PDB: &str = "made_without_pdb";

/// Marks a row made from fingerprints with the symbol key of its module, the prior row it was made
/// from, and how the row job fetches the build again.
pub fn mark_without_pdb(
    row: &mut Value,
    pdb_key: &str,
    prior_version: &str,
    channel: &str,
    download: &str,
) {
    row[WITHOUT_PDB] = json!({
        "pdb_key": pdb_key,
        "prior_version": prior_version,
        "channel": channel,
        "download": download,
    });
}

/// Where a row made from fingerprints differs from the row the build's own symbols give. A value
/// the fingerprint row left out is not a difference, since the runtime does without it.
pub fn differences_from_pdb_row(found: &Value, from_pdb: &Value) -> Vec<String> {
    fn walk(path: &str, found: &Value, pdb: &Value, out: &mut Vec<String>) {
        match (found, pdb) {
            (Value::Null, _) => {}
            (Value::Object(left), Value::Object(right)) => {
                for (key, value) in left {
                    let other = right.get(key).unwrap_or(&Value::Null);
                    walk(&format!("{path}.{key}"), value, other, out);
                }
            }
            (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
                for (index, (a, b)) in left.iter().zip(right).enumerate() {
                    walk(&format!("{path}[{index}]"), a, b, out);
                }
            }
            _ if found != pdb => {
                out.push(format!(
                    "{path}: {found} from fingerprints, {pdb} from the PDB"
                ));
            }
            _ => {}
        }
    }
    let mut out = vec![];
    for part in ["frame", "build"] {
        walk(part, &found[part], &from_pdb[part], &mut out);
    }
    out
}

/// Compares the marked row of `browser` `version` in `table` with the row made from the build's
/// symbols. When they agree the mark is removed, since the symbols now vouch for the row. The
/// differences are returned, and the table is left as it was when there are any.
pub fn check_against_pdb(
    table: &mut Value,
    browser: &str,
    version: &str,
    from_pdb: &Value,
) -> Result<Vec<String>, String> {
    let row = table["rows"]
        .as_array_mut()
        .ok_or("the table has no rows")?
        .iter_mut()
        .find(|row| row["frame"]["browser"] == browser && row["frame"]["version"] == version)
        .ok_or_else(|| format!("the table has no row for {browser} {version}"))?;
    if row.get(WITHOUT_PDB).is_none() {
        return Err(format!(
            "the row of {browser} {version} was not made without its PDB"
        ));
    }
    let differences = differences_from_pdb_row(row, from_pdb);
    if differences.is_empty() {
        if let Some(object) = row.as_object_mut() {
            object.remove(WITHOUT_PDB);
        }
    }
    Ok(differences)
}

/// Adds to each review line that starts with a prior address the public name that address lies
/// in, from the prior build's symbols, so a person sees which function was not found.
pub fn name_review_lines(review: &mut [String], named: &BTreeMap<u32, (u32, Vec<String>)>) {
    for line in review {
        let Some(digits) = line.strip_prefix("0x") else {
            continue;
        };
        let end = digits
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(digits.len());
        let Ok(address) = u32::from_str_radix(&digits[..end], 16) else {
            continue;
        };
        if let Some((distance, names)) = named.get(&address) {
            if let Some(name) = names.first() {
                line.push_str(&format!(" ({name}+{distance:#x} in the prior build)"));
            }
        }
    }
}

/// A folded function that fills a known table slot is the one in that slot of the new table.
fn by_slot(
    target: u32,
    choices: &[(u32, usize)],
    build: &Value,
    pe: &PeFile,
    named: &BTreeMap<u32, (u32, Vec<String>)>,
    symbols: &HashMap<String, Vec<u32>>,
) -> Option<u32> {
    for (vtable_path, slot_path, function_path) in VTABLE_SLOTS {
        if usize_at(build, function_path) != Some(u64::from(target)) {
            continue;
        }
        let vtable = usize_at(build, vtable_path)? as u32;
        let slot = usize_at(build, slot_path)? as u32;
        let (distance, names) = named.get(&vtable)?;
        let [(new_vtable, _)] = candidates(names, *distance, symbols)[..] else {
            continue;
        };
        let held = pe.pointer_rva(new_vtable + slot * 8)?;
        if choices.iter().any(|(place, _)| *place == held) {
            return Some(held);
        }
    }
    // The classes of hidden buttons keep the function of one known slot.
    let slot = usize_at(build, "toolbar.theme_slot")? as u32;
    for pair in build["toolbar"]["hidden"].as_array()? {
        if read_address(&pair[1]) != Some(u64::from(target)) {
            continue;
        }
        let (distance, names) = named.get(&(read_address(&pair[0])? as u32))?;
        let [(new_vtable, _)] = candidates(names, *distance, symbols)[..] else {
            continue;
        };
        let held = pe.pointer_rva(new_vtable + slot * 8)?;
        if choices.iter().any(|(place, _)| *place == held) {
            return Some(held);
        }
    }
    None
}

/// The frame entry keeps plain numbers. The walk over a row reads `0x` strings.
fn frame_addresses(frame: &Value) -> Value {
    let field = |key: &str| frame[key].as_u64().map_or(Value::Null, write_address);
    json!({
        "frame_view_vtable": field("frame_view_vtable"),
        "client_view_bounds": field("client_view_bounds"),
        "hwnd_for_view": field("hwnd_for_view"),
    })
}

fn apply_frame_addresses(frame: &mut Value, addresses: &Value) {
    for key in ["frame_view_vtable", "client_view_bounds", "hwnd_for_view"] {
        if let Some(value) = read_address(&addresses[key]) {
            frame[key] = json!(value);
        }
    }
}

fn refresh_frame_guard(frame: &mut Value, pe: &PeFile, report: &mut PortReport) {
    let (Some(address), Some(old)) = (
        frame["hwnd_for_view"].as_u64(),
        frame["hwnd_for_view_prologue"].as_str().and_then(hex),
    ) else {
        report
            .review
            .push("the frame entry has no hwnd_for_view".into());
        return;
    };
    if let Some(new) = read_guard(
        "frame.hwnd_for_view_prologue",
        address,
        &old,
        true,
        pe,
        report,
    ) {
        frame["hwnd_for_view_prologue"] = Value::String(hex_text(&new));
    }
}

fn check_frame_slot(frame: &mut Value, pe: &PeFile, report: &mut PortReport) {
    let field = |key: &str| frame[key].as_u64().map(|value| value as u32);
    let (Some(vtable), Some(slot), Some(function)) = (
        field("frame_view_vtable"),
        field("client_view_slot"),
        field("client_view_bounds"),
    ) else {
        report.review.push("the frame entry is incomplete".into());
        return;
    };
    if pe.pointer_rva(vtable + slot * 8) == Some(function) {
        return;
    }
    // A milestone that removes a virtual function moves the slot, and the table itself says where
    // the function went, as the recipes of the build's own slots do.
    match pe.vtable_slot(vtable, function) {
        Some(moved) => {
            frame["client_view_slot"] = json!(moved);
            report
                .derived
                .push(format!("frame.client_view_slot: {slot:#x} -> {moved:#x}"));
        }
        None => report.review.push(format!(
            "the frame vtable {vtable:#x} slot {slot} does not hold {function:#x}"
        )),
    }
}

/// The start of the public name of `views::View::ConvertPointToScreen(const View*, gfx::Point*)`:
/// a static function returning nothing. The other overload returns the point and is named `SA?AV`.
pub const SCREEN_ORIGIN_NAME: &str = "?ConvertPointToScreen@View@views@@SAX";

/// How many bytes of that function's start the runtime compares before it ever calls it.
const SCREEN_ORIGIN_BYTES: usize = 12;

/// Whether a public name is the one `screen_origin` is found by.
pub fn is_screen_origin_name(name: &str) -> bool {
    stable_name(name).starts_with(SCREEN_ORIGIN_NAME)
}

/// The first whole instructions of a function, at least `SCREEN_ORIGIN_BYTES` long.
fn function_start(pe: &PeFile, address: u32) -> Option<Vec<u8>> {
    if !pe.is_code(address) {
        return None;
    }
    let window = pe.read(address, 32)?;
    let (ends, _) = x64::boundaries(window, SCREEN_ORIGIN_BYTES);
    let end = *ends.iter().find(|end| **end >= SCREEN_ORIGIN_BYTES)?;
    Some(window[..end].to_vec())
}

/// Adds `screen_origin`, `[address, first bytes]` of `views::View::ConvertPointToScreen`, to a row
/// whose prior row had none, by the function's public name in the new build. A prior row that had
/// it was ported with every other address. A build with no single place for the name goes without
/// it, and the runtime keeps placing bubbles from the window's monitor, so the row is not stopped.
pub fn seed_screen_origin(row: &mut Value, places: &[u32], pe: &PeFile, report: &mut PortReport) {
    if row["build"]["screen_origin"].is_array() {
        return;
    }
    let mut places = places.to_vec();
    places.sort_unstable();
    places.dedup();
    let found = match places.as_slice() {
        [only] => function_start(pe, *only).map(|bytes| (*only, bytes)),
        _ => None,
    };
    match found {
        Some((address, bytes)) => {
            row["build"]["screen_origin"] =
                json!([write_address(u64::from(address)), hex_text(&bytes)]);
            report.derived.push(format!(
                "build.screen_origin: {SCREEN_ORIGIN_NAME} at {address:#x}"
            ));
        }
        None => {
            row["build"]["screen_origin"] = Value::Null;
            report.left_out.push(format!(
                "build.screen_origin: no single readable function named {SCREEN_ORIGIN_NAME}: {places:x?}"
            ));
        }
    }
}

/// The review items for the parts of the prior row that only a person writes today. A row made
/// without them opens Rivet's frame with those features off, so it goes to a person instead of
/// the table. The runtime names them in the prior row's `hand_only` list.
pub fn hand_only_review(prior: &Value) -> Vec<String> {
    prior["hand_only"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(Value::as_str)
        .map(|part| {
            format!("the prior row has {part}, which this generator cannot make yet; without it those features would be off")
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr6_a_row_whose_prior_had_hand_written_parts_goes_to_a_person() {
        let prior = serde_json::json!({
            "frame": {}, "build": {},
            "hand_only": ["address bar functions and the app window layout", "page tools", "the bubble anchor"]
        });
        let review = hand_only_review(&prior);
        assert_eq!(review.len(), 3);
        assert!(review[0].contains("address bar functions"));
        // A prior row with nothing written by hand adds nothing.
        assert!(hand_only_review(&serde_json::json!({ "frame": {}, "build": {} })).is_empty());
    }
    const IMAGE_BASE: u64 = 0x1_8000_0000;

    /// A small 64-bit module: one code section at RVA 0x100000, read from the given bytes.
    fn module(code: &[(u32, &[u8])]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x400];
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        bytes[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[0x98 + 24..0x98 + 32].copy_from_slice(&IMAGE_BASE.to_le_bytes());
        let header = 0x98 + 240;
        bytes[header + 8..header + 12].copy_from_slice(&0x180000u32.to_le_bytes());
        bytes[header + 12..header + 16].copy_from_slice(&0x100000u32.to_le_bytes());
        bytes[header + 20..header + 24].copy_from_slice(&0x400u32.to_le_bytes());
        bytes[header + 36..header + 40].copy_from_slice(&0x2000_0000u32.to_le_bytes());
        bytes.resize(0x400 + 0x10000, 0xCC);
        for (rva, data) in code {
            let at = 0x400 + (*rva - 0x100000) as usize;
            bytes[at..at + data.len()].copy_from_slice(data);
        }
        bytes
    }

    fn prior() -> Value {
        json!({
            "frame": {
                "browser": "B", "version": "1.0.0.1", "module": "b.dll",
                "frame_view_vtable": 0x200000, "client_view_slot": 1, "client_view_bounds": 0x200100,
                "hwnd_for_view": 0x200200, "hwnd_for_view_prologue": "4883ec28",
            },
            "build": {
                "browser": "B", "module": "b.dll", "version": [1, 0, 0, 1],
                "toolbar": {
                    "paint": "0x200300", "paint_prologue": "415741564155415456575553",
                    "view_layer": "0x20", "missing": "0x7fff0000",
                    "colours": [[1, 2147483648u32]],
                },
                "tabs": {"signatures": [["0x200300", "41574156"]]},
            },
        })
    }

    fn symbols_old() -> Vec<Symbol> {
        [
            (0x200000, "vt"),
            (0x200100, "bounds"),
            (0x200200, "hwnd"),
            (0x200300, "paint"),
        ]
        .map(|(address, name)| (address, name.to_owned()))
        .to_vec()
    }

    fn places(rows: &[(&str, &[u32])]) -> HashMap<String, Vec<u32>> {
        rows.iter()
            .map(|(name, list)| ((*name).to_owned(), list.to_vec()))
            .collect()
    }

    fn pointer(rva: u32) -> [u8; 8] {
        (IMAGE_BASE + u64::from(rva)).to_le_bytes()
    }

    fn port(
        prior: &Value,
        new: &[(&str, &[u32])],
        code: &[(u32, &[u8])],
        version: &str,
    ) -> (Value, PortReport) {
        let mut targets = addresses_of(&prior["build"]);
        for key in ["frame_view_vtable", "client_view_bounds", "hwnd_for_view"] {
            targets.insert(prior["frame"][key].as_u64().unwrap() as u32);
        }
        let named = name_addresses(&targets, &symbols_old());
        let bytes = module(code);
        let pe = PeFile::parse(&bytes).unwrap();
        let symbols = places(new);
        port_row(
            prior,
            &named,
            &NewBuild {
                version,
                pe: &pe,
                symbols: &symbols,
                private: &BTreeMap::new(),
            },
        )
    }

    const NEW_SYMBOLS: &[(&str, &[u32])] = &[
        ("vt", &[0x101000]),
        ("bounds", &[0x101100]),
        ("hwnd", &[0x101200]),
        ("paint", &[0x101300]),
    ];

    fn new_code() -> Vec<(u32, Vec<u8>)> {
        vec![
            (0x101008, pointer(0x101100).to_vec()),
            (0x101200, hex("4883ec18").unwrap()),
            (0x101300, hex("415741564155415456575553").unwrap()),
        ]
    }

    fn code_refs(code: &[(u32, Vec<u8>)]) -> Vec<(u32, &[u8])> {
        code.iter()
            .map(|(rva, data)| (*rva, data.as_slice()))
            .collect()
    }

    #[test]
    fn nfr3_screen_origin_is_found_by_its_public_name_in_the_new_build() {
        // push rsi; push rdi; sub rsp, 0x28; mov rsi, rdx; mov rdi, rcx; call.
        let start = hex("56574883ec284889d64889cfe800000000").unwrap();
        let bytes = module(&[(0x101400, start.as_slice())]);
        let pe = PeFile::parse(&bytes).unwrap();
        let mut row = prior();
        let mut report = PortReport::default();
        seed_screen_origin(&mut row, &[0x101400, 0x101400], &pe, &mut report);
        assert_eq!(
            row["build"]["screen_origin"],
            json!(["0x101400", "56574883ec284889d64889cf"])
        );
        assert!(report.review.is_empty() && report.left_out.is_empty());
        // A row that already has it keeps the place the port gave it.
        seed_screen_origin(&mut row, &[0x101500], &pe, &mut report);
        assert_eq!(row["build"]["screen_origin"][0], "0x101400");
        // No name, or two places for it, leaves the part out without stopping the row.
        for places in [&[][..], &[0x101400, 0x101500][..]] {
            let mut row = prior();
            let mut report = PortReport::default();
            seed_screen_origin(&mut row, places, &pe, &mut report);
            assert!(row["build"]["screen_origin"].is_null());
            assert_eq!(report.left_out.len(), 1);
            assert!(report.review.is_empty());
        }
        // Only the in-place overload, which returns nothing, is the one.
        assert!(is_screen_origin_name(
            "?ConvertPointToScreen@View@views@@SAXPEBV12@PEAVPoint@gfx@@@Z"
        ));
        assert!(!is_screen_origin_name(
            "?ConvertPointToScreen@View@views@@SA?AVPoint@gfx@@PEBV12@AEBV34@@Z"
        ));
        assert!(!is_screen_origin_name(
            "?ConvertRectToScreen@View@views@@SAXPEBV12@PEAVRect@gfx@@@Z"
        ));
    }

    #[test]
    fn nfr3_a_new_row_takes_addresses_by_name_and_bytes_from_the_new_module() {
        let code = new_code();
        let (row, report) = port(&prior(), NEW_SYMBOLS, &code_refs(&code), "1.0.0.2");
        assert_eq!(report.review, Vec::<String>::new());
        assert_eq!(row["build"]["version"], json!([1, 0, 0, 2]));
        assert_eq!(row["frame"]["version"], "1.0.0.2");
        assert_eq!(row["build"]["toolbar"]["paint"], "0x101300");
        assert_eq!(row["build"]["tabs"]["signatures"][0][0], "0x101300");
        assert_eq!(row["frame"]["frame_view_vtable"], 0x101000);
        // Bytes come from the new module.
        assert_eq!(row["frame"]["hwnd_for_view_prologue"], "4883ec18");
        // Offsets, the absent marker and colours are carried over as they are.
        assert_eq!(row["build"]["toolbar"]["view_layer"], "0x20");
        assert_eq!(row["build"]["toolbar"]["missing"], "0x7fff0000");
        assert_eq!(row["build"]["toolbar"]["colours"][0][1], 2147483648u32);
        assert!(row.get("frame_addresses").is_none());
    }

    #[test]
    fn nfr3_a_name_the_new_build_lacks_stops_the_row() {
        let code = new_code();
        let symbols: Vec<_> = NEW_SYMBOLS
            .iter()
            .filter(|(name, _)| *name != "paint")
            .copied()
            .collect();
        let (_, report) = port(&prior(), &symbols, &code_refs(&code), "1.0.0.2");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("no new place")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr3_a_row_that_crosses_a_milestone_stops_for_review() {
        let code = new_code();
        let (_, report) = port(&prior(), NEW_SYMBOLS, &code_refs(&code), "2.0.0.1");
        assert!(
            report.review.iter().any(|line| line.contains("milestone")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr3_a_frame_slot_that_no_longer_holds_its_function_stops_the_row() {
        let code: Vec<_> = new_code()
            .into_iter()
            .filter(|(rva, _)| *rva != 0x101008)
            .collect();
        let (_, report) = port(&prior(), NEW_SYMBOLS, &code_refs(&code), "1.0.0.2");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("does not hold")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr3_folded_names_that_stay_apart_are_settled_by_the_guard_or_stop_the_row() {
        let mut code = new_code();
        code.push((0x101400, hex("565755534883ec584889ce48").unwrap()));
        let two: &[u32] = &[0x101300, 0x101400];
        let symbols: Vec<_> = NEW_SYMBOLS
            .iter()
            .map(|(name, list)| {
                if *name == "paint" {
                    (*name, two)
                } else {
                    (*name, *list)
                }
            })
            .collect();
        let (row, report) = port(&prior(), &symbols, &code_refs(&code), "1.0.0.2");
        assert_eq!(row["build"]["toolbar"]["paint"], "0x101300");
        assert_eq!(report.settled.len(), 1);
        assert!(report.review.is_empty(), "{:?}", report.review);
        // Two places with the same start cannot be told apart.
        let mut same = new_code();
        same.push((0x101400, hex("415741564155415456575553").unwrap()));
        let (_, report) = port(&prior(), &symbols, &code_refs(&same), "1.0.0.2");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("several places")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr3_a_function_with_no_public_name_is_not_moved_by_distance() {
        let mut prior = prior();
        prior["build"]["toolbar"]["inside"] = json!("0x200340");
        let code = new_code();
        let (_, report) = port(&prior, NEW_SYMBOLS, &code_refs(&code), "1.0.0.2");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("no name in both")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr3_a_guard_a_hook_copies_stops_on_an_instruction_in_the_new_build() {
        let mut prior = prior();
        // Fifteen bytes that end on an instruction, and a new start where byte fifteen splits one.
        prior["build"]["toolbar"]["paint_prologue"] = json!("41574156565755534881ecb8000000");
        let mut code = new_code();
        code[2].1 = hex("4156565755534881eca00000004889ce0000").unwrap();
        let (row, report) = port(&prior, NEW_SYMBOLS, &code_refs(&code), "1.0.0.2");
        assert_eq!(
            row["build"]["toolbar"]["paint_prologue"],
            "4156565755534881eca0000000"
        );
        assert!(report.review.is_empty(), "{:?}", report.review);
    }

    #[test]
    fn nfr3_stable_name_drops_what_the_linker_adds() {
        assert_eq!(stable_name("??1GURL@@QEAA@XZ\0"), "??1GURL@@QEAA@XZ");
        assert_eq!(stable_name("?F@@YAXXZ.llvm.123456"), "?F@@YAXXZ");
        assert_eq!(
            stable_name(".weak.??_7BrowserView@@6B@.default.?RejectPromise@@"),
            "??_7BrowserView@@6B@"
        );
    }

    #[test]
    fn nfr3_addresses_are_told_from_offsets_and_the_absent_marker() {
        let row = json!({"a": "0x10", "b": "0xfffff", "c": "0x100000", "d": ["0x7fff0000", "0x4fb01f0"], "e": 1234567, "f": "0xabcd_not"});
        assert_eq!(addresses_of(&row), BTreeSet::from([0x100000, 0x4fb01f0]));
    }

    #[test]
    fn nfr3_an_address_inside_a_table_is_named_by_the_table_below_it() {
        let symbols: Vec<Symbol> = vec![
            (0x1000, "other".into()),
            (0x2000, "??_7Table@@6B@".into()),
            (0x2000, "alias".into()),
            (0x9000, "far".into()),
        ];
        let named = name_addresses(&BTreeSet::from([0x2000, 0x2250, 0x8000]), &symbols);
        assert_eq!(
            named[&0x2000],
            (0, vec!["??_7Table@@6B@".to_owned(), "alias".to_owned()])
        );
        assert_eq!(named[&0x2250].0, 0x250);
        assert!(!named.contains_key(&0x8000));
    }

    #[test]
    fn nfr3_x64_lengths_of_real_function_starts() {
        // Chrome 156.0.8078.74 chip text: the frame setup ends at byte 13.
        let chip = hex("4156565755534881eca00000004889").unwrap();
        let mut padded = chip.clone();
        padded.resize(40, 0x90);
        let (seen, complete) = x64::boundaries(&padded, 15);
        assert!(complete);
        assert_eq!(&seen[..6], [2, 3, 4, 5, 6, 13]);
        assert!(!seen.contains(&15));
        // A stack cookie load is RIP-relative.
        let cookie = hex("56574883ec784889ce488b0510640810").unwrap();
        let (seen, _) = x64::boundaries(&cookie, cookie.len());
        assert_eq!(seen.last(), Some(&16));
        assert_eq!(x64::has_relative(&cookie), Some(true));
        assert_eq!(x64::has_relative(&hex("4883ec28").unwrap()), Some(false));
        // Rounded corners: movss, ucomiss with a displacement, jne.
        let corners = hex("f30f10020f2e81840400007533").unwrap();
        assert_eq!(x64::boundaries(&corners, 13).0, [4, 11, 13]);
    }

    #[test]
    fn nfr3_only_the_displacement_differs_between_two_builds_of_a_function() {
        let old = hex("5657534883ec5089ce488b0510f0450d").unwrap();
        let new = hex("5657534883ec5089ce488b0590a5330d").unwrap();
        assert_ne!(old, new);
        assert_eq!(x64::shape(&old), x64::shape(&new));
        let other = hex("415741565657534883ec4089d34889ce").unwrap();
        assert_ne!(x64::shape(&old), x64::shape(&other));
    }

    #[test]
    fn nfr3_guards_are_judged_by_what_a_hook_can_do_with_them() {
        let kind = |text| guard_kind(&hex(text).unwrap());
        // Twelve plain bytes, and a frame setup ending in the stack cookie load that hooks relocate.
        assert_eq!(kind("415741564155415456575553"), GuardKind::Copyable);
        assert_eq!(
            kind("5657534883ec5089ce488b0510f0450d"),
            GuardKind::Copyable
        );
        // Cut inside an instruction, or too short for the jump: only ever compared.
        assert_eq!(kind("565755534883ec584889ce48"), GuardKind::Compared);
        assert_eq!(kind("4883ec28488b"), GuardKind::Compared);
        assert_eq!(kind("564883ec20e8"), GuardKind::Compared);
        // A cookie load with more after it, which a row counts bytes to.
        assert_eq!(kind("488b0510f0450d4889442420"), GuardKind::Fixed);
    }

    #[test]
    fn nfr3_an_unknown_instruction_is_not_guessed() {
        assert_eq!(x64::decode(&[0x0F, 0x05]), None);
        assert_eq!(x64::boundaries(&[0x56, 0xCF, 0x90], 3), (vec![1], false));
        assert_eq!(x64::shape(&[0x56, 0xCF]), None);
    }

    #[test]
    fn nfr6_the_operands_of_a_body_instruction_are_located() {
        // mov rax, [rcx + 0x558]: a displacement of four bytes from the third byte.
        let load = x64::decode(&[0x48, 0x8B, 0x81, 0x58, 0x05, 0x00, 0x00]).unwrap();
        assert_eq!(load.displacement, Some((3, 4)));
        assert_eq!(load.immediate, None);
        // call [rax + 0x2b8]: a table slot read through its displacement.
        let call = x64::decode(&[0xFF, 0x90, 0xB8, 0x02, 0x00, 0x00]).unwrap();
        assert_eq!(call.displacement, Some((2, 4)));
        // mov edx, 0x9c45, and cmp byte [rcx + 0x30], 1.
        let id = x64::decode(&[0xBA, 0x45, 0x9C, 0x00, 0x00]).unwrap();
        assert_eq!(id.immediate, Some((1, 4)));
        let compare = x64::decode(&[0x80, 0x79, 0x30, 0x01]).unwrap();
        assert_eq!(compare.displacement, Some((2, 1)));
        assert_eq!(compare.immediate, Some((3, 1)));
        // lock cmpxchg and a VEX move decode only as body instructions.
        let lock = [0xF0, 0x48, 0x0F, 0xB1, 0x51, 0x08];
        assert_eq!(x64::decode(&lock), None);
        assert_eq!(x64::decode_body(&lock).unwrap().displacement, Some((5, 1)));
        let vex = [0xC5, 0xF8, 0x10, 0x41, 0x10];
        assert_eq!(x64::decode_body(&vex).unwrap().length, 5);
        assert_eq!(
            x64::skeleton(&[0x48, 0x8B, 0x81, 0x58, 0x05, 0x00, 0x00], &load),
            [0x48, 0x8B, 0x81, 0, 0, 0, 0]
        );
    }

    /// A verified row whose functions sit at 0x101000 in its module.
    fn verified_row() -> Value {
        json!({
            "frame": {
                "browser": "B", "version": "1.0.0.2", "module": "b.dll",
                "frame_view_vtable": 0x101000, "client_view_slot": 1, "client_view_bounds": 0x101100,
                "hwnd_for_view": 0x101200, "hwnd_for_view_prologue": "4883ec28",
            },
            "build": {
                "browser": "B", "module": "b.dll", "version": [1, 0, 0, 2],
                "toolbar": {
                    "paint": "0x101300", "paint_prologue": "415741564155415456575553",
                    "view_layer": "0x20", "missing": "0x7fff0000",
                },
                "tabs": {"signatures": [["0x101300", "41574156"]]},
            },
        })
    }

    /// The same functions `shift` bytes further on, with `displacement` in every RIP-relative
    /// field, as the next build of a milestone lays them out.
    fn shifted_code(shift: u32, displacement: u8) -> Vec<(u32, Vec<u8>)> {
        let d = format!("{displacement:02x}");
        vec![
            (0x101008 + shift, pointer(0x101100 + shift).to_vec()),
            // mov [rsp+8],rbx; push rdi; sub rsp,20h; mov rax,[rip+d]; xor rax,rsp
            (
                0x101100 + shift,
                hex(&format!("48895c2408574883ec20488b05{d}0000004833c4")).unwrap(),
            ),
            // sub rsp,28h; mov rcx,[rip+d]; call rel32
            (
                0x101200 + shift,
                hex(&format!("4883ec28488b0d{d}000000e8{d}000000")).unwrap(),
            ),
            // push r15..rbx; sub rsp,0a0h
            (
                0x101300 + shift,
                hex("4157415641554154565755534881eca0000000").unwrap(),
            ),
        ]
    }

    fn fingerprinted() -> Value {
        let bytes = module(&code_refs(&shifted_code(0, 0x10)));
        let pe = PeFile::parse(&bytes).unwrap();
        let mut row = verified_row();
        add_fingerprints(&mut row, &pe).unwrap();
        row
    }

    fn find(row: &Value, code: &[(u32, Vec<u8>)], version: &str) -> Result<Value, Vec<String>> {
        let bytes = module(&code_refs(code));
        let pe = PeFile::parse(&bytes).unwrap();
        find_row(row, version, &pe)
    }

    #[test]
    fn nfr6_an_unknown_build_takes_the_addresses_its_fingerprints_find() {
        let row = fingerprinted();
        assert_eq!(row["fingerprints"]["0x101000"]["slot"], 1);
        assert_eq!(row["fingerprints"]["0x101000"]["holds"], "0x101100");
        let found = find(&row, &shifted_code(0x2000, 0x44), "1.0.0.3").unwrap();
        assert_eq!(found["build"]["toolbar"]["paint"], "0x103300");
        assert_eq!(found["build"]["tabs"]["signatures"][0][0], "0x103300");
        assert_eq!(found["frame"]["frame_view_vtable"], 0x103000);
        assert_eq!(found["frame"]["client_view_bounds"], 0x103100);
        assert_eq!(found["frame"]["hwnd_for_view"], 0x103200);
        assert_eq!(found["frame"]["version"], "1.0.0.3");
        assert_eq!(found["build"]["version"], json!([1, 0, 0, 3]));
        // Values that are not addresses stay, and the prior build's fingerprints do not.
        assert_eq!(found["build"]["toolbar"]["view_layer"], "0x20");
        assert!(found.get("fingerprints").is_none());
        assert!(found.get("frame_addresses").is_none());
    }

    #[test]
    fn nfr6_a_fingerprint_that_matches_two_places_changes_nothing() {
        let mut code = shifted_code(0x2000, 0x44);
        code.push((
            0x105300,
            hex("4157415641554154565755534881eca0000000").unwrap(),
        ));
        let reasons = find(&fingerprinted(), &code, "1.0.0.3").unwrap_err();
        assert!(
            reasons
                .iter()
                .any(|line| line.contains("0x101300 matches 2 places")),
            "{reasons:?}"
        );
    }

    #[test]
    fn nfr6_a_changed_function_start_changes_nothing() {
        let mut code = shifted_code(0x2000, 0x44);
        // push r15; push r14; push r13; push rbp (was push r12) ...
        code[3].1 = hex("4157415641555556575553904881eca0000000").unwrap();
        let reasons = find(&fingerprinted(), &code, "1.0.0.3").unwrap_err();
        assert!(
            reasons
                .iter()
                .any(|line| line.contains("0x101300 matches no place")),
            "{reasons:?}"
        );
    }

    #[test]
    fn nfr6_a_table_that_no_longer_holds_its_function_changes_nothing() {
        let code: Vec<_> = shifted_code(0x2000, 0x44)
            .into_iter()
            .filter(|(rva, _)| *rva != 0x103008)
            .collect();
        let reasons = find(&fingerprinted(), &code, "1.0.0.3").unwrap_err();
        assert!(
            reasons.iter().any(|line| line.contains("0 tables hold")),
            "{reasons:?}"
        );
    }

    #[test]
    fn nfr6_fingerprints_do_not_cross_a_milestone() {
        let reasons = find(&fingerprinted(), &shifted_code(0x2000, 0x44), "2.0.0.1").unwrap_err();
        assert!(
            reasons.iter().any(|line| line.contains("milestone")),
            "{reasons:?}"
        );
    }

    #[test]
    fn nfr6_a_row_without_fingerprints_finds_nothing() {
        let reasons = find(&verified_row(), &shifted_code(0x2000, 0x44), "1.0.0.3").unwrap_err();
        assert_eq!(reasons, ["the row has no fingerprints"]);
    }

    /// The fingerprints of the verified row, read from its own module as the row job reads them
    /// when the prior row stored none.
    fn prints_of_prior() -> serde_json::Map<String, Value> {
        let bytes = module(&code_refs(&shifted_code(0, 0x10)));
        let pe = PeFile::parse(&bytes).unwrap();
        let (prints, missing) = fingerprints_of(&verified_row(), &pe);
        assert!(missing.is_empty(), "{missing:?}");
        prints
    }

    fn candidate(
        prints: &serde_json::Map<String, Value>,
        code: &[(u32, Vec<u8>)],
        version: &str,
    ) -> (Value, PortReport) {
        let bytes = module(&code_refs(code));
        let pe = PeFile::parse(&bytes).unwrap();
        find_candidate(&verified_row(), prints, None, version, &pe)
    }

    /// `module` with an exception table at 0x10f000 that lists `starts`.
    fn with_starts(code: &[(u32, Vec<u8>)], starts: &[u32]) -> Vec<u8> {
        let ranges: Vec<(u32, u32)> = starts.iter().map(|start| (*start, start + 0x20)).collect();
        with_ranges(code, &ranges)
    }

    /// `module` with an exception table at 0x10f000 that lists each `(start, end)`.
    fn with_ranges(code: &[(u32, Vec<u8>)], ranges: &[(u32, u32)]) -> Vec<u8> {
        let mut table = vec![];
        for (start, end) in ranges {
            table.extend_from_slice(&start.to_le_bytes());
            table.extend_from_slice(&end.to_le_bytes());
            table.extend_from_slice(&0u32.to_le_bytes());
        }
        let mut code = code.to_vec();
        code.push((0x10f000, table));
        let mut bytes = module(&code_refs(&code));
        let directory = 0x98 + 112 + 24;
        bytes[directory..directory + 4].copy_from_slice(&0x10f000u32.to_le_bytes());
        bytes[directory + 4..directory + 8]
            .copy_from_slice(&(12 * ranges.len() as u32).to_le_bytes());
        bytes
    }

    /// The verified row's paint function, which reads `view_layer` with `loads` past the bytes its
    /// fingerprint holds.
    fn paint_reading(loads: &str) -> Vec<u8> {
        let mut bytes = hex("4157415641554154565755534881eca0000000").unwrap();
        bytes.resize(32, 0x90);
        bytes.extend(hex(loads).unwrap());
        bytes.push(0xC3);
        bytes
    }

    /// A candidate row for a build whose paint function reads `view_layer` with `new_loads`, made
    /// from the verified row and its build, whose paint function reads it with `prior_loads`.
    fn read_view_layer(prior_loads: &str, new_loads: &str) -> (Value, PortReport) {
        let ranges = |shift: u32, paint: usize| {
            vec![
                (0x101100 + shift, 0x101120 + shift),
                (0x101200 + shift, 0x101220 + shift),
                (0x101300 + shift, 0x101300 + shift + paint as u32),
            ]
        };
        let mut old = shifted_code(0, 0x10);
        old[3].1 = paint_reading(prior_loads);
        let prior_bytes = with_ranges(&old, &ranges(0, old[3].1.len()));
        let prior_pe = PeFile::parse(&prior_bytes).unwrap();
        let (prints, missing) = fingerprints_of(&verified_row(), &prior_pe);
        assert!(missing.is_empty(), "{missing:?}");
        let mut new = shifted_code(0x2000, 0x44);
        new[3].1 = paint_reading(new_loads);
        let bytes = with_ranges(&new, &ranges(0x2000, new[3].1.len()));
        let pe = PeFile::parse(&bytes).unwrap();
        find_candidate(&verified_row(), &prints, Some(&prior_pe), "1.0.0.3", &pe)
    }

    #[test]
    fn nfr6_a_carried_value_is_read_from_the_instructions_that_use_it() {
        // mov rax, [rcx + 0x20]; mov rdx, [rcx + 0x20], and the new build moved the member to 0x28.
        let (row, report) =
            read_view_layer("488b8120000000488b9120000000", "488b8128000000488b9128000000");
        assert!(report.review.is_empty(), "{:?}", report.review);
        assert_eq!(row["build"]["toolbar"]["view_layer"], "0x28");
        assert!(report.carried.is_empty(), "{:?}", report.carried);
        assert_eq!(
            report.derived,
            ["toolbar.view_layer: 0x20 -> 0x28, read at 2 uses in 1 of its functions"]
        );
    }

    #[test]
    fn nfr6_a_value_no_instruction_gives_stops_the_row() {
        // The new build reads the member with a short displacement: not the same instruction.
        let (_, report) = read_view_layer("488b8120000000488b9120000000", "488b4128488b5128");
        assert_eq!(report.carried, ["toolbar.view_layer"]);
        assert!(report.derived.is_empty(), "{:?}", report.derived);
        assert!(
            report.review.iter().any(|line| line
                .starts_with("toolbar.view_layer was not read from the new build: its 2 uses")),
            "{:?}",
            report.review
        );
        // One use alone is not read either, and neither are uses that disagree.
        let (_, report) = read_view_layer("488b8120000000", "488b8128000000");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("a second reading is needed")),
            "{:?}",
            report.review
        );
        let (_, report) =
            read_view_layer("488b8120000000488b9120000000", "488b8128000000488b9130000000");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("its uses disagree")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr6_a_function_start_is_found_only_at_a_function_start() {
        // The paint function's first bytes also open a block inside another function at 0x105300.
        let mut code = shifted_code(0x2000, 0x44);
        code.push((
            0x105300,
            hex("4157415641554154565755534881eca0000000").unwrap(),
        ));
        let prior_bytes = with_starts(&shifted_code(0, 0x10), &[0x101100, 0x101200, 0x101300]);
        let prior_pe = PeFile::parse(&prior_bytes).unwrap();
        let prior_starts = prior_pe.function_starts();
        assert_eq!(prior_starts.len(), 3);
        // The row holds no value without a recipe, so only its addresses are looked for.
        let mut verified = verified_row();
        verified["build"]["toolbar"]
            .as_object_mut()
            .unwrap()
            .remove("view_layer");
        let (prints, _) = fingerprints_of(&verified, &prior_pe);
        let bytes = with_starts(&code, &[0x103100, 0x103200, 0x103300]);
        let pe = PeFile::parse(&bytes).unwrap();
        let (row, report) = find_candidate(&verified, &prints, Some(&prior_pe), "1.0.0.3", &pe);
        assert!(report.review.is_empty(), "{:?}", report.review);
        assert_eq!(row["build"]["toolbar"]["paint"], "0x103300");
        // A part moved out of another function starts where it may, and a whole function does not.
        let mut split = shifted_code(0x2000, 0x44);
        split.push((
            0x105308,
            hex("4157415641554154565755534881eca0000000").unwrap(),
        ));
        let bytes = with_starts(&split, &[0x103100, 0x103200, 0x103300, 0x105308]);
        let pe = PeFile::parse(&bytes).unwrap();
        let (row, report) = find_candidate(&verified, &prints, Some(&prior_pe), "1.0.0.3", &pe);
        assert!(report.review.is_empty(), "{:?}", report.review);
        assert_eq!(row["build"]["toolbar"]["paint"], "0x103300");
        // Bytes that match only inside another function are no place at all.
        let bytes = with_starts(&code, &[0x103100, 0x103200]);
        let pe = PeFile::parse(&bytes).unwrap();
        let (_, report) = find_candidate(&verified, &prints, Some(&prior_pe), "1.0.0.3", &pe);
        assert!(
            report
                .review
                .iter()
                .any(|line| line
                    .starts_with("0x101300 matches no function start, of 2 places; not settled:")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr6_a_build_without_symbols_gets_a_row_from_the_prior_builds_fingerprints() {
        let (row, report) = candidate(&prints_of_prior(), &shifted_code(0x2000, 0x44), "1.0.0.3");
        assert!(report.review.is_empty(), "{:?}", report.review);
        assert!(report.made_without_pdb);
        assert_eq!(report.fingerprinted.len(), 4, "{:?}", report.fingerprinted);
        assert_eq!(row["build"]["toolbar"]["paint"], "0x103300");
        assert_eq!(row["frame"]["frame_view_vtable"], 0x103000);
        assert_eq!(row["frame"]["hwnd_for_view"], 0x103200);
        // Values that are not addresses are left to the recipes, and nothing of the prior build stays.
        assert_eq!(row["build"]["toolbar"]["view_layer"], "0x20");
        assert!(row.get("fingerprints").is_none());
    }

    #[test]
    fn nfr6_an_address_its_fingerprint_does_not_pin_down_is_not_vouched_for() {
        // Two places match the paint function: that part keeps its prior value and stops the row,
        // and the rest of the row is still found.
        let mut code = shifted_code(0x2000, 0x44);
        code.push((
            0x105300,
            hex("4157415641554154565755534881eca0000000").unwrap(),
        ));
        let (row, report) = candidate(&prints_of_prior(), &code, "1.0.0.3");
        assert!(
            report
                .review
                .iter()
                .any(|line| line.contains("0x101300 matches 2 places")),
            "{:?}",
            report.review
        );
        assert!(report
            .review
            .iter()
            .any(|line| line.contains("no new place")));
        assert_eq!(row["build"]["toolbar"]["paint"], "0x101300");
        assert_eq!(row["frame"]["hwnd_for_view"], 0x103200);
        // An address the prior build gave no fingerprint is not looked for at all.
        let mut prints = prints_of_prior();
        prints.remove("0x101200");
        let (_, report) = candidate(&prints, &shifted_code(0x2000, 0x44), "1.0.0.3");
        assert!(
            report
                .review
                .iter()
                .any(|line| line == "0x101200: no fingerprint"),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr6_a_row_made_without_its_pdb_is_checked_against_the_pdb_row() {
        let (mut row, _) = candidate(&prints_of_prior(), &shifted_code(0x2000, 0x44), "1.0.0.3");
        mark_without_pdb(&mut row, "ABC1", "1.0.0.2", "stable", "");
        assert_eq!(row[WITHOUT_PDB]["prior_version"], "1.0.0.2");
        let mut from_pdb = row.clone();
        from_pdb.as_object_mut().unwrap().remove(WITHOUT_PDB);
        // A part the fingerprint row left out is no difference.
        row["build"]["screen_origin"] = Value::Null;
        from_pdb["build"]["screen_origin"] = json!(["0x103400", "5657"]);
        let table = json!({"rows": [row]});
        let mut agreed = table.clone();
        assert_eq!(
            check_against_pdb(&mut agreed, "B", "1.0.0.3", &from_pdb).unwrap(),
            Vec::<String>::new()
        );
        assert!(agreed["rows"][0].get(WITHOUT_PDB).is_none());
        // A different address is a difference, and the row keeps its mark.
        from_pdb["build"]["toolbar"]["paint"] = json!("0x103310");
        let mut differs = table.clone();
        let differences = check_against_pdb(&mut differs, "B", "1.0.0.3", &from_pdb).unwrap();
        assert_eq!(differences.len(), 1, "{differences:?}");
        assert!(
            differences[0].starts_with("build.toolbar.paint:"),
            "{differences:?}"
        );
        assert_eq!(differs, table);
        // A row made from symbols has nothing to check.
        let mut plain = json!({"rows": [from_pdb.clone()]});
        assert!(check_against_pdb(&mut plain, "B", "1.0.0.3", &from_pdb).is_err());
    }

    #[test]
    fn nfr6_a_row_made_from_a_marked_prior_does_not_inherit_the_mark() {
        let mut prior = prior();
        mark_without_pdb(&mut prior, "ABC1", "1.0.0.0", "stable", "");
        let code = new_code();
        let (row, _) = port(&prior, NEW_SYMBOLS, &code_refs(&code), "1.0.0.2");
        assert!(row.get(WITHOUT_PDB).is_none());
    }

    #[test]
    fn nfr6_a_review_line_names_the_prior_function_it_is_about() {
        let named = BTreeMap::from([(0x101300, (0x10, vec!["?Paint@V@@".to_owned()]))]);
        let mut review = vec![
            "0x101300 matches no place".to_owned(),
            "0x101400: no fingerprint".to_owned(),
            "the milestone changed".to_owned(),
        ];
        name_review_lines(&mut review, &named);
        assert_eq!(
            review[0],
            "0x101300 matches no place (?Paint@V@@+0x10 in the prior build)"
        );
        assert_eq!(review[1], "0x101400: no fingerprint");
        assert_eq!(review[2], "the milestone changed");
    }

    #[test]
    fn nfr3_an_address_that_is_neither_code_nor_a_table_gets_no_fingerprints() {
        let bytes = module(&code_refs(&shifted_code(0, 0x10)));
        let pe = PeFile::parse(&bytes).unwrap();
        let mut row = verified_row();
        // Inside the code section's range but past what the file holds.
        row["build"]["toolbar"]["state"] = json!("0x1f0000");
        let error = add_fingerprints(&mut row, &pe).unwrap_err();
        assert!(error.contains("0x1f0000"), "{error}");
        assert!(row.get("fingerprints").is_none());
    }
}
