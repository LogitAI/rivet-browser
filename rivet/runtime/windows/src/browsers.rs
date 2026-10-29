//! Finding the browsers a space can be based on, and showing a space of another one.
//!
//! Different browsers are different processes and a window cannot be moved between them, so a
//! space of another browser lives in its own window. Switching to it brings that window forward
//! when one already runs the space's profile, and otherwise asks the `rivet` launcher to start
//! it with the profile folder and the Rivet runtime. The window switched from stays open.

use crate::spaces::{BrowserKind, HandOver};
use std::path::PathBuf;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_NOEXPAND, RRF_RT_REG_SZ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, ShowWindow, GW_OWNER, HWND_MESSAGE, SW_RESTORE,
};

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
        .map(|path| rivet_platform_windows::resolve_browser_executable(&path))
}

/// The browser this window belongs to.
pub(crate) fn current() -> Option<BrowserKind> {
    let program = std::env::current_exe().ok()?;
    BrowserKind::from_program(&program.to_string_lossy())
}

/// The process that runs a browser profile. Chromium names the message-only window of its process
/// singleton after the profile folder, so the folder finds the process without reading anything of
/// its command line or memory.
fn profile_process(directory: &std::path::Path) -> Option<u32> {
    let class = wide("Chrome_MessageWindow");
    let wanted = directory.to_string_lossy();
    let mut previous: HWND = std::ptr::null_mut();
    loop {
        let found = unsafe { FindWindowExW(HWND_MESSAGE, previous, class.as_ptr(), std::ptr::null()) };
        if found.is_null() {
            return None;
        }
        previous = found;
        let mut name = [0u16; 1024];
        let length = unsafe { GetWindowTextW(found, name.as_mut_ptr(), name.len() as i32) };
        if length > 0 && crate::spaces::same_directory(&String::from_utf16_lossy(&name[..length as usize]), &wanted) {
            let mut process = 0u32;
            unsafe { GetWindowThreadProcessId(found, &mut process) };
            return Some(process).filter(|process| *process != 0);
        }
    }
}

/// The browser windows of a process, topmost first.
fn windows_of(process: u32) -> Vec<crate::spaces::Candidate> {
    struct Search {
        process: u32,
        found: Vec<crate::spaces::Candidate>,
    }
    unsafe extern "system" fn visit(window: HWND, parameter: isize) -> i32 {
        let search = &mut *(parameter as *mut Search);
        let mut owner = 0u32;
        GetWindowThreadProcessId(window, &mut owner);
        if owner != search.process {
            return 1;
        }
        let mut class = [0u16; 64];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
        if length <= 0 || String::from_utf16_lossy(&class[..length as usize]) != "Chrome_WidgetWin_1" {
            return 1;
        }
        search.found.push(crate::spaces::Candidate {
            window: window as isize,
            shown: IsWindowVisible(window) != 0 || IsIconic(window) != 0,
            owned: !GetWindow(window, GW_OWNER).is_null(),
        });
        1
    }
    let mut search = Search { process, found: Vec::new() };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as isize) };
    search.found
}

/// Brings the window that already runs the profile `directory` to the front. Returns whether there
/// was one. Only that window is touched, and nothing is moved into another window.
pub(crate) fn raise_existing(directory: &std::path::Path) -> bool {
    let Some(process) = profile_process(directory) else {
        return false;
    };
    let Some(window) = crate::spaces::pick_window(&windows_of(process)) else {
        return false;
    };
    let window = window as HWND;
    unsafe {
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        SetForegroundWindow(window);
    }
    true
}

/// Starts the other browser's session through the launcher. The wait happens off the window's
/// thread, and the window it was started from stays open whatever the outcome.
pub(crate) fn start(hand_over: HandOver, url: String, directory: PathBuf) {
    use std::os::windows::process::CommandExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    // One start at a time, so a double click starts one session.
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
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
        if let Err(error) = outcome {
            let message = format!("rivet: could not hand over to {}: {error}", hand_over.browser.label());
            crate::frame::log(&message);
            crate::vertical_tabs::trace(&message);
        }
        BUSY.store(false, Ordering::Release);
    });
}
