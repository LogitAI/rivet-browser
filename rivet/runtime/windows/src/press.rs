//! The press feedback of the appbar's controls, as time functions.
//!
//! A pressed control sinks quickly and darkens a little, stays down for at least a beat even on a
//! short tap, and springs back slowly with a small overshoot when released. The curves and times
//! are the ones the shell study gives `button.pressed` and `button`.

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function.
#[derive(Clone, Copy)]
pub struct Bezier(f32, f32, f32, f32);

impl Bezier {
    /// The progress at `t` of the way through the time, from 0 to 1 and beyond for a spring.
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        // Find the curve parameter whose x is t. x is monotonic for the curves used here.
        let (mut low, mut high) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if self.x(middle) < t {
                low = middle;
            } else {
                high = middle;
            }
        }
        self.y((low + high) / 2.0)
    }

    fn x(self, s: f32) -> f32 {
        Self::axis(self.0, self.2, s)
    }

    fn y(self, s: f32) -> f32 {
        Self::axis(self.1, self.3, s)
    }

    fn axis(a: f32, b: f32, s: f32) -> f32 {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    }
}

/// `transform .12s cubic-bezier(.25, .7, .35, 1)`.
const SINK: Bezier = Bezier(0.25, 0.7, 0.35, 1.0);
/// `transform .36s cubic-bezier(.3, 1.45, .55, 1)`.
const SPRING: Bezier = Bezier(0.3, 1.45, 0.55, 1.0);
/// CSS `ease`, which the brightness filter uses.
const EASE: Bezier = Bezier(0.25, 0.1, 0.25, 1.0);

/// How long the press takes, in milliseconds.
pub const SINK_MS: f32 = 120.0;
/// How long a short tap keeps the control down at least.
pub const HOLD_MS: f32 = 90.0;
/// How long the scale takes to spring back, and the brightness to return.
pub const SPRING_MS: f32 = 360.0;
pub const BRIGHT_MS: f32 = 300.0;

/// One press of a control, from its start to the end of its release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Press<K> {
    key: K,
    down_at: f32,
    up_at: Option<f32>,
    /// The scale and brightness amounts the press started from, when it began mid-release.
    from: (f32, f32),
}

/// The scale amount and the brightness amount of a control at one moment. An amount of 1 is fully
/// pressed, 0 is at rest, and a negative scale amount is the overshoot of the spring.
pub type Amounts = (f32, f32);

impl<K: Copy + PartialEq> Press<K> {
    fn release_at(&self) -> Option<f32> {
        self.up_at.map(|up| up.max(self.down_at + HOLD_MS))
    }

    /// The amounts while the control is held down, at `now` on the same clock as the press.
    fn sunk(&self, now: f32) -> Amounts {
        let sink = |from: f32, curve: Bezier, span: f32| {
            from + (1.0 - from) * curve.at((now - self.down_at) / span)
        };
        (
            sink(self.from.0, SINK, SINK_MS),
            sink(self.from.1, EASE, SINK_MS),
        )
    }

    /// The amounts at `now`, in milliseconds on the same clock as the press.
    pub fn amounts(&self, now: f32) -> Amounts {
        let Some(release) = self.release_at().filter(|release| now > *release) else {
            return self.sunk(now);
        };
        // The release leaves from wherever the press had got to.
        let (scale, bright) = self.sunk(release);
        (
            scale * (1.0 - SPRING.at((now - release) / SPRING_MS)),
            bright * (1.0 - EASE.at((now - release) / BRIGHT_MS)),
        )
    }

    /// Whether the control still moves at `now`.
    pub fn active(&self, now: f32) -> bool {
        match self.release_at() {
            None => true,
            Some(release) => now < release + SPRING_MS,
        }
    }
}

/// The presses of one window's controls.
#[derive(Clone, Debug)]
pub struct Presses<K> {
    list: Vec<Press<K>>,
}

impl<K> Default for Presses<K> {
    fn default() -> Self {
        Self { list: Vec::new() }
    }
}

impl<K: Copy + PartialEq> Presses<K> {
    /// A control goes down. A control that is still springing back sinks again from where it is.
    pub fn press(&mut self, key: K, now: f32) {
        let from = self
            .list
            .iter()
            .find(|p| p.key == key)
            .map_or((0.0, 0.0), |p| p.amounts(now));
        self.list.retain(|p| p.key != key);
        self.list.push(Press {
            key,
            down_at: now,
            up_at: None,
            from,
        });
    }

    /// The control comes up, or the pointer leaves it.
    #[cfg(test)]
    pub fn release(&mut self, key: K, now: f32) {
        if let Some(press) = self
            .list
            .iter_mut()
            .find(|p| p.key == key && p.up_at.is_none())
        {
            press.up_at = Some(now);
        }
    }

    pub fn release_all(&mut self, now: f32) {
        for press in self.list.iter_mut().filter(|p| p.up_at.is_none()) {
            press.up_at = Some(now);
        }
    }

    /// Drops finished presses, and reports whether anything still moves.
    pub fn settle(&mut self, now: f32) -> bool {
        self.list.retain(|p| p.active(now));
        !self.list.is_empty()
    }

    pub fn amounts(&self, now: f32) -> Vec<(K, f32, f32)> {
        self.list
            .iter()
            .map(|p| {
                let (scale, bright) = p.amounts(now);
                (p.key, scale, bright)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr3_press_sinks_in_120_ms_and_springs_back_in_360_ms() {
        let mut presses = Presses::default();
        presses.press(1, 0.0);
        let (scale, bright) = presses.list[0].amounts(SINK_MS);
        assert!((scale - 1.0).abs() < 0.01 && (bright - 1.0).abs() < 0.01);
        presses.release(1, 500.0);
        let end = 500.0 + SPRING_MS;
        let (scale, bright) = presses.list[0].amounts(end);
        assert!(scale.abs() < 0.01 && bright.abs() < 0.01);
        assert!(presses.list[0].active(end - 1.0));
        assert!(!presses.list[0].active(end));
    }

    #[test]
    fn fr3_press_has_a_small_overshoot_when_it_springs_back() {
        let mut presses = Presses::default();
        presses.press(1, 0.0);
        presses.release(1, 300.0);
        let lowest = (0..=36)
            .map(|step| presses.list[0].amounts(300.0 + step as f32 * 10.0).0)
            .fold(f32::MAX, f32::min);
        // A negative amount is a scale past 1: with .96 it is at most about 1.004.
        assert!(lowest < -0.02 && lowest > -0.3, "overshoot {lowest}");
    }

    #[test]
    fn fr3_short_tap_still_holds_the_press_for_a_beat() {
        let mut presses = Presses::default();
        presses.press(1, 0.0);
        presses.release(1, 10.0);
        let (scale, _) = presses.list[0].amounts(HOLD_MS);
        assert!(scale > 0.9, "held at {scale}");
        // The spring starts only after the hold.
        let (early, _) = presses.list[0].amounts(HOLD_MS + 40.0);
        assert!(early < scale);
    }

    #[test]
    fn fr3_pressing_again_while_springing_back_starts_from_where_it_is() {
        let mut presses = Presses::default();
        presses.press(1, 0.0);
        presses.release(1, 200.0);
        let mid = presses.list[0].amounts(300.0).0;
        presses.press(1, 300.0);
        assert!((presses.list[0].amounts(300.0).0 - mid).abs() < 0.001);
        assert_eq!(presses.list.len(), 1);
    }

    #[test]
    fn fr3_settled_presses_leave_the_list() {
        let mut presses = Presses::default();
        presses.press(1, 0.0);
        presses.release(1, 0.0);
        assert!(presses.settle(100.0));
        assert!(!presses.settle(HOLD_MS + SPRING_MS + 1.0));
    }
}
