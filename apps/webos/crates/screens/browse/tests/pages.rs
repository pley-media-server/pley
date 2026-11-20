//! The library, collection and playlist pages driven headless against the fake server: what takes
//! focus, what the keys do, and what reaches the server.

mod common;

use common::TestApp;
use pley_base::{Cmd, Key, Rect};
use pley_core::device::Drive;
use pley_core::model::{LibraryQuery, LibrarySort, NewPlaylist, PlaylistKind, RangeFilter, ResolutionFilter};
use pley_core::store::keys;
use pley_fixtures::{Mode, ServerEvent, ids};
use pley_shell::{BackgroundSession, LibraryTab, PlayQueue, Route};
use pley_ui::{Id, theme};

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
    assert!(app.until(|a| a.shows("Favorites") && a.shows("Road Trip")));
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
    assert!(app.until(|a| a.shows("Favorites")));
    press(&mut app, &[Key::Right, Key::Right, Key::Ok]);
    assert!(app.until(|a| a.shows("Rename playlist")));
    // OK on the title types it; Back closes the keyboard, Down reaches Save.
    app.press(Key::Ok);
    app.type_text(" two");
    press(&mut app, &[Key::Back, Key::Down, Key::Ok]);
    assert!(app.until(|a| !a.server.writes("PATCH", "/api/playlists/1").is_empty()));
    let sent = app.server.writes("PATCH", "/api/playlists/1")[0].body.clone();
    assert!(sent.contains("Favorites two"), "sent {sent}");
    assert!(app.until(|a| a.shows("Favorites two")));
}

#[test]
fn delete_asks_first_then_leaves_the_page() {
    let mut app = TestApp::start();
    app.open(Route::Playlist { id: ids::PLAYLIST_VIDEO });
    assert!(app.until(|a| a.shows("Favorites")));
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
    assert!(app.until(|a| a.shows("Added to Favorites")));
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
    assert!(!app.shows("4K") && !app.shows("HDR"), "badges on a grid that isn't about quality");

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
    assert!(app.until(|a| times(a, "4K") > 1 && a.shows("HDR")), "no chip or no badges");

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
    assert!(app.until(|a| a.shows("1,500 movies") && !a.shows("HDR")), "the filter didn't clear");
    // Once the panel has slid out, no chip or poster says 4K.
    assert!(app.until(|a| !a.shows("4K") && !a.shows("All resolutions")), "the chip stayed");
    assert!(app.until(|a| a.focused() == Some(a.id(&["chips", "filter"]))), "focus went with the chip");

    // Dynamic range: Dolby Vision, then Save as… writes it in Plex's syntax.
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Dynamic range")));
    press(&mut app, &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("HDR+")), "the Dynamic range panel didn't open");
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

/// Opens the Movies library in `view`, kept on the TV before it opens, and waits for its listing
/// to say nothing matches.
fn open_in(app: &mut TestApp, view: LibraryQuery) {
    assert!(app.until(|a| a.state.profile().is_some()), "no profile");
    let (server, profile) = (app.state.server().unwrap().to_string(), app.state.profile().unwrap());
    app.state.store().set(&keys::library_view(&server, profile, ids::MOVIES), &view).unwrap();
    app.open(Route::Library { id: ids::MOVIES, tab: None, sort: None });
    assert!(app.until(|a| a.shows("Dolby Vision") && a.shows("No matches")), "the kept view didn't show");
    app.advance(0.5);
}

/// The chips' pills in the last frame, in their order.
fn chip_rects(app: &TestApp) -> Vec<Rect> {
    let pills = app.list.cmds.iter().filter_map(|c| match c {
        Cmd::Fill { rect, .. } if rect.y == theme::HEADER_Y_LIBRARY && rect.h == theme::BUTTON_HEIGHT => Some(*rect),
        _ => None,
    });
    pills.collect()
}

#[test]
fn the_chips_fit_the_screen_whatever_is_chosen() {
    let right = theme::CANVAS_W - theme::SAFE_X;
    let mut app = TestApp::start();
    // Unwatched, 1080p and Dolby Vision, after the Genre and Year chips, leave the count less
    // room than it takes: it doesn't show, rather than show cut short ("0 of 1,5…").
    let crowded = LibraryQuery {
        watched: Some(false),
        resolution: Some(ResolutionFilter::Fhd),
        dynamic_range: Some(RangeFilter::DolbyVision),
        ..LibraryQuery::default()
    };
    open_in(&mut app, crowded);
    assert!(app.until(|a| a.shows("Genre") && a.shows("Year")), "no Genre or Year chip");
    let room = right - chip_rects(&app).last().unwrap().right();
    assert!(room > 48.0, "the count has some room: {room}");
    assert!(!app.shows_part("of 1,500 movies"), "the count shows cut short");

    // The most the row holds: by release date, Unwatched, a long genre, a year, 1080p and Dolby
    // Vision. With the genre's and the sort's names cut short it is wider than the screen still.
    let fullest = LibraryQuery {
        sort: LibrarySort::Release,
        watched: Some(false),
        genre: Some("Science Fiction & Fantasy".into()),
        year: Some(2019),
        resolution: Some(ResolutionFilter::Fhd),
        dynamic_range: Some(RangeFilter::DolbyVision),
        ..LibraryQuery::default()
    };
    open_in(&mut app, fullest);
    let chips = ["sort", "filter", "unwatched", "genre", "year", "resolution", "range"];
    assert!(app.until(|a| a.shows("2019")), "no year chip");
    let rects = chip_rects(&app);
    assert_eq!(rects.len(), chips.len(), "{rects:?}");
    assert!(rects.last().unwrap().right() > right, "the row fits: {rects:?}");
    assert!(!app.shows_part("of 1,500 movies"), "the count shows cut short");
    // Along the row, each chip shows whole between the gutter and the safe area's edge.
    app.ui.request_focus(app.id(&["chips", "sort"]));
    for (i, chip) in chips.iter().enumerate() {
        if i > 0 {
            app.press(Key::Right);
        }
        app.advance(0.5);
        assert_eq!(app.focused(), Some(app.id(&["chips", chip])), "{chip}");
        let r = chip_rects(&app)[i];
        assert!(r.x >= theme::GUTTER - 0.01 && r.right() <= right + 0.01, "{chip} at {r:?}");
    }
    // Back at the first, the row is where it started.
    press(&mut app, &[Key::Left; 6]);
    app.advance(0.5);
    assert_eq!(app.focused(), Some(app.id(&["chips", "sort"])));
    assert_eq!(chip_rects(&app)[0].x, theme::GUTTER);
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

#[test]
fn a_librarys_collections_show_by_category_and_filter_by_category() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: Some(LibraryTab::Collections), sort: None });
    // A headed row for each category in the categories' order, the server's Other among them.
    assert!(app.until(|a| ["Highlights", "Other", "Box sets"].iter().all(|h| a.shows(h))));
    assert!(app.shows("26 collections"));
    assert!(!app.shows("Type") && !app.shows("All types"), "the apps don't filter by how a collection was made");
    let rows = app.id(&["collections-grid"]);
    let first = rows.with(ids::CATEGORY_HIGHLIGHTS as u64).with("row").with(ids::SMART_COLLECTION as u64);
    assert!(app.until(|a| a.focused() == Some(first)), "the first category's first collection takes focus");
    press(&mut app, &[Key::Down; 3]);
    assert!(app.until(|a| a.shows("Seasonal")), "no Seasonal row last");
    press(&mut app, &[Key::Up; 3]);
    assert_eq!(app.focused(), Some(first));
    // Up to the one chip, which keeps focus left and right; up again to the tabs, and down back.
    let chip = app.id(&["collection-chips", "category"]);
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(chip));
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(chip));
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "collections"])));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(chip));
    // The panel lists the categories in the server's order, Other where it is.
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("All categories")), "no Category panel");
    let at = |t: &str| app.list.texts().collect::<Vec<_>>().iter().rposition(|x| *x == t);
    let order = ["All categories", "Highlights", "Other", "Box sets", "Seasonal"].map(at);
    assert!(order.iter().all(Option::is_some) && order.is_sorted(), "{order:?}");
    // Seasonal: one group left, a grid without headings, the chip named for it.
    press(&mut app, &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("7 collections") && !a.shows("Box sets")), "Seasonal didn't filter");
    assert!(app.shows("Holiday Favorites") && !app.shows("Highlights"), "one group has no heading");
    assert_eq!(app.focused(), Some(chip));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(app.id(&["collections-grid"]).with(0u64)), "the grid is under the chip");
    // All categories shows them all again.
    app.press(Key::Up);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("All categories")));
    press(&mut app, &[Key::Up; 3]);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Highlights") && a.shows("Box sets")), "the filter wasn't cleared");
}

/// The shows library's own collections: four of Other, then Holiday Favorites in Seasonal.
const SHOWS_OTHER: [i64; 4] = [ids::COLLECTION + 24, ids::COLLECTION + 25, ids::COLLECTION + 26, ids::COLLECTION + 27];

#[test]
fn clearing_a_category_that_was_the_last_to_choose_from_moves_focus_to_the_collections() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::SHOWS, tab: Some(LibraryTab::Collections), sort: None });
    assert!(app.until(|a| a.shows("Seasonal") && a.shows("Other")));
    let chip = app.id(&["collection-chips", "category"]);
    app.ui.request_focus(chip);
    app.advance(0.2);
    // Seasonal; then the server's other collections go, so Seasonal is the only category left.
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("All categories")));
    press(&mut app, &[Key::Down, Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("1 collection") && !a.shows("Other")));
    assert_eq!(app.focused(), Some(chip));
    app.server.hide_collections(&SHOWS_OTHER);
    app.server.emit(ServerEvent::LibrariesChanged);
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("All categories") && !a.shows("Other")), "the list didn't change");
    // All categories: nothing left to choose between, so the chip goes and focus moves on.
    press(&mut app, &[Key::Up, Key::Ok]);
    app.advance(0.5);
    assert!(app.until(|a| !a.shows("All categories")));
    let grid = app.id(&["collections-grid"]);
    assert_eq!(app.focused(), Some(grid.with(0u64)), "focus was lost with the chip");
}

#[test]
fn a_chosen_category_emptied_by_the_server_moves_focus_from_its_chip() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::SHOWS, tab: Some(LibraryTab::Collections), sort: None });
    assert!(app.until(|a| a.shows("Seasonal") && a.shows("Other")));
    let chip = app.id(&["collection-chips", "category"]);
    app.ui.request_focus(chip);
    app.advance(0.2);
    // Other, then the server takes its collections away: the filter goes, and the chip with it.
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("All categories")));
    press(&mut app, &[Key::Down, Key::Ok]);
    assert!(app.until(|a| a.shows("4 collections")));
    assert_eq!(app.focused(), Some(chip));
    app.server.hide_collections(&SHOWS_OTHER);
    app.server.emit(ServerEvent::LibrariesChanged);
    assert!(app.until(|a| a.shows("1 collection") && a.shows("Holiday Favorites")));
    app.advance(0.5);
    let grid = app.id(&["collections-grid"]);
    assert_eq!(app.focused(), Some(grid.with(0u64)), "focus was lost with the chip");
}

#[test]
fn the_merged_page_shows_both_libraries_and_opens_each() {
    let mut app = TestApp::start();
    app.open(Route::MergedCollection { ids: vec![ids::HOLIDAY_MOVIES, ids::HOLIDAY_SHOWS] });
    assert!(app.until(|a| a.shows("Holiday Favorites") && a.shows("Movies") && a.shows("TV Shows")));
    assert!(app.shows_part("4 movies") && app.shows_part("4 shows"), "the facts count both");
    let play = app.id(&["buttons", "play"]);
    assert!(app.until(|a| a.focused() == Some(play)), "Play takes focus");
    // Play, Shuffle, then each library's own collection.
    press(&mut app, &[Key::Right, Key::Right, Key::Right, Key::Ok]);
    assert_eq!(app.route(), Some(&Route::Collection { id: ids::HOLIDAY_SHOWS }));
    assert!(app.until(|a| a.shows("Seasonal")), "a collection's page names its category");
}

#[test]
fn the_merged_pages_play_queues_every_librarys_collection() {
    let mut app = TestApp::start();
    app.open(Route::MergedCollection { ids: vec![ids::HOLIDAY_MOVIES, ids::HOLIDAY_SHOWS] });
    let play = app.id(&["buttons", "play"]);
    assert!(app.until(|a| a.focused() == Some(play)));
    app.press(Key::Ok);
    // A movie, with Next going on through the whole page rather than its own library's collection.
    let queue = Some(PlayQueue::Collections(vec![ids::HOLIDAY_MOVIES, ids::HOLIDAY_SHOWS]));
    assert!(matches!(app.route(), Some(Route::Player(r)) if r.queue == queue), "{:?}", app.route());
}

#[test]
fn a_recommended_rows_see_all_opens_its_collection() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: Some(LibraryTab::Recommended), sort: None });
    assert!(app.until(|a| a.shows("Recently Added")));
    // The row of a collection, the last, ends with See all, which opens the collection.
    let rows = app.id(&["rows"]);
    let key = format!("collection:{}", ids::SMART_COLLECTION);
    let see_all: Vec<Id> = (0..8u64).map(|i| rows.with(i).with(key.as_str()).with(u64::MAX)).collect();
    press(&mut app, &[Key::Down; 8]);
    press(&mut app, &[Key::Right; 40]);
    assert!(app.focused().is_some_and(|f| see_all.contains(&f)), "no See all on the collection's row");
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Collection { id: ids::SMART_COLLECTION }));
}

#[test]
fn a_held_up_stays_in_the_collections_rows_and_a_press_reaches_the_chip() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: Some(LibraryTab::Collections), sort: None });
    assert!(app.until(|a| a.shows("Highlights") && a.shows("Box sets")));
    let rows = app.id(&["collections-grid"]);
    let first = rows.with(ids::CATEGORY_HIGHLIGHTS as u64).with("row").with(ids::SMART_COLLECTION as u64);
    assert!(app.until(|a| a.focused() == Some(first)));
    press(&mut app, &[Key::Down, Key::Down]);
    assert_ne!(app.focused(), Some(first));
    // Held, Up glides back to the first row and stays there, as in a library's grid.
    app.hold(Key::Up, 1.5);
    assert_eq!(app.focused(), Some(first), "a held Up left the rows");
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["collection-chips", "category"])));
}

#[test]
fn down_from_the_tabs_while_the_collections_come_ends_on_the_first_collection() {
    let mut app = TestApp::start();
    app.open(Route::Library { id: ids::MOVIES, tab: Some(LibraryTab::Recommended), sort: None });
    assert!(app.until(|a| a.shows("Recently Added")));
    let tab = app.id(&["tabs", "recommended"]);
    app.ui.request_focus(tab);
    app.advance(0.2);
    // The Collections tab while the server is slow: Down reaches a placeholder of the grid.
    app.server.set_mode(Mode::Slow).unwrap();
    press(&mut app, &[Key::Right, Key::Right, Key::Ok]);
    app.press(Key::Down);
    assert!(!app.shows("Highlights"), "the collections came before Down");
    assert_ne!(app.focused(), Some(app.id(&["tabs", "collections"])), "Down stayed on the tabs");
    app.server.set_mode(Mode::Normal).unwrap();
    // The rows come: the placeholder is gone, and focus goes to the first category's first
    // collection rather than whichever tile was nearest.
    assert!(app.until(|a| a.shows("Highlights") && a.shows("Box sets")));
    app.advance(0.5);
    let rows = app.id(&["collections-grid"]);
    let first = rows.with(ids::CATEGORY_HIGHLIGHTS as u64).with("row").with(ids::SMART_COLLECTION as u64);
    assert_eq!(app.focused(), Some(first));
}

#[test]
fn a_server_before_categories_shows_every_collection_in_one_grid() {
    let mut app = TestApp::start();
    app.server.set_before_categories();
    app.open(Route::Library { id: ids::MOVIES, tab: Some(LibraryTab::Collections), sort: None });
    // The categories' 404 doesn't hold the tab up: every collection in one grid, no headings and
    // no Category chip.
    assert!(app.until(|a| a.shows("26 collections")));
    assert!(!app.shows("Other") && !app.shows("Highlights") && !app.shows("Category"));
    let grid = app.id(&["collections-grid"]);
    assert!(app.until(|a| a.focused() == Some(grid.with(0u64))), "the first collection takes focus");
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["tabs", "collections"])), "something stands between the grid and the tabs");
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(grid.with(0u64)));
}

#[test]
fn a_collections_page_names_its_category_but_never_other() {
    let mut app = TestApp::start();
    // One of the shows library's collections without a category: the server sends it as Other.
    app.open(Route::Collection { id: SHOWS_OTHER[0] });
    assert!(app.until(|a| a.shows("Collection") && a.shows("Play") && a.shows_part("TV Shows")));
    assert!(!app.shows("Other"), "the page named Other, which only says it has no category");
    app.open(Route::Collection { id: ids::HOLIDAY_SHOWS });
    assert!(app.until(|a| a.shows("Seasonal")), "a real category isn't named");
}
