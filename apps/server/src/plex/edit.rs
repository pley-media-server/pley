//! Metadata edits through the Plex API, as python-plexapi and Kometa send
//! them (administrators only):
//!
//! - `PUT /library/sections/{id}/all?type=1&id=1,2&title.value=…&title.locked=1`
//!   and `PUT /library/metadata/{id}?…`: fields become pley's overrides, kept
//!   through refreshes (see [`crate::overrides`]); `field.locked=1` keeps the
//!   current value, `field.locked=0` gives the field back to the metadata
//!   agent;
//! - `genre`, `label` and `collection` tags: `genre[0].tag.tag=Action` adds,
//!   `genre[].tag.tag-=Action,Drama` removes (each name percent-encoded once
//!   more). An item joins or leaves the collection of that name in its
//!   library, which is made when missing;
//! - `POST /library/metadata/{id}/posters` (or `/arts`) with a picture as the
//!   body sets the poster (or background); with `?url=`, and
//!   `PUT /library/metadata/{id}/poster?url=` (or `/art`), a TMDB image;
//! - collections (`type=18` edits through their section, or by their keys):
//!   title, sort title and summary, and their pictures by their keys or
//!   `POST /library/collections/{key}/posters` (or `/arts`).

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use super::keys::{self, Key};
use super::{Params, PlexAdmin, PlexError, PlexResult};
use crate::auth::AuthUser;
use crate::events::ServerEvent;
use crate::state::AppState;
use crate::{collections, labels, overrides};

/// Plex's type number of collections.
const COLLECTION_TYPE: &str = "18";

/// Plex's field names and pley's.
const FIELDS: &[(&str, &str)] = &[
    ("title", "title"),
    ("titleSort", "sort_title"),
    ("originalTitle", "original_title"),
    ("year", "year"),
    ("summary", "summary"),
    ("tagline", "tagline"),
    ("studio", "studio"),
    ("contentRating", "content_rating"),
    ("originallyAvailableAt", "release_date"),
];

/// Tags a request adds and removes, and whether it (un)locks their list.
#[derive(Debug, Default, PartialEq)]
struct Tags {
    add: Vec<String>,
    remove: Vec<String>,
    locked: Option<bool>,
}

impl Tags {
    fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty() && self.locked.is_none()
    }
}

/// What one request changes.
#[derive(Debug, Default, PartialEq)]
struct Edit {
    /// pley field and its new value.
    values: Vec<(&'static str, String)>,
    /// pley field locked (keep its value) or not (back to the agent).
    locks: Vec<(&'static str, bool)>,
    genres: Tags,
    labels: Tags,
    collections: Tags,
}

impl Edit {
    fn parse(params: &Params) -> Self {
        let mut edit = Edit::default();
        for (key, value) in &params.0 {
            if let Some((name, part)) = key.split_once('.')
                && let Some(&(_, field)) = FIELDS.iter().find(|(plex, _)| *plex == name)
            {
                match part {
                    "value" => edit.values.push((field, value.clone())),
                    "locked" => edit.locks.push((field, value == "1")),
                    _ => {}
                }
                continue;
            }
            let Some(tags) = edit.tags_for(key) else { continue };
            let rest = &key[key.find(['[', '.']).unwrap_or(key.len())..];
            if rest == ".locked" {
                tags.locked = Some(value == "1");
            } else if rest.ends_with("].tag.tag-") {
                tags.remove.extend(
                    value.split(',').map(|t| crate::util::percent_decode(t, false)).filter(|t| !t.trim().is_empty()),
                );
            } else if rest.ends_with("].tag.tag") && !value.trim().is_empty() {
                tags.add.push(value.trim().to_owned());
            }
        }
        edit
    }

    fn tags_for(&mut self, key: &str) -> Option<&mut Tags> {
        let name = &key[..key.find(['[', '.']).unwrap_or(key.len())];
        match name {
            "genre" => Some(&mut self.genres),
            "label" => Some(&mut self.labels),
            "collection" => Some(&mut self.collections),
            _ => None,
        }
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
            && self.locks.is_empty()
            && self.genres.is_empty()
            && self.labels.is_empty()
            && self.collections.is_empty()
    }
}

/// `PUT /library/sections/{id}/all?type=…&id=1,2&…`
pub async fn section(
    State(state): State<AppState>,
    PlexAdmin(_admin): PlexAdmin,
    Path(section): Path<String>,
    params: Params,
) -> PlexResult<StatusCode> {
    let section: i64 = section.parse().map_err(|_| PlexError::NotFound)?;
    let ids: Vec<i64> =
        params.get("id").unwrap_or_default().split(',').filter_map(|i| i.trim().parse().ok()).take(500).collect();
    if ids.is_empty() {
        return Err(PlexError::BadRequest("edits name the items they change (`id`)".into()));
    }
    if params.get("type") == Some(COLLECTION_TYPE) {
        // Collections' keys, or their ids as this API gave them before.
        let ids = ids.into_iter().map(|key| keys::collection_id(key).ok_or(PlexError::NotFound));
        let ids = ids.collect::<PlexResult<Vec<i64>>>()?;
        return edit_collections(&state, Some(section), &ids, &params).await;
    }
    apply(&state, Some(section), &ids, &Edit::parse(&params)).await
}

/// Title, sort title and summary of collections, which python-plexapi edits
/// through their section like items (`section`), or by their keys.
async fn edit_collections(
    state: &AppState,
    section: Option<i64>,
    ids: &[i64],
    params: &Params,
) -> PlexResult<StatusCode> {
    let value = |name: &str| params.get(&format!("{name}.value")).map(str::trim);
    let mut libraries = Vec::new();
    for &id in ids {
        let library_id = collections::library_of(&state.db, id).await?.ok_or(PlexError::NotFound)?;
        if section.is_some_and(|s| s != library_id) {
            return Err(PlexError::NotFound);
        }
        if !libraries.contains(&library_id) {
            libraries.push(library_id);
        }
    }
    for &id in ids {
        let title = value("title").filter(|t| !t.is_empty());
        let summary = value("summary").map(|s| Some(s).filter(|s| !s.is_empty()));
        collections::update(&state.db, id, title, summary, None).await?;
        // An explicit sort title (Kometa orders collections with them).
        if let Some(sort) = value("titleSort").filter(|s| !s.is_empty()) {
            sqlx::query("UPDATE collections SET sort_title = ?, edited = 1 WHERE id = ?")
                .bind(sort)
                .bind(id)
                .execute(&state.db.write)
                .await?;
        }
    }
    for library_id in libraries {
        state.events.send(ServerEvent::LibraryChanged { library_id });
    }
    Ok(StatusCode::OK)
}

/// `PUT /library/metadata/{keys}?…`: items, or collections by their keys.
pub async fn item(
    State(state): State<AppState>,
    PlexAdmin(_admin): PlexAdmin,
    Path(ids): Path<String>,
    params: Params,
) -> PlexResult<StatusCode> {
    let keys: Vec<Key> = ids.split(',').filter_map(|i| i.trim().parse().ok()).take(500).map(Key::of).collect();
    let items: Vec<i64> = keys.iter().filter_map(|k| if let Key::Item(id) = k { Some(*id) } else { None }).collect();
    let collections: Vec<i64> =
        keys.iter().filter_map(|k| if let Key::Collection(id) = k { Some(*id) } else { None }).collect();
    match (items.len(), collections.len()) {
        (0, 0) if keys.is_empty() => Err(PlexError::NotFound),
        (0, 0) => Err(PlexError::BadRequest("playlists are edited at /playlists/{key}".into())),
        (_, 0) if items.len() == keys.len() => apply(&state, None, &items, &Edit::parse(&params)).await,
        (0, _) if collections.len() == keys.len() => edit_collections(&state, None, &collections, &params).await,
        _ => Err(PlexError::BadRequest("items, collections and playlists are edited apart".into())),
    }
}

/// Applies `edit` to every item of `ids` (in `section` when given).
async fn apply(state: &AppState, section: Option<i64>, ids: &[i64], edit: &Edit) -> PlexResult<StatusCode> {
    if edit.is_empty() {
        return Ok(StatusCode::OK);
    }
    for (field, value) in &edit.values {
        overrides::validate(field, Some(value.trim()))?;
    }
    let mut libraries = Vec::new();
    for &id in ids {
        let row: Option<(i64, Option<String>)> = sqlx::query_as("SELECT library_id, genres FROM items WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db.read)
            .await?;
        let Some((library_id, genres)) = row else { return Err(PlexError::NotFound) };
        if section.is_some_and(|s| s != library_id) {
            return Err(PlexError::NotFound);
        }
        let mut changes: Vec<(String, Option<String>)> =
            edit.values.iter().map(|(field, value)| ((*field).to_owned(), Some(value.trim().to_owned()))).collect();
        for &(field, locked) in &edit.locks {
            if edit.values.iter().any(|(f, _)| *f == field) {
                continue;
            }
            if locked {
                // Keep what the field holds now.
                let current: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                    "SELECT CAST({field} AS TEXT) FROM items WHERE id = ?"
                )))
                .bind(id)
                .fetch_one(&state.db.read)
                .await?;
                if let Some(current) = current {
                    changes.push((field.to_owned(), Some(current)));
                }
            } else {
                changes.push((field.to_owned(), None));
            }
        }
        if !edit.genres.add.is_empty() || !edit.genres.remove.is_empty() {
            let mut list: Vec<String> = genres.and_then(|g| serde_json::from_str(&g).ok()).unwrap_or_default();
            list.retain(|g| !edit.genres.remove.iter().any(|r| r.trim().eq_ignore_ascii_case(g)));
            for genre in &edit.genres.add {
                if !list.iter().any(|g| g.eq_ignore_ascii_case(genre)) {
                    list.push(genre.clone());
                }
            }
            changes.push((
                "genres".into(),
                Some(serde_json::to_string(&list).map_err(|e| PlexError::Internal(e.into()))?),
            ));
        } else if edit.genres.locked == Some(false) {
            changes.push(("genres".into(), None));
        }
        if !changes.is_empty() {
            overrides::set(&state.db, id, &changes).await?;
        }
        // A value sent unlocked is written but not kept: the next refresh may
        // replace it, as with Plex.
        let unlocked: Vec<(String, Option<String>)> = edit
            .values
            .iter()
            .filter(|(field, _)| edit.locks.contains(&(*field, false)))
            .map(|(field, _)| ((*field).to_owned(), None))
            .collect();
        if !unlocked.is_empty() {
            overrides::set(&state.db, id, &unlocked).await?;
        }
        if !edit.labels.add.is_empty() || !edit.labels.remove.is_empty() {
            labels::edit(&state.db, id, &edit.labels.add, &edit.labels.remove).await?;
        }
        for title in &edit.collections.add {
            let collection = match collection_named(state, library_id, title).await? {
                Some(collection) => collection,
                None => collections::create(&state.db, library_id, title, None).await?,
            };
            collections::add_items(&state.db, collection, library_id, &[id]).await?;
        }
        for title in &edit.collections.remove {
            if let Some(collection) = collection_named(state, library_id, title).await? {
                collections::remove_item(&state.db, collection, id).await?;
            }
        }
        if !libraries.contains(&library_id) {
            libraries.push(library_id);
        }
    }
    for library_id in libraries {
        state.events.send(ServerEvent::LibraryChanged { library_id });
    }
    Ok(StatusCode::OK)
}

async fn collection_named(state: &AppState, library_id: i64, title: &str) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT id FROM collections WHERE library_id = ? AND title = ? COLLATE NOCASE AND smart_filter IS NULL ORDER BY id",
    )
        .bind(library_id)
        .bind(title.trim())
        .fetch_optional(&state.db.read)
        .await
}

/// The override field of the picture a path names (`posters`, `poster`, …).
fn artwork_field(name: &str) -> Option<&'static str> {
    match name {
        "posters" | "poster" | "thumbs" | "thumb" => Some("poster"),
        "arts" | "art" => Some("backdrop"),
        _ => None,
    }
}

/// What a picture goes onto: an item or a collection, and its library.
/// Playlists show their first entry's picture and keep none of their own.
async fn picture_target(state: &AppState, user: &AuthUser, key: i64) -> PlexResult<(Key, i64)> {
    let target = Key::of(key);
    let library_id = match target {
        Key::Item(id) => sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db.read)
            .await?
            .ok_or(PlexError::NotFound)?,
        Key::Collection(id) => collections::library_of(&state.db, id).await?.ok_or(PlexError::NotFound)?,
        Key::Playlist(id) => {
            crate::playlists::get_rows(&state.db, user, id).await?.ok_or(PlexError::NotFound)?;
            return Err(PlexError::BadRequest(
                "a playlist shows its first entry's picture: Pley keeps none of its own".into(),
            ));
        }
    };
    Ok((target, library_id))
}

/// `POST /library/metadata/{key}/posters` (or `/arts`): the body as the
/// picture, or `?url=` a TMDB image, for an item or a collection.
pub async fn upload_artwork(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((key, name)): Path<(i64, String)>,
    params: Params,
    body: Bytes,
) -> PlexResult<StatusCode> {
    let field = artwork_field(&name).filter(|_| name.ends_with('s')).ok_or(PlexError::NotFound)?;
    let (target, library_id) = picture_target(&state, &admin, key).await?;
    let blob = match params.get("url").filter(|u| !u.is_empty()) {
        Some(url) => state.metadata.download_artwork(url).await.map_err(|e| PlexError::BadRequest(e.to_string()))?,
        None => crate::api::editing::store_picture(&state, body).await.map_err(PlexError::from)?,
    };
    set_artwork_blob(&state, target, library_id, field, blob).await
}

/// `PUT /library/metadata/{key}/poster?url=…` (or `/art`): a TMDB image.
pub async fn choose_artwork(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((key, name)): Path<(i64, String)>,
    params: Params,
) -> PlexResult<StatusCode> {
    let field = artwork_field(&name).filter(|_| !name.ends_with('s')).ok_or(PlexError::NotFound)?;
    let url = params.get("url").filter(|u| !u.is_empty()).ok_or(PlexError::BadRequest("`url` is missing".into()))?;
    let (target, library_id) = picture_target(&state, &admin, key).await?;
    let blob = state.metadata.download_artwork(url).await.map_err(|e| PlexError::BadRequest(e.to_string()))?;
    set_artwork_blob(&state, target, library_id, field, blob).await
}

/// `POST /library/collections/{key}/posters` (or `/arts`): a collection's
/// picture, uploaded or (`?url=`) a TMDB image.
pub async fn upload_collection_artwork(
    state: State<AppState>,
    admin: PlexAdmin,
    Path((key, name)): Path<(i64, String)>,
    params: Params,
    body: Bytes,
) -> PlexResult<StatusCode> {
    let id = super::lists::collection_path(key)?;
    upload_artwork(state, admin, Path((keys::collection(id), name)), params, body).await
}

async fn set_artwork_blob(
    state: &AppState,
    target: Key,
    library_id: i64,
    field: &str,
    blob: String,
) -> PlexResult<StatusCode> {
    match target {
        Key::Item(id) => overrides::set(&state.db, id, &[(field.to_owned(), Some(blob))]).await?,
        Key::Collection(id) => {
            // `field` is `poster` or `backdrop`, both columns of collections.
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE collections SET {field} = ?, updated_at = ? WHERE id = ?"
            )))
            .bind(blob)
            .bind(crate::util::now())
            .bind(id)
            .execute(&state.db.write)
            .await?;
        }
        Key::Playlist(_) => return Err(PlexError::NotFound),
    }
    state.events.send(ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Params {
        Params(pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect())
    }

    #[test]
    fn edits_as_python_plexapi_sends_them() {
        let edit = Edit::parse(&params(&[
            ("type", "1"),
            ("id", "7"),
            ("title.value", "Alien: Director's Cut"),
            ("title.locked", "1"),
            ("summary.locked", "0"),
            ("genre[0].tag.tag", "Horror"),
            ("genre[1].tag.tag", "Science Fiction"),
            ("genre.locked", "1"),
            ("label[].tag.tag-", "Old%20Overlay,4K"),
            ("collection[0].tag.tag", "Alien Collection"),
            ("unknown.value", "x"),
        ]));
        assert_eq!(edit.values, [("title", "Alien: Director's Cut".to_owned())]);
        assert_eq!(edit.locks, [("title", true), ("summary", false)]);
        assert_eq!(
            edit.genres,
            Tags { add: vec!["Horror".into(), "Science Fiction".into()], remove: vec![], locked: Some(true) }
        );
        assert_eq!(edit.labels.remove, ["Old Overlay", "4K"]);
        assert_eq!(edit.collections.add, ["Alien Collection"]);
        assert!(Edit::parse(&params(&[("type", "1"), ("id", "7")])).is_empty());
        assert_eq!(crate::util::percent_decode("Sci%2DFi%zz%+", false), "Sci-Fi%zz%+");
        assert_eq!(crate::util::percent_decode("a+b", true), "a b");
    }
}
