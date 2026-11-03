//! A stand-in for a playback: a state the test sets, and a record of what the screen asked of it.
//! The tests and the `states` example share it.

#![allow(dead_code)]

use pley_core::media::EngineStats;
use pley_core::model::{ServerEvent, SubtitleResult};
use pley_player::controller::{ChapterMark, SeekPreview, TrackChoice, UpNextCard};
use pley_player::sleep::SleepChoice;
use pley_player::{Phase, PlayRequest, PlayerClock, PlayerState, SavedPlayback};
use pley_player_ui::Playback;
use pley_subs::Shown;

/// What the screen asked of the playback.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    Play,
    Pause,
    Seek(i64),
    Audio(i64),
    Subtitle(Option<i64>),
    Find,
    Add(String),
    Offset(i64),
    Speed(f32),
    Sleep(SleepChoice),
    KeepWatching,
    Quality(String),
    Skip,
    PlayNext,
    CancelUpNext,
    PlayOriginal,
    Details(bool),
    Retry,
    Close,
}

/// A playback whose state the test writes.
pub struct Fake {
    pub state: PlayerState,
    pub clock: PlayerClock,
    pub shown: Shown,
    pub playing: bool,
    pub preview: Option<SeekPreview>,
    pub stats: EngineStats,
    pub wake: Option<i64>,
    pub calls: Vec<Call>,
    /// What a search finds at once; none: it goes on searching.
    pub finds: Option<Vec<SubtitleResult>>,
}

impl Fake {
    /// A 20-minute movie playing at 1:00, its details and tracks known.
    pub fn movie() -> Fake {
        let state = PlayerState {
            phase: Phase::Playing,
            item_id: 1,
            title: "Sample Movie (2019)".into(),
            method: Some("Direct play".into()),
            quality: "Original".into(),
            ..PlayerState::default()
        };
        Fake {
            state,
            clock: PlayerClock { position_ms: 60_000, duration_ms: 1_200_000, buffered_ms: 90_000 },
            shown: Shown::default(),
            playing: true,
            preview: None,
            stats: EngineStats { player: "webOS media pipeline".into(), ..EngineStats::default() },
            wake: None,
            calls: Vec::new(),
            finds: None,
        }
    }

    /// An episode with two subtitle tracks (English on), two audio tracks and three chapters.
    pub fn episode() -> Fake {
        let mut f = Fake::movie();
        f.state.title = "Fixture Show".into();
        f.state.subtitle = Some("S1 · E2 · The Second".into());
        f.state.kind = Some(pley_core::model::ItemKind::Episode);
        f.state.audio = vec![track(Some(1), "English (AAC 2.0)", true), track(Some(2), "French (AC-3 5.1)", false)];
        f.state.subtitles = vec![
            track(None, "Off", false),
            track(Some(3), "English (SRT)", true),
            track(Some(4), "French (PGS)", false),
        ];
        f.state.chapters = vec![
            chapter(0, 240_000, Some("Intro")),
            chapter(240_000, 960_000, Some("Part 1")),
            chapter(960_000, 1_200_000, Some("Credits")),
        ];
        f
    }

    /// The Up Next card, counting down from `seconds`.
    pub fn card(seconds: i64) -> UpNextCard {
        UpNextCard {
            item_id: 20_003,
            title: "The Third".into(),
            heading: "Up next · S1 · E3".into(),
            image_url: None,
            seconds_left: Some(seconds),
        }
    }

    fn record(&mut self, call: Call) {
        self.calls.push(call);
    }

    /// Whether the screen asked for `call`.
    pub fn asked(&self, call: &Call) -> bool {
        self.calls.contains(call)
    }
}

pub fn track(index: Option<i64>, label: &str, selected: bool) -> TrackChoice {
    TrackChoice { index, label: label.into(), selected, burn_in: false, added: false }
}

pub fn chapter(start_ms: i64, end_ms: i64, title: Option<&str>) -> ChapterMark {
    ChapterMark { title: title.map(str::to_string), start_ms, end_ms, image_url: None }
}

impl Playback for Fake {
    fn tick(&mut self, _now_ms: i64) -> bool {
        false
    }

    fn next_wake(&self) -> Option<i64> {
        self.wake
    }

    fn state(&self) -> &PlayerState {
        &self.state
    }

    fn clock(&self) -> PlayerClock {
        self.clock
    }

    fn subtitles(&self) -> &Shown {
        &self.shown
    }

    fn wants_to_play(&self) -> bool {
        self.playing
    }

    fn seek_preview(&self, _target_ms: i64) -> Option<SeekPreview> {
        self.preview.clone()
    }

    fn engine_stats(&self) -> EngineStats {
        self.stats.clone()
    }

    fn readout_stats(&self) -> EngineStats {
        self.stats.clone()
    }

    fn server_event(&mut self, _event: &ServerEvent) {}

    fn play(&mut self) {
        self.playing = true;
        if self.state.phase == Phase::Paused {
            self.state.phase = Phase::Playing;
        }
        self.record(Call::Play);
    }

    fn pause(&mut self) {
        self.playing = false;
        if self.state.phase == Phase::Playing {
            self.state.phase = Phase::Paused;
        }
        self.record(Call::Pause);
    }

    fn seek_to(&mut self, position_ms: i64) {
        self.clock.position_ms = position_ms;
        self.record(Call::Seek(position_ms));
    }

    fn select_audio(&mut self, index: i64) {
        for t in &mut self.state.audio {
            t.selected = t.index == Some(index);
        }
        self.record(Call::Audio(index));
    }

    fn select_subtitle(&mut self, index: Option<i64>) {
        for t in &mut self.state.subtitles {
            t.selected = t.index == index;
        }
        self.record(Call::Subtitle(index));
    }

    fn find_subtitles(&mut self) {
        self.state.search = match &self.finds {
            Some(found) => pley_player::controller::SubtitleSearch::Found(found.clone()),
            None => pley_player::controller::SubtitleSearch::Searching,
        };
        self.record(Call::Find);
    }

    fn add_found_subtitle(&mut self, result: &SubtitleResult) {
        self.state.search = pley_player::controller::SubtitleSearch::Adding(result.id.clone());
        self.record(Call::Add(result.id.clone()));
    }

    fn set_subtitle_offset(&mut self, ms: i64) {
        self.state.subtitle_offset_ms = ms as i32;
        self.record(Call::Offset(ms));
    }

    fn set_speed(&mut self, rate: f32) {
        self.state.speed = rate;
        self.record(Call::Speed(rate));
    }

    fn set_sleep(&mut self, choice: SleepChoice) {
        self.state.sleep.choice = choice;
        self.record(Call::Sleep(choice));
    }

    fn keep_watching(&mut self) {
        self.state.sleep.seconds_left = None;
        self.record(Call::KeepWatching);
    }

    fn select_quality(&mut self, id: &str) {
        self.record(Call::Quality(id.to_string()));
    }

    fn skip(&mut self) {
        self.state.skip_label = None;
        self.record(Call::Skip);
    }

    fn play_next(&mut self) {
        self.state.up_next = None;
        self.record(Call::PlayNext);
    }

    fn cancel_up_next(&mut self) {
        self.state.up_next = None;
        self.record(Call::CancelUpNext);
    }

    fn play_original(&mut self) {
        self.record(Call::PlayOriginal);
    }

    fn set_details_open(&mut self, open: bool) {
        // As the controller does: closed, the details are let go.
        if !open {
            self.state.details = None;
        }
        self.record(Call::Details(open));
    }

    fn dismiss_notice(&mut self) {
        self.state.notice = None;
    }

    fn retry(&mut self) {
        self.state.phase = Phase::Preparing;
        self.record(Call::Retry);
    }

    fn save_state(&self) -> SavedPlayback {
        SavedPlayback {
            request: PlayRequest::item(self.state.item_id),
            quality: "original".into(),
            playing: self.playing,
        }
    }

    fn close(&mut self) {
        self.record(Call::Close);
    }
}
