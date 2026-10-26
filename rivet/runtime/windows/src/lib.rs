//! The in-browser Rivet runtime bootstrap.
//!
//! This DLL intentionally has no browser-specific hook yet. Its only current contract is a
//! stable ABI probe after it has been loaded into an explicitly selected browser process.

/// The ABI expected by the Windows launcher.
pub const RUNTIME_ABI_VERSION: u32 = 1;

/// Lets the launcher verify that it loaded a Rivet runtime, rather than an arbitrary DLL.
#[no_mangle]
pub extern "C" fn rivet_runtime_abi_version() -> u32 {
    RUNTIME_ABI_VERSION
}

/// Starts Rivet after the DLL has loaded and the Windows loader lock has been released.
///
/// The first native-frame implementation is initialized from this stable entry point. Calling it
/// again is harmless, which lets a launcher recover after a command retry.
#[cfg(windows)]
#[no_mangle]
pub extern "system" fn rivet_runtime_start() -> u32 {
    use std::sync::atomic::{AtomicBool, Ordering};

    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::AcqRel) {
        return 1;
    }
    std::thread::spawn(install_prototype_frame);
    1
}

/// Adds the first native Rivet panel directly to the Chromium app window.
#[cfg(windows)]
fn install_prototype_frame() {
    use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, Ellipse, EndPaint, FillRect, GetStockObject,
        SelectObject, SetBkColor, SetBkMode, SetDCBrushColor, SetTextColor, TextOutW, DC_BRUSH,
        DEFAULT_GUI_FONT, HDC, PAINTSTRUCT, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
        GetWindowThreadProcessId, IsWindow, IsWindowVisible, PeekMessageW, RegisterClassW,
        SetWindowPos, TranslateMessage, ES_AUTOHSCROLL, MSG, PM_REMOVE, SWP_NOACTIVATE,
        SWP_NOZORDER, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_PAINT, WM_QUIT, WNDCLASSW, WS_BORDER,
        WS_CHILD, WS_TABSTOP, WS_VISIBLE,
    };

    const PANEL_WIDTH: i32 = 292;
    const PANEL_CLASS: [u16; 17] = [
        b'R' as u16,
        b'i' as u16,
        b'v' as u16,
        b'e' as u16,
        b't' as u16,
        b'P' as u16,
        b'a' as u16,
        b'n' as u16,
        b'e' as u16,
        b'l' as u16,
        b'2' as u16,
        0,
        0,
        0,
        0,
        0,
        0,
    ];

    const fn color(red: u8, green: u8, blue: u8) -> COLORREF {
        (red as COLORREF) | ((green as COLORREF) << 8) | ((blue as COLORREF) << 16)
    }

    unsafe fn fill(dc: HDC, rect: RECT, value: COLORREF) {
        let brush = CreateSolidBrush(value);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
    }

    unsafe fn label(dc: HDC, x: i32, y: i32, value: &str, ink: COLORREF) {
        let wide: Vec<u16> = value.encode_utf16().collect();
        SetTextColor(dc, ink);
        TextOutW(dc, x, y, wide.as_ptr(), wide.len() as i32);
    }

    unsafe fn static_label(parent: HWND, value: &str, x: i32, y: i32, width: i32, height: i32) {
        let class = [
            b'S' as u16,
            b'T' as u16,
            b'A' as u16,
            b'T' as u16,
            b'I' as u16,
            b'C' as u16,
            0,
        ];
        let text: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            width,
            height,
            parent,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
    }

    unsafe extern "system" fn panel_proc(
        window: HWND,
        message: u32,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_ERASEBKGND => 1,
            WM_CTLCOLORSTATIC => {
                let dc = _wparam as HDC;
                let background = color(0xf2, 0xee, 0xe5);
                SetTextColor(dc, color(0x1f, 0x20, 0x24));
                SetBkColor(dc, background);
                SetDCBrushColor(dc, background);
                GetStockObject(DC_BRUSH) as LRESULT
            }
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(window, &mut paint);
                let mut bounds: RECT = std::mem::zeroed();
                GetClientRect(window, &mut bounds);
                fill(dc, bounds, color(0xf2, 0xee, 0xe5));

                let font = GetStockObject(DEFAULT_GUI_FONT);
                let old_font = SelectObject(dc, font);
                SetBkMode(dc, TRANSPARENT as i32);

                // The mark is intentionally small. Rivet is the attachment to the browser, not
                // a competing browser chrome.
                let dark = color(0x1f, 0x20, 0x24);
                let accent = color(0x7c, 0x2d, 0x4f);
                let mark = CreateSolidBrush(dark);
                let old_brush = SelectObject(dc, mark);
                Ellipse(dc, 24, 24, 44, 44);
                SelectObject(dc, old_brush);
                DeleteObject(mark);
                let cut = CreateSolidBrush(color(0xf2, 0xee, 0xe5));
                let old_cut = SelectObject(dc, cut);
                Ellipse(dc, 31, 31, 39, 39);
                SelectObject(dc, old_cut);
                DeleteObject(cut);
                fill(
                    dc,
                    RECT {
                        left: 38,
                        top: 36,
                        right: 48,
                        bottom: 46,
                    },
                    color(0xf2, 0xee, 0xe5),
                );

                label(dc, 57, 26, "Rivet", dark);
                label(dc, 24, 70, "WORK / AGENT BRIEF", accent);
                fill(
                    dc,
                    RECT {
                        left: 24,
                        top: 93,
                        right: 268,
                        bottom: 94,
                    },
                    color(0xc9, 0xc1, 0xb5),
                );

                label(dc, 25, 113, "TODAY", color(0x77, 0x74, 0x6f));
                fill(
                    dc,
                    RECT {
                        left: 17,
                        top: 137,
                        right: 275,
                        bottom: 170,
                    },
                    color(0xfb, 0xfa, 0xf6),
                );
                let active = CreateSolidBrush(accent);
                let old_active = SelectObject(dc, active);
                Ellipse(dc, 28, 149, 36, 157);
                SelectObject(dc, old_active);
                DeleteObject(active);
                label(dc, 49, 147, "Agent brief", dark);
                label(dc, 28, 184, "Ember workspace", color(0x51, 0x50, 0x56));
                label(dc, 28, 216, "Release notes", color(0x51, 0x50, 0x56));

                label(dc, 25, 262, "SAVED", color(0x77, 0x74, 0x6f));
                label(dc, 28, 289, "Research queue", color(0x51, 0x50, 0x56));
                label(dc, 28, 321, "Design references", color(0x51, 0x50, 0x56));

                let card_top = (bounds.bottom - 119).max(350);
                fill(
                    dc,
                    RECT {
                        left: 17,
                        top: card_top,
                        right: 275,
                        bottom: card_top + 2,
                    },
                    dark,
                );
                label(dc, 24, card_top + 16, "One browser,", dark);
                label(dc, 24, card_top + 35, "wherever you work.", dark);
                label(
                    dc,
                    24,
                    card_top + 67,
                    "RIVET / EMBER LINK",
                    color(0x77, 0x74, 0x6f),
                );

                SelectObject(dc, old_font);
                EndPaint(window, &paint);
                0
            }
            _ => DefWindowProcW(window, message, _wparam, _lparam),
        }
    }

    unsafe extern "system" fn find_parent(window: HWND, parameter: LPARAM) -> i32 {
        let parent = &mut *(parameter as *mut HWND);
        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        if process_id == GetCurrentProcessId() && IsWindowVisible(window) != 0 {
            *parent = window;
            return 0;
        }
        1
    }

    let mut parent: HWND = std::ptr::null_mut();
    unsafe {
        EnumWindows(Some(find_parent), (&mut parent as *mut HWND) as isize);
        if parent.is_null() {
            return;
        }
        let mut window_class: WNDCLASSW = std::mem::zeroed();
        window_class.lpfnWndProc = Some(panel_proc);
        window_class.hInstance = GetModuleHandleW(std::ptr::null());
        window_class.lpszClassName = PANEL_CLASS.as_ptr();
        RegisterClassW(&window_class);
        let panel = CreateWindowExW(
            0,
            PANEL_CLASS.as_ptr(),
            PANEL_CLASS.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            PANEL_WIDTH,
            48,
            parent,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if panel.is_null() {
            return;
        }
        // Chromium can suppress direct GDI text when it repaints a cross-thread child window.
        // Static controls travel through the standard window and input path, so they stay visible.
        static_label(panel, "Rivet", 57, 26, 170, 25);
        static_label(panel, "WORK / AGENT BRIEF", 24, 70, 220, 18);
        static_label(panel, "TODAY", 25, 113, 120, 18);
        static_label(panel, "Agent brief", 49, 147, 190, 20);
        static_label(panel, "Ember workspace", 28, 184, 210, 20);
        static_label(panel, "Release notes", 28, 216, 210, 20);
        static_label(panel, "SAVED", 25, 262, 120, 18);
        static_label(panel, "Research queue", 28, 289, 210, 20);
        static_label(panel, "Design references", 28, 321, 210, 20);
        static_label(panel, "One browser,", 24, 372, 210, 20);
        static_label(panel, "wherever you work.", 24, 392, 210, 20);
        static_label(panel, "RIVET / EMBER LINK", 24, 426, 210, 18);
        let edit_class = [b'E' as u16, b'D' as u16, b'I' as u16, b'T' as u16, 0];
        let prompt = [
            b'A' as u16,
            b's' as u16,
            b'k' as u16,
            b' ' as u16,
            b'R' as u16,
            b'i' as u16,
            b'v' as u16,
            b'e' as u16,
            b't' as u16,
            b' ' as u16,
            b'a' as u16,
            b'b' as u16,
            b'o' as u16,
            b'u' as u16,
            b't' as u16,
            b' ' as u16,
            b't' as u16,
            b'h' as u16,
            b'i' as u16,
            b's' as u16,
            b' ' as u16,
            b'p' as u16,
            b'a' as u16,
            b'g' as u16,
            b'e' as u16,
            b'.' as u16,
            0,
        ];
        let command = CreateWindowExW(
            0,
            edit_class.as_ptr(),
            prompt.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            17,
            170,
            PANEL_WIDTH - 34,
            34,
            panel,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        loop {
            if IsWindow(parent) == 0 || IsWindow(panel) == 0 {
                break;
            }
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if message.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let mut bounds = std::mem::zeroed();
            if GetClientRect(parent, &mut bounds) != 0 {
                SetWindowPos(
                    panel,
                    std::ptr::null_mut(),
                    0,
                    0,
                    PANEL_WIDTH,
                    bounds.bottom - bounds.top,
                    // Chromium creates a Direct3D child after our injected worker starts. Do
                    // not preserve the old sibling order or that renderer covers Rivet entirely.
                    SWP_NOACTIVATE,
                );
                if !command.is_null() {
                    let panel_height = bounds.bottom - bounds.top;
                    SetWindowPos(
                        command,
                        std::ptr::null_mut(),
                        17,
                        (panel_height - 168).max(340),
                        PANEL_WIDTH - 34,
                        34,
                        SWP_NOZORDER,
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(windows)]
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: windows_module,
    _reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    // Loader-lock work must stay minimal. Runtime initialization starts only through the control
    // channel after the DLL has loaded.
    1
}

#[cfg(windows)]
#[allow(non_camel_case_types)]
type windows_module = isize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_runtime_exposes_the_expected_abi() {
        assert_eq!(rivet_runtime_abi_version(), RUNTIME_ABI_VERSION);
    }

    #[cfg(windows)]
    #[test]
    fn fr2_runtime_bootstrap_is_idempotent() {
        assert_eq!(rivet_runtime_start(), 1);
        assert_eq!(rivet_runtime_start(), 1);
    }
}
