//! Animation through the `Ui`: tweens that run and settle, the frames they ask for, and the
//! snaps of held keys, Reduce motion and Low-tier TVs; pictures fading in.

use std::cell::{Cell, RefCell};

use pley_base::{Cmd, ImageRequest, Key, Priority, Rect};

use super::row;
use crate::testing::{Harness, Step};
use crate::theme::{BG, FG};
use crate::{Easing, Group, Id, ImageOpts, Motion, Tier, Ui};

#[test]
fn an_animation_runs_to_its_target_then_stops_asking_for_frames() {
    let target = Cell::new(0.0);
    let values = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let v = ui.animate(Id::new("v"), target.get(), Motion::FOCUS);
        values.borrow_mut().push(v);
    };
    let mut h = Harness::new();
    // A new value starts where it is headed, and asks for nothing.
    assert_eq!(h.idle(&mut screen).output.wake_at, None);
    target.set(1.0);
    let f = h.idle(&mut screen);
    assert_eq!(f.output.wake_at, Some(0.0));
    let frames = h.run(&[Step::Wait(1.0)], &mut screen);
    // 60 frames a second for 150 ms, the last one landing.
    assert_eq!(frames.len(), 10);
    assert_eq!(frames.last().unwrap().output.wake_at, None);
    let v = values.borrow();
    assert_eq!(v[1], 0.0);
    assert!(v[2..].windows(2).all(|w| w[1] >= w[0]) && v[2] > 0.0);
    assert_eq!(*v.last().unwrap(), 1.0);
    // v[1 + k] is the value k/60 s in.
    assert!((v[1 + 5] - Easing::FAST_OUT_SLOW_IN.ease((5.0 / 60.0) / 0.15)).abs() < 1e-4);
}

#[test]
fn a_held_key_snaps_animations() {
    let target = Cell::new(0.0);
    let value = Cell::new(-1.0);
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 210.0, 0..10, Group::row());
        value.set(ui.animate(Id::new("v"), target.get(), Motion::FOCUS));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.run(&[Step::Down(Key::Right), Step::Wait(0.1), Step::Repeat(Key::Right)], &mut screen);
    target.set(1.0);
    let f = h.idle(&mut screen);
    assert_eq!(value.get(), 1.0);
    // Snapped values ask for no frames: the next comes for the held key's next move.
    assert!((f.output.wake_at.unwrap() - (f.time + 0.045)).abs() < 1e-9);
}

/// Holds Right along a row of 40 for a second, the TV repeating from 400 ms, with a scroll 100 a
/// tile following focus; every frame's scroll.
fn held_scroll(reduced: bool) -> Vec<f32> {
    let values = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 210.0, 0..40, Group::row());
        let at = (0..40).find(|&i| ui.focused() == Some(super::t("a", i))).unwrap_or(0);
        let v = ui.scroll(Id::new("scroll"), at as f32 * 100.0, Motion::OVERLAY);
        values.borrow_mut().push(v);
    };
    let mut h = Harness::new();
    h.state.set_reduced_motion(reduced);
    h.idle(&mut screen);
    h.step(Step::Down(Key::Right), &mut screen);
    h.wait(0.4, &mut screen);
    for _ in 0..12 {
        h.step(Step::Repeat(Key::Right), &mut screen);
        h.wait(0.05, &mut screen);
    }
    values.take()
}

#[test]
fn a_held_key_glides_a_scroll_from_move_to_move() {
    let v = held_scroll(false);
    assert!(v.windows(2).all(|w| w[1] >= w[0]));
    // Glides overlap: once the hold is under way, every frame shows the scroll on its way, never
    // resting on a tile.
    let gliding = &v[v.len() / 2..];
    assert!(gliding.iter().all(|x| x % 100.0 > 0.5), "{gliding:?}");
    assert!(*v.last().unwrap() > 1500.0);
    // Reduce motion: each move lands at once.
    let v = held_scroll(true);
    assert!(v.iter().all(|x| x % 100.0 == 0.0), "{v:?}");
    assert!(*v.last().unwrap() > 1500.0);
}

#[test]
fn reduce_motion_snaps_all_but_reveals() {
    let values = RefCell::new((0.0, 0.0));
    let target = Cell::new(0.0);
    let mut screen = |ui: &mut Ui| {
        let a = ui.animate(Id::new("scale"), target.get(), Motion::FOCUS);
        let b = ui.animate(Id::new("label"), target.get(), Motion::REVEAL);
        *values.borrow_mut() = (a, b);
    };
    let mut h = Harness::new();
    h.state.set_reduced_motion(true);
    h.idle(&mut screen);
    target.set(1.0);
    h.idle(&mut screen);
    assert_eq!(*values.borrow(), (1.0, 0.0));
}

#[test]
fn low_tier_tvs_animate_only_what_takes_focus() {
    let values = RefCell::new((0.0, 0.0));
    let focused = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        let (on, off) = if focused.get() { (1.0, 0.0) } else { (0.0, 1.0) };
        let a = ui.animate(Id::new("gaining"), on, if focused.get() { Motion::FOCUS } else { Motion::UNFOCUS });
        let b = ui.animate(Id::new("losing"), off, if focused.get() { Motion::UNFOCUS } else { Motion::FOCUS });
        *values.borrow_mut() = (a, b);
    };
    let mut h = Harness::new();
    h.state.set_tier(Tier::Low);
    h.idle(&mut screen);
    focused.set(true);
    h.idle(&mut screen);
    // The one gaining focus starts animating; the one losing it snaps.
    assert_eq!(*values.borrow(), (0.0, 0.0));
}

#[test]
fn colours_and_rectangles_animate_too() {
    let on = Cell::new(false);
    let seen = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let c = ui.animate_color(Id::new("c"), if on.get() { FG } else { BG }, Motion::PRESS);
        let r = ui.animate_rect(
            Id::new("r"),
            if on.get() { Rect::new(100.0, 0.0, 200.0, 20.0) } else { Rect::new(0.0, 0.0, 100.0, 20.0) },
            Motion::PRESS,
        );
        seen.borrow_mut().push((ui.time(), c, r));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    on.set(true);
    h.idle(&mut screen);
    h.run(&[Step::Wait(0.051)], &mut screen);
    let (time, c, r) = *seen.borrow().last().unwrap();
    assert!((time - 3.0 / 60.0).abs() < 1e-9);
    // Half way through PRESS's 100 ms, linear.
    let mid = BG.lerp(FG, 0.5);
    let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
    assert!(near(c.r, mid.r) && near(c.g, mid.g) && near(c.b, mid.b) && c.a == 255, "{c:?}");
    assert!((r.x - 50.0).abs() < 1e-3 && (r.w - 150.0).abs() < 1e-3, "{r:?}");
    h.run(&[Step::Wait(1.0)], &mut screen);
    assert_eq!(seen.borrow().last().unwrap().1, FG);
}

#[test]
fn an_explicit_start_animates_something_appearing() {
    let value = Cell::new(-1.0);
    let mut screen = |ui: &mut Ui| value.set(ui.animate_from(Id::new("panel"), 0.0, 1.0, Motion::OVERLAY));
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert_eq!((value.get(), f.output.wake_at), (0.0, Some(0.0)));
    h.run(&[Step::Wait(1.0)], &mut screen);
    assert_eq!(value.get(), 1.0);
}

#[test]
fn pictures_not_in_memory_fade_in_and_others_show_at_once() {
    let shown = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let req = ImageRequest::new("poster", 216);
        let s = ui.image(Rect::new(96.0, 210.0, 108.0, 162.0), &req, &ImageOpts::cover().rounded(4.0));
        shown.borrow_mut().push(s.opacity);
    };
    let mut h = Harness::new();
    // Loading: nothing drawn, nothing asked.
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.is_empty());
    assert_eq!((shown.borrow()[0], f.output.wake_at), (0.0, None));
    // Ready just now: it fades in over 150 ms, a frame at a time.
    h.images.ready("poster", 240, 360, 0.0);
    h.idle(&mut screen);
    let frames = h.run(&[Step::Wait(1.0)], &mut screen);
    assert_eq!(frames.len(), 10);
    let last = frames.last().unwrap();
    assert!(matches!(last.list.cmds[0], Cmd::Image { opacity, radius, .. } if opacity == 1.0 && radius == 4.0));
    assert!(shown.borrow().windows(2).skip(1).all(|w| w[1] > w[0]));
    assert_eq!(h.images.requests()[0].0.width_px, 216);
    assert_eq!(h.images.requests()[0].1, Priority::Visible);
    // Already in memory: at once.
    h.images.ready("poster", 240, 360, -10.0);
    let f = h.idle(&mut screen);
    assert_eq!((*shown.borrow().last().unwrap(), f.output.wake_at), (1.0, None));
    // Low-tier TVs never fade.
    h.state.set_tier(Tier::Low);
    h.images.ready("poster", 240, 360, h.time());
    assert_eq!(h.idle(&mut screen).list.cmds.len(), 1);
    assert_eq!(*shown.borrow().last().unwrap(), 1.0);
}

#[test]
fn frames_can_be_asked_for_later() {
    let mut screen = |ui: &mut Ui| {
        ui.repaint_after(2.0);
        ui.repaint_after(5.0);
    };
    let mut h = Harness::new();
    assert_eq!(h.idle(&mut screen).output.wake_at, Some(2.0));
    let mut screen = |ui: &mut Ui| ui.request_repaint();
    assert_eq!(h.idle(&mut screen).output.wake_at, Some(0.0));
}
