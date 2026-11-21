//! Signing in with Tailscale (see [`crate::tailscale`]): who Tailscale says
//! a request comes from, signing in as the account they linked, and linking
//! it on the account page.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::auth::{confirm_password, login_response_as};
use crate::auth::{AuthUser, ClientIp, CurrentToken, TailnetPeer, hash_token};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;
use crate::tailscale::Person;
use crate::util::now;

/// What a caller other than an administrator is told when Tailscale can't
/// be asked: the details (where its service is) are for administrators.
const CANT_ASK: &str = "Pley can't ask Tailscale who this is right now";

/// The person Tailscale says this request comes from, when signing in with
/// Tailscale is on, the request names this server by its Tailscale name or
/// address, and the connection came over the tailnet from a person's
/// device. Any other name may be a website's that a browser was led to
/// resolve to this server (DNS rebinding): that site's page would be signed
/// in as whoever opened it. The name is checked first, so requests a proxy
/// brings from elsewhere cost Tailscale nothing. `Err` when Tailscale can't
/// be asked.
async fn person_here(
    state: &AppState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
) -> Result<Option<Person>, String> {
    if !state.auth.tailscale_sign_in() {
        return Ok(None);
    }
    let Some(peer) = peer else { return Ok(None) };
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !state.auth.tailscale.node().await?.named_by(host) {
        return Ok(None);
    }
    state.auth.tailscale.person(peer).await
}

/// [`person_here`], with what Tailscale said when it couldn't be asked kept
/// for administrators.
async fn person_here_for(
    state: &AppState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    admin: bool,
) -> ApiResult<Option<Person>> {
    person_here(state, peer, headers).await.map_err(|err| {
        tracing::debug!("signing in with Tailscale: {err}");
        AppError::Unavailable(if admin {
            format!("Pley can't ask Tailscale who this is: {err}")
        } else {
            CANT_ASK.into()
        })
    })
}

/// A request from Pley's own page, or from no page at all. A sign-in with a
/// password needs no such check, as another site's page doesn't know the
/// password; a sign-in with Tailscale takes nothing such a page lacks.
pub struct FromPleysPage;

impl<S: Send + Sync> FromRequestParts<S> for FromPleysPage {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if crate::auth::from_another_site(parts) {
            return Err(AppError::forbidden("this request didn't come from Pley's page"));
        }
        Ok(Self)
    }
}

#[derive(Serialize)]
pub struct Here {
    /// The person Tailscale says this request comes from; `null` for no one
    /// (signing in with Tailscale off, another network, another address).
    tailscale: Option<HerePerson>,
}

#[derive(Serialize)]
struct HerePerson {
    login: String,
    name: String,
    /// The account they linked, which they sign in as.
    username: Option<String>,
    /// On the account page: they linked another account, not this one (a
    /// profile switched to on their device), which can't link them too.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    elsewhere: bool,
}

/// The account linked to `person`, by its name.
async fn linked_username(state: &AppState, person: &Person, enabled_only: bool) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT u.username FROM tailscale_links l JOIN users u ON u.id = l.user_id
         WHERE l.tailscale_user = ? AND (NOT u.disabled OR NOT ?)",
    )
    .bind(&person.id)
    .bind(enabled_only)
    .fetch_optional(&state.db.read)
    .await?)
}

/// `GET /api/auth/tailscale`: who Tailscale says this is, for the sign-in
/// page.
pub async fn here(
    State(state): State<AppState>,
    TailnetPeer(peer): TailnetPeer,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let person = person_here(&state, peer, &headers).await.unwrap_or_else(|err| {
        tracing::debug!("signing in with Tailscale: {err}");
        None
    });
    let tailscale = match person {
        None => None,
        Some(person) => {
            let username = linked_username(&state, &person, true).await?;
            Some(HerePerson { login: person.login, name: person.name, username, elsewhere: false })
        }
    };
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(Here { tailscale })).into_response())
}

#[derive(Deserialize)]
pub struct SignIn {
    /// A code from the authenticator app, or a recovery code, for an
    /// account with two-step sign-in.
    #[serde(default)]
    code: Option<String>,
}

/// `POST /api/auth/tailscale {code?}`: signs in as the account the person
/// Tailscale says this is linked, for a session that works only over the
/// tailnet from them. Two-step sign-in still takes its code, and an
/// administrator's account signs in so only with it on.
pub async fn sign_in(
    State(state): State<AppState>,
    _page: FromPleysPage,
    TailnetPeer(peer): TailnetPeer,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<SignIn>,
) -> ApiResult<Response> {
    let person = person_here_for(&state, peer, &headers, false).await?.ok_or_else(|| {
        AppError::forbidden(
            "Tailscale doesn't say who this is: open Pley at its Tailscale name, on a device signed in to Tailscale",
        )
    })?;
    let row: Option<(i64, String, bool, bool)> = sqlx::query_as(
        "SELECT u.id, u.username, u.is_admin, u.disabled FROM tailscale_links l JOIN users u ON u.id = l.user_id
         WHERE l.tailscale_user = ?",
    )
    .bind(&person.id)
    .fetch_optional(&state.db.read)
    .await?;
    let Some((id, username, is_admin, disabled)) = row else {
        return Err(AppError::forbidden(
            "no account here is linked to this Tailscale user: sign in with your password, then link it on your \
             account page",
        ));
    };
    if disabled {
        return Err(AppError::forbidden("this account is disabled"));
    }
    if state.two_step.is_on(id).await? {
        state.two_step.check(&state.auth.throttle, id, body.code.as_deref()).await?;
    } else if state.auth.two_step_required() {
        return Err(AppError::forbidden(
            "this server requires two-step sign-in: sign in with your password to set it up",
        ));
    } else if is_admin {
        return Err(AppError::forbidden(
            "administrators sign in with Tailscale only with two-step sign-in on: sign in with your password",
        ));
    }
    sqlx::query("UPDATE tailscale_links SET login_name = ? WHERE user_id = ?")
        .bind(&person.login)
        .bind(id)
        .execute(&state.db.write)
        .await?;
    tracing::info!("'{username}' signed in with Tailscale");
    login_response_as(&state, id, &headers, ip.as_deref(), None, Some(&person.id)).await
}

#[derive(Serialize)]
pub struct Mine {
    /// Signing in with Tailscale is on.
    on: bool,
    /// The Tailscale user this account is linked to.
    linked: Option<Linked>,
    /// The person Tailscale says this request comes from, who can be linked.
    here: Option<HerePerson>,
    /// For administrators: this server's Tailscale name, while Tailscale
    /// answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    server_name: Option<String>,
    /// For administrators: why Tailscale can't be asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

#[derive(Serialize)]
struct Linked {
    login: String,
    linked_at: i64,
}

async fn mine_of(state: &AppState, user: &AuthUser, peer: Option<SocketAddr>, headers: &HeaderMap) -> ApiResult<Mine> {
    let on = state.auth.tailscale_sign_in();
    let linked: Option<(String, i64)> =
        sqlx::query_as("SELECT login_name, created_at FROM tailscale_links WHERE user_id = ?")
            .bind(user.id)
            .fetch_optional(&state.db.read)
            .await?;
    let (mut server_name, mut problem) = (None, None);
    if on && user.account_admin {
        match state.auth.tailscale.node().await {
            Ok(node) => server_name = Some(node.name).filter(|name| !name.is_empty()),
            Err(err) => problem = Some(err),
        }
    }
    let here = match person_here(state, peer, headers).await {
        Ok(person) => person,
        Err(err) => {
            problem.get_or_insert(err);
            None
        }
    };
    let here = match here {
        None => None,
        Some(person) => {
            // Another account's name is that account's to know.
            let linked = linked_username(state, &person, false).await?;
            let elsewhere = linked.as_ref().is_some_and(|name| *name != user.username);
            let username = linked.filter(|name| *name == user.username);
            Some(HerePerson { login: person.login, name: person.name, username, elsewhere })
        }
    };
    if !user.account_admin {
        problem = None;
    }
    Ok(Mine { on, linked: linked.map(|(login, linked_at)| Linked { login, linked_at }), here, server_name, problem })
}

/// `GET /api/me/tailscale`: the Tailscale user this account is linked to,
/// and the one this device could link.
pub async fn mine(
    State(state): State<AppState>,
    user: AuthUser,
    TailnetPeer(peer): TailnetPeer,
    headers: HeaderMap,
) -> ApiResult<Json<Mine>> {
    Ok(Json(mine_of(&state, &user, peer, &headers).await?))
}

#[derive(Deserialize)]
pub struct Link {
    /// The account's password: a session alone can't add a way in.
    #[serde(default)]
    password: Option<String>,
    /// With two-step sign-in, a code from the authenticator app or a
    /// recovery code.
    #[serde(default)]
    code: Option<String>,
}

/// `PUT /api/me/tailscale {password, code?}`: links the person Tailscale
/// says this request comes from to the account, in place of any other.
pub async fn link(
    State(state): State<AppState>,
    user: AuthUser,
    CurrentToken(token): CurrentToken,
    TailnetPeer(peer): TailnetPeer,
    headers: HeaderMap,
    Json(body): Json<Link>,
) -> ApiResult<Json<Mine>> {
    if !state.auth.tailscale_sign_in() {
        return Err(AppError::forbidden("signing in with Tailscale is off on this server"));
    }
    let person = person_here_for(&state, peer, &headers, user.account_admin).await?.ok_or_else(|| {
        AppError::bad_request("open Pley at its Tailscale name, on a device signed in to Tailscale, to link it")
    })?;
    confirm_password(&state, &user, body.password).await?;
    if state.two_step.is_on(user.id).await? {
        state.two_step.check(&state.auth.throttle, user.id, body.code.as_deref()).await?;
    } else if user.account_admin {
        return Err(AppError::forbidden(
            "turn on two-step sign-in first: administrators sign in with Tailscale only with it",
        ));
    }
    let mut tx = state.db.write.begin().await?;
    // The password was right when it was checked: a new one since signed
    // this session out, and so links nothing.
    let session = token.as_deref().map(hash_token).unwrap_or_default();
    let signed_in: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sessions WHERE token_hash = ?)")
        .bind(&session)
        .fetch_one(&mut *tx)
        .await?;
    if !signed_in {
        return Err(AppError::Unauthorized);
    }
    let elsewhere: Option<i64> =
        sqlx::query_scalar("SELECT user_id FROM tailscale_links WHERE tailscale_user = ? AND user_id != ?")
            .bind(&person.id)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await?;
    if elsewhere.is_some() {
        return Err(AppError::conflict("this Tailscale user is linked to another account: unlink it there first"));
    }
    let before: Option<String> = sqlx::query_scalar("SELECT tailscale_user FROM tailscale_links WHERE user_id = ?")
        .bind(user.id)
        .fetch_optional(&mut *tx)
        .await?;
    if before.as_deref() == Some(person.id.as_str()) {
        sqlx::query("UPDATE tailscale_links SET login_name = ? WHERE user_id = ?")
            .bind(&person.login)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
    } else {
        // Sessions signed in as the Tailscale user linked before end with it.
        crate::tailscale::remove_link_within(&mut tx, user.id, None).await?;
        sqlx::query(
            "INSERT INTO tailscale_links (user_id, tailscale_user, login_name, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(user.id)
        .bind(&person.id)
        .bind(&person.login)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    state.auth.forget_sessions();
    tracing::info!("'{}' linked a Tailscale user to their account", user.username);
    Ok(Json(mine_of(&state, &user, peer, &headers).await?))
}

/// `DELETE /api/me/tailscale`: unlinks the Tailscale user, and signs out the
/// sessions signed in with it and the profiles switched to from them, this
/// one too when it is one.
pub async fn unlink(State(state): State<AppState>, user: AuthUser) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    crate::tailscale::remove_link_within(&mut tx, user.id, None).await?;
    tx.commit().await?;
    state.auth.forget_sessions();
    tracing::info!("'{}' unlinked their Tailscale user", user.username);
    Ok(StatusCode::NO_CONTENT)
}
