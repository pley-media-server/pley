//! Hubs: Plex's rows. `/hubs` (Home) and `/hubs/sections/{id}` (a section's
//! Recommended) show the rows administrators chose, as Pley's own apps get
//! them, except that collections of one title stay rows of their own on
//! Home, side by side, as Plex has them (Plex's apps put them together).
//! `/hubs/sections/{id}/manage` is Plex's hub management, which Kometa's
//! `visible_library`, `visible_home` and `visible_shared` use: Pley's own
//! collections go by Plex's identifiers for its built-in rows (and some of
//! Pley's own where Plex has none), others by Plex's
//! `custom.collection.<section>.<key>`. Pley has one Home for everyone,
//! which its administrators see too: Plex's owner's Home is it.

use axum::extract::{Path, State};

use super::keys;
use super::library::{
    continue_rows, hub, hub_type, item_nodes, library_for, listing_detail, recent_rows, visible_libraries,
};
use super::meta::Sections;
use super::{Node, Params, PlexAdmin, PlexError, PlexResult, PlexUser};
use crate::collections::{self, Builtin, CollectionRow, Names};
use crate::events::ServerEvent;
use crate::i18n;
use crate::library::{LibraryKind, LibraryRow};
use crate::rows::Row;
use crate::state::AppState;

/// The words of the Plex-compatible API: the server's language.
fn language(state: &AppState) -> &'static str {
    i18n::server_language(&state.settings.get().language)
}

/// A collection's hub identifier.
fn identifier(lib: &LibraryRow, row: &CollectionRow) -> String {
    match row.builtin.as_deref().and_then(Builtin::parse) {
        Some(builtin) => builtin.plex_identifier(lib.kind()),
        None => format!("custom.collection.{}.{}", lib.id, keys::collection(row.id)),
    }
}

/// The collection of `lib` an identifier names.
fn named<'a>(lib: &LibraryRow, rows: &'a [CollectionRow], identifier: &str) -> PlexResult<&'a CollectionRow> {
    rows.iter().find(|row| self::identifier(lib, row) == identifier).ok_or(PlexError::NotFound)
}

/// Where a row opens: Recently Added the section's own list, as before,
/// others their collection.
fn row_key(lib_id: i64, row: &Row) -> Option<String> {
    match row.collection_ids.as_slice() {
        [] => Some(format!("/library/sections/{lib_id}/recentlyAdded")),
        _ if row.key.starts_with("recently_added:") => Some(format!("/library/sections/{lib_id}/recentlyAdded")),
        [id] => Some(format!("/library/collections/{}/children", keys::collection(*id))),
        _ => None,
    }
}

/// A row's hub identifier: on Home, Recently Added keeps Plex's
/// `home.<area>.recent`.
fn row_identifier(lib: &LibraryRow, row: &Row, rows: &[CollectionRow], home: bool) -> String {
    if home && row.key.starts_with("recently_added:") {
        let area = match lib.kind() {
            LibraryKind::Movies => "movies",
            LibraryKind::Shows => "television",
            _ => "music",
        };
        return format!("home.{area}.recent");
    }
    match row.collection_ids.first().and_then(|id| rows.iter().find(|r| r.id == *id)) {
        Some(collection) => identifier(lib, collection),
        None => "music.recentlyadded".into(),
    }
}

/// `/hubs`: Continue Watching, then Home's rows, those of the sections
/// `contentDirectoryID` names when it is given.
pub async fn home(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    // Rows hold 30 titles, as in Pley's own apps.
    let count = params.parse::<usize>("count").unwrap_or(12).clamp(1, crate::rows::ROW_SIZE);
    let mut libraries = visible_libraries(&state, &user).await?;
    if let Some(ids) = params.get("contentDirectoryID") {
        let wanted: Vec<i64> = ids.split(',').filter_map(|i| i.trim().parse().ok()).collect();
        libraries.retain(|l| wanted.contains(&l.id));
    }
    let sections = Sections::new(&libraries);
    let detail = listing_detail(&params);
    let mut hubs = Vec::new();
    let rows = continue_rows(&state, &user, &libraries, None, count as i64).await?;
    if !rows.is_empty() {
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        let continuing = i18n::t(language(&state), "hubs.continueWatching");
        hubs.push(hub("home.continue".into(), continuing, t, Some("/hubs/continueWatching/items".into()), items));
    }
    for mut row in crate::rows::home_apart(&state.db, &user, language(&state)).await? {
        let Some(lib) = row.library_id.and_then(|id| libraries.iter().find(|l| l.id == id)) else { continue };
        let collections = collections::rows(&state.db, &row.collection_ids).await?;
        row.items.truncate(count);
        let t = hub_type(&row.items);
        let items = item_nodes(&state, &row.items, &sections, detail).await?;
        let identifier = row_identifier(lib, &row, &collections, true);
        hubs.push(hub(identifier, row.title.clone(), t, row_key(lib.id, &row), items));
    }
    if let Some(identifiers) = params.get("identifier") {
        let wanted: Vec<&str> = identifiers.split(',').collect();
        hubs.retain(
            |h| matches!(h.get("hubIdentifier"), Some(super::xml::Value::Str(id)) if wanted.contains(&id.as_str())),
        );
    }
    let mut mc = Node::container()
        .attr("size", hubs.len())
        .attr("allowSync", false)
        .attr("identifier", "com.plexapp.plugins.library");
    mc.extend(hubs);
    Ok(mc)
}

/// `/hubs/sections/{id}`: a section's Continue Watching, then its
/// Recommended rows; a music section's Recently Added.
pub async fn section(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    // Rows hold 30 titles, as in Pley's own apps.
    let count = params.parse::<usize>("count").unwrap_or(12).clamp(1, crate::rows::ROW_SIZE);
    let sections = Sections::new(&libraries);
    let detail = listing_detail(&params);
    let mut mc = Node::container()
        .attr("allowSync", false)
        .attr("librarySectionID", lib.id)
        .attr("librarySectionTitle", &lib.name);
    let mut size = 0;
    if lib.kind().is_audio() {
        let rows = recent_rows(&state, &user, &lib, count as i64).await?;
        if !rows.is_empty() {
            let t = hub_type(&rows);
            let items = item_nodes(&state, &rows, &sections, detail).await?;
            let title = i18n::t(language(&state), "hubs.recentlyAdded");
            let key = Some(format!("/library/sections/{}/recentlyAdded", lib.id));
            mc.push(hub("music.recentlyadded".into(), title, t, key, items));
            size += 1;
        }
        mc.set("size", size);
        return Ok(mc);
    }
    let prefix = if lib.kind() == LibraryKind::Shows { "tv" } else { "movie" };
    let rows = continue_rows(&state, &user, &libraries, Some(lib.id), count as i64).await?;
    if !rows.is_empty() {
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        let title = i18n::t(language(&state), "hubs.continueWatching");
        mc.push(hub(format!("{prefix}.inprogress"), title, t, None, items));
        size += 1;
    }
    let collections = collections::library_rows(&state.db, lib.id).await?;
    for mut row in crate::rows::recommended(&state.db, &user, &lib, language(&state)).await? {
        row.items.truncate(count);
        let t = hub_type(&row.items);
        let items = item_nodes(&state, &row.items, &sections, detail).await?;
        let identifier = row_identifier(&lib, &row, &collections, false);
        mc.push(hub(identifier, row.title.clone(), t, row_key(lib.id, &row), items));
        size += 1;
    }
    mc.set("size", size);
    Ok(mc)
}

/// A movie or TV section and its collections, for hub management.
async fn managed_library(
    state: &AppState,
    user: &crate::auth::AuthUser,
    id: &str,
) -> PlexResult<(LibraryRow, Vec<CollectionRow>)> {
    let (lib, _) = library_for(state, user, id).await?;
    let rows = if lib.kind().is_audio() { Vec::new() } else { collections::library_rows(&state.db, lib.id).await? };
    Ok((lib, rows))
}

fn one_or_zero(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

/// `/hubs/sections/{id}/manage`: the section's hubs, Recommended's in
/// order, then Pley's own that are on neither page; with `metadataItemId`,
/// that collection's, if it is shown anywhere (how Kometa reads it). For
/// those who manage the library: it names collections whoever asks may not
/// see.
pub async fn managed(
    State(state): State<AppState>,
    PlexAdmin(user): PlexAdmin,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, mut rows) = managed_library(&state, &user, &id).await?;
    rows.retain(|r| r.builtin.is_some() || r.home_position.is_some() || r.recommended_position.is_some());
    if let Some(key) = params.parse::<i64>("metadataItemId") {
        let wanted = keys::collection_id(key);
        rows.retain(|r| Some(r.id) == wanted && r.builtin.is_none());
    }
    rows.sort_by_key(|r| (r.recommended_position.is_none(), r.recommended_position, r.home_position.is_none()));
    let names = Names::load(&state.db, language(&state)).await?;
    let mut mc = Node::container().attr("size", rows.len()).attr("allowSync", false).attr("librarySectionID", lib.id);
    for row in &rows {
        let home = row.home_position.is_some();
        let recommended = row.recommended_position.is_some();
        let mut hub = Node::new("Hub")
            .attr("identifier", identifier(&lib, row))
            .attr("title", names.title(row))
            .attr("recommendationsVisibility", if recommended { "all" } else { "none" })
            .attr("homeVisibility", if home { "all" } else { "none" })
            .attr("promotedToRecommended", one_or_zero(recommended))
            .attr("promotedToOwnHome", one_or_zero(home))
            .attr("promotedToSharedHome", one_or_zero(home));
        // Plex's built-in hubs say they can't be removed: python-plexapi
        // takes a hub that doesn't say as one that can.
        hub = match row.builtin {
            Some(_) => hub.attr("deletable", "0"),
            None => hub.attr("deletable", "1").attr("metadataItemId", keys::collection(row.id)),
        };
        mc.push(hub);
    }
    Ok(mc)
}

/// A promotion parameter: `promoted…=1`, or a `…Visibility` other than
/// `none`.
fn promoted(params: &Params, flag: &str) -> Option<bool> {
    params.get(flag).map(|v| matches!(v.trim(), "1" | "true"))
}

fn visible(params: &Params, name: &str) -> Option<bool> {
    params.get(name).map(|v| v.trim() != "none")
}

/// Puts a collection on, or takes it off, Recommended and Home as the
/// parameters say; those not given leave it as it was. Pley's one Home is
/// the one its administrators see too, so Plex's owner's Home decides; the
/// friends' counts only when the owner's isn't given. python-plexapi and
/// Kometa send both, the one they don't change as they last read it (the
/// same, here): were either to keep a collection on Home, `demoteHome()` or
/// Kometa's `visible_home: false` would never take it off.
async fn promote_as(state: &AppState, row: &CollectionRow, params: &Params) -> PlexResult<()> {
    let recommended = promoted(params, "promotedToRecommended").or(visible(params, "recommendationsVisibility"));
    if let Some(on) = recommended {
        collections::set_on_recommended(&state.db, row.id, on).await?;
    }
    let home = visible(params, "homeVisibility")
        .or(promoted(params, "promotedToOwnHome"))
        .or(promoted(params, "promotedToSharedHome"));
    if let Some(on) = home {
        collections::set_on_home(&state.db, row.id, on).await?;
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(())
}

/// `POST /hubs/sections/{id}/manage?metadataItemId=…&promotedTo…=…`: shows
/// a collection of the section where the parameters say.
pub async fn promote(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (_, rows) = managed_library(&state, &admin, &id).await?;
    let key = params.parse::<i64>("metadataItemId").ok_or_else(|| PlexError::BadRequest("metadataItemId".into()))?;
    let wanted = keys::collection_id(key).ok_or(PlexError::NotFound)?;
    let row = rows.iter().find(|r| r.id == wanted).ok_or(PlexError::NotFound)?;
    promote_as(&state, row, &params).await?;
    Ok(Node::container().attr("size", 0))
}

/// `PUT /hubs/sections/{id}/manage/{identifier}?promotedTo…=…`
pub async fn change(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((id, identifier)): Path<(String, String)>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, rows) = managed_library(&state, &admin, &id).await?;
    promote_as(&state, named(&lib, &rows, &identifier)?, &params).await?;
    Ok(Node::container().attr("size", 0))
}

/// `DELETE /hubs/sections/{id}/manage/{identifier}`: a collection's hub
/// goes from both pages (the collection stays). Pley's own can't go, as
/// Plex's built-in hubs can't: they are hidden instead.
pub async fn unpromote(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((id, identifier)): Path<(String, String)>,
) -> PlexResult<Node> {
    let (lib, rows) = managed_library(&state, &admin, &id).await?;
    let row = named(&lib, &rows, &identifier)?;
    if row.builtin.is_some() {
        return Err(PlexError::BadRequest("a built-in hub is hidden, not deleted".into()));
    }
    collections::set_on_recommended(&state.db, row.id, false).await?;
    collections::set_on_home(&state.db, row.id, false).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: lib.id });
    Ok(Node::container().attr("size", 0))
}

/// `PUT /hubs/sections/{id}/manage/{identifier}/move?after=…`: places a
/// hub of Recommended after another (first without `after`; last after a
/// hub that isn't on it, as the listing has those last). Moving changes
/// only the order: a hub that isn't on Recommended stays off. Home's order
/// is set in Pley.
pub async fn move_hub(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((id, identifier)): Path<(String, String)>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, rows) = managed_library(&state, &admin, &id).await?;
    let moved = named(&lib, &rows, &identifier)?;
    let after = match params.get("after").filter(|a| !a.is_empty()) {
        Some(after) => Some(named(&lib, &rows, after)?.id),
        None => None,
    };
    if moved.recommended_position.is_none() {
        return Ok(Node::container().attr("size", 0));
    }
    let mut order: Vec<&CollectionRow> = rows.iter().filter(|r| r.recommended_position.is_some()).collect();
    order.sort_by_key(|r| r.recommended_position);
    let mut ids: Vec<i64> = order.iter().map(|r| r.id).filter(|id| *id != moved.id).collect();
    let at = match after {
        Some(after) => ids.iter().position(|id| *id == after).map_or(ids.len(), |i| i + 1),
        None => 0,
    };
    ids.insert(at, moved.id);
    collections::order_recommended(&state.db, lib.id, &ids).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: lib.id });
    Ok(Node::container().attr("size", 0))
}
