//! Watch history against the fake server: the plays and their count, opening one, removing one
//! from its cross or with a long OK (focus going to its neighbour), Show more, and the empty page.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::HistoryPage;
use pley_core::store::snapshot_keys;
use pley_fixtures::ids;
use pley_settings::HistoryScreen;
use pley_settings::history::{PAGE, plays};
use pley_shell::Route;

/// The ids of the profile's plays, newest first, and how many there are, as the server has them.
fn server_history(app: &TestApp) -> (Vec<i64>, i64) {
    let page = app.state.api().unwrap().history(0, 100).unwrap();
    (page.entries.iter().map(|e| e.id).collect(), page.total)
}

/// The first plays as saved for the next visit.
fn saved(app: &TestApp) -> Option<HistoryPage> {
    let (server, profile) = (app.state.server()?, app.state.profile()?);
    app.state.snapshots().get(server, profile, snapshot_keys::HISTORY_HEAD)
}

fn history() -> TestApp {
    let mut app = TestApp::open_on(Route::Settings { section: None });
    // The event stream connecting asks the pages for everything again: History, open by then,
    // would ask for its plays anew under the steps that follow.
    assert!(app.until(|a| a.state.connected()), "the events never connected");
    app.settle();
    app.open(Route::History);
    assert!(app.until(|a| a.shows("Watch history") && a.focused().is_some_and(|f| f != a.page())));
    app.settle();
    app
}

#[test]
fn shows_the_plays_newest_first_with_their_count() {
    let mut app = history();
    let (entries, total) = server_history(&app);
    assert!(total > PAGE, "the fixture has more than a page of plays");
    assert!(app.until(|a| a.shows(&plays(total))));
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), entries[0], false)));
    // An episode shows its show, and what it is, when and where under it.
    assert!(app.shows("Fixture Show"));
    assert!(app.shows_start("S1 · E1 · "));
}

#[test]
fn ok_opens_the_play() {
    let mut app = history();
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Item { id: ids::SHOW_S1E1 }));
}

#[test]
fn the_first_plays_show_as_saved_while_the_server_is_away() {
    let mut app = history();
    let (entries, total) = server_history(&app);
    // The server's answer is saved, its first page only.
    assert!(app.until(|a| saved(a).is_some_and(|p| p.entries.len() == PAGE as usize && p.total == total)));
    app.press(Key::Back);
    app.settle();
    app.server.stop();
    app.open(Route::History);
    assert!(app.until(|a| a.shows(&plays(total))));
    // The saved plays, not "Can't reach your server", once asking has failed.
    assert!(app.until(|a| a.state.offline()));
    app.settle();
    assert!(!app.shows("Can't reach your server"));
    assert!(app.shows("Fixture Show"));
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), entries[0], false)));
}

#[test]
fn the_cross_removes_a_play_and_focus_goes_to_the_next_cross() {
    let mut app = history();
    let (entries, total) = server_history(&app);
    assert!(app.until(|a| saved(a).is_some()));
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), entries[0], true)));
    app.press(Key::Ok);
    // Gone at once, focus on the same place in the next row.
    assert!(app.shows(&plays(total - 1)));
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), entries[1], true)));
    assert!(app.until(|a| server_history(a).1 == total - 1));
    assert!(!server_history(&app).0.contains(&entries[0]));
    // The saved plays forget it too, so the next visit doesn't show it first.
    assert!(app.until(|a| saved(a).is_some_and(|p| p.total == total - 1)));
    assert!(!saved(&app).unwrap().entries.iter().any(|e| e.id == entries[0]));
}

#[test]
fn a_long_ok_removes_a_play_too() {
    let mut app = history();
    let (entries, total) = server_history(&app);
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), entries[1], false)));
    app.hold_ok();
    assert_eq!(app.route(), Some(&Route::History));
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), entries[2], false)));
    assert!(app.until(|a| server_history(a).1 == total - 1));
    assert!(!server_history(&app).0.contains(&entries[1]));
}

#[test]
fn show_more_brings_the_next_plays() {
    let mut app = history();
    let (entries, total) = server_history(&app);
    let more = app.page().with("rows").with("more");
    app.press_n(Key::Down, PAGE as usize);
    assert_eq!(app.focused(), Some(more));
    app.press(Key::Ok);
    // The fixture's plays fit in two pages: the button goes once the second comes.
    assert_eq!(total as usize, entries.len());
    assert!(entries.len() <= 2 * PAGE as usize);
    assert!(app.until(|a| !a.shows("Show more") && !a.shows("Loading…")));
    app.settle();
    app.press_n(Key::Down, entries.len());
    assert_eq!(app.focused(), Some(HistoryScreen::row_id(app.page(), *entries.last().unwrap(), false)));
    assert!(app.shows(&plays(total)));
}

#[test]
fn an_empty_history_says_so_with_back() {
    let mut app = TestApp::start(ids::SAM, None, Route::Settings { section: None });
    app.open(Route::History);
    assert!(app.until(|a| a.shows("No history yet")));
    assert!(app.shows("Movies, episodes and songs you finish show up here."));
    app.settle();
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Settings { section: None }));
}
