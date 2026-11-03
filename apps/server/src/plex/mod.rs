//! Plex Media Server compatible API (docs/COMPATIBILITY.md).
//!
//! Served on pley's own port by default ([`on_web_port`]), or on listeners
//! of its own (port 32400, Plex's), so tools written for Plex —
//! python-plexapi, Tautulli, Overseerr, Kometa, direct-play clients — can
//! browse the libraries, read and write watch state, stream files and
//! follow sessions. Responses are XML `MediaContainer`s like Plex's, or JSON
//! with `Accept: application/json`. Everything goes through the same catalog,
//! auth and playback code as pley's own API.

mod collection_search;
mod edit;
mod keys;
mod library;
mod lists;
pub(crate) mod meta;
mod notifications;
mod playback;
pub(crate) use playback::SessionKeys;
mod queues;
mod server;
mod xml;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{FromRequestParts, Query, Request, State};
use axum::http::request::Parts;
use axum::http::uri::PathAndQuery;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, delete, get, post, put};
use tower::util::{MapRequest, MapRequestLayer};
use tower::{Layer, ServiceExt};
use tower_http::compression::predicate::{NotForContentType, Predicate, SizeAbove};
use tower_http::compression::{CompressionLayer, CompressionLevel};

use crate::auth::grants::APP_TOKEN_PREFIX;
use crate::auth::{AuthUser, ClientIp, UsedFrom, compatibility, token_without_cookie};
use crate::error::AppError;
use crate::state::AppState;
pub use xml::Node;

/// Plex version reported to clients. Tools gate features on it.
pub const PLEX_VERSION: &str = concat!("1.41.0.0-pley.", env!("CARGO_PKG_VERSION"));

/// The Plex API as a service: paths are normalised before routing.
pub type Service = MapRequest<Router, fn(Request) -> Request>;

pub fn service(state: AppState) -> Service {
    let info = Arc::new(PlexInfo::load(&state.config.data_dir));
    MapRequestLayer::new(normalize as fn(Request) -> Request).layer(router(state, info))
}

/// This API on pley's own port, in front of the web app and its API: see
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

/// Paths only this API has. Its clients' requests for them come to it on
/// pley's port even without its headers: a player fetching a stream's
/// segments, a check of `/identity`. `/`, `/search`, `/playlists…` and
/// `/library` are the web app's pages too: there, its headers decide.
const ONLY_PLEX: [&str; 30] = [
    "/identity",
    "/:",
    "/video/:",
    "/audio/:",
    "/photo/:",
    "/library/sections",
    "/library/metadata",
    "/library/parts",
    "/library/streams",
    "/library/collections",
    "/library/media",
    "/library/all",
    "/library/onDeck",
    "/library/recentlyAdded",
    "/hubs",
    "/status",
    "/transcode",
    "/playQueues",
    "/myplex",
    "/servers",
    "/system",
    "/accounts",
    "/clients",
    "/devices",
    "/activities",
    "/butler",
    "/updater",
    "/prefs",
    "/media/providers",
    "/actions",
];

/// Whether `request`, on pley's port, is this API's: its clients name
/// themselves (`X-Plex-…` headers, or `X-Plex-Token` in the query, which
/// browsers asking for the web app never send), or it asks for a path only
/// this API has. pley's own API and documents stay pley's.
pub fn is_plex_request(request: &Request) -> bool {
    let path = request.uri().path();
    if path.starts_with("/api/") || path.starts_with("/.well-known/") {
        return false;
    }
    names_itself(request) || only_plex(path)
}

/// Whether `request` says it is for this API: an `X-Plex-…` header, or
/// such a parameter in the query. The Jellyfin-compatible API leaves these
/// to it on pley's port.
pub fn names_itself(request: &Request) -> bool {
    request.headers().keys().any(|name| name.as_str().starts_with("x-plex-"))
        || request.uri().query().is_some_and(|query| {
            query.split('&').any(|pair| pair.get(..7).is_some_and(|key| key.eq_ignore_ascii_case("x-plex-")))
        })
}

fn only_plex(path: &str) -> bool {
    ONLY_PLEX
        .iter()
        .any(|prefix| path.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with('/')))
}

/// Paths both answer, this API and the web app, as its headers say.
fn shared_path(path: &str) -> bool {
    matches!(path, "/" | "/search" | "/library" | "/playlists") || path.starts_with("/playlists/")
}

/// pley's port answers this API's requests ([`is_plex_request`]) with this
/// API, while it is on it (`plex_bind = "web"`, the default), and the rest
/// with pley. Its answers keep pley's web app safe on the origin they now
/// share: no page, script or frame may come of them, and no cache hands one
/// side's answer to the other.
pub async fn on_web_port(State(plex): State<OnWebPort>, request: Request, next: Next) -> Response {
    if !plex.state.network.plex_on_web() {
        return next.run(request).await;
    }
    let shared = shared_path(request.uri().path());
    let mut response = if is_plex_request(&request) {
        let Ok(mut response) = plex.service.oneshot(request).await;
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'none'; sandbox"));
        headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        headers.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("private"));
        response
    } else {
        next.run(request).await
    };
    if shared {
        let headers = response.headers_mut();
        let mut vary: Vec<String> =
            headers.get_all(header::VARY).iter().filter_map(|v| v.to_str().ok()).map(str::to_owned).collect();
        vary.push("X-Plex-Token, X-Plex-Client-Identifier".to_owned());
        if let Ok(value) = HeaderValue::from_str(&vary.join(", ")) {
            headers.insert(header::VARY, value);
        }
    }
    response
}

/// Serve this API on `listener` until `shutdown` completes, then wait for the
/// requests in progress (the main server limits that wait).
pub async fn serve(
    state: AppState,
    listener: tokio::net::TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) {
    if let Ok(addr) = listener.local_addr() {
        tracing::info!("Plex-compatible API listening on http://{}", display_addr(addr));
    }
    crate::http::serve(listener, service(state), shutdown).await;
}

fn display_addr(addr: SocketAddr) -> String {
    if addr.ip().is_unspecified() { format!("localhost:{}", addr.port()) } else { addr.to_string() }
}

/// Identity shared by every response.
pub struct PlexInfo {
    /// Stable 40 hex characters, like Plex's `machineIdentifier`.
    pub machine_id: String,
}

impl PlexInfo {
    pub(crate) fn load(data_dir: &Path) -> Self {
        Self { machine_id: crate::identity::load_id(data_dir) }
    }

    /// Deterministic UUID for a library section.
    pub fn section_uuid(&self, library_id: i64) -> String {
        let hex = format!("{}{library_id:012x}", &self.machine_id[..20]);
        format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
    }
}

fn router(state: AppState, info: Arc<PlexInfo>) -> Router {
    let compressible = SizeAbove::new(1024)
        .and(NotForContentType::new("video/"))
        .and(NotForContentType::new("audio/"))
        .and(NotForContentType::new("image/"))
        .and(NotForContentType::new("application/octet-stream"))
        .and(NotForContentType::new("application/vnd.apple.mpegurl"));
    let transcode = Router::new()
        .route("/_/transcode/universal/start.m3u8", get(playback::universal_start))
        .route("/_/transcode/universal/start", get(playback::universal_start))
        .route("/_/transcode/universal/decision", get(playback::universal_decision))
        .route("/_/transcode/universal/stop", get(playback::universal_stop))
        .route("/_/transcode/universal/ping", get(playback::universal_ping))
        .route("/_/transcode/universal/session/{session}/base/{file}", get(playback::universal_file))
        .route("/_/transcode/universal/session/{session}/{file}", get(playback::universal_file_flat));

    Router::new()
        .route("/", get(server::root))
        .route("/identity", get(server::identity))
        .route("/accounts", get(server::accounts))
        .route("/myplex/account", get(server::myplex_account))
        .route("/system", get(server::system))
        .route("/servers", get(server::servers))
        .route("/clients", get(server::empty))
        .route("/devices", get(server::empty))
        .route("/activities", get(server::empty))
        .route("/butler", get(server::empty))
        .route("/playlists", get(lists::list).post(lists::create))
        .route("/playlists/{id}", get(lists::get).put(lists::edit).delete(lists::delete))
        .route("/playlists/{id}/items", get(lists::items).put(lists::add))
        .route("/playlists/{id}/items/{entry}", delete(lists::remove))
        .route("/playlists/{id}/items/{entry}/move", put(lists::move_item))
        .route("/playlists/{id}/composite/{version}", get(lists::composite))
        .route("/updater/status", get(server::empty))
        .route("/prefs", get(server::prefs))
        .route("/_/prefs", get(server::prefs).put(server::settings_refused))
        .route("/media/providers", get(server::media_providers))
        .route("/library", get(library::root))
        .route("/library/sections", get(library::sections))
        .route("/library/sections/all/refresh", any(library::refresh_all))
        .route("/library/sections/{id}", get(library::section))
        .route("/library/sections/{id}/all", get(library::all).put(edit::section))
        .route("/library/sections/{id}/refresh", any(library::refresh))
        .route("/library/sections/{id}/collections", get(lists::section_collections))
        .route("/library/sections/{id}/albums", get(library::section_albums))
        .route("/library/sections/{id}/recentlyAdded", get(library::section_recently_added))
        .route("/library/sections/{id}/onDeck", get(library::section_on_deck))
        .route("/library/sections/{id}/firstCharacter", get(library::first_character))
        .route("/library/sections/{id}/search", get(library::section_search))
        .route("/library/sections/{id}/emptyTrash", any(server::accepted))
        .route("/library/sections/{id}/analyze", any(server::accepted))
        .route("/library/sections/{id}/prefs", get(library::section_prefs).put(server::settings_refused))
        .route("/library/sections/{id}/{filter}", get(library::filter_choices))
        .route("/library/all", get(library::library_all))
        .route("/library/onDeck", get(library::on_deck))
        .route("/library/recentlyAdded", get(library::recently_added))
        .route("/library/metadata/{id}", get(library::metadata).put(edit::item))
        .route("/library/metadata/{id}/children", get(library::children))
        .route("/library/metadata/{id}/extras", get(library::extras))
        .route("/library/metadata/{id}/allLeaves", get(library::all_leaves))
        .route("/library/metadata/{id}/grandchildren", get(library::all_leaves))
        .route("/library/metadata/{id}/similar", get(library::similar))
        .route("/library/metadata/{id}/related", get(library::related))
        .route("/library/metadata/{id}/refresh", any(library::refresh_item))
        .route(
            "/library/metadata/{id}/{image}",
            get(library::image)
                .post(edit::upload_artwork)
                .put(edit::choose_artwork)
                .layer(axum::extract::DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/library/metadata/{id}/{image}/{version}", get(library::image_version))
        .route("/library/collections", post(lists::create_collection))
        .route("/library/collections/{id}", get(lists::collection).delete(lists::collection_delete))
        .route("/library/collections/{id}/items", put(lists::collection_add))
        .route("/library/collections/{id}/items/{item}", delete(lists::collection_remove))
        .route("/library/collections/{id}/children", get(lists::collection_children))
        .route("/library/collections/{id}/prefs", put(lists::collection_prefs))
        .route(
            "/library/collections/{id}/{image}",
            get(lists::collection_image)
                .post(edit::upload_collection_artwork)
                .layer(axum::extract::DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/library/collections/{id}/{image}/{version}", get(lists::collection_image_version))
        .route("/library/parts/{id}/indexes/{quality}", get(playback::part_index))
        .route("/library/parts/{id}/indexes/{quality}/{offset}", get(playback::part_index_frame))
        .route("/library/media/{id}/chapterImages/{n}", get(playback::chapter_image))
        .route("/library/parts/{id}/{version}/{file}", get(playback::part_file))
        .route("/library/streams/{id}", get(playback::stream_file))
        .route("/hubs", get(library::hubs))
        .route("/hubs/search", get(library::hub_search))
        .route("/hubs/continueWatching/items", get(library::continue_watching))
        .route("/hubs/home/recentlyAdded", get(library::home_recently_added))
        .route("/hubs/sections/{id}", get(library::section_hubs))
        .route("/hubs/sections/{id}/continueWatching/items", get(library::section_continue_watching))
        .route("/search", get(library::search))
        .route("/photo/_/transcode", get(library::photo_transcode))
        .route("/_/timeline", any(playback::timeline))
        .route("/_/progress", any(playback::progress))
        .route("/_/scrobble", any(playback::scrobble))
        .route("/_/unscrobble", any(playback::unscrobble))
        .route("/_/rate", any(playback::rate))
        .route("/playQueues", post(queues::create))
        .route("/playQueues/{id}", get(queues::get).put(queues::add))
        .route("/playQueues/{id}/items", delete(queues::clear))
        .route("/playQueues/{id}/items/{entry}", delete(queues::remove))
        .route("/playQueues/{id}/items/{entry}/move", put(queues::move_item))
        .route("/playQueues/{id}/shuffle", put(queues::shuffled))
        .route("/playQueues/{id}/unshuffle", put(queues::unshuffled))
        .route("/actions/removeFromContinueWatching", put(playback::remove_from_continue_watching))
        .route("/status/sessions", get(playback::sessions))
        .route("/status/sessions/terminate", get(playback::terminate))
        .route("/_/websockets/notifications", get(notifications::socket))
        .route("/status/sessions/history/all", get(server::history))
        .route("/status/sessions/history/{id}", delete(server::delete_history))
        .route("/transcode/sessions", get(playback::transcode_sessions))
        .nest("/video", transcode.clone())
        .nest("/audio", transcode)
        .fallback(|| async { PlexError::NotFound })
        .layer(middleware::from_fn(render))
        .layer(CompressionLayer::new().quality(CompressionLevel::Precise(4)).compress_when(compressible))
        .layer(axum::Extension(info))
        .with_state(state)
}

/// Plex paths use `:` segments (`/:/timeline`, `/video/:/transcode/…`), which
/// axum cannot route, and clients add trailing slashes. Rewrite both.
fn normalize(mut req: Request) -> Request {
    let path = req.uri().path();
    let colon = path.contains("/:/") || path.ends_with("/:");
    let trailing = path.len() > 1 && path.ends_with('/');
    if !colon && !trailing {
        return req;
    }
    let mut fixed = path.replace("/:/", "/_/");
    if fixed.ends_with("/:") {
        fixed.truncate(fixed.len() - 1);
        fixed.push('_');
    }
    while fixed.len() > 1 && fixed.ends_with('/') {
        fixed.pop();
    }
    let path_and_query = match req.uri().query() {
        Some(query) => format!("{fixed}?{query}"),
        None => fixed,
    };
    if let Ok(pq) = PathAndQuery::try_from(path_and_query) {
        let mut parts = req.uri().clone().into_parts();
        parts.path_and_query = Some(pq);
        if let Ok(uri) = Uri::from_parts(parts) {
            *req.uri_mut() = uri;
        }
    }
    req
}

// ------------------------------------------------------------------ rendering

/// Handlers return a [`Node`]; it is serialised here once the requested
/// format (XML or JSON) is known.
#[derive(Clone)]
struct NodeBody(Arc<Node>);

impl IntoResponse for Node {
    fn into_response(self) -> Response {
        let mut response = StatusCode::OK.into_response();
        response.extensions_mut().insert(NodeBody(Arc::new(self)));
        response
    }
}

/// Whether the caller is an administrator, once [`PlexUser`] knows: others
/// get files' names, not where the server keeps them.
#[derive(Clone, Default)]
struct Viewer(Arc<std::sync::OnceLock<bool>>);

async fn render(mut request: Request, next: Next) -> Response {
    let json = wants_json(request.headers());
    let viewer = Viewer::default();
    request.extensions_mut().insert(viewer.clone());
    let mut response = next.run(request).await;
    if let Some(NodeBody(mut node)) = response.extensions_mut().remove::<NodeBody>() {
        if viewer.0.get() != Some(&true) {
            Arc::make_mut(&mut node).names_only();
        }
        let (content_type, body) =
            if json { ("application/json", node.to_json()) } else { ("text/xml;charset=utf-8", node.to_xml()) };
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        *response.body_mut() = Body::from(body);
    }
    let headers = response.headers_mut();
    headers.insert("x-plex-protocol", HeaderValue::from_static("1.0"));
    headers.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-cache"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    // Data and pictures, never a page: nothing to sniff, run or frame.
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
    );
    response
}

fn wants_json(headers: &HeaderMap) -> bool {
    headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("application/json"))
}

// ------------------------------------------------------------------ errors

#[derive(Debug)]
pub enum PlexError {
    Unauthorized,
    /// A token that isn't a compatibility token, such as a Pley session's,
    /// which this API took before: `401`, saying what to use instead.
    NotACompatibilityToken,
    Forbidden,
    NotFound,
    BadRequest(String),
    Unavailable(String),
    Internal(anyhow::Error),
}

pub type PlexResult<T> = Result<T, PlexError>;

impl From<AppError> for PlexError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Unauthorized
            | AppError::TwoStep(_)
            | AppError::TwoStepSetup(..)
            | AppError::InvalidToken
            | AppError::TokenPlacement
            | AppError::CompatibilityToken => PlexError::Unauthorized,
            AppError::Forbidden(_) | AppError::ForbiddenAs(..) | AppError::InsufficientScope(_) => PlexError::Forbidden,
            AppError::NotFound(_) | AppError::SessionStopped(_) => PlexError::NotFound,
            AppError::BadRequest(m) | AppError::Conflict(m) | AppError::ConflictWith(m, _) => PlexError::BadRequest(m),
            AppError::Unavailable(m) | AppError::TooManyRequests(m) | AppError::NotImplemented(m) => {
                PlexError::Unavailable(m)
            }
            AppError::Internal(e) => PlexError::Internal(e),
        }
    }
}

impl From<crate::listing::FilterError> for PlexError {
    fn from(err: crate::listing::FilterError) -> Self {
        PlexError::BadRequest(err.0)
    }
}

impl From<sqlx::Error> for PlexError {
    fn from(err: sqlx::Error) -> Self {
        AppError::from(err).into()
    }
}

impl IntoResponse for PlexError {
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            PlexError::Unauthorized => (StatusCode::UNAUTHORIZED, String::new()),
            PlexError::NotACompatibilityToken => (
                StatusCode::UNAUTHORIZED,
                "this API takes tokens for other apps only: make one in Pley, under Account".to_owned(),
            ),
            PlexError::Forbidden => (StatusCode::FORBIDDEN, String::new()),
            PlexError::NotFound => (StatusCode::NOT_FOUND, String::new()),
            PlexError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            PlexError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
            PlexError::Internal(err) => {
                tracing::error!(error = ?err, "internal error in the Plex API");
                (StatusCode::INTERNAL_SERVER_ERROR, String::new())
            }
        };
        let title = format!("{} {}", status.as_u16(), status.canonical_reason().unwrap_or(""));
        let detail = if detail.is_empty() { String::new() } else { format!("<p>{}</p>", html_escape(&detail)) };
        let body = format!("<html><head><title>{title}</title></head><body><h1>{title}</h1>{detail}</body></html>");
        (status, [(header::CONTENT_TYPE, "text/html")], body).into_response()
    }
}

fn html_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

// ------------------------------------------------------------------ auth

/// The caller, authenticated by a compatibility token (see
/// [`crate::auth::compatibility`]) in `X-Plex-Token` (header or query), or
/// in `Authorization: Bearer`, `X-Pley-Token` or `?token=` as pley's own
/// tokens go. Its rights are what the token's scopes allow, each checked by
/// name: everyone's sessions, history and accounts, and file paths, only
/// with `activity` ([`sees_everyone`]), and changes only through
/// [`PlexAdmin`]. A token is never an administrator (`is_admin` is false).
///
/// Never by a session's token: those are for pley's own apps, with every
/// right the account has. Never by the web UI's cookie either: browsers send
/// it to this port too (cookies aren't per port), and several Plex endpoints
/// change things on a GET, so a page elsewhere could otherwise act for
/// whoever is signed in to pley.
#[derive(Debug, Clone)]
pub struct PlexUser(pub AuthUser);

impl FromRequestParts<AppState> for PlexUser {
    type Rejection = PlexError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = match parts.extensions.get::<AuthUser>() {
            Some(user) => user.clone(),
            None => {
                let token = request_token(parts).ok_or(PlexError::Unauthorized)?;
                if !token.starts_with(compatibility::PREFIX) {
                    return Err(PlexError::NotACompatibilityToken);
                }
                let Ok(ClientIp(ip)) = ClientIp::from_request_parts(parts, state).await;
                let user_agent = parts.headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_owned);
                let used = UsedFrom { ip, user_agent };
                let user =
                    state.auth.validate_compatibility(&token, Some(&used)).await.ok_or(PlexError::Unauthorized)?;
                parts.extensions.insert(user.clone());
                user
            }
        };
        if let Some(Viewer(admin)) = parts.extensions.get::<Viewer>() {
            let _ = admin.set(sees_everyone(&user));
        }
        Ok(PlexUser(user))
    }
}

/// Whether the caller sees everyone's sessions, history and accounts, and
/// where files are: a token with `activity`, on an account that is an
/// administrator now. Seeing, never changing.
pub(crate) fn sees_everyone(user: &AuthUser) -> bool {
    user.compatibility_allows(compatibility::Scope::Activity)
}

/// The caller as collections are listed to it: with `manage`, those still
/// empty too, which it may fill (as administrators see them in Pley).
pub(crate) fn collections_viewer(user: &AuthUser) -> AuthUser {
    AuthUser { is_admin: user.compatibility_allows(compatibility::Scope::Manage), ..user.clone() }
}

/// Changes only administrators make (library and item refreshes, edits,
/// collections, stopping others' sessions): a compatibility token with
/// `manage`, on an account that is an administrator; `activity` or `play`
/// alone get `403`. The handler acts with an administrator's rights for
/// that change, none of which administers accounts or the server.
pub struct PlexAdmin(pub AuthUser);

impl FromRequestParts<AppState> for PlexAdmin {
    type Rejection = PlexError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let PlexUser(user) = PlexUser::from_request_parts(parts, state).await?;
        if !user.compatibility_allows(compatibility::Scope::Manage) {
            return Err(PlexError::Forbidden);
        }
        Ok(PlexAdmin(AuthUser { is_admin: true, ..user }))
    }
}

/// The token of a Plex request: `X-Plex-Token` (header or query), or a
/// bearer token. Never the web UI's cookie: pages on other sites could make
/// a browser send that. Never an app's token (`pley_…` but `pley_ct_…`):
/// apps use pley's own API only, within what they were allowed.
pub(super) fn request_token(parts: &Parts) -> Option<String> {
    plex_token(parts)
        .or_else(|| token_without_cookie(parts))
        .filter(|token| token.starts_with(compatibility::PREFIX) || !token.starts_with(APP_TOKEN_PREFIX))
}

fn plex_token(parts: &Parts) -> Option<String> {
    if let Some(token) = parts.headers.get("x-plex-token").and_then(|v| v.to_str().ok()) {
        let token = token.trim();
        if !token.is_empty() {
            return Some(token.to_owned());
        }
    }
    let Query(pairs) = Query::<Vec<(String, String)>>::try_from_uri(&parts.uri).ok()?;
    pairs.into_iter().find(|(k, v)| k.eq_ignore_ascii_case("x-plex-token") && !v.is_empty()).map(|(_, v)| v)
}

// ------------------------------------------------------------------ request helpers

pub use crate::listing::{Page, Params};

impl Params {
    /// Case-insensitive lookup (`X-Plex-…` parameters).
    pub fn get_ci(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Params {
    type Rejection = PlexError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match Query::<Vec<(String, String)>>::try_from_uri(&parts.uri) {
            Ok(Query(pairs)) => Ok(Params(pairs)),
            Err(_) => Err(PlexError::BadRequest("invalid query string".into())),
        }
    }
}

/// `X-Plex-Container-Start` / `-Size` paging, from headers or the query.
impl<S: Send + Sync> FromRequestParts<S> for Page {
    type Rejection = PlexError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<Vec<(String, String)>>::try_from_uri(&parts.uri).map(|Query(q)| q).unwrap_or_default();
        let value = |name: &str| -> Option<usize> {
            parts
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
                .or_else(|| query.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()))
                .and_then(|v| v.trim().parse().ok())
        };
        Ok(Page { start: value("x-plex-container-start").unwrap_or(0), size: value("x-plex-container-size") })
    }
}

/// Client details Plex apps send with every request.
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub identifier: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub platform: Option<String>,
    pub platform_version: Option<String>,
    pub device: Option<String>,
    pub device_name: Option<String>,
    pub address: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = PlexError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<Vec<(String, String)>>::try_from_uri(&parts.uri).map(|Query(q)| q).unwrap_or_default();
        let value = |name: &str| -> Option<String> {
            parts
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
                .or_else(|| query.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()))
                .map(|v| v.trim().chars().take(128).collect::<String>())
                .filter(|v| !v.is_empty())
        };
        let address = parts
            .extensions
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|axum::extract::ConnectInfo(addr)| addr.ip().to_string());
        Ok(ClientInfo {
            identifier: value("x-plex-client-identifier"),
            product: value("x-plex-product"),
            version: value("x-plex-version"),
            platform: value("x-plex-platform"),
            platform_version: value("x-plex-platform-version"),
            device: value("x-plex-device"),
            device_name: value("x-plex-device-name"),
            address,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plex_request(uri: &str, headers: &[(&str, &str)]) -> bool {
        let mut request = Request::builder().uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        is_plex_request(&request.body(Body::empty()).unwrap())
    }

    #[test]
    fn its_clients_are_told_from_the_web_apps() {
        // The same paths: its clients name themselves, browsers don't.
        for path in ["/", "/search?query=x", "/playlists", "/playlists/3", "/library"] {
            assert!(!plex_request(path, &[]), "{path}: the web app's page");
            assert!(plex_request(path, &[("X-Plex-Token", "t")]), "{path}");
            assert!(plex_request(path, &[("X-Plex-Client-Identifier", "c")]), "{path}");
        }
        assert!(plex_request("/?X-Plex-Token=t", &[]));
        assert!(plex_request("/search?query=x&x-plex-token=t", &[]));
        // Its own paths without any of that: a player's segments, /identity.
        assert!(plex_request("/identity", &[]));
        assert!(plex_request("/video/:/transcode/universal/session/abc/base/00001.ts", &[]));
        assert!(plex_request("/:/timeline?ratingKey=1", &[]));
        assert!(plex_request("/library/metadata/12/thumb/3", &[]));
        // The web app's pages and pley's API stay pley's.
        for path in ["/library/12", "/account", "/settings/network", "/assets/index.js", "/status-page", "/api/health"]
        {
            assert!(!plex_request(path, &[]), "{path}");
        }
        assert!(!plex_request("/api/libraries", &[("X-Plex-Token", "t")]), "pley's API, whatever it carries");
        assert!(!plex_request("/.well-known/oauth-authorization-server", &[("X-Plex-Token", "t")]));
    }

    #[test]
    fn every_route_is_reached_on_pleys_port() {
        // This module's routes, as clients write them (`/:/` for `/_/`):
        // each is only this API's, or one its headers decide.
        let source = include_str!("mod.rs");
        let mut routes = 0;
        for line in source.lines().map(str::trim).filter(|line| line.starts_with(".route(\"")) {
            let path = line.trim_start_matches(".route(\"").split('"').next().unwrap();
            let path = path.replace("/_/", "/:/").replace("{id}", "1").replace("{session}", "s");
            let path = if path.starts_with("/:/transcode") { format!("/video{path}") } else { path };
            assert!(only_plex(&path) || shared_path(&path), "{path} isn't reached on pley's port without its headers");
            routes += 1;
        }
        assert!(routes > 80, "{routes} routes read");
    }

    fn rewritten(uri: &str) -> String {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        normalize(req).uri().to_string()
    }

    #[test]
    fn rewrites_colon_segments_and_trailing_slashes() {
        assert_eq!(rewritten("/:/timeline?time=1"), "/_/timeline?time=1");
        assert_eq!(
            rewritten("/video/:/transcode/universal/start.m3u8?path=%2Flibrary"),
            "/video/_/transcode/universal/start.m3u8?path=%2Flibrary"
        );
        assert_eq!(rewritten("/library/sections/"), "/library/sections");
        assert_eq!(rewritten("/"), "/");
        assert_eq!(rewritten("/library/metadata/1"), "/library/metadata/1");
    }

    #[test]
    fn pages() {
        let page = Page { start: 1, size: Some(2) };
        assert_eq!(page.apply(vec![1, 2, 3, 4]), (vec![2, 3], 4));
        let page = Page { start: 0, size: Some(0) };
        assert_eq!(page.apply(vec![1, 2]), (vec![], 2));
        assert_eq!(Page::default().apply(vec![1, 2]), (vec![1, 2], 2));
    }

    #[test]
    fn section_uuids_are_stable() {
        let info = PlexInfo { machine_id: "0123456789abcdef0123456789abcdef01234567".into() };
        assert_eq!(info.section_uuid(3), "01234567-89ab-cdef-0123-000000000003");
    }
}
