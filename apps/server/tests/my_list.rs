//! Taking titles out of Continue Watching, and My List (the watchlist):
//! pley's API and the Jellyfin-compatible one (docs/API.md, "Continue Watching"
//! and "My List").

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, jf_names, titles};

/// Movies and a show of three episodes, rated, and a kid who may watch
/// titles up to 12 years.
struct World {
    app: TestApp,
    admin: String,
    kid: String,
    movies: i64,
    tv: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "Family Film (2019)"] {
        app.media_file(&format!("movies/{name}/{name}.mkv"), 2048);
    }
    for episode in ["S01E01 - Pilot", "S01E02 - Second", "S01E03 - Third"] {
        app.media_file(&format!("tv/Show (2010)/Season 01/Show - {episode}.mkv"), 2048);
    }
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    for (title, rating) in [("Alien", "R"), ("Brazil", "R"), ("Family Film", "PG"), ("Show", "TV-14")] {
        app.set_rating(title, Some(rating)).await;
    }
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 12})).await;
    World { app, admin, kid, movies, tv }
}

/// Titles of a hub of `hubs` (`GET /api/hubs/home`, `/api/libraries/{id}/hubs`).
fn hub(hubs: &Value, key: &str) -> Vec<String> {
    hubs["hubs"].as_array().unwrap().iter().find(|h| h["key"] == key).map(|h| titles(&h["items"])).unwrap_or_default()
}

fn keys(hubs: &Value) -> Vec<String> {
    hubs["hubs"].as_array().unwrap().iter().map(|h| h["key"].as_str().unwrap().to_owned()).collect()
}

async fn progress(app: &TestApp, token: &str, item: i64, position_ms: i64) {
    let body = json!({"position_ms": position_ms, "duration_ms": 100_000});
    let reply = app.post(&format!("/api/items/{item}/progress"), token, body).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.text());
}

/// Plays are told apart by the second.
async fn next_second() {
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
}

#[tokio::test]
async fn titles_taken_out_of_continue_watching_come_back_when_played() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (alien, brazil) = (movies["Alien"], movies["Brazil"]);
    let show = app.ids(w.tv, admin).await["Show"];
    let episodes = app.children(app.children(show, admin).await["Season 1"], admin).await;
    let (pilot, second, third) = (episodes["Pilot"], episodes["Second"], episodes["Third"]);

    progress(app, admin, pilot, 95_000).await;
    next_second().await;
    progress(app, admin, alien, 50_000).await;
    next_second().await;
    progress(app, admin, brazil, 40_000).await;
    let home = || async { hub(&app.get("/api/hubs/home", admin).await.body, "continue_watching") };
    assert_eq!(home().await, ["Brazil", "Alien", "Second"]);

    // A movie: gone from every Continue Watching, its place kept.
    let hidden = app.delete(&format!("/api/continue-watching/{alien}"), admin).await;
    assert_eq!(hidden.status, StatusCode::NO_CONTENT, "{}", hidden.text());
    assert_eq!(home().await, ["Brazil", "Second"]);
    let library = app.get(&format!("/api/libraries/{}/hubs", w.movies), admin).await;
    assert_eq!(hub(&library.body, &format!("continue_watching:{}", w.movies)), ["Brazil"]);
    assert_eq!(jf_names(&app.jf_json("/UserItems/Resume", admin).await), ["Brazil"]);
    assert_eq!(jf_names(&app.jf_json("/Shows/NextUp", admin).await), ["Second"]);
    let state = &app.get(&format!("/api/items/{alien}"), admin).await.body["user_state"];
    assert_eq!((state["position_ms"].clone(), state["watched"].clone()), (json!(50_000), json!(false)));

    // A next-up episode takes its whole show out, an episode in progress too.
    assert_eq!(app.delete(&format!("/api/continue-watching/{second}"), admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(home().await, ["Brazil"]);
    let tv = app.get(&format!("/api/libraries/{}/hubs", w.tv), admin).await;
    assert!(hub(&tv.body, &format!("continue_watching:{}", w.tv)).is_empty());
    assert!(jf_names(&app.jf_json("/Shows/NextUp", admin).await).is_empty());

    // Playing again brings them back as they would have been.
    next_second().await;
    progress(app, admin, alien, 52_000).await;
    assert_eq!(home().await, ["Alien", "Brazil"]);
    next_second().await;
    progress(app, admin, second, 30_000).await;
    assert_eq!(home().await, ["Second", "Alien", "Brazil"]);

    // An episode in progress takes its show out; marking an episode watched
    // by hand counts as playing it.
    assert_eq!(app.delete(&format!("/api/continue-watching/{second}"), admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(home().await, ["Alien", "Brazil"]);
    assert_eq!(app.post(&format!("/api/items/{third}/watched"), admin, json!({})).await.status, StatusCode::NO_CONTENT);
    assert!(home().await.contains(&"Second".to_owned()));

    // Only the user's own Continue Watching changes; titles they can't see
    // answer 404.
    progress(app, &w.kid, movies["Family Film"], 30_000).await;
    assert_eq!(app.delete(&format!("/api/continue-watching/{alien}"), &w.kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.delete("/api/continue-watching/999999", admin).await.status, StatusCode::NOT_FOUND);
    app.delete(&format!("/api/continue-watching/{}", movies["Family Film"]), admin).await;
    assert_eq!(hub(&app.get("/api/hubs/home", &w.kid).await.body, "continue_watching"), ["Family Film"]);
}

#[tokio::test]
async fn apps_take_titles_out_with_the_play_scope() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let alien = app.ids(w.movies, admin).await["Alien"];
    let me = app.get("/api/auth/me", admin).await.body["id"].as_i64().unwrap();
    progress(app, admin, alien, 50_000).await;
    let (browse, _) = app.app_token(me, "library").await;
    let (play, _) = app.app_token(me, "library play").await;
    let uri = format!("/api/continue-watching/{alien}");
    assert_eq!(app.app_call(Method::DELETE, &uri, &browse, None).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.app_call(Method::DELETE, &uri, &play, None).await.status, StatusCode::NO_CONTENT);
    for (method, uri, token, status) in [
        (Method::GET, "/api/watchlist".to_owned(), &browse, StatusCode::OK),
        (Method::PUT, format!("/api/watchlist/{alien}"), &browse, StatusCode::FORBIDDEN),
        (Method::PUT, format!("/api/watchlist/{alien}"), &play, StatusCode::NO_CONTENT),
        (Method::DELETE, format!("/api/watchlist/{alien}"), &browse, StatusCode::FORBIDDEN),
        (Method::DELETE, format!("/api/watchlist/{alien}"), &play, StatusCode::NO_CONTENT),
    ] {
        assert_eq!(app.app_call(method.clone(), &uri, token, None).await.status, status, "{method} {uri}");
    }
}

#[tokio::test]
async fn my_list_keeps_movies_until_watched_and_shows_until_removed() {
    use pley::events::ServerEvent;

    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (alien, brazil) = (movies["Alien"], movies["Brazil"]);
    let show = app.ids(w.tv, admin).await["Show"];
    let season = app.children(show, admin).await["Season 1"];
    let pilot = app.children(season, admin).await["Pilot"];
    let list = || async {
        let reply = app.get("/api/watchlist", admin).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
        titles(&reply.body["items"])
    };
    assert!(list().await.is_empty());
    assert!(!keys(&app.get("/api/hubs/home", admin).await.body).contains(&"my_list".to_owned()));

    let mut events = app.state.events.subscribe();
    for id in [alien, show, alien] {
        assert_eq!(app.put(&format!("/api/watchlist/{id}"), admin, json!({})).await.status, StatusCode::NO_CONTENT);
    }
    // Added twice, said once.
    let mut changes = 0;
    while let Ok(event) = events.try_recv() {
        if matches!(event, ServerEvent::WatchlistChanged { .. }) {
            changes += 1;
        }
    }
    assert_eq!(changes, 2);
    for refused in [season, pilot] {
        assert_eq!(
            app.put(&format!("/api/watchlist/{refused}"), admin, json!({})).await.status,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(app.put("/api/watchlist/999999", admin, json!({})).await.status, StatusCode::NOT_FOUND);

    // Newest first, as any listing's items.
    assert_eq!(list().await, ["Show", "Alien"]);
    let items = app.get("/api/watchlist", admin).await.body["items"].clone();
    assert_eq!(items[1]["user_state"]["watched"], json!(false));
    assert_eq!(items[0]["kind"], json!("show"));
    assert_eq!(app.get(&format!("/api/items/{alien}"), admin).await.body["in_watchlist"], json!(true));
    assert_eq!(app.get(&format!("/api/items/{brazil}"), admin).await.body["in_watchlist"], json!(false));

    // On Home after Continue Watching.
    progress(app, admin, brazil, 30_000).await;
    let home = app.get("/api/hubs/home", admin).await.body;
    assert_eq!(keys(&home)[..2], ["continue_watching", "my_list"]);
    assert_eq!(home["hubs"][1]["title"], json!("My List"));
    assert_eq!(hub(&home, "my_list"), ["Show", "Alien"]);

    // A movie watched to the end leaves; a show stays (an episode watched,
    // or the whole show marked watched).
    progress(app, admin, alien, 50_000).await;
    assert_eq!(list().await, ["Show", "Alien"]);
    let mut events = app.state.events.subscribe();
    progress(app, admin, alien, 95_000).await;
    assert_eq!(list().await, ["Show"]);
    let sent: Vec<ServerEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(sent.iter().any(|e| matches!(e, ServerEvent::WatchlistChanged { .. })), "{sent:?}");
    progress(app, admin, pilot, 95_000).await;
    app.post(&format!("/api/items/{show}/watched"), admin, json!({})).await;
    assert_eq!(list().await, ["Show"]);
    // Marked watched by hand, a movie leaves too.
    app.put(&format!("/api/watchlist/{brazil}"), admin, json!({})).await;
    app.post(&format!("/api/items/{brazil}/watched"), admin, json!({})).await;
    assert_eq!(list().await, ["Show"]);
    // So does one marked watched through the Jellyfin API.
    app.put(&format!("/api/watchlist/{alien}"), admin, json!({})).await;
    let played = app.jf(Method::POST, &format!("/UserPlayedItems/{}", app.jf_item(alien)), admin, None).await;
    assert_eq!(played.status, StatusCode::OK, "{}", played.text());
    assert_eq!(list().await, ["Show"]);

    // Removed: gone, and Home's hub with it.
    assert_eq!(app.delete(&format!("/api/watchlist/{show}"), admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.delete(&format!("/api/watchlist/{show}"), admin).await.status, StatusCode::NO_CONTENT);
    assert!(list().await.is_empty());
    assert!(!keys(&app.get("/api/hubs/home", admin).await.body).contains(&"my_list".to_owned()));
    assert_eq!(app.get(&format!("/api/items/{show}"), admin).await.body["in_watchlist"], json!(false));
}

#[tokio::test]
async fn my_list_is_each_users_own_and_follows_age_limits() {
    let w = world().await;
    let (app, admin, kid) = (&w.app, w.admin.as_str(), w.kid.as_str());
    let movies = app.ids(w.movies, admin).await;
    let (alien, family) = (movies["Alien"], movies["Family Film"]);
    app.put(&format!("/api/watchlist/{alien}"), admin, json!({})).await;
    // Above the kid's age limit: as if it didn't exist.
    assert_eq!(app.put(&format!("/api/watchlist/{alien}"), kid, json!({})).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.put(&format!("/api/watchlist/{family}"), kid, json!({})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(titles(&app.get("/api/watchlist", kid).await.body["items"]), ["Family Film"]);
    assert_eq!(titles(&app.get("/api/watchlist", admin).await.body["items"]), ["Alien"]);

    // Re-rated above it, the film leaves the kid's list (and Home), and
    // comes back with its rating; it can still be taken out meanwhile.
    app.set_rating("Family Film", Some("R")).await;
    assert!(titles(&app.get("/api/watchlist", kid).await.body["items"]).is_empty());
    assert!(hub(&app.get("/api/hubs/home", kid).await.body, "my_list").is_empty());
    app.set_rating("Family Film", Some("PG")).await;
    assert_eq!(titles(&app.get("/api/watchlist", kid).await.body["items"]), ["Family Film"]);
    app.set_rating("Family Film", Some("R")).await;
    assert_eq!(app.delete(&format!("/api/watchlist/{family}"), kid).await.status, StatusCode::NO_CONTENT);
    app.set_rating("Family Film", Some("PG")).await;
    assert!(titles(&app.get("/api/watchlist", kid).await.body["items"]).is_empty());
}

#[tokio::test]
async fn my_list_events_reach_only_their_user() {
    use futures::StreamExt;

    let w = world().await;
    let (app, admin, kid) = (&w.app, w.admin.as_str(), w.kid.as_str());
    let family = app.ids(w.movies, admin).await["Family Film"];
    let request = axum::http::Request::builder()
        .uri("/api/events")
        .header("authorization", format!("Bearer {admin}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    let mut stream = response.into_body().into_data_stream();
    // The kid's change first: the administrator's stream never hears of it.
    app.put(&format!("/api/watchlist/{family}"), kid, json!({})).await;
    app.put(&format!("/api/watchlist/{family}"), admin, json!({})).await;
    app.state.events.send(pley::events::ServerEvent::LibrariesChanged);

    let mut received = String::new();
    let read = async {
        while let Some(chunk) = stream.next().await {
            received.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
            if received.contains("libraries_changed") {
                return;
            }
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), read).await.expect("the events");
    assert_eq!(received.matches(r#"{"type":"watchlist_changed"}"#).count(), 1, "{received}");
}
