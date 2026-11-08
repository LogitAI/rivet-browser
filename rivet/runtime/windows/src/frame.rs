//! Subclasses the browser window to show the appbar and route the appbar strip's input.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_VISIBLE_FRAME_BORDER_THICKNESS,
};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, SetFocus, TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CallWindowProcW, GetAncestor, GetClassNameW, GetClientRect, GetCursorPos,
    GetPropW, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    IsZoomed, KillTimer, LoadCursorW, PeekMessageW, PostMessageW, RegisterWindowMessageW,
    RemovePropW, SetCursor, SetPropW, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW,
    WindowFromPoint, GA_ROOT, GWLP_WNDPROC, GWL_STYLE, HTBORDER, HTCAPTION, HTCLOSE, HTMAXBUTTON,
    HTMINBUTTON, HTTOP, HTTOPLEFT, ICON_BIG, ICON_SMALL, IDC_SIZEWE, MSG, PM_REMOVE, SM_CXICON,
    SM_CXPADDEDBORDER, SM_CXSMICON, SM_CYSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, WH_GETMESSAGE, WM_CAPTURECHANGED, WM_CLOSE, WM_DPICHANGED,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST,
    WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
    WM_NCRBUTTONDOWN, WM_NCRBUTTONUP, WM_NULL, WM_SETCURSOR, WM_SETICON, WM_SYSCOMMAND,
    WM_WINDOWPOSCHANGED, WNDPROC, WS_THICKFRAME,
};

use super::appbar::Appbar;
use super::features::frame_layout;
use super::taskbar;
use super::{
    dpi_after_change, layout_scale_dpi, scale, strip_hit, title_bar_top, toggle_rect, AppbarLayout,
    StripHit, APPBAR_WIDTH, FLOAT_INSET, RESIZER_WIDTH,
};
use crate::feature::Step;
use crate::press::Presses;
use crate::spaces::{SavedTab, Spaces, MAX_SPACES};
use crate::tab_rows::{self, Row};

/// The composition root of the runtime's layers. A feature that has moved behind the operating
/// system and browser access traits is given these adapters, and nothing else picks one.
pub(crate) const OS: crate::os::win32::Win32 = crate::os::win32::Win32;
pub(crate) const BROWSER: crate::browser::chromium::Chromium = crate::browser::chromium::Chromium;

const BROWSER_CLASS: &str = "Chrome_WidgetWin_1";
const ORIGINAL_PROC: &str = "Rivet.OriginalWindowProc";
const ATTACH_MESSAGE: &str = "Rivet.AttachAppbar";
const TOGGLE_MESSAGE: &str = "Rivet.ToggleAppbar";
/// Hit-test code for the appbar's own content. Windows gives it no default behaviour, and the
/// browser never receives the non-client mouse messages that carry it.
const APPBAR_HIT: u32 = HTBORDER;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppbarHit {
    /// The Rivet mark, which opens the start page.
    Mark,
    Pinned(usize),
    Folder([u8; 16]),
    NewTab,
    Tab(usize),
    CloseTab(usize),
    /// A space dot in the footer.
    Space(usize),
    /// The chevron beside the mark, which opens the workspace menu.
    WorkspaceMenu,
    /// The name and badge of the current space, which open the workspace menu as well.
    SpaceHeader,
    /// A space in the workspace menu.
    MenuSpace(usize),
    /// The footer button that opens the Rivet settings menu.
    Settings,
    /// The footer button that adds a space.
    NewSpace,
    ProfileSettings,
    ManageProfiles,
    /// The settings entry that switches who keeps passwords and passkeys.
    Credentials,
    /// The settings entry that opens the Rivet settings card over the page.
    RivetSettings,
    /// The entry of a tab's menu that shows the tab beside the one after it.
    SplitTab(usize),
    /// The entry of a tab's menu that pins the tab, or unpins a pinned one.
    PinTab(usize),
    /// The entry of a tab's menu that puts the tab into a new folder.
    FolderTab(usize),
    /// The button in the toolbar row that hides and shows the appbar.
    Toggle,
    /// The seam on the appbar's right edge that drags its width.
    Resizer,
}

/// A menu drawn on the appbar's own surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The spaces, under the space header.
    Workspace,
    /// The Rivet settings, above the footer.
    Settings,
    /// The menu of one tab row, opened with the right button. `top` is where the row ends, in the
    /// appbar's device-independent pixels, which is where the menu begins.
    Tab { index: usize, top: i32 },
}

/// The entries of a tab's menu.
pub(crate) const TAB_ITEMS: [&str; 3] = [
    "Split with next tab",
    "Pin or unpin tab",
    "Move to new folder",
];

/// The action of a tab menu entry.
pub(crate) fn tab_item_hit(row: usize, index: usize) -> Option<AppbarHit> {
    match row {
        0 => Some(AppbarHit::SplitTab(index)),
        1 => Some(AppbarHit::PinTab(index)),
        2 => Some(AppbarHit::FolderTab(index)),
        _ => None,
    }
}

/// The entries of the settings menu. An entry without an action is shown dimmed.
pub(crate) const SETTINGS_ITEMS: [(&str, Option<AppbarHit>); 4] = [
    ("Profile settings", Some(AppbarHit::ProfileSettings)),
    ("Manage profiles", Some(AppbarHit::ManageProfiles)),
    ("Passwords and passkeys", Some(AppbarHit::Credentials)),
    ("Rivet settings", Some(AppbarHit::RivetSettings)),
];

/// The title of a settings entry as drawn. The provider entry names the current choice.
pub(crate) fn settings_title(
    title: &'static str,
    hit: Option<AppbarHit>,
    provider: rivet_core::CredentialProvider,
) -> String {
    if hit == Some(AppbarHit::Credentials) {
        format!("Passwords: {}", provider.label())
    } else {
        title.to_owned()
    }
}

/// The setup file this window's Rivet profile was started from.
fn setup_file() -> Option<std::path::PathBuf> {
    rivet_core::paths::Layout::current().map(|layout| layout.setup_file())
}

/// The saved provider, which the next start of a Rivet profile uses.
pub(crate) fn saved_credentials() -> rivet_core::CredentialProvider {
    setup_file().map_or_else(Default::default, |path| {
        rivet_core::credentials::load_from_setup(&path)
    })
}

const WORKSPACE_MENU_TOP: f32 = 138.0;
const WORKSPACE_ROW: f32 = 48.0;
const SETTINGS_ROW: f32 = 34.0;
const SETTINGS_PAD: f32 = 4.0;
const FOOT_HEIGHT: f32 = 38.0;

/// Finds the window's views by class metadata from the window's own user data, and traces them
/// beside the browser view and parent field this build's row records. The walk only reads memory,
/// so it runs on a worker thread and never holds up the window; the offsets it derives are kept by
/// build fingerprint, so its budget is paid once per build. Returns whether it started.
unsafe fn trace_reflection(window: HWND) -> bool {
    use crate::chromium::reflect;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
    let Some(image) = crate::chromium::image::recorded() else {
        crate::trace::trace("reflect: the browser module is not known");
        return false;
    };
    let headers = std::slice::from_raw_parts(image.base as *const u8, 0x1000.min(image.size));
    let ranges = reflect::ranges_of(image.base, &reflect::sections(headers));
    let Some(build) = reflect::fingerprint(headers) else {
        crate::trace::trace("reflect: the browser module has no fingerprint");
        return false;
    };
    let handler = GetWindowLongPtrW(window, GWLP_USERDATA) as usize;
    // Read on the window's own thread, where the browser layout recorded it.
    let host = crate::hooks::page_area::laid_out_host(window);
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let found = reflect::find_cached(
            &ranges,
            handler,
            std::time::Duration::from_millis(500),
            build,
        );
        let took = started.elapsed().as_millis();
        let parent_row = crate::chromium::view::View::PARENT;
        match found {
            Ok((found, cached)) => crate::trace::trace(&format!(
                "reflect: browser view by path {:x?}, view list at {:#x}, parent at {:#x} (row {:#x}), \
                 browser view {:x?} (row {:x?}, view part {} bytes), same {}, cached {cached}, \
                 {} objects, {took} ms on a worker thread",
                found.path,
                found.children,
                found.parent,
                parent_row,
                found.browser_view,
                host,
                found.view_part,
                found.parent == parent_row
                    && found.browser_view.is_some()
                    && found.browser_view == host,
                found.visited,
            )),
            Err(error) => crate::trace::trace(&format!("reflect: {error}, {took} ms")),
        }
        // Where the row knows the browser view, what memory shows around it, for designing the walk.
        if let Some(view) = host {
            let reached = reflect::reach(&ranges, handler, std::time::Duration::from_millis(500))
                .ok()
                .map(|(object, _, _)| object);
            crate::trace::trace(&reflect::facts(&ranges, view, parent_row, reached));
            let holder = crate::chromium::memory::readable(view + parent_row, 8)
                .then(|| unsafe { *((view + parent_row) as *const usize) });
            if let Some(holder) = holder {
                crate::trace::trace(&format!(
                    "facts: holder parts {}",
                    reflect::parts(&ranges, ranges.text.0, holder, 12)
                ));
            }
            crate::trace::trace(&format!(
                "facts: view fields naming a class {}",
                reflect::named_fields(&ranges, view, 16)
            ));
        }
    });
    true
}
/// How far above the page area's bottom the held build warning and its margin reach, in DIPs, or
/// zero when this process shows no such warning.
pub(crate) fn held_reserve() -> i32 {
    if held_warning().is_some() {
        crate::appbar::HELD_RESERVE
    } else {
        0
    }
}

thread_local! {
    /// The windows that took the attach and wait for the browser to lay them out for Rivet's frame.
    static AWAITING_LAYOUT: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
}

/// Marks, as the UI thread takes the attach, that the window waits for the browser to lay it out
/// for Rivet's frame. It stays cloaked until then, or until the longest hide passes: until that
/// layout a frame can still hold the browser's own title strip, at another scale.
fn await_layout(window: HWND) {
    AWAITING_LAYOUT.with(|all| all.borrow_mut().push(window as isize));
}

/// The browser laid the window out for Rivet's frame. The first such layout after the attach starts
/// the short reveal, by when Rivet's surfaces are drawn at that layout's scale; later ones do not.
unsafe fn laid_out(window: HWND) {
    if take_waiting(window) {
        GAVE_UP.with(|all| all.borrow_mut().remove(&(window as isize)));
        crate::trace::trace(&format!(
            "attach: complete, the browser laid window {:#x} out for Rivet's frame",
            window as usize
        ));
        crate::features::look::first_look(window, crate::features::look::FirstLook::LaidOut);
    }
}

/// Whether the window still waits for the browser's layout for Rivet's frame.
fn is_waiting(window: HWND) -> bool {
    AWAITING_LAYOUT.with(|all| all.borrow().contains(&(window as isize)))
}

/// The timer that asks the browser again to lay a window out for Rivet's frame.
const ATTACH_LAYOUT_TIMER: usize = 0x5249_414C;
/// How often, and how many times, the browser is asked again, all within the longest hide.
const ATTACH_LAYOUT_RETRY_MS: u32 = 300;
const ATTACH_LAYOUT_TRIES: u32 = 4;

thread_local! {
    /// How many times each window was asked again to lay out for Rivet's frame.
    static ATTACH_LAYOUT_ASKED: RefCell<HashMap<isize, u32>> = RefCell::new(HashMap::new());
}

/// What to do when the retry timer fires for a window that took the attach: nothing once the
/// browser has laid it out, ask again while tries are left, or give up with a warning.
#[derive(Debug, PartialEq, Eq)]
enum AttachRetry {
    Done,
    AskAgain(u32),
    GiveUp,
}

fn attach_retry(waiting: bool, asked: u32) -> AttachRetry {
    match (waiting, asked < ATTACH_LAYOUT_TRIES) {
        (false, _) => AttachRetry::Done,
        (true, true) => AttachRetry::AskAgain(asked + 1),
        (true, false) => AttachRetry::GiveUp,
    }
}

/// The retry timer of a window's attach fired.
unsafe fn on_attach_layout_timer(window: HWND) {
    let asked =
        ATTACH_LAYOUT_ASKED.with(|all| all.borrow().get(&(window as isize)).copied().unwrap_or(0));
    match attach_retry(is_waiting(window), asked) {
        AttachRetry::Done => {
            KillTimer(window, ATTACH_LAYOUT_TIMER);
        }
        AttachRetry::AskAgain(tries) => {
            ATTACH_LAYOUT_ASKED.with(|all| all.borrow_mut().insert(window as isize, tries));
            crate::trace::trace(&format!(
                "attach: the browser has not laid window {:#x} out for Rivet's frame yet; asked again (try {tries})",
                window as usize
            ));
            relayout(window);
            // Temporary probe: whether the window Rivet's subclass is on still exists.
            crate::trace::trace(&format!(
                "attach probe: after try {tries}, window {:#x} IsWindow {} subclassed {}",
                window as usize,
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(window) != 0,
                attached(window)
            ));
        }
        AttachRetry::GiveUp => {
            KillTimer(window, ATTACH_LAYOUT_TIMER);
            // The window's own events ask again from here on, with no more polling.
            GAVE_UP.with(|all| all.borrow_mut().insert(window as isize));
            crate::trace::trace(&format!(
                "attach: warning, the browser never laid window {:#x} out for Rivet's frame after {} tries",
                window as usize, ATTACH_LAYOUT_TRIES
            ));
        }
    }
}

/// Whether the window waited for its layout, which it then no longer does.
fn take_waiting(window: HWND) -> bool {
    AWAITING_LAYOUT.with(|all| {
        let mut all = all.borrow_mut();
        let at = all.iter().position(|key| *key == window as isize);
        at.map(|at| all.remove(at)).is_some()
    })
}

/// The warning of this process when it runs from a held browser build. The clock is read once per
/// process, and the time left is worked out anew for each paint because a window can stay open for days.
fn held_warning() -> Option<String> {
    use rivet_platform_windows::hold;
    use std::sync::OnceLock;
    static STATUS: OnceLock<Option<hold::Clock>> = OnceLock::new();
    let clock = STATUS
        .get_or_init(|| hold::held_status(&std::env::current_exe().ok()?))
        .as_ref()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    Some(hold::warning_text(
        &clock.installed,
        clock.first_detected,
        now,
    ))
}

/// The rectangle of a menu as (x, y, width, height) in the appbar's own coordinates, for an
/// appbar of `width` by `height` DIPs with `spaces` spaces.
pub(crate) fn menu_area(
    menu: Menu,
    width: f32,
    height: f32,
    spaces: usize,
) -> (f32, f32, f32, f32) {
    let (x, w) = (10.0, width - 18.0);
    match menu {
        Menu::Workspace => (x, WORKSPACE_MENU_TOP, w, spaces as f32 * WORKSPACE_ROW),
        Menu::Settings => {
            let h = SETTINGS_ITEMS.len() as f32 * SETTINGS_ROW + 2.0 * SETTINGS_PAD;
            (x, height - FOOT_HEIGHT - 6.0 - h, w, h)
        }
        Menu::Tab { top, .. } => {
            let h = TAB_ITEMS.len() as f32 * SETTINGS_ROW + 2.0 * SETTINGS_PAD;
            // Below the row, or above it when the menu would run past the appbar's bottom.
            let below = top as f32 + 4.0;
            let y = if below + h > height - FOOT_HEIGHT {
                top as f32 - 34.0 - 4.0 - h
            } else {
                below
            };
            (x, y, w, h)
        }
    }
}

/// Whether a point is inside the menu, and which entry it is on.
fn menu_hit(
    menu: Menu,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    spaces: usize,
) -> (bool, Option<AppbarHit>) {
    let (ax, ay, aw, ah) = menu_area(menu, width, height, spaces);
    if x < ax || x >= ax + aw || y < ay || y >= ay + ah {
        return (false, None);
    }
    let hit = match menu {
        Menu::Workspace => Some(AppbarHit::MenuSpace(((y - ay) / WORKSPACE_ROW) as usize)),
        Menu::Settings => {
            let row = ((y - ay - SETTINGS_PAD) / SETTINGS_ROW).floor();
            if row < 0.0 {
                None
            } else {
                SETTINGS_ITEMS.get(row as usize).and_then(|(_, hit)| *hit)
            }
        }
        Menu::Tab { index, .. } => {
            let row = ((y - ay - SETTINGS_PAD) / SETTINGS_ROW).floor();
            if row < 0.0 {
                None
            } else {
                tab_item_hit(row as usize, index)
            }
        }
    };
    (true, hit)
}

/// The appbar control a pointer is over, the one it went down on, and the tint of the space shown.
type AppbarState = (Option<AppbarHit>, Option<AppbarHit>, u32);

thread_local! {
    /// Appbars of the browser windows owned by this UI thread.
    static APPBARS: RefCell<HashMap<isize, Appbar>> = RefCell::new(HashMap::new());
    /// The open menu of each window, with the window that had the focus before it opened.
    static MENUS: RefCell<HashMap<isize, (isize, Menu)>> = RefCell::new(HashMap::new());
    /// The spaces of each window and the tabs they keep.
    static SPACES: RefCell<HashMap<isize, Spaces>> = RefCell::new(HashMap::new());
    /// The controls of each window that are being pressed or springing back.
    static PRESSES: RefCell<HashMap<isize, Presses<AppbarHit>>> = RefCell::new(HashMap::new());
    static APPBAR_STATE: RefCell<HashMap<isize, AppbarState>> = RefCell::new(HashMap::new());
    static KEYBOARD_HOOK: RefCell<isize> = const { RefCell::new(0) };
    /// The width and the hidden or peeking state of each window's appbar. It lives as long as the
    /// window does.
    static LAYOUTS: RefCell<HashMap<isize, AppbarLayout>> = RefCell::new(HashMap::new());
    /// How far the floating appbar has slid in, from 0 to 1, while it moves.
    static SLIDE: RefCell<HashMap<isize, f32>> = RefCell::new(HashMap::new());
    /// Windows whose appbar width is being dragged.
    static DRAGGING: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
}

/// `VK_OEM_5`, the backslash key on a US layout.
const VK_OEM_5: usize = 0xDC;
/// `WM_MOUSELEAVE`, which `windows-sys` does not export.
const WM_MOUSELEAVE: u32 = 0x02A3;
const PEEK_TIMER: usize = 0x5249_5650;
/// One frame of a control's press or release, at about 60 Hz.
const PRESS_TIMER: usize = 0x5249_5652;

/// Milliseconds on a clock that all press feedback of this process shares.
fn now_ms() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f32()
        * 1000.0
}
/// One frame of the slide, of about 220 ms from out to in.
const SLIDE_STEP: f32 = 16.0 / 220.0;

fn layout_of(window: HWND) -> AppbarLayout {
    LAYOUTS
        .with(|layouts| layouts.borrow().get(&(window as isize)).copied())
        .unwrap_or(AppbarLayout::new())
}

fn edit_layout<R>(window: HWND, edit: impl FnOnce(&mut AppbarLayout) -> R) -> R {
    LAYOUTS.with(|layouts| {
        edit(
            layouts
                .borrow_mut()
                .entry(window as isize)
                .or_insert(AppbarLayout::new()),
        )
    })
}

fn dragging(window: HWND) -> bool {
    DRAGGING.with(|set| set.borrow().contains(&(window as isize)))
}

/// A press on a tab row that may become a drag of the tab onto the page.
struct TabDrag {
    index: usize,
    title: String,
    /// Where the press began, in client pixels.
    press: POINT,
    /// The pointer has moved far enough that the press is a drag.
    live: bool,
}

thread_local! {
    static TAB_DRAGS: RefCell<HashMap<isize, TabDrag>> = RefCell::new(HashMap::new());
}

fn tab_dragging(window: HWND) -> bool {
    TAB_DRAGS.with(|drags| {
        drags
            .borrow()
            .get(&(window as isize))
            .is_some_and(|drag| drag.live)
    })
}

/// How far the pointer travels before a press on a tab becomes a drag, in device-independent
/// pixels.
const DRAG_THRESHOLD: f32 = 6.0;
/// The gap between the two halves of a page that a drop shows, as in the split itself.
const DROP_GAP: f32 = 8.0;

/// Where a dragged tab would land.
struct Drop {
    side: Option<crate::features::split::Side>,
    /// The slot of the split page the drop lands on, or none for the window's page.
    slot: Option<usize>,
    view: crate::appbar::DropView,
}

/// The drop under a point in client pixels, or none when the point is off the page.
unsafe fn drop_at(window: HWND, point: POINT) -> Option<Drop> {
    use crate::features::split::Side;
    let factor = layout_dpi(window) as f32 / 96.0;
    let (x, y) = (point.x as f32, point.y as f32);
    // A drop splits the page under the pointer, which is one page of a split or the whole area.
    let ([page_x, page_y, page_w, page_h], slot) = crate::features::split::drop_page(
        window,
        (x / factor).floor() as i32,
        (y / factor).floor() as i32,
    )?;
    let (left, top) = (page_x as f32 * factor, page_y as f32 * factor);
    let (width, height) = (page_w as f32 * factor, page_h as f32 * factor);
    if x < left || x > left + width || y < top || y > top + height {
        return None;
    }
    // A split that holds all the pages it can only offers to open here.
    let full = crate::features::split::split_full(window);
    let (side, [zone_x, zone_y, w, h]) = crate::features::split::drop_zone(
        [left, top, width, height],
        x,
        y,
        full,
        DROP_GAP * factor,
    );
    let label = match side {
        Some(Side::Left) => "Split left",
        Some(Side::Right) => "Split right",
        Some(Side::Top) => "Split top",
        Some(Side::Bottom) => "Split bottom",
        None => "Open here",
    };
    Some(Drop {
        side,
        slot,
        view: crate::appbar::DropView {
            x: zone_x,
            y: zone_y,
            width: w,
            height: h,
            label: label.to_owned(),
        },
    })
}

/// Highlights where something dragged to client pixel `point` would land, as a tab drag does, or
/// clears the highlight for none. The Mini Window's grip drag uses it.
pub(crate) unsafe fn show_drop_zone(window: HWND, point: Option<POINT>) {
    let dpi = layout_dpi(window);
    let zone = point
        .and_then(|point| drop_at(window, point))
        .map(|drop| drop.view);
    APPBARS.with(|appbars| {
        if let Some(appbar) = appbars.borrow_mut().get_mut(&(window as isize)) {
            if let Err(error) = appbar.update_drag(zone.as_ref(), None, dpi) {
                log(&format!("rivet: could not draw the drop target: {error}"));
            }
        }
    });
}

unsafe fn show_tab_drag(window: HWND, point: Option<POINT>) {
    let dpi = layout_dpi(window);
    let factor = dpi as f32 / 96.0;
    let title = TAB_DRAGS.with(|drags| {
        drags
            .borrow()
            .get(&(window as isize))
            .map(|d| d.title.clone())
    });
    let (zone, ghost) = match (point, title) {
        (Some(point), Some(title)) => (
            drop_at(window, point).map(|drop| drop.view),
            Some(crate::appbar::GhostView {
                x: point.x as f32 + 12.0 * factor,
                y: point.y as f32 + 8.0 * factor,
                title,
                colour: 0x7c2d4f,
                letter: "R".to_owned(),
            }),
        ),
        _ => (None, None),
    };
    APPBARS.with(|appbars| {
        if let Some(appbar) = appbars.borrow_mut().get_mut(&(window as isize)) {
            if let Err(error) = appbar.update_drag(zone.as_ref(), ghost.as_ref(), dpi) {
                log(&format!("rivet: could not draw the tab drag: {error}"));
            }
        }
    });
}

/// A left press on a tab row starts a press that may turn into a drag.
unsafe fn tab_drag_press(window: HWND, hit: Option<AppbarHit>, lparam: LPARAM) {
    let Some(AppbarHit::Tab(index)) = hit else {
        TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize)));
        crate::features::link_preview::cancel_hold(window);
        return;
    };
    // Held long enough, the press opens the tab's link preview instead.
    crate::features::link_preview::press(window, index);
    let title = crate::features::tabs::tabs(window)
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.index == index))
        .map(|tab| tab.title)
        .unwrap_or_default();
    TAB_DRAGS.with(|drags| {
        drags.borrow_mut().insert(
            window as isize,
            TabDrag {
                index,
                title,
                press: client_point(window, lparam),
                live: false,
            },
        )
    });
}

/// The pointer moves over the appbar with a tab pressed: past the threshold it takes the tab.
unsafe fn tab_drag_start(window: HWND, lparam: LPARAM) {
    let point = client_point(window, lparam);
    let threshold = DRAG_THRESHOLD * layout_dpi(window) as f32 / 96.0;
    let start = TAB_DRAGS.with(|drags| {
        let mut drags = drags.borrow_mut();
        let Some(drag) = drags.get_mut(&(window as isize)).filter(|drag| !drag.live) else {
            return false;
        };
        let moved = ((point.x - drag.press.x) as f32).hypot((point.y - drag.press.y) as f32);
        drag.live = moved >= threshold;
        drag.live
    });
    if start {
        // A press that became a drag is no long press.
        crate::features::link_preview::cancel_hold(window);
        rivet_platform_windows::focus::capture(window);
        // The press no longer clicks the tab.
        press_up(window);
        APPBAR_STATE.with(|states| {
            if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
                entry.1 = None;
            }
        });
        show_tab_drag(window, Some(point));
    }
}

/// Ends a press on the appbar without its click, for a press that did something else, such as
/// opening a link preview.
pub(crate) unsafe fn cancel_appbar_press(window: HWND) {
    TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize)));
    press_up(window);
    APPBAR_STATE.with(|states| {
        if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
            entry.1 = None;
        }
    });
}

/// The pointer moves, with the window capturing it, while a tab is dragged.
unsafe fn tab_drag_move(window: HWND, lparam: LPARAM) {
    let point = POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    };
    show_tab_drag(window, Some(point));
}

/// Ends a tab drag. A drop on the page splits it or opens the tab. `release` is the pointer of the
/// button release, and none when the drag was cancelled.
unsafe fn tab_drag_end(window: HWND, release: Option<LPARAM>) {
    let Some(drag) = TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize))) else {
        return;
    };
    rivet_platform_windows::focus::release(window);
    show_tab_drag(window, None);
    if let Some(lparam) = release.filter(|_| drag.live) {
        let point = POINT {
            x: (lparam & 0xffff) as i16 as i32,
            y: ((lparam >> 16) & 0xffff) as i16 as i32,
        };
        let (x, y) = local_dip(window, point.x, point.y);
        if x >= 0.0
            && x < layout_of(window).side as f32
            && crate::features::tab_list::drop_on_list(window, drag.index, y)
        {
            crate::trace::trace(&format!(
                "tab drag: moved tab {} in the tab list",
                drag.index
            ));
        } else if let Some(drop) = drop_at(window, point) {
            let done = match drop.side {
                Some(side) => {
                    crate::features::split::split_beside_page(window, drag.index, side, drop.slot)
                }
                None => crate::features::tabs::activate_tab(window, drag.index),
            };
            crate::trace::trace(&format!(
                "tab drag: dropped tab {} on {:?}: {done}",
                drag.index, drop.side
            ));
        }
    }
    update_appbar(window);
}

/// The button of a split page's top row the pointer is over, and the page's index.
type RowHit = (usize, crate::features::split::RowButton);

thread_local! {
    static ROW_HOVER: RefCell<HashMap<isize, RowHit>> = RefCell::new(HashMap::new());
    static ROW_PRESS: RefCell<HashMap<isize, RowHit>> = RefCell::new(HashMap::new());
}

/// The caption button a pointer is over, and the one it went down on.
type CaptionState = (
    Option<crate::features::address_bar::Caption>,
    Option<crate::features::address_bar::Caption>,
);

thread_local! {
    /// The caption button each window's pointer is over, and the one it went down on.
    static CAPTION_STATE: RefCell<HashMap<isize, CaptionState>> =
        RefCell::new(HashMap::new());
}

/// The caption button under the pointer of a window, for drawing its hover.
pub(crate) fn caption_hover(window: HWND) -> Option<crate::features::address_bar::Caption> {
    CAPTION_STATE.with(|all| {
        all.borrow()
            .get(&(window as isize))
            .and_then(|state| state.0)
    })
}

/// Records the caption button under the pointer. Returns whether it changed.
fn set_caption_hover(window: HWND, button: Option<crate::features::address_bar::Caption>) -> bool {
    CAPTION_STATE.with(|all| {
        let mut all = all.borrow_mut();
        let state = all.entry(window as isize).or_default();
        std::mem::replace(&mut state.0, button) != button
    })
}

/// The top row button under a point in client pixels. Every page's row is Rivet's apart from the
/// focused page's address, which is the browser's own address bar. The other pages' rows answer
/// anywhere.
unsafe fn row_hit(window: HWND, point: POINT) -> Option<RowHit> {
    use crate::features::split::{row_buttons, RowButton, ROW_HEIGHT};
    let factor = layout_dpi(window) as f32 / 96.0;
    let (x, y) = (point.x as f32 / factor, point.y as f32 / factor);
    for (index, pane) in crate::features::split::panes(window).iter().enumerate() {
        let top = pane.row_y as f32;
        if x < pane.x as f32
            || x >= (pane.x + pane.width) as f32
            || y < top
            || y >= top + ROW_HEIGHT as f32
        {
            continue;
        }
        let local = x - pane.x as f32;
        let button = row_buttons(pane.width as f32, pane.first, pane.caption)
            .into_iter()
            .find(|(_button, left, width)| local >= *left && local < left + width)
            .map(|(button, _, _)| button);
        return match (button, pane.active) {
            (Some(button), _) => Some((index, button)),
            (None, false) => Some((index, RowButton::Address)),
            (None, true) => None,
        };
    }
    None
}

/// The rows of the pages of a split in window pixels, with the tab each shows.
unsafe fn pane_rows(window: HWND) -> Vec<crate::appbar::PaneRow> {
    use crate::features::split::{RowButton, ROW_HEIGHT};
    let panes = crate::features::split::panes(window);
    if panes.is_empty() {
        return Vec::new();
    }
    let backend = crate::browsers::split_backend();
    // The second view's pages belong to Rivet, not to the browser's own split.
    let second_view = crate::features::second_view::is_split(window);
    let factor = layout_dpi(window) as f32 / 96.0;
    let tabs = crate::features::tabs::open_tabs(window).unwrap_or_default();
    let hover = ROW_HOVER.with(|all| all.borrow().get(&(window as isize)).copied());
    panes
        .iter()
        .enumerate()
        .map(|(index, pane)| {
            let saved = pane.tab.and_then(|tab| tabs.get(tab)).map(|tab| &tab.saved);
            // A page that is no tab of the strip says its own title and address.
            let own = if pane.tab.is_none() && !second_view {
                crate::browsers::split_backend().pane_text(window, pane)
            } else {
                None
            };
            let shows_active = saved.is_some_and(|tab| tab.active);
            let mut row = crate::appbar::PaneRow {
                x: pane.x as f32 * factor,
                y: pane.row_y as f32 * factor,
                width: pane.width as f32 * factor,
                height: ROW_HEIGHT as f32 * factor,
                reach_left: 0.0,
                reach_right: 0.0,
                caption: pane.caption * factor,
                active: pane.active,
                editable: crate::features::split::row_editable(
                    second_view,
                    pane.active,
                    shows_active,
                    pane.y == panes[0].y,
                    pane.first,
                    crate::features::split::address_follows_page(),
                ),
                first: pane.first,
                last: pane.last,
                title: saved
                    .map(|tab| tab.title.clone())
                    .or_else(|| own.as_ref().map(|text| text.0.clone()))
                    .unwrap_or_default(),
                url: saved
                    .map(|tab| tab.url.clone())
                    .or_else(|| own.as_ref().map(|text| text.1.clone()))
                    .map(|url| {
                        url.trim_start_matches("https://")
                            .trim_start_matches("http://")
                            .to_owned()
                    })
                    .unwrap_or_default(),
                hover: hover
                    .filter(|(over, _)| *over == index)
                    .map(|(_, button)| button),
                can_back: pane.tab.is_some()
                    || (!second_view && backend.pane_can(window, pane, RowButton::Back)),
                can_forward: pane.tab.is_some()
                    || (!second_view && backend.pane_can(window, pane, RowButton::Forward)),
                read_only: pane.tab.is_none(),
                caption_hover: if pane.caption > 0.0 {
                    caption_hover(window)
                } else {
                    None
                },
                zoomed: IsZoomed(window) != 0,
            };
            // The start page is shown as Rivet's new tab, as in the single page bar.
            if crate::features::address_bar::is_start_page(&row.url) {
                row.title = crate::address_display::START_PAGE_TITLE.to_owned();
                row.url = crate::address_display::START_PAGE_ADDRESS.to_owned();
            }
            row
        })
        .collect()
}

/// Follows the pointer over the rows, and repaints them when it moves to another button.
unsafe fn track_row_hover(window: HWND, point: Option<POINT>) {
    let hit = point.and_then(|point| row_hit(window, point));
    let changed = ROW_HOVER.with(|all| {
        let mut all = all.borrow_mut();
        let before = all.get(&(window as isize)).copied();
        match hit {
            Some(hit) => all.insert(window as isize, hit),
            None => all.remove(&(window as isize)),
        };
        before != hit
    });
    if changed {
        update_appbar(window);
    }
}

/// Back, forward or reload of the page the window shows, as the browser's own app commands.
unsafe fn navigate(window: HWND, button: crate::features::split::RowButton) {
    use crate::features::split::RowButton;
    const APPCOMMAND: u32 = 0x0319;
    let command: isize = match button {
        RowButton::Back => 1,
        RowButton::Forward => 2,
        _ => 3,
    };
    PostMessageW(window, APPCOMMAND, window as usize, command << 16);
}

/// A button of a page's row in a split of Rivet's second view is released. A press on a pane that
/// is not focused focuses it first, so the address field and the browser's commands follow it.
unsafe fn second_view_row_action(
    window: HWND,
    panes: &[crate::features::split::Pane],
    hit: RowHit,
) {
    use crate::features::second_view;
    use crate::features::split::RowButton;
    let Some(pane) = panes.get(hit.0) else { return };
    let slot = second_view::slot_of_pane(window, pane).unwrap_or(0);
    let focus = |pane: &crate::features::split::Pane| {
        second_view::set_focused(slot);
        if let Some(tab) = pane.tab {
            crate::features::tabs::activate_tab(window, tab);
            // Activating an extra view's tab moves its page; every page goes back where it was.
            second_view::hold(window);
        }
        second_view::focus_pane(window, slot);
    };
    let done = match hit.1 {
        RowButton::Grip => true,
        RowButton::Address => {
            focus(pane);
            true
        }
        RowButton::Back | RowButton::Forward | RowButton::Reload => {
            if pane.tab.is_some() {
                focus(pane);
                navigate(window, hit.1);
            }
            pane.tab.is_some()
        }
        // The other pages stay. The split ends when one page is left.
        RowButton::Popout => crate::features::split::pop_out(window, slot),
        RowButton::Unsplit => second_view::close_pane(window, slot),
    };
    crate::trace::trace(&format!("split row: {:?} of page {}: {done}", hit.1, hit.0));
}

/// A button of a page's top row is released.
unsafe fn row_action(window: HWND, hit: RowHit) {
    use crate::features::split::RowButton;
    let panes = crate::features::split::panes(window);
    let Some(pane) = panes.get(hit.0) else { return };
    if crate::features::second_view::is_split(window) {
        second_view_row_action(window, &panes, hit);
        update_appbar(window);
        return;
    }
    let Some(tab) = pane.tab else {
        // A page that is no tab has no tab to focus: the browser's backend knows what its buttons do.
        let done = crate::browsers::split_backend().pane_action(window, pane, hit.1);
        crate::trace::trace(&format!("split row: {:?} of page {}: {done}", hit.1, hit.0));
        update_appbar(window);
        return;
    };
    match hit.1 {
        RowButton::Grip => {}
        RowButton::Address => {
            crate::features::tabs::activate_tab(window, tab);
        }
        // The browser acts on the page it shows, so another page is focused first.
        RowButton::Back | RowButton::Forward | RowButton::Reload => {
            if !pane.active {
                crate::features::tabs::activate_tab(window, tab);
            }
            navigate(window, hit.1);
        }
        RowButton::Popout => {
            let done = crate::features::mini_window::pop_out_pane(window, hit.0);
            crate::trace::trace(&format!("split row: {:?} of page {}: {done}", hit.1, hit.0));
        }
        RowButton::Unsplit => {
            crate::features::tabs::activate_tab(window, tab);
            let done = crate::features::split::unsplit_active(window);
            crate::trace::trace(&format!("split row: {:?} of page {}: {done}", hit.1, hit.0));
        }
    }
    update_appbar(window);
}

fn slide_of(window: HWND) -> f32 {
    SLIDE
        .with(|slides| slides.borrow().get(&(window as isize)).copied())
        .unwrap_or(0.0)
}

/// Where the page panel begins, in device-independent pixels. Chromium's own vertical tab strip
/// keeps its width at the left even while the appbar is hidden, because the browser offers no way
/// to take it out of the layout, so the page cannot begin before it ends.
fn page_x(window: HWND) -> i32 {
    let leading = layout_of(window).leading();
    crate::features::vertical_tabs::layout_width(window).map_or(leading, |strip| leading.max(strip))
}

/// Converts client pixels to device-independent pixels of the appbar's own coordinates: below the
/// window's top border, and inside the floating rectangle while the hidden appbar peeks.
unsafe fn local_dip(window: HWND, x: i32, y: i32) -> (f32, f32) {
    let factor = layout_dpi(window) as f32 / 96.0;
    let inset = if layout_of(window).collapsed {
        FLOAT_INSET as f32
    } else {
        0.0
    };
    (
        x as f32 / factor - inset,
        (y - top_border(window)) as f32 / factor - inset,
    )
}

/// The height of the appbar below the top border, in device-independent pixels.
unsafe fn appbar_height(window: HWND) -> f32 {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let factor = layout_dpi(window) as f32 / 96.0;
    let height = (client.bottom - top_border(window)) as f32 / factor;
    if layout_of(window).collapsed {
        height - 2.0 * FLOAT_INSET as f32
    } else {
        height
    }
}

/// The tint of the space a window shows now.
pub(crate) fn space_tint(window: HWND) -> u32 {
    with_spaces(window, |spaces| spaces.list()[spaces.current()].tint)
}

fn with_spaces<R>(window: HWND, edit: impl FnOnce(&mut Spaces) -> R) -> R {
    SPACES.with(|all| {
        edit(all.borrow_mut().entry(window as isize).or_insert_with(|| {
            let mut spaces = Spaces::new();
            spaces.adopt_browser(crate::installed_browsers::current());
            spaces
        }))
    })
}

/// The appbar width in DIPs and whether it is hidden. The spaces file keeps both.
pub(crate) fn appbar_settings(window: HWND) -> (i32, bool) {
    let layout = layout_of(window);
    (layout.side, layout.collapsed)
}

/// Sets the appbar width as a drag of its edge would.
pub(crate) unsafe fn set_appbar_width(window: HWND, width: i32) {
    if edit_layout(window, |layout| layout.set_width(width)) {
        sync_strip(window);
        update_appbar(window);
        relayout(window);
    }
}

/// Hides or shows the appbar as its toggle would.
pub(crate) unsafe fn set_appbar_hidden(window: HWND, hidden: bool) {
    if layout_of(window).collapsed != hidden {
        toggle_appbar(window);
    }
}

/// The spaces of a window and the index of the one it shows.
pub(crate) fn spaces_snapshot(window: HWND) -> (Vec<crate::spaces::SpaceInfo>, usize) {
    with_spaces(window, |spaces| (spaces.list().to_vec(), spaces.current()))
}

/// Changes one space. The window's frame follows a new colour of the space it shows, and the
/// spaces file is written on the window's timer.
pub(crate) unsafe fn edit_space(
    window: HWND,
    index: usize,
    edit: impl FnOnce(&mut crate::spaces::SpaceInfo),
) -> bool {
    let (done, tint) = with_spaces(window, |spaces| {
        let done = spaces.edit(index, edit);
        (done, spaces.list()[spaces.current()].tint)
    });
    APPBAR_STATE.with(|states| {
        if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
            entry.2 = tint;
        }
    });
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(window, std::ptr::null(), 0);
    update_appbar(window);
    done
}

/// The name of the space a window shows now.
pub(crate) fn space_name(window: HWND) -> String {
    with_spaces(window, |spaces| {
        spaces.list()[spaces.current()].name.clone()
    })
}

/// The address of the Rivet start page for the space called `space`.
unsafe fn start_url(space: &crate::spaces::SpaceInfo) -> Option<String> {
    let browser = crate::installed_browsers::sole_label().unwrap_or(space.browser.label());
    runtime_directory().map(|directory| {
        rivet_core::start_page_url(&directory, &space.name, browser, &space.profile, space.tint)
    })
}

/// The address of the Rivet start page for the space `window` shows.
pub(crate) fn start_page_of(window: crate::os::WindowId) -> Option<String> {
    // The spaces of a window are Rivet's own state, and the runtime's folder is read from this
    // module's own address.
    unsafe { start_url_of(window.hwnd()) }
}

/// The address of the Rivet start page for the space `window` shows.
pub(crate) unsafe fn start_url_of(window: HWND) -> Option<String> {
    let space = with_spaces(window, |spaces| spaces.list()[spaces.current()].clone());
    start_url(&space)
}

unsafe fn start_page(window: HWND) {
    if crate::features::tabs::new_start_tab(window) {
        return;
    }
    let space = with_spaces(window, |spaces| spaces.list()[spaces.current()].clone());
    if let Some(url) = start_url(&space) {
        if crate::features::tabs::new_tab(window) {
            crate::features::navigation::open(window, &url);
        }
    }
}

/// Shows the tabs of another space. The current space keeps its tabs, the window opens the ones
/// the target kept, or a start page when it kept none, and only then closes the old ones, so the
/// window never has zero tabs.
unsafe fn switch_space(window: HWND, index: usize) {
    let Some(open) = crate::features::tabs::open_tabs(window) else {
        return;
    };
    let Some(name) = with_spaces(window, |spaces| {
        spaces
            .list()
            .get(index)
            .filter(|_| index != spaces.current())
            .cloned()
    }) else {
        return;
    };
    let Some(start) = start_url(&name) else {
        return;
    };
    let target = with_spaces(window, |spaces| spaces.list()[index].clone());
    let root = rivet_core::paths::data_home();
    if let Some(hand_over) = root.as_deref().and_then(|root| {
        crate::spaces::hand_over(&target, crate::installed_browsers::current(), root)
    }) {
        let saved: Vec<SavedTab> = open.into_iter().map(|tab| tab.saved).collect();
        hand_over_to(window, index, saved, hand_over, start);
        return;
    }
    let old: Vec<usize> = open.iter().map(|tab| tab.id).collect();
    let saved: Vec<SavedTab> = open.into_iter().map(|tab| tab.saved).collect();
    crate::features::tab_list::leave_space(window);
    let Some(restore) = with_spaces(window, |spaces| spaces.switch_to(index, saved, &start)) else {
        return;
    };
    crate::trace::trace(&format!(
        "space: to {}, restoring {:?}",
        name.name,
        restore
            .iter()
            .map(|tab| tab.url.as_str())
            .collect::<Vec<_>>()
    ));
    let tint = with_spaces(window, |spaces| spaces.list()[spaces.current()].tint);
    APPBAR_STATE.with(|states| {
        if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
            entry.2 = tint;
        }
    });
    show_tabs(window, old, &restore);
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(window, std::ptr::null(), 0);
}

/// The names of the window's spaces, in order, for the link rules.
pub(crate) fn space_names(window: HWND) -> Vec<String> {
    with_spaces(window, |spaces| {
        spaces
            .list()
            .iter()
            .map(|space| space.name.clone())
            .collect()
    })
}

/// The name of the space the window shows.
pub(crate) fn current_space_name(window: HWND) -> Option<String> {
    with_spaces(window, |spaces| {
        spaces
            .list()
            .get(spaces.current())
            .map(|space| space.name.clone())
    })
}

fn space_index(window: HWND, name: &str) -> Option<usize> {
    with_spaces(window, |spaces| {
        spaces
            .list()
            .iter()
            .position(|space| space.name.eq_ignore_ascii_case(name))
    })
}

/// Whether the window has a space of this name.
pub(crate) fn has_space_named(window: HWND, name: &str) -> bool {
    space_index(window, name).is_some()
}

/// Makes the space of this name the one the window shows, for a link a rule sends there. False
/// when there is no such space, or when it lives in another browser and so opened its own window.
pub(crate) unsafe fn show_space_named(window: HWND, name: &str) -> bool {
    let Some(index) = space_index(window, name) else {
        return false;
    };
    if with_spaces(window, |spaces| spaces.current()) != index {
        switch_space(window, index);
    }
    with_spaces(window, |spaces| spaces.current()) == index
}

/// Shows a space that lives in another browser. A window cannot move between browser processes,
/// so the space has its own window. This window stays open on its own space and keeps its tabs.
/// If a window already runs the space's profile it is brought forward, and it keeps the space it
/// shows. Otherwise the other browser's profile gets the spaces with `index` current and the
/// launcher starts it with the Rivet runtime.
unsafe fn hand_over_to(
    window: HWND,
    index: usize,
    saved: Vec<SavedTab>,
    hand_over: crate::spaces::HandOver,
    start: String,
) {
    save_spaces(window, saved.clone());
    let running = crate::installed_browsers::raise_existing(&hand_over.user_data_dir);
    if crate::spaces::show_for(running) == crate::spaces::Show::BringForward {
        crate::trace::trace(&format!(
            "space: bringing forward the {} window",
            hand_over.browser.label()
        ));
        return;
    }
    let Some(directory) = runtime_directory() else {
        return;
    };
    let layout = layout_of(window);
    let dir = hand_over.user_data_dir.to_string_lossy().into_owned();
    let (mut next, root) = (
        with_spaces(window, |spaces| spaces.clone()),
        rivet_core::paths::data_home(),
    );
    let Some(root) = root else {
        return;
    };
    let Some(restore) = next.switch_to(index, saved, &start) else {
        return;
    };
    let text = next.to_json(&restore, layout.side, layout.collapsed, &dir);
    let path = crate::spaces::store_path(&root, Some(&dir));
    if let Err(error) = crate::spaces::write_atomic(&path, &text) {
        log(&format!(
            "rivet: could not save the spaces for the other browser: {error}"
        ));
        return;
    }
    crate::trace::trace(&format!("space: starting {}", hand_over.browser.label()));
    crate::installed_browsers::start(hand_over, start, directory);
}

/// Opens `restore` as the window's tabs and closes the tabs `old`, which are the ones shown now,
/// only afterwards so the window never has zero tabs.
unsafe fn show_tabs(window: HWND, old: Vec<usize>, restore: &[SavedTab]) {
    let start = start_url_of(window).unwrap_or_default();
    for tab in restore {
        if crate::features::tabs::new_tab(window) && !tab.url.is_empty() {
            crate::features::navigation::open(
                window,
                crate::spaces::restored_url(&tab.url, &start),
            );
        }
    }
    let shown: Vec<usize> = crate::features::tabs::open_tabs(window)
        .unwrap_or_default()
        .into_iter()
        .map(|tab| tab.id)
        .filter(|id| !old.contains(id))
        .collect();
    for id in old {
        crate::features::tabs::close_tab_by_id(window, id);
    }
    crate::trace::trace(&format!(
        "space: now showing {:?}",
        crate::features::tabs::open_tabs(window)
            .unwrap_or_default()
            .iter()
            .map(|tab| (tab.saved.url.clone(), tab.saved.active))
            .collect::<Vec<_>>()
    ));
    // Tabs that shared a group share a new one, after the old tabs left the strip.
    let mut groups: Vec<(String, [u8; 16])> = Vec::new();
    for tab in restore {
        let Some(group) = &tab.group else { continue };
        if groups.iter().any(|(key, _)| *key == group.key) {
            continue;
        }
        let members: Vec<usize> = restore
            .iter()
            .enumerate()
            .filter(|(_, other)| other.group.as_ref().is_some_and(|g| g.key == group.key))
            .filter_map(|(slot, _)| shown.get(slot).copied())
            .collect();
        crate::trace::trace(&format!(
            "space: grouping {members:x?} as {:?}",
            group.title
        ));
        let made = crate::features::tabs::group_tabs(window, &members, &group.title, group.color);
        crate::trace::trace(&format!("space: grouped {made:02x?}"));
        if let Some(id) = made {
            groups.push((group.key.clone(), id));
        }
    }
    let wanted = restore.iter().position(|tab| tab.active).unwrap_or(0);
    if let Some(id) = shown.get(wanted) {
        let position = crate::features::tabs::open_tabs(window)
            .and_then(|tabs| tabs.iter().position(|tab| tab.id == *id));
        if let Some(position) = position {
            crate::features::tabs::activate_tab(window, position);
        }
    }
    // A collapsed group is folded last, and never over the tab that is shown.
    let active_group = restore
        .get(wanted)
        .and_then(|tab| tab.group.as_ref())
        .map(|g| g.key.clone());
    for (key, id) in &groups {
        let collapsed = restore
            .iter()
            .filter_map(|tab| tab.group.as_ref())
            .any(|group| group.key == *key && group.collapsed);
        if !collapsed || active_group.as_deref() == Some(key.as_str()) {
            continue;
        }
        crate::features::tabs::toggle_group(window, id);
    }
}

/// What the persistence of spaces knows between timer ticks.
#[derive(Default)]
struct Persist {
    /// The one window of this process that saves and restores. A second window of the same
    /// profile would only overwrite the file with its own tabs.
    owner: isize,
    /// The text last written, so an unchanged state costs no write.
    last: String,
    /// The tabs of the current space the file held, until the window shows them once.
    pending: Option<Vec<SavedTab>>,
    /// Saves stay off until then, after the window was asked to close, because closing empties the
    /// tab list one tab at a time and that must not become the saved state.
    quiet_until: Option<std::time::Instant>,
    /// The start pages the window opened with were compared with its space.
    start_checked: bool,
}

thread_local! {
    static PERSIST: RefCell<Persist> = RefCell::new(Persist::default());
}

/// The spaces file of this process's browser profile.
pub(crate) fn spaces_file() -> Option<(std::path::PathBuf, String)> {
    let root = rivet_core::paths::data_home()?;
    let dir = crate::spaces::user_data_dir_of(
        std::env::args_os().map(|arg| arg.to_string_lossy().into_owned()),
    );
    Some((
        crate::spaces::store_path(&root, dir.as_deref()),
        dir.unwrap_or_default(),
    ))
}

/// Restores the saved spaces, the appbar width and its hidden state when a window attaches.
/// A corrupt file is kept under another name and its copy is read instead. Without a good one the
/// defaults stay.
unsafe fn load_persisted(window: HWND) {
    let owns = PERSIST.with(|persist| {
        let mut persist = persist.borrow_mut();
        let free = persist.owner == 0;
        if free {
            persist.owner = window as isize;
        }
        free
    });
    let Some((path, _)) = spaces_file().filter(|_| owns) else {
        return;
    };
    let Some(mut stored) = crate::spaces::read_recovering(&path, Spaces::from_json) else {
        return;
    };
    let pending = stored.spaces.take_current_tabs();
    stored
        .spaces
        .adopt_browser(crate::installed_browsers::current());
    let tint = stored.spaces.list()[stored.spaces.current()].tint;
    edit_layout(window, |layout| {
        layout.set_width(stored.width);
        layout.collapsed = stored.hidden;
    });
    SPACES.with(|all| all.borrow_mut().insert(window as isize, stored.spaces));
    APPBAR_STATE.with(|states| {
        states
            .borrow_mut()
            .entry(window as isize)
            .or_insert((None, None, tint))
            .2 = tint;
    });
    PERSIST.with(|persist| persist.borrow_mut().pending = Some(pending));
}

/// Shows the tabs the file held for the current space, once and only over a window that shows
/// nothing of its own, then writes the state whenever it changed.
unsafe fn persist_tick(window: HWND) {
    if PERSIST.with(|persist| persist.borrow().owner) != window as isize {
        return;
    }
    let Some(open) = crate::features::tabs::open_tabs(window) else {
        return;
    };
    if let Some(tabs) = PERSIST.with(|persist| persist.borrow_mut().pending.take()) {
        if !tabs.is_empty()
            && open
                .iter()
                .all(|tab| crate::spaces::is_blank_tab(&tab.saved.url))
        {
            show_tabs(window, open.iter().map(|tab| tab.id).collect(), &tabs);
            return;
        }
    }
    // The launcher opens a start page before the window knows its space. Once the addresses are
    // known, a page made for another space is replaced, once per process.
    let unchecked = !PERSIST.with(|persist| persist.borrow().start_checked);
    if unchecked && !open.is_empty() && open.iter().all(|tab| !tab.saved.url.is_empty()) {
        PERSIST.with(|persist| persist.borrow_mut().start_checked = true);
        let start = start_url_of(window).unwrap_or_default();
        let urls: Vec<&str> = open.iter().map(|tab| tab.saved.url.as_str()).collect();
        if crate::spaces::start_pages_stale(&urls, &start) {
            let tabs: Vec<SavedTab> = open.iter().map(|tab| tab.saved.clone()).collect();
            show_tabs(window, open.iter().map(|tab| tab.id).collect(), &tabs);
            return;
        }
    }
    let quiet = PERSIST.with(|persist| {
        persist
            .borrow()
            .quiet_until
            .is_some_and(|until| std::time::Instant::now() < until)
    });
    if quiet {
        return;
    }
    save_spaces(window, open.into_iter().map(|tab| tab.saved).collect());
}

/// Writes the spaces with `open` as the tabs of the current space, unless nothing changed.
unsafe fn save_spaces(window: HWND, open: Vec<SavedTab>) {
    let Some((path, dir)) = spaces_file() else {
        return;
    };
    let layout = layout_of(window);
    let text = with_spaces(window, |spaces| {
        spaces.to_json(&open, layout.side, layout.collapsed, &dir)
    });
    let changed = PERSIST.with(|persist| persist.borrow().last != text);
    if !changed {
        return;
    }
    match crate::spaces::write_atomic(&path, &text) {
        Ok(()) => PERSIST.with(|persist| persist.borrow_mut().last = text),
        Err(error) => log(&format!("rivet: could not save the spaces: {error}")),
    }
}

/// The window was asked to close: keep what it shows now, and stop saving while it empties.
unsafe fn persist_closing(window: HWND) {
    if PERSIST.with(|persist| persist.borrow().owner) != window as isize {
        return;
    }
    persist_tick(window);
    PERSIST.with(|persist| {
        persist.borrow_mut().quiet_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(5))
    });
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam == PM_REMOVE as usize {
        let message = &mut *(lparam as *mut MSG);
        if message.message == WM_KEYDOWN
            && message.wParam == b'S' as usize
            && GetKeyState(0x11) < 0
            && GetKeyState(0x10) < 0
            && GetKeyState(0x12) >= 0
        {
            let window = GetAncestor(message.hwnd, GA_ROOT);
            if attached(window) {
                PostMessageW(window, toggle_message(), 0, 0);
                message.message = WM_NULL;
            }
        }
        // Ctrl+Shift+\ splits the page or ends the split.
        if message.message == WM_KEYDOWN
            && message.wParam == VK_OEM_5
            && GetKeyState(0x11) < 0
            && GetKeyState(0x10) < 0
            && GetKeyState(0x12) >= 0
        {
            let window = GetAncestor(message.hwnd, GA_ROOT);
            if attached(window) {
                PostMessageW(
                    window,
                    RegisterWindowMessageW(wide("Rivet.ToggleSplit").as_ptr()),
                    0,
                    0,
                );
                message.message = WM_NULL;
            }
        }
        // Ctrl+Shift+F12 switches developer mode of the page.
        if message.message == WM_KEYDOWN
            && message.wParam == 0x7B
            && GetKeyState(0x11) < 0
            && GetKeyState(0x10) < 0
            && GetKeyState(0x12) >= 0
        {
            let window = GetAncestor(message.hwnd, GA_ROOT);
            if attached(window) {
                PostMessageW(
                    window,
                    RegisterWindowMessageW(wide("Rivet.ToggleDevMode").as_ptr()),
                    0,
                    0,
                );
                message.message = WM_NULL;
            }
        }
        // Ctrl+Shift+T reopens the closed tab where it was.
        if message.message == WM_KEYDOWN
            && message.wParam == b'T' as usize
            && GetKeyState(0x11) < 0
            && GetKeyState(0x10) < 0
            && GetKeyState(0x12) >= 0
        {
            let window = GetAncestor(message.hwnd, GA_ROOT);
            if attached(window) {
                PostMessageW(
                    window,
                    RegisterWindowMessageW(wide("Rivet.ReopenTab").as_ptr()),
                    0,
                    0,
                );
                message.message = WM_NULL;
            }
        }
        // Ctrl+T opens the command bar or the new tab page, as the setting says.
        if message.message == WM_KEYDOWN
            && message.wParam == b'T' as usize
            && GetKeyState(0x11) < 0
            && GetKeyState(0x10) >= 0
            && GetKeyState(0x12) >= 0
        {
            let window = GetAncestor(message.hwnd, GA_ROOT);
            if attached(window) {
                PostMessageW(
                    window,
                    RegisterWindowMessageW(wide("Rivet.CtrlT").as_ptr()),
                    0,
                    0,
                );
                message.message = WM_NULL;
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe fn install_keyboard_hook() {
    KEYBOARD_HOOK.with(|hook| {
        if *hook.borrow() == 0 {
            let installed = SetWindowsHookExW(
                WH_GETMESSAGE,
                Some(keyboard_hook),
                std::ptr::null_mut(),
                GetCurrentThreadId(),
            );
            if installed.is_null() {
                log("rivet: could not install keyboard hook");
            }
            *hook.borrow_mut() = installed as isize;
        }
    });
}

fn menu_open(window: HWND) -> bool {
    open_menu_kind(window).is_some()
}

fn open_menu_kind(window: HWND) -> Option<Menu> {
    MENUS.with(|menus| menus.borrow().get(&(window as isize)).map(|menu| menu.1))
}

/// Opens a menu. It takes the keyboard and the pointer until it closes.
unsafe fn open_menu(window: HWND, menu: Menu) {
    let previous = GetFocus();
    MENUS.with(|menus| {
        menus
            .borrow_mut()
            .insert(window as isize, (previous as isize, menu))
    });
    SetFocus(window);
    rivet_platform_windows::focus::capture_for_menu(window);
}

/// Whether a client point is inside the open menu, and the entry it is on.
unsafe fn menu_pointer(window: HWND, lparam: LPARAM) -> (bool, Option<AppbarHit>) {
    let Some(menu) = open_menu_kind(window) else {
        return (false, None);
    };
    let x = (lparam & 0xffff) as i16 as i32;
    let y = ((lparam >> 16) & 0xffff) as i16 as i32;
    let (lx, ly) = local_dip(window, x, y);
    menu_hit(
        menu,
        lx,
        ly,
        layout_of(window).side as f32,
        appbar_height(window),
        with_spaces(window, |spaces| spaces.list().len()),
    )
}

unsafe fn close_menu(window: HWND) {
    let previous = MENUS.with(|menus| menus.borrow_mut().remove(&(window as isize)));
    if let Some((previous, _)) = previous {
        // The pointer was on the menu, so nothing under it is known until it moves again.
        APPBAR_STATE.with(|states| {
            if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
                entry.0 = None;
            }
        });
        rivet_platform_windows::focus::release(window);
        if previous != 0 {
            SetFocus(previous as HWND);
        }
    }
}

/// Where the tab row at `y` ends, in the appbar's coordinates, with the tab it shows. Rows are
/// laid out as `appbar_hit` walks them.
fn tab_row_end(rows: &[Row], y: f32) -> Option<(usize, i32)> {
    let mut top = 138.0;
    for row in rows {
        if matches!(row, Row::NewTab) && top > 138.0 {
            top += 17.0;
        }
        if (top..top + tab_rows::height(row)).contains(&y) {
            return match row {
                Row::Tab { index, .. } => Some((*index, (top + tab_rows::height(row)) as i32)),
                _ => None,
            };
        }
        top += tab_rows::height(row);
    }
    None
}

fn appbar_hit(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rows: &[Row],
    spaces: usize,
) -> Option<AppbarHit> {
    let left = 10.0;
    let inner = width - 18.0;
    if !(x >= left && x < left + inner) {
        return None;
    }
    if (left + 1.0..left + 29.0).contains(&x) && (12.0..40.0).contains(&y) {
        return Some(AppbarHit::Mark);
    }
    if (width - 36.0..width - 8.0).contains(&x) && (12.0..40.0).contains(&y) {
        return Some(AppbarHit::WorkspaceMenu);
    }
    if (106.0..136.0).contains(&y) {
        return Some(AppbarHit::SpaceHeader);
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
        if (top..top + tab_rows::height(row)).contains(&y) {
            return Some(match row {
                Row::Group(group) => AppbarHit::Folder(group.id),
                // The title of the split group answers nothing.
                Row::SplitTitle { .. } => return None,
                Row::NewTab => AppbarHit::NewTab,
                Row::Tab { index, .. } if x >= left + inner - 34.0 => AppbarHit::CloseTab(*index),
                Row::Tab { index, .. } => AppbarHit::Tab(*index),
            });
        }
        top += tab_rows::height(row);
    }
    let foot = height - FOOT_HEIGHT;
    if (foot..foot + 28.0).contains(&y) {
        if (left..left + 28.0).contains(&x) {
            return Some(AppbarHit::Settings);
        }
        if (width - 36.0..width - 8.0).contains(&x) {
            // A full footer has no room for another space, and the button is dimmed.
            return (spaces < MAX_SPACES).then_some(AppbarHit::NewSpace);
        }
        let dots = spaces as f32 * 32.0 - 4.0;
        let start = left + 28.0 + (inner - 56.0 - dots) / 2.0;
        let index = ((x - start) / 32.0).floor();
        if index >= 0.0 && (index as usize) < spaces {
            let index = index as usize;
            if x < start + index as f32 * 32.0 + 28.0 {
                return Some(AppbarHit::Space(index));
            }
        }
    }
    None
}

fn appbar_model(window: HWND) -> Vec<Row> {
    let split = unsafe { crate::features::split::shown_tabs(window) };
    if let Some(rows) = unsafe { crate::features::tab_list::rows(window) } {
        return tab_rows::group_split(rows, &split);
    }
    let tabs = unsafe { crate::features::tabs::tabs(window) }.unwrap_or_default();
    tab_rows::group_split(tab_rows::build_rows(&tabs), &split)
}

unsafe fn appbar_point(window: HWND, lparam: LPARAM) -> Option<AppbarHit> {
    let point = client_point(window, lparam);
    let layout = layout_of(window);
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let (x, y) = local_dip(window, point.x, point.y);
    if let Some(menu) = open_menu_kind(window) {
        return menu_hit(
            menu,
            x,
            y,
            layout.side as f32,
            appbar_height(window),
            with_spaces(window, |spaces| spaces.list().len()),
        )
        .1;
    }
    if layout.collapsed {
        if layout.peek && in_floating(layout, x, y, appbar_height(window)) {
            return appbar_hit(
                x,
                y,
                layout.side as f32,
                appbar_height(window),
                &appbar_model(window),
                with_spaces(window, |spaces| spaces.list().len()),
            );
        }
        return toggle_at(window, point);
    }
    if let Some(hit) = toggle_at(window, point) {
        return Some(hit);
    }
    if x >= (layout.side - RESIZER_WIDTH) as f32 && x < layout.side as f32 {
        return Some(AppbarHit::Resizer);
    }
    appbar_hit(
        x,
        y,
        layout.side as f32,
        appbar_height(window),
        &appbar_model(window),
        with_spaces(window, |spaces| spaces.list().len()),
    )
}

/// Whether a point in the floating appbar's own coordinates is inside it.
fn in_floating(layout: AppbarLayout, x: f32, y: f32, height: f32) -> bool {
    (0.0..layout.side as f32).contains(&x) && (0.0..height).contains(&y)
}

/// Whether a client point, in pixels, is on the toggle button.
unsafe fn toggle_at(window: HWND, point: POINT) -> Option<AppbarHit> {
    let factor = layout_dpi(window) as f32 / 96.0;
    let (left, top, width, height) = toggle_rect(page_x(window));
    let (x, y) = (point.x as f32 / factor, point.y as f32 / factor);
    ((left as f32..(left + width) as f32).contains(&x)
        && (top as f32..(top + height) as f32).contains(&y))
    .then_some(AppbarHit::Toggle)
}

/// Whether a client point, in pixels, is inside the floating appbar of a peeking window.
unsafe fn floating_at(window: HWND, point: POINT) -> bool {
    let layout = layout_of(window);
    let (x, y) = local_dip(window, point.x, point.y);
    layout.collapsed && layout.peek && in_floating(layout, x, y, appbar_height(window))
}

unsafe fn appbar_action(window: HWND, hit: AppbarHit) {
    match hit {
        AppbarHit::Mark => start_page(window),
        AppbarHit::WorkspaceMenu | AppbarHit::SpaceHeader | AppbarHit::Settings => {
            let menu = if hit == AppbarHit::Settings {
                Menu::Settings
            } else {
                Menu::Workspace
            };
            if open_menu_kind(window) == Some(menu) {
                close_menu(window);
            } else {
                close_menu(window);
                open_menu(window, menu);
            }
        }
        AppbarHit::Credentials => {
            // The browser reads the provider's switches and preferences when a Rivet profile
            // starts, so the choice applies from the next start.
            let next = saved_credentials().other();
            if let Some(path) = setup_file() {
                let saved = rivet_core::credentials::save_to_setup(&path, next);
                crate::trace::trace(&format!(
                    "passwords and passkeys: {} saved {saved:?}",
                    next.as_str()
                ));
            }
            crate::frame::update_appbar(window);
        }
        AppbarHit::RivetSettings => {
            close_menu(window);
            crate::features::settings::open(window);
        }
        AppbarHit::ProfileSettings | AppbarHit::ManageProfiles => {
            close_menu(window);
            let url = if hit == AppbarHit::ProfileSettings {
                "chrome://settings/people"
            } else {
                "chrome://settings/manageProfile"
            };
            if crate::features::tabs::new_tab(window) {
                crate::features::navigation::open(window, url);
            }
        }
        AppbarHit::Tab(index) => {
            crate::features::tabs::activate_tab(window, index);
        }
        AppbarHit::SplitTab(index) => {
            close_menu(window);
            // The split is made from the active tab and the one after it.
            let activated = crate::features::tabs::activate_tab(window, index);
            let split = activated && crate::features::split::split_active(window);
            crate::trace::trace(&format!("tab menu: split tab {index}: {activated} {split}"));
        }
        AppbarHit::CloseTab(index) => {
            crate::features::tab_list::close(window, index);
        }
        AppbarHit::PinTab(index) => {
            close_menu(window);
            crate::features::tab_list::toggle_pinned(window, index);
        }
        AppbarHit::FolderTab(index) => {
            close_menu(window);
            crate::features::tab_list::new_folder(window, index, "New folder");
        }
        AppbarHit::NewTab => {
            start_page(window);
        }
        AppbarHit::Pinned(index) => {
            const URLS: [&str; 4] = [
                "https://mail.google.com/",
                "https://calendar.google.com/",
                "https://docs.google.com/",
                "https://keep.google.com/",
            ];
            if crate::features::tabs::new_tab(window) {
                crate::features::navigation::open(window, URLS[index]);
            }
        }
        AppbarHit::Space(index) | AppbarHit::MenuSpace(index) => {
            close_menu(window);
            switch_space(window, index);
        }
        AppbarHit::NewSpace => {
            if let Some(index) = with_spaces(window, |spaces| spaces.add()) {
                switch_space(window, index);
            }
        }
        AppbarHit::Folder(id) => {
            if !crate::features::tab_list::toggle_folder(window, &id) {
                crate::features::tabs::toggle_group(window, &id);
            }
        }
        AppbarHit::Toggle => toggle_appbar(window),
        AppbarHit::Resizer => {}
    }
}

/// Opens the menu of the tab row under a right click, in screen coordinates.
unsafe fn open_tab_menu(window: HWND, lparam: LPARAM) {
    let point = client_point(window, lparam);
    let (x, y) = local_dip(window, point.x, point.y);
    let layout = layout_of(window);
    if !(10.0..layout.side as f32 - 8.0).contains(&x) {
        return;
    }
    if let Some((index, top)) = tab_row_end(&appbar_model(window), y) {
        close_menu(window);
        open_menu(window, Menu::Tab { index, top });
        update_appbar(window);
    }
}

/// A control goes down: it starts to sink, and keeps being drawn until it has sprung back.
unsafe fn press_down(window: HWND, hit: Option<AppbarHit>) {
    let Some(hit) = hit.filter(|hit| *hit != AppbarHit::Resizer) else {
        return;
    };
    PRESSES.with(|all| {
        all.borrow_mut()
            .entry(window as isize)
            .or_default()
            .press(hit, now_ms())
    });
    SetTimer(window, PRESS_TIMER, 16, None);
}

/// The pointer comes up or leaves the control that was down.
unsafe fn press_up(window: HWND) {
    PRESSES.with(|all| {
        if let Some(presses) = all.borrow_mut().get_mut(&(window as isize)) {
            presses.release_all(now_ms());
        }
    });
}

/// Advances the press feedback by one frame, and stops the timer when everything has settled.
unsafe fn press_tick(window: HWND) {
    let moving = PRESSES.with(|all| {
        all.borrow_mut()
            .get_mut(&(window as isize))
            .is_some_and(|presses| presses.settle(now_ms()))
    });
    if !moving {
        KillTimer(window, PRESS_TIMER);
    }
    update_appbar(window);
}

/// The scale and brightness amounts of the window's controls in motion.
fn press_effects(window: HWND) -> Vec<crate::appbar::Effect> {
    PRESSES.with(|all| {
        all.borrow()
            .get(&(window as isize))
            .map(|presses| presses.amounts(now_ms()))
            .unwrap_or_default()
    })
}

unsafe fn toggle_message() -> u32 {
    RegisterWindowMessageW(wide(TOGGLE_MESSAGE).as_ptr())
}

/// Makes Chromium's tab strip, which the appbar covers, as wide as the appbar: nothing while the
/// appbar is hidden.
unsafe fn sync_strip(window: HWND) {
    let layout = layout_of(window);
    crate::features::vertical_tabs::set_strip_override(window, layout.strip());
}

/// Lays the browser out again, now that the appbar's width or mode changed, and redraws.
unsafe fn relayout(window: HWND) {
    // The frame view lays out again on request, which also works for a maximized window.
    crate::features::vertical_tabs::invalidate_strip(window);
    if frame_layout::relayout(window) {
        update_appbar(window);
        return;
    }
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

/// Hides the docked appbar so the page takes its width, or docks it again.
unsafe fn toggle_appbar(window: HWND) {
    close_menu(window);
    edit_layout(window, |layout| layout.toggle());
    crate::trace::trace(&format!("appbar: hidden {}", layout_of(window).collapsed));
    sync_strip(window);
    SLIDE.with(|slides| slides.borrow_mut().remove(&(window as isize)));
    KillTimer(window, PEEK_TIMER);
    relayout(window);
}

/// Shows or puts away the floating appbar of a hidden one, sliding it.
unsafe fn set_peek(window: HWND, on: bool) {
    let changed = edit_layout(window, |layout| {
        let changed = layout.collapsed && layout.peek != on;
        if changed {
            layout.peek = on;
        }
        changed
    });
    if changed {
        crate::trace::trace(&format!("appbar: peek {on}"));
        SetTimer(window, PEEK_TIMER, 16, None);
    }
}

/// Follows the pointer, in client pixels, to the left edge or away from the floating appbar.
unsafe fn track_pointer(window: HWND, x: i32, y: i32) {
    let layout = layout_of(window);
    if !layout.collapsed || menu_open(window) {
        return;
    }
    let factor = layout_dpi(window) as f32 / 96.0;
    let height = appbar_height(window) + 2.0 * FLOAT_INSET as f32;
    let (dx, dy) = (
        (x as f32 / factor).floor() as i32,
        ((y - top_border(window)) as f32 / factor).floor() as i32,
    );
    if !layout.peek && layout.reaches_edge(dx) {
        // Windows reports a leave at once when the pointer is not over the window, which only
        // happens for input that was posted rather than moved.
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        if GetAncestor(WindowFromPoint(cursor), GA_ROOT) == window {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: window,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tracking);
        }
        set_peek(window, true);
    } else if layout.peek && !layout.keeps_peek(dx, dy, height as i32) {
        set_peek(window, false);
    }
}

/// Puts the floating appbar away when the real pointer is no longer over it. `strict` also counts
/// the pointer being over another window as leaving.
unsafe fn check_peek(window: HWND, strict: bool) {
    let layout = layout_of(window);
    if !layout.collapsed || !layout.peek || menu_open(window) || dragging(window) {
        return;
    }
    let mut cursor = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor);
    if GetAncestor(WindowFromPoint(cursor), GA_ROOT) != window {
        // Tests drive the window with posted input while the real pointer is elsewhere. The
        // flag file in the debug folder keeps such a peek from leaving with the real pointer.
        let held = debug_directory().is_some_and(|dir| dir.join("rivet-hold-peek.flag").exists());
        if strict && !held {
            set_peek(window, false);
        }
        return;
    }
    ScreenToClient(window, &mut cursor);
    let factor = layout_dpi(window) as f32 / 96.0;
    let height = appbar_height(window) + 2.0 * FLOAT_INSET as f32;
    let (x, y) = (
        (cursor.x as f32 / factor).floor() as i32,
        ((cursor.y - top_border(window)) as f32 / factor).floor() as i32,
    );
    if !layout.keeps_peek(x, y, height as i32) {
        set_peek(window, false);
    }
}

/// Advances the slide of the floating appbar by one frame, and polls the pointer while it rests.
unsafe fn peek_tick(window: HWND) {
    let layout = layout_of(window);
    let before = slide_of(window);
    let target = if layout.collapsed && layout.peek {
        1.0
    } else {
        0.0
    };
    let after = if target > before {
        (before + SLIDE_STEP).min(1.0)
    } else {
        (before - SLIDE_STEP).max(0.0)
    };
    SLIDE.with(|slides| slides.borrow_mut().insert(window as isize, after));
    if after != before {
        update_appbar(window);
    } else if target == 1.0 {
        check_peek(window, false);
    } else {
        KillTimer(window, PEEK_TIMER);
    }
}

unsafe fn end_drag(window: HWND) {
    if dragging(window) {
        apply_drag(window);
    }
    if DRAGGING.with(|set| set.borrow_mut().remove(&(window as isize))) {
        rivet_platform_windows::focus::release(window);
        APPBAR_STATE.with(|states| {
            if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
                entry.0 = None;
                entry.1 = None;
            }
        });
        update_appbar(window);
    }
}

/// Ticks while a pointer position of the width drag waits for its frame.
const DRAG_TIMER: usize = 0x5249_5653 + 1;

/// The shortest time between two layouts of the width drag, one displayed frame.
const DRAG_FRAME_MS: f32 = 16.0;

/// The pointer x of a pending width drag, when the browser was last laid out for it, and whether
/// its timer is running.
type DragPending = (Option<i32>, f32, bool);

thread_local! {
    /// The latest pointer x of a width drag that has not been laid out yet, and when the browser
    /// was last laid out for the drag.
    static DRAG_PENDING: RefCell<HashMap<isize, DragPending>> = RefCell::new(HashMap::new());
}

/// Follows a drag of the resizer: the appbar's right edge goes to the pointer and the browser lays
/// the page out beside it. Pointer moves can arrive faster than the browser lays out, so the moves
/// already waiting are folded into the latest one, and a layout starts as soon as the last one
/// is at least a short moment old. A timer only finishes a move that arrived too early, and the
/// release of the button applies whatever is still waiting.
unsafe fn drag_to(window: HWND, x: i32) {
    let mut x = x;
    let mut waiting: MSG = std::mem::zeroed();
    while PeekMessageW(&mut waiting, window, WM_MOUSEMOVE, WM_MOUSEMOVE, PM_REMOVE) != 0 {
        x = (waiting.lParam & 0xffff) as i16 as i32;
    }
    let now = now_ms();
    let due = DRAG_PENDING.with(|pending| {
        let mut all = pending.borrow_mut();
        let entry = all
            .entry(window as isize)
            .or_insert((None, f32::MIN, false));
        entry.0 = Some(x);
        let wait = DRAG_FRAME_MS - (now - entry.1);
        if wait <= 0.0 {
            true
        } else {
            // Setting a running timer again would restart it and starve it under a flood of moves.
            if !entry.2 {
                entry.2 = true;
                SetTimer(window, DRAG_TIMER, wait.ceil().max(1.0) as u32, None);
            }
            false
        }
    });
    if due {
        apply_drag(window);
    }
}

/// Lays the browser out for the latest pointer position of the width drag, if one is waiting.
unsafe fn apply_drag(window: HWND) {
    let now = now_ms();
    let latest = DRAG_PENDING.with(|pending| {
        pending
            .borrow_mut()
            .get_mut(&(window as isize))
            .and_then(|entry| {
                entry.1 = now;
                entry.2 = false;
                entry.0.take()
            })
    });
    KillTimer(window, DRAG_TIMER);
    let Some(x) = latest else { return };
    let factor = layout_dpi(window) as f32 / 96.0;
    let wanted = AppbarLayout::width_at((x as f32 / factor).round() as i32);
    if edit_layout(window, |layout| layout.set_width(wanted)) {
        crate::trace::trace(&format!("appbar: width {}", layout_of(window).side));
        sync_strip(window);
        // The appbar takes its new width before the browser lays the page out, so what the
        // browser exposes is only ever ground or appbar, never the appbar's old surface over a
        // page that has already moved.
        update_appbar(window);
        relayout(window);
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

/// Whether `window` has the class of the browser's top-level windows.
pub(crate) unsafe fn is_browser_window_class(window: HWND) -> bool {
    class_name(window) == BROWSER_CLASS
}

unsafe fn attach_message() -> u32 {
    RegisterWindowMessageW(wide(ATTACH_MESSAGE).as_ptr())
}

/// The frame surrounding the browser's page panel.
fn nav_frame_color(window: HWND) -> Option<u32> {
    attached(window).then(|| 0xFF00_0000 | rivet_core::space_frame(space_tint(window)))
}

const TAB_REFRESH_TIMER: usize = 0x5249_5645;
const OPEN_LATER_TIMER: usize = 0x5249_5646;
/// Lays the second page view out again for a short while after the window changes size, because
/// the browser may lay its page view out after the size message returns.
const SECOND_VIEW_TIMER: usize = 0x5249_5647;
/// Ticks of `SECOND_VIEW_TIMER` in a row that moved nothing, after which it stops.
static SECOND_VIEW_IDLE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Lays the second page view out now and keeps doing so for a few ticks.
pub(crate) unsafe fn follow_second_view(window: HWND) {
    crate::features::pane_infobar::follow(window);
    if !crate::features::second_view::is_open(window) {
        return;
    }
    crate::features::second_view::relayout(window);
    SECOND_VIEW_IDLE.store(0, std::sync::atomic::Ordering::Relaxed);
    SetTimer(window, SECOND_VIEW_TIMER, 40, None);
}

thread_local! {
    /// Start pages waiting for the browser to finish its own work on a new tab: the window, the page
    /// and the address.
    static OPEN_LATER: RefCell<Vec<(isize, usize, String)>> = const { RefCell::new(Vec::new()) };
}

/// Opens `url` in the page `contents` of `window` after `delay` milliseconds, if it is still active.
pub(crate) unsafe fn open_later(window: HWND, contents: usize, url: String, delay: u32) {
    OPEN_LATER.with(|later| later.borrow_mut().push((window as isize, contents, url)));
    windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(window, OPEN_LATER_TIMER, delay, None);
}

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

/// The windows of this process that show the Rivet frame. Call it on the browser UI thread.
pub(crate) fn rivet_windows() -> Vec<HWND> {
    APPBARS.with(|appbars| {
        appbars
            .borrow()
            .keys()
            .map(|window| *window as HWND)
            .collect()
    })
}

/// Every top-level browser window of this process, whoever made it. Only windows of the browser's
/// class are visited, so the half second tab timer does not read the class of every window.
pub(crate) fn browser_windows_of_process() -> Vec<HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowExW;
    thread_local! {
        static CLASS: Vec<u16> = wide(BROWSER_CLASS);
    }
    let mut found: Vec<HWND> = Vec::new();
    CLASS.with(|class| unsafe {
        let me = GetCurrentProcessId();
        let mut window = FindWindowExW(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            class.as_ptr(),
            std::ptr::null(),
        );
        while !window.is_null() {
            let mut owner = 0;
            GetWindowThreadProcessId(window, &mut owner);
            if owner == me {
                found.push(window);
            }
            window = FindWindowExW(
                std::ptr::null_mut(),
                window,
                class.as_ptr(),
                std::ptr::null(),
            );
        }
    });
    found
}

fn attached(window: HWND) -> bool {
    unsafe { attached_window(window) }
}

pub(crate) unsafe fn attached_window(window: HWND) -> bool {
    APPBARS.with(|appbars| appbars.borrow().contains_key(&(window as isize)))
}

unsafe fn framed(window: HWND) -> bool {
    // Chromium removes the sizing frame for fullscreen and restores it when leaving.
    GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_THICKFRAME != 0 && IsIconic(window) == 0
}

unsafe fn dpi(window: HWND) -> u32 {
    GetDpiForWindow(window).max(96)
}

/// The folder of hand-placed test and trace switches: the process's own `rivet-debug` when its
/// profile has one, else the one in Rivet's own data. Never the install folder, which an update
/// replaces and the person's account cannot write.
pub(crate) fn debug_directory() -> Option<std::path::PathBuf> {
    own_debug_directory()
        .or_else(|| rivet_core::paths::Layout::current().map(|layout| layout.debug()))
}

/// The folder of debug switches that belong to this browser process alone: `rivet-debug` inside the
/// profile it was started with. Tests run side by side on one machine, each on its own profile, and
/// the shared debug folder would let one test's window take another test's command.
pub(crate) fn own_debug_directory() -> Option<std::path::PathBuf> {
    own_debug_directory_in(std::env::args())
}

fn own_debug_directory_in(arguments: impl Iterator<Item = String>) -> Option<std::path::PathBuf> {
    rivet_core::paths::session_debug_dir(arguments)
}

/// The folder of debug command files that the window's timer looks in, decided once per process:
/// the process's own when its profile has one, else the shared one when it exists. `None` means
/// no folder existed when the first window attached, and the timer then touches no files at all,
/// so an ordinary window never polls the disk. A tester places the folder before the launch.
pub(crate) fn debug_switches() -> Option<&'static std::path::Path> {
    static FOUND: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            pick_debug_switches(
                own_debug_directory(),
                rivet_core::paths::Layout::current().map(|layout| layout.debug()),
                |dir| dir.is_dir(),
            )
        })
        .as_deref()
}

fn pick_debug_switches(
    own: Option<std::path::PathBuf>,
    shared: Option<std::path::PathBuf>,
    exists: impl Fn(&std::path::Path) -> bool,
) -> Option<std::path::PathBuf> {
    own.filter(|dir| exists(dir))
        .or_else(|| shared.filter(|dir| exists(dir)))
}

/// Every how many tab refresh ticks the window checks that its taskbar identity is still Rivet's.
const TASKBAR_CHECK_EVERY: u64 = 4;

fn taskbar_check_due(tick: u64) -> bool {
    tick % TASKBAR_CHECK_EVERY == 0
}

thread_local! {
    static TAB_TICKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The log a switched-on trace writes, rotated when it is full. `None` while the switch is off.
fn trace_file(name: &str) -> Option<std::path::PathBuf> {
    crate::trace::switched(name)
}

pub(crate) unsafe fn runtime_directory() -> Option<std::path::PathBuf> {
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
    // A test run's window goes off every monitor and stops activating before anything else runs.
    crate::offscreen::keep_from_attach(window);
    // The window stays cloaked until it has Rivet's look, so no frame shows the browser's own strip.
    crate::features::look::first_look(window, crate::features::look::FirstLook::Attached);
    // DirectComposition targets and frame changes belong on the UI thread that owns the window.
    let posted = PostMessageW(window, attach_message(), 0, 0) != 0;
    if posted {
        // The scale Rivet draws at from the start, before the browser's first layout records its
        // own; a browser may lay out a window only once a second tab opens.
        crate::trace::trace(&format!(
            "layout dpi: window {:#x} at {}",
            window as usize,
            layout_dpi(window)
        ));
        crate::features::control::serve(window);
    } else {
        crate::features::look::first_look(window, crate::features::look::FirstLook::AttachFailed);
    }
    posted
}

unsafe fn call_browser(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let original: WNDPROC = std::mem::transmute(GetPropW(window, wide(ORIGINAL_PROC).as_ptr()));
    CallWindowProcW(original, window, message, wparam, lparam)
}

/// What the attach sequence carries from one step to the next.
struct Attach {
    window: HWND,
    /// Whether the browser's own vertical tab strip was reported to the appbar.
    vertical_tabs: bool,
}

fn caption_row_height(window: HWND) -> Option<i32> {
    attached(window).then_some(crate::CAPTION_HEIGHT + frame_layout::PANEL_TOP)
}

fn top_inset_extra(window: HWND) -> Option<i32> {
    // A Rivet window has no title bar above its panel: the browser's top elements begin at the
    // panel's top gap on every browser, whatever height its own title bar has.
    // A Mini Window keeps the room of Rivet's row on its own surface.
    attached(window)
        .then_some(frame_layout::PANEL_GAP)
        .or_else(|| crate::features::mini_window::top_inset(window))
}

/// The features of a Rivet window in the order they are installed. A feature the browser does not
/// have leaves the window as it is, and a failure of a feature the window cannot do without ends
/// the attach, so the browser keeps working without Rivet.
fn attach_steps() -> Vec<Step<Attach>> {
    use crate::feature::{Feature, FeatureStep, OnFailure, Step};
    let feature = |feature, needs, install, on_failure, skip_when_unavailable, log_failure| {
        Step::Feature(FeatureStep {
            feature,
            needs,
            install,
            on_failure,
            skip_when_unavailable,
            log_failure,
        })
    };
    vec![
        feature(
            Feature::FrameLayout,
            frame_layout::needs_layout,
            |_| unsafe { frame_layout::install(reserved_width) },
            OnFailure::Abort,
            false,
            Some("rivet: not attaching to this browser"),
        ),
        feature(
            Feature::FrameColour,
            frame_layout::needs_colour,
            |_| unsafe { frame_layout::install_frame_paint(nav_frame_color) },
            OnFailure::Abort,
            // A build without a recorded frame paint still gets the Rivet frame in the browser's own
            // frame colour, instead of no Rivet frame at all.
            true,
            Some("rivet: could not paint the frame"),
        ),
        Step::Setup(|_| {
            frame_layout::PANEL.store(true, std::sync::atomic::Ordering::Relaxed);
            frame_layout::on_panel_change(panel_moved);
            true
        }),
        feature(
            Feature::CaptionPaint,
            frame_layout::needs_caption_paint,
            |_| unsafe { frame_layout::install_caption_paint() },
            OnFailure::Continue,
            false,
            Some("rivet: could not paint caption buttons"),
        ),
        feature(
            Feature::CaptionLayout,
            frame_layout::needs_caption_layout,
            |_| unsafe { frame_layout::install_caption_layout(caption_row_height) },
            OnFailure::Continue,
            false,
            Some("rivet: could not lay out caption buttons"),
        ),
        // The caption height and the top inset size the panel's top row, so they never depend on the
        // trace file. Failures show in the trace when it is on.
        feature(
            Feature::CaptionHeight,
            frame_layout::needs_caption_height,
            |_| unsafe {
                let caption = frame_layout::install_caption_height(caption_row_height);
                crate::trace::trace(&format!("caption height: {caption:?}"));
                caption
            },
            OnFailure::Continue,
            false,
            None,
        ),
        feature(
            Feature::TopInset,
            frame_layout::needs_top_inset,
            |_| unsafe {
                let top = frame_layout::install_top_inset(top_inset_extra);
                crate::trace::trace(&format!("top inset: {top:?}"));
                top
            },
            OnFailure::Continue,
            false,
            None,
        ),
        Step::Setup(|_| unsafe {
            if let Some(path) = trace_file("rivet-nav.log") {
                crate::trace::set_trace(path);
            }
            let hide = trace_file("rivet-hide-tab-strip.log");
            if let Some(path) = hide {
                crate::trace::set_trace(path);
                if let Err(error) = crate::features::vertical_tabs::hide_tab_strip_everywhere() {
                    crate::trace::trace(&format!("hide failed: {error}"));
                }
            }
            // The trace switch only switches the trace on. The toolbar and the
            // vertical-tab frame are what the top row needs, so they never depend on it.
            if crate::trace::tracing() {
                crate::trace::trace_crashes();
            }
            true
        }),
        Step::Setup(|_| unsafe {
            let new_tab = crate::features::tabs::install_new_tab(crate::builds::get().toolbar.skip);
            crate::trace::trace(&format!("new tab: {new_tab:?}"));
            let links = crate::features::page_links::install();
            crate::trace::trace(&format!("links in pages: {links:?}"));
            true
        }),
        feature(
            Feature::Split,
            crate::features::split::needs,
            |_| unsafe {
                let split = crate::features::split::install();
                crate::trace::trace(&format!("split: {split:?}"));
                split
            },
            OnFailure::Continue,
            false,
            None,
        ),
        feature(
            Feature::SplitLayout,
            crate::features::split::needs,
            |_| unsafe {
                let panels = crate::features::split::install_panels();
                crate::trace::trace(&format!("split panels: {panels:?}"));
                panels
            },
            OnFailure::Continue,
            false,
            None,
        ),
        feature(
            Feature::VerticalTabs,
            crate::features::vertical_tabs::needs,
            |attach| unsafe {
                let installed = crate::features::vertical_tabs::install(report_vertical_tabs);
                if let Err(error) = &installed {
                    crate::trace::trace(&format!("unavailable: {error}"));
                }
                attach.vertical_tabs = installed.is_ok();
                installed
            },
            OnFailure::Continue,
            false,
            None,
        ),
        Step::Setup(|_| {
            crate::features::app_window::install_features();
            true
        }),
        Step::Setup(|attach| unsafe { compose_appbar(attach.window) }),
        Step::Setup(|attach| unsafe {
            let window = attach.window;
            crate::features::widgets::register(window);
            if debug_directory().is_some_and(|dir| {
                dir.join("rivet-group-probe.flag").exists() || dir.join("rivet-split.flag").exists()
            }) {
                PostMessageW(window, group_probe_message(), 0, 0);
            }
            crate::features::activity::switch_on_if_asked();
            // Tab titles and selection can change without a window title change.
            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(
                window,
                TAB_REFRESH_TIMER,
                500,
                None,
            );
            if attach.vertical_tabs {
                for controller in crate::features::vertical_tabs::reported() {
                    PostMessageW(window, vertical_tabs_message(), controller, 0);
                }
            }
            if let Err(error) = taskbar::apply(window as isize, runtime_directory().as_deref()) {
                log(&format!(
                    "rivet: could not set the taskbar identity: {error}"
                ));
            }
            set_rivet_icons(window);
            true
        }),
        feature(
            Feature::TabStrip,
            crate::features::tab_strip::needs,
            |attach| unsafe {
                let hidden = crate::features::tab_strip::hide(attach.window);
                crate::trace::trace(&format!("tab strip hidden: {hidden}"));
                if hidden {
                    Ok(())
                } else {
                    Err("the tab strip was not hidden".to_owned())
                }
            },
            OnFailure::Continue,
            true,
            None,
        ),
        Step::Setup(|attach| unsafe {
            // A frame change makes the browser lay out its frame again, now through the layout hook.
            SetWindowPos(
                attach.window,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            update_appbar(attach.window);
            true
        }),
    ]
}

unsafe fn create_appbar(window: HWND) {
    if attached(window) {
        return;
    }
    if let Some(path) = trace_file("rivet-vertical-tabs.log") {
        crate::trace::set_trace(path);
    }
    // Without the layout hook the browser would draw underneath the appbar, so an unknown
    // browser build keeps working without Rivet instead.
    let mut attach = Attach {
        window,
        vertical_tabs: false,
    };
    crate::feature::run(&attach_steps(), &mut attach, log);
}

/// Gives the window its appbar and the state that goes with it. False when composition is
/// unavailable, and the browser keeps working without Rivet.
unsafe fn compose_appbar(window: HWND) -> bool {
    let appbar = match Appbar::new(window as isize) {
        Ok(appbar) => appbar,
        Err(error) => {
            log(&format!("rivet: could not compose the appbar: {error}"));
            return false;
        }
    };
    APPBARS.with(|appbars| appbars.borrow_mut().insert(window as isize, appbar));
    install_keyboard_hook();
    load_persisted(window);
    sync_strip(window);
    APPBAR_STATE.with(|states| {
        states
            .borrow_mut()
            .entry(window as isize)
            .or_insert((None, None, 0xd08a52));
    });
    true
}

/// Places or hides the top bar's caret alone, for a blink or a caret move that changed nothing
/// else. The rest of the window's surface stays as it is.
pub(crate) unsafe fn update_address_caret(window: HWND) {
    let dpi = layout_dpi(window);
    let caret = crate::features::address_bar::caret(window);
    APPBARS.with(|appbars| {
        if let Some(appbar) = appbars.borrow_mut().get_mut(&(window as isize)) {
            if let Err(error) = appbar.place_address_caret(caret, dpi, true) {
                log(&format!(
                    "rivet: could not draw the top bar's caret: {error}"
                ));
            }
        }
    });
}

thread_local! {
    static TICK_GATE: RefCell<crate::features::tick_gate::Gate> = RefCell::default();
}

/// Whether the tab refresh timer saw anything the window's surface shows differently from the
/// last tick: its tabs, titles and states, its pages and their rows, the top bar and the cards
/// over the page, its size and its spaces. Pointer and key input draw on their own.
unsafe fn tick_needs_draw(window: HWND) -> bool {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let visible = framed(window);
    let address = if crate::features::address_bar::busy() {
        None
    } else {
        crate::features::address_bar::view(window)
    };
    let command = if crate::features::command_bar::busy() {
        None
    } else {
        crate::features::command_bar::view(window)
    };
    let card = if crate::features::settings::busy() {
        None
    } else {
        crate::features::settings::view(window)
    };
    let spaces = with_spaces(window, |spaces| (spaces.list().to_vec(), spaces.current()));
    let signature = crate::features::tick_gate::signature(&[
        &(client.right, client.bottom, layout_dpi(window), visible),
        &layout_of(window),
        &page_x(window),
        &appbar_model(window),
        &pane_rows(window),
        &address,
        &command,
        &card,
        &crate::features::link_preview::view(window),
        &crate::features::link_rules::view(window),
        &crate::dev_bar::view(window),
        &crate::features::dev_page::window_in_dev_mode(window),
        &held_warning(),
        &spaces,
        &open_menu_kind(window),
        &dragging(window),
        &frame_layout::panel_bounds(window),
        &ground_holes(window),
    ]);
    TICK_GATE.with(|gate| gate.borrow_mut().due(window as isize, signature))
}

/// Places or hides the caret of the command bar or the settings card alone, for a blink that
/// changed nothing else.
pub(crate) unsafe fn update_card_caret(window: HWND, which: crate::appbar::caret::Which) {
    use crate::appbar::caret::Which;
    let dpi = layout_dpi(window);
    let caret = match which {
        Which::Address => crate::features::address_bar::caret(window),
        Which::Command => crate::features::command_bar::caret(window),
        Which::Settings => crate::features::settings::caret(window),
    };
    APPBARS.with(|appbars| {
        if let Some(appbar) = appbars.borrow_mut().get_mut(&(window as isize)) {
            if let Err(error) = appbar.place_caret(which, caret, dpi, true) {
                log(&format!("rivet: could not draw a field's caret: {error}"));
            }
        }
    });
}

/// `update_appbar` for a window that may have lost Rivet's frame, which is then left alone.
pub(crate) fn redraw_appbar(window: crate::os::WindowId) {
    // Only a window with Rivet's frame is drawn, on the UI thread that owns it.
    if attached(window.hwnd()) {
        unsafe { update_appbar(window.hwnd()) }
    }
}

/// Places the appbar visual over the strip the browser's layout left free and redraws it when its
/// size or DPI changed.
pub(crate) unsafe fn update_appbar(window: HWND) {
    crate::features::activity::count(crate::features::activity::Counter::AppbarUpdate);
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let dpi = layout_dpi(window);
    let layout = layout_of(window);
    // A hidden appbar leaves its column to the page, apart from the gap of the shell study.
    let width = scale(
        if layout.collapsed {
            page_x(window)
        } else {
            layout.side
        },
        dpi,
    );
    let visible = framed(window);
    // A window's composition target is positioned at its client origin, which is also where the
    // browser's layout leaves the strip free. The window's own top border runs across the whole
    // window, so the appbar starts below it exactly like the browser's title bar.
    let top = top_border(window);
    APPBARS.with(|appbars| {
        if let Some(appbar) = appbars.borrow_mut().get_mut(&(window as isize)) {
            let hover = APPBAR_STATE.with(|states| {
                states
                    .borrow()
                    .get(&(window as isize))
                    .and_then(|state| state.0)
            });
            let rows = if visible {
                pane_rows(window)
            } else {
                Vec::new()
            };
            if let Err(error) = appbar.update_rows(&rows, dpi) {
                log(&format!("rivet: could not draw the page rows: {error}"));
            }
            // A repaint asked for from inside the bar's own work waits for the next one.
            if !crate::features::address_bar::busy() {
                let top_bar = if visible {
                    crate::features::address_bar::view(window)
                } else {
                    None
                };
                if let Err(error) = appbar.update_address(window as isize, top_bar.as_ref(), dpi) {
                    log(&format!("rivet: could not draw the top bar: {error}"));
                }
            }
            let preview = if visible {
                crate::features::link_preview::view(window)
            } else {
                None
            };
            if let Err(error) = appbar.update_preview(preview.as_ref(), dpi) {
                log(&format!("rivet: could not draw the link preview: {error}"));
            }
            let rules = if visible {
                crate::features::link_rules::view(window)
            } else {
                None
            };
            if let Err(error) = appbar.update_link_rules(rules.as_ref(), dpi) {
                log(&format!("rivet: could not draw the link rules: {error}"));
            }
            if !crate::features::command_bar::busy() {
                let command = if visible {
                    crate::features::command_bar::view(window)
                } else {
                    None
                };
                if let Err(error) = appbar.update_command(window as isize, command.as_ref(), dpi) {
                    log(&format!("rivet: could not draw the command bar: {error}"));
                }
            }
            if !crate::features::settings::busy() {
                let card = if visible {
                    crate::features::settings::view(window)
                } else {
                    None
                };
                if let Err(error) = appbar.update_settings(window as isize, card.as_ref(), dpi) {
                    log(&format!(
                        "rivet: could not draw the Rivet settings: {error}"
                    ));
                }
            }
            let bar = if visible {
                crate::dev_bar::view(window)
            } else {
                None
            };
            if let Err(error) = appbar.update_dev(bar.as_ref(), dpi) {
                log(&format!("rivet: could not draw the developer bar: {error}"));
            }
            // The warning keeps clear of the corner where the Sidebar's open button shows, which is
            // the lower right corner of the page area, or of the window when no page area is known.
            let per_dip = dpi as f32 / 96.0;
            let (page, open_zone) =
                crate::side_rail::page_and_open_zone(window).unwrap_or_else(|| {
                    let (w, h) = (
                        client.right as f32 / per_dip,
                        client.bottom as f32 / per_dip,
                    );
                    ([0.0, 0.0, w, h], crate::side_rail::open_zone_at(w, h))
                });
            if let Err(error) =
                appbar.update_held(held_warning().as_deref(), page, Some(open_zone), dpi)
            {
                log(&format!(
                    "rivet: could not draw the held build warning: {error}"
                ));
            }
            let (spaces, current) =
                with_spaces(window, |spaces| (spaces.list().to_vec(), spaces.current()));
            appbar.set_state(
                appbar_model(window),
                &spaces,
                current,
                hover,
                press_effects(window),
                open_menu_kind(window),
            );
            appbar.set_layout(!layout.collapsed, dragging(window));
            let height = client.bottom - top;
            if let Err(error) = appbar.update(visible, 0, top, width, height, dpi) {
                log(&format!("rivet: could not draw the appbar: {error}"));
            }
            appbar.set_dark_row(crate::features::dev_page::window_in_dev_mode(window));
            let (x, y, size) = {
                let (x, y, w, _) = toggle_rect(page_x(window));
                (scale(x, dpi), scale(y, dpi), scale(w, dpi))
            };
            if let Err(error) = appbar.update_toggle(visible, x, y, size, dpi) {
                log(&format!("rivet: could not draw the toggle: {error}"));
            }
            let slide = slide_of(window);
            let eased = 1.0 - (1.0 - slide).powi(3);
            let floating = (height as f32 * 96.0 / dpi as f32) as i32 - 2 * FLOAT_INSET;
            if let Err(error) = appbar.update_peek(
                visible && layout.collapsed,
                top,
                layout.side,
                floating,
                dpi,
                eased,
            ) {
                log(&format!(
                    "rivet: could not draw the floating appbar: {error}"
                ));
            }
            sync_ground(window, appbar, visible, dpi);
            crate::side_rail::update(window, appbar, visible, dpi);
        }
    });
}

/// The pages of a window as the holes of its ground: each page of a split with its top row, or
/// the one panel the browser laid out.
unsafe fn ground_holes(window: HWND) -> Vec<crate::ground::Hole> {
    use crate::ground::{Hole, Raise};
    let panes = crate::features::split::panes(window);
    if !panes.is_empty() {
        let pages: Vec<frame_layout::Rect> = panes
            .iter()
            .map(|pane| {
                // The hole runs from the page's row to its end, with any bars of the browser between.
                let y = pane.row_y + crate::features::split::ROW_HEIGHT;
                frame_layout::Rect {
                    x: pane.x,
                    y,
                    width: pane.width,
                    height: pane.y + pane.height - y,
                }
            })
            .collect();
        let active = panes.iter().position(|pane| pane.active).unwrap_or(0);
        let mut holes =
            crate::ground::split_holes(&pages, active, crate::features::split::ROW_HEIGHT);
        // The Sidebar's rail and the caption buttons sit at the window's right edge, in the panels
        // of the right-most pages as in a window that is not split.
        let right = panes
            .iter()
            .map(|pane| pane.x + pane.width)
            .max()
            .unwrap_or(0);
        if let Some(panel) = frame_layout::panel_bounds(window) {
            for (hole, pane) in holes.iter_mut().zip(&panes) {
                if pane.x + pane.width == right {
                    hole.rect.width = hole.rect.width.max(panel.x + panel.width - hole.rect.x);
                }
            }
        }
        return holes;
    }
    let leading = LAID_LEADING
        .with(|all| all.borrow().get(&(window as isize)).copied())
        .unwrap_or(0);
    frame_layout::panel_bounds(window)
        .map(|mut rect| {
            let cut = (leading - rect.x).max(0).min(rect.width - 1);
            rect.x += cut;
            rect.width -= cut;
            vec![Hole {
                rect,
                raise: Raise::Page,
            }]
        })
        .unwrap_or_default()
}

/// The holes of the ground, with their left edges kept right of the appbar while its width is
/// being dragged. The browser lays out only when the drag ends, so until then the page would
/// show through the wider appbar.
unsafe fn ground_holes_at(window: HWND) -> Vec<crate::ground::Hole> {
    let mut holes = ground_holes(window);
    let layout = layout_of(window);
    if dragging(window) && !layout.collapsed {
        for hole in &mut holes {
            let cut = (layout.side - hole.rect.x).max(0).min(hole.rect.width - 1);
            hole.rect.x += cut;
            hole.rect.width -= cut;
        }
    }
    holes
}

/// Draws the window's ground for the pages the browser has laid out.
unsafe fn sync_ground(window: HWND, appbar: &mut Appbar, visible: bool, dpi: u32) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let holes = ground_holes_at(window);
    let r = appbar.update_ground(visible, client.right, client.bottom, dpi, &holes);
    if let Err(error) = r {
        log(&format!("rivet: could not draw the ground: {error}"));
    }
}

/// The browser laid a window's page panel out somewhere new, so the ground's hole follows it.
fn panel_moved(window: HWND) {
    unsafe {
        let (visible, dpi) = (framed(window), layout_dpi(window));
        APPBARS.with(|appbars| {
            // A layout inside a redraw is followed by that redraw's own ground update.
            if let Ok(mut appbars) = appbars.try_borrow_mut() {
                if let Some(appbar) = appbars.get_mut(&(window as isize)) {
                    sync_ground(window, appbar, visible, dpi);
                }
            }
        });
    }
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
    /// Where each window's page began when the browser last laid it out. The client view reaches
    /// left under the browser's own tab strip, so this, not its edge, is where the page starts.
    static LAID_LEADING: RefCell<HashMap<isize, i32>> = RefCell::new(HashMap::new());
    /// The scale each browser window last laid itself out at, as a DPI.
    static LAYOUT_DPI: RefCell<HashMap<isize, u32>> = RefCell::new(HashMap::new());
    /// The scale the browser laid a window out at while its `WM_DPICHANGED` was being handled.
    static LAID_DURING_DPI_CHANGE: RefCell<HashMap<isize, u32>> = RefCell::new(HashMap::new());
}

/// The DPI the browser actually laid the window out at. While a window crosses monitors, the
/// browser and Windows can briefly disagree, and the appbar must match the browser's layout.
pub(crate) unsafe fn layout_dpi(window: HWND) -> u32 {
    LAYOUT_DPI
        .with(|layouts| layouts.borrow().get(&(window as isize)).copied())
        .unwrap_or_else(|| dpi(window))
}

/// The strip the browser's layout leaves free, in device-independent pixels. Also records the
/// scale of this layout, which is the browser's client width over its layout width.
fn reserved_width(window: HWND, bounds: frame_layout::Rect) -> Option<i32> {
    unsafe {
        if !attached(window) || !framed(window) {
            return None;
        }
        // The browser is laying the window out for Rivet's frame; the first such layout after the
        // attach lets the window show.
        laid_out(window);
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(window, &mut client);
        let layout_width = bounds.x + bounds.width;
        if layout_width > 0 && client.right > 0 {
            let dpi = layout_scale_dpi(client.right, layout_width);
            LAID_DURING_DPI_CHANGE.with(|laid| laid.borrow_mut().insert(window as isize, dpi));
            let changed = LAYOUT_DPI
                .with(|layouts| layouts.borrow_mut().insert(window as isize, dpi) != Some(dpi));
            if changed {
                crate::trace::trace(&format!(
                    "layout dpi: window {:#x} at {dpi}",
                    window as usize
                ));
                update_appbar(window);
            }
        }
        // The scale is recorded above for both tab layouts. The strip is only subtracted here.
        let leading = layout_of(window).leading();
        LAID_LEADING.with(|all| all.borrow_mut().insert(window as isize, leading));
        if let Some(strip) = crate::features::vertical_tabs::layout_width(window) {
            return Some(crate::reserved_for_strip(leading, strip));
        }
        Some(leading)
    }
}

pub(crate) fn log(message: &str) {
    crate::trace::trace(message);
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
    // The floating appbar and the toggle button belong to Rivet wherever the browser's own frame
    // would put them.
    if floating_at(window, point) || toggle_at(window, point).is_some() {
        return StripHit::Appbar;
    }
    let layout = layout_of(window);
    let dpi = layout_dpi(window);
    let border = if IsZoomed(window) != 0 {
        0
    } else {
        GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    };
    let strip = strip_hit(point.x, point.y, dpi, border, layout.strip());
    let factor = dpi as f32 / 96.0;
    // The split's own rows and the developer bar take their input from the client area.
    if strip == StripHit::Browser
        && row_hit(window, point).is_none()
        && crate::dev_bar::hit(window, point).is_none()
    {
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(window, &mut client);
        // The panel starts one gap below the client area's top edge, so the row is measured from it.
        return crate::title_row_hit(
            point.x as f32 / factor,
            point.y as f32 / factor,
            client.right as f32 / factor,
            point.y < border,
            layout.leading() as f32,
            IsZoomed(window) != 0,
        );
    }
    let x = point.x as f32 / factor;
    let y = (point.y - top_border(window)) as f32 / factor;
    let side = layout.side as f32;
    let on_caption_button = strip == StripHit::Caption
        && x >= side - 36.0
        && x < side - 8.0
        && (12.0..40.0).contains(&y);
    if on_caption_button || strip == StripHit::Resizer {
        StripHit::Appbar
    } else {
        strip
    }
}

unsafe extern "system" fn browser_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = handle(window, message, wparam, lparam);
    after_event(window, message, wparam);
    result
}

thread_local! {
    /// The windows whose retries ran out before the browser laid them out for Rivet's frame.
    static GAVE_UP: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    /// Set while Rivet asks a window again, so the ask's own messages do not ask once more.
    static ASKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a window event asks the browser again to lay the window out for Rivet's frame: only for
/// a window that still waits for that layout after its timed retries ran out, and only on an event
/// after which the browser lays out, a restore or size, a show or an activation.
fn ask_on_event(waiting: bool, gave_up: bool, message: u32, wparam: WPARAM) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SIZE_MINIMIZED, WM_ACTIVATE, WM_SHOWWINDOW, WM_SIZE,
    };
    if !(waiting && gave_up) {
        return false;
    }
    match message {
        WM_SIZE => wparam != SIZE_MINIMIZED as WPARAM,
        WM_SHOWWINDOW => wparam != 0,
        WM_ACTIVATE => wparam & 0xFFFF != 0,
        _ => false,
    }
}

/// After the window handled a message: asks again for the layout for Rivet's frame when the event
/// is one the browser lays out after, for a window whose timed retries ran out.
unsafe fn after_event(window: HWND, message: u32, wparam: WPARAM) {
    let gave_up = GAVE_UP.with(|all| all.borrow().contains(&(window as isize)));
    if ASKING.with(|asking| asking.get())
        || !ask_on_event(is_waiting(window), gave_up, message, wparam)
    {
        return;
    }
    crate::trace::trace(&format!(
        "attach: window {:#x} still not laid out for Rivet's frame; asked again on message {message:#x}",
        window as usize
    ));
    ASKING.with(|asking| asking.set(true));
    relayout(window);
    ASKING.with(|asking| asking.set(false));
}

unsafe fn handle(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(result) = crate::offscreen::message(window, message, wparam, lparam) {
        return result;
    }
    if message == RegisterWindowMessageW(wide("Rivet.NewStartPage").as_ptr()) && attached(window) {
        start_page(window);
        return 0;
    }
    if message == RegisterWindowMessageW(wide("Rivet.CtrlT").as_ptr()) && attached(window) {
        if !crate::features::tab_list::open_command_bar(window) {
            start_page(window);
        }
        return 0;
    }
    if message == RegisterWindowMessageW(wide("Rivet.ReopenTab").as_ptr()) && attached(window) {
        crate::features::tab_list::reopen(window);
        update_appbar(window);
        return 0;
    }
    if message
        == RegisterWindowMessageW(wide(crate::features::second_view::LAYOUT_MESSAGE).as_ptr())
        && attached(window)
    {
        crate::features::second_view::relayout(window);
        update_appbar(window);
        return 0;
    }
    if message == RegisterWindowMessageW(wide(crate::features::split::RELAYOUT_MESSAGE).as_ptr())
        && attached(window)
    {
        crate::hooks::page_area::relayout(window);
        update_appbar(window);
        return 0;
    }
    if message == RegisterWindowMessageW(wide("Rivet.ToggleSplit").as_ptr()) && attached(window) {
        crate::features::split::toggle_split(window);
        update_appbar(window);
        return 0;
    }
    if message == RegisterWindowMessageW(wide(rivet_protocol::window::MINI_WINDOW_MESSAGE).as_ptr())
        && attached(window)
    {
        crate::features::mini_window::command(window, wparam, lparam);
        update_appbar(window);
        return 0;
    }
    if message == crate::features::control::message() && attached(window) {
        crate::features::control::take(window, lparam);
        return 0;
    }
    if message == windows_sys::Win32::UI::WindowsAndMessaging::WM_COPYDATA && attached(window) {
        if let Some(result) = crate::features::link_rules::copy_data(window, lparam) {
            return result;
        }
    }
    // The command bar's scrim and the link preview's header lie over the pages, so they take the
    // pointer and the keyboard before the pages do.
    if attached(window) {
        if let Some(result) = crate::features::settings::message(window, message, wparam, lparam) {
            return result;
        }
        if let Some(result) = crate::features::link_rules::message(window, message, wparam, lparam)
        {
            return result;
        }
        if let Some(result) = crate::features::command_bar::message(window, message, wparam, lparam)
        {
            return result;
        }
        if let Some(result) =
            crate::features::link_preview::message(window, message, wparam, lparam)
        {
            return result;
        }
        if let Some(result) = crate::features::page_links::message(window, message, wparam, lparam)
        {
            return result;
        }
    }
    if attached(window) {
        // The second page view's divider and its keyboard focus take the mouse before the rest.
        let browser = || call_browser(window, message, wparam, lparam);
        if let Some(result) =
            crate::features::second_view::on_mouse(window, message, lparam, &browser)
        {
            return result;
        }
    }
    if message == toggle_message() && attached(window) {
        toggle_appbar(window);
        return 0;
    }
    // Rivet's top bar takes the pointer over it and, while its address is edited, the keyboard and
    // the input method.
    if attached(window) && !menu_open(window) && !tab_dragging(window) && !dragging(window) {
        if let Some(result) = crate::features::address_bar::message(window, message, wparam, lparam)
        {
            return result;
        }
    }
    if attached(window) && message == WM_MOUSEMOVE && !tab_dragging(window) {
        track_row_hover(
            window,
            Some(POINT {
                x: (lparam & 0xffff) as i16 as i32,
                y: ((lparam >> 16) & 0xffff) as i16 as i32,
            }),
        );
    }
    if attached(window) && message == WM_MOUSELEAVE {
        track_row_hover(window, None);
        if crate::dev_bar::track(window, None) {
            update_appbar(window);
        }
    }
    if attached(window) && message == WM_MOUSEMOVE && !tab_dragging(window) {
        let point = POINT {
            x: (lparam & 0xffff) as i16 as i32,
            y: ((lparam >> 16) & 0xffff) as i16 as i32,
        };
        if crate::dev_bar::track(window, Some(point)) {
            update_appbar(window);
        }
    }
    if message == RegisterWindowMessageW(wide("Rivet.ToggleDevMode").as_ptr()) && attached(window) {
        crate::features::dev_page::toggle_dev_mode(window);
        return 0;
    }
    match message {
        WM_LBUTTONDOWN
            if attached(window)
                && !menu_open(window)
                && crate::dev_bar::press(
                    window,
                    POINT {
                        x: (lparam & 0xffff) as i16 as i32,
                        y: ((lparam >> 16) & 0xffff) as i16 as i32,
                    },
                ) =>
        {
            0
        }
        WM_LBUTTONUP if attached(window) && crate::dev_bar::pressed(window) => {
            crate::dev_bar::release(
                window,
                POINT {
                    x: (lparam & 0xffff) as i16 as i32,
                    y: ((lparam >> 16) & 0xffff) as i16 as i32,
                },
            );
            0
        }
        WM_LBUTTONDOWN
            if attached(window)
                && !menu_open(window)
                && row_hit(
                    window,
                    POINT {
                        x: (lparam & 0xffff) as i16 as i32,
                        y: ((lparam >> 16) & 0xffff) as i16 as i32,
                    },
                )
                .is_some() =>
        {
            let hit = row_hit(
                window,
                POINT {
                    x: (lparam & 0xffff) as i16 as i32,
                    y: ((lparam >> 16) & 0xffff) as i16 as i32,
                },
            );
            if let Some(hit) = hit {
                ROW_PRESS.with(|all| all.borrow_mut().insert(window as isize, hit));
                // A grip drag may end anywhere, so its release comes back to this window.
                if hit.1 == crate::features::split::RowButton::Grip {
                    rivet_platform_windows::focus::capture(window);
                }
            }
            0
        }
        WM_LBUTTONUP
            if attached(window)
                && ROW_PRESS.with(|all| all.borrow().contains_key(&(window as isize))) =>
        {
            let pressed = ROW_PRESS.with(|all| all.borrow_mut().remove(&(window as isize)));
            let now = row_hit(
                window,
                POINT {
                    x: (lparam & 0xffff) as i16 as i32,
                    y: ((lparam >> 16) & 0xffff) as i16 as i32,
                },
            );
            let (x, y) = (
                (lparam & 0xffff) as i16 as i32,
                ((lparam >> 16) & 0xffff) as i16 as i32,
            );
            // A grip released outside the pages makes its page a Mini Window.
            let grip =
                pressed.filter(|pressed| pressed.1 == crate::features::split::RowButton::Grip);
            if grip.is_some() {
                rivet_platform_windows::focus::release(window);
            }
            if let Some(hit) =
                grip.filter(|hit| crate::features::mini_window::grip_released(window, hit.0, x, y))
            {
                crate::trace::trace(&format!(
                    "split row: grip of page {} dropped outside the pages",
                    hit.0
                ));
                update_appbar(window);
            } else if let Some(hit) = pressed.filter(|pressed| Some(*pressed) == now) {
                row_action(window, hit);
            }
            0
        }
        WM_MOUSEMOVE if attached(window) && tab_dragging(window) => {
            tab_drag_move(window, lparam);
            0
        }
        WM_LBUTTONUP if attached(window) && tab_dragging(window) => {
            tab_drag_end(window, Some(lparam));
            0
        }
        WM_CAPTURECHANGED | WM_KEYDOWN
            if attached(window)
                && tab_dragging(window)
                && (message == WM_CAPTURECHANGED || wparam == 0x1b) =>
        {
            tab_drag_end(window, None);
            0
        }
        WM_MOUSEMOVE if attached(window) && dragging(window) => {
            drag_to(window, (lparam & 0xffff) as i16 as i32);
            0
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED if attached(window) && dragging(window) => {
            end_drag(window);
            0
        }
        WM_SETCURSOR
            if attached(window)
                && (dragging(window)
                    || (lparam & 0xffff) as u32 == APPBAR_HIT
                        && APPBAR_STATE.with(|states| {
                            states
                                .borrow()
                                .get(&(window as isize))
                                .is_some_and(|state| state.0 == Some(AppbarHit::Resizer))
                        })) =>
        {
            SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_SIZEWE));
            1
        }
        WM_MOUSEMOVE if attached(window) && layout_of(window).collapsed && !menu_open(window) => {
            track_pointer(
                window,
                (lparam & 0xffff) as i16 as i32,
                ((lparam >> 16) & 0xffff) as i16 as i32,
            );
            call_browser(window, message, wparam, lparam)
        }
        WM_MOUSELEAVE if attached(window) && layout_of(window).collapsed => {
            check_peek(window, true);
            call_browser(window, message, wparam, lparam)
        }
        WM_KEYDOWN if attached(window) && menu_open(window) && wparam == 0x1b => {
            close_menu(window);
            update_appbar(window);
            0
        }
        WM_CAPTURECHANGED if attached(window) && menu_open(window) => {
            close_menu(window);
            update_appbar(window);
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_RBUTTONDOWN
            if attached(window) && menu_open(window) =>
        {
            close_menu(window);
            update_appbar(window);
            0
        }
        WM_LBUTTONDOWN if attached(window) && menu_open(window) => {
            let (inside, hit) = menu_pointer(window, lparam);
            if !inside {
                close_menu(window);
            } else {
                APPBAR_STATE.with(|states| {
                    if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
                        entry.1 = hit;
                    }
                });
                press_down(window, hit);
            }
            update_appbar(window);
            0
        }
        WM_LBUTTONUP if attached(window) && menu_open(window) => {
            let (_, hit) = menu_pointer(window, lparam);
            let action = APPBAR_STATE.with(|states| {
                let mut states = states.borrow_mut();
                let entry = states.get_mut(&(window as isize)).unwrap();
                let action = if entry.1 == hit { hit } else { None };
                entry.1 = None;
                action
            });
            press_up(window);
            if let Some(hit) = action {
                appbar_action(window, hit);
            }
            update_appbar(window);
            0
        }
        WM_MOUSEMOVE if attached(window) && menu_open(window) => {
            let (_, hit) = menu_pointer(window, lparam);
            let left_pressed = APPBAR_STATE.with(|states| {
                let mut states = states.borrow_mut();
                let Some(entry) = states.get_mut(&(window as isize)) else {
                    return false;
                };
                entry.0 = hit;
                entry.1.is_some() && entry.1 != hit
            });
            if left_pressed {
                press_up(window);
            }
            update_appbar(window);
            0
        }
        WM_NCMOUSEMOVE if wparam as u32 == APPBAR_HIT && attached(window) => {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: window,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tracking);
            tab_drag_start(window, lparam);
            let hit = appbar_point(window, lparam);
            let left_pressed = APPBAR_STATE.with(|states| {
                let mut all = states.borrow_mut();
                let entry = all.entry(window as isize).or_insert((None, None, 0xd08a52));
                entry.0 = hit;
                entry.1.is_some() && entry.1 != hit && !dragging(window)
            });
            // The pointer left the control it went down on, which lets the control spring back.
            if left_pressed {
                press_up(window);
            }
            update_appbar(window);
            0
        }
        WM_NCMOUSELEAVE if attached(window) => {
            CAPTION_STATE.with(|all| all.borrow_mut().remove(&(window as isize)));
            if !dragging(window) {
                press_up(window);
                APPBAR_STATE.with(|states| {
                    if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
                        entry.0 = None;
                    }
                });
            }
            update_appbar(window);
            check_peek(window, true);
            0
        }
        // Other parts of the window clear a hover left on the appbar and show a hidden appbar
        // when the pointer reaches the window's left edge.
        WM_NCMOUSEMOVE if attached(window) && !dragging(window) => {
            let stale = APPBAR_STATE.with(|states| {
                states
                    .borrow_mut()
                    .get_mut(&(window as isize))
                    .is_some_and(|entry| entry.0.take().is_some())
            });
            let point = client_point(window, lparam);
            track_pointer(window, point.x, point.y);
            let caption = crate::caption_of(wparam as u32);
            let moved = set_caption_hover(window, caption);
            if stale || moved {
                update_appbar(window);
            }
            if caption.is_some() {
                // Rivet draws these buttons, so the browser never sees the pointer over them, and
                // the leave notice clears their hover.
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: window,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tracking);
                return 0;
            }
            call_browser(window, message, wparam, lparam)
        }
        // Rivet's caption buttons act on release over the button they went down on, as Windows' own.
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK
            if attached(window) && crate::caption_of(wparam as u32).is_some() =>
        {
            CAPTION_STATE.with(|all| {
                all.borrow_mut().entry(window as isize).or_default().1 =
                    crate::caption_of(wparam as u32)
            });
            0
        }
        WM_NCLBUTTONUP if attached(window) && crate::caption_of(wparam as u32).is_some() => {
            let button = crate::caption_of(wparam as u32);
            let pressed = CAPTION_STATE.with(|all| {
                all.borrow_mut()
                    .entry(window as isize)
                    .or_default()
                    .1
                    .take()
            });
            if let Some(button) = button.filter(|button| pressed == Some(*button)) {
                PostMessageW(
                    window,
                    WM_SYSCOMMAND,
                    crate::caption_command(button, IsZoomed(window) != 0) as usize,
                    lparam,
                );
            }
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
            tab_drag_press(window, hit, lparam);
            if hit == Some(AppbarHit::Resizer) {
                DRAGGING.with(|set| set.borrow_mut().insert(window as isize));
                rivet_platform_windows::focus::capture(window);
            }
            press_down(window, hit);
            update_appbar(window);
            0
        }
        WM_NCLBUTTONUP if wparam as u32 == APPBAR_HIT && attached(window) => {
            TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize)));
            crate::features::link_preview::cancel_hold(window);
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
            press_up(window);
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
            StripHit::Appbar | StripHit::Resizer => APPBAR_HIT as LRESULT,
            StripHit::Minimize => HTMINBUTTON as LRESULT,
            StripHit::Maximize => HTMAXBUTTON as LRESULT,
            StripHit::Close => HTCLOSE as LRESULT,
        },
        // Clicks on the appbar's content never reach the browser, which would otherwise see
        // non-client input it did not lay out.
        WM_NCLBUTTONDBLCLK | WM_NCRBUTTONDOWN | WM_NCRBUTTONUP
            if wparam as u32 == APPBAR_HIT && attached(window) =>
        {
            if message == WM_NCRBUTTONUP {
                open_tab_menu(window, lparam);
            }
            if message == WM_NCLBUTTONDBLCLK
                && appbar_point(window, lparam) == Some(AppbarHit::Resizer)
            {
                end_drag(window);
                if edit_layout(window, |layout| layout.set_width(APPBAR_WIDTH)) {
                    sync_strip(window);
                    relayout(window);
                }
            }
            0
        }
        WM_WINDOWPOSCHANGED if attached(window) => {
            let result = call_browser(window, message, wparam, lparam);
            // Every browser sends this on a resize, so the second view follows it on all of them.
            follow_second_view(window);
            update_appbar(window);
            crate::features::window_corners::keep_round(window);
            result
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER
            if wparam == SECOND_VIEW_TIMER && attached(window) =>
        {
            let moved = crate::features::second_view::relayout(window);
            let idle = if moved {
                SECOND_VIEW_IDLE.store(0, std::sync::atomic::Ordering::Relaxed);
                0
            } else {
                SECOND_VIEW_IDLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
            };
            // A page view whose bounds changes are not hooked is checked as long as the view is
            // open, since the browser may lay it out again at any time and undo the narrowing.
            if crate::features::second_view::may_stop_following(
                crate::features::second_view::is_open(window),
                crate::features::second_view::follows_bounds(window),
                idle,
            ) {
                KillTimer(window, SECOND_VIEW_TIMER);
            }
            if moved {
                update_appbar(window);
            }
            0
        }
        // The browser sets its own or the page's icon. A Rivet window keeps Rivet's.
        WM_SETICON if attached(window) => match rivet_icon(window, wparam) {
            Some(icon) => call_browser(window, message, wparam, icon),
            None => call_browser(window, message, wparam, lparam),
        },
        WM_DPICHANGED if attached(window) => {
            LAID_DURING_DPI_CHANGE.with(|laid| laid.borrow_mut().remove(&(window as isize)));
            let result = call_browser(window, message, wparam, lparam);
            // One scale serves the appbar, the address bar and the page offset. A browser that did
            // not lay out during the change still moved to the new monitor's DPI, so the old
            // monitor's scale is never kept.
            let laid =
                LAID_DURING_DPI_CHANGE.with(|laid| laid.borrow_mut().remove(&(window as isize)));
            let now = dpi_after_change(laid, (wparam & 0xffff) as u32);
            LAYOUT_DPI.with(|layouts| layouts.borrow_mut().insert(window as isize, now));
            // The browser relays out inside this call and reports its scale through the layout
            // hook. Repaint so the appbar follows it.
            update_appbar(window);
            set_rivet_icons(window);
            result
        }
        WM_CLOSE if attached(window) => {
            persist_closing(window);
            call_browser(window, message, wparam, lparam)
        }
        WM_NCDESTROY => {
            PERSIST.with(|persist| {
                let mut persist = persist.borrow_mut();
                if persist.owner == window as isize {
                    persist.owner = 0;
                }
            });
            if attached(window) {
                taskbar::clear(window as isize);
            }
            close_menu(window);
            KillTimer(window, PEEK_TIMER);
            KillTimer(window, SECOND_VIEW_TIMER);
            crate::features::second_view::forget(window);
            crate::features::sidebar_page::forget(window);
            crate::features::pane_infobar::forget(window);
            LAYOUTS.with(|layouts| layouts.borrow_mut().remove(&(window as isize)));
            SLIDE.with(|slides| slides.borrow_mut().remove(&(window as isize)));
            DRAGGING.with(|set| set.borrow_mut().remove(&(window as isize)));
            TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize)));
            KillTimer(window, PRESS_TIMER);
            SPACES.with(|all| all.borrow_mut().remove(&(window as isize)));
            PRESSES.with(|all| all.borrow_mut().remove(&(window as isize)));
            APPBARS.with(|appbars| appbars.borrow_mut().remove(&(window as isize)));
            TICK_GATE.with(|gate| gate.borrow_mut().forget(window as isize));
            crate::features::address_bar::forget(window);
            crate::features::command_bar::forget(window);
            crate::features::settings::forget(window);
            crate::features::link_rules::forget(window);
            crate::features::link_preview::forget(window);
            crate::features::page_links::forget(window);
            APPBAR_STATE.with(|states| states.borrow_mut().remove(&(window as isize)));
            crate::features::vertical_tabs::forget(window);
            crate::features::tab_strip::forget(window);
            frame_layout::forget_view(window);
            crate::features::widgets::forget(window);
            crate::side_rail::forget(window);
            crate::features::mini_window::forget(window);
            LAYOUT_DPI.with(|layouts| layouts.borrow_mut().remove(&(window as isize)));
            let key = wide(ORIGINAL_PROC);
            let original = GetPropW(window, key.as_ptr());
            SetWindowLongPtrW(window, GWLP_WNDPROC, original as isize);
            RemovePropW(window, key.as_ptr());
            let original: WNDPROC = std::mem::transmute(original);
            CallWindowProcW(original, window, message, wparam, lparam)
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER
            if crate::features::look::on_timer(window, wparam) =>
        {
            // The reveal came from the longest hide while the window still waited for the browser
            // to lay it out for Rivet's frame: the window showed before Rivet's look was in place.
            if take_waiting(window) {
                crate::trace::trace(&format!(
                    "first look: warning, window {:#x} shown after the longest hide of {} ms \
                     without the browser's layout for Rivet's frame",
                    window as usize,
                    crate::features::look::LONGEST_HIDE_MS
                ));
            }
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER if wparam == ATTACH_LAYOUT_TIMER => {
            on_attach_layout_timer(window);
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER if wparam == OPEN_LATER_TIMER => {
            windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(window, OPEN_LATER_TIMER);
            let due: Vec<_> = OPEN_LATER.with(|later| {
                let mut later = later.borrow_mut();
                let (mine, rest): (Vec<_>, Vec<_>) = later
                    .drain(..)
                    .partition(|(owner, ..)| *owner == window as isize);
                *later = rest;
                mine
            });
            for (_, contents, url) in due {
                let opened = crate::features::navigation::open_if_active(window, contents, &url);
                crate::trace::trace(&format!(
                    "new tab in a Rivet window opens the start page later: {opened}"
                ));
            }
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER
            if wparam == PEEK_TIMER && attached(window) =>
        {
            peek_tick(window);
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER
            if wparam == PRESS_TIMER && attached(window) =>
        {
            press_tick(window);
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER
            if wparam == TAB_REFRESH_TIMER && attached(window) =>
        {
            use crate::features::activity::{add_since, clock, count, timed, Counter};
            let tick_started = clock();
            count(Counter::TabTick);
            let tick = TAB_TICKS.with(|ticks| {
                ticks.set(ticks.get() + 1);
                ticks.get()
            });
            if debug_switches().is_some() {
                count(Counter::DebugScan);
            }
            timed(Counter::DebugMicros, || {
                crate::features::side_panel::tick(window, debug_switches())
            });
            timed(Counter::ListMicros, || {
                crate::features::tab_list::tick(window)
            });
            timed(Counter::FollowMicros, || {
                crate::features::command_bar::tick(window);
                // An extension page may load into the browser's side panel some time after the
                // click that opened it, after the side rail's pointer timer has gone quiet.
                crate::features::sidebar_page::follow(window);
            });
            // A browser can write its own identity again, for example when a profile finishes
            // loading. The taskbar groups by that identity, so Rivet puts its own back. Reading the
            // identity goes through the shell's property store, so it is not done on every tick.
            let taskbar_started = clock();
            if taskbar_check_due(tick) {
                count(Counter::TaskbarCheck);
                if taskbar::current_id(window as isize).as_deref() != Some(taskbar::APP_ID) {
                    crate::trace::trace("taskbar identity was replaced; applying Rivet's again");
                    let _ = taskbar::apply(window as isize, runtime_directory().as_deref());
                    set_rivet_icons(window);
                }
            }
            add_since(Counter::TaskbarMicros, taskbar_started);
            crate::features::activity::report_on_tick();
            let debug_started = clock();
            if let Some(flag) = debug_switches().map(|dir| dir.join("rivet-split-now.flag")) {
                if std::fs::remove_file(&flag).is_ok() {
                    crate::features::split::probe(window);
                }
            }
            // A checked build runs one tab command from `rivet-tab.txt` without a pointer: `new [ADDRESS]`,
            // `activate N`, `close N` or `navigate ADDRESS`, and traces the tab list afterwards.
            if let Some(flag) = debug_switches().map(|dir| dir.join("rivet-tab.txt")) {
                if let Ok(text) = std::fs::read_to_string(&flag) {
                    let _ = std::fs::remove_file(&flag);
                    let mut words = text.split_whitespace();
                    let index = words
                        .clone()
                        .nth(1)
                        .and_then(|word| word.parse::<usize>().ok())
                        .unwrap_or(0);
                    let done = match words.next() {
                        // `new ADDRESS` opens the tab on that address at once, as the start page does, so the
                        // browser's own new tab page is never shown in the app window first.
                        Some("new") => {
                            let address = text.trim().trim_start_matches("new").trim();
                            crate::features::tabs::new_tab(window)
                                && (address.is_empty()
                                    || crate::features::navigation::open(window, address))
                        }
                        Some("activate") => crate::features::tabs::activate_tab(window, index),
                        Some("close") => crate::features::tabs::close_tab(window, index),
                        Some("closesel") => crate::features::tab_list::close(window, index),
                        // App window spike: a tab added through Chromium's own tab strip functions.
                        Some(word @ ("back" | "forward" | "reload" | "stop")) => {
                            crate::features::page_bar::parse_command(word).is_some_and(|command| {
                                crate::features::page_bar::run(
                                    &OS,
                                    &BROWSER,
                                    crate::os::WindowId::from_hwnd(window),
                                    command,
                                )
                            })
                        }
                        Some("pages") => {
                            let pages = crate::features::page_bar::pages(
                                &BROWSER,
                                crate::os::WindowId::from_hwnd(window),
                            )
                            .unwrap_or_default();
                            for page in &pages {
                                crate::trace::trace(&format!(
                                    "page {} active {} title {:?} url {:?}",
                                    page.index, page.active, page.title, page.url
                                ));
                            }
                            !pages.is_empty()
                        }
                        Some("viewclose") => crate::features::second_view::close(window),
                        Some("viewfocus") => crate::features::second_view::focus(window),
                        Some("share") => {
                            crate::features::second_view::set_share(window, index as u32)
                        }
                        Some("tree") => crate::features::app_window::view_tree(window),
                        Some("dump") => crate::features::app_window::dump_small(window),
                        Some("scan") => crate::features::app_window::scan_type(window),
                        Some("flip") => {
                            let offsets: Vec<usize> = text
                                .split_whitespace()
                                .skip(1)
                                .filter_map(|word| word.parse().ok())
                                .collect();
                            crate::features::app_window::flip_new_tab(window, &offsets)
                        }
                        Some("view") => {
                            crate::features::second_view::open_from_other_window(window)
                        }
                        // The Sidebar: `dock N` shows tab N's page in the second view docked at the
                        // right, `undock` gives it back to its tab.
                        Some("dock") => crate::features::second_view::open_tab(
                            window,
                            index,
                            crate::features::second_view::Mode::Docked {
                                width: crate::features::second_view::SIDEBAR_WIDTH,
                            },
                        ),
                        Some("undock") => {
                            crate::features::second_view::is_docked(window)
                                && crate::features::second_view::close(window)
                        }
                        // The top bar: `bar` starts editing the address, `bartext` traces what it holds,
                        // `barsecurity` the page's security level and `barinfo` opens the page info bubble.
                        Some("bar") => crate::features::address_bar::focus(window),
                        // `reflect` finds the window's views by their class metadata, reading memory
                        // only, and traces what it found beside what this build's row says.
                        Some("reflect") => trace_reflection(window),
                        // `focuswindow`: a row that needs a focused window, on a test desktop only.
                        // `focuswindow popup` focuses the window's top owned window instead, such as
                        // the browser's password save offer, so its default button takes Enter.
                        Some("focuswindow") => {
                            let popup = words.next() == Some("popup");
                            let done = crate::offscreen::focus_for_row(window, popup);
                            crate::trace::trace(&format!("activate on the test desktop: {done}"));
                            done
                        }
                        // The command bar: `command` opens it, `commandtype TEXT` puts text in its
                        // field, `commandrun N` runs row N. The link preview: `preview N` shows tab N
                        // floating, `previewopen` opens it as a tab and `previewesc` dismisses it.
                        // Rivet settings: `settings` opens the card, `settingskey KEY [TEXT]` presses
                        // up, down, left, right, enter, esc or tab on it, or `type` puts TEXT in the
                        // row being edited.
                        Some("settings") => crate::features::settings::open(window),
                        Some("settingskey") => {
                            let rest = text.trim().trim_start_matches("settingskey").trim();
                            let (key, typed) = rest.split_once(' ').unwrap_or((rest, ""));
                            crate::features::settings::key_for_test(window, key, typed)
                        }
                        Some("command") => crate::features::command_bar::open(window),
                        Some("commandtype") => {
                            let text = text.trim().trim_start_matches("commandtype").trim();
                            crate::features::command_bar::type_for_test(window, text)
                        }
                        Some("commandrun") => {
                            crate::features::command_bar::run_for_test(window, index)
                        }
                        Some("preview") => crate::features::link_preview::open(window, index),
                        // Link rules: `linkrules` opens the panel, `linkrulesadd` adds the page's
                        // site, `linkrulestarget N` and `linkrulesremove N` click rule N, and
                        // `linkopen ADDRESS` routes an address as `rivet session open` does.
                        Some("linkrules") => crate::features::link_rules::open(window),
                        Some(word @ ("linkrulesadd" | "linkrulestarget" | "linkrulesremove")) => {
                            use crate::features::link_rules::{click_for_test, describe, Hit};
                            let hit = match word {
                                "linkrulesadd" => Hit::AddSite,
                                "linkrulestarget" => Hit::Target(index),
                                _ => Hit::Remove(index),
                            };
                            let done = click_for_test(window, hit);
                            crate::trace::trace(&format!("link rules now [{}]", describe(window)));
                            done
                        }
                        Some("linkopen") => {
                            let address = text.trim().trim_start_matches("linkopen").trim();
                            crate::features::link_rules::open_request(window, address)
                        }
                        Some("previewopen") => crate::features::link_preview::open_as_tab(window),
                        Some("previewesc") => crate::features::link_preview::dismiss(window),
                        Some("bartext") => {
                            crate::trace::trace(&format!(
                                "address bar text {:?}",
                                crate::features::address_bar::text(window)
                            ));
                            true
                        }
                        Some("barsecurity") => {
                            crate::trace::trace(&format!(
                                "address bar security {:?}",
                                crate::features::omnibox::security_level(window)
                            ));
                            true
                        }
                        Some("barinfo") => crate::features::omnibox::page_info(window),
                        Some("barsuggest") => {
                            let text = text.trim().trim_start_matches("barsuggest").trim();
                            let found = crate::features::omnibox::suggest(window, text);
                            crate::trace::trace(&format!("address bar suggestions {found:?}"));
                            found.is_some()
                        }
                        // `baranchors` traces where the address and its security chip sit, so a
                        // test can click the chip; `baredit copy|cut|paste|undo|selectall` edits the
                        // field through the text engine as the keyboard shortcuts do; `devactions`
                        // traces the developer row's buttons.
                        Some("baranchors") => {
                            let anchors = crate::features::address_bar::anchors(window);
                            crate::trace::trace(&format!(
                                "address bar anchors {anchors:?} dpi {}",
                                layout_dpi(window)
                            ));
                            anchors.is_some()
                        }
                        Some("baredit") => {
                            let word = text.trim().trim_start_matches("baredit").trim();
                            crate::features::address_bar::edit_for_test(window, word)
                        }
                        Some("devactions") => {
                            let actions: Vec<_> = crate::features::dev_page::dev_actions()
                                .into_iter()
                                .map(|(action, _)| action)
                                .collect();
                            crate::trace::trace(&format!(
                                "dev mode {} actions {actions:?}",
                                crate::features::dev_page::window_in_dev_mode(window)
                            ));
                            true
                        }
                        Some("barimedone") => {
                            crate::features::address_bar::finish_composition_for_test(window)
                        }
                        Some("barime") => {
                            let text = text.trim().trim_start_matches("barime").trim();
                            crate::features::address_bar::compose_for_test(window, text)
                        }
                        Some("barclassify") => {
                            let text = text.trim().trim_start_matches("barclassify").trim();
                            crate::trace::trace(&format!(
                                "address bar classify {:?}",
                                crate::features::omnibox::classify(window, text)
                            ));
                            true
                        }
                        Some("move") => crate::features::app_window::move_tab_in(window),
                        Some("add") => {
                            let address = text.trim().trim_start_matches("add").trim();
                            crate::features::app_window::add_tab(window, address)
                        }
                        // Opens an address in the active page. A browser whose hand-off from a second
                        // process opens a window of its own has no other way to give a window two pages.
                        Some("navigate") => {
                            let address = text.trim().trim_start_matches("navigate").trim();
                            crate::features::navigation::open(window, address)
                        }
                        _ => crate::features::tab_list::command(window, &text).unwrap_or(false),
                    };
                    let titles: Vec<String> = crate::features::tabs::tabs(window)
                        .map(|tabs| {
                            tabs.iter()
                                .map(|tab| {
                                    format!("{}{}", if tab.active { "*" } else { "" }, tab.title)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    crate::trace::trace(&format!(
                        "tab command {:?}: {done} tabs {titles:?}",
                        text.trim()
                    ));
                    let urls: Vec<String> = crate::features::tabs::open_tabs(window)
                        .map(|tabs| tabs.into_iter().map(|tab| tab.saved.url).collect())
                        .unwrap_or_default();
                    crate::trace::trace(&format!("tab urls {urls:?}"));
                }
            }
            // A checked build drops the first inactive tab on a side of the page without a pointer.
            if let Some(flag) = debug_switches().map(|dir| dir.join("rivet-drop.flag")) {
                if let Ok(text) = std::fs::read_to_string(&flag) {
                    let _ = std::fs::remove_file(&flag);
                    use crate::features::split::Side;
                    // The flag reads a side, then optionally the slot of the page to split.
                    let mut words = text.split_whitespace();
                    let side = match words.next().unwrap_or("") {
                        "left" => Side::Left,
                        "top" => Side::Top,
                        "bottom" => Side::Bottom,
                        _ => Side::Right,
                    };
                    let slot = words.next().and_then(|word| word.parse::<usize>().ok());
                    // The first tab that is neither active nor shown in the split yet.
                    let shown = crate::features::split::shown_tabs(window);
                    let tabs = crate::features::tabs::tabs(window).unwrap_or_default();
                    let other = tabs
                        .iter()
                        .find(|tab| !tab.active && !shown.contains(&tab.index))
                        .map(|tab| tab.index);
                    let done = other.is_some_and(|index| {
                        crate::features::split::split_beside_page(window, index, side, slot)
                    });
                    let pages = crate::features::split::panes(window).len();
                    crate::trace::trace(&format!(
                        "drop flag: {side:?} of {slot:?} {other:?} {done} pages {pages}"
                    ));
                }
            }
            add_since(Counter::DebugMicros, debug_started);
            timed(Counter::PersistMicros, || persist_tick(window));
            if timed(Counter::GateMicros, || tick_needs_draw(window)) {
                timed(Counter::DrawMicros, || update_appbar(window));
            }
            add_since(Counter::TickMicros, tick_started);
            0
        }
        _ if message == group_probe_message() && attached(window) => {
            if debug_directory().is_some_and(|dir| dir.join("rivet-split.flag").exists()) {
                crate::features::split::probe(window);
            } else {
                crate::features::tabs::probe_group(window);
            }
            update_appbar(window);
            0
        }
        _ if message == vertical_tabs_message() => {
            crate::features::vertical_tabs::offer(window, wparam);
            0
        }
        _ if message == attach_message() => {
            // A test run's windows stay off every monitor and inactive from here on, the browser's
            // new windows and bubbles among them.
            crate::offscreen::keep_thread();
            crate::offscreen::put_away(window);
            crate::features::look::first_look(window, crate::features::look::FirstLook::UiThread);
            await_layout(window);
            // The browser may not lay the window out for Rivet's frame, as when it was minimized at
            // the attach; it is asked again until it does.
            // Temporary probe: RIVET_PROBE_NO_ATTACH_RETRY turns the timed retry off, to measure
            // whether it causes Whale windows to stop laying out. Removed with the fix.
            if std::env::var_os("RIVET_PROBE_NO_ATTACH_RETRY").is_none() {
                SetTimer(window, ATTACH_LAYOUT_TIMER, ATTACH_LAYOUT_RETRY_MS, None);
            }
            crate::features::tab_list::connect_command_bar();
            create_appbar(window);
            crate::features::window_corners::keep_round(window);
            0
        }
        _ => call_browser(window, message, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fr2_an_attach_asks_the_browser_again_until_it_lays_out_or_gives_up() {
        use super::{attach_retry, AttachRetry, ATTACH_LAYOUT_TRIES};
        assert_eq!(attach_retry(false, 0), AttachRetry::Done);
        assert_eq!(attach_retry(true, 0), AttachRetry::AskAgain(1));
        assert_eq!(
            attach_retry(true, ATTACH_LAYOUT_TRIES - 1),
            AttachRetry::AskAgain(ATTACH_LAYOUT_TRIES)
        );
        assert_eq!(attach_retry(true, ATTACH_LAYOUT_TRIES), AttachRetry::GiveUp);
        // Every try comes before the longest hide shows the window.
        const {
            assert!(
                super::ATTACH_LAYOUT_RETRY_MS * (super::ATTACH_LAYOUT_TRIES + 1)
                    <= crate::features::look::LONGEST_HIDE_MS
            )
        };
    }

    #[test]
    fn fr2_after_the_retries_a_restore_asks_the_browser_again() {
        use super::ask_on_event;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SIZE_MINIMIZED, SIZE_RESTORED, WM_ACTIVATE, WM_MOVE, WM_SHOWWINDOW, WM_SIZE,
        };
        // A restore, a show and an activation of a window that gave up ask again.
        assert!(ask_on_event(true, true, WM_SIZE, SIZE_RESTORED as usize));
        assert!(ask_on_event(true, true, WM_SHOWWINDOW, 1));
        assert!(ask_on_event(true, true, WM_ACTIVATE, 1));
        // Minimizing, hiding, deactivating and other messages do not.
        assert!(!ask_on_event(true, true, WM_SIZE, SIZE_MINIMIZED as usize));
        assert!(!ask_on_event(true, true, WM_SHOWWINDOW, 0));
        assert!(!ask_on_event(true, true, WM_ACTIVATE, 0));
        assert!(!ask_on_event(true, true, WM_MOVE, 0));
        // While the timed retries run, or once the layout came, events ask nothing.
        assert!(!ask_on_event(true, false, WM_SIZE, SIZE_RESTORED as usize));
        assert!(!ask_on_event(false, true, WM_SIZE, SIZE_RESTORED as usize));
    }

    #[test]
    fn fr12_only_the_first_layout_after_the_attach_starts_the_reveal() {
        let (window, other) = (0x1000 as super::HWND, 0x2000 as super::HWND);
        assert!(
            !super::take_waiting(window),
            "a layout before the attach changes nothing"
        );
        super::await_layout(window);
        assert!(!super::take_waiting(other));
        assert!(super::take_waiting(window));
        assert!(!super::take_waiting(window), "later layouts change nothing");
    }

    /// Windows and the browser can disagree on a window's scale: while the window crosses monitors
    /// of different DPIs, or when it lies off every monitor. Rivet's bar, rows, second page view and
    /// hit areas sit on the browser's layout, so every conversion between pixels and DIPs takes the
    /// scale the browser laid the window out at, never the DPI Windows reports.
    #[test]
    fn fr2_rivet_converts_pixels_at_the_browsers_layout_scale() {
        fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        let mut readers = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let calls = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .filter(|line| line.contains("GetDpiForWindow("))
                .count();
            if calls > 0 {
                readers.push(file.strip_prefix(&root).unwrap().display().to_string());
            }
        }
        // `frame::dpi` reads the window's DPI once, for a window the browser has not laid out yet.
        assert_eq!(readers, ["frame.rs"]);
    }

    #[test]
    fn idle_timer_looks_for_no_debug_files_without_a_debug_folder() {
        use std::path::{Path, PathBuf};
        let own = Some(PathBuf::from("own"));
        let shared = Some(PathBuf::from("shared"));
        let none = |_: &Path| false;
        assert_eq!(
            super::pick_debug_switches(own.clone(), shared.clone(), none),
            None
        );
        let only_shared = |dir: &Path| dir == Path::new("shared");
        assert_eq!(
            super::pick_debug_switches(own.clone(), shared.clone(), only_shared),
            shared
        );
        assert_eq!(
            super::pick_debug_switches(own.clone(), shared, |_: &Path| true),
            own
        );
    }

    #[test]
    fn idle_taskbar_identity_is_checked_every_two_seconds_not_every_tick() {
        let due: Vec<u64> = (1..=12)
            .filter(|&tick| super::taskbar_check_due(tick))
            .collect();
        assert_eq!(due, vec![4, 8, 12]);
    }

    #[test]
    fn nfr3_a_build_without_frame_paint_still_attaches_the_rivet_frame() {
        let steps = super::attach_steps();
        let colour = steps.iter().find_map(|step| match step {
            crate::feature::Step::Feature(step)
                if step.feature == crate::feature::Feature::FrameColour =>
            {
                Some(step)
            }
            _ => None,
        });
        assert!(colour.expect("the frame colour step").skip_when_unavailable);
    }

    #[test]
    fn test_switches_are_read_from_the_browsers_own_profile() {
        let profile = rivet_core::paths::TestDir::new("rivet-own-debug");
        let given = || {
            [
                "chrome.exe".to_owned(),
                format!("--user-data-dir={}", profile.path().display()),
            ]
            .into_iter()
        };
        assert_eq!(super::own_debug_directory_in(given()), None);
        std::fs::create_dir_all(profile.path().join("rivet-debug")).unwrap();
        assert_eq!(
            super::own_debug_directory_in(given()),
            Some(profile.path().join("rivet-debug"))
        );
    }

    use super::{
        appbar_hit, menu_hit, settings_title, tab_row_end, AppbarHit, Menu, SETTINGS_ITEMS,
    };

    #[test]
    fn fr3_workspace_menu_opens_from_header_and_chevron() {
        assert_eq!(
            appbar_hit(20.0, 120.0, 268.0, 700.0, &[], 3),
            Some(AppbarHit::SpaceHeader)
        );
        assert_eq!(
            appbar_hit(245.0, 25.0, 268.0, 700.0, &[], 3),
            Some(AppbarHit::WorkspaceMenu)
        );
        assert_eq!(appbar_hit(100.0, 25.0, 268.0, 700.0, &[], 3), None);
    }

    #[test]
    fn fr3_mark_opens_the_start_page() {
        assert_eq!(
            appbar_hit(20.0, 25.0, 268.0, 700.0, &[], 3),
            Some(AppbarHit::Mark)
        );
        assert_eq!(appbar_hit(41.0, 25.0, 268.0, 700.0, &[], 3), None);
    }

    #[test]
    fn fr3_workspace_menu_lists_every_space_and_nothing_else() {
        let hit = |y| menu_hit(Menu::Workspace, 20.0, y, 268.0, 700.0, 3);
        assert_eq!(hit(145.0), (true, Some(AppbarHit::MenuSpace(0))));
        assert_eq!(hit(193.0), (true, Some(AppbarHit::MenuSpace(1))));
        assert_eq!(hit(241.0), (true, Some(AppbarHit::MenuSpace(2))));
        assert_eq!(hit(290.0), (false, None));
        assert_eq!(
            menu_hit(Menu::Workspace, 5.0, 145.0, 268.0, 700.0, 3),
            (false, None)
        );
        // A fourth space makes the menu one row longer.
        assert_eq!(
            menu_hit(Menu::Workspace, 20.0, 290.0, 268.0, 700.0, 4),
            (true, Some(AppbarHit::MenuSpace(3)))
        );
    }

    #[test]
    fn fr3_settings_menu_rises_from_the_footer_and_opens_rivet_settings() {
        let (_, top, _, height) = super::menu_area(Menu::Settings, 268.0, 700.0, 3);
        assert!(
            top + height <= 700.0 - 38.0,
            "the menu stays above the footer"
        );
        let hit = |row: f32| {
            menu_hit(
                Menu::Settings,
                20.0,
                top + 4.0 + row * 34.0 + 5.0,
                268.0,
                700.0,
                3,
            )
        };
        assert_eq!(hit(0.0), (true, Some(AppbarHit::ProfileSettings)));
        assert_eq!(hit(1.0), (true, Some(AppbarHit::ManageProfiles)));
        assert_eq!(hit(2.0), (true, Some(AppbarHit::Credentials)));
        assert_eq!(hit(3.0), (true, Some(AppbarHit::RivetSettings)));
        assert_eq!(
            menu_hit(Menu::Settings, 20.0, top - 1.0, 268.0, 700.0, 3),
            (false, None)
        );
    }

    #[test]
    fn nfr8_rivet_settings_name_the_chosen_password_and_passkey_provider() {
        use rivet_core::CredentialProvider;
        let (title, hit) = SETTINGS_ITEMS[2];
        assert_eq!(hit, Some(AppbarHit::Credentials));
        assert_eq!(
            settings_title(title, hit, CredentialProvider::System),
            "Passwords: Windows passkeys"
        );
        assert_eq!(
            settings_title(title, hit, CredentialProvider::Browser),
            "Passwords: Browser password manager"
        );
        assert_eq!(
            settings_title(
                "Manage profiles",
                Some(AppbarHit::ManageProfiles),
                CredentialProvider::System
            ),
            "Manage profiles"
        );
    }

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
            count: 0,
        };
        let tabs = vec![crate::tab_rows::Tab {
            index: 4,
            title: "Child".into(),
            active: false,
            group: Some(group.clone()),
        }];
        let rows = crate::tab_rows::build_rows(&tabs);
        assert_eq!(
            appbar_hit(40.0, 150.0, 220.0, 700.0, &rows, 3),
            Some(AppbarHit::Folder(group.id))
        );
        assert_eq!(
            appbar_hit(40.0, 180.0, 220.0, 700.0, &rows, 3),
            Some(AppbarHit::Tab(4))
        );
        assert_eq!(
            appbar_hit(205.0, 180.0, 220.0, 700.0, &rows, 3),
            Some(AppbarHit::CloseTab(4))
        );
        assert_eq!(
            appbar_hit(40.0, 230.0, 220.0, 700.0, &rows, 3),
            Some(AppbarHit::NewTab)
        );
    }
    #[test]
    fn fr3_pinned_tiles_have_four_non_overlapping_targets() {
        let tile = (220.0 - 18.0 - 18.0) / 4.0;
        for index in 0..4 {
            let x = 10.0 + index as f32 * (tile + 6.0) + tile / 2.0;
            assert_eq!(
                appbar_hit(x, 70.0, 220.0, 700.0, &[], 3),
                Some(AppbarHit::Pinned(index))
            );
        }
    }

    #[test]
    fn fr3_tab_rows_split_activation_and_close_hit_areas() {
        assert_eq!(
            appbar_hit(40.0, 207.0, 220.0, 700.0, &test_rows(), 3),
            Some(AppbarHit::Tab(1))
        );
        assert_eq!(
            appbar_hit(205.0, 207.0, 220.0, 700.0, &test_rows(), 3),
            Some(AppbarHit::CloseTab(1))
        );
        assert_eq!(appbar_hit(40.0, 300.0, 220.0, 700.0, &test_rows(), 3), None);
    }

    #[test]
    fn fr3_new_tab_and_space_targets_are_distinct() {
        assert_eq!(
            appbar_hit(40.0, 145.0, 220.0, 700.0, &test_rows(), 3),
            Some(AppbarHit::NewTab)
        );
        assert_eq!(
            appbar_hit(90.0, 670.0, 220.0, 700.0, &test_rows(), 3),
            Some(AppbarHit::Space(0))
        );
    }

    #[test]
    fn fr3_footer_has_settings_the_spaces_and_a_new_space_button() {
        let foot = |x| appbar_hit(x, 675.0, 268.0, 700.0, &[], 3);
        assert_eq!(foot(20.0), Some(AppbarHit::Settings));
        assert_eq!(foot(240.0), Some(AppbarHit::NewSpace));
        // Three dots are centred between the two buttons.
        let start = 10.0 + 28.0 + (250.0 - 56.0 - 92.0) / 2.0;
        for index in 0..3 {
            assert_eq!(
                foot(start + index as f32 * 32.0 + 14.0),
                Some(AppbarHit::Space(index))
            );
        }
        assert_eq!(foot(start + 30.0), None, "the gap between two dots");
    }

    #[test]
    fn fr3_footer_follows_the_number_of_spaces_and_dims_new_space_when_full() {
        let foot = |x, spaces| appbar_hit(x, 675.0, 268.0, 700.0, &[], spaces);
        let start = 10.0 + 28.0 + (250.0 - 56.0 - (6.0 * 32.0 - 4.0)) / 2.0;
        assert_eq!(
            foot(start + 5.0 * 32.0 + 14.0, 6),
            Some(AppbarHit::Space(5))
        );
        assert_eq!(foot(start + 5.0 * 32.0 + 14.0, 3), None);
        assert_eq!(
            foot(240.0, 6),
            None,
            "a full footer has no room for a space"
        );
        assert_eq!(foot(240.0, 5), Some(AppbarHit::NewSpace));
    }

    #[test]
    fn fr3_tab_menu_offers_split_with_next_tab_under_the_row_it_was_opened_on() {
        use crate::tab_rows::Row;
        let rows = vec![
            Row::NewTab,
            Row::Tab {
                index: 0,
                title: "a".into(),
                active: true,
                child: false,
            },
            Row::Tab {
                index: 1,
                title: "b".into(),
                active: false,
                child: false,
            },
        ];
        // The new tab row has no menu, and the tab rows follow it after the 17 DIP divider.
        assert_eq!(tab_row_end(&rows, 150.0), None);
        let (index, top) = tab_row_end(&rows, 138.0 + 34.0 + 17.0 + 34.0 + 5.0).unwrap();
        assert_eq!(index, 1);
        let menu = Menu::Tab { index, top };
        let (_, y, _, h) = super::menu_area(menu, 268.0, 700.0, 3);
        assert!(y >= top as f32);
        assert_eq!(
            menu_hit(menu, 20.0, y + 4.0 + 10.0, 268.0, 700.0, 3),
            (true, Some(AppbarHit::SplitTab(1)))
        );
        assert_eq!(
            menu_hit(menu, 20.0, y + h + 2.0, 268.0, 700.0, 3),
            (false, None)
        );
        assert_eq!(menu_hit(menu, 20.0, y + 1.0, 268.0, 700.0, 3), (true, None));
    }

    #[test]
    fn fr3_tab_menu_near_the_footer_opens_above_its_row() {
        let menu = Menu::Tab { index: 3, top: 640 };
        let (_, y, _, h) = super::menu_area(menu, 268.0, 700.0, 3);
        assert!(y + h <= 640.0 - 34.0);
    }
}
