//! Rivet's extra page views: Chromium `ContentsWebView`s of Rivet's own, added to the browser view
//! beside the window's page, which show pages of other tabs. Only Chromium's own views functions
//! are called. The addresses come from the per-build record that `app_window::recorded` reads.
//!
//! The views show the pages of a split beside the window's page, up to four pages in a grid, or a
//! sidebar page docked over the right part of it. The grid itself is `split_grid`.
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
use crate::features::split::Side;
use crate::features::split_grid::{Grid, Metrics, MAX_PANES};

/// The gap between two pages, which is the divider Rivet draws and drags, in DIPs.
pub(crate) const DIVIDER: i32 = 8;
/// The narrowest a page of the split may get, in DIPs.
pub(crate) const MIN_PANE: i32 = 120;

/// The width of a docked sidebar page, in DIPs.
pub(crate) const SIDEBAR_WIDTH: i32 = 360;

/// The window message that lays the views out again, posted by the hook and by size changes.
pub(crate) const LAYOUT_MESSAGE: &str = "Rivet.SecondViewLayout";

/// The sizes the grid keeps between and above its pages.
const METRICS: Metrics = Metrics {
    divider: DIVIDER,
    row: crate::features::split::ROW_HEIGHT,
    min: MIN_PANE,
};

/// How the extra views sit beside the window's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The pages of the window's grid, with dividers between them.
    Split,
    /// One page docked over the right edge of the page, which keeps its full width.
    Docked { width: i32 },
    /// No page in the view. The window's page view only leaves the Sidebar's rail free.
    Inset,
    /// Floating over the page, which keeps its full size, below a header Rivet draws. This is the
    /// link preview.
    Floating,
}

/// The height of the header Rivet draws above a floating page, in DIPs.
pub(crate) const PREVIEW_HEADER: i32 = 40;

/// Where a floating page sits in the page area `full`, below its header. The study's preview keeps
/// 64 DIPs at the sides, 20 above its header and 40 below. A small window keeps a narrower margin.
pub(crate) fn floating(full: [i32; 4], reserve: i32) -> [i32; 4] {
    let [x, y, w, h] = full;
    let side = 64.min(w / 10);
    let top = 20.min(h / 10);
    // `reserve` DIPs at the page bottom belong to a notice that sits there, such as the held build
    // warning, so the floating page keeps clear of them.
    let bottom = 40.min(h / 10).max(reserve);
    [
        x + side,
        y + top + PREVIEW_HEADER,
        (w - 2 * side).max(1),
        (h - top - PREVIEW_HEADER - bottom).max(1),
    ]
}

/// The width the Sidebar's rail keeps free at the right of the page area, by window.
static RAILS: Mutex<Vec<(isize, i32)>> = Mutex::new(Vec::new());

fn rail_of(window: isize) -> i32 {
    RAILS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window)
        .map_or(0, |(_, width)| *width)
}

/// The height Rivet's address bar keeps free at the top of the page area, by window.
static TOPS: Mutex<Vec<(isize, i32)>> = Mutex::new(Vec::new());

fn top_of(window: isize) -> i32 {
    TOPS.lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window)
        .map_or(0, |(_, height)| *height)
}

/// The grid of each window's split.
static GRIDS: Mutex<Vec<(isize, Grid)>> = Mutex::new(Vec::new());

fn grid_of(window: isize) -> Grid {
    GRIDS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window)
        .map_or_else(Grid::single, |(_, grid)| grid.clone())
}

fn set_grid(window: isize, grid: Option<Grid>) {
    let mut grids = GRIDS.lock().unwrap_or_else(|error| error.into_inner());
    grids.retain(|(key, _)| *key != window);
    if let Some(grid) = grid {
        grids.push((window, grid));
    }
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
    let panel =
        crate::features::frame_layout::panel_bounds(split.window as HWND).map(|panel| panel.y);
    bar_inset(height, panel, area_top)
}

/// The slot whose page has the keyboard, for a page whose tab is in another window: a click into
/// it never changes this window's active tab, so the click itself is remembered. Zero is the
/// window's own page view.
static FOCUSED: AtomicUsize = AtomicUsize::new(0);

/// The pages of a split as panes with their own rows, in reading order: top to bottom, then left
/// to right. Each page is (x, y, width, height in window DIPs, whether it has the keyboard, the
/// tab it shows). A page on the top line keeps its row at the panel's top, so a browser bar above
/// it, such as an infobar, stays between the row and the page. A lower page's row sits right above
/// it. The top left page leaves room for the appbar toggle and the top right page carries the
/// caption buttons.
pub(crate) fn grid_panes(
    pages: &[([i32; 4], bool, Option<usize>)],
    window_right: i32,
    panel_top: Option<i32>,
) -> Vec<crate::features::split::Pane> {
    use crate::features::split::{caption_over, Pane, ROW_HEIGHT};
    let Some(top) = pages.iter().map(|page| page.0[1]).min() else {
        return Vec::new();
    };
    // A lower page sits at least a page, a divider and a row below the top line; a browser bar
    // above a page moves it far less.
    let on_top = |rect: &[i32; 4]| rect[1] - top < MIN_PANE;
    let mut pages = pages.to_vec();
    pages.sort_by_key(|(rect, _, _)| (!on_top(rect), rect[1], rect[0]));
    let left = pages
        .iter()
        .filter(|page| on_top(&page.0))
        .map(|page| page.0[0])
        .min()
        .unwrap_or(0);
    let right = pages
        .iter()
        .filter(|page| on_top(&page.0))
        .map(|page| page.0[0] + page.0[2])
        .max()
        .unwrap_or(0);
    pages
        .iter()
        .map(|(rect, active, tab)| {
            let top_line = on_top(rect);
            Pane {
                x: rect[0],
                y: rect[1],
                width: rect[2],
                height: rect[3],
                row_y: if top_line {
                    panel_top.map_or(rect[1] - ROW_HEIGHT, |panel| {
                        panel.min(rect[1] - ROW_HEIGHT)
                    })
                } else {
                    rect[1] - ROW_HEIGHT
                },
                active: *active,
                first: top_line && rect[0] == left,
                last: top_line && rect[0] + rect[2] == right,
                caption: if top_line {
                    caption_over(window_right, rect[0] + rect[2])
                } else {
                    0.0
                },
                tab: *tab,
            }
        })
        .collect()
}

/// The strip index of the window's tab that holds `page`.
unsafe fn tab_of(window: HWND, page: usize) -> Option<usize> {
    let base = base()?;
    let model = crate::features::tabs::tab_model(base, window)?;
    let count = crate::features::tabs::tabs(window)?.len();
    (page != 0).then_some(())?;
    (0..count).find(|index| {
        crate::features::tabs::contents_at(base, model, *index as i32) as usize == page
    })
}

/// A view's place in the window as x, y, width and height in DIPs.
unsafe fn window_rect(view: usize) -> [i32; 4] {
    let [x, y] = window_origin(view);
    let bounds = bounds_of(view);
    [x, y, bounds[2], bounds[3]]
}

/// The place of a view's page in the window in DIPs. A view that keeps its page inside a border
/// was grown by it, and its row and hit areas follow the page.
unsafe fn page_rect(view: usize) -> [i32; 4] {
    let bounds = bounds_of(view);
    let children: Vec<[i32; 4]> = children_of(view)
        .into_iter()
        .map(|child| bounds_of(child))
        .collect();
    crate::features::page_fit::shrink(
        window_rect(view),
        crate::features::page_fit::holder_inset([bounds[2], bounds[3]], &children),
    )
}

/// The slot that has the keyboard: an extra page of this window's tabs has it while its tab is
/// active, a page of another window while it was clicked last, and the window's own page otherwise.
unsafe fn active_slot(window: HWND, state: &Split, grid: &Grid) -> usize {
    let active = active_page(window);
    let focused = FOCUSED.load(Ordering::Acquire);
    for slot in grid.leaves().into_iter().filter(|slot| *slot > 0) {
        let extra = state.extras[slot - 1];
        let on = match tab_of(window, extra.page) {
            Some(_) => active != 0 && active == extra.page,
            None => focused == slot,
        };
        if on {
            return slot;
        }
    }
    0
}

/// The pages of the window's split, each with its own row, or none.
pub(crate) unsafe fn panes(window: HWND) -> Vec<crate::features::split::Pane> {
    let Some(state) =
        split().filter(|split| split.window == window as isize && split.mode == Mode::Split)
    else {
        return Vec::new();
    };
    let grid = grid_of(window as isize);
    let focused = active_slot(window, &state, &grid);
    let mut pages = Vec::new();
    for slot in grid.leaves() {
        let rect = page_rect(view_of(&state, slot));
        if rect[2] <= 0 || rect[3] <= 0 {
            return Vec::new();
        }
        let page = if slot == 0 {
            state.shown
        } else {
            state.extras[slot - 1].page
        };
        pages.push((rect, slot == focused, tab_of(window, page)));
    }
    grid_panes(
        &pages,
        crate::features::split::client_right(window),
        crate::features::frame_layout::panel_bounds(window).map(|panel| panel.y),
    )
}

/// The slot of the window's split whose page `pane` is.
pub(crate) unsafe fn slot_of_pane(
    window: HWND,
    pane: &crate::features::split::Pane,
) -> Option<usize> {
    let state = split().filter(|split| split.window == window as isize)?;
    grid_of(window as isize).leaves().into_iter().find(|slot| {
        let rect = page_rect(view_of(&state, *slot));
        rect[0] == pane.x && rect[1] == pane.y
    })
}

/// The slot of the focused page, which is where a tab dropped without a page under it goes.
pub(crate) unsafe fn focused_slot(window: HWND) -> usize {
    match split().filter(|split| split.window == window as isize && split.mode == Mode::Split) {
        Some(state) => active_slot(window, &state, &grid_of(window as isize)),
        None => 0,
    }
}

/// Whether the window's split has room for another page.
pub(crate) fn has_room(window: HWND) -> bool {
    match split().filter(|split| split.window == window as isize) {
        Some(state) if state.mode == Mode::Split => {
            grid_of(window as isize).leaves().len() < MAX_PANES
        }
        Some(state) => state.mode == Mode::Inset,
        None => true,
    }
}

/// Remembers which page of the split has the keyboard, for a page of another window.
pub(crate) fn set_focused(slot: usize) {
    FOCUSED.store(slot, Ordering::Release);
}

/// Whether the empty view stays open to keep a strip of the page area free.
fn keeps_inset(window: isize) -> bool {
    rail_of(window) > 0 || top_of(window) > 0
}

/// One of Rivet's extra page views, or an unused slot when `view` is zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Extra {
    /// Rivet's `ContentsWebView`.
    view: usize,
    /// The page view of the window that owns the shown page, which let go of it, and the page.
    owner_view: usize,
    page: usize,
}

/// What the split of one window holds.
#[derive(Clone, Copy, Debug)]
struct Split {
    mode: Mode,
    window: isize,
    /// The browser view's `views::View`, the parent of Rivet's views.
    parent: usize,
    /// The window's own page view, whose bounds the hook narrows.
    main: usize,
    /// The vtable the page view had before the hook's copy replaced it.
    main_vtable: usize,
    /// The page view of another window that let go of the page the window's page view now shows,
    /// after a page of that window took over from the window's own closed page.
    main_owner: usize,
    /// Rivet's views for slots 1 to 3. A docked page and the rail inset use the first.
    extras: [Extra; MAX_PANES - 1],
    /// The page the window's page view shows, which is not always the active tab's page.
    shown: usize,
    /// The window's active page when Rivet looked last, so only a change of it is acted on.
    active: usize,
}

/// The view that shows `slot`'s page.
fn view_of(state: &Split, slot: usize) -> usize {
    if slot == 0 {
        state.main
    } else {
        state.extras[slot - 1].view
    }
}

/// What Rivet does when the window's active page may have changed while its split is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keep {
    /// Nothing changed.
    Nothing,
    /// The tab of the page in extra `slot` became active, so the browser moved that page into the
    /// window's page view. The page view gets `main` back and the extra view gets its page back.
    /// The tab stays active, so the address bar and the tab list follow the page that was clicked.
    Restore { main: usize, slot: usize },
    /// Another tab became active and the browser shows it in the window's page view, as it should.
    Follow { main: usize },
}

/// Decides what a change of the window's active page means for an open split of the same window.
/// `pages` are the pages of the extra slots 1, 2 and 3, zero where a slot is unused.
pub(crate) fn keep(active: usize, last_active: usize, pages: &[usize], shown: usize) -> Keep {
    if active == 0 || active == last_active {
        return Keep::Nothing;
    }
    if let Some(index) = pages.iter().position(|page| *page != 0 && *page == active) {
        return if shown != 0 && shown != active {
            Keep::Restore {
                main: shown,
                slot: index + 1,
            }
        } else {
            Keep::Nothing
        };
    }
    Keep::Follow { main: active }
}

/// The page the window's page view should show when the split closes: the active page, so the
/// window agrees with its tab strip again.
pub(crate) fn page_after_close(active: usize, shown: usize) -> Option<usize> {
    (active != 0 && active != shown).then_some(active)
}

static SPLIT: Mutex<Option<Split>> = Mutex::new(None);
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

fn store(state: Option<Split>) {
    *SPLIT.lock().unwrap_or_else(|error| error.into_inner()) = state;
}

/// Whether this build records what the extra views need, which makes them the split and sidebar
/// mechanism of the build.
pub(crate) fn available() -> bool {
    recorded("new").is_some() && recorded("ctor").is_some() && recorded("add_child").is_some()
}

/// Whether the window shows a docked sidebar page now.
pub(crate) fn is_docked(window: HWND) -> bool {
    split().is_some_and(|split| {
        split.window == window as isize && matches!(split.mode, Mode::Docked { .. })
    })
}

/// Whether the window shows a split now.
pub(crate) fn is_split(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize && split.mode == Mode::Split)
}

/// Whether the window shows a floating page now.
pub(crate) fn is_floating(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize && split.mode == Mode::Floating)
}

/// The tab of the window whose page floats over it, and that page.
pub(crate) unsafe fn floating_page(window: HWND) -> Option<(usize, usize)> {
    let split =
        split().filter(|split| split.window == window as isize && split.mode == Mode::Floating)?;
    let page = split.extras[0].page;
    Some((tab_of(window, page)?, page))
}

/// The floating page's view in client pixels, as left, top, right and bottom.
pub(crate) unsafe fn floating_client_rect(window: HWND) -> Option<[i32; 4]> {
    let split =
        split().filter(|split| split.window == window as isize && split.mode == Mode::Floating)?;
    let view = split.extras[0].view;
    if view == 0 {
        return None;
    }
    let scale = scale_of(window);
    let [x, y] = window_origin(view);
    let [_, _, width, height] = bounds_of(view);
    let to_px = |value: i32| (value as f32 * scale).round() as i32;
    Some([to_px(x), to_px(y), to_px(x + width), to_px(y + height)])
}

/// Whether the window shows an extra view now.
pub(crate) fn is_open(window: HWND) -> bool {
    split().is_some_and(|split| split.window == window as isize)
}

/// A word of memory that may not be mapped.
unsafe fn word(address: usize) -> Option<usize> {
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // A walk over the browser's views reads many thousands of words, so a word known readable is
    // read in place rather than with a system call each.
    if let Some(value) = crate::chromium::memory::read_word(address) {
        return Some(value);
    }
    let mut value = 0usize;
    let mut done = 0usize;
    let ok = ReadProcessMemory(
        GetCurrentProcess(),
        address as *const c_void,
        (&mut value as *mut usize).cast(),
        8,
        &mut done,
    );
    (ok != 0 && done == 8).then_some(value)
}

fn base() -> Option<usize> {
    crate::features::tabs::base()
}

/// Where a View keeps its parent, its children and its bounds in this build.
fn offsets() -> (usize, usize, usize) {
    let parent = recorded("vparent").unwrap_or(0x198);
    let children = recorded("vchildren").unwrap_or(0x1A0);
    (
        parent,
        children,
        recorded("vbounds").unwrap_or(parent + 0x30),
    )
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
    (0..count)
        .filter_map(|index| word(begin + index * 8))
        .collect()
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
        queue.extend(
            children_of(view)
                .into_iter()
                .map(|child| (child, depth + 1)),
        );
    }
    found
}

/// Whether a page view holds this page in one of its words, as `WebView::web_contents_`.
unsafe fn holds(view: usize, page: usize, size: usize) -> bool {
    if let Some(found) = crate::chromium::memory::holds_word(view, 8, size, page) {
        return found;
    }
    (8..size)
        .step_by(8)
        .any(|offset| word(view + offset) == Some(page))
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
        if (3..=64).contains(&children.len())
            && children
                .iter()
                .all(|child| word(*child).is_some_and(|vptr| vptr > base && vptr < end))
        {
            return Some(q);
        }
    }
    None
}

/// `ContentsWebView::SetWebContents`, a function of the class or a slot of its vtable, by build.
unsafe fn set_page(view: usize, page: usize) -> bool {
    let Some(base) = base() else { return false };
    if view == 0 {
        return false;
    }
    if let Some(rva) = recorded("set_contents") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize)>(base, rva)(
            view, page,
        );
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
        crate::features::tabs::function::<unsafe extern "system" fn(usize, *const [i32; 4])>(
            base, rva,
        )(view, &rect);
        return;
    }
    let previous = bounds_of(view);
    if previous == rect {
        return;
    }
    *((view + offsets().2) as *mut [i32; 4]) = rect;
    if let Some(slot) = recorded("bounds_slot") {
        let function = *((*(view as *const usize) + slot * 8) as *const usize);
        std::mem::transmute::<usize, unsafe extern "system" fn(usize, *const [i32; 4])>(function)(
            view, &previous,
        );
    }
}

/// The virtual function at `slot` of a view's vtable, or the recorded function `name`.
unsafe fn virtual_of(view: usize, name: &str, slot_name: &str) -> Option<usize> {
    let base = base()?;
    recorded(name).map(|rva| base + rva).or_else(|| {
        recorded(slot_name).map(|slot| *((*(view as *const usize) + slot * 8) as *const usize))
    })
}

/// The main page's columns, as x and width within a page area `width` DIPs wide, while the
/// window shows a split. The browser's bar above the page is narrowed to them.
pub(crate) fn split_columns(window: HWND, width: i32) -> Option<(i32, i32)> {
    split().filter(|split| split.window == window as isize && split.mode == Mode::Split)?;
    grid_bounds(
        &grid_of(window as isize),
        [0, 0, width, 1],
        rail_of(window as isize),
    )
    .into_iter()
    .find(|(slot, _)| *slot == 0)
    .map(|(_, rect)| (rect[0], rect[2]))
}

/// The first view below `root` that holds a page whose vtable is `page_vtable`, as the view and
/// the page. The Sidebar finds the extension page that the browser's side panel shows this way.
pub(crate) unsafe fn page_under(root: usize, page_vtable: usize) -> Option<(usize, usize)> {
    let size = recorded("size").unwrap_or(0x528);
    let mut queue = vec![(root, 0usize)];
    let mut visited = 0;
    while let Some((view, depth)) = queue.pop() {
        visited += 1;
        if visited > 400 || depth > 10 {
            continue;
        }
        if view != root {
            let page = (8..size)
                .step_by(8)
                .filter_map(|offset| word(view + offset))
                .find(|candidate| {
                    *candidate > 0x10000
                        && *candidate != view
                        && word(*candidate) == Some(page_vtable)
                });
            if let Some(page) = page {
                return Some((view, page));
            }
        }
        queue.extend(
            children_of(view)
                .into_iter()
                .map(|child| (child, depth + 1)),
        );
    }
    None
}

/// Gives a page view `page`, or no page for zero, as `WebView::SetWebContents` does.
pub(crate) unsafe fn hand_page(view: usize, page: usize) -> bool {
    set_page(view, page)
}

/// The window's page view bounds and the docked view's, both in the page view's parent, for the
/// full page area `full`. `rail` DIPs at the right of the area stay free for the Sidebar's rail.
/// A grid lays its pages out with `grid_bounds`.
pub(crate) fn layout(full: [i32; 4], mode: Mode, rail: i32) -> ([i32; 4], [i32; 4]) {
    let full = without_rail(full, rail);
    let [x, y, w, h] = full;
    match mode {
        Mode::Inset | Mode::Split => (full, [x + w, y, 0, h]),
        Mode::Floating => (full, floating(full, crate::frame::held_reserve())),
        Mode::Docked { width } => {
            let width = width.clamp(0, w.max(0));
            (full, [x + w - width, y, width, h])
        }
    }
}

/// The page area less the Sidebar's rail at its right.
pub(crate) fn without_rail(full: [i32; 4], rail: i32) -> [i32; 4] {
    [full[0], full[1], (full[2] - rail.max(0)).max(1), full[3]]
}

/// Each slot's bounds in the page view's parent for the grid of a split in the full page area.
pub(crate) fn grid_bounds(grid: &Grid, full: [i32; 4], rail: i32) -> Vec<(usize, [i32; 4])> {
    grid.cells(without_rail(full, rail), METRICS)
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

/// The page area `full` of the page view `main` over a small inset of its container, which
/// otherwise shows as a band around the page.
unsafe fn fitted(main: usize, full: [i32; 4]) -> [i32; 4] {
    let container = word(main + offsets().0)
        .filter(|parent| *parent != 0)
        .map(|parent| bounds_of(parent));
    container.map_or(full, |container| {
        crate::features::page_fit::fill(full, [container[2], container[3]])
    })
}

/// The page area the pages are laid out in, in the page view's parent, below the bar row.
unsafe fn area_of(split: &Split) -> Option<[i32; 4]> {
    // Bounds Rivet did not set came from the browser's layout and are the new page area.
    let now = bounds_of(split.main);
    let last = *LAST.lock().unwrap_or_else(|error| error.into_inner());
    if now != last && now[2] > 0 {
        crate::trace::trace_changed(
            "page view area",
            &format!("second view: the browser gave the page view {now:?}, Rivet set {last:?}"),
        );
        *FULL.lock().unwrap_or_else(|error| error.into_inner()) = now;
    }
    let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
    let full = fitted(split.main, full);
    (full[2] > 0).then(|| below_bar(full, top_inset(split, full)))
}

/// Puts every page where the mode and the grid say, from the page area the browser last gave the
/// page view.
unsafe fn place(split: &Split) -> bool {
    let Some(full) = area_of(split) else {
        return false;
    };
    let rail = rail_of(split.window);
    let window = split.window as HWND;
    let targets: Vec<(usize, [i32; 4])> = match split.mode {
        Mode::Split => grid_bounds(&grid_of(split.window), full, rail),
        _ => {
            let (first, second) = layout(full, split.mode, rail);
            vec![(0, first), (1, second)]
        }
    };
    let [dx, dy] = offset_in(split.main, split.parent);
    let mut moves = Vec::new();
    for (slot, rect) in targets {
        let view = view_of(split, slot);
        if view == 0 {
            continue;
        }
        // The extra views are children of the browser view, the page view sits deeper.
        let rect = if slot == 0 {
            rect
        } else {
            let moved = [rect[0] + dx, rect[1] + dy, rect[2], rect[3]];
            // The browser keeps its page inset inside each page view; a split's pages line up.
            let moved = if split.mode == Mode::Split {
                let raw = *FULL.lock().unwrap_or_else(|error| error.into_inner());
                crate::features::page_fit::outset(moved, raw, fitted(split.main, raw))
            } else {
                moved
            };
            // A page on the top line rises over the browser's bars, which belong to the main page.
            let moved = if rect[1] == full[1] {
                crate::features::pane_infobar::lift(
                    split.mode,
                    moved,
                    crate::features::pane_infobar::bar_above(window),
                )
            } else {
                moved
            };
            crate::features::sidebar_page::fit(window, split.mode, moved)
        };
        // A page view that keeps its page inside a border grows by it, so every pane's page starts
        // where its row ends.
        let now = bounds_of(view);
        let children: Vec<[i32; 4]> = children_of(view)
            .into_iter()
            .map(|child| bounds_of(child))
            .collect();
        let rect = crate::features::page_fit::grow(
            rect,
            crate::features::page_fit::holder_inset([now[2], now[3]], &children),
        );
        if slot == 0 {
            *LAST.lock().unwrap_or_else(|error| error.into_inner()) = rect;
        }
        crate::trace::trace_changed(
            if slot == 0 { "place 0" } else { "place n" },
            &format!(
                "second view: slot {slot} to {rect:?} from area {full:?}, raw {:?}, now {:?}, children {:?}, parent {:?}",
                *FULL.lock().unwrap_or_else(|error| error.into_inner()),
                bounds_of(view),
                children_of(view)
                    .into_iter()
                    .map(|child| bounds_of(child))
                    .collect::<Vec<_>>(),
                word(view + offsets().0).filter(|p| *p != 0).map(|p| bounds_of(p))
            ),
        );
        // Bounds that are already right are not set again, so a layout does not ask for another one.
        if bounds_of(view) != rect {
            moves.push((view, rect));
        }
    }
    if moves.is_empty() {
        return false;
    }
    PLACING.store(true, Ordering::Release);
    for (view, rect) in moves {
        set_bounds(view, rect);
    }
    PLACING.store(false, Ordering::Release);
    true
}

/// Asks the window to lay its views out again, outside the current message.
unsafe fn post_layout(window: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW};
    let name: Vec<u16> = LAYOUT_MESSAGE.encode_utf16().chain(Some(0)).collect();
    PostMessageW(window, RegisterWindowMessageW(name.as_ptr()), 0, 0);
}

/// Lays the views of `window` out again from the page area the browser gave its page view last.
/// Returns whether a view moved. The size messages of the window and the hook's message both end
/// here, outside any browser layout.
pub(crate) unsafe fn relayout(window: HWND) -> bool {
    PENDING.store(false, Ordering::Release);
    hold(window);
    // A bar row asked for before the view existed opens the empty view here, outside any browser
    // layout and outside the appbar's paint.
    if split().is_none() && top_of(window as isize) > 0 {
        let opened = open_with(window, window, 0, Mode::Inset);
        crate::trace::trace(&format!(
            "second view: bar row {}: {opened}",
            top_of(window as isize)
        ));
        return opened;
    }
    let Some(split) = split().filter(|split| split.window == window as isize) else {
        return false;
    };
    let moved = place(&split);
    if moved {
        let count = LAYOUTS.fetch_add(1, Ordering::Relaxed);
        if count < 12 || count % 25 == 0 {
            let views: Vec<[i32; 4]> = split
                .extras
                .iter()
                .filter(|extra| extra.view != 0)
                .map(|extra| bounds_of(extra.view))
                .collect();
            crate::trace::trace(&format!(
                "second view: laid out, full {:?} page view {:?} views {views:?}",
                *FULL.lock().unwrap_or_else(|error| error.into_inner()),
                bounds_of(split.main)
            ));
        }
    }
    moved
}

/// The window's active page, or zero.
unsafe fn active_page(window: HWND) -> usize {
    let Some(base) = base() else { return 0 };
    crate::features::tabs::tab_model(base, window).map_or(0, |model| {
        crate::features::tabs::contents_at(base, model, active_tab(window)) as usize
    })
}

/// Keeps every page where it is when a click in an extra view made its page's tab active. The
/// browser then moved that page into the window's page view. Rivet puts the page the window showed
/// back there and the clicked page back in its view. The tab stays active, so the address bar and
/// the tab list follow the clicked page. Only a page of the window's own tabs needs this, since a
/// page of another window activates its tab in that window. Returns whether the pages moved.
pub(crate) unsafe fn hold(window: HWND) -> bool {
    let Some(state) = split().filter(|split| split.window == window as isize) else {
        return false;
    };
    let active = active_page(window);
    let pages: Vec<usize> = state.extras.iter().map(|extra| extra.page).collect();
    let decision = keep(active, state.active, &pages, state.shown);
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
        Keep::Restore { main, slot } => {
            // The extra view lets go first. Its page now sits in the page view, so the browser's
            // detach leaves it there. The page view then takes its page back and lets go of the
            // clicked one, which the extra view shows again.
            let extra = state.extras[slot - 1];
            set_page(extra.view, 0);
            set_page(state.main, main);
            set_page(extra.view, extra.page);
            set_focused(slot);
            true
        }
    };
    {
        let mut stored = SPLIT.lock().unwrap_or_else(|error| error.into_inner());
        if stored.is_some_and(|split| split.window == window as isize) {
            *stored = Some(next);
        }
    }
    if moved {
        focus(window);
        crate::trace::trace(&format!(
            "second view: kept the clicked page in its view beside {:#x}",
            next.shown
        ));
    } else if decision != Keep::Nothing {
        crate::trace::trace(&format!(
            "second view: the window's page view shows {:#x}",
            next.shown
        ));
    }
    moved
}

/// Whether a bounds change the hook sees needs a layout: not while Rivet moves views, not when the
/// bounds did not change, and not when they are the bounds Rivet set last.
pub(crate) fn needs_layout(
    placing: bool,
    previous: [i32; 4],
    now: [i32; 4],
    last: [i32; 4],
) -> bool {
    !placing && previous != now && now != last
}

/// Whether a browser layout that moved the page view is answered inside that layout.
pub(crate) fn in_layout_placement() -> bool {
    true
}

/// The page view's `OnBoundsChanged`: the browser's first, then the split's own bounds.
unsafe extern "system" fn bounds_changed(this: usize, previous: *const [i32; 4]) {
    let original: unsafe extern "system" fn(usize, *const [i32; 4]) =
        std::mem::transmute(ORIGINAL_BOUNDS_CHANGED.load(Ordering::Acquire));
    original(this, previous);
    let Some(split) = split().filter(|split| split.main == this) else {
        return;
    };
    let last = *LAST.lock().unwrap_or_else(|error| error.into_inner());
    let previous = if previous.is_null() {
        [0; 4]
    } else {
        *previous
    };
    if !needs_layout(
        PLACING.load(Ordering::Acquire),
        previous,
        bounds_of(this),
        last,
    ) {
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
        post_layout(split.window as HWND);
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
    copy[slot + 1] = bounds_changed as *const () as usize;
    let copy: &'static mut [usize] = Box::leak(copy.into_boxed_slice());
    *(main as *mut usize) = copy.as_ptr() as usize + 8;
    Some(vtable)
}

/// The strip index of the window's active tab, or the first.
unsafe fn active_tab(window: HWND) -> i32 {
    crate::features::tabs::tabs(window)
        .and_then(|tabs| {
            tabs.iter()
                .find(|tab| tab.active)
                .map(|tab| tab.index as i32)
        })
        .unwrap_or(0)
}

/// Opens a split in `window` with `page`, a page whose tab is in `owner`, another window, to the
/// right of the window's page.
pub(crate) unsafe fn open(window: HWND, owner: HWND, page: usize) -> bool {
    add_page(window, owner, page, 0, Side::Right)
}

/// The page of the window's own tab `index`, when it is not the active one.
unsafe fn page_of_tab(window: HWND, index: usize) -> Option<usize> {
    let base = base()?;
    if index as i32 == active_tab(window) {
        crate::trace::trace("second view: the active tab is already shown");
        return None;
    }
    let model = crate::features::tabs::tab_model(base, window)?;
    let page = crate::features::tabs::contents_at(base, model, index as i32) as usize;
    (page != 0).then_some(page)
}

/// Opens an extra view with the page of the window's own tab `index`, which is not the active
/// one, laid out as `mode`. The tab keeps its page; closing the view shows it in the tab again.
pub(crate) unsafe fn open_tab(window: HWND, index: usize, mode: Mode) -> bool {
    let Some(page) = page_of_tab(window, index) else {
        return false;
    };
    let opened = open_with(window, window, page, mode);
    crate::trace::trace(&format!("second view: tab {index} {mode:?}: {opened}"));
    opened
}

/// Shows the page of the window's own tab `index` on `side` of the page in `target`.
pub(crate) unsafe fn add_tab(window: HWND, index: usize, target: usize, side: Side) -> bool {
    let Some(page) = page_of_tab(window, index) else {
        return false;
    };
    let added = add_page(window, window, page, target, side);
    crate::trace::trace(&format!(
        "second view: tab {index} {side:?} of slot {target}: {added}"
    ));
    added
}

/// Shows `page`, whose tab is in `owner`, on `side` of the page in slot `target` of the window's
/// split, which opens when the window has none. This is also where a page that left the split, such
/// as a Mini Window, comes back. Returns false when the grid is full, the page is already shown,
/// or the window shows a docked page.
pub(crate) unsafe fn add_page(
    window: HWND,
    owner: HWND,
    page: usize,
    target: usize,
    side: Side,
) -> bool {
    let state = split().filter(|split| split.window == window as isize);
    let Some(state) = state.filter(|state| state.mode == Mode::Split) else {
        if state.is_some_and(|state| state.mode != Mode::Inset) {
            return false;
        }
        let mut grid = Grid::single();
        grid.insert(0, 1, side);
        set_grid(window as isize, Some(grid));
        let opened = open_with(window, owner, page, Mode::Split);
        if !opened {
            set_grid(window as isize, None);
        }
        return opened;
    };
    let mut grid = grid_of(window as isize);
    let Some(slot) = grid.free_slot() else {
        crate::trace::trace("second view: the grid is full");
        return false;
    };
    if page == state.shown
        || state.extras.iter().any(|extra| extra.page == page)
        || !grid.contains(target)
    {
        return false;
    }
    let Some(view) = new_view(window, state.parent, page) else {
        return false;
    };
    let owner_view = release_from_owner(owner, view, page, *(view as *const usize));
    set_page(view, page);
    grid.insert(target, slot, side);
    set_grid(window as isize, Some(grid));
    let mut next = state;
    next.extras[slot - 1] = Extra {
        view,
        owner_view,
        page,
    };
    store(Some(next));
    place(&next);
    crate::frame::follow_second_view(window);
    crate::trace::trace(&format!("second view: slot {slot} {side:?} of slot {target}, view {view:#x} owner view {owner_view:#x}"));
    true
}

/// Makes one of Rivet's `ContentsWebView`s in the profile of `source`, a page or the window's
/// active page, and adds it to the browser view `parent`. Returns the view.
unsafe fn new_view(window: HWND, parent: usize, source: usize) -> Option<usize> {
    let base = base()?;
    let (Some(new), Some(ctor), Some(add)) =
        (recorded("new"), recorded("ctor"), recorded("add_child"))
    else {
        crate::trace::trace("second view: an address is missing for this build");
        return None;
    };
    let size = recorded("size").unwrap_or(0x528);
    // A view that only keeps the rail free has no page of its own and takes the active page's profile.
    let source = if source != 0 {
        source
    } else {
        active_page(window)
    };
    if source == 0 {
        return None;
    }
    let context = if let Some(rva) = recorded("ctx") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(base, rva)(
            source,
        )
    } else if let Some(offset) = recorded("ctxoff") {
        word(source + offset).unwrap_or(0)
    } else {
        0
    };
    if context == 0 {
        return None;
    }
    let view = crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(
        base, new,
    )(size);
    crate::features::tabs::function::<unsafe extern "system" fn(usize, usize) -> usize>(base, ctor)(
        view, context,
    );
    let children = children_of(parent).len();
    crate::features::tabs::function::<unsafe extern "system" fn(usize, usize, usize) -> usize>(
        base, add,
    )(parent, view, children);
    if let Some(behaviour) = recorded("focus_behavior") {
        // `FocusBehavior::ALWAYS`: a click or `RequestFocus` may give this view the keyboard.
        crate::features::tabs::function::<unsafe extern "system" fn(usize, i32)>(base, behaviour)(
            view, 1,
        );
    }
    Some(view)
}

/// Opens the first extra view in `window` with `page`, whose tab is in `owner`, laid out as `mode`.
/// A grid's layout is set before this.
pub(crate) unsafe fn open_with(window: HWND, owner: HWND, page: usize, mode: Mode) -> bool {
    if let Some(state) = split() {
        if state.window != window as isize || !upgrades(state.mode, mode) {
            crate::trace::trace("second view: already open");
            return false;
        }
        // The view that only kept the rail free takes the page.
        let view = state.extras[0].view;
        let owner_view = release_from_owner(owner, view, page, *(view as *const usize));
        set_page(view, page);
        let mut next = Split { mode, ..state };
        next.extras[0] = Extra {
            view,
            owner_view,
            page,
        };
        store(Some(next));
        place(&next);
        crate::frame::follow_second_view(window);
        crate::trace::trace(&format!("second view: open {mode:?}, view {view:#x} from the rail inset, owner view {owner_view:#x}"));
        return true;
    }
    let Some(base) = base() else { return false };
    let size = recorded("size").unwrap_or(0x528);
    let Some(parent) = browser_view(window) else {
        crate::trace::trace("second view: the browser view was not found");
        return false;
    };
    let Some(view) = new_view(window, parent, page) else {
        return false;
    };
    let vtable = *(view as *const usize);
    if let Some(rva) = recorded("find_slot") {
        let slots: Vec<usize> = (0..0x200usize)
            .filter(|slot| word(vtable + slot * 8) == Some(base + rva))
            .collect();
        crate::trace::trace(&format!(
            "second view: {rva:#x} is at vtable slots {slots:x?}"
        ));
    }
    for slot in recorded("dump_slots")
        .map(|slot| vec![slot, slot + 1, slot + 2])
        .unwrap_or_default()
    {
        let function = word(vtable + slot * 8).unwrap_or(0);
        let bytes: Vec<String> = (0..3)
            .map(|i| format!("{:016x}", word(function + i * 8).unwrap_or(0).swap_bytes()))
            .collect();
        crate::trace::trace(&format!(
            "second view: slot {slot:#x} = {:#x}: {}",
            function.wrapping_sub(base),
            bytes.join("")
        ));
    }
    // The window's page view is the other `ContentsWebView` of its browser view.
    // DevTools has page views of the same class, so the page view is the one holding the active page.
    let own = active_page(window);
    let Some(main) = views_with_vtable(parent, vtable)
        .into_iter()
        .find(|candidate| *candidate != view && own != 0 && holds(*candidate, own, size))
    else {
        crate::trace::trace("second view: the window's page view was not found");
        destroy_view(parent, view);
        return false;
    };
    // The owner window's page view lets go of the page first, as a tab move does. A tab of this
    // window that is not active is shown by no page view, so nothing lets go of it.
    let owner_view = if page == 0 {
        0
    } else {
        release_from_owner(owner, view, page, vtable)
    };
    if page != 0 {
        set_page(view, page);
    }
    *FULL.lock().unwrap_or_else(|error| error.into_inner()) = bounds_of(main);
    *LAST.lock().unwrap_or_else(|error| error.into_inner()) = bounds_of(main);
    let main_vtable = hook_main(main).unwrap_or(0);
    let mut extras = [Extra::default(); MAX_PANES - 1];
    extras[0] = Extra {
        view,
        owner_view,
        page,
    };
    let state = Split {
        mode,
        window: window as isize,
        parent,
        main,
        main_vtable,
        main_owner: 0,
        extras,
        shown: own,
        active: own,
    };
    store(Some(state));
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

/// Whether a view open as `now` can take a page shown as `next` without a new view: only the view
/// that keeps the rail free, and only for a page.
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
    if page == 0 {
        return 0;
    }
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

/// Takes one of Rivet's views out of the browser view and deletes it.
unsafe fn destroy_view(parent: usize, view: usize) {
    let Some(base) = base() else { return };
    // `RemoveChildView` hands ownership back without deleting. Without it, a View's destructor
    // takes the view out of its parent itself.
    if let Some(remove) = recorded("remove_child") {
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize)>(base, remove)(
            parent, view,
        );
    }
    match virtual_of(view, "dtor", "dtor_slot") {
        // The scalar deleting destructor; flag 1 frees the memory.
        Some(destroy) => {
            std::mem::transmute::<usize, unsafe extern "system" fn(usize, u32) -> usize>(destroy)(
                view, 1,
            );
        }
        None => crate::trace::trace("second view: no destructor for this build; the view is kept"),
    }
}

/// Keeps `width` DIPs free at the right of the window's page area for the Sidebar's rail, or
/// none for zero. The page view narrows through the same layout as a split, so it works on every
/// build that records the extra views.
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
    post_layout(window);
}

/// The page area of a split in window DIPs as x, y, width and height, below the bar row and left
/// of the rail.
unsafe fn window_area(state: &Split) -> [i32; 4] {
    let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
    let [ox, oy] = window_origin(state.main);
    let main = bounds_of(state.main);
    let full = fitted(state.main, full);
    let full = below_bar(full, top_inset(state, full));
    let (x, y) = (ox - main[0] + full[0], oy - main[1] + full[1]);
    [x, y, full[2] - rail_of(state.window), full[3]]
}

/// The window's page area in window DIPs as left, top, right and bottom: the area the browser gave
/// its page view, also while a split narrows it.
pub(crate) unsafe fn page_area(window: HWND) -> Option<[i32; 4]> {
    if let Some(state) = split().filter(|split| split.window == window as isize) {
        let [x, y, w, h] = window_area(&state);
        return Some([x, y, x + w, y + h]);
    }
    // With no split, the page view is the view of the browser view that holds the active page. It
    // is looked for once per layout and page; a paint in between reads its bounds alone.
    let base = base()?;
    let own = active_page(window);
    let end = base + crate::chromium::image::recorded().map_or(0, |image| image.size);
    let size = recorded("size").unwrap_or(0x528);
    let epoch = layout_epoch();
    if let Some(main) = known_page(window as isize, own, epoch) {
        if own != 0
            && word(main).is_some_and(|vptr| vptr > base && vptr < end)
            && holds(main, own, size)
        {
            let [x, y] = window_origin(main);
            let [_, _, w, h] = bounds_of(main);
            return Some([x, y, x + w, y + h]);
        }
    }
    let parent = browser_view(window)?;
    let mut queue = vec![(parent, 0usize)];
    let mut best: Option<(i64, usize)> = None;
    let mut visited = 0;
    while let Some((view, depth)) = queue.pop() {
        visited += 1;
        if visited > 600 || depth > 8 {
            continue;
        }
        if own != 0
            && word(view).is_some_and(|vptr| vptr > base && vptr < end)
            && holds(view, own, size)
        {
            let [_, _, w, h] = bounds_of(view);
            let area = w as i64 * h as i64;
            if w > 0 && h > 0 && best.is_none_or(|(most, _)| area > most) {
                best = Some((area, view));
            }
        }
        queue.extend(
            children_of(view)
                .into_iter()
                .map(|child| (child, depth + 1)),
        );
    }
    remember_page(window as isize, own, epoch, best.map(|(_, view)| view));
    let (_, main) = best?;
    let [x, y] = window_origin(main);
    let [_, _, w, h] = bounds_of(main);
    Some([x, y, x + w, y + h])
}

/// Counts the browser's layouts and window size and scale changes, after which a page view found
/// before is looked for again.
static LAYOUT_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The browser laid a window out, or a window's size or scale changed.
pub(crate) fn note_layout() {
    LAYOUT_EPOCH.fetch_add(1, Ordering::Relaxed);
}

fn layout_epoch() -> u64 {
    LAYOUT_EPOCH.load(Ordering::Relaxed)
}

/// The page view of a window as last found, for which page and after which layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FoundPage {
    own: usize,
    epoch: u64,
    view: usize,
}

thread_local! {
    static FOUND_PAGES: std::cell::RefCell<std::collections::HashMap<isize, FoundPage>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The page view found for `window` showing page `own`, when nothing was laid out since.
fn known_page(window: isize, own: usize, epoch: u64) -> Option<usize> {
    FOUND_PAGES.with(|all| {
        all.borrow()
            .get(&window)
            .filter(|found| found.own == own && found.epoch == epoch)
            .map(|found| found.view)
    })
}

/// Keeps what a walk found for `window`, or forgets the window when it found nothing.
fn remember_page(window: isize, own: usize, epoch: u64, view: Option<usize>) {
    FOUND_PAGES.with(|all| {
        let mut all = all.borrow_mut();
        match view {
            Some(view) => {
                all.insert(window, FoundPage { own, epoch, view });
            }
            None => {
                all.remove(&window);
            }
        }
    });
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

/// Takes the page in `slot` out of the window's split. Its sibling takes its place, the page goes
/// back to its tab, and the split ends when one page is left. Taking the window's own page out
/// moves the page that takes over from it into the window's page view and makes its tab active.
pub(crate) unsafe fn close_pane(window: HWND, slot: usize) -> bool {
    let Some(state) =
        split().filter(|split| split.window == window as isize && split.mode == Mode::Split)
    else {
        return false;
    };
    let mut grid = grid_of(window as isize);
    if !grid.contains(slot) {
        return false;
    }
    let mut next = state;
    if slot == 0 {
        let Some(heir) = grid.heir(0) else {
            return false;
        };
        let extra = state.extras[heir - 1];
        // The heir's view lets go, and the window's page view shows that page instead.
        set_page(extra.view, 0);
        next.extras[heir - 1] = Extra::default();
        next.shown = extra.page;
        next.active = extra.page;
        next.main_owner = extra.owner_view;
        store(Some(next));
        match tab_of(window, extra.page) {
            Some(tab) => {
                crate::features::tabs::activate_tab(window, tab);
            }
            None => {
                set_page(state.main, extra.page);
            }
        }
        // The window's own page goes back to its tab, and a page of another window to its owner.
        if state.main_owner != 0 {
            set_page(state.main_owner, state.shown);
        }
        destroy_view(state.parent, extra.view);
        grid.remove(0);
        grid.rename(heir, 0);
        set_focused(0);
    } else {
        let extra = state.extras[slot - 1];
        set_page(extra.view, 0);
        next.extras[slot - 1] = Extra::default();
        // The window shows its own page's tab again when the closed page's tab was active.
        if active_page(window) == extra.page {
            if let Some(tab) = tab_of(window, state.shown) {
                crate::features::tabs::activate_tab(window, tab);
            }
            next.active = state.shown;
        }
        store(Some(next));
        destroy_view(state.parent, extra.view);
        if extra.owner_view != 0 {
            set_page(extra.owner_view, extra.page);
        }
        grid.remove(slot);
        if FOCUSED.load(Ordering::Acquire) == slot {
            set_focused(0);
        }
    }
    let left = grid.leaves().len();
    set_grid(window as isize, Some(grid));
    crate::trace::trace(&format!(
        "second view: closed slot {slot}, {left} pages left"
    ));
    if left < 2 {
        return close(window);
    }
    place(&next);
    crate::frame::follow_second_view(window);
    true
}

/// Closes every extra view: each lets go of its page, the page view gets its own vtable and its
/// full bounds back, the views are deleted, and each page returns to its owner window's page view.
/// While the rail is shown, an empty view opens again and keeps the rail free.
pub(crate) unsafe fn close(window: HWND) -> bool {
    let Some(split) = split().filter(|split| split.window == window as isize) else {
        return false;
    };
    let reopen = after_close(split.mode, if keeps_inset(window as isize) { 1 } else { 0 });
    store(None);
    set_grid(window as isize, None);
    set_focused(0);
    for extra in split.extras.iter().filter(|extra| extra.view != 0) {
        set_page(extra.view, 0);
    }
    // The window's page view may show a page that is not the active tab's. It shows the active
    // one again, so the window agrees with its tab strip.
    if let Some(page) = page_after_close(active_page(window), split.shown) {
        set_page(split.main, page);
        if split.main_owner != 0 {
            set_page(split.main_owner, split.shown);
        }
    }
    if split.main_vtable != 0 {
        *(split.main as *mut usize) = split.main_vtable;
    }
    let full = *FULL.lock().unwrap_or_else(|error| error.into_inner());
    if full[2] > 0 {
        set_bounds(split.main, full);
    }
    *LAST.lock().unwrap_or_else(|error| error.into_inner()) = [0; 4];
    for extra in split.extras.iter().filter(|extra| extra.view != 0) {
        destroy_view(split.parent, extra.view);
        if extra.owner_view != 0 {
            set_page(extra.owner_view, extra.page);
        }
    }
    crate::trace::trace(&format!(
        "second view: closed after {} layouts",
        LAYOUTS.swap(0, Ordering::Relaxed)
    ));
    if reopen.is_some() {
        let inset = open_with(window, window, 0, Mode::Inset);
        crate::trace::trace(&format!("second view: closed to the rail inset: {inset}"));
    }
    crate::features::pane_infobar::follow(window);
    true
}

/// Drops the state of a window that is being destroyed, without calling into its views, which the
/// browser deletes with the window.
pub(crate) fn forget(window: HWND) {
    TOPS.lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|(key, _)| *key != window as isize);
    set_grid(window as isize, None);
    let mut split = SPLIT.lock().unwrap_or_else(|error| error.into_inner());
    if split.is_some_and(|split| split.window == window as isize) {
        *split = None;
    }
}

/// Gives the page view of `slot` the keyboard through `View::RequestFocus`.
unsafe fn focus_slot(window: HWND, slot: usize) -> bool {
    let Some(split) = split().filter(|split| split.window == window as isize) else {
        return false;
    };
    let view = view_of(&split, slot);
    if view == 0 {
        return false;
    }
    let Some(function) = virtual_of(view, "request_focus", "focus_slot") else {
        return false;
    };
    std::mem::transmute::<usize, unsafe extern "system" fn(usize)>(function)(view);
    true
}

/// Gives the focused extra view the keyboard, or the first extra view when the window's own page
/// was focused.
pub(crate) unsafe fn focus(window: HWND) -> bool {
    let Some(split) = split().filter(|split| split.window == window as isize) else {
        return false;
    };
    let focused = FOCUSED.load(Ordering::Acquire);
    let slot = if focused > 0 && view_of(&split, focused) != 0 {
        focused
    } else {
        1
    };
    focus_slot(window, slot)
}

/// Gives the page of `slot` the keyboard, an extra view or the window's own page.
pub(crate) unsafe fn focus_pane(window: HWND, slot: usize) -> bool {
    set_focused(slot);
    slot > 0 && focus_slot(window, slot)
}

/// Sets the first page's share of the window's first divider, in thousandths, and lays the pages
/// out again.
pub(crate) unsafe fn set_share(window: HWND, share: u32) -> bool {
    let Some(split) = split().filter(|split| split.window == window as isize) else {
        return false;
    };
    let mut grid = grid_of(window as isize);
    if !grid.set_root_share(share) {
        return false;
    }
    set_grid(window as isize, Some(grid));
    place(&split);
    crate::trace::trace(&format!(
        "second view: share {share}, full {:?}",
        *FULL.lock().unwrap_or_else(|error| error.into_inner())
    ));
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

/// The pixels per DIP of a window, at the scale the browser laid it out. Windows can report another
/// DPI for the window, as it does while the window crosses monitors of different DPIs.
unsafe fn scale_of(window: HWND) -> f32 {
    crate::frame::layout_dpi(window) as f32 / 96.0
}

/// The dividers of the window's split in window DIPs.
unsafe fn dividers(window: HWND) -> Vec<crate::features::split_grid::Divider> {
    match split().filter(|split| split.window == window as isize && split.mode == Mode::Split) {
        Some(state) => grid_of(window as isize).dividers(window_area(&state), METRICS),
        None => Vec::new(),
    }
}

/// The extra slot whose view is under a window DIP point.
unsafe fn extra_at(window: HWND, x: i32, y: i32) -> Option<usize> {
    let state = split().filter(|split| split.window == window as isize)?;
    (1..MAX_PANES).find(|slot| {
        let view = view_of(&state, *slot);
        let [vx, vy, w, h] = if view == 0 { [0; 4] } else { window_rect(view) };
        w > 0 && x >= vx && x < vx + w && y >= vy && y < vy + h
    })
}

fn inside(rect: [i32; 4], x: i32, y: i32) -> bool {
    x >= rect[0] && x < rect[0] + rect[2] && y >= rect[1] && y < rect[1] + rect[3]
}

/// Opens a split with the first page of another window of this browser, the window that owns that
/// page's tab.
pub(crate) unsafe fn open_from_other_window(window: HWND) -> bool {
    let Some(base) = base() else { return false };
    for other in crate::frame::browser_windows_of_process() {
        if other == window {
            continue;
        }
        let Some(model) = crate::features::tabs::tab_model(base, other) else {
            continue;
        };
        let page = crate::features::tabs::contents_at(base, model, 0) as usize;
        if page != 0 {
            return open(window, other, page);
        }
    }
    crate::trace::trace("second view: no other window has a page");
    false
}

/// The split node whose divider is being dragged, or none.
static DRAGGING: AtomicUsize = AtomicUsize::new(usize::MAX);

/// What a mouse message means for the split, before the browser sees it. A press on a divider
/// starts a drag that moves it and is not passed on. A press in an extra view goes to the browser
/// first, then the view asks for the keyboard. Returns the result when the message is handled here.
pub(crate) unsafe fn on_mouse(
    window: HWND,
    message: u32,
    lparam: isize,
    browser: &dyn Fn() -> isize,
) -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    };
    if !is_open(window) {
        return None;
    }
    let scale = scale_of(window);
    let x = ((lparam & 0xffff) as i16 as f32 / scale).floor() as i32;
    let y = (((lparam >> 16) & 0xffff) as i16 as f32 / scale).floor() as i32;
    match message {
        WM_LBUTTONDOWN => {
            if let Some(divider) = dividers(window)
                .into_iter()
                .find(|divider| inside(divider.rect, x, y))
            {
                DRAGGING.store(divider.node, Ordering::Release);
                rivet_platform_windows::focus::capture(window);
                return Some(0);
            }
            let Some(slot) = extra_at(window, x, y) else {
                // A press in the window's own page gives it the keyboard back. The rows follow.
                if is_split(window) && FOCUSED.swap(0, Ordering::AcqRel) != 0 {
                    post_layout(window);
                }
                return None;
            };
            set_focused(slot);
            let result = browser();
            // The click may have made the page's tab active, which moves the page into the
            // window's page view. Rivet puts it back now, and again after the browser's own
            // messages, since the activation may come later.
            if !hold(window) {
                focus_slot(window, slot);
            }
            post_layout(window);
            Some(result)
        }
        WM_MOUSEMOVE if DRAGGING.load(Ordering::Acquire) != usize::MAX => {
            let node = DRAGGING.load(Ordering::Acquire);
            if let Some(divider) = dividers(window)
                .into_iter()
                .find(|divider| divider.node == node)
            {
                let position = match divider.dir {
                    crate::features::split_grid::Dir::Row => x,
                    crate::features::split_grid::Dir::Column => y,
                };
                let share = crate::features::split_grid::share_at(
                    divider.span,
                    divider.dir,
                    position,
                    METRICS,
                );
                let mut grid = grid_of(window as isize);
                if grid.set_share(node, share) {
                    set_grid(window as isize, Some(grid));
                    if let Some(state) = split() {
                        place(&state);
                    }
                }
            }
            Some(0)
        }
        WM_LBUTTONUP | WM_CAPTURECHANGED
            if DRAGGING.swap(usize::MAX, Ordering::AcqRel) != usize::MAX =>
        {
            if message == WM_LBUTTONUP {
                rivet_platform_windows::focus::release(window);
            }
            Some(0)
        }
        _ => None,
    }
}

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
            crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(base, rva)(
                page,
            )
        } else if let Some(offset) = recorded("ctxoff") {
            word(page + offset).unwrap_or(0)
        } else {
            0
        };
        let parent = browser_view(window)?;
        if context == 0 {
            return None;
        }
        let view = crate::features::tabs::function::<unsafe extern "system" fn(usize) -> usize>(
            base, new,
        )(recorded("size").unwrap_or(0x528));
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize) -> usize>(
            base, ctor,
        )(view, context);
        let children = children_of(parent).len();
        crate::features::tabs::function::<unsafe extern "system" fn(usize, usize, usize) -> usize>(
            base, add,
        )(parent, view, children);
        set_page(view, page);
        if let Some(behaviour) = recorded("focus_behavior") {
            crate::features::tabs::function::<unsafe extern "system" fn(usize, i32)>(
                base, behaviour,
            )(view, 1);
        }
        Some((view, parent))
    }

    /// Places a view at `rect` (x, y, width, height) in window DIPs.
    pub(crate) unsafe fn place(view: usize, parent: usize, rect: [i32; 4]) {
        let [ox, oy] = window_origin(parent);
        set_bounds(
            view,
            [rect[0] - ox, rect[1] - oy, rect[2].max(1), rect[3].max(1)],
        );
    }

    /// Lets go of the view's page, takes the view out of its parent and deletes it. The page stays
    /// with its tab.
    pub(crate) unsafe fn drop_view(view: usize, parent: usize) {
        let Some(base) = base() else { return };
        set_page(view, 0);
        if let Some(remove) = recorded("remove_child") {
            crate::features::tabs::function::<unsafe extern "system" fn(usize, usize)>(
                base, remove,
            )(parent, view);
        }
        match virtual_of(view, "dtor", "dtor_slot") {
            Some(destroy) => {
                std::mem::transmute::<usize, unsafe extern "system" fn(usize, u32) -> usize>(
                    destroy,
                )(view, 1);
            }
            None => {
                crate::trace::trace("bare view: no destructor for this build; the view is kept")
            }
        }
    }

    /// The window's active page, or zero.
    pub(crate) unsafe fn active(window: HWND) -> usize {
        active_page(window)
    }

    /// The page of the window's tab `index`, or zero.
    pub(crate) unsafe fn page_of_tab(window: HWND, index: usize) -> usize {
        let Some(base) = base() else { return 0 };
        crate::features::tabs::tab_model(base, window).map_or(0, |model| {
            crate::features::tabs::contents_at(base, model, index as i32) as usize
        })
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;

    #[test]
    fn nfr2_a_paint_without_a_layout_does_not_walk_the_views_again() {
        let (window, page) = (0x5150_i32 as isize, 0x7000);
        let epoch = layout_epoch();
        assert_eq!(
            known_page(window, page, epoch),
            None,
            "nothing found yet: walk"
        );
        remember_page(window, page, epoch, Some(0x9000));
        // A paint with the same page and no layout since takes the view found before.
        assert_eq!(known_page(window, page, epoch), Some(0x9000));
        assert_eq!(known_page(window, page, epoch), Some(0x9000));
        // Another page, or a layout, walks again.
        assert_eq!(known_page(window, 0x7008, epoch), None);
        note_layout();
        assert_eq!(known_page(window, page, layout_epoch()), None);
        // A walk that found nothing leaves nothing to take.
        remember_page(window, page, layout_epoch(), None);
        assert_eq!(known_page(window, page, layout_epoch()), None);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        after_close, below_bar, grid_bounds, keep, layout, may_stop_following, needs_layout,
        page_after_close, upgrades, Keep, Mode, DIVIDER, MIN_PANE, SIDEBAR_WIDTH,
    };
    use crate::features::split::Side;
    use crate::features::split_grid::Grid;

    fn pair(side: Side) -> Grid {
        let mut grid = Grid::single();
        grid.insert(0, 1, side);
        grid
    }

    #[test]
    fn fr9_a_browser_layout_of_the_page_view_is_answered_inside_that_layout() {
        // Answering later from a message resized the page twice per layout and left it white.
        assert!(super::in_layout_placement());
    }

    #[test]
    fn fr12_the_page_view_starts_below_the_bar_row_in_every_mode() {
        let full = below_bar([0, 0, 1000, 700], 40);
        assert_eq!(full, [0, 40, 1000, 660]);
        let (page, view) = layout(full, Mode::Inset, 0);
        assert_eq!(
            page,
            [0, 40, 1000, 660],
            "the page begins where the bar ends"
        );
        assert_eq!(view[2], 0);
        let cells = grid_bounds(&pair(Side::Right), full, 0);
        assert_eq!((cells[0].1[1], cells[1].1[1]), (40, 40));
        let (page, _) = layout(full, Mode::Inset, 40);
        assert_eq!(
            page,
            [0, 40, 960, 660],
            "the rail and the bar row both stay free"
        );
        // No row leaves the area as the browser gave it, and a tiny area keeps a page.
        assert_eq!(below_bar([0, 0, 1000, 700], 0), [0, 0, 1000, 700]);
        assert_eq!(below_bar([0, 0, 100, 20], 40)[3], 1);
    }

    #[test]
    fn fr10_rail_inset_keeps_the_page_narrowed_without_a_bounds_hook() {
        // The page view narrows by the rail and the empty view takes no width.
        let (page, view) = layout([0, 50, 1000, 600], Mode::Inset, 40);
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
    const THIRD: usize = 0x4000;

    #[test]
    fn fr13_clicking_the_second_view_page_keeps_both_pages_where_they_are() {
        // The split page's tab became active, so the window's page view gets its page back.
        assert_eq!(
            keep(SPLIT, MAIN, &[SPLIT, 0, 0], MAIN),
            Keep::Restore {
                main: MAIN,
                slot: 1
            }
        );
    }

    #[test]
    fn fr13_clicking_any_extra_page_of_a_grid_keeps_every_page_where_it_is() {
        assert_eq!(
            keep(THIRD, MAIN, &[SPLIT, OTHER, THIRD], MAIN),
            Keep::Restore {
                main: MAIN,
                slot: 3
            }
        );
        assert_eq!(
            keep(OTHER, THIRD, &[SPLIT, OTHER, THIRD], MAIN),
            Keep::Restore {
                main: MAIN,
                slot: 2
            }
        );
        // An unused slot never matches.
        assert_eq!(
            keep(OTHER, MAIN, &[SPLIT, 0, 0], MAIN),
            Keep::Follow { main: OTHER }
        );
    }

    #[test]
    fn fr13_a_kept_page_is_not_moved_again_while_its_tab_stays_active() {
        assert_eq!(keep(SPLIT, SPLIT, &[SPLIT], MAIN), Keep::Nothing);
    }

    #[test]
    fn fr13_clicking_the_window_page_again_leaves_both_pages_in_place() {
        assert_eq!(
            keep(MAIN, SPLIT, &[SPLIT], MAIN),
            Keep::Follow { main: MAIN }
        );
    }

    #[test]
    fn fr13_choosing_another_tab_shows_it_beside_the_split_page() {
        assert_eq!(
            keep(OTHER, MAIN, &[SPLIT], MAIN),
            Keep::Follow { main: OTHER }
        );
        // A click on the split page after that keeps the chosen page in the window's page view.
        assert_eq!(
            keep(SPLIT, OTHER, &[SPLIT], OTHER),
            Keep::Restore {
                main: OTHER,
                slot: 1
            }
        );
    }

    #[test]
    fn fr13_an_unknown_page_changes_nothing() {
        assert_eq!(keep(0, MAIN, &[SPLIT], MAIN), Keep::Nothing);
        assert_eq!(keep(SPLIT, MAIN, &[SPLIT], 0), Keep::Nothing);
    }

    #[test]
    fn fr13_closing_the_split_shows_the_active_tab_page_in_the_window() {
        assert_eq!(page_after_close(SPLIT, MAIN), Some(SPLIT));
        assert_eq!(page_after_close(MAIN, MAIN), None);
        assert_eq!(page_after_close(0, MAIN), None);
    }

    #[test]
    fn fr9_a_tab_dropped_on_the_left_shows_its_page_left_of_the_window_page() {
        let cells = grid_bounds(&pair(Side::Left), [0, 0, 1008, 700], 0);
        assert_eq!(cells[0], (1, [0, 0, 500, 700]));
        assert_eq!(cells[1], (0, [500 + DIVIDER, 0, 500, 700]));
    }

    #[test]
    fn fr9_a_tab_dropped_on_the_top_or_bottom_stacks_the_pages() {
        use crate::features::split::ROW_HEIGHT;
        let cells = grid_bounds(&pair(Side::Bottom), [0, 0, 1000, 1052], 0);
        assert_eq!(cells[0], (0, [0, 0, 1000, 500]));
        assert_eq!(cells[1], (1, [0, 500 + DIVIDER + ROW_HEIGHT, 1000, 500]));
        let cells = grid_bounds(&pair(Side::Top), [0, 0, 1000, 1052], 0);
        assert_eq!(cells[0].0, 1, "a page dropped on the top is shown above");
    }

    #[test]
    fn fr9_split_pages_start_with_equal_shares() {
        let cells = grid_bounds(&pair(Side::Right), [10, 50, 1008, 700], 0);
        assert_eq!(cells[0].1, [10, 50, 500, 700]);
        assert_eq!(cells[1].1, [10 + 500 + DIVIDER, 50, 500, 700]);
    }

    #[test]
    fn fr10_a_sidebar_page_floats_over_the_right_of_the_page_which_keeps_its_width() {
        let full = [10, 50, 1000, 700];
        let (main, view) = layout(
            full,
            Mode::Docked {
                width: SIDEBAR_WIDTH,
            },
            0,
        );
        assert_eq!(main, full);
        assert_eq!(view, [10 + 1000 - SIDEBAR_WIDTH, 50, SIDEBAR_WIDTH, 700]);
        let (_, narrow) = layout(
            [0, 0, 200, 700],
            Mode::Docked {
                width: SIDEBAR_WIDTH,
            },
            0,
        );
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
    fn fr9_a_dragged_divider_leaves_each_page_its_narrowest_width() {
        let mut grid = pair(Side::Right);
        grid.set_root_share(0);
        let cells = grid_bounds(&grid, [0, 0, 1000, 600], 0);
        assert_eq!(cells[0].1[2], MIN_PANE);
        assert_eq!(cells[1].1[2], 1000 - DIVIDER - MIN_PANE);
        grid.set_root_share(1000);
        let cells = grid_bounds(&grid, [0, 0, 1000, 600], 0);
        assert_eq!(cells[1].1[2], MIN_PANE);
        assert_eq!(cells[0].1[2] + DIVIDER + cells[1].1[2], 1000);
    }

    #[test]
    fn fr10_the_sidebar_rail_narrows_the_page_and_a_docked_page_ends_at_the_rail() {
        let full = [0, 0, 1000, 700];
        let (main, view) = layout(full, Mode::Inset, 40);
        assert_eq!(main, [0, 0, 960, 700]);
        assert_eq!(view[2], 0, "the rail inset shows no page");
        let (main, view) = layout(
            full,
            Mode::Docked {
                width: SIDEBAR_WIDTH,
            },
            40,
        );
        assert_eq!(main, [0, 0, 960, 700]);
        assert_eq!(view, [960 - SIDEBAR_WIDTH, 0, SIDEBAR_WIDTH, 700]);
    }

    #[test]
    fn fr10_an_undocked_page_leaves_the_rail_inset_while_the_rail_is_shown() {
        assert_eq!(
            after_close(
                Mode::Docked {
                    width: SIDEBAR_WIDTH
                },
                40
            ),
            Some(Mode::Inset)
        );
        assert_eq!(after_close(Mode::Split, 40), Some(Mode::Inset));
        assert_eq!(
            after_close(
                Mode::Docked {
                    width: SIDEBAR_WIDTH
                },
                0
            ),
            None
        );
        assert_eq!(
            after_close(Mode::Inset, 40),
            None,
            "closing the inset itself removes the view"
        );
    }

    #[test]
    fn fr9_a_split_or_dock_takes_the_rail_inset_view_but_not_an_open_page() {
        assert!(upgrades(Mode::Inset, Mode::Split));
        assert!(upgrades(
            Mode::Inset,
            Mode::Docked {
                width: SIDEBAR_WIDTH
            }
        ));
        assert!(!upgrades(
            Mode::Split,
            Mode::Docked {
                width: SIDEBAR_WIDTH
            }
        ));
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
        let panes = super::grid_panes(
            &[
                ([276, 109, 600, 643], false, Some(0)),
                ([884, 52, 600, 700], true, Some(1)),
            ],
            1484,
            Some(8),
        );
        let left = panes.iter().find(|pane| pane.x == 276).unwrap();
        let right = panes.iter().find(|pane| pane.x == 884).unwrap();
        assert_eq!((left.row_y, right.row_y), (8, 8));
        assert!(left.row_y + ROW_HEIGHT <= left.y);
        assert!(left.first && right.last);
    }

    #[test]
    fn fr9_each_split_page_has_its_own_row_right_above_it_with_equal_tops() {
        use crate::features::split::{CAPTION_ROOM, ROW_HEIGHT};
        // The window's page on the left, the second view on the right, focused.
        let panes = super::grid_panes(
            &[
                ([276, 52, 600, 700], false, Some(0)),
                ([884, 52, 600, 700], true, Some(1)),
            ],
            1484,
            None,
        );
        assert_eq!(panes.len(), 2);
        let (left, right) = (panes[0], panes[1]);
        assert_eq!(left.y, right.y, "the pages start at the same top");
        assert_eq!(
            (left.row_y, right.row_y),
            (52 - ROW_HEIGHT, 52 - ROW_HEIGHT)
        );
        assert!(left.first && !left.last && right.last && !right.first);
        assert!(
            !left.active && right.active,
            "only the focused page is highlighted"
        );
        assert_eq!(left.caption, 0.0);
        assert_eq!(
            right.caption, CAPTION_ROOM,
            "the top right page carries the window buttons"
        );
        assert_eq!((left.tab, right.tab), (Some(0), Some(1)));
        // A page shown left of the window's page comes first.
        let panes = super::grid_panes(
            &[
                ([884, 52, 600, 700], true, Some(2)),
                ([276, 52, 600, 700], false, None),
            ],
            1484,
            Some(8),
        );
        assert_eq!((panes[0].tab, panes[1].tab), (None, Some(2)));
        assert!(panes[1].active && panes[1].caption > 0.0 && panes[0].first);
    }

    #[test]
    fn fr9_in_a_four_page_grid_only_the_top_right_page_carries_the_window_buttons() {
        use crate::features::split::{CAPTION_ROOM, ROW_HEIGHT};
        let top = [
            ([276, 52, 600, 300], false, Some(0)),
            ([884, 52, 600, 300], false, Some(1)),
        ];
        let low = [
            ([276, 404, 600, 300], true, Some(2)),
            ([884, 404, 600, 300], false, Some(3)),
        ];
        let pages: Vec<_> = [low[1], top[1], low[0], top[0]].into_iter().collect();
        let panes = super::grid_panes(&pages, 1484, Some(8));
        let tabs: Vec<_> = panes.iter().map(|pane| pane.tab).collect();
        assert_eq!(tabs, [Some(0), Some(1), Some(2), Some(3)], "reading order");
        assert_eq!(panes.iter().filter(|pane| pane.caption > 0.0).count(), 1);
        assert_eq!(panes[1].caption, CAPTION_ROOM);
        assert_eq!(
            panes.iter().filter(|pane| pane.first).count(),
            1,
            "one page leaves room for the toggle"
        );
        assert!(panes[0].first && panes[1].last);
        // Lower rows sit right above their pages, not at the panel's top.
        assert_eq!(
            (panes[2].row_y, panes[3].row_y),
            (404 - ROW_HEIGHT, 404 - ROW_HEIGHT)
        );
        assert_eq!(panes[0].row_y, 8);
        assert!(panes[2].active && !panes[0].active);
    }

    #[test]
    fn fr13_a_split_beside_the_rail_divides_only_the_area_left_of_it() {
        let cells = grid_bounds(&pair(Side::Right), [0, 0, 1048, 600], 40);
        assert_eq!(cells[0].1[2] + DIVIDER + cells[1].1[2], 1008);
        assert_eq!(cells[1].1[0] + cells[1].1[2], 1008);
    }
}
