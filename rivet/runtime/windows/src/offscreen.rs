//! Keeps a test run's browser windows away from the person using the machine.
//!
//! A test sets `RIVET_KEEP_OFFSCREEN`. Under it every top-level window of the browser's UI thread,
//! the Rivet window, a Mini Window the browser makes, a page info bubble or a permission prompt,
//! is created off every monitor, is put back off every monitor whenever it moves or shows, and is
//! never activated. A window the browser makes at its default place would otherwise open on a
//! monitor, shown and active, and take the keyboard from whatever the person was doing.
//!
//! Outside a test, the only rule here is that a press nobody really made never starts the system's
//! move or size loop, which holds the mouse until the button comes up.

use std::cell::Cell;

use rivet_platform_windows::focus;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetDesktopWindow, GetWindowRect, IsWindowVisible, SetWindowPos,
    SetWindowsHookExW, CBT_CREATEWNDW, CWPRETSTRUCT, CWPSTRUCT, CW_USEDEFAULT, GA_PARENT,
    HCBT_ACTIVATE, HCBT_CREATEWND, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, MA_NOACTIVATE, SC_MOVE,
    SC_SIZE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WH_CALLWNDPROC, WH_CALLWNDPROCRET, WH_CBT,
    WINDOWPOS, WM_MOUSEACTIVATE, WM_NCLBUTTONDOWN, WM_SHOWWINDOW, WM_SYSCOMMAND,
    WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING, WS_CHILD,
};

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
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
        );
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

/// Before a window is made: a top-level one starts off every monitor. Before a window is
/// activated: it is not, so no test window ever takes the foreground or the keyboard.
unsafe extern "system" fn cbt_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HCBT_ACTIVATE as i32 {
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
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// After a window moved or showed: one that still reached a monitor is put away at once.
unsafe extern "system" fn after_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam != 0 {
        let done = &*(lparam as *const CWPRETSTRUCT);
        if done.message == WM_WINDOWPOSCHANGED || done.message == WM_SHOWWINDOW {
            put_away(done.hwnd);
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
