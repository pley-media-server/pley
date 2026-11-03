//! Play queues (`/playQueues`): what a Plex client plays, and next. A queue
//! is made from items (`uri=…/library/metadata/1,2`), a container (a show
//! or season plays its episodes, an album its tracks), a section listing
//! (`…/library/sections/1/all?…`) or a playlist (`playlistID=`), optionally
//! shuffled. With `continuous=1`, an episode is followed by the rest of its
//! show. Queues live in memory, as clients make a new one for each playback:
//! at most [`MAX_QUEUES`], each for a day after its last use, and only for the
//! user who made it.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use parking_lot::Mutex;

use super::library::{base_container, item_nodes, listing_detail, visible_libraries};
use super::meta::Sections;
use super::{Node, Params, PlexError, PlexResult, PlexUser};
use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow};
use crate::playlists;
use crate::state::AppState;

/// Queues kept at once (the least recently used go first).
const MAX_QUEUES: usize = 1000;
/// Queues one user keeps: theirs go first, so nobody can make everyone
/// else's queues go.
const MAX_QUEUES_EACH: usize = 50;
/// How long an unused queue is kept.
const KEEP: Duration = Duration::from_secs(24 * 60 * 60);
/// Items in one queue at most.
const MAX_ITEMS: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    /// Plex's `playQueueItemID`.
    id: i64,
    item: i64,
}

#[derive(Debug, Clone)]
struct Queue {
    user_id: i64,
    entries: Vec<Entry>,
    /// The order before shuffling, to unshuffle to.
    unshuffled: Option<Vec<Entry>>,
    selected: i64,
    source: String,
    version: i64,
    used: Instant,
}

static QUEUES: LazyLock<Mutex<HashMap<i64, Queue>>> = LazyLock::new(Mutex::default);
static NEXT_QUEUE: AtomicI64 = AtomicI64::new(1);
static NEXT_ENTRY: AtomicI64 = AtomicI64::new(1);

fn entries_of(items: &[ItemRow]) -> Vec<Entry> {
    items.iter().take(MAX_ITEMS).map(|i| Entry { id: NEXT_ENTRY.fetch_add(1, Ordering::Relaxed), item: i.id }).collect()
}

fn store(queue: Queue) -> i64 {
    let id = NEXT_QUEUE.fetch_add(1, Ordering::Relaxed);
    let mut queues = QUEUES.lock();
    queues.retain(|_, q| q.used.elapsed() < KEEP);
    let oldest = |queues: &HashMap<i64, Queue>, user: Option<i64>| {
        queues
            .iter()
            .filter(|(_, q)| user.is_none_or(|u| q.user_id == u))
            .min_by_key(|(_, q)| q.used)
            .map(|(id, _)| *id)
    };
    if queues.values().filter(|q| q.user_id == queue.user_id).count() >= MAX_QUEUES_EACH
        && let Some(theirs) = oldest(&queues, Some(queue.user_id))
    {
        queues.remove(&theirs);
    }
    if queues.len() >= MAX_QUEUES
        && let Some(any) = oldest(&queues, None)
    {
        queues.remove(&any);
    }
    queues.insert(id, queue);
    id
}

/// Runs `change` on the user's queue.
fn with_queue<T>(user: &AuthUser, id: i64, change: impl FnOnce(&mut Queue) -> T) -> PlexResult<T> {
    let mut queues = QUEUES.lock();
    let queue =
        queues.get_mut(&id).filter(|q| q.user_id == user.id && q.used.elapsed() < KEEP).ok_or(PlexError::NotFound)?;
    queue.used = Instant::now();
    Ok(change(queue))
}

fn shuffle(entries: &mut [Entry], first: i64) {
    // A Fisher-Yates shuffle from the clock; the selected entry stays first.
    let mut seed =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in (1..entries.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        entries.swap(i, j);
    }
    if let Some(at) = entries.iter().position(|e| e.id == first) {
        entries[..=at].rotate_right(1);
    }
}

/// What a `uri` or `playlistID` names, as the user may see it, in playing
/// order: containers become their episodes or tracks.
async fn resolve(state: &AppState, user: &AuthUser, params: &Params) -> PlexResult<Vec<ItemRow>> {
    if let Some(playlist) = params.parse::<i64>("playlistID") {
        let (_, rows) = playlists::get_rows(&state.db, user, playlist).await?.ok_or(PlexError::NotFound)?;
        return Ok(rows.into_iter().map(|(_, row)| row).collect());
    }
    // python-plexapi encodes the path inside `library:///directory/…` once more.
    let uri = crate::util::percent_decode(params.get("uri").unwrap_or_default(), false);
    if let Some(start) = uri.find("/library/sections/") {
        let rest = &uri[start + "/library/sections/".len()..];
        let (section, query) = rest.split_once("/all").ok_or(PlexError::BadRequest("unknown uri".into()))?;
        let section: i64 = section.parse().map_err(|_| PlexError::NotFound)?;
        let lib =
            visible_libraries(state, user).await?.into_iter().find(|l| l.id == section).ok_or(PlexError::NotFound)?;
        let found = crate::listing::smart_items(&state.db, user.id, &lib, query.trim_start_matches('?')).await?;
        let ids: Vec<i64> = found.iter().map(|r| r.id).collect();
        return Ok(playlists::expand(&state.db, user, &ids).await?);
    }
    if let Some(start) = uri.find("/playlists/") {
        let id = uri[start + "/playlists/".len()..]
            .split(['/', '?'])
            .next()
            .and_then(|i| i.parse().ok())
            .ok_or(PlexError::NotFound)?;
        let (_, rows) = playlists::get_rows(&state.db, user, id).await?.ok_or(PlexError::NotFound)?;
        return Ok(rows.into_iter().map(|(_, row)| row).collect());
    }
    let ids = super::lists::uri_items(&uri);
    if ids.is_empty() {
        return Err(PlexError::BadRequest("uri names nothing to play".into()));
    }
    Ok(playlists::expand(&state.db, user, &ids).await?)
}

/// An episode, then the episodes after it in its show (Plex's continuous play).
async fn continuing(state: &AppState, user: &AuthUser, episode: &ItemRow) -> PlexResult<Vec<ItemRow>> {
    let Some(show) = episode.grandparent_id else { return Ok(vec![episode.clone()]) };
    let all = playlists::expand(&state.db, user, &[show]).await?;
    let from = all.iter().position(|e| e.id == episode.id).unwrap_or(0);
    Ok(all.into_iter().skip(from).collect())
}

/// The queue as Plex lists it: its items, which one plays and where it came from.
async fn container(state: &AppState, user: &AuthUser, id: i64, params: &Params) -> PlexResult<Node> {
    let queue = with_queue(user, id, |q| q.clone())?;
    let ids: Vec<i64> = queue.entries.iter().map(|e| e.item).collect();
    let rows: HashMap<i64, ItemRow> =
        catalog::items_by_ids(&state.db, user.id, &ids).await?.into_iter().map(|r| (r.id, r)).collect();
    // Entries the user may no longer see are left out.
    let visible: Vec<(Entry, ItemRow)> =
        queue.entries.iter().filter_map(|e| rows.get(&e.item).map(|r| (*e, r.clone()))).collect();
    let libraries = visible_libraries(state, user).await?;
    let items: Vec<ItemRow> = visible.iter().map(|(_, r)| r.clone()).collect();
    let nodes = item_nodes(state, &items, &Sections::new(&libraries), listing_detail(params)).await?;
    let offset = visible.iter().position(|(e, _)| e.id == queue.selected).unwrap_or(0);
    let selected = visible.get(offset);
    let mut mc = base_container()
        .attr("size", nodes.len())
        .attr("playQueueID", id)
        .opt("playQueueSelectedItemID", selected.map(|(e, _)| e.id))
        .attr("playQueueSelectedItemOffset", offset)
        .opt("playQueueSelectedMetadataItemID", selected.map(|(_, r)| r.id.to_string()))
        .attr("playQueueShuffled", queue.unshuffled.is_some())
        .attr("playQueueSourceURI", queue.source)
        .attr("playQueueTotalCount", nodes.len())
        .attr("playQueueVersion", queue.version);
    for (node, (entry, _)) in nodes.into_iter().zip(&visible) {
        let mut node = node;
        node.set("playQueueItemID", entry.id);
        mc.push(node);
    }
    Ok(mc)
}

/// `POST /playQueues?type=video&uri=…&key=…&shuffle=0&continuous=0`
pub async fn create(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let mut items = resolve(&state, &user, &params).await?;
    // The item to start with (`key=/library/metadata/{id}`).
    let start = params
        .get("key")
        .and_then(|k| k.rsplit('/').next())
        .and_then(|id| id.parse::<i64>().ok())
        .and_then(|id| items.iter().position(|r| r.id == id));
    if params.flag("continuous") && items.len() == 1 && items[0].kind == "episode" {
        items = continuing(&state, &user, &items[0]).await?;
    }
    let mut entries = entries_of(&items);
    if entries.is_empty() {
        return Err(PlexError::BadRequest("nothing to play".into()));
    }
    let selected = entries[start.unwrap_or(0).min(entries.len() - 1)].id;
    let unshuffled = params.flag("shuffle").then(|| {
        let before = entries.clone();
        shuffle(&mut entries, selected);
        before
    });
    let source = params.get("uri").map(str::to_owned).unwrap_or_default();
    let id = store(Queue { user_id: user.id, entries, unshuffled, selected, source, version: 1, used: Instant::now() });
    container(&state, &user, id, &params).await
}

/// `GET /playQueues/{id}`
pub async fn get(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    container(&state, &user, id, &params).await
}

/// `PUT /playQueues/{id}?uri=…` (or `playlistID=`): appended, or after the
/// playing item with `next=1`.
pub async fn add(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    with_queue(&user, id, |_| ())?;
    let added = entries_of(&resolve(&state, &user, &params).await?);
    let next = params.flag("next");
    with_queue(&user, id, |q| {
        let at =
            if next { q.entries.iter().position(|e| e.id == q.selected).map_or(0, |i| i + 1) } else { q.entries.len() };
        let room = MAX_ITEMS.saturating_sub(q.entries.len());
        q.entries.splice(at..at, added.iter().copied().take(room));
        if let Some(before) = &mut q.unshuffled {
            before.extend(added.iter().copied().take(room));
        }
        q.version += 1;
    })?;
    container(&state, &user, id, &params).await
}

/// `DELETE /playQueues/{id}/items/{playQueueItemID}`
pub async fn remove(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, entry)): Path<(i64, i64)>,
    params: Params,
) -> PlexResult<Node> {
    let found = with_queue(&user, id, |q| {
        let before = q.entries.len();
        q.entries.retain(|e| e.id != entry);
        if let Some(order) = &mut q.unshuffled {
            order.retain(|e| e.id != entry);
        }
        if q.selected == entry {
            q.selected = q.entries.first().map_or(0, |e| e.id);
        }
        q.version += 1;
        q.entries.len() < before
    })?;
    if !found {
        return Err(PlexError::NotFound);
    }
    container(&state, &user, id, &params).await
}

/// `DELETE /playQueues/{id}/items`: all but the playing item.
pub async fn clear(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    with_queue(&user, id, |q| {
        q.entries.retain(|e| e.id == q.selected);
        q.unshuffled = None;
        q.version += 1;
    })?;
    container(&state, &user, id, &params).await
}

/// `PUT /playQueues/{id}/items/{playQueueItemID}/move?after={playQueueItemID}`
/// (first without `after`).
pub async fn move_item(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, entry)): Path<(i64, i64)>,
    params: Params,
) -> PlexResult<Node> {
    let after = params.parse::<i64>("after").filter(|a| *a > 0);
    let moved = with_queue(&user, id, |q| {
        let Some(from) = q.entries.iter().position(|e| e.id == entry) else { return false };
        let moving = q.entries.remove(from);
        let at = match after {
            Some(after) => match q.entries.iter().position(|e| e.id == after) {
                Some(i) => i + 1,
                None => {
                    q.entries.insert(from, moving);
                    return false;
                }
            },
            None => 0,
        };
        q.entries.insert(at, moving);
        q.version += 1;
        true
    })?;
    if !moved {
        return Err(PlexError::NotFound);
    }
    container(&state, &user, id, &params).await
}

/// `PUT /playQueues/{id}/shuffle`: the playing item first, the rest shuffled.
pub async fn shuffled(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    with_queue(&user, id, |q| {
        if q.unshuffled.is_none() {
            q.unshuffled = Some(q.entries.clone());
        }
        let first = q.selected;
        shuffle(&mut q.entries, first);
        q.version += 1;
    })?;
    container(&state, &user, id, &params).await
}

/// `PUT /playQueues/{id}/unshuffle`: back to the order it had.
pub async fn unshuffled(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
    params: Params,
) -> PlexResult<Node> {
    with_queue(&user, id, |q| {
        if let Some(order) = q.unshuffled.take() {
            q.entries = order;
            q.version += 1;
        }
    })?;
    container(&state, &user, id, &params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffling_keeps_every_entry_and_the_playing_one_first() {
        let mut entries: Vec<Entry> = (1..=50).map(|i| Entry { id: i, item: i * 10 }).collect();
        shuffle(&mut entries, 17);
        assert_eq!(entries[0].id, 17);
        let mut ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=50).collect::<Vec<_>>());
    }
}
