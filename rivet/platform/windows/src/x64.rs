//! A function's instructions, read as far as the row finder needs them.

/// One decoded instruction: its length, where its RIP-relative displacement or relative branch
/// target is, where a displacement from a register is, as in `[rcx + 0x558]`, and where its
/// immediate value is. Each place is `(offset, size)` within the instruction.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Instruction {
    pub length: usize,
    pub relative: Option<(usize, usize)>,
    pub displacement: Option<(usize, usize)>,
    pub immediate: Option<(usize, usize)>,
}

impl Instruction {
    fn plain(length: usize) -> Self {
        Self {
            length,
            relative: None,
            displacement: None,
            immediate: None,
        }
    }

    /// An instruction whose last `size` bytes, from `at`, are an immediate value.
    fn with_immediate(at: usize, size: usize) -> Self {
        Self {
            length: at + size,
            relative: None,
            displacement: None,
            immediate: Some((at, size)),
        }
    }

    /// A branch whose last `size` bytes, from `at`, are the distance to its target.
    fn branch(at: usize, size: usize) -> Self {
        Self {
            length: at + size,
            relative: Some((at, size)),
            displacement: None,
            immediate: None,
        }
    }
}

fn modrm(bytes: &[u8], at: usize, immediate: usize) -> Option<Instruction> {
    let byte = *bytes.get(at)?;
    let (mode, rm) = (byte >> 6, byte & 7);
    let mut length = at + 1;
    let mut relative = None;
    let mut displacement = None;
    if mode != 3 && rm == 4 {
        let sib = *bytes.get(length)?;
        length += 1;
        if mode == 0 && sib & 7 == 5 {
            displacement = Some((length, 4));
            length += 4;
        }
    }
    match mode {
        0 if rm == 5 => {
            relative = Some((length, 4));
            length += 4;
        }
        1 => {
            displacement = Some((length, 1));
            length += 1;
        }
        2 => {
            displacement = Some((length, 4));
            length += 4;
        }
        _ => {}
    }
    Some(Instruction {
        length: length + immediate,
        relative,
        displacement,
        immediate: (immediate > 0).then_some((length, immediate)),
    })
}

/// Decodes the instruction at the start of `bytes`, or `None` for one this does not know. The
/// list covers what function starts are made of, and anything else goes to a person.
pub fn decode(bytes: &[u8]) -> Option<Instruction> {
    decode_with(bytes, false)
}

/// Decodes as `decode` does, and also the instructions met inside a function body: lock and
/// segment prefixes, string and x87 instructions, VEX encoded vector instructions and more of
/// the two and three byte opcodes. It reads bodies to follow calls and operands, and never
/// judges a guard, so what a hook may copy stays as `decode` says.
pub fn decode_body(bytes: &[u8]) -> Option<Instruction> {
    decode_with(bytes, true)
}

fn decode_with(bytes: &[u8], body: bool) -> Option<Instruction> {
    let mut at = 0;
    let mut wide_operand = false;
    let mut rex_w = false;
    loop {
        match *bytes.get(at)? {
            0x66 => wide_operand = true,
            0xF2 | 0xF3 => {}
            0xF0 | 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 if body => {}
            _ => break,
        }
        at += 1;
    }
    if bytes.get(at)? & 0xF0 == 0x40 {
        rex_w = bytes[at] & 8 != 0;
        at += 1;
    }
    let opcode = *bytes.get(at)?;
    at += 1;
    let word = if wide_operand { 2 } else { 4 };
    match opcode {
        0x50..=0x5F | 0x90 | 0xC3 | 0xCC | 0x99 => Some(Instruction::plain(at)),
        0x98 => Some(Instruction::plain(at)),
        0x00..=0x03
        | 0x08..=0x0B
        | 0x10..=0x13
        | 0x18..=0x1B
        | 0x20..=0x23
        | 0x28..=0x2B
        | 0x30..=0x33
        | 0x38..=0x3B
        | 0x63
        | 0x84..=0x8B
        | 0x8D
        | 0x8F
        | 0xD0..=0xD3
        | 0xFE
        | 0xFF => modrm(bytes, at, 0),
        0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C | 0x6A | 0xA8 | 0xB0..=0xB7 => {
            Some(Instruction::with_immediate(at, 1))
        }
        0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D | 0x68 | 0xA9 => {
            Some(Instruction::with_immediate(at, word))
        }
        0xB8..=0xBF => Some(Instruction::with_immediate(
            at,
            if rex_w { 8 } else { word },
        )),
        0x80 | 0x83 | 0xC0 | 0xC1 | 0xC6 | 0x6B => modrm(bytes, at, 1),
        0x81 | 0xC7 | 0x69 => modrm(bytes, at, word),
        0xF6 => {
            let immediate = usize::from((bytes.get(at)? >> 3) & 7 < 2);
            modrm(bytes, at, immediate)
        }
        0xF7 => {
            let immediate = if (bytes.get(at)? >> 3) & 7 < 2 {
                word
            } else {
                0
            };
            modrm(bytes, at, immediate)
        }
        0x70..=0x7F | 0xEB => Some(Instruction::branch(at, 1)),
        0xE8 | 0xE9 => Some(Instruction::branch(at, 4)),
        0x0F => {
            let second = *bytes.get(at)?;
            at += 1;
            match second {
                0x80..=0x8F => Some(Instruction::branch(at, 4)),
                0x10..=0x17
                | 0x1E
                | 0x1F
                | 0x28..=0x2F
                | 0x40..=0x6F
                | 0x7E
                | 0x7F
                | 0x90..=0x9F
                | 0xAF
                | 0xB6
                | 0xB7
                | 0xBE
                | 0xBF
                | 0xD6
                | 0xEF => modrm(bytes, at, 0),
                _ if body => body_two_byte(bytes, at, second),
                _ => None,
            }
        }
        _ if body => body_one_byte(bytes, at, opcode),
        _ => None,
    }
}

/// One byte opcodes that only a function body holds. `at` is just past the opcode.
fn body_one_byte(bytes: &[u8], at: usize, opcode: u8) -> Option<Instruction> {
    match opcode {
        0x91..=0x97 | 0x9C..=0x9F | 0xA4..=0xA7 | 0xAA..=0xAF | 0xC9 | 0xF4 | 0xF5 => {
            Some(Instruction::plain(at))
        }
        0xF8..=0xFD => Some(Instruction::plain(at)),
        0xC2 => Some(Instruction::with_immediate(at, 2)),
        0xCD => Some(Instruction::with_immediate(at, 1)),
        0xD8..=0xDF => modrm(bytes, at, 0),
        0xE0..=0xE3 => Some(Instruction::branch(at, 1)),
        // Two byte VEX: one payload byte, then an opcode of the 0F map.
        0xC5 => {
            let opcode = *bytes.get(at + 1)?;
            modrm(bytes, at + 2, vex_immediate(1, opcode))
        }
        // Three byte VEX: the map in the first payload byte, then one more payload byte.
        0xC4 => {
            let map = bytes.get(at)? & 0x1F;
            let opcode = *bytes.get(at + 2)?;
            modrm(bytes, at + 3, vex_immediate(map, opcode))
        }
        _ => None,
    }
}

/// Whether a VEX instruction of `map` ends in an immediate byte.
fn vex_immediate(map: u8, opcode: u8) -> usize {
    match map {
        3 => 1,
        1 => usize::from(matches!(opcode, 0x70..=0x73 | 0xC2 | 0xC4..=0xC6)),
        _ => 0,
    }
}

/// Two byte opcodes that only a function body holds. `at` is just past the second byte.
fn body_two_byte(bytes: &[u8], at: usize, second: u8) -> Option<Instruction> {
    match second {
        0x0B | 0x31 | 0x77 | 0xA2 | 0xC8..=0xCF => Some(Instruction::plain(at)),
        0x0D
        | 0x18..=0x1D
        | 0x74..=0x76
        | 0x7C
        | 0x7D
        | 0xA3
        | 0xA5
        | 0xAB
        | 0xAD
        | 0xAE
        | 0xB0
        | 0xB1
        | 0xB3
        | 0xB8
        | 0xBB
        | 0xBC
        | 0xBD
        | 0xC0
        | 0xC1
        | 0xC3
        | 0xC7
        | 0xD0..=0xD5
        | 0xD7..=0xEE
        | 0xF0..=0xFE => modrm(bytes, at, 0),
        0x70..=0x73 | 0xA4 | 0xAC | 0xBA | 0xC2 | 0xC4..=0xC6 => modrm(bytes, at, 1),
        0x38 => modrm(bytes, at + 1, 0),
        0x3A => modrm(bytes, at + 1, 1),
        _ => None,
    }
}

/// The instruction boundaries within `bytes`, and whether all of them could be read up to `length`.
pub fn boundaries(bytes: &[u8], length: usize) -> (Vec<usize>, bool) {
    let mut at = 0;
    let mut seen = vec![];
    while at < length {
        match decode(&bytes[at..]) {
            Some(instruction) => {
                at += instruction.length;
                seen.push(at);
            }
            None => return (seen, false),
        }
    }
    (seen, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome_build::hex;

    #[test]
    fn nfr3_x64_lengths_of_real_function_starts() {
        // A frame setup that ends at byte 13.
        let chip = hex("4156565755534881eca00000004889").unwrap();
        let mut padded = chip.clone();
        padded.resize(40, 0x90);
        let (seen, complete) = boundaries(&padded, 15);
        assert!(complete);
        assert_eq!(&seen[..6], [2, 3, 4, 5, 6, 13]);
        assert!(!seen.contains(&15));
        // A stack cookie load is RIP-relative.
        let cookie = hex("56574883ec784889ce488b0510640810").unwrap();
        let (seen, _) = boundaries(&cookie, cookie.len());
        assert_eq!(seen.last(), Some(&16));
        assert!(decode(&cookie[9..]).unwrap().relative.is_some());
        // movss, ucomiss with a displacement, jne.
        let corners = hex("f30f10020f2e81840400007533").unwrap();
        assert_eq!(boundaries(&corners, 13).0, [4, 11, 13]);
    }

    #[test]
    fn nfr3_an_unknown_instruction_is_not_guessed() {
        assert_eq!(decode(&[0x0F, 0x05]), None);
        assert_eq!(boundaries(&[0x56, 0xCF, 0x90], 3), (vec![1], false));
    }

    #[test]
    fn nfr6_the_operands_of_a_body_instruction_are_located() {
        // mov rax, [rcx + 0x558]: a displacement of four bytes from the third byte.
        let load = decode(&[0x48, 0x8B, 0x81, 0x58, 0x05, 0x00, 0x00]).unwrap();
        assert_eq!(load.displacement, Some((3, 4)));
        assert_eq!(load.immediate, None);
        // call [rax + 0x2b8]: a table slot read through its displacement.
        let call = decode(&[0xFF, 0x90, 0xB8, 0x02, 0x00, 0x00]).unwrap();
        assert_eq!(call.displacement, Some((2, 4)));
        // mov edx, 0x9c45, and cmp byte [rcx + 0x30], 1.
        let id = decode(&[0xBA, 0x45, 0x9C, 0x00, 0x00]).unwrap();
        assert_eq!(id.immediate, Some((1, 4)));
        let compare = decode(&[0x80, 0x79, 0x30, 0x01]).unwrap();
        assert_eq!(compare.displacement, Some((2, 1)));
        assert_eq!(compare.immediate, Some((3, 1)));
        // lock cmpxchg and a VEX move decode only as body instructions.
        let lock = [0xF0, 0x48, 0x0F, 0xB1, 0x51, 0x08];
        assert_eq!(decode(&lock), None);
        assert_eq!(decode_body(&lock).unwrap().displacement, Some((5, 1)));
        let vex = [0xC5, 0xF8, 0x10, 0x41, 0x10];
        assert_eq!(decode_body(&vex).unwrap().length, 5);
    }
}
