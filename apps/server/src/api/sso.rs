//! Single sign-on (see [`crate::sso`]): the pages browsers pass through on
//! their way to the provider and back, pley's client metadata document, and
//! linking an account to an identity at the provider.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::auth::confirm_password;
use crate::auth::{
    AdminUser, AuthUser, ClientIp, CurrentToken, forwarded_https, hash_token, session_cookie, token_from_parts,
};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::settings::Settings;
use crate::sso::{self, FLOW_COOKIE, FLOW_TTL, LINK_COOKIE, LINK_TTL, Sso};
use crate::state::AppState;

/// Cookies of the sign-in pages carry `Secure` at an https address.
fn secure(settings: &Settings, headers: &HeaderMap) -> bool {
    settings.public_url.starts_with("https://") || forwarded_https(headers)
}

/// `303 See Other` to `location`, setting `cookies`.
fn see_other(location: &str, cookies: &[String]) -> Response {
    let location = HeaderValue::try_from(location).unwrap_or(HeaderValue::from_static("/"));
    let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response();
    for cookie in cookies {
        if let Ok(value) = HeaderValue::try_from(cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

/// Text from the provider, fit for one line of the log.
fn loggable(text: Option<&str>) -> String {
    sso::printable(text.unwrap_or_default())
}

/// The user a request is signed in as (the session cookie comes along on
/// these pages).
async fn signed_in(state: &AppState, parts: &Parts) -> Option<i64> {
    let token = token_from_parts(parts)?;
    state.auth.validate(&token).await.map(|user| user.id)
}

#[derive(Deserialize)]
pub struct StartQuery {
    next: Option<String>,
    /// Sent to the public address once already (a proxy may rewrite `Host`).
    hop: Option<String>,
}

/// `GET /api/auth/sso/start?next=/page`: off to the provider. Browsers are
/// sent here; problems come back to the web app as `?sso=<code>`.
pub async fn start(State(state): State<AppState>, parts: Parts, Query(query): Query<StartQuery>) -> Response {
    let settings = state.settings.get();
    let next = sso::local_path(query.next.as_deref());
    let linking = sso::read_cookie(&parts.headers, LINK_COOKIE).and_then(|c| state.sso.linking(&c));
    let back = if linking.is_some() { "/account" } else { "/login" };
    if !settings.sso_enabled {
        return see_other(&format!("{back}?sso=off"), &[]);
    }
    // Sign-ins run at the public address, where the provider sends people
    // back, so that the cookie set here comes along there.
    if query.hop.is_none() && !sso::at_public_address(&parts.headers, &settings.public_url) {
        let mut url = match url::Url::parse(&Sso::start_url(&settings)) {
            Ok(url) => url,
            Err(_) => return see_other(&format!("{back}?sso=off"), &[]),
        };
        url.query_pairs_mut().append_pair("next", &next).append_pair("hop", "1");
        return see_other(url.as_str(), &[]);
    }
    // Linking: the user who asked is still the one signed in here.
    let link = match linking {
        Some(user_id) if signed_in(&state, &parts).await == Some(user_id) => Some(user_id),
        Some(_) => return see_other("/account?sso=signin", &[]),
        None => None,
    };
    let secure = secure(&settings, &parts.headers);
    match state.sso.begin(&settings, next, link).await {
        Ok((flow, url)) => see_other(
            url.as_str(),
            &[sso::cookie(FLOW_COOKIE, &flow, FLOW_TTL, secure), sso::cookie(LINK_COOKIE, "", 0, secure)],
        ),
        Err(err) => {
            tracing::warn!("single sign-on: {err}");
            see_other(&format!("{back}?sso={}", err.code()), &[])
        }
    }
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `GET /api/auth/sso/callback`: where the provider sends people back, to
/// be signed in (or to have their identity linked).
pub async fn callback(
    State(state): State<AppState>,
    parts: Parts,
    ClientIp(ip): ClientIp,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let settings = state.settings.get();
    let secure = secure(&settings, &parts.headers);
    let clear = sso::cookie(FLOW_COOKIE, "", 0, secure);
    let cookie = sso::read_cookie(&parts.headers, FLOW_COOKIE);
    let flow = match state.sso.flow(cookie.as_deref(), query.state.as_deref().unwrap_or_default()) {
        Ok(flow) => flow,
        Err(err) => {
            tracing::info!("single sign-on: {err}");
            return see_other("/login?sso=expired", &[clear]);
        }
    };
    let back = if flow.link.is_some() { "/account" } else { "/login" };
    let fail = |code: &str| see_other(&format!("{back}?sso={code}"), std::slice::from_ref(&clear));
    if let Some(error) = query.error.as_deref() {
        let why = loggable(query.error_description.as_deref());
        tracing::info!("single sign-on: the provider answered {}: {why}", loggable(Some(error)));
        // access_denied: the person (or the provider's rules) said no.
        return fail(if error == "access_denied" { "denied" } else { "provider" });
    }
    let Some(code) = query.code.as_deref().filter(|c| !c.is_empty()) else { return fail("provider") };
    // A sign-in's cookie works until it expires, so codes the provider
    // refuses are counted by address: anyone could otherwise have pley ask
    // the provider's token endpoint about made-up codes without end.
    let attempt = match state.auth.throttle.begin(&format!("sso:{}", ip.as_deref().unwrap_or_default())) {
        Ok(attempt) => attempt,
        Err(_) => return fail("busy"),
    };
    let identity = match state.sso.finish(&settings, &flow, code).await {
        Ok(identity) => {
            attempt.succeeded();
            identity
        }
        Err(err) => {
            if matches!(err, sso::SsoError::Provider(_)) {
                attempt.failed();
            }
            tracing::warn!("single sign-on refused: {err}");
            return fail(err.code());
        }
    };

    if let Some(user_id) = flow.link {
        if signed_in(&state, &parts).await != Some(user_id) {
            return fail("signin");
        }
        return match sso::link(&state.db, &identity, user_id).await {
            Ok(true) => {
                tracing::info!("single sign-on: user {user_id} linked their identity at {}", identity.issuer);
                see_other("/account?sso=linked", &[clear])
            }
            Ok(false) => fail("taken"),
            Err(err) => {
                tracing::error!("single sign-on: linking failed: {err}");
                fail("error")
            }
        };
    }

    let user_id = match sso::linked_user(&state.db, &identity).await {
        Ok(Some((_, true))) => return fail("disabled"),
        Ok(Some((id, false))) => id,
        Ok(None) if settings.sso_new_accounts => match sso::create_user(&state.db, &identity).await {
            Ok(id) => {
                tracing::info!("single sign-on: made account {id} for someone new from {}", identity.issuer);
                id
            }
            Err(err) => {
                tracing::error!("single sign-on: making an account failed: {err}");
                return fail("error");
            }
        },
        Ok(None) => return fail("unknown"),
        Err(err) => {
            tracing::error!("single sign-on: {err}");
            return fail("error");
        }
    };
    // An account with a password that hasn't set up two-step sign-in, which
    // the server requires, gets in nowhere until it does (with its password,
    // which sets it up): its session would be refused at once.
    match state.auth.lacks_two_step(user_id).await {
        Ok(false) => {}
        Ok(true) => return fail("two_step"),
        Err(err) => {
            tracing::error!("single sign-on: {err}");
            return fail("error");
        }
    }
    // With a claim that makes administrators, the role follows it at each
    // sign-in through the provider, as an administrator's change in
    // Settings → Users would.
    if let Some(admin) = sso::admin(&settings, &identity) {
        match sso::set_admin(&state.db, user_id, admin).await {
            Ok(sso::RoleChange::Unchanged) => {}
            Ok(sso::RoleChange::KeptLastAdmin) => tracing::warn!(
                "single sign-on: user {user_id} signed in without {}, but stays an administrator: it is the last one",
                settings.sso_admin_claim
            ),
            Ok(change) => {
                let what = if change == sso::RoleChange::MadeAdmin { "now" } else { "no longer" };
                tracing::info!("single sign-on: user {user_id} is {what} an administrator, as the provider says");
                state.auth.forget_user(user_id);
                state.events.send(ServerEvent::OwnLibrariesChanged { user_id });
                state.events.send(ServerEvent::RequestsSettingsChanged { user_id: Some(user_id) });
            }
            Err(err) => {
                tracing::error!("single sign-on: {err}");
                return fail("error");
            }
        }
    }
    if let Err(err) = sso::used(&state.db, &identity).await {
        tracing::warn!("single sign-on: {err}");
    }
    let agent = parts.headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok());
    let token = match state.auth.create_session(user_id, agent, ip.as_deref()).await {
        Ok(token) => token,
        Err(err) => {
            tracing::error!("single sign-on: {err}");
            return fail("error");
        }
    };
    let next = match household(&state, user_id).await {
        Ok(true) => {
            format!("/profiles?next={}", url::form_urlencoded::byte_serialize(flow.next.as_bytes()).collect::<String>())
        }
        _ => flow.next.clone(),
    };
    see_other(&next, &[session_cookie(&token, secure), clear])
}

/// Whether `user_id` picks a profile of a household first (as after a
/// password sign-in).
async fn household(state: &AppState, user_id: i64) -> sqlx::Result<bool> {
    let profiles: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE home = 1 AND disabled = 0
         AND EXISTS (SELECT 1 FROM users WHERE id = ? AND home = 1)",
    )
    .bind(user_id)
    .fetch_one(&state.db.read)
    .await?;
    Ok(profiles > 1)
}

/// `GET /api/auth/sso/client`: pley's client metadata document, for
/// providers that take its address as pley's client ID (CIMD).
pub async fn client(State(state): State<AppState>) -> Response {
    let settings = state.settings.get();
    if !settings.sso_enabled || !settings.sso_client_id.is_empty() {
        return AppError::not_found("Pley has no client metadata document: it has a client ID").into_response();
    }
    ([(header::CACHE_CONTROL, "public, max-age=300")], Json(Sso::client_metadata(&settings))).into_response()
}

#[derive(Deserialize)]
pub struct Confirm {
    /// The account's password: a session alone can't change how it signs in.
    #[serde(default)]
    password: Option<String>,
    /// Its second step, for linking an account with two-step sign-in.
    #[serde(default)]
    code: Option<String>,
}

/// Whether the browser is at pley's public address (`Origin`, which a
/// proxy doesn't change, or else `Host`).
fn at_public_origin(headers: &HeaderMap, public_url: &str) -> bool {
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(origin) => origin.eq_ignore_ascii_case(public_url),
        None => sso::at_public_address(headers, public_url),
    }
}

/// `POST /api/auth/sso/link {password}`: the signed-in user's next sign-in at
/// the provider (`/api/auth/sso/start`, which the web app opens next) links
/// that identity to their account.
pub async fn request_link(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Json(body): Json<Confirm>,
) -> ApiResult<Response> {
    let settings = state.settings.get();
    if !settings.sso_enabled {
        return Err(AppError::bad_request("single sign-on is off"));
    }
    if !at_public_origin(&headers, &settings.public_url) {
        return Err(AppError::conflict(format!("link your account at {}", settings.public_url)));
    }
    confirm_password(&state, &user, body.password).await?;
    // Signing in through the provider skips the second step: linking takes it.
    state.two_step.check(&state.auth.throttle, user.id, body.code.as_deref()).await?;
    let cookie = sso::cookie(LINK_COOKIE, &state.sso.link_request(user.id), LINK_TTL, secure(&settings, &headers));
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response())
}

/// `GET /api/me/sso`: whether single sign-on is on, and the identity linked
/// to the signed-in user.
pub async fn mine(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    let settings = state.settings.get();
    if !settings.sso_enabled {
        return Ok(Json(serde_json::json!({ "enabled": false })));
    }
    let linked = sso::linked_identity(&state.db, user.id, &settings.sso_issuer).await?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&state.db.read)
        .await?;
    Ok(Json(serde_json::json!({
        "enabled": true,
        "name": provider_name(&settings),
        "linked": linked.is_some(),
        "email": linked.flatten(),
        "has_password": !hash.starts_with('!'),
        "public_url": settings.public_url,
        "start": Sso::start_url(&settings),
    })))
}

/// `DELETE /api/me/sso {password}`: the signed-in user's account no longer
/// signs in with the provider, and is signed out everywhere else (whoever
/// got in through the provider), as after a new password.
pub async fn unlink(
    State(state): State<AppState>,
    user: AuthUser,
    CurrentToken(token): CurrentToken,
    Json(body): Json<Confirm>,
) -> ApiResult<StatusCode> {
    let settings = state.settings.get();
    confirm_password(&state, &user, body.password).await?;
    if sso::unlink(&state.db, user.id, &settings.sso_issuer).await? {
        let keep = token.as_deref().map(hash_token).unwrap_or_default();
        state.oauth.forget_user(user.id);
        state.auth.sign_out_everywhere(user.id, Some(keep.as_str())).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/users/{id}/sso`: an administrator unlinks a user's
/// identities (a lost or taken-over account at the provider), which signs
/// the user out everywhere: whoever got in through it is out too.
pub async fn unlink_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?;
    let Some(hash) = hash else { return Err(AppError::not_found("no such user")) };
    // Without its identity, an account without a password has no way in.
    if hash.starts_with('!') {
        return Err(AppError::conflict("this account has no password: give it one before unlinking it"));
    }
    let done = sqlx::query("DELETE FROM user_identities WHERE user_id = ?").bind(id).execute(&state.db.write).await?;
    if done.rows_affected() == 0 {
        return Err(AppError::not_found("this user has no linked identity"));
    }
    state.oauth.forget_user(id);
    state.auth.sign_out_everywhere(id, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The provider's name on buttons: the one in the settings, or its host.
pub fn provider_name(settings: &Settings) -> String {
    if !settings.sso_name.is_empty() {
        return settings.sso_name.clone();
    }
    url::Url::parse(&settings.sso_issuer)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "single sign-on".into())
}
