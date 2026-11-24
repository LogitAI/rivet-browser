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
//! `chromium/reflect.rs` reads it, so the finder and the runtime agree on the static a table
//! names. Nothing is guessed: a step that finds nothing, or more than one table, gives no table.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::chrome_build::PeFile;
use crate::x64;

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
    // A function with a guarded stack frame first loads the stack cookie and mixes it with the
    // stack pointer (`xor rax, rsp`). That load is not the function's result.
    let at = (0..code.len() - 10).find(|&i| {
        // Any 64-bit register: a function with a stack frame keeps its result in one it saved
        // and moves it to `rax` at the end.
        code[i] & 0xFB == 0x48
            && matches!(code[i + 1], 0x8D | 0x8B)
            && code[i + 2] & 0xC7 == 0x05
            && code[i + 7..i + 10] != [0x48, 0x31, 0xE0]
    })?;
    let rel = i32::from_le_bytes(code[at + 3..at + 7].try_into().ok()?);
    let target = u32::try_from(i64::from(code_at) + at as i64 + 7 + i64::from(rel)).ok()?;
    Some((target, code[at + 1] == 0x8B))
}

/// Every place outside the code that holds `name` as a zero-terminated text.
pub(crate) fn texts(pe: &PeFile, name: &str) -> BTreeSet<u32> {
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
pub(crate) struct RipUse {
    pub(crate) at: u32,
    pub(crate) target: u32,
    register: u8,
    /// A `mov`, which reads the word at the target, rather than a `lea`.
    pub(crate) word: bool,
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

/// Every RIP-relative `lea` and `mov` and every direct `call` and `jmp` of a module's code, sorted
/// by what they point at. The code of a browser is hundreds of megabytes and each class name asks
/// about it many times, so it is read once and each question is a search in the sorted lists.
#[derive(Debug, Default)]
pub struct CodeIndex {
    uses: Vec<RipUse>,
    /// (target, site, whether it is a jump).
    branches: Vec<(u32, u32, bool)>,
}

impl CodeIndex {
    fn of<'p>(pe: &'p PeFile<'_>) -> &'p CodeIndex {
        pe.code_index.get_or_init(|| {
            let mut index = CodeIndex::default();
            for (base, bytes, is_code) in pe.sections() {
                if !is_code {
                    continue;
                }
                for at in 0..bytes.len() {
                    let rva = base + at as u32;
                    if let Some(found) = bytes.get(at..at + 7).and_then(|raw| rip_use(raw, rva)) {
                        index.uses.push(found);
                    }
                    if matches!(bytes[at], 0xE8 | 0xE9) {
                        if let Some(raw) = bytes.get(at + 1..at + 5) {
                            let rel = i32::from_le_bytes(raw.try_into().expect("four bytes"));
                            let target = i64::from(rva) + 5 + i64::from(rel);
                            // Only a branch into code can be one into a function.
                            if let Some(target) = u32::try_from(target)
                                .ok()
                                .filter(|target| pe.is_code(*target))
                            {
                                index.branches.push((target, rva, bytes[at] == 0xE9));
                            }
                        }
                    }
                }
            }
            index
                .uses
                .sort_unstable_by_key(|each| (each.target, each.at));
            index.branches.sort_unstable();
            index
        })
    }
}

/// Every RIP-relative `lea` or `mov` in the code whose target is one of `targets`, in the order
/// of where they are.
pub(crate) fn rip_uses(pe: &PeFile, targets: &BTreeSet<u32>) -> Vec<RipUse> {
    let index = CodeIndex::of(pe);
    let mut found = Vec::new();
    for target in targets {
        let from = index.uses.partition_point(|each| each.target < *target);
        found.extend(
            index.uses[from..]
                .iter()
                .take_while(|each| each.target == *target)
                .copied(),
        );
    }
    found.sort_unstable_by_key(|each: &RipUse| each.at);
    found
}

/// Every direct `call` or `jmp` into one of `targets`, as (site, whether it is a jump), in the
/// order of where they are.
pub(crate) fn branches_into(pe: &PeFile, targets: &BTreeSet<u32>) -> Vec<(u32, bool)> {
    let index = CodeIndex::of(pe);
    let mut found = Vec::new();
    for target in targets {
        let from = index.branches.partition_point(|each| each.0 < *target);
        found.extend(
            index.branches[from..]
                .iter()
                .take_while(|each| each.0 == *target)
                .map(|each| (each.1, each.2)),
        );
    }
    found.sort_unstable();
    found
}

/// The function part the exception table lists around an address.
pub(crate) fn enclosing(ranges: &BTreeMap<u32, u32>, rva: u32) -> Option<u32> {
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
        .filter(|each| each.word == found.1)
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

/// The tables a constructor leaves in the object it writes `around` into, in the order of where
/// they go. A constructor of a derived class writes over the tables of its bases, so the last
/// store at each place counts.
fn object_tables(stores: &[Store], around: &Store) -> Vec<(i64, u32)> {
    let mut last: BTreeMap<i64, u32> = BTreeMap::new();
    for store in stores.iter().filter(|store| store.base == around.base) {
        last.insert(store.offset, store.table);
    }
    // Without the table at the object's start the places cannot be counted.
    if !last.contains_key(&0) {
        return Vec::new();
    }
    last.into_iter().collect()
}

/// Which of an object's tables a class name means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The table at this place in the order of where they go: 0 is the object's start.
    At(usize),
    /// The table of the object's `views::View` part. `ui/views/view.h` declares `View` with seven
    /// polymorphic bases before `ui::metadata::MetaDataProvider` (`LayerDelegate`,
    /// `LayerObserver`, `LayerOwner`, `AcceleratorTarget`, `EventTarget`, `EventHandler`,
    /// `PropertyHandler`), so the `View` part's table is the seventh before the provider's.
    View,
}

/// How many tables of a `views::View` lie between its own table and its metadata provider's.
const VIEW_BEFORE_PROVIDER: usize = 7;

/// A class name and which of its object's tables is meant: `BrowserView` is the table at the start
/// of the object, `BrowserView#6` the seventh in the order of where they go, and `BrowserView#view`
/// the table of its `views::View` part. A class with several bases has a table for each, and the
/// order of the bases is the class's own declaration.
fn name_and_order(name: &str) -> (&str, Place) {
    match name.split_once('#') {
        Some((bare, "view")) => (bare, Place::View),
        Some((bare, order)) => (bare, Place::At(order.parse().unwrap_or(usize::MAX))),
        None => (name, Place::At(0)),
    }
}

/// Which of an object's tables, in the order of where they go, `order` means, when the metadata
/// provider's table is the one at `provider_offset`.
fn place_index(tables: &[(i64, u32)], provider_offset: i64, order: Place) -> Option<usize> {
    match order {
        Place::At(index) => Some(index),
        Place::View => tables
            .iter()
            .position(|(at, _)| *at == provider_offset)
            .and_then(|provider| provider.checked_sub(VIEW_BEFORE_PROVIDER)),
    }
}

/// The table each constructor that writes `provider` leaves at place `order` of the same object.
fn tables_beside(
    pe: &PeFile,
    ranges: &BTreeMap<u32, u32>,
    provider: u32,
    order: Place,
) -> BTreeSet<u32> {
    let writers: BTreeSet<u32> = rip_uses(pe, &BTreeSet::from([provider]))
        .iter()
        .filter(|each| !each.word)
        .filter_map(|each| enclosing(ranges, each.at))
        .collect();
    let mut found = BTreeSet::new();
    for writer in writers {
        let stores = stores_in(pe, writer, ranges[&writer]);
        for written in stores.iter().filter(|store| store.table == provider) {
            let tables = object_tables(&stores, written);
            // The provider's own table must be what the constructor leaves at its place.
            if !tables
                .iter()
                .any(|(at, table)| *at == written.offset && *table == provider)
            {
                continue;
            }
            let index = place_index(&tables, written.offset, order);
            if let Some((_, table)) = index.and_then(|index| tables.get(index)) {
                found.insert(*table);
            }
        }
    }
    found
}

/// The class tables of the views class `name` in `pe`, each the table at the place the name gives
/// in its objects. More than one means the name does not tell one class table, and the caller
/// takes none.
pub fn class_tables(pe: &PeFile, name: &str) -> BTreeSet<u32> {
    let (bare, order) = name_and_order(name);
    let ranges = pe.function_ranges();
    let mut found = BTreeSet::new();
    for each in statics_of(pe, &ranges, bare) {
        let getters = getters(pe, &ranges, each);
        if getters.is_empty() || getters.len() > CALLERS_MOST {
            continue;
        }
        for provider in tables_holding(pe, &getters) {
            found.extend(tables_beside(pe, &ranges, provider, order));
        }
    }
    found
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
        for (index, flags) in [0x6000_0020u32, 0x4000_0040, 0xC000_0040]
            .iter()
            .enumerate()
        {
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
            &[
                (0x1100, 0x1120),
                (0x1300, 0x1301),
                (0x1310, 0x1311),
                (0x1400, 0x1420),
            ],
        )
    }

    #[test]
    fn nfr6_a_frame_vtable_is_found_from_its_class_name() {
        let bytes = class_module("EdgeBrowserFrameViewWin");
        let pe = PeFile::parse(&bytes).unwrap();
        // The getter reads the static the way the runtime reads it.
        assert_eq!(load_target(&pe, 0x1200), Some((0x3000, true)));
        assert_eq!(
            class_tables(&pe, "EdgeBrowserFrameViewWin"),
            BTreeSet::from([0x2400])
        );
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
    fn nfr6_a_table_past_the_start_of_its_object_is_named_with_its_place() {
        // A class with several bases has a table for each, and a name without the place would give
        // the first.
        assert_eq!(name_and_order("BrowserView#6"), ("BrowserView", Place::At(6)));
        assert_eq!(name_and_order("BrowserView"), ("BrowserView", Place::At(0)));
        assert_eq!(name_and_order("SidePanel#view"), ("SidePanel", Place::View));
        let store = |table, offset| Store {
            index: 0,
            table,
            base: 6,
            offset,
        };
        let stores = [
            store(0x10, 0),
            store(0x20, 8),
            store(0x30, 0x1f8),
            store(0x11, 0),
        ];
        // A derived constructor's later store at a place replaces the base's.
        assert_eq!(
            object_tables(&stores, &stores[1]),
            [(0, 0x11), (8, 0x20), (0x1f8, 0x30)]
        );
        // An object whose first table is not seen gives no places.
        assert!(object_tables(&stores[1..3], &stores[1]).is_empty());
    }

    #[test]
    fn nfr6_a_view_part_is_the_seventh_table_before_its_metadata_provider() {
        // A derived class's own table, then the View part's table and its six other bases, then
        // the metadata provider's table.
        let tables: Vec<(i64, u32)> = (0..10).map(|index| (index * 8, 0x100 + index as u32)).collect();
        assert_eq!(place_index(&tables, 8 * 8, Place::View), Some(1));
        assert_eq!(place_index(&tables, 8 * 8, Place::At(3)), Some(3));
        // A provider too near the object's start has no View part before it.
        assert_eq!(place_index(&tables, 6 * 8, Place::View), None);
        assert_eq!(place_index(&tables, 0x999, Place::View), None);
    }

    #[test]
    fn nfr6_the_stack_cookie_is_not_a_functions_result() {
        // sub rsp; mov rax, [rip+cookie]; xor rax, rsp; mov [rsp+0x28], rax; mov rsi, [rip+static]
        let mut code = vec![0x48, 0x83, 0xEC, 0x30];
        code.extend([0x48, 0x8B, 0x05, 0x00, 0x10, 0x00, 0x00, 0x48, 0x31, 0xE0]);
        code.extend([0x48, 0x89, 0x44, 0x24, 0x28]);
        code.extend([0x48, 0x8B, 0x35, 0x00, 0x20, 0x00, 0x00]);
        code.resize(160, 0xCC);
        let bytes = module(&[(0x1000, code)], &[(0x1000, 0x10a0)]);
        let pe = PeFile::parse(&bytes).unwrap();
        assert_eq!(load_target(&pe, 0x1000), Some((0x1000 + 26 + 0x2000, true)));
    }
}
