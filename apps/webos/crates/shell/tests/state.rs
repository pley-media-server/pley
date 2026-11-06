//! The app's state against the fake server: saved first then fresh, refreshes driven by the
//! server's events (gathered, and held while a playback is on top), the server out of reach and
//! back, signing out, and "saved first" for a screen's own answers.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use common::{Heard, TestApp, device, fixtures, http, registry, sign_in, store};
use pley_core::store::snapshot_keys;
use pley_fixtures::{Mode, ServerEvent, ids};
use pley_shell::{AppEvent, Load, OFFLINE_ASK, OFFLINE_NOTICE, Resource, Route};

fn signed_in_app(server: &pley_fixtures::FixtureServer, heard: &Heard) -> TestApp {
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(server, ids::ALEX, &store, &http);
    TestApp::new(device, store, http, registry(heard, &[]))
}

fn changes(heard: &Heard) -> usize {
    heard.borrow().iter().filter(|(_, e)| *e == AppEvent::Changed).count()
}

fn heard_server_event(heard: &Heard, pick: impl Fn(&pley_core::model::ServerEvent) -> bool) -> bool {
    heard.borrow().iter().any(|(_, e)| matches!(e, AppEvent::Server(s) if pick(s)))
}

#[test]
fn answers_show_saved_first_then_fresh() {
    let server = fixtures();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, ids::ALEX, &store, &http);
    let mut app = TestApp::new(device.clone(), store.clone(), http.clone(), registry(&heard, &[]));
    // Nothing was saved yet; the first frame asks.
    assert!(app.state.me().is_none() && app.state.home().is_none());
    app.settle();
    assert_eq!(app.state.me().map(|m| m.name.as_str()), Some("Alex"));
    assert!(!app.state.libraries().unwrap().is_empty());
    assert!(!app.state.home().unwrap().is_empty());
    assert!(app.state.home_fresh());
    assert!(app.state.live_available());
    let profile = app.state.profile().unwrap();
    let server_url = server.url().to_string();
    let snapshots = app.state.snapshots().clone();
    let start = Instant::now();
    while snapshots.read(&server_url, profile, snapshot_keys::HOME).is_none()
        && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(app);

    // The next start draws who, the libraries, Live TV and Home from what was saved, before asking.
    let state = pley_shell::AppState::new(device, store, http, pley_core::Tasks::new(pley_base::Waker::noop()));
    assert_eq!(state.me().map(|m| m.name.as_str()), Some("Alex"));
    assert!(state.libraries().is_some_and(|l| !l.is_empty()));
    assert!(state.home().is_some_and(|h| !h.is_empty()));
    assert!(state.live_available());
    assert!(!state.home_fresh());
}

#[test]
fn server_events_drive_refreshes_gathered_and_held_while_playing() {
    let server = fixtures();
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let mut app = signed_in_app(&server, &heard);
    app.settle();
    // The stream connecting asks for everything again.
    assert!(app.until(|app| app.state.listening() && changes(&heard) >= 1), "the events never connected");
    app.settle();

    // A library's changes are gathered 1.5 s from the last.
    let before = changes(&heard);
    server.emit(ServerEvent::LibraryChanged { library_id: ids::MOVIES });
    assert!(
        app.until(|_| heard_server_event(&heard, |e| matches!(
            e,
            pley_core::model::ServerEvent::LibraryChanged { .. }
        )))
    );
    assert_eq!(changes(&heard), before);
    let due = app.state.next_due().unwrap();
    assert!((due - app.time - pley_shell::LIBRARY_GATHER).abs() < 0.05);
    app.advance(1.0);
    assert_eq!(changes(&heard), before);
    app.advance(0.6);
    assert_eq!(changes(&heard), before + 1);
    app.settle();

    // Watch-state changes are gathered 0.3 s from the first.
    let before = changes(&heard);
    server.emit(ServerEvent::UserStateChanged { item_ids: vec![11, 10], children: false });
    assert!(app.until(|_| heard_server_event(&heard, |e| matches!(
        e,
        pley_core::model::ServerEvent::UserStateChanged { .. }
    ))));
    app.advance(0.35);
    assert_eq!(changes(&heard), before + 1);
    app.settle();

    // While a playback is on top, nothing asks again until it ends.
    app.navigate(pley_shell::NavCommand::Push(Route::Player(pley_shell::PlayRequest::item(1))));
    let before = changes(&heard);
    server.emit(ServerEvent::WatchlistChanged);
    assert!(
        app.until(|_| heard_server_event(&heard, |e| matches!(e, pley_core::model::ServerEvent::WatchlistChanged)))
    );
    assert_eq!(changes(&heard), before);
    app.navigate(pley_shell::NavCommand::Pop);
    assert!(changes(&heard) > before);
}

#[test]
fn the_server_out_of_reach_shows_what_was_saved_and_comes_back() {
    let server = fixtures();
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let mut app = signed_in_app(&server, &heard);
    app.settle();
    assert!(!app.state.offline());

    server.set_mode(Mode::Down).unwrap();
    app.state.load_home();
    assert!(app.until(|app| app.state.offline()), "the server never went");
    assert!(app.state.offline_notice());
    app.advance(0.2);
    assert!(app.list.says(pley_shell::offline_text()));
    // What was saved stays.
    assert!(app.state.home().is_some());

    // The notice shows 6 s; the avatar's mark stays while the server is away.
    app.advance(OFFLINE_NOTICE - 1.2);
    assert!(app.state.offline_notice());
    app.advance(1.2);
    assert!(!app.state.offline_notice());
    assert!(app.state.offline());

    // The server is asked every 5 s; once it answers, the state clears and everything is asked again.
    server.set_mode(Mode::Normal).unwrap();
    let before = changes(&heard);
    app.advance(OFFLINE_ASK);
    assert!(app.until(|app| !app.state.offline()), "the server never came back");
    assert!(app.until(|_| changes(&heard) > before));
}

#[test]
fn signing_out_forgets_the_sign_in_and_what_was_saved() {
    let server = fixtures();
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let mut app = signed_in_app(&server, &heard);
    app.settle();
    assert!(app.state.signed_in());
    app.state.sign_out();
    assert!(!app.state.signed_in());
    assert!(app.state.me().is_none() && app.state.home().is_none());
    let url = server.url().to_string();
    assert_eq!(app.state.start_route(), Route::Link { server: url });
}

#[test]
fn a_screen_s_answer_shows_saved_first_then_fresh() {
    let server = fixtures();
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let mut app = signed_in_app(&server, &heard);
    app.settle();
    let key = snapshot_keys::item(ids::MOVIES);
    let mut res = Resource::new(Some(key.clone()), |api| api.libraries());
    let mut requests = pley_shell::Requests::default();
    let start = Instant::now();
    loop {
        let mut cx = pley_shell::AppCx::new(&mut app.state, &mut requests, app.time);
        res.update(&mut cx);
        if res.fresh() || start.elapsed() > Duration::from_secs(5) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(res.state(), Load::Ready { fresh: true, .. }));
    // Saved for the next screen: it shows at once, not fresh.
    let profile = app.state.profile().unwrap();
    let snapshots = app.state.snapshots().clone();
    let start = Instant::now();
    while snapshots.read(server.url(), profile, &key).is_none() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(snapshots.read(server.url(), profile, &key).is_some(), "the answer wasn't saved");
    let mut again = Resource::new(Some(key), |api| api.libraries());
    server.set_mode(Mode::Down).unwrap();
    // What was saved is read off the main thread, beside the ask: a server that refuses at once
    // can fail the ask first, and what was saved shows with the failure once read, as on Android.
    let start = Instant::now();
    loop {
        let mut cx = pley_shell::AppCx::new(&mut app.state, &mut requests, app.time);
        again.update(&mut cx);
        if (again.error().is_some() && again.value().is_some()) || start.elapsed() > Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(again.state(), Load::Failed { stale: Some(_), .. }), "{:?}", again.error());
    assert!(app.state.offline());
}

#[test]
fn a_sign_in_the_server_ended_goes_back_to_its_code_screen() {
    let server = fixtures();
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let mut app = signed_in_app(&server, &heard);
    app.settle();
    // The server refuses the token a refresh just gave: the sign-in is over (pley-core's rule).
    let tokens = app.state.tokens().unwrap().clone();
    let stale = tokens.load().unwrap().access_token;
    let fresh = tokens.refresh(Some(&stale)).unwrap();
    assert!(tokens.refused_after_refresh(&fresh));
    let url = server.url().to_string();
    assert!(app.until(|app| app.shell.top_route() == Some(&Route::Link { server: url.clone() })));
    assert!(!app.state.signed_in());
    assert!(!app.state.listening());
    assert!(heard.borrow().iter().any(|(_, e)| *e == AppEvent::SignedOut));
}

/// A registry that counts the pages it makes, by route name.
fn counting(heard: &Heard, made: &Rc<RefCell<Vec<&'static str>>>) -> pley_shell::Registry {
    let inner = registry(heard, &[]);
    let mut r = pley_shell::Registry::new();
    let made = made.clone();
    r.add(move |route, cx| {
        made.borrow_mut().push(route.name());
        Some(inner.make(route, cx))
    });
    r
}

#[test]
fn the_accounts_language_shows_once_it_is_heard_and_the_pages_are_made_again() {
    use pley_i18n::Language;
    let server = fixtures();
    server.set_account_language(ids::ALEX, Some("fr"));
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let made = Rc::new(RefCell::new(Vec::new()));
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, ids::ALEX, &store, &http);
    let mut app = TestApp::new(device.clone(), store.clone(), http.clone(), counting(&heard, &made));
    // Until the account is heard, the language shown last (none yet: the TV's, English).
    assert_eq!(app.state.language(), Language::En);
    let pages = made.borrow().len();
    assert!(app.until(|app| app.state.language() == Language::Fr), "the account's language never showed");
    assert_eq!(pley_i18n::language(), Language::Fr);
    assert_eq!(store.shown_language(), Some(Language::Fr));
    app.settle();
    assert!(made.borrow().len() > pages, "Home wasn't made again");

    // The next start shows French at once, before the account answers.
    server.set_account_language(ids::ALEX, None);
    drop(app);
    let state = pley_shell::AppState::new(device, store, http, pley_core::Tasks::new(pley_base::Waker::noop()));
    assert_eq!(state.language(), Language::Fr);
    // Automatic for the account: this TV's English.
    assert_eq!(state.account_automatic_language(), Language::En);
}

#[test]
fn signed_out_the_globe_s_choice_comes_before_the_tv_and_the_server() {
    use pley_i18n::Language;
    let server = fixtures();
    server.set_server_language("es");
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let device = device();
    device.set_languages(&["de-DE"]);
    let (store, http) = (store(&device), http());
    store.set_active_server(Some(server.url())).unwrap();
    let mut app = TestApp::new(device.clone(), store.clone(), http, registry(&heard, &[]));
    // A TV in a language Pley hasn't: the server's, once it has said.
    assert!(app.until(|app| app.state.language() == Language::Es), "the server's language never showed");
    assert_eq!(app.state.automatic_language(), Language::Es);
    // Picked with the globe: at once, and kept on the TV.
    app.state.choose_language_here(Some(Language::Fr));
    assert_eq!(app.state.language(), Language::Fr);
    assert_eq!(store.device_language(), Some(Language::Fr));
    assert_eq!(app.state.account_automatic_language(), Language::Fr);
    // Automatic forgets it.
    app.state.choose_language_here(None);
    assert_eq!(app.state.language(), Language::Es);
    assert_eq!(store.device_language(), None);
}
