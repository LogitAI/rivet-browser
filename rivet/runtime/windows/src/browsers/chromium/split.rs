//! Chromium's split view on the browser UI thread. Two tabs shown side by side in one window are
//! Chromium's own split tabs, so Rivet starts and ends them through the tab strip model and lets the
//! browser lay out, focus and restore them. Calls are gated by code signatures.
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use crate::features::frame_layout::Rect;
use crate::builds::Build;
use crate::feature::{available_here, Needs};
use crate::features::split::{caption_over, client_right, fill_area, in_a_line, row_buttons, row_top, Pane, RowButton, RELAYOUT_MESSAGE, ROW_HEIGHT};
use crate::targets::Target;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};

use crate::features::tabs::{base, function, tab_model};

/// The values of the loaded browser build.
fn bv() -> &'static crate::builds::Split {
    &crate::builds::get().split
}

/// What split view needs from the browser.
pub(crate) fn needs(build: &Build) -> Needs {
    let row = &build.split;
    let mut targets = vec![
        Target::new("split calculate layout", row.calculate_layout, row.calculate_layout_prologue),
        Target::new("split container layout", row.container_layout, row.calculate_layout_prologue),
        Target::new("web contents focused", row.web_contents_focused, row.web_contents_focused_prologue),
        Target::new("update border", row.update_border, row.update_border_prologue),
    ];
    // A browser whose top row does not follow the focused page has no use for the flex layout, and
    // one that builds the page corners without a setter keeps the corners it has.
    if row.flex_layout != crate::builds::ABSENT {
        targets.push(Target::new("split flex layout", row.flex_layout, row.calculate_layout_prologue));
    }
    if row.set_rounded_corners != crate::builds::ABSENT {
        targets.push(Target::new("set rounded corners", row.set_rounded_corners, row.set_rounded_corners_prologue));
    }
    targets.extend(row.signatures.iter().map(|(rva, bytes)| Target::new("split function", *rva, bytes)));
    Needs::checked(row.supported, crate::builds::unchecked_reason(build, "split view"), targets)
}

/// The part of the page area layout that belongs to split view: the pages of a split fill the area
/// the Sidebar left, and the toolbar follows the active page.
pub(crate) unsafe fn on_page_area(area: &mut crate::hooks::page_area::PageArea) {
    let row = &crate::builds::get().side_panel;
    if let (Some(toolbar), Some(contents)) = (area.bounds(row.toolbar), area.bounds(row.contents_container)) {
        if area.rail == 0 && active_is_split(area.window) {
            if let Some(page) = area.bounds(row.page_area) {
                fill_area(&mut *contents, &*page);
            }
        }
        fit_toolbar(area.window, &mut *toolbar, &*contents);
    }
}


/// Whether the loaded build's row has split view.
fn checked() -> bool {
    static CHECKED: OnceLock<bool> = OnceLock::new();
    *CHECKED.get_or_init(|| available_here(needs))
}


/// `AddToNewSplit(this, out id, indices, visual data, created source)`.
type AddToNewSplit = unsafe extern "system" fn(
    *mut c_void,
    *mut [u8; 16],
    *const [usize; 3],
    *const [u8; 16],
    u32,
) -> *mut [u8; 16];
/// `RemoveSplit(this, id)`.
type RemoveSplit = unsafe extern "system" fn(*mut c_void, *const [u8; 16]);
/// `GetSplitForTab(this, out optional id, index)`.
type GetSplitForTab = unsafe extern "system" fn(*mut c_void, *mut [u8; 24], i32) -> *mut [u8; 24];
type Allocate = unsafe extern "system" fn(usize) -> *mut i32;

/// The side-by-side layout and an even split. Chrome's visual data is a layout value, then a
/// double ratio at offset 8.
const LAYOUT_SIDE_BY_SIDE: u32 = 0;
const LAYOUT_STACKED: u32 = 1;
const EVEN_RATIO: f64 = 0.5;
/// Chrome only uses the creation source for metrics.
const CREATED_SOURCE: u32 = 0;

fn checked_base() -> Option<usize> {
    if !checked() {
        return None;
    }
    static CHECK: OnceLock<bool> = OnceLock::new();
    let base = base()?;
    let signatures = bv().signatures;
    let ok = *CHECK.get_or_init(|| {
        signatures.iter().all(|(rva, bytes)| unsafe {
            std::slice::from_raw_parts((base + rva) as *const u8, bytes.len()) == *bytes
        })
    });
    if !ok {
        crate::trace::trace("split: signature mismatch");
    }
    ok.then_some(base)
}

/// The id of the split the tab at `index` belongs to.
unsafe fn split_of(base: usize, model: *mut c_void, index: usize) -> Option<[u8; 16]> {
    let mut optional = [0u8; 24];
    function::<GetSplitForTab>(base, bv().get_split_for_tab)(model, &mut optional, index as i32);
    (optional[16] != 0).then(|| {
        let mut id = [0; 16];
        id.copy_from_slice(&optional[..16]);
        id
    })
}
/// Whether the active tab of the window is shown in a split.
unsafe fn active_is_split(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::features::tabs::tabs(window) else { return false };
    tabs.iter()
        .find(|tab| tab.active)
        .is_some_and(|tab| split_of(base, model, tab.index).is_some())
}

/// The tab indices `AddToNewSplit` is given. A build that adds the active tab by itself wants only the
/// other tab, and any other wants both, in strip order.
fn tab_indices(first: usize, second: usize, active: Option<usize>, adds_active: bool) -> Vec<usize> {
    if adds_active {
        vec![if Some(first) == active { second } else { first }]
    } else {
        vec![first, second]
    }
}

/// The vector Chrome takes over, as its three words: the data, then either the end pointer or the
/// size, then either the capacity's end pointer or the capacity.
fn index_vector(data: usize, count: usize, ends_by_pointer: bool) -> [usize; 3] {
    if ends_by_pointer {
        let end = data + 4 * count;
        [data, end, end]
    } else {
        [data, count, count]
    }
}

/// Makes a split of two tabs. The first one is shown first, which is on the left or on top.
unsafe fn split_pair(window: HWND, first: usize, second: usize, layout: u32) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    if split_of(base, model, first).is_some() || split_of(base, model, second).is_some() {
        return false;
    }
    let active = crate::features::tabs::tabs(window)
        .and_then(|tabs| tabs.iter().find(|tab| tab.active).map(|tab| tab.index));
    let listed = tab_indices(first, second, active, bv().split_adds_active);
    let indices = function::<Allocate>(base, bv().allocate)(4 * listed.len());
    if indices.is_null() {
        return false;
    }
    for (at, index) in listed.iter().enumerate() {
        indices.add(at).write(*index as i32);
    }
    let span = index_vector(indices as usize, listed.len(), bv().split_vector_ends_by_pointer);
    let mut visual = [0u8; 16];
    visual[..4].copy_from_slice(&layout.to_le_bytes());
    visual[8..16].copy_from_slice(&EVEN_RATIO.to_le_bytes());
    let mut id = [0u8; 16];
    function::<AddToNewSplit>(base, bv().add_to_new_split)(
        model,
        &mut id,
        &span,
        &visual,
        CREATED_SOURCE,
    );
    crate::trace::trace(&format!("split: created {id:02x?} of {first} and {second}"));
    true
}

/// Ends the split of the active tab. Both tabs stay open.
pub unsafe fn unsplit_active(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::features::tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    let Some(id) = split_of(base, model, active) else { return false };
    function::<RemoveSplit>(base, bv().remove_split)(model, &id);
    true
}

/// The corner radius of a page panel in a split, in device-independent pixels.
const PANEL_RADIUS: f32 = 12.0;


type SetRoundedCorners = unsafe extern "system" fn(*mut c_void, *const [f32; 4]);
static ORIGINAL_CORNERS: AtomicUsize = AtomicUsize::new(0);

/// Pages of Rivet windows are panels with the shell study's radius. Other windows keep Chrome's.
unsafe extern "system" fn set_rounded_corners(view: *mut c_void, corners: *const [f32; 4]) {
    let original: SetRoundedCorners = std::mem::transmute(ORIGINAL_CORNERS.load(Ordering::Acquire));
    if crate::browsers::rivet_owner(view).is_some() {
        // Each page sits under its own top row, so only the lower corners are rounded: upper
        // left, upper right, lower right, lower left.
        let mut wanted = (*corners).map(|radius| if radius > 0.0 { PANEL_RADIUS } else { 0.0 });
        wanted[0] = 0.0;
        wanted[1] = 0.0;
        original(view, &wanted);
    } else {
        original(view, corners);
    }
}

unsafe fn write_code(target: *mut u8, bytes: &[u8]) -> Result<(), String> {
    let mut previous = 0;
    if VirtualProtect(target.cast(), bytes.len(), PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return Err("could not unprotect browser code".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
    VirtualProtect(target.cast(), bytes.len(), previous, &mut previous);
    windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache(
        windows_sys::Win32::System::Threading::GetCurrentProcess(),
        target.cast(),
        bytes.len(),
    );
    Ok(())
}

/// `jmp [rip + 0]` to an absolute address.
fn absolute_jump(code: &mut Vec<u8>, to: usize) {
    code.extend_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    code.extend_from_slice(&(to as u64).to_le_bytes());
}

/// Makes the pages of split tabs in Rivet windows rounded panels. It does nothing unless the
/// function still starts with the recorded bytes.
pub unsafe fn install() -> Result<(), String> {
    if !checked() {
        return Err("this browser build has no recorded split view".into());
    }
    let base = checked_base().ok_or("this browser build has no recorded split view")?;
    if bv().set_rounded_corners == crate::builds::ABSENT {
        return Ok(());
    }
    // A start with no short jump in it moves into a trampoline as it is.
    if bv().set_rounded_corners_changed == crate::builds::ABSENT {
        return detour(
            base,
            bv().set_rounded_corners,
            bv().set_rounded_corners_prologue,
            set_rounded_corners as *const () as usize,
            &ORIGINAL_CORNERS,
        );
    }
    let code = (base + bv().set_rounded_corners) as *mut u8;
    if std::slice::from_raw_parts(code, 13) != bv().set_rounded_corners_prologue {
        return Err("the rounded corners function is not the recorded one".into());
    }
    if ORIGINAL_CORNERS.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    // The comparison runs in the trampoline. Its `jne` has a short offset, so it is rebuilt: taken
    // goes to the original's changed path, and not taken goes on to the original's `jp`.
    let mut trampoline = bv().set_rounded_corners_prologue[..11].to_vec();
    trampoline.extend_from_slice(&[0x75, 0x0E]);
    absolute_jump(&mut trampoline, base + bv().set_rounded_corners_parity);
    absolute_jump(&mut trampoline, base + bv().set_rounded_corners_changed);
    let memory = VirtualAlloc(
        std::ptr::null(),
        trampoline.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    ) as *mut u8;
    if memory.is_null() {
        return Err("could not allocate executable memory".into());
    }
    std::ptr::copy_nonoverlapping(trampoline.as_ptr(), memory, trampoline.len());
    ORIGINAL_CORNERS.store(memory as usize, Ordering::Release);
    // mov rax, hook; jmp rax. The rest of the old jne is never run.
    let mut patch = vec![0x48, 0xB8];
    patch.extend_from_slice(&(set_rounded_corners as *const () as u64).to_le_bytes());
    patch.extend_from_slice(&[0xFF, 0xE0]);
    write_code(code, &patch)
}

type UpdateBorder = unsafe extern "system" fn(*mut c_void, u32, u32, u32);
static ORIGINAL_BORDER: AtomicUsize = AtomicUsize::new(0);


unsafe fn virtual_slot<T: Copy>(view: *mut c_void, slot: usize) -> T {
    let table = *(view as *const *const usize);
    std::mem::transmute_copy(&*table.add(slot))
}

unsafe extern "system" fn update_border(view: *mut c_void, show: u32, a: u32, b: u32) {
    let original: UpdateBorder = std::mem::transmute(ORIGINAL_BORDER.load(Ordering::Acquire));
    original(view, show, a, b);
    if crate::browsers::rivet_owner(view).is_none() {
        return;
    }
    // Chrome has just laid its outline and the space inside it out. The mini toolbar stays.
    let no_border: usize = 0;
    virtual_slot::<unsafe extern "system" fn(*mut c_void, *const usize)>(view, bv().set_border_slot)(
        view,
        &no_border,
    );
    let child = *((view as usize + bv().outline_member) as *const *mut c_void);
    if !child.is_null() {
        virtual_slot::<unsafe extern "system" fn(*mut c_void, u32)>(child, bv().set_visible_slot)(
            child, 0,
        );
    }
}

/// Replaces the start of a function, which must be whole instructions that do not depend on their
/// own address, with a jump to `hook`. The trampoline that runs the original is stored.
unsafe fn detour(
    base: usize,
    rva: usize,
    prologue: &[u8],
    hook: usize,
    store: &AtomicUsize,
) -> Result<(), String> {
    use crate::targets::{detour_styled, Cookie, Style};
    let style = Style { cookie: Cookie::None, once: true, journaled: false, ..Style::JOURNALED };
    detour_styled(base, rva, prologue, hook, Some(store), style).map(|_| ())
}

/// The window's panel already leaves its gap on every side, so the pages of a split start exactly
/// where a single page does.
const SPLIT_INSETS: [i32; 4] = [0, 0, 0, 0];
/// The width of the resize area between two pages, and the gap the shell study wants.
const CHROME_GAP: i32 = 10;
const PANEL_GAP: i32 = 8;
type CalculateLayout = unsafe extern "system" fn(*mut c_void, *mut u8, *const c_void) -> *mut u8;
static ORIGINAL_LAYOUT: AtomicUsize = AtomicUsize::new(0);

/// One window's split as the last layout left it: the view, and the page rectangles in the view.
struct SplitPanes {
    view: usize,
    /// The resize area between the pages, which paints the gap.
    gap: usize,
    rects: Vec<Rect>,
}

static SPLITS: Mutex<Vec<(usize, SplitPanes)>> = Mutex::new(Vec::new());

/// `ProposedLayout::child_layouts`: an inlined vector of 0x30 byte entries after a size word.
/// Each starts with the view, then a visible flag at 8 and the bounds at 0xc.
unsafe fn layout_children(out: *mut u8) -> (*mut u8, usize) {
    let size = *(out.add(8) as *const usize);
    let data = if size & 1 == 1 { *(out.add(0x10) as *const *mut u8) } else { out.add(0x10) };
    (data, size >> 1)
}

unsafe fn child_bounds(child: *mut u8) -> *mut [i32; 4] {
    child.add(0xc) as *mut [i32; 4]
}

/// Narrows the resize area between two side by side pages from Chrome's width to the panel gap.
fn narrow_gap(left: &mut [i32; 4], middle: &mut [i32; 4], right: &mut [i32; 4]) -> bool {
    let touching = left[0] + left[2] == middle[0] && middle[0] + middle[2] == right[0];
    if !touching || middle[2] != CHROME_GAP || left[1] != right[1] {
        return false;
    }
    let spare = (CHROME_GAP - PANEL_GAP) / 2;
    left[2] += spare;
    middle[0] += spare;
    middle[2] -= 2 * spare;
    right[0] -= spare;
    right[2] += spare;
    true
}

unsafe extern "system" fn calculate_layout(
    delegate: *mut c_void,
    out: *mut u8,
    bounds: *const c_void,
) -> *mut u8 {
    let original: CalculateLayout = std::mem::transmute(ORIGINAL_LAYOUT.load(Ordering::Acquire));
    let view = (delegate as usize - bv().layout_delegate_offset) as *mut c_void;
    let Some(widget) = crate::browsers::rivet_owner(view) else {
        return original(delegate, out, bounds);
    };
    *((view as usize + bv().split_insets_member) as *mut [i32; 4]) = SPLIT_INSETS;
    let result = original(delegate, out, bounds);
    let (data, count) = layout_children(out);
    let shown: Vec<*mut u8> = (0..count)
        .map(|index| data.add(index * 0x30))
        .filter(|child| *child.add(8) != 0)
        .collect();
    // The pages are the visible entries that are the view's contents containers.
    let containers = *((view as usize + bv().containers_member) as *const *const usize);
    let total = *((view as usize + bv().container_count_member) as *const usize);
    let panes: Vec<*mut u8> = (0..total.min(4))
        .filter_map(|index| {
            let container = *containers.add(index);
            shown.iter().copied().find(|child| *(*child as *const usize) == container)
        })
        .collect();
    let mut splits = SPLITS.lock().unwrap_or_else(|error| error.into_inner());
    let previous = splits
        .iter()
        .position(|(key, _)| *key == widget)
        .map(|at| splits.remove(at).1);
    if panes.len() < 2 {
        if previous.is_some() {
            request_relayout(view as usize);
        }
        // A tab dragged onto a single page needs the page area before any split exists.
        if let Some(window) = crate::features::frame_layout::hwnd_of_view(view) {
            crate::hooks::page_area::remember_own_area(window, view as usize);
        }
        return result;
    }
    let mut gap = 0;
    if let [first, second] = panes[..] {
        let (a, b) = (*child_bounds(first), *child_bounds(second));
        let between = shown.iter().copied().find(|child| {
            let rect = *child_bounds(*child);
            rect[0] == a[0] + a[2] && rect[0] + rect[2] == b[0]
        });
        if let Some(between) = between {
            gap = *(between as *const usize);
            narrow_gap(&mut *child_bounds(first), &mut *child_bounds(between), &mut *child_bounds(second));
        }
        // Pages stacked on top of each other: the lower page's top row goes into the space the
        // resize area leaves, so the upper page ends higher.
        let stacked_between = shown.iter().copied().find(|child| {
            let rect = *child_bounds(*child);
            a[0] == b[0] && a[2] == b[2] && rect[1] == a[1] + a[3] && rect[1] + rect[3] == b[1]
        });
        if let Some(between) = stacked_between {
            gap = *(between as *const usize);
            let bottom = b[1] - ROW_HEIGHT - PANEL_GAP;
            (*child_bounds(first))[3] = (bottom - a[1]).max(1);
            let strip = &mut *child_bounds(between);
            strip[1] = bottom;
            strip[3] = PANEL_GAP;
        }
    }
    let rects = panes
        .iter()
        .map(|child| {
            let [x, y, width, height] = *child_bounds(*child);
            Rect { x, y, width, height }
        })
        .collect();
    let changed = previous.as_ref().is_none_or(|before| before.rects != rects);
    if changed {
        request_relayout(view as usize);
    }
    if let Some(window) = crate::features::frame_layout::hwnd_of_view(view) {
        crate::hooks::page_area::remember_own_area(window, view as usize);
    }
    splits.push((widget, SplitPanes { view: view as usize, gap, rects }));
    result
}
/// Whether the browser's address can follow the focused page. A toolbar that places it by hand keeps
/// it over the first page, so only that page's row shows the browser's address.
pub fn address_follows_page() -> bool {
    crate::builds::get().toolbar.toolbar_layout != crate::builds::ABSENT
}

/// `views::FlexLayout::CalculateProposedLayout`, which lays out the children of the browser's top
/// row. It is a method of a layout manager that keeps its host view at +0x38.
static ORIGINAL_FLEX: AtomicUsize = AtomicUsize::new(0);
/// The primary vtable of `LocationBarView`, which tells the top row's container among all hosts.
/// The size of a view is two ints at +0x1d0.

/// The windows whose top row currently only holds the address of the focused page, with where that
/// address starts in the row and how wide it is.
static ADDRESS_ONLY: Mutex<Vec<(isize, (i32, i32))>> = Mutex::new(Vec::new());

/// In a split the top row of the browser shrinks to the focused page's address, so its other
/// children (back, forward, reload, the split button) are hidden by the layout, which restores them
/// as soon as the window leaves the split. The address fills the row.
unsafe extern "system" fn flex_layout(layout: *mut c_void, out: *mut u8, bounds: *const c_void) -> *mut u8 {
    let original: CalculateLayout = std::mem::transmute(ORIGINAL_FLEX.load(Ordering::Acquire));
    let result = original(layout, out, bounds);
    if ADDRESS_ONLY.lock().unwrap_or_else(|error| error.into_inner()).is_empty() {
        return result;
    }
    let Some(base) = base() else { return result };
    let (data, count) = layout_children(out);
    let location = (0..count).map(|index| data.add(index * 0x30)).find(|child| {
        let view = *(*child as *const usize);
        view != 0 && *(view as *const usize) == base + bv().location_bar_vtable
    });
    let Some(location) = location else { return result };
    let host = *((layout as usize + bv().flex_layout_host) as *const usize);
    let address = crate::features::frame_layout::hwnd_of_view(host as *const c_void).and_then(|window| {
        ADDRESS_ONLY
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .find(|(key, _)| *key == window as isize)
            .map(|(_, address)| *address)
    });
    let Some((left, width)) = address else { return result };
    for index in 0..count {
        let child = data.add(index * 0x30);
        if child != location {
            *child.add(8) = 0;
        }
    }
    let rect = &mut *child_bounds(location);
    rect[0] = left;
    rect[2] = width;
    LOCATION.store(*(location as *const usize), Ordering::Relaxed);
    result
}

/// The address view the top row's layout last placed.
static LOCATION: AtomicUsize = AtomicUsize::new(0);

/// A browser's toolbar can move the address after the flex layout placed it, so once the toolbar is
/// done, the address of a split goes back to the focused page.
pub unsafe fn place_address(toolbar: *mut c_void) {
    let location = LOCATION.load(Ordering::Relaxed);
    let layout = &crate::builds::get().layout;
    let Some(base) = base() else { return };
    if location == 0 || !std::slice::from_raw_parts((base + layout.set_bounds_rect) as *const u8, layout.set_bounds_rect_prologue.len()).eq(layout.set_bounds_rect_prologue) {
        return;
    }
    let Some(window) = crate::features::frame_layout::hwnd_of_view(toolbar) else { return };
    let address = ADDRESS_ONLY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window as isize)
        .map(|(_, address)| *address);
    let Some((left, width)) = address else { return };
    let mut bounds = *((location + layout.view_bounds_offset) as *const [i32; 4]);
    if (bounds[0], bounds[2]) != (left, width) {
        bounds[0] = left;
        bounds[2] = width;
        function::<unsafe extern "system" fn(usize, *const [i32; 4])>(base, layout.set_bounds_rect)(location, &bounds);
    }
}

/// `views::View::SchedulePaint(this)`. The background only paints again when it is told to, so a
/// change of the pages or of the focused one must tell it.
type SchedulePaint = unsafe extern "system" fn(*mut c_void);

unsafe fn repaint(view: usize) {
    // A row without the function leaves the gap as the browser paints it. Edge 156.0.4314.58 is such
    // a row: scheduling the gap's paint after a click made the click stop activating its pane.
    if view == 0 || bv().schedule_paint == crate::builds::ABSENT {
        return;
    }
    let Some(base) = base() else { return };
    let wanted = bv().schedule_paint_prologue;
    let code = std::slice::from_raw_parts((base + bv().schedule_paint) as *const u8, wanted.len());
    if code == wanted {
        function::<SchedulePaint>(base, bv().schedule_paint)(view as *mut c_void);
    }
}

type FocusedHook = unsafe extern "system" fn(*mut c_void, *mut c_void);
static ORIGINAL_FOCUSED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn web_contents_focused(view: *mut c_void, web_view: *mut c_void) {
    let original: FocusedHook = std::mem::transmute(ORIGINAL_FOCUSED.load(Ordering::Acquire));
    original(view, web_view);
    let widget = crate::browsers::rivet_owner(view).unwrap_or(0);
    let split = SPLITS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == widget)
        .map(|(_, split)| split.gap);
    if let Some(gap) = split {
        repaint(gap);
        request_relayout(view as usize);
    }
}

/// The member of a contents container that holds its mini toolbar, the chip with the page's address
/// that Chrome floats over the lower corner of a page. Rivet's rows show the address, so it stays hidden.
static ORIGINAL_CONTAINER_LAYOUT: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn container_layout(
    delegate: *mut c_void,
    out: *mut u8,
    bounds: *const c_void,
) -> *mut u8 {
    let original: CalculateLayout = std::mem::transmute(ORIGINAL_CONTAINER_LAYOUT.load(Ordering::Acquire));
    let result = original(delegate, out, bounds);
    let view = (delegate as usize - bv().container_delegate_offset) as *mut c_void;
    if crate::browsers::rivet_owner(view).is_some() {
        let toolbar = *((view as usize + bv().mini_toolbar_member) as *const usize);
        let (data, count) = layout_children(out);
        for index in 0..count {
            let child = data.add(index * 0x30);
            if toolbar != 0 && *(child as *const usize) == toolbar && *child.add(8) != 0 {
                *child.add(8) = 0;
            }
        }
    }
    result
}
unsafe fn request_relayout(view: usize) {
    let Some(window) = crate::features::frame_layout::hwnd_of_view(view as *const c_void) else { return };
    let name: Vec<u16> = RELAYOUT_MESSAGE.encode_utf16().chain(Some(0)).collect();
    let message = windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(name.as_ptr());
    windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(window as _, message, 0, 0);
}

/// The bounds of the pages' area as the browser's last layout left them, by window. The area's
/// vertical extent is `page_geometry`'s, and its horizontal one is here because the area starts
/// after the appbar.
static CONTENTS: Mutex<Vec<(isize, [i32; 4])>> = Mutex::new(Vec::new());

/// The page area of a window as left, top, right and bottom in device-independent pixels.
pub unsafe fn page_area(window: HWND) -> Option<[i32; 4]> {
    let Some(geometry) = crate::hooks::page_area::page_geometry(window) else { return crate::hooks::page_area::own_area(window) };
    let contents = CONTENTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window as isize)
        .map(|(_, bounds)| *bounds)?;
    let top = geometry.top - crate::hooks::page_area::area_top(window)? + contents[1];
    Some([contents[0], top, contents[0] + contents[2], geometry.bottom])
}

/// The tabs of the split the active tab is in, in strip order, which is the order of its pages.
unsafe fn split_tabs(window: HWND) -> Vec<usize> {
    let Some(base) = checked_base() else { return Vec::new() };
    let Some(model) = tab_model(base, window) else { return Vec::new() };
    let Some(tabs) = crate::features::tabs::tabs(window) else { return Vec::new() };
    let Some(active) = tabs.iter().find(|tab| tab.active) else { return Vec::new() };
    let Some(id) = split_of(base, model, active.index) else { return Vec::new() };
    tabs.iter()
        .filter(|tab| split_of(base, model, tab.index) == Some(id))
        .map(|tab| tab.index)
        .collect()
}
/// The pages of the window's split, or none when the active tab is not in one.
pub unsafe fn panes(window: HWND) -> Vec<Pane> {
    let (area, view_top) = match (
        page_area(window),
        crate::hooks::page_area::page_geometry(window).zip(crate::hooks::page_area::area_top(window)),
    ) {
        (Some(area), Some((geometry, top))) => (area, geometry.top - top),
        _ => match crate::hooks::page_area::own_area(window) {
            Some(area) => (area, area[1] - ROW_HEIGHT),
            None => return Vec::new(),
        },
    };
    let tabs = split_tabs(window);
    if tabs.len() < 2 {
        return Vec::new();
    }
    let splits = SPLITS.lock().unwrap_or_else(|error| error.into_inner());
    let Some((_, split)) = splits.iter().find(|(_, split)| {
        crate::features::frame_layout::hwnd_of_view(split.view as *const c_void)
            .is_some_and(|found| found as isize == window as isize)
    }) else {
        return Vec::new();
    };
    if !in_a_line(&split.rects) {
        return Vec::new();
    }
    let active = *((split.view + bv().active_member) as *const i32);
    let count = split.rects.len();
    let window_right = client_right(window);
    split
        .rects
        .iter()
        .enumerate()
        .map(|(index, rect)| Pane {
            x: area[0] + rect.x,
            y: area[1] + rect.y,
            width: rect.width,
            height: rect.height,
            row_y: row_top(rect.y == split.rects[0].y, view_top, area[1] + rect.y),
            active: index as i32 == active,
            first: index == 0,
            last: index + 1 == count,
            caption: if rect.y == split.rects[0].y {
                caption_over(window_right, area[0] + rect.x + rect.width)
            } else {
                0.0
            },
            tab: tabs.get(index).copied(),
        })
        .collect()
}

/// Narrows the browser's top row to the address of the focused page of a split, so the browser's
/// own address bar, with its completion and editing, sits where that page's address is. The rest
/// of every row is drawn by Rivet. `contents` are the bounds of the pages in the same coordinates.
pub unsafe fn fit_toolbar(window: HWND, toolbar: &mut [i32; 4], contents: &[i32; 4]) {
    {
        let mut all = CONTENTS.lock().unwrap_or_else(|error| error.into_inner());
        match all.iter_mut().find(|(key, _)| *key == window as isize) {
            Some(entry) => entry.1 = *contents,
            None => all.push((window as isize, *contents)),
        }
    }
    let leave = || {
        ADDRESS_ONLY.lock().unwrap_or_else(|error| error.into_inner()).retain(|(key, _)| *key != window as isize);
    };
    let splits = SPLITS.lock().unwrap_or_else(|error| error.into_inner());
    let Some((_, split)) = splits.iter().find(|(_, split)| {
        crate::features::frame_layout::hwnd_of_view(split.view as *const c_void)
            .is_some_and(|found| found as isize == window as isize)
    }) else {
        return leave();
    };
    if split.rects.len() < 2 || !in_a_line(&split.rects) || !active_is_split(window) {
        return leave();
    }
    let active = (*((split.view + bv().active_member) as *const i32)).max(0) as usize;
    let Some(pane) = split.rects.get(active) else { return leave() };
    let window_right = client_right(window);
    let on_top = pane.y == split.rects[0].y;
    let caption = if on_top { caption_over(window_right, contents[0] + pane.x + pane.width) } else { 0.0 };
    let buttons = row_buttons(pane.width as f32, active == 0, caption);
    // The browser clips its top row to the strip above the pages, so only a page whose row is that
    // strip can show the browser's address bar. The row of a lower page draws its address itself.
    let address = buttons.iter().find(|button| button.0 == RowButton::Address).filter(|_| on_top);
    let place = address_in_row(toolbar[0], contents[0] + pane.x, address.map(|(_, x, width)| (*x, *width)));
    let mut all = ADDRESS_ONLY.lock().unwrap_or_else(|error| error.into_inner());
    match all.iter_mut().find(|(key, _)| *key == window as isize) {
        Some(entry) => entry.1 = place,
        None => all.push((window as isize, place)),
    }
}

/// Where the address goes inside the browser's top row in a split, as its left edge and width. The
/// row itself keeps its full width: Rivet's rows cover it except at the focused page's address and
/// behind the caption buttons, which are the browser's, and without it the browser's own frame colour
/// shows behind those buttons.
fn address_in_row(row_left: i32, page_left: i32, address: Option<(f32, f32)>) -> (i32, i32) {
    match address {
        Some((x, width)) => (page_left - row_left + x as i32, width as i32),
        // No room for an address: a sliver of it that shows nothing.
        None => (page_left - row_left, 1),
    }
}

/// Rivet's panel look for the pages of a split: Chrome's outline goes.
pub unsafe fn install_panels() -> Result<(), String> {
    if !checked() {
        return Err("this browser build has no recorded split view".into());
    }
    let base = checked_base().ok_or("this browser build has no recorded split view")?;
    detour(
        base,
        bv().calculate_layout,
        &bv().calculate_layout_prologue,
        calculate_layout as *const () as usize,
        &ORIGINAL_LAYOUT,
    )?;
    detour(
        base,
        bv().container_layout,
        &bv().calculate_layout_prologue,
        container_layout as *const () as usize,
        &ORIGINAL_CONTAINER_LAYOUT,
    )?;
    detour(
        base,
        bv().web_contents_focused,
        &bv().web_contents_focused_prologue,
        web_contents_focused as *const () as usize,
        &ORIGINAL_FOCUSED,
    )?;
    if bv().flex_layout != crate::builds::ABSENT {
        detour(
            base,
            bv().flex_layout,
            &bv().calculate_layout_prologue,
            flex_layout as *const () as usize,
            &ORIGINAL_FLEX,
        )?;
    }
    detour(
        base,
        bv().update_border,
        &bv().update_border_prologue,
        update_border as *const () as usize,
        &ORIGINAL_BORDER,
    )
}

/// Chromium's split tabs: two tabs of the strip that the browser shows side by side.
pub(crate) struct TabModelSplit;

impl crate::browsers::SplitBackend for TabModelSplit {
    fn needs(&self, build: &Build) -> Needs {
        needs(build)
    }

    unsafe fn install(&self) -> Result<(), String> {
        install()
    }

    unsafe fn install_panels(&self) -> Result<(), String> {
        install_panels()
    }

    unsafe fn active_is_split(&self, window: HWND) -> bool {
        active_is_split(window)
    }

    unsafe fn split_pair(&self, window: HWND, first: usize, second: usize, stacked: bool) -> bool {
        split_pair(window, first, second, if stacked { LAYOUT_STACKED } else { LAYOUT_SIDE_BY_SIDE })
    }

    unsafe fn unsplit_active(&self, window: HWND) -> bool {
        unsplit_active(window)
    }

    unsafe fn panes(&self, window: HWND) -> Vec<Pane> {
        panes(window)
    }

    unsafe fn page_area(&self, window: HWND) -> Option<[i32; 4]> {
        page_area(window)
    }

    unsafe fn on_page_area(&self, area: &mut crate::hooks::page_area::PageArea) {
        on_page_area(area)
    }

    unsafe fn place_address(&self, toolbar: *mut c_void) {
        place_address(toolbar)
    }

    fn address_follows_page(&self) -> bool {
        address_follows_page()
    }
}

#[cfg(test)]
mod tests {
    use super::{address_in_row, index_vector, narrow_gap, tab_indices, SPLIT_INSETS};

    #[test]
    fn fr9_address_sits_inside_the_full_top_row_at_the_focused_page() {
        assert_eq!(address_in_row(267, 267, Some((145.0, 300.0))), (145, 300));
        assert_eq!(address_in_row(267, 854, Some((145.0, 300.0))), (732, 300));
        assert_eq!(address_in_row(267, 854, None), (587, 1));
    }

    #[test]
    fn fr9_split_outer_edges_match_the_single_page() {
        assert_eq!(SPLIT_INSETS, [0, 0, 0, 0]);
    }

    #[test]
    fn fr9_a_build_that_adds_the_active_tab_is_given_only_the_other_tab() {
        // The active tab is the first or the second of the pair, and the other one is listed alone.
        assert_eq!(tab_indices(0, 1, Some(0), true), [1]);
        assert_eq!(tab_indices(0, 1, Some(1), true), [0]);
        assert_eq!(tab_indices(2, 5, Some(5), true), [2]);
    }

    #[test]
    fn fr9_a_build_that_does_not_add_the_active_tab_is_given_both_in_strip_order() {
        assert_eq!(tab_indices(0, 1, Some(1), false), [0, 1]);
        assert_eq!(tab_indices(3, 4, None, false), [3, 4]);
    }

    #[test]
    fn fr9_the_index_vector_follows_the_builds_layout() {
        assert_eq!(index_vector(0x1000, 2, false), [0x1000, 2, 2]);
        assert_eq!(index_vector(0x1000, 1, true), [0x1000, 0x1004, 0x1004]);
        assert_eq!(index_vector(0x1000, 2, true), [0x1000, 0x1008, 0x1008]);
    }

    #[test]
    fn fr9_split_gap_is_eight_dips() {
        let (mut left, mut middle, mut right) = ([8, 0, 399, 683], [407, 0, 10, 683], [417, 0, 399, 683]);
        assert!(narrow_gap(&mut left, &mut middle, &mut right));
        assert_eq!(left[0] + left[2], 408);
        assert_eq!(middle, [408, 0, 8, 683]);
        assert_eq!(right[0], 416);
        assert_eq!(right[0] + right[2], 816);
    }

    #[test]
    fn fr9_split_gap_leaves_other_layouts_alone() {
        let (mut left, mut middle, mut right) = ([8, 0, 399, 683], [407, 0, 6, 683], [413, 0, 399, 683]);
        assert!(!narrow_gap(&mut left, &mut middle, &mut right));
        assert_eq!(middle, [407, 0, 6, 683]);
    }
}
