//! The browser access adapter for the Chromium build loaded in this process.
//!
//! It wraps the existing tab strip code, which still lives in `crate::features::tabs` until that
//! module moves into this layer. The behaviour is the same as before: every read goes through the
//! build's own functions, at addresses its row recorded and the loaded module verified.

use std::sync::OnceLock;

use super::{
    Availability, BrowserCommands, Capabilities, CapabilityReport, Feature, NavCommand, PageState,
    Pages,
};
use crate::os::WindowId;

/// The Chromium build loaded in this process.
pub(crate) struct Chromium;

impl Pages for Chromium {
    fn pages(&self, window: WindowId) -> Option<Vec<PageState>> {
        let window = window.hwnd();
        // The tab strip reads call the build's own functions. `base()` returns an address only
        // when the row's targets verified against the loaded module, and every pointer the reads
        // follow comes from those functions.
        unsafe {
            let base = crate::features::tabs::base()?;
            let model = crate::features::tabs::tab_model(base, window)?;
            let tabs = crate::features::tabs::tabs(window)?;
            Some(
                tabs.into_iter()
                    .map(|tab| {
                        let contents =
                            crate::features::tabs::contents_at(base, model, tab.index as i32);
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
    }
}

impl BrowserCommands for Chromium {
    /// Chromium's command ids (`chrome/app/chrome_command_ids.h`). The window's `WM_COMMAND`
    /// routes them to the browser command controller like its own menu does, so no build address
    /// is needed for them.
    fn command_id(&self, command: NavCommand) -> Option<usize> {
        Some(match command {
            NavCommand::Back => 33000,
            NavCommand::Forward => 33001,
            NavCommand::Reload => 33002,
            NavCommand::Stop => 33006,
        })
    }
}

impl Capabilities for Chromium {
    /// The loaded build does not change while the process runs, so the report is made once and
    /// written to the trace once.
    fn report(&self) -> CapabilityReport {
        static REPORT: OnceLock<CapabilityReport> = OnceLock::new();
        REPORT
            .get_or_init(|| {
                let report = CapabilityReport::new().with(Feature::PageBar, page_bar());
                for line in report.describe() {
                    crate::trace::trace(&line);
                }
                report
            })
            .clone()
    }
}

/// The address bar of an app window needs the tab strip model and its page reads.
fn page_bar() -> Availability {
    if crate::features::tabs::base().is_some() {
        return Availability::Available;
    }
    Availability::Off(
        crate::feature::needs_here(crate::features::tabs::needs)
            .unavailable()
            .unwrap_or_else(|| "the tab strip functions did not verify in this module".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_window_address_bar_commands_are_chromiums_own_ids() {
        assert_eq!(Chromium.command_id(NavCommand::Back), Some(33000));
        assert_eq!(Chromium.command_id(NavCommand::Forward), Some(33001));
        assert_eq!(Chromium.command_id(NavCommand::Reload), Some(33002));
        assert_eq!(Chromium.command_id(NavCommand::Stop), Some(33006));
    }
}
