//! A profile's avatar without its picture (docs/THEMES.md, "Profile colors"): a circle in the
//! profile's color with its initial on it, by the rule every app shares and
//! packages/tv/test/fixtures/avatar.json pins. These are the profile's colors, not the theme's, so
//! they stay under every theme and accent. `pley_kit::ProfileTile` draws every avatar by them: the
//! rail's, Settings' and "Who's watching?".

use pley_base::Color;

/// The colors offered for profiles, in the order the server and every app use; also what a
/// profile without a color of its own takes, picked from its name ([`color`]).
pub const COLORS: [Color; 8] = [
    Color::rgb(0x38bdf8),
    Color::rgb(0xa78bfa),
    Color::rgb(0xf472b6),
    Color::rgb(0xfb923c),
    Color::rgb(0xfacc15),
    Color::rgb(0x4ade80),
    Color::rgb(0x2dd4bf),
    Color::rgb(0xf87171),
];

/// The initial's two colors: Midnight's background on light circles, white on dark ones.
pub const INK_DARK: Color = Color::rgb(0x0b0d10);
pub const INK_LIGHT: Color = Color::rgb(0xffffff);

/// A color the server keeps for a profile, `#rgb` or `#rrggbb` in any case; none for anything
/// else.
pub fn own_color(color: Option<&str>) -> Option<Color> {
    let hex = color?.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let full = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_string(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16).ok().map(Color::rgb)
}

/// The circle's color for a profile named `name` whose own color is `own` (`avatar_color`, as the
/// server gives it): that color ([`own_color`]), else `COLORS[h % 8]`, `h` folded over the name's
/// code points from 0 as `h = h·31 + c`, wrapping at 32 bits, so a profile has the same color on
/// every screen and every app.
pub fn color(name: &str, own: Option<&str>) -> Color {
    own_color(own).unwrap_or_else(|| {
        let h = name.chars().fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
        COLORS[(h % COLORS.len() as u32) as usize]
    })
}

/// The initial's color on a `circle`: whichever of [`INK_DARK`] and [`INK_LIGHT`] has the higher
/// WCAG contrast with it, the dark one on a tie.
pub fn ink(circle: Color) -> Color {
    let on = luminance(circle) + 0.05;
    let dark = on / (luminance(INK_DARK) + 0.05);
    let light = (luminance(INK_LIGHT) + 0.05) / on;
    if dark >= light { INK_DARK } else { INK_LIGHT }
}

/// The name's first letter in capitals, `?` for a blank name.
pub fn initial(name: &str) -> String {
    name.trim().chars().next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect())
}

/// WCAG's relative luminance of `c`.
fn luminance(c: Color) -> f64 {
    let linear = |v: u8| {
        let c = f64::from(v) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// packages/tv/test/fixtures/avatar.json, which every app's tests read: the rule, the eight
    /// colors, the two inks and the cases.
    const SHARED: &str = include_str!("../../../../../packages/tv/test/fixtures/avatar.json");

    fn hex(v: &serde_json::Value) -> Color {
        own_color(v.as_str()).unwrap_or_else(|| panic!("{v} is no color"))
    }

    #[test]
    fn every_avatar_follows_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(SHARED).unwrap();
        let colors: Vec<Color> = fixture["colors"].as_array().unwrap().iter().map(hex).collect();
        assert_eq!(colors, COLORS);
        assert_eq!((hex(&fixture["ink"]["dark"]), hex(&fixture["ink"]["light"])), (INK_DARK, INK_LIGHT));
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 22);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let circle = color(name, case["avatar_color"].as_str());
            assert_eq!(circle, hex(&case["color"]), "{case}");
            assert_eq!(ink(circle), hex(&case["ink"]), "{case}");
            assert_eq!(initial(name), case["initial"].as_str().unwrap(), "{case}");
        }
    }

    #[test]
    fn only_a_hex_color_after_a_hash_is_the_profiles() {
        assert_eq!(own_color(Some("#a3e635")), Some(Color::rgb(0xa3e635)));
        assert_eq!(own_color(Some("#A3E")), Some(Color::rgb(0xaa33ee)));
        for not in ["38bdf8", "#zzzzzz", "#+12345", "#38bdf8 ", " #38bdf8", "#", ""] {
            assert_eq!(own_color(Some(not)), None, "{not:?}");
        }
        assert_eq!(own_color(None), None);
    }
}
