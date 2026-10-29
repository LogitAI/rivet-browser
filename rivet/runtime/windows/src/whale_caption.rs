//! Whale's caption buttons are children of its frame view, laid out in the strip along the window's top
//! edge, 32 DIPs tall, with a tab search chevron before them. A Rivet window's top row is taller and sits
//! inside the panel's gap, so after the frame lays out, the buttons are moved to where Chrome's caption
//! container puts them and the chevron is left out. Rivet's own tab list replaces it.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::builds::WhaleRow;
use crate::chrome_layout::{image, CaptionHeight, PANEL_GAP};

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static HEIGHT: OnceLock<CaptionHeight> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// What the toolbar keeps free along its right edge once the buttons are in place: the three buttons,
/// 46 DIPs each, and the panel's gap before them.
pub const RESERVE: i32 = 3 * 46 + PANEL_GAP;

fn row() -> Option<&'static WhaleRow> {
    crate::builds::get().whale_row.as_ref()
}

/// Whether the caption buttons of Rivet windows are placed, which lets the toolbar reach them.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// Whether this build records the frame's layout.
pub fn supported() -> bool {
    row().is_some_and(|row| row.frame_layout != crate::builds::ABSENT)
}

/// The rectangle a caption button gets, or `None` to keep the one the frame gave it.
fn caption_rect(bounds: [i32; 4], height: i32) -> Option<[i32; 4]> {
    if bounds[2] <= 0 || bounds[3] <= 0 {
        // A button that has no room, such as the restore button of a window that is not maximized.
        return None;
    }
    if bounds[1] == PANEL_GAP && bounds[3] == height {
        // Already where the panel has it, as left by an earlier layout the frame did not redo.
        return None;
    }
    Some([bounds[0] - PANEL_GAP, PANEL_GAP, bounds[2], height.max(0)])
}

unsafe extern "system" fn frame_layout(view: *mut c_void) {
    let original: unsafe extern "system" fn(*mut c_void) = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
    original(view);
    let (Some(row), Some(image), Some(height)) = (row(), image::recorded(), HEIGHT.get()) else { return };
    let Some(height) = crate::chrome_layout::hwnd_of_view(view).and_then(|window| height(window)) else { return };
    let begin = *((view as usize + row.children_offset) as *const usize);
    let end = *((view as usize + row.children_offset + 8) as *const usize);
    if begin < 0x10000 || end < begin || (end - begin) / 8 > 64 {
        return;
    }
    let set_bounds: unsafe extern "system" fn(*mut c_void, *const [i32; 4]) =
        std::mem::transmute(image.base + row.set_bounds_rect);
    for index in 0..(end - begin) / 8 {
        let child = *((begin + index * 8) as *const usize);
        if child < 0x10000 {
            continue;
        }
        let class = (*(child as *const usize)).wrapping_sub(image.base);
        let bounds = *((child + row.bounds_offset) as *const [i32; 4]);
        let wanted = if class == row.caption {
            caption_rect(bounds, height)
        } else if class == row.tab_search && bounds[2] > 0 {
            Some([0, 0, 0, 0])
        } else {
            None
        };
        if let Some(wanted) = wanted {
            set_bounds(child as *mut c_void, &wanted);
        }
    }
}

/// Replaces the frame's layout once for the process. `height` answers for the windows that are Rivet
/// windows, in DIPs from the top of the frame, and `None` for every other window, which is left alone.
pub unsafe fn install(height: CaptionHeight) -> Result<(), String> {
    if ACTIVE.load(Ordering::Acquire) {
        return Ok(());
    }
    let row = row().ok_or("this browser build has no recorded caption buttons")?;
    let image = image::recorded().ok_or("no recorded browser image")?;
    let layout = crate::builds::get().layout.frame_vtable + row.frame_layout_slot * 8;
    if !image.points_to(layout, row.frame_layout) {
        return Err("this browser build has no recorded frame layout".into());
    }
    // The buttons are moved with `SetBoundsRect`, which the row hook also recorded.
    if !image.has(row.set_bounds_rect, row.set_bounds_rect_prologue) {
        return Err("this browser build has no recorded bounds function".into());
    }
    let _ = HEIGHT.set(height);
    ORIGINAL.store(image.base + row.frame_layout, Ordering::Release);
    crate::chrome_toolbar::transaction(|| {
        crate::chrome_toolbar::replace_slot((image.base + layout) as *mut usize, frame_layout as *const () as usize)
    })?;
    ACTIVE.store(true, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_whale_caption_buttons_sit_in_the_panel_like_chromes() {
        // Chrome's container ends one gap before the window edge and reaches the 44 DIP row from the gap.
        assert_eq!(caption_rect([1187, 0, 46, 32], 44), Some([1179, 8, 46, 44]));
        assert_eq!(caption_rect([1141, 0, 46, 32], 44), Some([1133, 8, 46, 44]));
        // A button with no room keeps none, and one already in place is not set again.
        assert_eq!(caption_rect([0, 0, 0, 0], 44), None);
        assert_eq!(caption_rect([1179, 8, 46, 44], 44), None);
        // The toolbar stops 8 DIPs short of the first of three 46 DIP buttons.
        assert_eq!(RESERVE, 146);
    }
}
