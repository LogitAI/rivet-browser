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
    use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, ClientToScreen, CreateFontW, CreateRoundRectRgn, CreateSolidBrush,
        DeleteObject, Ellipse, EndPaint, FillRect, SelectObject, SetBkMode, SetTextColor,
        SetWindowRgn, TextOutW, CLEARTYPE_QUALITY, FW_SEMIBOLD, HDC, PAINTSTRUCT, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumChildWindows, EnumWindows,
        GetClassNameW, GetClientRect, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
        LoadImageW, PeekMessageW, RegisterClassW, SendMessageW, SetWindowPos, TranslateMessage,
        IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOZORDER,
        WM_ERASEBKGND, WM_PAINT, WM_QUIT, WM_SETICON, WNDCLASSW, WS_POPUP, WS_VISIBLE,
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

    #[allow(clippy::too_many_arguments)]
    unsafe fn label(
        dc: HDC,
        scale: i32,
        x: i32,
        y: i32,
        value: &str,
        size: i32,
        weight: i32,
        ink: COLORREF,
    ) {
        let face: Vec<u16> = "Segoe UI".encode_utf16().chain(Some(0)).collect();
        let font = CreateFontW(
            -size * scale,
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
        TextOutW(dc, x * scale, y * scale, wide.as_ptr(), wide.len() as i32);
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
                let scale = (GetDpiForWindow(window).max(96) / 96) as i32;
                fill(dc, bounds, color(0xf2, 0xee, 0xe5));
                SetBkMode(dc, TRANSPARENT as i32);
                let dark = color(0x1f, 0x20, 0x24);
                let accent = color(0x7c, 0x2d, 0x4f);
                let mark = CreateSolidBrush(dark);
                let old_brush = SelectObject(dc, mark);
                Ellipse(dc, 24 * scale, 24 * scale, 44 * scale, 44 * scale);
                SelectObject(dc, old_brush);
                DeleteObject(mark);
                let recess = CreateSolidBrush(color(0xf2, 0xee, 0xe5));
                let old_recess = SelectObject(dc, recess);
                Ellipse(dc, 30 * scale, 30 * scale, 38 * scale, 38 * scale);
                SelectObject(dc, old_recess);
                DeleteObject(recess);
                let pin = CreateSolidBrush(accent);
                let old_pin = SelectObject(dc, pin);
                Ellipse(dc, 32 * scale, 32 * scale, 36 * scale, 36 * scale);
                SelectObject(dc, old_pin);
                DeleteObject(pin);

                label(dc, scale, 57, 24, "Rivet", 20, FW_SEMIBOLD as i32, dark);
                label(
                    dc,
                    scale,
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
                        left: 24 * scale,
                        top: 97 * scale,
                        right: 268 * scale,
                        bottom: 98 * scale,
                    },
                    color(0xc9, 0xc1, 0xb5),
                );

                label(
                    dc,
                    scale,
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
                        left: 17 * scale,
                        top: 136 * scale,
                        right: 275 * scale,
                        bottom: 171 * scale,
                    },
                    color(0xfb, 0xfa, 0xf6),
                );
                let active = CreateSolidBrush(accent);
                let old_active = SelectObject(dc, active);
                Ellipse(dc, 28 * scale, 149 * scale, 36 * scale, 157 * scale);
                SelectObject(dc, old_active);
                DeleteObject(active);
                label(
                    dc,
                    scale,
                    49,
                    145,
                    "Agent brief",
                    12,
                    FW_SEMIBOLD as i32,
                    dark,
                );
                label(
                    dc,
                    scale,
                    28,
                    184,
                    "Ember workspace",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );
                label(
                    dc,
                    scale,
                    28,
                    216,
                    "Release notes",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );
                label(
                    dc,
                    scale,
                    24,
                    262,
                    "SAVED",
                    9,
                    FW_SEMIBOLD as i32,
                    color(0x77, 0x74, 0x6f),
                );
                label(
                    dc,
                    scale,
                    28,
                    289,
                    "Research queue",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );
                label(
                    dc,
                    scale,
                    28,
                    321,
                    "Design references",
                    12,
                    400,
                    color(0x51, 0x50, 0x56),
                );

                let command_top = bounds.bottom - 218 * scale;
                fill(
                    dc,
                    RECT {
                        left: 17 * scale,
                        top: command_top,
                        right: bounds.right - 17 * scale,
                        bottom: command_top + 42 * scale,
                    },
                    color(0xfb, 0xfa, 0xf6),
                );
                label(
                    dc,
                    scale,
                    29,
                    command_top / scale + 12,
                    "Ask Rivet about this page",
                    12,
                    400,
                    color(0x77, 0x74, 0x6f),
                );
                let card_top = bounds.bottom - 145 * scale;
                fill(
                    dc,
                    RECT {
                        left: 17 * scale,
                        top: card_top,
                        right: bounds.right - 17 * scale,
                        bottom: card_top + 2 * scale,
                    },
                    dark,
                );
                label(
                    dc,
                    scale,
                    24,
                    card_top / scale + 17,
                    "One browser,",
                    16,
                    FW_SEMIBOLD as i32,
                    dark,
                );
                label(
                    dc,
                    scale,
                    24,
                    card_top / scale + 38,
                    "wherever you work.",
                    16,
                    400,
                    dark,
                );
                label(
                    dc,
                    scale,
                    24,
                    card_top / scale + 77,
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

    unsafe extern "system" fn find_renderer(window: HWND, parameter: LPARAM) -> i32 {
        let renderers = &mut *(parameter as *mut Vec<HWND>);
        let mut class_name = [0_u16; 64];
        let length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        let class_name = String::from_utf16_lossy(&class_name[..length.max(0) as usize]);
        if matches!(
            class_name.as_str(),
            "Chrome_RenderWidgetHostHWND" | "Intermediate D3D Window"
        ) {
            renderers.push(window);
        }
        1
    }

    unsafe fn set_taskbar_icon(parent: HWND) {
        let mut module_path = [0_u16; 32768];
        let length = GetModuleFileNameW(
            std::ptr::null_mut(),
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
        set_taskbar_icon(parent);
        let scale = (GetDpiForWindow(parent).max(96) / 96) as i32;
        let panel_width = PANEL_WIDTH * scale;
        let mut window_class: WNDCLASSW = std::mem::zeroed();
        window_class.lpfnWndProc = Some(panel_proc);
        window_class.hInstance = GetModuleHandleW(std::ptr::null());
        window_class.lpszClassName = PANEL_CLASS.as_ptr();
        RegisterClassW(&window_class);
        let panel = CreateWindowExW(
            0,
            PANEL_CLASS.as_ptr(),
            PANEL_CLASS.as_ptr(),
            // A Chromium Direct3D child surface can compose over every sibling child HWND, even
            // when Rivet is topmost in the child z-order. The whole Rivet surface is an owned
            // popup in the browser process, so it composes above that renderer reliably.
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            panel_width,
            48,
            parent,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if panel.is_null() {
            return;
        }
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
                let mut screen_origin = POINT { x: 0, y: 0 };
                ClientToScreen(parent, &mut screen_origin);
                let height = bounds.bottom - bounds.top;
                SetWindowPos(
                    panel,
                    std::ptr::null_mut(),
                    screen_origin.x,
                    screen_origin.y,
                    panel_width,
                    height,
                    // Chromium creates a Direct3D child after our injected worker starts. Do
                    // not preserve the old sibling order or that renderer covers Rivet entirely.
                    SWP_NOACTIVATE,
                );
                // The renderer is a Chromium child HWND. Keeping its left edge at zero leaves
                // the page underneath Rivet. Resize the renderer itself so page layout and
                // hit-testing use the remaining pane, rather than merely hiding its left edge.
                let mut renderers = Vec::new();
                EnumChildWindows(
                    parent,
                    Some(find_renderer),
                    (&mut renderers as *mut Vec<HWND>) as isize,
                );
                for renderer in renderers {
                    if IsWindow(renderer) != 0 {
                        SetWindowPos(
                            renderer,
                            std::ptr::null_mut(),
                            panel_width,
                            0,
                            (bounds.right - bounds.left - panel_width).max(1),
                            height,
                            SWP_NOACTIVATE | SWP_NOZORDER,
                        );
                    }
                }
                // This is an owned popup because Chromium's Direct3D child surface can cover
                // an ordinary sibling child. Its region extends beyond the right edge, leaving
                // that edge square while matching the browser's rounded outer-left corners.
                let radius = 14 * scale;
                let region =
                    CreateRoundRectRgn(0, 0, panel_width + radius, height, radius * 2, radius * 2);
                SetWindowRgn(panel, region, 1);
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
