//! A library's listing as the grid shows it (the Android app's `LibraryListing`, the TV app's
//! `Listing` in `library-data.ts`): the first [`HEAD`] titles at once (saved first, fresh after),
//! which give the total; then, for a library of at most 5,000 titles (2,000 on Low-tier TVs), the
//! rest in pages of [`PAGE`], two requests at a time, until a short page; above that, only a window
//! of two pages around the first visible position (the page and the one before when in its first
//! half, else the one after; the last two at the end), at most [`KEPT_PAGES`] kept besides the
//! head, the least recently shown let go first and never the window's. The server's changes ask
//! for the same listing again once the load under way is done, keeping what shows meanwhile.
//!
//! [`Pager`] decides what to ask and what to let go, without any I/O, so it is tested on its own;
//! [`Listing`] runs it with the server and holds only the titles that came, never a slot per
//! position.

use std::collections::{BTreeMap, HashMap, VecDeque};

use pley_core::model::{ItemsPage, LibraryQuery};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_kit::TileKind;
use pley_shell::{AppCx, Load, Resource};

use crate::parts::answer;
use crate::tiles::Cell;
use crate::view::quality_of;

/// The first request's length: enough for the first screens, and it tells the total.
pub const HEAD: usize = 200;
/// Every later request's length.
pub const PAGE: usize = 1000;
/// Pages kept around what shows in a library too big to hold whole.
pub const KEPT_PAGES: usize = 4;
/// Libraries above this many titles are held only around what shows.
pub const WINDOW_ABOVE: usize = 5000;
/// The same on Low-tier TVs (`T0` on Android).
pub const WINDOW_ABOVE_LOW: usize = 2000;
/// The most titles a listing is taken to have: the server's total is input from outside, and a
/// grid's positions stay exact well past this.
pub const MOST_TITLES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageState {
    /// To ask at the next [`Pager::requests`].
    Wanted,
    Asked,
    Held,
    /// Couldn't be had: asked again once the window moves onto it, or the listing starts again.
    Failed,
}

/// What to ask for of a listing past its head, and what to let go: no I/O of its own.
#[derive(Clone, Debug)]
pub struct Pager {
    window_above: usize,
    total: Option<usize>,
    /// The whole listing: the next offset to ask, the offsets of the batch in flight, whether a
    /// page of it came short, and whether nothing more is to be asked.
    next: usize,
    batch: Vec<usize>,
    short: bool,
    ended: bool,
    /// A windowed listing: each page's state; the pages whose titles are held (those being asked
    /// again too), the least recently shown first; and the first page of the window shown.
    pages: HashMap<usize, PageState>,
    order: VecDeque<usize>,
    window: Option<usize>,
}

impl Pager {
    /// A pager holding listings of up to `window_above` titles whole.
    pub fn new(window_above: usize) -> Pager {
        Pager {
            window_above,
            total: None,
            next: HEAD,
            batch: Vec::new(),
            short: false,
            ended: true,
            pages: HashMap::new(),
            order: VecDeque::new(),
            window: None,
        }
    }

    /// Whether the listing is too big to hold whole.
    pub fn windowed(&self) -> bool {
        self.total.is_some_and(|t| t > self.window_above)
    }

    /// The server's head came with `total`: everything past it is asked again (a whole listing
    /// from the head's end; a windowed one, the window shown). Returns the offsets of the pages
    /// to let go.
    pub fn start(&mut self, total: usize) -> Vec<usize> {
        self.total = Some(total);
        self.next = HEAD;
        self.batch.clear();
        self.short = false;
        self.ended = total <= HEAD;
        let mut gone = Vec::new();
        if self.windowed() {
            // What shows of the window stays until its pages come again; the rest goes.
            let last = (total - 1) / PAGE;
            let window = self.window.take();
            let shown = |p: usize| p <= last && window.is_some_and(|s| p == s || p == s + 1);
            self.order.retain(|&p| {
                if !shown(p) {
                    gone.push(p * PAGE);
                }
                shown(p)
            });
            self.pages.clear();
            for &p in &self.order {
                self.pages.insert(p, PageState::Wanted);
            }
        } else {
            gone.extend(self.order.drain(..).map(|p| p * PAGE));
            self.pages.clear();
            self.window = None;
        }
        gone
    }

    /// The grid shows from position `first`: a windowed listing wants the page it is in and the
    /// one before it (when in its first half) or after it; at the end, the last two
    /// (`LibraryListing.moveWindow`).
    pub fn showing(&mut self, first: usize) {
        let Some(total) = self.total.filter(|_| self.windowed()) else { return };
        let last = (total - 1) / PAGE;
        let page = (first / PAGE).min(last);
        let start = if first % PAGE < PAGE / 2 { page.saturating_sub(1) } else { page }.min(last.saturating_sub(1));
        if self.window == Some(start) {
            return;
        }
        self.window = Some(start);
        for p in start..=(start + 1).min(last) {
            if self.order.contains(&p) {
                // Shown again: the last to let go.
                self.order.retain(|&q| q != p);
                self.order.push_back(p);
            }
            match self.pages.get(&p) {
                None | Some(PageState::Failed) => {
                    self.pages.insert(p, PageState::Wanted);
                }
                _ => {}
            }
        }
    }

    fn in_window(&self, p: usize) -> bool {
        self.window.is_some_and(|s| p == s || p == s + 1)
    }

    /// The offsets to ask now, each for [`PAGE`] titles.
    pub fn requests(&mut self) -> Vec<usize> {
        let Some(total) = self.total else { return Vec::new() };
        if self.windowed() {
            let mut wanted: Vec<usize> =
                self.pages.iter().filter(|(_, s)| **s == PageState::Wanted).map(|(p, _)| *p).collect();
            wanted.sort_unstable();
            for p in &wanted {
                self.pages.insert(*p, PageState::Asked);
            }
            return wanted.into_iter().map(|p| p * PAGE).collect();
        }
        if self.ended || !self.batch.is_empty() {
            return Vec::new();
        }
        self.batch = [self.next, self.next + PAGE].into_iter().filter(|&o| o < total).collect();
        if self.batch.is_empty() {
            self.ended = true;
        }
        self.batch.clone()
    }

    /// The page at `offset` came with `len` titles; the offsets of the pages to let go (a windowed
    /// listing keeps [`KEPT_PAGES`], the least recently shown going first, never the window's).
    pub fn answered(&mut self, offset: usize, len: usize) -> Vec<usize> {
        if self.windowed() {
            let p = offset / PAGE;
            self.pages.insert(p, PageState::Held);
            self.order.retain(|&q| q != p);
            self.order.push_back(p);
            let mut gone = Vec::new();
            while self.order.len() > KEPT_PAGES {
                let Some(at) = self.order.iter().position(|&q| !self.in_window(q)) else { break };
                let Some(old) = self.order.remove(at) else { break };
                self.pages.remove(&old);
                gone.push(old * PAGE);
            }
            return gone;
        }
        if let Some(at) = self.batch.iter().position(|&o| o == offset) {
            self.batch.remove(at);
            self.short |= len < PAGE;
            if self.batch.is_empty() {
                self.next += 2 * PAGE;
                // A page that failed in this batch has already ended the listing.
                self.ended |= self.short || self.total.is_none_or(|t| self.next >= t);
            }
        }
        Vec::new()
    }

    /// The page at `offset` couldn't be had: a whole listing stops there (a later refresh starts
    /// it again); a windowed one keeps what it showed of it, else asks again once the window moves
    /// onto it.
    pub fn failed(&mut self, offset: usize) {
        if self.windowed() {
            let p = offset / PAGE;
            let state = if self.order.contains(&p) { PageState::Held } else { PageState::Failed };
            self.pages.insert(p, state);
            return;
        }
        self.batch.retain(|&o| o != offset);
        self.ended = true;
    }

    /// Whether pages are being asked for.
    pub fn loading(&self) -> bool {
        !self.batch.is_empty() || self.pages.values().any(|s| matches!(s, PageState::Wanted | PageState::Asked))
    }
}

/// A page of the listing on its way: its offset, the listing it was asked for, and the cells it
/// will hold.
type PageRequest = (usize, u64, Promise<Result<Vec<Cell>, ApiError>>);

/// The server's total, bounded.
fn bounded(total: i64) -> usize {
    usize::try_from(total.clamp(0, MOST_TITLES as i64)).unwrap_or(0)
}

/// A library's listing in one view, as cells for the grid.
pub struct Listing {
    library: i64,
    query: LibraryQuery,
    kind: TileKind,
    head: Resource<ItemsPage>,
    pager: Pager,
    total: Option<usize>,
    /// The head's titles.
    first: Vec<Cell>,
    /// The titles past it by their page's offset: a whole listing's from [`HEAD`] on, a windowed
    /// one's at multiples of [`PAGE`] (its first page under the head too).
    pages: BTreeMap<usize, Vec<Cell>>,
    held: usize,
    requests: Vec<PageRequest>,
    /// Bumped each time the listing starts again: older answers are let go.
    generation: u64,
    /// The server was to be asked again while a load was under way: it is, once that is done.
    refresh_due: bool,
    version: u64,
    /// The width the tiles' pictures are asked at, in device pixels.
    px: u32,
}

impl Listing {
    /// The listing of `library` in `query`'s view, its tiles `kind`; held whole up to
    /// `window_above` titles.
    pub fn new(library: i64, query: LibraryQuery, kind: TileKind, window_above: usize) -> Listing {
        let key = snapshot_keys::library_head(library, &query.key());
        let q = query.clone();
        let head = Resource::new(Some(key), move |api| api.library_items(library, &q, 0, HEAD as i64));
        Listing {
            library,
            query,
            kind,
            head,
            pager: Pager::new(window_above),
            total: None,
            first: Vec::new(),
            pages: BTreeMap::new(),
            held: 0,
            requests: Vec::new(),
            generation: 0,
            refresh_due: false,
            version: 0,
            px: 0,
        }
    }

    /// Asks for what is needed and collects what came; whether what shows changed. `first` is the
    /// first position the grid shows; `px` the width pictures are asked at.
    pub fn update(&mut self, cx: &mut AppCx, first: usize, px: u32) -> bool {
        self.px = px;
        let mut changed = false;
        if self.head.update(cx) {
            changed = true;
            self.take_head(cx);
        }
        self.pager.showing(first);
        for offset in self.pager.requests() {
            let (library, query, kind, px) = (self.library, self.query.clone(), self.kind, self.px);
            let quality = quality_of(&query);
            let promise = cx.spawn(move |api| {
                let page = api.library_items(library, &query, offset as i64, PAGE as i64)?;
                Ok(page.items.into_iter().take(PAGE).map(|item| Cell::new(item, api, kind, px, quality)).collect())
            });
            self.requests.push((offset, self.generation, promise));
        }
        let mut i = 0;
        while i < self.requests.len() {
            let (offset, generation, p) = &mut self.requests[i];
            let (offset, generation) = (*offset, *generation);
            let Some(result) = answer(p) else {
                i += 1;
                continue;
            };
            self.requests.remove(i);
            if generation != self.generation {
                continue;
            }
            match result {
                Ok(cells) => {
                    let len = cells.len();
                    self.pages.insert(offset, cells);
                    for gone in self.pager.answered(offset, len) {
                        self.pages.remove(&gone);
                    }
                    changed = true;
                }
                Err(e) if e.cancelled() => self.pager.failed(offset),
                Err(e) => {
                    cx.state.report_failure(&e);
                    self.pager.failed(offset);
                }
            }
        }
        if self.refresh_due && !self.loading() {
            self.refresh_due = false;
            self.head.invalidate();
        }
        if changed {
            self.held = self.first.len() + self.pages.values().map(Vec::len).sum::<usize>();
            self.version = self.version.wrapping_add(1);
        }
        changed
    }

    fn take_head(&mut self, cx: &mut AppCx) {
        let (total, items, fresh) = match self.head.state() {
            Load::Ready { value, fresh } => (bounded(value.total), &value.items, fresh),
            Load::Failed { stale: Some(value), .. } => (bounded(value.total), &value.items, false),
            _ => return,
        };
        let Some(api) = cx.api() else { return };
        let quality = quality_of(&self.query);
        self.first =
            items.iter().take(HEAD).map(|item| Cell::new(item.clone(), api, self.kind, self.px, quality)).collect();
        self.total = Some(total);
        // A listing that got shorter lets go of what is past its end.
        self.pages.retain(|&start, _| start < total);
        for (start, cells) in self.pages.range_mut(..) {
            cells.truncate(total - start);
        }
        if fresh {
            let was = self.pager.windowed();
            let gone = self.pager.start(total);
            if was == self.pager.windowed() {
                for offset in gone {
                    self.pages.remove(&offset);
                }
            } else {
                // Whole and windowed listings lay their pages out apart.
                self.pages.clear();
            }
            self.generation += 1;
            for (_, _, p) in self.requests.drain(..) {
                p.cancel();
            }
        }
    }

    /// Whether the server is being asked for any of the listing.
    fn loading(&self) -> bool {
        self.head.asking() || !self.requests.is_empty()
    }

    /// Asks the server again, keeping what shows (its events, Retry); while a load is under way,
    /// once it is done.
    pub fn refresh(&mut self) {
        if self.loading() {
            self.refresh_due = true;
        } else {
            self.head.invalidate();
        }
    }

    /// Asks again if the answer is older than 30 s (the page shows again), unless it is being
    /// asked already.
    pub fn revalidate(&mut self, now: f64) {
        if !self.loading() {
            self.head.revalidate(now);
        }
    }

    /// How many titles match, once known.
    pub fn total(&self) -> Option<usize> {
        self.total
    }

    /// The cell at `i`, once its page has come.
    pub fn at(&self, i: usize) -> Option<&Cell> {
        if self.total.is_none_or(|t| i >= t) {
            return None;
        }
        if let Some(cell) = self.first.get(i) {
            return Some(cell);
        }
        let (&start, cells) = self.pages.range(..=i).next_back()?;
        cells.get(i - start)
    }

    /// Why the head couldn't be had, when nothing is known of the listing.
    pub fn error(&self) -> Option<&ApiError> {
        if self.head.value().is_some() { None } else { self.head.error() }
    }

    /// Whether the whole listing is held (the A–Z jump needs it).
    pub fn whole(&self) -> bool {
        self.total.is_some_and(|t| !self.pager.windowed() && self.held == t)
    }

    /// Whether only the part around what shows is held.
    pub fn windowed(&self) -> bool {
        self.pager.windowed()
    }

    /// Bumped whenever cells change.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Every cell, for the A–Z index (only meaningful when [`Listing::whole`]).
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.first.iter().chain(self.pages.values().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_listing_comes_two_pages_at_a_time_after_its_head() {
        let mut p = Pager::new(WINDOW_ABOVE);
        assert!(p.requests().is_empty(), "nothing before the head");
        p.start(3500);
        assert!(!p.windowed());
        assert_eq!(p.requests(), vec![200, 1200]);
        assert!(p.requests().is_empty(), "one batch at a time");
        p.answered(1200, PAGE);
        assert!(p.requests().is_empty());
        p.answered(200, PAGE);
        assert_eq!(p.requests(), vec![2200, 3200]);
        p.answered(2200, PAGE);
        p.answered(3200, 300);
        assert!(p.requests().is_empty());
        assert!(!p.loading());
    }

    #[test]
    fn a_short_page_ends_the_listing() {
        let mut p = Pager::new(WINDOW_ABOVE);
        p.start(4000);
        assert_eq!(p.requests(), vec![200, 1200]);
        p.answered(200, PAGE);
        // The library shrank while it was read: a short page ends it.
        p.answered(1200, 10);
        assert!(p.requests().is_empty());
        let mut small = Pager::new(WINDOW_ABOVE);
        small.start(150);
        assert!(small.requests().is_empty(), "the head held it all");
        small.start(700);
        assert_eq!(small.requests(), vec![200]);
    }

    #[test]
    fn a_failed_page_stops_until_the_listing_starts_again() {
        let mut p = Pager::new(WINDOW_ABOVE);
        p.start(3000);
        assert_eq!(p.requests(), vec![200, 1200]);
        p.failed(200);
        p.answered(1200, PAGE);
        assert!(p.requests().is_empty());
        p.start(3000);
        assert_eq!(p.requests(), vec![200, 1200]);
    }

    #[test]
    fn a_big_library_is_held_around_what_shows() {
        let mut p = Pager::new(WINDOW_ABOVE);
        p.start(20_000);
        assert!(p.windowed());
        assert!(p.requests().is_empty(), "nothing until the grid says where it is");
        p.showing(0);
        assert_eq!(p.requests(), vec![0, 1000]);
        // In a page's first half, the page before it too; in its second half, the one after.
        p.showing(5200);
        assert_eq!(p.requests(), vec![4000, 5000]);
        p.showing(5600);
        assert_eq!(p.requests(), vec![6000]);
        assert!(p.answered(0, PAGE).is_empty());
        assert!(p.answered(1000, PAGE).is_empty());
        assert!(p.answered(4000, PAGE).is_empty());
        assert!(p.answered(5000, PAGE).is_empty());
        // A fifth page lets the least recently shown go (the head stays with the listing).
        assert_eq!(p.answered(6000, PAGE), vec![0]);
        p.showing(500);
        assert_eq!(p.requests(), vec![0]);
        assert_eq!(p.answered(0, PAGE), vec![4000]);
        // At the end the window holds the last two pages, never past the end.
        p.showing(19_900);
        assert_eq!(p.requests(), vec![18_000, 19_000]);
    }

    #[test]
    fn scrolling_back_never_lets_go_of_the_window() {
        let mut p = Pager::new(WINDOW_ABOVE);
        p.start(20_000);
        for first in [0, 2600, 4600] {
            p.showing(first);
            for offset in p.requests() {
                p.answered(offset, PAGE);
            }
        }
        // Back into pages 1 and 2: page 1 comes again and page 3 goes, not page 2 on screen.
        p.showing(2100);
        assert_eq!(p.requests(), vec![1000]);
        assert_eq!(p.answered(1000, PAGE), vec![3000]);
        assert!(p.requests().is_empty(), "page 2 is still held");
    }

    #[test]
    fn a_failed_window_page_waits_for_the_window_to_come_back() {
        let mut p = Pager::new(WINDOW_ABOVE);
        p.start(9000);
        p.showing(2600);
        assert_eq!(p.requests(), vec![2000, 3000]);
        p.failed(2000);
        p.answered(3000, PAGE);
        p.showing(2600);
        assert!(p.requests().is_empty(), "not again at every frame");
        p.showing(4600);
        assert_eq!(p.requests(), vec![4000, 5000]);
        p.showing(2600);
        assert_eq!(p.requests(), vec![2000]);
    }

    #[test]
    fn low_tier_tvs_hold_less_whole() {
        let mut p = Pager::new(WINDOW_ABOVE_LOW);
        p.start(3000);
        assert!(p.windowed());
        let mut q = Pager::new(WINDOW_ABOVE);
        q.start(3000);
        assert!(!q.windowed());
    }

    #[test]
    fn a_fresh_head_asks_again_for_the_window_shown() {
        let mut p = Pager::new(WINDOW_ABOVE);
        p.start(9000);
        p.showing(600);
        assert_eq!(p.requests(), vec![0, 1000]);
        p.answered(0, PAGE);
        p.answered(1000, PAGE);
        p.showing(2600);
        assert_eq!(p.requests(), vec![2000, 3000]);
        p.answered(2000, PAGE);
        p.answered(3000, PAGE);
        // Pages 0 and 1 are out of the window: they go; 2 and 3 show until they come again.
        let mut gone = p.start(9000);
        gone.sort_unstable();
        assert_eq!(gone, vec![0, 1000]);
        p.showing(2600);
        assert_eq!(p.requests(), vec![2000, 3000]);
        // A library that shrank below the window is listed whole again.
        let mut gone = p.start(4000);
        gone.sort_unstable();
        assert_eq!(gone, vec![2000, 3000]);
        assert!(!p.windowed());
        assert_eq!(p.requests(), vec![200, 1200]);
    }

    #[test]
    fn totals_are_bounded() {
        assert_eq!(bounded(-5), 0);
        assert_eq!(bounded(1500), 1500);
        assert_eq!(bounded(i64::MAX), MOST_TITLES);
    }
}
