//! Subclasses the browser window to show the sidebar and route the sidebar strip's input.

use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_VISIBLE_FRAME_BORDER_THICKNESS,
};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetClassNameW, GetClientRect, GetPropW, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, PostMessageW,
    RegisterWindowMessageW, RemovePropW, SetPropW, SetWindowLongPtrW, SetWindowPos, GWLP_WNDPROC,
    GWL_STYLE, HTBORDER, HTCAPTION, HTTOP, HTTOPLEFT, ICON_BIG, ICON_SMALL, SM_CXICON,
    SM_CXPADDEDBORDER, SM_CXSMICON, SM_CYSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, WM_DPICHANGED, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
    WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCRBUTTONDOWN, WM_NCRBUTTONUP, WM_SETICON,
    WM_WINDOWPOSCHANGED, WNDPROC, WS_THICKFRAME,
};

use super::chrome_layout;
use super::sidebar::Sidebar;
use super::taskbar;
use super::{layout_scale_dpi, scale, strip_hit, StripHit, SIDEBAR_WIDTH};

const BROWSER_CLASS: &str = "Chrome_WidgetWin_1";
const ORIGINAL_PROC: &str = "Rivet.OriginalWindowProc";
const ATTACH_MESSAGE: &str = "Rivet.AttachSidebar";
/// Hit-test code for the sidebar's own content. Windows gives it no default behaviour, and the
/// browser never receives the non-client mouse messages that carry it.
const SIDEBAR_HIT: u32 = HTBORDER;

thread_local! {
    /// Sidebars of the browser windows owned by this UI thread.
    static SIDEBARS: RefCell<HashMap<isize, Sidebar>> = RefCell::new(HashMap::new());
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

unsafe fn class_name(window: HWND) -> String {
    let mut class = [0u16; 128];
    let length = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    String::from_utf16_lossy(&class[..length.max(0) as usize])
}

unsafe fn attach_message() -> u32 {
    RegisterWindowMessageW(wide(ATTACH_MESSAGE).as_ptr())
}

unsafe fn attached(window: HWND) -> bool {
    SIDEBARS.with(|sidebars| sidebars.borrow().contains_key(&(window as isize)))
}

unsafe fn framed(window: HWND) -> bool {
    // Chromium removes the sizing frame for fullscreen and restores it when leaving.
    GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_THICKFRAME != 0 && IsIconic(window) == 0
}

unsafe fn dpi(window: HWND) -> u32 {
    GetDpiForWindow(window).max(96)
}

unsafe fn runtime_directory() -> Option<std::path::PathBuf> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    let mut module = std::ptr::null_mut();
    GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        attach as *const () as *const u16,
        &mut module,
    );
    let mut path = [0u16; 32768];
    let length = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
    if length == 0 || length == path.len() {
        return None;
    }
    let path = std::path::PathBuf::from(String::from_utf16_lossy(&path[..length]));
    path.parent().map(std::path::Path::to_path_buf)
}

/// The Rivet icon for one of the window's icon slots at the window's DPI. Icons are created once
/// per slot and DPI and live as long as the browser process, because windows keep drawing them.
unsafe fn rivet_icon(window: HWND, slot: usize) -> Option<isize> {
    thread_local! {
        static ICONS: RefCell<HashMap<(usize, u32), isize>> = RefCell::new(HashMap::new());
    }
    let dpi = dpi(window);
    if let Some(icon) = ICONS.with(|icons| icons.borrow().get(&(slot, dpi)).copied()) {
        return Some(icon);
    }
    let metric = if slot == ICON_BIG as usize {
        SM_CXICON
    } else {
        SM_CXSMICON
    };
    let icon = taskbar::icon(GetSystemMetricsForDpi(metric, dpi))?.0 as isize;
    ICONS.with(|icons| icons.borrow_mut().insert((slot, dpi), icon));
    Some(icon)
}

/// Shows Rivet's icon in both of the window's icon slots.
unsafe fn set_rivet_icons(window: HWND) {
    for slot in [ICON_BIG as usize, ICON_SMALL as usize] {
        if let Some(icon) = rivet_icon(window, slot) {
            call_browser(window, WM_SETICON, slot, icon);
        }
    }
}

/// Subclasses the selected browser window. Runs on the launcher's injected thread.
pub unsafe fn attach(window: HWND) -> bool {
    let mut owner = 0;
    GetWindowThreadProcessId(window, &mut owner);
    if IsWindow(window) == 0
        || IsWindowVisible(window) == 0
        || owner != GetCurrentProcessId()
        || class_name(window) != BROWSER_CLASS
    {
        return false;
    }
    let key = wide(ORIGINAL_PROC);
    if !GetPropW(window, key.as_ptr()).is_null() {
        return true;
    }
    // Record the browser procedure before ours can receive a message. The browser UI thread may
    // dispatch between these calls, so the record is refreshed with the exact replaced value.
    SetPropW(
        window,
        key.as_ptr(),
        GetWindowLongPtrW(window, GWLP_WNDPROC) as _,
    );
    let replaced = SetWindowLongPtrW(window, GWLP_WNDPROC, browser_proc as *const () as isize);
    if replaced == 0 {
        RemovePropW(window, key.as_ptr());
        return false;
    }
    SetPropW(window, key.as_ptr(), replaced as _);
    // DirectComposition targets and frame changes belong on the UI thread that owns the window.
    PostMessageW(window, attach_message(), 0, 0) != 0
}

unsafe fn call_browser(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let original: WNDPROC = std::mem::transmute(GetPropW(window, wide(ORIGINAL_PROC).as_ptr()));
    CallWindowProcW(original, window, message, wparam, lparam)
}

unsafe fn create_sidebar(window: HWND) {
    if attached(window) {
        return;
    }
    // Without the layout hook the browser would draw underneath the sidebar, so an unknown
    // browser build keeps working without Rivet instead.
    if let Err(error) = chrome_layout::install(reserved_width) {
        log(&format!("rivet: not attaching to this browser: {error}"));
        return;
    }
    let sidebar = match Sidebar::new(window as isize) {
        Ok(sidebar) => sidebar,
        Err(error) => {
            // The browser keeps working without Rivet when composition is unavailable.
            log(&format!("rivet: could not compose the sidebar: {error}"));
            return;
        }
    };
    SIDEBARS.with(|sidebars| sidebars.borrow_mut().insert(window as isize, sidebar));
    if let Err(error) = taskbar::apply(window as isize, runtime_directory().as_deref()) {
        log(&format!(
            "rivet: could not set the taskbar identity: {error}"
        ));
    }
    set_rivet_icons(window);
    // A frame change makes the browser lay out its frame again, now through the layout hook.
    SetWindowPos(
        window,
        std::ptr::null_mut(),
        0,
        0,
        0,
        0,
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    update_sidebar(window);
}

/// Places the sidebar visual over the strip the browser's layout left free and redraws it when its
/// size or DPI changed.
unsafe fn update_sidebar(window: HWND) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let dpi = layout_dpi(window);
    let width = scale(SIDEBAR_WIDTH, dpi);
    let visible = framed(window);
    // A window's composition target is positioned at its client origin, which is also where the
    // browser's layout leaves the strip free. The window's own top border runs across the whole
    // window, so the sidebar starts below it exactly like the browser's title bar.
    let top = top_border(window);
    SIDEBARS.with(|sidebars| {
        if let Some(sidebar) = sidebars.borrow_mut().get_mut(&(window as isize)) {
            let height = client.bottom - top;
            if let Err(error) = sidebar.update(visible, 0, top, width, height, dpi) {
                log(&format!("rivet: could not draw the sidebar: {error}"));
            }
        }
    });
}

/// The thickness of the visible window border along the top edge. A maximized window has none.
unsafe fn top_border(window: HWND) -> i32 {
    if IsZoomed(window) != 0 {
        return 0;
    }
    let mut thickness: u32 = 0;
    let found = DwmGetWindowAttribute(
        window,
        DWMWA_VISIBLE_FRAME_BORDER_THICKNESS as u32,
        (&mut thickness as *mut u32).cast(),
        std::mem::size_of::<u32>() as u32,
    );
    if found == 0 {
        thickness as i32
    } else {
        0
    }
}

thread_local! {
    /// The scale each browser window last laid itself out at, as a DPI.
    static LAYOUT_DPI: RefCell<HashMap<isize, u32>> = RefCell::new(HashMap::new());
}

/// The DPI the browser actually laid the window out at. While a window crosses monitors, the
/// browser and Windows can briefly disagree, and the sidebar must match the browser's layout.
unsafe fn layout_dpi(window: HWND) -> u32 {
    LAYOUT_DPI
        .with(|layouts| layouts.borrow().get(&(window as isize)).copied())
        .unwrap_or_else(|| dpi(window))
}

/// The strip the browser's layout leaves free, in device-independent pixels. Also records the
/// scale of this layout, which is the browser's client width over its layout width.
fn reserved_width(window: HWND, bounds: chrome_layout::Rect) -> Option<i32> {
    unsafe {
        if !attached(window) || !framed(window) {
            return None;
        }
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(window, &mut client);
        let layout_width = bounds.x + bounds.width;
        if layout_width > 0 && client.right > 0 {
            let dpi = layout_scale_dpi(client.right, layout_width);
            let changed = LAYOUT_DPI
                .with(|layouts| layouts.borrow_mut().insert(window as isize, dpi) != Some(dpi));
            if changed {
                update_sidebar(window);
            }
        }
        Some(SIDEBAR_WIDTH)
    }
}

fn log(message: &str) {
    let text = wide(message);
    unsafe { OutputDebugStringW(text.as_ptr()) };
}

unsafe fn hit_test(window: HWND, lparam: LPARAM) -> StripHit {
    if !framed(window) {
        return StripHit::Browser;
    }
    let mut point = POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    };
    ScreenToClient(window, &mut point);
    let dpi = layout_dpi(window);
    let border = if IsZoomed(window) != 0 {
        0
    } else {
        GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    };
    strip_hit(point.x, point.y, dpi, border)
}

unsafe extern "system" fn browser_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCHITTEST if attached(window) => match hit_test(window, lparam) {
            StripHit::Browser => call_browser(window, message, wparam, lparam),
            // The caption band keeps native dragging, Snap Layouts, double-click maximize and
            // the system menu because Windows treats it as the window's own title bar.
            StripHit::Caption => HTCAPTION as LRESULT,
            StripHit::Top => HTTOP as LRESULT,
            StripHit::TopLeft => HTTOPLEFT as LRESULT,
            StripHit::Sidebar => SIDEBAR_HIT as LRESULT,
        },
        // Clicks on the sidebar's content never reach the browser, which would otherwise see
        // non-client input it did not lay out.
        WM_NCLBUTTONDOWN | WM_NCLBUTTONUP | WM_NCLBUTTONDBLCLK | WM_NCRBUTTONDOWN
        | WM_NCRBUTTONUP
            if wparam as u32 == SIDEBAR_HIT && attached(window) =>
        {
            0
        }
        WM_WINDOWPOSCHANGED if attached(window) => {
            let result = call_browser(window, message, wparam, lparam);
            update_sidebar(window);
            result
        }
        // The browser sets its own or the page's icon. A Rivet window keeps Rivet's.
        WM_SETICON if attached(window) => match rivet_icon(window, wparam) {
            Some(icon) => call_browser(window, message, wparam, icon),
            None => call_browser(window, message, wparam, lparam),
        },
        WM_DPICHANGED if attached(window) => {
            let result = call_browser(window, message, wparam, lparam);
            set_rivet_icons(window);
            result
        }
        WM_NCDESTROY => {
            if attached(window) {
                taskbar::clear(window as isize);
            }
            SIDEBARS.with(|sidebars| sidebars.borrow_mut().remove(&(window as isize)));
            LAYOUT_DPI.with(|layouts| layouts.borrow_mut().remove(&(window as isize)));
            let key = wide(ORIGINAL_PROC);
            let original = GetPropW(window, key.as_ptr());
            SetWindowLongPtrW(window, GWLP_WNDPROC, original as isize);
            RemovePropW(window, key.as_ptr());
            let original: WNDPROC = std::mem::transmute(original);
            CallWindowProcW(original, window, message, wparam, lparam)
        }
        _ if message == attach_message() => {
            create_sidebar(window);
            0
        }
        _ => call_browser(window, message, wparam, lparam),
    }
}
