//! Rivet's second page view: a Chromium `ContentsWebView` of Rivet's own, added to the browser
//! view beside the window's page, which shows a page whose tab another window owns. Only Chromium's
//! own views functions are called. The addresses come from the per-build record that
//! `app_window::recorded` reads.
//!
//! The same view shows a split page beside the window's page, or a sidebar page docked over the
//! right part of it.
//!
//! The browser lays the window's page view out to the full page area. Rivet then narrows it. The
//! layout follows the window's own size messages on every browser. Where a build records the page
//! view's `OnBoundsChanged` slot, a hook on that one object also asks for a layout, but it never
//! moves views itself: it posts a message, so no layout runs inside another.
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::HWND;

use crate::features::app_window::recorded;

/// The gap between the two pages, which is the divider Rivet draws and drags, in DIPs.
pub(crate) const DIVIDER: i32 = 8;
/// The narrowest a page of the split may get, in DIPs.
pub(crate) const MIN_PANE: i32 = 120;

/// The width of a docked sidebar page, in DIPs.
pub(crate) const SIDEBAR_WIDTH: i32 = 360;

/// The window message that lays the second view out again, posted by the hook and by size changes.
pub(crate) const LAYOUT_MESSAGE: &str = "Rivet.SecondViewLayout";

/// How the second view sits beside the window's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Side by side with a divider. `leading` puts the second view on the left.
    Split { leading: bool },
    /// Docked over the right edge of the page, which keeps its full width.
    Docked { width: i32 },
    /// No page in the second view. The window's page view only leaves the Sidebar's rail free.
    Inset,
}

/// The width the Sidebar's rail keeps free at the right of the page area, by window.
static RAILS: Mutex<Vec<(isize, i32)>> = Mutex::new(Vec::new());

fn rail_of(window: isize) -> i32 {
    RAILS.lock().unwrap_or_else(|error| error.into_inner()).iter().find(|(key, _)| *key == window).map_or(0, |(_, width)| *width)
}

/// The height Rivet's address bar keeps free at the top of the page area, by window.
static TOPS: Mutex<Vec<(isize, i32)>> = Mutex::new(Vec::new());

fn top_of(window: isize) -> i32 {
    TOPS.lock().unwrap_or_else(|error| error.into_inner()).iter().find(|(key, _)| *key == window).map_or(0, |(_, height)| *height)
}

/// The page area the browser gave the page view with `top` DIPs at its top left free for the
/// address bar's row. The pages of every mode are laid out in what is left.
pub(crate) fn below_bar(full: [i32; 4], top: i32) -> [i32; 4] {
    let top = top.clamp(0, (full[3] - 1).max(0));
    [full[0], full[1] + top, full[2], (full[3] - top).max(1)]
}

/// How many DIPs at the top of the page area the bar row covers. The bar sits at the top of the
/// browser's panel, `panel_top`, and is `height` tall. Most browsers start the page area right at
/// the panel's top, so the whole row is taken from it. A browser that starts its page area lower,
/// below a strip of its own, gives up only the part the row still covers, so the page starts at
/// the bar's bottom and the strip stays under the bar instead of showing as a band.
pub(crate) fn bar_inset(height: i32, panel_top: Option<i32>, area_top: i32) -> i32 {
    match panel_top {
        Some(top) if height > 0 => (top + height - area_top).clamp(0, height),
        _ => height,
    }
}

/// The bar row's inset of the page area that the browser gave the page view as `full`.
unsafe fn top_inset(split: &Split, full: [i32; 4]) -> i32 {
    let height = top_of(split.window);
    if height == 0 {
        return 0;
    }
    let [_, origin_y] = window_origin(split.main);
    let area_top = origin_y - bounds_of(split.main)[1] + full[1];
    let panel = crate::features::frame_layout::panel_bounds(split.window as HWND).map(|panel| panel.y);
    bar_inset(height, panel, area_top)
}

/// Whether the second view's page has the keyboard, for a page whose tab is in another window:
/// a click into it never changes this window's active tab, so the click itself is remembered.
static SECOND_FOCUSED: AtomicBool = AtomicBool::new(false);

/// The two pages of a split as panes with their own rows, left to right. `main` and `view` are the
/// window's page view and the second view in window DIPs as x, y, width and height. Each row sits
/// right above its page, and the pane at the window's top right carries the caption buttons.
pub(crate) fn pane_pair(
    main: [i32; 4],
    view: [i32; 4],
    second_active: bool,
    tabs: [Option<usize>; 2],
    window_right: i32,
    panel_top: Option<i32>,
) -> Vec<crate::features::split::Pane> {
    use crate::features::split::{caption_over, Pane, ROW_HEIGHT};
    let mut pages = [(main, !second_active, tabs[0]), (view, second_active, tabs[1])];
    pages.sort_by_key(|(rect, _, _)| rect[0]);
    pages
        .iter()
        .enumerate()
        .map(|(index, (rect, active, tab))| Pane {
            x: rect[0],
            y: rect[1],
            width: rect[2],
            height: rect[3],
            // A browser bar above a page, such as an infobar, stays between the row and the page:
            // the row keeps the panel's top.
            row_y: panel_top.map_or(rect[1] - ROW_HEIGHT, |top| top.min(rect[1] - ROW_HEIGHT)),
            active: *active,
            first: index == 0,
            last: index == 1,
            caption: caption_over(window_right, rect[0] + rect[2]),
            tab: *tab,
        })
        .collect()
}

/// The strip index of the window's tab that holds `page`.
unsafe fn tab_of(window: HWND, page: usize) -> Option<usize> {
    let base = base()?;
    let model = crate::features::tabs::tab_model(base, window)?;
    let count = crate::features::tabs::tabs(window)?.len();
    (page != 0).then_some(())?;
    (0..count).find(|index| crate::features::tabs::contents_at(base, model, *index as i32) as usize == page)
}

/// The pages of the window's split in the second view, each with its own row, or none.
pub(crate) unsafe fn panes(window: HWND) -> Vec<crate::features::split::Pane> {
    let Some(state) = split().filter(|split| split.window == window as isize && matches!(split.mode, Mode::Split { .. })) else {
        return Vec::new();
    };
    let rect = |view: usize| {
        let [x, y] = window_origin(view);
        let bounds = bounds_of(view);
        [x, y, bounds[2], bounds[3]]
    };
    let (main, view) = (rect(state.main), rect(state.view));
    if main[2] <= 0 || view[2] <= 0 {
        return Vec::new();
    }
    let second_tab = tab_of(window, state.page);
    let second_active = match second_tab {
        // A tab of this window: the browser makes it active when its page is clicked.
        Some(_) => {
            let active = active_page(window);
            active != 0 && active == state.page
        }
        None => SECOND_FOCUSED.load(Ordering::Acquire),
    };
    pane_pair(main, view, second_active, [tab_of(window, state.shown), second_tab], crate::features::split::client_right(window), crate::features::frame_layout::panel_bounds(window).map(|panel| panel.y))
}

/// Whether `pane` of the window's split is the second view's page.
pub(crate) unsafe fn is_second_pane(window: HWND, pane: &crate::features::split::Pane) -> bool {
    split().is_some_and(|split| split.window == window as isize && window_origin(split.view)[0] == pane.x)
}

/// Remembers which page of the split has the keyboard, for a page of another window.
pub(crate) fn set_second_focused(focused: bool) {
    SECOND_FOCUSED.store(focused, Ordering::Release);
}

/// Whether the empty view stays open to keep a strip of the page area free.
fn keeps_inset(window: isize) -> bool {
    rail_of(window) > 0 || top_of(window) > 0
}

/// What the split of one window holds.
#[derive(Clone, Copy, Debug)]
struct Split {
    mode: Mode,
    window: isize,
    /// Rivet's `ContentsWebView`.
    view: usize,
    /// The browser view's `views::View`, the parent of Rivet's view.
    parent: usize,
    /// The window's own page view, whose bounds the hook narrows.
    main: usize,
    /// The vtable the page view had before the hook's copy replaced it.
    main_vtable: usize,
    /// The page view of the window that owns the shown page, which let go of it, and the page.
    owner_view: usize,
    page: usize,
    /// The page the window's page view shows, which is not always the active tab's page.
    shown: usize,
    /// The window's active page when Rivet looked last, so only a change of it is acted on.
    active: usize,
}

/// What Rivet does when the window's active page may have changed while its split is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keep {
    /// Nothing changed.
    Nothing,
    /// The split page's tab became active, so the browser moved that page into the window's page
    /// view. The page view gets `main` back and the second view gets the split page back. The tab
    /// stays active, so the address bar and the tab list follow the page that was clicked.
    Restore { main: usize },
    /// Another tab became active and the browser shows it in the window's page view, as it should.
    Follow { main: usize },
}

/// Decides what a change of the window's active page means for an open split of the same window.
pub(crate) fn keep(active: usize, last_active: usize, split_page: usize, shown: usize) -> Keep {
    if active == 0 || active == last_active {
        return Keep::Nothing;
    }
    if active == split_page {
        return if shown != 0 && shown != split_page { Keep::Restore { main: shown } } else { Keep::Nothing };
    }
    Keep::Follow { main: active }
}

/// The page the window's page view should show when the split closes: the active page, so the
/// window agrees with its tab strip again.
pub(crate) fn page_after_close(active: usize, shown: usize) -> Option<usize> {
    (active != 0 && active != shown).then_some(active)
}

static SPLIT: Mutex<Option<Split>> = Mutex::new(None);
/// The first page's share of the width in thousandths.
static SHARE: AtomicU32 = AtomicU32::new(600);
/// The browser's `OnBoundsChanged` of the page view, which the hook calls first.
static ORIGINAL_BOUNDS_CHANGED: AtomicUsize = AtomicUsize::new(0);
/// Set while the hook moves views, so the bounds it sets do not run it again.
static PLACING: AtomicBool = AtomicBool::new(false);
/// The bounds the browser's layout gave the page view last, before the hook narrowed it.
static FULL: Mutex<[i32; 4]> = Mutex::new([0; 4]);
/// How many times the browser laid the page view out while the split was open, for the trace.
static LAYOUTS: AtomicU32 = AtomicU32::new(0);
/// The bounds Rivet set on the page view last. Bounds that differ came from the browser's layout.
static LAST: Mutex<[i32; 4]> = Mutex::new([0; 4]);
/// Set while a layout message is posted and not yet handled, so the hook posts one at a time.
static PENDING: AtomicBool = AtomicBool::new(false);

fn split() -> Option<Split> {
    *SPLIT.lock().unwrap_or_else(|error| error.into_inner())
}

/// Whether this build records what the second view needs, which makes it the split and sidebar
/// mechanism of the build.
pub(crate) fn available() -> bool {
    recorded("new").is_some() && recorded("ctor").is_some() && recorded("add_child").is_some()
}

/// Whether the window shows a docked sidebar page now.
pub(crate) fn is_docked(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize && matches!(split.mode, Mode::Docked { .. }))
}

/// Whether the window shows a split page now.
pub(crate) fn is_split(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize && matches!(split.mode, Mode::Split { .. }))
}

/// Whether the window shows a second view now.
pub(crate) fn is_open(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize)
}

/// A word of memory that may not be mapped.
unsafe fn word(address: usize) -> Option<usize> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut value = 0usize;
    let mut done = 0usize;
    let ok = ReadProcessMemory(GetCurrentProcess(), address as *const c_void, (&mut value as *mut usize).cast(), 8, &mut done);
    (ok != 0 && done == 8).then_some(value)
}

fn base() -> Option<usize> {
    crate::features::tabs::base()
}

/// Where a View keeps its parent, its children and its bounds in this build.
fn offsets() -> (usize, usize, usize) {
    let parent = recorded("vparent").unwrap_or(0x198);
    let children = recorded("vchildren").unwrap_or(0x1A0);
    (parent, children, recorded("vbounds").unwrap_or(parent + 0x30))
}

unsafe fn bounds_of(view: usize) -> [i32; 4] {
    *((view + offsets().2) as *const [i32; 4])
}

/// The children of a view. The list is (begin, end, capacity) or (begin, count, capacity), by build.
unsafe fn children_of(view: usize) -> Vec<usize> {
    let offset = offsets().1;
    let begin = word(view + offset).unwrap_or(0);
    let second = word(view + offset + 8).unwrap_or(0);
    let capacity = word(view + offset + 16).unwrap_or(0);
    if begin < 0x10000 {
        return Vec::new();
    }
    let count = if second >= begin && (second - begin) % 8 == 0 && (second - begin) / 8 <= 512 {
        (second - begin) / 8
    } else if second <= 512 && second <= capacity {
        second
    } else {
        return Vec::new();
    };
    (0..count).filter_map(|index| word(begin + index * 8)).collect()
}

/// The views below `root` whose vtable is `vtable`, which are the page views of that window.
unsafe fn views_with_vtable(root: usize, vtable: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut queue = vec![(root, 0usize)];
    let mut visited = 0;
    while let Some((view, depth)) = queue.pop() {
        visited += 1;
        if visited > 600 || depth > 8 {
            continue;
        }
        if word(view) == Some(vtable) {
            found.push(view);
        }
        queue.extend(children_of(view).into_iter().map(|child| (child, depth + 1)));
    }
    found
}

/// Whether a page view holds this page in one of its words, as `WebView::web_contents_`.
unsafe fn holds(view: usize, page: usize, size: usize) -> bool {
    (8..size).step_by(8).any(|offset| word(view + offset) == Some(page))
}

/// The browser view's `views::View` of a window.
unsafe fn browser_view(window: HWND) -> Option<usize> {
    let base = base()?;
    let end = base + crate::chromium::image::recorded().map_or(0, |image| image.size);
    let part = crate::features::tabs::browser_view(window)? as usize;
    for step in (0..0x2000usize).step_by(8) {
        let q = part.wrapping_sub(0x1000).wrapping_add(step);
        if !word(q).is_some_and(|vptr| vptr > base && vptr < end) {
            continue;
        }
        let children = children_of(q);
        if (3..=64).contains(&children.len()) && children.iter().all(|child| word(*child).is_some_and(|vptr| vptr > base && vptr < end)) {
            return Some(q);
        }
    }
    None
}

/// `ContentsWebView::SetWebContents`, a function of the class or a slot of its vtable, by build.
unsafe fn set_page(view: usize, page: usize) -> bool {
    let Some(base) = base() else { return false };
    if let Some(rva) = recorded("set_contents") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize)>(base, rva)(view, page);
    } else if let Some(slot) = recorded("slot") {
        let function = *((*(view as *const usize) + slot * 8) as *const usize);
        std::mem::transmute::<usize, unsafe extern "system" fn(usize, usize)>(function)(view, page);
    } else {
        return false;
    }
    true
}

/// `View::SetBoundsRect`, or, in a build where it is not found, the bounds written into the view
/// followed by its own `OnBoundsChanged`.
unsafe fn set_bounds(view: usize, rect: [i32; 4]) {
    let Some(base) = base() else { return };
    if let Some(rva) = recorded("set_bounds") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize, *const [i32; 4])>(base, rva)(view, &rect);
        return;
    }
    let previous = bounds_of(view);
    if previous == rect {
        return;
    }
    *((view + offsets().2) as *mut [i32; 4]) = rect;
    if let Some(slot) = recorded("bounds_slot") {
        let function = *((*(view as *const usize) + slot * 8) as *const usize);
        std::mem::transmute::<usize, unsafe extern "system" fn(usize, *const [i32; 4])>(function)(view, &previous);
    }
}

/// The virtual function at `slot` of a view's vtable, or the recorded function `name`.
unsafe fn virtual_of(view: usize, name: &str, slot_name: &str) -> Option<usize> {
    let base = base()?;
    recorded(name)
        .map(|rva| base + rva)
        .or_else(|| recorded(slot_name).map(|slot| *((*(view as *const usize) + slot * 8) as *const usize)))
}

/// The two pages' bounds for the full page area `full`, both in the page view's parent.
pub(crate) fn divide(full: [i32; 4], share: u32) -> ([i32; 4], [i32; 4]) {
    let [x, y, width, height] = full;
    let room = (width - DIVIDER).max(0);
    let mut first = (room as i64 * share as i64 / 1000) as i32;
    if room >= 2 * MIN_PANE {
        first = first.clamp(MIN_PANE, room - MIN_PANE);
    }
    ([x, y, first, height], [x + first + DIVIDER, y, room - first, height])
}

/// The window's page view bounds and the second view's, both in the page view's parent, for the
/// full page area `full`. `rail` DIPs at the right of the area stay free for the Sidebar's rail.
pub(crate) fn layout(full: [i32; 4], mode: Mode, share: u32, rail: i32) -> ([i32; 4], [i32; 4]) {
    let [x, y, w, h] = full;
    let full = [x, y, (w - rail.max(0)).max(1), h];
    match mode {
        Mode::Inset => (full, [full[0] + full[2], y, 0, h]),
        Mode::Split { leading: false } => divide(full, share),
        Mode::Split { leading: true } => {
            // The second view takes the left part, the page the right, with the same share rules.
            let (left, right) = divide(full, share);
            (right, left)
        }
        Mode::Docked { width } => {
            let [x, y, w, h] = full;
            let width = width.clamp(0, w.max(0));
            (full, [x + w - width, y, width, h])
        }
    }
}

/// The offset of a view's parent from `ancestor`, summing the bounds of the views between them.
unsafe fn offset_in(view: usize, ancestor: usize) -> [i32; 2] {
    let parent_offset = offsets().0;
    let (mut x, mut y) = (0, 0);
    let mut at = word(view + parent_offset).unwrap_or(0);
    for _ in 0..16 {
        if at == 0 || at == ancestor {
            break;
        }
        let bounds = bounds_of(at);
        x += bounds[0];
        y += bounds[1];
        at = word(at + parent_offset).unwrap_or(0);
    }
    [x, y]
}

/// Puts both pages where the share says, from the page area the browser last gave the page view.
unsafe fn place(split: &Split) -> bool {
    // Bounds Rivet did not set came from the browser's layout and are the new page area.
    let now = bounds_of(split.main);
    let last = *LAST.lock().unwrap_or_else(|error| error.into_inner());
    if now != last && now[2] > 0 {
        *FULL.lock().unwrap_or_else(|error| error.into_inner()) = now;
    }
    let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
    if full[2] <= 0 {
        return false;
    }
    let full = below_bar(full, top_inset(split, full));
    let (first, second) = layout(full, split.mode, SHARE.load(Ordering::Relaxed), rail_of(split.window));
    let [dx, dy] = offset_in(split.main, split.parent);
    let second = [second[0] + dx, second[1] + dy, second[2], second[3]];
    // Bounds that are already right are not set again, so a layout does not ask for another one.
    *LAST.lock().unwrap_or_else(|error| error.into_inner()) = first;
    if now == first && bounds_of(split.view) == second {
        return false;
    }
    PLACING.store(true, Ordering::Release);
    set_bounds(split.main, first);
    set_bounds(split.view, second);
    PLACING.store(false, Ordering::Release);
    true
}

/// Lays the second view of `window` out again from the page area the browser gave its page view
/// last. Returns whether a view moved. The size messages of the window and the hook's message both
/// end here, outside any browser layout.
pub(crate) unsafe fn relayout(window: HWND) -> bool {
    PENDING.store(false, Ordering::Release);
    hold(window);
    // A bar row asked for before the view existed opens the empty view here, outside any browser
    // layout and outside the appbar's paint.
    if split().is_none() && top_of(window as isize) > 0 {
        let opened = open_with(window, window, 0, Mode::Inset);
        crate::trace::trace(&format!("second view: bar row {}: {opened}", top_of(window as isize)));
        return opened;
    }
    let Some(split) = split().filter(|split| split.window == window as isize) else { return false };
    let moved = place(&split);
    if moved {
        let count = LAYOUTS.fetch_add(1, Ordering::Relaxed);
        if count < 12 || count % 25 == 0 {
            crate::trace::trace(&format!("second view: laid out, full {:?} page view {:?} view {:?}", *FULL.lock().unwrap_or_else(|error| error.into_inner()), bounds_of(split.main), bounds_of(split.view)));
        }
    }
    moved
}

/// The window's active page, or zero.
unsafe fn active_page(window: HWND) -> usize {
    let Some(base) = base() else { return 0 };
    crate::features::tabs::tab_model(base, window)
        .map_or(0, |model| crate::features::tabs::contents_at(base, model, active_tab(window)) as usize)
}

/// Keeps both pages where they are when a click in the second view made its page's tab active.
/// The browser then moved that page into the window's page view. Rivet puts the page the window
/// showed back there and the clicked page back in the second view. The tab stays active, so the
/// address bar and the tab list follow the clicked page. Only a split of the window's own tab
/// needs this, since a page of another window activates its tab in that window. Returns whether
/// the pages moved.
pub(crate) unsafe fn hold(window: HWND) -> bool {
    let Some(state) = split().filter(|split| split.window == window as isize) else { return false };
    let active = active_page(window);
    let decision = keep(active, state.active, state.page, state.shown);
    let mut next = state;
    if active != 0 {
        next.active = active;
    }
    let moved = match decision {
        Keep::Nothing => false,
        Keep::Follow { main } => {
            next.shown = main;
            false
        }
        Keep::Restore { main } => {
            // The second view lets go first. Its page now sits in the page view, so the browser's
            // detach leaves it there. The page view then takes its page back and lets go of the
            // clicked one, which the second view shows again.
            set_page(state.view, 0);
            set_page(state.main, main);
            set_page(state.view, state.page);
            true
        }
    };
    {
        let mut slot = SPLIT.lock().unwrap_or_else(|error| error.into_inner());
        if slot.is_some_and(|split| split.window == window as isize) {
            *slot = Some(next);
        }
    }
    if moved {
        focus(window);
        crate::trace::trace(&format!("second view: kept page {:#x} beside {:#x} after its tab became active", state.page, next.shown));
    } else if decision != Keep::Nothing {
        crate::trace::trace(&format!("second view: the window's page view shows {:#x}", next.shown));
    }
    moved
}

/// Whether a bounds change the hook sees needs a layout: not while Rivet moves views, not when the
/// bounds did not change, and not when they are the bounds Rivet set last.
pub(crate) fn needs_layout(placing: bool, previous: [i32; 4], now: [i32; 4], last: [i32; 4]) -> bool {
    !placing && previous != now && now != last
}

/// Whether a browser layout that moved the page view is answered inside that layout.
pub(crate) fn in_layout_placement() -> bool {
    true
}

/// The page view's `OnBoundsChanged`: the browser's first, then the split's own bounds.
unsafe extern "system" fn bounds_changed(this: usize, previous: *const [i32; 4]) {
    let original: unsafe extern "system" fn(usize, *const [i32; 4]) = std::mem::transmute(ORIGINAL_BOUNDS_CHANGED.load(Ordering::Acquire));
    original(this, previous);
    let Some(split) = split().filter(|split| split.main == this) else { return };
    let last = *LAST.lock().unwrap_or_else(|error| error.into_inner());
    let previous = if previous.is_null() { [0; 4] } else { *previous };
    if !needs_layout(PLACING.load(Ordering::Acquire), previous, bounds_of(this), last) {
        return;
    }
    // The browser's layout just gave the page view the whole page area. Rivet narrows it again
    // inside the same layout, so the page is never shown at the full width in between. Moving it
    // later from a message made every browser layout resize the page twice; a renderer slower than
    // that churn never painted a frame at either size and the pane stayed white.
    if in_layout_placement() {
        place(&split);
    }
    // The appbar and the hold follow from the message, outside the browser's layout.
    if !PENDING.swap(true, Ordering::AcqRel) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
        let name: Vec<u16> = LAYOUT_MESSAGE.encode_utf16().chain(Some(0)).collect();
        PostMessageW(split.window as HWND, RegisterWindowMessageW(name.as_ptr()), 0, 0);
    }
}

/// Replaces the page view's vtable with a copy whose `OnBoundsChanged` is the hook. Only this
/// object changes; other page views keep the browser's vtable.
unsafe fn hook_main(main: usize) -> Option<usize> {
    let slot = recorded("bounds_slot")?;
    let vtable = *(main as *const usize);
    // The word before the vtable points to its type information, which the copy keeps.
    const SLOTS: usize = 0x200;
    let mut copy = vec![0usize; SLOTS + 1];
    for (index, entry) in copy.iter_mut().enumerate() {
        *entry = word(vtable - 8 + index * 8)?;
    }
    ORIGINAL_BOUNDS_CHANGED.store(copy[slot + 1], Ordering::Release);
    copy[slot + 1] = bounds_changed as usize;
    let copy: &'static mut [usize] = Box::leak(copy.into_boxed_slice());
    *(main as *mut usize) = copy.as_ptr() as usize + 8;
    Some(vtable)
}

/// The strip index of the window's active tab, or the first.
unsafe fn active_tab(window: HWND) -> i32 {
    crate::features::tabs::tabs(window).and_then(|tabs| tabs.iter().find(|tab| tab.active).map(|tab| tab.index as i32)).unwrap_or(0)
}

/// Opens the second view in `window` with `page`, a page whose tab is in `owner`, another window.
pub(crate) unsafe fn open(window: HWND, owner: HWND, page: usize) -> bool {
    open_with(window, owner, page, Mode::Split { leading: false })
}

/// Opens the second view with the page of the window's own tab `index`, which is not the active
/// one. The tab keeps its page; closing the view shows it in the tab again.
pub(crate) unsafe fn open_tab(window: HWND, index: usize, mode: Mode) -> bool {
    let Some(base) = base() else { return false };
    if index as i32 == active_tab(window) {
        crate::trace::trace("second view: the active tab is already shown");
        return false;
    }
    let Some(model) = crate::features::tabs::tab_model(base, window) else { return false };
    let page = crate::features::tabs::contents_at(base, model, index as i32) as usize;
    if page == 0 {
        return false;
    }
    let opened = open_with(window, window, page, mode);
    crate::trace::trace(&format!("second view: tab {index} {mode:?}: {opened}"));
    opened
}

/// Opens the second view in `window` with `page`, whose tab is in `owner`, laid out as `mode`.
pub(crate) unsafe fn open_with(window: HWND, owner: HWND, page: usize, mode: Mode) -> bool {
    if let Some(state) = split() {
        if state.window != window as isize || !upgrades(state.mode, mode) {
            crate::trace::trace("second view: already open");
            return false;
        }
        // The view that only kept the rail free takes the page.
        let owner_view = release_from_owner(owner, state.view, page, *(state.view as *const usize));
        set_page(state.view, page);
        let next = Split { mode, owner_view, page, ..state };
        *SPLIT.lock().unwrap_or_else(|error| error.into_inner()) = Some(next);
        place(&next);
        crate::frame::follow_second_view(window);
        crate::trace::trace(&format!("second view: open {mode:?}, view {:#x} from the rail inset, owner view {owner_view:#x}", state.view));
        return true;
    }
    let Some(base) = base() else { return false };
    let (Some(new), Some(ctor), Some(add)) = (recorded("new"), recorded("ctor"), recorded("add_child")) else {
        crate::trace::trace("second view: an address is missing for this build");
        return false;
    };
    let size = recorded("size").unwrap_or(0x528);
    // A view that only keeps the rail free has no page of its own and takes the active page's profile.
    let source = if page != 0 {
        page
    } else {
        crate::features::tabs::tab_model(base, window)
            .map_or(0, |model| crate::features::tabs::contents_at(base, model, active_tab(window)) as usize)
    };
    if source == 0 {
        return false;
    }
    let context = if let Some(rva) = recorded("ctx") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(base, rva)(source)
    } else if let Some(offset) = recorded("ctxoff") {
        word(source + offset).unwrap_or(0)
    } else {
        0
    };
    let Some(parent) = browser_view(window) else {
        crate::trace::trace("second view: the browser view was not found");
        return false;
    };
    if context == 0 {
        return false;
    }
    let view = crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(base, new)(size);
    crate::features::tabs::function::<unsafe extern "system" fn(usize, usize) -> usize>(base, ctor)(view, context);
    let vtable = *(view as *const usize);
    if let Some(rva) = recorded("find_slot") {
        let slots: Vec<usize> = (0..0x200usize).filter(|slot| word(vtable + slot * 8) == Some(base + rva)).collect();
        crate::trace::trace(&format!("second view: {rva:#x} is at vtable slots {slots:x?}"));
    }
    for slot in recorded("dump_slots").map(|slot| vec![slot, slot + 1, slot + 2]).unwrap_or_default() {
        let function = word(vtable + slot * 8).unwrap_or(0);
        let bytes: Vec<String> = (0..3).map(|i| format!("{:016x}", word(function + i * 8).unwrap_or(0).swap_bytes())).collect();
        crate::trace::trace(&format!("second view: slot {slot:#x} = {:#x}: {}", function.wrapping_sub(base), bytes.join("")));
    }
    // The window's page view is the other `ContentsWebView` of its browser view.
    // DevTools has page views of the same class, so the page view is the one holding the active page.
    let own = crate::features::tabs::tab_model(base, window)
        .map(|model| crate::features::tabs::contents_at(base, model, active_tab(window)) as usize)
        .unwrap_or(0);
    let Some(main) = views_with_vtable(parent, vtable).into_iter().find(|candidate| *candidate != view && own != 0 && holds(*candidate, own, size)) else {
        crate::trace::trace("second view: the window's page view was not found");
        return false;
    };
    // The owner window's page view lets go of the page first, as a tab move does. A tab of this
    // window that is not active is shown by no page view, so nothing lets go of it.
    let owner_view = if page == 0 { 0 } else { release_from_owner(owner, view, page, vtable) };
    let children = children_of(parent).len();
    crate::features::tabs::function::<unsafe extern "system" fn(usize, usize, usize) -> usize>(base, add)(parent, view, children);
    if page != 0 {
        set_page(view, page);
    }
    if let Some(behaviour) = recorded("focus_behavior") {
        // `FocusBehavior::ALWAYS`: a click or `RequestFocus` may give this view the keyboard.
        crate::features::tabs::function::<unsafe extern "system" fn(usize, i32)>(base, behaviour)(view, 1);
    }
    *FULL.lock().unwrap_or_else(|error| error.into_inner()) = bounds_of(main);
    *LAST.lock().unwrap_or_else(|error| error.into_inner()) = bounds_of(main);
    let main_vtable = hook_main(main).unwrap_or(0);
    let state = Split { mode, window: window as isize, view, parent, main, main_vtable, owner_view, page, shown: own, active: own };
    *SPLIT.lock().unwrap_or_else(|error| error.into_inner()) = Some(state);
    place(&state);
    // The browser may lay its page view out once more after this, for example after a tab switch.
    crate::frame::follow_second_view(window);
    crate::trace::trace(&format!(
        "second view: open {mode:?}, view {view:#x} main {main:#x} owner view {owner_view:#x} hooked {} full {:?}",
        main_vtable != 0,
        *FULL.lock().unwrap_or_else(|error| error.into_inner())
    ));
    true
}

/// Whether a second view open as `now` can take a page shown as `next` without a new view: only
/// the view that keeps the rail free, and only for a page.
pub(crate) fn upgrades(now: Mode, next: Mode) -> bool {
    now == Mode::Inset && next != Mode::Inset
}

/// What a close leaves: the rail inset while the rail is shown and a page was in the view, or
/// nothing.
pub(crate) fn after_close(mode: Mode, rail: i32) -> Option<Mode> {
    (rail > 0 && mode != Mode::Inset).then_some(Mode::Inset)
}

/// The owner window's page view lets go of the page first, as a tab move does. A tab of this
/// window that is not active is shown by no page view, so nothing lets go of it.
unsafe fn release_from_owner(owner: HWND, view: usize, page: usize, vtable: usize) -> usize {
    let size = recorded("size").unwrap_or(0x528);
    let owner_view = browser_view(owner)
        .map(|root| views_with_vtable(root, vtable))
        .unwrap_or_default()
        .into_iter()
        .find(|candidate| *candidate != view && holds(*candidate, page, size))
        .unwrap_or(0);
    if owner_view != 0 {
        set_page(owner_view, 0);
    }
    owner_view
}

/// Keeps `width` DIPs free at the right of the window's page area for the Sidebar's rail, or
/// none for zero. The page view narrows through the same layout as a split, so it works on every
/// build that records the second view.
pub(crate) unsafe fn set_rail(window: HWND, width: i32) -> bool {
    {
        let mut rails = RAILS.lock().unwrap_or_else(|error| error.into_inner());
        rails.retain(|(key, _)| *key != window as isize);
        if width > 0 {
            rails.push((window as isize, width));
        }
    }
    let state = split().filter(|split| split.window == window as isize);
    let done = match state {
        Some(state) if state.mode == Mode::Inset && !keeps_inset(window as isize) => close(window),
        Some(state) => {
            place(&state);
            crate::frame::follow_second_view(window);
            true
        }
        None if width > 0 => open_with(window, window, 0, Mode::Inset),
        None => true,
    };
    crate::trace::trace(&format!("second view: rail {width}: {done}"));
    done
}

/// Keeps `height` DIPs free at the top of the window's page area for Rivet's address bar, or none
/// for zero. This moves the page view itself, so it needs no record of the browser's layout object.
/// The view opens from the layout message, never from inside a paint or a browser layout.
pub(crate) unsafe fn set_top(window: HWND, height: i32) {
    {
        let mut tops = TOPS.lock().unwrap_or_else(|error| error.into_inner());
        tops.retain(|(key, _)| *key != window as isize);
        if height > 0 {
            tops.push((window as isize, height));
        }
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
    let name: Vec<u16> = LAYOUT_MESSAGE.encode_utf16().chain(Some(0)).collect();
    PostMessageW(window, RegisterWindowMessageW(name.as_ptr()), 0, 0);
}

/// The window's page area in window DIPs as left, top, right and bottom: the area the browser gave
/// its page view, also while a split narrows it.
pub(crate) unsafe fn page_area(window: HWND) -> Option<[i32; 4]> {
    if let Some(state) = split().filter(|split| split.window == window as isize) {
        let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
        let [ox, oy] = window_origin(state.main);
        let main = bounds_of(state.main);
        let full = below_bar(full, top_inset(&state, full));
        let (x, y) = (ox - main[0] + full[0], oy - main[1] + full[1]);
        return Some([x, y, x + full[2] - rail_of(window as isize), y + full[3]]);
    }
    // With no split, the page view is the view of the browser view that holds the active page.
    let base = base()?;
    let model = crate::features::tabs::tab_model(base, window)?;
    let own = crate::features::tabs::contents_at(base, model, active_tab(window)) as usize;
    let parent = browser_view(window)?;
    let end = base + crate::chromium::image::recorded().map_or(0, |image| image.size);
    let size = recorded("size").unwrap_or(0x528);
    let mut queue = vec![(parent, 0usize)];
    let mut best: Option<(i64, usize)> = None;
    let mut visited = 0;
    while let Some((view, depth)) = queue.pop() {
        visited += 1;
        if visited > 600 || depth > 8 {
            continue;
        }
        if own != 0 && word(view).is_some_and(|vptr| vptr > base && vptr < end) && holds(view, own, size) {
            let [_, _, w, h] = bounds_of(view);
            let area = w as i64 * h as i64;
            if w > 0 && h > 0 && best.map_or(true, |(most, _)| area > most) {
                best = Some((area, view));
            }
        }
        queue.extend(children_of(view).into_iter().map(|child| (child, depth + 1)));
    }
    let (_, main) = best?;
    let [x, y] = window_origin(main);
    let [_, _, w, h] = bounds_of(main);
    Some([x, y, x + w, y + h])
}

/// The window's page area in window DIPs with the rail's strip included, which is where the
/// Sidebar's rail is drawn.
pub(crate) unsafe fn full_page_area(window: HWND) -> Option<[i32; 4]> {
    let mut area = page_area(window)?;
    if split().is_some_and(|split| split.window == window as isize) {
        area[2] += rail_of(window as isize);
    }
    Some(area)
}

/// Whether the browser's own bounds changes of the window's page view reach Rivet. Without the
/// hook, Rivet keeps checking the page view's bounds while the view is open.
pub(crate) fn follows_bounds(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize && split.main_vtable != 0)
}

/// Whether the follow timer may stop after `idle` ticks that moved nothing.
pub(crate) fn may_stop_following(open: bool, hooked: bool, idle: u32) -> bool {
    !open || (hooked && idle >= 25)
}

/// Closes the second view: it lets go of its page, the page view gets its own vtable and its full
/// bounds back, the view is deleted, and the page returns to its owner window's page view. While
/// the rail is shown, the view stays empty and keeps the rail free.
pub(crate) unsafe fn close(window: HWND) -> bool {
    let Some(split) = split().filter(|split| split.window == window as isize) else { return false };
    if let Some(mode) = after_close(split.mode, if keeps_inset(window as isize) { 1 } else { 0 }) {
        set_page(split.view, 0);
        if split.owner_view != 0 {
            set_page(split.owner_view, split.page);
        }
        let active = active_page(window);
        let shown = match page_after_close(active, split.shown) {
            Some(page) => {
                set_page(split.main, page);
                page
            }
            None => split.shown,
        };
        let next = Split { mode, owner_view: 0, page: 0, shown, active, ..split };
        *SPLIT.lock().unwrap_or_else(|error| error.into_inner()) = Some(next);
        place(&next);
        crate::frame::follow_second_view(window);
        crate::trace::trace("second view: closed to the rail inset");
        return true;
    }
    let Some(base) = base() else { return false };
    *SPLIT.lock().unwrap_or_else(|error| error.into_inner()) = None;
    set_page(split.view, 0);
    // The window's page view may show a page that is not the active tab's. It shows the active
    // one again, so the window agrees with its tab strip.
    if let Some(page) = page_after_close(active_page(window), split.shown) {
        set_page(split.main, page);
    }
    if split.main_vtable != 0 {
        *(split.main as *mut usize) = split.main_vtable;
    }
    let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
    if full[2] > 0 {
        set_bounds(split.main, full);
    }
    *LAST.lock().unwrap_or_else(|error| error.into_inner()) = [0; 4];
    // `RemoveChildView` hands ownership back without deleting. Without it, a View's destructor
    // takes the view out of its parent itself.
    if let Some(remove) = recorded("remove_child") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize)>(base, remove)(split.parent, split.view);
    }
    match virtual_of(split.view, "dtor", "dtor_slot") {
        // The scalar deleting destructor; flag 1 frees the memory.
        Some(destroy) => {
            std::mem::transmute::<usize, unsafe extern "system" fn(usize, u32) -> usize>(destroy)(split.view, 1);
        }
        None => crate::trace::trace("second view: no destructor for this build; the view is kept"),
    }
    if split.owner_view != 0 {
        set_page(split.owner_view, split.page);
    }
    crate::trace::trace(&format!("second view: closed after {} layouts; page {:#x} back to {:#x}", LAYOUTS.swap(0, Ordering::Relaxed), split.page, split.owner_view));
    true
}

/// Drops the state of a window that is being destroyed, without calling into its views, which the
/// browser deletes with the window.
pub(crate) fn forget(window: HWND) {
    TOPS.lock().unwrap_or_else(|error| error.into_inner()).retain(|(key, _)| *key != window as isize);
    let mut split = SPLIT.lock().unwrap_or_else(|error| error.into_inner());
    if split.is_some_and(|split| split.window == window as isize) {
        *split = None;
    }
}

/// Gives the second view the keyboard through `View::RequestFocus`.
pub(crate) unsafe fn focus(window: HWND) -> bool {
    let Some(split) = split().filter(|split| split.window == window as isize) else { return false };
    let Some(function) = virtual_of(split.view, "request_focus", "focus_slot") else { return false };
    std::mem::transmute::<usize, unsafe extern "system" fn(usize)>(function)(split.view);
    true
}

/// Sets the first page's share of the width, in thousandths, and lays both pages out again.
pub(crate) unsafe fn set_share(window: HWND, share: u32) -> bool {
    SHARE.store(share.clamp(0, 1000), Ordering::Relaxed);
    let Some(split) = split().filter(|split| split.window == window as isize) else { return false };
    place(&split);
    crate::trace::trace(&format!("second view: share {share}, full {:?}, page view {:?}, view {:?}", *FULL.lock().unwrap_or_else(|error| error.into_inner()), bounds_of(split.main), bounds_of(split.view)));
    true
}

/// The window position of a view's origin in DIPs, summing the bounds of it and its ancestors.
unsafe fn window_origin(view: usize) -> [i32; 2] {
    let parent_offset = offsets().0;
    let (mut x, mut y, mut at) = (0, 0, view);
    for _ in 0..16 {
        if at == 0 {
            break;
        }
        let bounds = bounds_of(at);
        x += bounds[0];
        y += bounds[1];
        at = word(at + parent_offset).unwrap_or(0);
    }
    [x, y]
}

/// The second view and the divider in client pixels, as left, top, right, bottom.
pub(crate) unsafe fn client_rects(window: HWND) -> Option<([i32; 4], [i32; 4])> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    let split = split().filter(|split| split.window == window as isize)?;
    let scale = GetDpiForWindow(window).max(96) as f32 / 96.0;
    let [vx, vy] = window_origin(split.view);
    let view = bounds_of(split.view);
    let to_px = |value: i32| (value as f32 * scale).round() as i32;
    let second = [to_px(vx), to_px(vy), to_px(vx + view[2]), to_px(vy + view[3])];
    let divider = match split.mode {
        Mode::Split { leading: true } => [to_px(vx + view[2]), to_px(vy), to_px(vx + view[2] + DIVIDER), to_px(vy + view[3])],
        Mode::Split { leading: false } => [to_px(vx - DIVIDER), to_px(vy), to_px(vx), to_px(vy + view[3])],
        // A docked page has no divider to drag.
        Mode::Docked { .. } | Mode::Inset => [0, 0, 0, 0],
    };
    Some((second, divider))
}

/// The share a divider dragged to client x `x` gives the first page.
pub(crate) unsafe fn share_at(window: HWND, x: i32) -> Option<u32> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    let split = split().filter(|split| split.window == window as isize)?;
    let scale = GetDpiForWindow(window).max(96) as f32 / 96.0;
    let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
    let [ox, _] = window_origin(split.main);
    let main = bounds_of(split.main);
    // The page view's origin in the window, less its own x, is where the full area's x sits.
    let left = (ox - main[0] + full[0]) as f32 * scale;
    let room = ((full[2] - DIVIDER).max(1)) as f32 * scale;
    let share = ((x as f32 - left - DIVIDER as f32 * scale / 2.0) / room * 1000.0).round();
    Some(share.clamp(0.0, 1000.0) as u32)
}


/// Opens the second view with the first page of another window of this browser, the window that
/// owns that page's tab.
pub(crate) unsafe fn open_from_other_window(window: HWND) -> bool {
    let Some(base) = base() else { return false };
    for other in crate::frame::browser_windows_of_process() {
        if other == window {
            continue;
        }
        let Some(model) = crate::features::tabs::tab_model(base, other) else { continue };
        let page = crate::features::tabs::contents_at(base, model, 0) as usize;
        if page != 0 {
            return open(window, other, page);
        }
    }
    crate::trace::trace("second view: no other window has a page");
    false
}

/// Whether the divider is being dragged.
static DRAGGING: AtomicBool = AtomicBool::new(false);

/// What a mouse message means for the split, before the browser sees it. A press on the divider
/// starts a drag that moves it and is not passed on. A press in the second view goes to the browser
/// first, then the view asks for the keyboard. Returns the result when the message is handled here.
pub(crate) unsafe fn on_mouse(window: HWND, message: u32, lparam: isize, browser: &dyn Fn() -> isize) -> Option<isize> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE};
    if !is_open(window) {
        return None;
    }
    let x = (lparam & 0xffff) as i16 as i32;
    let y = ((lparam >> 16) & 0xffff) as i16 as i32;
    let inside = |rect: [i32; 4]| x >= rect[0] && x < rect[2] && y >= rect[1] && y < rect[3];
    match message {
        WM_LBUTTONDOWN => {
            let (second, divider) = client_rects(window)?;
            if inside(divider) {
                DRAGGING.store(true, Ordering::Release);
                SetCapture(window);
                return Some(0);
            }
            if !inside(second) {
                // A press in the window's own page gives it the keyboard back. The rows follow.
                if is_split(window) && SECOND_FOCUSED.swap(false, Ordering::AcqRel) {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
                    let name: Vec<u16> = LAYOUT_MESSAGE.encode_utf16().chain(Some(0)).collect();
                    PostMessageW(window, RegisterWindowMessageW(name.as_ptr()), 0, 0);
                }
                return None;
            }
            if inside(second) {
                SECOND_FOCUSED.store(true, Ordering::Release);
                let result = browser();
                // The click may have made the page's tab active, which moves the page into the
                // window's page view. Rivet puts it back now, and again after the browser's own
                // messages, since the activation may come later.
                if !hold(window) {
                    focus(window);
                }
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
                let name: Vec<u16> = LAYOUT_MESSAGE.encode_utf16().chain(Some(0)).collect();
                PostMessageW(window, RegisterWindowMessageW(name.as_ptr()), 0, 0);
                return Some(result);
            }
            None
        }
        WM_MOUSEMOVE if DRAGGING.load(Ordering::Acquire) => {
            if let Some(share) = share_at(window, x) {
                set_share(window, share);
            }
            Some(0)
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED if DRAGGING.swap(false, Ordering::AcqRel) => {
            if message == WM_LBUTTONUP {
                ReleaseCapture();
            }
            Some(0)
        }
        _ => None,
    }
}


/// Bare `ContentsWebView`s for other features of the window, such as the Mini Window. Each is a
/// view of the browser view like the second view, with no layout of its own: its owner places it.
pub(crate) mod bare {
    use super::*;

    /// A new page view in `window` showing `page`, a tab of that window that no page view shows.
    /// Returns the view and its parent. It is added last, so it is drawn over the window's pages.
    pub(crate) unsafe fn new_view(window: HWND, page: usize) -> Option<(usize, usize)> {
        let base = base()?;
        let (new, ctor, add) = (recorded("new")?, recorded("ctor")?, recorded("add_child")?);
        if page == 0 {
            return None;
        }
        let context = if let Some(rva) = recorded("ctx") {
            crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(base, rva)(page)
        } else if let Some(offset) = recorded("ctxoff") {
            word(page + offset).unwrap_or(0)
        } else {
            0
        };
        let parent = browser_view(window)?;
        if context == 0 {
            return None;
        }
        let view = crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(base, new)(recorded("size").unwrap_or(0x528));
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize) -> usize>(base, ctor)(view, context);
        let children = children_of(parent).len();
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize, usize) -> usize>(base, add)(parent, view, children);
        set_page(view, page);
        if let Some(behaviour) = recorded("focus_behavior") {
            crate::features::tabs::function::<unsafe extern "system" fn(usize, i32)>(base, behaviour)(view, 1);
        }
        Some((view, parent))
    }

    /// Places a view at `rect` (x, y, width, height) in window DIPs.
    pub(crate) unsafe fn place(view: usize, parent: usize, rect: [i32; 4]) {
        let [ox, oy] = window_origin(parent);
        set_bounds(view, [rect[0] - ox, rect[1] - oy, rect[2].max(1), rect[3].max(1)]);
    }

    /// Lets go of the view's page, takes the view out of its parent and deletes it. The page stays
    /// with its tab.
    pub(crate) unsafe fn drop_view(view: usize, parent: usize) {
        let Some(base) = base() else { return };
        set_page(view, 0);
        if let Some(remove) = recorded("remove_child") {
            crate::features::tabs::function::<unsafe extern "system" fn(usize, usize)>(base, remove)(parent, view);
        }
        match virtual_of(view, "dtor", "dtor_slot") {
            Some(destroy) => {
                std::mem::transmute::<usize, unsafe extern "system" fn(usize, u32) -> usize>(destroy)(view, 1);
            }
            None => crate::trace::trace("bare view: no destructor for this build; the view is kept"),
        }
    }

    /// Gives the view the keyboard.
    pub(crate) unsafe fn focus(view: usize) -> bool {
        let Some(function) = virtual_of(view, "request_focus", "focus_slot") else { return false };
        std::mem::transmute::<usize, unsafe extern "system" fn(usize)>(function)(view);
        true
    }

    /// The browser moved `page` into the window's page view because its tab became active. The page
    /// view takes `behind` back and `view` shows `page` again. Returns whether the pages moved.
    pub(crate) unsafe fn take_back(window: HWND, view: usize, page: usize, behind: usize) -> bool {
        let Some(parent) = browser_view(window) else { return false };
        let size = recorded("size").unwrap_or(0x528);
        let Some(main) = views_with_vtable(parent, *(view as *const usize))
            .into_iter()
            .find(|candidate| *candidate != view && holds(*candidate, page, size))
        else {
            return false;
        };
        set_page(view, 0);
        set_page(main, behind);
        set_page(view, page);
        true
    }

    /// The window's active page, or zero.
    pub(crate) unsafe fn active(window: HWND) -> usize {
        active_page(window)
    }

    /// The page of the window's tab `index`, or zero.
    pub(crate) unsafe fn page_of_tab(window: HWND, index: usize) -> usize {
        let Some(base) = base() else { return 0 };
        crate::features::tabs::tab_model(base, window)
            .map_or(0, |model| crate::features::tabs::contents_at(base, model, index as i32) as usize)
    }
}
#[cfg(test)]
mod tests {
    use super::{after_close, below_bar, divide, keep, layout, may_stop_following, needs_layout, page_after_close, upgrades, Keep, Mode, DIVIDER, MIN_PANE, SIDEBAR_WIDTH};

    #[test]
    fn fr9_a_browser_layout_of_the_page_view_is_answered_inside_that_layout() {
        // Answering later from a message resized the page twice per layout and left it white.
        assert!(super::in_layout_placement());
    }

    #[test]
    fn fr12_the_page_view_starts_below_the_bar_row_in_every_mode() {
        let full = below_bar([0, 0, 1000, 700], 40);
        assert_eq!(full, [0, 40, 1000, 660]);
        let (page, view) = layout(full, Mode::Inset, 500, 0);
        assert_eq!(page, [0, 40, 1000, 660], "the page begins where the bar ends");
        assert_eq!(view[2], 0);
        let (first, second) = layout(full, Mode::Split { leading: false }, 500, 0);
        assert_eq!((first[1], second[1]), (40, 40));
        let (page, _) = layout(full, Mode::Inset, 500, 40);
        assert_eq!(page, [0, 40, 960, 660], "the rail and the bar row both stay free");
        // No row leaves the area as the browser gave it, and a tiny area keeps a page.
        assert_eq!(below_bar([0, 0, 1000, 700], 0), [0, 0, 1000, 700]);
        assert_eq!(below_bar([0, 0, 100, 20], 40)[3], 1);
    }

    #[test]
    fn fr10_rail_inset_keeps_the_page_narrowed_without_a_bounds_hook() {
        // The page view narrows by the rail and the empty view takes no width.
        let (page, view) = layout([0, 50, 1000, 600], Mode::Inset, 50, 40);
        assert_eq!(page, [0, 50, 960, 600]);
        assert_eq!(view[2], 0);
        // Without the bounds hook the follow timer runs as long as the view is open.
        assert!(!may_stop_following(true, false, 1000));
        assert!(may_stop_following(true, true, 25));
        assert!(!may_stop_following(true, true, 24));
        assert!(may_stop_following(false, false, 0));
    }

    const MAIN: usize = 0x1000;
    const SPLIT: usize = 0x2000;
    const OTHER: usize = 0x3000;

    #[test]
    fn fr13_clicking_the_second_view_page_keeps_both_pages_where_they_are() {
        // The split page's tab became active, so the window's page view gets its page back.
        assert_eq!(keep(SPLIT, MAIN, SPLIT, MAIN), Keep::Restore { main: MAIN });
    }

    #[test]
    fn fr13_a_kept_page_is_not_moved_again_while_its_tab_stays_active() {
        assert_eq!(keep(SPLIT, SPLIT, SPLIT, MAIN), Keep::Nothing);
    }

    #[test]
    fn fr13_clicking_the_window_page_again_leaves_both_pages_in_place() {
        assert_eq!(keep(MAIN, SPLIT, SPLIT, MAIN), Keep::Follow { main: MAIN });
    }

    #[test]
    fn fr13_choosing_another_tab_shows_it_beside_the_split_page() {
        assert_eq!(keep(OTHER, MAIN, SPLIT, MAIN), Keep::Follow { main: OTHER });
        // A click on the split page after that keeps the chosen page in the window's page view.
        assert_eq!(keep(SPLIT, OTHER, SPLIT, OTHER), Keep::Restore { main: OTHER });
    }

    #[test]
    fn fr13_an_unknown_page_changes_nothing() {
        assert_eq!(keep(0, MAIN, SPLIT, MAIN), Keep::Nothing);
        assert_eq!(keep(SPLIT, MAIN, SPLIT, 0), Keep::Nothing);
    }

    #[test]
    fn fr13_closing_the_split_shows_the_active_tab_page_in_the_window() {
        assert_eq!(page_after_close(SPLIT, MAIN), Some(SPLIT));
        assert_eq!(page_after_close(MAIN, MAIN), None);
        assert_eq!(page_after_close(0, MAIN), None);
    }

    #[test]
    fn fr9_a_tab_dropped_on_the_left_shows_its_page_left_of_the_window_page() {
        let (main, view) = layout([0, 0, 1008, 700], Mode::Split { leading: true }, 500, 0);
        assert_eq!(view, [0, 0, 500, 700]);
        assert_eq!(main, [500 + DIVIDER, 0, 500, 700]);
    }

    #[test]
    fn fr10_a_sidebar_page_floats_over_the_right_of_the_page_which_keeps_its_width() {
        let full = [10, 50, 1000, 700];
        let (main, view) = layout(full, Mode::Docked { width: SIDEBAR_WIDTH }, 500, 0);
        assert_eq!(main, full);
        assert_eq!(view, [10 + 1000 - SIDEBAR_WIDTH, 50, SIDEBAR_WIDTH, 700]);
        let (_, narrow) = layout([0, 0, 200, 700], Mode::Docked { width: SIDEBAR_WIDTH }, 500, 0);
        assert_eq!(narrow, [0, 0, 200, 700]);
    }

    #[test]
    fn fr13_a_bounds_change_asks_for_a_layout_only_when_the_browser_moved_the_page_view() {
        let full = [0, 0, 1000, 600];
        let narrowed = [0, 0, 596, 600];
        // The browser's layout gave the page view a new area.
        assert!(needs_layout(false, narrowed, full, narrowed));
        // Rivet's own bounds, the same bounds again, or a change while Rivet moves views do not.
        assert!(!needs_layout(false, full, narrowed, narrowed));
        assert!(!needs_layout(false, full, full, narrowed));
        assert!(!needs_layout(true, narrowed, full, narrowed));
    }

    #[test]
    fn fr9_split_pages_share_the_page_area_with_a_divider_between_them() {
        let (first, second) = divide([10, 50, 1008, 700], 500);
        assert_eq!(first, [10, 50, 500, 700]);
        assert_eq!(second, [10 + 500 + DIVIDER, 50, 500, 700]);
        assert_eq!(first[2] + DIVIDER + second[2], 1008);
    }

    #[test]
    fn fr9_a_dragged_divider_leaves_each_page_its_narrowest_width() {
        let (first, second) = divide([0, 0, 1000, 600], 0);
        assert_eq!(first[2], MIN_PANE);
        assert_eq!(second[2], 1000 - DIVIDER - MIN_PANE);
        let (first, second) = divide([0, 0, 1000, 600], 1000);
        assert_eq!(second[2], MIN_PANE);
        assert_eq!(first[2] + DIVIDER + second[2], 1000);
    }

    #[test]
    fn fr10_the_sidebar_rail_narrows_the_page_and_a_docked_page_ends_at_the_rail() {
        let full = [0, 0, 1000, 700];
        let (main, view) = layout(full, Mode::Inset, 500, 40);
        assert_eq!(main, [0, 0, 960, 700]);
        assert_eq!(view[2], 0, "the rail inset shows no page");
        let (main, view) = layout(full, Mode::Docked { width: SIDEBAR_WIDTH }, 500, 40);
        assert_eq!(main, [0, 0, 960, 700]);
        assert_eq!(view, [960 - SIDEBAR_WIDTH, 0, SIDEBAR_WIDTH, 700]);
    }

    #[test]
    fn fr10_an_undocked_page_leaves_the_rail_inset_while_the_rail_is_shown() {
        assert_eq!(after_close(Mode::Docked { width: SIDEBAR_WIDTH }, 40), Some(Mode::Inset));
        assert_eq!(after_close(Mode::Docked { width: SIDEBAR_WIDTH }, 0), None);
        assert_eq!(after_close(Mode::Inset, 40), None, "closing the inset itself removes the view");
    }

    #[test]
    fn fr9_a_split_or_dock_takes_the_rail_inset_view_but_not_an_open_page() {
        assert!(upgrades(Mode::Inset, Mode::Split { leading: false }));
        assert!(upgrades(Mode::Inset, Mode::Docked { width: SIDEBAR_WIDTH }));
        assert!(!upgrades(Mode::Split { leading: false }, Mode::Docked { width: SIDEBAR_WIDTH }));
        assert!(!upgrades(Mode::Inset, Mode::Inset));
    }

    #[test]
    fn fr12_the_page_starts_at_the_bars_bottom_where_the_page_area_starts_lower() {
        use super::bar_inset;
        // The page area starts at the panel's top: the whole row is taken from it.
        assert_eq!(bar_inset(44, Some(8), 8), 44);
        // A browser strip of 33 DIPs puts the page area at 41: only 11 DIPs are taken, so the
        // page starts at 52, the bar's bottom, and no band shows between them.
        assert_eq!(41 + bar_inset(44, Some(8), 41), 8 + 44);
        // A page area below the bar keeps its place, and no bar takes nothing.
        assert_eq!(bar_inset(44, Some(8), 80), 0);
        assert_eq!(bar_inset(0, Some(8), 8), 0);
        assert_eq!(bar_inset(44, None, 41), 44);
    }

    #[test]
    fn fr9_a_split_row_stays_at_the_panel_top_above_a_browser_infobar() {
        use crate::features::split::ROW_HEIGHT;
        // The left page sits 57 DIP lower under a browser infobar; both rows keep the panel's top.
        let panes = super::pane_pair([276, 109, 600, 643], [884, 52, 600, 700], true, [Some(0), Some(1)], 1484, Some(8));
        assert_eq!((panes[0].row_y, panes[1].row_y), (8, 8));
        assert!(panes[0].row_y + ROW_HEIGHT <= panes[0].y);
    }

    #[test]
    fn fr9_each_split_page_has_its_own_row_right_above_it_with_equal_tops() {
        use crate::features::split::{CAPTION_ROOM, ROW_HEIGHT};
        // The window's page on the left, the second view on the right, focused.
        let panes = super::pane_pair([276, 52, 600, 700], [884, 52, 600, 700], true, [Some(0), Some(1)], 1484, None);
        assert_eq!(panes.len(), 2);
        let (left, right) = (panes[0], panes[1]);
        assert_eq!(left.y, right.y, "the pages start at the same top");
        assert_eq!((left.row_y, right.row_y), (52 - ROW_HEIGHT, 52 - ROW_HEIGHT));
        assert!(left.first && !left.last && right.last && !right.first);
        assert!(!left.active && right.active, "only the focused page is highlighted");
        assert_eq!(left.caption, 0.0);
        assert_eq!(right.caption, CAPTION_ROOM, "the top right page carries the window buttons");
        assert_eq!((left.tab, right.tab), (Some(0), Some(1)));
        // A leading second view comes first.
        let panes = super::pane_pair([884, 52, 600, 700], [276, 52, 600, 700], false, [Some(2), None], 1484, Some(8));
        assert_eq!((panes[0].tab, panes[1].tab), (None, Some(2)));
        assert!(panes[1].active && panes[1].caption > 0.0 && panes[0].first);
    }

    #[test]
    fn fr13_a_split_beside_the_rail_divides_only_the_area_left_of_it() {
        let (first, second) = layout([0, 0, 1048, 600], Mode::Split { leading: false }, 500, 40);
        assert_eq!(first[2] + DIVIDER + second[2], 1008);
        assert_eq!(second[0] + second[2], 1008);
    }
}
