//! Finding the browsers a space can be based on, and starting a session in another one.
//!
//! A space of another browser cannot be shown inside this window, so switching to it asks the
//! `rivet` launcher to start that browser with the space's profile folder and the Rivet runtime,
//! and closes this window once the other one is up.

use crate::spaces::{BrowserKind, HandOver};
use std::path::PathBuf;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_NOEXPAND, RRF_RT_REG_SZ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The default value of an App Paths key, which is where an installer registers its browser.
/// Nothing is guessed when the key is absent or points at a missing file.
fn registered(root: HKEY, name: &str) -> Option<PathBuf> {
    let key = wide(&format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{name}"));
    let flags = RRF_RT_REG_SZ | RRF_NOEXPAND;
    let mut bytes = 0u32;
    let status = unsafe {
        RegGetValueW(root, key.as_ptr(), std::ptr::null(), flags, std::ptr::null_mut(), std::ptr::null_mut(), &mut bytes)
    };
    if status != 0 || bytes < 2 {
        return None;
    }
    let mut value = vec![0u16; bytes as usize / 2 + 1];
    let status = unsafe {
        RegGetValueW(root, key.as_ptr(), std::ptr::null(), flags, std::ptr::null_mut(), value.as_mut_ptr().cast(), &mut bytes)
    };
    if status != 0 {
        return None;
    }
    let length = value.iter().position(|unit| *unit == 0)?;
    let path = PathBuf::from(String::from_utf16_lossy(&value[..length]).trim_matches('"'));
    path.is_file().then_some(path)
}

/// Where the browser is installed, from its App Paths registration.
pub(crate) fn installed(browser: BrowserKind) -> Option<PathBuf> {
    [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
        .into_iter()
        .find_map(|root| registered(root, browser.executable()))
}

/// The browser this window belongs to.
pub(crate) fn current() -> Option<BrowserKind> {
    let program = std::env::current_exe().ok()?;
    BrowserKind::from_program(&program.to_string_lossy())
}

/// Starts the other browser's session through the launcher, and closes `window` when it is up.
/// The wait happens off the window's thread, and a failed start leaves the window as it is.
pub(crate) fn start_and_close(window: HWND, hand_over: HandOver, url: String, directory: PathBuf) {
    use std::os::windows::process::CommandExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    // One hand-over at a time, so a double click starts one session.
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    let window = window as isize;
    std::thread::spawn(move || {
        let outcome = (|| {
            let program = installed(hand_over.browser)
                .ok_or_else(|| format!("{} is not installed", hand_over.browser.label()))?;
            let launcher = directory.join("rivet.exe");
            if !launcher.is_file() {
                return Err(format!("the launcher {} is missing", launcher.display()));
            }
            let runtime = directory.join("rivet_runtime_windows.dll");
            let arguments =
                crate::spaces::start_arguments(&hand_over, &program, runtime.is_file().then_some(runtime.as_path()), &url);
            // The launcher's error goes to a file, not a pipe: the browser it starts inherits the
            // launcher's handles, and a pipe would stay open until that browser exits.
            let report = std::env::temp_dir().join(format!("rivet-handover-{}.txt", std::process::id()));
            let file = std::fs::File::create(&report).map_err(|error| format!("could not make {}: {error}", report.display()))?;
            let status = std::process::Command::new(&launcher)
                .args(arguments)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(file)
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .status()
                .map_err(|error| format!("could not run the launcher: {error}"))?;
            let stderr = std::fs::read_to_string(&report).unwrap_or_default();
            let _ = std::fs::remove_file(&report);
            if crate::spaces::handover_started(status.success(), &stderr) {
                Ok(())
            } else {
                Err(stderr)
            }
        })();
        match outcome {
            Ok(()) => unsafe {
                PostMessageW(window as HWND, WM_CLOSE, 0, 0);
            },
            Err(error) => {
                let message = format!("rivet: could not hand over to {}: {error}", hand_over.browser.label());
                crate::frame::log(&message);
                crate::vertical_tabs::trace(&message);
            }
        }
        BUSY.store(false, Ordering::Release);
    });
}
