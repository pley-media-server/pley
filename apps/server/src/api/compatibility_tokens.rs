//! `/api/me/compatibility-tokens`: the account page's tokens for other apps
//! and tools that use the compatibility API (see
//! [`crate::auth::compatibility`]). A session's routes only: apps' tokens
//! get none of them, and compatibility tokens open no route of this API.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::auth::compatibility::{self, TokenRow};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// A token, as the account page lists it (never the token itself).
#[derive(Serialize)]
pub struct CompatibilityToken {
    id: i64,
    name: String,
    /// What it may do: `play`, and `activity` or `manage`.
    scopes: Vec<&'static str>,
    created_at: i64,
    /// `None` until it is first used.
    last_used_at: Option<i64>,
    /// When it ends unless it is used before.
    expires_at: i64,
    /// Of its last use, from the User-Agent: "Firefox on Windows", or the
    /// agent itself ("Tautulli/2.15").
    device: Option<String>,
    ip: Option<String>,
}

impl From<TokenRow> for CompatibilityToken {
    fn from(row: TokenRow) -> Self {
        CompatibilityToken {
            id: row.id,
            scopes: row.scopes().names(),
            name: row.name,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
            expires_at: row.expires_at,
            device: row.user_agent.as_deref().map(|agent| crate::accounts::device_name(Some(agent))),
            ip: row.last_ip,
        }
    }
}

/// A new token: the token itself, shown this once, and how it is listed.
#[derive(Serialize)]
pub struct Created {
    token: String,
    #[serde(flatten)]
    listed: CompatibilityToken,
}

#[derive(Deserialize)]
pub struct NewToken {
    name: String,
    /// Space-separated; `play` without it.
    #[serde(default)]
    scope: Option<String>,
}

/// `GET /api/me/compatibility-tokens`: the caller's tokens, most recently
/// used first.
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<CompatibilityToken>>> {
    let rows = compatibility::list(&state.db, user.id).await?;
    Ok(Json(rows.into_iter().map(CompatibilityToken::from).collect()))
}

/// `POST /api/me/compatibility-tokens`: a new token. `activity` and
/// `manage` only for an administrator.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<NewToken>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let asked = compatibility::asked(&body.name, body.scope.as_deref())?;
    compatibility::may_have(asked.scopes, user.is_admin)?;
    let (token, row) = state.auth.create_compatibility_token(user.id, &asked).await?;
    tracing::info!("'{}' made a token for other apps named '{}'", user.username, asked.name);
    Ok((StatusCode::CREATED, Json(Created { token, listed: row.into() })))
}

/// `DELETE /api/me/compatibility-tokens/{id}`: revokes one of the caller's
/// tokens.
pub async fn revoke(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if !state.auth.end_compatibility_token(user.id, id).await? {
        return Err(AppError::not_found("no such token"));
    }
    Ok(StatusCode::NO_CONTENT)
}
