//! packages/tv/test/live/{session,pages}.test.ts, ported: a live channel on
//! the TV's player (on `SimEngine`), taken over by another viewer, a
//! category's channels by pages, and the guide of the rows in view.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use pley_base::Waker;
use pley_core::api::LiveBrowse;
use pley_core::http::{Method, Streamed};
use pley_core::media::MediaKind;
use pley_core::model::*;
use pley_core::task::Tasks;
use pley_core::{ApiError, Cancel, CapsReport};
use pley_player::Reporter;
use pley_player::env::PlayerEnv;
use pley_player::live::{
    Category, ChannelPages, GuideStore, LivePhase, LiveSession, TroubleKind, holders, plays_as_sent,
};
use pley_player::server::{PlayerServer, SubtitleAnswer};
use pley_player::sim::{SIM_LOAD_MS, SimClock, SimEngine, SimHandle};

fn channel(id: i64) -> LiveChannel {
    LiveChannel {
        id,
        source_id: 1,
        name: format!("Ch {id}"),
        number: Some((100 + id).to_string()),
        ..LiveChannel::default()
    }
}

fn refused(status: u16, message: &str, body: Option<serde_json::Value>) -> ApiError {
    ApiError::Status { status, code: "conflict".into(), message: message.into(), body }
}

type Watch = Box<dyn Fn(i64, usize) -> Result<LiveWatching, ApiError> + Send + Sync>;

/// The server: each watch answered by `answer` (a session and a keyed address unless it says otherwise).
struct LiveFake {
    asked: Mutex<Vec<String>>,
    /// Why the server says a session ended, by session.
    ended: Mutex<HashMap<String, String>>,
    watches: Mutex<usize>,
    answer: Mutex<Option<Watch>>,
    /// Watches wait for the test to answer.
    hold: Mutex<Option<Option<LiveWatching>>>,
    held: Condvar,
    total: i64,
    /// What the plan of every session says.
    plan: Mutex<Option<LivePlan>>,
    /// Each stream.ts read: `stream {channel} {client}`.
    streams: Mutex<Vec<String>>,
    /// What lets each stream.ts read go.
    stream_cancels: Mutex<Vec<Cancel>>,
    /// The stream.ts reads still open as each playlist was asked for.
    open_at_playlist: Mutex<Vec<usize>>,
    /// Plans asked for, and whether they wait for the test.
    plans: Mutex<usize>,
    plans_held: AtomicBool,
    /// Sessions whose keyed playlist answers `410`, as a session ended on the server.
    gone: Mutex<Vec<String>>,
}

/// What the pretend server's stream.ts sends.
const STREAM_BYTES: &[u8] = b"\x47 MPEG-TS as the provider sends it";

impl LiveFake {
    fn new() -> Arc<LiveFake> {
        LiveFake::with_total(0)
    }

    fn with_total(total: i64) -> Arc<LiveFake> {
        Arc::new(LiveFake {
            asked: Mutex::default(),
            ended: Mutex::default(),
            watches: Mutex::new(0),
            answer: Mutex::new(None),
            hold: Mutex::new(None),
            held: Condvar::new(),
            total,
            plan: Mutex::new(None),
            streams: Mutex::default(),
            stream_cancels: Mutex::default(),
            open_at_playlist: Mutex::default(),
            plans: Mutex::new(0),
            plans_held: AtomicBool::new(false),
            gone: Mutex::default(),
        })
    }

    fn set_plan(&self, plan: LivePlan) {
        *self.plan.lock().unwrap() = Some(plan);
    }

    fn streams(&self) -> Vec<String> {
        self.streams.lock().unwrap().clone()
    }

    fn answer(&self, f: impl Fn(i64, usize) -> Result<LiveWatching, ApiError> + Send + Sync + 'static) {
        *self.answer.lock().unwrap() = Some(Box::new(f));
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    fn watches(&self) -> Vec<String> {
        self.asked().into_iter().filter(|a| a.starts_with("watch")).collect()
    }

    fn end(&self, session: &str, why: &str) {
        self.ended.lock().unwrap().insert(session.into(), why.into());
    }

    fn hold_watches(&self) {
        *self.hold.lock().unwrap() = Some(None);
    }

    fn answer_held(&self, w: LiveWatching) {
        *self.hold.lock().unwrap() = Some(Some(w));
        self.held.notify_all();
    }

    fn log(&self, s: String) {
        self.asked.lock().unwrap().push(s);
    }
}

fn not_here<T>() -> Result<T, ApiError> {
    Err(refused(404, "no", None))
}

impl PlayerServer for LiveFake {
    fn server(&self) -> &str {
        "http://tv.test"
    }
    fn url(&self, path: &str) -> String {
        format!("http://tv.test{path}")
    }
    fn item(&self, _: i64) -> Result<ItemDetail, ApiError> {
        not_here()
    }
    fn next(&self, _: i64) -> Result<Option<ItemSummary>, ApiError> {
        Ok(None)
    }
    fn playlist(&self, _: i64) -> Result<PlaylistDetail, ApiError> {
        not_here()
    }
    fn collection(&self, _: i64) -> Result<CollectionDetail, ApiError> {
        not_here()
    }
    fn play(&self, _: &PlaybackRequest) -> Result<PlaybackInfo, ApiError> {
        not_here()
    }
    fn progress(&self, _: i64, _: &ProgressReport) -> Result<(), ApiError> {
        Ok(())
    }
    fn stop_session(&self, _: &str) -> Result<(), ApiError> {
        Ok(())
    }
    fn session_status(&self, _: &str, _: Duration) -> Result<SessionStatus, ApiError> {
        not_here()
    }
    fn set_bitrate(&self, _: &str, _: u32) -> Result<BitrateAnswer, ApiError> {
        not_here()
    }
    fn set_streams(&self, _: i64, _: &StreamChoice) -> Result<(), ApiError> {
        Ok(())
    }
    fn reachable(&self, _: Duration) -> Result<ServerInfo, ApiError> {
        Ok(ServerInfo::default())
    }
    fn media(&self, method: Method, url: &str, _: Duration) -> Result<String, ApiError> {
        let m = if method == Method::Head { "HEAD" } else { "GET" };
        self.log(format!("{m} {}", url.replace("http://tv.test", "")));
        let open = self.stream_cancels.lock().unwrap().iter().filter(|c| !c.is_cancelled()).count();
        self.open_at_playlist.lock().unwrap().push(open);
        let session = url.split("pley_mk_").nth(1).and_then(|r| r.split('/').next()).map(|n| format!("s{n}"));
        if session.is_some_and(|s| self.gone.lock().unwrap().contains(&s)) {
            // A keyed address only says the session has gone, not why.
            return Err(ApiError::SessionStopped { message: Some("this channel isn't being watched any more".into()) });
        }
        Ok("#EXTM3U".into())
    }
    fn network(&self, _: &str, _: Duration) -> Result<NetworkAnswer, ApiError> {
        not_here()
    }
    fn tell_network(&self, _: &str, _: &NetworkReport, _: Duration) -> Result<NetworkAnswer, ApiError> {
        not_here()
    }
    fn speed_test(&self, _: u64) -> Result<SpeedSample, ApiError> {
        not_here()
    }
    fn trickplay(&self, _: i64) -> Result<TrickplayInfo, ApiError> {
        not_here()
    }
    fn subtitles(&self, _: &str, _: &str, _: Duration) -> Result<SubtitleAnswer, ApiError> {
        not_here()
    }
    fn search_subtitles(&self, _: i64, _: Option<i64>, _: &[String]) -> Result<Vec<SubtitleResult>, ApiError> {
        not_here()
    }
    fn add_subtitle(&self, _: i64, _: &AddSubtitle) -> Result<SubtitleTrack, ApiError> {
        not_here()
    }
    fn watch_channel(&self, id: i64, req: &WatchRequest) -> Result<LiveWatching, ApiError> {
        let taking = req.take_over.map(|t| format!(" taking {t}")).unwrap_or_default();
        self.log(format!("watch {id} {}{taking}", req.client_id.as_deref().unwrap_or("")));
        let n = {
            let mut w = self.watches.lock().unwrap();
            *w += 1;
            *w
        };
        let mut hold = self.hold.lock().unwrap();
        while let Some(waiting) = hold.as_mut() {
            if let Some(w) = waiting.take() {
                *hold = None;
                return Ok(w);
            }
            hold = self.held.wait(hold).unwrap();
        }
        drop(hold);
        match &*self.answer.lock().unwrap() {
            Some(f) => f(id, n),
            None => Ok(LiveWatching {
                session_id: format!("s{n}"),
                url: format!("/api/live/media/pley_mk_{n}/index.m3u8"),
                kind: "hls".into(),
                channel: Some(channel(id)),
            }),
        }
    }
    fn live_stream(&self, channel: i64, client_id: &str, cancel: Cancel) -> Result<Streamed, ApiError> {
        self.stream_cancels.lock().unwrap().push(cancel);
        self.streams.lock().unwrap().push(format!("stream {channel} {client_id}"));
        Ok(Streamed { status: 200, headers: Vec::new(), body: Box::new(Cursor::new(STREAM_BYTES.to_vec())) })
    }
    fn live_plan(&self, session: &str) -> Result<Option<LivePlan>, ApiError> {
        *self.plans.lock().unwrap() += 1;
        // The plan comes once the stream.ts read beside it was asked for (it is, at once).
        let until = Instant::now() + Duration::from_secs(2);
        while self.streams.lock().unwrap().is_empty() && Instant::now() < until {
            thread::sleep(Duration::from_millis(1));
        }
        while self.plans_held.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(1));
        }
        match self.ended.lock().unwrap().get(session) {
            Some(why) => Err(ApiError::SessionStopped { message: Some(why.clone()) }),
            None => Ok(self.plan.lock().unwrap().clone()),
        }
    }
    fn leave_channel(&self, session: &str) -> Result<(), ApiError> {
        self.log(format!("leave {session}"));
        Ok(())
    }
    fn live_channel(&self, _: i64) -> Result<LiveChannel, ApiError> {
        not_here()
    }
    fn live_groups(&self) -> Result<LiveGroups, ApiError> {
        Ok(LiveGroups::default())
    }
    fn live_browse(&self, b: &LiveBrowse) -> Result<LiveChannelPage, ApiError> {
        let group = b.category.group().map(|g| format!(" {g}")).unwrap_or_default();
        match b.around {
            Some(a) => self.log(format!("{} around {a}", b.category.wire())),
            None => self.log(format!("{}{group} @{}", b.category.wire(), b.offset)),
        }
        let limit = if b.limit > 0 { b.limit } else { 100 };
        let offset = b.around.map_or(b.offset, |a| (a - 1) / limit * limit);
        let channels = (offset..self.total.min(offset + limit))
            .map(|i| LiveChannel {
                id: i + 1,
                name: format!("Ch {}", i + 1),
                number: Some((i + 1).to_string()),
                ..LiveChannel::default()
            })
            .collect();
        Ok(LiveChannelPage { total: self.total, offset, channels })
    }
    fn live_guide(&self, from: i64, to: i64, ids: &[i64]) -> Result<LiveGuide, ApiError> {
        self.log(format!("{from}: {}", ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")));
        Ok(LiveGuide { from, to, ..LiveGuide::default() })
    }
    fn set_live_favorite(&self, _: i64, _: bool) -> Result<(), ApiError> {
        Ok(())
    }
}

/// A live player on `SimEngine` and the pretend server, on a clock the test moves.
struct Live {
    fake: Arc<LiveFake>,
    clock: SimClock,
    sim: SimHandle,
    c: LiveSession,
    now: i64,
}

impl Live {
    fn new(fake: Arc<LiveFake>) -> Live {
        let clock = SimClock::manual();
        let (engine, sim) = SimEngine::new(clock.clone());
        let server: Arc<dyn PlayerServer> = fake.clone();
        let caps = Arc::new(|| None::<CapsReport>);
        let env = PlayerEnv::new(server, Tasks::with_threads(2, Waker::noop()), Reporter::new(), caps, "webos-tv");
        let c = LiveSession::new(channel(1), &env, Box::new(engine), 0);
        Live { fake, clock, sim, c, now: 0 }
    }

    fn settle(&mut self) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            self.c.tick(self.now);
            if !self.c.busy() {
                break;
            }
            assert!(Instant::now() < until, "a server call never came back");
            thread::sleep(Duration::from_millis(1));
        }
        self.c.tick(self.now);
    }

    fn advance(&mut self, ms: i64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 50).min(end);
            self.clock.set(self.now);
            self.settle();
        }
    }

    /// The first picture shows.
    fn first_frame(&mut self) {
        self.advance(SIM_LOAD_MS);
    }
}

#[test]
fn watches_with_this_tvs_id_waits_for_the_playlist_then_plays_its_keyed_address_from_the_live_edge() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    assert_eq!(w.fake.asked(), ["watch 1 webos-tv", "GET /api/live/media/pley_mk_1/index.m3u8"]);
    let load = &w.sim.loads()[0];
    assert_eq!(
        (load.url.as_str(), load.kind, load.start_ms),
        ("http://tv.test/api/live/media/pley_mk_1/index.m3u8", pley_core::media::MediaKind::LiveHls, 0)
    );
    assert_eq!(w.c.state().phase, LivePhase::Tuning);
    w.first_frame();
    let s = w.c.state();
    assert_eq!((s.phase, s.shown, s.audio_only), (LivePhase::Playing, true, false));
}

#[test]
fn changing_channels_lets_the_last_one_go_first_and_holds_one_session() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.c.tune(channel(2));
    w.settle();
    assert_eq!(w.fake.asked()[2..], ["leave s1", "watch 2 webos-tv", "GET /api/live/media/pley_mk_2/index.m3u8"]);
    assert!(w.sim.commands().contains(&"unload".to_string()));
    let s = w.c.state();
    assert_eq!((s.phase, s.shown, s.tunes, s.channel.id), (LivePhase::Tuning, false, 2, 2));
}

#[test]
fn says_the_servers_own_words_when_every_connection_is_in_use_and_tries_again() {
    let fake = LiveFake::new();
    let busy = Arc::new(Mutex::new(true));
    let still = busy.clone();
    fake.answer(move |id, n| {
        if *still.lock().unwrap() {
            Err(refused(409, "The one connection of Provider is in use, watching News.", None))
        } else {
            Ok(LiveWatching {
                session_id: format!("s{n}"),
                url: "/api/live/media/pley_mk_x/index.m3u8".into(),
                kind: "hls".into(),
                channel: Some(channel(id)),
            })
        }
    });
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    assert_eq!(w.c.state().phase, LivePhase::Error);
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!(
        (t.kind, t.title.as_str(), t.message.as_str(), t.can_retry),
        (
            TroubleKind::Busy,
            "All connections are in use",
            "The one connection of Provider is in use, watching News.",
            true
        )
    );
    *busy.lock().unwrap() = false;
    w.c.retry();
    w.settle();
    assert_eq!(w.c.state().phase, LivePhase::Tuning);
    assert_eq!(w.fake.watches().len(), 2);
}

#[test]
fn lists_what_holds_each_connection_and_takes_one_over_where_it_may() {
    let connections = serde_json::json!([
        { "channel_id": 7, "channel_name": "Sport", "watchers": [], "people": 1, "yours": true, "can_take_over": true },
        { "channel_id": 8, "channel_name": "Films", "watchers": ["ana", "bo"], "people": 2, "yours": false, "can_take_over": false }
    ]);
    let fake = LiveFake::new();
    let body = serde_json::json!({ "code": "conflict", "message": "", "connections": connections });
    fake.answer(move |id, n| {
        if n == 1 {
            Err(refused(409, "Provider allows 2 connections at once.", Some(body.clone())))
        } else {
            Ok(LiveWatching {
                session_id: format!("s{n}"),
                url: "/api/live/media/pley_mk_x/index.m3u8".into(),
                kind: "hls".into(),
                channel: Some(channel(id)),
            })
        }
    });
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!(t.connections.iter().map(|c| c.channel_id).collect::<Vec<_>>(), [7, 8]);
    assert_eq!(t.connections.iter().map(holders).collect::<Vec<_>>(), ["You, on another device", "ana, bo"]);
    w.c.take_over(7);
    w.settle();
    assert_eq!(w.fake.watches(), ["watch 1 webos-tv", "watch 1 webos-tv taking 7"]);
    assert_eq!(w.c.state().phase, LivePhase::Tuning);
}

#[test]
fn asks_a_server_without_keyed_addresses_to_be_updated() {
    let fake = LiveFake::new();
    fake.answer(|id, _| {
        Ok(LiveWatching {
            session_id: "old".into(),
            url: "/api/live/sessions/old/index.m3u8".into(),
            kind: "hls".into(),
            channel: Some(channel(id)),
        })
    });
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    assert_eq!(w.c.state().trouble.as_ref().map(|t| t.kind), Some(TroubleKind::Older));
    assert!(w.sim.loads().is_empty());
}

#[test]
fn watches_anew_when_the_player_fails_twice_then_says_so() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.sim.fail(2, "gone");
    w.settle();
    w.advance(1_000);
    w.first_frame();
    w.sim.fail(2, "gone");
    w.settle();
    w.advance(3_000);
    assert_eq!(w.fake.watches().len(), 3);
    w.first_frame();
    w.sim.fail(2, "gone");
    w.settle();
    assert_eq!(w.c.state().phase, LivePhase::Error);
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.title.as_str(), t.can_retry), ("Ch 1 stopped", true));
    // A session that ends with an error is let go.
    assert_eq!(w.fake.asked().last().map(String::as_str), Some("leave s3"));
}

#[test]
fn says_when_the_server_stopped_it() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.fake.end("s1", "Stopped by the administrator.");
    w.sim.fail(410, "gone");
    w.settle();
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.message.as_str()), (TroubleKind::Stopped, "Stopped by the administrator."));
}

#[test]
fn says_who_took_the_connection_over_rather_than_watching_again() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.fake.end("s1", "Sam is watching Films now, on the connection this channel used.");
    w.sim.fail(2, "the playlist went");
    w.settle();
    w.advance(5_000);
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!(
        (t.kind, t.title.as_str(), t.message.as_str(), t.can_retry),
        (
            TroubleKind::Stopped,
            "Live TV stopped",
            "Sam is watching Films now, on the connection this channel used.",
            true
        )
    );
    assert_eq!(w.fake.watches().len(), 1);
    // Its session is gone already: nothing to leave.
    assert!(!w.fake.asked().contains(&"leave s1".to_string()));
}

#[test]
fn says_the_channel_isnt_being_watched_any_more_when_the_server_ended_it_without_words() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.fake.end("s1", "");
    w.sim.fail(2, "the playlist went");
    w.settle();
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.message.as_str()), (TroubleKind::Stopped, "This channel isn't being watched any more."));
    // A `410` with no words says the same, not a video's "stopped by the administrator".
    let bare = pley_player::live::trouble_of(&ApiError::SessionStopped { message: None }, "Ch 1");
    assert_eq!(
        (bare.title.as_str(), bare.message.as_str()),
        ("Live TV stopped", "This channel isn't being watched any more.")
    );
}

#[test]
fn asks_why_when_the_playlist_has_gone_before_it_plays() {
    let fake = LiveFake::new();
    fake.gone.lock().unwrap().push("s1".into());
    fake.end("s1", "Someone else needed this channel's connection to watch another channel.");
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!(
        (t.kind, t.message.as_str()),
        (TroubleKind::Stopped, "Someone else needed this channel's connection to watch another channel.")
    );
    assert!(w.sim.loads().is_empty());
    assert!(!w.fake.asked().contains(&"leave s1".to_string()));
}

// ---- taken over by another viewer

#[test]
fn a_take_over_stops_the_channel_at_once_with_the_servers_words_and_it_isnt_watched_again() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.c.taken_over(1, Some("Sam is watching Two HD now, on the connection this channel used."));
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!(
        (w.c.state().phase, t.kind, t.title.as_str(), t.message.as_str(), t.can_retry),
        (
            LivePhase::Error,
            TroubleKind::Stopped,
            "Live TV stopped",
            "Sam is watching Two HD now, on the connection this channel used.",
            true
        )
    );
    assert_eq!(w.sim.commands().last().map(String::as_str), Some("unload"));
    // The stream breaking off after changes nothing: no retry, no other words.
    w.sim.fail(2, "the stream ended");
    w.settle();
    w.advance(60_000);
    assert_eq!(w.fake.watches().len(), 1);
    assert_eq!(w.c.state().trouble, Some(t));
    // Its session is gone on the server: nothing to leave.
    assert!(!w.fake.asked().contains(&"leave s1".to_string()));
}

#[test]
fn a_take_over_matches_the_channel_and_leaves_another_alone() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.c.taken_over(2, Some("Watching moved to Two HD on another device."));
    assert_eq!((w.c.state().phase, w.c.state().trouble.is_none()), (LivePhase::Playing, true));
    w.c.taken_over(1, Some("Watching moved to Two HD on another device."));
    assert_eq!(w.c.state().trouble.as_ref().map(|t| t.kind), Some(TroubleKind::Stopped));
}

#[test]
fn a_take_over_says_so_once_however_many_of_the_channels_viewers_the_server_ended() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.c.taken_over(1, Some("Sam is watching Two HD now, on the connection this channel used."));
    let version = w.c.version();
    let unloads = w.sim.commands().iter().filter(|c| *c == "unload").count();
    // Its HLS session and its stream's reader each have an event.
    w.c.taken_over(1, Some("Sam is watching Two HD now, on the connection this channel used."));
    assert_eq!(w.c.version(), version);
    assert_eq!(w.sim.commands().iter().filter(|c| *c == "unload").count(), unloads);
}

#[test]
fn a_take_overs_words_replace_a_410_without_any_that_came_first() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.fake.end("s1", "");
    w.sim.fail(2, "the playlist went");
    w.settle();
    assert_eq!(
        w.c.state().trouble.as_ref().map(|t| t.message.as_str()),
        Some("This channel isn't being watched any more.")
    );
    w.c.taken_over(1, Some("Watching moved to Two HD on another device."));
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.message.as_str()), (TroubleKind::Stopped, "Watching moved to Two HD on another device."));
}

#[test]
fn a_take_over_leaves_the_words_of_a_stop_that_came_first() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.fake.end("s1", "The channel was stopped.");
    w.sim.fail(2, "the playlist went");
    w.settle();
    let shown = w.c.state().trouble.clone().unwrap();
    assert_eq!((shown.kind, shown.message.as_str()), (TroubleKind::Stopped, "The channel was stopped."));
    let version = w.c.version();
    // Later, the same account's phone watches the channel and is taken over.
    w.c.taken_over(1, Some("Watching moved to Two HD on another device."));
    assert_eq!(w.c.version(), version);
    assert_eq!(w.c.state().trouble.as_ref(), Some(&shown));
}

#[test]
fn a_take_overs_card_stays_through_the_background_and_nothing_is_watched_until_watch_again() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.c.taken_over(1, Some("Watching moved to Two HD on another device."));
    let shown = w.c.state().clone();
    w.c.suspend();
    w.settle();
    w.c.resume();
    w.settle();
    w.advance(10_000);
    assert_eq!(w.c.state(), &shown);
    assert_eq!(w.fake.watches().len(), 1);
    assert_eq!(w.sim.loads().len(), 1);
    w.c.retry();
    w.settle();
    assert_eq!(w.fake.watches().len(), 2);
}

#[test]
fn a_take_over_in_the_second_before_a_failures_retry_stops_it_there() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    // The stream broke off first; the server didn't say yet that watching ended.
    w.sim.fail(2, "the stream ended");
    w.settle();
    w.advance(500);
    assert_eq!(w.fake.watches().len(), 1);
    w.c.taken_over(1, Some("Someone else needed this channel's connection to watch another channel."));
    w.advance(10_000);
    assert_eq!(w.fake.watches().len(), 1);
    assert_eq!(w.c.state().trouble.as_ref().map(|t| t.kind), Some(TroubleKind::Stopped));
}

#[test]
fn a_take_over_of_a_stream_ts_stops_it_before_it_would_be_watched_as_hls() {
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "copy", "aac"));
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    assert!(w.c.direct());
    // The server tells first, then ends the stream.
    w.c.taken_over(1, Some("Watching moved to Two HD on another device."));
    assert!(!w.c.direct());
    w.sim.fail(2, "the stream broke off");
    w.settle();
    w.advance(5_000);
    assert_eq!(w.fake.watches().len(), 1);
    assert_eq!(w.sim.loads().len(), 1);
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.message.as_str()), (TroubleKind::Stopped, "Watching moved to Two HD on another device."));
}

#[test]
fn a_take_over_stops_a_channel_still_tuning_and_lets_its_late_watch_go() {
    let fake = LiveFake::new();
    fake.hold_watches();
    let mut w = Live::new(fake.clone());
    w.c.tune(channel(1));
    let until = Instant::now() + Duration::from_secs(5);
    while fake.watches().is_empty() {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    w.c.taken_over(1, Some("Watching moved to Two HD on another device."));
    assert_eq!(w.c.state().trouble.as_ref().map(|t| t.kind), Some(TroubleKind::Stopped));
    fake.answer_held(LiveWatching {
        session_id: "late".into(),
        url: "/api/live/media/pley_mk_late/index.m3u8".into(),
        kind: "hls".into(),
        channel: Some(channel(1)),
    });
    w.settle();
    while !fake.asked().contains(&"leave late".to_string()) {
        assert!(Instant::now() < until, "the late watch was never let go");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(w.sim.loads().is_empty());
}

#[test]
fn watch_again_after_a_take_over_watches_as_any_watch_never_taking_a_connection_by_itself() {
    let fake = LiveFake::new();
    let body = serde_json::json!({
        "code": "conflict", "message": "",
        "connections": [{ "channel_id": 2, "channel_name": "Two HD", "watchers": ["sam"], "people": 1, "yours": false, "can_take_over": false }]
    });
    fake.answer(move |id, n| {
        if n == 1 {
            Ok(LiveWatching {
                session_id: "s1".into(),
                url: "/api/live/media/pley_mk_1/index.m3u8".into(),
                kind: "hls".into(),
                channel: Some(channel(id)),
            })
        } else {
            Err(refused(
                409,
                "Provider allows one connection at once, and it is in use: watching Two HD.",
                Some(body.clone()),
            ))
        }
    });
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.c.taken_over(1, Some("Sam is watching Two HD now, on the connection this channel used."));
    w.c.retry();
    w.settle();
    assert_eq!(w.fake.watches(), ["watch 1 webos-tv", "watch 1 webos-tv"]);
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.connections.len()), (TroubleKind::Busy, 1));
}

#[test]
fn asks_the_server_why_a_stall_lasts() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.sim.wait(true);
    w.settle();
    w.advance(5_000);
    // Still watching on the server: it waits on.
    assert_eq!(w.c.state().phase, LivePhase::Buffering);
    w.sim.wait(false);
    w.settle();
    w.sim.wait(true);
    w.settle();
    w.fake.end("s1", "Watching moved to Films on another device.");
    w.advance(5_000);
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.message.as_str()), (TroubleKind::Stopped, "Watching moved to Films on another device."));
    assert_eq!(w.fake.watches().len(), 1);
}

#[test]
fn watches_anew_after_a_stall_of_20_s() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.sim.wait(true);
    w.settle();
    w.advance(21_000);
    assert_eq!(w.fake.watches().len(), 2);
}

#[test]
fn gives_up_on_a_channel_whose_picture_never_comes() {
    let mut w = Live::new(LiveFake::new());
    // The pipeline opens nothing in time.
    w.sim.set_load_ms(60_000);
    w.c.tune(channel(1));
    w.settle();
    w.advance(30_000);
    // The first try failing is tried again, as any failure.
    assert_eq!(w.c.state().phase, LivePhase::Tuning);
    w.advance(1_000);
    assert_eq!(w.fake.watches().len(), 2);
}

#[test]
fn knows_a_channel_with_sound_and_no_picture() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    // The live playlist carries no video facts: the sim's stats have no width.
    w.advance(1_500);
    assert!(w.c.state().audio_only);
}

#[test]
fn plays_from_the_live_edge_and_nothing_else() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    assert_eq!(w.c.state().phase, LivePhase::Playing);
    assert_eq!(w.sim.loads()[0].start_ms, 0);
    let before = w.fake.asked().len();
    w.advance(120_000);
    assert_eq!(w.fake.asked().len(), before);
    assert!(w.sim.commands().iter().all(|c| c != "pause" && !c.starts_with("seek")));
}

#[test]
fn lets_go_of_the_session_when_the_tv_hides_the_app_and_watches_again_when_it_shows() {
    let mut w = Live::new(LiveFake::new());
    w.c.tune(channel(1));
    w.settle();
    w.c.suspend();
    w.settle();
    assert_eq!(w.fake.asked().last().map(String::as_str), Some("leave s1"));
    w.c.resume();
    w.settle();
    let asked = w.fake.asked();
    assert_eq!(asked[asked.len() - 2..], ["watch 1 webos-tv", "GET /api/live/media/pley_mk_2/index.m3u8"]);
}

#[test]
fn tells_the_server_at_once_when_the_player_closes_and_drops_a_watch_that_answers_after() {
    let fake = LiveFake::new();
    fake.hold_watches();
    let mut w = Live::new(fake.clone());
    w.c.tune(channel(1));
    let until = Instant::now() + Duration::from_secs(5);
    while fake.watches().is_empty() {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    w.c.close();
    fake.answer_held(LiveWatching {
        session_id: "late".into(),
        url: "/api/live/media/pley_mk_late/index.m3u8".into(),
        kind: "hls".into(),
        channel: Some(channel(1)),
    });
    while !fake.asked().contains(&"leave late".to_string()) {
        assert!(Instant::now() < until, "the late watch was never let go");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(fake.asked(), ["watch 1 webos-tv", "leave late"]);
    assert!(w.sim.loads().is_empty());
}

#[test]
fn a_session_the_watch_made_is_let_go_when_the_player_closes_before_it_plays() {
    let fake = LiveFake::new();
    fake.plans_held.store(true, Ordering::SeqCst);
    let mut w = Live::new(fake.clone());
    w.c.tune(channel(1));
    // The watch answered, and the tune waits on the session's plan: the player closes.
    let until = Instant::now() + Duration::from_secs(5);
    while *fake.plans.lock().unwrap() == 0 {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    w.c.close();
    fake.plans_held.store(false, Ordering::SeqCst);
    while !fake.asked().contains(&"leave s1".to_string()) {
        assert!(Instant::now() < until, "the session was never let go: {:?}", fake.asked());
        thread::sleep(Duration::from_millis(1));
    }
    // Nor is its playlist asked for.
    assert_eq!(fake.asked(), ["watch 1 webos-tv", "leave s1"]);
    assert!(w.sim.loads().is_empty());
}

// ---- the channel's stream.ts

fn plan(video: &str, audio: &str, audio_codec: &str) -> LivePlan {
    LivePlan {
        video_codec: Some("h264".into()),
        audio_codec: Some(audio_codec.into()),
        video: video.into(),
        audio: audio.into(),
        ..LivePlan::default()
    }
}

/// What the TV's player gets at `url`: the status line and the body; nothing where nothing listens.
fn fetch(url: &str) -> (String, Vec<u8>) {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_at(rest.find('/').unwrap());
    let Ok(mut conn) = TcpStream::connect(host) else { return (String::new(), Vec::new()) };
    conn.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = conn.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes());
    let mut all = Vec::new();
    let _ = conn.read_to_end(&mut all);
    let end = all.windows(4).position(|w| w == b"\r\n\r\n").map_or(all.len(), |p| p + 4);
    let status = String::from_utf8_lossy(&all[..end]).lines().next().unwrap_or("").to_string();
    (status, all[end..].to_vec())
}

#[test]
fn a_plan_sent_as_it_is_or_with_sound_the_tv_decodes_plays_from_stream_ts() {
    for (video, audio, codec, direct) in [
        ("copy", "copy", "aac", true),
        ("copy", "none", "", true),
        ("none", "copy", "aac", true),
        ("copy", "aac", "ac3", true),
        ("copy", "aac", "eac3", true),
        ("copy", "aac", "mp2", true),
        ("copy", "aac", "dts", false),
        ("copy", "aac", "truehd", false),
        ("h264_vaapi", "copy", "aac", false),
    ] {
        assert_eq!(plays_as_sent(&plan(video, audio, codec)), direct, "{video} {audio} {codec}");
    }
}

#[test]
fn plays_the_channel_s_stream_ts_from_the_tv_s_loopback_address_when_the_plan_sends_it_as_it_is() {
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "copy", "aac"));
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    // No playlist waited for: the stream.ts plays from its first keyframe.
    assert_eq!(w.fake.asked(), ["watch 1 webos-tv"]);
    let load = w.sim.loads()[0].clone();
    assert_eq!(load.kind, MediaKind::LiveTs);
    assert!(load.url.starts_with("http://127.0.0.1:") && load.url.ends_with("/stream.ts"), "{}", load.url);
    assert!(w.c.direct());
    // The TV's player reads there what the app reads with the token, under this TV's stream id.
    assert_eq!(fetch(&load.url), ("HTTP/1.1 200 OK".to_string(), STREAM_BYTES.to_vec()));
    assert!(w.fake.streams().iter().all(|s| s == "stream 1 webos-tv-stream"));
    w.first_frame();
    assert_eq!(w.c.state().phase, LivePhase::Playing);
    // Leaving closes its address.
    w.c.close();
    assert_ne!(fetch(&load.url).0, "HTTP/1.1 200 OK");
}

#[test]
fn plays_the_hls_when_the_server_converts_the_channel_and_lets_its_stream_ts_go() {
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "aac", "dts"));
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    assert_eq!(w.fake.asked(), ["watch 1 webos-tv", "GET /api/live/media/pley_mk_1/index.m3u8"]);
    assert_eq!(w.sim.loads()[0].kind, MediaKind::LiveHls);
    assert!(!w.c.direct());
    // As the Samsung app's: let go as soon as the plan says, not once the playlist has come.
    assert_eq!(w.fake.streams(), ["stream 1 webos-tv-stream"]);
    assert_eq!(*w.fake.open_at_playlist.lock().unwrap(), [0]);
}

#[test]
fn a_stream_ts_that_fails_before_it_plays_is_watched_as_hls_at_once() {
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "copy", "aac"));
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    w.sim.fail(2, "no such format");
    w.settle();
    let loads = w.sim.loads();
    assert_eq!(loads.iter().map(|l| l.kind).collect::<Vec<_>>(), [MediaKind::LiveTs, MediaKind::LiveHls]);
    assert_eq!(w.fake.watches().len(), 2);
    assert!(!w.c.direct());
    assert_eq!(w.c.state().phase, LivePhase::Tuning);
}

#[test]
fn a_stream_ts_that_doesn_t_start_in_10_s_is_watched_as_hls() {
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "copy", "aac"));
    let mut w = Live::new(fake);
    w.sim.set_load_ms(60_000);
    w.c.tune(channel(1));
    w.settle();
    w.advance(9_900);
    assert_eq!(w.sim.loads().len(), 1);
    w.advance(200);
    assert_eq!(w.sim.loads().last().map(|l| l.kind), Some(MediaKind::LiveHls));
}

#[test]
fn a_stream_ts_that_breaks_off_asks_the_server_first_then_watches_the_hls() {
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "copy", "aac"));
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    // A stall of a stream.ts asks nothing: its HLS session, unread, would say watching ended.
    w.sim.wait(true);
    w.settle();
    w.fake.end("s1", "Watching moved to Films on another device.");
    w.advance(5_000);
    assert_eq!(w.c.state().phase, LivePhase::Buffering);
    w.sim.wait(false);
    w.settle();
    // Broken off: the server says watching ended there.
    w.sim.fail(2, "the stream broke off");
    w.settle();
    let t = w.c.state().trouble.clone().unwrap();
    assert_eq!((t.kind, t.message.as_str()), (TroubleKind::Stopped, "Watching moved to Films on another device."));
    assert_eq!(w.fake.watches().len(), 1);

    // It didn't end there: the HLS.
    let fake = LiveFake::new();
    fake.set_plan(plan("copy", "copy", "aac"));
    let mut w = Live::new(fake);
    w.c.tune(channel(1));
    w.settle();
    w.first_frame();
    w.sim.fail(2, "the stream broke off");
    w.settle();
    assert_eq!(w.sim.loads().last().map(|l| l.kind), Some(MediaKind::LiveHls));
    assert_eq!(w.fake.watches().len(), 2);
}

// ---- a category's channels

fn pages_of(total: i64, category: Category, frozen: bool) -> (Arc<LiveFake>, ChannelPages) {
    let fake = LiveFake::with_total(total);
    let server: Arc<dyn PlayerServer> = fake.clone();
    (fake, ChannelPages::new(server, Tasks::with_threads(1, Waker::noop()), category, frozen))
}

fn wait_pages(pages: &mut ChannelPages) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        pages.poll(0);
        if !pages.busy() {
            pages.poll(0);
            return;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn channels_come_a_page_at_a_time_as_they_show_the_first_saying_how_many() {
    let (fake, mut pages) = pages_of(57_176, Category::Group("UK| Sport".into()), false);
    pages.want(0, 12, 0);
    wait_pages(&mut pages);
    assert_eq!(fake.asked(), ["group UK| Sport @0"]);
    assert_eq!(pages.total(), Some(57_176));
    assert_eq!(pages.at(11).map(|c| c.id), Some(12));
    assert!(pages.at(150).is_none());
    pages.want(190, 210, 0);
    wait_pages(&mut pages);
    // The first page is fresh; the second and third come.
    let mut asked = fake.asked();
    asked[1..].sort();
    assert_eq!(asked, ["group UK| Sport @0", "group UK| Sport @100", "group UK| Sport @200"]);
    assert_eq!(pages.at(205).map(|c| c.id), Some(206));
}

#[test]
fn channels_know_where_a_channel_is_by_the_page_around_it() {
    let (fake, mut pages) = pages_of(57_176, Category::All, false);
    pages.neighbor(30_042, 0, 0);
    wait_pages(&mut pages);
    assert_eq!(pages.take_neighbor().flatten().map(|c| c.id), Some(30_042));
    assert_eq!(fake.asked(), ["all around 30042"]);
    // Known now: not asked again.
    assert_eq!(pages.find(30_050), Some(30_049));
    pages.neighbor(30_050, 0, 0);
    wait_pages(&mut pages);
    assert_eq!(pages.take_neighbor().flatten().map(|c| c.id), Some(30_050));
    assert_eq!(fake.asked().len(), 1);
}

#[test]
fn channels_go_round_their_ends_for_channel_up_and_down() {
    let (_, mut pages) = pages_of(250, Category::Favorites, true);
    let mut neighbor = |id, by| {
        pages.neighbor(id, by, 0);
        wait_pages(&mut pages);
        pages.take_neighbor().flatten().map(|c| c.id)
    };
    assert_eq!(neighbor(250, 1), Some(1));
    assert_eq!(neighbor(1, -1), Some(250));
    assert_eq!(neighbor(100, 1), Some(101));
    assert_eq!(neighbor(42, -1), Some(41));
}

#[test]
fn channels_show_a_favourite_marked_at_once() {
    let (_, mut pages) = pages_of(10, Category::All, false);
    pages.want(0, 10, 0);
    wait_pages(&mut pages);
    pages.patch(3, |c| c.favorite = true);
    assert_eq!(pages.at(2).map(|c| c.favorite), Some(true));
}

#[test]
fn the_guide_asks_only_for_the_channels_shown_and_only_once_a_chunk() {
    let fake = LiveFake::new();
    let server: Arc<dyn PlayerServer> = fake.clone();
    let mut store = GuideStore::new(server, Tasks::with_threads(1, Waker::noop()), 0);
    store.want(0, 3600, &[1, 2, 3], 0);
    store.want(0, 3600, &[2, 3, 4], 0);
    let until = Instant::now() + Duration::from_secs(5);
    while store.busy() {
        store.poll(0);
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(fake.asked(), ["0: 1,2,3", "0: 4"]);
    assert!(store.answered(1, 60));
    assert!(!store.answered(9, 60));
    store.want(0, 3600, &[], 0);
    assert_eq!(fake.asked().len(), 2);
}
