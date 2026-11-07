//! Counts of a window's timer ticks and repaints, so a tester can check that an idle window does
//! next to nothing. Counting is off unless `rivet-counters.flag` is in the debug folder when the
//! first window attaches. While it is on, a line of totals goes to the trace every ten seconds.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// What is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Counter {
    /// A tick of the side rail's pointer timer.
    RailTick,
    /// A layout the side rail asked the browser for because it had no page geometry.
    RailRelayout,
    /// A tick of the window's tab refresh timer.
    TabTick,
    /// A full update of the window's Rivet surface.
    AppbarUpdate,
    /// A repaint of the top bar's surface.
    AddressPaint,
    /// A move or blink of the top bar's caret without a repaint of the bar.
    CaretMove,
    /// A look at the window's taskbar identity.
    TaskbarCheck,
    /// A look for debug command files.
    DebugScan,
    /// Microseconds spent in the tab refresh timer, in all.
    TickMicros,
    /// Microseconds spent deciding whether the tick draws.
    GateMicros,
    /// Microseconds spent saving the spaces.
    PersistMicros,
    /// Microseconds spent in the tab list's tick, links moved from other windows included.
    ListMicros,
    /// Microseconds spent looking at the taskbar identity.
    TaskbarMicros,
    /// Microseconds spent on debug command files.
    DebugMicros,
    /// Microseconds spent drawing the surface from the tick.
    DrawMicros,
    /// Microseconds spent following the active page and the side panel.
    FollowMicros,
}

const ALL: [Counter; 16] = [
    Counter::RailTick,
    Counter::RailRelayout,
    Counter::TabTick,
    Counter::AppbarUpdate,
    Counter::AddressPaint,
    Counter::CaretMove,
    Counter::TaskbarCheck,
    Counter::DebugScan,
    Counter::TickMicros,
    Counter::GateMicros,
    Counter::PersistMicros,
    Counter::ListMicros,
    Counter::TaskbarMicros,
    Counter::DebugMicros,
    Counter::DrawMicros,
    Counter::FollowMicros,
];

impl Counter {
    fn name(self) -> &'static str {
        match self {
            Counter::RailTick => "rail_tick",
            Counter::RailRelayout => "rail_relayout",
            Counter::TabTick => "tab_tick",
            Counter::AppbarUpdate => "appbar_update",
            Counter::AddressPaint => "address_paint",
            Counter::CaretMove => "caret_move",
            Counter::TaskbarCheck => "taskbar_check",
            Counter::DebugScan => "debug_scan",
            Counter::TickMicros => "tick_us",
            Counter::GateMicros => "gate_us",
            Counter::PersistMicros => "persist_us",
            Counter::ListMicros => "list_us",
            Counter::TaskbarMicros => "taskbar_us",
            Counter::DebugMicros => "debug_us",
            Counter::DrawMicros => "draw_us",
            Counter::FollowMicros => "follow_us",
        }
    }
}

/// A set of counters that does nothing until it is switched on.
pub(crate) struct Counts {
    on: AtomicBool,
    values: [AtomicU64; ALL.len()],
}

impl Counts {
    pub(crate) const fn new() -> Self {
        Self {
            on: AtomicBool::new(false),
            values: [const { AtomicU64::new(0) }; ALL.len()],
        }
    }

    pub(crate) fn switch_on(&self) {
        self.on.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    pub(crate) fn count(&self, counter: Counter) {
        if self.is_on() {
            self.values[counter as usize].fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn add(&self, counter: Counter, amount: u64) {
        if self.is_on() {
            self.values[counter as usize].fetch_add(amount, Ordering::Relaxed);
        }
    }

    pub(crate) fn get(&self, counter: Counter) -> u64 {
        self.values[counter as usize].load(Ordering::Relaxed)
    }

    /// The totals as one trace line, such as `counters: rail_tick=3 rail_relayout=0 ...`.
    pub(crate) fn line(&self) -> String {
        let mut line = String::from("counters:");
        for counter in ALL {
            line.push_str(&format!(" {}={}", counter.name(), self.get(counter)));
        }
        line
    }
}

static COUNTS: Counts = Counts::new();

/// Counts one event of the process, when counting is on.
pub(crate) fn count(counter: Counter) {
    COUNTS.count(counter);
}

/// Runs `work` and adds the microseconds it took to `counter`, when counting is on. With
/// counting off the clock is not read.
pub(crate) fn timed<R>(counter: Counter, work: impl FnOnce() -> R) -> R {
    if !COUNTS.is_on() {
        return work();
    }
    let start = std::time::Instant::now();
    let result = work();
    COUNTS.add(counter, start.elapsed().as_micros() as u64);
    result
}

/// The time now, when counting is on, for `add_since`.
pub(crate) fn clock() -> Option<std::time::Instant> {
    COUNTS.is_on().then(std::time::Instant::now)
}

/// Adds the microseconds since `start` to `counter`.
pub(crate) fn add_since(counter: Counter, start: Option<std::time::Instant>) {
    if let Some(start) = start {
        COUNTS.add(counter, start.elapsed().as_micros() as u64);
    }
}

/// Switches counting on when the debug folder asks for it. Looked at once per process.
pub(crate) fn switch_on_if_asked() {
    static LOOKED: std::sync::Once = std::sync::Once::new();
    LOOKED.call_once(|| {
        if crate::frame::debug_switches()
            .is_some_and(|dir| dir.join("rivet-counters.flag").is_file())
        {
            COUNTS.switch_on();
        }
    });
}

/// Ticks of the tab refresh timer between two trace lines of totals.
const REPORT_EVERY: u64 = 20;

/// Writes the totals to the trace every `REPORT_EVERY` tab refresh ticks, while counting is on.
pub(crate) fn report_on_tick() {
    if !COUNTS.is_on() {
        return;
    }
    if report_due(COUNTS.get(Counter::TabTick)) {
        crate::trace::trace(&COUNTS.line());
    }
}

fn report_due(ticks: u64) -> bool {
    ticks > 0 && ticks % REPORT_EVERY == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_counters_stay_zero_until_switched_on() {
        let counts = Counts::new();
        counts.count(Counter::AppbarUpdate);
        assert_eq!(counts.get(Counter::AppbarUpdate), 0);
        counts.switch_on();
        counts.count(Counter::AppbarUpdate);
        counts.count(Counter::AppbarUpdate);
        counts.count(Counter::CaretMove);
        assert_eq!(counts.get(Counter::AppbarUpdate), 2);
        assert_eq!(counts.get(Counter::CaretMove), 1);
        assert_eq!(counts.get(Counter::RailTick), 0);
    }

    #[test]
    fn idle_timing_counters_add_amounts_only_when_on() {
        let counts = Counts::new();
        counts.add(Counter::TickMicros, 50);
        assert_eq!(counts.get(Counter::TickMicros), 0);
        counts.switch_on();
        counts.add(Counter::TickMicros, 50);
        counts.add(Counter::TickMicros, 25);
        assert_eq!(counts.get(Counter::TickMicros), 75);
    }

    #[test]
    fn idle_counters_line_names_every_counter() {
        let counts = Counts::new();
        counts.switch_on();
        counts.count(Counter::RailTick);
        let line = counts.line();
        assert!(line.starts_with("counters: rail_tick=1 rail_relayout=0"));
        for counter in ALL {
            assert!(line.contains(&format!(" {}=", counter.name())));
        }
    }

    #[test]
    fn idle_counters_report_every_ten_seconds_of_ticks() {
        assert!(!report_due(0));
        assert!(!report_due(19));
        assert!(report_due(20));
        assert!(report_due(40));
    }
}
