//! Casting (docs/API.md, "Casting to another device"): a device listed while its event stream is
//! open, what other devices of the profile send it, and what it says it
//! plays.

mod common;

use std::time::Duration;

use axum::body::{Body, BodyDataStream};
use axum::http::{Method, Request, StatusCode};
use futures::StreamExt;
use serde_json::{Value, json};

use common::TestApp;

const TV: &str = "tv-0123456789";

struct World {
    app: TestApp,
    viewer: String,
    viewer_id: i64,
    other: String,
    film: i64,
    show: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001)/Film (2001).mkv", 2048);
    app.media_file("tv/Show (2010)/Season 01/Show - S01E01 - Pilot.mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let film = app.ids(movies, &admin).await["Film"];
    let show = app.ids(tv, &admin).await["Show"];
    let (viewer, viewer_id) = app.user(&admin, "viewer", json!({})).await;
    let (other, _) = app.user(&admin, "other", json!({})).await;
    World { app, viewer, viewer_id, other, film, show }
}

/// The event stream of `token`, opened as cast device `query` says; `Err`
/// with its status when it is refused.
async fn open(app: &TestApp, token: &str, query: &str) -> Result<BodyDataStream, StatusCode> {
    let request = Request::builder()
        .uri(format!("/api/events?{query}"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    if response.status() != StatusCode::OK {
        return Err(response.status());
    }
    Ok(response.into_body().into_data_stream())
}

fn tv_query() -> String {
    format!("cast_device={TV}&cast_name=Living%20room&cast_platform=android_tv")
}

/// The next event of `kind` on `stream` (or a panic after 10 s).
async fn next_event(stream: &mut BodyDataStream, kind: &str) -> Value {
    let read = async {
        let mut received = String::new();
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            let found = received
                .lines()
                .filter_map(|line| line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:")))
                .filter_map(|data| serde_json::from_str::<Value>(data).ok())
                .find(|event| event["type"] == kind);
            if let Some(event) = found {
                return event;
            }
        }
        panic!("the stream ended before {kind}");
    };
    tokio::time::timeout(Duration::from_secs(10), read).await.unwrap_or_else(|_| panic!("no {kind} event"))
}

/// Whether `stream` sends an event of `kind` within half a second.
async fn hears(stream: &mut BodyDataStream, kind: &str) -> bool {
    tokio::time::timeout(Duration::from_millis(500), next_event(stream, kind)).await.is_ok()
}

#[tokio::test]
async fn a_device_is_listed_while_its_stream_is_open() {
    let w = world().await;
    let app = &w.app;
    let (tv_token, _) = app.app_token(w.viewer_id, "library play").await;
    let mut phone = open(app, &w.viewer, "").await.unwrap();

    let tv = open(app, &tv_token, &tv_query()).await.unwrap();
    assert!(next_event(&mut phone, "cast_devices_changed").await.is_object());
    let listed = app.get("/api/cast/devices", &w.viewer).await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.body, json!([{"id": TV, "name": "Living room", "platform": "android_tv", "state": null}]));
    // Only the profile's own.
    assert_eq!(app.get("/api/cast/devices", &w.other).await.body, json!([]));

    // Gone with its stream.
    drop(tv);
    assert!(next_event(&mut phone, "cast_devices_changed").await.is_object());
    assert_eq!(app.get("/api/cast/devices", &w.viewer).await.body, json!([]));
}

#[tokio::test]
async fn being_cast_to_needs_play_and_a_valid_device() {
    let w = world().await;
    let app = &w.app;
    let (browse, _) = app.app_token(w.viewer_id, "library").await;
    assert_eq!(open(app, &browse, &tv_query()).await.err(), Some(StatusCode::FORBIDDEN));
    assert!(open(app, &browse, "").await.is_ok(), "the stream itself needs library only");
    // Casting to one is play's too.
    assert_eq!(app.app_call(Method::GET, "/api/cast/devices", &browse, None).await.status, StatusCode::FORBIDDEN);
    for query in [
        format!("cast_device={TV}"),
        format!("cast_device={TV}&cast_name=TV&cast_platform=toaster"),
        "cast_device=short&cast_name=TV&cast_platform=roku".to_owned(),
        format!("cast_device={TV}&cast_name=%20%20&cast_platform=roku"),
        format!("cast_device={TV}&cast_name={}&cast_platform=roku", "x".repeat(65)),
    ] {
        assert_eq!(open(app, &w.viewer, &query).await.err(), Some(StatusCode::BAD_REQUEST), "{query}");
    }
    assert_eq!(app.get("/api/cast/devices", &w.viewer).await.body, json!([]));
}

#[tokio::test]
async fn a_device_plays_what_it_is_sent_and_says_what_it_plays() {
    let w = world().await;
    let app = &w.app;
    let (tv_token, _) = app.app_token(w.viewer_id, "library play").await;
    let mut tv = open(app, &tv_token, &tv_query()).await.unwrap();
    let mut phone = open(app, &w.viewer, "").await.unwrap();
    let play = format!("/api/cast/devices/{TV}/play");

    // Only the TV's stream hears what it is sent.
    let sent = app.post(&play, &w.viewer, json!({"item_id": w.film, "position_ms": 60_000})).await;
    assert_eq!(sent.status, StatusCode::NO_CONTENT, "{}", sent.text());
    assert_eq!(
        next_event(&mut tv, "cast_play").await,
        json!({"type": "cast_play", "device_id": TV, "item_id": w.film, "file_id": null, "position_ms": 60_000})
    );
    assert!(!hears(&mut phone, "cast_play").await, "not to the profile's other streams");
    // Where the profile left off: no position.
    assert_eq!(app.post(&play, &w.viewer, json!({"item_id": w.film})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(next_event(&mut tv, "cast_play").await["position_ms"], Value::Null);

    // What can't be: someone else's device, a device not connected, a title
    // not to be found, one that isn't a video, a file of another title.
    let refused = app.post(&play, &w.other, json!({"item_id": w.film})).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    let elsewhere = app.post("/api/cast/devices/no-such-device/play", &w.viewer, json!({"item_id": w.film})).await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
    assert_eq!(app.post(&play, &w.viewer, json!({"item_id": 999_999})).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.post(&play, &w.viewer, json!({"item_id": w.show})).await.status, StatusCode::BAD_REQUEST);
    let file = app.file_of(w.film).await;
    let wrong_file = app.post(&play, &w.viewer, json!({"item_id": w.film, "file_id": file + 1000})).await;
    assert_eq!(wrong_file.status, StatusCode::NOT_FOUND);
    let negative = app.post(&play, &w.viewer, json!({"item_id": w.film, "position_ms": -1})).await;
    assert_eq!(negative.status, StatusCode::BAD_REQUEST);

    // Steering it.
    let control = format!("/api/cast/devices/{TV}/control");
    let seek = app.post(&control, &w.viewer, json!({"action": "seek", "position_ms": 90_000})).await;
    assert_eq!(seek.status, StatusCode::NO_CONTENT);
    assert_eq!(
        next_event(&mut tv, "cast_control").await,
        json!({"type": "cast_control", "device_id": TV, "action": "seek", "position_ms": 90_000})
    );
    assert_eq!(app.post(&control, &w.viewer, json!({"action": "pause"})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(next_event(&mut tv, "cast_control").await["action"], json!("pause"));
    assert_eq!(app.post(&control, &w.viewer, json!({"action": "seek"})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        app.post(&control, &w.viewer, json!({"action": "rewind"})).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // The TV says what it plays: the profile's streams hear it, and the
    // list shows it.
    let state = format!("/api/cast/devices/{TV}/state");
    let report = json!({"item_id": w.film, "position_ms": 91_000, "duration_ms": 5_400_000, "state": "paused"});
    let reported = app.app_call(Method::PUT, &state, &tv_token, Some(report)).await;
    assert_eq!(reported.status, StatusCode::NO_CONTENT, "{}", reported.text());
    let heard = next_event(&mut phone, "cast_state").await;
    assert_eq!((&heard["device_id"], &heard["state"]["item"]["id"]), (&json!(TV), &json!(w.film)));
    assert_eq!((&heard["state"]["position_ms"], &heard["state"]["state"]), (&json!(91_000), &json!("paused")));
    let listed = app.get("/api/cast/devices", &w.viewer).await.body;
    assert_eq!(listed[0]["state"]["duration_ms"], json!(5_400_000));
    assert!(listed[0]["state"]["age_ms"].as_u64().is_some());
    // A title it can't see isn't one it plays.
    let unseen = json!({"item_id": 999_999, "position_ms": 0});
    assert_eq!(app.app_call(Method::PUT, &state, &tv_token, Some(unseen)).await.status, StatusCode::NOT_FOUND);
    // Nothing any more.
    let stopped = app.app_call(Method::PUT, &state, &tv_token, Some(json!({"item_id": null}))).await;
    assert_eq!(stopped.status, StatusCode::NO_CONTENT);
    assert_eq!(next_event(&mut phone, "cast_state").await["state"], Value::Null);
    assert_eq!(app.get("/api/cast/devices", &w.viewer).await.body[0]["state"], Value::Null);

    // Another profile can't report for it.
    assert_eq!(app.put(&state, &w.other, json!({"item_id": null})).await.status, StatusCode::NOT_FOUND);
    drop(tv);
    assert_eq!(app.post(&play, &w.viewer, json!({"item_id": w.film})).await.status, StatusCode::NOT_FOUND);
}
