//! Keeps the rounded Windows 11 corners on every Rivet window.
//!
//! Windows 11 rounds a top-level window only while it has no window region and its corner
//! preference allows it. Chromium gives a window whose frame it draws itself a plain rectangular
//! region, and some browsers draw their app window frame themselves, so those windows came out with
//! square corners while the others were round. A rectangle that covers the whole window shapes
//! nothing, so Rivet removes it and asks the desktop window manager for round corners. A shaped
//! region, or any region of a maximised or full-screen window, is the browser's and stays.

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, GetRgnBox, GetWindowRgn, SetWindowRgn, HRGN, SIMPLEREGION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsZoomed};

/// What a window's region is, as far as its corners are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Region {
    /// The window has no region.
    None,
    /// One rectangle of this width and height, from the window's top left corner.
    Rect {
        left: i32,
        top: i32,
        width: i32,
        height: i32,
    },
    /// Anything else: several rectangles or a curve.
    Shaped,
}

/// Whether the region must go for the window's corners to be round. Only a single rectangle that
/// covers exactly the whole window qualifies, since removing it changes nothing but the corners.
/// A maximised window keeps whatever region the browser gave it, because that region trims the
/// frame that hangs past the monitor edge.
pub(crate) fn region_blocks_corners(
    region: Region,
    maximised: bool,
    width: i32,
    height: i32,
) -> bool {
    !maximised
        && region
            == Region::Rect {
                left: 0,
                top: 0,
                width,
                height,
            }
}

unsafe fn region_of(window: HWND) -> Region {
    let region: HRGN = CreateRectRgn(0, 0, 0, 0);
    if region.is_null() {
        return Region::Shaped;
    }
    let kind = GetWindowRgn(window, region);
    let mut bounds: RECT = std::mem::zeroed();
    let result = match kind as u32 {
        0 => Region::None,
        x if x == SIMPLEREGION as u32 && GetRgnBox(region, &mut bounds) != 0 => Region::Rect {
            left: bounds.left,
            top: bounds.top,
            width: bounds.right - bounds.left,
            height: bounds.bottom - bounds.top,
        },
        _ => Region::Shaped,
    };
    DeleteObject(region as _);
    result
}

/// Asks for round corners and removes a whole-window rectangle region. Called after the browser
/// has handled each size change, since Chromium sets its region again on every one.
pub(crate) unsafe fn keep_round(window: HWND) {
    // Removing the region sends the window a position change of its own, which calls this again.
    // The browser may answer that change by setting its region once more, so the inner call stops
    // here instead of going round and round.
    thread_local!(static BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    if BUSY.with(|busy| busy.replace(true)) {
        return;
    }
    keep_round_once(window);
    BUSY.with(|busy| busy.set(false));
}

unsafe fn keep_round_once(window: HWND) {
    let preference = DWMWCP_ROUND;
    DwmSetWindowAttribute(
        window,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        &preference as *const _ as *const _,
        std::mem::size_of_val(&preference) as u32,
    );
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(window, &mut rect) == 0 {
        return;
    }
    if region_blocks_corners(
        region_of(window),
        IsZoomed(window) != 0,
        rect.right - rect.left,
        rect.bottom - rect.top,
    ) {
        SetWindowRgn(window, std::ptr::null_mut(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_whole_window_rectangle_region_is_removed_for_round_corners() {
        let whole = Region::Rect {
            left: 0,
            top: 0,
            width: 1600,
            height: 1000,
        };
        assert!(region_blocks_corners(whole, false, 1600, 1000));
    }

    #[test]
    fn fr2_window_without_region_is_left_alone() {
        assert!(!region_blocks_corners(Region::None, false, 1600, 1000));
    }

    #[test]
    fn fr2_shaped_or_partial_region_stays_the_browsers() {
        assert!(!region_blocks_corners(Region::Shaped, false, 1600, 1000));
        let inset = Region::Rect {
            left: 8,
            top: 8,
            width: 1584,
            height: 984,
        };
        assert!(!region_blocks_corners(inset, false, 1600, 1000));
    }

    #[test]
    fn fr2_maximised_window_keeps_its_region() {
        let whole = Region::Rect {
            left: 0,
            top: 0,
            width: 1600,
            height: 1000,
        };
        assert!(!region_blocks_corners(whole, true, 1600, 1000));
    }
}
