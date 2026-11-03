//! Playlists (per user, private) — see `crate::playlists`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{ApiResult, AppError};
use crate::library::LibraryRow;
use crate::playlists::{self, EntryDto, Kind, PlaylistDto};
use crate::state::AppState;

fn clean_title(title: &str) -> ApiResult<String> {
    let title = title.trim().chars().take(200).collect::<String>();
    if title.is_empty() {
        return Err(AppError::bad_request("a title is required"));
    }
    Ok(title)
}

#[derive(Serialize)]
pub struct PlaylistList {
    playlists: Vec<PlaylistDto>,
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<PlaylistList>> {
    Ok(Json(PlaylistList { playlists: playlists::list(&state.db, &user).await? }))
}

#[derive(Serialize)]
pub struct PlaylistDetail {
    #[serde(flatten)]
    playlist: PlaylistDto,
    entries: Vec<EntryDto>,
}

async fn detail(state: &AppState, user: &AuthUser, id: i64, summaries: bool) -> ApiResult<PlaylistDetail> {
    let (playlist, entries) = playlists::get(&state.db, user, id, summaries)
        .await?
        .ok_or_else(|| AppError::not_found(format!("playlist {id} not found")))?;
    Ok(PlaylistDetail { playlist, entries })
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<PlaylistDetail>> {
    Ok(Json(detail(&state, &user, id, super::wants_summaries(&query)).await?))
}

#[derive(Deserialize)]
pub struct NewPlaylist {
    title: String,
    /// `video` or `audio`; taken from the first item when absent.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    item_ids: Vec<i64>,
    /// A smart playlist: a listing of `library_id` in Plex's filter
    /// syntax (`genre=Comedy&unwatched=1&sort=year:desc`) whose items it plays.
    #[serde(default)]
    smart_filter: Option<String>,
    #[serde(default)]
    library_id: Option<i64>,
}

/// A smart filter of a library the user may open.
async fn smart_filter_of(
    state: &AppState,
    user: &AuthUser,
    library_id: Option<i64>,
    filter: &str,
) -> ApiResult<(LibraryRow, String)> {
    let library_id = library_id.ok_or_else(|| AppError::bad_request("a smart playlist names its library_id"))?;
    let lib = crate::access::visible_libraries(&state.db, user)
        .await?
        .into_iter()
        .find(|l| l.id == library_id)
        .ok_or_else(|| AppError::not_found(format!("library {library_id} not found")))?;
    let filter = filter.trim().trim_start_matches('?');
    if filter.len() > 2000 || !crate::listing::valid_smart_filter(&lib, filter) {
        return Err(AppError::bad_request("smart_filter must be a library filter such as genre=Comedy&unwatched=1"));
    }
    Ok((lib, filter.to_owned()))
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<NewPlaylist>,
) -> ApiResult<(StatusCode, Json<PlaylistDetail>)> {
    let title = clean_title(&body.title)?;
    if let Some(filter) = &body.smart_filter {
        let (lib, filter) = smart_filter_of(&state, &user, body.library_id, filter).await?;
        let kind = if lib.kind() == crate::library::LibraryKind::Music { Kind::Audio } else { Kind::Video };
        let id = playlists::create_smart(&state.db, &user, &title, kind, lib.id, &filter).await?;
        return Ok((StatusCode::CREATED, Json(detail(&state, &user, id, false).await?)));
    }
    let kind = match body.kind.as_deref() {
        Some(k) => Kind::parse(k).ok_or_else(|| AppError::bad_request("kind must be video or audio"))?,
        None => playlists::kind_of_first(&state.db, &user, &body.item_ids).await?.ok_or_else(|| {
            AppError::bad_request("say whether the playlist is for video or audio, or start it with an item")
        })?,
    };
    let id = playlists::create(&state.db, &user, &title, kind).await?;
    if !body.item_ids.is_empty()
        && let Err(err) = playlists::add(&state.db, &user, id, &body.item_ids).await
    {
        playlists::delete(&state.db, &user, id).await?;
        return Err(err);
    }
    Ok((StatusCode::CREATED, Json(detail(&state, &user, id, false).await?)))
}

#[derive(Deserialize)]
pub struct PlaylistPatch {
    #[serde(default)]
    title: Option<String>,
    /// A new filter for a smart playlist.
    #[serde(default)]
    smart_filter: Option<String>,
}

pub async fn rename(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<PlaylistPatch>,
) -> ApiResult<Json<PlaylistDetail>> {
    if let Some(filter) = &body.smart_filter {
        let (playlist, _) = playlists::get_rows(&state.db, &user, id)
            .await?
            .ok_or_else(|| AppError::not_found(format!("playlist {id} not found")))?;
        if playlist.smart_filter.is_none() {
            return Err(AppError::bad_request("only smart playlists have a filter"));
        }
        let (_, filter) = smart_filter_of(&state, &user, playlist.library_id, filter).await?;
        playlists::set_smart_filter(&state.db, &user, id, &filter).await?;
    }
    if let Some(title) = &body.title {
        let title = clean_title(title)?;
        if !playlists::rename(&state.db, &user, id, &title).await? {
            return Err(AppError::not_found(format!("playlist {id} not found")));
        }
    }
    Ok(Json(detail(&state, &user, id, false).await?))
}

pub async fn delete(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if !playlists::delete(&state.db, &user, id).await? {
        return Err(AppError::not_found(format!("playlist {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ItemIds {
    item_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct Added {
    added: u64,
}

pub async fn add(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<ItemIds>,
) -> ApiResult<Json<Added>> {
    let added = playlists::add(&state.db, &user, id, &body.item_ids).await?;
    Ok(Json(Added { added }))
}

pub async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, entry_id)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    if playlists::is_smart(&state.db, &user, id).await? {
        return Err(AppError::bad_request("a smart playlist is filled by its filter"));
    }
    if !playlists::remove_entry(&state.db, &user, id, entry_id).await? {
        return Err(AppError::not_found("entry not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct EntryIds {
    entry_ids: Vec<i64>,
}

pub async fn reorder(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<EntryIds>,
) -> ApiResult<Json<PlaylistDetail>> {
    if playlists::is_smart(&state.db, &user, id).await? {
        return Err(AppError::bad_request("a smart playlist plays in its filter's order"));
    }
    if !playlists::reorder(&state.db, &user, id, &body.entry_ids).await? {
        return Err(AppError::not_found(format!("playlist {id} not found")));
    }
    Ok(Json(detail(&state, &user, id, false).await?))
}
