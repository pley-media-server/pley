use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use pley_transcoder_api::JobStatus;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use super::remote::{self, SessionKind};
use crate::access;
use crate::auth::scopes::Scope;
use crate::auth::{AuthUser, CurrentToken};
use crate::catalog::{self, ItemSummary};
use crate::error::{ApiResult, AppError};
use crate::library::scanner::ScanStatus;
use crate::markers::AnalysisStatus;
use crate::playback::{DeviceKind, Method, TranscodeInfo};
use crate::schedule::LibraryWorkStatus;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SessionInfo {
    session_id: String,
    kind: SessionKind,
    user_id: i64,
    username: String,
    /// The device it plays on (see [`crate::playback::Session::device`]),
    /// which device that is and what kind.
    device: Option<String>,
    device_id: String,
    device_kind: crate::playback::DeviceKind,
    item: ItemSummary,
    method: Method,
    position_ms: i64,
    state: String,
    started_at: i64,
    updated_at: i64,
    transcode: Option<TranscodeInfo>,
}

#[derive(Serialize)]
pub struct Activity {
    scans: Vec<ScanStatus>,
    sessions: Vec<SessionInfo>,
    /// Intro/credits detection in progress.
    analysis: Option<AnalysisStatus>,
    /// Seasons waiting for intro/credits detection (administrators only).
    seasons_pending: usize,
    /// Files waiting for seek previews, those being made included
    /// (administrators only).
    previews_pending: usize,
    /// Files whose seek previews are being made now (administrators only).
    previews_running: usize,
    /// When what waits is done, while some does (administrators only).
    library_work: Option<LibraryWorkStatus>,
}

/// `GET /api/activity`: what plays and what the server is busy with. Who
/// oversees playback ([`AuthUser::oversees_playback`]) sees everyone's
/// sessions, anyone else their own. The library work's counters go by
/// administrator rights, which an app's token never has.
pub async fn activity(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Activity>> {
    let everyone = user.oversees_playback();
    let sessions: Vec<_> = state.sessions.active().into_iter().filter(|s| everyone || s.user_id == user.id).collect();
    let jobs: HashMap<String, JobStatus> = if sessions.iter().any(|s| s.job_id().is_some()) {
        match state.transcoder.status().await {
            Ok(status) => status.jobs.into_iter().map(|j| (j.id.clone(), j)).collect(),
            Err(_) => HashMap::new(),
        }
    } else {
        HashMap::new()
    };
    let mut out = Vec::new();
    for session in sessions {
        let Some(row) = catalog::item(&state.db, session.user_id, session.item_id).await? else { continue };
        let play = session.play.lock().clone();
        out.push(SessionInfo {
            session_id: session.id.clone(),
            kind: SessionKind::Item,
            user_id: session.user_id,
            username: session.username.clone(),
            device: session.device.clone(),
            device_id: session.device_id.clone(),
            device_kind: session.device_kind,
            item: row.to_summary(),
            method: session.method,
            position_ms: play.position_ms,
            state: play.state,
            started_at: session.started_at,
            updated_at: play.updated_at,
            transcode: session
                .job_id()
                .and_then(|id| jobs.get(&id))
                .map(|job| super::playback::transcode_seen_by(&user, job)),
        });
    }
    let analysis = if user.is_admin { state.markers.status() } else { None };
    let seasons_pending = if user.is_admin { state.markers.pending() } else { 0 };
    let previews_pending = if user.is_admin { state.trickplay.pending() } else { 0 };
    let previews_running = if user.is_admin { state.trickplay.running() } else { 0 };
    let library_work = (seasons_pending + previews_pending > 0).then(|| state.schedule.status());
    // Scans of libraries the user can't open stay private (their names too).
    let libraries = access::library_ids(&state.db, &user).await?;
    let scans = state
        .scanner
        .statuses()
        .into_iter()
        .filter(|s| libraries.as_ref().is_none_or(|ids| ids.contains(&s.library_id)))
        .collect();
    Ok(Json(Activity {
        scans,
        sessions: out,
        analysis,
        seasons_pending,
        previews_pending,
        previews_running,
        library_work,
    }))
}

/// Event streams one session (or app token) may keep open: a few tabs and
/// devices' worth, not a connection and a task each without end.
const MAX_STREAMS_EACH: usize = 16;

/// Event streams open, by the hash of the token that opened them.
static STREAMS: LazyLock<dashmap::DashMap<String, usize>> = LazyLock::new(dashmap::DashMap::new);

/// One of a token's event streams, counted while it lasts.
struct StreamSlot(String);

impl StreamSlot {
    fn take(token: Option<&str>) -> ApiResult<Self> {
        let key = crate::auth::hash_token(token.unwrap_or_default());
        let mut open = STREAMS.entry(key.clone()).or_insert(0);
        if *open >= MAX_STREAMS_EACH {
            return Err(AppError::TooManyRequests("too many open event streams: close a tab".into()));
        }
        *open += 1;
        Ok(Self(key))
    }
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        STREAMS.remove_if_mut(&self.0, |_, open| {
            *open = open.saturating_sub(1);
            *open == 0
        });
    }
}

/// What an event stream says of its device, to take casts from the user's
/// remotes as a receiver ([`remote::Receivers`]).
#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    /// The app's own id for the device ([`remote::receiver_id`]).
    receiver: Option<String>,
    device_name: Option<String>,
    device_kind: Option<String>,
    /// What it plays when cast to, comma-separated ([`remote::cast_kinds`]).
    casts: Option<String>,
}

pub async fn events(
    State(state): State<AppState>,
    user: AuthUser,
    CurrentToken(token): CurrentToken,
    headers: HeaderMap,
    query: Result<Query<EventsQuery>, QueryRejection>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let slot = StreamSlot::take(token.as_deref())?;
    // Events about libraries the user can't open aren't theirs to see.
    let libraries = Arc::new(parking_lot::RwLock::new(access::library_ids(&state.db, &user).await?));
    // Requests' events go to an app's stream only with `requests`, and
    // those for managers to managers' streams, while the session is one.
    let asks = user.allowed(Scope::Requests);
    let manager = Arc::new(AtomicBool::new(user.manages_requests()));
    // Everyone's sessions' changes go to the streams of those who oversee
    // playback (administrators, whose dashboard lists them, and pley's own
    // apps allowed `activity` on their accounts), while they do.
    let admin = Arc::new(AtomicBool::new(user.oversees_playback()));
    // A stream that names its device as a receiver makes it one the user's
    // remotes cast to, until the stream ends. A query that doesn't parse
    // costs the stream nothing: it just isn't a receiver.
    let query = query.map(|Query(query)| query).unwrap_or_default();
    let receiver = query.receiver.as_deref().and_then(remote::receiver_id).and_then(|id| {
        let declared = query.device_kind.as_deref().and_then(DeviceKind::from_name);
        let receiver = remote::Receiver {
            device_id: remote::device_id_of(&user, token.as_deref()).unwrap_or_else(|| format!("receiver-{id}")),
            name: remote::device_name(query.device_name.as_deref(), &headers),
            given_name: query.device_name.as_deref().and_then(crate::oauth::clients::clean_name),
            kind: remote::device_kind_of(declared, &user, remote::user_agent(&headers)),
            casts: query.casts.as_deref().map(remote::cast_kinds).unwrap_or_default(),
            switches: query.casts.as_deref().is_some_and(remote::casts_switch),
        };
        state.receivers.open(&state.events, user.id, id, receiver)
    });
    // The receiver this stream is, for the casts only it gets.
    let receiver_id = receiver.as_ref().and(query.receiver.as_deref()).map(str::to_owned);
    // The session or app token is checked again every so often
    // (`Config::event_recheck`): the stream ends once it is signed out, it
    // expires or its user is disabled, and follows changes of access.
    let ended = tokio_util::sync::CancellationToken::new();
    {
        let (state, libraries, manager, admin, ended) =
            (state.clone(), libraries.clone(), manager.clone(), admin.clone(), ended.clone());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(state.config.event_recheck);
            tick.tick().await;
            loop {
                tokio::select! {
                    _ = tick.tick() => {}
                    () = ended.cancelled() => break,
                }
                let Some(user) = (match token.as_deref() {
                    Some(token) => state.auth.validate_any(token).await,
                    None => None,
                }) else {
                    ended.cancel();
                    break;
                };
                if let Ok(ids) = access::library_ids(&state.db, &user).await {
                    *libraries.write() = ids;
                }
                // An account made an administrator, or no longer one.
                manager.store(user.manages_requests(), Ordering::Relaxed);
                admin.store(user.oversees_playback(), Ordering::Relaxed);
            }
        });
    }
    // Dropped with the stream (the page went away): the check above stops.
    let gone = ended.clone().drop_guard();
    let me = user.id;
    let key = slot.0.clone();
    let stream = BroadcastStream::new(state.events.subscribe())
        .filter_map(|event| event.ok())
        // Events for one user (their My List, their stopped session) go to
        // that user's streams only, and their sessions' changes to
        // administrators' too; requests' to those they name.
        .filter(move |event| match event.audience() {
            Some(to) => asks && to.reaches(me, manager.load(Ordering::Relaxed), &key),
            None => {
                event.user_id().is_none_or(|id| id == me)
                    || (event.to_administrators() && admin.load(Ordering::Relaxed))
            }
        })
        // A cast from another profile goes to the receiver it names alone.
        .filter(move |event| event.receiver_only().is_none_or(|id| receiver_id.as_deref() == Some(id)))
        .filter(move |event| match (event.library_id(), &*libraries.read()) {
            (Some(id), Some(ids)) => ids.contains(&id),
            _ => true,
        })
        .map(move |event| {
            let _ = (&gone, &slot, &receiver);
            Ok(Event::default().json_data(event).unwrap_or_else(|_| Event::default().comment("encode error")))
        });
    // The stream never ends on its own and the web UI keeps one open while
    // someone is signed in: end it when the server stops, or the graceful
    // shutdown would wait for it.
    let stream = futures::StreamExt::take_until(stream, state.stopping.clone().cancelled_owned());
    let stream = futures::StreamExt::take_until(stream, ended.cancelled_owned());
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
