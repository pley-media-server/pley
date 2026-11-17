//! Collections (`TvCollectionsScreen.kt`) and a collection's page (`TvCollectionScreen.kt`; the TV
//! app's `collections.tsx`), which also opens a Home row of several libraries' same-titled
//! collections together (the merged page).
//!
//! The rail's Collections page shows "Collections" at (96, 27) and a row of poster tiles per movie
//! or show library that has collections, their headers from y 80. A collection's page shows its
//! backdrop (else its poster) and a 436 dp hero (text column 96–560 from y 58: what it is, its
//! title, its facts, its summary, then Play and Shuffle at its foot), then its titles as a grid of
//! seven columns whose focused row's wells settle at y 58; the backdrop darkens toward Bg 92 % as
//! the grid comes up. Its category is a badge beside "Collection". The merged page is the same
//! page: its facts count its movies and shows, and a button after Shuffle for each library opens
//! that library's own collection.

use std::rc::Rc;

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::http::sentence;
use pley_core::model::{
    CollectionDetail, CollectionList, CollectionSort, CollectionSummary, ItemKind, ItemSummary, Library, LibraryKind,
    MergedCollection, MergedPart,
};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_i18n::t;
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, Badge, BadgeStyle, BarItem, Button, ColumnState, GridState,
    PosterGrid, RoundButton, Row, RowColumn, RowState, Section, Tile, TileKind, backdrop_px, button_bar,
};
use pley_shell::{AppCx, AppEvent, Chrome, Load, PlayQueue, PlayRequest, Resource, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Motion, Ui};

use crate::parts::{Rest, column_message, failed_title, page_title, skeleton_blocks};
use crate::tiles::{Cell, Quality, page_of, requests};
use crate::view::{collection_count, describe_smart_filter};

/// The rows' header line on the Collections page.
const ROWS_Y: f32 = theme::HEADER_Y_LIBRARY;
/// A hero's height, from the page's top to the first row of the grid or list (less 16).
pub const HERO_H: f32 = 436.0;
/// Where the hero's text starts, and where the grid's focused row settles.
pub const CONTENT_Y: f32 = 58.0;
/// The text column's width (96 to 560).
pub const TEXT_W: f32 = 464.0;
/// The first row's wells under the hero.
const FIRST_ROW_Y: f32 = HERO_H + 16.0;
/// Play and Shuffle's line, a line higher when "Nothing in this collection yet." is under them.
const BUTTONS_Y: f32 = HERO_H - 36.0 - theme::BUTTON_HEIGHT;
const BUTTONS_Y_EMPTY: f32 = BUTTONS_Y - 20.0;
/// How long a problem shows beside the buttons.
const SAY_FOR: f64 = 4.0;
/// The darkening over the backdrop once the page has moved.
const DIM: f32 = 0.92;
/// The least room beside the buttons a problem is said in; with less, it goes above them.
const SAID_MIN_W: f32 = 240.0;
/// A library's button on the merged page cuts its name at this width.
const LIBRARY_LABEL_MAX: f32 = 150.0;

/// A collection as a poster tile shows it: its title, its size and its picture.
pub struct CollectionTile {
    pub id: i64,
    pub title: String,
    pub subtitle: String,
    pub art: Vec<ImageRequest>,
}

impl CollectionTile {
    fn new(cx: &AppCx, c: &CollectionSummary, kind: Option<LibraryKind>, px: u32) -> CollectionTile {
        let art = match (cx.api(), &c.poster_url) {
            (Some(api), Some(path)) => requests(api, std::slice::from_ref(path), px),
            _ => Vec::new(),
        };
        CollectionTile { id: c.id, title: c.title.clone(), subtitle: collection_count(c.item_count, kind), art }
    }
}

/// A movie or show library's collections: a row of the Collections page, loaded on its own and
/// saved under the same key as the library's Collections tab.
struct Shelf {
    id: i64,
    name: String,
    kind: LibraryKind,
    list: Resource<CollectionList>,
    tiles: Vec<CollectionTile>,
    state: RowState,
}

impl Shelf {
    fn new(library: &Library) -> Shelf {
        let id = library.id;
        Shelf {
            id,
            name: library.name.clone(),
            kind: library.kind,
            list: Resource::new(Some(snapshot_keys::collections(id)), move |api| api.library_collections(id)),
            tiles: Vec::new(),
            state: RowState::default(),
        }
    }
}

/// The rail's Collections page.
pub struct CollectionsScreen {
    /// The library whose row takes focus first (the rail's entry names one).
    first: Option<i64>,
    /// Every movie and show library's, in the server's order of libraries.
    shelves: Vec<Shelf>,
    /// The shelves with collections, as the rows' sections.
    shown: Vec<usize>,
    sections: Vec<Section>,
    column: ColumnState,
    sent_to_rail: bool,
    has_focus: bool,
}

impl CollectionsScreen {
    /// The page, focus starting in `first`'s row when it has collections.
    pub fn new(first: Option<i64>) -> CollectionsScreen {
        CollectionsScreen {
            first,
            shelves: Vec::new(),
            shown: Vec::new(),
            sections: Vec::new(),
            column: ColumnState::default(),
            sent_to_rail: false,
            has_focus: false,
        }
    }

    /// Follows the libraries: a shelf for each movie and show library, those kept as they were.
    /// Whether they changed.
    fn follow(&mut self, libraries: &[Library]) -> bool {
        let wanted = libraries.iter().filter(|l| matches!(l.kind, LibraryKind::Movies | LibraryKind::Shows));
        let same = wanted
            .clone()
            .map(|l| (l.id, l.name.as_str(), l.kind))
            .eq(self.shelves.iter().map(|s| (s.id, s.name.as_str(), s.kind)));
        if same {
            return false;
        }
        let mut old: std::collections::HashMap<i64, Shelf> = self.shelves.drain(..).map(|s| (s.id, s)).collect();
        self.shelves = wanted
            .map(|l| match old.remove(&l.id) {
                Some(shelf) => Shelf { name: l.name.clone(), kind: l.kind, ..shelf },
                None => Shelf::new(l),
            })
            .collect();
        true
    }
}

impl Screen for CollectionsScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        page_title(ui, t!("library.collections.title"), theme::CANVAS_W - theme::GUTTER - theme::SAFE_X);
        self.has_focus = false;
        let Some(libraries) = cx.state.libraries() else {
            // The libraries couldn't be had, with nothing saved: say so, with Retry.
            if let Some(error) = cx.state.account_error() {
                let problem = error.problem();
                let title = failed_title(error, t!("library.collections.failed"));
                let r =
                    column_message(ui, ui.id("state"), ROWS_Y, title, &problem, Some(t!("library.collections.retry")));
                self.has_focus = true;
                if r.is_some_and(|r| r.activated) {
                    cx.state.load_account();
                }
            }
            return;
        };
        let mut changed = self.follow(libraries);
        let px = ui.image_px(TileKind::Poster.size().w);
        for shelf in &mut self.shelves {
            if shelf.list.update(cx) {
                let list = shelf.list.value().map(|c| c.collections.as_slice()).unwrap_or_default();
                shelf.tiles = list.iter().map(|c| CollectionTile::new(cx, c, Some(shelf.kind), px)).collect();
                changed = true;
            }
        }
        if changed {
            self.shown = (0..self.shelves.len()).filter(|&i| !self.shelves[i].tiles.is_empty()).collect();
            let height = TileKind::Poster.row_height(true);
            self.sections =
                self.shown.iter().map(|&i| Section::new(self.shelves[i].id as u64, height, Some(ROWS_Y))).collect();
        }
        if self.shown.is_empty() {
            // Nothing to show yet while any library's collections are on their way.
            if self.shelves.iter().any(|s| matches!(s.list.state(), Load::Loading)) {
                return;
            }
            // Every library failed with nothing saved: the failure, with Retry.
            let failure = self
                .shelves
                .iter()
                .map(|s| s.list.error().filter(|_| s.list.value().is_none()))
                .collect::<Option<Vec<_>>>();
            if let Some(error) = failure.and_then(|errors| errors.first().copied()) {
                let problem = error.problem();
                let title = failed_title(error, t!("library.collections.failed"));
                let r =
                    column_message(ui, ui.id("state"), ROWS_Y, title, &problem, Some(t!("library.collections.retry")));
                self.has_focus = true;
                if r.is_some_and(|r| r.activated) {
                    self.shelves.iter_mut().for_each(|s| s.list.invalidate());
                }
                return;
            }
            let (title, body) = (t!("library.collections.noneTitle"), t!("library.collections.noneBody"));
            column_message(ui, ui.id("state"), ROWS_Y, title, body, None);
            if !std::mem::replace(&mut self.sent_to_rail, true) {
                cx.to_rail();
            }
            return;
        }
        self.has_focus = true;
        let column_id = ui.id("rows");
        let first = self.shown.iter().map(|&i| &self.shelves[i]).find(|s| Some(s.id) == self.first);
        let first = first.unwrap_or(&self.shelves[self.shown[0]]);
        ui.initial_focus(column_id.with(first.id as u64).with("row").with(first.tiles[0].id as u64));
        let (shown, shelves) = (&self.shown, &mut self.shelves);
        let mut open = None;
        RowColumn::new(ROWS_Y).clip_above(Some(ROWS_Y - theme::ROWS_CLIP_ABOVE)).show(
            ui,
            column_id,
            &mut self.column,
            &self.sections,
            |ui, i, top| {
                let shelf = &mut shelves[shown[i]];
                let tiles = &shelf.tiles;
                let row_id = ui.id("row");
                let r = Row::new(TileKind::Poster, top).header(&shelf.name).long_press(false).show(
                    ui,
                    row_id,
                    &mut shelf.state,
                    tiles.len(),
                    |k| tiles[k].id as u64,
                    |ui, slot| {
                        let t = &tiles[slot.index];
                        Tile::new(TileKind::Poster, &t.title).subtitle(&t.subtitle).art(&t.art).paint(ui, slot);
                    },
                );
                if let Some(k) = r.activated {
                    open = Some(Route::Collection { id: tiles[k].id });
                }
            },
        );
        if let Some(route) = open {
            cx.push(route);
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        for shelf in &mut self.shelves {
            match event {
                AppEvent::Changed => shelf.list.invalidate(),
                AppEvent::Shown => shelf.list.revalidate(cx.now),
                _ => {}
            }
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.has_focus
    }
}

/// What a collection's page shows: one collection, or several libraries' same-titled ones.
enum Source {
    One(Resource<CollectionDetail>),
    Merged(Resource<MergedCollection>),
}

impl Source {
    /// Collects what came; whether the page has something new to show.
    fn update(&mut self, cx: &mut AppCx) -> bool {
        match self {
            Source::One(r) => r.update(cx),
            Source::Merged(r) => r.update(cx),
        }
    }

    /// What there is to show, made into one shape.
    fn shown(&self) -> Option<Shown> {
        match self {
            Source::One(r) => r.value().map(|d| Shown { detail: d.clone(), parts: Vec::new() }),
            Source::Merged(r) => r.value().map(Shown::merged),
        }
    }

    /// Nothing yet: `Loading`; failed with nothing to show: the error.
    fn state(&self) -> Load<'_, ()> {
        let (loading, error, value) = match self {
            Source::One(r) => (matches!(r.state(), Load::Loading), r.error(), r.value().is_some()),
            Source::Merged(r) => (matches!(r.state(), Load::Loading), r.error(), r.value().is_some()),
        };
        match (loading, error) {
            (true, _) => Load::Loading,
            (false, Some(error)) if !value => Load::Failed { error, stale: None },
            _ => Load::Ready { value: &(), fresh: true },
        }
    }

    fn has_value(&self) -> bool {
        match self {
            Source::One(r) => r.value().is_some(),
            Source::Merged(r) => r.value().is_some(),
        }
    }

    fn has_error(&self) -> bool {
        match self {
            Source::One(r) => r.error().is_some(),
            Source::Merged(r) => r.error().is_some(),
        }
    }

    fn invalidate(&mut self) {
        match self {
            Source::One(r) => r.invalidate(),
            Source::Merged(r) => r.invalidate(),
        }
    }

    fn revalidate(&mut self, now: f64) {
        match self {
            Source::One(r) => r.revalidate(now),
            Source::Merged(r) => r.revalidate(now),
        }
    }
}

/// The page's collection: a merged one as a collection of its titles, with its libraries' own.
struct Shown {
    detail: CollectionDetail,
    /// The merged page's collections, one per library; empty for one collection.
    parts: Vec<MergedPart>,
}

impl Shown {
    fn merged(m: &MergedCollection) -> Shown {
        // Its backdrop is its first title's that has one.
        let backdrop = m.items.iter().find_map(|i| i.backdrop_url.clone());
        let collection = CollectionSummary {
            id: m.collections.first().map_or(0, |p| p.id),
            title: m.title.clone(),
            item_count: m.items.len() as i64,
            backdrop_url: backdrop,
            ..CollectionSummary::default()
        };
        Shown { detail: CollectionDetail { collection, items: m.items.clone() }, parts: m.collections.clone() }
    }

    fn is_merged(&self) -> bool {
        !self.parts.is_empty()
    }
}

/// A collection's page, or the merged page of several.
pub struct CollectionScreen {
    /// The collection, or the first of the merged ones (it names the backdrop).
    id: i64,
    source: Source,
    cells: Vec<Cell>,
    /// The collection shown, shared with the frame that draws it.
    shown: Option<Rc<Shown>>,
    backdrop: BackdropState,
    art: Option<BackdropArt>,
    grid: GridState,
    /// What a problem with Play says beside the buttons, until when.
    said: Option<(String, f64)>,
    on_deck: Option<Promise<Result<Option<ItemSummary>, ApiError>>>,
    rest: Rest,
    /// Whether focus is in the grid (the hero has moved up out of the way).
    in_grid: bool,
    /// The facts line, and whether its library was known when it was worked out.
    facts: String,
    facts_library: bool,
    facts_due: bool,
}

impl CollectionScreen {
    /// The page of collection `id`.
    pub fn new(id: i64) -> CollectionScreen {
        let source = Source::One(Resource::new(Some(snapshot_keys::collection(id)), move |api| api.collection(id)));
        CollectionScreen::with(id, source)
    }

    /// The page of several libraries' same-titled collections, `ids`, as one (a Home row's).
    pub fn merged(ids: Vec<i64>) -> CollectionScreen {
        let key = snapshot_keys::merged_collection(&ids);
        let first = ids.first().copied().unwrap_or_default();
        let source = Source::Merged(Resource::new(Some(key), move |api| api.merged_collection(&ids)));
        CollectionScreen::with(first, source)
    }

    fn with(id: i64, source: Source) -> CollectionScreen {
        CollectionScreen {
            id,
            source,
            cells: Vec::new(),
            shown: None,
            backdrop: BackdropState::default(),
            art: None,
            grid: GridState::default(),
            said: None,
            on_deck: None,
            rest: Rest::default(),
            in_grid: false,
            facts: String::new(),
            facts_library: false,
            facts_due: true,
        }
    }

    /// The merged page's facts: "6 movies  ·  2 shows".
    fn merged_facts(&mut self, detail: &CollectionDetail) {
        let movies = detail.items.iter().filter(|i| i.kind == ItemKind::Movie).count();
        let shows = detail.items.iter().filter(|i| i.kind == ItemKind::Show).count();
        let mut facts = Vec::new();
        if movies > 0 {
            facts.push(t!("library.count.movies", count = movies));
        }
        if shows > 0 {
            facts.push(t!("library.count.shows", count = shows));
        }
        if facts.is_empty() {
            facts.push(t!("library.count.titles", count = detail.items.len()));
        }
        self.facts = facts.join("  ·  ");
        self.facts_library = true;
        self.facts_due = false;
    }

    /// "4 movies  ·  Movies  ·  By release date" (or the smart filter's words): its size, its
    /// library and how it is ordered.
    fn work_out_facts(&mut self, detail: &CollectionDetail, library: Option<&Library>) {
        let notes = match &detail.smart_filter {
            Some(f) => describe_smart_filter(f).join("  ·  "),
            None => match detail.sort {
                CollectionSort::Release => t!("library.collections.byRelease").into(),
                CollectionSort::Title => t!("library.collections.byTitle").into(),
                CollectionSort::Custom => t!("library.collections.customOrder").into(),
                CollectionSort::Unknown => String::new(),
            },
        };
        let count = match library {
            Some(l) => collection_count(detail.item_count, Some(l.kind)),
            None => t!("library.count.titles", count = detail.item_count),
        };
        let facts: Vec<&str> = [count.as_str(), library.map_or("", |l| l.name.as_str()), notes.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        self.facts = facts.join("  ·  ");
        self.facts_library = library.is_some();
        self.facts_due = false;
    }

    fn say(&mut self, text: String, now: f64) {
        self.said = Some((text, now + SAY_FOR));
    }

    /// Plays the collection: movies from the first (or a random one) with the collection as the
    /// queue; a collection of shows plays the first (or a random) show's next episode.
    /// On the merged page, Next and Previous follow the title's own library's collection.
    fn play(&mut self, cx: &mut AppCx, shuffle: bool) {
        let Some(shown) = self.shown.clone() else { return };
        let items = &shown.detail.items;
        let now = cx.now;
        let Some(pick) = play_pick(items, shown.is_merged(), shuffle.then_some(|len| random_index(now, len))) else {
            return;
        };
        let item = &items[pick];
        let queue = if shown.is_merged() {
            shown.parts.iter().find(|p| p.library_id == item.library_id).map(|p| PlayQueue::Collection(p.id))
        } else {
            Some(PlayQueue::Collection(self.id))
        };
        if item.kind != ItemKind::Show {
            cx.play(PlayRequest { queue, ..PlayRequest::item(item.id) });
            return;
        }
        let show = item.id;
        self.on_deck = Some(cx.spawn(move |api| api.on_deck(show)));
    }

    fn hero(&mut self, ui: &mut Ui, cx: &mut AppCx, shown: &Shown, lift: f32) {
        let detail = &shown.detail;
        let x = theme::GUTTER;
        let mut y = CONTENT_Y - lift;
        // What it is: the layers icon, "Collection", and its TMDB and Smart badges.
        let row_h = 18.0;
        ui.icon(Rect::new(x, y, 18.0, 18.0), Icon::Layers, theme::MUTED);
        let mut bx = x + 18.0 + 8.0;
        let label_y = y + (row_h - theme::LABEL.line_height) / 2.0;
        let label = t!("library.collections.label");
        bx += ui.text(Point::new(bx, label_y), label, &theme::LABEL, &TextOpts::default(), theme::MUTED).w + 8.0;
        let badge_y = y + (row_h - pley_kit::badge::BADGE_HEIGHT) / 2.0;
        if detail.automatic {
            bx += Badge::new("TMDB", BadgeStyle::Outline).show(ui, Point::new(bx, badge_y)).w + 8.0;
        }
        if detail.smart_filter.is_some() {
            bx += Badge::new(t!("library.collections.smart"), BadgeStyle::Outline).show(ui, Point::new(bx, badge_y)).w
                + 8.0;
        }
        if let Some(category) = &detail.category {
            Badge::new(&category.title, BadgeStyle::Outline).show(ui, Point::new(bx, badge_y));
        }
        y += row_h + 10.0;
        y += ui.text(Point::new(x, y), &detail.title, &theme::HERO_TITLE, &TextOpts::wrap(TEXT_W, 2), theme::FG).h;
        y += 12.0;
        y += ui.text(Point::new(x, y), &self.facts, &theme::LABEL, &TextOpts::line(TEXT_W), theme::MUTED).h;
        if let Some(summary) = detail.summary.as_deref().filter(|s| !s.is_empty()) {
            y += 12.0;
            ui.text(Point::new(x, y), summary, &theme::BODY_LARGE, &TextOpts::wrap(TEXT_W, 3), theme::SUMMARY);
        }
        let empty = detail.items.is_empty();
        let by = (if empty { BUTTONS_Y_EMPTY } else { BUTTONS_Y }) - lift;
        let play = Button::pill(t!("library.collections.play")).icon(Icon::Play).enabled(!empty);
        let shuffle =
            RoundButton::new(Icon::Shuffle, t!("library.collections.shuffle")).enabled(detail.items.len() >= 2);
        let bar_id = ui.id("buttons");
        let initial = if empty { bar_id.with("shuffle") } else { bar_id.with("play") };
        ui.initial_focus(initial);
        // The merged page's libraries, each opening its own collection (those this TV doesn't
        // know by name left out).
        let libraries = cx.state.libraries().unwrap_or_default();
        let named: Vec<(String, i64, &str)> = shown
            .parts
            .iter()
            .filter_map(|p| {
                libraries.iter().find(|l| l.id == p.library_id).map(|l| (p.id.to_string(), p.id, l.name.as_str()))
            })
            .collect();
        let mut items = vec![("play", BarItem::Button(play)), ("shuffle", BarItem::Round(shuffle))];
        for (key, _, name) in &named {
            let b = Button::pill(name).icon(Icon::Layers).max_label_width(LIBRARY_LABEL_MAX);
            items.push((key.as_str(), BarItem::Button(b)));
        }
        let answers = button_bar(ui, bar_id, Point::new(x, by), &items);
        let bar_w: f32 = items
            .iter()
            .map(|(_, item)| match item {
                BarItem::Button(b) => b.size(ui).w,
                BarItem::Round(r) => r.size().w,
            })
            .sum::<f32>()
            + theme::BUTTON_GAP * (items.len() - 1) as f32;
        let open = answers.iter().skip(2).zip(&named).find(|(r, _)| r.activated).map(|(_, (_, id, _))| *id);
        if answers[0].activated {
            self.play(cx, false);
        }
        if answers[1].activated {
            self.play(cx, true);
        }
        if let Some(id) = open {
            cx.push(Route::Collection { id });
        }
        if let Some((text, until)) = &self.said {
            if cx.now < *until {
                // After the whole bar (the merged page's libraries included), or above it where the
                // bar leaves too little room on its line.
                let after = x + bar_w + 12.0;
                let right = theme::CANVAS_W - theme::SAFE_X;
                let at = if right - after >= SAID_MIN_W {
                    Point::new(after, by + 14.0 - 2.0)
                } else {
                    Point::new(x, by - theme::LABEL.line_height - 12.0)
                };
                let opts = TextOpts::line(right - at.x);
                ui.text(at, text, &theme::LABEL, &opts, theme::DANGER_TEXT);
                ui.repaint_after(until - cx.now);
            } else {
                self.said = None;
            }
        }
        if empty {
            let ey = by + theme::BUTTON_HEIGHT + 36.0;
            let opts = TextOpts::wrap(TEXT_W, 2);
            ui.text(Point::new(x, ey), t!("library.collections.empty"), &theme::BODY, &opts, theme::MUTED);
        }
    }
}

/// The title Play (or Shuffle, with `roll` picking in `0..len`) starts from. The merged page plays
/// its movies whenever it has any, as the web's does, and a show only when it has no movie.
fn play_pick(items: &[ItemSummary], merged: bool, roll: Option<impl Fn(usize) -> usize>) -> Option<usize> {
    let movies: Vec<usize> = (0..items.len()).filter(|&i| items[i].kind == ItemKind::Movie).collect();
    let from: Vec<usize> = if merged && !movies.is_empty() { movies } else { (0..items.len()).collect() };
    let at = match roll {
        Some(roll) => roll(from.len()).min(from.len().saturating_sub(1)),
        None => 0,
    };
    from.get(at).copied()
}

/// A pick in `0..len` from the clock, for Shuffle.
pub fn random_index(now: f64, len: usize) -> usize {
    let mut x = (now * 1_000_003.0) as u64 ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x % len.max(1) as u64) as usize
}

impl Screen for CollectionScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if self.source.update(cx)
            && let Some(shown) = self.source.shown()
        {
            let detail = &shown.detail;
            let px = ui.image_px(TileKind::LibraryGrid.size().w);
            self.cells = match cx.api() {
                Some(api) => detail
                    .items
                    .iter()
                    .map(|i| Cell::new(i.clone(), api, TileKind::LibraryGrid, px, Quality::NONE))
                    .collect(),
                None => Vec::new(),
            };
            let path = detail.backdrop_url.clone().or(detail.poster_url.clone());
            let art = match (cx.api(), path) {
                (Some(api), Some(p)) => vec![api.image(&p, backdrop_px(ui))],
                _ => Vec::new(),
            };
            self.art = Some(BackdropArt::new(self.id as u64, art, detail.title.clone()));
            self.shown = Some(Rc::new(shown));
            self.facts_due = true;
        }
        if let Some(p) = &mut self.on_deck
            && let Some(answer) = p.take()
        {
            self.on_deck = None;
            match answer {
                Ok(Some(episode)) => cx.play(PlayRequest::item(episode.id)),
                Ok(None) => self.say(t!("library.collections.noEpisodes").into(), cx.now),
                Err(e) if e.cancelled() => {}
                Err(e) => self.say(sentence(&e.to_string()), cx.now),
            }
        }
        match self.source.state() {
            Load::Loading => {
                skeleton_blocks(
                    ui,
                    &[
                        (58.0, 120.0, 18.0, radius::BADGE),
                        (90.0, 360.0, 44.0, radius::BADGE),
                        (148.0, 240.0, 18.0, radius::BADGE),
                        (180.0, TEXT_W, 70.0, radius::BADGE),
                        (298.0, 128.0, theme::BUTTON_HEIGHT, radius::CONTROL),
                    ],
                );
                return;
            }
            Load::Failed { error, stale: None } => {
                let problem = error.problem();
                let title = failed_title(error, t!("library.collections.failedOne"));
                let retry = Some(t!("library.collections.retry"));
                let r = column_message(ui, ui.id("state"), CONTENT_Y, title, &problem, retry);
                if r.is_some_and(|r| r.activated) {
                    self.source.invalidate();
                }
                return;
            }
            Load::Ready { .. } | Load::Failed { stale: Some(_), .. } => {}
        }
        let Some(shown) = self.shown.clone() else { return };
        let detail = &shown.detail;
        // The facts name the library, which may come after the collection.
        if shown.is_merged() && self.facts_due {
            self.merged_facts(detail);
        } else if !shown.is_merged() && (self.facts_due || (!self.facts_library && cx.state.libraries().is_some())) {
            let library = cx.state.libraries().and_then(|ls| ls.iter().find(|l| l.id == detail.library_id));
            let library = library.cloned();
            self.work_out_facts(detail, library.as_ref());
        }

        // The page moves up once focus is in the grid: the hero out of the way, the focused row at 58.
        let grid_id = ui.id("grid");
        self.in_grid = ui.focused_in(grid_id).is_some();
        let lift = ui.animate(ui.id("lift"), if self.in_grid { FIRST_ROW_Y - CONTENT_Y } else { 0.0 }, LIFT);
        let moved = lift + self.grid.shown_scroll();
        Backdrop::new(BackdropScrim::Item).show(ui, ui.id("backdrop"), &mut self.backdrop, self.art.as_ref());
        if moved > 0.5 {
            let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
            ui.fill(full, theme::BG.alpha(DIM * (moved / HERO_H).min(1.0)), 0.0);
        }
        // Drawn even once off screen: Up from the grid's first row comes back to its buttons.
        self.hero(ui, cx, &shown, moved);
        let cells = &self.cells;
        let offset = FIRST_ROW_Y - CONTENT_Y - lift;
        let grid = PosterGrid::new(TileKind::LibraryGrid, CONTENT_Y - theme::GRID_CONTENT_TOP);
        let r = ui.translate(Point::new(0.0, offset), |ui| {
            grid.show(ui, grid_id, &mut self.grid, cells.len(), |ui, slot| {
                cells[slot.index].tile(slot.kind).paint(ui, slot)
            })
        });
        if let Some(i) = r.activated {
            cx.push(page_of(&cells[i].item));
        }
        if let Some(i) = r.actions {
            cx.tile_actions(cells[i].item.id, false);
        }
        let focused = r.focused.map(|i| &cells[i].item);
        let rested = focused.is_some() && ui.rested();
        self.rest.update(cx, focused, rested);
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => self.source.invalidate(),
            AppEvent::Shown => self.source.revalidate(cx.now),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.source.has_value() || self.source.has_error()
    }
}

/// The page moving between its hero and its grid: 240 ms, as pages of rows move.
const LIFT: Motion = pley_kit::PAGE_SCROLL;

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, kind: ItemKind) -> ItemSummary {
        ItemSummary { id, kind, ..ItemSummary::default() }
    }

    #[test]
    fn the_merged_page_plays_its_movies_before_any_show() {
        let items =
            [item(1, ItemKind::Show), item(2, ItemKind::Movie), item(3, ItemKind::Show), item(4, ItemKind::Movie)];
        let none: Option<fn(usize) -> usize> = None;
        assert_eq!(play_pick(&items, true, none), Some(1), "Play: the first movie");
        assert_eq!(play_pick(&items, true, Some(|_| 1)), Some(3), "Shuffle: among the movies");
        assert_eq!(play_pick(&items, true, Some(|len| len * 9)), Some(3), "a roll past the end stays in");
        // One collection keeps its order; a merged page of shows plays a show.
        assert_eq!(play_pick(&items, false, none), Some(0));
        let shows = [item(1, ItemKind::Show), item(3, ItemKind::Show)];
        assert_eq!(play_pick(&shows, true, Some(|_| 1)), Some(1));
        assert_eq!(play_pick(&[], true, none), None);
    }

    #[test]
    fn shuffle_picks_within_the_list() {
        for i in 0..50 {
            assert!(random_index(i as f64 * 0.37, 7) < 7);
        }
        assert_eq!(random_index(1.0, 1), 0);
        assert_eq!(random_index(1.0, 0), 0);
    }
}
