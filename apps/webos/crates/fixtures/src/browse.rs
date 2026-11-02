//! What the app reads: the server, the profile, libraries and their
//! listings, hubs, items, people, collections, search, history and every
//! picture (docs/API.md: "Libraries", "Items", "Home & search", "Cast and
//! crew", "Collections", "Watch history").

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use serde_json::json;

use crate::catalog::Catalog;
use crate::household::Profile;
use crate::http::{Response, lock};
use crate::model::{
    Hub, ItemDetail, ItemSummary, Kind, Library, PersonCredit, PersonDetail, PersonRole, PersonSummary,
};
use crate::pictures::{Shape, picture};
use crate::server::{
    Answer, Cx, LIBRARY, Need, PLAY, Route, State, bad_request, error, free, json, locked, no_content, not_found,
};
use crate::views::{self, ListOpts};
use crate::{ServerEvent, ids};

pub(crate) fn routes() -> Vec<Route> {
    vec![
        locked("GET", "/api/health", Need::Public, |_, _| Ok(json(200, &json!({ "status": "ok" })))),
        locked("GET", "/api/server", Need::Public, |_, state| {
            Ok(json(
                200,
                &json!({
                    "name": ids::SERVER_NAME,
                    "version": ids::SERVER_VERSION,
                    "commit": "unknown",
                    "setup_required": false,
                    "setup_code_required": false,
                    "two_step_required": state.two_step_required,
                }),
            ))
        }),
        locked("GET", "/api/auth/me", Need::AnyApp, |cx, state| {
            Ok(json(200, &views::me(state.profile(cx.caller().profile), cx.caller().scope())))
        }),
        free("GET", "/api/users/*/avatar", Need::AnyApp, avatar),
        locked("GET", "/api/libraries", Need::Scope(LIBRARY), |cx, state| {
            let shared = matches!(cx.query("order"), Some("shared"));
            Ok(json(200, &libraries(cx.catalog(), state.profile(cx.caller().profile), shared)))
        }),
        locked("PUT", "/api/me/libraries/order", Need::Scope(PLAY), library_order),
        locked("DELETE", "/api/me/libraries/order", Need::Scope(PLAY), |cx, state| {
            let me = cx.caller().profile;
            state.profile_mut(me).library_order = None;
            cx.shared.events.emit_to(me, &ServerEvent::LibrariesChanged);
            Ok(json(200, &libraries(cx.catalog(), state.profile(me), false)))
        }),
        locked("GET", "/api/libraries/*", Need::Scope(LIBRARY), |cx, _| {
            let id = cx.id(0)?;
            let library = cx.catalog().library(id).ok_or_else(|| not_found(&format!("library {id} not found")))?;
            Ok(json(200, library))
        }),
        locked("GET", "/api/libraries/*/items", Need::Scope(LIBRARY), listing),
        locked("GET", "/api/libraries/*/filters", Need::Scope(LIBRARY), filters),
        locked("GET", "/api/libraries/*/hubs", Need::Scope(LIBRARY), |cx, state| {
            let id = cx.id(0)?;
            let library = cx.catalog().library(id).ok_or_else(|| not_found(&format!("library {id} not found")))?;
            let hubs = library_hubs(cx, library, state.profile(cx.caller().profile));
            Ok(json(200, &json!({ "hubs": hubs })))
        }),
        locked("GET", "/api/libraries/*/collections", Need::Scope(LIBRARY), |cx, _| {
            let id = cx.id(0)?;
            let catalog = cx.catalog();
            catalog.library(id).ok_or_else(|| not_found(&format!("library {id} not found")))?;
            let mut list: Vec<_> = catalog
                .collections()
                .iter()
                .filter(|c| c.library_id == id)
                .map(|c| views::collection_summary(catalog, c))
                .filter(|c| c.item_count > 0)
                .collect();
            list.sort_by_key(|c| c.title.to_lowercase());
            Ok(json(200, &json!({ "collections": list })))
        }),
        locked("GET", "/api/collections/*", Need::Scope(LIBRARY), |cx, state| {
            let id = cx.id(0)?;
            let collection = cx.catalog().collection(id).ok_or_else(|| not_found("collection not found"))?;
            let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
            let detail = views::collection_detail(cx.catalog(), collection, state.profile(cx.caller().profile), opts);
            Ok(json(200, &detail))
        }),
        free("GET", "/api/collections/*/images/*", Need::Scope(LIBRARY), |cx| {
            let id = cx.id(0)?;
            let collection = cx.catalog().collection(id).ok_or_else(|| not_found("no artwork"))?;
            let shape = match cx.value(1) {
                "poster" => Shape::Poster,
                "backdrop" => Shape::Wide,
                _ => return Err(not_found("no artwork")),
            };
            let bytes = cx.shared.pictures.artwork(collection.id * 1_000 + 7, shape, width(cx));
            Ok(picture(bytes, "image/jpeg", artwork_cache(cx)))
        }),
        locked("GET", "/api/items/*", Need::Scope(LIBRARY), item),
        locked("GET", "/api/items/*/children", Need::Scope(LIBRARY), |cx, state| {
            let id = cx.id(0)?;
            let catalog = cx.catalog();
            catalog.item(id).ok_or_else(|| item_not_found(id))?;
            let profile = state.profile(cx.caller().profile);
            let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
            let items: Vec<ItemSummary> =
                catalog.children(id).iter().filter_map(|c| views::summary_of(catalog, *c, profile, opts)).collect();
            Ok(json(200, &json!({ "items": items })))
        }),
        locked("GET", "/api/items/*/on-deck", Need::Scope(LIBRARY), |cx, state| {
            let id = cx.id(0)?;
            let catalog = cx.catalog();
            let item = catalog.item(id).ok_or_else(|| item_not_found(id))?;
            let profile = state.profile(cx.caller().profile);
            let deck =
                on_deck(catalog, item, profile).map(|e| views::summary(catalog, e, profile, ListOpts::default()));
            Ok(json(200, &json!({ "item": deck })))
        }),
        locked("GET", "/api/items/*/next", Need::Scope(LIBRARY), |cx, state| {
            let id = cx.id(0)?;
            let catalog = cx.catalog();
            let item = catalog.item(id).ok_or_else(|| item_not_found(id))?;
            let profile = state.profile(cx.caller().profile);
            let next = next(catalog, item).map(|n| views::summary(catalog, n, profile, ListOpts::default()));
            Ok(json(200, &json!({ "item": next })))
        }),
        locked("GET", "/api/items/*/related", Need::Scope(LIBRARY), |cx, state| {
            let id = cx.id(0)?;
            let catalog = cx.catalog();
            let item = catalog.item(id).ok_or_else(|| item_not_found(id))?;
            let limit = number(cx, "limit")?.unwrap_or(20).clamp(1, 60) as usize;
            let profile = state.profile(cx.caller().profile);
            let items: Vec<ItemSummary> = related(catalog, item, limit)
                .into_iter()
                .map(|r| views::summary(catalog, r, profile, ListOpts::default()))
                .collect();
            Ok(json(200, &json!({ "items": items })))
        }),
        free("GET", "/api/items/*/images/*", Need::Scope(LIBRARY), item_image),
        locked("GET", "/api/items/*/collections", Need::Scope(LIBRARY), |cx, _| {
            let id = cx.id(0)?;
            cx.catalog().item(id).ok_or_else(|| item_not_found(id))?;
            let refs: Vec<_> =
                cx.catalog().collections_of(id).iter().map(|c| json!({ "id": c.id, "title": c.title })).collect();
            Ok(json(200, &json!({ "collections": refs })))
        }),
        locked("POST", "/api/items/*/prefetch", Need::Scope(PLAY), |cx, _| {
            let id = cx.id(0)?;
            cx.catalog().item(id).ok_or_else(|| item_not_found(id))?;
            Ok(Response::new(202))
        }),
        locked("GET", "/api/people/*", Need::Scope(LIBRARY), person),
        free("GET", "/api/people/*/photo", Need::Scope(LIBRARY), |cx| {
            let id = cx.id(0)?;
            let person = cx.catalog().person(id).filter(|p| p.has_photo).ok_or_else(|| not_found("no photo"))?;
            let bytes = cx.shared.pictures.photo(person.id, width(cx));
            Ok(picture(bytes, "image/jpeg", artwork_cache(cx)))
        }),
        locked("GET", "/api/history", Need::Scope(LIBRARY), history),
        locked("GET", "/api/hubs/home", Need::Scope(LIBRARY), |cx, state| {
            let hubs = home_hubs(cx, state.profile_mut(cx.caller().profile));
            Ok(json(200, &json!({ "hubs": hubs })))
        }),
        locked("GET", "/api/search", Need::Scope(LIBRARY), search),
        free("GET", "/api/events", Need::Scope(LIBRARY), crate::events::stream),
        // Pley's web app's only: apps' tokens are refused.
        locked("GET", "/api/activity", Need::WebOnly, |_, _| Ok(no_content())),
    ]
}

fn item_not_found(id: i64) -> Response {
    not_found(&format!("item {id} not found"))
}

/// An integer query parameter; `400` when it isn't one.
pub(crate) fn number(cx: &Cx<'_>, name: &str) -> Result<Option<i64>, Response> {
    match cx.query(name) {
        None | Some("") => Ok(None),
        Some(v) => v.parse::<i64>().map(Some).map_err(|_| bad_request(&format!("{name} must be a number"))),
    }
}

/// The `w` asked for, in pixels.
fn width(cx: &Cx<'_>) -> Option<u32> {
    cx.query("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0)
}

/// Pictures with a version are kept for a year; without, five minutes.
fn artwork_cache(cx: &Cx<'_>) -> &'static str {
    if cx.query("v").is_some() { "private, max-age=31536000, immutable" } else { "private, max-age=300" }
}

/// The libraries in the profile's order (or the shared one).
fn libraries<'a>(catalog: &'a Catalog, profile: &Profile, shared: bool) -> Vec<&'a Library> {
    let mut list: Vec<&Library> = catalog.libraries.iter().collect();
    if let (Some(order), false) = (&profile.library_order, shared) {
        list.sort_by_key(|l| order.iter().position(|id| *id == l.id).unwrap_or(usize::MAX));
    }
    list
}

/// PUT /api/me/libraries/order `{ "ids": [..] }`: every library once.
fn library_order(cx: &Cx<'_>, state: &mut State) -> Answer {
    let body = cx.req.json().ok_or_else(|| bad_request("the body isn't JSON"))?;
    let ids: Vec<i64> = body["ids"]
        .as_array()
        .ok_or_else(|| bad_request("ids is needed"))?
        .iter()
        .map(|v| v.as_i64().ok_or_else(|| bad_request("ids are numbers")))
        .collect::<Result<_, _>>()?;
    let unique: HashSet<i64> = ids.iter().copied().collect();
    if unique.len() != ids.len() {
        return Err(bad_request("a library is in the list twice"));
    }
    let known: HashSet<i64> = cx.catalog().libraries.iter().map(|l| l.id).collect();
    if unique != known {
        return Err(error(409, "conflict", "the list doesn't match the libraries"));
    }
    let me = cx.caller().profile;
    state.profile_mut(me).library_order = Some(ids);
    cx.shared.events.emit_to(me, &ServerEvent::LibrariesChanged);
    Ok(json(200, &libraries(cx.catalog(), state.profile(me), false)))
}

fn avatar(cx: &Cx<'_>) -> Answer {
    let id = cx.id(0)?;
    let color = {
        let state = lock(&cx.shared.state);
        let profile =
            state.household.profile(id).filter(|p| p.has_avatar).ok_or_else(|| not_found("no such picture"))?;
        profile.avatar_color
    };
    let rgb = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x38bdf8);
    Ok(picture(cx.shared.pictures.avatar(rgb), "image/jpeg", "private, max-age=31536000, immutable"))
}

/// GET /api/items/{id}: the item with the profile's state and track choices.
fn item(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let catalog = cx.catalog();
    let item = catalog.item(id).ok_or_else(|| item_not_found(id))?;
    let me = cx.caller().profile;
    let listed = views::listed(catalog, state.profile_mut(me));
    let added = |file: i64| -> Vec<crate::model::MediaStream> {
        state.added_subtitles.get(&file).map(|list| list.iter().map(|a| a.stream()).collect()).unwrap_or_default()
    };
    let detail = views::detail(catalog, item, state.profile(me), &listed, &added);
    Ok(json(200, &detail))
}

/// The value a listing sorts by.
fn sort_key(catalog: &Catalog, item: &ItemDetail, profile: &Profile, sort: &str) -> Option<f64> {
    let b = &item.base;
    match sort {
        "year" => b.year.map(|y| y as f64),
        "added" => Some(b.added_at as f64),
        "rating" => b.rating,
        "user_rating" => views::state(catalog, item, profile).rating,
        "release" => b.release_date.as_ref().and_then(|d| d.replace('-', "").parse::<f64>().ok()),
        "duration" => b.duration_ms.map(|d| d as f64),
        "last_played" => views::state(catalog, item, profile).last_played_at.map(|t| t as f64),
        "bitrate" => views::media_summary(catalog, item).and_then(|m| m.bitrate).map(|b| b as f64),
        _ => None,
    }
}

const SORTS: [&str; 9] =
    ["title", "year", "added", "rating", "user_rating", "release", "duration", "last_played", "bitrate"];

/// GET /api/libraries/{id}/items: sorted, filtered and paged, with an ETag
/// that changes with the query and the profile's state, and `304` for a tag
/// that still holds.
fn listing(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let catalog = cx.catalog();
    catalog.library(id).ok_or_else(|| not_found(&format!("library {id} not found")))?;
    let profile = state.profile(cx.caller().profile);
    let mut hasher = DefaultHasher::new();
    (id, cx.req.raw_query.as_deref().unwrap_or(""), profile.id, profile.version, ids::SERVER_VERSION).hash(&mut hasher);
    let first = hasher.finish();
    first.hash(&mut hasher);
    let tag = format!("W/\"{:016x}{:08x}\"", first, hasher.finish() as u32);
    if cx.req.header("if-none-match") == Some(tag.as_str()) {
        return Ok(Response::new(304).with_header("ETag", tag).with_header("Cache-Control", "private, no-cache"));
    }
    let sort = cx.query("sort").unwrap_or("title");
    if !SORTS.contains(&sort) {
        return Err(bad_request(&format!("unknown sort '{sort}'")));
    }
    let descending = match cx.query("order") {
        None => !matches!(sort, "title" | "year" | "duration"),
        Some("asc") => false,
        Some("desc") => true,
        Some(other) => return Err(bad_request(&format!("unknown order '{other}'"))),
    };
    let offset = number(cx, "offset")?.unwrap_or(0).max(0) as usize;
    let limit = number(cx, "limit")?.map(|l| l.max(0) as usize);

    let mut items = catalog.top(id);
    if let Some(genre) = cx.query("genre") {
        items.retain(|i| i.genres.iter().any(|g| g == genre));
    }
    if let Some(year) = number(cx, "year")? {
        items.retain(|i| i.base.year == Some(year));
    }
    if let Some(watched) = cx.query("watched") {
        let want = matches!(watched, "1" | "true" | "yes");
        items.retain(|i| views::state(catalog, i, profile).watched == want);
    }
    let mut keyed: Vec<(Option<f64>, String, &ItemDetail)> =
        items.into_iter().map(|i| (sort_key(catalog, i, profile, sort), i.base.sort_title.to_lowercase(), i)).collect();
    keyed.sort_by(|a, b| {
        let by_title = a.1.cmp(&b.1).then(a.2.id().cmp(&b.2.id()));
        if sort == "title" {
            return if descending { by_title.reverse() } else { by_title };
        }
        // Items without the value come last, whichever the order.
        let primary = match (a.0, b.0) {
            (Some(x), Some(y)) => {
                let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                if descending { o.reverse() } else { o }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        primary.then(by_title)
    });
    let total = keyed.len();
    let opts = ListOpts { listing: true, summaries: cx.summaries(), media: sort == "bitrate" };
    let page: Vec<ItemSummary> = keyed
        .into_iter()
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .map(|(_, _, i)| views::summary(catalog, i, profile, opts))
        .collect();
    Ok(json(200, &json!({ "total": total, "items": page }))
        .with_header("ETag", tag)
        .with_header("Cache-Control", "private, no-cache"))
}

/// GET /api/libraries/{id}/filters: genres (case-insensitively sorted) and
/// years (newest first).
fn filters(cx: &Cx<'_>, _: &mut State) -> Answer {
    let id = cx.id(0)?;
    let catalog = cx.catalog();
    catalog.library(id).ok_or_else(|| not_found(&format!("library {id} not found")))?;
    let items = catalog.top(id);
    let mut genres: Vec<&str> = items.iter().flat_map(|i| i.genres.iter().map(String::as_str)).collect();
    genres.sort_by_key(|g| g.to_lowercase());
    genres.dedup();
    let mut years: Vec<i64> = items.iter().filter_map(|i| i.base.year).collect();
    years.sort_by(|a, b| b.cmp(a));
    years.dedup();
    Ok(json(200, &json!({ "genres": genres, "years": years })))
}

/// Continue Watching: in progress, and the episode after a show's last
/// finished one, newest played first, but what was taken out of it.
fn continue_watching<'a>(catalog: &'a Catalog, profile: &Profile, library: Option<i64>) -> Vec<&'a ItemDetail> {
    let mut rows: Vec<(i64, i64)> = Vec::new();
    let mut started_shows = HashSet::new();
    let mut last_per_show: HashMap<i64, (i64, i64)> = HashMap::new();
    for (&id, s) in &profile.states {
        let Some(item) = catalog.item(id) else { continue };
        let show = (item.kind() == Kind::Episode).then_some(item.base.grandparent_id).flatten();
        if s.position_ms > 0 && !s.watched {
            rows.push((s.last_played_at.unwrap_or(0), id));
            if let Some(show) = show {
                started_shows.insert(show);
            }
        }
        if let (Some(show), Some(at)) = (show, s.last_played_at) {
            let last = last_per_show.entry(show).or_insert((at, id));
            if at > last.0 {
                *last = (at, id);
            }
        }
    }
    for (show, (at, episode)) in last_per_show {
        if started_shows.contains(&show) || !profile.state(episode).watched {
            continue;
        }
        let leaves = catalog.leaves(show);
        let Some(next) = leaves.iter().position(|id| *id == episode).and_then(|i| leaves.get(i + 1)) else { continue };
        let state = profile.state(*next);
        if !state.watched && state.position_ms == 0 {
            rows.push((at, *next));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    rows.into_iter()
        .filter_map(|(_, id)| catalog.item(id))
        .filter(|i| library.is_none_or(|l| i.base.library_id == l))
        .filter(|i| !views::hidden_from_continue(catalog, i, profile))
        .take(20)
        .collect()
}

/// The newest of a library: movies, shows, or albums.
fn recent<'a>(catalog: &'a Catalog, library: &Library, limit: usize) -> Vec<&'a ItemDetail> {
    let mut items: Vec<&ItemDetail> = if library.kind == "music" {
        catalog
            .top_ids(library.id)
            .iter()
            .flat_map(|a| catalog.children(*a))
            .filter_map(|id| catalog.item(*id))
            .collect()
    } else {
        catalog.top(library.id)
    };
    items.sort_by(|a, b| b.base.added_at.cmp(&a.base.added_at).then(a.id().cmp(&b.id())));
    items.truncate(limit);
    items
}

fn hub(
    catalog: &Catalog,
    key: String,
    title: String,
    library: Option<i64>,
    items: Vec<&ItemDetail>,
    profile: &Profile,
    opts: ListOpts,
) -> Hub {
    Hub {
        key,
        title,
        library_id: library,
        items: items.into_iter().map(|i| views::summary(catalog, i, profile, opts)).collect(),
    }
}

/// GET /api/hubs/home: Continue Watching, My List, then each library's
/// newest, empty rows left out.
fn home_hubs(cx: &Cx<'_>, profile: &mut Profile) -> Vec<Hub> {
    let catalog = cx.catalog();
    let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
    let listed = views::listed(catalog, profile);
    let profile = &*profile;
    let mut hubs = Vec::new();
    let watching = continue_watching(catalog, profile, None);
    if !watching.is_empty() {
        hubs.push(hub(catalog, "continue_watching".into(), "Continue Watching".into(), None, watching, profile, opts));
    }
    let mine: Vec<&ItemDetail> = listed.iter().take(20).filter_map(|id| catalog.item(*id)).collect();
    if !mine.is_empty() {
        hubs.push(hub(catalog, "my_list".into(), "My List".into(), None, mine, profile, opts));
    }
    for library in libraries(catalog, profile, false) {
        let items = recent(catalog, library, 20);
        if !items.is_empty() {
            let title = format!("Recently Added in {}", library.name);
            hubs.push(hub(
                catalog,
                format!("recently_added:{}", library.id),
                title,
                Some(library.id),
                items,
                profile,
                opts,
            ));
        }
    }
    hubs
}

/// GET /api/libraries/{id}/hubs.
fn library_hubs(cx: &Cx<'_>, library: &Library, profile: &Profile) -> Vec<Hub> {
    let catalog = cx.catalog();
    let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
    let id = library.id;
    let video = library.kind != "music";
    let top = catalog.top(id);
    let mut hubs = Vec::new();
    if video {
        let watching = continue_watching(catalog, profile, Some(id));
        hubs.push(hub(
            catalog,
            format!("continue_watching:{id}"),
            "Continue Watching".into(),
            Some(id),
            watching,
            profile,
            opts,
        ));
    }
    let added_title = match library.kind {
        "shows" => "Recently Added Shows",
        "music" => "Recently Added Albums",
        _ => "Recently Added",
    };
    hubs.push(hub(
        catalog,
        format!("recently_added:{id}"),
        added_title.into(),
        Some(id),
        recent(catalog, library, 30),
        profile,
        opts,
    ));
    if video {
        let mut released = top.clone();
        released.sort_by(|a, b| b.base.release_date.cmp(&a.base.release_date).then(a.id().cmp(&b.id())));
        released.truncate(30);
        hubs.push(hub(
            catalog,
            format!("recently_released:{id}"),
            "Recently Released".into(),
            Some(id),
            released,
            profile,
            opts,
        ));
        let mut rated: Vec<&ItemDetail> = top.iter().copied().filter(|i| i.base.rating.is_some()).collect();
        rated.sort_by(|a, b| {
            b.base.rating.partial_cmp(&a.base.rating).unwrap_or(Ordering::Equal).then(a.id().cmp(&b.id()))
        });
        rated.truncate(30);
        hubs.push(hub(catalog, format!("top_rated:{id}"), "Top Rated".into(), Some(id), rated, profile, opts));
    }
    hubs.retain(|h| !h.items.is_empty());
    hubs
}

/// The episode a show's or season's Play starts: in progress, else the first
/// unwatched, else the first.
fn on_deck<'a>(catalog: &'a Catalog, item: &ItemDetail, profile: &Profile) -> Option<&'a ItemDetail> {
    if !matches!(item.kind(), Kind::Show | Kind::Season) {
        return None;
    }
    let episodes = catalog.leaves(item.id());
    let chosen = episodes
        .iter()
        .find(|id| {
            let s = profile.state(**id);
            s.position_ms > 0 && !s.watched
        })
        .or_else(|| episodes.iter().find(|id| !profile.state(**id).watched))
        .or(episodes.first())?;
    catalog.item(*chosen)
}

/// The episode after an episode (across seasons), the track after a track
/// within its album.
fn next<'a>(catalog: &'a Catalog, item: &ItemDetail) -> Option<&'a ItemDetail> {
    let top = match item.kind() {
        Kind::Episode => item.base.grandparent_id?,
        Kind::Track => item.base.parent_id?,
        _ => return None,
    };
    let leaves = catalog.leaves(top);
    let at = leaves.iter().position(|id| *id == item.id())?;
    leaves.get(at + 1).and_then(|id| catalog.item(*id))
}

/// Titles of the same library sharing genres, most shared first, then by
/// rating.
fn related<'a>(catalog: &'a Catalog, item: &ItemDetail, limit: usize) -> Vec<&'a ItemDetail> {
    if !matches!(item.kind(), Kind::Movie | Kind::Show) {
        return Vec::new();
    }
    let mut scored: Vec<(usize, &ItemDetail)> = catalog
        .top(item.base.library_id)
        .into_iter()
        .filter(|other| other.id() != item.id())
        .map(|other| (other.genres.iter().filter(|g| item.genres.contains(g)).count(), other))
        .filter(|(shared, _)| *shared > 0)
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(
                b.1.base.rating.unwrap_or(0.0).partial_cmp(&a.1.base.rating.unwrap_or(0.0)).unwrap_or(Ordering::Equal),
            )
            .then(a.1.id().cmp(&b.1.id()))
    });
    scored.into_iter().take(limit).map(|(_, i)| i).collect()
}

/// GET /api/items/{id}/images/{poster|backdrop|thumb}: missing art falls back
/// to the parent's, as the server's.
fn item_image(cx: &Cx<'_>) -> Answer {
    let id = cx.id(0)?;
    let item = cx.catalog().item(id).ok_or_else(|| item_not_found(id))?;
    let kind = item.kind();
    let b = &item.base;
    let (owner, shape) = match cx.value(1) {
        "poster" => {
            let shape =
                if matches!(kind, Kind::Artist | Kind::Album | Kind::Track) { Shape::Square } else { Shape::Poster };
            let owner = if matches!(kind, Kind::Episode | Kind::Track) { b.parent_id } else { Some(id) };
            (owner, shape)
        }
        "backdrop" => {
            let owner = match kind {
                Kind::Season | Kind::Album | Kind::Extra => b.parent_id,
                Kind::Episode | Kind::Track => b.grandparent_id,
                _ => Some(id),
            };
            (owner, Shape::Wide)
        }
        "thumb" if matches!(kind, Kind::Movie | Kind::Episode | Kind::Extra) => (Some(id), Shape::Wide),
        _ => return Err(not_found("no artwork")),
    };
    let owner = owner.unwrap_or(id);
    let seed = owner * 3 + if cx.value(1) == "poster" { 0 } else { 1 };
    let bytes = cx.shared.pictures.artwork(seed, shape, width(cx));
    Ok(picture(bytes, "image/jpeg", artwork_cache(cx)))
}

/// GET /api/people/{id}: what they are credited in, newest first; episode
/// credits count towards their show.
fn person(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let catalog = cx.catalog();
    let person = catalog.person(id).ok_or_else(|| not_found("person not found"))?;
    let profile = state.profile(cx.caller().profile);
    let mut order: Vec<i64> = Vec::new();
    let mut by_title: HashMap<i64, Vec<&ItemDetail>> = HashMap::new();
    let mut seen = HashSet::new();
    for item_id in catalog.credits.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
        if !seen.insert(*item_id) {
            continue;
        }
        let Some(item) = catalog.item(*item_id) else { continue };
        let title =
            if item.kind() == Kind::Episode { item.base.grandparent_id.unwrap_or(item.id()) } else { item.id() };
        if !by_title.contains_key(&title) {
            order.push(title);
        }
        by_title.entry(title).or_default().push(item);
    }
    let mut credits: Vec<PersonCredit> = Vec::new();
    for title_id in order {
        let Some(title) = catalog.item(title_id) else { continue };
        let mut roles: Vec<PersonRole> = Vec::new();
        for item in &by_title[&title_id] {
            let episode = item.kind() == Kind::Episode;
            for credit in item.cast.iter().chain(&item.crew).filter(|c| c.id == id) {
                match roles.iter_mut().find(|r| r.role == credit.role && r.character == credit.character) {
                    Some(role) if episode => role.episodes = Some(role.episodes.unwrap_or(0) + 1),
                    Some(_) => {}
                    None => roles.push(PersonRole {
                        role: credit.role,
                        character: credit.character.clone(),
                        episodes: episode.then_some(1),
                    }),
                }
            }
        }
        credits.push(PersonCredit { item: views::summary(catalog, title, profile, ListOpts::default()), roles });
    }
    if credits.is_empty() {
        return Err(not_found("person not found"));
    }
    credits.sort_by_key(|c| std::cmp::Reverse(c.item.year.unwrap_or(0)));
    Ok(json(200, &PersonDetail { id, name: person.name.clone(), photo_url: person.photo_url(), credits }))
}

/// GET /api/search: titles (movies, shows and artists first), people, and
/// Live TV channels.
fn search(cx: &Cx<'_>, state: &mut State) -> Answer {
    let q = cx.query("q").unwrap_or("").trim().to_lowercase();
    let limit = number(cx, "limit")?.unwrap_or(40).clamp(1, 200) as usize;
    if q.is_empty() {
        return Ok(json(200, &json!({ "items": [], "people": [], "channels": [] })));
    }
    let words: Vec<&str> = q.split_whitespace().take(8).collect();
    let matches = |name: &str| {
        let name = name.to_lowercase();
        let parts: Vec<&str> = name.split([' ', '-', ':']).filter(|p| !p.is_empty()).collect();
        words.iter().all(|w| parts.iter().any(|p| p.starts_with(w)))
    };
    let catalog = cx.catalog();
    let profile = state.profile(cx.caller().profile);
    let group = |kind: Kind| match kind {
        Kind::Movie | Kind::Show | Kind::Artist => 0,
        Kind::Album | Kind::Season => 1,
        _ => 2,
    };
    let mut found: Vec<(u8, u8, String, &ItemDetail)> = catalog
        .items()
        .filter(|i| !matches!(i.kind(), Kind::Extra | Kind::Season) && matches(&i.base.title))
        .map(|i| {
            let title = i.base.title.to_lowercase();
            let rank = if title == q {
                0
            } else if title.starts_with(&q) {
                1
            } else {
                2
            };
            (group(i.kind()), rank, i.base.sort_title.to_lowercase(), i)
        })
        .collect();
    found.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
    let items: Vec<ItemSummary> =
        found.into_iter().take(limit).map(|(_, _, _, i)| views::summary(catalog, i, profile, opts)).collect();
    let mut people: Vec<&crate::catalog::Person> = catalog.people().iter().filter(|p| matches(&p.name)).collect();
    let credited = |id: i64| catalog.credits.get(&id).map_or(0, Vec::len);
    people.sort_by(|a, b| credited(b.id).cmp(&credited(a.id)).then(a.id.cmp(&b.id)));
    let people: Vec<PersonSummary> = people
        .into_iter()
        .take(12)
        .map(|p| PersonSummary { id: p.id, name: p.name.clone(), photo_url: p.photo_url() })
        .collect();
    let channels = if profile.live { state.live.search(&q, profile, cx.now(), 12) } else { Vec::new() };
    Ok(json(200, &json!({ "items": items, "people": people, "channels": channels })))
}

/// GET /api/history: the profile's plays, newest first.
fn history(cx: &Cx<'_>, state: &mut State) -> Answer {
    let catalog = cx.catalog();
    let offset = number(cx, "offset")?.unwrap_or(0).max(0) as usize;
    let limit = number(cx, "limit")?.unwrap_or(50).clamp(1, 200) as usize;
    let under: Option<HashSet<i64>> = number(cx, "item_id")?.map(|id| {
        let mut set: HashSet<i64> = catalog.leaves(id).into_iter().collect();
        set.insert(id);
        set
    });
    let library = number(cx, "library_id")?;
    let since = number(cx, "since")?;
    let profile = state.profile(cx.caller().profile);
    let mut records: Vec<_> = profile
        .history
        .iter()
        .filter(|r| under.as_ref().is_none_or(|u| u.contains(&r.item_id)))
        .filter(|r| library.is_none_or(|l| catalog.item(r.item_id).is_some_and(|i| i.base.library_id == l)))
        .filter(|r| since.is_none_or(|s| r.viewed_at >= s))
        .collect();
    records.sort_by(|a, b| b.viewed_at.cmp(&a.viewed_at).then(b.id.cmp(&a.id)));
    let opts = ListOpts { summaries: cx.summaries(), ..ListOpts::default() };
    let entries: Vec<_> = records
        .iter()
        .skip(offset)
        .take(limit)
        .filter_map(|r| views::history_entry(catalog, r, profile, opts))
        .collect();
    Ok(json(200, &json!({ "total": records.len(), "entries": entries })))
}
