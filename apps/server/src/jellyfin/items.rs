//! Browsing: libraries (views), listings with Jellyfin's filters, sorts and
//! paging, an item's details, children and extras, the home screen's rows
//! (latest, resume, next up), genres, people, search, pictures, and the
//! user's watch state, favorites (My List) and ratings.

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;

use super::dto::{self, Batch, Fields, Item, NameId};
use super::ids::Kind;
use super::{Body, Jellyfin, JfError, JfResult, JfUser, Q, QueryResult};
use crate::auth::{AuthUser, SourceIp};
use crate::catalog::{self, ItemRow};
use crate::events::ServerEvent;
use crate::library::LibraryRow;
use crate::listing::{self, Page, Params, SectionQuery};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/userviews", get(views))
        .route("/users/{user}/views", get(views))
        .route("/userviews/groupingoptions", get(grouping_options))
        .route("/users/{user}/groupingoptions", get(grouping_options))
        .route("/library/virtualfolders", get(virtual_folders))
        .route("/items", get(items))
        .route("/users/{user}/items", get(items))
        .route("/items/latest", get(latest))
        .route("/items/suggestions", get(suggestions))
        .route("/users/{user}/suggestions", get(suggestions))
        .route("/users/{user}/items/latest", get(latest))
        .route("/useritems/resume", get(resume))
        .route("/users/{user}/items/resume", get(resume))
        .route("/items/filters", get(filters))
        .route("/items/filters2", get(filters2))
        .route("/items/{id}", get(item).delete(super::lists::delete_item))
        .route("/users/{user}/items/{id}", get(user_item))
        .route("/items/{id}/similar", get(similar))
        .route("/items/{id}/localtrailers", get(trailers))
        .route("/users/{user}/items/{id}/localtrailers", get(user_trailers))
        .route("/items/{id}/specialfeatures", get(special_features))
        .route("/users/{user}/items/{id}/specialfeatures", get(user_special_features))
        .route("/items/{id}/ancestors", get(ancestors))
        .route("/items/{id}/intros", get(nothing))
        .route("/users/{user}/items/{id}/intros", get(nothing))
        .route("/videos/{id}/additionalparts", get(nothing))
        .route("/items/{id}/thememedia", get(nothing))
        .route("/items/{id}/themesongs", get(nothing))
        .route("/items/{id}/collections", get(nothing))
        .route("/items/{id}/images/{kind}", get(image).head(image))
        .route("/items/{id}/images/{kind}/{index}", get(image_at).head(image_at))
        .route("/shows/nextup", get(next_up))
        .route("/shows/upcoming", get(nothing))
        .route("/shows/{id}/seasons", get(seasons))
        .route("/shows/{id}/episodes", get(episodes))
        .route("/genres", get(genres))
        .route("/musicgenres", get(genres))
        .route("/artists", get(artists))
        .route("/artists/albumartists", get(artists))
        .route("/studios", get(studios))
        .route("/years", get(years))
        .route("/tags", get(nothing))
        .route("/persons", get(persons))
        .route("/search/hints", get(search_hints))
        .route(
            "/playlists/{id}/items",
            get(playlist_items).post(super::lists::add_to_playlist).delete(super::lists::remove_from_playlist),
        )
        .route("/userplayeditems/{id}", post(played).delete(unplayed))
        .route("/users/{user}/playeditems/{id}", post(user_played).delete(user_unplayed))
        .route("/userfavoriteitems/{id}", post(favorite).delete(unfavorite))
        .route("/users/{user}/favoriteitems/{id}", post(user_favorite).delete(user_unfavorite))
        .route("/useritems/{id}/userdata", get(get_user_data).post(set_user_data))
        .route("/users/{user}/items/{id}/userdata", get(user_get_user_data).post(user_set_user_data))
        .route("/useritems/{id}/rating", post(rate).delete(unrate))
        .route("/users/{user}/items/{id}/rating", post(user_rate).delete(user_unrate))
}

/// The most items a page has: clients asking for more get this many.
const MAX_LIMIT: usize = 1000;

/// Libraries the user can see.
pub async fn libraries(state: &AppState, user: &AuthUser) -> JfResult<Vec<LibraryRow>> {
    Ok(crate::access::visible_libraries(&state.db, user).await?)
}

/// A user id in a path must be the caller's: tokens see only their own.
fn own(jellyfin: &Jellyfin, user: &AuthUser, id: &str) -> JfResult<()> {
    if jellyfin.ids.number(Kind::User, id) == Some(user.id) { Ok(()) } else { Err(JfError::NotFound) }
}

/// The item of an id, as the user may see it.
pub async fn visible_item(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, id: &str) -> JfResult<ItemRow> {
    let number = jellyfin.ids.number(Kind::Item, id).ok_or(JfError::NotFound)?;
    let row = catalog::item(&state.db, user.id, number).await?.ok_or(JfError::NotFound)?;
    if !libraries(state, user).await?.iter().any(|l| l.id == row.library_id) {
        return Err(JfError::NotFound);
    }
    Ok(row)
}

pub async fn dtos(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    rows: &[ItemRow],
    fields: Fields,
) -> JfResult<Vec<Item>> {
    let batch = Batch::load(state, user, rows, fields).await?;
    Ok(rows.iter().map(|row| dto::build(jellyfin, row, &batch)).collect())
}

fn page(q: &Q) -> Page {
    let start = q.number("startIndex").unwrap_or(0).max(0) as usize;
    let size = q.number("limit").map(|l| (l.max(0) as usize).min(MAX_LIMIT));
    Page { start, size: Some(size.unwrap_or(MAX_LIMIT)) }
}

fn answer(items: Vec<Item>, total: usize, start: usize) -> Response {
    Json(QueryResult { items, total_record_count: total, start_index: start }).into_response()
}

// ------------------------------------------------------------------ views

async fn views(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    user_path: Option<Path<String>>,
) -> JfResult<Response> {
    if let Some(Path(id)) = user_path {
        own(&jellyfin, &user, &id)?;
    }
    let items: Vec<Item> = libraries(&state, &user).await?.iter().map(|lib| Item::library(&jellyfin, lib)).collect();
    let total = items.len();
    Ok(answer(items, total, 0))
}

/// `/UserViews/GroupingOptions`: the libraries Jellyfin lets a user group
/// into its views, its movies' and shows', by name. Pley groups nothing, but
/// apps ask as they load their home screen, and Infuse stops at a `404`.
async fn grouping_options(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    user_path: Option<Path<String>>,
) -> JfResult<Json<Vec<NameId>>> {
    if let Some(Path(id)) = user_path {
        own(&jellyfin, &user, &id)?;
    }
    let mut options: Vec<NameId> = libraries(&state, &user)
        .await?
        .iter()
        .filter(|lib| lib.kind() != crate::library::LibraryKind::Music)
        .map(|lib| NameId { name: lib.name.clone(), id: jellyfin.ids.of(Kind::Library, lib.id) })
        .collect();
    options.sort_by_key(|option| option.name.to_lowercase());
    Ok(Json(options))
}

/// `/Library/VirtualFolders`: the libraries and their folders on disk, which
/// Jellyfin shows administrators alone. Nothing administers through this
/// API, so everyone is refused as Jellyfin refuses those who don't (`403`):
/// apps that ask as they load (Infuse) carry on, and stop at a `404`.
async fn virtual_folders(_user: JfUser) -> JfError {
    JfError::Forbidden
}

// ------------------------------------------------------------------ listings

/// Jellyfin's sorts as the listing's (`SortBy=SortName,ProductionYear`,
/// `SortOrder=Descending`).
fn sort_of(q: &Q) -> String {
    let orders = q.list("sortOrder");
    let mut out = Vec::new();
    for (i, field) in q.list("sortBy").iter().enumerate() {
        let key = match field.to_ascii_lowercase().as_str() {
            "sortname" | "name" => "titleSort",
            "datecreated" | "datelastcontentadded" => "addedAt",
            "premieredate" => "originallyAvailableAt",
            "productionyear" => "year",
            "communityrating" | "criticrating" => "rating",
            "dateplayed" => "lastViewedAt",
            "playcount" => "viewCount",
            "runtime" => "duration",
            "indexnumber" | "parentindexnumber" | "airedepisodeorder" => "index",
            "random" => "random",
            _ => continue,
        };
        let descending = orders.get(i).or(orders.first()).is_some_and(|o| o.eq_ignore_ascii_case("descending"));
        out.push(if descending { format!("{key}:desc") } else { key.to_owned() });
    }
    out.join(",")
}

/// A listing's filters (`Filters=IsUnplayed`, `Genres=…`, `Years=…`) as the
/// listing query smart collections use.
fn filter_params(q: &Q, kind: &str) -> Vec<(String, String)> {
    let mut pairs = vec![("type".to_owned(), listing::type_number(kind).to_string())];
    let sort = sort_of(q);
    if !sort.is_empty() && sort != "random" {
        pairs.push(("sort".into(), sort));
    }
    let filters: Vec<String> = q.list("filters").iter().map(|f| f.to_ascii_lowercase()).collect();
    if filters.iter().any(|f| f == "isplayed") || q.flag("isPlayed") == Some(true) {
        pairs.push(("watched".into(), "1".into()));
    }
    if filters.iter().any(|f| f == "isunplayed") || q.flag("isPlayed") == Some(false) {
        pairs.push(("unwatched".into(), "1".into()));
    }
    if filters.iter().any(|f| f == "isresumable") {
        pairs.push(("inProgress".into(), "1".into()));
    }
    for (param, field) in [("genres", "genre"), ("officialRatings", "contentRating"), ("tags", "label")] {
        let values = q.list(param);
        if !values.is_empty() {
            pairs.push((field.into(), values.join(",")));
        }
    }
    let years = q.list("years");
    if !years.is_empty() {
        pairs.push(("year".into(), years.join(",")));
    }
    if let Some(studios) = q.get("studios") {
        pairs.push(("studio".into(), studios.split('|').next().unwrap_or_default().to_owned()));
    }
    if let Some(prefix) = q.get("nameStartsWith") {
        pairs.push(("title<".into(), prefix.to_owned()));
    }
    pairs
}

/// What a listing finds: its rows, its total, where it starts.
struct Found {
    rows: Vec<ItemRow>,
    total: usize,
}

/// The kinds `IncludeItemTypes` asks for that live in `lib`, else its own.
fn kinds_in(lib: &LibraryRow, wanted: &[&'static str], recursive: bool) -> Vec<&'static str> {
    let kinds = listing::kinds_of(lib);
    if wanted.is_empty() {
        return if recursive && lib.kind() != crate::library::LibraryKind::Shows {
            vec![kinds[0]]
        } else {
            vec![lib.kind().top_level_kind()]
        };
    }
    kinds.iter().copied().filter(|k| wanted.contains(k)).collect()
}

/// A listing of libraries (all the user sees, or one), with the filters
/// and the page asked for, merged across libraries in the sort asked for.
async fn list_libraries(
    state: &AppState,
    user: &AuthUser,
    libs: &[LibraryRow],
    q: &Q,
    restrict: Option<&[i64]>,
) -> JfResult<Found> {
    let wanted: Vec<&'static str> = q.list("includeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let recursive = q.flag("recursive").unwrap_or(false);
    let page = page(q);
    let mut rows = Vec::new();
    let mut total = 0;
    let queries: Vec<(&LibraryRow, &'static str)> = libs
        .iter()
        .flat_map(|lib| kinds_in(lib, &wanted, recursive).into_iter().map(move |kind| (lib, kind)))
        .collect();
    let merged = queries.len() > 1;
    let random = sort_of(q).contains("random");
    // Each query's share of a merged page is among its first `start + size`;
    // a random page is drawn from all of them.
    let window = if random {
        Page { start: 0, size: None }
    } else if merged {
        Page { start: 0, size: page.size.map(|s| s + page.start) }
    } else {
        page
    };
    for (lib, kind) in queries {
        let mut pairs = filter_params(q, kind);
        if let Some(ids) = restrict {
            if ids.is_empty() {
                continue;
            }
            pairs.push(("id".into(), ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")));
        }
        let params = Params(pairs);
        let (found, count) =
            SectionQuery::asked_by(user, lib.id, kind, &params)?.page(&state.db, user.id, &window).await?;
        rows.extend(found);
        total += count;
    }
    if random {
        shuffle(&mut rows);
    } else if merged {
        let sort = listing::parse_sort(Some(&sort_of(q)).filter(|s| !s.is_empty()).map(String::as_str));
        let bitrates = listing::bitrates_for(&state.db, &rows, &sort).await?;
        listing::sort_rows(&mut rows, &sort, &bitrates);
    }
    if random || merged {
        rows = rows.into_iter().skip(page.start).take(page.size.unwrap_or(usize::MAX)).collect();
    }
    Ok(Found { rows, total })
}

/// `rows` in a random order (Fisher-Yates), for `SortBy=Random`.
fn shuffle(rows: &mut [ItemRow]) {
    for i in (1..rows.len()).rev() {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        rows.swap(i, (u64::from_le_bytes(bytes) % (i as u64 + 1)) as usize);
    }
}

/// Children of an item, as `ParentId` lists them.
async fn children_of(state: &AppState, user: &AuthUser, parent: &ItemRow, q: &Q) -> JfResult<Vec<ItemRow>> {
    let wanted: Vec<&'static str> = q.list("includeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let recursive = q.flag("recursive").unwrap_or(false);
    let mut rows = catalog::children(&state.db, user.id, parent.id).await?;
    let leaves = matches!(wanted.as_slice(), ["episode"]) || wanted.contains(&"track");
    if (recursive || leaves) && matches!(parent.kind.as_str(), "show" | "artist") {
        let mut all = Vec::new();
        for child in rows {
            all.extend(catalog::children(&state.db, user.id, child.id).await?);
        }
        rows = all;
    }
    if !wanted.is_empty() {
        rows.retain(|r| wanted.contains(&r.kind.as_str()));
    }
    Ok(rows)
}

/// `/Items`: by `Ids`, a search, `ParentId` (a library, an item, a
/// collection or a playlist), the user's favorites or a person's titles.
async fn list(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, q: &Q) -> JfResult<(Vec<ItemRow>, usize)> {
    let libs = libraries(state, user).await?;
    let visible = |rows: Vec<ItemRow>| -> Vec<ItemRow> {
        rows.into_iter().filter(|r| libs.iter().any(|l| l.id == r.library_id)).collect()
    };
    let page = page(q);
    let wanted: Vec<&'static str> = q.list("includeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let excluded: Vec<&'static str> = q.list("excludeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let paged = |mut rows: Vec<ItemRow>| {
        if !wanted.is_empty() {
            rows.retain(|r| wanted.contains(&r.kind.as_str()));
        }
        rows.retain(|r| !excluded.contains(&r.kind.as_str()));
        page.apply(rows)
    };

    let ids: Vec<i64> = q.list("ids").iter().filter_map(|id| jellyfin.ids.number(Kind::Item, id)).collect();
    if q.get("ids").is_some() {
        return Ok(paged(visible(catalog::items_by_ids(&state.db, user.id, &ids).await?)));
    }
    if let Some(term) = q.get("searchTerm") {
        let rows = visible(crate::search::items(&state.db, user.id, term, 200).await?);
        return Ok(paged(rows));
    }

    // Narrowed to favorites or to a person's titles, through the listing.
    let mut restrict: Option<Vec<i64>> = None;
    let filters: Vec<String> = q.list("filters").iter().map(|f| f.to_ascii_lowercase()).collect();
    if filters.iter().any(|f| f == "isfavorite") || q.flag("isFavorite") == Some(true) {
        let list = catalog::watchlist(&state.db, user.id, None).await?;
        restrict = Some(list.iter().map(|r| r.id).collect());
    }
    let people: Vec<i64> = q.list("personIds").iter().filter_map(|id| jellyfin.ids.number(Kind::Person, id)).collect();
    if !people.is_empty() {
        let mut credited = HashSet::new();
        for person in people {
            if let Some(found) = crate::people::person(&state.db, user.id, person).await? {
                credited.extend(found.credits.iter().map(|c| c.item.id));
            }
        }
        let credited: Vec<i64> = credited.into_iter().collect();
        restrict = Some(match restrict {
            Some(before) => before.into_iter().filter(|id| credited.contains(id)).collect(),
            None => credited,
        });
    }

    let artist = q.get("albumArtistIds").or(q.get("artistIds")).and_then(|ids| ids.split([',', '|']).next());
    let parent = q.get("parentId").or(artist);
    match parent.and_then(|id| jellyfin.ids.read(id)) {
        Some((Kind::Library, id)) => {
            let lib: Vec<LibraryRow> = libs.iter().filter(|l| l.id == id).cloned().collect();
            if lib.is_empty() {
                return Err(JfError::NotFound);
            }
            let found = list_libraries(state, user, &lib, q, restrict.as_deref()).await?;
            Ok((found.rows, found.total))
        }
        Some((Kind::Item, id)) => {
            let parent = catalog::item(&state.db, user.id, id).await?.ok_or(JfError::NotFound)?;
            if !libs.iter().any(|l| l.id == parent.library_id) {
                return Err(JfError::NotFound);
            }
            let mut rows = children_of(state, user, &parent, q).await?;
            if let Some(ids) = &restrict {
                rows.retain(|r| ids.contains(&r.id));
            }
            Ok(page.apply(rows))
        }
        Some((Kind::Collection, id)) => {
            let (_, rows) = crate::collections::get(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
            Ok(paged(visible(rows)))
        }
        Some((Kind::Playlist, id)) => {
            let (_, entries) = crate::playlists::get_rows(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
            Ok(paged(visible(entries.into_iter().map(|(_, row)| row).collect())))
        }
        Some(_) => Err(JfError::NotFound),
        None if parent.is_some() => Err(JfError::NotFound),
        None => {
            let found = list_libraries(state, user, &libs, q, restrict.as_deref()).await?;
            Ok((found.rows, found.total))
        }
    }
}

/// `/Items` with no parent and nothing to look for: the user's libraries,
/// as Jellyfin answers (clients list their folders so).
fn asks_for_libraries(q: &Q) -> bool {
    [
        "parentId",
        "includeItemTypes",
        "searchTerm",
        "ids",
        "filters",
        "isFavorite",
        "isPlayed",
        "personIds",
        "genres",
        "years",
        "artistIds",
        "albumArtistIds",
        "studios",
        "mediaTypes",
        "nameStartsWith",
    ]
    .iter()
    .all(|name| q.get(name).is_none())
        && q.flag("recursive") != Some(true)
}

/// Collections and playlists, which `/Items` lists for `BoxSet` and
/// `Playlist` (and clients show as their own rows). Only when they are all
/// that is asked for: a search, ids or other types go to the listing.
async fn lists(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, q: &Q) -> JfResult<Option<(Vec<Item>, usize)>> {
    let types: Vec<String> = q.list("includeItemTypes").iter().map(|t| t.to_ascii_lowercase()).collect();
    if types.is_empty()
        || types.iter().any(|t| t != "boxset" && t != "playlist")
        || q.get("searchTerm").is_some()
        || q.get("ids").is_some()
    {
        return Ok(None);
    }
    let page = page(q);
    let mut items = Vec::new();
    if types.iter().any(|t| t == "boxset") {
        for lib in libraries(state, user).await? {
            if q.get("parentId").is_some_and(|p| jellyfin.ids.number(Kind::Library, p) != Some(lib.id)) {
                continue;
            }
            for c in crate::collections::list(&state.db, user, lib.id).await? {
                let tag = collection_tag(jellyfin, c.updated_at, user);
                let mut collection = list_item(
                    jellyfin,
                    Kind::Collection,
                    c.id,
                    &c.title,
                    "BoxSet",
                    c.item_count,
                    c.updated_at,
                    c.created_at,
                    c.summary.clone(),
                    Some(tag),
                );
                collection.can_delete = manages(user);
                items.push(collection);
            }
        }
        items.sort_by(|a, b| a.sort_name.cmp(&b.sort_name));
    }
    if types.iter().any(|t| t == "playlist") {
        items.extend(crate::playlists::list(&state.db, user).await?.into_iter().map(|p| {
            list_item(
                jellyfin,
                Kind::Playlist,
                p.id,
                &p.title,
                "Playlist",
                p.item_count,
                p.updated_at,
                p.created_at,
                None,
                None,
            )
        }));
    }
    Ok(Some(page.apply(items)))
}

#[allow(clippy::too_many_arguments)]
fn list_item(
    jellyfin: &Jellyfin,
    kind: Kind,
    id: i64,
    title: &str,
    item_type: &'static str,
    count: i64,
    updated_at: i64,
    created_at: i64,
    overview: Option<String>,
    picture: Option<String>,
) -> Item {
    let mut item = Item {
        name: title.to_owned(),
        server_id: jellyfin.server_id.clone(),
        id: jellyfin.ids.of(kind, id),
        etag: Some(format!("{updated_at:x}")),
        date_created: Some(super::users::iso(created_at)),
        // Playlists are listed to their owner alone, who may delete them.
        can_delete: kind == Kind::Playlist,
        sort_name: Some(title.to_lowercase()),
        is_folder: true,
        kind: item_type,
        media_type: "Unknown",
        child_count: Some(count),
        recursive_item_count: Some(count),
        location_type: Some("FileSystem"),
        play_access: Some("Full"),
        overview,
        ..Default::default()
    };
    if let Some(tag) = picture {
        item.image_tags.insert("Primary", tag);
    }
    item
}

/// Whether the caller changes collections: with `manage` ([`super::JfManager`]).
fn manages(user: &AuthUser) -> bool {
    user.compatibility_allows(crate::auth::compatibility::Scope::Manage)
}

/// A collection's picture tag: when it last changed, and whose list it was
/// in. Its picture is its own or that of its first title the user may see,
/// and clients load it without a token, so the tag says whose eyes choose.
fn collection_tag(jellyfin: &Jellyfin, updated_at: i64, user: &AuthUser) -> String {
    format!("{updated_at:x}{}", jellyfin.ids.of(Kind::User, user.id))
}

async fn items(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    user_path: Option<Path<String>>,
    q: Q,
) -> JfResult<Response> {
    if let Some(Path(id)) = user_path {
        own(&jellyfin, &user, &id)?;
    }
    let start = page(&q).start;
    if asks_for_libraries(&q) {
        let items: Vec<Item> =
            libraries(&state, &user).await?.iter().map(|lib| Item::library(&jellyfin, lib)).collect();
        let total = items.len();
        return Ok(answer(items, total, 0));
    }
    if let Some((items, total)) = lists(&state, &jellyfin, &user, &q).await? {
        return Ok(answer(items, total, start));
    }
    // A playlist's titles, as some apps list them, with their entries' ids.
    let playlist = q.get("parentId").and_then(|p| jellyfin.ids.number(Kind::Playlist, p));
    if let Some(playlist) = playlist.filter(|_| q.get("ids").is_none() && q.get("searchTerm").is_none()) {
        return playlist_entries(&state, &jellyfin, &user, playlist, &q).await;
    }
    let (rows, total) = list(&state, &jellyfin, &user, &q).await?;
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, start))
}

/// `/Items/Latest`: the newest titles of a library, or of every library the
/// user sees, a bare list: shows by their newest episode, as Pley's
/// Recently Added has them.
async fn latest(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    user_path: Option<Path<String>>,
    q: Q,
) -> JfResult<Response> {
    if let Some(Path(id)) = user_path {
        own(&jellyfin, &user, &id)?;
    }
    let limit = q.number("limit").unwrap_or(16).clamp(1, 100);
    let mut libs = libraries(&state, &user).await?;
    if let Some(parent) = q.get("parentId") {
        let id = jellyfin.ids.number(Kind::Library, parent).ok_or(JfError::NotFound)?;
        libs.retain(|l| l.id == id);
        if libs.is_empty() {
            return Err(JfError::NotFound);
        }
    }
    let mut rows = Vec::new();
    for lib in &libs {
        rows.extend(catalog::recently_added(&state.db, user.id, lib.id, lib.kind().as_str(), limit).await?);
    }
    // One library's are in its order already; several are merged.
    if libs.len() > 1 {
        rows.sort_by_key(|r| std::cmp::Reverse(r.added_at));
    }
    rows.truncate(limit as usize);
    Ok(Json(dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?).into_response())
}

/// `/UserItems/Resume`: movies and episodes in progress.
async fn resume(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    user_path: Option<Path<String>>,
    q: Q,
) -> JfResult<Response> {
    if let Some(Path(id)) = user_path {
        own(&jellyfin, &user, &id)?;
    }
    let library = q.get("parentId").and_then(|p| jellyfin.ids.number(Kind::Library, p));
    let audio_only =
        q.list("mediaTypes").iter().all(|m| m.eq_ignore_ascii_case("audio")) && q.get("mediaTypes").is_some();
    if audio_only {
        return Ok(answer(Vec::new(), 0, 0));
    }
    let libs = libraries(&state, &user).await?;
    let rows: Vec<ItemRow> = catalog::in_progress(&state.db, user.id, library, 100)
        .await?
        .into_iter()
        .filter(|r| libs.iter().any(|l| l.id == r.library_id))
        .collect();
    let page = page(&q);
    let (rows, total) = page.apply(rows);
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, page.start))
}

/// `/Shows/NextUp`: the next episode of the shows the user is watching, or
/// of one show (`SeriesId`).
async fn next_up(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let libs = libraries(&state, &user).await?;
    let rows: Vec<ItemRow> = if let Some(series) = q.get("seriesId") {
        let show = visible_item(&state, &jellyfin, &user, series).await?;
        let next = catalog::on_deck(&state.db, user.id, &show).await?;
        catalog::items_by_ids(&state.db, user.id, &next.into_iter().collect::<Vec<_>>()).await?
    } else {
        let library = q.get("parentId").and_then(|p| jellyfin.ids.number(Kind::Library, p));
        let limit = q.number("limit").unwrap_or(20).clamp(1, 100);
        let next: Vec<i64> =
            catalog::next_up(&state.db, user.id, library, limit).await?.into_iter().map(|(e, _)| e).collect();
        catalog::items_by_ids(&state.db, user.id, &next).await?
    };
    let rows: Vec<ItemRow> = rows.into_iter().filter(|r| libs.iter().any(|l| l.id == r.library_id)).collect();
    let total = rows.len();
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, 0))
}

async fn seasons(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<Response> {
    let show = visible_item(&state, &jellyfin, &user, &id).await?;
    let rows: Vec<ItemRow> =
        catalog::children(&state.db, user.id, show.id).await?.into_iter().filter(|r| r.kind == "season").collect();
    let total = rows.len();
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, 0))
}

/// `/Shows/{id}/Episodes`: a season's (`SeasonId`, or `Season` by number) or
/// the whole show's.
async fn episodes(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<Response> {
    let show = visible_item(&state, &jellyfin, &user, &id).await?;
    let seasons = catalog::children(&state.db, user.id, show.id).await?;
    let season = q.get("seasonId").and_then(|s| jellyfin.ids.number(Kind::Item, s));
    let number = q.number("season");
    let mut rows = Vec::new();
    for s in seasons.iter().filter(|s| s.kind == "season") {
        if season.is_some_and(|id| id != s.id) || number.is_some_and(|n| Some(n) != s.idx) {
            continue;
        }
        rows.extend(catalog::children(&state.db, user.id, s.id).await?);
    }
    if let Some(start) = q.get("startItemId").and_then(|i| jellyfin.ids.number(Kind::Item, i))
        && let Some(at) = rows.iter().position(|r| r.id == start)
    {
        rows.drain(..at);
    }
    let page = page(&q);
    let (rows, total) = page.apply(rows);
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, page.start))
}

/// `/Items/Suggestions`: titles to try, newest first, not watched yet.
async fn suggestions(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    user_path: Option<Path<String>>,
    q: Q,
) -> JfResult<Response> {
    if let Some(Path(id)) = user_path {
        own(&jellyfin, &user, &id)?;
    }
    let limit = q.number("limit").unwrap_or(12).clamp(1, 50) as usize;
    let mut rows = Vec::new();
    for lib in libraries(&state, &user).await? {
        if lib.kind() == crate::library::LibraryKind::Music {
            continue;
        }
        let found = catalog::recently_added(&state.db, user.id, lib.id, lib.kind().as_str(), limit as i64 * 2).await?;
        rows.extend(
            found
                .into_iter()
                .filter(|r| matches!(r.kind.as_str(), "movie" | "show") && !r.watched && r.position_ms == 0),
        );
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.added_at));
    rows.truncate(limit);
    let total = rows.len();
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, 0))
}

/// Lists Pley has nothing in: an item's intros, additional parts, theme
/// media, the collections it is in, episodes yet to air, tags.
async fn nothing(_user: JfUser) -> Response {
    answer(Vec::new(), 0, 0)
}

// ------------------------------------------------------------------ one item

/// An id that names something besides an item: a library, a person, a
/// genre, a collection or a playlist, as clients open them.
async fn other(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, kind: Kind, id: i64) -> JfResult<Item> {
    match kind {
        Kind::Library => {
            let libs = libraries(state, user).await?;
            let lib = libs.iter().find(|l| l.id == id).ok_or(JfError::NotFound)?;
            Ok(Item::library(jellyfin, lib))
        }
        Kind::Person => {
            let person = crate::people::person(&state.db, user.id, id).await?.ok_or(JfError::NotFound)?;
            let mut item = Item {
                name: person.name.clone(),
                server_id: jellyfin.server_id.clone(),
                id: jellyfin.ids.of(Kind::Person, id),
                sort_name: Some(person.name.to_lowercase()),
                is_folder: false,
                kind: "Person",
                media_type: "Unknown",
                child_count: Some(person.credits.len() as i64),
                ..Default::default()
            };
            if let Some(url) = &person.photo_url {
                item.image_tags.insert("Primary", super::users::tag_of(url));
            }
            Ok(item)
        }
        Kind::Collection => {
            let (c, _) = crate::collections::get(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
            let tag = collection_tag(jellyfin, c.updated_at, user);
            let mut collection = list_item(
                jellyfin,
                Kind::Collection,
                c.id,
                &c.title,
                "BoxSet",
                c.item_count,
                c.updated_at,
                c.created_at,
                c.summary.clone(),
                Some(tag),
            );
            collection.can_delete = manages(user);
            Ok(collection)
        }
        Kind::Playlist => {
            let (p, _) = crate::playlists::get_rows(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
            Ok(list_item(
                jellyfin,
                Kind::Playlist,
                p.id,
                &p.title,
                "Playlist",
                p.item_count,
                p.updated_at,
                p.created_at,
                None,
                None,
            ))
        }
        _ => Err(JfError::NotFound),
    }
}

async fn details(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, id: &str) -> JfResult<Response> {
    match jellyfin.ids.read(id) {
        Some((Kind::Item, _)) => {
            let row = visible_item(state, jellyfin, user, id).await?;
            // Opened, as on Pley's own item page: its seek previews are made.
            if matches!(row.kind.as_str(), "movie" | "episode") {
                for file in catalog::files_for_item(&state.db, row.id).await? {
                    state.trickplay.request(&file);
                }
            }
            let mut items = dtos(state, jellyfin, user, std::slice::from_ref(&row), Fields::all()).await?;
            let mut item = items.remove(0);
            // What an editor sends back: the locks and the labels (to editors
            // only, see `labels::shown_to`).
            let (locked, all, sort) = super::editing::locks(state, row.id).await?;
            item.locked_fields = Some(locked);
            item.lock_data = Some(all);
            item.forced_sort_name = sort;
            if crate::labels::shown_to(user) {
                item.tags =
                    Some(crate::labels::for_items(&state.db, &[row.id]).await?.remove(&row.id).unwrap_or_default());
            }
            Ok(Json(item).into_response())
        }
        Some((kind, number)) => Ok(Json(other(state, jellyfin, user, kind, number).await?).into_response()),
        None => Err(JfError::NotFound),
    }
}

async fn item(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    details(&state, &jellyfin, &user, &id).await
}

async fn user_item(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((user_id, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&jellyfin, &user, &user_id)?;
    details(&state, &jellyfin, &user, &id).await
}

/// `/Items/{id}/Similar`: what Pley's own page shows as More Like This.
async fn similar(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<Response> {
    let row = visible_item(&state, &jellyfin, &user, &id).await?;
    let limit = q.number("limit").unwrap_or(12).clamp(1, 50) as usize;
    let rows = crate::related::related(&state.db, user.id, row.id, limit).await?;
    let total = rows.len();
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, 0))
}

async fn extras_of(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: &str,
    trailers: bool,
) -> JfResult<Response> {
    let row = visible_item(state, jellyfin, user, id).await?;
    let rows: Vec<ItemRow> = catalog::extras(&state.db, user.id, row.id)
        .await?
        .into_iter()
        .filter(|r| (r.extra_type.as_deref() == Some("trailer")) == trailers)
        .collect();
    Ok(Json(dtos(state, jellyfin, user, &rows, Fields { media_sources: true, ..Fields::default() }).await?)
        .into_response())
}

async fn trailers(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    extras_of(&state, &jellyfin, &user, &id, true).await
}

async fn user_trailers(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((user_id, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&jellyfin, &user, &user_id)?;
    extras_of(&state, &jellyfin, &user, &id, true).await
}

async fn special_features(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    extras_of(&state, &jellyfin, &user, &id, false).await
}

async fn user_special_features(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((user_id, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&jellyfin, &user, &user_id)?;
    extras_of(&state, &jellyfin, &user, &id, false).await
}

/// `/Items/{id}/Ancestors`: its parents up to its library.
async fn ancestors(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    let row = visible_item(&state, &jellyfin, &user, &id).await?;
    let up: Vec<i64> = [row.parent_id, row.grandparent_id].into_iter().flatten().collect();
    let rows = catalog::items_by_ids(&state.db, user.id, &up).await?;
    let mut items = dtos(&state, &jellyfin, &user, &rows, Fields::default()).await?;
    if let Some(lib) = libraries(&state, &user).await?.iter().find(|l| l.id == row.library_id) {
        items.push(Item::library(&jellyfin, lib));
    }
    Ok(Json(items).into_response())
}

// ------------------------------------------------------------------ genres, people, search

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LegacyFilters {
    genres: Vec<String>,
    tags: Vec<String>,
    official_ratings: Vec<String>,
    years: Vec<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Filters2 {
    genres: Vec<NameId>,
    tags: Vec<String>,
}

/// The genres and years of the libraries asked for (`ParentId`), or of all.
async fn genres_and_years(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    q: &Q,
) -> JfResult<(Vec<String>, Vec<i64>)> {
    let mut libs = libraries(state, user).await?;
    if let Some(parent) = q.get("parentId") {
        let id = jellyfin.ids.number(Kind::Library, parent).ok_or(JfError::NotFound)?;
        libs.retain(|l| l.id == id);
        if libs.is_empty() {
            return Err(JfError::NotFound);
        }
    }
    let wanted: Vec<&'static str> = q.list("includeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let (mut genres, mut years) = (Vec::new(), Vec::new());
    for lib in &libs {
        let kind =
            wanted.iter().find(|k| listing::kinds_of(lib).contains(k)).copied().unwrap_or(lib.kind().top_level_kind());
        let (g, y) = catalog::library_filters(&state.db, user.id, lib.id, kind).await?;
        genres.extend(g);
        years.extend(y);
    }
    genres.sort_by_key(|g| g.to_lowercase());
    genres.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    years.sort_unstable_by(|a, b| b.cmp(a));
    years.dedup();
    Ok((genres, years))
}

async fn filters(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let (genres, years) = genres_and_years(&state, &jellyfin, &user, &q).await?;
    Ok(Json(LegacyFilters { genres, tags: Vec::new(), official_ratings: Vec::new(), years }).into_response())
}

async fn filters2(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let (genres, _) = genres_and_years(&state, &jellyfin, &user, &q).await?;
    let genres = genres
        .into_iter()
        .map(|g| NameId { id: jellyfin.ids.of(Kind::Genre, super::ids::genre_number(&g)), name: g })
        .collect();
    Ok(Json(Filters2 { genres, tags: Vec::new() }).into_response())
}

/// `/Genres`: genre items, which clients list and filter by name.
async fn genres(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let (genres, _) = genres_and_years(&state, &jellyfin, &user, &q).await?;
    let items: Vec<Item> = genres
        .into_iter()
        .map(|g| Item {
            id: jellyfin.ids.of(Kind::Genre, super::ids::genre_number(&g)),
            sort_name: Some(g.to_lowercase()),
            name: g,
            server_id: jellyfin.server_id.clone(),
            is_folder: true,
            kind: "Genre",
            media_type: "Unknown",
            ..Default::default()
        })
        .collect();
    let page = page(&q);
    let (items, total) = page.apply(items);
    Ok(answer(items, total, page.start))
}

/// `/Artists`, `/Artists/AlbumArtists`: the artists of the music libraries
/// asked for (`ParentId`), or of all.
async fn artists(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let mut libs: Vec<LibraryRow> = libraries(&state, &user)
        .await?
        .into_iter()
        .filter(|l| l.kind() == crate::library::LibraryKind::Music)
        .collect();
    if let Some(parent) = q.get("parentId") {
        let id = jellyfin.ids.number(Kind::Library, parent).ok_or(JfError::NotFound)?;
        libs.retain(|l| l.id == id);
        if libs.is_empty() {
            return Err(JfError::NotFound);
        }
    }
    let mut params = vec![("type".to_owned(), listing::type_number("artist").to_string())];
    let sort = sort_of(&q);
    if !sort.is_empty() && sort != "random" {
        params.push(("sort".into(), sort));
    }
    if let Some(term) = q.get("searchTerm") {
        params.push(("title".into(), term.to_owned()));
    }
    let params = Params(params);
    let page = page(&q);
    let several = libs.len() > 1;
    let window = if several { Page { start: 0, size: page.size.map(|s| s + page.start) } } else { page };
    let (mut rows, mut total) = (Vec::new(), 0);
    for lib in &libs {
        let (found, count) =
            SectionQuery::asked_by(&user, lib.id, "artist", &params)?.page(&state.db, user.id, &window).await?;
        rows.extend(found);
        total += count;
    }
    if several {
        let sort = listing::parse_sort(params.get("sort"));
        listing::sort_rows(&mut rows, &sort, &Default::default());
        rows = rows.into_iter().skip(page.start).take(page.size.unwrap_or(usize::MAX)).collect();
    }
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::of(&q)).await?;
    Ok(answer(items, total, page.start))
}

/// `/Years`: the years of the titles the user may see, newest first, as
/// items clients list and filter by (`/Items?Years=`).
async fn years(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let (_, years) = genres_and_years(&state, &jellyfin, &user, &q).await?;
    let items: Vec<Item> = years
        .into_iter()
        .map(|year| Item {
            id: jellyfin.ids.of(Kind::Year, year),
            name: year.to_string(),
            sort_name: Some(year.to_string()),
            production_year: Some(year),
            server_id: jellyfin.server_id.clone(),
            is_folder: true,
            kind: "Year",
            media_type: "Unknown",
            ..Default::default()
        })
        .collect();
    let page = page(&q);
    let (items, total) = page.apply(items);
    Ok(answer(items, total, page.start))
}

/// `/Studios`: the studios of the titles the user may see.
async fn studios(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let libs = libraries(&state, &user).await?;
    let mut names: Vec<String> = Vec::new();
    for lib in libs.iter().filter(|l| l.kind() != crate::library::LibraryKind::Music) {
        let found: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "{} SELECT DISTINCT i.studio FROM {} WHERE {} AND i.library_id = ? AND i.kind = ? AND i.studio IS NOT NULL",
            catalog::ME_CTE,
            catalog::VISIBLE_ITEMS,
            catalog::VISIBLE
        )))
        .bind(user.id)
        .bind(lib.id)
        .bind(lib.kind().top_level_kind())
        .fetch_all(&state.db.read)
        .await?;
        names.extend(found);
    }
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let items: Vec<Item> = names
        .into_iter()
        .map(|n| Item {
            id: jellyfin.ids.of(Kind::Studio, super::ids::genre_number(&n)),
            sort_name: Some(n.to_lowercase()),
            name: n,
            server_id: jellyfin.server_id.clone(),
            is_folder: true,
            kind: "Studio",
            media_type: "Unknown",
            ..Default::default()
        })
        .collect();
    let page = page(&q);
    let (items, total) = page.apply(items);
    Ok(answer(items, total, page.start))
}

/// `/Persons?SearchTerm=`: people credited in what the user may see.
async fn persons(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let Some(term) = q.get("searchTerm") else { return Ok(answer(Vec::new(), 0, 0)) };
    let limit = q.number("limit").unwrap_or(20).clamp(1, 100);
    let found = crate::search::people(&state.db, user.id, term, limit).await?;
    let items: Vec<Item> = found
        .into_iter()
        .map(|p| {
            let mut item = Item {
                id: jellyfin.ids.of(Kind::Person, p.id),
                sort_name: Some(p.name.to_lowercase()),
                name: p.name,
                server_id: jellyfin.server_id.clone(),
                kind: "Person",
                media_type: "Unknown",
                ..Default::default()
            };
            if let Some(url) = &p.photo_url {
                item.image_tags.insert("Primary", super::users::tag_of(url));
            }
            item
        })
        .collect();
    let total = items.len();
    Ok(answer(items, total, 0))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SearchHint {
    item_id: String,
    id: String,
    name: String,
    #[serde(rename = "Type")]
    kind: &'static str,
    media_type: &'static str,
    is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    album_artist: Option<String>,
    /// Always there, as the Kotlin SDK requires.
    artists: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SearchHints {
    search_hints: Vec<SearchHint>,
    total_record_count: usize,
}

async fn search_hints(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let term = q.get("searchTerm").unwrap_or_default();
    let limit = q.number("limit").unwrap_or(20).clamp(1, 100) as usize;
    let wanted: Vec<&'static str> = q.list("includeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let libs = libraries(&state, &user).await?;
    let rows: Vec<ItemRow> = crate::search::items(&state.db, user.id, term, 200)
        .await?
        .into_iter()
        .filter(|r| {
            libs.iter().any(|l| l.id == r.library_id) && (wanted.is_empty() || wanted.contains(&r.kind.as_str()))
        })
        .take(limit)
        .collect();
    let items = dtos(&state, &jellyfin, &user, &rows, Fields::default()).await?;
    let hints: Vec<SearchHint> = items
        .into_iter()
        .zip(&rows)
        .map(|(item, row)| SearchHint {
            item_id: item.id.clone(),
            id: item.id,
            name: item.name,
            kind: item.kind,
            media_type: item.media_type,
            is_folder: item.is_folder,
            production_year: item.production_year,
            primary_image_tag: item.image_tags.get("Primary").cloned(),
            run_time_ticks: item.run_time_ticks,
            series: item.series_name,
            album: (row.kind == "track").then(|| row.parent_title.clone()).flatten(),
            album_artist: item.album_artist,
            artists: item.artists.unwrap_or_default(),
        })
        .collect();
    let total = hints.len();
    Ok(Json(SearchHints { search_hints: hints, total_record_count: total }).into_response())
}

async fn playlist_items(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<Response> {
    let id = jellyfin.ids.number(Kind::Playlist, &id).ok_or(JfError::NotFound)?;
    playlist_entries(&state, &jellyfin, &user, id, &q).await
}

/// The entries of playlist `id` the user sees, in its order, each with its
/// own id (`PlaylistItemId`), by which apps remove and move it: an item may
/// be there more than once. `IncludeItemTypes` and `ExcludeItemTypes`
/// narrow them, as other lists.
async fn playlist_entries(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: i64,
    q: &Q,
) -> JfResult<Response> {
    let (_, mut entries) = crate::playlists::get_rows(&state.db, user, id).await?.ok_or(JfError::NotFound)?;
    let wanted: Vec<&'static str> = q.list("includeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    let excluded: Vec<&'static str> = q.list("excludeItemTypes").iter().filter_map(|t| dto::kind_of_type(t)).collect();
    entries.retain(|(_, row)| {
        (wanted.is_empty() || wanted.contains(&row.kind.as_str())) && !excluded.contains(&row.kind.as_str())
    });
    let page = page(q);
    let (entries, total) = page.apply(entries);
    let (keys, rows): (Vec<i64>, Vec<ItemRow>) = entries.into_iter().unzip();
    let mut items = dtos(state, jellyfin, user, &rows, Fields::of(q)).await?;
    for (item, entry) in items.iter_mut().zip(keys) {
        item.playlist_item_id = Some(jellyfin.ids.of(Kind::PlaylistEntry, entry));
    }
    Ok(answer(items, total, page.start))
}

// ------------------------------------------------------------------ pictures

/// Pley's artwork kind and Jellyfin's image type: a poster is Primary (an
/// episode's still), a backdrop Backdrop, a frame of the video Thumb.
pub(super) fn artwork_kind(row_kind: &str, image: &str) -> Option<&'static str> {
    Some(match image {
        "primary" if matches!(row_kind, "episode" | "extra") => "thumb",
        "primary" => "poster",
        "backdrop" | "art" => "backdrop",
        "thumb" if matches!(row_kind, "movie" | "episode" | "extra") => "thumb",
        "thumb" => "backdrop",
        _ => return None,
    })
}

/// The width a picture is made at: `maxWidth`, `fillWidth` or `width`.
fn image_width(q: &Q) -> Option<u32> {
    ["maxWidth", "fillWidth", "width"]
        .iter()
        .find_map(|name| q.number(name))
        .filter(|w| *w > 0)
        .map(|w| w.min(3840) as u32)
}

/// A picture of an item, a person or a collection. Clients load pictures
/// without a token, as Jellyfin lets them: an id can't be guessed
/// ([`super::ids`]) and is given only to those who may see what it names.
/// With a token, what it names must be one the user may see.
/// `viewer`: the user of the request's token, if it has one.
async fn picture(
    state: &AppState,
    jellyfin: &Jellyfin,
    id: &str,
    image: &str,
    q: &Q,
    headers: &HeaderMap,
    viewer: Option<AuthUser>,
) -> JfResult<Response> {
    let image = image.to_ascii_lowercase();
    let versioned = q.get("tag").is_some();
    match jellyfin.ids.read(id) {
        Some((Kind::Item, number)) => {
            let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM items WHERE id = ?")
                .bind(number)
                .fetch_optional(&state.db.read)
                .await?;
            let kind = kind.ok_or(JfError::NotFound)?;
            if let Some(user) = &viewer {
                visible_item(state, jellyfin, user, id).await?;
            }
            let artwork = artwork_kind(&kind, &image).ok_or(JfError::NotFound)?;
            Ok(crate::api::items::artwork(state, number, artwork, image_width(q), versioned, headers).await?)
        }
        Some((Kind::Person, number)) if image == "primary" => {
            if let Some(user) = &viewer
                && !crate::people::is_visible(&state.db, user.id, number).await?
            {
                return Err(JfError::NotFound);
            }
            Ok(crate::api::people::photo_of(state, number, image_width(q), versioned, headers).await?)
        }
        Some((Kind::Collection, number)) => {
            // The user it was listed to, from its tag ([`collection_tag`]).
            let viewer = match viewer {
                Some(user) => user,
                None => {
                    let tag = q.get("tag").ok_or(JfError::NotFound)?;
                    let at =
                        tag.len().checked_sub(32).filter(|at| tag.is_char_boundary(*at)).ok_or(JfError::NotFound)?;
                    let listed_to = jellyfin.ids.number(Kind::User, &tag[at..]).ok_or(JfError::NotFound)?;
                    AuthUser::load(&state.db, listed_to).await?.ok_or(JfError::NotFound)?
                }
            };
            let kind = if image == "backdrop" { "backdrop" } else { "poster" };
            match crate::collections::artwork(&state.db, &state.store, &viewer, number, kind).await? {
                Some(Ok(path)) => {
                    Ok(crate::api::collections::serve_file(state, &path, image_width(q), versioned, headers).await?)
                }
                Some(Err(item)) => {
                    Ok(crate::api::items::artwork(state, item, kind, image_width(q), versioned, headers).await?)
                }
                None => Err(JfError::NotFound),
            }
        }
        _ => Err(JfError::NotFound),
    }
}

async fn image(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, kind)): Path<(String, String)>,
    SourceIp(from): SourceIp,
    q: Q,
    headers: HeaderMap,
    uri: Uri,
) -> JfResult<Response> {
    let viewer = super::token_user(&state, &headers, uri.query(), from).await?;
    picture(&state, &jellyfin, &id, &kind, &q, &headers, viewer).await
}

async fn image_at(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, kind, index)): Path<(String, String, String)>,
    SourceIp(from): SourceIp,
    q: Q,
    headers: HeaderMap,
    uri: Uri,
) -> JfResult<Response> {
    // Pley keeps one backdrop and one picture of each kind.
    if index != "0" {
        return Err(JfError::NotFound);
    }
    let viewer = super::token_user(&state, &headers, uri.query(), from).await?;
    picture(&state, &jellyfin, &id, &kind, &q, &headers, viewer).await
}

// ------------------------------------------------------------------ watch state

async fn user_data_of(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, id: &str) -> JfResult<dto::UserData> {
    let row = visible_item(state, jellyfin, user, id).await?;
    let favorite = catalog::in_watchlist(&state.db, user.id, row.id).await?;
    Ok(dto::user_data(jellyfin, &row, favorite, row.duration_ms))
}

async fn mark(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, id: &str, watched: bool) -> JfResult<Response> {
    let row = visible_item(state, jellyfin, user, id).await?;
    if catalog::set_watched(&state.db, &state.events, user.id, row.id, &row.kind, watched).await? && watched {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(user_data_of(state, jellyfin, user, id).await?).into_response())
}

async fn played(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    mark(&s, &j, &user, &id, true).await
}

async fn unplayed(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    mark(&s, &j, &user, &id, false).await
}

async fn user_played(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    mark(&s, &j, &user, &id, true).await
}

async fn user_unplayed(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    mark(&s, &j, &user, &id, false).await
}

/// Favorites are My List, which holds movies and shows.
async fn set_favorite(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: &str,
    on: bool,
) -> JfResult<Response> {
    let row = visible_item(state, jellyfin, user, id).await?;
    if !matches!(row.kind.as_str(), "movie" | "show") {
        return Err(JfError::BadRequest("only movies and shows go in My List".into()));
    }
    let changed = if on {
        catalog::add_to_watchlist(&state.db, user.id, row.id).await?
    } else {
        catalog::remove_from_watchlist(&state.db, user.id, row.id).await?
    };
    if changed {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }
    Ok(Json(user_data_of(state, jellyfin, user, id).await?).into_response())
}

async fn favorite(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    set_favorite(&s, &j, &user, &id, true).await
}

async fn unfavorite(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    set_favorite(&s, &j, &user, &id, false).await
}

async fn user_favorite(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    set_favorite(&s, &j, &user, &id, true).await
}

async fn user_unfavorite(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    set_favorite(&s, &j, &user, &id, false).await
}

async fn get_user_data(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    Ok(Json(user_data_of(&s, &j, &user, &id).await?).into_response())
}

async fn user_get_user_data(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    Ok(Json(user_data_of(&s, &j, &user, &id).await?).into_response())
}

/// `POST /UserItems/{id}/UserData`: played, favorite, or the resume point
/// set or cleared (`PlaybackPositionTicks`). Sync tools such as WatchState
/// copy a place in a video from another server this way, sending the ticks
/// as text, with when it was played (`LastPlayedDate`), which Pley keeps.
async fn update_user_data(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: &str,
    body: Body,
) -> JfResult<Response> {
    let row = visible_item(state, jellyfin, user, id).await?;
    // What can't be read is refused, as Jellyfin does: a sync tool told it
    // worked would not send it again.
    let given = |name: &str| body.get(name).is_some_and(|v| !v.is_null());
    let ticks = match body.number("PlaybackPositionTicks") {
        None if given("PlaybackPositionTicks") => {
            return Err(JfError::BadRequest("PlaybackPositionTicks is not a whole number".into()));
        }
        ticks => ticks,
    };
    let last_played = match body.str("LastPlayedDate").and_then(crate::util::iso_secs) {
        None if given("LastPlayedDate") => return Err(JfError::BadRequest("LastPlayedDate is not a date".into())),
        at => at,
    };
    // Apps that post back the whole UserData they were given, to change one
    // thing, send what it holds too: only what differs changes. (Marking
    // unplayed clears the place, which `Played: false` as given mustn't, and
    // watched takes a movie off My List, which `IsFavorite: true` as given
    // mustn't put back.)
    let played = body.flag("Played");
    let marked = played.filter(|played| *played != dto::played(&row));
    let favorite = match body.flag("IsFavorite") {
        Some(favorite) if matches!(row.kind.as_str(), "movie" | "show") => {
            Some((favorite, catalog::in_watchlist(&state.db, user.id, row.id).await?))
        }
        _ => None,
    };
    if let Some(played) = marked {
        mark(state, jellyfin, user, id, played).await?;
    }
    if let Some((favorite, listed)) = favorite
        && favorite != listed
    {
        set_favorite(state, jellyfin, user, id, favorite).await?;
    }
    // Only what plays has a place, as with players' reports, and only a
    // place other than the one kept moves it (one just marked has none).
    let kept_ms = if marked.is_some() { 0 } else { row.position_ms };
    let moved = ticks
        .map(|t| t.max(0) / dto::TICKS_PER_MS)
        .filter(|ms| played != Some(true) && dto::is_playable(&row.kind) && catalog::resume_point(*ms) != kept_ms);
    let mut counted = false;
    if let Some(ms) = moved {
        let duration = row.duration_ms.unwrap_or(0);
        let recorded =
            catalog::record_progress(&state.db, &state.events, user.id, row.id, ms, duration, None, "stopped").await?;
        if recorded.left_watchlist {
            state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
        }
        counted = recorded.counted;
    }
    // Clocks run ahead, and a time to come would keep the item first in
    // Continue Watching: it is today at the latest.
    if (marked.is_some() || moved.is_some())
        && let Some(at) = last_played.filter(|at| *at > 0)
    {
        let at = at.min(crate::util::now());
        catalog::set_last_played(&state.db, &state.events, user.id, row.id, &row.kind, at, counted).await?;
    }
    Ok(Json(user_data_of(state, jellyfin, user, id).await?).into_response())
}

async fn set_user_data(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    body: Body,
) -> JfResult<Response> {
    update_user_data(&s, &j, &user, &id, body).await
}

async fn user_set_user_data(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
    body: Body,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    update_user_data(&s, &j, &user, &id, body).await
}

/// `POST /UserItems/{id}/Rating?Likes=true|false`: a like is 10 points, a
/// dislike 2 (Pley rates 0-10), and `DELETE` clears it.
async fn set_rating(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    id: &str,
    rating: Option<f64>,
) -> JfResult<Response> {
    let row = visible_item(state, jellyfin, user, id).await?;
    catalog::set_rating(&state.db, user.id, row.id, rating).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Json(user_data_of(state, jellyfin, user, id).await?).into_response())
}

fn likes(q: &Q) -> Option<f64> {
    Some(if q.flag("likes").unwrap_or(true) { 10.0 } else { 2.0 })
}

async fn rate(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<Response> {
    set_rating(&s, &j, &user, &id, likes(&q)).await
}

async fn unrate(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    set_rating(&s, &j, &user, &id, None).await
}

async fn user_rate(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
    q: Q,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    set_rating(&s, &j, &user, &id, likes(&q)).await
}

async fn user_unrate(
    State(s): State<AppState>,
    Extension(j): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((u, id)): Path<(String, String)>,
) -> JfResult<Response> {
    own(&j, &user, &u)?;
    set_rating(&s, &j, &user, &id, None).await
}
