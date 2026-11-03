//! A user's watch state reaching their other devices (docs/API.md,
//! "Activity & events"): `user_state_changed` when something is watched or
//! unwatched, a resume point starts, settles or ends, or a title leaves
//! Continue Watching, through pley's API and the Jellyfin-compatible one; never
//! for the reports that only keep the place.

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use futures::StreamExt;
use pley::events::ServerEvent;
use serde_json::{Value, json};
use tokio::sync::broadcast::Receiver;

use common::TestApp;

/// A film, a show of two episodes in one season, and a second user.
struct World {
    app: TestApp,
    admin: String,
    other: String,
    film: i64,
    show: i64,
    season: i64,
    pilot: i64,
    second: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001)/Film (2001).mkv", 2048);
    for episode in ["S01E01 - Pilot", "S01E02 - Second"] {
        app.media_file(&format!("tv/Show (2010)/Season 01/Show - {episode}.mkv"), 2048);
    }
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let film = app.ids(movies, &admin).await["Film"];
    let show = app.ids(tv, &admin).await["Show"];
    let season = app.children(show, &admin).await["Season 1"];
    let episodes = app.children(season, &admin).await;
    let (other, _) = app.user(&admin, "other", json!({})).await;
    World { app, admin, other, film, show, season, pilot: episodes["Pilot"], second: episodes["Second"] }
}

/// The `user_state_changed` events sent since the last look: their
/// `(item_ids, children)`.
fn heard(events: &mut Receiver<ServerEvent>) -> Vec<(Vec<i64>, bool)> {
    let mut out = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let ServerEvent::UserStateChanged { item_ids, children, .. } = event {
            out.push((item_ids, children));
        }
    }
    out
}

/// The administrator's player reports `position_ms` of 100 s.
async fn report(w: &World, item: i64, position_ms: i64, state: &str) {
    let body = json!({"position_ms": position_ms, "duration_ms": 100_000, "state": state});
    let reply = w.app.post(&format!("/api/items/{item}/progress"), &w.admin, body).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.text());
}

/// `POST /api/items/{item}/watched` (or `unwatched`) as `token`'s user.
async fn mark(w: &World, token: &str, item: i64, watched: bool) {
    let how = if watched { "watched" } else { "unwatched" };
    let reply = w.app.post(&format!("/api/items/{item}/{how}"), token, json!({})).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.text());
}

#[tokio::test]
async fn progress_is_told_when_it_changes_what_devices_show() {
    let w = world().await;
    let mut events = w.app.state.events.subscribe();
    let film = || (vec![w.film], false);

    // The first seconds keep no resume point: nothing to tell.
    report(&w, w.film, 10_000, "playing").await;
    assert!(heard(&mut events).is_empty());
    // A resume point starts: the film is in Continue Watching now.
    report(&w, w.film, 20_000, "playing").await;
    assert_eq!(heard(&mut events), [film()]);
    // Reports while it plays only keep the place.
    report(&w, w.film, 30_000, "playing").await;
    report(&w, w.film, 40_000, "buffering").await;
    assert!(heard(&mut events).is_empty());
    // Paused somewhere new: where Play starts elsewhere moved, once.
    report(&w, w.film, 42_000, "paused").await;
    report(&w, w.film, 42_000, "paused").await;
    assert_eq!(heard(&mut events), [film()]);
    report(&w, w.film, 52_000, "playing").await;
    assert!(heard(&mut events).is_empty());
    // Watched: once, and not again for the reports during the credits.
    report(&w, w.film, 90_000, "playing").await;
    report(&w, w.film, 99_000, "playing").await;
    report(&w, w.film, 99_500, "stopped").await;
    assert_eq!(heard(&mut events), [film()]);
    // Watched again: a resume point starts, then the end clears it.
    report(&w, w.film, 30_000, "playing").await;
    report(&w, w.film, 95_000, "stopped").await;
    assert_eq!(heard(&mut events), [film(), film()]);
    // Stopped short of a resume point, after one: it is gone.
    report(&w, w.film, 30_000, "playing").await;
    report(&w, w.film, 5_000, "stopped").await;
    assert_eq!(heard(&mut events), [film(), film()]);

    // An episode comes with its season and show, whose counts follow it.
    report(&w, w.pilot, 20_000, "playing").await;
    assert_eq!(heard(&mut events), [(vec![w.pilot, w.season, w.show], false)]);

    // A report sent late, from before something newer, changes nothing:
    // nothing to tell either.
    let late = json!({"position_ms": 60_000, "duration_ms": 100_000, "state": "stopped", "age_ms": 3_600_000});
    let reply = w.app.post(&format!("/api/items/{}/progress", w.pilot), &w.admin, late).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert!(heard(&mut events).is_empty());
}

#[tokio::test]
async fn marks_are_told_once_whatever_they_cover() {
    let w = world().await;
    let admin = w.admin.as_str();
    let mut events = w.app.state.events.subscribe();

    // A whole show: one event, the show with what is under it.
    mark(&w, admin, w.show, true).await;
    assert_eq!(heard(&mut events), [(vec![w.show], true)]);
    // Watched already: nothing changes, nothing to tell.
    mark(&w, admin, w.pilot, true).await;
    mark(&w, admin, w.season, true).await;
    assert!(heard(&mut events).is_empty());
    // An episode, with its season and show.
    mark(&w, admin, w.second, false).await;
    assert_eq!(heard(&mut events), [(vec![w.second, w.season, w.show], false)]);
    // A season, with its show.
    mark(&w, admin, w.season, false).await;
    assert_eq!(heard(&mut events), [(vec![w.season, w.show], true)]);
    mark(&w, admin, w.pilot, false).await;
    mark(&w, admin, w.film, false).await;
    assert!(heard(&mut events).is_empty());

    // A resume point counts as not watched: marking it watched is told.
    report(&w, w.film, 30_000, "stopped").await;
    heard(&mut events);
    mark(&w, admin, w.film, true).await;
    assert_eq!(heard(&mut events), [(vec![w.film], false)]);

    // Taken out of Continue Watching (an episode takes its show), and back
    // as it plays again.
    report(&w, w.second, 30_000, "stopped").await;
    heard(&mut events);
    let hidden = w.app.delete(&format!("/api/continue-watching/{}", w.second), admin).await;
    assert_eq!(hidden.status, StatusCode::NO_CONTENT, "{}", hidden.text());
    assert_eq!(heard(&mut events), [(vec![w.show], false)]);
    report(&w, w.second, 40_000, "playing").await;
    assert_eq!(heard(&mut events), [(vec![w.second, w.season, w.show], false)]);
}

#[tokio::test]
async fn jellyfin_clients_marks_and_reports_are_told_too() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    // Players report where they are, not how long the title is.
    sqlx::query("UPDATE items SET duration_ms = 100000").execute(&app.state.db.write).await.unwrap();
    let mut events = app.state.events.subscribe();
    let jellyfin = |method: Method, uri: String, body: Option<Value>| async move {
        let reply = app.jf(method, &uri, admin, body).await;
        assert!(reply.status.is_success(), "{uri}: {}", reply.text());
    };

    // Marking a season played marks its episodes: one event.
    jellyfin(Method::POST, format!("/UserPlayedItems/{}", app.jf_item(w.season)), None).await;
    assert_eq!(heard(&mut events), [(vec![w.season, w.show], true)]);
    // Marked again: nothing changes.
    jellyfin(Method::POST, format!("/UserPlayedItems/{}", app.jf_item(w.pilot)), None).await;
    assert!(heard(&mut events).is_empty());
    jellyfin(Method::DELETE, format!("/UserPlayedItems/{}", app.jf_item(w.pilot)), None).await;
    assert_eq!(heard(&mut events), [(vec![w.pilot, w.season, w.show], false)]);

    // A player's reports: a resume point starts, ticks keep the place, the
    // stop settles it.
    let film = app.jf_item(w.film);
    for (ms, path) in [(5_000, ""), (20_000, "/Progress"), (30_000, "/Progress"), (35_000, "/Stopped")] {
        let body = json!({"ItemId": film, "PositionTicks": ms * 10_000});
        jellyfin(Method::POST, format!("/Sessions/Playing{path}"), Some(body)).await;
    }
    assert_eq!(heard(&mut events), [(vec![w.film], false), (vec![w.film], false)]);
    let body = json!({"ItemId": film, "PositionTicks": 95_000 * 10_000_i64});
    jellyfin(Method::POST, "/Sessions/Playing/Progress".into(), Some(body)).await;
    assert_eq!(heard(&mut events), [(vec![w.film], false)]);
}

#[tokio::test]
async fn only_the_users_own_streams_hear_it() {
    let w = world().await;
    let request = axum::http::Request::builder()
        .uri("/api/events")
        .header("authorization", format!("Bearer {}", w.admin))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(w.app.api.clone(), request).await.unwrap();
    let mut stream = response.into_body().into_data_stream();
    // The other user's mark first: the administrator's stream never hears of it.
    mark(&w, &w.other, w.film, true).await;
    mark(&w, &w.admin, w.pilot, true).await;
    w.app.state.events.send(ServerEvent::LibrariesChanged);

    let mut received = String::new();
    let read = async {
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if received.contains("libraries_changed") {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read).await.expect("the events");
    let heard: Vec<Value> = received
        .lines()
        .filter_map(|line| line.strip_prefix("data: ").or_else(|| line.strip_prefix("data:")))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .filter(|event| event["type"] == "user_state_changed")
        .collect();
    let expected = json!({"type": "user_state_changed", "item_ids": [w.pilot, w.season, w.show], "children": false});
    assert_eq!(heard, [expected], "{received}");
}
