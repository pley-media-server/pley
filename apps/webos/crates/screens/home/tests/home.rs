//! Home against the fake server: what it shows first, where focus goes, what the keys open and
//! what it asks the server ahead.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::ItemDetail;
use pley_core::store::snapshot_keys;
use pley_fixtures::{Mode, ids};
use pley_home::HomeScreen;
use pley_shell::{ITEM_TTL, Route};

/// The app on Home with its rows in, and Continue Watching's titles.
fn home() -> (TestApp, Vec<i64>) {
    let mut app = TestApp::start();
    assert_eq!(app.route(), Some(&Route::Home));
    assert!(app.until(|a| a.shows("Resume")), "Home showed no hero");
    let cw = app.state.home().unwrap()[0].clone();
    assert!(cw.continue_watching(), "the first row is {}", cw.key);
    let ids = cw.items.iter().map(|i| i.id).collect();
    (app, ids)
}

fn resume(app: &TestApp) -> pley_ui::Id {
    HomeScreen::resume_id(app.page())
}

fn tile(app: &TestApp, row: &str, item: i64) -> pley_ui::Id {
    HomeScreen::tile_id(app.page(), row, item)
}

#[test]
fn home_opens_on_resume_of_continue_watchings_first_title() {
    let (mut app, cw) = home();
    assert!(app.until(|a| a.focused() == Some(resume(a))), "focus is on {:?}", app.focused());
    let first = app.state.home().unwrap()[0].items[0].clone();
    assert_eq!(first.id, cw[0]);
    assert!(app.shows(&first.title), "the hero isn't Continue Watching's first title");
    assert!(app.shows("Continue Watching"));
    // The hero's title is read ahead once focus has rested on Resume.
    let path = format!("/api/items/{}/prefetch", cw[0]);
    assert!(app.until(|a| !a.server.writes("POST", &path).is_empty()), "no read-ahead of the hero");
}

#[test]
fn down_enters_the_rows_and_up_comes_back_to_the_button_it_left() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Right);
    app.press(Key::Right);
    let info = app.id(&["hero", "info"]);
    assert_eq!(app.focused(), Some(info), "Right twice from Resume (From start, then Info)");
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(tile(&app, "continue_watching", cw[0])));
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(info));
}

#[test]
fn resting_on_a_tile_shows_it_and_reads_it_ahead() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Down);
    app.press(Key::Right);
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(tile(&app, "continue_watching", cw[2])));
    let path = format!("/api/items/{}/prefetch", cw[2]);
    // Not before focus rests (400 ms)...
    assert!(app.server.writes("POST", &path).is_empty());
    // ...then the server reads it ahead, and the info block names it and says what its details say.
    app.advance(0.4);
    assert!(app.until(|a| !a.server.writes("POST", &path).is_empty()), "no read-ahead of the rested tile");
    let detail = app.state.api().unwrap().item(cw[2]).unwrap();
    let rating = detail.content_rating.clone().expect("the fixture's movies have a content rating");
    assert!(app.until(|a| a.shows(&rating)), "the info block hasn't the details' content rating {rating}");
    let title = detail.summary.title.clone();
    assert!(app.list.texts().filter(|t| *t == title).count() >= 2, "the info block doesn't name {title}");
}

#[test]
fn back_in_the_rows_goes_to_the_hero_and_the_rail_whose_right_lands_on_resume() {
    let (mut app, _) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Down);
    app.press(Key::Down);
    app.press(Key::Down);
    assert!(app.focused() != Some(resume(&app)));
    app.press(Key::Back);
    app.advance(0.3);
    assert!(app.shell.rail_open(), "the rail didn't open");
    assert_eq!(app.route(), Some(&Route::Home), "Back left Home");
    app.press(Key::Right);
    app.advance(0.3);
    assert!(!app.shell.rail_open());
    assert_eq!(app.focused(), Some(resume(&app)));
}

#[test]
fn a_tile_opens_its_page_and_its_actions() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Down);
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Item { id: cw[0] }));
    app.press(Key::Back);
    app.advance(0.3);
    assert_eq!(app.route(), Some(&Route::Home));
    let held = tile(&app, "continue_watching", cw[0]);
    assert_eq!(app.focused(), Some(held), "focus didn't come back to the tile");
    // Its actions open in a side panel over Home, which stays the page, focus on the first.
    app.hold_ok();
    assert!(app.until(|a| a.shell.panel_open() && a.shows("Remove from Continue Watching")), "no actions panel");
    assert_eq!(app.route(), Some(&Route::Home));
    assert!(app.shows("Resume") && app.shows("Play from start"));
    assert_ne!(app.focused(), Some(held), "focus stayed on the tile under the panel");
    // Back closes it, focus back on the tile.
    app.press(Key::Back);
    app.advance(0.3);
    assert!(!app.shell.panel_open());
    assert_eq!(app.route(), Some(&Route::Home), "Back left Home");
    assert_eq!(app.focused(), Some(held), "focus didn't come back to the tile");
}

#[test]
fn a_choice_in_a_tiles_actions_runs_with_focus_back_on_the_tile() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Down);
    let held = tile(&app, "continue_watching", cw[0]);
    app.press(Key::Menu);
    assert!(app.until(|a| a.shell.panel_open()), "no actions panel");
    // Play from start, the second row: the player opens over Home, and Back comes to the tile.
    app.press(Key::Down);
    app.press(Key::Ok);
    match app.route() {
        Some(Route::Player(request)) => assert!(request.item_id == cw[0] && request.from_start),
        other => panic!("Play from start opened {other:?}"),
    }
    app.press(Key::Back);
    app.advance(0.3);
    assert_eq!(app.route(), Some(&Route::Home));
    assert!(!app.shell.panel_open(), "the panel came back with Home");
    assert_eq!(app.focused(), Some(held), "focus didn't come back to the tile");
}

#[test]
fn a_tile_its_actions_took_away_leaves_focus_to_its_neighbour() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Down);
    let held = tile(&app, "continue_watching", cw[0]);
    app.press(Key::Menu);
    assert!(app.until(|a| a.shell.panel_open() && a.shows("Remove from Continue Watching")), "no actions panel");
    // Resume, Play from start, Mark as watched, then Remove from Continue Watching.
    for _ in 0..3 {
        app.press(Key::Down);
    }
    app.press(Key::Ok);
    // The panel closes and focus is back on the tile before the title leaves the row.
    assert!(!app.shell.panel_open(), "the panel didn't close");
    assert_eq!(app.focused(), Some(held), "focus didn't come back to the tile");
    let path = format!("/api/continue-watching/{}", cw[0]);
    assert!(app.until(|a| !a.server.writes("DELETE", &path).is_empty()), "the change wasn't sent");
    app.state.refresh();
    assert!(app.until(|a| a.state.home().is_some_and(|h| h[0].items.first().map(|i| i.id) == Some(cw[1]))));
    app.advance(0.2);
    assert_eq!(app.focused(), Some(tile(&app, "continue_watching", cw[1])));
}

#[test]
fn resume_plays_the_hero_from_where_it_was_left() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Ok);
    match app.route() {
        Some(Route::Player(request)) => assert!(request.item_id == cw[0] && !request.from_start),
        other => panic!("Resume opened {other:?}"),
    }
}

#[test]
fn marking_the_hero_watched_tells_the_server() {
    let (mut app, cw) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    app.press(Key::Right);
    app.press(Key::Right);
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(app.id(&["hero", "watched"])));
    app.press(Key::Ok);
    let path = format!("/api/items/{}/watched", cw[0]);
    assert!(app.until(|a| !a.server.writes("POST", &path).is_empty()), "the mark didn't reach the server");
    // Watched, it leaves Continue Watching.
    let gone = |a: &mut TestApp| {
        a.state
            .home()
            .is_some_and(|h| h.iter().filter(|h| h.continue_watching()).all(|h| h.items.iter().all(|i| i.id != cw[0])))
    };
    assert!(app.until(gone), "the watched title is still in Continue Watching");
}

#[test]
fn a_slow_server_shows_nothing_then_the_skeleton_then_home() {
    let mut app = TestApp::start_with(Mode::Slow);
    app.advance(0.1);
    assert!(!app.shows("Resume"));
    let blocks = |app: &TestApp| {
        app.list
            .cmds
            .iter()
            .filter(|c| matches!(c, pley_base::Cmd::Fill { rect, .. } if rect.y == 422.0 && rect.h == 115.0))
            .count()
    };
    assert_eq!(blocks(&app), 0, "the skeleton showed before 300 ms");
    app.advance(0.3);
    assert_eq!(blocks(&app), 4, "no skeleton after 300 ms");
    app.server.set_mode(Mode::Normal).unwrap();
    assert!(app.until(|a| a.shows("Resume")), "Home never came");
    assert!(app.until(|a| a.focused() == Some(resume(a))), "Resume didn't take focus once Home came");
}

#[test]
fn a_server_out_of_reach_says_so_and_retry_asks_again() {
    let mut app = TestApp::start_with(Mode::Down);
    assert!(app.until(|a| a.shows("Can't reach your server")), "no notice");
    let retry = app.id(&["message", "action"]);
    assert!(app.until(|a| a.focused() == Some(retry)), "Retry hasn't focus: {:?}", app.focused());
    app.server.set_mode(Mode::Normal).unwrap();
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Resume")), "Retry didn't bring Home");
}

#[test]
fn the_tile_focus_rests_on_gets_its_details_whatever_was_asked_ahead() {
    let (mut app, _) = home();
    app.until(|a| a.focused() == Some(resume(a)));
    let key = format!("recently_added:{}", ids::MOVIES);
    let rows = app.state.home().unwrap().to_vec();
    let at = rows.iter().position(|h| h.key == key).expect("no row of the movies");
    let movies: Vec<i64> = rows[at].items.iter().map(|i| i.id).collect();
    assert!(movies.len() >= 15, "the row has {} movies", movies.len());
    for _ in 0..=at {
        app.press(Key::Down);
    }
    assert_eq!(app.focused(), Some(tile(&app, &key, movies[0])));
    // A slow server: while the first answers are on their way, focus rests on tile after tile,
    // each wanting the next two ahead, more than the queue holds.
    app.server.set_mode(Mode::Slow).unwrap();
    for i in [0, 3, 6, 9, 12] {
        while app.focused() != Some(tile(&app, &key, movies[i])) {
            app.press(Key::Right);
        }
        app.advance(0.45);
    }
    assert!(app.until(|a| a.state.cached_item(movies[12]).is_some()), "the rested tile's details never came");
}

#[test]
fn the_heros_saved_details_show_while_the_server_is_away() {
    let (mut app, cw) = home();
    let detail = app.state.api().unwrap().item(cw[0]).unwrap();
    let rating = detail.content_rating.clone().expect("the fixture's movies have a content rating");
    assert!(app.until(|a| a.shows(&rating)), "the hero hasn't its details' content rating {rating}");
    // Home saved them as the title's page does.
    let (server, profile) = (app.state.server().unwrap().to_string(), app.state.profile().unwrap());
    let saved = snapshot_keys::item(cw[0]);
    let found = |a: &mut TestApp| a.state.snapshots().get::<ItemDetail>(&server, profile, &saved).is_some();
    assert!(app.until(found), "the hero's details weren't saved");
    // Later, the server away and the app's own cache of them gone: a new Home shows them from what
    // was saved.
    app.server.set_mode(Mode::Down).unwrap();
    app.time += ITEM_TTL + 1.0;
    app.open(Route::Home);
    // The hero waits the moment they take to read: its buttons never show without them.
    assert!(app.until(|a| a.shows("Resume")), "no hero");
    assert!(app.shows(&rating), "the hero showed before its saved details");
}
