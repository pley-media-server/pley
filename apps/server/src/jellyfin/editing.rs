//! Editing metadata through this API, as Pley's own editor does
//! (`api/editing.rs`, [`crate::overrides`]): only with a token that may
//! manage libraries ([`super::JfManager`]).
//!
//! * `POST /Items/{id}` takes a `BaseItemDto`, as apps send back the one they
//!   were given with their changes: the fields Pley keeps that differ from
//!   what it has are edited and locked, as Pley's editor locks them, so
//!   refreshes keep them. `LockedFields` and `LockData` lock and unlock
//!   them. A change to something Pley doesn't keep (cast and crew, ratings,
//!   countries) answers `400`, and nothing of that request is applied.
//! * Pictures: TMDB's choices (`/RemoteImages`), one of them or another on a
//!   host Pley takes pictures from (`/RemoteImages/Download`), one uploaded
//!   (`POST /Images/{type}`, base64 as Jellyfin takes it), and back to the
//!   automatic one (`DELETE /Images/{type}`).
//! * Identify: TMDB's candidates (`/Items/RemoteSearch/Movie`, `/Series`)
//!   and Fix Match to one of them (`/Items/RemoteSearch/Apply/{id}`).
//! * Refreshing a title or a library (`/Items/{id}/Refresh`).

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use super::ids::Kind;
use super::{Body, Jellyfin, JfError, JfManager, JfResult, Q};
use crate::auth::AuthUser;
use crate::catalog::ItemRow;
use crate::events::ServerEvent;
use crate::state::AppState;
use crate::{collections, labels, overrides};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/items/{id}", post(update_item))
        .route("/items/{id}/metadataeditor", get(metadata_editor))
        .route("/items/{id}/externalidinfos", get(external_ids))
        .route("/items/{id}/refresh", post(refresh))
        .route("/items/{id}/remoteimages", get(remote_images))
        .route("/items/{id}/remoteimages/providers", get(image_providers))
        .route("/items/{id}/remoteimages/download", post(download_image))
        .route(
            "/items/{id}/images/{kind}",
            post(upload_image).delete(delete_image).layer(DefaultBodyLimit::max(UPLOAD_BODY_MAX)),
        )
        .route("/items/{id}/images/{kind}/{index}", post(upload_image_at).delete(delete_image_at))
        .route("/items/remotesearch/movie", post(search_movies))
        .route("/items/remotesearch/series", post(search_shows))
        .route("/items/remotesearch/apply/{id}", post(apply_match))
}

/// A picture of 25 MB, as base64.
const UPLOAD_BODY_MAX: usize = 34 * 1024 * 1024;

/// Jellyfin's lockable fields (`MetadataField`) and the fields Pley locks
/// for them.
const LOCKS: &[(&str, &str)] = &[
    ("Name", "title"),
    ("Overview", "summary"),
    ("Genres", "genres"),
    ("Studios", "studio"),
    ("OfficialRating", "content_rating"),
];

/// The fields Pley locks, pictures aside: `LockData` locks every one.
const KEPT: &[&str] = &[
    "title",
    "sort_title",
    "original_title",
    "year",
    "summary",
    "tagline",
    "studio",
    "content_rating",
    "release_date",
    "genres",
];

/// An item's locks as Jellyfin says them: `LockedFields`, the fields it
/// names that Pley keeps as edited, and `LockData`, whether every field
/// that holds something is kept so. With the sort title, when it is kept.
pub(super) async fn locks(state: &AppState, id: i64) -> JfResult<(Vec<&'static str>, bool, Option<String>)> {
    let locked = overrides::locked_fields(&state.db, id).await?;
    let current = fields_of(state, id).await?;
    let is_locked = |field: &str| locked.iter().any(|l| l == field);
    let names = LOCKS.iter().filter(|(_, field)| is_locked(field)).map(|(name, _)| *name).collect();
    let all = KEPT.iter().all(|field| is_locked(field) || current.value(field).is_none());
    let sort = is_locked("sort_title").then(|| current.sort_title.clone());
    Ok((names, all, sort))
}

/// An item's fields as Pley has them.
#[derive(Debug, sqlx::FromRow)]
struct Current {
    library_id: i64,
    kind: String,
    title: String,
    sort_title: String,
    original_title: Option<String>,
    year: Option<i64>,
    summary: Option<String>,
    tagline: Option<String>,
    studio: Option<String>,
    content_rating: Option<String>,
    release_date: Option<String>,
    genres: Option<String>,
}

impl Current {
    /// A field's value as overrides keep it.
    fn value(&self, field: &str) -> Option<String> {
        match field {
            "title" => Some(self.title.clone()),
            "sort_title" => Some(self.sort_title.clone()),
            "original_title" => self.original_title.clone(),
            "year" => self.year.map(|y| y.to_string()),
            "summary" => self.summary.clone(),
            "tagline" => self.tagline.clone(),
            "studio" => self.studio.clone(),
            "content_rating" => self.content_rating.clone(),
            "release_date" => self.release_date.clone(),
            "genres" => self.genres.clone(),
            _ => None,
        }
    }

    fn genres(&self) -> Vec<String> {
        self.genres.as_deref().and_then(|g| serde_json::from_str(g).ok()).unwrap_or_default()
    }
}

async fn fields_of(state: &AppState, id: i64) -> JfResult<Current> {
    Ok(sqlx::query_as(
        "SELECT library_id, kind, title, sort_title, original_title, year, summary, tagline, studio, content_rating,
                release_date, genres FROM items WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.db.read)
    .await?)
}

/// The item `id` as the caller may see it, and its fields.
async fn current(state: &AppState, user: &AuthUser, id: i64) -> JfResult<(ItemRow, Current)> {
    let row = crate::catalog::item(&state.db, user.id, id).await?.ok_or(JfError::NotFound)?;
    Ok((row, fields_of(state, id).await?))
}

fn refused(field: &str, why: &str) -> JfError {
    JfError::BadRequest(format!("`{field}` can't be edited: {why}"))
}

/// A string field the body sends: trimmed; `None` when it is left out or
/// null, as when an app doesn't send it.
fn text(body: &Body, name: &str) -> Option<String> {
    body.get(name).and_then(Value::as_str).map(|s| s.trim().to_owned())
}

/// Names in a list the body sends: of strings, or of `{"Name"}` objects.
fn names(body: &Body, name: &str) -> Option<Vec<String>> {
    let list = body.get(name)?.as_array()?;
    let names = list.iter().filter_map(|v| v.as_str().or_else(|| super::field(v, "Name").and_then(Value::as_str)));
    Some(names.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect())
}

/// What one `POST /Items/{id}` changes in an item.
#[derive(Debug, Default)]
struct Edit {
    /// Pley's fields and their new values.
    values: Vec<(&'static str, String)>,
    labels: Option<Vec<String>>,
    /// Locks as `LockedFields` sends them, by Pley's fields.
    locked: Option<BTreeSet<&'static str>>,
    lock_all: bool,
}

impl Edit {
    /// The fields of `body` that differ from `current`, refused whole when one
    /// can't be applied.
    fn of(body: &Body, current: &Current) -> JfResult<Edit> {
        let mut edit = Edit::default();
        let mut set = |field: &'static str, value: String, now: Option<String>| {
            if now.as_deref().unwrap_or_default() != value {
                edit.values.push((field, value));
            }
        };
        if let Some(name) = text(body, "Name") {
            set("title", name, Some(current.title.clone()));
        }
        if let Some(sort) = text(body, "ForcedSortName").filter(|s| !s.is_empty()) {
            set("sort_title", sort, Some(current.sort_title.clone()));
        }
        // Left out of the item's answer when it is the title.
        if let Some(original) = text(body, "OriginalTitle").filter(|s| !s.is_empty()) {
            set("original_title", original, current.original_title.clone().or(Some(current.title.clone())));
        }
        if let Some(year) = body.number("ProductionYear") {
            set("year", year.to_string(), current.year.map(|y| y.to_string()));
        }
        if let Some(date) = text(body, "PremiereDate").filter(|d| !d.is_empty()) {
            let day: String = date.chars().take(10).collect();
            set("release_date", day, current.release_date.clone());
        }
        if let Some(overview) = text(body, "Overview") {
            set("summary", overview, current.summary.clone());
        }
        if let Some(taglines) = names(body, "Taglines") {
            set("tagline", taglines.into_iter().next().unwrap_or_default(), current.tagline.clone());
        }
        if let Some(studios) = names(body, "Studios") {
            set("studio", studios.into_iter().next().unwrap_or_default(), current.studio.clone());
        }
        if let Some(rating) = text(body, "OfficialRating") {
            set("content_rating", rating, current.content_rating.clone());
        }
        if let Some(genres) = names(body, "Genres")
            && genres != current.genres()
        {
            let json = serde_json::to_string(&genres).map_err(|e| JfError::Internal(e.into()))?;
            edit.values.push(("genres", json));
        }
        edit.labels = names(body, "Tags");
        if let Some(list) = body.get("LockedFields").and_then(Value::as_array) {
            let mut locked = BTreeSet::new();
            for name in list.iter().filter_map(Value::as_str) {
                match LOCKS.iter().find(|(jellyfin, _)| jellyfin.eq_ignore_ascii_case(name)) {
                    Some((_, field)) => {
                        locked.insert(*field);
                    }
                    // Pley's labels are never refreshed: always kept.
                    None if name.eq_ignore_ascii_case("Tags") => {}
                    None => return Err(refused(name, "Pley doesn't keep it, so it has nothing to lock")),
                }
            }
            edit.locked = Some(locked);
        }
        edit.lock_all = body.flag("LockData") == Some(true);
        Ok(edit)
    }
}

/// The fields Pley doesn't keep, refused when the body changes them from
/// what the item's answer says: cast and crew, ratings, countries.
fn refuse_what_pley_doesnt_keep(body: &Body, given: &Value) -> JfResult<()> {
    let people = |value: Option<&Value>| -> BTreeSet<(String, String)> {
        let list = value.and_then(Value::as_array).cloned().unwrap_or_default();
        list.iter()
            .filter_map(|p| {
                let name = super::field(p, "Name")?.as_str()?.trim().to_owned();
                let kind = super::field(p, "Type").and_then(Value::as_str).unwrap_or_default().to_owned();
                Some((name, kind))
            })
            .collect()
    };
    if body.get("People").is_some() && people(body.get("People")) != people(super::field(given, "People")) {
        return Err(refused("People", "Pley takes cast and crew from the metadata source"));
    }
    if let Some(sent) = body.get("CommunityRating").and_then(Value::as_f64) {
        let has = super::field(given, "CommunityRating").and_then(Value::as_f64);
        if has.is_none_or(|has| (has - sent).abs() > 0.05) {
            return Err(refused("CommunityRating", "Pley shows the metadata source's rating"));
        }
    }
    if body.get("CriticRating").is_some() {
        return Err(refused("CriticRating", "Pley keeps no critics' rating"));
    }
    if names(body, "ProductionLocations").is_some_and(|places| !places.is_empty()) {
        return Err(refused("ProductionLocations", "Pley keeps no countries"));
    }
    Ok(())
}

/// `POST /Items/{id}` with a `BaseItemDto`: a title's fields, or a
/// collection's name, overview and sort name. `204`, as Jellyfin answers.
async fn update_item(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
    body: Body,
) -> JfResult<StatusCode> {
    if !body.0.is_object() {
        return Err(JfError::BadRequest("send the item as a BaseItemDto, in JSON".into()));
    }
    match jellyfin.ids.read(&id) {
        Some((Kind::Item, id)) => edit_item(&state, &jellyfin, &user, id, &body).await,
        Some((Kind::Collection, id)) => edit_collection(&state, &user, id, &body).await,
        Some((Kind::Playlist, _)) => Err(JfError::BadRequest("a playlist is renamed at /Playlists/{id}".into())),
        _ => Err(JfError::NotFound),
    }
}

async fn edit_item(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: i64,
    body: &Body,
) -> JfResult<StatusCode> {
    let (row, current) = current(state, user, id).await?;
    let given = super::items::dtos(state, jellyfin, user, std::slice::from_ref(&row), super::dto::Fields::all())
        .await?
        .pop()
        .map(|item| serde_json::to_value(item).unwrap_or_default())
        .unwrap_or_default();
    refuse_what_pley_doesnt_keep(body, &given)?;
    let edit = Edit::of(body, &current)?;
    for (field, value) in &edit.values {
        overrides::validate(field, Some(value))?;
    }
    let was_locked = overrides::locked_fields(&state.db, id).await?;
    let mut changes: Vec<(String, Option<String>)> =
        edit.values.iter().map(|(field, value)| ((*field).to_owned(), Some(value.clone()))).collect();
    let edited = |field: &str| edit.values.iter().any(|(f, _)| *f == field);
    for field in KEPT.iter().copied().filter(|field| !edited(field)) {
        let asked = edit.lock_all || edit.locked.as_ref().is_some_and(|locked| locked.contains(field));
        let locked_now = was_locked.iter().any(|f| f == field);
        // Only the fields `LockedFields` can name are unlocked by leaving them out.
        let can_say = LOCKS.iter().any(|(_, f)| *f == field) && edit.locked.is_some();
        if asked && !locked_now {
            if let Some(value) = current.value(field) {
                changes.push((field.to_owned(), Some(value)));
            }
        } else if !asked && locked_now && can_say {
            changes.push((field.to_owned(), None));
        }
    }
    if !changes.is_empty() {
        overrides::set(&state.db, id, &changes).await?;
    }
    if let Some(wanted) = &edit.labels {
        let has: Vec<String> = labels::for_items(&state.db, &[id]).await?.remove(&id).unwrap_or_default();
        let add: Vec<String> =
            wanted.iter().filter(|l| !has.iter().any(|h| h.eq_ignore_ascii_case(l))).cloned().collect();
        let remove: Vec<String> =
            has.iter().filter(|h| !wanted.iter().any(|l| l.eq_ignore_ascii_case(h))).cloned().collect();
        if !add.is_empty() || !remove.is_empty() {
            labels::edit(&state.db, id, &add, &remove).await?;
        }
    }
    if changes.iter().any(|(_, value)| value.is_none()) && state.metadata.can_refresh(&current.kind) {
        crate::api::editing::refresh_later(state, id, current.library_id);
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: current.library_id });
    Ok(StatusCode::NO_CONTENT)
}

/// A collection's name, overview and sort name, and its labels (`Tags`):
/// Pley keeps every edit of a collection.
async fn edit_collection(state: &AppState, user: &AuthUser, id: i64, body: &Body) -> JfResult<StatusCode> {
    let (collection, _) = collections::get(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
    let name = text(body, "Name").filter(|n| !n.is_empty() && *n != collection.title);
    let name: Option<String> = name.map(|n| n.chars().take(200).collect());
    let overview = text(body, "Overview").filter(|o| Some(o.as_str()) != collection.summary.as_deref());
    let summary = overview.as_ref().map(|o| Some(o.as_str()).filter(|o| !o.is_empty()));
    collections::update(&state.db, id, name.as_deref(), summary, None).await?;
    if let Some(sort) = text(body, "ForcedSortName").filter(|s| !s.is_empty()) {
        let sort: String = sort.chars().take(200).collect();
        sqlx::query("UPDATE collections SET sort_title = ?, edited = 1 WHERE id = ?")
            .bind(sort)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(wanted) = names(body, "Tags") {
        let has: Vec<String> = labels::for_collections(&state.db, &[id]).await?.remove(&id).unwrap_or_default();
        let add: Vec<String> =
            wanted.iter().filter(|l| !has.iter().any(|h| h.eq_ignore_ascii_case(l))).cloned().collect();
        let remove: Vec<String> =
            has.iter().filter(|h| !wanted.iter().any(|l| l.eq_ignore_ascii_case(h))).cloned().collect();
        labels::edit_collection(&state.db, id, &add, &remove).await?;
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: collection.library_id });
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MetadataEditorInfo {
    parental_rating_options: [(); 0],
    countries: [(); 0],
    cultures: [(); 0],
    external_id_infos: [(); 0],
    content_type_options: [(); 0],
}

/// `GET /Items/{id}/MetadataEditor`: what an editor offers to pick from,
/// nothing beyond the fields themselves.
async fn metadata_editor(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
) -> JfResult<Json<MetadataEditorInfo>> {
    let id = jellyfin.ids.number(Kind::Item, &id).ok_or(JfError::NotFound)?;
    current(&state, &user, id).await?;
    Ok(Json(MetadataEditorInfo {
        parental_rating_options: [],
        countries: [],
        cultures: [],
        external_id_infos: [],
        content_type_options: [],
    }))
}

/// `GET /Items/{id}/ExternalIdInfos`: none to edit; ids change by Fix Match.
async fn external_ids(_user: JfManager) -> Json<[(); 0]> {
    Json([])
}

/// `POST /Items/{id}/Refresh`: a title's metadata fetched again, or a
/// library scanned. `204`; the work goes on after the answer.
async fn refresh(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
) -> JfResult<StatusCode> {
    match jellyfin.ids.read(&id) {
        Some((Kind::Item, id)) => {
            let (_, current) = current(&state, &user, id).await?;
            if !state.metadata.can_refresh(&current.kind) {
                return Err(JfError::BadRequest("this title's metadata source is off or has no key".into()));
            }
            crate::api::editing::refresh_later(&state, id, current.library_id);
        }
        Some((Kind::Library, id)) => {
            let libraries = crate::access::visible_libraries(&state.db, &user).await?;
            if !libraries.iter().any(|lib| lib.id == id) {
                return Err(JfError::NotFound);
            }
            state.scanner.enqueue(id, false);
        }
        _ => return Err(JfError::NotFound),
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Pley's picture kind for Jellyfin's image type of an item: a poster is
/// `Primary` (an episode's still), a backdrop `Backdrop`, a frame `Thumb`.
fn picture_kind(row: &ItemRow, image: &str) -> JfResult<&'static str> {
    super::items::artwork_kind(&row.kind, &image.to_ascii_lowercase())
        .ok_or_else(|| JfError::BadRequest(format!("Pley keeps no {image} pictures")))
}

/// The item `id` whose pictures are changed: a title. A playlist shows its
/// first entry's picture, and collections' are set through the
/// Plex-compatible API.
async fn picture_item(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, id: &str) -> JfResult<ItemRow> {
    match jellyfin.ids.read(id) {
        Some((Kind::Item, id)) => Ok(current(state, user, id).await?.0),
        Some((Kind::Collection | Kind::Playlist, _)) => {
            Err(JfError::BadRequest("only titles' pictures are set here".into()))
        }
        _ => Err(JfError::NotFound),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteImageInfo {
    provider_name: &'static str,
    url: String,
    thumbnail_url: String,
    height: i64,
    width: i64,
    community_rating: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(rename = "Type")]
    kind: &'static str,
    rating_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteImageResult {
    images: Vec<RemoteImageInfo>,
    total_record_count: usize,
    providers: Vec<&'static str>,
}

const TMDB: &str = "TheMovieDb";

/// `GET /Items/{id}/RemoteImages?type=`: TMDB's pictures of that type (else
/// posters and backdrops), to choose from. None without a TMDB key.
async fn remote_images(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<Json<RemoteImageResult>> {
    let row = picture_item(&state, &jellyfin, &user, &id).await?;
    if !state.metadata.tmdb_enabled() {
        return Ok(Json(RemoteImageResult { images: Vec::new(), total_record_count: 0, providers: Vec::new() }));
    }
    let types: Vec<String> = match q.get("type") {
        Some(kind) => vec![kind.to_owned()],
        None => vec!["Primary".into(), "Backdrop".into()],
    };
    let mut images = Vec::new();
    for image in &types {
        let kind = picture_kind(&row, image)?;
        let jellyfin_type = match image.to_ascii_lowercase().as_str() {
            "backdrop" => "Backdrop",
            "thumb" => "Thumb",
            _ => "Primary",
        };
        let found =
            state.metadata.artwork_candidates(row.id, kind).await.map_err(|e| JfError::BadRequest(e.to_string()))?;
        images.extend(found.into_iter().map(|c| RemoteImageInfo {
            provider_name: TMDB,
            url: c.url,
            thumbnail_url: c.preview_url,
            height: c.height,
            width: c.width,
            community_rating: c.votes,
            language: c.language,
            kind: jellyfin_type,
            rating_type: "Score",
        }));
    }
    let start = q.number("startIndex").unwrap_or(0).max(0) as usize;
    let limit = q.number("limit").map_or(usize::MAX, |l| l.max(0) as usize);
    let total = images.len();
    let images = images.into_iter().skip(start).take(limit).collect();
    Ok(Json(RemoteImageResult { images, total_record_count: total, providers: vec![TMDB] }))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ImageProviderInfo {
    name: &'static str,
    supported_images: [&'static str; 3],
}

/// `GET /Items/{id}/RemoteImages/Providers`: TMDB, while it has a key.
async fn image_providers(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
) -> JfResult<Json<Vec<ImageProviderInfo>>> {
    picture_item(&state, &jellyfin, &user, &id).await?;
    let providers = state
        .metadata
        .tmdb_enabled()
        .then_some(ImageProviderInfo { name: TMDB, supported_images: ["Primary", "Backdrop", "Thumb"] });
    Ok(Json(providers.into_iter().collect()))
}

/// Sets picture `kind` of `row` to the stored picture `blob`, kept through
/// refreshes, as Pley's editor does.
async fn set_picture(state: &AppState, row: &ItemRow, kind: &str, blob: String) -> JfResult<StatusCode> {
    overrides::set(&state.db, row.id, &[(kind.to_owned(), Some(blob))]).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Items/{id}/RemoteImages/Download?type=&imageUrl=`: that picture,
/// on TMDB or another host Pley takes pictures from (others answer `400`,
/// so the server is never sent into its own network).
async fn download_image(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<StatusCode> {
    let row = picture_item(&state, &jellyfin, &user, &id).await?;
    let kind = picture_kind(&row, q.get("type").unwrap_or("Primary"))?;
    let url = q.get("imageUrl").ok_or_else(|| JfError::BadRequest("name the picture (imageUrl)".into()))?;
    let bytes = state.metadata.download_artwork(url).await.map_err(|e| JfError::BadRequest(e.to_string()))?;
    let blob = crate::api::editing::store_picture(&state, bytes.into()).await?;
    set_picture(&state, &row, kind, blob).await
}

/// A picture as apps upload it: base64, as Jellyfin takes it, or the file's
/// own bytes.
fn picture_bytes(body: &[u8]) -> Bytes {
    let text: Vec<u8> = body.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
    match base64::engine::general_purpose::STANDARD.decode(&text) {
        Ok(decoded) => Bytes::from(decoded),
        Err(_) => Bytes::copy_from_slice(body),
    }
}

/// `POST /Items/{id}/Images/{type}`: the picture in the body, re-encoded
/// as Pley stores uploads (25 MB at most).
async fn upload_image(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path((id, image)): Path<(String, String)>,
    body: Bytes,
) -> JfResult<StatusCode> {
    let row = picture_item(&state, &jellyfin, &user, &id).await?;
    let kind = picture_kind(&row, &image)?;
    let blob = crate::api::editing::store_picture(&state, picture_bytes(&body)).await?;
    set_picture(&state, &row, kind, blob).await
}

/// `POST /Items/{id}/Images/{type}/{index}`: Pley keeps one of each, the
/// first.
async fn upload_image_at(
    state: State<AppState>,
    jellyfin: Extension<Arc<Jellyfin>>,
    user: JfManager,
    Path((id, image, index)): Path<(String, String, String)>,
    body: Bytes,
) -> JfResult<StatusCode> {
    if index != "0" {
        return Err(JfError::BadRequest("Pley keeps one picture of each type".into()));
    }
    upload_image(state, jellyfin, user, Path((id, image)), body).await
}

/// `DELETE /Items/{id}/Images/{type}`: back to the automatic picture (the
/// file's own, or the metadata source's).
async fn delete_image(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path((id, image)): Path<(String, String)>,
) -> JfResult<StatusCode> {
    let row = picture_item(&state, &jellyfin, &user, &id).await?;
    let kind = picture_kind(&row, &image)?;
    crate::api::editing::reset_picture(&state, row.id, &row.kind, row.library_id, kind).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_image_at(
    state: State<AppState>,
    jellyfin: Extension<Arc<Jellyfin>>,
    user: JfManager,
    Path((id, image, index)): Path<(String, String, String)>,
) -> JfResult<StatusCode> {
    if index != "0" {
        return Err(JfError::NotFound);
    }
    delete_image(state, jellyfin, user, Path((id, image))).await
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteSearchResult {
    name: String,
    provider_ids: std::collections::BTreeMap<&'static str, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overview: Option<String>,
    search_provider_name: &'static str,
    artists: [(); 0],
}

/// `POST /Items/RemoteSearch/Movie` and `/Series` (`{"SearchInfo": {"Name",
/// "Year"}, "ItemId"}`): TMDB's candidates, for Fix Match.
async fn search(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    body: &Body,
    tv: bool,
) -> JfResult<Json<Vec<RemoteSearchResult>>> {
    if !state.metadata.tmdb_enabled() {
        return Err(JfError::BadRequest("add a TMDB API key in Settings → Server to search for matches".into()));
    }
    let info = body.get("SearchInfo").cloned().unwrap_or_default();
    let item = match body.str("ItemId").and_then(|id| jellyfin.ids.number(Kind::Item, id)) {
        Some(id) => Some(current(state, user, id).await?.1),
        None => None,
    };
    let name = super::field(&info, "Name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty());
    let title = name.map(str::to_owned).or_else(|| item.as_ref().map(|i| i.title.clone()));
    let title = title.ok_or_else(|| JfError::BadRequest("name the title to look for (SearchInfo.Name)".into()))?;
    let year = super::field(&info, "Year")
        .and_then(Value::as_i64)
        .or_else(|| item.as_ref().filter(|i| i.title.trim().eq_ignore_ascii_case(title.trim())).and_then(|i| i.year));
    let found = state.metadata.search_matches(tv, title.trim(), year).await.map_err(JfError::Internal)?;
    Ok(Json(
        found
            .into_iter()
            .map(|c| RemoteSearchResult {
                name: c.title,
                provider_ids: [("Tmdb", c.tmdb_id.to_string())].into_iter().collect(),
                production_year: c.year,
                image_url: c.poster_url,
                overview: c.overview,
                search_provider_name: TMDB,
                artists: [],
            })
            .collect(),
    ))
}

async fn search_movies(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    body: Body,
) -> JfResult<Json<Vec<RemoteSearchResult>>> {
    search(&state, &jellyfin, &user, &body, false).await
}

async fn search_shows(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    body: Body,
) -> JfResult<Json<Vec<RemoteSearchResult>>> {
    search(&state, &jellyfin, &user, &body, true).await
}

/// `POST /Items/RemoteSearch/Apply/{id}` with one of the candidates: Fix
/// Match to its TMDB id, and the title refreshed from it.
async fn apply_match(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
    body: Body,
) -> JfResult<StatusCode> {
    let id = jellyfin.ids.number(Kind::Item, &id).ok_or(JfError::NotFound)?;
    let (_, current) = current(&state, &user, id).await?;
    if !matches!(current.kind.as_str(), "movie" | "show") {
        return Err(JfError::BadRequest("only movies and shows are matched".into()));
    }
    let ids = body.get("ProviderIds").cloned().unwrap_or_default();
    let tmdb = super::field(&ids, "Tmdb")
        .and_then(|v| v.as_str().map(str::to_owned).or_else(|| v.as_i64().map(|n| n.to_string())))
        .and_then(|v| v.trim().parse::<i64>().ok())
        .ok_or_else(|| JfError::BadRequest("Pley matches by TMDB id (ProviderIds.Tmdb)".into()))?;
    state.metadata.rematch(id, tmdb).await.map_err(JfError::Internal)?;
    state.events.send(ServerEvent::LibraryChanged { library_id: current.library_id });
    Ok(StatusCode::NO_CONTENT)
}
