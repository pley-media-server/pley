//! End-to-end tests of the Plex-compatible API (`pley::plex`), in-process
//! against a temporary data directory, like tests/api.rs.

mod common;

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use pley::{AppState, Config};

struct TestApp {
    api: Router,
    plex: pley::plex::Service,
    state: AppState,
    media: tempfile::TempDir,
    _data: tempfile::TempDir,
}

struct Reply {
    status: StatusCode,
    text: String,
    json: Value,
    bytes: Vec<u8>,
}

impl TestApp {
    async fn new() -> Self {
        let data = common::tempdir();
        let mut config = Config::for_data_dir(data.path());
        config.transcoder_path = Some(data.path().join("no-transcoder-in-tests"));
        let state = AppState::init(config).await.unwrap();
        let api = pley::api::router(state.clone());
        let plex = pley::plex::service(state.clone());
        Self { api, plex, state, media: tempfile::tempdir().unwrap(), _data: data }
    }

    async fn api(&self, method: Method, uri: &str, token: Option<&str>, body: Option<Value>) -> Value {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let resp = self.api.clone().oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 24).await.unwrap();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    /// Raw bytes of a pley API response.
    async fn api_raw(&self, uri: &str, token: &str) -> (StatusCode, Vec<u8>) {
        let req = Request::builder().uri(uri).header(header::AUTHORIZATION, format!("Bearer {token}"));
        let resp = self.api.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = resp.status();
        (status, axum::body::to_bytes(resp.into_body(), 1 << 24).await.unwrap().to_vec())
    }

    async fn plex(&self, uri: &str, headers: &[(&str, &str)]) -> Reply {
        let mut req = Request::builder().method(Method::GET).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = self.plex.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 24).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, text, json, bytes: bytes.to_vec() }
    }

    async fn plex_delete(&self, uri: &str, token: &str) -> StatusCode {
        let req = Request::builder().method(Method::DELETE).uri(uri).header("x-plex-token", token);
        self.plex.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap().status()
    }

    /// JSON response with the token as Plex clients send it.
    async fn plex_json(&self, uri: &str, token: &str) -> Value {
        let reply = self.plex(uri, &[("x-plex-token", token), ("accept", "application/json")]).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text);
        reply.json["MediaContainer"].clone()
    }

    fn media_file(&self, rel: &str) {
        let path = self.media.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; 4096]).unwrap();
    }

    /// Sets pley up: the administrator's session, for pley's own API, and a
    /// compatibility token of theirs that may do everything, for this one.
    async fn setup(&self) -> (String, String) {
        let r = self
            .api(
                Method::POST,
                "/api/setup",
                None,
                Some(
                    json!({"username": "admin", "password": "secret-one", "setup_code": self.state.auth.setup.shown()}),
                ),
            )
            .await;
        let session = r["token"].as_str().unwrap().to_owned();
        let token = common::compatibility_token(&self.api, &session, "play activity manage").await;
        (session, token)
    }

    async fn library(&self, token: &str, name: &str, kind: &str, rel: &str) -> i64 {
        std::fs::create_dir_all(self.media.path().join(rel)).unwrap();
        let path = self.media.path().join(rel).to_string_lossy().into_owned();
        let r = self
            .api(
                Method::POST,
                "/api/libraries",
                Some(token),
                Some(json!({"name": name, "kind": kind, "paths": [path]})),
            )
            .await;
        for _ in 0..200 {
            if self.state.scanner.statuses().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        r["id"].as_i64().unwrap()
    }
}

fn metadata(container: &Value) -> Vec<Value> {
    container["Metadata"].as_array().cloned().unwrap_or_default()
}

fn titles(container: &Value) -> Vec<String> {
    metadata(container).iter().map(|m| m["title"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn identity_auth_and_formats() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;

    let identity = app.plex("/identity", &[]).await;
    assert_eq!(identity.status, StatusCode::OK);
    assert!(identity.text.contains("machineIdentifier=\""), "{}", identity.text);
    assert!(identity.text.starts_with("<?xml"));

    assert_eq!(app.plex("/library/sections", &[]).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.plex("/library/sections", &[("x-plex-token", "nope")]).await.status, StatusCode::UNAUTHORIZED);
    // A session's token is for pley's own apps.
    let refused = app.plex("/library/sections", &[("x-plex-token", &session)]).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert!(refused.text.contains("tokens for other apps"), "{}", refused.text);
    assert_eq!(app.plex("/library/sections", &[("x-plex-token", &token)]).await.status, StatusCode::OK);
    let query = app.plex(&format!("/library/sections?X-Plex-Token={token}"), &[]).await;
    assert_eq!(query.status, StatusCode::OK);
    let bearer = format!("Bearer {token}");
    assert_eq!(app.plex("/library/sections", &[("authorization", &bearer)]).await.status, StatusCode::OK);

    // `/:/…` paths and trailing slashes are routed.
    let root = app.plex_json("/", &token).await;
    assert_eq!(root["machineIdentifier"].as_str().unwrap().len(), 40);
    assert_eq!(app.plex(&format!("/library/sections/?X-Plex-Token={token}"), &[]).await.status, StatusCode::OK);
    let prefs = app.plex(&format!("/:/prefs?X-Plex-Token={token}"), &[]).await;
    assert_eq!(prefs.status, StatusCode::OK, "{}", prefs.text);
    assert_eq!(app.plex("/no/such/endpoint", &[("x-plex-token", &token)]).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sections_listings_filters_and_paging() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001).mkv");
    app.media_file("movies/Bravo (2002).mp4");
    app.media_file("movies/The Charlie Movie (2003).mkv");
    let lib = app.library(&session, "Films", "movies", "movies").await;

    let sections = app.plex_json("/library/sections", &token).await;
    let dirs = sections["Directory"].as_array().unwrap();
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0]["key"], lib.to_string());
    assert_eq!(dirs[0]["type"], "movie");
    assert_eq!(dirs[0]["title"], "Films");
    assert!(dirs[0]["Location"][0]["path"].as_str().unwrap().ends_with("movies"));

    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    assert_eq!(titles(&all), vec!["Alpha", "Bravo", "The Charlie Movie"]);
    assert_eq!(all["totalSize"], 3);
    let movie = &metadata(&all)[2];
    assert_eq!(movie["type"], "movie");
    assert_eq!(movie["titleSort"], "Charlie Movie");
    assert_eq!(movie["year"], 2003);
    assert!(movie["key"].as_str().unwrap().starts_with("/library/metadata/"));
    assert!(movie["Media"][0]["Part"][0]["key"].as_str().unwrap().starts_with("/library/parts/"));

    // Paging through headers (python-plexapi) and query parameters.
    let page = app
        .plex(
            &format!("/library/sections/{lib}/all"),
            &[
                ("x-plex-token", &token),
                ("accept", "application/json"),
                ("x-plex-container-start", "1"),
                ("x-plex-container-size", "1"),
            ],
        )
        .await;
    let page = &page.json["MediaContainer"];
    assert_eq!(
        (page["size"].as_i64(), page["totalSize"].as_i64(), page["offset"].as_i64()),
        (Some(1), Some(3), Some(1))
    );
    assert_eq!(titles(page), vec!["Bravo"]);
    let none = app.plex_json(&format!("/library/sections/{lib}/all?X-Plex-Container-Size=0"), &token).await;
    assert_eq!((none["size"].as_i64(), none["totalSize"].as_i64()), (Some(0), Some(3)));

    // Filters and sorting.
    let found = app.plex_json(&format!("/library/sections/{lib}/all?title=charl"), &token).await;
    assert_eq!(titles(&found), vec!["The Charlie Movie"]);
    let found = app.plex_json(&format!("/library/sections/{lib}/all?year%3E%3E=2001"), &token).await;
    assert_eq!(titles(&found), vec!["Bravo", "The Charlie Movie"]);
    let sorted = app.plex_json(&format!("/library/sections/{lib}/all?sort=movie.year:desc"), &token).await;
    assert_eq!(titles(&sorted), vec!["The Charlie Movie", "Bravo", "Alpha"]);
    let episodes = app.plex_json(&format!("/library/sections/{lib}/all?type=4"), &token).await;
    assert_eq!(episodes["size"], 0);

    // The filter description python-plexapi validates searches against.
    let meta =
        app.plex_json(&format!("/library/sections/{lib}/all?includeMeta=1&X-Plex-Container-Size=0"), &token).await;
    assert_eq!(meta["Meta"][0]["Type"][0]["type"], "movie");
    assert!(meta["Meta"][0]["FieldType"].as_array().unwrap().len() >= 5);

    // XML for clients that don't ask for JSON.
    let xml = app.plex(&format!("/library/sections/{lib}/all"), &[("x-plex-token", &token)]).await;
    assert!(xml.text.contains("<Video ratingKey=\""), "{}", xml.text);
    assert!(xml.text.contains("totalSize=\"3\""));
}

#[tokio::test]
async fn watch_state_and_sessions_are_shared_with_pley() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001).mp4");
    let lib = app.library(&session, "Movies", "movies", "movies").await;
    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    let id = metadata(&all)[0]["ratingKey"].as_str().unwrap().to_owned();

    let scrobble = app
        .plex(&format!("/:/scrobble?key={id}&identifier=com.plexapp.plugins.library"), &[("x-plex-token", &token)])
        .await;
    assert_eq!(scrobble.status, StatusCode::OK);
    let item = app.api(Method::GET, &format!("/api/items/{id}"), Some(&session), None).await;
    assert_eq!(item["user_state"]["watched"], true);
    let details = app.plex_json(&format!("/library/metadata/{id}"), &token).await;
    assert_eq!(metadata(&details)[0]["viewCount"], 1);
    app.plex(&format!("/:/unscrobble?key={id}"), &[("x-plex-token", &token)]).await;
    let item = app.api(Method::GET, &format!("/api/items/{id}"), Some(&session), None).await;
    assert_eq!(item["user_state"]["watched"], false);
    // Clients read `viewCount > 0` as played.
    let details = app.plex_json(&format!("/library/metadata/{id}"), &token).await;
    assert!(metadata(&details)[0].get("viewCount").is_none(), "{details}");

    // A Plex client playing on its own shows up as a pley session.
    let client = [
        ("x-plex-token", token.as_str()),
        ("x-plex-client-identifier", "test-client"),
        ("x-plex-product", "Test Player"),
    ];
    let timeline =
        app.plex(&format!("/:/timeline?ratingKey={id}&state=playing&time=20000&duration=600000"), &client).await;
    assert_eq!(timeline.status, StatusCode::OK, "{}", timeline.text);
    let sessions = app.plex_json("/status/sessions", &token).await;
    let playing = &metadata(&sessions)[0];
    assert_eq!(playing["ratingKey"], id);
    assert_eq!(playing["viewOffset"], 20000);
    assert_eq!(playing["User"]["title"], "admin");
    assert_eq!(playing["Player"]["product"], "Test Player");
    assert_eq!(playing["Player"]["machineIdentifier"], "test-client");
    let activity = app.api(Method::GET, "/api/activity", Some(&session), None).await;
    assert_eq!(activity["sessions"].as_array().unwrap().len(), 1);
    let item = app.api(Method::GET, &format!("/api/items/{id}"), Some(&session), None).await;
    assert_eq!(item["user_state"]["position_ms"], 20000);

    app.plex(&format!("/:/timeline?ratingKey={id}&state=stopped&time=21000"), &client).await;
    let sessions = app.plex_json("/status/sessions", &token).await;
    assert_eq!(sessions["size"], 0);
}

#[tokio::test]
async fn shows_hierarchy_search_and_hubs() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("tv/Some Show/Season 1/Some Show - S01E01 - Pilot.mkv");
    app.media_file("tv/Some Show/Season 1/Some Show - S01E02.mkv");
    app.media_file("tv/Some Show/Season 2/Some Show - S02E01.mkv");
    let lib = app.library(&session, "TV", "shows", "tv").await;

    let shows = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    let show = &metadata(&shows)[0];
    assert_eq!(show["type"], "show");
    assert_eq!(show["leafCount"], 3);
    assert_eq!(show["childCount"], 2);
    let show_id = show["ratingKey"].as_str().unwrap().to_owned();
    assert_eq!(show["key"], format!("/library/metadata/{show_id}/children"));

    let seasons = app.plex_json(&format!("/library/metadata/{show_id}/children"), &token).await;
    assert_eq!(titles(&seasons), vec!["Season 1", "Season 2"]);
    assert_eq!(metadata(&seasons)[0]["parentRatingKey"], show_id);
    let leaves = app.plex_json(&format!("/library/metadata/{show_id}/allLeaves"), &token).await;
    let episodes = metadata(&leaves);
    assert_eq!(episodes.len(), 3);
    assert_eq!((episodes[2]["parentIndex"].as_i64(), episodes[2]["index"].as_i64()), (Some(2), Some(1)));
    assert_eq!(episodes[0]["grandparentTitle"], "Some Show");

    // Episodes of one show through an ancestor filter.
    let filtered = app.plex_json(&format!("/library/sections/{lib}/all?type=4&show.id={show_id}"), &token).await;
    assert_eq!(filtered["size"], 3);

    let search = app.plex_json("/hubs/search?query=some", &token).await;
    let hub = &search["Hub"][0];
    assert_eq!(hub["type"], "show");
    assert_eq!(hub["Metadata"][0]["title"], "Some Show");

    let hubs = app.plex_json("/hubs", &token).await;
    let identifiers: Vec<&str> =
        hubs["Hub"].as_array().unwrap().iter().filter_map(|h| h["hubIdentifier"].as_str()).collect();
    assert!(identifiers.contains(&"home.television.recent"), "{identifiers:?}");
}

#[tokio::test]
async fn cast_and_crew_tags_and_filters() {
    use pley::people::{self, NewCredit, PersonId, Role};

    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001).mp4");
    app.media_file("movies/Beta (2002).mp4");
    let lib = app.library(&session, "Movies", "movies", "movies").await;
    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    let id_of = |title: &str| {
        metadata(&all).iter().find(|m| m["title"] == title).unwrap()["ratingKey"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
    };
    let (alpha, beta) = (id_of("Alpha"), id_of("Beta"));
    let credit = |tmdb_id: i64, name: &str, role: Role, character: Option<&str>| NewCredit {
        person: PersonId::Tmdb(tmdb_id),
        name: name.into(),
        photo: Some("https://image.tmdb.org/t/p/h632/face.jpg".into()),
        role,
        character: character.map(str::to_owned),
    };
    people::replace(
        &app.state.db,
        alpha,
        &[credit(1, "Ann Actor", Role::Actor, Some("Hero")), credit(2, "Dan Director", Role::Director, None)],
    )
    .await
    .unwrap();
    people::replace(&app.state.db, beta, &[credit(2, "Dan Director", Role::Director, None)]).await.unwrap();

    let details = app.plex_json(&format!("/library/metadata/{alpha}"), &token).await;
    let movie = &metadata(&details)[0];
    assert_eq!(movie["Role"][0]["tag"], "Ann Actor");
    assert_eq!(movie["Role"][0]["role"], "Hero");
    assert_eq!(movie["Role"][0]["thumb"], "https://image.tmdb.org/t/p/h632/face.jpg");
    let director = movie["Director"][0]["id"].as_i64().unwrap();
    assert_eq!(movie["Director"][0]["filter"], format!("director={director}"));

    let directors = app.plex_json(&format!("/library/sections/{lib}/director"), &token).await;
    assert_eq!(directors["Directory"][0]["title"], "Dan Director");
    let by_id = app.plex_json(&format!("/library/sections/{lib}/all?director={director}"), &token).await;
    assert_eq!(titles(&by_id), vec!["Alpha", "Beta"]);
    let by_name = app.plex_json(&format!("/library/sections/{lib}/all?actor=ann%20actor"), &token).await;
    assert_eq!(titles(&by_name), vec!["Alpha"]);
    let without = app.plex_json(&format!("/library/sections/{lib}/all?actor!=Ann%20Actor"), &token).await;
    assert_eq!(titles(&without), vec!["Beta"]);
}

#[tokio::test]
async fn play_history() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001).mp4");
    let lib = app.library(&session, "Movies", "movies", "movies").await;
    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    let id = metadata(&all)[0]["ratingKey"].as_str().unwrap().to_owned();
    let client = [
        ("x-plex-token", token.as_str()),
        ("x-plex-client-identifier", "history-client"),
        ("x-plex-device-name", "Living Room TV"),
    ];
    for time in [30_000, 590_000, 595_000] {
        let reply =
            app.plex(&format!("/:/timeline?ratingKey={id}&state=playing&time={time}&duration=600000"), &client).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    }
    let history = app.plex_json("/status/sessions/history/all?sort=viewedAt:desc", &token).await;
    let entries = metadata(&history);
    assert_eq!(entries.len(), 1, "{history}");
    assert_eq!(entries[0]["ratingKey"], id);
    assert_eq!(entries[0]["type"], "movie");
    assert!(entries[0]["viewedAt"].as_i64().unwrap() > 0);
    let key = entries[0]["historyKey"].as_str().unwrap().to_owned();
    let since = app.plex_json("/status/sessions/history/all?viewedAt%3E=4102444800", &token).await;
    assert!(metadata(&since).is_empty(), "nothing after 2100");

    let delete = app.plex_delete(&key, &token).await;
    assert_eq!(delete, StatusCode::OK);
    assert!(metadata(&app.plex_json("/status/sessions/history/all", &token).await).is_empty());
}

#[tokio::test]
async fn user_ratings_through_rate() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001).mp4");
    app.media_file("movies/Beta (2002).mp4");
    let lib = app.library(&session, "Movies", "movies", "movies").await;
    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    let beta = metadata(&all).iter().find(|m| m["title"] == "Beta").unwrap()["ratingKey"].as_str().unwrap().to_owned();
    let rated = app
        .plex(
            &format!("/:/rate?key={beta}&identifier=com.plexapp.plugins.library&rating=8"),
            &[("x-plex-token", &token)],
        )
        .await;
    assert_eq!(rated.status, StatusCode::OK, "{}", rated.text);
    let details = app.plex_json(&format!("/library/metadata/{beta}"), &token).await;
    assert_eq!(metadata(&details)[0]["userRating"], 8.0);
    let sorted = app.plex_json(&format!("/library/sections/{lib}/all?sort=userRating:desc"), &token).await;
    assert_eq!(titles(&sorted), vec!["Beta", "Alpha"]);
    let rated = app.plex_json(&format!("/library/sections/{lib}/all?userRating%3E%3E=5"), &token).await;
    assert_eq!(titles(&rated), vec!["Beta"]);

    app.plex(
        &format!("/:/rate?key={beta}&identifier=com.plexapp.plugins.library&rating=-1"),
        &[("x-plex-token", &token)],
    )
    .await;
    let details = app.plex_json(&format!("/library/metadata/{beta}"), &token).await;
    assert!(metadata(&details)[0].get("userRating").is_none());
}

#[tokio::test]
async fn preview_indexes_and_chapter_thumbs() {
    use pley_transcoder_api::{Chapter, MediaInfo, TrickplayInfo};

    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001).mkv");
    let lib = app.library(&session, "Movies", "movies", "movies").await;
    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    let item: i64 = metadata(&all)[0]["ratingKey"].as_str().unwrap().parse().unwrap();
    let db = &app.state.db;
    let (file_id, path): (i64, String) = sqlx::query_as("SELECT id, path FROM media_files WHERE item_id = ?")
        .bind(item)
        .fetch_one(&db.read)
        .await
        .unwrap();

    // A file with three chapters and (fake) seek previews: tests have no transcoder.
    let chapter = |start_ms, end_ms, title: &str| Chapter { start_ms, end_ms, title: Some(title.into()) };
    let info = MediaInfo {
        duration_ms: Some(90_000),
        chapters: vec![
            chapter(0, 30_000, "Opening"),
            chapter(30_000, 60_000, "Middle"),
            chapter(60_000, 90_000, "End"),
        ],
        ..Default::default()
    };
    sqlx::query("UPDATE media_files SET media_info = ?, video_codec = 'h264' WHERE id = ?")
        .bind(serde_json::to_string(&info).unwrap())
        .bind(file_id)
        .execute(&db.write)
        .await
        .unwrap();
    let fingerprint = app.state.store.fingerprint_of(file_id, &path, None).await.unwrap();
    let dir = app.state.store.media_dir(&fingerprint).unwrap().join("trickplay");
    std::fs::create_dir_all(&dir).unwrap();
    // One sheet of 10 tiles (8x4 each), a different color per tile.
    let mut sheet = image::RgbImage::new(8 * 5, 4 * 2);
    for (x, y, px) in sheet.enumerate_pixels_mut() {
        let n = (y / 4) * 5 + x / 8;
        *px = image::Rgb([(n * 25) as u8, 100, 200]);
    }
    sheet.save(dir.join("0.jpg")).unwrap();
    let trickplay = TrickplayInfo {
        id: "t".into(),
        interval_ms: 10_000,
        width: 8,
        height: 4,
        columns: 5,
        rows: 2,
        count: 10,
        sheets: 1,
        version: 0,
    };
    std::fs::write(dir.join("index.json"), serde_json::to_vec(&trickplay).unwrap()).unwrap();

    // pley's API: chapters get pictures.
    let detail = app.api(Method::GET, &format!("/api/items/{item}"), Some(&session), None).await;
    let chapters = detail["files"][0]["chapters"].as_array().unwrap().clone();
    assert_eq!(chapters.len(), 3);
    let url = chapters[1]["thumb_url"].as_str().unwrap().to_owned();
    let picture = app.api_raw(&url, &session).await;
    assert_eq!(picture.0, StatusCode::OK);
    assert_eq!(&picture.1[..2], &[0xff, 0xd8], "a JPEG");

    // Plex: the part has preview indexes and the chapters have pictures.
    let details = app.plex_json(&format!("/library/metadata/{item}"), &token).await;
    let movie = &metadata(&details)[0];
    assert_eq!(movie["Media"][0]["Part"][0]["indexes"], "sd");
    assert_eq!(movie["Chapter"][1]["tag"], "Middle");
    assert_eq!(movie["Chapter"][1]["startTimeOffset"], 30_000);
    let thumb = movie["Chapter"][2]["thumb"].as_str().unwrap().to_owned();
    let image = app.plex(&format!("{thumb}?X-Plex-Token={token}"), &[]).await;
    assert_eq!(image.status, StatusCode::OK);

    let bif = app.plex(&format!("/library/parts/{file_id}/indexes/sd?X-Plex-Token={token}"), &[]).await;
    assert_eq!(bif.status, StatusCode::OK);
    let frame = app.plex(&format!("/library/parts/{file_id}/indexes/sd/45000?X-Plex-Token={token}"), &[]).await;
    assert_eq!(frame.status, StatusCode::OK);
    // 45 s in, at a frame every 10 s: the fifth of ten, and tile 4's colour.
    let word = |at: usize| u32::from_le_bytes(bif.bytes[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!(word(12), 10);
    assert_eq!(frame.bytes, &bif.bytes[word(64 + 4 * 8 + 4)..word(64 + 5 * 8 + 4)]);
    let pixel = image::load_from_memory(&frame.bytes).unwrap().to_rgb8()[(4, 2)];
    assert!(pixel[0].abs_diff(100) < 20 && pixel[2].abs_diff(200) < 20, "tile 4 is {pixel:?}");

    // The BIF index is kept in the cache, not beside the previews.
    assert!(!dir.join("index-sd.bif").exists());
    let kept: Vec<_> = std::fs::read_dir(app.state.config.cache_dir().join("bif"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(std::fs::read(&kept[0]).unwrap(), bif.bytes);
    let again = app.plex(&format!("/library/parts/{file_id}/indexes/sd?X-Plex-Token={token}"), &[]).await;
    assert_eq!(again.bytes, bif.bytes);
}

#[tokio::test]
async fn extras_as_clips() {
    let app = TestApp::new().await;
    let (session, token) = app.setup().await;
    app.media_file("movies/Alpha (2001)/Alpha (2001).mp4");
    app.media_file("movies/Alpha (2001)/Alpha (2001)-trailer.mp4");
    app.media_file("movies/Alpha (2001)/Deleted Scenes/Cut.mp4");
    let lib = app.library(&session, "Movies", "movies", "movies").await;
    let all = app.plex_json(&format!("/library/sections/{lib}/all"), &token).await;
    assert_eq!(titles(&all), vec!["Alpha"]);
    let id = metadata(&all)[0]["ratingKey"].as_str().unwrap().to_owned();
    let extras = app.plex_json(&format!("/library/metadata/{id}/extras"), &token).await;
    let clips = metadata(&extras);
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0]["type"], "clip");
    assert_eq!(clips[0]["subtype"], "trailer");
    assert_eq!(clips[0]["extraType"], 1);
    assert_eq!(clips[1]["subtype"], "deletedScene");
    assert!(clips[0]["Media"][0]["Part"][0]["key"].as_str().is_some(), "playable");
}
