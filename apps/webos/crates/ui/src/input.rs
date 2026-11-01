//! The remote and the Magic Remote as a frame receives them, and the timings that shape them:
//! the key-repeat gate, OK's long press, and the pointer's rules (ported from the TV app's
//! `packages/tv/src/ui/keys.ts` and `input.ts`, which follow the Android TV app).

use pley_base::{KeyEvent, PointerEvent};

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

/// The Android TV app's `KeyRepeatGate` (`ui/tv/kit/KeyRepeatGate.kt`, ported through the TV app's
/// `keys.ts`): while a direction key repeats (LG's remote every ~50 ms), at most one move every
/// 100 ms goes through; a first press always does. Holding a key turns motion off, so a held key
/// moves without animating. Focus has rested once 400 ms passed with no direction key held and
/// no move. A hold whose key-up was lost ends 500 ms after the last key.
#[derive(Clone, Debug)]
pub struct KeyGate {
    holding: bool,
    last_move: f64,
    last_key: f64,
}

impl Default for KeyGate {
    fn default() -> KeyGate {
        KeyGate::new()
    }
}

impl KeyGate {
    /// One move per this many seconds while a key repeats (`MOVE_INTERVAL_MS`).
    pub const MOVE_INTERVAL: f64 = 0.100;
    /// Focus rests after this long without a move (`REST_MS`).
    pub const REST: f64 = 0.400;
    /// A hold ends this long after its last key when the key-up never came.
    pub const LOST_KEY_UP: f64 = 0.500;
    /// Frame times are sums of floats: a time scheduled for a limit must count as reaching it.
    const EPS: f64 = 1e-6;

    /// A gate with no key held, rested.
    pub fn new() -> KeyGate {
        KeyGate { holding: false, last_move: f64::NEG_INFINITY, last_key: f64::NEG_INFINITY }
    }

    /// Whether a direction key's event goes through at `now`: a key-up and a first press always
    /// do; a repeat only once [`KeyGate::MOVE_INTERVAL`] has passed since the last move.
    pub fn admit(&mut self, pressed: bool, repeat: bool, now: f64) -> bool {
        if !pressed {
            self.holding = false;
            self.last_move = now;
            return true;
        }
        self.last_key = now;
        if !repeat {
            self.holding = false;
            self.last_move = now;
            return true;
        }
        self.holding = true;
        if now - self.last_move < KeyGate::MOVE_INTERVAL - KeyGate::EPS {
            return false;
        }
        self.last_move = now;
        true
    }

    /// Something moved without a key (the pointer gave focus): rest starts over.
    pub fn moved(&mut self, now: f64) {
        self.last_move = self.last_move.max(now);
    }

    /// Whether a direction key is held (repeating) at `now`.
    pub fn holding(&mut self, now: f64) -> bool {
        self.expire(now);
        self.holding
    }

    /// Whether focus has rested at `now`: no key held, and no move for [`KeyGate::REST`].
    pub fn rested(&mut self, now: f64) -> bool {
        self.expire(now);
        !self.holding && now - self.last_move >= KeyGate::REST - KeyGate::EPS
    }

    /// When the gate's answer next changes by itself: the hold expiring while a key is held, else
    /// focus resting; `None` once it has rested.
    pub fn next_change(&mut self, now: f64) -> Option<f64> {
        self.expire(now);
        if self.holding {
            return Some(self.last_key + KeyGate::LOST_KEY_UP);
        }
        let at = self.last_move + KeyGate::REST;
        (at - now > KeyGate::EPS).then_some(at)
    }

    fn expire(&mut self, now: f64) {
        if self.holding && now - self.last_key >= KeyGate::LOST_KEY_UP - KeyGate::EPS {
            self.holding = false;
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

    #[test]
    fn a_held_direction_moves_once_per_100_ms() {
        let mut g = KeyGate::new();
        assert!(g.admit(true, false, 0.0));
        // LG repeats every 50 ms after its first delay: every other repeat goes through.
        let admitted: Vec<bool> = (0..8).map(|i| g.admit(true, true, 0.4 + 0.05 * i as f64)).collect();
        assert_eq!(admitted, [true, false, true, false, true, false, true, false]);
        assert!(g.holding(0.75));
        assert!(g.admit(false, false, 0.8));
        assert!(!g.holding(0.8));
    }

    #[test]
    fn a_first_press_always_goes_through_and_ends_a_hold() {
        let mut g = KeyGate::new();
        g.admit(true, false, 0.0);
        g.admit(true, true, 0.5);
        assert!(g.holding(0.52));
        assert!(g.admit(true, false, 0.53));
        assert!(!g.holding(0.53));
    }

    #[test]
    fn a_lost_key_up_ends_the_hold_after_500_ms() {
        let mut g = KeyGate::new();
        g.admit(true, false, 0.0);
        g.admit(true, true, 1.0);
        assert!(g.holding(1.49));
        assert!((g.next_change(1.49).unwrap() - 1.5).abs() < 1e-9);
        assert!(!g.holding(1.5));
        // The hold's last key counts as its last move, so it has rested by then.
        assert!(g.rested(1.5));
    }

    #[test]
    fn rests_400_ms_after_the_last_move() {
        let mut g = KeyGate::new();
        assert!(g.rested(0.0));
        assert_eq!(g.next_change(0.0), None);
        g.admit(true, false, 1.0);
        assert!(!g.rested(1.2));
        assert!((g.next_change(1.2).unwrap() - 1.4).abs() < 1e-9);
        g.admit(false, false, 1.1);
        assert!(!g.rested(1.4));
        assert!(g.rested(1.5));
        g.moved(2.0);
        assert!(!g.rested(2.3));
        assert!(g.rested(2.4));
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
