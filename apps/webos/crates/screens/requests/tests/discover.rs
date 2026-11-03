//! Discover and its lists against the fake server: the hero and its main action, the rows, the way
//! to the requests, See all's grid, and requests turned off.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::{DiscoverList, TitleKind};
use pley_fixtures::{ids, request_titles as titles};
use pley_requests::{DiscoverScreen, TitlesScreen};
use pley_shell::Route;

/// The app on Discover, its hero's main button focused.
fn discover() -> TestApp {
    let mut app = TestApp::start();
    app.open(Route::Discover);
    assert!(app.until_shows("Silent Harbour"), "no hero");
    assert!(app.until(|a| a.focused() == Some(DiscoverScreen::main_id(a.page()))), "the hero's button hasn't focus");
    app
}

#[test]
fn discover_opens_on_the_first_trending_title_and_its_rows() {
    let mut app = discover();
    assert!(app.until(|a| a.shows("Trending this week") && a.shows("Popular movies")), "no rows");
    assert!(app.shows("Request"));
    // Alex manages: the way to the requests says Requests, with what waits (one pending, one failed).
    assert!(app.shows("Requests") && app.shows("2"));
}

#[test]
fn request_on_the_hero_asks_and_its_button_follows() {
    let mut app = discover();
    app.press(Key::Ok);
    assert!(app.until(|a| !a.writes("POST", "/api/requests").is_empty()), "nothing asked");
    let body = &app.writes("POST", "/api/requests")[0].body;
    assert!(body.contains(&format!("\"tmdb_id\":{}", titles::TRENDING_FIRST)) && body.contains("\"movie\""), "{body}");
    assert!(app.until(|a| a.shows("“Silent Harbour” is approved and on its way.")), "no toast");
    assert!(app.until(|a| a.shows("Requested") && a.shows("Searching")), "the hero didn't follow the request");
}

#[test]
fn ok_on_a_poster_opens_its_title_and_up_reaches_the_requests() {
    let mut app = discover();
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(DiscoverScreen::requests_id(app.page())));
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Requests));

    let mut app = discover();
    app.press(Key::Down);
    app.press(Key::Ok);
    assert_eq!(
        app.route(),
        Some(&Route::Title { kind: TitleKind::Movie, tmdb_id: titles::TRENDING_FIRST, pick: false }),
        "the first poster of Trending"
    );
}

#[test]
fn back_in_the_rows_brings_the_hero_back() {
    let mut app = discover();
    app.press(Key::Down);
    app.press(Key::Down);
    assert_ne!(app.focused(), Some(DiscoverScreen::main_id(app.page())));
    app.press(Key::Back);
    assert!(app.until(|a| a.focused() == Some(DiscoverScreen::main_id(a.page()))), "focus didn't go back to the hero");
    assert_eq!(app.route(), Some(&Route::Discover));
}

#[test]
fn see_all_lists_the_whole_row_and_reads_on() {
    let mut app = TestApp::start();
    app.open(Route::DiscoverList { list: DiscoverList::Movies });
    assert!(app.until_shows("Popular movies"), "no title");
    assert!(app.until(|a| a.shows("Northern Lights")), "no posters");
    // Its badges: in the library, asked for.
    assert!(app.shows("In library") && app.shows("Requested"));
    assert!(app.until(|a| a.focused() == Some(TitlesScreen::cell_id(a.page(), 0))));
    // Down through the grid reads the next pages: two hold 40 titles, the eighth row needs a third.
    let past_two_pages = |a: &TestApp| a.focused() == Some(TitlesScreen::cell_id(a.page(), 42));
    assert!(
        app.until(|a| {
            a.press(Key::Down);
            past_two_pages(a)
        }),
        "the grid didn't read on"
    );
}

#[test]
fn a_search_with_nothing_on_tmdb_says_so() {
    let mut app = TestApp::start();
    app.open(Route::DiscoverSearch { query: "  zzz  ".into() });
    assert!(app.until_shows("Nothing on TMDB matches “zzz”"), "no empty search");
    assert!(app.shows("Try a different spelling, or the title in its own language."));
    assert!(app.shows("Results for “zzz”"));
}

#[test]
fn requests_turned_off_say_so() {
    let mut app = TestApp::start_as(ids::ALEX, |s| s.set_requests(false));
    app.open(Route::Discover);
    assert!(app.until_shows("Requests are off"), "no notice");
    assert!(app.shows("This server doesn't take requests for movies and shows."));
}

#[test]
fn an_account_that_may_not_ask_reaches_its_requests() {
    let mut app = TestApp::start_as(ids::ROBIN, |_| {});
    app.open(Route::Discover);
    assert!(app.until_shows("Requests aren't open to this account"), "no notice");
    // Robin follows a request: the page offers the way to it.
    assert!(app.until(|a| a.shows("Your requests")));
}
