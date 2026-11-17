//! Audiobook libraries: books found in folders and `.m4b` files, their
//! parts, where Play takes up a book, and Continue Listening.

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use pley_transcoder_api::{
    API_VERSION, Capabilities, MediaInfo, MediaStream, ProbeRequest, StreamKind, TranscoderInfo,
};
use serde_json::{Value, json};

use common::{TestApp, titles};

/// A transcoder stand-in whose probe reads a file's tags from the file
/// itself, where [`tagged`] writes them: other files have none.
async fn start_transcoder() -> String {
    let app = Router::new()
        .route(
            "/v1/info",
            get(|| async {
                Json(TranscoderInfo {
                    api_version: API_VERSION,
                    capabilities: Capabilities { ffmpeg_available: true, ..Default::default() },
                    ..Default::default()
                })
            }),
        )
        .route(
            "/v1/probe",
            post(|Json(req): Json<ProbeRequest>| async move {
                let tags = std::fs::read(&req.path).ok().and_then(|b| serde_json::from_slice(&b).ok());
                Json(MediaInfo {
                    container: Some("mp4".into()),
                    duration_ms: Some(60_000),
                    streams: vec![MediaStream {
                        index: 0,
                        kind: StreamKind::Audio,
                        codec: "aac".into(),
                        default: true,
                        ..Default::default()
                    }],
                    tags: tags.unwrap_or_default(),
                    ..Default::default()
                })
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// A Pley whose files' tags are read by [`start_transcoder`], and its
/// administrator's token.
async fn tagged_app() -> (TestApp, String) {
    let base = start_transcoder().await;
    let app = TestApp::with_config(|c| c.transcoder_url = Some(base)).await;
    app.state.transcoder.start();
    assert!(app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    let token = app.setup_admin().await;
    (app, token)
}

/// An audio file whose tags are `tags`.
fn tagged(app: &TestApp, rel: &str, tags: Value) {
    let path = app.media.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, tags.to_string()).unwrap();
}

/// Scans a library again, and waits for the scan to finish.
async fn rescan(app: &TestApp, token: &str, lib: i64) {
    let r = app.post(&format!("/api/libraries/{lib}/scan"), token, json!({})).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    app.wait_for_scans().await;
}

/// A library's books by title: their writers, their series, and their
/// parts in order.
async fn shelf(app: &TestApp, token: &str, lib: i64) -> BTreeMap<String, (Value, Value, Vec<String>)> {
    let mut books = BTreeMap::new();
    for (title, id) in app.ids(lib, token).await {
        let book = app.get(&format!("/api/items/{id}"), token).await.body["book"].clone();
        let parts = app.get(&format!("/api/items/{id}/children"), token).await;
        books.insert(title, (book["authors"].clone(), book["series"].clone(), titles(&parts.body["items"])));
    }
    books
}

/// A library of two books: one in a folder of two parts, in its author's
/// series, and one `.m4b` file.
async fn books() -> (TestApp, String, i64) {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    let book = "books/Author A/Series X/Book 1 - The Long Road (2006)";
    app.media_file(&format!("{book}/01 - Prologue.mp3"), 512);
    app.media_file(&format!("{book}/02 - Chapter One.mp3"), 512);
    app.media_file("books/Author B/Middle Book.m4b", 512);
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;
    (app, token, lib)
}

async fn progress(app: &TestApp, token: &str, part: i64, position_ms: i64) {
    let r = app
        .post(
            &format!("/api/items/{part}/progress"),
            token,
            json!({"position_ms": position_ms, "duration_ms": 3_600_000, "state": "stopped"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.body);
}

async fn on_deck(app: &TestApp, token: &str, book: i64) -> Value {
    let deck = app.get(&format!("/api/items/{book}/on-deck"), token).await;
    assert_eq!(deck.status, StatusCode::OK);
    deck.body["item"].clone()
}

async fn hub(app: &TestApp, token: &str, uri: &str, key: &str) -> Vec<String> {
    let hubs = app.get(uri, token).await;
    assert_eq!(hubs.status, StatusCode::OK, "{:?}", hubs.body);
    hubs.body["hubs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["key"].as_str().is_some_and(|k| k.starts_with(key)))
        .map(|h| titles(&h["items"]))
        .unwrap_or_default()
}

/// Books are the library's titles, with their writers; a book's page says
/// who wrote it and the series it's in, and lists its parts in order.
#[tokio::test]
async fn books_and_their_parts() {
    let (app, token, lib) = books().await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &token).await;
    assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
    let items = page.body["items"].as_array().unwrap();
    // By title, "The" left out.
    assert_eq!(titles(&page.body["items"]), ["The Long Road", "Middle Book"]);
    assert!(items.iter().all(|i| i["kind"] == "audiobook"), "{items:?}");
    assert_eq!(items[0]["authors"], json!(["Author A"]));
    assert_eq!(items[0]["year"], 2006);
    assert_eq!(items[1]["authors"], json!(["Author B"]));

    let ids = app.ids(lib, &token).await;
    let road = ids["The Long Road"];
    let detail = app.get(&format!("/api/items/{road}"), &token).await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(
        detail.body["book"],
        json!({"authors": ["Author A"], "narrators": [], "series": "Series X", "series_index": 1.0})
    );
    let parts = app.get(&format!("/api/items/{road}/children"), &token).await;
    assert_eq!(titles(&parts.body["items"]), ["Prologue", "Chapter One"]);
    assert!(parts.body["items"].as_array().unwrap().iter().all(|p| p["kind"] == "part"));
    assert_eq!(parts.body["items"][0]["parent_title"], "The Long Road");

    // The single file is a book of one part, named as it is.
    let middle = ids["Middle Book"];
    let parts = app.get(&format!("/api/items/{middle}/children"), &token).await;
    assert_eq!(titles(&parts.body["items"]), ["Middle Book"]);
    let detail = app.get(&format!("/api/items/{middle}"), &token).await;
    assert_eq!(detail.body["book"]["series"], Value::Null);
    // Movies and shows have no book.
    assert_eq!(
        app.get(&format!("/api/items/{}", parts.body["items"][0]["id"]), &token).await.body["book"],
        Value::Null
    );
}

/// Play on a book takes it up in the part listened to last, where it was
/// left; a part finishes only near its end, not at 90 % as a film does,
/// and the next part comes after it. A book being listened to is in
/// Continue Listening, on Home and in its library, until it is finished.
#[tokio::test]
async fn play_takes_up_a_book_where_it_was_left() {
    let (app, token, lib) = books().await;
    let road = app.ids(lib, &token).await["The Long Road"];
    let parts = app.children(road, &token).await;
    let (prologue, one) = (parts["Prologue"], parts["Chapter One"]);

    assert_eq!(on_deck(&app, &token, road).await["id"], prologue, "a book not started starts at its beginning");
    assert!(hub(&app, &token, "/api/hubs/home", "continue_listening").await.is_empty());

    progress(&app, &token, prologue, 600_000).await;
    let deck = on_deck(&app, &token, road).await;
    assert_eq!((deck["id"].as_i64(), deck["user_state"]["position_ms"].as_i64()), (Some(prologue), Some(600_000)));
    assert_eq!(hub(&app, &token, "/api/hubs/home", "continue_listening").await, ["The Long Road"]);
    assert_eq!(hub(&app, &token, &format!("/api/libraries/{lib}/hubs"), "continue_listening").await, ["The Long Road"]);
    // Audiobooks aren't videos to continue watching.
    assert!(hub(&app, &token, "/api/hubs/home", "continue_watching").await.is_empty());

    // 95 % of an hour is three minutes before the end: still to hear.
    progress(&app, &token, prologue, 3_420_000).await;
    let state = &app.get(&format!("/api/items/{prologue}"), &token).await.body["user_state"];
    assert_eq!((state["watched"].as_bool(), state["position_ms"].as_i64()), (Some(false), Some(3_420_000)));

    // Its last seconds finish it, and the next part is next.
    progress(&app, &token, prologue, 3_590_000).await;
    assert_eq!(on_deck(&app, &token, road).await["id"], one);
    assert_eq!(hub(&app, &token, "/api/hubs/home", "continue_listening").await, ["The Long Road"]);
    let book = app.get(&format!("/api/items/{road}"), &token).await;
    assert_eq!((book.body["leaf_count"].as_i64(), book.body["viewed_leaf_count"].as_i64()), (Some(2), Some(1)));
    assert_eq!(book.body["user_state"]["watched"], false);

    // Finished, the book leaves Continue Listening.
    progress(&app, &token, one, 3_600_000).await;
    assert!(hub(&app, &token, "/api/hubs/home", "continue_listening").await.is_empty());
    assert_eq!(app.get(&format!("/api/items/{road}"), &token).await.body["user_state"]["watched"], true);

    // Taken out of Continue Listening, a book comes back when played again.
    progress(&app, &token, one, 60_000).await;
    assert_eq!(hub(&app, &token, "/api/hubs/home", "continue_listening").await, ["The Long Road"]);
    let hidden = app.delete(&format!("/api/continue-watching/{one}"), &token).await;
    assert_eq!(hidden.status, StatusCode::NO_CONTENT, "{:?}", hidden.body);
    assert!(hub(&app, &token, "/api/hubs/home", "continue_listening").await.is_empty());
    progress(&app, &token, one, 120_000).await;
    assert_eq!(hub(&app, &token, "/api/hubs/home", "continue_listening").await, ["The Long Road"]);
}

/// Plex's and Jellyfin's apps would read books as albums of tracks: their
/// APIs list no audiobook library.
#[tokio::test]
async fn compatibility_apis_leave_audiobooks_out() {
    let (app, token, lib) = books().await;
    app.media_file("movies/Some Movie (2001)/Some Movie (2001).mkv", 512);
    let movies = app.library(&token, "Movies", "movies", "movies").await;

    let sections = app.plex_json("/library/sections", &token).await;
    let names: Vec<&str> =
        sections["Directory"].as_array().unwrap().iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert_eq!(names, ["Movies"]);
    let hubs = app.plex_json("/hubs", &token).await;
    assert_eq!(titles(&hubs["Hub"]), ["Recently Added in Movies"]);
    let book = app.ids(lib, &token).await["Middle Book"];
    let r = app.plex(Method::GET, &format!("/library/metadata/{book}"), &token, &[]).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.plex(Method::GET, &format!("/library/sections/{lib}/all"), &token, &[]).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        movies,
        app.plex_json("/library/sections", &token).await["Directory"][0]["key"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
    );

    let jf = app.jellyfin_token(&token).await;
    let views = app.jf_json("/UserViews", &jf).await;
    let names: Vec<&str> = views["Items"].as_array().unwrap().iter().map(|v| v["Name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Movies"]);
}

/// Books are found as Audiobookshelf finds them, each file by its own
/// folders and tags: a book in several `.m4b` files is one, books side by
/// side in their author's folder are each their own.
#[tokio::test]
async fn books_are_found_in_the_usual_layouts() {
    let (app, token) = tagged_app().await;
    tagged(&app, "books/Author A/Book One/Book One.m4b", json!({"album": "Book One"}));
    app.media_file("books/Author A/Series X/Book Two/Book Two.m4b", 512);
    tagged(&app, "books/Author B/Split Book/Split Book 1.m4b", json!({"album": "Split Book", "track": "1"}));
    tagged(&app, "books/Author B/Split Book/Split Book 2.m4b", json!({"album": "Split Book", "track": "2"}));
    tagged(&app, "books/Author C/Book Four.m4b", json!({"album": "Book Four"}));
    tagged(&app, "books/Author C/Book Five.m4b", json!({"album": "Book Five"}));
    for n in 1..=3 {
        app.media_file(&format!("books/Book Six/0{n}.mp3"), 512);
    }
    app.media_file("books/Book Seven/Book Seven.m4b", 512);
    app.media_file("books/Loose One.mp3", 512);
    app.media_file("books/Loose Two.m4b", 512);
    app.media_file("books/Author D/Book Eight/CD 1/01 - Start.mp3", 512);
    app.media_file("books/Author D/Book Eight/CD 2/01 - End.mp3", 512);
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;

    let book = |authors: &str, series: Value, parts: &[&str]| {
        (json!([authors]), series, parts.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>())
    };
    let unknown = "Unknown Author";
    let expected: BTreeMap<String, _> = [
        ("Book One", book("Author A", Value::Null, &["Book One"])),
        ("Book Two", book("Author A", json!("Series X"), &["Book Two"])),
        ("Split Book", book("Author B", Value::Null, &["Split Book 1", "Split Book 2"])),
        ("Book Four", book("Author C", Value::Null, &["Book Four"])),
        ("Book Five", book("Author C", Value::Null, &["Book Five"])),
        ("Book Six", book(unknown, Value::Null, &["01", "02", "03"])),
        ("Book Seven", book(unknown, Value::Null, &["Book Seven"])),
        ("Loose One", book(unknown, Value::Null, &["Loose One"])),
        ("Loose Two", book(unknown, Value::Null, &["Loose Two"])),
        ("Book Eight", book("Author D", Value::Null, &["Start", "End"])),
    ]
    .into_iter()
    .map(|(title, book)| (title.to_owned(), book))
    .collect();
    assert_eq!(shelf(&app, &token, lib).await, expected);
}

/// The picture of a book in its folder, under the media folder.
async fn cover(app: &TestApp, book: i64) -> Option<String> {
    let poster: Option<String> = sqlx::query_scalar("SELECT poster FROM items WHERE id = ?")
        .bind(book)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    poster
        .map(|p| std::path::Path::new(&p).strip_prefix(app.media.path()).unwrap().to_string_lossy().replace('\\', "/"))
}

/// A folder's picture is the cover of the book whose folder it is, not of
/// books side by side in their author's or their series' folder, which
/// keep the pictures named after them, or else the ones in their files.
#[tokio::test]
async fn books_side_by_side_keep_their_own_covers() {
    let (app, token) = tagged_app().await;
    tagged(&app, "books/Author C/Book Four.m4b", json!({"album": "Book Four"}));
    tagged(&app, "books/Author C/Book Five.m4b", json!({"album": "Book Five"}));
    app.media_file("books/Author C/folder.jpg", 64);
    app.media_file("books/Author C/Book Five.jpg", 64);
    app.media_file("books/Author E/Series Y/Book Nine.m4b", 512);
    app.media_file("books/Author E/Series Y/Book Ten.m4b", 512);
    app.media_file("books/Author E/Series Y/cover.jpg", 64);
    tagged(&app, "books/Author A/Book One/Book One.m4b", json!({"album": "Book One"}));
    app.media_file("books/Author A/Book One/cover.jpg", 64);
    app.media_file("books/Author B/Book Three/01.mp3", 512);
    app.media_file("books/Author B/Book Three/02.mp3", 512);
    app.media_file("books/Author B/Book Three/folder.jpg", 64);
    app.media_file("books/Author F/Series W/Book Sixteen.m4b", 512);
    app.media_file("books/Author F/Series W/cover.jpg", 64);
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;

    let ids = app.ids(lib, &token).await;
    assert_eq!(cover(&app, ids["Book Four"]).await, None, "the author's picture isn't the book's");
    assert_eq!(cover(&app, ids["Book Five"]).await.as_deref(), Some("books/Author C/Book Five.jpg"));
    assert_eq!(cover(&app, ids["Book Nine"]).await, None, "nor the series' picture");
    assert_eq!(cover(&app, ids["Book Ten"]).await, None);
    assert_eq!(cover(&app, ids["Book One"]).await.as_deref(), Some("books/Author A/Book One/cover.jpg"));
    assert_eq!(cover(&app, ids["Book Three"]).await.as_deref(), Some("books/Author B/Book Three/folder.jpg"));

    // Alone in its folder, a book has the folder's picture, until another
    // book comes beside it.
    assert_eq!(cover(&app, ids["Book Sixteen"]).await.as_deref(), Some("books/Author F/Series W/cover.jpg"));
    app.media_file("books/Author F/Series W/Book Seventeen.m4b", 512);
    let r = app.post(&format!("/api/libraries/{lib}/scan"), &token, json!({})).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    app.wait_for_scans().await;
    assert_eq!(cover(&app, ids["Book Sixteen"]).await, None);
}

/// Parts that tags number not play in their files' order, with the numbers
/// in the names read as numbers; a book's title of numbers keeps them.
#[tokio::test]
async fn parts_play_in_their_names_order() {
    let (app, token) = tagged_app().await;
    for n in 1..=10 {
        app.media_file(&format!("books/Author A/Book Nine/Part {n}.mp3"), 512);
    }
    // Titled, but not numbered, by their tags.
    tagged(&app, "books/Author A/Book Ten/Chapter 2.mp3", json!({"title": "Zebra"}));
    tagged(&app, "books/Author A/Book Ten/Chapter 10.mp3", json!({"title": "Apple"}));
    app.media_file("books/Author B/10.20.30/01.mp3", 512);
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;

    let ids = app.ids(lib, &token).await;
    let parts = app.get(&format!("/api/items/{}/children", ids["Book Nine"]), &token).await;
    let expected: Vec<String> = (1..=10).map(|n| format!("Part {n}")).collect();
    assert_eq!(titles(&parts.body["items"]), expected);
    let by_title = app.children(ids["Book Nine"], &token).await;
    let next = app.get(&format!("/api/items/{}/next", by_title["Part 9"]), &token).await;
    assert_eq!(next.body["item"]["title"], "Part 10");
    let next = app.get(&format!("/api/items/{}/next", by_title["Part 2"]), &token).await;
    assert_eq!(next.body["item"]["title"], "Part 3");

    let parts = app.get(&format!("/api/items/{}/children", ids["Book Ten"]), &token).await;
    assert_eq!(titles(&parts.body["items"]), ["Zebra", "Apple"]);
    assert!(ids.contains_key("10.20.30"), "{ids:?}");
}

/// A book is what its first part says: a file added later without tags
/// doesn't name it from its folders, and when its first part goes, the
/// next one says what it is, unless its details are locked.
#[tokio::test]
async fn a_book_is_what_its_first_part_says() {
    let (app, token) = tagged_app().await;
    let tags = |summary: &str| json!({"album": "Tagged Title", "artist": "Author G", "description": summary});
    tagged(&app, "books/Author F/Book Eleven/01.mp3", tags("From the first part."));
    tagged(&app, "books/Author F/Book Eleven/02.mp3", tags("From the second part."));
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;
    let book = app.ids(lib, &token).await["Tagged Title"];
    let details = || async {
        let item = app.get(&format!("/api/items/{book}"), &token).await.body;
        (item["title"].clone(), item["book"]["authors"].clone(), item["summary"].clone())
    };
    let first = (json!("Tagged Title"), json!(["Author G"]), json!("From the first part."));
    assert_eq!(details().await, first);

    app.media_file("books/Author F/Book Eleven/03.mp3", 512);
    rescan(&app, &token, lib).await;
    assert_eq!(app.children(book, &token).await.len(), 3);
    assert_eq!(details().await, first, "a part added later doesn't rename the book");

    std::fs::remove_file(app.media.path().join("books/Author F/Book Eleven/01.mp3")).unwrap();
    rescan(&app, &token, lib).await;
    assert_eq!(details().await, (json!("Tagged Title"), json!(["Author G"]), json!("From the second part.")));

    // Locked, its details stay when only the untagged part is left.
    sqlx::query("UPDATE items SET metadata_locked = 1 WHERE id = ?")
        .bind(book)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    std::fs::remove_file(app.media.path().join("books/Author F/Book Eleven/02.mp3")).unwrap();
    rescan(&app, &token, lib).await;
    assert_eq!(app.children(book, &token).await.len(), 1);
    assert_eq!(details().await, (json!("Tagged Title"), json!(["Author G"]), json!("From the second part.")));
}

/// A book a scan made but stopped before reading (the server went away
/// part way) is read by the next scan, though none of its files changed.
#[tokio::test]
async fn a_book_a_stopped_scan_left_unread_is_read_next_time() {
    let (app, token) = tagged_app().await;
    tagged(&app, "books/Author H/Book Fifteen/01.mp3", json!({"album": "Book Fifteen", "artist": "Author H"}));
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;
    let book = app.ids(lib, &token).await["Book Fifteen"];
    // As `Writer::book` leaves a new book until the scan reads it.
    sqlx::query("UPDATE items SET authors = NULL, narrators = NULL, series = NULL, summary = NULL WHERE id = ?")
        .bind(book)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    rescan(&app, &token, lib).await;
    let item = app.get(&format!("/api/items/{book}"), &token).await.body;
    assert_eq!(item["book"]["authors"], json!(["Author H"]));
}

/// A book's series is read from the tags Libation, tone and
/// Audiobookshelf write: `part` for its place, and `grouping`.
#[tokio::test]
async fn series_from_the_tags_tools_write() {
    let (app, token) = tagged_app().await;
    tagged(&app, "books/Author A/Book Twelve.m4b", json!({"album": "Book Twelve", "series": "Series Y", "part": "3"}));
    tagged(&app, "books/Author A/Book Thirteen.m4b", json!({"album": "Book Thirteen", "grouping": "Series Z #2"}));
    tagged(&app, "books/Author A/Book Fourteen.m4b", json!({"album": "Book Fourteen", "grouping": "Fantasy"}));
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;

    let ids = app.ids(lib, &token).await;
    let series = |title: &str| {
        let id = ids[title];
        let app = &app;
        let token = &token;
        async move {
            let book = &app.get(&format!("/api/items/{id}"), token).await.body["book"];
            (book["series"].clone(), book["series_index"].clone())
        }
    };
    assert_eq!(series("Book Twelve").await, (json!("Series Y"), json!(3.0)));
    assert_eq!(series("Book Thirteen").await, (json!("Series Z"), json!(2.0)));
    assert_eq!(series("Book Fourteen").await, (Value::Null, Value::Null));
}

/// A file whose tags are crafted to be long is scanned as quickly as any
/// other, and its book keeps only so much of them (SECURITY.md, "Untrusted
/// media").
#[tokio::test]
async fn long_tags_are_cut() {
    let (app, token) = tagged_app().await;
    let authors: String = (0..200_000).map(|n| format!("Author {n}; ")).collect();
    let tags = json!({"album": "t".repeat(100_000), "artist": authors, "description": "d".repeat(100_000)});
    tagged(&app, "books/Author A/Book Fifteen.m4b", tags);
    let started = std::time::Instant::now();
    let lib = app.library(&token, "Audiobooks", "audiobooks", "books").await;
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());

    let book = app.ids(lib, &token).await["t".repeat(500).as_str()];
    let item = app.get(&format!("/api/items/{book}"), &token).await.body;
    let authors = item["book"]["authors"].as_array().unwrap();
    assert_eq!((authors.len(), &authors[49]), (50, &json!("Author 49")));
    assert_eq!(item["summary"].as_str().map(str::len), Some(20_000));
}
