//! HTTP for the app: blocking requests with timeouts and cancellation, the
//! bearer token for the server's own origin only, one refresh-and-retry on
//! `invalid_token`, and Pley's error body read into [`ApiError`].
//!
//! The network itself is a [`Transport`], so tests (and the simulator's fake
//! server) can answer requests without sockets. [`NetTransport`] is the real
//! one: HTTP/1.1 over TCP, TLS through rustls for servers behind an https
//! proxy, kept-alive connections, gzip answers.

mod errors;
mod net;

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use errors::{ApiError, NetError, sentence};
pub use net::NetTransport;

use crate::addresses;
use crate::task::Cancel;

/// What a request may take when it doesn't say (the TV app's 20 s).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a connection to a home-network server may take: it accepts at
/// once or never (the Android app's 3 s; Tailscale addresses excluded).
pub const HOME_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a connection to anything else may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The most of an answer [`Http::send`] keeps in memory; bigger ones are refused.
pub const MAX_BODY: u64 = 64 << 20;
/// The answer header that says a subtitle answer is partial (`0`) or whole.
pub const COMPLETE_HEADER: &str = "x-pley-complete";

/// An HTTP method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    /// The method as the request line writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// A request. Built with [`Request::new`] and its setters; it is bound to the
/// running task's [`Cancel`] when there is one.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: Method,
    /// Absolute: `http://192.168.1.20:7539/api/items/1`.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// The whole exchange, body included; none: only `read_timeout` limits it.
    pub timeout: Option<Duration>,
    /// The longest wait for any one read (an event stream's keep-alives).
    pub read_timeout: Option<Duration>,
    /// None: [`HOME_CONNECT_TIMEOUT`] for home-network hosts, [`CONNECT_TIMEOUT`] otherwise.
    pub connect_timeout: Option<Duration>,
    pub cancel: Option<Cancel>,
}

impl Request {
    /// A request with the default timeout, cancelled with the running task.
    pub fn new(method: Method, url: impl Into<String>) -> Request {
        Request {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout: Some(DEFAULT_TIMEOUT),
            read_timeout: None,
            connect_timeout: None,
            cancel: Cancel::current(),
        }
    }

    /// A `GET` of `url`.
    pub fn get(url: impl Into<String>) -> Request {
        Request::new(Method::Get, url)
    }

    /// Adds a header (replacing one of the same name).
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Request {
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self.headers.push((name.to_string(), value.into()));
        self
    }

    /// A JSON body.
    pub fn json<T: Serialize + ?Sized>(self, body: &T) -> Request {
        let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"null".to_vec());
        Request { body: Some(bytes), ..self.header("Content-Type", "application/json") }
    }

    /// An `application/x-www-form-urlencoded` body of `fields`, in order.
    pub fn form(self, fields: &[(&str, &str)]) -> Request {
        Request {
            body: Some(form(fields).into_bytes()),
            ..self.header("Content-Type", "application/x-www-form-urlencoded")
        }
    }

    /// Limits the whole exchange to `timeout`.
    pub fn timeout(mut self, timeout: Duration) -> Request {
        self.timeout = Some(timeout);
        self
    }

    /// No limit on the whole exchange: a stream read for as long as it runs.
    pub fn streaming(mut self, read_timeout: Duration) -> Request {
        self.timeout = None;
        self.read_timeout = Some(read_timeout);
        self
    }

    /// Limits connecting to `timeout`.
    pub fn connect_timeout(mut self, timeout: Duration) -> Request {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Cancels the request with `cancel` instead of the running task's token.
    pub fn cancel(mut self, cancel: Option<Cancel>) -> Request {
        self.cancel = cancel;
        self
    }

    /// A header's value, case-insensitively.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The connect timeout this request gets.
    pub fn effective_connect_timeout(&self) -> Duration {
        self.connect_timeout.unwrap_or(if addresses::at_home(&self.url) {
            HOME_CONNECT_TIMEOUT
        } else {
            CONNECT_TIMEOUT
        })
    }
}

/// A whole answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

impl Response {
    /// A JSON answer with `status`, for fake transports.
    pub fn json<T: Serialize + ?Sized>(status: u16, body: &T) -> Response {
        Response {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: serde_json::to_vec(body).unwrap_or_default(),
        }
    }

    /// An answer with `status` and no body.
    pub fn empty(status: u16) -> Response {
        Response { status, ..Response::default() }
    }

    /// A header's value, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// Whether the status is 2xx.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body read as JSON.
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, ApiError> {
        serde_json::from_slice(&self.body).map_err(|e| ApiError::Unreadable(e.to_string()))
    }

    /// The body as text (invalid UTF-8 replaced).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Whether a subtitle answer is whole (`X-Pley-Complete` absent or not `0`).
    pub fn complete(&self) -> bool {
        self.header(COMPLETE_HEADER).is_none_or(|v| v.trim() != "0")
    }
}

/// An answer whose body is read as it arrives: the event stream, a speed
/// test, picture subtitles.
pub struct Streamed {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn Read + Send>,
}

impl std::fmt::Debug for Streamed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Streamed").field("status", &self.status).field("headers", &self.headers).finish()
    }
}

impl Streamed {
    /// A header's value, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// Whether the status is 2xx.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The whole body, refused past `limit` bytes.
    pub fn read_all(self, limit: u64) -> Result<Response, NetError> {
        let mut body = Vec::new();
        let read = self.body.take(limit + 1).read_to_end(&mut body).map_err(|e| NetError::from_io(&e));
        read?;
        if body.len() as u64 > limit {
            return Err(NetError::TooBig);
        }
        Ok(Response { status: self.status, headers: self.headers, body })
    }
}

/// The network as requests see it. Implementations answer `open` with the
/// status and headers once they have arrived; the body is read from there.
pub trait Transport: Send + Sync {
    /// Sends `req` and answers once the status and headers are in.
    fn open(&self, req: Request) -> Result<Streamed, NetError>;
}

/// The app's HTTP client: a transport, the app's `User-Agent`, gzip answers.
/// Cheap to clone.
#[derive(Clone)]
pub struct Http {
    transport: Arc<dyn Transport>,
    user_agent: Arc<str>,
}

impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http").field("user_agent", &self.user_agent).finish()
    }
}

impl Http {
    /// A client over `transport` that names itself `user_agent`
    /// (`pleyLG/0.2.0 (webOS 23; OLED65C3PUA)`, see [`crate::device::user_agent`]).
    pub fn new(transport: Arc<dyn Transport>, user_agent: &str) -> Http {
        Http { transport, user_agent: user_agent.into() }
    }

    /// A client over the real network.
    pub fn network(user_agent: &str) -> Http {
        Http::new(Arc::new(NetTransport::new()), user_agent)
    }

    /// The `User-Agent` every request carries.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// Sends `req`; the answer once its headers are in, the body read as it comes.
    pub fn open(&self, mut req: Request) -> Result<Streamed, NetError> {
        if req.header_value("User-Agent").is_none() {
            req.headers.push(("User-Agent".into(), self.user_agent.to_string()));
        }
        if req.header_value("Accept-Encoding").is_none() {
            req.headers.push(("Accept-Encoding".into(), "gzip".into()));
        }
        // The language shown, so what the server writes itself (Home's rows) comes in it.
        if req.header_value("Accept-Language").is_none() {
            req.headers.push(("Accept-Language".into(), pley_i18n::language().code().into()));
        }
        let mut answer = self.transport.open(req)?;
        let gzip = answer.header("Content-Encoding").is_some_and(|e| e.trim().eq_ignore_ascii_case("gzip"));
        if gzip {
            answer.headers.retain(|(n, _)| {
                !n.eq_ignore_ascii_case("Content-Encoding") && !n.eq_ignore_ascii_case("Content-Length")
            });
            answer.body = Box::new(flate2::read::GzDecoder::new(answer.body));
        }
        Ok(answer)
    }

    /// Sends `req` and reads the whole answer (at most [`MAX_BODY`]).
    pub fn send(&self, req: Request) -> Result<Response, NetError> {
        self.open(req)?.read_all(MAX_BODY)
    }
}

/// Where the API gets its access token. The sign-in's tokens implement it.
pub trait TokenSource: Send + Sync {
    /// The access token to send, refreshed first when it ends within a minute;
    /// [`ApiError::SignedOut`] without a sign-in.
    fn access_token(&self) -> Result<String, ApiError>;
    /// A new access token in place of `stale`, which the server refused.
    fn refresh(&self, stale: &str) -> Result<String, ApiError>;
    /// Whether the server refused `sent` just after a refresh issued it: the
    /// sign-in is then over (and forgotten), since another refresh won't help.
    fn refused_after_refresh(&self, sent: &str) -> bool;
    /// A request to the server got no answer: the TV may be on another
    /// network now, so what answers at the address next proves it is the
    /// server before a token goes. Nothing to do for a source without proofs.
    fn doubt_server(&self) {}
}

/// Requests to one server as one sign-in: the bearer token on the server's
/// own origin only (never on an address it hands out elsewhere: SECURITY.md,
/// the LG app's 0.1.0 fix), refreshed once when refused.
#[derive(Clone)]
pub struct Authorized {
    http: Http,
    origin: Arc<str>,
    tokens: Arc<dyn TokenSource>,
}

impl Authorized {
    /// `server` is the server's origin (or any address on it).
    pub fn new(http: Http, server: &str, tokens: Arc<dyn TokenSource>) -> Authorized {
        let origin = addresses::origin(server).unwrap_or_else(|| server.to_string());
        Authorized { http, origin: origin.into(), tokens }
    }

    /// The server's origin: the only one the token goes to.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The client the requests go through.
    pub fn http(&self) -> &Http {
        &self.http
    }

    /// Where the access token comes from.
    pub fn tokens(&self) -> &Arc<dyn TokenSource> {
        &self.tokens
    }

    /// Whether `url` is on this server: only then does it carry the token.
    pub fn own(&self, url: &str) -> bool {
        addresses::origin(url).as_deref() == Some(&*self.origin)
    }

    /// Sends `req` and reads the whole answer; anything but 2xx is an error.
    pub fn send(&self, req: Request) -> Result<Response, ApiError> {
        let own = self.own(&req.url);
        let answer = self.open(req)?;
        answer.read_all(MAX_BODY).map_err(|e| {
            // The answer broke off: as unanswered as no answer at all.
            if own && !matches!(e, NetError::Cancelled | NetError::TooBig) {
                self.tokens.doubt_server();
            }
            ApiError::Net(e)
        })
    }

    /// Sends `req`; the answer once its headers are in. Anything but 2xx is
    /// an error, its body read for Pley's `{code, message}`.
    pub fn open(&self, req: Request) -> Result<Streamed, ApiError> {
        let answer = self.exchange(req)?;
        if answer.ok() {
            return Ok(answer);
        }
        let status = answer.status;
        let whole = answer.read_all(1 << 20).unwrap_or(Response { status, ..Response::default() });
        Err(ApiError::from_response(&whole))
    }

    /// Sends `req` with the token when it is for this server, refreshing it
    /// once when refused; the answer whatever its status (redirects included).
    /// The token waits for the server to prove itself at its address.
    pub fn exchange(&self, req: Request) -> Result<Streamed, ApiError> {
        if !self.own(&req.url) {
            return self.http.open(req).map_err(ApiError::Net);
        }
        let answer = self.exchange_own(req);
        // Out of reach, the TV may be on another network now: what answers
        // at this address next proves it is the server before a token goes.
        if let Err(ApiError::Net(e)) = &answer
            && *e != NetError::Cancelled
        {
            self.tokens.doubt_server();
        }
        answer
    }

    fn exchange_own(&self, req: Request) -> Result<Streamed, ApiError> {
        let token = self.tokens.access_token()?;
        let first = self.http.open(with_token(req.clone(), &token)).map_err(ApiError::Net)?;
        if first.status == 401 && invalid_token(&first) {
            drop(first);
            if self.tokens.refused_after_refresh(&token) {
                return Err(ApiError::SignedOut);
            }
            let fresh = self.tokens.refresh(&token)?;
            return self.http.open(with_token(req, &fresh)).map_err(ApiError::Net);
        }
        Ok(first)
    }
}

fn with_token(req: Request, token: &str) -> Request {
    req.header("Authorization", format!("Bearer {token}"))
}

/// A 401 that is about the token: `WWW-Authenticate` naming `invalid_token`,
/// or none at all (an older server).
pub(crate) fn invalid_token(answer: &Streamed) -> bool {
    answer.header("WWW-Authenticate").is_none_or(|c| c.trim().is_empty() || c.contains("invalid_token"))
}

/// Whether a byte stays as it is in `encodeURIComponent`.
fn unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b)
}

/// `text` percent-encoded as JavaScript's `encodeURIComponent` does.
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if unreserved(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `path` with the query added in order, values left out when none (the
/// server's listing ETag hashes the query as written).
pub fn with_query(path: &str, query: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = query
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| format!("{}={}", encode_component(k), encode_component(v))))
        .collect();
    if parts.is_empty() {
        return path.to_string();
    }
    format!("{path}{}{}", if path.contains('?') { '&' } else { '?' }, parts.join("&"))
}

/// `application/x-www-form-urlencoded` of `fields`, in order.
pub fn form(fields: &[(&str, &str)]) -> String {
    fields.iter().map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v))).collect::<Vec<_>>().join("&")
}

type Answerer = dyn Fn(&Request) -> Result<Response, NetError> + Send + Sync;

/// A transport that answers from a function, recording every request: for
/// tests and headless runs.
pub struct FakeTransport {
    answer: Box<Answerer>,
    seen: Mutex<Vec<Request>>,
}

impl FakeTransport {
    /// A transport whose answers `answer` gives.
    pub fn new(answer: impl Fn(&Request) -> Result<Response, NetError> + Send + Sync + 'static) -> FakeTransport {
        FakeTransport { answer: Box::new(answer), seen: Mutex::new(Vec::new()) }
    }

    /// Every request made so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// `METHOD path` of every request so far (`GET /api/hubs/home`).
    pub fn asked(&self) -> Vec<String> {
        self.requests()
            .iter()
            .map(|r| {
                let path = addresses::Url::parse(&r.url).map_or(r.url.clone(), |u| u.path);
                format!("{} {}", r.method.as_str(), path)
            })
            .collect()
    }
}

impl Transport for FakeTransport {
    fn open(&self, req: Request) -> Result<Streamed, NetError> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).push(req.clone());
        if req.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(NetError::Cancelled);
        }
        let r = (self.answer)(&req)?;
        Ok(Streamed { status: r.status, headers: r.headers, body: Box::new(std::io::Cursor::new(r.body)) })
    }
}

#[cfg(test)]
mod tests;
