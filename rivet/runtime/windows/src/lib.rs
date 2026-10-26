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

#[cfg(windows)]
static HOST_BROWSER: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static HOST_PANEL: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(windows)]
const RIVET_PANEL_WIDTH: i32 = 292;

#[cfg(windows)]
unsafe fn layout_host(host: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    };

    let panel = HOST_PANEL.load(std::sync::atomic::Ordering::Acquire)
        as windows_sys::Win32::Foundation::HWND;
    let browser = HOST_BROWSER.load(std::sync::atomic::Ordering::Acquire)
        as windows_sys::Win32::Foundation::HWND;
    if panel.is_null() || browser.is_null() {
        return;
    }
    let mut bounds: RECT = std::mem::zeroed();
    if GetClientRect(host, &mut bounds) == 0 {
        return;
    }
    let dpi = GetDpiForWindow(host).max(96) as i32;
    let width = RIVET_PANEL_WIDTH * dpi / 96;
    let height = bounds.bottom - bounds.top;
    SetWindowPos(
        panel,
        std::ptr::null_mut(),
        0,
        0,
        width,
        height,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
    SetWindowPos(
        browser,
        std::ptr::null_mut(),
        width,
        0,
        (bounds.right - bounds.left - width).max(1),
        height,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}

#[cfg(windows)]
unsafe extern "system" fn host_window_proc(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, PostQuitMessage, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_SIZE,
    };

    match message {
        WM_DPICHANGED => {
            // Windows supplies physical coordinates for the destination monitor. Applying this
            // rectangle avoids DPI virtualization, a stale caption height, and a delayed panel.
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(
                window,
                std::ptr::null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            layout_host(window);
            0
        }
        WM_SIZE => {
            layout_host(window);
            0
        }
        WM_CLOSE => {
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// Adds the first native Rivet panel directly to the Chromium app window.
#[cfg(windows)]
fn install_prototype_frame() {
    use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, Ellipse, EndPaint, FillRect,
        SelectObject, SetBkMode, SetTextColor, TextOutW, CLEARTYPE_QUALITY, FW_SEMIBOLD, HDC,
        PAINTSTRUCT, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
        GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
        LoadImageW, PeekMessageW, RegisterClassW, SendMessageW, SetParent, SetWindowLongPtrW,
        SetWindowPos, TranslateMessage, GWL_STYLE, IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE,
        MSG, PM_REMOVE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        WM_ERASEBKGND, WM_PAINT, WM_QUIT, WM_SETICON, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
        WS_DLGFRAME, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPEDWINDOW, WS_SYSMENU,
        WS_THICKFRAME, WS_VISIBLE,
    };

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
    const HOST_CLASS: [u16; 16] = [
        b'R' as u16,
        b'i' as u16,
        b'v' as u16,
        b'e' as u16,
        b't' as u16,
        b'H' as u16,
        b'o' as u16,
        b's' as u16,
        b't' as u16,
        b'W' as u16,
        b'i' as u16,
        b'n' as u16,
        b'd' as u16,
        b'o' as u16,
        b'w' as u16,
        0,
    ];
    const fn color(red: u8, green: u8, blue: u8) -> COLORREF {
        (red as COLORREF) | ((green as COLORREF) << 8) | ((blue as COLORREF) << 16)
    }

    fn px(value: i32, dpi: i32) -> i32 {
        (value * dpi + 48) / 96
    }

    unsafe fn fill(dc: HDC, rect: RECT, value: COLORREF) {
        let brush = CreateSolidBrush(value);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn label(
        dc: HDC,
        dpi: i32,
        x: i32,
        y: i32,
        value: &str,
        size: i32,
        weight: i32,
        ink: COLORREF,
    ) {
        let face: Vec<u16> = "Segoe UI".encode_utf16().chain(Some(0)).collect();
        let font = CreateFontW(
            -px(size, dpi),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        let wide: Vec<u16> = value.encode_utf16().collect();
        SetTextColor(dc, ink);
        TextOutW(dc, px(x, dpi), px(y, dpi), wide.as_ptr(), wide.len() as i32);
        SelectObject(dc, old_font);
        DeleteObject(font);
    }

    unsafe extern "system" fn panel_proc(
        window: HWND,
        message: u32,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(window, &mut paint);
                let mut bounds: RECT = std::mem::zeroed();
                GetClientRect(window, &mut bounds);
                let dpi = GetDpiForWindow(window).max(96) as i32;
                fill(dc, bounds, color(0xf2, 0xee, 0xe5));
                SetBkMode(dc, TRANSPARENT as i32);
                let dark = color(0x1f, 0x20, 0x24);
                let accent = color(0x7c, 0x2d, 0x4f);
                let mark = CreateSolidBrush(dark);
                let old_brush = SelectObject(dc, mark);
                Ellipse(dc, px(24, dpi), px(24, dpi), px(44, dpi), px(44, dpi));
                SelectObject(dc, old_brush);
                DeleteObject(mark);
                let recess = CreateSolidBrush(color(0xf2, 0xee, 0xe5));
                let old_recess = SelectObject(dc, recess);
                Ellipse(dc, px(30, dpi), px(30, dpi), px(38, dpi), px(38, dpi));
                SelectObject(dc, old_recess);
                DeleteObject(recess);
                let pin = CreateSolidBrush(accent);
                let old_pin = SelectObject(dc, pin);
                Ellipse(dc, px(32, dpi), px(32, dpi), px(36, dpi), px(36, dpi));
                SelectObject(dc, old_pin);
                DeleteObject(pin);

                label(dc, dpi, 57, 24, "Rivet", 20, FW_SEMIBOLD as i32, dark);
                label(
                    dc,
                    dpi,
                    24,
                    71,
                    "WORK / AGENT BRIEF",
                    9,
                    FW_SEMIBOLD as i32,
                    accent,
                );
                fill(
                    dc,
                    RECT {
                        left: px(24, dpi),
                        top: px(97, dpi),
                        right: px(268, dpi),
                        bottom: px(98, dpi),
                    },
                    color(0xc9, 0xc1, 0xb5),
                );

                label(
                    dc,
                    dpi,
                    24,
                    114,
                    "TODAY",
                    9,
                    FW_SEMIBOLD as i32,
                    color(0x77, 0x74, 0x6f),
                );
                fill(
                    dc,
                    RECT {
                        left: px(17, dpi),
                        top: px(136, dpi),
                        right: px(275, dpi),
                        bottom: px(171, dpi),
                    },
                    color(0xfb, 0xfa, 0xf6),
                );
                let active = CreateSolidBrush(accent);
                let old_active = SelectObject(dc, active);
                Ellipse(dc, px(28, dpi), px(149, dpi), px(36, dpi), px(157, dpi));
                SelectObject(dc, old_active);
                DeleteObject(active);
                label(
                    dc,
                    dpi,
                    49,
                    145,
                    "Agent brief",
                    12,
                    FW_SEMIBOLD as i32,
                    dark,
                );
                label(
                    dc,
                    dpi,
                    28,
                    184,
                    "Ember workspace",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );
                label(
                    dc,
                    dpi,
                    28,
                    216,
                    "Release notes",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );
                label(
                    dc,
                    dpi,
                    24,
                    262,
                    "SAVED",
                    9,
                    FW_SEMIBOLD as i32,
                    color(0x77, 0x74, 0x6f),
                );
                label(
                    dc,
                    dpi,
                    28,
                    289,
                    "Research queue",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );
                label(
                    dc,
                    dpi,
                    28,
                    321,
                    "Design references",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );

                let logical_height = bounds.bottom * 96 / dpi;
                let command_top = px(logical_height - 218, dpi);
                fill(
                    dc,
                    RECT {
                        left: px(17, dpi),
                        top: command_top,
                        right: bounds.right - px(17, dpi),
                        bottom: command_top + px(42, dpi),
                    },
                    color(0xfb, 0xfa, 0xf6),
                );
                label(
                    dc,
                    dpi,
                    29,
                    logical_height - 206,
                    "Ask Rivet about this page",
                    12,
                    400,
                    color(0x77, 0x74, 0x6f),
                );
                let card_top = px(logical_height - 145, dpi);
                fill(
                    dc,
                    RECT {
                        left: px(17, dpi),
                        top: card_top,
                        right: bounds.right - px(17, dpi),
                        bottom: card_top + px(2, dpi),
                    },
                    dark,
                );
                label(
                    dc,
                    dpi,
                    24,
                    logical_height - 128,
                    "One browser,",
                    16,
                    FW_SEMIBOLD as i32,
                    dark,
                );
                label(
                    dc,
                    dpi,
                    24,
                    logical_height - 107,
                    "wherever you work.",
                    16,
                    400,
                    dark,
                );
                label(
                    dc,
                    dpi,
                    24,
                    logical_height - 68,
                    "RIVET / EMBER LINK",
                    9,
                    FW_SEMIBOLD as i32,
                    color(0x77, 0x74, 0x6f),
                );
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

    unsafe fn set_taskbar_icon(parent: HWND) {
        let runtime_name: Vec<u16> = "rivet_runtime_windows.dll"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let runtime_module = GetModuleHandleW(runtime_name.as_ptr());
        if runtime_module.is_null() {
            return;
        }
        let mut module_path = [0_u16; 32768];
        let length = GetModuleFileNameW(
            runtime_module,
            module_path.as_mut_ptr(),
            module_path.len() as u32,
        );
        if length == 0 || length as usize == module_path.len() {
            return;
        }
        let mut icon_path =
            std::path::PathBuf::from(String::from_utf16_lossy(&module_path[..length as usize]));
        icon_path.set_file_name("rivet.ico");
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = icon_path.as_os_str().encode_wide().chain(Some(0)).collect();
        let icon = LoadImageW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            IMAGE_ICON,
            0,
            0,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        );
        if !icon.is_null() {
            SendMessageW(parent, WM_SETICON, 1, icon as isize);
            SendMessageW(parent, WM_SETICON, 0, icon as isize);
        }
    }

    unsafe fn remove_browser_nonclient(window: HWND) {
        let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
        let child_style = (style
            & !(WS_CAPTION
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_SYSMENU
                | WS_BORDER
                | WS_DLGFRAME))
            | WS_CHILD
            | WS_VISIBLE;
        SetWindowLongPtrW(window, GWL_STYLE, child_style as isize);
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }

    let mut parent: HWND = std::ptr::null_mut();
    unsafe {
        // Chromium's injected worker starts in a DPI-virtualized context. A popup created in
        // that context is scaled down again on high-DPI monitors, so its 292px design becomes a
        // 146px strip at 200%. Use physical, per-monitor pixels before any Rivet HWND exists.
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        EnumWindows(Some(find_parent), (&mut parent as *mut HWND) as isize);
        if parent.is_null() {
            return;
        }
        let dpi = GetDpiForWindow(parent).max(96) as i32;
        let panel_width = RIVET_PANEL_WIDTH * dpi / 96;
        let mut browser_bounds: RECT = std::mem::zeroed();
        if GetWindowRect(parent, &mut browser_bounds) == 0 {
            return;
        }
        let mut window_class: WNDCLASSW = std::mem::zeroed();
        window_class.lpfnWndProc = Some(panel_proc);
        window_class.hInstance = GetModuleHandleW(std::ptr::null());
        window_class.lpszClassName = PANEL_CLASS.as_ptr();
        RegisterClassW(&window_class);
        let mut host_class: WNDCLASSW = std::mem::zeroed();
        host_class.lpfnWndProc = Some(host_window_proc);
        host_class.hInstance = GetModuleHandleW(std::ptr::null());
        host_class.lpszClassName = HOST_CLASS.as_ptr();
        RegisterClassW(&host_class);
        // Windows remains responsible for minimize, maximize, and close. Keeping the title
        // text empty leaves the left side available for Rivet's native surface.
        let host_title: Vec<u16> = vec![0];
        let host = CreateWindowExW(
            0,
            HOST_CLASS.as_ptr(),
            host_title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            browser_bounds.left,
            browser_bounds.top,
            browser_bounds.right - browser_bounds.left,
            browser_bounds.bottom - browser_bounds.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if host.is_null() {
            return;
        }
        // Windows retains the real caption buttons while the caption surface adopts Rivet's
        // paper tone. This keeps native hit testing and snap layouts intact.
        let caption_color: u32 = color(0xf2, 0xee, 0xe5);
        DwmSetWindowAttribute(
            host,
            DWMWA_CAPTION_COLOR as u32,
            (&caption_color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let panel = CreateWindowExW(
            0,
            PANEL_CLASS.as_ptr(),
            PANEL_CLASS.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            panel_width,
            48,
            host,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if panel.is_null() {
            return;
        }
        // The real Chrome app window becomes the host's child. Chromium receives the right-pane
        // size as its own window size, which makes normal page layout reflow instead of leaving
        // an overlay above a full-width page.
        remove_browser_nonclient(parent);
        SetParent(parent, host);
        remove_browser_nonclient(parent);
        HOST_BROWSER.store(parent as isize, std::sync::atomic::Ordering::Release);
        HOST_PANEL.store(panel as isize, std::sync::atomic::Ordering::Release);
        set_taskbar_icon(host);
        layout_host(host);
        SetWindowPos(
            parent,
            std::ptr::null_mut(),
            panel_width,
            0,
            (browser_bounds.right - browser_bounds.left - panel_width).max(1),
            browser_bounds.bottom - browser_bounds.top,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOZORDER,
        );
        // Rivet owns the host and panel HWNDs on this injected worker thread. Chrome keeps its
        // own UI thread, while this loop dispatches native host and sidebar messages.
        let mut next_style_check = std::time::Instant::now();
        loop {
            if IsWindow(host) == 0 || IsWindow(parent) == 0 || IsWindow(panel) == 0 {
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
            if std::time::Instant::now() >= next_style_check {
                // Chromium can restore its app-window decoration after navigation or a monitor
                // transition. Rivet owns this child and keeps its nonclient area disabled.
                remove_browser_nonclient(parent);
                next_style_check =
                    std::time::Instant::now() + std::time::Duration::from_millis(250);
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
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
