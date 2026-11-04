//! The Mini Window: one page floating over the pages of a Rivet window.
//!
//! The page is a tab of the window shown in a bare page view of the browser view, placed over the
//! page area like the second view of a split. Its frame and its small row are a visual of the
//! window's own surface, with a hole where the page is, and take their input from the window's own
//! messages. Nothing here is a window of its own.
//!
//! The row moves it, its edges resize it, and its buttons open its page in the window or
//! close it. Dragged by its grip, like a split pane's, it drops into the pages: near an edge it
//! splits the page there and elsewhere over the pages it opens there, as a dragged tab does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use windows::core::{Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    ID2D1DeviceContext, D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectComposition::{
    IDCompositionDesktopDevice, IDCompositionVisual, IDCompositionVisual2,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory5, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows_numerics::{Matrix3x2, Vector2};
use windows_sys::Win32::Foundation::HWND;

use crate::appbar::Appbar;
use crate::features::second_view::bare;
use crate::features::split::Side;
use rivet_protocol::window::{Edge, MiniWindowCommand};

/// The height of the Mini Window's own row, in DIPs.
pub const ROW: i32 = 32;
/// The size a new Mini Window takes, as the demo's `#mini`.
pub const DEFAULT_SIZE: [i32; 2] = [360, 270];
/// The smallest Mini Window.
pub const MIN_SIZE: [i32; 2] = [220, 140];
/// The gap a new Mini Window keeps from the right and bottom of the pages.
const MARGIN: i32 = 24;
/// How far outside and inside the frame a press grabs an edge to resize.
const GRAB: f32 = 6.0;
/// The buttons of the row, and the room they take at its right end.
const BUTTON: f32 = 28.0;
const BUTTON_PAD: f32 = 4.0;
/// Where the grip starts in the row, just past the left edge's resize band.
pub const GRIP_X: f32 = GRAB;
const GRIP_GLYPH: char = '\u{E7C2}';
/// The frame's corner radius and the room around it for its shadow.
const RADIUS: f32 = 10.0;
const SHADOW: f32 = 24.0;

const PAPER: u32 = 0xfbfaf7;
const INK: u32 = 0x1f2024;
const SOFT_INK: u32 = 0x4a4850;
const ICONS: &str = "Segoe Fluent Icons";
/// Open in the window, and close.
const RETURN_GLYPH: char = '\u{E73F}';
const CLOSE_GLYPH: char = '\u{E8BB}';

/// Which edges a press on the frame resizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// What a point of the Mini Window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// The row outside its buttons, which only moves the window.
    Row,
    /// The grip at the row's left end. A drag by it moves the window and drops it into the pages
    /// as a split pane's grip does: on an edge it splits there, in the middle it opens there.
    Grip,
    /// The button that opens the page in the window, in place of the page shown there.
    Return,
    Close,
    Resize(Edges),
    /// The page itself, whose input belongs to the browser.
    Page,
}

/// Where a new Mini Window goes in the pages' area `area` (left, top, right, bottom): at the lower
/// right, as x, y, width and height.
pub fn default_rect(area: [i32; 4]) -> [i32; 4] {
    let [width, height] = DEFAULT_SIZE;
    clamp(
        [
            area[2] - MARGIN - width,
            area[3] - MARGIN - height,
            width,
            height,
        ],
        area,
    )
}

/// Keeps a Mini Window inside the pages' area: no smaller than the least size, no larger than the
/// area, and wholly inside it, so its row can always be reached.
pub fn clamp(rect: [i32; 4], area: [i32; 4]) -> [i32; 4] {
    let (room_w, room_h) = ((area[2] - area[0]).max(1), (area[3] - area[1]).max(1));
    let width = rect[2].max(MIN_SIZE[0]).min(room_w);
    let height = rect[3].max(MIN_SIZE[1]).min(room_h);
    let x = rect[0].min(area[2] - width).max(area[0]);
    let y = rect[1].min(area[3] - height).max(area[1]);
    [x, y, width, height]
}

/// The page inside a Mini Window: below the row, inside a one DIP frame.
pub fn page_rect(rect: [i32; 4]) -> [i32; 4] {
    [
        rect[0] + 1,
        rect[1] + ROW,
        (rect[2] - 2).max(1),
        (rect[3] - ROW - 1).max(1),
    ]
}

/// The return and close buttons of the row as x offsets from the window's left edge.
pub fn row_buttons(width: f32) -> (f32, f32) {
    let close = width - BUTTON_PAD - BUTTON;
    (close - BUTTON, close)
}

/// What the point (`x`, `y`) is on a Mini Window at `rect`, or none when it is off the window.
pub fn hit(rect: [i32; 4], x: f32, y: f32) -> Option<Hit> {
    let [left, top, width, height] = rect.map(|value| value as f32);
    let (right, bottom) = (left + width, top + height);
    if x < left - GRAB || x >= right + GRAB || y < top - GRAB || y >= bottom + GRAB {
        return None;
    }
    let edges = Edges {
        left: x < left + GRAB,
        right: x >= right - GRAB,
        top: y < top + GRAB / 2.0,
        bottom: y >= bottom - GRAB,
    };
    if edges != Edges::default() {
        return Some(Hit::Resize(edges));
    }
    if y < top + ROW as f32 {
        let (back, close) = row_buttons(width);
        let local = x - left;
        return Some(if local < GRIP_X + BUTTON {
            Hit::Grip
        } else if local >= close {
            Hit::Close
        } else if local >= back {
            Hit::Return
        } else {
            Hit::Row
        });
    }
    Some(Hit::Page)
}

/// The rectangle a drag of `edges` by (`dx`, `dy`) makes of `start`. The opposite edges stay put,
/// and the window keeps its least size.
pub fn resized(start: [i32; 4], edges: Edges, dx: i32, dy: i32) -> [i32; 4] {
    let [mut left, mut top, width, height] = start;
    let (mut right, mut bottom) = (left + width, top + height);
    if edges.left {
        left = (left + dx).min(right - MIN_SIZE[0]);
    }
    if edges.right {
        right = (right + dx).max(left + MIN_SIZE[0]);
    }
    if edges.top {
        top = (top + dy).min(bottom - MIN_SIZE[1]);
    }
    if edges.bottom {
        bottom = (bottom + dy).max(top + MIN_SIZE[1]);
    }
    [left, top, right - left, bottom - top]
}

/// Where a Mini Window dropped at (`x`, `y`) lands, as a tab dropped there would: near an edge of
/// the pages it returns as a split on that edge, elsewhere over the pages it opens there in place
/// of the page, and off the pages it stays a Mini Window (none).
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

/// Opens the Mini Window's page in the window's page: its tab becomes the active one.
pub unsafe fn open_here(window: HWND) -> bool {
    let Some(page) = with(window, |mini| mini.page) else {
        return false;
    };
    let tab = tab_of(window, page);
    close(window);
    let done = tab.is_some_and(|tab| crate::features::tabs::activate_tab(window, tab));
    crate::trace::trace(&format!(
        "mini window: opened tab {tab:?} in the page: {done}"
    ));
    crate::frame::update_appbar(window);
    done
}

/// The cursor that shows which edges a press resizes.
pub fn cursor_of(edges: Edges) -> windows_sys::core::PCWSTR {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
    };
    match (edges.left || edges.right, edges.top || edges.bottom) {
        (true, true) if (edges.left && edges.top) || (edges.right && edges.bottom) => IDC_SIZENWSE,
        (true, true) => IDC_SIZENESW,
        (true, false) => IDC_SIZEWE,
        _ => IDC_SIZENS,
    }
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

/// What a drag on the frame does.
#[derive(Clone, Copy, Debug)]
struct Drag {
    hit: Hit,
    /// The pointer where the press began, in window DIPs, and the window then.
    start: [f32; 2],
    rect: [i32; 4],
    moved: bool,
}

/// What the visual was painted for.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    width: i32,
    height: i32,
    dpi: u32,
    title: String,
    hover: Option<Hit>,
    lifted: bool,
}

/// One window's Mini Window.
struct Mini {
    view: usize,
    parent: usize,
    /// The page shown, which is a tab of the window.
    page: usize,
    /// The page the window's own page view shows behind it.
    behind: usize,
    /// x, y, width and height in window DIPs.
    rect: [i32; 4],
    drag: Option<Drag>,
    hover: Option<Hit>,
    /// When the page was last clicked, so a tab activation that follows the click is undone.
    pressed: Option<Instant>,
    surface: Option<Surface>,
}

struct Surface {
    device: IDCompositionDesktopDevice,
    dwrite: IDWriteFactory5,
    visual: IDCompositionVisual2,
    key: Option<Key>,
}

thread_local! {
    static MINIS: RefCell<HashMap<isize, Mini>> = RefCell::new(HashMap::new());
}

fn with<R>(window: HWND, act: impl FnOnce(&mut Mini) -> R) -> Option<R> {
    MINIS.with(|all| all.borrow_mut().get_mut(&(window as isize)).map(act))
}

/// Whether the window shows a Mini Window.
pub fn is_open(window: HWND) -> bool {
    MINIS.with(|all| all.borrow().contains_key(&(window as isize)))
}

/// The Mini Window's rectangle in window DIPs.
pub fn rect_of(window: HWND) -> Option<[i32; 4]> {
    with(window, |mini| mini.rect)
}

/// The pages' area of the window in DIPs as left, top, right and bottom.
unsafe fn area(window: HWND) -> Option<[i32; 4]> {
    crate::features::split::page_area(window)
}

fn scale(window: HWND) -> f32 {
    unsafe { crate::frame::layout_dpi(window) as f32 / 96.0 }
}

/// The strip index of the tab that holds `page`.
unsafe fn tab_of(window: HWND, page: usize) -> Option<usize> {
    crate::features::tabs::open_tabs(window)?
        .iter()
        .position(|tab| tab.id == page)
}

/// Shows the window's tab `tab` as the Mini Window. The tab must not be the active one, since the
/// window's own page view shows that page. A Mini Window already open closes first.
pub unsafe fn open(window: HWND, tab: usize) -> bool {
    if is_open(window) {
        close(window);
    }
    let page = bare::page_of_tab(window, tab);
    let behind = bare::active(window);
    if page == 0 || page == behind {
        crate::trace::trace(&format!(
            "mini window: tab {tab} cannot float, it is the shown page"
        ));
        return false;
    }
    let Some(area) = area(window) else {
        return false;
    };
    let Some((view, parent)) = bare::new_view(window, page) else {
        crate::trace::trace("mini window: no page view for this build");
        return false;
    };
    let rect = default_rect(area);
    bare::place(view, parent, page_rect(rect));
    let mini = Mini {
        view,
        parent,
        page,
        behind,
        rect,
        drag: None,
        hover: None,
        pressed: None,
        surface: None,
    };
    MINIS.with(|all| all.borrow_mut().insert(window as isize, mini));
    crate::trace::trace(&format!("mini window: open tab {tab} at {rect:?}"));
    crate::frame::update_appbar(window);
    true
}

/// Takes the page of split pane `index` out of the split and floats it. The other page stays as
/// the window's page.
pub unsafe fn pop_out_pane(window: HWND, index: usize) -> bool {
    let panes = crate::features::split::panes(window);
    let Some(tab) = panes.get(index).and_then(|pane| pane.tab) else {
        return false;
    };
    // A split on Rivet's own page views keeps its other pages.
    if let Some(slot) = crate::features::second_view::slot_of_pane(window, &panes[index]) {
        let done = crate::features::split::pop_out(window, slot);
        crate::trace::trace(&format!("mini window: pop out page {index} of the split: {done}"));
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

/// Floats the window's active page. The window shows the tab beside it, or a new tab when it has
/// only this one, so the page is never shown twice.
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

/// Floats the focused page: the focused pane of a split, or the window's only page.
pub unsafe fn pop_out_focused(window: HWND) -> bool {
    let panes = crate::features::split::panes(window);
    match panes.iter().position(|pane| pane.active) {
        Some(index) if !panes.is_empty() => pop_out_pane(window, index),
        _ => pop_out_active(window),
    }
}

/// Removes the Mini Window. Its page stays with its tab.
pub unsafe fn close(window: HWND) -> bool {
    let Some(mini) = MINIS.with(|all| all.borrow_mut().remove(&(window as isize))) else {
        return false;
    };
    bare::drop_view(mini.view, mini.parent);
    if let Some(surface) = &mini.surface {
        let _ = surface.visual.SetContent(None);
        let _ = surface.device.Commit();
    }
    crate::trace::trace("mini window: closed, the page stays in its tab");
    crate::frame::update_appbar(window);
    true
}

/// Sends the Mini Window's page back to its tab as a split on `edge` of the window's page.
pub unsafe fn return_to(window: HWND, edge: Edge) -> bool {
    let Some(page) = with(window, |mini| mini.page) else {
        return false;
    };
    let Some(tab) = tab_of(window, page) else {
        return close(window);
    };
    close(window);
    let done = crate::features::split::split_beside_page(window, tab, side_of(edge), None);
    crate::trace::trace(&format!(
        "mini window: returned tab {tab} to the {edge:?} edge: {done}"
    ));
    crate::frame::update_appbar(window);
    done
}

/// Moves the Mini Window's top left corner to (`x`, `y`) in window DIPs.
pub unsafe fn move_to(window: HWND, x: i32, y: i32) -> bool {
    set_rect(window, |rect| [x, y, rect[2], rect[3]])
}

/// Gives the Mini Window a new size, keeping its top left corner.
pub unsafe fn resize_to(window: HWND, width: i32, height: i32) -> bool {
    set_rect(window, |rect| [rect[0], rect[1], width, height])
}

unsafe fn set_rect(window: HWND, change: impl FnOnce([i32; 4]) -> [i32; 4]) -> bool {
    let Some(area) = area(window) else {
        return false;
    };
    let Some(rect) = with(window, |mini| {
        mini.rect = clamp(change(mini.rect), area);
        bare::place(mini.view, mini.parent, page_rect(mini.rect));
        mini.rect
    }) else {
        return false;
    };
    crate::trace::trace(&format!("mini window: at {rect:?}"));
    crate::frame::update_appbar(window);
    true
}

/// Runs a command posted to the window as the Mini Window message.
pub unsafe fn command(window: HWND, wparam: usize, lparam: isize) -> bool {
    let Some(command) = MiniWindowCommand::decode(wparam, lparam) else {
        return false;
    };
    let done = match command {
        MiniWindowCommand::PopOut => pop_out_focused(window),
        MiniWindowCommand::Move { x, y } => move_to(window, x as i32, y as i32),
        MiniWindowCommand::Resize { width, height } => {
            resize_to(window, width as i32, height as i32)
        }
        MiniWindowCommand::Return(edge) => return_to(window, edge),
        MiniWindowCommand::Close => close(window),
    };
    crate::trace::trace(&format!("mini window: command {command:?}: {done}"));
    done
}

/// A split pane's grip was released at client pixel `point`. Outside the pages, that page becomes
/// the Mini Window. Returns whether it did.
pub unsafe fn grip_released(window: HWND, index: usize, x: i32, y: i32) -> bool {
    let factor = scale(window);
    let Some(area) = area(window) else {
        return false;
    };
    outside(area, x as f32 / factor, y as f32 / factor) && pop_out_pane(window, index)
}

/// Keeps the page in the Mini Window when the browser makes its tab active. A click in the page
/// does that, and the page goes back to the Mini Window. A tab chosen in the tab list shows the
/// page in the window, so the Mini Window closes.
unsafe fn keep_page(window: HWND) {
    let Some((view, page, behind, pressed)) = with(window, |mini| {
        (mini.view, mini.page, mini.behind, mini.pressed)
    }) else {
        return;
    };
    let active = bare::active(window);
    if active == 0 || active == behind {
        return;
    }
    if active != page {
        with(window, |mini| mini.behind = active);
        return;
    }
    let clicked = pressed.is_some_and(|at| at.elapsed() < Duration::from_millis(1500));
    if clicked && bare::take_back(window, view, page, behind) {
        bare::focus(view);
        crate::trace::trace("mini window: kept the page after its tab became active");
    } else {
        // The page view shows the page now; the Mini Window lets go without touching it.
        if let Some(mini) = MINIS.with(|all| all.borrow_mut().remove(&(window as isize))) {
            if let Some(surface) = &mini.surface {
                let _ = surface.visual.SetContent(None);
            }
            bare::drop_view(mini.view, mini.parent);
        }
        crate::trace::trace("mini window: its tab was chosen, so the page is back in the window");
    }
}

/// The mouse over the Mini Window's frame and row. Returns the result when Rivet took the message;
/// presses in the page belong to the browser.
pub unsafe fn on_mouse(window: HWND, message: u32, lparam: isize) -> Option<isize> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LoadCursorW, SetCursor, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WM_SETCURSOR,
    };
    if !is_open(window) {
        return None;
    }
    let factor = scale(window);
    let x = (lparam & 0xffff) as i16 as f32 / factor;
    let y = ((lparam >> 16) & 0xffff) as i16 as f32 / factor;
    match message {
        WM_SETCURSOR => {
            // The edges show the resize cursor while the pointer is on them or drags them.
            let edges = with(window, |mini| {
                match mini.drag.map(|drag| drag.hit).or(mini.hover) {
                    Some(Hit::Resize(edges)) => Some(edges),
                    _ => None,
                }
            })??;
            SetCursor(LoadCursorW(std::ptr::null_mut(), cursor_of(edges)));
            Some(1)
        }
        WM_LBUTTONDOWN => {
            let target = with(window, |mini| {
                let target = hit(mini.rect, x, y);
                match target {
                    Some(Hit::Page) => mini.pressed = Some(Instant::now()),
                    Some(hit) => {
                        mini.drag = Some(Drag {
                            hit,
                            start: [x, y],
                            rect: mini.rect,
                            moved: false,
                        })
                    }
                    None => {}
                }
                target
            })??;
            if target == Hit::Page {
                return None;
            }
            SetCapture(window);
            Some(0)
        }
        WM_MOUSEMOVE => {
            let drag = with(window, |mini| mini.drag).flatten();
            let Some(drag) = drag else {
                let changed = with(window, |mini| {
                    let now = hit(mini.rect, x, y).filter(|hit| *hit != Hit::Page);
                    std::mem::replace(&mut mini.hover, now) != now
                })?;
                if changed {
                    crate::frame::update_appbar(window);
                }
                return None;
            };
            let (dx, dy) = (
                (x - drag.start[0]).round() as i32,
                (y - drag.start[1]).round() as i32,
            );
            let next = match drag.hit {
                Hit::Row | Hit::Grip => Some([
                    drag.rect[0] + dx,
                    drag.rect[1] + dy,
                    drag.rect[2],
                    drag.rect[3],
                ]),
                Hit::Resize(edges) => Some(resized(drag.rect, edges, dx, dy)),
                _ => None,
            };
            if let Some(next) = next {
                with(window, |mini| {
                    if let Some(live) = mini.drag.as_mut() {
                        live.moved = true;
                    }
                });
                set_rect(window, |_| next);
            }
            if drag.hit == Hit::Grip {
                // The grip drops into the pages, so where it would land is highlighted.
                let point = windows_sys::Win32::Foundation::POINT {
                    x: (x * factor) as i32,
                    y: (y * factor) as i32,
                };
                crate::frame::show_drop_zone(window, Some(point));
            }
            Some(0)
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED => {
            let drag = with(window, |mini| mini.drag.take()).flatten()?;
            if drag.hit == Hit::Grip {
                crate::frame::show_drop_zone(window, None);
            }
            if message == WM_LBUTTONUP {
                ReleaseCapture();
            } else {
                return Some(0);
            }
            match drag.hit {
                Hit::Close if hit(rect_of(window)?, x, y) == Some(Hit::Close) => {
                    close(window);
                }
                Hit::Return if hit(rect_of(window)?, x, y) == Some(Hit::Return) => {
                    // The button opens the page in the window, as the demo's "Open in space".
                    open_here(window);
                }
                // The row only moves the window. The grip drops it into the pages.
                Hit::Grip if drag.moved => {
                    match area(window).and_then(|area| drop_target(area, x, y)) {
                        Some(Some(edge)) => {
                            return_to(window, edge);
                        }
                        Some(None) => {
                            open_here(window);
                        }
                        None => {}
                    }
                }
                _ => {}
            }
            crate::frame::update_appbar(window);
            Some(0)
        }
        _ => None,
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
    unsafe fn new(appbar: &Appbar) -> Result<Self> {
        let (device, root, dwrite) = appbar.composition();
        let visual = device.CreateVisual()?;
        // Above every other part of the window's surface: the Mini Window floats over the pages.
        root.AddVisual(&visual, true, None::<&IDCompositionVisual>)?;
        Ok(Self {
            device,
            dwrite,
            visual,
            key: None,
        })
    }

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

    unsafe fn rounded(
        &self,
        context: &ID2D1DeviceContext,
        area: D2D_RECT_F,
        radius: f32,
        colour: D2D1_COLOR_F,
    ) -> Result<()> {
        let brush = context.CreateSolidColorBrush(&colour, None)?;
        context.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: area,
                radiusX: radius,
                radiusY: radius,
            },
            &brush,
        );
        Ok(())
    }

    /// The frame, its shadow and its row, with the page left clear.
    unsafe fn draw(
        &self,
        context: &ID2D1DeviceContext,
        key: &Key,
        width: f32,
        height: f32,
    ) -> Result<()> {
        // A soft shadow from rings of falling strength, stronger while the window is lifted.
        let strength = if key.lifted { 0.05 } else { 0.03 };
        for ring in (1..=8).rev() {
            let spread = ring as f32 * SHADOW / 8.0;
            let drop = spread * 0.4;
            self.rounded(
                context,
                area_rect(
                    SHADOW - spread,
                    SHADOW - spread + drop,
                    width + 2.0 * spread,
                    height + 2.0 * spread,
                ),
                RADIUS + spread,
                rgb(0x281c1e, strength * (1.0 - ring as f32 / 9.0)),
            )?;
        }
        let (left, top) = (SHADOW, SHADOW);
        self.rounded(
            context,
            area_rect(left - 1.0, top - 1.0, width + 2.0, height + 2.0),
            RADIUS + 1.0,
            rgb(0x000000, 0.08),
        )?;
        self.rounded(
            context,
            area_rect(left, top, width, height),
            RADIUS,
            rgb(PAPER, 1.0),
        )?;
        // The row: the page's address, then the return and close buttons.
        let row = ROW as f32;
        let (back, close) = row_buttons(width);
        self.text(
            context,
            &key.title,
            "Segoe UI Variable Text",
            12.0,
            600,
            area_rect(
                left + GRIP_X + BUTTON + 4.0,
                top,
                back - GRIP_X - BUTTON - 8.0,
                row,
            ),
            rgb(INK, 1.0),
            false,
        )?;
        for (hit, x, glyph) in [
            (Hit::Grip, GRIP_X, GRIP_GLYPH),
            (Hit::Return, back, RETURN_GLYPH),
            (Hit::Close, close, CLOSE_GLYPH),
        ] {
            let button = area_rect(left + x, top + (row - BUTTON) / 2.0, BUTTON, BUTTON);
            if key.hover == Some(hit) {
                self.rounded(context, button, 6.0, rgb(INK, 0.07))?;
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
        // The page shows through: its rectangle is cleared to nothing.
        let page = page_rect([0, 0, width as i32, height as i32]);
        let hole = area_rect(
            left + page[0] as f32,
            top + page[1] as f32,
            page[2] as f32,
            page[3] as f32,
        );
        context.PushAxisAlignedClip(&hole, D2D1_ANTIALIAS_MODE_ALIASED);
        context.Clear(Some(&rgb(0, 0.0)));
        context.PopAxisAlignedClip();
        Ok(())
    }

    unsafe fn paint(&mut self, key: Key, left: f32, top: f32) -> Result<()> {
        let factor = key.dpi as f32 / 96.0;
        self.visual
            .SetOffsetX2(((left - SHADOW) * factor).round())?;
        self.visual.SetOffsetY2(((top - SHADOW) * factor).round())?;
        if self.key.as_ref() != Some(&key) {
            let (w, h) = (key.width as f32, key.height as f32);
            let px = |value: f32| (value * factor).round().max(1.0) as u32;
            let surface = self.device.CreateSurface(
                px(w + 2.0 * SHADOW),
                px(h + 2.0 * SHADOW),
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
            let drawn = self.draw(&context, &key, w, h);
            surface.EndDraw()?;
            drawn?;
            self.visual.SetContent(&surface)?;
            self.key = Some(key);
        }
        self.device.Commit()
    }
}

/// Places the Mini Window's page and paints its frame. Called with the other visuals of the
/// window's surface.
pub unsafe fn update(window: HWND, appbar: &Appbar, visible: bool, dpi: u32) {
    if !is_open(window) {
        return;
    }
    keep_page(window);
    let area = area(window);
    let title = with(window, |mini| mini.page)
        .and_then(|page| {
            let tabs = crate::features::tabs::open_tabs(window)?;
            let tab = tabs.into_iter().find(|tab| tab.id == page)?;
            let url = tab
                .saved
                .url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .to_owned();
            Some(if url.is_empty() { tab.saved.title } else { url })
        })
        .unwrap_or_default();
    let result = with(window, |mini| -> Result<()> {
        if let Some(area) = area {
            mini.rect = clamp(mini.rect, area);
        }
        bare::place(mini.view, mini.parent, page_rect(mini.rect));
        if mini.surface.is_none() {
            mini.surface = Some(Surface::new(appbar)?);
        }
        let surface = mini.surface.as_mut().expect("made above");
        if !visible {
            surface.key = None;
            surface.visual.SetContent(None)?;
            return surface.device.Commit();
        }
        let key = Key {
            width: mini.rect[2],
            height: mini.rect[3],
            dpi,
            title: format!("Mini Window \u{00b7} {title}"),
            hover: mini.hover,
            lifted: mini.drag.is_some_and(|drag| drag.moved),
        };
        surface.paint(key, mini.rect[0] as f32, mini.rect[1] as f32)
    });
    if let Some(Err(error)) = result {
        crate::trace::trace(&format!("mini window: could not draw: {error}"));
    }
}

/// Forgets a window that is going away. The browser deletes its views with it.
pub fn forget(window: HWND) {
    MINIS.with(|all| all.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: [i32; 4] = [100, 80, 1100, 780];

    #[test]
    fn fr9_a_new_mini_window_floats_at_the_lower_right_of_the_pages() {
        let rect = default_rect(AREA);
        assert_eq!(rect, [1100 - 24 - 360, 780 - 24 - 270, 360, 270]);
    }

    #[test]
    fn fr9_a_mini_window_stays_inside_the_pages_and_keeps_its_least_size() {
        assert_eq!(clamp([-500, -500, 360, 270], AREA), [100, 80, 360, 270]);
        assert_eq!(
            clamp([5000, 5000, 360, 270], AREA),
            [1100 - 360, 780 - 270, 360, 270]
        );
        assert_eq!(
            clamp([300, 300, 10, 10], AREA),
            [300, 300, MIN_SIZE[0], MIN_SIZE[1]]
        );
        assert_eq!(clamp([0, 0, 4000, 4000], AREA), [100, 80, 1000, 700]);
    }

    #[test]
    fn fr9_the_mini_windows_page_sits_below_its_own_row() {
        assert_eq!(
            page_rect([10, 20, 360, 270]),
            [11, 20 + ROW, 358, 270 - ROW - 1]
        );
    }

    #[test]
    fn fr9_the_row_moves_the_window_and_its_buttons_return_or_close_it() {
        let rect = [100, 100, 360, 270];
        assert_eq!(
            hit(rect, 150.0, 115.0),
            Some(Hit::Row),
            "the row only moves the window"
        );
        assert_eq!(
            hit(rect, 100.0 + GRIP_X + 2.0, 115.0),
            Some(Hit::Grip),
            "the grip at the left end drops it into the pages"
        );
        assert_eq!(
            hit(rect, 100.0 + GRIP_X + BUTTON + 1.0, 115.0),
            Some(Hit::Row)
        );
        let (back, close) = row_buttons(360.0);
        assert_eq!(hit(rect, 100.0 + back + 4.0, 115.0), Some(Hit::Return));
        assert_eq!(hit(rect, 100.0 + close + 4.0, 115.0), Some(Hit::Close));
        assert_eq!(
            hit(rect, 250.0, 250.0),
            Some(Hit::Page),
            "the page's input stays the browser's"
        );
        assert_eq!(hit(rect, 50.0, 50.0), None);
    }

    #[test]
    fn fr9_the_mini_windows_edges_resize_it() {
        let rect = [100, 100, 360, 270];
        assert_eq!(
            hit(rect, 98.0, 250.0),
            Some(Hit::Resize(Edges {
                left: true,
                ..Edges::default()
            }))
        );
        assert_eq!(
            hit(rect, 461.0, 372.0),
            Some(Hit::Resize(Edges {
                right: true,
                bottom: true,
                ..Edges::default()
            }))
        );
        let edges = Edges {
            right: true,
            bottom: true,
            ..Edges::default()
        };
        assert_eq!(resized(rect, edges, 40, 30), [100, 100, 400, 300]);
        let left = Edges {
            left: true,
            ..Edges::default()
        };
        assert_eq!(
            resized(rect, left, 1000, 0),
            [100 + 360 - MIN_SIZE[0], 100, MIN_SIZE[0], 270],
            "the right edge stays"
        );
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
    }

    #[test]
    fn fr9_the_mini_windows_edges_show_the_resize_cursor_of_their_direction() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
        };
        assert_eq!(
            cursor_of(Edges {
                left: true,
                ..Edges::default()
            }),
            IDC_SIZEWE
        );
        assert_eq!(
            cursor_of(Edges {
                bottom: true,
                ..Edges::default()
            }),
            IDC_SIZENS
        );
        assert_eq!(
            cursor_of(Edges {
                right: true,
                bottom: true,
                ..Edges::default()
            }),
            IDC_SIZENWSE
        );
        assert_eq!(
            cursor_of(Edges {
                right: true,
                top: true,
                ..Edges::default()
            }),
            IDC_SIZENESW
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
