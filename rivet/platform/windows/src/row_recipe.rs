//! Reads the values of a row that are not addresses from the new build itself.
//!
//! A row holds member offsets, table slots and object sizes as well as addresses. They have no
//! public symbols, and a new milestone moves them without notice: Chromium 157 moved many view
//! members by 8 bytes and every frame view slot above one removed virtual by one. A port that
//! copies them gives a row that looks complete and is wrong. So each such value has recipes that
//! read it from the new build's own instructions and tables. A value is taken only when every
//! recipe that applies gives the same answer and passes its structural check. A value whose recipe
//! fails is reported and never guessed, so its row goes to a person and the runtime keeps the
//! feature off.
//!
//! The recipes read the row's addresses, so they run after the addresses have moved to the new
//! build. Paths are the row's own field names, as the runtime writes the row.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::chrome_build::PeFile;
use crate::row_port::x64;

/// The lowest value taken for an address. Offsets and slots lie below it.
const LOWEST_ADDRESS: u64 = 0x10_0000;
/// The address a build records for what it does not have.
const ABSENT: u64 = 0x7FFF_0000;
/// How many slots of a class table are read at most.
const TABLE_SLOTS: u32 = 512;
/// How far before a constructor call the allocation size may be set.
const ALLOCATION_WINDOW: usize = 32;

/// One way to read a value from a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    /// The slot of the function at `function` in the class table at `vtable`. Its check: every slot
    /// up to it points at code, so the slot lies inside one table.
    Slot {
        vtable: &'static str,
        function: &'static str,
    },
    /// The member that a short accessor at `function` reads from `this` with its first
    /// instruction, as `mov rax, [rcx + 0x558]`. Its check: the instruction is the function's first.
    FirstLoad { function: &'static str },
    /// The size every direct caller of the constructor at `constructor` allocates its object with:
    /// `mov edx, size` or `mov ecx, size`, the allocation call, `mov rcx, rax`, then the call. Its
    /// check: the instructions decode one after another up to the constructor call, and every
    /// caller that allocates agrees.
    AllocationSize { constructor: &'static str },
}

/// A value of a row and the recipes that read it.
pub struct Field {
    pub path: &'static str,
    pub recipes: &'static [Recipe],
}

/// Every value of a row that has a recipe. A value of a row that is not listed here is carried over
/// from the prior row, and the port reports it as such.
pub const FIELDS: &[Field] = &[
    Field {
        path: "tabs.browser_in_view",
        recipes: &[Recipe::FirstLoad {
            function: "tabs.active_web_contents",
        }],
    },
    Field {
        path: "side_panel.browser_offset",
        recipes: &[Recipe::FirstLoad {
            function: "navigation.active_web_contents",
        }],
    },
    Field {
        path: "app_window.web_view_size",
        recipes: &[Recipe::AllocationSize {
            constructor: "app_window.web_view_ctor",
        }],
    },
    Field {
        path: "toolbar.paint_slot",
        recipes: &[
            Recipe::Slot {
                vtable: "toolbar.browser_widget_vtable",
                function: "toolbar.paint",
            },
            Recipe::Slot {
                vtable: "toolbar.plain_widget_vtable",
                function: "toolbar.paint",
            },
        ],
    },
    Field {
        path: "toolbar.layout_slot",
        recipes: &[
            Recipe::Slot {
                vtable: "toolbar.browser_widget_vtable",
                function: "toolbar.layout",
            },
            Recipe::Slot {
                vtable: "toolbar.plain_widget_vtable",
                function: "toolbar.layout",
            },
        ],
    },
    Field {
        path: "toolbar.active_web_contents_slot",
        recipes: &[Recipe::Slot {
            vtable: "toolbar.browser_delegate_vtable",
            function: "toolbar.active_web_contents",
        }],
    },
    Field {
        path: "toolbar.web_contents_title_slot",
        recipes: &[Recipe::Slot {
            vtable: "toolbar.web_contents_vtable",
            function: "toolbar.web_contents_title",
        }],
    },
    Field {
        path: "toolbar.infobar_layout_slot",
        recipes: &[Recipe::Slot {
            vtable: "toolbar.infobar_vtable",
            function: "toolbar.infobar_layout",
        }],
    },
    Field {
        path: "side_panel.proposed_slot",
        recipes: &[Recipe::Slot {
            vtable: "side_panel.layout_vtable",
            function: "side_panel.proposed",
        }],
    },
    Field {
        path: "layout.on_paint_slot",
        recipes: &[Recipe::Slot {
            vtable: "layout.frame_vtable",
            function: "layout.on_paint",
        }],
    },
    Field {
        path: "layout.top_inset_slot",
        recipes: &[Recipe::Slot {
            vtable: "layout.frame_vtable",
            function: "layout.top_inset",
        }],
    },
    Field {
        path: "layout.layout_slot",
        recipes: &[Recipe::Slot {
            vtable: "layout.frame_vtable",
            function: "layout.layout",
        }],
    },
    Field {
        path: "layout.caption_size_slot",
        recipes: &[Recipe::Slot {
            vtable: "layout.caption_vtable",
            function: "layout.caption_size",
        }],
    },
    Field {
        path: "layout.caption_paint_slot",
        recipes: &[Recipe::Slot {
            vtable: "layout.caption_vtable",
            function: "layout.caption_paint",
        }],
    },
    Field {
        path: "location_bar.slot",
        recipes: &[Recipe::Slot {
            vtable: "location_bar.vtable",
            function: "location_bar.function",
        }],
    },
    Field {
        path: "location_bar.preferred_slot",
        recipes: &[Recipe::Slot {
            vtable: "location_bar.vtable",
            function: "location_bar.preferred",
        }],
    },
];

/// What the recipes found for one value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every recipe that applies gave this value and passed its check.
    Found(u64),
    /// The row has no such value, or none of its recipes has the addresses it reads.
    NotApplicable,
    /// Why the value cannot be read from this build.
    Failed(String),
}

/// The value at a path such as `layout.top_inset_slot`.
fn at_path<'a>(row: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(row, |value, key| &value[key])
}

/// A number as the runtime writes it: a `0x` string or a plain number.
pub fn number(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => u64::from_str_radix(text.strip_prefix("0x")?, 16).ok(),
        _ => None,
    }
}

/// The address at a path, or `None` when the row records none there.
fn address(build: &Value, path: &str) -> Option<u32> {
    number(at_path(build, path))
        .filter(|value| (LOWEST_ADDRESS..ABSENT).contains(value))
        .and_then(|value| u32::try_from(value).ok())
}

/// The member offset that the first instruction of a function reads from `this`: a 64-bit `mov`
/// into any register from `[rcx + displacement]`.
pub fn first_load(bytes: &[u8]) -> Option<u64> {
    let [rex, 0x8B, modrm, rest @ ..] = bytes else {
        return None;
    };
    // REX.W, and no REX.B, so the base register is rcx and not r9.
    if rex & 0xF8 != 0x48 || rex & 0x01 != 0 || modrm & 0x07 != 0x01 {
        return None;
    }
    match modrm >> 6 {
        0b01 => rest
            .first()
            .filter(|byte| **byte < 0x80)
            .map(|byte| u64::from(*byte)),
        0b10 => rest
            .get(..4)
            .map(|bytes| u64::from(u32::from_le_bytes(bytes.try_into().expect("four bytes")))),
        _ => None,
    }
}

/// The allocation size set before the end of `code`, which ends where the constructor is called.
/// From the `mov` of the size, the instructions must decode one after another to the end, with one
/// call (the allocation) and then `mov rcx, rax`, which hands the new object to the constructor.
pub fn allocation_before(code: &[u8]) -> Option<u64> {
    let end = code.len();
    for at in (end.saturating_sub(ALLOCATION_WINDOW)..end).rev() {
        if !matches!(code[at], 0xB9 | 0xBA) {
            continue;
        }
        // A REX prefix would name another register or width.
        if at > 0 && code[at - 1] & 0xF0 == 0x40 {
            continue;
        }
        let (mut cursor, mut allocated, mut handed) = (at, false, false);
        while cursor < end {
            let Some(instruction) = x64::decode(&code[cursor..]) else {
                break;
            };
            let bytes = &code[cursor..(cursor + instruction.length).min(end)];
            if bytes.first() == Some(&0xE8) {
                if allocated {
                    break;
                }
                allocated = true;
            } else if allocated && bytes == [0x48, 0x89, 0xC1] {
                handed = true;
            }
            cursor += instruction.length;
        }
        if cursor == end && allocated && handed {
            let size = code.get(at + 1..at + 5)?;
            return Some(u64::from(u32::from_le_bytes(size.try_into().ok()?)));
        }
    }
    None
}

/// Every size the direct callers of a constructor allocate its object with, and how many direct
/// callers there are.
fn allocation_sizes(pe: &PeFile, constructor: u32) -> (BTreeSet<u64>, usize) {
    let mut sizes = BTreeSet::new();
    let mut calls = 0;
    for (base, bytes, is_code) in pe.sections() {
        if !is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(5) {
            if bytes[at] != 0xE8 {
                continue;
            }
            let relative =
                i32::from_le_bytes(bytes[at + 1..at + 5].try_into().expect("four bytes"));
            let target = i64::from(base) + at as i64 + 5 + i64::from(relative);
            if target != i64::from(constructor) {
                continue;
            }
            calls += 1;
            if let Some(size) = allocation_before(&bytes[..at]) {
                sizes.insert(size);
            }
        }
    }
    (sizes, calls)
}

/// Runs one recipe: `Ok(None)` when the row lacks the addresses it reads.
fn run(recipe: &Recipe, build: &Value, pe: &PeFile) -> Result<Option<u64>, String> {
    match *recipe {
        Recipe::Slot { vtable, function } => {
            let (Some(table), Some(target)) = (address(build, vtable), address(build, function))
            else {
                return Ok(None);
            };
            let slot = (0..TABLE_SLOTS)
                .find(|slot| pe.pointer_rva(table + slot * 8) == Some(target))
                .ok_or_else(|| format!("{vtable} {table:#x} holds no {function} {target:#x}"))?;
            let inside = (0..=slot).all(|slot| {
                pe.pointer_rva(table + slot * 8)
                    .is_some_and(|entry| pe.is_code(entry))
            });
            if !inside {
                return Err(format!(
                    "slot {slot} of {vtable} {table:#x} lies past the end of its table"
                ));
            }
            Ok(Some(u64::from(slot)))
        }
        Recipe::FirstLoad { function } => {
            let Some(target) = address(build, function) else {
                return Ok(None);
            };
            let bytes = pe
                .read(target, 8)
                .ok_or_else(|| format!("{function} {target:#x} lies outside the module"))?;
            first_load(bytes).map(Some).ok_or_else(|| {
                format!(
                    "{function} {target:#x} does not start by reading a member of this: {}",
                    bytes
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                )
            })
        }
        Recipe::AllocationSize { constructor } => {
            let Some(target) = address(build, constructor) else {
                return Ok(None);
            };
            let (sizes, calls) = allocation_sizes(pe, target);
            let found: Vec<u64> = sizes.into_iter().collect();
            match found.as_slice() {
                [size] if *size % 8 == 0 && *size >= 0x40 => Ok(Some(*size)),
                [size] => Err(format!("{constructor} is allocated with {size:#x}, which no object has")),
                [] => Err(format!(
                    "none of the {calls} direct callers of {constructor} {target:#x} allocates with a constant size"
                )),
                many => Err(format!("the callers of {constructor} allocate {many:#x?}")),
            }
        }
    }
}

/// Reads one value with all of its recipes.
pub fn derive_field(field: &Field, build: &Value, pe: &PeFile) -> Outcome {
    if number(at_path(build, field.path)).is_none() {
        return Outcome::NotApplicable;
    }
    let mut found = BTreeSet::new();
    for recipe in field.recipes {
        match run(recipe, build, pe) {
            Ok(Some(value)) => {
                found.insert(value);
            }
            Ok(None) => {}
            Err(reason) => return Outcome::Failed(reason),
        }
    }
    let found: Vec<u64> = found.into_iter().collect();
    match found.as_slice() {
        [] => Outcome::NotApplicable,
        [value] => Outcome::Found(*value),
        many => Outcome::Failed(format!("its recipes disagree: {many:#x?}")),
    }
}

/// What the recipes did to a row.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Derived {
    /// Values read again, as `path: prior -> new` where they changed and `path: value` otherwise.
    pub read: Vec<String>,
    /// Values whose recipes failed, with the reason. Each one stops the row.
    pub failed: Vec<String>,
}

/// Reads every value with a recipe from the new build and writes it into the row, in the same form
/// the row held it.
pub fn derive_row(build: &mut Value, pe: &PeFile) -> Derived {
    let mut derived = Derived::default();
    for field in FIELDS {
        match derive_field(field, build, pe) {
            Outcome::Found(value) => {
                let (parent, key) = field.path.split_once('.').expect("a path has a parent");
                let prior = &build[parent][key];
                let old = number(prior);
                let written = if prior.is_string() {
                    Value::String(format!("{value:#x}"))
                } else {
                    json!(value)
                };
                build[parent][key] = written;
                derived.read.push(match old {
                    Some(old) if old != value => format!("{}: {old:#x} -> {value:#x}", field.path),
                    _ => format!("{}: {value:#x}", field.path),
                });
            }
            Outcome::NotApplicable => {}
            Outcome::Failed(reason) => derived.failed.push(format!("{}: {reason}", field.path)),
        }
    }
    derived
}

/// Values Rivet chooses rather than reads from a build, such as how long it waits before it opens
/// a new tab. The row carries them for the runtime, so they are kept from the prior row.
pub const SETTINGS: &[&str] = &["tabs.new_tab_delay_ms"];

/// Lists of `[id, colour]` pairs: the id is a value of the build, and the colour is Rivet's choice.
pub const ID_COLOUR_PAIRS: &[&str] = &["toolbar.dev.colours"];

/// The values of a row that are Rivet's settings rather than facts about its build.
pub fn settings(build: &Value) -> Vec<String> {
    let mut out: Vec<String> = SETTINGS
        .iter()
        .filter(|path| number(at_path(build, path)).is_some())
        .map(|path| (*path).to_owned())
        .collect();
    for list in ID_COLOUR_PAIRS {
        if let Some(pairs) = at_path(build, list).as_array() {
            out.extend((0..pairs.len()).map(|index| format!("{list}[{index}][1]")));
        }
    }
    out
}

/// Every number of a row below the address range that no recipe reads: member offsets, slots,
/// sizes, ids and constants that a port can only carry over. Rivet's own settings are not listed.
pub fn without_recipe(build: &Value) -> Vec<String> {
    fn walk(path: &str, value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Array(items)
                if items.len() == 2
                    && path
                        .rsplit_once('[')
                        .is_some_and(|(list, _)| ID_COLOUR_PAIRS.contains(&list)) =>
            {
                if number(&items[0]).is_some_and(|id| id < LOWEST_ADDRESS) {
                    out.push(format!("{path}[0]"));
                }
            }
            Value::Object(object) => {
                for (key, item) in object {
                    let next = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    walk(&next, item, out);
                }
            }
            Value::Array(items) => {
                // An array of numbers is one value, such as a list of colour ids.
                if !items.is_empty() && items.iter().all(|item| number(item).is_some()) {
                    if items
                        .iter()
                        .any(|item| number(item).is_some_and(|value| value < LOWEST_ADDRESS))
                    {
                        out.push(path.to_owned());
                    }
                    return;
                }
                for (index, item) in items.iter().enumerate() {
                    walk(&format!("{path}[{index}]"), item, out);
                }
            }
            _ => {
                if number(value).is_some_and(|value| value < LOWEST_ADDRESS) {
                    out.push(path.to_owned());
                }
            }
        }
    }
    let mut out = vec![];
    walk("", build, &mut out);
    // The version is the build's name, and the fingerprints describe the prior build.
    out.retain(|path| {
        path != "version"
            && !path.starts_with("version[")
            && !FIELDS.iter().any(|field| field.path == path)
            && !SETTINGS.contains(&path.as_str())
    });
    out
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
        bytes[header + 8..header + 12].copy_from_slice(&0x10000u32.to_le_bytes());
        bytes[header + 12..header + 16].copy_from_slice(&0x100000u32.to_le_bytes());
        bytes[header + 16..header + 20].copy_from_slice(&0x10000u32.to_le_bytes());
        bytes[header + 20..header + 24].copy_from_slice(&0x400u32.to_le_bytes());
        bytes[header + 36..header + 40].copy_from_slice(&0x2000_0000u32.to_le_bytes());
        bytes.resize(0x400 + 0x10000, 0xCC);
        for (rva, data) in code {
            let at = 0x400 + (*rva - 0x100000) as usize;
            bytes[at..at + data.len()].copy_from_slice(data);
        }
        bytes
    }

    fn pointer(rva: u32) -> [u8; 8] {
        (IMAGE_BASE + u64::from(rva)).to_le_bytes()
    }

    fn call(from: u32, to: u32) -> Vec<u8> {
        let relative = i64::from(to) - i64::from(from) - 5;
        let mut bytes = vec![0xE8];
        bytes.extend_from_slice(&(relative as i32).to_le_bytes());
        bytes
    }

    #[test]
    fn nfr6_a_member_offset_is_read_from_the_accessor_that_loads_it() {
        assert_eq!(
            first_load(&[0x48, 0x8B, 0x81, 0x58, 0x05, 0x00, 0x00]),
            Some(0x558)
        );
        assert_eq!(first_load(&[0x48, 0x8B, 0x41, 0x30, 0xC3]), Some(0x30));
        // `mov rax, [r9 + 0x30]` reads another object, and a store is not a read.
        assert_eq!(first_load(&[0x49, 0x8B, 0x41, 0x30]), None);
        assert_eq!(first_load(&[0x48, 0x89, 0x41, 0x30]), None);
        // A function that starts some other way gives nothing rather than a guess.
        assert_eq!(first_load(&[0x56, 0x48, 0x83, 0xEC, 0x20]), None);
    }

    #[test]
    fn nfr6_an_object_size_is_read_where_its_constructor_is_called() {
        // mov edx, 0x520; xor r8d, r8d; call alloc; mov r14, rax; mov rcx, rax; mov rdx, r15
        let mut code = vec![0xBA, 0x20, 0x05, 0x00, 0x00, 0x45, 0x31, 0xC0];
        code.extend(call(0x10_0008, 0x10_0800));
        code.extend([0x49, 0x89, 0xC6, 0x48, 0x89, 0xC1, 0x4C, 0x89, 0xFA]);
        assert_eq!(allocation_before(&code), Some(0x520));
        // Without the hand-over to the constructor, the size belongs to something else.
        let mut other = code.clone();
        other[16..19].copy_from_slice(&[0x48, 0x89, 0xC2]);
        assert_eq!(allocation_before(&other), None);
    }

    #[test]
    fn nfr6_an_object_size_comes_from_every_direct_caller_of_its_constructor() {
        let constructor = 0x10_2000;
        let mut site = vec![0xBA, 0x20, 0x05, 0x00, 0x00, 0x45, 0x31, 0xC0];
        site.extend(call(0x10_0008, 0x10_0800));
        site.extend([0x48, 0x89, 0xC1]);
        site.extend(call(0x10_0010, constructor));
        let bytes = module(&[(0x10_0000, site.as_slice()), (0x10_2000, &[0x56, 0xC3][..])]);
        let pe = PeFile::parse(&bytes).unwrap();
        let build = json!({
            "app_window": {"web_view_ctor": "0x102000", "web_view_size": "0x528"}
        });
        let field = FIELDS
            .iter()
            .find(|field| field.path == "app_window.web_view_size")
            .unwrap();
        assert_eq!(derive_field(field, &build, &pe), Outcome::Found(0x520));
    }

    #[test]
    fn nfr6_a_slot_is_the_place_of_its_function_in_the_class_table() {
        let mut table = vec![];
        for function in [0x10_1000u32, 0x10_1100, 0x10_1200] {
            table.extend(pointer(function));
        }
        let bytes = module(&[(0x10_8000, table.as_slice())]);
        let pe = PeFile::parse(&bytes).unwrap();
        let mut build = json!({
            "layout": {
                "frame_vtable": "0x108000", "top_inset": "0x101200", "top_inset_slot": 3,
                "layout": "0x101100", "layout_slot": "0x1",
            }
        });
        let derived = derive_row(&mut build, &pe);
        assert_eq!(build["layout"]["top_inset_slot"], 2);
        assert_eq!(build["layout"]["layout_slot"], "0x1");
        assert_eq!(
            derived.read,
            vec![
                "layout.top_inset_slot: 0x3 -> 0x2",
                "layout.layout_slot: 0x1"
            ]
        );
        assert!(derived.failed.is_empty());
    }

    #[test]
    fn nfr6_a_value_its_recipes_cannot_read_is_refused_not_guessed() {
        let bytes = module(&[(0x10_0000, &[0x56, 0x48, 0x83, 0xEC, 0x20][..])]);
        let pe = PeFile::parse(&bytes).unwrap();
        let mut build = json!({
            "tabs": {"active_web_contents": "0x100000", "browser_in_view": "0x560"},
            "layout": {"frame_vtable": "0x108000", "on_paint": "0x101000", "on_paint_slot": 89},
        });
        let derived = derive_row(&mut build, &pe);
        assert_eq!(derived.failed.len(), 2, "{:?}", derived.failed);
        assert!(derived.failed[0].starts_with("tabs.browser_in_view: "));
        assert!(derived.failed[1].starts_with("layout.on_paint_slot: "));
        // The prior values stay as they were, and the failures stop the row.
        assert_eq!(build["tabs"]["browser_in_view"], "0x560");
        assert_eq!(build["layout"]["on_paint_slot"], 89);
    }

    #[test]
    fn nfr6_two_recipes_that_disagree_refuse_the_value() {
        let mut first = vec![];
        let mut second = vec![];
        for function in [0x10_1000u32, 0x10_1100] {
            first.extend(pointer(function));
        }
        for function in [0x10_1100u32, 0x10_1000] {
            second.extend(pointer(function));
        }
        let bytes = module(&[
            (0x10_8000, first.as_slice()),
            (0x10_8100, second.as_slice()),
        ]);
        let pe = PeFile::parse(&bytes).unwrap();
        let mut build = json!({
            "toolbar": {
                "browser_widget_vtable": "0x108000", "plain_widget_vtable": "0x108100",
                "paint": "0x101100", "paint_slot": "0x1",
            }
        });
        let derived = derive_row(&mut build, &pe);
        assert!(
            derived
                .failed
                .iter()
                .any(|line| line.starts_with("toolbar.paint_slot: its recipes disagree")),
            "{:?}",
            derived.failed
        );
    }

    #[test]
    fn nfr6_values_with_no_recipe_are_listed_by_path() {
        let build = json!({
            "version": [157, 0, 8103, 38],
            "tabs": {"browser_in_view": "0x558", "title": "0x00b77780", "groups": true},
            "layout": {"frame_ids": [0x141, 0x142], "caption_container_offset": "0x3c0"},
        });
        assert_eq!(
            without_recipe(&build),
            vec!["layout.caption_container_offset", "layout.frame_ids"]
        );
    }

    #[test]
    fn nfr6_rivet_settings_are_not_values_of_the_build() {
        let build = json!({
            "tabs": {"new_tab_delay_ms": 0, "tab_contents": "0x0"},
            "toolbar": {"dev": {"colours": [[316, 4280953905u32], [2082, 0]], "console": 40005}},
        });
        assert_eq!(
            without_recipe(&build),
            vec![
                "tabs.tab_contents",
                "toolbar.dev.colours[0][0]",
                "toolbar.dev.colours[1][0]",
                "toolbar.dev.console"
            ]
        );
        assert_eq!(
            settings(&build),
            vec![
                "tabs.new_tab_delay_ms",
                "toolbar.dev.colours[0][1]",
                "toolbar.dev.colours[1][1]"
            ]
        );
    }
}
