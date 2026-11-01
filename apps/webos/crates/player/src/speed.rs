//! Playback speed (packages/tv/src/player/speed.ts): the speeds offered, for
//! one title at a time (the next plays at normal speed). LG's pipeline plays
//! them with their sound, so LG offers them, as the web app did.

use crate::quality::js_number;

/// The speeds offered, slowest first.
pub const SPEEDS: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

/// "Normal", "1.5×".
pub fn speed_label(speed: f32) -> String {
    if speed == 1.0 { "Normal".to_string() } else { format!("{}×", js_number(speed as f64)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(speed_label(1.0), "Normal");
        assert_eq!(speed_label(1.5), "1.5×");
        assert_eq!(speed_label(2.0), "2×");
        assert_eq!(speed_label(0.75), "0.75×");
    }
}
