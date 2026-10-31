//! Windows primitives used to load the Rivet runtime into an explicitly selected process.
//!
//! The API does not discover processes and never chooses a browser. Its caller must supply the
//! browser process identifier and the absolute runtime DLL path after applying Rivet's session
//! ownership checks.

pub mod chrome_build;
pub mod signed_table;

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

/// The executable that owns a browser's windows. Some browsers register a small launcher beside
/// versioned folders, and the launcher starts the same-named executable of the newest version
/// folder. Windows belong to that process, so window discovery and the runtime need its path.
/// Browsers without such a stub, like Chrome and Edge, resolve to the path they were given.
pub fn resolve_browser_executable(path: &Path) -> std::path::PathBuf {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return path.to_path_buf();
    };
    let version = |folder: &std::ffi::OsStr| -> Option<Vec<u32>> {
        let parts: Vec<u32> = folder
            .to_str()?
            .split('.')
            .map(|part| part.parse().ok())
            .collect::<Option<_>>()?;
        (parts.len() == 4).then_some(parts)
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let candidate = entry.path().join(name);
            Some((version(&entry.file_name())?, candidate))
        })
        .filter(|(_, candidate)| candidate.is_file())
        .max_by(|left, right| left.0.cmp(&right.0))
        .map_or_else(|| path.to_path_buf(), |(_, candidate)| candidate)
}

/// The window class Chromium uses for browser top-level windows.
const BROWSER_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";
const WS_CAPTION_BITS: u32 = 0x00C0_0000;
const WS_THICKFRAME_BIT: u32 = 0x0004_0000;
const WS_EX_TOOLWINDOW_BIT: u32 = 0x0000_0080;

/// What Rivet observed about one visible window of the browser executable.
///
/// Chromium also creates notification, popup and helper windows in the same process. These
/// attributes separate a real browser window from those before a window may be attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWindow {
    pub handle: isize,
    pub process_id: u32,
    pub class_name: String,
    pub style: u32,
    pub extended_style: u32,
    pub has_owner: bool,
}

impl ObservedWindow {
    fn is_browser_window(&self) -> bool {
        self.class_name == BROWSER_WINDOW_CLASS
            && (self.style & WS_THICKFRAME_BIT != 0 || self.style & WS_CAPTION_BITS != 0)
            && !self.has_owner
            && self.extended_style & WS_EX_TOOLWINDOW_BIT == 0
    }

    fn window(&self) -> BrowserWindow {
        BrowserWindow {
            handle: self.handle,
            process_id: self.process_id,
        }
    }
}

/// Returns visible top-level browser windows owned by the exact browser executable.
#[cfg(windows)]
pub fn browser_windows(browser: &Path) -> Result<Vec<BrowserWindow>, WindowDiscoveryError> {
    Ok(observe_windows(browser)?
        .iter()
        .filter(|window| window.is_browser_window())
        .map(ObservedWindow::window)
        .collect())
}

#[cfg(windows)]
fn observe_windows(browser: &Path) -> Result<Vec<ObservedWindow>, WindowDiscoveryError> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowThreadProcessId,
        IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
    };

    let browser = browser
        .canonicalize()
        .map_err(|_| WindowDiscoveryError::BrowserPathMissing)?;
    struct Enumeration<'a> {
        browser: &'a Path,
        windows: Vec<ObservedWindow>,
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
        let Some(image) = process_image(process_id) else {
            return 1;
        };
        if !same_path(&image, enumeration.browser) {
            return 1;
        }
        let mut class = [0u16; 256];
        let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        enumeration.windows.push(ObservedWindow {
            handle: window as isize,
            process_id,
            class_name: String::from_utf16_lossy(&class[..length]),
            style: GetWindowLongW(window, GWL_STYLE) as u32,
            extended_style: GetWindowLongW(window, GWL_EXSTYLE) as u32,
            has_owner: !GetWindow(window, GW_OWNER).is_null(),
        });
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

#[cfg(windows)]
fn process_image(process_id: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return None;
        }
        let mut image = vec![0u16; 32_768];
        let mut length = image.len() as u32;
        let found = QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut length) != 0;
        CloseHandle(process);
        if !found {
            return None;
        }
        Path::new(&String::from_utf16_lossy(&image[..length as usize]))
            .canonicalize()
            .ok()
    }
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
        let windows = observe_windows(browser)?;
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

#[cfg_attr(not(windows), allow(dead_code))]
fn first_unseen_window(
    existing: &[BrowserWindow],
    current: &[ObservedWindow],
) -> Option<BrowserWindow> {
    current
        .iter()
        .filter(|candidate| candidate.is_browser_window())
        .find(|candidate| {
            !existing
                .iter()
                .any(|known| known.handle == candidate.handle)
        })
        .map(ObservedWindow::window)
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
    RuntimeRejectedWindow,
    BrowserDidNotRespond,
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
                write!(
                    f,
                    "could not locate the window-specific start entry in the runtime DLL"
                )
            }
            Self::StartRemoteThread(code) => write!(
                f,
                "could not start the browser runtime loader (Windows error {code})"
            ),
            Self::StartRuntime(code) => write!(
                f,
                "could not start the Rivet runtime in the browser process (Windows error {code})"
            ),
            Self::RuntimeRejectedWindow => {
                write!(f, "the Rivet runtime rejected the selected browser window")
            }
            Self::BrowserDidNotRespond => write!(
                f,
                "the browser process did not respond to the runtime loader in time"
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

/// How long the browser gets to run one remote loader or start thread.
#[cfg(windows)]
const REMOTE_THREAD_TIMEOUT_MS: u32 = 10_000;

#[cfg(windows)]
mod guards {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Memory::{VirtualFreeEx, MEM_RELEASE};

    /// Closes a process or thread handle when dropped.
    pub struct OwnedHandle(pub HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Frees memory allocated inside the browser process when dropped.
    pub struct RemoteAllocation {
        pub process: HANDLE,
        pub address: *mut std::ffi::c_void,
    }

    impl RemoteAllocation {
        /// Gives up ownership. Used when the browser may still be reading the memory.
        pub fn leak(self) {
            std::mem::forget(self);
        }
    }

    impl Drop for RemoteAllocation {
        fn drop(&mut self) {
            unsafe {
                VirtualFreeEx(self.process, self.address, 0, MEM_RELEASE);
            }
        }
    }
}

#[cfg(windows)]
enum ThreadWait {
    Finished,
    TimedOut,
}

#[cfg(windows)]
fn wait_for_remote_thread(thread: &guards::OwnedHandle) -> Result<ThreadWait, InjectError> {
    use windows_sys::Win32::Foundation::{GetLastError, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    match unsafe { WaitForSingleObject(thread.0, REMOTE_THREAD_TIMEOUT_MS) } {
        WAIT_OBJECT_0 => Ok(ThreadWait::Finished),
        WAIT_TIMEOUT => Ok(ThreadWait::TimedOut),
        _ => Err(InjectError::WaitRemoteThread(unsafe { GetLastError() })),
    }
}

#[cfg(windows)]
fn thread_exit_code(thread: &guards::OwnedHandle) -> Option<u32> {
    use windows_sys::Win32::System::Threading::GetExitCodeThread;

    let mut code = 0;
    (unsafe { GetExitCodeThread(thread.0, &mut code) } != 0).then_some(code)
}

/// Finds an export of the runtime module that is already loaded in the browser process.
///
/// The export directory is read from the browser's memory, so Rivet never loads the runtime into
/// its own process just to learn an offset.
#[cfg(windows)]
fn remote_export_address(
    process: &guards::OwnedHandle,
    module_base: usize,
    export: &str,
) -> Option<usize> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;

    let read = |offset: usize, buffer: &mut [u8]| {
        let mut copied = 0;
        let ok = unsafe {
            ReadProcessMemory(
                process.0,
                module_base.checked_add(offset)? as *const _,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut copied,
            )
        };
        Some(ok != 0 && copied == buffer.len())
    };
    let relative = find_export_rva(
        &mut |offset, buffer| read(offset, buffer).unwrap_or(false),
        export,
    )?;
    module_base.checked_add(relative as usize)
}

/// Resolves a named export of a PE image by reading it through `read`.
///
/// `read(offset, buffer)` fills `buffer` from `offset` bytes after the image base and reports
/// whether it succeeded. Forwarded exports are rejected because they do not point into the image.
fn find_export_rva(read: &mut dyn FnMut(usize, &mut [u8]) -> bool, export: &str) -> Option<u32> {
    fn u16_at(read: &mut dyn FnMut(usize, &mut [u8]) -> bool, offset: usize) -> Option<u16> {
        let mut bytes = [0u8; 2];
        read(offset, &mut bytes).then(|| u16::from_le_bytes(bytes))
    }
    fn u32_at(read: &mut dyn FnMut(usize, &mut [u8]) -> bool, offset: usize) -> Option<u32> {
        let mut bytes = [0u8; 4];
        read(offset, &mut bytes).then(|| u32::from_le_bytes(bytes))
    }

    if u16_at(read, 0)? != 0x5A4D {
        return None;
    }
    let nt = u32_at(read, 0x3C)? as usize;
    if u32_at(read, nt)? != 0x0000_4550 {
        return None;
    }
    let optional = nt.checked_add(24)?;
    let directories = match u16_at(read, optional)? {
        0x10B => optional.checked_add(96)?,
        0x20B => optional.checked_add(112)?,
        _ => return None,
    };
    let export_rva = u32_at(read, directories)?;
    let export_size = u32_at(read, directories + 4)?;
    if export_rva == 0 || export_size == 0 {
        return None;
    }
    let table = export_rva as usize;
    let name_count = u32_at(read, table + 24)? as usize;
    let functions = u32_at(read, table + 28)? as usize;
    let names = u32_at(read, table + 32)? as usize;
    let ordinals = u32_at(read, table + 36)? as usize;
    if name_count > 0xFFFF {
        return None;
    }
    let wanted = export.as_bytes();
    for index in 0..name_count {
        let name_rva = u32_at(read, names.checked_add(index * 4)?)? as usize;
        let mut name = vec![0u8; wanted.len() + 1];
        if !read(name_rva, &mut name) || &name[..wanted.len()] != wanted || name[wanted.len()] != 0
        {
            continue;
        }
        let ordinal = u16_at(read, ordinals.checked_add(index * 2)?)? as usize;
        let rva = u32_at(read, functions.checked_add(ordinal * 4)?)?;
        let inside_exports = rva >= export_rva && (rva - export_rva) < export_size;
        return (!inside_exports).then_some(rva);
    }
    None
}

/// Loads `runtime_path` into exactly `process_id` on Windows.
///
/// The caller is responsible for proving that the process belongs to the current Rivet session.
#[cfg(windows)]
pub fn inject_runtime(
    process_id: u32,
    window_handle: Option<isize>,
    runtime_path: &Path,
) -> Result<(), InjectError> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Memory::{
        VirtualAllocEx, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateRemoteThread, OpenProcess, LPTHREAD_START_ROUTINE, PROCESS_CREATE_THREAD,
        PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
    };

    validate_runtime_path(runtime_path)?;
    let mut path = runtime_path.as_os_str().encode_wide().collect::<Vec<_>>();
    path.push(0);
    let bytes = path.len() * std::mem::size_of::<u16>();

    // Guards release every handle and allocation on all paths, so a failed injection cannot leave
    // memory behind in the selected browser process.
    unsafe {
        let process = OpenProcess(
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
        let process = guards::OwnedHandle(process);
        let remote = VirtualAllocEx(
            process.0,
            std::ptr::null_mut(),
            bytes,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote.is_null() {
            return Err(InjectError::AllocateRemoteMemory(GetLastError()));
        }
        let remote = guards::RemoteAllocation {
            process: process.0,
            address: remote,
        };
        let mut written = 0;
        if WriteProcessMemory(
            process.0,
            remote.address,
            path.as_ptr().cast::<c_void>(),
            bytes,
            &mut written,
        ) == 0
            || written != bytes
        {
            return Err(InjectError::WriteRemoteMemory(GetLastError()));
        }

        let kernel = wide("kernel32.dll");
        let module = GetModuleHandleW(kernel.as_ptr());
        let procedure = if module.is_null() {
            None
        } else {
            GetProcAddress(module, c"LoadLibraryW".as_ptr().cast())
        };
        let Some(procedure) = procedure else {
            return Err(InjectError::FindLoadLibrary);
        };
        let start: LPTHREAD_START_ROUTINE = Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(*mut c_void) -> u32,
        >(procedure));
        let thread = CreateRemoteThread(
            process.0,
            std::ptr::null(),
            0,
            start,
            remote.address,
            0,
            std::ptr::null_mut(),
        );
        if thread.is_null() {
            return Err(InjectError::StartRemoteThread(GetLastError()));
        }
        let thread = guards::OwnedHandle(thread);
        if let ThreadWait::TimedOut = wait_for_remote_thread(&thread)? {
            // The loader may still be reading the path, so the memory must outlive this call.
            remote.leak();
            return Err(InjectError::BrowserDidNotRespond);
        }
        let loaded = thread_exit_code(&thread);
        drop(thread);
        drop(remote);
        if loaded.is_none_or(|module_handle| module_handle == 0) {
            return Err(InjectError::RemoteLoadFailed);
        }

        let remote_base = remote_runtime_module_base(process_id, runtime_path)?;
        let remote_start =
            remote_export_address(&process, remote_base, "rivet_runtime_start_for_window")
                .ok_or(InjectError::FindRuntimeStart)?;
        let start: LPTHREAD_START_ROUTINE = Some(std::mem::transmute::<
            usize,
            unsafe extern "system" fn(*mut c_void) -> u32,
        >(remote_start));
        let runtime_thread = CreateRemoteThread(
            process.0,
            std::ptr::null(),
            0,
            start,
            window_handle.unwrap_or(0) as *mut c_void,
            0,
            std::ptr::null_mut(),
        );
        if runtime_thread.is_null() {
            return Err(InjectError::StartRuntime(GetLastError()));
        }
        let runtime_thread = guards::OwnedHandle(runtime_thread);
        if let ThreadWait::TimedOut = wait_for_remote_thread(&runtime_thread)? {
            return Err(InjectError::BrowserDidNotRespond);
        }
        match thread_exit_code(&runtime_thread) {
            Some(0) => return Err(InjectError::RuntimeRejectedWindow),
            Some(_) => {}
            None => return Err(InjectError::StartRuntime(GetLastError())),
        }
    }
    Ok(())
}
/// Reports that process injection is unavailable on non-Windows targets.
#[cfg(not(windows))]
pub fn inject_runtime(
    _process_id: u32,
    _window_handle: Option<isize>,
    _runtime_path: &Path,
) -> Result<(), InjectError> {
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

    fn observed(handle: isize) -> ObservedWindow {
        ObservedWindow {
            handle,
            process_id: 42,
            class_name: BROWSER_WINDOW_CLASS.into(),
            style: WS_CAPTION_BITS | WS_THICKFRAME_BIT,
            extended_style: 0,
            has_owner: false,
        }
    }

    #[test]
    fn fr1_ignores_owned_and_tool_windows_when_selecting_the_launched_window() {
        let owned = ObservedWindow {
            has_owner: true,
            ..observed(11)
        };
        let tool = ObservedWindow {
            extended_style: WS_EX_TOOLWINDOW_BIT,
            ..observed(12)
        };
        let other_class = ObservedWindow {
            class_name: "Chrome_RenderWidgetHostHWND".into(),
            ..observed(13)
        };
        let frameless = ObservedWindow {
            style: 0,
            ..observed(14)
        };
        let real = observed(15);
        assert_eq!(
            first_unseen_window(&[], &[owned, tool, other_class, frameless, real]),
            Some(BrowserWindow {
                handle: 15,
                process_id: 42,
            })
        );
        assert_eq!(
            first_unseen_window(&[], &[observed(16)]).map(|w| w.handle),
            Some(16)
        );
    }

    #[test]
    fn fr1_selects_the_new_window_even_when_its_process_already_owned_other_windows() {
        let existing = [BrowserWindow {
            handle: 10,
            process_id: 42,
        }];
        let current = [observed(10), observed(11)];
        assert_eq!(
            first_unseen_window(&existing, &current),
            Some(BrowserWindow {
                handle: 11,
                process_id: 42,
            })
        );
    }

    fn synthetic_pe(forwarded: bool) -> Vec<u8> {
        let mut image = vec![0u8; 0x400];
        let put16 = |image: &mut Vec<u8>, at: usize, v: u16| {
            image[at..at + 2].copy_from_slice(&v.to_le_bytes())
        };
        let put32 = |image: &mut Vec<u8>, at: usize, v: u32| {
            image[at..at + 4].copy_from_slice(&v.to_le_bytes())
        };
        put16(&mut image, 0, 0x5A4D);
        put32(&mut image, 0x3C, 0x80);
        put32(&mut image, 0x80, 0x0000_4550);
        put16(&mut image, 0x80 + 24, 0x20B);
        // Export directory at RVA 0x200, size 0x100.
        put32(&mut image, 0x80 + 24 + 112, 0x200);
        put32(&mut image, 0x80 + 24 + 116, 0x100);
        put32(&mut image, 0x200 + 24, 2); // names
        put32(&mut image, 0x200 + 28, 0x240); // functions
        put32(&mut image, 0x200 + 32, 0x250); // name pointers
        put32(&mut image, 0x200 + 36, 0x260); // ordinals
        put32(&mut image, 0x240, 0x1111);
        put32(&mut image, 0x244, if forwarded { 0x210 } else { 0x1234 });
        put32(&mut image, 0x250, 0x270);
        put32(&mut image, 0x254, 0x290);
        put16(&mut image, 0x260, 0);
        put16(&mut image, 0x262, 1);
        image[0x270..0x270 + 6].copy_from_slice(b"other\0");
        let name = b"rivet_runtime_start_for_window\0";
        image[0x290..0x290 + name.len()].copy_from_slice(name);
        image
    }

    fn reader(image: &[u8]) -> impl FnMut(usize, &mut [u8]) -> bool + '_ {
        move |offset, buffer| match image.get(offset..offset + buffer.len()) {
            Some(bytes) => {
                buffer.copy_from_slice(bytes);
                true
            }
            None => false,
        }
    }

    #[test]
    fn fr2_resolves_the_window_start_export_from_a_remote_image() {
        let image = synthetic_pe(false);
        assert_eq!(
            find_export_rva(&mut reader(&image), "rivet_runtime_start_for_window"),
            Some(0x1234)
        );
        assert_eq!(find_export_rva(&mut reader(&image), "missing"), None);
        assert_eq!(find_export_rva(&mut reader(&image[..0x100]), "other"), None);
    }

    #[test]
    fn fr2_rejects_forwarded_exports_and_non_pe_images() {
        let image = synthetic_pe(true);
        assert_eq!(
            find_export_rva(&mut reader(&image), "rivet_runtime_start_for_window"),
            None
        );
        let garbage = vec![0u8; 0x400];
        assert_eq!(find_export_rva(&mut reader(&garbage), "other"), None);
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
        let error = inject_runtime(1, Some(1), Path::new("runtime.dll")).unwrap_err();
        assert_eq!(error, InjectError::UnsupportedPlatform);
    }

    #[test]
    fn nfr3_launcher_stub_resolves_to_the_newest_version_folder() {
        let root = std::env::temp_dir().join(format!("rivet-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for version in ["5.38.1.2", "5.39.412.57", "5.9.1.1"] {
            std::fs::create_dir_all(root.join(version)).unwrap();
            std::fs::write(root.join(version).join("whale.exe"), b"x").unwrap();
        }
        std::fs::create_dir_all(root.join("5.40.0.0")).unwrap();
        std::fs::write(root.join("whale.exe"), b"stub").unwrap();
        assert_eq!(
            resolve_browser_executable(&root.join("whale.exe")),
            root.join("5.39.412.57").join("whale.exe")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nfr3_samsung_internet_is_its_own_browser_process() {
        // The version folder holds the browser module and helper programs, but not a copy of the
        // registered executable, so the registered path is the process that owns the windows.
        let root = std::env::temp_dir().join(format!("rivet-samsung-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("143.0.4.29")).unwrap();
        for name in ["samsunginternet.dll", "notification_helper.exe", "chrome_pwa_launcher.exe"] {
            std::fs::write(root.join("143.0.4.29").join(name), b"x").unwrap();
        }
        std::fs::write(root.join("samsunginternet.exe"), b"x").unwrap();
        std::fs::write(root.join("samsunginternet_proxy.exe"), b"x").unwrap();
        assert_eq!(
            resolve_browser_executable(&root.join("samsunginternet.exe")),
            root.join("samsunginternet.exe")
        );
        let _ = std::fs::remove_dir_all(&root);
        // The installed build resolves the same way.
        let installed = Path::new(r"C:\Program Files\Samsung\Internet\Application\samsunginternet.exe");
        if installed.is_file() {
            assert_eq!(resolve_browser_executable(installed), installed);
        }
    }

    #[test]
    fn nfr3_browser_without_a_stub_keeps_its_path() {
        let root = std::env::temp_dir().join(format!("rivet-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("156.0.8078.48")).unwrap();
        std::fs::write(root.join("156.0.8078.48").join("chrome.dll"), b"x").unwrap();
        std::fs::write(root.join("chrome.exe"), b"x").unwrap();
        assert_eq!(resolve_browser_executable(&root.join("chrome.exe")), root.join("chrome.exe"));
        assert_eq!(
            resolve_browser_executable(&root.join("missing").join("a.exe")),
            root.join("missing").join("a.exe")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
