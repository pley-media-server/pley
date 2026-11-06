//! The Jellyfin-compatible API (docs/COMPATIBILITY.md): Jellyfin's apps and
//! the apps built for it (Plezy, Findroid, Jellyfin for Android TV and for
//! Kodi, Feishin, Infuse, Streamyfin) sign in to Pley and browse, play and
//! keep their watch state in it, as each user may. It is off unless an
//! administrator turns it on (Settings → Network), on Pley's own port or on
//! a port of its own.
//!
//! Its clients sign in with a password (`/Users/AuthenticateByName`) or
//! with Quick Connect ([`quick_connect`]) for a token for other apps
//! ([`crate::auth::compatibility`]): such tokens work on this API only, and
//! Pley's own tokens never on it. Paths are case-insensitive, as Jellyfin's
//! are, and ids are [`ids`].

pub mod ids;

mod dto;
mod items;
mod playback;
mod quick_connect;
mod socket;
mod system;
mod users;

pub(crate) use socket::Remotes;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;
use tower::util::MapRequest;

use crate::auth::{AuthUser, UsedFrom, compatibility};
use crate::error::AppError;
use crate::state::AppState;

/// The Jellyfin version reported to clients. Apps check it: Findroid takes
/// no server older than 12.0.0, Jellyfin for Android TV none older than
/// 10.11.0, and both read three or four numbers only.
pub const VERSION: &str = "12.4.0";

/// The product clients are told, by which they tell Jellyfin from Emby.
pub const PRODUCT: &str = "Jellyfin Server";

// ------------------------------------------------------------------ errors

#[derive(Debug)]
pub enum JfError {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest(String),
    TooManyRequests(String),
    Unavailable(String),
    Internal(anyhow::Error),
}

pub type JfResult<T> = Result<T, JfError>;

impl From<AppError> for JfError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Unauthorized
            | AppError::TwoStep(_)
            | AppError::TwoStepSetup(..)
            | AppError::InvalidToken
            | AppError::TokenPlacement
            | AppError::CompatibilityToken => JfError::Unauthorized,
            AppError::Forbidden(_) | AppError::ForbiddenAs(..) | AppError::InsufficientScope(_) => JfError::Forbidden,
            AppError::NotFound(_) | AppError::SessionStopped(_) => JfError::NotFound,
            AppError::BadRequest(m)
            | AppError::Conflict(m)
            | AppError::ConflictWith(m, _)
            | AppError::ConflictAs(_, m) => JfError::BadRequest(m),
            AppError::Unavailable(m) | AppError::TooManyRequests(m) | AppError::NotImplemented(m) => {
                JfError::Unavailable(m)
            }
            AppError::Internal(e) => JfError::Internal(e),
        }
    }
}

impl From<crate::listing::FilterError> for JfError {
    fn from(err: crate::listing::FilterError) -> Self {
        JfError::BadRequest(err.0)
    }
}

impl From<sqlx::Error> for JfError {
    fn from(err: sqlx::Error) -> Self {
        AppError::from(err).into()
    }
}

impl From<anyhow::Error> for JfError {
    fn from(err: anyhow::Error) -> Self {
        JfError::Internal(err)
    }
}

impl IntoResponse for JfError {
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            JfError::Unauthorized => (StatusCode::UNAUTHORIZED, String::new()),
            JfError::Forbidden => (StatusCode::FORBIDDEN, String::new()),
            JfError::NotFound => (StatusCode::NOT_FOUND, String::new()),
            JfError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            JfError::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, m),
            JfError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
            JfError::Internal(err) => {
                tracing::error!(error = ?err, "internal error in the Jellyfin API");
                (StatusCode::INTERNAL_SERVER_ERROR, String::new())
            }
        };
        (status, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], detail).into_response()
    }
}

// ------------------------------------------------------------------ clients

/// What a client says of itself in `Authorization: MediaBrowser Client="…",
/// Device="…", DeviceId="…", Version="…", Token="…"` (also written
/// `Emby`, and sent as `X-Emby-Authorization`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Client {
    pub client: Option<String>,
    pub device: Option<String>,
    pub device_id: Option<String>,
    pub version: Option<String>,
    pub token: Option<String>,
}

/// The longest value taken from the header: clients send short names.
const MAX_HEADER_VALUE: usize = 256;

impl Client {
    /// Reads the header's value; `None` when it isn't one of these.
    pub fn parse(value: &str) -> Option<Client> {
        let (scheme, rest) = value.trim().split_once(char::is_whitespace)?;
        if !scheme.eq_ignore_ascii_case("MediaBrowser") && !scheme.eq_ignore_ascii_case("Emby") {
            return None;
        }
        let mut client = Client::default();
        for part in split_params(rest) {
            let Some((key, value)) = part.split_once('=') else { continue };
            let value = value.trim().trim_matches('"');
            // Clients percent-encode values (names with commas and quotes),
            // some with `+` for spaces, as Jellyfin decodes them.
            let value = crate::util::percent_decode(value, true).trim().to_owned();
            if value.is_empty() || value.chars().count() > MAX_HEADER_VALUE || value.chars().any(char::is_control) {
                continue;
            }
            let slot = match key.trim().to_ascii_lowercase().as_str() {
                "client" => &mut client.client,
                "device" => &mut client.device,
                "deviceid" => &mut client.device_id,
                "version" => &mut client.version,
                "token" => &mut client.token,
                _ => continue,
            };
            *slot = Some(value);
        }
        Some(client)
    }

    /// From a request's headers: the authorization header, then the token
    /// headers clients send it in.
    pub fn of(headers: &HeaderMap) -> Client {
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        let mut client = [header("authorization"), header("x-emby-authorization")]
            .into_iter()
            .flatten()
            .find_map(Client::parse)
            .unwrap_or_default();
        if client.token.is_none() {
            client.token = [header("x-emby-token"), header("x-mediabrowser-token")]
                .into_iter()
                .flatten()
                .map(str::trim)
                .find(|t| !t.is_empty())
                .map(str::to_owned);
        }
        client
    }

    /// The name of the token a sign-in of this client gets, which the
    /// account page lists: "Findroid on Pixel 8".
    pub fn token_name(&self) -> String {
        let client = self.client.as_deref().unwrap_or("Jellyfin app");
        let name = match &self.device {
            Some(device) => format!("{client} on {device}"),
            None => client.to_owned(),
        };
        name.chars().take(compatibility::MAX_NAME_CHARS).collect()
    }
}

/// `a="1", b="x, y"`: the parameters, commas inside quotes kept.
fn split_params(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for (i, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The token of a request, wherever its client put it: the headers above,
/// or `api_key` / `ApiKey` in the query, which players fetching streams and
/// pictures use.
fn request_token(parts: &Parts, client: &Client) -> Option<String> {
    client.token.clone().or_else(|| query_value(parts.uri.query(), &["api_key", "apikey"]))
}

/// The user of a request from `from` that may come without a token (a
/// stream, a picture): `None` without one, an error for one that doesn't
/// work.
async fn token_user(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
    from: Option<std::net::IpAddr>,
) -> JfResult<Option<AuthUser>> {
    let token = Client::of(headers).token.or_else(|| query_value(query, &["api_key", "apikey"]));
    match token {
        None => Ok(None),
        Some(token) if token.starts_with(compatibility::PREFIX) => {
            let used = UsedFrom { ip: from.map(|ip| ip.to_string()), user_agent: None };
            let user = state.auth.validate_compatibility(&token, Some(&used)).await.ok_or(JfError::Unauthorized)?;
            Ok(Some(this_apis(user)))
        }
        Some(_) => Err(JfError::Unauthorized),
    }
}

/// A token's user as this API takes it: never as an administrator, whatever
/// its scopes allow on the Plex-compatible API.
pub(crate) fn this_apis(user: AuthUser) -> AuthUser {
    AuthUser { is_admin: false, ..user }
}

/// The first of `names` in a query string, matched in any case.
fn query_value(query: Option<&str>, names: &[&str]) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        names
            .iter()
            .any(|name| key.eq_ignore_ascii_case(name))
            .then(|| crate::util::percent_decode(value, true))
            .filter(|v| !v.is_empty())
    })
}

/// The caller: a token for other apps, which is the only kind this API
/// takes. Pley's own (a session's, an app's) are refused, so a client given
/// one can't reach Pley's own API through it.
pub struct JfUser {
    pub user: AuthUser,
    pub client: Client,
}

impl FromRequestParts<AppState> for JfUser {
    type Rejection = JfError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let client = Client::of(&parts.headers);
        let token = request_token(parts, &client).ok_or(JfError::Unauthorized)?;
        if !token.starts_with(compatibility::PREFIX) {
            return Err(JfError::Unauthorized);
        }
        let Ok(crate::auth::ClientIp(ip)) = crate::auth::ClientIp::from_request_parts(parts, state).await;
        let user_agent = parts.headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_owned);
        let used = UsedFrom { ip, user_agent };
        let user = state.auth.validate_compatibility(&token, Some(&used)).await.ok_or(JfError::Unauthorized)?;
        Ok(JfUser { user: this_apis(user), client })
    }
}

/// The client as it names itself, signed in or not.
pub struct ClientOf(pub Client);

impl<S: Send + Sync> FromRequestParts<S> for ClientOf {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientOf(Client::of(&parts.headers)))
    }
}

/// Query parameters, their names in any case, as Jellyfin reads them
/// (`parentId`, `ParentId`).
#[derive(Debug, Clone, Default)]
pub struct Q(Vec<(String, String)>);

/// The most parameters a request is read with, and the longest value.
const MAX_PARAMS: usize = 100;
const MAX_PARAM_CHARS: usize = 4096;

impl Q {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
    }

    /// A list: values separated by commas or `|`, or the parameter repeated,
    /// as clients write them.
    pub fn list(&self, name: &str) -> Vec<String> {
        self.0
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .flat_map(|(_, v)| v.split([',', '|']))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        self.get(name).map(|v| v.eq_ignore_ascii_case("true") || v == "1")
    }

    pub fn number(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.trim().parse().ok())
    }

    #[cfg(test)]
    pub fn of(pairs: &[(&str, &str)]) -> Q {
        Q(pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect())
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Q {
    type Rejection = JfError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let pairs = axum::extract::Query::<Vec<(String, String)>>::try_from_uri(&parts.uri)
            .map_err(|_| JfError::BadRequest("invalid query string".into()))?
            .0;
        if pairs.len() > MAX_PARAMS || pairs.iter().any(|(k, v)| k.len() + v.len() > MAX_PARAM_CHARS) {
            return Err(JfError::BadRequest("too many or too long parameters".into()));
        }
        Ok(Q(pairs))
    }
}

/// A request's JSON body, its keys read in any case as Jellyfin reads them
/// (`{"username","pw"}` and `{"Username","Pw"}` alike). An empty or broken
/// body is an empty object.
pub struct Body(pub serde_json::Value);

impl Body {
    pub fn get(&self, name: &str) -> Option<&serde_json::Value> {
        field(&self.0, name)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|v| v.as_str())
    }

    /// A number, sent as one or as a string.
    pub fn number(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
    }

    /// A boolean, sent as one or as a string (`"True"`).
    pub fn flag(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s.eq_ignore_ascii_case("true"))))
    }
}

/// A key of a JSON object, in any case.
pub fn field<'a>(value: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    value.as_object()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v).filter(|v| !v.is_null())
}

impl<S: Send + Sync> axum::extract::FromRequest<S> for Body {
    type Rejection = JfError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, state)
            .await
            .map_err(|_| JfError::BadRequest("the body is too big".into()))?;
        Ok(Body(serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::Value::Object(Default::default()))))
    }
}

/// Jellyfin's list answer.
#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub total_record_count: usize,
    pub start_index: usize,
}

// ------------------------------------------------------------------ the API

/// What every handler shares.
pub struct Jellyfin {
    pub ids: ids::Ids,
    /// The server's id as Jellyfin writes ids: 32 hexadecimal characters.
    pub server_id: String,
}

impl Jellyfin {
    /// `None` while the server can't use its `secret.key`, from which the ids'
    /// key derives: the API then answers that it is unavailable.
    fn new(state: &AppState) -> Option<Arc<Self>> {
        let secret = crate::two_step::load_key(&state.config.data_dir.join(crate::two_step::KEY_FILE)).ok()?;
        let server_id = state.identity.id.chars().take(32).collect();
        Some(Arc::new(Jellyfin { ids: ids::Ids::new(&secret), server_id }))
    }
}

/// The API as a service: paths are made lowercase before routing.
pub type Service = MapRequest<Router, fn(Request) -> Request>;

pub fn service(state: AppState) -> Service {
    let router = match Jellyfin::new(&state) {
        Some(jellyfin) => router(state, jellyfin),
        None => Router::new().fallback(|| async {
            JfError::Unavailable(format!(
                "the Jellyfin-compatible API needs the server's {}, which it can't use (its log says why)",
                crate::two_step::KEY_FILE
            ))
        }),
    };
    tower::ServiceBuilder::new().map_request(normalize as fn(Request) -> Request).service(router)
}

fn router(state: AppState, jellyfin: Arc<Jellyfin>) -> Router {
    Router::new()
        .merge(system::routes())
        .merge(users::routes())
        .merge(items::routes())
        .merge(playback::routes())
        .merge(quick_connect::routes())
        .merge(socket::routes())
        .fallback(|| async { JfError::NotFound })
        .layer(axum::middleware::from_fn(headers))
        // Requests' JSON bodies are small: sign-ins, reports, device profiles.
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
        .layer(axum::Extension(jellyfin))
        .layer(axum::Extension(Arc::new(playback::Streams::default())))
        .with_state(state)
}

/// Jellyfin's paths are case-insensitive, and clients write them both ways
/// (`/Items`, `/videos/…`): they are routed in lowercase, a trailing slash
/// left out.
fn normalize(mut req: Request) -> Request {
    let path = req.uri().path();
    let mut fixed = path.to_ascii_lowercase();
    while fixed.len() > 1 && fixed.ends_with('/') {
        fixed.pop();
    }
    if fixed == path {
        return req;
    }
    let path_and_query = match req.uri().query() {
        Some(query) => format!("{fixed}?{query}"),
        None => fixed,
    };
    if let Ok(pq) = axum::http::uri::PathAndQuery::try_from(path_and_query) {
        let mut parts = req.uri().clone().into_parts();
        parts.path_and_query = Some(pq);
        if let Ok(uri) = Uri::from_parts(parts) {
            *req.uri_mut() = uri;
        }
    }
    req
}

/// Answers are data and media, never pages.
async fn headers(request: Request, next: Next) -> Response {
    let (method, uri) = (request.method().clone(), request.uri().clone());
    let mut response = next.run(request).await;
    // What apps ask for that isn't here, or isn't theirs: what to look at
    // when one misbehaves. Tokens and media source ids are left out.
    if response.status().is_client_error() {
        tracing::debug!("Jellyfin API {method} {}: {}", crate::api::loggable_uri(&uri), response.status());
    }
    let headers = response.headers_mut();
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'; sandbox"),
    );
    headers.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-cache"));
    response
}

/// Serves the API on a port of its own.
pub async fn serve(
    state: AppState,
    listener: tokio::net::TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    if let Ok(addr) = listener.local_addr() {
        tracing::info!("Jellyfin-compatible API listening on http://{}", display_addr(addr));
    }
    crate::http::serve(listener, service(state), shutdown).await;
}

fn display_addr(addr: SocketAddr) -> String {
    if addr.ip().is_unspecified() { format!("localhost:{}", addr.port()) } else { addr.to_string() }
}

// ------------------------------------------------------------------ on pley's port

/// The API on Pley's own port, in front of the web app and its API: see
/// [`on_web_port`].
#[derive(Clone)]
pub struct OnWebPort {
    state: AppState,
    service: Service,
}

impl OnWebPort {
    pub fn new(state: &AppState) -> Self {
        Self { state: state.clone(), service: service(state.clone()) }
    }
}

/// The first segments of Jellyfin's paths (every family of its API, those
/// Pley answers and those it doesn't), lowercase: its clients' requests for
/// them come to it on Pley's port whatever they carry (a player fetching a
/// stream or a picture sends no header), and get its answers, never a page
/// of the web app's. The web app's pages are words of their own (`/item/1`,
/// `/live`, `/settings`) that Jellyfin has none of.
const ONLY_JELLYFIN: &[&str] = &[
    "albums",
    "artists",
    "audio",
    "auth",
    "backup",
    "branding",
    "channels",
    "clientlog",
    "collections",
    "devices",
    "displaypreferences",
    "environment",
    "fallbackfont",
    "genres",
    "getutctime",
    "items",
    "libraries",
    "livestreams",
    "livetv",
    "localization",
    "mediasegments",
    "movies",
    "musicgenres",
    "packages",
    "persons",
    "playback",
    "plugins",
    "providers",
    "quickconnect",
    "repositories",
    "scheduledtasks",
    "sessions",
    "shows",
    "socket",
    "songs",
    "startup",
    "studios",
    "syncplay",
    "system",
    "tags",
    "trailers",
    "userfavoriteitems",
    "userimage",
    "useritems",
    "userplayeditems",
    "users",
    "userviews",
    "videos",
    "web",
    "years",
];

/// Paths both answer, this API (`/Search/Hints`, `/Playlists/{id}/Items`,
/// `/Library/…`) and the web app (`/search`, `/playlists`, `/library/{id}`):
/// there, what the request carries decides ([`is_jellyfin_request`]).
const SHARED: &[&str] = &["library", "playlists", "search"];

fn first_segment(path: &str) -> String {
    path.trim_start_matches('/').split('/').next().unwrap_or_default().to_ascii_lowercase()
}

/// Whether a request on Pley's port is this API's: a path only it has, or a
/// shared one with a Jellyfin client's header or token (which browsers
/// asking for the web app never send). `/api/…` and the rest stay Pley's.
fn is_jellyfin_request(request: &Request) -> bool {
    let segment = first_segment(request.uri().path());
    if ONLY_JELLYFIN.contains(&segment.as_str()) {
        return true;
    }
    if !SHARED.contains(&segment.as_str()) {
        return false;
    }
    let client = Client::of(request.headers());
    client.client.is_some()
        || client.token.is_some()
        || query_value(request.uri().query(), &["api_key", "apikey"]).is_some()
}

/// While the API answers on Pley's port, its requests go to it and the
/// rest to Pley. Answers to the shared paths vary with what decides. A
/// request that names itself the Plex-compatible API's (`X-Plex-…`) is left
/// to that API while it is on Pley's port too: its paths and this one's
/// meet (`/library`, `/system`, `/devices`).
pub async fn on_web_port(State(api): State<OnWebPort>, request: Request, next: Next) -> Response {
    if !api.state.network.jellyfin_on_web() || (api.state.network.plex_on_web() && crate::plex::names_itself(&request))
    {
        return next.run(request).await;
    }
    let shared = SHARED.contains(&first_segment(request.uri().path()).as_str());
    let mut response = if is_jellyfin_request(&request) {
        let Ok(response) = api.service.oneshot(request).await;
        response
    } else {
        next.run(request).await
    };
    if shared {
        let headers = response.headers_mut();
        let mut vary: Vec<String> =
            headers.get_all(header::VARY).iter().filter_map(|v| v.to_str().ok()).map(str::to_owned).collect();
        vary.push("Authorization, X-Emby-Authorization, X-Emby-Token, X-MediaBrowser-Token".to_owned());
        if let Ok(value) = HeaderValue::from_str(&vary.join(", ")) {
            headers.insert(header::VARY, value);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_clients_say_of_themselves() {
        let client = Client::parse(
            r#"MediaBrowser Client="Plezy%20Windows", Device="Desk, upstairs", DeviceId="abc", Version="1.2", Token="pley_ct_x""#,
        )
        .unwrap();
        assert_eq!(client.client.as_deref(), Some("Plezy Windows"));
        assert_eq!(client.device.as_deref(), Some("Desk, upstairs"), "commas inside quotes");
        assert_eq!(client.device_id.as_deref(), Some("abc"));
        assert_eq!(client.token.as_deref(), Some("pley_ct_x"));
        assert_eq!(client.token_name(), "Plezy Windows on Desk, upstairs");

        let emby =
            Client::parse("Emby UserId=\"\", Client=\"Kodi\", Device=\"LibreELEC\", DeviceId=\"1\", Version=\"1\"");
        assert_eq!(emby.unwrap().client.as_deref(), Some("Kodi"));
        assert!(Client::parse("Bearer abc").is_none());
        let long = format!("MediaBrowser Client=\"{}\"", "x".repeat(MAX_HEADER_VALUE + 1));
        assert_eq!(Client::parse(&long).unwrap().client, None, "too long: left out");

        let mut headers = HeaderMap::new();
        headers.insert("x-emby-token", HeaderValue::from_static("pley_ct_y"));
        assert_eq!(Client::of(&headers).token.as_deref(), Some("pley_ct_y"));
        assert_eq!(query_value(Some("a=1&ApiKey=pley_ct_z"), &["api_key", "apikey"]).as_deref(), Some("pley_ct_z"));
        assert_eq!(query_value(Some("api_key="), &["api_key"]), None);
    }

    fn request(uri: &str, headers: &[(&str, &str)]) -> Request {
        let mut builder = Request::builder().uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn its_requests_are_told_from_the_web_apps() {
        for uri in ["/System/Info/Public", "/Items/abc/Images/Primary", "/videos/abc/stream", "/socket", "/Users/Me"] {
            assert!(is_jellyfin_request(&request(uri, &[])), "{uri}");
        }
        for uri in
            ["/", "/item/12", "/library/3", "/search?q=x", "/playlists/4", "/live", "/api/health", "/assets/x.js"]
        {
            assert!(!is_jellyfin_request(&request(uri, &[])), "{uri}: the web app's");
        }
        let auth = [("Authorization", r#"MediaBrowser Client="Findroid", Token="t""#)];
        assert!(is_jellyfin_request(&request("/Search/Hints?searchTerm=x", &auth)));
        assert!(is_jellyfin_request(&request("/Playlists/abc/Items?api_key=t", &[])));
        assert!(!is_jellyfin_request(&request("/api/libraries", &auth)), "Pley's API, whatever it carries");
    }

    #[test]
    fn paths_are_routed_in_lowercase() {
        let fixed = normalize(request("/Users/AuthenticateByName/?x=Y", &[]));
        assert_eq!(fixed.uri(), "/users/authenticatebyname?x=Y");
    }
}
