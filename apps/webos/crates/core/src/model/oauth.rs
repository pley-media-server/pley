//! Sign-in as the server answers it: its authorization metadata (RFC 8414),
//! device codes (RFC 8628), tokens (RFC 6749), OAuth refusals, and the
//! answer to a discovery broadcast.

use serde::{Deserialize, Serialize};

use super::nd;

/// `GET /.well-known/oauth-authorization-server` as sent. `auth::ServerMetadata`
/// is what the app keeps once its checks pass.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationMetadata {
    #[serde(default, deserialize_with = "nd")]
    pub issuer: String,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
    pub device_authorization_endpoint: Option<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub grant_types_supported: Option<Vec<String>>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
}

/// The device authorization endpoint's answer (RFC 8628 §3.2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceAuthorization {
    /// What the TV polls with. Never shown, never logged.
    #[serde(default, deserialize_with = "nd")]
    pub device_code: String,
    /// `BCDF-GHJK`.
    #[serde(default, deserialize_with = "nd")]
    pub user_code: String,
    #[serde(default, deserialize_with = "nd")]
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    /// Seconds; 600 on Pley.
    pub expires_in: Option<f64>,
    /// Seconds between polls; 5 when left out.
    pub interval: Option<f64>,
}

/// The token endpoint's answer (RFC 6749 §5.1): a code approved, a refresh,
/// or a profile switch.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenAnswer {
    /// `pley_at_…`.
    #[serde(default, deserialize_with = "nd")]
    pub access_token: String,
    /// `Bearer` when present.
    pub token_type: Option<String>,
    /// Seconds; an hour when left out.
    pub expires_in: Option<f64>,
    /// What was granted; the scope asked for when left out.
    pub scope: Option<String>,
    /// `pley_rt_…`: good once.
    pub refresh_token: Option<String>,
}

/// A token, device or revocation endpoint's refusal (RFC 6749 §5.2), with
/// Pley's `reason` for profile switches (`wrong_pin`, `pin_locked`,
/// `throttled`, `browser`, `not_allowed`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OAuthError {
    #[serde(default, deserialize_with = "nd")]
    pub error: String,
    pub error_description: Option<String>,
    pub reason: Option<String>,
}

/// One answer to a discovery broadcast (`pley?` to UDP 7539), sent from the
/// address to use.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Announcement {
    /// The protocol's version: 1.
    #[serde(default, deserialize_with = "nd")]
    pub pley: u32,
    /// 40 hex digits, stable across restarts and renames.
    pub id: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default, deserialize_with = "nd")]
    pub version: String,
    /// The web port on the answering address.
    #[serde(default, deserialize_with = "nd")]
    pub port: u16,
}
