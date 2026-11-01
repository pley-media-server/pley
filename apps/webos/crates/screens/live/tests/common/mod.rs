//! A headless app for Live TV's tests: the shell with this crate's screens alone (other pages are the
//! shell's placeholder), over a fake TV whose player is pley-player's `SimEngine`, signed in to the
//! fake server. Frames run with pley-ui's stub shaper and pictures.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Options, ids};
use pley_player::{SimClock, SimEngine, SimHandle};
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
    /// The players the TV made, the last last.
    pub engines: Arc<Mutex<Vec<SimHandle>>>,
}

impl TestApp {
    /// The app signed in to a new fake server as Alex.
    pub fn start() -> TestApp {
        TestApp::start_as(ids::ALEX)
    }

    /// The app signed in to a new fake server as `profile`.
    pub fn start_as(profile: i64) -> TestApp {
        let options = Options { scan_every: None, slow_latency: Duration::from_millis(1500), ..Options::default() };
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
        sign_in(&server, profile, &store, &http);
        let mut registry = Registry::new();
        registry.add(pley_live::screen);
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

    /// Types `text` as the platform would deliver it to a field being typed into.
    pub fn type_text(&mut self, text: &str) {
        self.frame(vec![InputEvent::Text(text.to_string())]);
        self.advance(0.1);
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

    /// The watches of channel `id` the server had.
    pub fn watches(&self, id: i64) -> usize {
        self.server.writes("POST", &format!("/api/live/channels/{id}/watch")).len()
    }

    /// The TV's last player.
    pub fn engine(&self) -> Option<SimHandle> {
        self.engines.lock().unwrap().last().cloned()
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
    };
    let url = server.url();
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let stored =
        StoredTokens::new(&parsed, url, &format!("{url}/api/oauth/token"), &format!("{url}/api/oauth/revoke"), now_ms);
    Tokens::shared(url, store, http).install(stored, &parsed.scope).unwrap();
    store.set_active_server(Some(url)).unwrap();
}
