//! Edge insets its page from its container by a small margin and paints a border around it. A
//! Rivet window has its own panel there, so inside a Rivet window the margin is zero and the border
//! is left out, and the page sits flush in the panel like Chrome's.
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

static ORIGINAL_MARGIN: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_PAINT_BORDER: AtomicUsize = AtomicUsize::new(0);

/// `gfx::Insets` as the browser lays it out: top, left, bottom, right.
type Insets = [i32; 4];

unsafe extern "system" fn container_margin(out: *mut Insets, arg: *mut c_void) -> *mut Insets {
    let original: unsafe extern "system" fn(*mut Insets, *mut c_void) -> *mut Insets =
        std::mem::transmute_copy(&ORIGINAL_MARGIN.load(Ordering::Acquire));
    let result = original(out, arg);
    if crate::chrome_toolbar::active() {
        *out = [0; 4];
    }
    result
}

unsafe extern "system" fn paint_border(border: *mut c_void, canvas: *mut c_void, view: *mut c_void) {
    if crate::chrome_toolbar::active() {
        return;
    }
    let original: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) =
        std::mem::transmute_copy(&ORIGINAL_PAINT_BORDER.load(Ordering::Acquire));
    original(border, canvas, view)
}

/// Puts the hooks in place for a build that records them.
pub(crate) unsafe fn install(base: usize, skip: &str) -> Result<(), String> {
    let Some(page) = crate::builds::get().edge_page.as_ref() else {
        return Ok(());
    };
    if !skip.contains("pagemargin") {
        crate::chrome_toolbar::detour(
            base,
            page.container_margin,
            page.container_margin_prologue,
            container_margin as *const () as usize,
            &ORIGINAL_MARGIN,
        )?;
    }
    if !skip.contains("pageborder") {
        crate::chrome_toolbar::detour(
            base,
            page.paint_border,
            page.paint_border_prologue,
            paint_border as *const () as usize,
            &ORIGINAL_PAINT_BORDER,
        )?;
    }
    Ok(())
}
