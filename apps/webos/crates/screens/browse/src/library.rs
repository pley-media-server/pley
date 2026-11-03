//! A library (`TvLibraryScreen.kt`, `TvLibraryControls.kt`, `TvLibraryParts.kt`, `JumpControl.kt`;
//! the TV app's `library.tsx`): its name and tabs; Recommended's rows; the whole library as a grid
//! with its sort, filters and A–Z jump; and its collections.
//!
//! The header is a 44 dp row at y 27: the name at x 96 and the tabs at the right safe edge, 8
//! apart; Up from a tab's content goes to the tab shown. The Library tab has its chips at y 80
//! (Sort, Filter, Unwatched, Genre, Year, the resolution and dynamic range while chosen, then the
//! count) and its grid's focused row at y 136;
//! the jump control sits at x 872, level with that row. The view (sort, order, filters) and the
//! tab are kept per server, profile and library, read off the main thread before the page shows
//! (500 ms at most); a row's See all opens the Library tab in its order, kept only once changed.

use pley_base::{Icon, Point, Size, TextOpts};
use pley_core::model::{
    CollectionList, CollectionSummary, Hub, Hubs, Library, LibraryFilters, LibraryKind, LibraryQuery, LibrarySort,
    RangeFilter, ResolutionFilter, SortOrder,
};
use pley_core::store::{keys, snapshot_keys};
use pley_core::{ApiError, Promise};
use pley_kit::{
    Button, ColumnState, GridState, Heading, JumpControl, ListRow, ListState, PosterGrid, Row, RowColumn, RowState,
    Section, SidePanel, Tile, TileKind, Trailing,
};
use pley_shell::{AppCx, AppEvent, Chrome, LibraryTab, Load, Resource, Route, Screen};
use pley_ui::theme;
use pley_ui::{Dir, Dirs, Edge, Group, Id, Tier, Ui};

use crate::alpha::{AlphaIndex, JUMP_FROM};
use crate::listing::{Listing, WINDOW_ABOVE, WINDOW_ABOVE_LOW};
use crate::parts::{Line, Rest, column_message, failed_title, titled_panel_lines};
use crate::tiles::{Cell, Quality, page_of, requests};
use crate::view::{
    collection_count, collections_label, count_label, default_order, default_view, filtered, fit, opening_tab,
    range_label, resolution_label, see_all_sort, see_all_view, smart_filter_of, sort_label, sorts_for, tab_label,
    tabs_for, unfiltered,
};

/// The header row's top and height.
const HEADER_Y: f32 = theme::SAFE_Y;
const HEADER_H: f32 = 44.0;
/// Between the tabs.
const TAB_GAP: f32 = 8.0;
/// The chips' row, and the Recommended rows' and Collections count's line.
const CHIPS_Y: f32 = theme::HEADER_Y_LIBRARY;
/// Between chips.
const CHIP_GAP: f32 = 10.0;
/// The grid's box: its focused row's wells 14 below, at 136.
const GRID_TOP: f32 = 122.0;
/// Where the Library tab's states start.
const STATE_TOP: f32 = 146.0;
/// Cells shown while the total isn't known (`UNKNOWN_CELLS`), and skeletons while a library's
/// collections come (`SKELETONS`).
const UNKNOWN_CELLS: usize = 18;
/// How long the page waits for its kept view before it shows anyway.
const KEPT_WAIT: f64 = 0.5;
/// Genre and Year chips cut their label at this width.
const CHIP_LABEL_MAX: f32 = 220.0;
/// The least the Genre and Sort chips' labels give way to, for the row to fit.
const CHIP_LABEL_MIN: f32 = 48.0;
/// The least room the count is drawn in.
const COUNT_LEAST: f32 = 48.0;

/// What this TV kept of a library: its view and its tab.
type Kept = (Option<LibraryQuery>, Option<String>);

/// A library's page.
pub struct LibraryScreen {
    id: i64,
    asked_tab: Option<LibraryTab>,
    see_all: Option<LibrarySort>,
    kept: Option<Promise<Kept>>,
    asked_at: Option<f64>,
    /// The view and tab kept, once read (or given up on).
    ready: Option<Kept>,
    tab: Option<LibraryTab>,
    /// The tab the page opened on: its content takes the first focus.
    opened: Option<LibraryTab>,
    grid: Option<GridTab>,
    recommended: Option<RecommendedTab>,
    collections: Option<CollectionsTab>,
    /// The libraries couldn't be had: the page says so, with Retry.
    libraries_failed: bool,
}

impl LibraryScreen {
    /// The page of library `id`, on `tab` (else the one it was left on); `sort`: a row's See all.
    pub fn new(id: i64, tab: Option<LibraryTab>, sort: Option<LibrarySort>) -> LibraryScreen {
        LibraryScreen {
            id,
            asked_tab: tab.or(sort.map(|_| LibraryTab::Library)),
            see_all: sort,
            kept: None,
            asked_at: None,
            ready: None,
            tab: None,
            opened: None,
            grid: None,
            recommended: None,
            collections: None,
            libraries_failed: false,
        }
    }

    /// The tab shown, once decided.
    pub fn tab(&self) -> Option<LibraryTab> {
        self.tab
    }

    fn read_kept(&mut self, cx: &mut AppCx) {
        if self.ready.is_some() {
            return;
        }
        let (Some(server), Some(profile)) = (cx.state.server().map(str::to_string), cx.state.profile()) else {
            self.ready = Some((None, None));
            return;
        };
        if self.kept.is_none() {
            let store = cx.store().clone();
            let (view, tab) =
                (keys::library_view(&server, profile, self.id), keys::library_tab(&server, profile, self.id));
            self.kept = Some(cx.tasks().spawn(move || (store.get::<LibraryQuery>(&view), store.get::<String>(&tab))));
            self.asked_at = Some(cx.now);
        }
        if let Some(p) = &mut self.kept
            && let Some(kept) = p.take()
        {
            self.ready = Some(kept);
            self.kept = None;
        } else if self.kept.as_ref().is_some_and(|p| p.failed())
            || self.asked_at.is_some_and(|t| cx.now - t >= KEPT_WAIT)
        {
            self.ready = Some((None, None));
            self.kept = None;
        }
    }

    fn keep_tab(&self, cx: &mut AppCx, tab: LibraryTab) {
        let (Some(server), Some(profile)) = (cx.state.server(), cx.state.profile()) else { return };
        let (store, key) = (cx.store().clone(), keys::library_tab(server, profile, self.id));
        cx.tasks().detach(move || {
            if let Err(e) = store.set(&key, tab.as_str()) {
                log::warn!("library tab not kept: {e}");
            }
        });
    }

    fn show_tab(&mut self, cx: &mut AppCx, tab: LibraryTab) {
        if self.tab != Some(tab) {
            self.tab = Some(tab);
            self.keep_tab(cx, tab);
        }
    }

    /// Draws the header: the name and the tabs, Down from which goes to `down` (the tab's content);
    /// returns the tab chosen.
    fn header(
        &mut self,
        ui: &mut Ui,
        library: &Library,
        tabs: &[LibraryTab],
        shown: LibraryTab,
        down: Id,
    ) -> Option<LibraryTab> {
        let buttons: Vec<Button> = tabs.iter().map(|t| Button::tab(tab_label(*t), *t == shown)).collect();
        let widths: Vec<f32> = buttons.iter().map(|b| b.size(ui).w).collect();
        let total: f32 = widths.iter().sum::<f32>() + TAB_GAP * (widths.len().saturating_sub(1)) as f32;
        let right = theme::CANVAS_W - theme::SAFE_X;
        let mut x = right - total;
        let name_w = (x - 16.0 - theme::GUTTER).max(40.0);
        let name_y = HEADER_Y + (HEADER_H - theme::PAGE_TITLE.line_height) / 2.0;
        ui.text(
            Point::new(theme::GUTTER, name_y),
            &library.name,
            &theme::PAGE_TITLE,
            &TextOpts::line(name_w),
            theme::FG,
        );
        let tabs_id = ui.id("tabs");
        let shown_id = tabs_id.with(shown.as_str());
        // Up from the content comes back to the tab shown, not the nearest one; Down goes to the
        // content: the chips, the rows or the grid (`focusProperties { down = contentFocus }`).
        let group = Group::row().enter_at(shown_id).edge(Dir::Right, Edge::Stop).edge(Dir::Down, Edge::To(down));
        let y = HEADER_Y + (HEADER_H - theme::BUTTON_HEIGHT) / 2.0;
        let mut chosen = None;
        ui.group(tabs_id, group, |ui| {
            for ((tab, button), w) in tabs.iter().zip(&buttons).zip(&widths) {
                if button.show(ui, tabs_id.with(tab.as_str()), Point::new(x, y)).activated {
                    chosen = Some(*tab);
                }
                x += w + TAB_GAP;
            }
        });
        chosen
    }
}

impl Screen for LibraryScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.libraries_failed = false;
        let Some(libraries) = cx.state.libraries() else {
            // The libraries couldn't be had, with nothing saved (`TvLibraryScreen.kt`'s libraryError).
            if let Some(error) = cx.state.account_error() {
                let problem = error.problem();
                let title = failed_title(error, "Couldn't load this library");
                let r = column_message(ui, ui.id("failed"), 140.0, title, &problem, Some("Retry"));
                self.libraries_failed = true;
                if r.is_some_and(|r| r.activated) {
                    cx.state.load_account();
                }
            }
            return;
        };
        let Some(library) = libraries.iter().find(|l| l.id == self.id).cloned() else {
            column_message(
                ui,
                ui.id("missing"),
                140.0,
                "This library isn't here",
                "It may have been removed, or this profile may not see it.",
                None,
            );
            return;
        };
        self.read_kept(cx);
        let Some((kept_view, kept_tab)) = self.ready.clone() else {
            ui.repaint_after(KEPT_WAIT);
            return;
        };
        let tabs = tabs_for(library.kind);
        let shown = match self.tab {
            Some(t) => t,
            None => {
                let kept = kept_tab.as_deref().and_then(LibraryTab::parse);
                let t = opening_tab(tabs, self.asked_tab, kept);
                self.tab = Some(t);
                self.opened = Some(t);
                t
            }
        };
        let initial = self.opened == Some(shown);
        let tab_id = ui.id("tabs").with(shown.as_str());
        // The header first: the tab's panels draw over it.
        let down = match shown {
            LibraryTab::Library => ui.id("chips").with("sort"),
            LibraryTab::Recommended => ui.id("rows"),
            LibraryTab::Collections => ui.id("collections-grid"),
        };
        let chosen_tab = self.header(ui, &library, tabs, shown, down);
        let low = ui.tier() == Tier::Low;
        match shown {
            LibraryTab::Library => {
                let see_all = self.see_all.take();
                let grid = self.grid.get_or_insert_with(|| {
                    // A See all's order is the view until it is changed here; only then is it kept.
                    let view = match see_all {
                        Some(sort) => see_all_view(sort, library.kind),
                        None => fit(kept_view.clone().unwrap_or_else(default_view), library.kind),
                    };
                    GridTab::new(&library, view, low)
                });
                let out = grid.ui(ui, cx, &library, initial, tab_id);
                if let Some(route) = out {
                    cx.push(route);
                }
            }
            LibraryTab::Recommended => {
                let rec = self.recommended.get_or_insert_with(|| RecommendedTab::new(&library));
                match rec.ui(ui, cx, &library, initial, tab_id) {
                    Some(RecommendedOut::Open(route)) => cx.push(route),
                    Some(RecommendedOut::SeeAll(sort)) => {
                        // A row's See all: the Library tab in that row's order, until changed there.
                        let view = see_all_view(sort, library.kind);
                        match &mut self.grid {
                            Some(grid) => grid.set_view(cx, &library, view, false),
                            None => self.see_all = Some(sort),
                        }
                        self.show_tab(cx, LibraryTab::Library);
                    }
                    Some(RecommendedOut::Collections) => self.show_tab(cx, LibraryTab::Collections),
                    None => {}
                }
            }
            LibraryTab::Collections => {
                let col = self.collections.get_or_insert_with(|| CollectionsTab::new(library.id));
                if let Some(route) = col.ui(ui, cx, &library, initial, tab_id) {
                    cx.push(route);
                }
            }
        }
        if let Some(tab) = chosen_tab {
            self.show_tab(cx, tab);
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        match (&mut self.grid, self.tab) {
            (Some(grid), Some(LibraryTab::Library)) => grid.close_panel(),
            _ => false,
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => {
                if let Some(g) = &mut self.grid {
                    g.listing.refresh();
                    g.filters.invalidate();
                }
                if let Some(r) = &mut self.recommended {
                    r.hubs.invalidate();
                    r.collections.invalidate();
                }
                if let Some(c) = &mut self.collections {
                    c.list.invalidate();
                }
            }
            AppEvent::Shown => {
                if let Some(g) = &mut self.grid {
                    g.listing.revalidate(cx.now);
                    g.filters.revalidate(cx.now);
                }
                if let Some(r) = &mut self.recommended {
                    r.hubs.revalidate(cx.now);
                    r.collections.revalidate(cx.now);
                }
                if let Some(c) = &mut self.collections {
                    c.list.revalidate(cx.now);
                }
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.tab.is_some() || self.libraries_failed
    }

    fn modal(&self) -> bool {
        self.tab == Some(LibraryTab::Library) && self.grid.as_ref().is_some_and(|g| g.panel.is_some())
    }
}

/// A panel of the Library tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Sort,
    Filter,
    Genre,
    Year,
    Resolution,
    DynamicRange,
}

/// A line of the Filter panel: the "Show" heading and its three choices, Genre and Year, then
/// Resolution and Dynamic range where the library has them to choose (or one is chosen), then
/// Clear filters and Save as… while the view is filtered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterLine {
    Show,
    Watched(Option<bool>),
    Panel(Panel),
    Clear,
    SaveAs,
}

/// The Library tab: chips, the grid, the A–Z jump and the panels.
struct GridTab {
    view: LibraryQuery,
    kind: TileKind,
    music: bool,
    listing: Listing,
    filters: Resource<LibraryFilters>,
    /// The Year panel's labels, worked out when the filters come.
    years: Vec<String>,
    grid: GridState,
    /// Bumped with each view, so a new view's grid starts afresh.
    generation: u64,
    alpha: Option<(u64, AlphaIndex)>,
    letter: usize,
    panel: Option<Panel>,
    /// The panel drawn while it slides out.
    last_panel: Panel,
    panel_list: ListState,
    /// A chip went with its value: Filter takes focus at the next frame.
    focus_filter: bool,
    rest: Rest,
    window_above: usize,
}

impl GridTab {
    fn new(library: &Library, view: LibraryQuery, low: bool) -> GridTab {
        let music = library.kind == LibraryKind::Music;
        let kind = if music { TileKind::Square } else { TileKind::LibraryGrid };
        let window_above = if low { WINDOW_ABOVE_LOW } else { WINDOW_ABOVE };
        let id = library.id;
        let filters = Resource::new(Some(snapshot_keys::filters(id)), move |api| {
            if music { Ok(LibraryFilters::default()) } else { api.library_filters(id) }
        });
        GridTab {
            listing: Listing::new(id, view.clone(), kind, window_above),
            view,
            kind,
            music,
            filters,
            years: Vec::new(),
            grid: GridState::default(),
            generation: 0,
            alpha: None,
            letter: 0,
            panel: None,
            last_panel: Panel::Sort,
            panel_list: ListState::default(),
            focus_filter: false,
            rest: Rest::default(),
            window_above,
        }
    }

    /// Lists the library in `view`; kept on the TV when `keep`.
    fn set_view(&mut self, cx: &mut AppCx, library: &Library, view: LibraryQuery, keep: bool) {
        let view = fit(view, library.kind);
        if keep && let (Some(server), Some(profile)) = (cx.state.server(), cx.state.profile()) {
            let (store, key, value) =
                (cx.store().clone(), keys::library_view(server, profile, library.id), view.clone());
            cx.tasks().detach(move || {
                if let Err(e) = store.set(&key, &value) {
                    log::warn!("library view not kept: {e}");
                }
            });
        }
        if view == self.view {
            return;
        }
        self.listing = Listing::new(library.id, view.clone(), self.kind, self.window_above);
        self.view = view;
        self.grid = GridState::default();
        self.generation += 1;
        self.alpha = None;
    }

    fn close_panel(&mut self) -> bool {
        self.panel.take().is_some()
    }

    /// Closes the Resolution or Dynamic range panel once chosen in. Cleared, its chip goes, so
    /// focus goes to Filter, where the choice is made again, once the panel has let it go.
    fn close_chooser(&mut self, cleared: bool) {
        self.panel = None;
        self.focus_filter |= cleared;
    }

    fn open_panel(&mut self, panel: Panel, current: usize) {
        self.panel = Some(panel);
        self.last_panel = panel;
        self.panel_list = ListState::at(current);
    }

    /// Draws the tab; returns a page to open.
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx, library: &Library, initial: bool, tab_id: Id) -> Option<Route> {
        let mut open = None;
        if self.filters.update(cx) {
            let years = self.filters.value().map_or(&[][..], |f| f.years.as_slice());
            self.years = years.iter().map(|y| y.to_string()).collect();
        }
        let px = ui.image_px(self.kind.size().w);
        let first = self.grid.index.saturating_sub(PosterGrid::columns_for(self.kind) * 2);
        self.listing.update(cx, first, px);
        let total = self.listing.total();
        let grid_id = ui.id(("grid", self.generation));

        // The A–Z jump: sorted by title, held whole, more than 30 titles.
        let jump = self.view.sort == LibrarySort::Title
            && self.listing.whole()
            && total.is_some_and(|t| t > JUMP_FROM)
            && !self.listing.windowed();
        if jump && self.alpha.as_ref().is_none_or(|(v, _)| *v != self.listing.version()) {
            let index = AlphaIndex::new(self.listing.cells().map(sort_key));
            self.alpha = Some((self.listing.version(), index));
        }
        let jump_id = ui.id("jump");

        // The states, else the grid.
        let message = self.message(total);
        let filtered_view = filtered(&self.view);
        if let Some(state) = message {
            match state {
                State::Failed => {
                    let error = self.listing.error().cloned().unwrap_or(ApiError::SignedOut);
                    let title = failed_title(&error, "Couldn't load this library");
                    let r = column_message(ui, ui.id("state"), STATE_TOP, title, &error.problem(), Some("Retry"));
                    if r.is_some_and(|r| r.activated) {
                        self.listing.refresh();
                    }
                }
                State::NoMatches => {
                    let body = "Nothing in this library matches the current filters.";
                    let r = column_message(ui, ui.id("state"), STATE_TOP, "No matches", body, Some("Clear filters"));
                    if r.is_some_and(|r| r.activated) {
                        let view = unfiltered(&self.view);
                        self.set_view(cx, library, view, true);
                    }
                }
                State::Empty => {
                    let body = "Nothing here yet. Titles appear as the server adds them.";
                    column_message(ui, ui.id("state"), STATE_TOP, "This library is empty", body, None);
                    if initial {
                        ui.initial_focus(tab_id);
                    }
                }
            }
        } else {
            let len = total.unwrap_or(UNKNOWN_CELLS);
            if initial {
                ui.initial_focus(grid_id.with(0u64));
            }
            let edge = if jump { Edge::To(jump_id) } else { Edge::Stop };
            // A held Right or Up stays in the grid; a press goes on to the jump control or the
            // chips.
            let grid = PosterGrid::new(self.kind, GRID_TOP).right_edge(edge).held_stop(Dirs::RIGHT.union(Dirs::UP));
            let kind = self.kind;
            let listing = &self.listing;
            let r = grid.show(ui, grid_id, &mut self.grid, len, |ui, slot| match listing.at(slot.index) {
                Some(cell) => cell.tile(kind).paint(ui, slot),
                None => Tile::skeleton(kind).paint(ui, slot),
            });
            if let Some(i) = r.activated
                && let Some(cell) = self.listing.at(i)
            {
                open = Some(page_of(&cell.item));
            }
            if let Some(i) = r.actions
                && let Some(cell) = self.listing.at(i)
            {
                cx.tile_actions(cell.item.id, false);
            }
            let focused = r.focused.and_then(|i| self.listing.at(i)).map(|c| &c.item);
            let rested = focused.is_some() && ui.rested();
            self.rest.update(cx, focused, rested);
            if jump && let Some((_, alpha)) = &self.alpha {
                let available = alpha.available();
                let control = JumpControl { available: &available, current: alpha.letter_at(self.grid.index) };
                let group = Group::free().edge(Dir::Left, Edge::To(grid_id)).edge(Dir::Right, Edge::Stop);
                let at = Point::new(pley_kit::JUMP_X, GRID_TOP + theme::GRID_CONTENT_TOP);
                let answer = ui.group(ui.id("jump-group"), group, |ui| control.show(ui, jump_id, at, &mut self.letter));
                if let Some(letter) = answer.chosen {
                    self.grid.focus_to(alpha.jump_target(letter));
                }
            }
        }

        // The chips, then the count.
        self.chips(ui, cx, library, total, filtered_view);

        // The panels: one at a time, sliding out as they close.
        open = open.or(self.panels(ui, cx, library));
        open
    }

    fn message(&self, total: Option<usize>) -> Option<State> {
        if total.is_none() && self.listing.error().is_some() {
            Some(State::Failed)
        } else if total == Some(0) && filtered(&self.view) {
            Some(State::NoMatches)
        } else if total == Some(0) {
            Some(State::Empty)
        } else {
            None
        }
    }

    fn chips(&mut self, ui: &mut Ui, cx: &mut AppCx, library: &Library, total: Option<usize>, filtered_view: bool) {
        let order = self.view.order.unwrap_or(default_order(self.view.sort));
        let sort_icon = if order == SortOrder::Asc { Icon::ArrowUpNarrowWide } else { Icon::ArrowDownWideNarrow };
        let (genres, years) = match self.filters.value() {
            Some(f) => (f.genres.len(), f.years.len()),
            None => (0, 0),
        };
        let year_label = self.view.year.map(|y| y.to_string());
        // Each chip's name, label and button.
        let sort = sort_label(self.view.sort);
        let mut chips: Vec<(&'static str, &str, Button)> = vec![("sort", sort, Button::chip(sort).icon(sort_icon))];
        if !self.music {
            chips.push(("filter", "Filter", Button::chip("Filter").icon(Icon::SlidersHorizontal)));
            let unwatched = Button::chip("Unwatched").selected(self.view.watched == Some(false));
            chips.push(("unwatched", "Unwatched", unwatched));
            if genres > 0 || self.view.genre.is_some() {
                let label = self.view.genre.as_deref().unwrap_or("Genre");
                let b = Button::chip(label).selected(self.view.genre.is_some()).max_label_width(CHIP_LABEL_MAX);
                chips.push(("genre", label, b));
            }
            if years > 0 || self.view.year.is_some() {
                let label = year_label.as_deref().unwrap_or("Year");
                chips.push(("year", label, Button::chip(label).selected(self.view.year.is_some())));
            }
            // The row has no room for two more choosers: Resolution and Dynamic range are chosen
            // in the Filter panel, and show here only while chosen, named by their value.
            if let Some(r) = self.view.resolution.map(resolution_label) {
                chips.push(("resolution", r, Button::chip(r).selected(true)));
            }
            if let Some(r) = self.view.dynamic_range.map(range_label) {
                chips.push(("range", r, Button::chip(r).selected(true)));
            }
        }
        // With most filters chosen, the genre's name gives way, then the sort's (its panel names
        // it whole), so the row ends within the safe area.
        let right = theme::CANVAS_W - theme::SAFE_X;
        let row_w = chips.iter().map(|(_, _, b)| b.size(ui).w + CHIP_GAP).sum::<f32>() - CHIP_GAP;
        let mut over = theme::GUTTER + row_w - right;
        for yielding in ["genre", "sort"] {
            if over <= 0.0 {
                break;
            }
            let Some((_, label, button)) = chips.iter_mut().find(|(name, _, _)| *name == yielding) else { continue };
            let shown = ui.measure(label, &theme::LABEL, &TextOpts::line(f32::MAX)).w.min(CHIP_LABEL_MAX);
            let cut = (shown - over).max(CHIP_LABEL_MIN).min(shown);
            *button = button.max_label_width(cut);
            over -= shown - cut;
        }
        let chips_id = ui.id("chips");
        let mut x = theme::GUTTER;
        let mut acted: Option<&'static str> = None;
        ui.group(chips_id, Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop), |ui| {
            for (name, _, button) in &chips {
                let r = button.show(ui, chips_id.with(*name), Point::new(x, CHIPS_Y));
                x += button.size(ui).w + CHIP_GAP;
                if r.activated {
                    acted = Some(*name);
                }
            }
        });
        if std::mem::take(&mut self.focus_filter) {
            ui.request_focus(chips_id.with("filter"));
        }
        // The count in what is left of the row; none where little more than its "…" would show.
        let width = theme::CANVAS_W - theme::SAFE_X - x + CHIP_GAP - 8.0;
        if let Some(total) = total.filter(|_| width >= COUNT_LEAST) {
            let of = (filtered_view && library.item_count >= total as i64).then_some(library.item_count);
            let text = count_label(total as i64, library.kind, of);
            let y = CHIPS_Y + (theme::BUTTON_HEIGHT - theme::CAPTION.line_height) / 2.0;
            ui.text(Point::new(x - CHIP_GAP + 8.0, y), &text, &theme::CAPTION, &TextOpts::line(width), theme::MUTED);
        }
        match acted {
            Some("sort") => {
                let at = sorts_for(library.kind).iter().position(|s| *s == self.view.sort).unwrap_or(0);
                self.open_panel(Panel::Sort, at);
            }
            Some("filter") => self.open_panel(Panel::Filter, self.show_line()),
            Some("unwatched") => {
                let watched = if self.view.watched == Some(false) { None } else { Some(false) };
                let view = LibraryQuery { watched, ..self.view.clone() };
                self.set_view(cx, library, view, true);
            }
            Some("genre") => {
                let at = self.genre_line();
                self.open_panel(Panel::Genre, at);
            }
            Some("year") => {
                let at = self.year_line();
                self.open_panel(Panel::Year, at);
            }
            Some("resolution") => {
                let at = self.resolution_line();
                self.open_panel(Panel::Resolution, at);
            }
            Some("range") => {
                let at = self.range_line();
                self.open_panel(Panel::DynamicRange, at);
            }
            _ => {}
        }
    }

    /// The Filter panel's lines ([`FilterLine`]).
    fn filter_lines(&self) -> Vec<FilterLine> {
        let mut lines = vec![
            FilterLine::Show,
            FilterLine::Watched(None),
            FilterLine::Watched(Some(false)),
            FilterLine::Watched(Some(true)),
            FilterLine::Panel(Panel::Genre),
            FilterLine::Panel(Panel::Year),
        ];
        if !self.resolutions().is_empty() || self.view.resolution.is_some() {
            lines.push(FilterLine::Panel(Panel::Resolution));
        }
        if !self.ranges().is_empty() || self.view.dynamic_range.is_some() {
            lines.push(FilterLine::Panel(Panel::DynamicRange));
        }
        if filtered(&self.view) {
            lines.extend([FilterLine::Clear, FilterLine::SaveAs]);
        }
        lines
    }

    /// The Filter panel's line with the "Show" choice in force (under the fixed title: "Show",
    /// All, Unwatched, Watched, then the filters' rows, Clear filters and Save as…).
    fn show_line(&self) -> usize {
        match self.view.watched {
            None => 1,
            Some(false) => 2,
            Some(true) => 3,
        }
    }

    /// The resolutions the Resolution panel offers: the server's, in its order, but any this app
    /// doesn't know.
    fn resolutions(&self) -> Vec<ResolutionFilter> {
        let offered = self.filters.value().map_or(&[][..], |f| f.resolutions.as_slice());
        offered.iter().copied().filter(|r| *r != ResolutionFilter::Unknown).collect()
    }

    /// The dynamic ranges the Dynamic range panel offers, as [`GridTab::resolutions`].
    fn ranges(&self) -> Vec<RangeFilter> {
        let offered = self.filters.value().map_or(&[][..], |f| f.dynamic_ranges.as_slice());
        offered.iter().copied().filter(|r| *r != RangeFilter::Unknown).collect()
    }

    /// The Resolution panel's line in force: All resolutions, then the resolutions.
    fn resolution_line(&self) -> usize {
        self.view.resolution.and_then(|r| self.resolutions().iter().position(|x| *x == r)).map_or(0, |i| i + 1)
    }

    /// The Dynamic range panel's line in force: All dynamic ranges, then the ranges.
    fn range_line(&self) -> usize {
        self.view.dynamic_range.and_then(|r| self.ranges().iter().position(|x| *x == r)).map_or(0, |i| i + 1)
    }

    /// The Genre panel's line in force: All genres, then the genres.
    fn genre_line(&self) -> usize {
        let genres = self.filters.value().map(|f| f.genres.as_slice()).unwrap_or(&[]);
        self.view.genre.as_ref().and_then(|g| genres.iter().position(|x| x == g)).map_or(0, |i| i + 1)
    }

    /// The Year panel's line in force: All years, then the years.
    fn year_line(&self) -> usize {
        let years = self.filters.value().map(|f| f.years.as_slice()).unwrap_or(&[]);
        self.view.year.and_then(|y| years.iter().position(|x| *x == y)).map_or(0, |i| i + 1)
    }

    /// The line focus opens `panel` on: the choice in force.
    fn initial_line(&self, panel: Panel, library: &Library) -> usize {
        match panel {
            Panel::Sort => sorts_for(library.kind).iter().position(|s| *s == self.view.sort).unwrap_or(0),
            Panel::Filter => self.show_line(),
            Panel::Genre => self.genre_line(),
            Panel::Year => self.year_line(),
            Panel::Resolution => self.resolution_line(),
            Panel::DynamicRange => self.range_line(),
        }
    }

    /// Draws the open panel (or the one sliding out), its title fixed above its lines; returns a
    /// page to open. Its lines are worked out only while it shows.
    fn panels(&mut self, ui: &mut Ui, cx: &mut AppCx, library: &Library) -> Option<Route> {
        let open = self.panel.is_some();
        let panel = self.panel.unwrap_or(self.last_panel);
        let initial = self.initial_line(panel, library);
        let sorts = sorts_for(library.kind);
        let panel_id = ui.id("panel");
        let list_id = ui.id(("panel-list", panel as u8));
        let mut chosen = None;
        let filter_lines = self.filter_lines();
        let (resolutions, ranges) = (self.resolutions(), self.ranges());
        let (view, labels, state) = (&self.view, &self.years, &mut self.panel_list);
        let filters = self.filters.value();
        let genres = filters.map_or(&[][..], |f| f.genres.as_slice());
        let years = filters.map_or(&[][..], |f| f.years.as_slice());
        SidePanel::new().back_closes(false).show(ui, panel_id, open, |ui, area| {
            let order = view.order.unwrap_or(default_order(view.sort));
            let genre_value = view.genre.as_deref().unwrap_or("All genres");
            let year_value = view.year.map_or_else(|| "All years".to_string(), |y| y.to_string());
            let resolution_value = view.resolution.map_or("All resolutions", resolution_label);
            let range_value = view.dynamic_range.map_or("All dynamic ranges", range_label);
            let mut lines: Vec<Line> = Vec::new();
            let title = match panel {
                Panel::Sort => {
                    for s in sorts {
                        lines.push(Line::Row(ListRow::new(sort_label(*s)).checked(*s == view.sort)));
                    }
                    lines.push(Line::Heading(Heading::Small, "Order"));
                    lines.push(Line::Row(ListRow::new("Ascending").checked(order == SortOrder::Asc)));
                    lines.push(Line::Row(ListRow::new("Descending").checked(order == SortOrder::Desc)));
                    "Sort by"
                }
                Panel::Filter => {
                    for line in &filter_lines {
                        lines.push(match line {
                            FilterLine::Show => Line::Heading(Heading::Small, "Show"),
                            FilterLine::Watched(w) => {
                                let label = match w {
                                    None => "All",
                                    Some(false) => "Unwatched",
                                    Some(true) => "Watched",
                                };
                                Line::Row(ListRow::new(label).checked(view.watched == *w))
                            }
                            FilterLine::Panel(p) => {
                                let (label, value) = match p {
                                    Panel::Genre => ("Genre", genre_value),
                                    Panel::Year => ("Year", year_value.as_str()),
                                    Panel::Resolution => ("Resolution", resolution_value),
                                    _ => ("Dynamic range", range_value),
                                };
                                Line::Row(ListRow::new(label).trailing(Trailing::Value(value)))
                            }
                            FilterLine::Clear => Line::Row(ListRow::new("Clear filters")),
                            FilterLine::SaveAs => Line::Row(ListRow::new("Save as…").icon(Icon::Save)),
                        });
                    }
                    "Filter"
                }
                Panel::Genre => {
                    lines.push(Line::Row(ListRow::new("All genres").checked(view.genre.is_none())));
                    for g in genres {
                        lines.push(Line::Row(ListRow::new(g).checked(view.genre.as_ref() == Some(g))));
                    }
                    "Genre"
                }
                Panel::Year => {
                    lines.push(Line::Row(ListRow::new("All years").checked(view.year.is_none())));
                    for (y, label) in years.iter().zip(labels) {
                        lines.push(Line::Row(ListRow::new(label).checked(view.year == Some(*y))));
                    }
                    "Year"
                }
                Panel::Resolution => {
                    lines.push(Line::Row(ListRow::new("All resolutions").checked(view.resolution.is_none())));
                    for r in &resolutions {
                        let row = ListRow::new(resolution_label(*r)).checked(view.resolution == Some(*r));
                        lines.push(Line::Row(row));
                    }
                    "Resolution"
                }
                Panel::DynamicRange => {
                    let none = view.dynamic_range.is_none();
                    lines.push(Line::Row(ListRow::new("All dynamic ranges").checked(none)));
                    for r in &ranges {
                        lines.push(Line::Row(ListRow::new(range_label(*r)).checked(view.dynamic_range == Some(*r))));
                    }
                    "Dynamic range"
                }
            };
            chosen = titled_panel_lines(ui, list_id, area, state, title, &lines, Some(initial));
        });
        let chosen = chosen.filter(|_| open)?;
        let view = self.view.clone();
        let mut route = None;
        match panel {
            Panel::Sort => {
                let n = sorts.len();
                let next = if chosen < n {
                    let sort = sorts[chosen];
                    // The sort in force keeps its order (`LibraryViewModel.sortBy`).
                    if sort == view.sort {
                        view
                    } else {
                        LibraryQuery { sort, order: Some(default_order(sort)), ..view }
                    }
                } else if chosen == n + 1 {
                    LibraryQuery { order: Some(SortOrder::Asc), ..view }
                } else {
                    LibraryQuery { order: Some(SortOrder::Desc), ..view }
                };
                self.set_view(cx, library, next, true);
                self.panel = None;
            }
            Panel::Filter => match filter_lines.get(chosen) {
                Some(FilterLine::Watched(watched)) => {
                    self.set_view(cx, library, LibraryQuery { watched: *watched, ..view }, true);
                }
                Some(FilterLine::Panel(sub)) => {
                    let at = self.initial_line(*sub, library);
                    self.open_panel(*sub, at);
                    ui.request_focus(ui.id(("panel-list", *sub as u8)).with(at as u64));
                }
                Some(FilterLine::Clear) => {
                    self.set_view(cx, library, unfiltered(&view), true);
                    self.panel = None;
                }
                Some(FilterLine::SaveAs) => {
                    // Save as…: Save these filters; Back from it lands on the Filter chip.
                    self.panel = None;
                    ui.request_focus(ui.id("chips").with("filter"));
                    let smart = pley_shell::SmartPlaylist { library_id: library.id, filter: smart_filter_of(&view) };
                    route = Some(Route::NewPlaylist { item_ids: Vec::new(), smart: Some(smart) });
                }
                Some(FilterLine::Show) | None => {}
            },
            Panel::Genre => {
                let genres = self.filters.value().map_or(&[][..], |f| f.genres.as_slice());
                let genre = if chosen == 0 { None } else { genres.get(chosen - 1).cloned() };
                self.set_view(cx, library, LibraryQuery { genre, ..view }, true);
                self.panel = None;
            }
            Panel::Year => {
                let years = self.filters.value().map_or(&[][..], |f| f.years.as_slice());
                let year = if chosen == 0 { None } else { years.get(chosen - 1).copied() };
                self.set_view(cx, library, LibraryQuery { year, ..view }, true);
                self.panel = None;
            }
            Panel::Resolution => {
                let resolution = if chosen == 0 { None } else { resolutions.get(chosen - 1).copied() };
                self.set_view(cx, library, LibraryQuery { resolution, ..view }, true);
                self.close_chooser(resolution.is_none());
            }
            Panel::DynamicRange => {
                let dynamic_range = if chosen == 0 { None } else { ranges.get(chosen - 1).copied() };
                self.set_view(cx, library, LibraryQuery { dynamic_range, ..view }, true);
                self.close_chooser(dynamic_range.is_none());
            }
        }
        route
    }
}

/// The Library tab's states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Failed,
    NoMatches,
    Empty,
}

/// A tab's content came after focus went to the tab (it had nothing to focus while loading): the
/// first time, focus goes on to `target`, as when the page opens with its content saved.
fn place(ui: &mut Ui, placed: &mut bool, tab: Id, target: Id) {
    if !std::mem::replace(placed, true) && ui.is_focused(tab) {
        ui.request_focus(target);
    }
}

/// What the A–Z reads of a title: its sort title, else its title.
fn sort_key(cell: &Cell) -> &str {
    if cell.item.sort_title.trim().is_empty() { &cell.item.title } else { &cell.item.sort_title }
}

/// What a Recommended row holds.
struct RecRow {
    key: String,
    title: String,
    kind: TileKind,
    cells: Vec<Cell>,
    see_all: Option<LibrarySort>,
    continue_watching: bool,
    /// Where the row is scrolled and focused, kept when the rows come again.
    state: RowState,
}

/// A collection as a tile shows it.
struct CollectionCell {
    collection: CollectionSummary,
    subtitle: String,
    count: String,
    art: Vec<pley_base::ImageRequest>,
}

impl CollectionCell {
    fn new(cx: &AppCx, collection: CollectionSummary, kind: Option<LibraryKind>, px: u32) -> CollectionCell {
        let art = match (cx.api(), &collection.poster_url) {
            (Some(api), Some(path)) => requests(api, std::slice::from_ref(path), px),
            _ => Vec::new(),
        };
        CollectionCell {
            subtitle: collection_count(collection.item_count, kind),
            count: collection.item_count.to_string(),
            art,
            collection,
        }
    }
}

/// What the Recommended tab asks of the page.
enum RecommendedOut {
    Open(Route),
    SeeAll(LibrarySort),
    Collections,
}

/// The Recommended tab: the library's rows, then its collections.
struct RecommendedTab {
    hubs: Resource<Hubs>,
    collections: Resource<CollectionList>,
    rows: Vec<RecRow>,
    collection_cells: Vec<CollectionCell>,
    column: ColumnState,
    collections_state: RowState,
    rest: Rest,
    /// Whether the first tile took focus from the tab once the rows came.
    placed: bool,
}

impl RecommendedTab {
    fn new(library: &Library) -> RecommendedTab {
        let id = library.id;
        let music = library.kind == LibraryKind::Music;
        RecommendedTab {
            hubs: Resource::new(Some(snapshot_keys::library_hubs(id)), move |api| api.library_hubs(id)),
            collections: Resource::new(Some(snapshot_keys::collections(id)), move |api| {
                if music { Ok(CollectionList::default()) } else { api.library_collections(id) }
            }),
            rows: Vec::new(),
            collection_cells: Vec::new(),
            column: ColumnState::default(),
            collections_state: RowState::default(),
            rest: Rest::default(),
            placed: false,
        }
    }

    fn build_rows(&mut self, ui: &Ui, cx: &AppCx, library: &Library) {
        let music = library.kind == LibraryKind::Music;
        let hubs: Vec<Hub> = self.hubs.value().map(|h| h.hubs.clone()).unwrap_or_default();
        let mut states: std::collections::HashMap<String, RowState> =
            self.rows.drain(..).map(|r| (r.key, r.state)).collect();
        self.rows = hubs
            .into_iter()
            .filter(|h| !h.items.is_empty())
            .map(|hub| {
                let continue_watching = hub.continue_watching();
                let kind = if continue_watching {
                    TileKind::Landscape
                } else if music {
                    TileKind::Square
                } else {
                    TileKind::Poster
                };
                let px = ui.image_px(kind.size().w);
                let cells = match cx.api() {
                    Some(api) => hub.items.into_iter().map(|i| Cell::new(i, api, kind, px, Quality::NONE)).collect(),
                    None => Vec::new(),
                };
                RecRow {
                    see_all: see_all_sort(&hub.key),
                    state: states.remove(&hub.key).unwrap_or_default(),
                    key: hub.key,
                    title: hub.title,
                    kind,
                    cells,
                    continue_watching,
                }
            })
            .collect();
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppCx,
        library: &Library,
        initial: bool,
        tab_id: Id,
    ) -> Option<RecommendedOut> {
        if self.hubs.update(cx) {
            self.build_rows(ui, cx, library);
        }
        if self.collections.update(cx) {
            let px = ui.image_px(TileKind::Poster.size().w);
            let list = self.collections.value().map(|c| c.collections.clone()).unwrap_or_default();
            self.collection_cells =
                list.into_iter().map(|c| CollectionCell::new(cx, c, Some(library.kind), px)).collect();
        }
        match self.hubs.state() {
            Load::Loading => {
                // Two still rows of seven posters.
                let pitch = 18.0 + 22.0 + TileKind::Poster.size().h + theme::ROW_GAP;
                for k in 0..2 {
                    pley_kit::skeleton_row(
                        ui,
                        CHIPS_Y + k as f32 * pitch,
                        Size::new(180.0, 18.0),
                        22.0,
                        TileKind::Poster,
                        7,
                    );
                }
                if initial {
                    ui.initial_focus(tab_id);
                }
                return None;
            }
            Load::Failed { error, stale: None } => {
                let title = failed_title(error, "Couldn't load recommendations");
                let problem = error.problem();
                let r = column_message(ui, ui.id("state"), CHIPS_Y, title, &problem, Some("Retry"));
                if r.is_some_and(|r| r.activated) {
                    self.hubs.invalidate();
                    self.collections.invalidate();
                }
                return None;
            }
            _ => {}
        }
        if self.rows.is_empty() && self.collection_cells.is_empty() {
            let body = "Recommendations appear once the library has some items.";
            column_message(ui, ui.id("state"), CHIPS_Y, "Nothing to recommend yet", body, None);
            if initial {
                ui.initial_focus(tab_id);
            }
            return None;
        }
        let mut sections: Vec<Section> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| Section::new(i as u64, r.kind.row_height(true), Some(CHIPS_Y)))
            .collect();
        let with_collections = !self.collection_cells.is_empty();
        if with_collections {
            sections.push(Section::new(u64::MAX, TileKind::Poster.row_height(true), Some(CHIPS_Y)));
        }
        let column_id = ui.id("rows");
        if initial {
            let first = column_id.with(0u64);
            let target = match self.rows.first() {
                Some(r) => first.with(r.key.as_str()).with(r.cells[0].item.id as u64),
                None => {
                    column_id.with(u64::MAX).with("collections").with(self.collection_cells[0].collection.id as u64)
                }
            };
            ui.initial_focus(target);
            place(ui, &mut self.placed, tab_id, target);
        }
        let mut out = None;
        let mut focused_at = None;
        let rows = &mut self.rows;
        let cols = &self.collection_cells;
        let cols_state = &mut self.collections_state;
        RowColumn::new(CHIPS_Y).clip_above(Some(CHIPS_Y - theme::ROWS_CLIP_ABOVE)).show(
            ui,
            column_id,
            &mut self.column,
            &sections,
            |ui, i, top| {
                if let Some(row) = rows.get_mut(i) {
                    let len = row.cells.len() + row.see_all.is_some() as usize;
                    let row_id = ui.id(row.key.as_str());
                    let (cells, kind) = (&row.cells, row.kind);
                    let r = Row::new(kind, top).header(&row.title).show(
                        ui,
                        row_id,
                        &mut row.state,
                        len,
                        |k| cells.get(k).map_or(u64::MAX, |c| c.item.id as u64),
                        |ui, slot| match cells.get(slot.index) {
                            Some(cell) => cell.tile(kind).paint(ui, slot),
                            None => Tile::see_all(kind, "See all").paint(ui, slot),
                        },
                    );
                    if let Some(k) = r.activated {
                        out = match (row.cells.get(k), row.see_all) {
                            (Some(cell), _) => Some(RecommendedOut::Open(page_of(&cell.item))),
                            (None, Some(sort)) => Some(RecommendedOut::SeeAll(sort)),
                            (None, None) => out.take(),
                        };
                    }
                    if let Some(cell) = r.actions.and_then(|k| row.cells.get(k)) {
                        cx.tile_actions(cell.item.id, row.continue_watching);
                    }
                    if let Some(k) = r.focused.filter(|k| *k < row.cells.len()) {
                        focused_at = Some((i, k));
                    }
                } else {
                    let row_id = ui.id("collections");
                    let r = Row::new(TileKind::Poster, top).header("Collections").long_press(false).show(
                        ui,
                        row_id,
                        cols_state,
                        cols.len() + 1,
                        |k| cols.get(k).map_or(u64::MAX, |c| c.collection.id as u64),
                        |ui, slot| match cols.get(slot.index) {
                            Some(c) => Tile::new(TileKind::Poster, &c.collection.title)
                                .subtitle(&c.subtitle)
                                .art(&c.art)
                                .paint(ui, slot),
                            None => Tile::see_all(TileKind::Poster, "See all").paint(ui, slot),
                        },
                    );
                    if let Some(k) = r.activated {
                        out = Some(match cols.get(k) {
                            Some(c) => RecommendedOut::Open(Route::Collection { id: c.collection.id }),
                            None => RecommendedOut::Collections,
                        });
                    }
                }
            },
        );
        let focused = focused_at.map(|(i, k)| &self.rows[i].cells[k].item);
        let rested = focused.is_some() && ui.rested();
        self.rest.update(cx, focused, rested);
        out
    }
}

/// The Collections tab: the count, then a grid of the library's collections (skeletons while they
/// come); its messages where the Library tab's are.
struct CollectionsTab {
    list: Resource<CollectionList>,
    cells: Vec<CollectionCell>,
    grid: GridState,
}

impl CollectionsTab {
    fn new(library: i64) -> CollectionsTab {
        CollectionsTab {
            list: Resource::new(Some(snapshot_keys::collections(library)), move |api| api.library_collections(library)),
            cells: Vec::new(),
            grid: GridState::default(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx, library: &Library, initial: bool, tab_id: Id) -> Option<Route> {
        if self.list.update(cx) {
            let px = ui.image_px(TileKind::LibraryGrid.size().w);
            let list = self.list.value().map_or(&[][..], |c| c.collections.as_slice());
            self.cells = list.iter().map(|c| CollectionCell::new(cx, c.clone(), Some(library.kind), px)).collect();
        }
        if let Load::Failed { error, stale: None } = self.list.state() {
            let title = failed_title(error, "Couldn't load collections");
            let problem = error.problem();
            let r = column_message(ui, ui.id("state"), STATE_TOP, title, &problem, Some("Retry"));
            if r.is_some_and(|r| r.activated) {
                self.list.invalidate();
            }
            return None;
        }
        let known = self.list.value().is_some();
        if known && self.cells.is_empty() {
            let body = if library.kind == LibraryKind::Shows {
                "There are no collections in this library yet."
            } else {
                "Movies of the same TMDB collection are grouped here once two of them are in this library."
            };
            column_message(ui, ui.id("state"), STATE_TOP, "No collections yet", body, None);
            if initial {
                ui.initial_focus(tab_id);
            }
            return None;
        }
        if known {
            let count = collections_label(self.cells.len());
            let y = CHIPS_Y + (theme::BUTTON_HEIGHT - theme::CAPTION.line_height) / 2.0;
            ui.text(Point::new(theme::GUTTER, y), &count, &theme::CAPTION, &TextOpts::line(600.0), theme::MUTED);
        }
        let grid_id = ui.id("collections-grid");
        if initial {
            ui.initial_focus(grid_id.with(0u64));
        }
        let cells = &self.cells;
        let len = if known { cells.len() } else { UNKNOWN_CELLS };
        // A held Up stays in the grid; the tabs take a press of its own.
        let r = PosterGrid::new(TileKind::LibraryGrid, GRID_TOP).long_press(false).held_stop(Dirs::UP).show(
            ui,
            grid_id,
            &mut self.grid,
            len,
            |ui, slot| match cells.get(slot.index) {
                Some(c) => Tile::new(TileKind::LibraryGrid, &c.collection.title)
                    .subtitle(&c.subtitle)
                    .art(&c.art)
                    .count(&c.count)
                    .paint(ui, slot),
                None => Tile::skeleton(TileKind::LibraryGrid).paint(ui, slot),
            },
        );
        r.activated.and_then(|i| cells.get(i)).map(|c| Route::Collection { id: c.collection.id })
    }
}
