//! Tokens for other apps (src/auth/compatibility.rs, docs/COMPATIBILITY.md):
//! made on the account page or by signing in to the Jellyfin-compatible API,
//! they open the compatibility APIs only, for their own account, do what
//! their scopes allow from where their origins allow, never administer
//! accounts, and end with the account's other sign-ins.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use common::{Reply, TestApp, encode};
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

/// The Plex-compatible API on Pley's port with these headers alone.
async fn plex_with(app: &TestApp, method: Method, uri: &str, headers: &[(&str, &str)]) -> Reply {
    app.call(method, uri, None, None, headers).await
}

/// Status of Plex's `GET /library/sections` with `token` in `X-Plex-Token`.
async fn sections(app: &TestApp, token: &str) -> StatusCode {
    plex_with(app, Method::GET, "/library/sections", &[("x-plex-token", token)]).await.status
}

fn count(container: &Value) -> usize {
    container["Metadata"].as_array().map_or(0, Vec::len)
}

#[tokio::test]
async fn a_token_opens_the_compatibility_apis_and_nothing_else() {
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
    // And wherever Plex's tools do.
    let bearer = format!("Bearer {token}");
    for (uri, headers) in [
        ("/library/sections".to_owned(), vec![("x-plex-token", token.as_str())]),
        (format!("/library/sections?X-Plex-Token={token}"), vec![]),
        ("/library/sections".to_owned(), vec![("x-plex-client-identifier", "c"), ("authorization", bearer.as_str())]),
    ] {
        let reply = plex_with(&app, Method::GET, &uri, &headers).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri} {headers:?}: {}", reply.text());
    }

    // Not a session's token, which the Plex API says, nor an app's.
    assert_eq!(views(&app, &admin).await, StatusCode::UNAUTHORIZED);
    let session_bearer = format!("Bearer {admin}");
    assert_eq!(
        views_with(&app, "/UserViews", &[("authorization", &session_bearer)]).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(views(&app, &app_token).await, StatusCode::UNAUTHORIZED);
    let session = plex_with(&app, Method::GET, "/library/sections", &[("x-plex-token", &admin)]).await;
    assert_eq!(session.status, StatusCode::UNAUTHORIZED);
    assert!(session.text().contains("tokens for other apps"), "{}", session.text());
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
        assert_eq!(reply.body["message"], "tokens for other apps work on the Plex- and Jellyfin-compatible APIs only");
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
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
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

    let made = app.post(TOKENS, &admin, json!({"name": " Monitor ", "scope": "activity"})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let token = made.body["token"].as_str().unwrap().to_owned();
    let id = made.body["id"].as_i64().unwrap();
    assert_eq!((&made.body["name"], &made.body["scopes"]), (&json!("Monitor"), &json!(["play", "activity"])));
    assert_eq!(made.body["allowed_from"], json!([]), "anywhere");
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

    // Without a scope, `play`; limited to where it is used from; names,
    // scopes and origins are checked, and no scope administers accounts.
    let plain = app.post(TOKENS, &admin, json!({"name": "Player"})).await;
    assert_eq!((plain.status, &plain.body["scopes"]), (StatusCode::CREATED, &json!(["play"])));
    let limited = app
        .post(
            TOKENS,
            &admin,
            json!({"name": "Library tool", "scope": "manage", "allowed_from": ["10.0.0.7/24", "local"]}),
        )
        .await;
    assert_eq!(limited.status, StatusCode::CREATED, "{:?}", limited.body);
    assert_eq!(limited.body["scopes"], json!(["play", "manage"]));
    assert_eq!(limited.body["allowed_from"], json!(["10.0.0.0/24", "local"]));
    let many: Vec<String> = (0..17).map(|i| format!("10.0.0.{i}")).collect();
    for bad in [
        json!({"name": ""}),
        json!({"name": "x".repeat(81)}),
        json!({"name": "Tool", "scope": "everything"}),
        json!({"name": "Tool", "scope": "users"}),
        json!({"name": "Tool", "scope": "library"}),
        json!({"name": "Tool", "allowed_from": ["somewhere"]}),
        json!({"name": "Tool", "allowed_from": ["10.0.0.0/33"]}),
        json!({"name": "Tool", "allowed_from": many}),
    ] {
        assert_eq!(app.post(TOKENS, &admin, bad.clone()).await.status, StatusCode::BAD_REQUEST, "{bad}");
    }

    // Each sees and revokes only their own.
    assert_eq!(app.get(TOKENS, &bob).await.body, json!([]));
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &bob).await.status, StatusCode::NOT_FOUND);
    assert_eq!(views(&app, &token).await, StatusCode::OK);
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(views(&app, &token).await, StatusCode::UNAUTHORIZED, "revoked at once");
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.delete(&format!("{TOKENS}/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
    let left = app.get(TOKENS, &admin).await.body;
    let names: Vec<&str> = left.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"Player") && names.contains(&"Library tool"), "{names:?}");
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

    // `activity`: everyone's, as activity monitors show them, and no changes.
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

    // `manage`: changes, as library tools make them, without seeing everyone's.
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

    // A collection emptied is still listed to the token that may fill it.
    let listed = format!("/library/sections/{}/collections", w.movies);
    let mine = app.plex_json(&listed, &manage).await["Metadata"][0]["ratingKey"].clone();
    let mine = mine.as_str().map(str::to_owned).unwrap_or_else(|| mine.to_string());
    let emptied =
        app.plex(Method::DELETE, &format!("/library/collections/{mine}/items/{}", w.film), &manage, &[]).await;
    assert_eq!(emptied.status, StatusCode::OK, "{}", emptied.text());
    assert_eq!(count(&app.plex_json(&listed, &manage).await), 1);
    assert_eq!(count(&app.plex_json(&listed, &activity).await), 0, "seeing isn't managing");

    // On the Jellyfin-compatible API, `activity` says it is an administrator,
    // as activity monitors need, and administers nothing (tests/jellyfin.rs).
    let policy = &app.jf_raw(Method::GET, "/Users/Me", &[("x-emby-token", &activity)], None).await.body["Policy"];
    assert_eq!(policy["IsAdministrator"], json!(true));
    let users = app.jf_raw(Method::GET, "/Users", &[("x-emby-token", &activity)], None).await.body;
    assert_eq!(users.as_array().map(Vec::len), Some(1), "the caller alone");
}

#[tokio::test]
async fn administrators_scopes_are_an_administrators_and_go_with_the_role() {
    let w = world().await;
    let app = &w.app;

    // bob isn't an administrator: neither scope.
    for scope in ["activity", "manage", "play activity manage"] {
        let refused = app.post(TOKENS, &w.bob, json!({"name": "Monitor", "scope": scope})).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{scope}");
    }
    assert!(app.get(TOKENS, &w.bob).await.body.as_array().unwrap().iter().all(|t| t["scopes"] == json!(["play"])));

    // An administrator now: his token sees everyone's and changes things.
    let promoted = app.patch(&format!("/api/users/{}", w.bob_id), &w.admin, json!({"is_admin": true})).await;
    assert_eq!(promoted.status, StatusCode::OK, "{:?}", promoted.body);
    let tool = app.compatibility_token(&w.bob, "activity manage").await;
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &tool).await), 2);
    assert_eq!(app.plex_json("/accounts", &tool).await["size"], json!(2));
    let refresh = format!("/library/sections/{}/refresh", w.movies);
    assert_eq!(app.plex(Method::GET, &refresh, &tool, &[]).await.status, StatusCode::OK);

    // Not any more: at once, though the token was made with them.
    let demoted = app.patch(&format!("/api/users/{}", w.bob_id), &w.admin, json!({"is_admin": false})).await;
    assert_eq!(demoted.status, StatusCode::OK, "{:?}", demoted.body);
    assert_eq!(app.plex(Method::GET, &refresh, &tool, &[]).await.status, StatusCode::FORBIDDEN);
    assert_eq!(count(&app.plex_json("/status/sessions/history/all", &tool).await), 1, "his own");
    assert_eq!(app.plex_json("/accounts", &tool).await["size"], json!(1));
    let sections = app.plex_json("/library/sections", &tool).await;
    assert_eq!(sections["Directory"][0]["Location"][0]["path"], "movies");
    assert_eq!(w.delete_bobs_play(&tool).await, StatusCode::OK, "his own play is his to delete");
}

#[tokio::test]
async fn no_token_administers_accounts() {
    let w = world().await;
    let app = &w.app;
    let every = app.compatibility_token(&w.admin, "activity manage").await;
    let bob = format!("/api/users/{}", w.bob_id);

    // Pley's administration refuses it, wherever it is sent.
    for (method, uri, body) in [
        (Method::POST, "/api/users".to_owned(), Some(json!({"username": "eve", "password": "test-pass-1"}))),
        (Method::PATCH, bob.clone(), Some(json!({"is_admin": true}))),
        (Method::PATCH, bob.clone(), Some(json!({"password": "taken-over-1"}))),
        (Method::DELETE, bob.clone(), None),
        (Method::DELETE, format!("{bob}/devices"), None),
        (Method::DELETE, format!("{bob}/two-step"), None),
        (Method::POST, "/api/invites".to_owned(), Some(json!({}))),
        (Method::PUT, "/api/settings".to_owned(), Some(json!({"require_two_step": false}))),
    ] {
        let bearer = app.call(method.clone(), &uri, Some(&every), body.clone(), &[]).await;
        assert_eq!(bearer.status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        let plex_header = app.call(method.clone(), &uri, None, body, &[("x-plex-token", &every)]).await;
        assert_ne!(plex_header.status, StatusCode::OK, "{method} {uri}");
        assert_ne!(plex_header.status, StatusCode::CREATED, "{method} {uri}");
        assert_ne!(plex_header.status, StatusCode::NO_CONTENT, "{method} {uri}");
    }
    // The Plex-compatible API lists accounts to it, and changes none of
    // them, nor sharing, nor the server's settings.
    assert_eq!(app.plex_json("/accounts", &every).await["size"], json!(2));
    for (method, uri) in [
        (Method::PUT, format!("/accounts/{}", w.bob_id)),
        (Method::DELETE, format!("/accounts/{}", w.bob_id)),
        (Method::POST, "/accounts".to_owned()),
        (Method::PUT, "/myplex/account".to_owned()),
        (Method::POST, "/home/users".to_owned()),
        (Method::POST, "/api/v2/home/users".to_owned()),
        (Method::POST, "/api/v2/shared_servers".to_owned()),
        (Method::PUT, "/:/prefs?FriendlyName=Taken".to_owned()),
    ] {
        let reply = app.plex(method.clone(), &uri, &every, &[]).await;
        assert!(reply.status.is_client_error(), "{method} {uri}: {}", reply.status);
    }
    let (name, admin, disabled): (String, bool, bool) =
        sqlx::query_as("SELECT username, is_admin, disabled FROM users WHERE id = ?")
            .bind(w.bob_id)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!((name.as_str(), admin, disabled), ("bob", false, false));
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(users, 2);
    assert_eq!(app.get("/api/auth/me", &w.bob).await.status, StatusCode::OK, "bob is signed in still");
}

/// A request to the Jellyfin-compatible API on its own port from `peer`.
async fn jellyfin_from(app: &TestApp, peer: &str, uri: &str, token: &str) -> StatusCode {
    let req = Request::builder()
        .uri(uri)
        .header("x-emby-token", token)
        .extension(axum::extract::ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()))
        .body(Body::empty())
        .unwrap();
    app.jellyfin.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn a_token_works_only_from_its_origins() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let made = app
        .post(TOKENS, &admin, json!({"name": "Library tool", "scope": "manage", "allowed_from": ["10.0.0.0/24"]}))
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let token = made.body["token"].as_str().unwrap().to_owned();

    // The Plex-compatible API, on Pley's port.
    let plex = |peer: &'static str| {
        let token = token.clone();
        let app = &app;
        async move { app.get_from(peer, "/library/sections", &[("x-plex-token", &token)]).await.status }
    };
    assert_eq!(plex("10.0.0.7:50000").await, StatusCode::OK);
    assert_eq!(plex("192.168.1.4:50000").await, StatusCode::UNAUTHORIZED);
    assert_eq!(plex("10.0.0.8:50000").await, StatusCode::OK, "cached, and still checked");
    assert_eq!(plex("10.0.1.8:50000").await, StatusCode::UNAUTHORIZED);
    // Without an address to check, as a request without a connection: refused.
    assert_eq!(sections(&app, &token).await, StatusCode::UNAUTHORIZED);
    // A proxy on the network may say for whom; a client elsewhere can't.
    let forwarded = app
        .get_from("192.168.1.2:443", "/library/sections", &[("x-plex-token", &token), ("x-forwarded-for", "10.0.0.9")])
        .await;
    assert_eq!(forwarded.status, StatusCode::OK);
    let spoofed = app
        .get_from("203.0.113.9:443", "/library/sections", &[("x-plex-token", &token), ("x-forwarded-for", "10.0.0.9")])
        .await;
    assert_eq!(spoofed.status, StatusCode::UNAUTHORIZED);

    // The Jellyfin-compatible API, and the stream addresses it hands out.
    assert_eq!(jellyfin_from(&app, "10.0.0.7:50000", "/UserViews", &token).await, StatusCode::OK);
    assert_eq!(jellyfin_from(&app, "192.168.1.4:50000", "/UserViews", &token).await, StatusCode::UNAUTHORIZED);
    let token_id = made.body["id"].as_i64().unwrap();
    let source = app.jf_ids().source(app.file_of(film).await, token_id);
    let stream = format!("/Videos/{}/stream?static=true&MediaSourceId={source}", app.jf_item(film));
    let player = |peer: &str| {
        Request::builder()
            .uri(&stream)
            .extension(axum::extract::ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(app.jellyfin.clone().oneshot(player("10.0.0.7:50001")).await.unwrap().status(), StatusCode::OK);
    let elsewhere = app.jellyfin.clone().oneshot(player("198.51.100.3:50001")).await.unwrap().status();
    assert_eq!(elsewhere, StatusCode::UNAUTHORIZED);

    // Listed with where it may be used from, and where it was.
    let listed = app.get(TOKENS, &admin).await;
    assert_eq!(listed.body[0]["allowed_from"], json!(["10.0.0.0/24"]));
    assert_eq!(listed.body[0]["ip"], "10.0.0.7");
}
