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
//! - `userRating.value`: the caller's own rating, as `/:/rate` sets it;
//!   `thumb.locked` and `art.locked` keep the poster and background, or give
//!   them back to the metadata agent;
//! - `POST /library/metadata/{id}/posters` (or `/arts`) with a picture as the
//!   body sets the poster (or background); with `?url=`, and
//!   `PUT /library/metadata/{id}/poster?url=` (or `/art`), a TMDB image;
//! - collections (`type=18` edits through their section, or by their keys):
//!   title, sort title, summary and labels, and their pictures by their keys
//!   or `POST /library/collections/{key}/posters` (or `/arts`).
//!
//! A field Pley doesn't keep (ratings other than the user's, credits,
//! countries, added dates, …) answers `400` naming it, and nothing of that
//! request is applied: tools such as Kometa take a `200` as done.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use super::keys::{self, Key};
use super::pictures;
use super::{Params, PlexAdmin, PlexError, PlexResult};
use crate::auth::AuthUser;
use crate::events::ServerEvent;
use crate::state::AppState;
use crate::{catalog, collections, labels, overrides};

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

/// Pictures that are locked or unlocked, never given a value this way
/// (they are uploaded): Plex's name and pley's field.
const PICTURES: &[(&str, &str)] = &[("thumb", "poster"), ("art", "backdrop")];

/// Tags one request may add or remove, of each kind.
const MAX_TAGS: usize = 1000;

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

    /// One `tag[…]` or `tag.locked` parameter: `rest` follows the tag's name.
    fn read(&mut self, name: &str, rest: &str, value: &str) -> PlexResult<()> {
        if rest == ".locked" {
            self.locked = Some(value == "1");
        } else if rest == "[].tag.tag-" {
            self.remove.extend(
                value.split(',').map(|t| crate::util::percent_decode(t, false)).filter(|t| !t.trim().is_empty()),
            );
        } else if rest.starts_with('[') && rest.ends_with("].tag.tag") {
            if !value.trim().is_empty() {
                self.add.push(value.trim().to_owned());
            }
        } else {
            return Err(refused(&format!("{name}{rest}")));
        }
        if self.add.len() > MAX_TAGS || self.remove.len() > MAX_TAGS {
            return Err(PlexError::BadRequest(format!("at most {MAX_TAGS} {name} tags at once")));
        }
        Ok(())
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
    /// The caller's own rating (0-10), or `Some(None)` to clear it.
    user_rating: Option<Option<f64>>,
}

/// A parameter that isn't an edit: the request's own and the client's.
fn not_an_edit(key: &str) -> bool {
    matches!(key, "type" | "id") || key.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("x-plex-"))
}

/// The answer to an edit of something Pley doesn't keep.
fn refused(name: &str) -> PlexError {
    PlexError::BadRequest(format!("`{name}` can't be edited: Pley doesn't keep it"))
}

/// A parameter's name (`genre` of `genre[0].tag.tag`) and the rest.
fn split_key(key: &str) -> (&str, &str) {
    key.split_at(key.find(['[', '.']).unwrap_or(key.len()))
}

impl Edit {
    /// An item edit, refused whole if any part of it can't be applied.
    fn parse(params: &Params) -> PlexResult<Self> {
        let mut edit = Edit::default();
        for (key, value) in &params.0 {
            if not_an_edit(key) {
                continue;
            }
            let (name, rest) = split_key(key);
            if let Some(tags) = edit.tags_for(name) {
                tags.read(name, rest, value)?;
                continue;
            }
            let field = FIELDS.iter().find(|(plex, _)| *plex == name).map(|(_, field)| *field);
            let picture = PICTURES.iter().find(|(plex, _)| *plex == name).map(|(_, field)| *field);
            match (field, picture, rest) {
                (Some(field), _, ".value") => edit.values.push((field, value.clone())),
                (Some(field), _, ".locked") | (_, Some(field), ".locked") => edit.locks.push((field, value == "1")),
                _ if name == "userRating" && rest == ".value" => {
                    let rating = match value.trim() {
                        "" => None,
                        value => Some(
                            value
                                .parse::<f64>()
                                .ok()
                                .and_then(catalog::valid_rating)
                                .ok_or_else(|| PlexError::BadRequest("userRating must be 0-10".into()))?,
                        ),
                    };
                    edit.user_rating = Some(rating);
                }
                // The user's rating is never changed by anyone else: always kept.
                _ if name == "userRating" && rest == ".locked" => {}
                _ => return Err(refused(name)),
            }
        }
        Ok(edit)
    }

    fn tags_for(&mut self, name: &str) -> Option<&mut Tags> {
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
            && self.user_rating.is_none()
    }
}

/// What one request changes in collections.
#[derive(Debug, Default, PartialEq)]
struct CollectionEdit {
    title: Option<String>,
    sort_title: Option<String>,
    /// `Some(None)` clears it.
    summary: Option<Option<String>>,
    labels: Tags,
}

impl CollectionEdit {
    /// A collection edit, refused whole if any part of it can't be applied.
    /// Pley keeps every edit of a collection: locks are accepted, unlocking
    /// is refused.
    fn parse(params: &Params) -> PlexResult<Self> {
        let mut edit = CollectionEdit::default();
        for (key, value) in &params.0 {
            if not_an_edit(key) {
                continue;
            }
            let (name, rest) = split_key(key);
            if name == "label" {
                edit.labels.read(name, rest, value)?;
                continue;
            }
            let value = value.trim();
            match (name, rest) {
                ("title" | "titleSort" | "summary" | "thumb" | "art", ".locked") if value == "1" => {}
                ("title" | "titleSort" | "summary" | "thumb" | "art", ".locked") => {
                    return Err(PlexError::BadRequest(format!(
                        "Pley keeps edits of collections: `{name}` stays locked"
                    )));
                }
                ("title", ".value") if value.is_empty() => {
                    return Err(PlexError::BadRequest("the title can't be empty".into()));
                }
                ("title", ".value") => edit.title = Some(value.chars().take(200).collect()),
                // Empty: sorted by the title again.
                ("titleSort", ".value") => edit.sort_title = Some(value.chars().take(200).collect()),
                ("summary", ".value") => edit.summary = Some(Some(value.to_owned()).filter(|s| !s.is_empty())),
                _ => return Err(refused(name)),
            }
        }
        Ok(edit)
    }
}

/// `PUT /library/sections/{id}/all?type=…&id=1,2&…`
pub async fn section(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
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
        return edit_collections(&state, Some(section), &ids, &CollectionEdit::parse(&params)?).await;
    }
    apply(&state, &admin, Some(section), &ids, &Edit::parse(&params)?).await
}

/// Title, sort title, summary and labels of collections, which
/// python-plexapi edits through their section like items (`section`), or by
/// their keys.
async fn edit_collections(
    state: &AppState,
    section: Option<i64>,
    ids: &[i64],
    edit: &CollectionEdit,
) -> PlexResult<StatusCode> {
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
    let summary = edit.summary.as_ref().map(|s| s.as_deref());
    for &id in ids {
        collections::update(&state.db, id, edit.title.as_deref(), summary, None).await?;
        if !edit.labels.add.is_empty() || !edit.labels.remove.is_empty() {
            labels::edit_collection(&state.db, id, &edit.labels.add, &edit.labels.remove).await?;
        }
        // An explicit sort title (Kometa orders collections with them), or
        // the title's own again.
        if let Some(sort) = &edit.sort_title {
            let sort = if sort.is_empty() {
                let title: String = sqlx::query_scalar("SELECT title FROM collections WHERE id = ?")
                    .bind(id)
                    .fetch_one(&state.db.read)
                    .await?;
                crate::util::sort_title(&title)
            } else {
                sort.clone()
            };
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
    PlexAdmin(admin): PlexAdmin,
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
        (_, 0) if items.len() == keys.len() => apply(&state, &admin, None, &items, &Edit::parse(&params)?).await,
        (0, _) if collections.len() == keys.len() => {
            edit_collections(&state, None, &collections, &CollectionEdit::parse(&params)?).await
        }
        _ => Err(PlexError::BadRequest("items, collections and playlists are edited apart".into())),
    }
}

/// Applies `edit` to every item of `ids` (in `section` when given), once
/// they are all found; `user`'s own rating for `userRating`.
async fn apply(
    state: &AppState,
    user: &AuthUser,
    section: Option<i64>,
    ids: &[i64],
    edit: &Edit,
) -> PlexResult<StatusCode> {
    if edit.is_empty() {
        return Ok(StatusCode::OK);
    }
    for (field, value) in &edit.values {
        overrides::validate(field, Some(value.trim()))?;
    }
    let mut rows = Vec::with_capacity(ids.len());
    for &id in ids {
        let row: Option<(i64, String, Option<String>)> =
            sqlx::query_as("SELECT library_id, kind, genres FROM items WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db.read)
                .await?;
        let Some((library_id, kind, genres)) = row else { return Err(PlexError::NotFound) };
        if section.is_some_and(|s| s != library_id) {
            return Err(PlexError::NotFound);
        }
        rows.push((id, library_id, kind, genres));
    }
    let mut libraries = Vec::new();
    for (id, library_id, kind, genres) in rows {
        let mut changes: Vec<(String, Option<String>)> =
            edit.values.iter().map(|(field, value)| ((*field).to_owned(), Some(value.trim().to_owned()))).collect();
        for &(field, locked) in &edit.locks {
            if edit.values.iter().any(|(f, _)| *f == field) {
                continue;
            }
            // An episode's picture is its still (Plex's `thumb` of it).
            let field = if field == "poster" && kind == "episode" { "thumb" } else { field };
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
        } else if edit.genres.locked == Some(true)
            && let Some(genres) = genres
        {
            // Kept as they are.
            changes.push(("genres".into(), Some(genres)));
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
        if let Some(rating) = edit.user_rating {
            catalog::set_rating(&state.db, user.id, id, rating).await?;
        }
        for title in &edit.collections.add {
            let title: String = title.chars().take(200).collect();
            let title = title.as_str();
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
/// Themes, logos and square art, which Pley keeps none of, answer `400`.
fn artwork_field(name: &str) -> PlexResult<&'static str> {
    match name {
        "posters" | "poster" | "thumbs" | "thumb" => Ok("poster"),
        "arts" | "art" => Ok("backdrop"),
        _ => match pictures::unkept(name) {
            Some(what) => Err(PlexError::BadRequest(format!("Pley keeps no {what}"))),
            None => Err(PlexError::NotFound),
        },
    }
}

/// What a picture goes onto: an item or a collection.
struct Target {
    key: Key,
    library_id: i64,
    /// An episode's poster is its still, as Plex shows it.
    episode: bool,
}

/// What `key` names, for a picture. Playlists show their first entry's
/// picture and keep none of their own.
async fn picture_target(state: &AppState, user: &AuthUser, key: i64) -> PlexResult<Target> {
    let target = Key::of(key);
    let (library_id, episode) = match target {
        Key::Item(id) => {
            let row: (i64, String) = sqlx::query_as("SELECT library_id, kind FROM items WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db.read)
                .await?
                .ok_or(PlexError::NotFound)?;
            (row.0, row.1 == "episode")
        }
        Key::Collection(id) => (collections::library_of(&state.db, id).await?.ok_or(PlexError::NotFound)?, false),
        Key::Playlist(id) => {
            crate::playlists::get_rows(&state.db, user, id).await?.ok_or(PlexError::NotFound)?;
            return Err(PlexError::BadRequest(
                "a playlist shows its first entry's picture: Pley keeps none of its own".into(),
            ));
        }
    };
    Ok(Target { key: target, library_id, episode })
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
    let field = artwork_field(&name)?;
    if !name.ends_with('s') {
        return Err(PlexError::NotFound);
    }
    let target = picture_target(&state, &admin, key).await?;
    let body = match params.get("url").filter(|u| !u.is_empty()) {
        Some(url) => {
            state.metadata.download_artwork(url).await.map_err(|e| PlexError::BadRequest(e.to_string()))?.into()
        }
        None => body,
    };
    let blob = crate::api::editing::store_picture(&state, body).await.map_err(PlexError::from)?;
    set_artwork_blob(&state, &target, field, blob).await
}

/// `PUT /library/metadata/{key}/poster?url=…` (or `/art`): a TMDB image,
/// or the picture `/posters` lists (python-plexapi's `setPoster()`), which
/// is the one shown already.
pub async fn choose_artwork(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((key, name)): Path<(i64, String)>,
    params: Params,
) -> PlexResult<StatusCode> {
    let field = artwork_field(&name)?;
    if name.ends_with('s') {
        return Err(PlexError::NotFound);
    }
    let url = params.get("url").filter(|u| !u.is_empty()).ok_or(PlexError::BadRequest("`url` is missing".into()))?;
    let target = picture_target(&state, &admin, key).await?;
    let kind = if field == "poster" { pictures::Kind::Poster } else { pictures::Kind::Art };
    match pictures::is_current(&state, &admin, key, kind, url).await? {
        Some(true) => return Ok(StatusCode::OK),
        Some(false) => {
            return Err(PlexError::BadRequest("Pley keeps the picture shown alone: upload another instead".into()));
        }
        None => {}
    }
    let bytes = state.metadata.download_artwork(url).await.map_err(|e| PlexError::BadRequest(e.to_string()))?;
    let blob = crate::api::editing::store_picture(&state, bytes.into()).await.map_err(PlexError::from)?;
    set_artwork_blob(&state, &target, field, blob).await
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

async fn set_artwork_blob(state: &AppState, target: &Target, field: &str, blob: String) -> PlexResult<StatusCode> {
    match target.key {
        Key::Item(id) => {
            let field = if field == "poster" && target.episode { "thumb" } else { field };
            overrides::set(&state.db, id, &[(field.to_owned(), Some(blob))]).await?
        }
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
    state.events.send(ServerEvent::LibraryChanged { library_id: target.library_id });
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
            ("thumb.locked", "1"),
            ("userRating.value", "8.0"),
            ("userRating.locked", "1"),
            ("X-Plex-Token", "t"),
        ]))
        .unwrap();
        assert_eq!(edit.values, [("title", "Alien: Director's Cut".to_owned())]);
        assert_eq!(edit.locks, [("title", true), ("summary", false), ("poster", true)]);
        assert_eq!(
            edit.genres,
            Tags { add: vec!["Horror".into(), "Science Fiction".into()], remove: vec![], locked: Some(true) }
        );
        assert_eq!(edit.labels.remove, ["Old Overlay", "4K"]);
        assert_eq!(edit.collections.add, ["Alien Collection"]);
        assert_eq!(edit.user_rating, Some(Some(8.0)));
        assert!(Edit::parse(&params(&[("type", "1"), ("id", "7")])).unwrap().is_empty());
        assert_eq!(Edit::parse(&params(&[("userRating.value", "")])).unwrap().user_rating, Some(None));
        assert_eq!(crate::util::percent_decode("Sci%2DFi%zz%+", false), "Sci-Fi%zz%+");
        assert_eq!(crate::util::percent_decode("a+b", true), "a b");
    }

    #[test]
    fn what_pley_doesnt_keep_is_refused() {
        for (key, value) in [
            ("rating.value", "7.5"),
            ("audienceRating.value", "8"),
            ("addedAt.value", "1700000000"),
            ("editionTitle.value", "Extended"),
            ("director[0].tag.tag", "Someone"),
            ("country[0].tag.tag", "France"),
            ("writer[].tag.tag-", "Someone"),
            ("logo.locked", "1"),
            ("thumb.value", "x"),
            ("userRating.value", "11"),
            ("genre[0].tag.ta", "Horror"),
            ("title", "No .value"),
        ] {
            let pairs = [("title.value", "Kept"), (key, value)];
            assert!(Edit::parse(&params(&pairs)).is_err(), "{key}");
        }
        let collection = CollectionEdit::parse(&params(&[
            ("titleSort.value", "001"),
            ("summary.value", ""),
            ("label[0].tag.tag", "Kometa"),
            ("label.locked", "1"),
            ("thumb.locked", "1"),
        ]))
        .unwrap();
        assert_eq!(collection.sort_title.as_deref(), Some("001"));
        assert_eq!(collection.summary, Some(None));
        assert_eq!(collection.labels.add, ["Kometa"]);
        for (key, value) in [
            ("contentRating.value", "PG"),
            ("userRating.value", "5"),
            ("summary.locked", "0"),
            ("genre[0].tag.tag", "x"),
        ] {
            assert!(CollectionEdit::parse(&params(&[(key, value)])).is_err(), "{key}");
        }
    }
}
