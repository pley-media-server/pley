//! What plays after an item, and seek previews (packages/tv/src/player/nextup.ts).
//! The next item is fetched fresh so its watch state is today's: the queue's
//! next video when playing through a playlist or collection, else the next
//! episode; movies and extras have none.

use pley_core::ApiError;
use pley_core::format::episode_label;
use pley_core::model::{ItemKind, ItemSummary, TrickplayInfo};

use crate::plan::Queue;
use crate::server::PlayerServer;

/// Whether anything can follow an item of `kind` played from `queue`.
pub fn has_next(kind: ItemKind, queue: Option<Queue>) -> bool {
    queue.is_some() || kind == ItemKind::Episode
}

/// What follows `item_id` (blocking: run on a task).
pub fn next_after(
    server: &dyn PlayerServer,
    item_id: i64,
    kind: ItemKind,
    queue: Option<Queue>,
) -> Result<Option<ItemSummary>, ApiError> {
    match queue {
        Some(Queue::Playlist(id)) => {
            let items: Vec<ItemSummary> = server.playlist(id)?.entries.into_iter().map(|e| e.item).collect();
            Ok(next_in(&items, item_id))
        }
        Some(Queue::Collection(id)) => Ok(next_in(&server.collection(id)?.items, item_id)),
        None if kind == ItemKind::Episode => server.next(item_id),
        None => Ok(None),
    }
}

/// The first video after `item_id` in `items`; none when it is the last, or isn't there.
pub fn next_in(items: &[ItemSummary], item_id: i64) -> Option<ItemSummary> {
    let videos: Vec<&ItemSummary> = items.iter().filter(|i| i.kind.playable_video()).collect();
    let at = videos.iter().rposition(|i| i.id == item_id)?;
    videos.get(at + 1).map(|i| (*i).clone())
}

/// Where `next` starts: from the start when watched, else its resume point.
pub fn start_of(next: &ItemSummary) -> i64 {
    if next.user_state.watched { 0 } else { next.user_state.position_ms }
}

/// "S1 · E2" for an episode, none otherwise.
pub fn label_of(next: &ItemSummary) -> Option<String> {
    (next.kind == ItemKind::Episode).then(|| episode_label(next.parent_index, next.index)).filter(|l| !l.is_empty())
}

/// Seek previews of a file (`GET /api/files/{id}/trickplay`) answer `404` until they are made: asked again this often…
pub const TRICKPLAY_RETRY_MS: i64 = 20_000;
/// …for this long.
pub const TRICKPLAY_GIVE_UP_MS: i64 = 10 * 60_000;

/// Where a seek preview sits: its sheet and its place in it, in the sheet's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thumbnail {
    pub sheet: i64,
    pub x: i64,
    pub y: i64,
}

/// Where the thumbnail for `position_ms` is.
pub fn thumbnail_at(info: &TrickplayInfo, position_ms: i64) -> Thumbnail {
    let k = info.thumbnail_at(position_ms);
    let (column, row) = info.cell_of(k);
    Thumbnail { sheet: info.sheet_of(k), x: column * info.width, y: row * info.height }
}

/// The seek preview shown for a target: the 2 s bucket it falls in
/// (RES/tv-player-logic.md §4.15), so the sheet changes no more often than that.
pub fn preview_bucket(target_ms: i64) -> i64 {
    target_ms.div_euclid(2000) * 2000
}

/// A sheet's address: `/api/files/{id}/trickplay/{sheet}.jpg?v=…`.
pub fn sheet_path(file_id: i64, info: &TrickplayInfo, sheet: i64) -> String {
    format!("/api/files/{file_id}/trickplay/{sheet}.jpg?v={}", encode_component(&info.id))
}

/// `encodeURIComponent`.
pub(crate) fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: i64, kind: ItemKind) -> ItemSummary {
        ItemSummary { id, kind, ..ItemSummary::default() }
    }

    #[test]
    fn the_next_video_in_a_queue() {
        let items = [summary(1, ItemKind::Movie), summary(2, ItemKind::Track), summary(3, ItemKind::Episode)];
        assert_eq!(next_in(&items, 1).map(|i| i.id), Some(3));
        assert_eq!(next_in(&items, 3), None);
        assert_eq!(next_in(&items, 9), None);
    }

    #[test]
    fn trickplay_maths() {
        let info = TrickplayInfo {
            id: "a b".into(),
            interval_ms: 10_000,
            width: 320,
            height: 180,
            columns: 10,
            rows: 10,
            count: 250,
            sheets: 3,
        };
        assert_eq!(thumbnail_at(&info, 0), Thumbnail { sheet: 0, x: 0, y: 0 });
        assert_eq!(thumbnail_at(&info, 1_234_000), Thumbnail { sheet: 1, x: 3 * 320, y: 2 * 180 });
        assert_eq!(thumbnail_at(&info, 99_000_000), Thumbnail { sheet: 2, x: 9 * 320, y: 4 * 180 });
        assert_eq!(sheet_path(7, &info, 1), "/api/files/7/trickplay/1.jpg?v=a%20b");
        assert_eq!(preview_bucket(3_999), 2_000);
        assert_eq!(
            label_of(&ItemSummary {
                kind: ItemKind::Episode,
                parent_index: Some(1),
                index: Some(2),
                ..ItemSummary::default()
            })
            .as_deref(),
            Some("S1 · E2")
        );
        assert_eq!(label_of(&summary(1, ItemKind::Movie)), None);
    }
}
