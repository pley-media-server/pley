//! `/:/websockets/notifications`: Plex's stream of server events. Tautulli
//! follows playback through it, and tools listen to it for new items and
//! scans. Each message is a JSON `NotificationContainer` of one type:
//!
//! - `playing`: a session started, paused, resumed, moved on or stopped
//!   (`PlaySessionStateNotification`, with the `sessionKey` that
//!   `/status/sessions` lists), and every 10 s while its position changes;
//! - `timeline`: items added to a library, first `created`, then done
//!   (`TimelineEntry` states 0 and 5), which is what Tautulli's recently
//!   added notifications wait for;
//! - `activity`: library scans starting, progressing and ending;
//! - `status`: a library scan finished (`LIBRARY_UPDATE`).
//!
//! A user hears about their own sessions (administrators about everyone's)
//! and the libraries they may open. The token is checked again every 10 s:
//! the socket closes once it is revoked, and changed rights apply.

use std::collections::HashMap;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::response::Response;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::MissedTickBehavior;

use super::{PlexError, PlexResult, PlexUser, meta, playback};
use crate::auth::AuthUser;
use crate::catalog;
use crate::events::{ScanPhase, ServerEvent};
use crate::state::AppState;
use crate::util::now;

/// How often positions of playing sessions are reported, and the token
/// checked again.
const TICK: Duration = Duration::from_secs(10);
/// New items reported for one library change; the rest come with the next.
const BATCH: i64 = 500;

/// The token the client connected with, to check it again later.
pub struct Token(String);

impl<S: Send + Sync> FromRequestParts<S> for Token {
    type Rejection = PlexError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        super::request_token(parts).map(Token).ok_or(PlexError::Unauthorized)
    }
}

pub async fn socket(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Token(token): Token,
    upgrade: WebSocketUpgrade,
) -> PlexResult<Response> {
    // Listening, and what is there already known, before the client hears
    // back: nothing that happens once it is connected goes unreported.
    let events = state.events.subscribe();
    let mut listener = Listener::new(state, user, token);
    listener.start().await.map_err(PlexError::Internal)?;
    Ok(upgrade.on_upgrade(move |socket| listener.run(socket, events)))
}

/// A session as last reported: its key and client stay the same to the end.
#[derive(Clone)]
struct Reported {
    key: i64,
    client: String,
    item_id: i64,
    state: &'static str,
    position_ms: i64,
}

/// One client's view of the server.
struct Listener {
    state: AppState,
    user: AuthUser,
    token: String,
    /// Names of the libraries the user may open.
    libraries: HashMap<i64, String>,
    /// What was last reported, by session.
    sessions: HashMap<String, Reported>,
    /// The newest item reported (or there at the start), by library.
    newest: HashMap<i64, i64>,
    /// Activity ids of the scans running, by library.
    scans: HashMap<i64, String>,
}

impl Listener {
    fn new(state: AppState, user: AuthUser, token: String) -> Self {
        Self {
            state,
            user,
            token,
            libraries: HashMap::new(),
            sessions: HashMap::new(),
            newest: HashMap::new(),
            scans: HashMap::new(),
        }
    }

    /// What is there already isn't news: sessions as they are, items up to now.
    async fn start(&mut self) -> anyhow::Result<()> {
        self.load_libraries().await?;
        let newest: Vec<(i64, i64)> = sqlx::query_as("SELECT library_id, MAX(id) FROM items GROUP BY library_id")
            .fetch_all(&self.state.db.read)
            .await?;
        self.newest = newest.into_iter().collect();
        let _ = self.sessions_changed();
        Ok(())
    }

    async fn load_libraries(&mut self) -> anyhow::Result<()> {
        let visible = crate::access::visible_libraries(&self.state.db, &self.user).await?;
        self.libraries = visible.into_iter().map(|l| (l.id, l.name)).collect();
        Ok(())
    }

    async fn run(mut self, mut socket: WebSocket, mut events: broadcast::Receiver<ServerEvent>) {
        let stopping = self.state.stopping.clone();
        let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + TICK, TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let notes = tokio::select! {
                () = stopping.cancelled() => break,
                message = socket.recv() => match message {
                    // Plex reads nothing from clients; pings are answered for us.
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => continue,
                },
                event = events.recv() => match event {
                    Ok(event) => self.on_event(event).await,
                    Err(RecvError::Lagged(_)) => self.caught_up().await,
                    Err(RecvError::Closed) => break,
                },
                _ = tick.tick() => {
                    let Some(user) = self.state.auth.validate(&self.token).await else { break };
                    let rights = (user.is_admin, user.all_libraries, user.max_age);
                    if rights != (self.user.is_admin, self.user.all_libraries, self.user.max_age) {
                        self.user = user;
                        if let Err(err) = self.load_libraries().await {
                            tracing::debug!("notification socket: {err}");
                        }
                    }
                    self.sessions_changed()
                }
            };
            for note in notes {
                if socket.send(Message::Text(note.to_string().into())).await.is_err() {
                    return;
                }
            }
        }
        let _ = socket.send(Message::Close(None)).await;
    }

    /// After missed events (a busy scan): sessions and every library looked
    /// at again.
    async fn caught_up(&mut self) -> Vec<Value> {
        let mut notes = self.sessions_changed();
        let libraries: Vec<i64> = self.libraries.keys().copied().collect();
        for library_id in libraries {
            match self.items_added(library_id).await {
                Ok(added) => notes.extend(added),
                Err(err) => tracing::debug!("notification socket: {err}"),
            }
        }
        notes
    }

    async fn on_event(&mut self, event: ServerEvent) -> Vec<Value> {
        let result = match event {
            ServerEvent::SessionsChanged => Ok(self.sessions_changed()),
            ServerEvent::LibrariesChanged => self.load_libraries().await.map(|()| Vec::new()),
            ServerEvent::LibraryChanged { library_id } => self.items_added(library_id).await,
            ServerEvent::ScanStarted { library_id } => {
                Ok(self.scan(library_id, "started", 0, "").into_iter().collect())
            }
            ServerEvent::ScanProgress { library_id, phase, processed, total } => {
                let progress = (processed.min(total) * 100).checked_div(total).unwrap_or(0) as i64;
                Ok(self.scan(library_id, "updated", progress, phase_name(phase)).into_iter().collect())
            }
            ServerEvent::ScanFinished { library_id, .. } => {
                let mut notes: Vec<Value> = self.scan(library_id, "ended", 100, "").into_iter().collect();
                self.scans.remove(&library_id);
                if self.libraries.contains_key(&library_id) {
                    notes.push(container(
                        "status",
                        "StatusNotification",
                        vec![json!({
                            "title": "Library scan complete",
                            "description": "",
                            "notificationName": "LIBRARY_UPDATE",
                        })],
                    ));
                }
                // Items a scan added that no change event reported yet.
                self.items_added(library_id).await.map(|added| notes.into_iter().chain(added).collect())
            }
            // pley's own: Plex clients hear of a stopped session as of any
            // that ends (`sessions_changed` follows).
            ServerEvent::WatchlistChanged { .. }
            | ServerEvent::OwnLibrariesChanged { .. }
            | ServerEvent::UserStateChanged { .. }
            | ServerEvent::SessionStopped { .. }
            | ServerEvent::LiveChannelsChanged
            | ServerEvent::LiveTunersChanged
            | ServerEvent::RequestsChanged { .. }
            | ServerEvent::RequestAvailable { .. }
            | ServerEvent::RequestDecided { .. }
            | ServerEvent::RequestCreated { .. }
            | ServerEvent::RequestFailed { .. }
            | ServerEvent::RequestsSettingsChanged { .. } => Ok(Vec::new()),
        };
        result.unwrap_or_else(|err| {
            tracing::debug!("notification socket: {err}");
            Vec::new()
        })
    }

    /// `playing` notifications for sessions that started, changed state or
    /// moved on since the last look, and `stopped` for those that ended.
    fn sessions_changed(&mut self) -> Vec<Value> {
        let live = self.state.sessions.active();
        let mine: Vec<_> = live.iter().filter(|s| self.user.is_admin || s.user_id == self.user.id).collect();
        let mut notes = Vec::new();
        for session in &mine {
            let play = session.play.lock().clone();
            let state = match play.state.as_str() {
                "paused" => "paused",
                "stopped" => "stopped",
                "buffering" => "buffering",
                _ => "playing",
            };
            let last = self.sessions.get(&session.id).cloned();
            if last.as_ref().is_some_and(|l| (l.state, l.position_ms) == (state, play.position_ms))
                || (last.is_none() && state == "stopped")
            {
                continue;
            }
            let reported = match last {
                Some(last) => Reported { state, position_ms: play.position_ms, ..last },
                None => {
                    let (key, client) = playback::session_key(&self.state, &session.id);
                    let client =
                        client.identifier.or_else(|| session.client_id.clone()).unwrap_or_else(|| session.id.clone());
                    Reported { key, client, item_id: session.item_id, state, position_ms: play.position_ms }
                }
            };
            notes.push(playing(&reported));
            self.sessions.insert(session.id.clone(), reported);
        }
        // Sessions gone without a word: stopped.
        let gone: Vec<String> = self.sessions.keys().filter(|id| !mine.iter().any(|s| &&s.id == id)).cloned().collect();
        for id in gone {
            if let Some(last) = self.sessions.remove(&id)
                && last.state != "stopped"
            {
                notes.push(playing(&Reported { state: "stopped", ..last }));
            }
        }
        // Keys stay for quiet sessions, which may wake.
        playback::forget_ended(&self.state, &self.state.sessions.list());
        notes
    }

    /// `timeline` notifications for the items added to a library since the
    /// newest one reported: `created`, then done.
    async fn items_added(&mut self, library_id: i64) -> anyhow::Result<Vec<Value>> {
        if !self.libraries.contains_key(&library_id) {
            return Ok(Vec::new());
        }
        let since = self.newest.get(&library_id).copied().unwrap_or(0);
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM items WHERE library_id = ? AND id > ? ORDER BY id LIMIT ?")
                .bind(library_id)
                .bind(since)
                .bind(BATCH)
                .fetch_all(&self.state.db.read)
                .await?;
        let Some(&last) = ids.last() else { return Ok(Vec::new()) };
        self.newest.insert(library_id, last);
        // What the user may see of them (age limits).
        let rows = catalog::items_by_ids(&self.state.db, self.user.id, &ids).await?;
        let updated_at = now();
        let entry = |row: &catalog::ItemRow, state: i64| {
            let mut entry = json!({
                "identifier": "com.plexapp.plugins.library",
                "sectionID": library_id.to_string(),
                "itemID": row.id.to_string(),
                "type": meta::type_number(&row.kind),
                "title": row.title,
                "state": state,
                "updatedAt": updated_at,
            });
            if state == 0 {
                entry["metadataState"] = json!("created");
            }
            entry
        };
        let rows: Vec<_> = rows.iter().filter(|r| meta::type_number(&r.kind) > 0).collect();
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![
            container("timeline", "TimelineEntry", rows.iter().map(|r| entry(r, 0)).collect()),
            container("timeline", "TimelineEntry", rows.iter().map(|r| entry(r, 5)).collect()),
        ])
    }

    /// An `activity` notification about a library's scan.
    fn scan(&mut self, library_id: i64, event: &str, progress: i64, subtitle: &str) -> Option<Value> {
        let name = self.libraries.get(&library_id)?;
        let uuid = self.scans.entry(library_id).or_insert_with(|| uuid::Uuid::new_v4().to_string()).clone();
        Some(container(
            "activity",
            "ActivityNotification",
            vec![json!({
                "event": event,
                "uuid": uuid,
                "Activity": {
                    "uuid": uuid,
                    "type": "library.update.section",
                    "cancellable": false,
                    "userID": 1,
                    "title": format!("Scanning {name}"),
                    "subtitle": subtitle,
                    "progress": progress,
                    "Context": { "librarySectionID": library_id.to_string() },
                },
            })],
        ))
    }
}

fn phase_name(phase: ScanPhase) -> &'static str {
    match phase {
        ScanPhase::Queued => "Waiting",
        ScanPhase::Walking => "Looking for files",
        ScanPhase::Probing => "Reading media",
        ScanPhase::Metadata => "Fetching metadata",
        ScanPhase::Done => "Done",
    }
}

/// A `playing` notification about a session.
fn playing(session: &Reported) -> Value {
    container(
        "playing",
        "PlaySessionStateNotification",
        vec![json!({
            "sessionKey": session.key.to_string(),
            "clientIdentifier": session.client,
            "guid": "",
            "ratingKey": session.item_id.to_string(),
            "url": "",
            "key": format!("/library/metadata/{}", session.item_id),
            "viewOffset": session.position_ms,
            "playQueueItemID": 0,
            "state": session.state,
        })],
    )
}

fn container(kind: &str, element: &str, entries: Vec<Value>) -> Value {
    let mut inner = json!({ "type": kind, "size": entries.len() });
    inner[element] = Value::Array(entries);
    json!({ "NotificationContainer": inner })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containers_are_shaped_like_plex() {
        let note = container("status", "StatusNotification", vec![json!({"title": "x"})]);
        assert_eq!(
            note,
            json!({"NotificationContainer": {"type": "status", "size": 1, "StatusNotification": [{"title": "x"}]}})
        );
    }
}
