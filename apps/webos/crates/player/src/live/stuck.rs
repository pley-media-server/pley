//! A channel played from its stream.ts that stands still for good while the stream goes on
//! coming (apps/web/src/player/live/stuck.ts): a provider whose stream starts its clock over,
//! as the server connects to it again after it ended or dropped the stream, may leave a player
//! on the picture it had, with no error and no word that it waits. Read again from the
//! server's last keyframe, it plays.
//!
//! Only the rules, on times and numbers the session gives: [`StuckWatch`] looks once a second
//! at where the TV's player says it is, [`Coming`] says whether the stream comes from what the
//! relay counts of it, and [`unstick`] says what to do with one that stands still.

/// How often the watch looks, ms.
pub const EVERY_MS: i64 = 1_000;
/// The stream comes now when some of it came within this, ms.
pub const COMING_WITHIN_MS: i64 = 1_500;
/// A stream is read again at most once in this, ms: one that stands still again sooner plays
/// as HLS, which the transcoder's FFmpeg plays through the jump.
pub const READ_AGAIN_ONCE_IN_MS: i64 = 120_000;

/// When a stream that stands still is stuck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stuck {
    /// The position standing still for this long, ms…
    pub after_ms: i64,
    /// …the stream coming for this much of it: not a provider gone quiet, which is waited for.
    pub coming_ms: i64,
}

pub const STUCK: Stuck = Stuck { after_ms: 8_000, coming_ms: 5_000 };

/// Whether the stream comes, from what the relay says of it each time the session asks: more
/// bytes than the last time, or a player waited on to take what came.
#[derive(Clone, Copy, Debug)]
pub struct Coming {
    bytes: u64,
    at: Option<i64>,
}

impl Coming {
    /// From `bytes` counted so far: what came before is another stream's.
    pub fn from(bytes: u64) -> Coming {
        Coming { bytes, at: None }
    }

    /// What the relay says at `now`: `bytes` counted in all, and whether it waits on a player.
    pub fn saw(&mut self, now: i64, bytes: u64, offering: bool) {
        if bytes != self.bytes || offering {
            self.bytes = bytes;
            self.at = Some(now);
        }
    }

    /// Whether some came just now.
    pub fn now(&self, now: i64) -> bool {
        self.at.is_some_and(|at| now - at < COMING_WITHIN_MS)
    }
}

/// Watches one reading of a stream, from its first picture, for standing still while it comes.
#[derive(Clone, Debug)]
pub struct StuckWatch {
    stuck: Stuck,
    /// Where the player last said it is; none until it says.
    position_ms: Option<u64>,
    moved_before: bool,
    played: bool,
    still_ms: i64,
    came_ms: i64,
    looked_at: i64,
}

impl StuckWatch {
    /// A watch from `now`, the player saying it is at `position_ms` (none: it hasn't said).
    pub fn new(now: i64, position_ms: Option<u64>) -> StuckWatch {
        StuckWatch::with(STUCK, now, position_ms)
    }

    pub fn with(stuck: Stuck, now: i64, position_ms: Option<u64>) -> StuckWatch {
        StuckWatch { stuck, position_ms, moved_before: false, played: false, still_ms: 0, came_ms: 0, looked_at: now }
    }

    /// When it looks next.
    pub fn next(&self) -> i64 {
        self.looked_at + EVERY_MS
    }

    /// Whether the player went on from one look to the next since the watch began.
    pub fn played(&self) -> bool {
        self.played
    }

    /// A look at `now`: where the player says it is (none: it hasn't said), whether the stream
    /// comes, and whether the player says it buffers. Once it stands still for good, whether
    /// it had played; the watch is done then.
    pub fn look(&mut self, now: i64, position_ms: Option<u64>, coming: bool, buffering: bool) -> Option<bool> {
        // A look that comes late counts no more than a look and a half.
        let ms = (now - self.looked_at).clamp(0, EVERY_MS * 3 / 2);
        self.looked_at = now;
        // A player that doesn't say where it is can't be told to stand still: it may well play.
        let at = position_ms?;
        // Forward only: a player whose clock starts over with the stream goes back once.
        let moved = self.position_ms.is_some_and(|before| at > before);
        self.position_ms = Some(at);
        // Played: on from one look to the next, not one step.
        self.played |= moved && self.moved_before;
        self.moved_before = moved;
        if moved {
            self.still_ms = 0;
            self.came_ms = 0;
            return None;
        }
        // A wait the player says is its own to give up on (the session's 20 s): neither counted
        // nor forgotten, so one that says it waits now and then while it stands still is caught.
        if buffering {
            return None;
        }
        self.still_ms += ms;
        if coming {
            self.came_ms += ms;
        }
        (self.still_ms >= self.stuck.after_ms && self.came_ms >= self.stuck.coming_ms && coming).then_some(self.played)
    }
}

/// What is done with a stream that stands still.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unstick {
    /// Read again from the server's last keyframe, on the same session.
    ReadAgain,
    /// Watched as HLS: one that never played wouldn't read again either, nor one that stands
    /// still again soon after it was.
    Hls,
}

/// `played`: it went on playing while watched; `read_again_at`: when it was last read again.
pub fn unstick(played: bool, read_again_at: Option<i64>, now: i64) -> Unstick {
    if played && read_again_at.is_none_or(|at| now - at >= READ_AGAIN_ONCE_IN_MS) {
        Unstick::ReadAgain
    } else {
        Unstick::Hls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A watch that played for three looks, at 3 s.
    fn playing() -> (StuckWatch, i64, u64) {
        let mut w = StuckWatch::new(0, Some(0));
        for s in 1..=3 {
            assert_eq!(w.look(s * 1_000, Some(s as u64 * 1_000), true, false), None);
        }
        assert!(w.played());
        (w, 3_000, 3_000)
    }

    /// Looks once a second from `now` at a player that stays at `position`; when it is stuck.
    fn still(w: &mut StuckWatch, now: i64, position: u64, coming: impl Fn(i64) -> bool, seconds: i64) -> Option<i64> {
        (1..=seconds).find(|s| w.look(now + s * 1_000, Some(position), coming(*s), false).is_some())
    }

    #[test]
    fn a_stream_standing_still_for_8_s_while_it_comes_is_stuck() {
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |_| true, 20), Some(8));
    }

    #[test]
    fn one_that_plays_is_never_stuck() {
        let mut w = StuckWatch::new(0, Some(0));
        for s in 1..=600 {
            assert_eq!(w.look(s * 1_000, Some(s as u64 * 1_000), true, false), None);
        }
    }

    #[test]
    fn a_provider_gone_quiet_is_waited_for() {
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |_| false, 600), None);
    }

    #[test]
    fn the_stream_has_to_come_for_5_of_the_seconds_and_still_come_then() {
        // Came for four seconds, then quiet: not stuck at 8 s, nor ever while it stays quiet.
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |s| s <= 4, 60), None);
        // Coming again: stuck as soon as it has come for five.
        assert_eq!(w.look(now + 61_000, Some(at), true, false), Some(true));
        // Came for five of the eight, but not at the eighth: at the next second it comes.
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |s| s <= 5 || s == 11, 20), Some(11));
        // In bursts, every other second: five of them take ten.
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |s| s % 2 == 0, 20), Some(10));
    }

    #[test]
    fn moving_on_starts_the_count_over() {
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |_| true, 7), None);
        assert_eq!(w.look(now + 8_000, Some(at + 40), true, false), None);
        assert_eq!(still(&mut w, now + 8_000, at + 40, |_| true, 20), Some(8));
    }

    #[test]
    fn a_position_that_goes_back_once_is_not_moving_and_one_that_goes_on_from_there_is() {
        // The player's clock started over with the stream, and it plays on from there.
        let (mut w, now, _) = playing();
        assert_eq!(w.look(now + 1_000, Some(0), true, false), None);
        for s in 2..=30 {
            assert_eq!(w.look(now + s * 1_000, Some((s as u64 - 1) * 1_000), true, false), None);
        }
        // It went back and stays there.
        let (mut w, now, _) = playing();
        assert_eq!(still(&mut w, now, 0, |_| true, 20), Some(8));
    }

    #[test]
    fn played_is_moving_on_two_looks_in_a_row() {
        // Never moved.
        let mut w = StuckWatch::new(0, Some(0));
        assert_eq!(still(&mut w, 0, 0, |_| true, 20), Some(8));
        assert!(!w.played());
        // A step now and then, never two looks running, then still.
        let mut w = StuckWatch::new(0, Some(0));
        for (s, at) in [(1, 1_000), (2, 1_000), (3, 1_400), (4, 1_400)] {
            assert_eq!(w.look(s * 1_000, Some(at), true, false), None);
        }
        assert!(!w.played());
        assert_eq!(still(&mut w, 4_000, 1_400, |_| true, 20), Some(7));
        assert!(!w.played());
        // Two in a row: played, and it stays so.
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |_| true, 20), Some(8));
        assert!(w.played());
    }

    #[test]
    fn a_player_that_doesnt_say_where_it_is_is_never_stuck() {
        // It may play all the while: nothing tells.
        let mut w = StuckWatch::new(0, None);
        for s in 1..=600 {
            assert_eq!(w.look(s * 1_000, None, true, false), None);
        }
        assert!(!w.played());
        // Its first word is where it is, not a move; it is watched from there.
        assert_eq!(w.look(601_000, Some(5_000), true, false), None);
        assert_eq!(w.look(602_000, Some(6_000), true, false), None);
        assert!(!w.played());
        assert_eq!(w.look(603_000, Some(7_000), true, false), None);
        assert!(w.played());
        assert_eq!(still(&mut w, 603_000, 7_000, |_| true, 20), Some(8));
    }

    #[test]
    fn a_wait_the_player_says_is_neither_counted_nor_forgotten() {
        // It says it buffers all along: the session's own give-up has it.
        let (mut w, now, at) = playing();
        for s in 1..=600 {
            assert_eq!(w.look(now + s * 1_000, Some(at), true, true), None);
        }
        // Still for 4 s, saying it buffers for 10, still again: stuck 4 s after.
        let (mut w, now, at) = playing();
        assert_eq!(still(&mut w, now, at, |_| true, 4), None);
        for s in 5..=14 {
            assert_eq!(w.look(now + s * 1_000, Some(at), true, true), None);
        }
        assert_eq!(still(&mut w, now + 14_000, at, |_| true, 20), Some(4));
    }

    #[test]
    fn a_late_look_counts_no_more_than_a_look_and_a_half() {
        let (mut w, now, at) = playing();
        // The app was held up for a minute: that isn't a minute of standing still.
        assert_eq!(w.look(now + 60_000, Some(at), true, false), None);
        assert_eq!(w.next(), now + 61_000);
        // 1.5 s counted, then a second a look: 8 s at the seventh.
        assert_eq!(still(&mut w, now + 60_000, at, |_| true, 20), Some(7));
    }

    #[test]
    fn the_stream_comes_while_bytes_did_within_a_second_and_a_half_or_a_player_is_waited_on() {
        // What was counted before is another stream's.
        let mut c = Coming::from(700);
        c.saw(0, 700, false);
        assert!(!c.now(0));
        c.saw(1_000, 900, false);
        assert!(c.now(1_000) && c.now(2_499));
        assert!(!c.now(2_500));
        c.saw(2_500, 900, false);
        assert!(!c.now(2_500));
        // The relay waits on a player that takes nothing: its stream is there.
        c.saw(3_000, 900, true);
        assert!(c.now(3_000) && c.now(4_000));
        c.saw(5_000, 900, false);
        assert!(!c.now(5_000));
    }

    #[test]
    fn one_that_played_is_read_again_once_in_two_minutes_and_any_other_plays_as_hls() {
        assert_eq!(unstick(true, None, 0), Unstick::ReadAgain);
        assert_eq!(unstick(false, None, 0), Unstick::Hls);
        assert_eq!(unstick(true, Some(10_000), 10_000 + READ_AGAIN_ONCE_IN_MS - 1), Unstick::Hls);
        assert_eq!(unstick(true, Some(10_000), 10_000 + READ_AGAIN_ONCE_IN_MS), Unstick::ReadAgain);
        assert_eq!(unstick(false, Some(10_000), 10_000 + READ_AGAIN_ONCE_IN_MS), Unstick::Hls);
    }
}
