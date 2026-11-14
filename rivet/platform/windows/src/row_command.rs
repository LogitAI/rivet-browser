//! Finds a function a browser command runs from its public command id.
//!
//! `chrome::BrowserCommandController::HandleCommandWithDisposition` runs every browser command
//! through one `switch (id)` (`chrome/browser/ui/browser_command_controller.cc`), and each case
//! calls the command's function first: `case IDC_NEW_TAB` calls `chrome::NewTab(browser_, ...)`.
//! The ids are public (`chrome/app/chrome_command_ids.h`, kept in `command_ids`). A switch that
//! dense compiles to a jump table: the id less the lowest case is checked against the number of
//! cases, then the table gives the case's code. So the case of an id is read from the table, and
//! the function is the first one its code calls.
//!
//! A build holds other switches over command ids, so the reading is checked on the prior build
//! first: the switches whose case for the id calls the prior row's function there tell the lowest
//! case of the dispatch switch, and in the new build only switches with that same lowest case are
//! read. Nothing is taken unless they all name one function.

use std::collections::BTreeSet;

use crate::chrome_build::PeFile;
use crate::row_port::x64;

/// How far a case's code is read for its first call.
const CASE_REACH: usize = 0x80;

/// A jump table switch: the lowest case it subtracts, the highest index it takes, its table and
/// where an index past it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    pub at: u32,
    pub lowest: i64,
    pub bound: u32,
    pub table: u32,
    pub default: u32,
}

fn i32_at(bytes: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn rip_target(rva: u32, length: usize, displacement: i32) -> Option<u32> {
    u32::try_from(i64::from(rva) + length as i64 + i64::from(displacement)).ok()
}

/// `lea r64, [rip + d]; movsxd r64, dword [r + index * 4]; add r64, r64; jmp r64` at `at`, as the
/// table it loads.
fn table_jump(bytes: &[u8], at: usize, rva: u32) -> Option<u32> {
    let code = bytes.get(at..at + 17)?;
    let [rex, 0x8D, modrm, ..] = *code else {
        return None;
    };
    if rex & 0xFB != 0x48 || modrm & 0xC7 != 0x05 {
        return None;
    }
    let register = (((rex >> 2) & 1) << 3) | ((modrm >> 3) & 7);
    let table = rip_target(rva, 7, i32_at(code, 3)?)?;
    // movsxd with a SIB byte that scales the index by four and takes the table as its base.
    let [load_rex, 0x63, load_modrm, sib] = code[7..11] else {
        return None;
    };
    if load_rex & 0xF8 != 0x48 || load_modrm & 0xC7 != 0x04 || sib >> 6 != 2 {
        return None;
    }
    if ((load_rex & 1) << 3) | (sib & 7) != register {
        return None;
    }
    // add r64, r64
    let [add_rex, add, add_modrm] = code[11..14] else {
        return None;
    };
    if add_rex & 0xF8 != 0x48 || !matches!(add, 0x01 | 0x03) || add_modrm >> 6 != 3 {
        return None;
    }
    // jmp r64, with a REX prefix for r8 to r15.
    let jump = if code[14] == 0x41 {
        &code[15..17]
    } else {
        &code[14..16]
    };
    (jump[0] == 0xFF && jump[1] & 0xF8 == 0xE0).then_some(table)
}

/// The `ja` that ends at `end`, as its target, with where it starts.
fn above_before(bytes: &[u8], end: usize, base: u32) -> Option<(usize, u32)> {
    if let Some(start) = end.checked_sub(6) {
        if bytes.get(start..start + 2) == Some(&[0x0F, 0x87][..]) {
            let target = rip_target(base + start as u32, 6, i32_at(bytes, start + 2)?)?;
            return Some((start, target));
        }
    }
    let start = end.checked_sub(2)?;
    let [0x77, rel] = *bytes.get(start..end)? else {
        return None;
    };
    Some((
        start,
        rip_target(base + start as u32, 2, i32::from(rel as i8))?,
    ))
}

/// The `cmp r32, imm` that ends at `end`, as its immediate, with where it starts.
fn compare_before(bytes: &[u8], end: usize) -> Option<(usize, u32)> {
    let at = |length: usize| {
        let start = end.checked_sub(length)?;
        Some((start, bytes.get(start..end)?))
    };
    // cmp eax, imm32
    if let Some((start, [0x3D, ..])) = at(5) {
        return Some((start, i32_at(bytes, start + 1)? as u32));
    }
    for rex in [false, true] {
        let skip = usize::from(rex);
        if let Some((start, code)) = at(6 + skip) {
            if (!rex || code[0] == 0x41) && code[skip] == 0x81 && code[skip + 1] & 0xF8 == 0xF8 {
                return Some((start, i32_at(bytes, start + skip + 2)? as u32));
            }
        }
        if let Some((start, code)) = at(3 + skip) {
            if (!rex || code[0] == 0x41) && code[skip] == 0x83 && code[skip + 1] & 0xF8 == 0xF8 {
                return Some((start, code[skip + 2] as i8 as i32 as u32));
            }
        }
    }
    None
}

/// The lowest case a switch subtracts before `end`: `lea r32, [r - n]`, `add r32, -n` or
/// `sub r32, n`, nearest first. Zero when the switch takes the id as it is.
fn lowest_before(bytes: &[u8], end: usize) -> i64 {
    for stop in (end.saturating_sub(16)..=end).rev() {
        for start in stop.saturating_sub(7)..stop {
            let Some(code) = bytes.get(start..stop) else {
                continue;
            };
            let skip = usize::from(matches!(code.first(), Some(0x40..=0x4F)) && code.len() > 1);
            let rest = &code[skip..];
            let value = match *rest {
                // lea r32, [r + disp32] or [r + disp8], with no SIB byte.
                [0x8D, modrm, ..] if modrm >> 6 == 2 && modrm & 7 != 4 && rest.len() == 6 => {
                    i32_at(rest, 2).map(|value| -i64::from(value))
                }
                [0x8D, modrm, disp] if modrm >> 6 == 1 && modrm & 7 != 4 => {
                    Some(-i64::from(disp as i8))
                }
                [0x05, ..] if rest.len() == 5 => i32_at(rest, 1).map(|value| -i64::from(value)),
                [0x2D, ..] if rest.len() == 5 => i32_at(rest, 1).map(i64::from),
                [0x81, modrm, ..] if rest.len() == 6 && modrm & 0xF8 == 0xC0 => {
                    i32_at(rest, 2).map(|value| -i64::from(value))
                }
                [0x81, modrm, ..] if rest.len() == 6 && modrm & 0xF8 == 0xE8 => {
                    i32_at(rest, 2).map(i64::from)
                }
                _ => None,
            };
            if let Some(value) = value {
                return value;
            }
        }
    }
    0
}

/// Every jump table switch in the build's code.
pub fn switches(pe: &PeFile) -> Vec<Switch> {
    let mut found = Vec::new();
    for (base, bytes, is_code) in pe.sections() {
        if !is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(17) {
            if bytes[at + 1] != 0x8D {
                continue;
            }
            let rva = base + at as u32;
            let Some(table) = table_jump(bytes, at, rva) else {
                continue;
            };
            if pe.read(table, 4).is_none() {
                continue;
            }
            // The bound check ends right before the table load, or before a `mov r32, r32` that
            // clears the index's upper half.
            let Some((above, default)) = [at, at.saturating_sub(2), at.saturating_sub(3)]
                .into_iter()
                .find_map(|end| above_before(bytes, end, base))
            else {
                continue;
            };
            let Some((compare, bound)) = compare_before(bytes, above) else {
                continue;
            };
            found.push(Switch {
                at: rva,
                lowest: lowest_before(bytes, compare),
                bound,
                table,
                default,
            });
        }
    }
    found
}

/// Where a switch sends `id`, unless it sends it where it sends an id it has no case for.
pub fn case_of(pe: &PeFile, switch: &Switch, id: i64) -> Option<u32> {
    let index = u32::try_from(id - switch.lowest).ok()?;
    if index > switch.bound {
        return None;
    }
    let entry = i32::from_le_bytes(pe.read(switch.table + index * 4, 4)?.try_into().ok()?);
    let case = rip_target(switch.table, 0, entry)?;
    (case != switch.default && pe.is_code(case)).then_some(case)
}

/// The first function the code at `at` calls before it leaves, after one jump it may start with.
pub fn first_call(pe: &PeFile, at: u32) -> Option<u32> {
    let mut at = at;
    let mut followed = false;
    let bytes = pe.read(at, CASE_REACH)?;
    let mut bytes = bytes.to_vec();
    let mut offset = 0;
    while offset < bytes.len() {
        let rva = at + offset as u32;
        match bytes[offset] {
            0xE8 => return rip_target(rva, 5, i32_at(&bytes, offset + 1)?),
            0xE9 if offset == 0 && !followed => {
                at = rip_target(rva, 5, i32_at(&bytes, 1)?)?;
                bytes = pe.read(at, CASE_REACH)?.to_vec();
                followed = true;
                continue;
            }
            0xC3 | 0xE9 | 0xEB | 0xCC => return None,
            _ => {}
        }
        let instruction = x64::decode_body(&bytes[offset..])?;
        if instruction.length == 0 {
            return None;
        }
        offset += instruction.length;
    }
    None
}

/// The function each switch's case for `id` calls first, with the switch.
pub fn command_calls(pe: &PeFile, id: i64) -> Vec<(Switch, u32)> {
    calls_through(pe, &switches(pe), id)
}

/// The function each of `switches`' case for `id` calls first, with the switch.
fn calls_through(pe: &PeFile, switches: &[Switch], id: i64) -> Vec<(Switch, u32)> {
    switches
        .iter()
        .filter_map(|switch| {
            let case = case_of(pe, switch, id)?;
            Some((*switch, first_call(pe, case)?))
        })
        .collect()
}

/// The function the command `id` runs in the new build, read the same way that gives `target` in
/// the prior build: only switches whose lowest case is that of a prior switch that sends the id to
/// `target` count, all of them must call one function, and in the prior build every such switch
/// calls `target`.
pub fn command_place(
    old_pe: &PeFile,
    new_pe: &PeFile,
    target: u32,
    id: i64,
) -> Result<u32, String> {
    let old = (old_pe, switches(old_pe));
    let new = (new_pe, switches(new_pe));
    command_place_in((old.0, &old.1), (new.0, &new.1), target, id)
}

/// `command_place` with the switches of both builds already read.
pub fn command_place_in(
    old: (&PeFile, &[Switch]),
    new: (&PeFile, &[Switch]),
    target: u32,
    id: i64,
) -> Result<u32, String> {
    let prior = calls_through(old.0, old.1, id);
    let lowest: BTreeSet<i64> = prior
        .iter()
        .filter(|(_, called)| *called == target)
        .map(|(switch, _)| switch.lowest)
        .collect();
    if lowest.is_empty() {
        return Err(format!(
            "no switch of the prior build sends command {id} to it"
        ));
    }
    let others = prior
        .iter()
        .filter(|(switch, called)| lowest.contains(&switch.lowest) && *called != target)
        .count();
    if others > 0 {
        return Err(format!(
            "{others} switches of the prior build like its dispatch send command {id} elsewhere"
        ));
    }
    let called: BTreeSet<u32> = calls_through(new.0, new.1, id)
        .into_iter()
        .filter(|(switch, _)| lowest.contains(&switch.lowest))
        .map(|(_, called)| called)
        .collect();
    match called.into_iter().collect::<Vec<_>>().as_slice() {
        [only] => Ok(*only),
        many => Err(format!(
            "command {id} reaches {} functions through the dispatch in the new build",
            many.len()
        )),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::row_resolve::tests::{call, module};

    pub const DISPATCH: u32 = 0x101000;
    const OTHER_SWITCH: u32 = 0x101800;
    const TABLE: u32 = 0x201000;
    const OTHER_TABLE: u32 = 0x203000;
    const DEFAULT: u32 = 0x101f00;
    const BACK: u32 = 0x102000;
    const ELSEWHERE: u32 = 0x104000;
    const LOWEST: i32 = 33000;
    const NEW_TAB_ID: i64 = 34014;

    fn rel(from: u32, length: u32, to: u32) -> [u8; 4] {
        ((i64::from(to) - i64::from(from) - i64::from(length)) as i32).to_le_bytes()
    }

    /// A switch at `at` over ids from `lowest` with `bound + 1` cases in the table at `table`:
    /// `lea eax, [rdx - lowest]; cmp eax, bound; ja default; lea rcx, [rip + table];
    /// movsxd rax, [rcx + rax * 4]; add rax, rcx; jmp rax`.
    fn switch_code(at: u32, lowest: i32, bound: u32, table: u32) -> Vec<u8> {
        let mut code = vec![0x8D, 0x82];
        code.extend((-lowest).to_le_bytes());
        code.push(0x3D);
        code.extend(bound.to_le_bytes());
        code.extend([0x0F, 0x87]);
        code.extend(rel(at + code.len() as u32 - 2, 6, DEFAULT));
        code.extend([0x48, 0x8D, 0x0D]);
        code.extend(rel(at + code.len() as u32 - 3, 7, table));
        code.extend([0x48, 0x63, 0x04, 0x81, 0x48, 0x01, 0xC8, 0xFF, 0xE0]);
        code
    }

    /// A case that loads the browser and a constant and calls `to`, then leaves the switch.
    fn case_code(at: u32, to: u32) -> Vec<u8> {
        let mut code = vec![0x48, 0x8B, 0x4E, 0x10, 0xBA, 0x01, 0, 0, 0];
        code.extend(call(at + code.len() as u32, to));
        code.extend([0xEB, 0x00]);
        code
    }

    /// A table of `bound + 1` entries from `table`, each the distance to `default` except `cases`.
    fn table_of(table: u32, bound: u32, cases: &[(u32, u32)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for index in 0..=bound {
            let to = cases
                .iter()
                .find(|(case, _)| *case == index)
                .map_or(DEFAULT, |(_, to)| *to);
            bytes.extend(((i64::from(to) - i64::from(table)) as i32).to_le_bytes());
        }
        bytes
    }

    /// A build whose dispatch over ids from 33000 sends IDC_BACK to `BACK` and IDC_NEW_TAB to
    /// `new_tab`, and that holds a second switch, over ids from 34000, whose case for 34014 calls
    /// another function. `new_tab` gets the bytes in `body` once, or at each of `copies`.
    pub fn dispatch_module(new_tab: Option<u32>, copies: &[u32]) -> Vec<u8> {
        let bound = 1100;
        let back_case = 0x101100;
        let new_tab_case = 0x101140;
        let other_case = 0x101900;
        let mut parts = vec![
            (DISPATCH, switch_code(DISPATCH, LOWEST, bound, TABLE)),
            (back_case, case_code(back_case, BACK)),
            (
                OTHER_SWITCH,
                switch_code(OTHER_SWITCH, 34000, 40, OTHER_TABLE),
            ),
            (other_case, case_code(other_case, ELSEWHERE)),
            (DEFAULT, vec![0xC3]),
            (BACK, vec![0xC3]),
            (ELSEWHERE, vec![0xC3]),
            (OTHER_TABLE, table_of(OTHER_TABLE, 40, &[(14, other_case)])),
        ];
        let mut cases = vec![(0, back_case)];
        if let Some(new_tab) = new_tab {
            parts.push((new_tab_case, case_code(new_tab_case, new_tab)));
            cases.push(((NEW_TAB_ID - i64::from(LOWEST)) as u32, new_tab_case));
        }
        parts.push((TABLE, table_of(TABLE, bound, &cases)));
        let body = [
            0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x83, 0xC4, 0x20, 0x5E, 0xC3,
        ];
        let mut ranges = vec![(DISPATCH, DISPATCH + 0x40)];
        for at in copies {
            parts.push((*at, body.to_vec()));
            ranges.push((*at, *at + body.len() as u32));
        }
        module(&parts, &ranges)
    }

    pub const OLD_NEW_TAB: u32 = 0x103000;
    pub const NEW_NEW_TAB: u32 = 0x103400;

    #[test]
    fn nfr6_a_switch_case_is_read_from_its_jump_table() {
        let bytes = dispatch_module(Some(OLD_NEW_TAB), &[OLD_NEW_TAB]);
        let pe = PeFile::parse(&bytes).unwrap();
        let found = switches(&pe);
        // The table load follows `lea`, `cmp` and `ja`: 6, 5 and 6 bytes.
        let dispatch = found
            .iter()
            .find(|switch| switch.at == DISPATCH + 17)
            .unwrap();
        assert_eq!(
            (
                dispatch.lowest,
                dispatch.bound,
                dispatch.table,
                dispatch.default
            ),
            (i64::from(LOWEST), 1100, TABLE, DEFAULT)
        );
        let back = case_of(&pe, dispatch, 33000).and_then(|case| first_call(&pe, case));
        assert_eq!(back, Some(BACK));
        // An id with no case goes where the switch sends any id it does not know.
        assert_eq!(case_of(&pe, dispatch, 33001), None);
        assert_eq!(case_of(&pe, dispatch, 40000), None);
        let mut called: Vec<u32> = command_calls(&pe, NEW_TAB_ID)
            .into_iter()
            .map(|(_, to)| to)
            .collect();
        called.sort();
        assert_eq!(called, [OLD_NEW_TAB, ELSEWHERE]);
    }

    /// What the resolver makes of `chrome::NewTab` between the two builds, with the places its
    /// fingerprint matched off the exception table's function starts.
    fn resolve_new_tab(loose: Vec<u32>) -> crate::row_resolve::Resolution {
        use crate::row_resolve::{resolve, Kind, Wanted};
        use std::collections::BTreeMap;
        let old = dispatch_module(Some(OLD_NEW_TAB), &[OLD_NEW_TAB]);
        let new = dispatch_module(Some(NEW_NEW_TAB), &[NEW_NEW_TAB]);
        let (old, new) = (PeFile::parse(&old).unwrap(), PeFile::parse(&new).unwrap());
        let wanted = BTreeMap::from([(
            OLD_NEW_TAB,
            Wanted {
                kind: Kind::Function,
                candidates: None,
                class: None,
                command: Some(NEW_TAB_ID),
                loose,
            },
        )]);
        let none = BTreeSet::new();
        resolve(&old, &new, &BTreeMap::new(), &wanted, &none, &none)
    }

    #[test]
    fn nfr6_new_tab_is_taken_where_its_command_and_fingerprint_agree() {
        let resolution = resolve_new_tab(vec![NEW_NEW_TAB]);
        assert_eq!(resolution.places.get(&OLD_NEW_TAB), Some(&NEW_NEW_TAB));
        assert_eq!(
            resolution.settled,
            ["0x103000 -> 0x103400 by its fingerprint and its command id"]
        );
        // The command id alone is one method.
        let alone = resolve_new_tab(Vec::new());
        assert!(alone.places.is_empty());
        let reason = &alone.reasons[&OLD_NEW_TAB];
        assert!(
            reason.starts_with("only its command id found it"),
            "{reason}"
        );
    }

    #[test]
    fn nfr6_new_tab_is_found_from_the_new_tab_command() {
        let old = dispatch_module(Some(OLD_NEW_TAB), &[OLD_NEW_TAB]);
        let new = dispatch_module(Some(NEW_NEW_TAB), &[NEW_NEW_TAB]);
        let (old, new) = (PeFile::parse(&old).unwrap(), PeFile::parse(&new).unwrap());
        // Only the dispatch's own switch counts; the other switch's case calls another function.
        assert_eq!(
            command_place(&old, &new, OLD_NEW_TAB, NEW_TAB_ID),
            Ok(NEW_NEW_TAB)
        );
        // The prior build must send the command to the prior row's function.
        let refused = command_place(&old, &new, 0x103800, NEW_TAB_ID).unwrap_err();
        assert!(
            refused.starts_with("no switch of the prior build"),
            "{refused}"
        );
        // A new build whose dispatch has no case for the command gives nothing.
        let gone = dispatch_module(None, &[]);
        let gone = PeFile::parse(&gone).unwrap();
        let none = command_place(&old, &gone, OLD_NEW_TAB, NEW_TAB_ID).unwrap_err();
        assert!(none.contains("reaches 0 functions"), "{none}");
    }
}
