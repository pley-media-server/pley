//! The language Pley is in: the server's (Settings → General), each
//! account's own, and the device's (docs/API.md, "Language").

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, titles};

async fn home(app: &TestApp, token: &str, accept_language: Option<&str>) -> Vec<String> {
    let extra: Vec<(&str, &str)> = accept_language.map(|v| ("accept-language", v)).into_iter().collect();
    let reply = app.call(Method::GET, "/api/hubs/home", Some(token), None, &extra).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    titles(&reply.body["hubs"])
}

#[tokio::test]
async fn home_is_in_the_accounts_then_the_servers_then_the_devices_language() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 1024);
    app.library(&admin, "Movies", "movies", "movies").await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;

    // Nothing chosen: the device's language, where Pley has it.
    assert_eq!(home(&app, &bob, None).await, ["Recently Added in Movies"]);
    assert_eq!(home(&app, &bob, Some("es-MX,es;q=0.9,en;q=0.5")).await, ["Añadido recientemente en Movies"]);
    assert_eq!(home(&app, &bob, Some("de-DE,de;q=0.9")).await, ["Recently Added in Movies"]);

    // The server's language comes before the device's.
    let saved = app.put("/api/settings", &admin, json!({"language": "fr"})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(home(&app, &bob, Some("es")).await, ["Ajouts récents dans Movies"]);
    let status = app.get_from("127.0.0.1:50000", "/api/server", &[]).await;
    assert_eq!(status.body["language"], json!("fr"), "shown before signing in");

    // And the account's own before both.
    let me = app.patch("/api/me", &bob, json!({"language": "es"})).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.text());
    assert_eq!(me.body["language"], json!("es"));
    assert_eq!(home(&app, &bob, Some("en")).await, ["Añadido recientemente en Movies"]);
    assert_eq!(app.get("/api/auth/me", &bob).await.body["language"], json!("es"));

    // Only Pley's languages; null goes back to the server's.
    for wrong in [json!("de"), json!("fr-FR"), json!(7)] {
        let reply = app.patch("/api/me", &bob, json!({"language": wrong})).await;
        assert!(reply.status.is_client_error(), "{wrong}: {}", reply.status);
    }
    assert_eq!(app.patch("/api/me", &bob, json!({"language": Value::Null})).await.body["language"], Value::Null);
    assert_eq!(home(&app, &bob, Some("es")).await, ["Ajouts récents dans Movies"]);

    // Each device's own again.
    app.put("/api/settings", &admin, json!({"language": ""})).await;
    assert_eq!(home(&app, &bob, Some("es")).await, ["Añadido recientemente en Movies"]);
    let status = app.get_from("127.0.0.1:50000", "/api/server", &[]).await;
    assert_eq!(status.body["language"], Value::Null);
}
