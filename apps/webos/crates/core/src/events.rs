//! The server's events (`GET /api/events`): Server-Sent Events read on a
//! thread of their own with the bearer header (packages/tv/src/api/events.ts,
//! the Android app's `EventStream`).
//!
//! Nothing is replayed on reconnecting, so every connection is announced as
//! [`StreamNews::Connected`] and the app asks again for what is older than
//! 30 s. The server ends a stream when its access token expires: a stream
//! that ran 30 s or more reconnects at once (with a refreshed token); any
//! other end waits 2 s, doubling to 30 s; a `401` refreshes the token first;
//! a `403` or the end of the sign-in stops it.

use std::io::Read;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use pley_base::Waker;

use crate::api::Api;
use crate::http::{ApiError, Request};
use crate::model::ServerEvent;
use crate::task::Cancel;

/// The first wait after a stream fails, doubled each time up to the last.
pub const FIRST_RETRY: Duration = Duration::from_secs(2);
/// The longest wait between tries.
pub const MAX_RETRY: Duration = Duration::from_secs(30);
/// A stream that ran this long and then ended ended as planned (its token expired).
pub const STABLE: Duration = Duration::from_secs(30);
/// The longest silence taken: the server says something every 15 s.
pub const READ_TIMEOUT: Duration = Duration::from_secs(45);
/// The longest event taken; a longer one is skipped.
const MAX_EVENT: usize = 1 << 20;

/// What the stream tells the app.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamNews {
    /// A connection opened: nothing missed is replayed, so ask again.
    Connected,
    Event(ServerEvent),
    /// The stream stopped for good: `403` (not allowed) or the sign-in ended.
    Stopped,
    /// The connection failed or ended, and the stream waits before trying again: until
    /// [`StreamNews::Connected`], what the events would say is asked for instead. Told once a
    /// wait.
    Lost,
}

/// Turns the stream's bytes into the data of its events: `data:` lines
/// (one leading space dropped) joined by newlines, an event per blank line.
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    data: String,
    has_data: bool,
    skipping: bool,
}

impl SseParser {
    /// A parser at the start of a stream.
    pub fn new() -> SseParser {
        SseParser::default()
    }

    /// The data of every event `bytes` completes.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b != b'\n' {
                if self.line.len() < MAX_EVENT {
                    self.line.push(b);
                } else {
                    self.skipping = true;
                }
                continue;
            }
            if self.line.last() == Some(&b'\r') {
                self.line.pop();
            }
            let line = std::mem::take(&mut self.line);
            if line.is_empty() {
                if self.has_data && !self.skipping {
                    out.push(std::mem::take(&mut self.data));
                }
                self.data.clear();
                self.has_data = false;
                self.skipping = false;
                continue;
            }
            if let Some(rest) = line.strip_prefix(b"data:") {
                let rest = rest.strip_prefix(b" ").unwrap_or(rest);
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(&String::from_utf8_lossy(rest));
                self.has_data = true;
                if self.data.len() > MAX_EVENT {
                    self.skipping = true;
                    self.data.clear();
                }
            }
        }
        out
    }
}

/// The event `data` holds; none for anything that isn't an object with a `type`.
pub fn parse_event(data: &str) -> Option<ServerEvent> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    v.get("type")?.as_str()?;
    serde_json::from_value(v).ok()
}

/// The event stream of one server and sign-in, read on its own thread while
/// it lives. Dropping it stops the thread.
pub struct EventStream {
    cancel: Cancel,
    nudges: std::sync::Arc<std::sync::Mutex<Cancel>>,
    thread: Option<JoinHandle<()>>,
}

impl EventStream {
    /// Starts reading `api`'s events into `tx`, waking the main loop with
    /// `waker` for every piece of news.
    pub fn start(api: Api, tx: Sender<StreamNews>, waker: Waker) -> EventStream {
        let cancel = Cancel::new();
        let nudges = std::sync::Arc::new(std::sync::Mutex::new(Cancel::new()));
        let (c, n) = (cancel.clone(), nudges.clone());
        let thread = thread::Builder::new()
            .name("pley-events".into())
            .spawn(move || run(&api, &tx, &waker, &c, &n))
            .expect("the event thread starts");
        EventStream { cancel, nudges, thread: Some(thread) }
    }

    /// The server answers again: a stream waiting out its back-off connects now.
    pub fn retry_now(&self) {
        self.nudges.lock().unwrap_or_else(|e| e.into_inner()).cancel();
    }

    /// Stops reading at once. Nothing more is sent; the thread ends by itself
    /// (it may be finishing a token refresh, which is never cut short), so the
    /// caller never waits on it.
    pub fn stop(&mut self) {
        self.cancel.cancel();
        self.retry_now();
        self.thread.take();
    }

    /// Whether the thread has ended (tests, and the app's shutdown).
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Sends `news` unless the stream was stopped; false once nobody listens.
fn tell(tx: &Sender<StreamNews>, waker: &Waker, cancel: &Cancel, news: StreamNews) -> bool {
    if cancel.is_cancelled() {
        return false;
    }
    let sent = tx.send(news).is_ok();
    waker.wake();
    sent
}

/// How one connection ended.
enum End {
    /// Ran and ended (or failed after opening) at `opened`.
    Closed {
        opened: Option<Instant>,
    },
    Unauthorized {
        token_refreshed: bool,
    },
    Forbidden,
    SignedOut,
    Failed,
    Stop,
}

fn run(api: &Api, tx: &Sender<StreamNews>, waker: &Waker, cancel: &Cancel, nudges: &std::sync::Mutex<Cancel>) {
    let mut failures = 0u32;
    let mut lost = false;
    while !cancel.is_cancelled() {
        let end = cancel.scope(|| connect(api, tx, waker, cancel));
        if matches!(end, End::Closed { opened: Some(_) }) {
            lost = false;
        }
        let wait = match end {
            End::Stop => return,
            End::Forbidden | End::SignedOut => {
                tell(tx, waker, cancel, StreamNews::Stopped);
                return;
            }
            End::Unauthorized { token_refreshed: true } => Duration::ZERO,
            End::Closed { opened: Some(at) } if at.elapsed() >= STABLE => {
                failures = 0;
                Duration::ZERO
            }
            End::Closed { opened } => {
                if opened.is_some() {
                    failures = 0;
                }
                failures += 1;
                retry_delay(failures)
            }
            End::Unauthorized { token_refreshed: false } | End::Failed => {
                failures += 1;
                retry_delay(failures)
            }
        };
        if !wait.is_zero() {
            if !lost {
                lost = true;
                tell(tx, waker, cancel, StreamNews::Lost);
            }
            let nudge = {
                let mut n = nudges.lock().unwrap_or_else(|e| e.into_inner());
                if n.is_cancelled() {
                    *n = Cancel::new();
                }
                n.clone()
            };
            // Wakes for a stop or a nudge, whichever comes first.
            let c = cancel.clone();
            let n2 = nudge.clone();
            let _forward = c.on_cancel(move || n2.cancel());
            if !nudge.sleep(wait) {
                failures = 0;
            }
        }
    }
}

/// The wait after `failures` failures in a row: 2 s, doubling, at most 30 s.
pub fn retry_delay(failures: u32) -> Duration {
    let mut wait = FIRST_RETRY;
    for _ in 1..failures {
        wait = (wait * 2).min(MAX_RETRY);
    }
    wait.min(MAX_RETRY)
}

fn connect(api: &Api, tx: &Sender<StreamNews>, waker: &Waker, cancel: &Cancel) -> End {
    let token = match api.tokens().access_token() {
        Ok(t) => t,
        Err(ApiError::SignedOut) => return End::SignedOut,
        Err(_) => return End::Failed,
    };
    let req = Request::get(api.url("/api/events"))
        .header("Accept", "text/event-stream")
        .header("Authorization", format!("Bearer {token}"))
        .streaming(READ_TIMEOUT);
    let answer = match api.http().open(req) {
        Ok(a) => a,
        Err(_) if cancel.is_cancelled() => return End::Stop,
        Err(_) => return End::Failed,
    };
    match answer.status {
        200 => {}
        401 => {
            return match api.tokens().refresh(&token) {
                Ok(_) => End::Unauthorized { token_refreshed: true },
                Err(ApiError::SignedOut) => End::SignedOut,
                Err(_) => End::Unauthorized { token_refreshed: false },
            };
        }
        403 => return End::Forbidden,
        _ => return End::Failed,
    }
    let opened = Instant::now();
    if !tell(tx, waker, cancel, StreamNews::Connected) {
        return End::Stop;
    }
    let mut body = answer.body;
    let mut parser = SseParser::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) | Err(_) => {
                return if cancel.is_cancelled() { End::Stop } else { End::Closed { opened: Some(opened) } };
            }
            Ok(n) => n,
        };
        for data in parser.push(&buf[..n]) {
            if let Some(event) = parse_event(&data)
                && !tell(tx, waker, cancel, StreamNews::Event(event))
            {
                return End::Stop;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::http::{FakeTransport, Http, Response, TokenSource};

    #[test]
    fn events_are_split_on_blank_lines_whatever_the_chunks() {
        let mut p = SseParser::new();
        let mut got = Vec::new();
        let stream = ": keep-alive\n\ndata: {\"type\":\"library_changed\",\"library_id\":2}\r\n\r\ndata:{\"type\":\ndata:  \"watchlist_changed\"}\n\n";
        for chunk in stream.as_bytes().chunks(7) {
            got.extend(p.push(chunk));
        }
        assert_eq!(got.len(), 2);
        assert_eq!(parse_event(&got[0]), Some(ServerEvent::LibraryChanged { library_id: 2 }));
        // One leading space of a `data:` line is dropped, not two.
        assert_eq!(got[1], "{\"type\":\n \"watchlist_changed\"}");
        assert_eq!(parse_event(&got[1]), Some(ServerEvent::WatchlistChanged));
    }

    #[test]
    fn request_events_read_whole_or_from_an_older_server() {
        use crate::model::{RequestQuery, RequestStatus, RequestSummary, TitleKind};
        let e = parse_event(
            r#"{"type":"request_available","request_id":4,"title":"Severance","kind":"show","tmdb_id":95396,"item_id":12,"poster_url":"/api/discover/images/p.jpg","seasons":[2],"complete":false}"#,
        );
        assert_eq!(
            e,
            Some(ServerEvent::RequestAvailable {
                request_id: 4,
                title: "Severance".into(),
                kind: Some(TitleKind::Show),
                tmdb_id: Some(95396),
                item_id: Some(12),
                poster_url: Some("/api/discover/images/p.jpg".into()),
                seasons: Some(vec![2]),
                complete: false,
            })
        );
        // An older server's: fewer fields, and a ready title is whole.
        let old =
            parse_event(r#"{"type":"request_available","request_id":4,"title":"Arrival","item_id":null}"#).unwrap();
        assert!(matches!(old, ServerEvent::RequestAvailable { complete: true, kind: None, .. }));
        assert_eq!(
            parse_event(r#"{"type":"requests_changed","request_id":null,"kind":null,"tmdb_id":null}"#),
            Some(ServerEvent::RequestsChanged { request_id: None, kind: None, tmdb_id: None })
        );
        let decided = parse_event(
            r#"{"type":"request_decided","request_id":5,"title":"Dune","kind":"movie","tmdb_id":438631,"status":"declined","reason":"Already on disc"}"#,
        )
        .unwrap();
        assert!(matches!(decided, ServerEvent::RequestDecided { approved: false, reason: Some(_), .. }));
        assert!(decided.about_requests() && !ServerEvent::WatchlistChanged.about_requests());

        let summary: RequestSummary = serde_json::from_str(
            r#"{"enabled":true,"access":"ask","movies_left":null,"manage":true,"pending":2,"failed":1,"unseen":3,"own":4,"sign_in_again":null,"new_field":1}"#,
        )
        .unwrap();
        assert!(crate::requests::may_ask(Some(&summary)));
        assert_eq!(crate::requests::requests_count(Some(&summary)).0, 6);
        let query = RequestQuery {
            status: vec![RequestStatus::Pending, RequestStatus::Failed],
            mine: true,
            ..RequestQuery::default()
        };
        let set: Vec<(&str, String)> = query.params().into_iter().filter_map(|(k, v)| Some((k, v?))).collect();
        assert_eq!(set, [("status", "pending,failed".to_string()), ("mine", "1".to_string())]);
    }

    #[test]
    fn data_that_isnt_an_event_is_skipped() {
        assert_eq!(parse_event("not json"), None);
        assert_eq!(parse_event("[1,2]"), None);
        assert_eq!(parse_event("{\"no\":\"type\"}"), None);
        assert_eq!(
            parse_event("{\"type\":\"transcoder_restarted\"}"),
            Some(ServerEvent::Other("transcoder_restarted".into()))
        );
    }

    #[test]
    fn retries_double_from_two_to_thirty_seconds() {
        let secs: Vec<u64> = (1..=6).map(|f| retry_delay(f).as_secs()).collect();
        assert_eq!(secs, [2, 4, 8, 16, 30, 30]);
    }

    struct Counting(AtomicUsize);

    impl TokenSource for Counting {
        fn access_token(&self) -> Result<String, ApiError> {
            Ok(format!("t{}", self.0.load(Ordering::SeqCst)))
        }

        fn refresh(&self, _stale: &str) -> Result<String, ApiError> {
            Ok(format!("t{}", self.0.fetch_add(1, Ordering::SeqCst) + 1))
        }

        fn refused_after_refresh(&self, _sent: &str) -> bool {
            false
        }
    }

    #[test]
    fn the_stream_refreshes_a_refused_token_reads_events_and_stops_on_403() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fake = Arc::new(FakeTransport::new(move |r| {
            let n = c.fetch_add(1, Ordering::SeqCst);
            Ok(match (n, r.header_value("Authorization")) {
                (0, Some("Bearer t0")) => Response::empty(401),
                (1, Some("Bearer t1")) => Response {
                    status: 200,
                    headers: vec![("Content-Type".into(), "text/event-stream".into())],
                    body: b"data: {\"type\":\"scan_started\",\"library_id\":1}\n\n".to_vec(),
                },
                (_, _) => Response::empty(403),
            })
        }));
        let api = Api::new(Http::new(fake.clone(), "ua"), "http://nas:7539", Arc::new(Counting(AtomicUsize::new(0))));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut stream = EventStream::start(api, tx, Waker::noop());
        let mut news = Vec::new();
        while let Ok(n) = rx.recv_timeout(Duration::from_secs(10)) {
            let done = n == StreamNews::Stopped;
            news.push(n);
            if done {
                break;
            }
        }
        stream.stop();
        assert_eq!(
            news,
            [
                StreamNews::Connected,
                StreamNews::Event(ServerEvent::ScanStarted { library_id: 1 }),
                // The connection ended at once: the stream waits before trying again.
                StreamNews::Lost,
                StreamNews::Stopped
            ]
        );
        assert!(fake.requests().iter().all(|r| r.header_value("Accept") == Some("text/event-stream")));
    }
}
