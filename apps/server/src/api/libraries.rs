use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{truthy, wants_summaries};
use crate::access;
use crate::auth::{AdminUser, AuthUser};
use crate::catalog::{self, ItemSummary, LibraryQuery, ME_CTE, SortKey, VISIBLE, VISIBLE_ITEMS};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::i18n;
use crate::library::{LibraryKind, LibraryRow, get_library, set_order};
use crate::state::AppState;
use crate::util::now;

#[derive(Serialize)]
pub struct LibraryDto {
    id: i64,
    name: String,
    kind: LibraryKind,
    paths: Vec<String>,
    item_count: i64,
    created_at: i64,
    scanned_at: Option<i64>,
    scanning: bool,
}

/// `lib` as `viewer` sees it: its count leaves out the titles their age
/// limit and label rules hide (administrators and profiles without either
/// see them all).
async fn to_dto(state: &AppState, lib: &LibraryRow, viewer: Option<&AuthUser>) -> ApiResult<LibraryDto> {
    let kind = lib.kind().top_level_kind();
    let limited = match viewer {
        Some(user) => crate::access::limited(&state.db, user).await?,
        None => false,
    };
    let count: i64 = match viewer {
        Some(user) if limited => {
            let sql = format!(
                "{ME_CTE} SELECT COUNT(*) FROM {VISIBLE_ITEMS} WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ?"
            );
            sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(user.id)
                .bind(lib.id)
                .bind(kind)
                .fetch_one(&state.db.read)
                .await?
        }
        _ => {
            sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE library_id = ? AND kind = ?")
                .bind(lib.id)
                .bind(kind)
                .fetch_one(&state.db.read)
                .await?
        }
    };
    Ok(LibraryDto {
        id: lib.id,
        name: lib.name.clone(),
        kind: lib.kind(),
        paths: lib.paths().iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        item_count: count,
        created_at: lib.created_at,
        scanned_at: lib.scanned_at,
        scanning: state.scanner.is_scanning(lib.id),
    })
}

async fn library_or_404(state: &AppState, id: i64) -> ApiResult<LibraryRow> {
    get_library(&state.db, id).await?.ok_or_else(|| AppError::not_found(format!("library {id} not found")))
}

/// The library, if this user may open it (others look like they don't exist).
pub(crate) async fn visible_library(state: &AppState, user: &AuthUser, id: i64) -> ApiResult<LibraryRow> {
    if !access::can_open_library(&state.db, user, id).await? {
        return Err(AppError::not_found(format!("library {id} not found")));
    }
    library_or_404(state, id).await
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    order: Option<String>,
}

/// The libraries the user may open, in their own order; with
/// `order=shared`, in the order everyone sees them in (the one
/// administrators choose, in Settings).
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Vec<LibraryDto>>> {
    let libraries = if query.order.as_deref() == Some("shared") {
        access::shared_order(&state.db, &user).await?
    } else {
        access::visible_libraries(&state.db, &user).await?
    };
    listed(&state, &user, libraries).await
}

/// `libraries` as `user` sees them.
async fn listed(state: &AppState, user: &AuthUser, libraries: Vec<LibraryRow>) -> ApiResult<Json<Vec<LibraryDto>>> {
    let mut out = Vec::new();
    for lib in libraries {
        out.push(for_user(user, to_dto(state, &lib, Some(user)).await?));
    }
    Ok(Json(out))
}

pub async fn get(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<LibraryDto>> {
    let lib = visible_library(&state, &user, id).await?;
    Ok(Json(for_user(&user, to_dto(&state, &lib, Some(&user)).await?)))
}

/// Apps and non-administrators aren't told where the server keeps its media.
fn for_user(user: &AuthUser, mut dto: LibraryDto) -> LibraryDto {
    if user.app.is_some() || !user.is_admin {
        dto.paths.clear();
    }
    dto
}

#[derive(Deserialize)]
pub struct CreateLibrary {
    name: String,
    kind: LibraryKind,
    paths: Vec<String>,
}

fn validate_paths(paths: &[String]) -> ApiResult<Vec<String>> {
    let mut out = Vec::new();
    for raw in paths {
        let p = raw.trim();
        if p.is_empty() {
            continue;
        }
        let path = PathBuf::from(p);
        if !path.is_absolute() {
            return Err(AppError::bad_request(format!("'{p}' is not an absolute path")));
        }
        if !path.is_dir() {
            return Err(AppError::bad_request(format!("folder '{p}' does not exist or is not accessible")));
        }
        let normalized = path.to_string_lossy().trim_end_matches(['/', '\\']).to_owned();
        let normalized = if normalized.ends_with(':') || normalized.is_empty() { p.to_owned() } else { normalized };
        if !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        return Err(AppError::bad_request("add at least one folder"));
    }
    Ok(out)
}

pub async fn create(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateLibrary>,
) -> ApiResult<(StatusCode, Json<LibraryDto>)> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request("library name must be 1-100 characters"));
    }
    let paths = validate_paths(&body.paths)?;
    // Listed last, with an id no library has had (migrations/0021).
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO libraries (id, name, kind, paths, created_at, position)
         VALUES ((SELECT last + 1 FROM library_ids), ?, ?, ?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM libraries))
         RETURNING id",
    )
    .bind(name)
    .bind(body.kind.as_str())
    .bind(serde_json::to_string(&paths).map_err(|e| AppError::Internal(e.into()))?)
    .bind(now())
    .fetch_one(&state.db.write)
    .await?;
    state.scanner.enqueue(id, false);
    state.watcher.reload().await;
    state.events.send(ServerEvent::LibrariesChanged);
    let lib = library_or_404(&state, id).await?;
    Ok((StatusCode::CREATED, Json(to_dto(&state, &lib, None).await?)))
}

#[derive(Deserialize)]
pub struct UpdateLibrary {
    name: Option<String>,
    paths: Option<Vec<String>>,
}

pub async fn update(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateLibrary>,
) -> ApiResult<Json<LibraryDto>> {
    let lib = library_or_404(&state, id).await?;
    if let Some(name) = body.name.as_deref().map(str::trim) {
        if name.is_empty() || name.chars().count() > 100 {
            return Err(AppError::bad_request("library name must be 1-100 characters"));
        }
        sqlx::query("UPDATE libraries SET name = ? WHERE id = ?").bind(name).bind(id).execute(&state.db.write).await?;
    }
    if let Some(paths) = &body.paths {
        let paths = validate_paths(paths)?;
        let json = serde_json::to_string(&paths).map_err(|e| AppError::Internal(e.into()))?;
        if json != lib.paths {
            sqlx::query("UPDATE libraries SET paths = ? WHERE id = ?")
                .bind(json)
                .bind(id)
                .execute(&state.db.write)
                .await?;
            state.scanner.enqueue(id, false);
            state.watcher.reload().await;
        }
    }
    state.events.send(ServerEvent::LibrariesChanged);
    let lib = library_or_404(&state, id).await?;
    Ok(Json(to_dto(&state, &lib, None).await?))
}

#[derive(Deserialize)]
pub struct LibraryOrder {
    ids: Vec<i64>,
}

impl LibraryOrder {
    /// `400` when an id is listed twice.
    fn check(&self) -> ApiResult<()> {
        let mut seen = HashSet::new();
        match self.ids.iter().find(|id| !seen.insert(**id)) {
            Some(id) => Err(AppError::bad_request(format!("library {id} is listed twice"))),
            None => Ok(()),
        }
    }
}

/// The order libraries are listed in, for everyone who hasn't chosen their
/// own: `ids` lists every library once. Answers them in their new order.
pub async fn reorder(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(body): Json<LibraryOrder>,
) -> ApiResult<Json<Vec<LibraryDto>>> {
    body.check()?;
    if !set_order(&state.db, &body.ids).await? {
        return Err(AppError::conflict("the libraries have changed: the order must list each of them once"));
    }
    state.events.send(ServerEvent::LibrariesChanged);
    let libraries = access::shared_order(&state.db, &admin).await?;
    listed(&state, &admin, libraries).await
}

/// `PUT /api/me/libraries/order`: the order the user sees their libraries
/// in, theirs alone: `ids` lists each library they may open once. Answers
/// them in it.
pub async fn set_my_order(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<LibraryOrder>,
) -> ApiResult<Json<Vec<LibraryDto>>> {
    body.check()?;
    if !access::set_own_order(&state.db, &user, &body.ids).await? {
        return Err(AppError::conflict("your libraries have changed: the order must list each of them once"));
    }
    state.events.send(ServerEvent::OwnLibrariesChanged { user_id: user.id });
    let libraries = access::visible_libraries(&state.db, &user).await?;
    listed(&state, &user, libraries).await
}

/// `DELETE /api/me/libraries/order`: the user sees their libraries in the
/// order everyone does again. Answers them in it.
pub async fn forget_my_order(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<LibraryDto>>> {
    access::forget_own_order(&state.db, &user).await?;
    state.events.send(ServerEvent::OwnLibrariesChanged { user_id: user.id });
    let libraries = access::shared_order(&state.db, &user).await?;
    listed(&state, &user, libraries).await
}

pub async fn remove(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let result = sqlx::query("DELETE FROM libraries WHERE id = ?").bind(id).execute(&state.db.write).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found(format!("library {id} not found")));
    }
    state.watcher.reload().await;
    state.events.send(ServerEvent::LibrariesChanged);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, Default)]
pub struct ScanBody {
    #[serde(default)]
    force: bool,
}

pub async fn scan(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    body: Option<Json<ScanBody>>,
) -> ApiResult<StatusCode> {
    library_or_404(&state, id).await?;
    state.scanner.enqueue(id, body.map(|b| b.force).unwrap_or(false));
    Ok(StatusCode::ACCEPTED)
}

pub async fn scan_all(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<StatusCode> {
    state.scanner.enqueue_all(false).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize)]
pub struct ItemsPage {
    total: i64,
    items: Vec<ItemSummary>,
}

/// A tag for a library listing as `user` sees it, from what it depends on:
/// the versions of the library's items and of the user's watch state (bumped
/// by triggers on every change), the user's age limit, the query, and this
/// server process (a newer version may answer differently).
async fn listing_etag(state: &AppState, user: &AuthUser, library_id: i64, query: &str) -> ApiResult<String> {
    let versions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT scope, version FROM listing_versions
         WHERE (scope = 'library' AND id = ?) OR (scope = 'user' AND id = ?) ORDER BY scope",
    )
    .bind(library_id)
    .bind(user.id)
    .fetch_all(&state.db.read)
    .await?;
    let mut hash = Sha256::new();
    hash.update(format!(
        "{}|{}|{library_id}|{query}|{versions:?}|{}|{:?}",
        env!("CARGO_PKG_VERSION"),
        crate::util::process_epoch(),
        user.id,
        (user.is_admin, user.max_age),
    ));
    Ok(format!("W/\"{}\"", hex::encode(&hash.finalize()[..12])))
}

pub async fn items(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
) -> ApiResult<Response> {
    let lib = visible_library(&state, &user, id).await?;
    // Unchanged since the browser's copy: say so instead of sending it again
    // (a whole library is several MB of JSON).
    let etag = listing_etag(&state, &user, lib.id, uri.query().unwrap_or_default()).await?;
    let cache = [(header::ETAG, etag.clone()), (header::CACHE_CONTROL, "private, no-cache".to_owned())];
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|tags| tags.split(',').any(|tag| tag.trim() == etag));
    if fresh {
        return Ok((StatusCode::NOT_MODIFIED, cache).into_response());
    }
    let sort = query
        .get("sort")
        .map(|s| SortKey::parse(s).ok_or_else(|| AppError::bad_request(format!("unknown sort '{s}'"))))
        .transpose()?;
    let sort = sort.unwrap_or(SortKey::Title);
    let descending = match query.get("order").map(String::as_str) {
        Some("desc") => true,
        Some("asc") => false,
        _ => matches!(
            sort,
            SortKey::Added
                | SortKey::Rating
                | SortKey::Release
                | SortKey::LastPlayed
                | SortKey::UserRating
                | SortKey::Bitrate
        ),
    };
    let year = query.get("year").and_then(|y| y.parse().ok());
    // An unknown value is refused, not ignored: the listing would hold everything.
    let named = |key: &str, named: fn(&str) -> &'static [&'static str]| -> ApiResult<&'static [&'static str]> {
        match query.get(key).map(|v| v.trim()).filter(|v| !v.is_empty()) {
            None => Ok(&[]),
            Some(v) => match named(v) {
                [] => Err(AppError::bad_request(format!("unknown {key} '{v}'"))),
                found => Ok(found),
            },
        }
    };
    let resolutions = named("resolution", catalog::resolutions_named)?;
    let dynamic_ranges = named("dynamic_range", catalog::dynamic_ranges_named)?;
    let q = LibraryQuery {
        library_id: lib.id,
        kind: lib.kind().top_level_kind(),
        sort,
        descending,
        genre: query.get("genre").map(String::as_str).filter(|g| !g.is_empty()),
        year,
        watched: query.get("watched").map(|w| truthy(w)),
        resolutions,
        dynamic_ranges,
        offset: query.get("offset").and_then(|o| o.parse().ok()).unwrap_or(0),
        limit: query.get("limit").and_then(|l| l.parse().ok()),
    };
    let (rows, total) = catalog::library_items(&state.db, user.id, &q).await?;
    // Library grids show posters: the other pictures are left out.
    let summaries = wants_summaries(&query);
    let mut items: Vec<ItemSummary> = rows
        .iter()
        .map(|r| catalog::ItemSummary { backdrop_url: None, thumb_url: None, ..r.to_summary_with(summaries) })
        .collect();
    // The posters say what the files are like.
    let mut media = catalog::media_summaries(&state.db, &items, q.files()).await?;
    for item in &mut items {
        item.media = media.remove(&item.id);
    }
    Ok((cache, Json(ItemsPage { total, items })).into_response())
}

pub async fn filters(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let lib = visible_library(&state, &user, id).await?;
    let kind = lib.kind().top_level_kind();
    let ((genres, years), media) = tokio::try_join!(
        catalog::library_filters(&state.db, user.id, lib.id, kind),
        catalog::media_filters(&state.db, user.id, lib.id, kind),
    )?;
    Ok(Json(serde_json::json!({
        "genres": genres,
        "years": years,
        "resolutions": media.resolutions,
        "dynamic_ranges": media.dynamic_ranges,
    })))
}

#[derive(Serialize)]
pub struct Hub {
    pub key: String,
    pub title: String,
    pub library_id: Option<i64>,
    pub items: Vec<ItemSummary>,
}

#[derive(Serialize)]
pub struct Hubs {
    pub hubs: Vec<Hub>,
}

pub async fn hubs(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Json<Hubs>> {
    let lib = visible_library(&state, &user, id).await?;
    let language = i18n::for_user(&state.db, user.id, &state.settings.get().language, &headers).await?;
    let kind = lib.kind();
    let video = kind != LibraryKind::Music;
    let top = kind.top_level_kind();
    let db = &state.db;
    // The hubs don't depend on each other: fetched at the same time.
    let (continuing, added, released, rated) = tokio::try_join!(
        // Video-only hubs are empty for music.
        async { if video { catalog::continue_watching(db, user.id, Some(lib.id), 20).await } else { Ok(Vec::new()) } },
        catalog::recently_added(db, user.id, lib.id, kind.as_str(), 30),
        async { if video { catalog::recently_released(db, user.id, lib.id, top, 30).await } else { Ok(Vec::new()) } },
        async { if video { catalog::top_rated(db, user.id, lib.id, top, 30).await } else { Ok(Vec::new()) } },
    )?;
    let added_title = match kind {
        LibraryKind::Music => "hubs.recentlyAddedAlbums",
        LibraryKind::Shows => "hubs.recentlyAddedShows",
        LibraryKind::Movies => "hubs.recentlyAdded",
    };
    let hub = |key: &str, title: &str, rows: Vec<catalog::ItemRow>| Hub {
        key: format!("{key}:{id}"),
        title: i18n::t(language, title),
        library_id: Some(id),
        items: rows.iter().map(|r| r.to_summary()).collect(),
    };
    let mut hubs = vec![
        hub("continue_watching", "hubs.continueWatching", continuing),
        hub("recently_added", added_title, added),
        hub("recently_released", "hubs.recentlyReleased", released),
        hub("top_rated", "hubs.topRated", rated),
    ];
    hubs.retain(|h| !h.items.is_empty());
    Ok(Json(Hubs { hubs }))
}
