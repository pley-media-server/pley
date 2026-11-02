//! A headless app for the shell's tests: the shell and the app's state over a fake TV, frames run
//! with pley-ui's stub shaper and pictures on a clock the test moves.

#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Rect, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::media::NullEngine;
use pley_core::{Device, FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Options};
use pley_shell::{AppCx, AppEvent, AppState, Chrome, NavCommand, Registry, Route, Screen, Shell};
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, Group, Id, InputEvent, Ui, UiState};

/// A fake TV with its folders in a new temporary folder.
pub fn device() -> Arc<FakeDevice> {
    Arc::new(FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>))
}

/// The store of `device`.
pub fn store(device: &FakeDevice) -> Store {
    use pley_core::Device;
    Store::open(device.data_dir().join("store")).unwrap()
}

/// The HTTP client, as the TV's.
pub fn http() -> Http {
    Http::network("pleyLG/0.1.0 (webOS 22; test)")
}

/// The fake server without its pretend scans, which would add events to the tests'.
pub fn fixtures() -> FixtureServer {
    FixtureServer::start_with(Options { scan_every: None, ..Options::default() }).unwrap()
}

/// Keeps a sign-in of `profile` on `server` in `store`, and makes it the server in use.
pub fn sign_in(server: &FixtureServer, profile: i64, store: &Store, http: &Http) {
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

/// What the test pages heard, newest last.
pub type Heard = Rc<RefCell<Vec<(Route, AppEvent)>>>;

/// A test page: a button at the page's left edge (OK on Home opens an item, Menu opens a panel
/// that Back closes) that records what it hears.
pub struct Page {
    pub route: Route,
    pub panel: bool,
    pub heard: Heard,
    pub ready: bool,
}

impl Screen for Page {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if !self.ready {
            return;
        }
        let id = ui.id("button");
        ui.initial_focus(id);
        let r = ui.focusable(id, Rect::new(96.0, 100.0, 120.0, 40.0));
        if r.activated {
            match self.route {
                Route::Home => cx.push(Route::Item { id: 1 }),
                _ => cx.toast("Saved"),
            }
        }
        if r.menu {
            self.panel = true;
        }
        if self.panel {
            let panel = ui.id("panel");
            ui.group(panel, Group::column().trap(), |ui| {
                let row = ui.id("row");
                ui.initial_focus(row);
                ui.focusable(row, Rect::new(600.0, 100.0, 300.0, 44.0));
            });
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        std::mem::take(&mut self.panel)
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        self.heard.borrow_mut().push((self.route.clone(), event.clone()));
    }

    fn chrome(&self) -> Chrome {
        self.route.chrome()
    }

    fn takes_focus(&self) -> bool {
        self.ready
    }

    fn modal(&self) -> bool {
        self.panel
    }
}

/// A registry drawing every page with [`Page`]; `slow` pages have nothing to focus at first.
pub fn registry(heard: &Heard, slow: &[&'static str]) -> Registry {
    let mut r = Registry::new();
    let heard = heard.clone();
    let slow: Vec<&'static str> = slow.to_vec();
    r.add(move |route, _cx| {
        let ready = !slow.contains(&route.name());
        Some(Box::new(Page { route: route.clone(), panel: false, heard: heard.clone(), ready }) as Box<dyn Screen>)
    });
    r
}

/// The app without a window.
pub struct TestApp {
    pub ui: UiState,
    pub list: DisplayList,
    pub images: StubImages,
    pub shell: Shell,
    pub state: AppState,
    pub device: Arc<FakeDevice>,
    pub time: f64,
}

impl TestApp {
    /// The app as it starts on `device` with `store`: the start gate's page, one frame drawn.
    pub fn new(device: Arc<FakeDevice>, store: Store, http: Http, registry: Registry) -> TestApp {
        let tasks = Tasks::new(Waker::noop());
        let mut state = AppState::new(device.clone(), store, http, tasks);
        let mut ui = UiState::new();
        let mut shell = Shell::new(registry);
        let route = state.start_route();
        shell.start(&mut state, &mut ui, route);
        let mut app =
            TestApp { ui, list: DisplayList::default(), images: StubImages::new(), shell, state, device, time: 0.0 };
        app.time = pley_base::time::now();
        app.frame(Vec::new());
        app
    }

    /// One frame at the current time with `events`, drawn again while focus or the page settles.
    pub fn frame(&mut self, events: Vec<InputEvent>) {
        let now = self.time;
        let mut input = events;
        self.shell.filter_input(&mut input, now);
        self.shell.update(&mut self.state, &mut self.ui, now);
        // As the app's loop does: Audio Guidance as the TV has it, and what it reads.
        self.ui.set_speaking(self.device.screen_reader());
        for _ in 0..4 {
            let frame = FrameInput { time: now, events: std::mem::take(&mut input) };
            let mut ui = Ui::new(&mut self.ui, &mut self.list, &StubShaper, &mut self.images, 2.0, &frame);
            self.shell.frame(&mut ui, &mut self.state);
            let out = ui.finish();
            let changed = self.shell.finish(&mut self.state, &mut self.ui, now);
            if let Some(words) = self.ui.take_speech() {
                self.device.speak(&words);
            }
            if !(out.redraw || changed) {
                break;
            }
        }
    }

    /// Presses and releases `key`, a frame for each, then lets 150 ms pass.
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

    /// Draws frames until `done` holds, waiting for answers from other threads (10 s at most).
    pub fn until(&mut self, mut done: impl FnMut(&mut TestApp) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            self.frame(Vec::new());
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Draws frames until the state has no answer on its way.
    pub fn settle(&mut self) {
        assert!(self.until(|app| !app.state.busy()), "answers never came");
    }

    /// The focused element.
    pub fn focused(&self) -> Option<Id> {
        self.ui.focused()
    }

    /// The focus id of the button on the page on top.
    pub fn button(&self) -> Id {
        self.shell.top().unwrap().group_id().with("button")
    }

    /// Opens `cmd` from outside a frame.
    pub fn navigate(&mut self, cmd: NavCommand) {
        self.shell.navigate(cmd, &mut self.state, &mut self.ui);
        self.frame(Vec::new());
    }
}
