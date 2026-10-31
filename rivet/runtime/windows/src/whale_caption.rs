//! Whale's caption buttons are children of its frame view, laid out in the strip along the window's top
//! edge, 32 DIPs tall, with a tab search chevron before them. A Rivet window's top row is taller and sits
//! inside the panel's gap, so after the frame lays out, the buttons are moved to where Chrome's caption
//! container puts them and the chevron is left out. Rivet's own tab list replaces it.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::builds::WhaleRow;
use crate::chrome_layout::{image, CaptionHeight, PANEL_GAP};
use crate::chrome_toolbar::{caption_icon, CaptionGlyph};
use windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed;

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

/// Which glyph a caption button at `x` draws in a frame `width` wide, or `None` when it is not at one
/// of the three places the layout above gives the buttons.
fn glyph_at(width: i32, x: i32, zoomed: bool) -> Option<CaptionGlyph> {
    match (width - PANEL_GAP - x) / 46 {
        _ if (width - PANEL_GAP - x) % 46 != 0 => None,
        1 => Some(CaptionGlyph::Close),
        2 if zoomed => Some(CaptionGlyph::Restore),
        2 => Some(CaptionGlyph::Maximize),
        3 => Some(CaptionGlyph::Minimize),
        _ => None,
    }
}

/// Windows' glyph ink. Close turns white on its red hover.
pub(crate) fn glyph_colour(glyph: CaptionGlyph, state: i32) -> u32 {
    if glyph == CaptionGlyph::Close && (state == 1 || state == 2) {
        0xffff_ffff
    } else {
        0xff1a_1a1a
    }
}

/// The fill behind a glyph: Windows' faint grey on hover and red on close, none at rest.
pub(crate) fn hover_colour(glyph: CaptionGlyph, state: i32) -> Option<u32> {
    match (glyph, state) {
        (CaptionGlyph::Close, 1 | 2) => Some(0xffc4_2b1c),
        (_, 1 | 2) => Some(0x0a00_0000),
        _ => None,
    }
}

static ICONS: OnceLock<[usize; 4]> = OnceLock::new();
static PAINT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// Whale's glyphs are resource images, so the button draws Rivet's Windows glyphs instead.
unsafe extern "system" fn button_paint(button: *mut c_void, canvas: *mut c_void) {
    let original: unsafe extern "system" fn(*mut c_void, *mut c_void) =
        std::mem::transmute(PAINT_ORIGINAL.load(Ordering::Acquire));
    let (Some(row), Some(image), Some(height), Some(icons)) = (row(), image::recorded(), HEIGHT.get(), ICONS.get()) else {
        return original(button, canvas);
    };
    let Some(window) = crate::chrome_layout::hwnd_of_view(button).filter(|window| height(*window).is_some()) else {
        return original(button, canvas);
    };
    let parent = *((button as usize + row.parent_offset) as *const usize);
    if parent < 0x10000 {
        return original(button, canvas);
    }
    let width = (*((parent + row.bounds_offset) as *const [i32; 4]))[2];
    let x = (*((button as usize + row.bounds_offset) as *const [i32; 4]))[0];
    let Some(glyph) = glyph_at(width, x, IsZoomed(window) != 0) else {
        return original(button, canvas);
    };
    let state = *((button as usize + row.button_state_offset) as *const i32);
    // The image paint this replaces may hold the hover, so it is drawn here with the glyph.
    if let Some(colour) = hover_colour(glyph, state) {
        let fill: unsafe extern "system" fn(*mut c_void, u32) =
            std::mem::transmute(image.base + crate::builds::get().layout.draw_color);
        fill(canvas, colour);
    }
    let paint: unsafe extern "system" fn(*mut c_void, *const c_void, i32, u32) =
        std::mem::transmute(image.base + row.paint_vector_icon);
    paint(canvas, icons[glyph as usize] as *const c_void, 46, glyph_colour(glyph, state));
}

unsafe fn install_paint(row: &WhaleRow, image: &image::Image) -> Result<(), String> {
    let slot = row.caption + row.button_paint_slot * 8;
    if !image.points_to(slot, row.caption_button_paint) || !image.has(row.paint_vector_icon, row.paint_vector_icon_prologue) {
        return Err("this browser build has no recorded caption paint".into());
    }
    let _ = ICONS.set([CaptionGlyph::Minimize, CaptionGlyph::Maximize, CaptionGlyph::Restore, CaptionGlyph::Close].map(|glyph| caption_icon(glyph, row.canvas_command) as usize));
    PAINT_ORIGINAL.store(image.base + row.caption_button_paint, Ordering::Release);
    crate::chrome_toolbar::transaction(|| crate::chrome_toolbar::replace_slot((image.base + slot) as *mut usize, button_paint as *const () as usize))
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
    // Glyphs are cosmetic: a build whose paint differs keeps Whale's own.
    if let Err(error) = install_paint(row, &image) {
        crate::vertical_tabs::trace(&format!("whale caption glyphs: {error}"));
    }
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

    #[test]
    fn fr12_whale_caption_glyphs_follow_the_buttons_place_and_state() {
        assert_eq!(glyph_at(800, 746, false), Some(CaptionGlyph::Close));
        assert_eq!(glyph_at(800, 700, false), Some(CaptionGlyph::Maximize));
        assert_eq!(glyph_at(800, 700, true), Some(CaptionGlyph::Restore));
        assert_eq!(glyph_at(800, 654, false), Some(CaptionGlyph::Minimize));
        assert_eq!(glyph_at(800, 655, false), None);
        assert_eq!(glyph_colour(CaptionGlyph::Close, 1), 0xffff_ffff);
        assert_eq!(glyph_colour(CaptionGlyph::Minimize, 1), 0xff1a_1a1a);
        assert_eq!(hover_colour(CaptionGlyph::Close, 1), Some(0xffc4_2b1c));
        assert_eq!(hover_colour(CaptionGlyph::Minimize, 0), None);
    }
}
