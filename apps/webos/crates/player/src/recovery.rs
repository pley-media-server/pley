//! The recovery ladder (packages/tv/src/player/recovery.ts, the Android
//! app's `Recovery.kt`):
//!
//! - Direct play the TV can't decode or open: a remux when the video is
//!   H.264 or HEVC and only the container or the audio failed, else a
//!   transcode; each once.
//! - 404 or 410 ("playback session expired") or an unreadable playlist: a
//!   new session at the position, at most 3 times.
//! - Network errors: a new session, at most 3.
//! - A 12 s stall: a direct play that has data but doesn't move becomes a
//!   transcode; a stream asks the server first; a stream that keeps
//!   stopping gets one new session, a starving one up to 3.
//! - Media that ends where it started, long before its end: a direct play
//!   becomes a transcode, a stream gets one new session, then it fails.
//!
//! Budgets start over after 5 s of playback and on Retry.

use pley_core::model::{PlaybackMethod, PlaybackMode};
use pley_i18n::t;

/// Why a fallback plays something other than the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    Unplayable,
    NotSmooth,
}

/// Why playback gave up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Unplayable,
    Stuck,
    Short,
    Stopped,
    Network,
    Engine,
}

/// What went wrong in the TV's player, in the terms recovery decides on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// It can't decode or open this: `audio` when the audio failed, `container` when the file did.
    Unsupported {
        audio: bool,
        container: bool,
        detail: String,
    },
    Http {
        status: u16,
        detail: String,
    },
    /// 410: stopped from elsewhere.
    Stopped {
        message: Option<String>,
    },
    Playlist {
        detail: String,
    },
    Network {
        detail: String,
    },
    Other {
        detail: String,
    },
}

/// What recovery decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    /// A new session at the position: in `mode` when it names one (a fallback kept for the item).
    NewSession {
        mode: Option<PlaybackMode>,
        reason: Option<FallbackReason>,
    },
    /// Ask the server how its transcode is doing, then call [`Recovery::after_server_check`].
    CheckServer,
    /// The server can't keep up: offer a lower quality, keep playing.
    SuggestLowerQuality,
    Fail {
        failure: Failure,
        detail: Option<String>,
    },
}

/// Playing on this long from where it last started resets the budgets.
pub const RECOVERED_AFTER_MS: i64 = 5_000;
/// New sessions for an expired session or a starving stall.
pub const MAX_RECREATE: u32 = 3;
/// New sessions for network errors.
pub const MAX_NETWORK: u32 = 3;
/// New sessions for a stall with data buffered.
pub const MAX_STUCK: u32 = 1;
/// New sessions for an end that came too soon.
pub const MAX_SHORT: u32 = 1;
/// Videos a remux copies, which the TV takes in another container.
const COPYABLE_VIDEO: [&str; 2] = ["h264", "hevc"];

/// One title's recovery budgets and the fallback it keeps.
#[derive(Clone, Debug, Default)]
pub struct Recovery {
    pub fallback_mode: Option<PlaybackMode>,
    pub fallback_reason: Option<FallbackReason>,
    pub struggle_switched: bool,
    remux_tried: bool,
    transcode_tried: bool,
    pub recreate: u32,
    pub network: u32,
    pub stuck: u32,
    pub short: u32,
    played_from: Option<i64>,
}

impl Recovery {
    /// Where playback is now: 5 s played on from where it last (re)started means the trouble is over.
    pub fn on_position(&mut self, position_ms: i64) {
        match self.played_from {
            Some(from) if position_ms >= from => {
                if position_ms - from >= RECOVERED_AFTER_MS {
                    self.recreate = 0;
                    self.network = 0;
                    self.stuck = 0;
                    self.short = 0;
                    self.played_from = Some(position_ms);
                }
            }
            _ => self.played_from = Some(position_ms),
        }
    }

    /// A seek or a new session: progress counts from where it lands.
    pub fn on_jump(&mut self) {
        self.played_from = None;
    }

    /// What to do about `error` in a session played by `method`, of a video in `video_codec`.
    pub fn decide(&mut self, error: &EngineError, method: PlaybackMethod, video_codec: Option<&str>) -> RecoveryAction {
        match error {
            EngineError::Unsupported { audio, container, detail } => {
                self.fallback(method, video_codec, *audio || *container, detail)
            }
            EngineError::Http { status, detail } => {
                if *status == 404 || *status == 410 {
                    self.expired(detail)
                } else {
                    self.network_error(detail)
                }
            }
            EngineError::Playlist { detail } => self.expired(detail),
            EngineError::Network { detail } => self.network_error(detail),
            EngineError::Stopped { message } => {
                RecoveryAction::Fail { failure: Failure::Stopped, detail: message.clone() }
            }
            EngineError::Other { detail } => {
                RecoveryAction::Fail { failure: Failure::Engine, detail: Some(detail.clone()) }
            }
        }
    }

    fn fallback(
        &mut self,
        method: PlaybackMethod,
        video_codec: Option<&str>,
        partial: bool,
        detail: &str,
    ) -> RecoveryAction {
        let codec = video_codec.unwrap_or("").to_ascii_lowercase();
        let copyable = COPYABLE_VIDEO.contains(&codec.as_str());
        if method == PlaybackMethod::Direct && partial && copyable && !self.remux_tried {
            self.remux_tried = true;
            return self.switch_to(PlaybackMode::Remux, FallbackReason::Unplayable);
        }
        if method != PlaybackMethod::Transcode && !self.transcode_tried {
            self.transcode_tried = true;
            return self.switch_to(PlaybackMode::Transcode, FallbackReason::Unplayable);
        }
        RecoveryAction::Fail { failure: Failure::Unplayable, detail: Some(detail.to_string()) }
    }

    fn switch_to(&mut self, mode: PlaybackMode, reason: FallbackReason) -> RecoveryAction {
        self.fallback_mode = Some(mode);
        self.fallback_reason = Some(reason);
        RecoveryAction::NewSession { mode: Some(mode), reason: Some(reason) }
    }

    fn expired(&mut self, detail: &str) -> RecoveryAction {
        if self.recreate < MAX_RECREATE {
            self.recreate += 1;
            return RecoveryAction::NewSession { mode: None, reason: None };
        }
        RecoveryAction::Fail { failure: Failure::Stopped, detail: Some(detail.to_string()) }
    }

    fn network_error(&mut self, detail: &str) -> RecoveryAction {
        if self.network < MAX_NETWORK {
            self.network += 1;
            return RecoveryAction::NewSession { mode: None, reason: None };
        }
        RecoveryAction::Fail { failure: Failure::Network, detail: Some(detail.to_string()) }
    }

    /// Playback stood still for 12 s while meant to play; `starving` when nothing is buffered ahead.
    pub fn on_stall(&mut self, method: PlaybackMethod, starving: bool) -> RecoveryAction {
        if !starving && method == PlaybackMethod::Direct && !self.transcode_tried {
            self.transcode_tried = true;
            return self.switch_to(PlaybackMode::Transcode, FallbackReason::NotSmooth);
        }
        if method != PlaybackMethod::Direct {
            return RecoveryAction::CheckServer;
        }
        self.after_stall(starving)
    }

    /// The server's transcode runs at `speed` (none when it can't say).
    pub fn after_server_check(&mut self, speed: Option<f64>, starving: bool) -> RecoveryAction {
        if speed.is_some_and(|s| s < 1.0) { RecoveryAction::SuggestLowerQuality } else { self.after_stall(starving) }
    }

    fn after_stall(&mut self, starving: bool) -> RecoveryAction {
        if !starving {
            if self.stuck < MAX_STUCK {
                self.stuck += 1;
                return RecoveryAction::NewSession { mode: None, reason: None };
            }
            return RecoveryAction::Fail { failure: Failure::Stuck, detail: None };
        }
        if self.recreate < MAX_RECREATE {
            self.recreate += 1;
            return RecoveryAction::NewSession { mode: None, reason: None };
        }
        RecoveryAction::Fail { failure: Failure::Stopped, detail: None }
    }

    /// The media ended right where it was started, long before its end.
    pub fn on_ended_short(&mut self, method: PlaybackMethod) -> RecoveryAction {
        if method == PlaybackMethod::Direct && !self.transcode_tried {
            self.transcode_tried = true;
            return self.switch_to(PlaybackMode::Transcode, FallbackReason::Unplayable);
        }
        if self.short < MAX_SHORT {
            self.short += 1;
            return RecoveryAction::NewSession { mode: None, reason: None };
        }
        RecoveryAction::Fail { failure: Failure::Short, detail: None }
    }

    /// The decoder drops frames: one switch to a transcode per item; none when it switched already or transcodes.
    pub fn on_struggling(&mut self, method: PlaybackMethod) -> Option<RecoveryAction> {
        if self.struggle_switched || method == PlaybackMethod::Transcode {
            return None;
        }
        self.struggle_switched = true;
        self.transcode_tried = true;
        Some(self.switch_to(PlaybackMode::Transcode, FallbackReason::NotSmooth))
    }

    /// "Play original": the fallback is forgotten.
    pub fn play_original(&mut self) {
        self.fallback_mode = None;
        self.fallback_reason = None;
    }

    /// A quality was picked: the fallback is forgotten.
    pub fn quality_changed(&mut self) {
        self.play_original();
    }

    /// Retry: the budgets start over.
    pub fn retry(&mut self) {
        self.recreate = 0;
        self.network = 0;
        self.stuck = 0;
        self.short = 0;
        self.played_from = None;
    }

    /// Another item: everything starts over.
    pub fn new_item(&mut self) {
        *self = Recovery::default();
    }
}

/// What people read for a failure (the Android app's strings_session.xml).
pub fn failure_text(failure: Failure, detail: Option<&str>) -> String {
    match failure {
        Failure::Unplayable => t!("player.errors.unplayable").into(),
        Failure::Stuck => t!("player.errors.sameSpot").into(),
        Failure::Short => t!("player.errors.endsEarly").into(),
        Failure::Stopped => t!("player.errors.stalled").into(),
        Failure::Network => t!("player.errors.reachHint").into(),
        Failure::Engine => {
            // The TV's own words when it gives any ("PIPELINE_ERROR_DECODE"), not its generic ones.
            let short = detail.unwrap_or("").split(':').next().unwrap_or("").trim();
            if !short.is_empty() && !short.eq_ignore_ascii_case("playback failed") {
                t!("player.errors.playerGaveUpWith", detail = short)
            } else {
                t!("player.errors.playerGaveUp").into()
            }
        }
    }
}

/// Dropped frames, sampled every 2 s while playing (the Android app's
/// `StruggleMeter`): struggling when more than 1 % of the frames of a 10 s
/// window were dropped. Counts that go back (a new decoder) restart it.
#[derive(Clone, Debug, Default)]
pub struct StruggleMeter {
    samples: Vec<(i64, u64, u64)>,
}

impl StruggleMeter {
    /// The window dropped frames are counted over.
    pub const WINDOW_MS: i64 = 10_000;
    /// More than this share of frames dropped is struggling.
    pub const SHARE: f64 = 0.01;

    /// A sample of the decoder's counters at `at` ms; whether it struggles now.
    pub fn sample(&mut self, at: i64, dropped: u64, total: u64) -> bool {
        if let Some(&(_, d, t)) = self.samples.last()
            && (dropped < d || total < t)
        {
            self.samples.clear();
        }
        self.samples.push((at, dropped, total));
        while self.samples.len() > 1 && at - self.samples[0].0 > Self::WINDOW_MS {
            self.samples.remove(0);
        }
        let (first_at, first_dropped, first_total) = self.samples[0];
        if at - first_at < Self::WINDOW_MS - 2_000 {
            return false;
        }
        let frames = total.saturating_sub(first_total);
        let drops = dropped.saturating_sub(first_dropped);
        if frames == 0 {
            return false;
        }
        if drops as f64 / frames as f64 > Self::SHARE {
            self.samples.clear();
            return true;
        }
        false
    }

    /// Starts over.
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsupported(audio: bool, container: bool) -> EngineError {
        EngineError::Unsupported { audio, container, detail: "no".into() }
    }

    #[test]
    fn direct_play_falls_back_to_a_remux_then_a_transcode() {
        let mut r = Recovery::default();
        let a = r.decide(&unsupported(true, false), PlaybackMethod::Direct, Some("hevc"));
        assert_eq!(
            a,
            RecoveryAction::NewSession { mode: Some(PlaybackMode::Remux), reason: Some(FallbackReason::Unplayable) }
        );
        let a = r.decide(&unsupported(true, false), PlaybackMethod::Remux, Some("hevc"));
        assert_eq!(
            a,
            RecoveryAction::NewSession {
                mode: Some(PlaybackMode::Transcode),
                reason: Some(FallbackReason::Unplayable)
            }
        );
        let a = r.decide(&unsupported(true, false), PlaybackMethod::Transcode, Some("hevc"));
        assert!(matches!(a, RecoveryAction::Fail { failure: Failure::Unplayable, .. }));
    }

    #[test]
    fn a_video_it_cant_decode_goes_straight_to_a_transcode() {
        let mut r = Recovery::default();
        let a = r.decide(&unsupported(false, false), PlaybackMethod::Direct, Some("av1"));
        assert_eq!(r.fallback_mode, Some(PlaybackMode::Transcode));
        assert!(matches!(a, RecoveryAction::NewSession { mode: Some(PlaybackMode::Transcode), .. }));
    }

    #[test]
    fn expired_sessions_are_made_again_three_times() {
        let mut r = Recovery::default();
        let gone = EngineError::Http { status: 404, detail: "gone".into() };
        for _ in 0..MAX_RECREATE {
            assert_eq!(
                r.decide(&gone, PlaybackMethod::Transcode, None),
                RecoveryAction::NewSession { mode: None, reason: None }
            );
        }
        assert!(matches!(
            r.decide(&gone, PlaybackMethod::Transcode, None),
            RecoveryAction::Fail { failure: Failure::Stopped, .. }
        ));
        // Five seconds played on: the budget is back.
        r.on_position(1_000);
        r.on_position(6_000);
        assert_eq!(
            r.decide(&gone, PlaybackMethod::Transcode, None),
            RecoveryAction::NewSession { mode: None, reason: None }
        );
    }

    #[test]
    fn network_errors_have_their_own_budget() {
        let mut r = Recovery::default();
        let net = EngineError::Network { detail: "x".into() };
        for _ in 0..MAX_NETWORK {
            assert!(matches!(r.decide(&net, PlaybackMethod::Direct, None), RecoveryAction::NewSession { .. }));
        }
        assert!(matches!(
            r.decide(&net, PlaybackMethod::Direct, None),
            RecoveryAction::Fail { failure: Failure::Network, .. }
        ));
        r.retry();
        assert!(matches!(r.decide(&net, PlaybackMethod::Direct, None), RecoveryAction::NewSession { .. }));
    }

    #[test]
    fn stalls() {
        let mut r = Recovery::default();
        assert_eq!(
            r.on_stall(PlaybackMethod::Direct, false),
            RecoveryAction::NewSession { mode: Some(PlaybackMode::Transcode), reason: Some(FallbackReason::NotSmooth) }
        );
        assert_eq!(r.on_stall(PlaybackMethod::Transcode, false), RecoveryAction::CheckServer);
        assert_eq!(r.after_server_check(Some(0.8), false), RecoveryAction::SuggestLowerQuality);
        assert_eq!(r.after_server_check(Some(1.5), false), RecoveryAction::NewSession { mode: None, reason: None });
        assert!(matches!(r.after_server_check(None, false), RecoveryAction::Fail { failure: Failure::Stuck, .. }));
        for _ in 0..MAX_RECREATE {
            assert!(matches!(r.after_server_check(None, true), RecoveryAction::NewSession { .. }));
        }
        assert!(matches!(r.after_server_check(None, true), RecoveryAction::Fail { failure: Failure::Stopped, .. }));
    }

    #[test]
    fn short_ends() {
        let mut r = Recovery::default();
        assert!(matches!(
            r.on_ended_short(PlaybackMethod::Direct),
            RecoveryAction::NewSession { mode: Some(PlaybackMode::Transcode), .. }
        ));
        assert_eq!(
            r.on_ended_short(PlaybackMethod::Transcode),
            RecoveryAction::NewSession { mode: None, reason: None }
        );
        assert!(matches!(
            r.on_ended_short(PlaybackMethod::Transcode),
            RecoveryAction::Fail { failure: Failure::Short, .. }
        ));
    }

    #[test]
    fn struggling_switches_once() {
        let mut r = Recovery::default();
        assert!(r.on_struggling(PlaybackMethod::Direct).is_some());
        assert!(r.struggle_switched);
        assert!(r.on_struggling(PlaybackMethod::Direct).is_none());
        r.new_item();
        assert!(r.on_struggling(PlaybackMethod::Transcode).is_none());
    }

    #[test]
    fn failure_texts() {
        assert_eq!(
            failure_text(Failure::Engine, Some("PIPELINE_ERROR_DECODE: bad")),
            "The TV's player gave up (PIPELINE_ERROR_DECODE)."
        );
        assert_eq!(failure_text(Failure::Engine, Some("Playback failed")), "The TV's player gave up.");
        assert_eq!(failure_text(Failure::Engine, None), "The TV's player gave up.");
        assert_eq!(failure_text(Failure::Short, None), "This video ends before the point it was to play from.");
    }

    #[test]
    fn the_struggle_meter_needs_eight_seconds_over_one_percent() {
        let mut m = StruggleMeter::default();
        assert!(!m.sample(0, 0, 0));
        assert!(!m.sample(2_000, 1, 50));
        assert!(!m.sample(6_000, 3, 150));
        assert!(m.sample(8_000, 5, 200));
        // Counters that go back start it over.
        assert!(!m.sample(10_000, 0, 10));
    }
}
