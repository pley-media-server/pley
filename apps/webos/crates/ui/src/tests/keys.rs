//! The remote's keys: the repeat gate, holding and resting, OK and its long press, the menu key,
//! and the keys a screen reads itself.

use std::cell::{Cell, RefCell};

use pley_base::{Key, Rect};

use super::{row, t};
use crate::testing::{Harness, Step};
use crate::{FocusOpts, Group, Id, Ui};

#[test]
fn a_held_direction_moves_every_45_ms_on_the_apps_clock() {
    let holding = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 210.0, 0..40, Group::row());
        holding.borrow_mut().push(ui.holding());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = h.step(Step::Down(Key::Right), &mut screen);
    assert_eq!(f.focused, Some(t("a", 1)));
    // Down, the next frame comes as the hold begins, 180 ms on.
    assert!((f.output.wake_at.unwrap() - 0.18).abs() < 1e-9);
    assert_eq!(h.step(Step::Wait(0.17), &mut screen).focused, Some(t("a", 1)));
    assert_eq!(holding.borrow().last(), Some(&false));
    // LG repeats every 50 ms from 400 ms; they only say the key is still down.
    h.wait(0.4 - h.time(), &mut screen);
    for _ in 0..12 {
        h.step(Step::Repeat(Key::Right), &mut screen);
        h.wait(0.05, &mut screen);
    }
    // From 180 ms to 1 s, a move every 45 ms: 19.
    let at = h.idle(&mut screen).focused;
    assert!((19..=21).any(|i| at == Some(t("a", i))), "{at:?}");
    assert_eq!(holding.borrow().last(), Some(&true));
    h.step(Step::Up(Key::Right), &mut screen);
    assert_eq!(holding.borrow().last(), Some(&false));
    assert_eq!(h.step(Step::Wait(0.5), &mut screen).focused, at);
}

#[test]
fn a_hold_whose_key_up_was_lost_ends_500_ms_after_the_last_key() {
    let holding = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 210.0, 0..30, Group::row());
        holding.borrow_mut().push((ui.time(), ui.holding()));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.step(Step::Down(Key::Right), &mut screen);
    h.run(&[Step::Wait(0.1)], &mut screen);
    // A repeat starts the hold at once.
    let f = h.step(Step::Repeat(Key::Right), &mut screen);
    assert_eq!(f.focused, Some(t("a", 2)));
    assert!((f.output.wake_at.unwrap() - 0.145).abs() < 1e-9);
    holding.borrow_mut().clear();
    let frames = h.run(&[Step::Wait(2.0)], &mut screen);
    // Frames come for the moves until the hold ends, and none after.
    let seen = holding.borrow();
    let (time, held) = *seen.last().unwrap();
    assert!((0.6..0.62).contains(&time) && !held, "{time}");
    assert!(seen[..seen.len() - 1].iter().all(|&(_, held)| held));
    assert_eq!(frames.last().unwrap().output.wake_at, None);
    // Ten more moves, 45 ms apart, after the one at 100 ms.
    let at = frames.last().unwrap().focused;
    assert!((11..=13).any(|i| at == Some(t("a", i))), "{at:?}");
}

#[test]
fn focus_rests_400_ms_after_the_last_move_and_a_frame_comes_then() {
    let rested = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 210.0, 0..5, Group::row());
        rested.borrow_mut().push((ui.time(), ui.rested()));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    assert_eq!(rested.borrow().last(), Some(&(0.0, true)));
    h.run(&[Step::Wait(1.0)], &mut screen);
    let f = h.step(Step::Press(Key::Right), &mut screen);
    assert_eq!(rested.borrow().last(), Some(&(1.0, false)));
    assert!((f.output.wake_at.unwrap() - 1.4).abs() < 1e-9);
    let frames = h.run(&[Step::Wait(2.0)], &mut screen);
    assert_eq!(frames.len(), 1);
    let (time, ok) = *rested.borrow().last().unwrap();
    assert!((time - 1.4).abs() < 1e-9 && ok);
    assert_eq!(frames[0].output.wake_at, None);
}

/// A row of plain tiles whose first tile also takes long presses; activations, long presses,
/// menus and presses are logged as "<what> <key>".
fn logged(log: &RefCell<Vec<String>>) -> impl FnMut(&mut Ui) + '_ {
    move |ui: &mut Ui| {
        let id = ui.id("a");
        ui.group(id, Group::row(), |ui| {
            for i in 0..3usize {
                let tid = ui.id(i);
                let opts = if i == 0 { FocusOpts::default().long_press() } else { FocusOpts::default() };
                let r = ui.focusable_with(tid, super::tile(i, 210.0), opts);
                let mut log = log.borrow_mut();
                if r.activated {
                    log.push(format!("activated {i}"));
                }
                if r.long_pressed {
                    log.push(format!("long {i}"));
                }
                if r.menu {
                    log.push(format!("menu {i}"));
                }
                if r.pressed {
                    log.push(format!("pressed {i}"));
                }
            }
        });
    }
}

#[test]
fn ok_presses_on_the_way_down_and_selects_on_release() {
    let log = RefCell::new(Vec::new());
    let mut screen = logged(&log);
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.step(Step::Press(Key::Right), &mut screen);
    h.step(Step::Down(Key::Ok), &mut screen);
    assert_eq!(*log.borrow(), ["pressed 1"]);
    h.step(Step::Up(Key::Ok), &mut screen);
    assert_eq!(*log.borrow(), ["pressed 1", "activated 1"]);
}

#[test]
fn ok_held_half_a_second_is_a_long_press_and_its_release_does_nothing() {
    let log = RefCell::new(Vec::new());
    let mut screen = logged(&log);
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = h.step(Step::Down(Key::Ok), &mut screen);
    assert!((f.output.wake_at.unwrap() - 0.5).abs() < 1e-9);
    h.run(&[Step::Wait(1.0), Step::Up(Key::Ok)], &mut screen);
    assert_eq!(*log.borrow(), ["pressed 0", "long 0"]);
    // Where nothing takes a long press, a held OK still selects on release.
    log.borrow_mut().clear();
    h.run(&[Step::Press(Key::Right), Step::Down(Key::Ok), Step::Wait(1.0), Step::Up(Key::Ok)], &mut screen);
    assert_eq!(*log.borrow(), ["pressed 1", "activated 1"]);
}

#[test]
fn ok_released_after_focus_moved_selects_nothing() {
    let log = RefCell::new(Vec::new());
    let mut screen = logged(&log);
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(&[Step::Down(Key::Ok), Step::Press(Key::Right), Step::Up(Key::Ok)], &mut screen);
    assert!(!log.borrow().iter().any(|l| l.starts_with("activated")));
}

#[test]
fn the_menu_key_acts_once_on_its_release() {
    let log = RefCell::new(Vec::new());
    let mut screen = logged(&log);
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(&[Step::Down(Key::Menu), Step::Repeat(Key::Menu)], &mut screen);
    assert!(log.borrow().is_empty());
    h.step(Step::Up(Key::Menu), &mut screen);
    assert_eq!(*log.borrow(), ["menu 0"]);
}

#[test]
fn other_keys_and_typed_text_reach_the_screen() {
    let seen = RefCell::new(Vec::new());
    let back = Cell::new(0);
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("field");
        ui.focusable(id, Rect::new(96.0, 100.0, 300.0, 40.0));
        // A panel claims Back first; the shell then doesn't see it.
        if ui.take_key(Key::Back) {
            back.set(back.get() + 1);
        }
        assert!(!ui.key_down(Key::Back));
        for k in ui.keys().iter().filter(|k| k.pressed) {
            seen.borrow_mut().push(format!("{:?}", k.key));
        }
        if !ui.typed().is_empty() {
            seen.borrow_mut().push(ui.typed().to_owned());
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(
        &[Step::Press(Key::Back), Step::Press(Key::Digit(5)), Step::Text("ab".into()), Step::Press(Key::PlayPause)],
        &mut screen,
    );
    assert_eq!(back.get(), 1);
    assert_eq!(*seen.borrow(), ["Digit(5)", "ab", "PlayPause"]);
    assert_eq!(h.state.focused(), Some(Id::new("field")));
}
