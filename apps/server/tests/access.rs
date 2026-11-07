//! Who sees what (docs/API.md, "Access rules"): a library a user may not open
//! and titles above their age limit answer 404 on every endpoint of pley's
//! API and of the Jellyfin-compatible API, and never show up in lists, hubs,
//! search or history.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, encode, jf_names, titles};

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

/// The same item and file through the Jellyfin-compatible API, for the
/// user `me` (their Jellyfin id) with `source`, the file as a media source
/// listed to them.
fn jellyfin_requests(item: &str, me: &str, source: &str) -> Vec<(Method, String, Option<Value>)> {
    let report = || Some(json!({"ItemId": item, "PositionTicks": 600_000_000_i64}));
    vec![
        (Method::GET, format!("/Items/{item}"), None),
        (Method::GET, format!("/Users/{me}/Items/{item}"), None),
        (Method::GET, format!("/Items?ParentId={item}"), None),
        (Method::GET, format!("/Items/{item}/Similar"), None),
        (Method::GET, format!("/Items/{item}/SpecialFeatures"), None),
        (Method::GET, format!("/Items/{item}/LocalTrailers"), None),
        (Method::GET, format!("/Items/{item}/Ancestors"), None),
        (Method::GET, format!("/Items/{item}/Images/Primary"), None),
        (Method::GET, format!("/Items/{item}/Images/Backdrop/0"), None),
        (Method::GET, format!("/Shows/{item}/Seasons"), None),
        (Method::GET, format!("/Shows/{item}/Episodes"), None),
        (Method::GET, format!("/Shows/NextUp?SeriesId={item}"), None),
        (Method::GET, format!("/MediaSegments/{item}"), None),
        (Method::GET, format!("/Items/{item}/PlaybackInfo"), None),
        (Method::POST, format!("/Items/{item}/PlaybackInfo"), Some(json!({}))),
        (Method::GET, format!("/Videos/{item}/stream?static=true"), None),
        (Method::GET, format!("/Videos/{item}/stream.mkv?static=true&MediaSourceId={source}"), None),
        (Method::GET, format!("/Videos/{item}/master.m3u8?MediaSourceId={source}"), None),
        (Method::GET, format!("/Items/{item}/Download"), None),
        (Method::GET, format!("/Videos/{item}/{source}/Subtitles/1000/Stream.vtt"), None),
        (Method::GET, format!("/Videos/{item}/{source}/Subtitles/1000/0/Stream.srt"), None),
        (Method::GET, format!("/UserItems/{item}/UserData"), None),
        (Method::POST, format!("/UserItems/{item}/UserData"), Some(json!({"Played": true}))),
        (Method::POST, format!("/UserPlayedItems/{item}"), None),
        (Method::DELETE, format!("/UserPlayedItems/{item}"), None),
        (Method::POST, format!("/Users/{me}/PlayedItems/{item}"), None),
        (Method::POST, format!("/UserFavoriteItems/{item}"), None),
        (Method::POST, format!("/UserItems/{item}/Rating?Likes=true"), None),
        (Method::DELETE, format!("/UserItems/{item}/Rating"), None),
        (Method::POST, "/Sessions/Playing".into(), report()),
        (Method::POST, "/Sessions/Playing/Progress".into(), report()),
        (Method::POST, "/Sessions/Playing/Stopped".into(), report()),
    ]
}

/// Every request answers 404 and gives nothing about `title` away.
async fn all_hidden(app: &TestApp, token: &str, item: i64, file: i64, title: &str) {
    for (method, uri, body) in item_requests(item, file) {
        let reply = app.call(method.clone(), &uri, Some(token), body, &[]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{method} {uri}: {}", reply.text());
        assert!(!reply.mentions(title), "{method} {uri} leaks {title}");
    }
    let (id, me, source) = (app.jf_item(item), app.jf_user(token).await, app.jf_source(file, token).await);
    for (method, uri, body) in jellyfin_requests(&id, &me, &source) {
        let reply = app.jf(method.clone(), &uri, token, body).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "Jellyfin {method} {uri}: {}", reply.text());
        assert!(!reply.mentions(title), "Jellyfin {method} {uri} leaks {title}");
    }
    // Streams and subtitles are opened by their media source's id alone too.
    for uri in [
        format!("/Videos/{id}/stream?static=true&MediaSourceId={source}"),
        format!("/Videos/{source}/stream?static=true"),
        format!("/Videos/{id}/{source}/Subtitles/1000/Stream.vtt"),
    ] {
        let reply = app.jf(Method::GET, &uri, "", None).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "Jellyfin {uri} without a token: {}", reply.text());
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
    let stream = format!("/Videos/{}/stream?static=true", app.jf_item(family));
    assert_eq!(app.jf(Method::GET, &stream, kid, None).await.status, StatusCode::OK);
    let source = app.jf_source(file, kid).await;
    let without_token = format!("/Videos/{}/stream?static=true&MediaSourceId={source}", app.jf_item(family));
    assert_eq!(app.jf(Method::GET, &without_token, "", None).await.status, StatusCode::OK);
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

    // Search and the Jellyfin episode listing leave them out too, slips and
    // titles found alike included.
    assert_eq!(app.get("/api/search?q=pilot", kid).await.body["items"], json!([]));
    assert_eq!(titles(&app.get("/api/search?q=pilot", &w.admin).await.body["items"]), ["Pilot"]);
    for slip in ["pliot", "pilooot"] {
        let q = format!("/api/search?q={slip}");
        assert_eq!(app.get(&q, kid).await.body["items"], json!([]), "{slip}");
        assert_eq!(titles(&app.get(&q, &w.admin).await.body["items"]), ["Pilot"], "{slip}");
    }
    let tv = app.jf_library(w.tv);
    let episodes = app.jf_json(&format!("/Items?ParentId={tv}&IncludeItemTypes=Episode&Recursive=true"), kid).await;
    assert_eq!(jf_names(&episodes), ["Hello"]);
    let seasons = app.jf_json(&format!("/Items?ParentId={tv}&IncludeItemTypes=Season&Recursive=true"), kid).await;
    assert_eq!(seasons["TotalRecordCount"], json!(1));
    assert!(jf_names(&app.jf_json("/Items?SearchTerm=pilot&Recursive=true", kid).await).is_empty());
    let hints = app.jf_json("/Search/Hints?SearchTerm=pilot", kid).await;
    assert_eq!(hints["TotalRecordCount"], json!(0));

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

    // The Jellyfin-compatible API: views, listings, home rows and search.
    assert_eq!(jf_names(&app.jf_json("/UserViews", kid).await), ["Movies", "TV", "Music"]);
    assert_eq!(jf_names(&app.jf_json("/Items", kid).await), ["Movies", "TV", "Music"]);
    let private = app.jf_library(w.private);
    for uri in [
        format!("/Items/{private}"),
        format!("/Items?ParentId={private}"),
        format!("/Items?ParentId={private}&Recursive=true&IncludeItemTypes=Movie"),
        format!("/Items/Filters?ParentId={private}"),
        format!("/Items/Filters2?ParentId={private}"),
        format!("/Genres?ParentId={private}"),
        format!("/Items/Latest?ParentId={private}"),
    ] {
        let reply = app.jf(Method::GET, &uri, kid, None).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}: {}", reply.text());
    }
    let (family, secret) = (app.jf_item(family), app.jf_item(secret));
    for uri in [
        "/Items?Recursive=true&IncludeItemTypes=Movie".to_owned(),
        "/Items/Latest".to_owned(),
        "/UserItems/Resume".to_owned(),
        "/Shows/NextUp".to_owned(),
        "/Items/Suggestions".to_owned(),
        "/Items?SearchTerm=secret&Recursive=true".to_owned(),
        "/Search/Hints?SearchTerm=secret".to_owned(),
        "/Studios".to_owned(),
        "/Persons?SearchTerm=secret".to_owned(),
        format!("/Items?Ids={family},{secret}"),
    ] {
        let reply = app.jf(Method::GET, &uri, kid, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text());
        assert!(!reply.mentions("Secret"), "{uri} leaks the private library");
    }
    let both = app.jf_json(&format!("/Items?Ids={family},{secret}"), kid).await;
    assert_eq!(jf_names(&both), ["Family Film"]);
    // The administrator sees it everywhere.
    assert!(app.jf(Method::GET, "/UserViews", &w.admin, None).await.mentions("Private"));
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
    assert_eq!(jf_names(&app.jf_json("/UserItems/Resume", kid).await), ["Family Film"]);
    assert_eq!(app.get("/api/history", kid).await.body["total"], json!(1));
    let played = "/Items?Recursive=true&IncludeItemTypes=Episode&Filters=IsPlayed";
    assert_eq!(jf_names(&app.jf_json(played, kid).await), ["Hello"]);

    // The film is re-rated R: it is gone from the kid's continue watching.
    app.set_rating("Family Film", Some("R")).await;
    assert_eq!(continuing(&app.get("/api/hubs/home", kid).await.body), None);
    assert_eq!(app.jf_json("/UserItems/Resume", kid).await["TotalRecordCount"], json!(0));

    // TV is taken away: the episode leaves the kid's history (and comes back with access).
    let patched =
        app.patch(&format!("/api/users/{}", w.kid_id), &w.admin, json!({"library_ids": [w.movies, w.music]})).await;
    assert_eq!(patched.status, StatusCode::OK, "{:?}", patched.body);
    let history = app.get("/api/history", kid).await;
    assert_eq!((history.body["total"].clone(), history.body["entries"].clone()), (json!(0), json!([])));
    assert!(jf_names(&app.jf_json(played, kid).await).is_empty());
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

    // Jellyfin's people search, details and the titles of a person.
    assert!(jf_names(&app.jf_json("/Persons?SearchTerm=rachel", kid).await).is_empty(), "only in a hidden title");
    assert_eq!(jf_names(&app.jf_json("/Persons?SearchTerm=rachel", &w.admin).await), ["Rachel Restricted"]);
    let family = app.jf_json(&format!("/Items/{}", app.jf_item(movies["Family Film"])), kid).await;
    let people: Vec<&str> = family["People"].as_array().unwrap().iter().map(|p| p["Name"].as_str().unwrap()).collect();
    assert_eq!(people, ["Dan Director"]);
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
    let person = |id: i64| app.jf_ids().of(pley::jellyfin::ids::Kind::Person, id);
    for uri in [format!("/Items/{}", person(rachel)), format!("/Items/{}/Images/Primary", person(rachel))] {
        assert_eq!(app.jf(Method::GET, &uri, kid, None).await.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(app.jf(Method::GET, &uri, &w.admin, None).await.status, StatusCode::OK, "{uri}");
    }
    let credited = format!("/Items?PersonIds={}&Recursive=true", person(rachel));
    assert!(jf_names(&app.jf_json(&credited, kid).await).is_empty());
    assert_eq!(jf_names(&app.jf_json(&credited, &w.admin).await), ["Adult Film"]);
    let photo = format!("/Items/{}/Images/Primary", person(dan));
    assert_eq!(app.jf(Method::GET, &photo, kid, None).await.status, StatusCode::OK);
}

#[tokio::test]
async fn odd_jellyfin_query_parameters_are_harmless() {
    let w = world().await;
    let movies = w.app.jf_library(w.movies);
    let uri = format!("/Items?ParentId={movies}&{}=1&IncludeItemTypes=Movie&Recursive=true", encode("abcdefé"));
    let reply = w.app.jf(Method::GET, &uri, &w.kid, None).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(jf_names(&reply.body), ["Family Film"]);
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

    // The adult film is the only one in 4K and Dolby Vision.
    for (title, width, height, range) in
        [("Adult Film", 3840, 2160, "dolby_vision"), ("Family Film", 1920, 1080, "sdr")]
    {
        sqlx::query(
            "UPDATE media_files SET width = ?, height = ?, dynamic_range = ?
             WHERE item_id = (SELECT id FROM items WHERE title = ?)",
        )
        .bind(width)
        .bind(height)
        .bind(range)
        .bind(title)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    }

    let filters = format!("/api/libraries/{}/filters", w.movies);
    assert_eq!(
        app.get(&filters, kid).await.body,
        json!({"genres": ["Family"], "years": [2019], "resolutions": [], "dynamic_ranges": []})
    );
    assert_eq!(
        app.get(&filters, &w.admin).await.body,
        json!({"genres": ["Family", "Horror"], "years": [2020, 2019, 2015],
               "resolutions": ["4k", "1080"], "dynamic_ranges": ["dolby_vision", "sdr"]})
    );

    let movies = app.jf_library(w.movies);
    let filters = format!("/Items/Filters?ParentId={movies}");
    let theirs = app.jf_json(&filters, kid).await;
    assert_eq!((theirs["Genres"].clone(), theirs["Years"].clone()), (json!(["Family"]), json!([2019])));
    let all = app.jf_json(&filters, &w.admin).await;
    assert_eq!((all["Genres"].clone(), all["Years"].clone()), (json!(["Family", "Horror"]), json!([2020, 2019, 2015])));
    let names = |list: &Value| -> Vec<String> {
        list.as_array().unwrap().iter().map(|g| g["Name"].as_str().unwrap().to_owned()).collect()
    };
    let filters2 = format!("/Items/Filters2?ParentId={movies}");
    assert_eq!(names(&app.jf_json(&filters2, kid).await["Genres"]), ["Family"]);
    assert_eq!(jf_names(&app.jf_json(&format!("/Genres?ParentId={movies}"), kid).await), ["Family"]);
    assert_eq!(jf_names(&app.jf_json("/Studios", kid).await), ["Day Studio"]);
    assert_eq!(jf_names(&app.jf_json("/Studios", &w.admin).await), ["Day Studio", "Night Studio"]);
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
async fn the_jellyfin_api_takes_only_tokens_for_other_apps() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let status = |uri: &'static str, header: Option<(&'static str, String)>| {
        let mut request = axum::http::Request::builder().uri(uri);
        if let Some((name, value)) = header {
            request = request.header(name, value);
        }
        let request = request.body(axum::body::Body::empty()).unwrap();
        async { tower::ServiceExt::oneshot(app.jellyfin.clone(), request).await.unwrap().status() }
    };
    // Browsers send pley's cookie to the Jellyfin port too: it must not sign
    // anyone in there, whatever token it holds. Nor do Pley's own tokens.
    let token = app.compatibility_token(&admin, "play").await;
    for refused in [
        ("cookie", format!("pley_token={admin}")),
        ("cookie", format!("pley_token={token}")),
        ("x-emby-token", admin.clone()),
        ("authorization", format!("Bearer {token}")),
        ("authorization", format!(r#"MediaBrowser Client="x", Token="{admin}""#)),
    ] {
        assert_eq!(status("/Users/Me", Some(refused.clone())).await, StatusCode::UNAUTHORIZED, "{refused:?}");
    }
    assert_eq!(status("/Users/Me", None).await, StatusCode::UNAUTHORIZED);
    for taken in [
        ("x-emby-token", token.clone()),
        ("x-mediabrowser-token", token.clone()),
        (
            "authorization",
            format!(r#"MediaBrowser Client="x", Device="y", DeviceId="z", Version="1", Token="{token}""#),
        ),
        ("x-emby-authorization", format!(r#"Emby Client="x", Token="{token}""#)),
    ] {
        assert_eq!(status("/Users/Me", Some(taken.clone())).await, StatusCode::OK, "{taken:?}");
    }
    let mut request = axum::http::Request::builder().uri(format!("/Users/Me?api_key={token}"));
    request = request.header("user-agent", "test");
    let reply = tower::ServiceExt::oneshot(app.jellyfin.clone(), request.body(axum::body::Body::empty()).unwrap());
    assert_eq!(reply.await.unwrap().status(), StatusCode::OK);
    // What anyone may read before signing in.
    assert_eq!(status("/System/Info/Public", None).await, StatusCode::OK);
    assert_eq!(status("/System/Info", None).await, StatusCode::UNAUTHORIZED);
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

/// Gives item `id` these labels, through the edit dialog's route.
async fn label(app: &TestApp, admin: &str, id: i64, labels: &[&str]) {
    let reply = app.patch(&format!("/api/items/{id}"), admin, json!({ "labels": labels })).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
}

#[tokio::test]
async fn label_rules_show_titles_above_the_age_limit_and_hide_others() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (adult, family, home) = (movies["Adult Film"], movies["Family Film"], movies["Home Video"]);
    label(app, admin, adult, &["Kids OK"]).await;
    label(app, admin, home, &["Kids OK"]).await;
    label(app, admin, family, &["Not Here"]).await;
    // Matched whatever their case.
    let rules = json!({"show_labels": ["kids ok"], "hide_labels": ["NOT HERE"]});
    assert_eq!(app.patch(&format!("/api/users/{}", w.kid_id), admin, rules).await.status, StatusCode::OK);
    let kid = w.kid.as_str();

    // Above the limit and unrated, shown by a label; within it, hidden by one.
    let listed = app.get(&format!("/api/libraries/{}/items", w.movies), kid).await;
    assert_eq!(titles(&listed.body["items"]), ["Adult Film", "Home Video"]);
    assert_eq!(app.get(&format!("/api/items/{adult}"), kid).await.status, StatusCode::OK);
    all_hidden(app, kid, family, app.file_of(family).await, "Family Film").await;
    assert_eq!(app.get("/api/search?q=family", kid).await.body["items"], json!([]));
    let count = app.get(&format!("/api/libraries/{}", w.movies), kid).await;
    assert_eq!(count.body["item_count"], json!(2));

    // "Never" wins over "always".
    label(app, admin, home, &["Kids OK", "Not Here"]).await;
    let listed = app.get(&format!("/api/libraries/{}/items", w.movies), kid).await;
    assert_eq!(titles(&listed.body["items"]), ["Adult Film"]);

    // A label on a show hides its episodes, one on an album its tracks.
    let shows = app.ids(w.tv, admin).await;
    let kids_show = shows["Kids Show"];
    let season = app.children(kids_show, admin).await["Season 1"];
    let hello = app.children(season, admin).await["Hello"];
    label(app, admin, kids_show, &["not here"]).await;
    assert_eq!(app.get(&format!("/api/libraries/{}/items", w.tv), kid).await.body["items"], json!([]));
    all_hidden(app, kid, hello, app.file_of(hello).await, "Hello").await;
    let artist = app.ids(w.music, admin).await["Band"];
    let album = app.children(artist, admin).await["Record"];
    let song = app.children(album, admin).await["Song"];
    label(app, admin, album, &["Not Here"]).await;
    assert_eq!(app.get(&format!("/api/items/{song}"), kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/items/{artist}"), kid).await.status, StatusCode::OK);

    // A label never opens a library.
    let secret = app.ids(w.private, admin).await["Secret"];
    label(app, admin, secret, &["Kids OK"]).await;
    assert_eq!(app.get(&format!("/api/items/{secret}"), kid).await.status, StatusCode::NOT_FOUND);

    // "Never" hides from a profile without an age limit too.
    let (teen, _) = app.user(admin, "teen", json!({"hide_labels": ["Not Here"]})).await;
    let listed = app.get(&format!("/api/libraries/{}/items", w.movies), &teen).await;
    assert_eq!(titles(&listed.body["items"]), ["Adult Film"]);
    let count = app.get(&format!("/api/libraries/{}", w.movies), &teen).await;
    assert_eq!(count.body["item_count"], json!(1));
    // Nor are the people credited only in what it hides.
    let credit = pley::people::NewCredit {
        person: pley::people::PersonId::Tmdb(9),
        name: "Nora Hidden".into(),
        photo: None,
        role: pley::people::Role::Actor,
        character: None,
    };
    pley::people::replace(&app.state.db, family, &[credit]).await.unwrap();
    assert_eq!(app.get("/api/search?q=nora", &teen).await.body["people"], json!([]));
    assert_eq!(app.get("/api/search?q=nora", admin).await.body["people"][0]["name"], json!("Nora Hidden"));

    // Administrators never are limited by them.
    let all = app.get(&format!("/api/libraries/{}/items", w.movies), admin).await;
    assert_eq!(titles(&all.body["items"]), ["Adult Film", "Family Film", "Home Video"]);
}

#[tokio::test]
async fn labels_are_shown_and_listed_to_administrators_only() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (adult, family) = (movies["Adult Film"], movies["Family Film"]);
    label(app, admin, adult, &["Override: Adult", " 4K ", "4k"]).await;
    label(app, admin, family, &["4K"]).await;
    assert_eq!(app.get(&format!("/api/items/{adult}"), admin).await.body["labels"], json!(["4K", "Override: Adult"]));
    let theirs = app.get(&format!("/api/items/{family}"), &w.kid).await;
    assert!(theirs.body.get("labels").is_none(), "{:?}", theirs.body);
    let listed = app.get("/api/labels", admin).await.body;
    assert_eq!(
        listed,
        json!([{"label": "4K", "items": 2, "rules": 0}, {"label": "Override: Adult", "items": 1, "rules": 0}])
    );
    // A label a rule names is offered before any title carries it.
    let rules = json!({"show_labels": ["Kids OK"], "hide_labels": ["override: adult"]});
    assert_eq!(app.patch(&format!("/api/users/{}", w.kid_id), admin, rules).await.status, StatusCode::OK);
    let invite = app.post("/api/invites", admin, json!({"hide_labels": ["Kids OK"]})).await;
    assert_eq!(invite.status, StatusCode::CREATED, "{:?}", invite.body);
    let listed = app.get("/api/labels", admin).await.body;
    assert_eq!(
        listed,
        json!([
            {"label": "4K", "items": 2, "rules": 0},
            {"label": "Kids OK", "items": 0, "rules": 2},
            {"label": "Override: Adult", "items": 1, "rules": 1}
        ])
    );
    assert_eq!(app.get("/api/labels", &w.kid).await.status, StatusCode::FORBIDDEN);
    // Nor do the compatibility APIs tell them, but to tokens that edit.
    let plex_labels = |item: &Value| item["Label"].as_array().map_or(0, Vec::len);
    let in_plex = format!("/library/metadata/{family}");
    assert_eq!(plex_labels(&app.plex_json(&in_plex, &w.kid).await["Metadata"][0]), 0);
    assert_eq!(plex_labels(&app.plex_json(&in_plex, admin).await["Metadata"][0]), 1);
    let choices = format!("/library/sections/{}/label", w.movies);
    assert!(app.plex_json(&choices, &w.kid).await.get("Directory").is_none_or(|d| d == &json!([])));
    assert_eq!(app.plex_json(&choices, admin).await["Directory"].as_array().map_or(0, Vec::len), 2);
    let in_jellyfin = format!("/Items/{}", app.jf_item(family));
    assert!(app.jf_json(&in_jellyfin, &w.kid).await.get("Tags").is_none());
    let manager = app.compatibility_token(admin, "manage").await;
    assert_eq!(app.jf_json(&in_jellyfin, &manager).await["Tags"], json!(["4K"]));

    // Replaced as a whole; seasons and episodes take them, collections' extras don't matter here.
    label(app, admin, adult, &["4K"]).await;
    assert_eq!(app.get(&format!("/api/items/{adult}"), admin).await.body["labels"], json!(["4K"]));
    let bad = app.patch(&format!("/api/items/{adult}"), admin, json!({"labels": "4K"})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let long = "x".repeat(121);
    let bad = app.patch(&format!("/api/items/{adult}"), admin, json!({"labels": [long]})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

/// The age limit's pickers are told how many movies and shows each library
/// has without a rating, which a limit hides; administrators only.
#[tokio::test]
async fn administrators_are_told_which_libraries_have_unrated_titles() {
    let w = world().await;
    let systems = w.app.get("/api/ratings/systems", &w.admin).await.body;
    assert_eq!(systems["unrated"], json!([{"library_id": w.movies, "items": 1}]), "Home Video; music doesn't count");
    let theirs = w.app.get("/api/ratings/systems", &w.kid).await.body;
    assert!(theirs.get("unrated").is_none(), "{theirs}");
}
