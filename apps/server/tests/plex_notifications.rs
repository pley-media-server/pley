//! `/:/websockets/notifications` over a real socket (docs/COMPATIBILITY.md): playback
//! states with the session keys of `/status/sessions`, new items as Tautulli
//! expects them, scans, and who hears about what.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use axum::http::{Method, StatusCode};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

use common::TestApp;

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The Plex API of `app` on a port of its own.
async fn serve(app: &TestApp) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stopping = app.state.stopping.clone();
    tokio::spawn(pley::plex::serve(app.state.clone(), listener, async move { stopping.cancelled().await }));
    addr
}

async fn connect(addr: SocketAddr, token: &str) -> Socket {
    let url = format!("ws://{addr}/:/websockets/notifications?X-Plex-Token={token}");
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

/// Notifications (their `NotificationContainer`s) until `done` holds for
/// the ones read so far.
async fn notes_until(socket: &mut Socket, done: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    let mut notes = Vec::new();
    let read = async {
        while !done(&notes) {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let note: Value = serde_json::from_str(text.as_str()).unwrap();
                    notes.push(note["NotificationContainer"].clone());
                }
                Some(Ok(_)) => {}
                other => panic!("the socket ended: {other:?}"),
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(15), read).await.expect("notifications in time");
    notes
}

/// The entries of the notifications of type `kind`.
fn entries(notes: &[Value], kind: &str, element: &str) -> Vec<Value> {
    notes
        .iter()
        .filter(|n| n["type"] == json!(kind))
        .flat_map(|n| n[element].as_array().cloned().unwrap_or_default())
        .collect()
}

fn playing(notes: &[Value]) -> Vec<Value> {
    entries(notes, "playing", "PlaySessionStateNotification")
}

fn timeline(notes: &[Value]) -> Vec<Value> {
    entries(notes, "timeline", "TimelineEntry")
}

/// Whether the timeline has `title` in `state`.
fn has(notes: &[Value], title: &str, state: i64) -> bool {
    timeline(notes).iter().any(|e| e["title"] == json!(title) && e["state"] == json!(state))
}

async fn report(app: &TestApp, token: &str, item: i64, state: &str, time: i64) {
    let uri =
        format!("/:/timeline?ratingKey={item}&key=/library/metadata/{item}&state={state}&time={time}&duration=600000");
    let reply = app.plex(Method::GET, &uri, token, &[("x-plex-client-identifier", "living-room-tv")]).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
}

#[tokio::test]
async fn playback_states_with_the_keys_of_status_sessions() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let alien = app.ids(movies, &admin).await["Alien"];
    let (kid, _) = app.user(&admin, "kid", json!({})).await;
    let addr = serve(&app).await;
    let mut watching = connect(addr, &admin).await;
    let mut other = connect(addr, &kid).await;

    report(&app, &admin, alien, "playing", 5_000).await;
    let notes = notes_until(&mut watching, |n| !playing(n).is_empty()).await;
    let started = playing(&notes)[0].clone();
    assert_eq!(started["state"], json!("playing"));
    assert_eq!(started["ratingKey"], json!(alien.to_string()));
    assert_eq!(started["key"], json!(format!("/library/metadata/{alien}")));
    assert_eq!(started["viewOffset"], json!(5_000));
    assert_eq!(started["clientIdentifier"], json!("living-room-tv"));
    let sessions = app.plex_json("/status/sessions", &admin).await;
    assert_eq!(sessions["Metadata"][0]["sessionKey"], started["sessionKey"], "the key /status/sessions lists");

    report(&app, &admin, alien, "paused", 7_000).await;
    let notes = notes_until(&mut watching, |n| !playing(n).is_empty()).await;
    let paused = &playing(&notes)[0];
    assert_eq!((&paused["state"], &paused["viewOffset"]), (&json!("paused"), &json!(7_000)));
    assert_eq!(paused["sessionKey"], started["sessionKey"]);

    report(&app, &admin, alien, "stopped", 7_000).await;
    let notes = notes_until(&mut watching, |n| !playing(n).is_empty()).await;
    assert_eq!(playing(&notes)[0]["state"], json!("stopped"));

    // Someone else's playback isn't the kid's business: the first thing the
    // kid hears about is a new film.
    app.media_file("movies/Brazil (1985).mkv", 1024);
    app.post(&format!("/api/libraries/{movies}/scan"), &admin, json!({"force": false})).await;
    let heard = notes_until(&mut other, |n| has(n, "Brazil", 5)).await;
    assert!(playing(&heard).is_empty(), "{heard:?}");
}

#[tokio::test]
async fn another_server_does_not_change_the_keys() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let alien = app.ids(movies, &admin).await["Alien"];
    let mut watching = connect(serve(&app).await, &admin).await;
    report(&app, &admin, alien, "playing", 5_000).await;
    let notes = notes_until(&mut watching, |n| !playing(n).is_empty()).await;

    // A server in the same process forgets the sessions it doesn't have.
    let other = TestApp::new().await;
    let other_admin = other.setup_admin().await;
    other.plex_json("/status/sessions", &other_admin).await;
    let sessions = app.plex_json("/status/sessions", &admin).await;
    assert_eq!(sessions["Metadata"][0]["sessionKey"], playing(&notes)[0]["sessionKey"]);
}

#[tokio::test]
async fn new_items_and_scans_where_the_user_may_look() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 1024);
    app.media_file("private/Secret (2001).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let private = app.library(&admin, "Private", "movies", "private").await;
    let (kid, _) = app.user(&admin, "kid", json!({"all_libraries": false, "library_ids": [movies]})).await;
    let addr = serve(&app).await;
    let mut everything = connect(addr, &admin).await;
    let mut some = connect(addr, &kid).await;

    // Only what is added from now on is news.
    app.media_file("private/Hidden (2002).mkv", 1024);
    app.post(&format!("/api/libraries/{private}/scan"), &admin, json!({"force": false})).await;
    app.wait_for_scans().await;
    app.media_file("movies/Brazil (1985).mkv", 1024);
    app.post(&format!("/api/libraries/{movies}/scan"), &admin, json!({"force": false})).await;

    let seen = notes_until(&mut everything, |n| has(n, "Hidden", 5) && has(n, "Brazil", 5)).await;
    let added = timeline(&seen);
    let brazil = added.iter().find(|e| e["title"] == json!("Brazil") && e["state"] == json!(0)).expect("created");
    assert_eq!(brazil["metadataState"], json!("created"));
    assert_eq!(brazil["identifier"], json!("com.plexapp.plugins.library"));
    assert_eq!(brazil["sectionID"], json!(movies.to_string()));
    assert_eq!(brazil["type"], json!(1));
    assert!(!timeline(&seen).iter().any(|e| ["Alien", "Secret"].contains(&e["title"].as_str().unwrap())), "old news");
    let activity = entries(&seen, "activity", "ActivityNotification");
    assert!(
        activity.iter().any(|a| a["event"] == json!("started") && a["Activity"]["title"] == json!("Scanning Private"))
    );
    assert!(activity.iter().any(|a| a["event"] == json!("ended")));
    let status = entries(&seen, "status", "StatusNotification");
    assert!(status.iter().any(|s| s["notificationName"] == json!("LIBRARY_UPDATE")));

    // The kid hears about Movies only.
    let heard = notes_until(&mut some, |n| has(n, "Brazil", 5)).await;
    assert!(!has(&heard, "Hidden", 0) && !has(&heard, "Hidden", 5), "{heard:?}");
    let scans = entries(&heard, "activity", "ActivityNotification");
    assert!(scans.iter().all(|a| a["Activity"]["title"] == json!("Scanning Movies")), "{scans:?}");
}
