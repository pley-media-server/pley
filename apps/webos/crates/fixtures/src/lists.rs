//! My List and leaving Continue Watching (docs/API.md): GET, PUT and DELETE
//! /api/watchlist, and DELETE /api/continue-watching/{id}. My List changes
//! tell the profile's event streams (`watchlist_changed`), and so does leaving
//! Continue Watching (`user_state_changed`).

use serde_json::json;

use crate::ServerEvent;
use crate::household::Listed;
use crate::model::{ItemSummary, Kind};
use crate::server::{Answer, Cx, LIBRARY, Need, PLAY, Route, State, bad_request, json, locked, no_content, not_found};
use crate::views::{self, ListOpts};

pub(crate) fn routes() -> Vec<Route> {
    vec![
        locked("GET", "/api/watchlist", Need::Scope(LIBRARY), |cx, state| {
            let catalog = cx.catalog();
            let profile = state.profile_mut(cx.caller().profile);
            let listed = views::listed(catalog, profile);
            let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
            let items: Vec<ItemSummary> =
                listed.iter().filter_map(|id| views::summary_of(catalog, *id, profile, opts)).collect();
            Ok(json(200, &json!({ "items": items })))
        }),
        locked("PUT", "/api/watchlist/*", Need::Scope(PLAY), |cx, state| change(cx, state, true)),
        locked("DELETE", "/api/watchlist/*", Need::Scope(PLAY), |cx, state| change(cx, state, false)),
        locked("DELETE", "/api/continue-watching/*", Need::Scope(PLAY), hide),
    ]
}

/// Adds (again is fine) or removes a movie or show.
fn change(cx: &Cx<'_>, state: &mut State, add: bool) -> Answer {
    let id = cx.id(0)?;
    let catalog = cx.catalog();
    let item = catalog.item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    if !matches!(item.kind(), Kind::Movie | Kind::Show) {
        return Err(bad_request("My List takes movies and shows"));
    }
    let me = cx.caller().profile;
    let watched = views::state(catalog, item, state.profile(me)).watched;
    let now = cx.now();
    let profile = state.profile_mut(me);
    let listed = profile.watchlist.iter().any(|l| l.item_id == id);
    let changed = if add {
        if !listed {
            profile.watchlist.push(Listed { item_id: id, added_at: now, watched_then: watched });
        }
        !listed
    } else {
        profile.watchlist.retain(|l| l.item_id != id);
        listed
    };
    if changed {
        profile.version += 1;
        cx.shared.events.emit_to(me, &ServerEvent::WatchlistChanged);
    }
    Ok(no_content())
}

/// Takes a movie, or an episode's whole show, out of Continue Watching until
/// it is played again.
fn hide(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let item = cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    let owner = match item.kind() {
        Kind::Movie | Kind::Show => id,
        Kind::Episode => item.base.grandparent_id.unwrap_or(id),
        Kind::Season => item.base.parent_id.unwrap_or(id),
        _ => return Err(bad_request("only movies and episodes are in Continue Watching")),
    };
    let me = cx.caller().profile;
    let now = cx.now();
    let profile = state.profile_mut(me);
    profile.hidden_from_continue.insert(owner, now);
    profile.version += 1;
    cx.shared.events.emit_to(me, &ServerEvent::UserStateChanged { item_ids: vec![owner], children: false });
    Ok(no_content())
}
