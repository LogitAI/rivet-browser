//! The window's control channel: the CLI's page, navigation, input, read, workspace and display
//! commands.
//!
//! A thread of the runtime serves a named pipe for the window. Each request is posted to the
//! window and runs on the browser UI thread between its own messages, never inside a call of the
//! browser, and every action uses the browser's own function: its tab strip model, `OpenURL`, its
//! navigation commands and its page addresses and titles. A hidden window is cloaked by the
//! Desktop Window Manager, so the browser keeps running its pages while nothing of it is shown.

use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rivet_core::control::{self, Failure, Key};
use rivet_protocol::{
    channel, ControlRequest, ControlResponse, Frame, NavStep, Page, ReadField, Visibility,
    Workspace,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};

/// How long the pipe thread waits for the UI thread to run a command.
const ANSWER_WAIT: Duration = Duration::from_secs(30);

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// The registered message that hands a command to the window's UI thread.
pub(crate) fn message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(
            wide("Rivet.Control").as_ptr(),
        )
    })
}

/// A command waiting for the UI thread, and its answer.
struct Job {
    request: ControlRequest,
    answer: Mutex<Option<ControlResponse>>,
    ready: Condvar,
}

/// Jobs posted and not yet taken, by address. A message whose `lparam` is not one of them, such
/// as one another program sent, is ignored and never read.
fn pending() -> &'static Mutex<Vec<usize>> {
    static PENDING: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

/// Windows the runtime cloaked, by handle.
fn hidden() -> &'static Mutex<HashSet<isize>> {
    static HIDDEN: OnceLock<Mutex<HashSet<isize>>> = OnceLock::new();
    HIDDEN.get_or_init(|| Mutex::new(HashSet::new()))
}

fn failure(reason: &str) -> ControlResponse {
    let mut out = rivet_protocol::ControlFailure {
        reason: reason.into(),
        ..Default::default()
    };
    out.set_kind(rivet_protocol::FailureKind::BrowserRefused);
    ControlResponse {
        result: Some(rivet_protocol::control_response::Result::Failure(out)),
        ..Default::default()
    }
}

/// Starts serving the control pipe of `window`, which the runtime just attached to. A browser that
/// Rivet started hidden has its first window cloaked here, before it is shown.
pub(crate) unsafe fn serve(window: HWND) {
    static FIRST: std::sync::Once = std::sync::Once::new();
    if std::env::var_os(rivet_protocol::window::HIDDEN_START_VARIABLE).is_some() {
        FIRST.call_once(|| {
            let _ = set_shown(window, false);
        });
    }
    let handle = window as isize;
    std::thread::spawn(move || serve_on(handle));
}

unsafe fn serve_on(handle: isize) {
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FlushFileBuffers, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

    let window = handle as HWND;
    let name = wide(&channel::pipe_name(GetCurrentProcessId(), handle));
    while IsWindow(window) != 0 {
        // The first instance only: if another program holds the name, this window serves nothing
        // rather than share it. The CLI also checks that the browser process serves the pipe.
        let pipe = CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            1 << 16,
            1 << 16,
            0,
            std::ptr::null(),
        );
        if pipe == INVALID_HANDLE_VALUE {
            crate::trace::trace(&format!("control: no pipe, error {}", GetLastError()));
            return;
        }
        let connected = ConnectNamedPipe(pipe, std::ptr::null_mut()) != 0
            || GetLastError() == ERROR_PIPE_CONNECTED;
        if !connected {
            CloseHandle(pipe);
            continue;
        }
        let mut stream = std::fs::File::from_raw_handle(pipe as _);
        match channel::accept(&mut stream) {
            Ok(request) => {
                let response = run_on_ui_thread(window, request);
                if let Err(error) = channel::reply(&mut stream, response) {
                    crate::trace::trace(&format!("control: reply failed: {error}"));
                }
            }
            Err(error) => crate::trace::trace(&format!("control: {error}")),
        }
        let pipe = stream.into_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
        FlushFileBuffers(pipe);
        DisconnectNamedPipe(pipe);
        CloseHandle(pipe);
    }
}

unsafe fn run_on_ui_thread(window: HWND, request: ControlRequest) -> ControlResponse {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;
    let job = Arc::new(Job {
        request,
        answer: Mutex::new(None),
        ready: Condvar::new(),
    });
    let raw = Arc::into_raw(job.clone()) as usize;
    pending().lock().unwrap().push(raw);
    if PostMessageW(window, message(), 0, raw as LPARAM) == 0 {
        pending().lock().unwrap().retain(|&item| item != raw);
        drop(Arc::from_raw(raw as *const Job));
        return failure("the browser window did not take the command");
    }
    let deadline = Instant::now() + ANSWER_WAIT;
    let mut answer = job.answer.lock().unwrap();
    while answer.is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return failure("the browser did not run the command in time");
        }
        answer = job.ready.wait_timeout(answer, left).unwrap().0;
    }
    answer.take().unwrap()
}

/// Runs a command the pipe thread posted. Called from the window procedure for [`message`].
pub(crate) unsafe fn take(window: HWND, lparam: LPARAM) {
    let raw = lparam as usize;
    {
        let mut waiting = pending().lock().unwrap();
        let Some(at) = waiting.iter().position(|&item| item == raw) else {
            return;
        };
        waiting.remove(at);
    }
    let job = Arc::from_raw(raw as *const Job);
    let response = control::handle(&mut WindowBrowser { window }, &job.request);
    *job.answer.lock().unwrap() = Some(response);
    job.ready.notify_all();
}

/// Cloaks or uncloaks `window`. A shown window outside every monitor comes onto the primary one,
/// unless a test asked to keep it off-screen. The window is never activated.
unsafe fn set_shown(window: HWND, shown: bool) -> Result<(), Failure> {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
    let cloak: i32 = (!shown) as i32;
    let result = DwmSetWindowAttribute(
        window,
        DWMWA_CLOAK as u32,
        &cloak as *const i32 as *const _,
        std::mem::size_of::<i32>() as u32,
    );
    if result < 0 {
        return Err(Failure::refused(format!(
            "the window manager did not {} the window ({result:#x})",
            if shown { "show" } else { "hide" }
        )));
    }
    if shown {
        hidden().lock().unwrap().remove(&(window as isize));
        if !rivet_platform_windows::focus::keep_offscreen() {
            bring_on_screen(window);
        }
    } else {
        hidden().lock().unwrap().insert(window as isize);
    }
    crate::trace::trace(&format!("control: window shown {shown}"));
    Ok(())
}

unsafe fn bring_on_screen(window: HWND) {
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SystemParametersInfoW, SPI_GETWORKAREA, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER,
    };
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(window, &mut rect) == 0
        || !MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_null()
    {
        return;
    }
    let mut work: RECT = std::mem::zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as *mut _, 0);
    SetWindowPos(
        window,
        std::ptr::null_mut(),
        work.left + 48,
        work.top + 48,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
    );
}

/// The window as [`control::Browser`] sees it.
struct WindowBrowser {
    window: HWND,
}

fn unavailable(what: &str) -> Failure {
    Failure::unavailable(format!(
        "this browser build does not record the functions for {what}"
    ))
}

impl WindowBrowser {
    /// The page `index`, or the active page.
    unsafe fn contents(&self, page: Option<u32>) -> Result<*mut std::ffi::c_void, Failure> {
        use crate::features::tabs;
        let contents = match page {
            None => crate::features::navigation::active_contents(self.window)
                .ok_or_else(|| unavailable("reading the active page"))?,
            Some(index) => {
                let base = tabs::base().ok_or_else(|| unavailable("the tab strip"))?;
                let model = tabs::tab_model(base, self.window)
                    .ok_or_else(|| unavailable("the tab strip"))?;
                if index as usize >= self.count()? {
                    return Err(Failure::not_found(format!("there is no page {index}")));
                }
                tabs::contents_at(base, model, index as i32)
            }
        };
        if contents.is_null() {
            return Err(Failure::not_found("the page is gone"));
        }
        Ok(contents)
    }

    unsafe fn count(&self) -> Result<usize, Failure> {
        crate::features::page_bar::pages(
            &crate::frame::BROWSER,
            crate::os::WindowId::from_hwnd(self.window),
        )
        .map(|pages| pages.len())
        .ok_or_else(|| unavailable("the tab strip"))
    }

    /// Makes page `index` the active one, for actions the browser runs on its active page.
    unsafe fn focus(&self, page: Option<u32>) -> Result<(), Failure> {
        match page {
            None => Ok(()),
            Some(index) => self.select_page_now(index),
        }
    }

    unsafe fn select_page_now(&self, index: u32) -> Result<(), Failure> {
        if index as usize >= self.count()? {
            return Err(Failure::not_found(format!("there is no page {index}")));
        }
        if crate::features::tabs::activate_tab(self.window, index as usize) {
            Ok(())
        } else {
            Err(Failure::refused(format!(
                "the browser did not select page {index}"
            )))
        }
    }

    unsafe fn post(&self, message: u32, wparam: usize, lparam: isize) -> Result<(), Failure> {
        if windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
            self.window,
            message,
            wparam,
            lparam,
        ) == 0
        {
            return Err(Failure::refused(
                "the browser window did not take the input",
            ));
        }
        Ok(())
    }
}

impl control::Browser for WindowBrowser {
    fn pages(&mut self) -> Result<Vec<Page>, Failure> {
        let pages = crate::features::page_bar::pages(
            &crate::frame::BROWSER,
            crate::os::WindowId::from_hwnd(self.window),
        )
        .ok_or_else(|| unavailable("the tab strip"))?;
        Ok(pages
            .into_iter()
            .map(|page| Page {
                index: page.index as u32,
                active: page.active,
                title: page.title,
                url: page.url,
            })
            .collect())
    }

    fn open_page(&mut self, url: &str) -> Result<(), Failure> {
        // The same path as a link the window's rules keep in the current space.
        let opened = unsafe {
            crate::features::tabs::new_tab(self.window)
                && crate::features::navigation::open(self.window, url)
        };
        opened
            .then_some(())
            .ok_or_else(|| Failure::refused("the browser did not open the page"))
    }

    fn close_page(&mut self, index: u32) -> Result<(), Failure> {
        unsafe {
            if index as usize >= self.count()? {
                return Err(Failure::not_found(format!("there is no page {index}")));
            }
            crate::features::tabs::close_tab(self.window, index as usize)
        }
        .then_some(())
        .ok_or_else(|| Failure::refused(format!("the browser did not close page {index}")))
    }

    fn select_page(&mut self, index: u32) -> Result<(), Failure> {
        unsafe { self.select_page_now(index) }
    }

    fn navigate(&mut self, page: Option<u32>, url: &str) -> Result<(), Failure> {
        let done = unsafe {
            match page {
                None => crate::features::navigation::open(self.window, url),
                Some(_) => crate::features::navigation::open_page(self.contents(page)?, url),
            }
        };
        done.then_some(())
            .ok_or_else(|| unavailable("opening an address"))
    }

    fn step(&mut self, page: Option<u32>, step: NavStep) -> Result<(), Failure> {
        use crate::features::page_bar::Command;
        let command = match step {
            NavStep::Back => Command::Back,
            NavStep::Forward => Command::Forward,
            _ => Command::Reload,
        };
        unsafe {
            self.focus(page)?;
            crate::features::page_bar::run(
                &crate::frame::OS,
                &crate::frame::BROWSER,
                crate::os::WindowId::from_hwnd(self.window),
                command,
            );
        }
        Ok(())
    }

    fn type_text(&mut self, page: Option<u32>, text: &str) -> Result<(), Failure> {
        use windows_sys::Win32::UI::WindowsAndMessaging::WM_CHAR;
        unsafe {
            self.focus(page)?;
            for unit in text.encode_utf16() {
                self.post(WM_CHAR, unit as usize, 1)?;
            }
        }
        Ok(())
    }

    fn click(&mut self, page: Option<u32>, x: i32, y: i32) -> Result<(), Failure> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        };
        const MK_LBUTTON: usize = 1;
        let point = ((y & 0xffff) << 16 | (x & 0xffff)) as isize;
        unsafe {
            self.focus(page)?;
            self.post(WM_MOUSEMOVE, 0, point)?;
            self.post(WM_LBUTTONDOWN, MK_LBUTTON, point)?;
            self.post(WM_LBUTTONUP, 0, point)?;
        }
        Ok(())
    }

    fn key(&mut self, page: Option<u32>, key: Key) -> Result<(), Failure> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC};
        use windows_sys::Win32::UI::WindowsAndMessaging::{WM_CHAR, WM_KEYDOWN, WM_KEYUP};
        unsafe {
            self.focus(page)?;
            let scan = MapVirtualKeyW(key.virtual_key as u32, MAPVK_VK_TO_VSC) as isize;
            let down = 1 | scan << 16;
            self.post(WM_KEYDOWN, key.virtual_key as usize, down)?;
            if let Some(character) = key.character {
                self.post(WM_CHAR, character as usize, down)?;
            }
            self.post(
                WM_KEYUP,
                key.virtual_key as usize,
                down | 0xC000_0000u32 as i32 as isize,
            )?;
        }
        Ok(())
    }

    fn read(&mut self, page: Option<u32>, field: ReadField) -> Result<String, Failure> {
        match field {
            ReadField::Url => Ok(unsafe { crate::features::tabs::page_address(self.contents(page)?) }),
            ReadField::Title => Ok(unsafe { crate::features::tabs::page_title(self.contents(page)?) }),
            _ => Err(Failure::unavailable(
                "reading page text needs a page script function that Rivet does not record for this browser build yet",
            )),
        }
    }

    fn frame(&mut self) -> Result<Frame, Failure> {
        unsafe { capture(self.window) }
    }

    fn workspaces(&mut self) -> Result<Vec<Workspace>, Failure> {
        let (spaces, current) = crate::frame::spaces_snapshot(self.window);
        Ok(spaces
            .into_iter()
            .enumerate()
            .map(|(index, space)| Workspace {
                name: space.name,
                current: index == current,
            })
            .collect())
    }

    fn switch_workspace(&mut self, name: &str) -> Result<(), Failure> {
        if !crate::frame::has_space_named(self.window, name) {
            return Err(Failure::not_found(format!("there is no space {name:?}")));
        }
        if unsafe { crate::frame::show_space_named(self.window, name) } {
            Ok(())
        } else {
            // A space of another browser opens in its own window, which this one cannot report.
            Err(Failure::refused(format!(
                "the space {name:?} lives in another browser and opened its own window"
            )))
        }
    }

    fn visibility(&self) -> Visibility {
        if hidden().lock().unwrap().contains(&(self.window as isize)) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        }
    }

    fn set_visibility(&mut self, visibility: Visibility) -> Result<(), Failure> {
        unsafe { set_shown(self.window, visibility != Visibility::Hidden) }
    }
}

/// The window as a person sees it, through the window manager's full-content print.
unsafe fn capture(window: HWND) -> Result<Frame, Failure> {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = 2;

    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(window, &mut rect);
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 || width > 16384 || height > 16384 {
        return Err(Failure::refused("the window has no size to capture"));
    }
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..std::mem::zeroed()
    };
    let screen = GetDC(std::ptr::null_mut());
    let memory = CreateCompatibleDC(screen);
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(
        memory,
        &info,
        DIB_RGB_COLORS,
        &mut bits,
        std::ptr::null_mut(),
        0,
    );
    let result = if bitmap.is_null() || bits.is_null() {
        Err(Failure::refused("no memory for the capture"))
    } else {
        let old = SelectObject(memory, bitmap);
        let printed = PrintWindow(window, memory, PW_RENDERFULLCONTENT) != 0;
        SelectObject(memory, old);
        if printed {
            let size = width as usize * height as usize * 4;
            let mut bgra = std::slice::from_raw_parts(bits as *const u8, size).to_vec();
            // The print leaves alpha undefined; every pixel of a window is opaque.
            for pixel in bgra.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            Ok(Frame {
                width: width as u32,
                height: height as u32,
                bgra,
            })
        } else {
            Err(Failure::refused(
                "the window manager did not print the window",
            ))
        }
    };
    if !bitmap.is_null() {
        DeleteObject(bitmap);
    }
    DeleteDC(memory);
    ReleaseDC(std::ptr::null_mut(), screen);
    result
}
