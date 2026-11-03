//! A headless app for the music pages' tests: the shell with this crate's screens alone, over a
//! fake TV whose player is pley-player's simulated engine, signed in to the fake server as Alex.
//! Frames run with pley-ui's stub shaper and pictures.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Options, ids};
use pley_music::MusicPlayer;
use pley_player::{MusicSession, SimClock, SimEngine};
use pley_shell::{AppState, NavCommand, Registry, Route, Shell};
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, Id, InputEvent, Ui, UiState};

/// The app without a window, and the fake server it is signed in to.
pub struct TestApp {
    pub ui: UiState,
    pub list: DisplayList,
    pub images: StubImages,
    pub shell: Shell,
    pub state: AppState,
    pub time: f64,
    pub server: FixtureServer,
}

impl TestApp {
    /// The app signed in to a new fake server as Alex, on Home (or wherever it starts).
    pub fn start() -> TestApp {
        let server = FixtureServer::start_with(Options { scan_every: None, ..Options::default() }).unwrap();
        let device =
            Arc::new(FakeDevice::new(|| Box::new(SimEngine::new(SimClock::system()).0) as Box<dyn PlayerEngine>));
        let store = {
            use pley_core::Device;
            Store::open(device.data_dir().join("store")).unwrap()
        };
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        sign_in(&server, ids::ALEX, &store, &http);
        let mut registry = Registry::new();
        registry.add(pley_music::screen);
        let mut state = AppState::new(device, store, http, Tasks::new(Waker::noop()));
        let mut ui = UiState::new();
        let mut shell = Shell::new(registry);
        let route = state.start_route();
        shell.start(&mut state, &mut ui, route);
        let mut app = TestApp {
            ui,
            list: DisplayList::default(),
            images: StubImages::new(),
            shell,
            state,
            time: pley_base::time::now(),
            server,
        };
        app.frame(Vec::new());
        app
    }

    /// One frame at the app's time with `events`, drawn again while focus or the page settles.
    pub fn frame(&mut self, events: Vec<InputEvent>) {
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

    /// Presses and releases `key`, then lets 150 ms pass.
    pub fn press(&mut self, key: Key) {
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: true, repeat: false, code: 0 })]);
        self.time += 0.05;
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: false, repeat: false, code: 0 })]);
        self.advance(0.15);
    }

    /// Lets `secs` pass on the app's clock, drawing frames as it goes.
    pub fn advance(&mut self, secs: f64) {
        let end = self.time + secs;
        while self.time < end {
            self.time = (self.time + 0.05).min(end);
            self.frame(Vec::new());
        }
    }

    /// Draws frames until `done` holds, the app's clock following the real one, so answers from
    /// other threads and the simulated engine's clock come (10 s at most).
    pub fn until(&mut self, mut done: impl FnMut(&mut TestApp) -> bool) -> bool {
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

    /// Opens `route` over the page on top.
    pub fn open(&mut self, route: Route) {
        self.shell.navigate(NavCommand::Push(route), &mut self.state, &mut self.ui);
        self.frame(Vec::new());
    }

    /// The page on top.
    pub fn route(&self) -> Option<&Route> {
        self.shell.top_route()
    }

    /// The focus id of `path` (keys, group by group) on the page on top.
    pub fn id(&self, path: &[&str]) -> Id {
        path.iter().fold(self.shell.top().unwrap().group_id(), |id, key| id.with(key))
    }

    /// The focused element.
    pub fn focused(&self) -> Option<Id> {
        self.ui.focused()
    }

    /// Whether the last frame drew `text`.
    pub fn shows(&self, text: &str) -> bool {
        self.list.texts().any(|t| t == text)
    }

    /// The app's music player, once something played.
    pub fn player(&mut self) -> Option<&mut MusicPlayer> {
        self.state.music.as_mut()?.as_any_mut().downcast_mut::<MusicPlayer>()
    }

    /// The music session, once something played.
    pub fn music(&mut self) -> Option<&mut MusicSession> {
        Some(self.player()?.session_mut())
    }

    /// The track playing now and whether it plays.
    pub fn now_playing(&mut self) -> Option<(i64, bool)> {
        self.music().and_then(|m| m.now_playing()).map(|n| (n.track.id, n.playing))
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
