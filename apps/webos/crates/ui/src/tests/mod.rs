//! Behaviour tests through the public API, frame by frame with the headless harness.

mod animation;
mod drawing;
mod keys;
mod moves;
mod pointer;

use pley_base::{Key, Rect};

use crate::testing::{Harness, Step};
use crate::{Group, Id, Ui};

/// A poster tile in a row whose tiles start at `y`: 108 × 162 from the gutter, 14 apart.
fn tile(slot: usize, y: f32) -> Rect {
    Rect::new(96.0 + slot as f32 * 122.0, y, 108.0, 162.0)
}

/// A row group named `name` whose tiles are keyed by `keys`, in slots from the gutter.
fn row(ui: &mut Ui, name: &str, y: f32, keys: impl IntoIterator<Item = usize>, group: Group) {
    let id = ui.id(name);
    ui.group(id, group, |ui| {
        for (slot, key) in keys.into_iter().enumerate() {
            let t = ui.id(key);
            ui.focusable(t, tile(slot, y));
        }
    });
}

/// The id of tile `key` in the row `name` drawn at the top level.
fn t(name: &str, key: usize) -> Id {
    Id::new(name).with(key)
}

fn press(h: &mut Harness, key: Key, draw: &mut dyn FnMut(&mut Ui)) -> Option<Id> {
    h.step(Step::Press(key), draw).focused
}
