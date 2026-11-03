//! Tokens for other apps (src/auth/compatibility.rs, docs/COMPATIBILITY.md):
//! made on the account page or by signing in to the Jellyfin-compatible API,
//! they open that API only, for their own account, and end with the
//! account's other sign-ins.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

use common::{Reply, TestApp};
use pley::util::now;

const TOKENS: &str = "/api/me/compatibility-tokens";

/// Jellyfin's `GET /UserViews` with these headers, the token sent as it is
/// (a session's too).
async fn views_with(app: &TestApp, uri: &str, headers: &[(&str, &str)]) -> Reply {
    app.jf_raw(Method::GET, uri, headers, None).await
}

/// Status of `GET /UserViews` with `token` in `X-Emby-Token`.
async fn views(app: &TestApp, token: &str) -> StatusCode {
    views_with(app, "/UserViews", &[("x-emby-token", token)]).await.status
}

#[tokio::test]
async fn a_token_opens_the_compatibility_api_and_nothing_else() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let token = app.compatibility_token(&admin, "play").await;
    assert!(token.starts_with("pley_ct_") && token.len() == 8 + 43, "{token}");
    let (app_token, _) = app.app_token(1, "library play").await;

    // Wherever Jellyfin's apps send a token.
    let authorization =
        format!(r#"MediaBrowser Client="Tool", Device="Box", DeviceId="1", Version="1", Token="{token}""#);
    for (uri, headers) in [
        ("/UserViews".to_owned(), vec![("x-emby-token", token.as_str())]),
        ("/UserViews".to_owned(), vec![("x-mediabrowser-token", token.as_str())]),
        ("/UserViews".to_owned(), vec![("authorization", authorization.as_str())]),
        ("/UserViews".to_owned(), vec![("x-emby-authorization", authorization.as_str())]),
        (format!("/UserViews?api_key={token}"), vec![]),
        (format!("/UserViews?ApiKey={token}"), vec![]),
    ] {
        let reply = views_with(&app, &uri, &headers).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri} {headers:?}: {}", reply.text());
    }

    // Not a session's token, nor an app's.
    assert_eq!(views(&app, &admin).await, StatusCode::UNAUTHORIZED);
    let session_bearer = format!("Bearer {admin}");
    assert_eq!(
        views_with(&app, "/UserViews", &[("authorization", &session_bearer)]).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(views(&app, &app_token).await, StatusCode::UNAUTHORIZED);

    // Pley's own API refuses it, wherever it is sent.
    let cookie = format!("pley_token={token}");
    let refused = [
        app.get("/api/auth/me", &token).await,
        app.get("/api/libraries", &token).await,
        app.call(Method::GET, "/api/auth/me", None, None, &[("x-pley-token", &token)]).await,
        app.call(Method::GET, &format!("/api/libraries?token={token}"), None, None, &[]).await,
        app.call(Method::GET, "/api/auth/me", None, None, &[("cookie", &cookie)]).await,
        app.get(TOKENS, &token).await,
        app.post(TOKENS, &token, json!({"name": "Another"})).await,
    ];
    for reply in refused {
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{:?}", reply.body);
        assert_eq!(reply.body["message"], "tokens for other apps work on the Jellyfin-compatible API only");
    }
    // A keyed media address takes a playback's key, never a token.
    let keyed = app.call(Method::GET, &format!("/api/media/{token}/index.m3u8"), None, None, &[]).await;
    assert_eq!(keyed.status, StatusCode::NOT_FOUND);
    // The session and the app work as they did.
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &app_token).await.status, StatusCode::OK);

    // Signing out with it ends it, and nothing else: Pley's sign-out, or
    // Jellyfin's.
    let out = app.call(Method::POST, "/api/auth/logout", Some(&token), None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED);
    let another = app.compatibility_token(&admin, "play").await;
    let out = app.jf_raw(Method::POST, "/Sessions/Logout", &[("x-emby-token", &another)], None).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(views(&app, &another).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK);
}

#[tokio::test]
async fn the_account_page_lists_makes_and_revokes_tokens() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    assert_eq!(app.get(TOKENS, &admin).await.body, json!([]));

    let made = app.post(TOKENS, &admin, json!({"name": " Infuse "})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let token = made.body["token"].as_str().unwrap().to_owned();
    let id = made.body["id"].as_i64().unwrap();
    assert_eq!((&made.body["name"], &made.body["scopes"]), (&json!("Infuse"), &json!(["play"])));
    assert_eq!(made.body["last_used_at"], Value::Null, "not used yet");
    let expires = made.body["expires_at"].as_i64().unwrap() - made.body["created_at"].as_i64().unwrap();
    assert_eq!(expires, 90 * 24 * 3600);

    // Listed without the token itself.
    let listed = app.get(TOKENS, &admin).await;
    assert_eq!(listed.body.as_array().unwrap().len(), 1);
    assert_eq!(listed.body[0]["id"], id);
    assert!(listed.body[0].get("token").is_none() && !listed.mentions(&token[8..]));

    // Used: when, and from what.
    let used =
        views_with(&app, "/UserViews", &[("x-emby-token", &token), (header::USER_AGENT.as_str(), "Infuse/8.1")]).await;
    assert_eq!(used.status, StatusCode::OK);
    let listed = app.get(TOKENS, &admin).await;
    assert!(listed.body[0]["last_used_at"].as_i64().is_some_and(|at| at >= now() - 5));
    assert_eq!(listed.body[0]["device"], "Infuse/8.1");

    // `play` may be named; names are checked, and no other scope is.
    let named = app.post(TOKENS, &admin, json!({"name": "Script", "scope": "play"})).await;
    assert_eq!((named.status, &named.body["scopes"]), (StatusCode::CREATED, &json!(["play"])));
    for bad in [
        json!({"name": ""}),
        json!({"name": "x".repeat(81)}),
        json!({"name": "Tool", "scope": "everything"}),
        json!({"name": "Tool", "scope": "activity"}),
        json!({"name": "Tool", "scope": "manage"}),
        json!({"name": "Tool", "scope": "library"}),
    ] {
        assert_eq!(app.post(TOKENS, &admin, bad.clone()).await.status, StatusCode::BAD_REQUEST, "{bad}");
    }

    // Each sees and revokes only their own.
    assert_eq!(app.get(TOKENS, &bob).await.body, json!([]));
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &bob).await.status, StatusCode::NOT_FOUND);
    assert_eq!(views(&app, &token).await, StatusCode::OK);
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED, "revoked at once");
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
    let left = app.get(TOKENS, &admin).await.body;
    assert_eq!(left.as_array().unwrap().len(), 1);
    assert_eq!(left[0]["name"], "Script");
}

#[tokio::test]
async fn a_token_sees_its_own_account_even_an_administrators() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.jf_item(app.ids(movies, &admin).await["Film"]);
    sqlx::query("UPDATE items SET duration_ms = 600000").execute(&app.state.db.write).await.unwrap();
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let (admins, bobs) = (app.jf_user(&admin).await, app.jf_user(&bob).await);

    // bob plays the film.
    let report = json!({"ItemId": film, "PositionTicks": 3_000_000_000_i64});
    assert_eq!(app.jf(Method::POST, "/Sessions/Playing", &bob, Some(report)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.state.sessions.list().len(), 1);

    // The administrator's token sees the administrator alone.
    let users = app.jf_json("/Users", &admin).await;
    let names: Vec<&str> = users.as_array().unwrap().iter().map(|u| u["Name"].as_str().unwrap()).collect();
    assert_eq!(names, ["admin"]);
    assert_eq!(app.jf_json("/Users/Public", "").await, json!([]), "no one is listed before signing in");
    assert_eq!(app.jf_json("/Sessions", &admin).await, json!([]), "not bob's playback");
    assert_eq!(app.jf_json("/Users/Me", &admin).await["Id"], json!(admins));
    for uri in [
        format!("/Users/{bobs}"),
        format!("/Users/{bobs}/Items/{film}"),
        format!("/Users/{bobs}/Views"),
        format!("/Users/{bobs}/Items/Resume"),
    ] {
        assert_eq!(app.jf(Method::GET, &uri, &admin, None).await.status, StatusCode::NOT_FOUND, "{uri}");
    }
    assert_eq!(
        app.jf(Method::POST, &format!("/Users/{bobs}/PlayedItems/{film}"), &admin, None).await.status,
        StatusCode::NOT_FOUND
    );
    let bobs_progress = app.jf_json(&format!("/Items/{film}"), &bob).await["UserData"]["PlaybackPositionTicks"].clone();
    assert_eq!(bobs_progress, json!(3_000_000_000_i64));
    let admins_view = app.jf_json(&format!("/Items/{film}"), &admin).await;
    assert_eq!(admins_view["UserData"]["PlaybackPositionTicks"], json!(0), "the administrator's own");
    // Nothing on it administers.
    let policy = &app.jf_json("/Users/Me", &admin).await["Policy"];
    assert_eq!((&policy["IsAdministrator"], &policy["EnableContentDeletion"]), (&json!(false), &json!(false)));
}

#[tokio::test]
async fn a_new_password_signing_out_everywhere_and_disabling_end_them() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let admins = app.compatibility_token(&admin, "play").await;
    let user = format!("/api/users/{bob_id}");

    // His own new password: his tokens end, the session that set it stays.
    let token = app.compatibility_token(&bob, "play").await;
    assert_eq!(views(&app, &token).await, StatusCode::OK);
    let changed = app
        .put("/api/auth/password", &bob, json!({"current_password": "test-pass-1", "new_password": "another-pass-2"}))
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{:?}", changed.body);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get(TOKENS, &bob).await.body, json!([]));

    // A new password an administrator gave him.
    let token = app.compatibility_token(&bob, "play").await;
    let set = app.patch(&user, &admin, json!({"password": "third-pass-3"})).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.body);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED);

    // Signed out everywhere.
    let bob = app.login("bob", "third-pass-3").await;
    let token = app.compatibility_token(&bob, "play").await;
    assert_eq!(app.delete(&format!("{user}/devices"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED);

    // Disabled: refused while it is, as his sessions are.
    let bob = app.login("bob", "third-pass-3").await;
    let token = app.compatibility_token(&bob, "play").await;
    assert_eq!(app.patch(&user, &admin, json!({"disabled": true})).await.status, StatusCode::OK);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.patch(&user, &admin, json!({"disabled": false})).await.status, StatusCode::OK);
    assert_eq!(views(&app, &token).await, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::OK);

    // Deleted with the account.
    assert_eq!(app.delete(&user, &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM compatibility_tokens WHERE user_id = ?")
        .bind(bob_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(left, 0);

    // The administrator's own went on through all of it.
    assert_eq!(views(&app, &admins).await, StatusCode::OK);
}

#[tokio::test]
async fn pleys_sign_in_makes_sessions_only() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    // Tools sign in through Jellyfin's sign-in for a token; Pley's own makes
    // a session, whatever else is asked.
    let body = json!({"username": "admin", "password": "secret-one", "client": "Kometa", "scope": "manage"});
    let signed_in = app.call(Method::POST, "/api/auth/login", None, Some(body), &[]).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let token = signed_in.body["token"].as_str().unwrap();
    assert!(!token.starts_with("pley_ct_"), "{token}");
    assert_eq!(app.get("/api/auth/me", token).await.status, StatusCode::OK);
    assert_eq!(views(&app, token).await, StatusCode::UNAUTHORIZED);
}
