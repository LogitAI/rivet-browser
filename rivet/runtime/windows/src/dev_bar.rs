//! The developer bar: the buttons beside the caption buttons of a page in developer mode.
//!
//! The bar is a visual of the browser window (see `appbar`) whose input comes from the window's own
//! message queue. Its buttons run the browser's own commands, as its keyboard shortcuts do, so the
//! browser decides what each one does for the page. The browser's top row is cut short of the bar,
//! so nothing of the browser lies under it.

use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, PostMessageW, WM_COMMAND};

use crate::dev_mode::{self, Action};

/// What the appbar draws of the bar, in window pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    /// Where the bar's first button starts inside the row, in pixels.
    pub bar_x: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<char>,
    pub hover: Option<usize>,
}

thread_local! {
    static HOVER: RefCell<HashMap<isize, usize>> = RefCell::new(HashMap::new());
    static PRESS: RefCell<HashMap<isize, usize>> = RefCell::new(HashMap::new());
}

/// The bar of `window` as left, top, width and height in DIPs with its buttons, or none while the
/// page is not in developer mode, the build has no button to show, or the window shows a split,
/// whose rows are Rivet's own. The second rectangle is the whole top row, which the visual covers
/// to lay the stripes of developer mode over it.
unsafe fn place(window: HWND) -> Option<([f32; 4], [f32; 4], Vec<(Action, u32)>)> {
    if !crate::chrome_toolbar::window_in_dev_mode(window) || !crate::chrome_split::panes(window).is_empty() {
        return None;
    }
    let actions = crate::chrome_toolbar::dev_actions();
    if actions.is_empty() {
        return None;
    }
    // The panel holds the browser's top row at its upper edge.
    let panel = crate::chrome_layout::panel_bounds(window)?;
    let row = crate::chrome_split::ROW_HEIGHT as f32;
    let width = dev_mode::bar_width(actions.len());
    let right = client_width(window) - crate::chrome_split::CAPTION_ROOM;
    let whole = [panel.x as f32, panel.y as f32, panel.width as f32, row];
    Some(([right - width, panel.y as f32, width, row], whole, actions))
}

/// The width of the window's client area in DIPs.
unsafe fn client_width(window: HWND) -> f32 {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    client.right as f32 * 96.0 / GetDpiForWindow(window).max(96) as f32
}

fn scale(window: HWND) -> f32 {
    unsafe { GetDpiForWindow(window).max(96) as f32 / 96.0 }
}

/// What to draw of the bar of `window`.
pub(crate) unsafe fn view(window: HWND) -> Option<View> {
    let (area, whole, actions) = place(window)?;
    let factor = scale(window);
    let hover = HOVER.with(|all| all.borrow().get(&(window as isize)).copied());
    Some(View {
        bar_x: (area[0] - whole[0]) * factor,
        x: whole[0] * factor,
        y: whole[1] * factor,
        width: whole[2] * factor,
        height: whole[3] * factor,
        glyphs: actions.iter().map(|(action, _)| action.look().0).collect(),
        hover,
    })
}

/// The button under a point in client pixels.
pub(crate) unsafe fn hit(window: HWND, point: POINT) -> Option<usize> {
    let (area, _, actions) = place(window)?;
    let factor = scale(window);
    let (x, y) = (point.x as f32 / factor, point.y as f32 / factor);
    if y < area[1] || y >= area[1] + area[3] {
        return None;
    }
    dev_mode::button_at(actions.len(), x - area[0])
}

/// Follows the pointer over the bar. Returns whether the button under it changed.
pub(crate) unsafe fn track(window: HWND, point: Option<POINT>) -> bool {
    let now = point.and_then(|point| hit(window, point));
    HOVER.with(|all| {
        let mut all = all.borrow_mut();
        let before = all.get(&(window as isize)).copied();
        match now {
            Some(index) => all.insert(window as isize, index),
            None => all.remove(&(window as isize)),
        };
        before != now
    })
}

/// A button goes down. Returns whether the bar took the press.
pub(crate) unsafe fn press(window: HWND, point: POINT) -> bool {
    match hit(window, point) {
        Some(index) => {
            PRESS.with(|all| all.borrow_mut().insert(window as isize, index));
            true
        }
        None => false,
    }
}

/// Whether a press that began on the bar is waiting for its button to come up.
pub(crate) fn pressed(window: HWND) -> bool {
    PRESS.with(|all| all.borrow().contains_key(&(window as isize)))
}

/// The button comes up: the browser's command runs when the pointer is still on the button it went
/// down on.
pub(crate) unsafe fn release(window: HWND, point: POINT) {
    let Some(down) = PRESS.with(|all| all.borrow_mut().remove(&(window as isize))) else { return };
    if hit(window, point) != Some(down) {
        return;
    }
    let Some((_, _, actions)) = place(window) else { return };
    if let Some((_, command)) = actions.get(down) {
        // The browser's own command, as its keyboard shortcut sends it.
        PostMessageW(window, WM_COMMAND, *command as usize, 0);
    }
}
