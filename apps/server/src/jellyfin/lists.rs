//! Making and changing playlists and collections from Jellyfin's apps, as
//! Pley's own ([`crate::playlists`], [`crate::collections`]) with their
//! rules. A playlist is its owner's alone: every token changes its own
//! account's, and nobody else's (`404`). Collections are a library's, shared
//! by everyone: only a token that may manage libraries (`manage`, on an
//! administrator's account) changes them, as on the Plex-compatible API.
//!
//! Playlist entries have ids of their own (`PlaylistItemId`, keyed as other
//! ids are), since an item may be in a playlist more than once.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;

use super::ids::Kind;
use super::{Body, Jellyfin, JfError, JfManager, JfResult, JfUser, Q};
use crate::auth::AuthUser;
use crate::events::ServerEvent;
use crate::state::AppState;
use crate::{collections, playlists};

/// Routes of their own; `/Playlists/{id}/Items` and `/Items/{id}`, which
/// browsing answers too, take theirs in [`super::items`].
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/playlists", post(create_playlist))
        .route("/playlists/{id}", get(playlist).post(update_playlist))
        .route("/playlists/{id}/items/{entry}/move/{index}", post(move_entry))
        .route("/playlists/{id}/users", get(playlist_users))
        .route("/playlists/{id}/users/{user}", get(playlist_user))
        .route("/collections", post(create_collection))
        .route("/collections/{id}/items", post(add_to_collection).delete(remove_from_collection))
}

/// The most ids one request names: a playlist holds 5,000 entries.
const MAX_IDS: usize = 5_000;
/// The longest name kept, as Pley's own API keeps them.
const MAX_NAME_CHARS: usize = 200;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Created {
    id: String,
}

/// A name as sent: trimmed, 200 characters at most, never empty.
fn name_of(name: Option<&str>, what: &str) -> JfResult<String> {
    let name: String = name.unwrap_or_default().trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        return Err(JfError::BadRequest(format!("{what} needs a name")));
    }
    Ok(name)
}

/// Ids in a body's list (`["…", "…"]`, or one string of them separated by
/// commas) or the query's, of `kind`: those that aren't are left out, as
/// ids of things the caller may not see are.
fn ids_of(jellyfin: &Jellyfin, kind: Kind, body: &Body, q: &Q, name: &str) -> JfResult<Vec<i64>> {
    let mut texts: Vec<String> = match body.get(name) {
        Some(serde_json::Value::Array(list)) => list.iter().filter_map(|v| v.as_str()).map(str::to_owned).collect(),
        Some(serde_json::Value::String(text)) => text.split(',').map(str::trim).map(str::to_owned).collect(),
        _ => Vec::new(),
    };
    texts.extend(q.list(name));
    if texts.len() > MAX_IDS {
        return Err(JfError::BadRequest(format!("{MAX_IDS} ids at most")));
    }
    Ok(texts.iter().filter_map(|id| jellyfin.ids.number(kind, id)).collect())
}

/// A `userId` sent with a change must be the caller's own: nothing here
/// acts for another account.
fn for_the_caller(jellyfin: &Jellyfin, user: &AuthUser, body: &Body, q: &Q) -> JfResult<()> {
    match body.str("UserId").or(q.get("userId")) {
        Some(id) if jellyfin.ids.number(Kind::User, id) != Some(user.id) => Err(JfError::Forbidden),
        _ => Ok(()),
    }
}

/// Pley's playlists are their owner's alone: making one public, or sharing
/// it, is refused rather than left undone.
fn not_shared(jellyfin: &Jellyfin, user: &AuthUser, body: &Body) -> JfResult<()> {
    let shared = body.flag("IsPublic") == Some(true)
        || body.get("Users").and_then(|v| v.as_array()).is_some_and(|users| {
            users.iter().any(|share| {
                super::field(share, "UserId")
                    .and_then(|id| id.as_str())
                    .and_then(|id| jellyfin.ids.number(Kind::User, id))
                    != Some(user.id)
            })
        });
    if shared {
        return Err(JfError::BadRequest("Pley's playlists are their owner's alone: they can't be shared".into()));
    }
    Ok(())
}

/// The caller's playlist `id`, its entries as they see them.
async fn own_playlist(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: &str,
) -> JfResult<(i64, playlists::PlaylistDto, Vec<(i64, crate::catalog::ItemRow)>)> {
    let id = jellyfin.ids.number(Kind::Playlist, id).ok_or(JfError::NotFound)?;
    let (playlist, entries) = playlists::get_rows(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
    Ok((id, playlist, entries))
}

/// A smart playlist is filled by its filter.
fn not_smart(playlist: &playlists::PlaylistDto) -> JfResult<()> {
    if playlist.smart_filter.is_some() {
        return Err(JfError::BadRequest("a smart playlist is filled by its filter".into()));
    }
    Ok(())
}

/// `POST /Playlists` (`CreatePlaylistDto` as the body, or its fields in the
/// query, as older apps send them): a playlist of the caller's, for the
/// `MediaType` asked for (`Audio` or `Video`), else that of its first item,
/// else video. Its id, as `PlaylistCreationResult`.
async fn create_playlist(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
    body: Body,
) -> JfResult<Json<Created>> {
    for_the_caller(&jellyfin, &user, &body, &q)?;
    not_shared(&jellyfin, &user, &body)?;
    let title = name_of(body.str("Name").or(q.get("name")), "a playlist")?;
    let items = ids_of(&jellyfin, Kind::Item, &body, &q, "ids")?;
    let kind = match body.str("MediaType").or(q.get("mediaType")).map(str::to_ascii_lowercase).as_deref() {
        Some("audio") => playlists::Kind::Audio,
        Some("video") => playlists::Kind::Video,
        _ => playlists::kind_of_first(&state.db, &user, &items).await?.unwrap_or(playlists::Kind::Video),
    };
    let id = playlists::create(&state.db, &user, &title, kind).await?;
    if !items.is_empty()
        && let Err(err) = playlists::add(&state.db, &user, id, &items).await
    {
        playlists::delete(&state.db, &user, id).await?;
        return Err(err.into());
    }
    Ok(Json(Created { id: jellyfin.ids.of(Kind::Playlist, id) }))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Share {
    user_id: String,
    can_edit: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaylistDto {
    open_access: bool,
    shares: Vec<Share>,
    item_ids: Vec<String>,
}

/// `GET /Playlists/{id}`: the caller's playlist, shared with no one, and
/// the items it holds that they may see, in its order.
async fn playlist(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Json<PlaylistDto>> {
    let (_, _, entries) = own_playlist(&state, &jellyfin, &user, &id).await?;
    let item_ids = entries.iter().map(|(_, row)| jellyfin.ids.of(Kind::Item, row.id)).collect();
    Ok(Json(PlaylistDto { open_access: false, shares: Vec::new(), item_ids }))
}

/// `POST /Playlists/{id}` (`UpdatePlaylistDto`): a new `Name`, and `Ids`,
/// the items it holds from now on, in that order (what apps that edit a
/// playlist as a whole send). Entries the caller can't see are kept.
async fn update_playlist(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    body: Body,
) -> JfResult<StatusCode> {
    let (id, playlist, _) = own_playlist(&state, &jellyfin, &user, &id).await?;
    not_shared(&jellyfin, &user, &body)?;
    let title = body.get("Name").map(|_| name_of(body.str("Name"), "a playlist")).transpose()?;
    if body.get("Ids").is_some() {
        not_smart(&playlist)?;
        let items = ids_of(&jellyfin, Kind::Item, &body, &Q::default(), "Ids")?;
        playlists::replace(&state.db, &user, id, &items).await?;
    }
    if let Some(title) = title {
        playlists::rename(&state.db, &user, id, &title).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Playlists/{id}/Items?ids=`: the items at the end, or from
/// `position` on (the entries the caller sees, counted from 0).
pub(super) async fn add_to_playlist(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<StatusCode> {
    let none = Body(serde_json::Value::Null);
    for_the_caller(&jellyfin, &user, &none, &q)?;
    let (id, playlist, before) = own_playlist(&state, &jellyfin, &user, &id).await?;
    not_smart(&playlist)?;
    let items = ids_of(&jellyfin, Kind::Item, &none, &q, "ids")?;
    playlists::add(&state.db, &user, id, &items).await?;
    if let Some(position) = q.number("position").filter(|&p| p >= 0) {
        let before: Vec<i64> = before.iter().map(|(entry, _)| *entry).collect();
        let (_, after) = playlists::get_rows(&state.db, &user, id).await?.ok_or(JfError::NotFound)?;
        let added = after.iter().map(|(entry, _)| *entry).filter(|entry| !before.contains(entry));
        let at = usize::try_from(position).unwrap_or(usize::MAX).min(before.len());
        let order: Vec<i64> = before[..at].iter().copied().chain(added).chain(before[at..].iter().copied()).collect();
        playlists::reorder(&state.db, &user, id, &order).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The entry of `entries` an id names: by its `PlaylistItemId`, or by its
/// item's id, the first entry of that item (some apps send the item's when
/// they have no entry's).
fn entry_of(jellyfin: &Jellyfin, entries: &[(i64, crate::catalog::ItemRow)], id: &str) -> Option<i64> {
    match jellyfin.ids.read(id)? {
        (Kind::PlaylistEntry, entry) => entries.iter().any(|(e, _)| *e == entry).then_some(entry),
        (Kind::Item, item) => entries.iter().find(|(_, row)| row.id == item).map(|(e, _)| *e),
        _ => None,
    }
}

/// `DELETE /Playlists/{id}/Items?entryIds=`: those entries go.
pub(super) async fn remove_from_playlist(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<StatusCode> {
    let (id, playlist, mut entries) = own_playlist(&state, &jellyfin, &user, &id).await?;
    not_smart(&playlist)?;
    let asked = q.list("entryIds");
    if asked.len() > MAX_IDS {
        return Err(JfError::BadRequest(format!("{MAX_IDS} ids at most")));
    }
    for text in asked {
        let Some(entry) = entry_of(&jellyfin, &entries, &text) else { continue };
        playlists::remove_entry(&state.db, &user, id, entry).await?;
        entries.retain(|(e, _)| *e != entry);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Playlists/{id}/Items/{entry}/Move/{index}`: the entry to that
/// place among those the caller sees, counted from 0.
async fn move_entry(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((id, entry, index)): Path<(String, String, String)>,
) -> JfResult<StatusCode> {
    let (id, playlist, entries) = own_playlist(&state, &jellyfin, &user, &id).await?;
    not_smart(&playlist)?;
    let entry = entry_of(&jellyfin, &entries, &entry).ok_or(JfError::NotFound)?;
    let index: usize = index.parse().map_err(|_| JfError::BadRequest("the new index is a number".into()))?;
    let others: Vec<i64> = entries.iter().map(|(e, _)| *e).filter(|e| *e != entry).collect();
    let after = index.min(others.len()).checked_sub(1).map(|at| others[at]);
    if !playlists::move_entry(&state.db, &user, id, entry, after).await? {
        return Err(JfError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /Playlists/{id}/Users`: whom the playlist is shared with, no one.
async fn playlist_users(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Json<Vec<Share>>> {
    own_playlist(&state, &jellyfin, &user, &id).await?;
    Ok(Json(Vec::new()))
}

/// `GET /Playlists/{id}/Users/{user}`: what the caller may do with their
/// own playlist, everything. Nobody else is known here (`404`).
async fn playlist_user(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((id, user_id)): Path<(String, String)>,
) -> JfResult<Json<Share>> {
    own_playlist(&state, &jellyfin, &user, &id).await?;
    if jellyfin.ids.number(Kind::User, &user_id) != Some(user.id) {
        return Err(JfError::NotFound);
    }
    Ok(Json(Share { user_id: jellyfin.ids.of(Kind::User, user.id), can_edit: true }))
}

/// `DELETE /Items/{id}`: the caller's playlist, or with `manage` a
/// collection. Titles stay: Pley deletes no media through an app (`403`).
pub(super) async fn delete_item(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<StatusCode> {
    match jellyfin.ids.read(&id) {
        Some((Kind::Playlist, id)) => {
            if !playlists::delete(&state.db, &user, id).await? {
                return Err(JfError::NotFound);
            }
        }
        Some((Kind::Collection, id)) => {
            let manager = super::manager(user)?;
            let library_id = collection_library(&state, &manager, id).await?;
            collections::delete(&state.db, id).await?;
            state.events.send(ServerEvent::LibraryChanged { library_id });
        }
        Some((Kind::Item, _)) => return Err(JfError::Forbidden),
        _ => return Err(JfError::NotFound),
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The library of collection `id`, one the caller may open.
async fn collection_library(state: &AppState, user: &AuthUser, id: i64) -> JfResult<i64> {
    let library_id = collections::library_of(&state.db, id).await?.ok_or(JfError::NotFound)?;
    let libraries = crate::access::visible_libraries(&state.db, user).await?;
    if !libraries.iter().any(|lib| lib.id == library_id) {
        return Err(JfError::NotFound);
    }
    Ok(library_id)
}

/// Smart collections fill themselves.
async fn not_smart_collection(state: &AppState, id: i64) -> JfResult<()> {
    if collections::is_smart(&state.db, id).await? {
        return Err(JfError::BadRequest("a smart collection is filled by its filter".into()));
    }
    Ok(())
}

const ONLY_MOVIES_AND_SHOWS: &str = "only movies and shows of the collection's library go in it";

/// `POST /Collections?name=&ids=&parentId=`: a collection of those movies
/// and shows, in the library `parentId` names, else that of its first title
/// the caller may see. Its id, as `CollectionCreationResult`.
async fn create_collection(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    q: Q,
) -> JfResult<Json<Created>> {
    let title = name_of(q.get("name"), "a collection")?;
    let none = Body(serde_json::Value::Null);
    let items = ids_of(&jellyfin, Kind::Item, &none, &q, "ids")?;
    let libraries = crate::access::visible_libraries(&state.db, &user).await?;
    let library = match q.get("parentId").map(|id| jellyfin.ids.read(id)) {
        Some(Some((Kind::Library, id))) => libraries.iter().find(|lib| lib.id == id),
        Some(_) => None,
        None => {
            let first = crate::catalog::items_by_ids(&state.db, user.id, &items).await?.into_iter().next();
            first.and_then(|item| libraries.iter().find(|lib| lib.id == item.library_id))
        }
    };
    let library = library.ok_or_else(|| {
        JfError::BadRequest("say which library the collection is in (parentId), or start it with a title".into())
    })?;
    if library.kind() == crate::library::LibraryKind::Music {
        return Err(JfError::BadRequest("collections are for movie and TV libraries".into()));
    }
    let id = collections::create(&state.db, library.id, &title, None).await?;
    if !items.is_empty() && collections::add_items(&state.db, id, library.id, &items).await? == 0 {
        collections::delete(&state.db, id).await?;
        return Err(JfError::BadRequest(ONLY_MOVIES_AND_SHOWS.into()));
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: library.id });
    Ok(Json(Created { id: jellyfin.ids.of(Kind::Collection, id) }))
}

/// `POST /Collections/{id}/Items?ids=`: those movies and shows of its
/// library join it.
async fn add_to_collection(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<StatusCode> {
    let id = jellyfin.ids.number(Kind::Collection, &id).ok_or(JfError::NotFound)?;
    let library_id = collection_library(&state, &user, id).await?;
    not_smart_collection(&state, id).await?;
    let none = Body(serde_json::Value::Null);
    let items = ids_of(&jellyfin, Kind::Item, &none, &q, "ids")?;
    if !items.is_empty() && collections::add_items(&state.db, id, library_id, &items).await? == 0 {
        return Err(JfError::BadRequest(ONLY_MOVIES_AND_SHOWS.into()));
    }
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /Collections/{id}/Items?ids=`: those titles leave it.
async fn remove_from_collection(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfManager(user): JfManager,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<StatusCode> {
    let id = jellyfin.ids.number(Kind::Collection, &id).ok_or(JfError::NotFound)?;
    let library_id = collection_library(&state, &user, id).await?;
    not_smart_collection(&state, id).await?;
    let none = Body(serde_json::Value::Null);
    for item in ids_of(&jellyfin, Kind::Item, &none, &q, "ids")? {
        collections::remove_item(&state.db, id, item).await?;
    }
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::NO_CONTENT)
}
