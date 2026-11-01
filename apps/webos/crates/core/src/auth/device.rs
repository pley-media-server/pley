//! Signing a TV in with a code (RFC 8628), as the Android TV app and the web
//! TV app do it (packages/tv/src/auth/device.ts, the link screen's rules):
//! a code from the server's device endpoint, shown with its address and a
//! QR code, then the token endpoint asked at the server's interval until
//! someone approves on a phone. Network failures back off 2, 4, 8, 16 and
//! 30 s; an expired code is renewed silently up to three times.

use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use pley_base::Waker;

use super::tokens::{StoredTokens, parse_token_answer};
use super::{AuthError, CLIENT_ID, DEVICE_CODE_GRANT, NO_CODES, SCOPES, ServerMetadata, fetch_metadata, post_form};
use crate::addresses::{UNENCRYPTED, Url, origin, unencrypted};
use crate::http::{Http, NetError};
use crate::model::DeviceAuthorization;
use crate::task::{Cancel, Clock, SystemClock};

/// RFC 8628 §3.2: without an interval, 5 seconds.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);
/// What a `slow_down` adds to this wait and every later one (§3.5).
pub const SLOW_DOWN: Duration = Duration::from_secs(5);
/// The waits after network failures in a row.
pub const BACKOFF: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
    Duration::from_secs(30),
];
/// How many times an expired code is replaced without asking (about 40 minutes of codes).
pub const RENEWALS: u32 = 3;

/// A code for signing in a TV, approved on a phone or computer at the
/// server's `/link` page.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub server: String,
    pub issuer: String,
    /// What the TV polls with. Never shown, never logged.
    pub device_code: String,
    /// As sent: `BCDF-GHJK`.
    pub user_code: String,
    pub verification_uri: String,
    /// Only kept when it is on the server's own origin.
    pub verification_uri_complete: Option<String>,
    /// When the code stops working, in ms since 1970.
    pub expires_at: u64,
    pub interval: Duration,
    pub token_endpoint: String,
    pub revocation_endpoint: String,
    /// What the TV asked for.
    pub scope: String,
}

impl fmt::Debug for DeviceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceCode")
            .field("issuer", &self.issuer)
            .field("user_code", &self.user_code)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl DeviceCode {
    /// What the QR code holds: the address with the code in it, else the page.
    pub fn qr_address(&self) -> &str {
        self.verification_uri_complete.as_deref().unwrap_or(&self.verification_uri)
    }

    /// The code as shown: `BCDF-GHJK`.
    pub fn shown_code(&self) -> String {
        display_code(&self.user_code)
    }

    /// The page as shown: `192.168.1.20:7539/link`.
    pub fn address(&self) -> String {
        verification_address(&self.verification_uri)
    }
}

/// A code as shown: upper case, letters and digits only, eight of them as
/// `XXXX-XXXX`; anything else as sent.
pub fn display_code(code: &str) -> String {
    let clean: String = code.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect();
    if clean.len() == 8 { format!("{}-{}", &clean[..4], &clean[4..]) } else { code.to_string() }
}

/// A page's address without its scheme, with its port and path
/// (`192.168.1.20:7539/link`).
pub fn verification_address(uri: &str) -> String {
    match Url::parse(uri) {
        Some(u) => {
            let host = match u.port {
                Some(p) => format!("{}:{p}", u.host),
                None => u.host,
            };
            let path = if u.path == "/" { String::new() } else { u.path };
            format!("{host}{path}")
        }
        None => uri.to_string(),
    }
}

/// The scope to ask for: what the server offers of Pley's own.
pub fn scope_for(metadata: &ServerMetadata) -> String {
    let wanted = SCOPES.iter().filter(|s| metadata.scopes_supported.as_ref().is_none_or(|o| o.iter().any(|x| x == *s)));
    wanted.copied().collect::<Vec<_>>().join(" ")
}

/// Where a code stands while it is polled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevicePoll {
    Waiting,
    SignedIn(StoredTokens),
    Denied,
    Expired,
    /// The server can't be reached: polling goes on by itself after `retry_in`.
    Unreachable {
        retry_in: Duration,
    },
    Failed(String),
}

/// What the code screen shows, as the sign-in goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// "Getting a code…"
    Starting,
    /// A code to show (a new one when the last expired).
    Code(DeviceCode),
    /// "Waiting for approval…"
    Waiting,
    /// "Can't reach the server. Trying again in N s"
    Unreachable { retry_in: Duration },
    /// "Request declined"
    Declined,
    /// "Code expired: press OK for a new one"
    Expired,
    /// The error's sentence; only "Change server" is offered.
    Failed(String),
    /// Approved: the tokens to install.
    SignedIn(StoredTokens),
}

/// How a sign-in ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkEnd {
    SignedIn(StoredTokens),
    Declined,
    Expired,
    Failed(String),
    /// Stopped (the screen went away): the code to poll again on coming back, while it works.
    Stopped(Option<DeviceCode>),
}

/// The device authorization grant against a Pley server, as Pley for LG TV.
pub struct DeviceSignIn {
    http: Http,
    client_id: String,
    clock: Arc<dyn Clock>,
}

impl DeviceSignIn {
    /// The flow over `http`, on the real clock.
    pub fn new(http: Http) -> DeviceSignIn {
        DeviceSignIn::with_clock(http, Arc::new(SystemClock))
    }

    /// With `clock` deciding time and waits (tests).
    pub fn with_clock(http: Http, clock: Arc<dyn Clock>) -> DeviceSignIn {
        DeviceSignIn { http, client_id: CLIENT_ID.into(), clock }
    }

    /// A new code from `server`.
    pub fn start(&self, server: &str) -> Result<DeviceCode, AuthError> {
        let home = origin(server).ok_or_else(|| AuthError::Failed("that isn't a server address".into()))?;
        if unencrypted(&home) {
            return Err(AuthError::Failed(UNENCRYPTED.into()));
        }
        let metadata = fetch_metadata(&self.http, &home)?;
        let Some(endpoint) = metadata.device_authorization_endpoint.clone().filter(|_| metadata.signs_in_with_codes())
        else {
            return Err(AuthError::Failed(NO_CODES.into()));
        };
        let scope = scope_for(&metadata);
        let answer = match post_form(&self.http, &endpoint, &[("client_id", &self.client_id), ("scope", &scope)]) {
            Err(e) if matches!(e.code(), Some("invalid_client" | "unauthorized_client")) => {
                // A server from before Pley's TV apps were built in.
                return Err(AuthError::Failed("this server doesn't know Pley's TV apps yet: update it".into()));
            }
            Err(e) if e.status() == Some(429) => {
                return Err(AuthError::Failed(
                    "too many TVs are signing in here right now: try again in a minute".into(),
                ));
            }
            Err(e) => return Err(e),
            Ok(a) => a,
        };
        parse_device_code(&answer.body, &home, &metadata, &scope, self.clock.now_ms())
    }

    /// Asks whether `code` was approved, every interval (slower when the
    /// server says so), until it is signed in, denied, expired or failed;
    /// `on` hears each change. Answers [`DevicePoll::Waiting`] when cancelled.
    pub fn poll(&self, code: &DeviceCode, on: &mut dyn FnMut(&DevicePoll), cancel: &Cancel) -> DevicePoll {
        let mut interval = if code.interval.is_zero() { DEFAULT_INTERVAL } else { code.interval };
        let mut failures = 0usize;
        let mut troubled = false;
        on(&DevicePoll::Waiting);
        let backoff = |failures: usize, interval: Duration| interval.max(BACKOFF[failures.min(BACKOFF.len()) - 1]);
        loop {
            let wait = if failures == 0 { interval } else { backoff(failures, interval) };
            let left = Duration::from_millis(code.expires_at.saturating_sub(self.clock.now_ms()));
            if !self.clock.sleep(wait.min(left), cancel) || cancel.is_cancelled() {
                return DevicePoll::Waiting;
            }
            let outcome = if self.clock.now_ms() >= code.expires_at {
                Some(DevicePoll::Expired)
            } else {
                let fields = [
                    ("grant_type", DEVICE_CODE_GRANT),
                    ("device_code", code.device_code.as_str()),
                    ("client_id", self.client_id.as_str()),
                ];
                match cancel.scope(|| post_form(&self.http, &code.token_endpoint, &fields)) {
                    Ok(answer) => {
                        failures = 0;
                        Some(match parse_token_answer(&answer.body, &code.scope) {
                            Ok(parsed) => DevicePoll::SignedIn(StoredTokens::new(
                                &parsed,
                                &code.issuer,
                                &code.token_endpoint,
                                &code.revocation_endpoint,
                                self.clock.now_ms(),
                            )),
                            Err(e) => DevicePoll::Failed(e.to_string()),
                        })
                    }
                    Err(AuthError::Unreachable(NetError::Cancelled)) => return DevicePoll::Waiting,
                    Err(AuthError::Unreachable(_)) => {
                        failures += 1;
                        None
                    }
                    Err(e @ AuthError::Refused { .. }) => {
                        failures = 0;
                        match e.code() {
                            Some("authorization_pending") => None,
                            Some("slow_down") => {
                                interval += SLOW_DOWN;
                                None
                            }
                            Some("access_denied") => Some(DevicePoll::Denied),
                            Some("expired_token") => Some(DevicePoll::Expired),
                            Some("invalid_grant") => {
                                Some(DevicePoll::Failed("this code doesn't work anymore: get a new one".into()))
                            }
                            _ => Some(DevicePoll::Failed(e.to_string())),
                        }
                    }
                    Err(e) => Some(DevicePoll::Failed(e.to_string())),
                }
            };
            if cancel.is_cancelled() {
                return DevicePoll::Waiting;
            }
            if let Some(outcome) = outcome {
                on(&outcome);
                return outcome;
            }
            if failures > 0 {
                troubled = true;
                on(&DevicePoll::Unreachable { retry_in: backoff(failures, interval) });
            } else if troubled {
                troubled = false;
                on(&DevicePoll::Waiting);
            }
        }
    }

    /// The whole sign-in, as the code screen runs it: a code (fetched again
    /// by itself while the server can't be reached), polled until approved;
    /// an expired code replaced silently up to [`RENEWALS`] times. `resume`
    /// is a code from an earlier visit, polled again while it works.
    pub fn run(
        &self,
        server: &str,
        resume: Option<DeviceCode>,
        on: &mut dyn FnMut(LinkState),
        cancel: &Cancel,
    ) -> LinkEnd {
        let mut renewals = 0;
        let mut code = resume.filter(|c| self.clock.now_ms() < c.expires_at);
        let mut first = true;
        loop {
            if cancel.is_cancelled() {
                return LinkEnd::Stopped(code);
            }
            let current = match code.take() {
                Some(c) => c,
                None => {
                    if first {
                        on(LinkState::Starting);
                    }
                    match self.fetch(server, on, cancel) {
                        Ok(c) => c,
                        Err(end) => return *end,
                    }
                }
            };
            first = false;
            on(LinkState::Code(current.clone()));
            let polled = self.poll(
                &current,
                &mut |p| match p {
                    DevicePoll::Waiting => on(LinkState::Waiting),
                    DevicePoll::Unreachable { retry_in } => on(LinkState::Unreachable { retry_in: *retry_in }),
                    _ => {}
                },
                cancel,
            );
            match polled {
                DevicePoll::SignedIn(tokens) => {
                    on(LinkState::SignedIn(tokens.clone()));
                    return LinkEnd::SignedIn(tokens);
                }
                DevicePoll::Denied => {
                    on(LinkState::Declined);
                    return LinkEnd::Declined;
                }
                DevicePoll::Expired if renewals < RENEWALS => renewals += 1,
                DevicePoll::Expired => {
                    on(LinkState::Expired);
                    return LinkEnd::Expired;
                }
                DevicePoll::Failed(message) => {
                    on(LinkState::Failed(message.clone()));
                    return LinkEnd::Failed(message);
                }
                DevicePoll::Waiting | DevicePoll::Unreachable { .. } => return LinkEnd::Stopped(Some(current)),
            }
        }
    }

    /// [`DeviceSignIn::run`] on a thread of its own (a sign-in lasts minutes:
    /// it would hold a task thread all that time), its states sent to `tx`
    /// with a wake of the main loop. Cancelling the token stops it; the
    /// thread's answer is how it ended.
    pub fn run_on_thread(
        self,
        server: String,
        resume: Option<DeviceCode>,
        tx: Sender<LinkState>,
        waker: Waker,
    ) -> (Cancel, JoinHandle<LinkEnd>) {
        let cancel = Cancel::new();
        let c = cancel.clone();
        let handle = std::thread::Builder::new()
            .name("pley-sign-in".into())
            .spawn(move || {
                let mut tell = |state: LinkState| {
                    if !c.is_cancelled() && tx.send(state).is_ok() {
                        waker.wake();
                    }
                };
                self.run(&server, resume, &mut tell, &c)
            })
            .expect("the sign-in thread starts");
        (cancel, handle)
    }

    /// A code, asked again after 2, 4, 8, 16 and 30 s while the server can't
    /// be reached; a sign-in error is final.
    fn fetch(&self, server: &str, on: &mut dyn FnMut(LinkState), cancel: &Cancel) -> Result<DeviceCode, Box<LinkEnd>> {
        let mut failures = 0;
        loop {
            match cancel.scope(|| self.start(server)) {
                Ok(c) => return Ok(c),
                Err(AuthError::Unreachable(NetError::Cancelled)) => return Err(Box::new(LinkEnd::Stopped(None))),
                Err(e) if e.unreachable() => {
                    failures += 1;
                    let wait = BACKOFF[(failures as usize).min(BACKOFF.len()) - 1];
                    on(LinkState::Unreachable { retry_in: wait });
                    if !self.clock.sleep(wait, cancel) {
                        return Err(Box::new(LinkEnd::Stopped(None)));
                    }
                }
                Err(e) => {
                    let message = e.to_string();
                    on(LinkState::Failed(message.clone()));
                    return Err(Box::new(LinkEnd::Failed(message)));
                }
            }
        }
    }
}

/// The code in `body`, the answer of the device endpoint of `server` (§3.2).
fn parse_device_code(
    body: &[u8],
    server: &str,
    metadata: &ServerMetadata,
    scope: &str,
    now_ms: u64,
) -> Result<DeviceCode, AuthError> {
    let bad = || AuthError::Failed("the server's answer can't be read: try again".into());
    let a: DeviceAuthorization = serde_json::from_slice(body).map_err(|_| bad())?;
    if a.device_code.is_empty() || a.user_code.is_empty() || a.verification_uri.is_empty() {
        return Err(bad());
    }
    // The page to open must be the server's own: a code typed elsewhere would sign someone else's TV in.
    if origin(&a.verification_uri).as_deref() != Some(metadata.issuer.as_str()) {
        return Err(AuthError::Failed(
            "this server sends people to another address to approve TVs: Pley doesn't".into(),
        ));
    }
    let complete = a.verification_uri_complete.filter(|u| origin(u).as_deref() == Some(metadata.issuer.as_str()));
    let expires_in = a.expires_in.filter(|e| *e > 0.0).unwrap_or(600.0);
    let interval = a.interval.filter(|i| *i > 0.0).map_or(DEFAULT_INTERVAL, Duration::from_secs_f64);
    Ok(DeviceCode {
        server: server.into(),
        issuer: metadata.issuer.clone(),
        device_code: a.device_code,
        user_code: a.user_code,
        verification_uri: a.verification_uri,
        verification_uri_complete: complete,
        expires_at: now_ms + (expires_in * 1000.0) as u64,
        interval,
        token_endpoint: metadata.token_endpoint.clone(),
        revocation_endpoint: metadata.revocation_endpoint.clone(),
        scope: scope.into(),
    })
}
