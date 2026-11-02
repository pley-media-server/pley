//! What can go wrong with a request, and the words for it (the Android app's
//! `describe()`, the TV app's `unreachable` and `problem`).

use std::fmt;
use std::io;

use crate::model::{ErrorBody, LiveBusy};

use super::Response;

/// No answer: the network failed, it took too long, or it was cancelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetError {
    /// Stopped on purpose (a screen went away): never shown.
    Cancelled,
    /// "no answer in time"
    TimedOut,
    /// Refused or no route: "nothing answers there".
    Refused,
    /// The name doesn't resolve: "that address isn't known on this network".
    UnknownHost,
    /// "its secure connection failed", with the TLS library's reason.
    Tls(String),
    /// An answer bigger than the app takes.
    TooBig,
    /// Anything else: "the connection to the server failed".
    Failed(String),
}

impl NetError {
    /// The error an I/O failure stands for.
    pub fn from_io(e: &io::Error) -> NetError {
        if let Some(inner) = e.get_ref().and_then(|i| i.downcast_ref::<NetError>()) {
            return inner.clone();
        }
        if let Some(tls) = e.get_ref().and_then(|i| i.downcast_ref::<rustls::Error>()) {
            return NetError::Tls(tls.to_string());
        }
        match e.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => NetError::TimedOut,
            io::ErrorKind::ConnectionRefused
            | io::ErrorKind::HostUnreachable
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::AddrNotAvailable => NetError::Refused,
            _ => NetError::Failed(e.to_string()),
        }
    }

    /// Why there was no answer, in the Android app's words, lower case.
    pub fn reason(&self) -> &'static str {
        match self {
            NetError::Cancelled => "cancelled",
            NetError::TimedOut => "no answer in time",
            NetError::Refused => "nothing answers there",
            NetError::UnknownHost => "that address isn't known on this network",
            NetError::Tls(_) => "its secure connection failed",
            NetError::TooBig => "the server's answer is too big",
            NetError::Failed(_) => "the connection to the server failed",
        }
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl std::error::Error for NetError {}

/// Why a call to the server gave no answer the app can use.
#[derive(Clone, Debug, PartialEq)]
pub enum ApiError {
    /// No answer at all.
    Net(NetError),
    /// The server refused: its status, Pley's `code` (else one by status) and
    /// `message`, and the body when it was JSON (fields past those, as a Live
    /// TV `409`'s `connections`).
    Status { status: u16, code: String, message: String, body: Option<serde_json::Value> },
    /// A session stopped from elsewhere (`410 session_stopped`), with what the
    /// one who stopped it wrote.
    SessionStopped { message: Option<String> },
    /// No sign-in, or the server ended it: sign in again.
    SignedOut,
    /// An answer that can't be read.
    Unreadable(String),
}

impl From<NetError> for ApiError {
    fn from(e: NetError) -> ApiError {
        ApiError::Net(e)
    }
}

/// The code the TV app gives a status whose body had none.
fn status_code(status: u16) -> &'static str {
    match status {
        400 => "bad_request",
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        409 => "conflict",
        410 => "gone",
        429 => "too_many_requests",
        503 => "unavailable",
        s if s >= 500 => "internal",
        _ => "bad_request",
    }
}

/// What the TV app shows for a session stopped from elsewhere without a note.
pub const STOPPED_BY_ADMIN: &str = "Playback was stopped by the server's administrator";

impl ApiError {
    /// The error a non-2xx answer stands for.
    pub fn from_response(r: &Response) -> ApiError {
        let json = serde_json::from_slice::<serde_json::Value>(&r.body).ok().filter(serde_json::Value::is_object);
        let body: ErrorBody = json.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
        let code = if body.code.is_empty() { status_code(r.status).to_string() } else { body.code };
        if r.status == 410 && code == "session_stopped" {
            let message = json
                .as_ref()
                .and_then(|v: &serde_json::Value| v.get("message"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            return ApiError::SessionStopped { message };
        }
        let message =
            if body.message.is_empty() { format!("the server refused (HTTP {})", r.status) } else { body.message };
        ApiError::Status { status: r.status, code, message, body: json }
    }

    /// The HTTP status, when the server answered.
    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Status { status, .. } => Some(*status),
            ApiError::SessionStopped { .. } => Some(410),
            _ => None,
        }
    }

    /// Whether the server answered `status`.
    pub fn is(&self, status: u16) -> bool {
        self.status() == Some(status)
    }

    /// Pley's error code (`not_found`, `insufficient_scope`…).
    pub fn code(&self) -> Option<&str> {
        match self {
            ApiError::Status { code, .. } => Some(code),
            ApiError::SessionStopped { .. } => Some("session_stopped"),
            _ => None,
        }
    }

    /// Whether this only says the work was cancelled: never shown.
    pub fn cancelled(&self) -> bool {
        matches!(self, ApiError::Net(NetError::Cancelled))
    }

    /// The server answered nothing: no connection, no answer in time, or a
    /// gateway in front of it saying it has none (502–504). Never a cancel. A
    /// 503 in Pley's own words (its JSON, with its `code`) is the server
    /// answering: pages show what it says, and the TV doesn't go offline for it.
    pub fn unreachable(&self) -> bool {
        match self {
            ApiError::Net(e) => *e != NetError::Cancelled,
            ApiError::Status { status: 502 | 504, .. } => true,
            ApiError::Status { status: 503, body, .. } => {
                !body.as_ref().is_some_and(|b| b.get("code").is_some_and(serde_json::Value::is_string))
            }
            _ => false,
        }
    }

    /// Nothing answers, or the server says it can't play just now (`503
    /// unavailable`: its transcoder restarts, say). Pages show the server's
    /// words for such a 503; mid-playback it is waited out like the server.
    pub fn out_of_reach(&self) -> bool {
        self.unreachable() || (self.is(503) && self.code() == Some("unavailable"))
    }

    /// Why the server answered nothing, as a sentence: "No answer in time.",
    /// "Nothing answers there."…
    pub fn unreachable_reason(&self) -> String {
        match self {
            ApiError::Net(e) => sentence(e.reason()),
            _ => sentence(NetError::Refused.reason()),
        }
    }

    /// What to show for this error under a screen's title: for an unreachable
    /// server, the reason and what to check; otherwise the error as a sentence.
    pub fn problem(&self) -> String {
        if self.unreachable() {
            format!("{} Check that the server is on and that this device is on its network.", self.unreachable_reason())
        } else {
            sentence(&self.to_string())
        }
    }

    /// The body read as `T` (a field past `code` and `message`).
    pub fn body_as<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        match self {
            ApiError::Status { body: Some(b), .. } => serde_json::from_value(b.clone()).ok(),
            _ => None,
        }
    }

    /// A Live TV `409`'s connections in use, when this is one.
    pub fn busy(&self) -> Option<LiveBusy> {
        if !self.is(409) {
            return None;
        }
        self.body_as::<LiveBusy>().filter(|b| !b.connections.is_empty())
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Net(e) => f.write_str(e.reason()),
            ApiError::Status { message, .. } => f.write_str(message),
            ApiError::SessionStopped { message } => f.write_str(message.as_deref().unwrap_or(STOPPED_BY_ADMIN)),
            ApiError::SignedOut => f.write_str("you were signed out: sign in again"),
            ApiError::Unreadable(_) => f.write_str("the server's answer can't be read: is it Pley, and up to date?"),
        }
    }
}

impl std::error::Error for ApiError {}

/// `text` as a sentence: trimmed, its first letter capitalised, a full stop
/// added unless it ends in `.`, `…`, `!` or `?` (the TV app's `sentence`).
pub fn sentence(text: &str) -> String {
    let t = text.trim();
    let mut chars = t.chars();
    let Some(first) = chars.next() else { return String::new() };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '…', '!', '?']) {
        out.push('.');
    }
    out
}
