//! Metadata providers end to end: libraries are scanned and their metadata
//! fetched from a local stand-in for TVmaze, TheTVDB, MusicBrainz and the
//! Cover Art Archive that answers with the fixtures in
//! `tests/fixtures/providers`. Nothing leaves the machine: the pictures the
//! answers name are stored beforehand, as if downloaded by an earlier
//! refresh.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tower::ServiceExt;

use pley::providers::Endpoints;
use pley::{AppState, Config};

const TVMAZE_SHOW: &str = include_str!("fixtures/providers/tvmaze_show.json");
const TVMAZE_EPISODES: &str = include_str!("fixtures/providers/tvmaze_episodes.json");
const TVMAZE_SEARCH: &str = include_str!("fixtures/providers/tvmaze_search.json");
const TVDB_SERIES: &str = include_str!("fixtures/providers/tvdb_series.json");
const TVDB_EPISODES: &str = include_str!("fixtures/providers/tvdb_episodes.json");
const MB_GROUP_SEARCH: &str = include_str!("fixtures/providers/musicbrainz_release_group_search.json");
const MB_ARTIST: &str = include_str!("fixtures/providers/musicbrainz_artist.json");
const MB_GENRES: &str = include_str!("fixtures/providers/musicbrainz_genres.txt");

const DISCOVERY: &str = "48117b90-a16e-34ca-a514-19c702df1158";
const DAFT_PUNK: &str = "056e4f3e-d505-4dad-8ec1-d04f521cbb56";

// ------------------------------------------------------------------ stand-in services

#[derive(Clone, Default)]
struct Mock {
    /// "GET /tvmaze/shows/4242?…" for every request.
    requests: Arc<Mutex<Vec<String>>>,
    tvdb_logins: Arc<AtomicUsize>,
}

impl Mock {
    fn requested(&self, prefix: &str) -> usize {
        self.requests.lock().iter().filter(|r| r.starts_with(prefix)).count()
    }
}

fn json_reply(body: &str) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body.to_owned()).into_response()
}

async fn record(State(mock): State<Mock>, req: Request<Body>, next: axum::middleware::Next) -> Response {
    let line = format!("{} {}", req.method(), req.uri());
    mock.requests.lock().push(line);
    next.run(req).await
}

fn tvdb_authorized(headers: &HeaderMap) -> bool {
    // The first token "expires" right away, so pley has to log in again.
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).is_some_and(|v| v == "Bearer token-2")
}

async fn start_mock() -> (Mock, String) {
    let mock = Mock::default();
    let app = Router::new()
        // TVmaze
        .route(
            "/tvmaze/search/shows",
            get(|Query(q): Query<Vec<(String, String)>>| async move {
                let wanted = q.iter().any(|(k, v)| k == "q" && v == "Harbour Lights");
                if wanted { json_reply(TVMAZE_SEARCH) } else { json_reply("[]") }
            }),
        )
        .route("/tvmaze/lookup/shows", get(|| async { StatusCode::NOT_FOUND }))
        .route(
            "/tvmaze/shows/{id}",
            get(|Path(id): Path<i64>| async move {
                if id == 4242 { json_reply(TVMAZE_SHOW) } else { StatusCode::NOT_FOUND.into_response() }
            }),
        )
        .route("/tvmaze/shows/{id}/episodes", get(|| async { json_reply(TVMAZE_EPISODES) }))
        // TheTVDB
        .route(
            "/tvdb/login",
            post(|State(mock): State<Mock>, body: axum::Json<Value>| async move {
                if body["apikey"] != "test-key" || body.get("pin").is_some() {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                let n = mock.tvdb_logins.fetch_add(1, Ordering::SeqCst) + 1;
                axum::Json(json!({ "status": "success", "data": { "token": format!("token-{n}") } })).into_response()
            }),
        )
        .route(
            "/tvdb/series/{id}/extended",
            get(|headers: HeaderMap| async move {
                if tvdb_authorized(&headers) {
                    json_reply(TVDB_SERIES)
                } else {
                    StatusCode::UNAUTHORIZED.into_response()
                }
            }),
        )
        .route(
            "/tvdb/series/{id}/episodes/default",
            get(|headers: HeaderMap| async move {
                if tvdb_authorized(&headers) {
                    json_reply(TVDB_EPISODES)
                } else {
                    StatusCode::UNAUTHORIZED.into_response()
                }
            }),
        )
        // MusicBrainz and the Cover Art Archive
        .route("/mb/genre/all", get(|| async { MB_GENRES }))
        .route(
            "/mb/release-group",
            get(|Query(q): Query<Vec<(String, String)>>| async move {
                let query = q.iter().find(|(k, _)| k == "query").map(|(_, v)| v.as_str()).unwrap_or_default();
                if query == r#"releasegroup:"Discovery" AND artist:"Daft Punk""# {
                    json_reply(MB_GROUP_SEARCH)
                } else {
                    json_reply(r#"{"release-groups": []}"#)
                }
            }),
        )
        .route(
            "/mb/artist/{id}",
            get(|Path(id): Path<String>| async move {
                if id == DAFT_PUNK { json_reply(MB_ARTIST) } else { StatusCode::NOT_FOUND.into_response() }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(mock.clone(), record))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (mock, format!("http://{addr}"))
}

// ------------------------------------------------------------------ pley

struct TestApp {
    router: Router,
    state: AppState,
    media: tempfile::TempDir,
    _data: tempfile::TempDir,
}

impl TestApp {
    /// pley talking to the stand-in services at `base`.
    async fn new(base: &str) -> Self {
        Self::with_endpoints(Endpoints {
            tvmaze: format!("{base}/tvmaze"),
            tvdb: format!("{base}/tvdb"),
            musicbrainz: format!("{base}/mb"),
            cover_art: format!("{base}/caa"),
            tmdb: format!("{base}/tmdb"),
            tmdb_images: format!("{base}/tmdb-images"),
            opensubtitles: format!("{base}/opensubtitles"),
        })
        .await
    }

    async fn with_endpoints(endpoints: Endpoints) -> Self {
        let data = tempfile::tempdir().unwrap();
        let mut config = Config::for_data_dir(data.path());
        config.transcoder_path = Some(data.path().join("no-transcoder-in-tests"));
        config.metadata_endpoints = Some(endpoints);
        let state = AppState::init(config).await.unwrap();
        let router = pley::api::router(state.clone());
        Self { router, state, media: tempfile::tempdir().unwrap(), _data: data }
    }

    /// Store a picture for every image URL in `texts`, as if downloaded
    /// before: refreshes find them and download nothing.
    async fn seed_pictures(&self, texts: &[&str]) {
        let urls = regex::Regex::new(r#"https://[^"\s]+\.jpg"#).unwrap();
        for text in texts {
            for url in urls.find_iter(text) {
                self.seed_picture(url.as_str()).await;
            }
        }
    }

    async fn seed_picture(&self, url: &str) {
        // JPEG magic and the URL: a different picture for each URL.
        let mut bytes = b"\xff\xd8\xff\xe0".to_vec();
        bytes.extend_from_slice(url.as_bytes());
        let blob = self.state.store.put(&bytes, "jpg").await.unwrap();
        sqlx::query("INSERT OR REPLACE INTO remote_images (url, blob, fetched_at) VALUES (?, ?, 0)")
            .bind(url)
            .bind(blob)
            .execute(&self.state.db.write)
            .await
            .unwrap();
    }

    fn media_file(&self, rel: &str) {
        let path = self.media.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; 2048]).unwrap();
    }

    async fn call(&self, method: Method, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn get(&self, uri: &str, token: &str) -> Value {
        let (status, body) = self.call(Method::GET, uri, Some(token), None).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}: {body}");
        body
    }

    async fn setup_admin(&self) -> String {
        let (status, body) = self
            .call(
                Method::POST,
                "/api/setup",
                None,
                Some(
                    json!({"username": "admin", "password": "secret-one", "setup_code": self.state.auth.setup.shown()}),
                ),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["token"].as_str().unwrap().to_owned()
    }

    async fn wait_for_scans(&self) {
        // Long enough for the real services too (MusicBrainz: a request a second).
        for _ in 0..2400 {
            if self.state.scanner.statuses().is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("scan did not finish");
    }

    async fn library(&self, token: &str, kind: &str, rel: &str) -> i64 {
        let path = self.media.path().join(rel);
        std::fs::create_dir_all(&path).unwrap();
        let body = json!({"name": rel, "kind": kind, "paths": [path.to_string_lossy()]});
        let (status, body) = self.call(Method::POST, "/api/libraries", Some(token), Some(body)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        self.wait_for_scans().await;
        body["id"].as_i64().unwrap()
    }

    /// The only top-level item of a library.
    async fn single_item(&self, token: &str, lib: i64) -> Value {
        let list = self.get(&format!("/api/libraries/{lib}/items"), token).await;
        let items = list["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{list}");
        self.get(&format!("/api/items/{}", items[0]["id"]), token).await
    }

    async fn children(&self, token: &str, id: &Value) -> Vec<Value> {
        self.get(&format!("/api/items/{id}/children"), token).await["items"].as_array().unwrap().clone()
    }

    async fn musicbrainz_id(&self, id: &Value) -> Option<String> {
        sqlx::query_scalar("SELECT musicbrainz_id FROM items WHERE id = ?")
            .bind(id.as_i64().unwrap())
            .fetch_one(&self.state.db.read)
            .await
            .unwrap()
    }
}

fn names(credits: &Value) -> Vec<(String, String, Option<String>)> {
    credits
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_owned(),
                c["role"].as_str().unwrap().to_owned(),
                c["character"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

fn titled<'a>(items: &'a [Value], title: &str) -> &'a Value {
    items.iter().find(|i| i["title"] == title).unwrap_or_else(|| panic!("no {title} in {items:?}"))
}

#[tokio::test]
async fn shows_from_tvmaze_then_thetvdb() {
    let (mock, base) = start_mock().await;
    let app = TestApp::new(&base).await;
    app.seed_pictures(&[TVMAZE_SHOW, TVMAZE_EPISODES, TVDB_SERIES, TVDB_EPISODES]).await;
    let token = app.setup_admin().await;
    app.media_file("tv/Harbour Lights (2015)/Season 01/Harbour Lights - S01E01.mkv");
    app.media_file("tv/Harbour Lights (2015)/Season 01/Harbour Lights - S01E02.mkv");
    app.media_file("tv/Harbour Lights (2015)/Specials/Harbour Lights - S00E01.mkv");

    // No key anywhere: TVmaze.
    let lib = app.library(&token, "shows", "tv").await;
    let show = app.single_item(&token, lib).await;
    assert_eq!(show["title"], "Harbour Lights");
    assert_eq!(
        show["summary"],
        "Harbour Lights follows coastguard Nell Ward & her brother as storms bring old secrets ashore.\n\nFilmed in Cornwall."
    );
    assert_eq!((show["year"].as_i64(), show["release_date"].as_str()), (Some(2015), Some("2015-09-14")));
    assert_eq!(show["genres"], json!(["Drama", "Mystery"]));
    assert_eq!(show["studio"], "BBC One");
    assert_eq!(show["rating"], 7.9);
    assert!(show["poster_url"].is_string() && show["backdrop_url"].is_string(), "{show}");
    assert_eq!(
        names(&show["cast"]),
        [
            ("Mara Quinn".to_owned(), "actor".to_owned(), Some("Nell Ward".to_owned())),
            ("Daniel Rees".to_owned(), "actor".to_owned(), Some("Tom Ward".to_owned())),
            ("Ivo Pell".to_owned(), "actor".to_owned(), Some("The Lighthouse (voice)".to_owned())),
        ]
    );
    assert!(show["cast"][0]["photo_url"].is_string());
    assert_eq!(names(&show["crew"]), [("Owen Tregarthen".to_owned(), "creator".to_owned(), None)]);

    let seasons = app.children(&token, &show["id"]).await;
    let first = titled(&seasons, "Season 1");
    assert_eq!(app.get(&format!("/api/items/{}", first["id"]), &token).await["summary"], "Nell comes home.");
    let episodes = app.children(&token, &first["id"]).await;
    let pilot = titled(&episodes, "The Wreck");
    assert_eq!(pilot["summary"], "A boat runs aground in the fog.");
    assert_eq!(pilot["release_date"], "2015-09-14");
    assert!(pilot["thumb_url"].is_string());
    titled(&episodes, "Undertow");
    // TVmaze's specials, numbered by air date.
    let specials = app.children(&token, &titled(&seasons, "Specials")["id"]).await;
    titled(&specials, "A Harbour Christmas");
    assert_eq!(mock.requested("GET /tvmaze/search/shows"), 1);
    assert_eq!(mock.requested("GET /tvmaze/shows/4242/episodes?specials=1"), 1);

    // A hand edit outlives the next source.
    let (status, _) = app
        .call(Method::PATCH, &format!("/api/items/{}", show["id"]), Some(&token), Some(json!({"summary": "Mine."})))
        .await;
    assert_eq!(status, StatusCode::OK);

    // TheTVDB needs a key, and becomes the source when chosen.
    let (status, body) =
        app.call(Method::PUT, "/api/settings", Some(&token), Some(json!({"tv_metadata": "tvdb"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let settings = json!({"tv_metadata": "tvdb", "tvdb_api_key": "test-key"});
    let (status, body) = app.call(Method::PUT, "/api/settings", Some(&token), Some(settings)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tv_metadata"], "tvdb");
    app.wait_for_scans().await;

    let show = app.single_item(&token, lib).await;
    assert_eq!(show["summary"], "Mine.");
    // TVmaze has no content ratings; TheTVDB does.
    assert_eq!(show["content_rating"], "TV-14");
    assert_eq!(names(&show["cast"])[0], ("Mara Quinn".to_owned(), "actor".to_owned(), Some("Nell Ward".to_owned())));
    assert_eq!(names(&show["crew"]), [("Owen Tregarthen".to_owned(), "creator".to_owned(), None)]);
    let seasons = app.children(&token, &show["id"]).await;
    let specials = app.children(&token, &titled(&seasons, "Specials")["id"]).await;
    titled(&specials, "A Harbour Christmas");
    // Found by the TheTVDB id TVmaze knew; the first token was refused once.
    assert_eq!(mock.requested("GET /tvdb/search"), 0);
    assert_eq!(mock.requested("GET /tvdb/series/290001/extended"), 2);
    assert_eq!(mock.tvdb_logins.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn settings_test_keys_and_services() {
    let (mock, base) = start_mock().await;
    let app = TestApp::new(&base).await;
    let token = app.setup_admin().await;
    let test = |body: Value| {
        let app = &app;
        let token = token.clone();
        async move {
            let (status, reply) = app.call(Method::POST, "/api/settings/test", Some(&token), Some(body)).await;
            assert_eq!(status, StatusCode::OK, "{reply}");
            (reply["ok"].as_bool().unwrap(), reply["message"].as_str().unwrap().to_owned())
        }
    };

    // A key typed in the form, not saved: it signs in, and pley's own token
    // (none yet) is left alone.
    let (ok, message) = test(json!({"service": "tvdb", "api_key": "test-key"})).await;
    assert!(ok, "{message}");
    assert_eq!(message, "TheTVDB accepted the key.");
    let (ok, message) = test(json!({"service": "tvdb", "api_key": "wrong"})).await;
    assert!(!ok);
    assert!(message.starts_with("TheTVDB rejected this key."), "{message}");
    let (ok, message) = test(json!({"service": "tvdb", "api_key": "test-key", "pin": "1234"})).await;
    assert_eq!((ok, message.as_str()), (false, "TheTVDB rejected this key and PIN."));
    // Nothing saved and nothing typed.
    let (ok, message) = test(json!({"service": "tvdb"})).await;
    assert_eq!((ok, message.as_str()), (false, "Enter a TheTVDB API key first."));
    assert_eq!(mock.requested("POST /tvdb/login"), 3);

    let (ok, message) = test(json!({"service": "tvmaze"})).await;
    assert_eq!((ok, message.as_str()), (true, "TVmaze answered."));
    let (ok, message) = test(json!({"service": "musicbrainz"})).await;
    assert_eq!((ok, message.as_str()), (true, "MusicBrainz answered."));
    // No transcoder runs in the tests.
    let (ok, message) = test(json!({"service": "transcoder"})).await;
    assert!(!ok && message.starts_with("Couldn't reach the transcoder"), "{message}");

    // Administrators only.
    let (status, _) = app.call(Method::POST, "/api/settings/test", None, Some(json!({"service": "tvmaze"}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn music_from_musicbrainz() {
    let (mock, base) = start_mock().await;
    let app = TestApp::new(&base).await;
    app.seed_picture(&format!("{base}/caa/release-group/{DISCOVERY}/front-500")).await;
    let token = app.setup_admin().await;
    app.media_file("music/Daft Punk/Discovery/01 - One More Time.mp3");
    app.media_file("music/Daft Punk/Discovery/02 - Aerodynamic.mp3");
    let lib = app.library(&token, "music", "music").await;

    let artist = app.single_item(&token, lib).await;
    assert_eq!(artist["title"], "Daft Punk");
    assert_eq!(artist["genres"], json!(["Electronic", "House", "French House", "Dance", "Disco"]));
    assert_eq!(app.musicbrainz_id(&artist["id"]).await.as_deref(), Some(DAFT_PUNK));

    let albums = app.children(&token, &artist["id"]).await;
    let album = app.get(&format!("/api/items/{}", titled(&albums, "Discovery")["id"]), &token).await;
    assert_eq!((album["year"].as_i64(), album["release_date"].as_str()), (Some(2001), Some("2001-02-26")));
    assert_eq!(album["genres"], json!(["House", "Electronic", "French House", "Progressive House", "Dance"]));
    assert!(album["poster_url"].is_string(), "the Cover Art Archive's cover: {album}");
    assert_eq!(app.musicbrainz_id(&album["id"]).await.as_deref(), Some(DISCOVERY));
    // The album credits the artist: a lookup instead of a search.
    assert_eq!(mock.requested("GET /mb/artist?"), 0);
    assert_eq!(mock.requested(&format!("GET /mb/artist/{DAFT_PUNK}")), 1);

    // Turned off: nothing is asked for.
    let (status, _) =
        app.call(Method::PUT, "/api/settings", Some(&token), Some(json!({"music_metadata": false}))).await;
    assert_eq!(status, StatusCode::OK);
    let before = mock.requests.lock().len();
    let (status, body) =
        app.call(Method::POST, &format!("/api/items/{}/refresh", album["id"]), Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["message"].as_str().unwrap().contains("MusicBrainz"));
    assert_eq!(mock.requests.lock().len(), before);
}

// ------------------------------------------------------------------ the real services
//
// `cargo test -p pley-server --test providers -- --ignored` checks the
// providers' real answers (a handful of requests). TheTVDB needs a key:
// set PLEY_TEST_TVDB_KEY (and PLEY_TEST_TVDB_PIN for keys that need one).

#[tokio::test]
#[ignore = "talks to TVmaze, MusicBrainz and the Cover Art Archive"]
async fn live_tvmaze_and_musicbrainz() {
    let app = TestApp::with_endpoints(Endpoints::default()).await;
    let token = app.setup_admin().await;
    app.media_file("tv/Breaking Bad (2008)/Season 01/Breaking Bad - S01E01.mkv");
    app.media_file("tv/Breaking Bad (2008)/Specials/Breaking Bad - S00E01.mkv");
    app.media_file("music/Daft Punk/Discovery/01 - One More Time.mp3");

    let tv = app.library(&token, "shows", "tv").await;
    let show = app.single_item(&token, tv).await;
    assert_eq!((show["title"].as_str(), show["year"].as_i64()), (Some("Breaking Bad"), Some(2008)), "{show}");
    assert_eq!(show["studio"], "AMC");
    assert!(show["summary"].as_str().is_some_and(|s| s.contains("Walter White") && !s.contains('<')), "{show}");
    assert!(show["poster_url"].is_string(), "{show}");
    assert!(names(&show["cast"]).contains(&("Bryan Cranston".into(), "actor".into(), Some("Walter White".into()))));
    let seasons = app.children(&token, &show["id"]).await;
    let first = app.children(&token, &titled(&seasons, "Season 1")["id"]).await;
    assert!(titled(&first, "Pilot")["thumb_url"].is_string());
    let specials = app.children(&token, &titled(&seasons, "Specials")["id"]).await;
    titled(&specials, "El Camino: A Breaking Bad Movie");

    let music = app.library(&token, "music", "music").await;
    let artist = app.single_item(&token, music).await;
    assert_eq!(app.musicbrainz_id(&artist["id"]).await.as_deref(), Some(DAFT_PUNK));
    assert!(artist["genres"].as_array().is_some_and(|g| !g.is_empty()), "{artist}");
    let albums = app.children(&token, &artist["id"]).await;
    let album = app.get(&format!("/api/items/{}", titled(&albums, "Discovery")["id"]), &token).await;
    assert_eq!((album["year"].as_i64(), album["release_date"].as_str()), (Some(2001), Some("2001-02-26")));
    assert!(album["genres"].as_array().is_some_and(|g| g.iter().any(|g| g == "House")), "{album}");
    assert!(album["poster_url"].is_string(), "no cover from the Cover Art Archive: {album}");
}

#[tokio::test]
#[ignore = "needs a TheTVDB key in PLEY_TEST_TVDB_KEY"]
async fn live_thetvdb() {
    let Ok(key) = std::env::var("PLEY_TEST_TVDB_KEY") else {
        eprintln!("PLEY_TEST_TVDB_KEY is not set");
        return;
    };
    let pin = std::env::var("PLEY_TEST_TVDB_PIN").unwrap_or_default();
    let app = TestApp::with_endpoints(Endpoints::default()).await;
    let token = app.setup_admin().await;
    let settings = json!({"tv_metadata": "tvdb", "tvdb_api_key": key, "tvdb_pin": pin});
    let (status, body) = app.call(Method::PUT, "/api/settings", Some(&token), Some(settings)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    app.media_file("tv/Breaking Bad (2008)/Season 01/Breaking Bad - S01E01.mkv");
    app.media_file("tv/Breaking Bad (2008)/Specials/Breaking Bad - S00E01.mkv");

    let tv = app.library(&token, "shows", "tv").await;
    let show = app.single_item(&token, tv).await;
    assert_eq!((show["title"].as_str(), show["year"].as_i64()), (Some("Breaking Bad"), Some(2008)), "{show}");
    assert!(show["summary"].is_string() && show["poster_url"].is_string(), "{show}");
    assert!(show["content_rating"].is_string(), "{show}");
    assert!(names(&show["cast"]).iter().any(|(name, _, _)| name == "Bryan Cranston"), "{show}");
    let seasons = app.children(&token, &show["id"]).await;
    let first = app.children(&token, &titled(&seasons, "Season 1")["id"]).await;
    titled(&first, "Pilot");
}
