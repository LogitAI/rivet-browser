//! A long press on a link inside a page opens it in Link Preview.
//!
//! The browser tells its status bubble the address of the link under the pointer through
//! Chromium's `UpdateTargetURL`. Rivet watches that call, so it knows the link without reading the
//! page. A press over a link that lasts 500 ms without moving opens the link in a background tab
//! and floats that tab's page as the preview. The page then loses the mouse capture, so its click
//! never happens and the page stays where it was. A shorter press stays an ordinary click.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::features::link_preview::HOLD_MS;

/// The timer of a press on a link that may become a long press.
const LINK_HOLD_TIMER: usize = 0x5249_564C;

/// How far a press may move, in pixels, before it is a drag rather than a long press.
const SLOP: i32 = 4;

/// The address an address of the link under the pointer previews, if it is one Rivet previews.
pub fn previewable(url: &str) -> Option<&str> {
    let url = url.trim();
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

/// Whether a press that began at `start` has moved too far at `now` to stay a long press.
pub fn moved_away(start: POINT, now: POINT) -> bool {
    (now.x - start.x).abs() > SLOP || (now.y - start.y).abs() > SLOP
}

/// The link under the pointer, as the browser last reported it, with the page it is in.
static HOVER: Mutex<Option<(usize, String)>> = Mutex::new(None);

/// A press on a link that may still become a long press, by window.
#[derive(Clone)]
struct Hold {
    start: POINT,
    url: String,
}

#[derive(Default)]
struct State {
    holds: HashMap<isize, Hold>,
    /// Windows whose press opened a preview, whose button release the page must not see.
    swallow: HashMap<isize, ()>,
    /// The tab each window opened for its link preview, closed again on Esc.
    link_tabs: HashMap<isize, usize>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

static ORIGINAL_TARGET_URL: AtomicUsize = AtomicUsize::new(0);

type TargetUrl = unsafe extern "system" fn(*mut c_void, *mut c_void, *const u8);

/// The text of a `GURL`'s spec, a `std::string` at its start.
unsafe fn spec(gurl: *const u8) -> Option<String> {
    if gurl.is_null() {
        return None;
    }
    let last = *gurl.add(23);
    let (data, size) = if last & 0x80 != 0 {
        (*(gurl as *const *const u8), *(gurl.add(8) as *const usize))
    } else {
        (gurl, last as usize)
    };
    (size <= 1 << 16)
        .then(|| String::from_utf8_lossy(std::slice::from_raw_parts(data, size)).into_owned())
}

/// `BrowserWebContentsDelegate::UpdateTargetURL(this, source, url)`.
unsafe extern "system" fn target_url_hook(this: *mut c_void, source: *mut c_void, url: *const u8) {
    let address = spec(url).unwrap_or_default();
    if let Ok(mut hover) = HOVER.lock() {
        *hover = (!address.is_empty()).then_some((source as usize, address));
    }
    let original = ORIGINAL_TARGET_URL.load(Ordering::Acquire);
    if original != 0 {
        std::mem::transmute::<usize, TargetUrl>(original)(this, source, url);
    }
}

/// Watches the link under the pointer, once per process. Nothing is hooked on a build without a
/// recorded `UpdateTargetURL`.
pub(crate) unsafe fn install() -> Result<(), String> {
    static HOOKED: OnceLock<Result<(), String>> = OnceLock::new();
    HOOKED
        .get_or_init(|| {
            let Some(build) = crate::builds::current() else {
                return Ok(());
            };
            let Some(tools) = crate::builds::page_tool_rows::page_tools_for(build) else {
                return Ok(());
            };
            let image = crate::chromium::image::recorded()
                .ok_or("this browser build has no recorded link address")?;
            let (rva, prologue) = tools.update_target_url;
            crate::targets::transaction(|| {
                crate::targets::detour(
                    image.base,
                    rva,
                    prologue,
                    target_url_hook as *const () as usize,
                    &ORIGINAL_TARGET_URL,
                )
            })
        })
        .clone()
}

/// The previewable link under the pointer in a page of `window`.
unsafe fn hovered_link(window: HWND) -> Option<String> {
    let (source, url) = HOVER.lock().ok()?.clone()?;
    let url = previewable(&url)?.to_owned();
    let tabs = crate::features::tabs::open_tabs(window)?;
    tabs.iter().any(|tab| tab.id == source).then_some(url)
}

fn point_of(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    }
}

const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const MK_LBUTTON: usize = 0x0001;

#[link(name = "user32")]
extern "system" {
    fn ReleaseCapture() -> i32;
}

unsafe fn cancel(window: HWND) {
    if state().holds.remove(&(window as isize)).is_some() {
        KillTimer(window, LINK_HOLD_TIMER);
    }
}

/// Handles a pointer message of the browser window for a long press on a link. `Some` means the
/// page must not see the message. Every other message reaches the page as before.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    let key = window as isize;
    match message {
        WM_LBUTTONDOWN => {
            cancel(window);
            if crate::features::link_preview::is_open(window)
                || !crate::features::second_view::available()
            {
                return None;
            }
            if let Some(url) = hovered_link(window) {
                state().holds.insert(
                    key,
                    Hold {
                        start: point_of(lparam),
                        url,
                    },
                );
                SetTimer(window, LINK_HOLD_TIMER, HOLD_MS, None);
            }
            None
        }
        WM_MOUSEMOVE => {
            if state().swallow.contains_key(&key) {
                if wparam & MK_LBUTTON != 0 {
                    return Some(0);
                }
                state().swallow.remove(&key);
            }
            let start = state().holds.get(&key).map(|hold| hold.start);
            if start.is_some_and(|start| moved_away(start, point_of(lparam))) {
                cancel(window);
            }
            None
        }
        WM_LBUTTONUP => {
            cancel(window);
            state().swallow.remove(&key).map(|()| 0)
        }
        WM_TIMER if wparam == LINK_HOLD_TIMER => {
            KillTimer(window, LINK_HOLD_TIMER);
            let Some(hold) = state().holds.remove(&key) else {
                return Some(0);
            };
            // The page loses the mouse, so the press never becomes a click on the link.
            state().swallow.insert(key, ());
            ReleaseCapture();
            open_link(window, &hold.url);
            crate::frame::update_appbar(window);
            Some(0)
        }
        _ => None,
    }
}

/// Opens `url` in a background tab and floats that tab's page as the preview.
unsafe fn open_link(window: HWND, url: &str) -> bool {
    let Some(contents) = crate::features::navigation::active_contents(window) else {
        return false;
    };
    let page = crate::features::navigation::open_in_background_tab(contents, url);
    let index = (!page.is_null())
        .then(|| crate::features::tabs::open_tabs(window))
        .flatten()
        .and_then(|tabs| tabs.iter().position(|tab| tab.id == page as usize));
    let opened = index.is_some_and(|index| crate::features::link_preview::open(window, index));
    crate::trace::trace(&format!("link preview: link in page: {opened}"));
    if opened {
        state().link_tabs.insert(window as isize, page as usize);
    } else if !page.is_null() {
        crate::features::tabs::close_tab_by_id(window, page as usize);
    }
    opened
}

/// The preview was dismissed: a tab Rivet opened only for a link preview closes again.
pub(crate) unsafe fn preview_dismissed(window: HWND) {
    if let Some(page) = state().link_tabs.remove(&(window as isize)) {
        crate::features::tabs::close_tab_by_id(window, page);
    }
}

/// The preview became a tab, which stays.
pub(crate) fn preview_kept(window: HWND) {
    state().link_tabs.remove(&(window as isize));
}

/// Forgets a closed window.
pub(crate) fn forget(window: HWND) {
    let key = window as isize;
    let mut state = state();
    state.holds.remove(&key);
    state.swallow.remove(&key);
    state.link_tabs.remove(&key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr3_long_press_previews_web_links_only() {
        assert_eq!(
            previewable("https://example.com/a"),
            Some("https://example.com/a")
        );
        assert_eq!(
            previewable("http://localhost:8080/"),
            Some("http://localhost:8080/")
        );
        assert_eq!(previewable(""), None);
        assert_eq!(previewable("javascript:void(0)"), None);
        assert_eq!(previewable("mailto:someone@example.com"), None);
    }

    #[test]
    fn fr3_a_press_that_moves_is_a_drag_not_a_long_press() {
        let start = POINT { x: 100, y: 100 };
        assert!(!moved_away(start, POINT { x: 103, y: 96 }));
        assert!(moved_away(start, POINT { x: 105, y: 100 }));
        assert!(moved_away(start, POINT { x: 100, y: 94 }));
    }

    #[test]
    fn fr3_a_link_press_becomes_a_preview_after_the_studys_half_second() {
        assert_eq!(HOLD_MS, 500);
        assert_ne!(LINK_HOLD_TIMER, crate::features::link_preview::HOLD_TIMER);
    }

    #[test]
    fn fr3_a_short_press_on_a_link_stays_a_click() {
        let window = 0x1234 as HWND;
        // No long press happened, so the release reaches the page.
        unsafe {
            assert_eq!(message(window, WM_LBUTTONUP, 0, 0), None);
            assert_eq!(message(window, WM_MOUSEMOVE, MK_LBUTTON, 0), None);
        }
    }

    #[test]
    fn fr3_after_a_long_press_the_page_never_sees_the_release() {
        let window = 0x5678 as HWND;
        state().swallow.insert(window as isize, ());
        unsafe {
            assert_eq!(message(window, WM_MOUSEMOVE, MK_LBUTTON, 0), Some(0));
            assert_eq!(message(window, WM_LBUTTONUP, 0, 0), Some(0));
            assert_eq!(message(window, WM_LBUTTONUP, 0, 0), None);
        }
    }

    #[test]
    fn fr3_link_address_is_read_from_both_forms_of_the_gurl_spec() {
        let mut short = [0u8; 24];
        short[..9].copy_from_slice(b"https://a");
        short[23] = 9;
        assert_eq!(
            unsafe { spec(short.as_ptr()) }.as_deref(),
            Some("https://a")
        );
        let text = b"https://example.com/a/long/address";
        let mut long = [0usize; 3];
        long[0] = text.as_ptr() as usize;
        long[1] = text.len();
        long[2] = (text.len() + 1) | (1 << 63);
        assert_eq!(
            unsafe { spec(long.as_ptr().cast()) }.as_deref(),
            Some("https://example.com/a/long/address")
        );
    }
}
