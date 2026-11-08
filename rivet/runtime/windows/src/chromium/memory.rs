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

pub(crate) fn prologue_is(address: usize, expected: &[u8]) -> bool {
    readable(address & !7, 8)
        && unsafe { std::slice::from_raw_parts(address as *const u8, expected.len()) } == expected
}
