//! The browser access layer: everything that touches Chromium, behind traits.
//!
//! This layer is the only place for build rows, symbols, function fingerprints, vtable slots,
//! memory reads and the `unsafe` they need. It reaches Chromium through the most stable surface
//! that works, in this order: the browser's own command ids and the DevTools Protocol, then
//! name-based reflection through views metadata and element ids, and only then values found in the
//! build itself. Features see plain Rust values and never a pointer.
//!
//! For every build the layer reports which features it can serve, as a `CapabilityReport`. A
//! feature the report marks off stays off, and the reason goes to the trace.
//!
//! The adapter for the loaded browser is `chromium::Chromium`. Today it wraps the existing code in
//! `crate::features::tabs` and friends, which move here as each feature moves.
//!
//! The groups the features use, and the surface each group is expected to reach:
//!
//! | Group | Features today | Surface |
//! |---|---|---|
//! | A. Back, forward, reload, stop, home | page_bar, control | command ids (`BrowserCommands`) |
//! | B. Open a typed address | navigation, control, address_bar | DevTools `Page.navigate` |
//! | C. Page address and title | page_bar, control, command_bar, link_rules | DevTools target info (`Pages`) |
//! | D. Security level and page info | address_bar, omnibox | DevTools security state, then build values |
//! | E. New, close, select and restore a tab | tabs, control, tab_list | command ids and DevTools |
//! | F. Tab order and groups | tabs, tab_list, vertical_tabs | build values, reflection to read |
//! | G. Split view and the second page view | split, split_grid, second_view | build values, reflection for bounds |
//! | H. Infobars per pane | pane_infobar | reflection |
//! | I. Mini window out and back | mini_window | command id out, build values back |
//! | J. App window without a tab strip | app_window, tab_strip | build values |
//! | K. Address suggestions | omnibox | build values |
//! | L. Frame, caption and paint | frame_layout, look, widgets | build values, reflection to find views |
//! | M. Vertical tabs, side panel, bubbles, DevTools | vertical_tabs, side_panel, bubble_anchor, dev_tools | command ids, reflection, build values |
//!
//! Only the traits of groups A, C and I (`TabMoves`, for the Mini Window's opening) exist so far.
//! A trait is added when the first feature that needs it moves, never before.

pub(crate) mod chromium;

use crate::os::WindowId;

/// One tab as Rivet's own address bar shows it.
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
pub(crate) enum NavCommand {
    Back,
    Forward,
    Reload,
    Stop,
}

/// Reads the tabs of a browser window.
pub(crate) trait Pages {
    /// Every tab of `window` with its title and visible address, in strip order. `None` when the
    /// window's tab strip cannot be read.
    fn pages(&self, window: WindowId) -> Option<Vec<PageState>>;
}

/// A page of the browser. It names the page's web contents, which stay the same while the tab
/// moves in the strip or between windows, and it is only compared, never followed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PageId(usize);

impl PageId {
    pub(crate) const fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    pub(crate) const fn raw(self) -> usize {
        self.0
    }
}

/// One tab of a window as the Mini Window sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabEntry {
    pub page: PageId,
    /// The committed address, or the visible one before the first commit.
    pub url: String,
    pub active: bool,
}

/// Moves tabs between browser windows of this process: group I, the Mini Window out and back.
pub(crate) trait TabMoves {
    /// The tabs of `window` in strip order. `None` when the window's tab strip cannot be read.
    fn tab_entries(&self, window: WindowId) -> Option<Vec<TabEntry>>;
    /// The profile `page` belongs to, as a value that is only compared. `None` when the build
    /// records no way to read it.
    fn profile_of(&self, page: PageId) -> Option<usize>;
    /// Whether the build can detach a tab from one window and insert it into another.
    fn can_move_tabs(&self) -> bool;
    /// Moves tab `index` of `from` to the end of `to`'s strip, as the tab `to` shows. Returns
    /// whether the browser took the tab.
    fn move_tab(&self, from: WindowId, index: usize, to: WindowId) -> bool;
    /// Closes the tab of `page` in `window`.
    fn close_page(&self, window: WindowId, page: PageId) -> bool;
    /// Makes tab `index` the one `window` shows.
    fn show_tab(&self, window: WindowId, index: usize) -> bool;
}

/// The browser's own command ids.
pub(crate) trait BrowserCommands {
    /// The id this build gives `command`, or `None` when the build has no such command.
    fn command_id(&self, command: NavCommand) -> Option<usize>;
}

/// The features whose availability the browser access layer reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feature {
    /// The tabs, addresses and navigation commands behind Rivet's own address bar.
    PageBar,
}

impl Feature {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Feature::PageBar => "page_bar",
        }
    }
}

/// Whether the loaded build can serve one feature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Availability {
    Available,
    /// The feature stays off in this build, for this reason.
    Off(String),
}

/// What the loaded build can serve, feature by feature.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CapabilityReport {
    entries: Vec<(Feature, Availability)>,
}

impl CapabilityReport {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The report with `feature` set to `availability`, replacing an earlier entry for it.
    pub(crate) fn with(mut self, feature: Feature, availability: Availability) -> Self {
        self.entries.retain(|(known, _)| *known != feature);
        self.entries.push((feature, availability));
        self
    }

    /// Whether `feature` may be turned on. A feature the report does not name stays off, so a
    /// missing check never switches a feature on by accident.
    pub(crate) fn is_available(&self, feature: Feature) -> bool {
        self.entries.iter().any(|(known, availability)| {
            *known == feature && *availability == Availability::Available
        })
    }

    /// One trace line per feature.
    pub(crate) fn describe(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|(feature, availability)| match availability {
                Availability::Available => format!("capability {}: available", feature.name()),
                Availability::Off(reason) => {
                    format!("capability {}: off ({reason})", feature.name())
                }
            })
            .collect()
    }
}

/// Reports what the loaded build can serve.
pub(crate) trait Capabilities {
    fn report(&self) -> CapabilityReport;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfr9_capability_report_gives_the_reason_a_feature_is_off() {
        let report = CapabilityReport::new().with(
            Feature::PageBar,
            Availability::Off("the tab strip is not recorded".to_owned()),
        );
        assert!(!report.is_available(Feature::PageBar));
        assert_eq!(
            report.describe(),
            vec!["capability page_bar: off (the tab strip is not recorded)".to_owned()]
        );
    }

    #[test]
    fn nfr9_feature_missing_from_the_report_stays_off() {
        assert!(!CapabilityReport::new().is_available(Feature::PageBar));
    }

    #[test]
    fn nfr9_later_entry_replaces_the_earlier_one() {
        let report = CapabilityReport::new()
            .with(Feature::PageBar, Availability::Off("not yet".to_owned()))
            .with(Feature::PageBar, Availability::Available);
        assert!(report.is_available(Feature::PageBar));
        assert_eq!(
            report.describe(),
            vec!["capability page_bar: available".to_owned()]
        );
    }
}
