//! Windows primitives used to load the Rivet runtime into an explicitly selected process.
//!
//! The API does not discover processes and never chooses a browser. Its caller must supply the
//! browser process identifier and the absolute runtime DLL path after applying Rivet's session
//! ownership checks.

use std::path::Path;
use std::time::{Duration, Instant};

/// A top-level browser window and the browser process that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserWindow {
    pub handle: isize,
    pub process_id: u32,
}

/// A failure while identifying the browser window that a Rivet launch created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowDiscoveryError {
    UnsupportedPlatform,
    BrowserPathMissing,
    TimedOut,
}

/// A failure while closing a Rivet-owned browser window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowCloseError {
    UnsupportedPlatform,
    WindowIsNotOwnedByTheSelectedBrowser,
    CloseMessageFailed,
}

impl std::fmt::Display for WindowCloseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(f, "browser-window closing is only available on Windows")
            }
            Self::WindowIsNotOwnedByTheSelectedBrowser => write!(
                f,
                "the recorded Rivet window no longer belongs to the selected browser process"
            ),
            Self::CloseMessageFailed => {
                write!(f, "could not request the Rivet browser window to close")
            }
        }
    }
}

impl std::error::Error for WindowCloseError {}

impl std::fmt::Display for WindowDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(f, "browser-window discovery is only available on Windows")
            }
            Self::BrowserPathMissing => write!(f, "the selected browser executable does not exist"),
            Self::TimedOut => write!(
                f,
                "the launched browser did not create a discoverable app window"
            ),
        }
    }
}

impl std::error::Error for WindowDiscoveryError {}

/// Returns visible top-level windows owned by the exact browser executable.
#[cfg(windows)]
pub fn browser_windows(browser: &Path) -> Result<Vec<BrowserWindow>, WindowDiscoveryError> {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    let browser = browser
        .canonicalize()
        .map_err(|_| WindowDiscoveryError::BrowserPathMissing)?;
    struct Enumeration<'a> {
        browser: &'a Path,
        windows: Vec<BrowserWindow>,
    }
    unsafe extern "system" fn visit_window(window: HWND, parameter: LPARAM) -> i32 {
        let enumeration = &mut *(parameter as *mut Enumeration<'_>);
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        if process_id == 0 {
            return 1;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return 1;
        }
        let mut image = vec![0u16; 32_768];
        let mut length = image.len() as u32;
        let found = QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut length) != 0;
        CloseHandle(process);
        if !found {
            return 1;
        }
        let image = String::from_utf16_lossy(&image[..length as usize]);
        let Ok(image) = Path::new(&image).canonicalize() else {
            return 1;
        };
        if same_path(&image, enumeration.browser) {
            enumeration.windows.push(BrowserWindow {
                handle: window as isize,
                process_id,
            });
        }
        1
    }

    let mut enumeration = Enumeration {
        browser: &browser,
        windows: Vec::new(),
    };
    unsafe {
        EnumWindows(
            Some(visit_window),
            (&mut enumeration as *mut Enumeration<'_>) as isize,
        );
    }
    Ok(enumeration.windows)
}

/// Waits for the app window that did not exist before Rivet started the browser command.
#[cfg(windows)]
pub fn wait_for_new_browser_window(
    browser: &Path,
    existing: &[BrowserWindow],
    timeout: Duration,
) -> Result<BrowserWindow, WindowDiscoveryError> {
    let deadline = Instant::now() + timeout;
    loop {
        let windows = browser_windows(browser)?;
        if let Some(window) = first_unseen_window(existing, &windows) {
            return Ok(window);
        }
        if Instant::now() >= deadline {
            return Err(WindowDiscoveryError::TimedOut);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Closes exactly one previously discovered browser window.
///
/// The window handle and process identifier are checked again to prevent a stale handle from
/// closing another application after Windows reuses it.
#[cfg(windows)]
pub fn close_browser_window(browser: &Path, window: BrowserWindow) -> Result<(), WindowCloseError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    let windows = browser_windows(browser)
        .map_err(|_| WindowCloseError::WindowIsNotOwnedByTheSelectedBrowser)?;
    if !windows.contains(&window) {
        return Err(WindowCloseError::WindowIsNotOwnedByTheSelectedBrowser);
    }
    if unsafe { PostMessageW(window.handle as _, WM_CLOSE, 0, 0) } == 0 {
        return Err(WindowCloseError::CloseMessageFailed);
    }
    Ok(())
}

#[cfg(windows)]
fn same_path(left: &Path, right: &Path) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

fn first_unseen_window(
    existing: &[BrowserWindow],
    current: &[BrowserWindow],
) -> Option<BrowserWindow> {
    current.iter().copied().find(|candidate| {
        !existing
            .iter()
            .any(|known| known.handle == candidate.handle)
    })
}

/// Reports that browser-window discovery is unavailable on non-Windows targets.
#[cfg(not(windows))]
pub fn browser_windows(_browser: &Path) -> Result<Vec<BrowserWindow>, WindowDiscoveryError> {
    Err(WindowDiscoveryError::UnsupportedPlatform)
}

/// Reports that browser-window discovery is unavailable on non-Windows targets.
#[cfg(not(windows))]
pub fn wait_for_new_browser_window(
    _browser: &Path,
    _existing: &[BrowserWindow],
    _timeout: Duration,
) -> Result<BrowserWindow, WindowDiscoveryError> {
    Err(WindowDiscoveryError::UnsupportedPlatform)
}

/// Reports that browser-window closing is unavailable on non-Windows targets.
#[cfg(not(windows))]
pub fn close_browser_window(
    _browser: &Path,
    _window: BrowserWindow,
) -> Result<(), WindowCloseError> {
    Err(WindowCloseError::UnsupportedPlatform)
}

/// A failure while preparing or performing one explicit runtime load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectError {
    UnsupportedPlatform,
    EmptyRuntimePath,
    MissingRuntimePath,
    RuntimePathContainsNul,
    OpenProcess(u32),
    AllocateRemoteMemory(u32),
    WriteRemoteMemory(u32),
    FindLoadLibrary,
    FindRuntimeStart,
    StartRemoteThread(u32),
    StartRuntime(u32),
    WaitRemoteThread(u32),
    RemoteLoadFailed,
    SnapshotModules(u32),
    RuntimeModuleNotFound,
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(f, "runtime injection is only available on Windows")
            }
            Self::EmptyRuntimePath => write!(f, "the runtime DLL path is empty"),
            Self::MissingRuntimePath => write!(f, "the runtime DLL does not exist"),
            Self::RuntimePathContainsNul => {
                write!(f, "the runtime DLL path contains a NUL character")
            }
            Self::OpenProcess(code) => write!(
                f,
                "could not open the selected browser process (Windows error {code})"
            ),
            Self::AllocateRemoteMemory(code) => write!(
                f,
                "could not allocate browser-process memory (Windows error {code})"
            ),
            Self::WriteRemoteMemory(code) => write!(
                f,
                "could not copy the runtime path into the browser process (Windows error {code})"
            ),
            Self::FindLoadLibrary => write!(f, "could not locate LoadLibraryW"),
            Self::FindRuntimeStart => {
                write!(f, "could not locate rivet_runtime_start in the runtime DLL")
            }
            Self::StartRemoteThread(code) => write!(
                f,
                "could not start the browser runtime loader (Windows error {code})"
            ),
            Self::StartRuntime(code) => write!(
                f,
                "could not start the Rivet runtime in the browser process (Windows error {code})"
            ),
            Self::WaitRemoteThread(code) => write!(
                f,
                "could not wait for the browser runtime loader (Windows error {code})"
            ),
            Self::RemoteLoadFailed => {
                write!(f, "the browser process did not load the Rivet runtime")
            }
            Self::SnapshotModules(code) => write!(
                f,
                "could not inspect modules in the selected browser process (Windows error {code})"
            ),
            Self::RuntimeModuleNotFound => {
                write!(
                    f,
                    "the selected browser process did not expose the loaded Rivet runtime"
                )
            }
        }
    }
}

impl std::error::Error for InjectError {}

/// Validates a runtime DLL before any Windows handle is opened.
pub fn validate_runtime_path(path: &Path) -> Result<(), InjectError> {
    if path.as_os_str().is_empty() {
        return Err(InjectError::EmptyRuntimePath);
    }
    if !path.is_file() {
        return Err(InjectError::MissingRuntimePath);
    }
    if path.to_string_lossy().contains('\0') {
        return Err(InjectError::RuntimePathContainsNul);
    }
    Ok(())
}

/// Returns the full-width base address of the selected runtime module in a browser process.
///
/// `GetExitCodeThread` can only return a 32-bit value, so it cannot safely provide an injected
/// DLL base address in a 64-bit Chrome process. Toolhelp reports the actual module address.
#[cfg(windows)]
pub fn remote_runtime_module_base(
    process_id: u32,
    runtime_path: &Path,
) -> Result<usize, InjectError> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };

    validate_runtime_path(runtime_path)?;
    let runtime = runtime_path
        .canonicalize()
        .map_err(|_| InjectError::MissingRuntimePath)?;
    unsafe {
        let snapshot =
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, process_id);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(InjectError::SnapshotModules(GetLastError()));
        }
        let mut entry: MODULEENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        let mut found = Module32FirstW(snapshot, &mut entry) != 0;
        while found {
            let length = entry
                .szExePath
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExePath.len());
            let path = String::from_utf16_lossy(&entry.szExePath[..length]);
            if Path::new(&path)
                .canonicalize()
                .is_ok_and(|candidate| same_path(&candidate, &runtime))
            {
                let base = entry.modBaseAddr as usize;
                CloseHandle(snapshot);
                return Ok(base);
            }
            entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
            found = Module32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    Err(InjectError::RuntimeModuleNotFound)
}

#[cfg(not(windows))]
pub fn remote_runtime_module_base(
    _process_id: u32,
    _runtime_path: &Path,
) -> Result<usize, InjectError> {
    Err(InjectError::UnsupportedPlatform)
}

/// Loads `runtime_path` into exactly `process_id` on Windows.
///
/// The caller is responsible for proving that the process belongs to the current Rivet session.
#[cfg(windows)]
pub fn inject_runtime(process_id: u32, runtime_path: &Path) -> Result<(), InjectError> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, FreeLibrary, GetLastError, HANDLE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleHandleW, GetProcAddress, LoadLibraryW,
    };
    use windows_sys::Win32::System::Memory::{
        VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject, INFINITE,
        LPTHREAD_START_ROUTINE, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION,
        PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
    };

    validate_runtime_path(runtime_path)?;
    let mut path = runtime_path.as_os_str().encode_wide().collect::<Vec<_>>();
    path.push(0);
    let bytes = path.len() * std::mem::size_of::<u16>();

    // Every handle and allocation below has one lexical owner so a failed injection cannot leave
    // an allocation in the selected browser process.
    unsafe {
        let process: HANDLE = OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            0,
            process_id,
        );
        if process.is_null() {
            return Err(InjectError::OpenProcess(GetLastError()));
        }
        let remote = VirtualAllocEx(
            process,
            std::ptr::null_mut(),
            bytes,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote.is_null() {
            let error = GetLastError();
            CloseHandle(process);
            return Err(InjectError::AllocateRemoteMemory(error));
        }
        let mut written = 0;
        if WriteProcessMemory(
            process,
            remote,
            path.as_ptr().cast::<c_void>(),
            bytes,
            &mut written,
        ) == 0
            || written != bytes
        {
            let error = GetLastError();
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return Err(InjectError::WriteRemoteMemory(error));
        }

        let kernel = wide("kernel32.dll");
        let module = GetModuleHandleW(kernel.as_ptr());
        let procedure = if module.is_null() {
            None
        } else {
            GetProcAddress(module, c"LoadLibraryW".as_ptr().cast())
        };
        let Some(procedure) = procedure else {
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return Err(InjectError::FindLoadLibrary);
        };
        let start: LPTHREAD_START_ROUTINE = Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(*mut c_void) -> u32,
        >(procedure));
        let thread = CreateRemoteThread(
            process,
            std::ptr::null(),
            0,
            start,
            remote,
            0,
            std::ptr::null_mut(),
        );
        if thread.is_null() {
            let error = GetLastError();
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return Err(InjectError::StartRemoteThread(error));
        }
        if WaitForSingleObject(thread, INFINITE) != WAIT_OBJECT_0 {
            let error = GetLastError();
            CloseHandle(thread);
            VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            CloseHandle(process);
            return Err(InjectError::WaitRemoteThread(error));
        }
        let mut module_handle = 0;
        let status = GetExitCodeThread(thread, &mut module_handle);
        CloseHandle(thread);
        VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        if status == 0 || module_handle == 0 {
            CloseHandle(process);
            return Err(InjectError::RemoteLoadFailed);
        }

        let remote_base = match remote_runtime_module_base(process_id, runtime_path) {
            Ok(base) => base,
            Err(error) => {
                CloseHandle(process);
                return Err(error);
            }
        };
        let local_runtime = LoadLibraryW(path.as_ptr());
        if local_runtime.is_null() {
            CloseHandle(process);
            return Err(InjectError::FindRuntimeStart);
        }
        let local_start = GetProcAddress(local_runtime, c"rivet_runtime_start".as_ptr().cast());
        let Some(local_start) = local_start else {
            FreeLibrary(local_runtime);
            CloseHandle(process);
            return Err(InjectError::FindRuntimeStart);
        };
        let offset = (local_start as usize).saturating_sub(local_runtime as usize);
        let remote_start = remote_base.saturating_add(offset);
        let start: LPTHREAD_START_ROUTINE = Some(std::mem::transmute::<
            usize,
            unsafe extern "system" fn(*mut c_void) -> u32,
        >(remote_start));
        let runtime_thread = CreateRemoteThread(
            process,
            std::ptr::null(),
            0,
            start,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
        );
        if runtime_thread.is_null() {
            let error = GetLastError();
            FreeLibrary(local_runtime);
            CloseHandle(process);
            return Err(InjectError::StartRuntime(error));
        }
        if WaitForSingleObject(runtime_thread, INFINITE) != WAIT_OBJECT_0 {
            let error = GetLastError();
            CloseHandle(runtime_thread);
            FreeLibrary(local_runtime);
            CloseHandle(process);
            return Err(InjectError::StartRuntime(error));
        }
        let mut runtime_status = 0;
        let runtime_finished = GetExitCodeThread(runtime_thread, &mut runtime_status);
        CloseHandle(runtime_thread);
        FreeLibrary(local_runtime);
        CloseHandle(process);
        if runtime_finished == 0 || runtime_status == 0 {
            return Err(InjectError::StartRuntime(GetLastError()));
        }
    }
    Ok(())
}

/// Reports that process injection is unavailable on non-Windows targets.
#[cfg(not(windows))]
pub fn inject_runtime(_process_id: u32, _runtime_path: &Path) -> Result<(), InjectError> {
    Err(InjectError::UnsupportedPlatform)
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr1_selects_the_new_window_even_when_its_process_already_owned_other_windows() {
        let existing = [BrowserWindow {
            handle: 10,
            process_id: 42,
        }];
        let current = [
            BrowserWindow {
                handle: 10,
                process_id: 42,
            },
            BrowserWindow {
                handle: 11,
                process_id: 42,
            },
        ];
        assert_eq!(
            first_unseen_window(&existing, &current),
            Some(BrowserWindow {
                handle: 11,
                process_id: 42,
            })
        );
    }

    #[test]
    fn fr2_rejects_a_missing_runtime_before_targeting_a_process() {
        let error =
            validate_runtime_path(Path::new("C:/does-not-exist/rivet-runtime.dll")).unwrap_err();
        assert_eq!(error, InjectError::MissingRuntimePath);
    }

    #[cfg(not(windows))]
    #[test]
    fn fr2_non_windows_never_attempts_injection() {
        let error = inject_runtime(1, Path::new("runtime.dll")).unwrap_err();
        assert_eq!(error, InjectError::UnsupportedPlatform);
    }
}
