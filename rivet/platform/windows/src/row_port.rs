//! Makes the supported-build row of a new browser build from the row of a prior build.
//!
//! Chromium's source changes little between two builds of a milestone, but every address moves.
//! So each address of the prior row is named through the prior build's public symbols, the same
//! decorated names are looked up in the new build's symbols, and the instruction bytes that guard
//! each hook are read again from the new module. Values that are not addresses (member offsets,
//! colour ids, layout constants) are carried over. They have no public symbols, so a row that
//! crosses a milestone is never published without a person's review, and every row must still pass
//! on the real browser.
//!
//! A row is plain data here, in the form the runtime writes: a `usize` is a `0x` string, and bytes
//! are bare hex. A value this code cannot explain stops the row: it goes to review rather than to
//! people's browsers, and the runtime's own prologue checks stand behind that.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::Serialize;
use serde_json::{json, Value};

use crate::chrome_build::{hex, PeFile};

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

/// A function's first instructions, read as far as this needs them.
pub mod x64 {
    /// One decoded instruction: its length and where its RIP-relative displacement or relative
    /// branch target is, if it has one.
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct Instruction {
        pub length: usize,
        pub relative: Option<(usize, usize)>,
    }

    fn modrm(bytes: &[u8], at: usize, immediate: usize) -> Option<Instruction> {
        let byte = *bytes.get(at)?;
        let (mode, rm) = (byte >> 6, byte & 7);
        let mut length = at + 1;
        let mut relative = None;
        if mode != 3 && rm == 4 {
            let sib = *bytes.get(length)?;
            length += 1;
            if mode == 0 && sib & 7 == 5 {
                length += 4;
            }
        }
        match mode {
            0 if rm == 5 => {
                relative = Some((length, 4));
                length += 4;
            }
            1 => length += 1,
            2 => length += 4,
            _ => {}
        }
        Some(Instruction {
            length: length + immediate,
            relative,
        })
    }

    /// Decodes the instruction at the start of `bytes`, or `None` for one this does not know. The
    /// list covers what function starts are made of, and anything else goes to a person.
    pub fn decode(bytes: &[u8]) -> Option<Instruction> {
        let mut at = 0;
        let mut wide_operand = false;
        let mut rex_w = false;
        loop {
            match *bytes.get(at)? {
                0x66 => wide_operand = true,
                0xF2 | 0xF3 => {}
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
        let plain = |length| {
            Some(Instruction {
                length,
                relative: None,
            })
        };
        match opcode {
            0x50..=0x5F | 0x90 | 0xC3 | 0xCC | 0x99 => plain(at),
            0x98 => plain(at),
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
                plain(at + 1)
            }
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D | 0x68 | 0xA9 => plain(at + word),
            0xB8..=0xBF => plain(at + if rex_w { 8 } else { word }),
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
            0x70..=0x7F | 0xEB => Some(Instruction {
                length: at + 1,
                relative: Some((at, 1)),
            }),
            0xE8 | 0xE9 => Some(Instruction {
                length: at + 4,
                relative: Some((at, 4)),
            }),
            0x0F => {
                let second = *bytes.get(at)?;
                at += 1;
                match second {
                    0x80..=0x8F => Some(Instruction {
                        length: at + 4,
                        relative: Some((at, 4)),
                    }),
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
                    _ => None,
                }
            }
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
fn whole_function(pe: &PeFile, address: u32) -> Option<Vec<u8>> {
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
        version: new.version.to_owned(),
        prior_version: prior_version.clone(),
        ..PortReport::default()
    };
    let parts: Vec<u16> = new
        .version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect();
    if parts.len() != 4 {
        report
            .review
            .push(format!("{} is not a four part version", new.version));
    }
    if prior_version.split('.').next() != new.version.split('.').next() {
        report.review.push(format!(
            "the milestone changed from {prior_version} to {}: member offsets, colour ids and layout constants have no public symbols, so they were carried over unchecked",
            new.version
        ));
    }
    row["build"]["version"] = json!(parts);
    row["frame"]["version"] = json!(new.version);

    let frame_as_addresses = frame_addresses(&row["frame"]);
    row["frame_addresses"] = frame_as_addresses;

    // Addresses.
    let mut moved: BTreeMap<u32, u32> = BTreeMap::new();
    let targets = addresses_of(&row);
    report.addresses = targets.len();
    for target in &targets {
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
    read_rounded_corners(&mut row["build"], new.pe, &mut report);
    refresh_guards("build", &mut row["build"], new.pe, &mut report);
    refresh_frame_guard(&mut row["frame"], new.pe, &mut report);
    check_vtable_slots(&mut row["build"], new.pe, &mut report);
    check_frame_slot(&mut row["frame"], new.pe, &mut report);
    (row, report)
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
    if pe.pointer_rva(vtable + slot * 8) != Some(function) {
        report.review.push(format!(
            "the frame vtable {vtable:#x} slot {slot} does not hold {function:#x}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
