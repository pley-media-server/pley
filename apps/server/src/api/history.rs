//! Watch history (`/api/history`).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Serialize;

use super::truthy;
use crate::auth::AuthUser;
use crate::error::{ApiResult, AppError};
use crate::history::{self, Filter, HistoryEntry};
use crate::state::AppState;

#[derive(Serialize)]
pub struct HistoryPage {
    total: i64,
    entries: Vec<HistoryEntry>,
}

/// Your plays, newest first. Administrators, and pley's own apps that
/// manage the server, can ask for one user's (`user_id`) or everyone's
/// (`all=1`), as the dashboard does.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<HistoryPage>> {
    let parse = |key: &str| query.get(key).and_then(|v| v.parse::<i64>().ok());
    let user_id = match (user.administers(), query.get("all").is_some_and(|v| truthy(v)), parse("user_id")) {
        (true, true, _) => None,
        (true, false, Some(id)) => Some(id),
        _ => Some(user.id),
    };
    let filter = Filter {
        user_id,
        item_id: parse("item_id"),
        library_id: parse("library_id"),
        since: parse("since"),
        offset: parse("offset").unwrap_or(0).max(0),
        limit: parse("limit").unwrap_or(50).clamp(1, 200),
    };
    let (records, total) = history::list(&state.db, user.id, &filter).await?;
    Ok(Json(HistoryPage { total, entries: records.iter().map(history::Record::to_entry).collect() }))
}

/// Removes a play: one's own, or, for who manages the server (the
/// dashboard), anyone's.
pub async fn delete(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if history::delete(&state.db, user.id, user.administers(), id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found(format!("history entry {id} not found")))
    }
}
