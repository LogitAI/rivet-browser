//! Named injection targets, and the one checked way to change browser code.
//!
//! A target is a place in the browser module that Rivet reads or overwrites: a name for traces, an
//! address in the module, and the bytes the browser must have there. Nothing is written unless the
//! bytes match, and every write of an installation is journaled so that a failed installation puts
//! the browser back as it was.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    PAGE_PROTECTION_FLAGS,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::builds::ABSENT;
use crate::chromium::image::Image;

/// A function or datum of the browser module that a feature depends on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    pub name: &'static str,
    pub rva: usize,
    /// The bytes the module has at `rva`. Empty when only the address matters.
    pub expected: &'static [u8],
}

/// Why a target cannot be used in this build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Miss {
    /// The row has no address for it.
    Absent(&'static str),
    /// The address lies outside the module, or the bytes there differ from the recorded ones.
    Differs(&'static str),
}

impl Miss {
    pub(crate) fn describe(&self) -> String {
        match self {
            Miss::Absent(name) => format!("{name} is not recorded for this build"),
            Miss::Differs(name) => format!("{name} differs from the recorded bytes"),
        }
    }
}

impl Target {
    pub(crate) const fn new(name: &'static str, rva: usize, expected: &'static [u8]) -> Self {
        Self {
            name,
            rva,
            expected,
        }
    }

    /// Whether the row records this target at all.
    pub(crate) fn recorded(&self) -> bool {
        self.rva != ABSENT && self.rva != 0
    }

    /// Whether the module has this target where the row says.
    pub(crate) unsafe fn check(&self, image: &Image) -> Result<(), Miss> {
        if !self.recorded() {
            return Err(Miss::Absent(self.name));
        }
        let found = if self.expected.is_empty() {
            image.at(self.rva, 1).is_some()
        } else {
            image.has(self.rva, self.expected)
        };
        if found {
            Ok(())
        } else {
            Err(Miss::Differs(self.name))
        }
    }

    /// The first target of `targets` the module does not have, if any.
    pub(crate) unsafe fn first_miss(targets: &[Target], image: &Image) -> Option<Miss> {
        targets.iter().find_map(|target| target.check(image).err())
    }
}

/// What an installation overwrote, oldest first: the address and the bytes that were there. A
/// failed installation puts all of it back, so the browser is never left half patched.
static JOURNAL: Mutex<Vec<(usize, Vec<u8>)>> = Mutex::new(Vec::new());
/// Set once every change of the installation has been made.
fn journal() -> std::sync::MutexGuard<'static, Vec<(usize, Vec<u8>)>> {
    JOURNAL.lock().unwrap_or_else(|error| error.into_inner())
}

/// Overwrites `bytes.len()` bytes at `target`, whatever their protection, without recording.
pub(crate) unsafe fn write_raw(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(
        target.cast(),
        bytes.len(),
        PAGE_EXECUTE_READWRITE,
        &mut previous,
    ) == 0
    {
        return Err("could not unprotect browser memory".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
    VirtualProtect(target.cast(), bytes.len(), previous, &mut previous);
    FlushInstructionCache(GetCurrentProcess(), target.cast(), bytes.len());
    Ok(())
}

/// Overwrites browser memory and remembers what was there.
unsafe fn write_recorded(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let original = std::slice::from_raw_parts(target, bytes.len()).to_vec();
    write_raw(target, bytes)?;
    journal().push((target as usize, original));
    Ok(())
}

pub(crate) unsafe fn write_code(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    write_recorded(target, bytes)
}

/// Puts back everything the journal holds, newest first. The trampolines already allocated stay
/// where they are: a thread may still be inside one, and they are only a few bytes.
unsafe fn rollback() {
    let entries = std::mem::take(&mut *journal());
    for (address, original) in entries.into_iter().rev() {
        let _ = write_raw(address as *mut u8, &original);
    }
}

/// Runs `install` and undoes every change it made when it fails.
pub(crate) unsafe fn transaction(
    install: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    journal().clear();
    let result = install();
    match &result {
        Ok(()) => journal().clear(),
        Err(_) => rollback(),
    }
    result
}

pub(crate) unsafe fn executable(code: &[u8]) -> Result<usize, String> {
    let memory = VirtualAlloc(
        std::ptr::null(),
        code.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    ) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(code.as_ptr(), memory, code.len());
    Ok(memory as usize)
}

/// `mov rax, imm64`.
pub(crate) fn load_rax(code: &mut Vec<u8>, value: usize) {
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(value as u64).to_le_bytes());
}

/// `jmp [rip + 0]` to `to`, which leaves every register as it is.
pub(crate) fn jump(code: &mut Vec<u8>, to: usize) {
    code.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    code.extend_from_slice(&(to as u64).to_le_bytes());
}

/// How a function's stack cookie load is carried into the trampoline.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cookie {
    /// The prologue holds no cookie load.
    None,
    /// The prologue ends with the seven byte load, which the trampoline redoes from its absolute
    /// address. A prologue that does not end with one is copied whole.
    Trailing,
}

/// The ways the browser's functions are detoured. Each browser function was first detoured by a
/// feature of its own, and the differences between them are kept here as data.
#[derive(Clone, Copy)]
pub(crate) struct Style {
    pub cookie: Cookie,
    /// The prologue must be at least this long.
    pub min_len: usize,
    /// A second detour of the same function is a no-op once its trampoline is stored.
    pub once: bool,
    /// The write belongs to the installation's journal.
    pub journaled: bool,
    /// A prologue of 14 bytes or more gets the register-neutral absolute jump, and the rest is
    /// padded with `nop`. Otherwise `mov rax, hook; jmp rax` is padded with `int3`.
    pub absolute_if_room: bool,
}

impl Style {
    /// A detour in a journaled installation, the default.
    pub(crate) const JOURNALED: Style = Style {
        cookie: Cookie::Trailing,
        min_len: 0,
        once: false,
        journaled: true,
        absolute_if_room: false,
    };
}

/// Replaces the start of a function with a jump to `hook`. `prologue` is checked first. The
/// trampoline that runs the original is stored in `store` before the function is changed, and it
/// is also returned.
pub(crate) unsafe fn detour_styled(
    base: usize,
    rva: usize,
    prologue: &[u8],
    hook: usize,
    store: Option<&AtomicUsize>,
    style: Style,
) -> Result<usize, String> {
    let code = (base + rva) as *mut u8;
    if prologue.len() < style.min_len
        || std::slice::from_raw_parts(code, prologue.len()) != prologue
    {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    if style.once {
        if let Some(store) = store {
            let stored = store.load(Ordering::Acquire);
            if stored != 0 {
                return Ok(stored);
            }
        }
    }
    let mut trampoline = Vec::new();
    let cookie_at = match style.cookie {
        Cookie::None => None,
        Cookie::Trailing => (prologue.len() >= 7
            && prologue[prologue.len() - 7..prologue.len() - 4] == [0x48, 0x8B, 0x05])
        .then(|| prologue.len() - 7),
    };
    match cookie_at {
        Some(at) => {
            let displacement = i32::from_le_bytes(prologue[at + 3..at + 7].try_into().unwrap());
            let cookie = (base + rva + at + 7).wrapping_add_signed(displacement as isize);
            trampoline.extend_from_slice(&prologue[..at]);
            // mov rax, [cookie]
            trampoline.extend_from_slice(&[0x48, 0xA1]);
            trampoline.extend_from_slice(&(cookie as u64).to_le_bytes());
            trampoline.extend_from_slice(&prologue[at + 7..]);
        }
        None => trampoline.extend_from_slice(prologue),
    }
    jump(&mut trampoline, base + rva + prologue.len());
    let original = executable(&trampoline)?;
    if let Some(store) = store {
        store.store(original, Ordering::Release);
    }
    let mut patch = Vec::new();
    if style.absolute_if_room && prologue.len() >= 14 {
        jump(&mut patch, hook);
        patch.resize(prologue.len(), 0x90);
    } else {
        load_rax(&mut patch, hook);
        patch.extend_from_slice(&[0xFF, 0xE0]);
        patch.resize(
            prologue.len(),
            if style.absolute_if_room { 0x90 } else { 0xCC },
        );
    }
    if style.journaled {
        write_code(code, &patch)?;
    } else {
        write_raw(code, &patch)?;
    }
    Ok(original)
}

/// Replaces the start of a function with a jump to `hook` and returns a trampoline that runs the
/// original. `prologue` is checked first and is copied whole, unless the function reloads its
/// stack cookie relative to itself: then `prologue` ends with that seven byte load, and the
/// trampoline copies what comes before it and loads the same cookie from its absolute address.
pub(crate) unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    hook: usize,
    store: &AtomicUsize,
) -> Result<(), String> {
    detour_styled(base, rva, prologue, hook, Some(store), Style::JOURNALED).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Memory::PAGE_READWRITE;

    /// A page of plain read-write memory to stand in for browser code and vtables.
    unsafe fn scratch_page() -> *mut u8 {
        use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, MEM_RESERVE};
        let page = VirtualAlloc(
            std::ptr::null(),
            0x1000,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        ) as *mut u8;
        assert!(!page.is_null());
        for index in 0..0x1000 {
            *page.add(index) = (index % 251) as u8;
        }
        page
    }

    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn nfr3_failed_installation_restores_every_patched_byte() {
        let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        unsafe {
            let page = scratch_page();
            let before = std::slice::from_raw_parts(page, 0x1000).to_vec();
            static STORE: AtomicUsize = AtomicUsize::new(0);
            let prologue = before[0x100..0x10c].to_vec();
            let result = transaction(|| {
                detour(page as usize, 0x100, &prologue, 0x1122_3344, &STORE)?;
                write_recorded(page.add(0x200), &0xdead_beef_usize.to_le_bytes())?;
                write_code(page.add(0x300), &[0xcc; 7])?;
                assert_ne!(std::slice::from_raw_parts(page, 0x1000), &before[..]);
                Err("the next change cannot be made".to_owned())
            });
            assert!(result.is_err());
            assert_eq!(std::slice::from_raw_parts(page, 0x1000), &before[..]);
            assert!(journal().is_empty());
        }
    }

    #[test]
    fn nfr3_complete_installation_keeps_its_changes() {
        let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
        unsafe {
            let page = scratch_page();
            let result = transaction(|| {
                write_recorded(page.add(0x40), &0x0102_0304_0506_0708_usize.to_le_bytes())?;
                write_code(page.add(0x80), &[0x90, 0x90])
            });
            assert!(result.is_ok());
            assert_eq!(*(page.add(0x40) as *const usize), 0x0102_0304_0506_0708);
            assert_eq!(*page.add(0x80), 0x90);
            assert!(journal().is_empty());
        }
    }
}
