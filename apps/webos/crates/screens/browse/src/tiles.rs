//! What a tile shows of an item, worked out once when the item comes (the Android app's
//! `ArtworkTileModel`, the TV app's `tileModel`), so drawing a grid formats nothing; the pictures'
//! addresses tried in turn (`artCandidates`); and the page an item opens.

use pley_base::ImageRequest;
use pley_core::Api;
use pley_core::format::{
    card_subtitle, episode_label, format_bitrate, format_bytes, format_runtime, format_time_left, is_in_progress,
    progress_fraction, summary_badges, unwatched_count,
};
use pley_core::model::{ItemKind, ItemSummary};
use pley_kit::{Tile, TileKind};
use pley_shell::Route;

/// Which picture of an item a tile shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtKind {
    /// The 2:3 poster (a square cover for music), else its season's or show's.
    Poster,
    /// A 16:9 still: a movie's backdrop, an episode's own frame, else its show's backdrop.
    Thumb,
    /// The backdrop, else the show's or the season's.
    Backdrop,
}

impl ArtKind {
    /// The picture tiles of `kind` show.
    pub fn of(kind: TileKind) -> ArtKind {
        match kind {
            TileKind::Landscape | TileKind::Episode => ArtKind::Thumb,
            _ => ArtKind::Poster,
        }
    }
}

/// The server's addresses of an item's picture, first choice first (`artCandidates` in the TV
/// app's `kit/art.tsx`).
pub fn art_paths(item: &ItemSummary, kind: ArtKind) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(3);
    let mut add = |u: Option<String>| {
        if let Some(u) = u.filter(|u| !u.is_empty())
            && !out.contains(&u)
        {
            out.push(u);
        }
    };
    let of = |id: Option<i64>, what: &str| id.map(|id| format!("/api/items/{id}/images/{what}"));
    match kind {
        ArtKind::Poster => {
            add(item.poster_url.clone());
            add(of(item.parent_id, "poster"));
            add(of(item.grandparent_id, "poster"));
        }
        ArtKind::Thumb => {
            if item.kind == ItemKind::Movie {
                add(item.backdrop_url.clone());
            }
            add(item.thumb_url.clone());
            if item.kind == ItemKind::Episode {
                add(of(item.grandparent_id, "backdrop"));
            } else {
                add(item.backdrop_url.clone());
            }
        }
        ArtKind::Backdrop => {
            add(item.backdrop_url.clone());
            add(of(item.grandparent_id, "backdrop"));
            add(of(item.parent_id, "backdrop"));
        }
    }
    out
}

/// The requests for `paths` at `px` device pixels wide.
pub fn requests(api: &Api, paths: &[String], px: u32) -> Vec<ImageRequest> {
    paths.iter().map(|p| api.image(p, px)).collect()
}

/// "42 Mbps · 23.4 GB" under a title of a library sorted by bitrate, or "Bitrate unknown".
pub fn bitrate_line(item: &ItemSummary) -> String {
    let m = item.media.as_ref();
    let parts: Vec<String> = [
        m.and_then(|m| m.bitrate).filter(|b| *b > 0).map(|b| format_bitrate(Some(b))),
        m.and_then(|m| m.size).filter(|s| *s > 0).map(|s| format_bytes(Some(s as u64))),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty())
    .collect();
    if parts.is_empty() { "Bitrate unknown".to_string() } else { parts.join(" · ") }
}

/// The page an item opens: an artist's or an album's for music, a track's album, else its own.
pub fn page_of(item: &ItemSummary) -> Route {
    match item.kind {
        ItemKind::Artist => Route::Artist { id: item.id },
        ItemKind::Album => Route::Album { id: item.id, track_id: None },
        ItemKind::Track => match item.parent_id {
            Some(album) => Route::Album { id: album, track_id: Some(item.id) },
            None => Route::Item { id: item.id },
        },
        _ => Route::Item { id: item.id },
    }
}

/// Whether an item belongs to music, whose pages fetch their own details.
pub fn is_music(item: &ItemSummary) -> bool {
    matches!(item.kind, ItemKind::Artist | ItemKind::Album | ItemKind::Track)
}

/// An item as a tile shows it: its caption, pictures and overlays, worked out once.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub item: ItemSummary,
    pub title: String,
    pub subtitle: String,
    pub art: Vec<ImageRequest>,
    /// How far it was played, when in progress.
    pub progress: Option<f32>,
    pub unwatched: u32,
    pub watched: bool,
    /// Quality badges (a library sorted by bitrate).
    pub badges: Vec<String>,
}

impl Cell {
    /// `item` as a tile of `kind` shows it, its picture asked at `px` device pixels; `bitrate`: a
    /// library sorted by bitrate, whose tiles carry quality badges and the bitrate line.
    pub fn new(item: ItemSummary, api: &Api, kind: TileKind, px: u32, bitrate: bool) -> Cell {
        let art = requests(api, &art_paths(&item, ArtKind::of(kind)), px);
        Cell::with_art(item, kind, art, bitrate)
    }

    /// `item` as a tile of `kind` shows it, with its pictures given.
    pub fn with_art(item: ItemSummary, kind: TileKind, art: Vec<ImageRequest>, bitrate: bool) -> Cell {
        let in_progress = is_in_progress(&item);
        let left =
            if in_progress { format_time_left(item.user_state.position_ms, item.duration_ms) } else { String::new() };
        let mut title = item.title.clone();
        let subtitle = if bitrate {
            bitrate_line(&item)
        } else if kind == TileKind::Episode {
            if let Some(i) = item.index {
                title = format!("{i}. {}", item.title);
            }
            if left.is_empty() { format_runtime(item.duration_ms) } else { left }
        } else if kind == TileKind::Landscape && item.kind == ItemKind::Episode {
            title = item.grandparent_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| item.title.clone());
            let rest = if !left.is_empty() {
                left
            } else if item.title != title {
                item.title.clone()
            } else {
                String::new()
            };
            [episode_label(item.parent_index, item.index), rest]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        } else if left.is_empty() {
            card_subtitle(&item)
        } else {
            left
        };
        let unwatched =
            if matches!(item.kind, ItemKind::Show | ItemKind::Season) { unwatched_count(&item) as u32 } else { 0 };
        let progress = in_progress.then(|| progress_fraction(item.user_state.position_ms, item.duration_ms));
        let badges = if bitrate { item.media.as_ref().map(summary_badges).unwrap_or_default() } else { Vec::new() };
        let watched = item.user_state.watched && unwatched == 0;
        Cell { item, title, subtitle, art, progress, unwatched, watched, badges }
    }

    /// The tile of `kind` showing this cell.
    pub fn tile(&self, kind: TileKind) -> Tile<'_> {
        Tile::new(kind, &self.title)
            .subtitle(&self.subtitle)
            .art(&self.art)
            .progress(self.progress)
            .unwatched(self.unwatched)
            .watched(self.watched)
            .badges(&self.badges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::{MediaSummary, UserState};

    fn movie() -> ItemSummary {
        ItemSummary {
            id: 7,
            kind: ItemKind::Movie,
            title: "Arrival".into(),
            year: Some(2016),
            duration_ms: Some(6_960_000),
            poster_url: Some("/api/items/7/images/poster?v=1".into()),
            backdrop_url: Some("/api/items/7/images/backdrop?v=1".into()),
            ..ItemSummary::default()
        }
    }

    #[test]
    fn pictures_are_tried_in_the_tv_apps_order() {
        let m = movie();
        assert_eq!(art_paths(&m, ArtKind::Poster), vec!["/api/items/7/images/poster?v=1"]);
        assert_eq!(art_paths(&m, ArtKind::Thumb), vec!["/api/items/7/images/backdrop?v=1"]);
        let episode = ItemSummary {
            kind: ItemKind::Episode,
            parent_id: Some(11),
            grandparent_id: Some(10),
            thumb_url: Some("/t".into()),
            ..ItemSummary::default()
        };
        assert_eq!(art_paths(&episode, ArtKind::Thumb), vec!["/t", "/api/items/10/images/backdrop"]);
        assert_eq!(
            art_paths(&episode, ArtKind::Poster),
            vec!["/api/items/11/images/poster", "/api/items/10/images/poster"]
        );
        assert_eq!(
            art_paths(&episode, ArtKind::Backdrop),
            vec!["/api/items/10/images/backdrop", "/api/items/11/images/backdrop"]
        );
    }

    #[test]
    fn tiles_say_what_android_says() {
        let c = Cell::with_art(movie(), TileKind::LibraryGrid, Vec::new(), false);
        assert_eq!((c.title.as_str(), c.subtitle.as_str(), c.progress, c.watched), ("Arrival", "2016", None, false));
        let started =
            ItemSummary { user_state: UserState { position_ms: 3_480_000, ..UserState::default() }, ..movie() };
        let c = Cell::with_art(started, TileKind::LibraryGrid, Vec::new(), false);
        assert_eq!(c.subtitle, "58 min left");
        assert!((c.progress.unwrap() - 0.5).abs() < 1e-3);
        let show = ItemSummary {
            kind: ItemKind::Show,
            leaf_count: Some(10),
            viewed_leaf_count: Some(10),
            child_count: Some(2),
            user_state: UserState { watched: true, ..UserState::default() },
            ..ItemSummary::default()
        };
        let c = Cell::with_art(show.clone(), TileKind::LibraryGrid, Vec::new(), false);
        assert_eq!((c.subtitle.as_str(), c.unwatched, c.watched), ("2 seasons", 0, true));
        let c = Cell::with_art(
            ItemSummary { viewed_leaf_count: Some(7), ..show },
            TileKind::LibraryGrid,
            Vec::new(),
            false,
        );
        assert_eq!((c.unwatched, c.watched), (3, false));
    }

    #[test]
    fn sorted_by_bitrate_tiles_carry_badges_and_the_rate() {
        let m = ItemSummary {
            media: Some(MediaSummary {
                bitrate: Some(42_000_000),
                size: Some(25_125_558_681),
                width: Some(3840),
                height: Some(2160),
                ..MediaSummary::default()
            }),
            ..movie()
        };
        let c = Cell::with_art(m, TileKind::LibraryGrid, Vec::new(), true);
        assert_eq!(c.subtitle, "42 Mbps · 23.4 GB");
        assert_eq!(c.badges, vec!["4K"]);
        let c = Cell::with_art(movie(), TileKind::LibraryGrid, Vec::new(), true);
        assert_eq!(c.subtitle, "Bitrate unknown");
    }

    #[test]
    fn music_opens_its_own_pages() {
        let artist = ItemSummary { id: 3, kind: ItemKind::Artist, ..ItemSummary::default() };
        assert_eq!(page_of(&artist), Route::Artist { id: 3 });
        let track = ItemSummary { id: 9, kind: ItemKind::Track, parent_id: Some(4), ..ItemSummary::default() };
        assert_eq!(page_of(&track), Route::Album { id: 4, track_id: Some(9) });
        assert_eq!(page_of(&movie()), Route::Item { id: 7 });
        assert!(is_music(&track) && !is_music(&movie()));
    }
}
