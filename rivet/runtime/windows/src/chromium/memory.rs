//! Reads of browser memory that must not fault.

use std::ffi::c_void;

use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT};

/// How many readable regions are remembered. A walk over browser objects reads the module's data
/// and several heap regions in turn, so remembering only the last one asked the system each time.
const REMEMBERED: usize = 16;

thread_local! {
    /// The regions last found readable, so a scan over objects does not query every word.
    static REGIONS: std::cell::RefCell<([(usize, usize); REMEMBERED], usize)> =
        const { std::cell::RefCell::new(([(0, 0); REMEMBERED], 0)) };
}

pub(crate) fn readable(address: usize, length: usize) -> bool {
    if address == 0 || address % 8 != 0 {
        return false;
    }
    let Some(last) = address.checked_add(length) else {
        return false;
    };
    let known = REGIONS.with(|regions| {
        regions
            .borrow()
            .0
            .iter()
            .any(|(start, end)| address >= *start && last <= *end)
    });
    if known {
        return true;
    }
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let got = unsafe {
        VirtualQuery(
            address as *const c_void,
            &mut info,
            std::mem::size_of_val(&info),
        )
    };
    let ok = got != 0
        && info.State == MEM_COMMIT
        && info.Protect & 0xEE != 0
        && info.Protect & 0x101 == 0;
    if !ok {
        return false;
    }
    let region = (
        info.BaseAddress as usize,
        info.BaseAddress as usize + info.RegionSize,
    );
    REGIONS.with(|regions| {
        let mut regions = regions.borrow_mut();
        let next = regions.1;
        regions.0[next] = region;
        regions.1 = (next + 1) % REMEMBERED;
    });
    last <= region.1
}

/// The pointer-sized value at `address`, read in place once the word is known readable. `None`
/// for an address that is not, which the caller may still try another way.
pub(crate) fn read_word(address: usize) -> Option<usize> {
    readable(address, 8).then(|| unsafe { *(address as *const usize) })
}

/// Whether one of the words of the `length` bytes at `address`, from byte `from` on, is `value`,
/// read in place in one go. `None` when the bytes are not all readable.
pub(crate) fn holds_word(address: usize, from: usize, length: usize, value: usize) -> Option<bool> {
    if !readable(address, length) {
        return None;
    }
    let words = unsafe { std::slice::from_raw_parts(address as *const usize, length / 8) };
    Some(words.iter().skip(from / 8).any(|word| *word == value))
}

pub(crate) fn prologue_is(address: usize, expected: &[u8]) -> bool {
    readable(address & !7, 8)
        && unsafe { std::slice::from_raw_parts(address as *const u8, expected.len()) } == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr2_browser_words_are_read_in_place_once_readable() {
        let words: Vec<usize> = vec![7, 11, 0x1234, 99];
        let at = words.as_ptr() as usize;
        assert_eq!(read_word(at + 16), Some(0x1234));
        assert_eq!(holds_word(at, 8, 32, 0x1234), Some(true));
        // The first word is skipped, as a view's own table pointer is.
        assert_eq!(holds_word(at, 8, 32, 7), Some(false));
        assert_eq!(read_word(0), None);
        assert_eq!(holds_word(0, 8, 32, 7), None);
    }
}
