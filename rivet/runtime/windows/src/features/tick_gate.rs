//! Decides whether the window's tab refresh timer has anything to draw. The timer runs every half
//! second, and nearly every tick finds the same tabs, titles and pages as the last one. A cheap
//! signature of what the timer can see is compared with the last one, and the window's surface is
//! drawn again only when it differs. A full redraw still runs now and then, for state the
//! signature cannot see.

use std::collections::HashMap;
use std::fmt::{Debug, Write};
use std::hash::{Hash, Hasher};

/// Every how many ticks the surface is drawn again even when the signature stayed the same.
pub const REDRAW_EVERY: u64 = 20;

/// Hashes the debug text of `parts` without keeping the text.
pub fn signature(parts: &[&dyn Debug]) -> u64 {
    struct Feed(std::collections::hash_map::DefaultHasher);
    impl Write for Feed {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            text.hash(&mut self.0);
            Ok(())
        }
    }
    let mut feed = Feed(std::collections::hash_map::DefaultHasher::new());
    for part in parts {
        let _ = write!(feed, "{part:?}\u{1f}");
    }
    feed.0.finish()
}

/// The last signature of each window and the ticks since its last redraw.
#[derive(Default)]
pub struct Gate {
    last: HashMap<isize, (u64, u64)>,
}

impl Gate {
    /// Whether the window must be drawn again on this tick, given what the tick sees now.
    pub fn due(&mut self, window: isize, signature: u64) -> bool {
        match self.last.get_mut(&window) {
            Some((last, quiet)) if *last == signature && *quiet + 1 < REDRAW_EVERY => {
                *quiet += 1;
                false
            }
            _ => {
                self.last.insert(window, (signature, 0));
                true
            }
        }
    }

    /// Forgets a window, so its next tick draws.
    pub fn forget(&mut self, window: isize) {
        self.last.remove(&window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_tick_with_unchanged_tabs_draws_nothing() {
        let mut gate = Gate::default();
        let tabs = signature(&[&vec![("a", "Title", true)], &96u32]);
        assert!(gate.due(1, tabs), "the first tick draws");
        for _ in 0..5 {
            assert!(!gate.due(1, tabs), "the same tabs draw nothing");
        }
    }

    #[test]
    fn idle_tick_draws_when_a_title_state_or_page_changed() {
        let mut gate = Gate::default();
        let before = signature(&[&vec![("a", "Title", true)]]);
        let titled = signature(&[&vec![("a", "Other", true)]]);
        let inactive = signature(&[&vec![("a", "Title", false)]]);
        let moved = signature(&[&vec![("b", "Title", true)]]);
        assert!(gate.due(1, before));
        assert!(gate.due(1, titled));
        assert!(gate.due(1, inactive));
        assert!(gate.due(1, moved));
        assert!(!gate.due(1, moved));
    }

    #[test]
    fn idle_tick_still_redraws_now_and_then() {
        let mut gate = Gate::default();
        let same = signature(&[&1]);
        let drawn = (0..REDRAW_EVERY * 3).filter(|_| gate.due(1, same)).count();
        assert_eq!(drawn, 3, "one draw per {REDRAW_EVERY} quiet ticks");
    }

    #[test]
    fn idle_tick_keeps_windows_apart() {
        let mut gate = Gate::default();
        let same = signature(&[&1]);
        assert!(gate.due(1, same));
        assert!(gate.due(2, same), "another window draws its own first tick");
        gate.forget(1);
        assert!(gate.due(1, same), "a forgotten window draws again");
    }

    #[test]
    fn idle_signature_tells_parts_apart() {
        assert_ne!(signature(&[&"ab", &"c"]), signature(&[&"a", &"bc"]));
        assert_eq!(signature(&[&"ab"]), signature(&[&"ab"]));
    }
}
