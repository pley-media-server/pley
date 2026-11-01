//! Pieces Live TV's pages share (`TvLiveKit.kt`): a channel's logo in its well, how far a
//! programme is, its Live or New badge, and text placed in a box.

use std::sync::Arc;

use pley_base::{Align, Color, ImageState, Point, Rect, TextLayout, TextOpts, TextStyle};
use pley_core::model::{LiveChannel, LiveProgram};
use pley_kit::{Badge, placeholder};
use pley_shell::AppCx;
use pley_ui::theme::{self, radius};
use pley_ui::{ImageOpts, Ui};

/// How a channel's well looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Well {
    /// Its fill under the logo (Surface3 on pages, white at 6–8 % in the player).
    pub color: Color,
    pub radius: f32,
    /// Without a logo, the name rather than the number.
    pub name: bool,
    /// The label's style without a logo.
    pub style: TextStyle,
}

impl Well {
    /// The pages' well: Surface3, the tiles' 4 dp corners, the number in `Tv.Label`.
    pub const PAGE: Well = Well { color: theme::SURFACE3, radius: radius::TILE, name: false, style: theme::LABEL };

    /// The well with the channel's name instead of its number.
    pub fn named(self) -> Well {
        Well { name: true, ..self }
    }

    /// The well with `style` for its label.
    pub fn styled(self, style: TextStyle) -> Well {
        Well { style, ..self }
    }

    /// The well filled with `color`, its corners `radius`.
    pub fn filled(self, color: Color, radius: f32) -> Well {
        Well { color, radius, ..self }
    }
}

/// A channel's logo whole, 8 dp inside its well; without one (or when it can't be had) its number
/// (or name) on the placeholder's gradient (`ChannelTile`, `ChannelLogo`).
pub fn channel_tile(ui: &mut Ui, cx: &AppCx, rect: Rect, channel: &LiveChannel, well: Well) {
    let logo = channel.logo.as_deref().filter(|l| !l.is_empty());
    let request = logo.and_then(|path| cx.image(path, ui.image_px(rect.w - 16.0)));
    let shown = request.as_ref().map(|r| ui.image_state(r, pley_base::Priority::Visible));
    let failed = shown.is_none_or(|s| matches!(s.state, ImageState::Failed));
    if failed {
        placeholder(ui, rect, &channel.name, "", well.radius);
        let label = if well.name {
            channel.name.clone()
        } else {
            channel.number.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| channel.name.chars().take(3).collect())
        };
        let inner = rect.expand(-6.0);
        let layout = ui.layout(&label, &well.style, &TextOpts::wrap(inner.w.max(1.0), 2).aligned(Align::Center));
        let at = Point::new(inner.x + (inner.w - layout.size.w) / 2.0, rect.y + (rect.h - layout.size.h) / 2.0);
        ui.draw_text(at, layout, theme::PLACEHOLDER_TITLE);
        return;
    }
    ui.fill(rect, well.color, well.radius);
    if let Some(shown) = shown {
        ui.draw_image(rect.expand(-8.0), &shown, &ImageOpts::contain());
    }
}

/// How far `program` is at `now`: a 3 dp track with the accent over it (`ProgramProgress`).
pub fn progress(ui: &mut Ui, rect: Rect, program: &LiveProgram, now: i64, track: Color, fill: Color) {
    ui.fill(rect, track, radius::BADGE);
    let f = program.progress_at(now);
    if f > 0.0 {
        ui.fill(Rect::new(rect.x, rect.y, rect.w * f, rect.h), fill, radius::BADGE);
    }
}

/// The badge a programme carries, Live before New, drawn from `at`; its width (0 for none).
pub fn programme_badge(ui: &mut Ui, at: Point, program: &LiveProgram) -> f32 {
    let badge = if program.is_live {
        Badge::live()
    } else if program.is_new {
        Badge::new_programme()
    } else {
        return 0.0;
    };
    badge.show(ui, at).w
}

/// The height of a programme badge (`Tv.Badge` and 2 dp above and below).
pub const BADGE_H: f32 = 17.0;

/// Draws `layout` centred vertically in the box from `top`, `height` tall, from `x`.
pub fn text_mid(ui: &mut Ui, x: f32, top: f32, height: f32, layout: Arc<TextLayout>, color: Color) {
    let y = top + (height - layout.size.h) / 2.0;
    ui.draw_text(Point::new(x, y), layout, color);
}

/// One line of `text` cut at `width`, centred vertically in the box from `top`; its width.
#[allow(clippy::too_many_arguments)]
pub fn line_mid(
    ui: &mut Ui,
    text: &str,
    style: &TextStyle,
    x: f32,
    top: f32,
    height: f32,
    width: f32,
    color: Color,
) -> f32 {
    let layout = ui.layout(text, style, &TextOpts::line(width.max(1.0)));
    let w = layout.size.w;
    text_mid(ui, x, top, height, layout, color);
    w
}

/// One line of `text` uncut: its laid-out width.
pub fn measure(ui: &Ui, text: &str, style: &TextStyle) -> f32 {
    ui.measure(text, style, &TextOpts { max_width: None, ellipsis: false, ..TextOpts::default() }).w
}

/// The rectangle `aspect` (width / height) wide that fits whole in `area`, centred.
pub fn fit(area: Rect, aspect: f32) -> Rect {
    if aspect <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return area;
    }
    let (w, h) = if area.w / area.h > aspect { (area.h * aspect, area.h) } else { (area.w, area.w / aspect) };
    Rect::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_picture_fits_across_a_tall_area() {
        let r = fit(Rect::new(0.0, 0.0, 600.0, 540.0), 16.0 / 9.0);
        assert_eq!((r.x, r.w), (0.0, 600.0));
        assert!((r.h - 337.5).abs() < 0.01 && (r.y - 101.25).abs() < 0.01);
        let narrow = fit(Rect::new(0.0, 0.0, 960.0, 540.0), 4.0 / 3.0);
        assert_eq!((narrow.y, narrow.h), (0.0, 540.0));
        assert!((narrow.x - 120.0).abs() < 0.01);
    }
}
