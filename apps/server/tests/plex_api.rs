//! More of the Plex-compatible API (docs/COMPATIBILITY.md): server endpoints, section
//! listings and filter choices, the order of sections, hubs, watch-state
//! reports and what only administrators may do. tests/plex.rs covers the
//! basics.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, plex_titles, titles};

/// Movies with genres and ratings, a two-season show and an album.
async fn world() -> (TestApp, String, i64, i64, i64) {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "2001 A Space Odyssey (1968)", "Zardoz (1974)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    for (s, e) in [(1, 1), (1, 2), (2, 1)] {
        app.media_file(&format!("tv/Show (2015)/Season {s:02}/Show - S{s:02}E{e:02}.mkv"), 1024);
    }
    app.media_file("music/Band/Record (2001)/01 - First.mp3", 512);
    app.media_file("music/Band/Record (2001)/02 - Second.mp3", 512);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let music = app.library(&admin, "Music", "music", "music").await;
    let m = app.ids(movies, &admin).await;
    for (title, genres, rating, studio) in [
        ("Alien", r#"["Horror","Science Fiction"]"#, "R", "Brandywine"),
        ("Brazil", r#"["Comedy","Science Fiction"]"#, "R", "Embassy"),
        ("2001 A Space Odyssey", r#"["Science Fiction"]"#, "G", "MGM"),
    ] {
        sqlx::query("UPDATE items SET genres = ?, content_rating = ?, studio = ? WHERE id = ?")
            .bind(genres)
            .bind(rating)
            .bind(studio)
            .bind(m[title])
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    (app, admin, movies, tv, music)
}

/// The `Guid` ids of an item's details.
async fn guids(app: &TestApp, token: &str, id: i64) -> Vec<String> {
    let details = app.plex_json(&format!("/library/metadata/{id}"), token).await;
    let item = details["Metadata"].as_array().and_then(|m| m.first()).or_else(|| details["Directory"].get(0)).unwrap();
    item["Guid"]
        .as_array()
        .map(|g| g.iter().map(|g| g["id"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn directory_titles(container: &Value) -> Vec<String> {
    container["Directory"]
        .as_array()
        .map(|d| d.iter().map(|d| d["title"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn server_endpoints() {
    let (app, admin, ..) = world().await;
    app.put("/api/settings", &admin, json!({"server_name": "Den"})).await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;

    let root = app.plex_json("/", &admin).await;
    assert_eq!(root["friendlyName"], json!("Den"));
    assert_eq!(root["machineIdentifier"].as_str().unwrap().len(), 40);
    let prefs = app.plex_json("/:/prefs", &admin).await;
    let friendly = prefs["Setting"].as_array().unwrap().iter().find(|s| s["id"] == "FriendlyName").unwrap().clone();
    assert_eq!(friendly["value"], json!("Den"));
    assert_eq!(app.plex(Method::PUT, "/:/prefs?FriendlyName=x", &admin, &[]).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/settings", &admin).await.body["server_name"], json!("Den"), "Plex can't change settings");

    // Accounts: administrators see everyone, others themselves.
    let accounts = app.plex_json("/accounts", &admin).await;
    assert_eq!(accounts["size"], json!(2));
    let mine = app.plex_json("/accounts", &bob).await;
    assert_eq!((mine["size"].clone(), mine["Account"][0]["id"].clone()), (json!(1), json!(bob_id)));
    let myplex = app.plex(Method::GET, "/myplex/account", &bob, &[]).await;
    assert_eq!(myplex.body["MyPlex"]["username"], json!("bob"));
    assert_eq!(myplex.body["MyPlex"]["authToken"], json!(""), "never linked to plex.tv");

    // The server is reported at the address the client used.
    let servers = app.plex(Method::GET, "/servers", &admin, &[("host", "media.lan:32401")]).await;
    let server = &servers.body["MediaContainer"]["Server"][0];
    assert_eq!((server["host"].clone(), server["port"].clone()), (json!("media.lan"), json!(32401)));
    // No port: the scheme's (a reverse proxy in front of pley's port).
    let servers = app.plex(Method::GET, "/servers", &admin, &[("host", "media.lan")]).await;
    assert_eq!(servers.body["MediaContainer"]["Server"][0]["port"], json!(80));
    let proxied = [("host", "media.example.com"), ("x-forwarded-proto", "https")];
    let servers = app.plex(Method::GET, "/servers", &admin, &proxied).await;
    assert_eq!(servers.body["MediaContainer"]["Server"][0]["port"], json!(443));

    // Errors are Plex-style HTML pages.
    let missing = app.plex(Method::GET, "/library/metadata/999999", &admin, &[]).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.text().contains("<h1>404 Not Found</h1>"), "{}", missing.text());
    let bad = app.plex(Method::GET, "/:/timeline?state=playing", &admin, &[]).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert!(bad.text().contains("ratingKey or key is required"));
}

#[tokio::test]
async fn provider_guids_as_plex_reports_them() {
    let (app, admin, _, tv, music) = world().await;
    let show = app.ids(tv, &admin).await["Show"];
    let artist = app.ids(music, &admin).await["Band"];
    let album = app.children(artist, &admin).await["Record"];
    for (id, column, value) in [
        (show, "tvdb_id", json!(73739)),
        (show, "imdb_id", json!("tt0411008")),
        (album, "musicbrainz_id", json!("b8048f24-c026-3398-b23a-b5e50716cbc7")),
        (artist, "musicbrainz_id", json!("a74b1b7f-71a5-4011-9441-d0b5e4122711")),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE items SET {column} = ? WHERE id = ?")))
            .bind(value.as_i64().map(|n| n.to_string()).unwrap_or_else(|| value.as_str().unwrap().to_owned()))
            .bind(id)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    assert_eq!(guids(&app, &admin, show).await, ["imdb://tt0411008", "tvdb://73739"]);
    assert_eq!(guids(&app, &admin, album).await, ["mbid://b8048f24-c026-3398-b23a-b5e50716cbc7"]);
    assert_eq!(guids(&app, &admin, artist).await, ["mbid://a74b1b7f-71a5-4011-9441-d0b5e4122711"]);
    // Found by them, as Overseerr and Kometa look shows up.
    let found = app.plex_json(&format!("/library/sections/{tv}/all?guid=tvdb://73739"), &admin).await;
    assert_eq!(plex_titles(&found), ["Show"]);
    let found = app.plex_json(&format!("/library/sections/{music}/all?type=9&guid=mbid://nothing"), &admin).await;
    assert!(plex_titles(&found).is_empty());
}

#[tokio::test]
async fn intro_and_credits_markers_on_request() {
    let (app, admin, movies, ..) = world().await;
    let alien = app.ids(movies, &admin).await["Alien"];
    let file: i64 = sqlx::query_scalar("SELECT id FROM media_files WHERE item_id = ?")
        .bind(alien)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO markers (file_id, kind, start_ms, end_ms, source, created_at)
         VALUES (?, 'intro', 30000, 90000, 'fingerprint', 0), (?, 'credits', 5000000, 5400000, 'chapter', 0)",
    )
    .bind(file)
    .bind(file)
    .execute(&app.state.db.write)
    .await
    .unwrap();
    let markers = |details: &Value| details["Metadata"][0]["Marker"].as_array().cloned().unwrap_or_default();

    let plain = app.plex_json(&format!("/library/metadata/{alien}"), &admin).await;
    assert!(markers(&plain).is_empty(), "only when asked for, as Plex does");
    let details = app.plex_json(&format!("/library/metadata/{alien}?includeMarkers=1"), &admin).await;
    let found = markers(&details);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0]["type"], json!("intro"));
    assert_eq!((found[0]["startTimeOffset"].as_i64(), found[0]["endTimeOffset"].as_i64()), (Some(30000), Some(90000)));
    assert_eq!(found[0]["final"], Value::Null);
    assert_eq!(found[1]["type"], json!("credits"));
    assert_eq!(found[1]["final"].as_i64(), Some(1));
    assert_ne!(found[0]["id"], found[1]["id"]);
}

#[tokio::test]
async fn more_like_this_as_plex_clients_read_it() {
    let (app, admin, movies, ..) = world().await;
    let m = app.ids(movies, &admin).await;
    let alien = m["Alien"];

    let related = app.plex_json(&format!("/library/metadata/{alien}/related"), &admin).await;
    let hub = &related["Hub"][0];
    assert_eq!(
        (&hub["title"], &hub["hubIdentifier"], &hub["type"]),
        (&json!("More Like This"), &json!("movie.similar"), &json!("movie"))
    );
    let titles = plex_titles(hub);
    assert!(titles.contains(&"Brazil".to_owned()) && titles.contains(&"2001 A Space Odyssey".to_owned()), "{titles:?}");
    assert!(!titles.contains(&"Alien".to_owned()), "not the item itself");
    let similar = app.plex_json(&format!("/library/metadata/{alien}/similar"), &admin).await;
    assert_eq!(plex_titles(&similar), titles, "the same items as a list");

    // Hidden items have no neighbours either.
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 12})).await;
    let hidden = app.plex(Method::GET, &format!("/library/metadata/{alien}/related"), &kid, &[]).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_administrators_refresh() {
    let (app, admin, movies, ..) = world().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let alien = app.ids(movies, &admin).await["Alien"];
    for uri in [
        format!("/library/sections/{movies}/refresh"),
        "/library/sections/all/refresh".to_owned(),
        format!("/library/metadata/{alien}/refresh"),
    ] {
        assert_eq!(app.plex(Method::PUT, &uri, &bob, &[]).await.status, StatusCode::FORBIDDEN, "{uri}");
        assert_eq!(app.plex(Method::PUT, &uri, &admin, &[]).await.status, StatusCode::OK, "{uri}");
    }
    app.wait_for_scans().await;
    let gone = app.plex(Method::PUT, "/library/sections/999/refresh", &admin, &[]).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn section_directories_and_filter_choices() {
    let (app, admin, movies, tv, music) = world().await;
    let section = app.plex_json(&format!("/library/sections/{movies}"), &admin).await;
    let keys: Vec<&str> = section["Directory"].as_array().unwrap().iter().map(|d| d["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["all", "recentlyAdded", "onDeck", "genre", "year", "decade", "search?type=1"]);
    let music_section = app.plex_json(&format!("/library/sections/{music}"), &admin).await;
    assert!(!directory_titles(&music_section).contains(&"On Deck".to_owned()), "music has no on deck");

    let genres = app.plex_json(&format!("/library/sections/{movies}/genre"), &admin).await;
    assert_eq!(directory_titles(&genres), ["Comedy", "Horror", "Science Fiction"]);
    let fast_key = genres["Directory"][2]["fastKey"].as_str().unwrap();
    assert_eq!(fast_key, format!("/library/sections/{movies}/all?type=1&genre=Science%20Fiction"));
    let by_genre = app.plex_json(&format!("{fast_key}&sort=titleSort"), &admin).await;
    assert_eq!(plex_titles(&by_genre), ["2001 A Space Odyssey", "Alien", "Brazil"]);
    let years = app.plex_json(&format!("/library/sections/{movies}/year"), &admin).await;
    assert_eq!(directory_titles(&years), ["1985", "1979", "1974", "1968"]);
    let decades = app.plex_json(&format!("/library/sections/{movies}/decade"), &admin).await;
    assert_eq!(directory_titles(&decades), ["1980", "1970", "1960"]);
    let ratings = app.plex_json(&format!("/library/sections/{movies}/contentRating"), &admin).await;
    assert_eq!(directory_titles(&ratings), ["G", "R"]);
    let studios = app.plex_json(&format!("/library/sections/{movies}/studio"), &admin).await;
    assert_eq!(directory_titles(&studios), ["Brandywine", "Embassy", "MGM"]);
    let unknown = app.plex(Method::GET, &format!("/library/sections/{movies}/mood"), &admin, &[]).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    // First letters of the sort titles; digits are "#".
    let letters = app.plex_json(&format!("/library/sections/{movies}/firstCharacter"), &admin).await;
    let counts: Vec<(String, i64)> = letters["Directory"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["title"].as_str().unwrap().to_owned(), d["size"].as_i64().unwrap()))
        .collect();
    assert_eq!(counts, [("#".to_owned(), 1), ("A".to_owned(), 1), ("B".to_owned(), 1), ("Z".to_owned(), 1)]);

    // Shows by type, seasons by index, albums of music sections.
    let seasons = app.plex_json(&format!("/library/sections/{tv}/all?type=3"), &admin).await;
    assert_eq!(plex_titles(&seasons), ["Season 1", "Season 2"]);
    let show = app.ids(tv, &admin).await["Show"];
    let second = app.plex_json(&format!("/library/metadata/{show}/children?index=2"), &admin).await;
    assert_eq!(plex_titles(&second), ["Season 2"]);
    let albums = app.plex_json(&format!("/library/sections/{music}/albums"), &admin).await;
    assert_eq!(plex_titles(&albums), ["Record"]);
    let tracks = app.plex_json(&format!("/library/sections/{music}/all?type=10&sort=index"), &admin).await;
    assert_eq!(plex_titles(&tracks), ["First", "Second"]);
    assert_eq!(tracks["Metadata"][1]["grandparentTitle"], json!("Band"));
    assert_eq!(tracks["Metadata"][1]["parentIndex"], json!(1), "disc 1 when there is no disc number");
}

#[tokio::test]
async fn sections_and_hubs_follow_the_library_order() {
    let (app, admin, movies, tv, music) = world().await;
    let order = app.put("/api/libraries/order", &admin, json!({"ids": [tv, music, movies]})).await;
    assert_eq!(order.status, StatusCode::OK, "{:?}", order.body);
    let (kid, _) = app.user(&admin, "kid", json!({"all_libraries": false, "library_ids": [movies, tv]})).await;

    // Everyone sees the sections in that order, users only theirs.
    for (token, wanted) in [(&admin, vec!["TV", "Music", "Movies"]), (&kid, vec!["TV", "Movies"])] {
        let sections = app.plex_json("/library/sections", token).await;
        assert_eq!(directory_titles(&sections), wanted);
        let providers = app.plex_json("/media/providers", token).await;
        assert_eq!(directory_titles(&providers["MediaProvider"][0]["Feature"][0]), wanted);
        let hubs = app.plex_json("/hubs", token).await;
        let rows: Vec<String> = wanted.iter().map(|name| format!("Recently Added in {name}")).collect();
        assert_eq!(titles(&hubs["Hub"]), rows);
    }
}

#[tokio::test]
async fn watch_state_reports() {
    let (app, admin, movies, tv, _) = world().await;
    let alien = app.ids(movies, &admin).await["Alien"];
    let show = app.ids(tv, &admin).await["Show"];
    let scrobble = |key: i64, action: &str| format!("/:/{action}?key={key}&identifier=com.plexapp.plugins.library");

    // Played and unplayed work on whole shows.
    assert_eq!(app.plex(Method::GET, &scrobble(show, "scrobble"), &admin, &[]).await.status, StatusCode::OK);
    let detail = app.plex_json(&format!("/library/metadata/{show}"), &admin).await;
    assert_eq!(
        (detail["Metadata"][0]["viewedLeafCount"].clone(), detail["Metadata"][0]["leafCount"].clone()),
        (json!(3), json!(3))
    );
    app.plex(Method::GET, &scrobble(show, "unscrobble"), &admin, &[]).await;
    let detail = app.plex_json(&format!("/library/metadata/{show}"), &admin).await;
    assert_eq!(detail["Metadata"][0]["viewedLeafCount"], json!(0));

    // Progress reports record the position, from `key` paths too.
    let progress = format!("/:/progress?key=/library/metadata/{alien}&time=125000&state=paused");
    assert_eq!(app.plex(Method::GET, &progress, &admin, &[]).await.status, StatusCode::OK);
    assert_eq!(app.get(&format!("/api/items/{alien}"), &admin).await.body["user_state"]["position_ms"], json!(125_000));
    let negative = format!("/:/timeline?ratingKey={alien}&time=-5&state=playing");
    app.plex(Method::GET, &negative, &admin, &[("x-plex-client-identifier", "tv")]).await;
    assert_eq!(app.get(&format!("/api/items/{alien}"), &admin).await.body["user_state"]["position_ms"], json!(0));
    // Containers can't report progress.
    let show_progress = format!("/:/timeline?ratingKey={show}&time=1000&state=playing");
    assert_eq!(app.plex(Method::GET, &show_progress, &admin, &[]).await.status, StatusCode::BAD_REQUEST);

    // A player that only ever says "stopped" never becomes a session.
    let sessions = || async { app.get("/api/activity", &admin).await.body["sessions"].as_array().unwrap().len() };
    let before = sessions().await;
    let stopped = format!("/:/timeline?ratingKey={alien}&time=1000&state=stopped");
    app.plex(Method::GET, &stopped, &admin, &[("x-plex-client-identifier", "ghost")]).await;
    assert_eq!(sessions().await, before);

    // Ratings: 0-10, negative clears, anything else is refused.
    let rate = |value: &str| format!("/:/rate?key={alien}&identifier=com.plexapp.plugins.library&rating={value}");
    assert_eq!(app.plex(Method::GET, &rate("11"), &admin, &[]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.plex(Method::GET, &rate("good"), &admin, &[]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.plex(Method::GET, &rate("7.6"), &admin, &[]).await.status, StatusCode::OK);
    assert_eq!(app.get(&format!("/api/items/{alien}"), &admin).await.body["user_state"]["rating"], json!(8.0));
}

#[tokio::test]
async fn hubs_and_on_deck() {
    let (app, admin, movies, tv, music) = world().await;
    let show = app.ids(tv, &admin).await["Show"];
    let seasons = app.children(show, &admin).await;
    let first = app.children(seasons["Season 1"], &admin).await;
    let finished = json!({"position_ms": 95_000, "duration_ms": 100_000});
    app.post(&format!("/api/items/{}/progress", first["Episode 1"]), &admin, finished).await;

    // The show knows what comes next.
    let detail = app.plex_json(&format!("/library/metadata/{show}?includeOnDeck=1"), &admin).await;
    assert_eq!(detail["Metadata"][0]["OnDeck"][0]["Metadata"][0]["title"], json!("Episode 2"));
    let continuing = app.plex_json("/hubs/continueWatching/items", &admin).await;
    assert_eq!(plex_titles(&continuing), ["Episode 2"]);

    // Section hubs: what has content, music without continue watching.
    let identifiers = |hubs: &Value| -> Vec<String> {
        hubs["Hub"].as_array().unwrap().iter().map(|h| h["hubIdentifier"].as_str().unwrap().to_owned()).collect()
    };
    assert_eq!(
        identifiers(&app.plex_json(&format!("/hubs/sections/{tv}"), &admin).await),
        ["tv.inprogress", "tv.recentlyadded"]
    );
    assert_eq!(identifiers(&app.plex_json(&format!("/hubs/sections/{music}"), &admin).await), ["music.recentlyadded"]);
    let counted = app.plex_json(&format!("/hubs/sections/{movies}?count=2"), &admin).await;
    assert_eq!(counted["Hub"][0]["size"], json!(2));

    // Home hubs, narrowed to some sections or hubs.
    let home = app.plex_json(&format!("/hubs?contentDirectoryID={music}"), &admin).await;
    assert_eq!(identifiers(&home), ["home.music.recent"]);
    let only = app.plex_json("/hubs?identifier=home.continue", &admin).await;
    assert_eq!(identifiers(&only), ["home.continue"]);
    let recent = app.plex_json(&format!("/hubs/home/recentlyAdded?type=1&sectionID={movies}"), &admin).await;
    assert_eq!(recent["size"], json!(4));
}

#[tokio::test]
async fn sidecar_subtitles_are_served_as_they_are() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 1024);
    let srt = "1\n00:00:01,000 --> 00:00:02,000\nHello\n";
    std::fs::write(app.media.path().join("movies/Alien (1979)/Alien (1979).en.srt"), srt).unwrap();
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let alien = app.ids(movies, &admin).await["Alien"];
    let file = app.file_of(alien).await;

    let detail = app.plex_json(&format!("/library/metadata/{alien}"), &admin).await;
    let streams = detail["Metadata"][0]["Media"][0]["Part"][0]["Stream"].as_array().unwrap().clone();
    let subtitle = streams.iter().find(|s| s["streamType"] == 3).expect("the sidecar is listed");
    assert_eq!(subtitle["id"], json!(file * 10_000 + 1000));
    let key = subtitle["key"].as_str().unwrap();
    let served = app.plex(Method::GET, key, &admin, &[]).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.text(), srt);
    // Ids of other files' streams don't reach this one.
    let elsewhere =
        app.plex(Method::GET, &format!("/library/streams/{}", (file + 1) * 10_000 + 1000), &admin, &[]).await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
}
