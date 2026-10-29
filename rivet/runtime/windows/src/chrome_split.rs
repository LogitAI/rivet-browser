//! Chrome 156 split view on the browser UI thread. Two tabs shown side by side in one window are
//! Chrome's own split tabs, so Rivet starts and ends them through the tab strip model and lets the
//! browser lay out, focus and restore them. Calls are gated by code signatures.
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use crate::chrome_layout::Rect;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};

use crate::chrome_tabs::{base, function, tab_model};

const ADD_TO_NEW_SPLIT: usize = 0x0A20_5410;
const REMOVE_SPLIT: usize = 0x0A20_7580;
const GET_SPLIT_FOR_TAB: usize = 0x0311_2370;
/// PartitionAlloc's operator new, which frees the index vector the callee takes over.
const ALLOCATE: usize = 0x05d3_d6a0;

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
    static CHECK: OnceLock<bool> = OnceLock::new();
    let base = base()?;
    let signatures: &[(usize, &[u8])] = &[
        (
            ADD_TO_NEW_SPLIT,
            &[0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53],
        ),
        (REMOVE_SPLIT, &[0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x58]),
        (
            GET_SPLIT_FOR_TAB,
            &[0x56, 0x48, 0x83, 0xec, 0x20, 0x45, 0x85, 0xc0, 0x78],
        ),
        (ALLOCATE, &[0x40, 0x53, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xd9]),
    ];
    let ok = *CHECK.get_or_init(|| {
        signatures.iter().all(|(rva, bytes)| unsafe {
            std::slice::from_raw_parts((base + rva) as *const u8, bytes.len()) == *bytes
        })
    });
    if !ok {
        crate::vertical_tabs::trace("split: signature mismatch");
    }
    ok.then_some(base)
}

/// The id of the split the tab at `index` belongs to.
unsafe fn split_of(base: usize, model: *mut c_void, index: usize) -> Option<[u8; 16]> {
    let mut optional = [0u8; 24];
    function::<GetSplitForTab>(base, GET_SPLIT_FOR_TAB)(model, &mut optional, index as i32);
    (optional[16] != 0).then(|| {
        let mut id = [0; 16];
        id.copy_from_slice(&optional[..16]);
        id
    })
}

/// Whether the active tab of the window is shown in a split.
pub unsafe fn active_is_split(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    tabs.iter()
        .find(|tab| tab.active)
        .is_some_and(|tab| split_of(base, model, tab.index).is_some())
}

/// Splits the active tab with the tab after it, or before it when it is the last one. Returns false
/// when the window has a single tab or the active tab is already split.
pub unsafe fn split_active(window: HWND) -> bool {
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    let other = if active + 1 < tabs.len() {
        active + 1
    } else if active > 0 {
        active - 1
    } else {
        return false;
    };
    split_pair(window, active.min(other), active.max(other), LAYOUT_SIDE_BY_SIDE)
}

/// The side of a page a tab is dropped on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Splits the active tab with `dragged`, which takes `side` of it. Returns false when the tab is
/// the active one, does not exist, or either tab is already in a split.
pub unsafe fn split_beside(window: HWND, dragged: usize, side: Side) -> bool {
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    if dragged == active || dragged >= tabs.len() {
        return false;
    }
    let (first, second) = match side {
        Side::Left | Side::Top => (dragged, active),
        Side::Right | Side::Bottom => (active, dragged),
    };
    let layout = match side {
        Side::Left | Side::Right => LAYOUT_SIDE_BY_SIDE,
        Side::Top | Side::Bottom => LAYOUT_STACKED,
    };
    split_pair(window, first, second, layout)
}

/// The part of a page near each edge where a drop splits it, as a share of its size.
const EDGE_SHARE: f32 = 0.28;

/// Where a tab dropped at (`x`, `y`) on a page of `page` (x, y, width, height) lands, and the
/// rectangle that shows it. A drop near an edge splits the page there and shows that half. A drop
/// elsewhere, or on a window that is already split, opens the tab in the page.
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

/// Makes a split of two tabs. The first one is shown first, which is on the left or on top.
unsafe fn split_pair(window: HWND, first: usize, second: usize, layout: u32) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    if split_of(base, model, first).is_some() || split_of(base, model, second).is_some() {
        return false;
    }
    let indices = function::<Allocate>(base, ALLOCATE)(8);
    if indices.is_null() {
        return false;
    }
    indices.write(first as i32);
    indices.add(1).write(second as i32);
    // Chrome takes over this vector of a pointer, an element count and a capacity.
    let span = [indices as usize, 2, 2];
    let mut visual = [0u8; 16];
    visual[..4].copy_from_slice(&layout.to_le_bytes());
    visual[8..16].copy_from_slice(&EVEN_RATIO.to_le_bytes());
    let mut id = [0u8; 16];
    function::<AddToNewSplit>(base, ADD_TO_NEW_SPLIT)(
        model,
        &mut id,
        &span,
        &visual,
        CREATED_SOURCE,
    );
    crate::vertical_tabs::trace(&format!("split: created {id:02x?} of {first} and {second}"));
    true
}

/// Ends the split of the active tab. Both tabs stay open.
pub unsafe fn unsplit_active(window: HWND) -> bool {
    let Some(base) = checked_base() else { return false };
    let Some(model) = tab_model(base, window) else { return false };
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return false };
    let Some(active) = tabs.iter().find(|tab| tab.active).map(|tab| tab.index) else {
        return false;
    };
    let Some(id) = split_of(base, model, active) else { return false };
    function::<RemoveSplit>(base, REMOVE_SPLIT)(model, &id);
    true
}

/// One-shot diagnostic for the isolated test profile: opens a second tab and splits.
pub unsafe fn probe(window: HWND) {
    if crate::chrome_tabs::tabs(window).is_some_and(|tabs| tabs.len() == 1) {
        crate::chrome_tabs::new_tab(window);
    }
    let split = split_active(window);
    crate::vertical_tabs::trace(&format!(
        "split probe: split {split} active_is_split {}",
        active_is_split(window)
    ));
    if unsafe { crate::frame::runtime_directory() }.is_some_and(|dir| dir.join("rivet-unsplit.flag").exists()) {
        let ended = toggle_split(window);
        let count = crate::chrome_tabs::tabs(window).map(|tabs| tabs.len());
        crate::vertical_tabs::trace(&format!(
            "split probe: unsplit {ended} active_is_split {} tabs {count:?}",
            active_is_split(window)
        ));
    }
}

/// The corner radius of a page panel in a split, in device-independent pixels.
const PANEL_RADIUS: f32 = 12.0;

/// `ContentsContainerView::SetRoundedCorners(this, const RoundedCornersF*)`. It starts with
/// `movss xmm0, [rdx]; ucomiss xmm0, [rcx + 0x484]; jne changed`, 13 bytes.
const SET_ROUNDED_CORNERS: usize = 0x0369_EB40;
const SET_ROUNDED_CORNERS_PROLOGUE: [u8; 13] = [
    0xf3, 0x0f, 0x10, 0x02, 0x0f, 0x2e, 0x81, 0x84, 0x04, 0x00, 0x00, 0x75, 0x33,
];
/// Where the `jne` goes when the corners differ, and the `jp` after it.
const SET_ROUNDED_CORNERS_CHANGED: usize = 0x0369_EB80;
const SET_ROUNDED_CORNERS_PARITY: usize = 0x0369_EB4D;

type SetRoundedCorners = unsafe extern "system" fn(*mut c_void, *const [f32; 4]);
static ORIGINAL_CORNERS: AtomicUsize = AtomicUsize::new(0);

/// Pages of Rivet windows are panels with the shell study's radius. Other windows keep Chrome's.
unsafe extern "system" fn set_rounded_corners(view: *mut c_void, corners: *const [f32; 4]) {
    let original: SetRoundedCorners = std::mem::transmute(ORIGINAL_CORNERS.load(Ordering::Acquire));
    let widget = crate::chrome_toolbar::widget_holding(view);
    if crate::chrome_toolbar::is_rivet(widget) {
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
    let base = checked_base().ok_or("this browser build has no recorded split view")?;
    let code = (base + SET_ROUNDED_CORNERS) as *mut u8;
    if std::slice::from_raw_parts(code, 13) != SET_ROUNDED_CORNERS_PROLOGUE {
        return Err("the rounded corners function is not the recorded one".into());
    }
    if ORIGINAL_CORNERS.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    // The comparison runs in the trampoline. Its `jne` has a short offset, so it is rebuilt: taken
    // goes to the original's changed path, and not taken goes on to the original's `jp`.
    let mut trampoline = SET_ROUNDED_CORNERS_PROLOGUE[..11].to_vec();
    trampoline.extend_from_slice(&[0x75, 0x0E]);
    absolute_jump(&mut trampoline, base + SET_ROUNDED_CORNERS_PARITY);
    absolute_jump(&mut trampoline, base + SET_ROUNDED_CORNERS_CHANGED);
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

/// `ContentsContainerView::UpdateBorderAndOverlay(this, show, a, b)`. Chrome rings the page it
/// works in with a 1 DIP outline and keeps 4 DIPs of space inside the ring. A Rivet panel has no
/// ring, so the page takes that space and the panel's depth marks it instead.
const UPDATE_BORDER: usize = 0x01FD_C720;
const UPDATE_BORDER_PROLOGUE: [u8; 12] = [
    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x48, 0x48, 0x89, 0xce,
];
type UpdateBorder = unsafe extern "system" fn(*mut c_void, u32, u32, u32);
static ORIGINAL_BORDER: AtomicUsize = AtomicUsize::new(0);

/// `views::View::SetBorder` and `SetVisible` in the view's vtable.
const SET_BORDER_SLOT: usize = 0xf0 / 8;
const SET_VISIBLE_SLOT: usize = 0x90 / 8;
/// The member of the container that holds Chrome's outline view.
const OUTLINE_MEMBER: usize = 0x448;

unsafe fn virtual_slot<T: Copy>(view: *mut c_void, slot: usize) -> T {
    let table = *(view as *const *const usize);
    std::mem::transmute_copy(&*table.add(slot))
}

unsafe extern "system" fn update_border(view: *mut c_void, show: u32, a: u32, b: u32) {
    let original: UpdateBorder = std::mem::transmute(ORIGINAL_BORDER.load(Ordering::Acquire));
    original(view, show, a, b);
    let widget = crate::chrome_toolbar::widget_holding(view);
    if !crate::chrome_toolbar::is_rivet(widget) {
        return;
    }
    // Chrome has just laid its outline and the space inside it out. The mini toolbar stays.
    let no_border: usize = 0;
    virtual_slot::<unsafe extern "system" fn(*mut c_void, *const usize)>(view, SET_BORDER_SLOT)(
        view,
        &no_border,
    );
    let child = *((view as usize + OUTLINE_MEMBER) as *const *mut c_void);
    if !child.is_null() {
        virtual_slot::<unsafe extern "system" fn(*mut c_void, u32)>(child, SET_VISIBLE_SLOT)(
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
    let code = (base + rva) as *mut u8;
    if std::slice::from_raw_parts(code, prologue.len()) != prologue {
        return Err(format!("{rva:#x} is not the recorded function"));
    }
    if store.load(Ordering::Acquire) != 0 {
        return Ok(());
    }
    let mut trampoline = prologue.to_vec();
    absolute_jump(&mut trampoline, base + rva + prologue.len());
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
    store.store(memory as usize, Ordering::Release);
    let mut patch = vec![0x48, 0xB8];
    patch.extend_from_slice(&(hook as u64).to_le_bytes());
    patch.extend_from_slice(&[0xFF, 0xE0]);
    patch.resize(prologue.len(), 0xCC);
    write_code(code, &patch)
}

/// `MultiContentsView::CalculateProposedLayout`, a method of the layout delegate that sits
/// `LAYOUT_DELEGATE_OFFSET` bytes into the view.
const CALCULATE_LAYOUT: usize = 0x0232_E330;
const CALCULATE_LAYOUT_PROLOGUE: [u8; 12] =
    [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53];
const LAYOUT_DELEGATE_OFFSET: usize = 0x328;
/// The insets Chrome keeps around the pages of a split, as top, left, bottom and right.
const SPLIT_INSETS_MEMBER: usize = 0x404;
/// The window's panel already leaves its gap on the right and the bottom, so only the left edge
/// keeps room beside the Sidebar.
const SPLIT_INSETS: [i32; 4] = [0, 8, 0, 0];
/// The width of the resize area between two pages, and the gap the shell study wants.
const CHROME_GAP: i32 = 10;
const PANEL_GAP: i32 = 8;
/// The container vector, its length and the active index of a `MultiContentsView`.
const CONTAINERS_MEMBER: usize = 0x350;
const CONTAINER_COUNT_MEMBER: usize = 0x358;
const ACTIVE_MEMBER: usize = 0x3c8;
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
    let view = (delegate as usize - LAYOUT_DELEGATE_OFFSET) as *mut c_void;
    let widget = crate::chrome_toolbar::widget_holding(view);
    if !crate::chrome_toolbar::is_rivet(widget) {
        return original(delegate, out, bounds);
    }
    *((view as usize + SPLIT_INSETS_MEMBER) as *mut [i32; 4]) = SPLIT_INSETS;
    let result = original(delegate, out, bounds);
    let (data, count) = layout_children(out);
    let shown: Vec<*mut u8> = (0..count)
        .map(|index| data.add(index * 0x30))
        .filter(|child| *child.add(8) != 0)
        .collect();
    // The pages are the visible entries that are the view's contents containers.
    let containers = *((view as usize + CONTAINERS_MEMBER) as *const *const usize);
    let total = *((view as usize + CONTAINER_COUNT_MEMBER) as *const usize);
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
    splits.push((widget, SplitPanes { view: view as usize, gap, rects }));
    result
}

/// `views::FlexLayout::CalculateProposedLayout`, which lays out the children of the browser's top
/// row. It is a method of a layout manager that keeps its host view at +0x38.
const FLEX_LAYOUT: usize = 0x054F_CC60;
const FLEX_LAYOUT_HOST: usize = 0x38;
static ORIGINAL_FLEX: AtomicUsize = AtomicUsize::new(0);
/// The primary vtable of `LocationBarView`, which tells the top row's container among all hosts.
const LOCATION_BAR_VTABLE: usize = 0x0FE8_6BE8;
/// The size of a view is two ints at +0x1d0.
const VIEW_SIZE: usize = 0x1d0;

/// The windows whose top row currently only holds the address of the focused page.
static ADDRESS_ONLY: Mutex<Vec<isize>> = Mutex::new(Vec::new());

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
        view != 0 && *(view as *const usize) == base + LOCATION_BAR_VTABLE
    });
    let Some(location) = location else { return result };
    let host = *((layout as usize + FLEX_LAYOUT_HOST) as *const usize);
    let in_split = crate::chrome_layout::hwnd_of_view(host as *const c_void).is_some_and(|window| {
        ADDRESS_ONLY.lock().unwrap_or_else(|error| error.into_inner()).contains(&(window as isize))
    });
    if !in_split {
        return result;
    }
    let width = *((host + VIEW_SIZE) as *const i32);
    for index in 0..count {
        let child = data.add(index * 0x30);
        if child != location {
            *child.add(8) = 0;
        }
    }
    let rect = &mut *child_bounds(location);
    rect[0] = 0;
    rect[2] = width;
    result
}

/// `views::View::SchedulePaint(this)`. The background only paints again when it is told to, so a
/// change of the pages or of the focused one must tell it.
const SCHEDULE_PAINT: usize = 0x054A_B3B0;
type SchedulePaint = unsafe extern "system" fn(*mut c_void);

unsafe fn repaint(view: usize) {
    if view == 0 {
        return;
    }
    let Some(base) = base() else { return };
    let code = std::slice::from_raw_parts((base + SCHEDULE_PAINT) as *const u8, 6);
    if code == [0x56, 0x48, 0x83, 0xec, 0x40, 0x48] {
        function::<SchedulePaint>(base, SCHEDULE_PAINT)(view as *mut c_void);
    }
}

/// `MultiContentsView::OnWebContentsFocused(this, web view)`, which changes the focused page.
const WEB_CONTENTS_FOCUSED: usize = 0x0A4C_B140;
const WEB_CONTENTS_FOCUSED_PROLOGUE: [u8; 13] =
    [0x56, 0x57, 0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x81, 0x98, 0x03, 0x00, 0x00];
type FocusedHook = unsafe extern "system" fn(*mut c_void, *mut c_void);
static ORIGINAL_FOCUSED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn web_contents_focused(view: *mut c_void, web_view: *mut c_void) {
    let original: FocusedHook = std::mem::transmute(ORIGINAL_FOCUSED.load(Ordering::Acquire));
    original(view, web_view);
    let widget = crate::chrome_toolbar::widget_holding(view);
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

const CONTAINER_LAYOUT: usize = 0x01FF_5270;
const CONTAINER_DELEGATE_OFFSET: usize = 0x320;
/// The member of a contents container that holds its mini toolbar, the chip with the page's address
/// that Chrome floats over the lower corner of a page. Rivet's rows show the address, so it stays hidden.
const MINI_TOOLBAR_MEMBER: usize = 0x440;
static ORIGINAL_CONTAINER_LAYOUT: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn container_layout(
    delegate: *mut c_void,
    out: *mut u8,
    bounds: *const c_void,
) -> *mut u8 {
    let original: CalculateLayout = std::mem::transmute(ORIGINAL_CONTAINER_LAYOUT.load(Ordering::Acquire));
    let result = original(delegate, out, bounds);
    let view = (delegate as usize - CONTAINER_DELEGATE_OFFSET) as *mut c_void;
    if crate::chrome_toolbar::is_rivet(crate::chrome_toolbar::widget_holding(view)) {
        let toolbar = *((view as usize + MINI_TOOLBAR_MEMBER) as *const usize);
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

/// Asks the window to lay the browser out again once the current layout is done, because the top
/// row follows the pages of a split.
pub const RELAYOUT_MESSAGE: &str = "Rivet.SplitLayout";

unsafe fn request_relayout(view: usize) {
    let Some(window) = crate::chrome_layout::hwnd_of_view(view as *const c_void) else { return };
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
    let geometry = crate::chrome_side_panel::page_geometry(window)?;
    let contents = CONTENTS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| *key == window as isize)
        .map(|(_, bounds)| *bounds)?;
    Some([contents[0], geometry.top, contents[0] + contents[2], geometry.bottom])
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
    let mut left = vec![RowButton::Grip, RowButton::Back, RowButton::Forward, RowButton::Reload];
    let mut right = vec![RowButton::Popout, RowButton::Unsplit];
    let needed = |left: &[RowButton], right: &[RowButton]| {
        let pitch = BUTTON + BUTTON_GAP;
        pitch * left.len() as f32 + ADDRESS_LEAD + ADDRESS_MIN + ADDRESS_TRAIL + pitch * right.len() as f32
            - BUTTON_GAP
    };
    for drop in [RowButton::Popout, RowButton::Forward, RowButton::Grip, RowButton::Reload, RowButton::Back] {
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
    let right_start = end - right.len() as f32 * BUTTON - (right.len() as f32 - 1.0).max(0.0) * BUTTON_GAP;
    let address = x + ADDRESS_LEAD;
    let address_width = right_start - ADDRESS_TRAIL - address;
    if address_width >= ADDRESS_MIN {
        buttons.push((RowButton::Address, address, address_width));
    }
    for (index, button) in right.iter().enumerate() {
        buttons.push((*button, right_start + index as f32 * (BUTTON + BUTTON_GAP), BUTTON));
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
    pub active: bool,
    pub first: bool,
    pub last: bool,
    /// How much of the row's right end the caption buttons cover. The buttons sit at the window's top
    /// right, so a page that ends before the Sidebar's rail is covered by less than their full width.
    pub caption: f32,
    /// The strip index of the tab this page shows.
    pub tab: usize,
}

/// Whether the pages share one row, as opposed to being stacked.
fn side_by_side(rects: &[Rect]) -> bool {
    rects.windows(2).all(|pair| pair[0].y == pair[1].y)
}

/// Whether the pages are laid out as a row or as a column, which are the layouts Rivet draws rows
/// for.
fn in_a_line(rects: &[Rect]) -> bool {
    side_by_side(rects) || rects.windows(2).all(|pair| pair[0].x == pair[1].x)
}

/// The tabs of the split the active tab is in, in strip order, which is the order of its pages.
unsafe fn split_tabs(window: HWND) -> Vec<usize> {
    let Some(base) = checked_base() else { return Vec::new() };
    let Some(model) = tab_model(base, window) else { return Vec::new() };
    let Some(tabs) = crate::chrome_tabs::tabs(window) else { return Vec::new() };
    let Some(active) = tabs.iter().find(|tab| tab.active) else { return Vec::new() };
    let Some(id) = split_of(base, model, active.index) else { return Vec::new() };
    tabs.iter()
        .filter(|tab| split_of(base, model, tab.index) == Some(id))
        .map(|tab| tab.index)
        .collect()
}

/// The right edge of the window's client area in device-independent pixels.
unsafe fn client_right(window: HWND) -> i32 {
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

/// The pages of the window's split, or none when the active tab is not in one.
pub unsafe fn panes(window: HWND) -> Vec<Pane> {
    let Some(area) = page_area(window) else { return Vec::new() };
    let tabs = split_tabs(window);
    if tabs.len() < 2 {
        return Vec::new();
    }
    let splits = SPLITS.lock().unwrap_or_else(|error| error.into_inner());
    let Some((_, split)) = splits.iter().find(|(_, split)| {
        crate::chrome_layout::hwnd_of_view(split.view as *const c_void)
            .is_some_and(|found| found as isize == window as isize)
    }) else {
        return Vec::new();
    };
    if !in_a_line(&split.rects) {
        return Vec::new();
    }
    let active = *((split.view + ACTIVE_MEMBER) as *const i32);
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
            active: index as i32 == active,
            first: index == 0,
            last: index + 1 == count,
            caption: if rect.y == split.rects[0].y {
                caption_over(window_right, area[0] + rect.x + rect.width)
            } else {
                0.0
            },
            tab: tabs.get(index).copied().unwrap_or(0),
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
        ADDRESS_ONLY.lock().unwrap_or_else(|error| error.into_inner()).retain(|key| *key != window as isize);
    };
    let splits = SPLITS.lock().unwrap_or_else(|error| error.into_inner());
    let Some((_, split)) = splits.iter().find(|(_, split)| {
        crate::chrome_layout::hwnd_of_view(split.view as *const c_void)
            .is_some_and(|found| found as isize == window as isize)
    }) else {
        return leave();
    };
    if split.rects.len() < 2 || !in_a_line(&split.rects) || !active_is_split(window) {
        return leave();
    }
    let active = (*((split.view + ACTIVE_MEMBER) as *const i32)).max(0) as usize;
    let Some(pane) = split.rects.get(active) else { return leave() };
    let window_right = client_right(window);
    let on_top = pane.y == split.rects[0].y;
    let caption = if on_top { caption_over(window_right, contents[0] + pane.x + pane.width) } else { 0.0 };
    let buttons = row_buttons(pane.width as f32, active == 0, caption);
    {
        let mut all = ADDRESS_ONLY.lock().unwrap_or_else(|error| error.into_inner());
        if !all.contains(&(window as isize)) {
            all.push(window as isize);
        }
    }
    // The browser clips its top row to the strip above the pages, so only a page whose row is that
    // strip can show the browser's address bar. The row of a lower page draws its address itself.
    let address = buttons.iter().find(|button| button.0 == RowButton::Address).filter(|_| on_top);
    match address {
        Some((_, x, width)) => {
            toolbar[0] = contents[0] + pane.x + *x as i32;
            toolbar[2] = *width as i32;
        }
        // No room for an address: the top row keeps a sliver that shows nothing.
        None => {
            toolbar[0] = contents[0] + pane.x;
            toolbar[2] = 1;
        }
    }
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

/// Rivet's panel look for the pages of a split: Chrome's outline goes.
pub unsafe fn install_panels() -> Result<(), String> {
    let base = checked_base().ok_or("this browser build has no recorded split view")?;
    detour(
        base,
        CALCULATE_LAYOUT,
        &CALCULATE_LAYOUT_PROLOGUE,
        calculate_layout as *const () as usize,
        &ORIGINAL_LAYOUT,
    )?;
    detour(
        base,
        CONTAINER_LAYOUT,
        &CALCULATE_LAYOUT_PROLOGUE,
        container_layout as *const () as usize,
        &ORIGINAL_CONTAINER_LAYOUT,
    )?;
    detour(
        base,
        WEB_CONTENTS_FOCUSED,
        &WEB_CONTENTS_FOCUSED_PROLOGUE,
        web_contents_focused as *const () as usize,
        &ORIGINAL_FOCUSED,
    )?;
    detour(
        base,
        FLEX_LAYOUT,
        &CALCULATE_LAYOUT_PROLOGUE,
        flex_layout as *const () as usize,
        &ORIGINAL_FLEX,
    )?;
    detour(
        base,
        UPDATE_BORDER,
        &UPDATE_BORDER_PROLOGUE,
        update_border as *const () as usize,
        &ORIGINAL_BORDER,
    )
}


#[cfg(test)]
mod tests {
    use super::{caption_over, drop_zone, narrow_gap, row_buttons, RowButton, Side, CAPTION_ROOM, TOGGLE_ROOM};

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

    #[test]
    fn fr9_tab_dropped_on_an_edge_splits_that_side() {
        let page = [100.0, 50.0, 800.0, 600.0];
        let (side, rect) = drop_zone(page, 880.0, 300.0, false, 8.0);
        assert_eq!(side, Some(Side::Right));
        assert_eq!(rect, [100.0 + 396.0 + 8.0, 50.0, 396.0, 600.0]);
        assert_eq!(drop_zone(page, 120.0, 300.0, false, 8.0).0, Some(Side::Left));
        assert_eq!(drop_zone(page, 500.0, 60.0, false, 8.0).0, Some(Side::Top));
        assert_eq!(drop_zone(page, 500.0, 640.0, false, 8.0).0, Some(Side::Bottom));
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
                assert!(edge <= limit, "row runs under the caption at {width}: {buttons:?}");
                assert!(buttons.iter().any(|b| b.0 == RowButton::Unsplit));
            }
        }
    }

    #[test]
    fn fr9_narrow_row_drops_the_mini_window_button_before_the_address() {
        let names: Vec<RowButton> = row_buttons(303.0, false, CAPTION_ROOM).iter().map(|b| b.0).collect();
        assert!(!names.contains(&RowButton::Popout));
        assert!(names.contains(&RowButton::Address) && names.contains(&RowButton::Unsplit));
    }

    #[test]
    fn fr9_caption_covers_only_what_the_pages_edge_leaves_of_it() {
        assert_eq!(caption_over(1000, 1000), CAPTION_ROOM);
        assert_eq!(caption_over(1000, 900), CAPTION_ROOM - 100.0);
        assert_eq!(caption_over(1000, 800), 0.0);
    }
}
