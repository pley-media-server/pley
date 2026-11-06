//! What every route shares: the state behind one lock, the token check, the
//! journal, the answers' common headers, and the routing table.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::catalog::Catalog;
use crate::events::EventHub;
use crate::extra::NetworkMemory;
use crate::household::{Household, Profile};
use crate::http::{App, Body, Pace, Request, Response, lock};
use crate::live::Live;
use crate::media::Media;
use crate::oauth::OAuth;
use crate::pictures::Pictures;
use crate::play::{AddedSubtitle, Playback};
use crate::requests::Requests;
use crate::{JournalEntry, Options};

/// The scopes an app may be allowed, as bits, in the order the server writes
/// them.
pub(crate) const LIBRARY: u8 = 1;
pub(crate) const PLAY: u8 = 2;
pub(crate) const PLAYLISTS: u8 = 4;
pub(crate) const PROFILES: u8 = 8;
pub(crate) const REQUESTS: u8 = 16;
pub(crate) const APPROVALS: u8 = 32;
pub(crate) const ALL_SCOPES: u8 = LIBRARY | PLAY | PLAYLISTS | PROFILES | REQUESTS | APPROVALS;
pub(crate) const SCOPES: [(&str, u8); 6] = [
    ("library", LIBRARY),
    ("play", PLAY),
    ("playlists", PLAYLISTS),
    ("profiles", PROFILES),
    ("requests", REQUESTS),
    ("approvals", APPROVALS),
];

/// The scope names of `bits`, space separated.
pub(crate) fn scope_names(bits: u8) -> String {
    SCOPES.iter().filter(|(_, b)| bits & b != 0).map(|(n, _)| *n).collect::<Vec<_>>().join(" ")
}

/// What a route needs of the caller's token (docs/API.md, "Scopes").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Need {
    /// Nothing: no token.
    Public,
    /// Any app's token.
    AnyApp,
    Scope(u8),
    /// Pley's web app only: apps' tokens are refused.
    WebOnly,
}

/// Who asks: the profile the token acts as and what it was allowed.
#[derive(Clone, Debug)]
pub(crate) struct Caller {
    pub profile: i64,
    pub scopes: u8,
    /// The sign-in the token belongs to; `None` without one.
    pub grant: Option<u64>,
    /// When the access token ends, in ms; `None` without one.
    pub expires_at_ms: Option<i64>,
}

impl Caller {
    pub fn scope(&self) -> String {
        scope_names(self.scopes)
    }
}

/// A route's answer; an error is an answer too, so `?` can return it.
pub(crate) type Answer = Result<Response, Response>;

pub(crate) type LockedFn = fn(&Cx<'_>, &mut State) -> Answer;
pub(crate) type FreeFn = fn(&Cx<'_>) -> Answer;

#[derive(Clone, Copy)]
pub(crate) enum Handler {
    /// Runs holding the state's lock.
    Locked(LockedFn),
    /// Runs without it: pictures and media, which take long to make or send.
    Free(FreeFn),
}

/// One route: a method and a path whose `*` parts are its values.
pub(crate) struct Route {
    pub method: &'static str,
    pub pattern: Vec<&'static str>,
    pub need: Need,
    pub handler: Handler,
}

impl Route {
    fn matches(&self, segments: &[String]) -> Option<Vec<String>> {
        if self.pattern.len() != segments.len() {
            return None;
        }
        let mut values = Vec::new();
        for (want, got) in self.pattern.iter().zip(segments) {
            if *want == "*" {
                values.push(got.clone());
            } else if want != got {
                return None;
            }
        }
        Some(values)
    }
}

fn split(path: &'static str) -> Vec<&'static str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// A route run under the state's lock.
pub(crate) fn locked(method: &'static str, path: &'static str, need: Need, f: LockedFn) -> Route {
    Route { method, pattern: split(path), need, handler: Handler::Locked(f) }
}

/// A route run without the state's lock.
pub(crate) fn free(method: &'static str, path: &'static str, need: Need, f: FreeFn) -> Route {
    Route { method, pattern: split(path), need, handler: Handler::Free(f) }
}

/// Everything the fake server knows that changes, behind one lock.
pub(crate) struct State {
    pub household: Household,
    pub oauth: OAuth,
    pub playback: Playback,
    pub live: Live,
    pub network: NetworkMemory,
    /// Requests and Discover.
    pub requests: Requests,
    /// Subtitle search is set up.
    pub subtitle_search: bool,
    /// The server requires two-step sign-in of every account with a password.
    pub two_step_required: bool,
    /// The server language `GET /api/server` names (`en` until a test changes it); none, as a
    /// Pley from before languages.
    pub server_language: Option<String>,
    /// What `PUT /api/me/language` answers instead of changing the language, as a server without
    /// the route: 404, or 405.
    pub language_route_refusal: Option<u16>,
    /// How often each text subtitle track was asked for, by (file, index).
    pub subtitle_asks: HashMap<(i64, i64), u32>,
    /// Subtitles added to files, by file.
    pub added_subtitles: HashMap<i64, Vec<AddedSubtitle>>,
    /// When casts went to each receiver in the last second, by profile and receiver.
    pub casts_sent: HashMap<(i64, String), Vec<i64>>,
    /// The switch tickets of casts to another profile's receiver, waiting to be spent.
    pub cast_tickets: HashMap<String, crate::cast::Ticket>,
}

impl State {
    pub fn profile(&self, id: i64) -> &Profile {
        self.household.profile(id).expect("callers act as profiles of the household")
    }

    pub fn profile_mut(&mut self, id: i64) -> &mut Profile {
        self.household.profile_mut(id).expect("callers act as profiles of the household")
    }
}

/// What the routes share.
pub(crate) struct Shared {
    pub options: Options,
    pub catalog: Catalog,
    pub media: Media,
    pub pictures: Pictures,
    pub state: Mutex<State>,
    pub events: Arc<EventHub>,
    pub journal: Mutex<Vec<JournalEntry>>,
    pub slow: AtomicBool,
    pub routes: Vec<Route>,
}

impl Shared {
    /// The server's clock, in ms.
    pub fn now_ms(&self) -> i64 {
        self.options.clock.now_ms()
    }

    /// Unix seconds.
    pub fn now(&self) -> i64 {
        self.now_ms().div_euclid(1_000)
    }
}

/// A request being answered, with what the route needs of it.
pub(crate) struct Cx<'a> {
    pub req: &'a Request,
    pub shared: &'a Shared,
    caller: Option<Caller>,
    pub values: Vec<String>,
}

impl Cx<'_> {
    /// The caller of a route that needs a token.
    pub fn caller(&self) -> &Caller {
        self.caller.as_ref().expect("routes with a token have a caller")
    }

    /// The caller, when the request carried a valid token.
    pub fn maybe_caller(&self) -> Option<&Caller> {
        self.caller.as_ref()
    }

    pub fn catalog(&self) -> &Catalog {
        &self.shared.catalog
    }

    pub fn value(&self, n: usize) -> &str {
        &self.values[n]
    }

    /// Path value `n` as an id: the server answers `400` in plain text for a
    /// path part that isn't a number.
    pub fn id(&self, n: usize) -> Result<i64, Response> {
        let value = self.value(n);
        value.parse::<i64>().map_err(|_| {
            Response::bytes(
                400,
                "text/plain; charset=utf-8",
                format!("Invalid URL: Cannot parse `{value}` to a `i64`").into_bytes(),
            )
        })
    }

    pub fn query(&self, name: &str) -> Option<&str> {
        self.req.query(name)
    }

    /// `summaries=1` (or true): lists carry every item's summary.
    pub fn summaries(&self) -> bool {
        matches!(self.query("summaries"), Some("1" | "true"))
    }

    pub fn now(&self) -> i64 {
        self.shared.now()
    }

    pub fn now_ms(&self) -> i64 {
        self.shared.now_ms()
    }
}

/// A JSON answer.
pub(crate) fn json<T: Serialize + ?Sized>(status: u16, value: &T) -> Response {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
    Response::bytes(status, "application/json", body)
}

/// Pley's own refusal: `{ "code", "message" }` (docs/API.md, "Errors").
pub(crate) fn error(status: u16, code: &str, message: &str) -> Response {
    json(status, &serde_json::json!({ "code": code, "message": message }))
}

pub(crate) fn not_found(message: &str) -> Response {
    error(404, "not_found", message)
}

pub(crate) fn bad_request(message: &str) -> Response {
    error(400, "bad_request", message)
}

pub(crate) fn forbidden(message: &str) -> Response {
    error(403, "forbidden", message)
}

pub(crate) fn no_content() -> Response {
    Response::new(204)
}

/// A session stopped from elsewhere: `410` with why.
pub(crate) fn session_stopped(message: Option<&str>) -> Response {
    json(410, &serde_json::json!({ "code": "session_stopped", "error": "session_stopped", "message": message }))
}

impl App for Shared {
    fn answer(&self, request: &Request) -> Response {
        let mut answer = dispatch(self, request);
        finish(request, &mut answer);
        answer
    }

    fn pace(&self) -> Option<Pace> {
        self.slow.load(Ordering::SeqCst).then_some(Pace {
            latency: self.options.slow_latency,
            bytes_per_second: self.options.slow_bytes_per_second,
        })
    }
}

fn dispatch(shared: &Shared, request: &Request) -> Response {
    let method = if request.method == "HEAD" { "GET" } else { request.method.as_str() };
    let mut known_path = false;
    for route in &shared.routes {
        let Some(values) = route.matches(&request.segments) else { continue };
        known_path = true;
        if route.method == method {
            return run(shared, request, route, values);
        }
    }
    if known_path {
        // As axum answers a wrong method: 405, no body.
        Response::new(405)
    } else if request.path.starts_with("/api/") || request.path == "/api" {
        not_found("no such endpoint")
    } else {
        not_found("not found")
    }
}

fn run(shared: &Shared, request: &Request, route: &Route, values: Vec<String>) -> Response {
    let mut caller = None;
    let mut held = None;
    if route.need != Need::Public {
        let state = lock(&shared.state);
        let who = match state.oauth.authenticate(request, shared.now_ms(), shared.options.require_auth) {
            Ok(who) => who,
            Err(refusal) => return refusal,
        };
        match route.need {
            Need::Scope(scope) if who.scopes & scope == 0 => return insufficient(Some(scope)),
            Need::WebOnly => return insufficient(None),
            _ => {}
        }
        let write = request.method != "GET" && request.method != "HEAD";
        if write && request.path.starts_with("/api/") && !request.path.starts_with("/api/oauth/") {
            lock(&shared.journal).push(JournalEntry {
                method: request.method.clone(),
                path: request.path.clone(),
                query: request.raw_query.clone(),
                profile_id: Some(who.profile),
                body: request.text(),
                at_ms: shared.now_ms(),
            });
        }
        caller = Some(who);
        held = Some(state);
    } else if let Some(header) = request.header("authorization")
        && header.starts_with("Bearer ")
    {
        // Public routes that do something more for a signed-in app (sign out).
        let state = lock(&shared.state);
        caller = state.oauth.authenticate(request, shared.now_ms(), true).ok();
        held = Some(state);
    }
    let cx = Cx { req: request, shared, caller, values };
    let answer = match route.handler {
        Handler::Locked(f) => {
            let mut state = held.unwrap_or_else(|| lock(&shared.state));
            f(&cx, &mut state)
        }
        Handler::Free(f) => {
            drop(held);
            f(&cx)
        }
    };
    answer.unwrap_or_else(|refusal| refusal)
}

/// The refusal of a route whose scope the app wasn't given (`None`: a route
/// only Pley's web app may use).
fn insufficient(scope: Option<u8>) -> Response {
    let Some(scope) = scope else {
        return error(403, "insufficient_scope", "apps can't do that: use Pley's web app")
            .with_header("WWW-Authenticate", "Bearer error=\"insufficient_scope\"");
    };
    let (name, words) = match scope {
        LIBRARY => ("library", "see the library"),
        PLAY => ("play", "play"),
        PLAYLISTS => ("playlists", "change playlists"),
        _ => ("profiles", "switch profiles"),
    };
    error(403, "insufficient_scope", &format!("this app wasn't allowed to {words}: sign in to it again to allow it"))
        .with_header("WWW-Authenticate", format!("Bearer error=\"insufficient_scope\", scope=\"{name}\""))
}

/// The headers every answer gets: `no-store` for the API unless the route
/// said otherwise, and gzip for text over 1 KiB when the client takes it, as
/// the server does.
fn finish(request: &Request, answer: &mut Response) {
    if request.path.starts_with("/api/") && answer.header("cache-control").is_none() {
        answer.set_header("Cache-Control", "no-store");
    }
    let takes_gzip = request
        .header("accept-encoding")
        .is_some_and(|v| v.split(',').any(|e| e.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("gzip")));
    let textual = answer.header("content-type").is_some_and(|t| {
        t.starts_with("application/json") || t.starts_with("text/vtt") || t.starts_with("application/vnd.apple.mpegurl")
    });
    if !takes_gzip || !textual || answer.header("content-encoding").is_some() || answer.status == 206 {
        return;
    }
    let Body::Bytes(bytes) = &answer.body else { return };
    if bytes.len() <= 1_024 {
        return;
    }
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    if encoder.write_all(bytes).is_err() {
        return;
    }
    let Ok(compressed) = encoder.finish() else { return };
    answer.body = Body::Bytes(compressed);
    answer.set_header("Content-Encoding", "gzip");
    answer.set_header("Vary", "Accept-Encoding");
}
