//! Movie and show metadata from TMDB, against a local stand-in that answers
//! like TMDB's API for a handful of titles. Nothing leaves the machine: the
//! answers name no pictures except the one a test stores beforehand.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use serde_json::{Value, json};

use common::{TestApp, encode, titles};

const KEY: &str = "test-key";

#[derive(Clone, Default)]
struct Tmdb {
    /// "/search/movie?query=Alien&…" for every request.
    requests: Arc<Mutex<Vec<String>>>,
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

fn answer(path: &str, query: &[(String, String)]) -> Option<Value> {
    let param = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    Some(match path {
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
    match answer(&path, &query) {
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
    let (mock, base) = start_tmdb().await;
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
    })
    .await;
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
    assert_eq!(bad.body["ok"], json!(false));
    assert!(bad.body["message"].as_str().unwrap().starts_with("TMDB rejected this key."), "{:?}", bad.body);
    // The key never shows up in what the test says.
    assert!(!bad.mentions("nope"));
    let none = app.post("/api/settings/test", &admin, json!({"service": "tmdb"})).await;
    assert_eq!(none.body["message"], json!("Enter a TMDB API key first."));
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
    assert_eq!(titles(&collections.body["collections"]), ["Alien Collection"]);
    assert_eq!(collections.body["collections"][0]["item_count"], json!(2));

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
