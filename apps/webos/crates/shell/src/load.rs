//! "Saved first, fresh after" for one answer a screen shows (the Android app's `ResourceCache`
//! and `Loadable`): what was saved for the server and
//! profile shows at once (read off the main thread), the server is asked, and its answer replaces
//! it and is saved in turn. A failure keeps what showed; an unreachable server raises the
//! offline state, and the first fresh answer after such a failure lowers it.

use std::sync::Arc;

use pley_core::{Api, ApiError, Promise};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::cx::AppCx;
use crate::state::REVALIDATE;

/// What a screen can show of an answer.
#[derive(Debug, PartialEq)]
pub enum Load<'a, T> {
    /// Nothing yet, neither saved nor answered: blank, then a skeleton after 300 ms.
    Loading,
    /// The answer; `fresh` once the server gave it in this launch (else it was saved).
    Ready { value: &'a T, fresh: bool },
    /// The last ask failed; `stale` is what showed before it, still shown when there is one.
    Failed { error: &'a ApiError, stale: Option<&'a T> },
}

type FetchFn<T> = Arc<dyn Fn(&Api) -> Result<T, ApiError> + Send + Sync>;

/// One answer of the server as a screen shows it. Call [`Resource::update`] each frame (it
/// starts what is needed and collects what came back), [`Resource::invalidate`] on
/// [`crate::AppEvent::Changed`] and Retry, and [`Resource::revalidate`] on
/// [`crate::AppEvent::Shown`] (the server is asked again when the answer is older than 30 s).
pub struct Resource<T> {
    key: Option<String>,
    fetch: FetchFn<T>,
    value: Option<T>,
    fresh: bool,
    error: Option<ApiError>,
    asked_at: Option<f64>,
    stale: bool,
    failed_before: bool,
    generation: Option<u64>,
    snapshot_done: bool,
    snapshot: Option<Promise<Option<T>>>,
    request: Option<Promise<Result<T, ApiError>>>,
}

impl<T> Resource<T>
where
    T: Serialize + DeserializeOwned + Send + 'static,
{
    /// An answer fetched with `fetch`, saved under the snapshot `key`
    /// (`pley_core::store::snapshot_keys`), or never saved without one.
    pub fn new(
        key: Option<String>,
        fetch: impl Fn(&Api) -> Result<T, ApiError> + Send + Sync + 'static,
    ) -> Resource<T> {
        Resource {
            key,
            fetch: Arc::new(fetch),
            value: None,
            fresh: false,
            error: None,
            asked_at: None,
            stale: false,
            failed_before: false,
            generation: None,
            snapshot_done: false,
            snapshot: None,
            request: None,
        }
    }

    /// Starts what is needed (the saved answer, the server's) and collects what came back.
    /// Whether what shows changed. Another server, sign-in or profile since the last call starts
    /// it over.
    pub fn update(&mut self, cx: &mut AppCx) -> bool {
        let generation = cx.state.generation();
        let mut changed = false;
        if self.generation != Some(generation) {
            changed = self.value.is_some() || self.error.is_some();
            self.forget();
            self.generation = Some(generation);
        }
        self.read_saved(cx);
        changed |= self.collect_saved();
        if self.request.is_none() && (self.asked_at.is_none() || self.stale) && cx.state.signed_in() {
            let fetch = self.fetch.clone();
            self.request = Some(cx.spawn(move |api| fetch(api)));
            self.asked_at = Some(cx.now);
            self.stale = false;
        }
        changed | self.collect_answer(cx)
    }

    fn forget(&mut self) {
        self.value = None;
        self.fresh = false;
        self.error = None;
        self.asked_at = None;
        self.stale = false;
        self.failed_before = false;
        self.snapshot_done = false;
        self.snapshot = None;
        self.request = None;
    }

    fn read_saved(&mut self, cx: &mut AppCx) {
        if self.snapshot_done || self.snapshot.is_some() {
            return;
        }
        let (Some(key), Some(server), Some(profile)) = (self.key.clone(), cx.state.server(), cx.state.profile()) else {
            self.snapshot_done = true;
            return;
        };
        let (server, snapshots) = (server.to_string(), cx.snapshots().clone());
        self.snapshot = Some(cx.tasks().spawn(move || snapshots.get::<T>(&server, profile, &key)));
    }

    fn collect_saved(&mut self) -> bool {
        let Some(p) = self.snapshot.as_mut() else { return false };
        if !p.ready() && !p.failed() {
            return false;
        }
        let saved = p.take().flatten();
        self.snapshot = None;
        self.snapshot_done = true;
        // The server may have answered first: its answer wins.
        if self.value.is_none()
            && let Some(v) = saved
        {
            self.value = Some(v);
            return true;
        }
        false
    }

    fn collect_answer(&mut self, cx: &mut AppCx) -> bool {
        let Some(p) = self.request.as_mut() else { return false };
        let answer = match p.take() {
            Some(a) => a,
            None if p.failed() => Err(ApiError::Unreadable("the request stopped".into())),
            None => return false,
        };
        self.request = None;
        match answer {
            Ok(value) => {
                if std::mem::take(&mut self.failed_before) {
                    cx.state.server_answered();
                }
                self.save(cx, &value);
                self.value = Some(value);
                self.fresh = true;
                self.error = None;
            }
            Err(e) if e.cancelled() => return false,
            Err(e) => {
                cx.state.report_failure(&e);
                self.failed_before = e.unreachable();
                self.error = Some(e);
            }
        }
        true
    }

    fn save(&self, cx: &mut AppCx, value: &T) {
        let (Some(key), Some(server), Some(profile)) = (self.key.clone(), cx.state.server(), cx.state.profile()) else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(value) else { return };
        let (server, snapshots) = (server.to_string(), cx.snapshots().clone());
        cx.tasks().detach(move || {
            if let Err(e) = snapshots.write(&server, profile, &key, &bytes) {
                log::warn!("snapshot {key} not saved: {e}");
            }
        });
    }

    /// Asks the server again at the next [`Resource::update`] (an event said it changed, Retry).
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// Asks again at the next update if the answer is older than 30 s (the screen shows again).
    pub fn revalidate(&mut self, now: f64) {
        if self.asked_at.is_some_and(|t| now - t > REVALIDATE) {
            self.stale = true;
        }
    }

    /// What shows.
    pub fn state(&self) -> Load<'_, T> {
        match (&self.error, &self.value) {
            (Some(error), stale) => Load::Failed { error, stale: stale.as_ref() },
            (None, Some(value)) => Load::Ready { value, fresh: self.fresh },
            (None, None) => Load::Loading,
        }
    }

    /// The answer shown, saved or fresh, whatever the last ask did.
    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// The answer, to change in place (a mark the person just set, shown before the server's next answer).
    pub fn value_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut()
    }

    /// Whether the server gave the answer in this launch.
    pub fn fresh(&self) -> bool {
        self.fresh
    }

    /// Why the last ask failed.
    pub fn error(&self) -> Option<&ApiError> {
        self.error.as_ref()
    }

    /// Whether the server is being asked.
    pub fn asking(&self) -> bool {
        self.request.is_some()
    }
}
