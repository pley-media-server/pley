//! Pley's themes and accents (docs/THEMES.md), as every app draws them: each theme's colors, its
//! kind, its dark twin and the colors an accent replaces in it, read from
//! `packages/themes/themes.json`, which the web app writes. A color is the accent's, from its set
//! for the theme's kind, when the theme follows that token, and the theme's own otherwise (the
//! web's `colorOf`).
//!
//! Each of the 63 palettes (nine themes, each in its own accent and the six others) is made once,
//! the first time one is asked for, and kept for the whole run. The palette shown is the
//! interface's thread's ([`show`]), read by every color of [`crate::theme`] with one load; a test's
//! thread shows its own, Midnight until it picks another.

use std::cell::Cell;
use std::sync::OnceLock;

use pley_base::Color;
use serde_json::Value;

/// The themes and accents, as the web app writes them.
const SPEC: &str = include_str!("../../../../../packages/themes/themes.json");

/// A theme, in the order the picker shows them (`THEMES` in `apps/web/src/theme/index.ts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Theme {
    #[default]
    Midnight,
    Midday,
    Twilight,
    Starlight,
    Moonlight,
    Nightshade,
    Greenlight,
    Sunshine,
    Blacklight,
}

impl Theme {
    /// Every theme, in the picker's order.
    pub const ALL: [Theme; 9] = [
        Theme::Midnight,
        Theme::Midday,
        Theme::Twilight,
        Theme::Starlight,
        Theme::Moonlight,
        Theme::Nightshade,
        Theme::Greenlight,
        Theme::Sunshine,
        Theme::Blacklight,
    ];

    /// Its id, as kept and in themes.json.
    pub fn id(self) -> &'static str {
        match self {
            Theme::Midnight => "midnight",
            Theme::Midday => "midday",
            Theme::Twilight => "twilight",
            Theme::Starlight => "starlight",
            Theme::Moonlight => "moonlight",
            Theme::Nightshade => "nightshade",
            Theme::Greenlight => "greenlight",
            Theme::Sunshine => "sunshine",
            Theme::Blacklight => "blacklight",
        }
    }

    /// The theme kept as `id`: Midnight for none, or for one this app doesn't know.
    pub fn from_id(id: Option<&str>) -> Theme {
        Theme::ALL.into_iter().find(|t| Some(t.id()) == id).unwrap_or_default()
    }

    /// Whether it is a light theme.
    pub fn light(self) -> bool {
        Palette::of(self, None).light
    }

    fn index(self) -> usize {
        Theme::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// An accent picked over a theme's own, in the picker's order after Default (`ACCENTS` in
/// `apps/web/src/theme/index.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accent {
    Sky,
    Violet,
    Miami,
    Gold,
    Mint,
    Amber,
}

impl Accent {
    /// Every accent, in the picker's order.
    pub const ALL: [Accent; 6] =
        [Accent::Sky, Accent::Violet, Accent::Miami, Accent::Gold, Accent::Mint, Accent::Amber];

    /// Its id, as kept and in themes.json.
    pub fn id(self) -> &'static str {
        match self {
            Accent::Sky => "sky",
            Accent::Violet => "violet",
            Accent::Miami => "miami",
            Accent::Gold => "gold",
            Accent::Mint => "mint",
            Accent::Amber => "amber",
        }
    }

    /// The accent kept as `id`: none (the theme's own) for none, or for one this app doesn't
    /// know.
    pub fn from_id(id: Option<&str>) -> Option<Accent> {
        Accent::ALL.into_iter().find(|a| Some(a.id()) == id)
    }

    fn index(self) -> usize {
        Accent::ALL.iter().position(|a| *a == self).unwrap_or(0)
    }
}

/// A theme's colors under an accent, by the tokens of docs/THEMES.md.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub theme: Theme,
    /// The accent picked; none: the theme's own.
    pub picked: Option<Accent>,
    /// A light theme: its players draw in [`Palette::twin`]'s colors.
    pub light: bool,
    /// The dark theme a light one's players draw in; a dark theme's own.
    pub twin: Theme,
    pub bg: Color,
    pub surface: Color,
    pub surface2: Color,
    pub surface3: Color,
    pub border: Color,
    pub border_strong: Color,
    pub fg: Color,
    pub muted: Color,
    pub subtle: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_fg: Color,
    pub success: Color,
    pub danger: Color,
    pub danger_fg: Color,
    pub warning: Color,
    pub info: Color,
    pub shade: Color,
    pub brand_light: Color,
    pub brand: Color,
    pub brand_deep: Color,
    pub brand_far: Color,
    pub brand_tile: Color,
}

/// Every token, by its name in themes.json.
#[cfg(test)]
const TOKENS: [&str; 23] = [
    "bg",
    "surface",
    "surface-2",
    "surface-3",
    "border",
    "border-strong",
    "fg",
    "muted",
    "subtle",
    "accent",
    "accent-hover",
    "accent-fg",
    "success",
    "danger",
    "danger-fg",
    "warning",
    "info",
    "shade",
    "brand-light",
    "brand",
    "brand-deep",
    "brand-far",
    "brand-tile",
];

impl Palette {
    /// `theme`'s colors under `accent` (none: its own).
    pub fn of(theme: Theme, accent: Option<Accent>) -> &'static Palette {
        let all = PALETTES.get_or_init(read);
        &all[theme.index() * SLOTS + accent.map_or(0, |a| a.index() + 1)]
    }

    /// The colors a player draws in: a light theme's dark twin's, under the same accent; a dark
    /// theme's own.
    pub fn for_video(&'static self) -> &'static Palette {
        if self.light { Palette::of(self.twin, self.picked) } else { self }
    }

    /// The color of the token named `name` in themes.json.
    pub fn token(&self, name: &str) -> Option<Color> {
        Some(match name {
            "bg" => self.bg,
            "surface" => self.surface,
            "surface-2" => self.surface2,
            "surface-3" => self.surface3,
            "border" => self.border,
            "border-strong" => self.border_strong,
            "fg" => self.fg,
            "muted" => self.muted,
            "subtle" => self.subtle,
            "accent" => self.accent,
            "accent-hover" => self.accent_hover,
            "accent-fg" => self.accent_fg,
            "success" => self.success,
            "danger" => self.danger,
            "danger-fg" => self.danger_fg,
            "warning" => self.warning,
            "info" => self.info,
            "shade" => self.shade,
            "brand-light" => self.brand_light,
            "brand" => self.brand,
            "brand-deep" => self.brand_deep,
            "brand-far" => self.brand_far,
            "brand-tile" => self.brand_tile,
            _ => return None,
        })
    }

    /// A key that differs between palettes of different logo colors, for pictures drawn in them.
    pub fn logo_key(&self) -> u64 {
        [self.brand_light, self.brand, self.brand_deep, self.brand_far, self.brand_tile]
            .iter()
            .fold(0xcbf2_9ce4_8422_2325u64, |h, c| {
                (h ^ u64::from(u32::from_be_bytes([c.r, c.g, c.b, c.a]))).wrapping_mul(0x0100_0000_01b3)
            })
    }
}

/// A theme's palettes: its own accent, then each of [`Accent::ALL`].
const SLOTS: usize = 1 + Accent::ALL.len();

static PALETTES: OnceLock<Vec<Palette>> = OnceLock::new();

/// The 63 palettes from themes.json, theme by theme. The file is built in and its tests read it,
/// so a theme or token it lacks stops the tests, not a TV.
fn read() -> Vec<Palette> {
    let spec: Value = serde_json::from_str(SPEC).expect("themes.json is JSON");
    let find = |list: &str, id: &str| -> Value {
        spec[list]
            .as_array()
            .and_then(|all| all.iter().find(|t| t["id"] == id))
            .cloned()
            .unwrap_or_else(|| panic!("themes.json has no {list} entry `{id}`"))
    };
    let mut out = Vec::with_capacity(Theme::ALL.len() * SLOTS);
    for theme in Theme::ALL {
        let t = find("themes", theme.id());
        let light = t["kind"] == "light";
        let twin = if light { Theme::from_id(t["twin"].as_str()) } else { theme };
        let follows: Vec<&str> = t["follows"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let accents = std::iter::once(None).chain(Accent::ALL.into_iter().map(Some));
        for accent in accents {
            let set = accent.map(|a| find("accents", a.id())[if light { "light" } else { "dark" }].clone());
            let color = |name: &str| -> Color {
                let own = || t["colors"][name].as_str();
                let picked = set.as_ref().filter(|_| follows.contains(&name)).and_then(|s| s[name].as_str());
                picked
                    .or_else(own)
                    .and_then(parse_color)
                    .unwrap_or_else(|| panic!("themes.json: {}'s {name} isn't a color", theme.id()))
            };
            out.push(Palette {
                theme,
                picked: accent,
                light,
                twin,
                bg: color("bg"),
                surface: color("surface"),
                surface2: color("surface-2"),
                surface3: color("surface-3"),
                border: color("border"),
                border_strong: color("border-strong"),
                fg: color("fg"),
                muted: color("muted"),
                subtle: color("subtle"),
                accent: color("accent"),
                accent_hover: color("accent-hover"),
                accent_fg: color("accent-fg"),
                success: color("success"),
                danger: color("danger"),
                danger_fg: color("danger-fg"),
                warning: color("warning"),
                info: color("info"),
                shade: color("shade"),
                brand_light: color("brand-light"),
                brand: color("brand"),
                brand_deep: color("brand-deep"),
                brand_far: color("brand-far"),
                brand_tile: color("brand-tile"),
            });
        }
    }
    out
}

/// A color as themes.json writes it: `#rrggbb`, or `rgb(r g b / a)` for a shade with its own
/// opacity.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok().map(Color::rgb)).flatten();
    }
    let inner = s.strip_prefix("rgb(")?.strip_suffix(')')?;
    let (rgb, alpha) = match inner.split_once('/') {
        Some((rgb, a)) => (rgb, a.trim().parse::<f32>().ok()?),
        None => (inner, 1.0),
    };
    let mut channels = rgb.split_whitespace().map(|c| c.parse::<u8>().ok());
    let (r, g, b) = (channels.next()??, channels.next()??, channels.next()??);
    if channels.next().is_some() || !(0.0..=1.0).contains(&alpha) {
        return None;
    }
    Some(Color { r, g, b, a: (alpha * 255.0).round() as u8 })
}

thread_local! {
    static SHOWN: Cell<Option<&'static Palette>> = const { Cell::new(None) };
}

/// The palette shown on this thread: the one [`show`] last gave, else Midnight's own.
pub fn palette() -> &'static Palette {
    SHOWN.with(Cell::get).unwrap_or_else(|| Palette::of(Theme::Midnight, None))
}

/// Shows `palette` on this thread from now on: the shell gives it at the start of each frame,
/// before anything draws, so a pick changes every color in the same frame.
pub fn show(palette: &'static Palette) {
    SHOWN.with(|s| s.set(Some(palette)));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web's `colorOf`, from the file as it is.
    fn color_of(spec: &Value, theme: &str, accent: Option<&str>, name: &str) -> String {
        let t = spec["themes"].as_array().unwrap().iter().find(|t| t["id"] == theme).unwrap();
        let kind = t["kind"].as_str().unwrap();
        let follows = t["follows"].as_array().unwrap().iter().any(|f| f == name);
        let picked = accent
            .and_then(|a| spec["accents"].as_array().unwrap().iter().find(|x| x["id"] == a))
            .and_then(|a| a[kind][name].as_str())
            .filter(|_| follows);
        picked.or_else(|| t["colors"][name].as_str()).unwrap().to_string()
    }

    /// Every color of every theme under every accent is the file's, so the contrast the web's
    /// `palettes.test.ts` holds the file to holds on the TV.
    #[test]
    fn every_palette_is_themes_json() {
        let spec: Value = serde_json::from_str(SPEC).unwrap();
        let ids: Vec<&str> = spec["themes"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
        assert_eq!(ids, Theme::ALL.map(Theme::id), "the themes and their order");
        let ids: Vec<&str> = spec["accents"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
        assert_eq!(ids, Accent::ALL.map(Accent::id), "the accents and their order");
        let mut checked = 0;
        for theme in Theme::ALL {
            let t = spec["themes"].as_array().unwrap().iter().find(|t| t["id"] == theme.id()).unwrap();
            let names: Vec<&String> = t["colors"].as_object().unwrap().keys().collect();
            assert_eq!(names.len(), TOKENS.len(), "{}: every token, no other", theme.id());
            let light = t["kind"] == "light";
            for accent in std::iter::once(None).chain(Accent::ALL.map(Some)) {
                let p = Palette::of(theme, accent);
                assert_eq!((p.theme, p.picked, p.light), (theme, accent, light));
                for name in TOKENS {
                    let want = color_of(&spec, theme.id(), accent.map(Accent::id), name);
                    let got = p.token(name).unwrap();
                    assert_eq!(Some(got), parse_color(&want), "{} under {accent:?}: {name} is {want}", theme.id());
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 9 * 7 * 23);
    }

    #[test]
    fn light_themes_play_in_their_dark_twins() {
        let spec: Value = serde_json::from_str(SPEC).unwrap();
        for theme in Theme::ALL {
            let t = spec["themes"].as_array().unwrap().iter().find(|t| t["id"] == theme.id()).unwrap();
            for accent in std::iter::once(None).chain(Accent::ALL.map(Some)) {
                let video = Palette::of(theme, accent).for_video();
                assert!(!video.light, "{}", theme.id());
                assert_eq!(video.picked, accent);
                match t["twin"].as_str() {
                    Some(twin) => assert_eq!(video.theme.id(), twin),
                    None => assert_eq!(video.theme, theme),
                }
            }
        }
        assert_eq!(
            Palette::of(Theme::Sunshine, Some(Accent::Gold)).for_video(),
            Palette::of(Theme::Midnight, Some(Accent::Gold))
        );
    }

    #[test]
    fn ids_this_app_doesnt_know_show_midnight_and_the_themes_own() {
        assert_eq!(Theme::from_id(Some("sunshine")), Theme::Sunshine);
        assert_eq!(Theme::from_id(Some("daylight")), Theme::Midnight);
        assert_eq!(Theme::from_id(None), Theme::Midnight);
        assert_eq!(Accent::from_id(Some("mint")), Some(Accent::Mint));
        assert_eq!(Accent::from_id(Some("teal")), None);
        assert_eq!(Accent::from_id(None), None);
    }

    #[test]
    fn colors_in_both_forms() {
        assert_eq!(parse_color("#38bdf8"), Some(Color::rgb(0x38bdf8)));
        assert_eq!(parse_color("rgb(16 26 44 / 0.42)"), Some(Color { r: 16, g: 26, b: 44, a: 107 }));
        assert_eq!(parse_color("rgb(1 2 3)"), Some(Color::rgb(0x010203)));
        assert_eq!(parse_color("#38bdf"), None);
        assert_eq!(parse_color("rgb(1 2 / 0.5)"), None);
    }

    #[test]
    fn each_thread_shows_its_own() {
        assert_eq!(palette(), Palette::of(Theme::Midnight, None));
        show(Palette::of(Theme::Sunshine, Some(Accent::Mint)));
        assert_eq!(palette().theme, Theme::Sunshine);
        std::thread::spawn(|| assert_eq!(palette().theme, Theme::Midnight)).join().unwrap();
        show(Palette::of(Theme::Midnight, None));
    }

    #[test]
    fn logos_of_other_colors_have_other_keys() {
        let midnight = Palette::of(Theme::Midnight, None).logo_key();
        assert_eq!(midnight, Palette::of(Theme::Midnight, Some(Accent::Sky)).logo_key());
        assert_ne!(midnight, Palette::of(Theme::Midnight, Some(Accent::Gold)).logo_key());
        // Sunshine keeps its own logo under any accent.
        assert_eq!(
            Palette::of(Theme::Sunshine, None).logo_key(),
            Palette::of(Theme::Sunshine, Some(Accent::Amber)).logo_key()
        );
    }
}
