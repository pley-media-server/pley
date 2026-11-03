//! Home (Android's `TvHome.kt` and `TvHomeParts.kt`; the TV app's `home.tsx`): the hero for the
//! first title of Continue Watching (its backdrop, title, metadata line, summary, Resume and the
//! round buttons), the server's rows under it, and over the rows the info block of the tile focus
//! rests on while the backdrop crossfades to its art. It draws at once from what was saved (the
//! app's state reads the saved Home before any network) and follows the server's answer; until
//! there is something, it is blank for 300 ms and then a still skeleton.
//!
//! Focus: Resume first. Down goes to the first row, Up from it back to the hero, which comes back
//! on the button it last had. Back in the rows brings the hero back with focus on Resume and opens
//! the rail, whose Right lands on Resume again. Once focus has rested 400 ms on a tile, the info
//! block and the backdrop follow it, its details come (and the next two tiles'), and the server
//! is asked to read ahead what Play would play. A tile its actions took out of its row (Remove
//! from Continue Watching) leaves focus to its neighbour.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use pley_base::{Color, Icon, ImageRequest, Point, Rect, Size, TextOpts, TextStyle};
use pley_core::http::sentence;
use pley_core::model::{Hub, ItemSummary, Library, LibraryKind, LibrarySort};
use pley_core::{ApiError, Promise};
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, BarItem, Button, ColumnState, InfoBlock, Message,
    MessageStyle, MetaLine, RoundButton, Row, RowColumn, RowState, Section, Tile, TileKind, TileSlot, backdrop_px,
    button_bar, prefetch_backdrops, skeleton_block,
};
use pley_shell::{AppCx, AppEvent, Chrome, ERROR_TOAST_SECONDS, LibraryTab, PlayRequest, Route, Screen, TOAST_SECONDS};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, Motion, Response, Ui};

use crate::ahead::{Ahead, Wish, answer_of};
use crate::libraries::collect_orders;
use crate::model::{
    ArtKind, HomeInfo, HomeLook, Play, SKELETON_AFTER, TileText, art_paths, hero_of, home_info, look_of, marked,
    page_of, play_of, row_kind, see_all_of, stand_in, tile_text,
};

/// The hero's height: the first row's header comes 28 below it, at y 384 (`TvHome.kt`
/// `HeroHeight`), and its wide tiles show whole under it.
pub const HERO_HEIGHT: f32 = 356.0;
/// Where the hero's column starts.
const HERO_TOP: f32 = 52.0;
/// The hero's title sits at the bottom of a box this tall (two lines grow it upward).
const HERO_TITLE_BOX: f32 = 72.0;
/// The text column over backdrops: x 96 to 560.
const TEXT_COLUMN: f32 = 464.0;
/// The info block's title sits at the bottom of a box this tall.
const INFO_TITLE_BOX: f32 = 52.0;
/// The metadata line's height.
const META_LINE: f32 = 18.0;
/// The words of a row's last tile when it lists more.
const SEE_ALL: &str = "See all";
/// What Play says of a show with nothing left to play (`tv_home_nothing_to_play`).
const NOTHING_TO_PLAY: &str = "Nothing to play";
/// The hero's section among the rows.
const HERO_SECTION: u64 = 0;
/// A row's See all tile, among its items' ids.
const SEE_ALL_KEY: u64 = u64::MAX;
/// Continue Watching titles whose details are asked for once Home shows, ahead of a press (it
/// also warms the server's keyframe index for them; `HomeViewModel.PREFETCHED`).
const WARM_CONTINUE_WATCHING: usize = 3;
/// Tiles after the one focus rests on whose details are asked for ahead.
const DETAILS_AHEAD: usize = 2;
/// Frames Back waits for Resume to take focus before the rail opens.
const RAIL_AFTER_FRAMES: u8 = 3;
/// The toast's widest, its padding included.
const TOAST_WIDTH: f32 = 420.0;

/// A tile of a row, worked out once when the rows change: what it says and the pictures it tries.
struct TileModel {
    item: ItemSummary,
    text: TileText,
    art: Vec<ImageRequest>,
    backdrop: BackdropArt,
}

/// A row of Home.
struct RowModel {
    /// Its section among the rows, from the server's key (`continue_watching`, `recently_added:1`).
    section: u64,
    title: String,
    kind: TileKind,
    continue_watching: bool,
    see_all: Option<(i64, LibrarySort)>,
    tiles: Vec<TileModel>,
}

impl RowModel {
    fn len(&self) -> usize {
        self.tiles.len() + usize::from(self.see_all.is_some())
    }

    fn key_of(&self, i: usize) -> u64 {
        self.tiles.get(i).map_or(SEE_ALL_KEY, |t| t.item.id as u64)
    }
}

/// Home as drawn: the rows and the hero, rebuilt only when what they come from changes.
#[derive(Default)]
struct Model {
    /// The server's rows it was built from, without the marks: an answer that says the same
    /// changes nothing.
    hubs: Vec<Hub>,
    /// The music libraries it was built with (their rows' tiles are square).
    music: Vec<i64>,
    rows: Vec<RowModel>,
    sections: Vec<Section>,
    hero: Option<TileModel>,
    /// Changes at every build.
    version: u64,
}

/// What the model was built from: the answers by address and length (a new answer is a new
/// allocation while the old one lives), the marks set here, the server and the screen's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Source {
    generation: u64,
    hubs: Option<(usize, usize)>,
    libraries: Option<(usize, usize)>,
    marks: u64,
    scale: u32,
}

/// A watched mark set on the hero, shown before the server's rows say so.
struct Mark {
    id: i64,
    watched: bool,
    /// On its way; `None` once the server took it (the mark goes with the next rows).
    request: Option<Promise<Result<(), ApiError>>>,
}

/// A tile, by its row's section and its item.
type TileAt = (u64, i64);

/// The episode a show's Play plays, on its way: tied to the press, it is dropped when Home is left.
struct OnDeck {
    answer: Promise<Result<Option<ItemSummary>, ApiError>>,
    from_start: bool,
    /// When Play was pressed, for the time to the first picture.
    pressed: f64,
}

/// A tile whose actions opened: if they take it out of its row, focus goes to its stand-in.
struct Leaving {
    tile: TileAt,
    stand_in: Option<TileAt>,
}

/// Focus going back to the hero while the rows don't draw it: Back (Resume, then the rail) or the
/// pointer (the button it last had).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToHero {
    Back,
    Pointer,
}

/// What a press asked for, carried out once the frame is drawn.
enum Intent {
    Open(Route),
    Actions { at: TileAt, continue_watching: bool },
    Play { item: ItemSummary, from_start: bool },
    Watched { item: ItemSummary, watched: bool },
}

/// What the hero or the info block says, worked out again only when its title, the rows or the
/// facts change.
#[derive(Default)]
struct InfoCache {
    key: Option<(i64, u64, u64)>,
    info: HomeInfo,
}

impl InfoCache {
    fn of(&mut self, item: &ItemSummary, ahead: &Ahead, model: u64) -> &HomeInfo {
        let key = (item.id, model, ahead.version());
        if self.key != Some(key) {
            self.info = home_info(item, ahead.get(item.id));
            self.key = Some(key);
        }
        &self.info
    }
}

/// Home's toast (`TvHomeToast`): at the foot of the page from the gutter, Surface3 with 6 dp
/// corners, its words in `Tv.Label` and Fg whatever they say; 3.5 s, a failure 6 s.
#[derive(Default)]
struct HomeToast {
    text: String,
    at: f64,
    failure: bool,
    live: bool,
}

impl HomeToast {
    fn show(&mut self, text: impl Into<String>, failure: bool, at: f64) {
        *self = HomeToast { text: text.into(), at, failure, live: true };
    }

    fn draw(&mut self, ui: &mut Ui) {
        if !self.live {
            return;
        }
        let age = ui.time() - self.at;
        let duration = if self.failure { ERROR_TOAST_SECONDS } else { TOAST_SECONDS };
        if age >= duration {
            self.live = false;
            return;
        }
        let fade = Motion::REVEAL.duration as f64;
        let alpha = if age < fade {
            Motion::REVEAL.progress(age)
        } else if age > duration - fade {
            ((duration - age) / fade) as f32
        } else {
            1.0
        };
        if age < fade || age > duration - fade {
            ui.request_repaint();
        } else {
            ui.repaint_after(duration - fade - age);
        }
        let layout = ui.layout(&self.text, &theme::LABEL, &TextOpts::wrap(TOAST_WIDTH - 40.0, 3));
        let size = Size::new(layout.size.w + 40.0, layout.size.h + 20.0);
        let at = Point::new(theme::GUTTER, theme::CANVAS_H - theme::SAFE_Y - size.h);
        ui.opacity(alpha.clamp(0.0, 1.0), |ui| {
            ui.fill(Rect::from_origin_size(at, size), theme::SURFACE3, radius::CONTROL);
            ui.draw_text(Point::new(at.x + 20.0, at.y + 10.0), layout, theme::FG);
        });
    }
}

/// Home.
pub struct HomeScreen {
    model: Model,
    source: Option<Source>,
    /// Compare the answers themselves at the next frame (the page was away while they changed).
    recheck: bool,
    column: ColumnState,
    rows: HashMap<u64, RowState>,
    backdrop: BackdropState,
    /// Focus is in the rows (the info block shows), not on the hero.
    in_rows: bool,
    /// The tile with focus at the last frame.
    focused: Option<TileAt>,
    /// The tile focus last rested on.
    rested: Option<TileAt>,
    leaving: Option<Leaving>,
    /// The hero's title was read ahead since focus came to its buttons.
    hero_read: bool,
    ahead: Ahead,
    hero_info: InfoCache,
    rested_info: InfoCache,
    toast: HomeToast,
    opened: Option<f64>,
    warmed: bool,
    /// Focus goes back to the hero at the next frame.
    to_hero: Option<ToHero>,
    /// Frames left to wait for Resume to have focus before the rail opens.
    rail_after: u8,
    marks: Vec<Mark>,
    marks_version: u64,
    on_deck: Option<OnDeck>,
    /// The look focus was sent to the rail for (a look with nothing to focus).
    rail_for: Option<HomeLook>,
    look: HomeLook,
    last_pointer: Option<Point>,
}

impl Default for HomeScreen {
    fn default() -> HomeScreen {
        HomeScreen::new()
    }
}

impl HomeScreen {
    /// Home, before its first frame.
    pub fn new() -> HomeScreen {
        HomeScreen {
            model: Model::default(),
            source: None,
            recheck: false,
            column: ColumnState::default(),
            rows: HashMap::new(),
            backdrop: BackdropState::default(),
            in_rows: false,
            focused: None,
            rested: None,
            leaving: None,
            hero_read: false,
            ahead: Ahead::default(),
            hero_info: InfoCache::default(),
            rested_info: InfoCache::default(),
            toast: HomeToast::default(),
            opened: None,
            warmed: false,
            to_hero: None,
            rail_after: 0,
            marks: Vec::new(),
            marks_version: 0,
            on_deck: None,
            rail_for: None,
            look: HomeLook::Blank,
            last_pointer: None,
        }
    }

    /// What the page shows now.
    pub fn look(&self) -> HomeLook {
        self.look
    }

    /// Whether focus is in the rows (rather than on the hero).
    pub fn in_rows(&self) -> bool {
        self.in_rows
    }

    /// The focus id of the hero's Resume (or Play) button on the page `page` (the shell's group
    /// of the page).
    pub fn resume_id(page: Id) -> Id {
        hero_id(page).with("play")
    }

    /// The focus id of item `item`'s tile in the row of the server's key `row` on the page `page`.
    pub fn tile_id(page: Id, row: &str, item: i64) -> Id {
        tile_focus(page, (section_key(row), item))
    }

    /// Collects the screen's own answers: details, Play's next episode, marks, library orders.
    fn poll(&mut self, cx: &mut AppCx) {
        self.ahead.poll(cx);
        collect_orders(cx);
        if let Some(deck) = self.on_deck.as_mut()
            && let Some(answer) = answer_of(&mut deck.answer)
        {
            let (from_start, pressed) = (deck.from_start, deck.pressed);
            self.on_deck = None;
            match answer {
                Ok(Some(next)) => {
                    cx.play(PlayRequest { from_start, pressed_at: pressed, ..PlayRequest::item(next.id) })
                }
                Ok(None) => self.toast.show(NOTHING_TO_PLAY, false, cx.now),
                Err(e) if e.cancelled() => {}
                Err(e) => self.toast.show(sentence(&e.to_string()), true, cx.now),
            }
        }
        let mut refused = false;
        let mut taken = false;
        let now = cx.now;
        for mark in &mut self.marks {
            let Some(p) = mark.request.as_mut() else { continue };
            let Some(answer) = answer_of(p) else { continue };
            mark.request = None;
            match answer {
                Ok(()) => taken = true,
                Err(e) => {
                    // Put back as the server has it.
                    mark.id = -1;
                    refused = true;
                    if !e.cancelled() {
                        self.toast.show(sentence(&e.to_string()), true, now);
                    }
                }
            }
        }
        if refused {
            self.marks.retain(|m| m.id >= 0);
            self.marks_version += 1;
        }
        if taken {
            // Home and every page listing the title ask again.
            cx.state.refresh();
        }
    }

    /// Rebuilds the rows when the answers they come from changed.
    fn sync(&mut self, ui: &Ui, cx: &mut AppCx) {
        let hubs = cx.state.home();
        let libraries = cx.state.libraries();
        let source = Source {
            generation: cx.state.generation(),
            hubs: hubs.map(|h| (h.as_ptr() as usize, h.len())),
            libraries: libraries.map(|l| (l.as_ptr() as usize, l.len())),
            marks: self.marks_version,
            scale: ui.scale().to_bits(),
        };
        if self.source == Some(source) && !std::mem::take(&mut self.recheck) {
            return;
        }
        if self.source.is_none_or(|s| s.hubs != source.hubs || s.generation != source.generation) {
            // New rows from the server: the marks it took are in them.
            let before = self.marks.len();
            self.marks.retain(|m| m.request.is_some());
            if self.marks.len() != before {
                self.marks_version += 1;
            }
        }
        let source = Source { marks: self.marks_version, ..source };
        let hubs = cx.state.home().unwrap_or(&[]);
        let music = music_ids(cx.state.libraries().unwrap_or(&[]));
        // An answer that says what the rows say already (asked again on the way back, a
        // revalidation) leaves them as they are.
        let same = self
            .source
            .is_some_and(|s| s.generation == source.generation && s.marks == source.marks && s.scale == source.scale)
            && self.model.hubs.as_slice() == hubs
            && self.model.music == music;
        self.source = Some(source);
        if !same {
            self.model = build(hubs, music, &self.marks, ui, cx, self.model.version + 1);
        }
    }

    fn rows_ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let page = ui.scope_id();
        let column_id = rows_id(page);
        let hero_id = hero_id(page);
        let resume_id = hero_id.with("play");
        if self.model.hero.is_none() {
            return;
        }

        // Where focus is, as the last frame left it (on the rail it is neither).
        let in_hero = ui.focused_in(hero_id).is_some();
        if in_hero {
            self.in_rows = false;
        } else if ui.focused_in(column_id).is_some() {
            self.in_rows = true;
        }
        ui.initial_focus(resume_id);
        if self.rail_after > 0 {
            if ui.is_focused(resume_id) {
                // Back from the rows, step two: the hero shows with Resume focused; the rail opens,
                // and its Right comes back to Resume.
                self.rail_after = 0;
                cx.to_rail();
            } else {
                self.rail_after -= 1;
            }
        }
        self.settle_leaving(ui, page);

        // The tile focus rests on: the info block and the backdrop follow it, and what it would
        // play is asked for ahead.
        let rested_now = ui.rested();
        if self.in_rows && rested_now && self.focused != self.rested {
            self.rested = self.focused;
            self.on_rest(cx);
        }
        if !in_hero {
            self.hero_read = false;
        } else if rested_now
            && !std::mem::replace(&mut self.hero_read, true)
            && let Some(hero) = &self.model.hero
        {
            // Focus rests on the hero's buttons: the server gets its title ready to play.
            self.ahead.read_ahead(&hero.item, cx);
        }

        let Some(hero) = self.model.hero.as_ref() else { return };
        let hero_info = self.hero_info.of(&hero.item, &self.ahead, self.model.version);
        let rested = if self.in_rows { self.rested.and_then(|at| tile_at(&self.model.rows, at)) } else { None };
        let shown = rested.map_or(hero, |(row, i)| &row.tiles[i]);
        let scrim = if self.in_rows { BackdropScrim::Rows } else { BackdropScrim::Hero };
        Backdrop::new(scrim).show(ui, page.with("backdrop"), &mut self.backdrop, Some(&shown.backdrop));
        if self.in_rows
            && !ui.holding()
            && let Some((row, i)) = self.focused.and_then(|at| tile_at(&self.model.rows, at))
        {
            // The focused tile's backdrop and its neighbours' load now: focus may rest on any of
            // them next.
            let art = |i: usize| row.tiles.get(i).map_or(&[][..], |t| t.backdrop.art.as_slice());
            prefetch_backdrops(ui, &[art(i), art(i + 1), i.checked_sub(1).map_or(&[][..], art)]);
        }
        let info = match rested {
            Some(_) => self.rested_info.of(&shown.item, &self.ahead, self.model.version),
            None => hero_info,
        };
        InfoBlock::show(ui, page.with("info"), self.in_rows, |ui| draw_info(ui, info));

        // The pointer moving up into the hero's place, hidden while focus is in the rows, brings it
        // back on the button it last had.
        let pointer = ui.pointer();
        let pointer_moved = pointer.is_some() && pointer != self.last_pointer;
        self.last_pointer = pointer;
        if self.in_rows {
            let line = theme::FOCUSED_ROW_HEADER_Y - theme::ROWS_CLIP_ABOVE;
            let zone = Rect::new(theme::RAIL_COLLAPSED, 0.0, theme::CANVAS_W - theme::RAIL_COLLAPSED, line);
            if ui.clickable(page.with("hero-zone"), zone).hovered && pointer_moved {
                // The rows draw the hero's buttons while the first row has focus; further down,
                // they are brought back as Back does.
                if self.column.section <= 1 {
                    ui.request_focus(hero_id);
                } else {
                    self.to_hero = Some(ToHero::Pointer);
                }
            }
        }

        if let Some(to) = self.to_hero.take() {
            // Back from the rows, step one: the page goes to its top with the hero (far down, at
            // once). This pass registers the hero's buttons alone, unseen, so they take focus:
            // drawn, the rows would put the page back where focus was.
            self.column.section = 0;
            self.column.shift = 0.0;
            self.in_rows = false;
            ui.clip(Rect::ZERO, |ui| draw_hero(ui, hero, hero_info, 0.0, 0.0, hero_id, &mut Vec::new()));
            match to {
                ToHero::Back => {
                    self.rail_after = RAIL_AFTER_FRAMES;
                    ui.request_focus(resume_id);
                }
                ToHero::Pointer => ui.request_focus(hero_id),
            }
            ui.request_repaint();
            return;
        }

        let fade = ui.animate(hero_id.with("fade"), if self.in_rows { 0.0 } else { 1.0 }, Motion::REVEAL);
        let mut intents = Vec::new();
        let mut focused = None;
        let model = &self.model;
        let row_states = &mut self.rows;
        RowColumn::new(0.0).show(ui, column_id, &mut self.column, &model.sections, |ui, i, top| {
            if i == 0 {
                draw_hero(ui, hero, hero_info, fade, top, hero_id, &mut intents);
                return;
            }
            let Some(row) = model.rows.get(i - 1) else { return };
            let state = row_states.entry(row.section).or_default();
            let r = Row::new(row.kind, top).header(&row.title).show(
                ui,
                ui.id("row"),
                state,
                row.len(),
                |i| row.key_of(i),
                |ui, slot| paint_tile(ui, row, slot),
            );
            // See all names no title: the info block stays on the tile before it.
            if let Some(t) = r.focused.and_then(|f| row.tiles.get(f)) {
                focused = Some((row.section, t.item.id));
            }
            if let Some(a) = r.activated {
                if let Some(t) = row.tiles.get(a) {
                    intents.push(Intent::Open(page_of(&t.item)));
                } else if let Some((library, sort)) = row.see_all {
                    let tab = Some(LibraryTab::Library);
                    intents.push(Intent::Open(Route::Library { id: library, tab, sort: Some(sort) }));
                }
            }
            if let Some(t) = r.actions.and_then(|a| row.tiles.get(a)) {
                let at = (row.section, t.item.id);
                intents.push(Intent::Actions { at, continue_watching: row.continue_watching });
            }
        });
        if focused.is_some() {
            self.focused = focused;
        }

        if !self.warmed {
            self.warmed = true;
            for row in self.model.rows.iter().filter(|r| r.continue_watching) {
                for t in row.tiles.iter().take(WARM_CONTINUE_WATCHING) {
                    self.ahead.want(t.item.id, Wish::Ahead, cx);
                }
            }
        }
        self.carry_out(intents, cx);
    }

    /// Focus came to rest on a tile: its details and the next two's, and the server's read-ahead.
    fn on_rest(&mut self, cx: &mut AppCx) {
        let Some((row, i)) = self.rested.and_then(|at| tile_at(&self.model.rows, at)) else { return };
        let item = &row.tiles[i].item;
        self.ahead.read_ahead(item, cx);
        self.ahead.want(item.id, Wish::Rested, cx);
        for t in row.tiles.iter().skip(i + 1).take(DETAILS_AHEAD) {
            self.ahead.want(t.item.id, Wish::Ahead, cx);
        }
    }

    /// A tile whose actions opened: once it is out of its row, its stand-in takes focus; once focus
    /// is elsewhere, nothing more to do.
    fn settle_leaving(&mut self, ui: &mut Ui, page: Id) {
        let Some(leaving) = &self.leaving else { return };
        if tile_at(&self.model.rows, leaving.tile).is_none() {
            match leaving.stand_in.filter(|s| tile_at(&self.model.rows, *s).is_some()) {
                Some(at) => ui.request_focus(tile_focus(page, at)),
                None => ui.request_focus(hero_id(page)),
            }
            self.leaving = None;
        } else if self.focused != Some(leaving.tile) {
            self.leaving = None;
        }
    }

    fn carry_out(&mut self, intents: Vec<Intent>, cx: &mut AppCx) {
        for intent in intents {
            match intent {
                Intent::Open(route) => {
                    // The person moved on: an episode Play is still waiting for won't start.
                    self.on_deck = None;
                    cx.push(route);
                }
                Intent::Actions { at, continue_watching } => {
                    let ids: Vec<(u64, Vec<i64>)> = self
                        .model
                        .rows
                        .iter()
                        .map(|r| (r.section, r.tiles.iter().map(|t| t.item.id).collect()))
                        .collect();
                    self.on_deck = None;
                    cx.tile_actions(at.1, continue_watching);
                    self.leaving = Some(Leaving { tile: at, stand_in: stand_in(&ids, at.0, at.1) });
                }
                Intent::Play { item, from_start } => match play_of(&item, from_start) {
                    Play::Now(request) => cx.play(request),
                    Play::OnDeck(id) => {
                        if self.on_deck.is_none() {
                            let answer = cx.spawn(move |api| api.on_deck(id));
                            self.on_deck = Some(OnDeck { answer, from_start, pressed: cx.now });
                        }
                    }
                    Play::Open(route) => cx.push(route),
                },
                Intent::Watched { item, watched } => {
                    let id = item.id;
                    self.marks.retain(|m| m.id != id);
                    let request = cx.spawn(move |api| api.set_watched(id, watched));
                    self.marks.push(Mark { id, watched, request: Some(request) });
                    self.marks_version += 1;
                }
            }
        }
    }

    /// Home's notices: what it can't show, with Retry when asking again may help.
    fn notice(&mut self, ui: &mut Ui, cx: &mut AppCx, look: HomeLook) {
        let (title, body, retry) = match look {
            HomeLook::Failed => {
                let error = cx.state.home_error();
                let title = match error {
                    Some(e) if e.unreachable() => "Can't reach your server",
                    _ => "Couldn't load your home screen",
                };
                (title, error.map(ApiError::problem).unwrap_or_default(), true)
            }
            HomeLook::NoLibraries => ("No libraries yet", "Ask the server owner to add a library.".to_string(), false),
            HomeLook::Scanning => {
                ("Scanning your libraries…", "New items will show up here as soon as they're found.".to_string(), false)
            }
            _ => (
                "Nothing here yet",
                "Your libraries are empty. Add media to the library folders and run a scan.".to_string(),
                false,
            ),
        };
        let mut message = Message::new(MessageStyle::Page, title).body(&body);
        if retry {
            message = message.action("Retry", Icon::RefreshCw);
        }
        let r = message.show(ui, ui.id("message"));
        if r.action.is_some_and(|a| a.activated) {
            cx.state.load_home();
            cx.state.load_account();
        }
    }
}

/// The hero's group of buttons on the page `page`.
fn hero_id(page: Id) -> Id {
    page.with("hero")
}

/// The rows' column on the page `page`.
fn rows_id(page: Id) -> Id {
    page.with("rows")
}

/// The focus id of the tile `at` on the page `page`.
fn tile_focus(page: Id, at: TileAt) -> Id {
    rows_id(page).with(at.0).with("row").with(at.1 as u64)
}

/// The tile `at` in `rows` as they are now: its row and its place in it.
fn tile_at(rows: &[RowModel], at: TileAt) -> Option<(&RowModel, usize)> {
    let row = rows.iter().find(|r| r.section == at.0)?;
    let i = row.tiles.iter().position(|t| t.item.id == at.1)?;
    Some((row, i))
}

/// A stable key for a row's section, never the hero's.
fn section_key(key: &str) -> u64 {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    h.finish().max(HERO_SECTION + 1)
}

/// The music libraries' ids.
fn music_ids(libraries: &[Library]) -> Vec<i64> {
    libraries.iter().filter(|l| l.kind == LibraryKind::Music).map(|l| l.id).collect()
}

/// Works out the rows and the hero from the server's rows, with the marks set here applied.
fn build(hubs: &[Hub], music: Vec<i64>, marks: &[Mark], ui: &Ui, cx: &AppCx, version: u64) -> Model {
    let item_of = |item: &ItemSummary| match marks.iter().find(|m| m.id == item.id) {
        Some(m) => marked(item, m.watched),
        None => item.clone(),
    };
    let backdrop_px = backdrop_px(ui);
    let pictures = |item: &ItemSummary, kind: ArtKind, px: u32| -> Vec<ImageRequest> {
        art_paths(item, kind).iter().filter_map(|p| cx.image(p, px)).collect()
    };
    let tile = |item: ItemSummary, kind: TileKind| {
        let backdrop = pictures(&item, ArtKind::Backdrop, backdrop_px);
        TileModel {
            text: tile_text(&item, kind),
            art: pictures(&item, ArtKind::of_tile(kind), ui.image_px(kind.size().w)),
            backdrop: BackdropArt::new(item.id as u64, backdrop, item.title.clone()),
            item,
        }
    };
    let rows: Vec<RowModel> = hubs
        .iter()
        .filter(|h| !h.items.is_empty())
        .map(|hub| {
            let kind = row_kind(hub, &music);
            RowModel {
                section: section_key(&hub.key),
                title: hub.title.clone(),
                kind,
                continue_watching: hub.continue_watching(),
                see_all: see_all_of(hub),
                tiles: hub.items.iter().map(|i| tile(item_of(i), kind)).collect(),
            }
        })
        .collect();
    let hero = hero_of(hubs).map(|item| tile(item_of(item), TileKind::Landscape));
    let mut sections = Vec::with_capacity(rows.len() + 1);
    if hero.is_some() {
        sections.push(Section::hero(HERO_SECTION, HERO_HEIGHT));
        sections.extend(rows.iter().map(|r| Section::row(r.section, r.kind.row_height(true))));
    }
    Model { hubs: hubs.to_vec(), music, rows, sections, hero, version }
}

/// Draws a tile of `row` in its slot (or asks for its picture ahead).
fn paint_tile(ui: &mut Ui, row: &RowModel, slot: &TileSlot) {
    match row.tiles.get(slot.index) {
        Some(t) => Tile::new(row.kind, &t.text.title)
            .subtitle(&t.text.subtitle)
            .art(&t.art)
            .progress(t.text.progress)
            .unwatched(t.text.unwatched)
            .watched(t.text.watched)
            .paint(ui, slot),
        None => Tile::see_all(row.kind, SEE_ALL).paint(ui, slot),
    }
}

/// The metadata line of `info` (`TvMetaLine`): the lead, the year, the content rating, the
/// runtime, the rating after its star and the quality badges.
fn meta_line<'a>(info: &'a HomeInfo, lead: &'a str) -> MetaLine<'a> {
    MetaLine::new()
        .lead(lead)
        .text(&info.year)
        .content_rating(&info.content_rating)
        .text(&info.runtime)
        .stars(&info.rating)
        .quality(&info.badges)
}

/// The info block over the rows (`TvInfoBlock`): from y 28, the title at the bottom of a 52 dp box,
/// the metadata line 10 below (an episode's number first), two lines of summary 8 below.
fn draw_info(ui: &mut Ui, info: &HomeInfo) {
    let r = InfoBlock::RECT;
    let title = ui.layout(&info.title, &theme::PAGE_TITLE, &TextOpts::line(r.w));
    let h = title.size.h;
    ui.draw_text(Point::new(r.x, r.y + INFO_TITLE_BOX - h), title, theme::FG);
    let mut y = r.y + INFO_TITLE_BOX + 10.0;
    let line = meta_line(info, &info.context);
    if !line.is_empty() {
        line.show(ui, Point::new(r.x, y), r.w);
        y += META_LINE + 8.0;
    }
    if !info.summary.is_empty() {
        ui.text(Point::new(r.x, y), &info.summary, &theme::BODY, &TextOpts::wrap(r.w, 2), theme::SUMMARY);
    }
}

/// Draws the hero with its top at `top` (`TvHomeHero`): at x 96 from 52 down, the title at the
/// bottom of a 72 dp box (an episode's show's), the episode's "S2 · E5 · …" 10 below, the
/// metadata line 8 below, the time left 10 below while in progress, three lines of summary 10
/// below, then 20 below Resume (or Play) and the round buttons From start, Info and Watched. A
/// missing piece closes its gap. `fade` is how far it shows: at 0 only its buttons are there, for
/// focus to come back to.
fn draw_hero(ui: &mut Ui, hero: &TileModel, info: &HomeInfo, fade: f32, top: f32, id: Id, out: &mut Vec<Intent>) {
    let item = &hero.item;
    let x = theme::GUTTER;
    let y = if fade > 0.0 {
        ui.opacity(fade, |ui| hero_column(ui, info, top + HERO_TOP, true))
    } else {
        hero_column(ui, info, top + HERO_TOP, false)
    };
    let play = if info.in_progress { "Resume" } else { "Play" };
    let watched = if info.watched { "Mark as unwatched" } else { "Mark as watched" };
    let mut items: Vec<(&str, BarItem)> = Vec::with_capacity(4);
    items.push(("play", BarItem::Button(Button::pill(play).icon(Icon::Play))));
    if info.in_progress {
        items.push(("start", BarItem::Round(RoundButton::new(Icon::RotateCcw, "From start"))));
    }
    items.push(("info", BarItem::Round(RoundButton::new(Icon::Info, "Info"))));
    items.push(("watched", BarItem::Round(RoundButton::new(Icon::CircleCheck, watched))));
    let responses: Vec<Response> = ui.opacity(fade.max(0.0), |ui| button_bar(ui, id, Point::new(x, y), &items));
    for ((key, _), r) in items.iter().zip(&responses) {
        if !r.activated {
            continue;
        }
        out.push(match *key {
            "play" => Intent::Play { item: item.clone(), from_start: false },
            "start" => Intent::Play { item: item.clone(), from_start: true },
            "info" => Intent::Open(page_of(item)),
            _ => Intent::Watched { item: item.clone(), watched: !info.watched },
        });
    }
}

/// The hero's text column from `y` down, drawn or only measured; where its buttons go.
fn hero_column(ui: &mut Ui, info: &HomeInfo, mut y: f32, draw: bool) -> f32 {
    let x = theme::GUTTER;
    let text = |ui: &mut Ui, y: f32, s: &str, style: &TextStyle, opts: TextOpts, color: Color| {
        if draw { ui.text(Point::new(x, y), s, style, &opts, color).h } else { ui.measure(s, style, &opts).h }
    };
    if draw {
        let opts = TextOpts { balance: true, ..TextOpts::wrap(TEXT_COLUMN, 2) };
        let title = ui.layout(&info.title, &theme::HERO_TITLE, &opts);
        let h = title.size.h;
        ui.draw_text(Point::new(x, y + HERO_TITLE_BOX - h), title, theme::FG);
    }
    y += HERO_TITLE_BOX;
    if !info.context.is_empty() {
        y += 10.0;
        y += text(ui, y, &info.context, &theme::TITLE, TextOpts::line(TEXT_COLUMN), theme::FG);
    }
    let line = meta_line(info, "");
    if !line.is_empty() {
        y += 8.0;
        if draw {
            line.show(ui, Point::new(x, y), TEXT_COLUMN);
        }
        y += META_LINE;
    }
    if info.in_progress && !info.time_left.is_empty() {
        y += 10.0;
        y += text(ui, y, &info.time_left, &theme::CAPTION, TextOpts::line(TEXT_COLUMN), theme::FG);
    }
    if !info.summary.is_empty() {
        y += 10.0;
        y += text(ui, y, &info.summary, &theme::BODY_LARGE, TextOpts::wrap(TEXT_COLUMN, 3), theme::SUMMARY);
    }
    y + 20.0
}

/// Home while it loads (after 300 ms): still Surface2 blocks where the hero and a wide row go
/// (`TvHomeSkeleton`).
fn draw_skeleton(ui: &mut Ui) {
    let x = theme::GUTTER;
    let bar = radius::BADGE;
    skeleton_block(ui, Rect::new(x, 84.0, 260.0, 38.0), bar);
    skeleton_block(ui, Rect::new(x, 132.0, 210.0, 18.0), bar);
    skeleton_block(ui, Rect::new(x, 168.0, 440.0, 15.0), bar);
    skeleton_block(ui, Rect::new(x, 190.0, 400.0, 15.0), bar);
    skeleton_block(ui, Rect::new(x, 212.0, 260.0, 15.0), bar);
    skeleton_block(ui, Rect::new(x, 250.0, 128.0, theme::BUTTON_HEIGHT), radius::CONTROL);
    skeleton_block(ui, Rect::new(x, 387.0, 180.0, 17.0), bar);
    let tile = TileKind::Landscape;
    for i in 0..4 {
        let at = Rect::new(x + i as f32 * tile.pitch(), 422.0, tile.size().w, tile.size().h);
        skeleton_block(ui, at, radius::TILE);
    }
}

impl Screen for HomeScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let now = ui.time();
        let opened = *self.opened.get_or_insert(now);
        self.poll(cx);
        self.sync(ui, cx);
        if let Some(hero) = &self.model.hero {
            self.ahead.want(hero.item.id, Wish::Hero, cx);
        }
        let scanning = !cx.state.scans().is_empty();
        let failed = cx.state.home_error().is_some();
        let rows = self.model.rows.len();
        let mut look = look_of(cx.state.home(), rows, cx.state.libraries(), failed, scanning, now - opened);
        if look == HomeLook::Rows
            && now - opened < SKELETON_AFTER
            && self.model.hero.as_ref().is_some_and(|h| !self.ahead.settled(h.item.id))
        {
            // The page is blank a few ms more while the hero's saved details are read, so its
            // buttons don't move down when its summary comes.
            look = HomeLook::Blank;
        }
        self.look = look;
        match look {
            HomeLook::Blank => ui.repaint_after(SKELETON_AFTER - (now - opened)),
            HomeLook::Skeleton => draw_skeleton(ui),
            HomeLook::Rows => self.rows_ui(ui, cx),
            _ => self.notice(ui, cx, look),
        }
        // A look with nothing to focus sends focus to the rail, once.
        if matches!(look, HomeLook::NoLibraries | HomeLook::Scanning | HomeLook::Empty) {
            if self.rail_for != Some(look) {
                self.rail_for = Some(look);
                cx.to_rail();
            }
        } else {
            self.rail_for = None;
        }
        self.toast.draw(ui);
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        // Back from the rows: the hero again with Resume focused, then the rail (`TvHome.kt`'s
        // BackHandler).
        if self.look == HomeLook::Rows && self.in_rows && self.model.hero.is_some() {
            self.to_hero = Some(ToHero::Back);
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => {
                self.ahead.forget();
                self.recheck = true;
            }
            AppEvent::Shown => {
                // Back on top (from a title's page, the player): what is older than 30 s is asked
                // again, and an episode Play was waiting for when Home was left won't start now.
                cx.state.revalidate();
                self.recheck = true;
                self.on_deck = None;
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        matches!(self.look, HomeLook::Rows | HomeLook::Failed)
    }
}
