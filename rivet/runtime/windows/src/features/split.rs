//! Split view: two pages shown side by side in one window. How a split is made, ended and laid out
//! belongs to the browser, so the browser's backend does that. What is common lives here: which tabs
//! a split joins, the rows each page gets, and where a dropped tab lands.
use crate::builds::Build;
use crate::feature::Needs;
use crate::features::frame_layout::Rect;

use windows_sys::Win32::Foundation::HWND;

fn backend() -> &'static dyn crate::browsers::SplitBackend {
    crate::browsers::split_backend()
}

/// What split view needs from the browser.
pub(crate) fn needs(build: &Build) -> Needs {
    crate::browsers::split_backend_for(build).needs(build)
}

/// Makes the changes split view needs once per process.
pub unsafe fn install() -> Result<(), String> {
    backend().install()
}

/// Rivet's panel look for the pages of a split.
pub unsafe fn install_panels() -> Result<(), String> {
    backend().install_panels()
}

/// The part of the page area layout that belongs to split view: the pages of a split fill the area
/// the Sidebar left, and the toolbar follows the active page.
pub(crate) unsafe fn on_page_area(area: &mut crate::hooks::page_area::PageArea) {
    backend().on_page_area(area)
}

/// Whether the browser's address can follow the focused page. A toolbar that places it by hand keeps
/// it over the first page, so only that page's row shows the browser's address.
pub fn address_follows_page() -> bool {
    backend().address_follows_page()
}

/// The page area of a window as left, top, right and bottom in device-independent pixels.
pub unsafe fn page_area(window: HWND) -> Option<[i32; 4]> {
    // Where the second view is the split, the page view itself says where the pages go, so a
    // tab drop works without the browser's own split.
    if crate::features::second_view::available() {
        return crate::features::second_view::page_area(window)
            .or_else(|| backend().page_area(window));
    }
    backend().page_area(window)
}

/// The pages of the window's split, or none when the active tab is not in one.
pub unsafe fn panes(window: HWND) -> Vec<Pane> {
    // Where Rivet's extra views are the split, their pages are the panes, each with its own row.
    if crate::features::second_view::available() {
        return crate::features::second_view::panes(window);
    }
    backend().panes(window)
}

/// Whether the address of a pane's row can be edited. With the second view, Rivet's own address
/// field moves to the focused pane, but only while that pane shows the window's active tab, which
/// is the page the field opens addresses in. The browser's own split keeps its address in the top
/// line, and over the first page where the toolbar places it by hand.
pub fn row_editable(
    second_view: bool,
    active: bool,
    shows_active_tab: bool,
    top_line: bool,
    first: bool,
    follows: bool,
) -> bool {
    if second_view {
        active && shows_active_tab
    } else {
        active && top_line && (first || follows)
    }
}

/// How strongly the buttons and address of a page's row are drawn. The rows of the pages that do
/// not have the keyboard fade, so the focused page stands out, as the demo's `.toolbar { opacity: .55 }`.
pub fn row_ink_opacity(focused: bool) -> f32 {
    if focused {
        1.0
    } else {
        0.55
    }
}

/// Whether the active page of the window is shown in a split.
pub unsafe fn active_is_split(window: HWND) -> bool {
    if crate::features::second_view::available() {
        return crate::features::second_view::is_split(window);
    }
    backend().active_is_split(window)
}

/// Ends the split of the active page. Both pages stay open.
pub unsafe fn unsplit_active(window: HWND) -> bool {
    if crate::features::second_view::available() {
        // The second view lets go of the page, and its tab shows it again.
        return crate::features::second_view::is_split(window)
            && crate::features::second_view::close(window);
    }
    backend().unsplit_active(window)
}

/// Shows tab `other` in one of Rivet's extra page views on `side` of the page in slot `target`.
unsafe fn split_in_view(window: HWND, other: usize, target: usize, side: Side) -> bool {
    use crate::features::second_view::{add_tab, is_docked};
    // A docked page does not make way for a split.
    if is_docked(window) {
        return false;
    }
    add_tab(window, other, target, side)
}

/// The tab a split of the active tab joins when no tab is named: the one after it, or the one before
/// it when it is the last. None when the window has a single tab.
pub fn partner_of(active: usize, count: usize) -> Option<usize> {
    if active + 1 < count {
        Some(active + 1)
    } else if active > 0 && active < count {
        Some(active - 1)
    } else {
        None
    }
}

/// Splits the active tab with the tab after it, or before it when it is the last one. Returns false
/// when the window has a single tab or the active tab is already split.
pub unsafe fn split_active(window: HWND) -> bool {
    let Some(tabs) = crate::features::tabs::tabs(window) else {
        return false;
    };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    let Some(other) = partner_of(active, tabs.len()) else {
        return false;
    };
    if crate::features::second_view::available() {
        if active_is_split(window) {
            return false;
        }
        let side = if other < active { Side::Left } else { Side::Right };
        return split_in_view(window, other, 0, side);
    }
    backend().split_pair(window, active.min(other), active.max(other), false)
}

/// The side of a page a tab is dropped on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// The two tabs a drop makes a split of, the one shown first coming first, and whether they are
/// stacked. A tab dropped on the left or top of the page is shown before the active one.
pub fn drop_pair(active: usize, dragged: usize, side: Side) -> (usize, usize, bool) {
    let (first, second) = match side {
        Side::Left | Side::Top => (dragged, active),
        Side::Right | Side::Bottom => (active, dragged),
    };
    (first, second, matches!(side, Side::Top | Side::Bottom))
}

/// Puts `dragged` on `side` of the page in slot `target` of the window's split, or of the focused
/// page when no slot is named. Up to four pages share the window. Returns false when the tab is the
/// active one, does not exist, is already shown, or the split is full.
pub unsafe fn split_beside_page(window: HWND, dragged: usize, side: Side, target: Option<usize>) -> bool {
    let Some(tabs) = crate::features::tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    if dragged == active || dragged >= tabs.len() {
        return false;
    }
    if crate::features::second_view::available() {
        let target = target.unwrap_or_else(|| crate::features::second_view::focused_slot(window));
        return split_in_view(window, dragged, target, side);
    }
    let (first, second, stacked) = drop_pair(active, dragged, side);
    backend().split_pair(window, first, second, stacked)
}

/// Whether the window takes no more pages in its split, so a drop only opens the tab.
pub unsafe fn split_full(window: HWND) -> bool {
    if crate::features::second_view::available() {
        return !crate::features::second_view::has_room(window);
    }
    // The browser's own split keeps two pages.
    active_is_split(window)
}

/// The page a drop at (`x`, `y`) in window DIPs lands on, as x, y, width and height, with the slot
/// of the split that shows it. Without a split, that is the whole page area.
pub unsafe fn drop_page(window: HWND, x: i32, y: i32) -> Option<([i32; 4], Option<usize>)> {
    for pane in panes(window) {
        if x >= pane.x && x < pane.x + pane.width && y >= pane.y && y < pane.y + pane.height {
            let slot = if crate::features::second_view::is_split(window) { crate::features::second_view::slot_of_pane(window, &pane) } else { None };
            return Some(([pane.x, pane.y, pane.width, pane.height], slot));
        }
    }
    let [left, top, right, bottom] = page_area(window)?;
    Some(([left, top, right - left, bottom - top], None))
}

/// The tabs whose pages the window's split shows, in reading order.
pub unsafe fn shown_tabs(window: HWND) -> Vec<usize> {
    panes(window).iter().filter_map(|pane| pane.tab).collect()
}

/// Takes the page of `slot` out of the window's split and floats it in a Mini Window. The other
/// pages stay, and the split ends when one page is left.
pub unsafe fn pop_out(window: HWND, slot: usize) -> bool {
    let tab = panes(window)
        .iter()
        .find(|pane| crate::features::second_view::slot_of_pane(window, pane) == Some(slot))
        .and_then(|pane| pane.tab);
    let Some(tab) = tab else {
        return false;
    };
    if !crate::features::second_view::close_pane(window, slot) {
        return false;
    }
    crate::features::mini_window::open(window, tab)
}

/// The part of a page near each edge where a drop splits it, as a share of its size.
const EDGE_SHARE: f32 = 0.28;

/// Where a tab dropped at (`x`, `y`) on a page of `page` (x, y, width, height) lands, and the
/// rectangle that shows it. A drop near an edge splits the page there and shows that half. A drop
/// elsewhere, or on a window whose split is full, opens the tab in the page.
pub fn drop_zone(page: [f32; 4], x: f32, y: f32, full: bool, gap: f32) -> (Option<Side>, [f32; 4]) {
    let [left, top, width, height] = page;
    let distances = [
        (Side::Left, (x - left) / width),
        (Side::Right, (left + width - x) / width),
        (Side::Top, (y - top) / height),
        (Side::Bottom, (top + height - y) / height),
    ];
    let (nearest, amount) = distances
        .into_iter()
        .min_by(|one, two| one.1.total_cmp(&two.1))
        .expect("four distances");
    let side = (amount < EDGE_SHARE && !full).then_some(nearest);
    let (half_width, half_height) = ((width - gap) / 2.0, (height - gap) / 2.0);
    let rect = match side {
        Some(Side::Left) => [left, top, half_width, height],
        Some(Side::Right) => [left + half_width + gap, top, half_width, height],
        Some(Side::Top) => [left, top, width, half_height],
        Some(Side::Bottom) => [left, top + half_height + gap, width, half_height],
        None => page,
    };
    (side, rect)
}

/// One-shot diagnostic for the isolated test profile: opens a second tab and splits.
pub unsafe fn probe(window: HWND) {
    if crate::features::tabs::tabs(window).is_some_and(|tabs| tabs.len() == 1) {
        crate::features::tabs::new_tab(window);
    }
    let split = split_active(window);
    crate::trace::trace(&format!(
        "split probe: split {split} active_is_split {}",
        active_is_split(window)
    ));
    if crate::frame::debug_directory().is_some_and(|dir| dir.join("rivet-unsplit.flag").exists()) {
        let ended = toggle_split(window);
        let count = crate::features::tabs::tabs(window).map(|tabs| tabs.len());
        crate::trace::trace(&format!(
            "split probe: unsplit {ended} active_is_split {} tabs {count:?}",
            active_is_split(window)
        ));
    }
}

/// Asks the window to lay the browser out again once the current layout is done, because the top
/// row follows the pages of a split.
pub const RELAYOUT_MESSAGE: &str = "Rivet.SplitLayout";

/// Where the row of a page goes, given the top of its page. Pages on the first line have their row
/// at the top of the browser view, as a single page does, because bars the browser keeps between
/// the row and the pages, like the bookmarks bar, stay below the row. A lower page's row sits right
/// above it.
pub fn row_top(first_line: bool, view_top: i32, page_top: i32) -> i32 {
    if first_line {
        view_top
    } else {
        page_top - ROW_HEIGHT
    }
}

/// The pages of a split fill the area the browser leaves for its pages, whatever insets it keeps
/// there, so they start where a single page does. The top stays below the browser's bars.
pub fn fill_area(contents: &mut [i32; 4], area: &[i32; 4]) {
    let bottom = area[1] + area[3];
    contents[0] = area[0];
    contents[2] = area[2];
    contents[3] = (bottom - contents[1]).max(1);
}

/// The height of the top row of a page, in device-independent pixels.
pub const ROW_HEIGHT: i32 = 44;
/// Room on the first page's row for the appbar toggle, and for the grip on every row.
pub const TOGGLE_ROOM: f32 = 28.0;
const BUTTON: f32 = 28.0;
const BUTTON_GAP: f32 = 2.0;
const ROW_PAD: f32 = 6.0;
/// The native caption buttons, which sit over the last page's row.
pub const CAPTION_ROOM: f32 = 138.0;

/// What a button of a page's top row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowButton {
    Grip,
    Back,
    Forward,
    Reload,
    Address,
    Popout,
    Unsplit,
}

/// The narrowest address worth showing, with its margins.
const ADDRESS_MIN: f32 = 56.0;
const ADDRESS_LEAD: f32 = 6.0;
const ADDRESS_TRAIL: f32 = 4.0;

/// The buttons of a page's top row as (button, x, width) from the page's left edge, for a page
/// `width` wide. The first page leaves room for the appbar toggle, and the page under the caption
/// buttons for the `caption` width they take from its row. A row that is too narrow drops buttons instead of overlapping them: the Mini Window
/// button first, then forward, the grip, reload and back. The button that ends the split stays.
pub fn row_buttons(width: f32, first: bool, caption: f32) -> Vec<(RowButton, f32, f32)> {
    let start = if first { TOGGLE_ROOM } else { 0.0 } + 2.0;
    let end = width - caption - ROW_PAD;
    let free = end - start;
    let mut left = vec![
        RowButton::Grip,
        RowButton::Back,
        RowButton::Forward,
        RowButton::Reload,
    ];
    let mut right = vec![RowButton::Popout, RowButton::Unsplit];
    let needed = |left: &[RowButton], right: &[RowButton]| {
        let pitch = BUTTON + BUTTON_GAP;
        pitch * left.len() as f32
            + ADDRESS_LEAD
            + ADDRESS_MIN
            + ADDRESS_TRAIL
            + pitch * right.len() as f32
            - BUTTON_GAP
    };
    for drop in [
        RowButton::Popout,
        RowButton::Forward,
        RowButton::Grip,
        RowButton::Reload,
        RowButton::Back,
    ] {
        if needed(&left, &right) <= free {
            break;
        }
        left.retain(|button| *button != drop);
        right.retain(|button| *button != drop);
    }
    let mut buttons = Vec::new();
    let mut x = start;
    for button in &left {
        buttons.push((*button, x, BUTTON));
        x += BUTTON + BUTTON_GAP;
    }
    let right_start =
        end - right.len() as f32 * BUTTON - (right.len() as f32 - 1.0).max(0.0) * BUTTON_GAP;
    let address = x + ADDRESS_LEAD;
    let address_width = right_start - ADDRESS_TRAIL - address;
    if address_width >= ADDRESS_MIN {
        buttons.push((RowButton::Address, address, address_width));
    }
    for (index, button) in right.iter().enumerate() {
        buttons.push((
            *button,
            right_start + index as f32 * (BUTTON + BUTTON_GAP),
            BUTTON,
        ));
    }
    buttons
}

/// One page of a split in the window's client area, in device-independent pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pane {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// The top of the page's row.
    pub row_y: i32,
    pub active: bool,
    pub first: bool,
    pub last: bool,
    /// How much of the row's right end the caption buttons cover. The buttons sit at the window's top
    /// right, so a page that ends before the Sidebar's rail is covered by less than their full width.
    pub caption: f32,
    /// The strip index of the tab this page shows, or none for a page that is no tab of the strip.
    pub tab: Option<usize>,
}

/// Whether the pages share one row, as opposed to being stacked.
pub(crate) fn side_by_side(rects: &[Rect]) -> bool {
    rects.windows(2).all(|pair| pair[0].y == pair[1].y)
}

/// Whether the pages are laid out as a row or as a column, which are the layouts Rivet draws rows
/// for.
pub(crate) fn in_a_line(rects: &[Rect]) -> bool {
    side_by_side(rects) || rects.windows(2).all(|pair| pair[0].x == pair[1].x)
}

/// The right edge of the window's client area in device-independent pixels.
pub(crate) unsafe fn client_right(window: HWND) -> i32 {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let dpi = GetDpiForWindow(window).max(96) as i32;
    client.right * 96 / dpi
}

/// The width of the caption buttons that covers a row ending at `pane_right`, in a window whose
/// client area ends at `window_right`.
pub fn caption_over(window_right: i32, pane_right: i32) -> f32 {
    (CAPTION_ROOM - (window_right - pane_right) as f32).clamp(0.0, CAPTION_ROOM)
}

/// Splits the active tab, or ends the split it is in. The keyboard shortcut and the appbar share
/// this, so both do the same thing.
pub unsafe fn toggle_split(window: HWND) -> bool {
    if active_is_split(window) {
        unsplit_active(window)
    } else {
        split_active(window)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        caption_over, drop_pair, drop_zone, fill_area, partner_of, row_buttons, row_top, RowButton,
        Side, CAPTION_ROOM, TOGGLE_ROOM,
    };

    #[test]
    fn fr9_split_with_a_new_start_tab_joins_the_tab_after_the_active_one() {
        // The start entries open a start tab in the window first; the split then takes it.
        assert_eq!(partner_of(0, 2), Some(1));
        assert_eq!(partner_of(0, 1), None);
    }

    #[test]
    fn fr9_a_split_without_a_named_tab_joins_the_next_tab_or_the_one_before_the_last() {
        assert_eq!(partner_of(0, 2), Some(1));
        assert_eq!(partner_of(1, 3), Some(2));
        assert_eq!(partner_of(2, 3), Some(1));
        assert_eq!(partner_of(0, 1), None, "a lone tab has nobody to join");
    }

    #[test]
    fn fr9_a_tab_dropped_on_the_left_or_top_is_shown_before_the_active_one() {
        assert_eq!(drop_pair(3, 1, Side::Left), (1, 3, false));
        assert_eq!(drop_pair(3, 1, Side::Right), (3, 1, false));
        assert_eq!(drop_pair(3, 1, Side::Top), (1, 3, true));
        assert_eq!(drop_pair(3, 1, Side::Bottom), (3, 1, true));
    }

    #[test]
    fn fr9_first_line_rows_stay_above_the_browsers_bars_and_lower_rows_sit_on_their_page() {
        // A bookmarks bar puts the page 35 DIPs lower, and the row stays at the top.
        assert_eq!(row_top(true, 8, 87), 8);
        assert_eq!(row_top(false, 8, 400), 356);
    }

    #[test]
    fn fr9_split_pages_fill_the_browsers_page_area_without_its_insets() {
        let mut contents = [271, 135, 1157, 699];
        fill_area(&mut contents, &[267, 79, 1165, 759]);
        assert_eq!(contents, [267, 135, 1165, 703]);
    }

    #[test]
    fn fr9_tab_dropped_on_an_edge_splits_that_side() {
        let page = [100.0, 50.0, 800.0, 600.0];
        let (side, rect) = drop_zone(page, 880.0, 300.0, false, 8.0);
        assert_eq!(side, Some(Side::Right));
        assert_eq!(rect, [100.0 + 396.0 + 8.0, 50.0, 396.0, 600.0]);
        assert_eq!(
            drop_zone(page, 120.0, 300.0, false, 8.0).0,
            Some(Side::Left)
        );
        assert_eq!(drop_zone(page, 500.0, 60.0, false, 8.0).0, Some(Side::Top));
        assert_eq!(
            drop_zone(page, 500.0, 640.0, false, 8.0).0,
            Some(Side::Bottom)
        );
    }

    #[test]
    fn fr9_tab_dropped_in_the_middle_or_on_a_full_split_opens_in_place() {
        let page = [100.0, 50.0, 800.0, 600.0];
        assert_eq!(drop_zone(page, 500.0, 350.0, false, 8.0), (None, page));
        assert_eq!(drop_zone(page, 880.0, 300.0, true, 8.0), (None, page));
    }

    #[test]
    fn fr9_each_page_row_has_grip_popout_and_unsplit() {
        let buttons = row_buttons(400.0, false, 0.0);
        let names: Vec<RowButton> = buttons.iter().map(|button| button.0).collect();
        assert_eq!(
            names,
            [
                RowButton::Grip,
                RowButton::Back,
                RowButton::Forward,
                RowButton::Reload,
                RowButton::Address,
                RowButton::Popout,
                RowButton::Unsplit
            ]
        );
        let unsplit = buttons.last().unwrap();
        assert_eq!(unsplit.1 + unsplit.2, 400.0 - 6.0);
    }

    #[test]
    fn fr9_first_row_leaves_room_for_the_toggle_and_last_for_the_caption_buttons() {
        let first = row_buttons(400.0, true, 0.0);
        assert!(first[0].1 >= TOGGLE_ROOM);
        let last = row_buttons(500.0, false, CAPTION_ROOM);
        let unsplit = last.last().unwrap();
        assert!(unsplit.1 + unsplit.2 <= 500.0 - CAPTION_ROOM);
    }

    /// No two buttons of a row overlap, nothing leaves the row, and the caption buttons keep their
    /// room, at every page width a 900 DIP window with the Sidebar can give.

    #[test]
    fn fr9_page_rows_never_overlap_at_any_width() {
        for width in 200..=900 {
            for (first, last) in [(true, false), (false, false), (false, true)] {
                let covered = if last { CAPTION_ROOM } else { 0.0 };
                let buttons = row_buttons(width as f32, first, covered);
                let limit = width as f32 - covered;
                let mut edge = if first { TOGGLE_ROOM } else { 0.0 };
                for (_, x, w) in &buttons {
                    assert!(*x >= edge, "overlap at {width} {first} {last}: {buttons:?}");
                    edge = x + w;
                }
                assert!(
                    edge <= limit,
                    "row runs under the caption at {width}: {buttons:?}"
                );
                assert!(buttons.iter().any(|b| b.0 == RowButton::Unsplit));
            }
        }
    }

    #[test]
    fn fr9_narrow_row_drops_the_mini_window_button_before_the_address() {
        let names: Vec<RowButton> = row_buttons(303.0, false, CAPTION_ROOM)
            .iter()
            .map(|b| b.0)
            .collect();
        assert!(!names.contains(&RowButton::Popout));
        assert!(names.contains(&RowButton::Address) && names.contains(&RowButton::Unsplit));
    }

    #[test]
    fn fr9_only_the_focused_panes_row_is_drawn_at_full_strength() {
        assert_eq!(super::row_ink_opacity(true), 1.0);
        assert_eq!(super::row_ink_opacity(false), 0.55);
    }

    #[test]
    fn fr12_the_address_is_edited_in_the_focused_pane_of_a_split() {
        use super::row_editable;
        // Second view: the focused pane that shows the active tab, wherever it sits.
        assert!(row_editable(true, true, true, true, false, false));
        assert!(
            !row_editable(true, false, true, true, true, true),
            "an unfocused pane only shows its address"
        );
        assert!(
            !row_editable(true, true, false, true, true, true),
            "a page of another window has no field"
        );
        // The browser's own split keeps the old rule.
        assert!(row_editable(false, true, true, true, true, false));
        assert!(!row_editable(false, true, true, true, false, false));
        assert!(!row_editable(false, true, true, false, true, true));
    }

    #[test]
    fn fr9_caption_covers_only_what_the_pages_edge_leaves_of_it() {
        assert_eq!(caption_over(1000, 1000), CAPTION_ROOM);
        assert_eq!(caption_over(1000, 900), CAPTION_ROOM - 100.0);
        assert_eq!(caption_over(1000, 800), 0.0);
    }
}
