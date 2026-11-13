//! Single sign-on against a stand-in OpenID Connect provider: discovery,
//! PKCE, the state bound to the browser, ID token checks, linking, new
//! accounts, the group that may sign in, the scopes pley asks for and the
//! client metadata document.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Form, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use parking_lot::Mutex;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, RsaKeyPair};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use common::{Reply, TestApp};

const PUBLIC: &str = "https://pley.example.com";
const HOST: &str = "pley.example.com";

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

/// A new Ed25519 key, as PKCS#8 (made for each run: no keys in the repository).
fn new_key() -> Vec<u8> {
    Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap().as_ref().to_vec()
}

/// The example RSA key of RFC 7515, Appendix A.2: published with the RFC,
/// so it is nobody's secret. ring can't make RSA keys, and Google's and
/// Authentik's tokens are RS256.
const RFC_RSA: [(&str, &str); 8] = [
    (
        "n",
        "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-\
         pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2Ca\
         pHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb5\
         4h4FRWyuXpoQ",
    ),
    ("e", "AQAB"),
    (
        "d",
        "Eq5xpGnNCivDflJsRQBXHx1hdR1k6Ulwe2JZD50LpXyWPEAeP88vLNO97IjlA7_GQ5sLKMgvfTeXZx9SE-7YwVol2NXOoAJe46sui395IW_GO-\
         pWJ1O0BkTGoVEn2bKVRUCgu-GjBVaYLU6f3l9kJfFNS3E0QbVdxzubSu3Mkqzjkn439X0M_V51gfpRLI9JYanrC4D4qAdGcopV_0ZHHzQlBjud\
         U2QvXt4ehNYTCBr6XCLQUShb1juUO1ZdiYoFaFQT5Tw8bGUl_x_jTj3ccPDVZFD9pIuhLhBOneufuBiB4cS98l2SR_RQyGWSeWjnczT0QU91p1\
         DhOVRuOopznQ",
    ),
    (
        "p",
        "4BzEEOtIpmVdVEZNCqS7baC4crd0pqnRH_5IB3jw3bcxGn6QLvnEtfdUdiYrqBdss1l58BQ3KhooKeQTa9AB0Hw_Py5PJdTJNPY8cQn7ouZ2KK\
         DcmnPGBY5t7yLc1QlQ5xHdwW1VhvKn-nXqhJTBgIPgtldC-KDV5z-y2XDwGUc",
    ),
    (
        "q",
        "uQPEfgmVtjL0Uyyx88GZFF1fOunH3-7cepKmtH4pxhtCoHqpWmT8YAmZxaewHgHAjLYsp1ZSe7zFYHj7C6ul7TjeLQeZD_YwD66t62wDmpe_Hl\
         B-TnBA-njbglfIsRLtXlnDzQkv5dTltRJ11BKBBypeeF6689rjcJIDEz9RWdc",
    ),
    (
        "dp",
        "BwKfV3Akq5_MFZDFZCnW-wzl-CCo83WoZvnLQwCTeDv8uzluRSnm71I3QCLdhrqE2e9YkxvuxdBfpT_PI7Yz-FOKnu1R6HsJeDCjn12Sk3vmAk\
         tV2zb34MCdy7cpdTh_YVr7tss2u6vneTwrA86rZtu5Mbr1C1XsmvkxHQAdYo0",
    ),
    (
        "dq",
        "h_96-mK1R_7glhsum81dZxjTnYynPbZpHziZjeeHcXYsXaaMwkOlODsWa7I9xXDoRwbKgB719rrmI2oKr6N3Do9U0ajaHF-NKJnwgjMd2w9cjz\
         3_-kyNlxAr2v4IKhGNpmM5iIgOS1VZnOZ68m6_pbLBSp3nssTdlqvd0tIiTHU",
    ),
    (
        "qi",
        "IYd7DHOhrWvxkwPQsRM2tOgrjbcrfvtQJipd-DlcxyVuuM9sQLdgjVk2oy26F0EmpScGLq2MowX7fhd_QJQ3ydy5cY7YIBi87w93IKLEdfnbJt\
         oOPLUW0ITrJReOgo1cq9SbsxYawBgfp_gh6A5603k2-ZQwVK0JKSHuLFkuQ3U",
    ),
];

fn rfc_rsa(name: &str) -> String {
    let value = RFC_RSA.iter().find(|(n, _)| *n == name).unwrap().1;
    value.split_whitespace().collect()
}

fn rfc_rsa_key() -> RsaKeyPair {
    let part = |name: &str| URL_SAFE_NO_PAD.decode(rfc_rsa(name)).unwrap();
    let (n, e, d, p, q, dp, dq, qi) =
        (part("n"), part("e"), part("d"), part("p"), part("q"), part("dp"), part("dq"), part("qi"));
    RsaKeyPair::from_components(&ring::rsa::KeyPairComponents {
        public_key: ring::rsa::PublicKeyComponents { n: &n[..], e: &e[..] },
        d: &d[..],
        p: &p[..],
        q: &q[..],
        dP: &dp[..],
        dQ: &dq[..],
        qInv: &qi[..],
    })
    .unwrap()
}

/// A JWS of `claims` signed by `sign`, its header naming `alg` and `kid`.
fn jws(alg: &str, kid: Option<&str>, claims: &Value, sign: impl FnOnce(&[u8]) -> Vec<u8>) -> String {
    let mut header = json!({"alg": alg, "typ": "JWT"});
    if let Some(kid) = kid {
        header["kid"] = json!(kid);
    }
    let input =
        format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
    let signature = URL_SAFE_NO_PAD.encode(sign(input.as_bytes()));
    format!("{input}.{signature}")
}

/// How the stand-in provider signs an ID token.
#[derive(Clone, Default)]
enum Signer {
    /// Its own Ed25519 key, `k1`.
    #[default]
    Provider,
    /// HS256 with this secret.
    Secret(String),
    /// Another Ed25519 key (PKCS#8), as `kid`.
    Foreign(Vec<u8>, String),
    /// RS256 with the RFC's key, `rsa`.
    Rsa,
}

/// What the stand-in provider does when pley redeems a code.
#[derive(Clone, Default)]
struct Grant {
    challenge: String,
    redirect_uri: String,
    client_id: String,
    /// The ID token's claims, over the usual ones (`iss`, `aud`, `sub`,
    /// `iat`, `exp`); `null` removes one.
    claims: Value,
    signer: Signer,
}

#[derive(Clone)]
struct Idp {
    issuer: String,
    pkcs8: Arc<Vec<u8>>,
    grants: Arc<Mutex<HashMap<String, Grant>>>,
    /// What the userinfo endpoint says.
    userinfo: Arc<Mutex<Value>>,
    /// The client credentials the token endpoint wants in HTTP Basic.
    basic: Arc<Mutex<Option<(String, String)>>>,
    cimd: bool,
}

impl Idp {
    fn grant(&self, code: &str, grant: Grant) {
        self.grants.lock().insert(code.to_owned(), grant);
    }

    fn sign_with(&self, code: &str, signer: Signer) {
        self.grants.lock().get_mut(code).unwrap().signer = signer;
    }
}

async fn discovery(State(idp): State<Idp>) -> Json<Value> {
    let base = &idp.issuer;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "userinfo_endpoint": format!("{base}/userinfo"),
        "jwks_uri": format!("{base}/jwks"),
        "id_token_signing_alg_values_supported": ["EdDSA", "HS256", "RS256"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic", "none"],
        "client_id_metadata_document_supported": idp.cimd,
    }))
}

async fn jwks(State(idp): State<Idp>) -> Json<Value> {
    let key = Ed25519KeyPair::from_pkcs8(&idp.pkcs8).unwrap();
    Json(json!({"keys": [
        // Keys pley can't use don't spoil the ones it can.
        {"kty": "EC", "crv": "P-521", "alg": "ES512", "kid": "big", "x": "AQ", "y": "AQ"},
        {"kty": "RSA", "use": "enc", "alg": "RSA-OAEP", "kid": "enc", "n": "AQAB", "e": "AQAB"},
        {"kty": "OKP", "crv": "Ed25519", "use": "sig", "alg": "EdDSA", "kid": "k1",
         "x": URL_SAFE_NO_PAD.encode(key.public_key().as_ref())},
        {"kty": "RSA", "use": "sig", "alg": "RS256", "kid": "rsa", "n": rfc_rsa("n"), "e": rfc_rsa("e")},
    ]}))
}

fn token_error(error: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error": error, "error_description": "the stand-in said no"})))
        .into_response()
}

async fn token(State(idp): State<Idp>, headers: HeaderMap, Form(form): Form<HashMap<String, String>>) -> Response {
    let Some(grant) = form.get("code").and_then(|code| idp.grants.lock().remove(code)) else {
        return token_error("invalid_grant");
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != grant.challenge {
        return token_error("invalid_grant");
    }
    if form.get("redirect_uri") != Some(&grant.redirect_uri)
        || form.get("grant_type").map(String::as_str) != Some("authorization_code")
    {
        return token_error("invalid_request");
    }
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok());
    let client = match idp.basic.lock().clone() {
        Some((id, secret)) if basic == Some(format!("{id}:{secret}")) => id,
        Some(_) => return token_error("invalid_client"),
        None => form.get("client_id").cloned().unwrap_or_default(),
    };
    if client != grant.client_id {
        return token_error("invalid_client");
    }
    let mut claims = json!({
        "iss": idp.issuer, "aud": grant.client_id, "sub": "ann-at-the-provider", "iat": now(), "exp": now() + 300,
    });
    for (name, value) in grant.claims.as_object().cloned().unwrap_or_default() {
        if value.is_null() {
            claims.as_object_mut().unwrap().remove(&name);
        } else {
            claims[name] = value;
        }
    }
    let ed25519 = |pkcs8: &[u8]| {
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8).unwrap();
        move |input: &[u8]| pair.sign(input).as_ref().to_vec()
    };
    let id_token = match grant.signer {
        Signer::Provider => jws("EdDSA", Some("k1"), &claims, ed25519(&idp.pkcs8)),
        Signer::Foreign(pkcs8, kid) => jws("EdDSA", Some(&kid), &claims, ed25519(&pkcs8)),
        Signer::Secret(secret) => jws("HS256", None, &claims, |input| {
            let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
            ring::hmac::sign(&key, input).as_ref().to_vec()
        }),
        Signer::Rsa => jws("RS256", Some("rsa"), &claims, |input| {
            let pair = rfc_rsa_key();
            let mut signature = vec![0; pair.public().modulus_len()];
            pair.sign(&ring::signature::RSA_PKCS1_SHA256, &SystemRandom::new(), input, &mut signature).unwrap();
            signature
        }),
    };
    Json(json!({"id_token": id_token, "access_token": "access-1", "token_type": "Bearer", "expires_in": 300}))
        .into_response()
}

async fn userinfo(State(idp): State<Idp>, headers: HeaderMap) -> Response {
    if headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) != Some("Bearer access-1") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(idp.userinfo.lock().clone()).into_response()
}

async fn start_idp(cimd: bool) -> Idp {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let idp = Idp {
        issuer: format!("http://{addr}"),
        pkcs8: Arc::new(new_key()),
        grants: Default::default(),
        userinfo: Arc::new(Mutex::new(json!({"sub": "ann-at-the-provider"}))),
        basic: Default::default(),
        cimd,
    };
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/token", post(token))
        .route("/userinfo", get(userinfo))
        .with_state(idp.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    idp
}

/// The value of the cookie `name` that a reply sets (`""` when it deletes it).
fn set_cookie(reply: &Reply, name: &str) -> Option<String> {
    reply.headers.get_all(header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).find_map(|cookie| {
        let (pair, _) = cookie.split_once(';').unwrap_or((cookie, ""));
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_owned())
    })
}

fn location(reply: &Reply) -> String {
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.text());
    reply.headers[header::LOCATION].to_str().unwrap().to_owned()
}

/// A sign-in on its way to the provider: what pley sent it there with.
struct Started {
    query: HashMap<String, String>,
    cookie: String,
}

impl Started {
    fn get(&self, name: &str) -> &str {
        self.query.get(name).map(String::as_str).unwrap_or_default()
    }

    /// The provider lets the person in as `claims` say (the nonce comes along).
    fn grant(&self, idp: &Idp, code: &str, mut claims: Value) {
        if claims.get("nonce").is_none() {
            claims["nonce"] = json!(self.get("nonce"));
        }
        idp.grant(
            code,
            Grant {
                challenge: self.get("code_challenge").into(),
                redirect_uri: self.get("redirect_uri").into(),
                client_id: self.get("client_id").into(),
                claims,
                ..Default::default()
            },
        );
    }
}

fn with_cookies(cookies: &str) -> Vec<(&str, &str)> {
    let mut headers = vec![("host", HOST)];
    if !cookies.is_empty() {
        headers.push(("cookie", cookies));
    }
    headers
}

async fn start(app: &TestApp, idp: &Idp, cookies: &str) -> Started {
    let reply =
        app.call(Method::GET, "/api/auth/sso/start?next=/library/nckkvbd", None, None, &with_cookies(cookies)).await;
    let to = url::Url::parse(&location(&reply)).unwrap();
    assert!(to.as_str().starts_with(&format!("{}/authorize?", idp.issuer)), "{to}");
    let cookie = set_cookie(&reply, "pley_sso").filter(|c| !c.is_empty()).expect("the sign-in's cookie");
    Started { query: to.query_pairs().into_owned().collect(), cookie }
}

/// Back from the provider with `code`.
async fn back(app: &TestApp, started: &Started, code: &str, cookies: &str) -> Reply {
    let uri = format!("/api/auth/sso/callback?code={code}&state={}", started.get("state"));
    let cookies = format!("pley_sso={}; {cookies}", started.cookie);
    app.call(Method::GET, &uri, None, None, &with_cookies(cookies.trim_end_matches("; "))).await
}

/// A sign-in the provider lets through as `claims` say, signed by `signer`:
/// where pley sends the browser afterwards.
async fn sign_in(app: &TestApp, idp: &Idp, code: &str, claims: Value, signer: Signer) -> Reply {
    let started = start(app, idp, "").await;
    started.grant(idp, code, claims);
    idp.sign_with(code, signer);
    back(app, &started, code, "").await
}

/// pley with an administrator and single sign-on through `idp` with the
/// client `pley` and the secret `s3cret`, and a second user, `ann`.
async fn world(cimd: bool) -> (TestApp, Idp, String, String, i64) {
    let idp = start_idp(cimd).await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let settings = if cimd {
        json!({"public_url": PUBLIC, "sso_enabled": true, "sso_issuer": idp.issuer, "sso_name": "Stand-in"})
    } else {
        *idp.basic.lock() = Some(("pley".into(), "s3cret".into()));
        json!({"public_url": PUBLIC, "sso_enabled": true, "sso_issuer": idp.issuer, "sso_name": "Stand-in",
               "sso_client_id": "pley", "sso_client_secret": "s3cret"})
    };
    let saved = app.put("/api/settings", &admin, settings).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    // Outside the household: no profile to pick after signing in.
    let (ann, ann_id) = app.user(&admin, "ann", json!({"home": false})).await;
    (app, idp, admin, ann, ann_id)
}

/// Asks to link the next sign-in to the user signed in with `token`, from
/// a page at `origin`.
async fn ask_to_link(app: &TestApp, token: &str, password: &str, origin: &str) -> Reply {
    let body = json!({"password": password});
    app.call(Method::POST, "/api/auth/sso/link", Some(token), Some(body), &[("host", HOST), ("origin", origin)]).await
}

/// Links the provider's `sub` to the user signed in with `token` (password
/// `test-pass-1`).
async fn link(app: &TestApp, idp: &Idp, token: &str, sub: &str) {
    link_with_password(app, idp, token, "test-pass-1", sub).await;
}

async fn link_with_password(app: &TestApp, idp: &Idp, token: &str, password: &str, sub: &str) {
    let asked = ask_to_link(app, token, password, PUBLIC).await;
    assert_eq!(asked.status, StatusCode::NO_CONTENT, "{}", asked.text());
    let request = set_cookie(&asked, "pley_sso_link").unwrap();
    let started = start(app, idp, &format!("pley_token={token}; pley_sso_link={request}")).await;
    assert_eq!(started.get("prompt"), "login", "linking signs in at the provider anew");
    let code = format!("link-{sub}");
    started.grant(idp, &code, json!({"sub": sub, "email": format!("{sub}@example.org")}));
    let linked = back(app, &started, &code, &format!("pley_token={token}")).await;
    assert_eq!(location(&linked), "/account?sso=linked");
}

async fn link_ann(app: &TestApp, idp: &Idp, ann: &str) {
    link(app, idp, ann, "ann-at-the-provider").await;
}

/// Signs in through the provider as the identity linked to ann: the session.
async fn through_provider(app: &TestApp, idp: &Idp, code: &str) -> String {
    let started = start(app, idp, "").await;
    started.grant(idp, code, json!({}));
    set_cookie(&back(app, &started, code, "").await, "pley_token").unwrap()
}

#[tokio::test]
async fn people_sign_in_with_the_identity_linked_to_their_account() {
    let (app, idp, _admin, ann, ann_id) = world(false).await;
    let server = app.call(Method::GET, "/api/server", None, None, &[]).await;
    assert_eq!(server.body["sso"], json!({"name": "Stand-in", "start": format!("{PUBLIC}/api/auth/sso/start")}));

    // Nobody is linked yet: no account.
    let started = start(&app, &idp, "").await;
    assert_eq!(started.get("client_id"), "pley");
    assert_eq!(started.get("redirect_uri"), format!("{PUBLIC}/api/auth/sso/callback"));
    assert_eq!(started.get("code_challenge_method"), "S256");
    assert_eq!(started.get("scope"), "openid email", "who signed in, and nothing more");
    assert_eq!(started.get("prompt"), "");
    started.grant(&idp, "code-1", json!({}));
    assert_eq!(location(&back(&app, &started, "code-1", "").await), "/login?sso=unknown");

    link_ann(&app, &idp, &ann).await;
    let mine = app.get("/api/me/sso", &ann).await;
    assert_eq!(mine.body["linked"], true);
    assert_eq!(mine.body["email"], "ann-at-the-provider@example.org");
    let users = app.get("/api/users", &app.login("admin", "secret-one").await).await;
    let listed = users.body.as_array().unwrap().iter().find(|u| u["id"] == ann_id).unwrap().clone();
    assert_eq!((listed["sso"].as_bool(), listed["has_password"].as_bool()), (Some(true), Some(true)));

    // Now the identity signs in to ann's account, back on the page asked for.
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "code-2", json!({}));
    let signed_in = back(&app, &started, "code-2", "").await;
    assert_eq!(location(&signed_in), "/library/nckkvbd");
    assert_eq!(set_cookie(&signed_in, "pley_sso").as_deref(), Some(""), "the sign-in's cookie goes");
    let token = set_cookie(&signed_in, "pley_token").unwrap();
    let me = app.get("/api/auth/me", &token).await;
    assert_eq!(me.body["username"], "ann");

    // Each code works once.
    let again = back(&app, &started, "code-2", "").await;
    assert_eq!(location(&again), "/login?sso=provider");

    // Members of the household pick a profile first, as after a password.
    let (_cat, _) = app.user(&app.login("admin", "secret-one").await, "cat", json!({})).await;
    link(&app, &idp, &app.login("cat", "test-pass-1").await, "cat-at-the-provider").await;
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "cat", json!({"sub": "cat-at-the-provider"}));
    assert_eq!(location(&back(&app, &started, "cat", "").await), "/profiles?next=%2Flibrary%2Fnckkvbd");
}

#[tokio::test]
async fn sign_ins_only_finish_in_the_browser_that_started_them() {
    let (app, idp, _admin, ann, _) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "code", json!({}));
    // Someone else's browser, sent the link back from the provider: no cookie.
    let elsewhere = app
        .call(
            Method::GET,
            &format!("/api/auth/sso/callback?code=code&state={}", started.get("state")),
            None,
            None,
            &[("host", HOST)],
        )
        .await;
    assert_eq!(location(&elsewhere), "/login?sso=expired");
    assert_eq!(set_cookie(&elsewhere, "pley_token"), None);
    // Another sign-in's state, or a cookie made up.
    let other = start(&app, &idp, "").await;
    let mixed = Started { query: other.query.clone(), cookie: started.cookie.clone() };
    assert_eq!(location(&back(&app, &mixed, "code", "").await), "/login?sso=expired");
    let forged = Started { query: started.query.clone(), cookie: "AAAA".repeat(30) };
    assert_eq!(location(&back(&app, &forged, "code", "").await), "/login?sso=expired");
    // The provider said no.
    let denied = app
        .call(
            Method::GET,
            &format!("/api/auth/sso/callback?error=access_denied&state={}", started.get("state")),
            None,
            None,
            &with_cookies(&format!("pley_sso={}", started.cookie)),
        )
        .await;
    assert_eq!(location(&denied), "/login?sso=denied");
}

#[tokio::test]
async fn id_tokens_must_be_for_this_client_this_sign_in_and_now() {
    let (app, idp, _admin, ann, _) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    let refused = [
        ("another nonce", json!({"nonce": "not-this-one"})),
        ("no nonce", json!({"nonce": null})),
        ("another client", json!({"aud": "someone-else"})),
        ("several audiences, issued to another", json!({"aud": ["pley", "other"], "azp": "other"})),
        ("another issuer", json!({"iss": "https://evil.example"})),
        ("expired", json!({"exp": now() - 600})),
        ("no expiry", json!({"exp": null})),
        ("not valid yet", json!({"nbf": now() + 600})),
        ("nobody", json!({"sub": ""})),
    ];
    for (n, (why, claims)) in refused.into_iter().enumerate() {
        let to = location(&sign_in(&app, &idp, &format!("bad-{n}"), claims, Signer::Provider).await);
        assert_eq!(to, "/login?sso=invalid", "{why}");
    }
    // Signed by a key the provider doesn't list (under its key's id, or another).
    for (n, kid) in ["k1", "k9"].into_iter().enumerate() {
        let signer = Signer::Foreign(new_key(), kid.into());
        let to = location(&sign_in(&app, &idp, &format!("foreign-{n}"), json!({}), signer).await);
        assert_eq!(to, "/login?sso=invalid", "{kid}");
    }
    // A code redeemed with another verifier (PKCE): the provider refuses.
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "pkce", json!({}));
    idp.grants.lock().get_mut("pkce").unwrap().challenge = "something-else".into();
    assert_eq!(location(&back(&app, &started, "pkce", "").await), "/login?sso=provider");
    // HS256 with the client secret is fine for a registered client, and
    // RS256 (Google's, Authentik's) with the provider's RSA key.
    let hs = sign_in(&app, &idp, "hs", json!({}), Signer::Secret("s3cret".into())).await;
    assert_eq!(location(&hs), "/library/nckkvbd");
    assert_eq!(location(&sign_in(&app, &idp, "rs", json!({}), Signer::Rsa).await), "/library/nckkvbd");
    let guessed = sign_in(&app, &idp, "hs-guess", json!({}), Signer::Secret("guessed".into())).await;
    assert_eq!(location(&guessed), "/login?sso=invalid");
}

/// CVE-2026-25537: in `jsonwebtoken` before 10.3, a time sent as a string
/// was read as no time at all, so an expired token passed. pley refuses it,
/// signed by the provider or not.
#[tokio::test]
async fn times_sent_as_strings_are_refused() {
    let (app, idp, _admin, ann, _) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    let refused = [
        json!({"exp": (now() - 600).to_string()}),
        json!({"exp": (now() + 300).to_string()}),
        json!({"nbf": (now() + 600).to_string()}),
        json!({"iat": now().to_string()}),
    ];
    for (n, claims) in refused.into_iter().enumerate() {
        for (signer, name) in
            [(Signer::Provider, "EdDSA"), (Signer::Rsa, "RS256"), (Signer::Secret("s3cret".into()), "HS256")]
        {
            let reply = sign_in(&app, &idp, &format!("string-{n}-{name}"), claims.clone(), signer).await;
            assert_eq!(location(&reply), "/login?sso=invalid", "{claims} ({name})");
            assert_eq!(set_cookie(&reply, "pley_token"), None);
        }
    }
}

#[tokio::test]
async fn new_accounts_start_without_libraries_and_only_when_allowed() {
    let (app, idp, admin, _ann, _) = world(false).await;
    app.post("/api/libraries", &admin, json!({"name": "Movies", "kind": "movies", "paths": [app.media_path("")]}))
        .await;
    let allowed = app.put("/api/settings", &admin, json!({"sso_new_accounts": true})).await;
    assert_eq!(allowed.status, StatusCode::OK);
    let started = start(&app, &idp, "").await;
    assert_eq!(started.get("scope"), "openid email profile", "a name for the new account");
    // The ID token says little; the userinfo endpoint the rest.
    *idp.userinfo.lock() =
        json!({"sub": "bea-at-the-provider", "preferred_username": "bea", "name": "Bea", "email": "bea@example.org"});
    started.grant(&idp, "new", json!({"sub": "bea-at-the-provider"}));
    let signed_in = back(&app, &started, "new", "").await;
    assert_eq!(location(&signed_in), "/library/nckkvbd");
    let token = set_cookie(&signed_in, "pley_token").unwrap();
    let me = app.get("/api/auth/me", &token).await.body;
    assert_eq!((me["username"].as_str(), me["name"].as_str()), (Some("bea"), Some("Bea")));
    assert_eq!((me["is_admin"].as_bool(), me["all_libraries"].as_bool()), (Some(false), Some(false)));
    assert_eq!(me["library_ids"], json!([]));
    assert_eq!((me["home"].as_bool(), me["has_password"].as_bool()), (Some(false), Some(false)));
    assert_eq!(me["request_access"], "off", "no requests until an administrator says so");
    assert_eq!(
        app.get("/api/libraries", &token).await.body,
        json!([]),
        "nothing to see until an administrator says so"
    );
    // No password: it can't be changed, and the identity can't be unlinked.
    let change =
        app.put("/api/auth/password", &token, json!({"current_password": "", "new_password": "a-new-one-1"})).await;
    assert_eq!(change.status, StatusCode::BAD_REQUEST);
    let unlink = app.call(Method::DELETE, "/api/me/sso", Some(&token), Some(json!({})), &[]).await;
    assert_eq!(unlink.status, StatusCode::FORBIDDEN);
    // Nor by an administrator, which would leave it no way in.
    let bea = me["id"].as_i64().unwrap();
    let stranded = app.delete(&format!("/api/users/{bea}/sso"), &admin).await;
    assert_eq!(stranded.status, StatusCode::CONFLICT, "{}", stranded.text());
    assert_eq!(app.get("/api/auth/me", &token).await.status, StatusCode::OK, "still linked, still signed in");
    // The same person again: the same account.
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "again", json!({"sub": "bea-at-the-provider"}));
    let token = set_cookie(&back(&app, &started, "again", "").await, "pley_token").unwrap();
    assert_eq!(app.get("/api/auth/me", &token).await.body["id"], me["id"]);
    // A disabled account stays out.
    let id = me["id"].as_i64().unwrap();
    app.patch(&format!("/api/users/{id}"), &admin, json!({"disabled": true})).await;
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "disabled", json!({"sub": "bea-at-the-provider"}));
    assert_eq!(location(&back(&app, &started, "disabled", "").await), "/login?sso=disabled");
}

#[tokio::test]
async fn only_the_group_that_may_sign_in_gets_in() {
    let (app, idp, admin, ann, _) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    app.put("/api/settings", &admin, json!({"sso_group": "pley-users"})).await;
    let started = start(&app, &idp, "").await;
    assert_eq!(started.get("scope"), "openid email groups");
    started.grant(&idp, "outsider", json!({"groups": ["family"]}));
    assert_eq!(location(&back(&app, &started, "outsider", "").await), "/login?sso=group");
    // Groups from the ID token…
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "member", json!({"groups": ["family", "pley-users"]}));
    assert_eq!(location(&back(&app, &started, "member", "").await), "/library/nckkvbd");
    // …however many there are, and wherever the one that may sign in is in
    // them (a token past 16 KiB, as from Keycloak or Authentik).
    let mut many: Vec<String> = (0..1500).map(|n| format!("project-group-{n:04}")).collect();
    many.push("pley-users".into());
    assert_eq!(location(&sign_in(&app, &idp, "many", json!({"groups": many}), Signer::Rsa).await), "/library/nckkvbd");
    // …or, when it has none, from the userinfo endpoint (Keycloak's paths too).
    *idp.userinfo.lock() = json!({"sub": "ann-at-the-provider", "groups": ["/pley-users"]});
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "userinfo", json!({}));
    assert_eq!(location(&back(&app, &started, "userinfo", "").await), "/library/nckkvbd");
    // Userinfo about someone else is ignored.
    *idp.userinfo.lock() = json!({"sub": "someone-else", "groups": ["pley-users"]});
    let started = start(&app, &idp, "").await;
    started.grant(&idp, "mixup", json!({}));
    assert_eq!(location(&back(&app, &started, "mixup", "").await), "/login?sso=group");
}

#[tokio::test]
async fn linking_takes_the_password_and_the_public_address() {
    let (app, idp, admin, ann, ann_id) = world(false).await;
    let wrong = ask_to_link(&app, &ann, "wrong", PUBLIC).await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN, "a session alone isn't enough");
    let elsewhere = ask_to_link(&app, &ann, "test-pass-1", "http://192.168.1.20:7539").await;
    assert_eq!(elsewhere.status, StatusCode::CONFLICT);
    assert!(elsewhere.text().contains(PUBLIC), "{}", elsewhere.text());

    // A link request only works for the user who asked.
    let asked = ask_to_link(&app, &ann, "test-pass-1", PUBLIC).await;
    let link = set_cookie(&asked, "pley_sso_link").unwrap();
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let as_bob = app
        .call(
            Method::GET,
            "/api/auth/sso/start",
            None,
            None,
            &with_cookies(&format!("pley_token={bob}; pley_sso_link={link}")),
        )
        .await;
    assert_eq!(location(&as_bob), "/account?sso=signin");

    link_ann(&app, &idp, &ann).await;
    // Bob can't take ann's identity.
    let asked = ask_to_link(&app, &bob, "test-pass-1", PUBLIC).await;
    let link = set_cookie(&asked, "pley_sso_link").unwrap();
    let started = start(&app, &idp, &format!("pley_token={bob}; pley_sso_link={link}")).await;
    started.grant(&idp, "steal", json!({}));
    assert_eq!(location(&back(&app, &started, "steal", &format!("pley_token={bob}")).await), "/account?sso=taken");

    // Unlinking takes the password too, and signs out whoever got in
    // through the provider, keeping the session that asked.
    let elsewhere = through_provider(&app, &idp, "elsewhere").await;
    assert_eq!(app.get("/api/auth/me", &elsewhere).await.body["username"], "ann");
    let unlink = |password: &'static str| {
        app.call(Method::DELETE, "/api/me/sso", Some(&ann), Some(json!({"password": password})), &[])
    };
    assert_eq!(unlink("wrong").await.status, StatusCode::FORBIDDEN);
    assert_eq!(unlink("test-pass-1").await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/me/sso", &ann).await.body["linked"], false, "the session that asked stays");
    assert_eq!(app.get("/api/auth/me", &elsewhere).await.status, StatusCode::UNAUTHORIZED, "signed out");

    // Only administrators unlink others, which signs the user out
    // everywhere (an account taken over at the provider).
    link_ann(&app, &idp, &ann).await;
    let elsewhere = through_provider(&app, &idp, "taken-over").await;
    assert_eq!(app.delete(&format!("/api/users/{ann_id}/sso"), &ann).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/users/{ann_id}/sso"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &elsewhere).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &ann).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn codes_the_provider_refuses_are_counted() {
    let (app, idp, _admin, ann, _) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    // One sign-in's cookie, sent back again and again with made-up codes.
    let started = start(&app, &idp, "").await;
    for n in 0..5 {
        assert_eq!(location(&back(&app, &started, &format!("made-up-{n}"), "").await), "/login?sso=provider");
    }
    assert_eq!(location(&back(&app, &started, "made-up-5", "").await), "/login?sso=busy");
    // Real codes wait too: the provider isn't asked at all.
    started.grant(&idp, "real", json!({}));
    assert_eq!(location(&back(&app, &started, "real", "").await), "/login?sso=busy");
    assert!(idp.grants.lock().contains_key("real"), "the code wasn't sent");
}

#[tokio::test]
async fn linking_an_account_with_two_step_sign_in_takes_its_code() {
    // Signing in through the provider skips pley's second step.
    let (app, _idp, _admin, ann, _) = world(false).await;
    let (secret, _) = app.turn_on_two_step(&ann, "test-pass-1").await;
    let asked = ask_to_link(&app, &ann, "test-pass-1", PUBLIC).await;
    assert_eq!((asked.status, asked.body["code"].clone()), (StatusCode::UNAUTHORIZED, json!("two_step")));
    assert!(set_cookie(&asked, "pley_sso_link").is_none());
    let code = pley::two_step::code_at(&secret, pley::util::now() + 30);
    let body = json!({"password": "test-pass-1", "code": code});
    let headers = [("host", HOST), ("origin", PUBLIC)];
    let asked = app.call(Method::POST, "/api/auth/sso/link", Some(&ann), Some(body), &headers).await;
    assert_eq!(asked.status, StatusCode::NO_CONTENT, "{}", asked.text());
    assert!(set_cookie(&asked, "pley_sso_link").is_some());
}

#[tokio::test]
async fn an_account_that_must_set_up_two_step_sign_in_does_so_with_its_password() {
    let (app, idp, admin, ann, _) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    // The server comes to require two-step sign-in, which ann doesn't use.
    app.turn_on_two_step(&admin, "secret-one").await;
    let required = app.put("/api/settings", &admin, json!({"require_two_step": true})).await;
    assert_eq!(required.status, StatusCode::OK, "{}", required.text());
    let reply = sign_in(&app, &idp, "required", json!({}), Signer::Provider).await;
    assert_eq!(location(&reply), "/login?sso=two_step");
    assert_eq!(set_cookie(&reply, "pley_token"), None, "no session that would be refused anyway");
}

#[tokio::test]
async fn sign_ins_move_to_the_public_address() {
    let (app, _idp, _admin, _ann, _) = world(false).await;
    // From the address on the local network, to the public one, once.
    let local =
        app.call(Method::GET, "/api/auth/sso/start?next=/library", None, None, &[("host", "192.168.1.20:7539")]).await;
    assert_eq!(location(&local), format!("{PUBLIC}/api/auth/sso/start?next=%2Flibrary&hop=1"));
    assert_eq!(set_cookie(&local, "pley_sso"), None);
    // A proxy that rewrites Host doesn't make it go round in circles.
    let hopped = app
        .call(Method::GET, "/api/auth/sso/start?next=/library&hop=1", None, None, &[("host", "127.0.0.1:7539")])
        .await;
    assert!(location(&hopped).contains("/authorize?"));
    // Pages elsewhere aren't where people go afterwards.
    let away = app.call(Method::GET, "/api/auth/sso/start?next=//evil.example", None, None, &[("host", HOST)]).await;
    assert!(set_cookie(&away, "pley_sso").is_some());
}

#[tokio::test]
async fn providers_that_take_client_metadata_documents_need_no_registration() {
    let (app, idp, admin, ann, _) = world(true).await;
    let doc = app.call(Method::GET, "/api/auth/sso/client", None, None, &[]).await;
    assert_eq!(doc.status, StatusCode::OK);
    assert_eq!(doc.body["client_id"], format!("{PUBLIC}/api/auth/sso/client"));
    assert_eq!(doc.body["redirect_uris"], json!([format!("{PUBLIC}/api/auth/sso/callback")]));
    assert_eq!(
        (doc.body["token_endpoint_auth_method"].as_str(), doc.body["scope"].as_str()),
        (Some("none"), Some("openid email"))
    );
    link_ann(&app, &idp, &ann).await;
    let started = start(&app, &idp, "").await;
    assert_eq!(started.get("client_id"), format!("{PUBLIC}/api/auth/sso/client"));
    started.grant(&idp, "cimd", json!({}));
    assert_eq!(location(&back(&app, &started, "cimd", "").await), "/library/nckkvbd");
    // A public client has no secret to check an HS256 token with.
    let hs = sign_in(&app, &idp, "hs", json!({}), Signer::Secret("guessable".into())).await;
    assert_eq!(location(&hs), "/login?sso=invalid");
    // With a client ID, there is no document.
    app.put("/api/settings", &admin, json!({"sso_client_id": "pley"})).await;
    assert_eq!(app.call(Method::GET, "/api/auth/sso/client", None, None, &[]).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_settings_test_the_provider_and_keep_the_secret() {
    let (app, idp, admin, ann, _) = world(false).await;
    let test = |body: Value| app.post("/api/settings/test", &admin, body);
    let found = test(json!({"service": "sso"})).await;
    assert_eq!(found.body["ok"], true, "{}", found.text());
    assert!(found.body["message"].as_str().unwrap().contains("/api/auth/sso/callback"), "{}", found.text());
    let cimd = test(json!({"service": "sso", "client_id": ""})).await;
    assert_eq!(cimd.body["ok"], false, "this provider doesn't take client metadata documents");
    let nowhere = test(json!({"service": "sso", "issuer": "http://127.0.0.1:9"})).await;
    assert_eq!(nowhere.body["ok"], false);
    let typo = test(json!({"service": "sso", "issuer": format!("{}/", idp.issuer)})).await;
    assert!(typo.body["message"].as_str().unwrap().contains("enter it exactly"), "{}", typo.text());
    assert_eq!(app.post("/api/settings/test", &ann, json!({"service": "sso"})).await.status, StatusCode::FORBIDDEN);

    let settings = app.get("/api/settings", &admin).await;
    assert_eq!(settings.body.get("sso_client_secret"), None);
    assert_eq!(settings.body["sso_client_secret_set"], true);
    assert!(!settings.mentions("s3cret"));
    // Saving other settings keeps it.
    app.put("/api/settings", &admin, json!({"sso_name": "Home"})).await;
    assert_eq!(app.get("/api/settings", &admin).await.body["sso_client_secret_set"], true);
}

/// Signs in through the provider as `sub` with `claims`: the session.
async fn session_as(app: &TestApp, idp: &Idp, code: &str, sub: &str, mut claims: Value) -> String {
    claims["sub"] = json!(sub);
    let reply = sign_in(app, idp, code, claims, Signer::Provider).await;
    set_cookie(&reply, "pley_token").unwrap_or_else(|| panic!("not signed in: {}", location(&reply)))
}

async fn is_admin(app: &TestApp, token: &str) -> bool {
    app.get("/api/auth/me", token).await.body["is_admin"].as_bool().unwrap()
}

#[tokio::test]
async fn a_claim_makes_administrators_at_each_sign_in() {
    let (app, idp, admin, ann, ann_id) = world(false).await;
    link_ann(&app, &idp, &ann).await;
    let set =
        app.put("/api/settings", &admin, json!({"sso_admin_claim": "groups", "sso_admin_value": "pley-admins"})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    let started = start(&app, &idp, "").await;
    assert_eq!(started.get("scope"), "openid email groups", "the groups to find administrators in");

    // In the group: an administrator, at once.
    let ann_session =
        session_as(&app, &idp, "in", "ann-at-the-provider", json!({"groups": ["family", "/pley-admins"]})).await;
    assert!(is_admin(&app, &ann_session).await);
    assert_eq!(app.get("/api/users", &ann_session).await.status, StatusCode::OK);
    // Signing in otherwise doesn't change it: ann's password still signs in.
    assert!(is_admin(&app, &app.login("ann", "test-pass-1").await).await);

    // Out of the group, or without the claim at all: no longer, and the
    // sessions from before lose it too.
    session_as(&app, &idp, "out", "ann-at-the-provider", json!({"groups": ["family"]})).await;
    assert!(!is_admin(&app, &ann_session).await);
    assert_eq!(app.get("/api/users", &ann_session).await.status, StatusCode::FORBIDDEN);
    session_as(&app, &idp, "in-again", "ann-at-the-provider", json!({"groups": ["pley-admins"]})).await;
    session_as(&app, &idp, "missing", "ann-at-the-provider", json!({})).await;
    let users = app.get("/api/users", &admin).await;
    let listed = users.body.as_array().unwrap().iter().find(|u| u["id"] == ann_id).unwrap().clone();
    assert_eq!(listed["is_admin"], false, "a claim missing makes no administrator");

    // Turned off, the provider no longer decides.
    app.patch(&format!("/api/users/{ann_id}"), &admin, json!({"is_admin": true})).await;
    app.put("/api/settings", &admin, json!({"sso_admin_claim": ""})).await;
    assert!(is_admin(&app, &session_as(&app, &idp, "off", "ann-at-the-provider", json!({})).await).await);
}

#[tokio::test]
async fn the_last_administrator_stays_one() {
    let (app, idp, admin, ann, _) = world(false).await;
    link_with_password(&app, &idp, &admin, "secret-one", "admin-at-the-provider").await;
    link_ann(&app, &idp, &ann).await;
    app.put("/api/settings", &admin, json!({"sso_admin_claim": "groups", "sso_admin_value": "pley-admins"})).await;
    // The only administrator, signing in without the group: still one.
    let admin_session = session_as(&app, &idp, "only", "admin-at-the-provider", json!({"groups": []})).await;
    assert!(is_admin(&app, &admin_session).await);
    // Once someone else is, the provider decides for this account too.
    session_as(&app, &idp, "ann", "ann-at-the-provider", json!({"groups": ["pley-admins"]})).await;
    let admin_session = session_as(&app, &idp, "now", "admin-at-the-provider", json!({"groups": []})).await;
    assert!(!is_admin(&app, &admin_session).await);
}

#[tokio::test]
async fn administrators_come_from_nested_claims_userinfo_and_new_accounts() {
    let (app, idp, admin, _ann, _) = world(false).await;
    let set = app
        .put(
            "/api/settings",
            &admin,
            json!({"sso_new_accounts": true, "sso_admin_claim": "realm_access.roles", "sso_admin_value": "pley-admin"}),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    // Keycloak's roles, from the userinfo endpoint when the ID token has none.
    *idp.userinfo.lock() = json!({
        "sub": "dee-at-the-provider", "preferred_username": "dee",
        "realm_access": {"roles": ["offline_access", "pley-admin"]},
    });
    let dee = session_as(&app, &idp, "dee", "dee-at-the-provider", json!({})).await;
    let me = app.get("/api/auth/me", &dee).await.body;
    assert_eq!((me["username"].as_str(), me["is_admin"].as_bool()), (Some("dee"), Some(true)), "made an administrator");
    // Userinfo about someone else says nothing about who administers.
    *idp.userinfo.lock() = json!({"sub": "someone-else", "realm_access": {"roles": ["pley-admin"]}});
    let eve = session_as(&app, &idp, "eve", "eve-at-the-provider", json!({"preferred_username": "eve"})).await;
    assert!(!is_admin(&app, &eve).await);
    // A claim true, with no value to look for.
    app.put("/api/settings", &admin, json!({"sso_admin_claim": "pley_admin", "sso_admin_value": ""})).await;
    let eve = session_as(&app, &idp, "eve-true", "eve-at-the-provider", json!({"pley_admin": true})).await;
    assert!(is_admin(&app, &eve).await);
    let eve = session_as(&app, &idp, "eve-string", "eve-at-the-provider", json!({"pley_admin": "true"})).await;
    assert!(!is_admin(&app, &eve).await, "a string isn't the claim true");
    // Names that aren't claims' are refused.
    for bad in ["realm_access..roles", ".roles", "roles.", "my roles"] {
        let refused = app.put("/api/settings", &admin, json!({"sso_admin_claim": bad})).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // So are claims that say who someone is: people choose a username or an
    // address, and nearly anyone's address is verified.
    for who in ["email_verified", "email", "preferred_username", "name", "sub"] {
        let refused = app.put("/api/settings", &admin, json!({"sso_admin_claim": who, "sso_admin_value": ""})).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{who}");
        assert!(refused.text().contains("groups or roles"), "{}", refused.text());
    }
    let kept = app.get("/api/settings", &admin).await;
    assert_eq!(kept.body["sso_admin_claim"], "pley_admin", "the claim before stays");
}
