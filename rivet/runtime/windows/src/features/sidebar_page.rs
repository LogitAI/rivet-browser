//! The Sidebar shows an extension's own side panel page in Rivet's second page view.
//!
//! The rail opens an extension's entry through Chromium's `SidePanelCoordinator`, as the
//! browser's own toolbar would. Chromium then loads the extension's side panel page in a
//! `WebContents` that an `ExtensionViewViews` inside the browser's `SidePanel` shows. Rivet takes
//! that page into its docked second view, which floats over the right of the window's page
//! without narrowing it, and the browser's own panel keeps no width. When the entry closes, the
//! page goes back to the browser's view first, so the browser destroys it where it made it.

use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::HWND;

use crate::features::second_view::{self, Mode};
use crate::features::side_panel::{self as panel, EntryKey, HEADER_ROW, PANEL_WIDTH};

/// The page an open extension entry shows in the second view, and the browser's view it came from.
#[derive(Clone, Copy)]
struct Hosted {
    key: EntryKey,
    holder: usize,
    page: usize,
}

thread_local! {
    static HOSTED: RefCell<HashMap<isize, Hosted>> = RefCell::new(HashMap::new());
}

/// What the Sidebar does when the entry the browser shows may have changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Nothing,
    /// An extension entry opened: its page moves into the second view.
    Take,
    /// The hosted entry closed: its page goes back to the browser's view.
    GiveBack,
    /// Another extension entry replaced the hosted one.
    Swap,
}

/// Decides the step from the entry the second view hosts and the extension entry the browser
/// shows now, both by key.
pub(crate) fn step<K: PartialEq>(hosted: Option<&K>, showing: Option<&K>) -> Step {
    match (hosted, showing) {
        (None, None) => Step::Nothing,
        (None, Some(_)) => Step::Take,
        (Some(_), None) => Step::GiveBack,
        (Some(hosted), Some(showing)) if hosted == showing => Step::Nothing,
        (Some(_), Some(_)) => Step::Swap,
    }
}

/// Whether the second view of `window` shows an extension's side panel page.
pub(crate) fn hosts(window: HWND) -> bool {
    HOSTED.with(|hosted| hosted.borrow().contains_key(&(window as isize)))
}

/// The docked second view's bounds for a hosted page: below Rivet's header row, and right of the
/// hairline that Rivet draws at the panel's left edge. Other second views keep `second`.
pub(crate) fn fit(window: HWND, mode: Mode, second: [i32; 4]) -> [i32; 4] {
    if hosts(window) { below_header(mode, second) } else { second }
}

/// A docked panel's bounds below the header row and right of the hairline.
pub(crate) fn below_header(mode: Mode, second: [i32; 4]) -> [i32; 4] {
    let [x, y, width, height] = second;
    if !matches!(mode, Mode::Docked { .. }) || width <= 1 {
        return second;
    }
    let header = HEADER_ROW.min((height - 1).max(0));
    [x + 1, y + header, width - 1, (height - header).max(1)]
}

/// The browser's own side panel and the page after the page area layout, while the second view
/// shows the panel's page: the page keeps the width it has with no panel, up to `page_right`, and
/// the browser's panel keeps no width at that edge.
pub(crate) fn float_over(contents: &mut [i32; 4], side_panel: &mut [i32; 4], page_right: i32) {
    contents[2] = (page_right - contents[0]).max(1);
    *side_panel = [page_right, side_panel[1], 0, side_panel[3]];
}

/// The extension entry the browser shows in the window, if any.
unsafe fn showing_extension(window: HWND) -> Option<EntryKey> {
    panel::listed_entries(window)
        .into_iter()
        .find(|key| panel::extension_id(key).is_some() && panel::entry_showing(window, key))
}

/// Follows the browser's side panel: a newly shown extension page moves into the second view, and
/// a page whose entry closed goes back. Called after a rail click and from the rail's poll.
pub(crate) unsafe fn follow(window: HWND) {
    if !second_view::available() || !panel::sidebar_checked() {
        return;
    }
    let showing = showing_extension(window);
    let hosted = HOSTED.with(|hosted| hosted.borrow().get(&(window as isize)).map(|entry| entry.key));
    match step(hosted.as_ref(), showing.as_ref()) {
        Step::Nothing => {}
        Step::Take => take(window, showing.expect("an entry shows")),
        Step::GiveBack => give_back(window),
        Step::Swap => {
            give_back(window);
            take(window, showing.expect("an entry shows"));
        }
    }
}

/// Moves the page of the shown entry from the browser's side panel into the docked second view.
unsafe fn take(window: HWND, key: EntryKey) {
    if second_view::is_split(window) {
        crate::trace::trace("sidebar page: a split holds the second view");
        return;
    }
    let Some(objects) = panel::objects(window).filter(|found| found.side_panel != 0) else { return };
    let Some(active) = crate::features::navigation::active_contents(window) else { return };
    let page_vtable = *(active as *const usize);
    // The entry's page may still be loading into its view; the next poll looks again.
    let Some((holder, page)) = second_view::page_under(objects.side_panel, page_vtable) else { return };
    // A page shows in one view at a time. The browser's view lets go of it first.
    second_view::hand_page(holder, 0);
    HOSTED.with(|hosted| hosted.borrow_mut().insert(window as isize, Hosted { key, holder, page }));
    if !second_view::open_with(window, window, page, Mode::Docked { width: PANEL_WIDTH }) {
        HOSTED.with(|hosted| hosted.borrow_mut().remove(&(window as isize)));
        second_view::hand_page(holder, page);
        crate::trace::trace("sidebar page: the second view did not open");
        return;
    }
    crate::hooks::page_area::relayout(window);
    crate::trace::trace(&format!("sidebar page: extension page {page:#x} from view {holder:#x} in the second view"));
}

/// Gives the hosted page back to the browser's side panel view and closes the docked view.
pub(crate) unsafe fn give_back(window: HWND) {
    let Some(entry) = HOSTED.with(|hosted| hosted.borrow_mut().remove(&(window as isize))) else { return };
    if second_view::is_docked(window) {
        second_view::close(window);
    }
    second_view::hand_page(entry.holder, entry.page);
    crate::hooks::page_area::relayout(window);
    crate::trace::trace(&format!("sidebar page: extension page {:#x} back to view {:#x}", entry.page, entry.holder));
}

/// The page area step: while the second view shows the side panel's page, the browser's panel
/// keeps no width and the page keeps its own.
pub(crate) unsafe fn on_page_area(area: &mut crate::hooks::page_area::PageArea) {
    if !hosts(area.window) {
        return;
    }
    let row = &crate::builds::get().side_panel;
    let page = area.bounds(row.page_area).map(|bounds| *bounds);
    let Some(page_right) = page.map(|page| page[0] + page[2]) else { return };
    for offset in [row.side_panel, row.side_panel_animation] {
        let Some(side_panel) = area.bounds(offset) else { continue };
        if let Some(contents) = area.bounds(row.contents_container) {
            float_over(&mut *contents, &mut *side_panel, page_right);
        }
        if let Some(outline) = area.bounds(row.contents_outline) {
            let mut unused = *side_panel;
            float_over(&mut *outline, &mut unused, page_right);
        }
    }
}

/// Drops a closed window without calling into its views, which the browser deletes with it.
pub(crate) fn forget(window: HWND) {
    HOSTED.with(|hosted| hosted.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr10_an_opened_extension_panel_moves_into_the_second_view_and_back() {
        assert_eq!(step::<u32>(None, None), Step::Nothing);
        assert_eq!(step(None, Some(&7)), Step::Take);
        assert_eq!(step(Some(&7), Some(&7)), Step::Nothing);
        assert_eq!(step(Some(&7), None), Step::GiveBack);
        assert_eq!(step(Some(&7), Some(&8)), Step::Swap);
    }

    #[test]
    fn fr10_the_extension_page_sits_below_the_header_right_of_the_hairline() {
        let docked = Mode::Docked { width: PANEL_WIDTH };
        assert_eq!(below_header(docked, [884, 44, 240, 700]), [885, 84, 239, 660]);
        // A split pane and the empty rail view are not panels.
        assert_eq!(below_header(Mode::Split, [884, 44, 240, 700]), [884, 44, 240, 700]);
        assert_eq!(below_header(Mode::Inset, [1124, 44, 0, 700]), [1124, 44, 0, 700]);
    }

    #[test]
    fn fr10_an_open_extension_panel_does_not_change_the_page_width() {
        // The browser narrowed the page by its 320 DIP panel; the page keeps the whole area.
        let mut contents = [0, 44, 844, 700];
        let mut side_panel = [844, 44, 320, 700];
        float_over(&mut contents, &mut side_panel, 1164);
        assert_eq!(contents, [0, 44, 1164, 700]);
        assert_eq!(side_panel, [1164, 44, 0, 700]);
    }
}
