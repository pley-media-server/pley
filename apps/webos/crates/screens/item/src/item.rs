//! A title's page: a movie, show, season or episode (Android's `TvItemScreen.kt`, `TvItemHero.kt`,
//! `TvEpisodes.kt`, `TvDetails.kt`, `TvMediaInfo.kt`, `TvPanels.kt`; the TV app's `item.tsx`).
//!
//! Over the title's backdrop: the hero (x 96, y 52: the title, its line, the metadata, the summary,
//! the credits, and the buttons Play or Resume, From start, watched, Audio and subtitles, My List,
//! Trailer, Add to playlist, More), then the season tabs and episodes, the extras, cast and related
//! rows, and the details with the versions and media info. The page moves so the focused part sits
//! on its line (tabs at 218, episodes at 272, other rows' headers and the details at 172), and once
//! focus leaves the hero an info block takes the top. Down past the last thing that takes focus
//! reads on through the details, 160 dp a step. Side panels choose the version and tracks, rate the
//! title, go to its show or collections, and act on a tile.

use std::collections::HashSet;

use pley_base::{Color, Icon, Key, Point, Rect, TextOpts};
use pley_core::format::season_title;
use pley_core::model::{
    AudioChoice, CollectionRef, ItemCollections, ItemDetail, ItemKind, ItemList, ItemSummary, StreamChoice, TitleKind,
};
use pley_core::requests::{StageWords, stage_words};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_kit::badge::META_HEIGHT;
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, Button, ColumnState, Dialog, Heading, InfoBlock, List,
    ListRow, ListState, MetaLine, ResponseExt, RoundButton, Row, RowColumn, RowState, SCROLL, Section, SidePanel,
    TileKind, backdrop_px, bar_group,
};
use pley_shell::{AppCx, AppEvent, Chrome, Load, PlayRequest, Resource, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Dirs, Edge, Group, Id, Ui};

use crate::actions::{Effect, ItemAction, effect, item_actions};
use crate::common::{self, ArtKind, Change, Elsewhere, NOTHING_TO_PLAY, TileData};
use crate::page::{
    self, Choice, Credits, FileInfo, Meta, Picks, Primary, PrimaryKind, TrackChoices, default_season, playing,
};
use crate::request::{self, TitleRequest};
use crate::words;

/// The hero's top (`HeroTop`).
const HERO_TOP: f32 = 52.0;
/// The title's box: the title sits at its bottom (`TitleBox`).
const TITLE_BOX: f32 = 72.0;
/// The text column's width: x 96 to 560.
const TEXT_W: f32 = theme::TEXT_COLUMN_END - theme::GUTTER;
/// The season tabs' line while focus is in the episodes (`TabsTop`).
const TABS_TOP: f32 = 218.0;
/// The episode tiles' line (`EpisodeTilesTop`).
const EPISODES_TOP: f32 = 272.0;
/// Room kept above the line under the info block (`Clearance`).
const CLEARANCE: f32 = 12.0;
/// The season tabs' row starts 20 left of the gutter, so the first label lines up with it.
const TABS_X: f32 = theme::GUTTER - 20.0;
/// The details end 200 dp before the screen's right edge (`DetailsEnd`): long lines read better
/// narrower.
const DETAILS_W: f32 = theme::CANVAS_W - theme::GUTTER - 200.0;
/// The details' label column (`LabelWidth`).
const FACT_LABEL_W: f32 = 160.0;
/// Media info's blocks: two side by side, 300 wide, 24 apart, labels 104 wide.
const BLOCK_W: f32 = 300.0;
const BLOCK_GAP: f32 = 24.0;
const BLOCK_LABEL_W: f32 = 104.0;
/// The safe area's bottom line, which reading on brings the end of the page to.
const SAFE_BOTTOM: f32 = theme::CANVAS_H - theme::SAFE_Y;
/// The room the page keeps under its end (`BottomRoom` in `TvItemScreen.kt`): it never moves up
/// further than leaves that much below its last line.
const BOTTOM_ROOM: f32 = 300.0;

const HERO: u64 = 0;
const EPISODES: u64 = 1;
const EXTRAS: u64 = 2;
const CAST: u64 = 3;
const RELATED: u64 = 4;
const DETAILS: u64 = 5;

/// The answers a page asks for besides the title's own, once its kind is known.
#[derive(Default)]
struct Data {
    on_deck: Option<Resource<Option<ItemSummary>>>,
    /// The show's seasons (a show's own children, a season's siblings).
    seasons: Option<Resource<ItemList>>,
    /// The episodes of the season shown.
    episodes: Option<(i64, Resource<ItemList>)>,
    related: Option<Resource<ItemList>>,
    collections: Option<Resource<ItemCollections>>,
}

/// The seasons and episodes part of the page.
struct Seasons {
    /// The season tabs, with more than one season (shows and seasons only).
    tabs: Vec<(i64, String)>,
    selected: Option<i64>,
    /// The header in place of the tabs: the season's title, else "Episodes".
    header: String,
    /// `None` while they are asked for.
    episodes: Option<Vec<TileData>>,
    /// The info block's line for each episode: "Season 2 · Episode 5 · 58 min · Aired 12 March
    /// 2024".
    info_lines: Vec<String>,
    /// The episode the row opens on.
    target: Option<i64>,
}

/// Everything the page shows, made when an answer or a pick changes it.
struct Model {
    item: ItemSummary,
    title: String,
    hero_title: String,
    context: Option<String>,
    meta: Meta,
    genres: String,
    summary: Option<String>,
    /// The tagline and summary, as Full summary shows them.
    full_summary: String,
    credits: Option<Credits>,
    primary: Primary,
    watched: bool,
    my_list: Option<bool>,
    tracks: Option<TrackChoices>,
    trailer: Option<i64>,
    facts: Vec<(String, String)>,
    versions: Vec<Choice>,
    media: Vec<FileInfo>,
    extras: Vec<TileData>,
    cast: Vec<TileData>,
    cast_title: &'static str,
    related: Vec<TileData>,
    seasons: Option<Seasons>,
    rating: Option<i64>,
    rateable: bool,
    show_id: Option<i64>,
    season_id: Option<i64>,
    collections: Vec<CollectionRef>,
    backdrop: BackdropArt,
    /// Where the title's request is (any stage), with the download's bar.
    request: Option<StageWords>,
    /// Request more seasons opens the title's page with its season picker.
    ask_more: Option<(TitleKind, i64)>,
    /// The answers its episode and related tiles were made from.
    revisions: Revisions,
}

/// Counts the changes to the answers whose tiles take longest to make (a daily show's season can
/// hold a thousand episodes), so the page remakes them only when they changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Revisions {
    episodes: u64,
    related: u64,
}

/// A side panel over the page.
#[derive(Clone, Debug, PartialEq)]
enum Panel {
    Tracks,
    More,
    /// A tile's (or the page's own) actions. Closing a panel gives focus back to what opened it
    /// (pley-ui's memory of the group it left).
    Actions {
        item: Box<ItemSummary>,
    },
}

/// What a panel row does.
#[derive(Clone, Debug, PartialEq)]
enum Does {
    Version(i64),
    Audio(Option<i64>),
    Subtitle(Option<i64>),
    Summary,
    Rate(Option<i64>),
    Open(Route),
    Act(ItemAction, Box<ItemSummary>),
}

/// One line of a panel.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Title(String),
    Heading(&'static str),
    Row { label: String, icon: Option<Icon>, check: Option<bool>, does: Does },
}

/// What the person did this frame, applied once the page is drawn.
#[derive(Clone, Debug, PartialEq)]
enum Intent {
    Play { from_start: bool },
    PlayRequest(PlayRequest),
    ToggleWatched,
    ToggleMyList,
    Open(Panel),
    Push(Route),
    Season(i64),
    MediaInfo,
    Row(Does),
    ClosePanel,
    CloseSummary,
}

/// A show's or season's next episode, asked of the server when Play is pressed.
struct PlayNext {
    promise: Promise<Result<Option<ItemSummary>, ApiError>>,
    from_start: bool,
    /// The next episode the page held, played when the server can't be reached (Android's
    /// `freshOr` in `CachedPlayItem`).
    held: Option<i64>,
}

/// A change on its way to the server, with what to put back if it is refused.
struct Pending {
    promise: Promise<Result<(), ApiError>>,
    undo: Undo,
    /// The page asks again for what it shows once the server took it.
    reload: bool,
    /// A title marked watched or not: kept over answers that come before the server takes it,
    /// which were asked before the mark.
    marks: Option<(i64, bool)>,
}

/// What a refused change puts back: only what it changed, so a fresher answer that came meanwhile
/// keeps the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Undo {
    /// The page's title's or one of its episodes' marks.
    Marks {
        id: i64,
        marks: Marks,
    },
    MyList(bool),
    Rating(Option<f64>),
    Nothing,
}

/// What marking watched changes on a title.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Marks {
    watched: bool,
    position_ms: i64,
    viewed_leaf_count: Option<i64>,
}

impl Marks {
    fn of(item: &ItemSummary) -> Marks {
        Marks {
            watched: item.user_state.watched,
            position_ms: item.user_state.position_ms,
            viewed_leaf_count: item.viewed_leaf_count,
        }
    }

    fn put(self, item: &mut ItemSummary) {
        item.user_state.watched = self.watched;
        item.user_state.position_ms = self.position_ms;
        item.viewed_leaf_count = self.viewed_leaf_count;
    }

    /// The marks of `item` once marked `watched` (or not): no place to resume from, and a show or
    /// season counts every episode as seen, or none.
    fn marked(item: &ItemSummary, watched: bool) -> Marks {
        let viewed = match item.leaf_count {
            Some(leaves) => Some(if watched { leaves } else { 0 }),
            None => item.viewed_leaf_count,
        };
        Marks { watched, position_ms: 0, viewed_leaf_count: viewed }
    }
}

/// What the page keeps between frames besides its data.
#[derive(Default)]
struct View {
    column: ColumnState,
    episodes: RowState,
    /// The season the episode row's state is for.
    episodes_for: Option<i64>,
    extras: RowState,
    cast: RowState,
    related: RowState,
    tabs_shift: f32,
    backdrop: BackdropState,
    panel: Option<Panel>,
    panel_open: bool,
    panel_list: ListState,
    /// The open panel's lines and the row it opens on, made when it opens or the page changes.
    entries: Vec<Entry>,
    first: usize,
    entries_stale: bool,
    summary_shown: bool,
    summary_open: bool,
    summary_cut: bool,
    media_open: bool,
    /// The steps read on past the last thing that takes focus, in dp, taken back in turn.
    reading: Vec<f32>,
    /// The section reading on started from.
    reading_from: Option<u64>,
    /// The key of the section focus is in.
    focus_key: Option<u64>,
    /// The element focused last frame: focus moving ends reading.
    last_focus: Option<Id>,
    /// The episode focus was on last; it stays while focus is on the tabs or in a panel
    /// (`TvEpisodes.kt` never clears it).
    focused_episode: Option<usize>,
    /// The episode the info block shows (focus rested on it).
    info_episode: Option<usize>,
    /// Where the hero's parts sit, measured once per model.
    hero: Option<HeroLayout>,
    /// The details' layout, measured once per model and per opening of Media info.
    details: Option<DetailsLayout>,
}

/// A movie's, show's, season's or episode's page.
pub struct ItemScreen {
    detail: Resource<ItemDetail>,
    seed: Option<ItemDetail>,
    data: Option<Data>,
    chosen_season: Option<i64>,
    picks: Picks,
    model: Option<Model>,
    dirty: bool,
    view: View,
    changes: Vec<Pending>,
    told: bool,
    prefetched: Option<(i64, f64)>,
    redirected: bool,
    /// A show's or season's next episode Play is waiting for (the page's own or a tile's).
    play_next: Option<PlayNext>,
    revisions: Revisions,
    /// The title's request, for a movie or show where requests are on.
    request: TitleRequest,
}

impl ItemScreen {
    /// The page of item `id`; a detail seen in the last five minutes shows at once.
    pub fn new(id: i64, cx: &mut AppCx) -> ItemScreen {
        ItemScreen {
            detail: Resource::new(Some(snapshot_keys::item(id)), move |api| api.item(id)),
            seed: cx.state.cached_item(id).cloned(),
            data: None,
            chosen_season: None,
            picks: Picks::default(),
            model: None,
            dirty: true,
            view: View::default(),
            changes: Vec::new(),
            told: false,
            prefetched: None,
            redirected: false,
            play_next: None,
            revisions: Revisions::default(),
            request: TitleRequest::new(id),
        }
    }

    fn shown(&self) -> Option<&ItemDetail> {
        self.detail.value().or(self.seed.as_ref())
    }

    fn shown_mut(&mut self) -> Option<&mut ItemDetail> {
        match self.detail.value_mut() {
            Some(d) => Some(d),
            None => self.seed.as_mut(),
        }
    }

    /// Starts what the page needs and collects what came back.
    fn update(&mut self, cx: &mut AppCx) {
        if let Some(next) = self.play_next.as_mut()
            && (next.promise.ready() || next.promise.failed())
        {
            let (from_start, held) = (next.from_start, next.held);
            match next.promise.take() {
                Some(Ok(Some(episode))) => cx.play(common::play_request(episode.id, from_start)),
                Some(Ok(None)) => cx.toast(NOTHING_TO_PLAY),
                Some(Err(e)) => match held.filter(|_| e.unreachable()) {
                    Some(id) => cx.play(common::play_request(id, from_start)),
                    None => cx.toast_error(e.problem()),
                },
                None => {}
            }
            self.play_next = None;
        }
        // The request is asked once the page has drawn, so it never holds the title up.
        let summary = cx.state.request_summary();
        let wanted = self.model.is_some() && self.shown().is_some_and(|d| page::wants_request(d.kind, summary));
        self.dirty |= self.request.update(cx, wanted);
        let was_fresh = self.detail.fresh();
        let mut changed = self.detail.update(cx);
        if changed && self.detail.fresh() && !was_fresh {
            // The player starts at once from what the page just fetched.
            if let Some(d) = self.detail.value() {
                cx.state.keep_item(d.clone());
            }
        }
        if self.data.is_none()
            && let Some(d) = self.shown()
        {
            self.data = Some(data_for(d));
            changed = true;
        }
        let selected = self.selected_season();
        let Some(data) = self.data.as_mut() else { return };
        if let Some(r) = data.on_deck.as_mut() {
            changed |= r.update(cx);
        }
        if let Some(r) = data.seasons.as_mut() {
            changed |= r.update(cx);
        }
        if let Some(r) = data.related.as_mut()
            && r.update(cx)
        {
            self.revisions.related += 1;
            changed = true;
        }
        if let Some(r) = data.collections.as_mut() {
            changed |= r.update(cx);
        }
        if let Some(season) = selected
            && data.episodes.as_ref().is_none_or(|e| e.0 != season)
        {
            data.episodes =
                Some((season, Resource::new(Some(snapshot_keys::children(season)), move |api| api.children(season))));
            self.revisions.episodes += 1;
            changed = true;
        }
        if let Some((_, r)) = data.episodes.as_mut()
            && r.update(cx)
        {
            self.revisions.episodes += 1;
            changed = true;
        }
        if changed {
            self.keep_marks();
        }
        self.dirty |= changed;
    }

    /// Marks still on their way to the server stay on what just came: an answer asked before
    /// them would otherwise take them back until the page asks again.
    fn keep_marks(&mut self) {
        let marks: Vec<(i64, bool)> = self.changes.iter().filter_map(|c| c.marks).collect();
        for (id, watched) in marks {
            if let Some(d) = self.shown_mut().filter(|d| d.id == id) {
                Marks::marked(&d.summary, watched).put(&mut d.summary);
            }
            for e in self.episode_items_mut().filter(|e| e.id == id) {
                Marks::marked(e, watched).put(e);
            }
        }
    }

    /// The season shown: chosen here, else a season's own, an episode's, or for a show its next
    /// episode's (once the server said which: the page doesn't jump).
    fn selected_season(&self) -> Option<i64> {
        if self.chosen_season.is_some() {
            return self.chosen_season;
        }
        let d = self.shown()?;
        match d.kind {
            ItemKind::Season => Some(d.id),
            ItemKind::Episode => d.parent_id,
            ItemKind::Show => {
                let data = self.data.as_ref()?;
                let deck = data.on_deck.as_ref()?;
                if matches!(deck.state(), Load::Loading) {
                    return None;
                }
                let seasons = data.seasons.as_ref()?.value()?;
                default_season(&seasons.items, deck.value().and_then(|v| v.as_ref()))
            }
            _ => None,
        }
    }

    fn on_deck(&self) -> Option<&ItemSummary> {
        self.data.as_ref()?.on_deck.as_ref()?.value()?.as_ref()
    }

    fn episodes(&self) -> Option<&ItemList> {
        let selected = self.selected_season()?;
        let (season, r) = self.data.as_ref()?.episodes.as_ref()?;
        if *season == selected { r.value() } else { None }
    }

    /// Makes the page's model from the answers and picks.
    fn rebuild(&mut self, ui: &Ui, cx: &AppCx) {
        let old = self.model.take();
        let Some(d) = self.shown() else {
            self.model = old;
            return;
        };
        // The episode and related tiles whose answers didn't change are kept as they are.
        let revisions = self.revisions;
        let (mut kept_episodes, mut kept_related) = (None, None);
        if let Some(old) = old {
            if old.revisions.episodes == revisions.episodes {
                kept_episodes = old.seasons.map(|s| (s.selected, s.episodes, s.info_lines));
            }
            if old.revisions.related == revisions.related {
                kept_related = Some(old.related);
            }
        }
        let data = self.data.as_ref();
        let on_deck = self.on_deck();
        let seasons: &[ItemSummary] =
            data.and_then(|x| x.seasons.as_ref()).and_then(|r| r.value()).map_or(&[], |l| &l.items[..]);
        let selected = self.selected_season();
        let episodes = self.episodes().map(|l| &l.items[..]);
        let related: &[ItemSummary] =
            data.and_then(|x| x.related.as_ref()).and_then(|r| r.value()).map_or(&[], |l| &l.items[..]);
        let collections: Vec<CollectionRef> = data
            .and_then(|x| x.collections.as_ref())
            .and_then(|r| r.value())
            .map(|c| c.collections.clone())
            .unwrap_or_default();

        let playing = playing(d, &self.picks);
        let own_episodes = match d.kind {
            ItemKind::Season if selected == Some(d.id) => episodes,
            _ => None,
        };
        let crew = page::crew_lines(d);
        let kind = d.kind;
        let episode_w = TileKind::Episode.size().w;
        let seasons_model = matches!(kind, ItemKind::Show | ItemKind::Season | ItemKind::Episode).then(|| {
            let tabs: Vec<(i64, String)> = if kind == ItemKind::Episode {
                Vec::new()
            } else {
                seasons.iter().map(|s| (s.id, season_title(Some(&s.title), s.index))).collect()
            };
            let header = match (tabs.as_slice(), kind) {
                ([(_, only)], _) => only.clone(),
                (_, ItemKind::Episode) => d
                    .parent_index
                    .map(|i| season_title(None, Some(i)))
                    .or(d.parent_title.clone().filter(|t| !t.is_empty()))
                    .unwrap_or_else(|| "Episodes".into()),
                _ => "Episodes".into(),
            };
            let (episodes, info_lines) = match kept_episodes.take() {
                Some((at, tiles, lines)) if at == selected => (tiles, lines),
                _ => {
                    let mut seen = HashSet::new();
                    let unique: Option<Vec<&ItemSummary>> =
                        episodes.map(|list| list.iter().filter(|e| seen.insert(e.id)).collect());
                    let lines = unique.iter().flatten().map(|e| words::episode_info_line(e)).collect();
                    let tiles = unique.map(|list| {
                        list.into_iter()
                            .map(|e| {
                                let length = words::episode_length(e);
                                let mut t = TileData::of(cx, ui, e, ArtKind::Thumb, episode_w, length);
                                t.title = words::episode_tile_title(e);
                                t
                            })
                            .collect()
                    });
                    (tiles, lines)
                }
            };
            Seasons {
                tabs: if tabs.len() > 1 { tabs } else { Vec::new() },
                selected,
                header,
                info_lines,
                episodes,
                target: page::episode_target(d, on_deck, selected),
            }
        });
        let landscape_w = TileKind::Landscape.size().w;
        let mut extra_ids = HashSet::new();
        let extras = d
            .extras
            .iter()
            .filter(|e| extra_ids.insert(e.id))
            .map(|e| TileData::of(cx, ui, e, ArtKind::Thumb, landscape_w, words::extra_subtitle(e)))
            .collect();
        let person_px = ui.image_px(TileKind::Person.size().w);
        let cast: Vec<TileData> = page::cast_and_crew(d, &crew)
            .into_iter()
            .map(|p| TileData {
                id: p.id,
                art: p.photo.as_deref().and_then(|u| cx.image(u, person_px)).into_iter().collect(),
                title: p.name,
                subtitle: p.subtitle,
                progress: None,
                watched: false,
                unwatched: 0,
                item: None,
            })
            .collect();
        let poster_w = TileKind::Poster.size().w;
        let related = match kept_related {
            Some(tiles) => tiles,
            None if kind.listable() => {
                let mut ids = HashSet::from([d.id]);
                related
                    .iter()
                    .filter(|r| ids.insert(r.id))
                    .map(|r| TileData::of(cx, ui, r, ArtKind::Poster, poster_w, pley_core::format::card_subtitle(r)))
                    .collect()
            }
            None => Vec::new(),
        };
        let hero_title = page::hero_title(d);
        let summary = cx.state.request_summary();
        let answer = self.request.answer().filter(|_| page::wants_request(kind, summary));
        let manage = summary.is_some_and(|s| s.manage);
        let stage = answer.and_then(|a| a.request.as_ref()).map(|r| stage_words(r, manage, request::tv_time(cx)));
        let ask_more = answer.filter(|a| page::asks_more(a, summary)).map(|a| (a.kind, a.tmdb_id));
        let backdrop = BackdropArt::new(
            d.id as u64,
            common::requests(cx, &common::art_paths(&d.summary, ArtKind::Backdrop), backdrop_px(ui)),
            hero_title.clone(),
        );
        self.model = Some(Model {
            item: d.summary.clone(),
            title: page::own_title(d),
            context: page::context_line(d),
            meta: page::meta(d, playing.file, seasons),
            genres: page::genres_line(d, &collections),
            summary: d.summary.summary.clone().filter(|s| !s.trim().is_empty()),
            full_summary: [d.tagline.as_deref(), d.summary.summary.as_deref()]
                .into_iter()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            credits: page::credits(d, &crew),
            primary: page::primary(d, on_deck, own_episodes),
            watched: page::watched(&d.summary),
            my_list: kind.listable().then_some(d.in_watchlist),
            tracks: page::track_choices(d, &playing),
            trailer: page::trailer(d).map(|t| t.id),
            facts: page::facts(d, &crew),
            versions: d
                .files
                .iter()
                .map(|f| Choice {
                    id: f.id,
                    label: page::version_long(f),
                    selected: Some(f.id) == playing.file.map(|p| p.id),
                })
                .collect(),
            media: page::media_info(&d.files),
            extras,
            cast_title: words::cast_title(kind == ItemKind::Episode, !d.cast.is_empty(), !crew.is_empty()),
            cast,
            related,
            seasons: seasons_model,
            rating: page::user_rating(d),
            rateable: page::rateable(kind),
            show_id: match kind {
                ItemKind::Episode => d.grandparent_id,
                ItemKind::Season => d.parent_id,
                _ => None,
            },
            season_id: if kind == ItemKind::Episode { d.parent_id } else { None },
            collections,
            hero_title,
            backdrop,
            request: stage,
            ask_more,
            revisions,
        });
        self.dirty = false;
        self.view.hero = None;
        self.view.details = None;
        self.view.entries_stale = true;
    }

    /// Play (or From start): a video plays itself with the version and tracks chosen here; a show
    /// or season its next episode, as the server names it at the press (`ItemViewModel.play`,
    /// `CachedPlayItem.resolve`).
    fn play(&mut self, from_start: bool, cx: &mut AppCx) {
        let Some(d) = self.shown() else { return };
        let (id, kind, no_files) = (d.id, d.kind, d.files.is_empty());
        match kind {
            ItemKind::Show | ItemKind::Season => self.play_next_of(id, from_start, cx),
            _ if no_files => cx.toast(NOTHING_TO_PLAY),
            _ => {
                if let Some(r) = self.own_request(from_start) {
                    cx.play(r);
                }
            }
        }
    }

    /// Plays show or season `id`'s next episode: at once when this page holds the server's answer
    /// of this launch, else once the server names it.
    fn play_next_of(&mut self, id: i64, from_start: bool, cx: &mut AppCx) {
        let own = self.shown().is_some_and(|d| d.id == id);
        let deck = self.data.as_ref().and_then(|d| d.on_deck.as_ref()).filter(|_| own);
        if let Some(r) = deck.filter(|r| r.fresh() && !r.asking() && r.error().is_none())
            && let Some(next) = r.value()
        {
            match next {
                Some(e) => cx.play(common::play_request(e.id, from_start)),
                None => cx.toast(NOTHING_TO_PLAY),
            }
            return;
        }
        let held = deck.and_then(|r| r.value()).and_then(|v| v.as_ref()).map(|e| e.id);
        let promise = cx.spawn(move |api| api.on_deck(id));
        self.play_next = Some(PlayNext { promise, from_start, held });
    }

    /// The request Play makes for the page's own video: the version and tracks chosen here.
    fn own_request(&self, from_start: bool) -> Option<PlayRequest> {
        let d = self.shown()?;
        let p = playing(d, &self.picks);
        let subtitle_here = match (p.file, self.picks.subtitle) {
            (Some(f), Some((at, pick))) if at == f.id => Some(pick.unwrap_or(-1)),
            _ => p.subtitle_index,
        };
        Some(PlayRequest {
            file_id: p.file.map(|f| f.id).filter(|_| d.files.len() > 1),
            audio_index: p.picked_audio,
            subtitle_index: subtitle_here,
            ..common::play_request(d.id, from_start)
        })
    }

    /// Collects the changes the server answered: puts back what it refused.
    fn collect_changes(&mut self, cx: &mut AppCx) {
        let mut i = 0;
        while i < self.changes.len() {
            let p = &mut self.changes[i];
            let answer = match p.promise.take() {
                Some(a) => a,
                None if p.promise.failed() => Err(ApiError::Unreadable("the request stopped".into())),
                None => {
                    i += 1;
                    continue;
                }
            };
            let done = self.changes.remove(i);
            match answer {
                Ok(()) if done.reload => {
                    self.reload();
                    cx.state.load_home();
                }
                Ok(()) => {}
                Err(e) => {
                    self.undo(done.undo);
                    self.dirty = true;
                    cx.toast_error(e.problem());
                }
            }
        }
    }

    /// Puts back what a refused change had changed on screen.
    fn undo(&mut self, undo: Undo) {
        match undo {
            Undo::Marks { id, marks } => {
                if let Some(d) = self.shown_mut().filter(|d| d.id == id) {
                    marks.put(&mut d.summary);
                }
                for e in self.episode_items_mut().filter(|e| e.id == id) {
                    marks.put(e);
                }
                self.revisions.episodes += 1;
            }
            Undo::MyList(on) => {
                if let Some(d) = self.shown_mut() {
                    d.in_watchlist = on;
                }
            }
            Undo::Rating(rating) => {
                if let Some(d) = self.shown_mut() {
                    d.summary.user_state.rating = rating;
                }
            }
            Undo::Nothing => {}
        }
    }

    /// The episodes held for the season shown.
    fn episode_items_mut(&mut self) -> impl Iterator<Item = &mut ItemSummary> {
        self.data
            .as_mut()
            .and_then(|d| d.episodes.as_mut())
            .and_then(|(_, r)| r.value_mut())
            .into_iter()
            .flat_map(|list| list.items.iter_mut())
    }

    /// Asks again for what the page shows (not the collections, which don't change with it).
    fn reload(&mut self) {
        self.detail.invalidate();
        self.request.again();
        if let Some(data) = self.data.as_mut() {
            if let Some(r) = data.on_deck.as_mut() {
                r.invalidate();
            }
            if let Some(r) = data.seasons.as_mut() {
                r.invalidate();
            }
            if let Some((_, r)) = data.episodes.as_mut() {
                r.invalidate();
            }
            if let Some(r) = data.related.as_mut() {
                r.invalidate();
            }
        }
    }

    fn revalidate(&mut self, now: f64) {
        self.detail.revalidate(now);
        if let Some(data) = self.data.as_mut() {
            if let Some(r) = data.on_deck.as_mut() {
                r.revalidate(now);
            }
            if let Some(r) = data.seasons.as_mut() {
                r.revalidate(now);
            }
            if let Some((_, r)) = data.episodes.as_mut() {
                r.revalidate(now);
            }
            if let Some(r) = data.related.as_mut() {
                r.revalidate(now);
            }
        }
    }

    /// Marks the page's title (or one of its episodes) watched or not, at once on screen.
    fn set_watched(&mut self, id: i64, watched: bool, cx: &mut AppCx) {
        let change = Change::Watched { id, watched };
        let mut before = None;
        if let Some(d) = self.shown_mut().filter(|d| d.id == id) {
            before = Some(Marks::of(&d.summary));
            Marks::marked(&d.summary, watched).put(&mut d.summary);
        }
        for e in self.episode_items_mut().filter(|e| e.id == id) {
            before = before.or(Some(Marks::of(e)));
            Marks::marked(e, watched).put(e);
        }
        let undo = before.map_or(Undo::Nothing, |marks| Undo::Marks { id, marks });
        self.revisions.episodes += 1;
        self.dirty = true;
        self.changes.push(Pending { promise: change.start(cx), undo, reload: true, marks: Some((id, watched)) });
    }

    fn set_my_list(&mut self, on: bool, cx: &mut AppCx) {
        let Some(d) = self.shown_mut() else { return };
        let undo = Undo::MyList(d.in_watchlist);
        d.in_watchlist = on;
        let id = d.id;
        self.dirty = true;
        let promise = Change::MyList { id, on }.start(cx);
        self.changes.push(Pending { promise, undo, reload: true, marks: None });
    }

    fn rate(&mut self, rating: Option<i64>, cx: &mut AppCx) {
        let Some(d) = self.shown_mut() else { return };
        let undo = Undo::Rating(d.summary.user_state.rating);
        d.summary.user_state.rating = rating.map(|r| r as f64);
        let id = d.id;
        self.dirty = true;
        let promise = cx.spawn(move |api| api.rate(id, rating.map(|r| r as f64)));
        self.changes.push(Pending { promise, undo, reload: false, marks: None });
    }

    /// Keeps a track pick for the title (and the rest of a show), and gives the player the detail
    /// that names it.
    fn pick_tracks(&mut self, audio: Option<Option<i64>>, subtitle: Option<Option<i64>>, cx: &mut AppCx) {
        let Some(d) = self.shown() else { return };
        let p = playing(d, &self.picks);
        let Some(file) = p.file.map(|f| f.id) else { return };
        let item = d.id;
        let choice = match (audio, subtitle) {
            (Some(a), _) => StreamChoice {
                file_id: Some(file),
                audio: a.map_or(AudioChoice::Automatic, AudioChoice::Track),
                subtitle_index: p.subtitle_index,
            },
            (None, Some(s)) => StreamChoice {
                file_id: Some(file),
                audio: p.picked_audio.map_or(AudioChoice::Keep, AudioChoice::Track),
                subtitle_index: s,
            },
            (None, None) => return,
        };
        if let Some(a) = audio {
            self.picks.audio = Some((file, a));
        }
        if let Some(s) = subtitle {
            self.picks.subtitle = Some((file, s));
        }
        if let Some(d) = self.shown_mut()
            && let Some(f) = d.files.iter_mut().find(|f| f.id == file)
        {
            if let Some(a) = audio {
                f.selected_audio = a;
                f.audio_picked = Some(a.is_some());
            }
            if let Some(s) = subtitle {
                f.selected_subtitle = s;
            }
        }
        let kept = self.shown().cloned();
        cx.state.tracks_changed(kept);
        self.dirty = true;
        let promise = cx.spawn(move |api| api.set_streams(item, &choice));
        self.changes.push(Pending { promise, undo: Undo::Nothing, reload: false, marks: None });
    }

    /// Gives way to another page, once, or says the title isn't found; whether the page did.
    fn elsewhere(&mut self, ui: &mut Ui, root: Id, cx: &mut AppCx) -> bool {
        if self.redirected {
            return true;
        }
        match self.shown().and_then(|d| common::elsewhere(&d.summary)) {
            Some(Elsewhere::Page(route)) => {
                self.redirected = true;
                cx.replace(route);
                true
            }
            Some(Elsewhere::NotFound) => {
                not_found(ui, root, cx);
                true
            }
            None => false,
        }
    }

    /// Asks the server to read ahead what Play plays, once a minute at most.
    fn prefetch(&mut self, target: i64, cx: &mut AppCx) {
        if self.prefetched.is_some_and(|(id, at)| id == target && cx.now - at < 60.0) {
            return;
        }
        self.prefetched = Some((target, cx.now));
        if let Some(api) = cx.api().cloned() {
            cx.tasks().detach(move || {
                let _ = api.prefetch(target);
            });
        }
    }

    fn close_panel(&mut self) {
        self.view.panel_open = false;
    }

    fn apply(&mut self, intent: Intent, ui: &mut Ui, cx: &mut AppCx, root: Id) {
        match intent {
            Intent::Play { from_start } => self.play(from_start, cx),
            Intent::PlayRequest(r) => cx.play(r),
            Intent::ToggleWatched => {
                if let Some(m) = &self.model {
                    let (id, watched) = (m.item.id, m.watched);
                    self.set_watched(id, !watched, cx);
                }
            }
            Intent::ToggleMyList => {
                if let Some(on) = self.model.as_ref().and_then(|m| m.my_list) {
                    self.set_my_list(!on, cx);
                }
            }
            Intent::Open(panel) => {
                self.view.panel = Some(panel);
                self.view.panel_open = true;
                self.refresh_panel();
                self.view.panel_list = ListState::at(self.view.first);
            }
            Intent::Push(route) => cx.push(route),
            Intent::Season(id) => {
                if self.selected_season() != Some(id) {
                    self.chosen_season = Some(id);
                    self.view.info_episode = None;
                    self.view.focused_episode = None;
                    self.dirty = true;
                }
            }
            Intent::MediaInfo => {
                self.view.media_open = !self.view.media_open;
                self.view.details = None;
            }
            Intent::ClosePanel => self.close_panel(),
            Intent::CloseSummary => {
                self.view.summary_open = false;
                ui.request_focus(root.with("buttons").with("more"));
            }
            Intent::Row(does) => self.panel_row(does, cx),
        }
    }

    fn panel_row(&mut self, does: Does, cx: &mut AppCx) {
        match does {
            Does::Version(id) => {
                self.picks.file = Some(id);
                self.dirty = true;
            }
            Does::Audio(a) => self.pick_tracks(Some(a), None, cx),
            Does::Subtitle(s) => self.pick_tracks(None, Some(s), cx),
            Does::Summary => {
                self.close_panel();
                self.view.summary_open = true;
                self.view.summary_shown = true;
            }
            Does::Rate(r) => self.rate(r, cx),
            Does::Open(route) => {
                self.close_panel();
                cx.push(route);
            }
            Does::Act(action, item) => {
                self.close_panel();
                let own = self.shown().is_some_and(|d| d.id == item.id);
                match effect(action, &item) {
                    Effect::Open(route) => cx.push(route),
                    Effect::Play(r) if own => self.play(r.from_start, cx),
                    Effect::Play(r) => cx.play(r),
                    // A show plays its next episode, which the server names (Android's
                    // `PlayItem.resolve`).
                    Effect::PlayNext { id, from_start } => self.play_next_of(id, from_start, cx),
                    Effect::Change(Change::Watched { id, watched }) => self.set_watched(id, watched, cx),
                    Effect::Change(c) => {
                        let promise = c.start(cx);
                        self.changes.push(Pending { promise, undo: Undo::Nothing, reload: true, marks: None });
                    }
                }
            }
        }
    }

    /// The panel's lines: titles, headings and rows (`TvPanels.kt`).
    fn entries(&self, panel: &Panel) -> Vec<Entry> {
        let Some(m) = &self.model else { return Vec::new() };
        let mut out = Vec::new();
        let row = |label: String, icon: Option<Icon>, check: Option<bool>, does: Does| Entry::Row {
            label,
            icon,
            check,
            does,
        };
        match panel {
            Panel::Tracks => {
                out.push(Entry::Title("Audio and subtitles".into()));
                let Some(t) = &m.tracks else { return out };
                if !t.versions.is_empty() {
                    out.push(Entry::Heading("Version"));
                    for v in &t.versions {
                        out.push(row(v.label.clone(), None, Some(v.selected), Does::Version(v.id)));
                    }
                }
                if !t.audio.is_empty() {
                    out.push(Entry::Heading("Audio"));
                    out.push(row(t.automatic_label.clone(), None, Some(t.automatic_audio), Does::Audio(None)));
                    for a in &t.audio {
                        out.push(row(a.label.clone(), None, Some(a.selected), Does::Audio(Some(a.id))));
                    }
                }
                if !t.subtitles.is_empty() {
                    out.push(Entry::Heading("Subtitles"));
                    out.push(row("Off".into(), None, Some(t.subtitles_off), Does::Subtitle(None)));
                    for s in &t.subtitles {
                        out.push(row(s.label.clone(), None, Some(s.selected), Does::Subtitle(Some(s.id))));
                    }
                }
            }
            Panel::More => {
                out.push(Entry::Title("More".into()));
                if self.view.summary_cut {
                    out.push(row("Full summary".into(), Some(Icon::Info), None, Does::Summary));
                }
                let mut given = false;
                if m.rateable {
                    out.push(Entry::Heading("Your rating"));
                    out.push(row("No rating".into(), None, Some(m.rating.is_none()), Does::Rate(None)));
                    given = true;
                    for stars in 1..=5u32 {
                        let value = i64::from(stars) * 2;
                        let checked = m.rating == Some(value) || m.rating == Some(value - 1);
                        out.push(row(
                            words::stars_label(stars),
                            Some(Icon::Star),
                            Some(checked),
                            Does::Rate(Some(value)),
                        ));
                    }
                }
                if let Some(show) = m.show_id {
                    given = true;
                    out.push(row("Go to show".into(), Some(Icon::Tv), None, Does::Open(Route::Item { id: show })));
                }
                if let Some(season) = m.season_id {
                    given = true;
                    out.push(row(
                        "Go to season".into(),
                        Some(Icon::Layers),
                        None,
                        Does::Open(Route::Item { id: season }),
                    ));
                }
                for c in &m.collections {
                    given = true;
                    let label = format!("Part of {}", c.title);
                    out.push(row(label, Some(Icon::Layers), None, Does::Open(Route::Collection { id: c.id })));
                }
                if !given {
                    let route =
                        Route::AddToPlaylist { item_ids: vec![m.item.id], title: words::actions_title(&m.item) };
                    out.push(row("Add to playlist".into(), Some(Icon::ListPlus), None, Does::Open(route)));
                }
            }
            Panel::Actions { item } => {
                out.push(Entry::Title(item.title.clone()));
                for a in item_actions(item, None, false, true) {
                    out.push(row(a.label().into(), Some(a.icon()), None, Does::Act(a, item.clone())));
                }
            }
        }
        out
    }

    /// Makes the open panel's lines again (it opened, or the page changed under it).
    fn refresh_panel(&mut self) {
        self.view.entries = match &self.view.panel {
            Some(panel) => self.entries(panel),
            None => Vec::new(),
        };
        self.view.first = first_row(&self.view.entries);
        self.view.entries_stale = false;
    }
}

/// The row a panel opens on: the first choice in force (the version, else the audio, else the
/// subtitles; the rating), else its first row.
fn first_row(entries: &[Entry]) -> usize {
    let rows = || entries.iter().enumerate().filter(|(_, e)| matches!(e, Entry::Row { .. }));
    let summary = |e: &Entry| matches!(e, Entry::Row { does: Does::Summary, .. });
    rows()
        .find(|(_, e)| matches!(e, Entry::Row { check: Some(true), .. }))
        .or_else(|| rows().find(|(_, e)| !summary(e)))
        .or_else(|| rows().next())
        .map_or(0, |(i, _)| i)
}

/// The answers a page of `d`'s kind asks for besides its own.
fn data_for(d: &ItemDetail) -> Data {
    let id = d.id;
    let mut data = Data::default();
    if matches!(d.kind, ItemKind::Show | ItemKind::Season) {
        data.on_deck = Some(Resource::new(Some(snapshot_keys::on_deck(id)), move |api| api.on_deck(id)));
    }
    let show = match d.kind {
        ItemKind::Show => Some(id),
        ItemKind::Season => d.parent_id,
        _ => None,
    };
    if let Some(show) = show {
        data.seasons = Some(Resource::new(Some(snapshot_keys::children(show)), move |api| api.children(show)));
    }
    if d.kind.listable() {
        data.related = Some(Resource::new(Some(snapshot_keys::related(id)), move |api| api.related(id, 20)));
        data.collections = Some(Resource::new(None, move |api| api.item_collections(id)));
    }
    data
}

/// Where each part of the hero sits, from the hero's top.
#[derive(Clone, Copy, Debug)]
struct HeroLayout {
    title_y: f32,
    context_y: f32,
    meta_y: f32,
    request_y: f32,
    left_y: f32,
    genres_y: f32,
    summary_y: f32,
    credits_y: f32,
    buttons_y: f32,
    height: f32,
}

/// The hero title's lines: two at most, balanced as Android breaks its headings
/// (`LineBreak.Heading`).
fn title_opts() -> TextOpts {
    TextOpts::wrap(TEXT_W, 2).balanced()
}

fn hero_layout(ui: &Ui, m: &Model) -> HeroLayout {
    let title_h = ui.measure(&m.hero_title, &theme::HERO_TITLE, &title_opts()).h;
    let mut y = HERO_TOP + TITLE_BOX.max(title_h);
    let title_y = y - title_h;
    let context_y = y + 8.0;
    if m.context.is_some() {
        y += 8.0 + theme::TITLE.line_height;
    }
    let meta_y = y + 12.0;
    y += 12.0 + META_HEIGHT;
    let request_y = y + request::LINE_GAP;
    if m.request.is_some() {
        y += request::LINE_GAP + request::LINE_HEIGHT;
    }
    let left_y = y + 8.0;
    if m.primary.kind == PrimaryKind::Resume && !m.primary.time_left.is_empty() {
        y += 8.0 + theme::CAPTION.line_height;
    }
    let genres_y = y + 6.0;
    if !m.genres.is_empty() {
        y += 6.0 + theme::CAPTION.line_height;
    }
    let summary_y = y + 10.0;
    if let Some(s) = &m.summary {
        y += 10.0 + ui.measure(s, &theme::BODY_LARGE, &TextOpts::wrap(TEXT_W, 3)).h;
    }
    let credits_y = y + 8.0;
    if m.credits.is_some() {
        y += 8.0 + theme::CAPTION.line_height;
    }
    let buttons_y = y + 22.0;
    y += 22.0 + theme::BUTTON_HEIGHT + theme::BUTTON_LABEL_ROOM + 8.0;
    HeroLayout { title_y, context_y, meta_y, request_y, left_y, genres_y, summary_y, credits_y, buttons_y, height: y }
}

/// The metadata line of `m`.
fn meta_line(m: &Meta) -> MetaLine<'_> {
    let mut line = MetaLine::new().star_size(16.0);
    for f in &m.facts {
        line = line.text(f);
    }
    if let Some(r) = &m.content_rating {
        line = line.content_rating(r);
    }
    if let Some(r) = &m.rating {
        line = line.stars(r);
    }
    line.quality(&m.badges)
}

/// How Down leaves a control above the episodes: into the row where it was left, else onto the
/// episode to watch.
fn into_episodes(ui: &Ui, row: Id, seasons: Option<&Seasons>) -> Edge {
    let Some(s) = seasons else { return Edge::Pass };
    let Some(episodes) = s.episodes.as_ref().filter(|e| !e.is_empty()) else { return Edge::Pass };
    if ui.remembered(row).is_some() {
        return Edge::To(row);
    }
    let target = s.target.filter(|t| episodes.iter().any(|e| e.id == *t)).unwrap_or(episodes[0].id);
    Edge::To(row.with(target as u64))
}

/// The hero, laid out as `l`: the title and what is said of it, then the buttons.
fn draw_hero(
    ui: &mut Ui,
    m: &Model,
    l: &HeroLayout,
    view: &mut View,
    top: f32,
    ids: &Ids,
    out: &mut Vec<Intent>,
) -> Option<i64> {
    let x = theme::GUTTER;
    ui.text(Point::new(x, top + l.title_y), &m.hero_title, &theme::HERO_TITLE, &title_opts(), theme::FG);
    if let Some(c) = &m.context {
        ui.text(Point::new(x, top + l.context_y), c, &theme::TITLE, &TextOpts::line(TEXT_W), theme::FG);
    }
    meta_line(&m.meta).show(ui, Point::new(x, top + l.meta_y), TEXT_W);
    if let Some(words) = &m.request {
        request::stage_line(ui, Point::new(x, top + l.request_y), TEXT_W, words);
    }
    if m.primary.kind == PrimaryKind::Resume && !m.primary.time_left.is_empty() {
        let at = Point::new(x, top + l.left_y);
        ui.text(at, &m.primary.time_left, &theme::CAPTION, &TextOpts::line(TEXT_W), theme::FG);
    }
    if !m.genres.is_empty() {
        ui.text(Point::new(x, top + l.genres_y), &m.genres, &theme::CAPTION, &TextOpts::line(TEXT_W), theme::MUTED);
    }
    let cut = match &m.summary {
        Some(s) => {
            let layout = ui.layout(s, &theme::BODY_LARGE, &TextOpts::wrap(TEXT_W, 3));
            let cut = layout.truncated;
            ui.draw_text(Point::new(x, top + l.summary_y), layout, theme::SUMMARY);
            cut
        }
        None => false,
    };
    if cut != view.summary_cut {
        // More offers Full summary only when the hero cut it.
        view.summary_cut = cut;
        view.entries_stale = true;
    }
    if let Some(c) = &m.credits {
        draw_credits(ui, c, Point::new(x, top + l.credits_y));
    }

    // The buttons, in Android's order; Down goes to the episode to watch.
    let unavailable = m.primary.kind == PrimaryKind::Unavailable;
    let down = into_episodes(ui, ids.episodes, m.seasons.as_ref());
    let y = top + l.buttons_y;
    let mut prefetch = None;
    ui.group(ids.buttons, bar_group().edge(Dir::Down, down), |ui| {
        let mut x = theme::GUTTER;
        let play = ids.buttons.with("play");
        let watched = ids.buttons.with("watched");
        ui.initial_focus(if unavailable { watched } else { play });
        let label = m.primary.label();
        let pill = if unavailable {
            Button::pill(&label).enabled(false)
        } else {
            Button::pill(&label).icon(Icon::Play).long_press()
        };
        let r = pill.show(ui, play, Point::new(x, y));
        if r.activated {
            out.push(Intent::Play { from_start: false });
        }
        if r.actions() {
            out.push(Intent::Open(Panel::Actions { item: Box::new(m.item.clone()) }));
        }
        // Focus on any of the buttons asks the server to read ahead what Play plays (the TV app's
        // `prefetchPlayback`).
        let mut gained = r.gained;
        x += pill.size(ui).w + theme::BUTTON_GAP;
        let mut round = |ui: &mut Ui, key: &str, icon: Icon, label: &str| {
            let id = ids.buttons.with(key);
            let r = RoundButton::new(icon, label).show(ui, id, Point::new(x, y));
            x += pley_kit::button::ROUND_SIZE + theme::BUTTON_GAP;
            gained |= r.gained;
            r.activated
        };
        if m.primary.kind == PrimaryKind::Resume && round(ui, "from-start", Icon::RotateCcw, "From start") {
            out.push(Intent::Play { from_start: true });
        }
        if let Some((kind, tmdb_id)) = m.ask_more
            && round(ui, "request-more", Icon::Plus, "Request more seasons")
        {
            out.push(Intent::Push(Route::Title { kind, tmdb_id, pick: true }));
        }
        let (icon, label) =
            if m.watched { (Icon::EyeOff, "Mark as unwatched") } else { (Icon::CircleCheck, "Mark as watched") };
        if round(ui, "watched", icon, label) {
            out.push(Intent::ToggleWatched);
        }
        if m.tracks.is_some() && round(ui, "tracks", Icon::Captions, "Audio and subtitles") {
            out.push(Intent::Open(Panel::Tracks));
        }
        if let Some(listed) = m.my_list {
            let (icon, label) =
                if listed { (Icon::BookmarkCheck, "In My List") } else { (Icon::BookmarkPlus, "Add to My List") };
            if round(ui, "my-list", icon, label) {
                out.push(Intent::ToggleMyList);
            }
        }
        if let Some(trailer) = m.trailer
            && round(ui, "trailer", Icon::Film, "Trailer")
        {
            out.push(Intent::PlayRequest(common::play_request(trailer, true)));
        }
        if round(ui, "playlist", Icon::ListPlus, "Add to playlist") {
            let title = words::actions_title(&m.item);
            out.push(Intent::Push(Route::AddToPlaylist { item_ids: vec![m.item.id], title }));
        }
        if round(ui, "more", Icon::Ellipsis, "More") {
            out.push(Intent::Open(Panel::More));
        }
        if gained {
            prefetch = m.primary.target;
        }
    });
    prefetch
}

/// "Directed by A, B · Starring X, Y, Z": labels in Muted, names in Fg, the dot in Subtle.
fn draw_credits(ui: &mut Ui, c: &Credits, at: Point) {
    let mut parts: Vec<(&str, Color)> = Vec::new();
    if let Some((label, names)) = &c.made {
        parts.push((label, theme::MUTED));
        parts.push((names, theme::FG));
    }
    if let Some(stars) = &c.starring {
        if c.made.is_some() {
            parts.push(("·", theme::SUBTLE));
        }
        parts.push(("Starring", theme::MUTED));
        parts.push((stars, theme::FG));
    }
    // The text engine places no trailing space, so the spaces between the parts are added here.
    let opts = TextOpts::default();
    let space = ui.measure("a a", &theme::CAPTION, &opts).w - ui.measure("aa", &theme::CAPTION, &opts).w;
    let mut x = at.x;
    let end = at.x + TEXT_W;
    for (i, (text, color)) in parts.into_iter().enumerate() {
        if i > 0 {
            x += space;
        }
        let room = end - x;
        if room <= 0.0 {
            break;
        }
        x += ui.text(Point::new(x, at.y), text, &theme::CAPTION, &TextOpts::line(room), color).w;
    }
}

/// The ids of the page's parts.
struct Ids {
    buttons: Id,
    tabs: Id,
    episodes: Id,
    extras: Id,
    cast: Id,
    related: Id,
    details: Id,
}

impl Ids {
    fn of(root: Id) -> Ids {
        Ids {
            buttons: root.with("buttons"),
            tabs: root.with("tabs"),
            episodes: root.with("episodes"),
            extras: root.with("extras"),
            cast: root.with("cast"),
            related: root.with("related"),
            details: root.with("details"),
        }
    }
}

/// The section's height: tabs or header, 14, the episode tiles and their captions.
fn episodes_height(s: &Seasons) -> f32 {
    let head = if s.tabs.is_empty() { theme::ROW_HEADER_HEIGHT } else { theme::BUTTON_HEIGHT };
    head + theme::HEADER_TO_TILES + TileKind::Episode.row_height(false)
}

/// The season tabs (or the season's header) and its episodes.
fn draw_episodes(ui: &mut Ui, m: &Model, view: &mut View, top: f32, ids: &Ids, out: &mut Vec<Intent>) {
    let Some(s) = &m.seasons else { return };
    let tiles_y = if s.tabs.is_empty() {
        let at = Point::new(theme::GUTTER, top);
        ui.text(at, &s.header, &theme::SECTION, &TextOpts::line(DETAILS_W), theme::FG);
        top + theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES
    } else {
        draw_tabs(ui, s, view, top, ids, out);
        top + theme::BUTTON_HEIGHT + theme::HEADER_TO_TILES
    };
    let Some(episodes) = &s.episodes else { return };
    if episodes.is_empty() {
        let at = Point::new(theme::GUTTER, tiles_y);
        ui.text(at, "No episodes found.", &theme::BODY, &TextOpts::line(DETAILS_W), theme::MUTED);
        return;
    }
    if view.episodes_for != s.selected {
        // A season shown anew opens on the episode to watch.
        let at = s.target.and_then(|t| episodes.iter().position(|e| e.id == t)).unwrap_or(0);
        view.episodes = RowState::at(at);
        view.episodes_for = s.selected;
    }
    let kind = TileKind::Episode;
    let r = Row::new(kind, tiles_y).show(
        ui,
        ids.episodes,
        &mut view.episodes,
        episodes.len(),
        |i| episodes[i].id as u64,
        |ui, slot| episodes[slot.index].tile(kind).paint(ui, slot),
    );
    if r.focused.is_some() {
        view.focused_episode = r.focused;
    }
    if let Some(i) = r.activated {
        out.push(Intent::Push(Route::Item { id: episodes[i].id }));
    }
    if let Some(i) = r.actions
        && let Some(item) = episodes[i].item.clone()
    {
        out.push(Intent::Open(Panel::Actions { item: Box::new(item) }));
    }
}

/// The season tabs (`SeasonTabs`): 40 dp pills from x 76, 4 apart; the shown season on Surface3;
/// focus coming back to it; a tab shows its season once focus rests on it 400 ms, or on OK.
fn draw_tabs(ui: &mut Ui, s: &Seasons, view: &mut View, top: f32, ids: &Ids, out: &mut Vec<Intent>) {
    let current = s.selected.map(|id| ids.tabs.with(id as u64));
    let down = into_episodes(ui, ids.episodes, Some(s));
    let mut group = Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop).edge(Dir::Down, down);
    if let Some(c) = current {
        group = group.enter_at(c);
    }
    // Tabs past the screen's edge slide in as focus reaches them.
    let widths: Vec<f32> = s.tabs.iter().map(|(_, label)| Button::season_tab(label, false).size(ui).w).collect();
    let shift = ui.animate(ids.tabs.with("shift"), view.tabs_shift, SCROLL);
    let rested = ui.rested();
    ui.group(ids.tabs, group, |ui| {
        let mut x = TABS_X - shift;
        for (i, (id, label)) in s.tabs.iter().enumerate() {
            let tid = ids.tabs.with(*id as u64);
            let shown = s.selected == Some(*id);
            let r = Button::season_tab(label, shown).show(ui, tid, Point::new(x, top));
            if r.focused {
                let (left, right) = (x + shift, x + shift + widths[i]);
                let end = theme::CANVAS_W - theme::SAFE_X;
                if right - view.tabs_shift > end {
                    view.tabs_shift = right - end;
                } else if left - view.tabs_shift < TABS_X {
                    view.tabs_shift = (left - TABS_X).max(0.0);
                }
                if !shown && (r.activated || rested) {
                    out.push(Intent::Season(*id));
                }
            } else if r.activated && !shown {
                out.push(Intent::Season(*id));
            }
            x += widths[i] + 4.0;
        }
    });
}

/// A row of tiles under the episodes.
fn draw_row(
    ui: &mut Ui,
    id: Id,
    kind: TileKind,
    top: f32,
    title: &str,
    tiles: &[TileData],
    state: &mut RowState,
) -> pley_kit::RowResponse {
    Row::new(kind, top).header(title).show(
        ui,
        id,
        state,
        tiles.len(),
        |i| tiles[i].id as u64,
        |ui, slot| tiles[slot.index].tile(kind).paint(ui, slot),
    )
}

/// The details' layout: measured once per model and per opening of Media info, not every frame.
struct DetailsLayout {
    height: f32,
    /// The bottoms of the rows in them that take focus (the versions, then Media info), from the
    /// section's top.
    bottoms: Vec<f32>,
    /// Each fact's row height.
    facts: Vec<f32>,
    /// Each file's media card, while Media info is open.
    cards: Vec<CardLayout>,
}

/// One file's media card: its height, each row of blocks' height, and each block's values'.
struct CardLayout {
    height: f32,
    rows: Vec<f32>,
    values: Vec<Vec<f32>>,
}

/// The fact values' lines: two at most beside their 160 dp labels.
fn fact_value_opts() -> TextOpts {
    TextOpts::wrap(DETAILS_W - 32.0 - FACT_LABEL_W, 2)
}

/// A media block's values: three lines at most beside their labels.
fn block_value_opts() -> TextOpts {
    TextOpts::wrap(BLOCK_W - BLOCK_LABEL_W, 3)
}

/// The row that opens and closes Media info.
fn media_row(open: bool) -> ListRow<'static> {
    let icon = if open { Icon::ChevronUp } else { Icon::ChevronDown };
    ListRow::new("Media info").icon(icon).supporting("Each file's video, audio and subtitle streams")
}

/// Measures the details: the facts card, Versions, Media info and, while open, its cards.
fn measure_details(ui: &Ui, m: &Model, media_open: bool) -> DetailsLayout {
    let w = DETAILS_W;
    let mut y = theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES;
    let value_opts = fact_value_opts();
    let facts: Vec<f32> = m
        .facts
        .iter()
        .map(|(_, v)| ui.measure(v, &theme::BODY, &value_opts).h.max(theme::BODY.line_height) + 6.0)
        .collect();
    if !facts.is_empty() {
        y += 32.0 + facts.iter().sum::<f32>();
    }
    let mut bottoms = Vec::new();
    if !m.versions.is_empty() {
        y += 20.0 + theme::TITLE.line_height + 8.0;
        for v in &m.versions {
            y += ListRow::new(&v.label).checked(v.selected).height(ui, w);
            bottoms.push(y);
        }
    }
    let mut cards = Vec::new();
    if !m.media.is_empty() {
        y += 20.0 + media_row(media_open).height(ui, w);
        bottoms.push(y);
        if media_open {
            y += 8.0;
            for (i, f) in m.media.iter().enumerate() {
                if i > 0 {
                    y += 12.0;
                }
                let card = measure_card(ui, f);
                y += card.height;
                cards.push(card);
            }
        }
    }
    DetailsLayout { height: y, bottoms, facts, cards }
}

/// Measures one file's media card (`TvMediaInfo`): its blocks two to a row.
fn measure_card(ui: &Ui, f: &FileInfo) -> CardLayout {
    let mut y = 16.0;
    if f.title.is_some() {
        y += theme::LABEL.line_height + 10.0;
    }
    let value_opts = block_value_opts();
    let values: Vec<Vec<f32>> = f
        .blocks
        .iter()
        .map(|b| b.facts.iter().map(|fact| ui.measure(&fact.value, &theme::CAPTION, &value_opts).h).collect())
        .collect();
    let block_h = |v: &Vec<f32>| theme::LABEL.line_height + 4.0 + v.iter().map(|h| h + 4.0).sum::<f32>();
    let mut rows = Vec::new();
    for (r, pair) in values.chunks(2).enumerate() {
        if r > 0 {
            y += 14.0;
        }
        let h = pair.iter().map(block_h).fold(0.0, f32::max);
        rows.push(h);
        y += h;
    }
    CardLayout { height: y + 16.0, rows, values }
}

/// Draws the details, laid out as `l`, from `top`.
fn draw_details(
    ui: &mut Ui,
    m: &Model,
    media_open: bool,
    l: &DetailsLayout,
    top: f32,
    ids: &Ids,
    out: &mut Vec<Intent>,
) {
    let x = theme::GUTTER;
    let w = DETAILS_W;
    ui.text(Point::new(x, top), "Details", &theme::SECTION, &TextOpts::line(w), theme::FG);
    let mut y = top + theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES;
    if !m.facts.is_empty() {
        let card_h = 32.0 + l.facts.iter().sum::<f32>();
        ui.fill(Rect::new(x, y, w, card_h), theme::SURFACE2.alpha(0.6), radius::CARD);
        let (label_opts, value_opts) = (TextOpts::line(FACT_LABEL_W - 8.0), fact_value_opts());
        let mut fy = y + 16.0 + 3.0;
        for ((label, value), h) in m.facts.iter().zip(&l.facts) {
            ui.text(Point::new(x + 16.0, fy), label, &theme::BODY, &label_opts, theme::SUBTLE);
            ui.text(Point::new(x + 16.0 + FACT_LABEL_W, fy), value, &theme::BODY, &value_opts, theme::SUMMARY);
            fy += h;
        }
        y += card_h;
    }
    let group = Group::column().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop);
    ui.group(ids.details, group, |ui| {
        let mut bottoms = l.bottoms.iter().map(|b| top + b);
        if !m.versions.is_empty() {
            y += 20.0;
            ui.text(Point::new(x, y), "Versions", &theme::TITLE, &TextOpts::line(w), theme::FG);
            y += theme::TITLE.line_height + 8.0;
            for v in &m.versions {
                let row = ListRow::new(&v.label).checked(v.selected);
                if row.show(ui, ids.details.with(("version", v.id)), Point::new(x, y), w).activated {
                    out.push(Intent::Row(Does::Version(v.id)));
                }
                y = bottoms.next().unwrap_or(y);
            }
        }
        if !m.media.is_empty() {
            y += 20.0;
            if media_row(media_open).show(ui, ids.details.with("media"), Point::new(x, y), w).activated {
                out.push(Intent::MediaInfo);
            }
            y = bottoms.next().unwrap_or(y);
            if media_open {
                y += 8.0;
                for (i, (f, card)) in m.media.iter().zip(&l.cards).enumerate() {
                    if i > 0 {
                        y += 12.0;
                    }
                    draw_card(ui, f, card, Point::new(x, y), w);
                    y += card.height;
                }
            }
        }
    });
}

/// Draws one file's media card, laid out as `card`, at `at`.
fn draw_card(ui: &mut Ui, f: &FileInfo, card: &CardLayout, at: Point, w: f32) {
    ui.fill(Rect::new(at.x, at.y, w, card.height), theme::SURFACE2.alpha(0.6), radius::CARD);
    let x = at.x + 16.0;
    let mut y = at.y + 16.0;
    if let Some(t) = &f.title {
        ui.text(Point::new(x, y), t, &theme::LABEL, &TextOpts::line(w - 32.0), theme::FG);
        y += theme::LABEL.line_height + 10.0;
    }
    let (label_opts, value_opts) = (TextOpts::line(BLOCK_LABEL_W - 6.0), block_value_opts());
    for (r, (pair, h)) in f.blocks.chunks(2).zip(&card.rows).enumerate() {
        if r > 0 {
            y += 14.0;
        }
        for (c, b) in pair.iter().enumerate() {
            let bx = x + c as f32 * (BLOCK_W + BLOCK_GAP);
            ui.text(Point::new(bx, y), &b.title, &theme::LABEL, &TextOpts::line(BLOCK_W), theme::FG);
            let mut fy = y + theme::LABEL.line_height + 4.0;
            let heights = card.values.get(r * 2 + c).map_or(&[][..], |v| &v[..]);
            for (fact, lh) in b.facts.iter().zip(heights) {
                ui.text(Point::new(bx, fy + 2.0), &fact.label, &theme::CAPTION, &label_opts, theme::MUTED);
                ui.text(Point::new(bx + BLOCK_LABEL_W, fy + 2.0), &fact.value, &theme::CAPTION, &value_opts, theme::FG);
                fy += lh + 4.0;
            }
        }
        y += h;
    }
}

/// "Item not found", with Back.
fn not_found(ui: &mut Ui, root: Id, cx: &mut AppCx) {
    let body = "It may have been removed, or this profile can't see it.";
    if common::message(ui, root.with("message"), Icon::CircleAlert, "Item not found", body, "Back").activated {
        cx.back();
    }
}

/// Gives back the reading steps past `room` dp, the last first, so the next Up moves the page.
fn give_back_steps(steps: &mut Vec<f32>, room: f32) {
    let mut over = steps.iter().sum::<f32>() - room.max(0.0);
    while over > 0.5 {
        let Some(last) = steps.last_mut() else { break };
        if *last <= over + 0.5 {
            over -= *last;
            steps.pop();
        } else {
            *last -= over;
            over = 0.0;
        }
    }
}

/// The loading page (`TvItemSkeleton`): still blocks where the hero's text and buttons will be.
fn skeleton(ui: &mut Ui) {
    let x = theme::GUTTER;
    let mut y = 82.0;
    common::block(ui, x, y, 320.0, 40.0);
    y += 40.0 + 14.0;
    common::block(ui, x, y, 220.0, 18.0);
    y += 18.0 + 24.0;
    for w in [464.0, 440.0, 300.0] {
        common::block(ui, x, y, w, 15.0);
        y += 15.0 + 7.0;
    }
    y += 38.0 - 7.0;
    ui.fill(Rect::new(x, y, 140.0, theme::BUTTON_HEIGHT), theme::SURFACE2, radius::CONTROL);
    let mut bx = x + 140.0 + theme::BUTTON_GAP;
    for _ in 0..3 {
        ui.fill(Rect::new(bx, y, theme::BUTTON_HEIGHT, theme::BUTTON_HEIGHT), theme::SURFACE2, radius::CONTROL);
        bx += theme::BUTTON_HEIGHT + theme::BUTTON_GAP;
    }
}

/// The info block over the page once focus leaves the hero (`InfoBlock`).
fn info_block(ui: &mut Ui, id: Id, m: &Model, episode: Option<(&TileData, &str)>, visible: bool) {
    InfoBlock::show(ui, id, visible, |ui| {
        let r = InfoBlock::RECT;
        let mut y = r.y;
        y += ui.text(Point::new(r.x, y), &m.hero_title, &theme::HERO_TITLE, &TextOpts::line(r.w), theme::FG).h;
        y += 10.0;
        meta_line(&m.meta).show(ui, Point::new(r.x, y), r.w);
        y += META_HEIGHT;
        let summary_opts = TextOpts::wrap(r.w, 2);
        match episode.and_then(|(e, line)| Some((e.item.as_ref()?, line))) {
            Some((e, line)) => {
                y += 14.0;
                y += ui.text(Point::new(r.x, y), &e.title, &theme::TITLE, &TextOpts::line(r.w), theme::FG).h;
                y += 4.0;
                y += ui.text(Point::new(r.x, y), line, &theme::CAPTION, &TextOpts::line(r.w), theme::MUTED).h;
                if let Some(s) = e.summary.as_deref().filter(|s| !s.trim().is_empty()) {
                    ui.text(Point::new(r.x, y + 6.0), s, &theme::BODY, &summary_opts, theme::SUMMARY);
                }
            }
            None => {
                if let Some(s) = &m.summary {
                    ui.text(Point::new(r.x, y + 8.0), s, &theme::BODY, &summary_opts, theme::SUMMARY);
                }
            }
        }
    });
}

impl Screen for ItemScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.update(cx);
        if let Some(due) = self.request.due() {
            ui.repaint_after((due - cx.now).max(0.0));
        }
        self.collect_changes(cx);
        let root = ui.scope_id();
        if self.elsewhere(ui, root, cx) {
            return;
        }
        if self.dirty && self.shown().is_some() {
            self.rebuild(ui, cx);
        }
        if self.model.is_none() {
            match self.detail.state() {
                Load::Failed { error, .. } if error.is(404) => not_found(ui, root, cx),
                Load::Failed { error, .. } => {
                    let title = common::failed_title(error, "Couldn't load this item");
                    let body = error.problem();
                    if common::message(ui, root.with("message"), Icon::CircleAlert, title, &body, "Retry").activated {
                        self.detail.invalidate();
                    }
                }
                _ => skeleton(ui),
            }
            return;
        }
        match self.detail.state() {
            // What showed before stays (saved, or seen in the last five minutes), and the error is
            // told once.
            Load::Failed { error, .. } if !self.told && !error.unreachable() => {
                self.told = true;
                cx.toast_error(error.problem());
            }
            Load::Ready { .. } => self.told = false,
            _ => {}
        }
        let ids = Ids::of(root);
        let mut out: Vec<Intent> = Vec::new();
        let mut prefetch = None;
        {
            let m = self.model.as_ref().expect("made above");
            let view = &mut self.view;
            Backdrop::new(BackdropScrim::Item).show(ui, root.with("backdrop"), &mut view.backdrop, Some(&m.backdrop));

            // The sections and the lines they come to rest on.
            let hero = *view.hero.get_or_insert_with(|| hero_layout(ui, m));
            let mut sections = vec![Section::hero(HERO, hero.height)];
            if let Some(s) = &m.seasons {
                let anchor = if s.tabs.is_empty() { EPISODES_TOP - 38.0 } else { TABS_TOP };
                sections.push(Section::new(EPISODES, episodes_height(s), Some(anchor)));
            }
            if !m.extras.is_empty() {
                sections.push(Section::row(EXTRAS, TileKind::Landscape.row_height(true)));
            }
            if !m.cast.is_empty() {
                sections.push(Section::row(CAST, TileKind::Person.row_height(true)));
            }
            if !m.related.is_empty() {
                sections.push(Section::row(RELATED, TileKind::Poster.row_height(true)));
            }
            if !m.facts.is_empty() || !m.media.is_empty() {
                let media_open = view.media_open;
                let h = view.details.get_or_insert_with(|| measure_details(ui, m, media_open)).height;
                sections.push(Section::row(DETAILS, h));
            }
            let details_layout = view.details.take();
            let bottoms: &[f32] = details_layout.as_ref().map_or(&[], |d| &d.bottoms);
            // The section focus was in, found by its key: a row that comes late (More Like This)
            // moves the ones after it.
            let focus_section = view
                .focus_key
                .and_then(|k| sections.iter().position(|s| s.key == k))
                .unwrap_or(view.column.section)
                .min(sections.len() - 1);
            view.column.section = focus_section;
            let key = sections[focus_section].key;
            if view.reading_from.is_some_and(|k| k != key) {
                view.reading.clear();
                view.reading_from = None;
            }
            // A focused version row deeper than the safe area brings the details up as far as it
            // needs; reading on moves the focused section's line up from there, but never so far
            // that the page's end rises past the room Android keeps under it: steps that no longer
            // fit (Media info closed) are given back.
            let row_lift = match ui.focused_index(ids.details).and_then(|i| bottoms.get(i)) {
                Some(b) if key == DETAILS => (theme::FOCUSED_ROW_HEADER_Y + b - SAFE_BOTTOM).max(0.0),
                _ => 0.0,
            };
            let unmoved = RowColumn::new(0.0).tops(&sections);
            let end = unmoved.last().copied().unwrap_or(0.0) + sections.last().map_or(0.0, |s| s.height);
            let focused = &sections[focus_section];
            let base = unmoved[focus_section] - focused.anchor.unwrap_or(0.0);
            give_back_steps(&mut view.reading, end + BOTTOM_ROOM - theme::CANVAS_H - base - row_lift);
            let lift = row_lift + view.reading.iter().sum::<f32>();
            if lift > 0.0 {
                let s = &mut sections[focus_section];
                s.anchor = Some(s.anchor.unwrap_or(0.0) - lift);
            }
            let reading = !view.reading.is_empty();
            let clip = if reading {
                Some(theme::SAFE_Y)
            } else {
                match key {
                    HERO => None,
                    EPISODES => Some(TABS_TOP - CLEARANCE),
                    _ => Some(theme::FOCUSED_ROW_HEADER_Y - theme::ROWS_CLIP_ABOVE),
                }
            };
            let column = RowColumn::new(0.0).clip_above(clip);
            // Where the last section is on screen, for reading on (`TvReadOn.kt`).
            let shift = column.shift_for(&sections, focus_section);
            let last_top = unmoved.last().copied().unwrap_or(0.0) - shift;
            let below = last_top + sections.last().map_or(0.0, |s| s.height) - SAFE_BOTTOM;
            let takes = if reading { Dirs::UP } else { Dirs::NONE };
            let page_id = root.with("page");
            let mut column_state = std::mem::take(&mut view.column);
            ui.group(page_id, Group::free().takes(takes), |ui| {
                column.show(ui, root.with("column"), &mut column_state, &sections, |ui, i, top| {
                    match sections[i].key {
                        HERO => prefetch = draw_hero(ui, m, &hero, view, top, &ids, &mut out),
                        EPISODES => draw_episodes(ui, m, view, top, &ids, &mut out),
                        EXTRAS => {
                            let r = draw_row(
                                ui,
                                ids.extras,
                                TileKind::Landscape,
                                top,
                                "Extras",
                                &m.extras,
                                &mut view.extras,
                            );
                            // An extra resumes where it was left, as Android plays it.
                            if let Some(t) = r.activated {
                                out.push(Intent::PlayRequest(common::play_request(m.extras[t].id, false)));
                            }
                        }
                        CAST => {
                            let r =
                                draw_row(ui, ids.cast, TileKind::Person, top, m.cast_title, &m.cast, &mut view.cast);
                            if let Some(t) = r.activated {
                                out.push(Intent::Push(Route::Person { id: m.cast[t].id }));
                            }
                        }
                        RELATED => {
                            let r = draw_row(
                                ui,
                                ids.related,
                                TileKind::Poster,
                                top,
                                "More Like This",
                                &m.related,
                                &mut view.related,
                            );
                            if let Some(t) = r.activated {
                                out.push(Intent::Push(Route::Item { id: m.related[t].id }));
                            }
                            if let Some(t) = r.actions
                                && let Some(item) = m.related[t].item.clone()
                            {
                                out.push(Intent::Open(Panel::Actions { item: Box::new(item) }));
                            }
                        }
                        _ => {
                            if let Some(l) = &details_layout {
                                draw_details(ui, m, view.media_open, l, top, &ids, &mut out);
                            }
                        }
                    }
                });
            });
            view.column = column_state;
            view.details = details_layout;
            view.focus_key = sections.get(view.column.section).map(|s| s.key);
            // Down past the last thing that takes focus reads on: first the last section's top up
            // to the rows' line, so what had focus goes whole, then a step at a time until the end
            // shows; Up takes those steps back first, and focus moving along a row ends reading,
            // the row coming back to its line. Only the page's own keys count: a panel or the
            // summary keeps its keys (`TvReadOn.kt`, `TvPanels.kt`).
            let page_keys = !view.panel_open && !view.summary_open && ui.focused_in(page_id).is_some();
            let moved = ui.focused() != view.last_focus;
            view.last_focus = ui.focused();
            if page_keys && ui.failed_move() == Some(Dir::Down) && below >= 1.0 {
                let to_line = last_top - theme::FOCUSED_ROW_HEADER_Y;
                let step = if to_line >= 1.0 { to_line } else { theme::READ_ON_STEP.min(below) };
                view.reading.push(step);
                view.reading_from = view.focus_key;
            } else if page_keys && reading && ui.key_down(Key::Up) {
                view.reading.pop();
            } else if page_keys && reading && moved {
                view.reading.clear();
            }

            // The info block follows the episode focus rests on.
            if view.focused_episode != view.info_episode && ui.rested() {
                view.info_episode = view.focused_episode;
            }
            let area = sections.get(view.column.section).map_or(HERO, |s| s.key);
            let episode = match (&m.seasons, area) {
                (Some(s), EPISODES) => view.info_episode.and_then(|i| {
                    let tile = s.episodes.as_ref()?.get(i)?;
                    Some((tile, s.info_lines.get(i)?.as_str()))
                }),
                _ => None,
            };
            info_block(ui, root.with("info"), m, episode, area != HERO && view.reading.is_empty());
        }
        if let Some(target) = prefetch {
            self.prefetch(target, cx);
        }
        self.panels(ui, root, &mut out);
        for intent in out {
            self.apply(intent, ui, cx, root);
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if self.view.summary_open {
            self.view.summary_open = false;
            return true;
        }
        if self.view.panel_open {
            self.close_panel();
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => self.reload(),
            AppEvent::Shown => {
                // A next episode still being asked for when another page opened over this one
                // doesn't play on coming back.
                self.play_next = None;
                self.revalidate(cx.now);
                self.request.again();
            }
            AppEvent::RequestsChanged { title } => self.request.changed(title.as_deref()),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn modal(&self) -> bool {
        self.view.panel_open || self.view.summary_open
    }
}

impl ItemScreen {
    /// The side panel and the summary dialog over the page.
    fn panels(&mut self, ui: &mut Ui, root: Id, out: &mut Vec<Intent>) {
        if self.view.panel.is_some() {
            if self.view.entries_stale {
                self.refresh_panel();
            }
            let list_id = root.with("panel-list");
            let view = &mut self.view;
            let (entries, first) = (&view.entries, view.first);
            let r =
                SidePanel::new().back_closes(false).show(ui, root.with("panel"), view.panel_open, |ui, area| {
                    let height = |ui: &Ui, i: usize| match &entries[i] {
                        Entry::Title(t) => Heading::PanelTitle.height(ui, t, area.w),
                        Entry::Heading(t) => Heading::Section.height(ui, t, area.w),
                        Entry::Row { label, icon, .. } => {
                            let mut row = ListRow::new(label);
                            if let Some(icon) = icon {
                                row = row.icon(*icon);
                            }
                            row.height(ui, area.w)
                        }
                    };
                    List::new(area).show(ui, list_id, &mut view.panel_list, entries.len(), height, |ui, slot| {
                        match &entries[slot.index] {
                            Entry::Title(t) => {
                                Heading::PanelTitle.show(ui, t, slot.rect.origin(), slot.rect.w);
                            }
                            Entry::Heading(t) => {
                                Heading::Section.show(ui, t, slot.rect.origin(), slot.rect.w);
                            }
                            Entry::Row { label, icon, check, does } => {
                                if slot.index == first {
                                    ui.initial_focus(slot.id);
                                }
                                let mut row = ListRow::new(label);
                                if let Some(icon) = icon {
                                    row = row.icon(*icon);
                                }
                                if let Some(c) = check {
                                    row = row.checked(*c);
                                }
                                if row.show(ui, slot.id, slot.rect.origin(), slot.rect.w).activated {
                                    out.push(Intent::Row(does.clone()));
                                }
                            }
                        }
                    });
                });
            if r.closed {
                out.push(Intent::ClosePanel);
            }
            if !view.panel_open && r.progress <= 0.0 {
                view.panel = None;
            }
        }
        if self.view.summary_shown {
            let Some(m) = &self.model else { return };
            let r = Dialog::reading(&m.title, &m.full_summary).show(ui, root.with("summary"), self.view.summary_open);
            if self.view.summary_open && (r.dismissed || r.chosen.is_some()) {
                out.push(Intent::CloseSummary);
            }
            if !self.view.summary_open && r.progress <= 0.0 {
                self.view.summary_shown = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_that_no_longer_fit_are_given_back_last_first() {
        let mut steps = vec![300.0, 160.0, 160.0];
        give_back_steps(&mut steps, 1000.0);
        assert_eq!(steps, [300.0, 160.0, 160.0]);
        // Media info closed: only 380 dp of the page are left to read.
        give_back_steps(&mut steps, 380.0);
        assert_eq!(steps, [300.0, 80.0]);
        give_back_steps(&mut steps, -20.0);
        assert!(steps.is_empty());
    }
}
