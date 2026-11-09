//! The Win32 adapter of the operating system layer.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COMMAND};

use super::{WindowCommands, WindowId};

impl WindowId {
    pub(crate) fn from_hwnd(window: HWND) -> Self {
        Self::from_raw(window as usize)
    }

    pub(crate) fn hwnd(self) -> HWND {
        self.raw() as HWND
    }
}

/// The real operating system: calls into Win32.
pub(crate) struct Win32;

impl WindowCommands for Win32 {
    fn send_command(&self, window: WindowId, id: usize) {
        // Sending a message to a window handle is safe even if the window has gone: Windows then
        // fails the call and nothing is delivered.
        unsafe {
            SendMessageW(window.hwnd(), WM_COMMAND, id, 0);
        }
    }
}

thread_local! {
    /// The top-level windows made on this thread while the move command runs.
    static CREATED: std::cell::RefCell<Vec<HWND>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Sends the command `command` to `window` and returns the top-level windows made on this thread
/// while it ran. A browser makes the window for a command inside it, on this thread, so the
/// thread's own window-creation hook sees it being created.
pub(crate) unsafe fn send_command_collecting_windows(window: HWND, command: usize) -> Vec<HWND> {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_CBT, WM_COMMAND,
    };
    CREATED.with(|created| created.borrow_mut().clear());
    let hook = SetWindowsHookExW(
        WH_CBT,
        Some(creation_hook),
        std::ptr::null_mut(),
        GetCurrentThreadId(),
    );
    if hook.is_null() {
        crate::trace::trace("os: no window-creation hook, so the command is not sent");
        return Vec::new();
    }
    SendMessageW(window, WM_COMMAND, command, 0);
    UnhookWindowsHookEx(hook);
    CREATED.with(|created| std::mem::take(&mut *created.borrow_mut()))
}

/// Keeps every top-level window made on the thread while the command runs.
unsafe extern "system" fn creation_hook(
    code: i32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, CBT_CREATEWNDW, HCBT_CREATEWND, WS_CHILD,
    };
    if code == HCBT_CREATEWND as i32 && lparam != 0 {
        let create = &*(lparam as *const CBT_CREATEWNDW);
        let top_level = !create.lpcs.is_null() && ((*create.lpcs).style as u32 & WS_CHILD) == 0;
        if top_level {
            CREATED.with(|created| created.borrow_mut().push(wparam as HWND));
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// The watch of `NewWindows` on this thread: its hook, the place new windows take, and the windows
/// it saw made.
struct Watch {
    hook: windows_sys::Win32::UI::WindowsAndMessaging::HHOOK,
    place: super::ScreenRect,
    made: Vec<HWND>,
}

/// A running tick: its window, its timer and what it calls.
type Tick = (usize, usize, fn(WindowId));

thread_local! {
    static WATCH: std::cell::RefCell<Option<Watch>> = const { std::cell::RefCell::new(None) };
    /// What each running tick calls, by window and timer.
    static TICKS: std::cell::RefCell<Vec<Tick>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

impl super::NewWindows for Win32 {
    fn launch(&self, program: &std::path::Path, arguments: &[String]) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        if rivet_platform_windows::focus::keep_offscreen() {
            // A test run starts the program as it started the browser: with SW_SHOWNOACTIVATE and
            // no inherited handle, so neither this process nor a window it opens is activated.
            return rivet_platform_windows::start::spawn_browser(
                program,
                arguments,
                &[],
                0x0800_0000,
            )
            .map(drop)
            .map_err(|error| error.to_string());
        }
        // CREATE_NO_WINDOW, and no handle of the browser reaches the program's standard streams.
        std::process::Command::new(program)
            .args(arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .map(drop)
            .map_err(|error| error.to_string())
    }

    fn watch_new_windows(&self, place: super::ScreenRect) -> Result<(), String> {
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_CBT};
        if WATCH.with(|watch| watch.borrow().is_some()) {
            return Err("a watch for new windows is already running".into());
        }
        // A thread hook sees only the windows of this thread, and it is removed by `stop_watching`.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_CBT,
                Some(watch_hook),
                std::ptr::null_mut(),
                GetCurrentThreadId(),
            )
        };
        if hook.is_null() {
            return Err("no window-creation hook".into());
        }
        WATCH.with(|watch| {
            *watch.borrow_mut() = Some(Watch {
                hook,
                place,
                made: Vec::new(),
            })
        });
        Ok(())
    }

    fn stop_watching(&self) -> Vec<WindowId> {
        use windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;
        let Some(watch) = WATCH.with(|watch| watch.borrow_mut().take()) else {
            return Vec::new();
        };
        // The hook was made by `watch_new_windows` on this thread and is removed once.
        unsafe {
            UnhookWindowsHookEx(watch.hook);
        }
        watch.made.into_iter().map(WindowId::from_hwnd).collect()
    }

    fn watched(&self) -> Vec<WindowId> {
        WATCH.with(|watch| {
            watch.try_borrow().map_or(Vec::new(), |watch| {
                watch.as_ref().map_or(Vec::new(), |watch| {
                    watch
                        .made
                        .iter()
                        .copied()
                        .map(WindowId::from_hwnd)
                        .collect()
                })
            })
        })
    }

    fn set_hidden(&self, window: WindowId, hidden: bool) {
        // Cloaking a window that has gone fails without effect.
        unsafe { cloak(window.hwnd(), hidden) }
    }

    fn close_window(&self, window: WindowId) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        // A message posted to a window that has gone is not delivered.
        unsafe {
            PostMessageW(window.hwnd(), WM_CLOSE, 0, 0);
        }
    }

    fn browser_windows(&self) -> Vec<WindowId> {
        crate::frame::browser_windows_of_process()
            .into_iter()
            .map(WindowId::from_hwnd)
            .collect()
    }

    fn start_ticking(&self, window: WindowId, timer: usize, every_ms: u32, tick: fn(WindowId)) {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetTimer;
        TICKS.with(|ticks| {
            let mut ticks = ticks.borrow_mut();
            ticks.retain(|(known, id, _)| (*known, *id) != (window.raw(), timer));
            ticks.push((window.raw(), timer, tick));
        });
        // The timer belongs to the window, so it ends with it.
        unsafe {
            SetTimer(window.hwnd(), timer, every_ms, Some(tick_proc));
        }
    }

    fn stop_ticking(&self, window: WindowId, timer: usize) {
        use windows_sys::Win32::UI::WindowsAndMessaging::KillTimer;
        TICKS.with(|ticks| {
            ticks
                .borrow_mut()
                .retain(|(known, id, _)| (*known, *id) != (window.raw(), timer))
        });
        unsafe {
            KillTimer(window.hwnd(), timer);
        }
    }
}

/// Hides or shows a window with the Desktop Window Manager's cloak.
unsafe fn cloak(window: HWND, hidden: bool) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
    let value: i32 = i32::from(hidden);
    DwmSetWindowAttribute(
        window,
        DWMWA_CLOAK as u32,
        (&value as *const i32).cast(),
        std::mem::size_of::<i32>() as u32,
    );
}

/// Runs what `start_ticking` named for this window and timer.
unsafe extern "system" fn tick_proc(window: HWND, _message: u32, timer: usize, _time: u32) {
    let tick = TICKS.with(|ticks| {
        ticks.try_borrow().ok().and_then(|ticks| {
            ticks
                .iter()
                .find(|(known, id, _)| (*known, *id) == (window as usize, timer))
                .map(|(_, _, tick)| *tick)
        })
    });
    if let Some(tick) = tick {
        tick(WindowId::from_hwnd(window));
    }
}

/// While a watch runs: a top-level browser window is created at the watch's place and hidden
/// before it is ever drawn, and it is not activated.
unsafe extern "system" fn watch_hook(
    code: i32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, CBT_CREATEWNDW, HCBT_ACTIVATE, HCBT_CREATEWND, WS_CHILD,
    };
    let window = wparam as HWND;
    if code == HCBT_CREATEWND as i32 && lparam != 0 {
        let create = &*(lparam as *const CBT_CREATEWNDW);
        let top_level = !create.lpcs.is_null() && ((*create.lpcs).style as u32 & WS_CHILD) == 0;
        if top_level && crate::frame::is_browser_window_class(window) {
            let place = WATCH.with(|watch| {
                let mut watch = watch.try_borrow_mut().ok()?;
                let watch = watch.as_mut()?;
                watch.made.push(window);
                Some(watch.place)
            });
            if let Some([x, y, width, height]) = place {
                let lpcs = &mut *create.lpcs;
                lpcs.x = x;
                lpcs.y = y;
                lpcs.cx = width;
                lpcs.cy = height;
                cloak(window, true);
            }
        }
    } else if code == HCBT_ACTIVATE as i32 {
        let made = WATCH.with(|watch| {
            watch.try_borrow().is_ok_and(|watch| {
                watch
                    .as_ref()
                    .is_some_and(|watch| watch.made.contains(&window))
            })
        });
        if made {
            // A nonzero answer keeps the window from being activated.
            return 1;
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// A monitor as Windows reports it: its rectangle in physical pixels as left, top, right and bottom,
/// its scale (pixels per DIP) and whether it is the primary one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Monitor {
    pub rect: [i32; 4],
    pub scale: f32,
    pub primary: bool,
}

/// The window's display, its nearest monitor as `display::win::ScreenWin` picks it with
/// `MonitorFromWindow`, and the primary monitor.
pub(crate) unsafe fn monitors_of(window: HWND) -> Option<(Monitor, Monitor)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HMONITOR, MONITORINFO,
        MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe fn describe(monitor: HMONITOR) -> Option<Monitor> {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let (mut x, mut y) = (96u32, 96u32);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) != 0 {
            x = 96;
        }
        let r = info.rcMonitor;
        Some(Monitor {
            rect: [r.left, r.top, r.right, r.bottom],
            scale: x.max(96) as f32 / 96.0,
            // MONITORINFOF_PRIMARY
            primary: info.dwFlags & 1 != 0,
        })
    }
    let near = describe(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST))?;
    let primary = describe(MonitorFromPoint(
        POINT { x: 0, y: 0 },
        MONITOR_DEFAULTTOPRIMARY,
    ))?;
    Some((near, primary))
}
