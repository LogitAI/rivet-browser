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
