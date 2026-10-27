//! Makes Chromium's own frame layout leave room for the Rivet sidebar.
//!
//! Chromium's Windows browser frame asks its frame view where the client view goes. Rivet
//! replaces that one virtual function in the frame view's vtable. For windows that carry a Rivet
//! sidebar it moves the client view right by the sidebar width. Chromium then lays out, paints,
//! hit-tests and positions menus, IME windows and accessibility bounds for the narrower view
//! itself, so nothing has to be translated after the fact.
//!
//! The vtable layout is private to each Chromium build. Rivet only patches builds whose module
//! contents match a recorded build exactly, and leaves every other build untouched.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};

/// Addresses inside one Chromium build, as offsets from the `chrome.dll` base.
struct Build {
    name: &'static str,
    /// The primary vtable of `BrowserFrameViewWin`.
    frame_view_vtable: usize,
    /// The vtable slot of `BrowserFrameViewWin::GetBoundsForClientView`.
    client_view_slot: usize,
    /// `BrowserFrameViewWin::GetBoundsForClientView`.
    client_view_bounds: usize,
    /// `views::HWNDForView`, with the first bytes of its body.
    hwnd_for_view: usize,
    hwnd_for_view_prologue: &'static [u8],
}

/// Builds whose addresses were read from Google's public symbols and checked against the binary.
const BUILDS: &[Build] = &[Build {
    name: "Google Chrome 156.0.8078.48 x64",
    frame_view_vtable: 0x0FE7_3E28,
    client_view_slot: 103,
    client_view_bounds: 0x0369_E1F0,
    hwnd_for_view: 0x036A_A950,
    hwnd_for_view_prologue: &[
        0x56, 0x48, 0x83, 0xec, 0x20, 0x48, 0x89, 0xce, 0x48, 0x8b, 0x01, 0x48, 0x8b, 0x40, 0x58,
    ],
}];

/// `gfx::Rect`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Moves the client view right by `reserved` device-independent pixels. Views coordinates are
/// already scaled for the monitor, so the reservation does not depend on DPI.
pub fn reserve_leading(bounds: Rect, reserved: i32) -> Rect {
    let reserved = reserved.clamp(0, (bounds.width - 1).max(0));
    Rect {
        x: bounds.x + reserved,
        width: bounds.width - reserved,
        ..bounds
    }
}

type ClientViewBounds = unsafe extern "system" fn(*const c_void, *mut Rect) -> *mut Rect;
type HwndForView = unsafe extern "system" fn(*const c_void) -> HWND;

struct Hook {
    build: &'static str,
    original: ClientViewBounds,
    hwnd_for_view: HwndForView,
    /// Returns the width to reserve for a window, or `None` when it carries no Rivet sidebar.
    reserved: fn(HWND) -> Option<i32>,
}

static HOOK: OnceLock<Result<Hook, String>> = OnceLock::new();

unsafe extern "system" fn client_view_bounds(view: *const c_void, out: *mut Rect) -> *mut Rect {
    let Some(Ok(hook)) = HOOK.get() else {
        unreachable!("the vtable is only patched after the hook is stored");
    };
    let result = (hook.original)(view, out);
    if let Some(reserved) = (hook.reserved)((hook.hwnd_for_view)(view)) {
        *out = reserve_leading(*out, reserved);
    }
    result
}

/// Installs the layout hook once per browser process and returns the matched build. Returns why
/// it cannot when the build is not recognised, so Rivet refuses to attach rather than guess.
pub unsafe fn install(reserved: fn(HWND) -> Option<i32>) -> Result<&'static str, String> {
    HOOK.get_or_init(|| patch(reserved))
        .as_ref()
        .map(|hook| hook.build)
        .map_err(Clone::clone)
}

unsafe fn patch(reserved: fn(HWND) -> Option<i32>) -> Result<Hook, String> {
    let module = GetModuleHandleW(windows_sys::w!("chrome.dll"));
    if module.is_null() {
        return Err("this browser process has no chrome.dll".into());
    }
    let base = module as usize;
    let build = BUILDS
        .iter()
        .find(|build| matches(base, build))
        .ok_or("this Chromium build is not supported yet")?;
    let slot = (base + build.frame_view_vtable + build.client_view_slot * 8) as *mut usize;
    let hook = Hook {
        build: build.name,
        original: std::mem::transmute::<usize, ClientViewBounds>(slot.read()),
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(base + build.hwnd_for_view),
        reserved,
    };
    // Layout runs on this UI thread only, so no call can reach the new slot before HOOK holds
    // this value.
    let mut previous: PAGE_PROTECTION_FLAGS = 0;
    if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect the frame view vtable".into());
    }
    slot.write(client_view_bounds as *const () as usize);
    VirtualProtect(slot.cast(), 8, previous, &mut previous);
    Ok(hook)
}

/// Checks that the loaded module is the recorded build: the vtable slot must still point at the
/// recorded function, and the helper must start with the recorded instructions.
unsafe fn matches(base: usize, build: &Build) -> bool {
    let slot = (base + build.frame_view_vtable + build.client_view_slot * 8) as *const usize;
    let prologue = std::slice::from_raw_parts(
        (base + build.hwnd_for_view) as *const u8,
        build.hwnd_for_view_prologue.len(),
    );
    slot.read() == base + build.client_view_bounds && prologue == build.hwnd_for_view_prologue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_client_view_moves_right_by_the_sidebar_width() {
        let bounds = Rect {
            x: 0,
            y: 32,
            width: 1200,
            height: 800,
        };
        assert_eq!(
            reserve_leading(bounds, 292),
            Rect {
                x: 292,
                y: 32,
                width: 908,
                height: 800
            }
        );
    }

    #[test]
    fn fr2_narrow_client_view_keeps_one_pixel() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        assert_eq!(reserve_leading(bounds, 292).width, 1);
    }
}
