//! Small 64-bit modules that tests build byte by byte, so the finder is checked on code whose every
//! instruction is known.

pub const IMAGE_BASE: u64 = 0x1_8000_0000;
/// Where a module keeps its exception table, in its data section.
const EXCEPTIONS: u32 = 0x20f000;

fn place(rva: u32) -> usize {
    if rva >= 0x200000 {
        0x10400 + (rva - 0x200000) as usize
    } else {
        0x400 + (rva - 0x100000) as usize
    }
}

/// A 64-bit module with a code section at 0x100000 and a data section at 0x200000, each 0x10000
/// long, holding the given bytes and an exception table that lists `ranges`. Code between
/// functions is padding.
pub fn module(parts: &[(u32, Vec<u8>)], ranges: &[(u32, u32)]) -> Vec<u8> {
    let entries: Vec<(u32, u32, u32)> = ranges.iter().map(|(start, end)| (*start, *end, 0)).collect();
    module_with_unwind(parts, &entries)
}

/// `module`, with the unwind data address of each exception table entry given.
pub fn module_with_unwind(parts: &[(u32, Vec<u8>)], entries: &[(u32, u32, u32)]) -> Vec<u8> {
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
    // The exception table is sorted by start, as a linker writes it.
    let mut sorted = entries.to_vec();
    sorted.sort_unstable();
    let mut table = vec![];
    for (start, end, unwind) in sorted {
        table.extend_from_slice(&start.to_le_bytes());
        table.extend_from_slice(&end.to_le_bytes());
        table.extend_from_slice(&unwind.to_le_bytes());
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

/// `call to`, placed at `from`.
pub fn call(from: u32, to: u32) -> Vec<u8> {
    let mut bytes = vec![0xE8];
    bytes.extend_from_slice(&((i64::from(to) - i64::from(from) - 5) as i32).to_le_bytes());
    bytes
}

/// `lea reg, [rip + to]`, placed at `from`, for `rcx` (`register` 1) and the other low registers.
pub fn lea(from: u32, register: u8, to: u32) -> Vec<u8> {
    let mut bytes = vec![0x48, 0x8D, 0x05 | (register << 3)];
    bytes.extend_from_slice(&((i64::from(to) - i64::from(from) - 7) as i32).to_le_bytes());
    bytes
}

/// A class table of the given functions.
pub fn table(functions: &[u32]) -> Vec<u8> {
    functions
        .iter()
        .flat_map(|function| (IMAGE_BASE + u64::from(*function)).to_le_bytes())
        .collect()
}

/// A zero-terminated text.
pub fn text(value: &str) -> Vec<u8> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes
}
