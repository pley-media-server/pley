//! What pley's Samsung and LG TV apps need of the server (docs/API.md,
//! "TV apps"): keyed media addresses for players that can't send the
//! Authorization header, CORS for apps their TVs load from `file://`, and
//! their built-in clients.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use common::TestApp;

const CAPS: &str = r#"{"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]}"#;

struct World {
    app: TestApp,
    viewer_id: i64,
    viewer: String,
    film: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001)/Film (2001).mkv", 4096);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let (viewer, viewer_id) = app.user(&admin, "viewer", json!({})).await;
    World { app, viewer_id, viewer, film }
}

/// A play request of the film, as a TV app makes it: `keyed_urls` set.
async fn play(w: &World, token: &str, keyed: bool) -> serde_json::Value {
    let caps: serde_json::Value = serde_json::from_str(CAPS).unwrap();
    let started =
        w.app.post("/api/playback", token, json!({"item_id": w.film, "caps": caps, "keyed_urls": keyed})).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    started.body
}

/// A request with no token at all, as a TV's player makes it.
async fn bare(app: &TestApp, uri: &str, extra: &[(&str, &str)]) -> common::Reply {
    app.call(Method::GET, uri, None, None, extra).await
}

#[tokio::test]
async fn a_keyed_address_opens_the_sessions_file_and_nothing_else() {
    let w = world().await;
    let (token, _) = w.app.app_token(w.viewer_id, "library play").await;

    // Without the flag, the address is the usual one, which needs the token.
    let plain = play(&w, &token, false).await;
    assert!(plain["url"].as_str().unwrap().starts_with("/api/files/"), "{plain}");

    let keyed = play(&w, &token, true).await;
    let url = keyed["url"].as_str().unwrap().to_owned();
    assert!(url.starts_with("/api/media/pley_mk_") && url.ends_with("/file.mkv"), "{url}");
    let key = url.trim_start_matches("/api/media/").split('/').next().unwrap().to_owned();
    assert_eq!(key.len(), "pley_mk_".len() + 43, "256 random bits");

    // The player asks with no token: the file, with ranges, never cached.
    let whole = bare(&w.app, &url, &[]).await;
    assert_eq!(whole.status, StatusCode::OK, "{}", whole.text());
    assert_eq!(whole.bytes.len(), 4096);
    assert_eq!(whole.headers[header::CACHE_CONTROL], "private, no-store");
    let part = bare(&w.app, &url, &[("range", "bytes=0-99")]).await;
    assert_eq!(part.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(part.bytes.len(), 100);

    // A key opens its own session's media only.
    assert_eq!(bare(&w.app, &format!("/api/media/{key}/index.m3u8"), &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(bare(&w.app, &format!("/api/media/{key}/../stream"), &[]).await.status, StatusCode::NOT_FOUND);
    let wrong = format!("/api/media/pley_mk_{}/file.mkv", "A".repeat(43));
    assert_eq!(bare(&w.app, &wrong, &[]).await.status, StatusCode::NOT_FOUND);
    // A key is no token: it opens no API route.
    assert_eq!(w.app.get("/api/hubs/home", &key).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_key_ends_with_its_session_and_its_sign_in() {
    let w = world().await;
    let (token, _) = w.app.app_token(w.viewer_id, "library play").await;

    // Stopped from elsewhere: 410 with the message, as the token's routes say.
    let first = play(&w, &token, true).await;
    let url = first["url"].as_str().unwrap().to_owned();
    let session = first["session_id"].as_str().unwrap();
    let stopped = w
        .app
        .call(
            Method::DELETE,
            &format!("/api/sessions/{session}"),
            Some(&w.viewer),
            Some(json!({"message": "Bedtime"})),
            &[],
        )
        .await;
    assert_eq!(stopped.status, StatusCode::NO_CONTENT, "{}", stopped.text());
    let gone = bare(&w.app, &url, &[]).await;
    assert_eq!(gone.status, StatusCode::GONE);
    assert_eq!(gone.body["code"], json!("session_stopped"));
    assert_eq!(gone.body["message"], json!("Bedtime"));

    // Ended by its player: forgotten.
    let second = play(&w, &token, true).await;
    let url = second["url"].as_str().unwrap().to_owned();
    assert_eq!(bare(&w.app, &url, &[]).await.status, StatusCode::OK);
    let ended = w
        .app
        .app_call(Method::DELETE, &format!("/api/playback/{}", second["session_id"].as_str().unwrap()), &token, None)
        .await;
    assert_eq!(ended.status, StatusCode::NO_CONTENT);
    assert_eq!(bare(&w.app, &url, &[]).await.status, StatusCode::NOT_FOUND);

    // The app signed out: its keys stop at once.
    let third = play(&w, &token, true).await;
    let url = third["url"].as_str().unwrap().to_owned();
    assert_eq!(bare(&w.app, &url, &[]).await.status, StatusCode::OK);
    let out = w.app.app_call(Method::POST, "/api/auth/logout", &token, None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{}", out.text());
    assert_eq!(bare(&w.app, &url, &[]).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_tv_apps_get_cors_and_no_website_does() {
    let w = world().await;
    let (token, _) = w.app.app_token(w.viewer_id, "library play").await;

    // A preflight from a TV app: the token's header may go.
    let preflight = w
        .app
        .call(
            Method::OPTIONS,
            "/api/hubs/home",
            None,
            None,
            &[
                ("origin", "file://"),
                ("access-control-request-method", "GET"),
                ("access-control-request-headers", "authorization"),
            ],
        )
        .await;
    assert_eq!(preflight.status, StatusCode::NO_CONTENT);
    assert_eq!(preflight.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert!(preflight.headers[header::ACCESS_CONTROL_ALLOW_HEADERS].to_str().unwrap().contains("Authorization"));

    // The call itself, from either TV's origin.
    for origin in ["file://", "null"] {
        let me = w.app.call(Method::GET, "/api/auth/me", Some(&token), None, &[("origin", origin)]).await;
        assert_eq!(me.status, StatusCode::OK);
        assert_eq!(me.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert!(me.headers[header::ACCESS_CONTROL_EXPOSE_HEADERS].to_str().unwrap().contains("WWW-Authenticate"));
    }
    // The server's metadata, which the TV reads before signing in.
    let metadata = w
        .app
        .call(
            Method::GET,
            "/.well-known/oauth-authorization-server",
            None,
            None,
            &[("origin", "file://"), ("host", "pley.local:7539")],
        )
        .await;
    assert_eq!(metadata.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");

    // Websites, and the consent page's own endpoints, get none.
    let site = w.app.call(Method::GET, "/api/auth/me", Some(&token), None, &[("origin", "https://evil.example")]).await;
    assert!(!site.headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    let consent = w
        .app
        .call(
            Method::OPTIONS,
            "/api/oauth/device/verify",
            None,
            None,
            &[("origin", "file://"), ("access-control-request-method", "POST")],
        )
        .await;
    assert!(!consent.headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

#[tokio::test]
async fn the_samsung_and_lg_apps_sign_in_with_a_code() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    for client in [
        "https://pley-media-server.github.io/pley/oauth/tizen.json",
        "https://pley-media-server.github.io/pley/oauth/webos.json",
    ] {
        let form = format!("client_id={}&scope=library%20play%20playlists%20profiles", common::encode(client));
        let started = app.form_post("/api/oauth/device", &form, &[]).await;
        assert_eq!(started.status, StatusCode::OK, "{client}: {}", started.text());
        assert!(started.body["user_code"].as_str().unwrap().len() == 9);
    }
}
