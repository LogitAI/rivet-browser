//! The browser's infobar belongs to the pane whose tab it is about.
//!
//! Chromium keeps one `InfoBarContainerView` per window, right above the page, and its layout
//! gives it the whole width of the page area. With a split, that bar spanned both panes although
//! it speaks for the window's own page only. Rivet narrows it to the pane of the window's page
//! view, and the other pane rises to the bar's top so it does not keep an empty strip.

use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::HWND;

use crate::features::address_bar::rides_above_page;
use crate::features::second_view::Mode;

thread_local! {
    /// Per window: the height of the bars right above the page, and the page area width and the
    /// main pane's columns the last layout narrowed them to.
    static BARS: RefCell<HashMap<isize, Seen>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Seen {
    height: i32,
    width: i32,
    columns: Option<(i32, i32)>,
}

/// The height of the browser's bars right above the window's page, from the last layout.
pub(crate) fn bar_above(window: HWND) -> i32 {
    BARS.with(|bars| {
        bars.borrow()
            .get(&(window as isize))
            .map_or(0, |seen| seen.height)
    })
}

/// The second view's bounds `second` with the pane raised over the browser's bars of height
/// `height`, which belong to the other pane. A view that shows no page stays.
pub(crate) fn lift(mode: Mode, second: [i32; 4], height: i32) -> [i32; 4] {
    let [x, y, width, bounds_height] = second;
    if mode == Mode::Inset || height <= 0 || width <= 0 {
        return second;
    }
    let height = height.min(y.max(0));
    [x, y - height, width, bounds_height + height]
}

/// A bar's bounds narrowed to the main pane, which starts `columns.0` DIPs into the page
/// `contents` and is `columns.1` wide.
pub(crate) fn narrow(bar: [i32; 4], contents: [i32; 4], columns: (i32, i32)) -> [i32; 4] {
    let (x, width) = columns;
    [contents[0] + x, bar[1], width.max(1), bar[3]]
}

/// The page area step: the bars above the page take the main pane's columns while a split is
/// open, and their height is kept for the second view's placement.
pub(crate) unsafe fn on_page_area(area: &mut crate::hooks::page_area::PageArea) {
    let row = &crate::builds::get().side_panel;
    let Some(contents) = area.bounds(row.contents_container) else {
        return;
    };
    let page = *contents;
    let columns = crate::features::second_view::split_columns(area.window, page[2]);
    let mut height = 0;
    for (_, bounds) in area.all_bounds() {
        if bounds == contents || !rides_above_page(*bounds, page) {
            continue;
        }
        height += (*bounds)[3];
        if let Some(columns) = columns {
            *bounds = narrow(*bounds, page, columns);
        }
    }
    let seen = Seen {
        height,
        width: page[2],
        columns,
    };
    BARS.with(|bars| bars.borrow_mut().insert(area.window as isize, seen));
}

/// Whether the page area must be laid out again: the split's columns changed since the last
/// layout narrowed the bars, and there are bars to narrow.
fn stale(seen: Option<Seen>, columns: impl Fn(i32) -> Option<(i32, i32)>) -> bool {
    seen.is_some_and(|seen| seen.height > 0 && columns(seen.width) != seen.columns)
}

/// Lays the page area out again when a split opened, closed or moved its divider since the bars
/// were last narrowed. Called when the second view changes, outside the browser's layout.
pub(crate) unsafe fn follow(window: HWND) {
    let seen = BARS.with(|bars| bars.borrow().get(&(window as isize)).copied());
    if stale(seen, |width| {
        crate::features::second_view::split_columns(window, width)
    }) {
        crate::hooks::page_area::relayout(window);
    }
}

/// Drops a closed window.
pub(crate) fn forget(window: HWND) {
    BARS.with(|bars| bars.borrow_mut().remove(&(window as isize)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr9_an_infobar_spans_only_the_pane_that_owns_its_tab() {
        // Brave 156.1.98.63 showed a 56 DIP infobar across both panes of a split.
        let contents = [0, 100, 1164, 600];
        let bar = [0, 44, 1164, 56];
        assert_eq!(narrow(bar, contents, (0, 694)), [0, 44, 694, 56]);
        // The window's page on the right, after the second view on the left.
        assert_eq!(narrow(bar, contents, (470, 694)), [470, 44, 694, 56]);
    }

    #[test]
    fn fr9_the_other_pane_rises_to_the_infobar_top() {
        let second = [702, 100, 462, 600];
        assert_eq!(lift(Mode::Split, second, 56), [702, 44, 462, 656]);
        assert_eq!(
            lift(Mode::Docked { width: 240 }, second, 56),
            [702, 44, 462, 656]
        );
        // No bar, an empty view, or a view at the top leave it.
        assert_eq!(lift(Mode::Split, second, 0), second);
        assert_eq!(lift(Mode::Inset, second, 56), second);
        assert_eq!(lift(Mode::Split, [0, 20, 100, 100], 56), [0, 0, 100, 120]);
    }

    #[test]
    fn fr9_the_page_area_is_laid_out_again_only_when_the_columns_changed() {
        let seen = Seen {
            height: 56,
            width: 1164,
            columns: None,
        };
        assert!(stale(Some(seen), |_| Some((0, 694))), "a split opened");
        assert!(!stale(Some(seen), |_| None));
        let narrowed = Seen {
            columns: Some((0, 694)),
            ..seen
        };
        assert!(stale(Some(narrowed), |_| None), "the split closed");
        assert!(
            !stale(
                Some(Seen {
                    height: 0,
                    ..narrowed
                }),
                |_| None
            ),
            "no bar to widen"
        );
        assert!(!stale(None, |_| Some((0, 694))));
    }
}
