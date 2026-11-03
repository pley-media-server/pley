//! Live TV's page against the fake server: where focus goes, the categories, the views, a
//! programme's details and favourites, search, and the profile it isn't open to.

mod common;

use common::TestApp;
use pley_base::{Key, KeyEvent};
use pley_core::Store;
use pley_core::model::LiveCategory;
use pley_fixtures::ids;
use pley_shell::Route;
use pley_ui::InputEvent;

/// The app on Live TV, its guide drawn and focused.
fn live() -> TestApp {
    let mut app = TestApp::start();
    app.open(Route::LiveTv);
    let guide = app.id(&["guide"]);
    assert!(app.until(|a| a.shows("Today") && a.focused() == Some(guide)), "the guide didn't take focus");
    // The guide's programmes are in once the info block says how long what's on has left.
    assert!(app.until(|a| a.list.texts().any(|t| t.contains(" left"))), "no programmes");
    app
}

fn category_row(app: &TestApp, line: u64) -> pley_ui::Id {
    app.id(&["categories"]).with(line)
}

/// Left until focus leaves the guide: what airs now may have begun inside the guide's first hour, and
/// Left goes back through it first.
fn to_categories(app: &mut TestApp) {
    let guide = app.id(&["guide"]);
    for _ in 0..8 {
        app.press(Key::Left);
        if app.focused() != Some(guide) {
            return;
        }
    }
    panic!("Left never left the guide");
}

/// What Live TV kept under the key starting `prefix` (it is the server's and profile's).
fn kept(store: &Store, prefix: &str) -> Option<String> {
    store.keys().into_iter().find(|k| k.starts_with(prefix)).and_then(|k| store.get::<String>(&k))
}

#[test]
fn the_page_opens_on_the_guide_of_every_channel() {
    let app = live();
    for text in ["Live TV", "Guide", "Channels", "Favourites", "Recently watched", "All channels", "Groups"] {
        assert!(app.shows(text) || app.shows(&text.to_uppercase()), "no {text}");
    }
    // The counts come with the categories, with thousands' separators.
    assert!(app.shows("42"), "All channels' count");
}

#[test]
fn left_goes_to_the_category_shown_and_ok_on_a_group_lists_it() {
    let mut app = live();
    to_categories(&mut app);
    // The search, Favourites, Recently watched, then All channels.
    assert_eq!(app.focused(), Some(category_row(&app, 3)));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(category_row(&app, 5)), "the heading is passed over");
    app.press(Key::Ok);
    let guide = app.id(&["guide"]);
    assert!(app.until(|a| a.focused() == Some(guide)), "focus didn't go to the group's channels");
    // UK| News holds channels 1 to 6; Engine Sport 1 (7) is UK| Sport's.
    assert!(
        app.until(|a| !a.list.texts().any(|t| t.starts_with("Engine Sport"))),
        "another group's channel shows: {:?}",
        app.list.texts().collect::<Vec<_>>()
    );
    let store = app.state.store().clone();
    let server = app.server.url().to_string();
    assert!(app.until(|_| kept(&store, "pley.live.category.").as_deref() == Some("group:UK| News")));
    assert!(store.keys().iter().any(|k| k.starts_with("pley.live.category.") && k.contains(&server)));
}

#[test]
fn up_from_the_first_row_reaches_the_tabs_and_channels_shows_cards() {
    let mut app = live();
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "guide"])));
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "channels"])));
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Voyage News")), "no cards");
    assert!(!app.shows("Today"), "the guide's ruler still shows");
    let store = app.state.store().clone();
    assert!(app.until(|_| kept(&store, "pley.live.view.").as_deref() == Some("channels")));
    // Down goes into the cards, to the one under the tab; OK on it watches it.
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(matches!(app.route(), Some(Route::LivePlayer { category: LiveCategory::All, .. })), "{:?}", app.route());
}

/// Holds `key` for 1.5 s, the TV repeating every 50 ms from 400 ms, then lets it go.
fn hold(app: &mut TestApp, key: Key) {
    let event = |pressed, repeat| vec![InputEvent::Key(KeyEvent { key, pressed, repeat, code: 0 })];
    app.frame(event(true, false));
    for i in 0..30 {
        app.time += 0.05;
        app.frame(if i >= 8 { event(true, true) } else { Vec::new() });
    }
    app.time += 0.05;
    app.frame(event(false, false));
    app.advance(0.2);
}

#[test]
fn a_held_up_stops_at_the_top_row_and_a_press_reaches_the_tabs() {
    let mut app = live();
    let guide = app.id(&["guide"]);
    app.press(Key::Down);
    app.press(Key::Down);
    hold(&mut app, Key::Up);
    assert_eq!(app.focused(), Some(guide), "a held Up left the guide");
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "guide"])));
    // The cards too.
    app.press(Key::Right);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Voyage News")), "no cards");
    app.press(Key::Down);
    app.press(Key::Down);
    let in_cards = app.focused();
    assert_ne!(in_cards, Some(app.id(&["tabs", "channels"])));
    hold(&mut app, Key::Up);
    assert_ne!(app.focused(), Some(app.id(&["tabs", "channels"])), "a held Up left the cards");
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "channels"])));
}

#[test]
fn ok_on_whats_on_now_watches_the_channel() {
    let mut app = live();
    app.press(Key::Ok);
    assert!(
        matches!(
            app.route(),
            Some(Route::LivePlayer { channel_id: ids::LIVE_CHANNEL, category: LiveCategory::All, .. })
        ),
        "{:?}",
        app.route()
    );
    // The player starts with the channel the page showed.
    assert!(app.state.known_live_channel(ids::LIVE_CHANNEL).is_some());
}

#[test]
fn menu_opens_the_details_and_the_favourite_changes_at_once() {
    let mut app = live();
    app.press(Key::Menu);
    let watch = app.id(&["details", "content", "watch"]);
    assert!(app.until(|a| a.focused() == Some(watch)), "the details' Watch now hasn't focus");
    assert!(app.shows("Watch now") && app.shows("ON NOW"));
    // Channel 1 is one of Alex's favourites.
    assert!(app.shows("Remove from favourites"));
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.shows("Add to favourites"), "the button didn't change at once");
    assert!(app.until(|a| !a.server.writes("DELETE", "/api/live/channels/1/favorite").is_empty()));
    // Back closes the panel, focus back in the guide.
    app.press(Key::Back);
    app.advance(0.4);
    assert_eq!(app.route(), Some(&Route::LiveTv));
    assert_eq!(app.focused(), Some(app.id(&["guide"])));
}

#[test]
fn back_away_from_now_comes_back_to_now_before_leaving() {
    let mut app = live();
    // The info block says how long what's on has left; a programme later doesn't.
    let on_now = |a: &TestApp| a.list.texts().any(|t| t.contains(" left"));
    assert!(on_now(&app));
    app.press(Key::Right);
    app.press(Key::Right);
    assert!(!on_now(&app), "still on what's on now");
    app.press(Key::Back);
    assert_eq!(app.route(), Some(&Route::LiveTv), "Back left the page instead of coming back to now");
    app.advance(0.4);
    assert_eq!(app.focused(), Some(app.id(&["guide"])));
    assert!(on_now(&app), "not back at now: {:?}", app.list.texts().collect::<Vec<_>>());
    // At now, Back goes to the category shown, and only from there does it leave.
    app.press(Key::Back);
    assert_eq!(app.route(), Some(&Route::LiveTv), "a second Back left the page");
    assert_eq!(app.focused(), Some(category_row(&app, 3)), "a second Back isn't on All channels");
    app.press(Key::Back);
    assert_ne!(app.focused(), Some(category_row(&app, 3)), "a third Back stayed on the categories");
}

#[test]
fn rewind_and_fast_forward_previous_and_next_move_the_guide_two_hours() {
    let mut app = live();
    let on_now = |a: &TestApp| a.list.texts().any(|t| t.contains(" left"));
    assert!(on_now(&app));
    for (on, back) in [(Key::FastForward, Key::Rewind), (Key::Next, Key::Previous)] {
        app.press(on);
        assert!(!on_now(&app), "{on:?} didn't move the guide on");
        assert_eq!(app.route(), Some(&Route::LiveTv));
        app.press(back);
        assert!(on_now(&app), "{back:?} didn't come back to now");
    }
}

#[test]
fn a_held_left_stops_at_the_guides_start() {
    let mut app = live();
    let guide = app.id(&["guide"]);
    app.press(Key::Right);
    app.press(Key::Right);
    // Left held: the guide goes back to its start and stays there however long it repeats.
    app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Left, pressed: true, repeat: false, code: 0 })]);
    for _ in 0..40 {
        app.time += 0.05;
        app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Left, pressed: true, repeat: true, code: 0 })]);
    }
    app.time += 0.05;
    app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Left, pressed: false, repeat: false, code: 0 })]);
    assert_eq!(app.focused(), Some(guide), "a held Left ran out of the guide");
    // A press of its own, a moment later, goes to the categories.
    app.advance(0.6);
    app.press(Key::Left);
    assert_eq!(app.focused(), Some(category_row(&app, 3)));
}

#[test]
fn info_on_a_card_and_back_from_the_cards_go_where_the_guide_does() {
    let mut app = live();
    app.press(Key::Up);
    app.press(Key::Right);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Voyage News")), "no cards");
    app.press(Key::Down);
    let card = app.focused();
    app.press(Key::Info);
    let watch = app.id(&["details", "content", "watch"]);
    assert!(app.until(|a| a.focused() == Some(watch)), "Info didn't open what's on");
    app.press(Key::Back);
    app.advance(0.4);
    assert_eq!(app.focused(), card, "focus isn't back on the card");
    app.press(Key::Back);
    assert_eq!(app.route(), Some(&Route::LiveTv), "Back from the cards left the page");
    assert_eq!(app.focused(), Some(category_row(&app, 3)), "Back from the cards isn't on All channels");
}

#[test]
fn a_search_looks_through_every_channel() {
    let mut app = live();
    to_categories(&mut app);
    for _ in 0..3 {
        app.press(Key::Up);
    }
    assert_eq!(app.focused(), Some(category_row(&app, 0)), "the search field");
    app.press(Key::Ok);
    app.type_text("zzz");
    assert!(app.until(|a| a.shows("No channels match")), "no empty search");
    assert!(app.shows("No channel's name, number or programme on now has “zzz” in it."));
}

#[test]
fn a_search_keeps_the_category_shown_marked_and_left_goes_back_to_it() {
    let mut app = live();
    to_categories(&mut app);
    for _ in 0..3 {
        app.press(Key::Up);
    }
    app.press(Key::Ok);
    // "news" names UK| News: it is listed first, under the field.
    app.type_text("news");
    assert!(app.until(|a| a.shows("GROUPS FOUND")), "no groups found");
    // Search, at the keyboard's bottom right: focus goes to the results.
    for key in [Key::Down; 6].into_iter().chain([Key::Right; 12]) {
        app.press(key);
    }
    app.press(Key::Ok);
    let guide = app.id(&["guide"]);
    assert!(app.until(|a| a.focused() == Some(guide)), "the results haven't focus");
    to_categories(&mut app);
    // The field, the heading, UK| News found, Favourites, Recently watched, then All channels.
    assert_eq!(app.focused(), Some(category_row(&app, 5)), "Left from the results isn't on the category shown");
}

#[test]
fn live_tv_closed_to_the_profile_says_so() {
    let mut app = TestApp::start_as(ids::SAM);
    app.open(Route::LiveTv);
    assert!(app.until(|a| a.shows("Live TV isn't open to this profile")), "no notice for Sam");
    assert!(!app.shows("Guide"), "no tabs without Live TV");
}
