//! Finds the values of a row in one browser build, from that build's own binary alone.
//!
//! Each value has a description in `rows/descriptions.json`, written from Chromium's public
//! source: the class name its `ui::metadata` gives a views class, the name a `TRACE_EVENT` gives a
//! function, a slot that a public header's declaration order gives, and the readers of
//! `row_recipe`. A description names the source file and the release tag it was written from. No
//! other build is read, and no address of another row is used as an answer.
//!
//! Every step must leave exactly one candidate. A step that leaves none, or more than one, gives
//! no value, and the value is reported as not found with the reason. A description may list
//! variations, such as a trace name before and after a rename, and every variation is tried on
//! every build: the candidates of all of them together must still be exactly one.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chrome_build::PeFile;
use crate::{row_class, row_recipe, x64};

/// The checked-in descriptions.
pub const DESCRIPTIONS: &str = include_str!("../rows/descriptions.json");

/// The version of the description format this finder reads.
pub const SCHEMA: u32 = 1;

/// How many slots of a class table are read at most.
const TABLE_SLOTS: u32 = 512;
/// How far a function body is read for its calls.
const BODY_MOST: u32 = 0x4000;
/// How many parts a chain of moved function parts may have before it counts as broken.
const PATH_DEPTH: usize = 32;

/// The description file.
#[derive(Debug, Clone, Deserialize)]
pub struct Descriptions {
    pub schema: u32,
    pub values: Vec<Description>,
}

/// How to find one value of a row.
#[derive(Debug, Clone, Deserialize)]
pub struct Description {
    /// The value's path in a row: `frame.<field>` for the frame part, `<section>.<field>` for the
    /// build part. A path that starts with `_` is a step other values use and not a row value.
    pub path: String,
    /// The public Chromium source files the description was written from.
    pub source: Vec<String>,
    /// The Chromium release tag of those files.
    pub tag: String,
    /// Why the description reads the build this way, in a sentence.
    #[serde(default)]
    pub note: String,
    #[serde(flatten)]
    pub how: How,
}

/// The ways a value is found.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum How {
    /// The class table of a views class, by the class names `METADATA_HEADER` gives it. With a
    /// slot, the function in that slot of the table.
    Class {
        names: Vec<String>,
        #[serde(default)]
        part: Part,
        #[serde(default)]
        slot: Option<u32>,
    },
    /// The function that uses one of the texts, such as a `TRACE_EVENT` name, then the steps.
    Trace {
        texts: Vec<String>,
        #[serde(default)]
        steps: Vec<Step>,
    },
    /// The class table that holds the function another value found at the slot.
    TableOf { function: String, slot: u32 },
    /// The function at a slot of a class table another value found.
    TableSlot { table: String, slot: u32 },
    /// The slot of a function in a class table, both found by other values.
    SlotOf { table: String, function: String },
    /// The member an accessor another value found reads from `this` with its first instruction.
    FirstLoad { function: String },
    /// The size every direct caller of a constructor another value found allocates it with.
    AllocationSize { constructor: String },
}

/// Which table of a views object a class description means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// The table at the object's start.
    #[default]
    Object,
    /// The table of the object's `views::View` part.
    View,
}

/// A step from one function to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Up to this many times, the function that calls this one from its only call site, while
    /// nothing takes this one's address. An optimiser may move a body into a helper with one
    /// caller. Stopping early is not a failure.
    SoleCaller(u32),
    /// The only function that calls this one and is no larger than this many bytes.
    OnlyCallerAtMostBytes(u32),
    /// The function that calls this one and is itself called from the most places, when it has at
    /// least this many times as many call sites as the next one.
    MostCallersLead(u32),
    /// The function this one calls at this place, counting its direct calls from 0.
    CalleeNth(usize),
}

impl How {
    /// The values this one is found from.
    pub fn needs(&self) -> Vec<&str> {
        match self {
            How::Class { .. } | How::Trace { .. } => vec![],
            How::TableOf { function, .. } => vec![function],
            How::TableSlot { table, .. } => vec![table],
            How::SlotOf { table, function } => vec![table, function],
            How::FirstLoad { function } => vec![function],
            How::AllocationSize { constructor } => vec![constructor],
        }
    }
}

/// Reads a description file and checks that every value is described once, from a named source.
pub fn parse(text: &str) -> Result<Vec<Description>, String> {
    let file: Descriptions =
        serde_json::from_str(text).map_err(|error| format!("the descriptions are not valid: {error}"))?;
    if file.schema != SCHEMA {
        return Err(format!(
            "the descriptions are schema {}, and this finder reads {SCHEMA}",
            file.schema
        ));
    }
    let mut paths = HashSet::new();
    for description in &file.values {
        if !paths.insert(description.path.as_str()) {
            return Err(format!("{} is described twice", description.path));
        }
        if description.source.is_empty() || description.tag.is_empty() {
            return Err(format!(
                "{} names no public source file and tag",
                description.path
            ));
        }
        let empty = match &description.how {
            How::Class { names, .. } => names.is_empty(),
            How::Trace { texts, .. } => texts.is_empty(),
            _ => false,
        };
        if empty {
            return Err(format!("{} has nothing to look for", description.path));
        }
    }
    // A value may need one that has no description yet. It is then reported as not found, with
    // the value it waits for, so the description is ready when its input gets one.
    Ok(file.values)
}

/// The checked-in descriptions.
pub fn descriptions() -> Result<Vec<Description>, String> {
    parse(DESCRIPTIONS)
}

/// What the finder made of one build.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Found {
    /// Each value found, by path.
    pub values: BTreeMap<String, u64>,
    /// Each value not found, by path, with the reason.
    pub not_found: BTreeMap<String, String>,
}

impl Found {
    /// The row values only: the steps whose path starts with `_` are left out.
    pub fn row_values(&self) -> impl Iterator<Item = (&String, &u64)> {
        self.values.iter().filter(|(path, _)| !path.starts_with('_'))
    }
}

/// Applies every description to one build.
pub fn find(pe: &PeFile, descriptions: &[Description]) -> Found {
    let mut finder = Finder {
        pe,
        ranges: pe.function_ranges(),
        by_path: descriptions
            .iter()
            .map(|description| (description.path.as_str(), description))
            .collect(),
        done: BTreeMap::new(),
        busy: BTreeSet::new(),
    };
    for description in descriptions {
        let _ = finder.value(&description.path);
    }
    let mut found = Found::default();
    for (path, result) in finder.done {
        match result {
            Ok(value) => {
                found.values.insert(path, value);
            }
            Err(reason) => {
                found.not_found.insert(path, reason);
            }
        }
    }
    found
}

/// The one item of a set, or why there is not one.
fn one<T: Copy + std::fmt::LowerHex>(items: &BTreeSet<T>, what: &str) -> Result<T, String> {
    let mut iter = items.iter();
    match (iter.next(), iter.next()) {
        (Some(only), None) => Ok(*only),
        (None, _) => Err(format!("no {what}")),
        _ => Err(format!(
            "{} {what}: {}",
            items.len(),
            items
                .iter()
                .take(8)
                .map(|item| format!("{item:#x}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

struct Finder<'d, 'p, 'a> {
    pe: &'p PeFile<'a>,
    ranges: BTreeMap<u32, u32>,
    by_path: BTreeMap<&'d str, &'d Description>,
    done: BTreeMap<String, Result<u64, String>>,
    busy: BTreeSet<String>,
}

impl Finder<'_, '_, '_> {
    fn value(&mut self, path: &str) -> Result<u64, String> {
        if let Some(done) = self.done.get(path) {
            return done.clone();
        }
        let Some(description) = self.by_path.get(path).copied() else {
            return Err(format!("{path} has no description"));
        };
        if self.busy.len() > PATH_DEPTH || !self.busy.insert(path.to_owned()) {
            return Err(format!("{path} is needed to find itself"));
        }
        let result = self.run(&description.how);
        self.busy.remove(path);
        self.done.insert(path.to_owned(), result.clone());
        result
    }

    /// An address another value found.
    fn need(&mut self, path: &str) -> Result<u32, String> {
        if !self.by_path.contains_key(path) {
            return Err(format!("needs {path}, which has no description"));
        }
        let value = self
            .value(path)
            .map_err(|_| format!("needs {path}, which was not found"))?;
        u32::try_from(value).map_err(|_| format!("{path} {value:#x} is not an address"))
    }

    fn run(&mut self, how: &How) -> Result<u64, String> {
        let pe = self.pe;
        match how {
            How::Class { names, part, slot } => {
                let mut tables = BTreeSet::new();
                for name in names {
                    let name = match part {
                        Part::Object => name.clone(),
                        Part::View => format!("{name}#view"),
                    };
                    tables.extend(row_class::class_tables(pe, &name));
                }
                let table = one(&tables, "class tables")?;
                match slot {
                    None => Ok(u64::from(table)),
                    Some(slot) => self.slot_function(table, *slot).map(u64::from),
                }
            }
            How::Trace { texts, steps } => {
                let mut function = self.holder(texts)?;
                for step in steps {
                    function = self.step(function, *step)?;
                }
                Ok(u64::from(function))
            }
            How::TableOf { function, slot } => {
                let function = self.need(function)?;
                let holders = self.words_holding(function);
                let word = one(&holders, "class table words that hold it")?;
                let table = word
                    .checked_sub(slot * 8)
                    .ok_or("the slot lies before the section")?;
                // Every slot up to it must be code, so the slot lies inside one table.
                self.slot_function(table, *slot)?;
                Ok(u64::from(table))
            }
            How::TableSlot { table, slot } => {
                let table = self.need(table)?;
                self.slot_function(table, *slot).map(u64::from)
            }
            How::SlotOf { table, function } => {
                let (table_at, function_at) = (self.need(table)?, self.need(function)?);
                let mut slots = BTreeSet::new();
                for slot in 0..TABLE_SLOTS {
                    match pe.pointer_rva(table_at + slot * 8) {
                        Some(entry) if pe.is_code(entry) => {
                            if entry == function_at {
                                slots.insert(slot);
                            }
                        }
                        _ => break,
                    }
                }
                let slot = one(&slots, "slots that hold it")?;
                // The recipe's own check, so the finder and a recipe agree on a slot.
                row_recipe::slot_in(pe, (table.as_str(), table_at), (function.as_str(), function_at))
                    .and_then(|recipe| match recipe == u64::from(slot) {
                        true => Ok(recipe),
                        false => Err(format!("the recipe reads slot {recipe}, not {slot}")),
                    })
            }
            How::FirstLoad { function } => {
                let at = self.need(function)?;
                row_recipe::member_read_by(pe, (function.as_str(), at))
            }
            How::AllocationSize { constructor } => {
                let at = self.need(constructor)?;
                row_recipe::allocation_size(pe, (constructor.as_str(), at))
            }
        }
    }

    /// The function at `slot` of the table at `table`, where every slot up to it is code.
    fn slot_function(&self, table: u32, slot: u32) -> Result<u32, String> {
        let pe = self.pe;
        for each in 0..=slot {
            let entry = pe.pointer_rva(table + each * 8);
            if !entry.is_some_and(|entry| pe.is_code(entry)) {
                return Err(format!(
                    "slot {each} of the table at {table:#x} is not code, so slot {slot} is past its end"
                ));
            }
        }
        pe.pointer_rva(table + slot * 8)
            .ok_or_else(|| format!("slot {slot} of the table at {table:#x} cannot be read"))
    }

    /// The first part of the listed function around an address.
    fn function_at(&self, rva: u32) -> Option<u32> {
        row_class::enclosing(&self.ranges, rva).map(|part| self.pe.primary_part(part))
    }

    /// The one function that loads the address of one of the texts.
    fn holder(&self, texts: &[String]) -> Result<u32, String> {
        let mut places = BTreeSet::new();
        for text in texts {
            places.extend(row_class::texts(self.pe, text));
        }
        if places.is_empty() {
            return Err("the build holds none of the texts".into());
        }
        let mut holders = BTreeSet::new();
        for each in row_class::rip_uses(self.pe, &places) {
            if each.word {
                continue;
            }
            let function = self.function_at(each.at).ok_or_else(|| {
                format!(
                    "the code at {:#x} that uses the text lies in no listed function",
                    each.at
                )
            })?;
            holders.insert(function);
        }
        one(&holders, "functions that use the text")
    }

    /// Every call site of a function, jumps included.
    fn call_sites(&self, function: u32) -> Vec<u32> {
        row_class::branches_into(self.pe, &BTreeSet::from([function]))
            .into_iter()
            .map(|(site, _)| site)
            .collect()
    }

    /// The functions that call `function`, other than itself.
    fn callers(&self, function: u32) -> BTreeSet<u32> {
        self.call_sites(function)
            .into_iter()
            .filter_map(|site| self.function_at(site))
            .filter(|caller| *caller != function)
            .collect()
    }

    fn address_taken(&self, function: u32) -> bool {
        row_class::rip_uses(self.pe, &BTreeSet::from([function]))
            .iter()
            .any(|each| !each.word)
    }

    fn size(&self, function: u32) -> u32 {
        self.ranges
            .get(&function)
            .map_or(0, |end| end.saturating_sub(function))
    }

    fn step(&self, function: u32, step: Step) -> Result<u32, String> {
        match step {
            Step::SoleCaller(times) => {
                let mut at = function;
                for _ in 0..times {
                    let sites = self.call_sites(at);
                    let [site] = sites.as_slice() else {
                        break;
                    };
                    if self.address_taken(at) {
                        break;
                    }
                    match self.function_at(*site) {
                        Some(caller) if caller != at => at = caller,
                        _ => break,
                    }
                }
                Ok(at)
            }
            Step::OnlyCallerAtMostBytes(bytes) => {
                let small: BTreeSet<u32> = self
                    .callers(function)
                    .into_iter()
                    .filter(|caller| (1..=bytes).contains(&self.size(*caller)))
                    .collect();
                one(&small, &format!("callers of at most {bytes:#x} bytes"))
            }
            Step::MostCallersLead(times) => {
                let mut counted: Vec<(usize, u32)> = self
                    .callers(function)
                    .into_iter()
                    .map(|caller| (self.call_sites(caller).len(), caller))
                    .collect();
                counted.sort_unstable_by(|a, b| b.cmp(a));
                match counted.as_slice() {
                    [] => Err("no callers".into()),
                    [(_, only)] => Ok(*only),
                    [(first, top), (second, _), ..] => {
                        if *first >= (times as usize) * (*second).max(1) {
                            Ok(*top)
                        } else {
                            Err(format!(
                                "no caller leads by {times} times: {first} call sites against {second}"
                            ))
                        }
                    }
                }
            }
            Step::CalleeNth(index) => {
                let calls = self.calls_in(function)?;
                calls.get(index).copied().ok_or_else(|| {
                    format!("{function:#x} makes {} direct calls, not {}", calls.len(), index + 1)
                })
            }
        }
    }

    /// The functions `function` calls directly, in the order of its code.
    fn calls_in(&self, function: u32) -> Result<Vec<u32>, String> {
        let end = self
            .ranges
            .get(&function)
            .copied()
            .ok_or_else(|| format!("{function:#x} is not a listed function start"))?;
        let length = end.saturating_sub(function).min(BODY_MOST) as usize;
        let bytes = self
            .pe
            .read(function, length)
            .ok_or_else(|| format!("{function:#x} cannot be read"))?;
        let mut calls = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let instruction = x64::decode_body(&bytes[at..])
                .filter(|instruction| instruction.length > 0)
                .ok_or_else(|| format!("an instruction at {:#x} cannot be read", function + at as u32))?;
            if bytes[at] == 0xE8 && instruction.length == 5 && at + 5 <= bytes.len() {
                let relative = i32::from_le_bytes(bytes[at + 1..at + 5].try_into().expect("four bytes"));
                let target = i64::from(function) + at as i64 + 5 + i64::from(relative);
                if let Some(target) = u32::try_from(target).ok().filter(|target| self.pe.is_code(*target)) {
                    calls.push(target);
                }
            }
            at += instruction.length;
        }
        Ok(calls)
    }

    /// Every pointer-sized word outside the code that holds the address of `function`.
    fn words_holding(&self, function: u32) -> BTreeSet<u32> {
        let wanted = self.pe.image_base() + u64::from(function);
        let mut found = BTreeSet::new();
        for (base, bytes, is_code) in self.pe.sections() {
            if is_code {
                continue;
            }
            for at in (0..bytes.len().saturating_sub(7)).step_by(8) {
                let word = u64::from_le_bytes(bytes[at..at + 8].try_into().expect("in range"));
                if word == wanted {
                    found.insert(base + at as u32);
                }
            }
        }
        found
    }
}

/// The group a value is reported in: its feature, or the window, whose values have no switch.
pub fn group_of(path: &str) -> &'static str {
    match path.strip_prefix("frame.") {
        Some(_) => WINDOW,
        None => crate::row_features::feature_of(path).map_or(WINDOW, |feature| feature.name),
    }
}

/// The group of the values the window cannot do without.
pub const WINDOW: &str = "window";

/// How many values of each group were found and not found, and which.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct GroupReport {
    pub found: Vec<String>,
    pub not_found: Vec<String>,
}

/// The finder's report on one build, by group.
pub fn report(found: &Found) -> BTreeMap<String, GroupReport> {
    let mut groups: BTreeMap<String, GroupReport> = BTreeMap::new();
    for (path, value) in found.row_values() {
        groups
            .entry(group_of(path).to_owned())
            .or_default()
            .found
            .push(format!("{path}: {value:#x}"));
    }
    for (path, reason) in found.not_found.iter().filter(|(path, _)| !path.starts_with('_')) {
        groups
            .entry(group_of(path).to_owned())
            .or_default()
            .not_found
            .push(format!("{path}: {reason}"));
    }
    groups
}

/// How the finder did on one group of a build that has a verified row.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct GroupScore {
    /// Values found and equal to the verified row.
    pub hit: usize,
    /// Values found and different from the verified row, with both. Any is a failure.
    pub wrong: Vec<String>,
    /// Values of the verified row the finder did not find, with the reason.
    pub not_found: Vec<String>,
    /// Values found that the verified row does not hold, so nothing checks them.
    pub unchecked: Vec<String>,
}

/// The finder's score on a build with a verified row.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Score {
    pub groups: BTreeMap<String, GroupScore>,
}

impl Score {
    /// How many values were found wrong. Must be zero.
    pub fn wrong(&self) -> usize {
        self.groups.values().map(|group| group.wrong.len()).sum()
    }
}

/// The address a build records for what it does not have.
const ABSENT: u64 = 0x7FFF_0000;

/// Row values that are Rivet's choices or public constants, not facts the binary holds.
fn not_of_the_binary(path: &str) -> bool {
    path == "version"
        || path.starts_with("version[")
        || path == "tabs.new_tab_delay_ms"
        || path.starts_with("toolbar.dev.colours")
        || path.contains(".signatures")
        || crate::command_ids::ROW_PATHS.contains(&path)
}

/// Every number of a row part, by path, as `row_recipe::number` reads it.
fn numbers(path: &str, value: &Value, out: &mut BTreeMap<String, u64>) {
    match value {
        Value::Object(object) => {
            for (key, item) in object {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                numbers(&next, item, out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                numbers(&format!("{path}[{index}]"), item, out);
            }
        }
        _ => {
            if let Some(number) = row_recipe::number(value) {
                out.insert(path.to_owned(), number);
            }
        }
    }
}

/// Compares what the finder found with a verified row, `{frame, build}`, value by value. A
/// verified value the row records as nothing (zero or the absent address) counts only when the
/// finder found something there, which is then wrong.
pub fn score(found: &Found, verified: &Value) -> Score {
    let mut want = BTreeMap::new();
    let mut frame = BTreeMap::new();
    numbers("", &verified["frame"], &mut frame);
    want.extend(frame.into_iter().map(|(path, value)| (format!("frame.{path}"), value)));
    numbers("", &verified["build"], &mut want);
    want.retain(|path, _| !not_of_the_binary(path));
    let mut score = Score::default();
    for (path, value) in &want {
        let group = score.groups.entry(group_of(path).to_owned()).or_default();
        let nothing = *value == 0 || *value == ABSENT;
        match found.values.get(path) {
            Some(made) if made == value => group.hit += 1,
            Some(made) => group
                .wrong
                .push(format!("{path}: found {made:#x}, verified {value:#x}")),
            None if nothing => {}
            None => group.not_found.push(match found.not_found.get(path) {
                Some(reason) => format!("{path}: {reason}"),
                None => format!("{path}: no description"),
            }),
        }
    }
    for (path, value) in found.row_values() {
        if !want.contains_key(path) {
            score
                .groups
                .entry(group_of(path).to_owned())
                .or_default()
                .unchecked
                .push(format!("{path}: {value:#x}"));
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_module::{call, lea, module, module_with_unwind, table, text};
    use serde_json::json;

    fn describe(path: &str, how: How) -> Description {
        Description {
            path: path.to_owned(),
            source: vec!["chrome/browser/example.cc".into()],
            tag: "156.0.8078.80".into(),
            note: String::new(),
            how,
        }
    }

    fn trace(texts: &[&str], steps: &[Step]) -> How {
        How::Trace {
            texts: texts.iter().map(|text| (*text).to_owned()).collect(),
            steps: steps.to_vec(),
        }
    }

    /// `lea rcx, [text]; ret` at `at`.
    fn user(at: u32, text_at: u32) -> Vec<u8> {
        let mut code = lea(at, 1, text_at);
        code.push(0xC3);
        code
    }

    /// `sub rsp, 28h; call to; add rsp, 28h; ret` at `at`: 14 bytes.
    fn caller(at: u32, to: u32) -> Vec<u8> {
        let mut code = vec![0x48, 0x83, 0xEC, 0x28];
        code.extend(call(at + 4, to));
        code.extend([0x48, 0x83, 0xC4, 0x28, 0xC3]);
        code
    }

    #[test]
    fn nfr6_the_checked_in_descriptions_are_valid_and_name_their_source() {
        let all = descriptions().unwrap();
        assert!(all.len() >= 10);
        for description in &all {
            assert!(
                description
                    .source
                    .iter()
                    .all(|file| !file.is_empty() && !file.contains(".dll")),
                "{}",
                description.path
            );
            assert!(description.tag.split('.').count() == 4, "{}", description.path);
        }
        // Values measured to be wrong on some build stay without a description.
        for refused in ["omnibox.input_new", "app_window.set_contents", "side_panel.toggle"] {
            assert!(all.iter().all(|each| each.path != refused), "{refused}");
        }
    }

    #[test]
    fn nfr6_a_description_file_is_checked_before_it_is_used() {
        // A value that needs an undescribed one is read, and reported as not found when used.
        let text = json!({"schema": 1, "values": [
            {"path": "a.slot", "source": ["x.h"], "tag": "1.2.3.4", "kind": "slot_of",
             "table": "a.table", "function": "a.function"}
        ]})
        .to_string();
        let waiting = parse(&text).unwrap();
        let bytes = module(&[(0x101000, vec![0xC3])], &[(0x101000, 0x101001)]);
        let pe = PeFile::parse(&bytes).unwrap();
        assert_eq!(
            find(&pe, &waiting).not_found["a.slot"],
            "needs a.table, which has no description"
        );
        assert!(parse(&json!({"schema": 2, "values": []}).to_string()).is_err());
        let twice = json!({"schema": 1, "values": [
            {"path": "a", "source": ["x.cc"], "tag": "1.2.3.4", "kind": "trace", "texts": ["A"]},
            {"path": "a", "source": ["x.cc"], "tag": "1.2.3.4", "kind": "trace", "texts": ["B"]}
        ]})
        .to_string();
        assert!(parse(&twice).unwrap_err().contains("described twice"));
        let unsourced = json!({"schema": 1, "values": [
            {"path": "a", "source": [], "tag": "1.2.3.4", "kind": "trace", "texts": ["A"]}
        ]})
        .to_string();
        assert!(parse(&unsourced).is_err());
        let steps = json!({"schema": 1, "values": [
            {"path": "a", "source": ["x.cc"], "tag": "1.2.3.4", "kind": "trace", "texts": ["A"],
             "steps": [{"sole_caller": 3}, {"only_caller_at_most_bytes": 128},
                       {"most_callers_lead": 10}, {"callee_nth": 2}]}
        ]})
        .to_string();
        assert_eq!(
            parse(&steps).unwrap()[0].how,
            trace(
                &["A"],
                &[
                    Step::SoleCaller(3),
                    Step::OnlyCallerAtMostBytes(128),
                    Step::MostCallersLead(10),
                    Step::CalleeNth(2)
                ]
            )
        );
    }

    #[test]
    fn nfr6_a_trace_name_gives_the_one_function_that_uses_it() {
        let bytes = module(
            &[
                (0x101000, user(0x101000, 0x201000)),
                (0x201000, text("WebContentsImpl::OpenURL")),
            ],
            &[(0x101000, 0x101008)],
        );
        let pe = PeFile::parse(&bytes).unwrap();
        let found = find(&pe, &[describe("navigation.open_url", trace(&["WebContentsImpl::OpenURL"], &[]))]);
        assert_eq!(found.values["navigation.open_url"], 0x101000);
        // A text the build does not hold gives nothing.
        let found = find(&pe, &[describe("navigation.open_url", trace(&["Other::Name"], &[]))]);
        assert!(found.not_found["navigation.open_url"].contains("none of the texts"));
    }

    #[test]
    fn nfr6_a_trace_name_two_functions_use_gives_nothing() {
        let bytes = module(
            &[
                (0x101000, user(0x101000, 0x201000)),
                (0x101100, user(0x101100, 0x201000)),
                (0x201000, text("AutocompleteController::Start")),
            ],
            &[(0x101000, 0x101008), (0x101100, 0x101108)],
        );
        let pe = PeFile::parse(&bytes).unwrap();
        let found = find(&pe, &[describe("omnibox.start", trace(&["AutocompleteController::Start"], &[]))]);
        assert!(found.values.is_empty());
        assert!(found.not_found["omnibox.start"].starts_with("2 functions"));
    }

    #[test]
    fn nfr6_variations_are_tried_together_and_must_leave_one() {
        let bytes = module(
            &[
                (0x101000, user(0x101000, 0x201000)),
                (0x201000, text("View::LayoutImmediately")),
            ],
            &[(0x101000, 0x101008)],
        );
        let pe = PeFile::parse(&bytes).unwrap();
        let how = trace(&["View::LayoutImmediately", "View::DeprecatedLayoutImmediately"], &[]);
        let found = find(&pe, &[describe("layout.layout_immediately", how)]);
        assert_eq!(found.values["layout.layout_immediately"], 0x101000);
    }

    #[test]
    fn nfr6_a_moved_part_of_a_function_counts_as_the_function() {
        // The part at 0x101800 continues the function at 0x101000: its unwind data at 0x202000 has
        // the chain flag and ends in the entry of 0x101000.
        let mut unwind = vec![0x04 << 3, 0, 0, 0];
        unwind.extend(0x101000u32.to_le_bytes());
        unwind.extend(0x101010u32.to_le_bytes());
        unwind.extend(0x202100u32.to_le_bytes());
        let bytes = module_with_unwind(
            &[
                (0x101000, vec![0xC3]),
                (0x101800, user(0x101800, 0x201000)),
                (0x201000, text("generate_rgb_vars")),
                (0x202000, unwind),
            ],
            &[(0x101000, 0x101010, 0x202100), (0x101800, 0x101808, 0x202000)],
        );
        let pe = PeFile::parse(&bytes).unwrap();
        assert_eq!(pe.primary_part(0x101800), 0x101000);
        assert_eq!(pe.primary_part(0x101000), 0x101000);
        let found = find(&pe, &[describe("side_panel.generate_colors_css", trace(&["generate_rgb_vars"], &[]))]);
        assert_eq!(found.values["side_panel.generate_colors_css"], 0x101000);
    }

    #[test]
    fn nfr6_steps_follow_the_one_caller_the_source_describes() {
        // The traced body at 0x101000 has one caller, the helper at 0x101100, which a thin wrapper
        // at 0x101200 and a large function at 0x101300 both call.
        let mut large = caller(0x101300, 0x101100);
        large.resize(0x200, 0x90);
        let bytes = module(
            &[
                (0x101000, user(0x101000, 0x201000)),
                (0x101100, caller(0x101100, 0x101000)),
                (0x101200, caller(0x101200, 0x101100)),
                (0x101300, large),
                (0x201000, text("TabStripModel::ActivateTab")),
            ],
            &[
                (0x101000, 0x101008),
                (0x101100, 0x10110e),
                (0x101200, 0x10120e),
                (0x101300, 0x101500),
            ],
        );
        let pe = PeFile::parse(&bytes).unwrap();
        let how = trace(
            &["TabStripModel::ActivateTab"],
            &[Step::SoleCaller(3), Step::OnlyCallerAtMostBytes(0x80)],
        );
        let found = find(&pe, &[describe("tabs.activate_tab", how)]);
        // The helper has two callers, so the climb stops there, and only one caller is small.
        assert_eq!(found.values["tabs.activate_tab"], 0x101200);
        // With no size bound, two callers remain and nothing is taken.
        let how = trace(
            &["TabStripModel::ActivateTab"],
            &[Step::SoleCaller(3), Step::OnlyCallerAtMostBytes(0x1000)],
        );
        let found = find(&pe, &[describe("tabs.activate_tab", how)]);
        assert!(found.not_found["tabs.activate_tab"].starts_with("2 callers"));
        // The callee step reads the calls in order.
        let how = trace(&["TabStripModel::ActivateTab"], &[Step::SoleCaller(1), Step::CalleeNth(0)]);
        let found = find(&pe, &[describe("x", how)]);
        assert_eq!(found.values["x"], 0x101000);
    }

    #[test]
    fn nfr6_a_caller_is_taken_by_its_lead_only() {
        // The factory at 0x101000 uses the service name and has two callers: 0x101100, called
        // from ten places, and 0x101200, called from one.
        let mut parts = vec![
            (0x101000, user(0x101000, 0x201000)),
            (0x101100, caller(0x101100, 0x101000)),
            (0x101200, caller(0x101200, 0x101000)),
            (0x201000, text("AutocompleteClassifier")),
        ];
        let mut ranges = vec![(0x101000, 0x101008), (0x101100, 0x10110e), (0x101200, 0x10120e)];
        for index in 0..11u32 {
            let at = 0x102000 + index * 0x20;
            let to = if index == 0 { 0x101200 } else { 0x101100 };
            parts.push((at, caller(at, to)));
            ranges.push((at, at + 14));
        }
        let bytes = module(&parts, &ranges);
        let pe = PeFile::parse(&bytes).unwrap();
        let lead = |times| {
            let how = trace(&["AutocompleteClassifier"], &[Step::MostCallersLead(times)]);
            find(&pe, &[describe("omnibox.classifier_for_profile", how)])
        };
        assert_eq!(lead(10).values["omnibox.classifier_for_profile"], 0x101100);
        assert!(lead(11).not_found["omnibox.classifier_for_profile"].starts_with("no caller leads"));
    }

    #[test]
    fn nfr6_a_slot_from_a_public_header_reads_the_table_the_found_function_is_in() {
        // OpenURL is slot 1 of the table at 0x203000. Slot 7 and slot 13 are other functions.
        let functions: Vec<u32> = (0..16).map(|slot| 0x101000 + slot * 0x10).collect();
        let mut parts = vec![
            (0x101010, user(0x101010, 0x201000)),
            (0x201000, text("WebContentsImpl::OpenURL")),
            (0x203000, table(&functions)),
        ];
        let mut ranges = vec![(0x101010, 0x101018)];
        for function in functions.iter().filter(|function| **function != 0x101010) {
            parts.push((*function, vec![0xC3]));
            ranges.push((*function, function + 1));
        }
        let bytes = module(&parts, &ranges);
        let pe = PeFile::parse(&bytes).unwrap();
        let descriptions = [
            describe("navigation.open_url", trace(&["WebContentsImpl::OpenURL"], &[])),
            describe(
                "_web_contents_table",
                How::TableOf {
                    function: "navigation.open_url".into(),
                    slot: 1,
                },
            ),
            describe(
                "tabs.visible_url",
                How::TableSlot {
                    table: "_web_contents_table".into(),
                    slot: 13,
                },
            ),
            describe(
                "_open_url_slot",
                How::SlotOf {
                    table: "_web_contents_table".into(),
                    function: "navigation.open_url".into(),
                },
            ),
            describe(
                "omnibox.browser_context",
                How::TableSlot {
                    table: "_web_contents_table".into(),
                    slot: 40,
                },
            ),
        ];
        let found = find(&pe, &descriptions);
        assert_eq!(found.values["_web_contents_table"], 0x203000);
        assert_eq!(found.values["tabs.visible_url"], 0x1010d0);
        assert_eq!(found.values["_open_url_slot"], 1);
        // A slot past the table's end is not read as a function.
        assert!(found.not_found["omnibox.browser_context"].contains("past its end"));
        // Steps are not row values.
        assert!(found.row_values().all(|(path, _)| !path.starts_with('_')));
    }

    #[test]
    fn nfr6_a_value_whose_input_is_not_found_is_not_found() {
        let bytes = module(&[(0x101000, vec![0xC3])], &[(0x101000, 0x101001)]);
        let pe = PeFile::parse(&bytes).unwrap();
        let descriptions = [
            describe("navigation.active_web_contents", trace(&["Missing"], &[])),
            describe(
                "side_panel.browser_offset",
                How::FirstLoad {
                    function: "navigation.active_web_contents".into(),
                },
            ),
        ];
        let found = find(&pe, &descriptions);
        assert_eq!(
            found.not_found["side_panel.browser_offset"],
            "needs navigation.active_web_contents, which was not found"
        );
    }

    #[test]
    fn nfr6_a_member_offset_is_read_from_the_accessor_the_finder_found() {
        // mov rax, [rcx + 0x558]; ret, which uses the text through a later lea.
        let mut accessor = vec![0x48, 0x8B, 0x81, 0x58, 0x05, 0x00, 0x00];
        accessor.extend(lea(0x101007, 2, 0x201000));
        accessor.push(0xC3);
        let bytes = module(
            &[(0x101000, accessor), (0x201000, text("Browser::GetActiveWebContents"))],
            &[(0x101000, 0x10100f)],
        );
        let pe = PeFile::parse(&bytes).unwrap();
        let descriptions = [
            describe("tabs.active_web_contents", trace(&["Browser::GetActiveWebContents"], &[])),
            describe(
                "tabs.browser_in_view",
                How::FirstLoad {
                    function: "tabs.active_web_contents".into(),
                },
            ),
        ];
        let found = find(&pe, &descriptions);
        assert_eq!(found.values["tabs.browser_in_view"], 0x558);
    }

    #[test]
    fn nfr6_the_finder_is_scored_value_by_value_against_a_verified_row() {
        let found = Found {
            values: BTreeMap::from([
                ("navigation.open_url".to_owned(), 0x81c7b80),
                ("omnibox.classify".to_owned(), 0x63cd11),
                ("frame.frame_view_vtable".to_owned(), 0xfce1070),
                ("layout.layout".to_owned(), 0x1234),
                ("_web_contents_table".to_owned(), 0x5000),
            ]),
            not_found: BTreeMap::from([(
                "side_panel.toggle".to_owned(),
                "2 functions that use the text".to_owned(),
            )]),
        };
        let verified = json!({
            "frame": {"frame_view_vtable": 265162864u64, "version": "156.0.8078.81"},
            "build": {
                "version": [156, 0, 8078, 81],
                "navigation": {"open_url": "0x81c7b80", "signatures": [["0x81c7b80", "4157"]]},
                "omnibox": {"classify": "0x63cd10"},
                "side_panel": {"toggle": "0x970e610", "browser_offset": "0x560"},
                "tabs": {"new_tab_delay_ms": 0, "tab_object": "0x7fff0000"},
            }
        });
        let score = score(&found, &verified);
        assert_eq!(score.wrong(), 1);
        assert_eq!(score.groups[WINDOW].hit, 2);
        assert_eq!(
            score.groups["address suggestions"].wrong,
            ["omnibox.classify: found 0x63cd11, verified 0x63cd10"]
        );
        assert_eq!(
            score.groups["sidebar"].not_found,
            [
                "side_panel.browser_offset: no description",
                "side_panel.toggle: 2 functions that use the text"
            ]
        );
        assert_eq!(score.groups[WINDOW].unchecked, ["layout.layout: 0x1234"]);
        let groups = report(&found);
        assert_eq!(groups["sidebar"].not_found.len(), 1);
        assert!(groups.values().all(|group| group.found.iter().all(|line| !line.starts_with('_'))));
    }
}
