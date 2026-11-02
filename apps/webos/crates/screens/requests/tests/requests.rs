//! Your requests and Requests against the fake server: the tabs, the cards, the news seen, and what
//! may be done to a request from its panel.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::requests::RequestsTab;
use pley_fixtures::ids;
use pley_requests::RequestsScreen;
use pley_shell::Route;

/// The app on the requests, its first card focused once `card` shows.
fn requests(profile: i64, card: &str) -> TestApp {
    let mut app = TestApp::start_as(profile, |_| {});
    app.open(Route::Requests);
    assert!(app.until_shows(card), "no {card}");
    assert!(app.until(|a| a.focused() == Some(RequestsScreen::card_id(a.page(), 0))), "the first card hasn't focus");
    app
}

#[test]
fn managers_open_on_needs_attention_oldest_first() {
    let app = requests(ids::ALEX, "Quiet Fields");
    for text in [
        "Requests",
        "Needs attention",
        "All",
        "Pending",
        "Failed",
        "Paper Moons",
        "Couldn't send",
        "Waiting for approval",
    ] {
        assert!(app.shows(text), "no {text}");
    }
    assert!(app.shows("Movies and shows") && app.shows("Everyone"), "the filters");
    assert!(app.writes("POST", "/api/requests/seen").is_empty(), "Alex had no news");
}

#[test]
fn your_requests_tells_the_server_the_news_was_seen() {
    let mut app = requests(ids::SAM, "Paper Moons");
    assert!(app.shows("Your requests") && app.shows("Long Night") && app.shows("Season 1 ready"));
    assert!(!app.shows("Needs attention"));
    assert!(app.until(|a| !a.writes("POST", "/api/requests/seen").is_empty()), "the news wasn't seen");
    // The Declined tab.
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(RequestsScreen::tab_id(app.page(), RequestsTab::All)));
    for _ in 0..4 {
        app.press(Key::Right);
    }
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Glass River") && !a.shows("Paper Moons")), "the tab didn't change");
}

#[test]
fn try_again_sends_a_failed_request_and_it_leaves_needs_attention() {
    let mut app = requests(ids::ALEX, "Quiet Fields");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Quiet Fields (2023)") && a.shows("Try again")), "no panel");
    assert_eq!(app.focused(), Some(RequestsScreen::panel_row_id(app.page(), "Action(Retry)")));
    app.press(Key::Ok);
    assert!(app.until(|a| !a.writes("POST", "/api/requests/4/approve").is_empty()), "nothing sent");
    assert!(app.until(|a| a.shows("Trying “Quiet Fields” again.")), "no toast");
    assert!(app.until(|a| !a.shows("Quiet Fields")), "it stayed in Needs attention");
}

#[test]
fn decline_with_a_reason_typed_on_the_keyboard() {
    let mut app = requests(ids::ALEX, "Paper Moons");
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Decline with a reason…")), "no panel");
    // Approve, Decline, then Decline with a reason….
    app.press(Key::Down);
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Decline “Paper Moons”?")), "no question");
    assert!(app.shows("Sam sees that it was declined, and your reason if you give one."));
    app.press(Key::Ok);
    app.type_text("Out of print");
    // Done, at the keyboard's bottom right: focus goes to Decline.
    for key in [Key::Down; 6].into_iter().chain([Key::Right; 12]) {
        app.press(key);
    }
    app.press(Key::Ok);
    assert!(app.until(|a| a.focused() == Some(RequestsScreen::panel_row_id(a.page(), "decline"))), "not on Decline");
    app.press(Key::Ok);
    assert!(app.until(|a| !a.writes("POST", "/api/requests/1/decline").is_empty()), "nothing declined");
    assert!(app.writes("POST", "/api/requests/1/decline")[0].body.contains("Out of print"));
    assert!(app.until(|a| a.shows("Declined “Paper Moons”.")), "no toast");
}

#[test]
fn withdrawing_asks_first() {
    let mut app = requests(ids::SAM, "Paper Moons");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Withdraw")), "no panel");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Withdraw your request for “Paper Moons”?")), "no question");
    assert_eq!(app.focused(), Some(RequestsScreen::panel_row_id(app.page(), "cancel")), "Cancel first");
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.until(|a| !a.writes("DELETE", "/api/requests/1").is_empty()), "not withdrawn");
    assert!(app.until(|a| a.shows("Withdrew your request for “Paper Moons”.")), "no toast");
    assert!(app.until(|a| !a.shows("Paper Moons")), "the card stayed");
}

#[test]
fn back_from_a_question_goes_back_to_the_actions() {
    let mut app = requests(ids::SAM, "Paper Moons");
    app.press(Key::Ok);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Withdraw your request for “Paper Moons”?")), "no question");
    app.press(Key::Back);
    assert!(app.until(|a| a.shows("Details") && !a.shows("Cancel")), "not back on the actions");
    app.press(Key::Back);
    assert!(app.until(|a| !a.shows("Details")), "the panel stayed");
    assert_eq!(app.route(), Some(&Route::Requests));
}
