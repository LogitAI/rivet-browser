//! Makes Chromium's own frame layout leave room for the Rivet appbar.
//!
//! Chromium's Windows browser frame asks its frame view where the client view goes. Rivet
//! replaces that one virtual function in the frame view's vtable. For windows that carry a Rivet
//! appbar it moves the client view right by the appbar width. Chromium then lays out, paints,
//! hit-tests and positions menus, IME windows and accessibility bounds for the narrower view
//! itself, so nothing has to be translated after the fact.
//!
//! The vtable layout is private to each Chromium build. Rivet only patches builds whose module
//! contents match an entry of the supported-build table exactly, and leaves every other build
//! untouched.

use std::ffi::c_void;
use std::sync::OnceLock;

use rivet_platform_windows::chrome_build::{self, ChromeBuild};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_PROTECTION_FLAGS, PAGE_READWRITE};

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

/// Decides the width to reserve for a window from the client-view bounds the browser chose, or
/// returns `None` when the window carries no Rivet appbar.
pub type Reserve = fn(HWND, Rect) -> Option<i32>;

struct Hook {
    original: ClientViewBounds,
    hwnd_for_view: HwndForView,
    reserved: Reserve,
}

static HOOK: OnceLock<Result<Hook, String>> = OnceLock::new();

unsafe extern "system" fn client_view_bounds(view: *const c_void, out: *mut Rect) -> *mut Rect {
    let Some(Ok(hook)) = HOOK.get() else {
        unreachable!("the vtable is only patched after the hook is stored");
    };
    let result = (hook.original)(view, out);
    if let Some(reserved) = (hook.reserved)((hook.hwnd_for_view)(view), *out) {
        *out = reserve_leading(*out, reserved);
    }
    result
}

/// Installs the layout hook once per browser process. Returns why it cannot when the build is
/// not recognised, so Rivet refuses to attach rather than guess.
pub unsafe fn install(reserved: Reserve) -> Result<(), String> {
    HOOK.get_or_init(|| patch(reserved))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

unsafe fn patch(reserved: Reserve) -> Result<Hook, String> {
    let builds = chrome_build::builds(chrome_build::TABLE);
    let (build, base) = builds
        .iter()
        .find_map(|build| {
            let module = wide(&build.module);
            let base = GetModuleHandleW(module.as_ptr()) as usize;
            (base != 0 && matches(base, build)).then_some((build, base))
        })
        .ok_or("this browser build is not supported yet")?;
    let slot = (base + build.frame_view_vtable as usize + build.client_view_slot as usize * 8)
        as *mut usize;
    let hook = Hook {
        original: std::mem::transmute::<usize, ClientViewBounds>(slot.read()),
        hwnd_for_view: std::mem::transmute::<usize, HwndForView>(
            base + build.hwnd_for_view as usize,
        ),
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
    log(&format!(
        "rivet: laying out {} {}",
        build.browser, build.version
    ));
    Ok(hook)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn log(message: &str) {
    let text = wide(message);
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(text.as_ptr()) };
}

/// Checks that the loaded module is the recorded build: the vtable slot must still point at the
/// recorded function, and the helper must start with the recorded instructions.
unsafe fn matches(base: usize, build: &ChromeBuild) -> bool {
    let Some(expected) = chrome_build::hex(&build.hwnd_for_view_prologue) else {
        return false;
    };
    let slot = (base + build.frame_view_vtable as usize + build.client_view_slot as usize * 8)
        as *const usize;
    let prologue = std::slice::from_raw_parts(
        (base + build.hwnd_for_view as usize) as *const u8,
        expected.len(),
    );
    slot.read() == base + build.client_view_bounds as usize && prologue == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_client_view_moves_right_by_the_appbar_width() {
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
