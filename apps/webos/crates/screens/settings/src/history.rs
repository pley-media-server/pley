//! Watch history (`TvHistoryScreen.kt`, `HistoryViewModel.kt`): the profile's plays, newest first,
//! 15 at a time with "Show more", each a row with its picture, title, what it is, when and on
//! which device. OK opens it; the cross at the row's end (Right) removes it at once, as a long
//! press of OK does, and focus moves to the play below it (or above, for the last). A removal the
//! server refuses comes back.
//!
//! The first 15 plays are saved for the server and profile (`CachedHistory`): they show at once,
//! and while the server is out of reach, until its answer replaces them.

use std::collections::HashSet;
use std::sync::Arc;

use pley_base::{Icon, ImageRequest, Point, Priority, Rect, TextOpts};
use pley_core::format::episode_label;
use pley_core::http::sentence;
use pley_core::model::{HistoryEntry, HistoryPage, ItemKind};
use pley_core::store::{Snapshots, snapshot_keys};
use pley_core::{ApiError, Promise};
use pley_i18n::t;
use pley_kit::look::{focus_progress, mix};
use pley_kit::{Button, SCROLL, first_art, placeholder, skeleton_block};
use pley_shell::{AppCx, AppEvent, Chrome, REVALIDATE, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{FocusOpts, Group, Id, ImageOpts, Ui, nudge};

/// Plays asked for at a time, and saved.
pub const PAGE: i64 = 15;
/// The most plays asked for at once when those shown are asked for again (`MAX_LIMIT`).
const MOST: i64 = 200;
/// A row's width (`ROW_WIDTH`), its least height, and the space between rows.
const ROW_W: f32 = 700.0;
const ROW_H: f32 = 64.0;
const ROW_GAP: f32 = 4.0;
/// The remove button's side, 12 after the row.
const CROSS: f32 = 40.0;
/// Where the rows start.
const TOP: f32 = 81.0;
/// Room under the last row (the list's bottom padding).
const BOTTOM: f32 = 120.0;

/// A unit a time is worded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// The age below which each unit is used, and its seconds (the web's table, as Android's
/// `Times.kt` has it).
const UNITS: [(i64, Unit, i64); 7] = [
    (60, Unit::Second, 1),
    (3_600, Unit::Minute, 60),
    (86_400, Unit::Hour, 3_600),
    (86_400 * 7, Unit::Day, 86_400),
    (86_400 * 30, Unit::Week, 86_400 * 7),
    (86_400 * 365, Unit::Month, 86_400 * 30),
    (i64::MAX, Unit::Year, 86_400 * 365),
];

/// Unix seconds `ts` worded from `now` in the language shown: "now", "3 minutes ago",
/// "yesterday", "last week", "in 2 days": the largest unit that fits, as `Intl.RelativeTimeFormat`
/// with numeric "auto" says it (Android's `formatRelative`). "never" without a time.
pub fn relative_time(ts: i64, now: i64) -> String {
    if ts == 0 {
        return t!("history.time.never").into();
    }
    // The time is the server's: any i64, so nothing here may overflow.
    let diff = ts.saturating_sub(now);
    let (_, unit, seconds) =
        UNITS.iter().copied().find(|u| diff.unsigned_abs() < u.0.unsigned_abs()).unwrap_or(UNITS[6]);
    // Java's Math.round: halves go up, toward positive infinity (a float casts back saturating).
    let value = (diff as f64 / seconds as f64 + 0.5).floor() as i64;
    let said = match (value, unit) {
        // Only seconds come to none: a unit is used from one of it on.
        (0, _) => t!("history.time.now"),
        (-1, Unit::Day) => t!("history.time.yesterday"),
        (-1, Unit::Week) => t!("history.time.lastWeek"),
        (-1, Unit::Month) => t!("history.time.lastMonth"),
        (-1, Unit::Year) => t!("history.time.lastYear"),
        (1, Unit::Day) => t!("history.time.tomorrow"),
        (1, Unit::Week) => t!("history.time.nextWeek"),
        (1, Unit::Month) => t!("history.time.nextMonth"),
        (1, Unit::Year) => t!("history.time.nextYear"),
        _ => "",
    };
    if !said.is_empty() {
        return said.into();
    }
    let n = value.unsigned_abs();
    match (value < 0, unit) {
        (true, Unit::Second) => t!("history.time.agoSecond", count = n),
        (true, Unit::Minute) => t!("history.time.agoMinute", count = n),
        (true, Unit::Hour) => t!("history.time.agoHour", count = n),
        (true, Unit::Day) => t!("history.time.agoDay", count = n),
        (true, Unit::Week) => t!("history.time.agoWeek", count = n),
        (true, Unit::Month) => t!("history.time.agoMonth", count = n),
        (true, Unit::Year) => t!("history.time.agoYear", count = n),
        (false, Unit::Second) => t!("history.time.inSecond", count = n),
        (false, Unit::Minute) => t!("history.time.inMinute", count = n),
        (false, Unit::Hour) => t!("history.time.inHour", count = n),
        (false, Unit::Day) => t!("history.time.inDay", count = n),
        (false, Unit::Week) => t!("history.time.inWeek", count = n),
        (false, Unit::Month) => t!("history.time.inMonth", count = n),
        (false, Unit::Year) => t!("history.time.inYear", count = n),
    }
}

/// A play as its row shows it (`historyRow`).
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryRow {
    pub entry_id: i64,
    /// The show for an episode, else the title.
    pub title: String,
    /// "S1 · E3 · Pilot", an artist and album, or the year.
    pub subtitle: String,
    pub time: String,
    pub device: Option<String>,
    /// The pictures to try, in order: the item's poster, then its parent's and grandparent's.
    pub art: Vec<String>,
    /// Albums and tracks have square art; the rest posters.
    pub square: bool,
    /// The page it opens: the album for a track (the track highlighted).
    pub opens: Route,
    /// The line under the title, made once.
    line: String,
}

impl HistoryRow {
    /// The line under the title: what it is, when, and where.
    pub fn line(&self) -> &str {
        &self.line
    }
}

/// `entry` as its row shows it, its time worded from `now` (Unix seconds).
pub fn history_row(entry: &HistoryEntry, now: i64) -> HistoryRow {
    let item = &entry.item;
    let (title, subtitle) = match item.kind {
        ItemKind::Episode => (
            item.grandparent_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| item.title.clone()),
            [episode_label(item.parent_index, item.index), item.title.clone()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        ItemKind::Track => (
            item.title.clone(),
            [item.grandparent_title.clone(), item.parent_title.clone()]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        _ => (item.title.clone(), item.year.map(|y| y.to_string()).unwrap_or_default()),
    };
    let opens = match item.kind {
        ItemKind::Track => match item.parent_id {
            Some(album) => Route::Album { id: album, track_id: Some(item.id) },
            None => Route::Item { id: item.id },
        },
        ItemKind::Album => Route::Album { id: item.id, track_id: None },
        ItemKind::Artist => Route::Artist { id: item.id },
        _ => Route::Item { id: item.id },
    };
    let art = [
        item.poster_url.clone().filter(|p| !p.is_empty()),
        item.parent_id.map(|id| format!("/api/items/{id}/images/poster")),
        item.grandparent_id.map(|id| format!("/api/items/{id}/images/poster")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let time = relative_time(entry.viewed_at, now);
    let device = entry.device.clone().filter(|d| !d.trim().is_empty());
    let line = [subtitle.as_str(), time.as_str(), device.as_deref().unwrap_or("")]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    HistoryRow {
        entry_id: entry.id,
        title,
        subtitle,
        time,
        device,
        art,
        square: matches!(item.kind, ItemKind::Track | ItemKind::Album | ItemKind::Artist),
        opens,
        line,
    }
}

/// "1 play", "12 plays".
pub fn plays(total: i64) -> String {
    t!("history.plays", count = total)
}

/// Seconds since 1970 now.
fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// What a promise holds once its task is done: `None` while it runs; a task that stopped without
/// an answer counts as a failure.
fn outcome<T>(p: &mut Promise<Result<T, ApiError>>) -> Option<Result<T, ApiError>> {
    match p.take() {
        Some(a) => Some(a),
        None if p.failed() => Some(Err(ApiError::Unreadable("the request stopped".into()))),
        None => None,
    }
}

/// Where this profile's saved answers are kept: the server, the profile, the snapshots.
fn saved_at(cx: &AppCx) -> Option<(String, i64, Arc<Snapshots>)> {
    Some((cx.state.server()?.to_string(), cx.state.profile()?, cx.snapshots().clone()))
}

/// Saves the first plays of `page` (off the main thread) for the next visit.
fn save_head(cx: &AppCx, page: &HistoryPage) {
    let Some((server, profile, snapshots)) = saved_at(cx) else { return };
    let head = HistoryPage { total: page.total, entries: page.entries.iter().take(PAGE as usize).cloned().collect() };
    cx.tasks().detach(move || {
        if let Err(e) = snapshots.put(&server, profile, snapshot_keys::HISTORY_HEAD, &head) {
            log::warn!("the watch history wasn't saved: {e}");
        }
    });
}

/// Takes play `entry` out of the saved first plays (the server has forgotten it).
fn forget_saved(snapshots: &Snapshots, server: &str, profile: i64, entry: i64) {
    let key = snapshot_keys::HISTORY_HEAD;
    let Some(mut page) = snapshots.get::<HistoryPage>(server, profile, key) else { return };
    let before = page.entries.len();
    page.entries.retain(|e| e.id != entry);
    if page.entries.len() < before {
        page.total = (page.total - 1).max(0);
        if let Err(e) = snapshots.put(server, profile, key, &page) {
            log::warn!("the watch history wasn't saved: {e}");
        }
    }
}

/// A removal waiting for the server, with what to put back if it refuses.
struct Removing {
    promise: Promise<Result<(), ApiError>>,
    row: HistoryRow,
    at: usize,
}

/// The watch history page.
#[derive(Default)]
pub struct HistoryScreen {
    rows: Option<Vec<HistoryRow>>,
    total: i64,
    /// The first page failed, with nothing shown.
    error: Option<ApiError>,
    /// What went wrong with Show more or a removal, in the header.
    problem: Option<String>,
    loading: Option<Promise<Result<HistoryPage, ApiError>>>,
    /// Whether `loading` adds to the list (Show more) rather than replacing it.
    more: bool,
    /// The plays were to be asked for again while a load was under way: they are once it is done.
    refresh_due: bool,
    /// The saved first plays, being read.
    saved: Option<Promise<Option<HistoryPage>>>,
    /// The server, sign-in and profile the plays are of (`AppState::generation`); `None` before
    /// the first frame.
    generation: Option<u64>,
    /// When the server last gave the first plays.
    answered_at: Option<f64>,
    removing: Vec<Removing>,
    /// The pictures of each row, made once per row.
    art: Vec<(i64, Vec<ImageRequest>)>,
    scroll: f32,
    index: usize,
    drawn: (usize, usize),
    started: bool,
}

impl HistoryScreen {
    /// The page: the saved first plays at once, then the server's.
    pub fn new() -> HistoryScreen {
        HistoryScreen::default()
    }

    /// The rows shown, once loaded.
    pub fn rows(&self) -> Option<&[HistoryRow]> {
        self.rows.as_deref()
    }

    /// The id of an entry's row (`remove`: its cross), under the page's `root`.
    pub fn row_id(root: Id, entry_id: i64, remove: bool) -> Id {
        root.with("rows").with(entry_id).with(if remove { "remove" } else { "open" })
    }

    /// Shows the saved first plays while the server is asked for them.
    fn start(&mut self, cx: &mut AppCx) {
        if let Some((server, profile, snapshots)) = saved_at(cx) {
            let key = snapshot_keys::HISTORY_HEAD;
            self.saved = Some(cx.tasks().spawn(move || snapshots.get::<HistoryPage>(&server, profile, key)));
        }
        self.load(cx, 0, PAGE);
    }

    fn load(&mut self, cx: &mut AppCx, offset: i64, limit: i64) {
        self.more = offset > 0;
        self.loading = Some(cx.spawn(move |api| api.history(offset, limit)));
    }

    /// Asks again for as many plays as show (`CachedHistory`'s fetch), once no load is under way.
    fn refresh(&mut self, cx: &mut AppCx) {
        if self.generation.is_none() {
            // Not drawn yet: the first frame asks.
            return;
        }
        if self.loading.is_some() {
            self.refresh_due = true;
            return;
        }
        // Plays being removed are still the server's until it answers: they are asked for too.
        let have = (self.rows.as_ref().map_or(0, Vec::len) + self.removing.len()) as i64;
        self.load(cx, 0, have.clamp(PAGE, MOST));
    }

    /// Shows `page`'s plays after those shown (Show more) or in their place. Plays being removed
    /// stay out while the server hasn't answered, whatever it listed meanwhile.
    fn show(&mut self, page: &HistoryPage, more: bool) {
        let now = now_secs();
        let removing: HashSet<i64> = self.removing.iter().map(|r| r.row.entry_id).collect();
        let fresh = page.entries.iter().filter(|e| !removing.contains(&e.id)).map(|e| history_row(e, now));
        let rows = self.rows.get_or_insert_with(Vec::new);
        if more {
            let have: HashSet<i64> = rows.iter().map(|r| r.entry_id).collect();
            rows.extend(fresh.filter(|r| !have.contains(&r.entry_id)));
        } else {
            *rows = fresh.collect();
        }
        let pending = page.entries.iter().filter(|e| removing.contains(&e.id)).count() as i64;
        self.total = (page.total - pending).max(rows.len() as i64);
    }

    fn collect(&mut self, cx: &mut AppCx) {
        if let Some(p) = self.saved.as_mut()
            && (p.ready() || p.failed())
        {
            let saved = p.take().flatten();
            self.saved = None;
            // The server may have answered first: its answer wins.
            if self.rows.is_none()
                && let Some(page) = saved
            {
                self.show(&page, false);
                self.error = None;
            }
        }
        if let Some(answer) = self.loading.as_mut().and_then(outcome) {
            self.loading = None;
            let more = self.more;
            match answer {
                Ok(page) => {
                    cx.state.server_answered();
                    if !more {
                        self.answered_at = Some(cx.now);
                        self.saved = None;
                        save_head(cx, &page);
                    }
                    self.show(&page, more);
                    self.error = None;
                    self.problem = None;
                }
                Err(e) if e.cancelled() => {}
                Err(e) => {
                    cx.state.report_failure(&e);
                    if more {
                        self.problem = Some(sentence(&e.to_string()));
                    } else if self.rows.is_none() {
                        self.error = Some(e);
                    }
                    // Plays already shown stay when asking again fails, as Android keeps its
                    // saved page: the shell says when the server is out of reach.
                }
            }
        }
        if self.refresh_due && self.loading.is_none() {
            self.refresh_due = false;
            self.refresh(cx);
        }
        let mut i = 0;
        while i < self.removing.len() {
            let Some(answer) = outcome(&mut self.removing[i].promise) else {
                i += 1;
                continue;
            };
            let r = self.removing.remove(i);
            if let Err(e) = answer {
                // Refused: the play comes back where it was (once: an answer since may hold it).
                let rows = self.rows.get_or_insert_with(Vec::new);
                if !rows.iter().any(|x| x.entry_id == r.row.entry_id) {
                    rows.insert(r.at.min(rows.len()), r.row);
                    self.total += 1;
                }
                if !e.cancelled() {
                    cx.state.report_failure(&e);
                    self.problem = Some(sentence(&e.to_string()));
                }
            }
        }
    }

    /// Removes row `at` at once and asks the server to forget it; focus goes to the same place on
    /// the play below, else the one above.
    fn remove(&mut self, ui: &mut Ui, cx: &mut AppCx, root: Id, at: usize, from_cross: bool) {
        let Some(rows) = self.rows.as_mut() else { return };
        if at >= rows.len() {
            return;
        }
        let row = rows.remove(at);
        self.total = (self.total - 1).max(0);
        let near = rows.get(at).or_else(|| at.checked_sub(1).and_then(|i| rows.get(i)));
        if let Some(near) = near {
            ui.request_focus(HistoryScreen::row_id(root, near.entry_id, from_cross));
        }
        let id = row.entry_id;
        let saved = saved_at(cx);
        let promise = cx.spawn(move |api| {
            api.remove_history(id)?;
            // The saved first plays forget it too, so the next visit doesn't show it first.
            if let Some((server, profile, snapshots)) = saved {
                forget_saved(&snapshots, &server, profile, id);
            }
            Ok(())
        });
        self.removing.push(Removing { promise, row, at });
    }

    fn header(&self, ui: &mut Ui) {
        let title = ui.text(
            Point::new(theme::GUTTER, theme::SAFE_Y),
            t!("history.title"),
            &theme::PAGE_TITLE,
            &TextOpts::default(),
            theme::FG,
        );
        let bottom = theme::SAFE_Y + title.h;
        if self.rows.is_some() {
            let x = theme::GUTTER + title.w + 16.0;
            let y = bottom - 6.0 - theme::CAPTION.line_height;
            ui.text(Point::new(x, y), &plays(self.total), &theme::CAPTION, &TextOpts::default(), theme::MUTED);
        }
        if let Some(problem) = &self.problem {
            let w = 360.0;
            let opts = TextOpts::line(w).aligned(pley_base::Align::End);
            let x = theme::CANVAS_W - theme::SAFE_X - w;
            ui.text(
                Point::new(x, bottom - theme::LABEL.line_height),
                problem,
                &theme::LABEL,
                &opts,
                theme::DANGER_TEXT,
            );
        }
    }

    fn list(&mut self, ui: &mut Ui, cx: &mut AppCx, root: Id) {
        let Some(rows) = self.rows.as_ref() else { return };
        let n = rows.len();
        let pitch = ROW_H + ROW_GAP;
        let more = (n as i64) < self.total;
        let more_top = n as f32 * pitch + 12.0;
        let content = if more { more_top + theme::BUTTON_HEIGHT } else { n as f32 * pitch - ROW_GAP } + BOTTOM;
        let view_h = theme::CANVAS_H - TOP;
        let max = (content - view_h).max(0.0);
        let list_id = root.with("rows");

        // Which play has focus (by the ids drawn last frame), and the scroll that keeps it in view.
        let focused = ui.focused();
        let (d0, d1) = self.drawn;
        let mut focused_now = false;
        if let Some(f) = focused {
            if let Some(i) = (d0..d1.min(n)).find(|&i| {
                let e = rows[i].entry_id;
                f == HistoryScreen::row_id(root, e, false) || f == HistoryScreen::row_id(root, e, true)
            }) {
                self.index = i;
                focused_now = true;
            } else if f == list_id.with("more") {
                self.index = n;
                focused_now = true;
            }
        }
        let top = self.index as f32 * pitch;
        let bottom_of = if self.index >= n { more_top + theme::BUTTON_HEIGHT } else { top + ROW_H };
        let anchored = (top - view_h * theme::SCROLL_ANCHOR).clamp(0.0, max);
        if !self.started {
            self.scroll = 0.0;
            self.started = true;
        } else if focused_now && ui.pointer_mode() {
            self.scroll = nudge(self.scroll, top, bottom_of, 0.0, view_h, 0.0, max);
        } else if focused_now {
            self.scroll = anchored;
        }
        self.scroll = self.scroll.clamp(0.0, max);
        let shown = ui.animate(list_id.with("scroll"), self.scroll, SCROLL);

        let first = ((shown / pitch).floor() as usize).min(n.saturating_sub(1));
        let past = (((shown + view_h) / pitch).ceil() as usize).min(n);
        let d0 = first.saturating_sub(3).min(self.index.saturating_sub(3));
        let d1 = (past + 3).max(self.index + 4).min(n);
        self.drawn = (d0, d1);

        let px = ui.image_px(48.0);
        for row in &rows[d0..d1] {
            if !self.art.iter().any(|(id, _)| *id == row.entry_id) {
                let reqs = row.art.iter().filter_map(|p| cx.image(p, px)).collect();
                self.art.push((row.entry_id, reqs));
            }
        }
        if self.art.len() > 4 * n.max(PAGE as usize) {
            self.art.retain(|(id, _)| rows.iter().any(|r| r.entry_id == *id));
        }

        let mut opened = None;
        let mut remove = None;
        let mut show_more = false;
        let view = Rect::new(0.0, TOP, theme::CANVAS_W, view_h);
        ui.clip(view, |ui| {
            ui.group(list_id, Group::column().edge(pley_ui::Dir::Up, pley_ui::Edge::Stop), |ui| {
                for (i, row) in rows.iter().enumerate().take(d1).skip(d0) {
                    let y = TOP + i as f32 * pitch - shown;
                    let art = self.art.iter().find(|(id, _)| *id == row.entry_id).map_or(&[][..], |(_, a)| a);
                    // Down from a cross lands on the cross below: each row is entered by geometry.
                    let group = Group::row().no_memory().edge(pley_ui::Dir::Right, pley_ui::Edge::Stop);
                    ui.group(list_id.with(row.entry_id), group, |ui| {
                        let r = history_line(ui, HistoryScreen::row_id(root, row.entry_id, false), y, row, art);
                        if i == 0 {
                            ui.initial_focus(r.id);
                        }
                        if r.activated {
                            opened = Some(row.opens.clone());
                        }
                        if r.long_pressed {
                            remove = Some((i, false));
                        }
                        let cross = remove_button(ui, HistoryScreen::row_id(root, row.entry_id, true), y);
                        if cross.activated {
                            remove = Some((i, true));
                        }
                    });
                }
                if more && d1 == n {
                    let label = if self.loading.is_some() { t!("history.loading") } else { t!("history.more") };
                    let at = Point::new(theme::GUTTER, TOP + more_top - shown);
                    let b = Button::pill(label).icon(Icon::ChevronDown).enabled(self.loading.is_none());
                    if b.show(ui, list_id.with("more"), at).activated {
                        show_more = true;
                    }
                }
            });
        });
        if let Some(route) = opened {
            cx.push(route);
        }
        if let Some((i, from_cross)) = remove {
            self.remove(ui, cx, root, i, from_cross);
        }
        if show_more && self.loading.is_none() {
            let have = self.rows.as_ref().map_or(0, Vec::len) as i64;
            self.load(cx, have, PAGE);
        }
    }
}

/// A play's row: 700 wide, at least 64 tall, padding 12 × 6, the picture in a 48 dp column, 16,
/// then the title (`Tv.Body`) over what, when and where (`Tv.Caption`). It never scales; focused
/// it takes an Accent fill. Long OK removes it.
fn history_line(ui: &mut Ui, id: Id, y: f32, row: &HistoryRow, art: &[ImageRequest]) -> pley_ui::Response {
    let rect = Rect::new(theme::GUTTER, y, ROW_W, ROW_H);
    let r = ui.focusable_with(id, rect, FocusOpts::default().long_press());
    let p = focus_progress(ui, id, r.focused);
    ui.fill(rect, mix(pley_base::Color::TRANSPARENT, theme::ACCENT, p), radius::FIELD);
    let (w, h) = if row.square { (48.0, 48.0) } else { (34.0, 51.0) };
    let cell = Rect::new(rect.x + 12.0 + (48.0 - w) / 2.0, rect.y + (ROW_H - h) / 2.0, w, h);
    ui.fill(cell, theme::SURFACE2, radius::BADGE);
    let shown = first_art(ui, art, Priority::Visible);
    if !shown.as_ref().is_some_and(|s| s.opaque()) {
        placeholder(ui, cell, &row.title, "", radius::BADGE);
    }
    if let Some(s) = shown {
        ui.draw_image(cell, &s, &ImageOpts::cover().rounded(radius::BADGE));
    }
    let x = rect.x + 12.0 + 48.0 + 16.0;
    let tw = rect.right() - 12.0 - x;
    let block = theme::BODY.line_height + theme::CAPTION.line_height;
    let ty = rect.y + (ROW_H - block) / 2.0;
    let fg = mix(theme::FG, theme::ON_ACCENT, p);
    let second = mix(theme::MUTED, theme::ON_ACCENT, p);
    ui.text(Point::new(x, ty), &row.title, &theme::BODY, &TextOpts::line(tw), fg);
    ui.text(Point::new(x, ty + theme::BODY.line_height), row.line(), &theme::CAPTION, &TextOpts::line(tw), second);
    r
}

/// The 40 dp round remove button after a row: Surface3 with a Muted cross, Accent when focused.
fn remove_button(ui: &mut Ui, id: Id, y: f32) -> pley_ui::Response {
    let rect = Rect::new(theme::GUTTER + ROW_W + 12.0, y + (ROW_H - CROSS) / 2.0, CROSS, CROSS);
    let r = ui.focusable(id, rect);
    let p = focus_progress(ui, id, r.focused);
    ui.fill(rect, mix(theme::SURFACE3, theme::ACCENT, p), CROSS / 2.0);
    let icon = Rect::new(rect.x + 10.0, rect.y + 10.0, 20.0, 20.0);
    ui.icon(icon, Icon::X, mix(theme::MUTED, theme::ON_ACCENT, p));
    r
}

/// A page-sized message with one focused button (`Message` in `TvHistoryScreen.kt`): x 96, 60
/// under the header, the title in `Tv.Title`, 10, the text in `Tv.Body` Muted, then the pill 32
/// below. Licences says it has nothing to show the same way.
pub(crate) fn message(ui: &mut Ui, id: Id, title: &str, body: &str, action: &str) -> bool {
    let x = theme::GUTTER;
    let w = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
    let mut y = TOP + 60.0;
    y += ui.text(Point::new(x, y), title, &theme::TITLE, &TextOpts::wrap(w, 2), theme::FG).h + 10.0;
    y += ui.text(Point::new(x, y), body, &theme::BODY, &TextOpts::wrap(w, 0), theme::MUTED).h + 32.0;
    ui.group(id, pley_kit::bar_group(), |ui| {
        let bid = ui.id("action");
        ui.initial_focus(bid);
        Button::pill(action).show(ui, bid, Point::new(x, y)).activated
    })
}

impl Screen for HistoryScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let generation = cx.state.generation();
        if self.generation != Some(generation) {
            // Shown for the first time, or for another server, sign-in or profile: start over.
            *self = HistoryScreen { generation: Some(generation), ..HistoryScreen::default() };
            self.start(cx);
        }
        self.collect(cx);
        let root = ui.scope_id();
        self.header(ui);
        match (&self.rows, &self.error) {
            (None, Some(e)) => {
                let title = if e.unreachable() { t!("history.unreachable") } else { t!("history.failed") };
                let body = e.problem();
                if message(ui, root.with("message"), title, &body, t!("history.retry")) {
                    self.error = None;
                    self.load(cx, 0, PAGE);
                }
            }
            (None, None) => {
                for i in 0..5 {
                    skeleton_block(ui, Rect::new(theme::GUTTER, TOP + i as f32 * 64.0, 560.0, 52.0), radius::FIELD);
                }
            }
            (Some(rows), _) if rows.is_empty() => {
                let (title, body) = (t!("history.emptyTitle"), t!("history.empty"));
                if message(ui, root.with("message"), title, body, t!("history.back")) {
                    cx.back();
                }
            }
            (Some(_), _) => self.list(ui, cx, root),
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            // Plays come and go elsewhere: the plays shown so far are asked for again.
            AppEvent::Changed => self.refresh(cx),
            // On top again: asked again when the answer is older than 30 s.
            AppEvent::Shown if self.answered_at.is_some_and(|t| cx.now - t > REVALIDATE) => self.refresh(cx),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.rows.is_some() || self.error.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::ItemSummary;

    #[test]
    fn times_are_worded_as_android_words_them() {
        // `FormatTest.relativeTimes`.
        let now = 1_700_000_000;
        assert_eq!(relative_time(now - 30, now), "30 seconds ago");
        assert_eq!(relative_time(now - 180, now), "3 minutes ago");
        assert_eq!(relative_time(now - 7200, now), "2 hours ago");
        assert_eq!(relative_time(now - 3 * 86_400, now), "3 days ago");
        assert_eq!(relative_time(now - 14 * 86_400, now), "2 weeks ago");
        assert_eq!(relative_time(now - 60 * 86_400, now), "2 months ago");
        assert_eq!(relative_time(now - 2 * 365 * 86_400, now), "2 years ago");
        assert_eq!(relative_time(now + 5 * 3600, now), "in 5 hours");
        assert_eq!(relative_time(now - 86_400, now), "yesterday");
        assert_eq!(relative_time(now - 7 * 86_400, now), "last week");
        assert_eq!(relative_time(now, now), "now");
        assert_eq!(relative_time(now - 60, now), "1 minute ago");
        assert_eq!(relative_time(0, now), "never");
        assert_eq!(relative_time(now + 86_400, now), "tomorrow");
        assert_eq!(relative_time(now - 400 * 86_400, now), "last year");
        assert_eq!(relative_time(now + 40 * 86_400, now), "next month");
    }

    #[test]
    fn times_are_worded_in_french_and_spanish() {
        use pley_i18n::{Language, with_language};
        let now = 1_700_000_000;
        with_language(Language::Fr, || {
            assert_eq!(relative_time(now - 180, now), "il y a 3 minutes");
            assert_eq!(relative_time(now - 60, now), "il y a 1 minute");
            assert_eq!(relative_time(now + 5 * 3600, now), "dans 5 heures");
            assert_eq!(relative_time(now - 86_400, now), "hier");
            assert_eq!(relative_time(now - 400 * 86_400, now), "l\u{2019}année dernière");
            assert_eq!(relative_time(now, now), "maintenant");
            assert_eq!(relative_time(0, now), "jamais");
            assert_eq!((plays(0), plays(2)), ("0 lecture".to_string(), "2 lectures".to_string()));
        });
        with_language(Language::Es, || {
            assert_eq!(relative_time(now - 14 * 86_400, now), "hace 2 semanas");
            assert_eq!(relative_time(now + 7 * 86_400, now), "la próxima semana");
            assert_eq!(relative_time(now - 30, now), "hace 30 segundos");
            assert_eq!((plays(1), plays(3)), ("1 reproducción".to_string(), "3 reproducciones".to_string()));
        });
    }

    #[test]
    fn any_time_the_server_gives_is_worded_without_overflowing() {
        let now = 1_700_000_000;
        assert!(relative_time(i64::MIN, now).ends_with(" years ago"));
        assert!(relative_time(-i64::MAX, now).ends_with(" years ago"));
        assert!(relative_time(i64::MAX, now).starts_with("in "));
        assert!(relative_time(i64::MAX, -now).starts_with("in "));
    }

    fn entry(kind: ItemKind, f: impl FnOnce(&mut ItemSummary)) -> HistoryEntry {
        let mut item = ItemSummary { id: 7, kind, title: "Pilot".into(), ..ItemSummary::default() };
        f(&mut item);
        HistoryEntry { id: 3, viewed_at: 1_000, device: Some("LG TV".into()), item, ..HistoryEntry::default() }
    }

    #[test]
    fn an_episode_shows_its_show_and_where_it_is() {
        let e = entry(ItemKind::Episode, |i| {
            i.grandparent_title = Some("The Show".into());
            i.parent_index = Some(1);
            i.index = Some(3);
            i.parent_id = Some(5);
            i.grandparent_id = Some(4);
        });
        let row = history_row(&e, 1_000 + 3 * 3600);
        assert_eq!(row.title, "The Show");
        assert_eq!(row.subtitle, "S1 · E3 · Pilot");
        assert_eq!(row.line(), "S1 · E3 · Pilot · 3 hours ago · LG TV");
        assert_eq!(row.opens, Route::Item { id: 7 });
        assert_eq!(row.art, ["/api/items/5/images/poster", "/api/items/4/images/poster"]);
        assert!(!row.square);
    }

    #[test]
    fn a_track_opens_its_album_on_it() {
        let e = entry(ItemKind::Track, |i| {
            i.title = "Song".into();
            i.parent_id = Some(61);
            i.parent_title = Some("Album".into());
            i.grandparent_title = Some("Artist".into());
        });
        let row = history_row(&HistoryEntry { device: None, ..e }, 1_000);
        assert_eq!((row.title.as_str(), row.subtitle.as_str()), ("Song", "Artist · Album"));
        assert_eq!(row.opens, Route::Album { id: 61, track_id: Some(7) });
        assert_eq!(row.line(), "Artist · Album · now");
        assert!(row.square);
    }

    #[test]
    fn a_movie_shows_its_year() {
        let row = history_row(&entry(ItemKind::Movie, |i| i.year = Some(1999)), 1_000);
        assert_eq!((row.title.as_str(), row.subtitle.as_str()), ("Pilot", "1999"));
        assert_eq!(plays(1), "1 play");
        assert_eq!(plays(22), "22 plays");
    }
}
