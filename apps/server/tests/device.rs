//! TVs sign in with a code they show (OAuth 2.0 Device Authorization Grant,
//! RFC 8628; src/oauth/devices.rs and src/api/oauth.rs): the TV asks for a
//! code, the person types it on pley's own page where they are signed in and
//! allows the app, and the TV's next question gets the tokens pley's apps
//! get on a phone. pley's own apps only, with limits for what anyone can
//! start and for wrong codes. Ten minutes go by when the store's clock is
//! moved on; the exact intervals are counted in the store's own tests.

mod common;

use std::collections::HashMap;
use std::time::Duration;

use axum::http::{Method, StatusCode, header};
use serde_json::json;

use common::{
    ANDROID_APP, DESKTOP_APP, HOMEASSISTANT_APP, HOST, IOS_APP, ISSUER, KODI_APP, MACOS_APP, ROKU_APP, Reply, TVOS_APP,
    TestApp, WEBOS_APP,
};
use pley::oauth::fetch::Source;

const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const OTHER_APP: &str = "https://apps.example.com/pley.json";
const NOT_FROM_PLEY: &str = "this page must be opened from Pley itself";
const BUSY: &str = "too many sign-ins in progress: try again in a minute";
const CODE_USED: &str = "that code was already used: start again on your TV";
const NO_SUCH_CODE: &str = "no TV is waiting for that code: check the code on your TV, or start again there";
const OVER: &str = "this request is over: start again from the app or the TV";
/// The letters of user codes (RFC 8628 §6.1).
const LETTERS: &str = "BCDFGHJKLMNPQRSTVWXZ";

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish()
}

/// A TV asks to sign in to `client_id` with a code, for `scope` when given,
/// with `extra` headers.
async fn ask(app: &TestApp, client_id: &str, scope: Option<&str>, extra: &[(&str, &str)]) -> Reply {
    let mut pairs = vec![("client_id", client_id)];
    if let Some(scope) = scope {
        pairs.push(("scope", scope));
    }
    app.form_post("/api/oauth/device", &form(&pairs), extra).await
}

/// A TV of pley's Android app asks for a code over a connection from `peer`
/// (an address and port), with `extra` headers.
async fn ask_from(app: &TestApp, peer: &str, extra: &[(&str, &str)]) -> Reply {
    app.form_post_from(peer, "/api/oauth/device", &form(&[("client_id", ANDROID_APP)]), extra).await
}

/// A TV of pley's Android app starts signing in: its device code, and the
/// code it shows.
async fn tv(app: &TestApp) -> (String, String) {
    let started = ask(app, ANDROID_APP, None, &[]).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let text = |name: &str| started.body[name].as_str().unwrap().to_owned();
    (text("device_code"), text("user_code"))
}

/// The TV asks for its answer, with `extra` headers.
async fn poll(app: &TestApp, device_code: &str, extra: &[(&str, &str)]) -> Reply {
    let body = form(&[("grant_type", DEVICE_GRANT), ("device_code", device_code), ("client_id", ANDROID_APP)]);
    app.form_post("/api/oauth/token", &body, extra).await
}

/// The person signed in with `cookie` types `typed` on the `/link` page.
async fn verify(app: &TestApp, cookie: &str, typed: &str) -> Reply {
    app.browser_call(Method::POST, "/api/oauth/device/verify", cookie, Some(json!({"user_code": typed})), &[]).await
}

/// The page shows the consent for the code `typed`: the request's id.
async fn consent(app: &TestApp, cookie: &str, typed: &str) -> String {
    let shown = verify(app, cookie, typed).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert_eq!(shown.body["status"], "consent");
    shown.body["id"].as_str().unwrap().to_owned()
}

async fn decide(app: &TestApp, cookie: &str, id: &str, allow: bool) -> Reply {
    app.browser_call(Method::POST, &format!("/api/oauth/requests/{id}"), cookie, Some(json!({"allow": allow})), &[])
        .await
}

/// The person signed in with `cookie` allows the TV showing `user_code`.
async fn allow(app: &TestApp, cookie: &str, user_code: &str) {
    let id = consent(app, cookie, user_code).await;
    let decided = decide(app, cookie, &id, true).await;
    assert_eq!((decided.status, &decided.body), (StatusCode::OK, &json!({"done": true})), "{}", decided.text());
}

async fn refresh(app: &TestApp, token: &str) -> Reply {
    let body = form(&[("grant_type", "refresh_token"), ("refresh_token", token), ("client_id", ANDROID_APP)]);
    app.form_post("/api/oauth/token", &body, &[]).await
}

async fn metadata(app: &TestApp, extra: &[(&str, &str)]) -> Reply {
    app.call(Method::GET, "/.well-known/oauth-authorization-server", None, None, extra).await
}

/// `(access token, refresh token)` of a token answer.
fn pair(reply: &Reply) -> (String, String) {
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    (reply.body["access_token"].as_str().unwrap().to_owned(), reply.body["refresh_token"].as_str().unwrap().to_owned())
}

fn oauth_error(reply: &Reply) -> (StatusCode, &str) {
    (reply.status, reply.body["error"].as_str().unwrap_or_default())
}

/// Answers to apps are never cached (RFC 6749 §5.1, RFC 8628 §3.2).
fn assert_not_cached(reply: &Reply) {
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store", "{}", reply.text());
    assert_eq!(reply.headers[header::PRAGMA], "no-cache");
}

/// Whether `code` is written as TVs show codes: BCDF-GHJK.
fn is_shown_code(code: &str) -> bool {
    code.len() == 9 && code.char_indices().all(|(i, c)| if i == 4 { c == '-' } else { LETTERS.contains(c) })
}

/// Codes that differ from `code` (as shown) in their last letter.
fn wrong_codes(code: &str) -> impl Iterator<Item = String> + '_ {
    let (start, last) = code.split_at(code.len() - 1);
    LETTERS.chars().filter(move |c| !last.contains(*c)).map(move |c| format!("{start}{c}"))
}

async fn grants(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap()
}

#[tokio::test]
async fn a_tv_signs_in_with_a_code_shown_on_screen() {
    let app = TestApp::new().await;
    assert_eq!(app.state.config.client_documents, Source::Off, "no document can be fetched");
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2020).mkv", 2048);
    app.library(&admin, "Movies", "movies", "movies").await;

    // The TV asks for a code to show.
    let started = ask(&app, ANDROID_APP, Some("library play"), &[]).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    assert_not_cached(&started);
    let body = &started.body;
    let device_code = body["device_code"].as_str().unwrap().to_owned();
    let user_code = body["user_code"].as_str().unwrap().to_owned();
    assert_eq!(device_code.len(), 43);
    assert!(is_shown_code(&user_code), "{user_code}");
    let letters = user_code.replace('-', "");
    assert_eq!(body["verification_uri"], format!("{ISSUER}/link"));
    assert_eq!(body["verification_uri_complete"], format!("{ISSUER}/link?code={letters}"));
    assert_eq!((&body["expires_in"], &body["interval"]), (&json!(600), &json!(5)));
    assert_eq!(body.as_object().unwrap().len(), 6, "{body}");

    // Until someone types the code, the TV waits.
    let waiting = poll(&app, &device_code, &[]).await;
    assert_eq!(oauth_error(&waiting), (StatusCode::BAD_REQUEST, "authorization_pending"));
    assert_not_cached(&waiting);

    // The person types it as people do, on pley's page, and sees the app.
    let shown = verify(&app, &admin, &format!(" {} ", letters.to_lowercase())).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert_eq!(shown.body["status"], "consent");
    assert_eq!(
        shown.body["client"],
        json!({"id": ANDROID_APP, "host": "pley-media-server.github.io", "name": "Pley for Android", "first_party": true})
    );
    // An agent that isn't one of pley's apps is named as the sign-ins list
    // names it; the test's connection has no address.
    assert_eq!(
        shown.body["redirect"],
        json!({"kind": "device", "to": user_code, "device": "pley-android/1.0", "address": null})
    );
    assert_eq!(shown.body["scopes"], json!(["library", "play"]));
    let expires_at = shown.body["expires_at"].as_i64().unwrap();
    assert!((pley::util::now() + 590..=pley::util::now() + 600).contains(&expires_at), "{expires_at}");
    let id = shown.body["id"].as_str().unwrap().to_owned();

    // Allowed: nothing goes back through the browser.
    let decided = decide(&app, &admin, &id, true).await;
    assert_eq!((decided.status, &decided.body), (StatusCode::OK, &json!({"done": true})), "{}", decided.text());
    let continued =
        app.browser_call(Method::GET, &format!("/api/oauth/requests/{id}/continue"), &admin, None, &[]).await;
    assert_eq!(continued.status, StatusCode::NOT_FOUND);
    assert!(!continued.headers.contains_key(header::LOCATION));

    // The TV's next question gets its tokens, as apps get them elsewhere.
    let tokens = poll(&app, &device_code, &[]).await;
    assert_not_cached(&tokens);
    assert_eq!(tokens.body["token_type"], "Bearer");
    assert_eq!(tokens.body["expires_in"], 3600);
    assert_eq!(tokens.body["scope"], "library play");
    let (access, refresh_token) = pair(&tokens);
    assert!(access.starts_with("pley_at_") && refresh_token.starts_with("pley_rt_"));
    let libraries = app.get("/api/libraries", &access).await;
    assert_eq!(libraries.status, StatusCode::OK, "{}", libraries.text());
    assert_eq!(libraries.body.as_array().unwrap().len(), 1);
    let me = app.get("/api/auth/me", &access).await;
    assert_eq!(
        (&me.body["username"], &me.body["is_admin"], &me.body["scope"]),
        (&json!("admin"), &json!(false), &json!("library play"))
    );

    // It refreshes like any app.
    let (renewed, _) = pair(&refresh(&app, &refresh_token).await);
    assert_eq!(app.get("/api/libraries", &renewed).await.status, StatusCode::OK);

    // The answer came once: the device code and the code shown are spent.
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await), (StatusCode::BAD_REQUEST, "invalid_grant"));
    assert_eq!(verify(&app, &admin, &user_code).await.status, StatusCode::NOT_FOUND);
    assert_eq!(grants(&app).await, 1);
}

#[tokio::test]
async fn a_tv_tells_its_server_from_another_device_at_its_address() {
    use base64::Engine;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (device_code, user_code) = tv(&app).await;
    allow(&app, &admin, &user_code).await;
    let tokens = poll(&app, &device_code, &[]).await;
    let (access, refresh_token) = pair(&tokens);

    // The tokens come with the server's id and a key of this sign-in's own.
    let server = app.call(Method::GET, "/api/server", None, None, &[]).await;
    let id = server.body["id"].as_str().unwrap().to_owned();
    assert_eq!(id.len(), 40, "discovery's id: {id}");
    assert_eq!(tokens.body["server_id"], json!(id));
    let proof_id = tokens.body["proof_id"].as_str().unwrap().to_owned();
    let key = tokens.body["proof_key"].as_str().unwrap().to_owned();
    assert!(server.body.get("proof").is_none(), "no proof without a challenge");

    // Asked with a challenge at its home address, the server proves it
    // holds that key, for that address.
    let challenge = "AAECAwQFBgcICQoLDA0ODw";
    let ask = format!("/api/server?challenge={challenge}&proof_id={proof_id}");
    let home = [("host", "192.168.1.20:7539")];
    let asked = app.get_from("192.168.1.30:50000", &ask, &home).await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text());
    assert_eq!(asked.headers[header::CACHE_CONTROL], "no-store");
    let key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&key).unwrap();
    let at_home = pley::identity::proof(&key_bytes, &id, "http://192.168.1.20:7539", challenge);
    assert_eq!(asked.body["proof"], json!(at_home));
    assert_eq!(asked.body["address"], json!("http://192.168.1.20:7539"), "what the proof covers");
    // A page in a sandboxed frame (Origin: null) in a browser at home gets
    // no proof, however it writes the challenge's name, nor does a
    // website's page; LG's TV app (null, and webOS in its User-Agent) and
    // the apps that send file:// or app:// get one they can read.
    let from = |uri: String, origin: &'static str, agent: &'static str| {
        let app = &app;
        async move {
            app.get_from(
                "192.168.1.30:50000",
                &uri,
                &[("host", "192.168.1.20:7539"), ("origin", origin), ("user-agent", agent)],
            )
            .await
        }
    };
    let safari = "Mozilla/5.0 (Macintosh) Safari/605.1.15";
    let spelled = format!("/api/server?%63hallenge={challenge}&proof_id={proof_id}");
    for (uri, origin) in [(&ask, "null"), (&spelled, "null"), (&ask, "https://example.com")] {
        let page = from(uri.clone(), origin, safari).await;
        assert_eq!(page.status, StatusCode::FORBIDDEN, "{uri} from {origin}: {}", page.text());
        assert!(page.body.get("proof").is_none());
    }
    for (origin, agent) in [
        ("null", "Mozilla/5.0 (Web0S; Linux/SmartTV) Chrome/87"),
        ("file://", "Mozilla/5.0 (SMART-TV; Linux; Tizen 9.0)"),
    ] {
        let tv = from(ask.clone(), origin, agent).await;
        assert_eq!(tv.body["proof"], json!(at_home), "{origin}");
        assert!(tv.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_some(), "{origin}");
    }
    let other =
        app.get_from("192.168.1.30:50000", "/api/server", &[("host", "192.168.1.20:7539"), ("origin", "null")]).await;
    assert!(
        other.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_some(),
        "the server's name, without a proof, as before"
    );
    // Through the HTTPS proxy, for the public address: not a proof for the
    // home one, which an app at that address refuses.
    let proxy = [("host", "pley.example.com"), ("x-forwarded-proto", "https"), ("x-forwarded-for", "203.0.113.9")];
    let public = app.get_from("127.0.0.1:50000", &ask, &proxy).await;
    assert_eq!(public.status, StatusCode::OK, "{}", public.text());
    assert_eq!(
        public.body["proof"],
        json!(pley::identity::proof(&key_bytes, &id, "https://pley.example.com", challenge))
    );
    assert_ne!(public.body["proof"], json!(at_home));
    // A plain-http address asked from the internet (a device elsewhere
    // passing the challenge on, saying it is the home address): no proof.
    // Nor an https one said by a client on the internet rather than a proxy
    // at home: it could name an address the server no longer has.
    for (peer, extra) in [
        ("203.0.113.9:50000", vec![("host", "192.168.1.20:7539")]),
        ("127.0.0.1:50000", vec![("host", "192.168.1.20:7539"), ("x-forwarded-for", "203.0.113.9")]),
        ("203.0.113.9:50000", vec![("host", "old.example.com"), ("x-forwarded-proto", "https")]),
    ] {
        let relayed = app.get_from(peer, &ask, &extra).await;
        assert_eq!(relayed.status, StatusCode::FORBIDDEN, "{peer} {extra:?}: {}", relayed.text());
        assert!(relayed.body.get("proof").is_none());
    }
    // Nor from an internet provider's shared address naming a home client
    // as a proxy would; a computer on Tailscale reaches it from its own.
    let shared = "100.72.3.4";
    let relayed = app
        .get_from(&format!("{shared}:50000"), &ask, &[("host", "192.168.1.20:7539"), ("x-forwarded-for", "127.0.0.1")])
        .await;
    let tailscale = pley::identity::at_home(shared.parse().unwrap());
    assert_eq!(relayed.status == StatusCode::OK, tailscale, "{}", relayed.text());
    for wrong in [
        format!("/api/server?challenge={challenge}"),
        format!("/api/server?proof_id={proof_id}"),
        format!("/api/server?challenge=short&proof_id={proof_id}"),
        format!("/api/server?challenge={challenge}&proof_id=nope"),
    ] {
        assert_eq!(app.get_from("192.168.1.30:50000", &wrong, &home).await.status, StatusCode::BAD_REQUEST, "{wrong}");
    }

    // A refresh keeps the key; another sign-in has its own.
    let refreshed = refresh(&app, &refresh_token).await;
    assert_eq!((&refreshed.body["proof_id"], &refreshed.body["proof_key"]), (&json!(proof_id), &json!(key)));
    let (device_code, user_code) = tv(&app).await;
    allow(&app, &admin, &user_code).await;
    let other = poll(&app, &device_code, &[]).await;
    assert_ne!(other.body["proof_key"], json!(key));

    // Pley's own sign-in (the desktop app's window) asks for a key of its
    // own; apps have theirs.
    let session = app.get("/api/server/proof-key", &admin).await;
    assert_eq!(session.status, StatusCode::OK, "{}", session.text());
    assert_eq!(session.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(session.body["server_id"], json!(id));
    let session_id = session.body["proof_id"].as_str().unwrap();
    assert!(session_id.starts_with('s'), "{session_id}");
    let session_key =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(session.body["proof_key"].as_str().unwrap()).unwrap();
    let asked = app
        .get_from("192.168.1.30:50000", &format!("/api/server?challenge={challenge}&proof_id={session_id}"), &home)
        .await;
    assert_eq!(
        asked.body["proof"],
        json!(pley::identity::proof(&session_key, &id, "http://192.168.1.20:7539", challenge))
    );
    assert_ne!(app.get("/api/server/proof-key", &access).await.status, StatusCode::OK, "not for apps");
    let anyone = app.call(Method::GET, "/api/server/proof-key", None, None, &[]).await;
    assert_eq!(anyone.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_disabled_account_gets_no_tokens_for_its_tv() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (device_code, user_code) = tv(&app).await;
    allow(&app, &bob, &user_code).await;
    let set = |disabled: bool| {
        let (app, admin) = (&app, &admin);
        async move {
            let set = app.patch(&format!("/api/users/{bob_id}"), admin, json!({"disabled": disabled})).await;
            assert_eq!(set.status, StatusCode::OK, "{}", set.text());
        }
    };
    set(true).await;
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await), (StatusCode::BAD_REQUEST, "access_denied"));
    // The answer is spent: enabling Bob again doesn't bring it back.
    set(false).await;
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await).1, "invalid_grant");
    assert_eq!(grants(&app).await, 0);

    // Nor does a TV that waits until he is enabled again get in.
    let (device_code, user_code) = tv(&app).await;
    allow(&app, &bob, &user_code).await;
    set(true).await;
    set(false).await;
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await), (StatusCode::BAD_REQUEST, "access_denied"));
    assert_eq!(grants(&app).await, 0);
}

#[tokio::test]
async fn polling_before_a_decision_is_pending_and_too_fast_slows_down() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (device_code, user_code) = tv(&app).await;
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await), (StatusCode::BAD_REQUEST, "authorization_pending"));

    // Asked again before the interval: slow down, each time (by 5 seconds
    // more each time: the store's tests count them).
    for _ in 0..2 {
        let slowed = poll(&app, &device_code, &[]).await;
        assert_eq!(oauth_error(&slowed), (StatusCode::BAD_REQUEST, "slow_down"));
        assert_not_cached(&slowed);
    }
    // While the person decides, it still waits.
    let id = consent(&app, &admin, &user_code).await;
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await).1, "slow_down");

    // Questions that aren't the TV's change nothing.
    let other_app = form(&[("grant_type", DEVICE_GRANT), ("device_code", &device_code), ("client_id", DESKTOP_APP)]);
    assert_eq!(oauth_error(&app.form_post("/api/oauth/token", &other_app, &[]).await).1, "invalid_grant");
    assert_eq!(oauth_error(&poll(&app, "made-up", &[]).await).1, "invalid_grant");
    for pairs in [
        [("grant_type", DEVICE_GRANT), ("client_id", ANDROID_APP)],
        [("grant_type", DEVICE_GRANT), ("device_code", device_code.as_str())],
    ] {
        let reply = app.form_post("/api/oauth/token", &form(&pairs), &[]).await;
        assert_eq!(oauth_error(&reply), (StatusCode::BAD_REQUEST, "invalid_request"), "{pairs:?}");
    }

    // Once the person answered, the answer comes at once, however soon.
    assert_eq!(decide(&app, &admin, &id, true).await.status, StatusCode::OK);
    let (access, _) = pair(&poll(&app, &device_code, &[]).await);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_denied_code_answers_access_denied_once() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (device_code, user_code) = tv(&app).await;
    let id = consent(&app, &admin, &user_code).await;
    let denied = decide(&app, &admin, &id, false).await;
    assert_eq!((denied.status, &denied.body), (StatusCode::OK, &json!({"done": true})), "{}", denied.text());
    // The same answer again gets the same reply; the other one is refused.
    assert_eq!(decide(&app, &admin, &id, false).await.body, denied.body);
    let changed = decide(&app, &admin, &id, true).await;
    assert_eq!(
        (changed.status, &changed.body["message"]),
        (StatusCode::CONFLICT, &json!("this request was answered already: start again on your TV"))
    );
    // Nobody can type the code again.
    let again = verify(&app, &admin, &user_code).await;
    assert_eq!((again.status, &again.body["message"]), (StatusCode::CONFLICT, &json!(CODE_USED)));

    // The TV is told once; then its request is gone.
    let told = poll(&app, &device_code, &[]).await;
    assert_eq!(oauth_error(&told), (StatusCode::BAD_REQUEST, "access_denied"));
    assert_not_cached(&told);
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await).1, "invalid_grant");
    assert_eq!(verify(&app, &admin, &user_code).await.status, StatusCode::NOT_FOUND);
    assert_eq!(grants(&app).await, 0);
}

#[tokio::test]
async fn only_pleys_own_apps_sign_in_with_a_code() {
    // Another publisher's app whose description says it signs in with codes,
    // on a server that allows other publishers' apps: for someone who isn't
    // signed in, pley reads no description, so it never learns that.
    let document = json!({
        "client_id": OTHER_APP,
        "client_name": "Example TV",
        "redirect_uris": ["https://apps.example.com/callback"],
        "grant_types": ["authorization_code", DEVICE_GRANT],
    })
    .to_string();
    let documents = HashMap::from([(OTHER_APP.to_owned(), document)]);
    let app = TestApp::with_config(|config| config.client_documents = Source::Fixed(documents)).await;
    let admin = app.setup_admin().await;
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": true})).await.status, StatusCode::OK);
    for client_id in [OTHER_APP, "https://apps.example.com/unknown.json", "not an address"] {
        let refused = ask(&app, client_id, None, &[]).await;
        assert_eq!(oauth_error(&refused), (StatusCode::BAD_REQUEST, "invalid_client"), "{client_id}");
        assert_eq!(refused.body["error_description"], "only Pley's own apps can sign in with a code");
        assert_not_cached(&refused);
    }
    assert_eq!(app.state.oauth.documents.fetched(), 0, "no description was read");
    // Someone signed in who opens the app's sign-in page does get it read.
    let query = form(&[("client_id", OTHER_APP), ("redirect_uri", "https://apps.example.com/callback")]);
    let page = app.browser_call(Method::POST, "/api/oauth/requests", &admin, Some(json!({"query": query})), &[]).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    assert_eq!(app.state.oauth.documents.fetched(), 1);
    // Desktops have browsers: pley's desktop app and the Apple app on a Mac
    // sign in on pley's page.
    for client_id in [DESKTOP_APP, MACOS_APP] {
        let desktop = ask(&app, client_id, None, &[]).await;
        assert_eq!(oauth_error(&desktop), (StatusCode::BAD_REQUEST, "unauthorized_client"), "{client_id}");
    }
    // An iPad on a stand or a shared iPhone may show a code instead.
    let ipad = ask(&app, IOS_APP, Some("library play"), &[]).await;
    assert_eq!(ipad.status, StatusCode::OK, "{}", ipad.text());
    let shown = verify(&app, &admin, ipad.body["user_code"].as_str().unwrap()).await;
    assert_eq!(shown.body["client"]["name"], "Pley for iPhone and iPad", "{}", shown.text());
    assert_eq!(shown.body["client"]["first_party"], true);

    // Scopes by the consent page's rules: library when none is asked for,
    // nothing pley doesn't offer, nothing beyond the app's description.
    for scope in ["openid", "library admin", "Library"] {
        assert_eq!(oauth_error(&ask(&app, ANDROID_APP, Some(scope), &[]).await).1, "invalid_scope", "{scope}");
    }
    // Managing the server is never allowed with a code, which anyone in
    // front of the screen can pass on, even to the app whose phones may.
    let server = ask(&app, ANDROID_APP, Some("library play server"), &[]).await;
    assert_eq!(oauth_error(&server).1, "invalid_scope");
    assert!(server.text().contains("managing the server"), "{}", server.text());
    let everything = ask(&app, ANDROID_APP, Some("library play playlists profiles"), &[]).await;
    assert_eq!(everything.status, StatusCode::OK, "{}", everything.text());
    let unsaid = ask(&app, ANDROID_APP, Some(""), &[]).await;
    let user_code = unsaid.body["user_code"].as_str().unwrap();
    assert_eq!(verify(&app, &admin, user_code).await.body["scopes"], json!(["library"]));

    // The token endpoint's form rules.
    let missing = app.form_post("/api/oauth/device", &form(&[("scope", "library")]), &[]).await;
    assert_eq!(oauth_error(&missing), (StatusCode::BAD_REQUEST, "invalid_request"));
    let twice = format!("{}&client_id=x", form(&[("client_id", ANDROID_APP)]));
    assert_eq!(oauth_error(&app.form_post("/api/oauth/device", &twice, &[]).await).1, "invalid_request");
    let secret = form(&[("client_id", ANDROID_APP), ("client_secret", "s3cret")]);
    let refused = app.form_post("/api/oauth/device", &secret, &[]).await;
    assert_eq!(oauth_error(&refused), (StatusCode::BAD_REQUEST, "invalid_client"));
    let android = form(&[("client_id", ANDROID_APP)]);
    let basic = app.form_post("/api/oauth/device", &android, &[("authorization", "Basic YTpi")]).await;
    assert_eq!(oauth_error(&basic), (StatusCode::UNAUTHORIZED, "invalid_client"));
    assert_eq!(basic.headers[header::WWW_AUTHENTICATE], r#"Basic realm="pley""#);
    let as_json = json!({"client_id": ANDROID_APP});
    let as_json = app.call(Method::POST, "/api/oauth/device", None, Some(as_json), &[("host", HOST)]).await;
    assert_eq!(oauth_error(&as_json), (StatusCode::BAD_REQUEST, "invalid_request"));
}

#[tokio::test]
async fn wrong_codes_are_throttled() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let (_, user_code) = tv(&app).await;
    // Codes that can't be a TV's are refused, and don't count.
    for typed in ["", "BCDF", "BCDF-GHJK-L", "BCDF-GHJ1", "AEIO-UAEI", "BCDF_GHJK", "BBBBBBBBB"] {
        let reply = verify(&app, &admin, typed).await;
        assert_eq!(
            (reply.status, &reply.body["message"]),
            (StatusCode::BAD_REQUEST, &json!("that isn't a code from a Pley app: codes are 8 letters, like BCDF-GHJK")),
            "{typed:?}"
        );
    }
    // Ten codes no TV shows...
    for wrong in wrong_codes(&user_code).take(10) {
        let reply = verify(&app, &admin, &wrong).await;
        assert_eq!((reply.status, &reply.body["message"]), (StatusCode::NOT_FOUND, &json!(NO_SUCH_CODE)), "{wrong}");
    }
    // ...and for a while not even the right one is looked up.
    let held = verify(&app, &admin, &user_code).await;
    assert_eq!(
        (held.status, &held.body["message"]),
        (StatusCode::TOO_MANY_REQUESTS, &json!("too many wrong codes: wait a few minutes and try again"))
    );
    // Others aren't held up by them.
    consent(&app, &bob, &user_code).await;
}

#[tokio::test]
async fn a_code_is_decided_once() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let (device_code, user_code) = tv(&app).await;
    let first = consent(&app, &admin, &user_code).await;

    // On one page at a time: neither another person nor another session
    // takes it.
    for cookie in [bob.clone(), app.login("admin", "secret-one").await] {
        let taken = verify(&app, &cookie, &user_code).await;
        assert_eq!((taken.status, &taken.body["message"]), (StatusCode::CONFLICT, &json!(CODE_USED)));
    }
    // Typed again on the same session's page, it is shown afresh, and the
    // page before can't answer any more.
    let second = consent(&app, &admin, &user_code).await;
    assert_ne!(first, second);
    let before = decide(&app, &admin, &first, true).await;
    assert_eq!((before.status, &before.body["message"]), (StatusCode::NOT_FOUND, &json!(OVER)));
    assert_eq!(decide(&app, &bob, &second, true).await.status, StatusCode::NOT_FOUND, "not Bob's to answer");

    // Answered once.
    assert_eq!(decide(&app, &admin, &second, true).await.status, StatusCode::OK);
    assert_eq!(decide(&app, &admin, &second, true).await.body, json!({"done": true}));
    assert_eq!(decide(&app, &admin, &second, false).await.status, StatusCode::CONFLICT);
    assert_eq!(verify(&app, &bob, &user_code).await.status, StatusCode::CONFLICT);

    // Two questions at once: one grant.
    let (one, other) = tokio::join!(poll(&app, &device_code, &[]), poll(&app, &device_code, &[]));
    let mut answers = [oauth_error(&one), oauth_error(&other)];
    answers.sort();
    assert_eq!(answers, [(StatusCode::OK, ""), (StatusCode::BAD_REQUEST, "invalid_grant")]);
    assert_eq!(grants(&app).await, 1);
}

#[tokio::test]
async fn a_code_shown_to_a_session_that_ended_goes_to_the_next_one() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let created = app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await;
    let kid = created.body["id"].as_i64().unwrap();

    // "Use another profile": the session that showed the code ends, and the
    // profile picked takes the code over.
    let (device_code, user_code) = tv(&app).await;
    let session = app.login("admin", "secret-one").await;
    consent(&app, &session, &user_code).await;
    let switched = app.post("/api/auth/switch", &session, json!({"user_id": kid})).await;
    assert_eq!(switched.status, StatusCode::OK, "{}", switched.text());
    let kids = switched.body["token"].as_str().unwrap().to_owned();
    allow(&app, &kids, &user_code).await;
    let (access, _) = pair(&poll(&app, &device_code, &[]).await);
    assert_eq!(app.get("/api/auth/me", &access).await.body["username"], "kid");

    // "Not you? Sign out" too.
    let (device_code, user_code) = tv(&app).await;
    let session = app.login("admin", "secret-one").await;
    consent(&app, &session, &user_code).await;
    let out = app.browser_call(Method::POST, "/api/auth/logout", &session, None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    allow(&app, &bob, &user_code).await;
    let (access, _) = pair(&poll(&app, &device_code, &[]).await);
    assert_eq!(app.get("/api/auth/me", &access).await.body["username"], "bob");
}

#[tokio::test]
async fn the_verify_api_takes_the_cookie_of_pleys_own_page_only() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, user_code) = tv(&app).await;
    let body = json!({"user_code": user_code});
    let path = "/api/oauth/device/verify";
    let page = [("host", HOST), ("origin", ISSUER), ("sec-fetch-site", "same-origin")];

    // A session anywhere but in the cookie, and an app's token anywhere, are
    // refused.
    let (access, _) = app.app_token(1, "library play playlists profiles").await;
    let app_cookie = format!("pley_token={access}");
    let session_header: Vec<(&str, &str)> = [&page[..], &[("x-pley-token", admin.as_str())]].concat();
    let app_token_cookie: Vec<(&str, &str)> = [&page[..], &[("cookie", app_cookie.as_str())]].concat();
    let in_url = format!("{path}?token={admin}");
    for (case, reply) in [
        ("a Bearer session", app.call(Method::POST, path, Some(&admin), Some(body.clone()), &page).await),
        ("X-Pley-Token", app.call(Method::POST, path, None, Some(body.clone()), &session_header).await),
        ("?token=", app.call(Method::POST, &in_url, None, Some(body.clone()), &page).await),
        ("an app's token", app.call(Method::POST, path, Some(&access), Some(body.clone()), &page).await),
        (
            "an app's token as the cookie",
            app.call(Method::POST, path, None, Some(body.clone()), &app_token_cookie).await,
        ),
    ] {
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{case}");
    }
    // Only from pley's own page.
    for (case, extra) in [
        ("no Origin", [("origin", "")]),
        ("another site", [("origin", "http://evil.example")]),
        ("another port", [("origin", "http://pley.local:7540")]),
        ("cross-site", [("sec-fetch-site", "cross-site")]),
        ("same-site", [("sec-fetch-site", "same-site")]),
    ] {
        let reply = app.browser_call(Method::POST, path, &admin, Some(body.clone()), &extra).await;
        assert_eq!((reply.status, &reply.body["message"]), (StatusCode::FORBIDDEN, &json!(NOT_FROM_PLEY)), "{case}");
    }
    // None of them took the code: pley's own page gets it, without CORS.
    let shown = verify(&app, &admin, &user_code).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert!(!shown.headers.contains_key("access-control-allow-origin"));
    assert_eq!(shown.headers[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn device_requests_are_capped() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    // 20 at a time from one address, whatever port each connection uses.
    let first = ask_from(&app, "203.0.113.9:50000", &[]).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    for port in 50_001..50_020 {
        assert_eq!(ask_from(&app, &format!("203.0.113.9:{port}"), &[]).await.status, StatusCode::OK);
    }
    let full = ask_from(&app, "203.0.113.9:50020", &[]).await;
    assert_eq!(oauth_error(&full), (StatusCode::TOO_MANY_REQUESTS, "temporarily_unavailable"));
    assert_eq!(full.body["error_description"], BUSY);
    assert_not_cached(&full);
    // What a caller writes in X-Forwarded-For doesn't make it someone else.
    for forged in ["203.0.113.10", "10.0.0.1", "10.0.0.1, 203.0.113.11", "spoof-1"] {
        let refused = ask_from(&app, "203.0.113.9:50021", &[("x-forwarded-for", forged)]).await;
        assert_eq!(oauth_error(&refused).1, "temporarily_unavailable", "{forged}");
    }
    // Another address has its own 20.
    assert_eq!(ask_from(&app, "198.51.100.7:50000", &[]).await.status, StatusCode::OK);

    // 1,000 in all.
    let android = pley::oauth::AppSnapshot::from(pley::oauth::clients::builtin(ANDROID_APP).unwrap());
    let library = pley::auth::scopes::Scopes::parse("library").unwrap();
    for i in 21..1_000 {
        let address = format!("192.0.2.{}", i / 20).parse().unwrap();
        app.state.oauth.devices.create(android.clone(), library, Some(address)).unwrap();
    }
    let none_left = ask_from(&app, "198.51.100.200:50000", &[]).await;
    assert_eq!(oauth_error(&none_left), (StatusCode::TOO_MANY_REQUESTS, "temporarily_unavailable"));

    // Nothing in progress was dropped to make room.
    let device_code = first.body["device_code"].as_str().unwrap();
    assert_eq!(oauth_error(&poll(&app, device_code, &[]).await).1, "authorization_pending");
    consent(&app, &admin, first.body["user_code"].as_str().unwrap()).await;
}

#[tokio::test]
async fn tvs_count_by_their_own_address_behind_a_proxy_and_by_network_on_ipv6() {
    let app = TestApp::new().await;
    let through_proxy = |forwarded: String| {
        let app = &app;
        async move { ask_from(app, "127.0.0.1:40000", &[("x-forwarded-for", forwarded.as_str())]).await }
    };
    // A proxy on this computer adds the address of the TV it forwards for
    // after whatever the TV sent: that address counts, never the rest.
    for i in 0..20 {
        let started = through_proxy(format!("10.0.0.{i}, 203.0.113.50")).await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    }
    let full = through_proxy("10.0.0.99, 203.0.113.50".into()).await;
    assert_eq!(oauth_error(&full).1, "temporarily_unavailable");
    assert_eq!(through_proxy("203.0.113.51".into()).await.status, StatusCode::OK, "another TV");
    // A header that names no address leaves the proxy's own, shared by all
    // such requests.
    for i in 0..20 {
        assert_eq!(through_proxy(format!("spoof-{i}")).await.status, StatusCode::OK);
    }
    assert_eq!(oauth_error(&through_proxy("spoof-20".into()).await).1, "temporarily_unavailable");

    // On IPv6, one host may use any address of its /64: they count together.
    for i in 1..=20 {
        let started = ask_from(&app, &format!("[2001:db8:1:2:{i:x}::{i:x}]:50000"), &[]).await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    }
    let full = ask_from(&app, "[2001:db8:1:2:ffff:ffff:ffff:ffff]:50000", &[]).await;
    assert_eq!(oauth_error(&full).1, "temporarily_unavailable");
    assert_eq!(ask_from(&app, "[2001:db8:1:3::1]:50000", &[]).await.status, StatusCode::OK, "another network");
}

#[tokio::test]
async fn codes_expire_after_ten_minutes() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (device_code, user_code) = tv(&app).await;

    // Typed with five minutes left, the consent page ends with the TV's
    // request, not ten minutes later.
    app.state.oauth.devices.wait(Duration::from_secs(300));
    let shown = verify(&app, &admin, &user_code).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    let expires_at = shown.body["expires_at"].as_i64().unwrap();
    let now = pley::util::now();
    assert!((now + 290..=now + 300).contains(&expires_at), "{expires_at} for {now}");
    let id = shown.body["id"].as_str().unwrap().to_owned();

    // Ten minutes after the TV asked, the page's answer comes too late...
    app.state.oauth.devices.wait(Duration::from_secs(300));
    let late = decide(&app, &admin, &id, true).await;
    assert_eq!((late.status, &late.body["message"]), (StatusCode::NOT_FOUND, &json!(OVER)));
    // ...the TV is told once that its code expired...
    let expired = poll(&app, &device_code, &[]).await;
    assert_eq!(oauth_error(&expired), (StatusCode::BAD_REQUEST, "expired_token"));
    assert_eq!(expired.body["error_description"], "the code expired: start again");
    assert_not_cached(&expired);
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await), (StatusCode::BAD_REQUEST, "invalid_grant"));
    // ...and nobody can type the code any more.
    let typed = verify(&app, &admin, &user_code).await;
    assert_eq!((typed.status, &typed.body["message"]), (StatusCode::NOT_FOUND, &json!(NO_SUCH_CODE)));
    assert_eq!(grants(&app).await, 0);
}

/// A TV that the person signed in with `cookie` allowed, and that hasn't
/// asked for its answer yet: its device code, and the code it shows.
async fn allowed_tv(app: &TestApp, cookie: &str) -> (String, String) {
    let (device_code, user_code) = tv(app).await;
    allow(app, cookie, &user_code).await;
    (device_code, user_code)
}

/// The TV of `device_code` asks for its answer and is told no, and nothing
/// was signed in.
async fn told_no(app: &TestApp, device_code: &str) {
    assert_eq!(oauth_error(&poll(app, device_code, &[]).await), (StatusCode::BAD_REQUEST, "access_denied"));
    assert_eq!(grants(app).await, 0);
}

#[tokio::test]
async fn a_tv_allowed_but_not_signed_in_yet_ends_with_the_accounts_sign_ins() {
    // Bob allows a TV that doesn't ask for its answer at once (a code
    // someone sent him, RFC 8628 §5.4); then he, or an administrator, ends
    // his sign-ins, which nothing in his apps shows yet. Nothing gets in
    // after what was meant to sign everything out.
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;

    // He changes his password: the code can't be used again either.
    let (device_code, user_code) = allowed_tv(&app, &bob).await;
    let password = json!({"current_password": "test-pass-1", "new_password": "test-pass-2"});
    let changed = app.put("/api/auth/password", &bob, password).await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{}", changed.text());
    let again = verify(&app, &bob, &user_code).await;
    assert_eq!((again.status, &again.body["message"]), (StatusCode::CONFLICT, &json!(CODE_USED)));
    told_no(&app, &device_code).await;

    // An administrator signs him out everywhere.
    let (device_code, _) = allowed_tv(&app, &bob).await;
    let out = app.delete(&format!("/api/users/{bob_id}/devices"), &admin).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{}", out.text());
    told_no(&app, &device_code).await;

    // An administrator gives him a new password.
    let bob = app.login("bob", "test-pass-2").await;
    let (device_code, _) = allowed_tv(&app, &bob).await;
    let set = app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"password": "test-pass-3"})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    told_no(&app, &device_code).await;

    // His account is deleted, and another made: it never gets his number.
    let bob = app.login("bob", "test-pass-3").await;
    let (device_code, _) = allowed_tv(&app, &bob).await;
    assert_eq!(app.delete(&format!("/api/users/{bob_id}"), &admin).await.status, StatusCode::NO_CONTENT);
    let (_, carol_id) = app.user(&admin, "carol", json!({})).await;
    assert!(carol_id > bob_id, "a new account counts on from the numbers ever given");
    told_no(&app, &device_code).await;
}

#[tokio::test]
async fn a_tv_allowed_from_a_session_that_signed_out_since_gets_nothing() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let other = app.login("bob", "test-pass-1").await;
    let (device_code, _) = allowed_tv(&app, &other).await;
    let out = app.browser_call(Method::POST, "/api/auth/logout", &other, None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let refused = poll(&app, &device_code, &[]).await;
    assert_eq!(oauth_error(&refused), (StatusCode::BAD_REQUEST, "access_denied"));
    assert_eq!(refused.body["error_description"], "the person who allowed it signed out since: sign in again");
    assert_eq!(grants(&app).await, 0);
    // The request is over.
    assert_eq!(oauth_error(&poll(&app, &device_code, &[]).await).1, "invalid_grant");

    // From a session still signed in, it gets in.
    let (device_code, _) = allowed_tv(&app, &bob).await;
    let (access, _) = pair(&poll(&app, &device_code, &[]).await);
    assert_eq!(app.get("/api/auth/me", &access).await.body["username"], "bob");
}

#[tokio::test]
async fn device_grants_act_like_other_app_grants() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;

    // An administrator's TV acts as a viewer.
    let (device_code, user_code) = tv(&app).await;
    allow(&app, &admin, &user_code).await;
    let (access, _) = pair(&poll(&app, &device_code, &[("user-agent", "pley-tv/2.0 okhttp/5")]).await);
    let me = app.get("/api/auth/me", &access).await;
    assert_eq!((&me.body["username"], &me.body["is_admin"]), (&json!("admin"), &json!(false)));
    for path in ["/api/settings", "/api/users", "/api/me/apps"] {
        let refused = app.get(path, &access).await;
        let refusal = (refused.status, &refused.body["code"]);
        assert_eq!(refusal, (StatusCode::FORBIDDEN, &json!("insufficient_scope")), "{path}");
    }
    // It is listed with the account's apps, as the TV that got the tokens.
    let listed = app.get("/api/me/apps", &admin).await;
    assert_eq!(listed.body.as_array().unwrap().len(), 1, "{}", listed.text());
    let grant = &listed.body[0];
    assert_eq!(
        (&grant["name"], &grant["first_party"], &grant["device"]),
        (&json!("Pley for Android"), &json!(true), &json!("pley-tv/2.0"))
    );
    assert_eq!(grant["scopes"], json!(["library"]));
    assert_eq!((&grant["profile"]["name"], &grant["approved_by"]["name"]), (&json!("admin"), &json!("admin")));

    // Bob's TV ends with his sign-ins: here, an administrator sets his
    // password.
    let (device_code, user_code) = tv(&app).await;
    allow(&app, &bob, &user_code).await;
    let (access, refresh_token) = pair(&poll(&app, &device_code, &[]).await);
    assert_eq!(app.get("/api/auth/me", &access).await.body["username"], "bob");
    let set = app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"password": "another-pass-2"})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(oauth_error(&refresh(&app, &refresh_token).await).1, "invalid_grant");
}

#[tokio::test]
async fn kodi_roku_lg_apple_tv_and_home_assistant_sign_in_only_with_a_code() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (client_id, name, agent, platform) in [
        (KODI_APP, "Pley for Kodi", "pleyKodi/0.1.0 (Kodi 21.2; Windows)", "Kodi 21.2; Windows"),
        (ROKU_APP, "Pley for Roku", "pleyRoku/0.1.0 (Roku OS 14.5; Roku Ultra)", "Roku OS 14.5; Roku Ultra"),
        (WEBOS_APP, "Pley for LG TV", "pleyLG/0.1.0 (webOS 23; OLED65C3PUA)", "webOS 23; OLED65C3PUA"),
        (
            HOMEASSISTANT_APP,
            "Pley for Home Assistant",
            "pleyHomeAssistant/0.1.0 (Home Assistant 2026.11.0)",
            "Home Assistant 2026.11.0",
        ),
        // An agent pley doesn't know is named as the sign-ins list names it.
        (TVOS_APP, "Pley for Apple TV", "pley-tv/1.0", "pley-tv/1.0"),
    ] {
        let asked = form(&[("client_id", client_id), ("scope", "library play")]);
        let started =
            app.form_post_from("192.168.1.20:51515", "/api/oauth/device", &asked, &[("user-agent", agent)]).await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.text());
        let device_code = started.body["device_code"].as_str().unwrap();
        let user_code = started.body["user_code"].as_str().unwrap();
        let shown = verify(&app, &admin, user_code).await;
        assert_eq!(shown.body["client"]["name"], name, "{}", shown.text());
        assert_eq!(shown.body["client"]["first_party"], true);
        // The page says which device is asking: what it says it is, and
        // where from.
        assert_eq!(
            shown.body["redirect"],
            json!({"kind": "device", "to": user_code, "device": platform, "address": "192.168.1.20"})
        );
        let id = shown.body["id"].as_str().unwrap();
        assert_eq!(decide(&app, &admin, id, true).await.status, StatusCode::OK);
        let asked = form(&[("grant_type", DEVICE_GRANT), ("device_code", device_code), ("client_id", client_id)]);
        let tokens = app.form_post("/api/oauth/token", &asked, &[("user-agent", agent)]).await;
        let (access, refresh_token) = pair(&tokens);
        assert_eq!(tokens.body["scope"], "library play");
        assert_eq!(app.get("/api/auth/me", &access).await.body["username"], "admin");
        let renewed =
            form(&[("grant_type", "refresh_token"), ("refresh_token", &refresh_token), ("client_id", client_id)]);
        let renewed = app.form_post("/api/oauth/token", &renewed, &[("user-agent", agent)]).await;
        pair(&renewed);

        // Listed by its name, with the platform it gave.
        let listed = app.get("/api/me/apps", &admin).await;
        let grant = listed.body.as_array().unwrap().iter().find(|g| g["client_id"] == client_id).unwrap();
        assert_eq!((&grant["name"], &grant["device"]), (&json!(name), &json!(platform)));

        // They have no browser: pley's sign-in page has no return address for
        // them, so no code is ever sent anywhere.
        for redirect_uri in ["app.pley.kodi:/oauth/callback", "app.pley.apple:/oauth/callback"] {
            let query = form(&[
                ("response_type", "code"),
                ("client_id", client_id),
                ("redirect_uri", redirect_uri),
                ("code_challenge", "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"),
                ("code_challenge_method", "S256"),
            ]);
            let page =
                app.browser_call(Method::POST, "/api/oauth/requests", &admin, Some(json!({"query": query})), &[]).await;
            assert_eq!(page.status, StatusCode::BAD_REQUEST, "{redirect_uri}: {}", page.text());
        }
    }
    // Home Assistant may ask for no more than its description allows.
    for scope in ["profiles", "library playlists", "requests"] {
        let refused = ask(&app, HOMEASSISTANT_APP, Some(scope), &[]).await;
        assert_eq!(oauth_error(&refused), (StatusCode::BAD_REQUEST, "invalid_scope"), "{scope}");
    }
}

#[tokio::test]
async fn a_tv_can_show_where_to_type_its_code_as_a_qr_code() {
    let app = TestApp::new().await;
    let drawn = app.call(Method::GET, "/api/oauth/device/qr?code=bcdf-ghjk", None, None, &[("host", HOST)]).await;
    assert_eq!(drawn.status, StatusCode::OK, "{}", drawn.text());
    assert_eq!(drawn.headers[header::CONTENT_TYPE], "image/png");
    assert_eq!(drawn.headers[header::CACHE_CONTROL], "no-store");
    // Module by module, the QR code of the link page with that code, in a
    // quiet zone of 4 light modules.
    let picture = image::load_from_memory(&drawn.bytes).unwrap().into_luma8();
    let expected = format!("{ISSUER}/link?code=BCDFGHJK");
    let code = qrcodegen::QrCode::encode_text(&expected, qrcodegen::QrCodeEcc::Medium).unwrap();
    let modules = code.size() + 8;
    assert_eq!(picture.width(), picture.height());
    let scale = picture.width() / modules as u32;
    assert_eq!(picture.width(), scale * modules as u32);
    for y in 0..modules {
        for x in 0..modules {
            let pixel = picture.get_pixel(x as u32 * scale + scale / 2, y as u32 * scale + scale / 2)[0];
            assert_eq!(pixel == 0, code.get_module(x - 4, y - 4), "module {x},{y}");
        }
    }
    // Only codes: nothing else can be drawn on pley's behalf.
    for asked in ["", "code=", "code=BCDF", "code=AEIO-UAEI", "code=bcdf-ghjkb", "code=https://evil.example"] {
        let refused =
            app.call(Method::GET, &format!("/api/oauth/device/qr?{asked}"), None, None, &[("host", HOST)]).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{asked}");
    }
}

#[tokio::test]
async fn metadata_lists_the_device_endpoint() {
    let app = TestApp::new().await;
    let found = metadata(&app, &[("host", HOST)]).await;
    assert_eq!(found.status, StatusCode::OK, "{}", found.text());
    assert_eq!(found.body["device_authorization_endpoint"], format!("{ISSUER}/api/oauth/device"));
    assert!(found.body["grant_types_supported"].as_array().unwrap().contains(&json!(DEVICE_GRANT)));
    // Behind a proxy, the endpoint and the page for the code follow its
    // address.
    let proxied = [("host", "pley.example.com"), ("x-forwarded-proto", "https")];
    let found = metadata(&app, &proxied).await;
    assert_eq!(found.body["device_authorization_endpoint"], "https://pley.example.com/api/oauth/device");
    let started = ask(&app, ANDROID_APP, None, &proxied).await;
    assert_eq!(started.body["verification_uri"], "https://pley.example.com/link");
    // A Host that isn't one starts nothing.
    let bad = ask(&app, ANDROID_APP, None, &[("host", "me@pley.local")]).await;
    assert_eq!(oauth_error(&bad), (StatusCode::BAD_REQUEST, "invalid_request"));
}

#[tokio::test]
async fn the_link_page_is_the_web_apps_and_cannot_be_framed() {
    let web = tempfile::tempdir().unwrap();
    std::fs::write(web.path().join("index.html"), "<!doctype html><title>pley</title>").unwrap();
    let dir = web.path().to_path_buf();
    let app = TestApp::with_config(move |config| config.web_dir = Some(dir)).await;
    let page = app.call(Method::GET, "/link?code=BCDFGHJK", None, None, &[]).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.text().contains("<title>pley</title>"), "the web app's page");
    assert_eq!(page.headers[header::X_FRAME_OPTIONS], "DENY");
    let policy = page.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
}
