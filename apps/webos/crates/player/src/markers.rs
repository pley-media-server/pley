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

/// Skip shows on its own this long from when it appears; then, while the playhead is still in
/// its marker, only with the player's controls.
pub const SKIP_ALONE_MS: i64 = 8_000;

/// When Skip shows on its own: for [`SKIP_ALONE_MS`] from when it appears, on the app's clock.
/// The playhead entering a marker (playing or seeking into it, or starting in it) opens a fresh
/// window; a seek inside the same marker doesn't. Skip appears again when what took its place
/// goes (Up Next canceled over credits), and its window starts then.
#[derive(Debug, Default)]
pub struct SkipWindow {
    /// The marker the playhead was last found in.
    marker: Option<Marker>,
    /// When Skip appeared for it; none while it isn't offered.
    since: Option<i64>,
}

impl SkipWindow {
    /// The marker the playhead is in now, as the controller's check finds it.
    pub fn at(&mut self, marker: Option<&Marker>) {
        if self.marker.as_ref() != marker {
            self.marker = marker.cloned();
            self.since = None;
        }
    }

    /// Whether Skip shows on its own at `now`; `offered` is false while something takes its
    /// place (Up Next's card).
    pub fn alone(&mut self, offered: bool, now: i64) -> bool {
        if self.marker.is_none() || !offered {
            self.since = None;
            return false;
        }
        now < *self.since.get_or_insert(now) + SKIP_ALONE_MS
    }

    /// When Skip stops showing on its own, while it still does at `now`.
    pub fn due(&self, now: i64) -> Option<i64> {
        self.since.map(|t| t + SKIP_ALONE_MS).filter(|t| *t > now)
    }

    /// Forgets the marker (a new item).
    pub fn reset(&mut self) {
        *self = SkipWindow::default();
    }
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
    fn skip_shows_alone_for_8_s_from_when_it_appears() {
        let intro = marker(MarkerKind::Intro, 10_000, 70_000);
        let credits = marker(MarkerKind::Credits, 100_000, 120_000);
        let mut w = SkipWindow::default();
        assert!(!w.alone(true, 0), "no marker");
        w.at(Some(&intro));
        assert!(w.alone(true, 1_000));
        assert_eq!(w.due(1_000), Some(9_000));
        assert!(w.alone(true, 8_999));
        assert!(!w.alone(true, 9_000));
        assert_eq!(w.due(9_000), None);
        // A seek inside the same intro: nothing new.
        w.at(Some(&intro));
        assert!(!w.alone(true, 10_000));
        // Out and back in: a fresh 8 s.
        w.at(None);
        assert!(!w.alone(true, 11_000));
        w.at(Some(&intro));
        assert!(w.alone(true, 12_000));
        assert!(!w.alone(true, 20_000));
        // The next marker: a fresh 8 s too.
        w.at(Some(&credits));
        assert!(w.alone(true, 30_000));
        assert!(w.alone(true, 37_999));
    }

    #[test]
    fn skip_shows_alone_again_from_when_up_next_goes() {
        let credits = marker(MarkerKind::Credits, 100_000, 120_000);
        let mut w = SkipWindow::default();
        w.at(Some(&credits));
        assert!(w.alone(true, 0));
        // Up Next's card takes its place, then is canceled 20 s on.
        assert!(!w.alone(false, 500));
        assert!(!w.alone(false, 20_000));
        assert!(w.alone(true, 20_500));
        assert!(w.alone(true, 28_499));
        assert!(!w.alone(true, 28_500));
        w.reset();
        assert!(!w.alone(true, 30_000));
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
