//! A category's channels, a page of 100 at a time as they come into view
//! (packages/tv/src/live/pages.ts, the web's `useChannelList`): a lineup of
//! tens of thousands costs what's on screen. It knows where a channel is in
//! the list (by the server's `around`), and the channel before or after one,
//! round the ends (channel up and down within the category).
//!
//! Answers come through the task pool: the owner calls
//! [`ChannelPages::poll`] each frame (with the app's clock in ms).

use std::collections::HashMap;
use std::sync::Arc;

use pley_core::ApiError;
use pley_core::model::{LiveChannel, LiveChannelPage};
use pley_core::task::{Promise, Tasks};

use crate::live::data::{CHANNEL_PAGE, Category};
use crate::server::PlayerServer;

/// A page asked for this long ago is asked for again as it shows: what's on moves on.
const STALE_MS: i64 = 60_000;
/// A page that failed waits this long before it is asked for again.
const RETRY_MS: i64 = 10_000;

struct Page {
    channels: Option<Vec<LiveChannel>>,
    at: i64,
    pending: bool,
}

enum Asked {
    Page(i64),
    Around,
}

/// Channel up or down, under way: from `id`, `by` places.
struct Neighbor {
    id: i64,
    by: i64,
    /// Where `id` is: none until known; `Some(None)` when it isn't in the list.
    index: Option<Option<i64>>,
    asked_around: bool,
}

/// A category's channels, by pages.
pub struct ChannelPages {
    server: Arc<dyn PlayerServer>,
    tasks: Tasks,
    category: Category,
    /// Pages refreshed only when asked for again: the player's list keeps the order it opened with.
    frozen: bool,
    pages: HashMap<i64, Page>,
    total: Option<i64>,
    error: Option<ApiError>,
    version: u64,
    jobs: Vec<(Asked, Promise<Result<LiveChannelPage, ApiError>>)>,
    neighbor: Option<Neighbor>,
    found: Option<Option<LiveChannel>>,
}

impl ChannelPages {
    /// The pages of `category`; `frozen` for the player's zapping list.
    pub fn new(server: Arc<dyn PlayerServer>, tasks: Tasks, category: Category, frozen: bool) -> ChannelPages {
        ChannelPages {
            server,
            tasks,
            category,
            frozen,
            pages: HashMap::new(),
            total: None,
            error: None,
            version: 0,
            jobs: Vec::new(),
            neighbor: None,
            found: None,
        }
    }

    /// The category listed.
    pub fn category(&self) -> &Category {
        &self.category
    }

    /// Channels in the whole list; none until the first page is in.
    pub fn total(&self) -> Option<i64> {
        self.total
    }

    /// The first page failed (and none came since).
    pub fn error(&self) -> Option<&ApiError> {
        self.error.as_ref()
    }

    /// Changes as pages come in.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether an answer is still to come.
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// The channel at `index`, once its page is in.
    pub fn at(&self, index: i64) -> Option<&LiveChannel> {
        if index < 0 {
            return None;
        }
        self.pages.get(&(index / CHANNEL_PAGE))?.channels.as_ref()?.get((index % CHANNEL_PAGE) as usize)
    }

    /// Where channel `id` is among the pages in, if it is.
    pub fn find(&self, id: i64) -> Option<i64> {
        let mut found: Option<i64> = None;
        for (n, p) in &self.pages {
            if let Some(i) = p.channels.as_ref().and_then(|c| c.iter().position(|c| c.id == id)) {
                let at = n * CHANNEL_PAGE + i as i64;
                found = Some(found.map_or(at, |f| f.min(at)));
            }
        }
        found
    }

    /// Asks for the pages covering `from` to `to` (and the first, which says how many there are) that it doesn't have fresh.
    pub fn want(&mut self, from: i64, to: i64, now: i64) {
        let last = self.total.map_or(0, |t| ((t + CHANNEL_PAGE - 1) / CHANNEL_PAGE - 1).max(0));
        let first = last.min((from / CHANNEL_PAGE).max(0));
        let end = last.min(first.max(from.max(to) / CHANNEL_PAGE));
        self.load(0, now);
        for n in first..=end {
            self.load(n, now);
        }
    }

    /// Page `n`, asked for unless it is fresh or on its way.
    pub fn load(&mut self, n: i64, now: i64) {
        if let Some(p) = self.pages.get(&n) {
            let fresh = match p.channels {
                Some(_) => self.frozen || now - p.at < STALE_MS,
                None => now - p.at < RETRY_MS,
            };
            if p.pending || fresh {
                return;
            }
        }
        let channels = self.pages.remove(&n).and_then(|p| p.channels);
        self.pages.insert(n, Page { channels, at: now, pending: true });
        let job = self.ask(n * CHANNEL_PAGE, None);
        self.jobs.push((Asked::Page(n), job));
    }

    fn ask(&self, offset: i64, around: Option<i64>) -> Promise<Result<LiveChannelPage, ApiError>> {
        let server = self.server.clone();
        let browse = self.category.browse(offset, CHANNEL_PAGE, around);
        self.tasks.spawn(move || server.live_browse(&browse))
    }

    /// The channel `by` places from `id`, round the ends of the list (the
    /// first channel when `id` isn't in it, tuned by its number say): asked
    /// now, answered by [`ChannelPages::take_neighbor`] once known.
    pub fn neighbor(&mut self, id: i64, by: i64, now: i64) {
        self.found = None;
        self.neighbor = Some(Neighbor { id, by, index: None, asked_around: false });
        self.advance_neighbor(now);
    }

    /// The channel [`ChannelPages::neighbor`] found, once (`Some(None)`: there is none).
    pub fn take_neighbor(&mut self) -> Option<Option<LiveChannel>> {
        self.found.take()
    }

    fn advance_neighbor(&mut self, now: i64) {
        let Some(n) = &self.neighbor else { return };
        let (id, by) = (n.id, n.by);
        if n.index.is_none() {
            if let Some(i) = self.find(id) {
                self.neighbor.as_mut().expect("checked").index = Some(Some(i));
            } else if !n.asked_around {
                self.neighbor.as_mut().expect("checked").asked_around = true;
                let job = self.ask(0, Some(id));
                self.jobs.push((Asked::Around, job));
                return;
            } else {
                return;
            }
        }
        let index = self.neighbor.as_ref().and_then(|n| n.index).flatten();
        let Some(total) = self.total.filter(|t| *t > 0) else {
            self.neighbor = None;
            self.found = Some(None);
            return;
        };
        let j = index.map_or(0, |at| (at + by).rem_euclid(total));
        if let Some(c) = self.at(j) {
            self.found = Some(Some(c.clone()));
            self.neighbor = None;
            return;
        }
        let page = j / CHANNEL_PAGE;
        match self.pages.get(&page) {
            Some(p) if p.pending => {}
            // Asked and answered, and still not there: there is none.
            Some(p) if p.channels.is_some() || now - p.at < RETRY_MS => {
                self.found = Some(None);
                self.neighbor = None;
            }
            _ => self.load(page, now),
        }
    }

    /// A channel changed here (a favourite marked): the pages show it at once.
    pub fn patch(&mut self, id: i64, change: impl Fn(&mut LiveChannel)) {
        for p in self.pages.values_mut() {
            if let Some(c) = p.channels.as_mut().and_then(|c| c.iter_mut().find(|c| c.id == id)) {
                change(c);
            }
        }
        self.version += 1;
    }

    /// Everything is asked for again as it shows (the lineup changed on the server).
    pub fn stale(&mut self) {
        for p in self.pages.values_mut() {
            p.at = i64::MIN / 2;
        }
        self.version += 1;
    }

    /// Takes the answers that came; true when what shows changed.
    pub fn poll(&mut self, now: i64) -> bool {
        let before = self.version;
        let mut i = 0;
        while i < self.jobs.len() {
            if !self.jobs[i].1.ready() && !self.jobs[i].1.failed() {
                i += 1;
                continue;
            }
            let (asked, mut job) = self.jobs.swap_remove(i);
            let answer = job.take().unwrap_or(Err(ApiError::Net(pley_core::NetError::Cancelled)));
            match asked {
                Asked::Page(n) => {
                    if let Some(p) = self.pages.get_mut(&n) {
                        p.pending = false;
                        p.at = now;
                    }
                    match answer {
                        Ok(page) => {
                            self.keep(page, now);
                            if n == 0 {
                                self.error = None;
                            }
                        }
                        Err(e) => {
                            if n == 0 && self.total.is_none() {
                                self.error = Some(e);
                            }
                            self.version += 1;
                        }
                    }
                }
                Asked::Around => match answer {
                    Ok(page) => {
                        let at = page.channels.iter().position(|c| Some(c.id) == self.neighbor.as_ref().map(|n| n.id));
                        let index = at.map(|a| page.offset + a as i64);
                        self.keep(page, now);
                        if let Some(n) = &mut self.neighbor {
                            n.index = Some(index);
                        }
                    }
                    Err(_) => {
                        if self.neighbor.take().is_some() {
                            self.found = Some(None);
                        }
                    }
                },
            }
        }
        self.advance_neighbor(now);
        self.version != before || self.found.is_some()
    }

    fn keep(&mut self, answer: LiveChannelPage, now: i64) {
        let n = answer.offset / CHANNEL_PAGE;
        let page = self.pages.entry(n).or_insert(Page { channels: None, at: now, pending: false });
        page.channels = Some(answer.channels);
        page.at = now;
        self.total = Some(answer.total);
        self.version += 1;
    }
}
