//! Metadata editing for administrators: edit fields, fix the match (TMDB's
//! for movies and shows, Open Library's or Audible's for audiobooks) and
//! choose or upload artwork. Edits are stored as overrides (see
//! [`crate::overrides`]), so later refreshes keep them.

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::items::item_or_404;
use crate::auth::AdminUser;
use crate::catalog::{self, ItemDetail};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::metadata::{ArtworkCandidate, MatchCandidate};
use crate::overrides;
use crate::providers::books::{BookCandidate, BookMatch};
use crate::state::AppState;
use crate::util::now;

async fn detail(state: &AppState, user_id: i64, id: i64) -> ApiResult<ItemDetail> {
    let row = item_or_404(state, user_id, id).await?;
    let mut detail = catalog::item_detail(&state.db, row).await?;
    detail.locked_fields = overrides::locked_fields(&state.db, id).await?;
    detail.nfo_fields = overrides::nfo_fields(&state.db, id).await?;
    detail.in_watchlist = catalog::in_watchlist(&state.db, user_id, id).await?;
    detail.labels = Some(crate::labels::for_items(&state.db, &[id]).await?.remove(&id).unwrap_or_default());
    Ok(detail)
}

/// `{ "title": "Alien", "year": 1979, "summary": null, … }`: a value sets and
/// locks the field, `null` unlocks it (the next refresh fills it again).
/// `"match": null` undoes Fix Match, for the item's NFO file to decide it.
/// `"labels": [...]` replaces the item's labels (they aren't metadata: a
/// refresh never changes them).
pub async fn edit(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
    Json(mut body): Json<HashMap<String, Value>>,
) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    let labels = match body.remove("labels") {
        None => None,
        Some(Value::Array(list)) if crate::labels::LABELLED.contains(&row.kind.as_str()) => {
            let names: Vec<String> = list.iter().filter_map(|v| v.as_str()).map(str::to_owned).collect();
            Some(crate::labels::check(&names)?)
        }
        Some(Value::Array(_)) => return Err(AppError::bad_request(format!("a {} has no labels", row.kind))),
        Some(_) => return Err(AppError::bad_request("'labels' is a list of names")),
    };
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
        if overrides::BOOK_FIELDS.contains(&field.as_str()) && row.kind != "audiobook" {
            return Err(AppError::bad_request(format!("only audiobooks have '{field}'")));
        }
        overrides::validate(&field, value.as_deref())?;
        changes.push((field, value));
    }
    overrides::set(&state.db, id, &changes).await?;
    if let Some(labels) = labels {
        crate::labels::replace(&state.db, id, &labels).await?;
    }
    // A book's sources only fill in what it lacks: what is unlocked is what
    // its files say again, for the refresh to fill in the rest.
    let unlocked: Vec<&str> = changes.iter().filter(|(_, v)| v.is_none()).map(|(f, _)| f.as_str()).collect();
    if row.kind == "audiobook" && !unlocked.is_empty() {
        crate::library::scanner::read_book_again(&state.db, id, &unlocked).await.map_err(AppError::Internal)?;
    }
    // Fix Match undone: the item's NFO file decides its match again, and a
    // book loses what its match filled in.
    if changes.iter().any(|(field, value)| field == overrides::MATCH && value.is_none()) {
        crate::library::scanner::nfo_match_dropped(&state.db, id).await?;
        if row.kind == "audiobook" {
            state.metadata.forget_book_match(id).await.map_err(AppError::Internal)?;
        }
    }
    if changes.iter().any(|(_, v)| v.is_none()) && state.metadata.can_refresh(&row.kind) {
        refresh_later(&state, id, row.library_id);
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(detail(&state, user.id, id).await?))
}

/// Fetches item `id`'s metadata again after the answer, as an unlocked field
/// is filled again.
pub(crate) fn refresh_later(state: &AppState, id: i64, library_id: i64) {
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
    /// An audiobook's writer.
    author: Option<String>,
}

/// What Fix Match offers: TMDB's movies or shows, or books.
#[derive(Serialize)]
#[serde(untagged)]
pub enum Candidates {
    Titles(Vec<MatchCandidate>),
    Books(Vec<BookCandidate>),
}

/// The most of a search term Fix Match passes on.
const MAX_SEARCH: usize = 200;

/// Candidates for "Fix match": TMDB's for movies and shows, Open Library's
/// and Audible's for audiobooks.
pub async fn matches(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
    Query(query): Query<MatchQuery>,
) -> ApiResult<Json<Candidates>> {
    let row = item_or_404(&state, user.id, id).await?;
    let search = |text: Option<String>| {
        text.map(|t| t.trim().chars().take(MAX_SEARCH).collect::<String>()).filter(|t| !t.is_empty())
    };
    if row.kind == "audiobook" {
        let title = search(query.title);
        let own: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT authors, audible_asin FROM items WHERE id = ?")
                .bind(id)
                .fetch_one(&state.db.read)
                .await?;
        // The book's own writer and recording go with its own title.
        let (author, own_asin) = match &title {
            Some(t) if !t.eq_ignore_ascii_case(row.title.trim()) => (search(query.author), None),
            _ => {
                let first =
                    own.0.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()).and_then(|a| a.into_iter().next());
                (search(query.author).or(first), own.1)
            }
        };
        let title = title.unwrap_or_else(|| row.title.clone());
        let found = state
            .metadata
            .search_books(&title, author.as_deref(), own_asin.as_deref())
            .await
            .map_err(|e| AppError::bad_request(e.to_string()))?;
        return Ok(Json(Candidates::Books(found)));
    }
    if !state.metadata.tmdb_enabled() {
        return Err(AppError::bad_request("add a TMDB API key in Settings → Server to search for matches"));
    }
    let tv = match row.kind.as_str() {
        "movie" => false,
        "show" => true,
        _ => return Err(AppError::bad_request("only movies, shows and audiobooks are matched")),
    };
    let title = query.title.filter(|t| !t.trim().is_empty()).unwrap_or(row.title.clone());
    // A different search term usually means the old year is wrong too.
    let year = query.year.or_else(|| title.trim().eq_ignore_ascii_case(row.title.trim()).then_some(row.year).flatten());
    let found = state.metadata.search_matches(tv, title.trim(), year).await.map_err(AppError::Internal)?;
    Ok(Json(Candidates::Titles(found)))
}

/// `{ "tmdb_id": 348 }` for a movie or show; `{ "openlibrary_id":
/// "OL21745884W" }` or `{ "asin": "B08G9PRS1K" }` for an audiobook.
#[derive(Deserialize)]
pub struct Rematch {
    tmdb_id: Option<i64>,
    openlibrary_id: Option<String>,
    asin: Option<String>,
}

pub async fn rematch(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<Rematch>,
) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    if row.kind == "audiobook" {
        let chosen = BookMatch::new(body.asin.as_deref(), body.openlibrary_id.as_deref())
            .ok_or_else(|| AppError::bad_request("an audiobook is matched to an Open Library work or an ASIN"))?;
        let result = state.metadata.rematch_book(id, &chosen).await;
        // The match is kept even when its source didn't answer: the next
        // refresh fetches it.
        state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
        result.map_err(|e| AppError::bad_request(e.to_string()))?;
        return Ok(Json(detail(&state, user.id, id).await?));
    }
    if !matches!(row.kind.as_str(), "movie" | "show") {
        return Err(AppError::bad_request("only movies, shows and audiobooks are matched"));
    }
    let tmdb_id = body.tmdb_id.ok_or_else(|| AppError::bad_request("a movie or show is matched to a TMDB id"))?;
    // A movie matched to the title of another copy went into it: that's the
    // movie now.
    let id = state.metadata.rematch(id, tmdb_id).await.map_err(AppError::Internal)?;
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
    let row = item_or_404(&state, user.id, id).await?;
    let kind = artwork_kind(&kind)?;
    if row.kind == "audiobook" {
        // A book's only picture is its cover.
        if kind != "poster" {
            return Ok(Json(Vec::new()));
        }
        return Ok(Json(state.metadata.book_covers(id).await.map_err(|e| AppError::bad_request(e.to_string()))?));
    }
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
    reset_picture(&state, id, &row.kind, row.library_id, kind).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Picture `kind` (poster, backdrop or thumb) of item `id`, of `item_kind`
/// in library `library_id`, goes back to the automatic one: the file's own
/// or the metadata source's, found again after the answer.
pub(crate) async fn reset_picture(
    state: &AppState,
    id: i64,
    item_kind: &str,
    library_id: i64,
    kind: &str,
) -> ApiResult<()> {
    let kind = artwork_kind(kind)?;
    overrides::set(&state.db, id, &[(kind.to_owned(), None)]).await?;
    // `kind` is one of three column names.
    sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE items SET {kind} = NULL, updated_at = ? WHERE id = ?")))
        .bind(now())
        .bind(id)
        .execute(&state.db.write)
        .await?;
    if state.metadata.can_refresh(item_kind) {
        refresh_later(state, id, library_id);
    } else {
        state.scanner.enqueue(library_id, false);
    }
    Ok(())
}

/// `GET /api/labels`: every label titles carry or profiles' and invitations'
/// rules name, with how many of each, for Edit details and the pickers of
/// profiles' label rules.
pub async fn labels(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<crate::labels::LabelUse>>> {
    Ok(Json(crate::labels::all(&state.db).await?))
}
