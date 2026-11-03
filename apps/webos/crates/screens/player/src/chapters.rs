//! The chapters as a row of cards over the scrubber (the Samsung app's `ChapterStrip`, as YouTube's
//! TV app has them): Up on the scrubber while a seek waits opens them on the chapter the seek was
//! going to, and the Chapters button on the chapter playing. Left and Right go through the
//! chapters, the scrubber showing the focused one's start; OK goes there; Down or Back closes them.
//! The video plays on meanwhile. A card is the seek preview's size, in its place: the chapter's
//! picture (its own, else the server's preview just after its start), letterboxed, then its name
//! and when it starts.

use std::collections::HashMap;

use pley_base::{Color, ImageRequest, Point, Rect, TextOpts};
use pley_player::controller::ChapterMark;
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, FocusOpts, Id, ImageOpts, Motion, Response, Ui};

use crate::controls::{self, PREVIEW_H, PREVIEW_TOP, PREVIEW_W, RING_OFFSET, RING_STROKE};
use crate::previews::Tile;
use crate::text;

/// Between two cards.
pub const GAP: f32 = 14.0;
/// Cards this far before and after the focused one ask for their pictures.
pub const ASK_BEFORE: usize = 2;
/// See [`ASK_BEFORE`].
pub const ASK_AFTER: usize = 5;
/// Cards this far before and after the focused one keep their pictures; farther, they let them
/// go (a film's chapters can span every preview sheet, each tens of MB drawn).
const KEPT_BEFORE: usize = 4;
/// See [`KEPT_BEFORE`].
const KEPT_AFTER: usize = 8;
/// Under the picture: the name, then the start.
const TEXT_GAP: f32 = 6.0;

/// The cards while they are open.
#[derive(Clone, Debug, Default)]
pub struct Strip {
    /// The chapter focused.
    pub index: usize,
    /// Opened with the Chapters button: closed, focus goes back to it, else to the scrubber.
    pub from_button: bool,
    /// The previews cut for the cards near focus, by chapter.
    tiles: HashMap<usize, Tile>,
}

impl Strip {
    /// The cards on `at`'s chapter, or the one before it when `at` falls between chapters.
    pub fn open(chapters: &[ChapterMark], at: i64, from_button: bool) -> Strip {
        let index = text::chapter_at(chapters, at)
            .unwrap_or_else(|| chapters.iter().rposition(|c| c.start_ms <= at).unwrap_or(0));
        Strip { index, from_button, tiles: HashMap::new() }
    }

    /// Left (−1) or Right (+1) among `count` chapters, stopping at the ends; whether focus moved.
    pub fn step(&mut self, by: i64, count: usize) -> bool {
        let last = count.saturating_sub(1) as i64;
        let next = (self.index as i64 + by).clamp(0, last) as usize;
        let moved = next != self.index;
        self.index = next;
        let (lo, hi) = (self.index.saturating_sub(KEPT_BEFORE), self.index + KEPT_AFTER);
        self.tiles.retain(|i, _| (lo..=hi).contains(i));
        moved
    }

    /// The cards that ask for their pictures, of `count`: the focused one first, then those near it.
    pub fn asking(&self, count: usize) -> Vec<usize> {
        if count == 0 {
            return Vec::new();
        }
        let index = self.index.min(count - 1);
        let (lo, hi) = (index.saturating_sub(ASK_BEFORE), (index + ASK_AFTER).min(count - 1));
        std::iter::once(index).chain((lo..=hi).filter(|i| *i != index)).collect()
    }

    /// Whether card `i` has its preview.
    pub fn has_tile(&self, i: usize) -> bool {
        self.tiles.contains_key(&i)
    }

    /// Card `i`'s preview came.
    pub fn keep_tile(&mut self, i: usize, tile: Tile) {
        self.tiles.insert(i, tile);
    }
}

/// A card's height: the picture, then the name and the start.
pub fn card_height() -> f32 {
    PREVIEW_H + TEXT_GAP + theme::LABEL.line_height + theme::CAPTION.line_height
}

/// Draws the row and registers it as one focusable taking every direction; its response (its OK
/// goes to the focused chapter).
pub fn show(ui: &mut Ui, id: Id, strip: &Strip, chapters: &[ChapterMark]) -> Response {
    // 6 dp of room above and below the cards for the focus ring.
    let area = Rect::new(0.0, PREVIEW_TOP - 6.0, theme::CANVAS_W, card_height() + 12.0);
    let r = ui.focusable_with(id, area, FocusOpts::default().takes(Dirs::ALL));
    let step = PREVIEW_W + GAP;
    let room = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    let content = chapters.len() as f32 * step - GAP;
    // The row slides so the focused card stays in view, the first at the gutter.
    let target = if content <= room { 0.0 } else { -(strip.index as f32 * step).min(content - room) };
    let shift = ui.animate(id.with("x"), target, Motion::FOCUS);
    ui.clip(area, |ui| {
        for (i, c) in chapters.iter().enumerate() {
            let x = theme::GUTTER + i as f32 * step + shift;
            if x + PREVIEW_W < 0.0 || x > theme::CANVAS_W {
                continue;
            }
            let well = Rect::new(x, PREVIEW_TOP, PREVIEW_W, PREVIEW_H);
            card(ui, well, c, i, i == strip.index, strip.tiles.get(&i));
        }
    });
    r
}

/// A chapter's card: its picture on Surface2, its name and its start; focused, the accent ring and
/// the name in Fg.
fn card(ui: &mut Ui, well: Rect, c: &ChapterMark, i: usize, focused: bool, tile: Option<&Tile>) {
    ui.fill(well, theme::SURFACE2, radius::TILE);
    match (c.image_url.as_deref(), tile) {
        (Some(url), _) => {
            ui.fill(well, Color::BLACK, radius::TILE);
            let req = ImageRequest::new(url, ui.image_px(PREVIEW_W));
            ui.image(well, &req, &ImageOpts::contain().rounded(radius::TILE));
        }
        (None, Some(t)) => controls::letterboxed(ui, well, t),
        (None, None) => {}
    }
    if focused {
        let reach = RING_OFFSET + RING_STROKE / 2.0;
        ui.stroke(well.expand(reach), theme::ACCENT_BRIGHT, radius::TILE + reach, RING_STROKE);
    }
    let y = well.bottom() + TEXT_GAP;
    let name = text::chapter_row(c, i);
    let color = if focused { theme::FG } else { theme::MUTED };
    ui.text(Point::new(well.x, y), &name, &theme::LABEL, &TextOpts::line(well.w), color);
    let start = text::time(c.start_ms);
    let at = Point::new(well.x, y + theme::LABEL.line_height);
    ui.text(at, &start, &theme::CAPTION, &TextOpts::line(well.w), theme::SUBTLE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapters(n: usize) -> Vec<ChapterMark> {
        (0..n)
            .map(|i| ChapterMark {
                title: None,
                start_ms: i as i64 * 60_000,
                end_ms: (i as i64 + 1) * 60_000,
                image_url: None,
            })
            .collect()
    }

    #[test]
    fn opens_on_the_chapter_of_where_it_was_going_and_stops_at_the_ends() {
        let c = chapters(5);
        let mut s = Strip::open(&c, 150_000, false);
        assert_eq!(s.index, 2);
        assert!(s.step(1, c.len()));
        assert!(s.step(1, c.len()));
        assert!(!s.step(1, c.len()), "past the last");
        assert_eq!(s.index, 4);
        let mut s = Strip::open(&c, 0, true);
        assert!(!s.step(-1, c.len()), "before the first");
        assert_eq!(s.index, 0);
    }

    #[test]
    fn the_cards_near_focus_ask_for_pictures_the_focused_first() {
        let mut s = Strip::open(&chapters(20), 600_000, false);
        assert_eq!(s.asking(20), [10, 8, 9, 11, 12, 13, 14, 15]);
        assert_eq!(Strip::open(&chapters(3), 0, false).asking(3), [0, 1, 2]);
        assert!(s.asking(0).is_empty());
        s.step(-1, 20);
        assert_eq!(s.asking(20)[0], 9);
    }
}
