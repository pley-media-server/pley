//! GET /api/events: the server's events as Server-Sent Events, to every open
//! stream. Each stream starts with a comment, gets a keep-alive comment every
//! [`Options::keep_alive`](crate::Options::keep_alive) (the server's 15 s) and
//! ends when its access token does, as the server's. Every
//! [`Options::scan_every`](crate::Options::scan_every) one library is
//! "scanned" (scan_started, its progress, scan_finished, library_changed);
//! writes add their own, and tests send any with
//! [`FixtureServer::emit`](crate::FixtureServer::emit).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::json;

use crate::http::{Body, Response, lock};
use crate::server::{Answer, Cx};

/// An event the server sends (docs/API.md, "Events").
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    ScanStarted {
        library_id: i64,
    },
    /// `phase`: `queued`, `walking`, `probing`, `metadata` or `done`.
    ScanProgress {
        library_id: i64,
        phase: String,
        processed: u64,
        total: u64,
    },
    ScanFinished {
        library_id: i64,
        added: u64,
        updated: u64,
        removed: u64,
    },
    LibraryChanged {
        library_id: i64,
    },
    LibrariesChanged,
    SessionsChanged,
    WatchlistChanged,
    /// The items (with their parent and grandparent) whose state changed;
    /// `children` when every episode or track under them did.
    UserStateChanged {
        item_ids: Vec<i64>,
        children: bool,
    },
    SessionStopped {
        session_id: String,
        message: Option<String>,
        by_admin: bool,
    },
    LiveChannelsChanged,
    LiveTunersChanged,
    /// To each viewer a take-over ended: the channel, and why.
    LiveTakenOver {
        session_id: String,
        channel_id: i64,
        message: String,
    },
}

impl ServerEvent {
    /// The event's `data:` JSON.
    pub fn to_json(&self) -> String {
        let value = match self {
            ServerEvent::ScanStarted { library_id } => json!({ "type": "scan_started", "library_id": library_id }),
            ServerEvent::ScanProgress { library_id, phase, processed, total } => json!({
                "type": "scan_progress", "library_id": library_id, "phase": phase,
                "processed": processed, "total": total,
            }),
            ServerEvent::ScanFinished { library_id, added, updated, removed } => json!({
                "type": "scan_finished", "library_id": library_id,
                "added": added, "updated": updated, "removed": removed,
            }),
            ServerEvent::LibraryChanged { library_id } => {
                json!({ "type": "library_changed", "library_id": library_id })
            }
            ServerEvent::LibrariesChanged => json!({ "type": "libraries_changed" }),
            ServerEvent::SessionsChanged => json!({ "type": "sessions_changed" }),
            ServerEvent::WatchlistChanged => json!({ "type": "watchlist_changed" }),
            ServerEvent::UserStateChanged { item_ids, children } => {
                json!({ "type": "user_state_changed", "item_ids": item_ids, "children": children })
            }
            ServerEvent::SessionStopped { session_id, message, by_admin } => json!({
                "type": "session_stopped", "session_id": session_id, "message": message, "by_admin": by_admin,
            }),
            ServerEvent::LiveChannelsChanged => json!({ "type": "live_channels_changed" }),
            ServerEvent::LiveTunersChanged => json!({ "type": "live_tuners_changed" }),
            ServerEvent::LiveTakenOver { session_id, channel_id, message } => json!({
                "type": "live_taken_over", "session_id": session_id, "channel_id": channel_id, "message": message,
            }),
        };
        value.to_string()
    }
}

enum Message {
    Data(Arc<str>),
    End,
}

struct Subscriber {
    id: u64,
    profile: i64,
    tx: Sender<Message>,
}

/// The open event streams.
#[derive(Default)]
pub(crate) struct EventHub {
    next: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl EventHub {
    fn subscribe(&self, profile: i64) -> (u64, Receiver<Message>) {
        let (tx, rx) = mpsc::channel();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.subscribers).push(Subscriber { id, profile, tx });
        (id, rx)
    }

    fn unsubscribe(&self, id: u64) {
        lock(&self.subscribers).retain(|s| s.id != id);
    }

    /// Sends `event` to every stream.
    pub fn emit(&self, event: &ServerEvent) {
        self.emit_text(event.to_json(), None);
    }

    /// Sends `event` to the streams of `profile` only.
    pub fn emit_to(&self, profile: i64, event: &ServerEvent) {
        self.emit_text(event.to_json(), Some(profile));
    }

    pub fn emit_text(&self, text: String, profile: Option<i64>) {
        let text: Arc<str> = text.into();
        for s in lock(&self.subscribers).iter() {
            if profile.is_none_or(|p| p == s.profile) {
                let _ = s.tx.send(Message::Data(text.clone()));
            }
        }
    }

    /// Ends every stream: the server stops listening.
    pub fn end_all(&self) {
        for s in lock(&self.subscribers).iter() {
            let _ = s.tx.send(Message::End);
        }
    }

    pub fn listeners(&self) -> usize {
        lock(&self.subscribers).len()
    }
}

/// A stream's place among the subscribers, given up when the stream ends, or
/// when it is never sent (a HEAD request).
struct Subscription {
    hub: Arc<EventHub>,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hub.unsubscribe(self.id);
    }
}

/// GET /api/events: the stream for the caller, until its token ends, it goes
/// away or the server stops.
pub(crate) fn stream(cx: &Cx<'_>) -> Answer {
    let caller = cx.caller().clone();
    let hub = cx.shared.events.clone();
    let clock = cx.shared.options.clock.clone();
    let keep_alive = cx.shared.options.keep_alive;
    let (id, rx) = hub.subscribe(caller.profile);
    let subscription = Subscription { hub, id };
    let run = move |out: &mut crate::http::ChunkWriter<'_>| {
        let _subscription = subscription;
        (|| {
            // The headers go out with the first comment.
            out.send(b": connected\n\n")?;
            loop {
                let left = caller.expires_at_ms.map_or(i64::MAX, |end| end - clock.now_ms());
                if left <= 0 {
                    return Ok(());
                }
                let wait = keep_alive.min(Duration::from_millis(left as u64));
                match rx.recv_timeout(wait) {
                    Ok(Message::Data(text)) => out.send(format!("data: {text}\n\n").as_bytes())?,
                    Ok(Message::End) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(RecvTimeoutError::Timeout) => {
                        if caller.expires_at_ms.is_none_or(|end| end > clock.now_ms()) {
                            out.send(b": keep-alive\n\n")?;
                        }
                    }
                }
            }
        })()
    };
    Ok(Response::new(200)
        .with_header("Content-Type", "text/event-stream")
        .with_header("Cache-Control", "no-cache")
        .with_body(Body::Stream(Box::new(run))))
}

/// The pretend scans: one library after another, every so often.
pub(crate) struct Scans {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Scans {
    /// Scans `libraries` (id, item count) in turn, every `every`.
    pub fn start(hub: Arc<EventHub>, libraries: Vec<(i64, i64)>, every: Duration) -> Scans {
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("pley-fixtures scans".into())
            .spawn(move || {
                let mut n = 0;
                while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(every) {
                    if libraries.is_empty() {
                        continue;
                    }
                    let (library_id, count) = libraries[n % libraries.len()];
                    n += 1;
                    let total = count.max(0) as u64;
                    for event in [
                        ServerEvent::ScanStarted { library_id },
                        ServerEvent::ScanProgress { library_id, phase: "walking".into(), processed: 0, total },
                        ServerEvent::ScanProgress { library_id, phase: "probing".into(), processed: total / 2, total },
                        ServerEvent::ScanProgress { library_id, phase: "metadata".into(), processed: total, total },
                        ServerEvent::ScanFinished { library_id, added: 0, updated: 0, removed: 0 },
                        ServerEvent::LibraryChanged { library_id },
                    ] {
                        hub.emit(&event);
                    }
                }
            })
            .ok();
        Scans { stop: Some(stop), thread }
    }

    pub fn stop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Scans {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_have_the_servers_shape() {
        let event = ServerEvent::UserStateChanged { item_ids: vec![20_002, 11_001, 10_001], children: false };
        let value: serde_json::Value = serde_json::from_str(&event.to_json()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "type": "user_state_changed", "item_ids": [20_002, 11_001, 10_001], "children": false })
        );
        let stopped = ServerEvent::SessionStopped { session_id: "ab".into(), message: None, by_admin: true };
        let value: serde_json::Value = serde_json::from_str(&stopped.to_json()).unwrap();
        assert_eq!(value["type"], "session_stopped");
        assert!(value["message"].is_null());
    }
}
