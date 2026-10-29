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
static BOUNDS_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static FEATURE_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
/// Each Rivet window and its browser, so a closed window's browser is forgotten by the window.
static BROWSERS: Mutex<Vec<(isize, usize)>> = Mutex::new(Vec::new());
static VIEWS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

fn locked() -> MutexGuard<'static, Vec<usize>> {
    VIEWS.lock().unwrap_or_else(|error| error.into_inner())
}

/// Whether this build records the function and has room for a detour in its first instructions.
pub fn supported() -> bool {
    let vertical = &crate::builds::get().vertical;
    if !vertical.detour_strip {
        return false;
    }
    if vertical.window_feature != crate::builds::ABSENT {
        return vertical.window_feature_prologue.len() >= SHORT_JUMP
            && (vertical.set_bounds_rect == crate::builds::ABSENT
                || vertical.set_bounds_rect_prologue.len() >= SHORT_JUMP);
    }
    vertical.should_draw_tab_strip != crate::builds::ABSENT
        && vertical.should_draw_tab_strip_prologue.len() >= 14
}

/// The shortest prologue the short detour replaces: `mov rax, imm64; jmp rax` is 12 bytes.
const SHORT_JUMP: usize = 12;

type SupportsFeature = unsafe extern "system" fn(*const c_void, u32, bool) -> bool;

fn browsers() -> MutexGuard<'static, Vec<(isize, usize)>> {
    BROWSERS.lock().unwrap_or_else(|error| error.into_inner())
}

/// Whether the call came from code inside `callers`, an RVA range of the browser image, or from
/// anywhere when it is `(0, 0)`.
unsafe fn asked_by_layout(callers: (usize, usize), caller: usize) -> bool {
    if callers == (0, 0) {
        return true;
    }
    let Some(image) = image::recorded() else { return false };
    let address = caller.wrapping_sub(image.base);
    address >= callers.0 && address < callers.1
}

// The detour is entered by a jump, so the browser's return address is on top of the stack. The
// stub hands it to the Rust function as a fourth argument and jumps there, which leaves the
// stack exactly as the browser's call made it.
std::arch::global_asm!(
    ".globl rivet_supports_feature_stub",
    "rivet_supports_feature_stub:",
    "mov r9, [rsp]",
    "jmp {target}",
    target = sym supports_feature,
);

extern "C" {
    fn rivet_supports_feature_stub();
}

/// Answers no to the tab strip feature for the browsers of Rivet windows, when the layout asks.
unsafe extern "system" fn supports_feature(
    browser: *const c_void,
    feature: u32,
    check: bool,
    caller: usize,
) -> bool {
    let vertical = &crate::builds::get().vertical;
    if feature == vertical.window_feature_tab_strip
        && browsers().iter().any(|(_, known)| *known == browser as usize)
    {
        if asked_by_layout(vertical.window_feature_callers, caller) {
            return false;
        }
    }
    let original: SupportsFeature = std::mem::transmute(FEATURE_ORIGINAL.load(Ordering::Acquire));
    original(browser, feature, check)
}

type SetBounds = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);

/// A view keeps its parent at +0x190, the begin and end of its children at +0x198 and +0x1A0 and
/// its bounds as x, y, width, height at +0x1C0, in the build this is recorded for.
const PARENT: usize = 0x190;
const CHILDREN: usize = 0x198;
const BOUNDS: usize = 0x1C0;

/// A word of memory that may not belong to a view, read so that a wrong guess cannot fault.
unsafe fn read(address: usize, bytes: usize, out: *mut u8) -> bool {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    address > 0x10000
        && ReadProcessMemory(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
            address as *const c_void,
            out.cast(),
            bytes,
            std::ptr::null_mut(),
        ) != 0
}

unsafe fn word(address: usize) -> Option<usize> {
    let mut value = 0usize;
    read(address, 8, (&mut value as *mut usize).cast()).then_some(value)
}

unsafe fn bounds_of(view: usize) -> Option<[i32; 4]> {
    let mut value = [0i32; 4];
    read(view + BOUNDS, 16, value.as_mut_ptr().cast()).then_some(value)
}

/// Gives a view that has just been told to take no room a size of nothing, which also repaints the
/// place it left. Without it the toolbar keeps the pixels the view drew there, since nothing else
/// invalidates them. A build that records no `SetBoundsRect` leaves the view as it is.
pub unsafe fn collapse(view: usize) {
    let original = BOUNDS_ORIGINAL.load(Ordering::Acquire);
    if original == 0 {
        return;
    }
    if let Some([x, y, width, height]) = bounds_of(view) {
        if width != 0 || height != 0 {
            let original: SetBounds = std::mem::transmute(original);
            original(view as *mut c_void, &[x, y, 0, 0]);
        }
    }
}

/// The left edge, in the frame's coordinates, of the caption buttons: the right-aligned short row
/// among the frame's children, which is the caption button container.
unsafe fn caption_left(frame: usize) -> Option<i32> {
    let frame_width = bounds_of(frame)?[2];
    let begin = word(frame + CHILDREN)?;
    let end = word(frame + CHILDREN + 8)?;
    if end < begin || (end - begin) / 8 > 64 {
        return None;
    }
    let mut found = None;
    for slot in (begin..end).step_by(8) {
        let Some(child) = word(slot) else { continue };
        let Some([x, y, width, height]) = bounds_of(child) else { continue };
        // The browser lays the client view out before the caption row, which has no height yet.
        if width > 0 && height <= 100 && y <= 20 && x + width >= frame_width - 20 && x > frame_width / 2 {
            found = Some(found.map_or(x, |known: i32| known.min(x)));
        }
    }
    found
}

/// The toolbar of a Rivet window stops short of the caption buttons, which the browser's layout
/// left to the strip it no longer places.
unsafe extern "system" fn set_bounds_rect(view: *mut c_void, rect: *const [i32; 4]) {
    let original: SetBounds = std::mem::transmute(BOUNDS_ORIGINAL.load(Ordering::Acquire));
    let vertical = &crate::builds::get().vertical;
    let mut moved = *rect;
    let known = locked().iter().any(|browser_view| {
        word(browser_view + vertical.toolbar_in_view) == Some(view as usize)
    });
    if known {
        // The toggle button of the appbar sits over the toolbar's first button unless the toolbar
        // starts after it.
        let lead = vertical.toolbar_leading.min(moved[2]).max(0);
        moved[0] += lead;
        moved[2] -= lead;
        let frame_vtable = image::recorded()
            .map(|image| image.base + crate::builds::get().layout.frame_vtable);
        let mut at = word(view as usize + PARENT);
        let mut left = 0;
        for _ in 0..8 {
            let Some(ancestor) = at else { break };
            if word(ancestor) == frame_vtable {
                if let Some(caption) = caption_left(ancestor) {
                    moved[2] = moved[2].min(caption - left - moved[0]).max(0);
                }
                break;
            }
            let Some(bounds) = bounds_of(ancestor) else { break };
            left += bounds[0];
            at = word(ancestor + PARENT);
        }
    }
    original(view, &moved)
}

unsafe extern "system" fn should_draw(view: *const c_void) -> bool {
    if locked().contains(&(view as usize)) {
        return false;
    }
    let original: ShouldDraw = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
    original(view)
}

/// Whether `view` is the browser view of a window that is hidden here.
pub fn registered(view: usize) -> bool {
    locked().contains(&view)
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
    let (rva, prologue, hook, original) = if vertical.window_feature != crate::builds::ABSENT {
        (
            vertical.window_feature,
            vertical.window_feature_prologue,
            rivet_supports_feature_stub as *const () as usize,
            &FEATURE_ORIGINAL,
        )
    } else {
        (
            vertical.should_draw_tab_strip,
            vertical.should_draw_tab_strip_prologue,
            should_draw as *const () as usize,
            &ORIGINAL,
        )
    };
    if vertical.set_bounds_rect != crate::builds::ABSENT
        && !image.has(vertical.set_bounds_rect, vertical.set_bounds_rect_prologue)
    {
        return Err("SetBoundsRect is not the recorded function".into());
    }
    detour(&image, rva, prologue, hook, original)?;
    if vertical.set_bounds_rect != crate::builds::ABSENT {
        detour(
            &image,
            vertical.set_bounds_rect,
            vertical.set_bounds_rect_prologue,
            set_bounds_rect as *const () as usize,
            &BOUNDS_ORIGINAL,
        )?;
    }
    Ok(())
}

/// Replaces the first instructions of a function with a jump to `hook` and keeps them in a
/// trampoline that resumes the function after them.
unsafe fn detour(
    image: &image::Image,
    rva: usize,
    prologue: &[u8],
    hook: usize,
    original: &AtomicUsize,
) -> Result<(), String> {
    if !image.has(rva, prologue) {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    let function = image.at(rva, prologue.len()).ok_or("function outside the image")? as *mut u8;
    let trampoline = VirtualAlloc(std::ptr::null(), 64, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE)
        as *mut u8;
    if trampoline.is_null() {
        return Err("could not allocate the trampoline".into());
    }
    let resume = crate::vertical_tabs::absolute_jump(function as usize + prologue.len());
    std::ptr::copy_nonoverlapping(prologue.as_ptr(), trampoline, prologue.len());
    std::ptr::copy_nonoverlapping(resume.as_ptr(), trampoline.add(prologue.len()), resume.len());
    original.store(trampoline as usize, Ordering::Release);
    // `mov rax, hook; jmp rax` when the prologue is too short for the 14 byte jump. rax is free at
    // a function's entry.
    let jump: Vec<u8> = if prologue.len() >= 14 {
        crate::vertical_tabs::absolute_jump(hook).to_vec()
    } else {
        let mut code = vec![0x48, 0xB8];
        code.extend_from_slice(&hook.to_le_bytes());
        code.extend_from_slice(&[0xFF, 0xE0]);
        code
    };
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(function.cast(), prologue.len(), PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the tab strip function".into());
    }
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
    if crate::builds::get().vertical.window_feature != crate::builds::ABSENT {
        let Some(base) = image::recorded().map(|image| image.base) else { return false };
        let Some(browser) = crate::chrome_tabs::browser(base, window) else { return false };
        let mut known = browsers();
        known.retain(|(owner, _)| *owner != window as isize);
        known.push((window as isize, browser as usize));
    }
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
    browsers().retain(|(owner, _)| *owner != window as isize);
}
