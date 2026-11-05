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

/// Runs `open` with the next bubble anchored at the security chip.
pub(crate) fn with_page_info<R>(open: impl FnOnce() -> R) -> R {
    PAGE_INFO.store(true, Ordering::Release);
    let result = open();
    PAGE_INFO.store(false, Ordering::Release);
    result
}

static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
type GetAnchorRect = unsafe extern "system" fn(*const core::ffi::c_void, *mut Rect) -> *mut Rect;

/// Client DIPs of `window` as screen DIPs.
unsafe fn to_screen(window: HWND, [x, y, w, h]: [f32; 4]) -> Rect {
    let factor = crate::frame::layout_dpi(window).max(96) as f32 / 96.0;
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(window, &mut origin);
    let (ox, oy) = (origin.x as f32 / factor, origin.y as f32 / factor);
    [
        (ox + x).round() as i32,
        (oy + y).round() as i32,
        w.round() as i32,
        h.round() as i32,
    ]
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
    // Bubbles are made on the browser's UI thread, where the window they belong to is active.
    let window = GetActiveWindow();
    if result.is_null() || window.is_null() || !crate::frame::rivet_windows().contains(&window) {
        return result;
    }
    if let Some((page, address, chip)) = targets(window) {
        *result = choose(
            *result,
            page,
            address,
            chip,
            PAGE_INFO.load(Ordering::Acquire),
        );
    }
    result
}

/// Wraps the build's `GetAnchorRect`. A build without a row, or whose bytes differ, is left alone.
pub(crate) fn install() {
    let Some(build) = crate::builds::current() else {
        return;
    };
    let Some(row) = row_for(build.browser, build.version) else {
        return;
    };
    let Some(image) = crate::chromium::image::recorded() else {
        return;
    };
    if unsafe { image.bytes(row.get_anchor_rect, row.prologue.len()) } != Some(row.prologue) {
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
                row.get_anchor_rect,
                row.prologue,
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
        assert!(with_page_info(|| PAGE_INFO.load(Ordering::Acquire)));
        assert!(!PAGE_INFO.load(Ordering::Acquire));
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
