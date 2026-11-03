//! The libraries list (Android's `TvLibraries.kt`; the TV app's `LibrariesScreen` in
//! `library.tsx`): the rail's "Libraries" (more than three libraries) and Settings' "Sort
//! libraries". A card a library, three to a row, in the profile's order: its kind's icon, its
//! name, how much it holds and "Scanning…" while the server scans it. When the rail has no room
//! for Playlists, a card for it follows the libraries' (not while sorting them), and this page is
//! the way there.
//!
//! With two or more, the profile may put them in its own order, which every list of them follows
//! on all its devices. Sort libraries (or Move in a card's actions) starts sorting: OK lifts a
//! card, the arrows move it a place along or a row (the wheel a row, the pointer where it points),
//! OK drops it there and the order is saved; Back puts it back, and Back or Done ends sorting.
//! "Use the server's order" goes back to the order everyone sees. Changes go to the server one at
//! a time, in the order they were made, whether or not the page is still open.

use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use pley_base::{Icon, Key, Point, Rect, Size, TextOpts};
use pley_core::ApiError;
use pley_core::http::sentence;
use pley_core::model::Library;
use pley_kit::look::{shade, spoken};
use pley_kit::{Button, Heading, ListRow, Message, MessageStyle, SCROLL, SidePanel, bar_group};
use pley_shell::{AppCx, AppEvent, Chrome, RailEntries, Route, Screen};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{Dirs, FocusOpts, Group, Id, Motion, Ui, nudge};

use crate::model::{grid_step, in_order, kind_icon, library_count_line, moved};

/// Cards a row.
pub const COLUMNS: usize = 3;
/// A card's size: three across from x 96 to the safe edge.
pub const CARD: Size = Size::new((theme::CANVAS_W - theme::GUTTER - theme::SAFE_X - 2.0 * CARD_GAP) / 3.0, 80.0);
/// Between cards, both ways.
pub const CARD_GAP: f32 = 16.0;
/// Inside a card, either side.
const CARD_PAD: f32 = 18.0;
/// The kind's icon sits in a well this big.
const WELL: f32 = 40.0;
/// From the header to the grid, and the grid's own top padding (`TvLibraries.kt`).
const HEADER_TO_GRID: f32 = 20.0;
const GRID_TOP: f32 = 8.0;
/// While sorting the hint takes the bottom: the view ends this much higher.
const HINT_ROOM: f32 = 80.0;
/// Cards sliding to their new places.
const SLIDE: Motion = Motion::new(0.200, pley_ui::Easing::FAST_OUT_SLOW_IN);

/// What a server said about sorting.
pub const SORT_NEEDS_NEWER: &str = "Sorting libraries needs a newer Pley on the server.";
const SORT_REFUSED: &str = "This server doesn't let the app sort your libraries: sort them on the web";
const SORT_RACED: &str = "Your libraries changed meanwhile: try again";
const HINT_SORTING: &str = "Select a library to move it. Your order applies on all your devices.";
const HINT_MOVING: &str = "Arrows to move it, OK to drop it here, Back to put it back";

/// Whether the libraries can be put in another order here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sorting {
    Yes,
    /// Fewer than two libraries.
    No,
    /// The server is too old for a profile's own order (it answered 404 or 405).
    Older,
    /// The sign-in may not change things (no `play` scope), or the server said no (403).
    Refused,
}

/// Servers that said this launch they can't sort the libraries, and why.
static NO_SORT: Mutex<Vec<(String, Sorting)>> = Mutex::new(Vec::new());

/// Whether the libraries of the server in use can be sorted here (`librarySorting`).
pub fn sorting_here(cx: &AppCx) -> Sorting {
    if cx.state.libraries().map_or(0, <[Library]>::len) < 2 {
        return Sorting::No;
    }
    if !cx.state.tokens().is_some_and(|t| t.allows("play")) {
        return Sorting::Refused;
    }
    let server = cx.state.server().unwrap_or_default();
    let no = NO_SORT.lock().unwrap_or_else(|e| e.into_inner());
    no.iter().find(|(s, _)| s == server).map_or(Sorting::Yes, |(_, why)| *why)
}

fn refuse_sorting(server: &str, why: Sorting) {
    let mut no = NO_SORT.lock().unwrap_or_else(|e| e.into_inner());
    no.retain(|(s, _)| s != server);
    no.push((server.to_string(), why));
}

/// A change of the libraries' order (`None`: back to the server's), for the server and the
/// sign-in and profile (the app state's generation) it was made in.
struct Order {
    server: String,
    generation: u64,
    ids: Option<Vec<i64>>,
}

/// The order changes on their way, for the whole app rather than the page that made them, as the
/// TV app keeps them in its state (`state.ts` `orderLibraries`): leaving the page doesn't stop
/// them, and their answers wait here until Home or the libraries list takes them.
struct Orders {
    waiting: VecDeque<Order>,
    /// The servers a task is sending changes to.
    sending: Vec<String>,
    answered: Vec<(Order, Result<Vec<Library>, ApiError>)>,
}

static ORDERS: Mutex<Orders> =
    Mutex::new(Orders { waiting: VecDeque::new(), sending: Vec::new(), answered: Vec::new() });

fn orders() -> MutexGuard<'static, Orders> {
    ORDERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sends the profile's order `ids` (`None`: the server's) after the changes made before it.
fn send_order(ids: Option<Vec<i64>>, cx: &AppCx) {
    let Some(server) = cx.state.server() else { return };
    orders().waiting.push_back(Order { server: server.to_string(), generation: cx.state.generation(), ids });
    start_sending(cx);
}

/// Starts the task that sends the changes waiting for the server in use, one at a time, unless one
/// runs. It stops at a change made in another sign-in or profile.
fn start_sending(cx: &AppCx) {
    let (Some(api), Some(server)) = (cx.api().cloned(), cx.state.server()) else { return };
    let (server, generation) = (server.to_string(), cx.state.generation());
    {
        let mut o = orders();
        if o.sending.contains(&server) || !o.waiting.iter().any(|w| w.server == server) {
            return;
        }
        o.sending.push(server.clone());
    }
    let waker = cx.tasks().waker().clone();
    cx.tasks().detach(move || {
        loop {
            let order = {
                let mut o = orders();
                let next = o.waiting.iter().position(|w| w.server == server && w.generation == generation);
                match next.and_then(|i| o.waiting.remove(i)) {
                    Some(order) => order,
                    None => {
                        o.sending.retain(|s| *s != server);
                        return;
                    }
                }
            };
            let answer = match &order.ids {
                Some(ids) => api.sort_libraries(ids),
                None => api.forget_library_order(),
            };
            orders().answered.push((order, answer));
            waker.wake();
        }
    });
}

/// Takes the answers to the order changes of the server in use: the libraries in their new order
/// (and Home asked again), or why not. Each change and whether the server took it.
pub(crate) fn collect_orders(cx: &mut AppCx) -> Vec<(Option<Vec<i64>>, bool)> {
    let generation = cx.state.generation();
    let (answered, restart) = {
        let Some(server) = cx.state.server() else { return Vec::new() };
        let mut o = orders();
        if !o.answered.iter().any(|(a, _)| a.server == server) && !o.waiting.iter().any(|w| w.server == server) {
            return Vec::new();
        }
        // Changes made in another sign-in or profile were that one's.
        o.waiting.retain(|w| w.server != server || w.generation == generation);
        let (mine, others): (Vec<_>, Vec<_>) =
            std::mem::take(&mut o.answered).into_iter().partition(|(a, _)| a.server == server);
        o.answered = others;
        let restart = !o.sending.iter().any(|s| s == server) && o.waiting.iter().any(|w| w.server == server);
        (mine, restart)
    };
    if restart {
        start_sending(cx);
    }
    let mut taken = Vec::new();
    for (order, answer) in answered {
        if order.generation != generation {
            continue;
        }
        match answer {
            Ok(libraries) => {
                // The server's answer lists them so now.
                cx.state.keep_libraries(libraries);
                cx.state.load_home();
                taken.push((order.ids, true));
            }
            Err(e) => {
                refused(&e, cx);
                taken.push((order.ids, false));
            }
        }
    }
    taken
}

/// Says why the server didn't take an order, and remembers when it can't sort them at all.
fn refused(e: &ApiError, cx: &mut AppCx) {
    let server = cx.state.server().unwrap_or_default().to_string();
    if e.is(404) || e.is(405) {
        refuse_sorting(&server, Sorting::Older);
        cx.toast_error(SORT_NEEDS_NEWER);
    } else if e.is(403) {
        refuse_sorting(&server, Sorting::Refused);
        cx.toast_error(SORT_REFUSED);
    } else if e.is(409) {
        // A library came or went since they were listed.
        cx.state.load_account();
        cx.toast_error(SORT_RACED);
    } else if !e.cancelled() {
        cx.toast_error(sentence(&e.to_string()));
    }
}

/// Where the card at `i` sits in the grid's content (its top-left, from the content's top).
pub fn card_place(i: usize) -> Point {
    let (col, row) = (i % COLUMNS, i / COLUMNS);
    Point::new(theme::GUTTER + col as f32 * (CARD.w + CARD_GAP), GRID_TOP + row as f32 * (CARD.h + CARD_GAP))
}

/// The grid's content height for `count` cards, with the safe area below the last row.
fn content_height(count: usize) -> f32 {
    let rows = count.div_ceil(COLUMNS);
    GRID_TOP + rows as f32 * (CARD.h + CARD_GAP) - CARD_GAP + theme::SAFE_Y
}

/// How far the grid moves up to show card `i` with keys: its row second from the top.
pub fn shift_for(i: usize, count: usize, view: f32) -> f32 {
    let top = card_place(i).y;
    let max = (content_height(count) - view).max(0.0);
    (top - GRID_TOP - (CARD.h + CARD_GAP)).clamp(0.0, max)
}

/// A library as its card shows it.
struct Card {
    library: Library,
    /// "412 movies", "38 shows · Scanning…".
    line: String,
}

/// The cards, worked out again only when the libraries, their order here or the scans change.
#[derive(Default)]
struct Cards {
    libraries: Vec<Library>,
    order: Option<Vec<i64>>,
    scans: Vec<i64>,
    cards: Rc<[Card]>,
}

/// The libraries list.
pub struct LibrariesScreen {
    /// Opened by Settings to sort: ending sorting goes back there.
    from_settings: bool,
    sorting: bool,
    /// The card lifted, moving with the arrows.
    lifted: Option<i64>,
    /// It has had focus since it was lifted (focus leaving it then puts it back).
    lifted_seen: bool,
    /// The libraries' ids as moved here, until the server's answer lists them so.
    order: Option<Vec<i64>>,
    /// The order before the lifted card was lifted: Back puts it back.
    before: Option<Option<Vec<i64>>>,
    /// The card whose actions show in the side panel.
    actions_for: Option<i64>,
    shift: f32,
    cards: Cards,
    /// A card to focus at the next frame (sorting began, the panel moved it).
    focus_card: Option<i64>,
    rail_sent: bool,
    last_pointer: Option<Point>,
    /// What the cards show, in order, for tests.
    shown: Vec<i64>,
}

impl LibrariesScreen {
    /// The list; `sort`: opened by Settings to sort them.
    pub fn new(sort: bool) -> LibrariesScreen {
        LibrariesScreen {
            from_settings: sort,
            sorting: sort,
            lifted: None,
            lifted_seen: false,
            order: None,
            before: None,
            actions_for: None,
            shift: 0.0,
            cards: Cards::default(),
            focus_card: None,
            rail_sent: false,
            last_pointer: None,
            shown: Vec::new(),
        }
    }

    /// Whether sorting is on.
    pub fn sorting(&self) -> bool {
        self.sorting
    }

    /// The card lifted, if one is.
    pub fn lifted(&self) -> Option<i64> {
        self.lifted
    }

    /// The libraries' ids as the cards showed them at the last frame.
    pub fn shown(&self) -> &[i64] {
        &self.shown
    }

    /// Whether an order is on its way to the server, or its answer not taken yet.
    pub fn saving(&self) -> bool {
        let o = orders();
        !o.waiting.is_empty() || !o.sending.is_empty() || !o.answered.is_empty()
    }

    /// The focus id of the card of library `id` on the page `page`.
    pub fn card_id(page: Id, id: i64) -> Id {
        page.with("cards").with(id)
    }

    /// The focus id of Playlists' card on the page `page`.
    pub fn playlists_id(page: Id) -> Id {
        page.with("cards").with("playlists")
    }

    fn ids_now(&self, cx: &AppCx) -> Vec<i64> {
        in_order(cx.state.libraries().unwrap_or(&[]), self.order.as_deref()).iter().map(|l| l.id).collect()
    }

    fn lift(&mut self, id: i64) {
        self.before = Some(self.order.clone());
        self.lifted = Some(id);
        self.lifted_seen = false;
    }

    fn move_to(&mut self, to: usize, cx: &AppCx) {
        let Some(lifted) = self.lifted else { return };
        let ids = self.ids_now(cx);
        let Some(from) = ids.iter().position(|id| *id == lifted) else { return };
        if to >= ids.len() || to == from {
            return;
        }
        self.order = Some(moved(&ids, from, to));
    }

    fn drop_card(&mut self, cx: &mut AppCx) {
        let was = self.before.take().flatten();
        self.lifted = None;
        let Some(ids) = self.order.clone() else { return };
        let unmoved: Vec<i64> =
            in_order(cx.state.libraries().unwrap_or(&[]), was.as_deref()).iter().map(|l| l.id).collect();
        if ids == unmoved {
            self.order = was;
        } else {
            send_order(Some(ids), cx);
        }
    }

    fn put_back(&mut self) {
        self.lifted = None;
        if let Some(was) = self.before.take() {
            self.order = was;
        }
    }

    fn server_order(&mut self, cx: &mut AppCx) {
        self.lifted = None;
        self.before = None;
        self.order = None;
        send_order(None, cx);
    }

    fn end_sorting(&mut self, cx: &mut AppCx) {
        if self.from_settings {
            cx.pop();
            return;
        }
        self.sorting = false;
        self.put_back();
    }

    /// Takes the answers to order changes: what the server took lists them so now (a newer move
    /// keeps its own); what it refused goes back.
    fn poll(&mut self, cx: &mut AppCx) {
        for (ids, taken) in collect_orders(cx) {
            if self.order == ids {
                self.order = None;
            }
            if !taken && self.before.as_ref() == Some(&ids) {
                self.before = None;
            }
        }
    }

    /// Works the cards out again if what they show changed.
    fn update_cards(&mut self, cx: &AppCx) {
        let libraries = cx.state.libraries().unwrap_or(&[]);
        let scans = cx.state.scans();
        let same = self.cards.libraries.as_slice() == libraries
            && self.cards.order == self.order
            && self.cards.scans.iter().copied().eq(scans.keys().copied());
        if same {
            return;
        }
        let cards: Vec<Card> = in_order(libraries, self.order.as_deref())
            .into_iter()
            .map(|l| {
                let line = library_count_line(&l, l.scanning || scans.contains_key(&l.id));
                Card { library: l, line }
            })
            .collect();
        self.shown = cards.iter().map(|c| c.library.id).collect();
        self.cards = Cards {
            libraries: libraries.to_vec(),
            order: self.order.clone(),
            scans: scans.keys().copied().collect(),
            cards: cards.into(),
        };
    }

    fn page(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let page = ui.scope_id();
        let sortable = sorting_here(cx) == Sorting::Yes;
        if !sortable {
            // Nothing left to sort (a library went, the server said no): the page as it is.
            self.sorting = false;
            self.lifted = None;
        }
        let in_sort = self.sorting && sortable;
        self.update_cards(cx);
        let cards = self.cards.cards.clone();
        if self.lifted.is_some_and(|id| !self.shown.contains(&id)) {
            self.lifted = None;
        }
        let lifted = self.lifted.filter(|_| in_sort);

        // The header: the title, and the pills while the profile may sort.
        let header_h = if sortable { theme::BUTTON_HEIGHT } else { theme::PAGE_TITLE.line_height };
        let title = if in_sort { "Sort libraries" } else { "Libraries" };
        let mut right = theme::CANVAS_W - theme::SAFE_X;
        let mut pressed_sort = false;
        let mut pressed_server = false;
        if sortable {
            let free = lifted.is_none();
            let sort = if in_sort {
                Button::pill("Done").icon(Icon::Check).enabled(free)
            } else {
                Button::pill("Sort libraries").icon(Icon::ArrowUpDown)
            };
            let server = in_sort.then(|| Button::pill("Use the server's order").icon(Icon::RotateCcw).enabled(free));
            let sort_w = sort.size(ui).w;
            let server_w = server.map_or(0.0, |b| b.size(ui).w);
            ui.group(page.with("header"), bar_group(), |ui| {
                let mut x = right - sort_w - server.map_or(0.0, |_| server_w + 12.0);
                if let Some(b) = server {
                    pressed_server = b.show(ui, ui.id("server-order"), Point::new(x, theme::SAFE_Y)).activated;
                    x += server_w + 12.0;
                }
                pressed_sort = sort.show(ui, ui.id("sort"), Point::new(x, theme::SAFE_Y)).activated;
            });
            right -= sort_w + server.map_or(0.0, |_| server_w + 12.0) + 16.0;
        }
        let title_y = theme::SAFE_Y + (header_h - theme::PAGE_TITLE.line_height) / 2.0;
        let title_w = right - theme::GUTTER;
        ui.text(Point::new(theme::GUTTER, title_y), title, &theme::PAGE_TITLE, &TextOpts::line(title_w), theme::FG);

        // The grid of cards under it, Playlists' last when the rail has no room for it.
        let view_top = theme::SAFE_Y + header_h + HEADER_TO_GRID;
        let view_h = theme::CANVAS_H - view_top - if in_sort { HINT_ROOM } else { 0.0 };
        let grid_id = page.with("cards");
        let count = cards.len();
        let playlists = !in_sort && cx.rail().is_some_and(RailEntries::playlists_on_libraries_page);
        let playlists_id = Self::playlists_id(page);
        let tiles = count + usize::from(playlists);
        let max = (content_height(tiles) - view_h).max(0.0);
        if let Some(first) = cards.first() {
            ui.initial_focus(grid_id.with(first.library.id));
        }
        if let Some(id) = self.focus_card.take() {
            ui.request_focus(grid_id.with(id));
        }
        let focused = cards
            .iter()
            .position(|c| ui.is_focused(grid_id.with(c.library.id)))
            .or_else(|| (playlists && ui.is_focused(playlists_id)).then_some(count));
        if let Some(i) = focused {
            let top = card_place(i).y;
            self.shift = if ui.pointer_mode() {
                nudge(self.shift, top, top + CARD.h, 0.0, view_h, 0.0, max)
            } else {
                shift_for(i, tiles, view_h)
            };
        }
        self.shift = self.shift.clamp(0.0, max);
        let shift = ui.animate(page.with("shift"), self.shift, SCROLL);
        // Only the cards near the view are drawn: those in it, and a row more either way for focus
        // to move to.
        let near = |y: f32, margin: f32| y + CARD.h > view_top - margin && y < view_top + view_h + margin;
        let row_h = CARD.h + CARD_GAP;

        // Focus leaving a lifted card puts it back.
        if let Some(id) = lifted {
            let has = ui.is_focused(grid_id.with(id));
            if has {
                self.lifted_seen = true;
            } else if self.lifted_seen && ui.focused().is_some() {
                self.put_back();
            }
        }
        let lifted = self.lifted.filter(|_| in_sort);

        // Keys a lifted card takes: the arrows move it, OK drops it.
        let mut moves: Vec<isize> = Vec::new();
        if lifted.is_some_and(|id| ui.is_focused(grid_id.with(id))) {
            for k in ui.keys().iter().filter(|k| k.pressed) {
                match k.key {
                    Key::Left => moves.push(-1),
                    Key::Right => moves.push(1),
                    Key::Up => moves.push(-(COLUMNS as isize)),
                    Key::Down => moves.push(COLUMNS as isize),
                    _ => {}
                }
            }
            let notches = ui.wheel(grid_id);
            for _ in 0..notches.unsigned_abs() {
                moves.push(if notches > 0 { -(COLUMNS as isize) } else { COLUMNS as isize });
            }
        }

        let mut opened = None;
        let mut actions = None;
        let mut dropped = false;
        let mut lifting = None;
        let mut open_playlists = false;
        let clip = Rect::new(0.0, view_top, theme::CANVAS_W, view_h);
        let group = if lifted.is_some() { Group::grid(COLUMNS as u16).wheel() } else { Group::grid(COLUMNS as u16) };
        ui.clip(clip, |ui| {
            ui.group(grid_id, group, |ui| {
                for (i, card) in cards.iter().enumerate() {
                    let place = card_place(i);
                    if !near(view_top + place.y - shift, row_h) {
                        continue;
                    }
                    let l = &card.library;
                    let id = grid_id.with(l.id);
                    // A card moved to another place slides there; the grid itself scrolls as one.
                    let slot = Rect::new(place.x, place.y, CARD.w, CARD.h);
                    let rect = ui.animate_rect(id.with("place"), slot, SLIDE).translate(0.0, view_top - shift);
                    let is_lifted = lifted == Some(l.id);
                    let face = Face { icon: kind_icon(l.kind), name: &l.name, line: &card.line };
                    let opts = FocusOpts::default().long_press().takes(if is_lifted { Dirs::ALL } else { Dirs::NONE });
                    let r = draw_card(ui, id, rect, &face, opts, in_sort, is_lifted);
                    if r.activated {
                        if is_lifted {
                            dropped = true;
                        } else if lifted.is_none() && in_sort {
                            lifting = Some(l.id);
                        } else if lifted.is_none() {
                            opened = Some(l.id);
                        }
                    }
                    if (r.long_pressed || r.menu) && lifted.is_none() {
                        actions = Some(l.id);
                    }
                }
                let place = card_place(count);
                if playlists && near(view_top + place.y - shift, row_h) {
                    // Playlists' card, without actions of its own: OK, held or not, opens it.
                    let slot = Rect::new(place.x, place.y, CARD.w, CARD.h);
                    let rect =
                        ui.animate_rect(playlists_id.with("place"), slot, SLIDE).translate(0.0, view_top - shift);
                    let face = Face { icon: Icon::ListMusic, name: "Playlists", line: "" };
                    let r = draw_card(ui, playlists_id, rect, &face, FocusOpts::default(), false, false);
                    open_playlists = r.activated;
                }
            });
            if lifted.is_some() {
                // Over the grid while a card is lifted: the pointer moves it where it points, its
                // click drops it there.
                let pointer = ui.pointer();
                let moved_pointer = pointer.is_some() && pointer != self.last_pointer;
                let rows = count.div_ceil(COLUMNS);
                for i in 0..rows * COLUMNS {
                    let place = card_place(i);
                    let y = view_top + place.y - shift;
                    if !near(y, 0.0) {
                        continue;
                    }
                    let at = Rect::new(place.x, y, CARD.w, CARD.h).expand(CARD_GAP / 2.0);
                    let r = ui.clickable(page.with("target").with(i), at);
                    if r.hovered && moved_pointer {
                        moves.clear();
                        self.move_to(i.min(count.saturating_sub(1)), cx);
                    }
                    if r.clicked {
                        dropped = true;
                    }
                }
            }
        });
        self.last_pointer = ui.pointer();

        for by in moves {
            let ids = self.ids_now(cx);
            if let Some(from) = lifted.and_then(|id| ids.iter().position(|x| *x == id))
                && let Some(to) = grid_step(from, by, ids.len(), COLUMNS)
            {
                self.move_to(to, cx);
            }
        }
        if dropped {
            self.drop_card(cx);
        }
        if let Some(id) = lifting {
            self.lift(id);
        }
        if let Some(id) = opened {
            cx.push(Route::Library { id, tab: None, sort: None });
        }
        if open_playlists {
            cx.push(Route::Playlists);
        }
        if pressed_sort {
            if in_sort {
                self.end_sorting(cx);
            } else {
                // The button goes: the first library takes focus, ready to lift.
                self.sorting = true;
                self.focus_card = cards.first().map(|c| c.library.id);
            }
        }
        if pressed_server {
            self.server_order(cx);
        }

        if in_sort {
            let hint = if lifted.is_some() { HINT_MOVING } else { HINT_SORTING };
            draw_hint(ui, hint);
        }

        // A card's actions: Open, and while they can be sorted, Move and the server's order.
        if let Some(id) = actions {
            self.actions_for = Some(id);
        }
        let panel_card = self.actions_for.and_then(|id| cards.iter().position(|c| c.library.id == id));
        let open = panel_card.is_some();
        let mut chosen = None;
        let panel = SidePanel::new().back_closes(false).show(ui, page.with("actions"), open, |ui, area| {
            let Some(i) = panel_card else { return };
            let card = &cards[i];
            let mut y = area.y;
            y += Heading::PanelTitle.show(ui, &card.library.name, Point::new(area.x - 16.0, y), area.w + 16.0);
            let caption = ui.text(
                Point::new(area.x, y - 4.0),
                &card.line,
                &theme::CAPTION,
                &TextOpts::line(area.w),
                theme::MUTED,
            );
            y += caption.h + 12.0;
            let mut rows: Vec<(&str, Icon)> = vec![("Open", Icon::ArrowRight)];
            if sortable {
                rows.push(("Move", Icon::ArrowUpDown));
                rows.push(("Use the server's order", Icon::RotateCcw));
            }
            for (k, (label, icon)) in rows.iter().enumerate() {
                let rid = ui.id(k);
                if k == 0 {
                    ui.initial_focus(rid);
                }
                let row = ListRow::new(label).icon(*icon);
                let h = row.height(ui, area.w);
                if row.show(ui, rid, Point::new(area.x, y), area.w).activated {
                    chosen = Some(k);
                }
                y += h + 2.0;
            }
        });
        if panel.closed {
            self.actions_for = None;
        }
        if let (Some(k), Some(i)) = (chosen, panel_card) {
            let id = cards[i].library.id;
            self.actions_for = None;
            match k {
                0 => cx.push(Route::Library { id, tab: None, sort: None }),
                1 => {
                    self.sorting = true;
                    self.lift(id);
                    self.focus_card = Some(id);
                }
                _ => {
                    self.server_order(cx);
                    self.focus_card = Some(id);
                }
            }
        }
    }
}

/// What a card shows: the icon in its well, its name, and the line under it ("412 movies"; none
/// on Playlists' card).
struct Face<'a> {
    icon: Icon,
    name: &'a str,
    line: &'a str,
}

/// A library's card (`LibraryCard`), and Playlists' like it: Surface2 with 6 dp corners; focused,
/// the Accent fill with OnAccent content at 1.05. Inside, 18 in: a 40 dp well (the content colour
/// at 12 %) with the icon, 14, then the name (`Tv.Title`) over "412 movies" (`Tv.Caption`, 75 %),
/// or the name alone in the middle; while sorting a grip at its end. Lifted it turns Surface3
/// with a 3 dp AccentBright ring inside its edge and keeps its size; it then takes the arrows.
/// Audio Guidance reads the name, then the line.
fn draw_card(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    face: &Face,
    opts: FocusOpts,
    sorting: bool,
    lifted: bool,
) -> pley_ui::Response {
    ui.name(id, || spoken(&[face.name, face.line]));
    let r = ui.focusable_with(id, rect, opts);
    if r.activated || r.long_pressed || r.menu {
        // What the press changes shows at the next frame.
        ui.request_repaint();
    }
    let (fill, content, scale) = if lifted {
        (theme::SURFACE3, theme::FG, 1.0)
    } else {
        let s = shade(ui, &focus::LIBRARY_CARD, &r);
        (s.fill, s.content, s.scale)
    };
    ui.transform(Point::ZERO, scale, rect.center(), |ui| {
        ui.fill(rect, fill, radius::CARD);
        if lifted {
            let w = focus::RING_STROKE;
            ui.stroke(rect.expand(-w / 2.0), theme::ACCENT_BRIGHT, radius::CARD - w / 2.0, w);
        }
        let well = Rect::new(rect.x + CARD_PAD, rect.y + (rect.h - WELL) / 2.0, WELL, WELL);
        ui.fill(well, content.alpha(theme::ICON_WELL_ALPHA), radius::CONTROL);
        let icon = Size::new(theme::ICON_SIZE, theme::ICON_SIZE);
        ui.icon(well.place(icon, 0.5, 0.5), face.icon, content);
        let x = well.right() + 14.0;
        let grip = if sorting { 12.0 + theme::ICON_SIZE } else { 0.0 };
        let w = (rect.right() - CARD_PAD - grip - x).max(1.0);
        let has_line = !face.line.is_empty();
        let block = theme::TITLE.line_height + if has_line { 2.0 + theme::CAPTION.line_height } else { 0.0 };
        let y = rect.y + (rect.h - block) / 2.0;
        ui.text(Point::new(x, y), face.name, &theme::TITLE, &TextOpts::line(w), content);
        if has_line {
            let sub = content.alpha(theme::CARD_SUBLINE_ALPHA);
            let at = Point::new(x, y + theme::TITLE.line_height + 2.0);
            ui.text(at, face.line, &theme::CAPTION, &TextOpts::line(w), sub);
        }
        if sorting {
            let at = Rect::new(rect.right() - CARD_PAD - theme::ICON_SIZE, rect.y, theme::ICON_SIZE, rect.h);
            let tint = if lifted { theme::ACCENT_BRIGHT } else { content.alpha(0.6) };
            ui.icon(at.place(icon, 0.5, 0.5), Icon::GripVertical, tint);
        }
    });
    r
}

/// What the remote does while sorting, at the bottom centre (`SortHint`): `Tv.Label` on Surface3,
/// 20 × 12 of padding.
fn draw_hint(ui: &mut Ui, text: &str) {
    let layout = ui.layout(text, &theme::LABEL, &TextOpts::line(theme::CANVAS_W - 2.0 * theme::GUTTER));
    let size = Size::new(layout.size.w + 40.0, layout.size.h + 24.0);
    let at = Point::new((theme::CANVAS_W - size.w) / 2.0, theme::CANVAS_H - theme::SAFE_Y - size.h);
    ui.fill(Rect::from_origin_size(at, size), theme::SURFACE3, radius::CONTROL);
    ui.draw_text(Point::new(at.x + 20.0, at.y + 12.0), layout, theme::FG);
}

impl Screen for LibrariesScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.poll(cx);
        match cx.state.libraries() {
            Some([]) => {
                Message::new(MessageStyle::Page, "No libraries yet")
                    .body("Ask the server owner to add a library.")
                    .show(ui, ui.id("message"));
                if !std::mem::replace(&mut self.rail_sent, true) {
                    cx.to_rail();
                }
            }
            Some(_) => {
                self.rail_sent = false;
                self.page(ui, cx);
            }
            None => {
                if cx.state.libraries().is_none() && cx.state.account_error().is_none() {
                    // On their way: blank.
                    cx.state.load_account();
                    return;
                }
                let error = cx.state.account_error();
                let title = match error {
                    Some(e) if e.unreachable() => "Can't reach your server",
                    _ => "Couldn't load your libraries",
                };
                let body = error.map(ApiError::problem).unwrap_or_default();
                let r = Message::new(MessageStyle::Page, title)
                    .body(&body)
                    .action("Retry", Icon::RefreshCw)
                    .show(ui, ui.id("message"));
                if r.action.is_some_and(|a| a.activated) {
                    cx.state.load_account();
                }
            }
        }
    }

    fn back(&mut self, cx: &mut AppCx) -> bool {
        if self.actions_for.take().is_some() {
            return true;
        }
        if self.lifted.is_some() {
            self.put_back();
            return true;
        }
        if self.sorting && !self.from_settings && sorting_here(cx) == Sorting::Yes {
            self.end_sorting(cx);
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        if matches!(event, AppEvent::Shown) {
            cx.state.revalidate();
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn modal(&self) -> bool {
        self.actions_for.is_some() || self.lifted.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_sit_three_to_a_row_from_the_gutter() {
        let pitch = CARD.w + CARD_GAP;
        assert_eq!(card_place(0), Point::new(96.0, 8.0));
        assert_eq!(card_place(4), Point::new(96.0 + pitch, 8.0 + 96.0));
        // About 261 wide, the third ending at the right safe edge.
        assert!((CARD.w - 261.33).abs() < 0.01);
        assert!((card_place(2).x + CARD.w - (theme::CANVAS_W - theme::SAFE_X)).abs() < 0.01);
    }

    #[test]
    fn the_focused_cards_row_sits_second_from_the_top() {
        let view = 453.0;
        // Twenty libraries: seven rows, of which some five show.
        assert_eq!(shift_for(0, 20, view), 0.0);
        assert_eq!(shift_for(4, 20, view), 0.0);
        assert_eq!(shift_for(9, 20, view), 2.0 * 96.0);
        // The last rows stop at the end of the list, the safe area under them.
        let max = content_height(20) - view;
        assert_eq!(shift_for(19, 20, view), max);
        // A list that fits never moves.
        assert_eq!(shift_for(8, 9, view), 0.0);
    }
}
