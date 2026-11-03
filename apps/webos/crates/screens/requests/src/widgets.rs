//! Controls of these pages the kit hasn't: a chip or tab with a number after its words (the way to
//! the requests, the requests' tabs), and a title's facts in a line.

use pley_base::{Icon, Point, Rect, Size, TextOpts};
use pley_core::model::{DiscoverDetail, DiscoverTitle, TitleKind};
use pley_core::requests::title_facts;
use pley_kit::MetaLine;
use pley_kit::look::shade;
use pley_ui::theme::{self, focus::ControlLook, radius};
use pley_ui::{Id, Response, Ui};

/// A count's pill: 20 high, at least 20 wide.
const COUNT_H: f32 = 20.0;

/// The count's words: "99+" past 99 (`max`), and so on.
pub fn count_text(n: i64, max: i64) -> String {
    if n > max { format!("{max}+") } else { n.to_string() }
}

/// What a counted control shows.
pub struct Counted<'a> {
    pub look: ControlLook,
    pub icon: Option<Icon>,
    pub label: &'a str,
    pub count: Option<String>,
    /// The count in the danger colour (failed requests).
    pub danger: bool,
    pub padding: f32,
}

impl Counted<'_> {
    fn count_w(&self, ui: &Ui) -> f32 {
        self.count
            .as_ref()
            .map_or(0.0, |c| 8.0 + (ui.measure(c, &theme::BADGE, &TextOpts::default()).w + 12.0).max(COUNT_H))
    }

    pub fn size(&self, ui: &Ui) -> Size {
        let icon = if self.icon.is_some() { theme::ICON_SIZE + 8.0 } else { 0.0 };
        let label = ui.measure(self.label, &theme::LABEL, &TextOpts::default()).w;
        Size::new(self.padding * 2.0 + icon + label + self.count_w(ui), theme::BUTTON_HEIGHT)
    }

    /// Draws it with its top-left corner at `at`, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::from_origin_size(at, self.size(ui));
        let r = ui.focusable(id, rect);
        let s = shade(ui, &self.look, &r);
        let count_w = self.count_w(ui);
        ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
            ui.fill(rect, s.fill, radius::CONTROL);
            let mut x = rect.x + self.padding;
            if let Some(icon) = self.icon {
                let r = Rect::new(x, rect.y + (rect.h - theme::ICON_SIZE) / 2.0, theme::ICON_SIZE, theme::ICON_SIZE);
                ui.icon(r, icon, s.content);
                x += theme::ICON_SIZE + 8.0;
            }
            let layout = ui.layout(self.label, &theme::LABEL, &TextOpts::default());
            let w = layout.size.w;
            ui.draw_text(Point::new(x, rect.y + (rect.h - layout.size.h) / 2.0), layout, s.content);
            x += w;
            if let Some(count) = &self.count {
                let pill = Rect::new(x + 8.0, rect.y + (rect.h - COUNT_H) / 2.0, count_w - 8.0, COUNT_H);
                let (fill, text) = if self.danger {
                    (theme::DANGER, theme::BG)
                } else if r.focused {
                    (theme::ON_ACCENT.alpha(0.16), s.content)
                } else {
                    (self.count_fill(), theme::FG)
                };
                ui.fill(pill, fill, COUNT_H / 2.0);
                let layout = ui.layout(count, &theme::BADGE, &TextOpts::default());
                let at = Point::new(pill.x + (pill.w - layout.size.w) / 2.0, pill.y + (pill.h - layout.size.h) / 2.0);
                ui.draw_text(at, layout, text);
            }
        });
        r
    }

    fn count_fill(&self) -> pley_base::Color {
        // The way to the requests counts in the accent; tabs on Surface4.
        if self.icon.is_some() { theme::ACCENT } else { theme::SURFACE4 }
    }
}

/// A title's facts in a line (`TitleMeta`): year and length (or seasons), Movie or Show, the
/// certification, TMDB's rating with a star.
pub fn title_meta(ui: &mut Ui, at: Point, t: &DiscoverTitle, d: Option<&DiscoverDetail>, width: f32) {
    let facts = match d {
        Some(d) => title_facts(d),
        None => t.year.filter(|y| *y != 0).map(|y| y.to_string()).unwrap_or_default(),
    };
    let which = if t.kind == TitleKind::Show { "Show" } else { "Movie" };
    let words = [facts.as_str(), which].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ");
    let cert = d.and_then(|d| d.certification.clone()).unwrap_or_default();
    let rating = t.rating.filter(|r| *r > 0.0).map(|r| format!("{r:.1}")).unwrap_or_default();
    MetaLine::new().text(&words).content_rating(&cert).stars(&rating).show(ui, at, width);
}
