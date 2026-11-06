//! Quick Connect, as Jellyfin's apps use it ([`crate::oauth::quick_connect`]
//! keeps the requests): an app asks for a code (`/QuickConnect/Initiate`)
//! and shows it, asks every few seconds whether it was allowed
//! (`/QuickConnect/Connect?secret=`), and once it was, trades its secret for
//! a token for other apps (`/Users/AuthenticateWithQuickConnect`) that may
//! play, named after the app as a password sign-in's is. The person allows
//! it on Pley's `/link` page, where they are signed in (two-step sign-in
//! included), or in a Jellyfin app signed in already
//! (`/QuickConnect/Authorize?code=`).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;

use super::ids::Kind;
use super::users::{iso, signed_in};
use super::{Body, ClientOf, Jellyfin, JfError, JfResult, JfUser, Q};
use crate::auth::{AuthUser, ClientIp, SourceIp, UsedFrom, compatibility};
use crate::oauth::quick_connect::{self, Allowed, Approver, Asker, Refused, Take};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/quickconnect/enabled", get(enabled))
        // POST since Jellyfin 10.9; some apps still try GET first.
        .route("/quickconnect/initiate", get(initiate).post(initiate))
        .route("/quickconnect/connect", get(connect))
        .route("/quickconnect/authorize", post(authorize))
        .route("/users/authenticatewithquickconnect", post(authenticate))
}

/// Jellyfin's `QuickConnectResult`. Every field is there, strings empty
/// when unknown: apps built on its Kotlin SDK take none as optional.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct QuickConnectResult {
    authenticated: bool,
    secret: String,
    code: String,
    device_id: String,
    device_name: String,
    app_name: String,
    app_version: String,
    date_added: String,
}

fn result(secret: &str, code: &str, asker: &Asker, added_at: i64, authenticated: bool) -> Json<QuickConnectResult> {
    let said = |value: &Option<String>| value.clone().unwrap_or_default();
    Json(QuickConnectResult {
        authenticated,
        secret: secret.to_owned(),
        code: code.to_owned(),
        device_id: said(&asker.device_id),
        device_name: said(&asker.device),
        app_name: said(&asker.app),
        app_version: said(&asker.version),
        date_added: iso(added_at),
    })
}

/// On while the API is: codes are allowed where the person is signed in.
async fn enabled() -> Json<bool> {
    Json(true)
}

/// A new request, of the app its `Authorization` header describes, from
/// `source`, which the limits per address count by.
async fn initiate(
    State(state): State<AppState>,
    SourceIp(source): SourceIp,
    ClientOf(client): ClientOf,
) -> JfResult<Response> {
    let asker = Asker {
        app: client.client,
        version: client.version,
        device: client.device,
        device_id: client.device_id,
        address: source,
    };
    let started = state.oauth.quick_connect.start(asker.clone(), crate::util::now()).map_err(|_| {
        JfError::TooManyRequests("too many Quick Connect sign-ins in progress: try again in a few minutes".into())
    })?;
    Ok(result(&started.secret, &started.code, &asker, started.added_at, false).into_response())
}

/// The request of `secret`: whether it was allowed yet. `404` once it is
/// over: expired, refused, or its token made.
async fn connect(State(state): State<AppState>, q: Q) -> JfResult<Response> {
    let secret = q.get("secret").ok_or(JfError::NotFound)?;
    let seen = state.oauth.quick_connect.seen(secret).ok_or(JfError::NotFound)?;
    Ok(result(secret, &seen.code, &seen.asker, seen.added_at, seen.authenticated).into_response())
}

/// `POST /QuickConnect/Authorize?code=`: a Jellyfin app signed in already
/// allows the app showing `code` to sign in to its account, as Jellyfin's
/// do. The new token may play, from where the caller's may be used. A
/// `userId` must be the caller's own: nothing here acts for another
/// account. `true`, as Jellyfin answers, or `404` for a code no app shows.
async fn authorize(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Json<bool>> {
    if q.get("userId").is_some_and(|id| jellyfin.ids.number(Kind::User, id) != Some(user.id)) {
        return Err(JfError::Forbidden);
    }
    let token = user.compatibility.ok_or(JfError::Unauthorized)?;
    let code = q.get("code").and_then(quick_connect::normalize).ok_or(JfError::NotFound)?;
    let origins = compatibility::origins_of(&state.db, user.id, token.token_id).await?.ok_or(JfError::Unauthorized)?;
    let asker =
        state.oauth.quick_connect.authorize(&code, user.id, token.token_id, origins).map_err(
            |refused| match refused {
                Refused::Throttled => JfError::TooManyRequests("too many wrong codes: wait a few minutes".into()),
                Refused::Unknown | Refused::Used => JfError::NotFound,
            },
        )?;
    tracing::info!("{} may sign in to {}'s account with Quick Connect", asker.label(), user.username);
    Ok(Json(true))
}

/// `POST /Users/AuthenticateWithQuickConnect {"Secret"}`: the app's token,
/// once it was allowed, answered as `/Users/AuthenticateByName` answers.
/// `401` while no one has allowed it yet, `404` for a secret no request in
/// progress has.
async fn authenticate(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    SourceIp(source): SourceIp,
    ClientIp(ip): ClientIp,
    ClientOf(client): ClientOf,
    headers: HeaderMap,
    body: Body,
) -> JfResult<Response> {
    let secret = body.str("Secret").ok_or(JfError::NotFound)?.to_owned();
    let allowed = match state.oauth.quick_connect.take(&secret) {
        Take::Unknown => return Err(JfError::NotFound),
        Take::Pending => return Err(JfError::Unauthorized),
        Take::Allowed(allowed) => allowed,
    };
    let made = issue(&state, &allowed, &client, source).await;
    // The request is over, whatever came of it.
    let signed_out = state.oauth.quick_connect.issued(&secret);
    let (token, account) = made?;
    if signed_out {
        state.auth.end_compatibility_token_of(&token).await?;
        return Err(JfError::Unauthorized);
    }
    tracing::info!(
        "'{}' signed in to the Jellyfin-compatible API with Quick Connect from {}",
        account.username,
        client.token_name()
    );
    // Its first use, from where it signed in: the account page shows it.
    let user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let used = UsedFrom { ip, user_agent };
    let user = state.auth.validate_compatibility(&token, Some(&used)).await.ok_or(JfError::Unauthorized)?;
    Ok(Json(signed_in(&state, &jellyfin, &super::this_apis(user), &client, token).await?).into_response())
}

/// The token of an app that was allowed, when it still may have one: its
/// account enabled, whoever allowed it still signed in where they did, and
/// the app where the token may be used from.
async fn issue(
    state: &AppState,
    allowed: &Allowed,
    client: &super::Client,
    from: Option<std::net::IpAddr>,
) -> JfResult<(String, AuthUser)> {
    let account = AuthUser::load(&state.db, allowed.user_id).await?.ok_or(JfError::Unauthorized)?;
    let still = match &allowed.approver {
        Approver::Session(session_hash) => crate::api::oauth::signed_in(state, session_hash).await?,
        Approver::Token(id) => compatibility::origins_of(&state.db, allowed.user_id, *id).await?.is_some(),
    };
    if !still || !allowed.origins.allow(from) {
        return Err(JfError::Unauthorized);
    }
    let asked = compatibility::Asked {
        name: client.token_name(),
        scopes: compatibility::Scopes::play(),
        origins: allowed.origins.clone(),
    };
    let (token, _) = state.auth.create_compatibility_token(account.id, &asked).await?;
    Ok((token, account))
}
