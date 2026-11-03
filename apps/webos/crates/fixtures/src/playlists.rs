//! The profile's playlists (docs/API.md, "Playlists"): listed and read with
//! `library`, made and changed with `playlists`. Another profile's answer
//! `404`; a smart one's entries can't be added, removed or reordered.

use serde_json::{Value, json};

use crate::ids;
use crate::model::{ItemDetail, PlaylistSummary};
use crate::server::{
    Answer, Cx, LIBRARY, Need, PLAYLISTS, Route, State, bad_request, json, locked, no_content, not_found,
};
use crate::views::{self, ListOpts};

/// The most entries a playlist holds.
const MAX_ENTRIES: usize = 5_000;

pub(crate) fn routes() -> Vec<Route> {
    vec![
        locked("GET", "/api/playlists", Need::Scope(LIBRARY), |cx, state| {
            let me = cx.caller().profile;
            let profile = state.profile(me);
            let mut list: Vec<PlaylistSummary> = state
                .household
                .playlists
                .values()
                .filter(|p| p.owner == me)
                .map(|p| views::playlist_detail(cx.catalog(), p, profile, ListOpts::default()).base)
                .collect();
            list.sort_by_key(|p| p.title.to_lowercase());
            Ok(json(200, &json!({ "playlists": list })))
        }),
        locked("GET", "/api/playlists/*", Need::Scope(LIBRARY), |cx, state| {
            let id = own(cx, state)?;
            Ok(detail(cx, state, id, 200))
        }),
        locked("POST", "/api/playlists", Need::Scope(PLAYLISTS), create),
        locked("PATCH", "/api/playlists/*", Need::Scope(PLAYLISTS), patch),
        locked("DELETE", "/api/playlists/*", Need::Scope(PLAYLISTS), |cx, state| {
            let id = own(cx, state)?;
            state.household.playlists.remove(&id);
            Ok(no_content())
        }),
        locked("POST", "/api/playlists/*/items", Need::Scope(PLAYLISTS), add),
        locked("DELETE", "/api/playlists/*/items/*", Need::Scope(PLAYLISTS), |cx, state| {
            let id = own(cx, state)?;
            let entry = cx.id(1)?;
            let now = cx.now();
            let playlist = state.household.playlists.get_mut(&id).expect("checked");
            if playlist.smart_filter.is_some() {
                return Err(bad_request("a smart playlist's entries come from its filter"));
            }
            let before = playlist.entries.len();
            playlist.entries.retain(|(e, _)| *e != entry);
            if playlist.entries.len() == before {
                return Err(not_found("no such entry"));
            }
            playlist.updated_at = now;
            Ok(no_content())
        }),
        locked("PUT", "/api/playlists/*/order", Need::Scope(PLAYLISTS), order),
    ]
}

/// The caller's own playlist of the path, or `404`.
fn own(cx: &Cx<'_>, state: &State) -> Result<i64, crate::http::Response> {
    let id = cx.id(0)?;
    match state.household.playlists.get(&id) {
        Some(p) if p.owner == cx.caller().profile => Ok(id),
        _ => Err(not_found("playlist not found")),
    }
}

fn detail(cx: &Cx<'_>, state: &State, id: i64, status: u16) -> crate::http::Response {
    let playlist = &state.household.playlists[&id];
    let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
    json(status, &views::playlist_detail(cx.catalog(), playlist, state.profile(cx.caller().profile), opts))
}

fn ids_of(body: &Value, name: &str) -> Result<Vec<i64>, crate::http::Response> {
    match &body[name] {
        Value::Null => Ok(Vec::new()),
        Value::Array(list) => {
            list.iter().map(|v| v.as_i64().ok_or_else(|| bad_request(&format!("{name} are numbers")))).collect()
        }
        _ => Err(bad_request(&format!("{name} is a list"))),
    }
}

/// Items as playlist entries: shows, seasons, albums and artists as their
/// episodes or tracks, in playing order.
fn expand<'a>(cx: &'a Cx<'_>, ids: &[i64]) -> Result<Vec<&'a ItemDetail>, crate::http::Response> {
    let catalog = cx.catalog();
    let mut out = Vec::new();
    for id in ids {
        catalog.item(*id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
        out.extend(catalog.leaves(*id).iter().filter_map(|leaf| catalog.item(*leaf)));
    }
    Ok(out)
}

/// POST /api/playlists: of items (the first decides the kind when none is
/// given), or smart.
fn create(cx: &Cx<'_>, state: &mut State) -> Answer {
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a playlist"))?;
    let title = body["title"].as_str().unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err(bad_request("a playlist needs a title"));
    }
    let me = cx.caller().profile;
    let now = cx.now();
    if let Some(filter) = body["smart_filter"].as_str() {
        let library = body["library_id"]
            .as_i64()
            .filter(|id| cx.catalog().library(*id).is_some())
            .ok_or_else(|| bad_request("a smart playlist needs its library"))?;
        let kind = if library == ids::MUSIC { "audio" } else { "video" };
        let id = state.household.new_playlist(me, &title, kind, Some(filter.to_string()), Some(library), now);
        return Ok(detail(cx, state, id, 201));
    }
    let leaves = expand(cx, &ids_of(&body, "item_ids")?)?;
    let kind = match body["kind"].as_str() {
        Some("video") => "video",
        Some("audio") => "audio",
        Some(_) => return Err(bad_request("kind is video or audio")),
        None => match leaves.first() {
            Some(first) if views::fits("audio", first) => "audio",
            _ => "video",
        },
    };
    if leaves.iter().any(|i| !views::fits(kind, i)) {
        return Err(bad_request(&format!("that doesn't fit a {kind} playlist")));
    }
    let id = state.household.new_playlist(me, &title, kind, None, None, now);
    for item in leaves.iter().take(MAX_ENTRIES) {
        let entry = state.household.new_entry();
        state.household.playlists.get_mut(&id).expect("just made").entries.push((entry, item.id()));
    }
    Ok(detail(cx, state, id, 201))
}

/// PATCH /api/playlists/{id} `{ title?, smart_filter? }`.
fn patch(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = own(cx, state)?;
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a playlist change"))?;
    let now = cx.now();
    let playlist = state.household.playlists.get_mut(&id).expect("checked");
    if let Some(filter) = body["smart_filter"].as_str() {
        if playlist.smart_filter.is_none() {
            return Err(bad_request("only smart playlists have a filter"));
        }
        playlist.smart_filter = Some(filter.to_string());
    }
    if let Some(title) = body["title"].as_str() {
        if title.trim().is_empty() {
            return Err(bad_request("a playlist needs a title"));
        }
        playlist.title = title.trim().to_string();
    }
    playlist.updated_at = now;
    Ok(detail(cx, state, id, 200))
}

/// POST /api/playlists/{id}/items `{ item_ids }`: `{ added }`.
fn add(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = own(cx, state)?;
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't item_ids"))?;
    let leaves = expand(cx, &ids_of(&body, "item_ids")?)?;
    let now = cx.now();
    let (kind, smart, held) = {
        let p = &state.household.playlists[&id];
        (p.kind, p.smart_filter.is_some(), p.entries.len())
    };
    if smart {
        return Err(bad_request("a smart playlist's entries come from its filter"));
    }
    if leaves.iter().any(|i| !views::fits(kind, i)) {
        return Err(bad_request(&format!("that doesn't fit a {kind} playlist")));
    }
    let room = MAX_ENTRIES.saturating_sub(held);
    let added: Vec<i64> = leaves.iter().take(room).map(|i| i.id()).collect();
    for item in &added {
        let entry = state.household.new_entry();
        state.household.playlists.get_mut(&id).expect("checked").entries.push((entry, *item));
    }
    state.household.playlists.get_mut(&id).expect("checked").updated_at = now;
    Ok(json(200, &json!({ "added": added.len() })))
}

/// PUT /api/playlists/{id}/order `{ entry_ids }`: these first, the rest after.
fn order(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = own(cx, state)?;
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't entry_ids"))?;
    let wanted = ids_of(&body, "entry_ids")?;
    let now = cx.now();
    let playlist = state.household.playlists.get_mut(&id).expect("checked");
    if playlist.smart_filter.is_some() {
        return Err(bad_request("a smart playlist's order is its filter's"));
    }
    let mut first: Vec<(i64, i64)> = Vec::new();
    for entry in wanted {
        if let Some(found) = playlist.entries.iter().find(|(e, _)| *e == entry)
            && !first.contains(found)
        {
            first.push(*found);
        }
    }
    let rest: Vec<(i64, i64)> = playlist.entries.iter().filter(|e| !first.contains(e)).copied().collect();
    playlist.entries = first.into_iter().chain(rest).collect();
    playlist.updated_at = now;
    Ok(detail(cx, state, id, 200))
}
