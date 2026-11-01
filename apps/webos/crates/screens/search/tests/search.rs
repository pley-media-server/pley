//! Search against the fake server, without a window: the shell with this crate's screen alone,
//! frames drawn with pley-ui's stub shaper on a clock the tests move.

use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::media::NullEngine;
use pley_core::model::LiveCategory;
use pley_core::{Device, FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_fixtures::{FixtureServer, Mode, Options, ids};
use pley_search::screen::{CANT_REACH, EMPTY_TITLE, KEYBOARD_ACTION, NONE_BODY, RECENT, RETRY, SEARCHING, none_title};
use pley_shell::{AppState, NavCommand, Registry, Route, Shell};
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, Id, InputEvent, Ui, UiState};

/// The app without a window, signed in to the fake server as Alex, on Search.
struct App {
    ui: UiState,
    list: DisplayList,
    images: StubImages,
    shell: Shell,
    state: AppState,
    store: Store,
    server: FixtureServer,
    time: f64,
}

impl App {
    fn new() -> App {
        App::with(|_, _| {})
    }

    /// The app, after `before` has had the store and the server's address.
    fn with(before: impl FnOnce(&Store, &str)) -> App {
        let server = FixtureServer::start_with(Options { scan_every: None, ..Options::default() }).unwrap();
        let device = Arc::new(FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>));
        let store = Store::open(device.data_dir().join("store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        sign_in(&server, &store, &http);
        before(&store, server.url());
        let state = AppState::new(device, store.clone(), http, Tasks::new(Waker::noop()));
        let mut registry = Registry::new();
        registry.add(pley_search::screen);
        let mut app = App {
            ui: UiState::new(),
            list: DisplayList::default(),
            images: StubImages::new(),
            shell: Shell::new(registry),
            state,
            store,
            server,
            time: pley_base::time::now(),
        };
        app.shell.start(&mut app.state, &mut app.ui, Route::Search { query: String::new() });
        app.frame(Vec::new());
        assert!(app.until(|a| !a.state.busy() && a.state.profile().is_some()), "the account never loaded");
        app
    }

    /// One frame at the current time with `events`, drawn again while focus or the page settles.
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

    /// Presses and releases `key`, then lets 150 ms pass.
    fn press(&mut self, key: Key) {
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: true, repeat: false, code: 0 })]);
        self.time += 0.05;
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: false, repeat: false, code: 0 })]);
        self.advance(0.15);
    }

    /// Text typed on a keyboard.
    fn type_text(&mut self, text: &str) {
        self.frame(vec![InputEvent::Text(text.into())]);
        self.advance(0.05);
    }

    /// Lets `secs` pass on the app's clock, drawing frames as it goes.
    fn advance(&mut self, secs: f64) {
        let end = self.time + secs;
        while self.time < end {
            self.time = (self.time + 0.05).min(end);
            self.frame(Vec::new());
        }
    }

    /// Draws frames until `done` holds, waiting for answers from other threads (10 s at most).
    fn until(&mut self, mut done: impl FnMut(&mut App) -> bool) -> bool {
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

    /// Lets typing pause and waits for what shows to say `text`.
    fn shows(&mut self, text: &str) {
        self.advance(0.3);
        assert!(self.until(|a| a.says(text)), "never showed {text:?}");
    }

    fn says(&self, text: &str) -> bool {
        self.list.says(text)
    }

    fn top(&self) -> &Route {
        self.shell.top_route().unwrap()
    }

    fn typing(&self) -> bool {
        self.shell.top().unwrap().screen().modal()
    }

    /// The field's focus id on the page on top.
    fn field(&self) -> Id {
        self.shell.top().unwrap().group_id().with("field")
    }

    fn focused(&self) -> Option<Id> {
        self.ui.focused()
    }

    fn recent(&self) -> Vec<String> {
        self.store.searches(self.server.url(), ids::ALEX)
    }

    /// Waits for the recent searches kept to be `want`: they are written on the task pool.
    fn kept(&mut self, want: &[&str]) -> bool {
        self.until(|a| a.recent() == want)
    }

    /// Presses and releases a digit key that also brings its text in the same frame, as a
    /// computer's keyboard does (and SDL may for a remote's number keys).
    fn digit_with_text(&mut self, d: u8) {
        let key = |pressed| InputEvent::Key(KeyEvent { key: Key::Digit(d), pressed, repeat: false, code: 0 });
        self.frame(vec![key(true), InputEvent::Text(d.to_string())]);
        self.time += 0.05;
        self.frame(vec![key(false)]);
        self.advance(0.05);
    }
}

/// Keeps a sign-in of Alex on `server` in `store`, and makes it the server in use.
fn sign_in(server: &FixtureServer, store: &Store, http: &Http) {
    let t = server.sign_in(ids::ALEX);
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
    store.set_last_profile(url, ids::ALEX).unwrap();
}

#[test]
fn the_field_has_focus_first_and_says_what_search_finds() {
    let app = App::new();
    assert_eq!(app.focused(), Some(app.field()));
    assert!(app.says(EMPTY_TITLE));
    assert!(app.says("Search movies, shows, people…"));
    assert!(!app.typing());
}

#[test]
fn ok_opens_the_keyboard_and_back_closes_it_leaving_focus_on_the_field() {
    let mut app = App::new();
    app.press(Key::Ok);
    assert!(app.typing());
    assert!(app.says(KEYBOARD_ACTION) && app.says("&123"));
    assert_ne!(app.focused(), Some(app.field()), "focus is on the keyboard");
    app.press(Key::Back);
    assert!(!app.typing());
    assert_eq!(app.top(), &Route::Search { query: String::new() }, "Back closed the keyboard, not the page");
    app.advance(0.3);
    assert_eq!(app.focused(), Some(app.field()));
}

#[test]
fn what_is_typed_is_asked_once_typing_pauses_and_shows_in_rows() {
    let mut app = App::new();
    app.press(Key::Ok);
    app.type_text("ma");
    assert!(!app.says("People"), "not before typing pauses");
    assert!(app.says(EMPTY_TITLE));
    app.shows("People");
    assert!(app.says("Movies") && app.says("Live TV"));
    assert!(!app.says(SEARCHING));
    assert!(app.typing(), "results come in while the keyboard shows");
}

#[test]
fn down_from_the_field_goes_to_the_first_row_and_up_comes_back() {
    let mut app = App::new();
    // Typed on a keyboard plugged into the TV, with the field focused.
    app.type_text("ma");
    app.shows("People");
    assert_eq!(app.focused(), Some(app.field()));
    app.press(Key::Down);
    let first = app.focused().unwrap();
    assert_ne!(first, app.field());
    app.press(Key::Right);
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.field()));
    app.press(Key::Down);
    assert_ne!(app.focused(), Some(first), "the row comes back to the tile it had last");
    app.press(Key::Left);
    assert_eq!(app.focused(), Some(first));
    // Right on the field stops; Left opens the rail.
    app.press(Key::Up);
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(app.field()));
    app.press(Key::Left);
    assert!(app.shell.rail_open());
}

#[test]
fn opening_a_result_keeps_the_query_and_back_returns_to_the_results() {
    let mut app = App::new();
    app.type_text("ma");
    app.shows("People");
    assert!(app.recent().is_empty());
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(matches!(app.top(), Route::Person { .. }), "opened {:?}", app.top());
    assert!(app.kept(&["ma"]));
    app.press(Key::Back);
    assert!(matches!(app.top(), Route::Search { .. }));
    assert!(app.says("People"), "the results are still there");
}

#[test]
fn holding_ok_on_a_title_opens_its_actions() {
    let mut app = App::new();
    app.type_text("ma");
    app.shows("Movies");
    // People, Live TV, then the movies.
    app.press(Key::Down);
    app.press(Key::Down);
    app.press(Key::Down);
    app.press(Key::Menu);
    assert!(matches!(app.top(), Route::ItemActions { continue_watching: false, .. }), "opened {:?}", app.top());
}

#[test]
fn a_channel_found_plays_among_every_channel() {
    let mut app = App::new();
    app.type_text(&ids::LIVE_NO_GUIDE_CHANNEL.to_string());
    app.shows("Live TV");
    assert!(app.says("No guide for this channel"));
    app.press(Key::Down);
    app.press(Key::Ok);
    match app.top() {
        Route::LivePlayer { channel_id, category, .. } => {
            assert_eq!((*channel_id, category), (ids::LIVE_NO_GUIDE_CHANNEL, &LiveCategory::All));
        }
        other => panic!("opened {other:?}"),
    }
    assert!(app.state.known_live_channel(ids::LIVE_NO_GUIDE_CHANNEL).is_some(), "the player starts with it at once");
}

#[test]
fn nothing_found_says_so() {
    let mut app = App::new();
    app.type_text("zzqx");
    app.shows(&none_title("zzqx"));
    assert!(app.says(NONE_BODY));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(app.field()), "nothing to go down to");
}

#[test]
fn search_on_the_keyboard_keeps_the_query_and_recent_searches_show_with_nothing_typed() {
    let mut app = App::with(|store, server| store.remember_search(server, ids::ALEX, "zzqx").unwrap());
    assert!(app.says(RECENT) && app.says("zzqx"));
    app.press(Key::Ok);
    app.type_text("ma");
    // The keyboard's Search key: down to the bottom row from "q", then along to it.
    for key in [Key::Down, Key::Down, Key::Down, Key::Right, Key::Right, Key::Right, Key::Right, Key::Right, Key::Ok] {
        app.press(key);
    }
    assert!(!app.typing());
    assert!(app.kept(&["ma", "zzqx"]));
    app.shows("People");
    assert_eq!(app.focused(), Some(app.field()));

    // Emptied, the field shows them again; a chip searches again and focus waits on the field.
    app.press(Key::Ok);
    for key in [Key::Down, Key::Down, Key::Down, Key::Right, Key::Right, Key::Right, Key::Right, Key::Ok, Key::Back] {
        app.press(key);
    }
    assert!(app.says(RECENT));
    app.press(Key::Down);
    app.press(Key::Right);
    app.press(Key::Ok);
    assert!(app.kept(&["zzqx", "ma"]));
    app.advance(0.1);
    assert_eq!(app.focused(), Some(app.field()));
    app.shows(&none_title("zzqx"));

    // Clear forgets them.
    app.press(Key::Ok);
    for key in [Key::Down, Key::Down, Key::Down, Key::Right, Key::Right, Key::Right, Key::Right, Key::Ok, Key::Back] {
        app.press(key);
    }
    app.press(Key::Down);
    app.press(Key::Right);
    app.press(Key::Right);
    app.press(Key::Ok);
    assert!(app.kept(&[]));
    app.advance(0.1);
    assert!(app.says(EMPTY_TITLE));
    assert_eq!(app.focused(), Some(app.field()));
}

#[test]
fn a_failure_offers_retry_below_the_field() {
    let mut app = App::new();
    app.server.set_mode(Mode::Down).unwrap();
    app.type_text("ma");
    app.shows(CANT_REACH);
    assert!(app.says(RETRY));
    assert_eq!(app.focused(), Some(app.field()), "focus stays on the field");
    app.press(Key::Down);
    assert_ne!(app.focused(), Some(app.field()));
    app.server.set_mode(Mode::Normal).unwrap();
    app.press(Key::Ok);
    app.shows("People");
    assert_eq!(app.focused(), Some(app.field()));
}

#[test]
fn a_search_opened_with_a_query_asks_it_at_once() {
    let mut app = App::new();
    app.shell.navigate(NavCommand::Reset(Route::Search { query: "ma".into() }), &mut app.state, &mut app.ui);
    app.frame(Vec::new());
    assert!(app.until(|a| a.says("People")));
}

#[test]
fn a_digit_that_brings_its_text_too_types_once() {
    let mut app = App::new();
    app.digit_with_text(4);
    app.digit_with_text(1);
    assert!(app.says("41") && !app.says("44") && !app.says("11"), "typed once on the field");
    // The remote's digits, with no text, still type.
    app.press(Key::Digit(7));
    assert!(app.says("417"));

    // On the keyboard as well.
    app.press(Key::Ok);
    assert!(app.typing());
    app.digit_with_text(2);
    assert!(app.says("4172") && !app.says("41722"));
}

#[test]
fn the_results_stay_when_asking_them_again_fails() {
    let mut app = App::new();
    app.type_text("ma");
    app.shows("People");
    app.press(Key::Down);
    app.press(Key::Right);
    let tile = app.focused();
    app.press(Key::Ok);
    assert!(matches!(app.top(), Route::Person { .. }), "opened {:?}", app.top());
    // Back after more than 30 s with the server gone: the results are asked again, and fail.
    app.server.set_mode(Mode::Down).unwrap();
    app.advance(31.0);
    app.press(Key::Back);
    assert!(app.until(|a| a.state.offline() && !a.says(SEARCHING)), "never asked again");
    assert!(app.says("People") && !app.says(CANT_REACH), "the results stay");
    assert_eq!(app.focused(), tile, "on the tile it had");
}
