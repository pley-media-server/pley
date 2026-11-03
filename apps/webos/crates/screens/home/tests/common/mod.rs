//! A headless app for Home's and the libraries list's tests: the shell with this crate's screens
//! and pley-item's tile actions alone (other pages are the shell's placeholder), over a fake TV, signed in to the fake server as
//! Alex. Frames run with pley-ui's stub shaper and pictures.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::media::NullEngine;
use pley_core::{Device, FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Mode, Options, ids};
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
    /// The TV: a drive plugged in.
    pub device: Arc<FakeDevice>,
}

impl TestApp {
    /// The app signed in to a new fake server as Alex, on Home.
    pub fn start() -> TestApp {
        TestApp::start_with(Mode::Normal)
    }

    /// The app signed in to a new fake server as Alex, the server in `mode` from the start.
    pub fn start_with(mode: Mode) -> TestApp {
        let options = Options { scan_every: None, slow_latency: Duration::from_millis(1500), ..Options::default() };
        let server = FixtureServer::start_with(options).unwrap();
        let device = Arc::new(FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>));
        let store = Store::open(device.data_dir().join("store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        sign_in(&server, ids::ALEX, &store, &http);
        server.set_mode(mode).unwrap();
        let mut registry = Registry::new();
        registry.add(pley_home::screen).tile_actions(pley_item::tile_actions);
        let state = AppState::new(device.clone(), store, http, Tasks::new(Waker::noop()));
        let mut app = TestApp {
            ui: UiState::new(),
            list: DisplayList::default(),
            images: StubImages::new(),
            shell: Shell::new(registry),
            state,
            time: pley_base::time::now(),
            server,
            device,
        };
        let route = app.state.start_route();
        app.shell.start(&mut app.state, &mut app.ui, route);
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

    /// Holds OK past a long press (500 ms), then lets it go.
    pub fn hold_ok(&mut self) {
        self.frame(vec![InputEvent::Key(KeyEvent { key: Key::Ok, pressed: true, repeat: false, code: 0 })]);
        self.advance(0.6);
        self.frame(vec![InputEvent::Key(KeyEvent { key: Key::Ok, pressed: false, repeat: false, code: 0 })]);
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
    /// other threads come (10 s at most).
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

    /// The id of `path` (keys, group by group) on the page on top.
    pub fn id(&self, path: &[&str]) -> Id {
        path.iter().fold(self.page(), |id, key| id.with(key))
    }

    /// The page on top's group.
    pub fn page(&self) -> Id {
        self.shell.top().unwrap().group_id()
    }

    /// The focused element.
    pub fn focused(&self) -> Option<Id> {
        self.ui.focused()
    }

    /// Whether the last frame drew `text`.
    pub fn shows(&self, text: &str) -> bool {
        self.list.texts().any(|t| t == text)
    }

    /// Whether the last frame drew a text starting with `start`.
    pub fn shows_start(&self, start: &str) -> bool {
        self.list.texts().any(|t| t.starts_with(start))
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
