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

/// Credits ending this close to the end of their file run to it: what follows
/// is a studio's logo or black, not a scene (the server's `CREDITS_TAIL_MS`).
pub const CREDITS_TAIL_MS: i64 = 15_000;

/// Whether `m` is credits that run to the end of their file (`duration_ms`,
/// 0 when not known). Only those bring Up Next: credits that something
/// follows (an after-credits scene, a game show's last minutes) bring Skip
/// Credits alone, or Up Next's countdown would skip what comes after them.
/// Up Next then comes at the end of the file, as it does without credits.
pub fn reaches_end(m: &Marker, duration_ms: i64) -> bool {
    m.kind == MarkerKind::Credits && duration_ms > 0 && duration_ms - m.end_ms <= CREDITS_TAIL_MS
}

/// Credits that run to the end start Up Next once each: seeking back into
/// the same credits doesn't show the card again. Forgotten for a new item.
#[derive(Debug, Default)]
pub struct CreditsGate {
    seen: HashSet<(i64, i64)>,
}

impl CreditsGate {
    /// Whether entering `m`, in a file of `duration_ms`, should start Up Next
    /// (the first time only).
    pub fn enter(&mut self, m: &Marker, duration_ms: i64) -> bool {
        reaches_end(m, duration_ms) && self.seen.insert((m.file_id, m.start_ms))
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
        assert!(!gate.enter(&marker(MarkerKind::Intro, 0, 10), 120_000));
        assert!(gate.enter(&credits, 120_000));
        assert!(!gate.enter(&credits, 120_000));
        gate.reset();
        assert!(gate.enter(&credits, 120_000));
    }

    #[test]
    fn only_credits_that_run_to_the_end_open_up_next() {
        let credits = marker(MarkerKind::Credits, 100_000, 120_000);
        assert!(reaches_end(&credits, 120_000));
        assert!(reaches_end(&credits, 135_000), "a logo after them");
        assert!(!reaches_end(&credits, 135_001));
        assert!(!reaches_end(&credits, 180_000), "a scene after them");
        assert!(!reaches_end(&credits, 0), "a file of unknown length");
        assert!(!reaches_end(&marker(MarkerKind::Intro, 100_000, 120_000), 120_000));
        // Not opened while the file's length isn't known, then once it is.
        let mut gate = CreditsGate::default();
        assert!(!gate.enter(&credits, 180_000));
        assert!(!gate.enter(&credits, 0));
        assert!(gate.enter(&credits, 130_000));
    }
}
