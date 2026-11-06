//! The live player's channel list (`ChannelList`, `CategoryChannels` in `TvLivePlayerScreen.kt`):
//! a 360 dp panel at the right, the picture moved aside to its left. Its heading names the category
//! listed and opens the channel groups to list another; under it the search ("Search channels",
//! every channel); then the category's channels a page at a time as they scroll, each with what's
//! on, the channel playing marked. It opens on the channel playing, two rows from the top.

use pley_base::{Color, Icon, Point, Rect, TextOpts};
use pley_core::model::{LiveCategory, LiveChannel, LiveGroups};
use pley_core::store::snapshot_keys;
use pley_i18n::t;
use pley_kit::{Keyboard, KeyboardState, SidePanel, TextEdit, TextField};
use pley_player::live::data::{channel_name, split_region};
use pley_player::live::{Category, ChannelPages};
use pley_player::{PlayerServer, now_ms};
use pley_shell::{AppCx, Resource};
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, Edge, Group, Id, Motion, Response, Ui};

use crate::categories::{Column, ColumnView, Picked};
use crate::draw::{Well, channel_tile, line_mid};
use crate::kept::{live_category, page_category};

/// The panel's width.
pub const WIDTH: f32 = SidePanel::WIDTH;
/// A channel's row and the space under it.
const ROW_H: f32 = 52.0;
const ROW_GAP: f32 = 4.0;
const PITCH: f32 = ROW_H + ROW_GAP;
/// The heading's height: a `Tv.Title` line and 6 above and below.
const HEADING_H: f32 = 34.0;
/// Channels loaded past the rows in view.
const MARGIN: i64 = 20;
/// Room under the last row.
const BOTTOM: f32 = 16.0;

/// What the list asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum ListAction {
    /// Watch this channel; channel up and down go through `category` from now on.
    Tune(Box<LiveChannel>, LiveCategory),
    /// The menu key: the list closes, as it opened.
    Close,
}

/// The channel list.
pub struct ChannelList {
    /// The category listed (a search lists every channel).
    shown: Category,
    picking: bool,
    column: Column,
    search: TextEdit,
    typing: bool,
    keyboard: KeyboardState,
    groups: Resource<LiveGroups>,
    /// Changes as the groups do: the group picker's lines are made again only then.
    groups_rev: u64,
    pages: Option<ChannelPages>,
    /// The row focus is on, or was last.
    index: usize,
    /// The rows drawn last frame.
    drawn: (usize, usize),
    scroll: f32,
    /// The channel playing, being found in the list to open on it.
    locate: Option<i64>,
    /// The row at `index` takes focus once it is drawn.
    focus_row: bool,
    /// The heading takes focus (back from the groups).
    focus_heading: bool,
    /// The category listed takes focus as the groups open (`selected.requestFocus()`).
    focus_groups: bool,
    /// Whether the list is open.
    pub open: bool,
}

impl ChannelList {
    /// A closed list of `category`.
    pub fn new(category: &LiveCategory) -> ChannelList {
        ChannelList {
            shown: page_category(category),
            picking: false,
            column: Column::default(),
            search: TextEdit::new(""),
            typing: false,
            keyboard: KeyboardState::default(),
            groups: Resource::new(Some(snapshot_keys::LIVE_GROUPS.to_string()), |api| api.live_groups()),
            groups_rev: 0,
            pages: None,
            index: 0,
            drawn: (0, 0),
            scroll: 0.0,
            locate: None,
            focus_row: false,
            focus_heading: false,
            focus_groups: false,
            open: false,
        }
    }

    /// Opens on `category` (the one zapped through), at the channel playing.
    pub fn open(&mut self, category: &LiveCategory, current: i64) {
        self.open = true;
        self.picking = false;
        self.typing = false;
        self.search.clear();
        let shown = page_category(category);
        if self.shown != shown {
            self.shown = shown;
            self.pages = None;
        }
        self.locate = Some(current);
        self.focus_row = true;
        if let Some(p) = self.pages.as_mut() {
            p.neighbor(current, 0, now_ms());
        }
    }

    /// Closes the list.
    pub fn close(&mut self) {
        self.open = false;
        self.typing = false;
        self.picking = false;
    }

    /// Back inside the list: the keyboard, then the groups, close first. Whether it did.
    pub fn back(&mut self) -> bool {
        if std::mem::take(&mut self.typing) {
            return true;
        }
        if std::mem::take(&mut self.picking) {
            self.focus_heading = true;
            return true;
        }
        false
    }

    /// Whether its search is being typed into: the remote's digits are the field's then.
    pub fn typing(&self) -> bool {
        self.typing
    }

    /// Channel `id` was marked a favourite (`on`) or not in the player: its row shows it at once.
    pub fn patch(&mut self, id: i64, on: bool) {
        if let Some(p) = self.pages.as_mut() {
            p.patch(id, |c| c.favorite = on);
        }
    }

    /// The favourites changed on the server: the groups' counts are asked for again.
    pub fn groups_changed(&mut self) {
        self.groups.invalidate();
    }

    fn listed(&self) -> Category {
        let q = self.search.text.trim();
        if q.is_empty() { self.shown.clone() } else { Category::Search(q.to_string()) }
    }

    /// Draws the list's panel (sliding in and out) and handles it; `current` is the channel playing.
    pub fn show(&mut self, ui: &mut Ui, cx: &mut AppCx, id: Id, current: i64) -> Option<ListAction> {
        let p = ui.animate_from(id.with("open"), 0.0, if self.open { 1.0 } else { 0.0 }, Motion::OVERLAY);
        if p <= 0.0 && !self.open {
            return None;
        }
        if self.groups.update(cx) {
            self.groups_rev += 1;
        }
        self.sync_pages(cx);
        let x = theme::CANVAS_W - WIDTH * p;
        ui.fill(Rect::new(x, 0.0, WIDTH + radius::DIALOG, theme::CANVAS_H), theme::SURFACE2, radius::DIALOG);
        ui.fill(Rect::new(x + radius::DIALOG, 0.0, WIDTH - radius::DIALOG, 1.0), theme::SURFACE4, 0.0);
        let area = Rect::new(
            x + SidePanel::PAD_START,
            theme::SAFE_Y,
            WIDTH - SidePanel::PAD_START - SidePanel::PAD_END,
            theme::CANVAS_H - 2.0 * theme::SAFE_Y,
        );
        let mut action = None;
        if self.open {
            ui.group(id, Group::column().trap(), |ui| action = self.content(ui, cx, id, area, current));
        } else {
            pley_kit::inert(ui, |ui| {
                self.content(ui, cx, id, area, current);
            });
        }
        if self.typing {
            let kb = id.with("keyboard");
            let label = t!("livetv.page.search");
            let r = Keyboard::new().action_label(label).show(ui, kb, &mut self.keyboard, &mut self.search);
            if r.done || r.closed {
                self.typing = false;
            }
        }
        action
    }

    fn sync_pages(&mut self, cx: &AppCx) {
        let listed = self.listed();
        if self.pages.as_ref().is_some_and(|p| *p.category() == listed) {
            return;
        }
        let Some(api) = cx.api() else { return };
        let server: std::sync::Arc<dyn PlayerServer> = std::sync::Arc::new(api.clone());
        let mut pages = ChannelPages::new(server, cx.tasks().clone(), listed.clone(), false);
        self.index = 0;
        self.scroll = 0.0;
        if let (Some(id), false) = (self.locate, matches!(listed, Category::Search(_))) {
            pages.neighbor(id, 0, now_ms());
        } else {
            self.locate = None;
        }
        self.pages = Some(pages);
    }

    fn content(&mut self, ui: &mut Ui, cx: &mut AppCx, id: Id, area: Rect, current: i64) -> Option<ListAction> {
        let mut action = None;
        // The heading: the category, OK to choose another.
        let heading_id = id.with("heading");
        let heading = Rect::new(area.x, area.y, area.w, HEADING_H);
        if std::mem::take(&mut self.focus_heading) {
            ui.request_focus(heading_id);
        }
        let r = ui.focusable(heading_id, heading);
        if r.menu {
            action = Some(ListAction::Close);
        }
        if r.activated {
            self.picking = !self.picking;
            self.focus_groups = self.picking;
            ui.request_repaint();
        }
        let lit = r.focused || r.pressed;
        if lit {
            ui.fill(heading, if r.pressed { theme::ACCENT_BRIGHT } else { theme::ACCENT }, radius::CONTROL);
        }
        let content = if lit { theme::ON_ACCENT } else { theme::FG };
        let mut hx = heading.x + 8.0;
        if self.picking {
            ui.icon(Rect::new(hx, heading.y + 7.0, 20.0, 20.0), Icon::ArrowLeft, content);
            hx += 30.0;
        } else if self.shown == Category::Favorites {
            ui.icon(Rect::new(hx, heading.y + 8.0, 18.0, 18.0), Icon::Star, content);
            hx += 26.0;
        }
        let label = if self.picking { t!("livetv.player.channelGroups").to_string() } else { list_label(&self.shown) };
        let room = heading.right() - 8.0 - hx - if self.picking { 0.0 } else { 26.0 };
        let w = line_mid(ui, &label, &theme::TITLE, hx, heading.y, heading.h, room, content);
        if !self.picking {
            ui.icon(Rect::new(hx + w + 8.0, heading.y + 8.0, 18.0, 18.0), Icon::ChevronDown, content);
        }
        let top = heading.bottom() + 8.0;

        if self.picking {
            // Another group: the categories, the one listed marked.
            let column_id = id.with("groups");
            let v = ColumnView {
                groups: self.groups.value(),
                rev: self.groups_rev,
                selected: Some(&self.shown),
                found: &[],
                search: None,
            };
            let view = Rect::new(
                area.x - crate::categories::INSET,
                top,
                area.w + crate::categories::INSET,
                area.bottom() - top,
            );
            let mut column = std::mem::take(&mut self.column);
            // Focus goes to the category listed as the groups open, and whenever it is lost.
            if std::mem::take(&mut self.focus_groups) || (ui.focused_in(column_id).is_none() && !r.focused) {
                column.focus_shown(ui, column_id, &v);
            }
            let shown = column.show(ui, column_id, view, &v, Edge::Pass, Dirs::NONE);
            self.column = column;
            if shown.menu {
                return Some(ListAction::Close);
            }
            if let Some(Picked::Category(c)) = shown.picked {
                self.search.clear();
                self.shown = c;
                self.picking = false;
                self.focus_row = true;
                self.locate = Some(current);
                self.pages = None;
                ui.request_repaint();
            }
            return action;
        }

        let field_id = id.with("search");
        let field = TextField::new(&self.search.text, area.w)
            .placeholder(t!("livetv.player.searchChannels"))
            .icon(Icon::Search)
            .typing(self.typing);
        let fr = field.show(ui, field_id, Point::new(area.x, top));
        if fr.activated {
            self.typing = true;
        }
        if fr.menu {
            action = Some(ListAction::Close);
        }
        let rows_top = top + pley_kit::keyboard::FIELD_HEIGHT + 8.0;
        let view = Rect::new(area.x, rows_top, area.w, area.bottom() - rows_top);

        let category = self.listed();
        let Some(pages) = self.pages.as_mut() else { return action };
        let ms = now_ms();
        pages.poll(ms);
        if let Some(target) = self.locate
            && let Some(found) = pages.take_neighbor()
        {
            self.locate = None;
            self.index = found.and_then(|_| pages.find(target)).unwrap_or(0).max(0) as usize;
            self.scroll = (self.index.saturating_sub(2) as f32 * PITCH).max(0.0);
        }
        let total = pages.total().unwrap_or(0).max(0) as usize;
        if pages.total() == Some(0) {
            ui.text(
                Point::new(view.x + 8.0, view.y + 8.0),
                t!("livetv.page.noMatch"),
                &theme::LABEL,
                &TextOpts::line(view.w - 16.0),
                theme::SUBTLE,
            );
        }
        let rows_id = id.with("rows");
        let max = (total as f32 * PITCH - ROW_GAP + BOTTOM - view.h).max(0.0);
        // The focused row follows focus at 30 % of the view, as lists do; only the rows drawn last
        // frame can hold it.
        let (a, b) = self.drawn;
        if let Some(i) = (a..b.min(total)).find(|&i| ui.is_focused(rows_id.with(i as u64))) {
            self.index = i;
            let lo = i as f32 * PITCH;
            if lo < self.scroll || lo + ROW_H > self.scroll + view.h {
                self.scroll = lo - view.h * theme::SCROLL_ANCHOR;
            }
        }
        self.scroll = self.scroll.clamp(0.0, max);
        let shown = ui.animate(rows_id.with("scroll"), self.scroll, pley_kit::SCROLL);
        let first = (shown / PITCH).floor().max(0.0) as usize;
        let past = (((shown + view.h) / PITCH).ceil().max(0.0) as usize).min(total);
        pages.want(first as i64 - MARGIN, past as i64 + MARGIN, ms);
        let (d0, d1) =
            (first.saturating_sub(3).min(self.index.saturating_sub(3)), (past + 3).max(self.index + 4).min(total));
        self.drawn = (d0, d1);
        if self.focus_row && total > 0 && self.locate.is_none() {
            self.index = self.index.min(total - 1);
            if pages.at(self.index as i64).is_some() {
                ui.request_focus(rows_id.with(self.index as u64));
                self.focus_row = false;
            }
        }
        let clock_now = pley_player::live::data::now_seconds();
        ui.clip(view, |ui| {
            ui.group(rows_id, Group::column(), |ui| {
                for i in d0..d1 {
                    let rect = Rect::new(view.x, view.y + i as f32 * PITCH - shown, view.w, ROW_H);
                    let Some(c) = pages.at(i as i64) else {
                        ui.fill(rect, Color::WHITE.alpha(0.04), radius::CONTROL);
                        continue;
                    };
                    let r = channel_row(ui, cx, rows_id.with(i as u64), rect, c, &category, c.id == current, clock_now);
                    if r.activated {
                        action = Some(ListAction::Tune(Box::new(c.clone()), live_category(&category)));
                    } else if r.menu {
                        action = Some(ListAction::Close);
                    }
                }
            });
        });
        action
    }
}

/// The heading's words for a category (`categoryLabel`).
pub fn list_label(c: &Category) -> String {
    match c {
        Category::All | Category::Search(_) => t!("livetv.categories.all").into(),
        Category::Favorites => t!("livetv.categories.favorites").into(),
        Category::Recent => t!("livetv.categories.recent").into(),
        Category::Group(name) => match split_region(name) {
            Some((region, label)) => format!("{region} · {label}"),
            None => name.clone(),
        },
    }
}

/// One channel's row: its logo, "number  name" and what's on, a star for a favourite; the channel
/// playing on Surface3, the focused row in the accent.
#[allow(clippy::too_many_arguments)]
fn channel_row(
    ui: &mut Ui,
    cx: &AppCx,
    id: Id,
    rect: Rect,
    c: &LiveChannel,
    category: &Category,
    current: bool,
    now: i64,
) -> Response {
    let r = ui.focusable(id, rect);
    if r.activated {
        ui.request_repaint();
    }
    let lit = r.focused || r.pressed;
    let fill = if r.pressed {
        theme::ACCENT_BRIGHT
    } else if r.focused {
        theme::ACCENT
    } else if current {
        theme::SURFACE3
    } else {
        Color::TRANSPARENT
    };
    if fill.a > 0 {
        ui.fill(rect, fill, radius::CONTROL);
    }
    let content = if lit { theme::ON_ACCENT } else { theme::FG };
    let tile = Rect::new(rect.x + 10.0, rect.y + (rect.h - 32.0) / 2.0, 52.0, 32.0);
    channel_tile(ui, cx, tile, c, Well::PAGE.filled(Color::WHITE.alpha(0.08), radius::TILE));
    let x = tile.right() + 12.0;
    let end = rect.right() - 10.0 - if c.favorite { 14.0 + 8.0 } else { 0.0 };
    let title = [c.number.clone().filter(|n| !n.is_empty()), Some(channel_name(c, Some(category)))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ");
    let on = c.on_at(now);
    let block = theme::LABEL.line_height + if on.is_some() { theme::CAPTION.line_height } else { 0.0 };
    let y = rect.y + (rect.h - block) / 2.0;
    ui.text(Point::new(x, y), &title, &theme::LABEL, &TextOpts::line(end - x), content);
    if let Some(p) = on {
        let at = Point::new(x, y + theme::LABEL.line_height);
        ui.text(at, &p.title, &theme::CAPTION, &TextOpts::line(end - x), content.alpha(0.75));
    }
    if c.favorite {
        ui.icon(Rect::new(rect.right() - 10.0 - 14.0, rect.y + (rect.h - 14.0) / 2.0, 14.0, 14.0), Icon::Star, content);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heading_names_the_category_listed() {
        assert_eq!(list_label(&Category::All), "All channels");
        assert_eq!(list_label(&Category::Favorites), "Favourites");
        assert_eq!(list_label(&Category::Group("UK| Sport".into())), "UK · Sport");
        assert_eq!(list_label(&Category::Group("Kids".into())), "Kids");
        pley_i18n::with_language(pley_i18n::Language::Fr, || {
            assert_eq!(list_label(&Category::All), "Toutes les chaînes");
            assert_eq!(list_label(&Category::Group("UK| Sport".into())), "UK · Sport");
        });
    }
}
