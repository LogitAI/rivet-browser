//! Two experiments that leave room for Rivet's surfaces without hooking the browser's layout.
//!
//! Both are off unless switched on for the browser process. A switch is an environment variable
//! the browser process inherits, or a flag file of the same name in the debug folder. Each is read
//! once per process, so a change needs a new browser process.
//!
//! - `RIVET_EXPERIMENT_NCCALCSIZE`: the window procedure answers `WM_NCCALCSIZE` with a client
//!   area that leaves out the appbar's column (`1`), or the column and the top row (`2`). The
//!   browser then lays itself out in the smaller client area, and the frame layout hook reserves
//!   nothing. Rivet's visuals move by the same amount the other way, which shows whether a visual
//!   of the window's composition target can still draw outside the client area.
//! - `RIVET_EXPERIMENT_PAGE_VIEW_ONLY`: the frame view's client view placement is not replaced,
//!   the caption layout, caption height and top inset are not installed and the page area layout
//!   is not patched. Rivet works out the panel from the client area itself and places the window's
//!   page view, and the pages of a grid, inside it.
//!
//! When both are on, only the page view experiment runs, since it already places every page.

use std::sync::OnceLock;

use crate::features::frame_layout::Rect;

/// The environment variable and flag file name of experiment A.
pub(crate) const NCCALCSIZE: &str = "RIVET_EXPERIMENT_NCCALCSIZE";
/// The environment variable and flag file name of experiment B.
pub(crate) const PAGE_VIEW_ONLY: &str = "RIVET_EXPERIMENT_PAGE_VIEW_ONLY";

/// What experiment A takes out of the client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NcInset {
    Off,
    /// The appbar's column at the left.
    Left,
    /// The appbar's column and the top row.
    LeftAndTop,
}

/// Experiment A from the variable's value and whether its flag file exists. A flag file alone
/// means the column only.
pub(crate) fn parse_nccalcsize(value: Option<&str>, flag: bool) -> NcInset {
    let value = value.map(|value| value.trim().to_ascii_lowercase());
    match value.as_deref() {
        Some("1" | "left" | "on" | "true") => NcInset::Left,
        Some("2" | "top" | "left+top") => NcInset::LeftAndTop,
        Some("0" | "off" | "false") => NcInset::Off,
        _ if flag => NcInset::Left,
        _ => NcInset::Off,
    }
}

/// An on or off switch from the variable's value and whether its flag file exists.
pub(crate) fn parse_switch(value: Option<&str>, flag: bool) -> bool {
    let value = value.map(|value| value.trim().to_ascii_lowercase());
    match value.as_deref() {
        Some("1" | "on" | "true") => true,
        Some("0" | "off" | "false") => false,
        _ => flag,
    }
}

/// Experiment A gives way to experiment B, which places every page itself.
pub(crate) fn effective_nccalcsize(inset: NcInset, page_view_only: bool) -> NcInset {
    if page_view_only {
        NcInset::Off
    } else {
        inset
    }
}

fn flag_exists(name: &str) -> bool {
    crate::frame::debug_directory()
        .is_some_and(|dir| dir.join(format!("{name}.flag")).exists())
}

fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Experiment B, read once per process.
pub(crate) fn page_view_only() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = parse_switch(
            variable(PAGE_VIEW_ONLY).as_deref(),
            flag_exists(PAGE_VIEW_ONLY),
        );
        crate::trace::trace(&format!("experiment: page view only {on}"));
        on
    })
}

/// Experiment A, read once per process.
pub(crate) fn nccalcsize() -> NcInset {
    static INSET: OnceLock<NcInset> = OnceLock::new();
    *INSET.get_or_init(|| {
        let inset = effective_nccalcsize(
            parse_nccalcsize(variable(NCCALCSIZE).as_deref(), flag_exists(NCCALCSIZE)),
            page_view_only(),
        );
        crate::trace::trace(&format!("experiment: client area inset {inset:?}"));
        inset
    })
}

/// The pixels experiment A takes from the left and the top of the client area, for an appbar
/// column `leading` DIPs wide and a top row `top` DIPs tall, at `dpi`.
pub(crate) fn nc_inset_pixels(inset: NcInset, leading: i32, top: i32, dpi: u32) -> [i32; 2] {
    let pixels = |dips: i32| (dips.max(0) * dpi as i32 + 48) / 96;
    match inset {
        NcInset::Off => [0, 0],
        NcInset::Left => [pixels(leading), 0],
        NcInset::LeftAndTop => [pixels(leading), pixels(top)],
    }
}

/// The client rect, as left, top, right and bottom, with `left` and `top` pixels taken from it.
/// At least one pixel of width and height is always left.
pub(crate) fn shrink_client(rect: [i32; 4], left: i32, top: i32) -> [i32; 4] {
    let [l, t, r, b] = rect;
    let left = left.clamp(0, (r - l - 1).max(0));
    let top = top.clamp(0, (b - t - 1).max(0));
    [l + left, t + top, r, b]
}

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

/// The client inset experiment A applied to each window last, in pixels, so Rivet's visuals move
/// back by it.
static APPLIED: std::sync::Mutex<Vec<(isize, [i32; 2])>> = std::sync::Mutex::new(Vec::new());

/// Remembers the inset applied to a window. Returns whether it changed.
pub(crate) fn remember_inset(window: isize, inset: [i32; 2]) -> bool {
    let mut all = APPLIED.lock().unwrap_or_else(|error| error.into_inner());
    match all.iter_mut().find(|entry| entry.0 == window) {
        Some(entry) => std::mem::replace(&mut entry.1, inset) != inset,
        None => {
            all.push((window, inset));
            inset != [0, 0]
        }
    }
}

/// The inset experiment A applied to a window last, in pixels.
pub(crate) fn applied_inset(window: isize) -> [i32; 2] {
    APPLIED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|entry| entry.0 == window)
        .map_or([0, 0], |entry| entry.1)
}

pub(crate) fn forget(window: isize) {
    APPLIED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|entry| entry.0 != window);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr3_layout_experiments_are_off_unless_switched_on() {
        assert_eq!(parse_nccalcsize(None, false), NcInset::Off);
        assert_eq!(parse_nccalcsize(Some(""), false), NcInset::Off);
        assert_eq!(parse_nccalcsize(Some("0"), true), NcInset::Off);
        assert_eq!(parse_nccalcsize(Some("1"), false), NcInset::Left);
        assert_eq!(parse_nccalcsize(Some(" Left "), false), NcInset::Left);
        assert_eq!(parse_nccalcsize(Some("2"), false), NcInset::LeftAndTop);
        assert_eq!(parse_nccalcsize(None, true), NcInset::Left);
        assert!(!parse_switch(None, false));
        assert!(!parse_switch(Some("yes please"), false));
        assert!(parse_switch(Some("1"), false));
        assert!(parse_switch(Some("TRUE"), false));
        assert!(parse_switch(None, true));
        assert!(!parse_switch(Some("off"), true));
    }

    #[test]
    fn nfr3_client_area_experiment_gives_way_to_page_view_only() {
        assert_eq!(effective_nccalcsize(NcInset::Left, true), NcInset::Off);
        assert_eq!(effective_nccalcsize(NcInset::LeftAndTop, false), NcInset::LeftAndTop);
    }

    #[test]
    fn fr2_client_area_leaves_out_the_appbar_column_at_the_window_scale() {
        assert_eq!(nc_inset_pixels(NcInset::Off, 268, 52, 144), [0, 0]);
        assert_eq!(nc_inset_pixels(NcInset::Left, 268, 52, 96), [268, 0]);
        assert_eq!(nc_inset_pixels(NcInset::Left, 268, 52, 144), [402, 0]);
        assert_eq!(nc_inset_pixels(NcInset::LeftAndTop, 268, 52, 144), [402, 78]);
        // A hidden appbar takes nothing.
        assert_eq!(nc_inset_pixels(NcInset::LeftAndTop, 0, 0, 120), [0, 0]);
    }

    #[test]
    fn fr2_shrunk_client_area_keeps_one_pixel() {
        assert_eq!(shrink_client([0, 0, 1600, 1000], 402, 78), [402, 78, 1600, 1000]);
        assert_eq!(shrink_client([8, 8, 1608, 1008], 402, 0), [410, 8, 1608, 1008]);
        assert_eq!(shrink_client([0, 0, 100, 50], 400, 400), [99, 49, 100, 50]);
        assert_eq!(shrink_client([0, 0, 100, 50], -5, -5), [0, 0, 100, 50]);
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

    #[test]
    fn fr2_applied_inset_is_remembered_per_window() {
        let window = 0x7777_0001;
        assert_eq!(applied_inset(window), [0, 0]);
        assert!(remember_inset(window, [402, 0]));
        assert!(!remember_inset(window, [402, 0]));
        assert_eq!(applied_inset(window), [402, 0]);
        forget(window);
        assert_eq!(applied_inset(window), [0, 0]);
    }
}
