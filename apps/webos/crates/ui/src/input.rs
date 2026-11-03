//! The remote and the Magic Remote as a frame receives them, and the timings that shape them:
//! the key-repeat gate, OK's long press, and the pointer's rules (ported from the TV app's
//! `packages/tv/src/ui/keys.ts` and `input.ts`, which follow the Android TV app).

use pley_base::{Key, KeyEvent, PointerEvent};

/// OK held this long is a long press (Android's `ViewConfiguration` of older versions; the TV app
/// uses the same 500 ms).
pub const LONG_PRESS: f64 = 0.500;
/// One OK press that webOS sends both as the key and as a click arrives as two events this close.
pub const TWIN: f64 = 0.400;
/// An OK key down longer than this lost its key-up: clicks count again.
pub const OK_HELD_MAX: f64 = 10.0;
/// Wheel notches closer than this are a run, treated as a held key.
pub const WHEEL_RUN: f64 = 0.300;
/// The pointer counts as showing this long after it last moved, for what OK does (webOS hides it
/// a few seconds after).
pub const POINTER_SHOWN: f64 = 3.0;

/// One thing that happened since the last frame, in the order it happened.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Key(KeyEvent),
    Pointer(PointerEvent),
    /// Text typed with the TV's keyboard (or the simulator's), for the field that asked for it.
    Text(String),
}

/// What a frame starts from: the time and everything that happened since the last frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameInput {
    /// The frame's time, in seconds on [`pley_base::time::now`]'s clock.
    pub time: f64,
    pub events: Vec<InputEvent>,
}

impl FrameInput {
    /// A frame at `time` with nothing new.
    pub fn at(time: f64) -> FrameInput {
        FrameInput { time, events: Vec::new() }
    }
}

/// The Magic Remote's wheel stepping focus moves it at most once per this many seconds.
pub const WHEEL_STEP: f64 = 0.100;

/// D-pad rule 5, held keys (the Samsung app's `KeyRepeatGate`, after the Android TV app's): a
/// press moves focus once. Held for 180 ms, focus moves on the app's own clock
/// ([`KeyGate::tick`]) at a flat pace, one move every 45 ms: sooner and more evenly than the TV's
/// repeats, which begin after about 400 ms, come unevenly, and here only say the key is still
/// down. While a key is held, rows, pages, grids and a tile's focus glide evenly over a little
/// more than the time between moves ([`KeyGate::GLIDE`]), so each glide runs into the next;
/// everything else snaps.
/// Focus has rested once 400 ms passed with no direction key held and no move. A key whose
/// key-up was lost counts as let go 500 ms after its last event.
///
/// Times are seconds of the frame clock as `f64`, whose precision holds to well under a
/// microsecond however long the TV has been on (the Samsung app's integer milliseconds overflowed
/// after 106 days).
#[derive(Clone, Debug)]
pub struct KeyGate {
    /// The direction keys down.
    down: Vec<Key>,
    /// The direction pressed and not let go yet: held once [`KeyGate::HOLD_DELAY`] has passed.
    pressed: Option<Key>,
    pressed_at: f64,
    holding: bool,
    /// The direction moving focus by itself; `None` while the wheel holds.
    held: Option<Key>,
    next_move: f64,
    last_move: f64,
    last_key: f64,
}

impl Default for KeyGate {
    fn default() -> KeyGate {
        KeyGate::new()
    }
}

impl KeyGate {
    /// How long a key is down before focus starts moving by itself (`HoldDelayMs`).
    pub const HOLD_DELAY: f64 = 0.180;
    /// The time between moves while a key is held: about 22 a second (`MoveIntervalMs`).
    pub const MOVE_INTERVAL: f64 = 0.045;
    /// How long a held move's glide lasts: a quarter more than [`KeyGate::MOVE_INTERVAL`], so the
    /// next starts before it ends (`GlideMs`).
    pub const GLIDE: f64 = KeyGate::MOVE_INTERVAL * 1.25;
    /// Focus rests after this long without a move (`RestMs`).
    pub const REST: f64 = 0.400;
    /// A key is taken as let go this long after its last event when the key-up never came (the
    /// TV repeats every 50 ms or so).
    pub const LOST_KEY_UP: f64 = 0.500;
    /// Frame times are sums of floats: a time scheduled for a limit must count as reaching it.
    const EPS: f64 = 1e-6;

    /// A gate with no key held, rested.
    pub fn new() -> KeyGate {
        KeyGate {
            down: Vec::new(),
            pressed: None,
            pressed_at: f64::NEG_INFINITY,
            holding: false,
            held: None,
            next_move: f64::NEG_INFINITY,
            last_move: f64::NEG_INFINITY,
            last_key: f64::NEG_INFINITY,
        }
    }

    /// Whether a direction key's event goes through at `now`: a key-up and a first press do; the
    /// TV's repeats don't, as [`KeyGate::tick`] moves focus while a key is held. A repeat of a
    /// key the gate didn't see go down counts as its press.
    pub fn admit(&mut self, key: Key, pressed: bool, now: f64) -> bool {
        if !pressed {
            self.down.retain(|k| *k != key);
            if self.pressed == Some(key) {
                self.pressed = None;
            }
            self.holding = false;
            self.held = None;
            self.last_move = now;
            return true;
        }
        self.last_key = now;
        if !self.down.contains(&key) {
            self.down.push(key);
            self.holding = false;
            self.held = None;
            self.pressed = Some(key);
            self.pressed_at = now;
            self.last_move = now;
            return true;
        }
        if !self.holding || self.held != Some(key) {
            self.hold(key, now);
        }
        false
    }

    /// Called each frame: the direction to move focus in now, if any. Moves come
    /// [`KeyGate::MOVE_INTERVAL`] apart on average, whenever the frames fall.
    pub fn tick(&mut self, now: f64) -> Option<Key> {
        self.expire(now);
        // Down long enough: the hold begins, before the TV's own repeats would.
        if !self.holding
            && let Some(key) = self.pressed.filter(|k| self.down.contains(k))
            && now - self.pressed_at >= KeyGate::HOLD_DELAY - KeyGate::EPS
        {
            self.hold(key, now);
        }
        let key = self.held.filter(|_| self.holding)?;
        if now < self.next_move - KeyGate::EPS {
            return None;
        }
        // Kept to the pace: a late frame doesn't push the next move later, nor does a stall bring
        // a burst.
        self.next_move = (self.next_move + KeyGate::MOVE_INTERVAL).max(now + KeyGate::MOVE_INTERVAL / 2.0);
        self.last_move = now;
        Some(key)
    }

    fn hold(&mut self, key: Key, now: f64) {
        self.holding = true;
        self.held = Some(key);
        // The first held move is due at once.
        self.next_move = now;
    }

    /// The wheel stepped focus at `now`; `run` says the notch came soon after the one before, so
    /// the wheel holds as a key would (rows and focus glide) until it rests.
    pub fn wheeled(&mut self, run: bool, now: f64) {
        self.last_move = now;
        if run {
            self.holding = true;
            self.last_key = now;
        } else if self.held.is_none() {
            self.holding = false;
        }
    }

    /// Something moved without a key (the pointer gave focus): rest starts over.
    pub fn moved(&mut self, now: f64) {
        self.last_move = self.last_move.max(now);
    }

    /// Whether a direction key (or the wheel) is held at `now`, moving focus by itself.
    pub fn holding(&mut self, now: f64) -> bool {
        self.expire(now);
        self.holding
    }

    /// Whether a direction key is down: frames keep coming for [`KeyGate::tick`].
    pub fn down(&self) -> bool {
        self.pressed.is_some() || self.holding
    }

    /// Whether focus has rested at `now`: no key held, and no move for [`KeyGate::REST`].
    pub fn rested(&mut self, now: f64) -> bool {
        self.expire(now);
        !self.holding && now - self.last_move >= KeyGate::REST - KeyGate::EPS
    }

    /// When the gate's answer next changes by itself: while a key is down, its hold beginning,
    /// its next move or its key-up counting as lost; else focus resting; `None` once it has
    /// rested.
    pub fn next_change(&mut self, now: f64) -> Option<f64> {
        self.expire(now);
        if self.down() {
            let lost = self.last_key + KeyGate::LOST_KEY_UP;
            let due = match (self.holding, self.held, self.pressed) {
                (true, Some(_), _) => self.next_move,
                (false, _, Some(_)) => self.pressed_at + KeyGate::HOLD_DELAY,
                _ => lost,
            };
            return Some(due.min(lost).max(now));
        }
        let at = self.last_move + KeyGate::REST;
        (at - now > KeyGate::EPS).then_some(at)
    }

    fn expire(&mut self, now: f64) {
        if self.down() && now - self.last_key >= KeyGate::LOST_KEY_UP - KeyGate::EPS {
            self.holding = false;
            self.held = None;
            self.pressed = None;
            self.down.clear();
            self.last_move = self.last_key;
        }
    }
}

/// Where a scroller goes when the pointer focused an element in it: nowhere, unless the element
/// is cut off at the view's edge, and then only as far as brings it in whole, so the pointer stays
/// on what it points at, as in LG's own apps (the TV app's `nudge`). `shift` is how far the
/// content is moved (a content position less `shift` is where it shows); the element spans `lo`
/// to `hi` in the content and the view shows `view_lo` to `view_hi`. The result stays within
/// `min` and `max`; an element taller than the view shows from its top.
pub fn nudge(shift: f32, lo: f32, hi: f32, view_lo: f32, view_hi: f32, min: f32, max: f32) -> f32 {
    let mut s = shift;
    if hi - s > view_hi {
        s = hi - view_hi;
    }
    if lo - s < view_lo {
        s = lo - view_lo;
    }
    s.min(max).max(min)
}

#[cfg(test)]
mod tests {
    use super::*;

    // apps/tizen-native/test/Pley.Tv.Core.Tests/KeyRepeatGateTests.cs

    #[test]
    fn a_press_moves_once_and_held_180_ms_focus_moves_by_itself() {
        let mut g = KeyGate::new();
        assert!(g.admit(Key::Right, true, 1.0));
        assert!(g.down());
        assert!(!g.holding(1.0));
        assert_eq!(g.tick(1.15), None);
        // Before the TV's repeats (400 ms), the hold begins, and its first move is due at once.
        assert_eq!(g.tick(1.18), Some(Key::Right));
        assert!(g.holding(1.18));
        assert_eq!(g.tick(1.2), None);
        // The TV's repeats only say the key is still down.
        assert!(!g.admit(Key::Right, true, 1.2));
        assert_eq!(g.tick(1.24), Some(Key::Right));
        assert!(g.admit(Key::Right, false, 1.24));
        assert!(!g.down());
        assert_eq!(g.tick(1.3), None);
    }

    #[test]
    fn a_quick_press_never_starts_a_hold() {
        let mut g = KeyGate::new();
        g.admit(Key::Left, true, 1.0);
        g.admit(Key::Left, false, 1.12);
        assert_eq!(g.tick(1.62), None);
        assert!(!g.holding(1.62));
    }

    #[test]
    fn a_hold_ends_with_its_key_up_or_when_the_key_up_was_lost() {
        let mut g = KeyGate::new();
        g.admit(Key::Right, true, 1.0);
        g.tick(1.3);
        assert!(g.holding(1.3));
        g.admit(Key::Right, false, 1.3);
        assert!(!g.holding(1.3));
        // A hold whose key-up was lost ends 500 ms after the key's last event.
        g.admit(Key::Left, true, 2.0);
        g.tick(2.3);
        assert!(g.holding(2.49));
        assert_eq!(g.tick(2.49), Some(Key::Left));
        assert!((g.next_change(2.49).unwrap() - 2.5).abs() < 1e-9);
        assert!(!g.holding(2.5));
        assert!(!g.down());
    }

    /// Two seconds of frames 17 ms apart from `start`, the TV repeating every 50 ms from 400 ms:
    /// the moves made.
    fn held_for_two_seconds(start: f64) -> usize {
        let mut g = KeyGate::new();
        g.admit(Key::Down, true, start);
        let mut moves = 0;
        for t in (0..2000).step_by(17) {
            let now = start + t as f64 / 1000.0;
            if t >= 400 && t % 50 < 17 {
                g.admit(Key::Down, true, now);
            }
            moves += g.tick(now).is_some() as usize;
        }
        moves
    }

    #[test]
    fn moves_keep_their_pace_whenever_the_frames_fall() {
        // Nothing for 180 ms, then one every 45 ms: about 41.
        assert!((39..=43).contains(&held_for_two_seconds(1.0)), "{}", held_for_two_seconds(1.0));
        const { assert!(KeyGate::GLIDE > KeyGate::MOVE_INTERVAL) };
    }

    #[test]
    fn held_keys_keep_their_pace_on_a_tv_on_for_months() {
        // The Samsung app's millisecond clock overflowed after 106 days; seconds in f64 keep
        // their precision far past that.
        for days in [106.0, 107.0, 1000.0] {
            let start = days * 86_400.0 + 0.123;
            assert_eq!(held_for_two_seconds(start), held_for_two_seconds(1.0), "{days} days");
        }
    }

    #[test]
    fn rests_400_ms_after_the_last_move_and_ends_a_hold_whose_key_up_was_lost() {
        let mut g = KeyGate::new();
        assert!(g.rested(0.0));
        assert_eq!(g.next_change(0.0), None);
        g.admit(Key::Down, true, 1.0);
        // Down, not yet held: the hold is the next change.
        assert!((g.next_change(1.0).unwrap() - 1.18).abs() < 1e-9);
        g.admit(Key::Down, true, 1.05);
        assert!(g.holding(1.05));
        assert!(!g.rested(1.05));
        // No repeat for 500 ms: the key-up was lost, and focus has rested since the last key.
        assert!(g.rested(1.65));
        assert!(!g.holding(1.65));
        assert!(!g.down());
        assert_eq!(g.tick(1.65), None);
        assert_eq!(g.next_change(1.65), None);
        // The key's next event is a press again.
        assert!(g.admit(Key::Down, true, 1.7));
        assert!(!g.holding(1.7));
        g.admit(Key::Down, false, 1.75);
        assert!(!g.rested(2.1));
        assert!(g.rested(2.15));
        g.moved(3.0);
        assert!(!g.rested(3.3));
        assert!(g.rested(3.4));
    }

    #[test]
    fn a_run_of_wheel_notches_holds_without_moving_by_itself() {
        let mut g = KeyGate::new();
        g.wheeled(false, 1.0);
        assert!(!g.holding(1.0));
        g.wheeled(true, 1.15);
        assert!(g.holding(1.2));
        assert_eq!(g.tick(1.3), None);
        assert!(!g.holding(1.65));
        assert!(g.rested(1.65));
    }

    #[test]
    fn nudge_moves_only_as_far_as_brings_an_element_in() {
        let inf = f32::INFINITY;
        // Shown whole: nowhere.
        assert_eq!(nudge(100.0, 200.0, 300.0, 0.0, 540.0, 0.0, inf), 100.0);
        // Its bottom at 700, shown down to 100 + 540: 60 more.
        assert_eq!(nudge(100.0, 600.0, 700.0, 0.0, 540.0, 0.0, inf), 160.0);
        // Its top at 350, shown from 300 + 160 (a clip line): back 110.
        assert_eq!(nudge(300.0, 350.0, 450.0, 160.0, 540.0, 0.0, inf), 190.0);
        // Its top at 120: back 140, but not past the content's top.
        assert_eq!(nudge(100.0, 120.0, 300.0, 160.0, 540.0, 0.0, inf), 0.0);
        // Within bounds, and an element taller than the view from its top.
        assert_eq!(nudge(0.0, 900.0, 1000.0, 0.0, 540.0, 0.0, 200.0), 200.0);
        assert_eq!(nudge(0.0, 100.0, 900.0, 0.0, 540.0, 0.0, inf), 100.0);
    }
}
