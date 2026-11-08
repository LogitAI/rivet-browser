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
//! followed, a few hops deep, until one leads to the view whose class is `RootView`. Below it the
//! view list is found as the field of pointers to views whose own parent field points back.

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

/// Where the code at `at` loads its result from: `lea rax, [rip+d]` within its first bytes, after
/// following one `jmp rel32` at its start. `read` gives `length` bytes at an address.
pub(crate) fn load_target(
    read: &dyn Fn(usize, usize) -> Option<Vec<u8>>,
    at: usize,
) -> Option<usize> {
    let rel =
        |bytes: &[u8], from: usize| i32::from_le_bytes(bytes[from..from + 4].try_into().unwrap());
    let mut code_at = at;
    let first = read(code_at, 5)?;
    if first[0] == 0xE9 {
        code_at = (code_at as i64 + 5 + rel(&first, 1) as i64) as usize;
    }
    let code = read(code_at, 64)?;
    (0..code.len() - 7)
        .find(|&i| code[i] == 0x48 && code[i + 1] == 0x8D && code[i + 2] == 0x05)
        .map(|i| (code_at as i64 + i as i64 + 7 + rel(&code, i + 3) as i64) as usize)
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
    for part in (0..0x100).step_by(8) {
        if !is_object(ranges, view + part) {
            continue;
        }
        let Some(slot) = word(view + part).and_then(|table| word(table + 8)) else {
            continue;
        };
        let Some(meta) = load_target(&bytes, slot) else {
            continue;
        };
        if !Ranges::has(ranges.data, meta) {
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
                return String::from_utf8(name).ok();
            }
        }
    }
    None
}

/// What the walk found, with the offsets it derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub root_view: usize,
    pub path: Vec<usize>,
    pub children: usize,
    pub parent: usize,
    pub browser_view: Option<usize>,
    pub visited: usize,
}

const MAX_DEPTH: usize = 4;
const FIELDS: usize = 0x800;
const MAX_NODES: usize = 4000;

/// Follows pointer fields from the window's `HWNDMessageHandler` to its `RootView`, then derives
/// the view list and parent fields and finds the `BrowserView` below it, within `budget`.
pub(crate) fn find(ranges: &Ranges, handler: usize, budget: Duration) -> Result<Found, String> {
    let start = Instant::now();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([(handler, Vec::new())]);
    let mut visited = 0;
    let mut root = None;
    while let Some((object, path)) = queue.pop_front() {
        if start.elapsed() > budget || visited > MAX_NODES {
            return Err(format!(
                "no RootView within the budget after {visited} objects"
            ));
        }
        visited += 1;
        if class_name(ranges, object).as_deref() == Some("RootView") {
            root = Some((object, path));
            break;
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
    let (root_view, path) = root.ok_or("no RootView reached from the window")?;
    let (children, parent) = view_list(ranges, root_view)
        .ok_or("no view list below the RootView whose views point back to it")?;
    let mut browser_view = None;
    let mut stack = vec![(root_view, 0)];
    while let Some((view, depth)) = stack.pop() {
        if start.elapsed() > budget {
            break;
        }
        for child in children_of(view, children) {
            if class_name(ranges, child).as_deref() == Some("BrowserView") {
                browser_view = Some(child);
                break;
            }
            if depth < 8 {
                stack.push((child, depth + 1));
            }
        }
        if browser_view.is_some() {
            break;
        }
    }
    Ok(Found {
        root_view,
        path,
        children,
        parent,
        browser_view,
        visited,
    })
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

/// The field of `root` that is its view list, and the field of each listed view that points back
/// to `root`, found together: a vector of views each of which names `root` at one same field.
fn view_list(ranges: &Ranges, root: usize) -> Option<(usize, usize)> {
    for field in (0..FIELDS).step_by(8) {
        let views = children_of(root, field);
        if views.is_empty() || !views.iter().all(|view| class_name(ranges, *view).is_some()) {
            continue;
        }
        let back = (0..FIELDS)
            .step_by(8)
            .find(|offset| views.iter().all(|view| word(view + offset) == Some(root)));
        if let Some(parent) = back {
            return Some((field, parent));
        }
    }
    None
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
            let mut code = vec![0xCCu8; 64];
            if at == 0x1000 {
                code[0] = 0xE9;
                code[1..5].copy_from_slice(&(0x2000i32 - 0x1005).to_le_bytes());
            } else if at == 0x2000 {
                code[..4].copy_from_slice(&[0x48, 0x83, 0xEC, 0x28]);
                code[4..7].copy_from_slice(&[0x48, 0x8D, 0x05]);
                code[7..11].copy_from_slice(&0x100i32.to_le_bytes());
            } else if at == 0x3000 {
                code[..3].copy_from_slice(&[0x48, 0x8D, 0x05]);
                code[3..7].copy_from_slice(&(-0x10i32).to_le_bytes());
            }
            code.truncate(length);
            Some(code)
        };
        assert_eq!(load_target(&read, 0x1000), Some(0x2000 + 4 + 7 + 0x100));
        assert_eq!(load_target(&read, 0x3000), Some(0x3000 + 7 - 0x10));
        assert_eq!(load_target(&read, 0x4000), None);
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
