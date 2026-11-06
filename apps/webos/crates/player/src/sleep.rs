//! The sleep timer (packages/tv/src/player/sleep.ts, the Android app's
//! `SleepTimer`): after the minutes chosen, or at the end of what plays, it
//! pauses. 30 s before, its state says so once a second for a notice;
//! [`SleepTimer::extend`] ("Keep watching") adds the same time again.
//! Minutes keep running while paused. Not remembered between playbacks.
//!
//! Time comes in (`now`, ms of the app's clock) with what plays
//! ([`SleepItem`]); the owner calls [`SleepTimer::tick`] at
//! [`SleepTimer::next_due`].

use pley_core::model::ItemKind;
use pley_i18n::t;

/// What the sleep timer is set to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepChoice {
    Off,
    Minutes(u32),
    EndOfItem,
}

/// The menu's choices, in order.
pub const SLEEP_CHOICES: [SleepChoice; 7] = [
    SleepChoice::Off,
    SleepChoice::Minutes(15),
    SleepChoice::Minutes(30),
    SleepChoice::Minutes(45),
    SleepChoice::Minutes(60),
    SleepChoice::Minutes(90),
    SleepChoice::EndOfItem,
];

/// "Off", "30 min", "End of this episode" (of this movie, track or video, by what plays).
pub fn sleep_label(c: SleepChoice, playing: Option<ItemKind>) -> String {
    match c {
        SleepChoice::Off => t!("player.sleep.off").into(),
        SleepChoice::Minutes(m) => t!("player.sleep.minutes", minutes = m),
        SleepChoice::EndOfItem => match playing {
            Some(ItemKind::Episode) => t!("player.sleep.end.episode"),
            Some(ItemKind::Movie) => t!("player.sleep.end.movie"),
            Some(ItemKind::Track) => t!("player.sleep.end.track"),
            _ => t!("player.sleep.end.video"),
        }
        .into(),
    }
}

/// What the timer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepState {
    pub choice: SleepChoice,
    /// Seconds before it pauses, during the notice (its last 30 s); none otherwise.
    pub seconds_left: Option<i64>,
    /// Minutes before it pauses, rounded up, for the menu; none when off or unknown.
    pub minutes_left: Option<i64>,
    /// It paused the playback, which hasn't played since.
    pub fired: bool,
}

impl Default for SleepState {
    fn default() -> SleepState {
        SLEEP_OFF
    }
}

/// The timer off.
pub const SLEEP_OFF: SleepState =
    SleepState { choice: SleepChoice::Off, seconds_left: None, minutes_left: None, fired: false };

/// The notice shows this long before the timer pauses.
pub const NOTICE_MS: i64 = 30_000;
/// Paused by the timer this long, the player leaves.
pub const LEAVE_AFTER_MS: i64 = 10 * 60_000;
const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60_000;

/// What plays, as the timer sees it: an id for the item, and how long it has left (none when unknown).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepItem {
    pub key: i64,
    pub left_ms: Option<i64>,
}

/// The sleep timer.
#[derive(Clone, Debug, Default)]
pub struct SleepTimer {
    pub state: SleepState,
    due: Option<i64>,
    deadline: i64,
    /// "Keep watching" at the end of this item: the timer waits for the next one's end.
    spared: Option<i64>,
}

impl SleepTimer {
    /// Sets the timer at `now` while `item` plays.
    pub fn set(&mut self, now: i64, choice: SleepChoice, item: SleepItem) {
        self.due = None;
        self.spared = None;
        self.state = SleepState { choice, ..SLEEP_OFF };
        if choice == SleepChoice::Off {
            return;
        }
        if let SleepChoice::Minutes(m) = choice {
            self.deadline = now + m as i64 * MINUTE_MS;
        }
        self.tick(now, item);
    }

    /// Adds the same time again: the minutes chosen, or until the end of the next item.
    pub fn extend(&mut self, now: i64, item: SleepItem) {
        match self.state.choice {
            SleepChoice::Off => return,
            SleepChoice::Minutes(m) => self.deadline += m as i64 * MINUTE_MS,
            SleepChoice::EndOfItem => self.spared = Some(item.key),
        }
        self.state.seconds_left = None;
        self.due = None;
        self.tick(now, item);
    }

    /// Whether the timer ends with the item `key`: what comes next waits.
    pub fn ends_with(&self, key: i64) -> bool {
        self.state.choice == SleepChoice::EndOfItem && Some(key) != self.spared
    }

    /// The item `key` played to its end: true when that is the timer's end, which then fires.
    pub fn item_ended(&mut self, key: i64) -> bool {
        if !self.ends_with(key) {
            return false;
        }
        self.fire();
        true
    }

    /// It plays again: the pause the timer made is over.
    pub fn resumed(&mut self) {
        self.state.fired = false;
    }

    /// Off.
    pub fn cancel(&mut self) {
        self.due = None;
        self.state = SLEEP_OFF;
    }

    /// When [`SleepTimer::tick`] has something to do.
    pub fn next_due(&self) -> Option<i64> {
        self.due
    }

    /// Updates what it shows at `now`; true when it fired (the owner pauses).
    pub fn tick(&mut self, now: i64, item: SleepItem) -> bool {
        self.due = None;
        let choice = self.state.choice;
        if choice == SleepChoice::Off {
            return false;
        }
        let left = match choice {
            SleepChoice::Minutes(_) => Some(self.deadline - now),
            _ if Some(item.key) == self.spared => None,
            _ => item.left_ms,
        };
        if self.spared.is_some_and(|s| s != item.key) {
            self.spared = None;
        }
        match left {
            None => self.show(None, None),
            Some(l) if l <= 0 && matches!(choice, SleepChoice::Minutes(_)) => {
                self.fire();
                return true;
            }
            Some(l) if l <= 0 => self.show(None, None),
            Some(l) if l <= NOTICE_MS => self.show(Some(ceil_div(l, SECOND_MS)), Some(1)),
            Some(l) => self.show(None, Some(ceil_div(l, MINUTE_MS))),
        }
        self.due = Some(now + Self::wait(choice, left));
        false
    }

    fn show(&mut self, seconds_left: Option<i64>, minutes_left: Option<i64>) {
        self.state.seconds_left = seconds_left;
        self.state.minutes_left = minutes_left;
    }

    /// Until the next second during the notice; else until the notice or the next minute, polling an item's end each second.
    fn wait(choice: SleepChoice, left: Option<i64>) -> i64 {
        let Some(left) = left.filter(|l| *l > 0) else { return SECOND_MS };
        if choice == SleepChoice::EndOfItem {
            return SECOND_MS;
        }
        if left <= NOTICE_MS {
            return nonzero(left % SECOND_MS, SECOND_MS);
        }
        (left - NOTICE_MS).min(nonzero(left % MINUTE_MS, MINUTE_MS))
    }

    fn fire(&mut self) {
        self.due = None;
        self.spared = None;
        self.state = SleepState { fired: true, ..SLEEP_OFF };
    }
}

fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1).div_euclid(b)
}

fn nonzero(v: i64, otherwise: i64) -> i64 {
    if v == 0 { otherwise } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: SleepItem = SleepItem { key: 1, left_ms: Some(3_600_000) };

    fn run(t: &mut SleepTimer, now: &mut i64, until: i64, item: SleepItem) -> bool {
        while let Some(due) = t.next_due().filter(|d| *d <= until) {
            *now = due;
            if t.tick(*now, item) {
                return true;
            }
        }
        *now = until;
        false
    }

    #[test]
    fn minutes_count_down_then_a_notice_then_it_fires() {
        let (mut t, mut now) = (SleepTimer::default(), 0);
        t.set(now, SleepChoice::Minutes(15), ITEM);
        assert_eq!(t.state.minutes_left, Some(15));
        assert!(!run(&mut t, &mut now, 14 * MINUTE_MS + 1, ITEM));
        assert_eq!(t.state.minutes_left, Some(1));
        assert_eq!(t.state.seconds_left, None);
        assert!(!run(&mut t, &mut now, 15 * MINUTE_MS - 10_000, ITEM));
        assert_eq!(t.state.seconds_left, Some(10));
        assert!(run(&mut t, &mut now, 15 * MINUTE_MS, ITEM));
        assert!(t.state.fired);
        assert_eq!(t.state.choice, SleepChoice::Off);
        t.resumed();
        assert!(!t.state.fired);
    }

    #[test]
    fn keep_watching_adds_the_same_time() {
        let (mut t, mut now) = (SleepTimer::default(), 0);
        t.set(now, SleepChoice::Minutes(30), ITEM);
        run(&mut t, &mut now, 30 * MINUTE_MS - 20_000, ITEM);
        assert_eq!(t.state.seconds_left, Some(20));
        t.extend(now, ITEM);
        assert_eq!(t.state.seconds_left, None);
        assert_eq!(t.state.minutes_left, Some(31));
    }

    #[test]
    fn the_end_of_an_item_fires_with_it_unless_spared() {
        let (mut t, now) = (SleepTimer::default(), 0);
        t.set(now, SleepChoice::EndOfItem, SleepItem { key: 7, left_ms: Some(20_000) });
        assert_eq!(t.state.seconds_left, Some(20));
        assert!(t.ends_with(7));
        t.extend(now, SleepItem { key: 7, left_ms: Some(20_000) });
        assert!(!t.ends_with(7));
        assert!(!t.item_ended(7));
        // The next item is the timer's end again.
        t.tick(now + 1_000, SleepItem { key: 8, left_ms: Some(50_000) });
        assert!(t.ends_with(8));
        assert!(t.item_ended(8));
        assert!(t.state.fired);
    }

    #[test]
    fn labels() {
        assert_eq!(sleep_label(SleepChoice::Off, None), "Off");
        assert_eq!(sleep_label(SleepChoice::Minutes(45), None), "45 min");
        assert_eq!(sleep_label(SleepChoice::EndOfItem, Some(ItemKind::Episode)), "End of this episode");
        assert_eq!(sleep_label(SleepChoice::EndOfItem, Some(ItemKind::Movie)), "End of this movie");
        assert_eq!(sleep_label(SleepChoice::EndOfItem, Some(ItemKind::Track)), "End of this track");
        assert_eq!(sleep_label(SleepChoice::EndOfItem, Some(ItemKind::Extra)), "End of this video");
    }
}
