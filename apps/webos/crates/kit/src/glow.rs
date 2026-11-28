//! The themes' backdrops (docs/THEMES.md, "Backdrops"): Twilight's sunset along the foot of the
//! screen, Moonlight's moon and Nightshade's light at the top right, Sunshine's bubbles of colored
//! light, and Sunshine's rainbow down the open rail. They are the web's `--glow` and `--sidebar`
//! (`apps/web/src/index.css`) on the 960 × 540 canvas, its percentages taken of the screen.
//!
//! Each is a few vertex gradients over the page's Bg, drawn under the page, never per-pixel work:
//! only the part of each glow inside the screen is filled. Low-tier TVs, whose GPUs have the least
//! to spare while a row scrolls, draw the plain Bg.

use pley_base::{Color, GradientStop, Point, Rect};
use pley_ui::theme::{self, Theme};
use pley_ui::{Tier, Ui};

/// `rgb(r g b / a)`.
const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color { r, g, b, a: (a * 255.0 + 0.5) as u8 }
}

/// A CSS radial gradient from `color` at its center to transparent at `reach` of its size: an
/// ellipse of `size` (fractions of the screen's width and height), or a circle reaching the
/// screen's farthest corner.
struct Glow {
    /// Its center, as fractions of the screen's width and height.
    at: (f32, f32),
    size: Option<(f32, f32)>,
    color: Color,
    reach: f32,
}

impl Glow {
    fn draw(&self, ui: &mut Ui, screen: Rect) {
        let center = Point::new(screen.w * self.at.0, screen.h * self.at.1);
        let (rx, ry) = match self.size {
            Some((w, h)) => (screen.w * w, screen.h * h),
            None => {
                let dx = center.x.max(screen.w - center.x);
                let dy = center.y.max(screen.h - center.y);
                let r = (dx * dx + dy * dy).sqrt();
                (r, r)
            }
        };
        let fade = Color { a: 0, ..self.color };
        ui.radial_ellipse(screen, center, (rx * self.reach, ry * self.reach), self.color, fade);
    }
}

/// Twilight's sun along the foot of the screen, topmost first (as CSS lists its layers).
const TWILIGHT: [Glow; 3] = [
    Glow { at: (0.5, 1.18), size: Some((1.1, 0.55)), color: rgba(255, 138, 76, 0.26), reach: 0.65 },
    Glow { at: (0.96, 1.06), size: Some((0.6, 0.5)), color: rgba(244, 114, 182, 0.26), reach: 0.7 },
    Glow { at: (0.04, 1.1), size: Some((0.55, 0.45)), color: rgba(168, 85, 247, 0.24), reach: 0.7 },
];

/// Moonlight's moon at the top right.
const MOONLIGHT: [Glow; 1] =
    [Glow { at: (0.92, -0.06), size: Some((0.55, 0.4)), color: rgba(245, 214, 140, 0.07), reach: 0.7 }];

/// Nightshade's faint green light at the top right.
const NIGHTSHADE: [Glow; 1] =
    [Glow { at: (0.88, -0.08), size: Some((0.7, 0.45)), color: rgba(95, 211, 154, 0.07), reach: 0.7 }];

/// Sunshine's bubbles, topmost first.
const SUNSHINE: [Glow; 5] = [
    Glow { at: (0.34, 0.1), size: None, color: rgba(255, 99, 146, 0.24), reach: 0.2 },
    Glow { at: (0.96, 0.08), size: None, color: rgba(56, 182, 255, 0.28), reach: 0.22 },
    Glow { at: (0.9, 0.92), size: None, color: rgba(255, 196, 0, 0.32), reach: 0.24 },
    Glow { at: (0.42, 0.96), size: None, color: rgba(74, 214, 140, 0.26), reach: 0.22 },
    Glow { at: (0.68, 0.48), size: None, color: rgba(150, 100, 255, 0.12), reach: 0.22 },
];

/// Sunshine's rainbow down its sidebar, top to bottom.
const RAINBOW: [(f32, Color); 6] = [
    (0.0, rgba(255, 99, 146, 0.22)),
    (0.2, rgba(255, 158, 64, 0.22)),
    (0.4, rgba(255, 214, 0, 0.24)),
    (0.6, rgba(74, 214, 140, 0.22)),
    (0.8, rgba(56, 182, 255, 0.22)),
    (1.0, rgba(150, 100, 255, 0.22)),
];

/// Whether this TV draws the themes' backdrops.
fn drawn(ui: &Ui) -> bool {
    ui.tier() != Tier::Low
}

/// The theme's backdrop over the whole screen, under the page: none for a theme without one, in a
/// player, or on a Low-tier TV.
pub fn theme_glow(ui: &mut Ui) {
    if !drawn(ui) {
        return;
    }
    let screen = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    let glows: &[Glow] = match theme::palette().theme {
        Theme::Twilight => {
            // The sun's last light, from 35 % of the way down, under its glows.
            let top = screen.h * 0.35;
            let stops = vec![
                GradientStop::new(0.0, rgba(190, 60, 130, 0.0)),
                GradientStop::new((0.72 - 0.35) / 0.65, rgba(190, 60, 130, 0.08)),
                GradientStop::new(1.0, rgba(255, 122, 72, 0.12)),
            ];
            ui.gradient(Rect::new(0.0, top, screen.w, screen.h - top), true, stops);
            &TWILIGHT
        }
        Theme::Moonlight => &MOONLIGHT,
        Theme::Nightshade => &NIGHTSHADE,
        Theme::Sunshine => &SUNSHINE,
        _ => &[],
    };
    for glow in glows.iter().rev() {
        glow.draw(ui, screen);
    }
}

/// The theme's sidebar over `rect` (the open rail's panel): Sunshine's rainbow, else nothing.
pub fn sidebar_glow(ui: &mut Ui, rect: Rect) {
    if theme::palette().theme == Theme::Sunshine && drawn(ui) {
        ui.gradient(rect, true, RAINBOW.iter().map(|&(at, c)| GradientStop::new(at, c)).collect());
    }
}

#[cfg(test)]
mod tests {
    use pley_base::Cmd;
    use pley_ui::testing::Harness;
    use pley_ui::theme::{Palette, Theme};

    use super::*;

    fn drawn_in(theme: Theme, tier: Tier) -> Vec<Cmd> {
        theme::show(Palette::of(theme, None));
        let mut h = Harness::new();
        h.state.set_tier(tier);
        let f = h.idle(&mut |ui: &mut Ui| theme_glow(ui));
        theme::show(Palette::of(Theme::Midnight, None));
        f.list.cmds
    }

    #[test]
    fn each_theme_draws_its_own_backdrop() {
        assert!(drawn_in(Theme::Midnight, Tier::Mid).is_empty());
        assert!(drawn_in(Theme::Blacklight, Tier::High).is_empty());
        assert_eq!(drawn_in(Theme::Moonlight, Tier::Mid).len(), 1);
        assert_eq!(drawn_in(Theme::Nightshade, Tier::Mid).len(), 1);
        // The sunset's band, then its three glows.
        let twilight = drawn_in(Theme::Twilight, Tier::High);
        assert!(matches!(twilight[0], Cmd::LinearGradient { .. }));
        assert_eq!(twilight.len(), 4);
        let sunshine = drawn_in(Theme::Sunshine, Tier::Mid);
        assert_eq!(sunshine.len(), 5);
        // The topmost bubble is drawn last; it reaches a fifth of the way to the farthest corner.
        let Cmd::RadialGradient { center, radius, squash, inner, .. } = sunshine[4] else { panic!() };
        assert_eq!((center, squash, inner), (Point::new(326.4, 54.0), 1.0, rgba(255, 99, 146, 0.24)));
        let far = ((960.0f32 - 326.4).powi(2) + (540.0f32 - 54.0).powi(2)).sqrt();
        assert!((radius - far * 0.2).abs() < 0.01);
    }

    #[test]
    fn low_tier_tvs_draw_the_plain_page() {
        assert!(drawn_in(Theme::Twilight, Tier::Low).is_empty());
        assert!(drawn_in(Theme::Sunshine, Tier::Low).is_empty());
    }
}
