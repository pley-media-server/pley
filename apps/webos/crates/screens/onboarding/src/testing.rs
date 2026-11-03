//! A headless rig for this crate's tests: one screen drawn with pley-ui's stub shaper over the
//! app's real state on a fake TV, against the fake server when a test needs one.

use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::media::NullEngine;
use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Options};
use pley_shell::{AppCx, AppState, NavCommand, Requests, Route, Screen};
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, Id, InputEvent, Ui, UiState};

/// The fake server, its sign-in codes polled every second and without its pretend scans.
pub fn fixtures() -> FixtureServer {
    FixtureServer::start_with(Options { scan_every: None, device_interval_secs: 1, ..Options::default() }).unwrap()
}

/// A screen's surroundings: the app's state, what its frames asked of the shell, and the last
/// frame drawn.
pub struct Rig {
    pub device: Arc<FakeDevice>,
    pub state: AppState,
    pub ui: UiState,
    pub list: DisplayList,
    /// Every navigation the screen asked for, oldest first.
    pub nav: Vec<NavCommand>,
    images: StubImages,
    pub time: f64,
}

impl Rig {
    /// A TV that chose no server yet.
    pub fn new() -> Rig {
        let device = Arc::new(FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>));
        Rig::on(device, |_, _| {})
    }

    /// A TV signed in to `server` as `profile`, the server in use.
    pub fn signed_in(server: &FixtureServer, profile: i64) -> Rig {
        let device = Arc::new(FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>));
        Rig::on(device, |store, http| {
            let t = server.sign_in(profile);
            let parsed = TokenAnswerParsed {
                access_token: t.access_token,
                refresh_token: Some(t.refresh_token),
                expires_in: t.expires_in as u64,
                scope: t.scope,
                proof: None,
            };
            let url = server.url();
            let now_ms = now_ms();
            let token = format!("{url}/api/oauth/token");
            let revoke = format!("{url}/api/oauth/revoke");
            let stored = StoredTokens::new(&parsed, url, &token, &revoke, now_ms);
            Tokens::shared(url, store, http).install(stored, &parsed.scope).unwrap();
            store.remember_server(url, "Fixture Server", None, now_ms).unwrap();
            store.set_active_server(Some(url)).unwrap();
        })
    }

    fn on(device: Arc<FakeDevice>, prepare: impl FnOnce(&Store, &Http)) -> Rig {
        use pley_core::Device;
        let store = Store::open(device.data_dir().join("store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        prepare(&store, &http);
        let state = AppState::new(device.clone(), store, http, Tasks::new(Waker::noop()));
        Rig {
            device,
            state,
            ui: UiState::new(),
            list: DisplayList::default(),
            nav: Vec::new(),
            images: StubImages::new(),
            time: pley_base::time::now(),
        }
    }

    /// Runs `f` with an [`AppCx`] outside a frame (to make a screen).
    pub fn cx<T>(&mut self, f: impl FnOnce(&mut AppCx) -> T) -> T {
        let mut requests = Requests::default();
        let out = {
            let mut cx = AppCx::new(&mut self.state, &mut requests, self.time);
            f(&mut cx)
        };
        self.nav.extend(requests.nav);
        out
    }

    /// One frame of `screen` with `events`, drawn again while focus settles, as the shell does.
    pub fn frame(&mut self, screen: &mut dyn Screen, events: Vec<InputEvent>) {
        let now = self.time;
        for event in self.state.update(now) {
            self.cx(|cx| screen.event(&event, cx));
        }
        let mut input = events;
        for _ in 0..3 {
            let frame = FrameInput { time: now, events: std::mem::take(&mut input) };
            let mut list = DisplayList::default();
            let mut requests = Requests::default();
            let out = {
                let mut ui = Ui::new(&mut self.ui, &mut list, &StubShaper, &mut self.images, 2.0, &frame);
                let mut cx = AppCx::new(&mut self.state, &mut requests, now);
                screen.ui(&mut ui, &mut cx);
                ui.finish()
            };
            self.list = list;
            self.nav.extend(requests.nav);
            if !out.redraw {
                break;
            }
        }
    }

    /// Presses and releases `key`, then lets 150 ms pass.
    pub fn press(&mut self, screen: &mut dyn Screen, key: Key) {
        let event = |pressed| InputEvent::Key(KeyEvent { key, pressed, repeat: false, code: 0 });
        self.frame(screen, vec![event(true)]);
        self.time += 0.05;
        self.frame(screen, vec![event(false)]);
        self.advance(screen, 0.15);
    }

    /// Holds OK down for `secs`, then lets it go.
    pub fn hold_ok(&mut self, screen: &mut dyn Screen, secs: f64) {
        let event = |pressed| InputEvent::Key(KeyEvent { key: Key::Ok, pressed, repeat: false, code: 0 });
        self.frame(screen, vec![event(true)]);
        self.advance(screen, secs);
        self.frame(screen, vec![event(false)]);
        self.advance(screen, 0.15);
    }

    /// Lets `secs` pass on the rig's clock, a frame every 50 ms.
    pub fn advance(&mut self, screen: &mut dyn Screen, secs: f64) {
        let end = self.time + secs;
        while self.time < end {
            self.time = (self.time + 0.05).min(end);
            self.frame(screen, Vec::new());
        }
    }

    /// Draws frames on the real clock until `done` holds (10 s at most).
    pub fn until(&mut self, screen: &mut dyn Screen, mut done: impl FnMut(&Rig) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            self.time = pley_base::time::now().max(self.time + 0.001);
            self.frame(screen, Vec::new());
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Whether some string of the last frame contains `needle`.
    pub fn says(&self, needle: &str) -> bool {
        self.list.says(needle)
    }

    /// The focused element.
    pub fn focused(&self) -> Option<Id> {
        self.ui.focused()
    }

    /// Whether the screen asked to start over from `route`.
    pub fn reset_to(&self, route: &Route) -> bool {
        self.nav.iter().any(|n| matches!(n, NavCommand::Reset(r) if r == route))
    }
}

/// Milliseconds since 1970.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
