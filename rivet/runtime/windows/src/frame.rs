//! Subclasses the browser window to show the appbar and route the appbar strip's input.

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
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetClassNameW, GetClientRect, GetPropW, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed, PostMessageW,
    RegisterWindowMessageW, RemovePropW, SetPropW, SetWindowLongPtrW, SetWindowPos, GWLP_WNDPROC,
    GWL_STYLE, HTBORDER, HTCAPTION, HTTOP, HTTOPLEFT, ICON_BIG, ICON_SMALL, SM_CXICON,
    SM_CXPADDEDBORDER, SM_CXSMICON, SM_CYSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, WM_DPICHANGED, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
    WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_NCRBUTTONDOWN,
    WM_NCRBUTTONUP, WM_SETICON, WM_WINDOWPOSCHANGED, WNDPROC, WS_THICKFRAME,
};

use super::appbar::Appbar;
use super::chrome_layout;
use super::taskbar;
use super::{layout_scale_dpi, scale, strip_hit, title_bar_top, StripHit, APPBAR_WIDTH};
use crate::tab_rows::{self, Row};

const BROWSER_CLASS: &str = "Chrome_WidgetWin_1";
const ORIGINAL_PROC: &str = "Rivet.OriginalWindowProc";
const ATTACH_MESSAGE: &str = "Rivet.AttachAppbar";
/// Hit-test code for the appbar's own content. Windows gives it no default behaviour, and the
/// browser never receives the non-client mouse messages that carry it.
const APPBAR_HIT: u32 = HTBORDER;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppbarHit {
    Pinned(usize),
    Folder([u8; 16]),
    NewTab,
    Tab(usize),
    CloseTab(usize),
    Space(usize),
}

thread_local! {
    /// Appbars of the browser windows owned by this UI thread.
    static APPBARS: RefCell<HashMap<isize, Appbar>> = RefCell::new(HashMap::new());
    static APPBAR_STATE: RefCell<HashMap<isize, (Option<AppbarHit>, Option<AppbarHit>, u32)>> = RefCell::new(HashMap::new());
}

fn appbar_hit(x: f32, y: f32, width: f32, height: f32, rows: &[Row]) -> Option<AppbarHit> {
    let left = 10.0;
    let inner = width - 18.0;
    if !(x >= left && x < left + inner) {
        return None;
    }
    if (52.0..94.0).contains(&y) {
        let tile = (inner - 18.0) / 4.0;
        let index = ((x - left) / (tile + 6.0)).floor() as usize;
        return (index < 4 && x < left + index as f32 * (tile + 6.0) + tile)
            .then_some(AppbarHit::Pinned(index));
    }
    let mut top = 138.0;
    for row in rows {
        if matches!(row, Row::NewTab) && top > 138.0 {
            top += 17.0;
        }
        if (top..top + 34.0).contains(&y) {
            return Some(match row {
                Row::Group(group) => AppbarHit::Folder(group.id),
                Row::NewTab => AppbarHit::NewTab,
                Row::Tab { index, .. } if x >= left + inner - 34.0 => AppbarHit::CloseTab(*index),
                Row::Tab { index, .. } => AppbarHit::Tab(*index),
            });
        }
        top += 34.0;
    }
    let foot = height - 38.0;
    let dots = 3.0 * 32.0 - 4.0;
    let start = left + 28.0 + (inner - 56.0 - dots) / 2.0;
    if (foot..foot + 28.0).contains(&y) {
        let index = ((x - start) / 32.0).floor() as usize;
        if index < 3 && x >= start + index as f32 * 32.0 && x < start + index as f32 * 32.0 + 28.0 {
            return Some(AppbarHit::Space(index));
        }
    }
    None
}

fn appbar_model(window: HWND) -> Vec<Row> {
    let tabs = unsafe { crate::chrome_tabs::tabs(window) }.unwrap_or_default();
    tab_rows::build_rows(&tabs)
}

unsafe fn appbar_point(window: HWND, lparam: LPARAM) -> Option<AppbarHit> {
    let point = client_point(window, lparam);
    let dpi = layout_dpi(window) as f32 / 96.0;
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    appbar_hit(
        point.x as f32 / dpi,
        (point.y - top_border(window)) as f32 / dpi,
        APPBAR_WIDTH as f32,
        (client.bottom - top_border(window)) as f32 / dpi,
        &appbar_model(window),
    )
}

unsafe fn appbar_action(window: HWND, hit: AppbarHit) {
    match hit {
        AppbarHit::Tab(index) => {
            crate::chrome_tabs::activate_tab(window, index);
        }
        AppbarHit::CloseTab(index) => {
            crate::chrome_tabs::close_tab(window, index);
        }
        AppbarHit::NewTab => {
            crate::chrome_tabs::new_tab(window);
        }
        AppbarHit::Pinned(index) => {
            const URLS: [&str; 4] = [
                "https://mail.google.com/",
                "https://calendar.google.com/",
                "https://docs.google.com/",
                "https://keep.google.com/",
            ];
            if crate::chrome_tabs::new_tab(window) {
                crate::chrome_navigation::open(window, URLS[index]);
            }
        }
        AppbarHit::Space(index) => {
            let tint = [0xd08a52, 0x5b8bab, 0xc0577f][index];
            APPBAR_STATE.with(|states| {
                let mut all = states.borrow_mut();
                let entry = all.entry(window as isize).or_insert((None, None, 0xd08a52));
                entry.2 = tint;
            });
        }
        AppbarHit::Folder(id) => {
            crate::chrome_tabs::toggle_group(window, &id);
        }
    }
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

static PANEL_EXPERIMENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The page colour of the shell study, so the native caption buttons sit on the top row.
fn nav_frame_color(window: HWND) -> Option<u32> {
    attached(window).then_some(0xFFFB_FAF7)
}

const NAV_CAPTION_WIDTH: i32 = 138;
const TAB_REFRESH_TIMER: usize = 0x5249_5645;

unsafe fn group_probe_message() -> u32 {
    RegisterWindowMessageW(wide("RivetGroupProbe").as_ptr())
}

unsafe fn vertical_tabs_message() -> u32 {
    RegisterWindowMessageW(wide("RivetVerticalTabs").as_ptr())
}

fn report_vertical_tabs(controller: usize) {
    let windows: Vec<isize> = APPBARS.with(|appbars| appbars.borrow().keys().copied().collect());
    for window in windows {
        unsafe { PostMessageW(window as HWND, vertical_tabs_message(), controller, 0) };
    }
}

fn attached(window: HWND) -> bool {
    unsafe { attached_window(window) }
}

unsafe fn attached_window(window: HWND) -> bool {
    APPBARS.with(|appbars| appbars.borrow().contains_key(&(window as isize)))
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

unsafe fn create_appbar(window: HWND) {
    if attached(window) {
        return;
    }
    // Without the layout hook the browser would draw underneath the appbar, so an unknown
    // browser build keeps working without Rivet instead.
    if let Err(error) = chrome_layout::install(reserved_width) {
        log(&format!("rivet: not attaching to this browser: {error}"));
        return;
    }
    // Experiment, off unless asked for: Chromium's vertical-tab frame for Rivet windows.
    if runtime_directory().is_some_and(|dir| dir.join("rivet-nav.log").exists()) {
        PANEL_EXPERIMENT.store(true, std::sync::atomic::Ordering::Relaxed);
        chrome_layout::PANEL.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(dir) = runtime_directory() {
            crate::vertical_tabs::set_trace(dir.join("rivet-nav.log"));
        }
        let installed = chrome_layout::install_frame_paint(nav_frame_color);
        crate::vertical_tabs::trace(&format!("frame colour: {installed:?}"));
        let caption = chrome_layout::install_caption_height(|window| {
            attached(window).then_some(crate::CAPTION_HEIGHT + chrome_layout::PANEL_TOP)
        });
        crate::vertical_tabs::trace(&format!("caption height: {caption:?}"));
        let top = chrome_layout::install_top_inset(|window| {
            attached(window).then_some(chrome_layout::PANEL_TOP)
        });
        crate::vertical_tabs::trace(&format!("top inset: {top:?}"));
        let layout = chrome_layout::install_caption_layout(|window| {
            attached(window).then_some(crate::CAPTION_HEIGHT + chrome_layout::PANEL_TOP)
        });
        crate::vertical_tabs::trace(&format!("caption layout: {layout:?}"));
    }
    let hide = runtime_directory().map(|dir| dir.join("rivet-hide-tab-strip.log"));
    if let Some(path) = hide.filter(|path| path.exists()) {
        crate::vertical_tabs::set_trace(path);
        if let Err(error) = crate::vertical_tabs::hide_tab_strip_everywhere() {
            crate::vertical_tabs::trace(&format!("hide failed: {error}"));
        }
    }
    let experiment = runtime_directory().map(|dir| dir.join("rivet-vertical-tabs.log"));
    let vertical_tabs = match experiment.filter(|path| path.exists()) {
        Some(path) => {
            crate::vertical_tabs::set_trace(path);
            let toolbar = crate::chrome_toolbar::install(runtime_directory());
            crate::vertical_tabs::trace(&format!("toolbar: {toolbar:?}"));
            crate::vertical_tabs::install(report_vertical_tabs)
                .map_err(|error| crate::vertical_tabs::trace(&format!("unavailable: {error}")))
                .is_ok()
        }
        None => false,
    };
    let appbar = match Appbar::new(window as isize) {
        Ok(appbar) => appbar,
        Err(error) => {
            // The browser keeps working without Rivet when composition is unavailable.
            log(&format!("rivet: could not compose the appbar: {error}"));
            return;
        }
    };
    APPBARS.with(|appbars| appbars.borrow_mut().insert(window as isize, appbar));
    APPBAR_STATE.with(|states| {
        states
            .borrow_mut()
            .entry(window as isize)
            .or_insert((None, None, 0xd08a52));
    });
    if vertical_tabs {
        crate::chrome_toolbar::adopt(window);
    }
    if runtime_directory().is_some_and(|dir| dir.join("rivet-group-probe.flag").exists()) {
        PostMessageW(window, group_probe_message(), 0, 0);
    }
    // Tab titles and selection can change without a window title change.
    windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(window, TAB_REFRESH_TIMER, 500, None);
    if vertical_tabs {
        for controller in crate::vertical_tabs::reported() {
            PostMessageW(window, vertical_tabs_message(), controller, 0);
        }
    }
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
    update_appbar(window);
}

/// Places the appbar visual over the strip the browser's layout left free and redraws it when its
/// size or DPI changed.
unsafe fn update_appbar(window: HWND) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let dpi = layout_dpi(window);
    let width = scale(APPBAR_WIDTH, dpi);
    let visible = framed(window);
    // A window's composition target is positioned at its client origin, which is also where the
    // browser's layout leaves the strip free. The window's own top border runs across the whole
    // window, so the appbar starts below it exactly like the browser's title bar.
    let top = top_border(window);
    APPBARS.with(|appbars| {
        if let Some(appbar) = appbars.borrow_mut().get_mut(&(window as isize)) {
            let (tabs, _, tint, hover, pressed) = APPBAR_STATE.with(|states| {
                let state = states
                    .borrow()
                    .get(&(window as isize))
                    .cloned()
                    .unwrap_or((None, None, 0xd08a52));
                (appbar_model(window), 0usize, state.2, state.0, state.1)
            });
            appbar.set_state(tabs, tint, hover, pressed);
            let height = client.bottom - top;
            if let Err(error) = appbar.update(visible, 0, top, width, height, dpi) {
                log(&format!("rivet: could not draw the appbar: {error}"));
            }
            if PANEL_EXPERIMENT.load(std::sync::atomic::Ordering::Relaxed) {
                if let Err(error) = appbar.update_panel(
                    visible,
                    top,
                    client.right,
                    client.bottom - top,
                    dpi,
                    NAV_CAPTION_WIDTH as f32,
                ) {
                    log(&format!("rivet: could not draw the panel: {error}"));
                }
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
    let border = if found == 0 { thickness as i32 } else { 0 };
    title_bar_top(border, layout_dpi(window))
}

thread_local! {
    /// The scale each browser window last laid itself out at, as a DPI.
    static LAYOUT_DPI: RefCell<HashMap<isize, u32>> = RefCell::new(HashMap::new());
}

/// The DPI the browser actually laid the window out at. While a window crosses monitors, the
/// browser and Windows can briefly disagree, and the appbar must match the browser's layout.
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
        if let Some(strip) = crate::vertical_tabs::strip_width(window) {
            return Some((crate::APPBAR_WIDTH - strip).max(0));
        }
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(window, &mut client);
        let layout_width = bounds.x + bounds.width;
        if layout_width > 0 && client.right > 0 {
            let dpi = layout_scale_dpi(client.right, layout_width);
            let changed = LAYOUT_DPI
                .with(|layouts| layouts.borrow_mut().insert(window as isize, dpi) != Some(dpi));
            if changed {
                update_appbar(window);
            }
        }
        Some(APPBAR_WIDTH)
    }
}

fn log(message: &str) {
    let text = wide(message);
    unsafe { OutputDebugStringW(text.as_ptr()) };
}

unsafe fn client_point(window: HWND, lparam: LPARAM) -> POINT {
    let mut point = POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    };
    ScreenToClient(window, &mut point);
    point
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
    // The browser lays out and paints parts of a window while handling its messages, so they run
    // in that window's look.
    if attached(window) {
        return crate::chrome_toolbar::for_window(window, || {
            handle(window, message, wparam, lparam)
        });
    }
    handle(window, message, wparam, lparam)
}

unsafe fn handle(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCMOUSEMOVE if wparam as u32 == APPBAR_HIT && attached(window) => {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: window,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tracking);
            let hit = appbar_point(window, lparam);
            APPBAR_STATE.with(|states| {
                let mut all = states.borrow_mut();
                let entry = all.entry(window as isize).or_insert((None, None, 0xd08a52));
                entry.0 = hit;
            });
            update_appbar(window);
            0
        }
        WM_NCMOUSELEAVE if attached(window) => {
            APPBAR_STATE.with(|states| {
                if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
                    entry.0 = None;
                }
            });
            update_appbar(window);
            0
        }
        WM_NCLBUTTONDOWN if wparam as u32 == APPBAR_HIT && attached(window) => {
            let hit = appbar_point(window, lparam);
            APPBAR_STATE.with(|states| {
                let mut all = states.borrow_mut();
                let entry = all.entry(window as isize).or_insert((None, None, 0xd08a52));
                entry.1 = hit;
                entry.0 = hit;
            });
            update_appbar(window);
            0
        }
        WM_NCLBUTTONUP if wparam as u32 == APPBAR_HIT && attached(window) => {
            let hit = appbar_point(window, lparam);
            let action = APPBAR_STATE.with(|states| {
                let mut states = states.borrow_mut();
                let entry = states
                    .entry(window as isize)
                    .or_insert((None, None, 0xd08a52));
                let action = (entry.1 == hit).then_some(hit).flatten();
                entry.1 = None;
                action
            });
            if let Some(hit) = action {
                appbar_action(window, hit);
            }
            update_appbar(window);
            0
        }
        WM_NCHITTEST if attached(window) => match hit_test(window, lparam) {
            StripHit::Browser => call_browser(window, message, wparam, lparam),
            // The caption band keeps native dragging, Snap Layouts, double-click maximize and
            // the system menu because Windows treats it as the window's own title bar.
            StripHit::Caption => HTCAPTION as LRESULT,
            StripHit::Top => HTTOP as LRESULT,
            StripHit::TopLeft => HTTOPLEFT as LRESULT,
            StripHit::Appbar => APPBAR_HIT as LRESULT,
        },
        // Clicks on the appbar's content never reach the browser, which would otherwise see
        // non-client input it did not lay out.
        WM_NCLBUTTONDBLCLK | WM_NCRBUTTONDOWN | WM_NCRBUTTONUP
            if wparam as u32 == APPBAR_HIT && attached(window) =>
        {
            0
        }
        WM_WINDOWPOSCHANGED if attached(window) => {
            let result = call_browser(window, message, wparam, lparam);
            update_appbar(window);
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
            APPBARS.with(|appbars| appbars.borrow_mut().remove(&(window as isize)));
            APPBAR_STATE.with(|states| states.borrow_mut().remove(&(window as isize)));
            crate::vertical_tabs::forget(window);
            crate::chrome_toolbar::forget(window);
            LAYOUT_DPI.with(|layouts| layouts.borrow_mut().remove(&(window as isize)));
            let key = wide(ORIGINAL_PROC);
            let original = GetPropW(window, key.as_ptr());
            SetWindowLongPtrW(window, GWLP_WNDPROC, original as isize);
            RemovePropW(window, key.as_ptr());
            let original: WNDPROC = std::mem::transmute(original);
            CallWindowProcW(original, window, message, wparam, lparam)
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER
            if wparam == TAB_REFRESH_TIMER && attached(window) =>
        {
            update_appbar(window);
            0
        }
        _ if message == group_probe_message() && attached(window) => {
            crate::chrome_tabs::probe_group(window);
            update_appbar(window);
            0
        }
        _ if message == vertical_tabs_message() => {
            crate::vertical_tabs::offer(window, wparam);
            0
        }
        _ if message == attach_message() => {
            create_appbar(window);
            0
        }
        _ => call_browser(window, message, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::{appbar_hit, AppbarHit};

    fn test_rows() -> Vec<crate::tab_rows::Row> {
        vec![
            crate::tab_rows::Row::NewTab,
            crate::tab_rows::Row::Tab {
                index: 0,
                title: String::new(),
                active: false,
                child: false,
            },
            crate::tab_rows::Row::Tab {
                index: 1,
                title: String::new(),
                active: false,
                child: false,
            },
        ]
    }
    #[test]
    fn fr3_group_and_child_targets_follow_the_visible_rows() {
        let group = crate::tab_rows::Group {
            id: [7; 16],
            title: "Work".into(),
            color: 3,
            collapsed: false,
        };
        let tabs = vec![crate::tab_rows::Tab {
            index: 4,
            title: "Child".into(),
            active: false,
            group: Some(group.clone()),
        }];
        let rows = crate::tab_rows::build_rows(&tabs);
        assert_eq!(
            appbar_hit(40.0, 150.0, 220.0, 700.0, &rows),
            Some(AppbarHit::Folder(group.id))
        );
        assert_eq!(
            appbar_hit(40.0, 180.0, 220.0, 700.0, &rows),
            Some(AppbarHit::Tab(4))
        );
        assert_eq!(
            appbar_hit(205.0, 180.0, 220.0, 700.0, &rows),
            Some(AppbarHit::CloseTab(4))
        );
        assert_eq!(
            appbar_hit(40.0, 230.0, 220.0, 700.0, &rows),
            Some(AppbarHit::NewTab)
        );
    }
    #[test]
    fn fr3_pinned_tiles_have_four_non_overlapping_targets() {
        let tile = (220.0 - 18.0 - 18.0) / 4.0;
        for index in 0..4 {
            let x = 10.0 + index as f32 * (tile + 6.0) + tile / 2.0;
            assert_eq!(
                appbar_hit(x, 70.0, 220.0, 700.0, &[]),
                Some(AppbarHit::Pinned(index))
            );
        }
    }

    #[test]
    fn fr3_tab_rows_split_activation_and_close_hit_areas() {
        assert_eq!(
            appbar_hit(40.0, 207.0, 220.0, 700.0, &test_rows()),
            Some(AppbarHit::Tab(1))
        );
        assert_eq!(
            appbar_hit(205.0, 207.0, 220.0, 700.0, &test_rows()),
            Some(AppbarHit::CloseTab(1))
        );
        assert_eq!(appbar_hit(40.0, 300.0, 220.0, 700.0, &test_rows()), None);
    }

    #[test]
    fn fr3_new_tab_and_space_targets_are_distinct() {
        assert_eq!(
            appbar_hit(40.0, 145.0, 220.0, 700.0, &test_rows()),
            Some(AppbarHit::NewTab)
        );
        assert_eq!(
            appbar_hit(90.0, 670.0, 220.0, 700.0, &test_rows()),
            Some(AppbarHit::Space(0))
        );
    }
}
