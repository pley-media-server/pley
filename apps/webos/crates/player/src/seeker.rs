//! Seeking with the remote (packages/tv/src/player/seeker.ts, the Android
//! app's `TvSeeker`): each press steps 10 s (media keys 30 s) from where the
//! last press left the target, and the seek happens once, 400 ms after the
//! last press; a badge sums the presses. A held key scrubs faster the longer
//! it is held (30×, then 90×, then a whole video in about 15 s) and seeks
//! once, on release.
//!
//! Time comes in (`now`, ms of the app's clock); the screen calls
//! [`Seeker::tick`] each frame and wakes at [`Seeker::next_due`].

/// A press of Left or Right.
pub const STEP_MS: i64 = 10_000;
/// A press of fast-forward or rewind.
pub const MEDIA_STEP_MS: i64 = 30_000;
/// The seek waits this long after the last press.
pub const SETTLE_MS: i64 = 400;
/// The badge goes this long after the last press.
pub const BADGE_MS: i64 = 600;
/// A held key's repeats count at most this much time each.
const MAX_REPEAT_GAP_MS: i64 = 200;
/// The target stays this far before the end.
const END_MARGIN_MS: i64 = 1_000;

/// Video ms scrubbed per ms held: 30× for 1.5 s, 90× until 4 s, then a whole video in ~15 s (at least 180×).
pub fn hold_rate(held_ms: i64, duration_ms: i64) -> f64 {
    if held_ms < 1_500 {
        30.0
    } else if held_ms < 4_000 {
        90.0
    } else {
        180f64.max(duration_ms as f64 / 15_000.0)
    }
}

/// The remote's seeking: a pending target, the badge and a held key's scrub.
#[derive(Clone, Debug, Default)]
pub struct Seeker {
    /// Where the next seek goes; none when none is pending.
    pub target: Option<i64>,
    /// The presses' sum in a row, for the badge; 0 hides it.
    pub badge_ms: i64,
    /// A key is held: the target scrubs.
    pub holding: bool,
    hold_start: i64,
    last_repeat: i64,
    settle_at: Option<i64>,
    badge_until: Option<i64>,
}

impl Seeker {
    fn clamp(ms: i64, duration_ms: i64) -> i64 {
        let top = if duration_ms > 0 { (duration_ms - END_MARGIN_MS).max(0) } else { i64::MAX };
        ms.clamp(0, top)
    }

    /// A press at `now`: one step (`direction` ±1) from the pending target, else from the playhead.
    pub fn press(&mut self, now: i64, direction: i64, step_ms: i64, position_ms: i64, duration_ms: i64) {
        let from = self.target.unwrap_or(position_ms);
        self.target = Some(Self::clamp(from + direction * step_ms, duration_ms));
        self.badge_ms += direction * step_ms;
        self.settle_at = Some(now + SETTLE_MS);
        self.badge_until = Some(now + BADGE_MS);
    }

    /// A repeat of a held key at `now`: no seek until it is let go.
    pub fn repeat(&mut self, now: i64, direction: i64, position_ms: i64, duration_ms: i64) {
        if !self.holding {
            self.holding = true;
            self.hold_start = now;
            self.last_repeat = now;
            self.badge_ms = 0;
            self.badge_until = None;
            self.settle_at = None;
            self.target.get_or_insert(position_ms);
        }
        let dt = MAX_REPEAT_GAP_MS.min(now - self.last_repeat);
        self.last_repeat = now;
        let advance = hold_rate(now - self.hold_start, duration_ms) * dt as f64;
        let from = self.target.unwrap_or(position_ms);
        self.target = Some(Self::clamp(from + (direction as f64 * advance) as i64, duration_ms));
    }

    /// The key let go: a held key seeks now (the target to seek to).
    pub fn release(&mut self) -> Option<i64> {
        if !self.holding {
            return None;
        }
        self.holding = false;
        self.commit()
    }

    /// Forgets the pending seek (Up Next or an error came).
    pub fn cancel(&mut self) {
        *self = Seeker::default();
    }

    /// Runs what is due at `now`: the seek to make, once its presses settled.
    pub fn tick(&mut self, now: i64) -> Option<i64> {
        if self.badge_until.is_some_and(|t| now >= t) {
            self.badge_until = None;
            self.badge_ms = 0;
        }
        if self.settle_at.is_some_and(|t| now >= t) {
            return self.commit();
        }
        None
    }

    /// When [`Seeker::tick`] has something to do next.
    pub fn next_due(&self) -> Option<i64> {
        match (self.settle_at, self.badge_until) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Whether a seek is pending (the controls stay while it is).
    pub fn pending(&self) -> bool {
        self.target.is_some()
    }

    fn commit(&mut self) -> Option<i64> {
        self.settle_at = None;
        self.target.take()
    }
}

/// The badge's text: "−10 s", "+30 s".
pub fn badge_text(ms: i64) -> String {
    let s = (ms.abs() + 500) / 1000;
    format!("{}{s} s", if ms < 0 { "−" } else { "+" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moves `now` by `ms`, running what falls due on the way, as timers would.
    fn advance(s: &mut Seeker, now: &mut i64, ms: i64, seeks: &mut Vec<i64>) {
        let until = *now + ms;
        while let Some(due) = s.next_due().filter(|d| *d <= until) {
            *now = due.max(*now);
            if let Some(t) = s.tick(*now) {
                seeks.push(t);
            }
        }
        *now = until;
    }

    #[test]
    fn steps_10_s_a_press_and_seeks_once_400_ms_after_the_last() {
        let (mut s, mut now, mut seeks) = (Seeker::default(), 0, Vec::new());
        s.press(now, -1, STEP_MS, 600_000, 7_200_000);
        advance(&mut s, &mut now, 100, &mut seeks);
        s.press(now, -1, STEP_MS, 600_000, 7_200_000);
        advance(&mut s, &mut now, 100, &mut seeks);
        s.press(now, -1, STEP_MS, 600_000, 7_200_000);
        assert_eq!(s.badge_ms, -30_000);
        assert_eq!(badge_text(s.badge_ms), "−30 s");
        assert!(seeks.is_empty());
        advance(&mut s, &mut now, SETTLE_MS + 10, &mut seeks);
        assert_eq!(seeks, [570_000]);
        advance(&mut s, &mut now, 600, &mut seeks);
        assert_eq!(s.badge_ms, 0);
    }

    #[test]
    fn stays_a_second_short_of_the_end_and_never_before_the_start() {
        let (mut s, mut now, mut seeks) = (Seeker::default(), 0, Vec::new());
        s.press(now, 1, 30_000, 7_190_000, 7_200_000);
        advance(&mut s, &mut now, 500, &mut seeks);
        s.press(now, -1, 30_000, 5_000, 7_200_000);
        advance(&mut s, &mut now, 500, &mut seeks);
        assert_eq!(seeks, [7_199_000, 0]);
    }

    #[test]
    fn scrubs_a_held_key_faster_and_faster_and_seeks_once_on_release() {
        let (mut s, mut now, mut seeks) = (Seeker::default(), 0, Vec::new());
        s.press(now, 1, STEP_MS, 600_000, 7_200_000);
        for _ in 0..100 {
            advance(&mut s, &mut now, 50, &mut seeks);
            s.repeat(now, 1, 600_000, 7_200_000);
        }
        assert!(seeks.is_empty());
        assert!(s.target.unwrap() > 600_000 + 150_000);
        assert!(s.release().is_some());
        assert!(s.target.is_none());
        assert!(s.release().is_none());
    }

    #[test]
    fn gets_to_a_whole_videos_end_in_about_15_s_of_holding() {
        assert_eq!(hold_rate(1_000, 7_200_000), 30.0);
        assert_eq!(hold_rate(3_000, 7_200_000), 90.0);
        assert_eq!(hold_rate(5_000, 7_200_000), 480.0);
        assert_eq!(hold_rate(5_000, 600_000), 180.0);
    }
}
