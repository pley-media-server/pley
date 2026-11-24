//! Movie and show metadata from TMDB, against a local stand-in that answers
//! like TMDB's API for a handful of titles. Nothing leaves the machine: the
//! answers name no pictures except the one a test stores beforehand.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use pley_transcoder_api::{
    API_VERSION, Capabilities, MediaInfo, MediaStream, ProbeRequest, StreamKind, TranscoderInfo,
};
use serde_json::{Value, json};

use common::{TestApp, encode, titles};

const KEY: &str = "test-key";

#[derive(Clone, Default)]
struct Tmdb {
    /// "/search/movie?query=Alien&…" for every request.
    requests: Arc<Mutex<Vec<String>>>,
    /// TMDB has since added a film and an episode it didn't have.
    added: Arc<AtomicBool>,
    /// Every request fails, as when TMDB is down.
    down: Arc<AtomicBool>,
}

impl Tmdb {
    fn requested(&self, prefix: &str) -> Vec<String> {
        self.requests.lock().iter().filter(|r| r.starts_with(prefix)).cloned().collect()
    }
}

fn movie(id: i64) -> Option<Value> {
    let (title, date, runtime, imdb, collection) = match id {
        348 => ("Alien", "1979-05-25", 117, "tt0078748", true),
        679 => ("Aliens", "1986-07-18", 137, "tt0090605", true),
        8077 => ("Alien³", "1992-05-22", 114, "tt0103644", true),
        99_999 => ("Alien Nation", "1988-10-07", 91, "tt0094631", false),
        4497 => ("Br\u{fc}no", "2009-07-10", 81, "tt0889583", false),
        77 => ("Nothing Like It", "2001-03-03", 95, "tt0000077", false),
        _ => return None,
    };
    Some(json!({
        "title": title,
        "original_title": title,
        "overview": format!("The story of {title}."),
        "tagline": if id == 348 { "In space no one can hear you scream." } else { "" },
        "vote_average": if id == 348 { 8.1 } else { 7.0 },
        "release_date": date,
        "runtime": runtime,
        "genres": [{"name": "Horror"}, {"name": "Science Fiction"}],
        "production_companies": [{"name": "Brandywine Productions"}, {"name": "20th Century Fox"}],
        "poster_path": null,
        "backdrop_path": null,
        "imdb_id": imdb,
        "release_dates": {"results": [
            {"iso_3166_1": "GB", "release_dates": [{"certification": ""}, {"certification": "15"}]},
            {"iso_3166_1": "US", "release_dates": [{"certification": "R"}]}
        ]},
        "credits": {
            "cast": [{"id": 10205, "name": "Sigourney Weaver", "character": "Ripley", "profile_path": null, "order": 0}],
            "crew": [{"id": 578, "name": "Ridley Scott", "job": "Director", "department": "Directing", "profile_path": null}]
        },
        "belongs_to_collection": if collection {
            json!({"id": 8091, "name": "Alien Collection", "poster_path": null, "backdrop_path": null})
        } else {
            Value::Null
        }
    }))
}

fn search_result(id: i64, title: &str, date: &str, popularity: f64) -> Value {
    json!({"id": id, "title": title, "original_title": title, "release_date": date, "popularity": popularity,
           "overview": format!("About {title}."), "poster_path": null})
}

/// TMDB's answer, `added` once it has the film and the episode it lacked.
fn answer(path: &str, query: &[(String, String)], added: bool) -> Option<Value> {
    let param = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let episode = |number: i64, name: &str| {
        json!({"episode_number": number, "name": name, "overview": format!("{name}, told."), "air_date": "2019-05-01",
               "still_path": null, "vote_average": 7.0, "crew": [], "guest_stars": []})
    };
    Some(match path {
        "/search/movie" if added && param("query") == Some("Nothing Like It") => {
            json!({"results": [search_result(77, "Nothing Like It", "2001-03-03", 5.0)]})
        }
        "/search/tv" if param("query") == Some("Harbor Lights") => json!({"results": [
            {"id": 9001, "name": "Harbor Lights", "original_name": "Harbor Lights", "first_air_date": "2019-05-01",
             "popularity": 10.0}
        ]}),
        "/tv/9001" => json!({
            "name": "Harbor Lights", "original_name": "Harbor Lights", "overview": "A lighthouse town.",
            "tagline": "", "vote_average": 7.5, "first_air_date": "2019-05-01", "genres": [], "networks": [],
            "poster_path": null, "backdrop_path": null, "external_ids": {"imdb_id": null, "tvdb_id": null},
            "content_ratings": {"results": []}, "aggregate_credits": {"cast": []}, "created_by": []
        }),
        "/tv/9001/season/1" => {
            let mut episodes = vec![episode(1, "Arrival at Dusk"), episode(2, "The Lantern"), episode(3, "Low Tide")];
            if added {
                episodes.push(episode(4, "Fog Bank"));
            }
            json!({"name": "Season 1", "overview": "", "poster_path": null, "air_date": "2019-05-01",
                   "episodes": episodes})
        }
        "/configuration" => json!({"images": {"secure_base_url": "https://image.tmdb.org/t/p/"}}),
        "/search/movie" => json!({"results": match param("query") {
            // The most popular result is not the right one.
            Some("Alien") => vec![
                search_result(99_999, "Alien Nation", "1988-10-07", 90.0),
                search_result(8077, "Alien³", "1992-05-22", 60.0),
                search_result(348, "Alien", "1979-05-25", 40.0),
            ],
            Some("Aliens") => vec![search_result(679, "Aliens", "1986-07-18", 30.0)],
            Some("Alien Nation") => vec![search_result(99_999, "Alien Nation", "1988-10-07", 90.0)],
            // TMDB finds titles written composed, "\u{fc}" one character, and
            // nothing for "u" and an accent apart.
            Some("Br\u{fc}no") => vec![search_result(4497, "Br\u{fc}no", "2009-07-10", 20.0)],
            _ => vec![],
        }}),
        "/search/tv" => json!({"results": match param("query") {
            Some("Lost") => vec![json!({"id": 4607, "name": "Lost", "original_name": "Lost", "first_air_date": "2004-09-22", "popularity": 80.0})],
            _ => vec![],
        }}),
        "/find/tt0078748" => {
            json!({"movie_results": [search_result(348, "Alien", "1979-05-25", 40.0)], "tv_results": []})
        }
        // Only Aliens has credits of its own (Alien's answer 404).
        "/movie/679/credits" => json!({
            "cast": [{"id": 5, "name": "Carrie Henn", "character": "Newt", "profile_path": null, "order": 0}],
            "crew": []
        }),
        "/movie/348/images" => json!({
            "posters": [
                {"file_path": "/fr.jpg", "width": 1000, "height": 1500, "iso_639_1": "fr", "vote_average": 9.0},
                {"file_path": "/en-low.jpg", "width": 1000, "height": 1500, "iso_639_1": "en", "vote_average": 5.0},
                {"file_path": "/none.jpg", "width": 1000, "height": 1500, "iso_639_1": null, "vote_average": 8.0},
                {"file_path": "/en-high.jpg", "width": 2000, "height": 3000, "iso_639_1": "en", "vote_average": 7.0}
            ],
            "backdrops": []
        }),
        "/tv/4607" => json!({
            "name": "Lost",
            "original_name": "Lost",
            "overview": "Survivors of a plane crash.",
            "tagline": "",
            "vote_average": 8.0,
            "first_air_date": "2004-09-22",
            "genres": [{"name": "Mystery"}],
            "networks": [{"name": "ABC"}],
            "poster_path": null,
            "backdrop_path": null,
            "external_ids": {"imdb_id": "tt0411008", "tvdb_id": 73739},
            "content_ratings": {"results": [{"iso_3166_1": "US", "rating": "TV-14"}]},
            "aggregate_credits": {"cast": [{"id": 1, "name": "Matthew Fox", "profile_path": null,
                                            "roles": [{"character": "Jack Shephard", "episode_count": 2}], "order": 0}]},
            "created_by": [{"id": 2, "name": "J. J. Abrams", "profile_path": null}]
        }),
        "/tv/4607/season/1" => json!({
            "name": "Season 1",
            "overview": "The crash.",
            "poster_path": null,
            "air_date": "2004-09-22",
            "episodes": [
                {"episode_number": 1, "name": "Pilot (1)", "overview": "Stranded.", "air_date": "2004-09-22",
                 "still_path": null, "vote_average": 8.5,
                 "crew": [{"id": 3, "name": "Jack Bender", "job": "Director", "department": "Directing", "profile_path": null}],
                 "guest_stars": [{"id": 4, "name": "Greg Grunberg", "character": "Pilot", "profile_path": null, "order": 0}]},
                {"episode_number": 2, "name": "", "overview": "", "air_date": "2004-09-29", "still_path": null,
                 "vote_average": 0.0, "crew": [], "guest_stars": []},
                {"episode_number": 3, "name": "Tabula Rasa", "overview": "Kate's past.", "air_date": "2004-10-06",
                 "still_path": null, "vote_average": 7.9, "crew": [], "guest_stars": []}
            ]
        }),
        path => {
            let id = path.strip_prefix("/movie/")?.parse().ok()?;
            movie(id)?
        }
    })
}

async fn tmdb(State(mock): State<Tmdb>, uri: Uri, Query(query): Query<Vec<(String, String)>>) -> Response {
    let path = uri.path().trim_start_matches("/tmdb").to_owned();
    mock.requests.lock().push(format!("{path}?{}", uri.query().unwrap_or_default()));
    if !query.iter().any(|(k, v)| k == "api_key" && v == KEY) {
        return (StatusCode::UNAUTHORIZED, axum::Json(json!({"status_code": 7}))).into_response();
    }
    if mock.down.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    match answer(&path, &query, mock.added.load(Ordering::SeqCst)) {
        Some(body) => axum::Json(body).into_response(),
        None => (StatusCode::NOT_FOUND, axum::Json(json!({"status_code": 34}))).into_response(),
    }
}

async fn start_tmdb() -> (Tmdb, String) {
    let mock = Tmdb::default();
    let app = Router::new().fallback(tmdb).with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (mock, format!("http://{addr}"))
}

/// pley with TMDB at the stand-in (the other providers too; they are never
/// asked here) and a movie library: Alien (named with its IMDb id), Aliens,
/// and a film TMDB doesn't know.
async fn world() -> (TestApp, Tmdb, String, i64) {
    world_with_transcoder(None).await
}

/// [`world`], its files probed by the transcoder at `transcoder` when given.
async fn world_with_transcoder(transcoder: Option<String>) -> (TestApp, Tmdb, String, i64) {
    let (mock, base) = start_tmdb().await;
    let has_transcoder = transcoder.is_some();
    let app = TestApp::with_config(|c| {
        c.metadata_endpoints = Some(pley::providers::Endpoints {
            tvmaze: format!("{base}/tvmaze"),
            tvdb: format!("{base}/tvdb"),
            musicbrainz: format!("{base}/mb"),
            cover_art: format!("{base}/caa"),
            tmdb: format!("{base}/tmdb"),
            tmdb_images: format!("{base}/tmdb-images"),
            opensubtitles: format!("{base}/opensubtitles"),
            audnexus: format!("{base}/audnexus"),
            openlibrary: format!("{base}/ol"),
            openlibrary_covers: format!("{base}/olcovers"),
        });
        if let Some(url) = transcoder {
            c.transcoder_url = Some(url);
        }
    })
    .await;
    if has_transcoder {
        app.state.transcoder.start();
        assert!(app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    }
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979) [tt0078748].mkv", 1024);
    app.media_file("movies/Aliens (1986).mkv", 1024);
    app.media_file("movies/Nothing Like It (2001).mkv", 1024);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    (app, mock, admin, lib)
}

async fn use_key(app: &TestApp, admin: &str, key: &str) {
    let reply = app.put("/api/settings", admin, json!({"tmdb_api_key": key})).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    app.wait_for_scans().await;
}

/// Waits until the item (refreshed in the background) satisfies `done`.
async fn refreshed(app: &TestApp, admin: &str, id: i64, done: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..200 {
        let item = app.get(&format!("/api/items/{id}"), admin).await.body;
        if done(&item) {
            return item;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("item {id} was not refreshed");
}

#[tokio::test]
async fn settings_test_a_tmdb_key_before_saving_it() {
    let (app, mock, admin, _) = world().await;
    let test = |key: &str| app.post("/api/settings/test", &admin, json!({"service": "tmdb", "api_key": key}));
    let good = test(KEY).await;
    assert_eq!((good.body["ok"].clone(), good.body["message"].clone()), (json!(true), json!("TMDB accepted the key.")));
    let bad = test("nope").await;
    assert_eq!((&bad.body["ok"], &bad.body["refused"]), (&json!(false), &json!(true)));
    assert!(bad.body["message"].as_str().unwrap().starts_with("TMDB rejected this key."), "{:?}", bad.body);
    // The key never shows up in what the test says.
    assert!(!bad.mentions("nope"));
    let none = app.post("/api/settings/test", &admin, json!({"service": "tmdb"})).await;
    assert_eq!(none.body["message"], json!("Enter a TMDB API key first."));
    assert_eq!(none.body["refused"], json!(false), "not TMDB's answer");
    // Testing saves nothing.
    assert_eq!(app.get("/api/settings", &admin).await.body["tmdb_api_key"], json!(""));
    assert_eq!(mock.requests.lock().iter().filter(|r| r.starts_with("/configuration")).count(), 2);
}

#[tokio::test]
async fn movies_are_matched_and_described() {
    let (app, tmdb, admin, lib) = world().await;
    assert!(tmdb.requests.lock().is_empty(), "no key, no requests");
    // One of the films is unknown to TMDB: that must not hold up the others
    // (the scan waits at most 10 s here).
    use_key(&app, &admin, KEY).await;
    let ids = app.ids(lib, &admin).await;

    let alien = app.get(&format!("/api/items/{}", ids["Alien"]), &admin).await.body;
    assert_eq!(alien["summary"], json!("The story of Alien."));
    assert_eq!(alien["tagline"], json!("In space no one can hear you scream."));
    assert_eq!((alien["rating"].clone(), alien["content_rating"].clone()), (json!(8.1), json!("R")));
    assert_eq!(alien["genres"], json!(["Horror", "Science Fiction"]));
    assert_eq!(alien["studio"], json!("Brandywine Productions"), "the first company");
    assert_eq!(
        (alien["release_date"].clone(), alien["duration_ms"].clone()),
        (json!("1979-05-25"), json!(117 * 60_000))
    );
    assert_eq!((alien["tmdb_id"].clone(), alien["imdb_id"].clone()), (json!(348), json!("tt0078748")));
    assert_eq!(alien["original_title"], Value::Null, "the same as the title");
    assert_eq!(
        (alien["cast"][0]["name"].clone(), alien["cast"][0]["character"].clone()),
        (json!("Sigourney Weaver"), json!("Ripley"))
    );
    assert_eq!(alien["crew"][0]["role"], json!("director"));

    // Alien was found by its IMDb id, Aliens by title and year.
    assert_eq!(tmdb.requested("/find/tt0078748").len(), 1);
    let searches = tmdb.requested("/search/movie");
    assert!(searches.iter().any(|s| s.contains("query=Aliens") && s.contains("year=1986")), "{searches:?}");
    assert!(searches.iter().all(|s| !s.contains("query=Alien&")), "Alien needed no search");
    let all = tmdb.requests.lock().clone();
    assert!(all.iter().all(|r| r.contains("api_key=test-key") && r.contains("language=en-US")), "{all:?}");

    // Both films are in the TMDB collection.
    let collections = app.get(&format!("/api/libraries/{lib}/collections"), &admin).await;
    let collections = common::own_collections(&collections.body);
    assert_eq!(titles(&collections), ["Alien Collection"]);
    assert_eq!(collections[0]["item_count"], json!(2));

    // A film TMDB doesn't know is not looked up again by every scan; a
    // forced scan tries again.
    let unknown = app.get(&format!("/api/items/{}", ids["Nothing Like It"]), &admin).await.body;
    assert_eq!(unknown["summary"], Value::Null);
    let lookups = || tmdb.requested("/search/movie?query=Nothing").len();
    assert_eq!(lookups(), 2, "with its year, then without");
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({})).await;
    app.wait_for_scans().await;
    assert_eq!(lookups(), 2);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    assert_eq!(lookups(), 4);
}

/// A library can leave its collections to another tool: turned off, Pley's
/// from TMDB go, edited ones too, and no refresh makes them; turned on, the
/// metadata is read again and they are back as TMDB has them.
#[tokio::test]
async fn a_library_can_go_without_collections_from_tmdb() {
    let (app, _tmdb, admin, lib) = world().await;
    use_key(&app, &admin, KEY).await;
    let collections = || async {
        let listed = app.get(&format!("/api/libraries/{lib}/collections"), &admin).await;
        common::own_collections(&listed.body)
    };
    let path = format!("/api/libraries/{lib}");
    let switch = |on: bool| app.patch(&path, &admin, json!({"tmdb_collections": on}));
    assert_eq!(app.get(&format!("/api/libraries/{lib}"), &admin).await.body["tmdb_collections"], json!(true));
    assert_eq!(titles(&collections().await), ["Alien Collection"]);

    let off = switch(false).await;
    assert_eq!((off.status, off.body["tmdb_collections"].clone()), (StatusCode::OK, json!(false)));
    assert_eq!(titles(&collections().await), Vec::<String>::new());
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    assert_eq!(titles(&collections().await), Vec::<String>::new(), "a refresh makes none");

    switch(true).await;
    app.wait_for_scans().await;
    let back = collections().await;
    assert_eq!(titles(&back), ["Alien Collection"]);
    assert_eq!(back[0]["item_count"], json!(2));

    // One an administrator edited goes too, and comes back as TMDB has it.
    let id = back[0]["id"].as_i64().unwrap();
    let edited = app.patch(&format!("/api/collections/{id}"), &admin, json!({"summary": "Ours."})).await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.body);
    switch(false).await;
    assert_eq!(titles(&collections().await), Vec::<String>::new());
    switch(true).await;
    app.wait_for_scans().await;
    let remade = collections().await;
    assert_eq!(titles(&remade), ["Alien Collection"]);
    assert_ne!(remade[0]["summary"], json!("Ours."));

    // A library can be made without them.
    app.media_file("more/Alien³ (1992).mkv", 1024);
    let made = app
        .post(
            "/api/libraries",
            &admin,
            json!({"name": "More", "kind": "movies", "paths": [app.media_path("more")], "tmdb_collections": false}),
        )
        .await;
    assert_eq!((made.status, made.body["tmdb_collections"].clone()), (StatusCode::CREATED, json!(false)));
    app.wait_for_scans().await;
    let more = made.body["id"].as_i64().unwrap();
    let listed = app.get(&format!("/api/libraries/{more}/collections"), &admin).await;
    assert_eq!(titles(&common::own_collections(&listed.body)), Vec::<String>::new());
}

/// The probes [`start_transcoder`]'s stand-in was asked for: those of files
/// in `held/`, which wait while `open` is false, and the others.
#[derive(Clone)]
struct Probes {
    others: Arc<AtomicUsize>,
    held: Arc<AtomicUsize>,
    open: Arc<tokio::sync::watch::Sender<bool>>,
}

/// A transcoder that probes every file as a 1080p H.264 video, counted in
/// `probes`.
async fn start_transcoder(probes: Probes) -> String {
    async fn probe(State(probes): State<Probes>, Json(req): Json<ProbeRequest>) -> Json<MediaInfo> {
        if req.path.replace('\\', "/").contains("/held/") {
            probes.held.fetch_add(1, Ordering::SeqCst);
            let _ = probes.open.subscribe().wait_for(|open| *open).await;
        } else {
            probes.others.fetch_add(1, Ordering::SeqCst);
        }
        Json(MediaInfo {
            container: Some("matroska".into()),
            duration_ms: Some(60_000),
            streams: vec![MediaStream {
                index: 0,
                kind: StreamKind::Video,
                codec: "h264".into(),
                width: Some(1920),
                height: Some(1080),
                default: true,
                ..Default::default()
            }],
            ..Default::default()
        })
    }
    let info = || async {
        Json(TranscoderInfo {
            api_version: API_VERSION,
            capabilities: Capabilities { ffmpeg_available: true, ..Default::default() },
            ..Default::default()
        })
    };
    let app = Router::new().route("/v1/info", get(info)).route("/v1/probe", post(probe)).with_state(probes);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// Turned back on while a scan of the library waits behind another
/// library's, the switch joins that scan: the films are fetched again and
/// their collections come back, but no file is probed again.
#[tokio::test]
async fn collections_from_tmdb_turned_on_beside_a_waiting_scan_probe_no_file() {
    let probes =
        Probes { others: Arc::default(), held: Arc::default(), open: Arc::new(tokio::sync::watch::channel(true).0) };
    let transcoder = start_transcoder(probes.clone()).await;
    let (app, tmdb, admin, lib) = world_with_transcoder(Some(transcoder)).await;
    use_key(&app, &admin, KEY).await;
    let collections = || async {
        let listed = app.get(&format!("/api/libraries/{lib}/collections"), &admin).await;
        titles(&common::own_collections(&listed.body))
    };
    let path = format!("/api/libraries/{lib}");
    let switch = |on: bool| app.patch(&path, &admin, json!({"tmdb_collections": on}));
    assert_eq!(collections().await, ["Alien Collection"]);
    assert_eq!(switch(false).await.status, StatusCode::OK);
    assert_eq!(collections().await, Vec::<String>::new());
    app.media_file("held/Held (2020).mkv", 1024);
    let held = app.library(&admin, "Held", "movies", "held").await;

    // The scanner probes the other library's file again, held there, and
    // a scan of this one waits.
    probes.open.send_replace(false);
    let before = probes.held.load(Ordering::SeqCst);
    app.state.scanner.enqueue(held, true);
    for _ in 0..500 {
        if probes.held.load(Ordering::SeqCst) > before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(probes.held.load(Ordering::SeqCst) > before, "the other library's scan is under way");
    app.state.scanner.enqueue(lib, false);
    let probed = probes.others.load(Ordering::SeqCst);
    let fetched = |id: i64| tmdb.requested(&format!("/movie/{id}?")).len();
    let (alien, aliens) = (fetched(348), fetched(679));
    assert_eq!(switch(true).await.status, StatusCode::OK);
    probes.open.send_replace(true);
    app.wait_for_scans().await;

    assert_eq!(probes.others.load(Ordering::SeqCst), probed, "no file of the library is probed again");
    assert!(fetched(348) > alien && fetched(679) > aliens, "its films are fetched again");
    assert_eq!(collections().await, ["Alien Collection"]);
}

/// A Mac's disks give a name's accents apart from their letters ("u" and a
/// diaeresis), and names copied from one keep them so. The film is found
/// under the name it was scanned with: it stays the item it was.
#[tokio::test]
async fn a_name_with_its_accents_apart_is_found_on_tmdb() {
    let (app, tmdb, admin, lib) = world().await;
    app.media_file("movies/Bru\u{308}no (2009)/Bru\u{308}no (2009).mkv", 1024);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({})).await;
    app.wait_for_scans().await;
    let scanned = app.ids(lib, &admin).await["Bru\u{308}no"];
    use_key(&app, &admin, KEY).await;

    let ids = app.ids(lib, &admin).await;
    assert_eq!(ids["Br\u{fc}no"], scanned, "{ids:?}");
    let bruno = app.get(&format!("/api/items/{scanned}"), &admin).await.body;
    assert_eq!(bruno["tmdb_id"], json!(4497));
    let searches = tmdb.requested("/search/movie");
    assert!(searches.iter().any(|s| s.contains("query=Br%C3%BCno")), "{searches:?}");
}

#[tokio::test]
async fn certifications_follow_the_region_and_edits_survive_refreshes() {
    let (app, _tmdb, admin, lib) = world().await;
    use_key(&app, &admin, KEY).await;
    let alien = app.ids(lib, &admin).await["Alien"];
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 15})).await;
    assert_eq!(app.get(&format!("/api/items/{alien}"), &kid).await.status, StatusCode::NOT_FOUND, "R is 17");

    // British certificates for en-GB: "15" (the empty one is skipped).
    app.put("/api/settings", &admin, json!({"metadata_language": "en-GB"})).await;
    let edited =
        app.patch(&format!("/api/items/{alien}"), &admin, json!({"title": "My Alien", "summary": "Mine."})).await;
    assert_eq!(edited.status, StatusCode::OK);
    assert_eq!(app.post(&format!("/api/items/{alien}/refresh"), &admin, json!({})).await.status, StatusCode::ACCEPTED);
    // The refresh writes TMDB's values, then the hand edits again: wait for both.
    let item = refreshed(&app, &admin, alien, |i| {
        i["content_rating"] == "15" && i["title"] == "My Alien" && i["summary"] == "Mine."
    })
    .await;
    assert_eq!(item["sort_title"], json!("my alien"));
    assert_eq!(app.get(&format!("/api/items/{alien}"), &kid).await.status, StatusCode::OK, "15 is allowed");

    // Unlocking the title hands it back to TMDB (the PATCH starts a refresh).
    app.patch(&format!("/api/items/{alien}"), &admin, json!({"title": null})).await;
    let item = refreshed(&app, &admin, alien, |i| i["title"] == "Alien" && i["summary"] == "Mine.").await;
    assert_eq!(item["locked_fields"], json!(["summary"]));
}

#[tokio::test]
async fn fix_match_and_artwork_choices() {
    let (app, tmdb, admin, lib) = world().await;
    use_key(&app, &admin, KEY).await;
    let alien = app.ids(lib, &admin).await["Alien"];

    // The best match first, whatever TMDB's order.
    let matches = app.get(&format!("/api/items/{alien}/matches"), &admin).await;
    assert_eq!(matches.status, StatusCode::OK, "{:?}", matches.body);
    let found: Vec<(i64, i64)> = matches
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["tmdb_id"].as_i64().unwrap(), m["year"].as_i64().unwrap()))
        .collect();
    assert_eq!(found, [(348, 1979), (99_999, 1988), (8077, 1992)]);
    // Another title drops the year (it was the old title's).
    let other = app.get(&format!("/api/items/{alien}/matches?title={}", encode("Alien Nation")), &admin).await;
    assert_eq!(other.body[0]["tmdb_id"], json!(99_999));
    let last = tmdb.requested("/search/movie").pop().unwrap();
    assert!(last.contains("Nation") && !last.contains("year="), "{last}");

    // Matching it to another entry refetches everything.
    let rematched = app.post(&format!("/api/items/{alien}/match"), &admin, json!({"tmdb_id": 8077})).await;
    assert_eq!(rematched.status, StatusCode::OK, "{:?}", rematched.body);
    assert_eq!(
        (rematched.body["title"].clone(), rematched.body["imdb_id"].clone()),
        (json!("Alien³"), json!("tt0103644"))
    );
    let back = app.post(&format!("/api/items/{alien}/match"), &admin, json!({"tmdb_id": 348})).await;
    assert_eq!(back.body["title"], json!("Alien"));

    // Posters in the metadata language first, then without language, then the rest; best voted first.
    let posters = app.get(&format!("/api/items/{alien}/artwork/poster"), &admin).await;
    let urls: Vec<&str> = posters.body.as_array().unwrap().iter().map(|p| p["url"].as_str().unwrap()).collect();
    let tmdb_image = |size: &str, name: &str| format!("https://image.tmdb.org/t/p/{size}/{name}");
    assert_eq!(
        urls,
        [
            tmdb_image("original", "en-high.jpg"),
            tmdb_image("original", "en-low.jpg"),
            tmdb_image("original", "none.jpg"),
            tmdb_image("original", "fr.jpg")
        ]
    );
    assert_eq!(posters.body[0]["preview_url"], json!(tmdb_image("w342", "en-high.jpg")));

    // Pictures come from the image hosts only; these are already stored, as
    // if downloaded.
    let elsewhere = app
        .put(&format!("/api/items/{alien}/artwork/poster"), &admin, json!({"url": "https://example.com/x.jpg"}))
        .await;
    assert_eq!(elsewhere.status, StatusCode::BAD_REQUEST);
    let downloaded = |url: String, bytes: Vec<u8>| {
        let app = &app;
        async move {
            let blob = app.state.store.put(&bytes, "png").await.unwrap();
            sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES (?, ?, 0)")
                .bind(&url)
                .bind(&blob)
                .execute(&app.state.db.write)
                .await
                .unwrap();
        }
    };
    let chosen = tmdb_image("original", "en-high.jpg");
    downloaded(chosen.clone(), png()).await;
    let picked = app.put(&format!("/api/items/{alien}/artwork/poster"), &admin, json!({"url": chosen})).await;
    assert_eq!(picked.status, StatusCode::OK, "{:?}", picked.body);
    assert!(picked.body["locked_fields"].as_array().unwrap().contains(&json!("poster")));
    // Re-encoded as an upload is: nothing of the file is served as it came.
    let poster = app.get(&format!("/api/items/{alien}/images/poster"), &admin).await;
    assert_eq!(image::guess_format(&poster.bytes).unwrap(), image::ImageFormat::Jpeg);

    // And from the hosts library tools take theirs from, such as GitHub's
    // raw files for Kometa's own, which serve anyone's files: one that only
    // starts like a picture is refused.
    let kometa = "https://raw.githubusercontent.com/team/images/master/poster.jpg";
    downloaded(kometa.into(), b"\xff\xd8\xff\xe0 not a picture".to_vec()).await;
    let picked = app.put(&format!("/api/items/{alien}/artwork/poster"), &admin, json!({"url": kometa})).await;
    assert_eq!(picked.status, StatusCode::BAD_REQUEST, "{:?}", picked.body);
    let kometa = "https://raw.githubusercontent.com/team/images/master/other.jpg";
    downloaded(kometa.into(), png()).await;
    let picked = app.put(&format!("/api/items/{alien}/artwork/poster"), &admin, json!({"url": kometa})).await;
    assert_eq!(picked.status, StatusCode::OK, "{:?}", picked.body);
}

fn png() -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(40, 60, image::Rgb([200, 30, 30]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

#[tokio::test]
async fn a_rejected_key_leaves_everything_to_try_again() {
    let (app, tmdb, admin, lib) = world().await;
    use_key(&app, &admin, "wrong").await;
    assert!(!tmdb.requests.lock().is_empty(), "TMDB was asked");
    let ids = app.ids(lib, &admin).await;
    let alien = app.get(&format!("/api/items/{}", ids["Alien"]), &admin).await.body;
    assert_eq!(alien["summary"], Value::Null);

    use_key(&app, &admin, KEY).await;
    for title in ["Alien", "Aliens"] {
        let item = app.get(&format!("/api/items/{}", ids[title]), &admin).await.body;
        assert_eq!(item["summary"], json!(format!("The story of {title}.")), "{title}");
    }
}

#[tokio::test]
async fn credits_are_filled_in_for_movies_matched_without_them() {
    let (app, tmdb, admin, lib) = world().await;
    use_key(&app, &admin, KEY).await;
    let ids = app.ids(lib, &admin).await;
    // As if matched by a version that stored no credits.
    sqlx::query("UPDATE items SET credits_at = NULL").execute(&app.state.db.write).await.unwrap();
    sqlx::query("DELETE FROM item_people").execute(&app.state.db.write).await.unwrap();

    let backfill = app.state.metadata.backfill_credits(None);
    tokio::time::timeout(Duration::from_secs(10), backfill).await.expect("the backfill is not held up");
    let aliens = app.get(&format!("/api/items/{}", ids["Aliens"]), &admin).await.body;
    assert_eq!(
        (aliens["cast"][0]["name"].clone(), aliens["cast"][0]["character"].clone()),
        (json!("Carrie Henn"), json!("Newt"))
    );
    let alien = app.get(&format!("/api/items/{}", ids["Alien"]), &admin).await.body;
    assert_eq!(alien["cast"], json!([]));

    // Neither is asked for again: Alien has none to give.
    let backfill = app.state.metadata.backfill_credits(None);
    tokio::time::timeout(Duration::from_secs(10), backfill).await.unwrap();
    assert_eq!((tmdb.requested("/movie/348/credits").len(), tmdb.requested("/movie/679/credits").len()), (1, 1));
}

#[tokio::test]
async fn shows_seasons_and_episodes_from_tmdb() {
    let (app, tmdb, admin, _) = world().await;
    // With a TMDB key, TMDB is the automatic TV source.
    use_key(&app, &admin, KEY).await;
    app.media_file("tv/Lost (2004)/Season 01/Lost - S01E01.mkv", 1024);
    app.media_file("tv/Lost (2004)/Season 01/Lost - S01E02.mkv", 1024);
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let searches = tmdb.requested("/search/tv?query=Lost");
    assert_eq!(searches.len(), 1, "{searches:?}");
    assert!(searches[0].contains("first_air_date_year=2004"), "{searches:?}");

    let show = app.ids(tv, &admin).await["Lost"];
    let detail = app.get(&format!("/api/items/{show}"), &admin).await.body;
    assert_eq!(detail["summary"], json!("Survivors of a plane crash."));
    assert_eq!((detail["studio"].clone(), detail["content_rating"].clone()), (json!("ABC"), json!("TV-14")));
    assert_eq!((detail["imdb_id"].clone(), detail["tmdb_id"].clone()), (json!("tt0411008"), json!(4607)));
    assert_eq!(detail["cast"][0]["character"], json!("Jack Shephard"));
    assert_eq!(
        (detail["crew"][0]["name"].clone(), detail["crew"][0]["role"].clone()),
        (json!("J. J. Abrams"), json!("creator"))
    );

    let season = app.children(show, &admin).await["Season 1"];
    let season_detail = app.get(&format!("/api/items/{season}"), &admin).await.body;
    assert_eq!(season_detail["summary"], json!("The crash."));
    let episodes = app.get(&format!("/api/items/{season}/children"), &admin).await;
    assert_eq!(titles(&episodes.body["items"]), ["Pilot (1)", "Episode 2"], "a missing name gets a number");
    let pilot = episodes.body["items"][0]["id"].as_i64().unwrap();
    let pilot = app.get(&format!("/api/items/{pilot}"), &admin).await.body;
    assert_eq!((pilot["summary"].clone(), pilot["release_date"].clone()), (json!("Stranded."), json!("2004-09-22")));
    assert_eq!(pilot["cast"][0]["name"], json!("Greg Grunberg"), "guest stars");
    assert_eq!(pilot["crew"][0]["role"], json!("director"));

    // The show's rating covers its episodes.
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 12})).await;
    assert_eq!(
        app.get(&format!("/api/items/{}", episodes.body["items"][0]["id"]), &kid).await.status,
        StatusCode::NOT_FOUND
    );
}

/// An episode whose file name gives no number is numbered by its title, and
/// keeps that number through rescans.
#[tokio::test]
async fn an_episode_without_a_number_is_found_by_its_title() {
    let (app, _, admin, _) = world().await;
    use_key(&app, &admin, KEY).await;
    app.media_file("tv/Lost (2004)/Season 01/Lost - S01E01.mkv", 1024);
    app.media_file("tv/Lost (2004)/Season 01/Lost - Tabula Rasa.mkv", 1024);
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let season = app.children(app.ids(tv, &admin).await["Lost"], &admin).await["Season 1"];
    let listed = || async {
        let episodes = app.get(&format!("/api/items/{season}/children"), &admin).await.body;
        episodes["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["index"].clone(), e["title"].clone()))
            .collect::<Vec<_>>()
    };
    let numbered = vec![(json!(1), json!("Pilot (1)")), (json!(3), json!("Tabula Rasa"))];
    assert_eq!(listed().await, numbered);
    let tabula = app.children(season, &admin).await["Tabula Rasa"];
    let detail = app.get(&format!("/api/items/{tabula}"), &admin).await.body;
    assert_eq!(detail["summary"], json!("Kate's past."));

    let rescan = app.post(&format!("/api/libraries/{tv}/scan"), &admin, json!({})).await;
    assert_eq!(rescan.status, StatusCode::ACCEPTED);
    app.wait_for_scans().await;
    assert_eq!(listed().await, numbered);
}

async fn rescan(app: &TestApp, admin: &str, lib: i64) {
    let reply = app.post(&format!("/api/libraries/{lib}/scan"), admin, json!({})).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    app.wait_for_scans().await;
}

/// Moves what the database knows of `items` (an SQL condition) `hours`
/// back in time: when they were added and last looked up.
async fn hours_ago(app: &TestApp, items: &str, hours: i64) {
    let sql = format!(
        "UPDATE items SET added_at = added_at - {seconds},
             metadata_at = CASE WHEN metadata_at < 0 THEN metadata_at + {seconds}
                                WHEN metadata_at > 0 THEN metadata_at - {seconds} END
         WHERE {items}",
        seconds = hours * 3600
    );
    sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&app.state.db.write).await.unwrap();
}

/// A film TMDB found nothing for is looked up again a day later, then less
/// and less often, so that one it adds later is found without a Refresh.
/// While TMDB doesn't answer, nothing is taken for unknown: what wasn't
/// looked up is at the next scan.
#[tokio::test]
async fn a_film_found_nothing_for_is_looked_up_again_less_and_less_often() {
    let (app, tmdb, admin, lib) = world().await;
    use_key(&app, &admin, KEY).await;
    let film = app.ids(lib, &admin).await["Nothing Like It"];
    let lookups = || tmdb.requested("/search/movie?query=Nothing").len();
    assert_eq!(lookups(), 2, "with its year, then without");
    let unknown = "title = 'Nothing Like It'";
    rescan(&app, &admin, lib).await;
    assert_eq!(lookups(), 2, "not again at once");

    // A day after it was looked up, it is again; then after as long again
    // as it had been in the library by then.
    for (hours, looked_up) in [(20, false), (5, true), (20, false), (6, true), (30, false), (25, true)] {
        let before = lookups();
        hours_ago(&app, unknown, hours).await;
        rescan(&app, &admin, lib).await;
        assert_eq!(lookups() > before, looked_up, "{hours} hours later");
    }

    // TMDB has it now. A scan of where files changed leaves it to the next
    // scan of the whole library.
    tmdb.added.store(true, Ordering::SeqCst);
    hours_ago(&app, unknown, 24 * 5).await;
    let before = lookups();
    app.state.scanner.enqueue_changes(lib, vec![app.media.path().join("movies/Aliens (1986).mkv")]);
    app.wait_for_scans().await;
    assert_eq!(lookups(), before);
    rescan(&app, &admin, lib).await;
    let detail = app.get(&format!("/api/items/{film}"), &admin).await.body;
    assert_eq!(
        (detail["tmdb_id"].clone(), detail["summary"].clone()),
        (json!(77), json!("The story of Nothing Like It."))
    );

    // TMDB down: a new film isn't taken for one it doesn't know.
    tmdb.down.store(true, Ordering::SeqCst);
    app.media_file("movies/Quiet Harbor (2015).mkv", 1024);
    rescan(&app, &admin, lib).await;
    let stored = || async {
        sqlx::query_scalar::<_, Option<i64>>("SELECT metadata_at FROM items WHERE title = 'Quiet Harbor'")
            .fetch_one(&app.state.db.read)
            .await
            .unwrap()
    };
    assert_eq!(stored().await, None, "not looked up yet");
    tmdb.down.store(false, Ordering::SeqCst);
    let searches = tmdb.requested("/search/movie?query=Quiet").len();
    rescan(&app, &admin, lib).await;
    assert!(tmdb.requested("/search/movie?query=Quiet").len() > searches, "looked up at the next scan");
    assert!(stored().await.is_some_and(|at| at < 0), "TMDB doesn't know it");
}

/// An episode added to a show TMDB described gets its details at the scan
/// that finds it; one TMDB doesn't have yet gets them once it does, without
/// the show being fetched again at every scan meanwhile.
#[tokio::test]
async fn episodes_added_to_a_show_get_their_details() {
    let (app, tmdb, admin, _) = world().await;
    use_key(&app, &admin, KEY).await;
    app.media_file("tv/Harbor Lights (2019)/Season 01/Harbor Lights - S01E01.mkv", 1024);
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let show = app.ids(tv, &admin).await["Harbor Lights"];
    let season = app.children(show, &admin).await["Season 1"];
    let episodes = || async {
        let listed = app.get(&format!("/api/items/{season}/children"), &admin).await.body;
        titles(&listed["items"])
    };
    assert_eq!(episodes().await, ["Arrival at Dusk"]);

    app.media_file("tv/Harbor Lights (2019)/Season 01/Harbor Lights - S01E03.mkv", 1024);
    app.media_file("tv/Harbor Lights (2019)/Season 01/Harbor Lights - S01E04.mkv", 1024);
    rescan(&app, &admin, tv).await;
    assert_eq!(episodes().await, ["Arrival at Dusk", "Low Tide", "Episode 4"]);
    // (Added before the show was fetched, as a second can't tell here.)
    sqlx::query("UPDATE items SET added_at = added_at - 10 WHERE kind = 'episode'")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let fetched = || tmdb.requested("/tv/9001/season/1").len();
    let before = fetched();
    rescan(&app, &admin, tv).await;
    assert_eq!(fetched(), before, "not fetched again at once for the episode TMDB lacks");

    tmdb.added.store(true, Ordering::SeqCst);
    hours_ago(&app, &format!("library_id = {tv}"), 26).await;
    rescan(&app, &admin, tv).await;
    assert_eq!(episodes().await, ["Arrival at Dusk", "Low Tide", "Fog Bank"]);
}

/// Copies of a film whose names give different years (TMDB corrected it
/// after the first was named) become one movie once both are matched to the
/// same title: its files are versions, and what was done with either stays.
/// A later scan keeps them one; fixing the match lets the other copy go.
#[tokio::test]
async fn copies_matched_to_the_same_title_are_one_movie() {
    let (app, _tmdb, admin, lib) = world().await;
    app.media_file("movies/Aliens (1987) 2160p.mkv", 2048);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({})).await;
    app.wait_for_scans().await;
    let aliens = || async {
        let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await.body;
        let mut found: Vec<(i64, i64)> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["title"] == "Aliens")
            .map(|i| (i["id"].as_i64().unwrap(), i["year"].as_i64().unwrap_or(0)))
            .collect();
        found.sort();
        found
    };
    // Without a key, nothing tells them apart from two films.
    let before = aliens().await;
    assert_eq!(before.iter().map(|(_, y)| *y).collect::<Vec<_>>(), [1986, 1987]);
    let (first, second) = (before[0].0, before[1].0);
    // Watched and on My List as the 4K copy, rated as the other.
    assert_eq!(
        app.post(&format!("/api/items/{second}/watched"), &admin, json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    assert!(app.put(&format!("/api/watchlist/{second}"), &admin, json!({})).await.status.is_success());
    let rated = app.put(&format!("/api/items/{first}/rating"), &admin, json!({"rating": 8.0})).await;
    assert!(rated.status.is_success(), "{:?}", rated.body);

    use_key(&app, &admin, KEY).await;
    let after = aliens().await;
    assert_eq!(after.len(), 1, "one movie: {after:?}");
    assert_eq!(after[0].0, first, "the first added stays");
    let movie = app.get(&format!("/api/items/{first}"), &admin).await.body;
    assert_eq!(movie["tmdb_id"], json!(679));
    assert_eq!(movie["files"].as_array().map(Vec::len), Some(2), "{movie}");
    assert_eq!(movie["user_state"]["watched"], json!(true), "{movie}");
    assert_eq!(movie["user_state"]["rating"], json!(8.0), "{movie}");
    assert_eq!(movie["in_watchlist"], json!(true), "{movie}");
    assert_eq!(app.get(&format!("/api/items/{second}"), &admin).await.status, StatusCode::NOT_FOUND);

    // Scanned again, the other copy written again as it changed (a forced
    // scan with no transcoder leaves unchanged files be), it stays one movie.
    app.media_file("movies/Aliens (1987) 2160p.mkv", 4096);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    assert_eq!(aliens().await.len(), 1);
    let movie = app.get(&format!("/api/items/{first}"), &admin).await.body;
    assert_eq!(movie["files"].as_array().map(Vec::len), Some(2), "{movie}");

    // Fixed to another title, the copy its name doesn't make goes back to
    // being a movie of its own, matched by its name.
    let fixed = app.post(&format!("/api/items/{first}/match"), &admin, json!({"tmdb_id": 8077})).await;
    assert_eq!(fixed.status, StatusCode::OK, "{:?}", fixed.body);
    assert_eq!(fixed.body["title"], json!("Alien³"));
    assert_eq!(fixed.body["files"].as_array().map(Vec::len), Some(1), "{:?}", fixed.body);
    let split = aliens().await;
    assert_eq!(split.len(), 1, "{split:?}");
    let other = app.get(&format!("/api/items/{}", split[0].0), &admin).await.body;
    assert_eq!((other["tmdb_id"].clone(), other["year"].clone()), (json!(679), json!(1986)), "{other}");
    assert!(other["files"][0]["path"].as_str().unwrap().contains("1987"), "{other}");
    // And it stays apart through another scan.
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    assert_eq!(app.ids(lib, &admin).await.len(), 4);
}

/// A copy that fixing a merged movie's match let go stays apart from it,
/// even when matched to the same title again (as an automatic match that
/// put two films together would do), until its own match is fixed to that
/// title: then it goes into the movie, which the fix answers with.
#[tokio::test]
async fn a_copy_let_go_stays_apart_until_its_match_is_fixed() {
    let (app, _tmdb, admin, lib) = world().await;
    app.media_file("movies/Aliens (1987) 2160p.mkv", 2048);
    use_key(&app, &admin, KEY).await;
    let aliens = || async {
        let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await.body;
        let mut found: Vec<i64> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["title"] == "Aliens")
            .map(|i| i["id"].as_i64().unwrap())
            .collect();
        found.sort();
        found
    };
    let merged = aliens().await;
    assert_eq!(merged.len(), 1, "{merged:?}");
    let movie = merged[0];

    // Fixed to the title it has, the copy its name doesn't make goes, and
    // is matched to that title again on its own, but stays a movie apart.
    let fixed = app.post(&format!("/api/items/{movie}/match"), &admin, json!({"tmdb_id": 679})).await;
    assert_eq!(fixed.status, StatusCode::OK, "{:?}", fixed.body);
    assert_eq!(fixed.body["id"], json!(movie));
    assert_eq!(fixed.body["files"].as_array().map(Vec::len), Some(1), "{:?}", fixed.body);
    let apart = aliens().await;
    assert_eq!(apart.len(), 2, "{apart:?}");
    let copy = apart[1];
    let other = app.get(&format!("/api/items/{copy}"), &admin).await.body;
    assert_eq!(other["tmdb_id"], json!(679), "{other}");
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    assert_eq!(aliens().await, apart);

    // Fixed by hand to that title, the copy goes into the movie, and the
    // fix answers with it rather than with the copy that's gone.
    let fixed = app.post(&format!("/api/items/{copy}/match"), &admin, json!({"tmdb_id": 679})).await;
    assert_eq!(fixed.status, StatusCode::OK, "{:?}", fixed.body);
    assert_eq!(fixed.body["id"], json!(movie), "{:?}", fixed.body);
    assert_eq!(fixed.body["files"].as_array().map(Vec::len), Some(2), "{:?}", fixed.body);
    assert_eq!(aliens().await, [movie]);
    assert_eq!(app.get(&format!("/api/items/{copy}"), &admin).await.status, StatusCode::NOT_FOUND);
}

/// A merged movie whose own copy went (an upgrade replaced it) keeps the
/// copy left, and what was done with it, through a fix of its match and a
/// scan that writes that copy again.
#[tokio::test]
async fn a_merged_movie_whose_own_copy_went_keeps_the_other() {
    let (app, _tmdb, admin, lib) = world().await;
    app.media_file("movies/Aliens (1987) 2160p.mkv", 2048);
    use_key(&app, &admin, KEY).await;
    let aliens = || async {
        let page = app.get(&format!("/api/libraries/{lib}/items"), &admin).await.body;
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["title"] == "Aliens")
            .map(|i| i["id"].as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    let merged = aliens().await;
    assert_eq!(merged.len(), 1, "{merged:?}");
    let movie = merged[0];
    assert_eq!(
        app.post(&format!("/api/items/{movie}/watched"), &admin, json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    std::fs::remove_file(app.media_path("movies/Aliens (1986).mkv")).unwrap();
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({})).await;
    app.wait_for_scans().await;
    assert_eq!(aliens().await, [movie]);

    let fixed = app.post(&format!("/api/items/{movie}/match"), &admin, json!({"tmdb_id": 679})).await;
    assert_eq!(fixed.status, StatusCode::OK, "{:?}", fixed.body);
    assert_eq!(fixed.body["files"].as_array().map(Vec::len), Some(1), "{:?}", fixed.body);
    // The copy left changes, so the scan writes it again.
    app.media_file("movies/Aliens (1987) 2160p.mkv", 4096);
    app.post(&format!("/api/libraries/{lib}/scan"), &admin, json!({})).await;
    app.wait_for_scans().await;
    assert_eq!(aliens().await, [movie]);
    let item = app.get(&format!("/api/items/{movie}"), &admin).await.body;
    assert_eq!(item["user_state"]["watched"], json!(true), "{item}");
    assert!(item["files"][0]["path"].as_str().unwrap().contains("1987"), "{item}");
}
