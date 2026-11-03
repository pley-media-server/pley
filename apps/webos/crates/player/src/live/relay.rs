//! A stream the TV's own player can open though it needs the token (the Samsung app's
//! `Net/StreamRelay.cs`): LG's pipeline takes an address and no request headers, and Pley takes
//! an app's token only in the Authorization header (SECURITY.md), so the app reads the stream
//! with it and hands it on as it comes. Only on the TV's loopback address, under a path of 128
//! random bits that lasts until the next [`StreamRelay::serve`], [`StreamRelay::stop`] or
//! [`Served::end`], and for one stream at a time: serving another, or stopping, ends the
//! connections of the one before.
//!
//! What it takes from a player is bounded: a request's head of 8 KiB at most, all of it sent
//! within 5 s, four players at once. Nothing it logs carries the path, which stands in for the
//! token here.
//!
//! What LG says of it: its player takes media over HTTP and HTTPS
//! (<https://webostv.developer.lge.com/develop/specifications/streaming-protocol-drm>) and plays
//! TS among its containers (docs/LG.md, "What an LG TV plays"). LG documents nothing on a native
//! app listening on the loopback address (its Developer Mode jail included) nor on its player
//! reading a stream with no length from there: a relay that can't listen leaves every channel to
//! its HLS, and a stream the player doesn't start within 10 s falls back to it.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use pley_core::ApiError;
use pley_core::http::Streamed;
use pley_core::task::Cancel;

/// The most of a request's head read: the player's GET is a few lines.
pub const HEAD_MOST: usize = 8 * 1024;
/// How long a player has to send its whole request once connected.
pub const HEAD_WAIT: Duration = Duration::from_secs(5);
/// The longest wait after the system fails to take a connection (out of file handles, say),
/// doubling from a millisecond: trying again at once would spin.
const ACCEPT_BACKOFF_MOST: Duration = Duration::from_secs(1);
/// How long a write to a player that reads nothing may wait: a live stream isn't paused.
const WRITE_WAIT: Duration = Duration::from_secs(30);
/// Players served at once; more are closed as they connect.
pub const PLAYERS_MOST: usize = 4;
const BUFFER: usize = 64 * 1024;

/// Opens the stream upstream (with the token), until the cancel fires.
pub type Opener = Arc<dyn Fn(&Cancel) -> Result<Streamed, ApiError> + Send + Sync>;

/// What is handed on now.
struct Serving {
    path: String,
    open: Opener,
    /// The stream opened as it was served, for the first player to ask: the server answers
    /// while the player starts.
    early: Option<Receiver<Result<Streamed, ApiError>>>,
    /// Fires when it is no longer served: its readings stop and its players' sockets close.
    cancel: Cancel,
}

struct Shared {
    serving: Mutex<Option<Serving>>,
    closed: AtomicBool,
    players: AtomicUsize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing is left half-written under these locks, so a poisoned one is still good.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Hands one stream on to the TV's player at a loopback address.
pub struct StreamRelay {
    shared: Arc<Shared>,
    addr: SocketAddr,
}

impl StreamRelay {
    /// Listens on `127.0.0.1` at a port the system picks. Fails where the app may not listen
    /// (the caller then plays the channel's HLS).
    pub fn new() -> io::Result<StreamRelay> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            serving: Mutex::new(None),
            closed: AtomicBool::new(false),
            players: AtomicUsize::new(0),
        });
        let accepting = shared.clone();
        std::thread::Builder::new().name("pley-relay".into()).spawn(move || accept(&listener, &accepting))?;
        Ok(StreamRelay { shared, addr })
    }

    /// Hands on what `open` reads, in place of what was handed on, to each player that asks,
    /// at `http://127.0.0.1:{port}/relay/{random}/stream.ts`. It is opened at once for the
    /// first player; its data waits in the connection until that player reads it. None when
    /// the system gives no randomness to make the path of.
    pub fn serve(&self, open: Opener) -> Option<Served> {
        self.serve_while(open, || true)
    }

    /// The same, unless `still` says no as it would be served. It is asked with what is served
    /// held, so a [`StreamRelay::stop`] or a serve made after `still` turned false is never
    /// replaced by this one, and one made before ends it.
    pub fn serve_while(&self, open: Opener, still: impl FnOnce() -> bool) -> Option<Served> {
        let bytes = pley_core::auth::proof::secure_random(16)?;
        let path = format!("/relay/{}/stream.ts", hex(&bytes));
        let cancel = Cancel::new();
        let mut serving = lock(&self.shared.serving);
        if !still() {
            return None;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let (early_open, early_cancel) = (open.clone(), cancel.clone());
        // Not opened early when no thread can be had: the player's GET opens it then.
        let early = std::thread::Builder::new()
            .name("pley-relay-open".into())
            .spawn(move || {
                // Never asked for, the answer is dropped with the receiver, closing it.
                let _ = tx.send(early_open(&early_cancel));
            })
            .ok()
            .map(|_| rx);
        let before = serving.replace(Serving { path: path.clone(), open, early, cancel: cancel.clone() });
        drop(serving);
        end(before);
        let url = format!("http://{}{path}", self.addr);
        Some(Served { url, path, cancel, shared: self.shared.clone() })
    }

    /// Hands on nothing more: the address served goes, and its connections with it.
    pub fn stop(&self) {
        let before = lock(&self.shared.serving).take();
        end(before);
    }

    /// Whether an address is served now.
    pub fn serving(&self) -> bool {
        lock(&self.shared.serving).is_some()
    }

    /// Players connected now (tests).
    pub fn players(&self) -> usize {
        self.shared.players.load(Ordering::SeqCst)
    }
}

/// One address the relay serves.
pub struct Served {
    url: String,
    path: String,
    cancel: Cancel,
    shared: Arc<Shared>,
}

impl Served {
    /// Where the TV's player reads it.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Hands it on no more: its address goes, its reading upstream and its players' connections
    /// close. Whatever was served after it stays.
    pub fn end(&self) {
        let mine = {
            let mut serving = lock(&self.shared.serving);
            if serving.as_ref().is_some_and(|s| s.path == self.path) { serving.take() } else { None }
        };
        end(mine);
        self.cancel.cancel();
    }
}

impl Drop for StreamRelay {
    fn drop(&mut self) {
        self.stop();
        self.shared.closed.store(true, Ordering::SeqCst);
        // Wakes the accepting thread, which sees it is closed and lets the listener go.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
    }
}

fn end(serving: Option<Serving>) {
    if let Some(s) = serving {
        // Closes the readings upstream (their sockets shut) and the players' connections.
        s.cancel.cancel();
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `a` and `b` are the same, in a time that doesn't say how much of them is: the path
/// stands in for a secret.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// A player's place, given back when its connection ends.
struct Player<'a>(&'a AtomicUsize);

impl Drop for Player<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept(listener: &TcpListener, shared: &Arc<Shared>) {
    let mut backoff = Duration::ZERO;
    for conn in listener.incoming() {
        if shared.closed.load(Ordering::SeqCst) {
            return;
        }
        let Ok(client) = conn else {
            // A failure that lasts (no file handles left) would fail again at once.
            backoff = (backoff * 2).clamp(Duration::from_millis(1), ACCEPT_BACKOFF_MOST);
            std::thread::sleep(backoff);
            continue;
        };
        backoff = Duration::ZERO;
        if shared.players.fetch_add(1, Ordering::SeqCst) >= PLAYERS_MOST {
            shared.players.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let s = shared.clone();
        let spawned = std::thread::Builder::new().name("pley-relay-player".into()).spawn(move || {
            let _place = Player(&s.players);
            handle(client, &s);
        });
        if spawned.is_err() {
            shared.players.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn handle(mut client: TcpStream, shared: &Shared) {
    let _ = client.set_write_timeout(Some(WRITE_WAIT));
    let Some((method, target)) = read_head(&mut client, HEAD_WAIT) else { return };
    let found = {
        let mut serving = lock(&shared.serving);
        serving.as_mut().filter(|s| same(&s.path, &target)).map(|s| {
            let early = if method == "GET" { s.early.take() } else { None };
            (s.cancel.clone(), s.open.clone(), early)
        })
    };
    let Some((cancel, open, early)) = found else {
        let _ = answer(&mut client, "404 Not Found", "", true);
        return;
    };
    match method.as_str() {
        // As the stream's head, without a length: it has none.
        "HEAD" => {
            let _ = answer(&mut client, "200 OK", "Content-Type: video/mp2t\r\n", false);
            return;
        }
        "GET" => {}
        _ => {
            let _ = answer(&mut client, "405 Method Not Allowed", "", true);
            return;
        }
    }
    // Ending the serving closes this player's connection too.
    let _closing = client.try_clone().ok().map(|c| {
        cancel.on_cancel(move || {
            let _ = c.shutdown(Shutdown::Both);
        })
    });
    let opened = match early {
        Some(rx) => rx.recv().unwrap_or(Err(ApiError::Net(pley_core::NetError::Cancelled))),
        None => open(&cancel),
    };
    let mut body = match opened {
        Ok(body) => body,
        Err(e) => {
            // Refused, or out of reach: the player's error makes the session fall back.
            log::info!("relay: the server's stream.ts didn't come ({})", e.problem());
            let _ = answer(&mut client, "502 Bad Gateway", "", true);
            return;
        }
    };
    if cancel.is_cancelled() {
        return;
    }
    if answer(&mut client, "200 OK", "Content-Type: video/mp2t\r\nCache-Control: no-store\r\n", false).is_err() {
        return;
    }
    // As it comes, to the end or until the player lets go: a live stream has no length.
    let mut buf = vec![0u8; BUFFER];
    let mut total = 0u64;
    loop {
        let n = match body.body.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if cancel.is_cancelled() || client.write_all(&buf[..n]).is_err() {
            break;
        }
        if total == 0 {
            log::debug!("relay: the stream's first {n} bytes handed on");
        }
        total += n as u64;
    }
    log::debug!("relay: the stream ended after {} KB", total / 1024);
}

/// The request line's method and target, from a head read up to its blank line; none when it
/// isn't HTTP, is longer than [`HEAD_MOST`] or isn't all in within `wait`: a player sending a
/// byte now and then holds its place no longer than one that sends nothing.
fn read_head(client: &mut TcpStream, wait: Duration) -> Option<(String, String)> {
    let deadline = Instant::now() + wait;
    let mut head = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        if head.len() >= HEAD_MOST {
            return None;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || client.set_read_timeout(Some(left)).is_err() {
            return None;
        }
        let want = chunk.len().min(HEAD_MOST - head.len());
        match client.read(&mut chunk[..want]) {
            Ok(0) | Err(_) => return None,
            Ok(n) => head.extend_from_slice(&chunk[..n]),
        }
    }
    let line = head.split(|b| *b == b'\r').next()?;
    let line = std::str::from_utf8(line).ok()?;
    let mut parts = line.split(' ');
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none() && version.starts_with("HTTP/")).then(|| (method.to_string(), target.to_string()))
}

fn answer(client: &mut TcpStream, status: &str, headers: &str, empty: bool) -> io::Result<()> {
    let length = if empty { "Content-Length: 0\r\n" } else { "" };
    client.write_all(format!("HTTP/1.1 {status}\r\n{headers}{length}Connection: close\r\n\r\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::atomic::AtomicU32;

    use super::*;

    fn bytes() -> Vec<u8> {
        (0..300_000u32).map(|i| i as u8).collect()
    }

    fn opener(body: Vec<u8>) -> Opener {
        Arc::new(move |_: &Cancel| {
            Ok(Streamed {
                status: 200,
                headers: vec![("Content-Type".into(), "video/mp2t".into())],
                body: Box::new(Cursor::new(body.clone())),
            })
        })
    }

    /// What a player gets for `request` at `url`'s host: the status line, the head, the body.
    fn ask(url: &str, request: &str) -> (String, String, Vec<u8>) {
        let host = url.strip_prefix("http://").and_then(|r| r.split('/').next()).unwrap();
        let mut conn = TcpStream::connect(host).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        // A head past the bound is closed before it is all sent.
        let _ = conn.write_all(request.as_bytes());
        let mut all = Vec::new();
        let _ = conn.read_to_end(&mut all);
        let end = all.windows(4).position(|w| w == b"\r\n\r\n").map_or(all.len(), |p| p + 4);
        let head = String::from_utf8_lossy(&all[..end]).into_owned();
        let status = head.lines().next().unwrap_or("").to_string();
        (status, head, all[end..].to_vec())
    }

    fn get(url: &str) -> (String, String, Vec<u8>) {
        let path = &url[url.find("/relay/").unwrap_or(url.len())..];
        ask(url, &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"))
    }

    fn until(done: impl Fn() -> bool) {
        let start = std::time::Instant::now();
        while !done() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn served(relay: &StreamRelay, open: Opener) -> String {
        relay.serve(open).unwrap().url().to_string()
    }

    #[test]
    fn a_player_gets_the_stream_on_loopback_as_it_comes_under_its_random_path_only() {
        let relay = StreamRelay::new().unwrap();
        let url = served(&relay, opener(bytes()));
        let rest = url.strip_prefix("http://127.0.0.1:").unwrap();
        let (port, path) = rest.split_once('/').unwrap();
        assert!(port.parse::<u16>().is_ok());
        let random = path.strip_prefix("relay/").and_then(|p| p.strip_suffix("/stream.ts")).unwrap();
        assert!(random.len() == 32 && random.bytes().all(|b| b.is_ascii_hexdigit()), "{url}");
        let (status, head, body) = get(&url);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(head.contains("Content-Type: video/mp2t\r\n") && head.contains("Cache-Control: no-store\r\n"));
        assert_eq!(body, bytes());
        let other = url.replace(random, &"0".repeat(32));
        assert_eq!(get(&other).0, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn the_stream_is_opened_as_it_is_served_for_the_first_player_and_anew_for_the_next() {
        let relay = StreamRelay::new().unwrap();
        let opens = Arc::new(AtomicU32::new(0));
        let counted = opens.clone();
        let url = served(
            &relay,
            Arc::new(move |_: &Cancel| {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(Streamed { status: 200, headers: Vec::new(), body: Box::new(Cursor::new(vec![7u8])) })
            }),
        );
        // Before any player asks: the server answers while the player starts.
        until(|| opens.load(Ordering::SeqCst) > 0);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert_eq!(get(&url).2, [7]);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert_eq!(get(&url).2, [7]);
        assert_eq!(opens.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn serving_another_or_stopping_ends_the_address_before() {
        let relay = StreamRelay::new().unwrap();
        let first = served(&relay, opener(bytes()));
        let second = served(&relay, opener(vec![1, 2, 3]));
        assert_ne!(first, second);
        assert_eq!(get(&first).0, "HTTP/1.1 404 Not Found");
        assert_eq!(get(&second).2, [1, 2, 3]);
        relay.stop();
        assert!(!relay.serving());
        assert_eq!(get(&second).0, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn ending_one_address_leaves_the_one_served_after_it() {
        let relay = StreamRelay::new().unwrap();
        let first = relay.serve(opener(vec![1])).unwrap();
        let second = relay.serve(opener(vec![2])).unwrap();
        first.end();
        assert_eq!(get(second.url()).2, [2]);
        second.end();
        assert!(!relay.serving());
        assert_eq!(get(second.url()).0, "HTTP/1.1 404 Not Found");
    }

    #[test]
    fn a_serve_no_longer_wanted_replaces_nothing() {
        let relay = StreamRelay::new().unwrap();
        let now = relay.serve(opener(vec![3])).unwrap();
        let opens = Arc::new(AtomicU32::new(0));
        let counted = opens.clone();
        let late: Opener = Arc::new(move |_: &Cancel| {
            counted.fetch_add(1, Ordering::SeqCst);
            Err(ApiError::Net(pley_core::NetError::Cancelled))
        });
        assert!(relay.serve_while(late, || false).is_none());
        assert_eq!(get(now.url()).2, [3]);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(opens.load(Ordering::SeqCst), 0, "nothing opened upstream for it");
    }

    #[test]
    fn a_head_trickled_a_byte_at_a_time_is_let_go_at_its_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let trickle = std::thread::spawn(move || {
            let mut conn = TcpStream::connect(addr).unwrap();
            // Never a whole head: a byte every 50 ms for 3 s.
            for _ in 0..60 {
                if conn.write_all(b"G").is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let (mut client, _) = listener.accept().unwrap();
        let start = Instant::now();
        assert_eq!(read_head(&mut client, Duration::from_millis(500)), None);
        let took = start.elapsed();
        assert!(took < Duration::from_millis(1500), "held for {took:?}");
        drop(client);
        trickle.join().unwrap();
    }

    #[test]
    fn stopping_closes_a_player_reading_the_stream() {
        // A stream that never ends.
        struct Endless;
        impl Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                std::thread::sleep(Duration::from_millis(2));
                buf[..188].fill(0x47);
                Ok(188)
            }
        }
        let relay = StreamRelay::new().unwrap();
        let endless: Opener =
            Arc::new(|_: &Cancel| Ok(Streamed { status: 200, headers: Vec::new(), body: Box::new(Endless) }));
        let url = served(&relay, endless);
        let reader = std::thread::spawn(move || get(&url));
        until(|| relay.players() > 0);
        std::thread::sleep(Duration::from_millis(50));
        relay.stop();
        let (status, _, body) = reader.join().unwrap();
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(!body.is_empty());
        until(|| relay.players() == 0);
        assert_eq!(relay.players(), 0);
    }

    #[test]
    fn a_stream_the_server_refuses_is_a_bad_gateway_to_the_player() {
        let relay = StreamRelay::new().unwrap();
        let refused: Opener = Arc::new(|_: &Cancel| {
            Err(ApiError::SessionStopped { message: Some("this channel isn't being watched any more".into()) })
        });
        let url = served(&relay, refused);
        assert_eq!(get(&url).0, "HTTP/1.1 502 Bad Gateway");
    }

    #[test]
    fn what_a_player_sends_is_bounded_and_only_get_and_head_are_answered() {
        let relay = StreamRelay::new().unwrap();
        let url = served(&relay, opener(vec![9]));
        let path = url[url.find("/relay/").unwrap()..].to_string();
        let (status, head, body) = ask(&url, &format!("HEAD {path} HTTP/1.1\r\n\r\n"));
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(head.contains("video/mp2t") && body.is_empty());
        assert_eq!(ask(&url, &format!("POST {path} HTTP/1.1\r\n\r\n")).0, "HTTP/1.1 405 Method Not Allowed");
        // Not HTTP, or a head past its bound: closed without an answer.
        assert_eq!(ask(&url, "hello\r\n\r\n").0, "");
        let long = format!("GET {path} HTTP/1.1\r\nX-Pad: {}\r\n\r\n", "a".repeat(HEAD_MOST));
        assert_eq!(ask(&url, &long).0, "");
        // The GET after them still has its stream.
        assert_eq!(get(&url).2, [9]);
    }

    #[test]
    fn paths_compare_whole() {
        assert!(same("/relay/ab/stream.ts", "/relay/ab/stream.ts"));
        assert!(!same("/relay/ab/stream.ts", "/relay/ac/stream.ts"));
        assert!(!same("/relay/ab/stream.ts", "/relay/ab/stream.t"));
    }
}
