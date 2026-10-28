//! The spaces of one browser window and the tabs each of them keeps.
//!
//! A window shows the tabs of one space. Leaving a space stores its tabs here, and coming back
//! restores them. The store lives in memory for as long as the window does.

/// A tab as a space remembers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTab {
    pub url: String,
    /// Kept with the address so a space can list its tabs without loading them.
    #[allow(dead_code)]
    pub title: String,
    pub active: bool,
}

/// What a space is called, how it is coloured and which browser profile it opens in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceInfo {
    pub name: String,
    pub tint: u32,
    pub profile: String,
}

/// The most spaces the footer has room for.
pub const MAX_SPACES: usize = 6;

const TINTS: [u32; MAX_SPACES] = [0xd08a52, 0x5b8bab, 0xc0577f, 0x6a9f58, 0x8a6fc4, 0xc9a227];

#[derive(Clone, Debug)]
pub struct Spaces {
    list: Vec<SpaceInfo>,
    current: usize,
    /// The tabs of every space but the current one, in the same order as `list`.
    saved: Vec<Vec<SavedTab>>,
}

impl Spaces {
    pub fn new() -> Self {
        let space = |name: &str, tint: u32, profile: &str| SpaceInfo {
            name: name.into(),
            tint,
            profile: profile.into(),
        };
        Self {
            list: vec![
                space("Work", TINTS[0], "Chrome · Work"),
                space("Personal", TINTS[1], "Chrome · Default"),
                space("Research", TINTS[2], "Chrome · Default"),
            ],
            current: 0,
            saved: vec![Vec::new(); 3],
        }
    }

    pub fn list(&self) -> &[SpaceInfo] {
        &self.list
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn can_add(&self) -> bool {
        self.list.len() < MAX_SPACES
    }

    /// Adds an empty space after the others, and returns its index.
    pub fn add(&mut self) -> Option<usize> {
        if !self.can_add() {
            return None;
        }
        let index = self.list.len();
        self.list.push(SpaceInfo {
            name: format!("Space {}", index + 1),
            tint: TINTS[index],
            profile: "Chrome · Default".into(),
        });
        self.saved.push(Vec::new());
        Some(index)
    }

    /// Makes `index` the current space. `open` are the tabs the window shows now, which this
    /// space keeps. Returns the tabs to show instead: the ones the target kept, or one start page
    /// at `start_url` for a space that has none. Returns `None` when there is nothing to switch.
    pub fn switch_to(
        &mut self,
        index: usize,
        open: Vec<SavedTab>,
        start_url: &str,
    ) -> Option<Vec<SavedTab>> {
        if index >= self.list.len() || index == self.current {
            return None;
        }
        self.saved[self.current] = open;
        self.current = index;
        let mut tabs = std::mem::take(&mut self.saved[index]);
        if tabs.is_empty() {
            tabs.push(SavedTab {
                url: start_url.into(),
                title: String::new(),
                active: true,
            });
        }
        if !tabs.iter().any(|tab| tab.active) {
            tabs[0].active = true;
        }
        Some(tabs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str, active: bool) -> SavedTab {
        SavedTab {
            url: url.into(),
            title: format!("Title of {url}"),
            active,
        }
    }

    #[test]
    fn fr3_new_window_starts_in_work_with_three_spaces() {
        let spaces = Spaces::new();
        assert_eq!(spaces.current(), 0);
        let names: Vec<_> = spaces.list().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Work", "Personal", "Research"]);
    }

    #[test]
    fn fr3_switching_stores_the_open_tabs_and_shows_a_start_page_for_an_empty_space() {
        let mut spaces = Spaces::new();
        let open = vec![tab("https://a.test/", true), tab("https://b.test/", false)];
        let shown = spaces.switch_to(1, open, "file:///start.html?space=Personal");
        assert_eq!(spaces.current(), 1);
        assert_eq!(
            shown,
            Some(vec![SavedTab {
                url: "file:///start.html?space=Personal".into(),
                title: String::new(),
                active: true,
            }])
        );
    }

    #[test]
    fn fr3_coming_back_restores_urls_titles_order_and_the_active_tab() {
        let mut spaces = Spaces::new();
        let work = vec![
            tab("https://a.test/", false),
            tab("https://b.test/", true),
            tab("https://c.test/", false),
        ];
        spaces.switch_to(2, work.clone(), "start");
        let back = spaces.switch_to(0, vec![tab("start", true)], "start");
        assert_eq!(back, Some(work));
        // The space left in between kept its start page.
        let again = spaces.switch_to(2, vec![tab("https://a.test/", true)], "start");
        assert_eq!(again, Some(vec![tab("start", true)]));
    }

    #[test]
    fn fr3_switching_to_the_current_or_a_missing_space_does_nothing() {
        let mut spaces = Spaces::new();
        assert_eq!(spaces.switch_to(0, vec![tab("x", true)], "start"), None);
        assert_eq!(spaces.switch_to(9, vec![tab("x", true)], "start"), None);
        assert_eq!(spaces.current(), 0);
    }

    #[test]
    fn fr3_a_restored_space_always_has_an_active_tab() {
        let mut spaces = Spaces::new();
        spaces.switch_to(1, vec![tab("a", false), tab("b", false)], "start");
        let shown = spaces
            .switch_to(0, vec![tab("start", true)], "start")
            .unwrap();
        assert_eq!(shown.iter().filter(|t| t.active).count(), 1);
        assert!(shown[0].active);
    }

    #[test]
    fn fr3_new_spaces_stop_at_the_footer_limit_and_start_empty() {
        let mut spaces = Spaces::new();
        assert_eq!(spaces.add(), Some(3));
        assert_eq!(spaces.list()[3].name, "Space 4");
        assert_eq!(spaces.add(), Some(4));
        assert_eq!(spaces.add(), Some(5));
        assert!(!spaces.can_add());
        assert_eq!(spaces.add(), None);
        let tints: std::collections::HashSet<_> = spaces.list().iter().map(|s| s.tint).collect();
        assert_eq!(tints.len(), MAX_SPACES);
        let shown = spaces.switch_to(3, vec![tab("a", true)], "start").unwrap();
        assert_eq!(shown[0].url, "start");
    }
}
