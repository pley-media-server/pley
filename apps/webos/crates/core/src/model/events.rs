//! The server's events (`GET /api/events`, apps/server/src/events.rs): one
//! JSON object with a `type` per `data:` line of the stream.

use serde::{Deserialize, Deserializer};

use super::{Control, RequestUser, TitleKind, nd, wire_enum};

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
    /// The user's remote, on another of their devices, for the player of `session_id`
    /// (docs/API.md, "Remote control"): `position_ms` for a seek, `offset_ms` for a skip,
    /// `stream_index` for another audio track or other subtitles (-1: none).
    PlayerCommand {
        session_id: String,
        command: Control,
        position_ms: Option<i64>,
        offset_ms: Option<i64>,
        stream_index: Option<i64>,
    },
    /// Live TV's sources, channels or guide changed.
    LiveChannelsChanged,
    LiveTunersChanged,
    /// Someone took over the connection of live channel `channel_id`: every viewer of it on this
    /// profile was ended, each with one of these, and `message` says why (docs/API.md, "Live TV").
    LiveTakenOver {
        session_id: String,
        channel_id: i64,
        message: Option<String>,
    },
    /// Any change of a request (its stage included); all three `None` when many changed (after
    /// Your requests was seen elsewhere).
    RequestsChanged {
        request_id: Option<i64>,
        kind: Option<TitleKind>,
        tmdb_id: Option<i64>,
    },
    /// Ready to watch, or some seasons of a show are (`complete` false, `seasons` those newly
    /// ready); `item_id` as this profile can open it.
    RequestAvailable {
        request_id: i64,
        title: String,
        kind: Option<TitleKind>,
        tmdb_id: Option<i64>,
        item_id: Option<i64>,
        poster_url: Option<String>,
        seasons: Option<Vec<i64>>,
        complete: bool,
    },
    /// Approved or declined (`approved`), with the reason given.
    RequestDecided {
        request_id: i64,
        title: String,
        kind: Option<TitleKind>,
        tmdb_id: Option<i64>,
        approved: bool,
        reason: Option<String>,
    },
    /// To managers: someone asked (`pending`: it waits for one of them).
    RequestCreated {
        request_id: i64,
        title: String,
        kind: Option<TitleKind>,
        tmdb_id: Option<i64>,
        requested_by: Option<RequestUser>,
        pending: bool,
    },
    /// To managers: a request couldn't be sent and the server won't try again by itself.
    RequestFailed {
        request_id: i64,
        title: String,
        kind: Option<TitleKind>,
        reason: Option<String>,
    },
    /// Requests turned on or off, the limits or the TMDB key changed, or this profile's access
    /// did: the summary is asked again.
    RequestsSettingsChanged,
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
    #[serde(default, deserialize_with = "nd")]
    channel_id: i64,
    #[serde(default, deserialize_with = "nd")]
    command: Control,
    #[serde(default)]
    position_ms: Option<i64>,
    #[serde(default)]
    offset_ms: Option<i64>,
    #[serde(default)]
    stream_index: Option<i64>,
    #[serde(default)]
    request_id: Option<i64>,
    #[serde(rename = "kind", default)]
    title_kind: Option<TitleKind>,
    #[serde(default)]
    tmdb_id: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    title: String,
    #[serde(default)]
    item_id: Option<i64>,
    #[serde(default)]
    poster_url: Option<String>,
    #[serde(default)]
    seasons: Option<Vec<i64>>,
    #[serde(default)]
    complete: Option<bool>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    requested_by: Option<RequestUser>,
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
            "player_command" => ServerEvent::PlayerCommand {
                session_id: r.session_id,
                command: r.command,
                position_ms: r.position_ms,
                offset_ms: r.offset_ms,
                stream_index: r.stream_index,
            },
            "live_channels_changed" => ServerEvent::LiveChannelsChanged,
            "live_tuners_changed" => ServerEvent::LiveTunersChanged,
            "live_taken_over" => {
                ServerEvent::LiveTakenOver { session_id: r.session_id, channel_id: r.channel_id, message: r.message }
            }
            "requests_changed" => {
                ServerEvent::RequestsChanged { request_id: r.request_id, kind: r.title_kind, tmdb_id: r.tmdb_id }
            }
            // An older server sends fewer fields: a show's ready is whole unless it says otherwise.
            "request_available" => ServerEvent::RequestAvailable {
                request_id: r.request_id.unwrap_or_default(),
                title: r.title,
                kind: r.title_kind,
                tmdb_id: r.tmdb_id,
                item_id: r.item_id,
                poster_url: r.poster_url,
                seasons: r.seasons,
                complete: r.complete.unwrap_or(true),
            },
            "request_decided" => ServerEvent::RequestDecided {
                request_id: r.request_id.unwrap_or_default(),
                title: r.title,
                kind: r.title_kind,
                tmdb_id: r.tmdb_id,
                approved: r.status.as_deref() == Some("approved"),
                reason: r.reason,
            },
            "request_created" => ServerEvent::RequestCreated {
                request_id: r.request_id.unwrap_or_default(),
                title: r.title,
                kind: r.title_kind,
                tmdb_id: r.tmdb_id,
                requested_by: r.requested_by,
                pending: r.status.as_deref() == Some("pending"),
            },
            "request_failed" => ServerEvent::RequestFailed {
                request_id: r.request_id.unwrap_or_default(),
                title: r.title,
                kind: r.title_kind,
                reason: r.reason,
            },
            "requests_settings_changed" => ServerEvent::RequestsSettingsChanged,
            _ => ServerEvent::Other(r.kind),
        })
    }
}

impl ServerEvent {
    /// Whether it is about requests: pages that show requests or Discover's titles hear it.
    pub fn about_requests(&self) -> bool {
        matches!(
            self,
            ServerEvent::RequestsChanged { .. }
                | ServerEvent::RequestAvailable { .. }
                | ServerEvent::RequestDecided { .. }
                | ServerEvent::RequestCreated { .. }
                | ServerEvent::RequestFailed { .. }
                | ServerEvent::RequestsSettingsChanged
        )
    }

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
            ServerEvent::PlayerCommand { .. } => "player_command",
            ServerEvent::LiveChannelsChanged => "live_channels_changed",
            ServerEvent::LiveTunersChanged => "live_tuners_changed",
            ServerEvent::LiveTakenOver { .. } => "live_taken_over",
            ServerEvent::RequestsChanged { .. } => "requests_changed",
            ServerEvent::RequestAvailable { .. } => "request_available",
            ServerEvent::RequestDecided { .. } => "request_decided",
            ServerEvent::RequestCreated { .. } => "request_created",
            ServerEvent::RequestFailed { .. } => "request_failed",
            ServerEvent::RequestsSettingsChanged => "requests_settings_changed",
            ServerEvent::Other(kind) => kind,
        }
    }
}
