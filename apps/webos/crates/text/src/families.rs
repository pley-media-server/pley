//! Family names: the generic names the app asks for, the system families tried
//! for each, the order fallback tries families in, and how a face's weight and
//! slant are matched to a style's.
//!
//! Names are compared in lower case. An entry ending in `*` matches every
//! family that starts with it.

/// Families tried for [`pley_base::Family::Mono`], in order. The TV app asks
/// CSS for "Roboto Mono" then "DejaVu Sans Mono"; the others are the desktop
/// systems' own. Without any, Roboto with tabular digits draws the code.
pub(crate) const MONO: &[&str] = &[
    "roboto mono",
    "dejavu sans mono",
    "droid sans mono",
    "noto sans mono",
    "liberation mono",
    "cascadia mono",
    "consolas",
    "sf mono",
    "menlo",
    "courier new",
];

/// Families tried for [`pley_base::Family::Sans`] when the app's own Roboto is
/// missing (a broken install): the TV's own faces first.
pub(crate) const SANS: &[&str] =
    &["roboto", "lg smart ui", "droid sans", "noto sans", "dejavu sans", "segoe ui", "helvetica neue", "arial"];

/// Subtitles' "Serif" (Android's caption font choices, `ScreenCaptions.kt`).
pub(crate) const SERIF: &[&str] =
    &["noto serif", "droid serif", "dejavu serif", "liberation serif", "times new roman", "georgia", "times"];

/// Subtitles' "Casual": Android's casual face is Coming Soon; it is offered only
/// when the system has one of these.
pub(crate) const CASUAL: &[&str] = &["coming soon", "comic neue", "comic sans ms", "chalkboard se"];

/// The families fallback tries first, in this order, for a character no face of
/// a style has; every other family follows in the order of its file's path.
/// The TV's faces come first (`/usr/share/fonts`: Droid Sans, and Droid Sans
/// Fallback for Chinese, Japanese and Korean), then the desktop systems' broad
/// faces, so the simulator draws other scripts the way a TV would.
pub(crate) const FALLBACK_FIRST: &[&str] = &[
    "droid sans",
    "droid sans fallback",
    "lg smart ui*",
    "noto sans",
    "noto sans*",
    "dejavu sans",
    "segoe ui",
    "arial",
    "nirmala ui",
    "leelawadee ui",
    "microsoft yahei*",
    "yu gothic*",
    "malgun gothic*",
    "microsoft jhenghei*",
    "ebrima",
    "gadugi",
    "segoe ui historic",
    "segoe ui symbol",
    "segoe ui emoji",
    "helvetica neue",
    "pingfang sc",
    "hiragino sans",
    "apple sd gothic neo",
    "geeza pro",
    "arial hebrew",
    "arial unicode ms",
    "apple symbols",
];

/// What a family name asks for.
pub(crate) enum Generic {
    /// The app's own face.
    Sans,
    /// A monospace face (or Roboto with tabular digits).
    Mono,
    /// The first of these families the system has.
    List(&'static [&'static str]),
}

/// The generic family a name stands for, if it is one (`"serif"`,
/// `"monospace"`, …); lower-case input.
pub(crate) fn generic(name: &str) -> Option<Generic> {
    Some(match name {
        "sans" | "sans-serif" | "sans serif" | "default" | "system-ui" => Generic::Sans,
        "mono" | "monospace" => Generic::Mono,
        "serif" => Generic::List(SERIF),
        "casual" | "cursive" => Generic::List(CASUAL),
        _ => return None,
    })
}

/// The words legacy family names end with when a font family names each of
/// its weights as a family of its own ("Roboto Medium", "Arial Black"), with
/// the weights each stands for.
const WEIGHT_WORDS: &[(&str, u16, u16)] = &[
    ("thin", 100, 200),
    ("hairline", 100, 200),
    ("extralight", 150, 250),
    ("extra light", 150, 250),
    ("ultralight", 150, 250),
    ("ultra light", 150, 250),
    ("light", 250, 399),
    ("semilight", 300, 399),
    ("semi light", 300, 399),
    ("medium", 450, 599),
    ("semibold", 550, 699),
    ("semi bold", 550, 699),
    ("demibold", 550, 699),
    ("demi bold", 550, 699),
    ("extrabold", 750, 850),
    ("extra bold", 750, 850),
    ("ultrabold", 750, 850),
    ("ultra bold", 750, 850),
    ("black", 800, 1000),
    ("heavy", 800, 1000),
];

/// The family a legacy family name belongs to: "Roboto Medium" at weight 500
/// is Roboto's (fontsource's static Roboto names its Medium so, with no
/// typographic family to say otherwise). `None` when the name doesn't end in
/// a word for its weight.
pub(crate) fn base_family(family: &str, weight: u16) -> Option<&str> {
    WEIGHT_WORDS.iter().find_map(|&(word, min, max)| {
        let split = family.len().checked_sub(word.len())?;
        if !(min..=max).contains(&weight) || !family.is_char_boundary(split) {
            return None;
        }
        let (head, tail) = family.split_at(split);
        let head = head.strip_suffix([' ', '-'])?.trim_end();
        (tail.eq_ignore_ascii_case(word) && !head.is_empty()).then_some(head)
    })
}

/// Whether a family name matches a list entry (see the module's note on `*`).
pub(crate) fn matches(entry: &str, family: &str) -> bool {
    match entry.strip_suffix('*') {
        Some(prefix) => family.starts_with(prefix),
        None => family == entry,
    }
}

/// Where a family comes in [`FALLBACK_FIRST`]; families not in it come after.
pub(crate) fn fallback_rank(family: &str) -> usize {
    FALLBACK_FIRST.iter().position(|e| matches(e, family)).unwrap_or(FALLBACK_FIRST.len())
}

/// How well a face suits a weight and slant: lower is better. A face of the
/// wrong slant ranks after every face of the right one; within a slant,
/// weights rank as CSS Fonts 4 §5.2 matches `font-weight` (for 400 to 500:
/// up to 500 first, then lighter, then heavier; above 500: heavier first;
/// below 400: lighter first).
pub(crate) fn match_rank(want_weight: u16, want_italic: bool, weight: u16, italic: bool) -> u32 {
    let (want, have) = (u32::from(want_weight), u32::from(weight));
    let distance = want.abs_diff(have);
    let side = if (400..=500).contains(&want) {
        if have >= want && have <= 500 {
            0
        } else if have < want {
            1
        } else {
            2
        }
    } else if want < 400 {
        u32::from(have > want)
    } else {
        u32::from(have < want)
    };
    u32::from(want_italic != italic) * 100_000 + side * 10_000 + distance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_rank_as_css_does() {
        // Medium falls back to Regular before Bold; Bold to heavier before lighter.
        assert!(match_rank(500, false, 400, false) < match_rank(500, false, 700, false));
        assert!(match_rank(700, false, 900, false) < match_rank(700, false, 500, false));
        assert!(match_rank(400, false, 500, false) < match_rank(400, false, 300, false));
        assert!(match_rank(400, false, 300, false) < match_rank(400, false, 600, false));
        assert_eq!(match_rank(700, true, 700, true), 0);
        // The right slant wins over the right weight.
        assert!(match_rank(400, true, 700, true) < match_rank(400, true, 400, false));
    }

    #[test]
    fn legacy_weight_families_join_their_family() {
        assert_eq!(base_family("Roboto Medium", 500), Some("Roboto"));
        assert_eq!(base_family("Arial Black", 900), Some("Arial"));
        assert_eq!(base_family("Segoe UI Semibold", 600), Some("Segoe UI"));
        assert_eq!(base_family("Noto Sans-Light", 300), Some("Noto Sans"));
        // Only a word that says the face's own weight.
        assert_eq!(base_family("Roboto Medium", 400), None);
        assert_eq!(base_family("Roboto", 500), None);
        assert_eq!(base_family("Medium", 500), None);
    }

    #[test]
    fn names() {
        assert!(matches("noto sans*", "noto sans hebrew"));
        assert!(!matches("noto sans", "noto sans hebrew"));
        assert_eq!(fallback_rank("droid sans"), 0);
        assert_eq!(fallback_rank("some face"), FALLBACK_FIRST.len());
        assert!(matches!(generic("monospace"), Some(Generic::Mono)));
        assert!(generic("roboto").is_none());
    }
}
