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
    GetFocus, GetKeyState, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE,
    TME_NONCLIENT, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CallWindowProcW, GetAncestor, GetClassNameW, GetClientRect, GetCursorPos, PeekMessageW,
    GetPropW, GetWindowLongPtrW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, KillTimer, LoadCursorW, PostMessageW, RegisterWindowMessageW,
    RemovePropW, SetCursor, SetPropW, SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW,
    WindowFromPoint, GA_ROOT, GWLP_WNDPROC, GWL_STYLE, HTBORDER, HTCAPTION, HTTOP, HTTOPLEFT,
    ICON_BIG, ICON_SMALL, IDC_SIZEWE, MSG, PM_REMOVE, SM_CXICON, SM_CXPADDEDBORDER, SM_CXSMICON,
    SM_CYSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, WH_GETMESSAGE, WM_CAPTURECHANGED, WM_CLOSE, WM_DPICHANGED, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
    WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_NCRBUTTONDOWN, WM_NCRBUTTONUP, WM_NULL,
    WM_SETCURSOR, WM_SETICON, WM_WINDOWPOSCHANGED, WNDPROC, WS_THICKFRAME,
};

use super::appbar::Appbar;
use super::chrome_layout;
use super::taskbar;
use super::{
    layout_scale_dpi, scale, strip_hit, title_bar_top, toggle_rect, AppbarLayout, StripHit,
    APPBAR_WIDTH, FLOAT_INSET, RESIZER_WIDTH,
};
use crate::press::Presses;
use crate::spaces::{SavedTab, Spaces, MAX_SPACES};
use crate::tab_rows::{self, Row};

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
    /// The entry of a tab's menu that shows the tab beside the one after it.
    SplitTab(usize),
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
pub(crate) const TAB_ITEMS: [&str; 1] = ["Split with next tab"];

/// The entries of the settings menu. An entry without an action is shown dimmed.
pub(crate) const SETTINGS_ITEMS: [(&str, Option<AppbarHit>); 3] = [
    ("Profile settings", Some(AppbarHit::ProfileSettings)),
    ("Manage profiles", Some(AppbarHit::ManageProfiles)),
    ("Rivet settings", None),
];

const WORKSPACE_MENU_TOP: f32 = 138.0;
const WORKSPACE_ROW: f32 = 48.0;
const SETTINGS_ROW: f32 = 34.0;
const SETTINGS_PAD: f32 = 4.0;
const FOOT_HEIGHT: f32 = 38.0;

/// The rectangle of a menu as (x, y, width, height) in the appbar's own coordinates, for an
/// appbar of `width` by `height` DIPs with `spaces` spaces.
pub(crate) fn menu_area(menu: Menu, width: f32, height: f32, spaces: usize) -> (f32, f32, f32, f32) {
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
            let y = if below + h > height - FOOT_HEIGHT { top as f32 - 34.0 - 4.0 - h } else { below };
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
            (row == 0.0).then_some(AppbarHit::SplitTab(index))
        }
    };
    (true, hit)
}

thread_local! {
    /// Appbars of the browser windows owned by this UI thread.
    static APPBARS: RefCell<HashMap<isize, Appbar>> = RefCell::new(HashMap::new());
    /// The open menu of each window, with the window that had the focus before it opened.
    static MENUS: RefCell<HashMap<isize, (isize, Menu)>> = RefCell::new(HashMap::new());
    /// The spaces of each window and the tabs they keep.
    static SPACES: RefCell<HashMap<isize, Spaces>> = RefCell::new(HashMap::new());
    /// The controls of each window that are being pressed or springing back.
    static PRESSES: RefCell<HashMap<isize, Presses<AppbarHit>>> = RefCell::new(HashMap::new());
    static APPBAR_STATE: RefCell<HashMap<isize, (Option<AppbarHit>, Option<AppbarHit>, u32)>> = RefCell::new(HashMap::new());
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
    TAB_DRAGS.with(|drags| drags.borrow().get(&(window as isize)).is_some_and(|drag| drag.live))
}

/// How far the pointer travels before a press on a tab becomes a drag, in device-independent
/// pixels.
const DRAG_THRESHOLD: f32 = 6.0;
/// The gap between the two halves of a page that a drop shows, as in the split itself.
const DROP_GAP: f32 = 8.0;

/// Where a dragged tab would land.
struct Drop {
    side: Option<crate::chrome_split::Side>,
    view: crate::appbar::DropView,
}

/// The drop under a point in client pixels, or none when the point is off the page.
unsafe fn drop_at(window: HWND, point: POINT) -> Option<Drop> {
    use crate::chrome_split::Side;
    let [area_left, area_top, area_right, area_bottom] = crate::chrome_split::page_area(window)?;
    let factor = layout_dpi(window) as f32 / 96.0;
    let (left, top) = (area_left as f32 * factor, area_top as f32 * factor);
    let (width, height) = (
        (area_right - area_left) as f32 * factor,
        (area_bottom - area_top) as f32 * factor,
    );
    let (x, y) = (point.x as f32, point.y as f32);
    if x < left || x > left + width || y < top || y > top + height {
        return None;
    }
    // Chrome keeps two pages in a split, so a window that has one only offers to open here.
    let full = crate::chrome_split::active_is_split(window);
    let (side, [zone_x, zone_y, w, h]) =
        crate::chrome_split::drop_zone([left, top, width, height], x, y, full, DROP_GAP * factor);
    let label = match side {
        Some(Side::Left) => "Split left",
        Some(Side::Right) => "Split right",
        Some(Side::Top) => "Split top",
        Some(Side::Bottom) => "Split bottom",
        None => "Open here",
    };
    Some(Drop {
        side,
        view: crate::appbar::DropView {
            x: zone_x,
            y: zone_y,
            width: w,
            height: h,
            label: label.to_owned(),
        },
    })
}

unsafe fn show_tab_drag(window: HWND, point: Option<POINT>) {
    let dpi = layout_dpi(window);
    let factor = dpi as f32 / 96.0;
    let title = TAB_DRAGS.with(|drags| drags.borrow().get(&(window as isize)).map(|d| d.title.clone()));
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
        return;
    };
    let title = crate::chrome_tabs::tabs(window)
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.index == index))
        .map(|tab| tab.title)
        .unwrap_or_default();
    TAB_DRAGS.with(|drags| {
        drags.borrow_mut().insert(
            window as isize,
            TabDrag { index, title, press: client_point(window, lparam), live: false },
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
        SetCapture(window);
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
    ReleaseCapture();
    show_tab_drag(window, None);
    if let Some(lparam) = release.filter(|_| drag.live) {
        let point = POINT {
            x: (lparam & 0xffff) as i16 as i32,
            y: ((lparam >> 16) & 0xffff) as i16 as i32,
        };
        if let Some(drop) = drop_at(window, point) {
            let done = match drop.side {
                Some(side) => crate::chrome_split::split_beside(window, drag.index, side),
                None => crate::chrome_tabs::activate_tab(window, drag.index),
            };
            crate::vertical_tabs::trace(&format!(
                "tab drag: dropped tab {} on {:?}: {done}",
                drag.index, drop.side
            ));
        }
    }
    update_appbar(window);
}

/// The button of a split page's top row the pointer is over, and the page's index.
type RowHit = (usize, crate::chrome_split::RowButton);

thread_local! {
    static ROW_HOVER: RefCell<HashMap<isize, RowHit>> = RefCell::new(HashMap::new());
    static ROW_PRESS: RefCell<HashMap<isize, RowHit>> = RefCell::new(HashMap::new());
}

/// The top row button under a point in client pixels. Every page's row is Rivet's apart from the
/// focused page's address, which is the browser's own address bar. The other pages' rows answer
/// anywhere.
unsafe fn row_hit(window: HWND, point: POINT) -> Option<RowHit> {
    use crate::chrome_split::{row_buttons, RowButton, ROW_HEIGHT};
    let factor = layout_dpi(window) as f32 / 96.0;
    let (x, y) = (point.x as f32 / factor, point.y as f32 / factor);
    for (index, pane) in crate::chrome_split::panes(window).iter().enumerate() {
        let top = (pane.y - ROW_HEIGHT) as f32;
        if x < pane.x as f32 || x >= (pane.x + pane.width) as f32 || y < top || y >= pane.y as f32 {
            continue;
        }
        let local = x - pane.x as f32;
        let button = row_buttons(pane.width as f32, pane.first, pane.caption)
            .into_iter()
            .find(|(button, left, width)| {
                local >= *left && local < left + width
            })
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
    use crate::chrome_split::ROW_HEIGHT;
    let panes = crate::chrome_split::panes(window);
    if panes.is_empty() {
        return Vec::new();
    }
    let factor = layout_dpi(window) as f32 / 96.0;
    let tabs = crate::chrome_tabs::open_tabs(window).unwrap_or_default();
    let hover = ROW_HOVER.with(|all| all.borrow().get(&(window as isize)).copied());
    panes
        .iter()
        .enumerate()
        .map(|(index, pane)| {
            let saved = tabs.get(pane.tab).map(|tab| &tab.saved);
            crate::appbar::PaneRow {
                x: pane.x as f32 * factor,
                y: (pane.y - ROW_HEIGHT) as f32 * factor,
                width: pane.width as f32 * factor,
                height: ROW_HEIGHT as f32 * factor,
                reach_left: 0.0,
                reach_right: 0.0,
                caption: pane.caption * factor,
                active: pane.active,
                editable: pane.active && pane.y == panes[0].y,
                first: pane.first,
                last: pane.last,
                title: saved.map(|tab| tab.title.clone()).unwrap_or_default(),
                url: saved
                    .map(|tab| tab.url.trim_start_matches("https://").trim_start_matches("http://").to_owned())
                    .unwrap_or_default(),
                hover: hover.filter(|(over, _)| *over == index).map(|(_, button)| button),
            }
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
unsafe fn navigate(window: HWND, button: crate::chrome_split::RowButton) {
    use crate::chrome_split::RowButton;
    const APPCOMMAND: u32 = 0x0319;
    let command: isize = match button {
        RowButton::Back => 1,
        RowButton::Forward => 2,
        _ => 3,
    };
    PostMessageW(window, APPCOMMAND, window as usize, command << 16);
}

/// A button of a page's top row is released.
unsafe fn row_action(window: HWND, hit: RowHit) {
    use crate::chrome_split::RowButton;
    let panes = crate::chrome_split::panes(window);
    let Some(pane) = panes.get(hit.0) else { return };
    match hit.1 {
        RowButton::Grip => {}
        RowButton::Address => {
            crate::chrome_tabs::activate_tab(window, pane.tab);
        }
        // The browser acts on the page it shows, so another page is focused first.
        RowButton::Back | RowButton::Forward | RowButton::Reload => {
            if !pane.active {
                crate::chrome_tabs::activate_tab(window, pane.tab);
            }
            navigate(window, hit.1);
        }
        RowButton::Popout | RowButton::Unsplit => {
            crate::chrome_tabs::activate_tab(window, pane.tab);
            let done = crate::chrome_split::unsplit_active(window);
            crate::vertical_tabs::trace(&format!("split row: {:?} of page {}: {done}", hit.1, hit.0));
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
    crate::vertical_tabs::layout_width(window).map_or(leading, |strip| leading.max(strip))
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
    SPACES.with(|all| edit(all.borrow_mut().entry(window as isize).or_insert_with(Spaces::new)))
}

/// The address of the Rivet start page for the space called `space`.
unsafe fn start_url(space: &str) -> Option<String> {
    runtime_directory().map(|directory| rivet_core::start_page_url(&directory, space))
}

unsafe fn start_page(window: HWND) {
    let space = with_spaces(window, |spaces| spaces.list()[spaces.current()].name.clone());
    if let Some(url) = start_url(&space) {
        if crate::chrome_tabs::new_tab(window) {
            crate::chrome_navigation::open(window, &url);
        }
    }
}

/// Shows the tabs of another space. The current space keeps its tabs, the window opens the ones
/// the target kept, or a start page when it kept none, and only then closes the old ones, so the
/// window never has zero tabs.
unsafe fn switch_space(window: HWND, index: usize) {
    let Some(open) = crate::chrome_tabs::open_tabs(window) else {
        return;
    };
    let Some(name) = with_spaces(window, |spaces| {
        spaces
            .list()
            .get(index)
            .filter(|_| index != spaces.current())
            .map(|space| space.name.clone())
    }) else {
        return;
    };
    let Some(start) = start_url(&name) else {
        return;
    };
    let target = with_spaces(window, |spaces| spaces.list()[index].clone());
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    if let Some(hand_over) = root
        .as_deref()
        .and_then(|root| crate::spaces::hand_over(&target, crate::browsers::current(), root))
    {
        let saved: Vec<SavedTab> = open.into_iter().map(|tab| tab.saved).collect();
        hand_over_to(window, index, saved, hand_over, start);
        return;
    }
    let old: Vec<usize> = open.iter().map(|tab| tab.id).collect();
    let saved: Vec<SavedTab> = open.into_iter().map(|tab| tab.saved).collect();
    let Some(restore) = with_spaces(window, |spaces| spaces.switch_to(index, saved, &start)) else {
        return;
    };
    crate::vertical_tabs::trace(&format!(
        "space: to {name}, restoring {:?}",
        restore.iter().map(|tab| tab.url.as_str()).collect::<Vec<_>>()
    ));
    let tint = with_spaces(window, |spaces| spaces.list()[spaces.current()].tint);
    APPBAR_STATE.with(|states| {
        if let Some(entry) = states.borrow_mut().get_mut(&(window as isize)) {
            entry.2 = tint;
        }
    });
    show_tabs(window, old, &restore);
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
    let running = crate::browsers::raise_existing(&hand_over.user_data_dir);
    if crate::spaces::show_for(running) == crate::spaces::Show::BringForward {
        crate::vertical_tabs::trace(&format!("space: bringing forward the {} window", hand_over.browser.label()));
        return;
    }
    let Some(directory) = runtime_directory() else {
        return;
    };
    let layout = layout_of(window);
    let dir = hand_over.user_data_dir.to_string_lossy().into_owned();
    let (mut next, root) = (
        with_spaces(window, |spaces| spaces.clone()),
        std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from),
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
        log(&format!("rivet: could not save the spaces for the other browser: {error}"));
        return;
    }
    crate::vertical_tabs::trace(&format!("space: starting {}", hand_over.browser.label()));
    crate::browsers::start(hand_over, start, directory);
}

/// Opens `restore` as the window's tabs and closes the tabs `old`, which are the ones shown now,
/// only afterwards so the window never has zero tabs.
unsafe fn show_tabs(window: HWND, old: Vec<usize>, restore: &[SavedTab]) {
    for tab in restore {
        if crate::chrome_tabs::new_tab(window) && !tab.url.is_empty() {
            crate::chrome_navigation::open(window, &tab.url);
        }
    }
    let shown: Vec<usize> = crate::chrome_tabs::open_tabs(window)
        .unwrap_or_default()
        .into_iter()
        .map(|tab| tab.id)
        .filter(|id| !old.contains(id))
        .collect();
    for id in old {
        crate::chrome_tabs::close_tab_by_id(window, id);
    }
    crate::vertical_tabs::trace(&format!(
        "space: now showing {:?}",
        crate::chrome_tabs::open_tabs(window)
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
        crate::vertical_tabs::trace(&format!("space: grouping {members:x?} as {:?}", group.title));
        let made = crate::chrome_tabs::group_tabs(window, &members, &group.title, group.color);
        crate::vertical_tabs::trace(&format!("space: grouped {made:02x?}"));
        if let Some(id) = made {
            groups.push((group.key.clone(), id));
        }
    }
    let wanted = restore.iter().position(|tab| tab.active).unwrap_or(0);
    if let Some(id) = shown.get(wanted) {
        let position = crate::chrome_tabs::open_tabs(window)
            .and_then(|tabs| tabs.iter().position(|tab| tab.id == *id));
        if let Some(position) = position {
            crate::chrome_tabs::activate_tab(window, position);
        }
    }
    // A collapsed group is folded last, and never over the tab that is shown.
    let active_group = restore.get(wanted).and_then(|tab| tab.group.as_ref()).map(|g| g.key.clone());
    for (key, id) in &groups {
        let collapsed = restore
            .iter()
            .filter_map(|tab| tab.group.as_ref())
            .any(|group| group.key == *key && group.collapsed);
        if !collapsed || active_group.as_deref() == Some(key.as_str()) {
            continue;
        }
        crate::chrome_tabs::toggle_group(window, id);
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
}

thread_local! {
    static PERSIST: RefCell<Persist> = RefCell::new(Persist::default());
}

/// The file of this process's browser profile, under `%LOCALAPPDATA%\Rivet`.
fn spaces_file() -> Option<(std::path::PathBuf, String)> {
    let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    let dir = crate::spaces::user_data_dir_of(
        std::env::args_os().map(|arg| arg.to_string_lossy().into_owned()),
    );
    Some((
        crate::spaces::store_path(&root, dir.as_deref()),
        dir.unwrap_or_default(),
    ))
}

/// Restores the saved spaces, the appbar width and its hidden state when a window attaches.
/// Missing or corrupt files leave the defaults. A corrupt file is kept under another name.
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
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Some(mut stored) = Spaces::from_json(&text) else {
        let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
        return;
    };
    let pending = stored.spaces.take_current_tabs();
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
    let Some(open) = crate::chrome_tabs::open_tabs(window) else {
        return;
    };
    if let Some(tabs) = PERSIST.with(|persist| persist.borrow_mut().pending.take()) {
        if !tabs.is_empty() && open.iter().all(|tab| crate::spaces::is_blank_tab(&tab.saved.url)) {
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
                    RegisterWindowMessageW(wide("Rivet.NewStartPage").as_ptr()),
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
    SetCapture(window);
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
        ReleaseCapture();
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
        if (top..top + 34.0).contains(&y) {
            return match row {
                Row::Tab { index, .. } => Some((*index, (top + 34.0) as i32)),
                _ => None,
            };
        }
        top += 34.0;
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
    let tabs = unsafe { crate::chrome_tabs::tabs(window) }.unwrap_or_default();
    tab_rows::build_rows(&tabs)
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
        AppbarHit::ProfileSettings | AppbarHit::ManageProfiles => {
            close_menu(window);
            let url = if hit == AppbarHit::ProfileSettings {
                "chrome://settings/people"
            } else {
                "chrome://settings/manageProfile"
            };
            if crate::chrome_tabs::new_tab(window) {
                crate::chrome_navigation::open(window, url);
            }
        }
        AppbarHit::Tab(index) => {
            crate::chrome_tabs::activate_tab(window, index);
        }
        AppbarHit::SplitTab(index) => {
            close_menu(window);
            // The split is made from the active tab and the one after it.
            let activated = crate::chrome_tabs::activate_tab(window, index);
            let split = activated && crate::chrome_split::split_active(window);
            crate::vertical_tabs::trace(&format!("tab menu: split tab {index}: {activated} {split}"));
        }
        AppbarHit::CloseTab(index) => {
            crate::chrome_tabs::close_tab(window, index);
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
            if crate::chrome_tabs::new_tab(window) {
                crate::chrome_navigation::open(window, URLS[index]);
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
            crate::chrome_tabs::toggle_group(window, &id);
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
    crate::vertical_tabs::set_strip_override(window, layout.strip());
}

/// Lays the browser out again, now that the appbar's width or mode changed, and redraws.
unsafe fn relayout(window: HWND) {
    // The frame view lays out again on request, which also works for a maximized window.
    crate::vertical_tabs::invalidate_strip(window);
    if chrome_layout::relayout(window) {
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
        // flag file next to the runtime keeps such a peek from leaving with the real pointer.
        let held = runtime_directory().is_some_and(|dir| dir.join("rivet-hold-peek.flag").exists());
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
        ReleaseCapture();
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

thread_local! {
    /// The latest pointer x of a width drag that has not been laid out yet, and when the browser
    /// was last laid out for the drag.
    static DRAG_PENDING: RefCell<HashMap<isize, (Option<i32>, f32, bool)>> = RefCell::new(HashMap::new());
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
        let entry = all.entry(window as isize).or_insert((None, f32::MIN, false));
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
        pending.borrow_mut().get_mut(&(window as isize)).and_then(|entry| {
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

unsafe fn attach_message() -> u32 {
    RegisterWindowMessageW(wide(ATTACH_MESSAGE).as_ptr())
}

/// The frame surrounding the browser's page panel.
fn nav_frame_color(window: HWND) -> Option<u32> {
    attached(window).then_some(0xFFECE8E2)
}

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
    if let Some(path) = runtime_directory()
        .map(|dir| dir.join("rivet-vertical-tabs.log"))
        .filter(|path| path.exists())
    {
        crate::vertical_tabs::set_trace(path);
    }
    // Without the layout hook the browser would draw underneath the appbar, so an unknown
    // browser build keeps working without Rivet instead.
    if let Err(error) = chrome_layout::install(reserved_width) {
        log(&format!("rivet: not attaching to this browser: {error}"));
        return;
    }
    if let Err(error) = chrome_layout::install_frame_paint(nav_frame_color) {
        log(&format!("rivet: could not paint the frame: {error}"));
        return;
    }
    chrome_layout::PANEL.store(true, std::sync::atomic::Ordering::Relaxed);
    chrome_layout::on_panel_change(panel_moved);
    if let Err(error) = chrome_layout::install_caption_paint() {
        log(&format!("rivet: could not paint caption buttons: {error}"));
    }
    if let Err(error) = chrome_layout::install_caption_layout(|window| {
        attached(window).then_some(crate::CAPTION_HEIGHT + chrome_layout::PANEL_TOP)
    }) {
        log(&format!(
            "rivet: could not lay out caption buttons: {error}"
        ));
    }
    // The caption height and the top inset size the panel's top row, so they never depend on the
    // trace file. Failures show in the trace when it is on.
    let caption = chrome_layout::install_caption_height(|window| {
        attached(window).then_some(crate::CAPTION_HEIGHT + chrome_layout::PANEL_TOP)
    });
    crate::vertical_tabs::trace(&format!("caption height: {caption:?}"));
    let top = chrome_layout::install_top_inset(|window| {
        attached(window).then_some(chrome_layout::PANEL_TOP + crate::builds::get().edge_vertical.as_ref().map_or(0, |edge| edge.top_inset_adjust))
    });
    crate::vertical_tabs::trace(&format!("top inset: {top:?}"));
    if runtime_directory().is_some_and(|dir| dir.join("rivet-nav.log").exists()) {
        if let Some(dir) = runtime_directory() {
            crate::vertical_tabs::set_trace(dir.join("rivet-nav.log"));
        }
    }
    let hide = runtime_directory().map(|dir| dir.join("rivet-hide-tab-strip.log"));
    if let Some(path) = hide.filter(|path| path.exists()) {
        crate::vertical_tabs::set_trace(path);
        if let Err(error) = crate::vertical_tabs::hide_tab_strip_everywhere() {
            crate::vertical_tabs::trace(&format!("hide failed: {error}"));
        }
    }
    // The file next to the runtime only switches the trace on. The toolbar and the vertical-tab
    // frame are what the top row needs, so they never depend on it.
    if crate::vertical_tabs::tracing() {
        crate::vertical_tabs::trace_crashes();
    }
    let toolbar = crate::chrome_toolbar::install(runtime_directory());
    crate::vertical_tabs::trace(&format!("toolbar: {toolbar:?}"));
    if let Err(error) = toolbar {
        log(&format!("rivet: could not install the toolbar: {error}"));
        return;
    }
    let split = crate::chrome_split::install();
    crate::vertical_tabs::trace(&format!("split: {split:?}"));
    let panels = crate::chrome_split::install_panels();
    crate::vertical_tabs::trace(&format!("split panels: {panels:?}"));
    let vertical_tabs = crate::vertical_tabs::install(report_vertical_tabs)
        .map_err(|error| crate::vertical_tabs::trace(&format!("unavailable: {error}")))
        .is_ok();
    let appbar = match Appbar::new(window as isize) {
        Ok(appbar) => appbar,
        Err(error) => {
            // The browser keeps working without Rivet when composition is unavailable.
            log(&format!("rivet: could not compose the appbar: {error}"));
            return;
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
    if crate::vertical_tabs::tracing() && crate::edge_vertical::skipped("dump") {
        let handle = window as isize;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            crate::vertical_tabs::trace("view tree after 5 s");
            unsafe { crate::edge_vertical::dump_window(handle as HWND) };
        });
    }
    if crate::builds::get().edge_vertical.is_some() {
        let adopted = crate::edge_vertical::adopt(window);
        crate::vertical_tabs::trace(&format!("edge vertical tabs: {adopted:?}"));
    }
    if vertical_tabs || crate::builds::get().edge_vertical.is_some() {
        crate::chrome_toolbar::adopt(window);
    }
    if runtime_directory().is_some_and(|dir| {
        dir.join("rivet-group-probe.flag").exists() || dir.join("rivet-split.flag").exists()
    }) {
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
pub(crate) unsafe fn update_appbar(window: HWND) {
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
            let rows = if visible { pane_rows(window) } else { Vec::new() };
            if let Err(error) = appbar.update_rows(&rows, dpi) {
                log(&format!("rivet: could not draw the page rows: {error}"));
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
    let panes = crate::chrome_split::panes(window);
    if !panes.is_empty() {
        let pages: Vec<chrome_layout::Rect> = panes
            .iter()
            .map(|pane| chrome_layout::Rect { x: pane.x, y: pane.y, width: pane.width, height: pane.height })
            .collect();
        let active = panes.iter().position(|pane| pane.active).unwrap_or(0);
        let mut holes = crate::ground::split_holes(&pages, active, crate::chrome_split::ROW_HEIGHT);
        // The Sidebar's rail and the caption buttons sit at the window's right edge, in the panel
        // of the last page as in a window that is not split.
        if let (Some(last), Some(panel)) = (holes.last_mut(), chrome_layout::panel_bounds(window)) {
            last.rect.width = last.rect.width.max(panel.x + panel.width - last.rect.x);
        }
        return holes;
    }
    let leading = LAID_LEADING.with(|all| all.borrow().get(&(window as isize)).copied()).unwrap_or(0);
    chrome_layout::panel_bounds(window)
        .map(|mut rect| {
            let cut = (leading - rect.x).max(0).min(rect.width - 1);
            rect.x += cut;
            rect.width -= cut;
            vec![Hole { rect, raise: Raise::Page }]
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
        // The scale is recorded above for both tab layouts. The strip is only subtracted here.
        let leading = layout_of(window).leading();
        LAID_LEADING.with(|all| all.borrow_mut().insert(window as isize, leading));
        if let Some(strip) = crate::vertical_tabs::layout_width(window) {
            return Some(crate::reserved_for_strip(leading, strip));
        }
        Some(leading)
    }
}

pub(crate) fn log(message: &str) {
    crate::vertical_tabs::trace(message);
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
    let x = point.x as f32 / factor;
    let y = (point.y - top_border(window)) as f32 / factor;
    let side = layout.side as f32;
    if strip == StripHit::Caption && x >= side - 36.0 && x < side - 8.0 && (12.0..40.0).contains(&y)
    {
        StripHit::Appbar
    } else if strip == StripHit::Resizer {
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
    // The browser lays out and paints parts of a window while handling its messages, so they run
    // in that window's look.
    // Accessibility queries can arrive while the window is still being built or torn down and
    // never draw, so they skip the scope.
    if attached(window) && message != windows_sys::Win32::UI::WindowsAndMessaging::WM_GETOBJECT {
        return crate::chrome_toolbar::for_window(window, || {
            handle(window, message, wparam, lparam)
        });
    }
    handle(window, message, wparam, lparam)
}

unsafe fn handle(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == RegisterWindowMessageW(wide("Rivet.NewStartPage").as_ptr()) && attached(window) {
        start_page(window);
        return 0;
    }
    if message == RegisterWindowMessageW(wide(crate::chrome_split::RELAYOUT_MESSAGE).as_ptr())
        && attached(window)
    {
        crate::chrome_side_panel::relayout_page_area(window);
        update_appbar(window);
        return 0;
    }
    if message == RegisterWindowMessageW(wide("Rivet.ToggleSplit").as_ptr()) && attached(window) {
        crate::chrome_split::toggle_split(window);
        update_appbar(window);
        return 0;
    }
    if message == toggle_message() && attached(window) {
        toggle_appbar(window);
        return 0;
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
    }
    match message {
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
            if let Some(hit) = pressed.filter(|pressed| Some(*pressed) == now) {
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
            if stale {
                update_appbar(window);
            }
            call_browser(window, message, wparam, lparam)
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
                SetCapture(window);
            }
            press_down(window, hit);
            update_appbar(window);
            0
        }
        WM_NCLBUTTONUP if wparam as u32 == APPBAR_HIT && attached(window) => {
            TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize)));
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
            LAYOUTS.with(|layouts| layouts.borrow_mut().remove(&(window as isize)));
            SLIDE.with(|slides| slides.borrow_mut().remove(&(window as isize)));
            DRAGGING.with(|set| set.borrow_mut().remove(&(window as isize)));
            TAB_DRAGS.with(|drags| drags.borrow_mut().remove(&(window as isize)));
            KillTimer(window, PRESS_TIMER);
            SPACES.with(|all| all.borrow_mut().remove(&(window as isize)));
            PRESSES.with(|all| all.borrow_mut().remove(&(window as isize)));
            APPBARS.with(|appbars| appbars.borrow_mut().remove(&(window as isize)));
            APPBAR_STATE.with(|states| states.borrow_mut().remove(&(window as isize)));
            crate::vertical_tabs::forget(window);
            crate::edge_vertical::forget(window);
            chrome_layout::forget_view(window);
            crate::chrome_toolbar::forget(window);
            crate::side_rail::forget(window);
            LAYOUT_DPI.with(|layouts| layouts.borrow_mut().remove(&(window as isize)));
            let key = wide(ORIGINAL_PROC);
            let original = GetPropW(window, key.as_ptr());
            SetWindowLongPtrW(window, GWLP_WNDPROC, original as isize);
            RemovePropW(window, key.as_ptr());
            let original: WNDPROC = std::mem::transmute(original);
            CallWindowProcW(original, window, message, wparam, lparam)
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
            crate::chrome_side_panel::tick(window, runtime_directory().as_deref());
            // A browser can write its own identity again, for example when a profile finishes
            // loading. The taskbar groups by that identity, so Rivet puts its own back.
            if taskbar::current_id(window as isize).as_deref() != Some(taskbar::APP_ID) {
                crate::vertical_tabs::trace("taskbar identity was replaced; applying Rivet's again");
                let _ = taskbar::apply(window as isize, runtime_directory().as_deref());
                set_rivet_icons(window);
            }
            if let Some(flag) = runtime_directory().map(|dir| dir.join("rivet-split-now.flag")) {
                if std::fs::remove_file(&flag).is_ok() {
                    crate::chrome_split::probe(window);
                }
            }
            // A checked build runs one tab command from `rivet-tab.txt` without a pointer: `new`,
            // `activate N` or `close N`, and traces the tab list afterwards.
            if let Some(flag) = runtime_directory().map(|dir| dir.join("rivet-tab.txt")) {
                if let Ok(text) = std::fs::read_to_string(&flag) {
                    let _ = std::fs::remove_file(&flag);
                    let mut words = text.split_whitespace();
                    let index = words.clone().nth(1).and_then(|word| word.parse::<usize>().ok()).unwrap_or(0);
                    let done = match words.next() {
                        Some("new") => crate::chrome_tabs::new_tab(window),
                        Some("activate") => crate::chrome_tabs::activate_tab(window, index),
                        Some("close") => crate::chrome_tabs::close_tab(window, index),
                        _ => false,
                    };
                    let titles: Vec<String> = crate::chrome_tabs::tabs(window)
                        .map(|tabs| tabs.iter().map(|tab| format!("{}{}", if tab.active { "*" } else { "" }, tab.title)).collect())
                        .unwrap_or_default();
                    crate::vertical_tabs::trace(&format!("tab command {:?}: {done} tabs {titles:?}", text.trim()));
                    let urls: Vec<String> = crate::chrome_tabs::open_tabs(window)
                        .map(|tabs| tabs.into_iter().map(|tab| tab.saved.url).collect())
                        .unwrap_or_default();
                    crate::vertical_tabs::trace(&format!("tab urls {urls:?}"));
                }
            }
            // A checked build drops the first inactive tab on a side of the page without a pointer.
            if let Some(flag) = runtime_directory().map(|dir| dir.join("rivet-drop.flag")) {
                if let Ok(text) = std::fs::read_to_string(&flag) {
                    let _ = std::fs::remove_file(&flag);
                    use crate::chrome_split::Side;
                    let side = match text.trim() {
                        "left" => Side::Left,
                        "top" => Side::Top,
                        "bottom" => Side::Bottom,
                        _ => Side::Right,
                    };
                    let tabs = crate::chrome_tabs::tabs(window).unwrap_or_default();
                    let other = tabs.iter().find(|tab| !tab.active).map(|tab| tab.index);
                    let done = other.is_some_and(|index| crate::chrome_split::split_beside(window, index, side));
                    crate::vertical_tabs::trace(&format!("drop flag: {side:?} {other:?} {done}"));
                }
            }
            persist_tick(window);
            update_appbar(window);
            0
        }
        _ if message == group_probe_message() && attached(window) => {
            if runtime_directory().is_some_and(|dir| dir.join("rivet-split.flag").exists()) {
                crate::chrome_split::probe(window);
            } else {
                crate::chrome_tabs::probe_group(window);
            }
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
    use super::{appbar_hit, menu_hit, tab_row_end, AppbarHit, Menu};

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
    fn fr3_settings_menu_rises_from_the_footer_with_a_disabled_last_entry() {
        let (_, top, _, height) = super::menu_area(Menu::Settings, 268.0, 700.0, 3);
        assert!(top + height <= 700.0 - 38.0, "the menu stays above the footer");
        let hit = |row: f32| menu_hit(Menu::Settings, 20.0, top + 4.0 + row * 34.0 + 5.0, 268.0, 700.0, 3);
        assert_eq!(hit(0.0), (true, Some(AppbarHit::ProfileSettings)));
        assert_eq!(hit(1.0), (true, Some(AppbarHit::ManageProfiles)));
        assert_eq!(hit(2.0), (true, None), "Rivet settings has no action yet");
        assert_eq!(
            menu_hit(Menu::Settings, 20.0, top - 1.0, 268.0, 700.0, 3),
            (false, None)
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
        assert_eq!(
            appbar_hit(40.0, 300.0, 220.0, 700.0, &test_rows(), 3),
            None
        );
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
        assert_eq!(foot(start + 5.0 * 32.0 + 14.0, 6), Some(AppbarHit::Space(5)));
        assert_eq!(foot(start + 5.0 * 32.0 + 14.0, 3), None);
        assert_eq!(foot(240.0, 6), None, "a full footer has no room for a space");
        assert_eq!(foot(240.0, 5), Some(AppbarHit::NewSpace));
    }

    #[test]
    fn fr3_tab_menu_offers_split_with_next_tab_under_the_row_it_was_opened_on() {
        use crate::tab_rows::Row;
        let rows = vec![
            Row::NewTab,
            Row::Tab { index: 0, title: "a".into(), active: true, child: false },
            Row::Tab { index: 1, title: "b".into(), active: false, child: false },
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
        assert_eq!(menu_hit(menu, 20.0, y + h + 2.0, 268.0, 700.0, 3), (false, None));
        assert_eq!(menu_hit(menu, 20.0, y + 1.0, 268.0, 700.0, 3), (true, None));
    }

    #[test]
    fn fr3_tab_menu_near_the_footer_opens_above_its_row() {
        let menu = Menu::Tab { index: 3, top: 640 };
        let (_, y, _, h) = super::menu_area(menu, 268.0, 700.0, 3);
        assert!(y + h <= 640.0 - 34.0);
    }
}
