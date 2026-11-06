//! One of Discover's rows whole (its See all), and everything TMDB has for words (the See all of
//! search's Discover row) (`discover.tsx`'s `TitlesPage`): titles as a grid of posters under the
//! page's title, read a page at a time as focus comes near the end, with a row of grey wells
//! while more come; at the end, Show more when 5 pages brought too few, which reads 5 more, or
//! Retry when a page failed.

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::model::DiscoverList;
use pley_core::requests::{DISCOVER_ROWS, problem_words, requests_problem, search_text};
use pley_i18n::t;
use pley_kit::{GridState, Message, MessageStyle, PosterGrid, Tile, TileKind, skeleton_block};
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, Ui};

use crate::common::{
    ProblemAction, SignInAgain, TitleTileModel, failed_title, marked, marks_version, paint_title, problem_page,
    summary_problem, title_page,
};
use crate::pages::Reader;

/// The grid starts under the page's title.
const LIST_TOP: f32 = 82.0;
/// Before the skeleton shows.
const SKELETON_AFTER: f64 = 0.3;

/// What a grid says when it has no titles.
struct Words {
    title: String,
    /// What the page says when it couldn't load.
    failed: &'static str,
    /// TMDB had nothing.
    none: (String, &'static str),
    /// Beside a lone Show more: the pages read so far had nothing.
    none_yet: String,
}

/// A grid of TMDB's titles.
pub struct TitlesScreen {
    words: Words,
    reader: Reader,
    grid: GridState,
    tiles: Vec<TitleTileModel>,
    source: Option<(usize, u64, (u64, usize, u64))>,
    sign_in: SignInAgain,
    opened: Option<f64>,
    settled: bool,
    railed: bool,
}

impl TitlesScreen {
    /// One of Discover's rows, whole.
    pub fn list(list: DiscoverList) -> TitlesScreen {
        let row = DISCOVER_ROWS.iter().find(|r| r.list == list).unwrap_or(&DISCOVER_ROWS[0]);
        let words = Words {
            title: row.title().to_string(),
            failed: t!("requests.discover.couldNotLoad"),
            none: (t!("requests.discover.nothing").into(), t!("requests.discover.nothingInList")),
            none_yet: t!("requests.discover.nothingYet").into(),
        };
        TitlesScreen::new(words, Reader::new(0, move |api, page| api.discover(list, page)))
    }

    /// Everything TMDB has for `query`.
    pub fn search(query: &str) -> TitlesScreen {
        let q = search_text(query);
        let words = Words {
            title: t!("requests.discover.resultsFor", query = q),
            failed: t!("requests.discover.searchFailed"),
            none: (t!("requests.discover.nothingFound", query = q), t!("requests.discover.nothingFoundHint")),
            none_yet: t!("requests.discover.nothingYetFor", query = q),
        };
        TitlesScreen::new(words, Reader::new(0, move |api, page| api.discover_search(&q, page, false)))
    }

    fn new(words: Words, mut reader: Reader) -> TitlesScreen {
        reader.want(4 * PosterGrid::columns_for(TileKind::Poster));
        TitlesScreen {
            words,
            reader,
            grid: GridState::default(),
            tiles: Vec::new(),
            source: None,
            sign_in: SignInAgain::default(),
            opened: None,
            settled: false,
            railed: false,
        }
    }

    /// The focus id of the grid's cell `index` on the page `page`.
    pub fn cell_id(page: Id, index: usize) -> Id {
        page.with("grid").with(index as u64)
    }

    fn rail_if(&mut self, cx: &mut AppCx, nothing: bool) {
        if nothing && !std::mem::replace(&mut self.railed, true) {
            cx.to_rail();
        }
    }
}

impl Screen for TitlesScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let page = ui.scope_id();
        let opened = *self.opened.get_or_insert(cx.now);
        let title_at = Point::new(theme::GUTTER, theme::SAFE_Y);
        let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;

        if let Some(gate) = summary_problem(cx.state.request_summary()) {
            self.settled = true;
            let (pressed, focusable) = problem_page(ui, cx, page.with("gate"), &gate, self.words.failed);
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

        self.reader.update(cx, true);
        let pages = self.reader.pages.clone();
        let source = (pages.titles.len(), pages.at.to_bits(), marks_version(cx));
        if self.source != Some(source) {
            self.source = Some(source);
            self.tiles =
                pages.titles.iter().map(|t| TitleTileModel::new(cx, ui, marked(cx, t, pages.at), true)).collect();
        }

        let grid_id = page.with("grid");
        let note_at = LIST_TOP + 26.0;
        if self.tiles.is_empty() && !pages.goes_on() {
            if let Some(e) = &pages.error {
                self.settled = true;
                let problem = requests_problem(e);
                let words = problem_words(&problem, false);
                let (title, body, retry) = match words {
                    Some(w) => (w.title.to_string(), w.body.to_string(), w.retry),
                    None => (failed_title(Some(e), self.words.failed).to_string(), e.problem(), true),
                };
                let mut message = Message::new(MessageStyle::Column { top: note_at }, &title).body(&body);
                if retry {
                    message = message.action(t!("requests.buttons.retry"), Icon::RefreshCw);
                }
                let r = message.show(ui, page.with("note"));
                self.rail_if(cx, !retry);
                if r.action.is_some_and(|a| a.activated) {
                    self.reader.again();
                }
            } else if self.reader.needs() {
                if cx.now - opened >= SKELETON_AFTER {
                    grid_skeleton(ui, LIST_TOP + theme::GRID_CONTENT_TOP);
                } else {
                    ui.repaint_after(SKELETON_AFTER - (cx.now - opened));
                }
            } else {
                self.settled = true;
                let (title, body) = &self.words.none;
                Message::new(MessageStyle::Column { top: note_at }, title).body(body).show(ui, page.with("note"));
                self.rail_if(cx, true);
            }
        } else {
            self.settled = true;
            self.railed = false;
            let grid = PosterGrid::new(TileKind::Poster, LIST_TOP).long_press(false);
            let cols = grid.column_count();
            let coming = !pages.settled();
            let failed = pages.error.is_some();
            let end_tile = pages.stopped || failed;
            let n = self.tiles.len();
            let count = if coming { (n + 1).div_ceil(cols) * cols } else { n + usize::from(end_tile) };
            if self.grid.index >= count {
                self.grid.index = count.saturating_sub(1);
            }
            ui.initial_focus(TitlesScreen::cell_id(page, 0));
            let tiles = &self.tiles;
            let r = grid.show(ui, grid_id, &mut self.grid, count, |ui, slot| match tiles.get(slot.index) {
                Some(t) => paint_title(ui, slot, t),
                None if end_tile => {
                    let label = if failed { t!("requests.buttons.retry") } else { t!("requests.buttons.showMore") };
                    Tile::see_all(TileKind::Poster, label).paint(ui, slot)
                }
                None => Tile::skeleton(TileKind::Poster).paint(ui, slot),
            });
            if let Some(i) = r.focused {
                // Two rows from the end: the next pages, until three rows more are in.
                if i + 2 * cols >= n {
                    self.reader.want(n + 3 * cols);
                }
            }
            if let Some(a) = r.activated {
                match self.tiles.get(a) {
                    Some(t) => cx.push(title_page(&t.title)),
                    None if end_tile => self.reader.again(),
                    None => {}
                }
            }
            // Beside a grid's lone Show more or Retry: why it has no titles yet.
            if n == 0 && end_tile {
                let (title, body) = match &pages.error {
                    Some(e) => {
                        let problem = requests_problem(e);
                        match problem_words(&problem, false) {
                            Some(w) => (w.title.to_string(), w.body.to_string()),
                            None => (failed_title(Some(e), self.words.failed).to_string(), e.problem()),
                        }
                    }
                    None => (self.words.none_yet.clone(), t!("requests.discover.nothingSoFar").to_string()),
                };
                let x = theme::GUTTER + TileKind::Poster.size().w + 28.0;
                let y = LIST_TOP + theme::GRID_CONTENT_TOP + 40.0;
                let w = theme::CANVAS_W - x - theme::SAFE_X;
                let h = ui.text(Point::new(x, y), &title, &theme::TITLE, &TextOpts::wrap(w, 2), theme::FG).h;
                ui.text(Point::new(x, y + h + 8.0), &body, &theme::BODY, &TextOpts::wrap(w, 3), theme::MUTED);
            }
        }

        // The page's title over the grid, which scrolls under it.
        ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, LIST_TOP), theme::BG, 0.0);
        ui.text(title_at, &self.words.title, &theme::PAGE_TITLE, &TextOpts::line(width), theme::FG);
        self.sign_in.show(ui, cx, page.with("sign-in"));
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        if matches!(event, AppEvent::Shown) {
            // Badges marked on a title's page show at once.
            self.source = None;
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

/// A grid's place while its first page comes.
fn grid_skeleton(ui: &mut Ui, top: f32) {
    let kind = TileKind::Poster;
    let size = kind.size();
    for r in 0..2 {
        for c in 0..6 {
            let at = Rect::new(
                theme::GUTTER + c as f32 * (size.w + theme::GRID_COLUMN_GAP),
                top + r as f32 * (size.h + 68.0),
                size.w,
                size.h,
            );
            skeleton_block(ui, at, radius::TILE);
        }
    }
}
