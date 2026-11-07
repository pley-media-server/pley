//! The libraries list against the fake server: its cards, opening a library and sorting them.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::device::Drive;
use pley_fixtures::{Mode, ids};
use pley_home::LibrariesScreen;
use pley_shell::{BackgroundSession, NavCommand, Route, current_entry};

/// The app on the libraries list, its cards in.
fn libraries(sort: bool) -> TestApp {
    let mut app = TestApp::start();
    assert!(app.until(|a| a.state.libraries().is_some()), "no libraries");
    app.open(Route::Libraries { sort });
    assert!(app.until(|a| a.shows("Movies") && a.shows("TV Shows") && a.shows("Music")), "no cards");
    app
}

fn card(app: &TestApp, id: i64) -> pley_ui::Id {
    LibrariesScreen::card_id(app.page(), id)
}

fn order(app: &TestApp) -> Vec<i64> {
    app.state.libraries().unwrap().iter().map(|l| l.id).collect()
}

#[test]
fn the_cards_say_what_each_library_holds_and_open_it() {
    let mut app = libraries(false);
    assert!(app.shows("Libraries"));
    assert!(app.shows("1,500 movies") && app.shows("300 shows") && app.shows("50 artists"));
    assert!(app.until(|a| a.focused() == Some(card(a, ids::MOVIES))), "the first card hasn't focus");
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(card(&app, ids::SHOWS)));
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Library { id: ids::SHOWS, tab: None, sort: None }));
}

#[test]
fn a_lifted_card_moves_with_the_arrows_and_its_order_is_saved() {
    let mut app = libraries(false);
    assert!(app.until(|a| a.focused() == Some(card(a, ids::MOVIES))));
    // Up to the header's Sort libraries, which starts sorting with focus on the first card.
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["header", "sort"])));
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Sort libraries") && a.shows("Done")), "sorting didn't start");
    assert_eq!(app.focused(), Some(card(&app, ids::MOVIES)));
    assert!(app.shows("Select a library to move it. Your order applies on all your devices."));
    // OK lifts it, Right moves it a place along, OK drops it there and sends the order.
    app.press(Key::Ok);
    assert!(app.shows("Arrows to move it, OK to drop it here, Back to put it back"));
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(card(&app, ids::MOVIES)), "the lifted card lost focus");
    app.press(Key::Ok);
    assert!(app.until(|a| order(a) == [ids::SHOWS, ids::MOVIES, ids::MUSIC]), "the order wasn't saved");
    let sent = app.server.writes("PUT", "/api/me/libraries/order");
    assert_eq!(sent.len(), 1);
    // Use the server's order forgets the profile's own.
    app.press(Key::Up);
    if app.focused() != Some(app.id(&["header", "server-order"])) {
        app.press(Key::Left);
    }
    assert_eq!(app.focused(), Some(app.id(&["header", "server-order"])));
    app.press(Key::Ok);
    assert!(app.until(|a| order(a) == [ids::MOVIES, ids::SHOWS, ids::MUSIC]), "the server's order didn't come back");
    assert_eq!(app.server.writes("DELETE", "/api/me/libraries/order").len(), 1);
}

#[test]
fn the_servers_order_stays_over_libraries_asked_before_it() {
    let mut app = TestApp::start_with_threads(16);
    assert!(app.until(|a| a.state.libraries().is_some()), "no libraries");
    app.open(Route::Libraries { sort: true });
    assert!(app.until(|a| a.focused() == Some(card(a, ids::MOVIES))), "no cards");
    // The profile's own order, saved.
    app.press(Key::Ok);
    app.press(Key::Right);
    app.press(Key::Ok);
    let custom = [ids::SHOWS, ids::MOVIES, ids::MUSIC];
    assert!(app.until(|a| order(a) == custom && a.server.writes("PUT", "/api/me/libraries/order").len() == 1));
    app.advance(0.5);
    // The libraries are asked again (another device changed something), and that answer takes 1.5 s.
    app.server.set_mode(Mode::Slow).unwrap();
    app.state.reload_account();
    let start = std::time::Instant::now();
    app.until(|_| start.elapsed() > std::time::Duration::from_millis(200));
    // Use the server's order goes at once, and the server takes it before that answer comes.
    app.server.set_mode(Mode::Normal).unwrap();
    app.press(Key::Up);
    if app.focused() != Some(app.id(&["header", "server-order"])) {
        app.press(Key::Left);
    }
    assert_eq!(app.focused(), Some(app.id(&["header", "server-order"])));
    app.press(Key::Ok);
    let server = [ids::MOVIES, ids::SHOWS, ids::MUSIC];
    assert!(app.until(|a| order(a) == server), "the server's order didn't come");
    // The answer asked before it comes, then the one asked after it: the server's order stays.
    let start = std::time::Instant::now();
    app.until(|a| {
        assert_eq!(order(a), server, "libraries asked before the server's order took it back");
        start.elapsed() > std::time::Duration::from_millis(2_500)
    });
    assert_eq!(app.server.writes("DELETE", "/api/me/libraries/order").len(), 1);
}

#[test]
fn an_order_saved_elsewhere_while_the_libraries_are_on_their_way_shows() {
    let mut app = TestApp::start_with_threads(16);
    assert!(app.until(|a| a.state.libraries().is_some()), "no libraries");
    app.advance(0.5);
    // The libraries are asked again, and that answer, from before what follows, takes 1.5 s.
    app.server.set_mode(Mode::Slow).unwrap();
    app.state.reload_account();
    let start = std::time::Instant::now();
    app.until(|_| start.elapsed() > std::time::Duration::from_millis(200));
    // Another of the profile's devices saves its own order meanwhile, and the server says so.
    app.server.set_mode(Mode::Normal).unwrap();
    let api = app.state.api().unwrap().clone();
    let custom = [ids::MUSIC, ids::MOVIES, ids::SHOWS];
    api.sort_libraries(&custom).unwrap();
    // The answer from before lands, then they are asked again: the new order shows.
    assert!(app.until(|a| order(a) == custom), "the libraries are in the order {:?}", order(&app));
}

#[test]
fn back_puts_a_lifted_card_back_then_ends_sorting() {
    let mut app = libraries(true);
    assert!(app.until(|a| a.shows("Sort libraries") && a.focused() == Some(card(a, ids::MOVIES))));
    app.press(Key::Ok);
    app.press(Key::Down);
    app.press(Key::Back);
    assert!(app.shows("Select a library to move it. Your order applies on all your devices."));
    app.advance(0.3);
    assert!(app.server.writes("PUT", "/api/me/libraries/order").is_empty(), "a card put back was saved");
    assert_eq!(order(&app), [ids::MOVIES, ids::SHOWS, ids::MUSIC]);
    // Opened by Settings to sort: Back goes back there.
    app.press(Key::Back);
    assert_ne!(app.route(), Some(&Route::Libraries { sort: true }));
}

#[test]
fn a_cards_actions_open_in_a_panel() {
    let mut app = libraries(false);
    assert!(app.until(|a| a.focused() == Some(card(a, ids::MOVIES))));
    app.hold_ok();
    assert!(app.until(|a| a.shows("Open") && a.shows("Move") && a.shows("Use the server's order")), "no panel");
    // Back closes the panel and leaves the page as it was.
    app.press(Key::Back);
    app.advance(0.4);
    assert!(!app.shows("Move"));
    assert_eq!(app.route(), Some(&Route::Libraries { sort: false }));
    // Move from the panel lifts the card at once.
    app.hold_ok();
    assert!(app.until(|a| a.shows("Move")));
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Arrows to move it, OK to drop it here, Back to put it back")), "Move didn't lift");
}

#[test]
fn orders_still_on_their_way_when_the_page_closes_reach_the_server() {
    let mut app = libraries(true);
    assert!(app.until(|a| a.focused() == Some(card(a, ids::MOVIES))));
    app.server.set_mode(Mode::Slow).unwrap();
    // Two moves, the second made while the first is on its way, then Back to Settings.
    app.press(Key::Ok);
    app.press(Key::Right);
    app.press(Key::Ok);
    app.press(Key::Ok);
    app.press(Key::Right);
    app.press(Key::Ok);
    app.press(Key::Back);
    assert_ne!(app.route(), Some(&Route::Libraries { sort: true }));
    // Both reach the server in order, and the libraries follow the last.
    let sent = |a: &mut TestApp| a.server.writes("PUT", "/api/me/libraries/order").len() == 2;
    assert!(app.until(sent), "not every order reached the server");
    let last = [ids::SHOWS, ids::MUSIC, ids::MOVIES];
    assert!(app.until(|a| order(a) == last), "the libraries are in the order {:?}", order(&app));
}

#[test]
fn audio_guidance_reads_each_cards_name_and_what_it_holds() {
    let mut app = libraries(false);
    assert!(app.until(|a| a.focused() == Some(card(a, ids::MOVIES))));
    app.ui.set_speaking(true);
    app.advance(0.1);
    let first = app.ui.take_speech().unwrap_or_default();
    assert!(first.contains("Movies, 1,500 movies"), "{first}");
    app.press(Key::Right);
    let next = app.ui.take_speech().unwrap_or_default();
    assert_eq!(next, "TV Shows, 300 shows");
}

/// Music with a queue: the rail shows Now Playing.
struct Queued;

impl BackgroundSession for Queued {
    fn queued(&self) -> bool {
        true
    }
    fn tick(&mut self, _now: f64) -> Option<f64> {
        None
    }
    fn pause(&mut self) {}
    fn end(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn the_rail_keeps_every_entry_and_this_page_lists_libraries_alone() {
    let mut app = TestApp::start();
    let rail_has = |a: &TestApp, id: &str| a.shell.rail_entries().has(id);
    // Alex's rail has Discover and Live TV, each library, Playlists and Collections.
    assert!(app.until(|a| rail_has(a, "discover") && rail_has(a, "live-tv")), "no Discover or Live TV");
    assert!(app.until(|a| rail_has(a, "library:3")), "not every library has an entry");
    // Now Playing and a drive as well: nothing makes way for them, the rail scrolls instead.
    app.state.music = Some(Box::new(Queued));
    app.device.set_drive(Some(Drive { root: std::env::temp_dir(), hidden: false }));
    assert!(app.until(|a| rail_has(a, "now-playing") && rail_has(a, "storage")), "no Now Playing or drive");
    let entries: Vec<&str> = app.shell.rail_entries().top.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        entries,
        [
            "search",
            "home",
            "discover",
            "live-tv",
            "library:1",
            "library:2",
            "library:3",
            "playlists",
            "collections",
            "now-playing",
            "storage"
        ]
    );
    let playlists = current_entry(Some(&Route::Playlists), app.shell.rail_entries());
    assert_eq!(playlists.as_deref(), Some("playlists"));
    // The Libraries page lists the libraries and nothing else.
    app.shell.navigate(NavCommand::Reset(Route::Libraries { sort: false }), &mut app.state, &mut app.ui);
    assert!(app.until(|a| a.shows("Music") && a.focused() == Some(card(a, ids::MOVIES))), "no cards");
    assert!(!app.shows("Playlists"));
}
