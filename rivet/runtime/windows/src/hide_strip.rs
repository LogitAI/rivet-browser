//! Takes the horizontal tab strip out of a Rivet window's layout.
//!
//! The browser view decides in one function whether the window draws a tab strip. Rivet detours
//! that function: it answers no for the browser views of Rivet windows and runs the browser's own
//! code for every other window. The views are recorded when a window attaches, on the UI thread,
//! because the layout itself must not resolve a window while the window is being built.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
    PAGE_PROTECTION_FLAGS,
};

use crate::chrome_layout::image;

type ShouldDraw = unsafe extern "system" fn(*const c_void) -> bool;

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static VIEWS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

fn locked() -> MutexGuard<'static, Vec<usize>> {
    VIEWS.lock().unwrap_or_else(|error| error.into_inner())
}

/// Whether this build records the function and has room for a detour in its first instructions.
pub fn supported() -> bool {
    let vertical = &crate::builds::get().vertical;
    vertical.detour_strip
        && vertical.should_draw_tab_strip != crate::builds::ABSENT
        && vertical.should_draw_tab_strip_prologue.len() >= 14
}

unsafe extern "system" fn should_draw(view: *const c_void) -> bool {
    if locked().contains(&(view as usize)) {
        return false;
    }
    let original: ShouldDraw = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
    original(view)
}

/// Detours the function once for the whole process. Nothing changes for a window until `hide`.
pub unsafe fn install() -> Result<(), String> {
    INSTALLED
        .get_or_init(|| patch())
        .clone()
}

unsafe fn patch() -> Result<(), String> {
    if !supported() {
        return Err("this browser build has no recorded tab strip function".into());
    }
    let vertical = &crate::builds::get().vertical;
    let image = image::recorded().ok_or("no recorded browser image")?;
    let prologue = vertical.should_draw_tab_strip_prologue;
    if !image.has(vertical.should_draw_tab_strip, prologue) {
        return Err("ShouldDrawTabStrip is not the recorded function".into());
    }
    let function = image
        .at(vertical.should_draw_tab_strip, prologue.len())
        .ok_or("function outside the image")? as *mut u8;
    let trampoline = VirtualAlloc(std::ptr::null(), 64, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE)
        as *mut u8;
    if trampoline.is_null() {
        return Err("could not allocate the trampoline".into());
    }
    let resume = crate::vertical_tabs::absolute_jump(function as usize + prologue.len());
    std::ptr::copy_nonoverlapping(prologue.as_ptr(), trampoline, prologue.len());
    std::ptr::copy_nonoverlapping(resume.as_ptr(), trampoline.add(prologue.len()), resume.len());
    ORIGINAL.store(trampoline as usize, Ordering::Release);
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(function.cast(), prologue.len(), PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect ShouldDrawTabStrip".into());
    }
    let jump = crate::vertical_tabs::absolute_jump(should_draw as *const () as usize);
    std::ptr::copy_nonoverlapping(jump.as_ptr(), function, jump.len());
    std::ptr::write_bytes(function.add(jump.len()), 0x90, prologue.len() - jump.len());
    VirtualProtect(function.cast(), prologue.len(), previous, &mut previous);
    windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
        windows_sys::Win32::System::Threading::GetCurrentProcess(),
        function.cast(),
        prologue.len(),
    );
    Ok(())
}

/// The browser view of a window, for the hook to recognise.
unsafe fn view_of(window: HWND) -> Option<usize> {
    crate::chrome_tabs::browser_view(window).map(|view| view as usize)
}

/// Removes the tab strip from the window's layout.
pub unsafe fn hide(window: HWND) -> bool {
    if install().is_err() {
        return false;
    }
    let Some(view) = view_of(window) else { return false };
    let mut views = locked();
    if !views.contains(&view) {
        views.push(view);
    }
    true
}

/// Forgets a window that is going away, so its view's address is never matched again.
pub unsafe fn forget(window: HWND) {
    if let Some(view) = view_of(window) {
        locked().retain(|known| *known != view);
    }
}
