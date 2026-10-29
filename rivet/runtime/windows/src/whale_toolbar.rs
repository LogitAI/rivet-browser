//! Whale's and Samsung Internet's toolbars carry buttons a Rivet window does not want, and its right sidebar takes a column
//! of the page. A view cannot be hidden with `SetVisible` here: the layout shows it again, and a hook
//! that refuses the call recurses until the stack is gone. Instead each of those classes answers
//! that it needs no room, draws nothing, and is left alone in every window that is not a Rivet window.
//!
//! The row that is left is laid out like Chrome's: Whale's layout puts its container 100 DIPs in, where
//! the appbar's toggle needs 32, and makes the buttons 26 DIPs wide, where Chrome's are 28. Every layout
//! ends in one `SetBoundsRect`, so that call is where the container, the buttons and the field get
//! their Chrome rectangles, for the views of Rivet windows only. The layout places the three buttons once
//! and not again while they stay as they are, so the container's own call is where they are moved.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;

use crate::builds::{HiddenViews, WhaleRow};
use crate::chrome_layout::image;

/// The functions each replaced slot held: the vtable, the slot and the function.
static HELD: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());
static ORIGINAL_SIDEBAR_MINIMUM: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
static ROW_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static ROW_INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

/// Where the container starts: the appbar's toggle sits in the space before it.
const LEADING: i32 = 32;
/// Chrome's toolbar buttons are 28 DIPs wide and 2 DIPs apart, and the first sits 6 DIPs into the container.
const BUTTON: i32 = 28;
const BUTTON_STEP: i32 = 30;
const FIRST_BUTTON: i32 = 6;
/// The address field starts here, which puts its page info icon where Chrome's is, and is 34 DIPs tall.
const FIELD_X: i32 = 107;
const FIELD_HEIGHT: i32 = 34;
/// Chrome's row is 44 DIPs: 5 above the field, 4 below it and the separator. Whale's is one shorter.
const ROW_EXTRA: i32 = 1;
/// The field and the buttons sit one DIP below the middle of what the container leaves, as Chrome's do.
const CONTENT_SHIFT: i32 = 1;

fn views() -> Option<&'static HiddenViews> {
    crate::builds::get().hidden_views.as_ref()
}

fn row() -> Option<&'static WhaleRow> {
    crate::builds::get().whale_row.as_ref()
}

/// Whether this build records the classes to hide.
pub fn supported() -> bool {
    views().is_some()
}

fn held(table: usize, slot: usize) -> Option<usize> {
    let known = HELD.lock().unwrap_or_else(|error| error.into_inner());
    known
        .iter()
        .find(|(vtable, held_slot, _)| *vtable == table && *held_slot == slot)
        .map(|(_, _, function)| *function)
}

/// Whether `view` lies in the browser view of a Rivet window.
unsafe fn in_rivet_window(view: *mut c_void) -> bool {
    let Some(views) = views() else { return false };
    let mut at = view as usize;
    for _ in 0..24 {
        if at < 0x10000 {
            return false;
        }
        // The browser's own object is recorded; its view lies a fixed distance inside it.
        if crate::hide_strip::registered(at.wrapping_sub(crate::builds::get().layout.browser_window_in_view)) {
            return true;
        }
        at = *((at + views.parent_offset) as *const usize);
    }
    false
}

type SizeOf = unsafe extern "system" fn(*mut c_void, *mut [i32; 2], usize) -> *mut c_void;

unsafe fn sized(view: *mut c_void, out: *mut [i32; 2], bounds: usize, slot: usize) -> *mut c_void {
    let table = *(view as *const usize);
    let Some(function) = held(table, slot) else { return out.cast() };
    let run: SizeOf = std::mem::transmute(function);
    let result = run(view, out, bounds);
    if in_rivet_window(view) {
        *out = [0, 0];
    }
    result
}

unsafe extern "system" fn minimum_size(view: *mut c_void, out: *mut [i32; 2], bounds: usize) -> *mut c_void {
    let Some(views) = views() else { return out.cast() };
    sized(view, out, bounds, views.slots[0])
}

unsafe extern "system" fn preferred_size(view: *mut c_void, out: *mut [i32; 2], bounds: usize) -> *mut c_void {
    let Some(views) = views() else { return out.cast() };
    sized(view, out, bounds, views.slots[1])
}

/// Whether a view of a hidden class lies above `view`, in a Rivet window.
unsafe fn below_hidden(view: *mut c_void) -> bool {
    let (Some(views), Some(image)) = (views(), image::recorded()) else { return false };
    let mut at = view as usize;
    for _ in 0..8 {
        at = *((at + views.parent_offset) as *const usize);
        if at < 0x10000 {
            return false;
        }
        let table = (*(at as *const usize)).wrapping_sub(image.base);
        if views.classes.iter().any(|(vtable, _)| *vtable == table) {
            return in_rivet_window(at as *mut c_void);
        }
    }
    false
}

unsafe fn painted_inside(view: *mut c_void, argument: *const c_void, slot: usize) {
    let table = *(view as *const usize);
    let Some(function) = held(table, slot) else { return };
    if below_hidden(view) {
        return;
    }
    let run: unsafe extern "system" fn(*mut c_void, *const c_void) = std::mem::transmute(function);
    run(view, argument)
}

unsafe extern "system" fn paint_children_inside(view: *mut c_void, info: *const c_void) {
    let Some(views) = views() else { return };
    painted_inside(view, info, views.slots[2])
}

unsafe extern "system" fn paint_inside(view: *mut c_void, canvas: *const c_void) {
    let Some(views) = views() else { return };
    painted_inside(view, canvas, views.slots[3])
}

/// A view that draws nothing paints neither itself nor its children, which are not clipped to it.
unsafe fn painted(view: *mut c_void, argument: *const c_void, slot: usize) {
    let table = *(view as *const usize);
    let Some(function) = held(table, slot) else { return };
    if in_rivet_window(view) {
        return;
    }
    let run: unsafe extern "system" fn(*mut c_void, *const c_void) = std::mem::transmute(function);
    run(view, argument)
}

unsafe extern "system" fn paint_children(view: *mut c_void, info: *const c_void) {
    let Some(views) = views() else { return };
    painted(view, info, views.slots[2])
}

unsafe extern "system" fn paint(view: *mut c_void, canvas: *const c_void) {
    let Some(views) = views() else { return };
    painted(view, canvas, views.slots[3])
}

/// The page's layout asks the sidebar for its minimum width without going through its vtable.
unsafe extern "system" fn sidebar_minimum(view: *mut c_void, out: *mut [i32; 2]) -> *mut c_void {
    let original: unsafe extern "system" fn(*mut c_void, *mut [i32; 2]) -> *mut c_void =
        std::mem::transmute(ORIGINAL_SIDEBAR_MINIMUM.load(Ordering::Acquire));
    let result = original(view, out);
    if in_rivet_window(view) {
        *out = [0, 0];
    }
    result
}

unsafe fn patch() -> Result<(), String> {
    let views = views().ok_or("this browser build has no recorded views to hide")?;
    let base = image::recorded().ok_or("no recorded browser image")?.base;
    let hooks = [
        minimum_size as *const () as usize,
        preferred_size as *const () as usize,
        paint_children as *const () as usize,
        paint as *const () as usize,
    ];
    // Every slot is read before any is replaced, so a build with other classes is left untouched.
    for (vtable, functions) in views.classes {
        for (index, slot) in views.slots.iter().enumerate() {
            let at = (base + vtable + slot * 8) as *const usize;
            if *at != base + functions[index] {
                return Err(format!("{vtable:#x} does not hold the recorded view functions"));
            }
        }
    }
    for (vtable, functions) in views.inside {
        for (index, slot) in views.slots[2..].iter().enumerate() {
            let at = (base + vtable + slot * 8) as *const usize;
            if *at != base + functions[index] {
                return Err(format!("{vtable:#x} does not hold the recorded view functions"));
            }
        }
    }
    crate::chrome_toolbar::transaction(|| {
        for (vtable, functions) in views.inside {
            for (index, slot) in views.slots[2..].iter().enumerate() {
                HELD.lock().unwrap_or_else(|error| error.into_inner()).push((
                    base + vtable,
                    *slot,
                    base + functions[index],
                ));
                let hook = [paint_children_inside as *const () as usize, paint_inside as *const () as usize][index];
                crate::chrome_toolbar::replace_slot((base + vtable + slot * 8) as *mut usize, hook)?;
            }
        }
        for (vtable, functions) in views.classes {
            for (index, slot) in views.slots.iter().enumerate() {
                HELD.lock().unwrap_or_else(|error| error.into_inner()).push((
                    base + vtable,
                    *slot,
                    base + functions[index],
                ));
                crate::chrome_toolbar::replace_slot((base + vtable + slot * 8) as *mut usize, hooks[index])?;
            }
        }
        if views.sidebar_minimum == crate::builds::ABSENT {
            return Ok(());
        }
        crate::chrome_toolbar::detour(
            base,
            views.sidebar_minimum,
            views.sidebar_minimum_prologue,
            sidebar_minimum as *const () as usize,
            &ORIGINAL_SIDEBAR_MINIMUM,
        )
    })
}

/// Puts the hooks in place once for the process. Nothing changes for a window until `hide`.
pub unsafe fn install() -> Result<(), String> {
    INSTALLED.get_or_init(|| patch()).clone()
}

/// Asks every view of a hidden class below `view` to be laid out again, which also runs the layouts
/// above it. The views were laid out before the hooks were in place.
unsafe fn invalidate(view: usize, base: usize, depth: usize, views: &HiddenViews) -> usize {
    if view < 0x10000 || depth > 24 || (*(view as *const usize)).wrapping_sub(base) > 0x1000_0000 {
        return 0;
    }
    let mut count = 0;
    if views.classes.iter().any(|(vtable, _)| *(view as *const usize) == base + vtable) {
        let layout: unsafe extern "system" fn(usize, i32) = std::mem::transmute(base + views.invalidate_layout);
        crate::hide_strip::collapse(view);
        layout(view, 0);
        count += 1;
    }
    let begin = *((view + views.children_offset) as *const usize);
    let end = *((view + views.children_offset + 8) as *const usize);
    if begin >= 0x10000 && end >= begin && (end - begin) / 8 <= 512 {
        for index in 0..(end - begin) / 8 {
            count += invalidate(*((begin + index * 8) as *const usize), base, depth + 1, views);
        }
    }
    count
}

/// Leaves the toolbar buttons and the sidebar out of a Rivet window. The window's views must already
/// be recorded as a Rivet window's.
pub unsafe fn hide(window: HWND) -> bool {
    if install().is_err() {
        return false;
    }
    // A row that cannot be placed leaves the toolbar as the browser lays it out.
    let placed = install_row();
    crate::vertical_tabs::trace(&format!("whale row: {placed:?}"));
    let (Some(views), Some(image), Some(view)) = (views(), image::recorded(), crate::chrome_tabs::browser_view(window)) else {
        return true;
    };
    if !image.has(views.invalidate_layout, views.invalidate_layout_prologue) {
        return true;
    }
    crate::vertical_tabs::trace(&format!(
        "whale views: browser view {:#x} vtable {:#x}",
        view as usize,
        (*(view as *const usize)).wrapping_sub(image.base)
    ));
    let root = view as usize + crate::builds::get().layout.browser_window_in_view;
    let count = invalidate(root, image.base, 0, views);
    crate::vertical_tabs::trace(&format!(
        "whale views: browser view {:#x} vtable {:#x}, {count} laid out again",
        view as usize,
        (*(view as *const usize)).wrapping_sub(image.base)
    ));
    true
}

/// Whether `parent` is the root view of a Rivet window's browser view, where the toolbar and the page lie.
unsafe fn is_root(row: &WhaleRow, parent: usize) -> bool {
    parent > 0x10000 && crate::hide_strip::registered(parent.wrapping_sub(crate::builds::get().layout.browser_window_in_view))
        && row.parent_offset != 0
}

/// The rectangle a view of the row gets in a Rivet window, or `None` to keep the browser's.
/// `rect` is x, y, width and height in the parent's coordinates.
unsafe fn placed(row: &WhaleRow, view: *mut c_void, rect: &[i32; 4]) -> Option<[i32; 4]> {
    let base = image::recorded()?.base;
    let class = (*(view as *const usize)).wrapping_sub(base);
    let parent = *((view as usize + row.parent_offset) as *const usize);
    // Chrome's row is one DIP taller than Whale's, and everything the browser lays out below it moves down.
    if class == row.toolbar && rect[3] == row.toolbar_height {
        return is_root(row, parent).then_some([rect[0], rect[1], rect[2], rect[3] + ROW_EXTRA]);
    }
    if class == row.separator && rect[1] == row.toolbar_height - 1 && rect[3] == 1 {
        // The line moves down with the row and runs the whole width, as Chrome's does.
        if parent < 0x10000 || !in_rivet_window(view) {
            return None;
        }
        let width = (*((parent + row.bounds_offset) as *const [i32; 4]))[2];
        return (width >= rect[2]).then_some([0, rect[1] + ROW_EXTRA, width, rect[3]]);
    }
    if rect[1] == row.toolbar_height && rect[3] > ROW_EXTRA {
        return is_root(row, parent).then_some([rect[0], rect[1] + ROW_EXTRA, rect[2], rect[3] - ROW_EXTRA]);
    }
    if class != row.container && class != row.field {
        return None;
    }
    if !in_rivet_window(view) || parent < 0x10000 {
        return None;
    }
    let parent_bounds = *((parent + row.bounds_offset) as *const [i32; 4]);
    if class == row.container {
        // The right edge stays where the browser put it, which is short of the caption buttons, unless
        // the buttons are moved: then the container reaches the room they leave.
        let right = if crate::whale_caption::active() {
            parent_bounds[2] - crate::whale_caption::RESERVE
        } else {
            rect[0] + rect[2]
        };
        if rect[0] <= LEADING || right > parent_bounds[2] || right - LEADING <= 0 {
            return None;
        }
        return Some([LEADING, rect[1], right - LEADING, rect[3]]);
    }
    if (*(parent as *const usize)).wrapping_sub(base) != row.container {
        return None;
    }
    let right = rect[0] + rect[2];
    if right - FIELD_X <= 0 {
        return None;
    }
    Some([FIELD_X, (parent_bounds[3] - FIELD_HEIGHT) / 2 + CONTENT_SHIFT, right - FIELD_X, FIELD_HEIGHT])
}

type SetBounds = unsafe extern "system" fn(*mut c_void, *const [i32; 4]);

/// Moves the back, forward and reload buttons of a container, which the layout sets once and then leaves.
unsafe fn place_buttons(row: &WhaleRow, container: *mut c_void, height: i32, set: SetBounds) {
    let Some(image) = image::recorded() else { return };
    let begin = *((container as usize + row.children_offset) as *const usize);
    let end = *((container as usize + row.children_offset + 8) as *const usize);
    if begin < 0x10000 || end < begin || (end - begin) / 8 > 512 {
        return;
    }
    for index in 0..(end - begin) / 8 {
        let child = *((begin + index * 8) as *const usize);
        if child < 0x10000 {
            continue;
        }
        let class = (*(child as *const usize)).wrapping_sub(image.base);
        if let Some(position) = row.buttons.iter().position(|known| *known == class) {
            let wanted = [FIRST_BUTTON + position as i32 * BUTTON_STEP, (height - BUTTON) / 2 + CONTENT_SHIFT, BUTTON, BUTTON];
            set(child as *mut c_void, &wanted);
        }
    }
}

unsafe extern "system" fn set_bounds_rect(view: *mut c_void, rect: *const [i32; 4]) {
    let original: SetBounds = std::mem::transmute(ROW_ORIGINAL.load(Ordering::Acquire));
    let Some(row) = row() else { return original(view, rect) };
    match placed(row, view, &*rect) {
        Some(moved) => {
            original(view, &moved);
            let base = image::recorded().map_or(0, |image| image.base);
            if (*(view as *const usize)).wrapping_sub(base) == row.container {
                place_buttons(row, view, moved[3], original);
            }
        }
        None => original(view, rect),
    }
}

/// Puts the row hook in place once for the process. Nothing changes for a window until `hide`.
unsafe fn install_row() -> Result<(), String> {
    ROW_INSTALLED
        .get_or_init(|| {
            let row = row().ok_or("this browser build has no recorded top row")?;
            let image = image::recorded().ok_or("no recorded browser image")?;
            // Each class must hold functions of this image, so a build with other ones is left alone.
            for vtable in [row.container, row.field, row.toolbar, row.separator].into_iter().chain(row.buttons) {
                let first = *((image.base + vtable) as *const usize);
                if first < image.base || first - image.base >= 0x1000_0000 {
                    return Err(format!("{vtable:#x} is not a recorded vtable"));
                }
            }
            crate::chrome_toolbar::transaction(|| {
                crate::chrome_toolbar::detour(
                    image.base,
                    row.set_bounds_rect,
                    row.set_bounds_rect_prologue,
                    set_bounds_rect as *const () as usize,
                    &ROW_ORIGINAL,
                )
            })
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_whale_row_geometry_is_chromes() {
        // The toggle's space, then 28 DIP buttons two apart, like the Chrome row.
        assert_eq!(LEADING, 32);
        assert_eq!((BUTTON, BUTTON_STEP - BUTTON), (28, 2));
        // Back, forward and reload fit before the address field with room left for it.
        assert!(FIRST_BUTTON + 2 * BUTTON_STEP + BUTTON < FIELD_X);
        // The 34 DIP field and the 28 DIP buttons both fit the 42 DIP container Whale lays out, one DIP
        // lower than centred, and the row Chrome has is one DIP taller than Whale's.
        assert!(FIELD_HEIGHT + CONTENT_SHIFT <= 42 && BUTTON <= FIELD_HEIGHT);
        assert_eq!((ROW_EXTRA, CONTENT_SHIFT), (1, 1));
    }
}
