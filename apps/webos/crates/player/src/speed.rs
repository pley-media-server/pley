//! Playback speed (packages/tv/src/player/speed.ts): the speeds offered, for
//! one title at a time (the next plays at normal speed). LG's pipeline plays
//! them with their sound, so LG offers them, as the web app did.

use pley_i18n::t;

use crate::quality::shown_number;

/// The speeds offered, slowest first.
pub const SPEEDS: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

/// "Normal", "1.5×", in the language shown ("1,5×").
pub fn speed_label(speed: f32) -> String {
    if speed == 1.0 { t!("player.speed.normal").to_string() } else { format!("{}×", shown_number(speed as f64)) }
}

#[cfg(test)]
mod tests {
    use pley_i18n::{Language, with_language};

    use super::*;

    #[test]
    fn labels() {
        assert_eq!(speed_label(1.0), "Normal");
        assert_eq!(speed_label(1.5), "1.5×");
        assert_eq!(speed_label(2.0), "2×");
        assert_eq!(speed_label(0.75), "0.75×");
    }

    #[test]
    fn french_writes_a_decimal_comma() {
        with_language(Language::Fr, || {
            assert_eq!(speed_label(1.0), "Normale");
            assert_eq!(speed_label(1.5), "1,5×");
            assert_eq!(speed_label(0.75), "0,75×");
        });
    }
}
