//! The library, collection and playlist pages driven headless against the fake server: what takes
//! focus, what the keys do, and what reaches the server.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::device::Drive;
use pley_core::model::{NewPlaylist, PlaylistKind};
use pley_fixtures::ids;
use pley_shell::{BackgroundSession, Route};
use pley_ui::Id;

/// An entry's card on a playlist's page.
fn card(app: &TestApp, entry: u64) -> Id {
    app.id(&["entries"]).with(entry).with("entry")
}

fn press(app: &mut TestApp, keys: &[Key]) {
    for k in keys {
        app.press(*k);
    }
}

#[test]
fn playlists_open_on_the_first_cover() {
    let mut app = TestApp::start();
    app.open(Route::Playlists);
    assert!(app.until(|a| a.shows("Favourites") && a.shows("Road Trip")));
    let first = app.id(&["grid"]).with(0u64);
    assert!(app.until(|a| a.focused() == Some(first)), "the first cover takes focus");
    assert!(app.shows_part("Smart"), "a smart playlist says so under its cover");
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Playlist { id: ids::PLAYLIST_VIDEO }));
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
fn audio_guidance_names_playlists_as_it_opens_with_the_rail_full() {
    let mut app = TestApp::start();
    let rail_has = |a: &TestApp, id: &str| a.shell.rail_entries().has(id);
    assert!(app.until(|a| rail_has(a, "discover") && rail_has(a, "live-tv")), "no Discover or Live TV");
    // Now Playing and a drive as well: Playlists keeps its place in the rail, and the page says
    // its name.
    app.state.music = Some(Box::new(Queued));
    app.device.set_drive(Some(Drive { root: std::env::temp_dir(), hidden: false }));
    assert!(app.until(|a| rail_has(a, "storage") && rail_has(a, "now-playing")), "no drive or Now Playing");
    assert!(rail_has(&app, "playlists"), "Playlists left the rail");
    app.ui.set_speaking(true);
    app.open(Route::Playlists);
    let words = app.ui.take_speech().unwrap_or_default();
    assert!(words.starts_with("Playlists"), "{words}");
}

#[test]
fn a_moved_entry_is_sent_in_its_new_place() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_VIDEO });
    assert!(app.until(|a| a.shows("Sample 1080p")));
    let play = app.id(&["buttons", "play"]);
    assert!(app.until(|a| a.focused() == Some(play)), "Play takes focus");
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(card(&app, 1)));
    app.press(Key::Menu);
    assert!(app.until(|a| a.shows("Play from here") && a.shows("Remove from playlist")));
    press(&mut app, &[Key::Down, Key::Down, Key::Ok]);
    let hint = "Up and down to move it, OK to drop it here, Back to put it back";
    assert!(app.until(|a| a.shows(hint)));
    assert_eq!(app.focused(), Some(card(&app, 1)), "the lifted entry keeps focus");
    press(&mut app, &[Key::Down, Key::Ok]);
    assert!(app.until(|a| !a.server.writes("PUT", "/api/playlists/1/order").is_empty()));
    let sent = app.server.writes("PUT", "/api/playlists/1/order")[0].body.clone();
    assert!(sent.contains("[2,1,3,"), "sent {sent}");
    assert!(app.until(|a| !a.shows(hint)));
}

#[test]
fn back_puts_a_lifted_entry_back_and_sends_nothing() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_VIDEO });
    assert!(app.until(|a| a.shows("Sample 1080p")));
    press(&mut app, &[Key::Down, Key::Menu]);
    assert!(app.until(|a| a.shows("Play from here")));
    press(&mut app, &[Key::Down, Key::Down, Key::Ok, Key::Down, Key::Back]);
    app.advance(0.5);
    assert!(!app.shows("Up and down to move it, OK to drop it here, Back to put it back"));
    assert_eq!(app.route(), Some(&Route::Playlist { id: ids::PLAYLIST_VIDEO }), "Back stayed on the page");
    assert!(app.server.writes("PUT", "/api/playlists/1/order").is_empty());
}

#[test]
fn a_removed_entry_goes_at_once_and_focus_moves_on() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_VIDEO });
    assert!(app.until(|a| a.shows("Sample 1080p")));
    press(&mut app, &[Key::Down, Key::Menu]);
    assert!(app.until(|a| a.shows("Remove from playlist")));
    press(&mut app, &[Key::Down, Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("Removed “Sample 1080p”")));
    assert!(app.until(|a| !a.server.writes("DELETE", "/api/playlists/1/items/1").is_empty()));
    assert!(app.until(|a| a.focused() == Some(card(a, 2))), "the next entry takes focus");
    assert!(app.until(|a| a.shows("11 items · 15h 21m") || a.shows_part("11 items")));
}

#[test]
fn rename_types_a_title_and_sends_it() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_VIDEO });
    assert!(app.until(|a| a.shows("Favourites")));
    press(&mut app, &[Key::Right, Key::Right, Key::Ok]);
    assert!(app.until(|a| a.shows("Rename playlist")));
    // OK on the title types it; Back closes the keyboard, Down reaches Save.
    app.press(Key::Ok);
    app.type_text(" two");
    press(&mut app, &[Key::Back, Key::Down, Key::Ok]);
    assert!(app.until(|a| !a.server.writes("PATCH", "/api/playlists/1").is_empty()));
    let sent = app.server.writes("PATCH", "/api/playlists/1")[0].body.clone();
    assert!(sent.contains("Favourites two"), "sent {sent}");
    assert!(app.until(|a| a.shows("Favourites two")));
}

#[test]
fn delete_asks_first_then_leaves_the_page() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_VIDEO });
    assert!(app.until(|a| a.shows("Favourites")));
    press(&mut app, &[Key::Right, Key::Right, Key::Right, Key::Ok]);
    assert!(app.until(|a| a.shows("Delete playlist?")));
    // Cancel has focus: Right reaches Delete.
    press(&mut app, &[Key::Right, Key::Ok]);
    assert!(app.until(|a| !a.server.writes("DELETE", "/api/playlists/1").is_empty()));
    assert!(app.until(|a| a.route() != Some(&Route::Playlist { id: ids::PLAYLIST_VIDEO })));
}

#[test]
fn a_music_playlist_opens_the_album_of_the_entry_played() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_AUDIO });
    assert!(app.until(|a| a.shows("Road Trip") && a.shows("Music playlist")));
    let play = app.id(&["buttons", "play"]);
    assert!(app.until(|a| a.focused() == Some(play)), "Play takes focus");
    app.press(Key::Ok);
    assert!(
        app.until(|a| matches!(a.route(), Some(Route::Album { id, .. }) if *id == ids::ALBUM)),
        "Play opened {:?}",
        app.route()
    );
    assert!(app.state.music.is_none(), "nothing plays from the playlist's page itself");
}

#[test]
fn down_from_the_library_tab_reaches_the_sort_chip() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: None, sort: None });
    let first = app.id(&[]).with(("grid", 0u64)).with(0u64);
    assert!(app.until(|a| a.focused() == Some(first)));
    // Right along the grid's first row, then Up twice: the chips, then the tab shown.
    press(&mut app, &[Key::Right, Key::Right, Key::Up, Key::Up]);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "library"])));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(app.id(&["chips", "sort"])));
}

#[test]
fn a_smart_playlist_offers_no_edits() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_SMART });
    assert!(app.until(|a| a.shows("Smart playlist")));
    assert!(app.until(|a| a.shows_part("Genre: Comedy")));
    press(&mut app, &[Key::Down, Key::Menu]);
    assert!(app.until(|a| a.shows("Play from here")));
    assert!(!app.shows("Move") && !app.shows("Remove from playlist"));
}

#[test]
fn a_new_playlist_is_made_and_opened() {
    let mut app = TestApp::start();
    app.open(Route::NewPlaylist { item_ids: Vec::new(), smart: None });
    assert!(app.until(|a| a.shows("Name it, then add to it with “Add to playlist” on any title.")));
    app.press(Key::Ok);
    app.type_text("Late shows");
    // Back closes the keyboard; Music, then Create.
    press(&mut app, &[Key::Back, Key::Down, Key::Down, Key::Ok, Key::Down, Key::Ok]);
    assert!(app.until(|a| !a.server.writes("POST", "/api/playlists").is_empty()));
    let sent = app.server.writes("POST", "/api/playlists")[0].body.clone();
    assert!(sent.contains("Late shows") && sent.contains("audio"), "sent {sent}");
    assert!(app.until(|a| matches!(a.route(), Some(Route::Playlist { .. }))), "the new playlist opens");
}

#[test]
fn adding_to_a_playlist_says_so_and_closes() {
    let mut app = TestApp::start();
    app.open(Route::AddToPlaylist { item_ids: vec![ids::SAMPLE_1080P], title: "Sample 1080p".into() });
    // The panel's lines: its title, the field, Create, "Your playlists", then the playlists.
    let first = app.id(&["lines"]).with(4u64);
    assert!(app.until(|a| a.focused() == Some(first)), "the first video playlist takes focus");
    assert!(!app.shows("Road Trip"), "music playlists don't take a movie");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Added to Favourites")));
    assert_eq!(app.server.writes("POST", "/api/playlists/1/items").len(), 1);
    app.advance(1.6);
    assert!(app.until(|a| !matches!(a.route(), Some(Route::AddToPlaylist { .. }))));
}

#[test]
fn add_to_playlist_scrolls_to_every_playlist() {
    let mut app = TestApp::start();
    let api = app.state.api().expect("signed in");
    for i in 0..9 {
        let body = NewPlaylist { title: format!("Mix {i}"), kind: Some(PlaylistKind::Video), ..NewPlaylist::default() };
        api.create_playlist(&body).expect("made");
    }
    app.open(Route::AddToPlaylist { item_ids: vec![ids::SAMPLE_1080P], title: "Sample 1080p".into() });
    let lines = app.id(&["lines"]);
    assert!(app.until(|a| a.focused() == Some(lines.with(4u64))), "the first playlist takes focus");
    // Ten video playlists: Down reaches the last, which shows.
    for _ in 0..9 {
        app.press(Key::Down);
    }
    assert_eq!(app.focused(), Some(lines.with(13u64)));
    assert!(app.shows("Mix 8"), "the last playlist is drawn");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows_part("Added to ")));
}

#[test]
fn a_library_lists_whole_and_sorts_from_its_panel() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: None, sort: None });
    let first = app.id(&[]).with(("grid", 0u64)).with(0u64);
    assert!(app.until(|a| a.focused() == Some(first)), "the grid's first title takes focus");
    // The A–Z jump shows only once the whole listing is held: the head, then two pages of 1,000.
    assert!(app.until(|a| a.shows("1,500 movies") && a.shows("A")), "the whole listing came");
    // Up to the chips, OK on the sort, Year.
    press(&mut app, &[Key::Up, Key::Ok]);
    assert!(app.until(|a| a.shows("Sort by") && a.shows("Order")));
    press(&mut app, &[Key::Down, Key::Ok]);
    let years = |a: &TestApp| a.list.texts().filter(|t| *t == "Year").count();
    assert!(app.until(|a| !a.shows("Sort by") && years(a) == 2), "the sort chip says Year");
    assert!(app.until(|a| a.shows("1,500 movies")));
}

#[test]
fn unwatched_counts_what_is_left() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: None, sort: None });
    assert!(app.until(|a| a.shows("1,500 movies")));
    // Up to the chips, Right twice to Unwatched.
    press(&mut app, &[Key::Up, Key::Right, Key::Right, Key::Ok]);
    assert!(app.until(|a| a.shows_part(" of 1,500 movies")));
}

/// How many times the last frame drew `text`.
fn times(app: &TestApp, text: &str) -> usize {
    app.list.texts().filter(|t| *t == text).count()
}

#[test]
fn resolution_and_dynamic_range_filter_from_the_filter_panel_and_badge_the_posters() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: None, sort: None });
    assert!(app.until(|a| a.shows("1,500 movies")));
    // By title, the posters carry no quality badges, though the listing has each title's media.
    assert!(!app.shows("4K") && !app.shows("HDR10"), "badges on a grid that isn't about quality");

    // Up to the chips, Right to Filter: its rows reach Resolution and Dynamic range after Year.
    press(&mut app, &[Key::Up, Key::Right, Key::Ok]);
    assert!(app.until(|a| a.shows("Resolution") && a.shows("Dynamic range")), "no rows for them");
    assert!(app.shows("All resolutions") && app.shows("All dynamic ranges"));
    // From All, five down: Unwatched, Watched, Genre, Year, Resolution.
    press(&mut app, &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("1080p")), "the Resolution panel didn't open");
    // The server's choices after All resolutions: 4K, then 1080p.
    press(&mut app, &[Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows_part(" of 1,500 movies")), "the count doesn't say it's filtered");
    // A chip names the choice; the posters wear their badges.
    assert!(app.until(|a| times(a, "4K") > 1 && a.shows("HDR10")), "no chip or no badges");

    // The view is kept: the library opens in it again.
    app.open(Route::Library { id: ids::MOVIES, tab: None, sort: None });
    assert!(app.until(|a| a.shows_part(" of 1,500 movies") && times(a, "4K") > 1), "the filter wasn't kept");

    // The chip opens its panel on the choice: All resolutions clears it, the badges go, and so
    // does the chip, focus going to Filter.
    let grid = app.id(&[]).with(("grid", 0u64)).with(0u64);
    assert!(app.until(|a| a.focused() == Some(grid)));
    press(&mut app, &[Key::Up, Key::Right, Key::Right, Key::Right, Key::Right, Key::Right]);
    assert_eq!(app.focused(), Some(app.id(&["chips", "resolution"])));
    press(&mut app, &[Key::Ok, Key::Up, Key::Ok]);
    assert!(app.until(|a| a.shows("1,500 movies") && !a.shows("HDR10")), "the filter didn't clear");
    // Once the panel has slid out, no chip or poster says 4K.
    assert!(app.until(|a| !a.shows("4K") && !a.shows("All resolutions")), "the chip stayed");
    assert!(app.until(|a| a.focused() == Some(a.id(&["chips", "filter"]))), "focus went with the chip");

    // Dynamic range: Dolby Vision, then Save as… writes it in Plex's syntax.
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Dynamic range")));
    press(&mut app, &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("HDR10+")), "the Dynamic range panel didn't open");
    // All dynamic ranges, HDR, then Dolby Vision.
    press(&mut app, &[Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("Dolby Vision") && a.shows_part(" of 1,500 movies")));
    assert!(app.until(|a| a.focused() == Some(a.id(&["chips", "filter"]))), "Filter has focus back");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Save as…")));
    // Down from All to Save as…, past Clear filters.
    press(&mut app, &[Key::Down; 8]);
    app.press(Key::Ok);
    let filter = "dynamicRange=dolby_vision&sort=titleSort";
    assert!(
        app.until(|a| matches!(a.route(), Some(Route::NewPlaylist { smart: Some(s), .. }) if s.filter == filter)),
        "saved {:?}",
        app.route()
    );
}

#[test]
fn collections_open_a_collection() {
    let mut app = TestApp::start();
    app.open(Route::Collections { library_id: Some(ids::MOVIES) });
    assert!(app.until(|a| a.shows("Movies") && a.shows_part("movies")));
    app.advance(0.3);
    app.press(Key::Ok);
    assert!(app.until(|a| matches!(a.route(), Some(Route::Collection { .. }))));
    assert!(app.until(|a| a.shows("Collection") && a.shows("Play")));
}
