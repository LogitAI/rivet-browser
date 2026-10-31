//! Developer mode of a page: which pages start in it, what the person has chosen since, and which
//! buttons its title bar carries.
//!
//! A tab's mode follows its page. Local pages start in it, and a person's choice holds for as long
//! as the tab stays on the host it was made on. A tab that goes to another host starts over with
//! that host's default.

/// A page address without its scheme's punctuation, cut to the host: `http://[::1]:3000/a?b` gives
/// `[::1]`. Only `http` and `https` pages have one.
pub fn host_of(address: &str) -> Option<String> {
    let address = address.trim();
    let lower = address.to_ascii_lowercase();
    let rest = ["https://", "http://"].iter().find_map(|scheme| lower.strip_prefix(scheme))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        let end = authority.find(']')?;
        &authority[..=end]
    } else {
        authority.split(':').next().unwrap_or("")
    };
    (!host.is_empty()).then(|| host.to_owned())
}

/// Whether a page at `address` starts in developer mode: it is served from this computer.
pub fn starts_in_dev_mode(address: &str) -> bool {
    matches!(host_of(address).as_deref(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// What one tab remembers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TabMode {
    tab: usize,
    host: String,
    /// The person's choice for this host, if they made one.
    chosen: Option<bool>,
}

/// The developer mode of the tabs of one browser process. A tab is told apart by an identity the
/// caller keeps for as long as the tab lives.
#[derive(Default)]
pub struct Modes {
    tabs: Vec<TabMode>,
}

/// The most tabs remembered at once. A tab identity can be reused after the tab closes, and the
/// host check keeps a reused identity from carrying a choice over to an unrelated page.
const REMEMBERED: usize = 256;

impl Modes {
    pub const fn new() -> Self {
        Modes { tabs: Vec::new() }
    }

    fn slot(&mut self, tab: usize, address: &str) -> &mut TabMode {
        let host = host_of(address).unwrap_or_default();
        let at = match self.tabs.iter().position(|known| known.tab == tab) {
            Some(at) => at,
            None => {
                if self.tabs.len() >= REMEMBERED {
                    self.tabs.remove(0);
                }
                self.tabs.push(TabMode { tab, host: host.clone(), chosen: None });
                self.tabs.len() - 1
            }
        };
        let known = &mut self.tabs[at];
        if known.host != host {
            known.host = host;
            known.chosen = None;
        }
        known
    }

    /// Whether the tab, showing `address` now, is in developer mode.
    pub fn is_on(&mut self, tab: usize, address: &str) -> bool {
        let slot = self.slot(tab, address);
        slot.chosen.unwrap_or_else(|| starts_in_dev_mode(address))
    }

    /// Switches the tab's mode and returns the new one.
    pub fn toggle(&mut self, tab: usize, address: &str) -> bool {
        let now = !self.is_on(tab, address);
        self.slot(tab, address).chosen = Some(now);
        now
    }
}

/// A button of the developer bar, in the order it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Inspect,
    Console,
}

impl Action {
    /// The Segoe Fluent Icons glyph and the label of the button.
    pub fn look(self) -> (char, &'static str) {
        match self {
            Action::Inspect => ('\u{E7C9}', "Inspect element"),
            Action::Console => ('\u{E756}', "Console"),
        }
    }
}

/// The buttons a window offers, from the browser commands its build can run. A command the build
/// does not know leaves its button out.
pub fn buttons(known: &[(Action, u32)]) -> Vec<(Action, u32)> {
    known.iter().filter(|(_, command)| *command != 0).copied().collect()
}

/// The size of the bar's buttons and the gap before the caption buttons, in DIPs.
pub const BUTTON: f32 = 30.0;
pub const LEAD: f32 = 6.0;

/// The room a bar of `count` buttons takes at the end of the row, in DIPs.
pub fn bar_width(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        LEAD + BUTTON * count as f32
    }
}

/// The button at `x` DIPs from the left edge of a bar of `count` buttons.
pub fn button_at(count: usize, x: f32) -> Option<usize> {
    let x = x - LEAD;
    if x < 0.0 {
        return None;
    }
    let index = (x / BUTTON) as usize;
    (index < count).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr11_local_hosts_start_in_developer_mode() {
        for address in [
            "http://localhost",
            "http://localhost:4321/demos/source/",
            "https://LOCALHOST/a?b=c#d",
            "http://127.0.0.1:8080",
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://[::1]:3000/x",
            "http://user:pass@localhost:3000/",
        ] {
            assert!(starts_in_dev_mode(address), "{address}");
        }
    }

    #[test]
    fn fr11_other_pages_do_not_start_in_developer_mode() {
        for address in [
            "https://example.com",
            "http://localhost.example.com/",
            "http://example.com/localhost",
            "http://example.com/?next=http://localhost",
            "http://127.0.0.2/",
            "http://127.0.0.1.example.com/",
            "http://[::2]/",
            "http://user@example.com@/",
            "file:///D:/localhost/index.html",
            "chrome://settings",
            "about:blank",
            "localhost",
            "",
        ] {
            assert!(!starts_in_dev_mode(address), "{address}");
        }
    }

    #[test]
    fn fr11_other_pages_enter_developer_mode_by_the_persons_toggle() {
        let mut modes = Modes::new();
        assert!(!modes.is_on(1, "https://example.com/a"));
        assert!(modes.toggle(1, "https://example.com/a"));
        assert!(modes.is_on(1, "https://example.com/b"));
        assert!(!modes.toggle(1, "https://example.com/b"));
        assert!(!modes.is_on(2, "https://example.com/b"));
    }

    #[test]
    fn fr11_mode_is_kept_per_tab() {
        let mut modes = Modes::new();
        assert!(modes.is_on(1, "http://localhost:3000"));
        assert!(!modes.toggle(1, "http://localhost:3000"));
        assert!(modes.is_on(2, "http://localhost:3000"));
        assert!(!modes.is_on(1, "http://localhost:3000/other"));
    }

    #[test]
    fn fr11_a_tab_that_changes_host_starts_over_with_the_default() {
        let mut modes = Modes::new();
        assert!(!modes.toggle(1, "http://localhost:3000"));
        assert!(modes.toggle(1, "https://example.com"));
        assert!(modes.is_on(1, "https://example.com/next"));
        // Back on a local page the choice for the other host is gone.
        assert!(modes.is_on(1, "http://localhost:3000"));
    }

    #[test]
    fn fr11_bar_leaves_out_commands_the_build_does_not_know() {
        let known = [(Action::Inspect, 0), (Action::Console, 40005)];
        assert_eq!(buttons(&known), vec![(Action::Console, 40005)]);
        assert!(buttons(&[(Action::Inspect, 0)]).is_empty());
        assert_eq!(bar_width(0), 0.0);
    }

    #[test]
    fn fr11_bar_hit_test_follows_the_buttons() {
        assert_eq!(button_at(3, 2.0), None);
        assert_eq!(button_at(3, LEAD + 1.0), Some(0));
        assert_eq!(button_at(3, LEAD + BUTTON + 1.0), Some(1));
        assert_eq!(button_at(3, LEAD + 3.0 * BUTTON - 1.0), Some(2));
        assert_eq!(button_at(3, LEAD + 3.0 * BUTTON + 1.0), None);
    }
}
