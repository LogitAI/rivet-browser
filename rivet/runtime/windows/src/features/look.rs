//! Rivet's own look inside the browser window: its colours, a helper that lends a colour
//! provider Rivet's colours for one call, and the first look of a window.
//!
//! A browser lays a window out for its own frame first, with its title strip above the page, and
//! only takes Rivet's layout once Rivet has attached and the browser has laid out again. Rivet keeps
//! the window cloaked through that change, so no frame with the browser's own strip is composed
//! after Rivet holds the window. Cloaking hides the window from the screen only: it keeps its size,
//! its taskbar button and its activation, and the browser keeps painting it.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

/// The timer that shows a window once Rivet's look is in place. It spells "RIF1".
const FIRST_LOOK_TIMER: usize = 0x5249_4631;

/// How long after Rivet's surfaces are created the window stays cloaked. The browser lays out
/// again for Rivet's frame within a few frames of the attach; this leaves it a handful more.
const REVEAL_AFTER_MS: u32 = 120;

/// The latest a window may stay cloaked, whatever else happens. A window must never stay hidden
/// because a step after the attach failed.
pub(crate) const LONGEST_HIDE_MS: u32 = 1500;

/// A moment in a window's first look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FirstLook {
    /// Rivet has subclassed the window, on the thread that attached it, and posted the attach to
    /// the window's UI thread. A timer can only be set on the UI thread.
    Attached,
    /// The window's UI thread took the attach and is about to create Rivet's surfaces.
    UiThread,
    /// The attach could not go on; the window stays the browser's.
    AttachFailed,
    /// Rivet's surfaces exist and the browser has been asked to lay out for them.
    RivetDrawn,
    /// The reveal timer fired.
    Timer,
}

/// What a window's cloak does at a moment of its first look: `Some(true)` hides it, `Some(false)`
/// shows it and `None` leaves it. The hide at the attach is followed by the UI thread's timer, so
/// every path ends shown.
pub(crate) fn cloak_at(moment: FirstLook) -> Option<bool> {
    match moment {
        FirstLook::Attached => Some(true),
        FirstLook::AttachFailed | FirstLook::Timer => Some(false),
        FirstLook::UiThread | FirstLook::RivetDrawn => None,
    }
}

/// When the reveal timer fires after a moment of the first look, in milliseconds.
pub(crate) fn reveal_in(moment: FirstLook) -> Option<u32> {
    match moment {
        FirstLook::UiThread => Some(LONGEST_HIDE_MS),
        FirstLook::RivetDrawn => Some(REVEAL_AFTER_MS),
        FirstLook::Attached | FirstLook::AttachFailed | FirstLook::Timer => None,
    }
}

/// Moves a window's first look on. `Attached` runs on the thread that attached the window, every
/// other moment on the window's UI thread.
pub(crate) unsafe fn first_look(window: HWND, moment: FirstLook) {
    if let Some(hidden) = cloak_at(moment) {
        let value: i32 = i32::from(hidden);
        DwmSetWindowAttribute(
            window,
            DWMWA_CLOAK as u32,
            (&value as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
    }
    match reveal_in(moment) {
        Some(ms) => {
            SetTimer(window, FIRST_LOOK_TIMER, ms, None);
        }
        None if moment != FirstLook::Attached => {
            KillTimer(window, FIRST_LOOK_TIMER);
        }
        None => {}
    }
}

/// Handles the reveal timer. Returns whether `timer` was it.
pub(crate) unsafe fn on_timer(window: HWND, timer: usize) -> bool {
    if timer != FIRST_LOOK_TIMER {
        return false;
    }
    first_look(window, FirstLook::Timer);
    true
}

/// Runs `work` with every colour of the provider at `provider` replaced by what `map` returns for
/// its id and value, then puts the provider's own values back. Used where a Rivet surface reads
/// colours outside a view's paint.
pub(crate) unsafe fn with_colour_map<R>(
    provider: *const u8,
    map: impl Fn(i32, u32) -> Option<u32>,
    work: impl FnOnce() -> R,
) -> R {
    fn collect(node: *mut u8, out: &mut Vec<*mut u8>, depth: usize) {
        if node.is_null() || depth > 40 {
            return;
        }
        unsafe {
            collect(*(node as *const *mut u8), out, depth + 1);
            out.push(node);
            collect(*(node.add(8) as *const *mut u8), out, depth + 1);
        }
    }
    let mut nodes = Vec::new();
    collect(*(provider.add(0x18) as *const *mut u8), &mut nodes, 0);
    let mut saved = Vec::new();
    for node in nodes {
        let slot = node.add(0x24) as *mut u32;
        let original = slot.read();
        if let Some(colour) = map(*(node.add(0x20) as *const i32), original) {
            slot.write(colour);
            saved.push((slot, original));
        }
    }
    let result = work();
    for (slot, original) in saved {
        slot.write(original);
    }
    result
}

/// Traces every colour a provider holds as `id=value`, in id order, for a checked build.
pub(crate) unsafe fn dump_colours(provider: *const u8) {
    fn walk(node: *const u8, out: &mut Vec<String>, depth: usize) {
        if node.is_null() || depth > 40 {
            return;
        }
        unsafe {
            walk(*(node as *const *const u8), out, depth + 1);
            out.push(format!(
                "{}={:08x}",
                *(node.add(0x20) as *const i32),
                *(node.add(0x24) as *const u32)
            ));
            walk(*(node.add(8) as *const *const u8), out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(*(provider.add(0x18) as *const *const u8), &mut out, 0);
    crate::trace::trace(&format!("colours {} {}", out.len(), out.join(" ")));
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [FirstLook; 5] = [
        FirstLook::Attached,
        FirstLook::UiThread,
        FirstLook::AttachFailed,
        FirstLook::RivetDrawn,
        FirstLook::Timer,
    ];

    #[test]
    fn fr12_window_is_cloaked_from_the_attach_until_rivets_look_is_drawn() {
        assert_eq!(cloak_at(FirstLook::Attached), Some(true));
        // Drawing Rivet's surfaces does not show the window at once: the browser still has to lay
        // out for them, so the reveal waits a few frames.
        assert_eq!(cloak_at(FirstLook::RivetDrawn), None);
        assert_eq!(reveal_in(FirstLook::RivetDrawn), Some(REVEAL_AFTER_MS));
        assert_eq!(cloak_at(FirstLook::Timer), Some(false));
    }

    #[test]
    fn fr12_a_failed_attach_shows_the_window_at_once() {
        assert_eq!(cloak_at(FirstLook::AttachFailed), Some(false));
        assert_eq!(reveal_in(FirstLook::AttachFailed), None);
    }

    #[test]
    fn fr12_every_hide_is_followed_by_a_reveal_within_the_longest_hide() {
        // Only the attach hides, and the attach thread cannot set a timer: the UI thread sets the
        // longest one as soon as it takes the attach, before anything that could fail.
        let hides: Vec<_> = ALL
            .into_iter()
            .filter(|m| cloak_at(*m) == Some(true))
            .collect();
        assert_eq!(hides, vec![FirstLook::Attached]);
        assert_eq!(reveal_in(FirstLook::UiThread), Some(LONGEST_HIDE_MS));
        assert_eq!(cloak_at(FirstLook::UiThread), None);
        for moment in ALL {
            if let Some(due) = reveal_in(moment) {
                assert!(due <= LONGEST_HIDE_MS, "{moment:?} hides for {due} ms");
            }
        }
        const { assert!(REVEAL_AFTER_MS < LONGEST_HIDE_MS) };
        // The timer itself shows the window and stops.
        assert_eq!(reveal_in(FirstLook::Timer), None);
    }

    #[test]
    fn fr12_first_look_timer_differs_from_rivets_other_timers() {
        // Every other timer Rivet sets on the browser window.
        for other in [
            0x5249_5253usize,
            0x5249_5645,
            0x5249_5646,
            0x5249_5647,
            0x5249_564C,
            0x5249_5650,
            0x5249_5652,
            0x5249_5654,
            // The Mini Window opening's timer on the Rivet window it pops out of.
            0x5249_4d4f,
        ] {
            assert_ne!(FIRST_LOOK_TIMER, other);
        }
    }
}
