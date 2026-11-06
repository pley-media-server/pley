//! What Home and the libraries list show, worked out from the server's answers: the rows' tile
//! kinds and "See all", the hero, what the hero, the info block and each tile say, the pictures
//! they try, the page an item opens, and the libraries' own order (`packages/tv/src/ui/screens/
//! home.tsx`, `item-actions.ts`, `library.tsx`, `library-data.ts`, `kit/tile.tsx`, `kit/art.tsx`;
//! Android's `HomeState.kt` and `TvLibraries.kt`). None of it draws, so all of it is tested here.

use pley_base::Icon;
use pley_core::format::{
    card_subtitle, episode_label, format_runtime, format_time_left, is_in_progress, progress_fraction, quality_badges,
    unwatched_count,
};
use pley_core::model::{Hub, ItemDetail, ItemKind, ItemSummary, Library, LibraryKind, LibrarySort};
use pley_i18n::t;
use pley_kit::TileKind;
use pley_shell::{PlayRequest, Route};

/// A row's tiles (`rowShape`, `HomeState.kt:137-157`): Continue Watching wide, a music library's
/// rows square, a row starting with an episode wide, of artists, albums or tracks square, the
/// rest posters.
pub fn row_kind(hub: &Hub, music_libraries: &[i64]) -> TileKind {
    if hub.key.starts_with("continue") {
        return TileKind::Landscape;
    }
    if hub.library_id.is_some_and(|l| music_libraries.contains(&l)) {
        return TileKind::Square;
    }
    match hub.items.first().map(|i| i.kind) {
        Some(ItemKind::Episode) => TileKind::Landscape,
        Some(ItemKind::Artist | ItemKind::Album | ItemKind::Track) => TileKind::Square,
        _ => TileKind::Poster,
    }
}

/// The sort a row's "See all" lists its library in, as the row lists it: the newest added, the
/// newest released or the best rated first (`seeAllOf`, `HomeState.kt:164-176`).
pub fn see_all_sort(hub: &Hub) -> Option<LibrarySort> {
    if hub.key.starts_with("recently_added") {
        Some(LibrarySort::Added)
    } else if hub.key.starts_with("recently_released") {
        Some(LibrarySort::Release)
    } else if hub.key.starts_with("top_rated") {
        Some(LibrarySort::Rating)
    } else {
        None
    }
}

/// Where a row's "See all" goes: its library, sorted as the row is. Only a library's own rows
/// have one.
pub fn see_all_of(hub: &Hub) -> Option<(i64, LibrarySort)> {
    Some((hub.library_id?, see_all_sort(hub)?))
}

/// The hero (`heroOf`, `HomeState.kt:178-188`): the first title of Continue Watching, else the
/// newest video (a movie, a show, a season or an episode) by when it was added, else the newest
/// item.
pub fn hero_of(hubs: &[Hub]) -> Option<&ItemSummary> {
    if let Some(first) = hubs.iter().find(|h| h.key.starts_with("continue")).and_then(|h| h.items.first()) {
        return Some(first);
    }
    let mut video: Option<&ItemSummary> = None;
    let mut any: Option<&ItemSummary> = None;
    for item in hubs.iter().flat_map(|h| &h.items) {
        if any.is_none_or(|a| item.added_at > a.added_at) {
            any = Some(item);
        }
        let is_video = matches!(item.kind, ItemKind::Movie | ItemKind::Show | ItemKind::Season | ItemKind::Episode);
        if is_video && video.is_none_or(|v| item.added_at > v.added_at) {
            video = Some(item);
        }
    }
    video.or(any)
}

/// What Home shows, by what the app knows (`HomeState`, `HomeViewModel.kt:320-340`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HomeLook {
    /// Nothing saved or answered yet, for the first 300 ms.
    Blank,
    /// Nothing saved or answered yet: the still skeleton.
    Skeleton,
    /// The server has no library for this profile.
    NoLibraries,
    /// The rows couldn't be had and none were saved.
    Failed,
    /// Every row is empty while a library scans.
    Scanning,
    /// Every row is empty.
    Empty,
    /// The hero and the rows.
    Rows,
}

/// Nothing shows this long before the skeleton does, so a Home read from what was saved never
/// flashes one (`SKELETON_DELAY_MS`).
pub const SKELETON_AFTER: f64 = 0.3;

/// What Home shows: `hubs` as the app holds them (saved or answered), how many `rows` of them have
/// items, the `libraries` when known, whether asking for the hubs `failed`, whether the server
/// says a library is `scanning`, and how long the page has been open.
pub fn look_of(
    hubs: Option<&[Hub]>,
    rows: usize,
    libraries: Option<&[Library]>,
    failed: bool,
    scanning: bool,
    open_for: f64,
) -> HomeLook {
    let no_libraries = libraries.is_some_and(<[Library]>::is_empty);
    let scanning = scanning || libraries.is_some_and(|l| l.iter().any(|l| l.scanning));
    match hubs {
        None if no_libraries => HomeLook::NoLibraries,
        None if failed => HomeLook::Failed,
        None if open_for < SKELETON_AFTER => HomeLook::Blank,
        None => HomeLook::Skeleton,
        Some(_) if rows > 0 => HomeLook::Rows,
        Some(_) if no_libraries => HomeLook::NoLibraries,
        Some(_) if scanning => HomeLook::Scanning,
        Some(_) => HomeLook::Empty,
    }
}

/// What the hero and the info block take from a title's details, which lists leave out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub summary: Option<String>,
    /// "PG-13", "15", "FSK 12".
    pub content_rating: Option<String>,
    /// The best file's quality badges, "4K", "DV P8.1", "Atmos" (the server lists the tallest
    /// file first).
    pub badges: Vec<String>,
    pub year: Option<i64>,
    pub rating: Option<f64>,
    pub duration_ms: Option<i64>,
}

impl Facts {
    /// The facts of `detail`.
    pub fn of(detail: &ItemDetail) -> Facts {
        Facts {
            summary: detail.summary.summary.clone().filter(|s| !s.trim().is_empty()),
            content_rating: detail.content_rating.clone().filter(|r| !r.trim().is_empty()),
            badges: detail.files.first().map(quality_badges).unwrap_or_default(),
            year: detail.summary.year,
            rating: detail.summary.rating,
            duration_ms: detail.summary.duration_ms,
        }
    }
}

/// What the hero and the info block say of an item (`homeInfo`, `HomeState.kt:195-222`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HomeInfo {
    /// The title; an episode's is its show's.
    pub title: String,
    /// An episode's "S2 · E5 · The Long Night".
    pub context: String,
    pub year: String,
    pub content_rating: String,
    /// "58 min" for a title that plays; "3 seasons", "12 episodes" or an album's artist and year
    /// for the rest.
    pub runtime: String,
    /// The critics' rating as the line writes it after its star ("7.4", "7,4" in French); empty
    /// at 0.
    pub rating: String,
    pub badges: Vec<String>,
    pub summary: String,
    /// "26 min left" while in progress.
    pub time_left: String,
    pub in_progress: bool,
    pub watched: bool,
}

/// What the hero and the info block say of `item`, with its details' `facts` once they came.
pub fn home_info(item: &ItemSummary, facts: Option<&Facts>) -> HomeInfo {
    let episode = item.kind == ItemKind::Episode;
    let runtime = match item.kind {
        ItemKind::Show | ItemKind::Season | ItemKind::Artist | ItemKind::Album => card_subtitle(item),
        _ => format_runtime(item.duration_ms.or(facts.and_then(|f| f.duration_ms))),
    };
    // A show with no seasons counted says its year, which the line has already.
    let says_year = item.kind == ItemKind::Show && item.year.is_some_and(|y| y.to_string() == runtime);
    let title = match &item.grandparent_title {
        Some(show) if episode && !show.is_empty() => show.clone(),
        _ => item.title.clone(),
    };
    let context = if episode {
        [episode_label(item.parent_index, item.index), item.title.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        String::new()
    };
    let in_progress = is_in_progress(item);
    let rating = item.rating.or(facts.and_then(|f| f.rating)).filter(|r| *r > 0.0);
    let summary = facts.and_then(|f| f.summary.as_deref()).or(item.summary.as_deref()).unwrap_or("");
    HomeInfo {
        title,
        context,
        year: item.year.or(facts.and_then(|f| f.year)).map(|y| y.to_string()).unwrap_or_default(),
        content_rating: facts.and_then(|f| f.content_rating.clone()).unwrap_or_default(),
        runtime: if says_year { String::new() } else { runtime },
        rating: rating.map(|r| pley_i18n::decimal(r, 1)).unwrap_or_default(),
        badges: facts.map(|f| f.badges.clone()).unwrap_or_default(),
        summary: summary.trim().to_string(),
        time_left: if in_progress {
            format_time_left(item.user_state.position_ms, item.duration_ms)
        } else {
            String::new()
        },
        in_progress,
        watched: item.user_state.watched,
    }
}

/// What a tile says of an item (`tileModel`, Android's `ArtworkTileModel`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileText {
    pub title: String,
    pub subtitle: String,
    /// How far it was played, while in progress.
    pub progress: Option<f32>,
    /// A show's or a season's episodes not watched yet.
    pub unwatched: u32,
    /// The tick: watched, with nothing unwatched under it.
    pub watched: bool,
}

/// What a `kind` tile says of `item`: a wide tile of an episode names its show, then "S1 · E3 ·"
/// and the time left or the episode's title; the others the title over the time left or the
/// kind's short line ("2019", "3 seasons").
pub fn tile_text(item: &ItemSummary, kind: TileKind) -> TileText {
    let in_progress = is_in_progress(item);
    let left =
        if in_progress { format_time_left(item.user_state.position_ms, item.duration_ms) } else { String::new() };
    let (title, subtitle) = match kind {
        TileKind::Episode => {
            let title = match item.index {
                Some(i) => format!("{i}. {}", item.title),
                None => item.title.clone(),
            };
            (title, if left.is_empty() { format_runtime(item.duration_ms) } else { left })
        }
        TileKind::Landscape if item.kind == ItemKind::Episode => {
            let title = item.grandparent_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| item.title.clone());
            let rest = if !left.is_empty() {
                left
            } else if item.title != title {
                item.title.clone()
            } else {
                String::new()
            };
            let line = [episode_label(item.parent_index, item.index), rest]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            (title, line)
        }
        _ => (item.title.clone(), if left.is_empty() { card_subtitle(item) } else { left }),
    };
    let unwatched = if matches!(item.kind, ItemKind::Show | ItemKind::Season) { unwatched_count(item) } else { 0 };
    TileText {
        title,
        subtitle,
        progress: in_progress.then(|| progress_fraction(item.user_state.position_ms, item.duration_ms)),
        unwatched: u32::try_from(unwatched).unwrap_or(u32::MAX),
        watched: item.user_state.watched && unwatched == 0,
    }
}

/// Which of an item's pictures a place shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArtKind {
    /// 2:3: posters, squares.
    Poster,
    /// 16:9 on a tile: a movie's backdrop, an episode's still.
    Thumb,
    /// Full screen behind the page.
    Backdrop,
}

impl ArtKind {
    /// The picture a tile of `kind` shows: wide tiles a still, the others a poster.
    pub fn of_tile(kind: TileKind) -> ArtKind {
        match kind {
            TileKind::Landscape | TileKind::Episode => ArtKind::Thumb,
            _ => ArtKind::Poster,
        }
    }
}

/// The server's addresses of `item`'s picture of `kind`, first choice first (`artCandidates`): a
/// poster falls back to its parent's and grandparent's; a wide tile shows a movie's backdrop, else
/// the still, else an episode's show's backdrop; a backdrop falls back to its show's, then its
/// parent's.
pub fn art_paths(item: &ItemSummary, kind: ArtKind) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(3);
    let mut add = |path: Option<String>| {
        if let Some(p) = path.filter(|p| !p.is_empty())
            && !out.contains(&p)
        {
            out.push(p);
        }
    };
    let of = |id: Option<i64>, which: &str| id.map(|id| format!("/api/items/{id}/images/{which}"));
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

/// The page an item opens (`pageOf`): an artist's or an album's own, a track's album (showing the
/// track), else the title's page.
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

/// What Play (or Resume) does with an item (`playItem`).
#[derive(Clone, Debug, PartialEq)]
pub enum Play {
    /// A movie, an episode or an extra plays at once (from where it was left unless from the
    /// start).
    Now(PlayRequest),
    /// A show or a season plays its next episode, which the server names (`/on-deck`).
    OnDeck(i64),
    /// Anything else opens its page.
    Open(Route),
}

/// What Play does with `item`, from the start or not.
pub fn play_of(item: &ItemSummary, from_start: bool) -> Play {
    match item.kind {
        ItemKind::Movie | ItemKind::Episode | ItemKind::Extra => {
            Play::Now(PlayRequest { from_start, ..PlayRequest::item(item.id) })
        }
        ItemKind::Show | ItemKind::Season => Play::OnDeck(item.id),
        _ => Play::Open(page_of(item)),
    }
}

/// Where focus goes when tile `item` leaves the row `row` of `rows` (each row's key and its items'
/// ids): the tile after it, else the one before, else the first of the row below, else of the
/// row above (`standIn`, `TvHomeModel.kt:22-31`).
pub fn stand_in<K: Copy + PartialEq>(rows: &[(K, Vec<i64>)], row: K, item: i64) -> Option<(K, i64)> {
    let at = rows.iter().position(|(key, _)| *key == row)?;
    let (key, items) = &rows[at];
    let i = items.iter().position(|id| *id == item)?;
    let beside = items.get(i + 1).or_else(|| i.checked_sub(1).and_then(|p| items.get(p)));
    if let Some(id) = beside {
        return Some((*key, *id));
    }
    let other = rows.get(at + 1).or_else(|| at.checked_sub(1).and_then(|p| rows.get(p)))?;
    other.1.first().map(|id| (other.0, *id))
}

/// Whether the server is asked to read ahead what Play plays when focus rests on `item`: the
/// videos that play themselves (`prefetchPlayback`).
pub fn reads_ahead(item: &ItemSummary) -> bool {
    item.kind.playable_video()
}

/// `item` as marked watched or not before the server says so: no resume point, and a show's or
/// season's episodes all watched or none (`setWatched`'s patch).
pub fn marked(item: &ItemSummary, watched: bool) -> ItemSummary {
    let mut out = item.clone();
    out.user_state.watched = watched;
    out.user_state.position_ms = 0;
    if watched {
        out.user_state.play_count += 1;
    }
    if let Some(leaves) = item.leaf_count {
        out.viewed_leaf_count = Some(if watched { leaves } else { 0 });
    }
    out
}

/// A library's line under its name (`libraryCount`, `HomeState.kt:225-233`): "412 movies",
/// "38 shows · Scanning…", "120 artists", "12 items" of a kind the app doesn't know.
pub fn library_count_line(library: &Library, scanning: bool) -> String {
    let n = library.item_count;
    let base = match library.kind {
        LibraryKind::Movies => t!("library.count.movies", count = n),
        LibraryKind::Shows => t!("library.count.shows", count = n),
        LibraryKind::Music => t!("library.count.artists", count = n),
        LibraryKind::Unknown => t!("library.count.items", count = n),
    };
    if scanning { format!("{base} · {}", t!("library.libraries.scanning")) } else { base }
}

/// A library's icon by kind, as the rail draws it (`kindIcon`).
pub fn kind_icon(kind: LibraryKind) -> Icon {
    match kind {
        LibraryKind::Movies => Icon::Clapperboard,
        LibraryKind::Shows => Icon::Tv,
        LibraryKind::Music => Icon::Music,
        LibraryKind::Unknown => Icon::Film,
    }
}

/// `libraries` in the order of `ids`; those `ids` don't name come after, as they came
/// (`inOrder`).
pub fn in_order(libraries: &[Library], ids: Option<&[i64]>) -> Vec<Library> {
    let Some(ids) = ids else { return libraries.to_vec() };
    let rank = |l: &Library, i: usize| ids.iter().position(|id| *id == l.id).unwrap_or(ids.len() + i);
    let mut ranked: Vec<(usize, &Library)> = libraries.iter().enumerate().map(|(i, l)| (rank(l, i), l)).collect();
    ranked.sort_by_key(|(r, _)| *r);
    ranked.into_iter().map(|(_, l)| l.clone()).collect()
}

/// `ids` with the one at `from` moved to `to` (`moved`).
pub fn moved(ids: &[i64], from: usize, to: usize) -> Vec<i64> {
    let mut next = ids.to_vec();
    if from < next.len() && to < next.len() {
        let id = next.remove(from);
        next.insert(to, id);
    }
    next
}

/// Where a card at `at` of `count` lands moved `by` places through a grid of `columns`: a place
/// along (on to the next row at a row's end), or a row up or down; down onto a shorter last row,
/// at its end. `None` when it can't go that way (`gridStep`).
pub fn grid_step(at: usize, by: isize, count: usize, columns: usize) -> Option<usize> {
    let to = at as isize + by;
    if to < 0 || count == 0 {
        return None;
    }
    let to = to as usize;
    if to < count {
        return Some(to);
    }
    (by > 1 && at / columns < (count - 1) / columns).then_some(count - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::UserState;

    fn item(id: i64, kind: ItemKind, title: &str) -> ItemSummary {
        ItemSummary { id, kind, title: title.into(), ..ItemSummary::default() }
    }

    fn hub(key: &str, library: Option<i64>, items: Vec<ItemSummary>) -> Hub {
        Hub { key: key.into(), title: key.into(), library_id: library, items }
    }

    fn library(id: i64, kind: LibraryKind, count: i64) -> Library {
        Library { id, name: format!("L{id}"), kind, item_count: count, ..Library::default() }
    }

    #[test]
    fn rows_take_the_tiles_of_their_kind() {
        let movie = item(1, ItemKind::Movie, "Arrival");
        let episode = item(2, ItemKind::Episode, "Pilot");
        let album = item(3, ItemKind::Album, "Discovery");
        assert_eq!(row_kind(&hub("continue_watching", None, vec![movie.clone()]), &[]), TileKind::Landscape);
        assert_eq!(row_kind(&hub("recently_added:2", Some(2), vec![episode]), &[]), TileKind::Landscape);
        assert_eq!(row_kind(&hub("recently_added:3", Some(3), vec![movie.clone()]), &[3]), TileKind::Square);
        assert_eq!(row_kind(&hub("my_list", None, vec![album]), &[]), TileKind::Square);
        assert_eq!(row_kind(&hub("recently_added:1", Some(1), vec![movie]), &[3]), TileKind::Poster);
        assert_eq!(row_kind(&hub("my_list", None, vec![]), &[]), TileKind::Poster);
    }

    #[test]
    fn see_all_goes_to_the_rows_library_in_the_rows_order() {
        assert_eq!(see_all_of(&hub("recently_added:1", Some(1), vec![])), Some((1, LibrarySort::Added)));
        assert_eq!(see_all_of(&hub("recently_released:2", Some(2), vec![])), Some((2, LibrarySort::Release)));
        assert_eq!(see_all_of(&hub("top_rated:2", Some(2), vec![])), Some((2, LibrarySort::Rating)));
        assert_eq!(see_all_of(&hub("continue_watching", None, vec![])), None);
        assert_eq!(see_all_of(&hub("my_list", None, vec![])), None);
        assert_eq!(see_all_of(&hub("recently_added", None, vec![])), None);
    }

    #[test]
    fn the_hero_is_continue_watchings_first_else_the_newest_video() {
        let mut old = item(1, ItemKind::Movie, "Old");
        old.added_at = 10;
        let mut new_show = item(2, ItemKind::Show, "Show");
        new_show.added_at = 30;
        let mut newer_movie = item(3, ItemKind::Movie, "Newer");
        newer_movie.added_at = 20;
        let mut extra = item(4, ItemKind::Extra, "Trailer");
        extra.added_at = 40;
        let mut album = item(5, ItemKind::Album, "Album");
        album.added_at = 50;
        // A show added last is the hero, as on Android; an extra or an album never comes before a
        // video.
        let rows = vec![
            hub("recently_added:1", Some(1), vec![old.clone(), newer_movie.clone(), extra]),
            hub("recently_added:2", Some(2), vec![new_show]),
            hub("recently_added:3", Some(3), vec![album.clone()]),
        ];
        assert_eq!(hero_of(&rows).map(|i| i.id), Some(2));
        let movies = vec![hub("recently_added:1", Some(1), vec![old.clone(), newer_movie])];
        assert_eq!(hero_of(&movies).map(|i| i.id), Some(3));
        let music = vec![hub("recently_added:3", Some(3), vec![album])];
        assert_eq!(hero_of(&music).map(|i| i.id), Some(5));
        let mut with_cw = rows.clone();
        with_cw.insert(0, hub("continue_watching", None, vec![old]));
        assert_eq!(hero_of(&with_cw).map(|i| i.id), Some(1));
        assert_eq!(hero_of(&[]), None);
    }

    #[test]
    fn an_episode_is_told_by_its_show_and_its_number() {
        let mut e = item(5, ItemKind::Episode, "The Long Night");
        e.grandparent_title = Some("Winter".into());
        e.parent_index = Some(2);
        e.index = Some(5);
        e.duration_ms = Some(3_480_000);
        e.user_state = UserState { position_ms: 1_920_000, ..UserState::default() };
        let facts = Facts { summary: Some("  Dragons.  ".into()), ..Facts::default() };
        let info = home_info(&e, Some(&facts));
        assert_eq!(info.title, "Winter");
        assert_eq!(info.context, "S2 · E5 · The Long Night");
        assert_eq!(info.runtime, "58 min");
        assert_eq!(info.time_left, "26 min left");
        assert!(info.in_progress);
        assert_eq!(info.summary, "Dragons.");

        let tile = tile_text(&e, TileKind::Landscape);
        assert_eq!(tile.title, "Winter");
        assert_eq!(tile.subtitle, "S2 · E5 · 26 min left");
        assert!(tile.progress.is_some_and(|p| (p - 0.551).abs() < 0.01));
    }

    #[test]
    fn a_show_says_its_seasons_not_its_year_twice() {
        let mut s = item(7, ItemKind::Show, "Fixture Show");
        s.year = Some(2019);
        s.child_count = Some(2);
        s.leaf_count = Some(6);
        s.viewed_leaf_count = Some(2);
        let info = home_info(&s, None);
        assert_eq!(info.runtime, "2 seasons");
        assert_eq!(info.context, "");
        let tile = tile_text(&s, TileKind::Poster);
        assert_eq!((tile.subtitle.as_str(), tile.unwatched, tile.watched), ("2 seasons", 4, false));
        s.child_count = None;
        s.leaf_count = None;
        assert_eq!(home_info(&s, None).runtime, "");
    }

    #[test]
    fn a_watched_movie_has_its_tick_and_no_progress() {
        let mut m = item(9, ItemKind::Movie, "Arrival");
        m.year = Some(2016);
        m.duration_ms = Some(6_960_000);
        m.user_state.watched = true;
        m.rating = Some(7.94);
        let tile = tile_text(&m, TileKind::Poster);
        assert_eq!((tile.subtitle.as_str(), tile.watched, tile.progress), ("2016", true, None));
        let info = home_info(&m, None);
        assert_eq!((info.rating.as_str(), info.runtime.as_str(), info.year.as_str()), ("7.9", "1h 56m", "2016"));
        m.rating = Some(0.0);
        assert_eq!(home_info(&m, None).rating, "");
        // Lists leave some facts out: the details fill them in.
        m.rating = None;
        m.year = None;
        let facts =
            Facts { rating: Some(6.5), year: Some(2015), content_rating: Some("PG".into()), ..Facts::default() };
        let info = home_info(&m, Some(&facts));
        assert_eq!((info.rating.as_str(), info.year.as_str(), info.content_rating.as_str()), ("6.5", "2015", "PG"));
    }

    #[test]
    fn pictures_fall_back_as_the_tv_app_does() {
        let mut e = item(20, ItemKind::Episode, "Pilot");
        e.thumb_url = Some("/api/items/20/images/thumb?v=1".into());
        e.parent_id = Some(11);
        e.grandparent_id = Some(10);
        assert_eq!(art_paths(&e, ArtKind::Thumb), ["/api/items/20/images/thumb?v=1", "/api/items/10/images/backdrop"]);
        assert_eq!(
            art_paths(&e, ArtKind::Backdrop),
            ["/api/items/10/images/backdrop", "/api/items/11/images/backdrop"]
        );
        assert_eq!(art_paths(&e, ArtKind::Poster), ["/api/items/11/images/poster", "/api/items/10/images/poster"]);
        let mut m = item(1, ItemKind::Movie, "Arrival");
        m.backdrop_url = Some("/b".into());
        m.thumb_url = Some("/t".into());
        assert_eq!(art_paths(&m, ArtKind::Thumb), ["/b", "/t"]);
        assert_eq!(ArtKind::of_tile(TileKind::Landscape), ArtKind::Thumb);
        assert_eq!(ArtKind::of_tile(TileKind::Square), ArtKind::Poster);
    }

    #[test]
    fn items_open_their_own_pages_and_play_their_own_way() {
        let mut track = item(62, ItemKind::Track, "Song");
        track.parent_id = Some(61);
        assert_eq!(page_of(&track), Route::Album { id: 61, track_id: Some(62) });
        assert_eq!(page_of(&item(60, ItemKind::Artist, "A")), Route::Artist { id: 60 });
        assert_eq!(page_of(&item(1, ItemKind::Movie, "M")), Route::Item { id: 1 });
        match play_of(&item(1, ItemKind::Movie, "M"), true) {
            Play::Now(r) => assert!(r.item_id == 1 && r.from_start),
            other => panic!("{other:?}"),
        }
        assert_eq!(play_of(&item(2, ItemKind::Season, "S"), false), Play::OnDeck(2));
        assert_eq!(play_of(&track, false), Play::Open(Route::Album { id: 61, track_id: Some(62) }));
        assert!(reads_ahead(&item(3, ItemKind::Extra, "E")));
        assert!(!reads_ahead(&item(4, ItemKind::Show, "S")));
    }

    #[test]
    fn a_tile_taken_out_of_its_row_leaves_focus_to_its_neighbour() {
        let rows = vec![("cw", vec![1, 2, 3]), ("my_list", vec![7]), ("added", vec![9, 10])];
        assert_eq!(stand_in(&rows, "cw", 1), Some(("cw", 2)));
        assert_eq!(stand_in(&rows, "cw", 3), Some(("cw", 2)));
        // The row goes with its only tile: the first of the row below, else of the row above.
        assert_eq!(stand_in(&rows, "my_list", 7), Some(("added", 9)));
        let last = vec![("cw", vec![1]), ("my_list", vec![7])];
        assert_eq!(stand_in(&last, "my_list", 7), Some(("cw", 1)));
        assert_eq!(stand_in(&[("cw", vec![1])], "cw", 1), None);
        assert_eq!(stand_in(&rows, "cw", 42), None);
    }

    #[test]
    fn home_shows_what_it_has_and_says_why_when_nothing() {
        let rows = [hub("continue_watching", None, vec![item(1, ItemKind::Movie, "M")])];
        let none: [Hub; 0] = [];
        let movies = [library(1, LibraryKind::Movies, 10)];
        let mut scanning = movies.clone();
        scanning[0].scanning = true;
        // Nothing yet: blank for 300 ms, then the skeleton; a failure says so.
        assert_eq!(look_of(None, 0, None, false, false, 0.1), HomeLook::Blank);
        assert_eq!(look_of(None, 0, None, false, false, 0.4), HomeLook::Skeleton);
        assert_eq!(look_of(None, 0, Some(&movies), true, false, 0.1), HomeLook::Failed);
        assert_eq!(look_of(None, 0, Some(&[]), true, false, 0.1), HomeLook::NoLibraries);
        // Saved or answered rows show, whatever else is known.
        assert_eq!(look_of(Some(&rows), 1, Some(&movies), true, false, 0.0), HomeLook::Rows);
        assert_eq!(look_of(Some(&none), 0, Some(&[]), false, false, 1.0), HomeLook::NoLibraries);
        assert_eq!(look_of(Some(&none), 0, Some(&scanning), false, false, 1.0), HomeLook::Scanning);
        assert_eq!(look_of(Some(&none), 0, Some(&movies), false, true, 1.0), HomeLook::Scanning);
        assert_eq!(look_of(Some(&none), 0, Some(&movies), false, false, 1.0), HomeLook::Empty);
        assert_eq!(look_of(Some(&none), 0, None, false, false, 1.0), HomeLook::Empty);
    }

    #[test]
    fn marking_clears_the_resume_point_and_counts_the_episodes() {
        let mut s = item(7, ItemKind::Show, "S");
        s.leaf_count = Some(6);
        s.viewed_leaf_count = Some(2);
        s.user_state.position_ms = 5000;
        let w = marked(&s, true);
        assert_eq!((w.user_state.watched, w.user_state.position_ms, w.viewed_leaf_count), (true, 0, Some(6)));
        assert_eq!(marked(&s, false).viewed_leaf_count, Some(0));
    }

    #[test]
    fn libraries_say_how_much_they_hold() {
        assert_eq!(library_count_line(&library(1, LibraryKind::Movies, 1500), false), "1,500 movies");
        assert_eq!(library_count_line(&library(2, LibraryKind::Shows, 38), true), "38 shows · Scanning…");
        assert_eq!(library_count_line(&library(3, LibraryKind::Music, 1), false), "1 artist");
        assert_eq!(library_count_line(&library(4, LibraryKind::Unknown, 12), false), "12 items");
        assert_eq!(kind_icon(LibraryKind::Shows), Icon::Tv);
    }

    #[test]
    fn libraries_count_in_the_language_shown() {
        use pley_i18n::{Language, with_language};
        let line = |l: Library, scanning| library_count_line(&l, scanning);
        with_language(Language::Fr, || {
            assert_eq!(line(library(1, LibraryKind::Movies, 1500), false), "1\u{a0}500 films");
            assert_eq!(line(library(2, LibraryKind::Shows, 1), true), "1 série · Analyse…");
            assert_eq!(line(library(3, LibraryKind::Music, 0), false), "0 artiste");
            assert_eq!(line(library(4, LibraryKind::Movies, 2_000_000), false), "2\u{a0}000\u{a0}000 de films");
        });
        with_language(Language::Es, || {
            assert_eq!(line(library(1, LibraryKind::Movies, 1500), false), "1500 películas");
            assert_eq!(line(library(2, LibraryKind::Shows, 12_000), true), "12.000 series · Analizando…");
            assert_eq!(line(library(4, LibraryKind::Unknown, 1), false), "1 elemento");
        });
        // The critics' rating as the language writes a decimal.
        let movie = ItemSummary { rating: Some(7.94), ..item(9, ItemKind::Movie, "M") };
        assert_eq!(with_language(Language::Fr, || home_info(&movie, None).rating), "7,9");
    }

    #[test]
    fn libraries_follow_an_order_and_move_through_the_grid() {
        let all = vec![
            library(1, LibraryKind::Movies, 1),
            library(2, LibraryKind::Shows, 1),
            library(3, LibraryKind::Music, 1),
        ];
        let ids = |l: Vec<Library>| l.iter().map(|l| l.id).collect::<Vec<_>>();
        assert_eq!(ids(in_order(&all, Some(&[3, 1]))), [3, 1, 2]);
        assert_eq!(ids(in_order(&all, None)), [1, 2, 3]);
        assert_eq!(moved(&[1, 2, 3, 4], 0, 2), [2, 3, 1, 4]);
        assert_eq!(moved(&[1, 2, 3, 4], 3, 0), [4, 1, 2, 3]);
        // Five cards in rows of three: 0 1 2 / 3 4.
        assert_eq!(grid_step(0, 1, 5, 3), Some(1));
        assert_eq!(grid_step(2, 1, 5, 3), Some(3));
        assert_eq!(grid_step(0, -1, 5, 3), None);
        assert_eq!(grid_step(2, 3, 5, 3), Some(4));
        assert_eq!(grid_step(1, 3, 5, 3), Some(4));
        assert_eq!(grid_step(4, 3, 5, 3), None);
        assert_eq!(grid_step(4, -3, 5, 3), Some(1));
    }
}
