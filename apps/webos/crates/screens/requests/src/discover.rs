//! Discover (`discover.tsx`'s `DiscoverScreen`): a hero for the first trending title with a line
//! of its overview and its main action, then TMDB's rows of posters with their badges, then this
//! profile's requests; the way to the requests at the top. Each row loads as focus comes within
//! two rows of it. Over the rows, the info block and the backdrop follow the tile focus rests on,
//! as on Home; Back in the rows brings the hero back.

use std::collections::HashMap;

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::model::{DiscoverDetail, DiscoverTitle, MediaRequest, RequestList, RequestQuery};
use pley_core::requests::{
    DISCOVER_ROWS, asked_line, has_requests, moving, requests_count, requests_problem, requests_title, seasons_label,
    stage_words, title_key, tmdb_credit,
};
use pley_i18n::t;
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, ColumnState, InfoBlock, Message, MessageStyle, Row, RowColumn,
    RowState, Section, Tile, TileKind, TileSlot, backdrop_px, skeleton_block,
};
use pley_shell::{AppCx, AppEvent, Chrome, Resource, Route, Screen};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{Id, Motion, Ui};

use crate::actions::{Actions, Extra, TitleIntent};
use crate::common::{
    Asking, Poll, ProblemAction, SignInAgain, TitleTileModel, ToneTile, TvClock, Unwatching, art, keep_title, marked,
    marks_version, paint_title, problem_page, stage_pill, summary_problem, title_page,
};
use crate::pages::Reader;
use crate::widgets::{Counted, count_text, title_meta};

/// A row reads pages until it has this many titles: more than a screen of posters.
const ROW_WANT: usize = 12;
/// The first rows load at once; each other as focus comes within two rows of it.
const LOAD_AHEAD: usize = 2;
pub const HERO_H: f32 = 356.0;
/// The hero's title's two lines end at 172 whatever their number, clear of the way to the requests.
const HERO_TITLE_TOP: f32 = 84.0;
const HERO_TITLE_BOX: f32 = 88.0;
const TEXT_COLUMN: f32 = 464.0;
/// Before the skeleton shows.
const SKELETON_AFTER: f64 = 0.3;
/// The requests at the foot.
const MINE_LIMIT: i64 = 20;

/// The hero's section, the rows' (1 + their index), the requests', the credit's.
const HERO: u64 = 0;
const MINE: u64 = 100;
const CREDIT: u64 = 101;

/// What the info block and the backdrop follow in the rows.
#[derive(Clone, Debug, PartialEq)]
enum Focused {
    Title(DiscoverTitle),
    Request(Box<MediaRequest>),
}

/// A row's tiles as drawn, made again when its titles or their badges change.
#[derive(Default)]
struct RowTiles {
    source: Option<(usize, u64, (u64, usize, u64))>,
    tiles: Vec<TitleTileModel>,
}

/// A request's tile at the foot.
struct MineTile {
    request: MediaRequest,
    art: Vec<pley_base::ImageRequest>,
}

/// Discover.
pub struct DiscoverScreen {
    readers: Vec<Reader>,
    rows: Vec<RowTiles>,
    row_states: HashMap<u64, RowState>,
    mine: Option<Resource<RequestList>>,
    mine_tiles: Vec<MineTile>,
    mine_source: Option<usize>,
    mine_poll: Poll,
    hero: Option<(String, Resource<DiscoverDetail>)>,
    hero_poll: Poll,
    column: ColumnState,
    backdrop: BackdropState,
    in_rows: bool,
    focused: Option<Focused>,
    rested: Option<Focused>,
    /// The section focus is in, by key (the rows load from it).
    section: u64,
    to_hero: bool,
    asking: Option<Asking>,
    unwatching: Option<Unwatching>,
    sign_in: SignInAgain,
    clock: TvClock,
    opened: Option<f64>,
    /// Something to focus: the hero, a message's button.
    settled: bool,
    /// Focus was sent to the rail for a page with nothing to focus.
    railed: bool,
}

impl Default for DiscoverScreen {
    fn default() -> DiscoverScreen {
        DiscoverScreen::new()
    }
}

impl DiscoverScreen {
    pub fn new() -> DiscoverScreen {
        let readers = DISCOVER_ROWS
            .iter()
            .map(|r| {
                let list = r.list;
                Reader::new(ROW_WANT, move |api, page| api.discover(list, page))
            })
            .collect();
        DiscoverScreen {
            readers,
            rows: DISCOVER_ROWS.iter().map(|_| RowTiles::default()).collect(),
            row_states: HashMap::new(),
            mine: None,
            mine_tiles: Vec::new(),
            mine_source: None,
            mine_poll: Poll::default(),
            hero: None,
            hero_poll: Poll::default(),
            column: ColumnState::default(),
            backdrop: BackdropState::default(),
            in_rows: false,
            focused: None,
            rested: None,
            section: HERO,
            to_hero: false,
            asking: None,
            unwatching: None,
            sign_in: SignInAgain::default(),
            clock: TvClock::default(),
            opened: None,
            settled: false,
            railed: false,
        }
    }

    /// The focus id of the hero's main button on the page `page`.
    pub fn main_id(page: Id) -> Id {
        page.with("hero").with("main")
    }

    /// The focus id of the way to the requests on the page `page`.
    pub fn requests_id(page: Id) -> Id {
        page.with("requests")
    }

    /// Reads the rows that are near, and this profile's requests.
    fn load(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let at = match self.section {
            HERO => 0,
            MINE | CREDIT => DISCOVER_ROWS.len(),
            k => k as usize - 1,
        };
        let reach = (at + LOAD_AHEAD).max(1);
        // Trending with nothing for this profile: every row is asked at once, for a hero.
        let first = &self.readers[0].pages;
        let all = (first.done() || first.stopped) && first.titles.is_empty();
        for (i, reader) in self.readers.iter_mut().enumerate() {
            reader.update(cx, all || i <= reach);
        }
        // This profile's requests, at the foot.
        let own = cx.state.request_summary().is_some_and(|s| s.own > 0);
        if own && self.mine.is_none() {
            let query = RequestQuery { mine: true, limit: Some(MINE_LIMIT), ..RequestQuery::default() };
            self.mine = Some(Resource::new(None, move |api| api.requests(&query)));
        } else if !own {
            self.mine = None;
        }
        if let Some(mine) = self.mine.as_mut() {
            mine.update(cx);
            let list = mine.value().map(|l| l.requests.as_slice()).unwrap_or_default();
            let every = if !list.is_empty() && moving(list) {
                Some(10.0)
            } else if cx.state.requests_events_down() {
                Some(30.0)
            } else {
                None
            };
            if self.mine_poll.due(ui, cx, every) {
                mine.invalidate();
            }
        }
    }

    /// The tiles of the rows and the foot, made again when what they come from changes.
    fn build(&mut self, ui: &Ui, cx: &AppCx) {
        let marks = marks_version(cx);
        for (i, row) in self.rows.iter_mut().enumerate() {
            let pages = &self.readers[i].pages;
            let source = (pages.titles.len(), pages.at.to_bits(), marks);
            if row.source == Some(source) {
                continue;
            }
            row.source = Some(source);
            let mixed = DISCOVER_ROWS[i].mixed;
            row.tiles =
                pages.titles.iter().map(|t| TitleTileModel::new(cx, ui, marked(cx, t, pages.at), mixed)).collect();
        }
        let list = self.mine.as_ref().and_then(|m| m.value());
        let source = list.map(|l| l as *const RequestList as usize ^ l.requests.len());
        if source != self.mine_source {
            self.mine_source = source;
            let w = TileKind::Landscape.size().w;
            self.mine_tiles = list
                .map(|l| l.requests.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let mut pictures = art(cx, ui, r.backdrop_url.as_deref(), w);
                    pictures.extend(art(cx, ui, r.poster_url.as_deref(), w));
                    MineTile { request: r.clone(), art: pictures }
                })
                .collect();
        }
    }

    /// The hero: the first title left (lists leave out what this profile can't ask for); its details
    /// for its main action, asked again when requests change for it, and while it downloads.
    fn hero(&mut self, ui: &mut Ui, cx: &mut AppCx) -> Option<(DiscoverTitle, Option<DiscoverDetail>)> {
        let title = self.rows.iter().find_map(|r| r.tiles.first()).map(|t| t.title.clone())?;
        let key = title_key(title.kind, title.tmdb_id);
        if self.hero.as_ref().is_none_or(|(k, _)| *k != key) {
            let (kind, id) = (title.kind, title.tmdb_id);
            self.hero = Some((key, Resource::new(None, move |api| api.discover_title(kind, id))));
        }
        let (_, detail) = self.hero.as_mut()?;
        if detail.update(cx)
            && let Some(d) = detail.value()
        {
            keep_title(cx, d);
        }
        let d = detail.value().cloned();
        let every = d.as_ref().filter(|d| moving(&d.requests)).map(|_| 10.0);
        if self.hero_poll.due(ui, cx, every) {
            detail.invalidate();
        }
        Some((title, d))
    }

    fn carry_out(&mut self, intent: TitleIntent, title: &DiscoverTitle, cx: &mut AppCx) {
        let open = |pick: bool| Route::Title { kind: title.kind, tmdb_id: title.tmdb_id, pick };
        match intent {
            TitleIntent::Ask(body) => {
                if self.asking.is_none() {
                    self.asking = Some(Asking::start(cx, body));
                }
            }
            TitleIntent::Pick => cx.push(open(true)),
            TitleIntent::Open(id) => cx.push(Route::Item { id }),
            TitleIntent::Unwatch(r) => {
                if self.unwatching.is_none() {
                    self.unwatching = Some(Unwatching::start(cx, *r));
                }
            }
            TitleIntent::SignIn => self.sign_in.open = true,
            TitleIntent::Extra(_) => cx.push(open(false)),
        }
    }

    /// Discover's top: the way to this profile's requests, with their number, for a profile that
    /// has any to see; it fades while focus is in the rows.
    fn bar(&mut self, ui: &mut Ui, cx: &mut AppCx, page: Id) {
        let s = cx.state.request_summary();
        if !has_requests(s) || !s.is_some_and(|s| s.own > 0 || s.manage) {
            return;
        }
        let fade = ui.animate(page.with("bar.fade"), if self.in_rows { 0.0 } else { 1.0 }, Motion::REVEAL);
        if fade <= 0.0 || self.in_rows {
            return;
        }
        let (count, danger) = requests_count(s);
        let chip = Counted {
            look: focus::CHIP,
            icon: Some(Icon::Inbox),
            label: requests_title(s),
            count: (count > 0).then(|| count_text(count, 99)),
            danger,
            padding: 16.0,
        };
        let id = DiscoverScreen::requests_id(page);
        let r = ui.opacity(fade, |ui| chip.show(ui, id, Point::new(theme::GUTTER, theme::SAFE_Y)));
        if r.activated {
            cx.push(Route::Requests);
        }
    }
}

impl Screen for DiscoverScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let page = ui.scope_id();
        let opened = *self.opened.get_or_insert(cx.now);
        let time = self.clock.time(cx);
        if self.asking.as_mut().and_then(|a| a.poll(cx, time)).is_some() {
            self.asking = None;
        }
        if self.unwatching.as_mut().and_then(|u| u.poll(cx, time)).is_some() {
            self.unwatching = None;
        }

        if let Some(gate) = summary_problem(cx.state.request_summary()) {
            self.settled = true;
            let (pressed, focusable) =
                problem_page(ui, cx, page.with("gate"), &gate, t!("requests.discover.loadFailed"));
            self.rail_if(cx, !focusable);
            match pressed {
                Some(ProblemAction::Retry) => cx.state.load_requests(false),
                Some(ProblemAction::SignIn) => self.sign_in.open = true,
                Some(ProblemAction::Requests) => cx.push(Route::Requests),
                None => {}
            }
            self.sign_in.show(ui, cx, page.with("sign-in"));
            return;
        }

        self.load(ui, cx);
        self.build(ui, cx);

        // TMDB not answering for the first list: the page says so once rather than in every row.
        let trending = &self.readers[0].pages;
        if trending.titles.is_empty()
            && let Some(e) = trending.error.clone()
        {
            self.settled = true;
            let problem = requests_problem(&e);
            let (pressed, focusable) =
                problem_page(ui, cx, page.with("blocked"), &problem, t!("requests.discover.loadFailed"));
            self.rail_if(cx, !focusable);
            match pressed {
                Some(ProblemAction::Retry) => self.readers[0].again(),
                Some(ProblemAction::SignIn) => self.sign_in.open = true,
                Some(ProblemAction::Requests) => cx.push(Route::Requests),
                None => {}
            }
            self.sign_in.show(ui, cx, page.with("sign-in"));
            return;
        }

        let Some((hero, detail)) = self.hero(ui, cx) else {
            if self.readers.iter().all(|r| r.pages.settled()) {
                self.settled = true;
                Message::new(MessageStyle::Item, t!("requests.discover.nothing"))
                    .icon(Some(Icon::Compass))
                    .body(t!("requests.discover.nothingText"))
                    .show(ui, page.with("nothing"));
                self.rail_if(cx, true);
            } else if cx.now - opened >= SKELETON_AFTER {
                skeleton(ui);
            } else {
                ui.repaint_after(SKELETON_AFTER - (cx.now - opened));
            }
            return;
        };
        self.settled = true;
        self.railed = false;
        self.rows_ui(ui, cx, page, &hero, detail.as_ref());
        self.bar(ui, cx, page);
        self.sign_in.show(ui, cx, page.with("sign-in"));
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        // Back from the rows: the hero again, with its main action.
        if self.in_rows {
            self.to_hero = true;
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::RequestsChanged { title } => {
                if let Some(m) = self.mine.as_mut() {
                    m.invalidate();
                }
                if let Some((key, d)) = self.hero.as_mut()
                    && title.as_ref().is_none_or(|t| t == key)
                {
                    d.invalidate();
                }
            }
            AppEvent::Shown | AppEvent::Visible(true) => {
                self.clock.forget();
                if let Some(m) = self.mine.as_mut() {
                    m.revalidate(cx.now);
                }
                if let Some((_, d)) = self.hero.as_mut() {
                    d.revalidate(cx.now);
                }
                // Badges marked on a title's page show at once.
                for row in &mut self.rows {
                    row.source = None;
                }
            }
            AppEvent::Changed => {
                if let Some(m) = self.mine.as_mut() {
                    m.revalidate(cx.now);
                }
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.settled
    }

    fn modal(&self) -> bool {
        self.sign_in.open
    }
}

impl DiscoverScreen {
    /// A page with nothing to focus sends focus to the rail, once.
    fn rail_if(&mut self, cx: &mut AppCx, nothing: bool) {
        if nothing && !std::mem::replace(&mut self.railed, true) {
            cx.to_rail();
        }
    }

    fn rows_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppCx,
        page: Id,
        hero: &DiscoverTitle,
        detail: Option<&DiscoverDetail>,
    ) {
        let column_id = page.with("rows");
        let hero_id = page.with("hero");
        let main_id = DiscoverScreen::main_id(page);
        let time = self.clock.time(cx);
        let actions = Actions::of(detail, hero, cx.state.request_summary(), time);
        let extras =
            [Extra { key: "details", icon: Icon::Info, label: t!("requests.buttons.details"), unclaimed_only: false }];
        ui.initial_focus(main_id);

        // Where focus is, as the last frame left it.
        if ui.focused_in(hero_id).is_some() {
            self.in_rows = false;
            self.section = HERO;
        } else if ui.focused_in(column_id).is_some() {
            self.in_rows = true;
        }
        if ui.rested() && self.focused != self.rested {
            self.rested = self.focused.clone();
        }

        // Backdrop and info block.
        let shown = if self.in_rows { self.rested.clone() } else { None };
        let (picture, name, key) = match &shown {
            Some(Focused::Title(t)) => (t.backdrop_url.clone(), t.title.clone(), title_key(t.kind, t.tmdb_id)),
            Some(Focused::Request(r)) => (r.backdrop_url.clone(), r.title.clone(), format!("r{}", r.id)),
            None => {
                let t = detail.map_or(hero, |d| &d.title);
                (t.backdrop_url.clone(), t.title.clone(), title_key(t.kind, t.tmdb_id))
            }
        };
        let px = backdrop_px(ui);
        let wanted = BackdropArt::new(hash(&key), picture.iter().filter_map(|p| cx.image(p, px)).collect(), name);
        let scrim = if self.in_rows { BackdropScrim::Rows } else { BackdropScrim::Hero };
        Backdrop::new(scrim).show(ui, page.with("backdrop"), &mut self.backdrop, Some(&wanted));
        let manage = cx.state.request_summary().is_some_and(|s| s.manage);
        InfoBlock::show(ui, page.with("info"), shown.is_some(), |ui| {
            if let Some(f) = &shown {
                draw_info(ui, f, manage, time);
            }
        });

        if std::mem::take(&mut self.to_hero) {
            // Back from the rows: the page goes to its top; the hero's buttons, registered
            // unseen this frame, take focus (drawn, the rows would put the page back).
            self.column.section = 0;
            self.column.shift = 0.0;
            self.in_rows = false;
            self.section = HERO;
            ui.clip(Rect::ZERO, |ui| {
                actions.show(ui, hero_id, Point::new(theme::GUTTER, 0.0), TEXT_COLUMN, true, true, &extras);
            });
            ui.request_focus(main_id);
            ui.request_repaint();
            return;
        }

        let fade = ui.animate(hero_id.with("fade"), if self.in_rows { 0.0 } else { 1.0 }, Motion::REVEAL);
        let mut sections = vec![Section::hero(HERO, HERO_H)];
        for i in 0..DISCOVER_ROWS.len() {
            // A row TMDB gave nothing for (or couldn't give) goes; one still loading keeps its place.
            if self.rows[i].tiles.is_empty() && self.readers[i].pages.settled() {
                continue;
            }
            sections.push(Section::row(1 + i as u64, TileKind::Poster.row_height(true)));
        }
        if !self.mine_tiles.is_empty() {
            sections.push(Section::row(MINE, TileKind::Landscape.row_height(true)));
        }
        sections.push(Section::new(CREDIT, 18.0, None));

        let mut intent = None;
        let mut open = None;
        let mut focused = None;
        let mut focused_section = None;
        let rows = &self.rows;
        let mine = &self.mine_tiles;
        let row_states = &mut self.row_states;
        let time_now = time;
        RowColumn::new(0.0).show(ui, column_id, &mut self.column, &sections, |ui, i, top| {
            let key = sections[i].key;
            match key {
                HERO => {
                    let at = Point::new(theme::GUTTER, top);
                    let buttons_y = ui.opacity(fade, |ui| draw_hero(ui, at, hero, detail));
                    let pressed = ui.opacity(fade.max(0.0), |ui| {
                        actions.show(
                            ui,
                            hero_id,
                            Point::new(theme::GUTTER, buttons_y),
                            TEXT_COLUMN,
                            true,
                            true,
                            &extras,
                        )
                    });
                    intent = intent.take().or(pressed);
                }
                CREDIT => {
                    let at = Point::new(theme::GUTTER, top);
                    ui.text(at, tmdb_credit(), &theme::CAPTION, &TextOpts::line(800.0), theme::SUBTLE);
                }
                MINE => {
                    let state = row_states.entry(MINE).or_default();
                    let r = Row::new(TileKind::Landscape, top).header(t!("requests.names.yourRequests")).show(
                        ui,
                        ui.id("row"),
                        state,
                        mine.len() + 1,
                        |i| mine.get(i).map_or(u64::MAX, |t| t.request.id as u64),
                        |ui, slot| paint_mine(ui, slot, mine, manage, time_now),
                    );
                    if let Some(f) = r.focused {
                        focused_section = Some(MINE);
                        focused = Some(mine.get(f).map(|t| Focused::Request(Box::new(t.request.clone()))));
                    }
                    if let Some(a) = r.activated {
                        open = Some(match mine.get(a) {
                            Some(t) => Route::Title { kind: t.request.kind, tmdb_id: t.request.tmdb_id, pick: false },
                            None => Route::Requests,
                        });
                    }
                }
                k => {
                    let index = k as usize - 1;
                    let row = &rows[index];
                    let title = DISCOVER_ROWS[index].title();
                    if row.tiles.is_empty() {
                        row_skeleton(ui, top, title);
                        return;
                    }
                    let state = row_states.entry(k).or_default();
                    let tiles = &row.tiles;
                    let r = Row::new(TileKind::Poster, top).header(title).show(
                        ui,
                        ui.id("row"),
                        state,
                        tiles.len() + 1,
                        |i| tiles.get(i).map_or(u64::MAX, |t| hash(&t.key())),
                        |ui, slot| match tiles.get(slot.index) {
                            Some(t) => paint_title(ui, slot, t),
                            None => Tile::see_all(TileKind::Poster, t!("requests.buttons.seeAll")).paint(ui, slot),
                        },
                    );
                    if let Some(f) = r.focused {
                        focused_section = Some(k);
                        focused = Some(tiles.get(f).map(|t| Focused::Title(t.title.clone())));
                    }
                    if let Some(a) = r.activated {
                        open = Some(match tiles.get(a) {
                            Some(t) => title_page(&t.title),
                            None => Route::DiscoverList { list: DISCOVER_ROWS[index].list },
                        });
                    }
                }
            }
        });
        if let Some(k) = focused_section {
            self.section = k;
        }
        // See all names no title: the info block stays on the tile before it.
        if let Some(Some(f)) = focused {
            self.focused = Some(f);
        }
        if let Some(route) = open {
            cx.push(route);
        }
        if let Some(intent) = intent {
            let t = detail.map_or(hero.clone(), |d| d.title.clone());
            self.carry_out(intent, &t, cx);
        }
    }
}

/// A stable number for a key.
fn hash(key: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    h.finish() >> 1
}

/// The hero's words from `at` (`DiscoverHero`): the title's two lines ending at 172, its facts,
/// a line of its overview; where its buttons go.
fn draw_hero(ui: &mut Ui, at: Point, hero: &DiscoverTitle, d: Option<&DiscoverDetail>) -> f32 {
    let t = d.map_or(hero, |d| &d.title);
    let x = at.x;
    let top = at.y + HERO_TITLE_TOP;
    let opts = TextOpts { balance: true, ..TextOpts::wrap(TEXT_COLUMN, 2) };
    let title = ui.layout(&t.title, &theme::HERO_TITLE, &opts);
    let h = title.size.h;
    ui.draw_text(Point::new(x, top + HERO_TITLE_BOX - h), title, theme::FG);
    let mut y = top + HERO_TITLE_BOX + 8.0;
    title_meta(ui, Point::new(x, y), hero, d, TEXT_COLUMN);
    y += 18.0;
    if !t.overview.is_empty() {
        y += 10.0;
        y += ui.text(Point::new(x, y), &t.overview, &theme::BODY_LARGE, &TextOpts::line(TEXT_COLUMN), theme::SUMMARY).h;
    }
    y + 20.0
}

/// The info block over the rows: the title, then a title's facts and overview, or a request's stage,
/// who asked and its line.
fn draw_info(ui: &mut Ui, f: &Focused, manage: bool, time: pley_core::requests::TvTime) {
    let r = InfoBlock::RECT;
    let name = match f {
        Focused::Title(t) => &t.title,
        Focused::Request(q) => &q.title,
    };
    let title = ui.layout(name, &theme::PAGE_TITLE, &TextOpts::line(r.w));
    let h = title.size.h;
    ui.draw_text(Point::new(r.x, r.y + 52.0 - h), title, theme::FG);
    let mut y = r.y + 52.0 + 10.0;
    match f {
        Focused::Title(t) => {
            title_meta(ui, Point::new(r.x, y), t, None, r.w);
            y += 18.0 + 8.0;
            if !t.overview.is_empty() {
                ui.text(Point::new(r.x, y), &t.overview, &theme::BODY, &TextOpts::wrap(r.w, 2), theme::SUMMARY);
            }
        }
        Focused::Request(q) => {
            let stage = stage_words(q, manage, time);
            let size = stage_pill(ui, Point::new(r.x, y), &stage, true, r.w);
            let seasons = q.seasons.as_deref().map(seasons_label).unwrap_or_default();
            if !seasons.is_empty() {
                let at = Point::new(r.x + size.w + 8.0, y + 2.0);
                ui.text(at, &seasons, &theme::LABEL, &TextOpts::line(r.w - size.w - 8.0), theme::MUTED);
            }
            y += size.h + 8.0;
            y += ui
                .text(
                    Point::new(r.x, y),
                    &asked_line(q, manage, time),
                    &theme::CAPTION,
                    &TextOpts::line(r.w),
                    theme::MUTED,
                )
                .h;
            if let Some(note) = &stage.note {
                ui.text(Point::new(r.x, y + 2.0), note, &theme::CAPTION, &TextOpts::line(r.w), theme::MUTED);
            }
        }
    }
}

/// A request at the foot as a wide tile: its picture, a download's progress on it, the title, its
/// stage in the stage's colour; See all after them.
fn paint_mine(ui: &mut Ui, slot: &TileSlot, mine: &[MineTile], manage: bool, time: pley_core::requests::TvTime) {
    let Some(t) = mine.get(slot.index) else {
        Tile::see_all(TileKind::Landscape, t!("requests.buttons.seeAll")).paint(ui, slot);
        return;
    };
    let w = stage_words(&t.request, manage, time);
    ToneTile {
        kind: TileKind::Landscape,
        title: &t.request.title,
        line: &w.text,
        tone: Some(w.tone),
        lines: 1,
        art: &t.art,
        progress: w.progress,
        badge: None,
        news: t.request.unseen,
    }
    .paint(ui, slot);
}

/// A row's place while its first page comes: its title and grey wells; nothing takes focus.
pub fn row_skeleton(ui: &mut Ui, top: f32, title: &str) {
    let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    ui.text(Point::new(theme::GUTTER, top), title, &theme::SECTION, &TextOpts::line(width), theme::FG);
    let kind = TileKind::Poster;
    let y = top + theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES;
    for i in 0..7 {
        let x = theme::GUTTER + i as f32 * kind.pitch();
        skeleton_block(ui, Rect::new(x, y, kind.size().w, kind.size().h), radius::TILE);
    }
}

/// Discover while its first list comes (after 300 ms): grey blocks where the hero and the first row go.
fn skeleton(ui: &mut Ui) {
    let x = theme::GUTTER;
    let block = |ui: &mut Ui, y: f32, w: f32, h: f32, r: f32| skeleton_block(ui, Rect::new(x, y, w, h), r);
    block(ui, 131.0, 300.0, 38.0, radius::BADGE);
    block(ui, 180.0, 200.0, 18.0, radius::BADGE);
    block(ui, 212.0, 440.0, 15.0, radius::BADGE);
    block(ui, 234.0, 380.0, 15.0, radius::BADGE);
    block(ui, 272.0, 128.0, 40.0, radius::CONTROL);
    block(ui, 384.0, 180.0, 18.0, radius::BADGE);
    let kind = TileKind::Poster;
    for i in 0..6 {
        let at = Rect::new(x + i as f32 * kind.pitch(), 422.0, kind.size().w, kind.size().h);
        skeleton_block(ui, at, radius::TILE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_hash_the_same_and_never_collide_with_see_all() {
        assert_eq!(hash("movie:603"), hash("movie:603"));
        assert_ne!(hash("movie:603"), hash("show:603"));
        assert!(hash("movie:1") < u64::MAX);
    }
}
