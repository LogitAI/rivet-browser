//! The Chromium builds Rivet's in-browser layout hook supports, and the PE reading needed to add
//! new ones.
//!
//! Each entry records where one build keeps the frame view's client-view bounds function. The
//! runtime only patches a loaded module that matches an entry exactly. The `chrome-builds` tool
//! fills entries from Google's public symbols, so a new browser release needs a data change, not
//! a code change.

use serde::{Deserialize, Serialize};

/// The checked-in table of supported builds.
pub const TABLE: &str = include_str!("../chrome-builds.json");

/// Addresses inside one build, as offsets from its module base. Edge builds its windows from
/// `EdgeBrowserFrameViewWin`, so its row names that class's vtable.
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

/// A PE image read from a file, with enough structure to map RVAs and find its debug record.
pub struct PeFile<'a> {
    bytes: &'a [u8],
    image_base: u64,
    sections: Vec<(u32, u32, u32)>,
    debug_directory: (u32, u32),
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
        // Data directory 6 is the debug directory.
        let debug_directory = (
            u32_at(bytes, optional + 112 + 48)?,
            u32_at(bytes, optional + 116 + 48)?,
        );
        let table = optional + optional_size;
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
            debug_directory,
        })
    }

    fn offset(&self, rva: u32) -> Option<usize> {
        self.sections
            .iter()
            .find(|(address, size, _)| (*address..address.saturating_add(*size)).contains(&rva))
            .map(|(address, _, raw)| (rva - address + raw) as usize)
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

    /// The symbol-server key of the image's PDB: GUID and age from its CodeView record.
    pub fn pdb_key(&self) -> Option<String> {
        let (directory, size) = self.debug_directory;
        for index in 0..size / 28 {
            let entry = self.offset(directory + index * 28)?;
            // Type 2 is CodeView.
            if u32_at(self.bytes, entry + 12)? != 2 {
                continue;
            }
            let record = u32_at(self.bytes, entry + 24)? as usize;
            if self.bytes.get(record..record + 4)? != b"RSDS" {
                return None;
            }
            let guid = self.bytes.get(record + 4..record + 20)?;
            let age = u32_at(self.bytes, record + 20)?;
            let tail: String = guid[8..].iter().map(|byte| format!("{byte:02X}")).collect();
            return Some(format!(
                "{:08X}{:04X}{:04X}{tail}{age:X}",
                u32_at(guid, 0)?,
                u16_at(guid, 4)?,
                u16_at(guid, 6)?,
            ));
        }
        None
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
    fn nfr3_reads_the_symbol_server_key_of_an_image() {
        let mut section = vec![0u8; 0x100];
        section[12..16].copy_from_slice(&2u32.to_le_bytes());
        section[24..28].copy_from_slice(&0x240u32.to_le_bytes());
        section[0x40..0x44].copy_from_slice(b"RSDS");
        let guid = [
            0x51, 0x19, 0x13, 0x7b, 0xa2, 0x33, 0x0f, 0x9e, 0x4c, 0x4c, 0x44, 0x20, 0x50, 0x44,
            0x42, 0x2e,
        ];
        section[0x44..0x54].copy_from_slice(&guid);
        section[0x54..0x58].copy_from_slice(&1u32.to_le_bytes());
        let bytes = image(&section);
        let pe = PeFile::parse(&bytes).unwrap();
        assert_eq!(pe.pdb_key().unwrap(), "7B13195133A29E0F4C4C44205044422E1");
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
        assert!(table.iter().any(|build| build.version == "150.0.7871.187"));
        assert!(table.iter().any(|build| build.module == "whale.dll" && build.version == "5.39.412.57"));
        assert!(table.iter().any(|build| build.browser == "Brave" && build.version == "156.1.98.62"));
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
