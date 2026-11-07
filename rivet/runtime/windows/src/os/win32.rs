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
