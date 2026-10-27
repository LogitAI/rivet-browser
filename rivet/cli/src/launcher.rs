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
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_NOEXPAND, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetDlgItem, GetMessageW,
        LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetWindowPos, ShowWindow,
        TranslateMessage, CW_USEDEFAULT, IDC_ARROW, MB_ICONERROR, MB_OK, MSG, SWP_NOACTIVATE,
        SWP_NOZORDER, SW_SHOW, WM_COMMAND, WM_DESTROY, WM_DPICHANGED, WM_SETFONT, WNDCLASSW,
        WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    };

    const HEADLINE: usize = 101;
    const DETAIL: usize = 102;
    /// Browser buttons use consecutive control identifiers from this value, in `BROWSERS` order.
    const FIRST_BROWSER: usize = 200;
    static CONTROL_FONT: AtomicIsize = AtomicIsize::new(0);

    /// A Chromium browser the picker offers, with the App Paths name its installer registers.
    struct Browser {
        label: &'static str,
        executable: &'static str,
    }

    const BROWSERS: [Browser; 5] = [
        Browser {
            label: "Google Chrome",
            executable: "chrome.exe",
        },
        Browser {
            label: "Microsoft Edge",
            executable: "msedge.exe",
        },
        Browser {
            label: "Brave",
            executable: "brave.exe",
        },
        Browser {
            label: "Naver Whale",
            executable: "whale.exe",
        },
        Browser {
            label: "Samsung Internet",
            executable: "samsunginternet.exe",
        },
    ];

    /// Two buttons per row below the description, in device-independent pixels.
    fn button_bounds(index: usize) -> (i32, i32, i32, i32) {
        let column = (index % 2) as i32;
        let row = (index / 2) as i32;
        (32 + column * 206, 128 + row * 52, 190, 40)
    }

    fn window_height() -> i32 {
        let rows = BROWSERS.len().div_ceil(2) as i32;
        128 + rows * 52 + 64
    }

    fn scale(value: i32, dpi: u32) -> i32 {
        (value * dpi as i32 + 48) / 96
    }

    unsafe fn layout(window: HWND, dpi: u32) {
        let text = [(HEADLINE, (32, 36, 380, 26)), (DETAIL, (32, 72, 390, 22))];
        let buttons =
            (0..BROWSERS.len()).map(|index| (FIRST_BROWSER + index, button_bounds(index)));
        for (id, (x, y, width, height)) in text.into_iter().chain(buttons) {
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
            for id in [HEADLINE, DETAIL]
                .into_iter()
                .chain(FIRST_BROWSER..FIRST_BROWSER + BROWSERS.len())
            {
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
    const WINDOW_CLASS: *const u16 = windows_sys::w!("RivetLauncher");
    const STATIC: *const u16 = windows_sys::w!("STATIC");
    const BUTTON: *const u16 = windows_sys::w!("BUTTON");

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

    /// Reads the default value of an App Paths key, which is where installers register the
    /// browser they own. Nothing is guessed when the key is absent or points at a missing file.
    fn registered_executable(root: HKEY, name: &str) -> Option<PathBuf> {
        let key = wide(&format!(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{name}"
        ));
        let mut bytes = 0u32;
        let flags = RRF_RT_REG_SZ | RRF_NOEXPAND;
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                std::ptr::null(),
                flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut bytes,
            )
        };
        if status != 0 || bytes < 2 {
            return None;
        }
        let mut value = vec![0u16; bytes as usize / 2 + 1];
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                std::ptr::null(),
                flags,
                std::ptr::null_mut(),
                value.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status != 0 {
            return None;
        }
        let length = value.iter().position(|unit| *unit == 0)?;
        let path = PathBuf::from(String::from_utf16_lossy(&value[..length]));
        path.is_file().then_some(path)
    }

    fn installed_browser(name: &str) -> Option<PathBuf> {
        [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
            .into_iter()
            .find_map(|root| registered_executable(root, name))
    }

    fn launch(browser: &Browser) -> Result<(), String> {
        let browser = installed_browser(browser.executable).ok_or_else(|| {
            format!("{} is not installed. Choose another browser or use the rivet CLI with an explicit executable path.", browser.label)
        })?;
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
                if let Some(browser) = control
                    .checked_sub(FIRST_BROWSER)
                    .and_then(|index| BROWSERS.get(index))
                {
                    match launch(browser) {
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
            class.lpszClassName = WINDOW_CLASS;
            if RegisterClassW(&class) == 0 {
                return Err("could not register the Rivet launcher window".into());
            }
            let title = wide("Rivet Browser");
            let window = CreateWindowExW(
                0,
                WINDOW_CLASS,
                title.as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                scale(480, GetDpiForSystem()),
                scale(window_height(), GetDpiForSystem()),
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
            CreateWindowExW(
                0,
                STATIC,
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
                STATIC,
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
            for (index, browser) in BROWSERS.iter().enumerate() {
                // A browser that is not installed stays visible but cannot be chosen, so the
                // picker never starts a guessed executable.
                let installed = installed_browser(browser.executable).is_some();
                let label = if installed {
                    format!("Use {}", browser.label)
                } else {
                    format!("{} (not installed)", browser.label)
                };
                let label = wide(&label);
                let button = CreateWindowExW(
                    0,
                    BUTTON,
                    label.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    0,
                    0,
                    0,
                    window,
                    (FIRST_BROWSER + index) as _,
                    instance,
                    std::ptr::null(),
                );
                EnableWindow(button, i32::from(installed));
            }
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
        use super::{button_bounds, scale, BROWSERS};

        #[test]
        fn fr1_picker_offers_every_supported_chromium_browser() {
            let names: Vec<_> = BROWSERS.iter().map(|browser| browser.executable).collect();
            assert_eq!(
                names,
                [
                    "chrome.exe",
                    "msedge.exe",
                    "brave.exe",
                    "whale.exe",
                    "samsunginternet.exe"
                ]
            );
        }

        #[test]
        fn fr1_picker_buttons_do_not_overlap() {
            for first in 0..BROWSERS.len() {
                for second in first + 1..BROWSERS.len() {
                    let (ax, ay, aw, ah) = button_bounds(first);
                    let (bx, by, bw, bh) = button_bounds(second);
                    assert!(ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay);
                }
            }
        }

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
