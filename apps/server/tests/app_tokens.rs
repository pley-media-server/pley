//! Apps' tokens (src/auth/grants.rs) open only the routes their scopes allow
//! (src/auth/scopes.rs), and only from `Authorization: Bearer`: every route
//! of api/mod.rs is classified, an administrator's app administers nothing
//! unless it is one of pley's own allowed `server`, and the
//! Jellyfin-compatible API refuses them.

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use futures::StreamExt;
use pley::auth::scopes::{APP_ROUTES, Need, Scope};
use serde_json::{Value, json};
use tower::ServiceExt;

use common::TestApp;

// ------------------------------------------------------------------ routes

/// Routes anyone may call.
const PUBLIC: &[(&str, &str)] = &[
    ("GET", "/api/health"),
    ("GET", "/api/server"),
    ("POST", "/api/setup"),
    ("POST", "/api/auth/login"),
    ("POST", "/api/auth/logout"),
    ("GET", "/api/invites/{code}"),
    ("POST", "/api/invites/{code}/accept"),
    // Apps exchange codes and refresh tokens, and sign out, with forms.
    ("POST", "/api/oauth/token"),
    ("POST", "/api/oauth/revoke"),
    // TVs start signing in with a code (pley's own apps only), and those
    // that can't draw its QR code ask for it as a picture.
    ("POST", "/api/oauth/device"),
    ("GET", "/api/oauth/device/qr"),
    // A session's media for players that can't send the token: the
    // session's own key stands in for it (tests/tv_apps.rs).
    ("GET", "/api/media/{key}/{file}"),
    ("GET", "/api/live/media/{key}/{file}"),
];

/// Routes for pley's own sign-in only: how an account signs in, its
/// profiles and devices.
const FIRST_PARTY: &[(&str, &str)] = &[
    ("PUT", "/api/auth/password"),
    ("GET", "/api/auth/profiles"),
    ("POST", "/api/auth/switch"),
    ("GET", "/api/me/two-step"),
    ("POST", "/api/me/two-step"),
    ("DELETE", "/api/me/two-step"),
    ("POST", "/api/me/two-step/setup"),
    ("POST", "/api/me/two-step/recovery-codes"),
    ("PATCH", "/api/me"),
    ("PUT", "/api/me/pin"),
    ("PUT", "/api/me/avatar"),
    ("DELETE", "/api/me/avatar"),
    ("GET", "/api/me/devices"),
    ("DELETE", "/api/me/devices/{id}"),
    ("GET", "/api/me/apps"),
    ("DELETE", "/api/me/apps/{id}"),
    ("GET", "/api/me/compatibility-tokens"),
    ("POST", "/api/me/compatibility-tokens"),
    ("DELETE", "/api/me/compatibility-tokens/{id}"),
    // Apps have their keys in their token answers.
    ("GET", "/api/server/proof-key"),
];

/// Administration no app reaches, not even with `server` (what it opens is
/// in APP_ROUTES): collections, editing titles' details, and what holds the
/// server's keys, passwords and providers' addresses, which stay with the
/// web app.
const ADMIN: &[(&str, &str)] = &[
    ("POST", "/api/libraries/{id}/collections"),
    ("PATCH", "/api/collections/{id}"),
    ("DELETE", "/api/collections/{id}"),
    ("POST", "/api/collections/{id}/items"),
    ("DELETE", "/api/collections/{id}/items/{item_id}"),
    ("PUT", "/api/collections/{id}/order"),
    ("PATCH", "/api/items/{id}"),
    ("GET", "/api/items/{id}/matches"),
    ("POST", "/api/items/{id}/match"),
    ("GET", "/api/items/{id}/artwork/{kind}"),
    ("PUT", "/api/items/{id}/artwork/{kind}"),
    ("DELETE", "/api/items/{id}/artwork/{kind}"),
    ("POST", "/api/items/{id}/artwork/{kind}/upload"),
    ("POST", "/api/items/{id}/refresh"),
    ("POST", "/api/items/{id}/analyze"),
    ("GET", "/api/system/library-work"),
    ("GET", "/api/system/backups"),
    ("POST", "/api/system/backups"),
    ("PUT", "/api/system/transcode-dir"),
    ("GET", "/api/settings"),
    ("PUT", "/api/settings"),
    ("POST", "/api/settings/test"),
    ("POST", "/api/settings/arr"),
    ("GET", "/api/network"),
    ("PUT", "/api/network"),
    ("GET", "/api/live/settings"),
    ("PUT", "/api/live/settings"),
    ("GET", "/api/live/sources"),
    ("POST", "/api/live/sources"),
    ("POST", "/api/live/sources/test"),
    ("PUT", "/api/live/sources/{id}"),
    ("DELETE", "/api/live/sources/{id}"),
    ("POST", "/api/live/sources/{id}/refresh"),
];

/// Routes that take pley's own page's cookie and nothing else: allowing
/// an app, or a TV by the code it shows (api/oauth.rs WebUser).
const COOKIE_ONLY: &[(&str, &str)] = &[
    ("POST", "/api/oauth/requests"),
    ("POST", "/api/oauth/requests/{id}"),
    ("GET", "/api/oauth/requests/{id}/continue"),
    ("POST", "/api/oauth/device/verify"),
];

/// Every method and path of the API router of `source` (api/mod.rs): the
/// `.route(path, …)` calls between `let api = Router::new()` and its
/// `.fallback`, with the methods named in each (`get(…)`, `.post(…)`, …).
fn routes_in(source: &str) -> Vec<(String, String)> {
    let start = source.find("let api = Router::new()").expect("the api router");
    let end = start + source[start..].find(".fallback(").expect("its fallback");
    let block = &source[start..end];
    let mut routes = Vec::new();
    let mut rest = block;
    while let Some(at) = rest.find(".route(") {
        rest = &rest[at + ".route(".len()..];
        let open = rest.find('"').expect("a path");
        let close = open + 1 + rest[open + 1..].find('"').expect("the end of the path");
        let path = format!("/api{}", &rest[open + 1..close]);
        let args = call_arguments(&rest[close + 1..]);
        let methods = methods_in(args);
        assert!(!methods.is_empty(), "no method for {path}");
        routes.extend(methods.into_iter().map(|method| (method, path.clone())));
        rest = &rest[close + 1 + args.len()..];
    }
    routes
}

/// The rest of a call's arguments: up to the parenthesis that closes it.
fn call_arguments(text: &str) -> &str {
    let mut depth = 1;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        match (in_string, c) {
            (true, _) if escaped => escaped = false,
            (true, '\\') => escaped = true,
            (true, '"') => in_string = false,
            (true, _) => {}
            (false, '"') => in_string = true,
            (false, '(') => depth += 1,
            (false, ')') => {
                depth -= 1;
                if depth == 0 {
                    return &text[..i];
                }
            }
            _ => {}
        }
    }
    panic!("unclosed .route(")
}

/// HTTP methods named in a route's arguments: `get(` at the start (after
/// the path's comma) or after a dot, outside strings.
fn methods_in(args: &str) -> Vec<String> {
    let chars: Vec<char> = args.chars().collect();
    let mut methods = Vec::new();
    let mut previous = ',';
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            previous = '"';
            i += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let begin = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[begin..i].iter().collect();
            let called = chars.get(i) == Some(&'(');
            if called && matches!(previous, ',' | '.') && ["get", "post", "put", "patch", "delete"].contains(&&*word) {
                methods.push(word.to_uppercase());
            }
            previous = 'a';
            continue;
        }
        if !c.is_whitespace() {
            previous = c;
        }
        i += 1;
    }
    methods
}

fn api_routes() -> Vec<(String, String)> {
    routes_in(include_str!("../src/api/mod.rs"))
}

fn app_route(method: &str, path: &str) -> Option<Need> {
    APP_ROUTES.iter().find(|(m, p, _)| *m == method && *p == path).map(|(_, _, need)| *need)
}

/// Which class lists a route: problems as sentences.
fn classification_problems(routes: &[(String, String)]) -> Vec<String> {
    let classes: [(&str, Vec<(&str, &str)>); 5] = [
        ("APP_ROUTES", APP_ROUTES.iter().map(|(m, p, _)| (*m, *p)).collect()),
        ("PUBLIC", PUBLIC.to_vec()),
        ("FIRST_PARTY", FIRST_PARTY.to_vec()),
        ("ADMIN", ADMIN.to_vec()),
        ("COOKIE_ONLY", COOKIE_ONLY.to_vec()),
    ];
    let mut problems = Vec::new();
    for (method, path) in routes {
        let found: Vec<&str> = classes
            .iter()
            .filter(|(_, list)| list.iter().any(|(m, p)| m == method && p == path))
            .map(|(name, _)| *name)
            .collect();
        match found.len() {
            1 => {}
            0 => problems.push(format!("{method} {path} is in no class: add it to one (apps get no route by default)")),
            _ => problems.push(format!("{method} {path} is in several classes: {found:?}")),
        }
    }
    for (name, list) in &classes {
        for (method, path) in list {
            if !routes.iter().any(|(m, p)| m == method && p == path) {
                problems.push(format!("{name} lists {method} {path}, which api/mod.rs doesn't route"));
            }
        }
    }
    problems
}

#[test]
fn every_route_is_classified() {
    let routes = api_routes();
    assert!(routes.len() >= 109, "{} routes read: the parser lost some", routes.len());
    assert!(routes.contains(&("GET".into(), "/api/health".into())), "a closure handler");
    assert!(routes.contains(&("DELETE".into(), "/api/me/avatar".into())), "a method after a layered one");
    assert!(routes.contains(&("POST".into(), "/api/items/{id}/artwork/{kind}/upload".into())), "a multi-line route");
    let problems = classification_problems(&routes);
    assert!(problems.is_empty(), "{problems:#?}");
    // The household's profiles, for apps allowed to switch between them.
    assert!(routes.contains(&("GET".into(), "/api/oauth/profiles".into())));
    assert_eq!(app_route("GET", "/api/oauth/profiles"), Some(Need::Scope(pley::auth::scopes::Scope::Profiles)));
    // The profile's track defaults are part of playing; its name and
    // picture (PATCH /api/me) stay the web's.
    assert_eq!(app_route("PUT", "/api/me/track-defaults"), Some(Need::Scope(pley::auth::scopes::Scope::Play)));
    assert!(FIRST_PARTY.contains(&("PATCH", "/api/me")));
    // Whether requests are on, for any app; asking with `requests`,
    // managing with `approvals`.
    assert_eq!(app_route("GET", "/api/requests/summary"), Some(Need::AnyApp));
    let requests = Some(Need::Scope(pley::auth::scopes::Scope::Requests));
    assert_eq!(app_route("GET", "/api/discover/images/{kind}/{file}"), requests);
    assert_eq!(app_route("GET", "/api/items/{id}/request"), requests);
    assert_eq!(app_route("POST", "/api/requests/seen"), requests);
    assert_eq!(app_route("DELETE", "/api/requests/{id}/watch"), requests);
    let approvals = Some(Need::Scope(pley::auth::scopes::Scope::Approvals));
    assert_eq!(app_route("POST", "/api/requests/{id}/decline"), approvals);
    assert_eq!(app_route("POST", "/api/requests/{id}/handled"), approvals);
    assert_eq!(app_route("GET", "/api/requests/options"), approvals);
    // TVs: anyone starts with a code, which only pley's own page takes.
    assert!(PUBLIC.contains(&("POST", "/api/oauth/device")));
    assert!(PUBLIC.contains(&("GET", "/api/oauth/device/qr")));
    assert!(COOKIE_ONLY.contains(&("POST", "/api/oauth/device/verify")));
}

#[test]
fn the_classification_notices_new_routes_and_misspellings() {
    // A route added to the router without a class.
    let source = include_str!("../src/api/mod.rs").replacen(
        ".route(\"/health\"",
        ".route(\"/me/secrets\", get(accounts::my_secrets).post(\n            accounts::more))\n        .route(\"/health\"",
        1,
    );
    let problems = classification_problems(&routes_in(&source));
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(problems.iter().all(|p| p.contains("/api/me/secrets") && p.contains("no class")));
    // A table entry naming no route (here, a route gone from the router).
    let source = include_str!("../src/api/mod.rs").replacen(".route(\"/hubs/home\", get(browse::home))", "", 1);
    let problems = classification_problems(&routes_in(&source));
    assert_eq!(problems, ["APP_ROUTES lists GET /api/hubs/home, which api/mod.rs doesn't route"]);
}

// ------------------------------------------------------------------ requests

/// A request with the token in `Authorization: Bearer`, and a JSON body
/// for methods that take one.
fn bearer(method: &str, uri: &str, token: &str) -> Request<Body> {
    let req = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).unwrap())
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if matches!(method, "POST" | "PUT" | "PATCH") {
        req.header(header::CONTENT_TYPE, "application/json").body(Body::from("{}")).unwrap()
    } else {
        req.body(Body::empty()).unwrap()
    }
}

/// Status and `WWW-Authenticate` of a request, without reading the body
/// (the event stream doesn't end).
async fn status_of(app: &TestApp, req: Request<Body>) -> (StatusCode, String) {
    let resp = app.api.clone().oneshot(req).await.unwrap();
    let challenge = resp.headers().get(header::WWW_AUTHENTICATE).map(|v| v.to_str().unwrap().to_owned());
    (resp.status(), challenge.unwrap_or_default())
}

/// A request path for a routed path: every parameter 1.
fn concrete(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let close = open + rest[open..].find('}').unwrap();
        out.push_str(&rest[..open]);
        out.push('1');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

const NO_SCOPE: &str = r#"Bearer error="insufficient_scope""#;

#[tokio::test]
async fn an_administrators_app_token_opens_only_app_routes() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    app.library(&admin, "Movies", "movies", "movies").await;
    let (token, _) = app.app_token(1, "library play playlists profiles requests approvals activity").await;
    for (method, path) in api_routes() {
        if PUBLIC.iter().any(|(m, p)| *m == method && *p == path) {
            // No sign-in needed (and logout would end the grant).
            continue;
        }
        let uri = concrete(&path);
        let (status, challenge) = status_of(&app, bearer(&method, &uri, &token)).await;
        let first_party = FIRST_PARTY.iter().chain(ADMIN).any(|(m, p)| *m == method && *p == path);
        if first_party {
            assert_eq!((status, challenge.as_str()), (StatusCode::FORBIDDEN, NO_SCOPE), "{method} {uri}");
        } else if app_route(&method, &path) == Some(Need::Scope(Scope::Server)) {
            // Administration: not without `server`.
            let expected = r#"Bearer error="insufficient_scope", scope="server""#;
            assert_eq!((status, challenge.as_str()), (StatusCode::FORBIDDEN, expected), "{method} {uri}");
        } else if app_route(&method, &path).is_some() {
            assert!(status != StatusCode::UNAUTHORIZED && challenge.is_empty(), "{method} {uri}: {status} {challenge}");
        } else if COOKIE_ONLY.iter().any(|(m, p)| *m == method && *p == path) {
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }
    // Still signed in: nothing above ended the grant.
    let me = app.get("/api/auth/me", &token).await;
    assert_eq!((me.status, &me.body["is_admin"]), (StatusCode::OK, &json!(false)));
    // HEAD is GET.
    let head = Request::builder()
        .method(Method::HEAD)
        .uri("/api/libraries")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert!(status_of(&app, head).await.1.is_empty());
}

#[tokio::test]
async fn each_scope_opens_only_its_routes() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, ann) = app.user(&admin, "ann", json!({})).await;
    for scope in ["library", "play", "playlists", "profiles", "requests", "approvals", "activity", "server"] {
        let (token, _) = app.app_token(ann, scope).await;
        for (method, path, need) in APP_ROUTES {
            let uri = concrete(path);
            let (status, challenge) = status_of(&app, bearer(method, &uri, &token)).await;
            // The scope a token without it is told of, when this one doesn't open the route.
            let refused = match need {
                Need::Scope(needed) if needed.name() != scope => Some(needed),
                Need::Either(one, other) if one.name() != scope && other.name() != scope => Some(one),
                _ => None,
            };
            match refused {
                Some(needed) => {
                    let expected = format!(r#"Bearer error="insufficient_scope", scope="{}""#, needed.name());
                    assert_eq!((status, challenge), (StatusCode::FORBIDDEN, expected), "{scope}: {method} {uri}");
                }
                None => assert!(challenge.is_empty(), "{scope}: {method} {uri}: {status} {challenge}"),
            }
            if scope == "server" && *need == Need::Scope(Scope::Server) {
                // Ann isn't an administrator: the scope opens the route,
                // her role doesn't let her through.
                assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
            }
        }
    }
    let (library, _) = app.app_token(ann, "library").await;
    let denied = app.app_call(Method::POST, "/api/playlists", &library, Some(json!({"title": "Mine"}))).await;
    assert_eq!((denied.status, &denied.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    assert_eq!(
        denied.body["message"],
        "this app wasn't allowed to make, change and delete the profile's playlists: sign in to it again to allow it"
    );
    let devices = app.get("/api/me/devices", &library).await;
    assert_eq!(devices.body["message"], "apps can't do that: use Pley's web app");
    let settings = app.get("/api/settings", &library).await;
    assert_eq!(settings.body["message"], "apps can't do that: use Pley's web app");
    let users = app.get("/api/users", &library).await;
    assert_eq!(
        users.body["message"],
        "this app wasn't allowed to manage the server's libraries, accounts, invitations and Live TV channels, and \
         see its dashboard, where the account is an administrator: sign in to it again to allow it"
    );
}

#[tokio::test]
async fn app_tokens_go_only_in_the_authorization_header() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (token, refresh) = app.app_token(1, "library").await;
    let misplaced = [
        app.call(Method::GET, &format!("/api/auth/me?token={token}"), None, None, &[]).await,
        app.call(Method::GET, "/api/auth/me", None, None, &[("x-pley-token", &token)]).await,
        app.call(Method::GET, "/api/auth/me", None, None, &[("cookie", &format!("pley_token={token}"))]).await,
        app.call(Method::GET, &format!("/api/libraries?token={token}"), None, None, &[]).await,
    ];
    for reply in misplaced {
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        assert_eq!(reply.body["message"], "app tokens go in the Authorization header");
        assert_eq!(reply.headers[header::WWW_AUTHENTICATE], r#"Bearer error="invalid_request""#);
    }
    // A signed-in page whose URL carries an app token: the cookie decides.
    let page = app
        .call(
            Method::GET,
            &format!("/api/auth/me?token={token}"),
            None,
            None,
            &[("cookie", &format!("pley_token={admin}"))],
        )
        .await;
    assert_eq!(
        (page.status, &page.body["is_admin"], &page.body["scope"]),
        (StatusCode::OK, &json!(true), &Value::Null)
    );
    // A refresh token is no bearer token, nor is an unknown one.
    for bad in [refresh.as_str(), "pley_at_unknown", "pley_x"] {
        let reply = app.get("/api/auth/me", bad).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{bad}");
        assert_eq!(reply.body["code"], "unauthorized");
        assert_eq!(reply.headers[header::WWW_AUTHENTICATE], r#"Bearer error="invalid_token""#);
    }
    // The scheme in any letter case.
    let lower =
        app.call(Method::GET, "/api/auth/me", None, None, &[("authorization", &format!("bearer {token}"))]).await;
    assert_eq!((lower.status, &lower.body["scope"]), (StatusCode::OK, &json!("library")));
    // Session tokens work as before, wherever they are.
    let session = app.call(Method::GET, "/api/auth/me", None, None, &[("x-pley-token", &admin)]).await;
    assert_eq!(session.status, StatusCode::OK);
    assert!(!session.headers.contains_key(header::WWW_AUTHENTICATE));
    let nothing = app.call(Method::GET, "/api/auth/me", None, None, &[]).await;
    assert_eq!(nothing.status, StatusCode::UNAUTHORIZED);
    assert!(!nothing.headers.contains_key(header::WWW_AUTHENTICATE), "sessions answer as they always did");
}

#[tokio::test]
async fn the_jellyfin_api_refuses_app_tokens() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let tool = app.compatibility_token(&admin, "play").await;
    let (token, _) = app.app_token(1, "library play playlists profiles").await;
    assert_eq!(app.jf(Method::GET, "/UserViews", &tool, None).await.status, StatusCode::OK);
    assert_eq!(app.jf(Method::GET, "/UserViews", &token, None).await.status, StatusCode::UNAUTHORIZED);
    let jellyfin = |uri: String, headers: Vec<(&'static str, String)>| {
        let mut req = Request::builder().uri(uri);
        for (name, value) in headers {
            req = req.header(name, value);
        }
        app.jellyfin.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    for (uri, headers) in [
        (format!("/UserViews?api_key={token}"), vec![]),
        ("/UserViews".to_owned(), vec![("authorization", format!("Bearer {token}"))]),
        ("/UserViews".to_owned(), vec![("x-emby-token", token.clone())]),
        ("/UserViews".to_owned(), vec![("authorization", format!(r#"MediaBrowser Token="{token}""#))]),
        // Not even next to a compatibility token: the header is read first.
        (format!("/UserViews?api_key={tool}"), vec![("x-emby-token", token.clone())]),
    ] {
        let resp = jellyfin(uri.clone(), headers).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    // The socket, before upgrading.
    let socket = app.jf(Method::GET, &format!("/socket?api_key={token}"), "", None).await;
    assert_eq!(socket.status, StatusCode::UNAUTHORIZED);
    let socket = app.jf(Method::GET, &format!("/socket?api_key={tool}"), "", None).await;
    assert_ne!(socket.status, StatusCode::UNAUTHORIZED, "a compatibility token gets as far as the upgrade");
}

// ------------------------------------------------------------------ rights

#[tokio::test]
async fn an_administrators_app_sees_everything_but_administers_nothing() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    app.media_file("private/Secret (2021).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let private = app.library(&admin, "Private", "movies", "private").await;
    let film = app.ids(movies, &admin).await["Film"];
    let secret = app.ids(private, &admin).await["Secret"];
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    // The administrator's own profile would see no library if roles didn't count.
    sqlx::query("UPDATE users SET all_libraries = 0, max_age = 12 WHERE id = 1")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    app.state.auth.forget_user(1);
    let (token, _) = app.app_token(1, "library play playlists").await;

    let me = app.get("/api/auth/me", &token).await;
    assert_eq!((me.status, &me.body["id"], &me.body["is_admin"]), (StatusCode::OK, &json!(1), &json!(false)));
    assert_eq!(me.body["scope"], "library play playlists");
    assert_eq!(app.get("/api/auth/me", &admin).await.body["is_admin"], json!(true));

    let libraries = app.get("/api/libraries", &token).await;
    let names: Vec<&str> = libraries.body.as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Movies", "Private"], "every library");
    assert_eq!(app.get(&format!("/api/items/{secret}"), &token).await.status, StatusCode::OK, "no age limit");

    // Only its own history, whatever it asks.
    for (user, item) in [(1, film), (bob_id, secret)] {
        sqlx::query("INSERT INTO play_history (user_id, item_id, viewed_at) VALUES (?, ?, 1)")
            .bind(user)
            .bind(item)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    assert_eq!(app.get("/api/history?all=1", &admin).await.body["total"], json!(2));
    assert_eq!(app.get("/api/history?all=1", &token).await.body["total"], json!(1));
    assert_eq!(app.get(&format!("/api/history?user_id={bob_id}"), &token).await.body["total"], json!(1));
    let theirs: i64 = sqlx::query_scalar("SELECT id FROM play_history WHERE user_id = ?")
        .bind(bob_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(app.delete(&format!("/api/history/{theirs}"), &token).await.status, StatusCode::NOT_FOUND);

    // Nobody else's playback.
    let caps = json!({"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]});
    let play = app.post("/api/playback", &bob, json!({"item_id": film, "caps": caps, "mode": "direct"})).await;
    assert_eq!(play.status, StatusCode::OK, "{:?}", play.body);
    let session = play.body["session_id"].as_str().unwrap();
    assert_eq!(app.get(&format!("/api/playback/{session}"), &admin).await.status, StatusCode::OK);
    assert_eq!(app.get(&format!("/api/playback/{session}"), &token).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/playback/{session}"), &token).await.status, StatusCode::FORBIDDEN);
    let activity = app.get("/api/activity", &token).await;
    assert_eq!((activity.status, &activity.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    let paths = &app.get("/api/libraries", &token).await.body[0]["paths"];
    assert_eq!(paths, &json!([]), "no folders");
    assert_eq!(me.body["account_admin"], json!(true), "pley's own apps are told the account's role");

    // Empty curated collections are for administrators to fill.
    let empty = app.post(&format!("/api/libraries/{movies}/collections"), &admin, json!({"title": "Empty"})).await;
    assert_eq!(empty.status, StatusCode::CREATED, "{:?}", empty.body);
    let collection = empty.body["id"].as_i64().unwrap();
    let titles = |reply: common::Reply| common::titles(&reply.body["collections"]);
    let listed = titles(app.get(&format!("/api/libraries/{movies}/collections"), &admin).await);
    assert_eq!(listed, ["Empty"]);
    let listed = titles(app.get(&format!("/api/libraries/{movies}/collections"), &token).await);
    assert!(listed.is_empty(), "{listed:?}");
    assert_eq!(app.get(&format!("/api/collections/{collection}"), &token).await.status, StatusCode::NOT_FOUND);

    // Administration, and turning into a session.
    let refused = [
        app.post("/api/libraries", &token, json!({"name": "X", "kind": "movies", "paths": []})).await,
        app.post("/api/auth/switch", &token, json!({"user_id": 1})).await,
        app.put("/api/auth/password", &token, json!({"current_password": "secret-one", "new_password": "x"})).await,
        app.get("/api/settings", &token).await,
        app.get("/api/users", &token).await,
    ];
    for reply in refused {
        assert_eq!((reply.status, &reply.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
        assert!(reply.body.get("token").is_none());
    }
}

#[tokio::test]
async fn pleys_own_app_allowed_server_manages_it_on_an_administrators_account() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let (_, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (token, _) = app.app_token(1, "library play server").await;

    // Still no administrator itself, told the account's role.
    let me = app.get("/api/auth/me", &token).await;
    assert_eq!((&me.body["is_admin"], &me.body["account_admin"]), (&json!(false), &json!(true)));
    assert_eq!(me.body["scope"], "library play server");

    // What Settings in the web app do, but for what holds keys and passwords.
    assert_eq!(app.get("/api/system", &token).await.status, StatusCode::OK);
    for (method, path) in ADMIN {
        let uri = concrete(path);
        let (status, challenge) = status_of(&app, bearer(method, &uri, &token)).await;
        assert_eq!((status, challenge.as_str()), (StatusCode::FORBIDDEN, NO_SCOPE), "{method} {uri}");
    }
    let users = app.get("/api/users", &token).await;
    assert_eq!((users.status, users.body.as_array().map(Vec::len)), (StatusCode::OK, Some(2)));
    let carol = json!({"username": "carol", "password": "test-pass-1"});
    assert_eq!(app.app_call(Method::POST, "/api/users", &token, Some(carol)).await.status, StatusCode::CREATED);
    let library = format!("/api/libraries/{movies}");
    let renamed = app.app_call(Method::PATCH, &library, &token, Some(json!({"name": "Films"}))).await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.body);
    let invite = app.app_call(Method::POST, "/api/invites", &token, Some(json!({}))).await;
    assert_eq!(invite.status, StatusCode::CREATED, "{:?}", invite.body);
    // Everyone's pictures, as the list of accounts shows them, outside the household too.
    let avatars = app.state.config.data_dir.join("avatars");
    std::fs::create_dir_all(&avatars).unwrap();
    std::fs::write(avatars.join("bob.jpg"), [0xff, 0xd8, 0xff, 0xd9]).unwrap();
    sqlx::query("UPDATE users SET avatar = 'bob.jpg', home = 0 WHERE id = ?")
        .bind(bob_id)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let picture = format!("/api/users/{bob_id}/avatar");
    assert_eq!(app.app_call(Method::GET, &picture, &token, None).await.status, StatusCode::OK);
    // With the folders it changes, and the dashboard's view of playback.
    let paths = &app.get("/api/libraries", &token).await.body[0]["paths"];
    assert_eq!(paths, &json!([app.media_path("movies")]));
    for user in [1, bob_id] {
        sqlx::query("INSERT INTO play_history (user_id, item_id, viewed_at) VALUES (?, ?, 1)")
            .bind(user)
            .bind(film)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    assert_eq!(app.get("/api/history?all=1", &token).await.body["total"], json!(2));
    assert_eq!(app.get("/api/activity", &token).await.status, StatusCode::OK);
    // The dashboard removes anyone's play, as the web's does.
    let theirs: i64 = sqlx::query_scalar("SELECT id FROM play_history WHERE user_id = ?")
        .bind(bob_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let (viewer, _) = app.app_token(1, "library play").await;
    assert_eq!(app.delete(&format!("/api/history/{theirs}"), &viewer).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.delete(&format!("/api/history/{theirs}"), &token).await.status, StatusCode::NO_CONTENT);
    sqlx::query("INSERT INTO play_history (user_id, item_id, viewed_at) VALUES (?, ?, 1)")
        .bind(bob_id)
        .bind(film)
        .execute(&app.state.db.write)
        .await
        .unwrap();

    // Its own sign-in stays in Account, as for the web's administrators.
    let own = [
        app.app_call(Method::PATCH, "/api/users/1", &token, Some(json!({"password": "another-pass-2"}))).await,
        app.app_call(Method::PATCH, "/api/users/1", &token, Some(json!({"pin": "1234"}))).await,
        app.app_call(Method::DELETE, "/api/users/1/two-step", &token, None).await,
        app.app_call(Method::DELETE, "/api/users/1/devices", &token, None).await,
        app.app_call(Method::DELETE, "/api/users/1", &token, None).await,
        app.app_call(Method::PATCH, "/api/users/1", &token, Some(json!({"display_name": "Boss"}))).await,
        app.app_call(Method::GET, "/api/users/1/devices", &token, None).await,
        app.app_call(Method::GET, "/api/users/1/apps", &token, None).await,
        app.app_call(Method::DELETE, "/api/users/1/apps/1", &token, None).await,
    ];
    for reply in own {
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{:?}", reply.body);
    }
    // Others' devices and apps, as the web's Users page shows them.
    for uri in [format!("/api/users/{bob_id}/devices"), format!("/api/users/{bob_id}/apps")] {
        assert_eq!(app.get(&uri, &token).await.status, StatusCode::OK, "{uri}");
    }
    for (method, uri, body) in [
        (Method::PUT, "/api/auth/password", Some(json!({"current_password": "secret-one", "new_password": "x"}))),
        (Method::GET, "/api/me/devices", None),
        (Method::POST, "/api/auth/switch", Some(json!({"user_id": 1}))),
        (Method::PATCH, "/api/collections/1", Some(json!({"title": "X"}))),
    ] {
        let reply = app.app_call(method, uri, &token, body).await;
        assert_eq!((reply.status, &reply.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")), "{uri}");
    }

    // Nothing for an account that isn't an administrator, an app by another
    // publisher, or once the role is gone.
    let (bobs, _) = app.app_token(bob_id, "library play server").await;
    assert_eq!(app.get("/api/auth/me", &bobs).await.body["account_admin"], json!(false));
    let other = third_party_token(&app, 1, "library play server").await;
    assert_eq!(app.get("/api/auth/me", &other).await.body.get("account_admin"), None, "only pley's own apps are told");
    let (dora, dora_id) = app.user(&admin, "dora", json!({"is_admin": true})).await;
    let (doras, _) = app.app_token(dora_id, "library play server").await;
    assert_eq!(app.get("/api/users", &doras).await.status, StatusCode::OK);
    let demoted = app.patch(&format!("/api/users/{dora_id}"), &admin, json!({"is_admin": false})).await;
    assert_eq!(demoted.status, StatusCode::OK, "{:?}", demoted.body);
    for refused in [&bobs, &other, &doras] {
        let reply = app.get("/api/users", refused).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{:?}", reply.body);
        assert_eq!(app.get("/api/libraries", refused).await.body[0]["paths"], json!([]));
    }
    assert_eq!(app.get("/api/history?all=1", &bobs).await.body["total"], json!(1), "her own");
    let (plain, _) = app.app_token(1, "library play").await;
    let reply = app.app_call(Method::GET, &picture, &plain, None).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND, "not outside the household without managing the server");
    drop(dora);
}

/// Tokens of an app by another publisher acting as `user_id`, allowed `scope`.
async fn third_party_token(app: &TestApp, user_id: i64, scope: &str) -> String {
    let issued = app
        .state
        .auth
        .issue(pley::auth::grants::NewGrant {
            user_id,
            approved_by: user_id,
            client_id: "https://apps.example.com/player.json".into(),
            client_name: Some("Example Player".into()),
            scopes: pley::auth::scopes::Scopes::parse(scope).unwrap(),
            refresh: true,
            first_party: false,
            session_hash: None,
            user_agent: None,
            ip: None,
        })
        .await
        .unwrap();
    issued.access_token
}

/// The users whose sessions `/api/activity` lists to `token`, sorted.
async fn activity_users(app: &TestApp, token: &str) -> Vec<i64> {
    let reply = app.get("/api/activity", token).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let mut users: Vec<i64> =
        reply.body["sessions"].as_array().unwrap().iter().map(|s| s["user_id"].as_i64().unwrap()).collect();
    users.sort_unstable();
    users
}

#[tokio::test]
async fn an_administrators_own_app_allowed_activity_sees_everyones_sessions() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let caps = json!({"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]});
    for token in [&admin, &bob] {
        let play = app.post("/api/playback", token, json!({"item_id": film, "caps": caps, "mode": "direct"})).await;
        assert_eq!(play.status, StatusCode::OK, "{:?}", play.body);
    }
    assert_eq!(activity_users(&app, &admin).await, [1, bob_id], "the dashboard");

    // One of pley's own apps on the administrator's account, allowed
    // activity: everyone's, as the dashboard, and none of its counters.
    let (own, _) = app.app_token(1, "library play activity").await;
    assert_eq!(activity_users(&app, &own).await, [1, bob_id]);
    let reply = app.get("/api/activity", &own).await;
    assert_eq!(reply.body["analysis"], Value::Null);
    assert_eq!(reply.body["library_work"], Value::Null);
    assert_eq!((&reply.body["seasons_pending"], &reply.body["previews_pending"]), (&json!(0), &json!(0)));
    assert_eq!(app.get("/api/auth/me", &own).await.body["is_admin"], json!(false), "still no administrator");
    // Pley for Home Assistant the same.
    let home = app
        .state
        .auth
        .issue(pley::auth::grants::NewGrant {
            user_id: 1,
            approved_by: 1,
            client_id: common::HOMEASSISTANT_APP.into(),
            client_name: Some("pley".into()),
            scopes: pley::auth::scopes::Scopes::parse("library play activity").unwrap(),
            refresh: true,
            first_party: true,
            session_hash: None,
            user_agent: Some("pleyHomeAssistant/0.1.0 (Home Assistant 2026.11.0)".into()),
            ip: None,
        })
        .await
        .unwrap();
    assert_eq!(activity_users(&app, &home.access_token).await, [1, bob_id]);

    // Without activity: refused, as any scope's route.
    let (play_only, _) = app.app_token(1, "library play").await;
    let refused = app.get("/api/activity", &play_only).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    assert_eq!(refused.headers[header::WWW_AUTHENTICATE], r#"Bearer error="insufficient_scope", scope="activity""#);

    // A non-administrator's app allowed activity: its own alone.
    let (bobs, _) = app.app_token(bob_id, "library play activity").await;
    assert_eq!(activity_users(&app, &bobs).await, [bob_id]);
    // An app by another publisher allowed activity on the administrator's
    // account: the administrator's own alone.
    let other = third_party_token(&app, 1, "library activity").await;
    assert_eq!(activity_users(&app, &other).await, [1]);
}

#[tokio::test]
async fn apps_and_non_administrators_dont_see_folders_or_file_paths() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020)/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let (token, _) = app.app_token(1, "library").await;
    let folder = app.media_path("movies");

    assert_eq!(app.get("/api/libraries", &admin).await.body[0]["paths"], json!([folder]));
    assert_eq!(app.get("/api/libraries", &token).await.body[0]["paths"], json!([]));
    assert_eq!(app.get(&format!("/api/libraries/{movies}"), &token).await.body["paths"], json!([]));
    let path = app.get(&format!("/api/items/{film}"), &admin).await.body["files"][0]["path"].clone();
    assert!(path.as_str().unwrap().starts_with(&folder), "{path}");
    let detail = app.get(&format!("/api/items/{film}"), &token).await;
    assert_eq!(detail.body["files"][0]["path"], "Film (2020).mkv");
    assert!(!detail.mentions(&folder.replace('\\', "\\\\")) && !detail.mentions(&folder));

    // Nor do people who aren't administrators, on either API.
    let (viewer, _) = app.user(&admin, "viewer", json!({"all_libraries": true})).await;
    assert_eq!(app.get("/api/libraries", &viewer).await.body[0]["paths"], json!([]));
    let detail = app.get(&format!("/api/items/{film}"), &viewer).await;
    assert_eq!(detail.body["files"][0]["path"], "Film (2020).mkv");
    // Jellyfin's apps are shown files' names, administrators' too.
    for token in [&viewer, &admin] {
        let item = app.jf_json(&format!("/Items/{}", app.jf_item(film)), token).await;
        assert_eq!(
            (&item["Path"], &item["MediaSources"][0]["Path"]),
            (&json!("/Film (2020).mkv"), &json!("/Film (2020).mkv"))
        );
        let views = app.jf(Method::GET, "/UserViews", token, None).await;
        assert!(!views.mentions(&folder.replace('\\', "\\\\")) && !views.mentions(&folder), "{}", views.text());
        assert!(!item.to_string().contains(&folder.replace('\\', "\\\\")));
    }
}

#[tokio::test]
async fn apps_see_only_their_households_pictures() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, ann) = app.user(&admin, "ann", json!({"home": true})).await;
    let (_, bob) = app.user(&admin, "bob", json!({"home": true})).await;
    let (_, carl) = app.user(&admin, "carl", json!({"home": false})).await;
    let (_, dave) = app.user(&admin, "dave", json!({"home": true})).await;
    let avatars = app.state.config.data_dir.join("avatars");
    std::fs::create_dir_all(&avatars).unwrap();
    for id in [ann, bob, carl, dave] {
        std::fs::write(avatars.join(format!("{id}.jpg")), b"jpeg").unwrap();
        sqlx::query("UPDATE users SET avatar = ? WHERE id = ?")
            .bind(format!("{id}.jpg"))
            .bind(id)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE users SET disabled = 1 WHERE id = ?").bind(dave).execute(&app.state.db.write).await.unwrap();
    let picture = |token: String, id: i64| {
        let app = &app;
        async move { app.get(&format!("/api/users/{id}/avatar"), &token).await.status }
    };
    let (anns, _) = app.app_token(ann, "library").await;
    assert_eq!(picture(anns.clone(), ann).await, StatusCode::OK, "its own");
    assert_eq!(picture(anns.clone(), bob).await, StatusCode::OK, "its household's");
    assert_eq!(picture(anns.clone(), carl).await, StatusCode::NOT_FOUND, "not in the household");
    assert_eq!(picture(anns.clone(), dave).await, StatusCode::NOT_FOUND, "disabled");
    let (carls, _) = app.app_token(carl, "library").await;
    assert_eq!(picture(carls.clone(), carl).await, StatusCode::OK);
    assert_eq!(picture(carls, ann).await, StatusCode::NOT_FOUND, "outside the household, only its own");
    assert_eq!(picture(admin, carl).await, StatusCode::OK, "sessions see every picture as before");
}

// ------------------------------------------------------------------ ends

#[tokio::test]
async fn logout_with_an_app_token_ends_its_grant() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (token, refresh) = app.app_token(1, "library").await;
    let (other, _) = app.app_token(1, "library").await;
    assert_eq!(app.get("/api/auth/me", &token).await.status, StatusCode::OK);
    let out = app.call(Method::POST, "/api/auth/logout", Some(&token), None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let after = app.get("/api/auth/me", &token).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert_eq!(after.headers[header::WWW_AUTHENTICATE], r#"Bearer error="invalid_token""#);
    let exchange = pley::auth::grants::RefreshInput {
        token: refresh,
        client_id: common::ANDROID_APP.into(),
        scope: None,
        user_agent: None,
        ip: None,
    };
    assert!(app.state.auth.refresh(exchange).await.is_err());
    assert_eq!(app.get("/api/auth/me", &other).await.status, StatusCode::OK, "another sign-in of the app stays");
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK, "and the session");
    // From a misplaced header too: the app is signing out.
    let out = app.call(Method::POST, "/api/auth/logout", None, None, &[("x-pley-token", &other)]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &other).await.status, StatusCode::UNAUTHORIZED);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn a_password_change_ends_the_accounts_apps() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (ann_session, ann) = app.user(&admin, "ann", json!({})).await;
    let (_, bob) = app.user(&admin, "bob", json!({})).await;
    let (anns, refresh) = app.app_token(ann, "library").await;
    let (bobs, _) = app.app_token(bob, "library").await;
    // An app acting as Bob that Ann allowed.
    let allowed = app
        .state
        .auth
        .issue(pley::auth::grants::NewGrant {
            user_id: bob,
            approved_by: ann,
            client_id: common::ANDROID_APP.into(),
            client_name: None,
            scopes: pley::auth::scopes::Scopes::parse("library").unwrap(),
            refresh: false,
            first_party: true,
            session_hash: None,
            user_agent: None,
            ip: None,
        })
        .await
        .unwrap();
    for token in [&anns, &bobs, &allowed.access_token] {
        assert_eq!(app.get("/api/auth/me", token).await.status, StatusCode::OK);
    }
    let changed = app
        .put(
            "/api/auth/password",
            &ann_session,
            json!({"current_password": "test-pass-1", "new_password": "another-pass-2"}),
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{:?}", changed.body);
    assert_eq!(app.get("/api/auth/me", &anns).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &allowed.access_token).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &bobs).await.status, StatusCode::OK, "Bob's own app stays");
    assert_eq!(app.get("/api/auth/me", &ann_session).await.status, StatusCode::OK, "the session that changed it stays");
    let exchange = pley::auth::grants::RefreshInput {
        token: refresh,
        client_id: common::ANDROID_APP.into(),
        scope: None,
        user_agent: None,
        ip: None,
    };
    assert!(matches!(app.state.auth.refresh(exchange).await, Err(pley::auth::grants::RefreshError::Invalid(_))));
}

#[tokio::test]
async fn an_apps_event_stream_ends_when_its_grant_does() {
    let app = TestApp::with_config(|config| config.event_recheck = Duration::from_millis(200)).await;
    app.setup_admin().await;
    let (token, _) = app.app_token(1, "library").await;
    let resp = app.api.clone().oneshot(bearer("GET", "/api/events", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let mut stream = resp.into_body().into_data_stream();
    // Open across several checks while the token works.
    let open = tokio::time::timeout(Duration::from_millis(700), async {
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
        }
    })
    .await;
    assert!(open.is_err(), "the stream ended while its token worked");
    assert!(app.state.auth.revoke_app_token(&token).await);
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
        }
    })
    .await;
    assert!(ended.is_ok(), "the stream outlived its grant");
}

#[tokio::test]
async fn jellyfin_session_files_are_its_tokens_while_they_work() {
    use pley::playback::{Method as Play, Session, TranscodeSpec};

    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let file = app.file_of(film).await;
    let (bob_session, bob) = app.user(&admin, "bob", json!({})).await;
    let (token, _) = app.app_token(bob, "play").await;

    // Playback an app starts is marked with its grant.
    let caps = json!({"containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"]});
    let body = json!({"item_id": film, "caps": caps, "mode": "direct"});
    let by_app = app.post("/api/playback", &token, body.clone()).await;
    let by_session = app.post("/api/playback", &admin, body).await;
    let session_of =
        |reply: &common::Reply| app.state.sessions.get(reply.body["session_id"].as_str().unwrap()).unwrap();
    let grant: i64 = sqlx::query_scalar("SELECT id FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(session_of(&by_app).app_grant, Some(grant));
    assert_eq!(session_of(&by_session).app_grant, None);

    // Transcode sessions, as the transcoder would have started them for a
    // compatibility token, an app or Pley's own player.
    let transcode = |user: i64, grant: Option<i64>, compatibility: Option<i64>| {
        let spec = TranscodeSpec { request: Default::default(), segment_count: 1, duration_ms: 1000 };
        let mut session =
            Session::new(user, "u".into(), None, film, file, Play::Transcode, 1000, Some(spec), Some("job".into()), 0);
        session.app_grant = grant;
        session.compatibility_token = compatibility;
        app.state.sessions.insert(session).id.clone()
    };
    app.compatibility_token(&bob_session, "play").await;
    let tool: i64 = sqlx::query_scalar("SELECT id FROM compatibility_tokens WHERE user_id = ?")
        .bind(bob)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let item = app.jf_item(film);
    let session_file = |session: String| {
        let req =
            Request::builder().uri(format!("/Videos/{item}/hls1/{session}/index.m3u8")).body(Body::empty()).unwrap();
        let jellyfin = app.jellyfin.clone();
        async move { jellyfin.oneshot(req).await.unwrap().status() }
    };
    let bobs = transcode(bob, None, Some(tool));
    // No transcoder runs here: it gets as far as asking it.
    assert_ne!(session_file(bobs.clone()).await, StatusCode::NOT_FOUND);
    assert_eq!(session_file(transcode(bob, Some(grant), None)).await, StatusCode::NOT_FOUND, "an app's session");
    assert_eq!(session_file(transcode(bob, None, None)).await, StatusCode::NOT_FOUND, "Pley's own player's");
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    assert_eq!(session_file(transcode(admin_id, None, Some(tool))).await, StatusCode::NOT_FOUND, "someone else's");
    sqlx::query("UPDATE users SET disabled = 1 WHERE id = ?").bind(bob).execute(&app.state.db.write).await.unwrap();
    assert_eq!(session_file(bobs.clone()).await, StatusCode::NOT_FOUND, "a disabled account's session");
    sqlx::query("UPDATE users SET disabled = 0 WHERE id = ?").bind(bob).execute(&app.state.db.write).await.unwrap();
    assert_ne!(session_file(bobs.clone()).await, StatusCode::NOT_FOUND);
    let revoked = app.delete(&format!("/api/me/compatibility-tokens/{tool}"), &bob_session).await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    assert_eq!(session_file(bobs).await, StatusCode::NOT_FOUND, "a revoked token's session");
}

// ------------------------------------------------------------------ the tables

#[tokio::test]
async fn the_apps_tables_come_with_an_upgrade() {
    use sqlx::migrate::Migrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};

    // A database of the version before apps: every migration below 0023.
    let dir = common::tempdir();
    let older = dir.path().join("older");
    std::fs::create_dir(&older).unwrap();
    for file in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap().flatten() {
        let name = file.file_name().to_string_lossy().into_owned();
        if name.as_str() < "0023" {
            std::fs::copy(file.path(), older.join(&name)).unwrap();
        }
    }
    let path = dir.path().join("pley.db");
    let pool =
        SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(true)).await.unwrap();
    Migrator::new(older.as_path()).await.unwrap().run(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at) VALUES (1, 'ann', 'x', 0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, last_seen_at, expires_at) VALUES ('s', 1, 0, 0, 9)",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let db = pley::db::Db::open(&path).await.unwrap();
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&db.read).await.unwrap();
    assert_eq!(sessions, 1, "kept");
    let auth = pley::auth::AuthService::new(db.clone());
    let issued = auth
        .issue(pley::auth::grants::NewGrant {
            user_id: 1,
            approved_by: 1,
            client_id: common::ANDROID_APP.into(),
            client_name: None,
            scopes: pley::auth::scopes::Scopes::parse("library").unwrap(),
            refresh: true,
            first_party: true,
            session_hash: None,
            user_agent: None,
            ip: None,
        })
        .await
        .unwrap();
    assert!(auth.validate_app(&issued.access_token).await.is_some());
    // Deleting the account deletes its grants and their tokens.
    sqlx::query("DELETE FROM users WHERE id = 1").execute(&db.write).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM app_grants) + (SELECT COUNT(*) FROM app_tokens)")
        .fetch_one(&db.read)
        .await
        .unwrap();
    assert_eq!(left, 0);
    db.close().await;

    // And a new database.
    let fresh = pley::db::Db::open(&dir.path().join("new.db")).await.unwrap();
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'app_%' ORDER BY name")
            .fetch_all(&fresh.read)
            .await
            .unwrap();
    assert_eq!(tables, ["app_grants", "app_tokens"]);
    fresh.close().await;
}

#[tokio::test]
async fn administration_refuses_app_tokens_even_on_a_route_apps_may_use() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    let (token, _) = app.app_token(1, "library play playlists profiles").await;
    // An administrators' handler on a route an app's scopes open.
    let router = axum::Router::new()
        .route("/api/libraries", axum::routing::get(|_: pley::auth::AdminUser| async { "administered" }))
        .with_state(app.state.clone());
    let resp = router.oneshot(bearer("GET", "/api/libraries", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn no_answer_an_app_reaches_carries_the_servers_secrets() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    app.library(&admin, "Movies", "movies", "movies").await;
    // Every key and password the server keeps, set as the web app sets them.
    let secrets = [
        "tmdb-secret-1111",
        "tvdb-secret-2222",
        "subtitles-key-3333",
        "subtitles-pass-4444",
        "radarr-secret-5555",
        "sonarr-secret-6666",
        "provider-pass-7777",
    ];
    let saved = app
        .put(
            "/api/settings",
            &admin,
            json!({
                "tmdb_api_key": secrets[0],
                "tvdb_api_key": secrets[1],
                "opensubtitles_api_key": secrets[2],
                "opensubtitles_username": "someone",
                "opensubtitles_password": secrets[3],
                "radarr_url": "http://localhost:7878",
                "radarr_api_key": secrets[4],
                "sonarr_url": "http://localhost:8989",
                "sonarr_api_key": secrets[5],
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.body);
    // A Live TV provider whose password is in its channels' addresses.
    let source: i64 = sqlx::query_scalar(
        "INSERT INTO live_sources (name, kind, url, username, password, created_at)
         VALUES ('Provider', 'xtream', 'http://provider.invalid', 'viewer', ?, 0) RETURNING id",
    )
    .bind(secrets[6])
    .fetch_one(&app.state.db.write)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO live_channels (source_id, key, name, stream_url, position)
         VALUES (?, '1', 'News', ?, 0)",
    )
    .bind(source)
    .bind(format!("http://provider.invalid/live/viewer/{}/1.ts", secrets[6]))
    .execute(&app.state.db.write)
    .await
    .unwrap();

    // The widest token an app can hold: pley's own, every scope, on the
    // administrator's account.
    let (token, _) = app.app_token(1, "library play playlists profiles requests approvals activity server").await;
    let mut read = 0;
    for (method, path, _) in APP_ROUTES.iter().filter(|(method, _, _)| *method == "GET") {
        let uri = concrete(path);
        let resp = app.api.clone().oneshot(bearer(method, &uri, &token)).await.unwrap();
        // The event stream and media don't end: what came in a moment is enough.
        let mut body = resp.into_body().into_data_stream();
        let mut text = Vec::new();
        while let Ok(Some(Ok(chunk))) = tokio::time::timeout(Duration::from_millis(200), body.next()).await {
            text.extend_from_slice(&chunk);
            if text.len() > 4 << 20 {
                break;
            }
        }
        let text = String::from_utf8_lossy(&text);
        for secret in secrets {
            assert!(!text.contains(secret), "{method} {uri} carries {secret}: {text}");
        }
        read += 1;
    }
    assert!(read > 50, "{read} routes read");
    // The lineup is read, with the channel, and without its address.
    let lineup = app.get("/api/live/lineup/page", &token).await;
    assert_eq!(lineup.body["total"], json!(1), "{:?}", lineup.body);
}
