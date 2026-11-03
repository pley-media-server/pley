//! The player screen's keys, focus and overlays against a stand-in playback, headless
//! (`TvPlayerScreen.kt`, `TvPlayerKeys.kt`, `TvSeeker.kt`; the TV app's `player.tsx`).

mod common;

use common::{Call, Fake, chapter};
use pley_base::{Cmd, Key, Point};
use pley_core::Settings;
use pley_core::model::{MediaStream, PlaybackMethod, StreamKind, SubtitleResult};
use pley_core::store::SubtitleSize;
use pley_player::Phase;
use pley_player::controller::SubtitleSearch;
use pley_player::details::{DetailRow, PlaybackDetails, RECONNECTING, SubtitleDelivery};
use pley_player_ui::{Btn, Out, Panel, PlayerView, Style, Target, keeps_awake};
use pley_ui::testing::{Frame, Harness, Step};

/// The view, the stand-in and the harness that runs their frames.
struct Player {
    h: Harness,
    view: PlayerView,
    p: Fake,
    settings: Settings,
    outs: Vec<Out>,
}

impl Player {
    fn new(p: Fake) -> Player {
        Player::with(p, Settings::default())
    }

    fn with(p: Fake, settings: Settings) -> Player {
        let mut player = Player { h: Harness::new(), view: PlayerView::new(), p, settings, outs: Vec::new() };
        player.idle();
        player
    }

    fn run(&mut self, steps: &[Step]) -> Vec<Frame> {
        let Player { h, view, p, settings, outs } = self;
        let env = pley_player_ui::FrameEnv { settings, hours24: true, unix_now: 1_700_000_000.0 };
        h.run(steps, &mut |ui| outs.push(view.frame(ui, p, &env)))
    }

    fn idle(&mut self) -> Frame {
        let Player { h, view, p, settings, outs } = self;
        let env = pley_player_ui::FrameEnv { settings, hours24: true, unix_now: 1_700_000_000.0 };
        h.idle(&mut |ui| outs.push(view.frame(ui, p, &env)))
    }

    fn press(&mut self, key: Key) {
        self.run(&[Step::Press(key)]);
    }

    fn wait(&mut self, secs: f64) {
        self.run(&[Step::Wait(secs)]);
    }

    /// What has focus, as the last frames settled it.
    fn focus(&mut self) -> Option<Target> {
        let Player { h, view, p, settings, outs } = self;
        let env = pley_player_ui::FrameEnv { settings, hours24: true, unix_now: 1_700_000_000.0 };
        let mut focus = None;
        h.idle(&mut |ui| {
            focus = view.focus(ui);
            outs.push(view.frame(ui, p, &env));
        });
        focus
    }

    /// Shows the controls on Play/Pause (Down, with them hidden) and moves along the row to `btn`.
    fn open_row_at(&mut self, btn: Btn) {
        self.press(Key::Down);
        for _ in 0..12 {
            if self.focus() == Some(Target::Button(btn)) {
                return;
            }
            self.press(Key::Right);
        }
        panic!("{btn:?} never took focus");
    }

    fn back(&mut self) -> bool {
        let closed = self.view.back(&mut self.p);
        self.idle();
        closed
    }

    fn left(&self) -> bool {
        self.outs.iter().any(|o| o.leave)
    }

    fn texts(&mut self) -> Vec<String> {
        self.idle().list.texts().map(str::to_string).collect()
    }
}

#[test]
fn opens_with_the_controls_hidden_and_ok_shows_them_then_pauses_and_plays() {
    let mut pl = Player::new(Fake::movie());
    assert!(!pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Video));
    // OK with the controls hidden shows them on the scrubber, as Up does; it never pauses there.
    pl.press(Key::Ok);
    assert!(pl.p.calls.is_empty(), "{:?}", pl.p.calls);
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    // OK on the scrubber pauses and plays.
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Pause]);
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Pause, Call::Play]);
}

#[test]
fn up_and_down_bring_the_controls_and_back_hides_them() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Up);
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    // Down from the scrubber goes to Play/Pause.
    pl.press(Key::Down);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::PlayPause)));
    assert!(pl.back());
    assert!(!pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Video));
    // Down with the controls hidden: Play/Pause, with subtitle tracks or without.
    pl.press(Key::Down);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::PlayPause)));
    let mut movie = Player::new(Fake::movie());
    movie.press(Key::Down);
    assert_eq!(movie.focus(), Some(Target::Button(Btn::PlayPause)));
    // Nothing of its own open: Back leaves the player.
    assert!(movie.back());
    assert!(!movie.back());
}

#[test]
fn the_controls_hide_five_seconds_after_the_last_key_unless_paused() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Up);
    pl.wait(4.9);
    assert!(pl.view.controls());
    pl.wait(0.2);
    assert!(!pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Video));
    pl.press(Key::Ok);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Pause]);
    assert!(pl.view.controls());
    pl.wait(8.0);
    assert!(pl.view.controls(), "paused, the controls stay");
}

#[test]
fn left_and_right_wait_for_ok_5_s_at_most_and_ok_plays_on() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Right);
    // With the controls hidden, Left and Right show them on the scrubber, and its preview.
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    pl.press(Key::Right);
    assert!(pl.texts().contains(&"1:20".to_string()), "the scrubber's time isn't the target's");
    pl.wait(4.0);
    assert!(pl.p.calls.is_empty(), "the seek went before OK or 5 s");
    pl.wait(1.0);
    assert_eq!(pl.p.calls, [Call::Seek(80_000)]);
    // Made, the seek lets the controls hide; within 2 s of it, OK on the scrubber confirms it: no
    // pause.
    assert_eq!(pl.focus(), Some(Target::Video));
    pl.press(Key::Up);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Seek(80_000)]);
    // The next OK pauses.
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Seek(80_000), Call::Pause]);
}

#[test]
fn the_play_pause_key_leaves_a_coming_seek_on_its_way() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Right);
    pl.wait(2.0);
    // The key pauses; only OK makes the seek at once.
    pl.press(Key::PlayPause);
    assert_eq!(pl.p.calls, [Call::Pause]);
    pl.press(Key::PlayPause);
    assert_eq!(pl.p.calls, [Call::Pause, Call::Play]);
    // The seek lands by itself 5 s after the last arrow: the Play/Pause presses don't put it off.
    pl.wait(2.8);
    assert_eq!(pl.p.calls, [Call::Pause, Call::Play]);
    pl.wait(0.4);
    assert_eq!(pl.p.calls, [Call::Pause, Call::Play, Call::Seek(70_000)]);
}

#[test]
fn ok_while_a_seek_is_coming_makes_it_at_once() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Left);
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Seek(50_000)]);
    pl.wait(6.0);
    assert_eq!(pl.p.calls, [Call::Seek(50_000)], "the seek isn't made twice");
}

#[test]
fn back_lets_a_coming_seek_go_and_it_plays_on_where_it_was() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Right);
    pl.press(Key::Right);
    assert!(pl.back());
    assert!(pl.view.controls(), "the first Back only lets the seek go");
    pl.wait(6.0);
    assert!(pl.p.calls.is_empty(), "the seek was made: {:?}", pl.p.calls);
    // With the controls showing and nothing coming, Back hides them.
    pl.press(Key::Up);
    assert!(pl.back());
    assert!(!pl.view.controls());
}

#[test]
fn held_left_scrubs_with_the_controls_and_seeks_on_release() {
    let mut pl = Player::new(Fake::movie());
    pl.run(&[Step::Down(Key::Left), Step::Wait(0.1), Step::Repeat(Key::Left)]);
    assert!(pl.view.controls(), "a held key shows the controls");
    for _ in 0..10 {
        pl.run(&[Step::Wait(0.1), Step::Repeat(Key::Left)]);
    }
    assert!(pl.p.calls.is_empty(), "nothing seeks while the key is held");
    pl.run(&[Step::Up(Key::Left)]);
    // Let go, it waits for OK where it got to, as presses do.
    assert!(pl.p.calls.is_empty(), "it seeked on release");
    pl.wait(5.1);
    let Some(Call::Seek(to)) = pl.p.calls.first().cloned() else { panic!("{:?}", pl.p.calls) };
    assert!(to < 50_000, "{to}");
    // The controls wait while a seek is coming, then hide as usual.
    pl.wait(5.5);
    assert!(!pl.view.controls());
}

#[test]
fn media_keys_seek_skip_chapters_and_leave() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::FastForward);
    pl.wait(0.5);
    assert_eq!(pl.p.calls, [Call::Seek(90_000)]);
    // Previous, more than 3 s into a chapter, restarts it.
    pl.p.clock.position_ms = 300_000;
    pl.press(Key::Previous);
    assert_eq!(pl.p.calls.last(), Some(&Call::Seek(240_000)));
    // Next on an episode plays the next one.
    pl.press(Key::Next);
    assert_eq!(pl.p.calls.last(), Some(&Call::PlayNext));
    // A movie goes to the next chapter instead.
    let mut movie = Player::new(Fake::movie());
    movie.p.state.chapters = vec![chapter(0, 100_000, None), chapter(100_000, 1_200_000, None)];
    movie.press(Key::Next);
    assert_eq!(movie.p.calls, [Call::Seek(100_000)]);
    movie.press(Key::Stop);
    assert!(movie.left());
}

#[test]
fn the_subtitles_key_turns_them_off_and_back_on() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Subtitles);
    assert_eq!(pl.p.calls, [Call::Subtitle(None)]);
    assert_eq!(pl.view.toast_text(), Some("Subtitles off"));
    pl.press(Key::Subtitles);
    assert_eq!(pl.p.calls.last(), Some(&Call::Subtitle(Some(3))));
    assert_eq!(pl.view.toast_text(), Some("Subtitles: English (SRT)"));
    // The toast goes after 2 s.
    pl.wait(2.1);
    pl.idle();
    assert_eq!(pl.view.toast_text(), None);
}

#[test]
fn the_audio_key_opens_audio_and_back_returns_to_its_button() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Audio);
    assert_eq!(pl.view.panel(), Some(Panel::Audio));
    pl.wait(0.3);
    assert_eq!(pl.focus(), Some(Target::Panel));
    assert!(pl.view.modal());
    assert!(pl.back());
    assert_eq!(pl.view.panel(), None);
    pl.wait(0.3);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Audio)));
    // With one track, it opens nothing.
    let mut movie = Player::new(Fake::movie());
    movie.p.state.audio = vec![common::track(Some(1), "English", true)];
    movie.press(Key::Audio);
    assert_eq!(movie.view.panel(), None);
}

#[test]
fn choosing_a_subtitle_track_closes_the_panel_and_the_look_keeps_it_open() {
    let mut pl = Player::new(Fake::episode());
    pl.open_row_at(Btn::Subtitles);
    pl.press(Key::Ok);
    pl.wait(0.3);
    assert_eq!(pl.view.panel(), Some(Panel::Subtitles));
    // Focus starts on English, the track playing; French is the next row.
    pl.press(Key::Down);
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Subtitle(Some(4))]);
    assert_eq!(pl.view.panel(), None);
    pl.wait(0.3);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Subtitles)));

    // The look's choices keep the panel open and go to the settings. Focus starts on French now;
    // then Find subtitles…, past the size heading, Small, Normal and Large.
    pl.press(Key::Ok);
    pl.wait(0.3);
    for _ in 0..4 {
        pl.press(Key::Down);
    }
    pl.press(Key::Ok);
    assert_eq!(pl.outs.iter().rev().find_map(|o| o.style), Some(Style::Size(SubtitleSize::Large)));
    assert_eq!(pl.view.panel(), Some(Panel::Subtitles));
}

#[test]
fn skip_takes_focus_while_the_controls_are_hidden_and_ok_skips() {
    let mut pl = Player::new(Fake::episode());
    pl.p.state.skip_label = Some("Skip Intro".into());
    pl.wait(0.2);
    assert_eq!(pl.focus(), Some(Target::Skip));
    assert!(pl.texts().contains(&"Skip Intro".to_string()));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Skip], "OK skips rather than pausing");
    pl.wait(0.3);
    assert_eq!(pl.focus(), Some(Target::Video));
}

#[test]
fn skip_and_a_click_on_the_bar_drop_a_seek_still_waiting_for_ok() {
    let mut pl = Player::new(Fake::episode());
    pl.p.state.skip_label = Some("Skip Intro".into());
    pl.wait(0.2);
    pl.press(Key::FastForward);
    pl.press(Key::Ok);
    pl.wait(6.0);
    assert_eq!(pl.p.calls, [Call::Skip], "the waiting seek undid the skip");

    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Right);
    pl.run(&[Step::Point(Point::new(480.0, 425.0)), Step::Click]);
    pl.wait(6.0);
    let seeks = pl.p.calls.iter().filter(|c| matches!(c, Call::Seek(_))).count();
    assert_eq!(seeks, 1, "the waiting seek undid the click: {:?}", pl.p.calls);
}

#[test]
fn up_from_the_scrubber_goes_to_skip() {
    let mut pl = Player::new(Fake::episode());
    pl.p.state.skip_label = Some("Skip Credits".into());
    pl.wait(0.2);
    pl.press(Key::Up);
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    pl.press(Key::Up);
    assert_eq!(pl.focus(), Some(Target::Skip));
    pl.press(Key::Down);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
}

#[test]
fn up_next_takes_the_screen_and_play_now_has_focus() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Up);
    pl.p.state.up_next = Some(Fake::card(7));
    pl.wait(0.3);
    assert!(!pl.view.controls(), "the controls make way");
    assert_eq!(pl.focus(), Some(Target::PlayNow));
    let texts = pl.texts();
    assert!(texts.contains(&"Up next · S1 · E3".to_string()), "{texts:?}");
    assert!(texts.contains(&"Cancel".to_string()));
    // Up and Down go nowhere; Right goes to Cancel.
    pl.press(Key::Up);
    assert_eq!(pl.focus(), Some(Target::PlayNow));
    pl.press(Key::Right);
    assert_eq!(pl.focus(), Some(Target::Cancel));
    pl.press(Key::Left);
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::PlayNext]);

    // Back cancels it.
    pl.p.state.up_next = Some(Fake::card(5));
    pl.wait(0.3);
    assert!(pl.back());
    assert_eq!(pl.p.calls.last(), Some(&Call::CancelUpNext));
}

#[test]
fn an_error_offers_try_again_and_back_leaves() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::Up);
    pl.p.state.phase =
        Phase::Error { message: "The server couldn't read this file.".into(), can_retry: true, title: None };
    pl.wait(0.3);
    assert!(!pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Error));
    let texts = pl.texts();
    assert!(texts.contains(&"Can't play this item".to_string()), "{texts:?}");
    assert!(texts.contains(&"Try again".to_string()));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Retry]);
    pl.p.state.phase = Phase::Error { message: "Gone.".into(), can_retry: false, title: None };
    pl.idle();
    assert!(!pl.back(), "Back leaves");
}

#[test]
fn an_error_lets_only_stop_of_the_media_keys_act() {
    let mut pl = Player::new(Fake::episode());
    pl.p.state.phase =
        Phase::Error { message: "The server couldn't read this file.".into(), can_retry: true, title: None };
    pl.idle();
    for key in [
        Key::PlayPause,
        Key::Play,
        Key::Pause,
        Key::FastForward,
        Key::Rewind,
        Key::Next,
        Key::Previous,
        Key::Subtitles,
        Key::Audio,
    ] {
        pl.press(key);
    }
    pl.wait(0.5);
    assert!(pl.p.calls.is_empty(), "{:?}", pl.p.calls);
    assert_eq!(pl.view.toast_text(), None);
    assert_eq!(pl.focus(), Some(Target::Error));
    assert!(!pl.left());
    pl.press(Key::Stop);
    assert!(pl.left(), "Stop didn't leave");
}

#[test]
fn the_end_and_a_stop_by_the_server_leave() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.phase = Phase::Ended;
    pl.idle();
    assert!(pl.left());

    let mut stopped = Player::new(Fake::movie());
    stopped.p.state.stopped_by_server = true;
    stopped.p.state.phase =
        Phase::Error { message: "An administrator stopped this.".into(), can_retry: false, title: None };
    stopped.idle();
    assert!(stopped.texts().contains(&"Playback stopped".to_string()));
    stopped.wait(19.0);
    assert!(!stopped.left());
    stopped.wait(1.5);
    assert!(stopped.left(), "it leaves 20 s after the stop");
}

#[test]
fn the_sleep_notice_takes_focus_and_keeps_watching() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.sleep.seconds_left = Some(28);
    pl.wait(0.2);
    assert_eq!(pl.focus(), Some(Target::Keep));
    assert!(pl.texts().contains(&"Pausing in 28 s".to_string()));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::KeepWatching]);
}

#[test]
fn the_pointer_brings_the_controls_and_a_click_on_the_bar_seeks_there() {
    let mut pl = Player::new(Fake::movie());
    pl.run(&[Step::Point(Point::new(480.0, 300.0))]);
    assert!(pl.view.controls());
    pl.run(&[Step::Point(Point::new(480.0, 425.0)), Step::Click]);
    let to = (1_200_000.0 * (480.0 - 48.0) / 864.0) as i64;
    let seeks: Vec<_> = pl.p.calls.iter().filter_map(|c| if let Call::Seek(t) = c { Some(*t) } else { None }).collect();
    assert_eq!(seeks.len(), 1, "{:?}", pl.p.calls);
    assert!((seeks[0] - to).abs() <= 1, "{} {to}", seeks[0]);
}

#[test]
fn the_details_are_asked_for_while_info_or_the_readout_shows() {
    let mut pl = Player::new(Fake::movie());
    pl.settings.playback_details = true;
    pl.idle();
    assert!(pl.p.asked(&Call::Details(true)));
    let texts = pl.texts();
    assert!(!texts.iter().any(|t| t == "Playback details"), "the compact readout has no title");
    // The controls hide the readout: the details are let go.
    pl.press(Key::Up);
    assert_eq!(pl.p.calls.last(), Some(&Call::Details(false)));
}

#[test]
fn the_top_bar_says_what_plays_and_when_it_ends_while_paused() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Ok);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Pause]);
    pl.wait(0.3);
    let texts = pl.texts();
    for want in ["Fixture Show", "S1 · E2 · The Second", "Direct play", "1:00", "−19:00"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    assert!(texts.iter().any(|t| t.starts_with("Ends at ")), "{texts:?}");
}

#[test]
fn coming_back_after_the_tv_hid_the_app_starts_clean_with_the_controls() {
    let mut pl = Player::new(Fake::episode());
    pl.wait(4.5);
    pl.press(Key::Audio);
    assert!(pl.view.panel().is_some());
    pl.view.reopened();
    pl.wait(0.3);
    assert_eq!(pl.view.panel(), None);
    // It comes back paused: the controls show on the scrubber, as a pause shows them.
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    // The title banner shown as the item began doesn't come again.
    pl.p.state.phase = pley_player::Phase::Paused;
    assert!(pl.back());
    pl.wait(0.5);
    assert!(!pl.texts().iter().any(|t| t == "Fixture Show"), "the banner came again");
}

#[test]
fn the_request_keeps_what_the_page_asked_for() {
    let r = pley_shell::PlayRequest {
        item_id: 7,
        file_id: Some(70),
        start_ms: Some(12_000),
        from_start: false,
        audio_index: Some(2),
        subtitle_index: Some(-1),
        queue: Some(pley_shell::PlayQueue::Playlist(3)),
        file: None,
        pressed_at: 1.5,
    };
    let p = pley_player_ui::player_request(&r);
    assert_eq!((p.item_id, p.file_id, p.start_ms), (7, Some(70), Some(12_000)));
    assert_eq!((p.audio_index, p.subtitle_index), (Some(2), Some(-1)));
    assert_eq!(p.queue, Some(pley_player::Queue::Playlist(3)));
}

#[test]
fn find_subtitles_searches_adds_and_back_returns_to_subtitles() {
    let mut pl = Player::new(Fake::episode());
    let found =
        SubtitleResult { id: "a".into(), name: "Release.One".into(), downloads: 3, ..SubtitleResult::default() };
    pl.p.finds = Some(vec![found]);
    pl.open_row_at(Btn::Subtitles);
    pl.press(Key::Ok);
    pl.wait(0.3);
    // From English, the track playing: French, then Find subtitles….
    pl.press(Key::Down);
    pl.press(Key::Down);
    pl.press(Key::Ok);
    assert_eq!(pl.view.panel(), Some(Panel::Find));
    assert!(pl.p.asked(&Call::Find));
    pl.wait(0.3);
    assert!(pl.texts().contains(&"Release.One".to_string()));
    // Back goes to Subtitles, on Find subtitles… again.
    assert!(pl.back());
    assert_eq!(pl.view.panel(), Some(Panel::Subtitles));
    pl.wait(0.3);
    pl.press(Key::Ok);
    assert_eq!(pl.view.panel(), Some(Panel::Find));
    pl.wait(0.3);
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls.last(), Some(&Call::Add("a".into())));
    assert_eq!(pl.view.panel(), Some(Panel::Find), "the search stays while it adds");
    // Added: the panel closes, sliding out as it was (never "Searching…"), and focus goes back
    // to Subtitles.
    pl.p.state.search = SubtitleSearch::Idle;
    let frames = pl.run(&[Step::Wait(0.3)]);
    assert!(frames.iter().any(|f| f.list.texts().any(|t| t == "Release.One")), "it slides out");
    assert!(frames.iter().all(|f| !f.list.texts().any(|t| t == "Searching…")));
    assert_eq!(pl.view.panel(), None);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Subtitles)));
}

#[test]
fn while_it_prepares_left_right_and_ok_wait_but_media_keys_act() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.phase = Phase::Preparing;
    pl.idle();
    pl.press(Key::Right);
    pl.press(Key::Ok);
    pl.wait(0.5);
    assert!(pl.p.calls.is_empty(), "{:?}", pl.p.calls);
    pl.press(Key::PlayPause);
    assert_eq!(pl.p.calls, [Call::Pause]);
}

#[test]
fn fast_forward_and_rewind_step_thirty_seconds_with_the_badge() {
    let mut pl = Player::new(Fake::movie());
    pl.press(Key::FastForward);
    assert!(pl.texts().contains(&"+30 s".to_string()));
    pl.press(Key::Rewind);
    pl.press(Key::Rewind);
    assert!(pl.texts().contains(&"−30 s".to_string()));
    pl.wait(0.5);
    assert_eq!(pl.p.calls, [Call::Seek(30_000)]);
}

#[test]
fn info_and_menu_bring_the_controls_on_skip_when_it_shows() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Info);
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    assert!(pl.back());
    pl.p.state.skip_label = Some("Skip Intro".into());
    pl.wait(0.2);
    assert_eq!(pl.focus(), Some(Target::Skip));
    pl.press(Key::Menu);
    assert!(pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Skip));
}

/// Where the first text drawn as `text` is.
fn text_at(frame: &Frame, text: &str) -> Option<Point> {
    frame.list.cmds.iter().find_map(|c| match c {
        Cmd::Text { origin, layout, .. } if &*layout.text == text => Some(*origin),
        _ => None,
    })
}

#[test]
fn info_scrolls_with_up_and_down_and_back_returns_to_its_button() {
    let mut pl = Player::new(Fake::movie());
    let rows = (1..=24).map(|n| DetailRow { label: format!("Row {n}"), value: "A value".into() }).collect();
    pl.p.state.details = Some(PlaybackDetails {
        method: "Transcoding".into(),
        why: None,
        pipeline: None,
        rows,
        playback_method: PlaybackMethod::Transcode,
        file: None,
        video: None,
        audio: None,
        subtitle: None,
        subtitle_delivery: None,
        transcode: None,
    });
    pl.open_row_at(Btn::Info);
    pl.press(Key::Ok);
    assert_eq!(pl.view.panel(), Some(Panel::Info));
    pl.wait(0.4);
    assert!(pl.p.asked(&Call::Details(true)));
    let top = text_at(&pl.idle(), "How it plays").expect("the first section");
    pl.press(Key::Down);
    pl.wait(0.4);
    let moved = text_at(&pl.idle(), "How it plays").expect("the first section");
    assert!((top.y - moved.y - 96.0).abs() < 0.01, "{top:?} {moved:?}");
    pl.press(Key::Up);
    pl.press(Key::Up);
    pl.wait(0.4);
    assert_eq!(text_at(&pl.idle(), "How it plays"), Some(top), "never above the top");
    assert!(pl.back());
    pl.wait(0.3);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Info)));
    assert_eq!(pl.p.calls.last(), Some(&Call::Details(false)));
}

#[test]
fn sync_steps_a_tenth_of_a_second_and_ok_puts_it_back() {
    let mut pl = Player::new(Fake::episode());
    pl.p.state.subtitle_sync = true;
    pl.open_row_at(Btn::Subtitles);
    pl.press(Key::Ok);
    pl.wait(0.3);
    // From English: Off, then Sync.
    pl.press(Key::Up);
    pl.press(Key::Up);
    pl.press(Key::Right);
    assert_eq!(pl.p.calls.last(), Some(&Call::Offset(100)));
    pl.press(Key::Right);
    pl.press(Key::Left);
    assert_eq!(pl.p.calls.last(), Some(&Call::Offset(100)));
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls.last(), Some(&Call::Offset(0)));
    assert_eq!(pl.view.panel(), Some(Panel::Subtitles), "Sync keeps the panel open");
}

#[test]
fn paused_by_the_sleep_timer_the_player_leaves_after_ten_minutes() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.sleep.fired = true;
    pl.p.state.phase = Phase::Paused;
    pl.p.playing = false;
    pl.idle();
    pl.wait(599.0);
    assert!(!pl.left());
    pl.wait(1.5);
    assert!(pl.left());
}

#[test]
fn notices_and_quality_notes_become_toasts() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.notice = Some("Subtitles added".into());
    pl.p.state.notice_id = 1;
    pl.idle();
    assert_eq!(pl.view.toast_text(), Some("Subtitles added"));
    assert_eq!(pl.p.state.notice, None, "a notice is said once");
    pl.p.state.note = Some("The original didn't play smoothly on this device.".into());
    pl.p.state.can_play_original = true;
    pl.idle();
    assert_eq!(
        pl.view.toast_text(),
        Some("The original didn't play smoothly on this device. Play original is in Quality.")
    );
}

/// Playback details with `rows` and nothing else known.
fn details(rows: Vec<DetailRow>) -> PlaybackDetails {
    PlaybackDetails {
        method: "Transcoding".into(),
        why: None,
        pipeline: None,
        rows,
        playback_method: PlaybackMethod::Transcode,
        file: None,
        video: None,
        audio: None,
        subtitle: None,
        subtitle_delivery: None,
        transcode: None,
    }
}

#[test]
fn the_screen_saver_is_kept_away_only_while_it_plays() {
    assert!(keeps_awake(&Phase::Playing, true));
    assert!(keeps_awake(&Phase::Buffering, true));
    assert!(keeps_awake(&Phase::Preparing, true));
    assert!(!keeps_awake(&Phase::Paused, false));
    assert!(!keeps_awake(&Phase::Buffering, false), "paused while it buffers");
    assert!(!keeps_awake(&Phase::Ended, true));
    assert!(!keeps_awake(&Phase::Error { message: "Gone.".into(), can_retry: true, title: None }, true));
}

#[test]
fn hiding_the_controls_puts_focus_on_the_video_not_the_sleep_notice() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.sleep.seconds_left = Some(28);
    pl.wait(0.2);
    assert_eq!(pl.focus(), Some(Target::Keep), "the notice takes focus as it comes");
    pl.press(Key::Up);
    assert!(pl.view.controls());
    assert!(pl.back());
    assert_eq!(pl.focus(), Some(Target::Video));
    pl.press(Key::Up);
    pl.wait(5.5);
    assert!(!pl.view.controls());
    assert_eq!(pl.focus(), Some(Target::Video));
    // OK is the video's again, not Keep watching's: it shows the controls on the scrubber.
    pl.press(Key::Ok);
    assert!(pl.p.calls.is_empty(), "{:?}", pl.p.calls);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
}

#[test]
fn up_next_with_reduce_motion_says_play_now_then_the_seconds() {
    let mut pl = Player::new(Fake::episode());
    pl.h.state.set_reduced_motion(true);
    pl.p.state.up_next = Some(Fake::card(7));
    pl.wait(0.3);
    let texts = pl.texts();
    assert!(texts.contains(&"Play now".to_string()), "{texts:?}");
    assert!(texts.contains(&"Next in 7".to_string()), "{texts:?}");
}

#[test]
fn up_next_shows_the_placeholder_until_its_still_is_in() {
    let placeholders = |pl: &mut Player| {
        let f = pl.idle();
        f.list.cmds.iter().filter(|c| matches!(c, Cmd::Quad { rect, .. } if rect.w == 300.0)).count()
    };
    let mut pl = Player::new(Fake::episode());
    pl.p.state.up_next = Some(Fake::card(7));
    pl.wait(0.3);
    assert_eq!(placeholders(&mut pl), 1, "none: the gradient for good");
    let mut card = Fake::card(7);
    card.image_url = Some("still".into());
    pl.p.state.up_next = Some(card);
    pl.wait(0.1);
    assert_eq!(placeholders(&mut pl), 1, "until the still is in");
    pl.h.images.ready("still", 480, 270, 0.0);
    pl.wait(0.3);
    assert_eq!(placeholders(&mut pl), 0);
}

#[test]
fn reconnecting_shows_under_the_ring_only() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.reconnecting = true;
    pl.wait(1.0);
    assert!(!pl.texts().iter().any(|t| t == RECONNECTING), "playing on from what it holds");
    pl.p.state.phase = Phase::Buffering;
    pl.wait(0.2);
    assert!(!pl.texts().iter().any(|t| t == RECONNECTING), "not before the ring's delay");
    pl.wait(0.5);
    assert!(pl.texts().iter().any(|t| t == RECONNECTING));
}

#[test]
fn info_slides_out_as_it_was() {
    let mut pl = Player::new(Fake::movie());
    pl.p.state.details = Some(details(vec![DetailRow { label: "Video".into(), value: "HEVC → H.264".into() }]));
    pl.open_row_at(Btn::Info);
    pl.press(Key::Ok);
    pl.wait(0.4);
    assert!(pl.texts().contains(&"How it plays".to_string()));
    assert!(pl.back());
    // Closed, the details are let go, but the panel slides out with what it showed.
    assert!(pl.p.state.details.is_none());
    let frames = pl.run(&[Step::Wait(0.4)]);
    let shown: Vec<_> = frames.iter().filter(|f| f.list.texts().any(|t| t == "Playback details")).collect();
    assert!(!shown.is_empty(), "it slides out");
    assert!(shown.iter().all(|f| f.list.texts().any(|t| t == "How it plays")));
    assert!(shown.iter().all(|f| !f.list.texts().any(|t| t == "Asking the server…")));
    assert!(!pl.texts().iter().any(|t| t == "Playback details"), "and is gone");
}

#[test]
fn info_says_the_tvs_network_speed_beside_the_servers() {
    let mut pl = Player::new(Fake::movie());
    let speed = DetailRow { label: "Speed".into(), value: "2.5× real time · 36 s ready".into() };
    pl.p.state.details = Some(details(vec![speed]));
    pl.p.stats.network_bps = Some(120_000_000);
    pl.open_row_at(Btn::Info);
    pl.press(Key::Ok);
    pl.wait(0.4);
    let texts = pl.texts();
    let at = |t: &str| texts.iter().position(|x| x == t);
    assert!(at("Speed") < at("TV network speed") && at("TV network speed") < at("Quality"), "{texts:?}");
    assert!(texts.contains(&"120 Mbps".to_string()));
    // Not known (no /proc/net/dev in the TV's jail, the simulator): no row.
    pl.p.stats.network_bps = None;
    pl.wait(0.1);
    assert!(!pl.texts().contains(&"TV network speed".to_string()));
}

#[test]
fn the_readout_has_one_bitrate_line() {
    let mut pl = Player::new(Fake::movie());
    let rows = vec![
        DetailRow { label: "Video".into(), value: "HEVC → H.264".into() },
        DetailRow { label: "Bitrate".into(), value: "40 Mbps → 8 Mbps".into() },
    ];
    pl.p.state.details = Some(details(rows));
    pl.p.stats.bitrate_kbps = Some(8_000);
    pl.settings.playback_details = true;
    pl.idle();
    let texts = pl.texts();
    assert_eq!(texts.iter().filter(|t| *t == "Bitrate").count(), 1, "{texts:?}");
    assert!(texts.contains(&"Video".to_string()), "{texts:?}");
}

#[test]
fn info_names_the_subtitle_language_with_its_title_and_flags() {
    let mut pl = Player::new(Fake::movie());
    let sub = MediaStream {
        kind: StreamKind::Subtitle,
        codec: "subrip".into(),
        language: Some("eng".into()),
        title: Some("SDH".into()),
        forced: true,
        text_based: Some(true),
        ..MediaStream::default()
    };
    let mut d = details(Vec::new());
    d.subtitle = Some(sub);
    d.subtitle_delivery = Some(SubtitleDelivery::ServerText);
    pl.p.state.details = Some(d);
    pl.open_row_at(Btn::Info);
    pl.press(Key::Ok);
    pl.wait(0.4);
    let texts = pl.texts();
    for want in ["English", "SDH", "Forced", "Shown"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    assert!(!texts.iter().any(|t| t == "eng"), "{texts:?}");
}

#[test]
fn the_title_shows_for_4_s_as_each_item_begins_unless_the_controls_show() {
    let mut pl = Player::new(Fake::episode());
    let texts = pl.texts();
    assert!(texts.iter().any(|t| t == "Fixture Show"), "no banner: {texts:?}");
    assert!(texts.iter().any(|t| t == "S1 · E2 · The Second"));
    pl.wait(4.5);
    assert!(!pl.texts().iter().any(|t| t == "Fixture Show"), "the banner stays past 4 s");
    // The next item, playing by itself: the banner again; the controls coming take its place.
    pl.p.state.item_id = 2;
    pl.p.state.title = "Next Show".into();
    pl.idle();
    pl.wait(0.5);
    assert!(pl.texts().iter().any(|t| t == "Next Show"), "no banner for the next item");
    pl.press(Key::Up);
    pl.wait(0.5);
    assert!(pl.back());
    pl.wait(0.5);
    assert!(!pl.view.controls());
    assert!(!pl.texts().iter().any(|t| t == "Next Show"), "the banner came back after the controls");
}

/// How opaque the black over the video is, if any.
fn black(frame: &Frame) -> Option<u8> {
    frame.list.cmds.iter().find_map(|c| match c {
        Cmd::Fill { rect, color, .. }
            if rect.w >= 960.0 && rect.h >= 540.0 && (color.r, color.g, color.b) == (0, 0, 0) =>
        {
            Some(color.a)
        }
        _ => None,
    })
}

#[test]
fn the_first_picture_fades_in_from_black_at_once_with_reduce_motion() {
    let mut pl = Player::new(Fake::movie());
    assert_eq!(black(&pl.idle()), Some(255), "the video wasn't covered in black before its first picture");
    pl.p.state.first_frame_ms = Some(900);
    pl.idle();
    pl.wait(0.1);
    let half = black(&pl.idle()).expect("the picture was there at once");
    assert!(half > 0 && half < 255, "{half}");
    pl.wait(0.2);
    assert_eq!(black(&pl.idle()), None, "still black after 200 ms");
    // A new stream: black until its picture, which fades in again.
    pl.p.state.first_frame_ms = None;
    assert_eq!(black(&pl.idle()), Some(255));
    pl.p.state.first_frame_ms = Some(400);
    assert!(black(&pl.idle()).is_some_and(|a| a > 0));

    let mut reduced = Player::new(Fake::movie());
    reduced.h.state.set_reduced_motion(true);
    reduced.p.state.first_frame_ms = Some(900);
    assert_eq!(black(&reduced.idle()), None, "Reduce motion: the picture at once");
}

#[test]
fn the_title_makes_way_for_playback_details() {
    let settings = Settings { playback_details: true, ..Settings::default() };
    let mut pl = Player::with(Fake::episode(), settings);
    pl.wait(0.5);
    let texts = pl.texts();
    assert!(!texts.iter().any(|t| t == "Fixture Show"), "the banner shows over the details: {texts:?}");
}

#[test]
fn up_while_a_seek_waits_opens_the_chapters_as_cards_and_ok_goes_to_one() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Right);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    pl.press(Key::Up);
    assert_eq!(pl.focus(), Some(Target::Chapters));
    let texts = pl.texts();
    for want in ["Intro", "Part 1", "Credits", "4:00"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    // The seek gave way to the cards; Right goes to the next chapter, the scrubber showing its start.
    pl.wait(6.0);
    assert!(pl.p.calls.is_empty(), "the seek went on: {:?}", pl.p.calls);
    assert!(pl.view.controls(), "the controls hid under the cards");
    pl.press(Key::Right);
    assert!(pl.texts().contains(&"16:00".to_string()), "the scrubber isn't at Credits' start");
    pl.press(Key::Right);
    pl.press(Key::Ok);
    assert_eq!(pl.p.calls, [Call::Seek(960_000)]);
    assert_eq!(pl.focus(), Some(Target::Scrubber));
    assert!(!pl.texts().contains(&"Part 1".to_string()), "the cards stayed");
}

#[test]
fn the_chapters_button_opens_the_cards_and_down_or_back_closes_them() {
    let mut pl = Player::new(Fake::episode());
    pl.press(Key::Down);
    for _ in 0..8 {
        if pl.focus() == Some(Target::Button(Btn::Chapters)) {
            break;
        }
        pl.press(Key::Left);
    }
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Chapters)));
    pl.press(Key::Ok);
    assert_eq!(pl.focus(), Some(Target::Chapters));
    assert!(pl.view.panel().is_none(), "a side panel opened");
    pl.press(Key::Down);
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Chapters)), "Down didn't go back to the button");
    pl.press(Key::Ok);
    assert_eq!(pl.focus(), Some(Target::Chapters));
    assert!(pl.back(), "Back left the player");
    assert!(pl.view.controls(), "Back hid the controls with the cards");
    assert_eq!(pl.focus(), Some(Target::Button(Btn::Chapters)));
    assert!(pl.p.calls.is_empty(), "{:?}", pl.p.calls);
}
