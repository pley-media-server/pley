//! Remote control of a user's own players (docs/API.md, "Remote control"):
//! `GET /api/sessions` lists them, `POST /api/sessions/{id}/command` sends
//! them `player_command` on the user's event streams.

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use futures::StreamExt;
use serde_json::{Value, json};

use common::TestApp;

/// What a browser playing Matroska directly says it plays.
const CAPS: &str = r#"{"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}"#;
/// The film's length, in milliseconds.
const LENGTH_MS: i64 = 100_000;

struct World {
    app: TestApp,
    admin: String,
    viewer: String,
    viewer_id: i64,
    other: String,
    film: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001)/Film (2001).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    sqlx::query("UPDATE items SET duration_ms = ?").bind(LENGTH_MS).execute(&app.state.db.write).await.unwrap();
    let (viewer, viewer_id) = app.user(&admin, "viewer", json!({})).await;
    let (other, _) = app.user(&admin, "other", json!({})).await;
    World { app, admin, viewer, viewer_id, other, film }
}

/// A direct play of the film by `token`'s user, with `fields` added to the
/// play request and `headers` sent with it: its session id.
async fn play_with(w: &World, token: &str, fields: Value, headers: &[(&str, &str)]) -> String {
    let mut body = json!({"item_id": w.film, "caps": serde_json::from_str::<Value>(CAPS).unwrap()});
    body.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    let started = w.app.call(Method::POST, "/api/playback", Some(token), Some(body), headers).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    assert_eq!(started.body["method"], json!("direct"));
    started.body["session_id"].as_str().unwrap().to_owned()
}

/// A play by a player that obeys every command.
async fn play(w: &World, token: &str) -> String {
    let controls = json!({"controls": ["play", "pause", "seek", "skip", "next", "previous"]});
    play_with(w, token, controls, &[]).await
}

async fn command(w: &World, token: &str, session: &str, body: Value) -> common::Reply {
    w.app.post(&format!("/api/sessions/{session}/command"), token, body).await
}

/// The events stream of `token`'s user, read until `until` shows up (or
/// `wait` has passed).
async fn events_until(w: &World, token: &str, act: impl Future<Output = ()>, until: &str, wait: Duration) -> String {
    streams_until(w, &[token], act, until, wait).await.remove(0)
}

/// What the events stream of each of `tokens` sent while `act` ran, and
/// for `wait` after it (or until it sent `until`).
async fn streams_until(
    w: &World,
    tokens: &[&str],
    act: impl Future<Output = ()>,
    until: &str,
    wait: Duration,
) -> Vec<String> {
    let mut streams = Vec::new();
    for token in tokens {
        let request = axum::http::Request::builder()
            .uri("/api/events")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = tower::ServiceExt::oneshot(w.app.api.clone(), request).await.unwrap();
        streams.push(response.into_body().into_data_stream());
    }
    act.await;
    let mut out = Vec::new();
    for mut stream in streams {
        let mut received = String::new();
        let read = async {
            while let Some(chunk) = stream.next().await {
                received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
                if received.contains(until) {
                    return;
                }
            }
        };
        let _ = tokio::time::timeout(wait, read).await;
        out.push(received);
    }
    out
}

/// The events of `kind` among what an events stream sent.
fn events_of(received: &str, kind: &str) -> Vec<Value> {
    received
        .lines()
        .filter_map(|line| line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:")))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .filter(|event| event["type"] == kind)
        .collect()
}

/// The kinds of the events sent since `events` subscribed.
fn kinds(events: &mut tokio::sync::broadcast::Receiver<pley::events::ServerEvent>) -> Vec<String> {
    let mut kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        kinds.push(serde_json::to_value(&event).unwrap()["type"].as_str().unwrap().to_owned());
    }
    kinds
}

#[tokio::test]
async fn a_users_players_are_listed_to_them_alone() {
    let w = world().await;
    let app = &w.app;
    // A player that names its device, and lists a command this server
    // doesn't know and one twice.
    let fields = json!({
        "controls": ["pause", "rewind", 5, "play", "pause"],
        "device_name": "  Living\u{202E} room\tTV  ",
    });
    let named = play_with(&w, &w.viewer, fields, &[]).await;
    // One that doesn't: named from its User-Agent.
    let chrome =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
    let unnamed = play_with(&w, &w.viewer, json!({}), &[("user-agent", chrome)]).await;
    play(&w, &w.other).await;
    play(&w, &w.admin).await;

    let listed = app.get("/api/sessions", &w.viewer).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let sessions = listed.body.as_array().unwrap();
    assert_eq!(sessions.len(), 2, "only the viewer's own: {sessions:?}");
    // Sessions started in the same second come in any order.
    let of = |id: &str| sessions.iter().find(|s| s["session_id"] == json!(id)).unwrap();
    let first = of(&named);
    assert_eq!(first["kind"], json!("item"));
    assert_eq!(first["device"], json!("Living room TV"), "cleaned like an app's name");
    assert_eq!(first["controls"], json!(["play", "pause"]), "known ones, once each");
    assert_eq!(first["item"]["id"], json!(w.film));
    assert_eq!(first["channel"], Value::Null);
    assert_eq!(first["state"], json!("playing"));
    assert_eq!((first["position_ms"].clone(), first["duration_ms"].clone()), (json!(0), json!(LENGTH_MS)));
    assert!(first["updated_at"].as_i64().unwrap() > 0);
    assert_eq!(of(&unnamed)["device"], json!("Chrome on Windows"));
    assert_eq!(of(&unnamed)["controls"], json!([]), "a player that never said obeys none");

    // An administrator's remote lists only their own players too; the
    // dashboard shows everyone's, with the device.
    let mine = app.get("/api/sessions", &w.admin).await.body;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let activity = app.get("/api/activity", &w.admin).await.body;
    let dashboard = activity["sessions"].as_array().unwrap();
    assert_eq!(dashboard.len(), 4);
    let shown = dashboard.iter().find(|s| s["session_id"] == json!(named)).unwrap();
    assert_eq!((shown["kind"].clone(), shown["device"].clone()), (json!("item"), json!("Living room TV")));

    // A stopped session isn't listed.
    assert_eq!(app.delete(&format!("/api/sessions/{named}"), &w.viewer).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/sessions", &w.viewer).await.body.as_array().unwrap().len(), 1);
}

/// Tokens of pley's Roku app acting as `user_id`.
async fn roku_token(w: &World, user_id: i64) -> String {
    let issued = w
        .app
        .state
        .auth
        .issue(pley::auth::grants::NewGrant {
            user_id,
            approved_by: user_id,
            client_id: common::ROKU_APP.into(),
            client_name: Some("pley".into()),
            scopes: pley::auth::scopes::Scopes::parse("library play").unwrap(),
            refresh: true,
            first_party: true,
            session_hash: None,
            user_agent: Some("pleyRoku/0.1.0 (Roku OS 15.0; Roku Ultra)".into()),
            ip: Some("10.0.0.9".into()),
        })
        .await
        .unwrap();
    issued.access_token
}

#[tokio::test]
async fn sessions_say_which_device_plays_them_and_what_kind_it_is() {
    let w = world().await;
    let app = &w.app;
    // Two plays by one browser sign-in: one device, a browser, whatever
    // the player calls it; another sign-in is another device.
    let tab = play_with(&w, &w.viewer, json!({"client_id": "tab-1", "device_name": "Den"}), &[]).await;
    let again = play_with(&w, &w.viewer, json!({"client_id": "tab-2", "keep_previous": true}), &[]).await;
    let other = play_with(&w, &w.other, json!({}), &[]).await;
    // Pley's Android app, which doesn't say what it plays on, and once it
    // does; a Roku, told from the app alone; a kind the server doesn't know.
    let (android, _) = app.app_token(w.viewer_id, "library play").await;
    let phone = play_with(&w, &android, json!({"client_id": "android"}), &[]).await;
    let tv = json!({"client_id": "android", "device_kind": "tv", "keep_previous": true});
    let tv = play_with(&w, &android, tv, &[]).await;
    let roku = play_with(&w, &roku_token(&w, w.viewer_id).await, json!({}), &[]).await;
    let fridge = play_with(&w, &w.viewer, json!({"client_id": "tab-3", "device_kind": "fridge"}), &[]).await;
    // A Jellyfin app's reports: its sign-in's token is its device.
    let report = json!({"ItemId": app.jf_item(w.film), "PositionTicks": 10_000_000_i64, "IsPaused": false});
    assert_eq!(app.jf(Method::POST, "/Sessions/Playing", &w.viewer, Some(report)).await.status, StatusCode::NO_CONTENT);

    let sessions = app.get("/api/activity", &w.admin).await.body["sessions"].clone();
    let sessions = sessions.as_array().unwrap();
    let of = |id: &str| sessions.iter().find(|s| s["session_id"] == json!(id)).unwrap();
    let device = |id: &str| (of(id)["device_id"].as_str().unwrap().to_owned(), of(id)["device_kind"].clone());
    let (web, kind) = device(&tab);
    assert!(web.starts_with("web-") && web.len() == "web-".len() + 16, "{web}");
    assert_eq!(kind, json!("browser"));
    assert_eq!(device(&again), (web.clone(), json!("browser")), "the same sign-in, another tab");
    assert_eq!(device(&fridge), (web.clone(), json!("browser")), "a kind the server doesn't know");
    assert_eq!(of(&tab)["device"], json!("Den"), "what the player calls it is beside it");
    let (theirs, kind) = device(&other);
    assert!(theirs.starts_with("web-") && theirs != web, "{theirs}");
    assert_eq!(kind, json!("browser"));
    let (grant, kind) = device(&phone);
    assert!(grant.starts_with("app-"), "{grant}");
    assert_eq!(kind, json!("other"), "the Android app hasn't said");
    assert_eq!(device(&tv), (grant, json!("tv")), "the same app's sign-in, once it says");
    let (roku_device, kind) = device(&roku);
    assert!(roku_device.starts_with("app-") && roku_device != device(&phone).0, "{roku_device}");
    assert_eq!(kind, json!("tv"));
    let jellyfin = sessions.iter().find(|s| s["device"] == json!("Bench")).expect("the Jellyfin app's session");
    assert!(jellyfin["device_id"].as_str().unwrap().starts_with("token-"), "{jellyfin}");
    assert_eq!(jellyfin["device_kind"], json!("other"));

    // The user's own list says the same.
    let mine = app.get("/api/sessions", &w.viewer).await.body;
    for session in mine.as_array().unwrap() {
        let id = session["session_id"].as_str().unwrap();
        let listed = of(id);
        assert_eq!((&session["device_id"], &session["device_kind"]), (&listed["device_id"], &listed["device_kind"]));
    }
}

#[tokio::test]
async fn sessions_changed_goes_to_the_user_and_administrators() {
    let w = world().await;
    let start = async {
        play(&w, &w.viewer).await;
    };
    let heard =
        streams_until(&w, &[&w.viewer, &w.admin, &w.other], start, "sessions_changed", Duration::from_secs(1)).await;
    let expected = vec![json!({"type": "sessions_changed"})];
    assert_eq!(events_of(&heard[0], "sessions_changed"), expected, "the user's own: {}", heard[0]);
    assert_eq!(events_of(&heard[1], "sessions_changed"), expected, "an administrator's: {}", heard[1]);
    assert!(events_of(&heard[2], "sessions_changed").is_empty(), "nobody else's: {}", heard[2]);
}

#[tokio::test]
async fn a_command_reaches_the_users_own_streams() {
    let w = world().await;
    let session = play(&w, &w.viewer).await;

    let pause = async {
        let sent = command(&w, &w.viewer, &session, json!({"command": "pause"})).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let heard = events_until(&w, &w.viewer, pause, "player_command", Duration::from_secs(10)).await;
    let expected = json!({
        "type": "player_command", "session_id": session, "command": "pause", "position_ms": null, "offset_ms": null,
        "stream_index": null,
    });
    assert_eq!(events_of(&heard, "player_command"), vec![expected]);

    // A seek and a skip carry where and how far; values the command doesn't
    // take are left out.
    let seek = async {
        let body = json!({"command": "seek", "position_ms": 42_000, "offset_ms": 5});
        assert_eq!(command(&w, &w.viewer, &session, body).await.status, StatusCode::ACCEPTED);
    };
    let heard = events_until(&w, &w.viewer, seek, "player_command", Duration::from_secs(10)).await;
    let event = &events_of(&heard, "player_command")[0];
    assert_eq!((event["command"].clone(), event["position_ms"].clone()), (json!("seek"), json!(42_000)));
    assert_eq!(event["offset_ms"], Value::Null);
    let skip = async {
        let body = json!({"command": "skip", "offset_ms": -30_000});
        assert_eq!(command(&w, &w.viewer, &session, body).await.status, StatusCode::ACCEPTED);
    };
    let heard = events_until(&w, &w.viewer, skip, "player_command", Duration::from_secs(10)).await;
    assert_eq!(events_of(&heard, "player_command")[0]["offset_ms"], json!(-30_000));

    // Nobody else's streams hear it, an administrator's neither.
    for token in [&w.other, &w.admin] {
        let next = async {
            assert_eq!(command(&w, &w.viewer, &session, json!({"command": "next"})).await.status, StatusCode::ACCEPTED);
        };
        let heard = events_until(&w, token, next, "player_command", Duration::from_millis(500)).await;
        assert!(events_of(&heard, "player_command").is_empty(), "{heard}");
    }
}

#[tokio::test]
async fn only_its_user_controls_a_session() {
    let w = world().await;
    let app = &w.app;
    let session = play(&w, &w.viewer).await;
    let pause = json!({"command": "pause"});

    let refused = command(&w, &w.other, &session, pause.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    // An administrator may stop it, nothing more.
    let refused = command(&w, &w.admin, &session, pause.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());

    assert_eq!(command(&w, &w.viewer, "no-such-session", pause.clone()).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.delete(&format!("/api/sessions/{session}"), &w.admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(command(&w, &w.viewer, &session, pause).await.status, StatusCode::NOT_FOUND, "stopped");
}

#[tokio::test]
async fn a_command_is_checked() {
    let w = world().await;
    let session = play(&w, &w.viewer).await;
    for body in [
        json!({}),
        json!({"command": "rewind"}),
        json!({"command": "seek"}),
        json!({"command": "seek", "position_ms": -1}),
        json!({"command": "seek", "position_ms": LENGTH_MS + 1}),
        json!({"command": "skip"}),
        json!({"command": "skip", "offset_ms": 600_001}),
        json!({"command": "skip", "offset_ms": -600_001}),
    ] {
        let refused = command(&w, &w.viewer, &session, body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}: {}", refused.text());
    }
    for body in [
        json!({"command": "seek", "position_ms": 0}),
        json!({"command": "seek", "position_ms": LENGTH_MS}),
        json!({"command": "skip", "offset_ms": 600_000}),
        json!({"command": "skip", "offset_ms": -600_000}),
    ] {
        assert_eq!(command(&w, &w.viewer, &session, body.clone()).await.status, StatusCode::ACCEPTED, "{body}");
    }

    // What the player didn't say it obeys.
    let pauses = play_with(&w, &w.viewer, json!({"client_id": "tv", "controls": ["pause"]}), &[]).await;
    let refused = command(&w, &w.viewer, &pauses, json!({"command": "next"})).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.body["code"], json!("not_controllable"));
    let silent = play_with(&w, &w.viewer, json!({"client_id": "kodi"}), &[]).await;
    let refused = command(&w, &w.viewer, &silent, json!({"command": "pause"})).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::CONFLICT, json!("not_controllable")));
}

#[tokio::test]
async fn commands_are_limited_each_second() {
    let w = world().await;
    let session = play(&w, &w.viewer).await;
    let mut answers = Vec::new();
    for _ in 0..12 {
        answers.push(command(&w, &w.viewer, &session, json!({"command": "pause"})).await.status);
    }
    // All within a second, unless the machine is very slow: ten at most.
    let sent = answers.iter().filter(|s| **s == StatusCode::ACCEPTED).count();
    assert!(answers.contains(&StatusCode::TOO_MANY_REQUESTS), "{answers:?}");
    assert!(sent >= 10, "{answers:?}");
    // Another session has its own.
    let other = play_with(&w, &w.viewer, json!({"client_id": "tv", "controls": ["pause"]}), &[]).await;
    assert_eq!(command(&w, &w.viewer, &other, json!({"command": "pause"})).await.status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn apps_control_with_the_play_scope() {
    let w = world().await;
    let app = &w.app;
    let session = play(&w, &w.viewer).await;
    let (phone, _) = app.app_token(w.viewer_id, "library play").await;
    let (browse, _) = app.app_token(w.viewer_id, "library").await;
    let uri = format!("/api/sessions/{session}/command");

    let listed = app.app_call(Method::GET, "/api/sessions", &phone, None).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    assert_eq!(listed.body.as_array().unwrap().len(), 1);
    let sent = app.app_call(Method::POST, &uri, &phone, Some(json!({"command": "pause"}))).await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());

    let refused = app.app_call(Method::GET, "/api/sessions", &browse, None).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::FORBIDDEN, json!("insufficient_scope")));
    let refused = app.app_call(Method::POST, &uri, &browse, Some(json!({"command": "pause"}))).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::FORBIDDEN, json!("insufficient_scope")));
}

#[tokio::test]
async fn a_report_after_a_seek_says_sessions_changed() {
    let w = world().await;
    let app = &w.app;
    let session = play(&w, &w.viewer).await;
    let progress = format!("/api/items/{}/progress", w.film);
    let report = |position_ms: i64| json!({"position_ms": position_ms, "duration_ms": LENGTH_MS, "state": "playing", "session_id": session});

    // Playing on as it was: nothing for remotes to show.
    let mut events = app.state.events.subscribe();
    assert_eq!(app.post(&progress, &w.viewer, report(1_000)).await.status, StatusCode::NO_CONTENT);
    assert!(!kinds(&mut events).contains(&"sessions_changed".to_owned()));

    // Somewhere else at once: a seek, which remotes show.
    assert_eq!(app.post(&progress, &w.viewer, report(60_000)).await.status, StatusCode::NO_CONTENT);
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()));
    let listed = app.get("/api/sessions", &w.viewer).await.body;
    assert_eq!(listed[0]["position_ms"], json!(60_000));
    // How long ago that report was, by the server's clock: remotes move the
    // place on from it whatever their own clock says.
    let age = listed[0]["age_ms"].as_i64().unwrap();
    assert!((0..5_000).contains(&age), "just reported: {age}");
    app.state.sessions.get(&session).unwrap().play.lock().reported_at_ms = Some(pley::util::now_ms() - 10_000);
    let listed = app.get("/api/sessions", &w.viewer).await.body;
    let age = listed[0]["age_ms"].as_i64().unwrap();
    assert!((10_000..15_000).contains(&age), "reported ten seconds ago: {age}");
    assert_eq!(listed[0]["position_ms"], json!(60_000), "the place is still the report's");
}

#[tokio::test]
async fn reading_the_file_between_reports_is_no_jump() {
    let w = world().await;
    let app = &w.app;
    let controls =
        json!({"item_id": w.film, "caps": serde_json::from_str::<Value>(CAPS).unwrap(), "controls": ["seek"]});
    let started = app.post("/api/playback", &w.viewer, controls).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let (session, url) = (started.body["session_id"].as_str().unwrap(), started.body["url"].as_str().unwrap());
    let progress = format!("/api/items/{}/progress", w.film);
    let report = |position_ms: i64| json!({"position_ms": position_ms, "duration_ms": LENGTH_MS, "state": "playing", "session_id": session});

    assert_eq!(app.post(&progress, &w.viewer, report(1_000)).await.status, StatusCode::NO_CONTENT);
    // Ten seconds on, with the file read just now: the player is ten
    // seconds further, which is no jump.
    app.state.sessions.get(session).unwrap().play.lock().reported_at_ms = Some(pley::util::now_ms() - 10_000);
    let read = app.call(Method::GET, url, Some(&w.viewer), None, &[("range", "bytes=0-99")]).await;
    assert_eq!(read.status, StatusCode::PARTIAL_CONTENT);
    let mut events = app.state.events.subscribe();
    assert_eq!(app.post(&progress, &w.viewer, report(11_000)).await.status, StatusCode::NO_CONTENT);
    assert!(!kinds(&mut events).contains(&"sessions_changed".to_owned()));
}

type JellyfinSocket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The Jellyfin socket of `token`'s user from device `bench-1` (the device
/// [`TestApp::jf`] sends as), once it has said hello.
async fn jellyfin_socket(w: &World, token: &str) -> JellyfinSocket {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(pley::jellyfin::serve(w.app.state.clone(), listener, std::future::pending()));
    let token = w.app.jellyfin_token(token).await;
    let url = format!("ws://{addr}/socket?api_key={token}&deviceId=bench-1");
    let mut ws = tokio_tungstenite::connect_async(url).await.unwrap().0;
    next_message(&mut ws, "ForceKeepAlive").await;
    ws
}

/// The socket's next message of type `kind`, within 10 s.
async fn next_message(ws: &mut JellyfinSocket, kind: &str) -> Value {
    let read = async {
        while let Some(message) = ws.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = message {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["MessageType"] == json!(kind) {
                    return value["Data"].clone();
                }
            }
        }
        panic!("the socket closed");
    };
    tokio::time::timeout(Duration::from_secs(10), read).await.expect("a message in time")
}

#[tokio::test]
async fn a_jellyfin_client_that_takes_media_control_obeys_through_its_socket() {
    let w = world().await;
    let app = &w.app;
    let film = app.jf_item(w.film);
    let started = app.jf(Method::POST, "/Sessions/Playing", &w.viewer, Some(json!({"ItemId": film}))).await;
    assert_eq!(started.status, StatusCode::NO_CONTENT, "{}", started.text());
    let session = app.state.sessions.list()[0].id.clone();
    let controls = || async { app.get("/api/sessions", &w.viewer).await.body[0]["controls"].clone() };

    // A client that never said it takes media control can only be stopped.
    assert_eq!(controls().await, json!([]));
    let refused = command(&w, &w.viewer, &session, json!({"command": "pause"})).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::CONFLICT, json!("not_controllable")));

    // Said so, with its socket open: every command, as Playstate, and a
    // change of track as a GeneralCommand.
    let said = app.jf(Method::POST, "/Sessions/Capabilities?SupportsMediaControl=true", &w.viewer, None).await;
    assert_eq!(said.status, StatusCode::NO_CONTENT);
    assert_eq!(controls().await, json!([]), "not without its socket");
    let mut ws = jellyfin_socket(&w, &w.viewer).await;
    let every = json!(["play", "pause", "seek", "skip", "next", "previous", "audio", "subtitles"]);
    assert_eq!(controls().await, every);
    assert_eq!(command(&w, &w.viewer, &session, json!({"command": "pause"})).await.status, StatusCode::ACCEPTED);
    assert_eq!(next_message(&mut ws, "Playstate").await, json!({"Command": "Pause"}));
    let seek = json!({"command": "seek", "position_ms": 30_000});
    assert_eq!(command(&w, &w.viewer, &session, seek).await.status, StatusCode::ACCEPTED);
    assert_eq!(
        next_message(&mut ws, "Playstate").await,
        json!({"Command": "Seek", "SeekPositionTicks": 300_000_000_i64})
    );
    let off = json!({"command": "subtitles", "stream_index": -1});
    assert_eq!(command(&w, &w.viewer, &session, off).await.status, StatusCode::ACCEPTED);
    let general = next_message(&mut ws, "GeneralCommand").await;
    assert_eq!(
        (general["Name"].clone(), general["Arguments"].clone()),
        (json!("SetSubtitleStreamIndex"), json!({"Index": "-1"}))
    );
    assert_eq!(general["ControllingUserId"], json!(app.jf_user(&w.viewer).await));
    // Its reports say which tracks it plays.
    let report =
        json!({"ItemId": film, "PositionTicks": 300_000_000_i64, "AudioStreamIndex": 1, "SubtitleStreamIndex": -1});
    assert_eq!(
        app.jf(Method::POST, "/Sessions/Playing/Progress", &w.viewer, Some(report)).await.status,
        StatusCode::NO_CONTENT
    );
    let listed = app.get("/api/sessions", &w.viewer).await.body;
    assert_eq!((listed[0]["audio_index"].clone(), listed[0]["subtitle_index"].clone()), (json!(1), json!(-1)));

    // Saying it no longer does, in the full capabilities' body.
    let body = json!({"PlayableMediaTypes": ["Video"], "SupportsMediaControl": false});
    let said = app.jf(Method::POST, "/Sessions/Capabilities/Full", &w.viewer, Some(body)).await;
    assert_eq!(said.status, StatusCode::NO_CONTENT);
    assert_eq!(controls().await, json!([]));
}

/// A stand-in IPTV provider with two channels, One (in the guide, with a
/// logo) and Two, whose streams send MPEG-TS packets for as long as they
/// are read: its playlist's address.
async fn provider() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let playlist = format!(
        "#EXTM3U url-tvg=\"{base}/guide.xml\"\n\
         #EXTINF:-1 tvg-id=\"one.test\" tvg-chno=\"1\" tvg-logo=\"{base}/logo.png\",One\n\
         {base}/live/1.ts\n\
         #EXTINF:-1 tvg-chno=\"2\",Two\n\
         {base}/live/2.ts\n"
    );
    // On from long before the test to long after it.
    let guide = r#"<?xml version="1.0" encoding="UTF-8"?>
<tv>
  <channel id="one.test"><display-name>One</display-name></channel>
  <programme start="20000101000000 +0000" stop="21000101000000 +0000" channel="one.test"><title>The News</title></programme>
</tv>"#;
    let app = axum::Router::new()
        .route("/list.m3u", axum::routing::get(move || async move { playlist }))
        .route("/guide.xml", axum::routing::get(move || async move { guide }))
        .route(
            "/live/{n}",
            axum::routing::get(|| async {
                let packets = futures::stream::unfold((), |()| async {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    let mut chunk = vec![0u8; 188 * 10];
                    for packet in chunk.chunks_mut(188) {
                        packet[0] = 0x47;
                    }
                    Some((Ok::<_, std::io::Error>(bytes::Bytes::from(chunk)), ()))
                });
                ([("content-type", "video/mp2t")], axum::body::Body::from_stream(packets))
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("{base}/list.m3u")
}

/// The world with Live TV: the provider's channels read, One's id and Two's.
async fn live_world() -> (World, i64, i64) {
    let w = world().await;
    let source = json!({"name": "Provider", "kind": "m3u", "url": provider().await, "max_connections": 4});
    let added = w.app.post("/api/live/sources", &w.admin, source).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    for _ in 0..100 {
        let sources = w.app.get("/api/live/sources", &w.admin).await.body;
        if sources[0]["guide_at"].is_i64() && !sources[0]["refreshing"].as_bool().unwrap_or(true) {
            let channels = w.app.get("/api/live/channels", &w.viewer).await.body;
            let (one, two) = (channels[0]["id"].as_i64().unwrap(), channels[1]["id"].as_i64().unwrap());
            return (w, one, two);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the source wasn't read");
}

/// `token`'s user watching `channel` as device `client`, with `fields` added
/// to the request: its session id.
async fn watch(w: &World, token: &str, channel: i64, client: &str, fields: Value) -> String {
    let mut body = json!({"client_id": client});
    body.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
    let watching = w.app.post(&format!("/api/live/channels/{channel}/watch"), token, body).await;
    assert_eq!(watching.status, StatusCode::OK, "{}", watching.text());
    watching.body["session_id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn live_viewers_are_listed_with_their_channel() {
    let (w, one, two) = live_world().await;
    let app = &w.app;
    let mut events = app.state.events.subscribe();
    let fields = json!({"controls": ["next", "previous", "fly"], "device_name": "Bedroom TV"});
    let session = watch(&w, &w.viewer, one, "tv", fields).await;
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()), "a start is a change");
    watch(&w, &w.other, two, "tv", json!({})).await;
    // The stream the TV reads for its session is that session, not another.
    let request = axum::http::Request::builder()
        .uri(format!("/api/live/channels/{one}/stream.ts?client_id=tv-stream"))
        .header("authorization", format!("Bearer {}", w.viewer))
        .body(axum::body::Body::empty())
        .unwrap();
    let reader = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    assert_eq!(reader.status(), StatusCode::OK);
    play(&w, &w.viewer).await;

    let sessions = app.get("/api/sessions", &w.viewer).await.body;
    let sessions = sessions.as_array().unwrap();
    assert_eq!(sessions.len(), 2, "the film and the channel, theirs alone: {sessions:?}");
    let live = sessions.iter().find(|s| s["kind"] == json!("live")).unwrap();
    let listed = &app.get("/api/live/channels", &w.viewer).await.body[0];
    assert_eq!(live["session_id"], json!(session));
    assert_eq!(live["device"], json!("Bedroom TV"));
    assert_eq!(live["item"], Value::Null);
    let channel = json!({"id": one, "name": "One", "number": "1", "logo": listed["logo"], "now": "The News"});
    assert_eq!(live["channel"], channel);
    assert!(live["channel"]["logo"].as_str().unwrap().starts_with(&format!("/api/live/channels/{one}/logo")));
    assert_eq!(live["state"], json!("playing"));
    assert_eq!((live["position_ms"].clone(), live["duration_ms"].clone()), (json!(0), Value::Null));
    assert_eq!(live["controls"], json!(["next", "previous"]));
    let age = live["age_ms"].as_i64().unwrap();
    assert!((0..5_000).contains(&age), "since the player last asked for the channel: {age}");
    // The channel and the film play on the same device: the sign-in's.
    let film = sessions.iter().find(|s| s["kind"] == json!("item")).unwrap();
    assert_eq!(live["device_id"], film["device_id"]);
    assert!(live["device_id"].as_str().unwrap().starts_with("web-"), "{}", live["device_id"]);
    assert_eq!((live["device_kind"].clone(), film["device_kind"].clone()), (json!("browser"), json!("browser")));

    // Changing channel on the device: the session it watched before is gone.
    // This time the player says what it plays on.
    drop(reader);
    let _ = kinds(&mut events);
    let changed = watch(&w, &w.viewer, two, "tv", json!({"device_kind": "tv"})).await;
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()), "a change of channel");
    let sessions = app.get("/api/sessions", &w.viewer).await.body;
    let live: Vec<&Value> = sessions.as_array().unwrap().iter().filter(|s| s["kind"] == json!("live")).collect();
    assert_eq!(live.len(), 1, "{live:?}");
    assert_eq!((live[0]["session_id"].clone(), live[0]["channel"]["now"].clone()), (json!(changed), Value::Null));
    assert_eq!(live[0]["controls"], json!([]), "this time it obeys none");
    assert_eq!(live[0]["device_kind"], json!("tv"));

    // Leaving: no longer listed, which remotes hear.
    let _ = kinds(&mut events);
    assert_eq!(app.delete(&format!("/api/live/sessions/{changed}"), &w.viewer).await.status, StatusCode::NO_CONTENT);
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()), "an end");
    let sessions = app.get("/api/sessions", &w.viewer).await.body;
    assert!(sessions.as_array().unwrap().iter().all(|s| s["kind"] == json!("item")), "{sessions}");
}

#[tokio::test]
async fn a_command_reaches_a_live_player() {
    let (w, one, _) = live_world().await;
    let session = watch(&w, &w.viewer, one, "tv", json!({"controls": ["next", "previous"]})).await;

    let next = async {
        let sent = command(&w, &w.viewer, &session, json!({"command": "next"})).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let heard = events_until(&w, &w.viewer, next, "player_command", Duration::from_secs(10)).await;
    let expected = json!({
        "type": "player_command", "session_id": session, "command": "next", "position_ms": null, "offset_ms": null,
        "stream_index": null,
    });
    assert_eq!(events_of(&heard, "player_command"), vec![expected]);

    // No pause or seek unless the player says it obeys them.
    for body in [json!({"command": "pause"}), json!({"command": "seek", "position_ms": 0})] {
        let refused = command(&w, &w.viewer, &session, body.clone()).await;
        assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::CONFLICT, json!("not_controllable")));
    }
    // A channel has no tracks to pick, whatever its player says.
    let tracks = watch(&w, &w.viewer, one, "tablet", json!({"controls": ["audio", "subtitles"]})).await;
    for body in [json!({"command": "audio", "stream_index": 1}), json!({"command": "subtitles", "stream_index": -1})] {
        assert_eq!(command(&w, &w.viewer, &tracks, body.clone()).await.status, StatusCode::BAD_REQUEST, "{body}");
    }
    let sessions = w.app.get("/api/sessions", &w.viewer).await.body;
    let live = sessions.as_array().unwrap().iter().find(|s| s["session_id"] == json!(tracks)).unwrap();
    assert_eq!((live["audio_index"].clone(), live["subtitle_index"].clone()), (Value::Null, Value::Null));
    assert_eq!(live["tracks"], json!({"audio": [], "subtitles": []}));
    let timeshifts = watch(&w, &w.viewer, one, "phone", json!({"controls": ["pause", "seek", "skip"]})).await;
    for body in [
        json!({"command": "pause"}),
        json!({"command": "seek", "position_ms": 3_600_000}),
        json!({"command": "skip", "offset_ms": -30_000}),
    ] {
        assert_eq!(command(&w, &w.viewer, &timeshifts, body.clone()).await.status, StatusCode::ACCEPTED, "{body}");
    }
    // Its length isn't known: a seek goes at most a day in.
    for body in [
        json!({"command": "skip", "offset_ms": 600_001}),
        json!({"command": "seek", "position_ms": 24 * 3_600_000 + 1}),
        json!({"command": "seek", "position_ms": i64::MAX}),
    ] {
        assert_eq!(command(&w, &w.viewer, &timeshifts, body.clone()).await.status, StatusCode::BAD_REQUEST, "{body}");
    }
    let day = json!({"command": "seek", "position_ms": 24 * 3_600_000});
    assert_eq!(command(&w, &w.viewer, &timeshifts, day).await.status, StatusCode::ACCEPTED);

    // Its user's alone, an administrator's request included.
    for token in [&w.other, &w.admin] {
        let refused = command(&w, token, &session, json!({"command": "next"})).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    }
}

#[tokio::test]
async fn stopping_a_live_viewer_ends_it() {
    let (w, one, _) = live_world().await;
    let app = &w.app;
    let session = watch(&w, &w.viewer, one, "tv", json!({"controls": ["next"]})).await;
    let uri = format!("/api/sessions/{session}");

    // Only its user or an administrator.
    assert_eq!(app.delete(&uri, &w.other).await.status, StatusCode::FORBIDDEN);
    let mut events = app.state.events.subscribe();
    let stop = async {
        let stopped = app.call(Method::DELETE, &uri, Some(&w.admin), Some(json!({"message": "Bedtime."})), &[]).await;
        assert_eq!(stopped.status, StatusCode::NO_CONTENT, "{}", stopped.text());
    };
    let heard = events_until(&w, &w.viewer, stop, "session_stopped", Duration::from_secs(10)).await;
    let expected = json!({"type": "session_stopped", "session_id": session, "message": "Bedtime.", "by_admin": true});
    assert_eq!(events_of(&heard, "session_stopped"), vec![expected]);
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()));

    // Its player is told why from then on.
    let gone = app.get(&format!("/api/live/sessions/{session}/index.m3u8"), &w.viewer).await;
    assert_eq!((gone.status, gone.body["message"].clone()), (StatusCode::GONE, json!("Bedtime.")), "{}", gone.text());
    assert!(app.get("/api/sessions", &w.viewer).await.body.as_array().unwrap().is_empty());
    assert_eq!(command(&w, &w.viewer, &session, json!({"command": "next"})).await.status, StatusCode::NOT_FOUND);
    // Stopped already is fine, to its user; anyone else is refused.
    assert_eq!(app.delete(&uri, &w.viewer).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.delete(&uri, &w.other).await.status, StatusCode::FORBIDDEN);

    // Its user stops their own: the stream a TV reads for it ends too.
    let session = watch(&w, &w.viewer, one, "tv", json!({})).await;
    let request = axum::http::Request::builder()
        .uri(format!("/api/live/channels/{one}/stream.ts?client_id=tv-stream"))
        .header("authorization", format!("Bearer {}", w.viewer))
        .body(axum::body::Body::empty())
        .unwrap();
    let reader = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    assert_eq!(reader.status(), StatusCode::OK);
    assert_eq!(app.delete(&format!("/api/sessions/{session}"), &w.viewer).await.status, StatusCode::NO_CONTENT);
    let mut stream = reader.into_body().into_data_stream();
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream goes on");
    let gone = app.get(&format!("/api/live/sessions/{session}/index.m3u8"), &w.viewer).await;
    assert_eq!(gone.status, StatusCode::GONE);
}

// ------------------------------------------------------------------ tracks

/// Gives the film the streams a probe would find: H.264 video, English
/// audio (the default) and French surround audio, forced English subtitles
/// and French ones.
async fn with_tracks(w: &World) {
    use pley_transcoder_api::{MediaInfo, MediaStream, StreamKind};
    let stream = |index, kind, codec: &str, language: Option<&str>| MediaStream {
        index,
        kind,
        codec: codec.into(),
        language: language.map(str::to_owned),
        ..Default::default()
    };
    let mut video = stream(0, StreamKind::Video, "h264", None);
    (video.width, video.height, video.pix_fmt) = (Some(1920), Some(1080), Some("yuv420p".into()));
    let mut english = stream(1, StreamKind::Audio, "aac", Some("eng"));
    (english.channels, english.default) = (Some(2), true);
    let mut french = stream(2, StreamKind::Audio, "aac", Some("fre"));
    (french.channels, french.title) = (Some(6), Some("Surround".into()));
    let mut signs = stream(3, StreamKind::Subtitle, "subrip", Some("eng"));
    (signs.forced, signs.title, signs.text_based) = (true, Some("Signs".into()), Some(true));
    let mut subtitles = stream(4, StreamKind::Subtitle, "subrip", Some("fre"));
    subtitles.text_based = Some(true);
    let info = MediaInfo {
        container: Some("mkv".into()),
        duration_ms: Some(LENGTH_MS),
        streams: vec![video, english, french, signs, subtitles],
        ..Default::default()
    };
    sqlx::query("UPDATE media_files SET media_info = ?, video_codec = 'h264', audio_codec = 'aac' WHERE id = ?")
        .bind(serde_json::to_string(&info).unwrap())
        .bind(w.app.file_of(w.film).await)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_change_of_track_is_one_of_the_files() {
    let w = world().await;
    with_tracks(&w).await;
    let session = play(&w, &w.viewer).await;
    // The player said it changes neither.
    for body in [json!({"command": "audio", "stream_index": 2}), json!({"command": "subtitles", "stream_index": 4})] {
        let refused = command(&w, &w.viewer, &session, body.clone()).await;
        assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::CONFLICT, json!("not_controllable")));
    }
    let controls = json!({"client_id": "tv", "controls": ["audio", "subtitles"]});
    let session = play_with(&w, &w.viewer, controls, &[]).await;

    let french = async {
        let sent = command(&w, &w.viewer, &session, json!({"command": "audio", "stream_index": 2})).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let heard = events_until(&w, &w.viewer, french, "player_command", Duration::from_secs(10)).await;
    let expected = json!({
        "type": "player_command", "session_id": session, "command": "audio", "position_ms": null, "offset_ms": null,
        "stream_index": 2,
    });
    assert_eq!(events_of(&heard, "player_command"), vec![expected]);
    let off = async {
        let body = json!({"command": "subtitles", "stream_index": -1, "position_ms": 5});
        assert_eq!(command(&w, &w.viewer, &session, body).await.status, StatusCode::ACCEPTED);
    };
    let heard = events_until(&w, &w.viewer, off, "player_command", Duration::from_secs(10)).await;
    let event = &events_of(&heard, "player_command")[0];
    assert_eq!((event["stream_index"].clone(), event["position_ms"].clone()), (json!(-1), Value::Null));

    for body in [
        json!({"command": "subtitles", "stream_index": 3}),
        json!({"command": "subtitles", "stream_index": 4}),
        json!({"command": "audio", "stream_index": 1}),
    ] {
        assert_eq!(command(&w, &w.viewer, &session, body.clone()).await.status, StatusCode::ACCEPTED, "{body}");
    }
    for body in [
        json!({"command": "audio"}),
        json!({"command": "audio", "stream_index": 0}),
        json!({"command": "audio", "stream_index": 3}),
        json!({"command": "audio", "stream_index": -1}),
        json!({"command": "audio", "stream_index": 9}),
        json!({"command": "subtitles"}),
        json!({"command": "subtitles", "stream_index": 1}),
        json!({"command": "subtitles", "stream_index": -2}),
        json!({"command": "subtitles", "stream_index": "3"}),
    ] {
        let refused = command(&w, &w.viewer, &session, body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}: {}", refused.text());
    }
}

#[tokio::test]
async fn sessions_say_which_tracks_play_and_which_there_are() {
    let w = world().await;
    let app = &w.app;
    with_tracks(&w).await;
    let session = play(&w, &w.viewer).await;
    let listed = app.get("/api/sessions", &w.viewer).await.body;
    // What the play request picked: the default audio, and the forced
    // subtitles in its language, as the user's choices have it.
    assert_eq!((listed[0]["audio_index"].clone(), listed[0]["subtitle_index"].clone()), (json!(1), json!(3)));
    let tracks = json!({
        "audio": [
            {"index": 1, "language": "eng", "title": null, "codec": "aac", "channels": 2, "default": true, "forced": false},
            {"index": 2, "language": "fre", "title": "Surround", "codec": "aac", "channels": 6, "default": false,
             "forced": false},
        ],
        "subtitles": [
            {"index": 3, "language": "eng", "title": "Signs", "codec": "subrip", "channels": null, "default": false,
             "forced": true},
            {"index": 4, "language": "fre", "title": null, "codec": "subrip", "channels": null, "default": false,
             "forced": false},
        ],
    });
    assert_eq!(listed[0]["tracks"], tracks);

    // The player says what it plays: remotes hear of a change at once.
    let progress = format!("/api/items/{}/progress", w.film);
    let report = |audio: Value, subtitles: Value| {
        json!({"position_ms": 1_000, "state": "paused", "session_id": session, "audio_index": audio,
               "subtitle_index": subtitles})
    };
    assert_eq!(app.post(&progress, &w.viewer, report(json!(1), json!(3))).await.status, StatusCode::NO_CONTENT);
    let mut events = app.state.events.subscribe();
    assert_eq!(app.post(&progress, &w.viewer, report(json!(1), json!(3))).await.status, StatusCode::NO_CONTENT);
    assert!(!kinds(&mut events).contains(&"sessions_changed".to_owned()), "the same tracks");
    assert_eq!(app.post(&progress, &w.viewer, report(json!(2), json!(-1))).await.status, StatusCode::NO_CONTENT);
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()), "another track");
    let listed = app.get("/api/sessions", &w.viewer).await.body;
    assert_eq!((listed[0]["audio_index"].clone(), listed[0]["subtitle_index"].clone()), (json!(2), json!(-1)));
    // A report that doesn't say, or says nothing a file has, changes nothing.
    assert_eq!(app.post(&progress, &w.viewer, report(Value::Null, json!(-7))).await.status, StatusCode::NO_CONTENT);
    let listed = app.get("/api/sessions", &w.viewer).await.body;
    assert_eq!((listed[0]["audio_index"].clone(), listed[0]["subtitle_index"].clone()), (json!(2), json!(-1)));

    // A file whose tracks aren't known says nothing of them.
    sqlx::query("UPDATE media_files SET media_info = NULL").execute(&app.state.db.write).await.unwrap();
    let other = play_with(&w, &w.other, json!({}), &[]).await;
    let theirs = app.get("/api/sessions", &w.other).await.body;
    assert_eq!(theirs[0]["session_id"], json!(other));
    assert_eq!((theirs[0]["audio_index"].clone(), theirs[0]["subtitle_index"].clone()), (Value::Null, Value::Null));
    assert_eq!(theirs[0]["tracks"], json!({"audio": [], "subtitles": []}));
}

// ------------------------------------------------------------------ casting

type Body = axum::body::BodyDataStream;

/// An events stream of `token`'s user, opened with `query` and held: the
/// body it sends. A receiver it names lasts as long as it is held.
async fn stream(w: &World, token: &str, query: &str) -> Body {
    let request = axum::http::Request::builder()
        .uri(format!("/api/events?{query}"))
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(w.app.api.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().into_data_stream()
}

/// What `stream` sent until `until` showed up, or `wait` passed.
async fn read_until(stream: &mut Body, until: &str, wait: Duration) -> String {
    let mut received = String::new();
    let read = async {
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if received.contains(until) {
                return;
            }
        }
    };
    let _ = tokio::time::timeout(wait, read).await;
    received
}

async fn devices(w: &World, token: &str) -> Value {
    let listed = w.app.get("/api/remote/devices", token).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.body
}

async fn cast(w: &World, token: &str, receiver: &str, body: Value) -> common::Reply {
    w.app.post(&format!("/api/remote/devices/{receiver}/cast"), token, body).await
}

#[tokio::test]
async fn a_receiver_is_listed_while_its_stream_is_open() {
    let w = world().await;
    let app = &w.app;
    let mut events = app.state.events.subscribe();
    let tv = stream(&w, &w.viewer, "receiver=tv-1&device_name=%20Den%20TV%20&device_kind=tv&casts=live,item,fax").await;
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()), "a device appeared");
    let listed = devices(&w, &w.viewer).await;
    let device_id = listed[0]["device_id"].as_str().unwrap().to_owned();
    assert!(device_id.starts_with("web-"), "{device_id}");
    let expected = json!([{"receiver_id": "tv-1", "device_id": device_id, "name": "Den TV", "kind": "tv", "casts": ["item", "live"]}]);
    assert_eq!(listed, expected);
    // Remotes merge it with what it plays by its device.
    play(&w, &w.viewer).await;
    assert_eq!(app.get("/api/sessions", &w.viewer).await.body[0]["device_id"], json!(device_id));

    // Nobody else's list has it, an administrator's neither.
    for token in [&w.other, &w.admin] {
        assert_eq!(devices(&w, token).await, json!([]));
    }
    // Streams that don't name a receiver, or name it wrong, are none.
    let plain = stream(&w, &w.viewer, "").await;
    let spaced = stream(&w, &w.viewer, "receiver=tv%202&casts=item").await;
    let long = stream(&w, &w.viewer, &format!("receiver={}&casts=item", "a".repeat(129))).await;
    let twice = stream(&w, &w.viewer, "receiver=tv-3&receiver=tv-4").await;
    assert_eq!(devices(&w, &w.viewer).await, expected);

    // Gone with its stream.
    let _ = kinds(&mut events);
    drop(tv);
    assert_eq!(devices(&w, &w.viewer).await, json!([]));
    assert!(kinds(&mut events).contains(&"sessions_changed".to_owned()), "a device went");
    drop((plain, spaced, long, twice));
}

#[tokio::test]
async fn a_device_is_listed_once() {
    let w = world().await;
    let app = &w.app;
    // Two tabs of one sign-in are one device: the one that connected last
    // is listed. Two streams of one receiver count once.
    let first = stream(&w, &w.viewer, "receiver=tab-1&device_name=Kitchen&casts=item").await;
    let second = stream(&w, &w.viewer, "receiver=tab-2&device_name=Kitchen&casts=item").await;
    let again = stream(&w, &w.viewer, "receiver=tab-2&device_name=Kitchen&casts=item").await;
    // An app's sign-in is another device, whose kind it says.
    let (android, _) = app.app_token(w.viewer_id, "library play").await;
    let phone = stream(&w, &android, "receiver=phone-1&device_name=Bedroom&device_kind=phone&casts=item,live").await;

    let listed = devices(&w, &w.viewer).await;
    let ids = |listed: &Value| -> Vec<String> {
        listed.as_array().unwrap().iter().map(|d| d["receiver_id"].as_str().unwrap().to_owned()).collect()
    };
    assert_eq!(ids(&listed), ["phone-1", "tab-2"], "by name: {listed}");
    assert_eq!((listed[0]["name"].clone(), listed[1]["name"].clone()), (json!("Bedroom"), json!("Kitchen")));
    assert!(listed[0]["device_id"].as_str().unwrap().starts_with("app-"), "{listed}");
    assert_eq!((listed[0]["kind"].clone(), listed[1]["kind"].clone()), (json!("phone"), json!("browser")));
    drop(second);
    assert_eq!(ids(&devices(&w, &w.viewer).await), ["phone-1", "tab-2"], "its other stream is open");
    drop(again);
    assert_eq!(ids(&devices(&w, &w.viewer).await), ["phone-1", "tab-1"]);

    // Apps list them with the play scope.
    let (browse, _) = app.app_token(w.viewer_id, "library").await;
    let refused = app.app_call(Method::GET, "/api/remote/devices", &browse, None).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::FORBIDDEN, json!("insufficient_scope")));
    let listed = app.app_call(Method::GET, "/api/remote/devices", &android, None).await;
    assert_eq!(ids(&listed.body), ["phone-1", "tab-1"]);
    drop((first, phone));
}

#[tokio::test]
async fn a_cast_reaches_the_users_own_streams() {
    let w = world().await;
    let mut tv = stream(&w, &w.viewer, "receiver=tv-1&casts=item").await;
    let chrome =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
    let send = async {
        let body = json!({"item_id": w.film, "start_ms": 5_000});
        let uri = "/api/remote/devices/tv-1/cast";
        let sent = w.app.call(Method::POST, uri, Some(&w.viewer), Some(body), &[("user-agent", chrome)]).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let tokens = [w.viewer.as_str(), w.other.as_str(), w.admin.as_str()];
    let heard = streams_until(&w, &tokens, send, r#""type":"cast""#, Duration::from_secs(1)).await;
    let expected = json!({
        "type": "cast", "receiver_id": "tv-1", "item_id": w.film, "playlist_id": null, "channel_id": null,
        "start_ms": 5_000, "shuffle": false, "from": "Chrome on Windows",
    });
    assert_eq!(events_of(&heard[0], "cast"), vec![expected.clone()], "the user's other streams");
    assert!(events_of(&heard[1], "cast").is_empty(), "nobody else's: {}", heard[1]);
    assert!(events_of(&heard[2], "cast").is_empty(), "an administrator's neither: {}", heard[2]);
    let received = read_until(&mut tv, r#""type":"cast""#, Duration::from_secs(10)).await;
    assert_eq!(events_of(&received, "cast"), vec![expected], "the receiver's own");

    // A playlist of the user's, shuffled.
    let made = w.app.post("/api/playlists", &w.viewer, json!({"title": "Mine", "item_ids": [w.film]})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    let playlist = made.body["id"].as_i64().unwrap();
    let send = async {
        let sent = cast(&w, &w.viewer, "tv-1", json!({"playlist_id": playlist, "shuffle": true})).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let heard = events_until(&w, &w.viewer, send, r#""type":"cast""#, Duration::from_secs(10)).await;
    let event = &events_of(&heard, "cast")[0];
    assert_eq!((event["playlist_id"].clone(), event["item_id"].clone()), (json!(playlist), Value::Null));
    assert_eq!((event["shuffle"].clone(), event["start_ms"].clone()), (json!(true), Value::Null));
}

/// Where the cast `token` sends to `tv-1` says it came from.
async fn cast_from(w: &World, token: &str) -> Value {
    let send = async {
        let uri = "/api/remote/devices/tv-1/cast";
        let body = Some(json!({"item_id": w.film}));
        let sent = w.app.call(Method::POST, uri, Some(token), body, &[("user-agent", "pleyAndroid/0.1.6")]).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let heard = events_until(w, &w.viewer, send, r#""type":"cast""#, Duration::from_secs(10)).await;
    events_of(&heard, "cast")[0]["from"].clone()
}

#[tokio::test]
async fn a_cast_comes_from_the_name_the_remote_gave_its_device() {
    let w = world().await;
    let _tv = stream(&w, &w.viewer, "receiver=tv-1&casts=item").await;
    let (android, _) = w.app.app_token(w.viewer_id, "library play").await;
    // A remote that isn't a receiver is named from its User-Agent.
    assert_eq!(cast_from(&w, &android).await, json!("Pley for Android"));
    // One that is goes by the name it gave its device there.
    let _phone = stream(&w, &android, "receiver=phone-1&device_name=Bedroom&device_kind=phone&casts=item").await;
    assert_eq!(cast_from(&w, &android).await, json!("Bedroom"));
}

#[tokio::test]
async fn a_cast_is_checked() {
    let w = world().await;
    let app = &w.app;
    let _tv = stream(&w, &w.viewer, "receiver=tv-1&casts=item").await;
    let _radio = stream(&w, &w.viewer, "receiver=radio&casts=live").await;
    let film = json!({"item_id": w.film});

    // Someone else's receiver is as unknown as one that isn't there.
    for (token, receiver) in [(&w.other, "tv-1"), (&w.admin, "tv-1"), (&w.viewer, "tv-2")] {
        let refused = cast(&w, token, receiver, film.clone()).await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND, "{receiver}: {}", refused.text());
    }
    // One target, the user may see, that the receiver plays.
    for body in [
        json!({}),
        json!({"item_id": w.film, "playlist_id": 1}),
        json!({"item_id": w.film, "channel_id": 1}),
        json!({"item_id": "film"}),
        json!({"item_id": w.film, "start_ms": -1}),
        json!({"item_id": w.film, "start_ms": 24 * 3_600_000 + 1}),
    ] {
        let refused = cast(&w, &w.viewer, "tv-1", body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}: {}", refused.text());
    }
    assert_eq!(cast(&w, &w.viewer, "tv-1", json!({"item_id": 999_999})).await.status, StatusCode::NOT_FOUND);
    let theirs = app.post("/api/playlists", &w.other, json!({"title": "Theirs", "item_ids": [w.film]})).await;
    let theirs = json!({"playlist_id": theirs.body["id"]});
    assert_eq!(cast(&w, &w.viewer, "tv-1", theirs).await.status, StatusCode::NOT_FOUND, "another user's playlist");
    let refused = cast(&w, &w.viewer, "radio", film.clone()).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::CONFLICT, json!("not_castable")));
    let day = json!({"item_id": w.film, "start_ms": 24 * 3_600_000});
    assert_eq!(cast(&w, &w.viewer, "tv-1", day).await.status, StatusCode::ACCEPTED);

    // A title in a library the user can't open is unknown to them.
    let (kid, _) = app.user(&w.admin, "kid", json!({"all_libraries": false, "library_ids": []})).await;
    let _kids_tv = stream(&w, &kid, "receiver=tv-1&casts=item").await;
    let refused = cast(&w, &kid, "tv-1", film.clone()).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());

    // Apps cast with the play scope.
    let (browse, _) = app.app_token(w.viewer_id, "library").await;
    let refused = app.app_call(Method::POST, "/api/remote/devices/tv-1/cast", &browse, Some(film.clone())).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::FORBIDDEN, json!("insufficient_scope")));
    let (phone, _) = app.app_token(w.viewer_id, "library play").await;
    let sent = app.app_call(Method::POST, "/api/remote/devices/tv-1/cast", &phone, Some(film)).await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
}

#[tokio::test]
async fn casts_are_limited_each_second() {
    let w = world().await;
    let _tv = stream(&w, &w.viewer, "receiver=tv-1&casts=item").await;
    let _box = stream(&w, &w.viewer, "receiver=box&casts=item").await;
    let mut answers = Vec::new();
    for _ in 0..12 {
        answers.push(cast(&w, &w.viewer, "tv-1", json!({"item_id": w.film})).await.status);
    }
    // All within a second, unless the machine is very slow: ten at most.
    let sent = answers.iter().filter(|s| **s == StatusCode::ACCEPTED).count();
    assert!(answers.contains(&StatusCode::TOO_MANY_REQUESTS), "{answers:?}");
    assert!(sent >= 10, "{answers:?}");
    // Another receiver has its own.
    assert_eq!(cast(&w, &w.viewer, "box", json!({"item_id": w.film})).await.status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn a_channel_is_cast_with_live_tv_access() {
    let (w, one, _) = live_world().await;
    let app = &w.app;
    let _tv = stream(&w, &w.viewer, "receiver=tv-1&casts=item,live").await;
    let _films = stream(&w, &w.viewer, "receiver=films&casts=item").await;

    let send = async {
        let sent = cast(&w, &w.viewer, "tv-1", json!({"channel_id": one})).await;
        assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    };
    let heard = events_until(&w, &w.viewer, send, r#""type":"cast""#, Duration::from_secs(10)).await;
    let event = &events_of(&heard, "cast")[0];
    assert_eq!((event["channel_id"].clone(), event["item_id"].clone()), (json!(one), Value::Null));
    let refused = cast(&w, &w.viewer, "films", json!({"channel_id": one})).await;
    assert_eq!((refused.status, refused.body["code"].clone()), (StatusCode::CONFLICT, json!("not_castable")));
    assert_eq!(cast(&w, &w.viewer, "tv-1", json!({"channel_id": 999_999})).await.status, StatusCode::NOT_FOUND);

    // Live TV for administrators only: the user casts no channel.
    let mut settings = app.get("/api/live/settings", &w.admin).await.body;
    settings["access"] = json!("admins");
    assert_eq!(app.put("/api/live/settings", &w.admin, settings).await.status, StatusCode::OK);
    let refused = cast(&w, &w.viewer, "tv-1", json!({"channel_id": one})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
}
