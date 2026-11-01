//! The programme guide (`TvGuide.kt`): what has focus described above the grid, channels down and
//! time across at 3.6 dp a minute (two hours in view), a time ruler, now marked by a line. The grid
//! is one focusable area; focus is a programme the guide moves itself:
//!
//! - Left and Right go to the programme before and after, the grid following by half hours; Left
//!   from the guide's first half hour (or on a channel without programmes) goes to the categories;
//! - Up and Down keep the time and change channel, Up from the first row going to the tabs above;
//!   CH+ and CH− move five rows;
//! - OK on a programme on now (or a channel without one) watches the channel, on another opens its
//!   details, as do holding OK, Menu and Info;
//! - Back away from now comes back to now first.
//!
//! The guide opens at the half hour before the one now is in and goes 48 hours from there; the
//! category's channels come 100 at a time as rows come near, and the programmes in six-hour chunks
//! for the rows near the view.

use pley_base::{Key, Point, Rect, TextOpts, Weight};
use pley_core::model::{LiveChannel, LiveProgram};
use pley_kit::SCROLL;
use pley_player::live::data::{GUIDE_SPAN, SLOT, channel_name, time_left};
use pley_player::live::{Category, GuideStore};
use pley_shell::AppCx;
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, FocusOpts, Id, Motion, Response, Ui};

use crate::clock::Clock;
use crate::draw::{BADGE_H, Well, channel_tile, line_mid, programme_badge};
use crate::lineup::Lineup;

/// The guide's scale: 3.6 dp a minute.
pub const DP_PER_MINUTE: f32 = 3.6;
const DP_PER_SECOND: f32 = DP_PER_MINUTE / 60.0;
/// The time in view: two hours.
pub const VIEW_S: i64 = 120 * 60;
/// A channel's row.
pub const ROW_H: f32 = 52.0;
/// The channels' column.
pub const CHANNEL_W: f32 = 160.0;
/// The channel cell's content starts this far in.
pub const CHANNEL_INSET: f32 = 10.0;
/// What has focus, described above the grid.
pub const INFO_H: f32 = 92.0;
/// The time ruler.
pub const RULER_H: f32 = 28.0;
/// Room under the last row.
pub const BOTTOM: f32 = 27.0;
/// CH+ and CH− move this many rows.
pub const PAGE: usize = 5;
/// Narrower than this a programme shows no text (Android's 70 px at the TV's 2×).
pub const MIN_TEXT: f32 = 35.0;
/// A programme begun before the left edge keeps its title in view, up to this much of it.
pub const KEEP_TITLE: f32 = 90.0;
/// Channels are loaded this many rows past those in view (`MARGIN`).
pub const MARGIN: usize = 20;
/// The page's right edge for the guide's content.
const END: f32 = theme::CANVAS_W - theme::SAFE_X;

/// The half hour `t` is in.
pub fn slot(t: i64) -> i64 {
    t.div_euclid(SLOT) * SLOT
}

/// Where the guide starts for `now`: the start of the half hour before the one now is in.
pub fn guide_start(now: i64) -> i64 {
    slot(now) - SLOT
}

/// The grid's left edge for `now`: the half hour before the one it is in, never before the guide.
pub fn left_for(now: i64, start: i64) -> i64 {
    (slot(now) - SLOT).max(start)
}

/// The programme on at `t`.
pub fn at(programs: Option<&[LiveProgram]>, t: i64) -> Option<&LiveProgram> {
    programs?.iter().find(|p| p.start <= t && p.stop > t)
}

/// The programmes overlapping `from`–`to`.
pub fn between(programs: Option<&[LiveProgram]>, from: i64, to: i64) -> impl Iterator<Item = &LiveProgram> {
    programs.unwrap_or(&[]).iter().filter(move |p| p.stop > from && p.start < to)
}

/// Where focus is in the guide: the row, the time, and the time at the grid's left edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub time: i64,
    pub left: i64,
}

impl Cursor {
    /// The cursor as the guide opens: the first row, now.
    pub fn new(now: i64, start: i64) -> Cursor {
        Cursor { row: 0, time: now, left: left_for(now, start) }
    }

    /// Puts the cursor at `time` (a programme's start, or a half hour), the grid following it.
    pub fn move_to(&mut self, time: i64, start: i64) {
        let end = start + GUIDE_SPAN;
        self.time = time.clamp(start, end - 1);
        if self.time < self.left {
            self.left = slot(self.time).max(start);
        } else if self.time >= self.left + VIEW_S - SLOT {
            self.left = (slot(self.time) - SLOT).clamp(start, end - VIEW_S);
        }
    }

    /// Back to the programme airing, the grid where it opened.
    pub fn back_to_now(&mut self, now: i64, start: i64) {
        self.time = now;
        self.left = left_for(now, start);
    }

    /// Whether Back has something to do here: the cursor off what airs, or the grid moved.
    pub fn away_from_now(&self, programs: Option<&[LiveProgram]>, now: i64, start: i64) -> bool {
        let off = match at(programs, self.time) {
            Some(p) => !p.airing_at(now),
            None => slot(self.time) != slot(now),
        };
        off || self.left != left_for(now, start)
    }

    /// Left: the programme before. True when it leaves the guide instead (from its first half hour,
    /// or a channel without programmes).
    pub fn step_left(&mut self, programs: Option<&[LiveProgram]>, start: i64) -> bool {
        let target = at(programs, self.time).map_or(self.time - SLOT, |p| p.start - 1);
        if target < start || programs.is_none_or(<[LiveProgram]>::is_empty) {
            return true;
        }
        let to = at(programs, target).map_or(slot(target), |p| p.start.max(slot(target)));
        self.move_to(to, start);
        false
    }

    /// Right: the programme after, up to the guide's end.
    pub fn step_right(&mut self, programs: Option<&[LiveProgram]>, start: i64) {
        let target = at(programs, self.time).map_or(slot(self.time) + SLOT, |p| p.stop);
        if target < start + GUIDE_SPAN {
            self.move_to(target, start);
        }
    }
}

/// What the guide asks of the page.
#[derive(Clone, Debug, PartialEq)]
pub enum GuideAction {
    Watch(Box<LiveChannel>),
    /// A programme's details (or the channel's, without one).
    Details(Box<(LiveChannel, Option<LiveProgram>)>),
    /// Left out of the guide: to the categories.
    Leave,
    /// Up from the first row: to the tabs.
    Up,
}

/// What the guide keeps between frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    pub cursor: Cursor,
    /// Where it opened (unix seconds), fixed while the page is open.
    pub start: i64,
    /// How far the rows are scrolled, in dp: where they are heading.
    scroll: f32,
    scrolled_for: Option<usize>,
    /// Where the rows showed last frame.
    shown_y: f32,
    /// Counts the jumps: a jump starts a new scroll, which takes its place at once.
    jumps: u32,
    /// The next scroll jumps instead of sliding (opening on a channel).
    snap: bool,
    /// Whether the area had focus last frame.
    pub focused: bool,
}

/// What the guide shows this frame.
pub struct GuideData<'a> {
    pub pages: Lineup<'a>,
    pub programs: &'a GuideStore,
    pub category: &'a Category,
    pub clock: Clock,
}

/// What the guide did this frame.
#[derive(Clone, Debug, Default)]
pub struct GuideOut {
    pub action: Option<GuideAction>,
    /// The rows on screen: first and past the last.
    pub rows: (usize, usize),
    pub response: Option<Response>,
}

impl Guide {
    /// A guide opening now.
    pub fn new(now: i64) -> Guide {
        let start = guide_start(now);
        Guide {
            cursor: Cursor::new(now, start),
            start,
            scroll: 0.0,
            scrolled_for: None,
            shown_y: 0.0,
            jumps: 0,
            snap: true,
            focused: false,
        }
    }

    /// Opens on row `row` (the channel watched last), at now.
    pub fn land(&mut self, row: usize, now: i64) {
        self.cursor.row = row;
        self.cursor.back_to_now(now, self.start);
        self.scrolled_for = None;
        self.snap = true;
    }

    /// Back while the guide has focus: to now when away from it; whether it did.
    pub fn back(&mut self, data: &GuideData) -> bool {
        let channel = data.pages.at(self.cursor.row as i64);
        let programs = channel.and_then(|c| data.programs.programs(c.id));
        if !self.focused || !self.cursor.away_from_now(programs, data.clock.now, self.start) {
            return false;
        }
        self.cursor.back_to_now(data.clock.now, self.start);
        true
    }

    /// The rows' area within the guide's `area`.
    pub fn rows_rect(area: Rect) -> Rect {
        let top = area.y + INFO_H + RULER_H;
        Rect::new(area.x, top, END - area.x, theme::CANVAS_H - top)
    }

    /// Draws the guide in `area` (from the content's top to the screen's foot) and handles its keys.
    pub fn show(&mut self, ui: &mut Ui, cx: &AppCx, id: Id, area: Rect, data: &GuideData) -> GuideOut {
        let mut out = GuideOut::default();
        let total = data.pages.total().unwrap_or(0).max(0) as usize;
        let now = data.clock.now;
        self.cursor.row = self.cursor.row.min(total.saturating_sub(1));
        let rows = Guide::rows_rect(area);
        let view_h = rows.h;

        // The focusable area, and its keys.
        let opts = FocusOpts::default().long_press().takes(Dirs::ALL).click_at();
        let r = ui.focusable_with(id, rows, opts);
        self.focused = r.focused;
        out.response = Some(r);
        if r.clicked
            && let Some(p) = r.pointer
        {
            self.point_at(p, rows, data, total);
        }
        let channel = data.pages.at(self.cursor.row as i64).cloned();
        let programs = |c: Option<&LiveChannel>| c.and_then(|c| data.programs.programs(c.id));
        if r.focused {
            for k in ui.keys().to_vec() {
                if !k.pressed {
                    if k.key == Key::Info
                        && let Some(c) = &channel
                        && let Some(p) = at(programs(Some(c)), self.cursor.time)
                    {
                        out.action = Some(GuideAction::Details(Box::new((c.clone(), Some(p.clone())))));
                    }
                    continue;
                }
                let row_programs = programs(data.pages.at(self.cursor.row as i64));
                match k.key {
                    Key::Left => {
                        if channel.is_none() || self.cursor.step_left(row_programs, self.start) {
                            out.action = Some(GuideAction::Leave);
                        }
                    }
                    Key::Right => self.cursor.step_right(row_programs, self.start),
                    Key::Up if self.cursor.row == 0 => out.action = Some(GuideAction::Up),
                    Key::Up => self.cursor.row -= 1,
                    Key::Down => self.cursor.row = (self.cursor.row + 1).min(total.saturating_sub(1)),
                    Key::ChannelUp if !k.repeat => self.cursor.row = self.cursor.row.saturating_sub(PAGE),
                    Key::ChannelDown if !k.repeat => {
                        self.cursor.row = (self.cursor.row + PAGE).min(total.saturating_sub(1));
                    }
                    _ => continue,
                }
                ui.request_repaint();
            }
            let current = channel.as_ref().and_then(|c| at(programs(Some(c)), self.cursor.time).cloned());
            if let Some(c) = &channel {
                if r.activated {
                    let watch = current.as_ref().is_none_or(|p| p.airing_at(now));
                    out.action = Some(if watch {
                        GuideAction::Watch(Box::new(c.clone()))
                    } else {
                        GuideAction::Details(Box::new((c.clone(), current.clone())))
                    });
                } else if (r.long_pressed || r.menu) && current.is_some() {
                    out.action = Some(GuideAction::Details(Box::new((c.clone(), current.clone()))));
                }
            }
        }

        // The rows follow focus: the focused row second from the top when it reaches an edge.
        let max = (total as f32 * ROW_H + BOTTOM - view_h).max(0.0);
        if self.scrolled_for != Some(self.cursor.row) {
            let first = (self.shown_y / ROW_H).floor() as usize;
            let last = ((self.shown_y + view_h - 1.0) / ROW_H).floor() as usize;
            let row = self.cursor.row;
            if self.scrolled_for.is_none() || row <= first || row >= last {
                self.scroll = (row.saturating_sub(1) as f32 * ROW_H).clamp(0.0, max);
            }
            self.scrolled_for = Some(row);
        }
        self.scroll = self.scroll.clamp(0.0, max);
        if std::mem::take(&mut self.snap) {
            self.jumps = self.jumps.wrapping_add(1);
        }
        let shown_y = ui.animate(id.with(("y", self.jumps)), self.scroll, SCROLL);
        self.shown_y = shown_y;
        let x_target = (self.cursor.left - self.start) as f32 * DP_PER_SECOND;
        let shown_x = ui.animate(id.with("x"), x_target, Motion::OVERLAY);

        let first = ((shown_y / ROW_H).floor().max(0.0) as usize).min(total);
        let past = (((shown_y + view_h) / ROW_H).ceil().max(0.0) as usize).min(total);
        out.rows = (first, past);

        let current_channel = channel.as_ref();
        let current = current_channel.and_then(|c| at(programs(Some(c)), self.cursor.time));
        self.info(ui, cx, area, data, current_channel, current);
        self.ruler(ui, area, shown_x, &data.clock);

        let grid_x = area.x + CHANNEL_W;
        let grid = Rect::new(grid_x, rows.y, END - grid_x, rows.h);
        ui.clip(Rect::new(area.x, rows.y, theme::CANVAS_W - area.x, rows.h), |ui| {
            for i in first.saturating_sub(1)..(past + 1).min(total) {
                let y = rows.y + i as f32 * ROW_H - shown_y;
                let row_focused = r.focused && i == self.cursor.row;
                match data.pages.at(i as i64) {
                    Some(c) => {
                        let focused_start = row_focused.then(|| current.map_or(-1, |p| p.start));
                        self.row(ui, cx, area, grid, y, c, data, shown_x, focused_start);
                    }
                    None => {
                        ui.fill(
                            Rect::new(area.x, y + 4.0, CHANNEL_W - 8.0, ROW_H - 8.0),
                            theme::SURFACE2,
                            radius::CONTROL,
                        );
                        let x = area.x + CHANNEL_W + 2.0;
                        ui.fill(Rect::new(x, y + 4.0, END - x, ROW_H - 8.0), theme::SURFACE2, radius::CONTROL);
                    }
                }
            }
        });
        // Now, down the grid.
        let now_x = grid_x + (now - self.start) as f32 * DP_PER_SECOND - shown_x;
        ui.clip(grid, |ui| ui.fill(Rect::new(now_x - 1.0, rows.y, 2.0, rows.h), theme::ACCENT.alpha(0.85), 0.0));
        out
    }

    /// The programme under the pointer takes the cursor (a click there is its OK).
    fn point_at(&mut self, p: Point, rows: Rect, data: &GuideData, total: usize) {
        let shown = self.scroll;
        let row = ((p.y - rows.y + shown) / ROW_H).floor();
        if row < 0.0 || row as usize >= total {
            return;
        }
        self.cursor.row = row as usize;
        let grid_x = rows.x + CHANNEL_W;
        if p.x >= grid_x {
            let t = self.cursor.left + ((p.x - grid_x) / DP_PER_SECOND) as i64;
            let programs = data.pages.at(self.cursor.row as i64).and_then(|c| data.programs.programs(c.id));
            self.cursor.time = at(programs, t).map_or(t, |p| p.start.max(self.start));
        }
    }

    /// What has focus, above the grid: its badge and title, the channel and times, what the guide
    /// says of it, and the channel's logo at the right (`Focused`).
    fn info(
        &self,
        ui: &mut Ui,
        cx: &AppCx,
        area: Rect,
        data: &GuideData,
        channel: Option<&LiveChannel>,
        program: Option<&LiveProgram>,
    ) {
        let Some(channel) = channel else { return };
        let clock = &data.clock;
        let name = channel_name(channel, Some(data.category));
        let top = area.y + 4.0;
        let logo = Rect::new(END - 120.0, top, 120.0, 68.0);
        channel_tile(ui, cx, logo, channel, Well::PAGE.named());
        let x = area.x;
        let width = logo.x - 24.0 - x;
        let title_h = theme::SECTION.line_height;
        let mut tx = x;
        if let Some(p) = program {
            let w = programme_badge(ui, Point::new(x, top + (title_h - BADGE_H) / 2.0), p);
            if w > 0.0 {
                tx += w + 8.0;
            }
        }
        let title = program.map_or(name.as_str(), |p| p.title.as_str());
        ui.text(Point::new(tx, top), title, &theme::SECTION, &TextOpts::line(x + width - tx), theme::FG);
        let mut meta: Vec<String> = Vec::new();
        let number = channel.number.clone().filter(|n| !n.is_empty());
        if let Some(p) = program {
            meta.push([number, Some(name.clone())].into_iter().flatten().collect::<Vec<_>>().join(" "));
            meta.push(format!("{} {}", clock.day(p.start), clock.span(p)));
            if p.airing_at(clock.now) {
                meta.push(time_left(p.stop, clock.now));
            }
            if let Some(e) = p.episode_label() {
                meta.push(e);
            }
        } else {
            meta.extend(number);
            meta.push(
                if channel.has_guide { "No information for this time" } else { "No guide for this channel" }.into(),
            );
        }
        let meta_y = top + title_h + 2.0;
        ui.text(Point::new(x, meta_y), &meta.join("  ·  "), &theme::LABEL, &TextOpts::line(width), theme::MUTED);
        let text = program.map(|p| {
            [p.subtitle.as_deref(), p.description.as_deref()]
                .into_iter()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" — ")
        });
        if let Some(text) = text.filter(|t| !t.is_empty()) {
            let y = meta_y + theme::LABEL.line_height + 6.0;
            ui.text(Point::new(x, y), &text, &theme::BODY, &TextOpts::wrap(width.min(560.0), 2), theme::MUTED);
        }
    }

    /// The day at the left over the channels, then a time every half hour (a midnight names its day).
    fn ruler(&self, ui: &mut Ui, area: Rect, shown_x: f32, clock: &Clock) {
        let top = area.y + INFO_H;
        let day = clock.day(self.cursor.left.max(self.start));
        line_mid(ui, &day, &theme::CAPTION, area.x + CHANNEL_INSET, top, RULER_H, CHANNEL_W - CHANNEL_INSET, theme::FG);
        let grid_x = area.x + CHANNEL_W;
        ui.clip(Rect::new(grid_x, top, END - grid_x, RULER_H), |ui| {
            let mut t = slot(self.cursor.left - VIEW_S);
            while t < self.cursor.left + 2 * VIEW_S {
                if t >= self.start {
                    let x = grid_x + (t - self.start) as f32 * DP_PER_SECOND - shown_x + 6.0;
                    let (text, color) =
                        if clock.midnight(t) { (clock.day(t), theme::FG) } else { (clock.time(t), theme::MUTED) };
                    let opts = TextOpts { max_width: None, ellipsis: false, ..TextOpts::default() };
                    let layout = ui.layout(&text, &theme::CAPTION, &opts);
                    crate::draw::text_mid(ui, x, top, RULER_H, layout, color);
                }
                t += SLOT;
            }
        });
    }

    /// One channel's row: its cell and its programmes, the one with focus in the accent
    /// (`GuideRow`). `focused_start` is the focused programme's start (−1 for none) on the focused row.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        ui: &mut Ui,
        cx: &AppCx,
        area: Rect,
        grid: Rect,
        y: f32,
        c: &LiveChannel,
        data: &GuideData,
        shown_x: f32,
        focused_start: Option<i64>,
    ) {
        let now = data.clock.now;
        let row_focused = focused_start.is_some();
        ui.fill(Rect::new(area.x, y + ROW_H - 1.0, theme::CANVAS_W - area.x, 1.0), theme::SURFACE4.alpha(0.6), 0.0);

        // The channel's cell.
        let cell = Rect::new(area.x, y + 4.0, CHANNEL_W, ROW_H - 8.0);
        if row_focused {
            ui.fill(cell, theme::SURFACE3, radius::CONTROL);
        }
        let logo = Rect::new(cell.x + CHANNEL_INSET, y + (ROW_H - 34.0) / 2.0, 56.0, 34.0);
        channel_tile(ui, cx, logo, c, Well::PAGE);
        let tx = logo.right() + 10.0;
        let tw = cell.right() - 8.0 - tx;
        let block = theme::CAPTION.line_height + theme::LABEL.line_height;
        let ty = y + (ROW_H - block) / 2.0;
        let number = c.number.as_deref().unwrap_or("");
        let nw = line_mid(ui, number, &theme::CAPTION, tx, ty, theme::CAPTION.line_height, tw, theme::SUBTLE);
        if c.favorite {
            let sx = tx + nw + if number.is_empty() { 0.0 } else { 4.0 };
            ui.icon(Rect::new(sx, ty + 2.5, 12.0, 12.0), pley_base::Icon::Star, theme::ACCENT);
        }
        let name = channel_name(c, Some(data.category));
        let name_color = if row_focused { theme::FG } else { theme::MUTED };
        ui.text(Point::new(tx, ty + theme::CAPTION.line_height), &name, &theme::LABEL, &TextOpts::line(tw), name_color);

        // Its programmes, as far as the grid may move a screen either way.
        let programs = data.programs.programs(c.id);
        let from = (self.cursor.left - VIEW_S).max(self.start);
        let to = self.cursor.left + 2 * VIEW_S;
        ui.clip(grid, |ui| {
            let mut any = false;
            for p in between(programs, from, to) {
                any = true;
                let start = p.start.max(self.start);
                let x = grid.x + (start - self.start) as f32 * DP_PER_SECOND - shown_x;
                let w = (p.stop - start) as f32 * DP_PER_SECOND;
                let focused = focused_start == Some(p.start);
                programme(ui, Rect::new(x, y, w, ROW_H), p, focused, now, &data.clock, grid.x - x);
            }
            let answered = data.programs.answered(c.id, self.cursor.left);
            if !any && answered {
                let focused = focused_start == Some(-1);
                let inner = Rect::new(grid.x + 2.0, y + 4.0, grid.w - 4.0, ROW_H - 8.0);
                ui.fill(inner, if focused { theme::ACCENT } else { theme::SURFACE.alpha(0.6) }, radius::CONTROL);
                let label = if c.has_guide { "No information for this time".to_string() } else { name.clone() };
                let color = if focused { theme::ON_ACCENT } else { theme::SUBTLE };
                line_mid(ui, &label, &theme::CAPTION, inner.x + 12.0, inner.y, inner.h, inner.w - 24.0, color);
            } else if focused_start == Some(-1) && any {
                // Focus in a stretch the guide says nothing of: that stretch, in the accent.
                let t = self.cursor.time;
                let before = programs.unwrap_or(&[]).iter().filter(|p| p.stop <= t).map(|p| p.stop).max();
                let after = programs.unwrap_or(&[]).iter().filter(|p| p.start > t).map(|p| p.start).min();
                let gap_start = before.unwrap_or(slot(t)).max(self.start);
                let gap_stop = after.unwrap_or(slot(t) + SLOT);
                let x = grid.x + (gap_start - self.start) as f32 * DP_PER_SECOND - shown_x;
                let w = (gap_stop - gap_start) as f32 * DP_PER_SECOND;
                let inner = Rect::new(x + 2.0, y + 4.0, (w - 4.0).max(0.0), ROW_H - 8.0);
                ui.fill(inner, theme::ACCENT, radius::CONTROL);
                if inner.w > MIN_TEXT {
                    let lx = inner.x.max(grid.x) + 10.0;
                    line_mid(
                        ui,
                        "No information",
                        &theme::CAPTION,
                        lx,
                        inner.y,
                        inner.h,
                        inner.right() - 10.0 - lx,
                        theme::ON_ACCENT,
                    );
                }
            }
        });
    }
}

/// One programme's block in a row at `rect` (its whole span; the row's full height): on now in
/// the primary container with an accent edge and its progress along the foot, later ones on
/// Surface2, ended ones faded, the focused one in the accent. `hidden` is how much of its start is
/// past the grid's left edge, which its words slide past to stay in view.
fn programme(ui: &mut Ui, rect: Rect, p: &LiveProgram, focused: bool, now: i64, clock: &Clock, hidden: f32) {
    let inner = Rect::new(rect.x + 2.0, rect.y + 4.0, (rect.w - 4.0).max(0.0), rect.h - 8.0);
    if inner.w <= 0.0 {
        return;
    }
    let airing = p.airing_at(now);
    let alpha = if p.stop <= now && !focused { 0.55 } else { 1.0 };
    ui.opacity(alpha, |ui| {
        let fill = if focused {
            theme::ACCENT
        } else if airing {
            theme::PRIMARY_CONTAINER
        } else {
            theme::SURFACE2
        };
        ui.fill(inner, fill, radius::CONTROL);
        if airing && !focused {
            ui.stroke(inner.expand(-0.5), theme::ACCENT.alpha(0.35), radius::CONTROL - 0.5, 1.0);
        }
        if rect.w > MIN_TEXT {
            let room = (rect.w - KEEP_TITLE).max(0.0);
            let shift = hidden.clamp(0.0, room);
            let x = rect.x + shift + 10.0;
            let w = (rect.right() - 10.0 - x).max(1.0);
            let block = theme::LABEL.line_height + theme::CAPTION.line_height;
            let top = inner.y + (inner.h - block) / 2.0;
            let weight = if airing || focused { Weight::Bold } else { Weight::Medium };
            let (title, span) =
                if focused { (theme::ON_ACCENT, theme::ON_ACCENT.alpha(0.8)) } else { (theme::FG, theme::SUBTLE) };
            ui.clip(inner, |ui| {
                let style = theme::LABEL.with_weight(weight);
                ui.text(Point::new(x, top), &p.title, &style, &TextOpts::line(w), title);
                let at = Point::new(x, top + theme::LABEL.line_height);
                ui.text(at, &clock.span(p), &theme::CAPTION, &TextOpts::line(w), span);
            });
        }
        if airing {
            let bar = Rect::new(inner.x, inner.bottom() - 2.0, inner.w * p.progress_at(now), 2.0);
            ui.fill(bar, if focused { theme::ON_ACCENT } else { theme::ACCENT }, 0.0);
        }
    });
}

/// The rows of channel ids the guide asks programmes for: those near the view, in blocks of 20 so
/// the asking comes in a few requests rather than one per row.
pub fn guide_rows(rows: (usize, usize), total: usize) -> (usize, usize) {
    let lo = rows.0.saturating_sub(5) / 20 * 20;
    let hi = ((rows.1 + 15).div_ceil(20) * 20).min(total);
    (lo, hi.max(lo))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(start: i64, stop: i64) -> LiveProgram {
        LiveProgram { channel_id: 1, start, stop, title: format!("P{start}"), ..LiveProgram::default() }
    }

    // 2026-10-30 20:10 UTC.
    const NOW: i64 = 1_793_390_400 + 600;

    #[test]
    fn the_guide_opens_at_the_half_hour_before() {
        let start = guide_start(NOW);
        assert_eq!(start, 1_793_388_600, "19:30");
        assert_eq!(left_for(NOW, start), start);
        let c = Cursor::new(NOW, start);
        assert_eq!((c.time, c.left), (NOW, start));
    }

    #[test]
    fn right_and_left_go_programme_by_programme_the_grid_following() {
        let start = guide_start(NOW);
        let list = vec![p(start, start + 3600), p(start + 3600, start + 7200), p(start + 7200, start + 9000)];
        let programs = Some(list.as_slice());
        let mut c = Cursor::new(NOW, start);
        c.step_right(programs, start);
        assert_eq!(c.time, start + 3600);
        assert_eq!(c.left, start, "still in view");
        c.step_right(programs, start);
        assert_eq!(c.time, start + 7200);
        // Within the last half hour of the view: the grid moves to the half hour before.
        assert_eq!(c.left, start + 7200 - SLOT);
        // Left goes to the programme before, the cursor no further back than the half hour before
        // where it was (`TvGuide.kt`: its start, at least the half hour).
        assert!(!c.step_left(programs, start));
        assert_eq!(c.time, start + 5400);
        assert_eq!(at(programs, c.time), Some(&list[1]));
        assert!(!c.step_left(programs, start));
        assert_eq!(c.time, start + 1800);
        assert_eq!(at(programs, c.time), Some(&list[0]));
        assert_eq!(c.left, start + 1800);
        assert!(c.step_left(programs, start), "from the guide's first programme Left leaves");
    }

    #[test]
    fn gaps_are_crossed_by_half_hours_and_programmeless_rows_leave() {
        let start = guide_start(NOW);
        let list = vec![p(start, start + 1800), p(start + 5400, start + 7200)];
        let programs = Some(list.as_slice());
        let mut c = Cursor { row: 0, time: start + 100, left: start };
        c.step_right(programs, start);
        assert_eq!(c.time, start + 1800);
        c.step_right(programs, start);
        assert_eq!(c.time, start + 3600);
        c.step_right(programs, start);
        assert_eq!(c.time, start + 5400);
        assert!(c.step_left(None, start));
        assert!(c.step_left(Some(&[]), start));
    }

    #[test]
    fn back_returns_to_now_and_then_has_nothing_to_do() {
        let start = guide_start(NOW);
        let list = vec![p(start, NOW + 600), p(NOW + 600, NOW + 4200)];
        let programs = Some(list.as_slice());
        let mut c = Cursor::new(NOW, start);
        assert!(!c.away_from_now(programs, NOW, start));
        c.step_right(programs, start);
        assert!(c.away_from_now(programs, NOW, start));
        c.back_to_now(NOW, start);
        assert!(!c.away_from_now(programs, NOW, start));
        // A channel without a guide: once at now, Back leaves.
        assert!(!c.away_from_now(None, NOW, start));
    }

    #[test]
    fn the_cursor_stays_within_the_guides_two_days() {
        let start = guide_start(NOW);
        let mut c = Cursor::new(NOW, start);
        c.move_to(start + GUIDE_SPAN + 5000, start);
        assert_eq!(c.time, start + GUIDE_SPAN - 1);
        assert_eq!(c.left, start + GUIDE_SPAN - VIEW_S);
        let mut end = c;
        end.step_right(None, start);
        assert_eq!(end, c, "nothing past the end");
    }

    #[test]
    fn programmes_are_asked_for_the_rows_near_the_view_in_blocks() {
        assert_eq!(guide_rows((0, 7), 300), (0, 40));
        assert_eq!(guide_rows((26, 33), 300), (20, 60));
        assert_eq!(guide_rows((0, 7), 12), (0, 12));
    }
}
