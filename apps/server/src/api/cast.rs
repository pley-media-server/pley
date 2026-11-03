//! Casting (docs/API.md, "Casting to another device"): the profile's devices that may be cast to,
//! what to play on one and how to steer it, and what each one plays.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::cast::{CastAction, CastDevice, PlayState};
use crate::catalog;
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::state::AppState;

/// Positions and lengths past this many milliseconds (a week) are mistakes.
const POSITION_MAX: i64 = 7 * 24 * 3600 * 1000;

pub async fn devices(State(state): State<AppState>, user: AuthUser) -> Json<Vec<CastDevice>> {
    Json(state.cast.list(user.id))
}

#[derive(Deserialize)]
pub struct PlayBody {
    item_id: i64,
    #[serde(default)]
    file_id: Option<i64>,
    /// Left out: where the profile left off.
    #[serde(default)]
    position_ms: Option<i64>,
}

/// Plays a movie, an episode or an extra on device `id`.
pub async fn play(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<PlayBody>,
) -> ApiResult<StatusCode> {
    let id = listed(&state, &user, &id)?;
    let row = catalog::item(&state.db, user.id, body.item_id)
        .await?
        .ok_or_else(|| AppError::not_found(format!("item {} not found", body.item_id)))?;
    if !matches!(row.kind.as_str(), "movie" | "episode" | "extra") {
        return Err(AppError::bad_request("only movies, episodes and extras are cast"));
    }
    if let Some(file_id) = body.file_id
        && !catalog::files_for_item(&state.db, row.id).await?.iter().any(|f| f.id == file_id)
    {
        return Err(AppError::not_found("file not found"));
    }
    let position_ms = body.position_ms.map(position).transpose()?;
    state.events.send(ServerEvent::CastPlay {
        user_id: user.id,
        device_id: id,
        item_id: row.id,
        file_id: body.file_id,
        position_ms,
    });
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ControlBody {
    action: CastAction,
    #[serde(default)]
    position_ms: Option<i64>,
}

/// Pauses, resumes, seeks or stops device `id`'s player.
pub async fn control(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<ControlBody>,
) -> ApiResult<StatusCode> {
    let id = listed(&state, &user, &id)?;
    let position_ms = match (body.action, body.position_ms) {
        (CastAction::Seek, Some(ms)) => Some(position(ms)?),
        (CastAction::Seek, None) => return Err(AppError::bad_request("seek needs position_ms")),
        _ => None,
    };
    state.events.send(ServerEvent::CastControl { user_id: user.id, device_id: id, action: body.action, position_ms });
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct StateBody {
    /// `None`: it plays nothing (any longer).
    item_id: Option<i64>,
    #[serde(default)]
    position_ms: i64,
    #[serde(default)]
    duration_ms: Option<i64>,
    #[serde(default = "playing")]
    state: PlayState,
}

fn playing() -> PlayState {
    PlayState::Playing
}

/// Device `id` says what it plays: the profile's other devices hear it.
pub async fn report(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<StateBody>,
) -> ApiResult<StatusCode> {
    let id = listed(&state, &user, &id)?;
    let playing = match body.item_id {
        None => None,
        Some(item_id) => {
            let row = catalog::item(&state.db, user.id, item_id)
                .await?
                .ok_or_else(|| AppError::not_found(format!("item {item_id} not found")))?;
            let duration_ms = body.duration_ms.map(position).transpose()?;
            Some((row.to_summary(), position(body.position_ms)?, duration_ms, body.state))
        }
    };
    if !state.cast.report(user.id, &id, playing) {
        return Err(not_connected());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Device `id` of the user, while it is listed.
fn listed(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<String> {
    let id = crate::cast::valid_id(id).map_err(|_| not_connected())?;
    if state.cast.connected(user.id, &id) { Ok(id) } else { Err(not_connected()) }
}

fn not_connected() -> AppError {
    AppError::not_found("that device isn't connected")
}

fn position(ms: i64) -> ApiResult<i64> {
    if (0..=POSITION_MAX).contains(&ms) {
        Ok(ms)
    } else {
        Err(AppError::bad_request("a position is 0 to a week in milliseconds"))
    }
}
