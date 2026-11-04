//! The player as the user's remote on another device drives it, against the fake server through
//! the app's event stream: the tracks it picks, switched as the player's own menu switches them
//! and said at once, and every progress report saying which play.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Options, ServerEvent, ids};
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
        let server = FixtureServer::start_with(Options { scan_every: None, ..Options::default() }).unwrap();
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

/// The remote picking track `index` of the kind of `command` for `session`.
fn pick(session: &str, command: &str, index: i64) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command: command.into(),
        position_ms: None,
        offset_ms: None,
        stream_index: Some(index),
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
    assert_eq!(controls, ["play", "pause", "seek", "skip", "audio", "subtitles"]);
    let session = listed["session_id"].as_str().unwrap().to_string();
    let engine = app.engine().expect("the player made an engine");
    assert!(app.until(|_| engine.playing()), "the film never played");

    // The French picture subtitles, drawn here from the server's .mks: said at once.
    let before = app.reports(ids::PGS_MOVIE).len();
    app.server.emit(pick(&session, "subtitles", ids::PGS_MOVIE_SUBTITLE));
    assert!(app.until(|a| a.reports(ids::PGS_MOVIE).len() > before), "the pick wasn't said at once");
    assert_eq!(tracks(&app.reports(ids::PGS_MOVIE)[before]), (Some(1), Some(ids::PGS_MOVIE_SUBTITLE)));
    let shown = |a: &TestApp| a.server.sessions_of(ids::ALEX).into_iter().find(|s| s["session_id"] == *session);
    let kept = shown(&app).unwrap();
    assert_eq!((kept["audio_index"].as_i64(), kept["subtitle_index"].as_i64()), (Some(1), Some(4)));

    // The French audio, switched in place by the TV's player, in the same session.
    let before = app.reports(ids::PGS_MOVIE).len();
    app.server.emit(pick(&session, "audio", 2));
    assert!(app.until(|a| a.reports(ids::PGS_MOVIE).len() > before), "the pick wasn't said at once");
    assert_eq!(tracks(&app.reports(ids::PGS_MOVIE)[before]), (Some(2), Some(4)));
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
