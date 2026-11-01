//! The server's events (`GET /api/events`, apps/server/src/events.rs): one
//! JSON object with a `type` per `data:` line of the stream.

use serde::{Deserialize, Deserializer};

use super::{nd, wire_enum};

wire_enum! {
    /// Where a library scan is.
    pub enum ScanPhase or Unknown {
        Queued = "queued",
        Walking = "walking",
        Probing = "probing",
        Metadata = "metadata",
        Done = "done",
        #[default]
        Unknown = "unknown",
    }
}

/// An event of the stream. A `type` this app doesn't know reads as
/// [`ServerEvent::Other`], never as an error.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    ScanStarted {
        library_id: i64,
    },
    ScanProgress {
        library_id: i64,
        phase: ScanPhase,
        processed: u64,
        total: u64,
    },
    ScanFinished {
        library_id: i64,
        added: u64,
        updated: u64,
        removed: u64,
    },
    /// A library's items changed (a scan, markers found): ask again for its views.
    LibraryChanged {
        library_id: i64,
    },
    /// The libraries, their order or the profile's access changed.
    LibrariesChanged,
    SessionsChanged,
    WatchlistChanged,
    /// Watch state changed, here or elsewhere: the item, then its parent and
    /// grandparent; `children`: everything under the first one too.
    UserStateChanged {
        item_ids: Vec<i64>,
        children: bool,
    },
    /// A session of this profile was stopped from elsewhere.
    SessionStopped {
        session_id: String,
        message: Option<String>,
        by_admin: bool,
    },
    /// Live TV's sources, channels or guide changed.
    LiveChannelsChanged,
    LiveTunersChanged,
    /// A type this app doesn't know, by its name.
    Other(String),
}

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "type", default, deserialize_with = "nd")]
    kind: String,
    #[serde(default, deserialize_with = "nd")]
    library_id: i64,
    #[serde(default, deserialize_with = "nd")]
    phase: ScanPhase,
    #[serde(default, deserialize_with = "nd")]
    processed: u64,
    #[serde(default, deserialize_with = "nd")]
    total: u64,
    #[serde(default, deserialize_with = "nd")]
    added: u64,
    #[serde(default, deserialize_with = "nd")]
    updated: u64,
    #[serde(default, deserialize_with = "nd")]
    removed: u64,
    #[serde(default, deserialize_with = "nd")]
    item_ids: Vec<i64>,
    #[serde(default, deserialize_with = "nd")]
    children: bool,
    #[serde(default, deserialize_with = "nd")]
    session_id: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    by_admin: bool,
}

impl<'de> Deserialize<'de> for ServerEvent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ServerEvent, D::Error> {
        let r = Raw::deserialize(d)?;
        Ok(match r.kind.as_str() {
            "scan_started" => ServerEvent::ScanStarted { library_id: r.library_id },
            "scan_progress" => ServerEvent::ScanProgress {
                library_id: r.library_id,
                phase: r.phase,
                processed: r.processed,
                total: r.total,
            },
            "scan_finished" => ServerEvent::ScanFinished {
                library_id: r.library_id,
                added: r.added,
                updated: r.updated,
                removed: r.removed,
            },
            "library_changed" => ServerEvent::LibraryChanged { library_id: r.library_id },
            "libraries_changed" => ServerEvent::LibrariesChanged,
            "sessions_changed" => ServerEvent::SessionsChanged,
            "watchlist_changed" => ServerEvent::WatchlistChanged,
            "user_state_changed" => ServerEvent::UserStateChanged { item_ids: r.item_ids, children: r.children },
            "session_stopped" => {
                ServerEvent::SessionStopped { session_id: r.session_id, message: r.message, by_admin: r.by_admin }
            }
            "live_channels_changed" => ServerEvent::LiveChannelsChanged,
            "live_tuners_changed" => ServerEvent::LiveTunersChanged,
            _ => ServerEvent::Other(r.kind),
        })
    }
}

impl ServerEvent {
    /// The event's `type` on the wire.
    pub fn kind(&self) -> &str {
        match self {
            ServerEvent::ScanStarted { .. } => "scan_started",
            ServerEvent::ScanProgress { .. } => "scan_progress",
            ServerEvent::ScanFinished { .. } => "scan_finished",
            ServerEvent::LibraryChanged { .. } => "library_changed",
            ServerEvent::LibrariesChanged => "libraries_changed",
            ServerEvent::SessionsChanged => "sessions_changed",
            ServerEvent::WatchlistChanged => "watchlist_changed",
            ServerEvent::UserStateChanged { .. } => "user_state_changed",
            ServerEvent::SessionStopped { .. } => "session_stopped",
            ServerEvent::LiveChannelsChanged => "live_channels_changed",
            ServerEvent::LiveTunersChanged => "live_tuners_changed",
            ServerEvent::Other(kind) => kind,
        }
    }
}
