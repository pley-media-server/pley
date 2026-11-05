//! The apps pley knows, and the rules every app's description is read by.
//!
//! An app's client ID is the https address of its client metadata document
//! (CIMD §3). pley's own apps are built in: their documents ship with the
//! server and are checked by the same rules as anyone else's, with one
//! exception (their return addresses aren't on the host of their client
//! ID). Documents of other publishers' apps are fetched by
//! [`crate::oauth::fetch`] and read here, where nothing in them is believed
//! further than these rules allow.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny};
use url::{Host, Url};

use super::devices::DEVICE_GRANT;

/// The longest client ID, and the longest registered return address.
pub const MAX_ID_LEN: usize = 512;
/// The largest document pley reads (CIMD §8.7 suggests 5 KB).
pub const MAX_DOCUMENT_BYTES: usize = 5_120;
const MAX_REDIRECT_URIS: usize = 10;
const MAX_REDIRECT_LEN: usize = 512;
/// What's left of an app's name after cleaning is cut to this many characters.
const MAX_NAME_CHARS: usize = 64;

/// pley for Android.
pub const ANDROID_ID: &str = "https://pley-media-server.github.io/pley/oauth/android.json";
/// pley for desktop.
pub const DESKTOP_ID: &str = "https://pley-media-server.github.io/pley/oauth/desktop.json";
/// pley for Home Assistant.
pub const HOMEASSISTANT_ID: &str = "https://pley-media-server.github.io/pley/oauth/homeassistant.json";
/// pley for iPhone and iPad (iOS and iPadOS).
pub const IOS_ID: &str = "https://pley-media-server.github.io/pley/oauth/ios.json";
/// pley for Kodi.
pub const KODI_ID: &str = "https://pley-media-server.github.io/pley/oauth/kodi.json";
/// pley for Mac (macOS). The Apple app's, beside the desktop app.
pub const MACOS_ID: &str = "https://pley-media-server.github.io/pley/oauth/macos.json";
/// pley for Roku.
pub const ROKU_ID: &str = "https://pley-media-server.github.io/pley/oauth/roku.json";
/// pley for Samsung TV (Tizen).
pub const TIZEN_ID: &str = "https://pley-media-server.github.io/pley/oauth/tizen.json";
/// pley for Apple TV (tvOS).
pub const TVOS_ID: &str = "https://pley-media-server.github.io/pley/oauth/tvos.json";
/// pley for LG TV (webOS).
pub const WEBOS_ID: &str = "https://pley-media-server.github.io/pley/oauth/webos.json";
/// The client IDs of pley's own apps, matched as exact strings.
pub const BUILTIN_IDS: &[&str] =
    &[ANDROID_ID, DESKTOP_ID, HOMEASSISTANT_ID, IOS_ID, KODI_ID, MACOS_ID, ROKU_ID, TIZEN_ID, TVOS_ID, WEBOS_ID];

/// An app that may ask people to allow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// The client ID, byte for byte as the app sends it.
    pub id: String,
    /// The client ID's host, in ASCII: what the consent page shows first
    /// (CIMD §8.5).
    pub host: String,
    /// What the app calls itself, cleaned by [`clean_name`]. pley can't
    /// check it, so it is never shown on its own.
    pub name: Option<String>,
    /// One of pley's own apps (built in). No fetched document gets this.
    pub first_party: bool,
    /// The return addresses the app registered, as written.
    pub redirect_uris: Vec<String>,
    /// The document's `scope`, as written: when there is one, an app may
    /// ask for nothing beyond it (names pley doesn't know are ignored).
    pub scope_cap: Option<String>,
    /// The app gets refresh tokens (its `grant_types` lists
    /// `refresh_token`, RFC 7591 §2).
    pub refresh: bool,
    /// The app signs in with a code shown on a screen (its `grant_types`
    /// lists RFC 8628's device code grant). Only pley's own apps may, all
    /// the same: see [`super::devices`].
    pub device: bool,
}

/// Where an authorization answer goes, for the consent page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectKind {
    /// A website, on the app's own host.
    Web { host: String },
    /// An app on this device, by its private-use scheme.
    App { scheme: String },
    /// An app on this computer, listening on 127.0.0.1 or [::1].
    Loopback,
}

/// Why an app can't sign in. Each says what to do next.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error(
        "only Pley's own apps can sign in to this server: an administrator can allow apps by other publishers in \
         Settings → Server → Apps"
    )]
    NotAllowed,
    #[error("Pley can't read app descriptions here: only Pley's own apps can sign in to this server")]
    Off,
    #[error("this app's address isn't one Pley accepts ({0}): ask the app's publisher to fix it")]
    ClientId(&'static str),
    #[error(
        "this app uses the address of Pley's own apps or of this server: don't allow it, and ask the app's publisher \
         about it"
    )]
    Reserved,
    #[error("the app's description isn't one Pley accepts ({0}): ask the app's publisher to fix it")]
    Document(String),
    #[error("Pley couldn't read the app's description ({0}): try again later, or ask the app's publisher")]
    Unreachable(String),
    /// Its address is on a private network (it resolves to no public
    /// address), which pley never reads from: trying again doesn't help.
    #[error("the app's description is on a private network, which Pley never reads from: ask the app's publisher")]
    Private,
    #[error("too many app descriptions read: try again in a minute")]
    Busy,
    #[error("the app asked to send you back to an address it didn't register ({0}): ask the app's publisher to fix it")]
    Redirect(&'static str),
}

/// Checks a client ID by pley's stricter subset of CIMD §3, and gives the
/// address to fetch, which is byte for byte the client ID.
///
/// It doesn't check whose host it is: see [`is_reserved_host`] and
/// [`crate::oauth::fetch::Documents::get`].
pub fn check_client_id(id: &str) -> Result<Url, ClientError> {
    use ClientError::ClientId as Bad;
    if id.len() > MAX_ID_LEN {
        return Err(Bad("it is longer than 512 characters"));
    }
    let url = Url::parse(id).map_err(|_| Bad("it isn't an address"))?;
    // CIMD compares client IDs as simple strings, so the one way of writing
    // each is the only one accepted. This refuses upper-case hosts, default
    // ports, dot segments (%2e%2e too), backslashes, IPv4 addresses written
    // as one number, Unicode hosts (they must come as punycode) and stray
    // whitespace.
    if url.as_str() != id {
        return Err(Bad("it isn't written in its standard form"));
    }
    if url.scheme() != "https" {
        return Err(Bad("it doesn't start with https://"));
    }
    // Domain names only: the HTTP client connects to an IP address without
    // asking the resolver, which is where private addresses are filtered
    // out (see fetch.rs).
    let Some(Host::Domain(host)) = url.host() else {
        return Err(Bad("its host isn't a domain name"));
    };
    if !host.contains('.') || host.ends_with('.') || host.ends_with(".localhost") {
        return Err(Bad("its host isn't a public domain name"));
    }
    // pley's policy, as atproto's: documents are always fetched from 443.
    if url.port().is_some() {
        return Err(Bad("it has a port"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Bad("it has a user name or password"));
    }
    // CIMD §3: SHOULD NOT have a query; pley refuses one, even an empty "?".
    if url.query().is_some() {
        return Err(Bad("it has a query"));
    }
    if url.fragment().is_some() {
        return Err(Bad("it has a fragment"));
    }
    if url.path() == "/" {
        return Err(Bad("it has no path"));
    }
    // CIMD §3: no "." or ".." segments. The canonical form above has none
    // already; this reads the string as it came, should that ever change.
    if id.split('/').skip(3).any(is_dot_segment) {
        return Err(Bad("its path has . or .. segments"));
    }
    Ok(url)
}

fn is_dot_segment(segment: &str) -> bool {
    let segment = segment.to_ascii_lowercase().replace("%2e", ".");
    segment == "." || segment == ".."
}

/// The fields of a client metadata document that pley reads. Everything
/// else is ignored, and never fetched: logo_uri, client_uri, tos_uri,
/// policy_uri, jwks and jwks_uri (CIMD §8.8; RFC 7591 §5). A field given
/// twice is refused (serde's derive does that).
#[derive(Deserialize)]
struct Document {
    client_id: String,
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    grant_types: Option<Vec<String>>,
    #[serde(default)]
    response_types: Option<Vec<String>>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default, deserialize_with = "present")]
    client_secret: bool,
    #[serde(default, deserialize_with = "present")]
    client_secret_expires_at: bool,
}

/// Whether a field is there at all, whatever its value (null too).
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}

/// Reads the client metadata document of `client_id`.
///
/// `trusted` is for pley's own apps only: their return addresses needn't be
/// on the client ID's host (rules (a) and (b) below), and those that run
/// only on TVs sign in only with a code, without `authorization_code` and
/// return addresses; every other rule applies to them too. The client is
/// never `first_party` here.
pub fn parse_document(client_id: &str, bytes: &[u8], trusted: bool) -> Result<Client, ClientError> {
    let bad = |why: &str| Err(ClientError::Document(why.to_owned()));
    let url = check_client_id(client_id)?;
    let host = url.host_str().unwrap_or_default().to_owned();
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return bad("it is larger than 5,120 bytes");
    }
    // A JSON object only: serde's derive would also take an array, field by
    // field.
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return bad("it isn't a JSON object");
    }
    // serde's messages may quote the document, which pley never shows.
    let document: Document = match serde_json::from_slice(bytes) {
        Ok(document) => document,
        Err(e) if e.is_data() => return bad("a field is missing, given twice or of the wrong type"),
        Err(_) => return bad("it isn't valid JSON"),
    };
    // CIMD §4: the document names the address it was read from.
    if document.client_id != client_id {
        return bad("its client_id isn't the address it was read from");
    }
    // CIMD §4.1: no shared secrets.
    if document.client_secret || document.client_secret_expires_at {
        return bad("it has a client secret");
    }
    // Every app is public: PKCE and exact return addresses stand in for
    // client authentication. client_secret_* are forbidden (CIMD §4.1), and
    // private_key_jwt would have pley verify keys (§8.2). Absent is read as
    // "none" although RFC 7591's default is client_secret_basic, which CIMD
    // forbids anyway.
    if !matches!(document.token_endpoint_auth_method.as_deref(), None | Some("none")) {
        return bad("its token_endpoint_auth_method isn't \"none\"");
    }
    // RFC 7591 §2: absent means ["authorization_code"]. pley reads
    // refresh_token and the device code grant from it; others are ignored.
    let grant_types = document.grant_types.unwrap_or_else(|| vec!["authorization_code".to_owned()]);
    let refresh = grant_types.iter().any(|g| g == "refresh_token");
    let device = grant_types.iter().any(|g| g == DEVICE_GRANT);
    // pley's apps for TVs that have no browser (Kodi, Roku) get no code:
    // with no return address, none can be sent anywhere.
    let code_only = trusted && device && !grant_types.iter().any(|g| g == "authorization_code");
    if code_only {
        if !document.redirect_uris.is_empty() || document.response_types.is_some() {
            return bad("an app that signs in only with a code has redirect_uris or response_types");
        }
    } else {
        if !grant_types.iter().any(|g| g == "authorization_code") {
            return bad("its grant_types leave out authorization_code");
        }
        if document.response_types.as_ref().is_some_and(|types| !types.iter().any(|t| t == "code")) {
            return bad("its response_types leave out code");
        }
        if document.redirect_uris.is_empty() {
            return bad("it has no redirect_uris");
        }
    }
    if document.redirect_uris.len() > MAX_REDIRECT_URIS {
        return bad("it has more than 10 redirect_uris");
    }
    for uri in &document.redirect_uris {
        if let Err(why) = check_registered_redirect(uri, &host, trusted) {
            return Err(ClientError::Document(format!("a redirect URI {why}")));
        }
    }
    Ok(Client {
        id: client_id.to_owned(),
        host,
        name: document.client_name.as_deref().and_then(clean_name),
        first_party: false,
        redirect_uris: document.redirect_uris,
        scope_cap: document.scope,
        refresh,
        device,
    })
}

/// A registered return address must be one of:
/// (a) https on exactly the client ID's host, without a port, user name or
///     fragment (CIMD §8.1 allows the restriction; the host shown on the
///     consent page is then the one that gets the code);
/// (b) a private-use scheme with a dot that is the client ID's host
///     reversed (apps.example.com → com.example.apps), as scheme:/path and
///     never scheme:// (RFC 8252 §7.1, §8.4; OAuth 2.1 -16 §2.3.1: schemes
///     without a period SHOULD be refused);
/// (c) http://127.0.0.1[:port]/… or http://[::1][:port]/… (RFC 8252 §7.3).
///
/// "localhost" (OAuth 2.1 -16 §8.5.4), any other http, fragments and user
/// names are refused. `trusted` skips only the host relationship of (a)
/// and (b).
fn check_registered_redirect(uri: &str, client_host: &str, trusted: bool) -> Result<(), &'static str> {
    if uri.len() > MAX_REDIRECT_LEN {
        return Err("is longer than 512 characters");
    }
    let url = Url::parse(uri).map_err(|_| "isn't an address")?;
    // Requests must send the canonical form (see redirect_kind), so any
    // other form could never match.
    if url.as_str() != uri {
        return Err("isn't written in its standard form");
    }
    if url.fragment().is_some() {
        return Err("has a fragment");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("has a user name or password");
    }
    match url.scheme() {
        "https" => {
            let on_host = match url.host() {
                Some(Host::Domain(host)) => trusted || host == client_host,
                _ => false,
            };
            if !on_host {
                return Err("on https isn't on the app's own host");
            }
            if url.port().is_some() {
                return Err("has a port");
            }
        }
        "http" => {
            if !is_loopback(&url) {
                return Err("on http isn't on 127.0.0.1 or [::1]");
            }
        }
        scheme => {
            if !scheme.contains('.') {
                return Err("has a scheme without a dot");
            }
            if !trusted && scheme != reversed(client_host) {
                return Err("has a scheme that isn't the app's host reversed");
            }
            let rest = &uri[scheme.len() + 1..];
            if url.host().is_some() || !rest.starts_with('/') || rest.starts_with("//") {
                return Err("isn't written scheme:/path");
            }
        }
    }
    Ok(())
}

/// apps.example.com → com.example.apps
fn reversed(host: &str) -> String {
    host.split('.').rev().collect::<Vec<_>>().join(".")
}

/// http on the loopback IP literals (never "localhost").
fn is_loopback(url: &Url) -> bool {
    url.scheme() == "http"
        && match url.host() {
            Some(Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
            Some(Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
            _ => false,
        }
}

/// Checks the return address a request asks for, and says where it goes.
///
/// It must be canonical and equal one registered string (CIMD §4.2 "exact
/// match"; OAuth 2.1 -16 §2.3.1). The one exception: for a registered
/// loopback address, scheme, host, path and query must match and the port
/// may differ (OAuth 2.1 -16 §2.3.1; RFC 8252 §7.3). 127.0.0.1 and [::1]
/// are not interchangeable. pley then sends the answer to the address as
/// requested, port included.
pub fn redirect_kind(client: &Client, requested: &str) -> Result<RedirectKind, ClientError> {
    let url = Url::parse(requested).map_err(|_| ClientError::Redirect("it isn't an address"))?;
    if url.as_str() != requested {
        return Err(ClientError::Redirect("it isn't written in its standard form"));
    }
    if client.redirect_uris.iter().any(|r| r == requested) {
        return Ok(match url.scheme() {
            "https" => RedirectKind::Web { host: url.host_str().unwrap_or_default().to_owned() },
            "http" => RedirectKind::Loopback,
            scheme => RedirectKind::App { scheme: scheme.to_owned() },
        });
    }
    if is_loopback(&url) && url.fragment().is_none() && url.username().is_empty() && url.password().is_none() {
        let same_but_port =
            client.redirect_uris.iter().filter_map(|r| Url::parse(r).ok()).any(|r| {
                is_loopback(&r) && r.host() == url.host() && r.path() == url.path() && r.query() == url.query()
            });
        if same_but_port {
            return Ok(RedirectKind::Loopback);
        }
    }
    Err(ClientError::Redirect("the app didn't register it"))
}

/// What an app calls itself, made safe to show: Unicode control and format
/// characters go (bidi controls such as U+202E, zero-width characters),
/// whitespace is collapsed and trimmed, and the rest is cut to 64
/// characters. Nothing left means no name. A name is never trusted.
///
/// Whitespace controls (tabs, line breaks) count as whitespace, so that
/// "pley\tapp" reads "pley app" rather than "pleyapp".
pub fn clean_name(raw: &str) -> Option<String> {
    let kept: String = raw
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() {
                Some(' ')
            } else if c.is_control() || is_format(c) {
                None
            } else {
                Some(c)
            }
        })
        .collect();
    let collapsed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = collapsed.chars().take(MAX_NAME_CHARS).collect();
    let cut = cut.trim_end();
    (!cut.is_empty()).then(|| cut.to_owned())
}

/// Unicode's format characters (general category Cf, Unicode 16), which
/// std can't tell apart: soft hyphens, bidi controls, zero-width
/// characters, byte order marks, tags and the like.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

/// pley's own apps (CIMD §7.2: "An authorization server MAY pre-register
/// Client Identifier URLs"). They are never fetched, online or not: they
/// sign in to servers without internet access, and a lost domain can't
/// change where their codes go.
static BUILTINS: LazyLock<Vec<Client>> = LazyLock::new(|| {
    [
        (ANDROID_ID, include_str!("builtin/android.json")),
        (DESKTOP_ID, include_str!("builtin/desktop.json")),
        (HOMEASSISTANT_ID, include_str!("builtin/homeassistant.json")),
        (IOS_ID, include_str!("builtin/ios.json")),
        (KODI_ID, include_str!("builtin/kodi.json")),
        (MACOS_ID, include_str!("builtin/macos.json")),
        (ROKU_ID, include_str!("builtin/roku.json")),
        (TIZEN_ID, include_str!("builtin/tizen.json")),
        (TVOS_ID, include_str!("builtin/tvos.json")),
        (WEBOS_ID, include_str!("builtin/webos.json")),
    ]
    .into_iter()
    .map(|(id, json)| {
        let mut client =
            parse_document(id, json.as_bytes(), true).unwrap_or_else(|e| panic!("built-in client {id}: {e}"));
        client.first_party = true;
        client
    })
    .collect()
});

/// One of pley's own apps, by its exact client ID.
pub fn builtin(id: &str) -> Option<&'static Client> {
    BUILTINS.iter().find(|client| client.id == id)
}

/// The host of pley's own client IDs: no other app may use it, so that the
/// "pley's own app" mark can't be imitated.
pub fn is_reserved_host(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host);
    BUILTINS.iter().any(|client| client.host.eq_ignore_ascii_case(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "https://apps.example.com/pley.json";

    /// A valid document for [`ID`], with `extra` fields added (each ending
    /// in a comma).
    fn document(extra: &str) -> String {
        format!(r#"{{{extra} "client_id": "{ID}", "redirect_uris": ["https://apps.example.com/callback"]}}"#)
    }

    fn with_redirects(redirects: &[&str]) -> String {
        let redirects = serde_json::to_string(redirects).unwrap();
        format!(r#"{{"client_id": "{ID}", "redirect_uris": {redirects}}}"#)
    }

    fn parse(json: &str) -> Result<Client, ClientError> {
        parse_document(ID, json.as_bytes(), false)
    }

    #[test]
    fn client_ids_must_be_canonical_https_urls_with_a_path() {
        let url = check_client_id(ID).unwrap();
        assert_eq!(url.as_str(), ID);
        assert_eq!(url.host_str(), Some("apps.example.com"));
        let longest = format!("https://apps.example.com/{}", "a".repeat(MAX_ID_LEN - 25));
        assert_eq!(longest.len(), 512);
        assert!(check_client_id(&longest).is_ok(), "512 characters are fine");

        let refused = [
            "http://apps.example.com/pley.json",
            "https://127.0.0.1/pley.json",
            "https://[::1]/pley.json",
            "https://[2606:4700:4700::1111]/pley.json",
            "https://2130706433/pley.json",
            "https://0x7f000001/pley.json",
            "https://localhost/pley.json",
            "https://pley.localhost/pley.json",
            "https://intranet/pley.json",
            "https://apps.example.com:8443/pley.json",
            "https://apps.example.com:443/pley.json",
            "https://user@apps.example.com/pley.json",
            "https://user:secret@apps.example.com/pley.json",
            "https://apps.example.com/",
            "https://apps.example.com",
            "https://apps.example.com/pley.json?v=1",
            "https://apps.example.com/pley.json?",
            "https://apps.example.com/pley.json#top",
            "https://apps.example.com/pley.json#",
            "https://apps.example.com/a/../pley.json",
            "https://apps.example.com/a/./pley.json",
            "https://apps.example.com/a/%2e%2e/pley.json",
            "https://apps.example.com/a/%2E%2e/pley.json",
            "https://apps.example.com/a\\pley.json",
            "https://Apps.Example.com/pley.json",
            "https://apps.example.com./pley.json",
            "https://bücher.example/pley.json",
            "https://xn--bcher-kva.example/pley.json\n",
            " https://apps.example.com/pley.json",
            "",
            "pley.json",
        ];
        for id in refused {
            assert!(
                matches!(check_client_id(id), Err(ClientError::ClientId(_))),
                "{id:?} should be refused, got {:?}",
                check_client_id(id)
            );
        }
        let too_long = format!("{longest}a");
        assert_eq!(too_long.len(), 513);
        assert_eq!(check_client_id(&too_long), Err(ClientError::ClientId("it is longer than 512 characters")));
        assert!(check_client_id("https://xn--bcher-kva.example/pley.json").is_ok(), "punycode is how IDNs come");
    }

    #[test]
    fn dot_segments_are_found_in_the_raw_string() {
        // The canonical check removes these before the raw check sees them;
        // the raw check is tested on its own here.
        for segment in [".", "..", "%2e", "%2E%2e", ".%2e", "%2e."] {
            assert!(is_dot_segment(segment), "{segment}");
        }
        for segment in ["", "a", "...", ".well-known", "%2e%2e%2e"] {
            assert!(!is_dot_segment(segment), "{segment}");
        }
    }

    #[test]
    fn documents_are_checked_before_anything_is_shown() {
        // The defaults.
        let client = parse(&document("")).unwrap();
        assert_eq!(
            client,
            Client {
                id: ID.into(),
                host: "apps.example.com".into(),
                name: None,
                first_party: false,
                redirect_uris: vec!["https://apps.example.com/callback".into()],
                scope_cap: None,
                refresh: false,
                device: false,
            }
        );
        // Everything a document may say.
        let client = parse(&document(
            r#""client_name": " Example  App ", "grant_types": ["authorization_code", "refresh_token"],
               "response_types": ["code"], "token_endpoint_auth_method": "none", "scope": "library play","#,
        ))
        .unwrap();
        assert_eq!(client.name.as_deref(), Some("Example App"));
        assert!(client.refresh, "refresh tokens when refresh_token is listed");
        assert_eq!(client.scope_cap.as_deref(), Some("library play"), "the scope cap, as written");
        assert!(!client.first_party, "documents never make pley's own apps");
        assert!(!client.device);
        // The device code grant is read from any document (only pley's own
        // apps may use it, all the same).
        let tv = parse(&document(
            r#""grant_types": ["authorization_code", "urn:ietf:params:oauth:grant-type:device_code"],"#,
        ))
        .unwrap();
        assert!(tv.device && !tv.refresh && !tv.first_party);
        assert!(parse(&document(r#""grant_types": ["urn:ietf:params:oauth:grant-type:device_code"],"#)).is_err());
        // Other grant types are ignored.
        assert!(parse(&document(r#""grant_types": ["authorization_code", "client_credentials"],"#)).is_ok());
        // Unknown fields, and fields pley never reads or fetches, are ignored.
        let ignored = document(
            r#""logo_uri": "https://apps.example.com/logo.png", "jwks_uri": "https://apps.example.com/jwks",
               "client_uri": "https://apps.example.com/", "tos_uri": 5, "software_id": {"a": [1]},"#,
        );
        assert!(parse(&ignored).is_ok());

        let refused = [
            // The document must name the address it was read from (CIMD §4).
            r#"{"client_id": "https://apps.example.com/other.json", "redirect_uris": ["https://apps.example.com/c"]}"#
                .to_owned(),
            r#"{"redirect_uris": ["https://apps.example.com/callback"]}"#.to_owned(),
            // No secrets (CIMD §4.1), even empty ones.
            document(r#""client_secret": "s3cret","#),
            document(r#""client_secret": null,"#),
            document(r#""client_secret_expires_at": 0,"#),
            // Public clients only.
            document(r#""token_endpoint_auth_method": "client_secret_basic","#),
            document(r#""token_endpoint_auth_method": "client_secret_post","#),
            document(r#""token_endpoint_auth_method": "private_key_jwt","#),
            document(r#""token_endpoint_auth_method": "None","#),
            // 1 to 10 return addresses.
            format!(r#"{{"client_id": "{ID}"}}"#),
            with_redirects(&[]),
            with_redirects(&["https://apps.example.com/callback"; 11]),
            document(r#""grant_types": ["refresh_token"],"#),
            document(r#""grant_types": [],"#),
            document(r#""response_types": ["token"],"#),
            // Duplicate keys, here a second list of return addresses.
            document(r#""redirect_uris": ["https://evil.example.com/callback"],"#),
            document(r#""client_name": "a", "client_name": "b","#),
            // Not an object, even one serde would read field by field.
            format!(r#"["{ID}", ["https://apps.example.com/callback"]]"#),
            "".to_owned(),
            "{".to_owned(),
            document(r#""client_name": 5,"#),
        ];
        for json in refused {
            assert!(
                matches!(parse(&json), Err(ClientError::Document(_))),
                "{json} should be refused: {:?}",
                parse(&json)
            );
        }
        assert!(parse(&with_redirects(&["https://apps.example.com/callback"; 10])).is_ok(), "10 are fine");

        // At most 5,120 bytes, even of a valid document.
        let small = document("");
        let padded = format!("{small}{}", " ".repeat(MAX_DOCUMENT_BYTES - small.len()));
        assert!(parse(&padded).is_ok(), "5,120 bytes are fine");
        assert_eq!(parse(&format!("{padded} ")), Err(ClientError::Document("it is larger than 5,120 bytes".into())));

        // serde's messages, which can quote the document, are never passed on.
        let quoted = parse(&document(r#""client_name": ["visit evil.example.com"],"#)).unwrap_err();
        assert!(!quoted.to_string().contains("evil"), "{quoted}");
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("pley \u{202E}rof ppa").as_deref(), Some("pley rof ppa"));
        assert_eq!(clean_name("\u{2066}Ex\u{200B}ample\u{2069}\u{FEFF}").as_deref(), Some("Example"));
        assert_eq!(clean_name("a\u{0007}b\u{009B}c\u{00AD}d\u{E0041}").as_deref(), Some("abcd"));
        assert_eq!(clean_name("  pley\t\n  app\u{2028} ").as_deref(), Some("pley app"));
        assert_eq!(clean_name(&"x".repeat(65)), Some("x".repeat(64)));
        assert_eq!(clean_name(&"é".repeat(70)), Some("é".repeat(64)), "characters, not bytes");
        assert_eq!(clean_name(&format!("{} y", "x".repeat(63))), Some("x".repeat(63)), "no space left at the end");
        assert_eq!(clean_name(" \t\u{3000} "), None);
        assert_eq!(clean_name("\u{200B}\u{202E}"), None);
        assert_eq!(clean_name(""), None);
    }

    #[test]
    fn redirect_uris_belong_to_the_app() {
        let accepted = [
            "https://apps.example.com/callback",
            "https://apps.example.com/callback?from=pley",
            "com.example.apps:/callback",
            "http://127.0.0.1/callback",
            "http://127.0.0.1:8080/callback",
            "http://[::1]/callback",
            "http://[::1]:8080/callback",
        ];
        for uri in accepted {
            assert!(
                parse(&with_redirects(&[uri])).is_ok(),
                "{uri} should be accepted: {:?}",
                parse(&with_redirects(&[uri]))
            );
        }
        let refused = [
            // (a) on another host, with a port, user name or fragment.
            "https://other.example.com/callback",
            "https://example.com/callback",
            "https://evil.apps.example.com/callback",
            "https://127.0.0.1/callback",
            "https://apps.example.com:8443/callback",
            "https://user@apps.example.com/callback",
            "https://apps.example.com/callback#done",
            // (b) not the reversed host, no dot, or scheme://.
            "apps.example.com:/callback",
            "com.example.other:/callback",
            "pleyapp:/callback",
            "javascript:alert(1)",
            "com.example.apps://callback",
            "com.example.apps:///callback",
            "com.example.apps:callback",
            "com.example.apps:/callback#x",
            // (c) only the loopback IP literals.
            "http://localhost/callback",
            "http://localhost:8080/callback",
            "http://apps.example.com/callback",
            "http://127.0.0.2/callback",
            "http://192.168.1.2/callback",
            "http://127.0.0.1/callback#x",
            "http://user@127.0.0.1/callback",
            // Not canonical, so a request could never match it.
            "HTTPS://apps.example.com/callback",
            "https://apps.example.com",
            "http://127.0.0.1:80/callback",
            "not an address",
        ];
        for uri in refused {
            assert!(matches!(parse(&with_redirects(&[uri])), Err(ClientError::Document(_))), "{uri} should be refused");
        }
        let long = format!("https://apps.example.com/{}", "a".repeat(MAX_REDIRECT_LEN - 25));
        assert_eq!(long.len(), 512);
        assert!(parse(&with_redirects(&[&long])).is_ok(), "512 characters are fine");
        let longer = format!("{long}a");
        assert!(parse(&with_redirects(&[&longer])).is_err(), "513 aren't");
        // One bad address spoils the document.
        assert!(parse(&with_redirects(&["https://apps.example.com/a", "http://localhost/b"])).is_err());

        // Trusted documents skip only the host relationship of (a) and (b).
        let trusted = |uri: &str| parse_document(ID, with_redirects(&[uri]).as_bytes(), true);
        assert!(trusted("https://other.example.com/callback").is_ok());
        assert!(trusted("com.example.other:/callback").is_ok());
        for uri in [
            "pleyapp:/callback",
            "com.example.other://callback",
            "https://other.example.com:8443/callback",
            "https://127.0.0.1/callback",
            "https://other.example.com/callback#x",
            "http://localhost/callback",
            "http://other.example.com/callback",
        ] {
            assert!(trusted(uri).is_err(), "{uri} is refused even when trusted");
        }
    }

    #[test]
    fn redirect_uris_match_exactly_but_for_a_loopback_port() {
        let client = parse(&with_redirects(&[
            "https://apps.example.com/callback",
            "com.example.apps:/callback",
            "http://127.0.0.1/callback",
            "http://[::1]:9000/callback?from=pley",
        ]))
        .unwrap();
        let kind = |uri: &str| redirect_kind(&client, uri);
        assert_eq!(
            kind("https://apps.example.com/callback"),
            Ok(RedirectKind::Web { host: "apps.example.com".into() })
        );
        assert_eq!(kind("com.example.apps:/callback"), Ok(RedirectKind::App { scheme: "com.example.apps".into() }));
        assert_eq!(kind("http://127.0.0.1/callback"), Ok(RedirectKind::Loopback));
        assert_eq!(kind("http://127.0.0.1:53111/callback"), Ok(RedirectKind::Loopback), "any loopback port");
        assert_eq!(kind("http://[::1]:9000/callback?from=pley"), Ok(RedirectKind::Loopback));
        assert_eq!(kind("http://[::1]/callback?from=pley"), Ok(RedirectKind::Loopback));
        assert_eq!(kind("http://[::1]:5/callback?from=pley"), Ok(RedirectKind::Loopback));

        for uri in [
            "https://apps.example.com/callback/",
            "https://apps.example.com/Callback",
            "https://APPS.example.com/callback",
            "https://apps.example.com/callback?x=1",
            "https://apps.example.com/callback#",
            "https://apps.example.com:8443/callback",
            "https://apps.example.com:443/callback",
            "https://apps.example.com/./callback",
            "com.example.apps:/callback/",
            "COM.example.apps:/callback",
            "http://127.0.0.1:53111/callback/",
            "http://127.0.0.1:53111/callback?x=1",
            "http://127.0.0.1:53111/callback#x",
            "http://user@127.0.0.1:53111/callback",
            "http://localhost:53111/callback",
            "http://[::1]:53111/callback",
            "http://127.0.0.1:9000/callback?from=pley",
            "http://[::1]:5/callback",
            "http://127.0.0.2/callback",
            "https://127.0.0.1/callback",
            "",
        ] {
            assert!(matches!(kind(uri), Err(ClientError::Redirect(_))), "{uri} should be refused: {:?}", kind(uri));
        }
    }

    #[test]
    fn pleys_own_apps_are_built_in_and_valid() {
        let android = builtin(ANDROID_ID).unwrap();
        assert_eq!(
            *android,
            Client {
                id: ANDROID_ID.into(),
                host: "pley-media-server.github.io".into(),
                name: Some("Pley for Android".into()),
                first_party: true,
                redirect_uris: vec!["app.pley.android:/oauth/callback".into()],
                scope_cap: Some("library play playlists profiles requests approvals".into()),
                refresh: true,
                // The same app runs on phones, Android TV and Fire TV.
                device: true,
            }
        );
        let desktop = builtin(DESKTOP_ID).unwrap();
        assert_eq!(desktop.name.as_deref(), Some("Pley for desktop"));
        assert_eq!(desktop.redirect_uris, ["http://127.0.0.1/oauth/callback", "http://[::1]/oauth/callback"]);
        assert!(desktop.first_party && desktop.refresh);
        assert!(!desktop.device, "desktops have browsers");
        // Its window shows the server's web pages, signed in as the web is:
        // it never asks for movies and shows with its token.
        assert_eq!(desktop.scope_cap.as_deref(), Some("library play playlists profiles"));
        assert_eq!(
            redirect_kind(android, "app.pley.android:/oauth/callback"),
            Ok(RedirectKind::App { scheme: "app.pley.android".into() })
        );
        assert_eq!(redirect_kind(desktop, "http://127.0.0.1:53111/oauth/callback"), Ok(RedirectKind::Loopback));

        // Both are valid documents; Android's redirect isn't on its host,
        // which only being built in allows.
        let android_json = include_str!("builtin/android.json");
        assert!(parse_document(ANDROID_ID, android_json.as_bytes(), true).is_ok());
        assert!(parse_document(ANDROID_ID, android_json.as_bytes(), false).is_err());
        assert!(parse_document(DESKTOP_ID, include_str!("builtin/desktop.json").as_bytes(), true).is_ok());

        // The Apple app on iPhone, iPad and Mac comes back by one scheme,
        // with its own client ID on each, so a Mac with the desktop app too
        // shows two sign-ins people can tell apart. An iPad on a stand or a
        // shared iPhone may sign in with a code; a Mac has a browser.
        for (id, name, device, json) in [
            (IOS_ID, "Pley for iPhone and iPad", true, include_str!("builtin/ios.json")),
            (MACOS_ID, "Pley for Mac", false, include_str!("builtin/macos.json")),
        ] {
            let apple = builtin(id).unwrap();
            assert_eq!(apple.name.as_deref(), Some(name));
            assert_eq!(apple.redirect_uris, ["app.pley.apple:/oauth/callback"]);
            assert_eq!(apple.scope_cap.as_deref(), Some("library play playlists profiles requests approvals"));
            assert!(apple.first_party && apple.refresh);
            assert_eq!(apple.device, device, "{name}");
            assert_eq!(
                redirect_kind(apple, "app.pley.apple:/oauth/callback"),
                Ok(RedirectKind::App { scheme: "app.pley.apple".into() })
            );
            for refused in [
                "app.pley.android:/oauth/callback",
                "app.pley.apple:/oauth/callback/",
                "app.pley.apple:/other",
                "http://127.0.0.1:53111/oauth/callback",
            ] {
                assert!(matches!(redirect_kind(apple, refused), Err(ClientError::Redirect(_))), "{name}: {refused}");
            }
            assert!(parse_document(id, json.as_bytes(), true).is_ok());
            assert!(parse_document(id, json.as_bytes(), false).is_err());
        }

        // Kodi, Roku, Apple TV and the Samsung and LG TVs sign in only with a
        // code: no return address, so no code can be sent to them, and only
        // being built in allows that.
        for (id, name, json) in [
            (KODI_ID, "Pley for Kodi", include_str!("builtin/kodi.json")),
            (ROKU_ID, "Pley for Roku", include_str!("builtin/roku.json")),
            (TVOS_ID, "Pley for Apple TV", include_str!("builtin/tvos.json")),
            (TIZEN_ID, "Pley for Samsung TV", include_str!("builtin/tizen.json")),
            (WEBOS_ID, "Pley for LG TV", include_str!("builtin/webos.json")),
        ] {
            let tv = builtin(id).unwrap();
            assert_eq!(tv.name.as_deref(), Some(name));
            assert!(tv.first_party && tv.refresh && tv.device);
            assert!(tv.redirect_uris.is_empty());
            assert_eq!(tv.scope_cap.as_deref(), Some("library play playlists profiles requests approvals"));
            for refused in ["app.pley.android:/oauth/callback", "app.pley.apple:/oauth/callback"] {
                assert!(matches!(redirect_kind(tv, refused), Err(ClientError::Redirect(_))), "{name}: {refused}");
            }
            assert!(parse_document(id, json.as_bytes(), false).is_err());
        }
        let tv_only = |extra: &str| {
            format!(
                r#"{{"client_id": "{KODI_ID}", "grant_types": ["urn:ietf:params:oauth:grant-type:device_code"],
                    {extra}}}"#
            )
        };
        assert!(parse_document(KODI_ID, tv_only(r#""redirect_uris": []"#).as_bytes(), true).is_ok());
        for refused in [
            r#""redirect_uris": ["app.pley.kodi:/oauth/callback"]"#,
            r#""redirect_uris": [], "response_types": ["code"]"#,
        ] {
            assert!(parse_document(KODI_ID, tv_only(refused).as_bytes(), true).is_err(), "{refused}");
        }
        let no_grant =
            format!(r#"{{"client_id": "{KODI_ID}", "grant_types": ["refresh_token"], "redirect_uris": []}}"#);
        assert!(parse_document(KODI_ID, no_grant.as_bytes(), true).is_err(), "a code or a return address");

        // Home Assistant signs in only with a code too, and may ask for less:
        // no playlists, profiles, requests or approvals.
        let home = builtin(HOMEASSISTANT_ID).unwrap();
        assert_eq!(home.name.as_deref(), Some("Pley for Home Assistant"));
        assert!(home.first_party && home.refresh && home.device);
        assert!(home.redirect_uris.is_empty());
        assert_eq!(home.scope_cap.as_deref(), Some("library play activity"));
        assert!(matches!(redirect_kind(home, "app.pley.android:/oauth/callback"), Err(ClientError::Redirect(_))));
        let json = include_str!("builtin/homeassistant.json");
        assert!(parse_document(HOMEASSISTANT_ID, json.as_bytes(), false).is_err());

        assert_eq!(BUILTIN_IDS.len(), 10);
        assert!(BUILTIN_IDS.iter().all(|id| builtin(id).is_some_and(|c| c.id == *id)));
        for near in [
            "https://pley-media-server.github.io/pley/oauth/android.json/",
            "https://pley-media-server.github.io/pley/oauth/Android.json",
            "https://PLEY-MEDIA-SERVER.github.io/pley/oauth/android.json",
            "http://pley-media-server.github.io/pley/oauth/android.json",
            "https://pley-media-server.github.io/pley/oauth/android.json?",
            "https://pley-media-server.github.io/pley/oauth/android.json#",
            " https://pley-media-server.github.io/pley/oauth/android.json",
            "https://pley-media-server.github.io/pley/oauth/apple.json",
            "https://pley-media-server.github.io/pley/oauth/iOS.json",
            "",
        ] {
            assert!(builtin(near).is_none(), "{near:?} isn't built in");
        }

        assert!(is_reserved_host("pley-media-server.github.io"));
        assert!(is_reserved_host("Pley-Media-Server.GitHub.io"));
        assert!(is_reserved_host("pley-media-server.github.io."));
        assert!(!is_reserved_host("other.github.io"));
        assert!(!is_reserved_host("github.io"));
        assert!(!is_reserved_host("apps.example.com"));
    }

    #[test]
    fn every_error_says_what_to_do_next() {
        let errors = [
            ClientError::NotAllowed,
            ClientError::Off,
            ClientError::ClientId("why"),
            ClientError::Reserved,
            ClientError::Document("why".into()),
            ClientError::Unreachable("why".into()),
            ClientError::Private,
            ClientError::Busy,
            ClientError::Redirect("why"),
        ];
        for error in errors {
            let message = error.to_string();
            assert!(message.starts_with(|c: char| c.is_lowercase()) || message.starts_with("Pley "), "{message}");
            assert!(message.contains(": "), "{message}");
            assert!(!message.ends_with('.'), "{message}");
        }
        assert_eq!(
            ClientError::NotAllowed.to_string(),
            "only Pley's own apps can sign in to this server: an administrator can allow apps by other publishers in \
             Settings → Server → Apps"
        );
    }
}
