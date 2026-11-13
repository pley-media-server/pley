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

/// What Infuse asks for as it loads its home screen, after the views: without
/// an answer, it shows "An error occurred". Its library mode looks for its
/// own plugin too.
#[tokio::test]
async fn apps_loading_their_home_screen_get_grouping_options_and_no_folders_or_plugins() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let music = app.library(admin, "Albums", "music", "music").await;
    let options = app.jf_json("/UserViews/GroupingOptions", admin).await;
    assert_eq!(jf_names(&options), ["Movies", "TV"], "movies and shows, by name: music isn't grouped");
    assert_eq!(options[0]["Id"], json!(app.jf_library(w.movies)));
    let views = jf_names(&app.jf_json("/UserViews", admin).await);
    assert!(views.contains(&"Albums".to_owned()), "{views:?}: {music}");
    let mine = app.jf_user(admin).await;
    assert_eq!(app.jf_json(&format!("/Users/{mine}/GroupingOptions"), admin).await, options);

    // Libraries' folders on disk are for administrators, which no one is here.
    let folders = app.jf(Method::GET, "/Library/VirtualFolders", admin, None).await;
    assert_eq!(folders.status, StatusCode::FORBIDDEN);
    assert_eq!(folders.text(), "");
    let unsigned = app.jf(Method::GET, "/Library/VirtualFolders", "", None).await;
    assert_eq!(unsigned.status, StatusCode::UNAUTHORIZED);
    // Its library mode looks for its own plugin, installed or to install.
    for path in ["/Plugins", "/Packages"] {
        assert_eq!(app.jf_json(path, admin).await, json!([]), "{path}");
    }
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

/// A request to this API with a raw body, as apps upload pictures.
async fn jf_upload(app: &TestApp, uri: &str, token: &str, content_type: &str, body: Vec<u8>) -> StatusCode {
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("x-emby-token", token)
        .header("content-type", content_type)
        .body(axum::body::Body::from(body))
        .unwrap();
    tower::ServiceExt::oneshot(app.jellyfin.clone(), req).await.unwrap().status()
}

/// The fields Pley's own API says are locked.
async fn locked(app: &TestApp, admin: &str, id: i64) -> Vec<String> {
    let item = app.get(&format!("/api/items/{id}"), admin).await.body;
    item["locked_fields"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn metadata_is_edited_with_a_token_that_may_manage_libraries() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let film = app.jf_item(w.film);
    let manager = app.compatibility_token(admin, "manage").await;
    let as_manager = |method: Method, uri: String, body: Option<Value>| {
        let token = manager.clone();
        async move { app.jf_raw(method, &uri, &[("x-emby-token", &token)], body).await }
    };

    // An app sends back the item it was given, with its changes.
    let mut item = as_manager(Method::GET, format!("/Items/{film}"), None).await.body;
    assert_eq!((&item["LockedFields"], &item["LockData"], &item["Tags"]), (&json!([]), &json!(false), &json!([])));
    item["Name"] = json!("Alpha Redux");
    item["Overview"] = json!("A new summary.");
    item["Genres"] = json!(["Drama", "Mystery"]);
    item["Tags"] = json!(["Favourites"]);
    item["OfficialRating"] = json!("PG-13");
    item["ProductionYear"] = json!(1980);
    let player = app.jf(Method::POST, &format!("/Items/{film}"), admin, Some(item.clone())).await;
    assert_eq!(player.status, StatusCode::FORBIDDEN, "a player's token edits nothing");
    let edited = as_manager(Method::POST, format!("/Items/{film}"), Some(item)).await;
    assert_eq!(edited.status, StatusCode::NO_CONTENT, "{}", edited.text());
    let item = as_manager(Method::GET, format!("/Items/{film}"), None).await.body;
    assert_eq!((&item["Name"], &item["Overview"]), (&json!("Alpha Redux"), &json!("A new summary.")));
    assert_eq!((&item["Genres"], &item["Tags"]), (&json!(["Drama", "Mystery"]), &json!(["Favourites"])));
    assert_eq!((&item["OfficialRating"], &item["ProductionYear"]), (&json!("PG-13"), &json!(1980)));
    // Edits are locked, as Pley's editor locks them.
    assert_eq!(item["LockedFields"], json!(["Name", "Overview", "Genres", "OfficialRating"]));
    assert_eq!(locked(app, admin, w.film).await, ["content_rating", "genres", "summary", "title", "year"]);

    // Left out of LockedFields, a field goes back to the metadata source;
    // LockData locks every one.
    let mut unlock = item.clone();
    unlock["LockedFields"] = json!(["Name", "Genres"]);
    assert_eq!(as_manager(Method::POST, format!("/Items/{film}"), Some(unlock)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(locked(app, admin, w.film).await, ["genres", "title", "year"]);
    let mut all = as_manager(Method::GET, format!("/Items/{film}"), None).await.body;
    all["LockData"] = json!(true);
    assert_eq!(as_manager(Method::POST, format!("/Items/{film}"), Some(all)).await.status, StatusCode::NO_CONTENT);
    let item = as_manager(Method::GET, format!("/Items/{film}"), None).await.body;
    assert_eq!(item["LockData"], json!(true));

    // What Pley doesn't keep is refused, and nothing of that request applied.
    let mut cast = item.clone();
    cast["Name"] = json!("Not this");
    cast["People"] = json!([{"Name": "Someone", "Type": "Director"}]);
    let refused = as_manager(Method::POST, format!("/Items/{film}"), Some(cast)).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.text().contains("People"), "{}", refused.text());
    let mut lock = item.clone();
    lock["LockedFields"] = json!(["Cast"]);
    assert_eq!(as_manager(Method::POST, format!("/Items/{film}"), Some(lock)).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(as_manager(Method::GET, format!("/Items/{film}"), None).await.body["Name"], json!("Alpha Redux"));

    // Collections: their name and overview.
    let made = app
        .post(
            &format!("/api/libraries/{}/collections", w.movies),
            admin,
            json!({"title": "Picks", "item_ids": [w.film]}),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    let collection = app.jf_ids().of(pley::jellyfin::ids::Kind::Collection, made.body["id"].as_i64().unwrap());
    let renamed =
        as_manager(Method::POST, format!("/Items/{collection}"), Some(json!({"Name": "Best", "Overview": "Ours."})))
            .await;
    assert_eq!(renamed.status, StatusCode::NO_CONTENT, "{}", renamed.text());
    let shown = as_manager(Method::GET, format!("/Items/{collection}"), None).await.body;
    assert_eq!((&shown["Name"], &shown["Overview"]), (&json!("Best"), &json!("Ours.")));

    let editor = as_manager(Method::GET, format!("/Items/{film}/MetadataEditor"), None).await;
    assert_eq!(editor.status, StatusCode::OK);
    for field in ["ParentalRatingOptions", "Countries", "Cultures", "ExternalIdInfos", "ContentTypeOptions"] {
        assert_eq!(editor.body[field], json!([]), "{field}");
    }
    let refresh = as_manager(Method::POST, format!("/Items/{}/Refresh", app.jf_library(w.movies)), None).await;
    assert_eq!(refresh.status, StatusCode::NO_CONTENT, "{}", refresh.text());
}

#[tokio::test]
async fn pictures_and_matches_through_the_editing_endpoints() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let film = app.jf_item(w.film);
    let manager = app.compatibility_token(admin, "manage").await;
    let as_manager = |method: Method, uri: String, body: Option<Value>| {
        let token = manager.clone();
        async move { app.jf_raw(method, &uri, &[("x-emby-token", &token)], body).await }
    };

    // A picture uploaded as Jellyfin takes it, in base64, then put back.
    let mut png = Vec::new();
    image::RgbImage::from_pixel(4, 6, image::Rgb([200, 30, 30]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png);
    let uri = format!("/Items/{film}/Images/Primary");
    assert_eq!(jf_upload(app, &uri, &manager, "image/png", encoded.into_bytes()).await, StatusCode::NO_CONTENT);
    assert!(locked(app, admin, w.film).await.contains(&"poster".to_owned()));
    let player = app.jellyfin_token(admin).await;
    assert_eq!(jf_upload(app, &uri, &player, "image/png", png.clone()).await, StatusCode::FORBIDDEN);
    assert_eq!(jf_upload(app, &uri, &manager, "image/png", b"not a picture".to_vec()).await, StatusCode::BAD_REQUEST);
    assert_eq!(as_manager(Method::DELETE, uri.clone(), None).await.status, StatusCode::NO_CONTENT);
    assert!(!locked(app, admin, w.film).await.contains(&"poster".to_owned()));
    // A real-sized picture, past the API's usual body limit, by index as
    // Jellyfin's Kotlin SDK sends it.
    let mut seed = 1u32;
    let noise = image::RgbImage::from_fn(320, 320, |_, _| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let [r, g, b, _] = seed.to_le_bytes();
        image::Rgb([r, g, b])
    });
    let mut big = Vec::new();
    noise.write_to(&mut std::io::Cursor::new(&mut big), image::ImageFormat::Png).unwrap();
    let big = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &big);
    assert!(big.len() > 256 * 1024);
    let indexed = format!("{uri}/0");
    assert_eq!(jf_upload(app, &indexed, &manager, "image/png", big.into_bytes()).await, StatusCode::NO_CONTENT);
    assert_eq!(as_manager(Method::DELETE, indexed, None).await.status, StatusCode::NO_CONTENT);

    // Without a TMDB key: no pictures to choose from, nor matches.
    let remote = as_manager(Method::GET, format!("/Items/{film}/RemoteImages?type=Primary"), None).await;
    assert_eq!(remote.body, json!({"Images": [], "TotalRecordCount": 0, "Providers": []}));
    assert_eq!(as_manager(Method::GET, format!("/Items/{film}/RemoteImages/Providers"), None).await.body, json!([]));
    let search = json!({"SearchInfo": {"Name": "Alpha", "Year": 1979}, "ItemId": &film});
    let searched = as_manager(Method::POST, "/Items/RemoteSearch/Movie".into(), Some(search)).await;
    assert_eq!(searched.status, StatusCode::BAD_REQUEST);
    // A picture is taken only from the hosts Pley takes pictures from.
    let elsewhere = format!("/Items/{film}/RemoteImages/Download?type=Primary&imageUrl=http://192.168.1.1/a.jpg");
    assert_eq!(as_manager(Method::POST, elsewhere, None).await.status, StatusCode::BAD_REQUEST);
    let no_id =
        as_manager(Method::POST, format!("/Items/RemoteSearch/Apply/{film}"), Some(json!({"Name": "Alpha"}))).await;
    assert_eq!(no_id.status, StatusCode::BAD_REQUEST, "Pley matches by TMDB id");
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

/// Fake seek previews of `item`'s first file (tests have no transcoder):
/// one sheet of four 8x4 tiles. Its id.
async fn previews(app: &TestApp, item: i64) -> i64 {
    let (file, path): (i64, String) = sqlx::query_as("SELECT id, path FROM media_files WHERE item_id = ? ORDER BY id")
        .bind(item)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let fingerprint = app.state.store.fingerprint_of(file, &path, None).await.unwrap();
    let dir = app.state.store.media_dir(&fingerprint).unwrap().join("trickplay");
    std::fs::create_dir_all(&dir).unwrap();
    image::RgbImage::new(16, 8).save(dir.join("0.jpg")).unwrap();
    let info = pley_transcoder_api::TrickplayInfo {
        id: "t".into(),
        interval_ms: 10_000,
        width: 8,
        height: 4,
        columns: 2,
        rows: 2,
        count: 4,
        sheets: 1,
        version: 1,
    };
    std::fs::write(dir.join("index.json"), serde_json::to_vec(&info).unwrap()).unwrap();
    file
}

#[tokio::test]
async fn seek_previews_are_pleys_own_and_open_as_the_stream_does() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let film = app.jf_item(w.film);
    let sheet = format!("/Videos/{film}/Trickplay/8/0.jpg");

    // None made yet: none listed, none served.
    assert_eq!(app.jf_json(&format!("/Items/{film}"), admin).await["Trickplay"], json!({}));
    assert_eq!(app.jf(Method::GET, &sheet, admin, None).await.status, StatusCode::NOT_FOUND);

    // Keyed by the file's media source, then by width, as players look them up.
    let file = previews(app, w.film).await;
    let source = app.jf_source(file, admin).await;
    let item = app.jf_json(&format!("/Items/{film}"), admin).await;
    let info = &item["Trickplay"][&source]["8"];
    assert_eq!(
        (&info["Width"], &info["Height"], &info["TileWidth"], &info["TileHeight"]),
        (&json!(8), &json!(4), &json!(2), &json!(2)),
        "{item}"
    );
    assert_eq!((&info["ThumbnailCount"], &info["Interval"]), (&json!(4), &json!(10_000)));
    assert!(info["Bandwidth"].as_u64().unwrap() > 0, "{info}");
    assert_eq!(item["MediaSources"][0]["Id"], json!(source));
    // Lists have them when they ask.
    let listed = app.jf_json(&format!("/Items?Ids={film}&Fields=Trickplay"), admin).await;
    assert_eq!(listed["Items"][0]["Trickplay"], item["Trickplay"]);
    assert!(app.jf_json(&format!("/Items?Ids={film}"), admin).await["Items"][0].get("Trickplay").is_none());

    // A sheet opens with a token, or with the media source's id alone.
    let with_token = app.jf(Method::GET, &sheet, admin, None).await;
    assert_eq!(with_token.status, StatusCode::OK);
    assert_eq!(with_token.headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(&with_token.bytes[..2], &[0xff, 0xd8], "a JPEG");
    let by_source = format!("{sheet}?MediaSourceId={source}");
    assert_eq!(app.jf_raw(Method::GET, &by_source, &[], None).await.status, StatusCode::OK);
    assert_eq!(app.jf_raw(Method::GET, &sheet, &[], None).await.status, StatusCode::UNAUTHORIZED);
    for uri in [
        format!("/Videos/{film}/Trickplay/320/0.jpg"),
        format!("/Videos/{film}/Trickplay/8/1.jpg"),
        format!("/Videos/{film}/Trickplay/8/0.png"),
    ] {
        assert_eq!(app.jf(Method::GET, &uri, admin, None).await.status, StatusCode::NOT_FOUND, "{uri}");
    }

    // The playlist of tiles names the sheets by the source, not the token.
    let playlist = app.jf(Method::GET, &format!("/Videos/{film}/Trickplay/8/tiles.m3u8"), admin, None).await;
    assert_eq!(playlist.status, StatusCode::OK);
    assert_eq!(playlist.headers[header::CONTENT_TYPE], "application/vnd.apple.mpegurl");
    let text = playlist.text();
    assert!(text.contains("#EXTINF:40,\n#EXT-X-TILES:RESOLUTION=8x4,LAYOUT=2x2,DURATION=10\n"), "{text}");
    assert!(text.contains(&format!("\n0.jpg?MediaSourceId={source}\n")), "{text}");
    assert!(!text.contains(&app.jellyfin_token(admin).await));

    // Someone who may not open the library, and a sign-in that ended: no sheet.
    let (bob, _) = app.user(admin, "bob", json!({"all_libraries": false, "library_ids": [w.tv]})).await;
    assert_eq!(app.jf(Method::GET, &sheet, &bob, None).await.status, StatusCode::NOT_FOUND);
    let bobs = format!("{sheet}?MediaSourceId={}", app.jf_source(file, &bob).await);
    assert_eq!(app.jf_raw(Method::GET, &bobs, &[], None).await.status, StatusCode::NOT_FOUND);
    let signed_in = sign_in(app, json!({"Username": "admin", "Pw": "secret-one"})).await;
    let token = signed_in.body["AccessToken"].as_str().unwrap().to_owned();
    let auth = [("x-emby-token", token.as_str())];
    let item = app.jf_raw(Method::GET, &format!("/Items/{film}"), &auth, None).await;
    let theirs = item.body["Trickplay"].as_object().unwrap().keys().next().unwrap().clone();
    let by_theirs = format!("{sheet}?MediaSourceId={theirs}");
    assert_eq!(app.jf_raw(Method::GET, &by_theirs, &[], None).await.status, StatusCode::OK);
    app.jf_raw(Method::POST, "/Sessions/Logout", &auth, None).await;
    assert_eq!(app.jf_raw(Method::GET, &by_theirs, &[], None).await.status, StatusCode::UNAUTHORIZED);
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
async fn sync_tools_copy_a_place_in_a_video() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let (film, me) = (app.jf_item(w.film), app.jf_user(admin).await);
    let user_data = format!("/Users/{me}/Items/{film}/UserData");
    let pley_state = || async { app.get(&format!("/api/items/{}", w.film), admin).await.body["user_state"].clone() };

    // As WatchState sends it: the ticks as text, the time with its zone.
    let body = json!({"PlaybackPositionTicks": "450000000", "LastPlayedDate": "2024-05-01T14:00:00+02:00"});
    let set = app.jf(Method::POST, &user_data, admin, Some(body)).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!(set.body["PlaybackPositionTicks"], json!(450_000_000_i64));
    assert_eq!(set.body["LastPlayedDate"], json!("2024-05-01T12:00:00.0000000Z"));
    let state = pley_state().await;
    assert_eq!((&state["position_ms"], &state["watched"]), (&json!(45_000), &json!(false)));
    let resume = app.jf_json("/UserItems/Resume", admin).await;
    assert_eq!(jf_names(&resume), ["Alpha"]);
    let item = app.jf_json(&format!("/Users/{me}/Items/{film}"), admin).await;
    assert_eq!(item["UserData"]["PlaybackPositionTicks"], json!(450_000_000_i64));

    // An app posting back what it was given, to change something else,
    // leaves the place and its time alone.
    let mut echoed = item["UserData"].clone();
    echoed["IsFavorite"] = json!(true);
    app.jf(Method::POST, &user_data, admin, Some(echoed)).await;
    let again = app.jf_json(&format!("/Users/{me}/Items/{film}"), admin).await;
    assert_eq!(again["UserData"]["LastPlayedDate"], json!("2024-05-01T12:00:00.0000000Z"));
    assert_eq!(again["UserData"]["PlaybackPositionTicks"], json!(450_000_000_i64));
    assert_eq!(again["UserData"]["IsFavorite"], json!(true));

    // Watched elsewhere and started over: unplayed, with the new place.
    app.jf(Method::POST, &format!("/UserPlayedItems/{film}"), admin, None).await;
    let body = json!({"PlaybackPositionTicks": 300_000_000_i64, "Played": false});
    let restarted = app.jf(Method::POST, &user_data, admin, Some(body)).await;
    assert_eq!(
        (&restarted.body["Played"], &restarted.body["PlaybackPositionTicks"]),
        (&json!(false), &json!(300_000_000_i64))
    );

    // A time to come is today at the latest, and keys go in any case, as
    // Jellyfin reads them.
    let body = json!({"playbackPositionTicks": 600_000_000_i64, "lastPlayedDate": "2999-01-01T00:00:00Z"});
    let later = app.jf(Method::POST, &user_data, admin, Some(body)).await;
    assert!(later.body["LastPlayedDate"].as_str().unwrap() < "2999", "{}", later.text());
    assert_eq!(later.body["PlaybackPositionTicks"], json!(600_000_000_i64));
    // What can't be read is refused, as Jellyfin does, and changes nothing.
    for wrong in [
        json!({"PlaybackPositionTicks": "soon"}),
        json!({"PlaybackPositionTicks": 6.5e8}),
        json!({"PlaybackPositionTicks": 650_000_000_i64, "LastPlayedDate": "yesterday"}),
    ] {
        let refused = app.jf(Method::POST, &user_data, admin, Some(wrong.clone())).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{wrong}");
    }
    let nulls = json!({"PlaybackPositionTicks": null, "LastPlayedDate": null, "IsFavorite": true});
    assert_eq!(app.jf(Method::POST, &user_data, admin, Some(nulls)).await.status, StatusCode::OK);
    assert_eq!(pley_state().await["position_ms"], json!(60_000));

    // Only what plays has a place.
    let show_data = format!("/Users/{me}/Items/{}/UserData", app.jf_item(w.show));
    let refused = app.jf(Method::POST, &show_data, admin, Some(json!({"PlaybackPositionTicks": "450000000"}))).await;
    assert_eq!(refused.body["PlaybackPositionTicks"], json!(0));

    // Past 90 % it is watched, and leaves My List, which devices hear of;
    // the play counts at the time given.
    let mut events = app.state.events.subscribe();
    let body = json!({"PlaybackPositionTicks": "950000000", "LastPlayedDate": "2024-05-02T08:00:00Z"});
    let finished = app.jf(Method::POST, &user_data, admin, Some(body)).await;
    assert_eq!((&finished.body["Played"], &finished.body["PlaybackPositionTicks"]), (&json!(true), &json!(0)));
    assert!(common::titles(&app.get("/api/watchlist", admin).await.body["items"]).is_empty());
    let heard: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(heard.iter().any(|e| matches!(e, pley::events::ServerEvent::WatchlistChanged { .. })), "{heard:?}");
    let viewed: i64 = sqlx::query_scalar("SELECT viewed_at FROM play_history WHERE item_id = ? ORDER BY id DESC")
        .bind(w.film)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(viewed, pley::util::iso_secs("2024-05-02T08:00:00Z").unwrap());

    // A place in a watched item is kept for a rewatch, which nothing shows:
    // sent again, as sync tools do each time, it changes nothing more.
    let again = json!({"PlaybackPositionTicks": "450000000", "LastPlayedDate": "2024-05-02T09:00:00Z"});
    app.jf(Method::POST, &user_data, admin, Some(again.clone())).await;
    let mut events = app.state.events.subscribe();
    app.jf(Method::POST, &user_data, admin, Some(again)).await;
    assert!(events.try_recv().is_err(), "nothing changed");

    // Marked watched as an app posts back what it was given: what it held
    // (in My List) doesn't put the movie back there.
    app.jf(Method::DELETE, &format!("/UserPlayedItems/{film}"), admin, None).await;
    app.jf(Method::POST, &format!("/UserFavoriteItems/{film}"), admin, None).await;
    let body = json!({"Played": true, "IsFavorite": true, "PlaybackPositionTicks": 0});
    let marked = app.jf(Method::POST, &user_data, admin, Some(body)).await;
    assert_eq!((&marked.body["Played"], &marked.body["IsFavorite"]), (&json!(true), &json!(false)));

    // A show marked with a time: its episodes were played then.
    let body = json!({"Played": true, "LastPlayedDate": "2024-05-03T08:00:00Z"});
    app.jf(Method::POST, &show_data, admin, Some(body)).await;
    let pilot = app.jf_json(&format!("/Users/{me}/Items/{}", app.jf_item(w.pilot)), admin).await;
    assert_eq!(pilot["UserData"]["LastPlayedDate"], json!("2024-05-03T08:00:00.0000000Z"));

    // Only the user's own, of what they may see.
    let (bob, _) = app.user(admin, "bob", json!({"all_libraries": false, "library_ids": [w.tv]})).await;
    let bobs = app.jf_user(&bob).await;
    let body = json!({"PlaybackPositionTicks": "450000000"});
    let theirs = app.jf(Method::POST, &format!("/Users/{bobs}/Items/{film}/UserData"), admin, Some(body.clone())).await;
    assert_eq!(theirs.status, StatusCode::NOT_FOUND);
    let unseen = app.jf(Method::POST, &format!("/Users/{bobs}/Items/{film}/UserData"), &bob, Some(body)).await;
    assert_eq!(unseen.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn jellyfin_apps_make_and_change_their_own_playlists() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (alpha, bravo, charlie) =
        (app.jf_item(movies["Alpha"]), app.jf_item(movies["Bravo"]), app.jf_item(movies["Charlie"]));

    // As the Kotlin SDK sends it: a body, every field there.
    let me = app.jf_user(admin).await;
    let made = app
        .jf(
            Method::POST,
            "/Playlists",
            admin,
            Some(json!({"Name": "Evening", "Ids": [&alpha, &bravo], "UserId": me, "MediaType": "Video", "Users": [], "IsPublic": false})),
        )
        .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let playlist = made.body["Id"].as_str().unwrap().to_owned();
    let items = app.jf_json(&format!("/Playlists/{playlist}/Items"), admin).await;
    assert_eq!(jf_names(&items), ["Alpha", "Bravo"]);
    let entries: Vec<String> =
        items["Items"].as_array().unwrap().iter().map(|i| i["PlaylistItemId"].as_str().unwrap().to_owned()).collect();
    assert!(entries.iter().all(|e| is_hex(e, 32)) && entries[0] != entries[1], "{entries:?}");
    // Apps that list a playlist's titles as its children get the entries too.
    let children = app.jf_json(&format!("/Items?ParentId={playlist}"), admin).await;
    assert_eq!(children["Items"][1]["PlaylistItemId"], json!(entries[1]));
    let dto = app.jf_json(&format!("/Playlists/{playlist}"), admin).await;
    assert_eq!(dto, json!({"OpenAccess": false, "Shares": [], "ItemIds": [&alpha, &bravo]}));

    // Added at the end, or where asked; moved; removed by entry.
    let added =
        app.jf(Method::POST, &format!("/Playlists/{playlist}/Items?ids={charlie}&userId={me}"), admin, None).await;
    assert_eq!(added.status, StatusCode::NO_CONTENT, "{}", added.text());
    let again = app.jf(Method::POST, &format!("/Playlists/{playlist}/Items?ids={alpha}&position=0"), admin, None).await;
    assert_eq!(again.status, StatusCode::NO_CONTENT);
    let items = app.jf_json(&format!("/Playlists/{playlist}/Items"), admin).await;
    assert_eq!(jf_names(&items), ["Alpha", "Alpha", "Bravo", "Charlie"], "an item may be there twice");
    let charlie_entry = items["Items"][3]["PlaylistItemId"].as_str().unwrap().to_owned();
    let moved = app.jf(Method::POST, &format!("/Playlists/{playlist}/Items/{charlie_entry}/Move/1"), admin, None).await;
    assert_eq!(moved.status, StatusCode::NO_CONTENT, "{}", moved.text());
    let items = app.jf_json(&format!("/Playlists/{playlist}/Items"), admin).await;
    assert_eq!(jf_names(&items), ["Alpha", "Charlie", "Alpha", "Bravo"]);
    let first = items["Items"][0]["PlaylistItemId"].as_str().unwrap();
    let removed = app.jf(Method::DELETE, &format!("/Playlists/{playlist}/Items?entryIds={first}"), admin, None).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        jf_names(&app.jf_json(&format!("/Playlists/{playlist}/Items"), admin).await),
        ["Charlie", "Alpha", "Bravo"]
    );
    // An app without the entry's id sends the item's: its first entry.
    let by_item = app.jf(Method::POST, &format!("/Playlists/{playlist}/Items/{charlie}/Move/2"), admin, None).await;
    assert_eq!(by_item.status, StatusCode::NO_CONTENT);
    let items = app.jf_json(&format!("/Playlists/{playlist}/Items"), admin).await;
    assert_eq!(jf_names(&items), ["Alpha", "Bravo", "Charlie"]);

    // Renamed, and filled anew as a whole, as Feishin saves an edited one.
    let update = json!({"Name": "Late", "Ids": [&bravo, &alpha]});
    let updated = app.jf(Method::POST, &format!("/Playlists/{playlist}"), admin, Some(update)).await;
    assert_eq!(updated.status, StatusCode::NO_CONTENT, "{}", updated.text());
    assert_eq!(app.jf_json(&format!("/Items/{playlist}"), admin).await["Name"], json!("Late"));
    assert_eq!(jf_names(&app.jf_json(&format!("/Playlists/{playlist}/Items"), admin).await), ["Bravo", "Alpha"]);

    // The owner's alone: not shared, nor anyone else's to see or change.
    let shared = app.jf(Method::POST, &format!("/Playlists/{playlist}"), admin, Some(json!({"IsPublic": true}))).await;
    assert_eq!(shared.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.jf_json(&format!("/Playlists/{playlist}/Users"), admin).await, json!([]));
    let mine = app.jf_json(&format!("/Playlists/{playlist}/Users/{me}"), admin).await;
    assert_eq!(mine, json!({"UserId": me, "CanEdit": true}));
    let (ann, _) = app.user(admin, "ann", json!({})).await;
    for (method, uri) in [
        (Method::GET, format!("/Playlists/{playlist}")),
        (Method::POST, format!("/Playlists/{playlist}/Items?ids={charlie}")),
        (Method::DELETE, format!("/Playlists/{playlist}/Items?entryIds={first}")),
        (Method::DELETE, format!("/Items/{playlist}")),
    ] {
        let theirs = app.jf(method.clone(), &uri, &ann, None).await;
        assert_eq!(theirs.status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
    let for_another = app.jf(Method::POST, &format!("/Playlists?Name=x&UserId={me}"), &ann, None).await;
    assert_eq!(for_another.status, StatusCode::FORBIDDEN, "for its own account only");

    // Older apps send the fields in the query. A playlist's kind holds.
    let query = app.jf(Method::POST, &format!("/Playlists?Name=Songs&Ids={alpha}&MediaType=Audio"), &ann, None).await;
    assert_eq!(query.status, StatusCode::BAD_REQUEST, "a film in a music playlist");
    let query = app.jf(Method::POST, &format!("/Playlists?Name=Films&Ids={alpha}"), &ann, None).await;
    assert_eq!(query.status, StatusCode::OK, "{}", query.text());
    let films = query.body["Id"].as_str().unwrap();
    assert_eq!(jf_names(&app.jf_json(&format!("/Playlists/{films}/Items"), &ann).await), ["Alpha"]);

    // Deleted by its owner; a title never.
    let listed = app.jf_json("/Items?IncludeItemTypes=Playlist&Recursive=true", admin).await;
    assert_eq!(listed["Items"][0]["CanDelete"], json!(true));
    assert_eq!(app.jf(Method::DELETE, &format!("/Items/{alpha}"), admin, None).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.jf(Method::DELETE, &format!("/Items/{playlist}"), admin, None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.jf(Method::GET, &format!("/Playlists/{playlist}"), admin, None).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn collections_change_with_a_token_that_may_manage_libraries() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (alpha, bravo) = (app.jf_item(movies["Alpha"]), app.jf_item(movies["Bravo"]));
    let library = app.jf_library(w.movies);
    let manager = app.compatibility_token(admin, "manage").await;
    let as_manager = |method: Method, uri: String| {
        let token = manager.clone();
        async move { app.jf_raw(method, &uri, &[("x-emby-token", &token)], None).await }
    };

    // A player's token: no collection management, and clients are told so.
    let me = app.jf_json("/Users/Me", admin).await;
    assert_eq!(me["Policy"]["EnableCollectionManagement"], json!(false));
    let refused = app.jf(Method::POST, &format!("/Collections?name=Picks&ids={alpha}"), admin, None).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let me = as_manager(Method::GET, "/Users/Me".into()).await;
    assert_eq!(me.body["Policy"]["EnableCollectionManagement"], json!(true));
    assert_eq!(me.body["Policy"]["IsAdministrator"], json!(false), "still no administrator");

    let made = as_manager(Method::POST, format!("/Collections?name=Picks&ids={alpha}&parentId={library}")).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let collection = made.body["Id"].as_str().unwrap().to_owned();
    let added = as_manager(Method::POST, format!("/Collections/{collection}/Items?ids={bravo}")).await;
    assert_eq!(added.status, StatusCode::NO_CONTENT, "{}", added.text());
    let again = as_manager(Method::POST, format!("/Collections/{collection}/Items?ids={bravo}")).await;
    assert_eq!(again.status, StatusCode::NO_CONTENT, "a title in it already: {}", again.text());
    let items = app.jf_json(&format!("/Items?ParentId={collection}"), admin).await;
    assert_eq!(jf_names(&items), ["Alpha", "Bravo"]);
    let pilot = app.jf_item(w.pilot);
    let episode = as_manager(Method::POST, format!("/Collections/{collection}/Items?ids={pilot}")).await;
    assert_eq!(episode.status, StatusCode::BAD_REQUEST, "movies and shows of its library only");
    let removed = as_manager(Method::DELETE, format!("/Collections/{collection}/Items?ids={alpha}")).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(jf_names(&app.jf_json(&format!("/Items?ParentId={collection}"), admin).await), ["Bravo"]);
    // Without a library, the first title's.
    let first = as_manager(Method::POST, format!("/Collections?name=More&ids={bravo}")).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());

    // Changed by no one else; deleted with the right token only.
    let (ann, _) = app.user(admin, "ann", json!({})).await;
    let theirs = app.jf(Method::POST, &format!("/Collections/{collection}/Items?ids={alpha}"), &ann, None).await;
    assert_eq!(theirs.status, StatusCode::FORBIDDEN);
    assert_eq!(
        app.jf(Method::DELETE, &format!("/Items/{collection}"), admin, None).await.status,
        StatusCode::FORBIDDEN
    );
    let listed = as_manager(Method::GET, "/Items?IncludeItemTypes=BoxSet&Recursive=true".into()).await;
    assert!(
        listed.body["Items"].as_array().unwrap().iter().all(|c| c["CanDelete"] == json!(true)),
        "{}",
        listed.text()
    );
    assert_eq!(as_manager(Method::DELETE, format!("/Items/{collection}")).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.jf(Method::GET, &format!("/Items/{collection}"), admin, None).await.status, StatusCode::NOT_FOUND);
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
        (format!("/Users/{mine}/GroupingOptions"), true),
        (format!("/Users/{bobs}/GroupingOptions"), false),
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
        "/UserViews/GroupingOptions".to_owned(),
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
        "/Packages".to_owned(),
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
    for path in
        ["/Search/Hints?SearchTerm=x".to_owned(), format!("/Playlists/{x}/Items"), "/Library/VirtualFolders".to_owned()]
    {
        let page = app.call(Method::GET, &path, None, None, &[]).await;
        assert!(!theirs(&page), "{path} without a token is the web app's");
        let reply = app.call(Method::GET, &path, None, None, &[("x-emby-token", token.as_str())]).await;
        assert!(theirs(&reply), "{path}: {} {}", reply.status, reply.text());
    }
}

#[tokio::test]
async fn activity_monitors_see_what_everyone_plays() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let (bob, _) = app.user(admin, "bob", json!({})).await;
    let pilot = app.jf_item(w.pilot);
    let report = json!({"ItemId": pilot, "PositionTicks": 600_000_000_i64, "IsPaused": true});
    let started = app.jf(Method::POST, "/Sessions/Playing", &bob, Some(report)).await;
    assert_eq!(started.status, StatusCode::NO_CONTENT, "{}", started.text());

    // An administrator's token with `activity`, as Tracearr is given.
    let monitor = app.compatibility_token(admin, "activity").await;
    let listed = app.jf_raw(Method::GET, "/Sessions", &[("x-emby-token", &monitor)], None).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let sessions = listed.body.as_array().unwrap();
    assert_eq!(sessions.len(), 1, "{}", listed.text());
    let session = &sessions[0];
    assert_eq!((&session["UserName"], &session["UserId"]), (&json!("bob"), &json!(app.jf_user(&bob).await)));
    assert_eq!(session["DeviceName"], json!("Bench"));
    let item = &session["NowPlayingItem"];
    assert_eq!((&item["Id"], &item["Name"], &item["Type"]), (&json!(pilot), &json!("Pilot"), &json!("Episode")));
    assert_eq!(
        (&item["SeriesName"], &item["ParentIndexNumber"], &item["IndexNumber"]),
        (&json!("Show"), &json!(1), &json!(1))
    );
    let play = &session["PlayState"];
    assert_eq!((&play["PositionTicks"], &play["IsPaused"]), (&json!(600_000_000_i64), &json!(true)));
    assert_eq!(play["PlayMethod"], json!("DirectPlay"));
    assert_eq!(play["MediaSourceId"], item["MediaSources"][0]["Id"]);

    // Pley's session id opens its stream: monitors are given a hash of it,
    // and of a device id made from it.
    let pleys = app.state.sessions.active()[0].clone();
    let (id, device) = (session["Id"].as_str().unwrap(), session["DeviceId"].as_str().unwrap());
    assert!(is_hex(id, 32) && is_hex(device, 32), "{session}");
    assert!(!pleys.id.contains(id) && !id.contains(&pleys.id) && !device.contains(&pleys.id));

    // Other tokens, the player's own included, see none.
    for token in [app.compatibility_token(admin, "play").await, app.compatibility_token(admin, "manage").await] {
        let none = app.jf_raw(Method::GET, "/Sessions", &[("x-emby-token", &token)], None).await;
        assert_eq!((none.status, none.body), (StatusCode::OK, json!([])));
    }
    assert_eq!(app.jf(Method::GET, "/Sessions", &bob, None).await.body, json!([]));
}

#[tokio::test]
async fn activity_tokens_say_they_administer_and_change_nothing() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let administers = async |scope: &str| {
        let token = app.compatibility_token(admin, scope).await;
        let me = app.jf_raw(Method::GET, "/Users/Me", &[("x-emby-token", &token)], None).await;
        assert_eq!(me.status, StatusCode::OK, "{}", me.text());
        me.body["Policy"]["IsAdministrator"].clone()
    };
    // Monitors such as Tracearr refuse a key that isn't an administrator's.
    assert_eq!(administers("activity").await, json!(true));
    assert_eq!(administers("play").await, json!(false));
    assert_eq!(administers("manage").await, json!(false));

    // It sees; it changes nothing.
    let monitor = app.compatibility_token(admin, "activity").await;
    let headers = [("x-emby-token", monitor.as_str())];
    let collection = app.jf_raw(Method::POST, "/Collections?name=Picks", &headers, None).await;
    assert_eq!(collection.status, StatusCode::FORBIDDEN);
    let film = format!("/Items/{}", app.jf_item(w.film));
    let edited = app.jf_raw(Method::POST, &film, &headers, Some(json!({"Name": "Renamed"}))).await;
    assert_eq!(edited.status, StatusCode::FORBIDDEN);
    let refreshed = app.jf_raw(Method::POST, &format!("{film}/Refresh"), &headers, None).await;
    assert_eq!(refreshed.status, StatusCode::FORBIDDEN);
}
