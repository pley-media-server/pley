//! The simulator's `--keys` script: keys, waits, typed text and the pointer, played once the
//! first page shows, as a person with a remote would.

use pley_base::{Key, KeyEvent, Point, PointerEvent};
use pley_ui::InputEvent;

/// How long a scripted key is held down: long enough for one frame to see it down.
const HOLD: f64 = 0.040;
/// The pause after each scripted key or click, so each lands on a frame that drew the last.
const GAP: f64 = 0.150;

/// One step of a script.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Key(Key),
    /// Waits this many seconds.
    Wait(f64),
    Text(String),
    /// Moves the pointer to a point in dp.
    Point(Point),
    /// Clicks where the pointer is.
    Click,
}

/// The key named `name` (`ok`, `back`, `chup`, `7`…).
pub fn key(name: &str) -> Option<Key> {
    Some(match name {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "ok" | "enter" => Key::Ok,
        "back" => Key::Back,
        "menu" => Key::Menu,
        "info" => Key::Info,
        "exit" => Key::Exit,
        "play" => Key::Play,
        "pause" => Key::Pause,
        "playpause" => Key::PlayPause,
        "stop" => Key::Stop,
        "ff" => Key::FastForward,
        "rew" => Key::Rewind,
        "next" => Key::Next,
        "prev" => Key::Previous,
        "chup" => Key::ChannelUp,
        "chdown" => Key::ChannelDown,
        "red" => Key::Red,
        "green" => Key::Green,
        "yellow" => Key::Yellow,
        "blue" => Key::Blue,
        "subtitles" => Key::Subtitles,
        "audio" => Key::Audio,
        "search" => Key::Search,
        "guide" => Key::Guide,
        d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => Key::Digit(d.as_bytes()[0] - b'0'),
        _ => return None,
    })
}

/// Reads a script: words separated by spaces.
pub fn parse(script: &str) -> Result<Vec<Step>, String> {
    script
        .split_whitespace()
        .map(|w| {
            if let Some(ms) = w.strip_prefix("wait:") {
                return ms.parse::<f64>().map(|ms| Step::Wait(ms / 1000.0)).map_err(|_| format!("bad wait: {w}"));
            }
            if let Some(text) = w.strip_prefix("text:") {
                return Ok(Step::Text(text.to_string()));
            }
            if let Some(at) = w.strip_prefix("point:") {
                let (x, y) = at.split_once(',').ok_or_else(|| format!("bad point: {w}"))?;
                let (x, y) = (x.parse::<f32>(), y.parse::<f32>());
                return match (x, y) {
                    (Ok(x), Ok(y)) => Ok(Step::Point(Point::new(x, y))),
                    _ => Err(format!("bad point: {w}")),
                };
            }
            if w == "click" {
                return Ok(Step::Click);
            }
            key(&w.to_ascii_lowercase()).map(Step::Key).ok_or_else(|| format!("unknown key: {w}"))
        })
        .collect()
}

/// A script being played.
#[derive(Debug)]
pub struct Player {
    steps: Vec<Step>,
    next: usize,
    /// When the next event is due.
    at: f64,
    /// A key down whose release is due.
    held: Option<Key>,
}

impl Player {
    /// Plays `steps` from `start`.
    pub fn new(steps: Vec<Step>, start: f64) -> Player {
        Player { steps, next: 0, at: start, held: None }
    }

    /// Starts playing at `t` (when it hasn't started yet).
    pub fn start_at(&mut self, t: f64) {
        if self.next == 0 && self.held.is_none() {
            self.at = t;
        }
    }

    /// Whether every step was played.
    pub fn done(&self) -> bool {
        self.held.is_none() && self.next >= self.steps.len()
    }

    /// When it next has something to do.
    pub fn due(&self) -> Option<f64> {
        (!self.done()).then_some(self.at)
    }

    /// The input due by `now`.
    pub fn take(&mut self, now: f64) -> Vec<InputEvent> {
        let mut out = Vec::new();
        while !self.done() && now >= self.at {
            if let Some(k) = self.held.take() {
                out.push(InputEvent::Key(KeyEvent { key: k, pressed: false, repeat: false, code: 0 }));
                self.at = now + GAP;
                // One frame between a key's release and what follows.
                break;
            }
            let step = self.steps[self.next].clone();
            self.next += 1;
            match step {
                Step::Key(k) => {
                    out.push(InputEvent::Key(KeyEvent { key: k, pressed: true, repeat: false, code: 0 }));
                    self.held = Some(k);
                    self.at = now + HOLD;
                    break;
                }
                Step::Wait(s) => self.at = now + s,
                Step::Text(t) => {
                    out.push(InputEvent::Text(t));
                    self.at = now + GAP;
                    break;
                }
                Step::Point(p) => {
                    out.push(InputEvent::Pointer(PointerEvent::Moved(p)));
                    self.at = now + GAP;
                    break;
                }
                Step::Click => {
                    out.push(InputEvent::Pointer(PointerEvent::Button { pressed: true }));
                    out.push(InputEvent::Pointer(PointerEvent::Button { pressed: false }));
                    self.at = now + GAP;
                    break;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_keys_waits_and_the_pointer() {
        let s = parse("down down OK wait:1500 text:abc point:100,200 click 7 chup").unwrap();
        assert_eq!(
            s,
            [
                Step::Key(Key::Down),
                Step::Key(Key::Down),
                Step::Key(Key::Ok),
                Step::Wait(1.5),
                Step::Text("abc".into()),
                Step::Point(Point::new(100.0, 200.0)),
                Step::Click,
                Step::Key(Key::Digit(7)),
                Step::Key(Key::ChannelUp),
            ]
        );
        assert!(parse("jump").is_err());
        assert!(parse("wait:soon").is_err());
    }

    #[test]
    fn a_key_goes_down_then_up_then_waits() {
        let mut p = Player::new(parse("ok wait:1000 back").unwrap(), 0.0);
        let down = p.take(0.0);
        assert!(matches!(down[..], [InputEvent::Key(KeyEvent { key: Key::Ok, pressed: true, .. })]));
        assert!(p.take(0.01).is_empty());
        let up = p.take(HOLD);
        assert!(matches!(up[..], [InputEvent::Key(KeyEvent { key: Key::Ok, pressed: false, .. })]));
        // The gap, then the wait.
        assert!(p.take(HOLD + GAP).is_empty());
        assert!(p.take(HOLD + GAP + 0.5).is_empty());
        let back = p.take(HOLD + GAP + 1.0);
        assert!(matches!(back[..], [InputEvent::Key(KeyEvent { key: Key::Back, pressed: true, .. })]));
        assert!(!p.done());
        p.take(10.0);
        assert!(p.done());
        assert_eq!(p.due(), None);
    }
}
