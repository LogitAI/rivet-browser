//! What Rivet's own address bar reads and does for the tabs of an app window.
//!
//! An app window has no browser toolbar, so Rivet draws the address bar. This module is the one
//! place that bar talks to: the address and title of every tab, and the browser's own navigation
//! commands. It reads the tabs through the browser access layer and sends the browser's command
//! id through the operating system layer, so it runs the same against test layers.

use crate::browser::{BrowserCommands, Capabilities, Feature, PageState, Pages};
use crate::os::{WindowCommands, WindowId};

pub(crate) use crate::browser::NavCommand as Command;

/// The navigation command a word of the control file names.
pub(crate) fn parse_command(word: &str) -> Option<Command> {
    Some(match word {
        "back" => Command::Back,
        "forward" => Command::Forward,
        "reload" => Command::Reload,
        "stop" => Command::Stop,
        _ => return None,
    })
}

/// Every tab of `window` with its title and visible address, in strip order. `None` when the
/// build cannot serve the address bar or the tab strip cannot be read.
pub(crate) fn pages(
    browser: &(impl Pages + Capabilities),
    window: WindowId,
) -> Option<Vec<PageState>> {
    if !browser.report().is_available(Feature::PageBar) {
        return None;
    }
    browser.pages(window)
}

/// Runs a navigation command on the active tab of `window`, on the browser UI thread. Returns
/// whether the build has the command.
pub(crate) fn run(
    os: &impl WindowCommands,
    browser: &impl BrowserCommands,
    window: WindowId,
    command: Command,
) -> bool {
    let Some(id) = browser.command_id(command) else {
        return false;
    };
    os.send_command(window, id);
    true
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::browser::{Availability, CapabilityReport};

    /// A browser access layer with fixed tabs, for tests without a browser.
    struct TestBrowser {
        report: CapabilityReport,
        pages: Option<Vec<PageState>>,
    }

    impl TestBrowser {
        fn serving(pages: Vec<PageState>) -> Self {
            Self {
                report: CapabilityReport::new().with(Feature::PageBar, Availability::Available),
                pages: Some(pages),
            }
        }
    }

    impl Pages for TestBrowser {
        fn pages(&self, _window: WindowId) -> Option<Vec<PageState>> {
            self.pages.clone()
        }
    }

    impl Capabilities for TestBrowser {
        fn report(&self) -> CapabilityReport {
            self.report.clone()
        }
    }

    impl BrowserCommands for TestBrowser {
        /// A build that has back and reload, and nothing else.
        fn command_id(&self, command: Command) -> Option<usize> {
            match command {
                Command::Back => Some(7),
                Command::Reload => Some(9),
                _ => None,
            }
        }
    }

    /// An operating system layer that records the commands it is asked to send.
    #[derive(Default)]
    struct TestOs {
        sent: RefCell<Vec<(WindowId, usize)>>,
    }

    impl WindowCommands for TestOs {
        fn send_command(&self, window: WindowId, id: usize) {
            self.sent.borrow_mut().push((window, id));
        }
    }

    fn page(index: usize, active: bool, url: &str) -> PageState {
        PageState {
            index,
            active,
            title: format!("Page {index}"),
            url: url.to_owned(),
        }
    }

    #[test]
    fn nfr9_page_bar_lists_pages_from_the_browser_access_layer() {
        let tabs = vec![
            page(0, false, "https://example.com/"),
            page(1, true, "https://example.org/"),
        ];
        let browser = TestBrowser::serving(tabs.clone());
        assert_eq!(pages(&browser, WindowId::from_raw(1)), Some(tabs));
    }

    #[test]
    fn nfr9_page_bar_stays_off_when_the_build_reports_it_unavailable() {
        let browser = TestBrowser {
            report: CapabilityReport::new().with(
                Feature::PageBar,
                Availability::Off("the tab strip is not recorded".to_owned()),
            ),
            pages: Some(vec![page(0, true, "https://example.com/")]),
        };
        assert_eq!(pages(&browser, WindowId::from_raw(1)), None);
    }

    #[test]
    fn nfr9_page_bar_sends_the_builds_command_id_through_the_os_layer() {
        let os = TestOs::default();
        let browser = TestBrowser::serving(Vec::new());
        let window = WindowId::from_raw(42);
        assert!(run(&os, &browser, window, Command::Back));
        assert!(run(&os, &browser, window, Command::Reload));
        assert_eq!(*os.sent.borrow(), vec![(window, 7), (window, 9)]);
    }

    #[test]
    fn nfr9_page_bar_sends_nothing_for_a_command_the_build_lacks() {
        let os = TestOs::default();
        let browser = TestBrowser::serving(Vec::new());
        assert!(!run(&os, &browser, WindowId::from_raw(42), Command::Stop));
        assert!(os.sent.borrow().is_empty());
    }

    #[test]
    fn app_window_address_bar_words_name_the_navigation_commands() {
        assert_eq!(parse_command("back"), Some(Command::Back));
        assert_eq!(parse_command("forward"), Some(Command::Forward));
        assert_eq!(parse_command("reload"), Some(Command::Reload));
        assert_eq!(parse_command("stop"), Some(Command::Stop));
        assert_eq!(parse_command("home"), None);
    }
}
