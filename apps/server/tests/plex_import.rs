//! The Plex import against a fake plex.tv and Plex Media Server: signing in,
//! the plan (libraries paired, Home users' limits), and what the import
//! brings over, twice.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use common::TestApp;
use serde_json::{Value, json};

/// The Plex account's token, and the Home user's after switching to them.
const OWNER: &str = "owner-plex-token";
const KID: &str = "kid-plex-token";
/// Their tokens for the server.
const OWNER_SERVER: &str = "owner-server-token";
const KID_SERVER: &str = "kid-server-token";
const MACHINE: &str = "machine-1";

fn token(headers: &HeaderMap) -> &str {
    headers.get("X-Plex-Token").and_then(|v| v.to_str().ok()).unwrap_or("")
}

fn container(body: Value) -> Response {
    axum::Json(json!({ "MediaContainer": body })).into_response()
}

fn file(path: &str, size: i64) -> Value {
    json!([{ "Part": [{ "file": path, "size": size }] }])
}

/// Every item of the fake server, with no one's watch state.
fn items(section: &str, kind: &str) -> Vec<Value> {
    match (section, kind) {
        ("1", "1") => vec![
            json!({"ratingKey": "101", "type": "movie", "title": "Alien", "addedAt": 1_000_000_000,
                   "thumb": "/library/metadata/101/thumb/1",
                   "Guid": [{"id": "imdb://tt0078748"}, {"id": "tmdb://348"}], "Media": file("/plexdata/movies/Alien (1979).mkv", 1024)}),
            json!({"ratingKey": "102", "type": "movie", "title": "Brazil", "addedAt": 1_100_000_000,
                   "year": 1985, "summary": "A clerk dreams.", "thumb": "http://elsewhere.invalid/brazil.png",
                   "Guid": [{"id": "tmdb://68"}], "Media": file("/plexdata/movies/Brazil (1985).mkv", 2048)}),
            json!({"ratingKey": "103", "type": "movie", "title": "Casablanca", "addedAt": 1_200_000_000,
                   "Guid": [{"id": "tmdb://289"}], "Media": file("/plexdata/movies/Casablanca (1942).mkv", 4096)}),
        ],
        ("2", "2") => {
            // Found only through its episodes, which come after it.
            vec![json!({"ratingKey": "200", "type": "show", "title": "Show", "summary": "A show.",
                        "thumb": "/library/metadata/200/thumb/1", "Guid": [{"id": "tvdb://999"}]})]
        }
        ("2", "3") => vec![json!({"ratingKey": "210", "type": "season", "title": "Season 1", "index": 1,
                                  "parentRatingKey": "200"})],
        // The second episode comes first, and Plex has its file under
        // another name: it's found by its numbers once the first is found.
        ("2", "4") => vec![
            json!({"ratingKey": "202", "type": "episode", "title": "Two", "index": 2, "parentIndex": 1,
                   "parentRatingKey": "210", "grandparentRatingKey": "200", "grandparentTitle": "Show",
                   "Media": file("/plexdata/tv/Show/S1/two.mkv", 5)}),
            json!({"ratingKey": "201", "type": "episode", "title": "One", "summary": "It begins.", "index": 1, "parentIndex": 1,
                   "parentRatingKey": "210", "grandparentRatingKey": "200", "grandparentTitle": "Show",
                   "Media": file("/plexdata/tv/Show (2015)/Season 01/Show - S01E01.mkv", 1024)}),
        ],
        _ => Vec::new(),
    }
}

/// What `server_token`'s account has watched, resumed and rated.
fn state_of(server_token: &str, key: &str) -> Value {
    match (server_token, key) {
        // Watched twice, and part way through a third time.
        (OWNER_SERVER, "101") => {
            json!({"viewCount": 2, "viewOffset": 30_000, "lastViewedAt": 1_500_000_000, "userRating": 8.0})
        }
        (OWNER_SERVER, "102") => json!({"viewOffset": 60_000, "lastViewedAt": 1_600_000_000}),
        (KID_SERVER, "101") => json!({"viewCount": 1, "lastViewedAt": 1_450_000_000}),
        _ => json!({}),
    }
}

async fn all(
    State(base): State<String>,
    Path(section): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let _ = base;
    let who = token(&headers).to_owned();
    if who != OWNER_SERVER && who != KID_SERVER {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if who == KID_SERVER && section != "1" {
        return StatusCode::NOT_FOUND.into_response();
    }
    if q.get("X-Plex-Container-Start").is_some_and(|s| s != "0") {
        return container(json!({ "Metadata": [] }));
    }
    let kind = q.get("type").map(String::as_str).unwrap_or("1");
    let mut list: Vec<Value> = items(&section, kind)
        .into_iter()
        .map(|mut item| {
            let state = state_of(&who, item["ratingKey"].as_str().unwrap());
            for (k, v) in state.as_object().unwrap() {
                item[k] = v.clone();
            }
            item
        })
        .collect();
    if let Some(label) = q.get("label") {
        // Alien has both: Scary, and Kometa's mark of its overlay.
        list.retain(|i| (label == "10" || label == "11") && i["ratingKey"] == "101");
    }
    if q.get("unwatched").is_some_and(|v| v == "0") {
        list.retain(|i| i.get("viewCount").is_some());
    }
    if q.get("inProgress").is_some_and(|v| v == "1") {
        list.retain(|i| i.get("viewOffset").is_some());
    }
    if q.contains_key("userRating>>") {
        list.retain(|i| i.get("userRating").is_some());
    }
    container(json!({ "Metadata": list }))
}

async fn history(Query(q): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    if token(&headers) != OWNER_SERVER {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if q.get("X-Plex-Container-Start").is_some_and(|s| s != "0") {
        return container(json!({ "Metadata": [] }));
    }
    let plays = match q.get("accountID").map(String::as_str) {
        Some("1") => json!([
            {"ratingKey": "101", "viewedAt": 1_400_000_000, "accountID": 1, "deviceID": 5},
            {"ratingKey": "101", "viewedAt": 1_500_000_000, "accountID": 1, "deviceID": 5},
            {"ratingKey": "103", "viewedAt": 1_300_000_000, "accountID": 1, "deviceID": 5},
        ]),
        Some("885") => json!([{"ratingKey": "101", "viewedAt": 1_450_000_000, "accountID": 885}]),
        _ => json!([]),
    };
    container(json!({ "Metadata": plays }))
}

fn moved(to: &str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, to.to_owned())]).into_response()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn picture(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "image/png")], bytes).into_response()
}

async fn fake_plex() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    // Each sign-in gets a PIN of its own; only the first is ever allowed,
    // the others expire.
    let pins = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(7));
    let app = Router::new()
        // plex.tv
        .route(
            "/api/v2/pins",
            post(move || async move {
                let id = pins.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                axum::Json(json!({"id": id, "code": "ABCD"}))
            }),
        )
        .route(
            "/api/v2/pins/{id}",
            get(|Path(id): Path<i64>| async move {
                if id != 7 {
                    return StatusCode::NOT_FOUND.into_response();
                }
                axum::Json(json!({"id": 7, "code": "ABCD", "authToken": OWNER})).into_response()
            }),
        )
        .route(
            "/api/v2/resources",
            get(|State(base): State<String>, headers: HeaderMap| async move {
                let server_token = match token(&headers) {
                    OWNER => OWNER_SERVER,
                    KID => KID_SERVER,
                    _ => return StatusCode::UNAUTHORIZED.into_response(),
                };
                let port = base.rsplit(':').next().unwrap().to_owned();
                axum::Json(json!([
                    {"name": "Phone", "clientIdentifier": "p", "provides": "player"},
                    {"name": "A friend's", "clientIdentifier": "shared-1", "provides": "server", "owned": false,
                     "accessToken": "x", "connections": []},
                    {"name": "Den", "clientIdentifier": MACHINE, "provides": "server", "owned": true,
                     "productVersion": "1.43.5", "accessToken": server_token, "httpsRequired": false,
                     "connections": [{"uri": base, "address": "127.0.0.1", "port": port, "local": true}]}
                ]))
                .into_response()
            }),
        )
        .route(
            "/api/v2/home/users",
            get(|State(base): State<String>, headers: HeaderMap| async move {
                if token(&headers) != OWNER {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                axum::Json(json!({"id": 4, "name": "Home", "users": [
                    {"id": 1, "uuid": "u-owner", "title": "Alex", "admin": true,
                     "thumb": format!("{base}/users/u-owner/avatar?c=1")},
                    {"id": 885, "uuid": "u-kid", "title": "Kid One", "restricted": true, "protected": true,
                     "thumb": format!("{base}/users/u-kid/avatar?c=1")},
                    // A Home member with their own account: Plex doesn't call them
                    // restricted, and keeps their rules as sharing filters.
                    {"id": 549, "uuid": "u-teen", "title": "Teen", "restricted": false},
                    // A managed user with no rules: restricted to Plex, but sees everything.
                    {"id": 612, "uuid": "u-free", "title": "Sam", "restricted": true}
                ]}))
                .into_response()
            }),
        )
        .route(
            "/api/v2/home/users/{uuid}/switch",
            post(|Path(uuid): Path<String>| async move {
                if uuid == "u-kid" {
                    axum::Json(json!({"authToken": KID})).into_response()
                } else {
                    StatusCode::FORBIDDEN.into_response()
                }
            }),
        )
        .route(
            "/api/users",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/xml")],
                    r#"<?xml version="1.0"?><MediaContainer><User id="885" title="Kid One" home="1"
                        filterMovies="contentRating=G%2CPG|label=Family&amp;label!=Scary"
                        filterTelevision="contentRating=TV-Y%2CTV-PG|label=Family" filterMusic=""/>
                        <User id="549" title="Teen" home="1" filterMovies="label!=Scary" filterTelevision="label!=Scary"
                        filterMusic=""/>
                        <User id="612" title="Sam" home="1" filterMovies="" filterTelevision="" filterMusic=""/>
                        </MediaContainer>"#,
                )
            }),
        )
        // The server
        .route(
            "/identity",
            get(|| async { container(json!({"machineIdentifier": MACHINE})) }),
        )
        .route(
            "/library/sections",
            get(|headers: HeaderMap| async move {
                let mut sections = vec![
                    json!({"key": "1", "title": "Films", "type": "movie", "Location": [{"id": 1, "path": "/plexdata/movies"}]}),
                    json!({"key": "2", "title": "TV", "type": "show", "Location": [{"id": 2, "path": "/plexdata/tv"}]}),
                    json!({"key": "3", "title": "Photos", "type": "photo", "Location": []}),
                ];
                match token(&headers) {
                    OWNER_SERVER => {}
                    KID_SERVER => sections.truncate(1),
                    _ => return StatusCode::UNAUTHORIZED.into_response(),
                }
                container(json!({ "Directory": sections }))
            }),
        )
        .route("/library/sections/{section}/all", get(all))
        .route(
            "/library/sections/{section}/label",
            get(|Path(section): Path<String>| async move {
                let labels = if section == "1" { json!([{"key": "10", "title": "Scary"}, {"key": "11", "title": "Overlay"}]) } else { json!([]) };
                container(json!({ "Directory": labels }))
            }),
        )
        // Titles' own pages: what was edited in Plex is locked.
        .route(
            "/library/metadata/{key}",
            get(|Path(key): Path<String>| async move {
                let page = match key.as_str() {
                    "101" => json!({"ratingKey": "101", "type": "movie", "title": "Alien", "summary": "Hand-written.",
                                    "contentRating": "nz/R", "studio": "Not locked",
                                    "Field": [{"name": "summary", "locked": true}, {"name": "contentRating", "locked": "1"}]}),
                    "210" => json!({"ratingKey": "210", "type": "season", "title": "The First", "summary": "",
                                    "Field": [{"name": "title", "locked": true}, {"name": "summary", "locked": true}]}),
                    "5000" => json!({"ratingKey": "5000", "type": "collection", "title": "Space",
                                     "Label": [{"tag": "Kometa"}]}),
                    _ => return container(json!({ "Metadata": [] })),
                };
                container(json!({ "Metadata": [page] }))
            }),
        )
        .route(
            "/library/sections/{section}/collections",
            get(|Path(section): Path<String>| async move {
                let list = if section == "1" {
                    json!([{"ratingKey": "5000", "type": "collection", "title": "Space", "smart": "0",
                            "collectionSort": "2", "summary": "In space."},
                           {"ratingKey": "5001", "type": "collection", "title": "Unwatched", "smart": "1"}])
                } else {
                    json!([])
                };
                container(json!({ "Metadata": list }))
            }),
        )
        .route(
            "/library/collections/{key}/children",
            get(|Path(key): Path<String>| async move {
                let list = if key == "5000" {
                    json!([{"ratingKey": "102", "type": "movie", "title": "Brazil"},
                           {"ratingKey": "101", "type": "movie", "title": "Alien"},
                           {"ratingKey": "103", "type": "movie", "title": "Casablanca"}])
                } else {
                    json!([])
                };
                container(json!({ "Metadata": list }))
            }),
        )
        .route(
            "/playlists",
            get(|headers: HeaderMap| async move {
                let list = match token(&headers) {
                    OWNER_SERVER => json!([
                        {"ratingKey": "6000", "type": "playlist", "title": "Favourites", "playlistType": "video", "smart": false},
                        {"ratingKey": "6001", "type": "playlist", "title": "Recent", "playlistType": "video", "smart": true},
                        {"ratingKey": "6002", "type": "playlist", "title": "Holidays", "playlistType": "photo", "smart": false}
                    ]),
                    KID_SERVER => json!([]),
                    _ => return StatusCode::UNAUTHORIZED.into_response(),
                };
                container(json!({ "Metadata": list }))
            }),
        )
        .route(
            "/playlists/{key}/items",
            get(|Path(key): Path<String>| async move {
                let list = if key == "6000" {
                    json!([{"ratingKey": "201", "type": "episode", "title": "One"},
                           {"ratingKey": "101", "type": "movie", "title": "Alien"},
                           {"ratingKey": "103", "type": "movie", "title": "Casablanca"}])
                } else {
                    json!([])
                };
                container(json!({ "Metadata": list }))
            }),
        )
        // Pictures: plex.tv's address for each leads on to it.
        .route("/users/u-owner/avatar", get(|| async { moved("/pictures/alex.png") }))
        .route("/users/u-kid/avatar", get(|| async { moved("/avatars/EB9F9F/4B/3") }))
        .route(
            "/users/elsewhere/avatar",
            get(|State(base): State<String>| async move {
                moved(&format!("{}/pictures/alex.png", base.replace("127.0.0.1", "localhost")))
            }),
        )
        .route("/hop/{n}", get(|Path(n): Path<u32>| async move {
            if n < 4 { moved(&format!("/hop/{}", n + 1)) } else { picture(png(300, 200)) }
        }))
        .route("/pictures/alex.png", get(|| async { picture(png(300, 200)) }))
        .route("/pictures/huge.png", get(|| async { picture(vec![0; 9 << 20]) }))
        .route("/pictures/page.html", get(|| async { ([(header::CONTENT_TYPE, "text/html")], "<html></html>") }))
        .route("/devices", get(|| async { container(json!({"Device": [{"id": 5, "name": "Living Room TV"}]})) }))
        .route("/status/sessions/history/all", get(history))
        .with_state(base.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// A transcoder that reads no file, as tests have none, and holds the TV
/// library's until its flag is lowered, so that library is still scanned.
async fn holding_transcoder() -> (String, Arc<AtomicBool>) {
    let held = Arc::new(AtomicBool::new(true));
    let app = Router::new()
        .route(
            "/v1/probe",
            post(|State(held): State<Arc<AtomicBool>>, body: String| async move {
                while body.contains("/tv/") && held.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let error = json!({"code": "unavailable", "message": "no ffprobe here"});
                (StatusCode::SERVICE_UNAVAILABLE, axum::Json(error)).into_response()
            }),
        )
        .with_state(held.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, held)
}

async fn finished(app: &TestApp, admin: &str) -> Value {
    for _ in 0..400 {
        let r = app.get("/api/import/plex", admin).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        if r.body["state"] != "running" {
            assert_eq!(r.body["scan"], Value::Null, "no scan once it's over");
            return r.body;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the import didn't finish");
}

/// The import's steps, as its progress lists them: each with its state.
fn steps(progress: &Value) -> Vec<(String, String)> {
    let rows = progress["steps"].as_array().cloned().unwrap_or_default();
    rows.iter().map(|r| (r["step"].as_str().unwrap().to_owned(), r["state"].as_str().unwrap().to_owned())).collect()
}

/// A number `sql` answers, with `bind` for its `?`.
/// Signs in to Plex with the owner's token and picks the fake server: each
/// run needs a sign-in.
async fn sign_in(app: &TestApp, admin: &str) {
    let r = app.post("/api/import/plex/token", admin, json!({"token": OWNER})).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.body);
    let r = app.post("/api/import/plex/server", admin, json!({"id": MACHINE})).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
}

async fn one(app: &TestApp, sql: &'static str, bind: i64) -> i64 {
    sqlx::query_scalar(sql).bind(bind).fetch_one(&app.state.db.read).await.unwrap()
}

#[tokio::test]
async fn brings_libraries_people_and_what_they_watched() {
    let plex = fake_plex().await;
    let (transcoder, tv_held) = holding_transcoder().await;
    let app = TestApp::with_config(|c| {
        c.plex_tv = plex.clone();
        c.transcoder_url = Some(transcoder);
    })
    .await;
    let admin = app.setup_admin().await;
    let admin_id: i64 =
        sqlx::query_scalar("SELECT id FROM users WHERE is_admin = 1").fetch_one(&app.state.db.read).await.unwrap();
    app.media_file("movies/Alien (1979).mkv", 1024);
    app.media_file("movies/Brazil (1985).mkv", 2048);
    app.media_file("tv/Show (2015)/Season 01/Show - S01E01.mkv", 1024);
    app.media_file("tv/Show (2015)/Season 01/Show - S01E02.mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;

    // Administrators only.
    let (someone, _) = app.user(&admin, "someone", json!({})).await;
    assert_eq!(app.get("/api/import/plex/plan", &someone).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post("/api/import/plex/pin", &someone, json!({})).await.status, StatusCode::FORBIDDEN);
    // Never apps, even an administrator's with every scope.
    let (app_token, _) = app.app_token(admin_id, "library play activity requests").await;
    let r = app.app_call(Method::GET, "/api/import/plex", &app_token, None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{:?}", r.body);

    // Nothing before signing in.
    let r = app.get("/api/import/plex/plan", &admin).await;
    assert_eq!((r.status, r.body["code"].as_str()), (StatusCode::CONFLICT, Some("plex_signed_out")));

    let r = app.post("/api/import/plex/pin", &admin, json!({"forward_url": "http://pley.local/setup"})).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["code"], "ABCD");
    assert!(r.body["auth_url"].as_str().unwrap().starts_with("https://app.plex.tv/auth#?clientID=pley-"));
    let r = app.get("/api/import/plex/pin", &admin).await;
    assert_eq!(r.body["signed_in"], true, "{:?}", r.body);

    let r = app.get("/api/import/plex/servers", &admin).await;
    // Its own first; one shared with it, for the web app to say so.
    assert_eq!(
        r.body,
        json!([{"id": MACHINE, "name": "Den", "owned": true, "version": "1.43.5"},
               {"id": "shared-1", "name": "A friend's", "owned": false, "version": null}])
    );
    // Only a server the account owns: a friend's has no history to read.
    let r = app.post("/api/import/plex/server", &admin, json!({"id": "shared-1"})).await;
    assert_eq!(r.body["code"], "plex_not_owned", "{:?}", r.body);
    let r = app.post("/api/import/plex/server", &admin, json!({"id": MACHINE})).await;
    assert_eq!((r.status, &r.body["name"]), (StatusCode::OK, &json!("Den")), "{:?}", r.body);

    let plan = app.get("/api/import/plex/plan", &admin).await;
    assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.body);
    assert_eq!(plan.body["server"], "Den");
    let libraries = plan.body["libraries"].as_array().unwrap();
    assert_eq!(libraries.len(), 2, "photos aren't brought: {libraries:?}");
    assert_eq!(libraries[0]["pairing"]["status"], "unsure", "Casablanca isn't in Pley: {:?}", libraries[0]);
    assert_eq!(libraries[0]["pairing"]["library_id"], movies);
    assert_eq!(
        libraries[0]["pairing"]["folders"],
        json!([{"plex": "/plexdata/movies", "pley": app.media_path("movies")}])
    );
    assert_eq!(libraries[1]["pairing"]["status"], "not_in_pley");
    // Nowhere Pley looks has its files: nothing's offered for its folder.
    assert_eq!(libraries[1]["pairing"]["folders_found"], json!([]));
    // How much each holds: movies; shows and their episodes.
    assert_eq!((&libraries[0]["items"], &libraries[0]["groups"]), (&json!(3), &Value::Null));
    assert_eq!((&libraries[1]["items"], &libraries[1]["groups"]), (&json!(2), &json!(1)));

    // A folder given is checked against the files the plan sampled.
    let check = |library: &'static str, plex: &'static str, pley: String| {
        let (app, admin) = (&app, &admin);
        async move {
            let r = app
                .post("/api/import/plex/folder", admin, json!({"library": library, "plex": plex, "pley": pley}))
                .await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
            r.body["state"].as_str().unwrap().to_owned()
        }
    };
    assert_eq!(check("1", "/plexdata/movies", app.media_path("movies")).await, "found");
    std::fs::create_dir_all(app.media_path("elsewhere")).unwrap();
    assert_eq!(check("1", "/plexdata/movies", app.media_path("elsewhere")).await, "none_found");
    assert_eq!(check("1", "/plexdata/movies", app.media_path("nowhere")).await, "missing");
    // No file was sampled from a folder Plex doesn't have.
    assert_eq!(check("1", "/plexdata/other", app.media_path("movies")).await, "untold");
    let r = app
        .post("/api/import/plex/folder", &someone, json!({"library": "1", "plex": "/plexdata/movies", "pley": "/"}))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Pictures: Alex's own, Plex's colour alone for Kid One (an initial on
    // it, which Pley draws itself), nothing for the others.
    let pictures: Vec<(&Value, &Value)> =
        plan.body["users"].as_array().unwrap().iter().map(|u| (&u["picture"], &u["color"])).collect();
    assert_eq!(
        pictures,
        [
            (&json!(true), &json!(null)),
            (&json!(false), &json!("#eb9f9f")),
            (&json!(false), &json!(null)),
            (&json!(false), &json!(null))
        ]
    );
    let alex = app.get("/api/import/plex/users/1/picture", &admin).await;
    assert_eq!((alex.status, alex.headers[header::CONTENT_TYPE].to_str().unwrap()), (StatusCode::OK, "image/jpeg"));
    assert_eq!(image::load_from_memory(&alex.bytes).unwrap().width(), 256, "made as a profile's picture is");
    assert_eq!(app.get("/api/import/plex/users/885/picture", &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/import/plex/users/1/picture", &someone).await.status, StatusCode::FORBIDDEN);

    let kid = &plan.body["users"][1];
    assert_eq!(kid["title"], "Kid One");
    assert_eq!(
        kid["limits"],
        json!({"max_age": 10, "max_age_system": "US", "show_labels": ["Family"], "hide_labels": ["Scary"],
               "left_over": [], "not_kept": []})
    );
    let teen = &plan.body["users"][2];
    assert_eq!((&teen["title"], &teen["restricted"]), (&json!("Teen"), &json!(true)));
    assert_eq!(
        teen["limits"],
        json!({"max_age": null, "max_age_system": null, "show_labels": [], "hide_labels": ["Scary"],
               "left_over": [], "not_kept": []})
    );
    let free = &plan.body["users"][3];
    assert_eq!((&free["title"], &free["restricted"]), (&json!("Sam"), &json!(false)));
    assert_eq!(free["limits"]["max_age"], json!(null));
    assert!(!plan.mentions(OWNER) && !plan.mentions(OWNER_SERVER), "no token in the plan");

    let choices = json!({
        "libraries": [
            {"key": "1", "library_id": movies},
            {"key": "2", "new": {"name": "TV", "paths": [app.media_path("tv")]}},
        ],
        "folders": [
            {"plex": "/plexdata/movies", "pley": app.media_path("movies")},
            {"plex": "/plexdata/tv", "pley": app.media_path("tv")},
        ],
        "users": [{"id": 885}],
    });
    // Pley's search found nothing for Alien; Brazil's NFO file names its
    // match; Pley has a TMDB collection called Space.
    sqlx::query("UPDATE items SET metadata_at = -1 WHERE title = 'Alien'").execute(&app.state.db.write).await.unwrap();
    sqlx::query(
        "INSERT INTO item_nfo (item_id, path, size, mtime, ids, fields)
         SELECT id, '/nfo', 1, 1, '{\"imdb\":\"tt0088846\"}', '{}' FROM items WHERE title = 'Brazil'",
    )
    .execute(&app.state.db.write)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO collections (library_id, title, sort_title, tmdb_id, created_at, updated_at)
         VALUES (?, 'Space', 'Space', 9, 0, 0)",
    )
    .bind(movies)
    .execute(&app.state.db.write)
    .await
    .unwrap();
    let r = app.post("/api/import/plex/start", &admin, choices.clone()).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);

    // Movies is matched while TV is still scanned: its titles get Plex's
    // dates added without waiting for TV's files.
    let alien: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE title = 'Alien'").fetch_one(&app.state.db.read).await.unwrap();
    for _ in 0..400 {
        if one(&app, "SELECT added_at FROM items WHERE id = ?", alien).await == 1_000_000_000 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(one(&app, "SELECT added_at FROM items WHERE id = ?", alien).await, 1_000_000_000);
    // Each library the import brings: Movies done, with the titles found in
    // it, TV's files still to be read.
    let mut scanning = app.get("/api/import/plex", &admin).await.body;
    for _ in 0..400 {
        if scanning["libraries"][0]["phase"] == "done" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        scanning = app.get("/api/import/plex", &admin).await.body;
    }
    let libraries = scanning["libraries"].as_array().unwrap();
    let names: Vec<&str> = libraries.iter().map(|l| l["library"].as_str().unwrap()).collect();
    assert_eq!(names, ["Movies", "TV"], "{scanning:?}");
    assert_eq!(libraries[0]["phase"], "done", "{scanning:?}");
    assert!(libraries[0]["found"].as_u64().is_some_and(|n| n > 0), "{scanning:?}");
    assert!(["queued", "walking", "probing"].contains(&libraries[1]["phase"].as_str().unwrap()), "{scanning:?}");
    assert_eq!(libraries[1]["found"], Value::Null);
    assert_eq!((&scanning["step"], &scanning["scan"]["library"]), (&json!("scanning"), &json!("TV")), "{scanning:?}");
    // Every step it goes through is listed: the profiles made, the scans
    // under way, the rest to come.
    let to_come =
        ["labels", "edits", "collections", "watch_state", "history", "playlists", "fetching", "matching_again"];
    let mut listed = vec![("profiles".to_owned(), "done".to_owned()), ("scanning".to_owned(), "running".to_owned())];
    listed.extend(to_come.iter().map(|step| (step.to_string(), "to_come".to_owned())));
    assert_eq!(steps(&scanning), listed);
    assert_eq!(scanning["steps"][0]["count"], 2, "the administrator's profile and the Home user's");
    let started_at = scanning["started_at"].as_i64().unwrap();
    assert!(started_at > 0, "{scanning:?}");
    // Marked as running, for a restart to be told.
    let marked: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'plex_import_running'")
        .fetch_optional(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(marked, Some(started_at.to_string()));
    // The Home user's profile, which never sees a label, can sign in, but
    // its library waits until the labels are on Pley's titles.
    let (kid, disabled, all): (i64, bool, bool) = sqlx::query_as(
        "SELECT u.id, u.disabled, u.all_libraries FROM users u JOIN import_accounts i ON i.user_id = u.id",
    )
    .fetch_one(&app.state.db.read)
    .await
    .unwrap();
    assert!(!disabled && !all);
    assert_eq!(one(&app, "SELECT COUNT(*) FROM user_libraries WHERE user_id = ?", kid).await, 0);
    let waiting: Vec<String> = sqlx::query_scalar(
        "SELECT l.name FROM import_waiting_libraries w JOIN libraries l ON l.id = w.library_id WHERE w.user_id = ?",
    )
    .bind(kid)
    .fetch_all(&app.state.db.read)
    .await
    .unwrap();
    assert_eq!(waiting, ["Movies"]);
    tv_held.store(false, Ordering::SeqCst);

    let done = finished(&app, &admin).await;
    assert_eq!(done["state"], "done", "{done:?}");
    assert!(!serde_json::to_string(&done).unwrap().contains(OWNER), "no token in the report");
    let marked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings WHERE key = 'plex_import_running'")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(marked, 0, "the mark goes with the report");

    let report = &done["report"];
    // The sign-in went with the run.
    let r = app.get("/api/import/plex/plan", &admin).await;
    assert_eq!(r.body["code"], "plex_signed_out", "{:?}", r.body);
    // How long it took, and each step that ran, in order.
    assert_eq!(report["started_at"], done["started_at"]);
    let timed: Vec<&str> = report["steps"].as_array().unwrap().iter().map(|s| s["step"].as_str().unwrap()).collect();
    assert_eq!(timed.first(), Some(&"profiles"), "{timed:?}");
    assert_eq!(timed.last(), Some(&"matching_again"), "{timed:?}");
    assert!(timed.contains(&"scanning") && timed.contains(&"fetching"), "{timed:?}");
    assert!(report["steps"].as_array().unwrap().iter().all(|s| s["secs"].is_u64()));
    // Plex's details where Pley has nothing better (no metadata fetched
    // here, Alien not found by its search), but not over Brazil's NFO
    // file; never Plex's posters, which Pley's fetch replaces.
    let titles: Vec<(String, Option<i64>, Option<String>, bool)> = sqlx::query_as(
        "SELECT title, year, summary, poster IS NOT NULL FROM items
         WHERE kind IN ('movie', 'show', 'episode') ORDER BY kind, title",
    )
    .fetch_all(&app.state.db.read)
    .await
    .unwrap();
    let titles: Vec<_> = titles.iter().map(|(t, y, s, p)| (t.as_str(), *y, s.as_deref(), *p)).collect();
    assert_eq!(
        titles,
        [
            ("One", None, Some("It begins."), false),
            ("Two", None, None, false),
            ("Alien", Some(1979), Some("Hand-written."), false),
            ("Brazil", Some(1985), None, false),
            ("Show", Some(2015), Some("A show."), false),
        ]
    );
    assert_eq!(report["libraries"][0]["matched"], 2);
    assert_eq!(report["libraries"][0]["missed"], 1);
    assert_eq!(report["libraries"][0]["missed_titles"], json!(["Casablanca"]));
    assert_eq!(report["libraries"][1]["created"], true);
    assert_eq!(report["libraries"][1]["matched"], 2, "{:?}", report["libraries"][1]);
    assert_eq!(report["labels_added"], 1);

    // The Home user's profile, with Plex's limits and libraries.
    let kid_id = report["profiles"][1]["user_id"].as_i64().unwrap();
    assert_eq!(report["profiles"][1]["created"], true);
    let (name, managed, home, all, max_age, system): (String, bool, bool, bool, Option<i64>, Option<String>) =
        sqlx::query_as(
            "SELECT username, managed, home, all_libraries, max_age, max_age_system FROM users WHERE id = ?",
        )
        .bind(kid_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(
        (name.as_str(), managed, home, all, max_age, system.as_deref()),
        ("kid.one", true, true, false, Some(10), Some("US"))
    );
    let opened: Vec<i64> = sqlx::query_scalar("SELECT library_id FROM user_libraries WHERE user_id = ?")
        .bind(kid_id)
        .fetch_all(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(opened, [movies]);
    assert_eq!(pley::accounts::label_rules(&app.state.db, kid_id, "hide").await.unwrap(), ["Scary"]);
    assert_eq!(pley::accounts::label_rules(&app.state.db, kid_id, "show").await.unwrap(), ["Family"]);
    // Its library opened once the import finished.
    assert_eq!(report["profiles"][1]["waiting"], json!([]));
    assert_eq!(one(&app, "SELECT COUNT(*) FROM import_waiting_libraries WHERE user_id = ?", kid_id).await, 0);

    // Pictures: the account's own, squared to Pley's size; Plex's default,
    // an initial on a colour, brings only its colour.
    let avatar_of = |id: i64| {
        let db = app.state.db.read.clone();
        async move {
            sqlx::query_as::<_, (Option<String>, Option<String>)>("SELECT avatar, avatar_color FROM users WHERE id = ?")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    let (alex, alex_color) = avatar_of(admin_id).await;
    let alex = alex.expect("the account's picture");
    let jpeg = std::fs::read(pley::accounts::avatars_dir(&app.state.config.data_dir).join(&alex)).unwrap();
    let decoded = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap();
    assert_eq!((decoded.width(), decoded.height(), alex_color), (256, 256, None));
    assert_eq!(avatar_of(kid_id).await, (None, Some("#eb9f9f".into())));
    // Names: the account's Plex name for the administrator, who had none.
    let name_of = |id: i64| {
        let db = app.state.db.read.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>("SELECT display_name FROM users WHERE id = ?")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(name_of(admin_id).await.as_deref(), Some("Alex"));

    // Titles: labels and dates added.
    let alien: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE title = 'Alien'").fetch_one(&app.state.db.read).await.unwrap();
    let brazil: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE title = 'Brazil'").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(pley::labels::for_items(&app.state.db, &[alien]).await.unwrap()[&alien], ["Scary"]);
    assert_eq!(one(&app, "SELECT added_at FROM items WHERE id = ?", alien).await, 1_000_000_000);

    // Matches and edits: Pley has no TMDB key here, so Alien starts from
    // Plex's match at the next refresh; Brazil keeps its NFO file's. Locked
    // fields become edits, an empty one left out.
    assert_eq!(report["matches_fixed"], 0);
    assert_eq!(one(&app, "SELECT tmdb_id FROM items WHERE id = ?", alien).await, 348);
    assert_eq!(one(&app, "SELECT metadata_at IS NULL FROM items WHERE id = ?", alien).await, 1);
    assert_eq!(one(&app, "SELECT tmdb_id IS NULL FROM items WHERE id = ?", brazil).await, 1);
    assert_eq!(report["edits"], json!({"content_rating": 1, "summary": 1, "title": 1}));
    let edited: Vec<(String, String)> =
        sqlx::query_as("SELECT field, value FROM item_overrides WHERE item_id = ? ORDER BY field")
            .bind(alien)
            .fetch_all(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(edited, [("content_rating".into(), "nz/R".into()), ("summary".into(), "Hand-written.".into())]);
    // Plex's rating keeps its country, and New Zealand's age, on a US server.
    let rating: (String, String, i64) =
        sqlx::query_as("SELECT content_rating, content_rating_country, rating_age FROM items WHERE id = ?")
            .bind(alien)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(rating, ("R".into(), "NZ".into(), 18));
    // The new TV library wasn't fetched (no key): its show took Plex's
    // TheTVDB id, for TheTVDB's or TVmaze's refresh to find it by.
    assert_eq!(one(&app, "SELECT tvdb_id FROM items WHERE kind = 'show' AND id > ?", 0).await, 999);
    let season: String = sqlx::query_scalar("SELECT title FROM items WHERE kind = 'season'")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(season, "The First");

    // Collections and playlists, smart ones left out.
    assert_eq!((&report["collections_made"], &report["collection_items"]), (&json!(1), &json!(2)));
    assert_eq!(report["smart_left"], 2);
    // Pley's TMDB collection isn't Plex's: a collection is made beside it.
    assert_eq!(
        one(
            &app,
            "SELECT COUNT(*) FROM collection_items i JOIN collections c ON c.id = i.collection_id WHERE c.tmdb_id = ?",
            9
        )
        .await,
        0
    );
    let (space, sort): (i64, String) =
        sqlx::query_as("SELECT id, sort FROM collections WHERE title = 'Space' AND tmdb_id IS NULL")
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(sort, "custom");
    let order: Vec<i64> =
        sqlx::query_scalar("SELECT item_id FROM collection_items WHERE collection_id = ? ORDER BY position")
            .bind(space)
            .fetch_all(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(order, [brazil, alien]);
    assert_eq!(pley::labels::for_collections(&app.state.db, &[space]).await.unwrap()[&space], ["Kometa"]);
    assert_eq!(report["profiles"][0]["playlists"], 1);
    let favourites: Vec<String> = sqlx::query_scalar(
        "SELECT i.title FROM playlist_items e JOIN playlists p ON p.id = e.playlist_id JOIN items i ON i.id = e.item_id
         WHERE p.title = 'Favourites' AND p.user_id = ? ORDER BY e.position",
    )
    .bind(admin_id)
    .fetch_all(&app.state.db.read)
    .await
    .unwrap();
    assert_eq!(favourites, ["One", "Alien"], "the episode with Plex's title");

    // Watch state, each person's own.
    let (watched, count, rating): (bool, i64, Option<f64>) =
        sqlx::query_as("SELECT watched, play_count, rating FROM user_item_state WHERE user_id = ? AND item_id = ?")
            .bind(admin_id)
            .bind(alien)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!((watched, count, rating), (true, 2, Some(8.0)));
    let rewatch: i64 = sqlx::query_scalar("SELECT position_ms FROM user_item_state WHERE user_id = ? AND item_id = ?")
        .bind(admin_id)
        .bind(alien)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(rewatch, 30_000, "a rewatch's resume point comes with its watched");
    let position: i64 = sqlx::query_scalar("SELECT position_ms FROM user_item_state WHERE user_id = ? AND item_id = ?")
        .bind(admin_id)
        .bind(brazil)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(position, 60_000);
    assert_eq!(one(&app, "SELECT COUNT(*) FROM user_item_state WHERE user_id = ? AND watched = 1", kid_id).await, 1);

    // History, with the devices' names; Casablanca's play has no title.
    let plays: Vec<(i64, i64, Option<String>)> =
        sqlx::query_as("SELECT user_id, viewed_at, device FROM play_history ORDER BY viewed_at")
            .fetch_all(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(
        plays,
        [
            (admin_id, 1_400_000_000, Some("Living Room TV".to_owned())),
            (kid_id, 1_450_000_000, None),
            (admin_id, 1_500_000_000, Some("Living Room TV".to_owned())),
        ]
    );

    // The kid starts Alien again in Pley, after Plex's watch.
    sqlx::query(
        "UPDATE user_item_state SET watched = 0, position_ms = 99000, last_played_at = 1700000000,
                updated_at = 1700000000 WHERE user_id = ? AND item_id = ?",
    )
    .bind(kid_id)
    .bind(alien)
    .execute(&app.state.db.write)
    .await
    .unwrap();
    // The administrator renames themselves.
    sqlx::query("UPDATE users SET display_name = 'Al' WHERE id = ?")
        .bind(admin_id)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    // And picks a colour of their own.
    sqlx::query("UPDATE users SET avatar_color = '#38bdf8' WHERE id = ?")
        .bind(kid_id)
        .execute(&app.state.db.write)
        .await
        .unwrap();

    // Again: nothing twice, and the same profile.
    sign_in(&app, &admin).await;
    let users_before = one(&app, "SELECT COUNT(*) FROM users WHERE id > ?", 0).await;
    let choices = json!({
        "libraries": [{"key": "1", "library_id": movies}],
        "folders": [{"plex": "/plexdata/movies", "pley": app.media_path("movies")}],
        "users": [{"id": 885}],
    });
    let r = app.post("/api/import/plex/start", &admin, choices).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    let done = finished(&app, &admin).await;
    assert_eq!(done["state"], "done", "{done:?}");
    assert_eq!(done["report"]["profiles"][1]["user_id"], kid_id);
    assert_eq!(done["report"]["profiles"][1]["created"], false);
    assert_eq!(done["report"]["profiles"][1]["waiting"], json!([]), "no library new to it");
    assert_eq!(done["report"]["libraries"][1]["library_id"], Value::Null, "TV left out this time");
    assert_eq!(one(&app, "SELECT COUNT(*) FROM users WHERE id > ?", 0).await, users_before);
    assert_eq!(one(&app, "SELECT COUNT(*) FROM play_history WHERE user_id > ?", 0).await, 3);
    assert_eq!(done["report"]["labels_added"], 0);
    // Pley's edits stay, and collections and playlists aren't made twice.
    assert_eq!(done["report"]["edits"], json!({}));
    assert_eq!((&done["report"]["collections_made"], &done["report"]["collection_items"]), (&json!(0), &json!(0)));
    assert_eq!(one(&app, "SELECT COUNT(*) FROM playlists WHERE user_id = ?", admin_id).await, 1);
    assert_eq!(done["report"]["profiles"][0]["playlists"], 0, "found again with nothing added");
    assert_eq!(one(&app, "SELECT COUNT(*) FROM playlist_items WHERE playlist_id > ?", 0).await, 2);
    let (watched, position): (bool, i64) =
        sqlx::query_as("SELECT watched, position_ms FROM user_item_state WHERE user_id = ? AND item_id = ?")
            .bind(kid_id)
            .bind(alien)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!((watched, position), (true, 99_000), "Pley's newer resume point stays, as a rewatch");
    // Pley's names, pictures and colours stay.
    assert_eq!(name_of(admin_id).await.as_deref(), Some("Al"));
    assert_eq!(avatar_of(admin_id).await, (Some(alex), None));
    assert_eq!(avatar_of(kid_id).await, (None, Some("#38bdf8".into())));

    // Nothing but the libraries and people, when that's what was chosen.
    sign_in(&app, &admin).await;
    let r = app
        .post(
            "/api/import/plex/start",
            &admin,
            json!({
                "libraries": [{"key": "1", "library_id": movies}],
                "folders": [{"plex": "/plexdata/movies", "pley": app.media_path("movies")}],
                "bring": {"watch_history": false, "collections": false, "edits": false},
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    let done = finished(&app, &admin).await;
    let report = &done["report"];
    assert_eq!((&report["profiles"][0]["watched"], &report["profiles"][0]["playlists"]), (&json!(0), &json!(0)));
    assert_eq!((&report["smart_left"], &report["edits"]), (&json!(0), &json!({})));
    assert_eq!(report["bring"], json!({"watch_history": false, "collections": false, "edits": false}));

    // A profile the administrator turns off stays off, whatever the import.
    // With as many labels it never sees as a profile keeps, and no age
    // limit, Plex's "Scary" can't be added: the youngest age hides it.
    let full: Vec<String> = (0..50).map(|n| format!("L{n}")).collect();
    let r = app
        .patch(&format!("/api/users/{kid_id}"), &admin, json!({"disabled": true, "max_age": null, "hide_labels": full}))
        .await;
    assert!(r.status.is_success(), "{:?}", r.body);
    // Plex's movies into a library of their own: the kid, who could open
    // them on Plex, may open it once the import finished.
    app.media_file("films/Alien (1979).mkv", 1024);
    sign_in(&app, &admin).await;
    let r = app
        .post(
            "/api/import/plex/start",
            &admin,
            json!({
                "libraries": [{"key": "1", "new": {"name": "Films", "paths": [app.media_path("films")]}}],
                "folders": [{"plex": "/plexdata/movies", "pley": app.media_path("films")}],
                "users": [{"id": 885}],
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    let done = finished(&app, &admin).await;
    assert_eq!(done["report"]["profiles"][1]["waiting"], json!([]));
    assert_eq!(one(&app, "SELECT disabled FROM users WHERE id = ?", kid_id).await, 1);
    assert_eq!(done["report"]["profiles"][1]["left_over"], json!(["Scary"]));
    assert_eq!(done["report"]["profiles"][1]["max_age"], 0);
    assert_eq!(one(&app, "SELECT max_age FROM users WHERE id = ?", kid_id).await, 0);
    let films = done["report"]["libraries"][0]["library_id"].as_i64().unwrap();
    let opened: Vec<i64> =
        sqlx::query_scalar("SELECT library_id FROM user_libraries WHERE user_id = ? ORDER BY library_id")
            .bind(kid_id)
            .fetch_all(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(opened, [movies, films]);

    // Open to every library, the kid opens those it had while it waits for
    // a new one, and every library again once the import finished.
    let r = app.patch(&format!("/api/users/{kid_id}"), &admin, json!({"all_libraries": true})).await;
    assert!(r.status.is_success(), "{:?}", r.body);
    app.media_file("more/Alien (1979).mkv", 1024);
    app.media_file("more tv/Show (2015)/Season 01/Show - S01E01.mkv", 1024);
    tv_held.store(true, Ordering::SeqCst);
    sign_in(&app, &admin).await;
    let r = app
        .post(
            "/api/import/plex/start",
            &admin,
            json!({
                "libraries": [
                    {"key": "1", "new": {"name": "More films", "paths": [app.media_path("more")]}},
                    {"key": "2", "new": {"name": "More TV", "paths": [app.media_path("more tv")]}},
                ],
                "folders": [
                    {"plex": "/plexdata/movies", "pley": app.media_path("more")},
                    {"plex": "/plexdata/tv", "pley": app.media_path("more tv")},
                ],
                "users": [{"id": 885}],
            }),
        )
        .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    // Its TV held: the kid waits for the films, every other library open.
    for _ in 0..400 {
        if one(&app, "SELECT COUNT(*) FROM import_waiting_libraries WHERE user_id = ?", kid_id).await > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(one(&app, "SELECT all_libraries FROM users WHERE id = ?", kid_id).await, 0);
    assert_eq!(one(&app, "SELECT waits_for_all FROM import_accounts WHERE user_id = ?", kid_id).await, 1);
    let more: i64 = sqlx::query_scalar("SELECT id FROM libraries WHERE name = 'More films'")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(one(&app, "SELECT library_id FROM import_waiting_libraries WHERE user_id = ?", kid_id).await, more);
    let opened: Vec<i64> = sqlx::query_scalar("SELECT library_id FROM user_libraries WHERE user_id = ?")
        .bind(kid_id)
        .fetch_all(&app.state.db.read)
        .await
        .unwrap();
    assert!(!opened.contains(&more) && opened.len() > 2, "{opened:?}");
    tv_held.store(false, Ordering::SeqCst);
    let done = finished(&app, &admin).await;
    assert_eq!(done["state"], "done", "{done:?}");
    assert_eq!(done["report"]["profiles"][1]["waiting"], json!([]));
    assert_eq!(one(&app, "SELECT all_libraries FROM users WHERE id = ?", kid_id).await, 1);
    assert_eq!(
        one(&app, "SELECT COUNT(*) FROM user_libraries WHERE user_id = ?", kid_id).await,
        one(&app, "SELECT COUNT(*) FROM libraries WHERE id > ?", 0).await,
        "each library given on its own while it waited"
    );
    assert_eq!(one(&app, "SELECT waits_for_all FROM import_accounts WHERE user_id = ?", kid_id).await, 0);

    // Libraries the administrator chooses end the waiting.
    sqlx::query("INSERT INTO import_waiting_libraries (user_id, library_id) VALUES (?, ?)")
        .bind(kid_id)
        .bind(films)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE import_accounts SET waits_for_all = 1 WHERE user_id = ?")
        .bind(kid_id)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let r = app.patch(&format!("/api/users/{kid_id}"), &admin, json!({"library_ids": [movies]})).await;
    assert!(r.status.is_success(), "{:?}", r.body);
    assert_eq!(one(&app, "SELECT COUNT(*) FROM import_waiting_libraries WHERE user_id = ?", kid_id).await, 0);
    assert_eq!(one(&app, "SELECT waits_for_all FROM import_accounts WHERE user_id = ?", kid_id).await, 0);

    // Forgetting the sign-in, before a run.
    sign_in(&app, &admin).await;
    assert_eq!(app.delete("/api/import/plex", &admin).await.status, StatusCode::NO_CONTENT);
    let r = app.get("/api/import/plex/plan", &admin).await;
    assert_eq!(r.body["code"], "plex_signed_out");
}

#[tokio::test]
async fn an_import_the_server_restarted_during_says_so() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    // Marked as running, with none running: the server restarted since.
    sqlx::query("INSERT INTO settings (key, value) VALUES ('plex_import_running', '1794100000')")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let progress = app.get("/api/import/plex", &admin).await.body;
    assert_eq!(
        (&progress["state"], &progress["started_at"]),
        (&json!("interrupted"), &json!(1_794_100_000)),
        "{progress:?}"
    );
    assert_eq!(progress["report"], Value::Null);

    // The libraries it made fetch their details, which it would have done
    // last; one from before it doesn't.
    let made = |name: &str, at: i64| {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO libraries (id, name, kind, paths, created_at, position)
             VALUES ((SELECT last + 1 FROM library_ids), ?, 'movies', '[]', ?, 0) RETURNING id",
        )
        .bind(name.to_owned())
        .bind(at)
        .fetch_one(&app.state.db.write)
    };
    made("Before", 1_794_000_000).await.unwrap();
    let films = made("Films", 1_794_100_005).await.unwrap();
    assert_eq!(app.state.plex_import.after_restart(&app.state).await, [films]);
}

#[tokio::test]
async fn a_picture_comes_from_plex_tv_and_never_from_elsewhere() {
    let base = fake_plex().await;
    let plex = pley::import::plex::Plex::new(&base, "pley-test");
    let at = |path: &str| format!("{base}{path}");

    let alex = plex.picture(&at("/users/u-owner/avatar?c=1")).await;
    assert_eq!((alex.bytes, alex.color), (Some(png(300, 200)), None));
    let kid = plex.picture(&at("/users/u-kid/avatar?c=1")).await;
    assert_eq!((kid.bytes, kid.color.as_deref()), (None, Some("#eb9f9f")));
    // Three redirects, and no more.
    assert!(plex.picture(&at("/hop/1")).await.bytes.is_some());
    assert!(plex.picture(&at("/hop/0")).await.bytes.is_none());
    // Not a picture, too large, or not from plex.tv: nothing.
    assert!(plex.picture(&at("/pictures/page.html")).await.bytes.is_none());
    assert!(plex.picture(&at("/pictures/huge.png")).await.bytes.is_none());
    assert!(plex.picture(&at("/users/elsewhere/avatar")).await.bytes.is_none());
    let elsewhere = at("/pictures/alex.png").replace("127.0.0.1", "localhost");
    assert!(plex.picture(&elsewhere).await.bytes.is_none());
    assert!(plex.picture("not an address").await.bytes.is_none());
}

/// A stand-in for TMDB that answers nothing until `open` is sent `true`,
/// and keeps what it was asked.
#[derive(Clone)]
struct Tmdb {
    asked: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    open: tokio::sync::watch::Receiver<bool>,
}

impl Tmdb {
    fn asked(&self, prefix: &str) -> Vec<String> {
        self.asked.lock().unwrap().iter().filter(|r| r.starts_with(prefix)).cloned().collect()
    }
}

async fn tmdb(State(tmdb): State<Tmdb>, uri: axum::http::Uri, Query(q): Query<HashMap<String, String>>) -> Response {
    let path = uri.path().strip_prefix("/tmdb").unwrap_or_default().to_owned();
    tmdb.asked.lock().unwrap().push(format!("{path}?{}", uri.query().unwrap_or_default()));
    let mut open = tmdb.open.clone();
    while !*open.borrow_and_update() {
        open.changed().await.unwrap();
    }
    let movie = |title: &str, date: &str, imdb: &str| {
        json!({"title": title, "original_title": title, "overview": format!("The story of {title}."),
               "tagline": "", "vote_average": 7.5, "release_date": date, "runtime": 100, "genres": [],
               "production_companies": [], "poster_path": null, "backdrop_path": null, "imdb_id": imdb,
               "release_dates": {"results": []}, "credits": {"cast": [], "crew": []},
               "belongs_to_collection": null})
    };
    let body = match path.as_str() {
        "/search/movie" if q.get("query").is_some_and(|t| t == "Casablanca") => json!({"results": [
            {"id": 289, "title": "Casablanca", "original_title": "Casablanca", "release_date": "1942-11-26",
             "popularity": 10.0}]}),
        "/search/movie" => json!({"results": []}),
        "/movie/348" => movie("Alien", "1979-05-25", "tt0078748"),
        "/movie/289" => movie("Casablanca", "1942-11-26", "tt0034583"),
        _ => return (StatusCode::NOT_FOUND, axum::Json(json!({"status_code": 34}))).into_response(),
    };
    axum::Json(body).into_response()
}

#[tokio::test]
async fn plex_data_comes_before_metadata_and_titles_found_by_id_after_it() {
    let plex = fake_plex().await;
    let (open, gate) = tokio::sync::watch::channel(false);
    let stand_in = Tmdb { asked: Default::default(), open: gate };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().route("/tmdb/{*path}", get(tmdb)).with_state(stand_in.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let app = TestApp::with_config(|c| {
        c.plex_tv = plex.clone();
        c.metadata_endpoints = Some(pley::providers::Endpoints {
            tvmaze: format!("{base}/tvmaze"),
            tvdb: format!("{base}/tvdb"),
            musicbrainz: format!("{base}/mb"),
            cover_art: format!("{base}/caa"),
            tmdb: format!("{base}/tmdb"),
            tmdb_images: format!("{base}/tmdb-images"),
            opensubtitles: format!("{base}/opensubtitles"),
        });
    })
    .await;
    let admin = app.setup_admin().await;
    let admin_id: i64 =
        sqlx::query_scalar("SELECT id FROM users WHERE is_admin = 1").fetch_one(&app.state.db.read).await.unwrap();
    let r = app.put("/api/settings", &admin, json!({"tmdb_api_key": "test-key"})).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    // Alien is the file Plex has; Casablanca was copied again (another size),
    // so only its ids can find it; Brazil isn't in Pley.
    app.media_file("movies/Alien (1979).mkv", 1024);
    app.media_file("movies/Casablanca (1942).mkv", 4000);
    app.post("/api/import/plex/pin", &admin, json!({})).await;
    assert_eq!(app.get("/api/import/plex/pin", &admin).await.body["signed_in"], true);
    app.post("/api/import/plex/server", &admin, json!({"id": MACHINE})).await;
    let choices = json!({
        "libraries": [{"key": "1", "new": {"name": "Films", "paths": [app.media_path("movies")]}}],
        "folders": [{"plex": "/plexdata/movies", "pley": app.media_path("movies")}],
    });
    let r = app.post("/api/import/plex/start", &admin, choices).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);

    // TMDB answers nothing yet, and what was found by file has come over.
    let mut progress = Value::Null;
    for _ in 0..400 {
        progress = app.get("/api/import/plex", &admin).await.body;
        if progress["first_pass_done"] == true || progress["state"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!((&progress["state"], &progress["first_pass_done"]), (&json!("running"), &json!(true)), "{progress:?}");
    // Finding the rest is listed, as Casablanca wasn't found by its file.
    let listed: Vec<String> = steps(&progress).into_iter().map(|(step, _)| step).collect();
    let all = ["profiles", "scanning", "labels", "edits", "collections", "watch_state", "history", "playlists"];
    assert_eq!(listed, [&all[..], &["fetching", "matching_again"]].concat(), "{progress:?}");
    assert_eq!(steps(&progress)[..2], [("profiles".into(), "done".into()), ("scanning".into(), "done".into())]);
    assert_eq!(progress["steps"][1]["count"], 1, "Alien, found by its file");
    let alien: i64 = sqlx::query_scalar("SELECT id FROM items WHERE title LIKE 'Alien%'")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let (tmdb_id, imdb_id, fetched, poster): (Option<i64>, Option<String>, bool, bool) =
        sqlx::query_as("SELECT tmdb_id, imdb_id, metadata_at IS NOT NULL, poster IS NOT NULL FROM items WHERE id = ?")
            .bind(alien)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(
        (tmdb_id, imdb_id.as_deref(), fetched, poster),
        (Some(348), Some("tt0078748"), false, false),
        "Plex's match, before any metadata"
    );
    let watched: i64 =
        sqlx::query_scalar("SELECT play_count FROM user_item_state WHERE user_id = ? AND item_id = ? AND watched = 1")
            .bind(admin_id)
            .bind(alien)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(watched, 2);

    open.send(true).unwrap();
    let done = finished(&app, &admin).await;
    assert_eq!(done["state"], "done", "{done:?}");
    // Alien was fetched by Plex's id, never searched for by its name.
    assert!(stand_in.asked("/search/movie?query=Alien").is_empty(), "{:?}", stand_in.asked("/search"));
    assert_eq!(stand_in.asked("/movie/348").len(), 1, "{:?}", stand_in.asked("/"));
    // Fetched, with Plex's locked summary kept over TMDB's.
    let (date, summary): (String, String) = sqlx::query_as("SELECT release_date, summary FROM items WHERE id = ?")
        .bind(alien)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!((date.as_str(), summary.as_str()), ("1979-05-25", "Hand-written."));
    // Casablanca, found by the id its fetch gave it, then its play.
    let report = &done["report"]["libraries"][0];
    assert_eq!((&report["matched"], &report["missed"]), (&json!(2), &json!(1)), "{report:?}");
    assert_eq!(report["missed_titles"], json!(["Brazil"]));
    let casablanca: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE tmdb_id = 289").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(one(&app, "SELECT COUNT(*) FROM play_history WHERE item_id = ?", casablanca).await, 1);
    // And put in the playlist made before it was found: still one playlist,
    // and the smart ones left out once.
    assert_eq!(one(&app, "SELECT COUNT(*) FROM playlist_items WHERE item_id = ?", casablanca).await, 1);
    assert_eq!(done["report"]["profiles"][0]["playlists"], 1);
    assert_eq!(done["report"]["smart_left"], 2);
}

#[tokio::test]
async fn an_expired_sign_in_is_said_once_then_forgotten() {
    let plex = fake_plex().await;
    let app = TestApp::with_config(|c| c.plex_tv = plex.clone()).await;
    let admin = app.setup_admin().await;
    // The first PIN, then a second one, which Plex lets expire.
    for _ in 0..2 {
        let r = app.post("/api/import/plex/pin", &admin, json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    }
    let r = app.get("/api/import/plex/pin", &admin).await;
    assert_eq!((r.status, r.body["code"].as_str()), (StatusCode::CONFLICT, Some("plex_pin_expired")));
    // plex.tv isn't asked again: signing in starts over.
    let r = app.get("/api/import/plex/pin", &admin).await;
    assert_eq!((r.status, r.body["code"].as_str()), (StatusCode::CONFLICT, Some("plex_signed_out")));
}
