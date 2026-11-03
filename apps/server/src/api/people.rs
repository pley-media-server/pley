//! Cast and crew pages (`/api/people/*`).

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::auth::AuthUser;
use crate::error::{ApiResult, AppError};
use crate::people::{self, PersonDetail};
use crate::state::AppState;

pub async fn get(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<PersonDetail>> {
    people::person(&state.db, user.id, id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("person {id} not found")))
}

pub async fn photo(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    // Like the person's page: only through titles this user may see.
    if !people::is_visible(&state.db, user.id, id).await? {
        return Err(AppError::not_found("no photo"));
    }
    let width = query.get("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0);
    photo_of(&state, id, width, query.contains_key("v"), &headers).await
}

/// A person's photo at `width`, for a caller that checked it may be seen;
/// `versioned`: asked for at an address that changes with the photo.
pub(crate) async fn photo_of(
    state: &AppState,
    id: i64,
    width: Option<u32>,
    versioned: bool,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let photo: Option<Option<String>> =
        sqlx::query_scalar("SELECT photo_url FROM people WHERE id = ?").bind(id).fetch_optional(&state.db.read).await?;
    let Some(remote) = photo.flatten() else { return Err(AppError::not_found("no photo")) };
    let path = state.metadata.person_photo(&remote).await.map_err(|err| {
        tracing::debug!("photo of person {id}: {err:#}");
        AppError::not_found("no photo")
    })?;
    let image = state.images.load(&path, width).await?;
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(image.etag.as_str()) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let mut out = HeaderMap::new();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(image.content_type));
    // Versioned URLs (`?v=<photo>`) change whenever the photo does.
    let cache = if versioned { "private, max-age=31536000, immutable" } else { "private, max-age=300" };
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(etag) = HeaderValue::from_str(&image.etag) {
        out.insert(header::ETAG, etag);
    }
    Ok((out, image.bytes).into_response())
}
