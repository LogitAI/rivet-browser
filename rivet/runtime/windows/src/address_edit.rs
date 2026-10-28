//! Experiment: typing an address into the top row Rivet paints.
//!
//! The top row is a composition visual and cannot take keyboard input, so a click on the address
//! opens a native edit box over it. The box is an owned popup, so it sits above the browser's
//! own surface and the Rivet visuals. Enter opens the address, Escape or leaving the box cancels.

use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, CLEARTYPE_QUALITY, DEFAULT_CHARSET, FW_NORMAL, HFONT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    PostMessageW, RegisterWindowMessageW, SendMessageW, SetWindowLongPtrW, ES_AUTOHSCROLL,
    GWLP_WNDPROC, GW_OWNER, WM_CHAR, WM_CLOSE, WM_KEYDOWN, WM_KILLFOCUS, WM_NCDESTROY, WM_SETFONT,
    WNDPROC, WS_POPUP, WS_VISIBLE,
};

/// `EM_SETSEL`, which selects text in an edit control.
const EM_SETSEL: u32 = 0x00B1;

/// The open edit box and the edit control's own window procedure.
static EDIT: AtomicIsize = AtomicIsize::new(0);
static ORIGINAL: AtomicIsize = AtomicIsize::new(0);
static FONT: AtomicIsize = AtomicIsize::new(0);
/// How often the open box took the focus back from the browser window.
static RECLAIMS: AtomicUsize = AtomicUsize::new(0);

unsafe fn reclaim_message() -> u32 {
    RegisterWindowMessageW(windows_sys::w!("RivetAddressReclaim"))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens the edit box at `(x, y, width, height)` in screen pixels, showing `text` selected.
pub unsafe fn begin(owner: HWND, x: i32, y: i32, width: i32, height: i32, dpi: u32, text: &str) {
    let open = EDIT.load(Ordering::Relaxed);
    if open != 0 {
        PostMessageW(open as HWND, WM_CLOSE, 0, 0);
    }
    let edit = CreateWindowExW(
        0,
        windows_sys::w!("EDIT"),
        wide(text).as_ptr(),
        WS_POPUP | WS_VISIBLE | ES_AUTOHSCROLL as u32,
        x,
        y,
        width,
        height,
        owner,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    crate::vertical_tabs::trace(&format!(
        "address edit {edit:?} at {x},{y} {width}x{height}"
    ));
    if edit.is_null() {
        return;
    }
    let font = CreateFontW(
        -(13 * dpi as i32 / 96),
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        windows_sys::w!("Segoe UI"),
    );
    FONT.store(font as isize, Ordering::Relaxed);
    SendMessageW(edit, WM_SETFONT, font as WPARAM, 1);
    let original = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
    ORIGINAL.store(original, Ordering::Relaxed);
    EDIT.store(edit as isize, Ordering::Relaxed);
    RECLAIMS.store(0, Ordering::Relaxed);
    SetFocus(edit);
    SendMessageW(edit, EM_SETSEL, 0, -1);
}

unsafe fn text(edit: HWND) -> String {
    let length = GetWindowTextLengthW(edit).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let read = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..read.max(0) as usize])
}

unsafe extern "system" fn edit_proc(
    edit: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let original: WNDPROC = std::mem::transmute(ORIGINAL.load(Ordering::Relaxed));
    match message {
        WM_KEYDOWN if wparam == VK_RETURN as WPARAM => {
            let owner = GetWindow(edit, GW_OWNER);
            let address = crate::chrome_navigation::address_for(&text(edit));
            if !address.is_empty() {
                crate::chrome_navigation::open(owner, &address);
            }
            PostMessageW(edit, WM_CLOSE, 0, 0);
            0
        }
        WM_KEYDOWN if wparam == VK_ESCAPE as WPARAM => {
            PostMessageW(edit, WM_CLOSE, 0, 0);
            0
        }
        // The edit control would beep for Enter and Escape.
        WM_CHAR if wparam == 0x0D || wparam == 0x1B => 0,
        WM_KILLFOCUS => {
            crate::vertical_tabs::trace(&format!("address edit lost focus to {wparam:#x}"));
            // The browser window takes the focus back right after the box opens. Leaving to
            // anything else, such as the page, cancels the edit.
            let owner = GetWindow(edit, GW_OWNER);
            if wparam as HWND == edit {
                return CallWindowProcW(original, edit, message, wparam, lparam);
            }
            if wparam as HWND == owner && RECLAIMS.fetch_add(1, Ordering::Relaxed) < 10 {
                PostMessageW(edit, reclaim_message(), 0, 0);
            } else {
                PostMessageW(edit, WM_CLOSE, 0, 0);
            }
            CallWindowProcW(original, edit, message, wparam, lparam)
        }
        _ if message == reclaim_message() => {
            SetFocus(edit);
            SendMessageW(edit, EM_SETSEL, 0, -1);
            0
        }
        WM_NCDESTROY => {
            let _ = EDIT.compare_exchange(edit as isize, 0, Ordering::Relaxed, Ordering::Relaxed);
            let font = FONT.swap(0, Ordering::Relaxed);
            let result = CallWindowProcW(original, edit, message, wparam, lparam);
            if font != 0 {
                DeleteObject(font as HFONT as _);
            }
            result
        }
        _ => CallWindowProcW(original, edit, message, wparam, lparam),
    }
}
