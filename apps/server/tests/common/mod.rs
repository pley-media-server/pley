//! An in-process pley for the integration tests that use it (tests/api.rs
//! and friends have their own copies): a temporary data directory, the pley
//! API and the Plex- and Jellyfin-compatible APIs on the same state, fake
//! media files. No transcoder runs unless a test points the config at one.

#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use pley::{AppState, Config};

#[path = "../../src/testing.rs"]
mod testing;
pub use testing::tempdir;

/// The client_id of pley's Android app.
pub const ANDROID_APP: &str = "https://pley-media-server.github.io/pley/oauth/android.json";
/// The client_id of pley's desktop app.
pub const DESKTOP_APP: &str = "https://pley-media-server.github.io/pley/oauth/desktop.json";
/// The client_id of pley's Apple app on iPhone and iPad.
pub const IOS_APP: &str = "https://pley-media-server.github.io/pley/oauth/ios.json";
/// The client_id of pley's Kodi add-on.
pub const KODI_APP: &str = "https://pley-media-server.github.io/pley/oauth/kodi.json";
/// The client_id of pley's Apple app on the Mac.
pub const MACOS_APP: &str = "https://pley-media-server.github.io/pley/oauth/macos.json";
/// The client_id of pley's Roku app.
pub const ROKU_APP: &str = "https://pley-media-server.github.io/pley/oauth/roku.json";
/// The client_id of pley's Apple app on Apple TV.
pub const TVOS_APP: &str = "https://pley-media-server.github.io/pley/oauth/tvos.json";
/// The client_id of pley's LG TV app.
pub const WEBOS_APP: &str = "https://pley-media-server.github.io/pley/oauth/webos.json";
/// The Host the tests' browsers and apps reach pley at.
pub const HOST: &str = "pley.local:7539";
/// The issuer for [`HOST`].
pub const ISSUER: &str = "http://pley.local:7539";

pub struct TestApp {
    pub api: Router,
    /// The Plex-compatible API on a port of its own.
    pub plex: pley::plex::Service,
    /// The Jellyfin-compatible API on a port of its own.
    pub jellyfin: pley::jellyfin::Service,
    pub state: AppState,
    pub media: tempfile::TempDir,
    _data: tempfile::TempDir,
    plex_tokens: PlexTokens,
    jellyfin_tokens: JellyfinTokens,
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
    pub bytes: Vec<u8>,
}

impl Reply {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    /// Whether the raw response mentions `needle` anywhere (leak checks).
    pub fn mentions(&self, needle: &str) -> bool {
        self.text().contains(needle)
    }
}

async fn reply(resp: axum::response::Response) -> Reply {
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024 * 1024).await.unwrap().to_vec();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply { status, headers, body, bytes }
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    /// A pley whose configuration `adjust` may change (a transcoder URL, …).
    pub async fn with_config(adjust: impl FnOnce(&mut Config)) -> Self {
        let data = tempdir();
        let mut config = Config::for_data_dir(data.path());
        config.transcoder_path = Some(data.path().join("no-transcoder-in-tests"));
        adjust(&mut config);
        let state = AppState::init(config).await.unwrap();
        let api = pley::api::router(state.clone());
        let plex = pley::plex::service(state.clone());
        let jellyfin = pley::jellyfin::service(state.clone());
        Self {
            api,
            plex,
            jellyfin,
            state,
            media: tempfile::tempdir().unwrap(),
            _data: data,
            plex_tokens: Default::default(),
            jellyfin_tokens: Default::default(),
        }
    }

    /// A new pley on the same data folder, as after a restart: what was
    /// saved stays, and what it keeps in memory starts over.
    pub async fn restart(&mut self) {
        self.state = AppState::init(self.state.config.clone()).await.unwrap();
        self.api = pley::api::router(self.state.clone());
        self.plex = pley::plex::service(self.state.clone());
        self.jellyfin = pley::jellyfin::service(self.state.clone());
    }

    pub fn media_file(&self, rel: &str, bytes: usize) {
        let path = self.media.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; bytes]).unwrap();
    }

    pub fn media_path(&self, rel: &str) -> String {
        self.media.path().join(rel).to_string_lossy().into_owned()
    }

    pub async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        reply(self.api.clone().oneshot(req).await.unwrap()).await
    }

    /// A GET without a token from `peer` (the other end of the connection),
    /// with `extra` headers (a Host, a proxy's X-Forwarded-*).
    pub async fn get_from(&self, peer: &str, uri: &str, extra: &[(&str, &str)]) -> Reply {
        let mut req = Request::builder().method(Method::GET).uri(uri);
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let req = req.extension(axum::extract::ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()));
        reply(self.api.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()).await
    }

    pub async fn get(&self, uri: &str, token: &str) -> Reply {
        self.call(Method::GET, uri, Some(token), None, &[]).await
    }

    pub async fn post(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::POST, uri, Some(token), Some(body), &[]).await
    }

    pub async fn put(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::PUT, uri, Some(token), Some(body), &[]).await
    }

    pub async fn patch(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::PATCH, uri, Some(token), Some(body), &[]).await
    }

    pub async fn delete(&self, uri: &str, token: &str) -> Reply {
        self.call(Method::DELETE, uri, Some(token), None, &[]).await
    }

    /// A compatibility token of the account signed in with `session`, made
    /// on its account page, that may do `scope`.
    pub async fn compatibility_token(&self, session: &str, scope: &str) -> String {
        compatibility_token(&self.api, session, scope).await
    }

    /// The token the Plex-compatible API is sent for `token` (see
    /// [`PlexTokens`]).
    pub async fn plex_token(&self, token: &str) -> String {
        self.plex_tokens.get(&self.state, &self.api, token).await
    }

    /// The Plex-compatible API, asking for JSON with the token as Plex
    /// clients send it ([`TestApp::plex_token`]'s for a session).
    pub async fn plex(&self, method: Method, uri: &str, token: &str, extra: &[(&str, &str)]) -> Reply {
        let token = self.plex_token(token).await;
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-plex-token", token)
            .header(header::ACCEPT, "application/json");
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        // On pley's port, as by default: `plex` is the API on its own.
        reply(self.api.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()).await
    }

    /// A Plex-compatible API request with `body` (an upload).
    pub async fn plex_upload(&self, method: Method, uri: &str, token: &str, body: Vec<u8>) -> Reply {
        let token = self.plex_token(token).await;
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("x-plex-token", token)
            .header(header::ACCEPT, "application/json")
            .body(Body::from(body))
            .unwrap();
        reply(self.api.clone().oneshot(req).await.unwrap()).await
    }

    /// `MediaContainer` of a successful Plex request.
    pub async fn plex_json(&self, uri: &str, token: &str) -> Value {
        let reply = self.plex(Method::GET, uri, token, &[]).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text());
        reply.body["MediaContainer"].clone()
    }

    /// The token the Jellyfin-compatible API is sent for `token` (see
    /// [`JellyfinTokens`]).
    pub async fn jellyfin_token(&self, token: &str) -> String {
        self.jellyfin_tokens.get(&self.state, &self.api, token).await
    }

    /// The Jellyfin-compatible API on its own port, with the token in the
    /// header Jellyfin's apps send ([`TestApp::jellyfin_token`]'s for a
    /// session; none for `""`), and a JSON `body`.
    pub async fn jf(&self, method: Method, uri: &str, token: &str, body: Option<Value>) -> Reply {
        let device = r#"Client="Test", Device="Bench", DeviceId="bench-1", Version="1.0""#;
        let authorization = if token.is_empty() {
            format!("MediaBrowser {device}")
        } else {
            format!(r#"MediaBrowser {device}, Token="{}""#, self.jellyfin_token(token).await)
        };
        self.jf_raw(method, uri, &[("authorization", &authorization)], body).await
    }

    /// The Jellyfin-compatible API with these headers alone (tokens as they
    /// are) and a JSON `body`.
    pub async fn jf_raw(&self, method: Method, uri: &str, headers: &[(&str, &str)], body: Option<Value>) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        reply(self.jellyfin.clone().oneshot(req).await.unwrap()).await
    }

    /// The body of a successful Jellyfin request.
    pub async fn jf_json(&self, uri: &str, token: &str) -> Value {
        let reply = self.jf(Method::GET, uri, token, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text());
        reply.body
    }

    /// The ids the Jellyfin-compatible API gives this pley's items, users…
    pub fn jf_ids(&self) -> pley::jellyfin::ids::Ids {
        let key = std::fs::read_to_string(self.state.config.data_dir.join("secret.key")).unwrap();
        pley::jellyfin::ids::Ids::new(&hex::decode(key.trim()).unwrap())
    }

    /// The Jellyfin id of item `id`.
    pub fn jf_item(&self, id: i64) -> String {
        self.jf_ids().of(pley::jellyfin::ids::Kind::Item, id)
    }

    /// The Jellyfin id of library `id`.
    pub fn jf_library(&self, id: i64) -> String {
        self.jf_ids().of(pley::jellyfin::ids::Kind::Library, id)
    }

    /// The Jellyfin id of the user `token` signs in.
    pub async fn jf_user(&self, token: &str) -> String {
        let user = self.state.auth.validate_compatibility(&self.jellyfin_token(token).await, None).await.unwrap();
        self.jf_ids().of(pley::jellyfin::ids::Kind::User, user.id)
    }

    /// The id of `file` as a media source listed to the sign-in of `token`.
    pub async fn jf_source(&self, file: i64, token: &str) -> String {
        let user = self.state.auth.validate_compatibility(&self.jellyfin_token(token).await, None).await.unwrap();
        self.jf_ids().source(file, user.compatibility.unwrap().token_id)
    }

    /// A pley API request with a raw `body` of `content_type` (a form).
    pub async fn send_body(&self, method: Method, uri: &str, token: &str, content_type: &str, body: Vec<u8>) -> Reply {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body))
            .unwrap();
        reply(self.api.clone().oneshot(req).await.unwrap()).await
    }

    pub async fn setup_admin(&self) -> String {
        let r = self
            .call(
                Method::POST,
                "/api/setup",
                None,
                Some(
                    json!({"username": "admin", "password": "secret-one", "setup_code": self.state.auth.setup.shown()}),
                ),
                &[],
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        r.body["token"].as_str().unwrap().to_owned()
    }

    pub async fn try_login(&self, username: &str, password: &str) -> Reply {
        self.call(Method::POST, "/api/auth/login", None, Some(json!({"username": username, "password": password})), &[])
            .await
    }

    pub async fn login(&self, username: &str, password: &str) -> String {
        let r = self.try_login(username, password).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        r.body["token"].as_str().unwrap().to_owned()
    }

    /// Turns on two-step sign-in for the user signed in with `token`: the
    /// authenticator app's secret, and the recovery codes.
    pub async fn turn_on_two_step(&self, token: &str, password: &str) -> (Vec<u8>, Vec<String>) {
        let setup = self.post("/api/me/two-step/setup", token, json!({"password": password})).await;
        assert_eq!(setup.status, StatusCode::OK, "{}", setup.text());
        let secret = pley::two_step::base32_decode(setup.body["secret"].as_str().unwrap()).unwrap();
        let code = pley::two_step::code_at(&secret, pley::util::now());
        let on = self.post("/api/me/two-step", token, json!({"code": code})).await;
        assert_eq!(on.status, StatusCode::OK, "{}", on.text());
        let codes = on.body["recovery_codes"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_owned());
        (secret, codes.collect())
    }

    /// A user with the password `test-pass-1` and the given fields, signed in:
    /// `(token, id)`.
    pub async fn user(&self, admin: &str, username: &str, mut fields: Value) -> (String, i64) {
        fields["username"] = json!(username);
        fields["password"] = json!("test-pass-1");
        let created = self.post("/api/users", admin, fields).await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
        (self.login(username, "test-pass-1").await, created.body["id"].as_i64().unwrap())
    }

    pub async fn wait_for_scans(&self) {
        for _ in 0..400 {
            if self.state.scanner.statuses().is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("scan did not finish");
    }

    /// A library of the media folder `rel`, scanned.
    pub async fn library(&self, token: &str, name: &str, kind: &str, rel: &str) -> i64 {
        std::fs::create_dir_all(self.media.path().join(rel)).unwrap();
        let r = self
            .post("/api/libraries", token, json!({"name": name, "kind": kind, "paths": [self.media_path(rel)]}))
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
        self.wait_for_scans().await;
        r.body["id"].as_i64().unwrap()
    }

    /// Title → id of the top-level items of a library.
    pub async fn ids(&self, library: i64, token: &str) -> HashMap<String, i64> {
        let page = self.get(&format!("/api/libraries/{library}/items"), token).await;
        assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
        page.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| (i["title"].as_str().unwrap().to_owned(), i["id"].as_i64().unwrap()))
            .collect()
    }

    /// Tokens of pley's Android app acting as `user_id`, allowed `scope` by
    /// that user: `(access token, refresh token)`.
    pub async fn app_token(&self, user_id: i64, scope: &str) -> (String, String) {
        let issued = self
            .state
            .auth
            .issue(pley::auth::grants::NewGrant {
                user_id,
                approved_by: user_id,
                client_id: ANDROID_APP.into(),
                client_name: Some("pley".into()),
                scopes: pley::auth::scopes::Scopes::parse(scope).unwrap(),
                refresh: true,
                first_party: true,
                session_hash: None,
                user_agent: Some("pley-android/1.0 okhttp/5".into()),
                ip: Some("10.0.0.9".into()),
            })
            .await
            .unwrap();
        (issued.access_token, issued.refresh_token.unwrap())
    }

    /// A pley API request as apps make it: the token in `Authorization:
    /// Bearer`, JSON bodies.
    pub async fn app_call(&self, method: Method, uri: &str, token: &str, body: Option<Value>) -> Reply {
        self.call(method, uri, Some(token), body, &[]).await
    }

    /// A request from pley's own page in a browser: the session cookie,
    /// Host [`HOST`], Origin [`ISSUER`] and `Sec-Fetch-Site: same-origin`,
    /// and a JSON body. `extra` headers replace those; an empty value
    /// leaves one out.
    pub async fn browser_call(
        &self,
        method: Method,
        uri: &str,
        cookie: &str,
        body: Option<Value>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, format!("theme=dark; pley_token={cookie}").parse().unwrap());
        headers.insert(header::HOST, HOST.parse().unwrap());
        headers.insert(header::ORIGIN, ISSUER.parse().unwrap());
        headers.insert("sec-fetch-site", "same-origin".parse().unwrap());
        for (name, value) in extra {
            let name = header::HeaderName::from_bytes(name.as_bytes()).unwrap();
            if value.is_empty() {
                headers.remove(&name);
            } else {
                headers.insert(name, value.parse().unwrap());
            }
        }
        let mut req = Request::builder().method(method).uri(uri);
        for (name, value) in &headers {
            req = req.header(name, value);
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        reply(self.api.clone().oneshot(req).await.unwrap()).await
    }

    /// A form POSTed as apps do to the token and revocation endpoints
    /// (`form` already encoded), to Host [`HOST`] with the Android app's
    /// User-Agent. `extra` headers replace those.
    pub async fn form_post(&self, uri: &str, form: &str, extra: &[(&str, &str)]) -> Reply {
        self.form_post_with(None, uri, form, extra).await
    }

    /// [`TestApp::form_post`] over a connection from `peer` (an IP address
    /// and port), as the server sees it.
    pub async fn form_post_from(&self, peer: &str, uri: &str, form: &str, extra: &[(&str, &str)]) -> Reply {
        self.form_post_with(Some(peer.parse().unwrap()), uri, form, extra).await
    }

    async fn form_post_with(
        &self,
        peer: Option<std::net::SocketAddr>,
        uri: &str,
        form: &str,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HOST.parse().unwrap());
        headers.insert(header::CONTENT_TYPE, "application/x-www-form-urlencoded".parse().unwrap());
        headers.insert(header::USER_AGENT, "pley-android/1.0 okhttp/5".parse().unwrap());
        for (name, value) in extra {
            headers.insert(header::HeaderName::from_bytes(name.as_bytes()).unwrap(), value.parse().unwrap());
        }
        let mut req = Request::builder().method(Method::POST).uri(uri);
        for (name, value) in &headers {
            req = req.header(name, value);
        }
        if let Some(peer) = peer {
            req = req.extension(axum::extract::ConnectInfo(peer));
        }
        reply(self.api.clone().oneshot(req.body(Body::from(form.to_owned())).unwrap()).await.unwrap()).await
    }

    /// Ids of the children (seasons, episodes, albums, tracks) of an item, by title.
    pub async fn children(&self, id: i64, token: &str) -> HashMap<String, i64> {
        let page = self.get(&format!("/api/items/{id}/children"), token).await;
        assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
        page.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| (i["title"].as_str().unwrap().to_owned(), i["id"].as_i64().unwrap()))
            .collect()
    }

    /// What the metadata agent stores for a content rating.
    pub async fn set_rating(&self, title: &str, rating: Option<&str>) {
        sqlx::query("UPDATE items SET content_rating = ?, rating_age = ? WHERE title = ?")
            .bind(rating)
            .bind(rating.and_then(pley::ratings::rating_age))
            .bind(title)
            .execute(&self.state.db.write)
            .await
            .unwrap();
    }

    /// Id of the first media file of an item.
    pub async fn file_of(&self, item: i64) -> i64 {
        sqlx::query_scalar("SELECT id FROM media_files WHERE item_id = ? ORDER BY id")
            .bind(item)
            .fetch_one(&self.state.db.read)
            .await
            .unwrap()
    }
}

/// A compatibility token of the account signed in with `session`, made
/// through `api` as its account page makes one, that may do `scope`.
pub async fn compatibility_token(api: &Router, session: &str, scope: &str) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/me/compatibility-tokens")
        .header(header::AUTHORIZATION, format!("Bearer {session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"name": "Tests", "scope": scope}).to_string()))
        .unwrap();
    let made = reply(api.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    made.body["token"].as_str().unwrap().to_owned()
}

/// The compatibility tokens that tests send the Jellyfin-compatible API for
/// the sessions they signed in with. A session's stands for a token of its
/// account that may play, as a Jellyfin sign-in gets, made once and kept as
/// an app keeps its token: when the session ends, requests go on with the
/// kept token, which ended with it. A new one is made only while the
/// session works and the kept one doesn't (a new password ends the
/// account's tokens but not the session that set it). Any other token goes
/// as it is.
#[derive(Default)]
pub struct JellyfinTokens(std::sync::Mutex<HashMap<String, String>>);

impl JellyfinTokens {
    /// The token to send for `token`, of the pley of `state` and `api`: a
    /// token for other apps that may play, as a Jellyfin sign-in gets, for
    /// a session; any other token as it is.
    pub async fn get(&self, state: &AppState, api: &Router, token: &str) -> String {
        if token.starts_with("pley_") {
            return token.to_owned();
        }
        let kept = self.0.lock().unwrap().get(token).cloned();
        if state.auth.validate(token).await.is_none() {
            return kept.unwrap_or_else(|| token.to_owned());
        }
        if let Some(kept) = kept
            && state.auth.validate_compatibility(&kept, None).await.is_some()
        {
            return kept;
        }
        let made = compatibility_token(api, token, "play").await;
        self.0.lock().unwrap().insert(token.to_owned(), made.clone());
        made
    }
}

/// The tokens tests send the Plex-compatible API, as [`JellyfinTokens`],
/// but a session stands for one with every right its account has (`play`,
/// and `activity manage` for an administrator), as a tool's would.
#[derive(Default)]
pub struct PlexTokens(std::sync::Mutex<HashMap<String, (bool, String)>>);

impl PlexTokens {
    /// The token to send for `token`, of the pley of `state` and `api`.
    pub async fn get(&self, state: &AppState, api: &Router, token: &str) -> String {
        if token.starts_with("pley_") {
            return token.to_owned();
        }
        let kept = self.0.lock().unwrap().get(token).cloned();
        let Some(user) = state.auth.validate(token).await else {
            return kept.map_or_else(|| token.to_owned(), |(_, kept)| kept);
        };
        if let Some((admin, kept)) = kept
            && admin == user.is_admin
            && state.auth.validate_compatibility(&kept, None).await.is_some()
        {
            return kept;
        }
        let scope = if user.is_admin { "play activity manage" } else { "play" };
        let made = compatibility_token(api, token, scope).await;
        self.0.lock().unwrap().insert(token.to_owned(), (user.is_admin, made.clone()));
        made
    }
}

pub fn titles(items: &Value) -> Vec<String> {
    items.as_array().map(|a| a.iter().map(|i| i["title"].as_str().unwrap().to_owned()).collect()).unwrap_or_default()
}

/// Titles of the `Metadata` of a Plex `MediaContainer`.
pub fn plex_titles(container: &Value) -> Vec<String> {
    titles(&container["Metadata"])
}

/// The names of the `Items` of a Jellyfin list.
pub fn jf_names(list: &Value) -> Vec<String> {
    list["Items"]
        .as_array()
        .or(list.as_array())
        .map(|a| a.iter().map(|i| i["Name"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// Percent-encodes a query value.
pub fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
