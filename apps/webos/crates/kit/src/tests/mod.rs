//! Headless tests of the kit: geometry, focus and what each control draws, through `pley-ui`'s
//! harness (a stub shaper whose characters advance half their size).

mod buttons;
mod containers;
mod feedback;
mod overlays;
mod tiles;
mod typing;

use pley_base::{Cmd, Color, DisplayList, Key, Point, Rect};
use pley_ui::testing::{Frame, Harness, Step};
use pley_ui::{Id, Ui};

/// Presses `key` and lets the motion settle; returns what has focus.
fn press(h: &mut Harness, key: Key, screen: &mut dyn FnMut(&mut Ui)) -> Option<Id> {
    h.step(Step::Press(key), screen);
    settle(h, screen).focused
}

/// Lets half a second pass (every animation ends); returns the last frame.
fn settle(h: &mut Harness, screen: &mut dyn FnMut(&mut Ui)) -> Frame {
    h.wait(0.5, screen);
    h.idle(screen)
}

/// Every filled rectangle of `color`.
fn fills_of(list: &DisplayList, color: Color) -> Vec<Rect> {
    list.cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Fill { rect, color: c, .. } if *c == color => Some(*rect),
            _ => None,
        })
        .collect()
}

/// Every outline: its rectangle, colour, radius and width.
fn strokes(list: &DisplayList) -> Vec<(Rect, Color, f32, f32)> {
    list.cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Stroke { rect, color, radius, width } => Some((*rect, *color, *radius, *width)),
            _ => None,
        })
        .collect()
}

/// Every scale pushed (`PushTransform` with a scale other than 1).
fn scales(list: &DisplayList) -> Vec<(f32, Point)> {
    list.cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::PushTransform { scale, pivot, .. } if *scale != 1.0 => Some((*scale, *pivot)),
            _ => None,
        })
        .collect()
}

/// Where the string `text` is drawn.
fn text_at(list: &DisplayList, text: &str) -> Option<Point> {
    list.cmds.iter().find_map(|c| match c {
        Cmd::Text { origin, layout, .. } if &*layout.text == text => Some(*origin),
        _ => None,
    })
}

/// The colour `text` is drawn in.
fn text_color(list: &DisplayList, text: &str) -> Option<Color> {
    list.cmds.iter().find_map(|c| match c {
        Cmd::Text { layout, color, .. } if &*layout.text == text => Some(*color),
        _ => None,
    })
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn close_rect(a: Rect, b: Rect) -> bool {
    close(a.x, b.x) && close(a.y, b.y) && close(a.w, b.w) && close(a.h, b.h)
}
