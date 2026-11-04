//! Reads of browser memory that must not fault.

use std::ffi::c_void;

use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT};

thread_local! {
    /// The last region found readable, so a scan over one object does not query every word.
    static REGION: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

pub(crate) fn readable(address: usize, length: usize) -> bool {
    if address == 0 || address % 8 != 0 {
        return false;
    }
    let Some(last) = address.checked_add(length) else {
        return false;
    };
    let (start, end) = REGION.with(|region| region.get());
    if address >= start && last <= end {
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
    REGION.with(|cell| cell.set(region));
    last <= region.1
}

pub(crate) fn prologue_is(address: usize, expected: &[u8]) -> bool {
    readable(address & !7, 8)
        && unsafe { std::slice::from_raw_parts(address as *const u8, expected.len()) } == expected
}
