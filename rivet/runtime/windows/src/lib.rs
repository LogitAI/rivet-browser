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
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, EnumWindows, GetClientRect, GetWindowThreadProcessId, IsWindow,
        IsWindowVisible, SetWindowPos, ES_AUTOHSCROLL, SWP_NOZORDER, WS_BORDER, WS_CHILD,
        WS_TABSTOP, WS_VISIBLE,
    };

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
        let class = [
            b'S' as u16,
            b'T' as u16,
            b'A' as u16,
            b'T' as u16,
            b'I' as u16,
            b'C' as u16,
            0,
        ];
        let text = [
            b'R' as u16,
            b'I' as u16,
            b'V' as u16,
            b'E' as u16,
            b'T' as u16,
            0,
        ];
        let panel = CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER,
            0,
            0,
            280,
            48,
            parent,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if panel.is_null() {
            return;
        }
        let title = [
            b'R' as u16,
            b'I' as u16,
            b'V' as u16,
            b'E' as u16,
            b'T' as u16,
            0,
        ];
        let workspace = [
            b'W' as u16,
            b'O' as u16,
            b'R' as u16,
            b'K' as u16,
            b'S' as u16,
            b'P' as u16,
            b'A' as u16,
            b'C' as u16,
            b'E' as u16,
            0,
        ];
        let edit = [b'E' as u16, b'D' as u16, b'I' as u16, b'T' as u16, 0];
        let command = [
            b'A' as u16,
            b's' as u16,
            b'k' as u16,
            b' ' as u16,
            b'R' as u16,
            b'i' as u16,
            b'v' as u16,
            b'e' as u16,
            b't' as u16,
            b'.' as u16,
            b'.' as u16,
            b'.' as u16,
            0,
        ];
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            20,
            20,
            230,
            28,
            panel,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        CreateWindowExW(
            0,
            class.as_ptr(),
            workspace.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            20,
            76,
            230,
            24,
            panel,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        CreateWindowExW(
            0,
            edit.as_ptr(),
            command.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            16,
            124,
            248,
            32,
            panel,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        loop {
            if IsWindow(parent) == 0 || IsWindow(panel) == 0 {
                break;
            }
            let mut bounds = std::mem::zeroed();
            if GetClientRect(parent, &mut bounds) != 0 {
                SetWindowPos(
                    panel,
                    std::ptr::null_mut(),
                    0,
                    0,
                    280,
                    bounds.bottom - bounds.top,
                    SWP_NOZORDER,
                );
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
