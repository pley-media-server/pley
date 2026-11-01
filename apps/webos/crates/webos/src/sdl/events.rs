//! SDL events read from the raw `SDL_Event` buffer at the offsets of LG's
//! SDL, and turned into the platform's events.
//!
//! LG's fork inserts an `inputSource` word after `windowID` in keyboard and
//! text events, which moves their fields 4 bytes from upstream SDL's: a
//! binding generated from upstream headers would misread every key. So events are read by
//! explicit offsets, never through such a binding. Mouse events keep SDL's
//! layout.

use pley_base::{KeyEvent, Point, PointerEvent};
use pley_platform::PlatformEvent;

use super::keys::{is_cursor_notice, map_key};

/// Room for one event: SDL2's `SDL_Event` is 56 bytes; LG's fork writes more
/// than the headers declare elsewhere, so the buffer has room to spare.
pub const EVENT_BYTES: usize = 128;

/// SDL2's event types.
pub mod kind {
    pub const QUIT: u32 = 0x100;
    pub const WILL_ENTER_BACKGROUND: u32 = 0x103;
    pub const DID_ENTER_BACKGROUND: u32 = 0x104;
    pub const WILL_ENTER_FOREGROUND: u32 = 0x105;
    pub const DID_ENTER_FOREGROUND: u32 = 0x106;
    pub const WINDOW: u32 = 0x200;
    pub const KEY_DOWN: u32 = 0x300;
    pub const KEY_UP: u32 = 0x301;
    pub const TEXT_INPUT: u32 = 0x303;
    pub const MOUSE_MOTION: u32 = 0x400;
    pub const MOUSE_BUTTON_DOWN: u32 = 0x401;
    pub const MOUSE_BUTTON_UP: u32 = 0x402;
    pub const MOUSE_WHEEL: u32 = 0x403;
    /// The first type `SDL_RegisterEvents` hands out.
    pub const USER: u32 = 0x8000;
}

/// SDL2's window events (`SDL_WindowEventID`) the platform acts on.
mod window {
    pub const RESIZED: u8 = 5;
    pub const SIZE_CHANGED: u8 = 6;
    pub const CLOSE: u8 = 14;
}

/// Field offsets in LG's layout.
mod at {
    pub const KEY_STATE: usize = 16;
    pub const KEY_REPEAT: usize = 17;
    pub const KEY_SCANCODE: usize = 20;
    pub const KEY_SYM: usize = 24;
    pub const TEXT: usize = 16;
    pub const TEXT_LEN: usize = 32;
    pub const WINDOW_EVENT: usize = 12;
    pub const MOUSE_X: usize = 20;
    pub const MOUSE_Y: usize = 24;
    pub const BUTTON: usize = 16;
    pub const BUTTON_STATE: usize = 17;
    pub const WHEEL_Y: usize = 20;
    pub const WHEEL_DIRECTION: usize = 24;
    pub const USER_CODE: usize = 12;
}

/// The left button: the Magic Remote's wheel click.
const BUTTON_LEFT: u8 = 1;
/// `SDL_MOUSEWHEEL_FLIPPED`: the deltas are the other way round.
const WHEEL_FLIPPED: u32 = 1;

/// What the user events the app pushes to itself ask for.
pub mod user {
    /// A worker's waker: run the loop.
    pub const WAKE: i32 = 1;
    /// Hide the app (the fallback when the TV's Home can't be shown).
    pub const MINIMIZE: i32 = 2;
}

/// One event, read.
#[derive(Clone, Debug, PartialEq)]
pub enum SdlEvent {
    Quit,
    /// 0x103 or 0x104.
    Background,
    /// 0x105 or 0x106.
    Foreground,
    Resized,
    Closed,
    Key {
        pressed: bool,
        repeat: bool,
        scancode: u32,
        keycode: i32,
    },
    Text(String),
    Motion {
        x: i32,
        y: i32,
    },
    Button {
        button: u8,
        pressed: bool,
    },
    /// Notches, positive away from the viewer.
    Wheel(i32),
    /// One of the app's own events (`user_type`), with its code.
    User(i32),
    Other(u32),
}

fn u32_at(buf: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(buf[at..at + 4].try_into().expect("four bytes"))
}

fn i32_at(buf: &[u8], at: usize) -> i32 {
    i32::from_ne_bytes(buf[at..at + 4].try_into().expect("four bytes"))
}

/// Reads the event in `buf`; `user_type` is the type the app registered for its own.
pub fn parse(buf: &[u8; EVENT_BYTES], user_type: u32) -> SdlEvent {
    let ty = u32_at(buf, 0);
    match ty {
        kind::QUIT => SdlEvent::Quit,
        kind::WILL_ENTER_BACKGROUND | kind::DID_ENTER_BACKGROUND => SdlEvent::Background,
        kind::WILL_ENTER_FOREGROUND | kind::DID_ENTER_FOREGROUND => SdlEvent::Foreground,
        kind::WINDOW => match buf[at::WINDOW_EVENT] {
            window::RESIZED | window::SIZE_CHANGED => SdlEvent::Resized,
            window::CLOSE => SdlEvent::Closed,
            _ => SdlEvent::Other(ty),
        },
        kind::KEY_DOWN | kind::KEY_UP => SdlEvent::Key {
            pressed: buf[at::KEY_STATE] == 1,
            repeat: buf[at::KEY_REPEAT] != 0,
            scancode: u32_at(buf, at::KEY_SCANCODE),
            keycode: i32_at(buf, at::KEY_SYM),
        },
        kind::TEXT_INPUT => {
            let raw = &buf[at::TEXT..at::TEXT + at::TEXT_LEN];
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            SdlEvent::Text(String::from_utf8_lossy(&raw[..end]).into_owned())
        }
        kind::MOUSE_MOTION => SdlEvent::Motion { x: i32_at(buf, at::MOUSE_X), y: i32_at(buf, at::MOUSE_Y) },
        kind::MOUSE_BUTTON_DOWN | kind::MOUSE_BUTTON_UP => {
            SdlEvent::Button { button: buf[at::BUTTON], pressed: buf[at::BUTTON_STATE] == 1 }
        }
        kind::MOUSE_WHEEL => {
            let y = i32_at(buf, at::WHEEL_Y);
            SdlEvent::Wheel(if u32_at(buf, at::WHEEL_DIRECTION) == WHEEL_FLIPPED { -y } else { y })
        }
        t if t == user_type => SdlEvent::User(i32_at(buf, at::USER_CODE)),
        _ => SdlEvent::Other(ty),
    }
}

/// What the platform must do itself after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Read the drawable's size again and report it.
    Resized,
    /// Back in front: the surface may be new, so clear its opaque region again.
    Foregrounded,
    /// The waker's event arrived: it may send another.
    Woken,
    Minimize,
}

/// Turns SDL's events into the platform's, with the little state that needs:
/// lifecycle edges come twice (will and did) in no promised order, and are
/// reported once each way.
#[derive(Debug)]
pub struct Translator {
    visible: bool,
    quit: bool,
}

impl Default for Translator {
    fn default() -> Translator {
        Translator { visible: true, quit: false }
    }
}

impl Translator {
    /// Appends what `event` means to `out`, and says what the platform must do.
    pub fn translate(&mut self, event: SdlEvent, out: &mut Vec<PlatformEvent>) -> Option<Effect> {
        match event {
            SdlEvent::Quit | SdlEvent::Closed => {
                if !std::mem::replace(&mut self.quit, true) {
                    out.push(PlatformEvent::Quit);
                }
            }
            SdlEvent::Background => {
                if std::mem::replace(&mut self.visible, false) {
                    out.push(PlatformEvent::Visible(false));
                }
            }
            SdlEvent::Foreground => {
                if !std::mem::replace(&mut self.visible, true) {
                    out.push(PlatformEvent::Visible(true));
                }
                return Some(Effect::Foregrounded);
            }
            SdlEvent::Resized => return Some(Effect::Resized),
            SdlEvent::Key { pressed, repeat, scancode, keycode } => {
                if is_cursor_notice(scancode) {
                    // LG's header names 484 "shown" and 485 "hidden", while 484 was
                    // measured as the pointer hiding itself; either way a pointer that
                    // shows also moves, and its motion brings pointer mode back.
                    if pressed && !repeat {
                        out.push(PlatformEvent::Pointer(PointerEvent::Hidden));
                    }
                } else {
                    let key = map_key(scancode, keycode);
                    out.push(PlatformEvent::Key(KeyEvent { key, pressed, repeat, code: scancode as i32 }));
                }
            }
            SdlEvent::Text(text) => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                if !text.is_empty() {
                    out.push(PlatformEvent::Text(text));
                }
            }
            SdlEvent::Motion { x, y } => {
                out.push(PlatformEvent::Pointer(PointerEvent::Moved(Point::new(x as f32, y as f32))));
            }
            SdlEvent::Button { button, pressed } => {
                if button == BUTTON_LEFT {
                    out.push(PlatformEvent::Pointer(PointerEvent::Button { pressed }));
                }
            }
            SdlEvent::Wheel(notches) => {
                if notches != 0 {
                    out.push(PlatformEvent::Pointer(PointerEvent::Wheel(notches as f32)));
                }
            }
            SdlEvent::User(user::WAKE) => return Some(Effect::Woken),
            SdlEvent::User(user::MINIMIZE) => return Some(Effect::Minimize),
            SdlEvent::User(_) | SdlEvent::Other(_) => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use pley_base::Key;

    use super::*;

    const USER: u32 = kind::USER;

    fn event(ty: u32, fields: &[(usize, &[u8])]) -> [u8; EVENT_BYTES] {
        let mut buf = [0u8; EVENT_BYTES];
        buf[0..4].copy_from_slice(&ty.to_ne_bytes());
        // A timestamp and window id, which nothing reads.
        buf[4..8].copy_from_slice(&123_456u32.to_ne_bytes());
        buf[8..12].copy_from_slice(&1u32.to_ne_bytes());
        for (at, bytes) in fields {
            buf[*at..*at + bytes.len()].copy_from_slice(bytes);
        }
        buf
    }

    /// A key event in LG's layout: `inputSource` at +12, then state, repeat,
    /// padding, the scancode at +20 and the keycode at +24.
    fn key(ty: u32, state: u8, repeat: u8, scancode: u32, keycode: i32) -> [u8; EVENT_BYTES] {
        event(
            ty,
            &[
                (12, &7u32.to_ne_bytes()),
                (16, &[state, repeat]),
                (20, &scancode.to_ne_bytes()),
                (24, &keycode.to_ne_bytes()),
            ],
        )
    }

    fn run(events: &[[u8; EVENT_BYTES]]) -> (Vec<PlatformEvent>, Vec<Effect>) {
        let mut t = Translator::default();
        let mut out = Vec::new();
        let effects = events.iter().filter_map(|e| t.translate(parse(e, USER), &mut out)).collect();
        (out, effects)
    }

    #[test]
    fn keys_are_read_at_lgs_offsets() {
        assert_eq!(
            parse(&key(kind::KEY_DOWN, 1, 0, 482, 0), USER),
            SdlEvent::Key { pressed: true, repeat: false, scancode: 482, keycode: 0 }
        );
        let (out, _) = run(&[
            key(kind::KEY_DOWN, 1, 0, 482, 0),
            key(kind::KEY_UP, 0, 0, 482, 0),
            key(kind::KEY_DOWN, 1, 0, 81, 81 | 1 << 30),
            key(kind::KEY_DOWN, 1, 1, 81, 81 | 1 << 30),
            key(kind::KEY_DOWN, 1, 0, 33, 52),
        ]);
        assert_eq!(
            out,
            [
                PlatformEvent::Key(KeyEvent { key: Key::Back, pressed: true, repeat: false, code: 482 }),
                PlatformEvent::Key(KeyEvent { key: Key::Back, pressed: false, repeat: false, code: 482 }),
                PlatformEvent::Key(KeyEvent { key: Key::Down, pressed: true, repeat: false, code: 81 }),
                PlatformEvent::Key(KeyEvent { key: Key::Down, pressed: true, repeat: true, code: 81 }),
                PlatformEvent::Key(KeyEvent { key: Key::Digit(4), pressed: true, repeat: false, code: 33 }),
            ]
        );
    }

    #[test]
    fn upstreams_offsets_would_misread_them() {
        // Upstream SDL puts the state at +12 and the scancode at +16: here
        // `inputSource` and the state, so reading there would be wrong.
        let buf = key(kind::KEY_DOWN, 1, 0, 482, 0);
        assert_ne!(u32_at(&buf, 16), 482);
        assert_eq!(u32_at(&buf, 20), 482);
    }

    #[test]
    fn text_starts_after_the_input_source() {
        let mut text = [0u8; 32];
        text[..3].copy_from_slice("é1".as_bytes());
        let buf = event(kind::TEXT_INPUT, &[(12, &9u32.to_ne_bytes()), (16, &text)]);
        assert_eq!(parse(&buf, USER), SdlEvent::Text("é1".into()));
        let mut control = [0u8; 32];
        control[0] = 8;
        let (out, _) = run(&[buf, event(kind::TEXT_INPUT, &[(16, &control)])]);
        assert_eq!(out, [PlatformEvent::Text("é1".into())]);
    }

    #[test]
    fn the_magic_remote_moves_clicks_and_turns() {
        let (out, _) = run(&[
            event(
                kind::MOUSE_MOTION,
                &[(12, &0u32.to_ne_bytes()), (20, &960i32.to_ne_bytes()), (24, &540i32.to_ne_bytes())],
            ),
            event(kind::MOUSE_BUTTON_DOWN, &[(16, &[1, 1, 1]), (20, &960i32.to_ne_bytes())]),
            event(kind::MOUSE_BUTTON_UP, &[(16, &[1, 0, 1])]),
            event(kind::MOUSE_BUTTON_DOWN, &[(16, &[3, 1, 1])]),
            event(kind::MOUSE_WHEEL, &[(16, &0i32.to_ne_bytes()), (20, &(-1i32).to_ne_bytes())]),
            event(kind::MOUSE_WHEEL, &[(20, &1i32.to_ne_bytes()), (24, &WHEEL_FLIPPED.to_ne_bytes())]),
            key(kind::KEY_DOWN, 1, 0, 484, 0),
            key(kind::KEY_UP, 0, 0, 484, 0),
        ]);
        assert_eq!(
            out,
            [
                PlatformEvent::Pointer(PointerEvent::Moved(Point::new(960.0, 540.0))),
                PlatformEvent::Pointer(PointerEvent::Button { pressed: true }),
                PlatformEvent::Pointer(PointerEvent::Button { pressed: false }),
                PlatformEvent::Pointer(PointerEvent::Wheel(-1.0)),
                PlatformEvent::Pointer(PointerEvent::Wheel(-1.0)),
                PlatformEvent::Pointer(PointerEvent::Hidden),
            ]
        );
    }

    #[test]
    fn lifecycle_edges_are_reported_once_each_way() {
        let (out, effects) = run(&[
            event(kind::WILL_ENTER_BACKGROUND, &[]),
            event(kind::DID_ENTER_BACKGROUND, &[]),
            event(kind::DID_ENTER_FOREGROUND, &[]),
            event(kind::WILL_ENTER_FOREGROUND, &[]),
            event(kind::QUIT, &[]),
            event(kind::WINDOW, &[(12, &[window::CLOSE])]),
        ]);
        assert_eq!(out, [PlatformEvent::Visible(false), PlatformEvent::Visible(true), PlatformEvent::Quit]);
        assert_eq!(effects, [Effect::Foregrounded, Effect::Foregrounded]);
    }

    #[test]
    fn the_apps_own_events_wake_and_minimize() {
        let user = |code: i32| event(USER, &[(12, &code.to_ne_bytes())]);
        let (out, effects) =
            run(&[user(user::WAKE), user(user::MINIMIZE), user(9), event(kind::WINDOW, &[(12, &[6])])]);
        assert!(out.is_empty());
        assert_eq!(effects, [Effect::Woken, Effect::Minimize, Effect::Resized]);
        assert_eq!(parse(&event(0x700, &[]), USER), SdlEvent::Other(0x700));
    }
}
