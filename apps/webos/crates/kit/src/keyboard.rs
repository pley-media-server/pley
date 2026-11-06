//! Typing: the text field and the on-screen keyboard.
//!
//! The text field (`TvInput.kt`, `TvSearchField.kt`) is 44 dp tall, Surface2 with 4 dp
//! corners and a 1 dp Control edge, 2 dp of AccentBright when focused or typing, never scaled; its
//! text in `Tv.Body` 16 dp in, the placeholder in Muted, an optional 20 dp icon (Muted, Fg when
//! focused) 12 dp before it, and while typing an AccentBright caret after the text, blinking every
//! 500 ms. Focus lands on it without typing; OK starts typing.
//!
//! Android types with the system's keyboard; the LG app draws its own, in the app's design
//! language, so every TV types the same way (text typed on a keyboard plugged into the TV, or by the
//! TV's own keyboard when the platform shows it, goes into the field as well).
//!
//! **The keyboard's layout.** A panel along the bottom of the screen, full width, Surface2 with a
//! 1 dp Surface4 line along its top (the side panels' level), 265 dp tall (top at y 275): 14 dp of
//! room, five rows of 40 dp keys 6 apart, then 27 dp (the safe area). Keys are 52 dp wide, 6
//! apart, ten to a row, the rows centred (x 193 to 767):
//!
//! ```text
//!   1  2  3  4  5  6  7  8  9  0
//!   q  w  e  r  t  y  u  i  o  p        symbols:  @ # $ % & * - + ( )
//!   a  s  d  f  g  h  j  k  l  '                  ! ? ' " : ; / \ _ =
//!   z  x  c  v  b  n  m  -  .  :                  , . < > [ ] { } ~ ^
//!   [Shift][&123][     Space     ][⌫][ Clear ][ Done ]
//! ```
//!
//! The bottom row's keys span 1, 1, 4, 1, 1.5 and 1.5 key widths. Shift capitalises the next
//! letter; "&123" turns to the symbols ("abc" back). Keys look like the PIN pad's: Surface3 with
//! 6 dp corners, the character in `Tv.Title` (words in `Tv.Label`), Accent with OnAccent and 1.08
//! when focused (150 ms in, 120 out), Surface4 while pressed. Focus starts on "q" and stays in the
//! keyboard while it shows; Up from the top row, Back and Done close it (Done also submits); the
//! remote's digit keys type digits.

use pley_base::{Icon, Key, Point, Rect, Size, TextOpts};
use pley_i18n::t;
use pley_ui::theme::{self, focus, radius};
use pley_ui::{Dir, Edge, FocusOpts, Group, Id, Response, Ui};

use crate::look::{
    focus_progress, free_line, inner_stroke, is_inert, mix, press_progress, register, text_centred, text_mid,
};

/// Text being typed, the caret at its end.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextEdit {
    pub text: String,
    /// The most characters it takes; 0 for no limit.
    pub max_chars: usize,
}

impl TextEdit {
    /// Text starting as `text`.
    pub fn new(text: impl Into<String>) -> TextEdit {
        TextEdit { text: text.into(), max_chars: 0 }
    }

    /// Taking at most `max` characters.
    pub fn limit(self, max: usize) -> TextEdit {
        TextEdit { max_chars: max, ..self }
    }

    /// Adds `s` at the end, leaving out control characters and what goes past the limit; whether
    /// anything was added.
    pub fn insert(&mut self, s: &str) -> bool {
        let room = match self.max_chars {
            0 => usize::MAX,
            max => max.saturating_sub(self.text.chars().count()),
        };
        let before = self.text.len();
        self.text.extend(s.chars().filter(|c| !c.is_control()).take(room));
        self.text.len() != before
    }

    /// Takes the last character away; whether there was one.
    pub fn backspace(&mut self) -> bool {
        self.text.pop().is_some()
    }

    /// Empties the text; whether there was any.
    pub fn clear(&mut self) -> bool {
        let had = !self.text.is_empty();
        self.text.clear();
        had
    }
}

/// A text field.
#[derive(Clone, Copy, Debug)]
pub struct TextField<'a> {
    text: &'a str,
    placeholder: &'a str,
    icon: Option<Icon>,
    width: f32,
    typing: bool,
}

/// A text field's height.
pub const FIELD_HEIGHT: f32 = 44.0;

impl<'a> TextField<'a> {
    /// A field `width` wide showing `text`.
    pub fn new(text: &'a str, width: f32) -> TextField<'a> {
        TextField { text, placeholder: "", icon: None, width, typing: false }
    }

    /// What it says while empty.
    pub fn placeholder(self, placeholder: &'a str) -> TextField<'a> {
        TextField { placeholder, ..self }
    }

    /// A 20 dp icon before the text (Search's magnifier).
    pub fn icon(self, icon: Icon) -> TextField<'a> {
        TextField { icon: Some(icon), ..self }
    }

    /// Typing into it now: the caret shows and the edge is AccentBright.
    pub fn typing(self, typing: bool) -> TextField<'a> {
        TextField { typing, ..self }
    }

    /// Draws the field from `at`, registered as `id`. OK on it ([`Response::activated`]) is the
    /// screen's cue to start typing.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::new(at.x, at.y, self.width, FIELD_HEIGHT);
        ui.name(id, || {
            let text = if self.text.is_empty() { self.placeholder } else { self.text };
            crate::look::spoken(&[text, t!("common.keyboard.field")])
        });
        let r = register(ui, id, rect, FocusOpts::default());
        let lit = r.focused || self.typing;
        ui.fill(rect, theme::SURFACE2, radius::FIELD);
        let (edge, w) =
            if lit { (focus::FIELD_EDGE_COLOR, focus::FIELD_EDGE) } else { (theme::CONTROL, focus::REST_EDGE) };
        inner_stroke(ui, rect, edge, radius::FIELD, w);
        let mut x = rect.x + 16.0;
        if let Some(icon) = self.icon {
            let tint = if lit { theme::FG } else { theme::MUTED };
            ui.icon(Rect::new(x, rect.y + 12.0, theme::ICON_SIZE, theme::ICON_SIZE), icon, tint);
            x += theme::ICON_SIZE + 12.0;
        }
        let area = Rect::new(x, rect.y, (rect.right() - 16.0 - x).max(0.0), rect.h);
        if self.text.is_empty() {
            let layout = ui.layout(self.placeholder, &theme::BODY, &TextOpts::line(area.w.max(1.0)));
            text_mid(ui, area.x, area.y, area.h, layout, theme::MUTED);
        }
        let layout = ui.layout(self.text, &theme::BODY, &free_line());
        let caret_w = 2.0;
        // A text longer than the field shows its end, where the caret is.
        let shift = (layout.size.w + caret_w + 1.0 - area.w).max(0.0);
        let text_w = layout.size.w;
        ui.clip(area, |ui| {
            if !self.text.is_empty() {
                text_mid(ui, area.x - shift, area.y, area.h, layout, theme::FG);
            }
            if self.typing {
                let now = ui.time();
                let half = (now / 0.5).floor();
                if half as i64 % 2 == 0 {
                    let cx = area.x - shift + text_w + 1.0;
                    ui.fill(Rect::new(cx, area.y + 12.0, caret_w, 20.0), theme::ACCENT_BRIGHT, 0.0);
                }
                ui.repaint_after((half + 1.0) * 0.5 - now);
            }
        });
        r
    }
}

/// Which keys the keyboard shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KeyboardPage {
    #[default]
    Letters,
    Symbols,
}

/// What the keyboard keeps between frames (owned by the screen).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyboardState {
    pub page: KeyboardPage,
    /// The next letter is a capital.
    pub shift: bool,
}

/// What the keyboard did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyboardResponse {
    /// The text changed.
    pub changed: bool,
    /// Done (or the action key) was chosen: submit, and stop typing.
    pub done: bool,
    /// Back, or Up from the top row: stop typing.
    pub closed: bool,
}

const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const LETTERS: [[&str; 10]; 3] = [
    ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
    ["a", "s", "d", "f", "g", "h", "j", "k", "l", "'"],
    ["z", "x", "c", "v", "b", "n", "m", "-", ".", ":"],
];
const SYMBOLS: [[&str; 10]; 3] = [
    ["@", "#", "$", "%", "&", "*", "-", "+", "(", ")"],
    ["!", "?", "'", "\"", ":", ";", "/", "\\", "_", "="],
    [",", ".", "<", ">", "[", "]", "{", "}", "~", "^"],
];

/// What a key of the bottom row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Special {
    Shift,
    Page,
    Space,
    Backspace,
    Clear,
    Done,
}

/// The bottom row: each key and how many key widths it spans.
const BOTTOM: [(Special, f32); 6] = [
    (Special::Shift, 1.0),
    (Special::Page, 1.0),
    (Special::Space, 4.0),
    (Special::Backspace, 1.0),
    (Special::Clear, 1.5),
    (Special::Done, 1.5),
];

/// The on-screen keyboard.
#[derive(Clone, Copy, Debug)]
pub struct Keyboard<'a> {
    action: &'a str,
}

impl Default for Keyboard<'_> {
    fn default() -> Self {
        Keyboard::new()
    }
}

/// How a key looks: the PIN pad's keys (`TvKeypad.kt`).
const KEY_LOOK: focus::ControlLook = focus::ControlLook {
    fill: theme::SURFACE3,
    content: theme::FG,
    edge: None,
    focus_fill: theme::ACCENT,
    focus_content: theme::ON_ACCENT,
    pressed_fill: theme::SURFACE4,
    focus_scale: 1.08,
};

impl<'a> Keyboard<'a> {
    /// A key's size.
    pub const KEY: Size = Size::new(52.0, 40.0);
    /// Between keys, both ways.
    pub const GAP: f32 = 6.0;
    /// The panel's height.
    pub const HEIGHT: f32 = 14.0 + 5.0 * 40.0 + 4.0 * 6.0 + 27.0;

    /// A keyboard whose action key says "Done".
    pub fn new() -> Keyboard<'a> {
        Keyboard { action: t!("common.keyboard.done") }
    }

    /// The action key's label ("Search", "Connect").
    pub fn action_label(self, label: &'a str) -> Keyboard<'a> {
        Keyboard { action: label }
    }

    /// The panel's rectangle.
    pub fn rect() -> Rect {
        Rect::new(0.0, theme::CANVAS_H - Keyboard::HEIGHT, theme::CANVAS_W, Keyboard::HEIGHT)
    }

    /// The left edge of the keys.
    pub fn keys_x() -> f32 {
        (theme::CANVAS_W - (10.0 * Keyboard::KEY.w + 9.0 * Keyboard::GAP)) / 2.0
    }

    /// The rectangle of the key at `row` (0 to 4) and `col`, in key widths from the left (the
    /// bottom row's keys start where their spans put them), `span` key widths wide.
    pub fn key_rect(row: usize, col: f32, span: f32) -> Rect {
        let top = Keyboard::rect().y + 14.0;
        let unit = Keyboard::KEY.w + Keyboard::GAP;
        Rect::new(
            Keyboard::keys_x() + col * unit,
            top + row as f32 * (Keyboard::KEY.h + Keyboard::GAP),
            span * unit - Keyboard::GAP,
            Keyboard::KEY.h,
        )
    }

    /// Draws the keyboard over the bottom of the screen and types into `edit`; it keeps focus
    /// while drawn.
    pub fn show(&self, ui: &mut Ui, id: Id, state: &mut KeyboardState, edit: &mut TextEdit) -> KeyboardResponse {
        let mut out = KeyboardResponse::default();
        let panel = Keyboard::rect();
        ui.fill(panel, theme::SURFACE2, 0.0);
        ui.fill(Rect::new(0.0, panel.y, panel.w, 1.0), theme::SURFACE4, 0.0);
        let live = !is_inert();
        if live {
            // The pointer's clicks on the panel between keys go nowhere.
            ui.clickable(id.with("kit.panel"), panel);
            let typed = ui.typed().to_owned();
            if !typed.is_empty() {
                out.changed |= edit.insert(&typed);
            }
            let digits: Vec<u8> = ui
                .keys()
                .iter()
                .filter_map(|k| match k.key {
                    Key::Digit(d) if k.pressed => Some(d),
                    _ => None,
                })
                .collect();
            for d in digits {
                out.changed |= edit.insert(&d.to_string());
            }
        }
        let group = Group::free()
            .edge(Dir::Up, Edge::Stop)
            .edge(Dir::Down, Edge::Stop)
            .edge(Dir::Left, Edge::Stop)
            .edge(Dir::Right, Edge::Stop);
        let group = if live { group.trap() } else { group };
        let rows = match state.page {
            KeyboardPage::Letters => &LETTERS,
            KeyboardPage::Symbols => &SYMBOLS,
        };
        ui.group(id, group, |ui| {
            for row in 0..4 {
                for col in 0..10 {
                    let label = if row == 0 { DIGITS[col] } else { rows[row - 1][col] };
                    let shown = if state.shift && state.page == KeyboardPage::Letters {
                        label.to_uppercase()
                    } else {
                        label.to_owned()
                    };
                    let kid = ui.id(("key", row, col));
                    if row == 1 && col == 0 {
                        ui.initial_focus(kid);
                    }
                    let rect = Keyboard::key_rect(row, col as f32, 1.0);
                    if key(ui, kid, rect, KeyFace::Text(&shown, true)).activated {
                        out.changed |= edit.insert(&shown);
                        if state.page == KeyboardPage::Letters && label.chars().all(char::is_alphabetic) {
                            state.shift = false;
                        }
                    }
                }
            }
            let mut col = 0.0;
            for (special, span) in BOTTOM {
                let kid = ui.id(("special", special as u8));
                let rect = Keyboard::key_rect(4, col, span);
                col += span;
                let face = match special {
                    Special::Shift => KeyFace::Text(t!("common.keyboard.shift"), false),
                    // Symbols, not words: the same in every language.
                    Special::Page if state.page == KeyboardPage::Letters => KeyFace::Text("&123", false),
                    Special::Page => KeyFace::Text("abc", false),
                    Special::Space => KeyFace::Text(t!("common.keyboard.space"), false),
                    Special::Backspace => KeyFace::Icon(Icon::Delete),
                    Special::Clear => KeyFace::Text(t!("common.keyboard.clear"), false),
                    Special::Done => KeyFace::Text(self.action, false),
                };
                let lit = special == Special::Shift && state.shift;
                let r = key(ui, kid, rect, face);
                if lit && !r.focused {
                    // Shift on: an Accent line under its label.
                    ui.fill(Rect::new(rect.x + 14.0, rect.bottom() - 6.0, rect.w - 28.0, 2.0), theme::ACCENT, 1.0);
                }
                if !r.activated {
                    continue;
                }
                match special {
                    Special::Shift => state.shift = !state.shift,
                    Special::Page => {
                        state.page = match state.page {
                            KeyboardPage::Letters => KeyboardPage::Symbols,
                            KeyboardPage::Symbols => KeyboardPage::Letters,
                        };
                        state.shift = false;
                    }
                    Special::Space => out.changed |= edit.insert(" "),
                    Special::Backspace => out.changed |= edit.backspace(),
                    Special::Clear => out.changed |= edit.clear(),
                    Special::Done => out.done = true,
                }
            }
        });
        if live && (ui.edge_hit(id) == Some(Dir::Up) || ui.take_key(Key::Back)) {
            out.closed = true;
        }
        if out.changed || out.done || out.closed {
            // The field drawn before the keyboard shows the text as it was: draw again.
            ui.request_repaint();
        }
        out
    }
}

/// What a key shows.
#[derive(Clone, Copy, Debug)]
enum KeyFace<'a> {
    /// A character (`true`, in `Tv.Title`) or a word (in `Tv.Label`).
    Text(&'a str, bool),
    Icon(Icon),
}

/// One key: registered, coloured and scaled as the PIN pad's keys are.
fn key(ui: &mut Ui, id: Id, rect: Rect, face: KeyFace) -> Response {
    ui.name(id, || match face {
        KeyFace::Text(" ", _) => t!("common.keyboard.space").to_string(),
        KeyFace::Text(t, _) => t.to_string(),
        KeyFace::Icon(Icon::Delete) => t!("common.keyboard.delete").to_string(),
        KeyFace::Icon(_) => String::new(),
    });
    let r = register(ui, id, rect, FocusOpts::default());
    let p = focus_progress(ui, id, r.focused);
    let q = press_progress(ui, id, r.pressed);
    let fill = mix(mix(KEY_LOOK.fill, KEY_LOOK.focus_fill, p), KEY_LOOK.pressed_fill, q);
    let content = mix(mix(KEY_LOOK.content, KEY_LOOK.focus_content, p), theme::FG, q);
    let scale = 1.0 + (KEY_LOOK.focus_scale - 1.0) * p;
    ui.transform(Point::ZERO, scale, rect.center(), |ui| {
        ui.fill(rect, fill, radius::CONTROL);
        match face {
            KeyFace::Text(t, big) => {
                let style = if big { theme::TITLE } else { theme::LABEL };
                text_centred(ui, rect, t, &style, content);
            }
            KeyFace::Icon(icon) => ui.icon(rect.place(Size::new(22.0, 22.0), 0.5, 0.5), icon, content),
        }
    });
    r
}
