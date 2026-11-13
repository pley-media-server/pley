//! What clients ask before and after signing in: the server's name, id and
//! version, its branding (none), whether Quick Connect is on.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Serialize;

use super::{Jellyfin, JfError, JfResult, JfUser, PRODUCT, VERSION};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/system/info/public", get(public_info))
        .route("/system/info", get(info))
        .route("/system/ping", get(ping).post(ping))
        .route("/system/endpoint", get(endpoint))
        .route("/branding/configuration", get(branding))
        .route("/branding/css", get(css))
        .route("/branding/css.css", get(css))
        .route("/quickconnect/enabled", get(quick_connect_enabled))
        .route("/quickconnect/initiate", get(quick_connect_off).post(quick_connect_off))
        .route("/quickconnect/connect", get(quick_connect_off))
        .route("/plugins", get(none))
        .route("/packages", get(none))
        .route("/localization/cultures", get(none))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PublicSystemInfo {
    server_name: String,
    version: String,
    product_name: &'static str,
    operating_system: &'static str,
    id: String,
    startup_wizard_completed: bool,
}

fn public(state: &AppState, jellyfin: &Jellyfin) -> PublicSystemInfo {
    PublicSystemInfo {
        server_name: state.settings.get().server_name.clone(),
        version: VERSION.to_owned(),
        product_name: PRODUCT,
        operating_system: "",
        id: jellyfin.server_id.clone(),
        startup_wizard_completed: true,
    }
}

/// `/System/Info/Public`: what clients check an address with before they
/// sign in. Nothing about the computer, its addresses or its users.
async fn public_info(State(state): State<AppState>, Extension(jellyfin): Extension<Arc<Jellyfin>>) -> Response {
    Json(public(&state, &jellyfin)).into_response()
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SystemInfo {
    #[serde(flatten)]
    public: PublicSystemInfo,
    operating_system_display_name: &'static str,
    has_pending_restart: bool,
    is_shutting_down: bool,
    supports_library_monitor: bool,
    web_socket_port_number: u16,
    can_self_restart: bool,
    can_launch_web_browser: bool,
    has_update_available: bool,
    completed_installations: [(); 0],
    cast_receiver_applications: [(); 0],
}

/// `/System/Info`, for a signed-in client.
async fn info(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    _user: JfUser,
    headers: HeaderMap,
) -> JfResult<Response> {
    Ok(Json(SystemInfo {
        public: public(&state, &jellyfin),
        operating_system_display_name: "",
        has_pending_restart: false,
        is_shutting_down: false,
        supports_library_monitor: true,
        web_socket_port_number: request_port(&headers),
        can_self_restart: false,
        can_launch_web_browser: false,
        has_update_available: false,
        completed_installations: [],
        cast_receiver_applications: [],
    })
    .into_response())
}

/// The port the client reached the API on, where its socket goes too: the
/// `Host` header's, else HTTPS's or HTTP's default.
fn request_port(headers: &HeaderMap) -> u16 {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default();
    let port = host.rsplit_once(':').filter(|(name, _)| !name.ends_with(']') || host.starts_with('['));
    if let Some(port) = port.and_then(|(_, p)| p.parse().ok()) {
        return port;
    }
    let https = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) == Some("https");
    if https { 443 } else { 80 }
}

async fn ping() -> &'static str {
    PRODUCT
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Endpoint {
    is_local: bool,
    is_in_network: bool,
}

/// `/System/Endpoint`: clients ask it whether they are at home, to pick a
/// quality. Pley's own Automatic quality measures that per player instead.
async fn endpoint(_user: JfUser) -> Json<Endpoint> {
    Json(Endpoint { is_local: false, is_in_network: true })
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Branding {
    login_disclaimer: &'static str,
    custom_css: &'static str,
    splashscreen_enabled: bool,
}

async fn branding() -> Json<Branding> {
    Json(Branding { login_disclaimer: "", custom_css: "", splashscreen_enabled: false })
}

async fn css() -> Response {
    ([(axum::http::header::CONTENT_TYPE, "text/css")], "").into_response()
}

/// Quick Connect: signing in by approving a code in Pley isn't offered.
async fn quick_connect_enabled() -> Json<bool> {
    Json(false)
}

/// Quick Connect's other calls, answered as Jellyfin answers them while it
/// is off: apps that try it anyway are told so, and offer the password.
async fn quick_connect_off() -> JfError {
    JfError::Unauthorized
}

/// Lists Pley has nothing in (plugins and the catalog of them, cultures),
/// answered empty so clients that look (Infuse, for its own plugin) don't
/// stop there.
async fn none(_user: JfUser) -> Json<[(); 0]> {
    Json([])
}
