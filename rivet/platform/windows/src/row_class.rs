//! Finds the class table of a views class in a build by the class name its `ui::metadata` holds.
//!
//! Chromium's views classes declare their metadata with `METADATA_HEADER` and `BEGIN_METADATA`
//! (`ui/base/metadata/metadata_header_macros.h` and `metadata_impl_macros.h`). `MetaData()` keeps
//! the class's `ClassMetaData` in a static pointer that code holding the class name fills on first
//! use, and `GetClassMetaData`, the second slot of the class's `ui::metadata::MetaDataProvider`
//! table, returns it, directly or through a jump to `MetaData()`. A constructor of the class writes
//! that table into the object next to the class's own table, which sits at the object's start
//! because `views::View` names `ui::LayerDelegate` as its first base (`ui/views/view.h`).
//!
//! So, from the name: the code that uses the text and its callers, the static pointer the first of
//! them to load one loads, every function that loads the same pointer as its result, the tables
//! whose second slot holds one of them, and the table the constructors that write such a table
//! write at offset zero of the same object. The load is read the way the runtime's
//! `chromium/reflect.rs` reads it, so the generator and the runtime agree on the static a table
//! names. Nothing is guessed: a step that finds nothing, or more than one table, gives no table.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::chrome_build::PeFile;
use crate::row_port::x64;

/// How many levels of callers above the code that uses the name are read.
const CALLER_DEPTH: usize = 3;
/// How many functions the callers may grow to before the name counts as too common to follow.
const CALLERS_MOST: usize = 64;
/// How many jumps a `GetClassMetaData` thunk may take to the function that loads the static.
const THUNK_DEPTH: usize = 3;
/// How far a constructor is read.
const BODY_MOST: u32 = 0x4000;

/// Where the code at `at` gets its result: the target of its first `lea rax, [rip+d]`, or the word
/// its first `mov rax, [rip+d]` reads, after up to three jumps that may first move `this` with
/// `sub rcx, n` or `add rcx, n`. The same reading as the runtime's `reflect::load_target`.
pub fn load_target(pe: &PeFile, at: u32) -> Option<(u32, bool)> {
    let mut code_at = at;
    for _ in 0..3 {
        let head = pe.read(code_at, 12)?;
        let skip = match head[..3] {
            [0x48, 0x83, 0xE9] | [0x48, 0x83, 0xC1] => 4,
            [0x48, 0x81, 0xE9] | [0x48, 0x81, 0xC1] => 7,
            _ => 0,
        };
        if head[skip] != 0xE9 {
            break;
        }
        let rel = i32::from_le_bytes(head[skip + 1..skip + 5].try_into().ok()?);
        code_at = u32::try_from(i64::from(code_at) + skip as i64 + 5 + i64::from(rel)).ok()?;
    }
    let code = pe.read(code_at, 128)?;
    let at = (0..code.len() - 7).find(|&i| {
        code[i] == 0x48 && matches!(code[i + 1], 0x8D | 0x8B) && code[i + 2] == 0x05
    })?;
    let rel = i32::from_le_bytes(code[at + 3..at + 7].try_into().ok()?);
    let target = u32::try_from(i64::from(code_at) + at as i64 + 7 + i64::from(rel)).ok()?;
    Some((target, code[at + 1] == 0x8B))
}

/// Every place outside the code that holds `name` as a zero-terminated text.
fn texts(pe: &PeFile, name: &str) -> BTreeSet<u32> {
    let mut needle = name.as_bytes().to_vec();
    needle.push(0);
    let mut found = BTreeSet::new();
    for (base, bytes, is_code) in pe.sections() {
        if is_code || bytes.len() < needle.len() {
            continue;
        }
        for (at, window) in bytes.windows(needle.len()).enumerate() {
            if window[0] == needle[0] && window == needle.as_slice() {
                found.insert(base + at as u32);
            }
        }
    }
    found
}

/// A `lea` or `mov` of a 64-bit register with a RIP-relative operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RipUse {
    at: u32,
    target: u32,
    register: u8,
    /// A `mov`, which reads the word at the target, rather than a `lea`.
    word: bool,
}

fn rip_use(bytes: &[u8], rva: u32) -> Option<RipUse> {
    let [rex, op, modrm, ..] = *bytes else {
        return None;
    };
    if rex & 0xF8 != 0x48 || !matches!(op, 0x8D | 0x8B) || modrm & 0xC7 != 0x05 {
        return None;
    }
    let rel = i32::from_le_bytes(bytes.get(3..7)?.try_into().ok()?);
    Some(RipUse {
        at: rva,
        target: u32::try_from(i64::from(rva) + 7 + i64::from(rel)).ok()?,
        register: (((rex >> 2) & 1) << 3) | ((modrm >> 3) & 7),
        word: op == 0x8B,
    })
}

/// Every RIP-relative `lea` or `mov` in the code whose target is one of `targets`.
fn rip_uses(pe: &PeFile, targets: &BTreeSet<u32>) -> Vec<RipUse> {
    let mut found = Vec::new();
    for (base, bytes, is_code) in pe.sections() {
        if !is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(7) {
            if let Some(found_use) = rip_use(&bytes[at..at + 7], base + at as u32) {
                if targets.contains(&found_use.target) {
                    found.push(found_use);
                }
            }
        }
    }
    found
}

/// Every direct `call` or `jmp` into one of `targets`, as (site, whether it is a jump).
fn branches_into(pe: &PeFile, targets: &BTreeSet<u32>) -> Vec<(u32, bool)> {
    let mut found = Vec::new();
    for (base, bytes, is_code) in pe.sections() {
        if !is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(5) {
            if !matches!(bytes[at], 0xE8 | 0xE9) {
                continue;
            }
            let rel = i32::from_le_bytes(bytes[at + 1..at + 5].try_into().expect("in range"));
            let site = base + at as u32;
            let target = i64::from(site) + 5 + i64::from(rel);
            if u32::try_from(target).is_ok_and(|target| targets.contains(&target)) {
                found.push((site, bytes[at] == 0xE9));
            }
        }
    }
    found
}

/// The function part the exception table lists around an address.
fn enclosing(ranges: &BTreeMap<u32, u32>, rva: u32) -> Option<u32> {
    let (start, end) = ranges.range(..=rva).next_back()?;
    (rva < *end).then_some(*start)
}

/// The static pointers of the class metadata the code that uses the class name fills.
fn statics_of(pe: &PeFile, ranges: &BTreeMap<u32, u32>, name: &str) -> BTreeSet<(u32, bool)> {
    let texts = texts(pe, name);
    if texts.is_empty() {
        return BTreeSet::new();
    }
    let mut layer: BTreeSet<u32> = rip_uses(pe, &texts)
        .iter()
        .filter(|found| !found.word)
        .filter_map(|found| enclosing(ranges, found.at))
        .collect();
    let mut all = layer.clone();
    for _ in 0..CALLER_DEPTH {
        if layer.is_empty() || all.len() > CALLERS_MOST {
            break;
        }
        layer = branches_into(pe, &layer)
            .into_iter()
            .filter_map(|(site, _)| enclosing(ranges, site))
            .filter(|function| !all.contains(function))
            .collect();
        all.extend(&layer);
    }
    if all.len() > CALLERS_MOST {
        return BTreeSet::new();
    }
    all.iter()
        .filter_map(|function| load_target(pe, *function))
        .filter(|(target, _)| !pe.is_code(*target))
        .collect()
}

/// The functions whose result is `found`: those that load it, and the thunks that jump to them.
fn getters(pe: &PeFile, ranges: &BTreeMap<u32, u32>, found: (u32, bool)) -> BTreeSet<u32> {
    let loads = rip_uses(pe, &BTreeSet::from([found.0]));
    let mut getters: BTreeSet<u32> = loads
        .iter()
        .filter(|each| each.register == 0 && each.word == found.1)
        .filter_map(|each| enclosing(ranges, each.at))
        .filter(|function| load_target(pe, *function) == Some(found))
        .collect();
    let mut layer = getters.clone();
    for _ in 0..THUNK_DEPTH {
        let mut next = BTreeSet::new();
        for (site, jump) in branches_into(pe, &layer) {
            if !jump {
                continue;
            }
            // `jmp`, `sub rcx, n; jmp` or `add rcx, n; jmp`, as a thunk of a base that is not the
            // object's first moves `this` back before it jumps.
            let starts = [
                Some(site),
                site.checked_sub(4).filter(|start| {
                    pe.read(*start, 3)
                        .is_some_and(|head| matches!(head, [0x48, 0x83, 0xE9 | 0xC1]))
                }),
                site.checked_sub(7).filter(|start| {
                    pe.read(*start, 3)
                        .is_some_and(|head| matches!(head, [0x48, 0x81, 0xE9 | 0xC1]))
                }),
            ];
            for start in starts.into_iter().flatten() {
                if !getters.contains(&start) && load_target(pe, start) == Some(found) {
                    next.insert(start);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        getters.extend(&next);
        layer = next;
    }
    getters
}

/// The tables outside the code whose second slot holds one of `functions`.
fn tables_holding(pe: &PeFile, functions: &BTreeSet<u32>) -> BTreeSet<u32> {
    let wanted: HashSet<u64> = functions
        .iter()
        .map(|function| pe.image_base() + u64::from(*function))
        .collect();
    let mut found = BTreeSet::new();
    for (base, bytes, is_code) in pe.sections() {
        if is_code {
            continue;
        }
        for at in (8..bytes.len().saturating_sub(7)).step_by(8) {
            let word = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("in range"));
            if wanted.contains(&word) {
                found.insert(base + at as u32 - 8);
            }
        }
    }
    found
}

/// A table a constructor writes into an object: the table, the register that holds the object
/// and how far into the object it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Store {
    index: usize,
    table: u32,
    base: u8,
    offset: i64,
}

/// `mov [base + offset], register` with a 64-bit register, as (base, offset).
fn store_of(raw: &[u8], register: u8) -> Option<(u8, i64)> {
    let rex = raw.first().copied().filter(|byte| byte & 0xF0 == 0x40)?;
    let [_, op, modrm, ..] = *raw else {
        return None;
    };
    if rex & 8 == 0 || op != 0x89 {
        return None;
    }
    let mode = modrm >> 6;
    let rm = modrm & 7;
    // Register to register, a SIB byte or a RIP-relative operand is not a store into an object.
    if mode == 3 || rm == 4 || (mode == 0 && rm == 5) {
        return None;
    }
    if ((((rex >> 2) & 1) << 3) | ((modrm >> 3) & 7)) != register {
        return None;
    }
    let offset = match mode {
        0 => 0,
        1 => i64::from(*raw.get(3)? as i8),
        _ => i64::from(i32::from_le_bytes(raw.get(3..7)?.try_into().ok()?)),
    };
    Some((((rex & 1) << 3) | rm, offset))
}

/// The tables a function writes into objects: a `lea` of a table outside the code, then a store
/// of that register.
fn stores_in(pe: &PeFile, start: u32, end: u32) -> Vec<Store> {
    let length = end.saturating_sub(start).min(BODY_MOST) as usize;
    let Some(bytes) = pe.read(start, length) else {
        return vec![];
    };
    let mut steps = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some(instruction) = x64::decode_body(&bytes[at..]) else {
            break;
        };
        if instruction.length == 0 || at + instruction.length > bytes.len() {
            break;
        }
        steps.push(&bytes[at..at + instruction.length]);
        at += instruction.length;
    }
    let mut stores = Vec::new();
    let mut offset = 0u32;
    for (index, raw) in steps.iter().enumerate() {
        let rva = start + offset;
        offset += raw.len() as u32;
        let Some(lea) = (raw.len() == 7).then(|| rip_use(raw, rva)).flatten() else {
            continue;
        };
        if lea.word || pe.is_code(lea.target) {
            continue;
        }
        if let Some((base, at)) = steps
            .get(index + 1)
            .and_then(|next| store_of(next, lea.register))
        {
            stores.push(Store {
                index,
                table: lea.target,
                base,
                offset: at,
            });
        }
    }
    stores
}

/// The table each constructor that writes `provider` writes at the start of the same object.
fn first_tables(pe: &PeFile, ranges: &BTreeMap<u32, u32>, provider: u32) -> BTreeSet<u32> {
    let writers: BTreeSet<u32> = rip_uses(pe, &BTreeSet::from([provider]))
        .iter()
        .filter(|each| !each.word)
        .filter_map(|each| enclosing(ranges, each.at))
        .collect();
    let mut found = BTreeSet::new();
    for writer in writers {
        let stores = stores_in(pe, writer, ranges[&writer]);
        for written in stores.iter().filter(|store| store.table == provider) {
            let first = stores
                .iter()
                .filter(|store| {
                    store.base == written.base && store.offset == 0 && store.table != provider
                })
                .min_by_key(|store| store.index.abs_diff(written.index));
            if let Some(first) = first {
                found.insert(first.table);
            }
        }
    }
    found
}

/// The class tables of the views class `name` in `pe`, each the table at the start of its
/// objects. More than one means the name does not tell one class table, and the caller takes none.
pub fn class_tables(pe: &PeFile, name: &str) -> BTreeSet<u32> {
    let ranges = pe.function_ranges();
    let mut found = BTreeSet::new();
    for each in statics_of(pe, &ranges, name) {
        let getters = getters(pe, &ranges, each);
        if getters.is_empty() {
            continue;
        }
        for provider in tables_holding(pe, &getters) {
            found.extend(first_tables(pe, &ranges, provider));
        }
    }
    found
}

/// How many levels of callees below the code that fills the metadata are read for its name.
const CALLEE_DEPTH: usize = 2;
/// How many names may be tried for one table before it counts as too unclear to name.
const NAMES_MOST: usize = 16;
/// How long a class name may be.
const NAME_MOST: usize = 120;

/// The zero-terminated text at `rva` when it reads as a C++ class name without its namespace.
fn identifier_at(pe: &PeFile, rva: u32) -> Option<String> {
    let bytes = [NAME_MOST + 1, 64, 32, 16, 8]
        .into_iter()
        .find_map(|length| pe.read(rva, length))?;
    let end = bytes.iter().position(|byte| *byte == 0)?;
    let name = &bytes[..end];
    let first = *name.first()?;
    (name.len() >= 2
        && (first.is_ascii_alphabetic() || first == b'_')
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_'))
    .then(|| String::from_utf8_lossy(name).into_owned())
}

/// What a function's body loads with a RIP-relative `lea` outside the code, and the functions it
/// calls or jumps to directly.
fn body_of(pe: &PeFile, ranges: &BTreeMap<u32, u32>, start: u32) -> (Vec<u32>, Vec<u32>) {
    let end = ranges.get(&start).copied().unwrap_or(start);
    let length = end.saturating_sub(start).min(BODY_MOST) as usize;
    let Some(bytes) = pe.read(start, length) else {
        return (vec![], vec![]);
    };
    let (mut loads, mut callees) = (Vec::new(), Vec::new());
    let mut at = 0;
    while at < bytes.len() {
        let Some(instruction) = x64::decode_body(&bytes[at..]) else {
            break;
        };
        if instruction.length == 0 || at + instruction.length > bytes.len() {
            break;
        }
        let raw = &bytes[at..at + instruction.length];
        let rva = start + at as u32;
        if let Some(found) = (raw.len() == 7).then(|| rip_use(raw, rva)).flatten() {
            if !found.word && !pe.is_code(found.target) {
                loads.push(found.target);
            }
        }
        if raw.len() == 5 && matches!(raw[0], 0xE8 | 0xE9) {
            let rel = i32::from_le_bytes(raw[1..5].try_into().expect("five bytes"));
            if let Ok(target) = u32::try_from(i64::from(rva) + 5 + i64::from(rel)) {
                if ranges.contains_key(&target) {
                    callees.push(target);
                }
            }
        }
        at += instruction.length;
    }
    (loads, callees)
}

/// The metadata statics a class table's objects reach: each constructor that writes the table at
/// the start of an object also writes other tables into it, and the second slot of the
/// `ui::metadata::MetaDataProvider` table among them returns the class's static, as `getters` reads.
fn statics_of_table(pe: &PeFile, ranges: &BTreeMap<u32, u32>, table: u32) -> BTreeSet<(u32, bool)> {
    let writers: BTreeSet<u32> = rip_uses(pe, &BTreeSet::from([table]))
        .iter()
        .filter(|each| !each.word)
        .filter_map(|each| enclosing(ranges, each.at))
        .collect();
    let mut found = BTreeSet::new();
    for writer in writers {
        let stores = stores_in(pe, writer, ranges[&writer]);
        for first in stores
            .iter()
            .filter(|store| store.table == table && store.offset == 0)
        {
            for other in stores.iter().filter(|store| {
                store.base == first.base && store.offset != 0 && store.table != table
            }) {
                let Some(getter) = pe.pointer_rva(other.table + 8).filter(|at| pe.is_code(*at))
                else {
                    continue;
                };
                if let Some(each) = load_target(pe, getter).filter(|(at, _)| !pe.is_code(*at)) {
                    found.insert(each);
                }
            }
        }
    }
    found
}

/// The names the code that fills a metadata static loads, nearest first: the texts that the
/// functions whose result is the static, and their callees, load. Only those that read as a class
/// name are kept.
fn names_of_static(
    pe: &PeFile,
    ranges: &BTreeMap<u32, u32>,
    found: (u32, bool),
) -> Vec<Vec<String>> {
    let mut layer: BTreeSet<u32> = rip_uses(pe, &BTreeSet::from([found.0]))
        .iter()
        .filter_map(|each| enclosing(ranges, each.at))
        .filter(|function| load_target(pe, *function) == Some(found))
        .collect();
    let mut seen = layer.clone();
    let mut by_depth = Vec::new();
    for _ in 0..=CALLEE_DEPTH {
        if layer.is_empty() || seen.len() > CALLERS_MOST {
            break;
        }
        let mut names = Vec::new();
        let mut next = BTreeSet::new();
        for function in &layer {
            let (loads, callees) = body_of(pe, ranges, *function);
            names.extend(loads.into_iter().filter_map(|at| identifier_at(pe, at)));
            next.extend(callees.into_iter().filter(|callee| !seen.contains(callee)));
        }
        by_depth.push(names);
        seen.extend(&next);
        layer = next;
    }
    by_depth
}

/// The views class a class table belongs to, by the class name its `ui::metadata` holds, read back
/// from the build: the metadata static the table's objects reach, and the names the code that fills
/// it loads. A name counts only where `class_tables` gives exactly this table for it, so a name
/// that is not the class's own gives none. The nearest code that yields exactly one such name
/// names the table; none, or two, gives no name.
pub fn class_name_of_table(pe: &PeFile, table: u32) -> Option<String> {
    let ranges = pe.function_ranges();
    let mut by_depth: Vec<Vec<String>> = Vec::new();
    for each in statics_of_table(pe, &ranges, table) {
        for (depth, names) in names_of_static(pe, &ranges, each).into_iter().enumerate() {
            if by_depth.len() <= depth {
                by_depth.resize(depth + 1, Vec::new());
            }
            by_depth[depth].extend(names);
        }
    }
    let mut tried = BTreeSet::new();
    for names in by_depth {
        let mut confirmed = BTreeSet::new();
        for name in names {
            if tried.len() >= NAMES_MOST {
                return None;
            }
            if tried.insert(name.clone()) && class_tables(pe, &name) == BTreeSet::from([table]) {
                confirmed.insert(name);
            }
        }
        match confirmed.len() {
            0 => continue,
            1 => return confirmed.into_iter().next(),
            _ => return None,
        }
    }
    None
}

/// Whether `name` is the class name of `table` in `pe`: `class_tables` gives this table alone.
pub fn names_table(pe: &PeFile, name: &str, table: u32) -> bool {
    class_tables(pe, name) == BTreeSet::from([table])
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE_BASE: u64 = 0x1_8000_0000;

    /// A module with code at 0x1000, read-only data at 0x2000 and data at 0x3000, each 0x1000
    /// long, and an exception table at 0x2f00 that lists `ranges`.
    fn module(code: &[(u32, Vec<u8>)], ranges: &[(u32, u32)]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x400];
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&3u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let optional = 0x98;
        bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[optional + 24..optional + 32].copy_from_slice(&IMAGE_BASE.to_le_bytes());
        let directory = optional + 112 + 24;
        bytes[directory..directory + 4].copy_from_slice(&0x2f00u32.to_le_bytes());
        bytes[directory + 4..directory + 8]
            .copy_from_slice(&(12 * ranges.len() as u32).to_le_bytes());
        for (index, flags) in [0x6000_0020u32, 0x4000_0040, 0xC000_0040].iter().enumerate() {
            let header = optional + 240 + 40 * index;
            let rva = 0x1000 * (index as u32 + 1);
            bytes[header + 8..header + 12].copy_from_slice(&0x1000u32.to_le_bytes());
            bytes[header + 12..header + 16].copy_from_slice(&rva.to_le_bytes());
            bytes[header + 16..header + 20].copy_from_slice(&0x1000u32.to_le_bytes());
            let raw = 0x400 + 0x1000 * index as u32;
            bytes[header + 20..header + 24].copy_from_slice(&raw.to_le_bytes());
            bytes[header + 36..header + 40].copy_from_slice(&flags.to_le_bytes());
        }
        bytes.resize(0x400 + 0x3000, 0);
        // Code between functions is padding.
        bytes[0x400..0x1400].fill(0xCC);
        let mut table = Vec::new();
        for (start, end) in ranges {
            table.extend_from_slice(&start.to_le_bytes());
            table.extend_from_slice(&end.to_le_bytes());
            table.extend_from_slice(&0u32.to_le_bytes());
        }
        let mut code = code.to_vec();
        code.push((0x2f00, table));
        for (rva, data) in code {
            let at = 0x400 + (rva - 0x1000) as usize;
            bytes[at..at + data.len()].copy_from_slice(&data);
        }
        bytes
    }

    fn rel(from: u32, length: u32, to: u32) -> [u8; 4] {
        (to as i32 - (from + length) as i32).to_le_bytes()
    }

    fn pointer(rva: u32) -> Vec<u8> {
        (IMAGE_BASE + u64::from(rva)).to_le_bytes().to_vec()
    }

    /// `name`'s metadata at 0x3000, filled by its `MetaData()` at 0x1100 from the text at 0x2100.
    /// `GetClassMetaData` at 0x1200 is a thunk to it in slot 1 of the `MetaDataProvider` table at
    /// 0x2200, and the constructor at 0x1400 writes the class table at 0x2400 at the object's
    /// start and the provider's table 0x58 bytes in.
    fn class_module(name: &str) -> Vec<u8> {
        let mut meta = vec![0x48, 0x8B, 0x05];
        meta.extend(rel(0x1100, 7, 0x3000));
        // test rax, rax; jne +7
        meta.extend([0x48, 0x85, 0xC0, 0x75, 0x07]);
        meta.extend([0x48, 0x8D, 0x0D]);
        meta.extend(rel(0x110c, 7, 0x2100));
        meta.push(0xC3);
        let mut thunk = vec![0x48, 0x83, 0xE9, 0x58, 0xE9];
        thunk.extend(rel(0x1204, 5, 0x1100));
        let mut constructor = vec![0x48, 0x8D, 0x05];
        constructor.extend(rel(0x1400, 7, 0x2400));
        // mov [rsi], rax
        constructor.extend([0x48, 0x89, 0x06]);
        constructor.extend([0x48, 0x8D, 0x0D]);
        constructor.extend(rel(0x140a, 7, 0x2200));
        // mov [rsi + 0x58], rcx
        constructor.extend([0x48, 0x89, 0x4E, 0x58]);
        constructor.push(0xC3);
        let mut text = name.as_bytes().to_vec();
        text.push(0);
        let mut provider = pointer(0x1300);
        provider.extend(pointer(0x1200));
        let mut class = pointer(0x1300);
        class.extend(pointer(0x1310));
        module(
            &[
                (0x1100, meta),
                (0x1200, thunk),
                (0x1300, vec![0xC3]),
                (0x1310, vec![0xC3]),
                (0x1400, constructor),
                (0x2100, text),
                (0x2200, provider),
                (0x2400, class),
            ],
            &[(0x1100, 0x1120), (0x1300, 0x1301), (0x1310, 0x1311), (0x1400, 0x1420)],
        )
    }

    #[test]
    fn nfr6_a_frame_vtable_is_found_from_its_class_name() {
        let bytes = class_module("EdgeBrowserFrameViewWin");
        let pe = PeFile::parse(&bytes).unwrap();
        // The getter reads the static the way the runtime reads it.
        assert_eq!(load_target(&pe, 0x1200), Some((0x3000, true)));
        assert_eq!(class_tables(&pe, "EdgeBrowserFrameViewWin"), BTreeSet::from([0x2400]));
        // A name the build does not hold, or holds only as part of a longer one, gives nothing.
        assert!(class_tables(&pe, "BrowserFrameViewWin").is_empty());
        assert!(class_tables(&pe, "WindowsCaptionButton").is_empty());
    }

    #[test]
    fn nfr6_a_class_whose_constructor_writes_no_first_table_gives_none() {
        let mut bytes = class_module("BrowserView");
        // The constructor stores another register than the one it loaded the class table into, so
        // no table is known at the object's start.
        let at = 0x400 + 0x407;
        assert_eq!(bytes[at..at + 3], [0x48, 0x89, 0x06]);
        bytes[at + 2] = 0x0E;
        let pe = PeFile::parse(&bytes).unwrap();
        assert_eq!(store_of(&[0x48, 0x89, 0x0E], 1), Some((6, 0)));
        assert_eq!(store_of(&[0x48, 0x89, 0x4E, 0x58], 1), Some((6, 0x58)));
        assert!(class_tables(&pe, "BrowserView").is_empty());
    }

    #[test]
    fn nfr6_a_class_name_counts_only_where_it_gives_the_prior_table_in_the_prior_build() {
        let prior = class_module("WindowsCaptionButton");
        let prior = PeFile::parse(&prior).unwrap();
        let new = class_module("WindowsCaptionButton");
        let new = PeFile::parse(&new).unwrap();
        let place = crate::row_resolve::class_place(&prior, &new, 0x2400, "WindowsCaptionButton");
        assert_eq!(place, Ok(0x2400));
        // The prior row's table is another one: the name does not tell this table.
        let other = crate::row_resolve::class_place(&prior, &new, 0x2200, "WindowsCaptionButton");
        assert!(other.unwrap_err().contains("in the prior build"));
        let missing = crate::row_resolve::class_place(&prior, &new, 0x2400, "CustomFloatingCorner");
        assert!(missing.is_err());
    }

    #[test]
    fn nfr6_a_class_name_is_read_from_the_prior_build_without_symbols() {
        let bytes = class_module("EdgeBrowserFrameViewWin");
        let pe = PeFile::parse(&bytes).unwrap();
        // The table the constructor writes at the object's start is named by the text its
        // metadata code loads, and the name gives that table back.
        assert_eq!(
            class_name_of_table(&pe, 0x2400).as_deref(),
            Some("EdgeBrowserFrameViewWin")
        );
        assert!(names_table(&pe, "EdgeBrowserFrameViewWin", 0x2400));
        // The provider table is written further into the object, so it names no class.
        assert_eq!(class_name_of_table(&pe, 0x2200), None);
        assert!(!names_table(&pe, "EdgeBrowserFrameViewWin", 0x2200));
        // A name the metadata code does not load does not name the table.
        assert!(!names_table(&pe, "BrowserFrameViewWin", 0x2400));
    }

    #[test]
    fn nfr6_a_class_name_is_a_plain_identifier() {
        let mut code = class_module("BrowserView");
        let at = 0x400 + 0x1000 + 0x100;
        assert_eq!(&code[at..at + 12], b"BrowserView\0");
        let pe = PeFile::parse(&code).unwrap();
        assert_eq!(identifier_at(&pe, 0x2100).as_deref(), Some("BrowserView"));
        // A path or a qualified name is not a class name.
        code[at + 7] = b':';
        let pe = PeFile::parse(&code).unwrap();
        assert_eq!(identifier_at(&pe, 0x2100), None);
    }
}
