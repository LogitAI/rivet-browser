//! The client end of a Rivet window's control pipe.

use crate::BrowserWindow;

/// Connects to the control pipe of `window`, waiting up to `wait` for the window to serve it. The
/// pipe is accepted only when the browser process of the window serves it, so another program that
/// took the name first never receives a request.
#[cfg(windows)]
pub fn connect(window: BrowserWindow, wait: std::time::Duration) -> Result<std::fs::File, String> {
    use std::os::windows::io::FromRawHandle;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING, SECURITY_IDENTIFICATION,
        SECURITY_SQOS_PRESENT,
    };
    use windows_sys::Win32::System::Pipes::{GetNamedPipeServerProcessId, WaitNamedPipeW};

    let name: Vec<u16> = rivet_protocol::channel::pipe_name(window.process_id, window.handle)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let deadline = Instant::now() + wait;
    loop {
        // Identification only: the browser may learn who connects but never act as this user.
        let pipe = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        if pipe != INVALID_HANDLE_VALUE {
            let mut server = 0u32;
            let owned = unsafe { GetNamedPipeServerProcessId(pipe, &mut server) } != 0
                && server == window.process_id;
            if !owned {
                unsafe { CloseHandle(pipe) };
                return Err(format!(
                    "the control pipe of window {:x} is not served by browser process {}",
                    window.handle, window.process_id
                ));
            }
            return Ok(unsafe { std::fs::File::from_raw_handle(pipe as _) });
        }
        let error = unsafe { GetLastError() };
        if Instant::now() >= deadline {
            return Err(format!(
                "the Rivet window {:x} does not answer on its control pipe (error {error}); is the runtime loaded?",
                window.handle
            ));
        }
        if error == ERROR_PIPE_BUSY {
            unsafe { WaitNamedPipeW(name.as_ptr(), 500) };
        } else {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}

/// Reports that the control pipe is unavailable on non-Windows targets.
#[cfg(not(windows))]
pub fn connect(
    _window: BrowserWindow,
    _wait: std::time::Duration,
) -> Result<std::fs::File, String> {
    Err("the Rivet control pipe is only available on Windows".into())
}
