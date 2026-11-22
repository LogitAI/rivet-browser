//! Makes the supported-build row of a new browser build from the row of a prior build.
//!
//! Chromium's source changes little between two builds of a milestone, but every address moves.
//! Each address of the prior row is found again in the new module by its fingerprint, the first
//! instructions of the function without their displacements, or by a slot of a class table that
//! holds such a function. What a fingerprint does not pin down is settled by `row_resolve`, which
//! takes a place only where two independent methods agree. The instruction bytes that guard each
//! hook are then read again from the new module. Values that are not addresses (member offsets,
//! table slots, object sizes) are read again from the new module where `row_recipe` has a recipe
//! for them, or from the instructions that use them. No browser's debug symbols are read: the
//! verified prior row, its fingerprints and the two modules are all the generator has.
//!
//! A row is plain data here, in the form the runtime writes: a `usize` is a `0x` string, and bytes
//! are bare hex. A value this code cannot explain stops the row or turns its feature off: it never
//! reaches people's browsers, and the runtime's own prologue checks stand behind that.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use serde_json::{json, Value};

use crate::chrome_build::{hex, PeFile};
use crate::row_resolve;

/// The lowest value taken for an address. Offsets and slots lie below it.
const LOWEST_ADDRESS: u64 = 0x10_0000;
/// The address a build records for what it does not have.
const ABSENT: u64 = 0x7FFF_0000;
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
    pub bytes_changed: Vec<String>,
    pub slots_moved: Vec<String>,
    /// Values that are not addresses, read again from the new module by their recipes.
    pub derived: Vec<String>,
    /// Values that are not addresses and have no recipe, so they were carried over.
    pub carried: Vec<String>,
    /// Optional parts the new build could not supply. The runtime does without them.
    pub left_out: Vec<String>,
    /// Addresses a fingerprint found in exactly one place of the new build.
    pub fingerprinted: Vec<String>,
    /// Values that are Rivet's settings rather than facts about the build, kept from the prior row.
    pub settings: Vec<String>,
    /// Features this row turns off, each with the values that could not be read for it. The row
    /// clears those values, so none of the prior build reaches a browser.
    pub switched_off: Vec<String>,
    /// Values the runtime does not read in this row, written as zero, with the reason.
    pub unused: Vec<String>,
    /// Values no instruction of the new build gave that the runtime finds by reflection, so the
    /// row leaves them out instead of carrying them.
    pub reflected: Vec<String>,
    /// Addresses no method placed in the new build that only features this row turns off hold,
    /// with the values that held them. The row clears those values like the feature's others.
    pub unplaced: Vec<String>,
    /// Whether every carried value must be settled: the milestone changed, or values were read.
    #[serde(skip)]
    must_check: bool,
    /// The milestone change, when there is one and nothing read the carried values.
    #[serde(skip)]
    milestone: Option<String>,
    /// Why each value that was tried could not be read.
    #[serde(skip)]
    unread: BTreeMap<String, String>,
    /// Values of features the row turns off whatever the milestone, with the reason: an address
    /// that has no place in the new build, or a recipe that failed.
    #[serde(skip)]
    off_values: BTreeMap<String, String>,
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

/// The value at a path such as `layout.on_paint_slot`.
fn at_path<'a>(row: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(row, |value, key| &value[key])
}

/// `(vtable, slot, function)`: a function the runtime replaces in a table, and so a check the new
/// module can answer on its own. A function listed twice settles a folded name.
const VTABLE_SLOTS: &[(&str, &str, &str)] = &[
    (
        "side_panel.layout_vtable",
        "side_panel.proposed_slot",
        "side_panel.proposed",
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
];

fn usize_at(row: &Value, path: &str) -> Option<u64> {
    read_address(at_path(row, path))
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

/// Addresses read from instructions above, which no fingerprint finds.
const READ_FROM_CODE: &[&str] = &[
    "split.set_rounded_corners_changed",
    "split.set_rounded_corners_parity",
];

/// Settles each value no recipe or instruction read from the new build. A value the runtime finds
/// by reflection is left out of the row. A value the runtime does not read in this row is written
/// as zero. A value of a feature the runtime can do without turns
/// that feature off for this build and is cleared, so the rest of the row is not held back by it.
/// Any other value stops the row with its reason, since no feature may run on a value of another
/// build.
fn settle_carried(row: &mut Value, report: &mut PortReport) {
    use crate::row_features::{self, Clear, RowFeature};
    let mut off: BTreeMap<&'static str, (&'static RowFeature, Vec<String>)> = BTreeMap::new();
    for (path, reason) in std::mem::take(&mut report.off_values) {
        if let Some(feature) = row_features::feature_of(&path) {
            row_features::clear_value(&mut row["build"], &path, feature);
            report.unread.insert(path.clone(), reason);
            off.entry(feature.name)
                .or_insert((feature, Vec::new()))
                .1
                .push(path);
        }
    }
    let must_check = report.must_check;
    let carried = if must_check {
        std::mem::take(&mut report.carried)
    } else {
        Vec::new()
    };
    let mut left = Vec::new();
    for path in carried {
        if row_features::is_reflected(&path) {
            row_features::leave_out(&mut row["build"], &path);
            report.unread.remove(&path);
            report.reflected.push(format!(
                "{path}: left out, the runtime finds it by reflection"
            ));
        } else if let Some(why) = row_features::unused(&row["build"], &path) {
            row_features::clear(&mut row["build"], &path, Clear::Zero);
            report.unused.push(format!("{path}: {why}"));
            report.unread.remove(&path);
        } else if let Some(feature) = row_features::feature_of(&path) {
            row_features::clear(&mut row["build"], &path, feature.clear);
            off.entry(feature.name)
                .or_insert((feature, Vec::new()))
                .1
                .push(path);
        } else {
            left.push(path);
        }
    }
    let mut entries = Vec::new();
    for (name, (_, paths)) in off {
        row_features::turn_off(&mut row["build"], name);
        let reasons: Vec<String> = paths
            .iter()
            .map(|path| match report.unread.remove(path) {
                Some(reason) => format!("{path} {reason}"),
                None => format!("{path} has no recipe"),
            })
            .collect();
        report
            .switched_off
            .push(format!("{name} is off: {}", reasons.join("; ")));
        entries.push(json!({"feature": name, "values": paths}));
    }
    if !entries.is_empty() {
        row[row_features::SWITCHED_OFF] = json!(entries);
    }
    if !must_check {
        return;
    }
    let mut unchecked = Vec::new();
    for path in &left {
        match report.unread.remove(path) {
            Some(reason) => report.review.push(format!("{path} {reason}")),
            None => unchecked.push(path.clone()),
        }
    }
    if !unchecked.is_empty() {
        let head = report
            .milestone
            .clone()
            .unwrap_or_else(|| "values were not read".to_owned());
        report.review.push(format!(
            "{head}: {} values have no recipe and were carried over unchecked: {}",
            unchecked.len(),
            unchecked.join(", ")
        ));
    }
    report.carried = left;
}

/// A copy of the prior row with the new version, and frame addresses in the `0x` form. `carries`
/// says the values with no recipe stay as the prior row has them, so a new milestone stops the
/// row; without it they are read from the new build later, each with its own review line.
fn begin_row(prior: &Value, version: &str, carries: bool) -> (Value, PortReport) {
    let mut row = prior.clone();
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
    write_command_ids(&mut row, version, &mut report);
    report.carried = crate::row_recipe::without_recipe(&prior["build"]);
    report.settings = crate::row_recipe::settings(&prior["build"]);
    let milestone_changed = prior_version.split('.').next() != version.split('.').next();
    report.must_check = !carries || milestone_changed;
    if carries && milestone_changed {
        report.milestone = Some(format!(
            "the milestone changed from {prior_version} to {version}"
        ));
    }
    row["build"]["version"] = json!(parts);
    row["frame"]["version"] = json!(version);

    let frame_as_addresses = frame_addresses(&row["frame"]);
    row["frame_addresses"] = frame_as_addresses;
    (row, report)
}

/// Writes Chromium's public command ids into a row from the table read from Chromium's source,
/// for runtimes that still read them from the row. They are never carried from the prior row: a
/// milestone the table does not know gets zero, which offers no command. A prior row with zero
/// never had the developer bar checked, so it keeps zero.
fn write_command_ids(row: &mut Value, version: &str, report: &mut PortReport) {
    let browser = row["build"]["browser"].as_str().unwrap_or_default();
    // Chrome, Edge and Brave carry their Chromium milestone as the first part of their version.
    let milestone = matches!(browser, "Google Chrome" | "Microsoft Edge" | "Brave")
        .then(|| version.split('.').next()?.parse::<u16>().ok())
        .flatten();
    let ids = milestone.and_then(crate::command_ids::dev_tools);
    let Some(dev) = row["build"]["toolbar"]["dev"].as_object_mut() else {
        return;
    };
    for (key, id) in [
        ("inspect", ids.map(|ids| ids.0)),
        ("console", ids.map(|ids| ids.1)),
    ] {
        let Some(prior) = dev.get(key).and_then(Value::as_u64) else {
            continue;
        };
        let value = if prior == 0 { 0 } else { id.unwrap_or(0) };
        dev.insert(key.to_owned(), json!(value));
        report.derived.push(match (prior == 0, id) {
            (true, _) => format!("toolbar.dev.{key}: 0, the prior row offers no developer bar"),
            (false, Some(id)) => format!(
                "toolbar.dev.{key}: {id}, from chrome_command_ids.h at Chromium {}",
                milestone.unwrap_or_default()
            ),
            (false, None) => format!(
                "toolbar.dev.{key}: 0, no command id is read for this build's Chromium milestone"
            ),
        });
    }
}

/// The prior row's functions that a browser command runs first, with the command's public id:
/// `chrome::NewTab`, the first call of the `IDC_NEW_TAB` case of the browser's command switch. The
/// id must be the same at the prior and the new build's Chromium milestone, which Chrome, Edge and
/// Brave carry as the first part of their version.
fn command_targets(prior: &Value, version: &str) -> BTreeMap<u32, i64> {
    let browser = prior["build"]["browser"].as_str().unwrap_or_default();
    if !matches!(browser, "Google Chrome" | "Microsoft Edge" | "Brave") {
        return BTreeMap::new();
    }
    let milestone = |version: &str| version.split('.').next()?.parse::<u16>().ok();
    let prior_version = prior["frame"]["version"].as_str().unwrap_or_default();
    let ids = (
        milestone(prior_version).and_then(crate::command_ids::new_tab),
        milestone(version).and_then(crate::command_ids::new_tab),
    );
    let (Some(id), Some(same)) = ids else {
        return BTreeMap::new();
    };
    let address =
        read_address(&prior["build"]["tabs"]["new_tab"]).filter(|value| is_address(*value));
    match address {
        Some(address) if id == same => BTreeMap::from([(address as u32, id as i64)]),
        _ => BTreeMap::new(),
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
    for failure in derived.failed {
        // A value of a feature the runtime can do without turns that feature off instead.
        let path = failure.split(": ").next().unwrap_or_default();
        if crate::row_features::feature_of(path).is_some() {
            report.off_values.insert(
                path.to_owned(),
                format!("could not be read from the new build: {failure}"),
            );
        } else {
            report.review.push(format!(
                "a value could not be read from the new build: {failure}"
            ));
        }
    }
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
/// milestone, from the new module alone. Each address must match exactly one place, no two addresses may
/// share one, and every function start and table slot is then checked again in the new module the
/// way a ported row is. Any doubt is an error with its reasons, and then the runtime changes
/// nothing in the browser.
pub fn find_row(prior: &Value, version: &str, pe: &PeFile) -> Result<Value, Vec<String>> {
    let Some(stored) = prior["fingerprints"].as_object() else {
        return Err(vec!["the row has no fingerprints".to_owned()]);
    };
    let (row, report) = find_candidate(prior, stored, None, &BTreeMap::new(), version, pe);
    if !report.review.is_empty() {
        return Err(report.review);
    }
    Ok(row)
}

/// Makes a candidate row for a new build from `stored`, the fingerprints of the prior row's
/// addresses in the prior build. An address is taken where its fingerprint matches exactly one
/// place. Offsets, slots and sizes with a recipe are read again from the new module.
///
/// `classes` names the views class of each class table of the prior row, as `prior_classes` reads
/// them. The new row records the names of the tables it placed, by the path of each value, so the
/// next row starts from them.
///
/// `prior_pe` is the prior row's own module, when the caller has it. A fingerprint taken at a
/// function start its exception table lists then counts only places the new module's exception
/// table also lists, since the same bytes inside another function are not the function. Each
/// address the fingerprints leave open is looked for by `row_resolve`, which takes a place only
/// where two independent methods agree, and each value with no recipe is read from the
/// instructions that use it. An address that stays open and that only features the runtime can do
/// without hold turns those features off and is cleared. Whatever else stays open keeps a review
/// line and its prior value, so such a row is never published as it is.
pub fn find_candidate(
    prior: &Value,
    stored: &serde_json::Map<String, Value>,
    prior_pe: Option<&PeFile>,
    classes: &BTreeMap<u32, String>,
    version: &str,
    pe: &PeFile,
) -> (Value, PortReport) {
    let (mut row, mut report) = begin_row(prior, version, prior_pe.is_none());
    let prior_starts = prior_pe.map(PeFile::function_starts).unwrap_or_default();
    let targets = fingerprint_targets(prior);
    report.addresses = targets.len();
    let mut functions = BTreeMap::new();
    let mut tables = BTreeMap::new();
    // Each address its fingerprint did not pin down, with its review line, waits for the methods
    // that read the prior build next to the new one.
    let mut wanted: BTreeMap<u32, (String, row_resolve::Wanted)> = BTreeMap::new();
    let commands = command_targets(prior, version);
    let want =
        |target: u32, kind: row_resolve::Kind, candidates: Option<Vec<u32>>| row_resolve::Wanted {
            kind,
            candidates,
            class: (kind == row_resolve::Kind::Table)
                .then(|| classes.get(&target).cloned())
                .flatten(),
            command: (kind == row_resolve::Kind::Function)
                .then(|| commands.get(&target).copied())
                .flatten(),
            loose: Vec::new(),
        };
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
                // Without the prior module no method can place it, but it is still settled with
                // the other open addresses, so a feature that is off does not stop the row.
                let kind = match prior_pe {
                    Some(prior_pe) if !prior_pe.is_code(*target) => row_resolve::Kind::Table,
                    _ => row_resolve::Kind::Function,
                };
                wanted.insert(*target, (line, want(*target, kind, None)));
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
                let mut asked = want(*target, function, None);
                asked.loose = all.to_vec();
                wanted.insert(*target, (line, asked));
            }
            [] => {
                let line = format!("{target:#x} matches no place");
                wanted.insert(*target, (line, want(*target, function, None)));
            }
            many => {
                let line = format!("{target:#x} matches {} places", many.len());
                wanted.insert(
                    *target,
                    (line, want(*target, function, Some(many.to_vec()))),
                );
            }
        }
    }
    for (target, (slot, holds)) in &tables {
        let table = row_resolve::Kind::Table;
        let Some(function) = moved.get(holds).copied() else {
            let line = format!("{target:#x}: the function in slot {slot} was not found");
            wanted.insert(*target, (line, want(*target, table, None)));
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
                wanted.insert(*target, (line, want(*target, table, candidates)));
            }
        }
    }
    let mut followed = BTreeMap::new();
    // Each address no method placed, with its review line. Those that only features the runtime
    // can do without hold do not stop the row.
    let mut open: BTreeMap<u32, String> = BTreeMap::new();
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
            // The tables a class name was confirmed for in the prior build are the tables of views
            // classes the slot shift compares.
            let views: BTreeSet<u32> = classes.keys().copied().collect();
            let resolution =
                row_resolve::resolve(prior_pe, pe, &moved, &asked, &table_targets, &views);
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
                        open.insert(*target, format!("{line}; not settled: {reason}"));
                    }
                }
            }
            report.settled.extend(resolution.settled);
            followed = resolution.followed;
        }
        _ => open.extend(wanted.into_iter().map(|(target, (line, _))| (target, line))),
    }
    release_unplaced(prior, &mut row, open, &mut report);
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
        object.remove(CLASSES);
    }
    record_classes(prior, &mut row, classes, &moved);
    if let Some(prior_pe) = prior_pe {
        read_carried(
            prior,
            &mut row,
            &moved,
            &followed,
            prior_pe,
            pe,
            &mut report,
        );
    }
    settle_carried(&mut row, &mut report);
    (row, report)
}

/// Settles each address no method placed in the new build. Where every value that holds it belongs
/// to a feature the runtime can do without, or is not read in this row, those values are cleared
/// and the features go off, and the address is listed in the report. An address that a part the
/// window cannot do without holds, the frame or the page area layout, stops the row with its line.
fn release_unplaced(
    prior: &Value,
    row: &mut Value,
    open: BTreeMap<u32, String>,
    report: &mut PortReport,
) {
    use crate::row_features::{self, Clear};
    if open.is_empty() {
        return;
    }
    let mut holders: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut note = |path: &str, value: &mut Value| {
        if READ_FROM_CODE.iter().any(|field| path.ends_with(field)) {
            return;
        }
        if let Some(address) = read_address(value).filter(|value| is_address(*value)) {
            holders
                .entry(address as u32)
                .or_default()
                .push(path.trim_start_matches('.').to_owned());
        }
    };
    let mut build = prior["build"].clone();
    walk_addresses("", &mut build, &mut note);
    // The frame part has no feature to turn off, so its paths always stop the row.
    let mut frame = frame_addresses(&prior["frame"]);
    walk_addresses("frame", &mut frame, &mut note);
    for (target, line) in open {
        let paths = holders.remove(&target).unwrap_or_default();
        let needed = paths.is_empty()
            || paths.iter().any(|path| {
                path.starts_with("frame.")
                    || (row_features::feature_of(path).is_none()
                        && row_features::unused(&row["build"], path).is_none())
            });
        if needed {
            report.review.push(line);
            continue;
        }
        for path in &paths {
            match row_features::feature_of(path) {
                Some(feature) => {
                    row_features::clear_value(&mut row["build"], path, feature);
                    report.off_values.insert(
                        path.clone(),
                        format!("has no place in the new build: {line}"),
                    );
                }
                None => {
                    let why = row_features::unused(&row["build"], path).unwrap_or_default();
                    // A list holds no single address, so a value in one becomes zero.
                    let how = if path.contains('[') {
                        Clear::Zero
                    } else {
                        Clear::Absent
                    };
                    row_features::clear(&mut row["build"], path, how);
                    report.unused.push(format!("{path}: {why}"));
                }
            }
        }
        report
            .unplaced
            .push(format!("{line}; cleared from {}", paths.join(", ")));
    }
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
/// that cannot be read keeps its reason and stays listed as carried, and `settle_carried` decides
/// what it does to the row, so the row is never published with a value of the prior build.
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
                    format!(
                        "{}: {:#x} -> {value:#x}, {how}",
                        reading.path, reading.prior
                    )
                });
            }
            Err(reason) => {
                report.unread.insert(
                    reading.path.clone(),
                    format!("was not read from the new build: {reason}"),
                );
                report.carried.push(reading.path);
            }
        }
    }
    for path in unreadable {
        report.unread.insert(
            path.clone(),
            "is not a number this generator can read".to_owned(),
        );
        report.carried.push(path);
    }
}

/// The key of a row that names the views class of each of its class tables, by the path of the
/// value that holds the table, as `{"build.layout.frame_vtable": "BrowserFrameViewWin"}`. The
/// runtime reads only `frame` and `build`, so the names change nothing in a browser.
pub const CLASSES: &str = "classes";

/// The path of every address a row holds, the frame entry's included, as `build.layout.frame_vtable`
/// or `frame.frame_view_vtable`.
fn address_paths(row: &Value) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let mut note = |path: &str, value: &mut Value| {
        if READ_FROM_CODE.iter().any(|field| path.ends_with(field)) {
            return;
        }
        if let Some(address) = read_address(value).filter(|value| is_address(*value)) {
            out.push((path.to_owned(), address as u32));
        }
    };
    let mut build = row["build"].clone();
    walk_addresses("build", &mut build, &mut note);
    let mut frame = frame_addresses(&row["frame"]);
    walk_addresses("frame", &mut frame, &mut note);
    out
}

/// The views class of each class table of the prior row, from the build alone. A name the prior
/// row records for the table is taken when it gives exactly that table in the prior module, and
/// a table with no such name is named by the class name its metadata holds, read back from the
/// prior module by `row_class`. Either way the name is the one the class's `BEGIN_METADATA` gives
/// in Chromium's source, and a table no name gives alone has none.
pub fn prior_classes(prior: &Value, prior_pe: &PeFile) -> BTreeMap<u32, String> {
    let recorded = prior[CLASSES].as_object();
    let mut out = BTreeMap::new();
    let mut tried = BTreeSet::new();
    for (path, address) in address_paths(prior) {
        if prior_pe.is_code(address) || out.contains_key(&address) {
            continue;
        }
        let name = recorded
            .and_then(|names| names.get(&path))
            .and_then(Value::as_str)
            .filter(|name| crate::row_class::names_table(prior_pe, name, address))
            .map(str::to_owned);
        let name = match name {
            Some(name) => Some(name),
            None if tried.insert(address) => {
                crate::row_class::class_name_of_table(prior_pe, address)
            }
            None => None,
        };
        if let Some(name) = name {
            out.insert(address, name);
        }
    }
    out
}

/// Writes into the new row the class name of each class table it placed, by the path of each value
/// that holds the table.
fn record_classes(
    prior: &Value,
    row: &mut Value,
    classes: &BTreeMap<u32, String>,
    moved: &BTreeMap<u32, u32>,
) {
    let mut names = serde_json::Map::new();
    for (path, address) in address_paths(prior) {
        if let Some(name) = classes
            .get(&address)
            .filter(|_| moved.contains_key(&address))
        {
            names.insert(path, Value::String(name.clone()));
        }
    }
    if !names.is_empty() {
        row[CLASSES] = Value::Object(names);
    }
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

/// Leaves `screen_origin`, `[address, first bytes]` of `views::View::ConvertPointToScreen`, out of a
/// row whose prior row had none. A prior row that had it was found again with every other address.
/// The runtime then keeps placing bubbles from the window's monitor, so the row is not stopped.
pub fn leave_out_screen_origin(row: &mut Value, report: &mut PortReport) {
    if row["build"]["screen_origin"].is_array() {
        return;
    }
    row["build"]["screen_origin"] = Value::Null;
    report
        .left_out
        .push("build.screen_origin: the prior row has none".to_owned());
}

/// The group a value of a row is scored in: its feature, or the window, whose parts cannot be off.
const WINDOW: &str = "window";

/// How one feature of a row made without debug symbols compares with the verified row of the same
/// build, value by value.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct FeatureScore {
    /// Values the made row holds exactly as the verified row does.
    pub equal: usize,
    /// Values the made row holds differently, with both. Any is a wrong value.
    pub different: Vec<String>,
    /// Values no method placed in the new build, so the made row still shows the prior build's and
    /// a review line stops the row. Not a wrong value: the row is not published.
    pub not_found: Vec<String>,
    /// Values the made row holds exactly as the prior row did although the verified row differs:
    /// nothing read them again. Wrong only in a row no review line stops.
    pub carried: Vec<String>,
    /// The made row turns the feature off, so none of its values is used.
    pub off: bool,
    /// Values the made row does not hold or does not use: the runtime finds them by reflection,
    /// does not read them in this build, or the prior row did not have them.
    pub not_held: Vec<String>,
}

/// The score of a made row against its answer key: each feature's comparison, and the review lines
/// that stop the row.
#[derive(Debug, Default, Serialize)]
pub struct RowScore {
    pub browser: String,
    pub version: String,
    pub prior_version: String,
    pub features: BTreeMap<String, FeatureScore>,
    pub review: Vec<String>,
}

impl RowScore {
    /// How many values were found but differ from the verified row, with the values carried from
    /// the prior build when no review line stops the row. Must be zero.
    pub fn wrong(&self) -> usize {
        self.features
            .values()
            .map(|score| match self.review.is_empty() {
                true => score.different.len() + score.carried.len(),
                false => score.different.len(),
            })
            .sum()
    }
}

/// The address a value was read at: the function of a prologue, or the first item of a pair.
fn read_at(path: &str) -> Option<String> {
    if let Some(function) = path.strip_suffix("_prologue") {
        return Some(function.to_owned());
    }
    path.strip_suffix("[1]").map(|pair| format!("{pair}[0]"))
}

/// Every value a row part holds that is not a list or object, by its path.
fn leaves(path: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                leaves(&next, item, out);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                leaves(&format!("{path}[{index}]"), item, out);
            }
        }
        _ => {
            out.insert(path.to_owned(), value.clone());
        }
    }
}

/// Compares a row made from a prior row with the row people verified for the same build. A value of
/// a feature the made row turns off is not compared, since the runtime does not use it. A value the
/// made row leaves out, or writes as zero because this build does not read it, is listed as not
/// held. A value a review line reports as having no new place is listed as not found. Every other
/// value must be equal, and one that differs is listed with both values.
pub fn score_row(made: &Value, report: &PortReport, verified: &Value, prior: &Value) -> RowScore {
    let off: BTreeSet<&str> = made[crate::row_features::SWITCHED_OFF]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|entry| entry["feature"].as_str())
        .collect();
    let unused: BTreeSet<&str> = report
        .unused
        .iter()
        .chain(&report.reflected)
        .filter_map(|line| line.split(": ").next())
        .collect();
    let unplaced: BTreeSet<String> = report
        .review
        .iter()
        .filter_map(|line| line.strip_prefix("no new place for ."))
        .filter_map(|rest| rest.split(' ').next())
        .map(|path| match path.starts_with("frame.") {
            true => path.to_owned(),
            false => format!("build.{path}"),
        })
        .collect();
    let mut score = RowScore {
        browser: report.browser.clone(),
        version: report.version.clone(),
        prior_version: report.prior_version.clone(),
        review: report.review.clone(),
        ..RowScore::default()
    };
    for part in ["frame", "build"] {
        let (mut want, mut have, mut before) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        leaves("", &verified[part], &mut want);
        leaves("", &prior[part], &mut before);
        leaves("", &made[part], &mut have);
        for (path, value) in want {
            let feature = match part {
                "build" => {
                    crate::row_features::feature_of(&path).map_or(WINDOW, |feature| feature.name)
                }
                _ => WINDOW,
            };
            let entry = score.features.entry(feature.to_owned()).or_default();
            if off.contains(feature) {
                entry.off = true;
                continue;
            }
            let shown = format!("{part}.{path}");
            match have.get(&path) {
                Some(found) if *found == value => entry.equal += 1,
                None | Some(Value::Null) => entry.not_held.push(shown),
                Some(_) if unused.iter().any(|unused| path.starts_with(unused)) => {
                    entry.not_held.push(shown)
                }
                Some(_) if unplaced.contains(&shown) => entry.not_found.push(shown),
                // Bytes read at an address with no new place say nothing about the new build.
                Some(_) if read_at(&shown).is_some_and(|at| unplaced.contains(&at)) => {
                    entry.not_found.push(shown)
                }
                // A part the generator could not confirm is declined, which is not a wrong value.
                Some(Value::Bool(false)) => entry.not_found.push(shown),
                Some(found) if before.get(&path) == Some(found) => entry
                    .carried
                    .push(format!("{shown}: carried {found}, verified {value}")),
                Some(found) => entry
                    .different
                    .push(format!("{shown}: made {found}, verified {value}")),
            }
        }
    }
    score
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

    fn pointer(rva: u32) -> [u8; 8] {
        (IMAGE_BASE + u64::from(rva)).to_le_bytes()
    }

    /// Places the prior row's addresses where `moved` says, as the fingerprints and `row_resolve`
    /// would, and settles the rest of the row the way every new row is settled.
    fn port(
        prior: &Value,
        moved: &[(u32, u32)],
        code: &[(u32, &[u8])],
        version: &str,
    ) -> (Value, PortReport) {
        let bytes = module(code);
        let pe = PeFile::parse(&bytes).unwrap();
        let (row, mut report) = begin_row(prior, version, true);
        report.addresses = addresses_of(&row["build"]).len();
        let moved: BTreeMap<u32, u32> = moved.iter().copied().collect();
        let (mut row, mut report) = finish_row(row, &moved, &pe, report);
        settle_carried(&mut row, &mut report);
        (row, report)
    }

    const NEW_PLACES: &[(u32, u32)] = &[
        (0x200000, 0x101000),
        (0x200100, 0x101100),
        (0x200200, 0x101200),
        (0x200300, 0x101300),
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
    fn nfr3_a_new_row_takes_the_found_places_and_bytes_from_the_new_module() {
        let code = new_code();
        let (row, report) = port(&prior(), NEW_PLACES, &code_refs(&code), "1.0.0.2");
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
    fn nfr3_an_address_with_no_new_place_stops_the_row() {
        let code = new_code();
        let places: Vec<_> = NEW_PLACES
            .iter()
            .filter(|(prior, _)| *prior != 0x200300)
            .copied()
            .collect();
        let (_, report) = port(&prior(), &places, &code_refs(&code), "1.0.0.2");
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
    fn nfr6_a_command_id_is_not_a_row_value() {
        let mut prior = prior();
        prior["build"]["browser"] = json!("Google Chrome");
        prior["frame"]["version"] = json!("157.0.8103.39");
        // A wrong prior value shows that the new row does not copy it.
        prior["build"]["toolbar"]["dev"] = json!({"inspect": 1, "console": 2, "colours": []});
        let code = new_code();
        let (row, report) = port(&prior, NEW_PLACES, &code_refs(&code), "158.0.8127.36");
        assert_eq!(row["build"]["toolbar"]["dev"]["inspect"], 40023);
        assert_eq!(row["build"]["toolbar"]["dev"]["console"], 40005);
        assert!(
            !report
                .carried
                .iter()
                .any(|path| path.starts_with("toolbar.dev.inspect")
                    || path.starts_with("toolbar.dev.console")),
            "{:?}",
            report.carried
        );
        let milestone = report
            .review
            .iter()
            .find(|line| line.contains("milestone"))
            .expect("the other values still cross the milestone");
        assert!(!milestone.contains("toolbar.dev.console"), "{milestone}");
        // A milestone whose header was not read offers no command rather than the prior one.
        let (row, _) = port(&prior, NEW_PLACES, &code_refs(&code), "199.0.1.1");
        assert_eq!(row["build"]["toolbar"]["dev"]["inspect"], 0);
        // A row that never offered the developer bar keeps offering none.
        prior["build"]["toolbar"]["dev"] = json!({"inspect": 0, "console": 0, "colours": []});
        let (row, _) = port(&prior, NEW_PLACES, &code_refs(&code), "158.0.8127.36");
        assert_eq!(row["build"]["toolbar"]["dev"]["console"], 0);
    }

    /// A prior row whose values that are not addresses all belong to features the runtime can do
    /// without, or are not read in this row.
    fn prior_with_features() -> Value {
        let mut prior = prior();
        let toolbar = prior["build"]["toolbar"].as_object_mut().unwrap();
        toolbar.remove("view_layer");
        toolbar.remove("colours");
        toolbar.insert(
            "dev".to_owned(),
            json!({"colours": [[321, 4280953905u32], [1597, 0]]}),
        );
        prior["build"]["split"] = json!({"supported": true, "outline_member": "0x40"});
        prior["build"]["omnibox"] = json!({"match_size": "0x820"});
        prior["build"]["tabs"]["tab_at"] = json!("0x200300");
        prior["build"]["tabs"]["tab_collection"] = json!("0x30");
        prior
    }

    #[test]
    fn nfr6_a_feature_with_an_unread_value_is_off_for_that_build() {
        let code = new_code();
        let (row, report) = port(
            &prior_with_features(),
            NEW_PLACES,
            &code_refs(&code),
            "2.0.0.1",
        );
        // Nothing holds the rest of the row back.
        assert_eq!(report.review, Vec::<String>::new());
        assert!(report.carried.is_empty(), "{:?}", report.carried);
        let build = &row["build"];
        // Each feature with a value of the prior build is off, and the value is gone.
        assert_eq!(build["split"]["supported"], false);
        assert_eq!(build["split"]["outline_member"], "0x0");
        assert!(build["omnibox"].is_null());
        assert_eq!(build["toolbar"]["dev"]["colours"], json!([]));
        let off: Vec<&str> = row[crate::row_features::SWITCHED_OFF]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["feature"].as_str().unwrap())
            .collect();
        assert_eq!(
            off,
            ["address suggestions", "developer colours", "split view"]
        );
        assert_eq!(report.switched_off.len(), 3, "{:?}", report.switched_off);
        // A value this build does not use is written as zero, and no feature goes off for it.
        assert_eq!(build["tabs"]["tab_collection"], "0x0");
        assert_eq!(report.unused.len(), 1, "{:?}", report.unused);
        // The addresses of the row still move to the new build.
        assert_eq!(build["tabs"]["tab_at"], "0x101300");
    }

    #[test]
    fn nfr6_a_row_without_window_offsets_is_not_blocked() {
        // Edge 157's row stopped on exactly these values. The runtime finds them by class name and
        // the view tree, so the new row leaves them out and nothing holds it back.
        let mut prior = prior_with_features();
        prior["build"]["layout"] = json!({"frame_browser_window_offset": "0x3b0"});
        prior["build"]["side_panel"] =
            json!({"host_view": "0x38", "page_area": "0x0", "contents_container": "0xd8"});
        prior["build"]["toolbar"]["get_widget_slot"] = json!("0xa");
        let code = new_code();
        let (row, report) = port(&prior, NEW_PLACES, &code_refs(&code), "2.0.0.1");
        assert_eq!(report.review, Vec::<String>::new());
        assert!(report.carried.is_empty(), "{:?}", report.carried);
        assert_eq!(report.reflected.len(), 5, "{:?}", report.reflected);
        let build = &row["build"];
        assert!(build["layout"].get("frame_browser_window_offset").is_none());
        assert_eq!(build["side_panel"], json!({}));
        assert!(build["toolbar"].get("get_widget_slot").is_none());
        // No feature went off for them.
        assert!(!report
            .switched_off
            .iter()
            .any(|line| line.contains("side_panel.") || line.contains("get_widget_slot")));
    }

    #[test]
    fn nfr6_a_value_the_window_cannot_do_without_still_stops_the_row() {
        let mut prior = prior_with_features();
        prior["build"]["layout"] = json!({"view_bounds_offset": "0x1c8"});
        let code = new_code();
        let (row, report) = port(&prior, NEW_PLACES, &code_refs(&code), "2.0.0.1");
        assert_eq!(report.carried, ["layout.view_bounds_offset"]);
        assert!(
            report.review.iter().any(|line| line.contains("milestone")
                && line.ends_with("carried over unchecked: layout.view_bounds_offset")),
            "{:?}",
            report.review
        );
        // The features it could do without are still off.
        assert_eq!(row["build"]["split"]["supported"], false);
    }

    #[test]
    fn nfr6_a_row_within_its_milestone_turns_nothing_off() {
        let code = new_code();
        let (row, report) = port(
            &prior_with_features(),
            NEW_PLACES,
            &code_refs(&code),
            "1.0.0.2",
        );
        assert_eq!(report.review, Vec::<String>::new());
        assert!(report.switched_off.is_empty());
        assert_eq!(row["build"]["split"]["supported"], true);
        assert!(row.get(crate::row_features::SWITCHED_OFF).is_none());
    }

    #[test]
    fn nfr3_a_row_that_crosses_a_milestone_stops_for_review() {
        let code = new_code();
        let (_, report) = port(&prior(), NEW_PLACES, &code_refs(&code), "2.0.0.1");
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
        let (_, report) = port(&prior(), NEW_PLACES, &code_refs(&code), "1.0.0.2");
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
    fn nfr3_a_guard_a_hook_copies_stops_on_an_instruction_in_the_new_build() {
        let mut prior = prior();
        // Fifteen bytes that end on an instruction, and a new start where byte fifteen splits one.
        prior["build"]["toolbar"]["paint_prologue"] = json!("41574156565755534881ecb8000000");
        let mut code = new_code();
        code[2].1 = hex("4156565755534881eca00000004889ce0000").unwrap();
        let (row, report) = port(&prior, NEW_PLACES, &code_refs(&code), "1.0.0.2");
        assert_eq!(
            row["build"]["toolbar"]["paint_prologue"],
            "4156565755534881eca0000000"
        );
        assert!(report.review.is_empty(), "{:?}", report.review);
    }

    #[test]
    fn nfr3_addresses_are_told_from_offsets_and_the_absent_marker() {
        let row = json!({"a": "0x10", "b": "0xfffff", "c": "0x100000", "d": ["0x7fff0000", "0x4fb01f0"], "e": 1234567, "f": "0xabcd_not"});
        assert_eq!(addresses_of(&row), BTreeSet::from([0x100000, 0x4fb01f0]));
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

    fn no_classes() -> BTreeMap<u32, String> {
        BTreeMap::new()
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
        find_candidate(
            &verified_row(),
            prints,
            None,
            &BTreeMap::new(),
            version,
            &pe,
        )
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
        let classes = BTreeMap::new();
        find_candidate(
            &verified_row(),
            &prints,
            Some(&prior_pe),
            &classes,
            "1.0.0.3",
            &pe,
        )
    }

    #[test]
    fn nfr6_a_carried_value_is_read_from_the_instructions_that_use_it() {
        // mov rax, [rcx + 0x20]; mov rdx, [rcx + 0x20], and the new build moved the member to 0x28.
        let (row, report) = read_view_layer(
            "488b8120000000488b9120000000",
            "488b8128000000488b9128000000",
        );
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
        let (_, report) = read_view_layer(
            "488b8120000000488b9120000000",
            "488b8128000000488b9130000000",
        );
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
        let (row, report) = find_candidate(
            &verified,
            &prints,
            Some(&prior_pe),
            &no_classes(),
            "1.0.0.3",
            &pe,
        );
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
        let (row, report) = find_candidate(
            &verified,
            &prints,
            Some(&prior_pe),
            &no_classes(),
            "1.0.0.3",
            &pe,
        );
        assert!(report.review.is_empty(), "{:?}", report.review);
        assert_eq!(row["build"]["toolbar"]["paint"], "0x103300");
        // Bytes that match only inside another function are no place at all.
        let bytes = with_starts(&code, &[0x103100, 0x103200]);
        let pe = PeFile::parse(&bytes).unwrap();
        let (_, report) = find_candidate(
            &verified,
            &prints,
            Some(&prior_pe),
            &no_classes(),
            "1.0.0.3",
            &pe,
        );
        assert!(
            report.review.iter().any(|line| line
                .starts_with("0x101300 matches no function start, of 2 places; not settled:")),
            "{:?}",
            report.review
        );
    }

    #[test]
    fn nfr6_a_new_build_gets_a_row_from_the_prior_rows_fingerprints_alone() {
        let (row, report) = candidate(&prints_of_prior(), &shifted_code(0x2000, 0x44), "1.0.0.3");
        assert!(report.review.is_empty(), "{:?}", report.review);
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
    fn nfr6_unplaced_functions_of_a_feature_that_is_off_do_not_block_the_row() {
        // Edge 157's row stopped on functions no method placed, most of which only features the
        // runtime can do without hold. The prior build gave these two no fingerprint.
        let mut prior = verified_row();
        prior["build"]["split"] = json!({
            "supported": true,
            "remove_split": "0x101400",
            "remove_split_prologue": "4883ec28488b0d10000000",
            "signatures": [["0x101400", "4883ec28"]],
        });
        prior["build"]["vertical"] = json!({
            "detour_strip": false,
            "should_draw_tab_strip": "0x101500",
            "should_draw_tab_strip_prologue": "4883ec28",
        });
        let bytes = module(&code_refs(&shifted_code(0x2000, 0x44)));
        let pe = PeFile::parse(&bytes).unwrap();
        let prints = prints_of_prior();
        let (row, report) = find_candidate(&prior, &prints, None, &no_classes(), "1.0.0.3", &pe);
        assert_eq!(report.review, Vec::<String>::new());
        assert_eq!(report.unplaced.len(), 2, "{:?}", report.unplaced);
        assert!(report.unplaced[0].starts_with("0x101400: no fingerprint"));
        assert!(report.unplaced[0].ends_with("split.remove_split, split.signatures[0][0]"));
        let build = &row["build"];
        // The feature is off and holds nothing of the prior build.
        assert_eq!(build["split"]["supported"], false);
        assert_eq!(build["split"]["remove_split"], "0x0");
        assert_eq!(build["split"]["signatures"][0][0], "0x0");
        assert_eq!(
            row[crate::row_features::SWITCHED_OFF][0]["feature"],
            "split view"
        );
        assert!(
            report.switched_off[0]
                .starts_with("split view is off: split.remove_split has no place"),
            "{:?}",
            report.switched_off
        );
        // A function this build does not read is absent, and turns nothing off.
        assert_eq!(build["vertical"]["should_draw_tab_strip"], "0x7fff0000");
        assert_eq!(report.switched_off.len(), 1, "{:?}", report.switched_off);
        // The rest of the row is found.
        assert_eq!(build["toolbar"]["paint"], "0x103300");

        // The same address held by a part the window cannot do without still stops the row.
        prior["build"]["layout"] = json!({"frame_vtable": "0x101400"});
        let prints = prints_of_prior();
        let (row, report) = find_candidate(&prior, &prints, None, &no_classes(), "1.0.0.3", &pe);
        assert!(
            report
                .review
                .iter()
                .any(|line| line == "0x101400: no fingerprint"),
            "{:?}",
            report.review
        );
        assert!(report
            .unplaced
            .iter()
            .all(|line| !line.contains("0x101400")));
        assert_eq!(row["build"]["split"]["supported"], true);
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

    #[test]
    fn nfr3_a_prior_row_without_screen_origin_gives_a_row_without_it() {
        let mut row = prior();
        let mut report = PortReport::default();
        leave_out_screen_origin(&mut row, &mut report);
        assert!(row["build"]["screen_origin"].is_null());
        assert_eq!(report.left_out.len(), 1);
        assert!(report.review.is_empty());
        // A row that has it keeps the place it was found at.
        row["build"]["screen_origin"] = json!(["0x101400", "5657"]);
        leave_out_screen_origin(&mut row, &mut report);
        assert_eq!(row["build"]["screen_origin"][0], "0x101400");
    }

    #[test]
    fn nfr6_a_new_row_records_the_class_names_of_the_tables_it_placed() {
        let mut prior = verified_row();
        // A name the prior row recorded is not carried: only names confirmed now are written.
        prior[CLASSES] = json!({"build.toolbar.stale": "Stale"});
        let classes = BTreeMap::from([(0x101000, "BrowserFrameViewWin".to_owned())]);
        let bytes = module(&code_refs(&shifted_code(0x2000, 0x44)));
        let pe = PeFile::parse(&bytes).unwrap();
        let (row, report) =
            find_candidate(&prior, &prints_of_prior(), None, &classes, "1.0.0.3", &pe);
        assert!(report.review.is_empty(), "{:?}", report.review);
        assert_eq!(
            row[CLASSES],
            json!({"frame.frame_view_vtable": "BrowserFrameViewWin"})
        );
        // With no confirmed name the row records none.
        let (row, _) = find_candidate(
            &prior,
            &prints_of_prior(),
            None,
            &BTreeMap::new(),
            "1.0.0.3",
            &pe,
        );
        assert!(row.get(CLASSES).is_none());
    }

    #[test]
    fn nfr6_every_address_of_a_row_is_known_by_its_path() {
        let paths = address_paths(&verified_row());
        assert!(paths.contains(&("frame.frame_view_vtable".to_owned(), 0x101000)));
        assert!(paths.contains(&("build.toolbar.paint".to_owned(), 0x101300)));
        assert!(paths.contains(&("build.tabs.signatures[0][0]".to_owned(), 0x101300)));
        // Offsets and the absent marker are not addresses.
        assert!(!paths.iter().any(|(path, _)| path.ends_with("view_layer")));
        assert!(!paths.iter().any(|(path, _)| path.ends_with("missing")));
    }

    #[test]
    fn nfr6_a_made_row_is_scored_value_by_value_against_the_verified_row() {
        let (made, report) = candidate(&prints_of_prior(), &shifted_code(0x2000, 0x44), "1.0.0.3");
        let mut verified = made.clone();
        let score = score_row(&made, &report, &verified, &json!({}));
        assert_eq!(score.wrong(), 0);
        assert!(score.features[WINDOW].equal > 0);
        // A value found at another place than the verified one is wrong.
        verified["build"]["toolbar"]["paint"] = json!("0x103310");
        let score = score_row(&made, &report, &verified, &json!({}));
        assert_eq!(score.wrong(), 1, "{:?}", score.features);
        let wrong: Vec<&String> = score.features.values().flat_map(|f| &f.different).collect();
        assert!(
            wrong[0].starts_with("build.toolbar.paint: made"),
            "{wrong:?}"
        );
        // A value a review line reports as unplaced is not found, not wrong.
        let (_, mut stopped) =
            candidate(&prints_of_prior(), &shifted_code(0x2000, 0x44), "1.0.0.3");
        stopped
            .review
            .push("no new place for .toolbar.paint 0x101300".into());
        let score = score_row(&made, &stopped, &verified, &json!({}));
        assert_eq!(score.wrong(), 0, "{:?}", score.features);
        assert_eq!(score.features[WINDOW].not_found, ["build.toolbar.paint"]);
        // Bytes read at that address are not found either.
        let mut bytes = verified.clone();
        bytes["build"]["toolbar"]["paint_prologue"] = json!("4883ec28");
        let mut read = made.clone();
        read["build"]["toolbar"]["paint_prologue"] = json!("cccccccc");
        let score = score_row(&read, &stopped, &bytes, &json!({}));
        assert_eq!(score.wrong(), 0, "{:?}", score.features);
        // A value kept from the prior row is wrong only when nothing stops the row.
        let mut kept = verified.clone();
        kept["build"]["toolbar"]["paint"] = made["build"]["toolbar"]["paint"].clone();
        kept["build"]["toolbar"]["slot"] = json!(7);
        let mut carried = made.clone();
        carried["build"]["toolbar"]["slot"] = json!(8);
        let before = json!({"build": {"toolbar": {"slot": 8}}});
        assert_eq!(score_row(&carried, &report, &kept, &before).wrong(), 1);
        assert_eq!(score_row(&carried, &stopped, &kept, &before).wrong(), 0);
        // A value the made row does not hold is not wrong.
        let mut made_less = made.clone();
        made_less["build"]["toolbar"]
            .as_object_mut()
            .unwrap()
            .remove("paint");
        let score = score_row(&made_less, &report, &verified, &json!({}));
        assert_eq!(score.wrong(), 0);
        // A feature the made row turns off is not compared at all.
        let mut off = made.clone();
        off[crate::row_features::SWITCHED_OFF] = json!([{"feature": "split view", "values": []}]);
        let mut split = verified.clone();
        split["build"]["split"] = json!({"supported": true, "outline_member": "0x40"});
        off["build"]["split"] = json!({"supported": false, "outline_member": "0x0"});
        let score = score_row(&off, &report, &split, &json!({}));
        assert!(score.features["split view"].off);
        assert!(score.features["split view"].different.is_empty());
    }
}
