//! A title's page against the fake server: its main action as each title's state has it, the
//! season picker, asking, and a title TMDB doesn't know.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::TitleKind;
use pley_fixtures::{ids, request_titles as titles};
use pley_requests::TitleScreen;
use pley_shell::Route;

fn title(kind: TitleKind, tmdb_id: i64, pick: bool) -> Route {
    Route::Title { kind, tmdb_id, pick }
}

#[test]
fn a_show_partly_in_the_library_offers_open_and_more_seasons() {
    let mut app = TestApp::start();
    app.open(title(TitleKind::Show, titles::SHOW_PARTIAL, false));
    assert!(app.until_shows("Fixture Show"), "no title");
    assert!(app.until(|a| a.shows("Open") && a.shows("Request more seasons")), "no main action");
    assert!(app.shows("Seasons") && app.shows("Cast") && app.shows("In library"));
    assert_eq!(app.focused(), Some(TitleScreen::main_id(app.page())), "Open first");

    // Request more seasons: the picker, the missing season ticked.
    app.press(Key::Right);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Which seasons?") && a.shows("Request 1 season")), "no picker");
    assert_eq!(app.focused(), Some(TitleScreen::pick_row_id(app.page(), 2)), "on season 3");
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(TitleScreen::pick_ask_id(app.page())));
    app.press(Key::Ok);
    assert!(app.until(|a| !a.writes("POST", "/api/requests").is_empty()), "nothing asked");
    let body = &app.writes("POST", "/api/requests")[0].body;
    assert!(body.contains("\"seasons\":[3]"), "{body}");
    assert!(app.until(|a| a.shows("“Fixture Show” is approved and on its way.")), "no toast");
    assert!(app.until(|a| !a.shows("Which seasons?")), "the picker stayed");
}

#[test]
fn opened_to_pick_the_picker_comes_with_the_page_and_unticks() {
    let mut app = TestApp::start_as(ids::SAM, |_| {});
    app.open(title(TitleKind::Show, titles::SHOW_NEW, true));
    assert!(app.until_shows("Which seasons?"), "no picker");
    assert!(app.shows("Request 1 season") && app.shows("Airs Mar 4, 2027"));
    // OK on the ticked season unticks it: nothing to ask for.
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Pick a season.")), "still ticked");
    // Back closes the picker, not the page.
    app.press(Key::Back);
    assert!(app.until(|a| !a.shows("Which seasons?")));
    assert!(matches!(app.route(), Some(Route::Title { .. })));
}

#[test]
fn a_request_in_progress_shows_its_stage() {
    let mut app = TestApp::start_as(ids::SAM, |_| {});
    app.open(title(TitleKind::Movie, titles::PENDING, false));
    assert!(app.until_shows("Paper Moons"), "no title");
    assert!(app.until(|a| a.shows("Requested") && a.shows("Waiting for approval")), "no stage");

    let mut app = TestApp::start();
    app.open(title(TitleKind::Movie, titles::DOWNLOADING, false));
    assert!(app.until(|a| a.shows("Requested") && a.list.texts().any(|t| t.starts_with("Downloading 42%"))));
}

#[test]
fn a_declined_title_says_when_it_may_be_asked_for_again() {
    let mut app = TestApp::start_as(ids::SAM, |_| {});
    app.open(title(TitleKind::Movie, titles::DECLINED, false));
    assert!(app.until_shows("Glass River"), "no title");
    assert!(
        app.until(|a| a.shows_start("Declined: “Not on any service”. You can ask again in 27")),
        "{:?}",
        app.list.texts().collect::<Vec<_>>()
    );
    assert!(!app.shows("Request"));
}

#[test]
fn someone_elses_request_is_followed_with_tell_me_too() {
    let mut app = TestApp::start_as(ids::SAM, |_| {});
    app.open(title(TitleKind::Movie, titles::DOWNLOADING, false));
    assert!(
        app.until(|a| a.shows("Tell me too") && a.shows_start("Someone asked for it")),
        "{:?}",
        app.list.texts().collect::<Vec<_>>()
    );
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("You'll hear when “Iron Harbour” is ready.")), "no toast");
    assert!(app.until(|a| a.shows("Following")), "not following");
}

#[test]
fn a_title_tmdb_doesnt_know_says_so() {
    let mut app = TestApp::start();
    app.open(title(TitleKind::Movie, 1, false));
    assert!(app.until_shows("Not on TMDB"), "no notice");
    app.press(Key::Ok);
    assert!(app.until(|a| !matches!(a.route(), Some(Route::Title { .. }))), "Back didn't go back");
}
