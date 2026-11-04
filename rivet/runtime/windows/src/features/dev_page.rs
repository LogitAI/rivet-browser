//! Developer mode of the page a Rivet window shows. The window's active tab is read from Chromium's
//! tab strip model, and `dev_mode` decides from its address and the person's choices.

use std::sync::Mutex;

use windows_sys::Win32::Foundation::HWND;

use crate::dev_mode::{Action, Modes};

static MODES: Mutex<Modes> = Mutex::new(Modes::new());

fn modes() -> std::sync::MutexGuard<'static, Modes> {
    MODES.lock().unwrap_or_else(|error| error.into_inner())
}

/// The active tab of `window` and its address.
unsafe fn active_page(window: HWND) -> Option<(usize, String)> {
    crate::features::tabs::open_tabs(window)?
        .into_iter()
        .find(|tab| tab.saved.active)
        .map(|tab| (tab.id, tab.saved.url))
}

/// Whether the page of `window` is in developer mode.
pub(crate) fn window_in_dev_mode(window: HWND) -> bool {
    match unsafe { active_page(window) } {
        Some((tab, address)) => modes().is_on(tab, &address),
        None => false,
    }
}

/// Switches developer mode of the page `window` shows, as the person's choice.
pub(crate) unsafe fn toggle_dev_mode(window: HWND) -> bool {
    let Some((tab, address)) = active_page(window) else {
        return false;
    };
    let on = modes().toggle(tab, &address);
    crate::trace::trace(&format!(
        "developer mode of window {:#x} is {on}",
        window as usize
    ));
    crate::hooks::page_area::relayout(window);
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(window, std::ptr::null(), 0);
    crate::frame::update_appbar(window);
    true
}

/// The buttons the developer bar of this build offers, with the command each runs.
pub(crate) fn dev_actions() -> Vec<(Action, u32)> {
    let dev = &crate::builds::get().toolbar.dev;
    // Chromium has no browser command that opens DevTools on its Network or Performance panel, so
    // those two buttons stay out until a build records a Chromium function for them.
    crate::dev_mode::bar_buttons(&[
        (Action::Inspect, dev.inspect),
        (Action::Console, dev.console),
        (Action::Network, 0),
        (Action::Performance, 0),
    ])
}

/// Runs one of Rivet's own buttons. Returns whether `action` is one.
pub(crate) unsafe fn run_own(window: HWND, action: Action) -> bool {
    match action {
        Action::CopyLink => {
            let copied = active_page(window).is_some_and(|(_, address)| copy_text(window, &address));
            crate::trace::trace(&format!("developer bar: copy link {copied}"));
        }
        Action::MiniWindow => {
            // The page floats, and the window shows the tab beside it.
            crate::features::mini_window::pop_out_active(window);
            crate::frame::update_appbar(window);
        }
        Action::Screenshot => {
            let copied = copy_page_picture(window);
            crate::trace::trace(&format!("developer bar: screenshot {copied}"));
        }
        _ => return false,
    }
    true
}

const CF_BITMAP: u32 = 2;
const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 2;
const PW_RENDERFULLCONTENT: u32 = 2;

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(owner: HWND) -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, data: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn CloseClipboard() -> i32;
    fn PrintWindow(window: HWND, dc: *mut core::ffi::c_void, flags: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
    fn GlobalLock(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GlobalUnlock(memory: *mut core::ffi::c_void) -> i32;
    fn GlobalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}

/// Puts `data` on the clipboard as `format`. The clipboard owns it afterwards.
unsafe fn set_clipboard(window: HWND, format: u32, data: *mut core::ffi::c_void) -> bool {
    if OpenClipboard(window) == 0 {
        return false;
    }
    EmptyClipboard();
    let done = !SetClipboardData(format, data).is_null();
    CloseClipboard();
    done
}

unsafe fn copy_text(window: HWND, text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
    if memory.is_null() {
        return false;
    }
    let target = GlobalLock(memory).cast::<u16>();
    if target.is_null() {
        GlobalFree(memory);
        return false;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
    GlobalUnlock(memory);
    if set_clipboard(window, CF_UNICODETEXT, memory) {
        true
    } else {
        GlobalFree(memory);
        false
    }
}

/// The page's part of the window's client area in pixels: the panel below its top row.
pub(crate) fn page_pixels(panel: [i32; 4], row: i32, scale: f32) -> [i32; 4] {
    let [x, y, w, h] = panel;
    let px = |dip: i32| (dip as f32 * scale).round() as i32;
    [px(x), px(y + row), px(w), px((h - row).max(0))]
}

/// Copies a picture of the page to the clipboard. The window draws itself into a bitmap, as
/// PrintWindow asks it to, so nothing on top of the window gets into the picture.
unsafe fn copy_page_picture(window: HWND) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
    let Some(panel) = crate::features::frame_layout::panel_bounds(window) else { return false };
    let scale = crate::frame::layout_dpi(window).max(96) as f32 / 96.0;
    let [x, y, w, h] = page_pixels([panel.x, panel.y, panel.width, panel.height], crate::features::split::ROW_HEIGHT, scale);
    let mut client: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let screen = GetDC(window);
    let whole_dc = CreateCompatibleDC(screen);
    let whole = CreateCompatibleBitmap(screen, client.right, client.bottom);
    let page_dc = CreateCompatibleDC(screen);
    let page = CreateCompatibleBitmap(screen, w.max(1), h.max(1));
    let old_whole = SelectObject(whole_dc, whole);
    let old_page = SelectObject(page_dc, page);
    let drawn = PrintWindow(window, whole_dc as _, PW_RENDERFULLCONTENT) != 0
        && BitBlt(page_dc, 0, 0, w, h, whole_dc, x, y, SRCCOPY) != 0;
    SelectObject(whole_dc, old_whole);
    SelectObject(page_dc, old_page);
    DeleteObject(whole);
    DeleteDC(whole_dc);
    DeleteDC(page_dc);
    ReleaseDC(window, screen);
    if drawn && set_clipboard(window, CF_BITMAP, page as _) {
        return true;
    }
    DeleteObject(page);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr11_screenshot_takes_the_page_below_the_address_row() {
        assert_eq!(page_pixels([8, 0, 1000, 744], 44, 1.0), [8, 44, 1000, 700]);
        assert_eq!(page_pixels([8, 0, 1000, 744], 44, 1.5), [12, 66, 1500, 1050]);
        assert_eq!(page_pixels([0, 0, 100, 20], 44, 1.0)[3], 0);
    }
}
