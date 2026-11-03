//! Where a server signs apps in (RFC 8414), kept only when it is the
//! server's own: its issuer is the address the app used and every endpoint is
//! on it, so an answer can't send the app's codes or tokens anywhere else
//! (packages/tv/src/auth/metadata.ts).

use std::time::Duration;

use super::{AuthError, DEVICE_CODE_GRANT, TOO_OLD};
use crate::addresses::origin;
use crate::http::{Http, Request};
use crate::model::AuthorizationMetadata;

/// Where the metadata is, on the server's origin.
pub const METADATA_PATH: &str = "/.well-known/oauth-authorization-server";

/// A server's sign-in endpoints, checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerMetadata {
    /// The server's origin.
    pub issuer: String,
    pub token_endpoint: String,
    pub revocation_endpoint: String,
    pub device_authorization_endpoint: Option<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub grant_types_supported: Option<Vec<String>>,
}

impl ServerMetadata {
    /// Whether a TV can sign in here with a code.
    pub fn signs_in_with_codes(&self) -> bool {
        self.device_authorization_endpoint.is_some()
            && self.grant_types_supported.as_ref().is_some_and(|g| g.iter().any(|t| t == DEVICE_CODE_GRANT))
    }
}

/// Asks `server` for its metadata (8 s) and checks it.
pub fn fetch_metadata(http: &Http, server: &str) -> Result<ServerMetadata, AuthError> {
    let home = origin(server).ok_or_else(|| AuthError::Failed("that isn't a server address".into()))?;
    let req = Request::get(format!("{home}{METADATA_PATH}"))
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(8));
    let answer = http.send(req).map_err(AuthError::Unreachable)?;
    if answer.status == 404 {
        return Err(AuthError::Failed(TOO_OLD.into()));
    }
    if !answer.ok() {
        return Err(AuthError::Failed(format!("the server couldn't answer (HTTP {})", answer.status)));
    }
    // Older servers answer their web app's page here.
    if !answer.header("Content-Type").unwrap_or("").to_ascii_lowercase().contains("json") {
        return Err(AuthError::Failed(TOO_OLD.into()));
    }
    parse_metadata(&answer.body, &home)
}

/// The checks of [`fetch_metadata`] on the answer `body` of the server at `server`.
pub fn parse_metadata(body: &[u8], server: &str) -> Result<ServerMetadata, AuthError> {
    let home = origin(server).ok_or_else(|| AuthError::Failed("that isn't a server address".into()))?;
    let too_old = || AuthError::Failed(TOO_OLD.into());
    let m: AuthorizationMetadata = serde_json::from_slice(body).map_err(|_| too_old())?;
    if m.issuer.is_empty() {
        return Err(too_old());
    }
    // RFC 8414 §3.3: the issuer is exactly the address asked at.
    if m.issuer != home {
        let shown: String = m.issuer.chars().take(100).collect();
        return Err(AuthError::Failed(format!(
            "this server calls itself {shown}: open it by that address, or check the proxy in front of it passes Host"
        )));
    }
    let endpoint = |url: Option<String>| -> Result<String, AuthError> {
        let url = url.filter(|u| !u.is_empty()).ok_or_else(too_old)?;
        if origin(&url).as_deref() != Some(m.issuer.as_str()) || !url.starts_with(&format!("{}/", m.issuer)) {
            return Err(AuthError::Failed("this server sends apps to another address to sign in: Pley doesn't".into()));
        }
        Ok(url)
    };
    let device = match m.device_authorization_endpoint.clone().filter(|u| !u.is_empty()) {
        Some(url) => Some(endpoint(Some(url))?),
        None => None,
    };
    let clean = |list: Option<Vec<String>>| list.map(|l| l.into_iter().filter(|s| !s.is_empty()).collect());
    Ok(ServerMetadata {
        token_endpoint: endpoint(m.token_endpoint.clone())?,
        revocation_endpoint: endpoint(m.revocation_endpoint.clone())?,
        device_authorization_endpoint: device,
        scopes_supported: clean(m.scopes_supported),
        grant_types_supported: clean(m.grant_types_supported),
        issuer: m.issuer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(issuer: &str, token: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "issuer": issuer,
            "token_endpoint": token,
            "revocation_endpoint": format!("{issuer}/api/oauth/revoke"),
            "device_authorization_endpoint": format!("{issuer}/api/oauth/device"),
            "grant_types_supported": ["refresh_token", DEVICE_CODE_GRANT],
            "scopes_supported": ["library", "play", "playlists", "profiles"]
        }))
        .unwrap()
    }

    #[test]
    fn a_servers_own_metadata_is_kept() {
        let server = "http://192.168.1.20:7539";
        let m = parse_metadata(&body(server, &format!("{server}/api/oauth/token")), server).unwrap();
        assert_eq!(m.token_endpoint, "http://192.168.1.20:7539/api/oauth/token");
        assert!(m.signs_in_with_codes());
    }

    #[test]
    fn an_issuer_elsewhere_is_refused() {
        let e =
            parse_metadata(&body("http://10.0.0.5:7539", "http://10.0.0.5:7539/api/oauth/token"), "http://nas:7539")
                .unwrap_err();
        assert!(e.to_string().starts_with("this server calls itself http://10.0.0.5:7539"), "{e}");
    }

    #[test]
    fn endpoints_elsewhere_are_refused() {
        let server = "http://nas:7539";
        let e = parse_metadata(&body(server, "https://evil.example.com/token"), server).unwrap_err();
        assert_eq!(e.to_string(), "this server sends apps to another address to sign in: Pley doesn't");
    }

    #[test]
    fn anything_else_is_too_old() {
        assert_eq!(parse_metadata(b"<html>", "http://nas:7539").unwrap_err().to_string(), TOO_OLD);
        assert_eq!(parse_metadata(b"{}", "http://nas:7539").unwrap_err().to_string(), TOO_OLD);
    }
}
