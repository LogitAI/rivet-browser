//! What Rivet's own address bar reads and does for the tabs of an app window.
//!
//! An app window has no browser toolbar, so Rivet draws the address bar. This module is the one
//! place that bar talks to: the address and title of every tab, and the browser's own navigation
//! commands. The commands go through the window's `WM_COMMAND`, which Chromium routes to its
//! browser command controller like its own menu does, so no build address is needed for them.

use windows_sys::Win32::Foundation::HWND;

/// One tab as the address bar shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PageState {
    /// The tab's index in the window's tab strip model.
    pub index: usize,
    pub active: bool,
    pub title: String,
    /// The visible address: a pending navigation's target until it commits, then the committed one.
    pub url: String,
}

/// A navigation command of the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Back,
    Forward,
    Reload,
    Stop,
}

impl Command {
    /// Chromium's command id (`chrome/app/chrome_command_ids.h`).
    pub(crate) fn id(self) -> usize {
        match self {
            Command::Back => 33000,
            Command::Forward => 33001,
            Command::Reload => 33002,
            Command::Stop => 33006,
        }
    }

    pub(crate) fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "back" => Command::Back,
            "forward" => Command::Forward,
            "reload" => Command::Reload,
            "stop" => Command::Stop,
            _ => return None,
        })
    }
}

/// Every tab of `window` with its title and visible address, in strip order.
pub(crate) unsafe fn pages(window: HWND) -> Option<Vec<PageState>> {
    let base = crate::features::tabs::base()?;
    let model = crate::features::tabs::tab_model(base, window)?;
    let tabs = crate::features::tabs::tabs(window)?;
    Some(
        tabs.into_iter()
            .map(|tab| {
                let contents = crate::features::tabs::contents_at(base, model, tab.index as i32);
                PageState {
                    index: tab.index,
                    active: tab.active,
                    title: crate::features::tabs::page_title(contents),
                    url: crate::features::tabs::page_address(contents),
                }
            })
            .collect(),
    )
}

/// Runs a navigation command on the active tab of `window`, on the browser UI thread.
pub(crate) unsafe fn run(window: HWND, command: Command) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_COMMAND};
    SendMessageW(window, WM_COMMAND, command.id(), 0);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_window_address_bar_commands_are_chromiums_own_ids() {
        assert_eq!(Command::parse("back").map(Command::id), Some(33000));
        assert_eq!(Command::parse("forward").map(Command::id), Some(33001));
        assert_eq!(Command::parse("reload").map(Command::id), Some(33002));
        assert_eq!(Command::parse("stop").map(Command::id), Some(33006));
        assert_eq!(Command::parse("home"), None);
    }
}
