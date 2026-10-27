#![cfg_attr(windows, windows_subsystem = "windows")]

//! The explicit browser picker for people who start Rivet without a terminal.
//!
//! It deliberately offers a browser choice rather than guessing one. The selected browser still
//! starts through `rivet session start`, so the graphical and CLI entry points share one path.

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateFontW, DeleteObject, CLEARTYPE_QUALITY, COLOR_WINDOW,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetDlgItem, GetMessageW,
        LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetWindowPos, ShowWindow,
        TranslateMessage, CW_USEDEFAULT, IDC_ARROW, MB_ICONERROR, MB_OK, MSG, SWP_NOACTIVATE,
        SWP_NOZORDER, SW_SHOW, WM_COMMAND, WM_DESTROY, WM_DPICHANGED, WM_SETFONT, WNDCLASSW,
        WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    };

    const EDGE_BUTTON: usize = 101;
    const CHROME_BUTTON: usize = 102;
    const HEADLINE: usize = 103;
    const DETAIL: usize = 104;
    static CONTROL_FONT: AtomicIsize = AtomicIsize::new(0);

    fn scale(value: i32, dpi: u32) -> i32 {
        (value * dpi as i32 + 48) / 96
    }

    unsafe fn layout(window: HWND, dpi: u32) {
        for (id, x, y, width, height) in [
            (HEADLINE, 32, 36, 380, 26),
            (DETAIL, 32, 72, 390, 22),
            (EDGE_BUTTON, 32, 128, 190, 40),
            (CHROME_BUTTON, 238, 128, 190, 40),
        ] {
            let control = GetDlgItem(window, id as i32);
            if !control.is_null() {
                SetWindowPos(
                    control,
                    std::ptr::null_mut(),
                    scale(x, dpi),
                    scale(y, dpi),
                    scale(width, dpi),
                    scale(height, dpi),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
        let face = wide("Segoe UI");
        let font = CreateFontW(
            -scale(14, dpi),
            0,
            0,
            0,
            400,
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
        if !font.is_null() {
            for id in [HEADLINE, DETAIL, EDGE_BUTTON, CHROME_BUTTON] {
                let control = GetDlgItem(window, id as i32);
                if !control.is_null() {
                    SendMessageW(control, WM_SETFONT, font as usize, 1);
                }
            }
            let previous = CONTROL_FONT.swap(font as isize, Ordering::AcqRel);
            if previous != 0 {
                DeleteObject(previous as _);
            }
        }
    }
    const WINDOW_CLASS: [u16; 15] = [
        b'R' as u16,
        b'i' as u16,
        b'v' as u16,
        b'e' as u16,
        b't' as u16,
        b'L' as u16,
        b'a' as u16,
        b'u' as u16,
        b'n' as u16,
        b'c' as u16,
        b'h' as u16,
        b'e' as u16,
        b'r' as u16,
        0,
        0,
    ];
    const STATIC: [u16; 7] = [
        b'S' as u16,
        b'T' as u16,
        b'A' as u16,
        b'T' as u16,
        b'I' as u16,
        b'C' as u16,
        0,
    ];
    const BUTTON: [u16; 7] = [
        b'B' as u16,
        b'U' as u16,
        b'T' as u16,
        b'T' as u16,
        b'O' as u16,
        b'N' as u16,
        0,
    ];

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn message(text: &str) {
        let caption = wide("Rivet Browser");
        let body = wide(text);
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }

    fn browser_path(which: usize) -> PathBuf {
        match which {
            EDGE_BUTTON => {
                PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
            }
            CHROME_BUTTON => {
                PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
            }
            _ => unreachable!(),
        }
    }

    fn launch(which: usize) -> Result<(), String> {
        let browser = browser_path(which);
        if !browser.is_file() {
            return Err(format!("{} is not installed. Choose the other browser or use the rivet CLI with an explicit executable path.", browser.display()));
        }
        let executable = std::env::current_exe()
            .map_err(|error| format!("could not resolve the Rivet launcher location: {error}"))?
            .with_file_name("rivet.exe");
        Command::new(executable)
            .args([
                "session",
                "start",
                "--browser",
                browser.to_string_lossy().as_ref(),
                "--url",
                "https://example.com/",
                "--display",
                "visible",
            ])
            .spawn()
            .map_err(|error| format!("could not start Rivet: {error}"))?;
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        window: HWND,
        message_id: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message_id {
            WM_DPICHANGED => {
                let rect = &*(lparam as *const RECT);
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                layout(window, (wparam & 0xffff) as u32);
                0
            }
            WM_COMMAND => {
                let control = wparam & 0xffff;
                if control == EDGE_BUTTON || control == CHROME_BUTTON {
                    match launch(control) {
                        Ok(()) => {
                            DestroyWindow(window);
                        }
                        Err(error) => {
                            message(&error);
                        }
                    }
                    return 0;
                }
                DefWindowProcW(window, message_id, wparam, lparam)
            }
            WM_DESTROY => {
                let font = CONTROL_FONT.swap(0, Ordering::AcqRel);
                if font != 0 {
                    DeleteObject(font as _);
                }
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(window, message_id, wparam, lparam),
        }
    }

    pub fn run() -> Result<(), String> {
        unsafe {
            // The picker can open on any monitor. Opt into the same per-monitor coordinate
            // system as Rivet's in-browser frame before creating its first HWND.
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(std::ptr::null());
            let mut class: WNDCLASSW = std::mem::zeroed();
            class.lpfnWndProc = Some(window_proc);
            class.hInstance = instance;
            class.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            class.hbrBackground = (COLOR_WINDOW as usize + 1) as _;
            class.lpszClassName = WINDOW_CLASS.as_ptr();
            if RegisterClassW(&class) == 0 {
                return Err("could not register the Rivet launcher window".into());
            }
            let title = wide("Rivet Browser");
            let window = CreateWindowExW(
                0,
                WINDOW_CLASS.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                scale(480, GetDpiForSystem()),
                scale(300, GetDpiForSystem()),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if window.is_null() {
                return Err("could not create the Rivet launcher window".into());
            }
            let headline = wide("Open your browser with Rivet");
            let detail = wide("Choose the browser profile you want to keep using.");
            let edge = wide("Use Microsoft Edge");
            let chrome = wide("Use Google Chrome");
            CreateWindowExW(
                0,
                STATIC.as_ptr(),
                headline.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                32,
                36,
                380,
                26,
                window,
                HEADLINE as _,
                instance,
                std::ptr::null(),
            );
            CreateWindowExW(
                0,
                STATIC.as_ptr(),
                detail.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                32,
                72,
                390,
                22,
                window,
                DETAIL as _,
                instance,
                std::ptr::null(),
            );
            CreateWindowExW(
                0,
                BUTTON.as_ptr(),
                edge.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                32,
                128,
                190,
                40,
                window,
                EDGE_BUTTON as _,
                instance,
                std::ptr::null(),
            );
            CreateWindowExW(
                0,
                BUTTON.as_ptr(),
                chrome.as_ptr(),
                WS_CHILD | WS_VISIBLE,
                238,
                128,
                190,
                40,
                window,
                CHROME_BUTTON as _,
                instance,
                std::ptr::null(),
            );
            layout(window, GetDpiForWindow(window).max(96));
            ShowWindow(window, SW_SHOW);
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::scale;

        #[test]
        fn fr2_launcher_scales_layout_for_monitor_dpi() {
            assert_eq!(scale(480, 96), 480);
            assert_eq!(scale(480, 192), 960);
            assert_eq!(scale(32, 144), 48);
        }
    }
}

fn main() {
    #[cfg(windows)]
    if let Err(error) = windows::run() {
        eprintln!("rivet-launcher: {error}");
    }

    #[cfg(not(windows))]
    eprintln!("rivet-launcher is only available on Windows");
}
