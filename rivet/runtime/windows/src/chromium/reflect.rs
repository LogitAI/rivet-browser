//! Finds a browser window's views by Chromium's own class metadata, reading memory only.
//!
//! Every view class carries `ui::metadata` class metadata with its name compiled in. A view's
//! `MetaDataProvider` part has a vtable whose second slot, `GetClassMetaData`, returns the class's
//! static metadata: either it loads that static's address at once, or it jumps to the class's
//! `MetaData()`, which loads it. Rivet decodes that load instead of calling it, then reads the
//! metadata's `type_name_`, a string view of the class name. No function of the browser is called.
//!
//! The way in needs no build value either: `gfx::WindowImpl` keeps itself in the window's
//! `GWLP_USERDATA`, and `views::HWNDMessageHandler` is a `WindowImpl`. From there pointer fields are
//! followed, a few hops deep, until one leads to the view whose class is `BrowserView`. The view
//! that holds it, its view list and its parent field are then found together.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::chromium::memory::readable;

/// The code, read-only data and writable data of the browser's module, as address ranges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Ranges {
    pub text: (usize, usize),
    pub rdata: (usize, usize),
    pub data: (usize, usize),
}

impl Ranges {
    fn has(range: (usize, usize), address: usize) -> bool {
        address >= range.0 && address < range.1
    }
}

/// The sections of a PE image from its headers, as (name, rva, size).
pub(crate) fn sections(headers: &[u8]) -> Vec<([u8; 8], usize, usize)> {
    let u16_at = |at: usize| {
        headers
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32_at = |at: usize| {
        headers
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let Some(nt) = u32_at(0x3C) else {
        return Vec::new();
    };
    if headers.get(nt..nt + 4) != Some(b"PE\0\0") {
        return Vec::new();
    }
    let (Some(count), Some(optional)) = (u16_at(nt + 6), u16_at(nt + 20)) else {
        return Vec::new();
    };
    let table = nt + 24 + optional;
    (0..count)
        .filter_map(|index| {
            let at = table + index * 40;
            let name: [u8; 8] = headers.get(at..at + 8)?.try_into().ok()?;
            Some((name, u32_at(at + 12)?, u32_at(at + 8)?))
        })
        .collect()
}

/// The module's ranges from the sections of the image at `base`.
pub(crate) fn ranges_of(base: usize, list: &[([u8; 8], usize, usize)]) -> Ranges {
    let mut ranges = Ranges::default();
    for (name, rva, size) in list {
        let range = (base + rva, base + rva + size);
        match name.split(|byte| *byte == 0).next().unwrap_or(&[]) {
            b".text" => ranges.text = range,
            b".rdata" => ranges.rdata = range,
            b".data" => ranges.data = range,
            _ => {}
        }
    }
    ranges
}

/// Where the code at `at` gets its result: the address of the first `lea rax, [rip+d]` or of the
/// word the first `mov rax, [rip+d]` reads, whichever comes first, as (address, whether it is a
/// word to read). `MetaData()` keeps its class's metadata in a static pointer it fills on first
/// use, so it reads that pointer. `read` gives `length` bytes at an address.
pub(crate) fn load_target(
    read: &dyn Fn(usize, usize) -> Option<Vec<u8>>,
    at: usize,
) -> Option<(usize, bool)> {
    let rel =
        |bytes: &[u8], from: usize| i32::from_le_bytes(bytes[from..from + 4].try_into().unwrap());
    let mut code_at = at;
    // A slot of a base that is not the object's first holds a thunk that moves `this` back to the
    // object (`sub rcx, n` or `add rcx, n`) before it jumps to the class's own function, which may
    // in turn jump to `MetaData()`. Up to three such steps are followed.
    for _ in 0..3 {
        let head = read(code_at, 12)?;
        let skip = match head[..3] {
            [0x48, 0x83, 0xE9] | [0x48, 0x83, 0xC1] => 4,
            [0x48, 0x81, 0xE9] | [0x48, 0x81, 0xC1] => 7,
            _ => 0,
        };
        if head[skip] == 0xE9 {
            code_at = (code_at as i64 + skip as i64 + 5 + rel(&head, skip + 1) as i64) as usize;
        } else {
            break;
        }
    }
    let code = read(code_at, 128)?;
    (0..code.len() - 7)
        .find(|&i| {
            code[i] == 0x48 && (code[i + 1] == 0x8D || code[i + 1] == 0x8B) && code[i + 2] == 0x05
        })
        .map(|i| {
            let target = (code_at as i64 + i as i64 + 7 + rel(&code, i + 3) as i64) as usize;
            (target, code[i + 1] == 0x8B)
        })
}

/// Whether `text` is a class name as Chromium writes one.
pub(crate) fn is_class_name(text: &[u8]) -> bool {
    (3..=64).contains(&text.len())
        && text[0].is_ascii_uppercase()
        && text.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

fn word(address: usize) -> Option<usize> {
    readable(address, 8).then(|| unsafe { (address as *const usize).read() })
}

fn bytes(address: usize, length: usize) -> Option<Vec<u8>> {
    let start = address & !7;
    let end = address + length;
    readable(start, (end - start + 7) & !7)
        .then(|| unsafe { std::slice::from_raw_parts(address as *const u8, length).to_vec() })
}

/// Whether `address` holds an object of the module: its first word is a vtable in read-only data
/// whose first two slots lie in the code.
fn is_object(ranges: &Ranges, address: usize) -> bool {
    let Some(table) = word(address) else {
        return false;
    };
    Ranges::has(ranges.rdata, table)
        && word(table).is_some_and(|slot| Ranges::has(ranges.text, slot))
        && word(table + 8).is_some_and(|slot| Ranges::has(ranges.text, slot))
}

/// The class name of the view at `view`, read through the metadata its `MetaDataProvider` part
/// names, which lies within the view's first 0x100 bytes.
pub(crate) fn class_name(ranges: &Ranges, view: usize) -> Option<String> {
    class_name_within(ranges, view, 0x100)
}

/// `class_name` for a pointer to the start of a `views::View`, whose `MetaDataProvider` part comes
/// after seven other bases and their members, so it is looked for within `reach` bytes.
fn class_name_within(ranges: &Ranges, view: usize, reach: usize) -> Option<String> {
    name_and_part(ranges, view, reach).map(|(name, _)| name)
}

/// `class_name_within` with the offset of the `MetaDataProvider` part it was read through.
fn name_and_part(ranges: &Ranges, view: usize, reach: usize) -> Option<(String, usize)> {
    for part in (0..reach).step_by(8) {
        if !is_object(ranges, view + part) {
            continue;
        }
        let Some(slot) = word(view + part).and_then(|table| word(table + 8)) else {
            continue;
        };
        let Some((target, is_word)) = load_target(&bytes, slot) else {
            continue;
        };
        if !Ranges::has(ranges.data, target) {
            continue;
        }
        let Some(meta) = (if is_word { word(target) } else { Some(target) }) else {
            continue;
        };
        if meta == 0 || !readable(meta, 0x90) {
            continue;
        }
        // `type_name_` is a string view: a pointer into read-only data and a length.
        for field in (8..0x80).step_by(8) {
            let (Some(text), Some(length)) = (word(meta + field), word(meta + field + 8)) else {
                continue;
            };
            if !Ranges::has(ranges.rdata, text) || !(3..=64).contains(&length) {
                continue;
            }
            if let Some(name) = bytes(text, length).filter(|name| is_class_name(name)) {
                return String::from_utf8(name).ok().map(|name| (name, part));
            }
        }
    }
    None
}

/// What the walk found, with the offsets it derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    /// The view whose view list holds the browser view.
    pub root_view: usize,
    pub path: Vec<usize>,
    pub children: usize,
    pub parent: usize,
    pub browser_view: Option<usize>,
    /// How far the browser view's `views::View` part lies from the object the walk reached.
    pub view_part: isize,
    pub visited: usize,
}

const MIN_DEPTH: usize = 2;
const MAX_DEPTH: usize = 4;
const FIELDS: usize = 0x800;
const MAX_NODES: usize = 4000;

/// Follows pointer fields from the window's `HWNDMessageHandler` to its `BrowserView` within
/// `budget`, then derives the view list and parent fields from the view that holds it.
pub(crate) fn find(ranges: &Ranges, handler: usize, budget: Duration) -> Result<Found, String> {
    let (browser_view, path, visited) = reach(ranges, handler, budget)?;
    let (holder, children, parent, view) = own_view_list(ranges, browser_view)
        .ok_or("no view part of the BrowserView lists views that name it as their parent")?;
    Ok(Found {
        root_view: holder,
        path,
        children,
        parent,
        browser_view: Some(view),
        view_part: view as isize - browser_view as isize,
        visited,
    })
}

/// The object named `BrowserView` that pointer fields lead to from the handler within `budget`,
/// with the path of fields and how many objects were visited.
pub(crate) fn reach(
    ranges: &Ranges,
    handler: usize,
    budget: Duration,
) -> Result<(usize, Vec<usize>, usize), String> {
    let start = Instant::now();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([(handler, Vec::new())]);
    let mut visited = 0;
    let mut root = None;
    // The first class names met, so a walk that misses says what it saw.
    let mut names: Vec<String> = Vec::new();
    while let Some((object, path)) = queue.pop_front() {
        if start.elapsed() > budget || visited > MAX_NODES {
            return Err(format!(
                "no BrowserView within the budget after {visited} objects; names met {names:?}"
            ));
        }
        visited += 1;
        // Views lie at least two pointers past the handler. Names are only read that deep, which
        // keeps the walk inside its budget.
        if path.len() >= MIN_DEPTH {
            if let Some(name) = class_name(ranges, object) {
                if name == "BrowserView" {
                    root = Some((object, path));
                    break;
                }
                if names.len() < 12 && !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        if path.len() >= MAX_DEPTH {
            continue;
        }
        for field in (0..FIELDS).step_by(8) {
            let Some(next) = word(object + field) else {
                break;
            };
            if next % 8 == 0 && seen.insert(next) && is_object(ranges, next) {
                let mut longer = path.clone();
                longer.push(field);
                queue.push_back((next, longer));
            }
        }
    }
    let (browser_view, path) =
        root.ok_or_else(|| format!("no BrowserView reached from the window; names met {names:?}"))?;
    Ok((browser_view, path, visited))
}

/// The BrowserView's `views::View` part and its member fields, found from its own view list. In
/// `views::View` the parent pointer comes right before the list of children (`parent_`, then
/// `children_`, a `std::vector` of views), and every child's parent field names the view. So the
/// View part is the address near the object reached whose field `children` is a list of views
/// each naming it at `children - 8`. Returns (parent view, view list field, parent field, View
/// part).
fn own_view_list(ranges: &Ranges, object: usize) -> Option<(usize, usize, usize, usize)> {
    let (_, part) = name_and_part(ranges, object, 0x100)?;
    let metadata = object + part;
    for view in (metadata.saturating_sub(AROUND)..=metadata)
        .rev()
        .step_by(8)
    {
        for children in (8..FIELDS).step_by(8) {
            let listed = children_of(view, children);
            if listed.is_empty() || listed.len() > 64 {
                continue;
            }
            let parent = children - 8;
            if listed
                .iter()
                .all(|child| word(child + parent) == Some(view))
            {
                return Some((word(view + parent).unwrap_or(0), children, parent, view));
            }
        }
    }
    None
}
/// How far around the object the walk reached its `views::View` part and parent field may lie. The
/// object's metadata part, through which the walk named it, comes late among `View`'s bases, so the
/// `View` part can begin before it.
const AROUND: usize = 0x800;

/// A build's fingerprint from its headers: the link time stamp and the image size.
pub(crate) fn fingerprint(headers: &[u8]) -> Option<(u32, u32)> {
    let u32_at = |at: usize| {
        headers
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let nt = u32_at(0x3C)? as usize;
    if headers.get(nt..nt + 4) != Some(b"PE\0\0") {
        return None;
    }
    Some((u32_at(nt + 8)?, u32_at(nt + 24 + 56)?))
}

/// The offsets a walk derived, kept for the rest of the process by build fingerprint, so the
/// walk's budget is paid once per build.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Offsets {
    path: Vec<usize>,
    children: usize,
    parent: usize,
    view_part: isize,
}

static CACHE: std::sync::Mutex<Vec<((u32, u32), Offsets)>> = std::sync::Mutex::new(Vec::new());

/// `find`, but through the offsets this build's earlier walk derived when there are some. The
/// cached path must still end at the view named `BrowserView`; when it does not, the walk runs again.
/// Returns what was found and whether the cache gave it.
pub(crate) fn find_cached(
    ranges: &Ranges,
    handler: usize,
    budget: Duration,
    build: (u32, u32),
) -> Result<(Found, bool), String> {
    let known = CACHE.lock().ok().and_then(|cache| {
        cache
            .iter()
            .find(|(key, _)| *key == build)
            .map(|(_, offsets)| offsets.clone())
    });
    if let Some(offsets) = known {
        let root = offsets
            .path
            .iter()
            .try_fold(handler, |object, field| word(object + field));
        if let Some(view) =
            root.filter(|root| class_name(ranges, *root).as_deref() == Some("BrowserView"))
        {
            let part = view.wrapping_add_signed(offsets.view_part);
            let browser_view = Some(part);
            let root_view = word(part + offsets.parent).unwrap_or(0);
            return Ok((
                Found {
                    root_view,
                    path: offsets.path,
                    children: offsets.children,
                    parent: offsets.parent,
                    browser_view,
                    view_part: offsets.view_part,
                    visited: 0,
                },
                true,
            ));
        }
    }
    let found = find(ranges, handler, budget)?;
    if let Ok(mut cache) = CACHE.lock() {
        cache.retain(|(key, _)| *key != build);
        cache.push((
            build,
            Offsets {
                path: found.path.clone(),
                children: found.children,
                parent: found.parent,
                view_part: found.view_part,
            },
        ));
    }
    Ok((found, false))
}

/// The views a `std::vector<View*>` at `view + field` holds.
fn children_of(view: usize, field: usize) -> Vec<usize> {
    let (Some(begin), Some(end)) = (word(view + field), word(view + field + 8)) else {
        return Vec::new();
    };
    if begin == 0 || end < begin || (end - begin) % 8 != 0 || (end - begin) / 8 > 256 {
        return Vec::new();
    }
    (begin..end).step_by(8).filter_map(word).collect()
}

/// What memory shows around a browser view this build's row already knows, read only, so the walk
/// can be designed from facts: the view's name and the offset of its `MetaDataProvider` part, the
/// holder its `parent` field names, and every list in the holder that holds the view, as (field,
/// begin, end, count, index).
pub(crate) fn facts(ranges: &Ranges, view: usize, parent: usize, reached: Option<usize>) -> String {
    let own = name_and_part(ranges, view, 0x800);
    let holder = word(view + parent);
    let holder_named = holder.and_then(|holder| name_and_part(ranges, holder, 0x800));
    let mut lists = Vec::new();
    let mut direct = Vec::new();
    if let Some(holder) = holder {
        for field in (0..FIELDS).step_by(8) {
            let (Some(begin), Some(end)) = (word(holder + field), word(holder + field + 8)) else {
                break;
            };
            if begin == view {
                direct.push(field);
            }
            if begin == 0 || end <= begin || (end - begin) % 8 != 0 || (end - begin) / 8 > 256 {
                continue;
            }
            let items: Vec<usize> = (begin..end).step_by(8).filter_map(word).collect();
            if let Some(index) = items.iter().position(|item| *item == view) {
                lists.push((field, begin, end, items.len(), index));
            }
        }
    }
    format!(
        "facts: browser view part {view:#x} named {own:?}, reached {:?} bytes away, holder {holder:x?} \
         named {holder_named:?}, holder fields naming the view {direct:x?}, holder lists of it \
         (field, begin, end, count, index) {lists:x?}",
        reached.map(|reached| reached as isize - view as isize)
    )
}

/// What the decode sees in the object at `object`: for each part whose first word is a module
/// vtable, (part, vtable offset in the module, GetClassMetaData slot offset, the load it decodes
/// as (offset, whether a word), and whether that names readable metadata), at most `limit` of them.
pub(crate) fn parts(ranges: &Ranges, base: usize, object: usize, limit: usize) -> String {
    let mut seen = Vec::new();
    for part in (0..0x800).step_by(8) {
        if seen.len() >= limit {
            break;
        }
        if !is_object(ranges, object + part) {
            continue;
        }
        let table = word(object + part).unwrap_or(0);
        let slot = word(table + 8).unwrap_or(0);
        let load = load_target(&bytes, slot);
        let meta =
            load.and_then(|(target, is_word)| if is_word { word(target) } else { Some(target) });
        let readable_meta = meta.is_some_and(|meta| meta != 0 && readable(meta, 0x90));
        seen.push(format!(
            "({part:#x}, {:#x}, {:#x}, {:x?}, {readable_meta})",
            table.wrapping_sub(base),
            slot.wrapping_sub(base),
            load.map(|(target, is_word)| (target.wrapping_sub(base), is_word))
        ));
    }
    seen.join(" ")
}

/// The fields of the view at `view` that name an object whose class decodes, as (field, name,
/// metadata part), at most `limit` of them.
pub(crate) fn named_fields(ranges: &Ranges, view: usize, limit: usize) -> String {
    let mut seen = Vec::new();
    for field in (0..0x800).step_by(8) {
        if seen.len() >= limit {
            break;
        }
        let Some(target) = word(view + field) else {
            break;
        };
        if target == view || !is_object(ranges, target) {
            continue;
        }
        if let Some((name, part)) = name_and_part(ranges, target, 0x800) {
            seen.push(format!("({field:#x}, {name}, {part:#x})"));
        }
    }
    seen.join(" ")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr3_the_module_sections_come_from_its_own_headers() {
        let mut headers = vec![0u8; 0x400];
        headers[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        headers[0x80..0x84].copy_from_slice(b"PE\0\0");
        headers[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
        headers[0x94..0x96].copy_from_slice(&0xF0u16.to_le_bytes());
        let table = 0x80 + 24 + 0xF0;
        for (index, (name, rva, size)) in [
            (b".text\0\0\0", 0x1000u32, 0x500u32),
            (b".rdata\0\0", 0x2000, 0x300),
        ]
        .iter()
        .enumerate()
        {
            let at = table + index * 40;
            headers[at..at + 8].copy_from_slice(*name);
            headers[at + 8..at + 12].copy_from_slice(&size.to_le_bytes());
            headers[at + 12..at + 16].copy_from_slice(&rva.to_le_bytes());
        }
        let list = sections(&headers);
        assert_eq!(list.len(), 2);
        let ranges = ranges_of(0x1_0000, &list);
        assert_eq!(ranges.text, (0x1_1000, 0x1_1500));
        assert_eq!(ranges.rdata, (0x1_2000, 0x1_2300));
        assert!(sections(&[0u8; 16]).is_empty());
    }

    #[test]
    fn nfr3_a_class_metadata_load_is_decoded_through_one_jump() {
        // At 0x1000: jmp to 0x2000. At 0x2000: some bytes, then lea rax,[rip+0x100], ret.
        let read = |at: usize, length: usize| -> Option<Vec<u8>> {
            let mut code = vec![0xCCu8; 128];
            if at == 0x1000 {
                code[0] = 0xE9;
                code[1..5].copy_from_slice(&(0x2000i32 - 0x1005).to_le_bytes());
            } else if at == 0x2000 {
                code[..4].copy_from_slice(&[0x48, 0x83, 0xEC, 0x28]);
                code[4..7].copy_from_slice(&[0x48, 0x8D, 0x05]);
                code[7..11].copy_from_slice(&0x100i32.to_le_bytes());
            } else if at == 0x5000 {
                code[..4].copy_from_slice(&[0x48, 0x83, 0xE9, 0x10]);
                code[4] = 0xE9;
                code[5..9].copy_from_slice(&(0x1000i32 - 0x5009).to_le_bytes());
            } else if at == 0x3000 {
                // mov rax, [rip-0x10]: MetaData() reading its static pointer.
                code[..3].copy_from_slice(&[0x48, 0x8B, 0x05]);
                code[3..7].copy_from_slice(&(-0x10i32).to_le_bytes());
            }
            code.truncate(length);
            Some(code)
        };
        assert_eq!(
            load_target(&read, 0x1000),
            Some((0x2000 + 4 + 7 + 0x100, false))
        );
        assert_eq!(load_target(&read, 0x3000), Some((0x3000 + 7 - 0x10, true)));
        assert_eq!(load_target(&read, 0x4000), None);
        // A thunk: sub rcx, 0x10, then jmp to 0x1000, which jumps on to 0x2000.
        assert_eq!(
            load_target(&read, 0x5000),
            Some((0x2000 + 4 + 7 + 0x100, false))
        );
    }

    #[test]
    fn nfr3_a_build_is_known_by_its_link_time_and_image_size() {
        let mut headers = vec![0u8; 0x200];
        headers[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        headers[0x80..0x84].copy_from_slice(b"PE\0\0");
        headers[0x88..0x8C].copy_from_slice(&0x6543_2100u32.to_le_bytes());
        headers[0x80 + 24 + 56..0x80 + 24 + 60].copy_from_slice(&0x0F00_0000u32.to_le_bytes());
        assert_eq!(fingerprint(&headers), Some((0x6543_2100, 0x0F00_0000)));
        assert_eq!(fingerprint(&[0u8; 8]), None);
    }

    #[test]
    fn nfr3_only_a_chromium_class_name_is_taken_as_one() {
        assert!(is_class_name(b"RootView"));
        assert!(is_class_name(b"BrowserView"));
        assert!(!is_class_name(b"../../ui/views/view.cc"));
        assert!(!is_class_name(b"ab"));
        assert!(!is_class_name(b"lowercase"));
    }
}
