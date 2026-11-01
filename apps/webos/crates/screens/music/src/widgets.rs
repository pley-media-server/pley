//! The pieces the music pages share (`TvMusicKit.kt`): a track's line, the art wells, the faint
//! art behind a page, the rating's stars and the page-sized message.

use pley_base::{Color, Icon, ImageRequest, Point, Priority, Rect, TextOpts};
use pley_core::ApiError;
use pley_core::model::ItemList;
use pley_kit::{Button, first_art, is_inert, look, placeholder};
use pley_shell::Resource;
use pley_ui::theme::{self, radius};
use pley_ui::{FocusOpts, Id, ImageOpts, Response, Ui};

use crate::pages::star_lit;

/// Registers a focusable drawn by this crate, as the kit registers its own: nothing while drawn
/// inert, and one more frame after it is selected, since the page changes in answer after drawing.
pub fn register(ui: &mut Ui, id: Id, rect: Rect, opts: FocusOpts) -> Response {
    if is_inert() {
        return inert_response(id);
    }
    let r = ui.focusable_with(id, rect, opts);
    if r.activated || r.long_pressed || r.menu {
        ui.request_repaint();
    }
    r
}

fn inert_response(id: Id) -> Response {
    Response {
        id,
        focused: false,
        gained: false,
        activated: false,
        clicked: false,
        long_pressed: false,
        menu: false,
        pressed: false,
        hovered: false,
        pointer: None,
    }
}

/// A picture well: the first of `art` that loads, cropped to fill `rect`, over the gradient
/// `seed` picks until it covers it (`Artwork` over a Surface2 box; the music pages give it no
/// title style, so no title is written on the gradient).
pub fn art_well(ui: &mut Ui, rect: Rect, art: &[ImageRequest], seed: &str, corner: f32) {
    let shown = first_art(ui, art, Priority::Visible);
    if shown.is_some_and(|s| s.opaque()) {
        ui.fill(rect, theme::SURFACE2, corner);
    } else {
        placeholder(ui, rect, seed, "", corner);
    }
    if let Some(s) = shown {
        ui.draw_image(rect, &s, &ImageOpts::cover().rounded(corner));
    }
}

/// The widest the faint art is asked for: drawn at a fifth of its strength, it needs no more (the
/// TV app's `FaintArt` asks for 960 px).
pub const FAINT_ART_PX: u32 = 960;

/// The album art over the whole page at `alpha` (14 % on an album, 20 % on Now Playing),
/// unblurred, fading into Bg from 40 % of the height down (`FaintArt`): the gradient `seed` picks
/// until the picture covers it, and nothing at all without a picture to show.
pub fn faint_art(ui: &mut Ui, art: &[ImageRequest], seed: &str, alpha: f32) {
    if art.is_empty() {
        return;
    }
    let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    let shown = first_art(ui, art, Priority::Visible);
    ui.opacity(alpha, |ui| {
        if !shown.as_ref().is_some_and(|s| s.opaque()) {
            placeholder(ui, full, seed, "", 0.0);
        }
        if let Some(s) = &shown {
            ui.draw_image(full, s, &ImageOpts::cover());
        }
    });
    ui.ramp(full, true, &theme::FAINT_ART_FADE);
}

/// A star's side and the gap between stars.
const STAR: f32 = 16.0;
const STAR_GAP: f32 = 2.0;

/// The profile's rating as five 16 dp stars from `at`, lit in Accent, else Surface4.
pub fn stars(ui: &mut Ui, at: Point, rating: i64) {
    for star in 1..=5 {
        let x = at.x + (star - 1) as f32 * (STAR + STAR_GAP);
        let color = if star_lit(rating, star) { theme::ACCENT } else { theme::SURFACE4 };
        ui.icon(Rect::new(x, at.y, STAR, STAR), Icon::Star, color);
    }
}

/// The most of a message's sentence laid out: it may be the server's own words, of any length.
const MESSAGE_CHARS: usize = 400;
const MESSAGE_LINES: u16 = 4;

/// `body` cut to [`MESSAGE_CHARS`].
fn clipped(body: &str) -> &str {
    body.char_indices().nth(MESSAGE_CHARS).map_or(body, |(end, _)| &body[..end])
}

/// A page-sized message (`MusicMessage`): at x 96, y 60, a title in `Tv.Title`, its sentence in
/// `Tv.Body` Muted 10 below (four lines at most), and one pill 32 below that, which takes the
/// first focus.
pub fn message(ui: &mut Ui, title: &str, body: &str, action: &str) -> Response {
    let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    let body = clipped(body);
    let mut y = 60.0;
    y += ui.text(Point::new(theme::GUTTER, y), title, &theme::TITLE, &TextOpts::wrap(width, 2), theme::FG).h;
    y += 10.0;
    let opts = TextOpts::wrap(width, MESSAGE_LINES);
    y += ui.text(Point::new(theme::GUTTER, y), body, &theme::BODY, &opts, theme::MUTED).h;
    y += 10.0 + 12.0 + 10.0;
    let id = ui.id("message");
    ui.initial_focus(id);
    Button::pill(action).show(ui, id, Point::new(theme::GUTTER, y))
}

/// What a page's list (an album's tracks, an artist's albums) shows.
pub enum ListState<'a> {
    /// Its skeleton: nothing came yet, or it is asked for again after Retry.
    Loading,
    /// Why it couldn't come, with Retry.
    Failed(&'a ApiError),
    /// What came, saved or fresh.
    Ready,
}

/// What `list` shows. Android keeps a list that failed loading for good; here it says why and
/// offers Retry, rather than calling it empty.
pub fn list_state(list: &Resource<ItemList>) -> ListState<'_> {
    match (list.value(), list.error()) {
        (Some(_), _) => ListState::Ready,
        (None, Some(e)) if !list.asking() => ListState::Failed(e),
        _ => ListState::Loading,
    }
}

/// A list that couldn't come, in its place at `at`: why, in `Tv.Body` Muted (three lines at
/// most), and Retry 12 below.
pub fn list_failure(ui: &mut Ui, at: Point, width: f32, body: &str) -> Response {
    let h = ui.text(at, clipped(body), &theme::BODY, &TextOpts::wrap(width, 3), theme::MUTED).h;
    Button::pill("Retry").show(ui, ui.id("retry"), Point::new(at.x, at.y + h + 12.0))
}

/// A track's line in a list (`TvTrackLine`): the number, or for the track playing an
/// audio-lines (playing) or play glyph in Accent; the title over its own artist; the length.
/// Focused it is an Accent pill without scale; the track it was opened from is Accent 14 %, the
/// one playing Fg 6 % with its title in AccentBright.
#[derive(Clone, Copy, Debug)]
pub struct TrackLine<'a> {
    pub number: &'a str,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub duration: &'a str,
    pub current: bool,
    pub playing: bool,
    pub highlighted: bool,
}

/// A line's least height, its padding, and its number column (`TvTrackLine`).
pub const LINE_MIN: f32 = 44.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 6.0;
const NUMBER_W: f32 = 28.0;
const NUMBER_GAP: f32 = 16.0;
const GLYPH: f32 = 18.0;

impl TrackLine<'_> {
    /// The line's height: its text with the padding, at least 44.
    pub fn height(&self) -> f32 {
        let text = theme::BODY.line_height + if self.subtitle.is_some() { theme::CAPTION.line_height } else { 0.0 };
        (text + 2.0 * PAD_Y).max(LINE_MIN)
    }

    /// Draws the line in `rect`, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, rect: Rect) -> Response {
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

        let number = Rect::new(rect.x + PAD_X, rect.y, NUMBER_W, rect.h);
        if self.current {
            let glyph = if self.playing { Icon::AudioLines } else { Icon::Play };
            let at = Rect::new(number.right() - GLYPH, rect.y + (rect.h - GLYPH) / 2.0, GLYPH, GLYPH);
            ui.icon(at, glyph, on(theme::ACCENT));
        } else if !self.number.is_empty() {
            let layout = ui.layout(self.number, &theme::CAPTION, &TextOpts::line(NUMBER_W));
            let at = Point::new(number.right() - layout.size.w, rect.y + (rect.h - layout.size.h) / 2.0);
            ui.draw_text(at, layout, on(theme::SUBTLE));
        }

        let right = rect.right() - PAD_X;
        let duration =
            (!self.duration.is_empty()).then(|| ui.layout(self.duration, &theme::CAPTION, &TextOpts::default()));
        let duration_w = duration.as_ref().map_or(0.0, |d| d.size.w);
        if let Some(d) = duration {
            let at = Point::new(right - d.size.w, rect.y + (rect.h - d.size.h) / 2.0);
            ui.draw_text(at, d, on(theme::MUTED));
        }

        let x = number.right() + NUMBER_GAP;
        let width = (right - duration_w - PAD_X - x).max(1.0);
        let block = theme::BODY.line_height + if self.subtitle.is_some() { theme::CAPTION.line_height } else { 0.0 };
        let mut y = rect.y + (rect.h - block) / 2.0;
        let title = if self.current { theme::ACCENT_BRIGHT } else { theme::FG };
        ui.text(Point::new(x, y), self.title, &theme::BODY, &TextOpts::line(width), on(title));
        y += theme::BODY.line_height;
        if let Some(s) = self.subtitle {
            ui.text(Point::new(x, y), s, &theme::CAPTION, &TextOpts::line(width), on(theme::MUTED));
        }
        r
    }
}
