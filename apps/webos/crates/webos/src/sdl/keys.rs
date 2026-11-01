//! The remote's keys as LG's SDL delivers them (DESIGN.md §5.2,
//! RES/plx-ui-gfx.md §10.1, RES/webos-ecosystem.md §2.7), and what each means
//! to the app (RES/tv-app-core.md §15.1).
//!
//! Arrows, OK and the digits arrive as ordinary SDL keys; LG's own keys as
//! scancodes from 300 up with keycode 0, so those are matched on the
//! scancode. Digits are matched on the keycode: their scancodes (30–39) are
//! ordinary keyboard positions, and 33 and 34 there are the digits 4 and 5,
//! not the web's CH+ and CH−.

use pley_base::Key;

/// SDL's standard scancodes (`SDL_scancode.h`).
pub mod scancode {
    pub const RETURN: u32 = 40;
    pub const ESCAPE: u32 = 41;
    pub const BACKSPACE: u32 = 42;
    pub const PAUSE: u32 = 72;
    pub const PAGE_UP: u32 = 75;
    pub const PAGE_DOWN: u32 = 78;
    pub const RIGHT: u32 = 79;
    pub const LEFT: u32 = 80;
    pub const DOWN: u32 = 81;
    pub const UP: u32 = 82;
    pub const KP_ENTER: u32 = 88;
    /// The keypad's 1 to 9 are 89 to 97, its 0 is 98.
    pub const KP_1: u32 = 89;
    pub const KP_0: u32 = 98;
    pub const APPLICATION: u32 = 101;
    pub const MENU: u32 = 118;
    pub const STOP: u32 = 120;
    pub const AUDIO_NEXT: u32 = 258;
    pub const AUDIO_PREV: u32 = 259;
    pub const AUDIO_STOP: u32 = 260;
    /// LG's Play/Pause key.
    pub const AUDIO_PLAY: u32 = 261;
    pub const AC_SEARCH: u32 = 268;
    pub const AC_BACK: u32 = 270;
    pub const AUDIO_REWIND: u32 = 285;
    pub const AUDIO_FAST_FORWARD: u32 = 286;

    /// CH+ and CH− as measured on LG's SDL.
    pub const LG_CHANNEL_UP: u32 = 300;
    pub const LG_CHANNEL_DOWN: u32 = 301;
    pub const LG_PLAY: u32 = 450;
    pub const LG_FAST_FORWARD: u32 = 451;
    pub const LG_REWIND: u32 = 452;
    /// CH+ and CH− as LG's `SDL_webOS.h` names them.
    pub const WEBOS_CHANNEL_UP: u32 = 480;
    pub const WEBOS_CHANNEL_DOWN: u32 = 481;
    /// Needs `SDL_WEBOS_ACCESS_POLICY_KEYS_BACK` before `SDL_Init`.
    pub const WEBOS_BACK: u32 = 482;
    /// The Magic Remote's pointer showed or hid: not keys.
    pub const WEBOS_CURSOR_SHOW: u32 = 484;
    pub const WEBOS_CURSOR_HIDE: u32 = 485;
    pub const WEBOS_RED: u32 = 486;
    pub const WEBOS_GREEN: u32 = 487;
    pub const WEBOS_YELLOW: u32 = 488;
    pub const WEBOS_BLUE: u32 = 489;
    pub const WEBOS_GUIDE: u32 = 495;
    pub const WEBOS_EXIT: u32 = 505;
}

/// SDL's keycodes for keys that type a character.
pub mod keycode {
    pub const BACKSPACE: i32 = 8;
    pub const RETURN: i32 = 13;
    pub const ESCAPE: i32 = 27;
    pub const DIGIT_0: i32 = 48;
    pub const DIGIT_9: i32 = 57;
}

/// Whether the scancode is the Magic Remote's pointer showing or hiding.
pub fn is_cursor_notice(scancode: u32) -> bool {
    matches!(scancode, scancode::WEBOS_CURSOR_SHOW | scancode::WEBOS_CURSOR_HIDE)
}

/// What the key with this scancode and keycode means.
pub fn map_key(code: u32, sym: i32) -> Key {
    use scancode as s;
    if (keycode::DIGIT_0..=keycode::DIGIT_9).contains(&sym) {
        return Key::Digit((sym - keycode::DIGIT_0) as u8);
    }
    match sym {
        keycode::RETURN => return Key::Ok,
        // The simulator's way too: Escape and Backspace go back.
        keycode::ESCAPE | keycode::BACKSPACE => return Key::Back,
        _ => {}
    }
    match code {
        s::UP => Key::Up,
        s::DOWN => Key::Down,
        s::LEFT => Key::Left,
        s::RIGHT => Key::Right,
        s::RETURN | s::KP_ENTER => Key::Ok,
        s::WEBOS_BACK | s::ESCAPE | s::BACKSPACE | s::AC_BACK => Key::Back,
        s::WEBOS_EXIT => Key::Exit,
        s::MENU | s::APPLICATION => Key::Menu,
        s::AUDIO_PLAY => Key::PlayPause,
        s::LG_PLAY => Key::Play,
        s::PAUSE => Key::Pause,
        s::STOP | s::AUDIO_STOP => Key::Stop,
        s::LG_FAST_FORWARD | s::AUDIO_FAST_FORWARD => Key::FastForward,
        s::LG_REWIND | s::AUDIO_REWIND => Key::Rewind,
        s::AUDIO_NEXT => Key::Next,
        s::AUDIO_PREV => Key::Previous,
        s::LG_CHANNEL_UP | s::WEBOS_CHANNEL_UP | s::PAGE_UP => Key::ChannelUp,
        s::LG_CHANNEL_DOWN | s::WEBOS_CHANNEL_DOWN | s::PAGE_DOWN => Key::ChannelDown,
        s::WEBOS_RED => Key::Red,
        s::WEBOS_GREEN => Key::Green,
        s::WEBOS_YELLOW => Key::Yellow,
        s::WEBOS_BLUE => Key::Blue,
        s::WEBOS_GUIDE => Key::Guide,
        s::AC_SEARCH => Key::Search,
        c if (s::KP_1..s::KP_0).contains(&c) => Key::Digit((c - s::KP_1 + 1) as u8),
        s::KP_0 => Key::Digit(0),
        _ => Key::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SDL's keycode for a key without a character: its scancode with bit 30 set.
    fn sym(code: u32) -> i32 {
        (code | 1 << 30) as i32
    }

    #[test]
    fn every_remote_key_means_what_the_tv_app_says() {
        let table: &[(u32, i32, Key)] = &[
            // The D-pad and OK, measured.
            (82, sym(82), Key::Up),
            (81, sym(81), Key::Down),
            (80, sym(80), Key::Left),
            (79, sym(79), Key::Right),
            (40, 13, Key::Ok),
            (88, sym(88), Key::Ok),
            // LG's own keys: scancodes with keycode 0.
            (482, 0, Key::Back),
            (505, 0, Key::Exit),
            (450, 0, Key::Play),
            (72, sym(72), Key::Pause),
            (261, sym(261), Key::PlayPause),
            (451, 0, Key::FastForward),
            (452, 0, Key::Rewind),
            (120, sym(120), Key::Stop),
            (260, sym(260), Key::Stop),
            (300, 0, Key::ChannelUp),
            (301, 0, Key::ChannelDown),
            (480, 0, Key::ChannelUp),
            (481, 0, Key::ChannelDown),
            (486, 0, Key::Red),
            (487, 0, Key::Green),
            (488, 0, Key::Yellow),
            (489, 0, Key::Blue),
            (495, 0, Key::Guide),
            (258, sym(258), Key::Next),
            (259, sym(259), Key::Previous),
            (285, sym(285), Key::Rewind),
            (286, sym(286), Key::FastForward),
            // Digits by keycode: scancode 33 is the digit 4, never CH+.
            (30, 49, Key::Digit(1)),
            (33, 52, Key::Digit(4)),
            (34, 53, Key::Digit(5)),
            (39, 48, Key::Digit(0)),
            (89, sym(89), Key::Digit(1)),
            (97, sym(97), Key::Digit(9)),
            (98, sym(98), Key::Digit(0)),
            // A keyboard on the TV, as in the simulator.
            (41, 27, Key::Back),
            (42, 8, Key::Back),
            (75, sym(75), Key::ChannelUp),
            (78, sym(78), Key::ChannelDown),
            (118, sym(118), Key::Menu),
            (101, sym(101), Key::Menu),
            // Not the app's: SDL's `P` (19) is no Play, `END` (77) no Select, Home stays the TV's.
            (19, 112, Key::Unknown),
            (77, sym(77), Key::Unknown),
            (384, 0, Key::Unknown),
            (269, sym(269), Key::Unknown),
        ];
        for &(code, keysym, want) in table {
            assert_eq!(map_key(code, keysym), want, "scancode {code}, keycode {keysym}");
        }
    }

    #[test]
    fn the_pointers_notices_are_not_keys() {
        assert!(is_cursor_notice(484) && is_cursor_notice(485));
        assert!(!is_cursor_notice(482));
    }
}
