//! A playlist's page (`TvPlaylistScreen.kt`; the TV app's `PlaylistScreen` in `playlists.tsx`).
//!
//! The first entry's backdrop under the item scrim, darkening toward Bg 92 % as the page moves
//! up; a 436 dp top (text column 96–560 from y 58: what it plays, the title, the facts, then Play,
//! Shuffle, Rename and Delete at its foot); then the entries from y 452, 106 apart, from x 80: the
//! number, a 160 × 90 still (a 90 × 90 cover for music) with its progress and watched mark, the
//! title over its second line, the length, and a More button 12 to the right. The page scrolls as
//! one column: a focused entry's top settles at y 140, the top shows whole while its buttons have
//! focus.
//!
//! OK on an entry plays from it: a video playlist plays as the player's queue, a music entry opens
//! its album, which plays it (`PlaylistViewModel.play`). Holding OK, the menu key or More opens its
//! actions (Play from here, Go to it, and for a playlist of the profile's own, Move and Remove).
//! Move lifts the entry: Up and Down move it, OK drops it there and sends the order, Back (or focus
//! leaving it) puts it back; the pointer carries it to the row it points at and a click drops it.
//! Orders go to the server one at a time, the latest only. A smart playlist fills itself: nothing
//! in it is edited here.

use std::collections::HashMap;

use pley_base::{Icon, Key, Point, Priority, Rect, TextOpts};
use pley_core::format::{card_subtitle, episode_label, format_duration, season_title};
use pley_core::http::sentence;
use pley_core::model::{
    ItemKind, ItemSummary, PlaylistDetail, PlaylistEntry, PlaylistKind, PlaylistPatch, PlaylistSummary,
};
use pley_core::store::snapshot_keys;
use pley_core::{Api, ApiError, Promise};
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, Badge, BadgeStyle, BarItem, Button, Dialog, DialogButton,
    ListRow, ListState, PAGE_SCROLL, RoundButton, SidePanel, SlotMode, Tile, TileKind, TileSlot, backdrop_px,
    button_bar, inert,
};
use pley_shell::{AppCx, AppEvent, Chrome, Load, PlayQueue, PlayRequest, Resource, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Dirs, Edge, FocusOpts, Group, Id, Motion, Response, Ui, nudge};

use crate::collections::random_index;
use crate::parts::{Line, Typing, answer, column_message, failed_title, panel_lines, skeleton_blocks};
use crate::playlists::playlist_meta;
use crate::tiles::{ArtKind, Cell, Quality, art_paths, page_of, requests};
use crate::view::describe_smart_filter;

/// The page's top, down to just above the first entry.
pub const HERO_H: f32 = 436.0;
/// Where the first entry starts.
pub const LIST_TOP: f32 = HERO_H + 16.0;
/// An entry's row: its 90 dp still, 8 above and below.
pub const ROW_H: f32 = 106.0;
/// Where a focused entry's top settles once the page has scrolled.
pub const ROW_ANCHOR: f32 = 140.0;
/// The top's text column: x 96 to 560, from y 58.
const TEXT_W: f32 = theme::TEXT_COLUMN_END - theme::GUTTER;
const CONTENT_Y: f32 = 58.0;
/// The buttons' line: the label room and 12 under them end the top; a line higher with the empty
/// text under them.
const BUTTONS_Y: f32 = HERO_H - theme::BUTTON_LABEL_ROOM - 12.0 - theme::BUTTON_HEIGHT;
const BUTTONS_Y_EMPTY: f32 = BUTTONS_Y - 20.0;
/// An entry row's card: from 16 left of the gutter to 12 before More, which ends at the safe edge.
const ROW_X: f32 = theme::GUTTER - 16.0;
const MORE_X: f32 = theme::CANVAS_W - theme::SAFE_X - 40.0;
const CARD_W: f32 = MORE_X - 12.0 - ROW_X;
/// More's square sits a little above the row's middle, as its label's room hangs under it.
const MORE_DY: f32 = 27.0;
/// The darkening over the backdrop once the page has moved a whole top's height.
const DIM: f32 = 0.92;
/// Entries sliding to their places while one is moved (200 ms).
const SLIDE: Motion = Motion::new(0.200, pley_ui::Easing::FAST_OUT_SLOW_IN);

/// An entry's two lines (`Playlists.rowTitles`): an episode's own title over its show and number,
/// a season's name over what a card says, else the title over its year or what a card says.
pub fn entry_titles(item: &ItemSummary) -> (String, String) {
    match item.kind {
        ItemKind::Episode => {
            let show = item.grandparent_title.clone().unwrap_or_default();
            let number = episode_label(item.parent_index, item.index);
            let second = [show, number].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
            (item.title.clone(), second)
        }
        ItemKind::Season => (season_title(Some(&item.title), item.index), card_subtitle(item)),
        _ => (item.title.clone(), card_subtitle(item)),
    }
}

/// `ids` with the one at `from` moved to `to` (unchanged when `from` is out of range;
/// `Playlists.moved`).
pub fn moved<T: Clone>(ids: &[T], from: usize, to: usize) -> Vec<T> {
    if from >= ids.len() {
        return ids.to_vec();
    }
    let mut next = ids.to_vec();
    let id = next.remove(from);
    next.insert(to.min(next.len()), id);
    next
}

/// The order the page shows while an entry is moved, and until the server has the order sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lift {
    /// The entry lifted.
    pub moving: Option<i64>,
    /// The entries' order as moved, not yet the server's.
    pub order: Option<Vec<i64>>,
}

impl Lift {
    /// Lifts `entry`.
    pub fn start(&mut self, entry: i64) {
        self.moving = Some(entry);
    }

    /// Moves the lifted entry `by` places among `shown` (the entries in the order they show);
    /// its new place, or `None` when it can't go there.
    pub fn move_by(&mut self, shown: &[i64], by: isize) -> Option<usize> {
        let entry = self.moving?;
        let from = shown.iter().position(|e| *e == entry)?;
        let to = from.checked_add_signed(by).filter(|t| *t < shown.len())?;
        self.order = Some(moved(shown, from, to));
        Some(to)
    }

    /// Moves the lifted entry to place `to` (the pointer carrying it); its new place.
    pub fn carry_to(&mut self, shown: &[i64], to: usize) -> Option<usize> {
        let entry = self.moving?;
        let from = shown.iter().position(|e| *e == entry)?;
        if to == from || to >= shown.len() {
            return None;
        }
        self.order = Some(moved(shown, from, to));
        Some(to)
    }

    /// Drops the lifted entry where it is: the order to send, when it differs from `server`'s
    /// (the order the server holds, or will once what was sent arrives).
    pub fn drop(&mut self, server: &[i64]) -> Option<Vec<i64>> {
        self.moving = None;
        self.order.clone().filter(|o| o.as_slice() != server)
    }

    /// Puts the lifted entry back where it was.
    pub fn put_back(&mut self) {
        self.moving = None;
        self.order = None;
    }

    /// The server's own order is shown again (a fresh playlist, a refused move), unless an entry
    /// is being moved meanwhile.
    pub fn settle(&mut self) {
        if self.moving.is_none() {
            self.order = None;
        }
    }

    /// The server answered `sent`: the page shows the server's order again if the order shown is
    /// still the one sent; a newer move keeps its own (`order.compareAndSet(ids, null)`).
    pub fn settle_sent(&mut self, sent: &[i64]) {
        if self.moving.is_none() && self.order.as_deref() == Some(sent) {
            self.order = None;
        }
    }

    /// The places in `entries` of those shown, in the order shown: those not `gone`, as moved.
    pub fn arrange(&self, entries: &[PlaylistEntry], gone: &[i64]) -> Vec<usize> {
        let mut shown: Vec<usize> = (0..entries.len()).filter(|&i| !gone.contains(&entries[i].entry_id)).collect();
        if let Some(order) = &self.order {
            let pos: HashMap<i64, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
            shown.sort_by_key(|&i| pos.get(&entries[i].entry_id).copied().unwrap_or(usize::MAX));
        }
        shown
    }
}

/// An entry as its row shows it, worked out the first time it is drawn.
struct Row {
    /// The still (or cover) with its progress and watched mark.
    cell: Cell,
    title: String,
    subtitle: String,
    duration: String,
}

impl Row {
    fn new(item: &ItemSummary, api: Option<&Api>, kind: TileKind, px: u32) -> Row {
        let (title, subtitle) = entry_titles(item);
        let art = api.map(|api| requests(api, &art_paths(item, ArtKind::of(kind)), px)).unwrap_or_default();
        Row {
            cell: Cell::with_art(item.clone(), kind, art, Quality::NONE),
            title,
            subtitle,
            duration: format_duration(item.duration_ms),
        }
    }
}

/// What the page shows of the playlist, worked out when the playlist, a move, a removal or a
/// rename changes it rather than at every frame.
#[derive(Default)]
struct View {
    /// The places in the playlist's entries of those shown, in the order shown.
    shown: Vec<usize>,
    /// Their entry ids, in the same order.
    ids: Vec<i64>,
    /// The title shown: the one renamed to until the server has it.
    title: String,
    /// "12 items · 3h 12m" (less the entries on their way out) and the smart filter's words.
    facts: String,
    /// Delete's question.
    question: String,
}

/// What is open over the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    /// An entry's actions.
    Actions(i64),
    Rename,
    Delete,
}

/// A change on its way to the server.
enum Write {
    Remove(i64, Promise<Result<(), ApiError>>),
    Rename(Promise<Result<PlaylistDetail, ApiError>>),
    Delete(Promise<Result<(), ApiError>>),
}

/// A write's answer.
enum Done {
    Removed(i64, Result<(), ApiError>),
    Renamed(Result<PlaylistDetail, ApiError>),
    Deleted(Result<(), ApiError>),
}

/// An order on its way to the server, and its answer.
type Sending = (Vec<i64>, Promise<Result<PlaylistDetail, ApiError>>);

/// A playlist's page.
pub struct PlaylistScreen {
    id: i64,
    detail: Resource<PlaylistDetail>,
    /// The rows drawn so far, by entry: kept through moves, renames and removals, worked out again
    /// when a fresh playlist comes.
    rows: HashMap<i64, Row>,
    view: View,
    /// The view is worked out again before it is next used.
    dirty: bool,
    /// Entries taken out here, on their way to the server or gone from it: hidden from every
    /// answer, as one asked before the removal still holds them.
    gone: Vec<i64>,
    lift: Lift,
    /// Whether the lifted entry has had focus since it was lifted (focus leaving it puts it back).
    lift_focused: bool,
    /// The title renamed to, until the server has it.
    renamed: Option<String>,
    writes: Vec<Write>,
    /// The order on its way to the server, and the latest one moved since, sent once it answers.
    sending: Option<Sending>,
    queued: Option<Vec<i64>>,
    overlay: Option<Overlay>,
    /// The panel drawn while it slides out.
    last_panel: Overlay,
    /// The entry whose actions show: its two lines and "Go to …".
    actions: (String, String, String),
    panel_list: ListState,
    typing: Typing,
    backdrop: BackdropState,
    art: Option<BackdropArt>,
    /// The page's scroll, where it is heading.
    scroll: f32,
    /// The focused entry's place; `None` in the top.
    at: Option<usize>,
    /// The places of the entries drawn last frame: only they can have focus.
    drawn: (usize, usize),
    /// Focus goes here once it is drawn (a removed entry's neighbour).
    focus_next: Option<Id>,
    /// Where the pointer was last frame: only its moves carry a lifted entry.
    last_pointer: Option<Point>,
}

impl PlaylistScreen {
    /// The page of playlist `id`.
    pub fn new(id: i64) -> PlaylistScreen {
        PlaylistScreen {
            id,
            detail: Resource::new(Some(snapshot_keys::playlist(id)), move |api| api.playlist(id)),
            rows: HashMap::new(),
            view: View::default(),
            dirty: true,
            gone: Vec::new(),
            lift: Lift::default(),
            lift_focused: false,
            renamed: None,
            writes: Vec::new(),
            sending: None,
            queued: None,
            overlay: None,
            last_panel: Overlay::Rename,
            actions: Default::default(),
            panel_list: ListState::default(),
            typing: Typing::default(),
            backdrop: BackdropState::default(),
            art: None,
            scroll: 0.0,
            at: None,
            drawn: (0, 0),
            focus_next: None,
            last_pointer: None,
        }
    }

    /// Works out the view from the playlist, what was taken out, the order moved and the title.
    fn refresh_view(&mut self) {
        self.dirty = false;
        let Some(detail) = self.detail.value() else {
            self.view = View::default();
            return;
        };
        let shown = self.lift.arrange(&detail.entries, &self.gone);
        let ids = shown.iter().map(|&i| detail.entries[i].entry_id).collect();
        let title = self.renamed.clone().unwrap_or_else(|| detail.title.clone());
        // The count and length, less the entries on their way out.
        let (n, ms) = detail
            .entries
            .iter()
            .filter(|e| self.gone.contains(&e.entry_id))
            .fold((0, 0), |(n, ms), e| (n + 1, ms + e.item.duration_ms.unwrap_or(0)));
        let left = PlaylistSummary {
            item_count: (detail.item_count - n).max(0),
            duration_ms: (detail.duration_ms - ms).max(0),
            ..PlaylistSummary::default()
        };
        let mut facts = vec![playlist_meta(&left)];
        if let Some(filter) = detail.smart_filter.as_deref() {
            facts.extend(describe_smart_filter(filter));
        }
        let facts = facts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  ");
        let question = format!("“{title}” will be deleted. What's in it stays in your libraries.");
        self.view = View { shown, ids, title, facts, question };
    }

    /// The order the server holds once what is on its way arrives.
    fn server_order(&self) -> Vec<i64> {
        if let Some(order) = self.queued.as_ref().or(self.sending.as_ref().map(|(o, _)| o)) {
            return order.clone();
        }
        let entries = self.detail.value().map_or(&[][..], |d| d.entries.as_slice());
        entries.iter().map(|e| e.entry_id).filter(|e| !self.gone.contains(e)).collect()
    }

    /// Plays from shown entry `from` (or a random one): a video playlist in the player with the
    /// playlist as the queue; music opens the entry's album, which plays it.
    fn play(&mut self, cx: &mut AppCx, from: usize, shuffle: bool) {
        let Some(detail) = self.detail.value() else { return };
        let shown = &self.view.shown;
        if shown.is_empty() {
            return;
        }
        let start = if shuffle { random_index(cx.now, shown.len()) } else { from.min(shown.len() - 1) };
        let Some(entry) = detail.entries.get(shown[start]) else { return };
        if detail.kind == PlaylistKind::Audio {
            cx.push(page_of(&entry.item));
        } else {
            cx.play(PlayRequest { queue: Some(PlayQueue::Playlist(self.id)), ..PlayRequest::item(entry.item.id) });
        }
    }

    /// Sends `order`, or keeps it to send once the order on its way is answered.
    fn send_order(&mut self, cx: &mut AppCx, order: Vec<i64>) {
        if self.sending.is_some() {
            self.queued = Some(order);
            return;
        }
        let (id, body) = (self.id, order.clone());
        self.sending = Some((order, cx.spawn(move |api| api.reorder_playlist(id, &body))));
    }

    fn collect(&mut self, cx: &mut AppCx) {
        if let Some((sent, p)) = &mut self.sending
            && let Some(result) = answer(p)
        {
            let sent = std::mem::take(sent);
            self.sending = None;
            match result {
                Ok(server) => {
                    // The server's order, for the entries this page still holds: what was taken out
                    // meanwhile stays out.
                    if let Some(v) = self.detail.value_mut() {
                        let pos: HashMap<i64, usize> =
                            server.entries.iter().enumerate().map(|(i, e)| (e.entry_id, i)).collect();
                        v.entries.sort_by_key(|e| pos.get(&e.entry_id).copied().unwrap_or(usize::MAX));
                    }
                    self.lift.settle_sent(&sent);
                }
                Err(e) => {
                    if self.queued.is_none() {
                        self.lift.settle();
                    }
                    fail(cx, &e);
                }
            }
            if let Some(next) = self.queued.take() {
                self.send_order(cx, next);
            }
            self.dirty = true;
        }
        let mut i = 0;
        while i < self.writes.len() {
            let done = match &mut self.writes[i] {
                Write::Remove(entry, p) => answer(p).map(|r| Done::Removed(*entry, r)),
                Write::Rename(p) => answer(p).map(Done::Renamed),
                Write::Delete(p) => answer(p).map(Done::Deleted),
            };
            let Some(done) = done else {
                i += 1;
                continue;
            };
            self.writes.remove(i);
            self.dirty = true;
            match done {
                Done::Removed(entry, Ok(())) => {
                    // Out of the playlist held here, rather than asking for all of it again.
                    if let Some(v) = self.detail.value_mut()
                        && let Some(k) = v.entries.iter().position(|e| e.entry_id == entry)
                    {
                        let gone = v.entries.remove(k);
                        v.playlist.item_count = (v.playlist.item_count - 1).max(0);
                        v.playlist.duration_ms = (v.playlist.duration_ms - gone.item.duration_ms.unwrap_or(0)).max(0);
                    }
                    // It stays in `gone`: an answer the server gave before the removal leaves it out.
                    self.rows.remove(&entry);
                }
                Done::Removed(entry, Err(e)) => {
                    self.gone.retain(|g| *g != entry);
                    fail(cx, &e);
                }
                Done::Renamed(Ok(server)) => {
                    // Only the title: the entries' order is the reorders' business.
                    if let Some(v) = self.detail.value_mut() {
                        v.playlist.title = server.playlist.title;
                    }
                    self.renamed = None;
                }
                Done::Renamed(Err(e)) => {
                    self.renamed = None;
                    fail(cx, &e);
                }
                Done::Deleted(Ok(())) => cx.pop(),
                Done::Deleted(Err(e)) => fail(cx, &e),
            }
        }
    }
}

/// A write the server refused: its words as an error toast.
fn fail(cx: &mut AppCx, e: &ApiError) {
    if !e.cancelled() {
        cx.state.report_failure(e);
        cx.toast_error(sentence(&e.to_string()));
    }
}

/// The response of something drawn but not registered.
fn idle(id: Id) -> Response {
    Response {
        id,
        focused: false,
        gained: false,
        activated: false,
        clicked: false,
        long_pressed: false,
        menu: false,
        pressed: false,
        hovered: false,
        pointer: None,
    }
}

impl PlaylistScreen {
    /// Takes entry `entry` out: at once on the page, with a toast; focus to the next entry (else
    /// the one before, else Delete); put back if the server refuses.
    fn remove(&mut self, cx: &mut AppCx, entry: i64, ids: &PageIds) {
        let shown = &self.view.ids;
        let Some(k) = shown.iter().position(|&e| e == entry) else { return };
        let next = shown.get(k + 1).or(k.checked_sub(1).and_then(|p| shown.get(p)));
        self.focus_next = Some(match next {
            Some(&e) => ids.card(e),
            None => ids.bar.with("delete"),
        });
        let item = self.detail.value().and_then(|d| d.entries.iter().find(|e| e.entry_id == entry));
        let title = item.map(|e| entry_titles(&e.item).0).unwrap_or_default();
        self.gone.push(entry);
        self.dirty = true;
        cx.toast(format!("Removed “{title}”"));
        let id = self.id;
        self.writes.push(Write::Remove(entry, cx.spawn(move |api| api.remove_from_playlist(id, entry))));
    }

    /// Sends the title typed, when it will do and differs; the page shows it at once.
    fn rename(&mut self, cx: &mut AppCx) {
        let Some(title) = self.typing.title() else { return };
        self.overlay = None;
        self.typing.typing = false;
        if title == self.view.title {
            return;
        }
        self.renamed = Some(title.clone());
        self.dirty = true;
        let id = self.id;
        let patch = PlaylistPatch { title: Some(title), smart_filter: None };
        self.writes.push(Write::Rename(cx.spawn(move |api| api.patch_playlist(id, &patch))));
    }

    fn open_actions(&mut self, entry: i64) {
        let Some(e) = self.detail.value().and_then(|d| d.entries.iter().find(|e| e.entry_id == entry)) else { return };
        let (title, subtitle) = entry_titles(&e.item);
        let go_to = format!("Go to {title}");
        self.actions = (title, subtitle, go_to);
        self.overlay = Some(Overlay::Actions(entry));
        self.last_panel = Overlay::Actions(entry);
        self.panel_list = ListState::default();
    }

    /// The panel (an entry's actions, or Rename with the keyboard while typing) and Delete's
    /// question.
    fn overlays(&mut self, ui: &mut Ui, cx: &mut AppCx, ids: &PageIds, editable: bool) {
        let panel_open = matches!(self.overlay, Some(Overlay::Actions(_) | Overlay::Rename));
        let panel = match self.overlay {
            Some(o @ (Overlay::Actions(_) | Overlay::Rename)) => o,
            _ => self.last_panel,
        };
        let mut chosen = None;
        let mut save = false;
        let n = self.view.ids.len();
        let (title, subtitle, go_to) = &self.actions;
        let state = &mut self.panel_list;
        let typing = &mut self.typing;
        let list_id = ui.id("actions");
        let field_id = ui.id("rename-field");
        let save_id = ui.id("rename-save");
        SidePanel::new().back_closes(false).show(ui, ui.id("panel"), panel_open, |ui, area| match panel {
            Overlay::Actions(entry) => {
                let mut lines = vec![
                    Line::Title(title, subtitle),
                    Line::Row(ListRow::new("Play from here").icon(Icon::Play)),
                    Line::Row(ListRow::new(go_to).icon(Icon::ArrowRight)),
                ];
                if editable {
                    lines.push(Line::Row(ListRow::new("Move").icon(Icon::ArrowUpDown).enabled(n > 1)));
                    lines.push(Line::Row(ListRow::new("Remove from playlist").icon(Icon::X).danger(true)));
                }
                chosen = panel_lines(ui, list_id, area, state, &lines, Some(1)).map(|i| (entry, i));
            }
            Overlay::Rename => {
                let mut y = area.y + 8.0;
                let opts = TextOpts::wrap(area.w - 16.0, 2);
                y += ui.text(Point::new(area.x + 16.0, y), "Rename playlist", &theme::TITLE, &opts, theme::FG).h;
                y += 12.0 + 12.0;
                ui.initial_focus(field_id);
                typing.field(ui, field_id, Point::new(area.x, y), area.w, "Title");
                y += 44.0 + 12.0;
                let hint = "Press OK on the title to type it.";
                let opts = TextOpts::wrap(area.w - 4.0, 2);
                y += ui.text(Point::new(area.x + 4.0, y), hint, &theme::CAPTION, &opts, theme::MUTED).h;
                y += 12.0 + 4.0 + 12.0;
                let ok = typing.title().is_some();
                save = Button::pill("Save").enabled(ok).show(ui, save_id, Point::new(area.x, y)).activated;
            }
            Overlay::Delete => {}
        });
        if panel == Overlay::Rename && panel_open && self.typing.keyboard(ui, ui.id("keyboard"), "Done") {
            save = true;
        }
        if save && self.overlay == Some(Overlay::Rename) {
            self.rename(cx);
            ui.request_repaint();
        }
        if let Some((entry, line)) = chosen.filter(|_| panel_open) {
            self.overlay = None;
            match line {
                1 => {
                    let at = self.view.ids.iter().position(|&e| e == entry);
                    self.play(cx, at.unwrap_or(0), false);
                }
                2 => {
                    if let Some(e) = self.detail.value().and_then(|d| d.entries.iter().find(|e| e.entry_id == entry)) {
                        cx.push(page_of(&e.item));
                    }
                }
                3 => {
                    self.lift.start(entry);
                    self.lift_focused = false;
                    ui.request_focus(ids.card(entry));
                }
                4 => self.remove(cx, entry, ids),
                _ => {}
            }
            ui.request_repaint();
        }

        let buttons = [DialogButton::new("Cancel"), DialogButton::danger("Delete")];
        let open = self.overlay == Some(Overlay::Delete);
        let dialog = Dialog::new("Delete playlist?").body(&self.view.question).buttons(&buttons).initial(0);
        let answer = dialog.show(ui, ui.id("delete"), open);
        if open {
            match answer.chosen {
                Some(1) => {
                    self.overlay = None;
                    let id = self.id;
                    self.writes.push(Write::Delete(cx.spawn(move |api| api.delete_playlist(id))));
                }
                Some(_) => self.overlay = None,
                None if answer.dismissed => self.overlay = None,
                None => {}
            }
        }
    }
}

/// The page's focus ids, worked out once a frame.
struct PageIds {
    entries: Id,
    bar: Id,
}

impl PageIds {
    fn row(&self, entry: i64) -> Id {
        self.entries.with(entry as u64)
    }

    fn card(&self, entry: i64) -> Id {
        self.row(entry).with("entry")
    }

    fn more(&self, entry: i64) -> Id {
        self.row(entry).with("more")
    }
}

/// What was done to an entry this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryAct {
    Play,
    Actions,
    Drop,
    CarryHere,
}

/// How an entry's card looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardLook {
    Rest,
    Focused,
    /// Being moved.
    Lifted,
}

/// Draws an entry's card (`EntryRow`): its number, still (the tile with its overlays, as
/// `ArtworkTile` draws it), lines and length (or, lifted, the arrows), on Accent when focused and
/// on Surface3 with a ring when lifted.
fn draw_card(ui: &mut Ui, id: Id, rect: Rect, row: &Row, number: usize, video: bool, look: CardLook) {
    let lifted = look == CardLook::Lifted;
    let focused = look == CardLook::Focused;
    let fill = if lifted {
        theme::SURFACE3
    } else if focused {
        theme::ACCENT
    } else {
        pley_base::Color::TRANSPARENT
    };
    ui.fill(rect, fill, radius::CARD);
    if lifted {
        let s = theme::focus::RING_STROKE;
        ui.stroke(rect.expand(-s / 2.0), theme::ACCENT_BRIGHT, radius::CARD - s / 2.0, s);
    }
    let on_accent = focused && !lifted;
    let (content, secondary) = if on_accent { (theme::ON_ACCENT, theme::ON_ACCENT) } else { (theme::FG, theme::MUTED) };
    let mid = rect.y + rect.h / 2.0;
    let mut x = rect.x + 16.0;
    let number_color = if on_accent { theme::ON_ACCENT } else { theme::SUBTLE };
    let number = number.to_string();
    let layout = ui.layout(&number, &theme::LABEL, &TextOpts::line(28.0));
    let nw = layout.size.w;
    ui.draw_text(Point::new(x + 28.0 - nw, mid - theme::LABEL.line_height / 2.0), layout, number_color);
    x += 28.0 + 16.0;
    let (kind, w) = if video { (TileKind::Landscape, 160.0) } else { (TileKind::Square, 90.0) };
    let still = Rect::new(x, mid - 45.0, w, 90.0);
    let art_id = id.with("art");
    let slot = TileSlot {
        index: 0,
        id: art_id,
        kind,
        rect: still,
        response: idle(art_id),
        mode: SlotMode::Draw,
        priority: Priority::Visible,
    };
    Tile { caption: false, ..row.cell.tile(kind) }.paint(ui, &slot);
    x += w + 20.0;
    let mut right = rect.right() - 16.0;
    if lifted {
        let ax = right - 22.0;
        ui.icon(Rect::new(ax, mid - 22.0, 22.0, 22.0), Icon::ChevronUp, theme::ACCENT_BRIGHT);
        ui.icon(Rect::new(ax, mid, 22.0, 22.0), Icon::ChevronDown, theme::ACCENT_BRIGHT);
        right = ax - 16.0;
    } else if !row.duration.is_empty() {
        let layout = ui.layout(&row.duration, &theme::CAPTION, &TextOpts::default());
        let w = layout.size.w;
        ui.draw_text(Point::new(right - w, mid - theme::CAPTION.line_height / 2.0), layout, secondary);
        right -= w + 16.0;
    }
    let width = (right - x).max(1.0);
    let lines_h = theme::TITLE.line_height + if row.subtitle.is_empty() { 0.0 } else { theme::CAPTION.line_height };
    let mut y = mid - lines_h / 2.0;
    y += ui.text(Point::new(x, y), &row.title, &theme::TITLE, &TextOpts::line(width), content).h;
    if !row.subtitle.is_empty() {
        ui.text(Point::new(x, y), &row.subtitle, &theme::CAPTION, &TextOpts::line(width), secondary);
    }
}

impl Screen for PlaylistScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if self.detail.update(cx) {
            if self.detail.fresh() {
                // A fresh playlist: its rows are worked out again, and the server's order shows
                // unless an order is still on its way.
                self.rows.clear();
                if self.sending.is_none() && self.queued.is_none() {
                    self.lift.settle();
                }
            }
            let lead = self.detail.value().and_then(|d| d.entries.first().map(|e| (&e.item, d.title.clone())));
            self.art = lead.map(|(item, title)| {
                let art = cx.api().map(|api| requests(api, &art_paths(item, ArtKind::Backdrop), backdrop_px(ui)));
                BackdropArt::new(item.id as u64, art.unwrap_or_default(), title)
            });
            self.dirty = true;
        }
        self.collect(cx);
        if self.dirty {
            self.refresh_view();
        }
        match self.detail.state() {
            Load::Loading => {
                skeleton_blocks(
                    ui,
                    &[
                        (58.0, 120.0, 18.0, radius::BADGE),
                        (90.0, 360.0, 44.0, radius::BADGE),
                        (148.0, 200.0, 18.0, radius::BADGE),
                        (314.0, 128.0, theme::BUTTON_HEIGHT, theme::BUTTON_HEIGHT / 2.0),
                    ],
                );
                return;
            }
            Load::Failed { error, stale: None } => {
                if error.status() == Some(404) {
                    let body = "It was deleted, or it belongs to another profile.";
                    let r = column_message(ui, ui.id("state"), CONTENT_Y, "Playlist not found", body, Some("Back"));
                    if r.is_some_and(|r| r.activated) {
                        cx.pop();
                    }
                } else {
                    let problem = error.problem();
                    let title = failed_title(error, "Couldn't load this playlist");
                    let r = column_message(ui, ui.id("state"), CONTENT_Y, title, &problem, Some("Retry"));
                    if r.is_some_and(|r| r.activated) {
                        self.detail.invalidate();
                    }
                }
                return;
            }
            _ => {}
        }
        let Some(detail) = self.detail.value() else { return };
        let video = detail.kind != PlaylistKind::Audio;
        let smart = detail.smart();
        let editable = !smart;
        let n = self.view.ids.len();
        let can_play = n > 0;
        let ids = PageIds { entries: ui.id("entries"), bar: ui.id("buttons") };

        // Where focus is: an entry (its card or More), the buttons, or elsewhere (a panel). Only
        // the entries drawn last frame can have it; the others are looked through only when an
        // answer moved the focused one away.
        let view_ids = &self.view.ids;
        let holds = |k: usize, f: Id| ids.card(view_ids[k]) == f || ids.more(view_ids[k]) == f;
        let focus_entry = ui.focused().filter(|_| ui.focused_in(ids.entries).is_some()).and_then(|f| {
            let (d0, d1) = (self.drawn.0.min(n), self.drawn.1.min(n));
            (d0..d1).find(|&k| holds(k, f)).or_else(|| (0..n).find(|&k| holds(k, f)))
        });
        let in_bar = ui.focused_in(ids.bar).is_some();
        if focus_entry.is_some() {
            self.at = focus_entry;
        } else if in_bar {
            self.at = None;
        }
        self.at = self.at.filter(|i| *i < n);

        // The page scrolls with focus: the top whole while its buttons have it, a focused entry's
        // top at its line; with the pointer, only as far as brings a cut-off entry in whole.
        let max = if n > 0 { (LIST_TOP + (n - 1) as f32 * ROW_H - ROW_ANCHOR).max(0.0) } else { 0.0 };
        let buttons_y = if can_play { BUTTONS_Y } else { BUTTONS_Y_EMPTY };
        let (lo, hi) = match self.at {
            Some(i) => (LIST_TOP + i as f32 * ROW_H, LIST_TOP + (i + 1) as f32 * ROW_H),
            None => (buttons_y, buttons_y + theme::BUTTON_HEIGHT),
        };
        if focus_entry.is_some() || in_bar {
            self.scroll = if ui.pointer_mode() {
                nudge(self.scroll, lo, hi, 0.0, theme::CANVAS_H, 0.0, max)
            } else if self.at.is_some() {
                lo - ROW_ANCHOR
            } else {
                0.0
            };
        }
        self.scroll = self.scroll.clamp(0.0, max);
        let shift = ui.animate(ui.id("scroll"), self.scroll, PAGE_SCROLL);

        // The backdrop, darkening as the entries pass over it.
        Backdrop::new(BackdropScrim::Item).show(ui, ui.id("backdrop"), &mut self.backdrop, self.art.as_ref());
        if shift > 0.5 {
            let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
            ui.fill(full, theme::BG.alpha(DIM * (shift / HERO_H).min(1.0)), 0.0);
        }

        // The top: what it plays, the title, the facts, the buttons.
        let x = theme::GUTTER;
        let mut y = CONTENT_Y - shift;
        let kind_icon = if video { Icon::ListVideo } else { Icon::ListMusic };
        ui.icon(Rect::new(x, y, 18.0, 18.0), kind_icon, theme::MUTED);
        let kind = if smart {
            "Smart playlist"
        } else if video {
            "Video playlist"
        } else {
            "Music playlist"
        };
        let ky = y + (18.0 - theme::CAPTION.line_height) / 2.0;
        ui.text(Point::new(x + 26.0, ky), kind, &theme::CAPTION, &TextOpts::line(TEXT_W - 26.0), theme::MUTED);
        y += 18.0 + 10.0;
        let title = &self.view.title;
        y += ui.text(Point::new(x, y), title, &theme::HERO_TITLE, &TextOpts::wrap(TEXT_W, 2), theme::FG).h;
        y += 12.0;
        let mut fx = x;
        if smart {
            let badge_y = y + (theme::LABEL.line_height - pley_kit::badge::BADGE_HEIGHT) / 2.0;
            fx += Badge::new("Smart", BadgeStyle::Outline).show(ui, Point::new(fx, badge_y)).w + 8.0;
        }
        let opts = TextOpts::wrap((TEXT_W - (fx - x)).max(1.0), 2);
        ui.text(Point::new(fx, y), &self.view.facts, &theme::LABEL, &opts, theme::MUTED);

        let lifting = self.lift.moving.is_some();
        let by = buttons_y - shift;
        let mut items = vec![
            ("play", BarItem::Button(Button::pill("Play").icon(Icon::Play).enabled(can_play))),
            ("shuffle", BarItem::Round(RoundButton::new(Icon::Shuffle, "Shuffle").enabled(n >= 2))),
        ];
        if editable {
            items.push(("rename", BarItem::Round(RoundButton::new(Icon::Pencil, "Rename"))));
        }
        items.push(("delete", BarItem::Round(RoundButton::new(Icon::Trash2, "Delete"))));
        let first = if can_play {
            "play"
        } else if editable {
            "rename"
        } else {
            "delete"
        };
        ui.initial_focus(ids.bar.with(first));
        // While an entry is carried, only the other entries answer: the pointer passing over a
        // button would take focus from it and put it back.
        let bar_at = Point::new(x, by);
        let answers = if lifting {
            inert(ui, |ui| button_bar(ui, ids.bar, bar_at, &items))
        } else {
            button_bar(ui, ids.bar, bar_at, &items)
        };
        let pressed = |key: &str| items.iter().position(|(k, _)| *k == key).is_some_and(|i| answers[i].activated);
        let (play, shuffle, rename, delete) =
            (pressed("play"), pressed("shuffle"), pressed("rename"), pressed("delete"));
        if !can_play {
            let text = if smart {
                "Nothing matches this playlist's filter yet."
            } else if video {
                "This playlist is empty. Use “Add to playlist” on a movie, show or episode, or hold OK on a poster."
            } else {
                "This playlist is empty. Use “Add to playlist” on an artist or album, or hold OK on a cover."
            };
            let ey = by + theme::BUTTON_HEIGHT + theme::BUTTON_LABEL_ROOM + 12.0;
            ui.text(Point::new(x, ey), text, &theme::BODY, &TextOpts::wrap(TEXT_W, 3), theme::MUTED);
        }

        // The entries near the screen, and always the focused one's neighbours.
        let first_row = (((shift - LIST_TOP) / ROW_H).floor() - 2.0).max(0.0) as usize;
        let past_row = (first_row + (theme::CANVAS_H / ROW_H).ceil() as usize + 4).min(n);
        let (r0, r1) = match self.at {
            Some(i) => (first_row.min(i.saturating_sub(1)), past_row.max((i + 2).min(n))),
            None => (first_row.min(n), past_row),
        };
        self.drawn = (r0, r1);
        let pointer = ui.pointer();
        let pointer_moved = pointer.is_some() && pointer != self.last_pointer;
        self.last_pointer = pointer;
        let mut act: Option<(usize, EntryAct)> = None;
        let rows = &mut self.rows;
        let view = &self.view;
        let moving = self.lift.moving;
        let api = cx.api();
        let (row_kind, row_px) = if video { (TileKind::Landscape, 160.0) } else { (TileKind::Square, 90.0) };
        let px = ui.image_px(row_px);
        ui.group(ids.entries, Group::column(), |ui| {
            for k in r0..r1 {
                let Some(entry) = detail.entries.get(view.shown[k]) else { continue };
                let e = entry.entry_id;
                let row = rows.entry(e).or_insert_with(|| Row::new(&entry.item, api, row_kind, px));
                let lifted = moving == Some(e);
                let top = ui.animate(ids.row(e).with("y"), LIST_TOP + k as f32 * ROW_H, SLIDE) - shift;
                let card = Rect::new(ROW_X, top, CARD_W, ROW_H);
                let group = Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop);
                ui.group(ids.row(e), group, |ui| {
                    let r = if lifting && !lifted {
                        // While one is lifted, the others are where the pointer can carry it.
                        let target = ui.clickable(ids.card(e), card);
                        if target.activated {
                            act = Some((k, EntryAct::Drop));
                        } else if target.hovered && pointer_moved {
                            act = Some((k, EntryAct::CarryHere));
                        }
                        target
                    } else {
                        let opts = if lifted {
                            FocusOpts::default().takes(Dirs::ALL)
                        } else {
                            FocusOpts::default().long_press()
                        };
                        ui.focusable_with(ids.card(e), card, opts)
                    };
                    let look = match (lifted, r.focused) {
                        (true, _) => CardLook::Lifted,
                        (false, true) => CardLook::Focused,
                        (false, false) => CardLook::Rest,
                    };
                    draw_card(ui, ids.card(e), card, row, k + 1, video, look);
                    if lifted {
                        if r.activated {
                            act = Some((k, EntryAct::Drop));
                        }
                    } else if r.activated && !lifting {
                        act = Some((k, EntryAct::Play));
                    } else if r.long_pressed || r.menu {
                        act = Some((k, EntryAct::Actions));
                    }
                    let more = RoundButton::new(Icon::Ellipsis, "More").on_art(false);
                    let at = Point::new(MORE_X, top + MORE_DY);
                    if lifting {
                        inert(ui, |ui| more.show(ui, ids.more(e), at));
                    } else if more.show(ui, ids.more(e), at).activated {
                        act = Some((k, EntryAct::Actions));
                    }
                });
            }
        });

        if play {
            self.play(cx, 0, false);
        }
        if shuffle {
            self.play(cx, 0, true);
        }
        if rename {
            self.typing = Typing::new(&self.view.title);
            self.overlay = Some(Overlay::Rename);
            self.last_panel = Overlay::Rename;
        }
        if delete {
            self.overlay = Some(Overlay::Delete);
        }

        // The lifted entry: Up and Down move it; focus leaving it puts it back.
        if let Some(e) = self.lift.moving {
            if ui.is_focused(ids.card(e)) {
                self.lift_focused = true;
                let steps: Vec<isize> = ui
                    .keys()
                    .iter()
                    .filter(|k| k.pressed)
                    .filter_map(|k| match k.key {
                        Key::Up => Some(-1),
                        Key::Down => Some(1),
                        _ => None,
                    })
                    .collect();
                let mut order = self.lift.order.clone().unwrap_or_else(|| self.view.ids.clone());
                for by in steps {
                    if self.lift.move_by(&order, by).is_some() {
                        order = self.lift.order.clone().unwrap_or_default();
                        self.dirty = true;
                        ui.request_repaint();
                    }
                }
            } else if self.lift_focused && act.is_none() && self.overlay.is_none() {
                self.lift.put_back();
                self.dirty = true;
            }
        }
        match act {
            Some((k, EntryAct::Play)) => self.play(cx, k, false),
            Some((k, EntryAct::Actions)) => self.open_actions(self.view.ids[k]),
            Some((_, EntryAct::Drop)) => {
                let server = self.server_order();
                match self.lift.drop(&server) {
                    Some(order) => self.send_order(cx, order),
                    None => self.lift.settle_sent(&server),
                }
                self.dirty = true;
            }
            Some((k, EntryAct::CarryHere)) => {
                self.lift.carry_to(&self.view.ids, k);
                self.dirty = true;
            }
            None => {}
        }
        if act.is_some() {
            ui.request_repaint();
        }
        if let Some(target) = self.focus_next.take() {
            ui.request_focus(target);
        }

        if lifting {
            let hint = "Up and down to move it, OK to drop it here, Back to put it back";
            let layout = ui.layout(hint, &theme::LABEL, &TextOpts::default());
            let size = layout.size;
            let (w, h) = (size.w + 40.0, size.h + 24.0);
            let rect = Rect::new((theme::CANVAS_W - w) / 2.0, theme::CANVAS_H - theme::SAFE_Y - h, w, h);
            ui.fill(rect, theme::SURFACE3, h / 2.0);
            ui.draw_text(Point::new(rect.x + 20.0, rect.y + 12.0), layout, theme::FG);
        }

        self.overlays(ui, cx, &ids, editable);
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if self.typing.back() {
            return true;
        }
        if self.lift.moving.is_some() {
            self.lift.put_back();
            self.dirty = true;
            return true;
        }
        self.overlay.take().is_some()
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => self.detail.invalidate(),
            AppEvent::Shown => self.detail.revalidate(cx.now),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.detail.value().is_some() || self.detail.error().is_some()
    }

    fn modal(&self) -> bool {
        self.overlay.is_some() || self.typing.typing || self.lift.moving.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64) -> PlaylistEntry {
        PlaylistEntry { entry_id: id, item: ItemSummary { id: id * 10, ..ItemSummary::default() } }
    }

    #[test]
    fn rows_read_as_on_android() {
        let episode = ItemSummary {
            kind: ItemKind::Episode,
            title: "Office Olympics".into(),
            grandparent_title: Some("The Office".into()),
            parent_index: Some(2),
            index: Some(3),
            ..ItemSummary::default()
        };
        assert_eq!(entry_titles(&episode), ("Office Olympics".into(), "The Office · S2 · E3".into()));
        let season = ItemSummary { kind: ItemKind::Season, index: Some(0), ..ItemSummary::default() };
        assert_eq!(entry_titles(&season).0, "Specials");
        let movie =
            ItemSummary { kind: ItemKind::Movie, title: "Arrival".into(), year: Some(2016), ..ItemSummary::default() };
        assert_eq!(entry_titles(&movie), ("Arrival".into(), "2016".into()));
    }

    #[test]
    fn moving_reorders_and_sends_only_a_change() {
        assert_eq!(moved(&[1, 2, 3, 4], 0, 2), vec![2, 3, 1, 4]);
        assert_eq!(moved(&[1, 2, 3], 5, 0), vec![1, 2, 3]);
        let server = [1, 2, 3, 4];
        let mut lift = Lift::default();
        assert_eq!(lift.move_by(&server, 1), None, "nothing lifted");
        lift.start(2);
        assert_eq!(lift.move_by(&server, -1), Some(0));
        let shown = lift.order.clone().unwrap();
        assert_eq!(shown, vec![2, 1, 3, 4]);
        assert_eq!(lift.move_by(&shown, -1), None, "not past the top");
        assert_eq!(lift.move_by(&shown, 1), Some(1));
        // Back where it was: nothing to send.
        assert_eq!(lift.drop(&server), None);
        assert_eq!(lift.moving, None);
        lift.start(4);
        assert_eq!(lift.carry_to(&server, 1), Some(1));
        assert_eq!(lift.drop(&server), Some(vec![1, 4, 2, 3]));
        // Until the server answers, the page keeps the order sent; entries taken out stay out.
        let entries = [entry(1), entry(2), entry(3), entry(4)];
        assert_eq!(lift.arrange(&entries, &[]), vec![0, 3, 1, 2]);
        assert_eq!(lift.arrange(&entries, &[4]), vec![0, 1, 2]);
        lift.settle();
        assert_eq!(lift.order, None);
        assert_eq!(lift.arrange(&entries, &[2]), vec![0, 2, 3]);
    }

    #[test]
    fn only_the_answer_to_the_order_shown_settles_it() {
        let mut lift = Lift::default();
        lift.start(1);
        lift.move_by(&[1, 2, 3], 1);
        let first = lift.drop(&[1, 2, 3]).unwrap();
        assert_eq!(first, vec![2, 1, 3]);
        // Moved again before the server answered: the newer order stays shown.
        lift.start(1);
        lift.move_by(&first, 1);
        let second = lift.drop(&first).unwrap();
        lift.settle_sent(&first);
        assert_eq!(lift.order.as_deref(), Some(&second[..]));
        lift.settle_sent(&second);
        assert_eq!(lift.order, None);
    }

    #[test]
    fn back_puts_a_lifted_entry_back() {
        let mut lift = Lift::default();
        lift.start(3);
        lift.move_by(&[1, 2, 3], -2);
        lift.put_back();
        assert_eq!(lift, Lift::default());
        // An answer while moving keeps the order being moved.
        lift.start(1);
        lift.move_by(&[1, 2, 3], 1);
        lift.settle();
        assert!(lift.order.is_some());
        lift.settle_sent(&[2, 1, 3]);
        assert!(lift.order.is_some());
    }
}
