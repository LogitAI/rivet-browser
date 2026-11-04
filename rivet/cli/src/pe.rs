//! Reads the names a Windows executable imports and exports, which is how Rivet tells a Chromium
//! browser from other programs by looking at the files and not at their names.
//!
//! Only the headers and the two name tables are read, so a 300 MB browser module costs a few
//! small reads.

use std::io::{Read, Seek, SeekFrom};

/// The DLL names a file imports and the function names it exports.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Names {
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

const MAX_NAMES: u32 = 50_000;
const MAX_IMPORTS: u32 = 2_000;

struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_pointer: u32,
    raw_size: u32,
}

struct Reader<'a, R> {
    source: &'a mut R,
    sections: Vec<Section>,
}

impl<R: Read + Seek> Reader<'_, R> {
    fn bytes(&mut self, offset: u64, length: usize) -> Option<Vec<u8>> {
        self.source.seek(SeekFrom::Start(offset)).ok()?;
        let mut buffer = vec![0u8; length];
        self.source.read_exact(&mut buffer).ok()?;
        Some(buffer)
    }

    fn u16_at(&mut self, offset: u64) -> Option<u16> {
        let bytes = self.bytes(offset, 2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32_at(&mut self, offset: u64) -> Option<u32> {
        let bytes = self.bytes(offset, 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn offset_of(&self, rva: u32) -> Option<u64> {
        self.sections.iter().find_map(|section| {
            let extent = section.virtual_size.max(section.raw_size);
            (rva >= section.virtual_address && rva - section.virtual_address < extent)
                .then(|| u64::from(section.raw_pointer) + u64::from(rva - section.virtual_address))
        })
    }

    fn text_at(&mut self, rva: u32) -> Option<String> {
        let offset = self.offset_of(rva)?;
        let mut collected = Vec::new();
        self.source.seek(SeekFrom::Start(offset)).ok()?;
        let mut chunk = [0u8; 64];
        while collected.len() < 512 {
            let read = self.source.read(&mut chunk).ok()?;
            if read == 0 {
                break;
            }
            if let Some(end) = chunk[..read].iter().position(|byte| *byte == 0) {
                collected.extend_from_slice(&chunk[..end]);
                return String::from_utf8(collected).ok();
            }
            collected.extend_from_slice(&chunk[..read]);
        }
        None
    }
}

/// The imported DLL names and exported function names of a PE file, or `None` when the file is
/// not a readable PE file.
pub fn names<R: Read + Seek>(source: &mut R) -> Option<Names> {
    let mut reader = Reader {
        source,
        sections: Vec::new(),
    };
    if reader.bytes(0, 2)? != b"MZ" {
        return None;
    }
    let header = u64::from(reader.u32_at(0x3c)?);
    if reader.bytes(header, 4)? != b"PE\0\0" {
        return None;
    }
    let section_count = reader.u16_at(header + 6)?;
    let optional_size = u64::from(reader.u16_at(header + 20)?);
    let optional = header + 24;
    let directories = match reader.u16_at(optional)? {
        0x10b => optional + 96,
        0x20b => optional + 112,
        _ => return None,
    };
    let table = optional + optional_size;
    for index in 0..u64::from(section_count.min(96)) {
        let at = table + index * 40;
        let section = Section {
            virtual_size: reader.u32_at(at + 8)?,
            virtual_address: reader.u32_at(at + 12)?,
            raw_size: reader.u32_at(at + 16)?,
            raw_pointer: reader.u32_at(at + 20)?,
        };
        reader.sections.push(section);
    }
    let export_rva = reader.u32_at(directories)?;
    let import_rva = reader.u32_at(directories + 8)?;
    let mut found = Names::default();
    if import_rva != 0 {
        if let Some(start) = reader.offset_of(import_rva) {
            for index in 0..u64::from(MAX_IMPORTS) {
                let at = start + index * 20;
                let name = reader.u32_at(at + 12)?;
                if name == 0 {
                    break;
                }
                found.imports.extend(reader.text_at(name));
            }
        }
    }
    if export_rva != 0 {
        if let Some(start) = reader.offset_of(export_rva) {
            let count = reader.u32_at(start + 24)?.min(MAX_NAMES);
            let list = reader.u32_at(start + 32)?;
            if let Some(list) = reader.offset_of(list) {
                for index in 0..u64::from(count) {
                    let name = reader.u32_at(list + index * 4)?;
                    found.exports.extend(reader.text_at(name));
                }
            }
        }
    }
    Some(found)
}

/// A minimal PE file with the given imports and exports, for tests of everything that reads one.
#[cfg(test)]
pub(crate) fn synthetic(imports: &[&str], exports: &[&str]) -> Vec<u8> {
    const SECTION_RVA: u32 = 0x1000;
    const SECTION_RAW: u32 = 0x400;
    // The section holds the export directory, its name table, the import descriptors and all names.
    let mut section: Vec<u8> = Vec::new();
    let strings_at = |offset: usize| SECTION_RVA + offset as u32;
    let export_dir = 0usize;
    let export_names_array = 40usize;
    let import_dir = export_names_array + exports.len() * 4;
    let strings = import_dir + (imports.len() + 1) * 20;
    let mut name_rvas = Vec::new();
    let mut text: Vec<u8> = Vec::new();
    for name in exports.iter().chain(imports.iter()) {
        name_rvas.push(strings_at(strings + text.len()));
        text.extend_from_slice(name.as_bytes());
        text.push(0);
    }
    section.resize(export_dir + 40, 0);
    section[24..28].copy_from_slice(&(exports.len() as u32).to_le_bytes());
    section[32..36].copy_from_slice(&strings_at(export_names_array).to_le_bytes());
    for rva in &name_rvas[..exports.len()] {
        section.extend_from_slice(&rva.to_le_bytes());
    }
    for rva in &name_rvas[exports.len()..] {
        let mut descriptor = [0u8; 20];
        descriptor[12..16].copy_from_slice(&rva.to_le_bytes());
        section.extend_from_slice(&descriptor);
    }
    section.extend_from_slice(&[0u8; 20]);
    section.extend_from_slice(&text);

    let mut file = vec![0u8; SECTION_RAW as usize];
    file[0..2].copy_from_slice(b"MZ");
    file[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    file[0x40..0x44].copy_from_slice(b"PE\0\0");
    file[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
    file[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
    let optional = 0x58;
    file[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
    let directories = optional + 112;
    if !exports.is_empty() {
        file[directories..directories + 4].copy_from_slice(&strings_at(export_dir).to_le_bytes());
    }
    if !imports.is_empty() {
        file[directories + 8..directories + 12]
            .copy_from_slice(&strings_at(import_dir).to_le_bytes());
    }
    let table = optional + 240;
    file[table + 8..table + 12].copy_from_slice(&(section.len() as u32).to_le_bytes());
    file[table + 12..table + 16].copy_from_slice(&SECTION_RVA.to_le_bytes());
    file[table + 16..table + 20].copy_from_slice(&(section.len() as u32).to_le_bytes());
    file[table + 20..table + 24].copy_from_slice(&SECTION_RAW.to_le_bytes());
    file.extend_from_slice(&section);
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn fr1_reads_the_imports_and_exports_of_a_pe_file() {
        let file = synthetic(
            &["KERNEL32.dll", "chrome_elf.dll"],
            &["GetHandleVerifier", "IsBrowserProcess"],
        );
        let found = names(&mut Cursor::new(file)).unwrap();
        assert_eq!(found.imports, ["KERNEL32.dll", "chrome_elf.dll"]);
        assert_eq!(found.exports, ["GetHandleVerifier", "IsBrowserProcess"]);
    }

    #[test]
    fn fr1_a_file_that_is_not_a_pe_file_has_no_names() {
        assert_eq!(names(&mut Cursor::new(b"not a program".to_vec())), None);
        assert_eq!(names(&mut Cursor::new(Vec::new())), None);
        let mut truncated = synthetic(&["a.dll"], &[]);
        truncated.truncate(0x100);
        assert_eq!(names(&mut Cursor::new(truncated)), None);
    }

    #[test]
    fn fr1_a_pe_file_without_tables_has_empty_names() {
        let found = names(&mut Cursor::new(synthetic(&[], &[]))).unwrap();
        assert_eq!(found, Names::default());
    }
}
