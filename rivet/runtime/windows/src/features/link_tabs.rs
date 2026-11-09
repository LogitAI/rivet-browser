//! Links and new-window links of a Rivet window open as new tabs of its current space.
//!
//! Chromium sends a link that asks for a new tab or a new window out of an app window into a
//! window of its own. This takes such a window's tabs back into the Rivet window with Chromium's
//! own tab strip functions, `DetachTabAtForInsertion` and `InsertDetachedTabAt`, which keep the
//! page, its history and its opener. The emptied window is closed by the browser itself.

use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;

thread_local! {
    /// The browser windows of this process seen so far. `None` until the first look, so windows
    /// that were there before any Rivet window are left alone.
    static KNOWN: RefCell<Option<HashSet<isize>>> = const { RefCell::new(None) };
    /// When each new window without a tab was first seen, so it is looked at again only for a while.
    static EMPTY_SINCE: RefCell<std::collections::HashMap<isize, std::time::Instant>> =
        RefCell::new(std::collections::HashMap::new());
}

/// How long a new window may show no tab before it is left alone.
pub const EMPTY_WINDOW_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether a window that just appeared holds pages that belong in a Rivet window. Developer tools
/// keep their own window, and so does a window Rivet asked for as a Mini Window.
pub fn takes_window(urls: &[String]) -> bool {
    !urls.is_empty()
        && urls.iter().all(|url| {
            !url.starts_with("devtools://") && !crate::features::mini_window::is_marker_url(url)
        })
}

/// What becomes of a window that appeared.
#[derive(Debug, PartialEq, Eq)]
pub enum Fate {
    /// Its tabs move into the Rivet window.
    Take,
    /// It keeps its own tabs: a Mini Window, developer tools, or a window another Rivet window
    /// is in front of.
    Leave,
    /// It shows no tab yet, as a window the browser has only begun to fill. It is looked at again
    /// on the next tick instead of being taken for a window that holds nothing.
    Retry,
    /// It showed no tab for longer than `EMPTY_WINDOW_WAIT`, and is left alone from now on.
    GiveUp,
}

/// The fate of a window that appeared, from its tabs' addresses, whether Rivet asked for it as a
/// Mini Window and whether the Rivet window is in front of the others.
pub fn fate(urls: &[String], mini: bool, front: bool, empty_for: std::time::Duration) -> Fate {
    if mini || !front {
        Fate::Leave
    } else if urls.is_empty() && empty_for >= EMPTY_WINDOW_WAIT {
        Fate::GiveUp
    } else if urls.is_empty() {
        Fate::Retry
    } else if takes_window(urls) {
        Fate::Take
    } else {
        Fate::Leave
    }
}

/// The windows a link may have opened: the shown ones. A browser can keep a hidden window ready
/// and show it for the next link, so a hidden window is not known yet and counts as new once it
/// shows.
pub fn shown(windows: &[(isize, bool)]) -> Vec<isize> {
    windows
        .iter()
        .filter(|(_, visible)| *visible)
        .map(|(window, _)| *window)
        .collect()
}

/// The windows that appeared since `known`, and the new set of known windows.
pub fn appeared(known: &HashSet<isize>, now: &[isize], rivet: &[isize]) -> Vec<isize> {
    now.iter()
        .copied()
        .filter(|window| !known.contains(window) && !rivet.contains(window))
        .collect()
}

/// Looks for new browser windows of this process and moves their tabs into `window`, the Rivet
/// window that is now in front of the others of this process.
pub(crate) unsafe fn tick(window: HWND) {
    let rivet: Vec<isize> = crate::frame::rivet_windows()
        .into_iter()
        .map(|window| window as isize)
        .collect();
    let now: Vec<isize> = shown(
        &crate::frame::browser_windows_of_process()
            .into_iter()
            .map(|window| {
                let visible =
                    windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(window) != 0;
                (window as isize, visible)
            })
            .collect::<Vec<_>>(),
    );
    let fresh = KNOWN.with(|known| {
        let mut known = known.borrow_mut();
        let fresh = known
            .as_ref()
            .map(|seen| appeared(seen, &now, &rivet))
            .unwrap_or_default();
        *known = Some(now.iter().copied().collect());
        fresh
    });
    if fresh.is_empty() {
        return;
    }
    let front = in_front(window, &rivet);
    for other in fresh {
        let urls: Vec<String> = crate::features::tabs::open_tabs(other as HWND)
            .unwrap_or_default()
            .into_iter()
            .map(|tab| tab.saved.url)
            .collect();
        // A window Rivet asked the browser for as a Mini Window is not a link's.
        let mini = crate::features::mini_window::holds_mini_request(other as HWND, &urls);
        let empty_for = if urls.is_empty() {
            EMPTY_SINCE.with(|since| {
                since
                    .borrow_mut()
                    .entry(other)
                    .or_insert_with(std::time::Instant::now)
                    .elapsed()
            })
        } else {
            EMPTY_SINCE.with(|since| since.borrow_mut().remove(&other));
            std::time::Duration::ZERO
        };
        let decided = fate(&urls, mini, front, empty_for);
        crate::trace::trace(&format!(
            "link tabs: window {other:#x} appeared with {} tabs (Mini Window {mini}, Rivet window in front {front}): {decided:?}",
            urls.len()
        ));
        match decided {
            Fate::Take => {}
            Fate::Leave => continue,
            Fate::GiveUp => {
                EMPTY_SINCE.with(|since| since.borrow_mut().remove(&other));
                crate::trace::trace(&format!(
                    "link tabs: window {other:#x} showed no tab for {} s; left alone",
                    EMPTY_WINDOW_WAIT.as_secs()
                ));
                continue;
            }
            Fate::Retry => {
                // Forgotten, so the next tick sees it as new again.
                KNOWN.with(|known| {
                    if let Some(seen) = known.borrow_mut().as_mut() {
                        seen.remove(&other);
                    }
                });
                crate::trace::trace(&format!(
                    "link tabs: window {other:#x} shows no tab yet; looked at again"
                ));
                continue;
            }
        }
        let moved = move_all(window, other as HWND);
        crate::trace::trace(&format!(
            "link tabs: moved {moved} of {} tabs from window {other:#x}",
            urls.len()
        ));
        // A link opened in a new window is one tab. Its link rule may send it on from here.
        if let ([url], 1) = (urls.as_slice(), moved) {
            crate::features::link_rules::after_link_moved(window, url);
        }
    }
}

/// Whether `window` is above every other Rivet window, which makes it the one a link came from.
unsafe fn in_front(window: HWND, rivet: &[isize]) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, GW_HWNDPREV};
    let mut above = GetWindow(window, GW_HWNDPREV);
    while !above.is_null() {
        if rivet.contains(&(above as isize)) {
            return false;
        }
        above = GetWindow(above, GW_HWNDPREV);
    }
    true
}

type Count = unsafe extern "system" fn(*mut c_void) -> i32;
type Detach = unsafe extern "system" fn(*mut c_void, *mut usize, i32) -> *mut usize;
/// `InsertDetachedTabAt(model, index, tab, add types, group)`. Some builds take one more optional
/// by pointer after the group, so a sixth empty one is always passed; a build that takes five
/// ignores it.
type Insert = unsafe extern "system" fn(
    *mut c_void,
    i32,
    *mut usize,
    i32,
    *const [u64; 8],
    *const [u64; 16],
) -> i32;

/// Moves every tab of `from` to the end of `window`'s tabs, the last one active. Returns how many
/// moved. A build whose row has no detach and insert moves none.
unsafe fn move_all(window: HWND, from: HWND) -> usize {
    use crate::features::tabs::{base, function, tab_model};
    let (Some(base), Some(detach), Some(insert)) = (
        base(),
        crate::features::app_window::recorded("detach"),
        crate::features::app_window::recorded("insert"),
    ) else {
        return 0;
    };
    let (Some(source), Some(target)) = (tab_model(base, from), tab_model(base, window)) else {
        return 0;
    };
    let count = function::<Count>(base, crate::builds::get().tabs.tab_count);
    let mut moved = 0;
    while count(source) > 0 && moved < 64 {
        let mut tab: usize = 0;
        function::<Detach>(base, detach)(source, &mut tab, 0);
        if tab == 0 {
            break;
        }
        let none = [0u64; 8];
        let no_more = [0u64; 16];
        // At the end of the strip, active: `ADD_ACTIVE | ADD_FORCE_INDEX`.
        function::<Insert>(base, insert)(target, count(target), &mut tab, 1 | 4, &none, &no_more);
        moved += 1;
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr3_a_window_kept_hidden_counts_as_new_when_it_shows_for_a_link() {
        let rivet = [1];
        // The first look: the Rivet window and a hidden spare window.
        let known: HashSet<isize> = shown(&[(1, true), (7, false)]).into_iter().collect();
        assert!(!known.contains(&7));
        // The browser shows the spare window for a link.
        let now = shown(&[(1, true), (7, true)]);
        assert_eq!(appeared(&known, &now, &rivet), [7]);
    }

    #[test]
    fn fr3_a_new_window_without_tabs_yet_is_looked_at_again() {
        use std::time::Duration;
        const ZERO: Duration = Duration::ZERO;
        // A window that stays empty past the wait is left alone.
        assert_eq!(fate(&[], false, true, EMPTY_WINDOW_WAIT), Fate::GiveUp);
        assert_eq!(fate(&[], false, true, Duration::from_secs(4)), Fate::Retry);
        let link = vec!["https://example.com/".to_owned()];
        assert_eq!(fate(&link, false, true, ZERO), Fate::Take);
        // The browser has not put the link's tab in the window yet.
        assert_eq!(fate(&[], false, true, ZERO), Fate::Retry);
        assert_eq!(fate(&link, true, true, ZERO), Fate::Leave);
        assert_eq!(fate(&link, false, false, ZERO), Fate::Leave);
        assert_eq!(
            fate(
                &["devtools://devtools/bundled/inspector.html".to_owned()],
                false,
                true,
                ZERO
            ),
            Fate::Leave
        );
    }

    #[test]
    fn fr3_a_window_a_link_opened_is_taken_but_developer_tools_are_not() {
        assert!(takes_window(&["https://example.com/".into()]));
        assert!(!takes_window(&[
            "devtools://devtools/bundled/inspector.html".into()
        ]));
        assert!(!takes_window(&[]));
    }

    #[test]
    fn fr9_links_never_take_the_window_rivet_asked_for_as_a_mini_window() {
        assert!(!takes_window(&[
            "file:///C:/Rivet/start.html?space=Work#rivet-mini-window-12-3".into()
        ]));
        assert!(takes_window(&[
            "file:///C:/Rivet/start.html?space=Work".into()
        ]));
    }

    #[test]
    fn fr3_samsung_internet_records_detach_and_insert_like_the_other_browsers() {
        let samsung = crate::builds::compiled()
            .iter()
            .find(|build| build.browser == "Samsung Internet" && build.version == [30, 0, 4, 29])
            .and_then(|build| build.app_window.as_ref())
            .expect("Samsung Internet 143.0.4.29 has an app window row");
        assert_eq!(samsung.detach, Some(0x0877_3D90));
        assert_eq!(samsung.insert, Some(0x0322_A3C0));
        assert!(samsung.detach_prologue.len() >= 8 && samsung.insert_prologue.len() >= 8);
    }

    #[test]
    fn fr3_only_windows_that_appeared_after_the_first_look_are_new() {
        let known: HashSet<isize> = [1, 2].into_iter().collect();
        assert_eq!(
            appeared(&known, &[1, 2, 3, 4], &[4]),
            vec![3],
            "a Rivet window is never taken"
        );
        assert!(appeared(&known, &[1], &[]).is_empty());
    }
}
