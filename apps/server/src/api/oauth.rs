//! The authorization server's endpoints: apps sign in on pley's own page
//! (OAuth 2.1, draft-ietf-oauth-v2-1-16, "OAuth 2.1" below), with PKCE and
//! client metadata documents (see [`crate::oauth`]).
//!
//! * `/.well-known/oauth-authorization-server` says where everything is
//!   (RFC 8414), for the address it was asked at.
//! * The authorization endpoint is a page of the web app,
//!   `/oauth/authorize`. It hands the app's request to
//!   `POST /api/oauth/requests`, which checks it, shows the person what the
//!   app asks for, and takes their answer; the browser then goes back to the
//!   app through `GET /api/oauth/requests/{id}/continue` (a 303). These take
//!   the web app's cookie and nothing else, from pley's own page only.
//! * Apps exchange codes and refresh tokens at `POST /api/oauth/token` and
//!   sign out at `POST /api/oauth/revoke`: forms, no cookies, no secrets.
//! * TVs, which have no usable browser, sign in with a code they show
//!   (RFC 8628): pley's own apps start at `POST /api/oauth/device`, the
//!   person types the code on the web app's `/link` page, which hands it to
//!   `POST /api/oauth/device/verify` (the cookie only, as above) and shows
//!   the consent, and the TV asks the token endpoint for the answer.
//! * pley's own apps, allowed `profiles`, list the household at
//!   `GET /api/oauth/profiles` and switch with a grant of the token
//!   endpoint, by the web app's rules.
//! * People see and sign out the apps using their account at
//!   `/api/me/apps`; administrators, anyone's at `/api/users/{id}/apps`.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

use super::auth::{
    NO_WAY_IN, NOT_ALLOWED, PIN_LOCKED, PIN_NEEDED, SwitchError, SwitchTarget, check_own_pin, check_switch,
    switch_needs, switch_targets,
};
use crate::auth::grants::{self, GrantRow, IssueError, Issued, NewGrant, RefreshError, RefreshInput, SwitchInput};
use crate::auth::scopes::{Scope, Scopes};
use crate::auth::{AdminUser, AuthUser, ClientIp, SourceIp, cookie_token, forwarded_https, hash_token};
use crate::error::{ApiResult, AppError};
use crate::oauth::AppSnapshot;
use crate::oauth::clients::{self, ClientError, RedirectKind};
use crate::oauth::codes::{self, CodeGrant, Redeem};
use crate::oauth::devices::{self, DEVICE_GRANT, Poll, Refused};
use crate::oauth::pending::{self, DecideError, Kind, PendingRequest, Redirect, ReturnedError};
use crate::state::AppState;
use crate::util::now;

/// The longest query the consent page may hand over.
const QUERY_MAX: usize = 8 * 1024;
/// The longest return address an app may ask for.
const REDIRECT_URI_MAX: usize = 512;
/// The longest `state` an app may send (bytes).
const STATE_MAX: usize = 1024;

const NOT_FROM_PLEY: &str = "this page must be opened from Pley itself";
/// A request that is over can't be told from one that never was, so the
/// sentence fits an app's request and a TV's alike.
const OVER: &str = "this request is over: start again from the app or the TV";
const BUSY: &str = "too many sign-ins in progress: try again in a minute";

/// The grant type of a profile switch by one of pley's own apps (an
/// extension grant, RFC 6749 §4.5).
pub(crate) const SWITCH_PROFILE: &str = "urn:pley:params:oauth:grant-type:switch-profile";

// ------------------------------------------------------------------ issuer

/// The address this server was reached at (RFC 8414 §2's issuer): "https"
/// when the proxy in front says so, the Host header, and nothing else, in
/// its canonical form (lower-case host, no default port, no trailing slash).
/// Each request has its own, so a server works at its home and public
/// addresses alike; a forged Host only changes the forger's own answers.
pub(crate) fn issuer(headers: &HeaderMap) -> ApiResult<String> {
    let unknown = || AppError::bad_request("Pley can't tell which address it was reached at: open it by its address");
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).map(str::trim).unwrap_or_default();
    if host.is_empty() || host.contains(['/', '\\', '?', '#', '@']) {
        return Err(unknown());
    }
    let scheme = if forwarded_https(headers) { "https" } else { "http" };
    let url = Url::parse(&format!("{scheme}://{host}")).map_err(|_| unknown())?;
    let plain = url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.host().is_some();
    if !plain {
        return Err(unknown());
    }
    Ok(url.origin().ascii_serialization())
}

/// The host of an issuer.
fn issuer_host(issuer: &str) -> String {
    Url::parse(issuer).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default()
}

/// Whether an app's `resource` (RFC 8707) names this server: absent, or the
/// issuer with or without a trailing slash.
fn is_this_server(resource: Option<&str>, issuer: &str) -> bool {
    resource.is_none_or(|resource| resource.strip_suffix('/').unwrap_or(resource) == issuer)
}

// ------------------------------------------------------------------ metadata

/// `GET /.well-known/oauth-authorization-server` (RFC 8414): where an app
/// signs in, for the address it asked at. Never cached: it depends on Host.
pub async fn metadata(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    let issuer = issuer(&headers)?;
    let body = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/api/oauth/token"),
        "revocation_endpoint": format!("{issuer}/api/oauth/revoke"),
        // RFC 8628 §4.
        "device_authorization_endpoint": format!("{issuer}/api/oauth/device"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token", SWITCH_PROFILE, DEVICE_GRANT],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": Scope::ALL.map(Scope::name),
        "authorization_response_iss_parameter_supported": true,
        "client_id_metadata_document_supported": state.settings.get().third_party_apps,
        "service_documentation": "https://github.com/pley-media-server/pley/blob/master/docs/API.md",
    });
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response())
}

/// Any other `/.well-known/` document: pley has none (and not the web app's
/// page either, which would look like an answer).
pub async fn no_document() -> AppError {
    AppError::not_found("no such document")
}

// ------------------------------------------------------------------ the web app's own page

/// Someone signed in to pley's web app, on pley's own page: the session
/// cookie only (never `Authorization`, `X-Pley-Token` or `?token=`, so
/// neither an app nor a script can allow an app), and for changes, a request
/// whose Origin is this server (and Sec-Fetch-Site same-origin when the
/// browser sends it). No CORS anywhere (RFC 9700 §2.6).
pub struct WebUser {
    pub user: AuthUser,
    /// SHA-256 of the session token: requests belong to this session.
    pub session_hash: String,
}

impl FromRequestParts<AppState> for WebUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = cookie_token(parts).ok_or(AppError::Unauthorized)?;
        // Never an app's token: validate takes sessions only.
        let user = state.auth.validate(&token).await.ok_or(AppError::Unauthorized)?;
        if !from_this_site(parts) {
            return Err(AppError::forbidden(NOT_FROM_PLEY));
        }
        Ok(WebUser { user, session_hash: hash_token(&token) })
    }
}

/// Whether a request comes from pley's own page (see [`WebUser`]).
/// Sec-Fetch-Site: <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Sec-Fetch-Site>.
fn from_this_site(parts: &Parts) -> bool {
    let site = parts.headers.get("sec-fetch-site").map(|v| v.to_str().unwrap_or_default());
    if parts.method == Method::GET || parts.method == Method::HEAD {
        // Going back to the app: a navigation from the page, or typed.
        return site.is_none_or(|site| matches!(site, "same-origin" | "none"));
    }
    if site.is_some_and(|site| site != "same-origin") {
        return false;
    }
    crate::auth::origin_is_host(parts)
}

// ------------------------------------------------------------------ requests (the consent page)

#[derive(Deserialize)]
pub struct StartRequest {
    /// The consent page's query, without "?".
    query: String,
}

#[derive(Serialize)]
struct ClientDto<'a> {
    id: &'a str,
    host: &'a str,
    name: Option<&'a str>,
    first_party: bool,
}

/// The parameters pley reads, at the authorization, token, device and
/// revocation endpoints: each may come once.
const READ: &[&str] = &[
    "response_type",
    "response_mode",
    "client_id",
    "client_secret",
    "redirect_uri",
    "scope",
    "state",
    "code_challenge",
    "code_challenge_method",
    "resource",
    "request",
    "request_uri",
    "grant_type",
    "code",
    "code_verifier",
    "refresh_token",
    "device_code",
    "profile",
    "pin",
    "token",
];

/// The parameters of a query or a form (OAuth 2.1 §3.1-3.2): one sent
/// without a value is as if left out, and those pley doesn't read are
/// ignored, given twice or not. One that pley reads given twice is the
/// error: its name.
fn parameters(text: &[u8]) -> Result<Vec<(String, String)>, String> {
    let mut seen = HashSet::new();
    let mut params = Vec::new();
    for (name, value) in url::form_urlencoded::parse(text) {
        if value.is_empty() {
            continue;
        }
        if READ.contains(&name.as_ref()) && !seen.insert(name.clone()) {
            return Err(name.into_owned());
        }
        params.push((name.into_owned(), value.into_owned()));
    }
    Ok(params)
}

/// The value of `name`, when it was given (never empty: see [`parameters`]).
fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// A parameter's name for a message, when it is a plain one.
fn shown_name(name: &str) -> Option<&str> {
    (name.len() <= 32 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then_some(name)
}

/// `POST /api/oauth/requests`: the consent page hands over the app's
/// request (its query). Checked in this order (OAuth 2.1 §4.1.2.1):
/// a. every parameter pley reads at most once;
/// b. the app (built in, or read from its description when allowed);
/// c. the return address, registered (exact, but for a loopback port).
///
/// What fails there is shown on the page and never sent to the app: pley
/// can't know the address is the app's (OAuth 2.1 §4.1.2.1; RFC 9700
/// §4.11.2). Everything after that goes back to the app, but only when the
/// person clicks (OAuth 2.1 §7.13.2: no open redirector).
pub async fn create_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    web: WebUser,
    Json(body): Json<StartRequest>,
) -> ApiResult<Json<Value>> {
    let issuer = issuer(&headers)?;
    if body.query.len() > QUERY_MAX {
        return Err(AppError::bad_request("the app's request is too long: ask the app's publisher to fix it"));
    }
    let params = parameters(body.query.as_bytes()).map_err(|name| {
        AppError::bad_request(match shown_name(&name) {
            Some(name) => format!("the app's request gives {name} twice: ask the app's publisher to fix it"),
            None => "the app's request gives a parameter twice: ask the app's publisher to fix it".to_owned(),
        })
    })?;
    let client_id = param(&params, "client_id").ok_or_else(|| {
        AppError::bad_request("the app didn't say which app it is: ask the app's publisher to fix it")
    })?;
    let third_party_allowed = state.settings.get().third_party_apps;
    let client =
        state.oauth.documents.get(client_id, web.user.id, third_party_allowed, &issuer_host(&issuer)).await.map_err(
            |err| match err {
                ClientError::Busy => AppError::TooManyRequests(err.to_string()),
                err => AppError::bad_request(err.to_string()),
            },
        )?;
    let redirect_uri = param(&params, "redirect_uri").ok_or_else(|| {
        AppError::bad_request("the app didn't say where to send you back: ask the app's publisher to fix it")
    })?;
    let unlisted =
        || AppError::bad_request("this app didn't list that return address: ask the app's publisher to fix it");
    if redirect_uri.len() > REDIRECT_URI_MAX {
        return Err(unlisted());
    }
    let kind = clients::redirect_kind(&client, redirect_uri).map_err(|_| unlisted())?;
    // Where the answer goes, shown before any click: with consent, and with
    // a mistake that goes back to the app (RFC 9700 §4.11.2).
    let redirect = match kind {
        RedirectKind::Web { host } => json!({"kind": "web", "to": host}),
        RedirectKind::App { scheme } => json!({"kind": "app", "to": scheme}),
        RedirectKind::Loopback => {
            let host = Url::parse(redirect_uri).ok().and_then(|url| url.host_str().map(str::to_owned));
            json!({"kind": "loopback", "to": host.unwrap_or_default()})
        }
    };

    let checked = check_request(&params, &client, &issuer);
    let (scopes, error) = match checked {
        Ok(scopes) => (scopes, None),
        Err(error) => (Scopes::default(), Some(error)),
    };
    let request = PendingRequest {
        user_id: web.user.id,
        session_hash: web.session_hash,
        issuer,
        app: (&client).into(),
        kind: Kind::Redirect(Redirect {
            redirect_uri: redirect_uri.to_owned(),
            state: param(&params, "state").map(str::to_owned),
            code_challenge: if error.is_none() {
                param(&params, "code_challenge").unwrap_or_default().to_owned()
            } else {
                String::new()
            },
        }),
        scopes,
        error: error.clone(),
    };
    let id = state.oauth.pending.create(request).map_err(|_| AppError::Unavailable(BUSY.into()))?;
    let client_dto =
        ClientDto { id: &client.id, host: &client.host, name: client.name.as_deref(), first_party: client.first_party };
    if let Some(error) = error {
        return Ok(Json(json!({
            "status": "error",
            "error": error.error,
            "message": error.description,
            "back_url": format!("/api/oauth/requests/{id}/continue"),
            "client": client_dto,
            "redirect": redirect,
        })));
    }
    Ok(Json(json!({
        "status": "consent",
        "id": id,
        "expires_at": now() + pending::PENDING_LIFETIME.as_secs() as i64,
        "client": client_dto,
        "redirect": redirect,
        "scopes": scopes.names(),
    })))
}

/// The checks of a request whose mistakes go back to the app (d. of
/// [`create_request`]); the scopes it asks for. Descriptions are ASCII
/// (RFC 6749 §4.1.2.1): the app gets them as error_description.
fn check_request(params: &[(String, String)], client: &clients::Client, issuer: &str) -> Result<Scopes, ReturnedError> {
    let fail = |error: &'static str, description: &str| ReturnedError { error, description: description.to_owned() };
    match param(params, "response_type") {
        Some("code") => {}
        None => {
            return Err(fail(
                "invalid_request",
                "this app didn't say how it signs in (response_type): ask the app's publisher to fix it",
            ));
        }
        Some(_) => {
            return Err(fail(
                "unsupported_response_type",
                "this app asked to sign in in a way Pley doesn't support (response_type): ask the app's publisher to \
                 fix it",
            ));
        }
    }
    if param(params, "response_mode").is_some_and(|mode| mode != "query") {
        return Err(fail(
            "invalid_request",
            "this app asked for its answer in a way Pley doesn't support (response_mode): ask the app's publisher to \
             fix it",
        ));
    }
    if param(params, "request").is_some() || param(params, "request_uri").is_some() {
        return Err(fail("invalid_request", "request objects aren't supported: ask the app's publisher to fix it"));
    }
    let challenge_ok = param(params, "code_challenge_method") == Some("S256")
        && param(params, "code_challenge").is_some_and(codes::is_challenge);
    if !challenge_ok {
        return Err(fail(
            "invalid_request",
            "this app didn't protect its sign-in the way Pley requires (PKCE with S256): ask the app's publisher to \
             fix it",
        ));
    }
    let asked = asked_scopes(param(params, "scope"), client)?;
    if !is_this_server(param(params, "resource"), issuer) {
        return Err(fail(
            "invalid_target",
            "this app asked for another server (resource): ask the app's publisher to fix it",
        ));
    }
    if param(params, "state").is_some_and(|state| state.len() > STATE_MAX) {
        return Err(fail("invalid_request", "this app's state is too long: ask the app's publisher to fix it"));
    }
    Ok(asked)
}

/// The scopes an app asks for in `scope` (here and at the device endpoint):
/// none means `library`; names pley doesn't know (openid among them),
/// `profiles` and `approvals` for apps by other publishers, and more than
/// the app's description allows are refused.
fn asked_scopes(scope: Option<&str>, client: &clients::Client) -> Result<Scopes, ReturnedError> {
    let fail = |description: &str| ReturnedError { error: "invalid_scope", description: description.to_owned() };
    let asked = match scope.map(Scopes::parse) {
        None => Ok(Scopes::default()),
        Some(parsed) => parsed,
    };
    let asked = match asked {
        Ok(asked) if asked.is_empty() => Scopes::default().with(Scope::Library),
        Ok(asked) => asked,
        Err(name) if name == "openid" => {
            return Err(fail("Pley doesn't sign people in to other sites (openid): ask the app's publisher to fix it"));
        }
        Err(_) => return Err(fail("this app asked for access Pley doesn't offer: ask the app's publisher to fix it")),
    };
    if asked.contains(Scope::Profiles) && !client.first_party {
        // A profile's PIN is typed into the app: pley's own apps only.
        return Err(fail("only Pley's own apps can switch profiles (profiles): ask the app's publisher to fix it"));
    }
    if asked.contains(Scope::Approvals) && !client.first_party {
        // Deciding what everyone asks for: pley's own apps only.
        return Err(fail("only Pley's own apps can approve requests (approvals): ask the app's publisher to fix it"));
    }
    if client.scope_cap.as_deref().is_some_and(|cap| !asked.is_subset_of(Scopes::from_stored(cap))) {
        return Err(fail("this app asked for more than its description allows: ask the app's publisher to fix it"));
    }
    Ok(asked)
}

#[derive(Deserialize)]
pub struct Decision {
    allow: bool,
}

/// `POST /api/oauth/requests/{id}`: the person allows the app (a code is
/// made) or not. The same answer again gets the same reply; the other one
/// a conflict. A TV's request answers the TV instead
/// ([`decide_for_device`]). One that is over, an app's or a TV's, gets 404
/// ([`OVER`]).
pub async fn decide(
    State(state): State<AppState>,
    web: WebUser,
    Path(id): Path<String>,
    Json(body): Json<Decision>,
) -> ApiResult<Json<Value>> {
    let shown = state.oauth.pending.get(&id, web.user.id, &web.session_hash);
    if shown.is_some_and(|shown| matches!(shown.kind, Kind::Device(_))) {
        return decide_for_device(&state, &web, &id, body.allow);
    }
    let decided = state.oauth.pending.decide(&id, web.user.id, &web.session_hash, body.allow, |request, back| {
        state.oauth.codes.new_code(CodeGrant {
            app: request.app.clone(),
            redirect_uri: back.redirect_uri.clone(),
            code_challenge: back.code_challenge.clone(),
            user_id: request.user_id,
            session_hash: request.session_hash.clone(),
            scopes: request.scopes,
        })
    });
    match decided {
        Ok(()) => Ok(Json(json!({"continue_url": format!("/api/oauth/requests/{id}/continue")}))),
        Err(DecideError::NotFound) => Err(AppError::not_found(OVER)),
        Err(DecideError::Conflict) => {
            Err(AppError::conflict("this request was answered already: start again from the app"))
        }
        Err(DecideError::Code(_)) => Err(AppError::Unavailable(BUSY.into())),
    }
}

/// `GET /api/oauth/requests/{id}/continue`: back to the app with the
/// answer, by a 303 (RFC 9700 §4.12) to its return address as it was sent:
/// code, state and iss when allowed; error (access_denied, or the request's
/// mistake), state and iss otherwise (RFC 9207: iss in every answer).
/// It repeats while the code works and wasn't exchanged, and for a minute
/// after a refusal or a mistake. After that, a browser that follows the
/// link (the "Open the app" link of a tab left open, say) is sent to the
/// web app's page saying the request is over, and a script gets 404.
pub async fn continue_to_app(
    State(state): State<AppState>,
    headers: HeaderMap,
    web: WebUser,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let back = state
        .oauth
        .pending
        .location(&id, web.user.id, &web.session_hash)
        .filter(|back| back.code.as_deref().is_none_or(|code| state.oauth.codes.unredeemed(code)));
    let location = match back {
        Some(back) => HeaderValue::from_str(&back.location).map_err(|e| AppError::Internal(e.into()))?,
        None if is_navigation(&headers) => HeaderValue::from_static(OVER_PAGE),
        None => return Err(AppError::not_found(OVER)),
    };
    Ok((
        StatusCode::SEE_OTHER,
        [(header::LOCATION, location), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
    )
        .into_response())
}

/// The web app's page for a request that is over.
const OVER_PAGE: &str = "/oauth/over";

/// Whether the browser asked for a page to show (it followed a link),
/// rather than a script for data.
fn is_navigation(headers: &HeaderMap) -> bool {
    let value = |name| headers.get(name).and_then(|v: &HeaderValue| v.to_str().ok()).unwrap_or_default();
    value("sec-fetch-mode") == "navigate" || value(header::ACCEPT.as_str()).contains("text/html")
}

// ------------------------------------------------------------------ token and revocation (apps)

/// An error of the token and revocation endpoints (RFC 6749 §5.2).
#[derive(Debug)]
pub struct OAuthError {
    status: StatusCode,
    error: &'static str,
    description: String,
    /// The app tried HTTP Basic authentication: say how it would work.
    basic: bool,
    /// Why a profile switch was refused, for the app to word it (an extra
    /// field beside RFC 6749's).
    reason: Option<&'static str>,
}

impl OAuthError {
    fn new(error: &'static str, description: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, error, description: description.into(), basic: false, reason: None }
    }

    fn invalid_request(description: impl Into<String>) -> Self {
        Self::new("invalid_request", description)
    }

    fn invalid_grant(description: impl Into<String>) -> Self {
        Self::new("invalid_grant", description)
    }

    fn invalid_client(description: &str, basic: bool) -> Self {
        Self { status: StatusCode::UNAUTHORIZED, basic, ..Self::new("invalid_client", description) }
    }

    fn server_error(err: impl std::fmt::Display) -> Self {
        tracing::error!("an app's token request failed: {err}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            ..Self::new("server_error", "Pley couldn't answer: try again")
        }
    }
}

/// What every answer of the token endpoint says about caching (RFC 6749 §5.1).
const NO_STORE: [(header::HeaderName, HeaderValue); 2] = [
    (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
    (header::PRAGMA, HeaderValue::from_static("no-cache")),
];

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let mut body = json!({"error": self.error, "error_description": self.description});
        if let Some(reason) = self.reason {
            body["reason"] = json!(reason);
        }
        let mut response = (self.status, NO_STORE, Json(body)).into_response();
        if self.basic {
            response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static(r#"Basic realm="pley""#));
        }
        response
    }
}

/// The parameters of a token or revocation request: a form
/// (application/x-www-form-urlencoded) of at most 16 KiB, each parameter at
/// most once, and no client authentication (every app is public).
struct Form(Vec<(String, String)>);

impl Form {
    fn read(headers: &HeaderMap, body: Result<Bytes, BytesRejection>) -> Result<Form, OAuthError> {
        let basic = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split_once(' '))
            .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("basic"));
        if basic {
            return Err(OAuthError::invalid_client(
                "Pley's apps have no secrets: leave out the Authorization header",
                true,
            ));
        }
        let form = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/x-www-form-urlencoded"));
        if !form {
            return Err(OAuthError::invalid_request(
                "send the parameters as a form (application/x-www-form-urlencoded)",
            ));
        }
        let body = body.map_err(|_| OAuthError::invalid_request("the request is too large"))?;
        let params = parameters(&body).map_err(|name| {
            OAuthError::invalid_request(match shown_name(&name) {
                Some(name) => format!("{name} is given twice"),
                None => "a parameter is given twice".to_owned(),
            })
        })?;
        if param(&params, "client_secret").is_some() {
            // 400: a 401 would need a challenge (RFC 9110 §15.5.2), and there
            // is no way of authenticating to offer.
            return Err(OAuthError::new("invalid_client", "Pley's apps have no secrets: leave out client_secret"));
        }
        Ok(Form(params))
    }

    fn get(&self, name: &str) -> Option<&str> {
        param(&self.0, name)
    }

    fn required(&self, name: &'static str) -> Result<&str, OAuthError> {
        self.get(name).ok_or_else(|| OAuthError::invalid_request(format!("{name} is missing")))
    }

    /// `resource` (RFC 8707), when sent, must be this server.
    fn check_resource(&self, headers: &HeaderMap) -> Result<(), OAuthError> {
        let Some(resource) = self.get("resource") else { return Ok(()) };
        let issuer = issuer(headers).map_err(|_| OAuthError::invalid_request("the Host header is missing or wrong"))?;
        if is_this_server(Some(resource), &issuer) {
            Ok(())
        } else {
            Err(OAuthError::new("invalid_target", "tokens are for this server only (resource)"))
        }
    }
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_owned)
}

/// A token answer (RFC 6749 §5.1), never cached.
fn tokens(issued: Issued) -> Response {
    let mut body = json!({
        "access_token": issued.access_token,
        "token_type": "Bearer",
        "expires_in": issued.expires_in,
        "scope": issued.scopes.to_string(),
    });
    if let Some(refresh) = issued.refresh_token {
        body["refresh_token"] = json!(refresh);
    }
    (StatusCode::OK, NO_STORE, Json(body)).into_response()
}

/// `POST /api/oauth/token`: a code for the first tokens
/// (`authorization_code`), a refresh token for new ones (`refresh_token`,
/// rotated every time), a TV's question whether it was allowed
/// ([`DEVICE_GRANT`]), or a refresh token for tokens acting as another
/// profile of the household ([`SWITCH_PROFILE`], pley's own apps).
pub async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    ClientIp(ip): ClientIp,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let form = match Form::read(&headers, body) {
        Ok(form) => form,
        Err(err) => return err.into_response(),
    };
    let answer = match form.get("grant_type") {
        Some("authorization_code") => exchange_code(&state, &headers, ip, &form).await,
        Some("refresh_token") => refresh(&state, &headers, ip, &form).await,
        Some(DEVICE_GRANT) => device_token(&state, &headers, ip, &form).await,
        Some(SWITCH_PROFILE) => switch_profile(&state, &headers, ip, &form).await,
        Some(_) => Err(OAuthError::new(
            "unsupported_grant_type",
            "Pley knows authorization_code, refresh_token, the device code grant and its own apps' profile switch",
        )),
        None => Err(OAuthError::invalid_request("grant_type is missing")),
    };
    answer.unwrap_or_else(IntoResponse::into_response)
}

/// grant_type=authorization_code (OAuth 2.1 §4.1.3). Anything wrong leaves
/// the code as it was; a right exchange after the first ends what the first
/// one got (§7.5.3). The code works only while the browser session that
/// allowed the app is signed in: a password change or signing out ends
/// codes too, as it ends the apps signed in already.
async fn exchange_code(
    state: &AppState,
    headers: &HeaderMap,
    ip: Option<String>,
    form: &Form,
) -> Result<Response, OAuthError> {
    let code = form.required("code")?;
    let client_id = form.required("client_id")?;
    let verifier = form.required("code_verifier")?;
    if !codes::is_verifier(verifier) {
        return Err(OAuthError::invalid_request("code_verifier must be 43 to 128 of A-Z a-z 0-9 . _ ~ -"));
    }
    form.check_resource(headers)?;
    let grant = match state.oauth.codes.redeem(code, client_id, form.get("redirect_uri"), verifier) {
        Redeem::First(grant) => grant,
        Redeem::Invalid(why) => return Err(OAuthError::invalid_grant(why)),
        Redeem::Replay(made) => {
            tracing::warn!(
                "{client_id} exchanged a code a second time (someone else may have it): its access is ended"
            );
            if let Some(token_hash) = made
                && let Err(err) = state.auth.end_grant_of_token(&token_hash).await
            {
                tracing::error!("could not end an app's access: {err}");
            }
            return Err(OAuthError::invalid_grant("the code was used already"));
        }
    };
    // Things may have changed in the minute since the person allowed it.
    let user = match crate::auth::AuthUser::load(&state.db, grant.user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            state.oauth.codes.failed(code);
            return Err(OAuthError::invalid_grant("the account is disabled"));
        }
        Err(err) => {
            state.oauth.codes.failed(code);
            return Err(OAuthError::server_error(err));
        }
    };
    if clients::builtin(&grant.app.id).is_none() && !state.settings.get().third_party_apps {
        state.oauth.codes.failed(code);
        return Err(OAuthError::invalid_grant("apps by other publishers can't sign in to this server"));
    }
    let issued = state
        .auth
        .issue(NewGrant {
            user_id: user.id,
            approved_by: user.id,
            client_id: grant.app.id.clone(),
            client_name: grant.app.name.clone(),
            scopes: grant.scopes,
            refresh: grant.app.refresh,
            first_party: grant.app.first_party,
            session_hash: Some(grant.session_hash.clone()),
            user_agent: user_agent(headers),
            ip,
        })
        .await;
    let issued = match issued {
        Ok(issued) => issued,
        Err(IssueError::SignedOut) => {
            state.oauth.codes.failed(code);
            return Err(OAuthError::invalid_grant(SIGNED_OUT));
        }
        Err(IssueError::Internal(err)) => {
            state.oauth.codes.failed(code);
            return Err(OAuthError::server_error(err));
        }
    };
    let refusal = match state.oauth.codes.issued(code, issued.on_record()) {
        codes::Issued::Kept => None,
        codes::Issued::Replayed => {
            tracing::warn!(
                "{} exchanged a code a second time while it was being issued (someone else may have it): its access \
                 is ended",
                grant.app.label()
            );
            Some("the code was used already")
        }
        codes::Issued::SignedOut => Some(SIGNED_OUT),
    };
    if let Some(refusal) = refusal {
        if let Err(err) = state.auth.end_grant(issued.grant_id).await {
            tracing::error!("could not end an app's access: {err}");
        }
        return Err(OAuthError::invalid_grant(refusal));
    }
    tracing::info!("{} may use {}'s account ({})", grant.app.label(), user.username, issued.scopes);
    Ok(tokens(issued))
}

/// Why a code or a TV's answer got nothing although it was right.
const SIGNED_OUT: &str = "the person who allowed it signed out since: sign in again";

/// grant_type=refresh_token (OAuth 2.1 §4.3): rotated every time; a spent
/// one shown again ends the app's access (see [`grants::refresh`]).
async fn refresh(
    state: &AppState,
    headers: &HeaderMap,
    ip: Option<String>,
    form: &Form,
) -> Result<Response, OAuthError> {
    let token = form.required("refresh_token")?;
    let client_id = form.required("client_id")?;
    let scope = match form.get("scope").map(Scopes::parse) {
        None => None,
        Some(Ok(scope)) => Some(scope),
        Some(Err(_)) => return Err(OAuthError::new("invalid_scope", "the scope names something Pley doesn't offer")),
    };
    form.check_resource(headers)?;
    if clients::builtin(client_id).is_none() && !state.settings.get().third_party_apps {
        return Err(OAuthError::invalid_grant("apps by other publishers can't sign in to this server"));
    }
    let input = RefreshInput {
        token: token.to_owned(),
        client_id: client_id.to_owned(),
        scope,
        user_agent: user_agent(headers),
        ip,
    };
    match state.auth.refresh(input).await {
        Ok(issued) => Ok(tokens(issued)),
        Err(RefreshError::WiderScope) => {
            Err(OAuthError::new("invalid_scope", "the app asked for more than it was allowed"))
        }
        Err(RefreshError::Internal(err)) => Err(OAuthError::server_error(err)),
        Err(err) => Err(OAuthError::invalid_grant(err.to_string())),
    }
}

// ------------------------------------------------------------------ TVs (RFC 8628)

const NOT_A_CODE: &str = "that isn't a code from a Pley app: codes are 8 letters, like BCDF-GHJK";
const NO_SUCH_CODE: &str = "no TV is waiting for that code: check the code on your TV, or start again there";
const CODE_USED: &str = "that code was already used: start again on your TV";

/// `POST /api/oauth/device` (RFC 8628 §3.1-3.2): a TV starts signing in. It
/// gets a code to show with the address where to type it, then asks the
/// token endpoint for the answer every `interval` seconds. pley's own apps
/// only: anyone may call this, and pley reads no app's description for
/// someone who isn't signed in. The form rules are the token endpoint's.
pub async fn device_authorization(
    State(state): State<AppState>,
    headers: HeaderMap,
    SourceIp(source): SourceIp,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let form = match Form::read(&headers, body) {
        Ok(form) => form,
        Err(err) => return err.into_response(),
    };
    start_on_device(&state, &headers, source, &form).unwrap_or_else(IntoResponse::into_response)
}

/// A TV's request to sign in, from `source` (see [`SourceIp`]: the limit
/// per address counts by it, never by a header the caller writes).
fn start_on_device(
    state: &AppState,
    headers: &HeaderMap,
    source: Option<IpAddr>,
    form: &Form,
) -> Result<Response, OAuthError> {
    let client_id = form.required("client_id")?;
    let Some(client) = clients::builtin(client_id) else {
        return Err(OAuthError::new("invalid_client", "only Pley's own apps can sign in with a code"));
    };
    if !client.device {
        return Err(OAuthError::new("unauthorized_client", "this app doesn't sign in with a code"));
    }
    let scopes = asked_scopes(form.get("scope"), client).map_err(|err| OAuthError::new(err.error, err.description))?;
    let issuer = issuer(headers).map_err(|_| OAuthError::invalid_request("the Host header is missing or wrong"))?;
    let app = AppSnapshot::from(client);
    // What the device says it is, so the person typing the code can tell
    // which one is signing in.
    let device = user_agent(headers).map(|agent| grant_device(&agent)).as_deref().and_then(clients::clean_name);
    let asker = devices::Asker { address: source, device };
    let from = source.map_or_else(|| "an unknown address".to_owned(), |source| source.to_string());
    let said = asker.device.as_deref().map(|device| format!(" ({device})")).unwrap_or_default();
    // When full, nothing in progress is dropped to make room.
    let started = state.oauth.devices.create(app.clone(), scopes, asker).map_err(|_| OAuthError {
        status: StatusCode::TOO_MANY_REQUESTS,
        ..OAuthError::new("temporarily_unavailable", BUSY)
    })?;
    tracing::info!("{}{said} asked to sign in with a code from {from}", app.label());
    let body = json!({
        "device_code": started.device_code,
        "user_code": devices::shown(&started.user_code),
        "verification_uri": format!("{issuer}/link"),
        "verification_uri_complete": format!("{issuer}/link?code={}", started.user_code),
        "expires_in": devices::LIFETIME.as_secs(),
        "interval": devices::INTERVAL.as_secs(),
    });
    Ok((StatusCode::OK, NO_STORE, Json(body)).into_response())
}

/// The code a TV asks for the QR code of.
#[derive(Deserialize)]
pub struct QrCode {
    code: String,
}

/// `GET /api/oauth/device/qr?code=…`: where to type a TV's code, as a QR code
/// in a PNG, for TVs that can't draw one (pley for Roku): this server's
/// `/link?code=…`, the same address as `verification_uri_complete`. Anyone
/// may ask, as anyone may start signing in: no request is looked up, so it
/// says nothing about which codes exist.
pub async fn device_qr(headers: HeaderMap, Query(asked): Query<QrCode>) -> ApiResult<Response> {
    let code = devices::normalize(&asked.code).ok_or_else(|| AppError::bad_request(NOT_A_CODE))?;
    let issuer = issuer(&headers)?;
    let png = qr_png(&format!("{issuer}/link?code={code}"))
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("a QR code that couldn't be drawn")))?;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "no-store")], png).into_response())
}

/// Pixels to a module of [`qr_png`]'s codes, and modules of quiet zone
/// around them, which scanners need.
const QR_SCALE: u32 = 8;
const QR_QUIET: i32 = 4;

/// `text` as a QR code (medium error correction): black on white, with its
/// quiet zone.
fn qr_png(text: &str) -> Option<Vec<u8>> {
    let code = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let side = (code.size() + 2 * QR_QUIET) as u32 * QR_SCALE;
    let picture = image::GrayImage::from_fn(side, side, |x, y| {
        // Outside the code (the quiet zone) a module is light.
        let dark = code.get_module((x / QR_SCALE) as i32 - QR_QUIET, (y / QR_SCALE) as i32 - QR_QUIET);
        image::Luma([if dark { 0 } else { 255 }])
    });
    let mut png = Vec::new();
    picture.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(png)
}

/// What the `/link` page sends: the code the TV shows, as it was typed.
#[derive(Deserialize)]
pub struct TypedCode {
    user_code: String,
}

/// `POST /api/oauth/device/verify`: the person typed the code their TV
/// shows on the web app's `/link` page (RFC 8628 §3.3). The answer is the
/// consent `POST /api/oauth/requests` answers with, for a request whose
/// answer goes to the TV. The cookie only, from pley's own page
/// ([`WebUser`]).
///
/// Wrong codes count (see [`devices::Devices::find`]). A code shown on
/// another page already is used, unless the session of that page ended
/// (signed out, or switched to another profile): then this page takes it
/// over, as it does from an earlier page of this session.
pub async fn verify_user_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    web: WebUser,
    Json(body): Json<TypedCode>,
) -> ApiResult<Json<Value>> {
    let issuer = issuer(&headers)?;
    let code = devices::normalize(&body.user_code).ok_or_else(|| AppError::bad_request(NOT_A_CODE))?;
    let found = state.oauth.devices.find(&code, web.user.id).map_err(|refused| match refused {
        Refused::Throttled => {
            AppError::TooManyRequests("too many wrong codes: wait a few minutes and try again".into())
        }
        Refused::Unknown => AppError::not_found(NO_SUCH_CODE),
        Refused::Used => AppError::conflict(CODE_USED),
    })?;
    if let Some((user_id, session_hash)) = found.deciding.as_deref().and_then(|id| state.oauth.pending.owner(id)) {
        let mine = user_id == web.user.id && session_hash == web.session_hash;
        if !mine && signed_in(&state, &session_hash).await? {
            return Err(AppError::conflict(CODE_USED));
        }
    }
    // It ends no later than the TV's request.
    let lifetime = found.left.min(pending::PENDING_LIFETIME);
    let request = PendingRequest {
        user_id: web.user.id,
        session_hash: web.session_hash,
        issuer,
        app: found.app.clone(),
        kind: Kind::Device(found.key.clone()),
        scopes: found.scopes,
        error: None,
    };
    let id = state.oauth.pending.create_lasting(request, lifetime).map_err(|_| AppError::Unavailable(BUSY.into()))?;
    if !state.oauth.devices.link(&found.key, found.deciding.as_deref(), &id) {
        // Someone was quicker.
        state.oauth.pending.remove(&id);
        return Err(AppError::conflict(CODE_USED));
    }
    if let Some(before) = &found.deciding {
        // The page that showed it before can't answer any more.
        state.oauth.pending.remove(before);
    }
    let app = &found.app;
    let address = found.asker.address.map(|address| address.to_canonical().to_string());
    Ok(Json(json!({
        "status": "consent",
        "id": id,
        "expires_at": now() + lifetime.as_secs() as i64,
        "client": ClientDto { id: &app.id, host: &app.host, name: app.name.as_deref(), first_party: app.first_party },
        "redirect": {"kind": "device", "to": devices::shown(&code), "device": found.asker.device, "address": address},
        "scopes": found.scopes.names(),
    })))
}

/// Whether the browser session whose token has the SHA-256 `session_hash`
/// is still signed in, to an enabled account.
async fn signed_in(state: &AppState, session_hash: &str) -> ApiResult<bool> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT s.user_id FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ? AND NOT u.disabled",
    )
    .bind(session_hash)
    .bind(now())
    .fetch_optional(&state.db.read)
    .await?;
    Ok(found.is_some())
}

/// `POST /api/oauth/requests/{id}` for a TV's request: allowed, the TV acts
/// as the profile the person uses, who allowed it; refused, it is told so.
/// The TV gets the answer when it next asks: nothing goes back through the
/// browser.
fn decide_for_device(state: &AppState, web: &WebUser, id: &str, allow: bool) -> ApiResult<Json<Value>> {
    let decided = state.oauth.pending.decide_device(id, web.user.id, &web.session_hash, allow, |request, key| {
        state.oauth.devices.decide(key, id, allow, web.user.id, &web.session_hash, request.scopes)?;
        if allow {
            tracing::info!(
                "{} may use {}'s account ({}) on a device",
                request.app.label(),
                web.user.username,
                request.scopes
            );
        }
        Ok(())
    });
    match decided {
        Ok(()) => Ok(Json(json!({"done": true}))),
        Err(DecideError::NotFound | DecideError::Code(devices::Over)) => Err(AppError::not_found(OVER)),
        Err(DecideError::Conflict) => {
            Err(AppError::conflict("this request was answered already: start again on your TV"))
        }
    }
}

/// grant_type=urn:ietf:params:oauth:grant-type:device_code (RFC 8628
/// §3.4-3.5): a TV asks whether it was allowed. Until the person answers:
/// authorization_pending, or slow_down when it asks sooner than its
/// interval, which then grows by 5 seconds. Then access_denied, or its
/// tokens, once; after 10 minutes, expired_token.
///
/// A device code never appears in an address or a page, so one shown again
/// after its answer is just unknown: unlike an authorization code, it can't
/// have leaked on the way, and nothing has to end.
///
/// As with codes, the tokens are made only while the browser session that
/// allowed the TV is signed in, and what the TV got ends when the account's
/// sign-ins end while they are made.
async fn device_token(
    state: &AppState,
    headers: &HeaderMap,
    ip: Option<String>,
    form: &Form,
) -> Result<Response, OAuthError> {
    let device_code = form.required("device_code")?;
    let client_id = form.required("client_id")?;
    form.check_resource(headers)?;
    let allowed = match state.oauth.devices.poll(device_code, client_id) {
        Poll::Allowed(allowed) => allowed,
        Poll::Pending => return Err(OAuthError::new("authorization_pending", "the person hasn't answered yet")),
        Poll::SlowDown => {
            return Err(OAuthError::new("slow_down", "asked too soon: wait 5 seconds longer between questions"));
        }
        Poll::Denied => return Err(OAuthError::new("access_denied", "the person didn't allow this app")),
        Poll::Expired => return Err(OAuthError::new("expired_token", "the code expired: start again")),
        Poll::Unknown => return Err(OAuthError::invalid_grant("unknown device_code")),
    };
    let issued = issue_for_device(state, headers, ip, &allowed).await;
    // The request is over, whatever came of it.
    let signed_out = state.oauth.devices.issued(device_code);
    let issued = issued?;
    if signed_out {
        if let Err(err) = state.auth.end_grant(issued.grant_id).await {
            tracing::error!("could not end an app's access: {err}");
        }
        return Err(OAuthError::new("access_denied", SIGNED_OUT));
    }
    Ok(tokens(issued))
}

/// The tokens of a TV that was allowed, when its account still may have them.
async fn issue_for_device(
    state: &AppState,
    headers: &HeaderMap,
    ip: Option<String>,
    allowed: &devices::DeviceGrant,
) -> Result<Issued, OAuthError> {
    // Things may have changed since the person allowed it.
    for id in [allowed.user_id, allowed.approved_by] {
        match AuthUser::load(&state.db, id).await {
            Ok(Some(_)) => {}
            Ok(None) => return Err(OAuthError::invalid_grant("the account is disabled")),
            Err(err) => return Err(OAuthError::server_error(err)),
        }
    }
    let issued = state
        .auth
        .issue(NewGrant {
            user_id: allowed.user_id,
            approved_by: allowed.approved_by,
            client_id: allowed.app.id.clone(),
            client_name: allowed.app.name.clone(),
            scopes: allowed.scopes,
            refresh: allowed.app.refresh,
            first_party: allowed.app.first_party,
            session_hash: Some(allowed.session_hash.clone()),
            user_agent: user_agent(headers),
            ip,
        })
        .await;
    match issued {
        Ok(issued) => Ok(issued),
        Err(IssueError::SignedOut) => Err(OAuthError::new("access_denied", SIGNED_OUT)),
        Err(IssueError::Internal(err)) => Err(OAuthError::server_error(err)),
    }
}

// ------------------------------------------------------------------ profiles (pley's own apps)

/// grant_type=urn:pley:params:oauth:grant-type:switch-profile (an
/// extension grant, RFC 6749 §4.5; see docs/API.md): one of pley's own
/// apps, allowed `profiles`, moves to another profile of the household.
///
/// It shows its refresh token, never an access token, so a stolen one-hour
/// token can't become a lasting grant; a spent one ends the grant as a
/// refresh does. The rules are the web app's switch ([`check_switch`]),
/// from the app's own rights: no administrator goes anywhere through an app.
/// Profiles that need their password are picked in the browser, where the
/// app signs in again: apps never see passwords or two-step codes. A
/// refusal spends nothing. A switch keeps the app, what it was allowed and
/// whoever allowed it, and ends the tokens it had at once.
///
/// A switch to the profile the app acts as checks that profile's PIN ("Who's
/// watching?" as a TV starts) and, when it is right, answers as a switch
/// would: a new pair, the same scope. Without a PIN, given or on the
/// profile, it is `invalid_request`, as before this check existed.
async fn switch_profile(
    state: &AppState,
    headers: &HeaderMap,
    ip: Option<String>,
    form: &Form,
) -> Result<Response, OAuthError> {
    let token = form.required("refresh_token")?;
    let client_id = form.required("client_id")?;
    let profile: i64 = form
        .required("profile")?
        .parse()
        .map_err(|_| OAuthError::invalid_request("profile must be the id of a profile"))?;
    form.check_resource(headers)?;
    let holder = grants::holder(&state.db, token, client_id, state.auth.two_step_required())
        .await
        .map_err(|err| refused_refresh(state, client_id, err))?;
    if clients::builtin(client_id).is_none() {
        return Err(OAuthError::new("unauthorized_client", "only Pley's own apps can switch profiles"));
    }
    if !holder.scopes.contains(Scope::Profiles) {
        return Err(OAuthError::new(
            "unauthorized_client",
            format!("this app wasn't allowed to {}: sign in to it again to allow it", Scope::Profiles.words()),
        ));
    }
    if profile == holder.user.id {
        // "Who's watching?" as a TV starts: the profile's own PIN, checked
        // and answered as a switch to it would be. Without a PIN to check,
        // the app has nothing to ask (older servers answered this to every
        // switch to the current profile, and apps take it to mean "can't
        // check").
        let pin = form.get("pin").ok_or_else(|| OAuthError::invalid_request(SAME_PROFILE))?;
        if !check_own_pin(state, &holder.user, pin).await.map_err(refused_switch)? {
            return Err(OAuthError::invalid_request(SAME_PROFILE));
        }
    } else {
        check_switch(state, &holder.user, profile, form.get("pin"), None, None, None).await.map_err(refused_switch)?;
    }
    let input = SwitchInput {
        token: token.to_owned(),
        grant_id: holder.grant_id,
        user_id: profile,
        user_agent: user_agent(headers),
        ip,
    };
    let (issued, dropped) =
        grants::switch_profile(&state.db, input).await.map_err(|err| refused_refresh(state, client_id, err))?;
    // Its tokens, cached as the profile it acted as, are gone.
    state.auth.forget_grants(&[issued.grant_id]);
    if !dropped.is_empty() {
        state.auth.forget_grants(&dropped);
    }
    state.auth.forget_user(holder.user.id);
    state.auth.forget_user(profile);
    if profile == holder.user.id {
        tracing::info!("{} confirmed {}'s PIN ({})", app_label(client_id), holder.user.username, issued.scopes);
    } else {
        tracing::info!(
            "{} switched from {}'s profile to profile {profile} ({})",
            app_label(client_id),
            holder.user.username,
            issued.scopes
        );
    }
    Ok(tokens(issued))
}

/// A switch to the profile the app acts as, without a PIN to check.
const SAME_PROFILE: &str = "the app uses this profile already";

/// A refresh token that got nothing, for a switch; a spent one ended its
/// grant (see [`grants::holder`]).
fn refused_refresh(state: &AppState, client_id: &str, err: RefreshError) -> OAuthError {
    match err {
        RefreshError::Reused { grant_id } => {
            tracing::warn!("{client_id} showed a spent refresh token (someone has a copy): its access is ended");
            state.auth.forget_grants(&[grant_id]);
            OAuthError::invalid_grant(RefreshError::Reused { grant_id }.to_string())
        }
        RefreshError::Ended { grant_id } => {
            tracing::warn!("{client_id} refreshed its tokens too often: its access is ended");
            state.auth.forget_grants(&[grant_id]);
            OAuthError::invalid_grant(RefreshError::Ended { grant_id }.to_string())
        }
        RefreshError::Internal(err) => OAuthError::server_error(err),
        err => OAuthError::invalid_grant(err.to_string()),
    }
}

/// A switch the rules refused: invalid_grant, with a `reason` the app words
/// for the person.
fn refused_switch(err: SwitchError) -> OAuthError {
    let (reason, description) = match err {
        SwitchError::NotFound | SwitchError::NotAllowed => ("not_allowed", NOT_ALLOWED.to_owned()),
        SwitchError::NoWayIn => ("not_allowed", NO_WAY_IN.to_owned()),
        SwitchError::Throttled(err) => ("throttled", err.to_string()),
        SwitchError::PinLocked => ("pin_locked", PIN_LOCKED.to_owned()),
        SwitchError::PinNeeded => ("wrong_pin", PIN_NEEDED.to_owned()),
        SwitchError::WrongPin => ("wrong_pin", "wrong PIN".to_owned()),
        SwitchError::PasswordNeeded | SwitchError::WrongPassword | SwitchError::TwoStep(_) => (
            "browser",
            "this profile can see more than yours: sign in to the app again, and pick the profile in the browser"
                .to_owned(),
        ),
        // It sets up two-step sign-in with its password, in the browser.
        // Apps never see passwords or codes, nor anyone's sign-in settings.
        SwitchError::SetUpFirst => (
            "browser",
            "this profile takes its password: sign in to the app again, and pick the profile in the browser".to_owned(),
        ),
        SwitchError::Internal(err) => return OAuthError::server_error(err),
    };
    OAuthError { reason: Some(reason), ..OAuthError::invalid_grant(description) }
}

/// A profile of the household, for apps: no usernames, roles, libraries,
/// age limits or sign-in settings.
#[derive(Serialize)]
pub struct AppProfile {
    id: i64,
    name: String,
    avatar_url: Option<String>,
    avatar_color: Option<String>,
    /// The profile the app acts as.
    current: bool,
    /// What switching to it takes: none, pin, password (in the browser) or
    /// locked. A profile that has to set up two-step sign-in, which the
    /// server requires, takes its password, which sets it up there.
    needs: &'static str,
}

/// `GET /api/oauth/profiles` (apps allowed `profiles`): the enabled
/// profiles of the household the caller belongs to, with what switching to
/// each takes from the caller's own rights (the web app's rules, without
/// the administrators' exemption for apps). Nobody's for a caller outside
/// the household.
pub async fn profiles(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<AppProfile>>> {
    let all = crate::accounts::users(&state.db).await?;
    if !all.iter().any(|u| u.id == user.id && u.home) {
        return Ok(Json(Vec::new()));
    }
    let targets: HashMap<i64, SwitchTarget> =
        switch_targets(&state, user.id).await?.into_iter().map(|target| (target.id, target)).collect();
    let required = state.auth.two_step_required();
    let listed = all
        .into_iter()
        .filter(|u| u.home && !u.disabled)
        .filter_map(|u| {
            let target = targets.get(&u.id)?;
            Some(AppProfile {
                id: u.id,
                name: u.name,
                avatar_url: u.avatar_url,
                avatar_color: u.avatar_color,
                current: u.id == user.id,
                needs: switch_needs(&user, target, required).name(),
            })
        })
        .collect();
    Ok(Json(listed))
}

/// `POST /api/oauth/revoke` (RFC 7009): an app signs out. A token of that
/// app (access or refresh, spent or expired) ends its whole grant, and the
/// answer is 200 with an empty body, as it is for a token pley doesn't know
/// (RFC 7009 §2.2). Another app's token is refused (400
/// unauthorized_client, §2.1) and stays. When the database fails, 503: the
/// token still works, and the app tries again later (§2.2.1).
pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    let form = match Form::read(&headers, body) {
        Ok(form) => form,
        Err(err) => return err.into_response(),
    };
    let (token, client_id) = match (form.required("token"), form.required("client_id")) {
        (Ok(token), Ok(client_id)) => (token, client_id),
        (Err(err), _) | (_, Err(err)) => return err.into_response(),
    };
    let unavailable = |err: AppError| {
        tracing::error!("an app couldn't be signed out: {err}");
        let mut answer = OAuthError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            ..OAuthError::new("temporarily_unavailable", "Pley couldn't sign the app out: try again in a moment")
        }
        .into_response();
        answer.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        answer
    };
    let grant = match grants::find_grant_of_token(&state.db, token).await {
        Ok(grant) => grant,
        Err(err) => return unavailable(err),
    };
    let Some((grant_id, owner)) = grant else { return (StatusCode::OK, NO_STORE).into_response() };
    if owner != client_id {
        return OAuthError::new("unauthorized_client", "this token belongs to another app").into_response();
    }
    let described = describe(&state, grant_id).await;
    // By the token, which belongs to this grant only.
    match state.auth.end_grant_of_token(&hash_token(token)).await {
        Ok(true) => {
            if let Some((app, user)) = described {
                tracing::info!("{app} was signed out of {user}'s account");
            }
        }
        Ok(false) => {}
        Err(err) => return unavailable(err),
    }
    (StatusCode::OK, NO_STORE).into_response()
}

/// How a grant is called in logs: the app and the profile it acts as.
async fn describe(state: &AppState, grant_id: i64) -> Option<(String, String)> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT g.client_id, u.username FROM app_grants g JOIN users u ON u.id = g.user_id WHERE g.id = ?",
    )
    .bind(grant_id)
    .fetch_optional(&state.db.read)
    .await
    .ok()
    .flatten();
    row.map(|(client_id, user)| (app_label(&client_id), user))
}

/// pley's own apps by name, others by host.
fn app_label(client_id: &str) -> String {
    match clients::builtin(client_id) {
        Some(client) => client.name.clone().unwrap_or_else(|| client.host.clone()),
        None => client_host(client_id),
    }
}

fn client_host(client_id: &str) -> String {
    Url::parse(client_id).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_else(|| client_id.to_owned())
}

// ------------------------------------------------------------------ apps lists

#[derive(Serialize)]
pub struct Person {
    id: i64,
    name: String,
}

/// An app allowed to use an account (`api/apps.ts`'s AppGrant).
#[derive(Serialize)]
pub struct AppGrant {
    id: i64,
    client_id: String,
    /// pley's own apps: their name. Others: what the app called itself
    /// when it was allowed, which pley can't check.
    name: Option<String>,
    first_party: bool,
    host: String,
    scopes: Vec<&'static str>,
    /// The profile it acts as.
    profile: Person,
    approved_by: Person,
    /// Of its last token request.
    device: Option<String>,
    ip: Option<String>,
    created_at: i64,
    last_used_at: i64,
}

impl From<GrantRow> for AppGrant {
    fn from(row: GrantRow) -> Self {
        let builtin = clients::builtin(&row.client_id);
        AppGrant {
            id: row.id,
            name: builtin.map_or(row.client_name, |client| client.name.clone()),
            first_party: builtin.is_some(),
            host: client_host(&row.client_id),
            client_id: row.client_id,
            scopes: row.scopes.names(),
            profile: Person { id: row.user_id, name: row.user_name },
            approved_by: Person { id: row.approved_by, name: row.approved_by_name },
            device: row.user_agent.as_deref().map(grant_device),
            ip: row.last_ip,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        }
    }
}

/// What an app's grant was last used from, by the User-Agent of its token
/// requests. pley's apps say "pleyAndroid/1.0 (Android 15)",
/// "pleyKodi/0.1.0 (Kodi 21.2; Windows)", "pleyRoku/0.1.0 (Roku OS 14.5;
/// Roku Ultra)" or "pleyLG/0.1.0 (webOS 23; OLED65C3PUA)": the row names the
/// app already, so the platform it gives in brackets.
fn grant_device(user_agent: &str) -> String {
    let apps = [("pleyAndroid/", "Android"), ("pleyKodi/", "Kodi"), ("pleyRoku/", "Roku"), ("pleyLG/", "webOS")];
    for (app, platform) in apps {
        if user_agent.contains(app) {
            let given = user_agent
                .split_once('(')
                .and_then(|(_, rest)| rest.split_once(')'))
                .map(|(inside, _)| inside.trim())
                .filter(|inside| !inside.is_empty());
            return given.unwrap_or(platform).chars().take(60).collect();
        }
    }
    // Samsung's TV app, and LG's before it was a native app, send their TV's
    // own agent: the system's version.
    if let Some(at) = user_agent.find("Tizen ") {
        return user_agent[at..].chars().take_while(|c| *c != ')' && *c != ';').take(60).collect();
    }
    if user_agent.contains("Web0S") {
        return webos_version(user_agent).map_or_else(|| "webOS".to_owned(), |v| format!("webOS {v}"));
    }
    crate::accounts::device_name(Some(user_agent))
}

/// The webOS of an LG TV's web engine, by its Chromium version: each webOS
/// ships its own and keeps it through its updates.
fn webos_version(user_agent: &str) -> Option<&'static str> {
    let at = user_agent.find("Chrome/")? + "Chrome/".len();
    let major: u32 = user_agent[at..].split('.').next()?.parse().ok()?;
    Some(match major {
        38 => "3",
        53 => "4",
        68 => "5",
        79 => "6",
        87 => "22",
        94 => "23",
        108 => "24",
        120 => "25",
        132 => "26",
        _ => return None,
    })
}

async fn apps_of(state: &AppState, user_id: i64) -> ApiResult<Vec<AppGrant>> {
    Ok(grants::list(&state.db, user_id).await?.into_iter().map(AppGrant::from).collect())
}

/// Ends grant `grant_id` when it acts as `user_id` or they allowed it.
async fn sign_out_app(state: &AppState, user_id: i64, grant_id: i64) -> ApiResult<StatusCode> {
    let listed = grants::list(&state.db, user_id).await?.into_iter().any(|row| row.id == grant_id);
    if !listed {
        return Err(AppError::not_found("no such app"));
    }
    let described = describe(state, grant_id).await;
    if state.auth.end_grant(grant_id).await?
        && let Some((app, user)) = described
    {
        tracing::info!("{app} was signed out of {user}'s account");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/me/apps`: the apps acting as the caller or allowed by them,
/// most recently used first.
pub async fn my_apps(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<AppGrant>>> {
    Ok(Json(apps_of(&state, user.id).await?))
}

/// `DELETE /api/me/apps/{id}`.
pub async fn sign_out_my_app(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    sign_out_app(&state, user.id, id).await
}

/// `GET /api/users/{id}/apps` (administrators).
pub async fn user_apps(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<AppGrant>>> {
    Ok(Json(apps_of(&state, id).await?))
}

/// `DELETE /api/users/{id}/apps/{grant}` (administrators).
pub async fn sign_out_user_app(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((id, grant)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    sign_out_app(&state, id, grant).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tvs_name_their_system() {
        let tizen = "Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.0) AppleWebKit/537.36 (KHTML, like Gecko) 76.0.3809.146/6.0 TV Safari/537.36";
        assert_eq!(grant_device(tizen), "Tizen 6.0");
        let webos = "Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/94.0.4606.128 Safari/537.36 WebAppManager";
        assert_eq!(grant_device(webos), "webOS 23");
        assert_eq!(grant_device(&webos.replace("Chrome/94", "Chrome/99")), "webOS");
        assert_eq!(grant_device("pleyKodi/0.1.0 (Kodi 21.2; Windows)"), "Kodi 21.2; Windows");
        assert_eq!(grant_device("pleyLG/0.1.0 (webOS 23; OLED65C3PUA)"), "webOS 23; OLED65C3PUA");
        assert_eq!(grant_device("pleyLG/0.1.0"), "webOS");
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, value.parse().unwrap());
        }
        headers
    }

    #[test]
    fn the_issuer_is_the_address_asked_at() {
        let at = |pairs: &[(&'static str, &str)]| issuer(&headers(pairs)).ok();
        assert_eq!(at(&[("host", "Pley.Local:7539")]).as_deref(), Some("http://pley.local:7539"));
        assert_eq!(at(&[("host", "pley.local:80")]).as_deref(), Some("http://pley.local"));
        assert_eq!(
            at(&[("host", "pley.example.com:443"), ("x-forwarded-proto", "https, http")]).as_deref(),
            Some("https://pley.example.com")
        );
        assert_eq!(at(&[("host", "[::1]:7539")]).as_deref(), Some("http://[::1]:7539"));
        assert_eq!(at(&[("host", "192.168.1.5:7539")]).as_deref(), Some("http://192.168.1.5:7539"));
        for bad in ["", "user@pley.local", "pley.local/path", "pley.local?x", "pley.local#x", "pley.local:port", "a b"]
        {
            assert_eq!(at(&[("host", bad)]), None, "{bad}");
        }
        assert_eq!(at(&[]), None, "no Host");
    }

    #[test]
    fn resources_name_this_server() {
        assert!(is_this_server(None, "http://pley.local:7539"));
        assert!(is_this_server(Some("http://pley.local:7539"), "http://pley.local:7539"));
        assert!(is_this_server(Some("http://pley.local:7539/"), "http://pley.local:7539"));
        assert!(!is_this_server(Some("http://pley.local:7539/api"), "http://pley.local:7539"));
        assert!(!is_this_server(Some("https://pley.local:7539"), "http://pley.local:7539"));
    }

    #[test]
    fn parameters_pley_reads_come_once() {
        assert_eq!(parameters(b"a=1&b=%20x+y").unwrap(), [("a".into(), "1".into()), ("b".into(), " x y".into())]);
        assert_eq!(parameters(b"code=1&b=2&code=1"), Err("code".to_owned()));
        assert_eq!(parameters(b"state=a&state=b"), Err("state".to_owned()));
        // Empty ones are as if left out, and others may come twice.
        assert_eq!(parameters(b"state=&state=x&client_secret=").unwrap(), [("state".into(), "x".into())]);
        assert_eq!(parameters(b"prompt=a&prompt=b").unwrap().len(), 2);
        assert_eq!(shown_name("client_id"), Some("client_id"));
        assert_eq!(shown_name("<b>"), None);
    }
}
