//! Developer mode of the page a Rivet window shows. The window's active tab is read from Chromium's
//! tab strip model, and `dev_mode` decides from its address and the person's choices.

use std::sync::Mutex;

use windows_sys::Win32::Foundation::HWND;

use crate::dev_mode::{Action, Modes};

static MODES: Mutex<Modes> = Mutex::new(Modes::new());

fn modes() -> std::sync::MutexGuard<'static, Modes> {
    MODES.lock().unwrap_or_else(|error| error.into_inner())
}

/// The active tab of `window` and its address.
unsafe fn active_page(window: HWND) -> Option<(usize, String)> {
    crate::features::tabs::open_tabs(window)?
        .into_iter()
        .find(|tab| tab.saved.active)
        .map(|tab| (tab.id, tab.saved.url))
}

/// Whether the page of `window` is in developer mode.
pub(crate) fn window_in_dev_mode(window: HWND) -> bool {
    match unsafe { active_page(window) } {
        Some((tab, address)) => modes().is_on(tab, &address),
        None => false,
    }
}

/// Switches developer mode of the page `window` shows, as the person's choice.
pub(crate) unsafe fn toggle_dev_mode(window: HWND) -> bool {
    let Some((tab, address)) = active_page(window) else { return false };
    let on = modes().toggle(tab, &address);
    crate::trace::trace(&format!("developer mode of window {:#x} is {on}", window as usize));
    crate::hooks::page_area::relayout(window);
    windows_sys::Win32::Graphics::Gdi::InvalidateRect(window, std::ptr::null(), 0);
    crate::frame::update_appbar(window);
    true
}

/// The buttons the developer bar of this build offers, with the command each runs.
pub(crate) fn dev_actions() -> Vec<(Action, u32)> {
    let dev = &crate::builds::get().toolbar.dev;
    crate::dev_mode::bar_buttons(&[(Action::Inspect, dev.inspect), (Action::Console, dev.console)])
}
