//! Keeps a test run's browser windows away from the person using the machine.
//!
//! A test sets `RIVET_KEEP_OFFSCREEN`. Under it every top-level window of the browser's UI thread,
//! the Rivet window, a Mini Window the browser makes, a page info bubble or a permission prompt,
//! is created off every monitor, is put back off every monitor whenever it moves or shows, and is
//! never activated. A window the browser makes at its default place would otherwise open on a
//! monitor, shown and active, and take the keyboard from whatever the person was doing.
//!
//! A test browser is started minimized and inactive, with no place or size on its command line,
//! so Chromium never activates its windows as it shows them. Each new window therefore first shows
//! minimized, on no monitor, and is restored here once, off every monitor and without activation.
//!
//! Outside a test, the only rule here is that a press nobody really made never starts the system's
//! move or size loop, which holds the mouse until the button comes up.

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

use rivet_platform_windows::focus;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetDesktopWindow, GetPropW, GetWindowPlacement, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, RegisterWindowMessageW, RemovePropW,
    SendMessageTimeoutW, SetPropW, SetWindowPlacement, SetWindowPos, SetWindowsHookExW,
    ShowWindowAsync, CBT_CREATEWNDW, CWPRETSTRUCT, CWPSTRUCT, CW_USEDEFAULT, GA_PARENT,
    HCBT_ACTIVATE, HCBT_CREATEWND, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, MA_NOACTIVATE, SC_MOVE,
    SC_SIZE, SMTO_ABORTIFHUNG, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    SW_SHOWNOACTIVATE, WH_CALLWNDPROC, WH_CALLWNDPROCRET, WH_CBT, WINDOWPLACEMENT, WINDOWPOS,
    WM_MOUSEACTIVATE, WM_NCDESTROY, WM_NCLBUTTONDOWN, WM_SHOWWINDOW, WM_SYSCOMMAND,
    WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING, WS_CHILD,
};

/// The message that asks the browser's UI thread to start keeping its windows off-screen.
const KEEP_MESSAGE: &str = "Rivet.KeepOffscreen";

/// The window property that marks a test window as seen shown, so its first minimized show is
/// restored once and a later minimize is left alone.
const SEEN_PROPERTY: &str = "Rivet.TestWindowSeen";

/// How long the attach waits for the UI thread to take the hooks, in milliseconds.
const KEEP_WAIT_MS: u32 = 2000;

/// The registered number of [`KEEP_MESSAGE`], looked up once.
static KEEP_NUMBER: AtomicU32 = AtomicU32::new(0);

unsafe fn keep_message() -> u32 {
    let known = KEEP_NUMBER.load(Ordering::Relaxed);
    if known != 0 {
        return known;
    }
    let name: Vec<u16> = KEEP_MESSAGE.encode_utf16().chain(Some(0)).collect();
    let number = RegisterWindowMessageW(name.as_ptr());
    KEEP_NUMBER.store(number, Ordering::Relaxed);
    number
}

/// Under a test, the first thing an attach does once the window is subclassed: the window goes
/// off every monitor at once, and the UI thread takes the no-activate hooks before it handles any
/// message already posted to it. The hooks belong on that thread, since a thread hook ends with
/// the thread that set it and this one ends when the attach returns. A sent message is handled
/// before posted ones, so this is the earliest the UI thread can run them.
pub(crate) unsafe fn keep_from_attach(window: HWND) {
    if !focus::keep_offscreen() {
        return;
    }
    put_away(window);
    let mut result = 0usize;
    if SendMessageTimeoutW(
        window,
        keep_message(),
        0,
        0,
        SMTO_ABORTIFHUNG,
        KEEP_WAIT_MS,
        &mut result,
    ) == 0
    {
        crate::frame::log("rivet: the UI thread did not take the off-screen hooks in time");
    }
}

thread_local! {
    /// Whether this thread's windows are kept off-screen already.
    static KEPT: Cell<bool> = const { Cell::new(false) };
}

/// Whether `window` is a top-level window. Child windows move with their parent, and a
/// message-only window is never shown.
unsafe fn top_level(window: HWND) -> bool {
    GetAncestor(window, GA_PARENT) == GetDesktopWindow()
}

/// Starts keeping every top-level window of the calling thread off-screen and inactive, when the
/// process runs under a test. Runs on the browser's UI thread, which owns all its windows.
pub(crate) unsafe fn keep_thread() {
    if !focus::keep_offscreen() || KEPT.with(Cell::get) {
        return;
    }
    let thread = GetCurrentThreadId();
    let cbt = SetWindowsHookExW(WH_CBT, Some(cbt_hook), std::ptr::null_mut(), thread);
    let call = SetWindowsHookExW(
        WH_CALLWNDPROC,
        Some(call_hook),
        std::ptr::null_mut(),
        thread,
    );
    let after = SetWindowsHookExW(
        WH_CALLWNDPROCRET,
        Some(after_hook),
        std::ptr::null_mut(),
        thread,
    );
    if cbt.is_null() || call.is_null() || after.is_null() {
        crate::frame::log("rivet: could not keep the test windows off-screen");
    }
    KEPT.with(|kept| kept.set(true));
}

/// Under a test, puts a visible top-level window that touches a monitor off every monitor, without
/// activating it or changing its size. Outside a test it does nothing.
pub(crate) unsafe fn put_away(window: HWND) {
    let mut rect: RECT = std::mem::zeroed();
    if !focus::keep_offscreen()
        || !top_level(window)
        || IsWindowVisible(window) == 0
        || GetWindowRect(window, &mut rect) == 0
    {
        return;
    }
    let current = [rect.left, rect.top, rect.right, rect.bottom];
    if let Some((x, y)) = focus::offscreen_spot(current, focus::screen_bounds()) {
        // From another thread the move is queued to the window's own thread, so this thread never
        // waits on the browser's UI thread.
        let foreign =
            GetWindowThreadProcessId(window, std::ptr::null_mut()) != GetCurrentThreadId();
        let queued = if foreign { SWP_ASYNCWINDOWPOS } else { 0 };
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER | queued,
        );
    }
}

fn seen_key() -> Vec<u16> {
    SEEN_PROPERTY.encode_utf16().chain(Some(0)).collect()
}

/// Whether `window` was seen shown already, and marks it seen.
unsafe fn take_first_sight(window: HWND) -> bool {
    let key = seen_key();
    let seen = !GetPropW(window, key.as_ptr()).is_null();
    if !seen {
        SetPropW(window, key.as_ptr(), 1usize as _);
    }
    seen
}

/// A test browser starts with `SW_SHOWMINNOACTIVE`, so every new window first shows minimized and
/// lies on no monitor. This restores such a window once, without activation. The restore goes
/// through `WM_WINDOWPOSCHANGING`, which puts it off every monitor. A window first seen restored is
/// only marked, so a minimize a test asks for later stays.
unsafe fn restore_first_show(window: HWND) {
    if !focus::keep_offscreen() || !top_level(window) || IsWindowVisible(window) == 0 {
        return;
    }
    let minimized = IsIconic(window) != 0;
    if focus::restores_first_show(minimized, take_first_sight(window)) {
        // Queued, since this runs inside the browser's own show or move of the window.
        ShowWindowAsync(window, SW_SHOWNOACTIVATE);
    }
}

/// On the UI thread as the runtime attaches: the window Rivet started shows minimized, and is
/// restored right of every monitor at the test size, without activation. Its restored place is
/// set in workspace coordinates, which differ from screen ones only by the taskbar, far less than
/// the margin that keeps it off every monitor.
unsafe fn restore_attached(window: HWND) {
    if !focus::keep_offscreen() || !top_level(window) || IsWindowVisible(window) == 0 {
        return;
    }
    let minimized = IsIconic(window) != 0;
    if !focus::restores_first_show(minimized, take_first_sight(window)) {
        return;
    }
    let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if GetWindowPlacement(window, &mut placement) == 0 {
        ShowWindowAsync(window, SW_SHOWNOACTIVATE);
        return;
    }
    let [left, top, right, bottom] = focus::test_window_place(focus::screen_bounds());
    placement.rcNormalPosition = RECT {
        left,
        top,
        right,
        bottom,
    };
    placement.flags = 0;
    placement.showCmd = SW_SHOWNOACTIVATE as u32;
    if SetWindowPlacement(window, &placement) == 0 {
        crate::frame::log("rivet: could not restore the minimized test window");
    }
}

/// Rewrites a `WM_WINDOWPOSCHANGING` request of a top-level window so it never activates the
/// window and never shows it on a monitor.
unsafe fn keep_place(window: HWND, place: *mut WINDOWPOS) {
    if place.is_null() || !top_level(window) {
        return;
    }
    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(window, &mut rect);
    let request = focus::PlaceRequest {
        flags: (*place).flags,
        x: (*place).x,
        y: (*place).y,
        cx: (*place).cx,
        cy: (*place).cy,
    };
    let kept = focus::keep_off_monitors(
        request,
        [rect.left, rect.top, rect.right, rect.bottom],
        IsWindowVisible(window) != 0,
        focus::screen_bounds(),
    );
    (*place).flags = kept.flags;
    (*place).x = kept.x;
    (*place).y = kept.y;
}

thread_local! {
    /// The windows a row asked to activate, on a test desktop of their own.
    static ALLOWED: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Activates `window` for a row that needs a focused window, such as a password save offer or
/// typed text, when the run lives on a test desktop of its own that is not the input desktop.
/// Activation there reaches no person's screen or keyboard. Anywhere else it stays blocked and
/// nothing happens. Returns whether the window was activated.
pub(crate) unsafe fn activate_on_test_desktop(window: HWND) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetActiveWindow;
    if !focus::activation_allowed() {
        return false;
    }
    ALLOWED.with(|allowed| {
        let mut allowed = allowed.borrow_mut();
        if !allowed.contains(&(window as usize)) {
            allowed.push(window as usize);
        }
    });
    !SetActiveWindow(window).is_null()
        || windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow() == window
}

/// Focuses `window`, or with `popup` its top owned window such as the password save offer, for a
/// row that needs a focused window. Both go through the one test desktop gate before any window is
/// looked up. Returns whether a window was activated.
pub(crate) unsafe fn focus_for_row(window: HWND, popup: bool) -> bool {
    focus_target(window, popup, focus::activation_allowed(), || {
        owned_popup(window)
    })
    .is_some_and(|target| activate_on_test_desktop(target))
}

/// The window a row's focus request goes to: none unless the gate allows activation, else
/// `window` itself or, with `popup`, what `find` returns.
fn focus_target(
    window: HWND,
    popup: bool,
    allowed: bool,
    find: impl FnOnce() -> Option<HWND>,
) -> Option<HWND> {
    if !allowed {
        return None;
    }
    if popup {
        find()
    } else {
        Some(window)
    }
}

/// The top window a browser window shows over itself, such as its password save offer: the first
/// visible window, in the thread's top-down order, that `window` owns.
pub(crate) unsafe fn owned_popup(window: HWND) -> Option<HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, GetWindow, GW_OWNER};
    unsafe extern "system" fn visit(found: HWND, list: LPARAM) -> i32 {
        let list = &mut *(list as *mut Vec<(usize, usize, bool)>);
        list.push((
            found as usize,
            GetWindow(found, GW_OWNER) as usize,
            IsWindowVisible(found) != 0,
        ));
        1
    }
    let thread = GetWindowThreadProcessId(window, std::ptr::null_mut());
    let mut list: Vec<(usize, usize, bool)> = Vec::new();
    EnumThreadWindows(thread, Some(visit), &mut list as *mut _ as LPARAM);
    first_owned(&list, window as usize).map(|found| found as HWND)
}

/// The first of `windows`, given top-down as (window, owner, visible), that is visible and owned by
/// `owner`.
fn first_owned(windows: &[(usize, usize, bool)], owner: usize) -> Option<usize> {
    windows
        .iter()
        .find(|(found, by, visible)| *visible && *by == owner && *found != owner)
        .map(|(found, _, _)| *found)
}

/// Before a window is made: a top-level one starts off every monitor. Before a window is
/// activated: it is not, so no test window ever takes the foreground or the keyboard.
unsafe extern "system" fn cbt_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HCBT_ACTIVATE as i32 && !ALLOWED.with(|allowed| allowed.borrow().contains(&wparam)) {
        return 1;
    }
    if code == HCBT_CREATEWND as i32 && lparam != 0 {
        let create = (*(lparam as *mut CBT_CREATEWNDW)).lpcs;
        if !create.is_null() && ((*create).style as u32) & WS_CHILD == 0 {
            let screen = focus::screen_bounds();
            let create = &mut *create;
            let placed = create.x != CW_USEDEFAULT && create.y != CW_USEDEFAULT;
            let (cx, cy) = (
                if create.cx == CW_USEDEFAULT {
                    1
                } else {
                    create.cx.max(1)
                },
                if create.cy == CW_USEDEFAULT {
                    1
                } else {
                    create.cy.max(1)
                },
            );
            let spot = if placed {
                focus::offscreen_spot([create.x, create.y, create.x + cx, create.y + cy], screen)
            } else {
                Some((screen[2] + focus::OFFSCREEN_MARGIN, screen[1]))
            };
            if let Some((x, y)) = spot {
                create.x = x;
                create.y = y;
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Every move, size or show request of the thread's windows passes here before the window sees it.
unsafe extern "system" fn call_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        let sent = &*(lparam as *const CWPSTRUCT);
        if sent.message == WM_WINDOWPOSCHANGING {
            keep_place(sent.hwnd, sent.lParam as *mut WINDOWPOS);
        } else if sent.message == WM_NCDESTROY {
            let key = seen_key();
            RemovePropW(sent.hwnd, key.as_ptr());
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// After a window moved or showed: one that still reached a monitor is put away at once, and a new
/// window that showed minimized is restored off every monitor.
unsafe extern "system" fn after_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        let done = &*(lparam as *const CWPRETSTRUCT);
        if done.message == WM_WINDOWPOSCHANGED || done.message == WM_SHOWWINDOW {
            put_away(done.hwnd);
        }
        // Only once the move is done is a minimized show minimized already.
        if done.message == WM_WINDOWPOSCHANGED {
            restore_first_show(done.hwnd);
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// What the subclassed browser window does with a message before anything else. A press that
/// would start the system's move or size loop is dropped when it was posted, or when the window
/// lies off every monitor, since that loop holds the person's mouse. Under a test the window
/// never activates on a click, and its moves keep it off every monitor.
pub(crate) unsafe fn message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    // Registered messages start at 0xC000, so other messages skip the lookup.
    if message >= 0xC000 && message == keep_message() && focus::keep_offscreen() {
        keep_thread();
        put_away(window);
        restore_attached(window);
        return Some(0);
    }
    match message {
        WM_NCLBUTTONDOWN
            if (wparam == HTCAPTION as usize
                || (HTLEFT as usize..=HTBOTTOMRIGHT as usize).contains(&wparam))
                && !focus::may_start_system_drag(window) =>
        {
            Some(0)
        }
        WM_SYSCOMMAND
            if matches!((wparam & 0xfff0) as u32, SC_MOVE | SC_SIZE)
                && (focus::keep_offscreen() || !focus::on_monitor(window)) =>
        {
            Some(0)
        }
        WM_MOUSEACTIVATE if focus::keep_offscreen() => Some(MA_NOACTIVATE as LRESULT),
        WM_WINDOWPOSCHANGING if focus::keep_offscreen() => {
            keep_place(window, lparam as *mut WINDOWPOS);
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr5_focusing_the_save_offer_takes_the_same_test_desktop_gate_as_the_window() {
        let window = 10 as HWND;
        let offer = 50 as HWND;
        // Off a test desktop nothing is focused, and the offer is not even looked up.
        for popup in [false, true] {
            let target = focus_target(window, popup, false, || panic!("looked up"));
            assert_eq!(target, None);
        }
        // On one, the window or its offer.
        assert_eq!(focus_target(window, false, true, || None), Some(window));
        assert_eq!(
            focus_target(window, true, true, || Some(offer)),
            Some(offer)
        );
        // This test's own thread is not on a test desktop, so the real gate refuses both.
        unsafe {
            assert!(!focus_for_row(window, false));
            assert!(!focus_for_row(window, true));
        }
    }

    #[test]
    fn nfr8_the_save_offer_to_focus_is_the_top_visible_window_the_browser_window_owns() {
        let browser = 10;
        let windows = [
            (30, 0, true),
            (40, browser, false),
            (50, browser, true),
            (60, browser, true),
            (browser, 0, true),
        ];
        assert_eq!(first_owned(&windows, browser), Some(50));
        assert_eq!(
            first_owned(&[(30, 0, true), (40, browser, false)], browser),
            None
        );
    }
}
