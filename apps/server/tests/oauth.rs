//! Apps sign in on pley's own page (src/api/oauth.rs): the metadata, the
//! consent API behind /oauth/authorize, codes with PKCE, the token and
//! revocation endpoints, the apps lists, and the switch for apps by other
//! publishers. pley's own apps sign in without any document being fetched.

mod common;

use std::collections::HashMap;

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use common::{ANDROID_APP, DESKTOP_APP, HOST, IOS_APP, ISSUER, MACOS_APP, Reply, TestApp, encode};
use pley::oauth::fetch::Source;

/// RFC 7636 appendix B.
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const ANDROID_BACK: &str = "app.pley.android:/oauth/callback";
const DESKTOP_BACK: &str = "http://127.0.0.1:53111/oauth/callback";
const APPLE_BACK: &str = "app.pley.apple:/oauth/callback";
const OTHER_APP: &str = "https://apps.example.com/pley.json";
const OTHER_BACK: &str = "https://apps.example.com/callback?from=pley";
const NOT_FROM_PLEY: &str = "this page must be opened from Pley itself";

/// The parameters of an app's authorization request.
fn request_params(client_id: &str, redirect_uri: &str) -> Vec<(String, String)> {
    [
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "library play"),
        ("state", "xyz"),
        ("code_challenge", CHALLENGE),
        ("code_challenge_method", "S256"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

/// A parameter set to a value, or left out (`None`).
type Change<'a> = (&'a str, Option<&'a str>);

/// `params` with `changes` applied: a new value, or `None` to leave one out
/// (names not there yet are added).
fn with(mut params: Vec<(String, String)>, changes: &[Change]) -> Vec<(String, String)> {
    for (name, value) in changes {
        match (params.iter().position(|(n, _)| n == name), value) {
            (Some(at), Some(value)) => params[at].1 = (*value).to_owned(),
            (Some(at), None) => {
                params.remove(at);
            }
            (None, Some(value)) => params.push(((*name).to_owned(), (*value).to_owned())),
            (None, None) => {}
        }
    }
    params
}

fn query(params: &[(String, String)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(params).finish()
}

/// What the authorization page hands to the server.
async fn start(app: &TestApp, cookie: &str, query: &str) -> Reply {
    app.browser_call(Method::POST, "/api/oauth/requests", cookie, Some(json!({"query": query})), &[]).await
}

/// A request that shows consent: its id.
async fn consent(app: &TestApp, cookie: &str, query: &str) -> String {
    let started = start(app, cookie, query).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    assert_eq!(started.body["status"], "consent", "{}", started.text());
    started.body["id"].as_str().unwrap().to_owned()
}

async fn decide(app: &TestApp, cookie: &str, id: &str, allow: bool) -> Reply {
    app.browser_call(Method::POST, &format!("/api/oauth/requests/{id}"), cookie, Some(json!({"allow": allow})), &[])
        .await
}

async fn back(app: &TestApp, cookie: &str, id: &str) -> Reply {
    app.browser_call(Method::GET, &format!("/api/oauth/requests/{id}/continue"), cookie, None, &[]).await
}

fn location(reply: &Reply) -> String {
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.text());
    reply.headers[header::LOCATION].to_str().unwrap().to_owned()
}

/// The query parameters of an address.
fn params_of(address: &str) -> HashMap<String, String> {
    url::Url::parse(address).unwrap().query_pairs().into_owned().collect()
}

/// The person signed in with `cookie` allows the app of `query`: the
/// address the browser goes back to.
async fn allow(app: &TestApp, cookie: &str, query: &str) -> String {
    let id = consent(app, cookie, query).await;
    let decided = decide(app, cookie, &id, true).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    assert_eq!(decided.body["continue_url"], format!("/api/oauth/requests/{id}/continue"));
    location(&back(app, cookie, &id).await)
}

/// The code of an allowed request.
async fn code_for(app: &TestApp, cookie: &str, client_id: &str, redirect_uri: &str) -> String {
    let address = allow(app, cookie, &query(&request_params(client_id, redirect_uri))).await;
    params_of(&address)["code"].clone()
}

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish()
}

async fn exchange(app: &TestApp, code: &str, client_id: &str, redirect_uri: &str, verifier: &str) -> Reply {
    let body = form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ]);
    app.form_post("/api/oauth/token", &body, &[]).await
}

async fn refresh(app: &TestApp, token: &str, client_id: &str) -> Reply {
    let body = form(&[("grant_type", "refresh_token"), ("refresh_token", token), ("client_id", client_id)]);
    app.form_post("/api/oauth/token", &body, &[]).await
}

/// `(access token, refresh token)` of a token answer.
fn pair(reply: &Reply) -> (String, String) {
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    (reply.body["access_token"].as_str().unwrap().to_owned(), reply.body["refresh_token"].as_str().unwrap().to_owned())
}

fn oauth_error(reply: &Reply) -> (StatusCode, &str) {
    (reply.status, reply.body["error"].as_str().unwrap_or_default())
}

/// Token answers are never cached (RFC 6749 §5.1).
fn assert_not_cached(reply: &Reply) {
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store", "{}", reply.text());
    assert_eq!(reply.headers[header::PRAGMA], "no-cache");
}

async fn metadata(app: &TestApp, extra: &[(&str, &str)]) -> Reply {
    app.call(Method::GET, "/.well-known/oauth-authorization-server", None, None, extra).await
}

/// A pley whose web app is a stand-in page, with an administrator.
async fn with_web_app() -> (TestApp, tempfile::TempDir, String) {
    let web = tempfile::tempdir().unwrap();
    std::fs::write(web.path().join("index.html"), "<!doctype html><title>pley</title>").unwrap();
    let dir = web.path().to_path_buf();
    let app = TestApp::with_config(move |config| config.web_dir = Some(dir)).await;
    let admin = app.setup_admin().await;
    (app, web, admin)
}

// ------------------------------------------------------------------ pley's own apps

#[tokio::test]
async fn pleys_android_app_signs_in_without_internet() {
    let app = TestApp::new().await;
    assert_eq!(app.state.config.client_documents, Source::Off, "no document can be fetched");
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    app.library(&admin, "Movies", "movies", "movies").await;

    // The app finds the endpoints.
    let found = metadata(&app, &[("host", HOST)]).await;
    assert_eq!(found.status, StatusCode::OK, "{}", found.text());
    assert_eq!(found.body["issuer"], ISSUER);
    assert_eq!(found.body["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(found.body["authorization_endpoint"], format!("{ISSUER}/oauth/authorize"));

    // The page hands its query over; the person sees pley's own app.
    let started = start(&app, &admin, &query(&request_params(ANDROID_APP, ANDROID_BACK))).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let body = &started.body;
    assert_eq!(body["status"], "consent");
    assert_eq!(
        body["client"],
        json!({"id": ANDROID_APP, "host": "pley-media-server.github.io", "name": "Pley for Android", "first_party": true})
    );
    assert_eq!(body["redirect"], json!({"kind": "app", "to": "app.pley.android"}));
    assert_eq!(body["scopes"], json!(["library", "play"]));
    let expires_at = body["expires_at"].as_i64().unwrap();
    assert!((pley::util::now() + 590..=pley::util::now() + 600).contains(&expires_at));
    let id = body["id"].as_str().unwrap().to_owned();

    // Allowed: back to the app with code, state and iss.
    assert_eq!(decide(&app, &admin, &id, true).await.status, StatusCode::OK);
    let went = back(&app, &admin, &id).await;
    assert_eq!(went.headers[header::CACHE_CONTROL], "no-store");
    let address = location(&went);
    assert!(address.starts_with("app.pley.android:/oauth/callback?code="), "{address}");
    let answer = params_of(&address);
    assert_eq!((answer["state"].as_str(), answer["iss"].as_str()), ("xyz", ISSUER));
    assert_eq!(answer.len(), 3);
    // The way back repeats while the code is unused (a second click).
    assert_eq!(location(&back(&app, &admin, &id).await), address);

    // The app exchanges the code.
    let tokens = exchange(&app, &answer["code"], ANDROID_APP, ANDROID_BACK, VERIFIER).await;
    assert_not_cached(&tokens);
    assert_eq!(tokens.body["token_type"], "Bearer");
    assert_eq!(tokens.body["expires_in"], 3600);
    assert_eq!(tokens.body["scope"], "library play");
    let (access, refresh_token) = pair(&tokens);
    assert!(access.starts_with("pley_at_") && refresh_token.starts_with("pley_rt_"));
    // Used: the way back is over.
    assert_eq!(back(&app, &admin, &id).await.status, StatusCode::NOT_FOUND);

    let libraries = app.get("/api/libraries", &access).await;
    assert_eq!(libraries.status, StatusCode::OK, "{}", libraries.text());
    assert_eq!(libraries.body.as_array().unwrap().len(), 1);
    let me = app.get("/api/auth/me", &access).await;
    assert_eq!(
        (&me.body["username"], &me.body["is_admin"], &me.body["scope"]),
        (&json!("admin"), &json!(false), &json!("library play"))
    );

    // Refresh: a new pair; asking for less changes nothing, for more is refused.
    let wider = form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", ANDROID_APP),
        ("scope", "library playlists"),
    ]);
    assert_eq!(
        oauth_error(&app.form_post("/api/oauth/token", &wider, &[]).await),
        (StatusCode::BAD_REQUEST, "invalid_scope")
    );
    let narrower = form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", ANDROID_APP),
        ("scope", "library"),
    ]);
    let renewed = app.form_post("/api/oauth/token", &narrower, &[]).await;
    assert_not_cached(&renewed);
    assert_eq!(renewed.body["scope"], "library play");
    let (access2, refresh2) = pair(&renewed);
    assert_ne!((&access2, &refresh2), (&access, &refresh_token));
    assert_eq!(app.get("/api/libraries", &access2).await.status, StatusCode::OK);

    // The old refresh token again: someone has a copy, so the access ends.
    let reused = refresh(&app, &refresh_token, ANDROID_APP).await;
    assert_eq!(oauth_error(&reused), (StatusCode::BAD_REQUEST, "invalid_grant"));
    assert_not_cached(&reused);
    assert_eq!(app.get("/api/libraries", &access2).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(oauth_error(&refresh(&app, &refresh2, ANDROID_APP).await).1, "invalid_grant");
}

#[tokio::test]
async fn the_desktop_app_signs_in_through_any_loopback_port() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let started = start(&app, &admin, &query(&request_params(DESKTOP_APP, DESKTOP_BACK))).await;
    assert_eq!(started.body["redirect"], json!({"kind": "loopback", "to": "127.0.0.1"}), "{}", started.text());
    assert_eq!(started.body["client"]["name"], "Pley for desktop");

    let address = allow(&app, &admin, &query(&request_params(DESKTOP_APP, DESKTOP_BACK))).await;
    assert!(address.starts_with("http://127.0.0.1:53111/oauth/callback?code="), "{address}");
    let code = &params_of(&address)["code"];
    let (access, _) = pair(&exchange(&app, code, DESKTOP_APP, DESKTOP_BACK, VERIFIER).await);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK);

    // IPv6 too, on its own port.
    let address = allow(&app, &admin, &query(&request_params(DESKTOP_APP, "http://[::1]:61000/oauth/callback"))).await;
    assert!(address.starts_with("http://[::1]:61000/oauth/callback?code="), "{address}");
    // Never "localhost", another path, or the other address family's registration.
    for refused in
        ["http://localhost:53111/oauth/callback", "http://127.0.0.1:53111/other", "http://127.0.0.2/oauth/callback"]
    {
        let reply = start(&app, &admin, &query(&request_params(DESKTOP_APP, refused))).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{refused}");
    }
}

#[tokio::test]
async fn the_apple_app_signs_in_through_its_own_scheme_on_iphone_ipad_and_mac() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (client_id, name) in [(IOS_APP, "Pley for iPhone and iPad"), (MACOS_APP, "Pley for Mac")] {
        let started = start(&app, &admin, &query(&request_params(client_id, APPLE_BACK))).await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.text());
        assert_eq!(started.body["redirect"], json!({"kind": "app", "to": "app.pley.apple"}), "{}", started.text());
        assert_eq!(
            started.body["client"],
            json!({"id": client_id, "host": "pley-media-server.github.io", "name": name, "first_party": true})
        );

        let address = allow(&app, &admin, &query(&request_params(client_id, APPLE_BACK))).await;
        assert!(address.starts_with("app.pley.apple:/oauth/callback?code="), "{address}");
        let code = &params_of(&address)["code"];
        let (access, refresh_token) = pair(&exchange(&app, code, client_id, APPLE_BACK, VERIFIER).await);
        assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK);
        pair(&refresh(&app, &refresh_token, client_id).await);

        // Only the address it registered: not another app's, another path,
        // or a loopback port as the desktop app's.
        for refused in [ANDROID_BACK, "app.pley.apple:/other", DESKTOP_BACK] {
            let reply = start(&app, &admin, &query(&request_params(client_id, refused))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{name}: {refused}");
        }
    }
    // Android's address isn't the Apple app's either.
    let reply = start(&app, &admin, &query(&request_params(ANDROID_APP, APPLE_BACK))).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);

    // One return address for both: a code given to Pley for iPhone and
    // iPad isn't Pley for Mac's to redeem.
    let request = query(&request_params(IOS_APP, APPLE_BACK));
    assert_eq!(start(&app, &admin, &request).await.status, StatusCode::OK);
    let address = allow(&app, &admin, &request).await;
    let crossed = exchange(&app, &params_of(&address)["code"], MACOS_APP, APPLE_BACK, VERIFIER).await;
    assert_eq!(oauth_error(&crossed), (StatusCode::BAD_REQUEST, "invalid_grant"));
}

#[tokio::test]
async fn two_step_accounts_take_their_code_before_any_app_is_allowed() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let (secret, _) = app.turn_on_two_step(&bob, "test-pass-1").await;
    let request = query(&request_params(ANDROID_APP, ANDROID_BACK));

    // Signed out, the page can't even ask.
    assert_eq!(start(&app, "", &request).await.status, StatusCode::UNAUTHORIZED);
    // The sign-in page as a browser uses it: the password alone isn't enough.
    let sign_in = |code: Option<String>| {
        let mut body = json!({"username": "bob", "password": "test-pass-1"});
        if let Some(code) = code {
            body["code"] = json!(code);
        }
        app.call(Method::POST, "/api/auth/login", None, Some(body), &[("origin", ISSUER)])
    };
    let half = sign_in(None).await;
    assert_eq!((half.status, &half.body["code"]), (StatusCode::UNAUTHORIZED, &json!("two_step")));
    assert!(!half.headers.contains_key(header::SET_COOKIE));
    let signed_in = sign_in(Some(pley::two_step::code_at(&secret, pley::util::now() + 30))).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let cookie = signed_in.headers[header::SET_COOKIE].to_str().unwrap();
    let cookie = cookie.strip_prefix("pley_token=").unwrap().split(';').next().unwrap().to_owned();

    let address = allow(&app, &cookie, &request).await;
    let (access, _) = pair(&exchange(&app, &params_of(&address)["code"], ANDROID_APP, ANDROID_BACK, VERIFIER).await);
    assert_eq!(app.get("/api/auth/me", &access).await.body["username"], "bob");
}

// ------------------------------------------------------------------ metadata

#[tokio::test]
async fn metadata_names_the_address_it_was_asked_at() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let at = metadata(&app, &[("host", "PLEY.Local:7539")]).await;
    assert_eq!(at.status, StatusCode::OK, "{}", at.text());
    assert_eq!(at.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        at.body,
        json!({
            "issuer": "http://pley.local:7539",
            "authorization_endpoint": "http://pley.local:7539/oauth/authorize",
            "token_endpoint": "http://pley.local:7539/api/oauth/token",
            "revocation_endpoint": "http://pley.local:7539/api/oauth/revoke",
            "device_authorization_endpoint": "http://pley.local:7539/api/oauth/device",
            "response_types_supported": ["code"],
            "response_modes_supported": ["query"],
            "grant_types_supported": [
                "authorization_code",
                "refresh_token",
                "urn:pley:params:oauth:grant-type:switch-profile",
                "urn:ietf:params:oauth:grant-type:device_code"
            ],
            "token_endpoint_auth_methods_supported": ["none"],
            "revocation_endpoint_auth_methods_supported": ["none"],
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": ["library", "play", "playlists", "profiles", "requests", "approvals"],
            "authorization_response_iss_parameter_supported": true,
            "client_id_metadata_document_supported": false,
            "service_documentation": "https://github.com/pley-media-server/pley/blob/master/docs/API.md",
        })
    );
    // The default port goes; a proxy's https is believed.
    assert_eq!(metadata(&app, &[("host", "pley.local:80")]).await.body["issuer"], "http://pley.local");
    let proxied = metadata(&app, &[("host", "pley.example.com"), ("x-forwarded-proto", "https")]).await;
    assert_eq!(proxied.body["issuer"], "https://pley.example.com");
    assert_eq!(proxied.body["token_endpoint"], "https://pley.example.com/api/oauth/token");
    // No Host, or one that isn't a host.
    assert_eq!(metadata(&app, &[]).await.status, StatusCode::BAD_REQUEST);
    for bad in ["me@pley.local", "pley.local/x"] {
        assert_eq!(metadata(&app, &[("host", bad)]).await.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // It follows the setting.
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": true})).await.status, StatusCode::OK);
    assert_eq!(metadata(&app, &[("host", HOST)]).await.body["client_id_metadata_document_supported"], true);
}

#[tokio::test]
async fn other_well_known_documents_are_404() {
    let (app, _web, _) = with_web_app().await;
    // The web app answers its pages...
    let page = app.call(Method::GET, "/library/1", None, None, &[]).await;
    assert!(page.status == StatusCode::OK && page.text().contains("<title>pley</title>"));
    // ...but not documents apps look for, which would look like answers.
    for path in ["/.well-known/openid-configuration", "/.well-known/oauth-protected-resource", "/.well-known/a/b"] {
        let reply = app.call(Method::GET, path, None, None, &[("host", HOST)]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(reply.body, json!({"code": "not_found", "message": "no such document"}), "{path}");
    }
    // The metadata comes before the web app.
    let found = metadata(&app, &[("host", HOST)]).await;
    assert_eq!((found.status, &found.body["issuer"]), (StatusCode::OK, &json!(ISSUER)));
}

// ------------------------------------------------------------------ the consent API

#[tokio::test]
async fn bad_clients_and_return_addresses_are_never_redirected_to() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let base = request_params(ANDROID_APP, ANDROID_BACK);
    let mut cases: Vec<(&str, String)> = vec![
        ("unknown client", query(&with(base.clone(), &[("client_id", Some(OTHER_APP))]))),
        ("no client", query(&with(base.clone(), &[("client_id", None)]))),
        ("no return address", query(&with(base.clone(), &[("redirect_uri", None)]))),
        ("unregistered", query(&with(base.clone(), &[("redirect_uri", Some("app.pley.android:/other"))]))),
        ("another app's", query(&with(base.clone(), &[("redirect_uri", Some("com.example.apps:/oauth/callback"))]))),
        ("not canonical", query(&with(base.clone(), &[("redirect_uri", Some("App.pley.android:/oauth/callback"))]))),
        (
            "dot segments",
            query(&with(base.clone(), &[("redirect_uri", Some("app.pley.android:/x/../oauth/callback"))])),
        ),
        (
            "too long",
            query(&with(base.clone(), &[("redirect_uri", Some(&format!("{ANDROID_BACK}{}", "a".repeat(512))))])),
        ),
        ("9 KiB", query(&with(base.clone(), &[("pad", Some(&"a".repeat(9 * 1024)))]))),
    ];
    let plain = query(&base);
    cases.push(("client twice", format!("{plain}&client_id={}", encode(ANDROID_APP))));
    cases.push(("state twice", format!("{plain}&state=abc")));
    for (case, request) in cases {
        let reply = start(&app, &admin, &request).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{case}: {}", reply.text());
        assert_eq!(reply.body["code"], "bad_request", "{case}");
        assert!(reply.body["message"].as_str().unwrap().contains(": "), "{case}: a next step");
        assert!(!reply.headers.contains_key(header::LOCATION), "{case}");
        assert!(!reply.mentions("back_url") && !reply.mentions("continue"), "{case}");
    }
    let unlisted =
        start(&app, &admin, &query(&with(base.clone(), &[("redirect_uri", Some("app.pley.android:/x"))]))).await;
    assert_eq!(unlisted.body["message"], "this app didn't list that return address: ask the app's publisher to fix it");
    let other = start(&app, &admin, &query(&with(base, &[("client_id", Some(OTHER_APP))]))).await;
    assert!(other.body["message"].as_str().unwrap().starts_with("only Pley's own apps can sign in to this server"));
}

#[tokio::test]
async fn other_mistakes_go_back_to_the_app_only_when_asked() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let base = request_params(ANDROID_APP, ANDROID_BACK);
    let plain_challenge = "a".repeat(43);
    let long_state = "s".repeat(1025);
    let cases: Vec<(&str, Vec<Change>)> = vec![
        ("unsupported_response_type", vec![("response_type", Some("token"))]),
        ("invalid_request", vec![("response_type", None)]),
        ("invalid_request", vec![("code_challenge_method", Some("plain")), ("code_challenge", Some(&plain_challenge))]),
        ("invalid_request", vec![("code_challenge", None)]),
        ("invalid_request", vec![("code_challenge", None), ("code_challenge_method", None)]),
        ("invalid_request", vec![("code_challenge", Some(&CHALLENGE[1..]))]),
        ("invalid_request", vec![("response_mode", Some("fragment"))]),
        ("invalid_request", vec![("request_uri", Some("urn:example:request"))]),
        ("invalid_request", vec![("request", Some("eyJhbGciOiJub25lIn0.e30."))]),
        ("invalid_scope", vec![("scope", Some("openid"))]),
        ("invalid_scope", vec![("scope", Some("library admin"))]),
        ("invalid_target", vec![("resource", Some("https://elsewhere.example.com"))]),
        ("invalid_request", vec![("state", Some(&long_state))]),
    ];
    for (error, changes) in cases {
        let changed = with(base.clone(), &changes);
        let reply = start(&app, &admin, &query(&changed)).await;
        assert_eq!(reply.status, StatusCode::OK, "{error} {changes:?}: {}", reply.text());
        assert!(!reply.headers.contains_key(header::LOCATION));
        let body = &reply.body;
        assert_eq!((&body["status"], &body["error"]), (&json!("error"), &json!(error)), "{changes:?}");
        assert_eq!(body["client"]["id"], ANDROID_APP);
        // Where the click goes is shown before it (RFC 9700 §4.11.2).
        assert_eq!(body["redirect"], json!({"kind": "app", "to": "app.pley.android"}));
        let message = body["message"].as_str().unwrap();
        assert!(message.is_ascii() && !message.contains('"') && !message.contains('\\'), "{message}");
        let back_url = body["back_url"].as_str().unwrap();
        let id = back_url.strip_prefix("/api/oauth/requests/").unwrap().strip_suffix("/continue").unwrap();
        // Nothing to allow.
        assert_eq!(decide(&app, &admin, id, true).await.status, StatusCode::CONFLICT);
        // Only a click goes back, with the error, state and iss.
        let address = location(&app.browser_call(Method::GET, back_url, &admin, None, &[]).await);
        assert!(address.starts_with(&format!("{ANDROID_BACK}?error={error}&error_description=")), "{address}");
        let answer = params_of(&address);
        assert_eq!(answer["error_description"], message);
        assert_eq!(answer["iss"], ISSUER);
        let state = changed.iter().find(|(n, _)| n == "state").map(|(_, v)| v.clone()).unwrap();
        assert_eq!(answer["state"], state);
    }
    // Unknown parameters (OpenID Connect's among them) are ignored, as are
    // an empty scope (library) and this server as the resource.
    let ignored = with(
        base,
        &[
            ("prompt", Some("login")),
            ("nonce", Some("n")),
            ("scope", Some("")),
            ("resource", Some(&format!("{ISSUER}/"))),
        ],
    );
    let reply = start(&app, &admin, &query(&ignored)).await;
    assert_eq!(
        (&reply.body["status"], &reply.body["scopes"]),
        (&json!("consent"), &json!(["library"])),
        "{}",
        reply.text()
    );
    // Parameters sent without a value are as if left out, and ones pley
    // doesn't read may come twice (OAuth 2.1 §3.1-3.2).
    let plain = query(&request_params(ANDROID_APP, ANDROID_BACK));
    for extra in ["prompt=&prompt=", "foo=1&foo=2", "state=", "request_uri=&request="] {
        let reply = start(&app, &admin, &format!("{plain}&{extra}")).await;
        assert_eq!(reply.body["status"], "consent", "{extra}: {}", reply.text());
    }
}

#[tokio::test]
async fn the_consent_api_takes_the_cookie_of_pleys_own_page_only() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let request = query(&request_params(ANDROID_APP, ANDROID_BACK));
    let body = json!({"query": request});
    let page = [("host", HOST), ("origin", ISSUER), ("sec-fetch-site", "same-origin")];

    // Only the cookie: a session in any other place is refused...
    let bearer = app.call(Method::POST, "/api/oauth/requests", Some(&admin), Some(body.clone()), &page).await;
    assert_eq!(bearer.status, StatusCode::UNAUTHORIZED);
    let mut with_header = page.to_vec();
    with_header.push(("x-pley-token", &admin));
    let header_token = app.call(Method::POST, "/api/oauth/requests", None, Some(body.clone()), &with_header).await;
    assert_eq!(header_token.status, StatusCode::UNAUTHORIZED);
    let in_url = format!("/api/oauth/requests?token={admin}");
    assert_eq!(app.call(Method::POST, &in_url, None, Some(body.clone()), &page).await.status, StatusCode::UNAUTHORIZED);
    // ...and so is an app's token, anywhere.
    let (access, _) = app.app_token(1, "library play playlists").await;
    assert_eq!(start(&app, &access, &request).await.status, StatusCode::UNAUTHORIZED);
    let app_bearer = app.call(Method::POST, "/api/oauth/requests", Some(&access), Some(body.clone()), &page).await;
    assert_eq!(app_bearer.status, StatusCode::UNAUTHORIZED);

    // Only from pley's own page.
    let call = |extra: Vec<(&'static str, &'static str)>| {
        let body = body.clone();
        let admin = admin.clone();
        let app = &app;
        async move { app.browser_call(Method::POST, "/api/oauth/requests", &admin, Some(body), &extra).await }
    };
    for (case, extra) in [
        ("no Origin", vec![("origin", "")]),
        ("Origin null", vec![("origin", "null")]),
        ("another site", vec![("origin", "http://evil.example")]),
        ("another port", vec![("origin", "http://pley.local:7540")]),
        ("no Host", vec![("host", "")]),
        ("cross-site", vec![("sec-fetch-site", "cross-site")]),
        ("same-site", vec![("sec-fetch-site", "same-site")]),
        ("Sec-Fetch-Site none on a POST", vec![("sec-fetch-site", "none")]),
    ] {
        let reply = call(extra).await;
        assert_eq!((reply.status, &reply.body["message"]), (StatusCode::FORBIDDEN, &json!(NOT_FROM_PLEY)), "{case}");
    }
    // Browsers that don't send Sec-Fetch-Site still send Origin.
    assert_eq!(call(vec![("sec-fetch-site", "")]).await.status, StatusCode::OK);

    // A request belongs to the person and the session that opened it.
    let id = consent(&app, &admin, &request).await;
    let other_session = app.login("admin", "secret-one").await;
    assert_eq!(decide(&app, &bob, &id, true).await.status, StatusCode::NOT_FOUND);
    let elsewhere = decide(&app, &other_session, &id, true).await;
    assert_eq!(
        (elsewhere.status, &elsewhere.body["message"]),
        (StatusCode::NOT_FOUND, &json!("this request is over: start again from the app or the TV"))
    );
    assert_eq!(decide(&app, &admin, "made-up", true).await.status, StatusCode::NOT_FOUND);
    let first = decide(&app, &admin, &id, true).await;
    assert_eq!(first.status, StatusCode::OK);
    let again = decide(&app, &admin, &id, true).await;
    assert_eq!((again.status, &again.body), (StatusCode::OK, &first.body), "the same decision, the same answer");
    assert_eq!(decide(&app, &admin, &id, false).await.status, StatusCode::CONFLICT);
    // Deciding from elsewhere is refused like starting.
    let foreign = app
        .browser_call(
            Method::POST,
            &format!("/api/oauth/requests/{id}"),
            &admin,
            Some(json!({"allow": true})),
            &[("origin", "http://evil.example")],
        )
        .await;
    assert_eq!(foreign.status, StatusCode::FORBIDDEN);

    // Going back: the same session, from the page or typed, never from elsewhere.
    let path = format!("/api/oauth/requests/{id}/continue");
    assert_eq!(back(&app, &other_session, &id).await.status, StatusCode::NOT_FOUND);
    assert_eq!(back(&app, &bob, &id).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.call(Method::GET, &path, Some(&admin), None, &[("host", HOST)]).await.status,
        StatusCode::UNAUTHORIZED
    );
    for site in ["cross-site", "same-site"] {
        let reply = app.browser_call(Method::GET, &path, &admin, None, &[("sec-fetch-site", site)]).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{site}");
    }
    for site in ["none", ""] {
        let reply =
            app.browser_call(Method::GET, &path, &admin, None, &[("sec-fetch-site", site), ("origin", "")]).await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER, "{site:?}");
    }
    // No CORS anywhere.
    assert!(
        !app.browser_call(Method::GET, &path, &admin, None, &[])
            .await
            .headers
            .contains_key("access-control-allow-origin")
    );
}

#[tokio::test]
async fn deny_sends_access_denied_with_state_and_iss() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let request = query(&request_params(DESKTOP_APP, DESKTOP_BACK));
    let id = consent(&app, &admin, &request).await;
    let denied = decide(&app, &admin, &id, false).await;
    assert_eq!(denied.status, StatusCode::OK);
    assert_eq!(decide(&app, &admin, &id, false).await.body, denied.body);
    assert_eq!(decide(&app, &admin, &id, true).await.status, StatusCode::CONFLICT, "no code after a refusal");
    let address = location(&back(&app, &admin, &id).await);
    assert_eq!(address, format!("{DESKTOP_BACK}?error=access_denied&state=xyz&iss={}", encode(ISSUER)));
    // It repeats, for a minute.
    assert_eq!(location(&back(&app, &admin, &id).await), address);
}

// ------------------------------------------------------------------ codes and tokens

#[tokio::test]
async fn codes_work_once_and_only_with_their_verifier() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let code = code_for(&app, &admin, ANDROID_APP, ANDROID_BACK).await;
    let wrong_verifier = "x".repeat(43);

    // None of these uses the code up.
    let wrong = exchange(&app, &code, ANDROID_APP, ANDROID_BACK, &wrong_verifier).await;
    assert_eq!(oauth_error(&wrong), (StatusCode::BAD_REQUEST, "invalid_grant"));
    assert_not_cached(&wrong);
    let no_verifier = form(&[("grant_type", "authorization_code"), ("code", &code), ("client_id", ANDROID_APP)]);
    assert_eq!(oauth_error(&app.form_post("/api/oauth/token", &no_verifier, &[]).await).1, "invalid_request");
    let short = exchange(&app, &code, ANDROID_APP, ANDROID_BACK, &VERIFIER[1..]).await;
    assert_eq!(oauth_error(&short).1, "invalid_request");
    assert_eq!(oauth_error(&exchange(&app, &code, DESKTOP_APP, ANDROID_BACK, VERIFIER).await).1, "invalid_grant");
    let elsewhere = exchange(&app, &code, ANDROID_APP, "app.pley.android:/other", VERIFIER).await;
    assert_eq!(oauth_error(&elsewhere).1, "invalid_grant");
    let fields = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("client_id", ANDROID_APP),
        ("code_verifier", VERIFIER),
    ];
    let as_json = app
        .call(
            Method::POST,
            "/api/oauth/token",
            None,
            Some(json!(fields.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>())),
            &[("host", HOST)],
        )
        .await;
    assert_eq!(oauth_error(&as_json), (StatusCode::BAD_REQUEST, "invalid_request"));
    let twice = format!("{}&code={}", form(&fields), encode(&code));
    assert_eq!(oauth_error(&app.form_post("/api/oauth/token", &twice, &[]).await).1, "invalid_request");
    let other_resource = format!("{}&resource={}", form(&fields), encode("https://elsewhere.example.com"));
    assert_eq!(oauth_error(&app.form_post("/api/oauth/token", &other_resource, &[]).await).1, "invalid_target");
    // No secrets: every app is public.
    let secret = format!("{}&client_secret=s3cret", form(&fields));
    let reply = app.form_post("/api/oauth/token", &secret, &[]).await;
    assert_eq!(oauth_error(&reply), (StatusCode::BAD_REQUEST, "invalid_client"));
    assert!(!reply.headers.contains_key(header::WWW_AUTHENTICATE));
    let basic = app.form_post("/api/oauth/token", &form(&fields), &[("authorization", "Basic YTpi")]).await;
    assert_eq!(oauth_error(&basic), (StatusCode::UNAUTHORIZED, "invalid_client"));
    assert_eq!(basic.headers[header::WWW_AUTHENTICATE], r#"Basic realm="pley""#);
    assert_not_cached(&basic);
    let unknown_grant = form(&[("grant_type", "password"), ("username", "admin"), ("password", "secret-one")]);
    assert_eq!(oauth_error(&app.form_post("/api/oauth/token", &unknown_grant, &[]).await).1, "unsupported_grant_type");

    // Everything right: once. The return address may be left out, and an
    // empty secret, as some libraries send, is as if left out.
    let first = app.form_post("/api/oauth/token", &format!("{}&client_secret=", form(&fields)), &[]).await;
    let (access, refresh_token) = pair(&first);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK);
    // Again, with everything right: someone else has the code, so the
    // access the first exchange got ends.
    let replay = exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER).await;
    assert_eq!(oauth_error(&replay), (StatusCode::BAD_REQUEST, "invalid_grant"));
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(oauth_error(&refresh(&app, &refresh_token, ANDROID_APP).await).1, "invalid_grant");
    // A code nobody made.
    let made_up = exchange(&app, "made-up", ANDROID_APP, ANDROID_BACK, VERIFIER).await;
    assert_eq!(oauth_error(&made_up).1, "invalid_grant");
}

#[tokio::test]
async fn a_disabled_account_gets_no_tokens_for_its_code() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let code = code_for(&app, &bob, ANDROID_APP, ANDROID_BACK).await;
    let off = app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"disabled": true})).await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    let reply = exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER).await;
    assert_eq!(oauth_error(&reply), (StatusCode::BAD_REQUEST, "invalid_grant"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn revocation_ends_the_grant_and_answers_200_for_tokens_it_doesnt_know() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let revoke = |token: String, client_id: &'static str| {
        let app = &app;
        async move { app.form_post("/api/oauth/revoke", &form(&[("token", &token), ("client_id", client_id)]), &[]).await }
    };
    // Nothing to end: still 200, and nothing happens.
    let (access, refresh_token) = app.app_token(1, "library").await;
    for token in ["garbage".to_owned(), "pley_at_unknown".to_owned(), admin.clone()] {
        let reply = revoke(token, ANDROID_APP).await;
        assert_eq!((reply.status, reply.bytes.len()), (StatusCode::OK, 0));
        assert_not_cached(&reply);
    }
    // Another app's token is refused, and stays (RFC 7009 §2.1).
    for token in [access.clone(), refresh_token.clone()] {
        let reply = revoke(token, DESKTOP_APP).await;
        assert_eq!(oauth_error(&reply), (StatusCode::BAD_REQUEST, "unauthorized_client"));
        assert_not_cached(&reply);
    }
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK, "another app's token stays");
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK, "sessions aren't app tokens");
    // Malformed.
    let missing = app.form_post("/api/oauth/revoke", &form(&[("token", &access)]), &[]).await;
    assert_eq!(oauth_error(&missing), (StatusCode::BAD_REQUEST, "invalid_request"));
    let json_body = app
        .call(Method::POST, "/api/oauth/revoke", None, Some(json!({"token": access, "client_id": ANDROID_APP})), &[])
        .await;
    assert_eq!(oauth_error(&json_body).1, "invalid_request");

    // An access token ends its whole grant.
    assert_eq!(revoke(access.clone(), ANDROID_APP).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(oauth_error(&refresh(&app, &refresh_token, ANDROID_APP).await).1, "invalid_grant");

    // So does a refresh token, and a spent one.
    let (access, refresh_token) = app.app_token(1, "library").await;
    assert_eq!(revoke(refresh_token, ANDROID_APP).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
    let (_, spent) = app.app_token(1, "library").await;
    let (access, _) = pair(&refresh(&app, &spent, ANDROID_APP).await);
    assert_eq!(revoke(spent, ANDROID_APP).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn revocation_says_when_it_couldnt_revoke() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    let (access, _) = app.app_token(1, "library").await;
    // The database can't be written to: the app is told to try again, and
    // its token isn't thrown away for nothing (RFC 7009 §2.2.1).
    app.state.db.write.close().await;
    let reply = app.form_post("/api/oauth/revoke", &form(&[("token", &access), ("client_id", ANDROID_APP)]), &[]).await;
    assert_eq!(oauth_error(&reply), (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"));
    assert_eq!(reply.headers[header::RETRY_AFTER], "5");
    assert_not_cached(&reply);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(count, 1, "the grant is still there");
}

// ------------------------------------------------------------------ other publishers

fn other_document() -> String {
    json!({
        "client_id": OTHER_APP,
        "client_name": "Example \u{202E}Player",
        "redirect_uris": [OTHER_BACK],
        "grant_types": ["authorization_code", "refresh_token"],
        "scope": "library play",
    })
    .to_string()
}

#[tokio::test]
async fn apps_by_other_publishers_need_the_setting() {
    let documents = HashMap::from([(OTHER_APP.to_owned(), other_document())]);
    let app = TestApp::with_config(|config| config.client_documents = Source::Fixed(documents)).await;
    let admin = app.setup_admin().await;
    let request = query(&request_params(OTHER_APP, OTHER_BACK));

    // Off: refused before anything is read.
    let refused = start(&app, &admin, &request).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        refused.body["message"],
        "only Pley's own apps can sign in to this server: an administrator can allow apps by other publishers in \
         Settings → Server → Apps"
    );

    // On: the app is shown by its host, and its name only as what it calls itself.
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": true})).await.status, StatusCode::OK);
    let started = start(&app, &admin, &request).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    assert_eq!(
        started.body["client"],
        json!({"id": OTHER_APP, "host": "apps.example.com", "name": "Example Player", "first_party": false})
    );
    assert_eq!(started.body["redirect"], json!({"kind": "web", "to": "apps.example.com"}));
    // Never more than its description says.
    let more =
        start(&app, &admin, &query(&with(request_params(OTHER_APP, OTHER_BACK), &[("scope", Some("playlists"))])))
            .await;
    assert_eq!(more.body["error"], "invalid_scope");

    // Back to its address as it was sent, its own query kept.
    let address = allow(&app, &admin, &request).await;
    assert!(address.starts_with(&format!("{OTHER_BACK}&code=")), "{address}");
    let code = &params_of(&address)["code"];
    let (access, refresh_token) = pair(&exchange(&app, code, OTHER_APP, OTHER_BACK, VERIFIER).await);
    assert_eq!(app.get("/api/libraries", &access).await.status, StatusCode::OK);
    let (access, refresh_token) = pair(&refresh(&app, &refresh_token, OTHER_APP).await);
    let listed = app.get("/api/me/apps", &admin).await;
    assert_eq!(listed.body[0]["name"], "Example Player");
    assert_eq!(listed.body[0]["first_party"], false);
    // A code given while they were allowed...
    let late_code = code_for(&app, &admin, OTHER_APP, OTHER_BACK).await;

    // Off again: signed out at once, and nothing more is given.
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": false})).await.status, StatusCode::OK);
    let me = app.get("/api/auth/me", &access).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(me.headers[header::WWW_AUTHENTICATE], r#"Bearer error="invalid_token""#);
    assert_eq!(oauth_error(&refresh(&app, &refresh_token, OTHER_APP).await).1, "invalid_grant");
    assert_eq!(oauth_error(&exchange(&app, &late_code, OTHER_APP, OTHER_BACK, VERIFIER).await).1, "invalid_grant");
    assert_eq!(app.get("/api/me/apps", &admin).await.body, json!([]));
    assert_eq!(start(&app, &admin, &request).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(metadata(&app, &[("host", HOST)]).await.body["client_id_metadata_document_supported"], false);
}

#[tokio::test]
async fn turning_other_publishers_off_leaves_pleys_own_apps_signed_in() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (access, _) = app.app_token(1, "library").await;
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": true})).await.status, StatusCode::OK);
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": false})).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/settings", &admin).await.body["third_party_apps"], false);
}

// ------------------------------------------------------------------ apps lists

#[tokio::test]
async fn apps_rows_name_the_device_rather_than_the_app_again() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (agent, device) in [
        ("pleyAndroid/0.1 (Android 15)", "Android 15"),
        ("pleyAndroid/0.1 (Android 15; Pixel 8)", "Android 15; Pixel 8"),
        ("pleyAndroid/0.1", "Android"),
    ] {
        let code = code_for(&app, &admin, ANDROID_APP, ANDROID_BACK).await;
        let body = form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", ANDROID_APP),
            ("code_verifier", VERIFIER),
        ]);
        pair(&app.form_post("/api/oauth/token", &body, &[("user-agent", agent)]).await);
        let listed = app.get("/api/me/apps", &admin).await.body;
        assert_eq!(listed[0]["device"], device, "{agent}");
    }
}

#[tokio::test]
async fn apps_lists_show_and_end_grants() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let code = code_for(&app, &bob, ANDROID_APP, ANDROID_BACK).await;
    let tokens = exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER).await;
    let (bobs_app, _) = pair(&tokens);
    let (admins_app, _) = app.app_token(1, "library").await;

    let mine = app.get("/api/me/apps", &bob).await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.text());
    let list = mine.body.as_array().unwrap();
    assert_eq!(list.len(), 1);
    let grant = &list[0];
    let grant_id = grant["id"].as_i64().unwrap();
    assert_eq!(grant["client_id"], ANDROID_APP);
    assert_eq!((&grant["name"], &grant["first_party"]), (&json!("Pley for Android"), &json!(true)));
    assert_eq!(grant["host"], "pley-media-server.github.io");
    assert_eq!(grant["scopes"], json!(["library", "play"]));
    assert_eq!(grant["profile"], json!({"id": bob_id, "name": "bob"}));
    assert_eq!(grant["approved_by"], json!({"id": bob_id, "name": "bob"}));
    assert_eq!(grant["device"], "pley-android/1.0");
    assert!(grant["created_at"].as_i64().unwrap() > 0 && grant["last_used_at"].as_i64().unwrap() > 0);
    // The administrator sees it in the Users dialog, not as their own.
    assert_eq!(app.get(&format!("/api/users/{bob_id}/apps"), &admin).await.body, mine.body);
    let admins = app.get("/api/me/apps", &admin).await.body;
    assert_eq!(admins.as_array().unwrap().len(), 1);
    let admins_grant = admins[0]["id"].as_i64().unwrap();

    // Others' apps can't be signed out, nor seen by non-administrators.
    assert_eq!(app.delete(&format!("/api/me/apps/{admins_grant}"), &bob).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/users/1/apps", &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/users/1/apps/{admins_grant}"), &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/users/1/apps/{grant_id}"), &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/auth/me", &admins_app).await.status, StatusCode::OK);
    // Apps can't see or end the list.
    assert_eq!(app.get("/api/me/apps", &bobs_app).await.status, StatusCode::FORBIDDEN);

    // Signed out by its person...
    assert_eq!(app.delete(&format!("/api/me/apps/{grant_id}"), &bob).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &bobs_app).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/me/apps", &bob).await.body, json!([]));
    assert_eq!(app.delete(&format!("/api/me/apps/{grant_id}"), &bob).await.status, StatusCode::NOT_FOUND);
    // ...or by an administrator.
    let (bobs_app, _) = app.app_token(bob_id, "library").await;
    let grant_id = app.get("/api/me/apps", &bob).await.body[0]["id"].as_i64().unwrap();
    let ended = app.delete(&format!("/api/users/{bob_id}/apps/{grant_id}"), &admin).await;
    assert_eq!(ended.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &bobs_app).await.status, StatusCode::UNAUTHORIZED);
}

// ------------------------------------------------------------------ limits and headers

#[tokio::test]
async fn pending_requests_are_capped() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let request = query(&request_params(ANDROID_APP, ANDROID_BACK));
    // Ten each: an eleventh pushes the oldest out.
    let mut ids = Vec::new();
    for _ in 0..11 {
        ids.push(consent(&app, &admin, &request).await);
    }
    assert_eq!(decide(&app, &admin, &ids[0], true).await.status, StatusCode::NOT_FOUND);
    assert_eq!(decide(&app, &admin, &ids[1], true).await.status, StatusCode::OK);

    // A thousand in all.
    let snapshot = pley::oauth::AppSnapshot {
        id: ANDROID_APP.into(),
        host: "pley-media-server.github.io".into(),
        name: None,
        first_party: true,
        refresh: true,
    };
    for user_id in 1000..1099 {
        for _ in 0..10 {
            let pending = pley::oauth::pending::PendingRequest {
                user_id,
                session_hash: "elsewhere".into(),
                issuer: ISSUER.into(),
                app: snapshot.clone(),
                kind: pley::oauth::pending::Kind::Redirect(pley::oauth::pending::Redirect {
                    redirect_uri: ANDROID_BACK.into(),
                    state: None,
                    code_challenge: CHALLENGE.into(),
                }),
                scopes: pley::auth::scopes::Scopes::parse("library").unwrap(),
                error: None,
            };
            app.state.oauth.pending.create(pending).unwrap();
        }
    }
    let busy = start(&app, &bob, &request).await;
    assert_eq!(
        (busy.status, &busy.body["message"]),
        (StatusCode::SERVICE_UNAVAILABLE, &json!("too many sign-ins in progress: try again in a minute"))
    );
    // Someone with ten gets a new one in place of their oldest.
    assert_eq!(start(&app, &admin, &request).await.status, StatusCode::OK);
}

#[tokio::test]
async fn the_authorize_page_cannot_be_framed() {
    let (app, _web, _) = with_web_app().await;
    let page = app
        .call(
            Method::GET,
            &format!("/oauth/authorize?{}", query(&request_params(ANDROID_APP, ANDROID_BACK))),
            None,
            None,
            &[],
        )
        .await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.text().contains("<title>pley</title>"), "the web app's page");
    assert_eq!(page.headers[header::X_FRAME_OPTIONS], "DENY");
    let policy = page.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(policy.contains("frame-ancestors 'none'") && policy.contains("form-action 'self'"), "{policy}");
    assert_eq!(page.headers["cross-origin-opener-policy"], "same-origin");
    assert_eq!(page.headers[header::REFERRER_POLICY], "same-origin");
}

/// What the consent page reads is only ever JSON from pley: the server's
/// answers carry nothing to run.
#[tokio::test]
async fn consent_answers_are_json_without_cors() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let reply = start(&app, &admin, &query(&request_params(ANDROID_APP, ANDROID_BACK))).await;
    assert!(reply.headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("application/json"));
    assert!(!reply.headers.contains_key("access-control-allow-origin"));
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn a_late_link_back_to_the_app_leads_to_a_page_saying_it_is_over() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let id = consent(&app, &admin, &query(&request_params(ANDROID_APP, ANDROID_BACK))).await;
    assert_eq!(decide(&app, &admin, &id, true).await.status, StatusCode::OK);
    let back = format!("/api/oauth/requests/{id}/continue");
    let code = params_of(&location(&back_with(&app, &admin, &back, &[]).await))["code"].clone();
    assert_eq!(exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER).await.status, StatusCode::OK);

    // The tab left open on "Open pley for Android": a page, not JSON.
    for followed in [&[("sec-fetch-mode", "navigate")][..], &[("accept", "text/html,application/xhtml+xml")]] {
        let reply = back_with(&app, &admin, &back, followed).await;
        assert_eq!(location(&reply), "/oauth/over", "{followed:?}");
        assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    }
    // A script still hears it is over.
    let fetched = back_with(&app, &admin, &back, &[("sec-fetch-mode", "cors")]).await;
    assert_eq!(fetched.status, StatusCode::NOT_FOUND);
    assert_eq!(fetched.body["message"], "this request is over: start again from the app or the TV");
}

async fn back_with(app: &TestApp, cookie: &str, back: &str, extra: &[(&str, &str)]) -> Reply {
    app.browser_call(Method::GET, back, cookie, None, extra).await
}

#[tokio::test]
async fn a_replayed_code_ends_only_the_grant_it_made() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, bob) = app.user(&admin, "bob", json!({})).await;
    let code = code_for(&app, &admin, ANDROID_APP, ANDROID_BACK).await;
    let (access, _) = pair(&exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER).await);
    let first: i64 = sqlx::query_scalar("SELECT id FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    // The app signs out, and someone else's app signs in: it gets a number
    // of its own, not the first one's (as SQLite would give it).
    let revoked =
        app.form_post("/api/oauth/revoke", &form(&[("token", &access), ("client_id", ANDROID_APP)]), &[]).await;
    assert_eq!(revoked.status, StatusCode::OK);
    let (bobs, _) = app.app_token(bob, "library").await;
    let second: i64 = sqlx::query_scalar("SELECT id FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_ne!(second, first, "the number isn't given again");

    // The code shown again ends nothing of Bob's.
    let replay = exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER).await;
    assert_eq!(oauth_error(&replay), (StatusCode::BAD_REQUEST, "invalid_grant"));
    assert_eq!(app.get("/api/auth/me", &bobs).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_code_exchanged_twice_at_once_leaves_no_access() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let code = code_for(&app, &admin, ANDROID_APP, ANDROID_BACK).await;
    // The second exchange comes while the first one's tokens are being issued.
    let (a, b) = tokio::join!(
        exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER),
        exchange(&app, &code, ANDROID_APP, ANDROID_BACK, VERIFIER)
    );
    for reply in [&a, &b] {
        if reply.status == StatusCode::OK {
            let access = reply.body["access_token"].as_str().unwrap();
            assert_eq!(app.get("/api/auth/me", access).await.status, StatusCode::UNAUTHORIZED, "{}", reply.text());
        } else {
            assert_eq!(oauth_error(reply), (StatusCode::BAD_REQUEST, "invalid_grant"), "{}", reply.text());
        }
    }
    let grants: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(grants, 0, "{} / {}", a.text(), b.text());
}

// ------------------------------------------------------------------ proxies

#[tokio::test]
async fn behind_a_proxy_that_leaves_the_port_out_pley_names_the_address_apps_used() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    // nginx's `$host`: the port the browser or app used is gone.
    let proxied = [("host", "media.example.com"), ("x-forwarded-proto", "https")];
    assert_eq!(metadata(&app, &proxied).await.body["issuer"], "https://media.example.com");

    let saved = app.put("/api/settings", &admin, json!({"proxy_addresses": ["https://Media.example.com:9999/"]})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.body["proxy_addresses"], json!(["https://media.example.com:9999"]));
    let found = metadata(&app, &proxied).await;
    assert_eq!(found.body["issuer"], "https://media.example.com:9999");
    assert_eq!(found.body["token_endpoint"], "https://media.example.com:9999/api/oauth/token");
    // Other addresses, and a Host with a port, stay as they came.
    assert_eq!(metadata(&app, &[("host", "pley.local:7539")]).await.body["issuer"], "http://pley.local:7539");

    // An address without a port of its own needs no setting.
    let refused = app.put("/api/settings", &admin, json!({"proxy_addresses": ["https://media.example.com"]})).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.text());
    let refused = app
        .put(
            "/api/settings",
            &admin,
            json!({"proxy_addresses": ["https://media.example.com:1", "https://media.example.com:2"]}),
        )
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.text());
}
