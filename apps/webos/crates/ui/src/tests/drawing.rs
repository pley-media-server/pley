//! What the `Ui` records, the scopes around it, and ids.

use std::cell::{Cell, RefCell};

use pley_base::{Cmd, Color, Icon, Point, Rect, TextOpts};

use crate::testing::{Harness, StubShaper};
use crate::theme::{self, BG, FG};
use crate::{Group, Id, Ui};

#[test]
fn draws_in_order_with_balanced_scopes() {
    let size = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        ui.fill(Rect::new(0.0, 0.0, 960.0, 540.0), BG, 0.0);
        // Nothing to see: not recorded.
        ui.fill(Rect::new(0.0, 0.0, 10.0, 10.0), Color::TRANSPARENT, 0.0);
        ui.clip(Rect::new(96.0, 0.0, 864.0, 540.0), |ui| {
            ui.translate(Point::new(-50.0, 0.0), |ui| {
                size.set(Some(ui.text(
                    Point::new(96.0, 172.0),
                    "Continue watching",
                    &theme::SECTION,
                    &TextOpts::line(400.0),
                    FG,
                )));
            });
        });
        ui.opacity(0.4, |ui| ui.icon(Rect::new(10.0, 10.0, 20.0, 20.0), Icon::Play, FG));
        // A whole opacity or no move records no scope.
        ui.opacity(1.0, |ui| ui.translate(Point::ZERO, |ui| ui.punch(Rect::new(0.0, 0.0, 1.0, 1.0))));
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    let kinds: Vec<&str> = f
        .list
        .cmds
        .iter()
        .map(|c| match c {
            Cmd::Fill { .. } => "fill",
            Cmd::PushClip { .. } => "clip",
            Cmd::PopClip => "/clip",
            Cmd::PushTransform { .. } => "move",
            Cmd::PopTransform => "/move",
            Cmd::Text { .. } => "text",
            Cmd::PushOpacity(_) => "fade",
            Cmd::PopOpacity => "/fade",
            Cmd::Icon { .. } => "icon",
            Cmd::Punch { .. } => "punch",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["fill", "clip", "move", "text", "/move", "/clip", "fade", "icon", "/fade", "punch"]);
    assert!(f.list.says("Continue watching"));
    assert_eq!(f.list.clear, BG);
    // 17 characters of 9 dp (18 / 2) in the stub's measure.
    let s = size.get().unwrap();
    assert_eq!((s.w, s.h), (17.0 * 9.0, 24.0));
}

#[test]
fn measuring_draws_nothing() {
    let mut screen = |ui: &mut Ui| {
        let s = ui.measure("Play", &theme::LABEL, &TextOpts::default());
        assert_eq!(s.w, 4.0 * StubShaper::advance(&theme::LABEL));
        ui.set_clear(Color::TRANSPARENT);
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.is_empty());
    assert_eq!(f.list.clear, Color::TRANSPARENT);
}

#[test]
fn ids_follow_their_scopes() {
    let seen = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let row = ui.id("continue");
        ui.group(row, Group::row(), |ui| seen.borrow_mut().push(ui.id(42u64)));
        ui.push_id("hero", |ui| seen.borrow_mut().push(ui.id("play")));
        seen.borrow_mut().push(ui.id("play"));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let want = [Id::new("continue").with(42u64), Id::new("hero").with("play"), Id::new("play")];
    assert_eq!(seen.borrow()[..3], want);
}

#[test]
fn pictures_are_asked_for_at_device_pixels() {
    let px = Cell::new(0);
    let mut screen = |ui: &mut Ui| px.set(ui.image_px(108.0));
    let mut h = Harness::new();
    h.idle(&mut screen);
    assert_eq!(px.get(), 216);
    h.scale = 1.5;
    h.idle(&mut screen);
    assert_eq!(px.get(), 162);
}

#[test]
fn a_ramp_spans_the_rectangle_drawn() {
    let mut screen = |ui: &mut Ui| ui.ramp(Rect::new(0.0, 0.0, 960.0, 540.0), false, &theme::BACKDROP_LEFT_SCRIM);
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    let Cmd::LinearGradient { vertical, stops, .. } = &f.list.cmds[0] else { panic!("{:?}", f.list.cmds) };
    assert!(!vertical);
    assert_eq!(stops.len(), 5);
    assert_eq!(stops[2].at, 560.0 / 960.0);
}
