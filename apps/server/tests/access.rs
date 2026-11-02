//! Who sees what (docs/API.md, "Access rules"): a library a user may not open
//! and titles above their age limit answer 404 on every endpoint of pley's
//! API and of the Plex-compatible API, and never show up in lists, hubs,
//! search or history.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, encode, plex_titles, titles};

/// Movies, TV, music and a private library, rated, with a kid who may open
/// the first three and watch titles up to 12 years.
struct World {
    app: TestApp,
    admin: String,
    kid: String,
    kid_id: i64,
    movies: i64,
    tv: i64,
    music: i64,
    private: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Adult Film (2020)", "Family Film (2019)", "Home Video (2015)"] {
        app.media_file(&format!("movies/{name}/{name}.mkv"), 2048);
        app.media_file(&format!("movies/{name}/{name}.en.srt"), 64);
    }
    app.media_file("tv/Grown Show (2010)/Season 01/Grown Show - S01E01 - Pilot.mkv", 2048);
    app.media_file("tv/Grown Show (2010)/Season 01/Grown Show - S01E02 - Second.mkv", 2048);
    app.media_file("tv/Kids Show (2012)/Season 01/Kids Show - S01E01 - Hello.mkv", 2048);
    app.media_file("music/Band/Record (2001)/01 - Song.mp3", 512);
    app.media_file("private/Secret (2021).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let music = app.library(&admin, "Music", "music", "music").await;
    let private = app.library(&admin, "Private", "movies", "private").await;
    for (title, rating) in
        [("Adult Film", "R"), ("Family Film", "PG"), ("Grown Show", "TV-MA"), ("Kids Show", "TV-Y"), ("Secret", "G")]
    {
        app.set_rating(title, Some(rating)).await;
    }
    let (kid, kid_id) = app
        .user(&admin, "kid", json!({"all_libraries": false, "library_ids": [movies, tv, music], "max_age": 12}))
        .await;
    World { app, admin, kid, kid_id, movies, tv, music, private }
}

const CAPS: &str = r#"{"containers": ["mp4", "mkv", "mp3"], "video_codecs": ["h264"], "audio_codecs": ["aac", "mp3"]}"#;

/// Requests that touch an item and its first file, as `(method, uri, body)`.
fn item_requests(item: i64, file: i64) -> Vec<(Method, String, Option<Value>)> {
    let caps: Value = serde_json::from_str(CAPS).unwrap();
    vec![
        (Method::GET, format!("/api/items/{item}"), None),
        (Method::GET, format!("/api/items/{item}/children"), None),
        (Method::GET, format!("/api/items/{item}/related"), None),
        (Method::GET, format!("/api/items/{item}/on-deck"), None),
        (Method::GET, format!("/api/items/{item}/next"), None),
        (Method::GET, format!("/api/items/{item}/collections"), None),
        (Method::GET, format!("/api/items/{item}/images/backdrop?w=300"), None),
        (
            Method::POST,
            format!("/api/items/{item}/progress"),
            Some(json!({"position_ms": 60_000, "duration_ms": 100_000})),
        ),
        (Method::POST, format!("/api/items/{item}/watched"), Some(json!({}))),
        (Method::POST, format!("/api/items/{item}/unwatched"), Some(json!({}))),
        (Method::PUT, format!("/api/items/{item}/rating"), Some(json!({"rating": 8}))),
        (Method::PUT, format!("/api/items/{item}/streams"), Some(json!({"subtitle_index": null}))),
        (Method::POST, "/api/playback".into(), Some(json!({"item_id": item, "caps": caps, "mode": "direct"}))),
        (Method::GET, format!("/api/files/{file}/stream"), None),
        (Method::GET, format!("/api/files/{file}/subtitles/1000.vtt"), None),
        (Method::GET, format!("/api/files/{file}/trickplay"), None),
        (Method::GET, format!("/api/files/{file}/trickplay/0.jpg"), None),
        (Method::GET, format!("/api/files/{file}/chapters/0.jpg"), None),
    ]
}

/// The same item and file through the Plex-compatible API.
fn plex_requests(item: i64, file: i64) -> Vec<(Method, String)> {
    let thumb = encode(&format!("/library/metadata/{item}/thumb/1"));
    vec![
        (Method::GET, format!("/library/metadata/{item}")),
        (Method::GET, format!("/library/metadata/{item}/children")),
        (Method::GET, format!("/library/metadata/{item}/allLeaves")),
        (Method::GET, format!("/library/metadata/{item}/extras")),
        (Method::GET, format!("/library/metadata/{item}/thumb/1")),
        (Method::GET, format!("/photo/:/transcode?url={thumb}&width=100")),
        (Method::GET, format!("/library/parts/{file}/1/file.mkv")),
        (Method::GET, format!("/library/parts/{file}/indexes/sd")),
        (Method::GET, format!("/library/streams/{}", file * 10_000 + 1000)),
        (Method::GET, format!("/:/timeline?ratingKey={item}&state=playing&time=1000&duration=100000")),
        (Method::GET, format!("/:/scrobble?key={item}&identifier=com.plexapp.plugins.library")),
        (Method::GET, format!("/:/unscrobble?key={item}&identifier=com.plexapp.plugins.library")),
        (Method::GET, format!("/:/rate?key={item}&identifier=com.plexapp.plugins.library&rating=8")),
        (Method::GET, format!("/video/:/transcode/universal/start.m3u8?path=%2Flibrary%2Fmetadata%2F{item}")),
        (Method::GET, format!("/video/:/transcode/universal/decision?path=%2Flibrary%2Fmetadata%2F{item}")),
    ]
}

/// Every request answers 404 and gives nothing about `title` away.
async fn all_hidden(app: &TestApp, token: &str, item: i64, file: i64, title: &str) {
    for (method, uri, body) in item_requests(item, file) {
        let reply = app.call(method.clone(), &uri, Some(token), body, &[]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{method} {uri}: {}", reply.text());
        assert!(!reply.mentions(title), "{method} {uri} leaks {title}");
    }
    for (method, uri) in plex_requests(item, file) {
        let reply = app.plex(method.clone(), &uri, token, &[]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "Plex {method} {uri}: {}", reply.text());
        assert!(!reply.mentions(title), "Plex {method} {uri} leaks {title}");
    }
}

async fn recorded_state(app: &TestApp, user: i64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM user_item_state WHERE user_id = ?")
        .bind(user)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

#[tokio::test]
async fn titles_above_the_age_limit_answer_404_everywhere() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let movies = app.ids(w.movies, &w.admin).await;
    let (adult, family) = (movies["Adult Film"], movies["Family Film"]);

    let listed = app.get(&format!("/api/libraries/{}/items", w.movies), kid).await;
    assert_eq!(titles(&listed.body["items"]), ["Family Film"], "R and unrated titles are left out");
    all_hidden(app, kid, adult, app.file_of(adult).await, "Adult Film").await;
    assert_eq!(recorded_state(app, w.kid_id).await, 0, "nothing was recorded for the hidden title");

    // The same requests reach a title the kid may watch.
    let file = app.file_of(family).await;
    assert_eq!(app.get(&format!("/api/items/{family}"), kid).await.status, StatusCode::OK);
    assert_eq!(app.get(&format!("/api/files/{file}/stream"), kid).await.status, StatusCode::OK);
    let plex = app.plex(Method::GET, &format!("/library/parts/{file}/1/file.mkv"), kid, &[]).await;
    assert_eq!(plex.status, StatusCode::OK);
    let rated = app.put(&format!("/api/items/{family}/rating"), kid, json!({"rating": 8})).await;
    assert_eq!(rated.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn seasons_and_episodes_go_by_their_show() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let shows = app.ids(w.tv, &w.admin).await;
    let grown = shows["Grown Show"];
    let season = app.children(grown, &w.admin).await["Season 1"];
    let episodes = app.children(season, &w.admin).await;
    let (pilot, second) = (episodes["Pilot"], episodes["Second"]);

    assert_eq!(titles(&app.get(&format!("/api/libraries/{}/items", w.tv), kid).await.body["items"]), ["Kids Show"]);
    for id in [grown, season, pilot, second] {
        assert_eq!(app.get(&format!("/api/items/{id}"), kid).await.status, StatusCode::NOT_FOUND, "item {id}");
    }
    all_hidden(app, kid, pilot, app.file_of(pilot).await, "Pilot").await;
    let admin_next = app.get(&format!("/api/items/{pilot}/next"), &w.admin).await;
    assert_eq!(admin_next.body["item"]["title"], json!("Second"));

    // Search and the Plex episode listing leave them out too.
    assert_eq!(app.get("/api/search?q=pilot", kid).await.body["items"], json!([]));
    assert_eq!(titles(&app.get("/api/search?q=pilot", &w.admin).await.body["items"]), ["Pilot"]);
    let episodes = app.plex_json(&format!("/library/sections/{}/all?type=4", w.tv), kid).await;
    assert_eq!(plex_titles(&episodes), ["Hello"]);
    let seasons = app.plex_json(&format!("/library/sections/{}/all?type=3", w.tv), kid).await;
    assert_eq!(seasons["size"], json!(1));

    // The kids' show is all there.
    let kids_show = shows["Kids Show"];
    assert_eq!(app.children(kids_show, kid).await.len(), 1);
    let deck = app.get(&format!("/api/items/{kids_show}/on-deck"), kid).await;
    assert_eq!(deck.body["item"]["title"], json!("Hello"));
}

#[tokio::test]
async fn music_is_exempt_and_administrators_are_never_limited() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let artist = app.ids(w.music, kid).await["Band"];
    let album = app.children(artist, kid).await["Record"];
    let track = app.children(album, kid).await["Song"];
    let file = app.file_of(track).await;
    assert_eq!(app.get(&format!("/api/files/{file}/stream"), kid).await.status, StatusCode::OK, "unrated music plays");

    // An administrator's stored limits are ignored.
    sqlx::query("UPDATE users SET max_age = 0, all_libraries = 0 WHERE username = 'admin'")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let admin = app.login("admin", "secret-one").await;
    assert_eq!(app.get("/api/libraries", &admin).await.body.as_array().unwrap().len(), 4);
    let all = app.get(&format!("/api/libraries/{}/items", w.movies), &admin).await;
    assert_eq!(titles(&all.body["items"]), ["Adult Film", "Family Film", "Home Video"]);
    let me = app.get("/api/auth/me", &admin).await;
    assert_eq!((me.body["all_libraries"].clone(), me.body["max_age"].clone()), (json!(true), Value::Null));
}

#[tokio::test]
async fn a_library_the_user_may_not_open_does_not_exist_for_them() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let secret = app.ids(w.private, &w.admin).await["Secret"];
    let file = app.file_of(secret).await;
    let family = app.ids(w.movies, &w.admin).await["Family Film"];

    let libraries = app.get("/api/libraries", kid).await;
    let names: Vec<&str> = libraries.body.as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Movies", "TV", "Music"]);
    for path in ["", "/items", "/filters", "/hubs", "/collections"] {
        let reply = app.get(&format!("/api/libraries/{}{path}", w.private), kid).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "/api/libraries/{{private}}{path}");
    }
    all_hidden(app, kid, secret, file, "Secret").await;
    let home = app.get("/api/hubs/home", kid).await;
    assert!(!home.mentions("Secret") && !home.mentions("Private"), "{}", home.text());
    assert_eq!(app.get("/api/search?q=secret", kid).await.body["items"], json!([]));

    // The Plex-compatible API: sections, lists, hubs and search.
    let sections = app.plex_json("/library/sections", kid).await;
    let keys: Vec<&str> =
        sections["Directory"].as_array().unwrap().iter().map(|d| d["key"].as_str().unwrap()).collect();
    assert_eq!(keys, [w.movies.to_string(), w.tv.to_string(), w.music.to_string()]);
    for path in ["", "/all", "/recentlyAdded", "/onDeck", "/genre", "/firstCharacter", "/search?query=secret"] {
        let reply = app.plex(Method::GET, &format!("/library/sections/{}{path}", w.private), kid, &[]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "/library/sections/{{private}}{path}");
    }
    let hubs = app.plex(Method::GET, &format!("/hubs/sections/{}", w.private), kid, &[]).await;
    assert_eq!(hubs.status, StatusCode::NOT_FOUND);
    for uri in [
        "/library/all".to_owned(),
        "/library/recentlyAdded".to_owned(),
        "/library/onDeck".to_owned(),
        "/hubs".to_owned(),
        "/hubs/home/recentlyAdded".to_owned(),
        "/hubs/search?query=secret".to_owned(),
        "/search?query=secret".to_owned(),
        format!("/library/metadata/{family},{secret}"),
    ] {
        let reply = app.plex(Method::GET, &uri, kid, &[]).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}");
        assert!(!reply.mentions("Secret"), "{uri} leaks the private library");
    }
    let both = app.plex_json(&format!("/library/metadata/{family},{secret}"), kid).await;
    assert_eq!(plex_titles(&both), ["Family Film"]);
    let providers = app.plex(Method::GET, "/media/providers", kid, &[]).await;
    assert_eq!(providers.status, StatusCode::OK);
    assert!(!providers.mentions("Private"), "/media/providers lists the private library: {}", providers.text());
    // The administrator sees it everywhere.
    assert!(app.plex(Method::GET, "/media/providers", &w.admin, &[]).await.mentions("Private"));
}

#[tokio::test]
async fn titles_that_become_hidden_leave_continue_watching_and_history() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let family = app.ids(w.movies, &w.admin).await["Family Film"];
    let kids_show = app.ids(w.tv, &w.admin).await["Kids Show"];
    let hello = app.children(app.children(kids_show, &w.admin).await["Season 1"], &w.admin).await["Hello"];

    let started = app
        .post(&format!("/api/items/{family}/progress"), kid, json!({"position_ms": 50_000, "duration_ms": 100_000}))
        .await;
    assert_eq!(started.status, StatusCode::NO_CONTENT);
    let finished = app
        .post(&format!("/api/items/{hello}/progress"), kid, json!({"position_ms": 99_000, "duration_ms": 100_000}))
        .await;
    assert_eq!(finished.status, StatusCode::NO_CONTENT);
    let continuing = |hubs: &Value| {
        hubs["hubs"].as_array().unwrap().iter().find(|h| h["key"] == "continue_watching").map(|h| titles(&h["items"]))
    };
    assert_eq!(continuing(&app.get("/api/hubs/home", kid).await.body), Some(vec!["Family Film".to_owned()]));
    assert_eq!(plex_titles(&app.plex_json("/library/onDeck", kid).await), ["Family Film"]);
    assert_eq!(app.get("/api/history", kid).await.body["total"], json!(1));

    // The film is re-rated R: it is gone from the kid's continue watching.
    app.set_rating("Family Film", Some("R")).await;
    assert_eq!(continuing(&app.get("/api/hubs/home", kid).await.body), None);
    assert_eq!(app.plex_json("/library/onDeck", kid).await["size"], json!(0));
    assert_eq!(app.plex_json("/hubs/continueWatching/items", kid).await["size"], json!(0));

    // TV is taken away: the episode leaves the kid's history (and comes back with access).
    let patched =
        app.patch(&format!("/api/users/{}", w.kid_id), &w.admin, json!({"library_ids": [w.movies, w.music]})).await;
    assert_eq!(patched.status, StatusCode::OK, "{:?}", patched.body);
    let history = app.get("/api/history", kid).await;
    assert_eq!((history.body["total"].clone(), history.body["entries"].clone()), (json!(0), json!([])));
    assert!(plex_titles(&app.plex_json("/status/sessions/history/all", kid).await).is_empty());
    // Administrators still see the play.
    let all = app.get("/api/history?all=1", &w.admin).await;
    assert_eq!(all.body["total"], json!(1));
    assert_eq!(all.body["entries"][0]["user"]["name"], json!("kid"));
    app.patch(&format!("/api/users/{}", w.kid_id), &w.admin, json!({"library_ids": [w.movies, w.tv, w.music]})).await;
    assert_eq!(app.get("/api/history", kid).await.body["total"], json!(1));
}

#[tokio::test]
async fn people_are_listed_only_through_titles_the_user_can_see() {
    use pley::people::{self, NewCredit, PersonId, Role};

    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let movies = app.ids(w.movies, &w.admin).await;
    // Both have photos, already stored (as if downloaded before).
    let photo = |name: &str| format!("https://image.tmdb.org/t/p/h632/{name}.jpg");
    for name in ["rachel", "dan"] {
        let blob =
            app.state.store.put(&[b"\xff\xd8\xff\xe0".as_slice(), name.as_bytes()].concat(), "jpg").await.unwrap();
        sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES (?, ?, 0)")
            .bind(photo(name))
            .bind(blob)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    let credit = |id: i64, name: &str, role: Role, picture: &str| NewCredit {
        person: PersonId::Tmdb(id),
        name: name.into(),
        photo: Some(photo(picture)),
        role,
        character: None,
    };
    people::replace(
        &app.state.db,
        movies["Adult Film"],
        &[credit(1, "Rachel Restricted", Role::Actor, "rachel"), credit(2, "Dan Director", Role::Director, "dan")],
    )
    .await
    .unwrap();
    people::replace(&app.state.db, movies["Family Film"], &[credit(2, "Dan Director", Role::Director, "dan")])
        .await
        .unwrap();

    let actors = app.plex_json(&format!("/library/sections/{}/actor", w.movies), kid).await;
    assert!(actors.get("Directory").is_none(), "only credited in a hidden title: {actors}");
    let directors = app.plex_json(&format!("/library/sections/{}/director", w.movies), kid).await;
    assert_eq!(titles(&directors["Directory"]), ["Dan Director"]);
    // Administrators see everyone.
    let actors = app.plex_json(&format!("/library/sections/{}/actor", w.movies), &w.admin).await;
    assert_eq!(titles(&actors["Directory"]), ["Rachel Restricted"]);
    // pley's own search and pages agree, photos included.
    assert_eq!(app.get("/api/search?q=rachel", kid).await.body["people"], json!([]));
    let cast = &app.get(&format!("/api/items/{}", movies["Adult Film"]), &w.admin).await.body["cast"];
    let rachel = cast[0]["id"].as_i64().unwrap();
    let dan =
        app.get(&format!("/api/items/{}", movies["Family Film"]), kid).await.body["crew"][0]["id"].as_i64().unwrap();
    assert_eq!(app.get(&format!("/api/people/{rachel}"), kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/people/{rachel}/photo"), kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/people/{rachel}/photo"), &w.admin).await.status, StatusCode::OK);
    assert_eq!(app.get(&format!("/api/people/{dan}/photo"), kid).await.status, StatusCode::OK);
}

#[tokio::test]
async fn odd_plex_query_parameters_are_harmless() {
    let w = world().await;
    let uri = format!("/library/sections/{}/all?{}=1&type=1", w.movies, encode("abcdefé"));
    let reply = w.app.plex(Method::GET, &uri, &w.kid, &[]).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(plex_titles(&reply.body["MediaContainer"]), ["Family Film"]);
}

/// Titles of the `Directory` nodes of a Plex filter list.
fn directory_titles(container: &Value) -> Vec<String> {
    container["Directory"].as_array().unwrap().iter().map(|d| d["title"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn library_counts_leave_out_titles_above_the_age_limit() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let count = |list: &Value, id: i64| {
        list.as_array().unwrap().iter().find(|l| l["id"] == json!(id)).map(|l| l["item_count"].clone())
    };
    let theirs = app.get("/api/libraries", kid).await.body;
    let all = app.get("/api/libraries", &w.admin).await.body;
    assert_eq!(count(&theirs, w.movies), Some(json!(1)), "only Family Film");
    assert_eq!(count(&all, w.movies), Some(json!(3)));
    assert_eq!(count(&theirs, w.tv), Some(json!(1)), "only Kids Show");
    assert_eq!(count(&theirs, w.music), Some(json!(1)), "music has no age limit");
    assert_eq!(app.get(&format!("/api/libraries/{}", w.movies), kid).await.body["item_count"], json!(1));
}

#[tokio::test]
async fn filter_choices_only_come_from_titles_the_user_can_see() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    for (title, genres, studio) in
        [("Adult Film", r#"["Horror"]"#, "Night Studio"), ("Family Film", r#"["Family"]"#, "Day Studio")]
    {
        sqlx::query("UPDATE items SET genres = ?, studio = ? WHERE title = ?")
            .bind(genres)
            .bind(studio)
            .bind(title)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }

    let filters = format!("/api/libraries/{}/filters", w.movies);
    assert_eq!(app.get(&filters, kid).await.body, json!({"genres": ["Family"], "years": [2019]}));
    assert_eq!(
        app.get(&filters, &w.admin).await.body,
        json!({"genres": ["Family", "Horror"], "years": [2020, 2019, 2015]})
    );

    for (filter, kid_sees, admin_sees) in [
        ("genre", vec!["Family"], vec!["Family", "Horror"]),
        ("year", vec!["2019"], vec!["2020", "2019", "2015"]),
        ("contentRating", vec!["PG"], vec!["PG", "R"]),
        ("studio", vec!["Day Studio"], vec!["Day Studio", "Night Studio"]),
    ] {
        let uri = format!("/library/sections/{}/{filter}", w.movies);
        assert_eq!(directory_titles(&app.plex_json(&uri, kid).await), kid_sees, "{filter}");
        assert_eq!(directory_titles(&app.plex_json(&uri, &w.admin).await), admin_sees, "{filter}");
    }
}

#[tokio::test]
async fn the_strictest_rating_of_an_episode_and_its_show_counts() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let shows = app.ids(w.tv, &w.admin).await;
    let kids_season = app.children(shows["Kids Show"], &w.admin).await["Season 1"];
    let hello = app.children(kids_season, &w.admin).await["Hello"];
    let grown_season = app.children(shows["Grown Show"], &w.admin).await["Season 1"];
    let pilot = app.children(grown_season, &w.admin).await["Pilot"];

    // An episode rated above its (allowed) show is hidden on its own...
    assert_eq!(app.get(&format!("/api/items/{hello}"), kid).await.status, StatusCode::OK);
    app.set_rating("Hello", Some("TV-MA")).await;
    assert_eq!(app.get(&format!("/api/items/{hello}"), kid).await.status, StatusCode::NOT_FOUND);
    // ...and a milder rating of its own doesn't open a hidden show's episode.
    app.set_rating("Pilot", Some("TV-Y")).await;
    assert_eq!(app.get(&format!("/api/items/{pilot}"), kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/search?q=pilot", kid).await.body["items"], json!([]));
    assert_eq!(app.get(&format!("/api/items/{pilot}"), &w.admin).await.status, StatusCode::OK);
}

#[tokio::test]
async fn the_plex_api_does_not_accept_the_web_cookie() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let status = |header: (&'static str, String)| {
        let request = axum::http::Request::builder()
            .uri("/library/sections")
            .header(header.0, header.1)
            .body(axum::body::Body::empty())
            .unwrap();
        async { tower::ServiceExt::oneshot(app.plex.clone(), request).await.unwrap().status() }
    };
    // Browsers send pley's cookie to the Plex port too, where several GET
    // requests change things: it must not sign anyone in there, whatever
    // token it holds.
    let token = app.compatibility_token(&admin, "play").await;
    assert_eq!(status(("cookie", format!("pley_token={admin}"))).await, StatusCode::UNAUTHORIZED);
    assert_eq!(status(("cookie", format!("pley_token={token}"))).await, StatusCode::UNAUTHORIZED);
    assert_eq!(status(("x-plex-token", token.clone())).await, StatusCode::OK);
    assert_eq!(status(("authorization", format!("Bearer {token}"))).await, StatusCode::OK);
}

#[tokio::test]
async fn events_about_libraries_the_user_cannot_open_are_not_sent_to_them() {
    use futures::StreamExt;
    use pley::events::ServerEvent;

    let w = world().await;
    let request = axum::http::Request::builder()
        .uri("/api/events")
        .header("authorization", format!("Bearer {}", w.kid))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(w.app.api.clone(), request).await.unwrap();
    let mut stream = response.into_body().into_data_stream();
    w.app.state.events.send(ServerEvent::LibraryChanged { library_id: w.private });
    w.app.state.events.send(ServerEvent::LibraryChanged { library_id: w.movies });

    let mut received = String::new();
    let read = async {
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if received.contains(&format!("\"library_id\":{}", w.movies)) {
                return;
            }
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), read).await.expect("the movies event");
    assert!(!received.contains(&format!("\"library_id\":{}", w.private)), "{received}");
}

#[tokio::test]
async fn a_changed_age_limit_or_library_list_tells_the_users_devices() {
    use pley::events::ServerEvent;

    let w = world().await;
    let mut events = w.app.state.events.subscribe();
    let mut told = move || -> Vec<ServerEvent> { std::iter::from_fn(|| events.try_recv().ok()).collect() };
    let kid_id = w.kid_id;
    let for_kid = move |heard: &[ServerEvent]| {
        heard.iter().any(|e| matches!(e, ServerEvent::OwnLibrariesChanged { user_id } if *user_id == kid_id))
    };
    let user = format!("/api/users/{}", w.kid_id);
    // A name changes nothing they see.
    let renamed = w.app.patch(&user, &w.admin, json!({"display_name": "Little One"})).await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.body);
    assert!(!for_kid(&told()));
    // Lifting the age limit, or giving them a library, does: their devices fetch Home again.
    for change in [json!({"max_age": null}), json!({"library_ids": [w.movies, w.tv, w.music, w.private]})] {
        let reply = w.app.patch(&user, &w.admin, change).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        let heard = told();
        assert!(for_kid(&heard), "{heard:?}");
        let json = serde_json::to_value(heard.iter().find(|e| for_kid(std::slice::from_ref(e))).unwrap()).unwrap();
        assert_eq!(json, json!({"type": "libraries_changed"}));
    }
}
