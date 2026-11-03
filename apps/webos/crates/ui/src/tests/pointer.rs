//! The Magic Remote: hover, clicks, OK sent both ways, scrims, the wheel, and where the pointer
//! finds things through clips and transforms.

use std::cell::{Cell, RefCell};

use pley_base::{Key, Point, Rect};

use super::{row, t, tile};
use crate::testing::{Harness, Step};
use crate::{FocusOpts, Group, Id, Ui};

/// The centre of tile `slot` of a row at `y`.
fn over(slot: usize, y: f32) -> Point {
    tile(slot, y).center()
}

#[test]
fn hovering_focuses_by_pointer_and_content_moving_under_it_does_not() {
    let shift = Cell::new(0.0f32);
    let hovered = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        ui.translate(Point::new(-shift.get(), 0.0), |ui| row(ui, "a", 210.0, 0..6, Group::row()));
        hovered.set(ui.hovered());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = h.step(Step::Point(over(2, 210.0)), &mut screen);
    assert_eq!(f.focused, Some(t("a", 2)));
    assert!(h.state.pointer_mode() && f.output.pointer_visible);
    // The row slides a tile under the still pointer: what is under it shows as hovered, but
    // focus stays.
    shift.set(122.0);
    h.idle(&mut screen);
    assert_eq!(h.idle(&mut screen).focused, Some(t("a", 2)));
    assert_eq!(hovered.get(), Some(t("a", 3)));
    // A key takes over: focus moves by keys, and the pointer hides.
    let f = h.step(Step::Press(Key::Right), &mut screen);
    assert_eq!(f.focused, Some(t("a", 3)));
    assert!(!h.state.pointer_mode() && !f.output.pointer_visible);
}

#[test]
fn a_click_focuses_and_selects() {
    let log = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("a");
        ui.group(id, Group::row(), |ui| {
            for i in 0..4usize {
                let tid = ui.id(i);
                let r = ui.focusable(tid, tile(i, 210.0));
                if r.activated {
                    log.borrow_mut().push((i, r.clicked, r.pointer));
                }
            }
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = h.run(&[Step::Point(over(3, 210.0)), Step::Click], &mut screen).pop().unwrap();
    assert_eq!(f.focused, Some(t("a", 3)));
    // Where the pointer is, in the tile's own coordinates.
    assert_eq!(*log.borrow(), [(3, true, Some(over(3, 210.0)))]);
}

#[test]
fn one_ok_press_sent_as_key_and_click_selects_once_in_either_order() {
    let count = Cell::new(0);
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("a");
        if ui.focusable(id, tile(0, 210.0)).activated {
            count.set(count.get() + 1);
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.step(Step::Point(over(0, 210.0)), &mut screen);
    // The key, then its click.
    h.run(&[Step::Press(Key::Ok), Step::Wait(0.06), Step::Click], &mut screen);
    assert_eq!(count.get(), 1);
    // The click, then its key.
    h.run(&[Step::Wait(1.0), Step::Click, Step::Wait(0.06), Step::Press(Key::Ok)], &mut screen);
    assert_eq!(count.get(), 2);
    // Presses that come one way only are each a press.
    h.run(&[Step::Wait(1.0), Step::Click, Step::Wait(0.15), Step::Click], &mut screen);
    h.run(&[Step::Wait(1.0), Step::Press(Key::Ok), Step::Wait(0.15), Step::Press(Key::Ok)], &mut screen);
    assert_eq!(count.get(), 6);
}

#[test]
fn a_click_while_ok_is_held_is_dropped() {
    let count = Cell::new(0);
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("a");
        if ui.focusable(id, tile(0, 210.0)).activated {
            count.set(count.get() + 1);
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(&[Step::Point(over(0, 210.0)), Step::Down(Key::Ok), Step::Click, Step::Up(Key::Ok)], &mut screen);
    assert_eq!(count.get(), 1);
}

#[test]
fn ok_while_the_pointer_shows_over_a_scrim_is_the_scrims_click() {
    let open = Cell::new(true);
    let selected = Cell::new(0);
    let mut screen = |ui: &mut Ui| {
        row(ui, "page", 210.0, 0..3, Group::row());
        if open.get() {
            let scrim = ui.id("scrim");
            if ui.clickable(scrim, Rect::new(0.0, 0.0, 960.0, 540.0)).activated {
                open.set(false);
            }
            let panel = ui.id("panel");
            ui.group(panel, Group::column().trap(), |ui| {
                let r = ui.id(0usize);
                if ui.focusable(r, Rect::new(640.0, 100.0, 300.0, 44.0)).activated {
                    selected.set(selected.get() + 1);
                }
            });
        }
    };
    let mut h = Harness::new();
    assert_eq!(h.idle(&mut screen).focused, Some(t("panel", 0)));
    h.run(&[Step::Point(Point::new(100.0, 100.0)), Step::Press(Key::Ok)], &mut screen);
    assert!(!open.get());
    assert_eq!(selected.get(), 0);
    // With the panel gone, focus is back on the page.
    assert_eq!(h.idle(&mut screen).focused, Some(t("page", 0)));
}

#[test]
fn ok_over_an_element_that_wants_the_clicks_place_clicks_there() {
    let clicks = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("scrubber");
        let r = ui.focusable_with(id, Rect::new(96.0, 480.0, 768.0, 20.0), FocusOpts::default().click_at());
        if r.activated {
            clicks.borrow_mut().push((r.clicked, r.pointer));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(&[Step::Point(Point::new(400.0, 490.0)), Step::Press(Key::Ok)], &mut screen);
    assert_eq!(*clicks.borrow(), [(true, Some(Point::new(400.0, 490.0)))]);
    // Once a key has hidden the pointer, OK is the key's again.
    h.run(&[Step::Press(Key::Left), Step::Press(Key::Ok)], &mut screen);
    assert_eq!(clicks.borrow().last(), Some(&(false, None)));
}

#[test]
fn moving_onto_something_outside_the_trap_tells_the_shell() {
    let outside = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        row(ui, "page", 210.0, 0..3, Group::row());
        let panel = ui.id("panel");
        ui.group(panel, Group::column().trap(), |ui| {
            let r = ui.id(0usize);
            ui.focusable(r, Rect::new(640.0, 100.0, 300.0, 44.0));
        });
        if let Some(id) = ui.pointer_outside() {
            outside.set(Some(id));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = h.step(Step::Point(over(1, 210.0)), &mut screen);
    assert_eq!(outside.get(), Some(t("page", 1)));
    assert_eq!(f.focused, Some(t("panel", 0)));
}

#[test]
fn the_pointer_leaves_focus_on_a_field_that_types() {
    let typing = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        let f = ui.id("field");
        ui.focusable(f, Rect::new(96.0, 100.0, 300.0, 40.0));
        let b = ui.id("button");
        ui.focusable(b, Rect::new(96.0, 200.0, 300.0, 40.0));
        ui.text_input(typing.get());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    typing.set(true);
    assert!(h.idle(&mut screen).output.text_input);
    assert_eq!(h.step(Step::Point(Point::new(200.0, 220.0)), &mut screen).focused, Some(Id::new("field")));
    typing.set(false);
    h.idle(&mut screen);
    assert_eq!(h.step(Step::Point(Point::new(210.0, 220.0)), &mut screen).focused, Some(Id::new("button")));
}

#[test]
fn the_wheel_steps_focus_a_notch_at_a_time_and_a_quick_run_is_a_held_key() {
    let holding = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("list");
        ui.group(id, Group::column(), |ui| {
            for i in 0..6usize {
                let r = ui.id(i);
                ui.focusable(r, Rect::new(96.0, 40.0 + 44.0 * i as f32, 400.0, 44.0));
            }
        });
        holding.borrow_mut().push(ui.holding());
    };
    let l = |i: usize| Some(t("list", i));
    let mut h = Harness::new();
    h.idle(&mut screen);
    // Toward the viewer is down.
    assert_eq!(h.step(Step::Wheel(-1.0), &mut screen).focused, l(1));
    assert_eq!(h.step(Step::Wheel(-1.0), &mut screen).focused, l(1));
    assert_eq!(holding.borrow().last(), Some(&false));
    h.run(&[Step::Wait(0.15)], &mut screen);
    assert_eq!(h.step(Step::Wheel(-1.0), &mut screen).focused, l(2));
    assert_eq!(holding.borrow().last(), Some(&true));
    h.run(&[Step::Wait(1.0)], &mut screen);
    assert_eq!(holding.borrow().last(), Some(&false));
    assert_eq!(h.step(Step::Wheel(1.0), &mut screen).focused, l(1));
}

#[test]
fn a_group_that_takes_the_wheel_gets_its_notches() {
    let notches = Cell::new(0);
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("details");
        ui.group(id, Group::column().wheel(), |ui| {
            let r = ui.id(0usize);
            ui.focusable(r, Rect::new(96.0, 300.0, 400.0, 200.0));
        });
        notches.set(notches.get() + ui.wheel(id));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(
        &[Step::Point(Point::new(200.0, 400.0)), Step::Wheel(-1.0), Step::Wheel(-1.0), Step::Wheel(1.0)],
        &mut screen,
    );
    assert_eq!(notches.get(), 1);
}

#[test]
fn the_pointer_finds_elements_where_they_show() {
    let mut screen = |ui: &mut Ui| {
        // Clipped away: not found where it would be.
        ui.clip(Rect::new(0.0, 0.0, 400.0, 540.0), |ui| {
            let id = ui.id("hidden");
            ui.focusable(id, Rect::new(500.0, 100.0, 100.0, 100.0));
            let id = ui.id("shown");
            ui.focusable(id, Rect::new(100.0, 100.0, 100.0, 100.0));
        });
        // Moved: found where it shows.
        ui.translate(Point::new(-200.0, 0.0), |ui| {
            let id = ui.id("moved");
            ui.focusable(id, Rect::new(700.0, 300.0, 100.0, 100.0));
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    assert_eq!(h.step(Step::Point(Point::new(150.0, 150.0)), &mut screen).focused, Some(Id::new("shown")));
    assert_eq!(h.step(Step::Point(Point::new(550.0, 150.0)), &mut screen).focused, Some(Id::new("shown")));
    assert_eq!(h.step(Step::Point(Point::new(551.0, 351.0)), &mut screen).focused, Some(Id::new("moved")));
}
