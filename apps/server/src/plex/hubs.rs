//! Hub management (`/hubs/sections/{id}/manage…`): a section's rows, which
//! of them show on its Recommended page and on Home, and in what order, as
//! Plex's web app and tools like Agregarr arrange them. They are Pley's
//! rows of the library ([`crate::hub_layout`]), so an arrangement made here
//! shows in every Pley app, and one made in Pley shows here.
//!
//! Plex's "own Home" is the server owner's: Pley's administrators. Its
//! "shared Home" is everyone else's.

use axum::extract::{Path, State};
use axum::http::StatusCode;

use super::keys;
use super::library::{builtin_hub_title, hub_identifier, library_for};
use super::{Node, Params, PlexAdmin, PlexError, PlexResult, PlexUser};
use crate::events::ServerEvent;
use crate::hub_layout::{self, HubKind, HubPlace};
use crate::library::LibraryRow;
use crate::state::AppState;

/// `GET /hubs/sections/{id}/manage`: the section's rows in order, those that
/// show nowhere included; with `metadataItemId`, the collection's row alone
/// (none when it isn't one), as python-plexapi reads a collection's
/// visibility.
pub async fn managed(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let mut places = hub_layout::layout(&state.db, &lib).await?;
    if let Some(key) = params.get("metadataItemId") {
        let collection = key.trim().parse().ok().and_then(keys::collection_id);
        places.retain(|p| collection.is_some_and(|id| p.kind() == Some(HubKind::Collection(id))));
    }
    managed_container(&state, &lib, &places).await
}

/// `POST /hubs/sections/{id}/manage?metadataItemId={key}`: makes the
/// collection a row of its section, last and shown nowhere until a
/// `promoted…` parameter says where (here or through `PUT`). A collection
/// already a row stays where it is.
pub async fn promote(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    let key: i64 = params
        .get("metadataItemId")
        .and_then(|k| k.trim().parse().ok())
        .ok_or_else(|| PlexError::BadRequest("`metadataItemId` names a collection".into()))?;
    let collection = keys::collection_id(key).ok_or(PlexError::NotFound)?;
    if crate::collections::library_of(&state.db, collection).await? != Some(lib.id) {
        return Err(PlexError::NotFound);
    }
    let mut places = hub_layout::layout(&state.db, &lib).await?;
    let kind = HubKind::Collection(collection);
    let index = match places.iter().position(|p| p.kind() == Some(kind)) {
        Some(index) => index,
        None => {
            places.push(HubPlace { hub: kind.key(), on_library: false, on_admin_home: false, on_home: false });
            places.len() - 1
        }
    };
    promoted(&mut places[index], &params)?;
    save(&state, &lib, &places).await
}

/// `PUT /hubs/sections/{id}/manage/{identifier}?promotedToRecommended=&promotedToOwnHome=&promotedToSharedHome=`:
/// where the row shows; what isn't given stays as it is.
pub async fn change(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((id, identifier)): Path<(String, String)>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    let mut places = hub_layout::layout(&state.db, &lib).await?;
    let index = find(&lib, &places, &identifier)?;
    promoted(&mut places[index], &params)?;
    save(&state, &lib, &places).await
}

/// `PUT /hubs/sections/{id}/manage/{identifier}/move?after={identifier}`:
/// moves the row right after another, or first without `after`.
pub async fn move_hub(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((id, identifier)): Path<(String, String)>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    let mut places = hub_layout::layout(&state.db, &lib).await?;
    let from = find(&lib, &places, &identifier)?;
    let place = places.remove(from);
    let to = match params.get("after").map(str::trim).filter(|a| !a.is_empty()) {
        Some(after) if after == identifier => from,
        Some(after) => find(&lib, &places, after)? + 1,
        None => 0,
    };
    places.insert(to, place);
    save(&state, &lib, &places).await
}

/// `DELETE /hubs/sections/{id}/manage/{identifier}`: the collection is no
/// longer a row. Built-in rows stay: they are hidden instead.
pub async fn remove(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path((id, identifier)): Path<(String, String)>,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    let mut places = hub_layout::layout(&state.db, &lib).await?;
    let index = find(&lib, &places, &identifier)?;
    if !matches!(places[index].kind(), Some(HubKind::Collection(_))) {
        return Err(PlexError::BadRequest("a built-in row stays: hide it instead".into()));
    }
    places.remove(index);
    save(&state, &lib, &places).await
}

/// `DELETE /hubs/sections/{id}/manage`: the section's rows as usual again,
/// no collection promoted.
pub async fn reset(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<String>,
) -> PlexResult<StatusCode> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    hub_layout::reset(&state.db, lib.id).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: lib.id });
    Ok(StatusCode::OK)
}

/// Where in `places` the row called `identifier` is.
fn find(lib: &LibraryRow, places: &[HubPlace], identifier: &str) -> PlexResult<usize> {
    places
        .iter()
        .position(|p| p.kind().is_some_and(|kind| hub_identifier(lib, kind) == identifier))
        .ok_or(PlexError::NotFound)
}

/// Sets where `place` shows from the `promoted…` parameters given.
fn promoted(place: &mut HubPlace, params: &Params) -> PlexResult<()> {
    for (name, field) in [
        ("promotedToRecommended", &mut place.on_library),
        ("promotedToOwnHome", &mut place.on_admin_home),
        ("promotedToSharedHome", &mut place.on_home),
    ] {
        if let Some(value) = params.get(name) {
            *field = match value.trim() {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => return Err(PlexError::BadRequest(format!("`{name}` is 1 or 0"))),
            };
        }
    }
    Ok(())
}

async fn save(state: &AppState, lib: &LibraryRow, places: &[HubPlace]) -> PlexResult<Node> {
    let places = hub_layout::set_layout(&state.db, lib, places).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: lib.id });
    managed_container(state, lib, &places).await
}

async fn managed_container(state: &AppState, lib: &LibraryRow, places: &[HubPlace]) -> PlexResult<Node> {
    let titles: std::collections::HashMap<i64, String> =
        sqlx::query_as::<_, (i64, String)>("SELECT id, title FROM collections WHERE library_id = ?")
            .bind(lib.id)
            .fetch_all(&state.db.read)
            .await?
            .into_iter()
            .collect();
    let mut mc = Node::container().attr("size", places.len()).attr("allowSync", false);
    for place in places {
        let Some(kind) = place.kind() else { continue };
        let (title, deletable) = match kind {
            HubKind::Builtin(builtin) => (builtin_hub_title(builtin).to_owned(), false),
            HubKind::Collection(id) => (titles.get(&id).cloned().unwrap_or_default(), true),
        };
        let home = match (place.on_admin_home, place.on_home) {
            (true, true) => "all",
            (true, false) => "admin",
            (false, true) => "shared",
            (false, false) => "none",
        };
        let mut hub = Node::new("Hub")
            .attr("identifier", hub_identifier(lib, kind))
            .attr("title", title)
            .attr("recommendationsVisibility", if place.on_library { "all" } else { "none" })
            .attr("homeVisibility", home)
            .attr("promotedToRecommended", place.on_library)
            .attr("promotedToOwnHome", place.on_admin_home)
            .attr("promotedToSharedHome", place.on_home);
        if deletable {
            hub = hub.attr("deletable", true);
        }
        mc.push(hub);
    }
    Ok(mc)
}
