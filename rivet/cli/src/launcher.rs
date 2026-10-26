#![cfg_attr(windows, windows_subsystem = "windows")]

//! The explicit browser picker for people who start Rivet without a terminal.
//!
//! It deliberately offers a browser choice rather than guessing one. The selected browser still
//! starts through `rivet session start`, so the graphical and CLI entry points share one path.

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::process::Command;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW,
        PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage, CW_USEDEFAULT, IDC_ARROW,
        MB_ICONERROR, MB_OK, MSG, SW_SHOW, WM_COMMAND, WM_DESTROY, WNDCLASSW, WS_CAPTION, WS_CHILD,
        WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    };

    const EDGE_BUTTON: usize = 101;
    const CHROME_BUTTON: usize = 102;
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
                480,
                300,
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
                std::ptr::null_mut(),
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
                std::ptr::null_mut(),
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
            ShowWindow(window, SW_SHOW);
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
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
