//! The guide's programmes, asked for in six-hour chunks as the guide moves
//! through time, and only for the channels in view, counted from `origin`,
//! where the guide opens (packages/tv/src/live/guide.ts): a lineup of tens of
//! thousands costs the rows on screen.

use std::collections::HashMap;
use std::sync::Arc;

use pley_core::ApiError;
use pley_core::model::{LiveGuide, LiveProgram};
use pley_core::task::{Promise, Tasks};

use crate::live::data::{GUIDE_CHUNK, chunk_starts, merge_guide};
use crate::server::PlayerServer;

/// A channel's chunk asked for this long ago is asked for again when it shows.
const STALE_MS: i64 = 5 * 60_000;
/// One that failed waits this long before it is asked for again.
const RETRY_MS: i64 = 15_000;

#[derive(Clone, Copy)]
struct Asked {
    at: i64,
    ok: bool,
    pending: bool,
}

/// A chunk asked for: its start, the channels, and the answer to come.
type Chunk = (i64, Vec<i64>, Promise<Result<LiveGuide, ApiError>>);

/// The guide of the rows in view.
pub struct GuideStore {
    server: Arc<dyn PlayerServer>,
    tasks: Tasks,
    origin: i64,
    programs: HashMap<i64, Vec<LiveProgram>>,
    /// When each channel's chunk was asked for, by chunk start.
    asked: HashMap<i64, HashMap<i64, Asked>>,
    version: u64,
    jobs: Vec<Chunk>,
}

impl GuideStore {
    /// A guide opening at `origin` (unix seconds).
    pub fn new(server: Arc<dyn PlayerServer>, tasks: Tasks, origin: i64) -> GuideStore {
        GuideStore {
            server,
            tasks,
            origin,
            programs: HashMap::new(),
            asked: HashMap::new(),
            version: 0,
            jobs: Vec::new(),
        }
    }

    /// Where the guide opened.
    pub fn origin(&self) -> i64 {
        self.origin
    }

    /// Channel `id`'s programmes, in order.
    pub fn programs(&self, id: i64) -> Option<&[LiveProgram]> {
        self.programs.get(&id).map(Vec::as_slice)
    }

    /// Changes as chunks come in: rows read their programmes again.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether an answer is still to come.
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// Whether channel `id`'s programmes around `t` have been asked for and came (or failed).
    pub fn answered(&self, id: i64, t: i64) -> bool {
        let Some(start) = chunk_starts(t, t + 1, self.origin).first().copied() else { return false };
        self.asked.get(&start).and_then(|c| c.get(&id)).is_some_and(|a| !a.pending)
    }

    /// Asks for channels `ids`' chunks covering `from`–`to` (unix seconds) that it doesn't have fresh; `now` in ms.
    pub fn want(&mut self, from: i64, to: i64, ids: &[i64], now: i64) {
        if ids.is_empty() {
            return;
        }
        for start in chunk_starts(from, to, self.origin) {
            let chunk = self.asked.entry(start).or_default();
            let missing: Vec<i64> = ids
                .iter()
                .copied()
                .filter(|id| {
                    chunk.get(id).is_none_or(|a| !a.pending && now - a.at >= if a.ok { STALE_MS } else { RETRY_MS })
                })
                .collect();
            if missing.is_empty() {
                continue;
            }
            for id in &missing {
                chunk.insert(*id, Asked { at: now, ok: false, pending: true });
            }
            let server = self.server.clone();
            let ids = missing.clone();
            let job = self.tasks.spawn(move || server.live_guide(start, start + GUIDE_CHUNK, &ids));
            self.jobs.push((start, missing, job));
        }
    }

    /// Takes the chunks that came; true when rows should read their programmes again.
    pub fn poll(&mut self, now: i64) -> bool {
        let before = self.version;
        let mut i = 0;
        while i < self.jobs.len() {
            if !self.jobs[i].2.ready() && !self.jobs[i].2.failed() {
                i += 1;
                continue;
            }
            let (start, ids, mut job) = self.jobs.swap_remove(i);
            let ok = match job.take() {
                Some(Ok(guide)) => {
                    merge_guide(&mut self.programs, &guide);
                    true
                }
                _ => false,
            };
            if let Some(chunk) = self.asked.get_mut(&start) {
                for id in ids {
                    chunk.insert(id, Asked { at: now, ok, pending: false });
                }
            }
            self.version += 1;
        }
        self.version != before
    }

    /// The guide changed on the server: everything is asked for again as it shows.
    pub fn forget(&mut self) {
        self.asked.clear();
        self.programs.clear();
        self.jobs.clear();
        self.version += 1;
    }
}
