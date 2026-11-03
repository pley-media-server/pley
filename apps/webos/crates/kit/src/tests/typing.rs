//! The text field, the on-screen keyboard and the number pads.

use std::cell::{Cell, RefCell};

use pley_base::{Cmd, Key, Point, Rect};
use pley_ui::testing::{Harness, Step};
use pley_ui::theme;
use pley_ui::{Id, Ui};

use super::{fills_of, press, settle, strokes};
use crate::keyboard::{Keyboard, KeyboardResponse, KeyboardState, TextEdit, TextField};
use crate::pad::{Keypad, PadKey, PadState, PinDots};

#[test]
fn the_keyboard_types_letters_capitals_digits_and_edits() {
    let edit = RefCell::new(TextEdit::default());
    let kb = RefCell::new(KeyboardState::default());
    let last = Cell::new(KeyboardResponse::default());
    let mut screen = |ui: &mut Ui| {
        let text = edit.borrow().text.clone();
        let fid = ui.id("field");
        TextField::new(&text, 520.0).typing(true).show(ui, fid, Point::new(96.0, 27.0));
        let r = Keyboard::new().action_label("Search").show(
            ui,
            Id::new("kb"),
            &mut kb.borrow_mut(),
            &mut edit.borrow_mut(),
        );
        last.set(r);
    };
    let key = |row: usize, col: usize| Some(Id::new("kb").with(("key", row, col)));
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    // Focus starts on "q", in the keyboard's panel along the bottom.
    assert_eq!(f.focused, key(1, 0));
    assert!(fills_of(&f.list, theme::SURFACE2).contains(&Keyboard::rect()));
    assert_eq!(Keyboard::rect(), Rect::new(0.0, 275.0, 960.0, 265.0));
    assert_eq!(Keyboard::key_rect(1, 0.0, 1.0), Rect::new(193.0, 335.0, 52.0, 40.0));
    let type_ok = |h: &mut Harness, screen: &mut dyn FnMut(&mut Ui)| {
        h.step(Step::Press(Key::Ok), screen);
    };
    type_ok(&mut h, &mut screen);
    assert_eq!(press(&mut h, Key::Right, &mut screen), key(1, 1));
    type_ok(&mut h, &mut screen);
    assert_eq!(edit.borrow().text, "qw");
    // Down from "q" to Shift, then back up to "a": a capital.
    press(&mut h, Key::Left, &mut screen);
    press(&mut h, Key::Down, &mut screen);
    press(&mut h, Key::Down, &mut screen);
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(Id::new("kb").with(("special", 0u8))));
    type_ok(&mut h, &mut screen);
    assert!(kb.borrow().shift);
    press(&mut h, Key::Up, &mut screen);
    assert_eq!(press(&mut h, Key::Up, &mut screen), key(2, 0));
    type_ok(&mut h, &mut screen);
    assert_eq!(edit.borrow().text, "qwA");
    assert!(!kb.borrow().shift);
    // The remote's digits and text typed on a keyboard go in too.
    h.step(Step::Press(Key::Digit(7)), &mut screen);
    h.step(Step::Text("é".into()), &mut screen);
    assert_eq!(edit.borrow().text, "qwA7é");
    // Backspace, then Clear, along the bottom row.
    press(&mut h, Key::Down, &mut screen);
    press(&mut h, Key::Down, &mut screen);
    for _ in 0..3 {
        press(&mut h, Key::Right, &mut screen);
    }
    type_ok(&mut h, &mut screen);
    assert_eq!(edit.borrow().text, "qwA7");
    press(&mut h, Key::Right, &mut screen);
    type_ok(&mut h, &mut screen);
    assert_eq!(edit.borrow().text, "");
    assert!(last.get().changed);
    // Done says so; Up past the top row closes.
    press(&mut h, Key::Right, &mut screen);
    type_ok(&mut h, &mut screen);
    assert!(last.get().done);
    for _ in 0..4 {
        press(&mut h, Key::Up, &mut screen);
    }
    h.step(Step::Press(Key::Up), &mut screen);
    assert!(last.get().closed);
}

#[test]
fn a_text_field_lights_its_edge_when_focused_and_shows_a_caret_while_typing() {
    let typing = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("field");
        TextField::new("dune", 520.0).placeholder("Search movies, shows, people…").typing(typing.get()).show(
            ui,
            id,
            Point::new(96.0, 27.0),
        );
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = h.idle(&mut screen);
    let rect = Rect::new(96.0, 27.0, 520.0, 44.0);
    assert!(strokes(&f.list).iter().any(|s| s.0 == rect.expand(-1.0) && s.1 == theme::ACCENT_BRIGHT && s.3 == 2.0));
    assert!(fills_of(&f.list, theme::ACCENT_BRIGHT).is_empty());
    typing.set(true);
    let f = h.idle(&mut screen);
    // The caret after "dune" (4 characters of 7), 16 in.
    assert_eq!(fills_of(&f.list, theme::ACCENT_BRIGHT), vec![Rect::new(96.0 + 16.0 + 28.0 + 1.0, 39.0, 2.0, 20.0)]);
    // It blinks every half second.
    h.wait(0.5, &mut screen);
    let f = h.idle(&mut screen);
    assert!(fills_of(&f.list, theme::ACCENT_BRIGHT).is_empty());
}

#[test]
fn the_pin_pad_starts_on_5_and_takes_the_remotes_digits() {
    let pad = RefCell::new(PadState::default());
    let pressed = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        if let Some(k) = Keypad::pin().show(ui, Id::new("pad"), Point::new(370.0, 120.0), &mut pad.borrow_mut()) {
            pressed.set(Some(k));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("pad").with(4usize)));
    assert_eq!(Keypad::size(), pley_base::Size::new(220.0, 298.0));
    h.step(Step::Press(Key::Ok), &mut screen);
    assert_eq!(pressed.get(), Some(PadKey::Digit(5)));
    // Down twice and Right: the OK key.
    press(&mut h, Key::Down, &mut screen);
    press(&mut h, Key::Down, &mut screen);
    h.step(Step::Press(Key::Ok), &mut screen);
    assert_eq!(pressed.get(), Some(PadKey::Digit(0)));
    press(&mut h, Key::Right, &mut screen);
    h.step(Step::Press(Key::Ok), &mut screen);
    assert_eq!(pressed.get(), Some(PadKey::Ok));
    // A digit from the remote flashes its key Surface4.
    let f = h.step(Step::Press(Key::Digit(3)), &mut screen);
    assert_eq!(pressed.get(), Some(PadKey::Digit(3)));
    let key3 = Rect::new(370.0 + 2.0 * 78.0, 120.0, 64.0, 64.0);
    assert!(fills_of(&f.list, theme::SURFACE4).contains(&key3));
    h.wait(0.2, &mut screen);
    let f = h.idle(&mut screen);
    assert!(fills_of(&f.list, theme::SURFACE3).contains(&key3));
}

#[test]
fn a_disabled_pad_is_dim_and_takes_nothing() {
    let pressed = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        let mut st = PadState::default();
        if let Some(k) = Keypad::address().enabled(false).show(ui, Id::new("pad"), Point::new(370.0, 120.0), &mut st) {
            pressed.set(Some(k));
        }
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert_eq!(f.focused, None);
    assert!(f.list.cmds.contains(&Cmd::PushOpacity(theme::DISABLED_ALPHA)));
    assert!(f.list.says(":") && f.list.says("."));
    h.step(Step::Press(Key::Digit(1)), &mut screen);
    assert_eq!(pressed.get(), None);
}

#[test]
fn pin_dots_show_what_is_typed_and_shake_when_refused() {
    let shake = Cell::new(None);
    let mut screen = |ui: &mut Ui| PinDots { typed: 2 }.show(ui, 480.0, 200.0, shake.get());
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    // Four dots, 14 across and 16 apart, centred: two typed, two rings.
    let width = 4.0 * 14.0 + 3.0 * 16.0;
    let first = Rect::new(480.0 - width / 2.0, 200.0, 14.0, 14.0);
    assert_eq!(fills_of(&f.list, theme::FG), vec![first, first.translate(30.0, 0.0)]);
    assert_eq!(strokes(&f.list).len(), 2);
    shake.set(Some(h.time()));
    h.wait(0.02, &mut screen);
    let f = h.idle(&mut screen);
    let x = fills_of(&f.list, theme::FG)[0].x;
    assert!(x < first.x && x > first.x - 6.0, "{x}");
    assert_eq!(PinDots::shake_offset(0.04), -6.0);
    assert_eq!(PinDots::shake_offset(0.06), 0.0);
    assert_eq!(PinDots::shake_offset(1.0), 0.0);
}

#[test]
fn text_edits_keep_to_their_limit() {
    let mut e = TextEdit::new("ab").limit(4);
    assert!(e.insert("c\u{7}def"));
    assert_eq!(e.text, "abcd");
    assert!(!e.insert("x"));
    assert!(e.backspace() && e.clear() && !e.clear());
}
