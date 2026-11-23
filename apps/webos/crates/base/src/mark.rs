//! Pley's mark, as a placeholder shows it once no picture will come: the logo's three posters in
//! white, each lighter than the one behind, a glyph cut out of the front one for the title's kind
//! of library. packages/placeholder-mark/marks.json defines it for every app; this is the LG app's
//! copy, which a test holds to that file. `pley-gfx` rasterises it, `pley-kit` sizes it.

/// The side of the square the paths are on (the logo's).
pub const GRID: f32 = 64.0;
/// The mark's side, as a share of its box's shorter side, alone and over a title.
pub const SHARE: f32 = 0.4;
pub const SHARE_OVER_TITLE: f32 = 0.26;
/// The mark's largest side, and its gap to a title under it, in dp.
pub const MAX: f32 = 120.0;
pub const GAP: f32 = 10.0;

/// The posters back to front, each with the share of the tint it is filled with. The glyph is cut
/// out of the last.
pub const POSTERS: [(&str, f32); 3] = [
    ("M46 21.5L49 21.5C50.66 21.5 52 22.84 52 24.5L52 39.5C52 41.16 50.66 42.5 49 42.5L46 42.5Z", 0.16),
    ("M36 18.5L40.5 18.5C42.43 18.5 44 20.07 44 22L44 42C44 43.93 42.43 45.5 40.5 45.5L36 45.5Z", 0.24),
    (
        "M16 15.5L30 15.5C32.21 15.5 34 17.29 34 19.5L34 44.5C34 46.71 32.21 48.5 30 48.5L16 48.5C13.79 48.5 12 46.71 \
         12 44.5L12 19.5C12 17.29 13.79 15.5 16 15.5Z",
        0.36,
    ),
];

/// The glyph cut out of the mark's front poster: the kind of library a title is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkGlyph {
    /// The logo's play: anything that is no library's own kind (extras, collections).
    Play,
    /// A clapperboard.
    Movies,
    /// A TV.
    Shows,
    /// Notes.
    Music,
    /// An open book.
    Audiobooks,
}

/// An item's kind (the API's `kind`) to its glyph.
const KINDS: [(&str, MarkGlyph); 9] = [
    ("movie", MarkGlyph::Movies),
    ("show", MarkGlyph::Shows),
    ("season", MarkGlyph::Shows),
    ("episode", MarkGlyph::Shows),
    ("artist", MarkGlyph::Music),
    ("album", MarkGlyph::Music),
    ("track", MarkGlyph::Music),
    ("audiobook", MarkGlyph::Audiobooks),
    ("part", MarkGlyph::Audiobooks),
];

impl MarkGlyph {
    /// Every glyph.
    pub const ALL: [MarkGlyph; 5] =
        [MarkGlyph::Play, MarkGlyph::Movies, MarkGlyph::Shows, MarkGlyph::Music, MarkGlyph::Audiobooks];

    /// The glyph of a kind no library has as its own.
    pub const DEFAULT: MarkGlyph = MarkGlyph::Play;

    /// The glyph for an item of `kind` (the API's `kind`): its library's, else [`MarkGlyph::DEFAULT`].
    pub fn for_kind(kind: &str) -> MarkGlyph {
        KINDS.iter().find(|(k, _)| *k == kind).map_or(MarkGlyph::DEFAULT, |(_, glyph)| *glyph)
    }

    /// Its name in marks.json.
    pub fn name(self) -> &'static str {
        match self {
            MarkGlyph::Play => "play",
            MarkGlyph::Movies => "movies",
            MarkGlyph::Shows => "shows",
            MarkGlyph::Music => "music",
            MarkGlyph::Audiobooks => "audiobooks",
        }
    }

    /// Its path on the grid: drawn with the front poster's as one path filled even-odd, it is cut
    /// out of it.
    pub fn path(self) -> &'static str {
        match self {
            MarkGlyph::Play => {
                "M19 27L19 37C19 37.36 19.19 37.69 19.51 37.87C19.82 38.05 20.2 38.04 20.51 37.86L29.01 32.86C29.31 \
                 32.68 29.5 32.35 29.5 32C29.5 31.65 29.31 31.32 29.01 31.14L20.51 26.14C20.2 25.96 19.82 25.95 19.51 \
                 26.13C19.19 26.31 19 26.64 19 27Z"
            }
            MarkGlyph::Movies => {
                "M17.3 30.51C17.36 30.69 17.56 30.79 17.74 30.73L20.8 29.74C20.93 29.7 21.03 29.57 21.04 29.43L21.17 \
                 27.18C21.19 26.93 20.95 26.75 20.71 26.82L16.88 28.07C16.69 28.13 16.59 28.33 16.65 28.51ZM22 \
                 28.85C21.99 29.09 22.23 29.28 22.46 29.2L24.69 28.47C24.83 28.43 24.93 28.31 24.94 28.16L25.07 \
                 25.91C25.09 25.66 24.85 25.48 24.61 25.56L22.38 26.28C22.24 26.33 22.15 26.45 22.14 26.59ZM25.9 \
                 27.58C25.89 27.83 26.13 28.01 26.36 27.93L28.68 27.18C28.86 27.12 28.96 26.92 28.9 26.74L28.25 \
                 24.74C28.19 24.56 28 24.46 27.81 24.52L26.28 25.02C26.14 25.06 26.05 25.18 26.04 25.33ZM17.41 \
                 33.44C17.41 32.72 17.99 32.14 18.71 32.14L28.11 32.14C28.83 32.14 29.41 32.72 29.41 33.44L29.41 \
                 38.24C29.41 38.96 28.83 39.54 28.11 39.54L18.71 39.54C17.99 39.54 17.41 38.96 17.41 38.24Z"
            }
            MarkGlyph::Shows => {
                "M19.59 24.83C19.31 25.07 19.29 25.49 19.53 25.77L22.62 29.32C22.82 29.55 23.18 29.55 23.38 \
                 29.32L26.47 25.77C26.71 25.49 26.69 25.07 26.41 24.83L26.39 24.82C26.11 24.58 25.7 24.6 25.46 \
                 24.88L23.23 27.44C23.11 27.58 22.89 27.58 22.77 27.44L20.54 24.88C20.3 24.6 19.89 24.58 19.61 \
                 24.82ZM16.25 31.72C16.25 30.68 17.1 29.82 18.15 29.82L27.85 29.82C28.9 29.82 29.75 30.68 29.75 \
                 31.72L29.75 37.52C29.75 38.57 28.9 39.42 27.85 39.42L18.15 39.42C17.1 39.42 16.25 38.57 16.25 37.52Z"
            }
            MarkGlyph::Music => {
                "M20.16 26.47L29.61 24.47L29.61 34.87C29.61 35.97 28.88 36.93 27.82 37.23C26.77 37.53 25.65 37.09 \
                 25.07 36.16C24.49 35.23 24.61 34.02 25.35 33.22C26.09 32.41 27.28 32.19 28.26 32.68L28.26 \
                 27.26L21.51 28.69L21.51 36.87C21.51 37.97 20.78 38.93 19.72 39.23C18.67 39.53 17.55 39.09 16.97 \
                 38.16C16.39 37.23 16.51 36.02 17.25 35.22C17.99 34.41 19.18 34.19 20.16 34.68Z"
            }
            MarkGlyph::Audiobooks => {
                "M16 27.58C16 27.01 16.52 26.58 17.08 26.7L21.95 27.69C22.18 27.74 22.35 27.95 22.35 28.18L22.35 \
                 36.86C22.35 37.18 22.06 37.42 21.75 37.35L16.72 36.32C16.3 36.24 16 35.87 16 35.44ZM23.65 \
                 28.18C23.65 27.95 23.82 27.74 24.05 27.69L28.92 26.7C29.48 26.58 30 27.01 30 27.58L30 35.44C30 \
                 35.87 29.7 36.24 29.28 36.32L24.25 37.35C23.94 37.42 23.65 37.18 23.65 36.86Z"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::Value;

    /// packages/placeholder-mark/marks.json, from the repository this crate is in.
    fn marks() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/placeholder-mark/marks.json");
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        serde_json::from_str(&text).unwrap()
    }

    fn number(v: &Value, key: &str) -> f32 {
        v[key].as_f64().unwrap_or_else(|| panic!("marks.json has no number {key}")) as f32
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn the_sizes_match_marks_json() {
        let m = marks();
        assert!(close(number(&m, "grid"), GRID));
        assert!(close(number(&m, "share"), SHARE));
        assert!(close(number(&m, "shareOverTitle"), SHARE_OVER_TITLE));
        assert!(close(number(&m, "max"), MAX));
        assert!(close(number(&m, "gap"), GAP));
    }

    #[test]
    fn the_posters_match_marks_json() {
        let m = marks();
        let posters = m["posters"].as_array().unwrap();
        assert_eq!(posters.len(), POSTERS.len());
        for (p, (path, alpha)) in posters.iter().zip(POSTERS) {
            assert_eq!(p["path"].as_str(), Some(path));
            assert!(close(number(p, "alpha"), alpha), "{p}");
        }
    }

    #[test]
    fn the_glyphs_match_marks_json() {
        let m = marks();
        let glyphs = m["glyphs"].as_object().unwrap();
        assert_eq!(glyphs.len(), MarkGlyph::ALL.len());
        for glyph in MarkGlyph::ALL {
            assert_eq!(glyphs.get(glyph.name()).and_then(Value::as_str), Some(glyph.path()), "{glyph:?}");
        }
    }

    #[test]
    fn the_kinds_and_the_default_match_marks_json() {
        let m = marks();
        let kinds = m["kinds"].as_object().unwrap();
        assert_eq!(kinds.len(), KINDS.len());
        for (kind, glyph) in KINDS {
            assert_eq!(kinds.get(kind).and_then(Value::as_str), Some(glyph.name()), "{kind}");
        }
        assert_eq!(m["default"].as_str(), Some(MarkGlyph::DEFAULT.name()));
    }

    #[test]
    fn a_kind_takes_its_librarys_glyph_else_play() {
        assert_eq!(MarkGlyph::for_kind("movie"), MarkGlyph::Movies);
        assert_eq!(MarkGlyph::for_kind("episode"), MarkGlyph::Shows);
        assert_eq!(MarkGlyph::for_kind("album"), MarkGlyph::Music);
        assert_eq!(MarkGlyph::for_kind("part"), MarkGlyph::Audiobooks);
        for other in ["extra", "collection", "unknown", ""] {
            assert_eq!(MarkGlyph::for_kind(other), MarkGlyph::Play, "{other}");
        }
    }
}
