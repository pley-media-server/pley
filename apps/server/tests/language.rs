//! The language Pley is in: each account's own, the device's, and the
//! server language (Settings → General) (docs/API.md, "Language").

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
async fn home_is_in_the_accounts_then_the_devices_then_the_servers_language() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alien (1979).mkv", 1024);
    app.library(&admin, "Movies", "movies", "movies").await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;

    // Nothing chosen: the device's language where Pley has it, else the
    // server language, English until an administrator picks one.
    assert_eq!(home(&app, &bob, Some("es-MX,es;q=0.9,en;q=0.5")).await, ["Añadido recientemente en Movies"]);
    assert_eq!(home(&app, &bob, Some("de-DE,de;q=0.9")).await, ["Recently Added in Movies"]);
    assert_eq!(home(&app, &bob, None).await, ["Recently Added in Movies"]);
    let status = app.get_from("127.0.0.1:50000", "/api/server", &[]).await;
    assert_eq!(status.body["language"], json!("en"), "English until chosen");

    let saved = app.put("/api/settings", &admin, json!({"language": "fr"})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(home(&app, &bob, Some("de-DE")).await, ["Ajouts récents dans Movies"]);
    assert_eq!(home(&app, &bob, None).await, ["Ajouts récents dans Movies"]);
    assert_eq!(home(&app, &bob, Some("es")).await, ["Añadido recientemente en Movies"], "the device's first");
    let status = app.get_from("127.0.0.1:50000", "/api/server", &[]).await;
    assert_eq!(status.body["language"], json!("fr"), "shown before signing in");

    // The account's own before both.
    let me = app.patch("/api/me", &bob, json!({"language": "es"})).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.text());
    assert_eq!(me.body["language"], json!("es"));
    assert_eq!(home(&app, &bob, Some("en")).await, ["Añadido recientemente en Movies"]);
    assert_eq!(app.get("/api/auth/me", &bob).await.body["language"], json!("es"));

    // Only Pley's languages; null goes back to automatic.
    for wrong in [json!("de"), json!("fr-FR"), json!(7)] {
        let reply = app.patch("/api/me", &bob, json!({"language": wrong})).await;
        assert!(reply.status.is_client_error(), "{wrong}: {}", reply.status);
    }
    assert_eq!(app.patch("/api/me", &bob, json!({"language": Value::Null})).await.body["language"], Value::Null);
    assert_eq!(home(&app, &bob, Some("de")).await, ["Ajouts récents dans Movies"]);

    // An app sets it as the web does, with the play scope.
    let (bob_app, _) = app.app_token(bob_id, "library play").await;
    let chosen = app.put("/api/me/language", &bob_app, json!({"language": "es"})).await;
    assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.text());
    assert_eq!(chosen.body["language"], json!("es"));
    assert_eq!(app.put("/api/me/language", &bob_app, json!({"language": "de"})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.put("/api/me/language", &bob_app, json!({})).await.body["language"], Value::Null);
    let (library_only, _) = app.app_token(bob_id, "library").await;
    assert_eq!(
        app.put("/api/me/language", &library_only, json!({"language": "es"})).await.status,
        StatusCode::FORBIDDEN
    );

    // Not chosen again: English.
    app.put("/api/settings", &admin, json!({"language": ""})).await;
    assert_eq!(home(&app, &bob, Some("de")).await, ["Recently Added in Movies"]);
    let status = app.get_from("127.0.0.1:50000", "/api/server", &[]).await;
    assert_eq!(status.body["language"], json!("en"));
}
