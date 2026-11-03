//! The bodies of the server's refusals (apps/server/src/error.rs).

use serde::{Deserialize, Serialize};

use super::{BusyConnection, nd};

/// Pley's error body, `{ "code": "not_found", "message": "item 42 not found" }`.
/// Subtitle search adds `setup` (false: no OpenSubtitles key on the server).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    #[serde(default, deserialize_with = "nd")]
    pub code: String,
    #[serde(default, deserialize_with = "nd")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<bool>,
}

/// A `410 session_stopped`: a session stopped from elsewhere, with what the
/// one who stopped it wrote.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStoppedBody {
    #[serde(default, deserialize_with = "nd")]
    pub code: String,
    pub message: Option<String>,
}

/// A Live TV `409`: every connection of the channel's source is in use.
/// Asking again with `take_over` set to a connection that `can_take_over`
/// takes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveBusy {
    #[serde(default, deserialize_with = "nd")]
    pub code: String,
    /// "IPTV One allows one connection at once, and it is in use: watching BBC One."
    #[serde(default, deserialize_with = "nd")]
    pub message: String,
    #[serde(default, deserialize_with = "nd")]
    pub connections: Vec<BusyConnection>,
}
