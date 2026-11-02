//! A list from TMDB read page by page (`usePages` in `discover.tsx`): until it holds the titles
//! wanted or TMDB has no more, at most 5 pages at a time. For a profile with an age limit page
//! after page can come empty, and each costs the server TMDB's details of its titles, so a fill
//! that found too few stops, and reads on only when asked (Show more), as after a failure (Retry).

use std::sync::Arc;

use pley_core::model::{DiscoverPage, DiscoverTitle};
use pley_core::requests::add_titles;
use pley_core::{Api, ApiError, Promise};
use pley_shell::AppCx;

use crate::common::answer_of;

/// TMDB has at most 500 pages of anything.
pub const MAX_PAGES: i64 = 500;
/// A list reads at most this many pages at a time.
pub const FILL_PAGES: usize = 5;

/// A list as far as it was read: page by page, each title once.
#[derive(Clone, Debug)]
pub struct Pages {
    pub titles: Vec<DiscoverTitle>,
    /// The last page read.
    pub page: i64,
    pub total_pages: i64,
    /// When the first page came (the app's clock): badges marked since are newer than the list's.
    pub at: f64,
    pub error: Option<ApiError>,
    /// The last fill read its pages without finding the titles it wanted: more are read when
    /// asked for (Show more).
    pub stopped: bool,
}

impl Default for Pages {
    fn default() -> Pages {
        Pages { titles: Vec::new(), page: 0, total_pages: 1, at: 0.0, error: None, stopped: false }
    }
}

impl Pages {
    /// Every page read, or as many as TMDB gives: a short page, even an empty one, isn't the end.
    pub fn done(&self) -> bool {
        self.page >= self.total_pages.min(MAX_PAGES)
    }

    /// Nothing more is read unless asked for: every page read, or a fill stopped short or failed.
    pub fn settled(&self) -> bool {
        self.done() || self.stopped || self.error.is_some()
    }

    /// With its next page.
    fn add(&mut self, next: DiscoverPage, now: f64) {
        add_titles(&mut self.titles, next.results);
        self.page += 1;
        self.total_pages = next.total_pages;
        if self.at == 0.0 {
            self.at = now;
        }
        self.error = None;
        self.stopped = false;
    }

    /// No titles, but more pages to read since a fill stopped short: a grid stays, its one tile
    /// Show more (Retry after a failure) and its wells while it reads on, so focus stays in it.
    pub fn goes_on(&self) -> bool {
        self.titles.is_empty() && self.page >= FILL_PAGES as i64 && !self.done()
    }
}

type FetchFn = Arc<dyn Fn(&Api, i64) -> Result<DiscoverPage, ApiError> + Send + Sync>;

/// A list being read: what it holds, and the fill on its way.
pub struct Reader {
    pub pages: Pages,
    fetch: FetchFn,
    job: Option<Promise<Result<DiscoverPage, ApiError>>>,
    /// Pages this fill may still read.
    left: usize,
    /// The titles wanted.
    want: usize,
    generation: Option<u64>,
}

impl Reader {
    /// A list read with `fetch(api, page)` until it holds `want` titles.
    pub fn new(
        want: usize,
        fetch: impl Fn(&Api, i64) -> Result<DiscoverPage, ApiError> + Send + Sync + 'static,
    ) -> Reader {
        Reader { pages: Pages::default(), fetch: Arc::new(fetch), job: None, left: 0, want, generation: None }
    }

    /// Wants at least `n` titles.
    pub fn want(&mut self, n: usize) {
        self.want = self.want.max(n);
    }

    /// More is to be read, now (`enabled`) or as soon as it may.
    pub fn needs(&self) -> bool {
        !self.pages.settled() && self.pages.titles.len() < self.want
    }

    /// Starts a fill when one is needed and `enabled`, and takes what came back; whether what it
    /// holds changed.
    pub fn update(&mut self, cx: &mut AppCx, enabled: bool) -> bool {
        let generation = cx.state.generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.pages = Pages::default();
            self.job = None;
        }
        let mut changed = false;
        if let Some(answer) = self.job.as_mut().and_then(answer_of) {
            self.job = None;
            match answer {
                Ok(page) => {
                    let had = self.pages.titles.len();
                    self.pages.add(page, cx.now);
                    self.left = self.left.saturating_sub(1);
                    let short = !self.pages.done() && self.pages.titles.len() < self.want;
                    if short && self.left == 0 {
                        self.pages.stopped = true;
                    }
                    changed = self.pages.titles.len() != had || !short || self.left == 0;
                    if short && self.left > 0 {
                        self.next(cx);
                    }
                }
                Err(e) if e.cancelled() => {}
                Err(e) => {
                    self.pages.error = Some(e);
                    changed = true;
                }
            }
        }
        if self.job.is_none() && enabled && self.needs() {
            self.left = FILL_PAGES;
            self.next(cx);
        }
        changed
    }

    fn next(&mut self, cx: &AppCx) {
        let (fetch, page) = (self.fetch.clone(), self.pages.page + 1);
        self.job = Some(cx.spawn(move |api| fetch(api, page)));
    }

    /// Reads on after a fill that stopped short or failed.
    pub fn again(&mut self) {
        self.pages.error = None;
        self.pages.stopped = false;
    }
}
