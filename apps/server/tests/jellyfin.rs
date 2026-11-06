//! The Jellyfin-compatible API (src/jellyfin, docs/COMPATIBILITY.md): signing
//! in, browsing, playing and keeping watch state as Jellyfin's apps do, the
//! ids they are given, the socket, and the API on Pley's own port.

mod common;

use std::collections::HashSet;

use axum::http::{Method, StatusCode, header};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};

use common::{Reply, TestApp, jf_names};

/// What Findroid sends with every request before it has a token.
const FINDROID: &str = r#"MediaBrowser Client="Findroid", Device="Phone", DeviceId="phone-1", Version="0.16.1""#;

async fn sign_in(app: &TestApp, body: Value) -> Reply {
    let headers = [("authorization", FINDROID), ("user-agent", "Findroid/1.2.0")];
    app.jf_raw(Method::POST, "/Users/AuthenticateByName", &headers, Some(body)).await
}

async fn sessions_of(app: &TestApp, user_id: i64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

#[tokio::test]
async fn signing_in_gives_a_token_for_this_api_and_no_session() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let before = sessions_of(&app, 1).await;

    let signed_in = sign_in(&app, json!({"Username": "admin", "Pw": "secret-one"})).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let token = signed_in.body["AccessToken"].as_str().unwrap().to_owned();
    assert!(token.starts_with("pley_ct_"), "{token}");
    let server = app.jf_json("/System/Info/Public", "").await["Id"].clone();
    assert_eq!(signed_in.body["ServerId"], server);
    let user = &signed_in.body["User"];
    assert_eq!((&user["Name"], &user["Id"]), (&json!("admin"), &json!(app.jf_user(&admin).await)));
    assert_eq!(user["Policy"]["IsAdministrator"], json!(false), "nothing administers here");
    assert_eq!(signed_in.body["SessionInfo"]["DeviceId"], json!("phone-1"));
    assert!(!signed_in.headers.contains_key(header::SET_COOKIE));
    assert_eq!(sessions_of(&app, 1).await, before, "no session of Pley's");

    // The token is this API's, named after the app and its device.
    let me = app.jf_raw(Method::GET, "/Users/Me", &[("x-emby-token", &token)], None).await;
    assert_eq!((me.status, &me.body["Name"]), (StatusCode::OK, &json!("admin")));
    assert_eq!(app.get("/api/auth/me", &token).await.status, StatusCode::UNAUTHORIZED);
    let listed = app.get("/api/me/compatibility-tokens", &admin).await.body;
    let findroid = listed.as_array().unwrap().iter().find(|t| t["name"] == "Findroid on Phone").expect("listed");
    assert_eq!(findroid["device"], "Findroid/1.2.0", "from where it signed in, at once");
    assert!(findroid["last_used_at"].is_i64());

    // Older apps send `Password`; names are trimmed as Pley's sign-in trims them.
    let older = sign_in(&app, json!({"username": " admin ", "Password": "secret-one"})).await;
    assert_eq!(older.status, StatusCode::OK, "{}", older.text());

    // Signing out ends the token it was sent with, in the query too.
    let older = older.body["AccessToken"].as_str().unwrap().to_owned();
    let out = app.jf_raw(Method::POST, &format!("/Sessions/Logout?api_key={older}"), &[], None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = app.jf_raw(Method::GET, "/Users/Me", &[("x-emby-token", &older)], None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    let me = app.jf_raw(Method::GET, "/Users/Me", &[("x-emby-token", &token)], None).await;
    assert_eq!(me.status, StatusCode::OK, "the other sign-in goes on");

    for wrong in [
        json!({"Username": "admin", "Pw": "wrong-one"}),
        json!({"Username": "nobody", "Pw": "secret-one"}),
        json!({"Username": "admin"}),
        json!({}),
    ] {
        let refused = sign_in(&app, wrong.clone()).await;
        assert_eq!(refused.status, StatusCode::UNAUTHORIZED, "{wrong}");
        assert!(refused.body.get("AccessToken").is_none());
    }
}

#[tokio::test]
async fn signing_in_takes_every_check_pleys_sign_in_does() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;

    // A disabled account.
    let (_, carol) = app.user(&admin, "carol", json!({})).await;
    app.patch(&format!("/api/users/{carol}"), &admin, json!({"disabled": true})).await;
    let disabled = sign_in(&app, json!({"Username": "carol", "Pw": "test-pass-1"})).await;
    assert_eq!(disabled.status, StatusCode::FORBIDDEN);

    // Two-step sign-in: an app has nowhere to type the code.
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    app.turn_on_two_step(&bob, "test-pass-1").await;
    let two_step = sign_in(&app, json!({"Username": "bob", "Pw": "test-pass-1"})).await;
    assert_eq!(two_step.status, StatusCode::UNAUTHORIZED);
    assert!(two_step.body.get("AccessToken").is_none());

    // Sent as text, as another site's page may without asking the server
    // first: refused before the password is tried, so it locks no one out.
    app.user(&admin, "dave", json!({})).await;
    for _ in 0..6 {
        let req = axum::http::Request::builder()
            .method(Method::POST)
            .uri("/Users/AuthenticateByName")
            .header("content-type", "text/plain;charset=UTF-8")
            .body(axum::body::Body::from(json!({"Username": "dave", "Pw": "wrong-pass"}).to_string()))
            .unwrap();
        let status = tower::ServiceExt::oneshot(app.jellyfin.clone(), req).await.unwrap().status();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let right = sign_in(&app, json!({"Username": "dave", "Pw": "test-pass-1"})).await;
    assert_eq!(right.status, StatusCode::OK, "{}", right.text());

    // Wrong passwords count as any sign-in's: the right one waits too.
    for _ in 0..5 {
        let wrong = sign_in(&app, json!({"Username": "dave", "Pw": "wrong-pass"})).await;
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    }
    let right = sign_in(&app, json!({"Username": "dave", "Pw": "test-pass-1"})).await;
    assert_ne!(right.status, StatusCode::OK, "{}", right.text());
    assert!(right.body.get("AccessToken").is_none());

    // Required of everyone: an account without it can't sign in this way.
    let (_, erin) = app.user(&admin, "erin", json!({})).await;
    app.turn_on_two_step(&admin, "secret-one").await;
    let required = app.put("/api/settings", &admin, json!({"require_two_step": true})).await;
    assert_eq!(required.status, StatusCode::OK, "{}", required.text());
    let required = sign_in(&app, json!({"Username": "erin", "Pw": "test-pass-1"})).await;
    assert_eq!(required.status, StatusCode::UNAUTHORIZED);
    assert_eq!(sessions_of(&app, erin).await, 0);
}

#[tokio::test]
async fn the_server_says_what_it_is_and_paths_go_in_any_case() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let info = app.jf_json("/System/Info/Public", "").await;
    assert_eq!((&info["ProductName"], &info["Version"]), (&json!("Jellyfin Server"), &json!("12.4.0")));
    assert_eq!(info["StartupWizardCompleted"], json!(true));
    assert!(is_hex(info["Id"].as_str().unwrap(), 32), "{info}");
    for path in ["/system/info/public", "/SYSTEM/INFO/PUBLIC/", "/System/Info/Public?format=json"] {
        assert_eq!(app.jf_json(path, "").await, info, "{path}");
    }
    assert_eq!(app.jf(Method::GET, "/System/Info", "", None).await.status, StatusCode::UNAUTHORIZED);
    let full = app.jf_json("/System/Info", &admin).await;
    assert_eq!(full["Id"], info["Id"]);
    // Jellyfin's apps read these as they must be there.
    for field in ["HasPendingRestart", "IsShuttingDown", "SupportsLibraryMonitor", "WebSocketPortNumber"] {
        assert!(full.get(field).is_some(), "{field}: {full}");
    }
    let behind = app
        .jf_raw(
            Method::GET,
            "/System/Info",
            &[("x-emby-token", &app.jellyfin_token(&admin).await), ("host", "media.example.com:8920")],
            None,
        )
        .await;
    assert_eq!(behind.body["WebSocketPortNumber"], json!(8920), "the port the app used");
    assert_eq!(app.jf(Method::GET, "/System/Ping", "", None).await.status, StatusCode::OK);
    // Apps time their connection by it, signed in.
    let test = app.jf(Method::GET, "/Playback/BitrateTest?size=300000", &admin, None).await;
    assert_eq!((test.status, test.headers[header::CONTENT_LENGTH].to_str().unwrap()), (StatusCode::OK, "300000"));
    assert_eq!(app.jf(Method::GET, "/Playback/BitrateTest?size=10", "", None).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.jf(Method::GET, "/Nothing/Here", &admin, None).await.status, StatusCode::NOT_FOUND);
    // Nothing it answers is a page.
    let reply = app.jf(Method::GET, "/Nothing/Here", "", None).await;
    assert!(reply.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("sandbox"));
    assert_eq!(reply.headers[header::CONTENT_TYPE], "text/plain; charset=utf-8");
}

/// Three films and a show of two episodes, with a sidecar subtitle.
struct World {
    app: TestApp,
    admin: String,
    movies: i64,
    tv: i64,
    film: i64,
    show: i64,
    pilot: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alpha (1979)", "Bravo (1985)", "Charlie (1942)"] {
        app.media_file(&format!("movies/{name}/{name}.mkv"), 2048);
    }
    app.media_file("movies/Alpha (1979)/Alpha (1979).en.srt", 64);
    app.media_file("tv/Show (2010)/Season 01/Show - S01E01 - Pilot.mkv", 2048);
    app.media_file("tv/Show (2010)/Season 01/Show - S01E02 - Second.mkv", 3072);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let film = app.ids(movies, &admin).await["Alpha"];
    let show = app.ids(tv, &admin).await["Show"];
    let season = app.children(show, &admin).await["Season 1"];
    let pilot = app.children(season, &admin).await["Pilot"];
    sqlx::query("UPDATE items SET duration_ms = 100000").execute(&app.state.db.write).await.unwrap();
    World { app, admin, movies, tv, film, show, pilot }
}

#[tokio::test]
async fn libraries_list_with_filters_sorts_and_pages() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let views = app.jf_json("/UserViews", admin).await;
    assert_eq!(jf_names(&views), ["Movies", "TV"]);
    let kinds: Vec<&Value> = views["Items"].as_array().unwrap().iter().map(|v| &v["CollectionType"]).collect();
    assert_eq!(kinds, [&json!("movies"), &json!("tvshows")]);

    let movies = app.jf_library(w.movies);
    let all =
        format!("/Items?ParentId={movies}&IncludeItemTypes=Movie&Recursive=true&SortBy=SortName&SortOrder=Descending");
    let listed = app.jf_json(&all, admin).await;
    assert_eq!(jf_names(&listed), ["Charlie", "Bravo", "Alpha"]);
    assert_eq!(listed["TotalRecordCount"], json!(3));
    let page = app.jf_json(&format!("{all}&StartIndex=1&Limit=1"), admin).await;
    assert_eq!(
        (jf_names(&page), &page["StartIndex"], &page["TotalRecordCount"]),
        (vec!["Bravo".to_owned()], &json!(1), &json!(3))
    );
    // Parameters in any case, lists joined by commas.
    let lower =
        format!("/items?parentid={movies}&includeitemtypes=Movie&recursive=true&sortby=ProductionYear,SortName");
    assert_eq!(jf_names(&app.jf_json(&lower, admin).await), ["Charlie", "Alpha", "Bravo"]);
    let listed_years = app.jf_json(&format!("/Years?ParentId={movies}"), admin).await;
    assert_eq!(jf_names(&listed_years), ["1985", "1979", "1942"]);
    assert_eq!(listed_years["Items"][0]["Type"], json!("Year"));
    let years = format!("/Items?ParentId={movies}&IncludeItemTypes=Movie&Recursive=true&Years=1979,1985");
    assert_eq!(jf_names(&app.jf_json(&years, admin).await), ["Alpha", "Bravo"]);
    let latest = app.jf_json(&format!("/Items/Latest?ParentId={movies}&Limit=2"), admin).await;
    assert_eq!(latest.as_array().unwrap().len(), 2, "a bare list");

    // A show's seasons and episodes, and an episode's place in them.
    let show = app.jf_item(w.show);
    assert_eq!(jf_names(&app.jf_json(&format!("/Shows/{show}/Seasons"), admin).await), ["Season 1"]);
    let episodes = app.jf_json(&format!("/Shows/{show}/Episodes"), admin).await;
    assert_eq!(jf_names(&episodes), ["Pilot", "Second"]);
    let pilot = &episodes["Items"][0];
    assert_eq!(
        (&pilot["Type"], &pilot["SeriesName"], &pilot["IndexNumber"]),
        (&json!("Episode"), &json!("Show"), &json!(1))
    );
    assert_eq!((&pilot["ParentIndexNumber"], &pilot["SeriesId"]), (&json!(1), &json!(show)));
    let tv = app.jf_library(w.tv);
    let recursive = app.jf_json(&format!("/Items?ParentId={tv}&IncludeItemTypes=Episode&Recursive=true"), admin).await;
    assert_eq!(recursive["TotalRecordCount"], json!(2));
    // Several kinds of one library, merged, then a page of them; a list
    // given as the parameter repeated too.
    let mixed = format!("/Items?ParentId={tv}&IncludeItemTypes=Series,Episode&Recursive=true&SortBy=SortName");
    let whole = jf_names(&app.jf_json(&mixed, admin).await);
    assert_eq!(whole, ["Pilot", "Second", "Show"]);
    let page = app.jf_json(&format!("{mixed}&StartIndex=1&Limit=2"), admin).await;
    assert_eq!((jf_names(&page), &page["TotalRecordCount"]), (whole[1..].to_vec(), &json!(3)));
    let repeated = format!("/Items?ParentId={tv}&IncludeItemTypes=Series&IncludeItemTypes=Episode&Recursive=true");
    assert_eq!(app.jf_json(&repeated, admin).await["TotalRecordCount"], json!(3));
    // A random pick is any of them, not the first in order.
    let random = format!("/Items?ParentId={movies}&IncludeItemTypes=Movie&Recursive=true&SortBy=Random&Limit=1");
    let mut picked = HashSet::new();
    for _ in 0..40 {
        let one = app.jf_json(&random, admin).await;
        assert_eq!(one["TotalRecordCount"], json!(3));
        picked.extend(jf_names(&one));
    }
    assert!(picked.len() > 1, "{picked:?}");

    // Search, and a search for titles and collections.
    assert_eq!(jf_names(&app.jf_json("/Items?SearchTerm=bravo&Recursive=true", admin).await), ["Bravo"]);
    let with_boxsets = "/Items?SearchTerm=bravo&IncludeItemTypes=Movie,BoxSet&Recursive=true";
    assert_eq!(jf_names(&app.jf_json(with_boxsets, admin).await), ["Bravo"]);
    let hints = app.jf_json("/Search/Hints?SearchTerm=alpha", admin).await;
    assert_eq!(hints["SearchHints"][0]["ItemId"], json!(app.jf_item(w.film)));
}

#[tokio::test]
async fn ids_are_this_servers_own_and_cant_be_guessed() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let film = app.jf_item(w.film);
    assert!(is_hex(&film, 32), "{film}");
    let listed = app.jf_json(&format!("/Items?Ids={film}"), admin).await;
    assert_eq!(listed["Items"][0]["Id"], json!(film));

    // Written with dashes or in capitals, as some apps do.
    let dashed = format!("{}-{}-{}-{}-{}", &film[..8], &film[8..12], &film[12..16], &film[16..20], &film[20..]);
    for id in [dashed.clone(), dashed.to_uppercase(), film.to_uppercase()] {
        assert_eq!(app.jf_json(&format!("/Items/{id}"), admin).await["Name"], json!("Alpha"), "{id}");
    }
    // Anything else names nothing: another key's ids, a changed one, a number.
    let other = TestApp::new().await;
    let others = other.jf_item(w.film);
    assert_ne!(others, film, "another server's key");
    let flipped = format!("{}{}", &film[..31], if film.ends_with('0') { '1' } else { '0' });
    let library = app.jf_library(w.movies);
    for uri in [
        format!("/Items/{others}"),
        format!("/Items/{flipped}"),
        format!("/Items/{}", w.film),
        "/Items/00000000000000000000000000000000".to_owned(),
        format!("/Items/{library}/PlaybackInfo"),
        format!("/Videos/{library}/stream?static=true"),
        format!("/Items/{flipped}/Images/Primary"),
    ] {
        let reply = app.jf(Method::GET, &uri, admin, None).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}: {}", reply.text());
    }
}

#[tokio::test]
async fn details_carry_what_apps_need_and_never_where_files_are() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let film = app.jf_item(w.film);
    let item = app.jf_json(&format!("/Items/{film}"), admin).await;
    assert_eq!(
        (&item["Type"], &item["MediaType"], &item["IsFolder"]),
        (&json!("Movie"), &json!("Video"), &json!(false))
    );
    assert_eq!((&item["ProductionYear"], &item["RunTimeTicks"]), (&json!(1979), &json!(1_000_000_000_i64)));
    assert_eq!(item["ServerId"], app.jf_json("/System/Info/Public", "").await["Id"]);
    let data = &item["UserData"];
    for field in ["PlaybackPositionTicks", "PlayCount", "IsFavorite", "Played", "Key"] {
        assert!(data.get(field).is_some(), "UserData.{field}: {data}");
    }
    let source = &item["MediaSources"][0];
    assert!(is_hex(source["Id"].as_str().unwrap(), 64), "{source}");
    assert_eq!((&source["Path"], &source["Protocol"]), (&json!("/Alpha (1979).mkv"), &json!("File")));
    let subtitle = source["MediaStreams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["Type"] == "Subtitle")
        .expect("the sidecar subtitle");
    assert_eq!(subtitle["IsExternal"], json!(true));
    let delivery = subtitle["DeliveryUrl"].as_str().unwrap();
    assert!(
        delivery.starts_with(&format!("/Videos/{film}/{}/Subtitles/", source["Id"].as_str().unwrap())),
        "{delivery}"
    );
    let folder = app.media_path("movies");
    assert!(!item.to_string().contains(&folder.replace('\\', "\\\\")) && !item.to_string().contains(&folder));
    // Each sign-in's sources are its own.
    let (bob, _) = app.user(admin, "bob", json!({})).await;
    let bobs = app.jf_json(&format!("/Items/{film}"), &bob).await;
    assert_ne!(bobs["MediaSources"][0]["Id"], source["Id"]);
}

#[tokio::test]
async fn a_stream_opens_by_its_source_while_its_sign_in_works() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let signed_in = sign_in(app, json!({"Username": "admin", "Pw": "secret-one"})).await;
    let token = signed_in.body["AccessToken"].as_str().unwrap().to_owned();
    let film = app.jf_item(w.film);
    let info = app
        .jf_raw(Method::POST, &format!("/Items/{film}/PlaybackInfo"), &[("x-emby-token", &token)], Some(json!({})))
        .await;
    assert_eq!(info.status, StatusCode::OK, "{}", info.text());
    let source = &info.body["MediaSources"][0];
    assert_eq!(source["SupportsDirectPlay"], json!(true), "no device profile: plays anything");
    assert!(source.get("TranscodingUrl").is_none());
    let source = source["Id"].as_str().unwrap().to_owned();

    // Players open it without the token, and seek with ranges.
    let stream = format!("/Videos/{film}/stream.mkv?static=true&MediaSourceId={source}");
    let whole = app.jf_raw(Method::GET, &stream, &[], None).await;
    assert_eq!((whole.status, whole.headers[header::CONTENT_LENGTH].to_str().unwrap()), (StatusCode::OK, "2048"));
    let part = app.jf_raw(Method::GET, &stream, &[("range", "bytes=1000-")], None).await;
    assert_eq!(part.status, StatusCode::PARTIAL_CONTENT);
    // As some take the source's id for the item's.
    assert_eq!(app.jf_raw(Method::GET, &format!("/Videos/{source}/stream"), &[], None).await.status, StatusCode::OK);
    let download = app.jf_raw(Method::GET, &format!("/Items/{film}/Download?api_key={token}"), &[], None).await;
    assert!(download.headers[header::CONTENT_DISPOSITION].to_str().unwrap().starts_with("attachment"));
    let subtitles = format!("/Videos/{film}/{source}/Subtitles/1000/Stream.vtt");
    assert_eq!(app.jf_raw(Method::GET, &subtitles, &[], None).await.status, StatusCode::OK);

    // Signed out: the source's id opens nothing.
    let out = app.jf_raw(Method::POST, "/Sessions/Logout", &[("x-emby-token", &token)], None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    for uri in [stream.clone(), subtitles.clone()] {
        assert_eq!(app.jf_raw(Method::GET, &uri, &[], None).await.status, StatusCode::UNAUTHORIZED, "{uri}");
    }
    // Nor while the account is disabled.
    let (bob, bob_id) = app.user(admin, "bob", json!({})).await;
    let bobs = app.jf_source(app.file_of(w.film).await, &bob).await;
    let bobs_stream = format!("/Videos/{film}/stream?MediaSourceId={bobs}");
    assert_eq!(app.jf_raw(Method::GET, &bobs_stream, &[], None).await.status, StatusCode::OK);
    app.patch(&format!("/api/users/{bob_id}"), admin, json!({"disabled": true})).await;
    assert_eq!(app.jf_raw(Method::GET, &bobs_stream, &[], None).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn watch_state_favorites_and_ratings_are_pleys() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let film = app.jf_item(w.film);
    let pley_state = || async { app.get(&format!("/api/items/{}", w.film), admin).await.body["user_state"].clone() };

    let played = app.jf(Method::POST, &format!("/UserPlayedItems/{film}"), admin, None).await;
    assert_eq!((played.status, &played.body["Played"]), (StatusCode::OK, &json!(true)));
    assert_eq!(pley_state().await["watched"], json!(true));
    let unplayed = app.jf(Method::DELETE, &format!("/UserPlayedItems/{film}"), admin, None).await;
    assert_eq!(unplayed.body["Played"], json!(false));

    // Favorites are My List, which holds movies and shows.
    let favorite = app.jf(Method::POST, &format!("/UserFavoriteItems/{film}"), admin, None).await;
    assert_eq!(favorite.body["IsFavorite"], json!(true));
    assert_eq!(common::titles(&app.get("/api/watchlist", admin).await.body["items"]), ["Alpha"]);
    let favorites = app.jf_json("/Items?Filters=IsFavorite&Recursive=true&IncludeItemTypes=Movie,Series", admin).await;
    assert_eq!(jf_names(&favorites), ["Alpha"]);
    let episode = app.jf(Method::POST, &format!("/UserFavoriteItems/{}", app.jf_item(w.pilot)), admin, None).await;
    assert_eq!(episode.status, StatusCode::BAD_REQUEST);
    app.jf(Method::DELETE, &format!("/UserFavoriteItems/{film}"), admin, None).await;
    assert!(common::titles(&app.get("/api/watchlist", admin).await.body["items"]).is_empty());

    // A thumb up is Pley's 10, and cleared.
    let liked = app.jf(Method::POST, &format!("/UserItems/{film}/Rating?Likes=true"), admin, None).await;
    assert_eq!((&liked.body["Likes"], &liked.body["Rating"]), (&json!(true), &json!(10.0)));
    let cleared = app.jf(Method::DELETE, &format!("/UserItems/{film}/Rating"), admin, None).await;
    assert!(cleared.body.get("Likes").is_none());

    // A player's reports keep the place, which Resume lists; clearing it ends it.
    let report = json!({"ItemId": film, "PositionTicks": 600_000_000_i64, "IsPaused": false});
    assert_eq!(app.jf(Method::POST, "/Sessions/Playing", admin, Some(report)).await.status, StatusCode::NO_CONTENT);
    let stopped = json!({"ItemId": film, "PositionTicks": 600_000_000_i64});
    app.jf(Method::POST, "/Sessions/Playing/Stopped", admin, Some(stopped)).await;
    let resume = app.jf_json("/UserItems/Resume", admin).await;
    assert_eq!(jf_names(&resume), ["Alpha"]);
    assert_eq!(resume["Items"][0]["UserData"]["PlaybackPositionTicks"], json!(600_000_000_i64));
    assert_eq!(pley_state().await["position_ms"], json!(60_000));
    // Starting again, before the player goes to the place: it stays.
    for start in [json!({"ItemId": film}), json!({"ItemId": film, "PositionTicks": 0})] {
        app.jf(Method::POST, "/Sessions/Playing", admin, Some(start)).await;
        assert_eq!(pley_state().await["position_ms"], json!(60_000));
    }
    let body = json!({"PlaybackPositionTicks": 0});
    let reset = app.jf(Method::POST, &format!("/UserItems/{film}/UserData"), admin, Some(body)).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert!(jf_names(&app.jf_json("/UserItems/Resume", admin).await).is_empty());
}

#[tokio::test]
async fn a_user_id_in_a_path_is_the_callers_own() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let (bob, _) = app.user(admin, "bob", json!({})).await;
    let (mine, bobs) = (app.jf_user(admin).await, app.jf_user(&bob).await);
    let film = app.jf_item(w.film);
    for (uri, ok) in [
        (format!("/Users/{mine}/Views"), true),
        (format!("/Users/{mine}/Items/{film}"), true),
        (format!("/Users/{mine}/Items?Recursive=true&IncludeItemTypes=Movie"), true),
        (format!("/Users/{bobs}/Views"), false),
        (format!("/Users/{bobs}/Items/{film}"), false),
        (format!("/Users/{bobs}/Items?Recursive=true&IncludeItemTypes=Movie"), false),
        (format!("/Users/{bobs}/Items/Latest"), false),
        (format!("/Users/{bobs}/Suggestions"), false),
        (format!("/Users/{bobs}/Items/Resume"), false),
    ] {
        let status = app.jf(Method::GET, &uri, admin, None).await.status;
        assert_eq!(status == StatusCode::OK, ok, "{uri}: {status}");
    }
}

/// Jellyfin's socket on a real port, for `token`.
async fn socket(
    app: &TestApp,
    token: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(pley::jellyfin::serve(app.state.clone(), listener, std::future::pending()));
    let url = format!("ws://{addr}/socket?api_key={token}&deviceId=bench-1");
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

/// The next message of a type among `wanted`, within 10 s.
async fn next_of(
    ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    wanted: &[&str],
) -> Value {
    let read = async {
        while let Some(message) = ws.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = message {
                let value: Value = serde_json::from_str(&text).unwrap();
                if wanted.contains(&value["MessageType"].as_str().unwrap_or_default()) {
                    return value;
                }
            }
        }
        panic!("the socket closed");
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), read).await.expect("a message in time")
}

#[tokio::test]
async fn the_socket_keeps_alive_and_tells_the_users_devices() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let token = app.jellyfin_token(admin).await;
    let mut ws = socket(app, &token).await;
    let hello = next_of(&mut ws, &["ForceKeepAlive"]).await;
    assert_eq!(hello["Data"], json!(60));
    ws.send(tokio_tungstenite::tungstenite::Message::Text(r#"{"MessageType":"KeepAlive"}"#.into())).await.unwrap();
    next_of(&mut ws, &["KeepAlive"]).await;

    // Its watch state, as UserDataChanged.
    let film = app.jf_item(w.film);
    app.post(&format!("/api/items/{}/watched", w.film), admin, json!({})).await;
    let changed = next_of(&mut ws, &["UserDataChanged"]).await;
    assert_eq!(changed["Data"]["UserId"], json!(app.jf_user(admin).await));
    assert!(changed["Data"]["UserDataList"].as_array().unwrap().iter().any(|d| d["ItemId"] == json!(film)));

    // Playback this device reported, stopped from elsewhere: Playstate Stop.
    let report = json!({"ItemId": film, "PositionTicks": 10_000_000});
    app.jf(Method::POST, "/Sessions/Playing", admin, Some(report)).await;
    let session = app.state.sessions.list()[0].id.clone();
    assert_eq!(app.delete(&format!("/api/sessions/{session}"), admin).await.status, StatusCode::NO_CONTENT);
    let stop = next_of(&mut ws, &["Playstate"]).await;
    assert_eq!(stop["Data"]["Command"], json!("Stop"));

    // A socket needs a token for this API.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(pley::jellyfin::serve(app.state.clone(), listener, std::future::pending()));
    for token in ["", admin] {
        let refused = tokio_tungstenite::connect_async(format!("ws://{addr}/socket?api_key={token}")).await;
        assert!(refused.is_err(), "{token:?}");
    }
}

#[tokio::test]
async fn on_pleys_port_its_requests_go_to_it_and_the_rest_to_pley() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let token = app.jellyfin_token(&admin).await;
    let product = |reply: &Reply| reply.body["ProductName"].clone();

    // Off by default: Pley's port answers as Pley.
    let off = app.call(Method::GET, "/System/Info/Public", None, None, &[]).await;
    assert_eq!(product(&off), Value::Null);

    // A port nothing listens on: this app serves no port of its own.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let web = format!("127.0.0.1:{port}");
    let on = json!({"web": [&web], "jellyfin": [], "jellyfin_on_web": true, "discovery": false});
    assert_eq!(app.put("/api/network", &admin, on).await.status, StatusCode::OK);
    for path in ["/System/Info/Public", "/system/info/public"] {
        let reply = app.call(Method::GET, path, None, None, &[]).await;
        assert_eq!(product(&reply), json!("Jellyfin Server"), "{path}");
    }
    let emby = [("x-emby-token", token.as_str())];
    let views = app.call(Method::GET, "/UserViews", None, None, &emby).await;
    assert_eq!(views.status, StatusCode::OK, "{}", views.text());
    // Paths both have: a Jellyfin app's header or token decides.
    let hints = app.call(Method::GET, "/Search/Hints?SearchTerm=x", None, None, &emby).await;
    assert_eq!(hints.body["TotalRecordCount"], json!(0));
    let page = app.call(Method::GET, "/search?q=x", None, None, &[]).await;
    assert!(page.body.get("SearchHints").is_none(), "the web app's page");
    assert!(page.headers[header::VARY].to_str().unwrap().to_ascii_lowercase().contains("authorization"));
    // Pley's own API is Pley's, whatever is sent.
    let health = app.call(Method::GET, "/api/health", None, None, &emby).await;
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(app.call(Method::GET, "/api/auth/me", None, None, &emby).await.status, StatusCode::UNAUTHORIZED);

    // Off again.
    let off = json!({"web": [&web], "jellyfin": [], "jellyfin_on_web": false, "discovery": false});
    assert_eq!(app.put("/api/network", &admin, off).await.status, StatusCode::OK);
    assert_eq!(product(&app.call(Method::GET, "/System/Info/Public", None, None, &[]).await), Value::Null);
}

#[tokio::test]
async fn every_part_of_the_api_is_reached_on_pleys_port() {
    let app = TestApp::with_config(|config| config.jellyfin_on_web = true).await;
    let admin = app.setup_admin().await;
    let token = app.jellyfin_token(&admin).await;
    let x = "00000000000000000000000000000000";
    let theirs = |reply: &Reply| {
        reply.headers.get(header::CONTENT_SECURITY_POLICY).and_then(|v| v.to_str().ok())
            == Some("default-src 'none'; frame-ancestors 'none'; sandbox")
    };
    // Every first segment of its routes, as players send them: no header.
    for path in [
        "/UserViews".to_owned(),
        "/Users/Me".to_owned(),
        "/Items".to_owned(),
        "/UserItems/Resume".to_owned(),
        "/UserPlayedItems/x".to_owned(),
        "/UserFavoriteItems/x".to_owned(),
        "/Shows/NextUp".to_owned(),
        "/Genres".to_owned(),
        "/MusicGenres".to_owned(),
        "/Artists".to_owned(),
        "/Studios".to_owned(),
        "/Persons".to_owned(),
        format!("/Videos/{x}/stream"),
        format!("/Audio/{x}/stream"),
        "/Sessions".to_owned(),
        format!("/MediaSegments/{x}"),
        "/socket".to_owned(),
        "/System/Info/Public".to_owned(),
        "/Branding/Configuration".to_owned(),
        "/QuickConnect/Enabled".to_owned(),
        "/Plugins".to_owned(),
        "/Localization/Cultures".to_owned(),
        "/DisplayPreferences/usersettings".to_owned(),
        "/Years".to_owned(),
        "/Tags".to_owned(),
        "/Playback/BitrateTest".to_owned(),
    ] {
        let reply = app.call(Method::GET, &path, None, None, &[]).await;
        assert!(theirs(&reply), "{path}: {} {}", reply.status, reply.text());
    }
    // Paths the web app has too: with a token only.
    for path in ["/Search/Hints?SearchTerm=x".to_owned(), format!("/Playlists/{x}/Items")] {
        let page = app.call(Method::GET, &path, None, None, &[]).await;
        assert!(!theirs(&page), "{path} without a token is the web app's");
        let reply = app.call(Method::GET, &path, None, None, &[("x-emby-token", token.as_str())]).await;
        assert!(theirs(&reply), "{path}: {} {}", reply.status, reply.text());
    }
}

// ------------------------------------------------------------------ remote control

/// What a browser playing Matroska directly says it plays.
const CAPS: &str = r#"{"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}"#;

type Events = axum::body::BodyDataStream;

/// An events stream of Pley's of `token`'s user, opened with `query` and
/// held: a receiver it names lasts as long as it is.
async fn events(app: &TestApp, token: &str, query: &str) -> Events {
    let request = axum::http::Request::builder()
        .uri(format!("/api/events?{query}"))
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().into_data_stream()
}

/// The next event of `kind` that `stream` sends, within 10 s.
async fn next_event(stream: &mut Events, kind: &str) -> Value {
    let read = async {
        let mut received = String::new();
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            while let Some(end) = received.find("\n\n") {
                let block: String = received.drain(..end + 2).collect();
                for line in block.lines() {
                    let Some(data) = line.strip_prefix("data:") else { continue };
                    let event: Value = serde_json::from_str(data.trim()).unwrap();
                    if event["type"] == kind {
                        return event;
                    }
                }
            }
        }
        panic!("the stream ended");
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), read).await.expect("an event in time")
}

/// Gives the film the streams a probe would find: H.264 video, two audio
/// tracks and subtitles.
async fn with_tracks(app: &TestApp, film: i64) {
    use pley_transcoder_api::{MediaInfo, MediaStream, StreamKind};
    let stream = |index, kind, codec: &str| MediaStream { index, kind, codec: codec.into(), ..Default::default() };
    let mut video = stream(0, StreamKind::Video, "h264");
    (video.width, video.height, video.pix_fmt) = (Some(1920), Some(1080), Some("yuv420p".into()));
    let mut subtitles = stream(3, StreamKind::Subtitle, "subrip");
    subtitles.text_based = Some(true);
    let info = MediaInfo {
        container: Some("mkv".into()),
        duration_ms: Some(100_000),
        streams: vec![video, stream(1, StreamKind::Audio, "aac"), stream(2, StreamKind::Audio, "aac"), subtitles],
        ..Default::default()
    };
    sqlx::query("UPDATE media_files SET media_info = ?, video_codec = 'h264', audio_codec = 'aac' WHERE id = ?")
        .bind(serde_json::to_string(&info).unwrap())
        .bind(app.file_of(film).await)
        .execute(&app.state.db.write)
        .await
        .unwrap();
}

/// The film played directly on `token`'s device named `device`, by a
/// player that obeys `controls`: its session id.
async fn pley_plays(app: &TestApp, token: &str, film: i64, device: &str, controls: Value) -> String {
    let body = json!({
        "item_id": film, "caps": serde_json::from_str::<Value>(CAPS).unwrap(),
        "controls": controls, "device_name": device,
    });
    let started = app.call(Method::POST, "/api/playback", Some(token), Some(body), &[]).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    started.body["session_id"].as_str().unwrap().to_owned()
}

/// What `token`'s user's Jellyfin app (device `bench-1`) lists at
/// `/Sessions`, with `query`.
async fn jf_sessions(app: &TestApp, token: &str, query: &str) -> Vec<Value> {
    app.jf_json(&format!("/Sessions{query}"), token).await.as_array().unwrap().clone()
}

/// The session id of the device named `name` among `sessions`.
fn named(sessions: &[Value], name: &str) -> String {
    let found = sessions.iter().find(|s| s["DeviceName"] == name);
    found.unwrap_or_else(|| panic!("{name}: {sessions:?}"))["Id"].as_str().unwrap().to_owned()
}

/// A viewer, with a TV that takes casts and a phone that plays the film.
struct Household {
    viewer: String,
    _tv: Events,
    session: String,
}

async fn household(w: &World) -> Household {
    let app = &w.app;
    let (viewer, viewer_id) = app.user(&w.admin, "viewer", json!({})).await;
    let tv = events(app, &viewer, "receiver=tv-1&device_name=Den%20TV&device_kind=tv&casts=item,live,switch").await;
    let (phone, _) = app.app_token(viewer_id, "library play").await;
    let every = json!(["play", "pause", "seek", "skip", "next", "previous", "audio", "subtitles"]);
    let session = pley_plays(app, &phone, w.film, "Phone", every).await;
    Household { viewer, _tv: tv, session }
}

#[tokio::test]
async fn sessions_list_the_users_devices_as_pleys_remote_does() {
    let w = world().await;
    let app = &w.app;
    let h = household(&w).await;
    // Another profile's TV, of the household: never listed.
    let _theirs = events(app, &w.admin, "receiver=tv-9&device_name=Attic&casts=item,switch").await;

    let listed = jf_sessions(app, &h.viewer, "").await;
    let names: Vec<&str> = listed.iter().map(|s| s["DeviceName"].as_str().unwrap_or_default()).collect();
    assert_eq!(names, ["Phone", "Bench", "Den TV"], "what plays first, then by name: {listed:?}");
    let ids: HashSet<&str> = listed.iter().map(|s| s["Id"].as_str().unwrap()).collect();
    assert!(ids.len() == 3 && ids.iter().all(|id| is_hex(id, 32)), "{ids:?}");
    let viewer = app.jf_user(&h.viewer).await;
    assert!(listed.iter().all(|s| s["UserId"] == json!(viewer) && s["UserName"] == "viewer"));

    // The phone, with what it plays and where.
    let phone = &listed[0];
    assert_eq!(phone["NowPlayingItem"]["Id"], json!(app.jf_item(w.film)));
    assert_eq!(phone["NowPlayingItem"]["Type"], "Movie");
    let state = &phone["PlayState"];
    assert_eq!((&state["IsPaused"], &state["CanSeek"]), (&json!(false), &json!(true)));
    assert_eq!(state["PlayMethod"], json!("DirectPlay"));
    assert!(state["PositionTicks"].is_i64(), "{state}");
    assert_eq!((&phone["Client"], &phone["SupportsRemoteControl"]), (&json!("Pley"), &json!(true)));
    let commands = phone["SupportedCommands"].as_array().unwrap();
    for command in ["SetAudioStreamIndex", "SetSubtitleStreamIndex", "PlayState"] {
        assert!(commands.contains(&json!(command)), "{command}: {commands:?}");
    }
    assert_eq!(phone["PlayableMediaTypes"], json!([]), "it takes no casts");
    // The TV takes them, and plays nothing.
    let tv = &listed[2];
    assert_eq!((&tv["PlayableMediaTypes"], tv.get("NowPlayingItem")), (&json!(["Audio", "Video"]), None));
    assert_eq!(tv["SupportedCommands"], json!(["Play"]));
    assert_eq!(tv["Capabilities"]["SupportsMediaControl"], json!(true));
    // The app's own device, as it names itself.
    let own = &listed[1];
    assert_eq!((&own["DeviceId"], &own["Client"]), (&json!("bench-1"), &json!("Test")));
    assert_eq!(own["ApplicationVersion"], json!("1.0"));
    assert_eq!(own["SupportsRemoteControl"], json!(false), "it never said it takes media control");
    assert_eq!(own["AdditionalUsers"], json!([]));

    // Its device alone, those it can control, those seen lately.
    assert_eq!(jf_sessions(app, &h.viewer, "?DeviceId=BENCH-1").await.len(), 1);
    let controllable = jf_sessions(app, &h.viewer, &format!("?ControllableByUserId={viewer}")).await;
    assert_eq!(controllable.len(), 2, "{controllable:?}");
    assert_eq!(jf_sessions(app, &h.viewer, "?ActiveWithinSeconds=60").await.len(), 3);
    let admins = app.jf_user(&w.admin).await;
    let refused = app.jf(Method::GET, &format!("/Sessions?ControllableByUserId={admins}"), &h.viewer, None).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "another user's devices are theirs");

    // Another profile of the household sees its own devices alone: a cast
    // there would switch the viewer's TV, which only Pley's own apps do.
    let theirs = jf_sessions(app, &w.admin, "").await;
    let names: Vec<Value> = theirs.iter().map(|s| s["DeviceName"].clone()).collect();
    assert_eq!(names, [json!("Attic"), json!("Bench")]);

    // A sign-in's session id is the one its device is listed by.
    let signed_in = sign_in(app, json!({"Username": "viewer", "Pw": "test-pass-1"})).await;
    let token = signed_in.body["AccessToken"].as_str().unwrap();
    let auth = format!(r#"{FINDROID}, Token="{token}""#);
    let mine = app.jf_raw(Method::GET, "/Sessions?DeviceId=phone-1", &[("authorization", &auth)], None).await;
    assert_eq!(mine.body[0]["Id"], signed_in.body["SessionInfo"]["Id"], "{}", mine.text());
    assert_eq!(mine.body[0]["DeviceName"], json!("Phone"));
}

#[tokio::test]
async fn a_jellyfin_app_casts_to_the_users_devices() {
    let w = world().await;
    let app = &w.app;
    let h = household(&w).await;
    let mut heard = events(app, &h.viewer, "").await;
    let listed = jf_sessions(app, &h.viewer, "").await;
    let (tv, phone, own) = (named(&listed, "Den TV"), named(&listed, "Phone"), named(&listed, "Bench"));
    let film = app.jf_item(w.film);
    let cast = |session: &str, query: String| {
        let uri = format!("/Sessions/{session}/Playing?{query}");
        let viewer = h.viewer.clone();
        async move { app.jf(Method::POST, &uri, &viewer, None).await }
    };

    // Played at once, from where it was asked, saying where it came from.
    let sent = cast(&tv, format!("PlayCommand=PlayNow&ItemIds={film}&StartPositionTicks=50000000")).await;
    assert_eq!(sent.status, StatusCode::NO_CONTENT, "{}", sent.text());
    let event = next_event(&mut heard, "cast").await;
    let expected = json!({
        "type": "cast", "receiver_id": "tv-1", "item_id": w.film, "playlist_id": null, "channel_id": null,
        "start_ms": 5_000, "shuffle": false, "audio_index": null, "subtitle_index": null, "from": "Bench",
    });
    assert_eq!(event, expected);

    // A playlist of the user's, shuffled: the item at StartIndex.
    let made = app.post("/api/playlists", &h.viewer, json!({"title": "Mine", "item_ids": [w.film]})).await;
    let playlist = app.jf_ids().of(pley::jellyfin::ids::Kind::Playlist, made.body["id"].as_i64().unwrap());
    let sent = cast(&tv, format!("playCommand=PlayShuffle&itemIds={film},{playlist}&startIndex=1")).await;
    assert_eq!(sent.status, StatusCode::NO_CONTENT, "{}", sent.text());
    let event = next_event(&mut heard, "cast").await;
    assert_eq!((&event["playlist_id"], &event["shuffle"]), (&made.body["id"], &json!(true)));

    // As Pley's casting checks it: a track the title doesn't have, a queue
    // it doesn't keep, nothing at StartIndex, a device that takes no casts,
    // an id that names nothing, a device that isn't the user's.
    let nothing = "0".repeat(32);
    for (session, query, status) in [
        (&tv, format!("PlayCommand=PlayNow&ItemIds={film}&AudioStreamIndex=7"), StatusCode::BAD_REQUEST),
        (&tv, format!("PlayCommand=PlayNext&ItemIds={film}"), StatusCode::BAD_REQUEST),
        (&tv, format!("PlayCommand=PlayNow&ItemIds={film}&StartIndex=1"), StatusCode::BAD_REQUEST),
        (&phone, format!("PlayCommand=PlayNow&ItemIds={film}"), StatusCode::BAD_REQUEST),
        (&own, format!("PlayCommand=PlayNow&ItemIds={film}"), StatusCode::BAD_REQUEST),
        (&tv, format!("PlayCommand=PlayNow&ItemIds={nothing}"), StatusCode::NOT_FOUND),
        (&nothing, format!("PlayCommand=PlayNow&ItemIds={film}"), StatusCode::NOT_FOUND),
    ] {
        let refused = cast(session, query.clone()).await;
        assert_eq!(refused.status, status, "{query}: {}", refused.text());
    }
    let admins_own = named(&jf_sessions(app, &w.admin, "").await, "Bench");
    let refused = cast(&admins_own, format!("PlayCommand=PlayNow&ItemIds={film}")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "another user's device");
}

#[tokio::test]
async fn a_jellyfin_app_controls_the_users_players() {
    let w = world().await;
    let app = &w.app;
    with_tracks(app, w.film).await;
    let h = household(&w).await;
    let mut heard = events(app, &h.viewer, "").await;
    let listed = jf_sessions(app, &h.viewer, "").await;
    let (tv, phone) = (named(&listed, "Den TV"), named(&listed, "Phone"));
    let post = |uri: String, body: Option<Value>| {
        let viewer = h.viewer.clone();
        async move { app.jf(Method::POST, &uri, &viewer, body).await }
    };

    // Jellyfin's Playstate commands, as Pley's remote sends them.
    for (command, sent, (field, value)) in [
        ("Pause", "pause", ("command", json!("pause"))),
        ("Unpause", "play", ("command", json!("play"))),
        ("PlayPause", "pause", ("command", json!("pause"))),
        ("Seek?SeekPositionTicks=300000000", "seek", ("position_ms", json!(30_000))),
        ("NextTrack", "next", ("command", json!("next"))),
        ("PreviousTrack", "previous", ("command", json!("previous"))),
        ("Rewind", "skip", ("offset_ms", json!(-10_000))),
        ("FastForward", "skip", ("offset_ms", json!(30_000))),
    ] {
        let reply = post(format!("/Sessions/{phone}/Playing/{command}"), None).await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{command}: {}", reply.text());
        let event = next_event(&mut heard, "player_command").await;
        assert_eq!((&event["session_id"], &event["command"]), (&json!(h.session), &json!(sent)), "{command}");
        assert_eq!(event[field], value, "{command}: {event}");
    }
    // Its general commands that Pley's remote has.
    let general = format!("/Sessions/{phone}/Command");
    let audio = json!({"Name": "SetAudioStreamIndex", "Arguments": {"Index": "2"}});
    assert_eq!(post(general.clone(), Some(audio)).await.status, StatusCode::NO_CONTENT);
    let event = next_event(&mut heard, "player_command").await;
    assert_eq!((&event["command"], &event["stream_index"]), (&json!("audio"), &json!(2)));
    let pause = json!({"Name": "PlayState", "Arguments": {"Command": "Pause"}});
    assert_eq!(post(general.clone(), Some(pause)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(next_event(&mut heard, "player_command").await["command"], json!("pause"));
    for refused in [
        json!({"Name": "SetAudioStreamIndex", "Arguments": {"Index": "3"}}),
        json!({"Name": "SetSubtitleStreamIndex"}),
        json!({"Name": "SetMaxStreamingBitrate", "Arguments": {"MaxBitrate": "8000000"}}),
        json!({"Name": "ChannelUp"}),
        json!({"Name": "DisplayMessage", "Arguments": {"Text": "hi"}}),
    ] {
        let reply = post(general.clone(), Some(refused.clone())).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{refused}: {}", reply.text());
    }
    let muted = post(format!("/Sessions/{phone}/Command/ToggleMute"), None).await;
    assert_eq!(muted.status, StatusCode::BAD_REQUEST);

    // Nothing plays on the TV: a stop stops nothing, and nothing else goes.
    assert_eq!(post(format!("/Sessions/{tv}/Playing/Pause"), None).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(post(format!("/Sessions/{tv}/Playing/Stop"), None).await.status, StatusCode::NO_CONTENT);
    // The phone stops as Pley's remote stops it.
    assert_eq!(post(format!("/Sessions/{phone}/Playing/Stop"), None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(next_event(&mut heard, "session_stopped").await["session_id"], json!(h.session));
    assert!(app.get("/api/sessions", &h.viewer).await.body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_socket_sends_the_users_sessions_once_asked() {
    let w = world().await;
    let app = &w.app;
    let (viewer, _) = app.user(&w.admin, "viewer", json!({})).await;
    let token = app.jellyfin_token(&viewer).await;
    let mut ws = socket(app, &token).await;
    next_of(&mut ws, &["ForceKeepAlive"]).await;
    let start = r#"{"MessageType":"SessionsStart","Data":"0,1000"}"#;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(start.into())).await.unwrap();
    let first = next_of(&mut ws, &["Sessions"]).await;
    assert_eq!(first["Data"].as_array().unwrap().len(), 1, "its own device: {first}");
    assert_eq!(first["Data"][0]["DeviceId"], json!("bench-1"));

    // A device that comes is told of.
    let _tv = events(app, &viewer, "receiver=tv-1&device_name=Den%20TV&casts=item").await;
    let next = next_of(&mut ws, &["Sessions"]).await;
    let names: Vec<Value> = next["Data"].as_array().unwrap().iter().map(|s| s["DeviceName"].clone()).collect();
    assert!(names.contains(&json!("Den TV")), "{next}");

    // Until it stops asking.
    let stop = r#"{"MessageType":"SessionsStop"}"#;
    ws.send(tokio_tungstenite::tungstenite::Message::Text(stop.into())).await.unwrap();
    let _kitchen = events(app, &viewer, "receiver=tv-2&device_name=Kitchen&casts=item").await;
    let quiet = tokio::time::timeout(std::time::Duration::from_secs(3), next_of(&mut ws, &["Sessions"])).await;
    assert!(quiet.is_err(), "no more after SessionsStop: {quiet:?}");
}
