//! The theme and accent the server's pages show (docs/THEMES.md), for what
//! the app draws itself around them: the window's background before a page
//! draws, its title bar and its icon. The server's pages tell the app what
//! they show (`theme_picked` in `lib.rs`): a profile's pick, or Midnight on
//! the screens outside a profile (signing in, "Who's watching?"). The
//! app's own pages (the picker) are outside any profile too, so the window
//! is Midnight in its own accent while they show, as they are (style.css).
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use tauri::image::Image;
use tauri::window::Color;

/// Every app's themes and accents, as the web app writes them.
const THEMES: &str = include_str!("../../../../packages/themes/themes.json");

#[derive(Deserialize)]
struct Spec {
    themes: Vec<ThemeSpec>,
    accents: Vec<AccentSpec>,
}

#[derive(Deserialize)]
struct ThemeSpec {
    id: String,
    kind: Kind,
    colors: HashMap<String, String>,
    /// The tokens an accent picked replaces.
    follows: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Dark,
    Light,
}

#[derive(Deserialize)]
struct AccentSpec {
    id: String,
    dark: HashMap<String, String>,
    light: HashMap<String, String>,
}

fn spec() -> &'static Spec {
    static SPEC: OnceLock<Spec> = OnceLock::new();
    SPEC.get_or_init(|| serde_json::from_str(THEMES).expect("packages/themes/themes.json"))
}

/// Pley's own theme, outside a profile and for a profile that picked none.
const MIDNIGHT: &str = "midnight";

/// A theme and an accent over its own (none for the theme's own), both
/// known: an id the app doesn't know is Midnight, or the theme's own accent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pick {
    theme: String,
    accent: Option<String>,
}

impl Default for Pick {
    fn default() -> Self {
        Pick { theme: MIDNIGHT.into(), accent: None }
    }
}

impl Pick {
    pub(crate) fn new(theme: &str, accent: Option<&str>) -> Self {
        let spec = spec();
        let theme = if spec.themes.iter().any(|t| t.id == theme) { theme } else { MIDNIGHT };
        let accent = accent.filter(|a| spec.accents.iter().any(|known| known.id == *a));
        Pick { theme: theme.into(), accent: accent.map(Into::into) }
    }

    fn spec(&self) -> &'static ThemeSpec {
        let themes = &spec().themes;
        themes.iter().find(|t| t.id == self.theme).unwrap_or(&themes[0])
    }

    pub(crate) fn kind(&self) -> Kind {
        self.spec().kind
    }

    /// Pley's own look: the app's icon as it ships.
    pub(crate) fn is_own(&self) -> bool {
        self.theme == MIDNIGHT && self.accent.is_none()
    }

    /// A token's color: the accent's when the theme follows it, as every app
    /// draws it (`colorOf` in apps/web/src/theme/spec.ts).
    pub(crate) fn color(&self, token: &str) -> Option<&'static str> {
        let theme = self.spec();
        let accent = self.accent.as_ref().and_then(|id| spec().accents.iter().find(|a| &a.id == id));
        let from_accent = accent.filter(|_| theme.follows.iter().any(|t| t == token)).and_then(|a| {
            match theme.kind {
                Kind::Dark => &a.dark,
                Kind::Light => &a.light,
            }
            .get(token)
        });
        from_accent.or_else(|| theme.colors.get(token)).map(String::as_str)
    }

    fn rgb(&self, token: &str) -> [f32; 3] {
        let hex = self.color(token).and_then(|c| c.strip_prefix('#')).unwrap_or("000000");
        let channel = |i: usize| hex.get(i..i + 2).and_then(|c| u8::from_str_radix(c, 16).ok()).unwrap_or(0);
        [channel(0) as f32, channel(2) as f32, channel(4) as f32]
    }

    /// The page's background, for the window before a page draws.
    pub(crate) fn background(&self) -> Color {
        let [r, g, b] = self.rgb("bg");
        Color(r as u8, g as u8, b as u8, 255)
    }

    /// The logo on its tile, `size` pixels square, in the pick's colors, as
    /// RGBA: the window's icon. The shapes are the web's
    /// (apps/web/src/components/Logo.tsx), on its 64 × 64 grid, each pixel
    /// the average of 4 × 4 points in it.
    pub(crate) fn logo(&self, size: u32) -> Image<'static> {
        const POINTS: u32 = 4;
        let [tile, far, deep, light, brand] =
            ["brand-tile", "brand-far", "brand-deep", "brand-light", "brand"].map(|token| self.rgb(token));
        let paint = |x: f32, y: f32| -> Option<[f32; 3]> {
            if !in_rect(x, y, [0.0, 0.0, 64.0, 64.0], 15.0, 15.0) {
                return None;
            }
            if in_rect(x, y, [12.0, 15.5, 34.0, 48.5], 4.0, 4.0) && !in_play(x, y) {
                // From brand-light at (12, 15) to brand at (34, 49).
                let t = (((x - 12.0) * 22.0 + (y - 15.0) * 34.0) / (22.0 * 22.0 + 34.0 * 34.0)).clamp(0.0, 1.0);
                return Some([0, 1, 2].map(|i| light[i] + (brand[i] - light[i]) * t));
            }
            if in_rect(x, y, [36.0, 18.5, 44.0, 45.5], 0.0, 3.5) {
                return Some(deep);
            }
            if in_rect(x, y, [46.0, 21.5, 52.0, 42.5], 0.0, 3.0) {
                return Some(far);
            }
            Some(tile)
        };
        let scale = 64.0 / size as f32;
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for py in 0..size {
            for px in 0..size {
                let (mut sum, mut covered) = ([0.0f32; 3], 0.0f32);
                for sy in 0..POINTS {
                    for sx in 0..POINTS {
                        let x = (px as f32 + (sx as f32 + 0.5) / POINTS as f32) * scale;
                        let y = (py as f32 + (sy as f32 + 0.5) / POINTS as f32) * scale;
                        if let Some(color) = paint(x, y) {
                            (0..3).for_each(|i| sum[i] += color[i]);
                            covered += 1.0;
                        }
                    }
                }
                if covered == 0.0 {
                    rgba.extend([0, 0, 0, 0]);
                } else {
                    let alpha = covered / (POINTS * POINTS) as f32 * 255.0;
                    rgba.extend([sum[0] / covered, sum[1] / covered, sum[2] / covered, alpha].map(|c| c.round() as u8));
                }
            }
        }
        Image::new_owned(rgba, size, size)
    }
}

/// Whether (x, y) is in the rectangle `[left, top, right, bottom]`, whose
/// left corners are rounded by `left_r` and right ones by `right_r`.
fn in_rect(x: f32, y: f32, [left, top, right, bottom]: [f32; 4], left_r: f32, right_r: f32) -> bool {
    if x < left || x > right || y < top || y > bottom {
        return false;
    }
    let (r, cx) = if x < left + left_r {
        (left_r, left + left_r)
    } else if x > right - right_r {
        (right_r, right - right_r)
    } else {
        return true;
    };
    let cy = y.clamp(top + r, bottom - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// Whether (x, y) is in the play shape cut out of the front poster: the
/// triangle (20, 27), (20, 37), (28.5, 32) grown by 1, its corners round.
fn in_play(x: f32, y: f32) -> bool {
    const A: (f32, f32) = (20.0, 27.0);
    const B: (f32, f32) = (20.0, 37.0);
    const C: (f32, f32) = (28.5, 32.0);
    let cross = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| (bx - ax) * (y - ay) - (by - ay) * (x - ax);
    let sides = [cross(A, B), cross(B, C), cross(C, A)];
    if sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0) {
        return true;
    }
    let near = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        (x - ax - t * dx).powi(2) + (y - ay - t * dy).powi(2) <= 1.0
    };
    near(A, B) || near(B, C) || near(C, A)
}

/// What the window is drawn in now: Midnight in its own accent until a
/// server's pages say otherwise, and again whenever the picker shows.
#[derive(Default)]
pub(crate) struct Shown(Mutex<Pick>);

impl Shown {
    /// Notes that the window shows `pick`; false when it did already.
    pub(crate) fn change(&self, pick: Pick) -> bool {
        let mut shown = self.0.lock().unwrap();
        if *shown == pick {
            return false;
        }
        *shown = pick;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn json() -> serde_json::Value {
        serde_json::from_str(THEMES).unwrap()
    }

    #[test]
    fn every_theme_and_accent_draws_the_files_colors() {
        let json = json();
        let accents: Vec<Option<&str>> =
            std::iter::once(None).chain(json["accents"].as_array().unwrap().iter().map(|a| a["id"].as_str())).collect();
        let mut checked = 0;
        for theme in json["themes"].as_array().unwrap() {
            let id = theme["id"].as_str().unwrap();
            let kind = theme["kind"].as_str().unwrap();
            let follows: Vec<&str> = theme["follows"].as_array().unwrap().iter().filter_map(|t| t.as_str()).collect();
            for accent in &accents {
                let pick = Pick::new(id, *accent);
                assert_eq!(pick, Pick { theme: id.into(), accent: accent.map(Into::into) });
                let set = accent
                    .map(|a| &json["accents"].as_array().unwrap().iter().find(|known| known["id"] == a).unwrap()[kind]);
                for (token, own) in theme["colors"].as_object().unwrap() {
                    let from_accent = set.filter(|_| follows.contains(&token.as_str())).map(|set| &set[token]);
                    let want = from_accent.and_then(|c| c.as_str()).or(own.as_str());
                    assert_eq!(pick.color(token), want, "{id} with {accent:?}: {token}");
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 9 * 7 * 23, "nine themes, six accents and the theme's own, 23 tokens");
    }

    #[test]
    fn an_accent_colors_only_what_the_theme_follows() {
        assert_eq!(Pick::default().color("brand"), Some("#38bdf8"));
        assert_eq!(Pick::new("midnight", Some("mint")).color("brand"), Some("#5fd39a"));
        assert_eq!(Pick::new("midnight", Some("mint")).color("success"), Some("#3cc2a6"));
        assert_eq!(Pick::new("midnight", Some("mint")).color("brand-tile"), Some("#0a1622"));
        // Sunshine keeps its toy-box logo, Blacklight its gray states.
        assert_eq!(Pick::new("sunshine", Some("violet")).color("brand"), Some("#ff8a1f"));
        assert_eq!(Pick::new("sunshine", Some("violet")).color("accent"), Some("#5a2ff0"));
        assert_eq!(Pick::new("blacklight", Some("mint")).color("success"), Some("#e0e0e0"));
        assert_eq!(Pick::new("blacklight", Some("mint")).color("accent"), Some("#5fd39a"));
        assert_eq!(Pick::new("midday", Some("sky")).color("accent"), Some("#025d8f"));
    }

    #[test]
    fn an_unknown_id_is_midnight_or_the_themes_own() {
        assert_eq!(Pick::new("dusk", Some("violet")), Pick::new("midnight", Some("violet")));
        assert_eq!(Pick::new("sunshine", Some("teal")), Pick::new("sunshine", None));
        assert_eq!(Pick::new("", None), Pick::default());
        assert!(Pick::default().is_own());
        assert!(!Pick::new("midnight", Some("sky")).is_own());
        assert_eq!(Pick::new("sunshine", None).kind(), Kind::Light);
        assert_eq!(Pick::new("blacklight", None).background(), Color(0, 0, 0, 255));
    }

    #[test]
    fn the_icon_is_the_logo_in_the_picks_colors() {
        let pick = Pick::new("twilight", Some("violet"));
        let icon = pick.logo(64);
        assert_eq!((icon.width(), icon.height()), (64, 64));
        let at = |x: usize, y: usize| <[u8; 4]>::try_from(&icon.rgba()[(y * 64 + x) * 4..][..4]).unwrap();
        let rgb = |token: &str| {
            let [r, g, b] = pick.rgb(token);
            [r as u8, g as u8, b as u8, 255]
        };
        // The tile's round corner is clear, its middle the tile's color.
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
        assert_eq!(at(32, 4), rgb("brand-tile"));
        // The play shape is cut out of the front poster, which goes from
        // brand-light at (12, 15) to brand at (34, 49).
        assert_eq!(at(22, 32), rgb("brand-tile"));
        let gradient = |x: f32, y: f32| {
            let t = ((x - 12.0) * 22.0 + (y - 15.0) * 34.0) / (22.0 * 22.0 + 34.0 * 34.0);
            let (light, brand) = (pick.rgb("brand-light"), pick.rgb("brand"));
            [0, 1, 2].map(|i| (light[i] + (brand[i] - light[i]) * t).round() as u8)
        };
        for (x, y) in [(16, 20), (30, 44)] {
            let [r, g, b, a] = at(x, y);
            assert_eq!(([r, g, b], a), (gradient(x as f32 + 0.5, y as f32 + 0.5), 255), "({x}, {y})");
        }
        assert_ne!(gradient(16.5, 20.5), gradient(30.5, 44.5));
        assert_eq!(at(40, 32), rgb("brand-deep"));
        assert_eq!(at(48, 32), rgb("brand-far"));
    }

    #[test]
    fn the_window_is_midnight_outside_a_profile_and_the_profiles_pick_inside() {
        let shown = Shown::default();
        // At start, on the picker: Midnight in its own accent, as it ships.
        assert!(!shown.change(Pick::default()));
        // A profile's pick, told by the server's pages.
        assert!(shown.change(Pick::new("sunshine", Some("violet"))));
        assert!(!shown.change(Pick::new("sunshine", Some("violet"))));
        // Back to the picker: Midnight, whatever the profile picked.
        assert!(shown.change(Pick::default()));
        // The server's pages again: the profile's pick comes back.
        assert!(shown.change(Pick::new("sunshine", Some("violet"))));
    }
}
