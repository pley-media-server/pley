//! A track's line (`TvTrackLine` in the Android TV app's music kit; the Samsung app's
//! `Kit/TrackList.cs`): an album's tracks, the music queue, and a drive's folders and videos.

use pley_base::{Color, Icon, Point, Rect, TextOpts};
use pley_ui::theme::{self, radius};
use pley_ui::{FocusOpts, Id, Response, Ui};

use crate::look::{self, register};

/// A track's line in a list: the number, or for the track playing an audio-lines (playing) or play
/// glyph in Accent, or an icon on a tile (a drive's folder or video); the title over its subtitle;
/// a bar of how far through it is; the length (or a file's size) at the end. Focused it is an
/// Accent pill without scale; the track it was opened from is Accent 14 %, the one playing Fg 6 %
/// with its title in AccentBright.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrackLine<'a> {
    pub number: &'a str,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub duration: &'a str,
    pub current: bool,
    pub playing: bool,
    pub highlighted: bool,
    /// In the number's place, on a 36 dp tile.
    pub icon: Option<Icon>,
    /// How far through it is, 0 to 1: a bar under the words when more than 0.
    pub progress: f32,
}

/// A line's least height, its padding, and its number column (`TvTrackLine`).
pub const LINE_MIN: f32 = 44.0;
/// A line with an icon: at least this tall, to hold its tile (`TrackLineView.IconLine`).
pub const ICON_LINE_MIN: f32 = 56.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 6.0;
const NUMBER_W: f32 = 28.0;
const NUMBER_GAP: f32 = 16.0;
const GLYPH: f32 = 18.0;
/// The icon's tile, the icon on it, and the gap after it.
const TILE: f32 = 36.0;
const TILE_ICON: f32 = 20.0;
const TILE_GAP: f32 = 14.0;
/// The progress bar, and its room under the words.
const BAR_W: f32 = 160.0;
const BAR_H: f32 = 3.0;
const BAR_GAP: f32 = 6.0;
/// What a focused line's tile darkens to.
const TILE_FOCUSED: Color = theme::alpha(Color::BLACK, 0.18);

impl TrackLine<'_> {
    /// The words and the bar, stacked.
    fn block(&self) -> f32 {
        let subtitle = if self.subtitle.is_some() { theme::CAPTION.line_height } else { 0.0 };
        let bar = if self.progress > 0.0 { BAR_GAP + BAR_H } else { 0.0 };
        theme::BODY.line_height + subtitle + bar
    }

    /// The line's height: its words and bar with the padding, at least 44 (56 with an icon).
    pub fn height(&self) -> f32 {
        let least = if self.icon.is_some() { ICON_LINE_MIN } else { LINE_MIN };
        (self.block() + 2.0 * PAD_Y).max(least)
    }

    /// What Audio Guidance reads for it: the title, the subtitle and the length (or size).
    pub fn spoken(&self) -> String {
        look::spoken(&[self.title, self.subtitle.unwrap_or(""), self.duration])
    }

    /// Draws the line in `rect`, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, rect: Rect) -> Response {
        ui.name(id, || self.spoken());
        let r = register(ui, id, rect, FocusOpts::default());
        let p = look::focus_progress(ui, id, r.focused);
        let rest = if self.highlighted {
            theme::TRACK_HIGHLIGHT
        } else if self.current {
            theme::TRACK_CURRENT
        } else {
            Color::TRANSPARENT
        };
        let fill = look::mix(rest, theme::ACCENT, p);
        if fill.a > 0 {
            ui.fill(rect, fill, radius::FIELD);
        }
        let on = |c: Color| look::mix(c, theme::ON_ACCENT, p);

        let lead = Rect::new(rect.x + PAD_X, rect.y, NUMBER_W, rect.h);
        let x = if let Some(icon) = self.icon {
            let tile = Rect::new(rect.x + PAD_X, rect.y + (rect.h - TILE) / 2.0, TILE, TILE);
            ui.fill(tile, look::mix(theme::SURFACE2, TILE_FOCUSED, p), radius::FIELD);
            let inset = (TILE - TILE_ICON) / 2.0;
            ui.icon(Rect::new(tile.x + inset, tile.y + inset, TILE_ICON, TILE_ICON), icon, on(theme::MUTED));
            tile.right() + TILE_GAP
        } else {
            if self.current {
                let glyph = if self.playing { Icon::AudioLines } else { Icon::Play };
                let at = Rect::new(lead.right() - GLYPH, rect.y + (rect.h - GLYPH) / 2.0, GLYPH, GLYPH);
                ui.icon(at, glyph, on(theme::ACCENT));
            } else if !self.number.is_empty() {
                let layout = ui.layout(self.number, &theme::CAPTION, &TextOpts::line(NUMBER_W));
                let at = Point::new(lead.right() - layout.size.w, rect.y + (rect.h - layout.size.h) / 2.0);
                ui.draw_text(at, layout, on(theme::SUBTLE));
            }
            lead.right() + NUMBER_GAP
        };

        let right = rect.right() - PAD_X;
        let duration =
            (!self.duration.is_empty()).then(|| ui.layout(self.duration, &theme::CAPTION, &TextOpts::default()));
        let duration_w = duration.as_ref().map_or(0.0, |d| d.size.w);
        if let Some(d) = duration {
            let at = Point::new(right - d.size.w, rect.y + (rect.h - d.size.h) / 2.0);
            ui.draw_text(at, d, on(theme::MUTED));
        }

        let width = (right - duration_w - PAD_X - x).max(1.0);
        let mut y = rect.y + (rect.h - self.block()) / 2.0;
        let title = if self.current { theme::ACCENT_BRIGHT } else { theme::FG };
        ui.text(Point::new(x, y), self.title, &theme::BODY, &TextOpts::line(width), on(title));
        y += theme::BODY.line_height;
        if let Some(s) = self.subtitle {
            ui.text(Point::new(x, y), s, &theme::CAPTION, &TextOpts::line(width), on(theme::MUTED));
            y += theme::CAPTION.line_height;
        }
        if self.progress > 0.0 {
            let bar = Rect::new(x, y + BAR_GAP, BAR_W.min(width), BAR_H);
            ui.fill(bar, theme::PROGRESS_TRACK_ON_ART, BAR_H / 2.0);
            let done = Rect::new(bar.x, bar.y, bar.w * self.progress.min(1.0), BAR_H);
            ui.fill(done, on(theme::ACCENT), BAR_H / 2.0);
        }
        r
    }
}
