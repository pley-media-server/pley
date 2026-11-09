//! A season tab with its poster (Android's poster tabs in `TvEpisodes.kt`), for a show whose
//! seasons have posters of their own.

use pley_base::{ImageRequest, Point, Priority, Rect, Size};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{FocusOpts, Id, ImageOpts, Response, Ui};

use crate::look::{free_line, mix, register, shade};
use crate::tile::{first_art, placeholder};

/// A poster tab's height.
pub const POSTER_TAB_HEIGHT: f32 = 72.0;
/// From the tab's start to its poster, and from its top: the first tab's poster sits at the gutter.
pub const POSTER_TAB_INSET: f32 = 6.0;
const POSTER_W: f32 = 40.0;
const POSTER_H: f32 = 60.0;
/// From the poster to the words, and from the words to the tab's end.
const GAP: f32 = 12.0;
const END: f32 = 20.0;

/// A season tab with its poster: 72 tall, a 40 by 60 poster 6 from its top and start (the
/// season's, else the show's; the title's gradient until it comes), the season's name over its
/// episodes beside it. It looks as a season tab does: transparent and Muted, the season shown on
/// Surface3 in Fg, focused the accent at 1.05.
#[derive(Clone, Copy, Debug)]
pub struct PosterTab<'a> {
    label: &'a str,
    episodes: &'a str,
    poster: &'a [ImageRequest],
    current: bool,
}

impl<'a> PosterTab<'a> {
    /// The tab of the season `label` ("Season 2"), with `episodes` ("8 episodes", or empty) under
    /// it and `poster`'s first picture that loads; `current` for the season shown.
    pub fn new(label: &'a str, episodes: &'a str, poster: &'a [ImageRequest], current: bool) -> PosterTab<'a> {
        PosterTab { label, episodes, poster, current }
    }

    /// Its size: the poster, the wider of its two lines, and the room around them; 72 tall.
    pub fn size(&self, ui: &Ui) -> Size {
        let label = ui.measure(self.label, &theme::LABEL, &free_line()).w;
        let episodes =
            if self.episodes.is_empty() { 0.0 } else { ui.measure(self.episodes, &theme::CAPTION, &free_line()).w };
        Size::new(POSTER_TAB_INSET + POSTER_W + GAP + label.max(episodes) + END, POSTER_TAB_HEIGHT)
    }

    /// Draws the tab with its top-left corner at `at`, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::from_origin_size(at, self.size(ui));
        let look = if self.current { focus::TAB_CURRENT } else { focus::TAB };
        ui.name(id, || {
            if self.episodes.is_empty() { self.label.to_string() } else { format!("{}, {}", self.label, self.episodes) }
        });
        let r = register(ui, id, rect, FocusOpts::default());
        let s = shade(ui, &look, &r);
        ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
            ui.fill(rect, s.fill, radius::CONTROL);
            let well = Rect::new(rect.x + POSTER_TAB_INSET, rect.y + POSTER_TAB_INSET, POSTER_W, POSTER_H);
            let shown = first_art(ui, self.poster, Priority::Visible);
            if !shown.is_some_and(|p| p.opaque()) {
                placeholder(ui, well, self.label, "", radius::BADGE);
            }
            if let Some(p) = shown {
                ui.draw_image(well, &p, &ImageOpts::cover().rounded(radius::BADGE));
            }
            let x = well.x + POSTER_W + GAP;
            let label = ui.layout(self.label, &theme::LABEL, &free_line());
            let episodes = (!self.episodes.is_empty()).then(|| ui.layout(self.episodes, &theme::CAPTION, &free_line()));
            let lines = theme::LABEL.line_height + if episodes.is_some() { theme::CAPTION.line_height } else { 0.0 };
            let y = rect.y + (rect.h - lines) / 2.0;
            ui.draw_text(Point::new(x, y), label, s.content);
            if let Some(layout) = episodes {
                let ink = mix(theme::MUTED, theme::ON_ACCENT.alpha(0.8), s.progress);
                ui.draw_text(Point::new(x, y + theme::LABEL.line_height), layout, ink);
            }
        });
        r
    }
}
