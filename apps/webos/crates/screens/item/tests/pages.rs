//! The title, actions and person pages against the fake server: what they show, where focus goes,
//! what they play and the changes they ask of the server.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::TitleKind;
use pley_fixtures::{ids, request_titles};
use pley_shell::{AppCx, PlayRequest, Requests, Route};

/// The fixture movie in progress for Alex.
const MOVIE: i64 = ids::PGS_MOVIE;
const MOVIE_TITLE: &str = "The Electric Harbor";

fn open_movie(app: &mut TestApp) {
    app.open(Route::Item { id: MOVIE });
    assert!(app.until(|a| a.shows(MOVIE_TITLE) && a.shows("Resume")), "the movie's page never showed");
}

fn open_show(app: &mut TestApp) {
    app.open(Route::Item { id: ids::SHOW });
    assert!(app.until(|a| a.shows("Resume S1 · E2")), "the show's page never offered its next episode");
}

fn played(app: &TestApp) -> Option<PlayRequest> {
    match app.route() {
        Some(Route::Player(r)) => Some(r.clone()),
        _ => None,
    }
}

#[test]
fn the_factory_draws_its_pages_only() {
    let mut app = TestApp::start();
    let mut requests = Requests::default();
    let mut cx = AppCx::new(&mut app.state, &mut requests, 0.0);
    let ours = [Route::Item { id: 1 }, Route::ItemActions { id: 1, continue_watching: true }, Route::Person { id: 2 }];
    for route in ours {
        assert!(pley_item::screen(&route, &mut cx).is_some(), "{route:?}");
    }
    for route in [Route::Home, Route::Artist { id: 1 }, Route::Album { id: 2, track_id: None }, Route::Playlists] {
        assert!(pley_item::screen(&route, &mut cx).is_none(), "{route:?}");
    }
}

#[test]
fn a_movie_in_progress_resumes_or_starts_over() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    assert_eq!(app.focused(), Some(app.id(&["buttons", "play"])));
    app.press(Key::Ok);
    let r = played(&app).expect("Resume opened no player");
    assert_eq!((r.item_id, r.from_start, r.start_ms), (MOVIE, false, None));

    app.press(Key::Back);
    assert!(app.until(|a| a.focused() == Some(a.id(&["buttons", "play"]))), "focus didn't come back to Resume");
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(app.id(&["buttons", "from-start"])));
    assert!(app.shows("From start"));
    app.press(Key::Ok);
    let r = played(&app).expect("From start opened no player");
    assert_eq!((r.item_id, r.from_start), (MOVIE, true));
}

#[test]
fn the_tracks_panel_picks_what_play_asks_for() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    // Resume, From start, watched, then Audio and subtitles.
    for _ in 0..3 {
        app.press(Key::Right);
    }
    assert_eq!(app.focused(), Some(app.id(&["buttons", "tracks"])));
    app.press(Key::Ok);
    assert!(app.modal() && app.shows("Audio and subtitles") && app.shows("Automatic (English)"));
    // Focus starts on the version in force; the second version is one row down.
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.modal(), "choosing a version closed the panel");
    app.press(Key::Back);
    assert!(app.until(|a| !a.modal() && a.focused() == Some(a.id(&["buttons", "tracks"]))));
    for _ in 0..3 {
        app.press(Key::Left);
    }
    app.press(Key::Ok);
    let r = played(&app).expect("Resume opened no player");
    assert_eq!(r.item_id, MOVIE);
    assert!(r.file_id.is_some_and(|f| f != ids::PGS_MOVIE_FILE), "the version picked wasn't asked for: {r:?}");
}

#[test]
fn holding_ok_on_play_opens_the_titles_actions() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    app.hold(Key::Ok, 0.7);
    assert!(app.modal(), "holding OK opened nothing");
    assert!(app.shows("Play from start") && app.shows("Mark as watched"));
    // My List and Continue Watching aren't offered on the title's own page.
    assert!(!app.shows("Add to My List") && !app.shows("Remove from Continue Watching"));
    assert_eq!(played(&app), None, "holding OK also played");
    app.press(Key::Back);
    assert!(app.until(|a| !a.modal() && a.focused() == Some(a.id(&["buttons", "play"]))));
}

#[test]
fn marking_watched_shows_at_once_and_tells_the_server() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    app.press(Key::Right);
    app.press(Key::Right);
    assert!(app.shows("Mark as watched"));
    app.press(Key::Ok);
    assert!(app.shows("Mark as unwatched"), "the mark didn't change at once");
    let path = format!("/api/items/{MOVIE}/watched");
    assert!(app.until(|a| !a.writes("POST", &path).is_empty()), "the server wasn't told");
    // Watched, the title plays from its start: the pill says Play and From start goes.
    assert!(app.until(|a| a.shows("Play") && !a.shows("Resume")));
}

#[test]
fn a_mark_the_server_took_stays_over_an_answer_asked_before_it() {
    let mut app = TestApp::start_with_threads(16);
    open_movie(&mut app);
    // The page asks again (another device changed something), and those answers take 800 ms.
    app.server.set_mode(pley_fixtures::Mode::Slow).unwrap();
    app.state.refresh();
    let start = std::time::Instant::now();
    app.until(|_| start.elapsed() > std::time::Duration::from_millis(200));
    // The mark goes at once, and the server takes it before that answer comes.
    app.server.set_mode(pley_fixtures::Mode::Normal).unwrap();
    app.press(Key::Right);
    app.press(Key::Right);
    app.press(Key::Ok);
    let path = format!("/api/items/{MOVIE}/watched");
    assert!(app.until(|a| !a.writes("POST", &path).is_empty()), "the server wasn't told");
    // The answer asked before the mark comes, then the one asked after it: the mark stays.
    let start = std::time::Instant::now();
    app.until(|a| {
        assert!(a.shows("Mark as unwatched"), "an answer from before the mark took it back");
        start.elapsed() > std::time::Duration::from_millis(1_500)
    });
    assert!(app.until(|a| a.shows("Play") && !a.shows("Resume")));
}

#[test]
fn a_show_goes_down_to_its_next_episode_and_up_to_the_seasons() {
    let mut app = TestApp::start();
    open_show(&mut app);
    assert!(app.until(|a| a.shows("Season 1") && a.shows("Season 2") && a.shows("2. Episode 2")));
    app.press(Key::Down);
    let next = app.id(&["episodes"]).with(ids::SHOW_S1E2 as u64);
    assert_eq!(app.focused(), Some(next), "Down didn't land on the next episode");
    // Once focus rests on it, the info block tells of it, and still does over the tabs.
    app.advance(0.5);
    let told = "Season 1 · Episode 2 · 20s · Aired 10 October 2025";
    assert!(app.shows(told));
    app.press(Key::Up);
    let season1 = app.id(&["tabs"]).with(ids::SHOW_SEASON_1 as u64);
    assert_eq!(app.focused(), Some(season1));
    app.advance(0.5);
    assert!(app.shows(told), "the info block let go of the episode on the tabs");
    // Resting on the next tab shows its season.
    app.press(Key::Right);
    app.advance(0.5);
    assert!(app.until(|a| a.focused() == Some(a.id(&["tabs"]).with(ids::SHOW_SEASON_2 as u64))));
    // Season 2's three episodes, none started, replace season 1's (whose second is in progress).
    let unstarted = |a: &TestApp| a.list.texts().filter(|t| *t == "20s").count();
    assert!(app.until(|a| unstarted(a) == 3), "season 2's episodes never came");
    app.press(Key::Down);
    let first = app.id(&["episodes"]).with(ids::SHOW_S2E1 as u64);
    assert!(app.until(|a| a.focused() == Some(first)), "season 2's episodes never took focus");
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Item { id: ids::SHOW_S2E1 }));
}

#[test]
fn a_shows_play_plays_its_next_episode() {
    let mut app = TestApp::start();
    open_show(&mut app);
    app.press(Key::Ok);
    // The episode offered can be the one saved: Play then plays the one the server names.
    assert!(app.until(|a| played(a).is_some()), "Play opened no player");
    let r = played(&app).unwrap();
    assert_eq!((r.item_id, r.from_start, r.file_id), (ids::SHOW_S1E2, false, None));
}

#[test]
fn a_show_missing_seasons_offers_to_request_more_of_them() {
    let mut app = TestApp::start();
    open_show(&mut app);
    // The button comes once the server named the seasons left (season 3 of the fixture show).
    let more = app.id(&["buttons", "request-more"]);
    let reached = app.until(|a| {
        a.ui.request_focus(more);
        a.frame(Vec::new());
        a.focused() == Some(more)
    });
    assert!(reached, "Request more seasons never showed");
    assert!(app.until(|a| a.shows("Request more seasons")), "its label shows with focus");
    app.press(Key::Ok);
    let picker = Route::Title { kind: TitleKind::Show, tmdb_id: request_titles::SHOW_PARTIAL, pick: true };
    assert_eq!(app.route(), Some(&picker));
}

#[test]
fn without_requests_a_show_offers_no_more_seasons() {
    let mut app = TestApp::start();
    app.server.set_requests(false);
    app.state.load_requests(false);
    assert!(app.until(|a| a.state.request_summary().is_some_and(|s| !s.enabled)), "requests never went off");
    open_show(&mut app);
    let more = app.id(&["buttons", "request-more"]);
    let start = std::time::Instant::now();
    let reached = app.until(|a| {
        a.ui.request_focus(more);
        a.frame(Vec::new());
        a.focused() == Some(more) || start.elapsed() > std::time::Duration::from_secs(1)
    });
    assert!(reached && app.focused() != Some(more), "no button to request more seasons");
}

#[test]
fn a_shows_play_before_its_next_episode_is_known_plays_it_once_named() {
    let mut app = TestApp::start();
    // Every answer takes 800 ms: the show comes, then its next episode.
    app.server.set_mode(pley_fixtures::Mode::Slow).unwrap();
    app.open(Route::Item { id: ids::SHOW });
    assert!(app.until(|a| a.shows("Play")), "the show never came");
    app.press(Key::Ok);
    assert_eq!(played(&app), None, "Play went ahead without the next episode");
    assert!(app.until(|a| played(a).is_some()), "the next episode never played");
    assert_eq!(played(&app).map(|r| r.item_id), Some(ids::SHOW_S1E2));
    assert_ne!(app.toast(), Some("Nothing to play: this item has no media files."));
}

#[test]
fn an_extra_resumes_where_it_was_left() {
    let mut app = TestApp::start();
    // The fourth movie has a trailer, in the row under the hero.
    app.open(Route::Item { id: 4 });
    assert!(app.until(|a| a.shows("Extras")), "the extras never showed");
    app.press(Key::Down);
    app.press(Key::Ok);
    let r = played(&app).expect("the extra didn't play");
    assert_eq!((r.item_id, r.from_start), (2_001, false));
}

#[test]
fn an_episode_page_names_its_show_and_place() {
    let mut app = TestApp::start();
    app.open(Route::Item { id: ids::SHOW_S1E2 });
    assert!(app.until(|a| a.shows("Fixture Show") && a.shows("S1 · E2 · Episode 2")));
    assert!(app.shows("Season 1"), "no header over the season's episodes");
    // More leads to the show and the season.
    while app.focused() != Some(app.id(&["buttons", "more"])) {
        let before = app.focused();
        app.press(Key::Right);
        assert_ne!(app.focused(), before, "More was never reached");
    }
    app.press(Key::Ok);
    assert!(app.shows("Go to show") && app.shows("Go to season") && app.shows("Your rating"));
}

#[test]
fn a_tiles_actions_page_acts_and_closes() {
    let mut app = TestApp::start();
    app.open(Route::ItemActions { id: MOVIE, continue_watching: true });
    assert!(app.until(|a| a.shows(MOVIE_TITLE) && a.shows("Remove from Continue Watching")));
    for label in ["Resume", "Play from start", "Mark as watched", "Add to My List", "Add to playlist"] {
        assert!(app.shows(label), "no {label}");
    }
    // The first row has focus: two down is Mark as watched.
    app.press(Key::Down);
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(!matches!(app.route(), Some(Route::ItemActions { .. })), "the actions page didn't close");
    let path = format!("/api/items/{MOVIE}/watched");
    assert!(app.until(|a| !a.writes("POST", &path).is_empty()), "the change wasn't sent");
}

#[test]
fn a_tiles_actions_play_a_show_from_its_next_episode() {
    let mut app = TestApp::start();
    app.open(Route::ItemActions { id: ids::SHOW, continue_watching: false });
    // A show isn't in progress itself: its first action is Play.
    assert!(app.until(|a| a.shows("Fixture Show") && a.shows("Play") && !a.shows("Go to show")));
    app.press(Key::Ok);
    assert!(app.until(|a| played(a).is_some()), "the next episode never played");
    assert_eq!(played(&app).map(|r| r.item_id), Some(ids::SHOW_S1E2));
}

#[test]
fn a_person_page_starts_on_their_first_title() {
    let mut app = TestApp::start();
    app.open(Route::Person { id: 1 });
    assert!(app.until(|a| a.shows("Ada Abbott") && a.shows("Acting")), "the person never showed");
    assert!(app.shows("41 movies · 8 shows in your libraries"));
    let focused = app.focused().expect("nothing focused");
    app.press(Key::Ok);
    assert!(matches!(app.route(), Some(Route::Item { .. })), "OK didn't open a title: {:?}", app.route());
    app.press(Key::Back);
    assert!(app.until(|a| a.focused() == Some(focused)), "focus didn't come back to the title");
    // The menu key opens its actions over the page; Back gives focus back to the title.
    app.press(Key::Menu);
    assert!(app.until(|a| a.shell.panel_open() && a.shows("Add to playlist")), "no actions panel");
    assert_eq!(app.route(), Some(&Route::Person { id: 1 }));
    assert!(!app.shows("Remove from Continue Watching"));
    app.press(Key::Back);
    app.advance(0.3);
    assert!(!app.shell.panel_open());
    assert_eq!(app.focused(), Some(focused), "focus didn't come back to the title");
}

#[test]
fn a_tiles_actions_panel_plays_a_show_from_its_next_episode() {
    let mut app = TestApp::start();
    app.open(Route::Item { id: ids::SHOW_S1E2 });
    assert!(app.until(|a| a.shows("Fixture Show")));
    // What a tile asks for: the show's actions over the page; Play is first.
    app.shell.navigate(
        pley_shell::NavCommand::TileActions { id: ids::SHOW, continue_watching: false },
        &mut app.state,
        &mut app.ui,
    );
    assert!(app.until(|a| a.shell.panel_open()), "no actions panel");
    app.press(Key::Ok);
    assert!(!app.shell.panel_open(), "the panel didn't close");
    assert!(app.until(|a| played(a).is_some()), "the next episode never played");
    assert_eq!(played(&app).map(|r| r.item_id), Some(ids::SHOW_S1E2));
}

#[test]
fn a_title_that_isnt_there_says_so() {
    let mut app = TestApp::start();
    app.open(Route::Item { id: 987_654_321 });
    assert!(app.until(|a| a.shows("Item not found")), "no message");
    assert!(app.shows("It may have been removed, or this profile can't see it."));
    app.press(Key::Ok);
    assert!(!matches!(app.route(), Some(Route::Item { .. })), "Back didn't leave the page");
}

#[test]
fn a_page_shows_still_blocks_until_the_title_comes() {
    let mut app = TestApp::start();
    app.server.set_mode(pley_fixtures::Mode::Slow).unwrap();
    app.open(Route::Item { id: ids::SAMPLE_1080P });
    assert!(!app.list.texts().any(|t| t == "Play" || t == "Resume"), "the page drew buttons before its title came");
    app.server.set_mode(pley_fixtures::Mode::Normal).unwrap();
    assert!(app.until(|a| a.shows("Play") || a.shows("Resume")), "the title never came");
}

#[test]
fn down_past_media_info_reads_on_and_up_takes_the_steps_back() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    let media = app.id(&["details", "media"]);
    for _ in 0..6 {
        if app.focused() == Some(media) {
            break;
        }
        app.press(Key::Down);
    }
    assert_eq!(app.focused(), Some(media), "Media info was never reached");
    app.press(Key::Ok);
    assert!(app.shows("Video") && app.shows("Codec"), "Media info didn't open");
    // Nothing below takes focus: Down reads on, and Up first takes that step back.
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(media));
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(media), "Up moved focus before the step was taken back");
    app.press(Key::Up);
    assert_ne!(app.focused(), Some(media), "Up never left Media info");
}

/// Moves focus to the movie's Media info, opens it and reads on `steps` times.
fn read_on_past_media_info(app: &mut TestApp, steps: usize) {
    let media = app.id(&["details", "media"]);
    for _ in 0..6 {
        if app.focused() == Some(media) {
            break;
        }
        app.press(Key::Down);
    }
    assert_eq!(app.focused(), Some(media), "Media info was never reached");
    app.press(Key::Ok);
    for _ in 0..steps {
        app.press(Key::Down);
    }
    app.advance(0.5);
    assert_eq!(app.focused(), Some(media));
}

/// Whether `text` was drawn on the screen, under the safe area's top.
fn on_screen(app: &TestApp, text: &str) -> bool {
    app.text_y(text).is_some_and(|y| (27.0..540.0).contains(&y))
}

#[test]
fn closing_media_info_while_reading_keeps_it_on_screen() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    read_on_past_media_info(&mut app, 4);
    // Closed, the details are much shorter: the page doesn't stay read on past their end.
    app.press(Key::Ok);
    app.advance(0.5);
    assert!(on_screen(&app, "Media info"), "Media info went off screen: {:?}", app.text_y("Media info"));
    // Up first takes back what reading is left, then leaves Media info.
    for _ in 0..4 {
        app.press(Key::Up);
    }
    assert_ne!(app.focused(), Some(app.id(&["details", "media"])), "Up never left Media info");
}

#[test]
fn moving_along_a_row_ends_reading() {
    let mut app = TestApp::start();
    open_show(&mut app);
    // The rows come from other threads, and the moves below go by the app's clock alone: they
    // start once More Like This is in (the page keeps it once it has drawn it).
    let related = pley_core::store::snapshot_keys::related(ids::SHOW);
    let in_place = |a: &TestApp| {
        let (Some(server), Some(profile)) = (a.state.server(), a.state.profile()) else { return false };
        a.state.snapshots().read(server, profile, &related).is_some()
    };
    assert!(app.until(|a| in_place(a)), "More Like This never came");
    // More Like This is the last row that takes focus; its header comes to the rows' line.
    let header = |a: &TestApp| a.text_y("More Like This");
    for _ in 0..6 {
        if header(&app) == Some(172.0) {
            break;
        }
        app.press(Key::Down);
        app.advance(0.4);
    }
    assert_eq!(header(&app), Some(172.0), "More Like This never took focus");
    // Down reads on into the details under it, moving the row up.
    app.press(Key::Down);
    app.advance(0.5);
    assert!(header(&app).is_some_and(|y| y < 100.0), "reading on didn't move the page: {:?}", header(&app));
    // The next title brings the row back to its line.
    app.press(Key::Right);
    app.advance(0.5);
    assert_eq!(header(&app), Some(172.0), "the row stayed read on");
}

#[test]
fn down_at_the_end_of_a_panel_leaves_the_page_where_it_was() {
    let mut app = TestApp::start();
    open_movie(&mut app);
    let more = app.id(&["buttons", "more"]);
    while app.focused() != Some(more) {
        let before = app.focused();
        app.press(Key::Right);
        assert_ne!(app.focused(), before, "More was never reached");
    }
    app.press(Key::Ok);
    assert!(app.modal() && app.shows("5 stars"));
    // Past the panel's last row, Down has nowhere to go: the page behind doesn't read on.
    for _ in 0..12 {
        app.press(Key::Down);
    }
    app.press(Key::Back);
    assert!(app.until(|a| !a.modal() && a.focused() == Some(more)), "focus didn't come back to More");
    app.advance(0.5);
    assert!(on_screen(&app, MOVIE_TITLE), "the hero went off screen: {:?}", app.text_y(MOVIE_TITLE));
    assert!(app.text_y("Details").is_none_or(|y| y > 540.0), "the page behind the panel read on");
}
