//! Your requests and Requests against the fake server: the tabs, the cards, the news seen, and what
//! may be done to a request from its panel.

mod common;

use common::TestApp;
use pley_base::{Cmd, Key, Rect};
use pley_core::requests::RequestsTab;
use pley_fixtures::ids;
use pley_requests::RequestsScreen;
use pley_shell::Route;
use pley_ui::theme;

/// The app on the requests, its first card focused once `card` shows.
fn requests(profile: i64, card: &str) -> TestApp {
    let mut app = TestApp::start_as(profile, |_| {});
    app.open(Route::Requests);
    assert!(app.until_shows(card), "no {card}");
    assert!(app.until(|a| a.focused() == Some(RequestsScreen::card_id(a.page(), 0))), "the first card hasn't focus");
    app
}

/// The cards drawn, top to bottom, as they are before focus lifts one.
fn cards(app: &TestApp) -> Vec<Rect> {
    let card_w = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    let mut cards: Vec<Rect> = app
        .list
        .cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Fill { rect, .. } if rect.w == card_w && rect.h == 88.0 => Some(*rect),
            _ => None,
        })
        .collect();
    cards.sort_by(|a, b| a.y.total_cmp(&b.y));
    cards
}

/// The focus ring around a card, to the outside of its line.
fn ring(app: &TestApp) -> Rect {
    let card_w = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    app.list
        .cmds
        .iter()
        .find_map(|c| match c {
            Cmd::Stroke { rect, width, .. } if rect.w > card_w => Some(rect.expand(width / 2.0)),
            _ => None,
        })
        .expect("no ring")
}

#[test]
fn the_first_card_is_as_far_from_the_second_as_the_others_and_its_ring_shows_whole() {
    let mut app = requests(ids::ALEX, "Paper Moons");
    app.advance(0.5);
    let cards = cards(&app);
    assert_eq!(cards.len(), 2);
    assert_eq!(cards[1].y - cards[0].bottom(), 10.0, "the cards are 10 apart");
    assert!(ring(&app).bottom() < cards[1].y, "the second card covers the first's ring");
}

#[test]
fn the_last_card_scrolls_up_clear_of_the_screens_foot() {
    let mut app = requests(ids::ALEX, "Quiet Fields");
    // The All tab, past the filters: six requests, more than the screen holds.
    app.press(Key::Up);
    app.press(Key::Up);
    app.press(Key::Right);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Silver Line")), "the tab didn't change");
    let last = |a: &TestApp| a.focused() == Some(RequestsScreen::card_id(a.page(), 5));
    for _ in 0..10 {
        if last(&app) {
            break;
        }
        app.press(Key::Down);
    }
    assert!(last(&app), "the last card hasn't focus");
    app.advance(0.5);
    let card = *cards(&app).last().unwrap();
    assert!(card.bottom() <= theme::CANVAS_H - theme::SAFE_Y, "the last card ends at {}", card.bottom());
    assert!(ring(&app).bottom() <= theme::CANVAS_H, "its ring is cut at {}", ring(&app).bottom());
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
fn an_empty_tab_says_why_as_androids_does() {
    let mut app = requests(ids::SAM, "Paper Moons");
    // The Available tab: none of Sam's is ready yet.
    app.press(Key::Up);
    for _ in 0..3 {
        app.press(Key::Right);
    }
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("No requests match") && !a.shows("Paper Moons")), "the tab isn't empty");
    assert!(app.shows("Try other filters."));
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
