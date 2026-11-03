//! The player against pley-fixtures' pretend Pley server, through pley-core's
//! real `Api`: keyed addresses, reports and session ends, subtitles that come
//! whole on a second asking, Up Next, and Live TV's stream.ts, its HLS and
//! busy connections.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use pley_base::Waker;
use pley_core::device::lg_2022_caps;
use pley_core::media::MediaKind;
use pley_core::model::{ItemKind, ProgressReport};
use pley_core::task::Tasks;
use pley_core::{Api, ApiError, CapsOverrides, Http, TokenSource};
use pley_fixtures::{FixtureServer, ids};
use pley_player::controller::{Phase, PlaybackController};
use pley_player::env::{HeldQuality, PlayerEnv};
use pley_player::live::{LivePhase, LiveSession, TroubleKind};
use pley_player::server::PlayerServer;
use pley_player::sim::{SIM_LOAD_MS, SimClock, SimEngine, SimHandle};
use pley_player::{PlayRequest, Reporter};

struct Fixed(String);

impl TokenSource for Fixed {
    fn access_token(&self) -> Result<String, ApiError> {
        Ok(self.0.clone())
    }

    fn refresh(&self, _stale: &str) -> Result<String, ApiError> {
        Err(ApiError::SignedOut)
    }

    fn refused_after_refresh(&self, _sent: &str) -> bool {
        false
    }
}

fn env(server: &FixtureServer) -> PlayerEnv {
    let http = Http::network("pleyLG/0.0.0 (webOS 22; test)");
    let api: Arc<dyn PlayerServer> =
        Arc::new(Api::new(http, server.url(), Arc::new(Fixed(server.access_token(ids::ALEX)))));
    let caps = Arc::new(|| Some(lg_2022_caps(&CapsOverrides::default())));
    let mut env =
        PlayerEnv::new(api, Tasks::new(Waker::noop()), Reporter::new(), caps, "webos-0123456789abcdef0123456789abcdef");
    env.quality = Arc::new(HeldQuality::with("original"));
    env
}

/// A controller on `SimEngine` against the fixtures, on a clock the test moves.
struct Run {
    clock: SimClock,
    sim: SimHandle,
    c: PlaybackController,
    now: i64,
}

impl Run {
    fn new(server: &FixtureServer, req: PlayRequest) -> Run {
        let clock = SimClock::manual();
        let (engine, sim) = SimEngine::new(clock.clone());
        let c = PlaybackController::new(req, env(server), Box::new(engine), 0);
        Run { clock, sim, c, now: 0 }
    }

    fn settle(&mut self) {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            self.c.tick(self.now);
            if !self.c.busy() && !self.c.warming() {
                break;
            }
            assert!(Instant::now() < until, "the server never answered");
            thread::sleep(Duration::from_millis(2));
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
}

fn progress_states(server: &FixtureServer, item: i64) -> Vec<String> {
    server
        .writes("POST", &format!("/api/items/{item}/progress"))
        .iter()
        .filter_map(|e| serde_json::from_str::<ProgressReport>(&e.body).ok())
        .map(|r| r.state.as_str().to_string())
        .collect()
}

#[test]
fn a_movie_plays_from_its_keyed_address_reports_and_ends_its_session() {
    let server = FixtureServer::start();
    let mut run = Run::new(&server, PlayRequest::item(ids::SAMPLE_1080P));
    run.c.start();
    run.settle();
    let load = run.sim.loads().first().cloned().expect("the engine was given the session");
    assert!(load.url.starts_with(server.url()), "{}", load.url);
    assert!(load.url.contains("/api/media/pley_mk_"), "{}", load.url);
    let session = run.c.session().cloned().unwrap();
    run.advance(SIM_LOAD_MS);
    assert_eq!(run.c.state().phase, Phase::Playing);
    run.advance(10_000);
    assert_eq!(progress_states(&server, ids::SAMPLE_1080P), ["playing"]);
    run.c.close();
    run.settle();
    assert_eq!(progress_states(&server, ids::SAMPLE_1080P), ["playing", "stopped"]);
    assert_eq!(server.writes("DELETE", &format!("/api/playback/{}", session.session_id)).len(), 1);
}

#[test]
fn text_subtitles_that_come_in_part_are_asked_again_and_show_whole() {
    let server = FixtureServer::start();
    let req = PlayRequest {
        item_id: ids::SAMPLE_HDR10,
        start_ms: Some(12_000),
        subtitle_index: Some(ids::SAMPLE_HDR10_SUBTITLE),
        ..PlayRequest::default()
    };
    let mut run = Run::new(&server, req);
    run.c.start();
    run.settle();
    assert!(run.c.state().subtitle_sync);
    run.advance(SIM_LOAD_MS + 5_000);
    let shown: Vec<String> = run.c.subtitles().text.iter().map(|c| c.plain_text()).collect();
    assert!(shown.iter().any(|t| t.contains("The end credits.")), "{shown:?}");
    assert_eq!(run.c.state().notice, None);
}

#[test]
fn the_next_episode_plays_after_the_credits_countdown() {
    let server = FixtureServer::start();
    let req = PlayRequest {
        item_id: ids::SHOW_S1E1,
        start_ms: Some(ids::CREDITS_START_MS - 1_000),
        ..PlayRequest::default()
    };
    let mut run = Run::new(&server, req);
    run.c.start();
    run.settle();
    assert_eq!(run.c.state().kind, Some(ItemKind::Episode));
    run.advance(SIM_LOAD_MS + 2_000);
    let card = run.c.state().up_next.clone().expect("Up Next shows in the credits");
    assert_eq!(card.item_id, ids::SHOW_S1E2);
    run.advance(11_000);
    assert_eq!(run.c.state().item_id, ids::SHOW_S1E2);
    assert!(run.sim.loads().len() >= 2);
}

/// The first `n` bytes the TV's player reads at `url` (plain HTTP), past the answer's head.
fn read_from(url: &str, n: usize) -> (String, Vec<u8>) {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_at(rest.find('/').unwrap());
    let mut conn = TcpStream::connect(host).unwrap();
    conn.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    conn.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes()).unwrap();
    let mut all = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let end = all.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4);
        if end.is_some_and(|e| all.len() >= e + n) {
            break;
        }
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(k) => all.extend_from_slice(&buf[..k]),
        }
    }
    let end = all.windows(4).position(|w| w == b"\r\n\r\n").map_or(all.len(), |p| p + 4);
    let status = String::from_utf8_lossy(&all[..end]).lines().next().unwrap_or("").to_string();
    (status, all[end..].iter().copied().take(n).collect())
}

#[test]
fn live_tv_plays_the_stream_ts_through_the_relay_then_its_keyed_hls_and_takes_over_a_busy_connection() {
    let server = FixtureServer::start();
    let env = env(&server);
    let channel = env.server.live_channel(ids::LIVE_CHANNEL).expect("the channel");
    let clock = SimClock::manual();
    let (engine, sim) = SimEngine::new(clock.clone());
    let mut live = LiveSession::new(channel.clone(), &env, Box::new(engine), 0);
    let mut now = 0;
    let settle = |live: &mut LiveSession, now: i64| {
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            live.tick(now);
            if !live.busy() {
                break;
            }
            assert!(Instant::now() < until, "the server never answered");
            thread::sleep(Duration::from_millis(2));
        }
        live.tick(now);
    };
    live.tune(channel.clone());
    settle(&mut live, now);
    // The fixtures' plan copies the channel: its stream.ts plays, handed on from the loopback
    // address, read upstream with the token.
    let load = sim.loads().first().cloned().expect("the channel plays");
    assert_eq!(load.kind, MediaKind::LiveTs);
    assert!(load.url.starts_with("http://127.0.0.1:"), "{}", load.url);
    let (status, bytes) = read_from(&load.url, 188);
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(bytes.first(), Some(&0x47), "an MPEG-TS packet's sync byte");
    now += SIM_LOAD_MS;
    clock.set(now);
    settle(&mut live, now);
    assert_eq!(live.state().phase, LivePhase::Playing);
    assert!(live.direct());

    // Failing before it plays, the channel's keyed HLS.
    live.tune(channel);
    settle(&mut live, now);
    sim.fail(2, "the pipeline refused it");
    settle(&mut live, now);
    let load = sim.loads().last().cloned().expect("the channel plays");
    assert_eq!(load.kind, MediaKind::LiveHls);
    assert!(load.url.contains("/api/live/media/"), "{}", load.url);
    now += SIM_LOAD_MS;
    clock.set(now);
    settle(&mut live, now);
    assert_eq!(live.state().phase, LivePhase::Playing);
    let before = sim.loads().len();

    let busy = env.server.live_channel(ids::LIVE_CONFLICT_CHANNEL).expect("the busy channel");
    live.tune(busy);
    settle(&mut live, now);
    let trouble = live.state().trouble.clone().expect("every connection is in use");
    assert_eq!(trouble.kind, TroubleKind::Busy);
    let take = trouble.connections.iter().find(|c| c.can_take_over).expect("one may be taken over").channel_id;
    live.take_over(take);
    settle(&mut live, now);
    assert_eq!(live.state().phase, LivePhase::Tuning);
    assert_eq!(sim.loads().len(), before + 1);
    live.close();
}
