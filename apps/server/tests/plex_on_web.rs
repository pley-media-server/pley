//! The Plex-compatible API on pley's own port (`plex_bind = "web"`, the
//! default, src/plex/mod.rs `on_web_port`): its clients get it, browsers the
//! web app, on the same paths. The other Plex tests go through this port too.

mod common;

use axum::http::{Method, StatusCode, header};

use common::TestApp;

fn header(reply: &common::Reply, name: header::HeaderName) -> &str {
    reply.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

#[tokio::test]
async fn its_clients_and_browsers_share_pleys_port() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;

    // A Plex client's `/`: the API, sandboxed on the web app's origin, kept
    // apart from the web app's `/` by caches.
    let root = app.plex(Method::GET, "/", &admin, &[]).await;
    assert_eq!(root.status, StatusCode::OK, "{}", root.text());
    assert!(root.body["MediaContainer"]["machineIdentifier"].is_string(), "{}", root.text());
    assert!(header(&root, header::CONTENT_SECURITY_POLICY).contains("sandbox"));
    let cache = header(&root, header::CACHE_CONTROL);
    assert!(!cache.is_empty() && !cache.contains("public"), "{cache}");
    assert!(header(&root, header::VARY).contains("X-Plex-Token"));

    // A browser's `/`: the web app, varying the same way.
    let page = app.call(Method::GET, "/", None, None, &[]).await;
    assert!(!page.text().contains("MediaContainer"), "{}", page.text());
    assert!(header(&page, header::VARY).contains("X-Plex-Token"));

    // Its own paths need no headers (a player's segments, /identity), and
    // the browser's cookie opens nothing there.
    let identity = app.call(Method::GET, "/identity", None, None, &[]).await;
    assert!(identity.text().contains("machineIdentifier"), "{}", identity.text());
    let cookie = format!("pley_token={admin}");
    let sections = app.call(Method::GET, "/library/sections", None, None, &[("cookie", &cookie)]).await;
    assert_eq!(sections.status, StatusCode::UNAUTHORIZED);

    // pley's own API stays pley's, whatever a request carries.
    let health = app.call(Method::GET, "/api/health", None, None, &[("x-plex-token", &admin)]).await;
    assert_eq!(health.body["status"], "ok");
}

#[tokio::test]
async fn it_shares_pleys_port_with_the_jellyfin_api() {
    let app = TestApp::with_config(|config| config.jellyfin_on_web = true).await;
    let admin = app.setup_admin().await;
    let plex = app.plex_token(&admin).await;
    let jellyfin = app.jellyfin_token(&admin).await;
    let mediabrowser =
        format!(r#"MediaBrowser Client="Tool", Device="Box", DeviceId="1", Version="1", Token="{jellyfin}""#);

    // Where their paths meet, each client gets its own API.
    for path in ["/library/sections", "/system", "/devices"] {
        let reply =
            app.call(Method::GET, path, None, None, &[("x-plex-token", &plex), ("accept", "application/json")]).await;
        assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.text());
        assert!(reply.body.get("MediaContainer").is_some(), "{path}: {}", reply.text());
    }
    let hints =
        app.call(Method::GET, "/Search/Hints?SearchTerm=x", None, None, &[("authorization", &mediabrowser)]).await;
    assert_eq!(hints.status, StatusCode::OK, "{}", hints.text());
    assert!(hints.body["SearchHints"].is_array(), "{}", hints.text());
    // Without a header, a path only Jellyfin's apps ask for this way is its.
    let info = app.call(Method::GET, "/System/Info/Public", None, None, &[]).await;
    assert_eq!(info.body["ProductName"], "Jellyfin Server", "{}", info.text());
    // And Plex's own paths are Plex's.
    let identity = app.call(Method::GET, "/identity", None, None, &[]).await;
    assert!(identity.text().contains("machineIdentifier"), "{}", identity.text());
    // A token of either works on both.
    let sections = app.call(Method::GET, "/library/sections", None, None, &[("x-plex-token", &jellyfin)]).await;
    assert_eq!(sections.status, StatusCode::OK);
}

#[tokio::test]
async fn off_it_answers_nowhere_on_pleys_port() {
    let app = TestApp::with_config(|config| config.plex_on_web = false).await;
    let identity = app.call(Method::GET, "/identity", None, None, &[]).await;
    assert!(!identity.text().contains("machineIdentifier"), "{}", identity.text());
    let admin = app.setup_admin().await;
    let root = app.call(Method::GET, "/", None, None, &[("x-plex-token", &admin)]).await;
    assert!(!root.text().contains("MediaContainer"), "{}", root.text());
}
