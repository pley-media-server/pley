//! What Home asks ahead of a press: the details of the titles it shows, and the server's read-ahead
//! of what Play would play (`POST /api/items/{id}/prefetch`) once focus rests on a video
//! (`packages/tv/src/ui/screens/home.tsx` `useDetail`, `item-actions.ts` `prefetchPlayback`;
//! Android's `HomeViewModel.kt` and `CachedRepositories.kt`).
//!
//! The details of what shows now (the hero's, the tile focus rests on) are saved first, fresh
//! after: what was saved under `item:<id>` (as the title's page saves it) shows at once, and the
//! server's answer replaces it and is saved in turn. The details of what may show next (the next
//! two tiles, the first three of Continue Watching) are only asked of the server, behind the
//! read-ahead. Home keeps at most two requests on the task pool, one for each, so a thread stays
//! free for the page a press opens.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use pley_core::model::{ItemDetail, ItemSummary};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_shell::AppCx;

use crate::model::{Facts, reads_ahead};

/// Details asked for ahead, waiting their turn, at most; the oldest wish gives way (Android's
/// prefetch queue holds 8).
const QUEUED: usize = 8;
/// Facts kept; past this the one used longest ago goes (the app's own cache keeps the 40 newest
/// details for 5 minutes).
const KEPT: usize = 64;
/// How long the server isn't asked again to read ahead the same title (`item-actions.ts:55-75`).
pub const READ_AHEAD_AGAIN: f64 = 60.0;

type DetailAnswer = Promise<Result<ItemDetail, ApiError>>;

/// The last title a server was asked to read ahead, and when: one for the whole app, as the TV
/// app's module-level `prefetching`, so a Home made again doesn't ask again within the minute.
static READ_AHEAD: Mutex<Option<(String, i64, f64)>> = Mutex::new(None);

/// Why Home wants a title's details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wish {
    /// The hero shows it.
    Hero,
    /// The info block shows it: focus rests on its tile.
    Rested,
    /// Focus may rest on it next.
    Ahead,
}

/// How Home came to know a title's facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Known {
    /// Read from what was saved: shown, and the server is still asked.
    Saved,
    /// The server's answer from before its state changed: shown until the new one comes.
    Stale,
    /// The server's answer (or the app's cache of it) since.
    Fresh,
}

struct Fact {
    facts: Facts,
    known: Known,
    /// When it was last wanted for what shows, to keep what is in use.
    used: u64,
}

/// The request for what shows now.
enum Now {
    Saved(i64, Promise<Option<ItemDetail>>),
    Server(i64, DetailAnswer),
}

/// The request for what may show next.
enum Next {
    ReadAhead(Promise<Result<(), ApiError>>),
    Details(i64, DetailAnswer),
}

/// Home's details and read-ahead.
#[derive(Default)]
pub struct Ahead {
    generation: Option<u64>,
    facts: HashMap<i64, Fact>,
    tick: u64,
    /// Bumped whenever a title's facts change, so what is worked out from them is again.
    version: u64,
    /// Titles whose saved details were looked for (found or not).
    read: HashSet<i64>,
    /// Asked and refused this time: not asked again until the server's state changes.
    refused: HashSet<i64>,
    now: Option<Now>,
    /// The tile focus rests on and the hero, waiting for the request before them.
    rested_next: Option<i64>,
    hero_next: Option<i64>,
    next: Option<Next>,
    /// The title to read ahead, which goes before the details asked for ahead.
    read_next: Option<i64>,
    queue: VecDeque<i64>,
}

impl Ahead {
    /// The facts of `id`, once known.
    pub fn get(&self, id: i64) -> Option<&Facts> {
        self.facts.get(&id).map(|f| &f.facts)
    }

    /// Changes whenever a title's facts do.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether what is known of `id` is all that will show before the server answers: its facts, or
    /// that nothing was saved of it.
    pub fn settled(&self, id: i64) -> bool {
        self.facts.contains_key(&id) || self.read.contains(&id)
    }

    /// Wants `id`'s details for `wish`: from the app's cache when it has them, else what was saved
    /// (for what shows) and then the server.
    pub fn want(&mut self, id: i64, wish: Wish, cx: &mut AppCx) {
        self.reset_if_new(cx);
        self.tick += 1;
        let tick = self.tick;
        let known = self.facts.get_mut(&id).map(|f| {
            if wish != Wish::Ahead {
                f.used = tick;
            }
            f.known
        });
        if known == Some(Known::Fresh) || self.asking(id) {
            return;
        }
        if known != Some(Known::Stale)
            && let Some(detail) = cx.state.cached_item(id)
        {
            let facts = Facts::of(detail);
            self.keep(id, facts, Known::Fresh);
            return;
        }
        let saved = wish != Wish::Ahead && known.is_none() && !self.read.contains(&id);
        if !saved && self.refused.contains(&id) {
            return;
        }
        match wish {
            Wish::Hero => self.hero_next = Some(id),
            Wish::Rested => self.rested_next = Some(id),
            Wish::Ahead => {
                if self.hero_next == Some(id) || self.rested_next == Some(id) || self.queue.contains(&id) {
                    return;
                }
                self.queue.push_back(id);
                if self.queue.len() > QUEUED {
                    self.queue.pop_front();
                }
            }
        }
        if wish != Wish::Ahead {
            self.queue.retain(|q| *q != id);
        }
        self.start(cx);
    }

    /// Asks the server to get `item` ready to play, if it is a video, before the details asked for
    /// ahead; never the same title twice within a minute.
    pub fn read_ahead(&mut self, item: &ItemSummary, cx: &mut AppCx) {
        if !reads_ahead(item) || cx.api().is_none() {
            return;
        }
        self.reset_if_new(cx);
        self.read_next = Some(item.id);
        self.start(cx);
    }

    /// Collects what came back and starts what waits; whether a title's facts changed.
    pub fn poll(&mut self, cx: &mut AppCx) -> bool {
        self.reset_if_new(cx);
        let mut changed = false;
        match self.now.take() {
            Some(Now::Saved(id, mut p)) if p.ready() || p.failed() => {
                self.read.insert(id);
                if let Some(detail) = p.take().flatten()
                    && !self.facts.contains_key(&id)
                {
                    self.keep(id, Facts::of(&detail), Known::Saved);
                    changed = true;
                }
                self.now = self.ask_server(id, cx).map(|p| Now::Server(id, p));
            }
            Some(Now::Server(id, mut p)) => match answer_of(&mut p) {
                Some(a) => changed |= self.answered(id, a, cx),
                None => self.now = Some(Now::Server(id, p)),
            },
            other => self.now = other,
        }
        match self.next.take() {
            Some(Next::ReadAhead(mut p)) => {
                if p.take().is_none() && !p.failed() {
                    self.next = Some(Next::ReadAhead(p));
                }
            }
            Some(Next::Details(id, mut p)) => match answer_of(&mut p) {
                Some(a) => changed |= self.answered(id, a, cx),
                None => self.next = Some(Next::Details(id, p)),
            },
            None => {}
        }
        self.start(cx);
        changed
    }

    /// The server's state changed (watch marks, a scan): what is known still shows, but the next
    /// wish for it asks again.
    pub fn forget(&mut self) {
        for f in self.facts.values_mut() {
            if f.known == Known::Fresh {
                f.known = Known::Stale;
            }
        }
        self.refused.clear();
    }

    /// Another server, sign-in or profile: nothing known holds.
    fn reset_if_new(&mut self, cx: &AppCx) {
        let generation = cx.state.generation();
        if self.generation != Some(generation) {
            *self = Ahead { generation: Some(generation), ..Ahead::default() };
        }
    }

    fn asking(&self, id: i64) -> bool {
        let now = match &self.now {
            Some(Now::Saved(a, _) | Now::Server(a, _)) => *a == id,
            None => false,
        };
        now || matches!(&self.next, Some(Next::Details(a, _)) if *a == id)
    }

    fn keep(&mut self, id: i64, facts: Facts, known: Known) {
        if self.facts.len() >= KEPT
            && !self.facts.contains_key(&id)
            && let Some(oldest) = self.facts.iter().min_by_key(|(_, f)| f.used).map(|(id, _)| *id)
        {
            self.facts.remove(&oldest);
        }
        self.tick += 1;
        self.facts.insert(id, Fact { facts, known, used: self.tick });
        self.version += 1;
    }

    fn answered(&mut self, id: i64, answer: Result<ItemDetail, ApiError>, cx: &mut AppCx) -> bool {
        match answer {
            Ok(detail) => {
                self.keep(id, Facts::of(&detail), Known::Fresh);
                save(id, &detail, cx);
                cx.state.keep_item(detail);
                true
            }
            Err(e) if e.cancelled() => false,
            Err(e) => {
                log::debug!("details of {id} not loaded: {e}");
                cx.state.report_failure(&e);
                self.refused.insert(id);
                false
            }
        }
    }

    fn ask_server(&self, id: i64, cx: &AppCx) -> Option<DetailAnswer> {
        let fresh = self.facts.get(&id).is_some_and(|f| f.known == Known::Fresh);
        (!fresh && !self.refused.contains(&id)).then(|| cx.spawn(move |api| api.item(id)))
    }

    /// The request for what shows now: what was saved, if not looked for yet, else the server.
    fn ask_now(&mut self, id: i64, cx: &AppCx) -> Option<Now> {
        if !self.facts.contains_key(&id) && !self.read.contains(&id) {
            if let (Some(server), Some(profile)) = (cx.state.server(), cx.state.profile()) {
                let (server, snapshots, key) = (server.to_string(), cx.snapshots().clone(), snapshot_keys::item(id));
                let saved = cx.tasks().spawn(move || snapshots.get::<ItemDetail>(&server, profile, &key));
                return Some(Now::Saved(id, saved));
            }
            self.read.insert(id);
        }
        self.ask_server(id, cx).map(|p| Now::Server(id, p))
    }

    fn start(&mut self, cx: &mut AppCx) {
        // The tile focus rests on first: the person looks at it now.
        while self.now.is_none()
            && let Some(id) = self.rested_next.take().or_else(|| self.hero_next.take())
        {
            self.now = self.ask_now(id, cx);
        }
        if self.next.is_none()
            && let Some(id) = self.read_next.take()
        {
            self.next = read_ahead(id, cx).map(Next::ReadAhead);
        }
        while self.next.is_none()
            && let Some(id) = self.queue.pop_front()
        {
            if !self.asking(id) {
                self.next = self.ask_server(id, cx).map(|p| Next::Details(id, p));
            }
        }
    }
}

/// A request's answer once it came (a stopped request as a failure).
pub(crate) fn answer_of<T>(p: &mut Promise<Result<T, ApiError>>) -> Option<Result<T, ApiError>> {
    match p.take() {
        Some(a) => Some(a),
        None if p.failed() => Some(Err(ApiError::Unreadable("the request stopped".into()))),
        None => None,
    }
}

/// Saves `detail` as the title's page does, for the next start and while the server is away.
fn save(id: i64, detail: &ItemDetail, cx: &AppCx) {
    let (Some(server), Some(profile)) = (cx.state.server(), cx.state.profile()) else { return };
    let (server, snapshots, detail) = (server.to_string(), cx.snapshots().clone(), detail.clone());
    cx.tasks().detach(move || {
        if let Err(e) = snapshots.put(&server, profile, &snapshot_keys::item(id), &detail) {
            log::warn!("details of {id} not saved: {e}");
        }
    });
}

/// Asks the server in use to read `id` ahead, unless it was asked to within the minute.
fn read_ahead(id: i64, cx: &AppCx) -> Option<Promise<Result<(), ApiError>>> {
    let server = cx.state.server()?;
    let mut last = READ_AHEAD.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref().is_some_and(|(s, l, at)| s == server && *l == id && cx.now - at < READ_AHEAD_AGAIN) {
        return None;
    }
    *last = Some((server.to_string(), id, cx.now));
    drop(last);
    Some(cx.spawn(move |api| api.prefetch(id)))
}
