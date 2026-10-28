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

/// The default appbar width in device-independent pixels.
const APPBAR_WIDTH: i32 = 268;

/// The range the appbar width can be dragged between, in device-independent pixels.
const APPBAR_MIN_WIDTH: i32 = 200;
const APPBAR_MAX_WIDTH: i32 = 420;

/// Width of the grab area on the appbar's right edge. Only the half inside the appbar is taken,
/// because the page beside it belongs to the browser.
const RESIZER_WIDTH: i32 = 6;

/// The strip along the window's left edge that shows a hidden appbar when the pointer reaches it.
const EDGE_WIDTH: i32 = 10;

/// The gap between the window edge and the page of a hidden appbar, and between the window edge
/// and the floating appbar.
const FLOAT_INSET: i32 = 8;

/// The empty leading space of the toolbar row, and the toggle button drawn in it.
const TOGGLE_SLOT: i32 = 32;
const TOGGLE_SIZE: i32 = 28;
/// Where the toggle sits, from the window's top edge in device-independent pixels.
const TOGGLE_TOP: i32 = 16;

/// The appbar of one window: how wide it is, and whether it is docked, hidden or peeking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AppbarLayout {
    side: i32,
    collapsed: bool,
    peek: bool,
}

impl AppbarLayout {
    const fn new() -> Self {
        Self {
            side: APPBAR_WIDTH,
            collapsed: false,
            peek: false,
        }
    }

    fn clamp_width(width: i32) -> i32 {
        width.clamp(APPBAR_MIN_WIDTH, APPBAR_MAX_WIDTH)
    }

    /// Sets the width, and reports whether it changed.
    fn set_width(&mut self, width: i32) -> bool {
        let width = Self::clamp_width(width);
        let changed = width != self.side;
        self.side = width;
        changed
    }

    /// Hides or shows the appbar. A peek never survives a change of mode.
    fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
        self.peek = false;
    }

    /// The width of the column the browser leaves free for the docked appbar.
    fn strip(&self) -> i32 {
        if self.collapsed {
            0
        } else {
            self.side
        }
    }

    /// Where the page panel begins. A hidden appbar leaves only the gap of the shell study.
    fn leading(&self) -> i32 {
        if self.collapsed {
            FLOAT_INSET
        } else {
            self.side
        }
    }

    /// Whether the pointer at this x, in device-independent pixels, shows the hidden appbar.
    fn reaches_edge(&self, x: i32) -> bool {
        self.collapsed && (0..EDGE_WIDTH).contains(&x)
    }

    /// Whether the floating appbar stays while the pointer is at this point below the window's
    /// top border. The edge strip belongs to it, so the pointer can slide in from the edge.
    fn keeps_peek(&self, x: i32, y: i32, height: i32) -> bool {
        self.collapsed && (0..FLOAT_INSET + self.side).contains(&x) && (0..height).contains(&y)
    }

    /// The width a drag to this x asks for. The appbar's right edge follows the pointer.
    fn width_at(x: i32) -> i32 {
        Self::clamp_width(x)
    }
}

/// What to reserve left of the client view so the page begins at `leading`, when the browser lays
/// its own strip out at the view's leading edge with `strip` DIPs. The reservation is never
/// negative, because the browser does not lay out a view that starts left of its window.
fn reserved_for_strip(leading: i32, strip: i32) -> i32 {
    (leading - strip).max(0)
}

/// The toggle button's rectangle as (x, y, width, height) in device-independent pixels of the
/// window's client area, for a page panel that begins at `leading`.
fn toggle_rect(leading: i32) -> (i32, i32, i32, i32) {
    (
        leading + (TOGGLE_SLOT - TOGGLE_SIZE) / 2,
        TOGGLE_TOP,
        TOGGLE_SIZE,
        TOGGLE_SIZE,
    )
}

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
    /// Resizes the appbar from its right edge.
    Resizer,
    /// Belongs to the appbar's own content.
    Appbar,
}

/// Classifies a point against a docked appbar `side` DIPs wide, which is zero while the appbar is
/// hidden. `resize_border` is zero when the window is maximized.
fn strip_hit(x: i32, y: i32, dpi: u32, resize_border: i32, side: i32) -> StripHit {
    if x < 0 || x >= scale(side, dpi) {
        return StripHit::Browser;
    }
    match (y < resize_border, x < resize_border) {
        (true, true) => StripHit::TopLeft,
        (true, false) => StripHit::Top,
        _ if x >= scale(side - RESIZER_WIDTH, dpi) => StripHit::Resizer,
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
mod address_display;
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
mod press;
mod spaces;
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
        assert_eq!(strip_hit(100, 10, 96, 0, 268), StripHit::Caption);
        assert_eq!(strip_hit(100, 40, 96, 0, 268), StripHit::Appbar);
        assert_eq!(strip_hit(500, 60, 192, 0, 268), StripHit::Caption);
    }

    #[test]
    fn fr2_appbar_top_edge_resizes_a_restored_window() {
        assert_eq!(strip_hit(100, 2, 96, 8, 268), StripHit::Top);
        assert_eq!(strip_hit(2, 2, 96, 8, 268), StripHit::TopLeft);
        assert_eq!(strip_hit(100, 2, 96, 0, 268), StripHit::Caption);
    }

    #[test]
    fn fr2_points_outside_the_strip_stay_with_the_browser() {
        assert_eq!(strip_hit(268, 10, 96, 0, 268), StripHit::Browser);
        assert_eq!(strip_hit(-1, 10, 96, 0, 268), StripHit::Browser);
    }

    #[test]
    fn fr3_appbar_width_stays_between_200_and_420() {
        let mut layout = AppbarLayout::new();
        assert_eq!(layout.side, 268);
        assert!(layout.set_width(100));
        assert_eq!(layout.side, 200);
        assert!(layout.set_width(900));
        assert_eq!(layout.side, 420);
        assert!(!layout.set_width(420));
        assert_eq!(AppbarLayout::width_at(310), 310);
        assert_eq!(AppbarLayout::width_at(-5), 200);
    }

    #[test]
    fn fr3_hidden_appbar_reserves_no_strip_and_toggles_back() {
        let mut layout = AppbarLayout::new();
        layout.set_width(300);
        assert_eq!((layout.strip(), layout.leading()), (300, 300));
        layout.toggle();
        assert_eq!((layout.strip(), layout.leading()), (0, FLOAT_INSET));
        layout.toggle();
        assert_eq!((layout.strip(), layout.leading()), (300, 300));
    }

    #[test]
    fn fr3_hidden_appbar_peeks_from_the_left_edge_only() {
        let mut layout = AppbarLayout::new();
        assert!(
            !layout.reaches_edge(3),
            "a docked appbar has nothing to peek"
        );
        layout.toggle();
        assert!(layout.reaches_edge(0));
        assert!(layout.reaches_edge(9));
        assert!(!layout.reaches_edge(10));
        assert!(!layout.reaches_edge(-1));
    }

    #[test]
    fn fr3_peek_stays_over_the_floating_appbar_and_leaves_with_the_pointer() {
        let mut layout = AppbarLayout::new();
        layout.toggle();
        assert!(layout.keeps_peek(5, 400, 900));
        assert!(layout.keeps_peek(FLOAT_INSET + 267, 400, 900));
        assert!(!layout.keeps_peek(FLOAT_INSET + 268, 400, 900));
        assert!(!layout.keeps_peek(100, 900, 900));
        layout.toggle();
        assert!(
            !layout.keeps_peek(100, 400, 900),
            "a docked appbar does not float"
        );
    }

    #[test]
    fn fr3_toggling_ends_a_peek() {
        let mut layout = AppbarLayout::new();
        layout.toggle();
        layout.peek = true;
        layout.toggle();
        assert!(!layout.peek);
    }

    #[test]
    fn fr3_resizer_is_the_inner_edge_of_the_docked_appbar() {
        assert_eq!(strip_hit(266, 100, 96, 0, 268), StripHit::Resizer);
        assert_eq!(strip_hit(262, 100, 96, 0, 268), StripHit::Resizer);
        assert_eq!(strip_hit(261, 100, 96, 0, 268), StripHit::Appbar);
        assert_eq!(strip_hit(266, 10, 96, 0, 268), StripHit::Resizer);
        assert_eq!(strip_hit(266, 100, 96, 0, 0), StripHit::Browser);
        assert_eq!(strip_hit(416, 100, 96, 0, 420), StripHit::Resizer);
    }

    #[test]
    fn fr3_page_edge_follows_the_appbar_for_every_width_and_when_hidden() {
        // The strip is made as wide as the appbar, so the page begins at the appbar's edge.
        for side in [200, 240, 268, 420] {
            assert_eq!(reserved_for_strip(side, side) + side, side);
        }
        // Hidden: no strip, and the page begins after the gap.
        assert_eq!(reserved_for_strip(FLOAT_INSET, 0), FLOAT_INSET);
        // A strip wider than the wanted edge never pushes the view off the window.
        assert_eq!(reserved_for_strip(200, 240), 0);
    }

    #[test]
    fn fr3_toggle_sits_in_the_leading_space_of_the_toolbar_row() {
        assert_eq!(toggle_rect(268), (270, TOGGLE_TOP, 28, 28));
        let (x, _, width, _) = toggle_rect(FLOAT_INSET);
        assert!(x >= FLOAT_INSET && x + width <= FLOAT_INSET + TOGGLE_SLOT);
    }

    #[cfg(windows)]
    #[test]
    fn fr2_runtime_rejects_a_window_that_does_not_exist() {
        assert_eq!(rivet_runtime_start_for_window(1), 0);
    }
}

#[cfg(windows)]
mod tab_rows;
