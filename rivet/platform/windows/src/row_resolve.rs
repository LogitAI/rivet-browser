//! Finds, in a build whose symbols are not published, the functions and class tables that the
//! prior row's fingerprints do not pin down to one place.
//!
//! Each such address is looked for by several independent methods, and a place is taken only when
//! two different methods name the same place and no method names another. The methods read the
//! prior build, whose row a person verified, next to the new build:
//!
//! - calls into it: a function known in both builds calls or loads it from the same spot of its
//!   body in both builds;
//! - its calls: in the prior build it calls functions that their own fingerprints find in the new
//!   build, and exactly one candidate makes the same calls from the same spots;
//! - its table slot: a class table known in both builds holds it, at the slot that lines up once
//!   the slots around it are compared, since a milestone can add or remove a virtual function;
//! - a text it uses: it refers to a text the new build holds once, and exactly one function there
//!   refers to that text;
//! - for a class table, references to it (a function known in both builds loads it from the same
//!   spot) and its slots (exactly one candidate holds the same functions in the same slots).
//!
//! Anything else stays unresolved with its reason, and its row goes to a person.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::rc::Rc;

use crate::chrome_build::PeFile;
use crate::row_port::{code_print, find_prints, find_table, whole_function, x64};

/// How many bytes of a function body are read at most.
const BODY_LIMIT: u32 = 0x4000;
/// How far a function the exception table does not list, a leaf, is read at most.
const LEAF_LIMIT: u32 = 0x200;
/// How many instructions before a site name it within its body.
const CONTEXT: usize = 3;
/// How many levels of calls are followed from the functions known in both builds.
const FOLLOW_DEPTH: usize = 3;
/// How many functions are followed at most.
const FOLLOW_LIMIT: usize = 4000;
/// How many slots of a class table are read at most.
const TABLE_SLOTS: u32 = 512;
/// How many slots of a class table are compared to tell one table from another.
const TABLE_SEEN: u32 = 0x80;
/// How many slots on each side of a slot are compared to line two tables up.
const ALIGN_REACH: u32 = 6;
/// How far a milestone may move a slot.
const ALIGN_SHIFT: i64 = 2;
/// How many slots must agree for two tables, or two stretches of them, to be the same.
const ALIGN_LEAST: usize = 3;
/// The shortest text taken as a sign of the function that uses it.
const TEXT_LEAST: usize = 8;
/// The longest text read.
const TEXT_MOST: usize = 256;
/// How many candidates a function with no fingerprint match may have.
const CANDIDATES_MOST: usize = 256;
/// How many times the methods run again with what the last round found.
const ROUNDS: usize = 4;

/// One instruction of a function body, as far as the methods need it.
#[derive(Debug, Clone)]
pub struct Step {
    pub rva: u32,
    /// The bytes with the relative value, displacement and immediate cleared.
    pub skeleton: Vec<u8>,
    /// Where a direct call or jump goes.
    pub call: Option<u32>,
    /// What a RIP-relative operand points at: data, a table or a function passed as a callback.
    pub reference: Option<u32>,
    /// A displacement from a register, as `[rcx + 0x558]` holds.
    pub displacement: Option<i64>,
    pub immediate: Option<i64>,
    /// A call or jump through memory, as `call [rax + 0x2b8]` calls a table slot.
    pub indirect: bool,
    /// Its operand is the stack: a local variable, or the frame's size. Neither is a value of a row.
    pub stack: bool,
}

/// Whether a memory operand's base is the stack or frame pointer, which hold locals, not members.
fn stack_based(raw: &[u8], displacement: Option<(usize, usize)>) -> bool {
    let Some((offset, size)) = displacement else {
        return false;
    };
    let opcode = opcode_at(raw);
    // REX.B names r12 or r13 where the base field says rsp or rbp.
    let extended = opcode > 0 && raw[opcode - 1] & 0xF0 == 0x40 && raw[opcode - 1] & 1 != 0;
    let mode = if size == 1 { 1 } else { 2 };
    let before = offset.checked_sub(1).and_then(|at| raw.get(at)).copied();
    let base = match before {
        // The ModRM byte itself, with no SIB byte after it.
        Some(modrm) if modrm >> 6 == mode && modrm & 7 != 4 => modrm & 7,
        // A SIB byte, whose low bits name the base.
        Some(sib) => sib & 7,
        None => return false,
    };
    !extended && matches!(base, 4 | 5)
}

fn signed(bytes: &[u8], field: (usize, usize)) -> Option<i64> {
    let (offset, size) = field;
    let part = bytes.get(offset..offset + size)?;
    Some(match size {
        1 => i64::from(part[0] as i8),
        2 => i64::from(i16::from_le_bytes([part[0], part[1]])),
        4 => i64::from(i32::from_le_bytes([part[0], part[1], part[2], part[3]])),
        8 => i64::from_le_bytes(part.try_into().ok()?),
        _ => return None,
    })
}

/// Where the opcode of an instruction is, past its prefixes.
fn opcode_at(raw: &[u8]) -> usize {
    let mut at = 0;
    while raw.get(at).is_some_and(|byte| {
        matches!(
            byte,
            0x66 | 0xF2 | 0xF3 | 0xF0 | 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65
        )
    }) {
        at += 1;
    }
    if raw.get(at).is_some_and(|byte| byte & 0xF0 == 0x40) {
        at += 1;
    }
    at
}

fn step(raw: &[u8], instruction: &x64::Instruction, rva: u32) -> Step {
    let next = i64::from(rva) + instruction.length as i64;
    let opcode = opcode_at(raw);
    let first = raw.get(opcode).copied();
    let second = raw.get(opcode + 1).copied();
    let branch = matches!(first, Some(0x70..=0x7F | 0xEB | 0xE0..=0xE3 | 0xE8 | 0xE9))
        || (first == Some(0x0F) && matches!(second, Some(0x80..=0x8F)));
    let mut call = None;
    let mut reference = None;
    if let Some(field) = instruction.relative {
        let target = signed(raw, field)
            .map(|distance| next + distance)
            .and_then(|target| u32::try_from(target).ok());
        if matches!(first, Some(0xE8 | 0xE9)) {
            call = target;
        } else if !branch {
            reference = target;
        }
    }
    Step {
        rva,
        skeleton: x64::skeleton(raw, instruction),
        call,
        reference,
        displacement: instruction
            .displacement
            .and_then(|field| signed(raw, field)),
        immediate: instruction.immediate.and_then(|field| signed(raw, field)),
        indirect: first == Some(0xFF)
            && second.is_some_and(|modrm| matches!((modrm >> 3) & 7, 2 | 4)),
        // `add rsp, n` and `sub rsp, n` size the frame.
        stack: stack_based(raw, instruction.displacement)
            || (matches!(first, Some(0x81 | 0x83)) && matches!(second, Some(0xC4 | 0xEC))),
    }
}

/// Reads a function body from its start, as far as its exception table entry or, for a leaf, its
/// padding, and stops at the first instruction it cannot read.
fn read_body(pe: &PeFile, ranges: &BTreeMap<u32, u32>, start: u32) -> Vec<Step> {
    if !pe.is_code(start) {
        return vec![];
    }
    let listed = ranges.get(&start).copied();
    let length = match listed {
        Some(end) => end.saturating_sub(start).min(BODY_LIMIT),
        None => LEAF_LIMIT,
    } as usize;
    let Some(bytes) = pe.read(start, length).or_else(|| pe.read(start, 16)) else {
        return vec![];
    };
    let mut steps = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        if listed.is_none() && rest.starts_with(&[0xCC, 0xCC]) {
            break;
        }
        let Some(instruction) = x64::decode_body(rest) else {
            break;
        };
        if instruction.length == 0 || instruction.length > rest.len() {
            break;
        }
        steps.push(step(&rest[..instruction.length], &instruction, start + at as u32));
        at += instruction.length;
    }
    steps
}

/// One build: its module, its function ranges and the bodies read so far.
pub struct Side<'a> {
    pub pe: &'a PeFile<'a>,
    ranges: BTreeMap<u32, u32>,
    bodies: HashMap<u32, Rc<Vec<Step>>>,
}

impl<'a> Side<'a> {
    pub fn new(pe: &'a PeFile<'a>) -> Self {
        Self {
            pe,
            ranges: pe.function_ranges(),
            bodies: HashMap::new(),
        }
    }

    pub fn body(&mut self, start: u32) -> Rc<Vec<Step>> {
        if let Some(body) = self.bodies.get(&start) {
            return Rc::clone(body);
        }
        let body = Rc::new(read_body(self.pe, &self.ranges, start));
        self.bodies.insert(start, Rc::clone(&body));
        body
    }

    /// The start of the function part that holds an address.
    fn enclosing(&self, rva: u32) -> Option<u32> {
        let (start, end) = self.ranges.range(..=rva).next_back()?;
        (rva < *end).then_some(*start)
    }

    fn is_start(&self, rva: u32) -> bool {
        self.ranges.contains_key(&rva)
    }
}

/// Names the instruction at `index` by the instructions around it, as they stay when code moves.
pub fn context(steps: &[Step], index: usize, before: usize, after: usize) -> Vec<u8> {
    let first = index.saturating_sub(before);
    let last = (index + after).min(steps.len().saturating_sub(1));
    let mut key = vec![(index - first) as u8];
    for step in &steps[first..=last] {
        key.push(step.skeleton.len() as u8);
        key.extend_from_slice(&step.skeleton);
    }
    key
}

/// Where each call or reference of a body points, named by the instructions before it, in order.
fn named_sites(steps: &[Step]) -> HashMap<Vec<u8>, Vec<u32>> {
    let mut sites: HashMap<Vec<u8>, Vec<u32>> = HashMap::new();
    for (index, step) in steps.iter().enumerate() {
        if let Some(target) = step.call.or(step.reference) {
            sites
                .entry(context(steps, index, CONTEXT, 0))
                .or_default()
                .push(target);
        }
    }
    sites
}

/// Pairs what the two bodies of one function point at: sites named the same way, which each body
/// has the same number of, in order.
fn pair_sites(old: &HashMap<Vec<u8>, Vec<u32>>, new: &HashMap<Vec<u8>, Vec<u32>>) -> Vec<(u32, u32)> {
    let mut pairs = Vec::new();
    for (key, targets) in old {
        if let Some(others) = new.get(key).filter(|others| others.len() == targets.len()) {
            pairs.extend(targets.iter().copied().zip(others.iter().copied()));
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Addresses of the prior build with their places in the new one, found by following the calls
/// and references of functions known in both. An address reached two ways that disagree has none.
#[derive(Debug, Default)]
pub struct Followed {
    pub places: BTreeMap<u32, u32>,
    conflicts: BTreeSet<u32>,
}

impl Followed {
    /// Records a pair, and says whether it is new.
    fn add(&mut self, old: u32, new: u32) -> bool {
        if self.conflicts.contains(&old) {
            return false;
        }
        match self.places.get(&old) {
            Some(place) if *place == new => false,
            Some(_) => {
                self.places.remove(&old);
                self.conflicts.insert(old);
                false
            }
            None => {
                self.places.insert(old, new);
                true
            }
        }
    }

    fn get(&self, old: u32) -> Option<u32> {
        self.places.get(&old).copied()
    }
}

/// Follows calls and references from the functions known in both builds, a few levels deep.
pub fn follow(old: &mut Side, new: &mut Side, known: &BTreeMap<u32, u32>) -> Followed {
    let mut followed = Followed::default();
    let mut frontier: Vec<(u32, u32)> = known
        .iter()
        .map(|(from, to)| (*from, *to))
        .filter(|(from, to)| old.pe.is_code(*from) && new.pe.is_code(*to))
        .collect();
    let mut visited = BTreeSet::new();
    for _ in 0..FOLLOW_DEPTH {
        let mut next = Vec::new();
        for (from_old, from_new) in frontier {
            if visited.len() >= FOLLOW_LIMIT || !visited.insert(from_old) {
                continue;
            }
            let old_sites = named_sites(&old.body(from_old));
            let new_sites = named_sites(&new.body(from_new));
            for (to_old, to_new) in pair_sites(&old_sites, &new_sites) {
                if known.contains_key(&to_old) {
                    continue;
                }
                if followed.add(to_old, to_new) && old.pe.is_code(to_old) && new.pe.is_code(to_new)
                {
                    next.push((to_old, to_new));
                }
            }
        }
        frontier = next;
    }
    followed
}

/// The calls a function makes, as its prior body names them, to functions whose place in the new
/// build is known: `(site, rank among sites named the same, how many such sites, new place)`.
fn expected_calls(
    old: &mut Side,
    target: u32,
    anchors: &BTreeMap<u32, u32>,
) -> Vec<(Vec<u8>, usize, usize, u32)> {
    let body = old.body(target);
    let mut expected = Vec::new();
    for (key, callees) in named_sites(&body) {
        for (rank, callee) in callees.iter().enumerate() {
            if let Some(place) = anchors.get(callee) {
                expected.push((key.clone(), rank, callees.len(), *place));
            }
        }
    }
    expected.sort();
    expected
}

/// The one candidate that makes the expected calls from the same spots, and contradicts none.
fn by_its_calls(
    new: &mut Side,
    expected: &[(Vec<u8>, usize, usize, u32)],
    candidates: &[u32],
) -> Vec<u32> {
    if expected.is_empty() {
        return vec![];
    }
    let least = expected.len().min(2);
    let mut passing = Vec::new();
    for candidate in candidates {
        let theirs = named_sites(&new.body(*candidate));
        let (mut hits, mut misses) = (0, 0);
        for (key, rank, count, place) in expected {
            match theirs.get(key) {
                Some(found) if found.len() == *count => {
                    if found[*rank] == *place {
                        hits += 1;
                    } else {
                        misses += 1;
                    }
                }
                _ => {}
            }
        }
        if misses == 0 && hits >= least {
            passing.push(*candidate);
        }
    }
    // Two candidates that both pass cannot be told apart: neither is an answer.
    if passing.len() == 1 {
        passing
    } else {
        vec![]
    }
}

/// The functions of a build that call any of `wanted` directly, by the function part each call is in.
fn calls_into(side: &Side, wanted: &BTreeSet<u32>) -> BTreeMap<u32, BTreeSet<u32>> {
    let mut found: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    if wanted.is_empty() {
        return found;
    }
    for (base, bytes, is_code) in side.pe.sections() {
        if !is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(4) {
            if !matches!(bytes[at], 0xE8 | 0xE9) {
                continue;
            }
            let distance = i32::from_le_bytes([bytes[at + 1], bytes[at + 2], bytes[at + 3], bytes[at + 4]]);
            let site = base + at as u32;
            let Ok(target) = u32::try_from(i64::from(site) + 5 + i64::from(distance)) else {
                continue;
            };
            if wanted.contains(&target) {
                if let Some(start) = side.enclosing(site) {
                    found.entry(target).or_default().insert(start);
                }
            }
        }
    }
    found
}

/// The functions the prior bodies of `targets` call that their own fingerprints find in exactly one
/// place of the new build. They do not depend on the calls followed from known functions.
fn callee_anchors(old: &mut Side, new: &Side, targets: &[u32]) -> BTreeMap<u32, u32> {
    let mut prints = BTreeMap::new();
    for target in targets {
        let body = old.body(*target);
        for step in body.iter() {
            let callee = step
                .call
                .or(step.reference.filter(|place| old.pe.is_code(*place)));
            if let Some(callee) = callee {
                if let Some(print) = code_print(old.pe, callee) {
                    prints.insert(callee, print);
                }
            }
        }
    }
    let mut anchors = BTreeMap::new();
    for (callee, places) in find_prints(new.pe, &prints) {
        // A print taken at a function start counts only function starts, as for the row itself.
        let places: Vec<u32> = places
            .into_iter()
            .filter(|place| !old.is_start(callee) || new.is_start(*place))
            .collect();
        if let [only] = places.as_slice() {
            anchors.insert(callee, *only);
        }
    }
    anchors
}

/// A function as the slots of a class table show it: its fingerprint, or the whole of a short one.
fn entry_shape(pe: &PeFile, function: u32) -> Option<Vec<u8>> {
    code_print(pe, function).or_else(|| whole_function(pe, function))
}

fn same_function(old: &Side, new: &Side, known: &BTreeMap<u32, u32>, from: u32, to: u32) -> bool {
    if let Some(place) = known.get(&from) {
        return *place == to;
    }
    match (entry_shape(old.pe, from), entry_shape(new.pe, to)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// The function in a slot of a class table, or `None` past the table's end.
fn entry(pe: &PeFile, table: u32, slot: i64) -> Option<u32> {
    let slot = u32::try_from(slot).ok()?;
    let at = table.checked_add(slot.checked_mul(8)?)?;
    pe.pointer_rva(at).filter(|function| pe.is_code(*function))
}

/// How far slot `slot` of the old table moved in the new one, judged by the slots around it.
fn line_up(
    old: &Side,
    new: &Side,
    known: &BTreeMap<u32, u32>,
    tables: (u32, u32),
    slot: u32,
) -> Option<i64> {
    let mut scores = Vec::new();
    for shift in -ALIGN_SHIFT..=ALIGN_SHIFT {
        let mut score = 0;
        for near in slot.saturating_sub(ALIGN_REACH)..=slot + ALIGN_REACH {
            if near == slot {
                continue;
            }
            let from = entry(old.pe, tables.0, i64::from(near));
            let to = entry(new.pe, tables.1, i64::from(near) + shift);
            if let (Some(from), Some(to)) = (from, to) {
                if same_function(old, new, known, from, to) {
                    score += 1;
                }
            }
        }
        scores.push((score, shift));
    }
    scores.sort_by(|left, right| right.0.cmp(&left.0));
    match scores.as_slice() {
        [(top, shift), (second, _), ..] if *top >= ALIGN_LEAST && top > second => Some(*shift),
        _ => None,
    }
}

/// The places the class tables known in both builds give a function by its slot.
fn by_table_slot(
    old: &Side,
    new: &Side,
    known: &BTreeMap<u32, u32>,
    tables: &[(u32, u32)],
    target: u32,
) -> BTreeSet<u32> {
    let mut found = BTreeSet::new();
    for pair in tables {
        for slot in 0..TABLE_SLOTS {
            let Some(function) = entry(old.pe, pair.0, i64::from(slot)) else {
                break;
            };
            if function != target {
                continue;
            }
            if let Some(shift) = line_up(old, new, known, *pair, slot) {
                if let Some(place) = entry(new.pe, pair.1, i64::from(slot) + shift) {
                    found.insert(place);
                }
            }
        }
    }
    found
}

/// The candidate tables whose slots hold the same functions as the old table, and how many agree.
fn by_slots(
    old: &Side,
    new: &Side,
    known: &BTreeMap<u32, u32>,
    target: u32,
    candidates: &[u32],
) -> Vec<u32> {
    let mut passing = Vec::new();
    for candidate in candidates {
        let (mut seen, mut agreed) = (0, 0);
        for slot in 0..TABLE_SEEN {
            let Some(function) = entry(old.pe, target, i64::from(slot)) else {
                break;
            };
            seen += 1;
            let agrees = (-1..=1).any(|shift| {
                entry(new.pe, *candidate, i64::from(slot) + shift)
                    .is_some_and(|place| same_function(old, new, known, function, place))
            });
            if agrees {
                agreed += 1;
            }
        }
        if agreed >= ALIGN_LEAST && agreed * 2 >= seen {
            passing.push(*candidate);
        }
    }
    passing
}

/// Tables of the new build that hold, near the same slot, a function the old table holds and
/// whose place is known.
fn table_candidates(old: &Side, new: &Side, known: &BTreeMap<u32, u32>, target: u32) -> Vec<u32> {
    let mut found = BTreeSet::new();
    let mut tried = 0;
    for slot in 0..TABLE_SEEN {
        let Some(function) = entry(old.pe, target, i64::from(slot)) else {
            break;
        };
        let Some(place) = known.get(&function).filter(|place| new.pe.is_code(**place)) else {
            continue;
        };
        for shifted in slot.saturating_sub(1)..=slot + 1 {
            found.extend(find_table(new.pe, shifted, *place));
        }
        tried += 1;
        if tried == 2 {
            break;
        }
    }
    found.into_iter().take(CANDIDATES_MOST).collect()
}

/// A table a function known in both builds loads from the same spot, or a place a few slots into
/// one, which keeps its distance from the table's start.
fn by_reference(followed: &Followed, target: u32) -> Option<u32> {
    let (from, to) = followed
        .places
        .range(target.saturating_sub(0x40)..=target)
        .next_back()?;
    to.checked_add(target - from)
}

/// The text at an address of a data section, with its terminating zero, when it is plain text.
fn text_at(pe: &PeFile, rva: u32) -> Option<Vec<u8>> {
    if pe.is_code(rva) {
        return None;
    }
    let window = pe
        .read(rva, TEXT_MOST + 1)
        .or_else(|| pe.read(rva, TEXT_LEAST + 1))?;
    let end = window.iter().position(|byte| *byte == 0)?;
    let text = &window[..end];
    if text.len() < TEXT_LEAST || !text.iter().all(|byte| (0x20..0x7F).contains(byte)) {
        return None;
    }
    Some(window[..=end].to_vec())
}

/// For each text, the functions of a build that load it, when the build holds it exactly once.
fn functions_using(side: &Side, texts: &BTreeSet<Vec<u8>>) -> BTreeMap<Vec<u8>, BTreeSet<u32>> {
    let mut by_head: HashMap<[u8; TEXT_LEAST], Vec<&Vec<u8>>> = HashMap::new();
    let mut first = [false; 256];
    for text in texts {
        let head: [u8; TEXT_LEAST] = text[..TEXT_LEAST].try_into().expect("checked length");
        first[head[0] as usize] = true;
        by_head.entry(head).or_default().push(text);
    }
    let mut places: BTreeMap<&Vec<u8>, Vec<u32>> = BTreeMap::new();
    for (base, bytes, is_code) in side.pe.sections() {
        if is_code {
            continue;
        }
        for at in 0..bytes.len().saturating_sub(TEXT_LEAST) {
            if !first[bytes[at] as usize] {
                continue;
            }
            let head: [u8; TEXT_LEAST] = bytes[at..at + TEXT_LEAST].try_into().expect("in range");
            for text in by_head.get(&head).map_or(&[][..], Vec::as_slice) {
                if bytes[at..].starts_with(text) {
                    places.entry(*text).or_default().push(base + at as u32);
                }
            }
        }
    }
    let at_address: HashMap<u32, &Vec<u8>> = places
        .iter()
        .filter_map(|(text, found)| match found.as_slice() {
            [only] => Some((*only, *text)),
            _ => None,
        })
        .collect();
    let mut users: BTreeMap<Vec<u8>, BTreeSet<u32>> = BTreeMap::new();
    for (base, bytes, is_code) in side.pe.sections() {
        if !is_code {
            continue;
        }
        // `lea` or `mov` of a register from [rip + disp32]: REX, 8D or 8B, a ModRM of 00 reg 101.
        for at in 3..bytes.len().saturating_sub(4) {
            if bytes[at - 1] & 0xC7 != 0x05
                || !matches!(bytes[at - 2], 0x8D | 0x8B)
                || bytes[at - 3] & 0xF0 != 0x40
            {
                continue;
            }
            let distance = i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            let site = base + at as u32;
            let Ok(target) = u32::try_from(i64::from(site) + 4 + i64::from(distance)) else {
                continue;
            };
            if let Some(text) = at_address.get(&target) {
                if let Some(start) = side.enclosing(site) {
                    users.entry((*text).clone()).or_default().insert(start);
                }
            }
        }
    }
    users
}

/// Which method gave a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Method {
    CallsInto,
    ItsCalls,
    TableSlot,
    Text,
    References,
    Slots,
}

impl fmt::Display for Method {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        out.write_str(match self {
            Method::CallsInto => "calls into it",
            Method::ItsCalls => "its calls",
            Method::TableSlot => "its table slot",
            Method::Text => "a text it uses",
            Method::References => "references to it",
            Method::Slots => "its slots",
        })
    }
}

fn names(methods: &BTreeSet<Method>) -> String {
    methods
        .iter()
        .map(Method::to_string)
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Takes a place only when two different methods name it, no method names another, and it is one
/// of the fingerprint's places where there are any.
pub fn decide(
    answers: &[(Method, u32)],
    candidates: Option<&[u32]>,
) -> Result<(u32, BTreeSet<Method>), String> {
    let places: BTreeSet<u32> = answers.iter().map(|(_, place)| *place).collect();
    let methods: BTreeSet<Method> = answers.iter().map(|(method, _)| *method).collect();
    match places.iter().copied().collect::<Vec<_>>().as_slice() {
        [] => Err("no method found it".to_owned()),
        [place] => {
            if candidates.is_some_and(|candidates| !candidates.contains(place)) {
                Err(format!(
                    "{} found {place:#x}, which is not one of its fingerprint's places",
                    names(&methods)
                ))
            } else if methods.len() < 2 {
                Err(format!(
                    "only {} found it, at {place:#x}, and a second method is needed",
                    names(&methods)
                ))
            } else {
                Ok((*place, methods))
            }
        }
        _ => {
            let mut said: Vec<String> = answers
                .iter()
                .map(|(method, place)| format!("{method} {place:#x}"))
                .collect();
            said.sort();
            said.dedup();
            Err(format!("the methods disagree: {}", said.join(", ")))
        }
    }
}

/// What an address of the prior row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Function,
    Table,
}

/// An address the fingerprints did not pin down, with the places they matched, if any.
#[derive(Debug, Clone)]
pub struct Wanted {
    pub kind: Kind,
    pub candidates: Option<Vec<u32>>,
}

/// What the methods found.
#[derive(Debug, Default)]
pub struct Resolution {
    /// The new place of each address two methods agree on.
    pub places: BTreeMap<u32, u32>,
    /// For the report: each place and the methods that found it.
    pub settled: Vec<String>,
    /// Why each other address has no place.
    pub reasons: BTreeMap<u32, String>,
    /// Functions of the prior build with their places in the new one, as the calls of known
    /// functions pair them, for reading values from the same instructions in both builds.
    pub followed: BTreeMap<u32, u32>,
}

/// Looks for each wanted address with every method, in rounds, since a place found in one round
/// is known in the next. `known` holds the places the fingerprints found, and `tables` names the
/// prior row's addresses that are class tables.
pub fn resolve(
    old_pe: &PeFile,
    new_pe: &PeFile,
    known: &BTreeMap<u32, u32>,
    wanted: &BTreeMap<u32, Wanted>,
    tables: &BTreeSet<u32>,
) -> Resolution {
    let mut old = Side::new(old_pe);
    let mut new = Side::new(new_pe);
    let mut known = known.clone();
    let mut resolution = Resolution::default();
    let functions: Vec<u32> = wanted
        .iter()
        .filter(|(_, want)| want.kind == Kind::Function)
        .map(|(target, _)| *target)
        .collect();

    // What does not depend on the known places is read once: the callees found by their own
    // fingerprints, the functions that call them, and the texts each function uses.
    let anchors = callee_anchors(&mut old, &new, &functions);
    let mut texts_of: BTreeMap<u32, BTreeSet<Vec<u8>>> = BTreeMap::new();
    for target in &functions {
        let body = old.body(*target);
        let texts: BTreeSet<Vec<u8>> = body
            .iter()
            .filter_map(|step| step.reference)
            .filter_map(|place| text_at(old.pe, place))
            .collect();
        texts_of.insert(*target, texts);
    }
    let all_texts: BTreeSet<Vec<u8>> = texts_of.values().flatten().cloned().collect();
    let users = functions_using(&new, &all_texts);
    let mut seen_anchors = anchors.clone();
    for (from, to) in &known {
        if old.pe.is_code(*from) {
            seen_anchors.insert(*from, *to);
        }
    }
    let mut expected = BTreeMap::new();
    let mut called = BTreeSet::new();
    for target in &functions {
        let calls = expected_calls(&mut old, *target, &seen_anchors);
        called.extend(calls.iter().map(|call| call.3));
        expected.insert(*target, calls);
    }
    let callers = calls_into(&new, &called);

    let mut pending: BTreeSet<u32> = wanted.keys().copied().collect();
    for _ in 0..ROUNDS {
        let followed = follow(&mut old, &mut new, &known);
        let table_pairs: Vec<(u32, u32)> = known
            .iter()
            .filter(|(from, _)| tables.contains(from))
            .map(|(from, to)| (*from, *to))
            .collect();
        let mut found_any = false;
        for target in pending.clone() {
            let want = &wanted[&target];
            let mut answers = Vec::new();
            match want.kind {
                Kind::Function => {
                    if let Some(place) = followed.get(target) {
                        answers.push((Method::CallsInto, place));
                    }
                    let calls = &expected[&target];
                    let candidates: Vec<u32> = match &want.candidates {
                        Some(list) => list.clone(),
                        None => calls
                            .iter()
                            .flat_map(|call| callers.get(&call.3).into_iter().flatten().copied())
                            .collect::<BTreeSet<u32>>()
                            .into_iter()
                            .take(CANDIDATES_MOST)
                            .collect(),
                    };
                    for place in by_its_calls(&mut new, calls, &candidates) {
                        answers.push((Method::ItsCalls, place));
                    }
                    for place in by_table_slot(&old, &new, &known, &table_pairs, target) {
                        answers.push((Method::TableSlot, place));
                    }
                    if let Some(place) = by_text(&texts_of[&target], &users) {
                        answers.push((Method::Text, place));
                    }
                }
                Kind::Table => {
                    if let Some(place) = by_reference(&followed, target) {
                        answers.push((Method::References, place));
                    }
                    let candidates = match &want.candidates {
                        Some(list) => list.clone(),
                        None => table_candidates(&old, &new, &known, target),
                    };
                    for place in by_slots(&old, &new, &known, target, &candidates) {
                        answers.push((Method::Slots, place));
                    }
                }
            }
            match decide(&answers, want.candidates.as_deref()) {
                Ok((place, methods)) => {
                    known.insert(target, place);
                    resolution.places.insert(target, place);
                    resolution.settled.push(format!(
                        "{target:#x} -> {place:#x} by {}",
                        names(&methods)
                    ));
                    resolution.reasons.remove(&target);
                    pending.remove(&target);
                    found_any = true;
                }
                Err(reason) => {
                    resolution.reasons.insert(target, reason);
                }
            }
        }
        resolution.followed = followed.places;
        if !found_any || pending.is_empty() {
            break;
        }
    }
    resolution
}

/// The one function that uses every text of a target the new build holds once.
fn by_text(texts: &BTreeSet<Vec<u8>>, users: &BTreeMap<Vec<u8>, BTreeSet<u32>>) -> Option<u32> {
    let mut common: Option<BTreeSet<u32>> = None;
    for text in texts {
        let Some(found) = users.get(text) else {
            continue;
        };
        common = Some(match common {
            Some(common) => common.intersection(found).copied().collect(),
            None => found.clone(),
        });
    }
    let common = common?;
    match common.iter().copied().collect::<Vec<_>>().as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// How many instructions on each side of a use name it within its body.
const VALUE_CONTEXT: usize = 2;

/// Which operand of an instruction holds a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operand {
    Displacement,
    Immediate,
}

/// A value of the prior row read again from the new build, or why it could not be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub path: String,
    pub prior: u64,
    /// The new value and how it was read.
    pub value: Result<(u64, String), String>,
}

fn keys(steps: &[Step]) -> Vec<Vec<u8>> {
    (0..steps.len())
        .map(|index| context(steps, index, VALUE_CONTEXT, VALUE_CONTEXT))
        .collect()
}

/// Whether an instruction uses `wanted`: as a member displacement or an immediate for a plain
/// value, or as the displacement of a call through a table for a slot, which is eight bytes each.
fn use_of(step: &Step, wanted: i64, slot: bool) -> Option<Operand> {
    if step.stack {
        return None;
    }
    if slot {
        return (step.indirect && step.displacement == Some(wanted)).then_some(Operand::Displacement);
    }
    if step.displacement == Some(wanted) {
        Some(Operand::Displacement)
    } else if step.immediate == Some(wanted) {
        Some(Operand::Immediate)
    } else {
        None
    }
}

/// Reads one value from the instructions that use it. Each use of `prior` in a prior function body
/// is named by the instructions around it, with their own operands cleared, and the same
/// instruction is looked up in the function's new body. A value is taken only when at least two
/// uses are read and every use read gives the same value.
fn read_value(
    old: &mut Side,
    new: &mut Side,
    pairs: &[(u32, u32)],
    prior: u64,
    slot: bool,
) -> Result<(u64, String), String> {
    if prior == 0 {
        return Err("zero is used too widely in code to be told apart".to_owned());
    }
    let wanted = if slot { prior.checked_mul(8) } else { Some(prior) }
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| format!("{prior:#x} is too large to be an operand"))?;
    let mut found: Vec<(u64, u32)> = Vec::new();
    let mut uses = 0;
    for (from, to) in pairs {
        let old_body = old.body(*from);
        let new_body = new.body(*to);
        let old_keys = keys(&old_body);
        let new_keys = keys(&new_body);
        for (index, step) in old_body.iter().enumerate() {
            let Some(operand) = use_of(step, wanted, slot) else {
                continue;
            };
            uses += 1;
            let key = &old_keys[index];
            let rank = old_keys[..index].iter().filter(|other| *other == key).count();
            let count = old_keys.iter().filter(|other| *other == key).count();
            let theirs: Vec<usize> = new_keys
                .iter()
                .enumerate()
                .filter(|(_, other)| *other == key)
                .map(|(at, _)| at)
                .collect();
            if theirs.len() != count {
                continue;
            }
            let other = &new_body[theirs[rank]];
            let read = match operand {
                Operand::Displacement => other.displacement,
                Operand::Immediate => other.immediate,
            };
            let Some(read) = read.and_then(|read| u64::try_from(read).ok()) else {
                continue;
            };
            if slot && read % 8 != 0 {
                continue;
            }
            found.push((if slot { read / 8 } else { read }, *from));
        }
    }
    let values: BTreeSet<u64> = found.iter().map(|(value, _)| *value).collect();
    let functions: BTreeSet<u32> = found.iter().map(|(_, function)| *function).collect();
    match values.iter().copied().collect::<Vec<_>>().as_slice() {
        [] if uses == 0 => Err(format!(
            "no instruction of the {} functions known in both builds uses {prior:#x}",
            pairs.len()
        )),
        [] => Err(format!(
            "its {uses} uses in the prior build were not found in the new one"
        )),
        [value] if found.len() >= 2 => Ok((
            *value,
            format!(
                "read at {} uses in {} of its functions",
                found.len(),
                functions.len()
            ),
        )),
        [value] => Err(format!(
            "read at only one use, as {value:#x}, and a second reading is needed"
        )),
        _ => Err(format!("its uses disagree: {values:#x?}")),
    }
}

/// Reads each value of the prior row that has no recipe from the instructions that use it in the
/// functions of its part of the row, and in the functions they call, in both builds. `sections`
/// pairs the prior and new places of each part's functions, and `followed` pairs the functions
/// the calls of known functions reach. A value's part is the first name of its path, and a value
/// whose name ends in `_slot` is a slot of a class table.
pub fn read_values(
    old_pe: &PeFile,
    new_pe: &PeFile,
    sections: &BTreeMap<String, Vec<(u32, u32)>>,
    followed: &BTreeMap<u32, u32>,
    values: &[(String, u64)],
) -> Vec<Reading> {
    let mut old = Side::new(old_pe);
    let mut new = Side::new(new_pe);
    let mut pairs_of: BTreeMap<&str, Vec<(u32, u32)>> = BTreeMap::new();
    for (section, pairs) in sections {
        let mut all: BTreeSet<(u32, u32)> = pairs.iter().copied().collect();
        for (from, _) in pairs {
            let body = old.body(*from);
            for step in body.iter() {
                if let Some(place) = step.call.and_then(|callee| followed.get(&callee)) {
                    all.insert((step.call.expect("checked"), *place));
                }
            }
        }
        pairs_of.insert(section.as_str(), all.into_iter().collect());
    }
    let mut readings = Vec::new();
    for (path, prior) in values {
        let section = path.split('.').next().unwrap_or_default();
        let name = path.rsplit('.').next().unwrap_or_default();
        let slot = name.split('[').next().unwrap_or_default().ends_with("_slot");
        let pairs = pairs_of.get(section).map_or(&[][..], Vec::as_slice);
        readings.push(Reading {
            path: path.clone(),
            prior: *prior,
            value: read_value(&mut old, &mut new, pairs, *prior, slot),
        });
    }
    readings
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const IMAGE_BASE: u64 = 0x1_8000_0000;
    /// Where a fixture keeps its exception table, in its data section.
    const EXCEPTIONS: u32 = 0x20f000;

    fn place(rva: u32) -> usize {
        if rva >= 0x200000 {
            0x10400 + (rva - 0x200000) as usize
        } else {
            0x400 + (rva - 0x100000) as usize
        }
    }

    /// A 64-bit module with a code section at 0x100000 and a data section at 0x200000, holding
    /// the given bytes and an exception table that lists `ranges`.
    pub fn module(parts: &[(u32, Vec<u8>)], ranges: &[(u32, u32)]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x400];
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        bytes[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[0x98 + 24..0x98 + 32].copy_from_slice(&IMAGE_BASE.to_le_bytes());
        let sections = [
            (0x100000u32, 0x400u32, 0x2000_0000u32),
            (0x200000, 0x10400, 0x4000_0000),
        ];
        for (index, (rva, raw, flags)) in sections.iter().enumerate() {
            let header = 0x98 + 240 + 40 * index;
            bytes[header + 8..header + 12].copy_from_slice(&0x10000u32.to_le_bytes());
            bytes[header + 12..header + 16].copy_from_slice(&rva.to_le_bytes());
            bytes[header + 16..header + 20].copy_from_slice(&0x10000u32.to_le_bytes());
            bytes[header + 20..header + 24].copy_from_slice(&raw.to_le_bytes());
            bytes[header + 36..header + 40].copy_from_slice(&flags.to_le_bytes());
        }
        bytes.resize(0x400 + 0x20000, 0);
        bytes[0x400..0x10400].fill(0xCC);
        let mut table = vec![];
        for (start, end) in ranges {
            table.extend_from_slice(&start.to_le_bytes());
            table.extend_from_slice(&end.to_le_bytes());
            table.extend_from_slice(&0u32.to_le_bytes());
        }
        let directory = 0x98 + 112 + 24;
        bytes[directory..directory + 4].copy_from_slice(&EXCEPTIONS.to_le_bytes());
        bytes[directory + 4..directory + 8].copy_from_slice(&(table.len() as u32).to_le_bytes());
        let at = place(EXCEPTIONS);
        bytes[at..at + table.len()].copy_from_slice(&table);
        for (rva, data) in parts {
            let at = place(*rva);
            bytes[at..at + data.len()].copy_from_slice(data);
        }
        bytes
    }

    pub fn call(from: u32, to: u32) -> Vec<u8> {
        let mut bytes = vec![0xE8];
        bytes.extend_from_slice(&((i64::from(to) - i64::from(from) - 5) as i32).to_le_bytes());
        bytes
    }

    pub fn table(functions: &[u32]) -> Vec<u8> {
        functions
            .iter()
            .flat_map(|function| (IMAGE_BASE + u64::from(*function)).to_le_bytes())
            .collect()
    }

    /// sub rsp, 28h; call `to`; add rsp, 28h; ret.
    fn caller(at: u32, to: u32) -> Vec<u8> {
        let mut bytes = vec![0x48, 0x83, 0xEC, 0x28];
        bytes.extend(call(at + 4, to));
        bytes.extend([0x48, 0x83, 0xC4, 0x28, 0xC3]);
        bytes
    }

    /// push rsi; sub rsp, 20h; mov rsi, rcx; add rsp, 20h; pop rsi; ret.
    const TARGET: [u8; 14] = [
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x83, 0xC4, 0x20, 0x5E, 0xC3,
    ];

    /// `mov eax, n; ret`: a different small function for each slot of a table.
    fn small(n: u8) -> Vec<u8> {
        vec![0xB8, n, 0, 0, 0, 0xC3]
    }

    const OLD_CALLER: u32 = 0x101000;
    const OLD_TARGET: u32 = 0x101100;
    const OLD_TABLE: u32 = 0x200100;
    const NEW_CALLER: u32 = 0x103000;
    const NEW_TARGETS: [u32; 2] = [0x103100, 0x103200];
    const NEW_TABLE: u32 = 0x200800;

    /// The prior build: a caller of the target, and a table that holds it in slot 3.
    fn old_module() -> Vec<u8> {
        let mut parts = vec![
            (OLD_CALLER, caller(OLD_CALLER, OLD_TARGET)),
            (OLD_TARGET, TARGET.to_vec()),
        ];
        let mut slots = vec![];
        for n in 0..6u8 {
            let at = 0x101400 + 0x10 * u32::from(n);
            parts.push((at, small(n)));
            slots.push(at);
        }
        slots.insert(3, OLD_TARGET);
        parts.push((OLD_TABLE, table(&slots)));
        module(
            &parts,
            &[(OLD_CALLER, OLD_CALLER + 14), (OLD_TARGET, OLD_TARGET + 14)],
        )
    }

    /// The new build: the target's bytes in two places, a caller of the first, and a table that
    /// holds `in_table` in slot 3.
    fn new_module(in_table: u32) -> Vec<u8> {
        let mut parts = vec![
            (NEW_CALLER, caller(NEW_CALLER, NEW_TARGETS[0])),
            (NEW_TARGETS[0], TARGET.to_vec()),
            (NEW_TARGETS[1], TARGET.to_vec()),
        ];
        let mut slots = vec![];
        for n in 0..6u8 {
            let at = 0x103400 + 0x10 * u32::from(n);
            parts.push((at, small(n)));
            slots.push(at);
        }
        slots.insert(3, in_table);
        parts.push((NEW_TABLE, table(&slots)));
        let ranges = [
            (NEW_CALLER, NEW_CALLER + 14),
            (NEW_TARGETS[0], NEW_TARGETS[0] + 14),
            (NEW_TARGETS[1], NEW_TARGETS[1] + 14),
        ];
        module(&parts, &ranges)
    }

    fn run(new_bytes: &[u8], known_table: bool) -> Resolution {
        let old_bytes = old_module();
        let old = PeFile::parse(&old_bytes).unwrap();
        let new = PeFile::parse(new_bytes).unwrap();
        let mut known = BTreeMap::from([(OLD_CALLER, NEW_CALLER)]);
        if known_table {
            known.insert(OLD_TABLE, NEW_TABLE);
        }
        let wanted = BTreeMap::from([(
            OLD_TARGET,
            Wanted {
                kind: Kind::Function,
                candidates: Some(NEW_TARGETS.to_vec()),
            },
        )]);
        resolve(&old, &new, &known, &wanted, &BTreeSet::from([OLD_TABLE]))
    }

    #[test]
    fn nfr6_a_function_with_several_places_takes_the_one_two_methods_agree_on() {
        let resolution = run(&new_module(NEW_TARGETS[0]), true);
        assert_eq!(resolution.places.get(&OLD_TARGET), Some(&NEW_TARGETS[0]));
        assert_eq!(
            resolution.settled,
            ["0x101100 -> 0x103100 by calls into it and its table slot"]
        );
        assert!(resolution.reasons.is_empty(), "{:?}", resolution.reasons);
    }

    #[test]
    fn nfr6_methods_that_disagree_leave_the_function_unresolved() {
        let resolution = run(&new_module(NEW_TARGETS[1]), true);
        assert!(resolution.places.is_empty());
        let reason = &resolution.reasons[&OLD_TARGET];
        assert!(reason.starts_with("the methods disagree: "), "{reason}");
        assert!(reason.contains("calls into it 0x103100"), "{reason}");
        assert!(reason.contains("its table slot 0x103200"), "{reason}");
    }

    #[test]
    fn nfr6_one_method_alone_does_not_place_a_function() {
        let resolution = run(&new_module(NEW_TARGETS[0]), false);
        assert!(resolution.places.is_empty());
        let reason = &resolution.reasons[&OLD_TARGET];
        assert!(reason.starts_with("only calls into it found it"), "{reason}");
    }

    #[test]
    fn nfr6_a_place_outside_the_fingerprint_matches_is_refused() {
        let answers = [(Method::CallsInto, 0x103300), (Method::TableSlot, 0x103300)];
        let refused = decide(&answers, Some(&NEW_TARGETS[..])).unwrap_err();
        assert!(refused.contains("not one of its fingerprint's places"), "{refused}");
        let (place, methods) = decide(&answers, None).unwrap();
        assert_eq!(place, 0x103300);
        assert_eq!(methods.len(), 2);
        assert_eq!(decide(&[], None).unwrap_err(), "no method found it");
    }
}
