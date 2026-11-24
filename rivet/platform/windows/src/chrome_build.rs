//! The Chromium builds Rivet's in-browser layout hook supports, and the PE reading needed to add
//! new ones.
//!
//! Each entry records where one build keeps the frame view's client-view bounds function. The
//! runtime only patches a loaded module that matches an entry exactly, so a new browser release
//! needs a data change, not a code change.

use serde::{Deserialize, Serialize};

/// The checked-in table of supported builds.
pub const TABLE: &str = include_str!("../chrome-builds.json");

/// Addresses inside one build, as offsets from its module base. A browser that builds its windows
/// from a subclass of `BrowserFrameViewWin` has that class's vtable here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromeBuild {
    pub browser: String,
    pub version: String,
    /// The module that holds the browser UI, for example `chrome.dll`.
    pub module: String,
    /// The primary vtable of `BrowserFrameViewWin`.
    pub frame_view_vtable: u32,
    /// The vtable slot of `BrowserFrameViewWin::GetBoundsForClientView`.
    pub client_view_slot: u32,
    /// `BrowserFrameViewWin::GetBoundsForClientView`.
    pub client_view_bounds: u32,
    /// `views::HWNDForView`.
    pub hwnd_for_view: u32,
    /// The first bytes of `views::HWNDForView`, as lowercase hex.
    pub hwnd_for_view_prologue: String,
}

/// Parses the table. The checked-in table is validated by a test, so the runtime treats a parse
/// failure as "no supported builds".
pub fn builds(table: &str) -> Vec<ChromeBuild> {
    serde_json::from_str(table).unwrap_or_default()
}

/// Inserts or replaces the entry for the same browser, version and module, keeping a stable
/// order so table diffs stay readable.
pub fn upsert(table: &mut Vec<ChromeBuild>, build: ChromeBuild) {
    table.retain(|known| {
        (&known.browser, &known.version, &known.module)
            != (&build.browser, &build.version, &build.module)
    });
    table.push(build);
    table.sort_by(|a, b| {
        (&a.browser, &a.module, &a.version).cmp(&(&b.browser, &b.module, &b.version))
    });
}

/// Decodes lowercase or uppercase hex.
pub fn hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(value.get(index..index + 2)?, 16).ok())
        .collect()
}

/// A PE image read from a file, with enough structure to map RVAs and find its functions.
pub struct PeFile<'a> {
    bytes: &'a [u8],
    image_base: u64,
    sections: Vec<(u32, u32, u32)>,
    code_sections: Vec<(u32, u32)>,
    exception_directory: (u32, u32),
    /// What `row_class` reads from the whole code once, kept for its later questions.
    pub(crate) code_index: std::cell::OnceCell<crate::row_class::CodeIndex>,
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

impl<'a> PeFile<'a> {
    /// Reads a 64-bit PE image. Returns `None` for anything else.
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let nt = u32_at(bytes, 0x3c)? as usize;
        if bytes.get(nt..nt + 4)? != b"PE\0\0" {
            return None;
        }
        let sections = u16_at(bytes, nt + 6)? as usize;
        let optional_size = u16_at(bytes, nt + 20)? as usize;
        let optional = nt + 24;
        if u16_at(bytes, optional)? != 0x20b {
            return None;
        }
        let image_base = u64_at(bytes, optional + 24)?;
        // Data directory 3 is the exception table, which lists where functions start.
        let exception_directory = (
            u32_at(bytes, optional + 112 + 24)?,
            u32_at(bytes, optional + 116 + 24)?,
        );
        let table = optional + optional_size;
        // IMAGE_SCN_MEM_EXECUTE.
        let code_sections = (0..sections)
            .map(|index| {
                let header = table + 40 * index;
                let executable = u32_at(bytes, header + 36)? & 0x2000_0000 != 0;
                Some(
                    executable.then_some((u32_at(bytes, header + 12)?, u32_at(bytes, header + 8)?)),
                )
            })
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        let sections = (0..sections)
            .map(|index| {
                let header = table + 40 * index;
                Some((
                    u32_at(bytes, header + 12)?,
                    u32_at(bytes, header + 8)?,
                    u32_at(bytes, header + 20)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            bytes,
            image_base,
            sections,
            code_sections,
            exception_directory,
            code_index: std::cell::OnceCell::new(),
        })
    }

    fn offset(&self, rva: u32) -> Option<usize> {
        self.sections
            .iter()
            .find(|(address, size, _)| (*address..address.saturating_add(*size)).contains(&rva))
            .map(|(address, _, raw)| (rva - address + raw) as usize)
    }

    /// Whether an RVA lies in a section of instructions.
    pub fn is_code(&self, rva: u32) -> bool {
        self.code_sections
            .iter()
            .any(|(address, size)| (*address..address.saturating_add(*size)).contains(&rva))
    }

    /// The image base the module was linked at.
    pub fn image_base(&self) -> u64 {
        self.image_base
    }

    /// Every section as `(rva, bytes)`, cut to what the file holds, with whether it is code.
    pub fn sections(&self) -> Vec<(u32, &'a [u8], bool)> {
        self.sections
            .iter()
            .filter_map(|(address, size, raw)| {
                let start = *raw as usize;
                let end = start.saturating_add(*size as usize).min(self.bytes.len());
                let bytes = self.bytes.get(start..end)?;
                Some((*address, bytes, self.is_code(*address)))
            })
            .collect()
    }

    /// Reads bytes at an RVA.
    pub fn read(&self, rva: u32, length: usize) -> Option<&'a [u8]> {
        let start = self.offset(rva)?;
        self.bytes.get(start..start + length)
    }

    /// Reads a pointer-sized vtable entry at an RVA and returns the RVA it points to.
    pub fn pointer_rva(&self, rva: u32) -> Option<u32> {
        let value = u64_at(self.bytes, self.offset(rva)?)?;
        u32::try_from(value.checked_sub(self.image_base)?).ok()
    }

    /// The start of every function the exception table lists. A function that needs no unwind
    /// data, such as a leaf that touches no stack, is not listed.
    pub fn function_starts(&self) -> std::collections::BTreeSet<u32> {
        let (directory, size) = self.exception_directory;
        let mut starts = std::collections::BTreeSet::new();
        let Some(table) = self.offset(directory).filter(|_| directory != 0) else {
            return starts;
        };
        // Each entry is the function's start, its end and its unwind data.
        for index in 0..(size / 12) as usize {
            match u32_at(self.bytes, table + index * 12) {
                Some(start) => starts.insert(start),
                None => break,
            };
        }
        starts
    }

    /// Every part of a function the exception table lists, as start and end. The linker moves cold
    /// parts out of a function, and each such part has its own entry.
    pub fn function_ranges(&self) -> std::collections::BTreeMap<u32, u32> {
        let (directory, size) = self.exception_directory;
        let mut ranges = std::collections::BTreeMap::new();
        let Some(table) = self.offset(directory).filter(|_| directory != 0) else {
            return ranges;
        };
        for index in 0..(size / 12) as usize {
            let entry = table + index * 12;
            match (u32_at(self.bytes, entry), u32_at(self.bytes, entry + 4)) {
                (Some(start), Some(end)) if end > start => {
                    ranges.insert(start, end);
                }
                (Some(_), Some(_)) => {}
                _ => break,
            }
        }
        ranges
    }

    /// The exception table entry that starts at `start`, as its start, end and unwind data. The
    /// linker writes the table sorted by start.
    fn runtime_function(&self, start: u32) -> Option<(u32, u32, u32)> {
        let (directory, size) = self.exception_directory;
        let table = self.offset(directory).filter(|_| directory != 0)?;
        let (mut low, mut high) = (0usize, (size / 12) as usize);
        while low < high {
            let middle = (low + high) / 2;
            let entry = table + middle * 12;
            let begin = u32_at(self.bytes, entry)?;
            match begin.cmp(&start) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => {
                    return Some((
                        begin,
                        u32_at(self.bytes, entry + 4)?,
                        u32_at(self.bytes, entry + 8)?,
                    ))
                }
            }
        }
        None
    }

    /// The first part of the function that the part at `start` belongs to. A part the linker moved
    /// out of its function has unwind data with `UNW_FLAG_CHAININFO`, which ends in the entry of the
    /// part it continues. A part without such data is a function's first part.
    pub fn primary_part(&self, start: u32) -> u32 {
        let mut at = start;
        for _ in 0..8 {
            let Some((_, _, unwind)) = self.runtime_function(at) else {
                break;
            };
            // The low bit marks an entry that points at another entry rather than unwind data.
            let unwind = unwind & !1;
            let Some(head) = self.read(unwind, 4) else {
                break;
            };
            if (head[0] >> 3) & 4 == 0 {
                break;
            }
            let codes = u32::from(head[2]);
            let chained = unwind + 4 + ((codes + 1) & !1) * 2;
            match self.read(chained, 4) {
                Some(raw) => at = u32::from_le_bytes(raw.try_into().expect("four bytes")),
                None => break,
            }
        }
        at
    }

    /// Finds the slot of `function` in the vtable at `vtable`, scanning a bounded number of slots.
    pub fn vtable_slot(&self, vtable: u32, function: u32) -> Option<u32> {
        (0..512u32).find(|slot| self.pointer_rva(vtable + slot * 8) == Some(function))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal 64-bit PE image with one section that maps RVA 0x1000 to file 0x200.
    fn image(section: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x200];
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let optional = 0x98;
        bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[optional + 24..optional + 32].copy_from_slice(&0x1_8000_0000u64.to_le_bytes());
        // Debug directory at RVA 0x1000, one entry.
        bytes[optional + 160..optional + 164].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[optional + 164..optional + 168].copy_from_slice(&28u32.to_le_bytes());
        let header = optional + 240;
        bytes[header + 8..header + 12].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[header + 12..header + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[header + 20..header + 24].copy_from_slice(&0x200u32.to_le_bytes());
        bytes.extend_from_slice(section);
        bytes.resize(0x200 + 0x1000, 0);
        bytes
    }

    #[test]
    fn nfr3_finds_a_function_in_a_vtable() {
        let mut section = vec![0u8; 0x100];
        for (slot, target) in [0x2000u64, 0x3000, 0x4000].iter().enumerate() {
            let at = 0x80 + slot * 8;
            section[at..at + 8].copy_from_slice(&(0x1_8000_0000 + target).to_le_bytes());
        }
        let bytes = image(&section);
        let pe = PeFile::parse(&bytes).unwrap();
        assert_eq!(pe.vtable_slot(0x1080, 0x4000), Some(2));
        assert_eq!(pe.vtable_slot(0x1080, 0x5000), None);
    }

    #[test]
    fn nfr3_checked_in_build_table_is_valid() {
        let table: Vec<ChromeBuild> = serde_json::from_str(TABLE).unwrap();
        assert!(table.iter().any(|build| build.version == "156.0.8078.48"));
        assert!(table.iter().any(|build| build.version == "156.0.8078.80"));
        assert!(table.iter().any(|build| build.version == "157.0.8103.38"));
        assert!(table.iter().any(|build| build.version == "150.0.7871.187"));
        assert!(table
            .iter()
            .any(|build| build.module == "whale.dll" && build.version == "5.39.412.57"));
        assert!(table
            .iter()
            .any(|build| build.browser == "Brave" && build.version == "156.1.98.62"));
        assert!(table
            .iter()
            .any(|build| build.browser == "Brave" && build.version == "156.1.98.63"));
        assert!(table
            .iter()
            .any(|build| build.browser == "Brave" && build.version == "157.1.99.55"));
        assert!(table
            .iter()
            .any(|build| build.module == "samsunginternet.dll" && build.version == "30.0.4.29"));
        assert!(table
            .iter()
            .any(|build| build.module == "msedge.dll" && build.version == "156.0.4314.50"));
        assert!(table
            .iter()
            .all(|build| hex(&build.hwnd_for_view_prologue).is_some()));
    }

    #[test]
    fn nfr3_upsert_replaces_the_same_build() {
        let build = builds(TABLE).remove(0);
        let mut table = vec![build.clone()];
        let mut newer = build.clone();
        newer.client_view_slot += 1;
        upsert(&mut table, newer.clone());
        assert_eq!(table, vec![newer]);
    }
}
