//! Where Rivet places a window's page view. The browser's own layout is left alone: Rivet works out
//! the panel from the client area itself and places the window's page view, and the pages of a grid,
//! inside it.

use crate::features::frame_layout::Rect;

/// A client size in pixels as DIPs at `dpi`.
pub(crate) fn client_dips(width: i32, height: i32, dpi: u32) -> [i32; 2] {
    let dips = |pixels: i32| (pixels.max(0) * 96 + dpi as i32 / 2) / dpi.max(1) as i32;
    [dips(width), dips(height)]
}

/// The bounds of the page view, in its parent's coordinates, for the panel Rivet placed in window
/// DIPs. `parent` is the window position of the page view's parent, and `bar` the height of the
/// bar row at the top of the panel.
pub(crate) fn page_view_rect(panel: Rect, parent: [i32; 2], bar: i32) -> [i32; 4] {
    let bar = bar.clamp(0, (panel.height - 1).max(0));
    [
        panel.x - parent[0],
        panel.y - parent[1] + bar,
        panel.width.max(1),
        (panel.height - bar).max(1),
    ]
}

pub(crate) fn forget(window: isize) {
    BROWSER_WIDTH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.0 != window);
}

/// The width the browser lays each window out in, in its own DIPs, read from a view that spans the
/// window. The browser picks its scale itself, which can differ from the one Windows reports for a
/// window off every monitor, so this width is what turns client pixels into the browser's DIPs.
static BROWSER_WIDTH: std::sync::Mutex<Vec<(isize, i32)>> = std::sync::Mutex::new(Vec::new());

/// Records the width of a view of the window that starts at its left edge. Returns whether it
/// changed.
pub(crate) fn note_browser_width(window: isize, view: [i32; 4]) -> bool {
    if view[0] != 0 || view[2] <= 0 {
        return false;
    }
    let mut all = BROWSER_WIDTH.lock().unwrap_or_else(|error| error.into_inner());
    match all.iter_mut().find(|entry| entry.0 == window) {
        Some(entry) => std::mem::replace(&mut entry.1, view[2]) != view[2],
        None => {
            all.push((window, view[2]));
            true
        }
    }
}

/// The client size in the browser's DIPs: by the width the browser laid the window out in when it
/// is known, else at `dpi`.
pub(crate) fn browser_dips(window: isize, width: i32, height: i32, dpi: u32) -> [i32; 2] {
    let known = BROWSER_WIDTH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window)
        .map(|entry| entry.1);
    match known {
        Some(dips) if width > 0 => [dips, (height.max(0) as i64 * dips as i64 / width as i64) as i32],
        _ => client_dips(width, height, dpi),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr12_client_pixels_follow_the_scale_the_browser_laid_out_at() {
        let window = 0x5150;
        // Before the browser laid anything out, the scale Windows reports is used.
        assert_eq!(browser_dips(window, 1800, 1067, 192), [900, 534]);
        // A view that does not start at the left edge says nothing of the window's width.
        assert!(!note_browser_width(window, [276, 0, 616, 518]));
        assert!(note_browser_width(window, [0, 0, 1440, 824]));
        assert_eq!(browser_dips(window, 1800, 1067, 192), [1440, 853]);
        forget(window);
        assert_eq!(browser_dips(window, 1800, 1067, 192), [900, 534]);
    }

    #[test]
    fn fr12_client_pixels_become_dips_at_the_window_scale() {
        assert_eq!(client_dips(1600, 1000, 96), [1600, 1000]);
        assert_eq!(client_dips(2400, 1500, 144), [1600, 1000]);
        assert_eq!(client_dips(2000, 1250, 120), [1600, 1000]);
    }

    #[test]
    fn fr12_page_view_sits_below_the_bar_inside_the_panel() {
        let panel = Rect {
            x: 268,
            y: 8,
            width: 1324,
            height: 984,
        };
        // A parent at the window's origin.
        assert_eq!(page_view_rect(panel, [0, 0], 44), [268, 52, 1324, 940]);
        // A parent that starts below the browser's own title row.
        assert_eq!(page_view_rect(panel, [0, 32], 44), [268, 20, 1324, 940]);
        // A bar taller than the panel leaves one DIP of page.
        assert_eq!(page_view_rect(Rect { height: 10, ..panel }, [0, 0], 44)[3], 1);
    }
}
