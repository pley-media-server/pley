//! Two-step sign-in (see [`crate::two_step`]): setting it up, moving it to
//! another authenticator app, turning it off and new recovery codes for the
//! signed-in user; an administrator turning it off for someone who lost
//! their authenticator.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use super::auth::confirm_password;
use crate::auth::{AdminUser, AuthUser};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;
use crate::two_step::{Setup, Status};

#[derive(Deserialize)]
pub struct Confirm {
    /// The account's password: a session alone can't change how it signs in.
    #[serde(default)]
    password: Option<String>,
    /// A code from the authenticator app, or a recovery code.
    #[serde(default)]
    code: Option<String>,
}

/// Shown once: only their hashes are kept.
#[derive(Serialize)]
pub struct RecoveryCodes {
    recovery_codes: Vec<String>,
}

/// `GET /api/me/two-step`
pub async fn status(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Status>> {
    Ok(Json(state.two_step.status(user.id).await?))
}

/// `POST /api/me/two-step/setup {password, code?}`: a new secret for an
/// authenticator app (a QR code to scan), until a code from it turns
/// two-step sign-in on. While it is on, the secret moves it to another app,
/// and takes a code from the current one (or a recovery code) as well, as
/// turning it off does.
pub async fn setup(State(state): State<AppState>, user: AuthUser, Json(body): Json<Confirm>) -> ApiResult<Json<Setup>> {
    // Said before the code is checked: a recovery code would be spent on a
    // secret the server can't make.
    state.two_step.ready_to_set_up()?;
    confirm_password(&state, &user, body.password).await?;
    let replacing = state.two_step.is_on(user.id).await?;
    if replacing {
        state.two_step.check(&state.auth.throttle, user.id, body.code.as_deref()).await?;
    }
    Ok(Json(state.two_step.begin(user.id, &user.username, &state.settings.get().server_name, replacing)?))
}

/// `POST /api/me/two-step {code}`: turns it on with a code from the secret
/// set up, or moves it to that secret; the new recovery codes.
pub async fn enable(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<Confirm>,
) -> ApiResult<Json<RecoveryCodes>> {
    let finished = state.two_step.finish(user.id, body.code.as_deref().unwrap_or_default()).await?;
    if finished.replaced {
        tracing::info!("'{}' moved two-step sign-in to another authenticator app", user.username);
    } else {
        tracing::info!("'{}' turned on two-step sign-in", user.username);
    }
    Ok(Json(RecoveryCodes { recovery_codes: finished.recovery_codes }))
}

/// `DELETE /api/me/two-step {password, code}`: turns it off, with the
/// password and a code (both, so neither alone can). Not while the server
/// requires it: it can move to another app instead.
pub async fn disable(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<Confirm>,
) -> ApiResult<StatusCode> {
    confirm_password(&state, &user, body.password).await?;
    if !state.two_step.is_on(user.id).await? {
        return Err(AppError::bad_request("two-step sign-in is off"));
    }
    if state.auth.two_step_required() {
        return Err(AppError::forbidden(
            "this server requires two-step sign-in, so it stays on: you can move it to another authenticator app",
        ));
    }
    state.two_step.check(&state.auth.throttle, user.id, body.code.as_deref()).await?;
    state.two_step.turn_off(user.id).await?;
    tracing::info!("'{}' turned off two-step sign-in", user.username);
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/me/two-step/recovery-codes {password}`: new recovery codes;
/// the old ones stop working.
pub async fn recovery_codes(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<Confirm>,
) -> ApiResult<Json<RecoveryCodes>> {
    confirm_password(&state, &user, body.password).await?;
    Ok(Json(RecoveryCodes { recovery_codes: state.two_step.new_recovery_codes(user.id).await? }))
}

/// `DELETE /api/users/{id}/two-step`: an administrator turns two-step
/// sign-in off for someone who lost their authenticator app and their
/// recovery codes. When the server requires it, they are signed out
/// everywhere, and set it up again at their next sign-in.
pub async fn reset_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    // One's own, from Account, which takes the password and a code: a
    // session alone isn't enough.
    if id == admin.id {
        return Err(AppError::bad_request("turn your own two-step sign-in off in Account"));
    }
    if !state.two_step.turn_off(id).await? {
        return Err(AppError::not_found("this user doesn't use two-step sign-in"));
    }
    if state.auth.two_step_required() {
        state.oauth.forget_user(id);
        state.auth.sign_out_everywhere(id, None).await?;
    }
    tracing::info!("'{}' turned off two-step sign-in for user {id}", admin.username);
    Ok(StatusCode::NO_CONTENT)
}
