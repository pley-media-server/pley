//! The music player (packages/tv/src/player/music.ts, Android's
//! `MusicPlayback`) on `SimEngine` and the pretend server.

mod common;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use pley_base::Waker;
use pley_core::CapsReport;
use pley_core::model::{Control, DeviceKind, ItemKind, ItemSummary, PlaybackMethod, PlaybackMode, ServerEvent};
use pley_core::task::Tasks;
use pley_player::Reporter;
use pley_player::env::PlayerEnv;
use pley_player::music::{MusicError, MusicSession, shuffled};
use pley_player::server::PlayerServer;
use pley_player::sim::{SIM_LOAD_MS, SimClock, SimEngine, SimHandle};
use pley_player::sleep::SleepChoice;

fn track(id: i64) -> ItemSummary {
    ItemSummary {
        id,
        kind: ItemKind::Track,
        title: format!("Track {id}"),
        duration_ms: Some(200_000),
        ..ItemSummary::default()
    }
}

/// A track plays as it is (`m<id>`), or converted when asked.
fn answer(req: &pley_core::model::PlaybackRequest) -> pley_core::model::PlaybackInfo {
    info(|i| {
        i.session_id = format!("m{}", req.item_id);
        i.duration_ms = 200_000;
        if req.mode == PlaybackMode::Transcode {
            i.method = PlaybackMethod::Transcode;
            i.hls = true;
            i.url = format!("/api/media/pley_mk_{}/index.m3u8", req.item_id);
        } else {
            i.url = format!("/api/media/pley_mk_{}/file.flac", req.item_id);
        }
    })
}

struct Music {
    fake: Arc<Fake>,
    clock: SimClock,
    sim: SimHandle,
    m: MusicSession,
    now: i64,
}

impl Music {
    fn new() -> Music {
        let fake = Fake::new(item(file()), answer);
        let clock = SimClock::manual();
        let (engine, sim) = SimEngine::new(clock.clone());
        let server: Arc<dyn PlayerServer> = fake.clone();
        let caps = Arc::new(|| Some(CapsReport { caps: lg(), lines: Vec::new(), dv_containers: Vec::new() }));
        let env = PlayerEnv::new(server, Tasks::with_threads(2, Waker::noop()), Reporter::new(), caps, "webos-abc");
        let mut m = MusicSession::new(env, Box::new(engine));
        // The same order each run: always the last of what is left.
        m.set_random(|| 0.999);
        Music { fake, clock, sim, m, now: 0 }
    }

    fn settle(&mut self) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            self.m.tick(self.now);
            if !self.m.busy() {
                break;
            }
            assert!(Instant::now() < until, "a server call never came back");
            thread::sleep(Duration::from_millis(1));
        }
        self.m.tick(self.now);
    }

    fn advance(&mut self, ms: i64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 50).min(end);
            self.clock.set(self.now);
            self.settle();
        }
    }

    fn calls(&self) -> Vec<String> {
        self.fake.calls()
    }
}

#[test]
fn shuffling_keeps_the_started_track_first() {
    let list = [1, 2, 3, 4, 5];
    let mut n = 0.0;
    let order = shuffled(&list, 2, || {
        n += 0.3;
        n % 1.0
    });
    assert_eq!(order[0], 3);
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(sorted, list);
}

#[test]
fn plays_a_queue_in_order_one_session_per_track_with_musics_own_client_id() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2), track(3)], 0, false);
    w.settle();
    let req = &w.fake.requests()[0];
    assert_eq!(
        (req.item_id, req.client_id.as_deref(), req.keyed_urls, req.mode),
        (1, Some("webos-abc-music"), true, PlaybackMode::Auto)
    );
    assert_eq!(w.sim.loads()[0].url, "http://nas:7539/api/media/pley_mk_1/file.flac");
    w.advance(SIM_LOAD_MS);
    let now = w.m.now_playing().unwrap();
    assert_eq!((now.index, now.count, now.playing, now.loading), (0, 3, true, false));
    // The track ends: its last place, its session's end, then the next track by itself.
    w.sim.end();
    w.settle();
    assert_eq!(w.calls()[1..3], ["progress stopped 200000", "delete m1"]);
    assert_eq!(w.fake.requests().last().map(|r| r.item_id), Some(2));
    assert_eq!(w.m.now_playing().map(|n| n.index), Some(1));
}

#[test]
fn after_the_queue_play_starts_its_last_track_again() {
    let mut w = Music::new();
    w.m.play(vec![track(1)], 0, false);
    w.settle();
    w.advance(SIM_LOAD_MS);
    w.sim.end();
    w.settle();
    assert!(w.m.now_playing().unwrap().ended);
    assert_eq!(w.m.position_ms(), 200_000);
    w.m.toggle();
    w.settle();
    assert_eq!(w.fake.requests().iter().map(|r| r.item_id).collect::<Vec<_>>(), [1, 1]);
}

#[test]
fn previous_restarts_a_track_past_3_s_else_plays_the_one_before() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2)], 1, false);
    w.settle();
    w.advance(SIM_LOAD_MS + 5_000);
    w.m.previous();
    w.settle();
    assert_eq!(w.sim.commands().last().map(String::as_str), Some("seek 0"));
    w.advance(1_000);
    w.m.previous();
    w.settle();
    assert_eq!(w.fake.requests().last().map(|r| r.item_id), Some(1));
}

#[test]
fn shuffle_on_puts_the_track_playing_first_and_off_goes_back_to_the_order_at_the_same_track() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2), track(3), track(4)], 2, false);
    w.settle();
    w.m.set_shuffle(true);
    assert_eq!(w.m.list()[0].id, 3);
    assert_eq!(w.m.now_playing().map(|n| (n.index, n.shuffle)), Some((0, true)));
    w.m.set_shuffle(false);
    assert_eq!(w.m.list().iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(w.m.now_playing().map(|n| n.index), Some(2));
}

#[test]
fn a_track_that_fails_plays_as_a_conversion_once_then_in_new_sessions_then_cant_be_played() {
    let mut w = Music::new();
    w.m.play(vec![track(1)], 0, false);
    w.settle();
    w.advance(SIM_LOAD_MS);
    for _ in 0..4 {
        w.sim.fail(3, "decode");
        w.settle();
        w.advance(1_000 + SIM_LOAD_MS);
    }
    let modes: Vec<PlaybackMode> = w.fake.requests().iter().map(|r| r.mode).collect();
    assert_eq!(modes, [PlaybackMode::Auto, PlaybackMode::Transcode, PlaybackMode::Transcode, PlaybackMode::Transcode]);
    assert_eq!(w.m.now_playing().and_then(|n| n.error), Some(MusicError::Unplayable));
    assert_eq!(MusicError::Unplayable.text(), "This track can't be played.");
}

#[test]
fn reports_every_10_s_while_playing_paused_on_pause_and_every_30_s_paused() {
    let mut w = Music::new();
    w.m.play(vec![track(1)], 0, false);
    w.settle();
    w.advance(10_000);
    let progress = |w: &Music| w.calls().into_iter().filter(|c| c.starts_with("progress")).collect::<Vec<_>>();
    assert_eq!(progress(&w), ["progress playing 9700"]);
    w.m.toggle();
    w.settle();
    assert_eq!(progress(&w).last().map(String::as_str), Some("progress paused 9700"));
    w.advance(20_000);
    assert_eq!(progress(&w).len(), 2);
    w.advance(10_000);
    assert_eq!(progress(&w).len(), 3);
}

#[test]
fn yields_to_a_video_and_play_starts_a_new_session_where_it_was() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2)], 0, false);
    w.settle();
    w.advance(SIM_LOAD_MS + 4_000);
    w.m.yield_to_video();
    w.settle();
    assert_eq!(w.calls()[1..], ["progress stopped 4000", "delete m1"]);
    assert!(!w.m.now_playing().unwrap().playing);
    w.m.toggle();
    w.settle();
    let last = w.fake.requests().last().cloned().unwrap();
    assert_eq!((last.item_id, last.start_ms), (1, Some(4_000)));
}

#[test]
fn a_sleep_timer_ending_with_the_track_leaves_the_next_one_waiting() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2)], 0, false);
    w.settle();
    w.advance(SIM_LOAD_MS);
    for _ in 0..6 {
        w.m.step_sleep();
    }
    assert_eq!(w.m.sleep().choice, SleepChoice::EndOfItem);
    w.sim.end();
    w.settle();
    let now = w.m.now_playing().unwrap();
    assert_eq!((now.index, now.playing), (1, false));
    assert_eq!(w.fake.requests().len(), 1);
}

#[test]
fn a_session_the_server_stops_stops_the_music() {
    let mut w = Music::new();
    w.m.play(vec![track(1)], 0, false);
    w.settle();
    w.m.server_event(&ServerEvent::SessionStopped { session_id: "m1".into(), message: None, by_admin: true });
    assert_eq!(w.m.now_playing().and_then(|n| n.error), Some(MusicError::Stopped));
    w.m.stop();
    assert!(w.m.now_playing().is_none());
}

fn command(session: &str, command: Control, position_ms: Option<i64>, offset_ms: Option<i64>) -> ServerEvent {
    ServerEvent::PlayerCommand { session_id: session.into(), command, position_ms, offset_ms, stream_index: None }
}

#[test]
fn music_says_it_obeys_the_whole_remote_and_does_as_its_buttons_reporting_at_once() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2), track(3)], 1, false);
    w.settle();
    w.advance(SIM_LOAD_MS + 10_000);
    let req = w.fake.requests()[0].clone();
    assert_eq!(
        req.controls,
        [Control::Play, Control::Pause, Control::Seek, Control::Skip, Control::Next, Control::Previous]
    );
    assert_eq!(req.device_kind, Some(DeviceKind::Tv));
    assert_eq!(w.m.session_id(), Some("m2"));
    let progress = |w: &Music| w.calls().into_iter().filter(|c| c.starts_with("progress")).collect::<Vec<_>>();
    let before = progress(&w).len();

    // Someone else's session: nothing.
    w.m.server_event(&command("m9", Control::Pause, None, None));
    w.settle();
    assert!(w.m.now_playing().unwrap().playing);
    // A pause reports as any pause does, and a pause on a pause says so again.
    w.m.server_event(&command("m2", Control::Pause, None, None));
    w.settle();
    assert!(!w.m.now_playing().unwrap().playing);
    w.m.server_event(&command("m2", Control::Pause, None, None));
    w.settle();
    assert_eq!(progress(&w)[before..], ["progress paused 10000", "progress paused 10000"]);
    w.m.server_event(&command("m2", Control::Play, None, None));
    w.settle();
    assert!(w.m.now_playing().unwrap().playing);
    assert_eq!(progress(&w)[before + 2..], ["progress playing 10000"]);
    // A seek in place reports where it went at once; a skip counts from where it is.
    w.m.server_event(&command("m2", Control::Seek, Some(120_000), None));
    w.settle();
    assert_eq!(progress(&w)[before + 3..], ["progress playing 120000"]);
    w.m.server_event(&command("m2", Control::Skip, None, Some(-30_000)));
    w.settle();
    assert_eq!(w.m.position_ms(), 90_000);
    // Previous past 3 s starts the track again; within them, the one before.
    w.m.server_event(&command("m2", Control::Previous, None, None));
    w.settle();
    assert_eq!((w.m.now_playing().unwrap().index, w.m.position_ms()), (1, 0));
    w.m.server_event(&command("m2", Control::Previous, None, None));
    w.settle();
    assert_eq!(w.m.now_playing().unwrap().index, 0);
    assert_eq!(w.fake.requests().last().map(|r| r.item_id), Some(1));
    // Next plays the next, with a session of its own; the old one's commands do nothing.
    w.m.server_event(&command("m1", Control::Next, None, None));
    w.settle();
    assert_eq!(w.m.now_playing().unwrap().index, 1);
    w.m.server_event(&command("m1", Control::Next, None, None));
    w.settle();
    assert_eq!(w.m.now_playing().unwrap().index, 1);
}

#[test]
fn music_has_no_tracks_for_the_remote_to_pick() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2)], 0, false);
    w.settle();
    w.advance(SIM_LOAD_MS + 10_000);
    let req = w.fake.requests()[0].clone();
    assert!(!req.controls.contains(&Control::Audio) && !req.controls.contains(&Control::Subtitles));
    let reports = w.fake.reports();
    assert!(!reports.is_empty(), "no report in 10 s");
    assert!(reports.iter().all(|r| r.audio_index.is_none() && r.subtitle_index.is_none()), "{reports:?}");
    // A track picked anyway changes nothing, and says nothing.
    let before = w.calls().len();
    let pick = ServerEvent::PlayerCommand {
        session_id: "m1".into(),
        command: Control::Audio,
        position_ms: None,
        offset_ms: None,
        stream_index: Some(1),
    };
    w.m.server_event(&pick);
    w.settle();
    assert_eq!(w.calls().len(), before);
    assert_eq!(w.m.now_playing().map(|n| (n.index, n.playing)), Some((0, true)));
}

/// A track opening counts as playing, as Android's `playWhenReady` does: the remote that sees its
/// session at once can pause it before it plays.
#[test]
fn a_remote_pause_while_the_track_opens_opens_it_paused() {
    let mut w = Music::new();
    w.m.play(vec![track(1)], 0, false);
    w.settle();
    assert_eq!(w.m.session_id(), Some("m1"));
    assert!(w.m.now_playing().is_some_and(|n| n.loading && !n.playing), "the track had already opened");
    let progress = |w: &Music| w.calls().into_iter().filter(|c| c.starts_with("progress")).collect::<Vec<_>>();
    // Play finds it playing, and says so again.
    w.m.server_event(&command("m1", Control::Play, None, None));
    w.settle();
    assert_eq!(progress(&w), ["progress playing 0"]);
    // Pause says so at once, and it opens paused.
    w.m.server_event(&command("m1", Control::Pause, None, None));
    w.settle();
    assert_eq!(progress(&w)[1..], ["progress paused 0"]);
    w.advance(SIM_LOAD_MS + 2_000);
    let now = w.m.now_playing().unwrap();
    assert_eq!((now.playing, now.loading), (false, false), "it played, or still says it loads");
    assert!(w.m.position_ms() < 1_000);
    // Play then plays it.
    w.m.server_event(&command("m1", Control::Play, None, None));
    w.settle();
    assert!(w.m.now_playing().unwrap().playing);
    assert_eq!(progress(&w).last().map(String::as_str), Some("progress playing 0"));
}

#[test]
fn music_obeys_nothing_once_it_gave_way_or_was_stopped() {
    let mut w = Music::new();
    w.m.play(vec![track(1), track(2)], 0, false);
    w.settle();
    w.advance(SIM_LOAD_MS + 4_000);
    w.m.yield_to_video();
    w.settle();
    assert_eq!(w.m.session_id(), None);
    let before = w.calls().len();
    w.m.server_event(&command("m1", Control::Play, None, None));
    w.m.server_event(&command("m1", Control::Next, None, None));
    w.settle();
    assert_eq!(w.calls().len(), before);
    assert_eq!(w.m.now_playing().map(|n| (n.index, n.playing)), Some((0, false)));
}
