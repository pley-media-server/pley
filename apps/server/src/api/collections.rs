//! Collections (per library, shared) — see `crate::collections`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::{Deserialize, Deserializer, Serialize};

use super::items::item_or_404;
use super::libraries::visible_library;
use crate::auth::{AdminUser, AuthUser};
use crate::catalog::ItemSummary;
use crate::collections::{self, CollectionDto, CollectionRef};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::state::AppState;

/// `Some(None)` for an explicit `null`, `None` when the field is absent.
fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

fn clean_title(title: &str) -> ApiResult<String> {
    let title = title.trim().chars().take(200).collect::<String>();
    if title.is_empty() {
        return Err(AppError::bad_request("a title is required"));
    }
    Ok(title)
}

#[derive(Serialize)]
pub struct CollectionList {
    collections: Vec<CollectionDto>,
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(library_id): Path<i64>,
) -> ApiResult<Json<CollectionList>> {
    visible_library(&state, &user, library_id).await?;
    Ok(Json(CollectionList { collections: collections::list(&state.db, &user, library_id).await? }))
}

#[derive(Serialize)]
pub struct CollectionDetail {
    #[serde(flatten)]
    collection: CollectionDto,
    items: Vec<ItemSummary>,
}

async fn detail(state: &AppState, user: &AuthUser, id: i64, summaries: bool) -> ApiResult<CollectionDetail> {
    let (collection, items) = collections::get(&state.db, user, id)
        .await?
        .ok_or_else(|| AppError::not_found(format!("collection {id} not found")))?;
    Ok(CollectionDetail { collection, items: items.iter().map(|r| r.to_summary_with(summaries)).collect() })
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<CollectionDetail>> {
    Ok(Json(detail(&state, &user, id, super::wants_summaries(&query)).await?))
}

#[derive(Deserialize)]
pub struct NewCollection {
    title: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    item_ids: Vec<i64>,
    /// A smart collection: the section listing query (Plex's filter syntax,
    /// `genre=Comedy&unwatched=1&sort=year:desc`) whose items it shows.
    #[serde(default)]
    smart_filter: Option<String>,
}

/// A smart filter that names something to find in `lib`.
fn smart_filter(lib: &crate::library::LibraryRow, filter: &str) -> ApiResult<String> {
    let filter = filter.trim().trim_start_matches('?');
    if filter.len() > 2000 || !crate::listing::valid_smart_filter(lib, filter) {
        return Err(AppError::bad_request("smart_filter must be a library filter such as genre=Comedy&unwatched=1"));
    }
    Ok(filter.to_owned())
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(library_id): Path<i64>,
    Json(body): Json<NewCollection>,
) -> ApiResult<(StatusCode, Json<CollectionDetail>)> {
    let lib = visible_library(&state, &admin, library_id).await?;
    if lib.kind() == crate::library::LibraryKind::Music {
        return Err(AppError::bad_request("collections are for movie and TV libraries"));
    }
    let title = clean_title(&body.title)?;
    let summary = body.summary.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let id = match &body.smart_filter {
        Some(filter) => {
            let filter = smart_filter(&lib, filter)?;
            let id = collections::create_smart(&state.db, library_id, &title, &filter).await?;
            collections::update(&state.db, id, None, Some(summary.as_deref()), None).await?;
            id
        }
        None => {
            let id = collections::create(&state.db, library_id, &title, summary.as_deref()).await?;
            collections::add_items(&state.db, id, library_id, &body.item_ids).await?;
            id
        }
    };
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok((StatusCode::CREATED, Json(detail(&state, &admin, id, false).await?)))
}

#[derive(Deserialize)]
pub struct CollectionPatch {
    #[serde(default)]
    title: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    summary: Option<Option<String>>,
    #[serde(default)]
    sort: Option<String>,
    /// A new filter for a smart collection.
    #[serde(default)]
    smart_filter: Option<String>,
}

async fn library_of(state: &AppState, id: i64) -> ApiResult<i64> {
    collections::library_of(&state.db, id)
        .await?
        .ok_or_else(|| AppError::not_found(format!("collection {id} not found")))
}

pub async fn update(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<CollectionPatch>,
) -> ApiResult<Json<CollectionDetail>> {
    let library_id = library_of(&state, id).await?;
    let title = body.title.as_deref().map(clean_title).transpose()?;
    if let Some(sort) = &body.sort
        && !matches!(sort.as_str(), "release" | "title" | "custom")
    {
        return Err(AppError::bad_request("sort must be release, title or custom"));
    }
    let summary = body.summary.map(|s| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()));
    if let Some(filter) = &body.smart_filter {
        if !collections::is_smart(&state.db, id).await? {
            return Err(AppError::bad_request("only smart collections have a filter"));
        }
        let lib = crate::library::get_library(&state.db, library_id)
            .await?
            .ok_or_else(|| AppError::not_found("library not found"))?;
        collections::set_smart_filter(&state.db, id, &smart_filter(&lib, filter)?).await?;
    }
    collections::update(&state.db, id, title.as_deref(), summary.as_ref().map(|s| s.as_deref()), body.sort.as_deref())
        .await?;
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(Json(detail(&state, &admin, id, false).await?))
}

pub async fn delete(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let library_id = library_of(&state, id).await?;
    collections::delete(&state.db, id).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::NO_CONTENT)
}

/// Smart collections fill themselves: their items aren't added, removed or
/// ordered by hand.
async fn refuse_smart(state: &AppState, id: i64) -> ApiResult<()> {
    if collections::is_smart(&state.db, id).await? {
        return Err(AppError::bad_request("a smart collection is filled by its filter"));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ItemIds {
    item_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct Added {
    added: u64,
}

pub async fn add_items(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<ItemIds>,
) -> ApiResult<Json<Added>> {
    let library_id = library_of(&state, id).await?;
    refuse_smart(&state, id).await?;
    if !body.item_ids.is_empty() && collections::eligible(&state.db, library_id, &body.item_ids).await? == 0 {
        return Err(AppError::bad_request("only movies and shows of the same library can be added"));
    }
    let added = collections::add_items(&state.db, id, library_id, &body.item_ids).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(Json(Added { added }))
}

pub async fn remove_item(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((id, item_id)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    let library_id = library_of(&state, id).await?;
    refuse_smart(&state, id).await?;
    if !collections::remove_item(&state.db, id, item_id).await? {
        return Err(AppError::not_found("the item is not in this collection"));
    }
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reorder(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<ItemIds>,
) -> ApiResult<StatusCode> {
    let library_id = library_of(&state, id).await?;
    refuse_smart(&state, id).await?;
    collections::reorder(&state.db, id, &body.item_ids).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::NO_CONTENT)
}

/// Collection artwork; without its own, the first visible item's.
pub async fn image(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, kind)): Path<(i64, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if !matches!(kind.as_str(), "poster" | "backdrop") {
        return Err(AppError::not_found("unknown image kind"));
    }
    let width = query.get("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0);
    match collections::artwork(&state.db, &state.store, &user, id, &kind).await? {
        Some(Ok(path)) => serve_file(&state, &path, width, query.contains_key("v"), &headers).await,
        Some(Err(item_id)) => {
            let w = width.map(|w| format!("?w={w}")).unwrap_or_default();
            Ok(Redirect::temporary(&format!("/api/items/{item_id}/images/{kind}{w}")).into_response())
        }
        None => Err(AppError::not_found("no artwork")),
    }
}

/// An artwork file, resized to `width`, with cache headers.
pub(crate) async fn serve_file(
    state: &AppState,
    path: &std::path::Path,
    width: Option<u32>,
    versioned: bool,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let image = state.images.load(path, width).await?;
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(image.etag.as_str()) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let mut out = HeaderMap::new();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(image.content_type));
    let cache = if versioned { "private, max-age=31536000, immutable" } else { "private, max-age=300" };
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(etag) = HeaderValue::from_str(&image.etag) {
        out.insert(header::ETAG, etag);
    }
    Ok((out, image.bytes).into_response())
}

#[derive(Serialize)]
pub struct ItemCollections {
    collections: Vec<CollectionRef>,
}

/// Collections a movie or show is part of.
pub async fn for_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<ItemCollections>> {
    item_or_404(&state, user.id, id).await?;
    Ok(Json(ItemCollections { collections: collections::for_item(&state.db, &user, id).await? }))
}
