//! Numbers as the language shown writes them, as CLDR has it: English groups thousands with a
//! comma and writes a decimal point ("12,345.5"); French groups with a no-break space and writes a
//! decimal comma ("12 345,5"); Spanish groups with a point from five digits on and writes a decimal
//! comma ("1234,5", "12.345,5").

use crate::{Language, language};

/// French's group separator: a no-break space (U+00A0), as the catalogs write French's spaces.
/// CLDR's narrow one (U+202F) isn't in the app's Roboto.
const NO_BREAK_SPACE: char = '\u{A0}';

/// `n` grouped by thousands in the language shown: "12,345", "12 345", "12.345".
pub fn number(n: i64) -> String {
    group(language(), n)
}

pub(crate) fn group(language: Language, n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let (separator, least) = match language {
        Language::En => (',', 4),
        Language::Fr => (NO_BREAK_SPACE, 4),
        // Spanish leaves four digits whole: "1234", "12.345".
        Language::Es => ('.', 5),
    };
    let mut out = String::with_capacity(digits.len() + 4);
    if n < 0 {
        out.push('-');
    }
    if digits.len() < least {
        out.push_str(&digits);
        return out;
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

/// `v` with `places` decimals (halves away from zero) in the language shown: "1.5", "1,5";
/// thousands grouped as [`number`] groups them.
pub fn decimal(v: f64, places: usize) -> String {
    write_decimal(language(), v, places)
}

pub(crate) fn write_decimal(language: Language, v: f64, places: usize) -> String {
    let fixed = format!("{:.*}", places, round_half_away(v, places));
    let (whole, fraction) = match fixed.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (fixed.as_str(), None),
    };
    let negative = whole.starts_with('-');
    let magnitude: i64 = whole.trim_start_matches('-').parse().unwrap_or(0);
    let mut out = group(language, magnitude);
    if negative && (magnitude != 0 || fraction.is_some_and(|f| f.bytes().any(|b| b != b'0'))) {
        out.insert(0, '-');
    }
    if let Some(f) = fraction {
        out.push(if language == Language::En { '.' } else { ',' });
        out.push_str(f);
    }
    out
}

fn round_half_away(v: f64, places: usize) -> f64 {
    let scale = 10f64.powi(places as i32);
    (v * scale).round() / scale
}
