//! The Plex import against a fake plex.tv and Plex Media Server: signing in,
//! the plan (libraries paired, Home users' limits), and what the import
//! brings over, twice.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
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
                   "Guid": [{"id": "tmdb://348"}], "Media": file("/plexdata/movies/Alien (1979).mkv", 1024)}),
            json!({"ratingKey": "102", "type": "movie", "title": "Brazil", "addedAt": 1_100_000_000,
                   "Media": file("/plexdata/movies/Brazil (1985).mkv", 2048)}),
            json!({"ratingKey": "103", "type": "movie", "title": "Casablanca", "addedAt": 1_200_000_000,
                   "Media": file("/plexdata/movies/Casablanca (1942).mkv", 4096)}),
        ],
        ("2", "2") => {
            vec![json!({"ratingKey": "200", "type": "show", "title": "Show", "Guid": [{"id": "tvdb://999"}]})]
        }
        ("2", "3") => vec![json!({"ratingKey": "210", "type": "season", "title": "Season 1", "index": 1,
                                  "parentRatingKey": "200"})],
        // The second episode comes first, and Plex has its file under
        // another name: it's found by its numbers once the first is found.
        ("2", "4") => vec![
            json!({"ratingKey": "202", "type": "episode", "title": "Two", "index": 2, "parentIndex": 1,
                   "parentRatingKey": "210", "grandparentRatingKey": "200", "grandparentTitle": "Show",
                   "Media": file("/plexdata/tv/Show/S1/two.mkv", 5)}),
            json!({"ratingKey": "201", "type": "episode", "title": "One", "index": 1, "parentIndex": 1,
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
        list.retain(|i| label == "10" && i["ratingKey"] == "101");
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
            get(|headers: HeaderMap| async move {
                if token(&headers) != OWNER {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                axum::Json(json!({"id": 4, "name": "Home", "users": [
                    {"id": 1, "uuid": "u-owner", "title": "Alex", "admin": true},
                    {"id": 885, "uuid": "u-kid", "title": "Kid One", "restricted": true, "protected": true},
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
                let labels = if section == "1" { json!([{"key": "10", "title": "Scary"}]) } else { json!([]) };
                container(json!({ "Directory": labels }))
            }),
        )
        // Titles' own pages: what was edited in Plex is locked.
        .route(
            "/library/metadata/{key}",
            get(|Path(key): Path<String>| async move {
                let page = match key.as_str() {
                    "101" => json!({"ratingKey": "101", "type": "movie", "title": "Alien", "summary": "Hand-written.",
                                    "contentRating": "gb/15", "studio": "Not locked",
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
                           {"ratingKey": "101", "type": "movie", "title": "Alien"}])
                } else {
                    json!([])
                };
                container(json!({ "Metadata": list }))
            }),
        )
        .route("/devices", get(|| async { container(json!({"Device": [{"id": 5, "name": "Living Room TV"}]})) }))
        .route("/status/sessions/history/all", get(history))
        .with_state(base.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

async fn finished(app: &TestApp, admin: &str) -> Value {
    for _ in 0..400 {
        let r = app.get("/api/import/plex", admin).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        if r.body["state"] != "running" {
            return r.body;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the import didn't finish");
}

/// A number `sql` answers, with `bind` for its `?`.
async fn one(app: &TestApp, sql: &'static str, bind: i64) -> i64 {
    sqlx::query_scalar(sql).bind(bind).fetch_one(&app.state.db.read).await.unwrap()
}

#[tokio::test]
async fn brings_libraries_people_and_what_they_watched() {
    let plex = fake_plex().await;
    let app = TestApp::with_config(|c| c.plex_tv = plex.clone()).await;
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
    let r = app.post("/api/import/plex/start", &admin, choices.clone()).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    let done = finished(&app, &admin).await;
    assert_eq!(done["state"], "done", "{done:?}");
    assert!(!serde_json::to_string(&done).unwrap().contains(OWNER), "no token in the report");

    let report = &done["report"];
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

    // Titles: labels and dates added.
    let alien: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE title = 'Alien'").fetch_one(&app.state.db.read).await.unwrap();
    let brazil: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE title = 'Brazil'").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(pley::labels::for_items(&app.state.db, &[alien]).await.unwrap()[&alien], ["Scary"]);
    assert_eq!(one(&app, "SELECT added_at FROM items WHERE id = ?", alien).await, 1_000_000_000);

    // Matches and edits: Pley has no TMDB key here, so Alien starts from
    // Plex's match; locked fields become edits, an empty one left out.
    assert_eq!(report["matches_fixed"], 0);
    assert_eq!(one(&app, "SELECT tmdb_id FROM items WHERE id = ?", alien).await, 348);
    assert_eq!(report["edits"], json!({"content_rating": 1, "summary": 1, "title": 1}));
    let edited: Vec<(String, String)> =
        sqlx::query_as("SELECT field, value FROM item_overrides WHERE item_id = ? ORDER BY field")
            .bind(alien)
            .fetch_all(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!(edited, [("content_rating".into(), "15".into()), ("summary".into(), "Hand-written.".into())]);
    let season: String = sqlx::query_scalar("SELECT title FROM items WHERE kind = 'season'")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(season, "The First");

    // Collections and playlists, smart ones left out.
    assert_eq!((&report["collections_made"], &report["collection_items"]), (&json!(1), &json!(2)));
    assert_eq!(report["smart_left"], 2);
    let (space, sort): (i64, String) = sqlx::query_as("SELECT id, sort FROM collections WHERE title = 'Space'")
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
    assert_eq!(favourites, ["Episode 1", "Alien"]);

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

    // Again: nothing twice, and the same profile.
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
    assert_eq!(done["report"]["libraries"][1]["library_id"], Value::Null, "TV left out this time");
    assert_eq!(one(&app, "SELECT COUNT(*) FROM users WHERE id > ?", 0).await, users_before);
    assert_eq!(one(&app, "SELECT COUNT(*) FROM play_history WHERE user_id > ?", 0).await, 3);
    assert_eq!(done["report"]["labels_added"], 0);
    // Pley's edits stay, and collections and playlists aren't made twice.
    assert_eq!(done["report"]["edits"], json!({}));
    assert_eq!((&done["report"]["collections_made"], &done["report"]["collection_items"]), (&json!(0), &json!(0)));
    assert_eq!(one(&app, "SELECT COUNT(*) FROM playlists WHERE user_id = ?", admin_id).await, 1);
    assert_eq!(one(&app, "SELECT COUNT(*) FROM playlist_items WHERE playlist_id > ?", 0).await, 2);
    let (watched, position): (bool, i64) =
        sqlx::query_as("SELECT watched, position_ms FROM user_item_state WHERE user_id = ? AND item_id = ?")
            .bind(kid_id)
            .bind(alien)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!((watched, position), (true, 99_000), "Pley's newer resume point stays, as a rewatch");

    // Nothing but the libraries and people, when that's what was chosen.
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

    // Forgetting the sign-in.
    assert_eq!(app.delete("/api/import/plex", &admin).await.status, StatusCode::NO_CONTENT);
    let r = app.get("/api/import/plex/plan", &admin).await;
    assert_eq!(r.body["code"], "plex_signed_out");
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
