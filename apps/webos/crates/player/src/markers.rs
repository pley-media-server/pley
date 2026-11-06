//! Skip Intro and Skip Credits (packages/tv/src/player/markers.ts).

use std::collections::HashSet;

use pley_core::model::{ItemDetail, Marker, MarkerKind};
use pley_i18n::t;

/// The intro and credits of the file playing.
pub fn markers_of(item: &ItemDetail, file_id: i64) -> Vec<Marker> {
    item.markers_of(file_id).filter(|m| matches!(m.kind, MarkerKind::Intro | MarkerKind::Credits)).cloned().collect()
}

/// The Skip button leaves this long before its marker ends.
pub const SKIP_LEAVES_MS: i64 = 1000;

/// The marker the playhead is in, for the Skip button: its end is still more than a second away.
pub fn active_marker(markers: &[Marker], position_ms: i64) -> Option<&Marker> {
    markers.iter().find(|m| m.start_ms <= position_ms && position_ms < m.end_ms - SKIP_LEAVES_MS)
}

/// Where Skip goes: an intro's end, less up to 750 ms (half a short one) so
/// the cut isn't missed; the credits' end. The caller clamps to the duration.
pub fn skip_target(m: &Marker) -> i64 {
    if m.kind == MarkerKind::Intro { m.end_ms - 750.min((m.end_ms - m.start_ms) / 2) } else { m.end_ms }
}

/// "Skip Intro" or "Skip Credits".
pub fn skip_label(m: &Marker) -> &'static str {
    if m.kind == MarkerKind::Intro { t!("player.skip.intro") } else { t!("player.skip.credits") }
}

/// Credits start Up Next once each: seeking back into the same credits
/// doesn't show the card again. Forgotten for a new item.
#[derive(Debug, Default)]
pub struct CreditsGate {
    seen: HashSet<(i64, i64)>,
}

impl CreditsGate {
    /// Whether entering `m` should start Up Next (the first time only).
    pub fn enter(&mut self, m: &Marker) -> bool {
        m.kind == MarkerKind::Credits && self.seen.insert((m.file_id, m.start_ms))
    }

    /// Forgets every credits seen.
    pub fn reset(&mut self) {
        self.seen.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(kind: MarkerKind, start_ms: i64, end_ms: i64) -> Marker {
        Marker { file_id: 1, kind, start_ms, end_ms, ..Marker::default() }
    }

    #[test]
    fn skip_shows_until_a_second_before_the_end() {
        let m = [marker(MarkerKind::Intro, 10_000, 70_000)];
        assert!(active_marker(&m, 9_999).is_none());
        assert!(active_marker(&m, 10_000).is_some());
        assert!(active_marker(&m, 68_999).is_some());
        assert!(active_marker(&m, 69_000).is_none());
    }

    #[test]
    fn targets_and_labels() {
        let intro = marker(MarkerKind::Intro, 10_000, 70_000);
        assert_eq!(skip_target(&intro), 69_250);
        assert_eq!(skip_target(&marker(MarkerKind::Intro, 10_000, 11_000)), 10_500);
        let credits = marker(MarkerKind::Credits, 100_000, 120_000);
        assert_eq!(skip_target(&credits), 120_000);
        assert_eq!(skip_label(&intro), "Skip Intro");
        assert_eq!(skip_label(&credits), "Skip Credits");
    }

    #[test]
    fn credits_open_up_next_once() {
        let mut gate = CreditsGate::default();
        let credits = marker(MarkerKind::Credits, 100_000, 120_000);
        assert!(!gate.enter(&marker(MarkerKind::Intro, 0, 10)));
        assert!(gate.enter(&credits));
        assert!(!gate.enter(&credits));
        gate.reset();
        assert!(gate.enter(&credits));
    }
}
