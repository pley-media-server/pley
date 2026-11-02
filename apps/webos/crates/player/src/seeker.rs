//! Seeking with the remote (the Samsung app's `Seeker`, the Android app's
//! `TvSeeker`): each press steps 10 s (media keys 30 s) from where the last
//! press left the target, and the seek happens once: Left and Right wait for
//! OK, going by themselves 5 s after the last key ([`WAIT_MS`]), so a few
//! presses don't start the server's conversion again for each; the media keys
//! and the Back 10 and Forward 10 buttons seek 400 ms after the last press
//! ([`SETTLE_MS`]). A badge sums the presses. A held key scrubs faster the
//! longer it is held (from 20×, doubling every 1.5 s, up to a whole video in
//! 10 s) and, let go, waits for OK as presses do.
//!
//! Time comes in (`now`, ms of the app's clock); the screen calls
//! [`Seeker::tick`] each frame and wakes at [`Seeker::next_due`].

/// A press of Left or Right.
pub const STEP_MS: i64 = 10_000;
/// A press of fast-forward or rewind.
pub const MEDIA_STEP_MS: i64 = 30_000;
/// The media keys' and buttons' jumps in a row seek once, this long after the last.
pub const SETTLE_MS: i64 = 400;
/// A seek with Left and Right waits for OK, and goes by itself this long after the last key.
pub const WAIT_MS: i64 = 5_000;
/// The badge goes this long after the last press.
pub const BADGE_MS: i64 = 600;
/// A held key's repeats count at most this much time each.
const MAX_REPEAT_GAP_MS: i64 = 200;
/// The target stays this far before the end.
const END_MARGIN_MS: i64 = 1_000;

/// A held key's speed as it starts, in ms of video per ms held.
const HOLD_START_RATE: f64 = 20.0;
/// A held key's speed doubles this often.
const HOLD_DOUBLES_MS: f64 = 1_500.0;
/// The speed it stops at: at least this, or a whole video in [`FULL_SWEEP_MS`].
const HOLD_TOP_RATE: f64 = 300.0;
/// At its top speed, a whole video goes by in about this long.
const FULL_SWEEP_MS: f64 = 10_000.0;

/// Video ms scrubbed per ms held (`Seeker.RateAt`): smoothly faster the longer it's held, from 20×
/// doubling every 1.5 s, up to a top that grows with the video (300×, or a whole video in 10 s).
pub fn hold_rate(held_ms: i64, duration_ms: i64) -> f64 {
    let top = HOLD_TOP_RATE.max(duration_ms as f64 / FULL_SWEEP_MS);
    (HOLD_START_RATE * 2f64.powf(held_ms.max(0) as f64 / HOLD_DOUBLES_MS)).min(top)
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

    /// A press at `now`: one step (`direction` ±1) from the pending target, else from the playhead;
    /// the seek goes `wait_ms` after the last press ([`WAIT_MS`] or [`SETTLE_MS`]) unless OK
    /// takes it sooner.
    pub fn press(&mut self, now: i64, direction: i64, step_ms: i64, wait_ms: i64, position_ms: i64, duration_ms: i64) {
        let from = self.target.unwrap_or(position_ms);
        self.target = Some(Self::clamp(from + direction * step_ms, duration_ms));
        self.badge_ms += direction * step_ms;
        self.settle_at = Some(now + wait_ms);
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
        let dt = MAX_REPEAT_GAP_MS.min(now - self.last_repeat).max(0);
        self.last_repeat = now;
        let advance = hold_rate(now - self.hold_start, duration_ms) * dt as f64;
        let from = self.target.unwrap_or(position_ms);
        self.target = Some(Self::clamp(from + (direction as f64 * advance) as i64, duration_ms));
    }

    /// The key let go at `now`: a scrub waits for OK where it got to, as presses do, going by
    /// itself [`WAIT_MS`] later. Whether a key was held.
    pub fn release(&mut self, now: i64) -> bool {
        if !self.holding {
            return false;
        }
        self.holding = false;
        self.settle_at = self.target.map(|_| now + WAIT_MS);
        true
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
    fn presses_add_up_and_wait_for_ok_5_s_at_most() {
        let (mut s, mut now, mut seeks) = (Seeker::default(), 0, Vec::new());
        for _ in 0..3 {
            s.press(now, -1, STEP_MS, WAIT_MS, 600_000, 7_200_000);
            advance(&mut s, &mut now, 100, &mut seeks);
        }
        assert_eq!(s.badge_ms, -30_000);
        assert_eq!(badge_text(s.badge_ms), "−30 s");
        assert_eq!(s.target, Some(570_000), "the scrubber shows where it goes");
        advance(&mut s, &mut now, 600, &mut seeks);
        assert_eq!(s.badge_ms, 0, "the badge goes");
        advance(&mut s, &mut now, WAIT_MS - 1_000, &mut seeks);
        assert!(seeks.is_empty(), "it went before OK or the wait");
        advance(&mut s, &mut now, 1_000, &mut seeks);
        assert_eq!(seeks, [570_000]);
    }

    #[test]
    fn the_media_keys_jumps_seek_400_ms_after_the_last() {
        let (mut s, mut now, mut seeks) = (Seeker::default(), 0, Vec::new());
        s.press(now, 1, MEDIA_STEP_MS, SETTLE_MS, 600_000, 7_200_000);
        advance(&mut s, &mut now, 100, &mut seeks);
        s.press(now, 1, MEDIA_STEP_MS, SETTLE_MS, 600_000, 7_200_000);
        advance(&mut s, &mut now, SETTLE_MS + 10, &mut seeks);
        assert_eq!(seeks, [660_000]);
    }

    #[test]
    fn stays_a_second_short_of_the_end_and_never_before_the_start() {
        let (mut s, mut now, mut seeks) = (Seeker::default(), 0, Vec::new());
        s.press(now, 1, 30_000, SETTLE_MS, 7_190_000, 7_200_000);
        advance(&mut s, &mut now, 500, &mut seeks);
        s.press(now, -1, 30_000, SETTLE_MS, 5_000, 7_200_000);
        advance(&mut s, &mut now, 500, &mut seeks);
        assert_eq!(seeks, [7_199_000, 0]);
    }

    /// How far a key held moves the target from `from` to `to` (ms of holding), repeating every 50 ms.
    fn scrubbed(s: &mut Seeker, from: i64, to: i64) -> i64 {
        let before = s.target.unwrap_or(0);
        let mut t = from + 50;
        while t <= to {
            s.repeat(t, 1, 0, 36_000_000);
            t += 50;
        }
        s.target.unwrap_or(0) - before
    }

    #[test]
    fn a_held_key_scrubs_twice_as_fast_every_1_5_s_and_waits_when_let_go() {
        let (mut s, mut seeks) = (Seeker::default(), Vec::new());
        s.repeat(0, 1, 60_000, 36_000_000);
        assert!(s.holding);
        s.repeat(50, 1, 60_000, 36_000_000);
        assert_eq!(s.target, Some(60_000 + (20.0 * 2f64.powf(50.0 / 1_500.0) * 50.0) as i64));
        // A second held goes about twice as far as the second 1.5 s before it.
        scrubbed(&mut s, 50, 500);
        let early = scrubbed(&mut s, 500, 1_500);
        scrubbed(&mut s, 1_500, 2_000);
        let later = scrubbed(&mut s, 2_000, 3_000);
        let ratio = later as f64 / early as f64;
        assert!((1.9..=2.1).contains(&ratio), "{ratio}");
        // Let go, it waits for OK as presses do: nothing yet, then once the wait is over.
        let mut now = 3_000;
        assert!(s.release(now));
        assert!(!s.holding);
        assert!(!s.release(now), "nothing held");
        advance(&mut s, &mut now, WAIT_MS - 10, &mut seeks);
        assert!(seeks.is_empty());
        advance(&mut s, &mut now, 20, &mut seeks);
        assert_eq!(seeks.len(), 1);
    }

    #[test]
    fn a_held_key_tops_out_at_a_whole_video_in_10_s_or_300_ms_a_ms() {
        assert_eq!(hold_rate(0, 3_600_000), 20.0);
        assert!((hold_rate(1_500, 3_600_000) - 40.0).abs() < 1e-6);
        assert!((hold_rate(4_500, 3_600_000) - 160.0).abs() < 1e-6);
        // An hour's video: an hour in 10 s, 360 ms of video a ms.
        assert_eq!(hold_rate(20_000, 3_600_000), 360.0);
        // A short one: 300 ms a ms at most.
        assert_eq!(hold_rate(20_000, 600_000), 300.0);
    }

    #[test]
    fn a_gap_between_repeats_counts_as_200_ms_at_most() {
        let mut s = Seeker::default();
        s.repeat(0, 1, 60_000, 3_600_000);
        s.repeat(1_000, 1, 60_000, 3_600_000);
        assert_eq!(s.target, Some(60_000 + (20.0 * 2f64.powf(1_000.0 / 1_500.0) * 200.0) as i64));
    }
}
