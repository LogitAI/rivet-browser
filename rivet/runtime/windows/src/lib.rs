//! The in-browser Rivet runtime bootstrap.
//!
//! Rivet attaches to the browser's own top-level window. It never creates a second window and
//! never reparents or restyles the browser window. Chromium's own frame layout is told to leave a
//! strip free on the left, so Chromium lays out its title bar, caption buttons and web content
//! beside it. The appbar is a DirectComposition visual of that same window, so it is part of the
//! window's surface. It minimizes, maximizes, snaps and animates with the browser because Windows
//! composes them as one window.

/// The ABI expected by the Windows launcher.
pub const RUNTIME_ABI_VERSION: u32 = 1;

/// Lets the launcher verify that it loaded a Rivet runtime, rather than an arbitrary DLL.
#[no_mangle]
pub extern "C" fn rivet_runtime_abi_version() -> u32 {
    RUNTIME_ABI_VERSION
}

/// The appbar width in device-independent pixels.
const APPBAR_WIDTH: i32 = 268;

/// Height of the appbar band that drags the window, matched to Chromium's app title bar.
const CAPTION_HEIGHT: i32 = 32;

/// Converts a device-independent length to physical pixels for a DPI.
fn scale(value: i32, dpi: u32) -> i32 {
    (value * dpi as i32 + 48) / 96
}

/// The DPI that maps a browser layout width in device-independent pixels onto its client width
/// in physical pixels.
fn layout_scale_dpi(client_pixels: i32, layout_width: i32) -> u32 {
    ((client_pixels as i64 * 96 + layout_width as i64 / 2) / layout_width.max(1) as i64) as u32
}

/// The first row of the browser's title bar in a restored window, in physical pixels. The browser
/// keeps one device-independent pixel, rounded up, for the top edge. The window's own border can
/// be thicker and then covers it.
fn title_bar_top(window_border: i32, dpi: u32) -> i32 {
    window_border.max(dpi.div_ceil(96) as i32)
}

/// What a point in the reserved strip does, given client coordinates of the browser window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StripHit {
    /// The point belongs to the browser, not to the strip.
    Browser,
    /// Resizes the window from its top edge.
    Top,
    /// Resizes the window from its top-left corner.
    TopLeft,
    /// Drags the window like its title bar.
    Caption,
    /// Belongs to the appbar's own content.
    Appbar,
}

/// Classifies a point. `resize_border` is zero when the window is maximized.
fn strip_hit(x: i32, y: i32, dpi: u32, resize_border: i32) -> StripHit {
    if x < 0 || x >= scale(APPBAR_WIDTH, dpi) {
        return StripHit::Browser;
    }
    match (y < resize_border, x < resize_border) {
        (true, true) => StripHit::TopLeft,
        (true, false) => StripHit::Top,
        _ if y < scale(CAPTION_HEIGHT, dpi) => StripHit::Caption,
        _ => StripHit::Appbar,
    }
}

/// Starts Rivet after the DLL has loaded and the Windows loader lock has been released.
///
/// The launcher supplies the exact browser window discovered for its session. A zero handle is
/// reserved for a hidden task, which does not create a frame. Attaching twice to one window is a
/// no-op, and several windows of one browser process can each carry their own appbar.
#[cfg(windows)]
#[no_mangle]
pub extern "system" fn rivet_runtime_start_for_window(window: isize) -> u32 {
    if window == 0 {
        return 1;
    }
    unsafe { u32::from(frame::attach(window as _)) }
}

#[cfg(windows)]
#[cfg(windows)]
mod appbar;
#[cfg(windows)]
mod chrome_layout;
#[cfg(windows)]
mod chrome_navigation;
#[cfg(windows)]
mod chrome_tabs;
#[cfg(windows)]
mod chrome_toolbar;
#[cfg(windows)]
mod frame;
#[cfg(windows)]
#[cfg(windows)]
mod taskbar;
#[cfg(windows)]
mod vertical_tabs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr2_runtime_exposes_the_expected_abi() {
        assert_eq!(rivet_runtime_abi_version(), RUNTIME_ABI_VERSION);
    }

    #[test]
    fn fr2_appbar_follows_the_scale_the_browser_laid_out_at() {
        assert_eq!(layout_scale_dpi(2400, 1200), 192);
        assert_eq!(layout_scale_dpi(1500, 1200), 120);
        assert_eq!(layout_scale_dpi(1200, 1200), 96);
    }

    #[test]
    fn fr2_appbar_starts_with_the_browser_title_bar() {
        // Measured on Chrome 156: the title bar colour starts at row 2 at 125% and at 200%.
        assert_eq!(title_bar_top(1, 120), 2);
        assert_eq!(title_bar_top(2, 192), 2);
        assert_eq!(title_bar_top(1, 96), 1);
    }

    #[test]
    fn fr2_appbar_caption_band_drags_the_browser_window() {
        assert_eq!(strip_hit(100, 10, 96, 0), StripHit::Caption);
        assert_eq!(strip_hit(100, 40, 96, 0), StripHit::Appbar);
        assert_eq!(strip_hit(500, 60, 192, 0), StripHit::Caption);
    }

    #[test]
    fn fr2_appbar_top_edge_resizes_a_restored_window() {
        assert_eq!(strip_hit(100, 2, 96, 8), StripHit::Top);
        assert_eq!(strip_hit(2, 2, 96, 8), StripHit::TopLeft);
        assert_eq!(strip_hit(100, 2, 96, 0), StripHit::Caption);
    }

    #[test]
    fn fr2_points_outside_the_strip_stay_with_the_browser() {
        assert_eq!(strip_hit(268, 10, 96, 0), StripHit::Browser);
        assert_eq!(strip_hit(-1, 10, 96, 0), StripHit::Browser);
    }

    #[cfg(windows)]
    #[test]
    fn fr2_runtime_rejects_a_window_that_does_not_exist() {
        assert_eq!(rivet_runtime_start_for_window(1), 0);
    }
}
