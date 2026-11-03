//! Collections (`/library/sections/{id}/collections`, `/library/collections/…`)
//! and playlists (`/playlists/…`). Read through [`crate::collections`] and
//! [`crate::playlists`], so the same library access and age limits apply.
//! Both are named by `ratingKey`s of their own ([`super::keys`]); paths
//! that name them take their ids as this API gave them before too.

use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use super::collection_search::Search;
use super::keys::{self, Key};
use super::library::{
    base_container, item_nodes, library_for, list_container, listing_detail, serve_image, visible_libraries,
};
use super::meta::Sections;
use super::{Node, Page, Params, PlexAdmin, PlexError, PlexInfo, PlexResult, PlexUser};
use crate::auth::AuthUser;
use crate::catalog::ItemRow;
use crate::collections::{self, CollectionDto};
use crate::labels;
use crate::library::{LibraryKind, LibraryRow};
use crate::playlists::{self, Kind, PlaylistDto};
use crate::state::AppState;

/// Plex's type number of collections (`/library/sections/{id}/all?type=18`).
pub const COLLECTION_TYPE: &str = "18";

/// The collection a `/library/collections/{key}` path names.
pub(super) fn collection_path(key: i64) -> PlexResult<i64> {
    keys::collection_id(key).ok_or(PlexError::NotFound)
}

/// The playlist a `/playlists/{key}` path names.
fn playlist_path(key: i64) -> PlexResult<i64> {
    keys::playlist_id(key).ok_or(PlexError::NotFound)
}

// ------------------------------------------------------------------ collections

/// Plex's collection orders (`collectionSort`) and Pley's.
const SORTS: [(i64, &str); 3] = [(0, "release"), (1, "title"), (2, "custom")];

/// Plex's `collectionMode` of every collection: the library's default.
/// Pley has no modes: collections are listed apart, and their items stay in
/// the library.
const MODE: i64 = -1;

/// Smart collections show each viewer what they may see: Plex's
/// `collectionFilterBasedOnUser` 1.
const FILTER_BY_VIEWER: i64 = 1;

fn sort_number(sort: &str) -> i64 {
    SORTS.iter().find(|(_, s)| *s == sort).map_or(0, |(n, _)| *n)
}

fn collection_node(c: &CollectionDto, lib: &LibraryRow, labels: &[String]) -> Node {
    let subtype = if lib.kind() == LibraryKind::Shows { "show" } else { "movie" };
    let sort = sort_number(&c.sort);
    let key = keys::collection(c.id);
    let mut node = Node::item("Directory")
        .attr("ratingKey", key.to_string())
        .attr("key", format!("/library/collections/{key}/children"))
        .attr("guid", format!("collection://{:024x}", c.id))
        .attr("type", "collection")
        .attr("title", &c.title)
        // Only a sort title set by hand, as Plex does.
        .opt("titleSort", Some(&c.sort_title).filter(|s| **s != crate::util::sort_title(&c.title)))
        .attr("subtype", subtype)
        .opt("summary", c.summary.as_deref())
        .attr("index", c.id)
        .attr("librarySectionID", lib.id)
        .attr("librarySectionTitle", &lib.name)
        .attr("librarySectionKey", format!("/library/sections/{}", lib.id))
        .attr("thumb", format!("/library/collections/{key}/thumb/{}", c.updated_at))
        .attr("art", format!("/library/collections/{key}/art/{}", c.updated_at))
        .attr("childCount", c.item_count)
        .attr("collectionMode", MODE)
        .attr("collectionSort", sort)
        .opt("collectionFilterBasedOnUser", c.smart_filter.is_some().then_some(FILTER_BY_VIEWER))
        .attr("smart", c.smart_filter.is_some())
        // A smart collection's filter, which python-plexapi reads back.
        .opt("content", c.smart_filter.as_ref().map(|f| format!("/library/sections/{}/all?{f}", lib.id)))
        .attr("addedAt", c.created_at)
        .attr("updatedAt", c.updated_at);
    for label in labels {
        node.push(Node::new("Label").attr("tag", label));
    }
    node
}

/// The collection's labels, for [`collection_node`].
async fn labels_of(state: &AppState, id: i64) -> PlexResult<Vec<String>> {
    Ok(labels::for_collections(&state.db, &[id]).await?.remove(&id).unwrap_or_default())
}

/// `/library/sections/{id}/collections`, also `/all?type=18`, with the
/// filters and sorts of [`Search`].
pub async fn section_collections(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let search = Search::parse(&params)?;
    let all = collections::list(&state.db, &super::collections_viewer(&user), lib.id).await?;
    let ids: Vec<i64> = all.iter().map(|c| c.id).collect();
    let labels = labels::for_collections(&state.db, &ids).await?;
    let (rows, total) = page.apply(search.apply(all, &labels));
    let nodes = rows
        .iter()
        .map(|c| collection_node(c, &lib, labels.get(&c.id).map(Vec::as_slice).unwrap_or_default()))
        .collect();
    let mut mc = list_container(Some(&lib), &info, nodes, total, &page)
        .attr("title2", "All Collections")
        .attr("viewGroup", "secondary");
    if params.flag("includeMeta") {
        mc.push(super::collection_search::meta(lib.id));
    }
    Ok(mc)
}

/// Labels of the collections of `lib` the user sees (filter choices).
pub(super) async fn collection_labels(state: &AppState, user: &AuthUser, lib: &LibraryRow) -> PlexResult<Vec<String>> {
    let all = collections::list(&state.db, &super::collections_viewer(user), lib.id).await?;
    let ids: Vec<i64> = all.iter().map(|c| c.id).collect();
    let mut found: Vec<String> = labels::for_collections(&state.db, &ids).await?.into_values().flatten().collect();
    found.sort_by_key(|l| l.to_lowercase());
    found.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(found)
}

pub(super) async fn collection_for(
    state: &AppState,
    user: &AuthUser,
    id: i64,
) -> PlexResult<(CollectionDto, Vec<ItemRow>, LibraryRow, Vec<LibraryRow>)> {
    let (collection, items) =
        collections::get(&state.db, &super::collections_viewer(user), id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(state, user).await?;
    let lib = libraries.iter().find(|l| l.id == collection.library_id).cloned().ok_or(PlexError::NotFound)?;
    Ok((collection, items, lib, libraries))
}

/// The collection as details list it (`/library/collections/{key}`, and
/// `/library/metadata/{key}` for its key), with its preferences on
/// `includePreferences=1`.
pub(super) async fn collection_details(
    state: &AppState,
    user: &AuthUser,
    info: &PlexInfo,
    id: i64,
    params: &Params,
) -> PlexResult<Node> {
    let (collection, _, lib, _) = collection_for(state, user, id).await?;
    let mut node = collection_node(&collection, &lib, &labels_of(state, id).await?);
    if params.flag("includePreferences") {
        node.push(preferences(&collection));
    }
    Ok(list_container(Some(&lib), info, vec![node], 1, &Page::default()))
}

/// A collection's preferences, as python-plexapi's `editAdvanced()` reads
/// them before it changes one. Each lists only the values Pley takes, so
/// that python-plexapi refuses another before it asks.
fn preferences(c: &CollectionDto) -> Node {
    let smart = c.smart_filter.is_some();
    let sorts = ["0:Release date", "1:Alphabetical", "2:Custom"];
    let sort = sort_number(&c.sort);
    // A smart collection keeps its filter's order.
    let sort_values = match usize::try_from(sort).ok().and_then(|n| sorts.get(n)) {
        Some(own) if smart => (*own).to_owned(),
        _ => sorts.join("|"),
    };
    let setting = |id: &'static str, label: &'static str, default: i64, value: i64, values: &str| {
        Node::new("Setting")
            .attr("id", id)
            .attr("label", label)
            .attr("summary", "")
            .attr("type", "int")
            .attr("default", default)
            .attr("value", value)
            .attr("hidden", false)
            .attr("advanced", false)
            .attr("group", "")
            .attr("enumValues", values)
    };
    let mut prefs = Node::new("Preferences")
        .single()
        // Pley has no collection modes but the library's default.
        .child(setting("collectionMode", "Collection mode", MODE, MODE, "-1:Library default"))
        .child(setting("collectionSort", "Collection order", 0, sort, &sort_values));
    if smart {
        prefs.push(setting(
            "collectionFilterBasedOnUser",
            "Filter collection based on",
            0,
            FILTER_BY_VIEWER,
            "1:Current viewer",
        ));
    }
    prefs
}

/// `/library/collections/{key}`
pub async fn collection(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(key): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    collection_details(&state, &user, &info, collection_path(key)?, &params).await
}

/// `PUT /library/collections/{key}/prefs?collectionSort=…`, as
/// python-plexapi's `sortUpdate()` and `modeUpdate()` send it: the order is
/// Pley's own (release, title, custom). Pley has no collection modes, and
/// its smart collections always show each viewer what they may see, so
/// changing either answers `400`. Nothing is changed unless all of it can be.
pub async fn collection_prefs(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(key): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    let id = collection_path(key)?;
    let (collection, _, lib, _) = collection_for(&state, &admin, id).await?;
    let smart = collection.smart_filter.is_some();
    let mut sort = None;
    for (name, value) in &params.0 {
        if name.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("x-plex-")) {
            continue;
        }
        let number: i64 =
            value.trim().parse().map_err(|_| PlexError::BadRequest(format!("`{name}` takes a number")))?;
        match name.as_str() {
            "collectionMode" if number == MODE => {}
            "collectionMode" => return Err(PlexError::BadRequest("Pley has no collection modes".into())),
            "collectionSort" => {
                let (_, to) = SORTS
                    .iter()
                    .find(|(n, _)| *n == number)
                    .ok_or_else(|| PlexError::BadRequest("collectionSort is 0, 1 or 2".into()))?;
                if smart && *to != collection.sort {
                    return Err(PlexError::BadRequest("a smart collection is in its filter's order".into()));
                }
                sort = Some(*to);
            }
            "collectionFilterBasedOnUser" if smart && number == FILTER_BY_VIEWER => {}
            "collectionFilterBasedOnUser" => {
                return Err(PlexError::BadRequest(
                    "Pley's smart collections show each viewer what they may see".into(),
                ));
            }
            _ => return Err(PlexError::BadRequest(format!("`{name}` isn't a preference of collections"))),
        }
    }
    if let Some(sort) = sort.filter(|s| *s != collection.sort) {
        collections::update(&state.db, id, None, None, Some(sort)).await?;
        state.events.send(crate::events::ServerEvent::LibraryChanged { library_id: lib.id });
    }
    Ok(Node::container().attr("size", 0))
}

/// The items of collection `id`, in its order.
pub(super) async fn collection_items(
    state: &AppState,
    user: &AuthUser,
    info: &PlexInfo,
    id: i64,
    params: &Params,
    page: &Page,
) -> PlexResult<Node> {
    let (collection, items, lib, libraries) = collection_for(state, user, id).await?;
    let (rows, total) = page.apply(items);
    let view_group = if lib.kind() == LibraryKind::Shows { "show" } else { "movie" };
    let nodes = item_nodes(state, &rows, &Sections::new(&libraries), listing_detail(params)).await?;
    let key = keys::collection(collection.id);
    Ok(list_container(Some(&lib), info, nodes, total, page)
        .attr("key", key.to_string())
        .attr("ratingKey", key.to_string())
        .attr("title2", &collection.title)
        .opt("summary", collection.summary.as_deref())
        .attr("thumb", format!("/library/collections/{key}/thumb/{}", collection.updated_at))
        .attr("art", format!("/library/collections/{key}/art/{}", collection.updated_at))
        .attr("viewGroup", view_group))
}

/// `/library/collections/{key}/children`
pub async fn collection_children(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(key): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    collection_items(&state, &user, &info, collection_path(key)?, &params, &page).await
}

/// Collection artwork, or its first visible item's.
pub(super) async fn collection_artwork(
    state: AppState,
    user: AuthUser,
    id: i64,
    image: &str,
    version: Option<String>,
    width: Option<u32>,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let (kind, item_image) = match image {
        "thumb" | "composite" => ("poster", "thumb"),
        "art" => ("backdrop", "art"),
        _ => return Err(PlexError::NotFound),
    };
    match collections::artwork(&state.db, &state.store, &super::collections_viewer(&user), id, kind).await? {
        Some(Ok(path)) => {
            Ok(crate::api::collections::serve_file(&state, &path, width, version.is_some(), &headers).await?)
        }
        // Not versioned: the first item may change without the collection.
        Some(Err(item)) => serve_image(state, user, item, item_image, None, width, headers).await,
        None => Err(PlexError::NotFound),
    }
}

/// `/library/collections/{key}/{thumb|art|composite}`
pub async fn collection_image(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((key, image)): Path<(i64, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    collection_artwork(state, user, collection_path(key)?, &image, None, params.parse("width"), headers).await
}

/// `/library/collections/{key}/{thumb|art|composite}/{version}`
pub async fn collection_image_version(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((key, image, version)): Path<(i64, String, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let id = collection_path(key)?;
    collection_artwork(state, user, id, &image, Some(version), params.parse("width"), headers).await
}

// ------------------------------------------------------------------ playlists

fn playlist_node(p: &PlaylistDto) -> Node {
    let key = keys::playlist(p.id);
    Node::item("Playlist")
        .attr("ratingKey", key.to_string())
        .attr("key", format!("/playlists/{key}/items"))
        .attr("guid", format!("com.plexapp.agents.none://pley-playlist-{}", p.id))
        .attr("type", "playlist")
        .attr("title", &p.title)
        .attr("smart", p.smart_filter.is_some())
        // A smart playlist's filter, which python-plexapi reads back.
        .opt(
            "content",
            p.smart_filter.as_ref().zip(p.library_id).map(|(f, lib)| format!("/library/sections/{lib}/all?{f}")),
        )
        .attr("playlistType", &p.kind)
        .attr("composite", format!("/playlists/{key}/composite/{}", p.updated_at))
        .attr("leafCount", p.item_count)
        .opt("duration", (p.duration_ms > 0).then_some(p.duration_ms))
        .attr("addedAt", p.created_at)
        .attr("updatedAt", p.updated_at)
}

fn playlist_container(playlist: &PlaylistDto) -> Node {
    base_container().attr("size", 1).child(playlist_node(playlist))
}

/// The playlist as `/playlists/{key}` lists it (and `/library/metadata/{key}`
/// for its key).
pub(super) async fn playlist_container_of(state: &AppState, user: &AuthUser, id: i64) -> PlexResult<Node> {
    let (playlist, _) = playlists::get_rows(&state.db, user, id).await?.ok_or(PlexError::NotFound)?;
    Ok(playlist_container(&playlist))
}

/// Collections and playlists among `keys`, as `/library/metadata/{keys}`
/// lists them, with the library of the first collection. Those the user
/// may not see are left out, as items are, and so are those past the first
/// 50: a smart one costs a listing.
pub(super) async fn list_nodes(
    state: &AppState,
    user: &AuthUser,
    keys: &[Key],
) -> PlexResult<(Vec<Node>, Option<LibraryRow>)> {
    let mut nodes = Vec::new();
    let mut first = None;
    for key in keys.iter().filter(|key| !matches!(key, Key::Item(_))).take(50) {
        match *key {
            Key::Collection(id) => match collection_for(state, user, id).await {
                Ok((collection, _, lib, _)) => {
                    nodes.push(collection_node(&collection, &lib, &labels_of(state, id).await?));
                    first.get_or_insert(lib);
                }
                Err(PlexError::NotFound) => {}
                Err(err) => return Err(err),
            },
            Key::Playlist(id) => {
                if let Some((playlist, _)) = playlists::get_rows(&state.db, user, id).await? {
                    nodes.push(playlist_node(&playlist));
                }
            }
            Key::Item(_) => {}
        }
    }
    Ok((nodes, first))
}

/// `/playlists` (`playlistType=video|audio|photo`, `smart=0|1`)
pub async fn list(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let mut rows = playlists::list(&state.db, &user).await?;
    if let Some(kind) = params.get("playlistType") {
        rows.retain(|p| p.kind == kind);
    }
    if let Some(smart) = params.get("smart") {
        rows.retain(|p| p.smart_filter.is_some() == (smart == "1"));
    }
    let mut mc = base_container().attr("size", rows.len());
    mc.extend(rows.iter().map(playlist_node));
    Ok(mc)
}

/// The section and listing query of a Plex `uri` for a smart collection
/// (`server://…/com.plexapp.plugins.library/library/sections/1/all?genre=…`).
fn uri_filter(uri: &str) -> Option<(i64, String)> {
    let start = uri.find("/library/sections/")? + "/library/sections/".len();
    let (section, query) = uri[start..].split_once("/all?")?;
    Some((section.parse().ok()?, query.to_owned()))
}

/// `POST /library/collections?type=1&title=…&smart=0|1&sectionId=…&uri=…`: a
/// collection of the items in `uri`, or a smart one filled by its filter.
pub async fn create_collection(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
) -> PlexResult<Node> {
    let title = params.get("title").unwrap_or_default().trim().chars().take(200).collect::<String>();
    if title.is_empty() {
        return Err(PlexError::BadRequest("title is required".into()));
    }
    let section: i64 =
        params.parse("sectionId").ok_or_else(|| PlexError::BadRequest("sectionId is required".into()))?;
    let lib =
        visible_libraries(&state, &admin).await?.into_iter().find(|l| l.id == section).ok_or(PlexError::NotFound)?;
    if lib.kind() == LibraryKind::Music {
        return Err(PlexError::BadRequest("collections are for movie and TV libraries".into()));
    }
    let uri = params.get("uri").unwrap_or_default();
    let id = if params.flag("smart") {
        let filter = uri_filter(uri)
            .filter(|(s, f)| *s == section && crate::listing::valid_smart_filter(&lib, f))
            .ok_or_else(|| PlexError::BadRequest("a smart collection needs a listing of its section as uri".into()))?
            .1;
        collections::create_smart(&state.db, section, &title, &filter).await?
    } else {
        let id = collections::create(&state.db, section, &title, None).await?;
        collections::add_items(&state.db, id, section, &uri_items(uri)).await?;
        id
    };
    state.events.send(crate::events::ServerEvent::LibraryChanged { library_id: section });
    collection_details(&state, &admin, &info, id, &Params::default()).await
}

/// `PUT /library/collections/{key}/items?uri=…`: a smart collection's new
/// filter, or items for a collection that holds them.
pub async fn collection_add(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(key): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    let id = collection_path(key)?;
    let library_id = collections::library_of(&state.db, id).await?.ok_or(PlexError::NotFound)?;
    let uri = params.get("uri").ok_or_else(|| PlexError::BadRequest("uri is required".into()))?;
    if collections::is_smart(&state.db, id).await? {
        let lib = crate::library::get_library(&state.db, library_id).await?.ok_or(PlexError::NotFound)?;
        let filter = uri_filter(uri)
            .filter(|(s, f)| *s == library_id && crate::listing::valid_smart_filter(&lib, f))
            .ok_or_else(|| PlexError::BadRequest("a smart collection's filter is a listing of its section".into()))?
            .1;
        collections::set_smart_filter(&state.db, id, &filter).await?;
    } else {
        collections::add_items(&state.db, id, library_id, &uri_items(uri)).await?;
    }
    state.events.send(crate::events::ServerEvent::LibraryChanged { library_id });
    collection_details(&state, &admin, &info, id, &Params::default()).await
}

/// `DELETE /library/collections/{key}/items/{item}`
pub async fn collection_remove(
    State(state): State<AppState>,
    PlexAdmin(_admin): PlexAdmin,
    Path((key, item)): Path<(i64, i64)>,
) -> PlexResult<StatusCode> {
    let id = collection_path(key)?;
    let library_id = collections::library_of(&state.db, id).await?.ok_or(PlexError::NotFound)?;
    if collections::is_smart(&state.db, id).await? {
        return Err(PlexError::BadRequest("a smart collection is filled by its filter".into()));
    }
    if !collections::remove_item(&state.db, id, item).await? {
        return Err(PlexError::NotFound);
    }
    state.events.send(crate::events::ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::OK)
}

/// `PUT /library/collections/{key}/items/{item}/move?after={item}`
/// (python-plexapi's `moveItem()`): the item right after the other, or
/// first, and the collection keeps that order (`collectionSort` 2).
pub async fn collection_move(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path((key, item)): Path<(i64, i64)>,
    params: Params,
) -> PlexResult<Node> {
    let id = collection_path(key)?;
    let (collection, ..) = collection_for(&state, &admin, id).await?;
    if collection.smart_filter.is_some() {
        return Err(PlexError::BadRequest("a smart collection is in its filter's order".into()));
    }
    let after = match params.get("after").filter(|a| !a.trim().is_empty()) {
        Some(after) => Some(after.trim().parse().map_err(|_| PlexError::BadRequest("`after` is an item".into()))?),
        None => None,
    };
    if !collections::move_item(&state.db, id, item, after).await? {
        return Err(PlexError::NotFound);
    }
    state.events.send(crate::events::ServerEvent::LibraryChanged { library_id: collection.library_id });
    collection_items(&state, &admin, &info, id, &Params::default(), &Page::default()).await
}

/// `PUT /library/collections/{key}/refresh`: a collection has no metadata
/// of its own to fetch again (automatic ones follow their films'), so this
/// answers once the collection is found, and changes nothing.
pub async fn collection_refresh(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(key): Path<i64>,
) -> PlexResult<Node> {
    collection_for(&state, &admin, collection_path(key)?).await?;
    Ok(Node::container().attr("size", 0))
}

/// `/hubs/sections/{id}/manage`: the hubs a section's collections are
/// promoted to. Pley shows collections on no home page or recommendations,
/// so there are none.
pub async fn managed_hubs(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    Ok(Node::container().attr("size", 0).attr("allowSync", false).attr("librarySectionID", lib.id))
}

/// `POST /hubs/sections/{id}/manage?metadataItemId=…&promotedTo…=…`: a
/// collection promoted nowhere stays so; promoting it answers `400`, as Pley
/// has no hubs to promote it to.
pub async fn promote_hub(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    library_for(&state, &admin, &id).await?;
    if params.0.iter().any(|(k, v)| k.starts_with("promoted") && v.trim() != "0") {
        return Err(PlexError::BadRequest("Pley shows collections on no home page or recommendations".into()));
    }
    Ok(Node::container().attr("size", 0))
}

/// `PUT /hubs/sections/{id}/manage/{identifier}[/move]` and `DELETE …`:
/// there are no hubs to change.
pub async fn change_hub(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(path): Path<Vec<(String, String)>>,
) -> PlexResult<Node> {
    let id = path.iter().find(|(name, _)| name == "id").map(|(_, id)| id.as_str()).unwrap_or_default();
    library_for(&state, &admin, id).await?;
    Err(PlexError::BadRequest("Pley has no hubs of collections to change".into()))
}

/// `DELETE /library/collections/{key}`
pub async fn collection_delete(
    State(state): State<AppState>,
    PlexAdmin(_admin): PlexAdmin,
    Path(key): Path<i64>,
) -> PlexResult<StatusCode> {
    let id = collection_path(key)?;
    let library_id = collections::library_of(&state.db, id).await?.ok_or(PlexError::NotFound)?;
    collections::delete(&state.db, id).await?;
    state.events.send(crate::events::ServerEvent::LibraryChanged { library_id });
    Ok(StatusCode::OK)
}

/// Item ids in a Plex `uri` (`server://…/library/metadata/1,2,3`,
/// `library://…/item/%2Flibrary%2Fmetadata%2F1`), as many as a playlist or
/// a play queue holds at most: each id costs a lookup.
pub(super) fn uri_items(uri: &str) -> Vec<i64> {
    const MAX_IDS: usize = 5000;
    let Some(start) = uri.find("/library/metadata/") else { return Vec::new() };
    let rest = &uri[start + "/library/metadata/".len()..];
    let ids = rest.split(['/', '?', '&']).next().unwrap_or_default();
    ids.split(',').filter_map(|id| id.trim().parse().ok()).take(MAX_IDS).collect()
}

/// `POST /playlists?type=video&title=…&uri=…`
pub async fn create(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let title = params.get("title").unwrap_or_default().trim().chars().take(200).collect::<String>();
    if title.is_empty() {
        return Err(PlexError::BadRequest("title is required".into()));
    }
    if params.flag("smart") {
        // A listing of a section the user may open.
        let (section, filter) = params
            .get("uri")
            .and_then(uri_filter)
            .ok_or_else(|| PlexError::BadRequest("a smart playlist needs a section listing as uri".into()))?;
        let lib =
            visible_libraries(&state, &user).await?.into_iter().find(|l| l.id == section).ok_or(PlexError::NotFound)?;
        if !crate::listing::valid_smart_filter(&lib, &filter) {
            return Err(PlexError::BadRequest("that listing finds nothing to play".into()));
        }
        let kind = if lib.kind() == LibraryKind::Music { Kind::Audio } else { Kind::Video };
        let id = playlists::create_smart(&state.db, &user, &title, kind, section, &filter).await?;
        return playlist_container_of(&state, &user, id).await;
    }
    let items = params.get("uri").map(uri_items).unwrap_or_default();
    let kind = match params.get("type") {
        Some(kind) => Kind::parse(kind),
        None => playlists::kind_of_first(&state.db, &user, &items).await?,
    }
    .ok_or_else(|| PlexError::BadRequest("type must be video or audio".into()))?;
    let id = playlists::create(&state.db, &user, &title, kind).await?;
    if !items.is_empty()
        && let Err(err) = playlists::add(&state.db, &user, id, &items).await
    {
        playlists::delete(&state.db, &user, id).await?;
        return Err(err.into());
    }
    playlist_container_of(&state, &user, id).await
}

/// `/playlists/{key}`
pub async fn get(State(state): State<AppState>, PlexUser(user): PlexUser, Path(key): Path<i64>) -> PlexResult<Node> {
    playlist_container_of(&state, &user, playlist_path(key)?).await
}

/// `PUT /playlists/{key}?title.value=…` (python-plexapi's `editTitle()`), or
/// `?title=…`. A playlist keeps its title alone: anything else answers `400`.
pub async fn edit(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(key): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    let id = playlist_path(key)?;
    let mut title = None;
    for (name, value) in &params.0 {
        match name.as_str() {
            "title" | "title.value" => title = Some(value.trim().chars().take(200).collect::<String>()),
            // Kept as it is set.
            "title.locked" => {}
            _ if name.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("x-plex-")) => {}
            _ => {
                let field = name.split(['.', '[']).next().unwrap_or_default();
                return Err(PlexError::BadRequest(format!("`{field}` can't be edited: Pley doesn't keep it")));
            }
        }
    }
    if let Some(title) = title {
        if title.is_empty() {
            return Err(PlexError::BadRequest("title is required".into()));
        }
        if !playlists::rename(&state.db, &user, id, &title).await? {
            return Err(PlexError::NotFound);
        }
    }
    playlist_container_of(&state, &user, id).await
}

/// `DELETE /playlists/{key}`
pub async fn delete(State(state): State<AppState>, PlexUser(user): PlexUser, Path(key): Path<i64>) -> PlexResult<Node> {
    if !playlists::delete(&state.db, &user, playlist_path(key)?).await? {
        return Err(PlexError::NotFound);
    }
    Ok(Node::container().attr("size", 0))
}

/// The entries of playlist `id`, which carry `playlistItemID`.
pub(super) async fn playlist_items(
    state: &AppState,
    user: &AuthUser,
    id: i64,
    params: &Params,
    page: &Page,
) -> PlexResult<Node> {
    let (playlist, entries) = playlists::get_rows(&state.db, user, id).await?.ok_or(PlexError::NotFound)?;
    let (entries, total) = page.apply(entries);
    let (entry_ids, rows): (Vec<i64>, Vec<ItemRow>) = entries.into_iter().unzip();
    let libraries = visible_libraries(state, user).await?;
    let mut nodes = item_nodes(state, &rows, &Sections::new(&libraries), listing_detail(params)).await?;
    for (node, entry_id) in nodes.iter_mut().zip(entry_ids) {
        node.set("playlistItemID", entry_id);
    }
    let key = keys::playlist(playlist.id);
    let mut mc = base_container()
        .attr("size", nodes.len())
        .attr("totalSize", total)
        .attr("offset", page.start)
        .attr("ratingKey", key.to_string())
        .attr("key", format!("/playlists/{key}/items"))
        .attr("title", &playlist.title)
        .attr("playlistType", &playlist.kind)
        .attr("smart", false)
        .attr("composite", format!("/playlists/{key}/composite/{}", playlist.updated_at))
        .attr("leafCount", playlist.item_count)
        .opt("duration", (playlist.duration_ms > 0).then_some(playlist.duration_ms));
    mc.extend(nodes);
    Ok(mc)
}

/// `/playlists/{key}/items`
pub async fn items(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(key): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    playlist_items(&state, &user, playlist_path(key)?, &params, &page).await
}

/// `PUT /playlists/{key}/items?uri=…`
pub async fn add(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(key): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    let id = playlist_path(key)?;
    // A smart playlist gets a new listing (python-plexapi's updateFilters).
    if playlists::is_smart(&state.db, &user, id).await? {
        let (playlist, _) = playlists::get_rows(&state.db, &user, id).await?.ok_or(PlexError::NotFound)?;
        let lib = visible_libraries(&state, &user)
            .await?
            .into_iter()
            .find(|l| Some(l.id) == playlist.library_id)
            .ok_or(PlexError::NotFound)?;
        let filter = params
            .get("uri")
            .and_then(uri_filter)
            .filter(|(s, f)| *s == lib.id && crate::listing::valid_smart_filter(&lib, f))
            .ok_or_else(|| PlexError::BadRequest("a smart playlist's filter is a listing of its section".into()))?
            .1;
        playlists::set_smart_filter(&state.db, &user, id, &filter).await?;
        return playlist_container_of(&state, &user, id).await;
    }
    let items = params.get("uri").map(uri_items).unwrap_or_default();
    if items.is_empty() {
        return Err(PlexError::BadRequest("uri is required".into()));
    }
    let added = playlists::add(&state.db, &user, id, &items).await?;
    let mc = playlist_container_of(&state, &user, id).await?;
    Ok(mc.attr("leafCountAdded", added).attr("leafCountRequested", items.len()))
}

/// `DELETE /playlists/{key}/items/{playlistItemID}`
pub async fn remove(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((key, entry)): Path<(i64, i64)>,
) -> PlexResult<Node> {
    let id = playlist_path(key)?;
    if playlists::is_smart(&state.db, &user, id).await? {
        return Err(PlexError::BadRequest("a smart playlist is filled by its filter".into()));
    }
    if !playlists::remove_entry(&state.db, &user, id, entry).await? {
        return Err(PlexError::NotFound);
    }
    playlist_container_of(&state, &user, id).await
}

/// `PUT /playlists/{key}/items/{playlistItemID}/move?after={playlistItemID}`
pub async fn move_item(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((key, entry)): Path<(i64, i64)>,
    params: Params,
) -> PlexResult<Node> {
    let id = playlist_path(key)?;
    let after = params.parse::<i64>("after").filter(|after| *after > 0);
    if playlists::is_smart(&state.db, &user, id).await? {
        return Err(PlexError::BadRequest("a smart playlist plays in its filter's order".into()));
    }
    if !playlists::move_entry(&state.db, &user, id, entry, after).await? {
        return Err(PlexError::NotFound);
    }
    playlist_container_of(&state, &user, id).await
}

/// A playlist's picture: its first entry's.
pub(super) async fn playlist_artwork(
    state: AppState,
    user: AuthUser,
    id: i64,
    width: Option<u32>,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let (_, entries) = playlists::get_rows(&state.db, &user, id).await?.ok_or(PlexError::NotFound)?;
    let first = entries.first().ok_or(PlexError::NotFound)?.1.id;
    serve_image(state, user, first, "thumb", None, width, headers).await
}

/// `/playlists/{key}/composite/{version}`: the first entry's artwork.
pub async fn composite(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((key, _version)): Path<(i64, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    playlist_artwork(state, user, playlist_path(key)?, params.parse("width"), headers).await
}

/// `/photo/:/transcode?url=…` for collection and playlist artwork; `None`
/// for other URLs.
pub async fn photo_transcode(
    state: &AppState,
    user: &AuthUser,
    url: &str,
    params: &Params,
    headers: &HeaderMap,
) -> PlexResult<Option<Response>> {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let width = params.parse::<u32>("width").filter(|w| *w > 0);
    for (prefix, playlist) in [("/library/collections/", false), ("/playlists/", true)] {
        let Some(start) = path.find(prefix) else { continue };
        let mut segments = path[start + prefix.len()..].split('/');
        let key: i64 = segments.next().and_then(|s| s.parse().ok()).ok_or(PlexError::NotFound)?;
        let image = segments.next().unwrap_or("thumb").to_owned();
        let version = segments.next().map(str::to_owned);
        let (state, user, headers) = (state.clone(), user.clone(), headers.clone());
        let response = if playlist {
            playlist_artwork(state, user, playlist_path(key)?, width, headers).await?
        } else {
            collection_artwork(state, user, collection_path(key)?, &image, version, width, headers).await?
        };
        return Ok(Some(response));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plex_uris() {
        assert_eq!(uri_items("server://abc/com.plexapp.plugins.library/library/metadata/12,13"), vec![12, 13]);
        assert_eq!(uri_items("library://uuid/item//library/metadata/7"), vec![7]);
        assert_eq!(uri_items("library://uuid/directory//library/sections/1/all"), Vec::<i64>::new());
    }
}
