//! Tokens for other apps and tools (src/auth/compatibility.rs,
//! docs/COMPATIBILITY.md): made on the account page or by signing in with
//! `client`, they open the compatibility API only, within their scopes, and
//! end with the account's other sign-ins.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

use common::{Reply, TestApp, encode};
use pley::two_step::{base32_decode, code_at};
use pley::util::now;

const TOKENS: &str = "/api/me/compatibility-tokens";

/// A compatibility API request on pley's port with these headers, the token
/// sent as it is (a session's too).
async fn plex_with(app: &TestApp, method: Method, uri: &str, headers: &[(&str, &str)]) -> Reply {
    app.call(method, uri, None, None, headers).await
}

/// Status of `GET /library/sections` with `token` in `X-Plex-Token`.
async fn sections(app: &TestApp, token: &str) -> StatusCode {
    plex_with(app, Method::GET, "/library/sections", &[("x-plex-token", token)]).await.status
}

fn count(container: &Value) -> usize {
    container["Metadata"].as_array().map_or(0, Vec::len)
}

#[tokio::test]
async fn a_token_opens_the_compatibility_api_and_nothing_else() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let token = app.compatibility_token(&admin, "play").await;
    assert!(token.starts_with("pley_ct_") && token.len() == 8 + 43, "{token}");
    let (app_token, _) = app.app_token(1, "library play").await;

    // Wherever the compatibility API takes a token.
    let bearer = format!("Bearer {token}");
    for (uri, headers) in [
        ("/library/sections".to_owned(), vec![("x-plex-token", token.as_str())]),
        (format!("/library/sections?X-Plex-Token={token}"), vec![]),
        ("/library/sections".to_owned(), vec![("authorization", bearer.as_str())]),
        ("/library/sections".to_owned(), vec![("x-pley-token", token.as_str())]),
        (format!("/library/sections?token={token}"), vec![]),
    ] {
        let reply = plex_with(&app, Method::GET, &uri, &headers).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri} {headers:?}: {}", reply.text());
    }

    // Not a session's token, which says what to use instead, nor an app's.
    let session = plex_with(&app, Method::GET, "/library/sections", &[("x-plex-token", &admin)]).await;
    assert_eq!(session.status, StatusCode::UNAUTHORIZED);
    assert!(session.text().contains("tokens for other apps"), "{}", session.text());
    let session_bearer = format!("Bearer {admin}");
    assert_eq!(
        plex_with(&app, Method::GET, "/library/sections", &[("authorization", &session_bearer)]).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(sections(&app, &app_token).await, StatusCode::UNAUTHORIZED);

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
        assert_eq!(reply.body["message"], "tokens for other apps work on the compatibility API only");
    }
    // A keyed media address takes a playback's key, never a token.
    let keyed = app.call(Method::GET, &format!("/api/media/{token}/index.m3u8"), None, None, &[]).await;
    assert_eq!(keyed.status, StatusCode::NOT_FOUND);
    // The session and the app work as they did.
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &app_token).await.status, StatusCode::OK);

    // A tool signing out with it ends it, and nothing else.
    let out = app.call(Method::POST, "/api/auth/logout", Some(&token), None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK);
}

#[tokio::test]
async fn the_account_page_lists_makes_and_revokes_tokens() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    assert_eq!(app.get(TOKENS, &admin).await.body, json!([]));

    let made = app.post(TOKENS, &admin, json!({"name": " Tautulli ", "scope": "activity"})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let token = made.body["token"].as_str().unwrap().to_owned();
    let id = made.body["id"].as_i64().unwrap();
    assert_eq!((&made.body["name"], &made.body["scopes"]), (&json!("Tautulli"), &json!(["play", "activity"])));
    assert_eq!(made.body["last_used_at"], Value::Null, "not used yet");
    let expires = made.body["expires_at"].as_i64().unwrap() - made.body["created_at"].as_i64().unwrap();
    assert_eq!(expires, 90 * 24 * 3600);

    // Listed without the token itself.
    let listed = app.get(TOKENS, &admin).await;
    assert_eq!(listed.body.as_array().unwrap().len(), 1);
    assert_eq!(listed.body[0]["id"], id);
    assert!(listed.body[0].get("token").is_none() && !listed.mentions(&token[8..]));

    // Used: when, and from what.
    let used = plex_with(
        &app,
        Method::GET,
        "/library/sections",
        &[("x-plex-token", &token), (header::USER_AGENT.as_str(), "Tautulli/2.15")],
    )
    .await;
    assert_eq!(used.status, StatusCode::OK);
    let listed = app.get(TOKENS, &admin).await;
    assert!(listed.body[0]["last_used_at"].as_i64().is_some_and(|at| at >= now() - 5));
    assert_eq!(listed.body[0]["device"], "Tautulli/2.15");

    // Without a scope, `play`; names and scopes are checked.
    let plezy = app.post(TOKENS, &admin, json!({"name": "Plezy"})).await;
    assert_eq!((plezy.status, &plezy.body["scopes"]), (StatusCode::CREATED, &json!(["play"])));
    for bad in [
        json!({"name": ""}),
        json!({"name": "x".repeat(81)}),
        json!({"name": "Kometa", "scope": "everything"}),
        json!({"name": "Kometa", "scope": "library"}),
    ] {
        assert_eq!(app.post(TOKENS, &admin, bad.clone()).await.status, StatusCode::BAD_REQUEST, "{bad}");
    }

    // Each sees and revokes only their own.
    assert_eq!(app.get(TOKENS, &bob).await.body, json!([]));
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &bob).await.status, StatusCode::NOT_FOUND);
    assert_eq!(sections(&app, &token).await, StatusCode::OK);
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED, "revoked at once");
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
    let left = app.get(TOKENS, &admin).await.body;
    assert_eq!(left.as_array().unwrap().len(), 1);
    assert_eq!(left[0]["name"], "Plezy");
}

/// A server where bob plays a film on a Plex client, and the administrator
/// and bob each have a play in their history.
struct World {
    app: TestApp,
    admin: String,
    bob: String,
    bob_id: i64,
    movies: i64,
    film: i64,
    /// bob's play in the history.
    bobs_play: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let bobs_tool = app.compatibility_token(&bob, "play").await;
    let timeline = format!("/:/timeline?ratingKey={film}&time=10000&state=playing&duration=600000");
    let playing = app.plex(Method::GET, &timeline, &bobs_tool, &[("x-plex-client-identifier", "bobs-tv")]).await;
    assert_eq!(playing.status, StatusCode::OK, "{}", playing.text());
    for user in [1, bob_id] {
        sqlx::query("INSERT INTO play_history (user_id, item_id, viewed_at) VALUES (?, ?, 1)")
            .bind(user)
            .bind(film)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    let bobs_play: i64 = sqlx::query_scalar("SELECT id FROM play_history WHERE user_id = ?")
        .bind(bob_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    World { app, admin, bob, bob_id, movies, film, bobs_play }
}

impl World {
    /// The id of bob's playback, as `/status/sessions` lists it to `token`.
    async fn bobs_session(&self, token: &str) -> String {
        let sessions = self.app.plex_json("/status/sessions", token).await;
        sessions["Metadata"][0]["Session"]["id"].as_str().unwrap().to_owned()
    }

    /// What administrators change, each with the status `token` gets.
    async fn changes(&self, token: &str, session: &str) -> Vec<(String, StatusCode)> {
        let (movies, film) = (self.movies, self.film);
        let collection = encode(&format!("server://x/com.plexapp.plugins.library/library/metadata/{film}"));
        let mut statuses = Vec::new();
        for (method, uri) in [
            (Method::GET, format!("/library/sections/{movies}/refresh")),
            (Method::GET, "/library/sections/all/refresh".to_owned()),
            (Method::PUT, format!("/library/metadata/{film}/refresh")),
            (Method::PUT, format!("/library/metadata/{film}?title.value=Changed&title.locked=1")),
            (
                Method::POST,
                format!("/library/collections?type=1&title=Mine&sectionId={movies}&smart=0&uri={collection}"),
            ),
            (Method::GET, format!("/status/sessions/terminate?sessionId={session}")),
        ] {
            let reply = self.app.plex(method, &uri, token, &[]).await;
            statuses.push((uri, reply.status));
        }
        statuses
    }

    /// Status of deleting bob's play from the history with `token`.
    async fn delete_bobs_play(&self, token: &str) -> StatusCode {
        let uri = format!("/status/sessions/history/{}", self.bobs_play);
        self.app.plex(Method::DELETE, &uri, token, &[]).await.status
    }
}

#[tokio::test]
async fn each_scope_sees_and_changes_what_it_allows() {
    let w = world().await;
    let app = &w.app;
    let play = app.compatibility_token(&w.admin, "play").await;
    let activity = app.compatibility_token(&w.admin, "activity").await;
    let manage = app.compatibility_token(&w.admin, "manage").await;
    let session = w.bobs_session(&activity).await;
    let folder = app.media_path("movies");

    // `play`: its own, even on an administrator's account.
    assert_eq!(app.plex_json("/status/sessions", &play).await["size"], json!(0), "not bob's playback");
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &play).await), 1);
    assert_eq!(count(&app.plex_json(&format!("/status/sessions/history/all?accountID={}", w.bob_id), &play).await), 1);
    assert_eq!(app.plex_json("/accounts", &play).await["size"], json!(1));
    let sections = app.plex_json("/library/sections", &play).await;
    assert_eq!(sections["Directory"][0]["Location"][0]["path"], "movies", "a folder's name, not where it is");
    for (uri, status) in w.changes(&play, &session).await {
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    assert_eq!(w.delete_bobs_play(&play).await, StatusCode::NOT_FOUND);
    let stop = format!("/video/:/transcode/universal/stop?session={session}");
    assert_eq!(app.plex(Method::GET, &stop, &play, &[]).await.status, StatusCode::OK);
    assert_eq!(w.bobs_session(&activity).await, session, "bob's playback goes on");

    // `activity`: everyone's, as Tautulli shows them, and no changes.
    assert_eq!(app.plex_json("/status/sessions", &activity).await["size"], json!(1));
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &activity).await), 2);
    assert_eq!(app.plex_json("/accounts", &activity).await["size"], json!(2));
    let sections = app.plex_json("/library/sections", &activity).await;
    assert_eq!(sections["Directory"][0]["Location"][0]["path"], json!(folder));
    for (uri, status) in w.changes(&activity, &session).await {
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    assert_eq!(w.delete_bobs_play(&activity).await, StatusCode::NOT_FOUND);
    assert_eq!(app.plex(Method::GET, &stop, &activity, &[]).await.status, StatusCode::OK);
    assert_eq!(w.bobs_session(&activity).await, session, "bob's playback goes on");

    // `manage`: changes, as Kometa makes them, without seeing everyone's.
    assert_eq!(app.plex_json("/status/sessions", &manage).await["size"], json!(0));
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &manage).await), 1);
    for (uri, status) in w.changes(&manage, &session).await {
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
    assert_eq!(app.plex_json("/status/sessions", &activity).await["size"], json!(0), "bob's playback stopped");
    let edited = app.get(&format!("/api/items/{}", w.film), &w.admin).await;
    assert_eq!(edited.body["title"], "Changed");
    let collections = app.get(&format!("/api/libraries/{}/collections", w.movies), &w.admin).await;
    assert_eq!(common::titles(&collections.body["collections"]), ["Mine"]);
    assert_eq!(w.delete_bobs_play(&manage).await, StatusCode::OK);
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &activity).await), 1);
}

#[tokio::test]
async fn administrators_scopes_are_an_administrators_and_go_with_the_role() {
    let w = world().await;
    let app = &w.app;

    // bob isn't an administrator: neither scope, from the account page or
    // signing in.
    for scope in ["activity", "manage", "play activity manage"] {
        let refused = app.post(TOKENS, &w.bob, json!({"name": "Tautulli", "scope": scope})).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{scope}");
        let body = json!({"username": "bob", "password": "test-pass-1", "client": "Tautulli", "scope": scope});
        let refused = app.call(Method::POST, "/api/auth/login", None, Some(body), &[]).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{scope}");
        assert!(refused.body.get("token").is_none());
    }
    assert!(app.get(TOKENS, &w.bob).await.body.as_array().unwrap().iter().all(|t| t["scopes"] == json!(["play"])));

    // An administrator now: his token sees everyone's and changes things.
    let promoted = app.patch(&format!("/api/users/{}", w.bob_id), &w.admin, json!({"is_admin": true})).await;
    assert_eq!(promoted.status, StatusCode::OK, "{:?}", promoted.body);
    let tautulli = app.compatibility_token(&w.bob, "activity manage").await;
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &tautulli).await), 2);
    assert_eq!(app.plex_json("/accounts", &tautulli).await["size"], json!(2));
    let refresh = format!("/library/sections/{}/refresh", w.movies);
    assert_eq!(app.plex(Method::GET, &refresh, &tautulli, &[]).await.status, StatusCode::OK);

    // Not any more: at once, though the token was made with them.
    let demoted = app.patch(&format!("/api/users/{}", w.bob_id), &w.admin, json!({"is_admin": false})).await;
    assert_eq!(demoted.status, StatusCode::OK, "{:?}", demoted.body);
    assert_eq!(app.plex(Method::GET, &refresh, &tautulli, &[]).await.status, StatusCode::FORBIDDEN);
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &tautulli).await), 1, "his own");
    assert_eq!(app.plex_json("/accounts", &tautulli).await["size"], json!(1));
    let sections = app.plex_json("/library/sections", &tautulli).await;
    assert_eq!(sections["Directory"][0]["Location"][0]["path"], "movies");
    assert_eq!(w.delete_bobs_play(&tautulli).await, StatusCode::OK, "his own play is his to delete");
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
    assert_eq!(sections(&app, &token).await, StatusCode::OK);
    let changed = app
        .put("/api/auth/password", &bob, json!({"current_password": "test-pass-1", "new_password": "another-pass-2"}))
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{:?}", changed.body);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get(TOKENS, &bob).await.body, json!([]));

    // A new password an administrator gave him.
    let token = app.compatibility_token(&bob, "play").await;
    let set = app.patch(&user, &admin, json!({"password": "third-pass-3"})).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.body);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);

    // Signed out everywhere.
    let bob = app.login("bob", "third-pass-3").await;
    let token = app.compatibility_token(&bob, "play").await;
    assert_eq!(app.delete(&format!("{user}/devices"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);

    // Disabled: refused while it is, as his sessions are.
    let bob = app.login("bob", "third-pass-3").await;
    let token = app.compatibility_token(&bob, "play").await;
    assert_eq!(app.patch(&user, &admin, json!({"disabled": true})).await.status, StatusCode::OK);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.patch(&user, &admin, json!({"disabled": false})).await.status, StatusCode::OK);
    assert_eq!(sections(&app, &token).await, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::OK);

    // Deleted with the account.
    assert_eq!(app.delete(&user, &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM compatibility_tokens WHERE user_id = ?")
        .bind(bob_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(left, 0);

    // The administrator's own went on through all of it.
    assert_eq!(sections(&app, &admins).await, StatusCode::OK);
}

/// `POST /api/auth/login` with `body`, and `extra` headers.
async fn sign_in(app: &TestApp, body: Value, extra: &[(&str, &str)]) -> Reply {
    app.call(Method::POST, "/api/auth/login", None, Some(body), extra).await
}

async fn sessions_of(app: &TestApp, user_id: i64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_tool_signs_in_for_a_token_and_no_session() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let before = sessions_of(&app, 1).await;
    let kometa = json!({"username": "admin", "password": "secret-one", "client": "Kometa", "scope": "manage"});

    let signed_in = sign_in(&app, kometa.clone(), &[]).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let token = signed_in.body["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("pley_ct_"), "{token}");
    assert_eq!(signed_in.body["user"]["username"], "admin");
    assert!(!signed_in.headers.contains_key(header::SET_COOKIE), "no cookie");
    assert_eq!(sessions_of(&app, 1).await, before, "no session");
    assert_eq!(app.plex(Method::GET, "/library/sections/all/refresh", &token, &[]).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &token).await.status, StatusCode::UNAUTHORIZED);
    let listed = app.get(TOKENS, &admin).await.body;
    assert_eq!((&listed[0]["name"], &listed[0]["scopes"]), (&json!("Kometa"), &json!(["play", "manage"])));

    // From a browser too: the token in the body, never a cookie.
    let browser = sign_in(&app, kometa.clone(), &[("origin", common::ISSUER)]).await;
    assert!(browser.body["token"].as_str().unwrap().starts_with("pley_ct_"));
    assert!(!browser.headers.contains_key(header::SET_COOKIE));
    // Without a scope, `play`.
    let plain = json!({"username": "admin", "password": "secret-one", "client": "Plezy"});
    let plain = sign_in(&app, plain, &[]).await.body["token"].as_str().unwrap().to_owned();
    assert_eq!(app.plex(Method::GET, "/library/sections/all/refresh", &plain, &[]).await.status, StatusCode::FORBIDDEN);
    assert_eq!(sessions_of(&app, 1).await, before);

    // What is asked for is checked, and every sign-in check applies.
    for bad in [
        json!({"username": "admin", "password": "secret-one", "client": ""}),
        json!({"username": "admin", "password": "secret-one", "client": "Kometa", "scope": "everything"}),
    ] {
        assert_eq!(sign_in(&app, bad.clone(), &[]).await.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let wrong = json!({"username": "admin", "password": "wrong-one", "client": "Kometa"});
    assert_eq!(sign_in(&app, wrong, &[]).await.status, StatusCode::UNAUTHORIZED);
    let (_, carol_id) = app.user(&admin, "carol", json!({})).await;
    app.patch(&format!("/api/users/{carol_id}"), &admin, json!({"disabled": true})).await;
    let disabled = json!({"username": "carol", "password": "test-pass-1", "client": "Infuse"});
    let disabled = sign_in(&app, disabled, &[]).await;
    assert_eq!(disabled.status, StatusCode::FORBIDDEN);
    assert!(disabled.body.get("token").is_none());

    // With two-step sign-in, its code.
    let (secret, _) = app.turn_on_two_step(&admin, "secret-one").await;
    let without = sign_in(&app, kometa.clone(), &[]).await;
    assert_eq!((without.status, &without.body["code"]), (StatusCode::UNAUTHORIZED, &json!("two_step")));
    let mut with_code = kometa.clone();
    with_code["code"] = json!(code_at(&secret, now() + 30));
    let signed_in = sign_in(&app, with_code, &[]).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let token = signed_in.body["token"].as_str().unwrap();
    assert!(token.starts_with("pley_ct_") && !signed_in.headers.contains_key(header::SET_COOKIE));
    assert_eq!(sections(&app, token).await, StatusCode::OK);
    assert_eq!(sessions_of(&app, 1).await, before);

    // Wrong passwords count as any sign-in's: the right one waits too.
    app.user(&admin, "dave", json!({})).await;
    let dave = json!({"username": "dave", "password": "wrong-pass", "client": "Script"});
    for _ in 0..5 {
        assert_eq!(sign_in(&app, dave.clone(), &[]).await.status, StatusCode::UNAUTHORIZED);
    }
    let right = json!({"username": "dave", "password": "test-pass-1", "client": "Script"});
    assert_eq!(sign_in(&app, right, &[]).await.status, StatusCode::TOO_MANY_REQUESTS);

    // Required of everyone: an account sets it up as it signs in for a
    // token, and gets its recovery codes with it.
    let (_, bob_id) = app.user(&admin, "bob", json!({})).await;
    assert_eq!(app.put("/api/settings", &admin, json!({"require_two_step": true})).await.status, StatusCode::OK);
    let tautulli = json!({"username": "bob", "password": "test-pass-1", "client": "Tautulli"});
    let asked = sign_in(&app, tautulli.clone(), &[]).await;
    assert_eq!((asked.status, &asked.body["code"]), (StatusCode::UNAUTHORIZED, &json!("two_step_setup")));
    let secret = base32_decode(asked.body["setup"]["secret"].as_str().unwrap()).unwrap();
    let mut set_up = tautulli.clone();
    set_up["setup_id"] = asked.body["setup"]["id"].clone();
    set_up["code"] = json!(code_at(&secret, now()));
    let done = sign_in(&app, set_up, &[]).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
    assert_eq!(done.body["recovery_codes"].as_array().unwrap().len(), 10);
    assert!(done.body["token"].as_str().unwrap().starts_with("pley_ct_"));
    assert!(!done.headers.contains_key(header::SET_COOKIE));
    assert_eq!(sessions_of(&app, bob_id).await, 0, "signed out when it became required, and no session since");
}
