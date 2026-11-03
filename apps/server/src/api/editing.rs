//! Metadata editing for administrators: edit fields, fix the TMDB match and
//! choose or upload artwork. Edits are stored as overrides (see
//! [`crate::overrides`]), so later refreshes keep them.

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use super::items::item_or_404;
use crate::auth::AdminUser;
use crate::catalog::{self, ItemDetail};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::metadata::{ArtworkCandidate, MatchCandidate};
use crate::overrides;
use crate::state::AppState;
use crate::util::now;

async fn detail(state: &AppState, user_id: i64, id: i64) -> ApiResult<ItemDetail> {
    let row = item_or_404(state, user_id, id).await?;
    let mut detail = catalog::item_detail(&state.db, row).await?;
    detail.locked_fields = overrides::locked_fields(&state.db, id).await?;
    detail.in_watchlist = catalog::in_watchlist(&state.db, user_id, id).await?;
    Ok(detail)
}

/// `{ "title": "Alien", "year": 1979, "summary": null, … }`: a value sets and
/// locks the field, `null` unlocks it (the next refresh fills it again).
pub async fn edit(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<HashMap<String, Value>>,
) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    let mut changes = Vec::new();
    for (field, value) in body {
        let value = match value {
            Value::Null => None,
            Value::String(s) => Some(s.trim().to_owned()),
            Value::Number(n) => Some(n.to_string()),
            Value::Array(list) => {
                let names: Vec<String> = list
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                Some(serde_json::to_string(&names).map_err(|e| AppError::Internal(e.into()))?)
            }
            _ => return Err(AppError::bad_request(format!("unsupported value for '{field}'"))),
        };
        if matches!(field.as_str(), "poster" | "backdrop" | "thumb") {
            return Err(AppError::bad_request("use the artwork endpoints to change pictures"));
        }
        overrides::validate(&field, value.as_deref())?;
        changes.push((field, value));
    }
    overrides::set(&state.db, id, &changes).await?;
    if changes.iter().any(|(_, v)| v.is_none()) && state.metadata.can_refresh(&row.kind) {
        refresh_later(&state, id, row.library_id);
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(detail(&state, user.id, id).await?))
}

fn refresh_later(state: &AppState, id: i64, library_id: i64) {
    let metadata = state.metadata.clone();
    let events = state.events.clone();
    tokio::spawn(async move {
        match metadata.refresh_item(id).await {
            Ok(()) => events.send(ServerEvent::LibraryChanged { library_id }),
            Err(err) => tracing::warn!("metadata refresh for item {id} failed: {err:#}"),
        }
    });
}

#[derive(Deserialize)]
pub struct MatchQuery {
    title: Option<String>,
    year: Option<i64>,
}

/// TMDB candidates for "Fix match" (movies and shows).
pub async fn matches(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
    Query(query): Query<MatchQuery>,
) -> ApiResult<Json<Vec<MatchCandidate>>> {
    let row = item_or_404(&state, user.id, id).await?;
    if !state.metadata.tmdb_enabled() {
        return Err(AppError::bad_request("add a TMDB API key in Settings → Server to search for matches"));
    }
    let tv = match row.kind.as_str() {
        "movie" => false,
        "show" => true,
        _ => return Err(AppError::bad_request("only movies and shows are matched")),
    };
    let title = query.title.filter(|t| !t.trim().is_empty()).unwrap_or(row.title.clone());
    // A different search term usually means the old year is wrong too.
    let year = query.year.or_else(|| title.trim().eq_ignore_ascii_case(row.title.trim()).then_some(row.year).flatten());
    let found = state.metadata.search_matches(tv, title.trim(), year).await.map_err(AppError::Internal)?;
    Ok(Json(found))
}

#[derive(Deserialize)]
pub struct Rematch {
    tmdb_id: i64,
}

pub async fn rematch(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Rematch>,
) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    if !matches!(row.kind.as_str(), "movie" | "show") {
        return Err(AppError::bad_request("only movies and shows are matched"));
    }
    state.metadata.rematch(id, body.tmdb_id).await.map_err(AppError::Internal)?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(detail(&state, user.id, id).await?))
}

fn artwork_kind(kind: &str) -> ApiResult<&str> {
    match kind {
        "poster" | "backdrop" | "thumb" => Ok(kind),
        _ => Err(AppError::not_found("unknown artwork kind")),
    }
}

pub async fn artwork_options(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((id, kind)): Path<(i64, String)>,
) -> ApiResult<Json<Vec<ArtworkCandidate>>> {
    item_or_404(&state, user.id, id).await?;
    let kind = artwork_kind(&kind)?;
    if !state.metadata.tmdb_enabled() {
        return Err(AppError::bad_request("add a TMDB API key in Settings → Server to browse artwork"));
    }
    Ok(Json(state.metadata.artwork_candidates(id, kind).await.map_err(|e| AppError::bad_request(e.to_string()))?))
}

#[derive(Deserialize)]
pub struct ChooseArtwork {
    url: String,
}

/// Use one of the TMDB images from [`artwork_options`].
pub async fn choose_artwork(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((id, kind)): Path<(i64, String)>,
    Json(body): Json<ChooseArtwork>,
) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    let kind = artwork_kind(&kind)?;
    let bytes = state.metadata.download_artwork(&body.url).await.map_err(|e| AppError::bad_request(e.to_string()))?;
    let blob = store_picture(&state, bytes.into()).await?;
    overrides::set(&state.db, id, &[(kind.to_owned(), Some(blob))]).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(detail(&state, user.id, id).await?))
}

const UPLOAD_MAX_BYTES: usize = 25 * 1024 * 1024;

/// Upload a picture (raw body) as the item's poster, backdrop or thumbnail.
pub async fn upload_artwork(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((id, kind)): Path<(i64, String)>,
    body: Bytes,
) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    let kind = artwork_kind(&kind)?.to_owned();
    let blob = store_picture(&state, body).await?;
    overrides::set(&state.db, id, &[(kind, Some(blob))]).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(detail(&state, user.id, id).await?))
}

/// Stores an uploaded picture, re-encoded as JPEG: only real pictures get
/// stored, and huge ones are scaled down. None of the file's metadata is
/// kept but Kometa's overlay mark, which tells it a poster already has its
/// overlay. Returns its reference.
pub(crate) async fn store_picture(state: &AppState, body: Bytes) -> ApiResult<String> {
    if body.is_empty() || body.len() > UPLOAD_MAX_BYTES {
        return Err(AppError::bad_request("pictures must be smaller than 25 MB"));
    }
    let _decoding = crate::images::UPLOAD_DECODES.acquire().await.map_err(|e| AppError::Internal(e.into()))?;
    let jpeg = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        use image::ImageEncoder;
        let img =
            crate::images::decode(&body).map_err(|_| AppError::bad_request("that file is not a supported picture"))?;
        let img = if img.width() > 3840 || img.height() > 3840 {
            img.resize(3840, 3840, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };
        let mut out = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
        if crate::images::has_overlay_mark(&body) {
            encoder
                .set_exif_metadata(crate::images::overlay_exif())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("marking picture: {e}")))?;
        }
        encoder
            .encode_image(&img.to_rgb8())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("encoding picture: {e}")))?;
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))??;
    state.store.put(&jpeg, "jpg").await.map_err(|e| AppError::Internal(e.into()))
}

/// Go back to automatic artwork (local files or the metadata agent).
pub async fn reset_artwork(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((id, kind)): Path<(i64, String)>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    let kind = artwork_kind(&kind)?;
    overrides::set(&state.db, id, &[(kind.to_owned(), None)]).await?;
    // `kind` is one of three column names.
    sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE items SET {kind} = NULL, updated_at = ? WHERE id = ?")))
        .bind(now())
        .bind(id)
        .execute(&state.db.write)
        .await?;
    if state.metadata.can_refresh(&row.kind) {
        refresh_later(&state, id, row.library_id);
    } else {
        state.scanner.enqueue(row.library_id, false);
    }
    Ok(StatusCode::NO_CONTENT)
}
