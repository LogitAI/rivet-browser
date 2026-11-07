//! What a Rivet window may do to the person's pointer and keyboard focus.
//!
//! A window takes the mouse with `SetCapture` while a drag runs, so the drag keeps its pointer when
//! it leaves the window. That is only right when a person pressed a button on a window they can
//! see. A test drives off-screen windows with posted messages, and a capture taken then would hold
//! the person's real mouse for a window nobody can see. So a window captures only when it lies on
//! a monitor and the button is really held down, and never in a test run.
//!
//! A test run sets `RIVET_KEEP_OFFSCREEN`. Under it every Rivet window, and every top-level window
//! of a browser Rivet runs, stays off every monitor and is never activated, brought forward or
//! given the keyboard focus of the desktop.

/// Set by a test run: windows stay off-screen and are never activated.
pub const KEEP_OFFSCREEN_VARIABLE: &str = "RIVET_KEEP_OFFSCREEN";

/// How far right of every monitor a window is put, in pixels.
pub const OFFSCREEN_MARGIN: i32 = 4000;

/// `SWP_NOSIZE`, `SWP_NOMOVE`, `SWP_NOACTIVATE`, `SWP_SHOWWINDOW` and `SWP_HIDEWINDOW`.
pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOMOVE: u32 = 0x0002;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const SWP_SHOWWINDOW: u32 = 0x0040;
pub const SWP_HIDEWINDOW: u32 = 0x0080;

/// Whether the switch's value asks for the test mode. An empty value or `0` does not.
pub fn keep_offscreen_value(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

/// Whether this process runs under a test that keeps its windows off-screen.
pub fn keep_offscreen() -> bool {
    keep_offscreen_value(std::env::var_os(KEEP_OFFSCREEN_VARIABLE).as_deref())
}

/// What decides whether a press may take the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureCheck {
    /// Some part of the window lies on a monitor and the window is visible.
    pub on_monitor: bool,
    /// The physical button is held down now. A posted press leaves it up.
    pub button_down: bool,
    /// The process runs under a test that keeps its windows off-screen.
    pub keep_offscreen: bool,
}

/// Whether a window may capture the mouse for a drag.
pub fn may_capture(check: CaptureCheck) -> bool {
    check.on_monitor && check.button_down && !check.keep_offscreen
}

/// A rectangle in screen pixels: left, top, right, bottom.
pub type Rect = [i32; 4];

fn overlaps(a: Rect, b: Rect) -> bool {
    a[2] > a[0] && a[3] > a[1] && a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

/// Where a window at `rect` goes so it touches no monitor, or `None` when it already touches
/// none. `screen` bounds every monitor. The window keeps its height on the screen and moves right
/// of the last monitor, so nothing of it shows wherever its top is.
pub fn offscreen_spot(rect: Rect, screen: Rect) -> Option<(i32, i32)> {
    overlaps(rect, screen).then_some((screen[2] + OFFSCREEN_MARGIN, rect[1]))
}

/// The size, in pixels, of the window a test run starts. Its pages and captures are laid out for it.
pub const TEST_WINDOW_SIZE: (i32, i32) = (1600, 1000);

/// Where the window a test run starts is restored to: right of every monitor in `screen`, level
/// with its top, at [`TEST_WINDOW_SIZE`].
pub fn test_window_place(screen: Rect) -> Rect {
    let left = screen[2] + OFFSCREEN_MARGIN;
    let top = screen[1];
    [
        left,
        top,
        left + TEST_WINDOW_SIZE.0,
        top + TEST_WINDOW_SIZE.1,
    ]
}

/// Whether a test run restores a top-level window now. A test browser shows each new window
/// minimized, so it lies on no monitor while it is new. Such a window is restored once, the first
/// time it is seen minimized before it was ever seen restored; a window a test minimizes later
/// stays minimized.
pub fn restores_first_show(minimized: bool, seen_before: bool) -> bool {
    minimized && !seen_before
}

/// A `WINDOWPOS` request: its flags, place and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceRequest {
    pub flags: u32,
    pub x: i32,
    pub y: i32,
    pub cx: i32,
    pub cy: i32,
}

/// The request a test run lets through for a top-level window now at `current`: it never
/// activates, and a window that is or becomes visible lands off every monitor. `visible` is whether
/// the window is visible before the change.
pub fn keep_off_monitors(
    request: PlaceRequest,
    current: Rect,
    visible: bool,
    screen: Rect,
) -> PlaceRequest {
    let mut out = request;
    out.flags |= SWP_NOACTIVATE;
    let shown =
        (visible || request.flags & SWP_SHOWWINDOW != 0) && request.flags & SWP_HIDEWINDOW == 0;
    if !shown {
        return out;
    }
    let (x, y) = if request.flags & SWP_NOMOVE != 0 {
        (current[0], current[1])
    } else {
        (request.x, request.y)
    };
    let (cx, cy) = if request.flags & SWP_NOSIZE != 0 {
        (current[2] - current[0], current[3] - current[1])
    } else {
        (request.cx, request.cy)
    };
    if let Some((left, top)) = offscreen_spot([x, y, x + cx, y + cy], screen) {
        out.flags &= !SWP_NOMOVE;
        out.x = left;
        out.y = top;
    }
    out
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_LBUTTON, VK_RBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, GetWindowRect, IsWindowVisible, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        SM_SWAPBUTTON, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    /// Whether any part of the visible window lies on a monitor.
    ///
    /// # Safety
    ///
    /// `window` must be a window handle, or null; it is only passed to Windows.
    pub unsafe fn on_monitor(window: HWND) -> bool {
        let mut rect: RECT = std::mem::zeroed();
        IsWindowVisible(window) != 0
            && GetWindowRect(window, &mut rect) != 0
            && !MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_null()
    }

    /// Whether the primary button is physically held down. `GetAsyncKeyState` reads the physical
    /// buttons, so with swapped buttons the primary one is the right one.
    ///
    /// # Safety
    ///
    /// Calls Windows only; it has no other requirement.
    pub unsafe fn primary_button_down() -> bool {
        let key = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        (GetAsyncKeyState(key as i32) as u16 & 0x8000) != 0
    }

    /// The rectangle that bounds every monitor, in the calling thread's pixels.
    ///
    /// # Safety
    ///
    /// Calls Windows only; it has no other requirement.
    pub unsafe fn screen_bounds() -> Rect {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        [
            left,
            top,
            left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        ]
    }

    /// Takes the mouse for a drag on `window` when [`may_capture`] allows it. Returns whether it
    /// did. A drag without the capture still gets every message sent to its own window.
    ///
    /// # Safety
    ///
    /// `window` must be a window handle, or null; it is only passed to Windows.
    pub unsafe fn capture(window: HWND) -> bool {
        let allowed = may_capture(CaptureCheck {
            on_monitor: on_monitor(window),
            button_down: primary_button_down(),
            keep_offscreen: keep_offscreen(),
        });
        if allowed {
            SetCapture(window);
        }
        allowed
    }

    /// Takes the mouse for an open menu on `window`, so a click outside the menu closes it. A menu
    /// opens when a click ends, so no button is held: only the window's place and the test switch
    /// decide. Returns whether it did.
    ///
    /// # Safety
    ///
    /// `window` must be a window handle, or null; it is only passed to Windows.
    pub unsafe fn capture_for_menu(window: HWND) -> bool {
        let allowed = may_capture(CaptureCheck {
            on_monitor: on_monitor(window),
            button_down: true,
            keep_offscreen: keep_offscreen(),
        });
        if allowed {
            SetCapture(window);
        }
        allowed
    }

    /// Whether a press on `window` may start a drag that the system runs, such as a move or a
    /// size of the window. That drag takes the mouse as a capture does.
    ///
    /// # Safety
    ///
    /// `window` must be a window handle, or null; it is only passed to Windows.
    pub unsafe fn may_start_system_drag(window: HWND) -> bool {
        may_capture(CaptureCheck {
            on_monitor: on_monitor(window),
            button_down: primary_button_down(),
            keep_offscreen: keep_offscreen(),
        })
    }

    /// Gives the mouse back when `window` holds it. A capture the browser took for something else
    /// is left alone.
    ///
    /// # Safety
    ///
    /// `window` must be a window handle, or null; it is only passed to Windows.
    pub unsafe fn release(window: HWND) {
        if GetCapture() == window {
            ReleaseCapture();
        }
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = [0, 0, 3840, 1080];

    #[test]
    fn nfr5_offscreen_window_never_captures_the_mouse() {
        assert!(!may_capture(CaptureCheck {
            on_monitor: false,
            button_down: true,
            keep_offscreen: false,
        }));
    }

    #[test]
    fn nfr5_posted_press_never_captures_the_mouse() {
        assert!(!may_capture(CaptureCheck {
            on_monitor: true,
            button_down: false,
            keep_offscreen: false,
        }));
    }

    #[test]
    fn nfr5_test_run_never_captures_the_mouse() {
        assert!(!may_capture(CaptureCheck {
            on_monitor: true,
            button_down: true,
            keep_offscreen: true,
        }));
    }

    #[test]
    fn nfr5_real_press_on_a_monitor_captures_the_mouse() {
        assert!(may_capture(CaptureCheck {
            on_monitor: true,
            button_down: true,
            keep_offscreen: false,
        }));
    }

    #[test]
    fn nfr5_test_window_is_restored_off_every_monitor_at_its_test_size() {
        let place = test_window_place(SCREEN);
        assert_eq!(
            place,
            [
                3840 + OFFSCREEN_MARGIN,
                0,
                3840 + OFFSCREEN_MARGIN + 1600,
                1000
            ]
        );
        assert_eq!(offscreen_spot(place, SCREEN), None);
        let left_monitor = [-1920, -200, 1920, 1080];
        assert_eq!(
            offscreen_spot(test_window_place(left_monitor), left_monitor),
            None
        );
    }

    #[test]
    fn nfr5_minimized_first_show_is_restored_once() {
        assert!(restores_first_show(true, false));
        assert!(!restores_first_show(true, true));
        assert!(!restores_first_show(false, false));
        assert!(!restores_first_show(false, true));
    }

    #[test]
    fn nfr5_test_switch_is_on_only_with_a_value() {
        assert!(keep_offscreen_value(Some("1".as_ref())));
        assert!(!keep_offscreen_value(Some("".as_ref())));
        assert!(!keep_offscreen_value(Some("0".as_ref())));
        assert!(!keep_offscreen_value(None));
    }

    #[test]
    fn nfr5_window_on_a_monitor_moves_right_of_every_monitor() {
        assert_eq!(
            offscreen_spot([100, 200, 900, 800], SCREEN),
            Some((3840 + OFFSCREEN_MARGIN, 200))
        );
        assert_eq!(offscreen_spot([9000, 1000, 10600, 2000], SCREEN), None);
        assert_eq!(offscreen_spot([100, 100, 100, 300], SCREEN), None);
    }

    #[test]
    fn nfr5_new_browser_window_shown_on_a_monitor_lands_off_screen_without_activation() {
        let request = PlaceRequest {
            flags: SWP_SHOWWINDOW,
            x: 120,
            y: 80,
            cx: 1200,
            cy: 800,
        };
        let kept = keep_off_monitors(request, [0, 0, 0, 0], false, SCREEN);
        assert_ne!(kept.flags & SWP_NOACTIVATE, 0);
        assert_eq!((kept.x, kept.y), (3840 + OFFSCREEN_MARGIN, 80));
        assert_eq!((kept.cx, kept.cy), (1200, 800));
    }

    #[test]
    fn nfr5_show_without_a_move_still_lands_off_screen() {
        let request = PlaceRequest {
            flags: SWP_SHOWWINDOW | SWP_NOMOVE | SWP_NOSIZE,
            x: 0,
            y: 0,
            cx: 0,
            cy: 0,
        };
        let kept = keep_off_monitors(request, [300, 300, 700, 500], false, SCREEN);
        assert_eq!(kept.flags & SWP_NOMOVE, 0);
        assert_ne!(kept.flags & SWP_NOSIZE, 0);
        assert_eq!((kept.x, kept.y), (3840 + OFFSCREEN_MARGIN, 300));
    }

    #[test]
    fn nfr5_off_screen_or_hidden_windows_keep_their_place() {
        let away = PlaceRequest {
            flags: 0,
            x: 9000,
            y: 1000,
            cx: 1600,
            cy: 1000,
        };
        assert_eq!(
            keep_off_monitors(away, [0, 0, 10, 10], true, SCREEN),
            PlaceRequest {
                flags: SWP_NOACTIVATE,
                ..away
            }
        );
        let hidden = PlaceRequest {
            flags: SWP_NOSIZE,
            x: 10,
            y: 10,
            cx: 0,
            cy: 0,
        };
        let kept = keep_off_monitors(hidden, [0, 0, 400, 400], false, SCREEN);
        assert_eq!((kept.x, kept.y), (10, 10));
        let hiding = PlaceRequest {
            flags: SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE,
            ..hidden
        };
        let kept = keep_off_monitors(hiding, [0, 0, 400, 400], true, SCREEN);
        assert_ne!(kept.flags & SWP_NOMOVE, 0);
    }
}
