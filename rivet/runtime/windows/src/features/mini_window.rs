//! The Mini Window: one page popped out of a Rivet window into its own small browser window.
//!
//! The page is browser content, so it lives in a real browser window of the same profile. Its tab
//! moves there with Chromium's own move-to-new-window function and comes back with Chromium's
//! detach and insert, so the page keeps its history and state. The new window is placed near
//! where the page was, at a small size, as a normal top-level window that is not forced on top.
//!
//! Rivet's slim row on that window is a DirectComposition visual of the window's own surface, like
//! the appbar, with its input taken from the subclassed window. Its buttons return the page to the
//! Rivet window as a split, open it there as a tab, or close the Mini Window. Dragged by its grip,
//! the window drops back into the pages: near an edge it splits the page there, elsewhere over the
//! pages it opens there as a tab.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::{Interface, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND as WinHwnd, POINT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDesktopDevice, IDCompositionTarget,
    IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory5, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows_numerics::{Matrix3x2, Vector2};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};

use crate::features::second_view::bare;
use crate::features::split::Side;
use rivet_protocol::window::{Edge, MiniWindowCommand};

/// The height of the Mini Window's own row, in DIPs.
pub const ROW: i32 = 32;
/// The size a new Mini Window takes, in DIPs: small, like Arc's Little Arc.
pub const DEFAULT_SIZE: [i32; 2] = [448, 296];
/// The smallest Mini Window.
pub const MIN_SIZE: [i32; 2] = [220, 140];
/// The gap a new Mini Window keeps from the right and bottom of the pages it came from.
const MARGIN: i32 = 24;
/// The buttons of the row, and the room they take at its right end.
const BUTTON: f32 = 28.0;
const BUTTON_PAD: f32 = 4.0;
/// Where the grip starts in the row.
pub const GRIP_X: f32 = 4.0;
const GRIP_GLYPH: char = '\u{E7C2}';

const PAPER: u32 = 0xfbfaf7;
const INK: u32 = 0x1f2024;
const SOFT_INK: u32 = 0x4a4850;
const ICONS: &str = "Segoe Fluent Icons";
/// Back into the Rivet window as a split, open there as a tab, and close.
const RETURN_GLYPH: char = '\u{E73F}';
const TAB_GLYPH: char = '\u{E8A7}';
const CLOSE_GLYPH: char = '\u{E8BB}';

/// The timer that keeps the row's title current.
const TITLE_TIMER: usize = 0x5249_4d57;

/// What a point of the Mini Window's row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// The row outside its buttons, which only moves the window.
    Row,
    /// The grip at the row's left end. A drag by it moves the window and drops it into the pages
    /// of its Rivet window as a split pane's grip does.
    Grip,
    /// Puts the page back into the Rivet window as a split beside its page.
    Return,
    /// Opens the page in the Rivet window as its shown tab.
    OpenTab,
    Close,
}

/// Where a new Mini Window goes in the pages' area `area` (left, top, right, bottom) of the window
/// it pops out of: at the lower right, as x, y, width and height in that window's DIPs. It may
/// reach past the area, since it is a window of its own.
pub fn default_rect(area: [i32; 4]) -> [i32; 4] {
    let [width, height] = DEFAULT_SIZE;
    let x = (area[2] - MARGIN - width).max(area[0]);
    let y = (area[3] - MARGIN - height).max(area[1]);
    [x, y, width, height]
}

/// The screen rectangle in pixels of `rect`, given in DIPs of a window whose client area starts
/// at screen pixel `origin` and has `scale` pixels per DIP.
pub fn on_screen(origin: [i32; 2], rect: [i32; 4], scale: f32) -> [i32; 4] {
    let px = |value: i32| (value as f32 * scale).round() as i32;
    [
        origin[0] + px(rect[0]),
        origin[1] + px(rect[1]),
        px(rect[2].max(MIN_SIZE[0])),
        px(rect[3].max(MIN_SIZE[1])),
    ]
}

/// The x offsets of the return, open-as-tab and close buttons from the row's left edge.
pub fn row_buttons(width: f32) -> [f32; 3] {
    let close = width - BUTTON_PAD - BUTTON;
    [close - 2.0 * BUTTON, close - BUTTON, close]
}

/// What the client point (`x`, `y`) in DIPs is on the row of a Mini Window `width` DIPs wide, or
/// none below the row, where the page's input belongs to the browser.
pub fn row_hit(width: f32, x: f32, y: f32) -> Option<Hit> {
    if !(0.0..ROW as f32).contains(&y) || x < 0.0 || x >= width {
        return None;
    }
    let [back, tab, close] = row_buttons(width);
    Some(if x < GRIP_X + BUTTON {
        Hit::Grip
    } else if x >= close {
        Hit::Close
    } else if x >= tab {
        Hit::OpenTab
    } else if x >= back {
        Hit::Return
    } else {
        Hit::Row
    })
}

/// The hit test answer for the row: the browser's resize borders stay, the row and its grip move
/// the window as a caption does, and the buttons are Rivet's own client area.
pub fn nc_answer(hit: Option<Hit>, original: isize) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT};
    if (HTLEFT as isize..=HTBOTTOMRIGHT as isize).contains(&original) {
        return original;
    }
    match hit {
        Some(Hit::Row | Hit::Grip) => HTCAPTION as isize,
        Some(_) => HTCLIENT as isize,
        None => original,
    }
}

/// Where a Mini Window dropped at (`x`, `y`) lands, as a tab dropped there would: near an edge of
/// the pages it returns as a split on that edge, elsewhere over the pages it opens there as a tab,
/// and off the pages it stays a Mini Window (none).
pub fn drop_target(area: [i32; 4], x: f32, y: f32) -> Option<Option<Edge>> {
    if outside(area, x, y) {
        return None;
    }
    let [left, top, right, bottom] = area.map(|value| value as f32);
    let (side, _) = crate::features::split::drop_zone(
        [left, top, right - left, bottom - top],
        x,
        y,
        false,
        0.0,
    );
    Some(side.map(|side| match side {
        Side::Left => Edge::Left,
        Side::Right => Edge::Right,
        Side::Top => Edge::Top,
        Side::Bottom => Edge::Bottom,
    }))
}

/// Whether a split pane's grip released at (`x`, `y`) left the pages, which makes that page a Mini
/// Window.
pub fn outside(area: [i32; 4], x: f32, y: f32) -> bool {
    let [left, top, right, bottom] = area.map(|value| value as f32);
    x < left || x >= right || y < top || y >= bottom
}

/// The split side an edge stands for.
pub fn side_of(edge: Edge) -> Side {
    match edge {
        Edge::Left => Side::Left,
        Edge::Right => Side::Right,
        Edge::Top => Side::Top,
        Edge::Bottom => Side::Bottom,
    }
}

/// What the visual was painted for.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    width: i32,
    dpi: u32,
    title: String,
    hover: Option<Hit>,
}

/// One Mini Window, kept by its own window handle.
struct Mini {
    /// The Rivet window the page came from and goes back to.
    owner: HWND,
    /// The browser's window procedure, called for everything Rivet does not take.
    original: isize,
    hover: Option<Hit>,
    /// The button a press began on.
    pressed: Option<Hit>,
    /// Whether the current move began on the grip, and whether it moved.
    grip_drag: bool,
    moving: bool,
    /// Set while Rivet hands the tab back, so the browser may close the empty window.
    releasing: bool,
    surface: Option<Surface>,
}

struct Surface {
    device: IDCompositionDesktopDevice,
    _target: IDCompositionTarget,
    dwrite: IDWriteFactory5,
    visual: IDCompositionVisual2,
    key: Option<Key>,
}

thread_local! {
    static MINIS: RefCell<HashMap<isize, Mini>> = RefCell::new(HashMap::new());
}

fn with<R>(mini: HWND, act: impl FnOnce(&mut Mini) -> R) -> Option<R> {
    MINIS.with(|all| all.borrow_mut().get_mut(&(mini as isize)).map(act))
}

/// The Mini Windows popped out of `owner`.
fn minis_of(owner: HWND) -> Vec<HWND> {
    MINIS.with(|all| {
        all.borrow()
            .iter()
            .filter(|(_, mini)| mini.owner == owner)
            .map(|(window, _)| *window as HWND)
            .collect()
    })
}

/// Whether `window` is a Mini Window.
pub fn is_mini(window: HWND) -> bool {
    MINIS.with(|all| all.borrow().contains_key(&(window as isize)))
}

/// The room a Mini Window keeps at its top for Rivet's row, in DIPs, so the browser lays the page
/// out below it.
pub fn top_inset(window: HWND) -> Option<i32> {
    is_mini(window).then_some(ROW)
}

/// The pages' area of the window in DIPs as left, top, right and bottom.
unsafe fn area(window: HWND) -> Option<[i32; 4]> {
    crate::features::split::page_area(window)
}

unsafe fn scale_of(window: HWND) -> f32 {
    windows_sys::Win32::UI::HiDpi::GetDpiForWindow(window).max(96) as f32 / 96.0
}

unsafe fn client_origin(window: HWND) -> [i32; 2] {
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    windows_sys::Win32::Graphics::Gdi::ClientToScreen(window, &mut point);
    [point.x, point.y]
}

/// The strip index of the tab that holds `page` in `window`.
unsafe fn tab_of(window: HWND, page: usize) -> Option<usize> {
    crate::features::tabs::open_tabs(window)?
        .iter()
        .position(|tab| tab.id == page)
}

/// The browser window of this process, other than `except`, whose tab strip holds `page`.
unsafe fn window_holding(page: usize, except: HWND) -> Option<HWND> {
    crate::frame::browser_windows_of_process()
        .into_iter()
        .filter(|window| *window != except)
        .find(|window| tab_of(*window, page).is_some())
}

/// Pops the window's tab `tab` out into a Mini Window of its own. The tab must not be the active
/// one, since the window's own page view shows that page.
pub unsafe fn open(window: HWND, tab: usize) -> bool {
    let page = bare::page_of_tab(window, tab);
    if page == 0 || page == bare::active(window) {
        crate::trace::trace(&format!(
            "mini window: tab {tab} cannot pop out, it is the shown page"
        ));
        return false;
    }
    if !crate::features::app_window::can_move_tabs() {
        crate::trace::trace("mini window: this build cannot move a tab between windows");
        return false;
    }
    let Some(area) = area(window) else {
        return false;
    };
    // Placed from the Rivet window's own place, so a window off every monitor keeps its Mini
    // Window off them too.
    let place = on_screen(
        client_origin(window),
        default_rect(area),
        crate::frame::layout_dpi(window) as f32 / 96.0,
    );
    if !crate::features::app_window::move_tab_to_new_window(window, tab) {
        crate::trace::trace("mini window: no move to a new window for this build");
        return false;
    }
    let Some(mini) = window_holding(page, window) else {
        crate::trace::trace("mini window: the browser made no window for the tab");
        return false;
    };
    adopt(window, mini, place);
    crate::trace::trace(&format!(
        "mini window: open tab {tab} in window {mini:?} at {place:?}"
    ));
    crate::frame::update_appbar(window);
    true
}

/// Takes the new browser window as the Mini Window of `owner`: places it, takes its messages and
/// gives it Rivet's row.
unsafe fn adopt(owner: HWND, mini: HWND, place: [i32; 4]) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetTimer, SetWindowLongPtrW, SetWindowPos, GWLP_WNDPROC,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
    };
    let original = GetWindowLongPtrW(mini, GWLP_WNDPROC);
    MINIS.with(|all| {
        all.borrow_mut().insert(
            mini as isize,
            Mini {
                owner,
                original,
                hover: None,
                pressed: None,
                grip_drag: false,
                moving: false,
                releasing: false,
                surface: None,
            },
        )
    });
    SetWindowLongPtrW(mini, GWLP_WNDPROC, mini_proc as *const () as isize);
    SetWindowPos(
        mini,
        std::ptr::null_mut(),
        place[0],
        place[1],
        place[2],
        place[3],
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );
    SetTimer(mini, TITLE_TIMER, 500, None);
    paint(mini);
}

/// Takes the page of split pane `index` out of the split and pops it out. The other page stays as
/// the window's page.
pub unsafe fn pop_out_pane(window: HWND, index: usize) -> bool {
    let panes = crate::features::split::panes(window);
    let Some(tab) = panes.get(index).and_then(|pane| pane.tab) else {
        return false;
    };
    // A split on Rivet's own page views keeps its other pages.
    if let Some(slot) = crate::features::second_view::slot_of_pane(window, &panes[index]) {
        let done = crate::features::split::pop_out(window, slot);
        crate::trace::trace(&format!(
            "mini window: pop out page {index} of the split: {done}"
        ));
        return done;
    }
    if let Some(other) = panes
        .iter()
        .enumerate()
        .find(|(at, _)| *at != index)
        .and_then(|(_, pane)| pane.tab)
    {
        crate::features::tabs::activate_tab(window, other);
    }
    crate::features::split::unsplit_active(window);
    let done = open(window, tab);
    crate::trace::trace(&format!("mini window: pop out pane {index}: {done}"));
    done
}

/// Pops the window's active page out. The window shows the tab beside it, or a new tab when it
/// has only this one, so the Rivet window never stands empty.
pub unsafe fn pop_out_active(window: HWND) -> bool {
    let Some(tabs) = crate::features::tabs::tabs(window) else {
        return false;
    };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    match crate::features::split::partner_of(active, tabs.len()) {
        Some(other) => {
            crate::features::tabs::activate_tab(window, other);
        }
        None => {
            if !crate::features::tabs::new_tab(window) {
                return false;
            }
        }
    }
    let done = open(window, active);
    crate::trace::trace(&format!(
        "mini window: pop out the active tab {active}: {done}"
    ));
    done
}

/// Pops the focused page out: the focused pane of a split, or the window's only page.
pub unsafe fn pop_out_focused(window: HWND) -> bool {
    let panes = crate::features::split::panes(window);
    match panes.iter().position(|pane| pane.active) {
        Some(index) if !panes.is_empty() => pop_out_pane(window, index),
        _ => pop_out_active(window),
    }
}

/// Hands the Mini Window's page back to its Rivet window and returns its index there. The empty
/// Mini Window is closed by the browser.
unsafe fn hand_back(mini: HWND, active: bool) -> Option<(HWND, usize)> {
    let mut owner = with(mini, |state| state.owner)?;
    if windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(owner) == 0 {
        owner = *crate::frame::rivet_windows().first()?;
    }
    let page = bare::page_of_tab(mini, 0);
    with(mini, |state| state.releasing = true);
    if page == 0 || !crate::features::app_window::move_tab(mini, 0, owner, active) {
        with(mini, |state| state.releasing = false);
        return None;
    }
    let index = tab_of(owner, page)?;
    Some((owner, index))
}

/// Opens the Mini Window's page in its Rivet window as the shown tab.
pub unsafe fn open_here(mini: HWND) -> bool {
    let done = hand_back(mini, true);
    crate::trace::trace(&format!(
        "mini window: opened the page as tab {:?} of its window",
        done.map(|(_, index)| index)
    ));
    if let Some((owner, _)) = done {
        crate::frame::update_appbar(owner);
    }
    done.is_some()
}

/// Closes the Mini Window. Its page goes back to its Rivet window as a tab in the background, so
/// nothing is lost.
pub unsafe fn close(mini: HWND) -> bool {
    let done = hand_back(mini, false);
    crate::trace::trace(&format!(
        "mini window: closed, the page stays as tab {:?}",
        done.map(|(_, index)| index)
    ));
    if let Some((owner, _)) = done {
        crate::frame::update_appbar(owner);
    }
    done.is_some()
}

/// Puts the Mini Window's page back into its Rivet window as a split on `edge` of the page.
pub unsafe fn return_to(mini: HWND, edge: Edge) -> bool {
    let Some((owner, index)) = hand_back(mini, false) else {
        return false;
    };
    let done = crate::features::split::split_beside_page(owner, index, side_of(edge), None);
    crate::trace::trace(&format!(
        "mini window: returned tab {index} to the {edge:?} edge: {done}"
    ));
    crate::frame::update_appbar(owner);
    done
}

/// Moves the Mini Window's top left corner to (`x`, `y`), given in DIPs of its Rivet window's
/// client area.
pub unsafe fn move_to(mini: HWND, x: i32, y: i32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };
    let Some(owner) = with(mini, |state| state.owner) else {
        return false;
    };
    let place = on_screen(
        client_origin(owner),
        [x, y, MIN_SIZE[0], MIN_SIZE[1]],
        crate::frame::layout_dpi(owner) as f32 / 96.0,
    );
    let done = SetWindowPos(
        mini,
        std::ptr::null_mut(),
        place[0],
        place[1],
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    ) != 0;
    crate::trace::trace(&format!(
        "mini window: at {:?}: {done}",
        [place[0], place[1]]
    ));
    done
}

/// Gives the Mini Window a new size in its own DIPs, keeping its top left corner.
pub unsafe fn resize_to(mini: HWND, width: i32, height: i32) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    };
    let scale = scale_of(mini);
    let px = |value: i32| (value as f32 * scale).round() as i32;
    let done = SetWindowPos(
        mini,
        std::ptr::null_mut(),
        0,
        0,
        px(width.max(MIN_SIZE[0])),
        px(height.max(MIN_SIZE[1])),
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    ) != 0;
    crate::trace::trace(&format!("mini window: size {width}x{height}: {done}"));
    done
}

/// Runs a command posted to a Rivet window as the Mini Window message. Commands other than pop out
/// act on the Mini Window that window last popped out.
pub unsafe fn command(window: HWND, wparam: usize, lparam: isize) -> bool {
    let Some(command) = MiniWindowCommand::decode(wparam, lparam) else {
        return false;
    };
    let mini = minis_of(window).into_iter().next();
    let done = match (command, mini) {
        (MiniWindowCommand::PopOut, _) => pop_out_focused(window),
        (MiniWindowCommand::Move { x, y }, Some(mini)) => move_to(mini, x as i32, y as i32),
        (MiniWindowCommand::Resize { width, height }, Some(mini)) => {
            resize_to(mini, width as i32, height as i32)
        }
        (MiniWindowCommand::Return(edge), Some(mini)) => return_to(mini, edge),
        (MiniWindowCommand::Close, Some(mini)) => close(mini),
        (_, None) => false,
    };
    crate::trace::trace(&format!("mini window: command {command:?}: {done}"));
    done
}

/// A split pane's grip was released at client pixel `point`. Outside the pages, that page becomes
/// a Mini Window. Returns whether it did.
pub unsafe fn grip_released(window: HWND, index: usize, x: i32, y: i32) -> bool {
    let factor = crate::frame::layout_dpi(window) as f32 / 96.0;
    let Some(area) = area(window) else {
        return false;
    };
    outside(area, x as f32 / factor, y as f32 / factor) && pop_out_pane(window, index)
}

/// The cursor as a client point of `owner`, in pixels and in DIPs.
unsafe fn cursor_in(owner: HWND) -> (windows_sys::Win32::Foundation::POINT, f32, f32) {
    let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point);
    windows_sys::Win32::Graphics::Gdi::ScreenToClient(owner, &mut point);
    let factor = crate::frame::layout_dpi(owner) as f32 / 96.0;
    (point, point.x as f32 / factor, point.y as f32 / factor)
}

/// The row point of a client `lparam` of the Mini Window, in DIPs, and the row's width.
unsafe fn local(mini: HWND, lparam: LPARAM) -> (f32, f32, f32) {
    let factor = scale_of(mini);
    let mut client: RECT = std::mem::zeroed();
    windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(mini, &mut client);
    let x = (lparam & 0xffff) as i16 as f32 / factor;
    let y = ((lparam >> 16) & 0xffff) as i16 as f32 / factor;
    (x, y, client.right as f32 / factor)
}

unsafe fn call_original(mini: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CallWindowProcW, DefWindowProcW, WNDPROC};
    match with(mini, |state| state.original) {
        Some(original) if original != 0 => {
            let original: WNDPROC = std::mem::transmute(original);
            CallWindowProcW(original, mini, message, wparam, lparam)
        }
        _ => DefWindowProcW(mini, message, wparam, lparam),
    }
}

/// The Mini Window's own messages: its row's hit test, buttons and grip, its close, and its
/// repaint. Everything else, the page's input among it, goes to the browser.
unsafe extern "system" fn mini_proc(
    mini: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows_sys::Win32::Foundation::POINT as WinPoint;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, KillTimer, SetWindowLongPtrW, GWLP_WNDPROC, HTCAPTION, WM_CLOSE,
        WM_DPICHANGED, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MOUSEMOVE, WM_MOVING, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_SIZE, WM_TIMER,
    };
    // `WM_MOUSELEAVE`, which `windows-sys` does not export.
    const WM_MOUSELEAVE: u32 = 0x02A3;
    if !is_mini(mini) {
        return call_original(mini, message, wparam, lparam);
    }
    match message {
        WM_NCHITTEST => {
            let original = call_original(mini, message, wparam, lparam);
            let mut point = WinPoint {
                x: (lparam & 0xffff) as i16 as i32,
                y: ((lparam >> 16) & 0xffff) as i16 as i32,
            };
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(mini, &mut point);
            let client = (point.y as isize & 0xffff) << 16 | (point.x as isize & 0xffff);
            let (x, y, width) = local(mini, client);
            nc_answer(row_hit(width, x, y), original)
        }
        WM_NCLBUTTONDOWN => {
            if wparam == HTCAPTION as usize {
                let mut point = WinPoint {
                    x: (lparam & 0xffff) as i16 as i32,
                    y: ((lparam >> 16) & 0xffff) as i16 as i32,
                };
                windows_sys::Win32::Graphics::Gdi::ScreenToClient(mini, &mut point);
                let client = (point.y as isize & 0xffff) << 16 | (point.x as isize & 0xffff);
                let (x, y, width) = local(mini, client);
                let grip = row_hit(width, x, y) == Some(Hit::Grip);
                with(mini, |state| state.grip_drag = grip);
            }
            call_original(mini, message, wparam, lparam)
        }
        WM_ENTERSIZEMOVE => {
            with(mini, |state| state.moving = false);
            call_original(mini, message, wparam, lparam)
        }
        WM_MOVING => {
            let (owner, grip) = with(mini, |state| {
                state.moving = true;
                (state.owner, state.grip_drag)
            })
            .unwrap_or((std::ptr::null_mut(), false));
            if grip && crate::frame::attached_window(owner) {
                // The grip drops into the pages, so where it would land is highlighted.
                let (point, x, y) = cursor_in(owner);
                let over = area(owner).is_some_and(|area| !outside(area, x, y));
                crate::frame::show_drop_zone(owner, over.then_some(point));
            }
            call_original(mini, message, wparam, lparam)
        }
        WM_EXITSIZEMOVE => {
            let result = call_original(mini, message, wparam, lparam);
            let (owner, grip, moved) = with(mini, |state| {
                let taken = (state.owner, state.grip_drag, state.moving);
                state.grip_drag = false;
                state.moving = false;
                taken
            })
            .unwrap_or((std::ptr::null_mut(), false, false));
            if grip && crate::frame::attached_window(owner) {
                crate::frame::show_drop_zone(owner, None);
                if moved {
                    let (_, x, y) = cursor_in(owner);
                    match area(owner).and_then(|area| drop_target(area, x, y)) {
                        Some(Some(edge)) => {
                            return_to(mini, edge);
                        }
                        Some(None) => {
                            open_here(mini);
                        }
                        None => {}
                    }
                }
            }
            result
        }
        WM_LBUTTONDOWN => {
            let (x, y, width) = local(mini, lparam);
            match row_hit(width, x, y) {
                Some(hit @ (Hit::Return | Hit::OpenTab | Hit::Close)) => {
                    with(mini, |state| state.pressed = Some(hit));
                    SetCapture(mini);
                    0
                }
                _ => call_original(mini, message, wparam, lparam),
            }
        }
        WM_LBUTTONUP => {
            let Some(pressed) = with(mini, |state| state.pressed.take()).flatten() else {
                return call_original(mini, message, wparam, lparam);
            };
            ReleaseCapture();
            let (x, y, width) = local(mini, lparam);
            if row_hit(width, x, y) == Some(pressed) {
                match pressed {
                    Hit::Return => {
                        return_to(mini, Edge::Right);
                    }
                    Hit::OpenTab => {
                        open_here(mini);
                    }
                    Hit::Close => {
                        close(mini);
                    }
                    _ => {}
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let (x, y, width) = local(mini, lparam);
            let now = row_hit(width, x, y);
            let changed = with(mini, |state| {
                std::mem::replace(&mut state.hover, now) != now
            })
            .unwrap_or(false);
            if changed {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: mini,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
                paint(mini);
            }
            call_original(mini, message, wparam, lparam)
        }
        WM_MOUSELEAVE => {
            if with(mini, |state| state.hover.take().is_some()).unwrap_or(false) {
                paint(mini);
            }
            call_original(mini, message, wparam, lparam)
        }
        WM_TIMER if wparam == TITLE_TIMER => {
            paint(mini);
            0
        }
        WM_SIZE | WM_DPICHANGED => {
            let result = call_original(mini, message, wparam, lparam);
            paint(mini);
            result
        }
        WM_CLOSE => {
            // Closing the window closes the Mini Window, and its page goes back to its tab.
            let releasing = with(mini, |state| state.releasing).unwrap_or(true);
            if !releasing && close(mini) {
                return 0;
            }
            call_original(mini, message, wparam, lparam)
        }
        WM_NCDESTROY => {
            KillTimer(mini, TITLE_TIMER);
            let state = MINIS.with(|all| all.borrow_mut().remove(&(mini as isize)));
            let Some(state) = state else {
                return 0;
            };
            if GetWindowLongPtrW(mini, GWLP_WNDPROC) == mini_proc as *const () as isize {
                SetWindowLongPtrW(mini, GWLP_WNDPROC, state.original);
            }
            crate::trace::trace("mini window: its window is gone");
            let original: windows_sys::Win32::UI::WindowsAndMessaging::WNDPROC =
                std::mem::transmute(state.original);
            if crate::frame::attached_window(state.owner) {
                crate::frame::update_appbar(state.owner);
            }
            windows_sys::Win32::UI::WindowsAndMessaging::CallWindowProcW(
                original, mini, message, wparam, lparam,
            )
        }
        _ => call_original(mini, message, wparam, lparam),
    }
}

/// The row's title: the page's address, or its title when it has none.
unsafe fn title_of(mini: HWND) -> String {
    let Some(tab) = crate::features::tabs::open_tabs(mini).and_then(|tabs| tabs.into_iter().next())
    else {
        return String::new();
    };
    let url = tab
        .saved
        .url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_owned();
    if url.is_empty() {
        tab.saved.title
    } else {
        url
    }
}

/// Paints the Mini Window's row on its own surface.
unsafe fn paint(mini: HWND) {
    let mut client: RECT = std::mem::zeroed();
    windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(mini, &mut client);
    let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(mini).max(96);
    let key = Key {
        width: (client.right as f32 * 96.0 / dpi as f32).round() as i32,
        dpi,
        title: title_of(mini),
        hover: with(mini, |state| state.hover).flatten(),
    };
    let result = with(mini, |state| -> Result<()> {
        if state.surface.is_none() {
            state.surface = Some(Surface::new(mini)?);
        }
        state.surface.as_mut().expect("made above").paint(key)
    });
    if let Some(Err(error)) = result {
        crate::trace::trace(&format!("mini window: could not draw: {error}"));
    }
}

fn rgb(value: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((value >> 16) & 0xff) as f32 / 255.0,
        g: ((value >> 8) & 0xff) as f32 / 255.0,
        b: (value & 0xff) as f32 / 255.0,
        a: alpha,
    }
}

fn area_rect(x: f32, y: f32, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

impl Surface {
    /// A composition target of the Mini Window itself, like the appbar's of a Rivet window.
    unsafe fn new(mini: HWND) -> Result<Self> {
        let mut d3d: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )?;
        let dxgi: IDXGIDevice = d3d.expect("D3D11CreateDevice returned a device").cast()?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d: ID2D1Device = factory.CreateDevice(&dxgi)?;
        let device: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d)?;
        let hwnd = WinHwnd(mini as _);
        let target = device
            .CreateTargetForHwnd(hwnd, true)
            .or_else(|_| device.CreateTargetForHwnd(hwnd, false))?;
        let visual = device.CreateVisual()?;
        target.SetRoot(&visual)?;
        let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        Ok(Self {
            device,
            _target: target,
            dwrite,
            visual,
            key: None,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one line of the row is drawn from all of these at once"
    )]
    unsafe fn text(
        &self,
        context: &ID2D1DeviceContext,
        text: &str,
        font: &str,
        size: f32,
        weight: i32,
        area: D2D_RECT_F,
        colour: D2D1_COLOR_F,
        centred: bool,
    ) -> Result<()> {
        let locale = HSTRING::from("en-us");
        let format = self.dwrite.CreateTextFormat(
            &HSTRING::from(font),
            None,
            DWRITE_FONT_WEIGHT(weight),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            PCWSTR(locale.as_ptr()),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        if centred {
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = self.dwrite.CreateTextLayout(
            &wide,
            &format,
            area.right - area.left,
            area.bottom - area.top,
        )?;
        let brush = context.CreateSolidColorBrush(&colour, None)?;
        context.DrawTextLayout(
            Vector2 {
                X: area.left,
                Y: area.top,
            },
            &layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
        );
        Ok(())
    }

    /// The row: the grip, the page's address, then the return, open-as-tab and close buttons.
    unsafe fn draw(&self, context: &ID2D1DeviceContext, key: &Key) -> Result<()> {
        let width = key.width as f32;
        let row = ROW as f32;
        let paper = context.CreateSolidColorBrush(&rgb(PAPER, 1.0), None)?;
        context.FillRectangle(&area_rect(0.0, 0.0, width, row), &paper);
        let [back, tab, close] = row_buttons(width);
        self.text(
            context,
            &format!("Mini Window \u{00b7} {}", key.title),
            "Segoe UI Variable Text",
            12.0,
            600,
            area_rect(
                GRIP_X + BUTTON + 4.0,
                0.0,
                back - GRIP_X - BUTTON - 8.0,
                row,
            ),
            rgb(INK, 1.0),
            false,
        )?;
        for (hit, x, glyph) in [
            (Hit::Grip, GRIP_X, GRIP_GLYPH),
            (Hit::Return, back, RETURN_GLYPH),
            (Hit::OpenTab, tab, TAB_GLYPH),
            (Hit::Close, close, CLOSE_GLYPH),
        ] {
            let button = area_rect(x, (row - BUTTON) / 2.0, BUTTON, BUTTON);
            if key.hover == Some(hit) {
                let brush = context.CreateSolidColorBrush(&rgb(INK, 0.07), None)?;
                context.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT {
                        rect: button,
                        radiusX: 6.0,
                        radiusY: 6.0,
                    },
                    &brush,
                );
            }
            self.text(
                context,
                &glyph.to_string(),
                ICONS,
                10.0,
                400,
                button,
                rgb(SOFT_INK, 1.0),
                true,
            )?;
        }
        Ok(())
    }

    unsafe fn paint(&mut self, key: Key) -> Result<()> {
        if self.key.as_ref() == Some(&key) {
            return Ok(());
        }
        let factor = key.dpi as f32 / 96.0;
        let px = |value: f32| (value * factor).round().max(1.0) as u32;
        let surface = self.device.CreateSurface(
            px(key.width as f32),
            px(ROW as f32),
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_ALPHA_MODE_PREMULTIPLIED,
        )?;
        let mut offset = POINT::default();
        let context: ID2D1DeviceContext = surface.BeginDraw(None, &mut offset)?;
        context.SetDpi(key.dpi as f32, key.dpi as f32);
        context.SetTransform(&Matrix3x2::translation(
            offset.x as f32 / factor,
            offset.y as f32 / factor,
        ));
        context.Clear(Some(&rgb(0, 0.0)));
        let drawn = self.draw(&context, &key);
        surface.EndDraw()?;
        drawn?;
        self.visual.SetContent(&surface)?;
        self.key = Some(key);
        self.device.Commit()
    }
}

/// Forgets a Rivet window that is going away. Its Mini Windows stay as browser windows of their
/// own; a later return goes to another Rivet window.
pub fn forget(_window: HWND) {}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: [i32; 4] = [100, 80, 1100, 780];

    #[test]
    fn fr9_a_new_mini_window_is_small_and_starts_at_the_lower_right_of_its_pages() {
        let rect = default_rect(AREA);
        assert_eq!(rect, [1100 - 24 - 448, 780 - 24 - 296, 448, 296]);
        assert_eq!(DEFAULT_SIZE, [448, 296]);
    }

    #[test]
    fn fr9_a_mini_window_is_placed_relative_to_its_rivet_window_even_off_every_monitor() {
        // A Rivet window far off the monitors keeps its Mini Window beside it, not on a monitor.
        let origin = [9000, 1000];
        assert_eq!(
            on_screen(origin, [652, 460, 448, 296], 1.5),
            [9000 + 978, 1000 + 690, 672, 444]
        );
        assert_eq!(
            on_screen([0, 0], [0, 0, 10, 10], 1.0),
            [0, 0, MIN_SIZE[0], MIN_SIZE[1]],
            "it keeps its least size"
        );
    }

    #[test]
    fn fr9_the_row_moves_the_window_and_its_buttons_return_open_or_close_it() {
        let width = 448.0;
        assert_eq!(row_hit(width, 150.0, 15.0), Some(Hit::Row));
        assert_eq!(row_hit(width, GRIP_X + 2.0, 15.0), Some(Hit::Grip));
        let [back, tab, close] = row_buttons(width);
        assert_eq!(row_hit(width, back + 4.0, 15.0), Some(Hit::Return));
        assert_eq!(row_hit(width, tab + 4.0, 15.0), Some(Hit::OpenTab));
        assert_eq!(row_hit(width, close + 4.0, 15.0), Some(Hit::Close));
        assert_eq!(
            row_hit(width, 150.0, ROW as f32 + 1.0),
            None,
            "the page's input stays the browser's"
        );
    }

    #[test]
    fn fr9_the_row_is_the_mini_windows_caption_and_keeps_the_browsers_resize_borders() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HTCAPTION, HTCLIENT, HTLEFT, HTTOPRIGHT,
        };
        assert_eq!(
            nc_answer(Some(Hit::Row), HTCLIENT as isize),
            HTCAPTION as isize
        );
        assert_eq!(
            nc_answer(Some(Hit::Grip), HTCLIENT as isize),
            HTCAPTION as isize
        );
        assert_eq!(
            nc_answer(Some(Hit::Close), HTCAPTION as isize),
            HTCLIENT as isize
        );
        assert_eq!(
            nc_answer(Some(Hit::Row), HTTOPRIGHT as isize),
            HTTOPRIGHT as isize
        );
        assert_eq!(nc_answer(None, HTLEFT as isize), HTLEFT as isize);
        assert_eq!(nc_answer(None, HTCLIENT as isize), HTCLIENT as isize);
    }

    #[test]
    fn fr9_a_mini_window_dropped_on_a_page_edge_returns_as_a_split_there() {
        assert_eq!(drop_target(AREA, 110.0, 400.0), Some(Some(Edge::Left)));
        assert_eq!(drop_target(AREA, 1090.0, 400.0), Some(Some(Edge::Right)));
        assert_eq!(drop_target(AREA, 600.0, 90.0), Some(Some(Edge::Top)));
        assert_eq!(drop_target(AREA, 600.0, 770.0), Some(Some(Edge::Bottom)));
        assert_eq!(
            drop_target(AREA, 600.0, 400.0),
            Some(None),
            "a drop over the middle opens the page there"
        );
        assert_eq!(
            drop_target(AREA, 50.0, 400.0),
            None,
            "off the pages it stays a Mini Window"
        );
        assert_eq!(side_of(Edge::Top), Side::Top);
    }

    #[test]
    fn fr9_a_split_pane_dragged_outside_the_pages_becomes_a_mini_window() {
        assert!(outside(AREA, 50.0, 400.0));
        assert!(outside(AREA, 600.0, 790.0));
        assert!(!outside(AREA, 600.0, 400.0));
    }
}
