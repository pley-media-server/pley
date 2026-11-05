//! The player as the user's remote on another device drives it, against the fake server through
//! the app's event stream: a film it casts, played from where it says with where it came from over
//! the picture, with the tracks picked on its page (and nothing for another device's cast, or
//! while the app is hidden); the tracks and the quality it picks, switched as the player's own
//! menus switch them and said at once, and every progress report saying which play.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{CastBody, FixtureServer, Options, ServerEvent, ids};
use pley_player::{SimClock, SimEngine, SimHandle};
use pley_shell::{AppState, NavCommand, PlayRequest, Registry, Route, Shell};
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, InputEvent, Ui, UiState};
use serde_json::Value;

/// The app with the player alone, signed in to the fake server as Alex, over a fake TV whose
/// engines the test watches.
struct TestApp {
    ui: UiState,
    list: DisplayList,
    images: StubImages,
    shell: Shell,
    state: AppState,
    time: f64,
    server: FixtureServer,
    engines: Arc<Mutex<Vec<SimHandle>>>,
}

impl TestApp {
    fn start() -> TestApp {
        // Writing to its streams often, the server hears soon of one the app closed.
        let options = Options { scan_every: None, keep_alive: Duration::from_millis(200), ..Options::default() };
        let server = FixtureServer::start_with(options).unwrap();
        let engines: Arc<Mutex<Vec<SimHandle>>> = Arc::default();
        let made = engines.clone();
        let device = Arc::new(FakeDevice::new(move || {
            let (engine, handle) = SimEngine::new(SimClock::system());
            made.lock().unwrap().push(handle);
            Box::new(engine) as Box<dyn PlayerEngine>
        }));
        let store = Store::open(pley_core::Device::data_dir(device.as_ref()).join("store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        sign_in(&server, ids::ALEX, &store, &http);
        let mut registry = Registry::new();
        registry.add(pley_player_ui::screen);
        let state = AppState::new(device, store, http, Tasks::new(Waker::noop()));
        let mut app = TestApp {
            ui: UiState::new(),
            list: DisplayList::default(),
            images: StubImages::new(),
            shell: Shell::new(registry),
            state,
            time: pley_base::time::now(),
            server,
            engines,
        };
        let route = app.state.start_route();
        app.shell.start(&mut app.state, &mut app.ui, route);
        app.frame(Vec::new());
        app
    }

    fn frame(&mut self, events: Vec<InputEvent>) {
        let now = self.time;
        let mut input = events;
        self.shell.filter_input(&mut input, now);
        self.shell.update(&mut self.state, &mut self.ui, now);
        for _ in 0..4 {
            let frame = FrameInput { time: now, events: std::mem::take(&mut input) };
            let mut ui = Ui::new(&mut self.ui, &mut self.list, &StubShaper, &mut self.images, 2.0, &frame);
            self.shell.frame(&mut ui, &mut self.state);
            let out = ui.finish();
            let changed = self.shell.finish(&mut self.state, &mut self.ui, now);
            if !(out.redraw || changed) {
                break;
            }
        }
    }

    fn press(&mut self, key: Key) {
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: true, repeat: false, code: 0 })]);
        self.time += 0.05;
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: false, repeat: false, code: 0 })]);
    }

    /// Lets `secs` pass on the app's clock, drawing frames as it goes.
    fn advance(&mut self, secs: f64) {
        let end = self.time + secs;
        while self.time < end {
            self.time = (self.time + 0.05).min(end);
            self.frame(Vec::new());
        }
    }

    /// Draws frames until `done` holds, the app's clock following the real one, so answers from
    /// other threads come (10 s at most).
    fn until(&mut self, mut done: impl FnMut(&mut TestApp) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            self.time = self.time.max(pley_base::time::now());
            self.frame(Vec::new());
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn open(&mut self, route: Route) {
        self.shell.navigate(NavCommand::Push(route), &mut self.state, &mut self.ui);
        self.frame(Vec::new());
    }

    /// The TV's last player.
    fn engine(&self) -> Option<SimHandle> {
        self.engines.lock().unwrap().last().cloned()
    }

    /// Alex's session of `item` once the server lists it, and the event stream the player hears
    /// the remote on is open.
    fn session_of(&mut self, item: i64) -> Value {
        let mut found = None;
        let heard = self.until(|a| {
            found = a.server.sessions_of(ids::ALEX).into_iter().find(|s| s["item_id"] == item);
            found.is_some() && a.server.event_listeners() > 0
        });
        assert!(heard, "the player never started a session of {item}, or the events never connected");
        found.unwrap()
    }

    /// The progress reports of `item` the server had, as their JSON.
    fn reports(&self, item: i64) -> Vec<Value> {
        let writes = self.server.writes("POST", &format!("/api/items/{item}/progress"));
        writes.iter().map(|w| serde_json::from_str(&w.body).unwrap()).collect()
    }

    /// Whether the server lists the TV as one of Alex's devices that take casts.
    fn receiving(&self) -> bool {
        let id = self.state.install_id();
        self.server.receivers_of(ids::ALEX).iter().any(|r| r["receiver_id"] == id)
    }

    /// Whether the last frame drew `text`.
    fn shows(&self, text: &str) -> bool {
        self.list.texts().any(|t| t == text)
    }

    /// Whether the last frame drew a text that starts with `start`.
    fn shows_start(&self, start: &str) -> bool {
        self.list.texts().any(|t| t.starts_with(start))
    }
}

/// Where a phone's remote casts from.
const PHONE: &str = "pleyAndroid/0.1.5 (Android 14; phone)";

/// A cast to `receiver` of `item`, as the server sends it to every stream of the profile.
fn cast_of(receiver: &str, item: i64) -> ServerEvent {
    ServerEvent::Cast {
        receiver_id: receiver.into(),
        item_id: Some(item),
        playlist_id: None,
        channel_id: None,
        start_ms: None,
        shuffle: false,
        audio_index: None,
        subtitle_index: None,
        from: Some("Pley for Android".into()),
    }
}

/// Keeps a sign-in of `profile` on `server` in `store`, and makes it the server in use.
fn sign_in(server: &FixtureServer, profile: i64, store: &Store, http: &Http) {
    let t = server.sign_in(profile);
    let parsed = TokenAnswerParsed {
        access_token: t.access_token,
        refresh_token: Some(t.refresh_token),
        expires_in: t.expires_in as u64,
        scope: t.scope,
        proof: None,
    };
    let url = server.url();
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let stored =
        StoredTokens::new(&parsed, url, &format!("{url}/api/oauth/token"), &format!("{url}/api/oauth/revoke"), now_ms);
    Tokens::shared(url, store, http).install(stored, &parsed.scope).unwrap();
    store.set_active_server(Some(url)).unwrap();
}

/// The remote's `command` for `session`, with where to or how far.
fn command(session: &str, command: &str, position_ms: Option<i64>, offset_ms: Option<i64>) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command: command.into(),
        position_ms,
        offset_ms,
        stream_index: None,
        quality: None,
    }
}

/// The remote picking track `index` of the kind of `command` for `session`.
fn pick(session: &str, command: &str, index: i64) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command: command.into(),
        position_ms: None,
        offset_ms: None,
        stream_index: Some(index),
        quality: None,
    }
}

/// The tracks a report said: its audio and its subtitles.
fn tracks(report: &Value) -> (Option<i64>, Option<i64>) {
    (report["audio_index"].as_i64(), report["subtitle_index"].as_i64())
}

#[test]
fn the_remotes_tracks_switch_as_the_menus_do_and_every_report_says_which_play() {
    let mut app = TestApp::start();
    // English AC-3 and French E-AC-3, English text and French picture subtitles; the server
    // chooses the English audio, and the profile's defaults no subtitles.
    app.open(Route::Player(PlayRequest::item(ids::PGS_MOVIE)));
    let listed = app.session_of(ids::PGS_MOVIE);
    let controls: Vec<&str> = listed["controls"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert_eq!(controls, ["play", "pause", "seek", "skip", "audio", "subtitles", "quality"]);
    let session = listed["session_id"].as_str().unwrap().to_string();
    let engine = app.engine().expect("the player made an engine");
    assert!(app.until(|_| engine.playing()), "the film never played");

    // The French picture subtitles, drawn here from the server's .mks: said at once, to the
    // remote and on the TV.
    let before = app.reports(ids::PGS_MOVIE).len();
    app.server.emit(pick(&session, "subtitles", ids::PGS_MOVIE_SUBTITLE));
    assert!(app.until(|a| a.reports(ids::PGS_MOVIE).len() > before), "the pick wasn't said at once");
    assert!(app.until(|a| a.shows_start("Subtitles: French")), "the TV didn't say the pick");
    assert_eq!(tracks(&app.reports(ids::PGS_MOVIE)[before]), (Some(1), Some(ids::PGS_MOVIE_SUBTITLE)));
    let shown = |a: &TestApp| a.server.sessions_of(ids::ALEX).into_iter().find(|s| s["session_id"] == *session);
    let kept = shown(&app).unwrap();
    assert_eq!((kept["audio_index"].as_i64(), kept["subtitle_index"].as_i64()), (Some(1), Some(4)));

    // The French audio, switched in place by the TV's player, in the same session.
    let before = app.reports(ids::PGS_MOVIE).len();
    app.server.emit(pick(&session, "audio", 2));
    assert!(app.until(|a| a.reports(ids::PGS_MOVIE).len() > before), "the pick wasn't said at once");
    assert_eq!(tracks(&app.reports(ids::PGS_MOVIE)[before]), (Some(2), Some(4)));
    assert!(app.until(|a| a.shows_start("Audio: French")), "the TV didn't say the pick");
    assert_eq!(engine.switched().last(), Some(&1), "the second audio track wasn't switched to");
    assert_eq!(app.server.writes("POST", "/api/playback").len(), 1, "the audio took a new session");

    // Off, then off again: what shows already is said again, so a remote a little behind settles.
    app.server.emit(pick(&session, "subtitles", -1));
    app.server.emit(pick(&session, "subtitles", -1));
    assert!(app.until(|a| a.reports(ids::PGS_MOVIE).len() >= before + 3), "the picks weren't said at once");
    let reports = app.reports(ids::PGS_MOVIE);
    let said: Vec<_> = reports[reports.len() - 2..].iter().map(tracks).collect();
    assert_eq!(said, [(Some(2), Some(-1)), (Some(2), Some(-1))]);

    // Leaving, the last report says them too.
    app.press(Key::Back);
    assert!(
        app.until(|a| a.reports(ids::PGS_MOVIE).iter().any(|r| r["state"] == "stopped")),
        "the player never said it stopped"
    );
    let stopped = app.reports(ids::PGS_MOVIE).into_iter().find(|r| r["state"] == "stopped").unwrap();
    assert_eq!(tracks(&stopped), (Some(2), Some(-1)));
}

#[test]
fn the_remotes_commands_are_said_for_2_s_at_the_top_right_and_never_bring_up_the_controls() {
    let mut app = TestApp::start();
    app.open(Route::Player(PlayRequest::item(ids::SAMPLE_HDR10)));
    let session = app.session_of(ids::SAMPLE_HDR10)["session_id"].as_str().unwrap().to_string();
    let engine = app.engine().expect("the player made an engine");
    assert!(app.until(|_| engine.playing()), "the film never played");

    // Paused, the controls would say when it ends: they don't show.
    app.server.emit(command(&session, "pause", None, None));
    assert!(app.until(|a| a.shows("Paused")), "the TV didn't say the pause");
    assert!(!engine.playing(), "the remote's pause didn't pause");
    assert!(!app.shows_start("Ends at"), "the remote's pause brought up the controls");
    app.advance(1.8);
    assert!(app.shows("Paused"), "the notice went early");
    app.advance(0.4);
    assert!(!app.shows("Paused"), "the notice stayed");

    app.server.emit(command(&session, "skip", None, Some(-10_000)));
    assert!(app.until(|a| a.shows("−10 s")), "the TV didn't say the skip");
    app.server.emit(command(&session, "seek", Some(12_000), None));
    assert!(app.until(|a| a.shows("Jumped to 0:12")), "the TV didn't say the seek");
    assert!(!app.shows_start("Ends at"), "the remote's seek brought up the controls");
    app.server.emit(command(&session, "play", None, None));
    assert!(app.until(|a| a.shows("Playing")), "the TV didn't say the play");
    assert!(engine.playing(), "the remote's play didn't play");
}

#[test]
fn a_cast_film_plays_from_where_the_remote_said_with_where_it_came_from_over_it() {
    let mut app = TestApp::start();
    assert!(app.until(|a| a.receiving()), "the TV never took casts");
    let id = app.state.install_id().to_string();
    let film = CastBody { start_ms: Some(6_000), ..CastBody::item(ids::SAMPLE_HDR10) };
    assert_eq!(app.server.cast(ids::ALEX, &id, &film, Some(PHONE)), 202);

    // The player opens over the page, asks the server for the film from there, and the TV's
    // player loads it there.
    assert!(app.until(|a| a.engine().is_some_and(|e| !e.loads().is_empty())), "the film never loaded");
    assert!(matches!(app.shell.top_route(), Some(Route::Player(r)) if r.item_id == ids::SAMPLE_HDR10));
    let asked: Value = serde_json::from_str(&app.server.writes("POST", "/api/playback")[0].body).unwrap();
    assert_eq!((asked["item_id"].as_i64(), asked["start_ms"].as_i64()), (Some(ids::SAMPLE_HDR10), Some(6_000)));
    assert_eq!(app.engine().unwrap().loads()[0].start_ms, 6_000);
    // "Playing from" over the player, for about 4 s.
    assert!(app.until(|a| a.shows("Playing from Pley for Android")), "no word of where it came from");
    app.advance(pley_shell::CAST_NOTICE_SECONDS - 0.5);
    assert!(app.shows("Playing from Pley for Android"), "the notice went early");
    app.advance(1.0);
    assert!(!app.shows("Playing from Pley for Android"), "the notice stayed");

    // Another cast replaces it: the film before ends on the server, and the other plays.
    let first = app.session_of(ids::SAMPLE_HDR10)["session_id"].as_str().unwrap().to_string();
    app.server.emit(cast_of(&id, ids::SAMPLE_1080P));
    assert!(app.until(|a| a.engines.lock().unwrap().len() == 2), "the second film never started");
    assert!(
        app.until(|a| !a.server.writes("DELETE", &format!("/api/playback/{first}")).is_empty()),
        "the film before never ended"
    );
    assert_eq!(app.shell.routes().len(), 2, "the second player opened over the first");
}

#[test]
fn a_cast_film_plays_with_the_tracks_picked_on_the_remotes_page_for_that_play() {
    let mut app = TestApp::start();
    assert!(app.until(|a| a.receiving()), "the TV never took casts");
    let id = app.state.install_id().to_string();
    // The French audio and the English text subtitles, picked on the phone.
    let film = CastBody { audio_index: Some(2), subtitle_index: Some(3), ..CastBody::item(ids::PGS_MOVIE) };
    assert_eq!(app.server.cast(ids::ALEX, &id, &film, Some(PHONE)), 202);
    let session = app.session_of(ids::PGS_MOVIE)["session_id"].as_str().unwrap().to_string();
    let asked: Value = serde_json::from_str(&app.server.writes("POST", "/api/playback")[0].body).unwrap();
    assert_eq!(asked["audio_index"].as_i64(), Some(2));
    let engine = app.engine().expect("the player made an engine");
    assert!(app.until(|_| engine.playing()), "the film never played");

    // What it plays, said when the remote asks: the cast's tracks.
    let before = app.reports(ids::PGS_MOVIE).len();
    app.server.emit(ServerEvent::PlayerCommand {
        session_id: session,
        command: "play".into(),
        position_ms: None,
        offset_ms: None,
        stream_index: None,
        quality: None,
    });
    assert!(app.until(|a| a.reports(ids::PGS_MOVIE).len() > before), "nothing was said");
    assert_eq!(tracks(&app.reports(ids::PGS_MOVIE)[before]), (Some(2), Some(3)));
    // For that play only: nothing is saved for the film.
    assert!(app.server.writes("PUT", &format!("/api/items/{}/streams", ids::PGS_MOVIE)).is_empty());
}

#[test]
fn the_remotes_quality_plays_as_the_menus_does_and_reports_say_it() {
    let mut app = TestApp::start();
    app.open(Route::Player(PlayRequest::item(ids::SAMPLE_HDR10)));
    let listed = app.session_of(ids::SAMPLE_HDR10);
    let session = listed["session_id"].as_str().unwrap().to_string();
    let engine = app.engine().expect("the player made an engine");
    assert!(app.until(|_| engine.playing()), "the film never played");

    // 8 Mbps 1080p, as picked in the Quality menu: a new session, which says the pick.
    app.server.emit(ServerEvent::PlayerCommand {
        session_id: session,
        command: "quality".into(),
        position_ms: None,
        offset_ms: None,
        stream_index: None,
        quality: Some("8m".into()),
    });
    assert!(app.until(|a| a.server.writes("POST", "/api/playback").len() == 2), "no new session");
    let asked: Value = serde_json::from_str(&app.server.writes("POST", "/api/playback")[1].body).unwrap();
    assert_eq!((asked["max_bitrate"].as_i64(), asked["max_height"].as_i64()), (Some(8_000), Some(1080)));
    assert!(
        app.until(|a| a.server.sessions_of(ids::ALEX).iter().any(|s| s["quality"] == "8m")),
        "the remote never saw the pick"
    );
}

#[test]
fn a_cast_for_another_device_or_while_the_app_is_hidden_plays_nothing() {
    let mut app = TestApp::start();
    assert!(app.until(|a| a.receiving()), "the TV never took casts");
    let id = app.state.install_id().to_string();
    // The profile's streams hear every cast; another device's is its own.
    app.server.emit(cast_of("webos-another", ids::SAMPLE_1080P));
    let start = Instant::now();
    app.until(|_| start.elapsed() > Duration::from_millis(500));
    assert!(app.engine().is_none(), "the TV played another device's cast");
    assert_eq!(app.shell.depth(), 1);

    // Hidden, the TV takes no casts: the server no longer lists it, and nothing plays when it
    // shows again.
    let now = app.time;
    app.shell.visible(false, &mut app.state, &mut app.ui, now);
    assert!(app.until(|a| !a.receiving()), "the TV stayed a receiver while hidden");
    assert_eq!(app.server.cast(ids::ALEX, &id, &CastBody::item(ids::SAMPLE_1080P), Some(PHONE)), 404);
    let now = app.time;
    app.shell.visible(true, &mut app.state, &mut app.ui, now);
    assert!(app.until(|a| a.receiving()), "the TV didn't take casts again");
    assert!(app.engine().is_none() && app.server.writes("POST", "/api/playback").is_empty());
}
