//! Whale's toolbar carries buttons a Rivet window does not want, and its right sidebar takes a column
//! of the page. A view cannot be hidden with `SetVisible` here: the layout shows it again, and a hook
//! that refuses the call recurses until the stack is gone. Instead each of those classes answers
//! that it needs no room, draws nothing, and is left alone in every window that is not a Rivet window.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;

use crate::builds::HiddenViews;
use crate::chrome_layout::image;

/// The functions each replaced slot held: the vtable, the slot and the function.
static HELD: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());
static ORIGINAL_SIDEBAR_MINIMUM: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

fn views() -> Option<&'static HiddenViews> {
    crate::builds::get().hidden_views.as_ref()
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
    crate::chrome_toolbar::transaction(|| {
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
