//! What the screens of getting in share (`OnboardingKit.kt`, the TV app's `onboarding/kit.tsx`):
//! the brand glow behind them, the Pley logo, the status line with its breathing dot, the sweep's
//! bar, a pad key standing on its own, and profiles' colours.

use std::sync::{Arc, Mutex};

use pley_base::{Cmd, Color, Point, Rect, RgbaImage, TextOpts, TextStyle};
use pley_ui::Ui;
use pley_ui::theme;

/// The sign-in screens' background (`OnboardingKit.kt` `brandGlow`): BrandDeep at 18 % glowing from
/// the bottom-right corner, gone by 60 % of the width. The page under it is Bg.
pub fn brand_glow(ui: &mut Ui) {
    let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    let corner = Point::new(theme::CANVAS_W, theme::CANVAS_H);
    ui.radial(full, corner, theme::CANVAS_W * 0.6, theme::BRAND_GLOW, Color::TRANSPARENT);
}

/// One line that is never cut.
pub(crate) fn free_line() -> TextOpts {
    TextOpts { max_width: None, max_lines: 1, ellipsis: false, ..TextOpts::default() }
}

/// The width of a space in `style`: a layout leaves out a line's trailing spaces, so two words set
/// apart by hand need it.
pub(crate) fn space(ui: &Ui, style: &TextStyle) -> f32 {
    (ui.measure("x x", style, &free_line()).w - ui.measure("xx", style, &free_line()).w).max(0.0)
}

/// The logo (`PleyLogo`): the mark on its tile, `height` square, then "Pley" in `Tv.PageTitle` at
/// three quarters of the height, 0.15 × the height after the mark, as the other TV apps draw it. The mark is drawn once as a
/// picture at the screen's pixels and kept.
#[derive(Clone, Debug, Default)]
pub struct Logo {
    picture: Option<(u32, Arc<RgbaImage>, u64)>,
}

/// The gap between the mark and the word, as a share of the logo's height.
const WORD_GAP: f32 = 0.15;

/// The marks drawn so far, by size in pixels, for the whole run: each screen and approval that
/// shows the logo would otherwise draw it again (a hundred thousand samples at 80 px).
static MARKS: Mutex<Vec<(u32, Arc<RgbaImage>)>> = Mutex::new(Vec::new());

/// The mark `px` square, drawn the first time it is asked for.
fn mark_of(px: u32) -> Arc<RgbaImage> {
    let mut marks = MARKS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, image)) = marks.iter().find(|(p, _)| *p == px) {
        return image.clone();
    }
    let image = Arc::new(mark(px));
    marks.push((px, image.clone()));
    image
}

impl Logo {
    /// The wordmark's type at `height`.
    fn wordmark(height: f32) -> TextStyle {
        TextStyle { size: height * 0.75, line_height: height, ..theme::PAGE_TITLE }
    }

    /// How wide the logo is at `height`.
    pub fn width(ui: &Ui, height: f32) -> f32 {
        height * (1.0 + WORD_GAP) + ui.measure("Pley", &Logo::wordmark(height), &free_line()).w
    }

    /// Draws the logo `height` tall with its top-left corner at `at`.
    pub fn show(&mut self, ui: &mut Ui, at: Point, height: f32) {
        let px = ui.image_px(height).max(1);
        let (image, key) = match &self.picture {
            Some((p, image, key)) if *p == px => (image.clone(), *key),
            _ => {
                let image = mark_of(px);
                let key = 0x706c_6579_6d61_726b ^ u64::from(px);
                self.picture = Some((px, image.clone(), key));
                (image, key)
            }
        };
        ui.cmd(Cmd::Raw { rect: Rect::new(at.x, at.y, height, height), image, key, radius: 0.0, opacity: 1.0 });
        let word = Point::new(at.x + height * (1.0 + WORD_GAP), at.y);
        ui.text(word, "Pley", &Logo::wordmark(height), &free_line(), theme::FG);
    }
}

/// The Pley mark (the web's `LogoMark`, a 64-unit square): three posters on a shelf on the
/// `#0a1622` tile, the front one in a light-to-Accent gradient with play cut out of it, as a
/// straight-alpha picture `px` square, each pixel sampled 4 × 4 times for smooth edges.
pub fn mark(px: u32) -> RgbaImage {
    const SAMPLES: u32 = 4;
    let mut image = RgbaImage::new(px, px);
    let unit = 64.0 / px as f32;
    for y in 0..px {
        for x in 0..px {
            let (mut r, mut g, mut b, mut covered) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let u = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * unit;
                    let v = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * unit;
                    if let Some(c) = mark_at(u, v) {
                        r += u32::from(c.r);
                        g += u32::from(c.g);
                        b += u32::from(c.b);
                        covered += 1;
                    }
                }
            }
            if covered > 0 {
                let i = ((y * px + x) * 4) as usize;
                let n = covered;
                image.pixels[i] = (r / n) as u8;
                image.pixels[i + 1] = (g / n) as u8;
                image.pixels[i + 2] = (b / n) as u8;
                image.pixels[i + 3] = (covered * 255 / (SAMPLES * SAMPLES)) as u8;
            }
        }
    }
    image
}

/// The mark's colour at (`u`, `v`) in its 64-unit square, none outside its tile.
fn mark_at(u: f32, v: f32) -> Option<Color> {
    if !in_rounded(u, v, [0.0, 0.0, 64.0, 64.0], 15.0, 15.0) {
        return None;
    }
    // The front poster, less the play shape.
    if in_rounded(u, v, [12.0, 15.5, 34.0, 48.5], 4.0, 4.0) && !in_play(u, v) {
        let (dx, dy) = (34.0 - 12.0, 49.0 - 15.0);
        let t = (((u - 12.0) * dx + (v - 15.0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        return Some(theme::BRAND_LIGHT.lerp(theme::ACCENT, t));
    }
    if in_rounded(u, v, [36.0, 18.5, 44.0, 45.5], 0.0, 3.5) {
        return Some(theme::BRAND_DEEP);
    }
    if in_rounded(u, v, [46.0, 21.5, 52.0, 42.5], 0.0, 3.0) {
        return Some(theme::BRAND_FAR);
    }
    Some(theme::BRAND_TILE)
}

/// Whether (`u`, `v`) is in the rectangle `[x0, y0, x1, y1]` whose left corners are rounded by
/// `left` and right corners by `right`.
fn in_rounded(u: f32, v: f32, [x0, y0, x1, y1]: [f32; 4], left: f32, right: f32) -> bool {
    if u < x0 || u > x1 || v < y0 || v > y1 {
        return false;
    }
    let r = if u < (x0 + x1) / 2.0 { left } else { right };
    if r <= 0.0 {
        return true;
    }
    let cx = u.clamp(x0 + r, x1 - r);
    let cy = v.clamp(y0 + r, y1 - r);
    (u - cx).powi(2) + (v - cy).powi(2) <= r * r
}

/// Whether (`u`, `v`) is in the play shape: a triangle pointing right with corners rounded by 1
/// unit, taken as the hexagon where its straight edges meet the arcs.
fn in_play(u: f32, v: f32) -> bool {
    const PLAY: [(f32, f32); 6] =
        [(19.0, 27.0), (19.0, 37.0), (20.51, 37.86), (29.01, 32.86), (29.01, 31.14), (20.51, 26.14)];
    (0..PLAY.len()).all(|i| {
        let (ax, ay) = PLAY[i];
        let (bx, by) = PLAY[(i + 1) % PLAY.len()];
        // The hexagon runs anticlockwise on screen (y down): inside is to the left of each edge.
        (bx - ax) * (v - ay) - (by - ay) * (u - ax) <= 0.0
    })
}

/// The breathing dot's opacity at `time` (`BreathingDot`): 100, 70, 40 and 70 % in steps of half a
/// second, so a waiting screen draws twice a second only. Its next step comes at the second value.
pub fn breathing(time: f64) -> (f32, f64) {
    const STEPS: [f32; 4] = [1.0, 0.7, 0.4, 0.7];
    let step = (time / 0.5).floor();
    (STEPS[(step as i64).rem_euclid(4) as usize], (step + 1.0) * 0.5)
}

/// A status line (`StatusLine`): the 8 dp Accent breathing dot and 12 dp when `waiting`, then the
/// text in `style` and `color`, at most two lines within `width`. Returns its height.
pub fn status_line(
    ui: &mut Ui,
    at: Point,
    width: f32,
    text: &str,
    style: &TextStyle,
    color: Color,
    waiting: bool,
) -> f32 {
    let mut x = at.x;
    if waiting {
        let now = ui.time();
        let (alpha, next) = breathing(now);
        let dot = Rect::new(x, at.y + (style.line_height - 8.0) / 2.0, 8.0, 8.0);
        ui.fill(dot, theme::ACCENT.alpha(alpha), 4.0);
        ui.repaint_after(next - now);
        x += 8.0 + 12.0;
    }
    let w = (at.x + width - x).max(1.0);
    ui.text(Point::new(x, at.y), text, style, &TextOpts::wrap(w, 2), color).h
}

/// The sweep's 160 × 4 bar (`SweepBar`): a Surface4 well filling with Accent as `progress` goes
/// from 0 to 1.
pub fn sweep_bar(ui: &mut Ui, at: Point, progress: f32) {
    let well = Rect::new(at.x, at.y, 160.0, 4.0);
    ui.fill(well, theme::SURFACE4, 2.0);
    let p = progress.clamp(0.0, 1.0);
    if p > 0.0 {
        ui.fill(Rect::new(at.x, at.y, 160.0 * p, 4.0), theme::ACCENT, 2.0);
    }
}

/// A profile's own colour when the server gave one as `#rgb` or `#rrggbb`.
pub fn parse_color(color: Option<&str>) -> Option<Color> {
    let hex = color?.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let full = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16).ok().map(Color::rgb)
}

/// The colour a profile's avatar is drawn on (`ProfileColors.of`): its own, else the one its name
/// gives.
pub fn profile_fill(name: &str, color: Option<&str>) -> Color {
    parse_color(color).unwrap_or_else(|| theme::profile_color(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_read_in_both_forms() {
        assert_eq!(parse_color(Some("#38bdf8")), Some(Color::rgb(0x38bdf8)));
        assert_eq!(parse_color(Some("#a3e")), Some(Color::rgb(0xaa33ee)));
        assert_eq!(parse_color(Some("38bdf8")), None);
        assert_eq!(parse_color(Some("#38bdf")), None);
        assert_eq!(parse_color(Some("#zzzzzz")), None);
        assert_eq!(parse_color(None), None);
        assert_eq!(profile_fill("Sam", Some("#a3e635")), Color::rgb(0xa3e635));
        assert_eq!(profile_fill("Sam", None), theme::profile_color("Sam"));
    }

    #[test]
    fn the_dot_breathes_in_half_second_steps() {
        assert_eq!(breathing(0.1), (1.0, 0.5));
        assert_eq!(breathing(0.6), (0.7, 1.0));
        assert_eq!(breathing(1.2), (0.4, 1.5));
        assert_eq!(breathing(1.7), (0.7, 2.0));
        assert_eq!(breathing(2.0).0, 1.0);
    }

    #[test]
    fn the_mark_has_its_tile_posters_and_play_cut_out() {
        let m = mark(64);
        let at = |u: u32, v: u32| {
            let i = ((v * 64 + u) * 4) as usize;
            [m.pixels[i], m.pixels[i + 1], m.pixels[i + 2], m.pixels[i + 3]]
        };
        // The tile's rounded corner is clear; its middle is the tile's colour.
        assert_eq!(at(0, 0)[3], 0);
        assert_eq!(at(5, 32), [0x0a, 0x16, 0x22, 255]);
        // The back posters.
        assert_eq!(at(49, 32), [0x0c, 0x4a, 0x6e, 255]);
        assert_eq!(at(40, 32), [0x02, 0x84, 0xc7, 255]);
        // The front poster around the play shape, and the tile showing through it.
        assert_eq!(at(14, 20)[3], 255);
        assert!(at(14, 20)[2] > 0xf0);
        assert_eq!(at(22, 32), [0x0a, 0x16, 0x22, 255]);
        assert!(in_play(20.0, 32.0) && !in_play(30.0, 32.0) && !in_play(19.5, 26.0));
    }
}
