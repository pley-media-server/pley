//! Requests and Discover, against local stand-ins for TMDB (its API and its
//! picture server), Radarr and Sonarr that answer like them for a handful
//! of titles and keep what they were sent. Nothing leaves the machine.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{BodyDataStream, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use parking_lot::Mutex;
use pley::events::{Audience, ServerEvent};
use pley::requests::RequestStatus;
use serde_json::{Value, json};
use tokio::sync::broadcast::Receiver;

use common::TestApp;

const KEY: &str = "test-key";
const RADARR_KEY: &str = "radarr-key";
const SONARR_KEY: &str = "sonarr-key";
/// What TMDB's picture server answers: the start of a JPEG.
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];

/// What one of the services was sent.
#[derive(Debug, Clone)]
struct Call {
    method: String,
    /// Without the service's prefix ("/movie", "/series/lookup").
    path: String,
    query: Vec<(String, String)>,
    /// `X-Api-Key`.
    key: Option<String>,
    body: Value,
}

#[derive(Clone, Default)]
struct Services {
    radarr: Arc<Mutex<Vec<Call>>>,
    sonarr: Arc<Mutex<Vec<Call>>>,
    tmdb: Arc<Mutex<Vec<String>>>,
    /// What TMDB's picture server was asked for (`/w342/matrix.jpg`).
    pictures: Arc<Mutex<Vec<String>>>,
    /// The picture server fails this many times before it answers.
    picture_failures: Arc<Mutex<u32>>,
    movies: Arc<Mutex<Vec<Value>>>,
    series: Arc<Mutex<Vec<Value>>>,
    /// Radarr refuses this many movies added before it takes one.
    refusals: Arc<Mutex<u32>>,
    /// How long Radarr's and Sonarr's queues take to answer.
    queue_delay: Arc<Mutex<Option<Duration>>>,
}

impl Services {
    fn radarr_calls(&self, method: &str, path: &str) -> Vec<Call> {
        self.radarr.lock().iter().filter(|c| c.method == method && c.path == path).cloned().collect()
    }

    fn sonarr_calls(&self, method: &str, path: &str) -> Vec<Call> {
        self.sonarr.lock().iter().filter(|c| c.method == method && c.path == path).cloned().collect()
    }
}

fn movie_details(id: i64) -> Option<Value> {
    let (title, date, imdb, rated) = match id {
        603 => ("The Matrix", "1999-03-31", "tt0133093", "R"),
        604 => ("The Matrix Reloaded", "2003-05-15", "tt0234215", "R"),
        605 => ("The Matrix Revolutions", "2003-11-05", "tt0242653", "R"),
        606 => ("Toy Robots", "2010-06-18", "tt1000606", "G"),
        _ => return None,
    };
    Some(json!({
        "id": id, "title": title, "original_title": title, "original_language": "en", "overview": "Neo.",
        "tagline": "Welcome to the Real World.", "release_date": date, "runtime": 136,
        "genres": [{"id": 28, "name": "Action"}, {"id": 878, "name": "Science Fiction"}],
        "production_companies": [{"name": "Village Roadshow Pictures"}],
        "poster_path": "/matrix.jpg", "backdrop_path": "/matrixwide.jpg",
        "vote_average": 8.2, "vote_count": 25000, "imdb_id": imdb,
        "keywords": {"keywords": [{"id": 310, "name": "artificial intelligence"}]},
        "release_dates": {"results": [{"iso_3166_1": "US", "release_dates": [{"certification": ""}, {"certification": rated}]}]},
        "credits": {"cast": [{"name": "Keanu Reeves", "character": "Neo", "profile_path": "/keanu.jpg"}]}
    }))
}

fn tmdb_answer(path: &str, query: &[(String, String)]) -> Option<Value> {
    let param = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    Some(match path {
        "/trending/all/week" => json!({"page": 1, "total_pages": 3, "results": [
            {"id": 603, "media_type": "movie", "title": "The Matrix", "release_date": "1999-03-31",
             "vote_average": 8.2, "vote_count": 25000, "poster_path": "/matrix.jpg", "overview": "Neo."},
            {"id": 1396, "media_type": "tv", "name": "Breaking Bad", "first_air_date": "2008-01-20",
             "vote_average": 8.9, "vote_count": 14000},
            {"id": 287, "media_type": "person", "name": "Brad Pitt"}
        ]}),
        "/discover/tv" if param("with_keywords") == Some("210024") => json!({"page": 1, "total_pages": 1, "results": [
            {"id": 9001, "name": "Space Cowboys", "first_air_date": "1998-04-03", "vote_average": 8.5, "vote_count": 3000}
        ]}),
        "/search/multi" => json!({"page": 1, "total_pages": 1, "results": match param("query") {
            Some("matrix") => vec![
                json!({"id": 603, "media_type": "movie", "title": "The Matrix", "release_date": "1999-03-31"}),
                json!({"id": 604, "media_type": "movie", "title": "The Matrix Reloaded", "release_date": "2003-05-15"}),
            ],
            _ => vec![],
        }}),
        "/search/movie" | "/search/tv" => json!({"results": []}),
        "/certification/movie/list" => json!({"certifications": {"US": [
            {"certification": "NR", "order": 0}, {"certification": "G", "order": 1},
            {"certification": "PG", "order": 2}, {"certification": "PG-13", "order": 3},
            {"certification": "R", "order": 4}, {"certification": "NC-17", "order": 5}
        ]}}),
        // Popular movies rated at most the ceiling asked for.
        "/discover/movie" if param("certification.lte").is_some() => json!({"page": 1, "total_pages": 1, "results": [
            {"id": 606, "title": "Toy Robots", "release_date": "2010-06-18", "vote_average": 7.1, "vote_count": 900}
        ]}),
        "/find/81189" if param("external_source") == Some("tvdb_id") => {
            json!({"movie_results": [], "tv_results": [{"id": 1396}]})
        }
        "/tv/1396" => json!({
            "name": "Breaking Bad", "original_name": "Breaking Bad", "original_language": "en", "origin_country": ["US"],
            "overview": "A chemistry teacher.", "first_air_date": "2008-01-20", "status": "Ended",
            "genres": [{"id": 18, "name": "Drama"}], "networks": [{"name": "AMC"}],
            "poster_path": "/bb.jpg", "backdrop_path": null, "vote_average": 8.9, "vote_count": 14000,
            "seasons": [
                {"season_number": 0, "name": "Specials", "episode_count": 2, "air_date": "2009-02-17"},
                {"season_number": 1, "name": "Season 1", "episode_count": 3, "air_date": "2008-01-20"},
                {"season_number": 2, "name": "Season 2", "episode_count": 2, "air_date": "2009-03-08"}
            ],
            "last_episode_to_air": {"season_number": 2, "episode_number": 2},
            "keywords": {"results": []},
            "content_ratings": {"results": [{"iso_3166_1": "US", "rating": "TV-MA"}]},
            "external_ids": {"imdb_id": "tt0903747", "tvdb_id": 81189},
            "aggregate_credits": {"cast": [{"name": "Bryan Cranston", "roles": [{"character": "Walter White"}]}]}
        }),
        "/tv/1396/external_ids" => json!({"imdb_id": "tt0903747", "tvdb_id": 81189}),
        "/tv/9001" => json!({
            "name": "Space Cowboys", "original_name": "Uchuu no Kaubōi", "original_language": "ja",
            "origin_country": ["JP"], "overview": "Bounty hunters.", "first_air_date": "1998-04-03",
            "status": "Ended", "genres": [{"id": 16, "name": "Animation"}], "networks": [{"name": "TV Tokyo"}],
            "seasons": [
                {"season_number": 1, "episode_count": 2, "air_date": "1998-04-03"},
                {"season_number": 2, "episode_count": 2, "air_date": "1999-04-03"},
                {"season_number": 3, "episode_count": 2, "air_date": "2000-04-03"}
            ],
            "last_episode_to_air": {"season_number": 3, "episode_number": 2},
            "keywords": {"results": [{"id": 210024, "name": "anime"}]},
            "external_ids": {"imdb_id": null, "tvdb_id": 76885}
        }),
        "/tv/9001/external_ids" => json!({"imdb_id": null, "tvdb_id": 76885}),
        path => movie_details(path.strip_prefix("/movie/")?.parse().ok()?)?,
    })
}

fn radarr_answer(s: &Services, call: &Call) -> (StatusCode, Value) {
    let param = |name: &str| call.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    let tmdb = || param("tmdbId").and_then(|t| t.parse::<i64>().ok());
    match (call.method.as_str(), call.path.as_str()) {
        ("GET", "/system/status") => (StatusCode::OK, json!({"version": "5.14.0.9383"})),
        ("GET", "/qualityprofile") => {
            (StatusCode::OK, json!([{"id": 4, "name": "HD-1080p"}, {"id": 5, "name": "Ultra-HD"}]))
        }
        ("GET", "/rootfolder") => (StatusCode::OK, json!([{"path": "/movies", "freeSpace": 5_000_000_000_i64}])),
        ("GET", "/movie") => {
            let found: Vec<Value> =
                s.movies.lock().iter().filter(|m| m["tmdbId"].as_i64() == tmdb()).cloned().collect();
            (StatusCode::OK, json!(found))
        }
        ("GET", "/movie/lookup/tmdb") => match tmdb().and_then(movie_details) {
            Some(m) => (StatusCode::OK, json!({"title": m["title"], "tmdbId": tmdb(), "year": 1999, "images": []})),
            None => (StatusCode::NOT_FOUND, json!({"message": "NotFound"})),
        },
        ("POST", "/movie") => {
            let mut refusals = s.refusals.lock();
            if *refusals > 0 {
                *refusals -= 1;
                let errors = json!([{"propertyName": "RootFolderPath", "errorMessage": "Root folder is not writable"}]);
                return (StatusCode::BAD_REQUEST, errors);
            }
            let mut movies = s.movies.lock();
            let mut movie = call.body.clone();
            movie["id"] = json!(11 + movies.len());
            movie["hasFile"] = json!(false);
            movies.push(movie.clone());
            (StatusCode::CREATED, movie)
        }
        ("PUT", path) if path.starts_with("/movie/") => (StatusCode::ACCEPTED, call.body.clone()),
        ("POST", "/command") => (StatusCode::CREATED, json!({"id": 1, "name": call.body["name"]})),
        ("GET", "/queue") => (
            StatusCode::OK,
            json!({"records": [
                {"movieId": 11, "size": 1000.0, "sizeleft": 400.0, "status": "downloading",
                 "trackedDownloadState": "downloading", "trackedDownloadStatus": "ok",
                 "estimatedCompletionTime": "2030-01-01T00:00:00Z"}
            ]}),
        ),
        _ => (StatusCode::NOT_FOUND, json!({"message": "NotFound"})),
    }
}

fn sonarr_answer(s: &Services, call: &Call) -> (StatusCode, Value) {
    let param = |name: &str| call.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    match (call.method.as_str(), call.path.as_str()) {
        ("GET", "/system/status") => (StatusCode::OK, json!({"version": "4.0.11.2680"})),
        ("GET", "/qualityprofile") => (StatusCode::OK, json!([{"id": 6, "name": "HD-720p"}])),
        ("GET", "/rootfolder") => (StatusCode::OK, json!([{"path": "/tv", "freeSpace": null}, {"path": "/anime"}])),
        ("GET", "/series") => {
            let tvdb = param("tvdbId").and_then(|t| t.parse::<i64>().ok());
            let found: Vec<Value> = s.series.lock().iter().filter(|m| m["tvdbId"].as_i64() == tvdb).cloned().collect();
            (StatusCode::OK, json!(found))
        }
        ("GET", "/series/lookup") => {
            let tvdb = param("term").and_then(|t| t.strip_prefix("tvdb:")?.parse::<i64>().ok());
            let seasons = |n: i64| (0..=n).map(|s| json!({"seasonNumber": s, "monitored": true})).collect::<Vec<_>>();
            let found = match tvdb {
                Some(81189) => vec![json!({"title": "Breaking Bad", "tvdbId": 81189, "seasons": seasons(2)})],
                Some(76885) => vec![json!({"title": "Space Cowboys", "tvdbId": 76885, "seasons": seasons(3)})],
                _ => vec![],
            };
            (StatusCode::OK, json!(found))
        }
        ("POST", "/series") => {
            let mut series = s.series.lock();
            let mut show = call.body.clone();
            show["id"] = json!(21 + series.len());
            series.push(show.clone());
            (StatusCode::CREATED, show)
        }
        ("PUT", path) if path.starts_with("/series/") => (StatusCode::ACCEPTED, call.body.clone()),
        ("POST", "/command") => (StatusCode::CREATED, json!({"id": 2, "name": call.body["name"]})),
        ("GET", "/queue") => (
            StatusCode::OK,
            json!({"records": [
                {"seriesId": 21, "seasonNumber": 2, "size": 100.0, "sizeleft": 100.0, "status": "queued"},
                {"seriesId": 21, "seasonNumber": 1, "size": 100.0, "sizeleft": 0.0, "status": "completed",
                 "trackedDownloadState": "importPending"}
            ]}),
        ),
        // Sonarr 4's: no language profiles.
        _ => (StatusCode::NOT_FOUND, json!({"message": "NotFound"})),
    }
}

async fn serve(
    State(s): State<Services>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    Query(query): Query<Vec<(String, String)>>,
    body: Bytes,
) -> Response {
    let path = uri.path();
    // Every picture but `gone.jpg`.
    if let Some(path) = path.strip_prefix("/tmdb-images") {
        s.pictures.lock().push(path.to_owned());
        let mut failures = s.picture_failures.lock();
        if *failures > 0 {
            *failures -= 1;
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if path.ends_with("/gone.jpg") {
            return StatusCode::NOT_FOUND.into_response();
        }
        return ([(header::CONTENT_TYPE, "image/jpeg")], JPEG).into_response();
    }
    if let Some(path) = path.strip_prefix("/tmdb") {
        s.tmdb.lock().push(format!("{path}?{}", uri.query().unwrap_or_default()));
        if !query.iter().any(|(k, v)| k == "api_key" && v == KEY) {
            return (StatusCode::UNAUTHORIZED, axum::Json(json!({"status_code": 7}))).into_response();
        }
        return match tmdb_answer(path, &query) {
            Some(body) => axum::Json(body).into_response(),
            None => (StatusCode::NOT_FOUND, axum::Json(json!({"status_code": 34}))).into_response(),
        };
    }
    let (service, key, path) = match (path.strip_prefix("/radarr/api/v3"), path.strip_prefix("/sonarr/api/v3")) {
        (Some(path), _) => ("radarr", RADARR_KEY, path),
        (_, Some(path)) => ("sonarr", SONARR_KEY, path),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let call = Call {
        method: method.to_string(),
        path: path.to_owned(),
        query,
        key: headers.get("x-api-key").and_then(|k| k.to_str().ok()).map(str::to_owned),
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    };
    let calls = if service == "radarr" { &s.radarr } else { &s.sonarr };
    calls.lock().push(call.clone());
    if call.key.as_deref() != Some(key) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let delay = *s.queue_delay.lock();
    if let Some(delay) = delay.filter(|_| call.path == "/queue") {
        tokio::time::sleep(delay).await;
    }
    let (status, body) = if service == "radarr" { radarr_answer(&s, &call) } else { sonarr_answer(&s, &call) };
    (status, axum::Json(body)).into_response()
}

async fn start_services() -> (Services, String) {
    let services = Services::default();
    let app = Router::new().fallback(serve).with_state(services.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (services, format!("http://{addr}"))
}

struct World {
    app: TestApp,
    services: Services,
    base: String,
    admin: String,
    ann: String,
    ann_id: i64,
    movies: i64,
    shows: i64,
}

/// pley with TMDB at the stand-in (the other providers too; they are never
/// asked here), empty movie and show libraries, and Ann, who asks.
async fn world() -> World {
    world_with(|_| {}).await
}

/// The same, with `adjust` changing pley's configuration.
async fn world_with(adjust: impl FnOnce(&mut pley::Config)) -> World {
    let (services, base) = start_services().await;
    let app = TestApp::with_config(|c| {
        c.metadata_endpoints = Some(pley::providers::Endpoints {
            tvmaze: format!("{base}/tvmaze"),
            tvdb: format!("{base}/tvdb"),
            musicbrainz: format!("{base}/mb"),
            cover_art: format!("{base}/caa"),
            tmdb: format!("{base}/tmdb"),
            tmdb_images: format!("{base}/tmdb-images"),
            opensubtitles: format!("{base}/opensubtitles"),
        });
        adjust(c);
    })
    .await;
    let admin = app.setup_admin().await;
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let shows = app.library(&admin, "Shows", "shows", "shows").await;
    let (ann, ann_id) = app.user(&admin, "ann", json!({})).await;
    World { app, services, base, admin, ann, ann_id, movies, shows }
}

impl World {
    /// Requests on, with Radarr and Sonarr at the stand-in; `more` changes
    /// some of the settings.
    async fn turn_on(&self, more: Value) {
        let mut settings = json!({
            "tmdb_api_key": KEY,
            "requests": true,
            "radarr_url": format!("{}/radarr", self.base),
            "radarr_api_key": RADARR_KEY,
            "radarr_quality_profile": 4,
            "radarr_root_folder": "/movies",
            "sonarr_url": format!("{}/sonarr", self.base),
            "sonarr_api_key": SONARR_KEY,
            "sonarr_quality_profile": 6,
            "sonarr_root_folder": "/tv",
            "sonarr_anime_root_folder": "/anime",
        });
        for (key, value) in more.as_object().unwrap() {
            settings[key] = value.clone();
        }
        let reply = self.app.put("/api/settings", &self.admin, settings).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
        self.app.wait_for_scans().await;
    }

    /// Files added to a library, scanned.
    async fn add(&self, library: i64, files: &[&str]) {
        for file in files {
            self.app.media_file(file, 1024);
        }
        let reply =
            self.app.post(&format!("/api/libraries/{library}/scan"), &self.admin, json!({"force": false})).await;
        assert_eq!(reply.status, StatusCode::ACCEPTED, "{}", reply.text());
        self.app.wait_for_scans().await;
    }

    /// Gives the library's movie or show `title` ids, as metadata would:
    /// `set` is SQL (`tmdb_id = 603`).
    async fn identify(&self, title: &str, set: &str) {
        let done = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE items SET {set} WHERE title = ? AND kind IN ('movie', 'show')"
        )))
        .bind(title)
        .execute(&self.app.state.db.write)
        .await
        .unwrap();
        assert_eq!(done.rows_affected(), 1, "{title}");
    }

    async fn ask(&self, token: &str, body: Value) -> common::Reply {
        self.app.post("/api/requests", token, body).await
    }
}

fn status(reply: &common::Reply) -> (StatusCode, String) {
    (reply.status, reply.body["status"].as_str().unwrap_or_default().to_owned())
}

/// What the server said since `events` last looked.
fn said(events: &mut Receiver<ServerEvent>) -> Vec<ServerEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

#[tokio::test]
async fn requests_stay_off_until_turned_on() {
    let w = world().await;
    let summary = w.app.get("/api/requests/summary", &w.ann).await;
    assert_eq!(summary.status, StatusCode::OK);
    assert_eq!(
        summary.body,
        json!({"enabled": false, "access": "ask", "movies_left": 0, "seasons_left": 0, "movies_limit": 0,
               "seasons_limit": 0, "movies_next_at": null, "seasons_next_at": null, "manage": false, "pending": 0,
               "failed": 0, "unseen": 0, "own": 0, "changed_at": null, "sign_in_again": null}),
        "every number 0"
    );
    for uri in [
        "/api/discover/trending",
        "/api/discover/search?q=matrix",
        "/api/requests",
        "/api/discover/movie/603",
        "/api/discover/images/poster/matrix.jpg",
        "/api/items/1/request",
        "/api/requests/options",
    ] {
        let off = w.app.get(uri, &w.ann).await;
        assert_eq!(off.status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!((&off.body["code"], &off.body["setup"]), (&json!("unavailable"), &json!(false)), "{uri}");
    }
    for (method, uri) in [(Method::DELETE, "/api/requests/1/watch"), (Method::POST, "/api/requests/1/handled")] {
        let off = w.app.call(method, uri, Some(w.admin.as_str()), None, &[]).await;
        assert_eq!((off.status, &off.body["code"]), (StatusCode::SERVICE_UNAVAILABLE, &json!("unavailable")), "{uri}");
    }
    // Before anything else is looked at.
    let off = w.app.post("/api/requests/seen", &w.ann, json!({})).await;
    assert_eq!(off.status, StatusCode::SERVICE_UNAVAILABLE);
    let off = w.app.get("/api/requests/abc", &w.ann).await;
    assert_eq!(off.status, StatusCode::SERVICE_UNAVAILABLE);
    // They find titles on TMDB.
    let refused = w.app.put("/api/settings", &w.admin, json!({"requests": true})).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    let mut events = w.app.state.events.subscribe();
    w.turn_on(json!({})).await;
    let heard: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        heard.iter().any(|e| matches!(e, ServerEvent::RequestsSettingsChanged { user_id: None })),
        "every app fetches its summary again"
    );
    let summary = w.app.get("/api/requests/summary", &w.admin).await;
    assert_eq!(
        summary.body,
        json!({"enabled": true, "access": "auto", "movies_left": null, "seasons_left": null, "movies_limit": null,
               "seasons_limit": null, "movies_next_at": null, "seasons_next_at": null, "manage": true, "pending": 0,
               "failed": 0, "unseen": 0, "own": 0, "changed_at": null, "sign_in_again": null})
    );
    // Malformed requests are answered as the rest.
    let bad = w.app.get("/api/requests/abc", &w.ann).await;
    assert_eq!((bad.status, &bad.body["code"]), (StatusCode::BAD_REQUEST, &json!("bad_request")), "{}", bad.text());
    let bad = w.app.send_body(Method::POST, "/api/requests", &w.ann, "application/json", b"{\"kind\":".to_vec()).await;
    assert_eq!((bad.status, &bad.body["code"]), (StatusCode::BAD_REQUEST, &json!("bad_request")), "{}", bad.text());
    let bad = w.ask(&w.ann, json!({"kind": "film", "tmdb_id": 603})).await;
    assert_eq!((bad.status, &bad.body["code"]), (StatusCode::BAD_REQUEST, &json!("bad_request")), "{}", bad.text());
    let bad =
        w.app.send_body(Method::POST, "/api/requests/1/decline", &w.admin, "application/json", b"[".to_vec()).await;
    assert_eq!((bad.status, &bad.body["code"]), (StatusCode::BAD_REQUEST, &json!("bad_request")), "{}", bad.text());
    let bad = w.app.get("/api/discover/movie/x", &w.ann).await;
    assert_eq!((bad.status, &bad.body["code"]), (StatusCode::BAD_REQUEST, &json!("bad_request")), "{}", bad.text());
    // A key TMDB refuses: Discover says TMDB is the trouble, and that
    // trying again won't help.
    let reply = w.app.put("/api/settings", &w.admin, json!({"tmdb_api_key": "wrong"})).await;
    assert_eq!(reply.status, StatusCode::OK);
    w.app.wait_for_scans().await;
    let refused = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        (&refused.body["code"], &refused.body["setup"], &refused.body["retry"]),
        (&json!("tmdb_key"), &json!(true), &json!(false)),
        "{}",
        refused.text()
    );
    assert!(refused.body["message"].as_str().unwrap().starts_with("TMDB refused this server's key"));
    // Nor are Radarr's and Sonarr's keys ever shown.
    let settings = w.app.get("/api/settings", &w.admin).await;
    assert!(!settings.mentions(RADARR_KEY) && !settings.mentions(SONARR_KEY), "{}", settings.text());
    assert_eq!(
        (&settings.body["radarr_api_key_set"], &settings.body["sonarr_api_key_set"]),
        (&json!(true), &json!(true))
    );
}

#[tokio::test]
async fn a_movie_is_asked_for_approved_sent_to_radarr_and_found() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let mut events = w.app.state.events.subscribe();

    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(trending.status, StatusCode::OK, "{}", trending.text());
    let results = trending.body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "people are left out: {results:?}");
    assert_eq!(results[0]["kind"], json!("movie"));
    assert_eq!(results[0]["status"], json!("none"));
    assert_eq!(results[0]["poster_url"], json!("/api/discover/images/poster/matrix.jpg"), "through pley");
    assert_eq!((results[0]["year"].as_i64(), results[0]["rating"].as_f64()), (Some(1999), Some(8.2)));
    assert_eq!((&results[1]["kind"], &results[1]["title"]), (&json!("show"), &json!("Breaking Bad")));
    assert_eq!(trending.body["total_pages"], json!(3));

    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(asked.status, StatusCode::CREATED, "{}", asked.text());
    assert_eq!(asked.body["status"], json!("pending"));
    assert_eq!(asked.body["requested_by"]["name"], json!("ann"));
    assert_eq!((&asked.body["title"], &asked.body["year"]), (&json!("The Matrix"), &json!(1999)));
    assert_eq!((&asked.body["seasons"], &asked.body["actions"]), (&Value::Null, &json!(["withdraw"])));
    let id = asked.body["id"].as_i64().unwrap();
    assert!(w.services.radarr_calls("POST", "/movie").is_empty(), "nothing goes to Radarr before it's approved");

    // Asked for already: Bob follows hers, Tell me too.
    let (bob, bob_id) = w.app.user(&w.admin, "bob", json!({})).await;
    let again = w.ask(&bob, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(
        (&again.body["id"], &again.body["mine"], &again.body["watching"], &again.body["actions"]),
        (&json!(id), &json!(false), &json!(true), &json!(["unwatch"]))
    );
    let unknown = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 999})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let seasons = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604, "seasons": [1]})).await;
    assert_eq!(seasons.status, StatusCode::BAD_REQUEST);

    let found = w.app.get("/api/discover/search?q=matrix", &w.ann).await;
    let results = found.body["results"].as_array().unwrap();
    assert_eq!((&results[0]["status"], &results[0]["request_id"]), (&json!("requested"), &json!(id)));
    assert_eq!(results[1]["status"], json!("none"));
    let summary = w.app.get("/api/requests/summary", &w.admin).await;
    assert_eq!(summary.body["pending"], json!(1));
    let heard: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(heard.iter().any(|e| matches!(e,
        ServerEvent::RequestsChanged { to, request_id, .. }
            if to.users.contains(&w.ann_id) && to.users.contains(&bob_id) && *request_id == Some(id))));

    // Only administrators decide; approving answers at once, and Radarr
    // gets it in the background.
    let denied = w.app.post(&format!("/api/requests/{id}/approve"), &w.ann, json!({})).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    assert_eq!((&approved.body["stage"], &approved.body["sent_to"]), (&json!("approved"), &Value::Null), "sending");
    assert_eq!(approved.body["actions"], json!(["decline", "delete"]));
    assert_eq!(approved.body["decided_by"]["name"], json!("admin"));
    w.app.state.requests.settled().await;
    let added = w.services.radarr_calls("POST", "/movie");
    assert_eq!(added.len(), 1);
    let movie = &added[0].body;
    assert_eq!((&movie["tmdbId"], &movie["qualityProfileId"]), (&json!(603), &json!(4)));
    assert_eq!((&movie["rootFolderPath"], &movie["monitored"]), (&json!("/movies"), &json!(true)));
    assert_eq!(movie["minimumAvailability"], json!("released"));
    assert_eq!(movie["addOptions"], json!({"searchForMovie": true}));
    // Approved already: as it is.
    let again = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&again), (StatusCode::OK, "approved".to_owned()), "{}", again.text());
    assert_eq!(
        again.body["sent_to"],
        json!({"service": "radarr", "quality_profile": "HD-1080p", "root_folder": "/movies"})
    );
    w.app.state.requests.settled().await;
    assert_eq!(w.services.radarr_calls("POST", "/movie").len(), 1, "sent once");

    // Radarr's queue says how far it is.
    w.app.state.requests.refresh_queues().await;
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(
        mine.body["download"],
        json!({"progress": 0.6, "state": "downloading", "eta": 1_893_456_000, "episodes": 1})
    );
    assert_eq!((&mine.body["stage"], &mine.body["actions"]), (&json!("downloading"), &json!([])));
    let withdrawn = w.app.delete(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!((withdrawn.status, &withdrawn.body["code"]), (StatusCode::CONFLICT, &json!("conflict")), "it stays");
    assert_eq!(withdrawn.body["request"]["id"], json!(id), "as it is now");

    // The movie arrives.
    w.add(w.movies, &["movies/The Matrix (1999).mkv"]).await;
    w.identify("The Matrix", "tmdb_id = 603").await;
    w.app.state.requests.sync().await.unwrap();
    let item = w.app.ids(w.movies, &w.admin).await["The Matrix"];
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(status(&mine), (StatusCode::OK, "available".to_owned()), "{}", mine.text());
    assert_eq!((&mine.body["item_id"], &mine.body["download"]), (&json!(item), &Value::Null));
    assert!(mine.body["available_at"].is_i64());
    let heard: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    for (who, user) in [("hers", w.ann_id), ("his, as he follows it", bob_id)] {
        assert!(
            heard.iter().any(|e| matches!(e,
                ServerEvent::RequestAvailable { to, request_id, item_id, complete: true, .. }
                    if to.users == [user] && !to.managers && *request_id == id && *item_id == Some(item))),
            "{who}: {heard:?}"
        );
    }
    let again = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!((again.status, &again.body["request"]["status"]), (StatusCode::CONFLICT, &json!("available")));
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    let matrix = &trending.body["results"][0];
    assert_eq!((&matrix["status"], &matrix["item_id"]), (&json!("library"), &json!(item)), "{matrix}");
    let more = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!((more.status, &more.body["code"]), (StatusCode::CONFLICT, &json!("in_library")), "a library has it");
    let page = w.app.get(&format!("/api/items/{item}/request"), &w.ann).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    assert_eq!(
        (&page.body["kind"], &page.body["tmdb_id"], &page.body["status"]),
        (&json!("movie"), &json!(603), &json!("library"))
    );
    assert_eq!((&page.body["request"], &page.body["askable"]), (&Value::Null, &json!([])), "none active");

    let list = w.app.get("/api/requests", &w.ann).await;
    assert_eq!((&list.body["total"], &list.body["counts"]["available"]), (&json!(1), &json!(1)));
    assert_eq!(list.body["counts"]["pending"], json!(0));
}

#[tokio::test]
async fn a_show_is_asked_for_the_seasons_a_library_lacks() {
    let w = world().await;
    w.turn_on(json!({})).await;
    // Season 1 whole, found by TheTVDB's id (another TV source named it).
    w.add(
        w.shows,
        &[
            "shows/Breaking Bad (2008)/Season 01/Breaking Bad - S01E01.mkv",
            "shows/Breaking Bad (2008)/Season 01/Breaking Bad - S01E02.mkv",
            "shows/Breaking Bad (2008)/Season 01/Breaking Bad - S01E03.mkv",
        ],
    )
    .await;
    w.identify("Breaking Bad", "tvdb_id = 81189, tmdb_id = NULL").await;

    let show = w.app.ids(w.shows, &w.admin).await["Breaking Bad"];
    let detail = w.app.get("/api/discover/show/1396", &w.ann).await;
    assert_eq!(detail.status, StatusCode::OK, "{}", detail.text());
    assert_eq!(
        (&detail.body["status"], &detail.body["item_id"]),
        (&json!("partial"), &json!(show)),
        "season 2 to come"
    );
    assert_eq!(detail.body["certification"], json!("TV-MA"));
    assert_eq!(detail.body["imdb_id"], json!("tt0903747"));
    assert_eq!(detail.body["poster_url"], json!("/api/discover/images/poster/bb.jpg"));
    let seasons = |detail: &common::Reply| -> Vec<(i64, i64, i64, String)> {
        detail.body["seasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let number = |name: &str| s[name].as_i64().unwrap();
                (number("number"), number("aired"), number("in_library"), s["state"].as_str().unwrap().to_owned())
            })
            .collect()
    };
    let state = |n: i64, aired, there, state: &str| (n, aired, there, state.to_owned());
    assert_eq!(seasons(&detail), [state(0, 2, 0, "missing"), state(1, 3, 3, "library"), state(2, 2, 0, "missing")]);
    assert_eq!((&detail.body["declined"], &detail.body["restriction"]), (&Value::Null, &Value::Null));
    assert_eq!((&detail.body["watching"], &detail.body["requests"]), (&json!(false), &json!([])));
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    let found = &trending.body["results"][1];
    assert_eq!((&found["status"], &found["item_id"]), (&json!("partial"), &json!(show)), "by TheTVDB's id");
    // Its page in the library: TMDB says which title it is.
    let page = w.app.get(&format!("/api/items/{show}/request"), &w.ann).await;
    assert_eq!(
        page.body,
        json!({"kind": "show", "tmdb_id": 1396, "status": "partial", "request": null, "askable": [2]}),
        "{}",
        page.text()
    );
    assert!(w.services.tmdb.lock().iter().any(|r| r.starts_with("/find/81189?")));
    assert_eq!(w.app.get("/api/items/999999/request", &w.ann).await.status, StatusCode::NOT_FOUND);

    let unknown = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 1396, "seasons": [5]})).await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    let whole = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 1396, "seasons": [1]})).await;
    assert_eq!(whole.status, StatusCode::CONFLICT, "a library has it whole");
    // Every season but specials, less the one there.
    let asked = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 1396})).await;
    assert_eq!(asked.status, StatusCode::CREATED, "{}", asked.text());
    assert_eq!(asked.body["seasons"], json!([2]));
    let id = asked.body["id"].as_i64().unwrap();
    // Season 1 is there, season 2 asked for: Bob follows hers.
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    let twice = w.ask(&bob, json!({"kind": "show", "tmdb_id": 1396, "seasons": [1, 2]})).await;
    assert_eq!(twice.status, StatusCode::OK, "{}", twice.text());
    assert_eq!((&twice.body["id"], &twice.body["watching"]), (&json!(id), &json!(true)));
    let detail = w.app.get("/api/discover/show/1396", &w.admin).await;
    let requested: Vec<bool> =
        detail.body["seasons"].as_array().unwrap().iter().map(|s| s["requested"].as_bool().unwrap()).collect();
    assert_eq!(requested, [false, false, true]);
    assert_eq!(seasons(&detail)[2], state(2, 2, 0, "requested"));
    assert_eq!(detail.body["seasons"][2]["request_id"], json!(id));
    assert_eq!((&detail.body["status"], &detail.body["request_id"]), (&json!("requested"), &json!(id)));
    assert_eq!(detail.body["requests"][0]["id"], json!(id), "administrators see everyone's");
    let page = w.app.get(&format!("/api/items/{show}/request"), &w.ann).await;
    assert_eq!((&page.body["status"], &page.body["askable"]), (&json!("requested"), &json!([])));
    assert_eq!(page.body["request"]["id"], json!(id), "hers");

    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    w.app.state.requests.settled().await;
    let added = w.services.sonarr_calls("POST", "/series");
    assert_eq!(added.len(), 1);
    let series = &added[0].body;
    assert_eq!((&series["tvdbId"], &series["qualityProfileId"]), (&json!(81189), &json!(6)));
    assert_eq!((&series["rootFolderPath"], &series["seriesType"]), (&json!("/tv"), &json!("standard")));
    assert_eq!((&series["seasonFolder"], &series["monitored"]), (&json!(true), &json!(true)));
    let monitored: Vec<bool> =
        series["seasons"].as_array().unwrap().iter().map(|s| s["monitored"].as_bool().unwrap()).collect();
    assert_eq!(monitored, [false, false, true], "the season asked for only");
    assert_eq!(series["addOptions"]["searchForMissingEpisodes"], json!(true));
    assert!(series.get("languageProfileId").is_none(), "Sonarr 4 has none");
    // Its queue, for the seasons asked for.
    w.app.state.requests.refresh_queues().await;
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(mine.body["download"], json!({"progress": 0.0, "state": "queued", "eta": null, "episodes": 1}));

    // One episode of season 2, then both.
    w.add(w.shows, &["shows/Breaking Bad (2008)/Season 02/Breaking Bad - S02E01.mkv"]).await;
    w.app.state.requests.sync().await.unwrap();
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(status(&mine), (StatusCode::OK, "approved".to_owned()));
    assert_eq!(mine.body["available_seasons"], json!([]));
    w.add(w.shows, &["shows/Breaking Bad (2008)/Season 02/Breaking Bad - S02E02.mkv"]).await;
    w.app.state.requests.sync().await.unwrap();
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(status(&mine), (StatusCode::OK, "available".to_owned()), "{}", mine.text());
    assert_eq!(mine.body["available_seasons"], json!([2]));
    assert!(mine.body["item_id"].is_i64());
}

#[tokio::test]
async fn answers_never_wait_for_radarrs_and_sonarrs_queues() {
    let w = world().await;
    w.turn_on(json!({})).await;
    // Their queues take 30 seconds to answer.
    *w.services.queue_delay.lock() = Some(Duration::from_secs(30));
    let movie = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    let show = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 1396})).await;
    let at_once = Duration::from_secs(2);
    for asked in [&movie, &show] {
        let uri = format!("/api/requests/{}/approve", asked.body["id"]);
        let approved = tokio::time::timeout(at_once, w.app.post(&uri, &w.admin, json!({}))).await.expect(&uri);
        assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    }
    w.app.state.requests.settled().await;
    let id = movie.body["id"].as_i64().unwrap();
    for uri in [format!("/api/requests/{id}"), "/api/requests".to_owned(), "/api/discover/movie/603".to_owned()] {
        let answer = tokio::time::timeout(at_once, w.app.get(&uri, &w.ann)).await.expect(&uri);
        assert_eq!(answer.status, StatusCode::OK, "{uri}: {}", answer.text());
    }
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!((&mine.body["stage"], &mine.body["download"]), (&json!("searching"), &Value::Null), "not known yet");

    // Once Radarr answers, its download shows.
    *w.services.queue_delay.lock() = None;
    w.app.state.requests.refresh_queues().await;
    let mine = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!((&mine.body["stage"], &mine.body["download"]["progress"]), (&json!("downloading"), &json!(0.6)));
}

#[tokio::test]
async fn a_request_is_available_once_a_library_its_maker_can_open_has_it() {
    let w = world().await;
    w.turn_on(json!({})).await;
    // Bob opens the shows only: the movies aren't his to watch.
    let (bob, bob_id) = w.app.user(&w.admin, "bob", json!({"all_libraries": false, "library_ids": [w.shows]})).await;
    w.add(w.movies, &["movies/The Matrix (1999).mkv"]).await;
    w.identify("The Matrix", "tmdb_id = 603").await;
    let asked = w.ask(&bob, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(asked.status, StatusCode::CREATED, "only a library he can't open has it: {}", asked.text());
    let id = asked.body["id"].as_i64().unwrap();
    w.app.state.requests.sync().await.unwrap();
    let his = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!((&his.body["status"], &his.body["item_id"]), (&json!("pending"), &Value::Null), "{}", his.text());

    // Let into the movies: it is.
    let reply = w.app.patch(&format!("/api/users/{bob_id}"), &w.admin, json!({"all_libraries": true})).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    w.app.state.requests.sync().await.unwrap();
    let his = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!(status(&his), (StatusCode::OK, "available".to_owned()), "{}", his.text());
    assert!(his.body["item_id"].is_i64());
}

#[tokio::test]
async fn anime_goes_to_its_own_folder_and_limits_count_seasons() {
    let w = world().await;
    w.turn_on(json!({"request_limit_seasons": 2, "request_limit_movies": 1})).await;
    let user = format!("/api/users/{}", w.ann_id);
    let reply = w.app.patch(&user, &w.admin, json!({"request_access": "auto"})).await;
    assert_eq!((reply.status, &reply.body["request_access"]), (StatusCode::OK, &json!("auto")));

    let anime = w.app.get("/api/discover/anime", &w.ann).await;
    assert_eq!(anime.body["results"][0]["title"], json!("Space Cowboys"), "{}", anime.text());
    let detail = w.app.get("/api/discover/show/9001", &w.ann).await;
    assert_eq!((&detail.body["anime"], &detail.body["original_title"]), (&json!(true), &json!("Uchuu no Kaubōi")));

    // All three seasons: more than the limit itself.
    let too_many = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 9001})).await;
    assert_eq!(too_many.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(too_many.body["message"], json!("this account may ask for at most 2 seasons per 7 days: ask for fewer"));
    assert_eq!(too_many.body["retry_at"], Value::Null);
    assert!(!too_many.headers.contains_key(header::RETRY_AFTER), "waiting won't help");
    // Approved as asked, to anime's own folder.
    let asked = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 9001, "seasons": [2, 1]})).await;
    assert_eq!(status(&asked), (StatusCode::CREATED, "approved".to_owned()), "{}", asked.text());
    assert_eq!((&asked.body["seasons"], &asked.body["anime"]), (&json!([1, 2]), &json!(true)));
    assert_eq!(asked.body["decided_by"]["name"], json!("ann"));
    w.app.state.requests.settled().await;
    let series = &w.services.sonarr_calls("POST", "/series")[0].body;
    assert_eq!((&series["rootFolderPath"], &series["seriesType"]), (&json!("/anime"), &json!("anime")));
    let sent = w.app.get(&format!("/api/requests/{}", asked.body["id"]), &w.admin).await;
    assert_eq!(
        sent.body["sent_to"],
        json!({"service": "sonarr", "quality_profile": "HD-720p", "root_folder": "/anime"}),
        "{}",
        sent.text()
    );

    let made = asked.body["created_at"].as_i64().unwrap();
    let none_left = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 9001, "seasons": [3]})).await;
    assert_eq!(none_left.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(none_left.body["message"].as_str().unwrap().starts_with("this account has asked for all the seasons"));
    assert_eq!(
        (&none_left.body["code"], &none_left.body["retry_at"]),
        (&json!("too_many_requests"), &json!(made + 7 * 86_400))
    );
    let wait: i64 = none_left.headers[header::RETRY_AFTER].to_str().unwrap().parse().unwrap();
    assert!((7 * 86_400 - 60..=7 * 86_400).contains(&wait), "{wait}");
    let summary = w.app.get("/api/requests/summary", &w.ann).await;
    assert_eq!((&summary.body["access"], &summary.body["seasons_left"]), (&json!("auto"), &json!(0)));
    assert_eq!((&summary.body["movies_left"], &summary.body["own"]), (&json!(1), &json!(1)));
    assert_eq!((&summary.body["movies_limit"], &summary.body["seasons_limit"]), (&json!(1), &json!(2)));
    assert_eq!(summary.body["seasons_next_at"], json!(made + 7 * 86_400));
    assert_eq!(summary.body["movies_next_at"], Value::Null, "movies are left");
    let detail = w.app.get("/api/discover/show/9001", &w.ann).await;
    assert_eq!((&detail.body["seasons_left"], &detail.body["seasons_next_at"]), (&json!(0), &json!(made + 7 * 86_400)));
    // Administrators have no limit, and a show there already gets the
    // seasons asked for monitored and searched for.
    let more = w.ask(&w.admin, json!({"kind": "show", "tmdb_id": 9001, "seasons": [3]})).await;
    assert_eq!(status(&more), (StatusCode::CREATED, "approved".to_owned()), "{}", more.text());
    w.app.state.requests.settled().await;
    assert_eq!(w.services.sonarr_calls("POST", "/series").len(), 1);
    let put = w.services.sonarr_calls("PUT", "/series/21");
    let monitored: Vec<bool> =
        put[0].body["seasons"].as_array().unwrap().iter().map(|s| s["monitored"].as_bool().unwrap()).collect();
    assert_eq!(monitored, [false, true, true, true]);
    let searches = w.services.sonarr_calls("POST", "/command");
    assert_eq!(searches.last().unwrap().body, json!({"name": "SeasonSearch", "seriesId": 21, "seasonNumber": 3}));
}

#[tokio::test]
async fn failures_say_why_and_approving_again_retries() {
    let w = world().await;
    w.turn_on(json!({})).await;
    *w.services.refusals.lock() = 1;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let mut events = w.app.state.events.subscribe();
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()));
    w.app.state.requests.settled().await;
    let failed = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(status(&failed), (StatusCode::OK, "failed".to_owned()));
    assert_eq!(failed.body["reason"], json!("Radarr refused: Root folder is not writable"));
    assert_eq!(failed.body["actions"], json!(["retry", "decline", "handled", "delete"]));
    assert_eq!(failed.body["retry_at"], Value::Null, "nothing tries again by itself");
    let heard: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        heard.iter().any(|e| matches!(e,
            ServerEvent::RequestFailed { to, request_id, reason, retry_at: None, .. }
                if to.users.is_empty() && to.managers && *request_id == id
                    && reason == "Radarr refused: Root folder is not writable")),
        "managers hear it: {heard:?}"
    );
    // Left for a manager: counted for them, not against her limits.
    let summary = w.app.get("/api/requests/summary", &w.admin).await;
    assert_eq!((&summary.body["pending"], &summary.body["failed"]), (&json!(0), &json!(1)));
    let summary = w.app.get("/api/requests/summary", &w.ann).await;
    assert_eq!(
        (&summary.body["manage"], &summary.body["failed"], &summary.body["own"]),
        (&json!(false), &json!(0), &json!(1))
    );
    // Again, with another profile and folder.
    let approved = w
        .app
        .post(&format!("/api/requests/{id}/approve"), &w.admin, json!({"quality_profile": 5, "root_folder": " /4k "}))
        .await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    assert_eq!(approved.body["reason"], Value::Null);
    w.app.state.requests.settled().await;
    let movie = &w.services.radarr_calls("POST", "/movie")[1].body;
    assert_eq!((&movie["qualityProfileId"], &movie["rootFolderPath"]), (&json!(5), &json!("/4k")));
    w.app.state.requests.refresh_queues().await;
    let sent = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(
        (&sent.body["stage"], &sent.body["sent_to"]),
        (&json!("downloading"), &json!({"service": "radarr", "quality_profile": "Ultra-HD", "root_folder": "/4k"}))
    );

    // A key Radarr refuses.
    let reply = w.app.put("/api/settings", &w.admin, json!({"radarr_api_key": "wrong"})).await;
    assert_eq!(reply.status, StatusCode::OK);
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await;
    let id = asked.body["id"].as_i64().unwrap();
    w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let failed = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(failed.body["reason"], json!("Radarr rejected the API key"));
    // Without Radarr, approving is all there is.
    let reply = w.app.put("/api/settings", &w.admin, json!({"radarr_url": ""})).await;
    assert_eq!(reply.status, StatusCode::OK);
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()));
    assert_eq!((&approved.body["stage"], &approved.body["download"]), (&json!("approved"), &Value::Null));
    assert_eq!(approved.body["actions"], json!(["decline", "handled", "delete"]), "nothing to send it to");
}

#[tokio::test]
async fn pleys_own_words_on_requests_are_in_each_managers_language() {
    const ENGLISH: &str = "Radarr refused: Root folder is not writable";
    const FRENCH: &str = "Radarr a refusé\u{a0}: Root folder is not writable";
    const SPANISH: &str = "Radarr lo rechazó: Root folder is not writable";
    let w = world().await;
    w.turn_on(json!({})).await;
    // Another administrator, who reads Spanish on any device.
    let (eva, _) = w.app.user(&w.admin, "eva", json!({"is_admin": true})).await;
    let chosen = w.app.patch("/api/me", &eva, json!({"language": "es"})).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
    // The first one on a French device, and on a German one: the server
    // language there, English.
    let mut french = events_in(&w.app, &w.admin, Some("fr-FR,fr;q=0.9")).await;
    let mut german = events_in(&w.app, &w.admin, Some("de-DE")).await;
    let mut spanish = events_in(&w.app, &eva, Some("fr")).await;

    *w.services.refusals.lock() = 1;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()));
    w.app.state.requests.settled().await;
    w.app.state.events.send(ServerEvent::LibrariesChanged);
    // Each stream hears it once, Pley's words in its language and
    // Radarr's as Radarr said them.
    for (stream, reason) in [(&mut french, FRENCH), (&mut german, ENGLISH), (&mut spanish, SPANISH)] {
        let heard = heard(stream).await;
        assert_eq!(heard.matches(r#""type":"request_failed""#).count(), 1, "{heard}");
        assert!(heard.contains(&format!(r#""reason":"{reason}""#)), "{heard}");
    }

    // The request reads the same way: the account's language, else the
    // device's, else the server language.
    let uri = format!("/api/requests/{id}");
    let read = w.app.call(Method::GET, &uri, Some(&w.admin), None, &[("accept-language", "fr")]).await;
    assert_eq!(read.body["reason"], json!(FRENCH), "{}", read.text());
    assert_eq!(w.app.get(&uri, &eva).await.body["reason"], json!(SPANISH));
    let list =
        w.app.call(Method::GET, "/api/requests?status=failed", Some(&eva), None, &[("accept-language", "fr")]).await;
    assert_eq!(list.body["requests"][0]["reason"], json!(SPANISH), "{}", list.text());
    assert_eq!(w.app.get(&uri, &w.admin).await.body["reason"], json!(ENGLISH));
    let saved = w.app.put("/api/settings", &w.admin, json!({"language": "fr"})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(w.app.get(&uri, &w.admin).await.body["reason"], json!(FRENCH));
    // Words kept before Pley kept what happened stay as they were.
    sqlx::query("UPDATE requests SET reason = 'Radarr refused: Movie exists' WHERE id = ?")
        .bind(id)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    assert_eq!(w.app.get(&uri, &eva).await.body["reason"], json!("Radarr refused: Movie exists"));

    // A key Radarr refuses, a request added by hand, and Sonarr where
    // nothing answers.
    let reply = w.app.put("/api/settings", &w.admin, json!({"radarr_api_key": "wrong"})).await;
    assert_eq!(reply.status, StatusCode::OK);
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await;
    let id = asked.body["id"].as_i64().unwrap();
    w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let uri = format!("/api/requests/{id}");
    assert_eq!(w.app.get(&uri, &w.admin).await.body["reason"], json!("Radarr a refusé la clé d’API"));
    assert_eq!(w.app.get(&uri, &eva).await.body["reason"], json!("Radarr rechazó la clave de API"));
    let handled = w.app.post(&format!("/api/requests/{id}/handled"), &eva, json!({})).await;
    assert_eq!(handled.body["note"], json!("Añadido a mano"), "{}", handled.text());
    let english = w.app.call(Method::GET, &uri, Some(&w.admin), None, &[("accept-language", "en")]).await;
    assert_eq!(english.body["note"], json!("Added by hand"));
    let moved =
        w.app.put("/api/settings", &w.admin, json!({"sonarr_url": "http://127.0.0.1:9", "sonarr_api_key": "k"})).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let options = w.app.get("/api/requests/options", &eva).await;
    assert_eq!(options.body["sonarr"]["error"], json!("Sonarr no responde"), "{}", options.text());
    let options = w.app.get("/api/requests/options", &w.admin).await;
    assert_eq!(options.body["sonarr"]["error"], json!("Sonarr ne répond pas"));
    // A send that can't reach Radarr says so the same way.
    let moved =
        w.app.put("/api/settings", &w.admin, json!({"radarr_url": "http://127.0.0.1:9", "radarr_api_key": "k"})).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let asked = w.ask(&w.admin, json!({"kind": "movie", "tmdb_id": 606})).await;
    assert_eq!(status(&asked), (StatusCode::CREATED, "approved".to_owned()), "{}", asked.text());
    w.app.state.requests.settled().await;
    let uri = format!("/api/requests/{}", asked.body["id"]);
    assert_eq!(w.app.get(&uri, &w.admin).await.body["reason"], json!("Radarr ne répond pas"));
    assert_eq!(w.app.get(&uri, &eva).await.body["reason"], json!("Radarr no responde"));
}

#[tokio::test]
async fn a_send_cut_short_is_made_again_as_it_was_approved() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    // Approved with another profile and folder, and the server stopped
    // before Radarr had it.
    sqlx::query(
        "UPDATE requests SET status = 'approved', decided_by = 1, decided_at = updated_at, sending_at = updated_at,
                             send_profile = 5, send_folder = '/4k'
         WHERE id = ?",
    )
    .bind(id)
    .execute(&w.app.state.db.write)
    .await
    .unwrap();
    let sending = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(
        (&sending.body["stage"], &sending.body["actions"]),
        (&json!("approved"), &json!(["decline", "delete"])),
        "being sent: {}",
        sending.text()
    );
    let again = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&again), (StatusCode::OK, "approved".to_owned()), "as it is");
    w.app.state.requests.settled().await;
    assert!(w.services.radarr_calls("POST", "/movie").is_empty(), "approving it again sends nothing");

    // At the start: sent with what was chosen.
    w.app.state.requests.resume().await.unwrap();
    w.app.state.requests.settled().await;
    let added = w.services.radarr_calls("POST", "/movie");
    assert_eq!(added.len(), 1);
    assert_eq!((&added[0].body["qualityProfileId"], &added[0].body["rootFolderPath"]), (&json!(5), &json!("/4k")));
    let sent = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(
        sent.body["sent_to"],
        json!({"service": "radarr", "quality_profile": "Ultra-HD", "root_folder": "/4k"}),
        "{}",
        sent.text()
    );
    w.app.state.requests.resume().await.unwrap();
    w.app.state.requests.settled().await;
    assert_eq!(w.services.radarr_calls("POST", "/movie").len(), 1, "sent once");

    // How it went can't be saved: failed, for a manager to see.
    sqlx::query(
        "CREATE TRIGGER unsaved BEFORE UPDATE OF sent_to ON requests WHEN NEW.sent_to IS NOT NULL
         BEGIN SELECT RAISE(FAIL, 'database or disk is full'); END",
    )
    .execute(&w.app.state.db.write)
    .await
    .unwrap();
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let mut events = w.app.state.events.subscribe();
    w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let failed = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(status(&failed), (StatusCode::OK, "failed".to_owned()), "{}", failed.text());
    let reason = failed.body["reason"].as_str().unwrap();
    assert!(reason.starts_with("Pley couldn't save how sending it went"), "{reason}");
    assert!(
        said(&mut events)
            .iter()
            .any(|e| matches!(e, ServerEvent::RequestFailed { request_id, .. } if *request_id == id))
    );
    assert_eq!(w.app.get("/api/requests/summary", &w.admin).await.body["failed"], json!(1));
    // Approved again, it is found in Radarr, where it got.
    sqlx::query("DROP TRIGGER unsaved").execute(&w.app.state.db.write).await.unwrap();
    w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let sent = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(sent.body["sent_to"]["service"], json!("radarr"), "{}", sent.text());
    assert_eq!(w.services.radarr_calls("POST", "/movie").len(), 2, "added once");
}

#[tokio::test]
async fn requests_are_declined_withdrawn_and_kept_to_their_makers() {
    let w = world().await;
    w.turn_on(json!({"request_limit_movies": 1})).await;
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();

    // Others' requests aren't there for Bob.
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &bob).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.app.delete(&format!("/api/requests/{id}"), &bob).await.status, StatusCode::NOT_FOUND);
    let list = w.app.get("/api/requests", &bob).await;
    assert_eq!((&list.body["total"], &list.body["requests"]), (&json!(0), &json!([])));
    let search = w.app.get("/api/discover/search?q=matrix", &bob).await;
    let reloaded = &search.body["results"][1];
    assert_eq!((&reloaded["status"], &reloaded["request_id"]), (&json!("requested"), &Value::Null));

    let long = "x".repeat(501);
    let refused = w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": long})).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    let declined =
        w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": " Not this one "})).await;
    assert_eq!(status(&declined), (StatusCode::OK, "declined".to_owned()));
    assert_eq!(declined.body["reason"], json!("Not this one"));
    assert_eq!(declined.body["actions"], json!(["approve", "handled", "delete"]));
    let twice = w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({})).await;
    assert_eq!((twice.status, &twice.body["request"]["status"]), (StatusCode::CONFLICT, &json!("declined")));
    // A declined request doesn't count.
    let summary = w.app.get("/api/requests/summary", &w.ann).await;
    assert_eq!(summary.body["movies_left"], json!(1));
    let other = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await;
    assert_eq!(other.status, StatusCode::CREATED, "{}", other.text());
    let past = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(past.status, StatusCode::TOO_MANY_REQUESTS);

    let all = w.app.get("/api/requests?status=pending", &w.admin).await;
    assert_eq!((&all.body["total"], &all.body["counts"]["declined"]), (&json!(1), &json!(1)));
    assert_eq!(all.body["requests"][0]["id"], other.body["id"]);
    let mine = w.app.get("/api/requests?mine=1", &w.admin).await;
    assert_eq!(mine.body["total"], json!(0), "the administrator asked for nothing");
    assert_eq!(w.app.get("/api/requests?status=lost", &w.ann).await.status, StatusCode::BAD_REQUEST);

    // Ann withdraws hers; a declined one stays for 30 days.
    let decided = declined.body["decided_at"].as_i64().unwrap();
    let hers = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(hers.body["actions"], json!([]), "{}", hers.text());
    let kept = w.app.delete(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!((kept.status, &kept.body["code"]), (StatusCode::CONFLICT, &json!("conflict")), "{}", kept.text());
    assert_eq!((&kept.body["retry_at"], &kept.body["request"]["id"]), (&json!(decided + 30 * 86_400), &json!(id)));
    sqlx::query("UPDATE requests SET decided_at = decided_at - 31 * 86400 WHERE id = ?")
        .bind(id)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    let hers = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(hers.body["actions"], json!(["withdraw", "ask_again"]), "{}", hers.text());
    assert_eq!(w.app.delete(&format!("/api/requests/{id}"), &w.ann).await.status, StatusCode::NO_CONTENT);
    let other_id = other.body["id"].as_i64().unwrap();
    assert_eq!(w.app.delete(&format!("/api/requests/{other_id}"), &w.ann).await.status, StatusCode::NO_CONTENT);
    let list = w.app.get("/api/requests", &w.ann).await;
    assert_eq!(list.body["total"], json!(0));
}

#[tokio::test]
async fn who_may_ask() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let kid = w.app.post("/api/users", &w.admin, json!({"username": "kid", "managed": true})).await;
    assert_eq!((kid.status, &kid.body["request_access"]), (StatusCode::CREATED, &json!("off")), "{}", kid.text());
    let (teen, _) = w.app.user(&w.admin, "teen", json!({"max_age": 13})).await;
    let summary = w.app.get("/api/requests/summary", &teen).await;
    assert_eq!((&summary.body["enabled"], &summary.body["access"]), (&json!(true), &json!("off")));
    for uri in ["/api/discover/trending", "/api/discover/movie/603"] {
        let refused = w.app.get(uri, &teen).await;
        assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("no_access")), "{uri}");
    }
    let refused = w.ask(&teen, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("no_access")));
    // Its own requests it still has (none), and their pictures only.
    let list = w.app.get("/api/requests", &teen).await;
    assert_eq!((list.status, &list.body["total"]), (StatusCode::OK, &json!(0)));
    let poster = w.app.get("/api/discover/images/poster/matrix.jpg", &teen).await;
    assert_eq!((poster.status, &poster.body["code"]), (StatusCode::FORBIDDEN, &json!("no_access")));

    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let mut events = w.app.state.events.subscribe();
    let user = format!("/api/users/{}", w.ann_id);
    let reply = w.app.patch(&user, &w.admin, json!({"request_access": "off"})).await;
    assert_eq!(reply.body["request_access"], json!("off"));
    let heard: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(heard.iter().any(|e| matches!(e,
        ServerEvent::RequestsSettingsChanged { user_id: Some(user_id) } if *user_id == w.ann_id)));
    assert_eq!(w.app.get("/api/discover/trending", &w.ann).await.status, StatusCode::FORBIDDEN);
    // She keeps hers: she reads it, with its pictures, and withdraws it.
    let summary = w.app.get("/api/requests/summary", &w.ann).await;
    assert_eq!((&summary.body["access"], &summary.body["own"]), (&json!("off"), &json!(1)));
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &w.ann).await.status, StatusCode::OK);
    for (uri, status) in [
        ("/api/discover/images/poster/matrix.jpg?w=200", StatusCode::OK),
        ("/api/discover/images/backdrop/matrixwide.jpg", StatusCode::OK),
        ("/api/discover/images/profile/keanu.jpg", StatusCode::FORBIDDEN),
        ("/api/discover/images/poster/bb.jpg", StatusCode::FORBIDDEN),
    ] {
        assert_eq!(w.app.get(uri, &w.ann).await.status, status, "{uri}");
    }
    assert_eq!(w.app.delete(&format!("/api/requests/{id}"), &w.ann).await.status, StatusCode::NO_CONTENT);
    let wrong = w.app.patch(&user, &w.admin, json!({"request_access": "always"})).await;
    assert!(wrong.status.is_client_error());

    // An age limit added: no more asking until an administrator lets it,
    // unless the same change says so.
    let (_, bob) = w.app.user(&w.admin, "bob", json!({})).await;
    let (_, cid) = w.app.user(&w.admin, "cid", json!({})).await;
    let limited = w.app.patch(&format!("/api/users/{bob}"), &w.admin, json!({"max_age": 13})).await;
    assert_eq!(limited.body["request_access"], json!("off"), "{}", limited.text());
    let both =
        w.app.patch(&format!("/api/users/{cid}"), &w.admin, json!({"max_age": 13, "request_access": "ask"})).await;
    assert_eq!(both.body["request_access"], json!("ask"), "{}", both.text());
    w.app.patch(&format!("/api/users/{bob}"), &w.admin, json!({"request_access": "ask"})).await;
    let changed = w.app.patch(&format!("/api/users/{bob}"), &w.admin, json!({"max_age": 16})).await;
    assert_eq!(changed.body["request_access"], json!("ask"), "only adding one changes it");
}

#[tokio::test]
async fn age_limits_keep_titles_above_them_out() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let (teen, teen_id) = w.app.user(&w.admin, "teen", json!({"max_age": 13})).await;
    let let_ask = w.app.patch(&format!("/api/users/{teen_id}"), &w.admin, json!({"request_access": "ask"})).await;
    assert_eq!(let_ask.body["request_access"], json!("ask"));
    // Rated R and TV-MA: left out, even when that leaves a page empty.
    let trending = w.app.get("/api/discover/trending", &teen).await;
    assert_eq!(trending.status, StatusCode::OK, "{}", trending.text());
    assert_eq!((&trending.body["results"], &trending.body["total_pages"]), (&json!([]), &json!(3)));
    let found = w.app.get("/api/discover/search?q=matrix", &teen).await;
    assert_eq!(found.body["results"], json!([]));
    // Popular movies come from TMDB within the limit.
    let movies = w.app.get("/api/discover/movies", &teen).await;
    assert_eq!(common::titles(&movies.body["results"]), ["Toy Robots"], "{}", movies.text());
    let asked: Vec<String> =
        w.services.tmdb.lock().iter().filter(|r| r.starts_with("/discover/movie?")).cloned().collect();
    assert!(asked[0].contains("certification.lte=PG-13") && asked[0].contains("certification_country=US"), "{asked:?}");
    // A title page reached another way says why.
    let detail = w.app.get("/api/discover/movie/603", &teen).await;
    assert_eq!(
        (&detail.body["status"], &detail.body["restriction"]),
        (&json!("restricted"), &json!("Rated R: above this account's age limit")),
        "{}",
        detail.text()
    );
    let refused = w.ask(&teen, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(
        (refused.status, &refused.body["code"], &refused.body["message"]),
        (StatusCode::FORBIDDEN, &json!("restricted"), &json!("Rated R: above this account's age limit"))
    );
    let asked = w.ask(&teen, json!({"kind": "movie", "tmdb_id": 606})).await;
    assert_eq!(asked.status, StatusCode::CREATED, "{}", asked.text());
    // Without one, nothing is left out.
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(trending.body["results"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn pictures_come_through_pley_and_are_kept() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let poster = w.app.get("/api/discover/images/poster/matrix.jpg", &w.ann).await;
    assert_eq!(poster.status, StatusCode::OK, "{}", poster.text());
    assert_eq!(poster.bytes, JPEG);
    assert_eq!(poster.headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(poster.headers[header::CACHE_CONTROL], "private, max-age=604800, immutable");
    let kept = w.app.state.config.cache_dir().join("discover").join("w342").join("matrix.jpg");
    assert_eq!(std::fs::read(&kept).unwrap(), JPEG, "kept as a file");
    // Kept: TMDB isn't asked again. Each width gets TMDB's size for it.
    assert_eq!(w.app.get("/api/discover/images/poster/matrix.jpg", &w.ann).await.bytes, JPEG);
    for uri in [
        "/api/discover/images/poster/matrix.jpg?w=100",
        "/api/discover/images/backdrop/matrixwide.jpg?w=3840",
        "/api/discover/images/profile/keanu.jpg?w=300",
        "/api/discover/images/still/episode.png?w=wide",
    ] {
        assert_eq!(w.app.get(uri, &w.ann).await.status, StatusCode::OK, "{uri}");
    }
    assert_eq!(
        *w.services.pictures.lock(),
        ["/w342/matrix.jpg", "/w154/matrix.jpg", "/original/matrixwide.jpg", "/h632/keanu.jpg", "/w300/episode.png"]
    );

    // TMDB has no such picture: believed for a while.
    for _ in 0..2 {
        let gone = w.app.get("/api/discover/images/poster/gone.jpg", &w.ann).await;
        assert_eq!((gone.status, &gone.body["code"]), (StatusCode::NOT_FOUND, &json!("not_found")));
    }
    assert_eq!(w.services.pictures.lock().iter().filter(|p| p.ends_with("/gone.jpg")).count(), 1);
    // TMDB failing: said so, and nothing kept or remembered.
    *w.services.picture_failures.lock() = 1;
    let down = w.app.get("/api/discover/images/poster/later.jpg", &w.ann).await;
    assert_eq!((down.status, &down.body["code"]), (StatusCode::BAD_GATEWAY, &json!("tmdb_down")), "{}", down.text());
    assert_eq!(down.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(w.app.get("/api/discover/images/poster/later.jpg", &w.ann).await.status, StatusCode::OK);
    // TMDB's own names and kinds only: nothing else is asked for.
    let asked = w.services.pictures.lock().len();
    for uri in [
        "/api/discover/images/poster/matrix.svg",
        "/api/discover/images/poster/..%2Fsecret.jpg",
        "/api/discover/images/poster/a-b.jpg",
        "/api/discover/images/logo/matrix.jpg",
    ] {
        let refused = w.app.get(uri, &w.ann).await;
        assert_eq!((refused.status, &refused.body["code"]), (StatusCode::NOT_FOUND, &json!("not_found")), "{uri}");
    }
    assert_eq!(w.services.pictures.lock().len(), asked);

    // Apps: with `requests`, the token in the header.
    let (app, _) = w.app.app_token(w.ann_id, "library requests").await;
    assert_eq!(w.app.get("/api/discover/images/poster/matrix.jpg", &app).await.status, StatusCode::OK);
    let (library, _) = w.app.app_token(w.ann_id, "library").await;
    let refused = w.app.get("/api/discover/images/poster/matrix.jpg", &library).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    let in_url = format!("/api/discover/images/poster/matrix.jpg?token={app}");
    assert_eq!(w.app.call(Method::GET, &in_url, None, None, &[]).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn news_is_unseen_until_looked_at() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let summary = w.app.get("/api/requests/summary", &w.ann).await;
    assert_eq!((&summary.body["own"], &summary.body["unseen"]), (&json!(1), &json!(0)), "asking is no news");
    assert_eq!(summary.body["changed_at"], asked.body["updated_at"]);
    let summary = w.app.get("/api/requests/summary", &w.admin).await;
    assert_eq!((&summary.body["pending"], &summary.body["own"]), (&json!(1), &json!(0)));
    assert_eq!(summary.body["changed_at"], asked.body["updated_at"], "managers: everyone's");
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    assert_eq!(w.app.get("/api/requests/summary", &bob).await.body["changed_at"], Value::Null, "not his");

    let declined = w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": "No"})).await;
    assert_eq!(status(&declined), (StatusCode::OK, "declined".to_owned()));
    assert_eq!(w.app.get("/api/requests/summary", &w.ann).await.body["unseen"], json!(1));
    let mut events = w.app.state.events.subscribe();
    let seen = w.app.post("/api/requests/seen", &w.ann, json!({})).await;
    assert_eq!(seen.status, StatusCode::NO_CONTENT);
    let heard: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        heard.iter().any(|e| matches!(e,
            ServerEvent::RequestsChanged { to, request_id: None, kind: None, tmdb_id: None }
                if to.users == [w.ann_id] && !to.managers && to.except_token.is_some())),
        "her other devices clear their badges too: {heard:?}"
    );
    assert_eq!(w.app.get("/api/requests/summary", &w.ann).await.body["unseen"], json!(0));

    // Declined lately: Discover says so, and until when.
    let detail = w.app.get("/api/discover/movie/604", &w.ann).await;
    assert_eq!(detail.body["status"], json!("declined"), "{}", detail.text());
    let at = declined.body["decided_at"].as_i64().unwrap();
    assert_eq!(detail.body["declined"], json!({"reason": "No", "at": at, "again_at": at + 30 * 86_400}));
    let found = w.app.get("/api/discover/search?q=matrix", &w.ann).await;
    assert_eq!(found.body["results"][1]["status"], json!("declined"));
    let found = w.app.get("/api/discover/search?q=matrix", &bob).await;
    assert_eq!(found.body["results"][1]["status"], json!("none"), "declined for her only");
}

#[tokio::test]
async fn a_request_says_where_it_is_and_whose_it_is() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    assert_eq!(asked.status, StatusCode::CREATED, "{}", asked.text());
    let id = asked.body["id"].as_i64().unwrap();
    assert_eq!(
        (&asked.body["stage"], &asked.body["mine"], &asked.body["watching"], &asked.body["unseen"]),
        (&json!("pending"), &json!(true), &json!(false), &json!(false))
    );
    for field in ["stage_at", "note", "retry_at", "sent_to"] {
        assert_eq!(asked.body[field], Value::Null, "{field}");
    }

    // Bob asks for it too, and follows hers: he sees where it is, not who
    // asked.
    let followed = w.ask(&bob, json!({"kind": "movie", "tmdb_id": 604})).await;
    assert_eq!(followed.status, StatusCode::OK, "{}", followed.text());
    let his = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!(his.status, StatusCode::OK, "{}", his.text());
    assert_eq!(his.body, followed.body);
    assert_eq!(
        (&his.body["mine"], &his.body["watching"], &his.body["watchers"], &his.body["requested_by"]),
        (&json!(false), &json!(true), &json!(0), &Value::Null)
    );
    assert_eq!(w.app.get("/api/requests", &bob).await.body["total"], json!(1), "those he follows are listed");
    let theirs = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(
        (&theirs.body["mine"], &theirs.body["watchers"], &theirs.body["requested_by"]["name"]),
        (&json!(false), &json!(1), &json!("ann"))
    );

    // Declined: news to her and to him; why, and who said so, are hers.
    let declined = w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": "Not now"})).await;
    assert_eq!(
        (&declined.body["stage"], &declined.body["reason"], &declined.body["unseen"]),
        (&json!("declined"), &json!("Not now"), &json!(false)),
        "{}",
        declined.text()
    );
    let hers = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(
        (&hers.body["reason"], &hers.body["decided_by"]["name"], &hers.body["unseen"]),
        (&json!("Not now"), &json!("admin"), &json!(true))
    );
    let his = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!(
        (&his.body["stage"], &his.body["reason"], &his.body["decided_by"], &his.body["unseen"]),
        (&json!("declined"), &Value::Null, &Value::Null, &json!(true))
    );
    assert_eq!(w.app.post("/api/requests/seen", &bob, json!({})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &bob).await.body["unseen"], json!(false));
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &w.ann).await.body["unseen"], json!(true), "hers to see");

    // Sent to Radarr, which downloads it: where it went is for managers.
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    let matrix = asked.body["id"].as_i64().unwrap();
    let approved = w.app.post(&format!("/api/requests/{matrix}/approve"), &w.admin, json!({})).await;
    assert_eq!((&approved.body["stage"], &approved.body["sent_to"]), (&json!("approved"), &Value::Null), "sending");
    w.app.state.requests.settled().await;
    w.app.state.requests.refresh_queues().await;
    let sent = w.app.get(&format!("/api/requests/{matrix}"), &w.admin).await;
    assert_eq!(
        (&sent.body["stage"], &sent.body["sent_to"]["service"]),
        (&json!("downloading"), &json!("radarr")),
        "{}",
        sent.text()
    );
    let hers = w.app.get(&format!("/api/requests/{matrix}"), &w.ann).await;
    assert_eq!((&hers.body["stage"], &hers.body["sent_to"]), (&json!("downloading"), &Value::Null));

    // Failed: what went wrong is for managers.
    *w.services.refusals.lock() = 1;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await;
    let id = asked.body["id"].as_i64().unwrap();
    w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let failed = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(
        (&failed.body["stage"], &failed.body["reason"]),
        (&json!("failed"), &json!("Radarr refused: Root folder is not writable"))
    );
    let hers = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!((&hers.body["stage"], &hers.body["reason"]), (&json!("failed"), &Value::Null));

    // Without Radarr: approved, not sent.
    let reply = w.app.put("/api/settings", &w.admin, json!({"radarr_url": ""})).await;
    assert_eq!(reply.status, StatusCode::OK);
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 606})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!((&approved.body["stage"], &approved.body["sent_to"]), (&json!("approved"), &Value::Null));

    // There, then gone from the library.
    w.add(w.movies, &["movies/The Matrix (1999).mkv"]).await;
    w.identify("The Matrix", "tmdb_id = 603").await;
    w.app.state.requests.sync().await.unwrap();
    let hers = w.app.get(&format!("/api/requests/{matrix}"), &w.ann).await;
    assert_eq!((&hers.body["stage"], &hers.body["unseen"]), (&json!("available"), &json!(true)), "{}", hers.text());
    let item = hers.body["item_id"].as_i64().unwrap();
    sqlx::query("DELETE FROM items WHERE id = ?").bind(item).execute(&w.app.state.db.write).await.unwrap();
    let hers = w.app.get(&format!("/api/requests/{matrix}"), &w.ann).await;
    assert_eq!((&hers.body["stage"], &hers.body["item_id"]), (&json!("gone"), &Value::Null));

    // And back: linked again.
    w.add(w.movies, &["movies/The Matrix (1999) - 4K.mkv"]).await;
    w.identify("The Matrix", "tmdb_id = 603").await;
    w.app.state.requests.sync().await.unwrap();
    let hers = w.app.get(&format!("/api/requests/{matrix}"), &w.ann).await;
    assert_eq!(hers.body["stage"], json!("available"), "{}", hers.text());
    assert!(hers.body["item_id"].is_i64());
}

#[tokio::test]
async fn apps_ask_with_requests_and_manage_with_approvals() {
    let w = world().await;
    w.turn_on(json!({})).await;
    // Signed in before it could ask: told to sign in again, every number 0.
    let (old, _) = w.app.app_token(w.ann_id, "library play").await;
    let summary = w.app.get("/api/requests/summary", &old).await;
    assert_eq!(summary.status, StatusCode::OK, "any app");
    assert_eq!((&summary.body["access"], &summary.body["sign_in_again"]), (&json!("ask"), &json!("requests")));
    assert_eq!((&summary.body["movies_left"], &summary.body["manage"]), (&json!(0), &json!(false)));
    let refused = w.app.get("/api/discover/trending", &old).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    // pley's own apps ask with `requests`; `approvals` does nothing for her.
    let (anns, _) = w.app.app_token(w.ann_id, "library requests approvals").await;
    let summary = w.app.get("/api/requests/summary", &anns).await;
    assert_eq!((&summary.body["sign_in_again"], &summary.body["manage"]), (&Value::Null, &json!(false)));
    let asked = w.ask(&anns, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(status(&asked), (StatusCode::CREATED, "pending".to_owned()), "{}", asked.text());
    let id = asked.body["id"].as_i64().unwrap();
    let refused = w.app.post(&format!("/api/requests/{id}/approve"), &anns, json!({})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("forbidden")));

    // An administrator's: approved as it asks, managing with `approvals`.
    let (admins, _) = w.app.app_token(1, "library requests").await;
    let summary = w.app.get("/api/requests/summary", &admins).await;
    assert_eq!(
        (&summary.body["access"], &summary.body["manage"], &summary.body["sign_in_again"], &summary.body["pending"]),
        (&json!("auto"), &json!(false), &json!("approvals"), &json!(0))
    );
    let refused = w.app.post(&format!("/api/requests/{id}/approve"), &admins, json!({})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &admins).await.status, StatusCode::NOT_FOUND, "hers");
    let (manager, _) = w.app.app_token(1, "library requests approvals").await;
    let summary = w.app.get("/api/requests/summary", &manager).await;
    assert_eq!(
        (&summary.body["manage"], &summary.body["pending"], &summary.body["sign_in_again"]),
        (&json!(true), &json!(1), &Value::Null)
    );
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &manager, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    let me = w.app.get("/api/auth/me", &manager).await;
    assert_eq!((&me.body["is_admin"], &me.body["request_access"]), (&json!(false), &json!("auto")));

    // Other publishers' apps can't be allowed `approvals`, and an
    // administrator's requests from them wait for a manager.
    let other = w
        .app
        .state
        .auth
        .issue(pley::auth::grants::NewGrant {
            user_id: 1,
            approved_by: 1,
            client_id: "https://player.example/client.json".into(),
            client_name: Some("Player".into()),
            scopes: pley::auth::scopes::Scopes::parse("library requests").unwrap(),
            refresh: false,
            first_party: false,
            session_hash: None,
            user_agent: None,
            ip: None,
        })
        .await
        .unwrap()
        .access_token;
    let summary = w.app.get("/api/requests/summary", &other).await;
    assert_eq!((&summary.body["access"], &summary.body["sign_in_again"]), (&json!("ask"), &Value::Null));
    assert_eq!(w.app.get("/api/auth/me", &other).await.body["request_access"], json!("ask"));
    let asked = w.ask(&other, json!({"kind": "movie", "tmdb_id": 605})).await;
    assert_eq!(status(&asked), (StatusCode::CREATED, "pending".to_owned()), "{}", asked.text());

    // Apps load the pictures of the people named in the requests they may
    // read, beside their household's.
    let (_, dee) = w.app.user(&w.admin, "dee", json!({"home": false})).await;
    let avatars = w.app.state.config.data_dir.join("avatars");
    std::fs::create_dir_all(&avatars).unwrap();
    for id in [1, dee] {
        std::fs::write(avatars.join(format!("{id}.jpg")), JPEG).unwrap();
        sqlx::query("UPDATE users SET avatar = ? WHERE id = ?")
            .bind(format!("{id}.jpg"))
            .bind(id)
            .execute(&w.app.state.db.write)
            .await
            .unwrap();
    }
    let picture = |token: String, id: i64| {
        let app = &w.app;
        async move { app.get(&format!("/api/users/{id}/avatar"), &token).await.status }
    };
    let (dees, _) = w.app.app_token(dee, "library requests").await;
    assert_eq!(picture(dees.clone(), 1).await, StatusCode::NOT_FOUND);
    assert_eq!(picture(manager.clone(), dee).await, StatusCode::NOT_FOUND);
    let asked = w.ask(&dees, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    assert_eq!(picture(manager.clone(), dee).await, StatusCode::OK, "who asked");
    assert_eq!(picture(dees.clone(), 1).await, StatusCode::NOT_FOUND, "nobody decided yet");
    w.app.post(&format!("/api/requests/{id}/decline"), &manager, json!({})).await;
    assert_eq!(picture(dees.clone(), 1).await, StatusCode::OK, "who decided");
    let (library, _) = w.app.app_token(dee, "library").await;
    assert_eq!(picture(library, 1).await, StatusCode::NOT_FOUND, "an app that can't read requests");
}

/// `/api/events` as `token`'s account hears it.
async fn events(app: &TestApp, token: &str) -> BodyDataStream {
    events_in(app, token, None).await
}

/// The same, on a device that asks for `accept_language`.
async fn events_in(app: &TestApp, token: &str, accept_language: Option<&str>) -> BodyDataStream {
    let mut request =
        axum::http::Request::builder().uri("/api/events").header("authorization", format!("Bearer {token}"));
    if let Some(language) = accept_language {
        request = request.header("accept-language", language);
    }
    let request = request.body(axum::body::Body::empty()).unwrap();
    let response = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().into_data_stream()
}

/// What a stream says up to `libraries_changed`.
async fn heard(stream: &mut BodyDataStream) -> String {
    let mut received = String::new();
    let read = async {
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if received.contains("libraries_changed") {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read).await.expect("the events");
    received
}

#[tokio::test]
async fn administrators_streams_hear_of_everyones_requests() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    let mut admin = events(&w.app, &w.admin).await;
    let mut bob = events(&w.app, &bob).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!(asked.status, StatusCode::CREATED);
    let id = asked.body["id"].as_i64().unwrap();
    w.app.state.events.send(ServerEvent::LibrariesChanged);
    let admin = heard(&mut admin).await;
    let changed = format!(r#"{{"type":"requests_changed","request_id":{id},"kind":"movie","tmdb_id":603}}"#);
    assert!(admin.contains(&changed), "{admin}");
    let created = format!(
        r#"{{"type":"request_created","request_id":{id},"title":"The Matrix","kind":"movie","tmdb_id":603,"seasons":null,"requested_by":{{"id":{},"name":"ann","avatar_url":null,"avatar_color":null}},"status":"pending"}}"#,
        w.ann_id
    );
    assert!(admin.contains(&created), "{admin}");
    let bob = heard(&mut bob).await;
    assert!(!bob.contains("request"), "{bob}");
}

#[tokio::test]
async fn the_settings_ask_radarr_and_sonarr_what_they_offer() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let options = w.app.post("/api/settings/arr", &w.admin, json!({"service": "radarr"})).await;
    assert_eq!(options.status, StatusCode::OK, "{}", options.text());
    assert_eq!(
        options.body,
        json!({
            "version": "5.14.0.9383",
            "quality_profiles": [{"id": 4, "name": "HD-1080p"}, {"id": 5, "name": "Ultra-HD"}],
            "root_folders": [{"path": "/movies", "free_bytes": 5_000_000_000_i64}],
            "language_profiles": []
        })
    );
    let sonarr = w
        .app
        .post(
            "/api/settings/arr",
            &w.admin,
            json!({"service": "sonarr", "url": format!("{}/sonarr/", w.base), "api_key": SONARR_KEY}),
        )
        .await;
    assert_eq!(sonarr.status, StatusCode::OK, "{}", sonarr.text());
    assert_eq!(
        sonarr.body["root_folders"],
        json!([{"path": "/tv", "free_bytes": null}, {"path": "/anime", "free_bytes": null}])
    );

    // The saved key goes to the saved address only.
    let elsewhere =
        w.app.post("/api/settings/arr", &w.admin, json!({"service": "radarr", "url": "http://127.0.0.2:9"})).await;
    assert_eq!(elsewhere.status, StatusCode::BAD_REQUEST);
    assert!(elsewhere.body["message"].as_str().unwrap().contains("saved address only"));
    let moved = w.app.put("/api/settings", &w.admin, json!({"radarr_url": "http://127.0.0.2:9"})).await;
    assert_eq!(moved.status, StatusCode::BAD_REQUEST, "{}", moved.text());
    let wrong = w.app.post("/api/settings/arr", &w.admin, json!({"service": "radarr", "api_key": "wrong"})).await;
    assert_eq!(
        (wrong.status, &wrong.body["message"]),
        (StatusCode::BAD_REQUEST, &json!("Radarr rejected the API key"))
    );
    let away = w
        .app
        .post("/api/settings/arr", &w.admin, json!({"service": "radarr", "url": "http://127.0.0.1:9", "api_key": "k"}))
        .await;
    assert_eq!(away.status, StatusCode::BAD_REQUEST);
    assert!(away.body["message"].as_str().unwrap().starts_with("Radarr couldn't be reached"), "{}", away.text());
    let bad = w.app.post("/api/settings/arr", &w.admin, json!({"service": "radarr", "url": "ftp://box"})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        w.app.post("/api/settings/arr", &w.ann, json!({"service": "radarr"})).await.status,
        StatusCode::FORBIDDEN
    );
    // Each key went to its own service only.
    assert!(w.services.radarr.lock().iter().all(|c| c.key.as_deref() != Some(SONARR_KEY)));
    assert!(w.services.sonarr.lock().iter().all(|c| c.key.as_deref() != Some(RADARR_KEY)));
    assert!(w.services.tmdb.lock().iter().all(|r| !r.contains(RADARR_KEY) && !r.contains(SONARR_KEY)));
}

#[tokio::test]
async fn asking_again_follows_the_requests_asking_already() {
    let w = world().await;
    w.turn_on(json!({"request_limit_seasons": 2})).await;
    let (bob, bob_id) = w.app.user(&w.admin, "bob", json!({})).await;
    let hers = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 9001, "seasons": [1]})).await;
    assert_eq!(hers.status, StatusCode::CREATED, "{}", hers.text());
    let id = hers.body["id"].as_i64().unwrap();

    // Tell me too: Bob follows hers, which costs him nothing.
    let too = w.ask(&bob, json!({"kind": "show", "tmdb_id": 9001, "seasons": [1]})).await;
    assert_eq!(too.status, StatusCode::OK, "{}", too.text());
    assert_eq!(
        (&too.body["id"], &too.body["mine"], &too.body["watching"], &too.body["requested_by"]),
        (&json!(id), &json!(false), &json!(true), &Value::Null)
    );
    let summary = w.app.get("/api/requests/summary", &bob).await;
    assert_eq!((&summary.body["seasons_left"], &summary.body["own"]), (&json!(2), &json!(1)));
    let again = w.ask(&bob, json!({"kind": "show", "tmdb_id": 9001, "seasons": [1]})).await;
    assert_eq!((again.status, &again.body["id"]), (StatusCode::OK, &json!(id)), "nothing changes");
    let own = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 9001, "seasons": [1]})).await;
    assert_eq!((own.status, &own.body["id"], &own.body["mine"]), (StatusCode::OK, &json!(id), &json!(true)));
    let theirs = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!(theirs.body["watchers"], json!(1), "her own changes nothing");

    // Every season he lacks: he follows hers for the first, and asks for
    // the other two.
    let more = w.ask(&bob, json!({"kind": "show", "tmdb_id": 9001})).await;
    assert_eq!(more.status, StatusCode::CREATED, "{}", more.text());
    assert_eq!((&more.body["seasons"], &more.body["mine"]), (&json!([2, 3]), &json!(true)));
    let his = more.body["id"].as_i64().unwrap();
    let detail = w.app.get("/api/discover/show/9001", &bob).await;
    assert_eq!((&detail.body["status"], &detail.body["watching"]), (&json!("requested"), &json!(true)));
    let requests: Vec<(i64, bool, bool)> = detail.body["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].as_i64().unwrap(), r["mine"].as_bool().unwrap(), r["watching"].as_bool().unwrap()))
        .collect();
    assert_eq!(requests, [(his, true, false), (id, false, true)]);

    // He stops following hers.
    let stopped = w.app.delete(&format!("/api/requests/{id}/watch"), &bob).await;
    assert_eq!(stopped.status, StatusCode::NO_CONTENT, "{}", stopped.text());
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &bob).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.app.delete(&format!("/api/requests/{id}/watch"), &bob).await.status, StatusCode::NOT_FOUND);
    let own = w.app.delete(&format!("/api/requests/{id}/watch"), &w.ann).await;
    assert_eq!((own.status, &own.body["request"]["mine"]), (StatusCode::CONFLICT, &json!(true)), "hers to withdraw");
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &w.admin).await.body["watchers"], json!(0));

    // Cid is approved as he asks: following her pending request approves
    // it, as if he had asked for it, and Sonarr gets it.
    let (cid, _) = w.app.user(&w.admin, "cid", json!({"request_access": "auto"})).await;
    let mut events = w.app.state.events.subscribe();
    let approved = w.ask(&cid, json!({"kind": "show", "tmdb_id": 9001, "seasons": [1]})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    assert_eq!(
        (&approved.body["id"], &approved.body["watching"], &approved.body["stage"]),
        (&json!(id), &json!(true), &json!("approved"))
    );
    w.app.state.requests.settled().await;
    assert_eq!(w.services.sonarr_calls("POST", "/series").len(), 1);
    let hers = w.app.get(&format!("/api/requests/{id}"), &w.ann).await;
    assert_eq!(hers.body["decided_by"]["name"], json!("cid"));
    let heard = said(&mut events);
    assert!(
        heard.iter().any(|e| matches!(e,
            ServerEvent::RequestDecided { to, request_id, status: RequestStatus::Approved, reason: None, .. }
                if to.users == [w.ann_id] && *request_id == id)),
        "{heard:?}"
    );
    assert!(!heard.iter().any(|e| matches!(e, ServerEvent::RequestCreated { .. })), "nothing new");

    // Declined: for those who follow it too, without the reason, and none
    // of them may ask for those seasons for 30 days.
    let (dee, dee_id) = w.app.user(&w.admin, "dee", json!({})).await;
    let follows = w.ask(&dee, json!({"kind": "show", "tmdb_id": 9001, "seasons": [2]})).await;
    assert_eq!((follows.status, &follows.body["id"]), (StatusCode::OK, &json!(his)));
    let mut events = w.app.state.events.subscribe();
    let declined = w.app.post(&format!("/api/requests/{his}/decline"), &w.admin, json!({"reason": "Too long"})).await;
    assert_eq!(status(&declined), (StatusCode::OK, "declined".to_owned()));
    let decided: Vec<(Vec<i64>, Option<String>)> = said(&mut events)
        .into_iter()
        .filter_map(|e| match e {
            ServerEvent::RequestDecided { to, reason, status: RequestStatus::Declined, .. } => Some((to.users, reason)),
            _ => None,
        })
        .collect();
    assert_eq!(decided, [(vec![bob_id], Some("Too long".to_owned())), (vec![dee_id], None)]);
    let at = declined.body["decided_at"].as_i64().unwrap();
    for token in [&bob, &dee] {
        let refused = w.ask(token, json!({"kind": "show", "tmdb_id": 9001, "seasons": [2]})).await;
        assert_eq!(
            (refused.status, &refused.body["code"], &refused.body["retry_at"]),
            (StatusCode::CONFLICT, &json!("declined"), &json!(at + 30 * 86_400)),
            "{}",
            refused.text()
        );
        assert!(refused.body["message"].as_str().unwrap().starts_with("season 2 was declined for this account"));
    }
    let detail = w.app.get("/api/discover/show/9001", &dee).await;
    assert_eq!(detail.body["status"], json!("declined"), "what is left to ask for was");
    let both = w.ask(&dee, json!({"kind": "show", "tmdb_id": 9001, "seasons": [2, 3]})).await;
    assert!(both.body["message"].as_str().unwrap().starts_with("seasons 2 and 3 were declined"), "{}", both.text());
    // Without seasons named: those not declined, season 1, which she
    // follows then.
    let rest = w.ask(&dee, json!({"kind": "show", "tmdb_id": 9001})).await;
    assert_eq!((rest.status, &rest.body["id"], &rest.body["watching"]), (StatusCode::OK, &json!(id), &json!(true)));
    // She keeps following the declined one for its 30 days: no longer a
    // follower, she could ask for season 2 again at once.
    let kept = w.app.delete(&format!("/api/requests/{his}/watch"), &dee).await;
    assert_eq!(
        (kept.status, &kept.body["code"], &kept.body["retry_at"]),
        (StatusCode::CONFLICT, &json!("conflict"), &json!(at + 30 * 86_400)),
        "{}",
        kept.text()
    );
    assert_eq!((&kept.body["request"]["watching"], &kept.body["request"]["actions"]), (&json!(true), &json!([])));
    sqlx::query("UPDATE requests SET decided_at = decided_at - 31 * 86400 WHERE id = ?")
        .bind(his)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    assert_eq!(w.app.get(&format!("/api/requests/{his}"), &dee).await.body["actions"], json!(["unwatch"]));
    assert_eq!(w.app.delete(&format!("/api/requests/{his}/watch"), &dee).await.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn asking_follows_only_requests_still_asking() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    for (tmdb_id, change) in [
        (604, "UPDATE requests SET status = 'declined', decided_by = 1, decided_at = updated_at WHERE id = ?"),
        (605, "DELETE FROM requests WHERE id = ?"),
    ] {
        let hers = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": tmdb_id})).await;
        let id = hers.body["id"].as_i64().unwrap();
        // Hers changes while Bob asks, once he has looked at the title: the
        // write waits for it.
        let mut tx = w.app.state.db.write.begin().await.unwrap();
        let (his, ()) = tokio::join!(w.ask(&bob, json!({"kind": "movie", "tmdb_id": tmdb_id})), async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            sqlx::query(change).bind(id).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        });
        assert_eq!(status(&his), (StatusCode::CREATED, "pending".to_owned()), "{change}: {}", his.text());
        assert_eq!((&his.body["mine"], &his.body["watching"]), (&json!(true), &json!(false)), "{change}");
        let followed: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM request_watchers").fetch_one(&w.app.state.db.read).await.unwrap();
        assert_eq!(followed, 0, "{change}");
    }
}

#[tokio::test]
async fn a_withdrawn_request_goes_to_its_earliest_follower() {
    let w = world().await;
    w.turn_on(json!({"request_limit_movies": 1})).await;
    let (bob, bob_id) = w.app.user(&w.admin, "bob", json!({})).await;
    let (cid, cid_id) = w.app.user(&w.admin, "cid", json!({})).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let id = asked.body["id"].as_i64().unwrap();
    for token in [&bob, &cid] {
        let follows = w.ask(token, json!({"kind": "movie", "tmdb_id": 604})).await;
        assert_eq!((follows.status, &follows.body["id"]), (StatusCode::OK, &json!(id)), "{}", follows.text());
    }
    // Cid followed it first.
    sqlx::query("UPDATE request_watchers SET created_at = created_at - 60 WHERE user_id = ?")
        .bind(cid_id)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();

    // Ann withdraws hers: it is Cid's now, and Bob still follows it.
    let mut events = w.app.state.events.subscribe();
    assert_eq!(w.app.delete(&format!("/api/requests/{id}"), &w.ann).await.status, StatusCode::NO_CONTENT);
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &w.ann).await.status, StatusCode::NOT_FOUND);
    let his = w.app.get(&format!("/api/requests/{id}"), &cid).await;
    assert_eq!(
        (&his.body["mine"], &his.body["watching"], &his.body["requested_by"]["name"], &his.body["actions"]),
        (&json!(true), &json!(false), &json!("cid"), &json!(["withdraw"])),
        "{}",
        his.text()
    );
    let bobs = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!((&bobs.body["mine"], &bobs.body["watching"]), (&json!(false), &json!(true)));
    let theirs = w.app.get(&format!("/api/requests/{id}"), &w.admin).await;
    assert_eq!((&theirs.body["requested_by"]["name"], &theirs.body["watchers"]), (&json!("cid"), &json!(1)));
    // His now, it counts toward his limit as his asking would have: asking
    // for her, following and having her withdraw isn't a way around it.
    let summary = w.app.get("/api/requests/summary", &cid).await;
    assert_eq!((&summary.body["movies_left"], &summary.body["own"]), (&json!(0), &json!(1)), "{}", summary.text());
    let more = w.ask(&cid, json!({"kind": "movie", "tmdb_id": 606})).await;
    assert_eq!(more.status, StatusCode::TOO_MANY_REQUESTS, "{}", more.text());
    assert_eq!(w.app.get("/api/requests/summary", &w.ann).await.body["movies_left"], json!(1), "no longer hers");
    let heard = said(&mut events);
    assert!(
        heard.iter().any(|e| matches!(e,
            ServerEvent::RequestsChanged { to, request_id: Some(r), .. }
                if *r == id && to.managers && [w.ann_id, bob_id, cid_id].iter().all(|u| to.users.contains(u)))),
        "{heard:?}"
    );

    // Cid's account is deleted: Bob's now. Then Bob's: nobody follows it,
    // so it goes with his account.
    let mut events = w.app.state.events.subscribe();
    let gone = w.app.delete(&format!("/api/users/{cid_id}"), &w.admin).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.text());
    let his = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!((&his.body["mine"], &his.body["watching"]), (&json!(true), &json!(false)), "{}", his.text());
    let heard = said(&mut events);
    assert!(
        heard.iter().any(|e| matches!(e,
            ServerEvent::RequestsChanged { to, request_id: None, .. } if to.users == [bob_id] && to.managers)),
        "{heard:?}"
    );
    assert_eq!(w.app.delete(&format!("/api/users/{bob_id}"), &w.admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(w.app.get(&format!("/api/requests/{id}"), &w.admin).await.status, StatusCode::NOT_FOUND);

    // Only its maker withdraws one; a manager's delete is for everyone.
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await;
    let id = asked.body["id"].as_i64().unwrap();
    let (dee, dee_id) = w.app.user(&w.admin, "dee", json!({})).await;
    w.ask(&dee, json!({"kind": "movie", "tmdb_id": 605})).await;
    let refused = w.app.delete(&format!("/api/requests/{id}"), &dee).await;
    assert_eq!((refused.status, &refused.body["request"]["watching"]), (StatusCode::CONFLICT, &json!(true)));
    let mut events = w.app.state.events.subscribe();
    assert_eq!(w.app.delete(&format!("/api/requests/{id}"), &w.admin).await.status, StatusCode::NO_CONTENT);
    for token in [&w.ann, &dee] {
        assert_eq!(w.app.get(&format!("/api/requests/{id}"), token).await.status, StatusCode::NOT_FOUND);
    }
    let heard = said(&mut events);
    assert!(
        heard.iter().any(|e| matches!(e,
            ServerEvent::RequestsChanged { to, request_id: Some(r), .. } if *r == id && to.users.contains(&dee_id))),
        "{heard:?}"
    );

    // Declined while Fay followed it, then hers as Eve's account goes: why,
    // and who said so, are still not hers to know.
    let (eve, eve_id) = w.app.user(&w.admin, "eve", json!({})).await;
    let (fay, _) = w.app.user(&w.admin, "fay", json!({})).await;
    let asked = w.ask(&eve, json!({"kind": "movie", "tmdb_id": 606})).await;
    let id = asked.body["id"].as_i64().unwrap();
    assert_eq!(w.ask(&fay, json!({"kind": "movie", "tmdb_id": 606})).await.status, StatusCode::OK);
    w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": "Not this one"})).await;
    assert_eq!(w.app.delete(&format!("/api/users/{eve_id}"), &w.admin).await.status, StatusCode::NO_CONTENT);
    let hers = w.app.get(&format!("/api/requests/{id}"), &fay).await;
    assert_eq!(
        (&hers.body["mine"], &hers.body["status"], &hers.body["reason"], &hers.body["decided_by"]),
        (&json!(true), &json!("declined"), &Value::Null, &Value::Null),
        "{}",
        hers.text()
    );
    let detail = w.app.get("/api/discover/movie/606", &fay).await;
    assert_eq!(
        (&detail.body["status"], &detail.body["declined"]["reason"]),
        (&json!("declined"), &Value::Null),
        "{}",
        detail.text()
    );
    // Decided again once hers: hers to know.
    sqlx::query("UPDATE requests SET inherited_at = inherited_at - 60 WHERE id = ?")
        .bind(id)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    w.app.post(&format!("/api/requests/{id}/handled"), &w.admin, json!({})).await;
    w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": "Still no"})).await;
    let hers = w.app.get(&format!("/api/requests/{id}"), &fay).await;
    assert_eq!((&hers.body["reason"], &hers.body["decided_by"]["name"]), (&json!("Still no"), &json!("admin")));
}

#[tokio::test]
async fn approving_keeps_the_seasons_chosen_and_added_by_hand_sends_nothing() {
    let w = world().await;
    w.turn_on(json!({"request_limit_seasons": 3})).await;
    let asked = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 9001})).await;
    assert_eq!(asked.body["seasons"], json!([1, 2, 3]), "{}", asked.text());
    let id = asked.body["id"].as_i64().unwrap();
    for (body, why) in [
        (json!({"seasons": []}), "none kept"),
        (json!({"seasons": [4]}), "one not asked for"),
        (json!({"seasons": "all"}), "not seasons"),
        (json!({"quality_profile": 0}), "no profile"),
        (json!({"root_folder": " "}), "no folder"),
    ] {
        let refused = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, body).await;
        assert_eq!(
            (refused.status, &refused.body["code"]),
            (StatusCode::BAD_REQUEST, &json!("bad_request")),
            "{why}: {}",
            refused.text()
        );
    }
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({"seasons": [3, 1]})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()), "{}", approved.text());
    assert_eq!((&approved.body["seasons"], &approved.body["stage"]), (&json!([1, 3]), &json!("approved")));
    // Season 2 left: it no longer counts against her.
    assert_eq!(w.app.get("/api/requests/summary", &w.ann).await.body["seasons_left"], json!(1));
    w.app.state.requests.settled().await;
    let series = &w.services.sonarr_calls("POST", "/series")[0].body;
    let monitored: Vec<bool> =
        series["seasons"].as_array().unwrap().iter().map(|s| s["monitored"].as_bool().unwrap()).collect();
    assert_eq!(monitored, [false, true, false, true]);
    let again = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({"seasons": [1]})).await;
    assert_eq!((again.status, &again.body["seasons"]), (StatusCode::OK, &json!([1, 3])), "sent: as it is");

    // Declined once sent, then approved again: sent again.
    let declined = w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": "Later"})).await;
    assert_eq!(status(&declined), (StatusCode::OK, "declined".to_owned()), "{}", declined.text());
    assert_eq!(declined.body["sent_to"]["service"], json!("sonarr"), "it went there");
    let approved = w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()));
    assert_eq!(approved.body["reason"], Value::Null);
    w.app.state.requests.settled().await;
    assert_eq!(w.services.sonarr_calls("PUT", "/series/21").len(), 1, "there already: monitored again");

    // Added by hand: approved, never sent.
    let movie = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let movie_id = movie.body["id"].as_i64().unwrap();
    assert_eq!(
        w.app.post(&format!("/api/requests/{movie_id}/handled"), &w.ann, json!({})).await.status,
        StatusCode::FORBIDDEN
    );
    let handled = w.app.post(&format!("/api/requests/{movie_id}/handled"), &w.admin, json!({})).await;
    assert_eq!(status(&handled), (StatusCode::OK, "approved".to_owned()), "{}", handled.text());
    assert_eq!(
        (&handled.body["stage"], &handled.body["sent_to"], &handled.body["note"], &handled.body["actions"]),
        (&json!("approved"), &Value::Null, &json!("Added by hand"), &json!(["approve", "decline", "delete"]))
    );
    let hers = w.app.get(&format!("/api/requests/{movie_id}"), &w.ann).await;
    assert_eq!((&hers.body["note"], &hers.body["decided_by"]["name"]), (&Value::Null, &json!("admin")));
    let again = w.app.post(&format!("/api/requests/{movie_id}/handled"), &w.admin, json!({})).await;
    assert_eq!(status(&again), (StatusCode::OK, "approved".to_owned()));
    w.app.state.requests.settled().await;
    assert!(w.services.radarr_calls("POST", "/movie").is_empty(), "never sent");
    let refused = w.app.post(&format!("/api/requests/{id}/handled"), &w.admin, json!({})).await;
    assert_eq!((refused.status, &refused.body["request"]["id"]), (StatusCode::CONFLICT, &json!(id)), "sent");
    // Approving it sends it after all.
    let approved = w.app.post(&format!("/api/requests/{movie_id}/approve"), &w.admin, json!({})).await;
    assert_eq!(status(&approved), (StatusCode::OK, "approved".to_owned()));
    w.app.state.requests.settled().await;
    assert_eq!(w.services.radarr_calls("POST", "/movie").len(), 1);
    assert_eq!(w.app.post("/api/requests/999/handled", &w.admin, json!({})).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_options_say_what_approving_may_choose() {
    let w = world().await;
    w.turn_on(json!({"radarr_anime_quality_profile": 5})).await;
    let options = w.app.get("/api/requests/options", &w.admin).await;
    assert_eq!(options.status, StatusCode::OK, "{}", options.text());
    assert_eq!(
        options.body,
        json!({
            "radarr": {
                "quality_profiles": [{"id": 4, "name": "HD-1080p"}, {"id": 5, "name": "Ultra-HD"}],
                "root_folders": [{"path": "/movies", "free_bytes": 5_000_000_000_i64}],
                "quality_profile": 4, "root_folder": "/movies",
                "anime_quality_profile": 5, "anime_root_folder": null, "error": null
            },
            "sonarr": {
                "quality_profiles": [{"id": 6, "name": "HD-720p"}],
                "root_folders": [{"path": "/tv", "free_bytes": null}, {"path": "/anime", "free_bytes": null}],
                "quality_profile": 6, "root_folder": "/tv",
                "anime_quality_profile": null, "anime_root_folder": "/anime", "error": null
            }
        })
    );
    // Kept a while: Radarr isn't asked again.
    assert_eq!(w.app.get("/api/requests/options", &w.admin).await.status, StatusCode::OK);
    assert_eq!(w.services.radarr_calls("GET", "/qualityprofile").len(), 1);
    // Managers only.
    let refused = w.app.get("/api/requests/options", &w.ann).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("forbidden")));
    let (app, _) = w.app.app_token(1, "library requests").await;
    let refused = w.app.get("/api/requests/options", &app).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    let (manager, _) = w.app.app_token(1, "library requests approvals").await;
    assert_eq!(w.app.get("/api/requests/options", &manager).await.status, StatusCode::OK);

    // Sonarr moved where nothing answers: said so, with nothing to choose
    // from; Radarr without a root folder isn't connected.
    let moved = w
        .app
        .put(
            "/api/settings",
            &w.admin,
            json!({"sonarr_url": "http://127.0.0.1:9", "sonarr_api_key": "k", "radarr_root_folder": ""}),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let options = w.app.get("/api/requests/options", &w.admin).await;
    assert_eq!(options.status, StatusCode::OK, "always");
    assert_eq!(options.body["radarr"], Value::Null);
    let sonarr = &options.body["sonarr"];
    assert_eq!(
        (&sonarr["error"], &sonarr["quality_profiles"], &sonarr["root_folders"], &sonarr["quality_profile"]),
        (&json!("Sonarr isn't answering"), &json!([]), &json!([]), &json!(6))
    );
}

#[tokio::test]
async fn the_list_filters_counts_and_names_the_people() {
    let w = world().await;
    w.turn_on(json!({})).await;
    let (bob, bob_id) = w.app.user(&w.admin, "bob", json!({"display_name": "Bob"})).await;
    let mut ids = Vec::new();
    for (token, kind, tmdb_id) in [
        (&w.ann, "movie", 603),
        (&w.ann, "movie", 604),
        (&w.ann, "show", 1396),
        (&bob, "movie", 605),
        (&bob, "movie", 606),
    ] {
        let asked = w.ask(token, json!({"kind": kind, "tmdb_id": tmdb_id})).await;
        assert_eq!(asked.status, StatusCode::CREATED, "{}", asked.text());
        ids.push(asked.body["id"].as_i64().unwrap());
    }
    let [matrix, reloaded, breaking, revolutions, robots] = ids[..] else { panic!() };
    w.app.post(&format!("/api/requests/{reloaded}/decline"), &w.admin, json!({})).await;
    *w.services.refusals.lock() = 1;
    w.app.post(&format!("/api/requests/{revolutions}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let listed = |reply: &common::Reply| -> Vec<i64> {
        reply.body["requests"].as_array().unwrap().iter().map(|r| r["id"].as_i64().unwrap()).collect()
    };
    let counts = |reply: &common::Reply| -> Vec<i64> {
        ["pending", "approved", "declined", "failed", "available"]
            .iter()
            .map(|s| reply.body["counts"][s].as_i64().unwrap())
            .collect()
    };
    let people = |reply: &common::Reply| -> Vec<String> {
        reply.body["people"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap().to_owned()).collect()
    };

    let some = w.app.get("/api/requests?status=pending,failed", &w.admin).await;
    assert_eq!(some.status, StatusCode::OK, "{}", some.text());
    assert_eq!((listed(&some), &some.body["total"]), (vec![robots, revolutions, breaking, matrix], &json!(4)));
    assert_eq!(counts(&some), [3, 0, 1, 1, 0], "every status");
    assert_eq!(people(&some), ["ann", "Bob"], "by name");
    // Needs attention, oldest first.
    let attention = w.app.get("/api/requests?attention=1&order=oldest", &w.admin).await;
    assert_eq!(listed(&attention), [matrix, breaking, revolutions, robots]);
    assert_eq!(counts(&attention), [3, 0, 0, 1, 0]);
    let shows = w.app.get("/api/requests?kind=show", &w.admin).await;
    assert_eq!((listed(&shows), counts(&shows)), (vec![breaking], vec![1, 0, 0, 0, 0]));
    let his = w.app.get(&format!("/api/requests?user_id={bob_id}"), &w.admin).await;
    assert_eq!((listed(&his), people(&his)), (vec![robots, revolutions], vec!["ann".to_owned(), "Bob".to_owned()]));
    let found = w.app.get("/api/requests?q=MATRIX", &w.admin).await;
    assert_eq!(
        (listed(&found), people(&found)),
        (vec![revolutions, reloaded, matrix], vec!["ann".to_owned(), "Bob".to_owned()])
    );
    let found = w.app.get("/api/requests?q=matrix%20r&user_id=1", &w.admin).await;
    assert_eq!((listed(&found), people(&found)), (vec![], vec!["ann".to_owned(), "Bob".to_owned()]));
    let none = w.app.get("/api/requests?q=100%25", &w.admin).await;
    assert_eq!(listed(&none), Vec::<i64>::new(), "% is a character");
    let page = w.app.get("/api/requests?limit=2&offset=1", &w.admin).await;
    assert_eq!((listed(&page), &page.body["total"]), (vec![revolutions, breaking], &json!(5)));
    let own = w.app.get("/api/requests?mine=1", &w.admin).await;
    assert_eq!((listed(&own), people(&own)), (vec![], Vec::<String>::new()));
    // Changed lately.
    sqlx::query("UPDATE requests SET updated_at = CASE WHEN id = ? THEN 2000 ELSE 1000 END")
        .bind(breaking)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    assert_eq!(listed(&w.app.get("/api/requests?changed_since=1500", &w.admin).await), [breaking]);
    // News: hers, newest news first.
    let unseen = w.app.get("/api/requests?unseen=1", &w.ann).await;
    assert_eq!((listed(&unseen), &unseen.body["total"]), (vec![reloaded], &json!(1)), "{}", unseen.text());
    assert_eq!(listed(&w.app.get("/api/requests?unseen=1", &w.admin).await), Vec::<i64>::new(), "not his");
    // Others have their own and those they follow, and no people.
    let hers = w.app.get(&format!("/api/requests?user_id={bob_id}"), &w.ann).await;
    assert_eq!((listed(&hers), people(&hers)), (vec![breaking, reloaded, matrix], Vec::<String>::new()));
    for query in ["kind=film", "order=random", "status=pending,lost", "user_id=x", "changed_since=soon"] {
        let bad = w.app.get(&format!("/api/requests?{query}"), &w.admin).await;
        assert_eq!((bad.status, &bad.body["code"]), (StatusCode::BAD_REQUEST, &json!("bad_request")), "{query}");
    }
    let long = w.app.get(&format!("/api/requests?q={}", "x".repeat(201)), &w.admin).await;
    assert_eq!(long.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn request_events_go_to_whom_they_are_for() {
    let w = world().await;
    w.turn_on(json!({})).await;
    // Bob opens the shows only; Cid every library.
    let (bob, bob_id) = w.app.user(&w.admin, "bob", json!({"all_libraries": false, "library_ids": [w.shows]})).await;
    let (cid, cid_id) = w.app.user(&w.admin, "cid", json!({})).await;
    let mut bus = w.app.state.events.subscribe();
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    let id = asked.body["id"].as_i64().unwrap();
    for token in [&bob, &cid] {
        assert_eq!(w.ask(token, json!({"kind": "movie", "tmdb_id": 603})).await.status, StatusCode::OK);
    }
    let news = said(&mut bus);
    let created: Vec<&ServerEvent> = news.iter().filter(|e| matches!(e, ServerEvent::RequestCreated { .. })).collect();
    assert_eq!(created.len(), 1, "made once, followed twice: {news:?}");
    assert!(matches!(created[0],
        ServerEvent::RequestCreated { to, request_id, status: RequestStatus::Pending, requested_by, seasons: None, .. }
            if to.managers && to.users.is_empty() && to.except_user == Some(w.ann_id) && *request_id == id
                && requested_by.name == "ann"));

    // Declined: she hears why, they hear that it was.
    w.app.post(&format!("/api/requests/{id}/decline"), &w.admin, json!({"reason": "No"})).await;
    let decided: Vec<(Vec<i64>, RequestStatus, Option<String>)> = said(&mut bus)
        .into_iter()
        .filter_map(|e| match e {
            ServerEvent::RequestDecided { to, status, reason, .. } => Some((to.users, status, reason)),
            _ => None,
        })
        .collect();
    assert_eq!(
        decided,
        [
            (vec![w.ann_id], RequestStatus::Declined, Some("No".to_owned())),
            (vec![bob_id, cid_id], RequestStatus::Declined, None)
        ]
    );
    // Approved after all: she hears it, and it is sent.
    w.app.post(&format!("/api/requests/{id}/approve"), &w.admin, json!({})).await;
    w.app.state.requests.settled().await;
    let decided: Vec<(Vec<i64>, RequestStatus)> = said(&mut bus)
        .into_iter()
        .filter_map(|e| match e {
            ServerEvent::RequestDecided { to, status, .. } => Some((to.users, status)),
            _ => None,
        })
        .collect();
    assert_eq!(decided, [(vec![w.ann_id], RequestStatus::Approved)]);

    // Ready: each who can open it hears so, with the title; Bob, who
    // can't, hears only that it changed.
    w.add(w.movies, &["movies/The Matrix (1999).mkv"]).await;
    w.identify("The Matrix", "tmdb_id = 603").await;
    w.app.state.requests.sync().await.unwrap();
    let item = w.app.ids(w.movies, &w.admin).await["The Matrix"];
    let news = said(&mut bus);
    let ready: Vec<(Vec<i64>, Option<i64>)> = news
        .iter()
        .filter_map(|e| match e {
            ServerEvent::RequestAvailable { to, item_id, .. } => Some((to.users.clone(), *item_id)),
            _ => None,
        })
        .collect();
    assert_eq!(ready, [(vec![w.ann_id], Some(item)), (vec![cid_id], Some(item))]);
    assert!(news.iter().any(|e| matches!(e,
        ServerEvent::RequestsChanged { to, request_id: Some(r), .. } if *r == id && to.users.contains(&bob_id))));
    let his = w.app.get(&format!("/api/requests/{id}"), &bob).await;
    assert_eq!((&his.body["stage"], &his.body["item_id"]), (&json!("available"), &Value::Null), "not his to open");

    // Streams: an app's hears requests' events with `requests` only, and
    // managers' with `approvals` too; seeing the news is for the account's
    // other streams.
    let (library, _) = w.app.app_token(w.ann_id, "library").await;
    let (asking, _) = w.app.app_token(w.ann_id, "library requests").await;
    let (admins, _) = w.app.app_token(1, "library requests").await;
    let (manager, _) = w.app.app_token(1, "library requests approvals").await;
    let mut streams = Vec::new();
    for token in [&library, &asking, &admins, &manager, &w.ann] {
        streams.push(events(&w.app, token).await);
    }
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    let other = asked.body["id"].as_i64().unwrap();
    assert_eq!(w.app.post("/api/requests/seen", &w.ann, json!({})).await.status, StatusCode::NO_CONTENT);
    w.app.state.events.send(ServerEvent::LibrariesChanged);
    let mut told = Vec::new();
    for stream in &mut streams {
        told.push(heard(stream).await);
    }
    let changed = format!(r#""type":"requests_changed","request_id":{other},"#);
    let seen = r#"{"type":"requests_changed","request_id":null,"kind":null,"tmdb_id":null}"#;
    assert!(!told[0].contains("request"), "without `requests`: {}", told[0]);
    assert!(told[1].contains(&changed) && told[1].contains(seen), "hers: {}", told[1]);
    assert!(!told[1].contains("request_created"), "not a manager's: {}", told[1]);
    assert!(!told[2].contains("request"), "an administrator's app without `approvals`: {}", told[2]);
    assert!(told[3].contains("request_created") && told[3].contains(&changed), "a manager's: {}", told[3]);
    assert!(told[4].contains(&changed) && !told[4].contains(seen), "the session that saw it: {}", told[4]);
}

/// Waits till `stream` hears what is for managers (`manager`), or no longer
/// does: its checks follow what its account is now.
async fn hears_managers(w: &World, stream: &mut BodyDataStream, manager: bool) {
    for _ in 0..50 {
        w.app.state.events.send(ServerEvent::RequestsChanged {
            to: Audience::of(Vec::new()),
            request_id: None,
            kind: None,
            tmdb_id: None,
        });
        w.app.state.events.send(ServerEvent::LibrariesChanged);
        if heard(stream).await.contains(r#""type":"requests_changed""#) == manager {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the stream's checks didn't follow its account");
}

#[tokio::test]
async fn an_apps_stream_hears_managers_while_its_account_is_an_administrator() {
    let w = world_with(|c| c.event_recheck = Duration::from_millis(100)).await;
    w.turn_on(json!({})).await;
    let (_, bob_id) = w.app.user(&w.admin, "bob", json!({})).await;
    let (bobs, _) = w.app.app_token(bob_id, "library requests approvals").await;
    let mut stream = events(&w.app, &bobs).await;
    assert_eq!(w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await.status, StatusCode::CREATED);
    w.app.state.events.send(ServerEvent::LibrariesChanged);
    let told = heard(&mut stream).await;
    assert!(!told.contains("request"), "not a manager's: {told}");

    // Made an administrator: a manager's stream from its next check on.
    let made = w.app.patch(&format!("/api/users/{bob_id}"), &w.admin, json!({"is_admin": true})).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    heard(&mut stream).await;
    hears_managers(&w, &mut stream, true).await;
    let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 604})).await;
    w.app.state.events.send(ServerEvent::LibrariesChanged);
    let told = heard(&mut stream).await;
    let created = format!(r#""type":"request_created","request_id":{},"#, asked.body["id"]);
    assert!(told.contains(&created), "{told}");

    // No longer one: no longer a manager's.
    let made = w.app.patch(&format!("/api/users/{bob_id}"), &w.admin, json!({"is_admin": false})).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    heard(&mut stream).await;
    hears_managers(&w, &mut stream, false).await;
    assert_eq!(w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 605})).await.status, StatusCode::CREATED);
    w.app.state.events.send(ServerEvent::LibrariesChanged);
    let told = heard(&mut stream).await;
    assert!(!told.contains("request_created"), "{told}");
}

#[tokio::test]
async fn approving_from_several_sessions_at_once_answers_each_and_sends_once() {
    let w = world().await;
    w.turn_on(json!({})).await;
    for (tmdb_id, action) in [(603, "approve"), (604, "handled")] {
        let asked = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": tmdb_id})).await;
        let uri = format!("/api/requests/{}/{action}", asked.body["id"]);
        let answers = futures::future::join_all((0..8).map(|_| w.app.post(&uri, &w.admin, json!({})))).await;
        for answer in &answers {
            assert_eq!(status(answer), (StatusCode::OK, "approved".to_owned()), "{action}: {}", answer.text());
        }
    }
    w.app.state.requests.settled().await;
    assert_eq!(w.services.radarr_calls("POST", "/movie").len(), 1, "sent once; the one added by hand never");
}

#[tokio::test]
async fn labels_a_profile_never_sees_keep_titles_out_of_discover() {
    let w = world().await;
    w.turn_on(json!({})).await;
    w.add(w.movies, &["movies/The Matrix (1999).mkv"]).await;
    w.identify("The Matrix", "tmdb_id = 603").await;
    let matrix: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE tmdb_id = 603").fetch_one(&w.app.state.db.read).await.unwrap();
    let labelled = w.app.patch(&format!("/api/items/{matrix}"), &w.admin, json!({"labels": ["Not for Ann"]})).await;
    assert_eq!(labelled.status, StatusCode::OK, "{}", labelled.text());
    let rules =
        w.app.patch(&format!("/api/users/{}", w.ann_id), &w.admin, json!({"hide_labels": ["not for ann"]})).await;
    assert_eq!(rules.status, StatusCode::OK, "{}", rules.text());

    // Left out of the lists, restricted on its page, and not to be asked
    // for, without the label being said.
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(common::titles(&trending.body["results"]), ["Breaking Bad"], "{}", trending.text());
    let detail = w.app.get("/api/discover/movie/603", &w.ann).await;
    assert_eq!(
        (&detail.body["status"], &detail.body["restriction"]),
        (&json!("restricted"), &json!("Not available to this account")),
        "{}",
        detail.text()
    );
    let refused = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("restricted")));
    assert!(!refused.mentions("Not for Ann"), "{}", refused.text());

    // Others see it as before.
    let trending = w.app.get("/api/discover/trending", &w.admin).await;
    assert_eq!(trending.body["results"].as_array().unwrap().len(), 2);

    // A show another TV source named, without TMDB's id, is found by its
    // other ids.
    w.add(w.shows, &["shows/Breaking Bad (2008)/Season 01/Breaking Bad - S01E01.mkv"]).await;
    w.identify("Breaking Bad", "tvdb_id = 81189, tmdb_id = NULL").await;
    let show = w.app.ids(w.shows, &w.admin).await["Breaking Bad"];
    let labelled = w.app.patch(&format!("/api/items/{show}"), &w.admin, json!({"labels": ["Not for Ann"]})).await;
    assert_eq!(labelled.status, StatusCode::OK, "{}", labelled.text());
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(common::titles(&trending.body["results"]), Vec::<String>::new(), "{}", trending.text());
    let detail = w.app.get("/api/discover/show/1396", &w.ann).await;
    assert_eq!(detail.body["status"], json!("restricted"), "{}", detail.text());
    let refused = w.ask(&w.ann, json!({"kind": "show", "tmdb_id": 1396, "seasons": [2]})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("restricted")));

    // A library Plex's labels are still on their way to: its titles may
    // have the label, so they're kept out too until it's shown.
    let unlabelled = w.app.patch(&format!("/api/items/{matrix}"), &w.admin, json!({"labels": []})).await;
    assert_eq!(unlabelled.status, StatusCode::OK, "{}", unlabelled.text());
    sqlx::query("UPDATE libraries SET labels_pending = 1 WHERE id = ?")
        .bind(w.movies)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(common::titles(&trending.body["results"]), Vec::<String>::new(), "{}", trending.text());
    let refused = w.ask(&w.ann, json!({"kind": "movie", "tmdb_id": 603})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("restricted")));
    let shown = w.app.patch(&format!("/api/libraries/{}", w.movies), &w.admin, json!({"labels_pending": false})).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    let trending = w.app.get("/api/discover/trending", &w.ann).await;
    assert_eq!(common::titles(&trending.body["results"]), ["The Matrix"], "{}", trending.text());
}
