//! The library, collection and playlist pages driven headless against the fake server: what takes
//! focus, what the keys do, and what reaches the server.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::{NewPlaylist, PlaylistKind};
use pley_fixtures::ids;
use pley_shell::Route;
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
