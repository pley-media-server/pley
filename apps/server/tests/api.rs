//! End-to-end tests of the HTTP API, run in-process against a temporary data
//! directory. No transcoder is started, so scanned files stay unprobed (the
//! server must still list, browse, search and direct-play them).

use std::path::Path;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use pley::people::{self, NewCredit, PersonId, Role};
use pley::{AppState, Config};

struct TestApp {
    router: Router,
    state: AppState,
    media: tempfile::TempDir,
    _data: tempfile::TempDir,
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
    bytes: Vec<u8>,
}

impl TestApp {
    async fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let mut config = Config::for_data_dir(data.path());
        config.transcoder_path = Some(data.path().join("no-transcoder-in-tests"));
        let state = AppState::init(config).await.unwrap();
        let router = pley::api::router(state.clone());
        Self { router, state, media: tempfile::tempdir().unwrap(), _data: data }
    }

    fn media_file(&self, rel: &str, bytes: usize) {
        let path = self.media.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; bytes]).unwrap();
    }

    fn media_path(&self, rel: &str) -> String {
        self.media.path().join(rel).to_string_lossy().into_owned()
    }

    async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024 * 1024).await.unwrap().to_vec();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, headers, body, bytes }
    }

    /// A request with a raw body (uploads).
    async fn call_raw(&self, method: Method, uri: &str, token: &str, bytes: Vec<u8>) -> Reply {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(bytes))
            .unwrap();
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024 * 1024).await.unwrap().to_vec();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, headers, body, bytes }
    }

    async fn get(&self, uri: &str, token: &str) -> Reply {
        self.call(Method::GET, uri, Some(token), None, &[]).await
    }

    async fn post(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::POST, uri, Some(token), Some(body), &[]).await
    }

    async fn setup_admin(&self) -> String {
        let r = self
            .call(
                Method::POST,
                "/api/setup",
                None,
                Some(
                    json!({"username": "admin", "password": "secret-one", "setup_code": self.state.auth.setup.shown()}),
                ),
                &[],
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        r.body["token"].as_str().unwrap().to_owned()
    }

    async fn wait_for_scans(&self) {
        for _ in 0..200 {
            if self.state.scanner.statuses().is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("scan did not finish");
    }

    async fn library(&self, token: &str, name: &str, kind: &str, rel: &str) -> i64 {
        std::fs::create_dir_all(self.media.path().join(rel)).unwrap();
        let r = self
            .post("/api/libraries", token, json!({"name": name, "kind": kind, "paths": [self.media_path(rel)]}))
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
        self.wait_for_scans().await;
        r.body["id"].as_i64().unwrap()
    }
}

fn titles(items: &Value) -> Vec<String> {
    items.as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn setup_login_and_permissions() {
    let app = TestApp::new().await;
    let status = app.call(Method::GET, "/api/server", None, None, &[]).await;
    assert_eq!(status.body["setup_required"], json!(true));
    assert_eq!(status.body["source"], json!(pley::SOURCE));

    let admin = app.setup_admin().await;
    let again = app
        .call(Method::POST, "/api/setup", None, Some(json!({"username": "x", "password": "another-password"})), &[])
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(app.call(Method::GET, "/api/server", None, None, &[]).await.body["setup_required"], json!(false));

    // Unauthenticated access is rejected, bad passwords too.
    assert_eq!(app.call(Method::GET, "/api/libraries", None, None, &[]).await.status, StatusCode::UNAUTHORIZED);
    let bad = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "admin", "password": "nope"})), &[])
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);

    // Login sets an HttpOnly cookie that authenticates too.
    let login = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "ADMIN", "password": "secret-one"})), &[])
        .await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.headers[header::SET_COOKIE].to_str().unwrap().to_owned();
    assert!(cookie.contains("HttpOnly"));
    let cookie_pair = cookie.split(';').next().unwrap().to_owned();
    let me = app.call(Method::GET, "/api/auth/me", None, None, &[("cookie", &cookie_pair)]).await;
    assert_eq!(me.body["username"], json!("admin"));
    assert_eq!(me.body["is_admin"], json!(true));

    // Another site's page changes nothing with the cookie (a page on another
    // port of this host is the same site to the browser); pley's own does.
    let clean = |extra: &'static [(&'static str, &'static str)]| {
        let mut headers = vec![("cookie", cookie_pair.as_str())];
        headers.extend_from_slice(extra);
        let app = &app;
        async move { app.call(Method::POST, "/api/system/storage/clean", None, None, &headers).await.status }
    };
    assert_eq!(clean(&[("sec-fetch-site", "same-site")]).await, StatusCode::FORBIDDEN);
    assert_eq!(clean(&[("origin", "http://nas:8080"), ("host", "nas:7539")]).await, StatusCode::FORBIDDEN);
    assert_ne!(clean(&[("sec-fetch-site", "same-origin")]).await, StatusCode::FORBIDDEN);
    assert_ne!(clean(&[("origin", "http://nas:7539"), ("host", "nas:7539")]).await, StatusCode::FORBIDDEN);
    // A `?token=` of the page's making doesn't turn the check off: the cookie
    // is what would sign the request in.
    let headers = [("cookie", cookie_pair.as_str()), ("sec-fetch-site", "same-site")];
    let with_query = app.call(Method::POST, "/api/system/storage/clean?token=x", None, None, &headers).await;
    assert_eq!(with_query.status, StatusCode::FORBIDDEN);
    // Nor do its links and images fetch anything with it (an administrator's
    // folder browser opening the attacker's share); the address bar does.
    let browse = |site: &'static str| {
        let headers = [("cookie", cookie_pair.as_str()), ("sec-fetch-site", site)];
        let app = &app;
        async move { app.call(Method::GET, "/api/fs", None, None, &headers).await.status }
    };
    assert_eq!(browse("cross-site").await, StatusCode::FORBIDDEN);
    assert_eq!(browse("same-site").await, StatusCode::FORBIDDEN);
    assert_eq!(browse("same-origin").await, StatusCode::OK);
    assert_eq!(browse("none").await, StatusCode::OK);
    // A token sent on purpose is no cookie.
    let headers = [("sec-fetch-site", "cross-site")];
    assert_eq!(app.call(Method::GET, "/api/fs", Some(&admin), None, &headers).await.status, StatusCode::OK);

    // A regular user cannot use admin endpoints.
    let created =
        app.post("/api/users", &admin, json!({"username": "bob", "password": "bob-password", "is_admin": false})).await;
    assert_eq!(created.status, StatusCode::CREATED);
    let bob = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "bob", "password": "bob-password"})), &[])
        .await;
    let bob = bob.body["token"].as_str().unwrap().to_owned();
    assert_eq!(app.get("/api/users", &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/settings", &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/libraries", &bob).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/activity", &bob).await.status, StatusCode::OK);

    // Wrong current password is a 400 (not a 401 that would sign the UI out).
    let change = app
        .call(
            Method::PUT,
            "/api/auth/password",
            Some(&bob),
            Some(json!({"current_password": "x", "new_password": "new-password"})),
            &[],
        )
        .await;
    assert_eq!(change.status, StatusCode::BAD_REQUEST);

    // Logout revokes the token.
    let out = app.call(Method::POST, "/api/auth/logout", Some(&bob), None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);

    // Admins cannot delete themselves.
    let me_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let del = app.call(Method::DELETE, &format!("/api/users/{me_id}"), Some(&admin), None, &[]).await;
    assert_eq!(del.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn movies_are_scanned_listed_and_searchable() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    app.media_file("movies/The Matrix (1999)/The Matrix (1999).mkv", 2048);
    app.media_file("movies/The Matrix (1999)/The Matrix (1999).en.srt", 10);
    app.media_file("movies/The Matrix (1999)/poster.jpg", 10);
    app.media_file("movies/Blade.Runner.2049.2017.2160p.mkv", 2048);
    app.media_file("movies/Alien (1979).mp4", 4096);
    app.media_file("movies/sample.mkv", 10);
    let lib = app.library(&token, "Movies", "movies", "movies").await;

    let page = app.get(&format!("/api/libraries/{lib}/items"), &token).await;
    assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
    assert_eq!(page.body["total"], json!(3));
    assert_eq!(titles(&page.body["items"]), vec!["Alien", "Blade Runner 2049", "The Matrix"]);

    let by_year = app.get(&format!("/api/libraries/{lib}/items?sort=year&order=desc&limit=1"), &token).await;
    assert_eq!(by_year.body["total"], json!(3));
    assert_eq!(titles(&by_year.body["items"]), vec!["Blade Runner 2049"]);

    // `total` counts every match, whatever the page.
    let second = app.get(&format!("/api/libraries/{lib}/items?offset=1&limit=1"), &token).await;
    assert_eq!(second.body["total"], json!(3));
    assert_eq!(titles(&second.body["items"]), vec!["Blade Runner 2049"]);
    let past_end = app.get(&format!("/api/libraries/{lib}/items?offset=5"), &token).await;
    assert_eq!(past_end.body["total"], json!(3));
    assert_eq!(past_end.body["items"], json!([]));

    let matrix = page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == "The Matrix").unwrap().clone();
    assert_eq!(matrix["year"], json!(1999));
    assert!(matrix.get("parent_id").is_none(), "fields without a value are left out");
    assert!(matrix["poster_url"].as_str().unwrap().contains("/images/poster"));
    let id = matrix["id"].as_i64().unwrap();
    let detail = app.get(&format!("/api/items/{id}"), &token).await;
    let streams = detail.body["files"][0]["streams"].as_array().unwrap();
    assert!(streams.iter().any(|s| s["external"] == json!(true) && s["language"] == json!("eng")));

    let poster = app.get(&format!("/api/items/{id}/images/poster?v=1"), &token).await;
    assert_eq!(poster.status, StatusCode::OK);
    assert!(poster.headers[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));

    let search = app.get("/api/search?q=matr", &token).await;
    assert_eq!(titles(&search.body["items"]), vec!["The Matrix"]);
    assert_eq!(app.get("/api/search?q=%20", &token).await.body["items"], json!([]));

    let library = app.get(&format!("/api/libraries/{lib}"), &token).await;
    assert_eq!(library.body["item_count"], json!(3));
    assert_eq!(library.body["scanning"], json!(false));

    // Removing a file on rescan removes the item; offline roots are not purged.
    std::fs::remove_file(app.media.path().join("movies/Alien (1979).mp4")).unwrap();
    let scan = app.post(&format!("/api/libraries/{lib}/scan"), &token, json!({"force": false})).await;
    assert_eq!(scan.status, StatusCode::ACCEPTED);
    app.wait_for_scans().await;
    assert_eq!(app.get(&format!("/api/libraries/{lib}/items"), &token).await.body["total"], json!(2));
}

#[tokio::test]
async fn shows_progress_and_hubs() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    for (s, e, title) in [(1, 1, "Pilot"), (1, 2, "Second"), (1, 3, "Third"), (2, 1, "Return")] {
        app.media_file(&format!("tv/My Show (2010)/Season {s:02}/My Show - S{s:02}E{e:02} - {title}.mkv"), 1024);
    }
    let lib = app.library(&token, "TV", "shows", "tv").await;

    let shows = app.get(&format!("/api/libraries/{lib}/items"), &token).await;
    assert_eq!(titles(&shows.body["items"]), vec!["My Show"]);
    let show = &shows.body["items"][0];
    assert_eq!(show["leaf_count"], json!(4));
    assert_eq!(show["child_count"], json!(2));
    assert_eq!(show["viewed_leaf_count"], json!(0));
    let show_id = show["id"].as_i64().unwrap();

    let seasons = app.get(&format!("/api/items/{show_id}/children"), &token).await;
    assert_eq!(titles(&seasons.body["items"]), vec!["Season 1", "Season 2"]);
    let season1 = seasons.body["items"][0]["id"].as_i64().unwrap();
    let episodes = app.get(&format!("/api/items/{season1}/children"), &token).await;
    assert_eq!(titles(&episodes.body["items"]), vec!["Pilot", "Second", "Third"]);
    let pilot = episodes.body["items"][0]["id"].as_i64().unwrap();
    let second = episodes.body["items"][1]["id"].as_i64().unwrap();
    assert_eq!(episodes.body["items"][0]["parent_index"], json!(1));
    assert_eq!(episodes.body["items"][0]["grandparent_title"], json!("My Show"));

    // On deck starts at the pilot; next follows the episode order across seasons.
    let deck = app.get(&format!("/api/items/{show_id}/on-deck"), &token).await;
    assert_eq!(deck.body["item"]["title"], json!("Pilot"));
    let third = episodes.body["items"][2]["id"].as_i64().unwrap();
    let next = app.get(&format!("/api/items/{third}/next"), &token).await;
    assert_eq!(next.body["item"]["title"], json!("Return"));

    // Finish the pilot, start the second episode.
    let r = app
        .post(
            &format!("/api/items/{pilot}/progress"),
            &token,
            json!({"position_ms": 2_900_000, "duration_ms": 3_000_000, "state": "stopped"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = app
        .post(
            &format!("/api/items/{second}/progress"),
            &token,
            json!({"position_ms": 600_000, "duration_ms": 3_000_000, "state": "paused"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    let pilot_detail = app.get(&format!("/api/items/{pilot}"), &token).await;
    assert_eq!(pilot_detail.body["user_state"]["watched"], json!(true));
    assert_eq!(pilot_detail.body["user_state"]["play_count"], json!(1));
    assert_eq!(pilot_detail.body["user_state"]["position_ms"], json!(0));

    let home = app.get("/api/hubs/home", &token).await;
    let hubs = home.body["hubs"].as_array().unwrap();
    let cw = hubs.iter().find(|h| h["key"] == "continue_watching").expect("continue watching hub");
    assert_eq!(titles(&cw["items"]), vec!["Second"]);
    assert_eq!(cw["items"][0]["user_state"]["position_ms"], json!(600_000));
    assert!(hubs.iter().any(|h| h["key"] == format!("recently_added:{lib}")));

    // Watching the pilot again part of the way keeps it watched, and out of
    // Continue Watching: apps play a watched item from its start.
    let r = app
        .post(
            &format!("/api/items/{pilot}/progress"),
            &token,
            json!({"position_ms": 120_000, "duration_ms": 3_000_000, "state": "stopped"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let home = app.get("/api/hubs/home", &token).await;
    let cw = home.body["hubs"].as_array().unwrap().iter().find(|h| h["key"] == "continue_watching").cloned().unwrap();
    assert_eq!(titles(&cw["items"]), vec!["Second"]);

    let deck = app.get(&format!("/api/items/{show_id}/on-deck"), &token).await;
    assert_eq!(deck.body["item"]["title"], json!("Second"));

    let lib_hubs = app.get(&format!("/api/libraries/{lib}/hubs"), &token).await;
    assert!(lib_hubs.body["hubs"].as_array().unwrap().iter().any(|h| h["key"] == format!("continue_watching:{lib}")));

    // Marking the show watched covers every episode; unwatching clears them.
    assert_eq!(
        app.post(&format!("/api/items/{show_id}/watched"), &token, json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    let show = app.get(&format!("/api/items/{show_id}"), &token).await;
    assert_eq!(show.body["viewed_leaf_count"], json!(4));
    assert_eq!(show.body["user_state"]["watched"], json!(true));
    let unwatched = app.get(&format!("/api/libraries/{lib}/items?watched=false"), &token).await;
    assert_eq!(unwatched.body["total"], json!(0));
    let watched = app.get(&format!("/api/libraries/{lib}/items?watched=true"), &token).await;
    assert_eq!(watched.body["total"], json!(1));
    assert_eq!(
        app.post(&format!("/api/items/{show_id}/unwatched"), &token, json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    let show = app.get(&format!("/api/items/{show_id}"), &token).await;
    assert_eq!(show.body["viewed_leaf_count"], json!(0));
}

#[tokio::test]
async fn music_hierarchy_from_folders() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    app.media_file("music/Artist A/Album One (2001)/01 - Intro.mp3", 100);
    app.media_file("music/Artist A/Album One (2001)/02 - Song.mp3", 100);
    app.media_file("music/Artist B/Other/1-01 First.flac", 100);
    let lib = app.library(&token, "Music", "music", "music").await;

    let artists = app.get(&format!("/api/libraries/{lib}/items"), &token).await;
    assert_eq!(titles(&artists.body["items"]), vec!["Artist A", "Artist B"]);
    let artist = artists.body["items"][0]["id"].as_i64().unwrap();
    let albums = app.get(&format!("/api/items/{artist}/children"), &token).await;
    assert_eq!(titles(&albums.body["items"]), vec!["Album One"]);
    assert_eq!(albums.body["items"][0]["year"], json!(2001));
    let album = albums.body["items"][0]["id"].as_i64().unwrap();
    let tracks = app.get(&format!("/api/items/{album}/children"), &token).await;
    assert_eq!(titles(&tracks.body["items"]), vec!["Intro", "Song"]);
    assert_eq!(tracks.body["items"][1]["index"], json!(2));
    assert_eq!(tracks.body["items"][0]["grandparent_title"], json!("Artist A"));
}

#[tokio::test]
async fn direct_play_supports_ranges_and_transcode_needs_the_transcoder() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    app.media_file("movies/Clip (2020).mp4", 10_000);
    app.media_file("movies/Other (2020).mkv", 10_000);
    let lib = app.library(&token, "Movies", "movies", "movies").await;
    let items = app.get(&format!("/api/libraries/{lib}/items"), &token).await;
    let id_of = |title: &str| {
        items.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let caps = json!({"containers": ["mp4", "webm"], "video_codecs": ["h264"], "audio_codecs": ["aac"]});

    let play = app.post("/api/playback", &token, json!({"item_id": id_of("Clip"), "caps": caps})).await;
    assert_eq!(play.status, StatusCode::OK, "{:?}", play.body);
    assert_eq!(play.body["method"], json!("direct"));
    let url = play.body["url"].as_str().unwrap().to_owned();
    let ranged = app.call(Method::GET, &url, Some(&token), None, &[("range", "bytes=100-199")]).await;
    assert_eq!(ranged.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(ranged.bytes.len(), 100);

    let activity = app.get("/api/activity", &token).await;
    assert_eq!(activity.body["sessions"].as_array().unwrap().len(), 1);
    let session = play.body["session_id"].as_str().unwrap();
    let stop = app.call(Method::DELETE, &format!("/api/playback/{session}"), Some(&token), None, &[]).await;
    assert_eq!(stop.status, StatusCode::NO_CONTENT);

    // MKV needs HLS from the transcoder, which is not running in tests.
    let play = app.post("/api/playback", &token, json!({"item_id": id_of("Other"), "caps": caps})).await;
    assert_eq!(play.status, StatusCode::SERVICE_UNAVAILABLE, "{:?}", play.body);

    // ...unless the player asks for the original file anyway.
    let body = json!({"item_id": id_of("Other"), "caps": caps, "mode": "direct", "client_id": "tab-1"});
    let play = app.post("/api/playback", &token, body).await;
    assert_eq!(play.status, StatusCode::OK, "{:?}", play.body);
    assert_eq!(play.body["method"], json!("direct"));

    // A client plays one thing at a time: its next session replaces the previous one.
    let first = play.body["session_id"].as_str().unwrap().to_owned();
    let body = json!({"item_id": id_of("Clip"), "caps": caps, "client_id": "tab-1"});
    let play = app.post("/api/playback", &token, body).await;
    assert_eq!(play.status, StatusCode::OK, "{:?}", play.body);
    let activity = app.get("/api/activity", &token).await;
    let sessions = activity.body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_ne!(play.body["session_id"].as_str().unwrap(), first);
    let other_client = json!({"item_id": id_of("Clip"), "caps": caps, "client_id": "tv"});
    app.post("/api/playback", &token, other_client).await;
    let activity = app.get("/api/activity", &token).await;
    assert_eq!(activity.body["sessions"].as_array().unwrap().len(), 2);

    // One prepared while the last plays on leaves that one to the client to end.
    let playing = play.body["session_id"].as_str().unwrap().to_owned();
    let body = json!({"item_id": id_of("Clip"), "caps": caps, "client_id": "tab-1", "keep_previous": true});
    let prepared = app.post("/api/playback", &token, body).await;
    assert_eq!(prepared.status, StatusCode::OK, "{:?}", prepared.body);
    let activity = app.get("/api/activity", &token).await;
    let sessions = activity.body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 3, "{sessions:?}");
    assert!(sessions.iter().any(|s| s["session_id"] == json!(playing)), "{sessions:?}");

    let system = app.get("/api/system", &token).await;
    assert_eq!(system.body["transcoder"]["available"], json!(false));
    assert_eq!(system.body["counts"]["movies"], json!(2));
    let transcoder = app.get("/api/transcoder", &token).await;
    assert_eq!(transcoder.body["available"], json!(false));
}

#[tokio::test]
async fn settings_validation_and_folder_browser() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    let settings = app.get("/api/settings", &token).await;
    assert_eq!(settings.body["hwaccel"], json!("auto"));
    assert_eq!(settings.body["tonemapping"], json!("auto"));

    // One this computer's system has (tests/settings_caps.rs has the rest).
    let accel = if cfg!(target_os = "macos") { "videotoolbox" } else { "nvenc" };
    let updated = app
        .call(Method::PUT, "/api/settings", Some(&token), Some(json!({"max_transcodes": 2, "hwaccel": accel})), &[])
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.body["max_transcodes"], json!(2));
    assert_eq!(updated.body["hwaccel"], json!(accel));
    let bad = app.call(Method::PUT, "/api/settings", Some(&token), Some(json!({"max_transcodes": 0})), &[]).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let bad = app.call(Method::PUT, "/api/settings", Some(&token), Some(json!({"hwaccel": "magic"})), &[]).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);

    let roots = app.get("/api/fs", &token).await;
    assert!(!roots.body["entries"].as_array().unwrap().is_empty());
    std::fs::create_dir_all(app.media.path().join("Alpha")).unwrap();
    std::fs::create_dir_all(app.media.path().join("beta")).unwrap();
    std::fs::write(app.media.path().join("file.txt"), b"x").unwrap();
    let listing = app.get(&format!("/api/fs?path={}", urlencode(app.media.path())), &token).await;
    let names: Vec<_> =
        listing.body["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, vec!["Alpha", "beta"]);

    let bad_lib =
        app.post("/api/libraries", &token, json!({"name": "X", "kind": "movies", "paths": ["relative/path"]})).await;
    assert_eq!(bad_lib.status, StatusCode::BAD_REQUEST);
}

fn urlencode(path: &Path) -> String {
    path.to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::test]
async fn subtitle_choice_carries_over_to_the_next_episode() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    for (e, title) in [(1, "Pilot"), (2, "Second")] {
        let base = format!("tv/Subbed (2020)/Season 01/Subbed - S01E{e:02} - {title}");
        app.media_file(&format!("{base}.mkv"), 1024);
        app.media_file(&format!("{base}.en.srt"), 64);
        app.media_file(&format!("{base}.fr.srt"), 64);
    }
    let lib = app.library(&token, "TV", "shows", "tv").await;
    let show = app.get(&format!("/api/libraries/{lib}/items"), &token).await.body["items"][0]["id"].as_i64().unwrap();
    let season = app.get(&format!("/api/items/{show}/children"), &token).await.body["items"][0]["id"].as_i64().unwrap();
    let episodes = app.get(&format!("/api/items/{season}/children"), &token).await;
    let (pilot, second) =
        (episodes.body["items"][0]["id"].as_i64().unwrap(), episodes.body["items"][1]["id"].as_i64().unwrap());

    let subtitle = |detail: &Value, language: &str| -> i64 {
        detail["files"][0]["streams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "subtitle" && s["language"] == language)
            .unwrap()["index"]
            .as_i64()
            .unwrap()
    };
    let detail = app.get(&format!("/api/items/{pilot}"), &token).await.body;
    assert_eq!(detail["files"][0]["selected_subtitle"], Value::Null, "nothing chosen yet");
    let english = subtitle(&detail, "eng");

    let chose = app
        .call(
            Method::PUT,
            &format!("/api/items/{pilot}/streams"),
            Some(&token),
            Some(json!({"subtitle_index": english})),
            &[],
        )
        .await;
    assert_eq!(chose.status, StatusCode::NO_CONTENT, "{:?}", chose.body);

    // The next episode starts with its own English track.
    let next = app.get(&format!("/api/items/{second}"), &token).await.body;
    assert_eq!(next["files"][0]["selected_subtitle"], json!(subtitle(&next, "eng")));

    // Turning subtitles off is remembered too.
    app.call(
        Method::PUT,
        &format!("/api/items/{second}/streams"),
        Some(&token),
        Some(json!({"subtitle_index": null})),
        &[],
    )
    .await;
    let pilot_again = app.get(&format!("/api/items/{pilot}"), &token).await.body;
    assert_eq!(pilot_again["files"][0]["selected_subtitle"], Value::Null);
}

async fn set_rating(app: &TestApp, title: &str, rating: Option<&str>) {
    let age = rating.and_then(pley::ratings::rating_age);
    sqlx::query("UPDATE items SET content_rating = ?, rating_age = ? WHERE title = ?")
        .bind(rating)
        .bind(age)
        .bind(title)
        .execute(&app.state.db.write)
        .await
        .unwrap();
}

#[tokio::test]
async fn library_access_and_age_limits() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Adult Film (2020).mkv", 1024);
    app.media_file("movies/Family Film (2019).mkv", 1024);
    app.media_file("movies/Home Video (2015).mkv", 1024);
    app.media_file("kids/Cartoon (2018).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let kids = app.library(&admin, "Kids", "movies", "kids").await;
    set_rating(&app, "Adult Film", Some("R")).await;
    set_rating(&app, "Family Film", Some("PG")).await;
    set_rating(&app, "Cartoon", Some("G")).await;

    let created = app
        .post(
            "/api/users",
            &admin,
            json!({"username": "kid", "managed": true, "all_libraries": false, "library_ids": [movies, kids], "max_age": 12}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["library_ids"], json!([movies, kids]));
    let kid_id = created.body["id"].as_i64().unwrap();
    let switched = app.post("/api/auth/switch", &admin, json!({"user_id": kid_id})).await;
    assert_eq!(switched.status, StatusCode::OK, "{:?}", switched.body);
    let kid = switched.body["token"].as_str().unwrap().to_owned();
    // Switching signed the admin out on this "device".
    let admin = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "admin", "password": "secret-one"})), &[])
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_owned();

    // R-rated and unrated titles are hidden; the libraries are visible.
    let libs = app.get("/api/libraries", &kid).await;
    assert_eq!(libs.body.as_array().unwrap().len(), 2);
    let items = app.get(&format!("/api/libraries/{movies}/items"), &kid).await;
    assert_eq!(titles(&items.body["items"]), vec!["Family Film"]);
    let all = app.get(&format!("/api/libraries/{movies}/items"), &admin).await;
    let id_of = |title: &str| {
        all.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let adult = id_of("Adult Film");
    assert_eq!(app.get(&format!("/api/items/{adult}"), &kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/items/{adult}/images/poster"), &kid).await.status, StatusCode::NOT_FOUND);
    let found = app.get("/api/search?q=film", &kid).await;
    assert_eq!(found.status, StatusCode::OK);
    assert!(!found.bytes.windows(10).any(|w| w == b"Adult Film"), "search leaks hidden titles");

    // Taking a library away hides it completely.
    let patched = app
        .call(Method::PATCH, &format!("/api/users/{kid_id}"), Some(&admin), Some(json!({"library_ids": [kids]})), &[])
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{:?}", patched.body);
    assert_eq!(app.get(&format!("/api/libraries/{movies}/items"), &kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/libraries", &kid).await.body.as_array().unwrap().len(), 1);

    // Lifting the restrictions shows everything.
    app.call(
        Method::PATCH,
        &format!("/api/users/{kid_id}"),
        Some(&admin),
        Some(json!({"max_age": null, "all_libraries": true})),
        &[],
    )
    .await;
    let items = app.get(&format!("/api/libraries/{movies}/items"), &kid).await;
    assert_eq!(titles(&items.body["items"]), vec!["Adult Film", "Family Film", "Home Video"]);
}

#[tokio::test]
async fn profiles_switch_with_pins() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let created = app
        .post("/api/users", &admin, json!({"username": "kid", "display_name": "Kiddo", "managed": true, "pin": "1234"}))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["name"], json!("Kiddo"));
    assert_eq!(created.body["has_pin"], json!(true));
    let kid_id = created.body["id"].as_i64().unwrap();

    let profiles = app.get("/api/auth/profiles", &admin).await;
    assert_eq!(profiles.body.as_array().unwrap().len(), 2);
    let admin_id = profiles.body.as_array().unwrap().iter().find(|u| u["is_admin"] == json!(true)).unwrap()["id"]
        .as_i64()
        .unwrap();

    let no_pin = app.post("/api/auth/switch", &admin, json!({"user_id": kid_id})).await;
    assert_eq!(no_pin.status, StatusCode::FORBIDDEN);
    let wrong = app.post("/api/auth/switch", &admin, json!({"user_id": kid_id, "pin": "9999"})).await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    let ok = app.post("/api/auth/switch", &admin, json!({"user_id": kid_id, "pin": "1234"})).await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    let kid = ok.body["token"].as_str().unwrap().to_owned();
    // The device's previous profile session is gone.
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &kid).await.body["id"], json!(kid_id));

    // Back to the administrator: its password is needed (it has no PIN).
    let denied = app.post("/api/auth/switch", &kid, json!({"user_id": admin_id})).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let back = app.post("/api/auth/switch", &kid, json!({"user_id": admin_id, "password": "secret-one"})).await;
    assert_eq!(back.status, StatusCode::OK, "{:?}", back.body);
    let admin = back.body["token"].as_str().unwrap().to_owned();

    // Guessing a PIN gets throttled.
    let mut statuses = Vec::new();
    for _ in 0..7 {
        statuses.push(app.post("/api/auth/switch", &admin, json!({"user_id": kid_id, "pin": "0000"})).await.status);
    }
    assert_eq!(statuses.last(), Some(&StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");

    // Managed profiles can't sign in with a password.
    let login = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "kid", "password": "!managed"})), &[])
        .await;
    assert_eq!(login.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invitations_create_friend_accounts() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Shared (2020).mkv", 1024);
    app.media_file("private/Secret (2021).mkv", 1024);
    let shared = app.library(&admin, "Shared", "movies", "movies").await;
    app.library(&admin, "Private", "movies", "private").await;

    let invite = app.post("/api/invites", &admin, json!({"all_libraries": false, "library_ids": [shared]})).await;
    assert_eq!(invite.status, StatusCode::CREATED, "{:?}", invite.body);
    let code = invite.body["code"].as_str().unwrap().to_owned();

    let check = app.call(Method::GET, &format!("/api/invites/{code}"), None, None, &[]).await;
    assert_eq!(check.status, StatusCode::OK);
    let accept = app
        .call(
            Method::POST,
            &format!("/api/invites/{code}/accept"),
            None,
            Some(json!({"username": "friend", "password": "pass1234"})),
            &[],
        )
        .await;
    assert_eq!(accept.status, StatusCode::OK, "{:?}", accept.body);
    assert_eq!(accept.body["user"]["home"], json!(false));
    let friend = accept.body["token"].as_str().unwrap().to_owned();
    let libs = app.get("/api/libraries", &friend).await;
    assert_eq!(libs.body.as_array().unwrap().len(), 1);
    assert_eq!(app.get("/api/auth/profiles", &friend).await.body, json!([]));

    // Single use.
    let again = app
        .call(
            Method::POST,
            &format!("/api/invites/{code}/accept"),
            None,
            Some(json!({"username": "friend2", "password": "pass1234"})),
            &[],
        )
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

async fn login_as(app: &TestApp, username: &str, password: &str, agent: &str) -> String {
    let r = app
        .call(
            Method::POST,
            "/api/auth/login",
            None,
            Some(json!({"username": username, "password": password})),
            &[("user-agent", agent)],
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    r.body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn devices_can_be_signed_out_and_accounts_disabled() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.post("/api/users", &admin, json!({"username": "bob", "password": "bobpass1", "home": false})).await;
    let laptop = login_as(&app, "bob", "bobpass1", "Mozilla/5.0 (Windows NT 10.0) Chrome/140.0 Safari/537.36").await;
    let phone = login_as(
        &app,
        "bob",
        "bobpass1",
        "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Version/18.0 Safari/604.1",
    )
    .await;

    let devices = app.get("/api/me/devices", &laptop).await;
    let list = devices.body.as_array().unwrap();
    assert_eq!(list.len(), 2);
    let current = list.iter().find(|d| d["current"] == json!(true)).unwrap();
    assert_eq!(current["name"], json!("Chrome on Windows"));
    let other = list.iter().find(|d| d["current"] == json!(false)).unwrap()["id"].as_str().unwrap().to_owned();
    let out = app.call(Method::DELETE, &format!("/api/me/devices/{other}"), Some(&laptop), None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &phone).await.status, StatusCode::UNAUTHORIZED);

    let bob_id = app.get("/api/auth/me", &laptop).await.body["id"].as_i64().unwrap();
    app.call(Method::PATCH, &format!("/api/users/{bob_id}"), Some(&admin), Some(json!({"disabled": true})), &[]).await;
    assert_eq!(app.get("/api/auth/me", &laptop).await.status, StatusCode::UNAUTHORIZED);
    let login = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "bob", "password": "bobpass1"})), &[])
        .await;
    assert_eq!(login.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn avatars_upload_and_serve() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(300, 200, image::Rgb([200, 30, 90])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let req = Request::builder()
        .method(Method::PUT)
        .uri("/api/me/avatar")
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(png))
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap()).unwrap();
    let url = body["avatar_url"].as_str().unwrap().to_owned();
    let image = app.get(&url, &admin).await;
    assert_eq!(image.status, StatusCode::OK);
    assert_eq!(image.headers[header::CONTENT_TYPE], "image/jpeg");

    let patched = app.call(Method::PATCH, "/api/me", Some(&admin), Some(json!({"avatar_color": "#38bdf8"})), &[]).await;
    assert_eq!(patched.body["avatar_color"], json!("#38bdf8"));
    let bad = app.call(Method::PATCH, "/api/me", Some(&admin), Some(json!({"avatar_color": "blue"})), &[]).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn metadata_edits_survive_rescans() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alein (1979).mkv", 1024);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let items = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    let id = items.body["items"][0]["id"].as_i64().unwrap();

    let edited = app
        .call(
            Method::PATCH,
            &format!("/api/items/{id}"),
            Some(&admin),
            Some(json!({"title": "Alien", "summary": "In space no one can hear you scream.", "genres": ["Horror", "Sci-Fi"], "content_rating": "R"})),
            &[],
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.body);
    assert_eq!(edited.body["title"], json!("Alien"));
    assert_eq!(edited.body["genres"], json!(["Horror", "Sci-Fi"]));
    assert_eq!(edited.body["locked_fields"], json!(["content_rating", "genres", "summary", "title"]));
    let bad =
        app.call(Method::PATCH, &format!("/api/items/{id}"), Some(&admin), Some(json!({"year": "soon"})), &[]).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let path =
        app.call(Method::PATCH, &format!("/api/items/{id}"), Some(&admin), Some(json!({"path": "/x"})), &[]).await;
    assert_eq!(path.status, StatusCode::BAD_REQUEST);

    // The file changes, so the scan rewrites the item from the file name; the edit wins.
    app.media_file("movies/Alein (1979).mkv", 2048);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    let after = app.get(&format!("/api/items/{id}"), &admin).await;
    assert_eq!(after.body["title"], json!("Alien"));
    assert_eq!(after.body["content_rating"], json!("R"));

    // Unlocking hands the field back to the scanner / metadata agent.
    app.call(Method::PATCH, &format!("/api/items/{id}"), Some(&admin), Some(json!({"title": null})), &[]).await;
    app.media_file("movies/Alein (1979).mkv", 4096);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    let unlocked = app.get(&format!("/api/items/{id}"), &admin).await;
    assert_eq!(unlocked.body["title"], json!("Alein"));
    assert_eq!(unlocked.body["locked_fields"], json!(["content_rating", "genres", "summary"]));

    // Non-admins can't edit.
    app.post("/api/users", &admin, json!({"username": "viewer", "password": "viewer-pass"})).await;
    let viewer = login_as(&app, "viewer", "viewer-pass", "test").await;
    let denied =
        app.call(Method::PATCH, &format!("/api/items/{id}"), Some(&viewer), Some(json!({"title": "x"})), &[]).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);

    // Uploaded artwork replaces the poster and is served.
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(200, 300, image::Rgb([10, 120, 200])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/items/{id}/artwork/poster/upload"))
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(png))
        .unwrap();
    let resp = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap()).unwrap();
    let poster = body["poster_url"].as_str().unwrap().to_owned();
    let image = app.get(&poster, &admin).await;
    assert_eq!(image.status, StatusCode::OK);
    assert_eq!(image.headers[header::CONTENT_TYPE], "image/jpeg");

    // Without a TMDB key, matching explains what is missing.
    let matches = app.get(&format!("/api/items/{id}/matches"), &admin).await;
    assert_eq!(matches.status, StatusCode::BAD_REQUEST);
    assert!(matches.body["message"].as_str().unwrap().contains("TMDB"));
}

fn credit(tmdb_id: i64, name: &str, role: Role, character: Option<&str>, photo: Option<&str>) -> NewCredit {
    NewCredit {
        person: PersonId::Tmdb(tmdb_id),
        name: name.into(),
        photo: photo.map(str::to_owned),
        role,
        character: character.map(str::to_owned),
    }
}

#[tokio::test]
async fn cast_and_crew() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/The Matrix (1999).mkv", 1024);
    // A root left empty looks offline and is not purged.
    app.media_file("movies/Speed (1994).mkv", 1024);
    app.media_file("hidden/John Wick (2014).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let hidden = app.library(&admin, "Hidden", "movies", "hidden").await;
    let id_in = |page: &Reply, title: &str| {
        page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let matrix = id_in(&app.get(&format!("/api/libraries/{movies}/items"), &admin).await, "The Matrix");
    let wick = id_in(&app.get(&format!("/api/libraries/{hidden}/items"), &admin).await, "John Wick");

    // Credits come from TMDB in real use.
    let keanu_photo = "https://image.tmdb.org/t/p/h632/keanu.jpg";
    let keanu = credit(6384, "Keanu Reeves", Role::Actor, Some("Neo"), Some(keanu_photo));
    people::replace(
        &app.state.db,
        matrix,
        &[
            keanu.clone(),
            credit(530, "Carrie-Anne Moss", Role::Actor, Some("Trinity"), None),
            credit(9339, "Lana Wachowski", Role::Director, None, None),
        ],
    )
    .await
    .unwrap();
    people::replace(&app.state.db, wick, &[credit(6384, "Keanu Reeves", Role::Actor, Some("John Wick"), None)])
        .await
        .unwrap();

    let detail = app.get(&format!("/api/items/{matrix}"), &admin).await;
    let cast = detail.body["cast"].as_array().unwrap();
    assert_eq!(cast.len(), 2);
    assert_eq!(cast[0]["name"], json!("Keanu Reeves"));
    assert_eq!(cast[0]["character"], json!("Neo"));
    let photo_url = cast[0]["photo_url"].as_str().unwrap().to_owned();
    assert!(photo_url.contains("/photo?v="), "{photo_url}");
    assert!(cast[1].get("photo_url").is_none());
    assert_eq!(detail.body["crew"][0]["role"], json!("director"));

    let person = cast[0]["id"].as_i64().unwrap();
    let page = app.get(&format!("/api/people/{person}"), &admin).await;
    assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
    assert_eq!(page.body["name"], json!("Keanu Reeves"));
    let credits = page.body["credits"].as_array().unwrap();
    assert_eq!(credits.len(), 2);
    assert_eq!(credits[0]["item"]["title"], json!("John Wick"), "newest first");
    assert_eq!(credits[1]["roles"], json!([{"role": "actor", "character": "Neo"}]));

    let search = app.get("/api/search?q=keanu%20ree", &admin).await;
    assert_eq!(search.body["people"][0]["id"], json!(person));
    assert_eq!(app.get("/api/search?q=eanu", &admin).await.body["people"], json!([]), "word prefixes only");

    // Photos are downloaded on first use; this one is already in the store.
    let blob = app.state.store.put(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0], "jpg").await.unwrap();
    sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES (?, ?, 0)")
        .bind(keanu_photo)
        .bind(&blob)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let photo = app.get(&photo_url, &admin).await;
    assert_eq!(photo.status, StatusCode::OK);
    assert_eq!(photo.headers[header::CONTENT_TYPE], "image/jpeg");
    let moss = cast[1]["id"].as_i64().unwrap();
    assert_eq!(app.get(&format!("/api/people/{moss}/photo"), &admin).await.status, StatusCode::NOT_FOUND);

    // Someone who can't see the Matrix library sees Keanu in John Wick only,
    // and nothing of people only credited there.
    let created = app
        .post(
            "/api/users",
            &admin,
            json!({"username": "guest", "password": "guestpass", "all_libraries": false, "library_ids": [hidden]}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let guest = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "guest", "password": "guestpass"})), &[])
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let page = app.get(&format!("/api/people/{person}"), &guest).await;
    let items: Vec<&Value> = page.body["credits"].as_array().unwrap().iter().map(|c| &c["item"]).collect();
    assert_eq!(titles(&json!(items)), vec!["John Wick"]);
    assert_eq!(app.get(&format!("/api/people/{moss}"), &guest).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/search?q=carrie", &guest).await.body["people"], json!([]));

    // Removing the movie forgets people credited nowhere else.
    std::fs::remove_file(app.media.path().join("movies/The Matrix (1999).mkv")).unwrap();
    app.post(&format!("/api/libraries/{movies}/scan"), &admin, json!({"force": false})).await;
    app.wait_for_scans().await;
    assert_eq!(app.get(&format!("/api/people/{moss}"), &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/people/{person}"), &admin).await.status, StatusCode::OK);
}

#[tokio::test]
async fn watch_history() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 1024);
    app.media_file("movies/Brazil (1985).mkv", 1024);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    let id_of = |title: &str| {
        page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let (alien, brazil) = (id_of("Alien"), id_of("Brazil"));
    let progress = |id: i64, position_ms: i64, token: String| {
        let app = &app;
        async move {
            let body = json!({"position_ms": position_ms, "duration_ms": 100_000, "state": "playing"});
            let reply = app
                .call(
                    Method::POST,
                    &format!("/api/items/{id}/progress"),
                    Some(&token),
                    Some(body),
                    &[("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/130.0 Safari/537.36")],
                )
                .await;
            assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
        }
    };

    // Reports keep coming after the end is reached: one play.
    progress(alien, 40_000, admin.clone()).await;
    progress(alien, 95_000, admin.clone()).await;
    progress(alien, 99_000, admin.clone()).await;
    // Watching again counts again; marking watched by hand doesn't.
    progress(alien, 30_000, admin.clone()).await;
    progress(alien, 92_000, admin.clone()).await;
    app.post(&format!("/api/items/{brazil}/watched"), &admin, json!({})).await;

    let history = app.get("/api/history", &admin).await;
    assert_eq!(history.status, StatusCode::OK, "{:?}", history.body);
    assert_eq!(history.body["total"], json!(2));
    let entry = &history.body["entries"][0];
    assert_eq!(entry["item"]["title"], json!("Alien"));
    assert_eq!(entry["device"], json!("Chrome on Windows"));
    assert_eq!(entry["user"]["name"], json!("admin"));
    let alien_detail = app.get(&format!("/api/items/{alien}"), &admin).await;
    assert_eq!(alien_detail.body["user_state"]["play_count"], json!(2));
    let paged = app.get("/api/history?limit=1&offset=1", &admin).await;
    assert_eq!(paged.body["total"], json!(2));
    assert_eq!(paged.body["entries"].as_array().unwrap().len(), 1);
    assert_eq!(app.get(&format!("/api/history?item_id={brazil}"), &admin).await.body["total"], json!(0));

    // Others see their own plays; administrators can see everyone's.
    let created =
        app.post("/api/users", &admin, json!({"username": "bob", "password": "bobpass1", "all_libraries": true})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let bob = app
        .call(Method::POST, "/api/auth/login", None, Some(json!({"username": "bob", "password": "bobpass1"})), &[])
        .await
        .body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    progress(brazil, 97_000, bob.clone()).await;
    let own = app.get("/api/history?all=1", &bob).await;
    assert_eq!(
        titles(&json!(own.body["entries"].as_array().unwrap().iter().map(|e| &e["item"]).collect::<Vec<_>>())),
        vec!["Brazil"]
    );
    assert_eq!(app.get("/api/history?all=1", &admin).await.body["total"], json!(3));
    let bobs_id = created.body["id"].as_i64().unwrap();
    assert_eq!(app.get(&format!("/api/history?user_id={bobs_id}"), &admin).await.body["total"], json!(1));

    // Entries can be removed by their user or an administrator.
    let admins_entry = history.body["entries"][0]["id"].as_i64().unwrap();
    let reply = app.call(Method::DELETE, &format!("/api/history/{admins_entry}"), Some(&bob), None, &[]).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    let reply = app.call(Method::DELETE, &format!("/api/history/{admins_entry}"), Some(&admin), None, &[]).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/history", &admin).await.body["total"], json!(1));
}

#[tokio::test]
async fn user_ratings() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "Casablanca (1942)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    let id_of = |title: &str| {
        page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let rate = |id: i64, rating: Value| {
        let app = &app;
        let admin = admin.clone();
        async move {
            app.call(
                Method::PUT,
                &format!("/api/items/{id}/rating"),
                Some(&admin),
                Some(json!({"rating": rating})),
                &[],
            )
            .await
        }
    };
    assert_eq!(rate(id_of("Brazil"), json!(9)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(rate(id_of("Alien"), json!(6.6)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(rate(id_of("Alien"), json!(11)).await.status, StatusCode::BAD_REQUEST);

    let alien = app.get(&format!("/api/items/{}", id_of("Alien")), &admin).await;
    assert_eq!(alien.body["user_state"]["rating"], json!(7.0), "rounded to a half star");
    let sorted = app.get(&format!("/api/libraries/{lib}/items?sort=user_rating"), &admin).await;
    assert_eq!(titles(&sorted.body["items"]), vec!["Brazil", "Alien", "Casablanca"], "unrated last");

    assert_eq!(rate(id_of("Brazil"), Value::Null).await.status, StatusCode::NO_CONTENT);
    let brazil = app.get(&format!("/api/items/{}", id_of("Brazil")), &admin).await;
    assert!(brazil.body["user_state"].get("rating").is_none());
}

fn png() -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbImage::from_pixel(8, 8, image::Rgb([30, 140, 220]))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

#[tokio::test]
async fn pictures_are_stored_by_content_and_cleaned_up() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 4096);
    app.media_file("movies/Brazil (1985).mkv", 4096);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    let id_of = |title: &str| {
        page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let (alien, brazil) = (id_of("Alien"), id_of("Brazil"));
    let db = &app.state.db;

    // Pictures kept the old way (absolute paths in the metadata folder) move into the store.
    let metadata_dir = app.state.config.metadata_dir();
    let old = metadata_dir.join("items").join(alien.to_string()).join("poster.png");
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    std::fs::write(&old, png()).unwrap();
    sqlx::query("UPDATE items SET poster = ? WHERE id = ?")
        .bind(old.to_string_lossy().into_owned())
        .bind(alien)
        .execute(&db.write)
        .await
        .unwrap();
    app.state.store.migrate_legacy(&metadata_dir).await.unwrap();
    let poster: String =
        sqlx::query_scalar("SELECT poster FROM items WHERE id = ?").bind(alien).fetch_one(&db.read).await.unwrap();
    assert!(poster.starts_with("blob:"), "{poster}");
    assert!(!old.exists());
    assert!(!metadata_dir.join("items").exists(), "old folders are removed");
    let image = app.get(&format!("/api/items/{alien}/images/poster"), &admin).await;
    assert_eq!(image.status, StatusCode::OK);
    assert_eq!(image.headers[header::CONTENT_TYPE], "image/png");

    // The same upload for two items is stored once.
    for id in [alien, brazil] {
        let reply =
            app.call_raw(Method::POST, &format!("/api/items/{id}/artwork/backdrop/upload"), &admin, png()).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    }
    let backdrops: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT value FROM item_overrides WHERE field = 'backdrop' AND item_id IN (?, ?)")
            .bind(alien)
            .bind(brazil)
            .fetch_all(&db.read)
            .await
            .unwrap();
    assert_eq!(backdrops.len(), 1);

    // Resetting one keeps the picture (the other uses it); resetting both frees it.
    let usage = app.state.store.usage().await.unwrap();
    assert_eq!(usage.blobs, 2);
    for id in [alien, brazil] {
        let reply =
            app.call(Method::DELETE, &format!("/api/items/{id}/artwork/backdrop"), Some(&admin), None, &[]).await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    }
    let report = app.state.store.clean_older_than(std::time::Duration::ZERO).await.unwrap();
    assert_eq!(report.blobs_removed, 1);
    assert!(app.state.store.exists(&poster), "the poster is still used");

    // Scanned files get a fingerprint (the OpenSubtitles hash).
    app.state.store.fingerprint_pending().await;
    let fingerprints: Vec<Option<String>> =
        sqlx::query_scalar("SELECT fingerprint FROM media_files").fetch_all(&db.read).await.unwrap();
    assert!(fingerprints.iter().all(|f| f.as_ref().is_some_and(|f| f.len() == 16)), "{fingerprints:?}");
}

#[tokio::test]
async fn more_like_this() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Aliens (1986)", "Brazil (1985)", "Casablanca (1942)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    let id_of = |title: &str| {
        page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let db = &app.state.db;
    for (title, genres) in [
        ("Alien", r#"["Horror","Science Fiction"]"#),
        ("Aliens", r#"["Action","Science Fiction"]"#),
        ("Brazil", r#"["Science Fiction","Comedy"]"#),
        ("Casablanca", r#"["Drama","Romance"]"#),
    ] {
        sqlx::query("UPDATE items SET genres = ? WHERE id = ?")
            .bind(genres)
            .bind(id_of(title))
            .execute(&db.write)
            .await
            .unwrap();
    }
    // Aliens shares a director with Alien in this test; Brazil only a genre.
    let director = credit(578, "Ridley Scott", Role::Director, None, None);
    people::replace(db, id_of("Alien"), std::slice::from_ref(&director)).await.unwrap();
    people::replace(db, id_of("Aliens"), &[director]).await.unwrap();

    let related = app.get(&format!("/api/items/{}/related", id_of("Alien")), &admin).await;
    assert_eq!(related.status, StatusCode::OK, "{:?}", related.body);
    assert_eq!(titles(&related.body["items"]), vec!["Aliens", "Brazil"], "Casablanca shares nothing");
}

#[tokio::test]
async fn trailers_and_other_extras() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 4096);
    app.media_file("movies/Alien (1979)/Trailers/Teaser.mkv", 1024);
    app.media_file("movies/Alien (1979)/Behind The Scenes/Making.Of.Alien.mkv", 1024);
    app.media_file("movies/Brazil (1985)/Brazil (1985).mkv", 4096);
    app.media_file("movies/Brazil (1985)/Brazil (1985)-trailer.mkv", 1024);
    // Bonus material without its movie is left out.
    app.media_file("movies/Lost Extras/Trailers/Nothing.mkv", 1024);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;

    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    assert_eq!(titles(&page.body["items"]), vec!["Alien", "Brazil"], "extras are not movies");
    let id_of = |title: &str| {
        page.body["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap()["id"].as_i64().unwrap()
    };
    let alien = app.get(&format!("/api/items/{}", id_of("Alien")), &admin).await;
    let extras = alien.body["extras"].as_array().unwrap().clone();
    assert_eq!(titles(&json!(extras)), vec!["Teaser", "Making Of Alien"], "trailers first");
    assert_eq!(extras[0]["extra_type"], json!("trailer"));
    assert_eq!(extras[1]["extra_type"], json!("behind_the_scenes"));
    let brazil = app.get(&format!("/api/items/{}", id_of("Brazil")), &admin).await;
    assert_eq!(titles(&brazil.body["extras"]), vec!["Trailer"]);

    // An extra plays like a movie and is found through its movie only.
    let trailer = extras[0]["id"].as_i64().unwrap();
    let detail = app.get(&format!("/api/items/{trailer}"), &admin).await;
    assert_eq!(detail.body["kind"], json!("extra"));
    assert_eq!(detail.body["files"].as_array().unwrap().len(), 1);
    let progress = app
        .post(&format!("/api/items/{trailer}/progress"), &admin, json!({"position_ms": 30_000, "duration_ms": 120_000}))
        .await;
    assert_eq!(progress.status, StatusCode::NO_CONTENT, "{:?}", progress.body);
    let play = app.post("/api/playback", &admin, json!({"item_id": trailer, "caps": {}, "mode": "direct"})).await;
    assert_eq!(play.status, StatusCode::OK, "{:?}", play.body);
    assert_eq!(play.body["method"], json!("direct"));
    assert_eq!(app.get("/api/search?q=teaser", &admin).await.body["items"], json!([]));

    // Removing the movie removes its extras.
    std::fs::remove_file(app.media.path().join("movies/Brazil (1985)/Brazil (1985).mkv")).unwrap();
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": false})).await;
    app.wait_for_scans().await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    assert_eq!(titles(&page.body["items"]), vec!["Alien"]);
    let files: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_files").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(files, 3, "Alien, its two extras");
}

#[tokio::test]
async fn show_extras_stay_out_of_seasons() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("tv/Lost (2004)/Season 1/Lost - S01E01.mkv", 2048);
    app.media_file("tv/Lost (2004)/Featurettes/On.Location.mkv", 1024);
    let lib = app.library(&admin, "TV", "shows", "tv").await;
    let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await;
    let show = page.body["items"][0]["id"].as_i64().unwrap();
    assert_eq!(page.body["items"][0]["child_count"], json!(1));
    let detail = app.get(&format!("/api/items/{show}"), &admin).await;
    assert_eq!(titles(&detail.body["extras"]), vec!["On Location"]);
    assert_eq!(detail.body["extras"][0]["extra_type"], json!("featurette"));
    let children = app.get(&format!("/api/items/{show}/children"), &admin).await;
    assert_eq!(titles(&children.body["items"]), vec!["Season 1"]);
}
