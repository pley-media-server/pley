//! The remote and the Magic Remote's pointer, as the interface sees them.
//!
//! Each platform maps its own codes to [`Key`] (LG's through SDL, the
//! simulator's from the keyboard); the raw code travels along for Diagnostics.

use crate::geom::Point;

/// What a remote key means to the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    /// OK, or the Magic Remote's wheel click.
    Ok,
    Back,
    /// The menu key (☰, "…" or Q.Menu, by remote): a title's actions.
    Menu,
    Info,
    /// The TV leaves the app itself on Exit; the key still reaches the app on
    /// some sets so it can save its place first.
    Exit,
    PlayPause,
    Play,
    Pause,
    Stop,
    FastForward,
    Rewind,
    /// Next and Previous (or skip forward and back).
    Next,
    Previous,
    /// CH+ and CH− (key codes 33 and 34 on LG).
    ChannelUp,
    ChannelDown,
    /// The channel list key.
    ChannelList,
    Guide,
    /// A digit, 0 to 9.
    Digit(u8),
    Red,
    Green,
    Yellow,
    Blue,
    /// The subtitles (CC) key.
    Subtitles,
    /// The audio track key.
    Audio,
    Search,
    /// A key with no meaning to the app; its raw code is in [`KeyEvent::code`].
    Unknown,
}

impl Key {
    /// Whether this key moves focus.
    pub fn is_direction(self) -> bool {
        matches!(self, Key::Up | Key::Down | Key::Left | Key::Right)
    }
}

/// A key going down or up. Held keys repeat as further presses with `repeat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub pressed: bool,
    pub repeat: bool,
    /// The platform's own code for the key, for Diagnostics and for mapping
    /// keys the app doesn't name.
    pub code: i32,
}

/// The Magic Remote's pointer (the mouse in the simulator), in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    Moved(Point),
    /// The button (the wheel click) went down or up where the pointer is.
    Button {
        pressed: bool,
    },
    /// The wheel turned, in notches: positive when turned away from the viewer
    /// (SDL's y > 0), which moves focus up.
    Wheel(f32),
    /// The pointer was hidden (the remote put it to sleep, or a key was pressed).
    Hidden,
}
