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

use crate::builds::Build;
use crate::chromium::image;
use crate::feature::Needs;
use crate::targets::Target;

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
    supported_in(&crate::builds::get().vertical)
}

fn supported_in(vertical: &crate::builds::VerticalTabs) -> bool {
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

/// What leaving the browser's tab strip out of the layout needs from the browser.
pub(crate) fn needs(build: &Build) -> Needs {
    let row = &build.vertical;
    let mut targets = Vec::new();
    if row.window_feature != crate::builds::ABSENT {
        targets.push(Target::new(
            "supports window feature",
            row.window_feature,
            row.window_feature_prologue,
        ));
        if row.set_bounds_rect != crate::builds::ABSENT {
            targets.push(Target::new(
                "set bounds rect",
                row.set_bounds_rect,
                row.set_bounds_rect_prologue,
            ));
        }
    } else {
        targets.push(Target::new(
            "should draw tab strip",
            row.should_draw_tab_strip,
            row.should_draw_tab_strip_prologue,
        ));
    }
    Needs::checked(
        supported_in(row),
        "this build does not record a tab strip detour",
        targets,
    )
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
    let Some(image) = image::recorded() else {
        return false;
    };
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
        && browsers()
            .iter()
            .any(|(_, known)| *known == browser as usize)
        && asked_by_layout(vertical.window_feature_callers, caller)
    {
        return false;
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
    // Memory known readable is copied in place, without a system call for every word.
    if address > 0x10000 && crate::chromium::memory::readable(address, bytes) {
        std::ptr::copy_nonoverlapping(address as *const u8, out, bytes);
        return true;
    }
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
        let Some([x, y, width, height]) = bounds_of(child) else {
            continue;
        };
        // The browser lays the client view out before the caption row, which has no height yet.
        if width > 0
            && height <= 100
            && y <= 20
            && x + width >= frame_width - 20
            && x > frame_width / 2
        {
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
    let known = locked()
        .iter()
        .any(|browser_view| word(browser_view + vertical.toolbar_in_view) == Some(view as usize));
    if known {
        // The toggle button of the appbar sits over the toolbar's first button unless the toolbar
        // starts after it.
        let lead = vertical.toolbar_leading.min(moved[2]).max(0);
        moved[0] += lead;
        moved[2] -= lead;
        let frame_vtable =
            image::recorded().map(|image| image.base + crate::builds::get().layout.frame_vtable);
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
            let Some(bounds) = bounds_of(ancestor) else {
                break;
            };
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

/// Detours the function once for the whole process. Nothing changes for a window until `hide`.
pub unsafe fn install() -> Result<(), String> {
    INSTALLED.get_or_init(|| patch()).clone()
}

unsafe fn patch() -> Result<(), String> {
    if !supported() {
        return Err("this browser build has no recorded tab strip function".into());
    }
    let vertical = &crate::builds::get().vertical;
    // In an app window the same window feature function is answered by the app window, which
    // already leaves the tab strip out of the layout. Two detours on one prologue would refuse
    // each other, whichever came first.
    if answered_by_app_window(
        vertical.window_feature,
        crate::features::app_window::recorded("feature_impl"),
    ) {
        return Ok(());
    }
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

/// Whether the app window's feature answer sits on the tab strip's window feature function.
fn answered_by_app_window(window_feature: usize, app_feature: Option<usize>) -> bool {
    window_feature != crate::builds::ABSENT && app_feature == Some(window_feature)
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
    use crate::targets::{detour_styled, Cookie, Style};
    if !image.has(rva, prologue) {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    let style = Style {
        cookie: Cookie::None,
        journaled: false,
        absolute_if_room: true,
        ..Style::JOURNALED
    };
    detour_styled(image.base, rva, prologue, hook, Some(original), style).map(|_| ())
}

/// The browser view of a window, for the hook to recognise.
unsafe fn view_of(window: HWND) -> Option<usize> {
    crate::features::tabs::browser_view(window).map(|view| view as usize)
}

/// Removes the tab strip from the window's layout.
pub unsafe fn hide(window: HWND) -> bool {
    if install().is_err() {
        return false;
    }
    let Some(view) = view_of(window) else {
        return false;
    };
    if crate::builds::get().vertical.window_feature != crate::builds::ABSENT {
        let Some(base) = image::recorded().map(|image| image.base) else {
            return false;
        };
        let Some(browser) = crate::features::tabs::browser(base, window) else {
            return false;
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr13_samsung_window_feature_is_detoured_once_by_the_app_window() {
        let vertical = &crate::builds::SAMSUNG.vertical;
        let app = crate::builds::SAMSUNG
            .app_window
            .as_ref()
            .and_then(|row| row.feature_impl);
        assert!(answered_by_app_window(vertical.window_feature, app));
        // A build whose app window does not answer the feature keeps the tab strip detour.
        assert!(!answered_by_app_window(vertical.window_feature, None));
        assert!(!answered_by_app_window(crate::builds::ABSENT, None));
    }
}
