//! The browser's own bubbles (password save, permission requests, downloads and page info) open
//! next to Rivet's address bar.
//!
//! An app window has no location bar, so Chromium's bubbles find no anchor view and open at the
//! edge of the monitor or in the window's corner. Every such bubble asks
//! `views::BubbleDialogDelegate::GetAnchorRect` where to go. Rivet wraps that one Chromium function:
//! the browser's answer stays when it points into the page, and is replaced by the address field
//! (the security chip for page info) when it points anywhere else. The bubble itself stays the
//! browser's, with its own window, input and storage.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

/// `views::BubbleDialogDelegate::GetAnchorRect` of one build, with its first bytes. The bytes are
/// whole instructions that do not depend on their own address, so they move into a trampoline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AnchorRow {
    pub browser: &'static str,
    pub version: [u16; 4],
    pub get_anchor_rect: usize,
    pub prologue: &'static [u8],
    /// Whether the row was driven on that build. Rows found from public symbols only are not.
    pub verified: bool,
}

/// `push r15; push r14; push r13; push r12; push rsi; push rdi; push rbx; sub rsp, 0xD0`.
const PROLOGUE_D0: &[u8] = &[
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xD0, 0x00,
    0x00, 0x00,
];

/// The builds whose `GetAnchorRect` was found and checked against the bytes of the installed
/// module. Chrome, Brave and Edge were found from their public symbols. Whale and Samsung Internet
/// have none, so theirs were found by the shape of Chromium's function: the anchor view read from
/// the delegate, its `GetAnchorBoundsInScreen` slot and the `anchor_rect_` optional written back.
/// None of them has been driven yet.
pub(crate) const ROWS: &[AnchorRow] = &[
    AnchorRow {
        browser: "Google Chrome",
        version: [156, 0, 8078, 74],
        get_anchor_rect: 0x0AA7_D550,
        prologue: PROLOGUE_D0,
        verified: false,
    },
    AnchorRow {
        browser: "Brave",
        version: [156, 1, 98, 63],
        get_anchor_rect: 0x0BED_C9D0,
        prologue: PROLOGUE_D0,
        verified: false,
    },
    AnchorRow {
        browser: "Google Chrome",
        version: [156, 0, 8078, 80],
        get_anchor_rect: 0x0AA7_2C00,
        prologue: PROLOGUE_D0,
        verified: false,
    },
    AnchorRow {
        browser: "Google Chrome",
        version: [157, 0, 8103, 38],
        get_anchor_rect: 0x0AF2_96E0,
        prologue: PROLOGUE_D0,
        verified: false,
    },
    AnchorRow {
        browser: "Brave",
        version: [157, 1, 99, 55],
        get_anchor_rect: 0x0C3B_8D60,
        prologue: PROLOGUE_D0,
        verified: false,
    },
    AnchorRow {
        browser: "Microsoft Edge",
        version: [156, 0, 4314, 58],
        get_anchor_rect: 0x0330_BF40,
        // `push r15; push r14; push r12; push rsi; push rdi; push rbx; sub rsp, 0xC8`.
        prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xC8, 0x00,
            0x00, 0x00,
        ],
        verified: false,
    },
    AnchorRow {
        browser: "Naver Whale",
        version: [5, 39, 412, 57],
        get_anchor_rect: 0x0903_7B10,
        prologue: PROLOGUE_D0,
        verified: false,
    },
    AnchorRow {
        browser: "Samsung Internet",
        version: [30, 0, 4, 29],
        get_anchor_rect: 0x090C_B920,
        // `push r15; push r14; push rsi; push rdi; push rbx; sub rsp, 0xD0; mov rsi, rdx`.
        prologue: &[
            0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xD0, 0x00, 0x00, 0x00,
            0x48, 0x89, 0xD6,
        ],
        verified: false,
    },
];

/// The row of a build, by its browser name and version.
/// The `GetAnchorRect` of a build and its first bytes: the row's own when it carries them, else
/// the built-in list's.
pub(crate) fn anchor_of(build: &crate::builds::Build) -> Option<(usize, &'static [u8])> {
    build.bubble_anchor.or_else(|| {
        row_for(build.browser, build.version).map(|row| (row.get_anchor_rect, row.prologue))
    })
}

pub(crate) fn row_for(browser: &str, version: [u16; 4]) -> Option<&'static AnchorRow> {
    ROWS.iter()
        .find(|row| row.browser == browser && row.version == version)
}

/// A `gfx::Rect`: x, y, width and height in screen DIPs.
pub(crate) type Rect = [i32; 4];

fn contains(outer: Rect, inner: Rect) -> bool {
    inner[2] > 0
        && inner[3] > 0
        && inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[0] + inner[2] <= outer[0] + outer[2]
        && inner[1] + inner[3] <= outer[1] + outer[3]
}

/// Where a bubble anchors. `page` is the area below the top row. The browser's own answer stays
/// when it lies in the page, as for a bubble anchored to page content. Anything else, the monitor
/// edge or the window's corner, becomes the security chip for page info and the address otherwise.
pub(crate) fn choose(
    original: Rect,
    page: Rect,
    address: Rect,
    chip: Rect,
    page_info: bool,
) -> Rect {
    if !page_info && contains(page, original) {
        return original;
    }
    if page_info {
        chip
    } else {
        address
    }
}

/// Set while Rivet opens the page info bubble from its security chip.
static PAGE_INFO: AtomicBool = AtomicBool::new(false);
/// The window whose security chip opens the page info bubble, while it opens.
static PAGE_INFO_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// Runs `open` with the next bubble anchored at the security chip of `window`.
pub(crate) fn with_page_info<R>(window: HWND, open: impl FnOnce() -> R) -> R {
    PAGE_INFO_WINDOW.store(window as usize, Ordering::Release);
    PAGE_INFO.store(true, Ordering::Release);
    let result = open();
    PAGE_INFO.store(false, Ordering::Release);
    PAGE_INFO_WINDOW.store(0, Ordering::Release);
    result
}

/// The Rivet window a bubble belongs to: the active window when it is a Rivet window, else the
/// window whose security chip opened page info, else the only Rivet window of the thread. A bubble
/// can open while its window is not active, as a permission request of a page in a window the
/// person has not clicked yet does, and it must still open by that window's address row.
pub(crate) fn bubble_window(active: usize, page_info: usize, windows: &[usize]) -> Option<usize> {
    if active != 0 && windows.contains(&active) {
        return Some(active);
    }
    if page_info != 0 && windows.contains(&page_info) {
        return Some(page_info);
    }
    match windows {
        [only] => Some(*only),
        _ => None,
    }
}

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
type GetAnchorRect = unsafe extern "system" fn(*const core::ffi::c_void, *mut Rect) -> *mut Rect;

/// Where Chromium puts a monitor's top left corner in screen DIPs. The primary monitor starts at
/// zero, and a monitor beside it starts where the primary ends in DIPs, offset along the shared edge
/// by its physical offset at the primary's scale, as `display::win::ScreenWin` lays displays out. A
/// monitor that touches no edge of the primary keeps its physical place at its own scale.
pub(crate) fn dip_origin(monitor: ([i32; 4], f32), primary: ([i32; 4], f32)) -> (f32, f32) {
    let ([l, t, r, b], scale) = monitor;
    let ([pl, pt, pr, pb], ps) = primary;
    let (pw, ph) = ((pr - pl) as f32 / ps, (pb - pt) as f32 / ps);
    if [l, t, r, b] == [pl, pt, pr, pb] {
        (0.0, 0.0)
    } else if l == pr {
        (pw, (t - pt) as f32 / ps)
    } else if r == pl {
        (-((r - l) as f32 / scale), (t - pt) as f32 / ps)
    } else if t == pb {
        ((l - pl) as f32 / ps, ph)
    } else if b == pt {
        ((l - pl) as f32 / ps, -((b - t) as f32 / scale))
    } else {
        (l as f32 / scale, t as f32 / scale)
    }
}

/// A point in physical screen pixels as Chromium's screen DIPs, on `monitor`.
pub(crate) fn to_dips(
    (x, y): (f32, f32),
    monitor: ([i32; 4], f32),
    primary: ([i32; 4], f32),
) -> (f32, f32) {
    let (ox, oy) = dip_origin(monitor, primary);
    let ([l, t, _, _], scale) = monitor;
    (ox + (x - l as f32) / scale, oy + (y - t as f32) / scale)
}

/// A rect in client DIPs of a window whose client origin is at `origin` pixels and whose layout
/// scale is `factor`, as screen DIPs on `monitors`, the window's display and the primary. Chromium
/// puts every view of a window into screen DIPs with that one display:
/// `display::win::ScreenWin::ScreenToDIPRect` takes the window's nearest monitor
/// (`GetScreenWinDisplayNearestHWND`), so the chip lands where the browser's own views do even when
/// it sits over another monitor than the client origin.
fn client_to_screen_dips(
    origin: (i32, i32),
    factor: f32,
    [x, y, w, h]: [f32; 4],
    monitors: Option<(crate::os::win32::Monitor, crate::os::win32::Monitor)>,
) -> Rect {
    let px = origin.0 as f32 + x * factor;
    let py = origin.1 as f32 + y * factor;
    let (sx, sy) = match monitors {
        Some((near, primary)) => {
            to_dips((px, py), (near.rect, factor), (primary.rect, primary.scale))
        }
        None => (px / factor, py / factor),
    };
    [
        sx.round() as i32,
        sy.round() as i32,
        w.round() as i32,
        h.round() as i32,
    ]
}

/// Client DIPs of `window` as screen DIPs.
unsafe fn to_screen(window: HWND, rect: [f32; 4]) -> Rect {
    let factor = crate::frame::layout_dpi(window).max(96) as f32 / 96.0;
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(window, &mut origin);
    client_to_screen_dips(
        (origin.x, origin.y),
        factor,
        rect,
        crate::os::win32::monitors_of(window),
    )
}
/// The page area, the address and the chip of a Rivet window, in screen DIPs.
unsafe fn targets(window: HWND) -> Option<(Rect, Rect, Rect)> {
    let (address, chip) = crate::features::address_bar::anchors(window)?;
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(window, &mut client);
    let factor = crate::frame::layout_dpi(window).max(96) as f32 / 96.0;
    let below = address[1] + address[3];
    let page = [
        0.0,
        below,
        client.right as f32 / factor,
        client.bottom as f32 / factor - below,
    ];
    Some((
        to_screen(window, page),
        to_screen(window, address),
        to_screen(window, chip),
    ))
}

unsafe extern "system" fn get_anchor_rect(
    delegate: *const core::ffi::c_void,
    out: *mut Rect,
) -> *mut Rect {
    let original: GetAnchorRect = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
    let result = original(delegate, out);
    // Bubbles are made on the browser's UI thread, which holds the Rivet windows.
    let windows: Vec<usize> = crate::frame::rivet_windows()
        .into_iter()
        .map(|window| window as usize)
        .collect();
    let Some(window) = bubble_window(
        GetActiveWindow() as usize,
        PAGE_INFO_WINDOW.load(Ordering::Acquire),
        &windows,
    ) else {
        return result;
    };
    let window = window as HWND;
    if result.is_null() {
        return result;
    }
    if let Some((page, address, chip)) = targets(window) {
        let original = *result;
        crate::trace::trace(&format!(
            "bubble anchor: original {original:?} address {address:?} chip {chip:?} client origin {:?} layout dpi {}",
            {
                let mut origin = POINT { x: 0, y: 0 };
                ClientToScreen(window, &mut origin);
                (origin.x, origin.y)
            },
            crate::frame::layout_dpi(window)
        ));
        *result = choose(
            *result,
            page,
            address,
            chip,
            PAGE_INFO.load(Ordering::Acquire),
        );
        crate::trace::trace(&format!("bubble anchor: chose {:?}", *result));
    }
    result
}

/// Wraps the build's `GetAnchorRect`. A build without a row, or whose bytes differ, is left alone.
pub(crate) fn install() {
    let Some(build) = crate::builds::current() else {
        return;
    };
    let Some((get_anchor_rect, prologue)) = anchor_of(build) else {
        return;
    };
    let Some(image) = crate::chromium::image::recorded() else {
        return;
    };
    if unsafe { image.bytes(get_anchor_rect, prologue.len()) } != Some(prologue) {
        crate::trace::trace("bubble anchor: GetAnchorRect is not the recorded one");
        return;
    }
    if ORIGINAL.load(Ordering::Acquire) != 0 {
        return;
    }
    let result = unsafe {
        crate::targets::transaction(|| {
            crate::targets::detour(
                image.base,
                get_anchor_rect,
                prologue,
                get_anchor_rect as *const () as usize,
                &ORIGINAL,
            )
        })
    };
    crate::trace::trace(&format!("bubble anchor: hook {result:?}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: Rect = [100, 144, 1200, 800];
    const ADDRESS: Rect = [300, 108, 600, 28];
    const CHIP: Rect = [310, 108, 22, 28];

    #[test]
    fn fr12_a_chip_over_another_monitor_is_converted_on_the_windows_display() {
        use crate::os::win32::Monitor;
        // A primary at 100 percent and two monitors at 125 percent stacked against its right edge,
        // as on the test machine.
        let primary = Monitor {
            rect: [0, 0, 1440, 900],
            scale: 1.0,
            primary: true,
        };
        let upper = Monitor {
            rect: [1440, -716, 4000, 724],
            scale: 1.25,
            primary: false,
        };
        // The window's display is the upper monitor; its chip 16 DIPs below the client origin
        // reaches over the lower one, and still lands where the browser's own views would.
        let chip = super::client_to_screen_dips(
            (1500, 710),
            1.25,
            [6.0, 16.0, 22.0, 28.0],
            Some((upper, primary)),
        );
        let (ox, oy) = super::to_dips((1500.0, 710.0), (upper.rect, 1.25), (primary.rect, 1.0));
        assert_eq!(
            chip,
            [
                (ox + 6.0).round() as i32,
                (oy + 16.0).round() as i32,
                22,
                28
            ]
        );
    }
    #[test]
    fn fr12_bubble_at_the_monitor_edge_moves_to_the_address_bar() {
        // Brave's save bubble was seen at x=4863, past the window.
        assert_eq!(choose([4863, 0, 0, 0], PAGE, ADDRESS, CHIP, false), ADDRESS);
        assert_eq!(choose([0, 0, 0, 0], PAGE, ADDRESS, CHIP, false), ADDRESS);
    }

    #[test]
    fn fr12_bubble_in_the_window_corner_moves_to_the_address_bar() {
        assert_eq!(
            choose([100, 100, 20, 20], PAGE, ADDRESS, CHIP, false),
            ADDRESS
        );
    }

    #[test]
    fn fr12_bubble_anchored_in_the_page_keeps_the_browsers_place() {
        let inside = [500, 400, 40, 20];
        assert_eq!(choose(inside, PAGE, ADDRESS, CHIP, false), inside);
    }

    #[test]
    fn fr12_page_info_anchors_at_the_security_chip() {
        assert_eq!(choose([4863, 0, 0, 0], PAGE, ADDRESS, CHIP, true), CHIP);
        assert_eq!(choose([500, 400, 40, 20], PAGE, ADDRESS, CHIP, true), CHIP);
        assert!(with_page_info(std::ptr::null_mut(), || PAGE_INFO.load(Ordering::Acquire)));
        assert!(!PAGE_INFO.load(Ordering::Acquire));
    }

    #[test]
    fn fr12_screen_dips_follow_chromiums_display_layout_on_mixed_scales() {
        // A 200% primary of 2880 x 1800 and a 125% monitor to its right, 725 pixels lower.
        let primary = ([0, 0, 2880, 1800], 2.0);
        let right = ([2880, 725, 5440, 2165], 1.25);
        assert_eq!(dip_origin(primary, primary), (0.0, 0.0));
        assert_eq!(dip_origin(right, primary), (1440.0, 362.5));
        // The window's client origin at (9000, 1000) pixels, past that monitor's edge, as Chromium
        // put the window's own anchor in the trace: 1440 + 6120 / 1.25 and 362.5 + 275 / 1.25.
        assert_eq!(to_dips((9000.0, 1000.0), right, primary), (6336.0, 582.5));
        // Monitors left of, above and below the primary, and a single monitor.
        assert_eq!(
            dip_origin(([-1920, 0, 0, 1080], 1.0), primary),
            (-1920.0, 0.0)
        );
        assert_eq!(
            dip_origin(([0, -1080, 1920, 0], 1.0), primary),
            (0.0, -1080.0)
        );
        assert_eq!(
            dip_origin(([500, 1800, 2420, 2880], 1.0), primary),
            (250.0, 900.0)
        );
        assert_eq!(to_dips((100.0, 50.0), primary, primary), (50.0, 25.0));
    }

    #[test]
    fn fr12_a_bubble_of_an_inactive_window_still_anchors_at_its_address_row() {
        // The active window is Rivet's: its bubble goes there.
        assert_eq!(bubble_window(7, 0, &[7, 9]), Some(7));
        // No Rivet window is active, as when a page asks for a permission in the background.
        assert_eq!(bubble_window(0, 0, &[9]), Some(9));
        assert_eq!(bubble_window(3, 0, &[9]), Some(9));
        // Page info of a chip names its window.
        assert_eq!(bubble_window(0, 9, &[7, 9]), Some(9));
        // With several windows and nothing to tell them apart, the browser's own place stays.
        assert_eq!(bubble_window(0, 0, &[7, 9]), None);
        assert_eq!(bubble_window(0, 0, &[]), None);
    }

    #[test]
    fn fr12_bubble_anchor_rows_are_recorded_per_build_and_unverified() {
        let chrome = row_for("Google Chrome", [156, 0, 8078, 74]).expect("Chrome .74 has a row");
        assert_eq!(chrome.get_anchor_rect, 0x0AA7_D550);
        let brave = row_for("Brave", [156, 1, 98, 63]).expect("Brave .63 has a row");
        assert_eq!(brave.get_anchor_rect, 0x0BED_C9D0);
        assert!(ROWS
            .iter()
            .all(|row| !row.verified && row.prologue.len() >= 14));
        for (browser, version, at) in [
            ("Google Chrome", [156, 0, 8078, 80], 0x0AA7_2C00),
            ("Google Chrome", [157, 0, 8103, 38], 0x0AF2_96E0),
            ("Brave", [157, 1, 99, 55], 0x0C3B_8D60),
            ("Microsoft Edge", [156, 0, 4314, 58], 0x0330_BF40),
            ("Naver Whale", [5, 39, 412, 57], 0x0903_7B10),
            ("Samsung Internet", [30, 0, 4, 29], 0x090C_B920),
        ] {
            let row = row_for(browser, version).expect("every supported build has a row");
            assert_eq!(row.get_anchor_rect, at, "{browser}");
        }
        for row in ROWS {
            let build = crate::builds::all()
                .iter()
                .find(|build| build.browser == row.browser && build.version == row.version);
            assert!(
                build.is_some(),
                "{} {:?} is in the build table",
                row.browser,
                row.version
            );
        }
    }
}
