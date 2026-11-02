//! What a search found, sorted into the page's rows (`SearchViewModel.searchGroups` and the TV
//! app's `GROUPS` in `search.tsx`), what each tile says and shows (`ArtworkTileModel.of`,
//! `ArtworkCandidates`), and where a result opens (`SearchViewModel.opens`); then what TMDB has for
//! the words that the libraries lack, as the Discover row after them shows it (`search.tsx`).

use pley_base::ImageRequest;
use pley_core::ApiError;
use pley_core::format::{
    card_subtitle, episode_label, format_time_left, is_in_progress, progress_fraction, unwatched_count,
};
use pley_core::model::{
    DiscoverPage, DiscoverTitle, ItemKind, ItemSummary, LiveChannel, LiveProgram, PersonSummary, SearchResults,
    TitleKind,
};
use pley_core::requests::{Tone, add_titles, title_badge, title_subtitle, title_tone};
use pley_kit::TileKind;
use pley_shell::Route;

/// The rows results come in, in the order they show: People, Live TV, then each kind of title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    People,
    Live,
    Movies,
    Shows,
    Seasons,
    Episodes,
    Artists,
    Albums,
    Tracks,
}

impl Group {
    /// Every group, in the page's order (Live TV's row after People, as on the web).
    pub const ALL: [Group; 9] = [
        Group::People,
        Group::Live,
        Group::Movies,
        Group::Shows,
        Group::Seasons,
        Group::Episodes,
        Group::Artists,
        Group::Albums,
        Group::Tracks,
    ];

    /// The row's header (`strings_tv_search.xml`).
    pub fn title(self) -> &'static str {
        match self {
            Group::People => "People",
            Group::Live => "Live TV",
            Group::Movies => "Movies",
            Group::Shows => "Shows",
            Group::Seasons => "Seasons",
            Group::Episodes => "Episodes",
            Group::Artists => "Artists",
            Group::Albums => "Albums",
            Group::Tracks => "Tracks",
        }
    }

    /// How its results are drawn (`tvTileKind`): people as circles, channels as Live TV's tiles,
    /// movies, shows and seasons as posters at their row size, episodes as stills, music square.
    pub fn tile(self) -> TileKind {
        match self {
            Group::People => TileKind::Person,
            Group::Live => TileKind::Episode,
            Group::Movies | Group::Shows | Group::Seasons => TileKind::Poster,
            Group::Episodes => TileKind::Landscape,
            Group::Artists | Group::Albums | Group::Tracks => TileKind::Square,
        }
    }

    /// The kind of title the group holds.
    pub fn item_kind(self) -> Option<ItemKind> {
        match self {
            Group::People | Group::Live => None,
            Group::Movies => Some(ItemKind::Movie),
            Group::Shows => Some(ItemKind::Show),
            Group::Seasons => Some(ItemKind::Season),
            Group::Episodes => Some(ItemKind::Episode),
            Group::Artists => Some(ItemKind::Artist),
            Group::Albums => Some(ItemKind::Album),
            Group::Tracks => Some(ItemKind::Track),
        }
    }

    /// Its place in [`Group::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A result's tile: what it says, its pictures and its marks.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultTile {
    /// The item's or person's id.
    pub id: i64,
    pub title: String,
    pub subtitle: String,
    /// The pictures to try, first choice first.
    pub art: Vec<ImageRequest>,
    /// How far it was played, when in progress.
    pub progress: Option<f32>,
    pub watched: bool,
    /// A show's or season's unwatched episodes.
    pub unwatched: u32,
    /// The page Select opens.
    pub opens: Route,
}

/// A Live TV channel found, with its logo.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelResult {
    pub channel: LiveChannel,
    pub logo: Option<ImageRequest>,
}

/// What a search found, sorted into rows and ready to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub people: Vec<ResultTile>,
    pub channels: Vec<ChannelResult>,
    /// The titles by group (Movies to Tracks), each in the server's order of relevance.
    pub titles: Vec<(Group, Vec<ResultTile>)>,
}

/// Turns a picture's address on the server into what the tile asks for at the size of a `TileKind`
/// tile; `None` without a server.
pub type Resolve<'a> = &'a dyn Fn(&str, TileKind) -> Option<ImageRequest>;

/// The most titles an answer shows: as many as were asked for.
pub const MAX_TITLES: usize = crate::query::LIMIT as usize;
/// The most people and the most channels an answer shows, as the server sends (12 of each).
pub const MAX_PEOPLE: usize = 12;
pub const MAX_CHANNELS: usize = 12;

/// The pictures' widths in device pixels for each kind of tile the results use, measured on the
/// UI thread so that an answer becomes tiles on the task pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Widths([u32; 5]);

impl Widths {
    const KINDS: [TileKind; 5] =
        [TileKind::Person, TileKind::Episode, TileKind::Poster, TileKind::Landscape, TileKind::Square];

    /// The widths `px` gives for each kind's width in dp (`Ui::image_px`).
    pub fn new(px: impl Fn(f32) -> u32) -> Widths {
        Widths(Widths::KINDS.map(|k| px(k.size().w)))
    }

    /// A `kind` tile's picture width.
    pub fn of(&self, kind: TileKind) -> u32 {
        let i = Widths::KINDS.iter().position(|k| *k == kind).unwrap_or(2);
        self.0[i]
    }
}

impl Found {
    /// The server's answer sorted into rows: people, channels, then each kind of title. Items of
    /// other kinds (extras) are left out, as on the web; empty groups too. However long the answer,
    /// no more than the server should send is kept.
    pub fn new(results: SearchResults, resolve: Resolve) -> Found {
        let items = &results.items[..results.items.len().min(MAX_TITLES)];
        let people = results.people.iter().take(MAX_PEOPLE).map(|p| person_tile(p, resolve)).collect();
        let channels = results
            .channels
            .into_iter()
            .take(MAX_CHANNELS)
            .map(|channel| {
                let logo =
                    channel.logo.as_deref().filter(|l| !l.is_empty()).and_then(|l| resolve(l, TileKind::Episode));
                ChannelResult { channel, logo }
            })
            .collect();
        let titles = Group::ALL
            .iter()
            .filter_map(|&group| {
                let kind = group.item_kind()?;
                let tiles: Vec<ResultTile> =
                    items.iter().filter(|i| i.kind == kind).map(|i| item_tile(i, group.tile(), resolve)).collect();
                (!tiles.is_empty()).then_some((group, tiles))
            })
            .collect();
        Found { people, channels, titles }
    }

    /// Nothing to show: `No results for “north”`.
    pub fn is_empty(&self) -> bool {
        self.people.is_empty() && self.channels.is_empty() && self.titles.is_empty()
    }

    /// The rows to show, in order.
    pub fn groups(&self) -> Vec<Group> {
        let mut out = Vec::with_capacity(self.titles.len() + 2);
        if !self.people.is_empty() {
            out.push(Group::People);
        }
        if !self.channels.is_empty() {
            out.push(Group::Live);
        }
        out.extend(self.titles.iter().map(|(g, _)| *g));
        out
    }

    /// The tiles of a row of people or titles (none for Live TV's).
    pub fn tiles(&self, group: Group) -> &[ResultTile] {
        match group {
            Group::People => &self.people,
            Group::Live => &[],
            _ => self.titles.iter().find(|(g, _)| *g == group).map_or(&[], |(_, t)| t.as_slice()),
        }
    }
}

/// A person found: a circle with their photo, their name under it; Select opens their page.
pub fn person_tile(person: &PersonSummary, resolve: Resolve) -> ResultTile {
    let art = person.photo_url.as_deref().filter(|u| !u.is_empty()).and_then(|u| resolve(u, TileKind::Person));
    ResultTile {
        id: person.id,
        title: person.name.clone(),
        subtitle: String::new(),
        art: art.into_iter().collect(),
        progress: None,
        watched: false,
        unwatched: 0,
        opens: Route::Person { id: person.id },
    }
}

/// A title found, drawn as `kind` (`ArtworkTileModel.of`): an episode as a still says its show
/// over "S2 · E5 · <its title>" (or the time left); the others their title over the time left or
/// the card's line ("2019", "3 seasons", "Daft Punk · 2001").
pub fn item_tile(item: &ItemSummary, kind: TileKind, resolve: Resolve) -> ResultTile {
    let in_progress = is_in_progress(item);
    let time_left =
        if in_progress { format_time_left(item.user_state.position_ms, item.duration_ms) } else { String::new() };
    let (title, subtitle) = if kind == TileKind::Landscape && item.kind == ItemKind::Episode {
        let show = item.grandparent_title.as_deref().filter(|t| !t.is_empty()).unwrap_or(&item.title);
        let rest = if !time_left.is_empty() {
            time_left
        } else if show != item.title {
            item.title.clone()
        } else {
            String::new()
        };
        let line = [episode_label(item.parent_index, item.index), rest]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        (show.to_string(), line)
    } else {
        let line = if time_left.is_empty() { card_subtitle(item) } else { time_left };
        (item.title.clone(), line)
    };
    let paths =
        if matches!(kind, TileKind::Landscape | TileKind::Episode) { thumb_art(item) } else { poster_art(item) };
    let unwatched = if matches!(item.kind, ItemKind::Show | ItemKind::Season) { unwatched_count(item) } else { 0 };
    ResultTile {
        id: item.id,
        title,
        subtitle,
        art: paths.iter().filter_map(|p| resolve(p, kind)).collect(),
        progress: in_progress.then(|| progress_fraction(item.user_state.position_ms, item.duration_ms)),
        watched: item.user_state.watched && unwatched == 0,
        unwatched: unwatched.clamp(0, i64::from(u32::MAX)) as u32,
        opens: opens(item),
    }
}

fn parent_art(id: i64, kind: &str) -> String {
    format!("/api/items/{id}/images/{kind}")
}

/// A poster's pictures (`ArtworkCandidates.poster`): its own, then its parent's (an episode's
/// season, a track's album), then its grandparent's (the show, the artist).
pub fn poster_art(item: &ItemSummary) -> Vec<String> {
    let mut out = Vec::with_capacity(3);
    if let Some(p) = item.poster_url.as_deref().filter(|p| !p.is_empty()) {
        out.push(p.to_string());
    }
    out.extend(item.parent_id.map(|id| parent_art(id, "poster")));
    out.extend(item.grandparent_id.map(|id| parent_art(id, "poster")));
    out
}

/// A 16:9 still's pictures (`ArtworkCandidates.thumb`): a movie's backdrop first (its thumb is a
/// frame grab), else its own thumb, then an episode's show backdrop or another kind's own backdrop.
pub fn thumb_art(item: &ItemSummary) -> Vec<String> {
    let backdrop = item.backdrop_url.as_deref().filter(|b| !b.is_empty());
    let mut out: Vec<String> = Vec::with_capacity(3);
    if item.kind == ItemKind::Movie
        && let Some(b) = backdrop
    {
        out.push(b.to_string());
    }
    if let Some(t) = item.thumb_url.as_deref().filter(|t| !t.is_empty()) {
        out.push(t.to_string());
    }
    if item.kind == ItemKind::Episode
        && let Some(show) = item.grandparent_id
    {
        out.push(parent_art(show, "backdrop"));
    }
    if item.kind != ItemKind::Episode
        && let Some(b) = backdrop
        && !out.iter().any(|o| o == b)
    {
        out.push(b.to_string());
    }
    out
}

/// Where Select on a title goes: an artist's or album's music page, a track its album (with the
/// track picked out), anything else its item page.
pub fn opens(item: &ItemSummary) -> Route {
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

/// What a channel tile's caption says under its name: what's on at `now` (unix seconds), or why
/// nothing is (`strings_tv_livetv.xml`).
pub fn channel_caption(channel: &LiveChannel, now: i64) -> &str {
    match channel.on_at(now) {
        Some(on) => &on.title,
        None if channel.has_guide => NOTHING_NOW,
        None => NO_GUIDE,
    }
}

/// What's on a channel at `now` and how far it is, for the line along the tile's foot.
pub fn channel_progress(channel: &LiveChannel, now: i64) -> Option<f32> {
    channel.on_at(now).map(|p: &LiveProgram| p.progress_at(now))
}

/// A channel with a guide that lists nothing now.
pub const NOTHING_NOW: &str = "Nothing in the guide now";
/// A channel without a guide.
pub const NO_GUIDE: &str = "No guide for this channel";

/// The Discover row shows up to 20 titles, from at most 3 of TMDB's pages (an age limit can leave
/// a page empty), as the web's does.
pub const TMDB_TITLES: usize = 20;
pub const TMDB_PAGES: i64 = 3;

/// A title TMDB has for the words that the libraries lack, as Discover's row shows it: a poster
/// with its badge ("In library", "Requested"…) in the badge's colour, its name over its year and
/// kind.
#[derive(Clone, Debug, PartialEq)]
pub struct TmdbTile {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    pub title: String,
    pub subtitle: String,
    pub art: Vec<ImageRequest>,
    pub badge: Option<&'static str>,
    pub tone: Tone,
}

impl TmdbTile {
    /// What names it in its row: TMDB numbers movies and shows apart.
    pub fn key(&self) -> u64 {
        ((self.tmdb_id as u64) << 1) | u64::from(self.kind == TitleKind::Show)
    }

    /// Its page, Request and all.
    pub fn opens(&self) -> Route {
        Route::Title { kind: self.kind, tmdb_id: self.tmdb_id, pick: false }
    }
}

/// A TMDB title as a tile, its poster at a poster tile's size.
pub fn tmdb_tile(t: &DiscoverTitle, resolve: Resolve) -> TmdbTile {
    let poster = t.poster_url.as_deref().filter(|p| !p.is_empty()).and_then(|p| resolve(p, TileKind::Poster));
    TmdbTile {
        kind: t.kind,
        tmdb_id: t.tmdb_id,
        title: t.title.clone(),
        subtitle: title_subtitle(t.kind, t.year, true),
        art: poster.into_iter().collect(),
        badge: title_badge(t.status, t.item_id),
        tone: title_tone(t.status, t.item_id),
    }
}

/// What TMDB has for the words, the libraries' whole titles left out (`missing`), read with
/// `page`: pages in turn until 20 titles came or the third was read, each title once. The first
/// page failing is the answer; a later one leaves what the first ones brought.
pub fn read_tmdb(mut page: impl FnMut(i64) -> Result<DiscoverPage, ApiError>) -> Result<Vec<DiscoverTitle>, ApiError> {
    let mut titles = Vec::new();
    let mut n = 1;
    loop {
        let p = match page(n) {
            Ok(p) => p,
            Err(e) if n == 1 => return Err(e),
            Err(_) => break,
        };
        add_titles(&mut titles, p.results);
        if titles.len() >= TMDB_TITLES || n >= p.total_pages.min(TMDB_PAGES) {
            break;
        }
        n += 1;
    }
    titles.truncate(TMDB_TITLES);
    Ok(titles)
}

#[cfg(test)]
mod tests {
    use pley_core::model::UserState;

    use super::*;

    fn resolve(path: &str, kind: TileKind) -> Option<ImageRequest> {
        Some(ImageRequest::new(format!("https://tv.example{path}"), kind.size().w as u32))
    }

    fn item(id: i64, kind: ItemKind, title: &str) -> ItemSummary {
        ItemSummary { id, kind, title: title.into(), ..ItemSummary::default() }
    }

    fn urls(tile: &ResultTile) -> Vec<&str> {
        tile.art.iter().map(|r| &*r.url).collect()
    }

    #[test]
    fn rows_come_in_the_pages_order_and_drop_extras_and_empty_groups() {
        let results = SearchResults {
            items: vec![
                item(1, ItemKind::Track, "North Song"),
                item(2, ItemKind::Movie, "North"),
                item(3, ItemKind::Extra, "North: trailer"),
                item(4, ItemKind::Episode, "Northbound"),
                item(5, ItemKind::Movie, "North by Northwest"),
                item(6, ItemKind::Artist, "Northern Lights"),
            ],
            people: vec![PersonSummary { id: 7, name: "Nora North".into(), photo_url: None }],
            channels: vec![LiveChannel { id: 8, name: "North News".into(), ..LiveChannel::default() }],
        };
        let found = Found::new(results, &resolve);
        assert_eq!(
            found.groups(),
            [Group::People, Group::Live, Group::Movies, Group::Episodes, Group::Artists, Group::Tracks]
        );
        let movies: Vec<i64> = found.tiles(Group::Movies).iter().map(|t| t.id).collect();
        assert_eq!(movies, [2, 5], "the server's order inside a group");
        assert!(found.titles.iter().all(|(_, tiles)| tiles.iter().all(|t| t.id != 3)), "extras are left out");
        assert!(found.tiles(Group::Live).is_empty());
        assert_eq!(found.people[0].opens, Route::Person { id: 7 });
        assert!(found.people[0].art.is_empty(), "no photo, no picture");
    }

    #[test]
    fn an_answer_longer_than_asked_is_cut_short() {
        let results = SearchResults {
            items: (0..500).map(|i| item(i, ItemKind::Movie, "North")).collect(),
            people: (0..100).map(|i| PersonSummary { id: i, name: "N".into(), photo_url: None }).collect(),
            channels: (0..100).map(|i| LiveChannel { id: i, ..LiveChannel::default() }).collect(),
        };
        let found = Found::new(results, &resolve);
        assert_eq!(found.tiles(Group::Movies).len(), MAX_TITLES);
        assert_eq!((found.people.len(), found.channels.len()), (MAX_PEOPLE, MAX_CHANNELS));
    }

    #[test]
    fn widths_follow_the_tile_kinds() {
        let w = Widths::new(|dp| (dp * 2.0) as u32);
        assert_eq!(w.of(TileKind::Poster), (TileKind::Poster.size().w * 2.0) as u32);
        assert_eq!(w.of(TileKind::Person), (TileKind::Person.size().w * 2.0) as u32);
    }

    #[test]
    fn an_answer_of_extras_alone_finds_nothing() {
        let results = SearchResults { items: vec![item(3, ItemKind::Extra, "Trailer")], ..SearchResults::default() };
        assert!(Found::new(results, &resolve).is_empty());
        assert!(Found::new(SearchResults::default(), &resolve).is_empty());
    }

    #[test]
    fn episodes_say_their_show_over_their_number_and_title() {
        let mut e = item(20, ItemKind::Episode, "Glasshouse");
        e.grandparent_title = Some("Severance".into());
        e.grandparent_id = Some(10);
        e.parent_index = Some(2);
        e.index = Some(5);
        e.thumb_url = Some("/api/items/20/images/thumb?v=1".into());
        let t = item_tile(&e, TileKind::Landscape, &resolve);
        assert_eq!((t.title.as_str(), t.subtitle.as_str()), ("Severance", "S2 · E5 · Glasshouse"));
        assert_eq!(
            urls(&t),
            ["https://tv.example/api/items/20/images/thumb?v=1", "https://tv.example/api/items/10/images/backdrop"]
        );

        // In progress: the time left instead of the title.
        e.duration_ms = Some(60 * 60_000);
        e.user_state = UserState { position_ms: 34 * 60_000, ..UserState::default() };
        let t = item_tile(&e, TileKind::Landscape, &resolve);
        assert_eq!(t.subtitle, "S2 · E5 · 26 min left");
        assert!(t.progress.is_some_and(|p| (p - 34.0 / 60.0).abs() < 1e-3));
    }

    #[test]
    fn titles_say_their_card_line() {
        let mut album = item(41, ItemKind::Album, "Discovery");
        album.parent_title = Some("Daft Punk".into());
        album.year = Some(2001);
        album.parent_id = Some(40);
        let t = item_tile(&album, TileKind::Square, &resolve);
        assert_eq!(t.subtitle, "Daft Punk · 2001");
        assert_eq!(urls(&t), ["https://tv.example/api/items/40/images/poster"], "the artist's picture stands in");
        assert_eq!(t.opens, Route::Album { id: 41, track_id: None });

        let mut track = item(42, ItemKind::Track, "One More Time");
        track.grandparent_title = Some("Daft Punk".into());
        track.parent_title = Some("Discovery".into());
        track.parent_id = Some(41);
        track.grandparent_id = Some(40);
        let t = item_tile(&track, TileKind::Square, &resolve);
        assert_eq!(t.subtitle, "Daft Punk · Discovery");
        assert_eq!(t.opens, Route::Album { id: 41, track_id: Some(42) }, "a track opens its album");

        let mut show = item(10, ItemKind::Show, "Severance");
        show.child_count = Some(2);
        show.leaf_count = Some(19);
        show.viewed_leaf_count = Some(4);
        show.poster_url = Some("/api/items/10/images/poster?v=3".into());
        let t = item_tile(&show, TileKind::Poster, &resolve);
        assert_eq!((t.subtitle.as_str(), t.unwatched, t.watched), ("2 seasons", 15, false));
        assert_eq!(urls(&t), ["https://tv.example/api/items/10/images/poster?v=3"]);

        let mut movie = item(2, ItemKind::Movie, "North");
        movie.year = Some(1994);
        movie.user_state.watched = true;
        let t = item_tile(&movie, TileKind::Poster, &resolve);
        assert_eq!((t.subtitle.as_str(), t.watched, t.progress), ("1994", true, None));
        assert_eq!(
            item_tile(&item(6, ItemKind::Artist, "A"), TileKind::Square, &resolve).opens,
            Route::Artist { id: 6 }
        );
    }

    #[test]
    fn a_movie_still_starts_with_its_backdrop() {
        let mut movie = item(2, ItemKind::Movie, "North");
        movie.thumb_url = Some("/t".into());
        movie.backdrop_url = Some("/b".into());
        assert_eq!(thumb_art(&movie), ["/b", "/t"]);
        let mut show = item(10, ItemKind::Show, "S");
        show.backdrop_url = Some("/b".into());
        assert_eq!(thumb_art(&show), ["/b"]);
    }

    fn title(kind: TitleKind, tmdb_id: i64, name: &str) -> DiscoverTitle {
        DiscoverTitle { kind, tmdb_id, title: name.into(), ..DiscoverTitle::default() }
    }

    fn page(total: i64, titles: Vec<DiscoverTitle>) -> DiscoverPage {
        DiscoverPage { page: 1, total_pages: total, results: titles }
    }

    #[test]
    fn tmdb_titles_are_badged_and_open_their_page() {
        use pley_core::model::TitleStatus;
        let mut dune = title(TitleKind::Movie, 438631, "Dune");
        dune.year = Some(2021);
        dune.poster_url = Some("/api/discover/images/w342/d.jpg".into());
        dune.status = TitleStatus::Requested;
        let t = tmdb_tile(&dune, &resolve);
        assert_eq!((t.title.as_str(), t.subtitle.as_str()), ("Dune", "2021 · Movie"));
        assert_eq!((t.badge, t.tone), (Some("Requested"), Tone::Blue));
        assert_eq!(urls_of(&t.art), ["https://tv.example/api/discover/images/w342/d.jpg"]);
        assert_eq!(t.opens(), Route::Title { kind: TitleKind::Movie, tmdb_id: 438631, pick: false });

        let mut show = title(TitleKind::Show, 438631, "Dune: Prophecy");
        show.status = TitleStatus::Partial;
        show.item_id = Some(9);
        let s = tmdb_tile(&show, &resolve);
        assert_eq!((s.badge, s.tone, s.subtitle.as_str()), (Some("Partly in library"), Tone::Green, "Show"));
        assert_ne!(s.key(), t.key(), "a movie and a show of the same number are two tiles");
        assert!(s.art.is_empty());
        let none = tmdb_tile(&title(TitleKind::Movie, 1, "Nothing yet"), &resolve);
        assert_eq!(none.badge, None);
    }

    #[test]
    fn tmdb_is_read_until_twenty_titles_or_three_pages() {
        // Pages of eight titles: three are read, and the row keeps twenty.
        let mut asked = Vec::new();
        let titles = read_tmdb(|n| {
            asked.push(n);
            Ok(page(10, (0..8).map(|i| title(TitleKind::Movie, n * 100 + i, "T")).collect()))
        })
        .unwrap();
        assert_eq!(asked, [1, 2, 3]);
        assert_eq!(titles.len(), TMDB_TITLES);

        // The last page TMDB has ends it, and a title on two pages comes once.
        let mut asked = Vec::new();
        let titles = read_tmdb(|n| {
            asked.push(n);
            Ok(page(2, vec![title(TitleKind::Movie, 1, "A"), title(TitleKind::Show, n, "B")]))
        })
        .unwrap();
        assert_eq!(asked, [1, 2]);
        let keys: Vec<(TitleKind, i64)> = titles.iter().map(|t| (t.kind, t.tmdb_id)).collect();
        assert_eq!(keys, [(TitleKind::Movie, 1), (TitleKind::Show, 1), (TitleKind::Show, 2)]);
    }

    #[test]
    fn a_later_page_failing_keeps_what_came() {
        let titles = read_tmdb(|n| match n {
            1 => Ok(page(3, vec![title(TitleKind::Movie, 1, "A")])),
            _ => Err(ApiError::Net(pley_core::NetError::TimedOut)),
        })
        .unwrap();
        assert_eq!(titles.len(), 1);
        let first = read_tmdb(|_| Err(ApiError::Net(pley_core::NetError::Refused)));
        assert!(first.is_err_and(|e| e.unreachable()));
    }

    fn urls_of(art: &[ImageRequest]) -> Vec<&str> {
        art.iter().map(|r| &*r.url).collect()
    }

    #[test]
    fn a_channel_says_whats_on_or_why_nothing_is() {
        let program = |start, stop| LiveProgram { start, stop, title: "News at Ten".into(), ..LiveProgram::default() };
        let mut c = LiveChannel { id: 1, name: "North News".into(), has_guide: true, ..LiveChannel::default() };
        assert_eq!(channel_caption(&c, 1_000), NOTHING_NOW);
        c.has_guide = false;
        assert_eq!(channel_caption(&c, 1_000), NO_GUIDE);
        c.now = Some(program(900, 1_100));
        assert_eq!(channel_caption(&c, 1_000), "News at Ten");
        assert!(channel_progress(&c, 1_000).is_some_and(|p| (p - 0.5).abs() < 1e-3));
        assert_eq!(channel_progress(&c, 2_000), None);
    }
}
