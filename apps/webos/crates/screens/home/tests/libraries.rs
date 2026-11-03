//! The libraries list against the fake server: its cards, opening a library, sorting them, and
//! Playlists' card while the rail has no room for it.

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
fn playlists_follows_the_libraries_while_the_rail_has_no_room_for_it() {
    let mut app = TestApp::start();
    let rail_has = |a: &TestApp, id: &str| a.shell.rail_entries().has(id);
    // Alex's rail has Discover and Live TV, its libraries under Libraries, and Playlists.
    assert!(app.until(|a| rail_has(a, "discover") && rail_has(a, "live-tv")), "no Discover or Live TV");
    assert!(app.until(|a| rail_has(a, "libraries")), "no Libraries");
    // The rail's Libraries: no card for Playlists while the rail has it.
    app.shell.navigate(NavCommand::Reset(Route::Libraries { sort: false }), &mut app.state, &mut app.ui);
    assert!(app.until(|a| a.shows("Music") && a.focused() == Some(card(a, ids::MOVIES))), "no cards");
    assert!(rail_has(&app, "playlists"));
    assert!(!app.shows("Playlists"));
    let playlists = LibrariesScreen::playlists_id(app.page());

    // Now Playing and a drive as well: no library is left to make way, and Playlists leaves the
    // rail for this page, after the libraries.
    app.state.music = Some(Box::new(Queued));
    app.device.set_drive(Some(Drive { root: std::env::temp_dir(), hidden: false }));
    assert!(app.until(|a| !rail_has(a, "playlists")), "Playlists kept its place in the rail");
    assert!(rail_has(&app, "now-playing") && rail_has(&app, "storage"));
    assert!(app.until(|a| a.shows("Playlists")), "no card for Playlists");
    assert_eq!(app.shell.rail_entries().top.len(), pley_shell::MAX_ITEMS);
    // Audio Guidance reads its name, as it reads a library's.
    app.ui.set_speaking(true);
    app.advance(0.1);
    app.ui.take_speech();
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(playlists));
    assert_eq!(app.ui.take_speech().as_deref(), Some("Playlists"));

    // OK opens Playlists, under the rail's Libraries; Back comes back to its card.
    app.press(Key::Ok);
    assert_eq!(app.shell.routes(), [&Route::Libraries { sort: false }, &Route::Playlists]);
    let current = || current_entry(app.shell.routes().first().copied(), app.shell.rail_entries());
    assert_eq!(current().as_deref(), Some("libraries"));
    app.press(Key::Back);
    assert_eq!(app.route(), Some(&Route::Libraries { sort: false }));
    assert_eq!(app.focused(), Some(playlists));

    // While sorting the libraries, it stays out of the way.
    app.press(Key::Up);
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["header", "sort"])));
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Sort libraries") && a.shows("Done")), "sorting didn't start");
    assert!(!app.shows("Playlists"));
    app.press(Key::Back);
    assert!(app.until(|a| a.shows("Playlists")), "the card didn't come back after sorting");

    // The drive pulled out: Playlists is back in the rail, and its card goes.
    app.device.set_drive(None);
    assert!(app.until(|a| rail_has(a, "playlists")), "Playlists didn't come back to the rail");
    assert!(!app.shows("Playlists"));
}
