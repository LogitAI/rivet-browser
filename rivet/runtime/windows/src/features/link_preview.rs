//! Link preview: a long press on a tab shows its page floating over the page being read.
//!
//! The page floats in Rivet's second page view, a Chromium `ContentsWebView` of the window's own
//! browser view, so the browser still renders it. Rivet draws only the header above it, on the
//! window's composition surface: the page's title and address, "Open as tab" and "Esc". Opening it
//! as a tab makes its tab the active one; Esc puts the page back in its tab without showing it.

use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::features::second_view::{self, Mode, PREVIEW_HEADER};

/// How long a press must last before it opens the preview, as the study's 500 ms.
pub(crate) const HOLD_MS: u32 = 500;
/// The timer of a press on a tab that may become a long press.
pub(crate) const HOLD_TIMER: usize = 0x5249_5650;

/// A button of the preview's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chip {
    OpenAsTab,
    Dismiss,
}

impl Chip {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Chip::OpenAsTab => "Open as tab",
            Chip::Dismiss => "Esc",
        }
    }
}

/// The study's `.chip`: 24 DIPs high, 8 between chips and at the header's right end.
pub(crate) const CHIP_HEIGHT: f32 = 24.0;
const CHIP_GAP: f32 = 8.0;
const OPEN_WIDTH: f32 = 82.0;
const DISMISS_WIDTH: f32 = 40.0;

/// The header's chips as (chip, x, width) from the header's left edge, for a header `width` DIPs
/// wide. They end at its right edge, "Esc" last.
pub(crate) fn chips(width: f32) -> [(Chip, f32, f32); 2] {
    let dismiss = width - CHIP_GAP - DISMISS_WIDTH;
    let open = dismiss - CHIP_GAP - OPEN_WIDTH;
    [
        (Chip::OpenAsTab, open, OPEN_WIDTH),
        (Chip::Dismiss, dismiss, DISMISS_WIDTH),
    ]
}

/// The chip under a point of the header, in DIPs from its top left corner.
pub(crate) fn chip_at(width: f32, x: f32, y: f32) -> Option<Chip> {
    let top = (PREVIEW_HEADER as f32 - CHIP_HEIGHT) / 2.0;
    if y < top || y >= top + CHIP_HEIGHT {
        return None;
    }
    chips(width)
        .into_iter()
        .find(|(_, left, w)| x >= *left && x < left + w)
        .map(|(chip, _, _)| chip)
}

/// What the header says: the study's `Link Preview · ember.thisisthepy.org/brief`.
pub(crate) fn heading(url: &str) -> String {
    let bare = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .trim_end_matches('/');
    if bare.is_empty() {
        "Link Preview".to_owned()
    } else {
        format!("Link Preview \u{b7} {bare}")
    }
}

/// What the appbar draws of the preview, in client pixels. The page itself is the browser's.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    /// The header's left, top and width, and its height.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub header: f32,
    /// The height of the page below the header, which the shadow goes around.
    pub page: f32,
    pub heading: String,
    pub hover: Option<Chip>,
}

#[derive(Default)]
struct Preview {
    hover: Option<Chip>,
    press: Option<Chip>,
}

thread_local! {
    /// The tab pressed in the appbar, by window, while the press may still become a long one.
    static HOLDS: RefCell<HashMap<isize, usize>> = RefCell::new(HashMap::new());
    static PREVIEWS: RefCell<HashMap<isize, Preview>> = RefCell::new(HashMap::new());
    /// A preview over a split, by window. The split holds the second view's slots, so the page
    /// floats in a bare page view of its own.
    static OVER_SPLIT: RefCell<HashMap<isize, Floating>> = RefCell::new(HashMap::new());
}

/// A page floating in a bare page view, over a split.
#[derive(Clone, Copy, Debug)]
struct Floating {
    view: usize,
    parent: usize,
    page: usize,
    tab: usize,
    /// The page's rectangle in window DIPs, as x, y, width and height.
    rect: [i32; 4],
}

fn over_split(window: HWND) -> Option<Floating> {
    OVER_SPLIT.with(|all| all.borrow().get(&(window as isize)).copied())
}

/// Floats the page of tab `index` in a bare page view over the window's split.
unsafe fn open_over_split(window: HWND, index: usize) -> bool {
    use crate::features::second_view::bare;
    let page = bare::page_of_tab(window, index);
    if page == 0 || crate::features::split::shown_tabs(window).contains(&index) {
        return false;
    }
    let Some([left, top, right, bottom]) = crate::features::split::page_area(window) else {
        return false;
    };
    let rect = second_view::floating([left, top, right - left, bottom - top]);
    let Some((view, parent)) = bare::new_view(window, page) else {
        return false;
    };
    bare::place(view, parent, rect);
    OVER_SPLIT.with(|all| {
        all.borrow_mut().insert(
            window as isize,
            Floating {
                view,
                parent,
                page,
                tab: index,
                rect,
            },
        )
    });
    true
}

/// The tab and page of the preview, wherever it floats.
unsafe fn shown(window: HWND) -> Option<(usize, usize)> {
    match over_split(window) {
        Some(floating) => Some((floating.tab, floating.page)),
        None => second_view::floating_page(window),
    }
}

/// The preview's page in client pixels, as left, top, right and bottom.
unsafe fn page_rect(window: HWND) -> Option<[i32; 4]> {
    let Some(floating) = over_split(window) else {
        return second_view::floating_client_rect(window);
    };
    let f = scale(window);
    let [x, y, w, h] = floating.rect;
    let px = |value: i32| (value as f32 * f).round() as i32;
    Some([px(x), px(y), px(x + w), px(y + h)])
}

/// A press began on the tab row of strip index `index`.
pub(crate) unsafe fn press(window: HWND, index: usize) {
    if !second_view::available() {
        return;
    }
    HOLDS.with(|holds| holds.borrow_mut().insert(window as isize, index));
    SetTimer(window, HOLD_TIMER, HOLD_MS, None);
}

/// The press ended or became a drag before it was long.
pub(crate) unsafe fn cancel_hold(window: HWND) {
    if HOLDS
        .with(|holds| holds.borrow_mut().remove(&(window as isize)))
        .is_some()
    {
        KillTimer(window, HOLD_TIMER);
    }
}

/// Whether the window shows a preview now.
pub(crate) fn is_open(window: HWND) -> bool {
    let open = second_view::is_floating(window) || over_split(window).is_some();
    if !open {
        PREVIEWS.with(|all| all.borrow_mut().remove(&(window as isize)));
    }
    open
}

/// Shows the page of tab `index` floating over the active page.
pub(crate) unsafe fn open(window: HWND, index: usize) -> bool {
    let opened = if second_view::is_split(window) {
        open_over_split(window, index)
    } else {
        second_view::open_tab(window, index, Mode::Floating)
    };
    crate::trace::trace(&format!("link preview: tab {index}: {opened}"));
    if opened {
        PREVIEWS.with(|all| all.borrow_mut().insert(window as isize, Preview::default()));
    }
    opened
}

/// Esc: the page goes back to its tab, which stays where it was.
pub(crate) unsafe fn dismiss(window: HWND) -> bool {
    PREVIEWS.with(|all| all.borrow_mut().remove(&(window as isize)));
    let closed = match OVER_SPLIT.with(|all| all.borrow_mut().remove(&(window as isize))) {
        Some(floating) => {
            second_view::bare::drop_view(floating.view, floating.parent);
            true
        }
        None => second_view::close(window),
    };
    crate::features::page_links::preview_dismissed(window);
    crate::trace::trace(&format!("link preview: dismissed: {closed}"));
    closed
}

/// The previewed page becomes the active tab of this space.
pub(crate) unsafe fn open_as_tab(window: HWND) -> bool {
    let Some((tab, _)) = shown(window) else {
        return dismiss(window);
    };
    crate::features::page_links::preview_kept(window);
    dismiss(window);
    let activated = crate::features::tabs::activate_tab(window, tab);
    crate::trace::trace(&format!("link preview: open as tab {tab}: {activated}"));
    activated
}

fn scale(window: HWND) -> f32 {
    unsafe { GetDpiForWindow(window).max(96) as f32 / 96.0 }
}

/// The header in client pixels as left, top, right and bottom, right above the floating page.
unsafe fn header_rect(window: HWND) -> Option<[f32; 4]> {
    let page = page_rect(window)?;
    let header = PREVIEW_HEADER as f32 * scale(window);
    Some([
        page[0] as f32,
        page[1] as f32 - header,
        page[2] as f32,
        page[1] as f32,
    ])
}

pub(crate) unsafe fn view(window: HWND) -> Option<View> {
    if !is_open(window) {
        return None;
    }
    let page = page_rect(window)?;
    let [left, top, right, bottom] = header_rect(window)?;
    let url = shown(window)
        .map(|(_, contents)| crate::features::tabs::page_address(contents as *mut _))
        .unwrap_or_default();
    let hover = PREVIEWS.with(|all| {
        all.borrow()
            .get(&(window as isize))
            .and_then(|preview| preview.hover)
    });
    Some(View {
        x: left,
        y: top,
        width: right - left,
        header: bottom - top,
        page: (page[3] - page[1]) as f32,
        heading: heading(&url),
        hover,
    })
}

/// The chip under a client point, and whether the point is on the header at all.
unsafe fn hit(window: HWND, point: POINT) -> (bool, Option<Chip>) {
    let Some([left, top, right, bottom]) = header_rect(window) else {
        return (false, None);
    };
    let (x, y) = (point.x as f32, point.y as f32);
    if x < left || x >= right || y < top || y >= bottom {
        return (false, None);
    }
    let f = scale(window);
    (
        true,
        chip_at((right - left) / f, (x - left) / f, (y - top) / f),
    )
}

fn point_of(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    }
}

const WM_KEYDOWN: u32 = 0x0100;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;

/// Handles a message of the browser window for the preview. `Some` means the preview used it.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    if message == WM_TIMER && wparam == HOLD_TIMER {
        KillTimer(window, HOLD_TIMER);
        if let Some(index) = HOLDS.with(|holds| holds.borrow_mut().remove(&(window as isize))) {
            // The press opened the preview, so its release no longer clicks the tab.
            crate::frame::cancel_appbar_press(window);
            open(window, index);
            crate::frame::update_appbar(window);
        }
        return Some(0);
    }
    if !is_open(window) {
        return None;
    }
    match message {
        WM_KEYDOWN if wparam == 0x1B => {
            dismiss(window);
            crate::frame::update_appbar(window);
            Some(0)
        }
        WM_MOUSEMOVE => {
            let (on, chip) = hit(window, point_of(lparam));
            let changed = PREVIEWS.with(|all| {
                let mut all = all.borrow_mut();
                let preview = all.entry(window as isize).or_default();
                std::mem::replace(&mut preview.hover, chip) != chip
            });
            if changed {
                crate::frame::update_appbar(window);
            }
            on.then_some(0)
        }
        WM_LBUTTONDOWN => {
            let (on, chip) = hit(window, point_of(lparam));
            if !on {
                return None;
            }
            PREVIEWS.with(|all| all.borrow_mut().entry(window as isize).or_default().press = chip);
            Some(0)
        }
        WM_LBUTTONUP => {
            let pressed = PREVIEWS.with(|all| {
                all.borrow_mut()
                    .get_mut(&(window as isize))
                    .and_then(|preview| preview.press.take())
            });
            let (on, chip) = hit(window, point_of(lparam));
            match pressed.filter(|pressed| Some(*pressed) == chip) {
                Some(Chip::OpenAsTab) => {
                    open_as_tab(window);
                }
                Some(Chip::Dismiss) => {
                    dismiss(window);
                }
                None => {}
            }
            if pressed.is_some() {
                crate::frame::update_appbar(window);
            }
            (on || pressed.is_some()).then_some(0)
        }
        _ => None,
    }
}

/// Forgets a closed window.
pub(crate) fn forget(window: HWND) {
    HOLDS.with(|holds| holds.borrow_mut().remove(&(window as isize)));
    PREVIEWS.with(|all| all.borrow_mut().remove(&(window as isize)));
    OVER_SPLIT.with(|all| all.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::second_view::{floating, layout};

    #[test]
    fn fr3_link_preview_floats_over_the_page_which_keeps_its_full_size() {
        let full = [0, 40, 1200, 800];
        let (page, preview) = layout(full, Mode::Floating, 0);
        assert_eq!(page, full, "the page being read is not narrowed");
        assert_eq!(preview, floating(full));
        // 64 at the sides, the 40 DIP header 20 below the bar, 40 below.
        assert_eq!(preview, [64, 40 + 20 + 40, 1200 - 128, 800 - 20 - 40 - 40]);
    }

    #[test]
    fn fr3_link_preview_keeps_a_page_in_a_small_window() {
        let preview = floating([0, 0, 300, 200]);
        assert_eq!(preview[0], 30);
        assert!(preview[2] > 0 && preview[3] > 0);
    }

    #[test]
    fn fr3_link_preview_header_offers_open_as_tab_and_esc_at_its_right_end() {
        let [open, dismiss] = chips(1000.0);
        assert_eq!(open.0, Chip::OpenAsTab);
        assert_eq!(dismiss.0, Chip::Dismiss);
        assert_eq!(dismiss.1 + dismiss.2, 1000.0 - 8.0);
        assert!(open.1 + open.2 <= dismiss.1);
        assert_eq!(chip_at(1000.0, dismiss.1 + 1.0, 20.0), Some(Chip::Dismiss));
        assert_eq!(chip_at(1000.0, open.1 + 1.0, 20.0), Some(Chip::OpenAsTab));
        assert_eq!(chip_at(1000.0, 10.0, 20.0), None, "the title is no button");
        assert_eq!(
            chip_at(1000.0, dismiss.1 + 1.0, 2.0),
            None,
            "above the chips"
        );
    }

    #[test]
    fn fr3_link_preview_header_names_the_page_without_its_scheme() {
        assert_eq!(
            heading("https://ember.thisisthepy.org/brief"),
            "Link Preview \u{b7} ember.thisisthepy.org/brief"
        );
        assert_eq!(heading(""), "Link Preview");
    }

    #[test]
    fn fr3_a_long_press_is_the_studys_half_second() {
        assert_eq!(HOLD_MS, 500);
    }
}
