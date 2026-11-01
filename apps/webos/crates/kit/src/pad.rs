//! The number pads of signing in (`TvKeypad.kt`, `TvAddressPad.kt`, `TvProfilesScreen.kt`):
//! three columns of 64 dp keys 14 apart, Surface3 with 6 dp corners and their digits in
//! `Tv.PinDigit`; focused Accent and 1.08 (150 ms in, 120 out); a key pressed, or typed with the
//! remote's digit keys, flashes Surface4 for 100 ms. Focus starts on 5. A disabled pad is drawn at
//! 40 % and can't take focus. The PIN pad's keys are 1–9, ⌫, 0 and OK; the address pad's 1–9, ".",
//! 0 and ":". [`PinDots`] shows how many digits of a PIN are in.

use pley_base::{Icon, Key, Point, Rect, Size};
use pley_ui::theme::{self, radius};
use pley_ui::{Group, Id, Ui};

use crate::look::{FLASH, focus_progress, is_inert, mix, register, text_centred};

/// A key of a pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadKey {
    Digit(u8),
    /// "." or ":" on the address pad.
    Symbol(char),
    Delete,
    Ok,
}

/// Which keys a pad has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadLayout {
    /// 1–9, ⌫, 0, OK.
    Pin,
    /// 1–9, ".", 0, ":".
    Address,
}

/// What a pad keeps between frames: the key flashing (owned by the screen).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadState {
    flash: Option<usize>,
    flashes: u32,
}

/// A number pad.
#[derive(Clone, Copy, Debug)]
pub struct Keypad {
    layout: PadLayout,
    enabled: bool,
}

impl Keypad {
    /// A key's side.
    pub const KEY: f32 = 64.0;
    /// Between keys.
    pub const GAP: f32 = 14.0;

    /// The PIN pad.
    pub fn pin() -> Keypad {
        Keypad { layout: PadLayout::Pin, enabled: true }
    }

    /// The address pad.
    pub fn address() -> Keypad {
        Keypad { layout: PadLayout::Address, enabled: true }
    }

    /// Drawn at 40 % and out of focus's way when `false` (a locked-out PIN).
    pub fn enabled(self, enabled: bool) -> Keypad {
        Keypad { enabled, ..self }
    }

    /// The pad's size: 220 × 298.
    pub fn size() -> Size {
        Size::new(3.0 * Keypad::KEY + 2.0 * Keypad::GAP, 4.0 * Keypad::KEY + 3.0 * Keypad::GAP)
    }

    /// The pad's keys, row by row.
    pub fn keys(&self) -> [PadKey; 12] {
        let d = PadKey::Digit;
        let (left, right) = match self.layout {
            PadLayout::Pin => (PadKey::Delete, PadKey::Ok),
            PadLayout::Address => (PadKey::Symbol('.'), PadKey::Symbol(':')),
        };
        [d(1), d(2), d(3), d(4), d(5), d(6), d(7), d(8), d(9), left, d(0), right]
    }

    /// Draws the pad with its top-left corner at `at`; returns the key pressed with OK, a click or
    /// the remote's digits this frame.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point, state: &mut PadState) -> Option<PadKey> {
        let keys = self.keys();
        let mut pressed = None;
        if self.enabled && !is_inert() {
            let typed = ui.keys().iter().find_map(|k| match k.key {
                Key::Digit(d) if k.pressed && !k.repeat => Some(d),
                _ => None,
            });
            if let Some(d) = typed {
                pressed = Some(PadKey::Digit(d));
                state.flash = keys.iter().position(|k| *k == PadKey::Digit(d));
                state.flashes = state.flashes.wrapping_add(1);
                ui.request_repaint();
            }
        }
        let alpha = if self.enabled { 1.0 } else { theme::DISABLED_ALPHA };
        ui.opacity(alpha, |ui| {
            ui.group(id, Group::grid(3), |ui| {
                for (i, k) in keys.iter().enumerate() {
                    let x = at.x + (i % 3) as f32 * (Keypad::KEY + Keypad::GAP);
                    let y = at.y + (i / 3) as f32 * (Keypad::KEY + Keypad::GAP);
                    let rect = Rect::new(x, y, Keypad::KEY, Keypad::KEY);
                    let kid = ui.id(i);
                    let (focused, activated) = if self.enabled {
                        if i == 4 {
                            ui.initial_focus(kid);
                        }
                        let r = register(ui, kid, rect, pley_ui::FocusOpts::default());
                        (r.focused, r.activated)
                    } else {
                        (false, false)
                    };
                    if activated {
                        pressed = Some(*k);
                        state.flash = Some(i);
                        state.flashes = state.flashes.wrapping_add(1);
                    }
                    let p = focus_progress(ui, kid, focused);
                    let flash = if state.flash == Some(i) {
                        ui.animate_from(kid.with(("kit.flash", state.flashes)), 1.0, 0.0, FLASH)
                    } else {
                        0.0
                    };
                    let fill = mix(mix(theme::SURFACE3, theme::ACCENT, p), theme::SURFACE4, flash);
                    let content = mix(mix(theme::FG, theme::ON_ACCENT, p), theme::FG, flash);
                    ui.transform(Point::ZERO, 1.0 + 0.08 * p, rect.center(), |ui| {
                        ui.fill(rect, fill, radius::CONTROL);
                        let glyph = rect.place(Size::new(28.0, 28.0), 0.5, 0.5);
                        match k {
                            PadKey::Digit(d) => text_centred(ui, rect, &d.to_string(), &theme::PIN_DIGIT, content),
                            PadKey::Symbol(c) => text_centred(ui, rect, &c.to_string(), &theme::PIN_DIGIT, content),
                            PadKey::Delete => ui.icon(glyph, Icon::Delete, content),
                            PadKey::Ok => ui.icon(glyph, Icon::Check, content),
                        }
                    });
                }
            });
        });
        pressed
    }
}

/// The dots of a PIN (`TvPinPad`): at least four, up to eight (one per digit typed), each 14 dp
/// across and 16 apart: Fg when typed, else a 2 dp Control ring. A refused PIN shakes them: x
/// offsets −6, 6, −6, 6, −6, 0 dp, 40 ms each (none with Reduce motion).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinDots {
    /// Digits typed.
    pub typed: usize,
}

impl PinDots {
    /// A dot's size.
    pub const DOT: f32 = 14.0;
    /// Between dots.
    pub const GAP: f32 = 16.0;
    const SHAKE: [f32; 6] = [-6.0, 6.0, -6.0, 6.0, -6.0, 0.0];
    const STEP: f64 = 0.040;

    /// The x offset of a shake that began `elapsed` seconds ago.
    pub fn shake_offset(elapsed: f64) -> f32 {
        if elapsed < 0.0 {
            return 0.0;
        }
        let steps = elapsed / PinDots::STEP;
        let i = steps.floor() as usize;
        if i >= PinDots::SHAKE.len() {
            return 0.0;
        }
        let from = if i == 0 { 0.0 } else { PinDots::SHAKE[i - 1] };
        let t = (steps - i as f64) as f32;
        from + (PinDots::SHAKE[i] - from) * t
    }

    /// Draws the dots centred on `center_x` from `y`; `shake_since` is when a refusal shook them.
    pub fn show(&self, ui: &mut Ui, center_x: f32, y: f32, shake_since: Option<f64>) {
        let n = self.typed.clamp(4, 8);
        let width = n as f32 * PinDots::DOT + (n - 1) as f32 * PinDots::GAP;
        let mut dx = 0.0;
        if let Some(since) = shake_since.filter(|_| !ui.reduced_motion()) {
            let elapsed = ui.time() - since;
            dx = PinDots::shake_offset(elapsed);
            if elapsed < PinDots::STEP * PinDots::SHAKE.len() as f64 {
                ui.request_repaint();
            }
        }
        let x0 = center_x - width / 2.0 + dx;
        for i in 0..n {
            let r = Rect::new(x0 + i as f32 * (PinDots::DOT + PinDots::GAP), y, PinDots::DOT, PinDots::DOT);
            if i < self.typed {
                ui.fill(r, theme::FG, PinDots::DOT / 2.0);
            } else {
                ui.stroke(r.expand(-1.0), theme::CONTROL, PinDots::DOT / 2.0 - 1.0, 2.0);
            }
        }
    }
}
