//! Signing a TV in to a Pley server: the server's authorization metadata
//! (RFC 8414) and its checks, the device code flow (RFC 8628), and the
//! tokens of each server (RFC 6749), refreshed one at a time, with Pley's
//! profile switch (packages/tv/src/auth, the Android app's `core/auth`).

mod device;
mod metadata;
pub mod proof;
mod tokens;

pub use device::{
    BACKOFF, DEFAULT_INTERVAL, DeviceCode, DevicePoll, DeviceSignIn, LinkEnd, LinkState, RENEWALS, SLOW_DOWN,
    display_code, scope_for, verification_address,
};
pub use metadata::{METADATA_PATH, ServerMetadata, fetch_metadata, parse_metadata};
pub use proof::{ServerProof, network_may_have_changed};
pub use tokens::{
    EARLY_MS, JUST_ISSUED_MS, StoredTokens, SwitchRefusal, TokenAnswerParsed, Tokens, extra_scopes, parse_token_answer,
    revoke,
};

use std::fmt;

use crate::http::{ApiError, Http, NetError, Request, Response};
use crate::model::OAuthError;

/// Pley for LG TV's OAuth client, built into the server
/// (`apps/server/src/oauth/builtin/webos.json`).
pub const CLIENT_ID: &str = "https://pley-media-server.github.io/pley/oauth/webos.json";
/// Everything Pley's own TV apps may ask for.
pub const SCOPES: [&str; 6] = ["library", "play", "playlists", "profiles", "requests", "approvals"];
/// Asked for only where the server lists them: one that doesn't know a scope refuses the whole
/// sign-in.
pub const LISTED_ONLY: [&str; 2] = ["requests", "approvals"];
/// RFC 8628's grant: a TV collects its tokens with the code it was given.
pub const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Pley's own apps move between the household's profiles with this grant.
pub const SWITCH_PROFILE_GRANT: &str = "urn:pley:params:oauth:grant-type:switch-profile";
/// What an older server answers to the metadata request.
pub const TOO_OLD: &str = "this server is too old for this app";
/// What a server without sign-in with a code says.
pub const NO_CODES: &str = "this server can't sign in TVs with a code yet: update it";
/// Why the scope guard signed the TV out.
pub const WIDER_SCOPE: &str = "the server allowed more than this app asks for: signed out";
/// How long a token, device or revocation request may take.
pub const FORM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Why the TV is signed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SignOutReason {
    /// The server refused a refresh or a switch: the sign-in is over.
    Ended,
    /// The server allowed more than the app asked for.
    WiderScope,
}

/// A sign-in problem.
#[derive(Clone, Debug, PartialEq)]
pub enum AuthError {
    /// No answer from the server; worth trying again by itself.
    Unreachable(NetError),
    /// A token, device or revocation endpoint refused (RFC 6749 §5.2).
    Refused { status: u16, oauth: Option<OAuthError> },
    /// The sign-in is over: sign in again.
    SignedOut(SignOutReason),
    /// Something to tell people, in a few words (an old server, an address elsewhere).
    Failed(String),
}

/// RFC 6749's error codes in words, for a refusal without a description.
fn oauth_words(code: &str) -> Option<&'static str> {
    Some(match code {
        "invalid_client" => "the server doesn't know this app",
        "unauthorized_client" => "the server doesn't let this app sign in this way",
        "invalid_grant" => "the server refused this sign-in",
        "access_denied" => "the sign-in was refused",
        "expired_token" => "the code expired",
        "invalid_scope" => "the server refused what this app asked for",
        "invalid_request" => "the server didn't understand the request",
        "unsupported_grant_type" => "the server doesn't take this kind of sign-in",
        _ => return None,
    })
}

impl AuthError {
    /// The OAuth error code of a refusal (`invalid_grant`…).
    pub fn code(&self) -> Option<&str> {
        match self {
            AuthError::Refused { oauth: Some(o), .. } => Some(&o.error),
            _ => None,
        }
    }

    /// Pley's reason for a refused profile switch (`wrong_pin`…).
    pub fn reason(&self) -> Option<&str> {
        match self {
            AuthError::Refused { oauth: Some(o), .. } => o.reason.as_deref(),
            _ => None,
        }
    }

    /// The HTTP status of a refusal.
    pub fn status(&self) -> Option<u16> {
        match self {
            AuthError::Refused { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// The server answered nothing (and it wasn't a cancel).
    pub fn unreachable(&self) -> bool {
        matches!(self, AuthError::Unreachable(e) if *e != NetError::Cancelled)
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Unreachable(_) => f.write_str("the server can't be reached: check its address and that it's on"),
            AuthError::Refused { status, oauth } => {
                let code = oauth.as_ref().map_or("", |o| o.error.as_str());
                match oauth.as_ref().and_then(|o| o.error_description.as_deref()).filter(|d| !d.is_empty()) {
                    Some(d) => f.write_str(d),
                    None => match oauth_words(code) {
                        Some(w) => f.write_str(w),
                        None if !code.is_empty() => write!(f, "the server refused ({})", code.replace('_', " ")),
                        None => write!(f, "the server refused (HTTP {status})"),
                    },
                }
            }
            AuthError::SignedOut(SignOutReason::WiderScope) => f.write_str(WIDER_SCOPE),
            AuthError::SignedOut(SignOutReason::Ended) => f.write_str("you were signed out: sign in again"),
            AuthError::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> ApiError {
        match e {
            AuthError::Unreachable(net) => ApiError::Net(net),
            AuthError::SignedOut(_) => ApiError::SignedOut,
            AuthError::Refused { status, ref oauth } => ApiError::Status {
                status,
                code: oauth.as_ref().map_or_else(|| "bad_request".into(), |o| o.error.clone()),
                message: e.to_string(),
                body: None,
            },
            AuthError::Failed(message) => ApiError::Status { status: 0, code: "failed".into(), message, body: None },
        }
    }
}

/// POSTs `fields` as a form to a token, device or revocation endpoint (15 s,
/// no redirects); the answer when it is 2xx, else the refusal.
///
/// Never cancelled with the task that asks: a refresh or switch whose answer
/// is lost has spent the refresh token on the server, which ends the sign-in.
pub(crate) fn post_form(http: &Http, url: &str, fields: &[(&str, &str)]) -> Result<Response, AuthError> {
    let req = Request::new(crate::http::Method::Post, url)
        .header("Accept", "application/json")
        .form(fields)
        .timeout(FORM_TIMEOUT)
        .cancel(None);
    let answer = http.send(req).map_err(AuthError::Unreachable)?;
    if answer.ok() {
        return Ok(answer);
    }
    let oauth = serde_json::from_slice::<OAuthError>(&answer.body).ok().filter(|o| !o.error.is_empty());
    Err(AuthError::Refused { status: answer.status, oauth })
}

#[cfg(test)]
mod tests;
