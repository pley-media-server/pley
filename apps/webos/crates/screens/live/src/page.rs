//! Live TV's page (`TvLiveTvScreen.kt`): "Live TV" with the
//! Guide and Channels tabs, the categories down the left under the search, and beside them the
//! category's channels in the guide or as cards. A programme's details open in the side panel, with
//! the favourite. The page opens as it was left: the view, the category and the channel watched
//! last.

use std::collections::HashSet;
use std::sync::Arc;

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::http::sentence;
use pley_core::model::{LiveChannel, LiveChannelPage, LiveGroup, LiveGroups, LiveProgram};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_kit::{Button, Keyboard, KeyboardState, Message, MessageStyle, SidePanel, TextEdit};
use pley_player::live::data::groups_matching;
use pley_player::live::{Category, ChannelPages, GuideStore};
use pley_player::{PlayerServer, now_ms};
use pley_shell::{AppCx, AppEvent, Chrome, Resource, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Dirs, Edge, Group, Id, Ui};

use crate::cards::{CardAction, Cards};
use crate::categories::{self, Column, ColumnView, Picked, Search};
use crate::clock::Clock;
use crate::details::{self, DetailsAction};
use crate::guide::{Guide, GuideAction, GuideData, MARGIN, guide_rows};
use crate::kept::{self, Kept, View};
use crate::lineup::{Lineup, first_page};

/// The title row: at the safe area's top, as tall as the tabs.
const HEADER_TOP: f32 = theme::SAFE_Y;
const HEADER_H: f32 = theme::BUTTON_HEIGHT;
/// The categories and the channels start 16 under the title row.
pub const CONTENT_TOP: f32 = HEADER_TOP + HEADER_H + 16.0;
/// The categories' left edge: the gutter less their rows' inset.
pub const CATEGORIES_X: f32 = theme::GUTTER - categories::INSET;
/// The channels' left edge, 16 past the categories.
pub const CHANNELS_X: f32 = CATEGORIES_X + categories::WIDTH + 16.0;
/// How long a favourite that wasn't saved says so.
const NOTICE_S: f64 = 4.0;
/// What's on is drawn anew this often (progress, the now line).
const TICK_S: i64 = 15;
/// The guide keeps the programmes of this many channels at most; past them it starts over with the rows
/// in view, so scrolling through a lineup of thousands doesn't pile up their programmes (Android keeps
/// its guide chunks in a 50-entry cache).
const GUIDE_CHANNELS_KEPT: usize = 500;

/// The first page of a category saved for next time: being read, read, and whether this showing
/// wrote it anew.
struct Saved {
    key: String,
    reading: Option<Promise<Option<LiveChannelPage>>>,
    page: Option<LiveChannelPage>,
    written: bool,
}

/// A favourite being marked on the server.
struct Marking {
    id: i64,
    on: bool,
    job: Promise<Result<(), ApiError>>,
}

/// Live TV's page.
pub struct LiveTvScreen {
    generation: Option<u64>,
    server: Option<Arc<dyn PlayerServer>>,
    kept: Option<Promise<Kept>>,
    ready: bool,
    view: View,
    category: Category,
    /// The channel watched last, with the category it was played in.
    last: Option<i64>,
    query: TextEdit,
    typing: bool,
    keyboard: KeyboardState,
    groups: Resource<LiveGroups>,
    /// The groups whose names have the search's words in them, for the groups' rev and the words.
    found: Vec<LiveGroup>,
    found_for: Option<(u64, String)>,
    /// Change as the groups, and the groups found, do: the categories are made again only then.
    groups_rev: u64,
    column_rev: u64,
    pages: Option<ChannelPages>,
    saved: Option<Saved>,
    programs: Option<GuideStore>,
    /// The channels whose programmes the guide asked for since it last started over.
    guide_channels: HashSet<i64>,
    guide: Guide,
    cards: Cards,
    column: Column,
    /// The channel being looked for to open on it.
    landing: Option<i64>,
    details: Option<(LiveChannel, Option<LiveProgram>)>,
    /// What the panel shows while it slides out.
    details_shown: Option<(LiveChannel, Option<LiveProgram>)>,
    /// The details just opened: their Watch takes focus.
    focus_details: bool,
    notice: Option<(String, f64)>,
    marking: Vec<Marking>,
    to_content: bool,
    to_category: bool,
    reported: bool,
    settled: bool,
    /// Live TV isn't open to this profile: nothing to focus, so the rail has focus.
    closed: bool,
    /// Whether Live TV's summary is to be asked for, when it isn't known: once as the page opens and
    /// each time it shows again, not every frame.
    ask_live: bool,
    /// The player was opened from here: back from it, the guide opens on the channel watched.
    watching: bool,
    /// The TV writes the time in 24 hours: read as the page opens and shows again.
    hours24: Option<bool>,
}

impl Default for LiveTvScreen {
    fn default() -> LiveTvScreen {
        LiveTvScreen::new()
    }
}

impl LiveTvScreen {
    /// The page, as it opens (nothing is asked before it first draws).
    pub fn new() -> LiveTvScreen {
        LiveTvScreen {
            generation: None,
            server: None,
            kept: None,
            ready: false,
            view: View::Guide,
            category: Category::All,
            last: None,
            query: TextEdit::new(""),
            typing: false,
            keyboard: KeyboardState::default(),
            groups: Resource::new(Some(snapshot_keys::LIVE_GROUPS.to_string()), |api| api.live_groups()),
            found: Vec::new(),
            found_for: None,
            groups_rev: 0,
            column_rev: 0,
            pages: None,
            saved: None,
            programs: None,
            guide_channels: HashSet::new(),
            guide: Guide::new(pley_player::live::data::now_seconds()),
            cards: Cards::default(),
            column: Column::default(),
            landing: None,
            details: None,
            details_shown: None,
            focus_details: false,
            notice: None,
            marking: Vec::new(),
            to_content: true,
            to_category: false,
            reported: false,
            settled: false,
            closed: false,
            ask_live: true,
            watching: false,
            hours24: None,
        }
    }

    /// The category whose channels show: the search's while there are words in it.
    pub fn listed(&self) -> Category {
        let q = self.query.text.trim();
        if q.is_empty() { self.category.clone() } else { Category::Search(q.to_string()) }
    }

    /// The view shown.
    pub fn view(&self) -> View {
        self.view
    }

    /// The category kept (not a search).
    pub fn category(&self) -> &Category {
        &self.category
    }

    /// Where the guide's focus is.
    pub fn guide(&self) -> &Guide {
        &self.guide
    }

    /// The programme's details showing, if any.
    pub fn details(&self) -> Option<&(LiveChannel, Option<LiveProgram>)> {
        self.details.as_ref()
    }

    /// The channel at `index` of the category listed, once loaded.
    pub fn channel_at(&self, index: i64) -> Option<&LiveChannel> {
        self.lineup().and_then(|l| l.at(index))
    }

    fn lineup(&self) -> Option<Lineup<'_>> {
        let pages = self.pages.as_ref()?;
        Some(Lineup { pages, saved: self.saved.as_ref().and_then(|s| s.page.as_ref()) })
    }

    /// Another server, sign-in or profile: everything starts over, from what was kept for it.
    fn sync(&mut self, cx: &mut AppCx, now: i64) {
        let generation = cx.state.generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.server = cx.api().map(|a| Arc::new(a.clone()) as Arc<dyn PlayerServer>);
            self.pages = None;
            self.saved = None;
            self.programs = None;
            self.guide_channels.clear();
            self.ask_live = true;
            self.ready = false;
            self.guide = Guide::new(now);
            let (store, server, profile) =
                (cx.store().clone(), cx.state.server().map(str::to_string), cx.state.profile());
            self.kept = Some(cx.tasks().spawn(move || kept::read(&store, server.as_deref(), profile)));
        }
        if let Some(p) = self.kept.as_mut()
            && (p.ready() || p.failed())
        {
            let k = p.take().unwrap_or_default();
            self.kept = None;
            self.view = k.view;
            self.category = k.category;
            self.last = cx.state.live_last_watched.as_ref().map(|l| l.0).or(k.last.map(|l| l.0));
            self.ready = true;
        }
    }

    /// The pages of the category listed, made anew when it changes, opening on the channel watched
    /// last when it is among them.
    fn ensure_pages(&mut self, cx: &AppCx, now: i64) {
        let Some(server) = self.server.clone() else { return };
        let listed = self.listed();
        if self.pages.as_ref().is_some_and(|p| *p.category() == listed) {
            return;
        }
        let mut pages = ChannelPages::new(server.clone(), cx.tasks().clone(), listed.clone(), false);
        self.landing = None;
        if !matches!(listed, Category::Search(_))
            && let Some(id) = self.last
        {
            pages.neighbor(id, 0, now_ms());
            self.landing = Some(id);
        }
        self.pages = Some(pages);
        self.reported = false;
        self.saved = match listed {
            Category::Search(_) => None,
            ref c => read_saved(cx, snapshot_keys::live_page(&c.key())),
        };
        if self.programs.is_none() {
            self.programs = Some(GuideStore::new(server, cx.tasks().clone(), self.guide.start));
        }
        self.guide.land(0, now);
        self.cards = Cards::default();
    }

    /// Takes what came back: pages, programmes, the saved first page, favourites.
    fn collect(&mut self, cx: &mut AppCx, now: i64) {
        let ms = now_ms();
        if let Some(pages) = self.pages.as_mut() {
            pages.poll(ms);
            if let Some(id) = self.landing
                && let Some(found) = pages.take_neighbor()
            {
                self.landing = None;
                if let Some(i) = found.and_then(|_| pages.find(id)) {
                    self.guide.land(i as usize, now);
                    self.cards.land(i as usize);
                }
            }
            match pages.error() {
                Some(e) if !self.reported => {
                    cx.state.report_failure(e);
                    self.reported = true;
                }
                None if self.reported && pages.total().is_some() => {
                    cx.state.server_answered();
                    self.reported = false;
                }
                _ => {}
            }
        }
        if let Some(p) = self.programs.as_mut() {
            p.poll(ms);
        }
        if let Some(saved) = self.saved.as_mut() {
            if let Some(p) = saved.reading.as_mut()
                && (p.ready() || p.failed())
            {
                saved.page = p.take().flatten();
                saved.reading = None;
            }
            if !saved.written
                && let Some(first) = self.pages.as_ref().and_then(first_page)
            {
                saved.written = true;
                write_saved(cx, saved.key.clone(), first);
            }
        }
        let mut i = 0;
        while i < self.marking.len() {
            let m = &mut self.marking[i];
            if !m.job.ready() && !m.job.failed() {
                i += 1;
                continue;
            }
            let mut m = self.marking.swap_remove(i);
            let (id, on) = (m.id, m.on);
            match m.job.take() {
                Some(Ok(())) => {
                    self.groups.invalidate();
                    if matches!(self.listed(), Category::Favorites)
                        && let Some(p) = self.pages.as_mut()
                    {
                        p.stale();
                    }
                }
                other => {
                    // Undone: the server didn't keep it.
                    self.patch(id, !on);
                    if let Some(g) = self.groups.value_mut() {
                        g.favorites = (g.favorites + if on { -1 } else { 1 }).max(0);
                    }
                    let why = match other {
                        Some(Err(e)) => sentence(&e.to_string()),
                        _ => "The request stopped.".to_string(),
                    };
                    self.notice = Some((format!("The favourite wasn't saved: {why}"), cx.now));
                }
            }
        }
    }

    /// Marks `channel` a favourite or not: every page, the details and the count at once, undone
    /// if the server refuses.
    fn toggle_favourite(&mut self, cx: &AppCx, channel: &LiveChannel) {
        let (id, on) = (channel.id, !channel.favorite);
        self.patch(id, on);
        if let Some(g) = self.groups.value_mut() {
            g.favorites = (g.favorites + if on { 1 } else { -1 }).max(0);
        }
        self.marking.push(Marking { id, on, job: cx.spawn(move |api| api.set_live_favorite(id, on)) });
    }

    fn patch(&mut self, id: i64, on: bool) {
        if let Some(p) = self.pages.as_mut() {
            p.patch(id, |c| c.favorite = on);
        }
        if let Some(page) = self.saved.as_mut().and_then(|s| s.page.as_mut()) {
            page.channels.iter_mut().filter(|c| c.id == id).for_each(|c| c.favorite = on);
        }
        for d in [&mut self.details, &mut self.details_shown].into_iter().flatten() {
            if d.0.id == id {
                d.0.favorite = on;
            }
        }
    }

    /// Opens the player on `channel`, channel up and down going through the category listed.
    fn watch(&mut self, cx: &mut AppCx, channel: &LiveChannel) {
        self.watching = true;
        cx.state.keep_live_channel(channel.clone());
        let category = kept::live_category(&self.listed());
        cx.push(Route::LivePlayer { channel_id: channel.id, category, pressed_at: cx.now });
    }

    fn select(&mut self, cx: &AppCx, category: Category) {
        self.query.clear();
        self.typing = false;
        if category != self.category {
            kept::keep_category(cx, &category);
        }
        self.category = category;
        self.to_content = true;
    }

    /// Asks again for what failed (Try again).
    fn retry(&mut self, cx: &mut AppCx) {
        self.pages = None;
        self.groups.invalidate();
        cx.state.load_live();
        self.to_content = true;
    }

    /// The groups a search names, found again only when the words or the groups change.
    fn find_groups(&mut self) {
        let words = self.query.text.trim();
        if self.found_for.as_ref().is_some_and(|(rev, w)| *rev == self.groups_rev && w == words) {
            return;
        }
        let groups = self.groups.value().map_or(&[][..], |g| g.groups.as_slice());
        self.found = groups_matching(groups, words, categories::FOUND_MAX);
        self.found_for = Some((self.groups_rev, words.to_string()));
        self.column_rev += 1;
    }

    /// The categories as they show: the category shown stays marked during a search too
    /// (`CategoryColumn`'s `selected = page.category`).
    fn column_view(&self) -> ColumnView<'_> {
        ColumnView {
            groups: self.groups.value(),
            rev: self.column_rev,
            selected: Some(&self.category),
            found: &self.found,
            search: Some(Search { text: &self.query.text, placeholder: "Search", typing: self.typing }),
        }
    }

    /// Asks for the programmes of the guide's rows `ids`, the store started over once it holds too
    /// many channels'.
    fn want_programs(&mut self, cx: &AppCx, ids: &[i64], from: i64, to: i64) {
        self.guide_channels.extend(ids.iter().copied());
        if self.guide_channels.len() > GUIDE_CHANNELS_KEPT
            && let Some(server) = self.server.clone()
        {
            self.programs = Some(GuideStore::new(server, cx.tasks().clone(), self.guide.start));
            self.guide_channels.clear();
            self.guide_channels.extend(ids.iter().copied());
        }
        if let Some(p) = self.programs.as_mut() {
            p.want(from, to, ids, now_ms());
        }
    }

    /// Back from the player: Recently watched has the channel watched first, favourites marked there
    /// show (the pages and the counts are asked for again), and the guide opens on the channel watched.
    fn back_from_player(&mut self, cx: &AppCx) {
        self.groups.invalidate();
        if let Some(p) = self.pages.as_mut() {
            p.stale();
        }
        let Some((id, _)) = cx.state.live_last_watched.clone() else { return };
        self.last = Some(id);
        if !matches!(self.listed(), Category::Search(_))
            && let Some(p) = self.pages.as_mut()
        {
            p.neighbor(id, 0, now_ms());
            self.landing = Some(id);
        }
    }
}

fn read_saved(cx: &AppCx, key: String) -> Option<Saved> {
    let (server, profile) = (cx.state.server()?.to_string(), cx.state.profile()?);
    let (snapshots, k) = (cx.snapshots().clone(), key.clone());
    let reading = cx.tasks().spawn(move || snapshots.get::<LiveChannelPage>(&server, profile, &k));
    Some(Saved { key, reading: Some(reading), page: None, written: false })
}

fn write_saved(cx: &AppCx, key: String, page: LiveChannelPage) {
    let (Some(server), Some(profile)) = (cx.state.server().map(str::to_string), cx.state.profile()) else { return };
    let snapshots = cx.snapshots().clone();
    cx.tasks().detach(move || {
        if let Err(e) = snapshots.put(&server, profile, &key, &page) {
            log::warn!("snapshot {key} not saved: {e}");
        }
    });
}

impl Screen for LiveTvScreen {
    fn media_keys(&mut self, ui: &mut Ui) -> bool {
        self.view == View::Guide && self.guide.media_keys(ui)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let hours24 = *self.hours24.get_or_insert_with(|| cx.device().clock_24h());
        let clock = Clock::read(hours24);
        let now = clock.now;
        self.sync(cx, now);
        if std::mem::take(&mut self.ask_live) && cx.state.live().is_none() {
            cx.state.load_live();
        }
        if self.groups.update(cx) {
            self.groups_rev += 1;
        }
        if self.ready {
            self.ensure_pages(cx, now);
        }
        // Until the first page says how many there are, it is asked for (again 10 s after a failure).
        if let Some(p) = self.pages.as_mut().filter(|p| p.total().is_none()) {
            p.want(0, 0, now_ms());
        }
        self.collect(cx, now);
        ui.repaint_after((TICK_S - now.rem_euclid(TICK_S)) as f64);

        let summary_allows = cx.state.live().is_none_or(|l| l.allowed);
        let refused = self.pages.as_ref().and_then(|p| p.error()).is_some_and(|e| e.is(403));
        let allowed = summary_allows && !refused;

        // The title row.
        let title_y = HEADER_TOP + (HEADER_H - theme::PAGE_TITLE.line_height) / 2.0;
        ui.text(Point::new(theme::GUTTER, title_y), "Live TV", &theme::PAGE_TITLE, &TextOpts::line(400.0), theme::FG);
        let tabs = ui.id("tabs");
        let tab_id = |v: View| tabs.with(v.key());
        if allowed {
            let guide = Button::tab("Guide", self.view == View::Guide);
            let channels = Button::tab("Channels", self.view == View::Channels);
            let (gw, cw) = (guide.size(ui).w, channels.size(ui).w);
            let right = theme::CANVAS_W - theme::SAFE_X;
            let mut chosen = None;
            ui.group(tabs, Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop), |ui| {
                if guide.show(ui, tab_id(View::Guide), Point::new(right - cw - 8.0 - gw, HEADER_TOP)).activated {
                    chosen = Some(View::Guide);
                }
                if channels.show(ui, tab_id(View::Channels), Point::new(right - cw, HEADER_TOP)).activated {
                    chosen = Some(View::Channels);
                }
            });
            if let Some(v) = chosen.filter(|v| *v != self.view) {
                self.view = v;
                kept::keep_view(cx, v);
            }
            self.closed = false;
        } else {
            self.settled = true;
            let body = "An administrator can open it in Pley's web app, under Settings → Live TV.";
            let closed = ui.id("closed");
            Message::new(
                MessageStyle::Column { top: HEADER_TOP + HEADER_H + 40.0 },
                "Live TV isn't open to this profile",
            )
            .body(body)
            .show(ui, closed);
            if !std::mem::replace(&mut self.closed, true) {
                cx.to_rail();
            }
            return;
        }

        // The categories.
        self.find_groups();
        let column_id = ui.id("categories");
        let column_rect = Rect::new(CATEGORIES_X, CONTENT_TOP, categories::WIDTH, theme::CANVAS_H - CONTENT_TOP);
        let mut column = std::mem::take(&mut self.column);
        if std::mem::take(&mut self.to_category) {
            column.focus_shown(ui, column_id, &self.column_view());
        }
        // A held Up stops at the search field; the tabs take a press of its own.
        let picked = column.show(ui, column_id, column_rect, &self.column_view(), Edge::Pass, Dirs::UP).picked;
        self.column = column;
        match picked {
            Some(Picked::Category(c)) => {
                self.select(cx, c);
                // The new category's channels from this frame on, so focus waits for them.
                self.ensure_pages(cx, now);
            }
            Some(Picked::Search) => self.typing = true,
            None => {}
        }

        // The channels.
        let listed = self.listed();
        let area = Rect::new(CHANNELS_X, CONTENT_TOP, theme::CANVAS_W - CHANNELS_X, theme::CANVAS_H - CONTENT_TOP);
        let guide_id = ui.id("guide");
        let cards_id = ui.id("cards");
        let total = self.lineup().and_then(|l| l.total());
        let error = self.pages.as_ref().and_then(|p| p.error()).cloned();
        let mut content: Option<Id> = None;
        let mut guide_action = None;
        let mut card_action = None;
        match (total, error) {
            (None, Some(e)) => {
                self.settled = true;
                let title = if e.unreachable() { "Can't reach your server" } else { "Live TV didn't load" };
                let body = e.problem();
                let retry_id = ui.id("retry");
                let r = ui.translate(Point::new(CHANNELS_X - theme::GUTTER, 0.0), |ui| {
                    Message::new(MessageStyle::Column { top: CONTENT_TOP + 24.0 }, title)
                        .body(&body)
                        .action("Try again", Icon::RefreshCw)
                        .show(ui, retry_id)
                });
                content = Some(retry_id.with("action"));
                if r.action.is_some_and(|a| a.activated) {
                    self.retry(cx);
                }
            }
            (None, None) => {}
            (Some(0), _) => {
                self.settled = true;
                self.to_content = false;
                let (title, body) = empty_words(&listed);
                ui.translate(Point::new(CHANNELS_X - theme::GUTTER, 0.0), |ui| {
                    let empty = ui.id("empty");
                    Message::new(MessageStyle::Column { top: CONTENT_TOP + 24.0 }, title).body(&body).show(ui, empty)
                });
            }
            (Some(_), _) => {
                self.settled = true;
                let (Some(pages), Some(programs)) = (self.pages.as_ref(), self.programs.as_ref()) else { return };
                let lineup = Lineup { pages, saved: self.saved.as_ref().and_then(|s| s.page.as_ref()) };
                match self.view {
                    View::Guide => {
                        let data = GuideData { pages: lineup, programs, category: &listed, clock };
                        let out = self.guide.show(ui, cx, guide_id, area, &data);
                        guide_action = out.action;
                        content = Some(guide_id);
                        let rows = out.rows;
                        let total = total.unwrap_or(0).max(0) as usize;
                        let (lo, hi) = guide_rows(rows, total);
                        let ids: Vec<i64> = (lo..hi).filter_map(|i| lineup.at(i as i64).map(|c| c.id)).collect();
                        let left = self.guide.cursor.left;
                        let from = (left - crate::guide::VIEW_S).max(self.guide.start);
                        let to = left + 2 * crate::guide::VIEW_S;
                        let (want_lo, want_hi) = (rows.0.saturating_sub(MARGIN), rows.1 + MARGIN);
                        self.want_programs(cx, &ids, from, to);
                        if let Some(p) = self.pages.as_mut() {
                            p.want(want_lo as i64, want_hi as i64, now_ms());
                        }
                    }
                    View::Channels => {
                        let out = self.cards.show(ui, cx, cards_id, CHANNELS_X, CONTENT_TOP, lineup, &listed, now);
                        card_action = out.action;
                        content = Some(Cards::cell_id(cards_id, self.cards.grid.index));
                        if let Some(p) = self.pages.as_mut() {
                            crate::cards::want(p, out.wanted, now_ms());
                        }
                    }
                }
            }
        }

        // Where focus goes: the channels as they show, else the category shown.
        match content {
            Some(id) => {
                ui.initial_focus(id);
                if std::mem::take(&mut self.to_content) {
                    ui.request_focus(id);
                }
            }
            None => {
                let mut column = std::mem::take(&mut self.column);
                let at = column.shown_at(&self.column_view());
                self.column = column;
                ui.initial_focus(column_id.with(at as u64));
            }
        }

        match guide_action {
            Some(GuideAction::Watch(c)) => self.watch(cx, &c),
            Some(GuideAction::Details(d)) => {
                self.details = Some(*d);
                self.focus_details = true;
            }
            Some(GuideAction::Leave) => self.to_category = true,
            Some(GuideAction::Up) => ui.request_focus(tab_id(self.view)),
            None => {}
        }
        match card_action {
            Some(CardAction::Watch(c)) => self.watch(cx, &c),
            Some(CardAction::Details(c)) => {
                let on = c.on_at(now).cloned();
                self.details = Some((c, on));
                self.focus_details = true;
            }
            Some(CardAction::Leave) => self.to_category = true,
            None => {}
        }
        if self.to_category {
            ui.request_repaint();
        }

        // A programme's details.
        let panel_id = ui.id("details");
        if let Some(d) = &self.details {
            self.details_shown = Some(d.clone());
        }
        let shown = self.details_shown.clone();
        let mut act = None;
        let panel = SidePanel::new().back_closes(false).show(ui, panel_id, self.details.is_some(), |ui, area| {
            if let Some((c, p)) = &shown {
                act = details::show(ui, cx, panel_id.with("content"), area, c, p.as_ref(), &clock);
            }
        });
        if std::mem::take(&mut self.focus_details) && self.details.is_some() {
            ui.request_focus(details::watch_id(panel_id.with("content")));
        }
        if panel.closed {
            self.details = None;
        }
        if panel.progress <= 0.0 && self.details.is_none() {
            self.details_shown = None;
        }
        if let (Some(a), Some((c, _))) = (act, self.details.clone()) {
            match a {
                DetailsAction::Watch => {
                    self.details = None;
                    self.watch(cx, &c);
                }
                DetailsAction::Favourite => self.toggle_favourite(cx, &c),
            }
        }

        // The keyboard over the foot while the search is typed into.
        if self.typing {
            let keyboard = ui.id("keyboard");
            let r = Keyboard::new().action_label("Search").show(ui, keyboard, &mut self.keyboard, &mut self.query);
            if r.done {
                self.typing = false;
                self.to_content = true;
            } else if r.closed {
                self.typing = false;
            }
        }

        // A favourite that wasn't saved, at the foot for 4 s.
        if let Some((text, at)) = &self.notice {
            let age = cx.now - at;
            if age >= NOTICE_S {
                self.notice = None;
            } else {
                notice(ui, text);
                ui.repaint_after(NOTICE_S - age);
            }
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if self.details.take().is_some() {
            return true;
        }
        if std::mem::take(&mut self.typing) {
            return true;
        }
        let in_content = match self.view {
            View::Guide => self.guide.focused,
            View::Channels => self.cards.focused,
        };
        if self.view == View::Guide
            && let (Some(pages), Some(programs)) = (self.pages.as_ref(), self.programs.as_ref())
        {
            let clock = Clock::read(true);
            let listed = self.listed();
            let lineup = Lineup { pages, saved: self.saved.as_ref().and_then(|s| s.page.as_ref()) };
            let data = GuideData { pages: lineup, programs, category: &listed, clock };
            if self.guide.back(&data) {
                return true;
            }
        }
        // From the guide or the cards, Back goes to the category shown first; only from the
        // categories (or the tabs) does the shell take it to the rail (`LiveTvScreen.Back`).
        if in_content {
            self.to_category = true;
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::LiveChanged | AppEvent::Changed => {
                self.groups.invalidate();
                self.ask_live = true;
                if let Some(p) = self.pages.as_mut() {
                    p.stale();
                }
                if *event == AppEvent::LiveChanged
                    && let Some(p) = self.programs.as_mut()
                {
                    p.forget();
                    self.guide_channels.clear();
                }
            }
            // Back from the TV's Home or another input: the guide stays where it was left.
            AppEvent::Visible(true) => {
                self.groups.revalidate(cx.now);
                self.ask_live = true;
                self.hours24 = None;
            }
            AppEvent::Shown => {
                self.groups.revalidate(cx.now);
                self.ask_live = true;
                self.hours24 = None;
                if std::mem::take(&mut self.watching) {
                    self.back_from_player(cx);
                }
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.settled && !self.closed
    }

    fn modal(&self) -> bool {
        self.details.is_some() || self.typing
    }
}

/// What an empty category says (`Empty`).
fn empty_words(listed: &Category) -> (&'static str, String) {
    match listed {
        Category::Search(q) => {
            ("No channels match", format!("No channel's name, number or programme on now has “{}” in it.", q.trim()))
        }
        Category::Favorites => {
            ("No favourites yet", "Hold OK on a channel and add it to your favourites to find it here.".into())
        }
        Category::Recent => ("Nothing watched yet", "The channels you watch show here, the last first.".into()),
        Category::Group(_) => ("No channels in this group.", String::new()),
        Category::All => (
            "No channels yet",
            "Channels appear here once an administrator adds an IPTV provider in Pley's web app.".into(),
        ),
    }
}

/// The notice at the foot's centre: Surface3, 6 dp corners, 20 × 12 of padding, `Tv.Body`.
fn notice(ui: &mut Ui, text: &str) {
    let layout = ui.layout(text, &theme::BODY, &TextOpts::wrap(600.0, 2));
    let (w, h) = (layout.size.w + 40.0, layout.size.h + 24.0);
    let rect = Rect::new((theme::CANVAS_W - w) / 2.0, theme::CANVAS_H - 32.0 - h, w, h);
    ui.fill(rect, theme::SURFACE3, radius::CONTROL);
    ui.draw_text(Point::new(rect.x + 20.0, rect.y + 12.0), layout, theme::FG);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_categories_say_why_in_their_own_words() {
        assert_eq!(empty_words(&Category::Favorites).0, "No favourites yet");
        assert_eq!(empty_words(&Category::Recent).0, "Nothing watched yet");
        assert_eq!(empty_words(&Category::Group("Kids".into())), ("No channels in this group.", String::new()));
        assert_eq!(empty_words(&Category::All).0, "No channels yet");
        let (title, body) = empty_words(&Category::Search(" zzz ".into()));
        assert_eq!(title, "No channels match");
        assert_eq!(body, "No channel's name, number or programme on now has “zzz” in it.");
    }

    #[test]
    fn the_columns_line_up_with_the_android_page() {
        assert_eq!(CONTENT_TOP, 83.0);
        assert_eq!(CATEGORIES_X, 84.0);
        assert_eq!(CHANNELS_X, 310.0);
    }
}
