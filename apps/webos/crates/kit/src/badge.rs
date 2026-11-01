//! Badges and the metadata line. Quality and rating badges are `Tv.Badge` text 18 dp tall with 6 dp
//! of padding and 2 dp corners: on art on Bg at 80 %, on the page outlined in 1 dp of Outline
//! (`TileCaption.kt`, `TileOverlays.kt`). Live TV's are uppercase with 5 × 2 of padding: LIVE and
//! ON NOW on Danger, NEW on PrimaryContainer (`TvLiveTvScreen.kt`). The metadata line (`TvMetaLine`
//! in `TvHomeParts.kt`) strings a title's facts together with "·".

use pley_base::{Color, Icon, Point, Rect, Size};
use pley_ui::Ui;
use pley_ui::theme::{self, radius};

use crate::look::{free_line, inner_stroke, line_within, text_mid};

/// How a [`Badge`] looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BadgeStyle {
    /// On art: Bg at 80 %, Fg text (quality badges on tiles, a collection's count).
    OnArt,
    /// On the page: no fill, a 1 dp Outline edge, Muted text (content ratings, quality facts).
    Outline,
    /// The unwatched count: Accent, OnAccent text, at least 20 wide.
    Count,
    /// LIVE and ON NOW: Danger, Bg text.
    Live,
    /// NEW: PrimaryContainer, AccentBright text.
    New,
    /// A channel's number on its tile: Bg at 80 %, Fg text, 5 × 2 of padding.
    Number,
}

/// A badge's height, except Live TV's.
pub const BADGE_HEIGHT: f32 = 18.0;

/// A small label in a box.
#[derive(Clone, Copy, Debug)]
pub struct Badge<'a> {
    text: &'a str,
    style: BadgeStyle,
}

impl<'a> Badge<'a> {
    /// A badge saying `text`.
    pub fn new(text: &'a str, style: BadgeStyle) -> Badge<'a> {
        Badge { text, style }
    }

    /// "LIVE".
    pub fn live() -> Badge<'static> {
        Badge::new("LIVE", BadgeStyle::Live)
    }

    /// "ON NOW".
    pub fn on_now() -> Badge<'static> {
        Badge::new("ON NOW", BadgeStyle::Live)
    }

    /// "NEW".
    pub fn new_programme() -> Badge<'static> {
        Badge::new("NEW", BadgeStyle::New)
    }

    fn padding(&self) -> (f32, f32) {
        match self.style {
            BadgeStyle::Live | BadgeStyle::New | BadgeStyle::Number => (5.0, 2.0),
            _ => (6.0, 0.0),
        }
    }

    fn colors(&self) -> (Color, Color) {
        match self.style {
            BadgeStyle::OnArt | BadgeStyle::Number => (theme::ON_ART, theme::FG),
            BadgeStyle::Outline => (Color::TRANSPARENT, theme::MUTED),
            BadgeStyle::Count => (theme::ACCENT, theme::ON_ACCENT),
            BadgeStyle::Live => (theme::DANGER, theme::BG),
            BadgeStyle::New => (theme::PRIMARY_CONTAINER, theme::ACCENT_BRIGHT),
        }
    }

    /// Its size.
    pub fn size(&self, ui: &Ui) -> Size {
        let w = ui.measure(self.text, &theme::BADGE, &free_line()).w;
        let (px, py) = self.padding();
        let h = if py > 0.0 { theme::BADGE.line_height + 2.0 * py } else { BADGE_HEIGHT };
        let min = if self.style == BadgeStyle::Count { BADGE_HEIGHT + 2.0 } else { 0.0 };
        Size::new((w + 2.0 * px).max(min), h)
    }

    /// Draws it with its top-left corner at `at`; returns its size.
    pub fn show(&self, ui: &mut Ui, at: Point) -> Size {
        let size = self.size(ui);
        let rect = Rect::from_origin_size(at, size);
        let (fill, text) = self.colors();
        ui.fill(rect, fill, radius::BADGE);
        if self.style == BadgeStyle::Outline {
            inner_stroke(ui, rect, theme::OUTLINE, radius::BADGE, 1.0);
        }
        let layout = ui.layout(self.text, &theme::BADGE, &free_line());
        let x = rect.x + (rect.w - layout.size.w) / 2.0;
        text_mid(ui, x, rect.y, rect.h, layout, text);
        size
    }
}

#[derive(Clone, Copy, Debug)]
enum Part<'a> {
    Lead(&'a str),
    Text(&'a str),
    Rating(&'a str),
    Stars(&'a str),
    Quality(&'a str),
}

/// A title's facts on one 18 dp line, 8 apart with "·" between them in Subtle (`TvMetaLine`): a
/// lead in Fg that gives way first when the line is too long, then facts in Muted, the content
/// rating outlined, "★ 7.4" (a 14 dp Accent star, 4, the rating in Fg), and quality badges 6
/// apart. Parts with nothing to say are left out, with their separator.
#[derive(Clone, Debug, Default)]
pub struct MetaLine<'a> {
    parts: Vec<Part<'a>>,
    star: f32,
}

/// The line's height.
pub const META_HEIGHT: f32 = 18.0;
const META_GAP: f32 = 8.0;
const QUALITY_GAP: f32 = 6.0;

impl<'a> MetaLine<'a> {
    /// An empty line (its star 14 dp, as on Home; the item page's is 16).
    pub fn new() -> MetaLine<'a> {
        MetaLine { parts: Vec::new(), star: 14.0 }
    }

    /// The star's size.
    pub fn star_size(self, size: f32) -> MetaLine<'a> {
        MetaLine { star: size, ..self }
    }

    fn push(mut self, p: Part<'a>, text: &str) -> MetaLine<'a> {
        if !text.is_empty() {
            self.parts.push(p);
        }
        self
    }

    /// The lead (an episode's "S2 · E5 · The Long Night"), in Fg.
    pub fn lead(self, text: &'a str) -> MetaLine<'a> {
        self.push(Part::Lead(text), text)
    }

    /// A fact: a year, a runtime, "3 seasons".
    pub fn text(self, text: &'a str) -> MetaLine<'a> {
        self.push(Part::Text(text), text)
    }

    /// The content rating, outlined.
    pub fn content_rating(self, text: &'a str) -> MetaLine<'a> {
        self.push(Part::Rating(text), text)
    }

    /// The audience or critic rating after a star: "7.4".
    pub fn stars(self, text: &'a str) -> MetaLine<'a> {
        self.push(Part::Stars(text), text)
    }

    /// Quality badges ("4K", "DV P8.1", "Atmos"), 6 apart.
    pub fn quality(mut self, badges: &'a [String]) -> MetaLine<'a> {
        for b in badges {
            self = self.push(Part::Quality(b), b);
        }
        self
    }

    /// Whether nothing would be drawn.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    fn width_of(&self, ui: &Ui, p: &Part) -> f32 {
        match p {
            Part::Lead(t) | Part::Text(t) => ui.measure(t, &theme::LABEL, &free_line()).w,
            Part::Rating(t) | Part::Quality(t) => Badge::new(t, BadgeStyle::Outline).size(ui).w,
            Part::Stars(t) => self.star + 4.0 + ui.measure(t, &theme::LABEL, &free_line()).w,
        }
    }

    /// Draws the line from `at`, within `max_width`; returns the width drawn.
    pub fn show(&self, ui: &mut Ui, at: Point, max_width: f32) -> f32 {
        if self.parts.is_empty() {
            return 0.0;
        }
        let dot = ui.measure("·", &theme::LABEL, &free_line()).w;
        let gap = |a: &Part, b: &Part| match (a, b) {
            (Part::Quality(_), Part::Quality(_)) => QUALITY_GAP,
            _ => META_GAP * 2.0 + dot,
        };
        let mut fixed = 0.0;
        let mut lead_w = 0.0;
        for (i, p) in self.parts.iter().enumerate() {
            let w = self.width_of(ui, p);
            match p {
                Part::Lead(_) => lead_w = w,
                _ => fixed += w,
            }
            if i > 0 {
                fixed += gap(&self.parts[i - 1], p);
            }
        }
        let lead_room = (max_width - fixed).max(0.0).min(lead_w);
        let mut x = at.x;
        for (i, p) in self.parts.iter().enumerate() {
            if i > 0 {
                let g = gap(&self.parts[i - 1], p);
                if g > QUALITY_GAP {
                    let layout = ui.layout("·", &theme::LABEL, &free_line());
                    text_mid(ui, x + META_GAP, at.y, META_HEIGHT, layout, theme::SUBTLE);
                }
                x += g;
            }
            match p {
                Part::Lead(t) => {
                    let layout = ui.layout(t, &theme::LABEL, &line_within(Some(lead_room)));
                    let w = layout.size.w;
                    text_mid(ui, x, at.y, META_HEIGHT, layout, theme::FG);
                    x += w;
                }
                Part::Text(t) => {
                    let layout = ui.layout(t, &theme::LABEL, &free_line());
                    let w = layout.size.w;
                    text_mid(ui, x, at.y, META_HEIGHT, layout, theme::MUTED);
                    x += w;
                }
                Part::Rating(t) | Part::Quality(t) => {
                    x += Badge::new(t, BadgeStyle::Outline).show(ui, Point::new(x, at.y)).w;
                }
                Part::Stars(t) => {
                    let star = Rect::new(x, at.y + (META_HEIGHT - self.star) / 2.0, self.star, self.star);
                    ui.icon(star, Icon::Star, theme::ACCENT);
                    x += self.star + 4.0;
                    let layout = ui.layout(t, &theme::LABEL, &free_line());
                    let w = layout.size.w;
                    text_mid(ui, x, at.y, META_HEIGHT, layout, theme::FG);
                    x += w;
                }
            }
        }
        x - at.x
    }
}
