//! The Search page (`TvSearchScreen.kt`, `TvSearchField.kt`; the TV app's `search.tsx`).
//!
//! The field sits at (96, 27), 520 wide, and has focus first. OK on it slides the kit's keyboard up
//! over the bottom of the page (Android opens the system keyboard; the LG app draws its own, and
//! text typed on a keyboard plugged into the TV goes into the field as well); Back, Up from the
//! keyboard's top row and its Search key close it, Search also keeping the query among the recent
//! searches. What is typed is asked 250 ms after typing pauses, and the results come as rows whose
//! headers sit on y 96 (scrolling up under the field, clipped above y 84), in the order of
//! [`Group::ALL`]. With nothing typed, the profile's recent searches show as chips with Clear, or,
//! with none, what search finds. Down from the field goes to the first row (at the tile it had
//! last), the chips or Retry; Up from them comes back; Left goes to the rail and Right stops.

use std::time::{SystemTime, UNIX_EPOCH};

use pley_base::{Icon, ImageState, Key, Point, Rect, Size, TextOpts};
use pley_core::model::{LiveCategory, LiveChannel, SignInAgain};
use pley_core::requests::{may_ask, problem_words, requests_problem, search_text};
use pley_core::store::keys;
use pley_core::{ApiError, Promise, Store};
use pley_kit::{
    Badge, BadgeStyle, Button, ColumnState, Keyboard, KeyboardState, ProgressBar, Row, RowColumn, RowState, SCROLL,
    Section, SlotMode, TextEdit, TextField, Tile, TileKind, TileSlot, bar_group, first_art, inert, placeholder,
    skeleton_row,
};
use pley_requests::title_badge;
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Edge, Group, Id, ImageOpts, Motion, Ui};

use crate::query::{LIMIT, Query, Shown};
use crate::recent::Recents;
use crate::results::{
    ChannelResult, Found, Group as ResultGroup, ResultTile, TmdbTile, Widths, channel_caption, channel_progress,
    read_tmdb, tmdb_tile,
};
use crate::tmdb::{OnTmdb, TmdbState};

/// The field's top-left corner: the gutter, the safe area's top.
pub const FIELD_AT: Point = Point::new(theme::GUTTER, theme::SAFE_Y);
/// The field's width.
pub const FIELD_WIDTH: f32 = 520.0;
/// Where the rows' headers sit (`ResultsTop`), and the recent searches' header.
pub const ROWS_TOP: f32 = theme::HEADER_Y_SEARCH;
/// Nothing of the rows shows above this line once they have moved.
pub const ROWS_CLIP: f32 = ROWS_TOP - theme::ROWS_CLIP_ABOVE;
/// Where a notice's title sits (`NoticeTop`).
pub const NOTICE_TOP: f32 = 120.0;
/// A notice's widest (`widthIn(max = 560.dp)`).
pub const NOTICE_WIDTH: f32 = 560.0;
/// "Searching…" to the right of the field.
const SEARCHING_GAP: f32 = 20.0;
/// Between the recent searches' header and their chips, and the room above the chips for their
/// growth (the chip row's padding).
const RECENT_GAP: f32 = 16.0;
const CHIP_ROOM: f32 = 6.0;
/// Between chips.
const CHIP_GAP: f32 = 12.0;
/// A chip's widest (`widthIn(max = 320.dp)`): its label, less the padding and the history icon.
const CHIP_LABEL_WIDTH: f32 = 320.0 - 2.0 * 16.0 - theme::ICON_SIZE - 8.0;
/// A channel's logo inside its well.
const LOGO_INSET: f32 = 8.0;
/// A channel tile's marks: the number badge and the favourite star, 6 in from the corners.
const MARK_INSET: f32 = 6.0;
const STAR_SIZE: f32 = 16.0;
/// What's on along a channel tile's foot.
const PROGRAM_LINE: f32 = 3.0;
/// How often what's on a channel found is looked at again (`NOW_TICK_MS`).
const NOW_TICK: f64 = 30.0;
/// The most characters the field takes.
pub const MAX_QUERY: usize = 120;
/// The most lines and characters of what went wrong a failure shows, so Retry stays on the page.
const PROBLEM_LINES: u16 = 3;
const PROBLEM_CHARS: usize = 300;

/// The field's placeholder.
pub const PLACEHOLDER: &str = "Search movies, shows, people…";
/// "Searching…" beside the field while an answer is on its way.
pub const SEARCHING: &str = "Searching…";
/// The recent searches' header.
pub const RECENT: &str = "Recent searches";
/// The text button after the recent searches, which forgets them.
pub const CLEAR: &str = "Clear";
/// What search finds, with nothing typed and no recent searches. Android's sentence ends with "or
/// use your keyboard's microphone": the keyboard drawn here has none.
pub const EMPTY_TITLE: &str = "Search your libraries";
/// The sentence under [`EMPTY_TITLE`].
pub const EMPTY_BODY: &str = "Find movies, shows, episodes, people, artists, albums, tracks and Live TV channels. Press OK on the field to type.";
/// Under `No results for “north”`.
pub const NONE_BODY: &str = "Try a different spelling or fewer words.";
/// A failure's title (`failedTitle`).
pub const FAILED: &str = "Search failed";
/// The failure's title when the server didn't answer.
pub const CANT_REACH: &str = "Can't reach your server";
/// The button under a failure, which asks again.
pub const RETRY: &str = "Retry";
/// The keyboard's action key (`ImeAction.Search`).
pub const KEYBOARD_ACTION: &str = "Search";

/// The header of the row of what TMDB has that the libraries lack.
pub const DISCOVER: &str = "Discover";
/// The tile after Discover's titles, which opens everything TMDB has for the words.
pub const SEE_ALL: &str = "See all";
/// The line over the Discover row when the libraries have nothing.
pub const NONE_IN_LIBRARIES: &str = "Nothing in your libraries matches";
/// Why the Discover row is empty when TMDB's search failed for a reason that isn't requests'.
pub const TMDB_FAILED: &str = "Couldn't search TMDB";
/// The sections after the libraries' rows: the line, the Discover row.
const NONE_KEY: u64 = 100;
const TMDB_KEY: u64 = 101;
/// The line's height.
const NONE_HEIGHT: f32 = 20.0;
/// The Discover row saying why TMDB didn't answer: its header, the problem in a line and why in
/// up to two, and Retry.
fn tmdb_problem_height() -> f32 {
    theme::ROW_HEADER_HEIGHT
        + theme::HEADER_TO_TILES
        + theme::TITLE.line_height
        + 4.0
        + 2.0 * theme::BODY.line_height
        + 16.0
        + theme::BUTTON_HEIGHT
}
/// The skeleton posters while TMDB answers.
const TMDB_SKELETONS: usize = 7;

/// `No results for “north”`.
pub fn none_title(query: &str) -> String {
    format!("No results for \u{201c}{query}\u{201d}")
}

/// What was chosen on the page this frame, carried out once it is drawn.
#[derive(Debug)]
enum Act {
    /// A result: the query is kept among the recent searches, then its page opens.
    Open(Route),
    /// A title's actions (OK held, or the menu key).
    Actions(i64),
    /// A channel found: it plays, changing channel among every channel.
    Watch(Box<LiveChannel>),
    /// A recent search, searched again.
    Recent(String),
    Clear,
    Retry,
    /// TMDB's search again, after it failed.
    RetryTmdb,
}

/// The ids of the page's parts.
struct Ids {
    field_row: Id,
    field: Id,
    column: Id,
    chips: Id,
    clear: Id,
    retry_row: Id,
    retry: Id,
    keyboard: Id,
    tmdb_row: Id,
    tmdb_retry_row: Id,
    tmdb_retry: Id,
}

impl Ids {
    /// The ids under the page's `base`; the rows' and chips' are new for each `turn` of what
    /// shows, so a row remembers only tiles of the results it shows, and the recent searches come
    /// back at their first chip.
    fn new(base: Id, turn: u64) -> Ids {
        Ids {
            field_row: base.with("field-row"),
            field: base.with("field"),
            column: base.with(("results", turn)),
            chips: base.with(("recent", turn)),
            clear: base.with(("recent-clear", turn)),
            retry_row: base.with("retry-row"),
            retry: base.with("retry"),
            keyboard: base.with("keyboard"),
            tmdb_row: base.with(("results", turn)).with("tmdb"),
            tmdb_retry_row: base.with("tmdb-retry-row"),
            tmdb_retry: base.with("tmdb-retry"),
        }
    }

    fn row(&self, group: ResultGroup) -> Id {
        self.column.with(("row", group.index()))
    }

    fn chip(&self, query: &str) -> Id {
        self.chips.with(("chip", query))
    }
}

/// Where the page's rows and chips are, kept while the page lives.
#[derive(Debug, Default)]
struct View {
    /// The results' turn the rows were laid out for.
    turn: Option<u64>,
    column: ColumnState,
    rows: [RowState; 9],
    /// How far the chips are moved left.
    chips: f32,
    /// The Discover row, and the turn of its words it was laid out for.
    tmdb: RowState,
    tmdb_turn: Option<u64>,
}

/// TMDB's titles for the Discover row, made into tiles on the task pool.
type TmdbAnswer = Promise<Result<Vec<TmdbTile>, ApiError>>;

/// The Search page.
pub struct SearchScreen {
    edit: TextEdit,
    query: Query,
    /// The answer on its way, sorted into rows on the task pool, with the query it answers.
    request: Option<(String, Promise<Result<Found, ApiError>>)>,
    /// The keyboard shows.
    typing: bool,
    keyboard: KeyboardState,
    /// The profile's recent searches.
    recents: Recents,
    /// The sign-in and profile the recent searches were read for.
    generation: Option<u64>,
    view: View,
    /// The last failure was the server out of reach: the next answer says it is back.
    unreachable: bool,
    /// Focus goes to the field at the next frame (a chip chosen, the history cleared, Retry).
    focus_field: bool,
    /// What TMDB has for the words, for the Discover row.
    tmdb: OnTmdb,
    /// TMDB's answer on its way, made into tiles on the task pool, with the words it answers.
    tmdb_request: Option<(String, TmdbAnswer)>,
    /// Requests or the libraries changed: the titles shown are asked again for their badges.
    tmdb_stale: bool,
    /// The pictures' widths on this screen, measured as it draws.
    widths: Option<Widths>,
}

impl SearchScreen {
    /// The page with `text` in the field, asked at once (a search opened with a query); the recent
    /// searches are kept in `store`.
    pub fn new(text: &str, now: f64, store: Store) -> SearchScreen {
        let edit = TextEdit::new(text.chars().take(MAX_QUERY).collect::<String>()).limit(MAX_QUERY);
        let mut query = Query::new();
        query.typed(&edit.text, now);
        query.now(now);
        SearchScreen {
            edit,
            query,
            request: None,
            typing: false,
            keyboard: KeyboardState::default(),
            recents: Recents::new(store),
            generation: None,
            view: View::default(),
            unreachable: false,
            focus_field: false,
            tmdb: OnTmdb::new(),
            tmdb_request: None,
            tmdb_stale: false,
            widths: None,
        }
    }

    /// What the field says.
    pub fn text(&self) -> &str {
        &self.edit.text
    }

    /// Whether the keyboard shows.
    pub fn typing(&self) -> bool {
        self.typing
    }

    /// Reads the recent searches again after another sign-in or profile, and asks again what the
    /// results show, which were the other profile's.
    fn follow(&mut self, cx: &mut AppCx) {
        let generation = cx.state.generation();
        if self.generation == Some(generation) {
            return;
        }
        if self.generation.is_some() {
            self.request = None;
            self.query.restart(cx.now);
            self.tmdb.clear();
            self.tmdb_request = None;
        }
        self.generation = Some(generation);
        let key = match (cx.state.server(), cx.state.profile()) {
            (Some(server), Some(profile)) => Some(keys::searches(server, profile)),
            _ => None,
        };
        self.recents.load(key);
    }

    /// Takes the answer when it has come.
    fn collect(&mut self, cx: &mut AppCx, now: f64) {
        let Some((_, promise)) = self.request.as_mut() else { return };
        let answer = match promise.take() {
            Some(answer) => answer,
            None if promise.failed() => Err(ApiError::Unreadable("the request stopped".into())),
            None => return,
        };
        let Some((query, _)) = self.request.take() else { return };
        match &answer {
            Ok(_) => {
                if std::mem::take(&mut self.unreachable) {
                    cx.state.server_answered();
                }
            }
            Err(e) if e.cancelled() => {}
            Err(e) => {
                log::info!("search failed: {e}");
                cx.state.report_failure(e);
                self.unreachable = e.unreachable();
            }
        }
        self.query.answered(&query, answer, now);
    }

    /// Asks the server for `query`; the answer becomes rows on the task pool, its pictures at the
    /// widths this screen draws them.
    fn ask(&mut self, ui: &Ui, cx: &AppCx, query: String) {
        let (asked, widths) = (query.clone(), Widths::new(|w| ui.image_px(w)));
        let promise = cx.spawn(move |api| {
            let results = api.search(&asked, LIMIT)?;
            Ok(Found::new(results, &|path, kind| Some(api.image(path, widths.of(kind)))))
        });
        self.request = Some((query, promise));
    }

    /// Asks TMDB for `words`, pages in turn until the row has its titles, without the libraries'
    /// whole titles; they become tiles on the task pool.
    fn ask_tmdb(&mut self, cx: &AppCx, words: String) {
        let Some(widths) = self.widths else { return };
        let asked = words.clone();
        let promise = cx.spawn(move |api| {
            let titles = read_tmdb(|n| api.discover_search(&asked, n, true))?;
            Ok(titles.iter().map(|t| tmdb_tile(t, &|path, kind| Some(api.image(path, widths.of(kind))))).collect())
        });
        self.tmdb_request = Some((words, promise));
    }

    /// TMDB's answer, when it has come. Focus on Retry, which goes with the problem, moves to the
    /// titles in its place, else to the field.
    fn collect_tmdb(&mut self, ui: &mut Ui, ids: &Ids) {
        let Some((_, promise)) = self.tmdb_request.as_mut() else { return };
        let answer = match promise.take() {
            Some(answer) => answer,
            None if promise.failed() => Err(ApiError::Unreadable("the request stopped".into())),
            None => return,
        };
        let Some((words, _)) = self.tmdb_request.take() else { return };
        self.tmdb.answered(&words, answer);
        ui.request_repaint();
        if ui.focused() == Some(ids.tmdb_retry) {
            match self.tmdb.shown(&words) {
                Some(TmdbState::Failed(_)) => {}
                Some(TmdbState::Ready(tiles)) if !tiles.is_empty() => {
                    ui.request_focus(ids.tmdb_row.with(tiles[0].key()));
                }
                _ => self.focus_field = true,
            }
        }
    }

    /// Keeps `query` first among the profile's recent searches.
    fn remember(&mut self, cx: &AppCx, query: &str) {
        self.recents.remember(cx.tasks(), query);
    }

    /// The field now says `text` (the keyboard, a chip, a keyboard plugged into the TV).
    fn set_text(&mut self, text: String, now: f64) {
        self.edit = TextEdit::new(text.chars().take(MAX_QUERY).collect::<String>()).limit(MAX_QUERY);
        self.query.typed(&self.edit.text, now);
    }

    fn act(&mut self, act: Act, cx: &mut AppCx, now: f64) {
        match act {
            Act::Open(route) => {
                if let Some(q) = self.query.answered_query().map(str::to_owned) {
                    self.remember(cx, &q);
                }
                cx.push(route);
            }
            Act::Actions(id) => cx.push(Route::ItemActions { id, continue_watching: false }),
            Act::Watch(channel) => {
                if let Some(q) = self.query.shown_query().map(str::to_owned) {
                    self.remember(cx, &q);
                }
                let channel_id = channel.id;
                cx.state.keep_live_channel(*channel);
                cx.push(Route::LivePlayer { channel_id, category: LiveCategory::All, pressed_at: cx.now });
            }
            Act::Recent(q) => {
                // The chips go once the results come: focus waits on the field.
                self.set_text(q.clone(), now);
                self.remember(cx, &q);
                self.focus_field = true;
            }
            Act::Clear => {
                self.recents.clear(cx.tasks());
                self.focus_field = true;
            }
            Act::Retry => {
                self.query.retry(now);
                self.focus_field = true;
            }
            Act::RetryTmdb => {
                if let Some(words) = self.tmdb.retry() {
                    self.ask_tmdb(cx, words);
                }
            }
        }
    }

    /// The field, and "Searching…" beside it while an answer is on its way.
    fn field(&mut self, ui: &mut Ui, ids: &Ids, below: Option<Id>, now: f64) {
        let group = Group::row()
            .edge(Dir::Left, Edge::Out)
            .edge(Dir::Right, Edge::Stop)
            .edge(Dir::Down, below.map_or(Edge::Pass, Edge::To));
        let field = TextField::new(&self.edit.text, FIELD_WIDTH)
            .placeholder(PLACEHOLDER)
            .icon(Icon::Search)
            .typing(self.typing);
        let r = ui.group(ids.field_row, group, |ui| field.show(ui, ids.field, FIELD_AT));
        if self.query.searching() {
            let y = FIELD_AT.y + (pley_kit::keyboard::FIELD_HEIGHT - theme::CAPTION.line_height) / 2.0;
            let x = FIELD_AT.x + FIELD_WIDTH + SEARCHING_GAP;
            ui.text(Point::new(x, y), SEARCHING, &theme::CAPTION, &TextOpts::default(), theme::MUTED);
        }
        if self.typing {
            return;
        }
        if r.activated {
            self.typing = true;
            self.keyboard = KeyboardState::default();
            ui.request_repaint();
        } else if r.focused {
            // Text typed on a keyboard plugged into the TV goes in as well, and the remote's digits
            // when they brought no text (a key that brings both types once, held or not).
            let typed = if ui.typed().is_empty() {
                ui.keys()
                    .iter()
                    .filter_map(|k| match k.key {
                        Key::Digit(d) if k.pressed => char::from_digit(u32::from(d), 10),
                        _ => None,
                    })
                    .collect()
            } else {
                ui.typed().to_owned()
            };
            if !typed.is_empty() && self.edit.insert(&typed) {
                self.query.typed(&self.edit.text, now);
                ui.request_repaint();
            }
        }
    }

    /// The keyboard, sliding up over the bottom of the page while typing and away after.
    fn keyboard(&mut self, ui: &mut Ui, cx: &mut AppCx, ids: &Ids, now: f64) {
        let target = if self.typing { 1.0 } else { 0.0 };
        let shown = ui.animate(ids.keyboard.with("slide"), target, Motion::OVERLAY);
        if shown <= 0.0 {
            return;
        }
        let keyboard = Keyboard::new().action_label(KEYBOARD_ACTION);
        let dy = (1.0 - shown) * Keyboard::HEIGHT;
        let (state, edit, typing) = (&mut self.keyboard, &mut self.edit, self.typing);
        let out = ui.translate(Point::new(0.0, dy), |ui| {
            if typing {
                keyboard.show(ui, ids.keyboard, state, edit)
            } else {
                inert(ui, |ui| keyboard.show(ui, ids.keyboard, state, edit))
            }
        });
        if !typing {
            return;
        }
        if out.changed {
            self.query.typed(&self.edit.text, now);
        }
        if out.done {
            let q = self.query.wanted().to_owned();
            if !q.is_empty() {
                self.remember(cx, &q);
            }
            self.query.now(now);
        }
        if out.done || out.closed {
            self.typing = false;
            self.focus_field = true;
        }
    }
}

impl Screen for SearchScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let now = ui.time();
        claim_typed_digits(ui);
        self.follow(cx);
        self.collect(cx, now);
        self.widths = Some(Widths::new(|w| ui.image_px(w)));
        let asks = asks_tmdb(cx);
        if let Some(q) = self.query.ask(now) {
            // Discover's row is asked beside the libraries, with the same pause, so it never
            // holds them up.
            if asks && let Some(words) = self.tmdb.want(&search_text(&q)) {
                self.ask_tmdb(cx, words);
            }
            self.ask(ui, cx, q);
        }
        if asks {
            // Requests opened to the profile after the libraries were asked; requests or the
            // libraries changed under the titles shown.
            let latest = self.query.asking().or(self.query.answered_query()).map(search_text);
            if let Some(words) = latest.and_then(|w| self.tmdb.want(&w)) {
                self.ask_tmdb(cx, words);
            }
            if std::mem::take(&mut self.tmdb_stale)
                && let Some(words) = self.tmdb.refresh()
            {
                self.ask_tmdb(cx, words);
            }
        }
        if let Some(due) = self.query.due() {
            ui.repaint_after((due - now).max(0.0));
        }
        let turn = self.query.turn();
        if self.view.turn != Some(turn) {
            // Rows open at their first tile, the chips at the gutter.
            self.view = View { turn: Some(turn), ..View::default() };
        }

        if self.view.tmdb_turn != Some(self.tmdb.turn()) {
            // Other words: the Discover row opens at its first title.
            self.view.tmdb = RowState::default();
            self.view.tmdb_turn = Some(self.tmdb.turn());
        }

        let ids = Ids::new(ui.scope_id(), turn);
        self.collect_tmdb(ui, &ids);
        ui.initial_focus(ids.field);
        if std::mem::take(&mut self.focus_field) {
            ui.request_focus(ids.field);
        }

        // What shows under the field, drawn first: the rows scroll up under it.
        let mut act = None;
        let below = match self.query.shown() {
            Shown::Idle if self.recents.list().is_empty() => {
                notice(ui, EMPTY_TITLE, EMPTY_BODY, 0);
                None
            }
            Shown::Idle => recents(ui, &ids, self.recents.list(), &mut self.view.chips, &mut act),
            Shown::Loading { .. } => {
                skeletons(ui);
                None
            }
            Shown::Ready { found, query } => {
                let discover = Discover::of(asks, &self.tmdb, query, cx);
                results(ui, &ids, Some(found), discover.as_ref(), &mut self.view, &mut act)
            }
            Shown::Empty { query } => match Discover::of(asks, &self.tmdb, query, cx) {
                // Nothing in the libraries: the line saying so, over what TMDB has.
                Some(discover) => results(ui, &ids, None, Some(&discover), &mut self.view, &mut act),
                None => {
                    notice(ui, &none_title(query), NONE_BODY, 0);
                    None
                }
            },
            Shown::Failed { error, .. } => failed(ui, &ids, error, &mut act),
        };
        self.field(ui, &ids, below, now);
        self.keyboard(ui, cx, &ids, now);
        if let Some(act) = act {
            ui.request_repaint();
            self.act(act, cx, now);
        }
        // A request whose answer no longer counts (the field emptied) stops, saying nothing.
        if self.request.as_ref().is_some_and(|(q, _)| self.query.asking() != Some(q.as_str())) {
            self.request = None;
        }
        if self.tmdb_request.as_ref().is_some_and(|(w, _)| self.tmdb.asking() != Some(w.as_str())) {
            self.tmdb_request = None;
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        // Back closes the keyboard first, leaving focus on the field.
        if !self.typing {
            return false;
        }
        self.typing = false;
        self.focus_field = true;
        true
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => {
                self.query.refresh(cx.now);
                self.tmdb_stale = true;
            }
            // A title's badge may have changed: asked again when it is among those shown.
            AppEvent::RequestsChanged { title } => {
                if title.as_deref().is_none_or(|t| self.tmdb.holds(t)) {
                    self.tmdb_stale = true;
                }
            }
            AppEvent::LiveChanged => {
                if let Shown::Ready { found, .. } = self.query.shown()
                    && !found.channels.is_empty()
                {
                    self.query.refresh(cx.now);
                }
            }
            AppEvent::Shown => self.query.revalidate(cx.now),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn modal(&self) -> bool {
        self.typing
    }
}

/// A key that brings text as well as its key press (a keyboard's digit row, SDL's text for a
/// remote's number key) types once: its digit key is claimed, so the field and the kit's keyboard
/// take from the keys only the remote's digits that came without text.
fn claim_typed_digits(ui: &mut Ui) {
    let digits: Vec<u8> = ui.typed().chars().filter_map(|c| c.to_digit(10)).map(|d| d as u8).collect();
    for d in digits {
        ui.take_key(Key::Digit(d));
    }
}

/// Seconds since 1970, as the guide counts them.
fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A notice at the gutter, y 120 (`Notice`): the title in `Tv.Title`, 8, the sentence in `Tv.Body`
/// Muted, at most 560 wide and `lines` lines (0: as many as it takes). Returns its bottom.
fn notice(ui: &mut Ui, title: &str, body: &str, lines: u16) -> f32 {
    let title = ui.layout(title, &theme::TITLE, &TextOpts::wrap(NOTICE_WIDTH, 2));
    let mut y = NOTICE_TOP + title.size.h;
    ui.draw_text(Point::new(theme::GUTTER, NOTICE_TOP), title, theme::FG);
    if !body.is_empty() {
        y += theme::space::S;
        let body = ui.layout(body, &theme::BODY, &TextOpts::wrap(NOTICE_WIDTH, lines));
        let h = body.size.h;
        ui.draw_text(Point::new(theme::GUTTER, y), body, theme::MUTED);
        y += h;
    }
    y
}

/// "Search failed" (or "Can't reach your server") with what went wrong (the server's words, cut
/// short), and Retry under it: 8, a 12 dp spacer, 8 (the notice's column spacing), then the pill.
/// Down from the field goes to it.
fn failed(ui: &mut Ui, ids: &Ids, error: &ApiError, act: &mut Option<Act>) -> Option<Id> {
    let title = if error.unreachable() { CANT_REACH } else { FAILED };
    let problem: String = error.problem().chars().take(PROBLEM_CHARS).collect();
    let bottom = notice(ui, title, &problem, PROBLEM_LINES);
    let at = Point::new(theme::GUTTER, bottom + 2.0 * theme::space::S + theme::space::M);
    let retry = Button::pill(RETRY).icon(Icon::RotateCcw);
    let r = ui.group(ids.retry_row, bar_group(), |ui| retry.show(ui, ids.retry, at));
    if r.activated {
        *act = Some(Act::Retry);
    }
    Some(ids.retry)
}

/// Two still skeleton rows while the first answer comes (no shimmer on TV): a 140 × 22 header, 14,
/// then seven posters with their caption bars, the rows 28 apart.
fn skeletons(ui: &mut Ui) {
    let header = Size::new(140.0, 22.0);
    let kind = TileKind::Poster;
    let height = header.h + theme::HEADER_TO_TILES + kind.size().h + theme::TILES_TO_CAPTION + theme::CAPTION_HEIGHT;
    for i in 0..2 {
        let y = ROWS_TOP + i as f32 * (height + theme::ROW_GAP);
        skeleton_row(ui, y, header, theme::HEADER_TO_TILES, kind, 7);
    }
}

/// "Recent searches" and a chip for each (the history icon, at most 320 wide, 12 apart), then
/// Clear; the chips move along to keep the focused one in view. Returns what Down from the field
/// goes to: the chip focused last while it is still there, else the first.
fn recents(ui: &mut Ui, ids: &Ids, recents: &[String], offset: &mut f32, act: &mut Option<Act>) -> Option<Id> {
    let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    ui.text(Point::new(theme::GUTTER, ROWS_TOP), RECENT, &theme::SECTION, &TextOpts::line(width), theme::FG);
    let y = ROWS_TOP + theme::SECTION.line_height + RECENT_GAP + CHIP_ROOM;
    let chips: Vec<Button> =
        recents.iter().map(|q| Button::chip(q).icon(Icon::History).max_label_width(CHIP_LABEL_WIDTH)).collect();
    let clear = Button::text(CLEAR);
    let mut spans = Vec::with_capacity(chips.len() + 1);
    let mut x = theme::GUTTER;
    for w in chips.iter().map(|c| c.size(ui).w).chain([clear.size(ui).w]) {
        spans.push((x, w));
        x += w + CHIP_GAP;
    }
    let focused = ui.focused();
    let at = recents
        .iter()
        .position(|q| focused == Some(ids.chip(q)))
        .or_else(|| (focused == Some(ids.clear)).then_some(recents.len()));
    if let Some(i) = at {
        // As little as brings the focused chip in whole, between the gutter and the safe area.
        let (x, w) = spans[i];
        let (lo, hi) = (theme::GUTTER, theme::CANVAS_W - theme::SAFE_X);
        if x + w - *offset > hi {
            *offset = x + w - hi;
        }
        if x - *offset < lo {
            *offset = x - lo;
        }
    }
    let shown = ui.animate(ids.chips.with("kit.scroll"), *offset, SCROLL);
    let group = Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop);
    ui.group(ids.chips, group, |ui| {
        for ((q, chip), (x, _)) in recents.iter().zip(&chips).zip(&spans) {
            if chip.show(ui, ids.chip(q), Point::new(x - shown, y)).activated {
                *act = Some(Act::Recent(q.clone()));
            }
        }
        let (x, _) = spans[recents.len()];
        if clear.show(ui, ids.clear, Point::new(x - shown, y)).activated {
            *act = Some(Act::Clear);
        }
    });
    let first = recents.first().map(|q| ids.chip(q)).unwrap_or(ids.clear);
    let there = |id: &Id| *id == ids.clear || recents.iter().any(|q| ids.chip(q) == *id);
    Some(ui.remembered(ids.chips).filter(there).unwrap_or(first))
}

/// The results as rows from y 96, each header moving up to that line while its row has focus,
/// then the Discover row (with nothing in the libraries, under the line saying so). Returns what
/// Down from the field goes to: the first row, at the tile it had last while that is still found,
/// else at its first.
fn results(
    ui: &mut Ui,
    ids: &Ids,
    found: Option<&Found>,
    discover: Option<&Discover>,
    view: &mut View,
    act: &mut Option<Act>,
) -> Option<Id> {
    let groups = found.map_or_else(Vec::new, Found::groups);
    let mut sections: Vec<Section> =
        groups.iter().map(|g| Section::new(g.index() as u64, g.tile().row_height(true), Some(ROWS_TOP))).collect();
    if let Some(d) = discover {
        // Under the line, where it is, so that the line stays in sight.
        let anchor = if groups.is_empty() {
            sections.push(Section::new(NONE_KEY, NONE_HEIGHT, Some(ROWS_TOP)));
            ROWS_TOP + NONE_HEIGHT + theme::ROW_GAP
        } else {
            ROWS_TOP
        };
        sections.push(Section::new(TMDB_KEY, d.height(), Some(anchor)));
    }
    let now = unix_now();
    if found.is_some_and(|f| !f.channels.is_empty()) {
        ui.repaint_after(NOW_TICK);
    }
    let (rows, tmdb_row) = (&mut view.rows, &mut view.tmdb);
    RowColumn::new(ROWS_TOP).clip_above(Some(ROWS_CLIP)).show(
        ui,
        ids.column,
        &mut view.column,
        &sections,
        |ui, i, top| match (sections[i].key, found, discover) {
            (NONE_KEY, ..) => {
                let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
                let at = Point::new(theme::GUTTER, top);
                ui.text(at, NONE_IN_LIBRARIES, &theme::BODY, &TextOpts::line(width), theme::MUTED);
            }
            (TMDB_KEY, _, Some(d)) => discover_row(ui, ids, tmdb_row, top, d, act),
            (_, Some(found), _) => {
                let group = groups[i];
                let (id, state) = (ids.row(group), &mut rows[group.index()]);
                match group {
                    ResultGroup::Live => channel_row(ui, id, state, top, &found.channels, now, act),
                    _ => tile_row(ui, id, state, top, group, found.tiles(group), act),
                }
            }
            _ => {}
        },
    );
    let Some(found) = found.filter(|_| !groups.is_empty()) else {
        return discover.and_then(|d| d.down_to(ui, ids));
    };
    let first = *groups.first()?;
    let row = ids.row(first);
    let tile = |key: i64| row.with(key as u64);
    let last = ui.remembered(row);
    let (key, there) = match first {
        ResultGroup::Live => {
            let channels = &found.channels;
            (channels.first()?.channel.id, channels.iter().any(|c| last == Some(tile(c.channel.id))))
        }
        _ => {
            let tiles = found.tiles(first);
            (tiles.first()?.id, tiles.iter().any(|t| last == Some(tile(t.id))))
        }
    };
    Some(last.filter(|_| there).unwrap_or(tile(key)))
}

/// Whether the Discover row shows: requests are open to the profile, and this sign-in may ask.
fn asks_tmdb(cx: &AppCx) -> bool {
    let s = cx.state.request_summary();
    may_ask(s) && s.is_none_or(|s| s.sign_in_again != Some(SignInAgain::Requests))
}

/// The Discover row after the libraries' rows, for the words of the results shown.
struct Discover<'a> {
    words: String,
    /// `None` before TMDB was asked for the words: grey wells, as while it answers.
    state: Option<&'a TmdbState>,
    /// The sign-in manages requests: why requests can't be used says what to do about it.
    manage: bool,
}

impl<'a> Discover<'a> {
    /// The row for the results of `query`, unless the profile may not ask or TMDB has nothing.
    fn of(asks: bool, tmdb: &'a OnTmdb, query: &str, cx: &AppCx) -> Option<Discover<'a>> {
        let words = search_text(query);
        let state = tmdb.shown(&words);
        (asks && !words.is_empty() && crate::tmdb::shows(state)).then(|| Discover {
            words,
            state,
            manage: cx.state.request_summary().is_some_and(|s| s.manage),
        })
    }

    fn height(&self) -> f32 {
        match self.state {
            Some(TmdbState::Failed(_)) => tmdb_problem_height(),
            _ => TileKind::Poster.row_height(true),
        }
    }

    /// What Down from the field goes to when the row comes first: its tile focused last while
    /// that is still there, else its first; Retry; nothing while it comes.
    fn down_to(&self, ui: &Ui, ids: &Ids) -> Option<Id> {
        match self.state {
            Some(TmdbState::Ready(tiles)) => {
                let last = ui.remembered(ids.tmdb_row);
                let see_all = ids.tmdb_row.with(u64::MAX);
                let there = |id: &Id| *id == see_all || tiles.iter().any(|t| ids.tmdb_row.with(t.key()) == *id);
                let first = tiles.first().map_or(see_all, |t| ids.tmdb_row.with(t.key()));
                Some(last.filter(there).unwrap_or(first))
            }
            Some(TmdbState::Failed(e)) => tmdb_problem(e, self.manage).2.then_some(ids.tmdb_retry),
            _ => None,
        }
    }
}

/// Why TMDB's search didn't answer, in a line and a sentence, and whether Retry may help: what
/// requests say of it, else the server's words.
fn tmdb_problem(e: &ApiError, manage: bool) -> (String, String, bool) {
    match problem_words(&requests_problem(e), manage) {
        Some(w) => (w.title.into(), w.body.into(), w.retry),
        None => {
            let title = if e.unreachable() { CANT_REACH } else { TMDB_FAILED };
            (title.into(), e.problem().chars().take(PROBLEM_CHARS).collect(), true)
        }
    }
}

/// The Discover row: TMDB's titles badged, and See all for everything TMDB has for the words;
/// grey wells while they come; why TMDB didn't answer, with Retry where it may help.
fn discover_row(ui: &mut Ui, ids: &Ids, state: &mut RowState, top: f32, d: &Discover, act: &mut Option<Act>) {
    let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    match d.state {
        Some(TmdbState::Ready(tiles)) => {
            let kind = TileKind::Poster;
            let r = Row::new(kind, top).header(DISCOVER).long_press(false).show(
                ui,
                ids.tmdb_row,
                state,
                tiles.len() + 1,
                |i| tiles.get(i).map_or(u64::MAX, TmdbTile::key),
                |ui, slot| match tiles.get(slot.index) {
                    Some(t) => paint_tmdb_tile(ui, slot, t),
                    None => Tile::see_all(kind, SEE_ALL).paint(ui, slot),
                },
            );
            if let Some(i) = r.activated {
                let route = match tiles.get(i) {
                    Some(t) => t.opens(),
                    None => Route::DiscoverSearch { query: d.words.clone() },
                };
                *act = Some(Act::Open(route));
            }
        }
        Some(TmdbState::Failed(e)) => {
            ui.text(Point::new(theme::GUTTER, top), DISCOVER, &theme::SECTION, &TextOpts::line(width), theme::FG);
            let (title, body, retry) = tmdb_problem(e, d.manage);
            let mut y = top + theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES;
            ui.text(Point::new(theme::GUTTER, y), &title, &theme::TITLE, &TextOpts::line(width), theme::FG);
            y += theme::TITLE.line_height + 4.0;
            ui.text(Point::new(theme::GUTTER, y), &body, &theme::BODY, &TextOpts::wrap(width, 2), theme::MUTED);
            y += 2.0 * theme::BODY.line_height + 16.0;
            if retry {
                let pill = Button::pill(RETRY).icon(Icon::RefreshCw);
                let at = Point::new(theme::GUTTER, y);
                let r = ui.group(ids.tmdb_retry_row, bar_group(), |ui| pill.show(ui, ids.tmdb_retry, at));
                if r.activated {
                    *act = Some(Act::RetryTmdb);
                }
            }
        }
        _ => {
            ui.text(Point::new(theme::GUTTER, top), DISCOVER, &theme::SECTION, &TextOpts::line(width), theme::FG);
            let y = top + theme::ROW_HEADER_HEIGHT;
            skeleton_row(ui, y, Size::new(0.0, 0.0), theme::HEADER_TO_TILES, TileKind::Poster, TMDB_SKELETONS);
        }
    }
}

/// A TMDB title's poster, with its badge at the upper left in the badge's colour.
fn paint_tmdb_tile(ui: &mut Ui, slot: &TileSlot, t: &TmdbTile) {
    let tile = Tile::new(TileKind::Poster, &t.title).subtitle(&t.subtitle).art(&t.art);
    let Some(badge) = t.badge.filter(|_| slot.mode == SlotMode::Draw) else {
        tile.paint(ui, slot);
        return;
    };
    let priority = slot.priority;
    let well = |ui: &mut Ui, rect: Rect| {
        let r = TileKind::Poster.radius();
        let shown = first_art(ui, &t.art, priority);
        if shown.is_some_and(|s| s.opaque()) {
            ui.fill(rect, theme::SURFACE2, r);
        } else {
            placeholder(ui, rect, &t.title, &t.title, r);
        }
        if let Some(s) = shown {
            ui.draw_image(rect, &s, &ImageOpts::cover().rounded(r));
        }
        ui.stroke(rect.expand(-0.5), theme::TILE_HAIRLINE, radius::TILE - 0.5, 1.0);
        title_badge(ui, rect, badge, t.tone);
    };
    tile.custom(&well).paint(ui, slot);
}

/// A row of people or titles: Select opens one; on titles, OK held or the menu key opens its
/// actions (people have none).
fn tile_row(
    ui: &mut Ui,
    id: Id,
    state: &mut RowState,
    top: f32,
    group: ResultGroup,
    tiles: &[ResultTile],
    act: &mut Option<Act>,
) {
    let kind = group.tile();
    let people = group == ResultGroup::People;
    let row = Row::new(kind, top).header(group.title()).long_press(!people);
    let r = row.show(
        ui,
        id,
        state,
        tiles.len(),
        |i| tiles[i].id as u64,
        |ui, slot| {
            let t = &tiles[slot.index];
            Tile::new(kind, &t.title)
                .subtitle(&t.subtitle)
                .art(&t.art)
                .progress(t.progress)
                .watched(t.watched)
                .unwatched(t.unwatched)
                .paint(ui, slot);
        },
    );
    if let Some(i) = r.activated {
        *act = Some(Act::Open(tiles[i].opens.clone()));
    } else if let Some(i) = r.actions.filter(|_| !people) {
        *act = Some(Act::Actions(tiles[i].id));
    }
}

/// The Live TV channels found, as Live TV's channel tiles; Select plays one.
fn channel_row(
    ui: &mut Ui,
    id: Id,
    state: &mut RowState,
    top: f32,
    channels: &[ChannelResult],
    now: i64,
    act: &mut Option<Act>,
) {
    let row = Row::new(TileKind::Episode, top).header(ResultGroup::Live.title()).long_press(false);
    let r = row.show(
        ui,
        id,
        state,
        channels.len(),
        |i| channels[i].channel.id as u64,
        |ui, slot| {
            channel_tile(ui, slot, &channels[slot.index], now);
        },
    );
    if let Some(i) = r.activated {
        *act = Some(Act::Watch(Box::new(channels[i].channel.clone())));
    }
}

/// A channel's tile (`ChannelTileCard`, `TvLiveTvScreen.kt`): the logo fitted 8 dp inside a
/// Surface3 well (without one, or when it fails, the name on its gradient; while it loads, the well
/// alone), the number as a badge at the top left, a star at the top right for a favourite, what's
/// on's progress along the foot; captioned with the name over what's on.
fn channel_tile(ui: &mut Ui, slot: &TileSlot, found: &ChannelResult, now: i64) {
    if slot.mode == SlotMode::Prefetch {
        if let Some(logo) = &found.logo {
            ui.image_state(logo, slot.priority);
        }
        return;
    }
    let channel = &found.channel;
    let progress = channel_progress(channel, now);
    let priority = slot.priority;
    let well = |ui: &mut Ui, rect: Rect| {
        let shown = found.logo.as_ref().map(|logo| ui.image_state(logo, priority));
        if shown.is_none_or(|s| matches!(s.state, ImageState::Failed)) {
            placeholder(ui, rect, &channel.name, &channel.name, radius::TILE);
        } else {
            ui.fill(rect, theme::SURFACE3, radius::TILE);
            if let Some(s) = shown {
                ui.draw_image(rect.expand(-LOGO_INSET), &s, &ImageOpts::contain());
            }
        }
        if let Some(n) = channel.number.as_deref().filter(|n| !n.is_empty()) {
            Badge::new(n, BadgeStyle::Number).show(ui, Point::new(rect.x + MARK_INSET, rect.y + MARK_INSET));
        }
        if channel.favorite {
            let star = Rect::new(rect.right() - MARK_INSET - STAR_SIZE, rect.y + MARK_INSET, STAR_SIZE, STAR_SIZE);
            ui.icon(star, Icon::Star, theme::ACCENT);
        }
        if let Some(p) = progress {
            ProgressBar::on_art().show(ui, Rect::new(rect.x, rect.bottom() - PROGRAM_LINE, rect.w, PROGRAM_LINE), p);
        }
    };
    Tile::new(TileKind::Episode, &channel.name).subtitle(channel_caption(channel, now)).custom(&well).paint(ui, slot);
}
