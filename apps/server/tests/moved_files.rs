//! Files renamed, moved, replaced and deleted (docs/FEATURES.md,
//! Libraries): a renamed or moved file keeps what people did with its
//! title, a movie or episode replaced by a copy named differently or added
//! again gets back what people watched of it, and a file deleted for good
//! still takes its title away.

mod common;

use std::path::Path;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{TestApp, titles};

/// A file whose bytes are its own (`seed`): a renamed file is found by its
/// size and a checksum of its bytes, which tell it from the others.
fn file_of_its_own(app: &TestApp, rel: &str, seed: u8, bytes: usize) {
    let path = app.media.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content: Vec<u8> = (0..bytes).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect();
    std::fs::write(path, content).unwrap();
}

fn rename(app: &TestApp, from: &str, to: &str) {
    let to = app.media.path().join(to);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::rename(app.media.path().join(from), to).unwrap();
}

fn remove(app: &TestApp, rel: &str) {
    let path = app.media.path().join(rel);
    if Path::new(&path).is_dir() {
        std::fs::remove_dir_all(path).unwrap();
    } else {
        std::fs::remove_file(path).unwrap();
    }
}

/// A library of the media folder `rel`, scanned, its files fingerprinted as
/// they are in the background after every scan.
async fn library(app: &TestApp, token: &str, name: &str, kind: &str, rel: &str) -> i64 {
    let id = app.library(token, name, kind, rel).await;
    app.state.store.fingerprint_pending().await;
    id
}

/// Scans `library` again, and fingerprints what it found.
async fn rescan(app: &TestApp, token: &str, library: i64) {
    let reply = app.post(&format!("/api/libraries/{library}/scan"), token, json!({})).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    app.wait_for_scans().await;
    app.state.store.fingerprint_pending().await;
}

async fn progress(app: &TestApp, token: &str, item: i64, position_ms: i64) {
    let body = json!({"position_ms": position_ms, "duration_ms": 100_000});
    let reply = app.post(&format!("/api/items/{item}/progress"), token, body).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.text());
}

async fn rate(app: &TestApp, token: &str, item: i64, rating: f64) {
    let reply = app.put(&format!("/api/items/{item}/rating"), token, json!({"rating": rating})).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.text());
}

async fn user_state(app: &TestApp, token: &str, item: i64) -> Value {
    let item = app.get(&format!("/api/items/{item}"), token).await;
    assert_eq!(item.status, StatusCode::OK, "{}", item.text());
    item.body["user_state"].clone()
}

/// `(position_ms, watched, rating)` of `item` for `token`'s user.
async fn watched(app: &TestApp, token: &str, item: i64) -> (Value, Value, Value) {
    let state = user_state(app, token, item).await;
    (state["position_ms"].clone(), state["watched"].clone(), state["rating"].clone())
}

fn hub(hubs: &Value, key: &str) -> Vec<String> {
    hubs["hubs"].as_array().unwrap().iter().find(|h| h["key"] == key).map(|h| titles(&h["items"])).unwrap_or_default()
}

async fn continue_watching(app: &TestApp, token: &str) -> Vec<String> {
    hub(&app.get("/api/hubs/home", token).await.body, "continue_watching")
}

async fn my_list(app: &TestApp, token: &str) -> Vec<String> {
    titles(&app.get("/api/watchlist", token).await.body["items"])
}

/// The titles of `token`'s user's plays, sorted.
async fn history(app: &TestApp, token: &str) -> Vec<String> {
    let page = app.get("/api/history", token).await;
    let entries = page.body["entries"].as_array().unwrap();
    let mut titles: Vec<String> = entries.iter().map(|e| e["item"]["title"].as_str().unwrap().to_owned()).collect();
    titles.sort();
    titles
}

async fn added_at(app: &TestApp, item: i64) -> i64 {
    sqlx::query_scalar("SELECT added_at FROM items WHERE id = ?")
        .bind(item)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

fn sorted(ids: &std::collections::HashMap<String, i64>) -> Vec<String> {
    let mut titles: Vec<String> = ids.keys().cloned().collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn renamed_and_moved_movies_keep_what_people_did_with_them() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    file_of_its_own(&app, "movies/Fenwik Hollow (2001)/Fenwik Hollow (2001).mkv", 1, 4096);
    file_of_its_own(&app, "movies/Bramblecot.mkv", 2, 4096);
    file_of_its_own(&app, "movies/Quillon Night (1998).mkv", 3, 4096);
    let lib = library(&app, &admin, "Movies", "movies", "movies").await;
    let ids = app.ids(lib, &admin).await;
    let (hollow, bramblecot, night) = (ids["Fenwik Hollow"], ids["Bramblecot"], ids["Quillon Night"]);
    let hollow_file = app.file_of(hollow).await;
    let hollow_added = added_at(&app, hollow).await;

    // Half of one seen, by two people, and on My List; another watched to
    // the end (a play in history) and rated; the third watched too.
    progress(&app, &admin, hollow, 50_000).await;
    progress(&app, &bob, hollow, 30_000).await;
    assert_eq!(app.put(&format!("/api/watchlist/{hollow}"), &admin, json!({})).await.status, StatusCode::NO_CONTENT);
    progress(&app, &admin, bramblecot, 95_000).await;
    rate(&app, &admin, bramblecot, 8.0).await;
    progress(&app, &admin, night, 95_000).await;

    // The typo in the first's folder and file fixed, the year added to the
    // second's name, the third deleted.
    rename(&app, "movies/Fenwik Hollow (2001)", "movies/Fenwick Hollow (2001)");
    rename(
        &app,
        "movies/Fenwick Hollow (2001)/Fenwik Hollow (2001).mkv",
        "movies/Fenwick Hollow (2001)/Fenwick Hollow (2001).mkv",
    );
    rename(&app, "movies/Bramblecot.mkv", "movies/Bramblecot (2004).mkv");
    remove(&app, "movies/Quillon Night (1998).mkv");
    rescan(&app, &admin, lib).await;

    let ids = app.ids(lib, &admin).await;
    assert_eq!(sorted(&ids), ["Bramblecot", "Fenwick Hollow"]);
    let (hollow, bramblecot) = (ids["Fenwick Hollow"], ids["Bramblecot"]);
    assert_eq!(watched(&app, &admin, hollow).await, (json!(50_000), json!(false), Value::Null));
    assert_eq!(watched(&app, &bob, hollow).await, (json!(30_000), json!(false), Value::Null));
    assert_eq!(watched(&app, &admin, bramblecot).await, (json!(0), json!(true), json!(8.0)));
    assert_eq!(user_state(&app, &admin, bramblecot).await["play_count"], json!(1));
    assert_eq!(my_list(&app, &admin).await, ["Fenwick Hollow"]);
    assert_eq!(continue_watching(&app, &admin).await, ["Fenwick Hollow"]);
    // The deleted film's play went with it.
    assert_eq!(history(&app, &admin).await, ["Bramblecot"]);
    // The file is the one it was (with the subtitles people added to it),
    // and the film no newer than it was.
    assert_eq!(app.file_of(hollow).await, hollow_file);
    assert_eq!(added_at(&app, hollow).await, hollow_added);
    assert_eq!(app.get(&format!("/api/items/{night}"), &admin).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn episodes_moved_to_another_show_folder_keep_their_state() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (n, name) in [(1, "Arrival"), (2, "Departure"), (3, "Return")] {
        let rel = format!("tv/Quillon Street/Season 01/Quillon Street - S01E0{n} - {name}.mkv");
        file_of_its_own(&app, &rel, 10 + n, 4096);
    }
    let tv = library(&app, &admin, "TV", "shows", "tv").await;
    let show = app.ids(tv, &admin).await["Quillon Street"];
    let season = app.children(show, &admin).await["Season 1"];
    let episodes = app.children(season, &admin).await;
    progress(&app, &admin, episodes["Arrival"], 95_000).await;
    progress(&app, &admin, episodes["Departure"], 40_000).await;
    assert_eq!(app.put(&format!("/api/watchlist/{show}"), &admin, json!({})).await.status, StatusCode::NO_CONTENT);
    rate(&app, &admin, show, 6.0).await;

    // The show's folder named with its year: another show to the scanner,
    // with seasons and episodes of its own.
    rename(&app, "tv/Quillon Street", "tv/Quillon Street (2015)");
    rescan(&app, &admin, tv).await;

    let shows = app.ids(tv, &admin).await;
    assert_eq!(sorted(&shows), ["Quillon Street"]);
    let moved = shows["Quillon Street"];
    assert_ne!(moved, show);
    let season = app.children(moved, &admin).await["Season 1"];
    let episodes = app.children(season, &admin).await;
    assert_eq!(watched(&app, &admin, episodes["Arrival"]).await, (json!(0), json!(true), Value::Null));
    assert_eq!(watched(&app, &admin, episodes["Departure"]).await, (json!(40_000), json!(false), Value::Null));
    assert_eq!(watched(&app, &admin, episodes["Return"]).await, (json!(0), json!(false), Value::Null));
    assert_eq!(user_state(&app, &admin, moved).await["rating"], json!(6.0));
    assert_eq!(my_list(&app, &admin).await, ["Quillon Street"]);
    assert_eq!(continue_watching(&app, &admin).await, ["Departure"]);
    let on_deck = app.get(&format!("/api/items/{moved}/on-deck"), &admin).await;
    assert_eq!(on_deck.body["item"]["title"], json!("Departure"), "{}", on_deck.text());
    assert_eq!(history(&app, &admin).await, ["Arrival"]);
}

#[tokio::test]
async fn a_title_replaced_or_added_again_gets_back_what_was_watched_of_it() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    file_of_its_own(&app, "movies/Fenwick Hollow (2001)/Fenwick Hollow (2001).mkv", 1, 4096);
    // Another film, so that the folder is never empty (as a drive not
    // mounted leaves it, whose films are kept).
    file_of_its_own(&app, "movies/Bramblecot (2004).mkv", 2, 1024);
    for n in [1, 2] {
        file_of_its_own(&app, &format!("tv/Quillon Street {{tmdb-77}}/Season 01/S01E0{n}.mkv"), 10 + n, 4096);
    }
    let movies = library(&app, &admin, "Movies", "movies", "movies").await;
    let tv = library(&app, &admin, "TV", "shows", "tv").await;
    let hollow = app.ids(movies, &admin).await["Fenwick Hollow"];
    // As the metadata agent would have matched it.
    sqlx::query("UPDATE items SET tmdb_id = 9001 WHERE id = ?")
        .bind(hollow)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    progress(&app, &admin, hollow, 95_000).await;
    rate(&app, &admin, hollow, 7.0).await;
    let show = app.ids(tv, &admin).await["Quillon Street"];
    let season = app.children(show, &admin).await["Season 1"];
    let pilot = app.children(season, &admin).await["Episode 1"];
    progress(&app, &admin, pilot, 95_000).await;

    // A better copy of the film, named after its TMDB id, in place of the
    // first: another file, and another title to the scanner.
    remove(&app, "movies/Fenwick Hollow (2001)/Fenwick Hollow (2001).mkv");
    file_of_its_own(&app, "movies/Fenwick Hollow (2001)/Fenwick Hollow (2001) {tmdb-9001}.mkv", 5, 8192);
    rescan(&app, &admin, movies).await;
    let ids = app.ids(movies, &admin).await;
    assert_eq!(sorted(&ids), ["Bramblecot", "Fenwick Hollow"]);
    let copy = ids["Fenwick Hollow"];
    assert_ne!(copy, hollow);
    assert_eq!(watched(&app, &admin, copy).await, (json!(0), json!(true), json!(7.0)));
    assert_eq!(history(&app, &admin).await, ["Episode 1", "Fenwick Hollow"]);

    // Deleted for good: gone from the library, and from history.
    remove(&app, "movies/Fenwick Hollow (2001)");
    rescan(&app, &admin, movies).await;
    assert_eq!(sorted(&app.ids(movies, &admin).await), ["Bramblecot"]);
    assert_eq!(history(&app, &admin).await, ["Episode 1"]);
    // Added again later, it is still watched.
    file_of_its_own(&app, "movies/Fenwick Hollow {tmdb-9001}.mkv", 6, 2048);
    rescan(&app, &admin, movies).await;
    let again = app.ids(movies, &admin).await["Fenwick Hollow"];
    assert_eq!(watched(&app, &admin, again).await, (json!(0), json!(true), json!(7.0)));
    assert_eq!(history(&app, &admin).await, ["Episode 1", "Fenwick Hollow"]);

    // An episode deleted and added again, as another file.
    remove(&app, "tv/Quillon Street {tmdb-77}/Season 01/S01E01.mkv");
    rescan(&app, &admin, tv).await;
    let season = app.children(show, &admin).await["Season 1"];
    assert_eq!(sorted(&app.children(season, &admin).await), ["Episode 2"]);
    file_of_its_own(&app, "tv/Quillon Street {tmdb-77}/Season 01/Quillon Street - S01E01.mkv", 20, 8192);
    rescan(&app, &admin, tv).await;
    let pilot = app.children(season, &admin).await["Episode 1"];
    assert_eq!(watched(&app, &admin, pilot).await, (json!(0), json!(true), Value::Null));

    // What nobody claims is forgotten after a while.
    remove(&app, "movies/Fenwick Hollow {tmdb-9001}.mkv");
    rescan(&app, &admin, movies).await;
    sqlx::query("UPDATE gone_items SET gone_at = gone_at - ?")
        .bind(pley::library::gone::KEPT_FOR + 1)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    file_of_its_own(&app, "movies/Fenwick Hollow {tmdb-9001}.mkv", 6, 2048);
    rescan(&app, &admin, movies).await;
    let again = app.ids(movies, &admin).await["Fenwick Hollow"];
    assert_eq!(watched(&app, &admin, again).await, (json!(0), json!(false), Value::Null));
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gone_items").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(kept, 0);
}

#[tokio::test]
async fn files_alike_but_for_their_bytes_are_not_taken_for_one_moved() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    file_of_its_own(&app, "movies/Bramblecot (2004).mkv", 1, 4096);
    let lib = library(&app, &admin, "Movies", "movies", "movies").await;
    let bramblecot = app.ids(lib, &admin).await["Bramblecot"];
    progress(&app, &admin, bramblecot, 50_000).await;

    // Another film of the same size comes as the first goes.
    remove(&app, "movies/Bramblecot (2004).mkv");
    file_of_its_own(&app, "movies/Quillon Night (1998).mkv", 2, 4096);
    rescan(&app, &admin, lib).await;
    let ids = app.ids(lib, &admin).await;
    assert_eq!(sorted(&ids), ["Quillon Night"]);
    assert_eq!(watched(&app, &admin, ids["Quillon Night"]).await, (json!(0), json!(false), Value::Null));
    assert!(continue_watching(&app, &admin).await.is_empty());
}

#[tokio::test]
async fn copies_alike_are_not_told_apart() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    // Two files of the same bytes (as placeholders, or tracks of silence).
    file_of_its_own(&app, "movies/Fenwick Hollow.mkv", 1, 4096);
    file_of_its_own(&app, "movies/Bramblecot.mkv", 1, 4096);
    file_of_its_own(&app, "movies/Quillon Night (1998).mkv", 3, 4096);
    let lib = library(&app, &admin, "Movies", "movies", "movies").await;
    let hollow = app.ids(lib, &admin).await["Fenwick Hollow"];
    progress(&app, &admin, hollow, 50_000).await;

    // Both renamed: which is which can't be told, and neither film takes
    // the other's place.
    rename(&app, "movies/Fenwick Hollow.mkv", "movies/Fenwick Hollow (2001).mkv");
    rename(&app, "movies/Bramblecot.mkv", "movies/Bramblecot (2004).mkv");
    rescan(&app, &admin, lib).await;
    let ids = app.ids(lib, &admin).await;
    assert_eq!(sorted(&ids), ["Bramblecot", "Fenwick Hollow", "Quillon Night"]);
    for title in ["Bramblecot", "Fenwick Hollow"] {
        assert_eq!(watched(&app, &admin, ids[title]).await, (json!(0), json!(false), Value::Null), "{title}");
    }
}

#[tokio::test]
async fn a_scan_stopped_before_its_end_finishes_the_move_next_time() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    file_of_its_own(&app, "movies/Fenwik Hollow (2001).mkv", 1, 4096);
    file_of_its_own(&app, "movies/Bramblecot (2004).mkv", 2, 4096);
    let lib = library(&app, &admin, "Movies", "movies", "movies").await;
    let hollow = app.ids(lib, &admin).await["Fenwik Hollow"];
    progress(&app, &admin, hollow, 50_000).await;
    assert_eq!(app.put(&format!("/api/watchlist/{hollow}"), &admin, json!({})).await.status, StatusCode::NO_CONTENT);

    // The scan that finds the file renamed stops after writing it, before
    // the end that removes what went (the server stopped, a full disk).
    let db = &app.state.db.write;
    sqlx::query("CREATE TRIGGER stopped BEFORE DELETE ON items BEGIN SELECT RAISE(ABORT, 'stopped'); END")
        .execute(db)
        .await
        .unwrap();
    rename(&app, "movies/Fenwik Hollow (2001).mkv", "movies/Fenwick Hollow (2001).mkv");
    rescan(&app, &admin, lib).await;
    sqlx::query("DROP TRIGGER stopped").execute(db).await.unwrap();

    rescan(&app, &admin, lib).await;
    let ids = app.ids(lib, &admin).await;
    assert_eq!(sorted(&ids), ["Bramblecot", "Fenwick Hollow"]);
    assert_eq!(watched(&app, &admin, ids["Fenwick Hollow"]).await, (json!(50_000), json!(false), Value::Null));
    assert_eq!(my_list(&app, &admin).await, ["Fenwick Hollow"]);
}

#[tokio::test]
async fn a_hand_match_and_what_probing_found_go_with_a_renamed_file() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    file_of_its_own(&app, "movies/Fenwik Hollow.mkv", 1, 4096);
    file_of_its_own(&app, "movies/Bramblecot (2004).mkv", 2, 4096);
    let lib = library(&app, &admin, "Movies", "movies", "movies").await;
    let hollow = app.ids(lib, &admin).await["Fenwik Hollow"];
    let file = app.file_of(hollow).await;
    // Matched by hand (Fix Match), and probed while the transcoder ran.
    let db = &app.state.db.write;
    sqlx::query("UPDATE items SET tmdb_id = 4242, imdb_id = 'tt4242424', metadata_at = 1 WHERE id = ?")
        .bind(hollow)
        .execute(db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO item_overrides (item_id, field, value) VALUES (?, 'match', '4242')")
        .bind(hollow)
        .execute(db)
        .await
        .unwrap();
    let info = r#"{"container":"mkv","format_name":"matroska,webm","duration_ms":4000,"bitrate":8192,"streams":[]}"#;
    sqlx::query(
        "UPDATE media_files SET media_info = ?, duration_ms = 4000, container = 'mkv', probed = 1 WHERE id = ?",
    )
    .bind(info)
    .bind(file)
    .execute(db)
    .await
    .unwrap();

    // Renamed while the transcoder is down (none runs in tests).
    rename(&app, "movies/Fenwik Hollow.mkv", "movies/Fenwick Hollow (2001).mkv");
    rescan(&app, &admin, lib).await;
    let renamed = app.ids(lib, &admin).await["Fenwick Hollow"];
    let matched: (Option<i64>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT i.tmdb_id, i.imdb_id, o.value FROM items i
         LEFT JOIN item_overrides o ON o.item_id = i.id AND o.field = 'match' WHERE i.id = ?",
    )
    .bind(renamed)
    .fetch_one(&app.state.db.read)
    .await
    .unwrap();
    assert_eq!(matched, (Some(4242), Some("tt4242424".to_owned()), Some("4242".to_owned())));
    let probed: (i64, Option<String>, bool) =
        sqlx::query_as("SELECT id, media_info, probed FROM media_files WHERE item_id = ?")
            .bind(renamed)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(probed.0, file);
    assert!(probed.1.is_some_and(|info| info.contains("\"duration_ms\":4000")), "what probing found stays");
    assert!(probed.2);
}

#[tokio::test]
async fn a_title_with_an_empty_imdb_id_gives_its_state_to_none() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    file_of_its_own(&app, "movies/Fenwick Hollow (2001).mkv", 1, 4096);
    file_of_its_own(&app, "movies/Bramblecot (2004).mkv", 2, 4096);
    let lib = library(&app, &admin, "Movies", "movies", "movies").await;
    let ids = app.ids(lib, &admin).await;
    // As a metadata source may leave them.
    sqlx::query("UPDATE items SET imdb_id = '' WHERE kind = 'movie'").execute(&app.state.db.write).await.unwrap();
    progress(&app, &admin, ids["Fenwick Hollow"], 95_000).await;

    remove(&app, "movies/Fenwick Hollow (2001).mkv");
    rescan(&app, &admin, lib).await;
    assert_eq!(sorted(&app.ids(lib, &admin).await), ["Bramblecot"]);
    assert_eq!(watched(&app, &admin, ids["Bramblecot"]).await, (json!(0), json!(false), Value::Null));
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gone_items").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(kept, 0);
}

#[tokio::test]
async fn tracks_moved_to_another_folder_keep_their_plays() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (n, name) in [(1, "First Light"), (2, "Second Wind")] {
        file_of_its_own(&app, &format!("music/Pale Orchard/Lantern Songs/0{n} - {name}.flac"), 30 + n, 4096);
    }
    let lib = library(&app, &admin, "Music", "music", "music").await;
    let artist = app.ids(lib, &admin).await["Pale Orchard"];
    let album = app.children(artist, &admin).await["Lantern Songs"];
    let first = app.children(album, &admin).await["First Light"];
    progress(&app, &admin, first, 95_000).await;
    rate(&app, &admin, first, 10.0).await;

    rename(&app, "music/Pale Orchard/Lantern Songs", "music/Pale Orchard/Lantern Songs (2019)");
    rescan(&app, &admin, lib).await;
    let artist = app.ids(lib, &admin).await["Pale Orchard"];
    let albums = app.children(artist, &admin).await;
    assert_eq!(albums.len(), 1, "{albums:?}");
    let album = albums.into_values().next().unwrap();
    let first = app.children(album, &admin).await["First Light"];
    assert_eq!(watched(&app, &admin, first).await, (json!(0), json!(true), json!(10.0)));
    assert_eq!(user_state(&app, &admin, first).await["play_count"], json!(1));
}
