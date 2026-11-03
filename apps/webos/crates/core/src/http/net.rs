//! The real network: HTTP/1.1 over TCP, TLS through rustls (ring, the
//! webpki roots: a TV's own certificate store is frozen at its release),
//! connections kept alive between requests.
//!
//! Written for the app rather than taken from a client crate because the app
//! needs what those don't give together: a connect timeout chosen per host,
//! cancellation that shuts the socket from another thread (a screen going
//! away, the event stream stopping), no redirects followed (the API decides
//! hop by hop where the token goes), and bodies read as they arrive.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use super::{Method, NetError, Request, Streamed, Transport};
use crate::addresses::Url;
use crate::task::{Cancel, CancelGuard};

/// Idle connections are dropped after this: the server closes them at 30 s.
const IDLE_KEEP: Duration = Duration::from_secs(25);
/// Idle connections kept in all, and to one server.
const MAX_IDLE: usize = 10;
const MAX_IDLE_PER_HOST: usize = 6;
/// The longest answer head taken.
const MAX_HEAD: usize = 64 * 1024;
/// A read with no deadline of its own still ends after this.
const SLOWEST_READ: Duration = Duration::from_secs(60);

/// How long one wait on the socket lasts before the cancel flag is looked at
/// again. On Linux (the TV) a cancel shuts the socket and wakes the read at
/// once; on Windows, where the simulator runs, a shutdown through another
/// handle doesn't, so reads wake every quarter second.
const SLICE: Duration = if cfg!(windows) { Duration::from_millis(250) } else { Duration::from_secs(3600) };

/// The socket, reading until a deadline or a cancel. TLS reads through it too,
/// so a TLS read is bounded the same way.
struct Sock {
    tcp: TcpStream,
    /// When the next read gives up.
    until: Option<Instant>,
    cancel: Option<Cancel>,
}

impl Read for Sock {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
                return Err(io::Error::other(NetError::Cancelled));
            }
            let left = self.until.map_or(SLOWEST_READ, |u| u.saturating_duration_since(Instant::now()));
            if left.is_zero() {
                return Err(io::Error::other(NetError::TimedOut));
            }
            self.tcp.set_read_timeout(Some(left.min(SLICE).max(Duration::from_millis(1))))?;
            match self.tcp.read(buf) {
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
                other => return other,
            }
        }
    }
}

impl Write for Sock {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tcp.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

enum Io {
    Plain(Sock),
    Tls(Box<StreamOwned<ClientConnection, Sock>>),
}

impl Read for Io {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Io::Plain(s) => s.read(buf),
            Io::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Io {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Io::Plain(s) => s.write(buf),
            Io::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Io::Plain(s) => s.flush(),
            Io::Tls(s) => s.flush(),
        }
    }
}

type Key = (String, String, u16);

impl Io {
    fn sock(&mut self) -> &mut Sock {
        match self {
            Io::Plain(s) => s,
            Io::Tls(s) => &mut s.sock,
        }
    }
}

struct Conn {
    io: BufReader<Io>,
    /// The same socket, for shutting it from another thread on a cancel.
    tcp: TcpStream,
    key: Key,
    idle_since: Instant,
}

impl Conn {
    /// Bounds the next read: what is left of `deadline`, at most `each`.
    fn arm(&mut self, deadline: Option<Instant>, each: Option<Duration>) -> io::Result<()> {
        let mut until = Instant::now() + each.unwrap_or(SLOWEST_READ);
        if let Some(d) = deadline {
            if d <= Instant::now() {
                return Err(io::Error::other(NetError::TimedOut));
            }
            until = until.min(d);
        }
        self.io.get_mut().sock().until = Some(until);
        Ok(())
    }

    /// Makes reads give up when `cancel` fires (a connection from the pool serves new requests).
    fn bind(&mut self, cancel: &Option<Cancel>) {
        self.io.get_mut().sock().cancel = cancel.clone();
    }
}

#[derive(Default)]
struct Pool {
    idle: Mutex<Vec<Conn>>,
}

impl Pool {
    fn take(&self, key: &Key) -> Option<Conn> {
        let mut idle = self.idle.lock().unwrap_or_else(|e| e.into_inner());
        idle.retain(|c| c.idle_since.elapsed() < IDLE_KEEP);
        let at = idle.iter().rposition(|c| &c.key == key)?;
        Some(idle.remove(at))
    }

    fn put(&self, mut conn: Conn) {
        conn.idle_since = Instant::now();
        let mut idle = self.idle.lock().unwrap_or_else(|e| e.into_inner());
        idle.retain(|c| c.idle_since.elapsed() < IDLE_KEEP);
        if idle.iter().filter(|c| c.key == conn.key).count() >= MAX_IDLE_PER_HOST {
            return;
        }
        if idle.len() >= MAX_IDLE {
            idle.remove(0);
        }
        idle.push(conn);
    }
}

/// The real network: connections per server kept alive for 25 s.
#[derive(Clone, Default)]
pub struct NetTransport {
    pool: Arc<Pool>,
}

impl NetTransport {
    /// A transport with no connection open yet.
    pub fn new() -> NetTransport {
        NetTransport::default()
    }
}

fn tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("ring supports the default TLS versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

fn net_err(e: io::Error, cancel: &Option<Cancel>) -> NetError {
    if cancel.as_ref().is_some_and(Cancel::is_cancelled) { NetError::Cancelled } else { NetError::from_io(&e) }
}

fn connect(url: &Url, req: &Request, deadline: Option<Instant>) -> Result<Conn, NetError> {
    let host = url.bare_host();
    let port = url.port_or_default();
    let mut wait = req.effective_connect_timeout();
    if let Some(d) = deadline {
        wait = wait.min(d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
    }
    let addrs: Vec<SocketAddr> = match (host, port).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(_) => return Err(NetError::UnknownHost),
    };
    if addrs.is_empty() {
        return Err(NetError::UnknownHost);
    }
    let mut last = NetError::Refused;
    let mut tcp = None;
    for addr in addrs {
        if req.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(NetError::Cancelled);
        }
        match TcpStream::connect_timeout(&addr, wait) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = NetError::from_io(&e),
        }
    }
    let tcp = tcp.ok_or(last)?;
    let _ = tcp.set_nodelay(true);
    let handle = tcp.try_clone().map_err(|e| NetError::from_io(&e))?;
    let sock = Sock { tcp, until: None, cancel: req.cancel.clone() };
    let io = if url.is_https() {
        let name = ServerName::try_from(host.to_string()).map_err(|_| NetError::UnknownHost)?;
        let session = ClientConnection::new(tls_config(), name).map_err(|e| NetError::Tls(e.to_string()))?;
        Io::Tls(Box::new(StreamOwned::new(session, sock)))
    } else {
        Io::Plain(sock)
    };
    let key = (url.scheme.clone(), url.host.clone(), port);
    Ok(Conn { io: BufReader::with_capacity(16 * 1024, io), tcp: handle, key, idle_since: Instant::now() })
}

/// How an answer's body is delimited.
enum Framing {
    Empty,
    Length(u64),
    Chunked { left: u64, crlf_due: bool },
    ToEnd,
}

struct Head {
    status: u16,
    headers: Vec<(String, String)>,
    framing: Framing,
    keep_alive: bool,
}

/// Why an exchange failed, and whether nothing at all came back (so a
/// connection reused from the pool, closed by the server meanwhile, can be
/// replaced and the request sent again).
struct Failure {
    error: io::Error,
    nothing_back: bool,
}

fn write_request(conn: &mut Conn, url: &Url, req: &Request, deadline: Option<Instant>) -> io::Result<()> {
    let mut head = format!("{} {} HTTP/1.1\r\nHost: {}\r\n", req.method.as_str(), url.path, url.host_header());
    for (name, value) in &req.headers {
        let reserved = ["host", "content-length", "connection", "transfer-encoding"];
        if reserved.iter().any(|r| name.eq_ignore_ascii_case(r))
            || name.contains(['\r', '\n'])
            || value.contains(['\r', '\n'])
        {
            continue;
        }
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    let body = req.body.as_deref().unwrap_or(&[]);
    if !body.is_empty() || matches!(req.method, Method::Post | Method::Put | Method::Patch) {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: keep-alive\r\n\r\n");
    let wait = deadline.map_or(Duration::from_secs(30), |d| d.saturating_duration_since(Instant::now()));
    let wait = wait.max(Duration::from_millis(1));
    // A TLS handshake reads while it writes: bound both.
    let sock = conn.io.get_mut().sock();
    sock.tcp.set_write_timeout(Some(wait))?;
    sock.until = Some(Instant::now() + wait);
    let io = conn.io.get_mut();
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(body);
    io.write_all(&bytes)?;
    io.flush()
}

fn read_head(conn: &mut Conn, req: &Request, deadline: Option<Instant>) -> Result<Head, Failure> {
    let mut first = true;
    loop {
        let mut raw = Vec::new();
        loop {
            conn.arm(deadline, req.read_timeout)
                .map_err(|error| Failure { error, nothing_back: first && raw.is_empty() })?;
            let buf = match conn.io.fill_buf() {
                Ok(b) => b,
                Err(error) => return Err(Failure { error, nothing_back: first && raw.is_empty() }),
            };
            if buf.is_empty() {
                let error = io::Error::new(io::ErrorKind::UnexpectedEof, "the connection closed");
                return Err(Failure { error, nothing_back: first && raw.is_empty() });
            }
            // Take bytes up to the end of the head only, so the body stays buffered.
            let before = raw.len();
            raw.extend_from_slice(buf);
            let end = find_head_end(&raw, before.saturating_sub(3));
            let used = match end {
                Some(e) => e - before,
                None => buf.len(),
            };
            conn.io.consume(used);
            if let Some(e) = end {
                raw.truncate(e);
                break;
            }
            if raw.len() > MAX_HEAD {
                let error = io::Error::new(io::ErrorKind::InvalidData, "the answer's head is too big");
                return Err(Failure { error, nothing_back: false });
            }
        }
        first = false;
        let mut headers = [httparse::EMPTY_HEADER; 96];
        let mut parsed = httparse::Response::new(&mut headers);
        let bad = |what: &str| Failure {
            error: io::Error::new(io::ErrorKind::InvalidData, what.to_string()),
            nothing_back: false,
        };
        match parsed.parse(&raw) {
            Ok(httparse::Status::Complete(_)) => {}
            _ => return Err(bad("the answer isn't HTTP")),
        }
        let status = parsed.code.ok_or_else(|| bad("the answer has no status"))?;
        // 100 Continue and other interim answers come before the real one.
        if (100..200).contains(&status) && status != 101 {
            continue;
        }
        let http11 = parsed.version == Some(1);
        let headers: Vec<(String, String)> = parsed
            .headers
            .iter()
            .map(|h| (h.name.to_string(), String::from_utf8_lossy(h.value).trim().to_string()))
            .collect();
        let get = |name: &str| headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str());
        let close = get("Connection").is_some_and(|v| v.to_ascii_lowercase().contains("close"));
        let chunked = get("Transfer-Encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
        let length = get("Content-Length").and_then(|v| v.parse::<u64>().ok());
        let framing = if req.method == Method::Head || status == 204 || status == 304 {
            Framing::Empty
        } else if chunked {
            Framing::Chunked { left: 0, crlf_due: false }
        } else if let Some(n) = length {
            Framing::Length(n)
        } else {
            Framing::ToEnd
        };
        let keep_alive = http11 && !close && !matches!(framing, Framing::ToEnd);
        return Ok(Head { status, headers, framing, keep_alive });
    }
}

/// Where `\r\n\r\n` ends in `raw`, searching from `from`.
fn find_head_end(raw: &[u8], from: usize) -> Option<usize> {
    raw[from..].windows(4).position(|w| w == b"\r\n\r\n").map(|i| from + i + 4)
}

impl Transport for NetTransport {
    fn open(&self, req: Request) -> Result<Streamed, NetError> {
        let url = Url::parse(&req.url).ok_or_else(|| NetError::Failed(format!("not an address: {}", req.url)))?;
        let deadline = req.timeout.map(|t| Instant::now() + t);
        let key: Key = (url.scheme.clone(), url.host.clone(), url.port_or_default());
        for attempt in 0..2 {
            if req.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
                return Err(NetError::Cancelled);
            }
            let pooled = if attempt == 0 { self.pool.take(&key) } else { None };
            let reused = pooled.is_some();
            let mut conn = match pooled {
                Some(c) => c,
                None => connect(&url, &req, deadline)?,
            };
            conn.bind(&req.cancel);
            let guard = req.cancel.as_ref().map(|c| {
                let socket = conn.tcp.try_clone().ok();
                c.on_cancel(move || {
                    if let Some(s) = socket {
                        let _ = s.shutdown(Shutdown::Both);
                    }
                })
            });
            let sent = write_request(&mut conn, &url, &req, deadline)
                .map_err(|error| Failure { error, nothing_back: true })
                .and_then(|()| read_head(&mut conn, &req, deadline));
            match sent {
                Ok(head) => {
                    let body = Body {
                        conn: Some(conn),
                        framing: head.framing,
                        pool: self.pool.clone(),
                        keep_alive: head.keep_alive,
                        deadline,
                        read_timeout: req.read_timeout,
                        cancel: req.cancel.clone(),
                        _guard: guard,
                    };
                    let mut body = body;
                    // An empty body is finished at once, so its connection goes back to the pool now.
                    if matches!(body.framing, Framing::Empty | Framing::Length(0)) {
                        body.finish();
                    }
                    return Ok(Streamed { status: head.status, headers: head.headers, body: Box::new(body) });
                }
                Err(f) if reused && f.nothing_back && !req.cancel.as_ref().is_some_and(Cancel::is_cancelled) => {
                    continue;
                }
                Err(f) => return Err(net_err(f.error, &req.cancel)),
            }
        }
        Err(NetError::Failed("the connection to the server failed".into()))
    }
}

/// An answer's body as it arrives; its connection goes back to the pool
/// once it was read to the end.
struct Body {
    conn: Option<Conn>,
    framing: Framing,
    pool: Arc<Pool>,
    keep_alive: bool,
    deadline: Option<Instant>,
    read_timeout: Option<Duration>,
    cancel: Option<Cancel>,
    _guard: Option<CancelGuard>,
}

impl Body {
    fn finish(&mut self) {
        self.framing = Framing::Empty;
        if let Some(conn) = self.conn.take()
            && self.keep_alive
        {
            self.pool.put(conn);
        }
    }

    fn fail(&mut self, e: io::Error) -> io::Error {
        self.conn = None;
        let kind = net_err(e, &self.cancel);
        io::Error::other(kind)
    }

    fn line(conn: &mut Conn) -> io::Result<String> {
        let mut raw = Vec::new();
        (&mut conn.io).take(4096).read_until(b'\n', &mut raw)?;
        if !raw.ends_with(b"\n") {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "a chunk was cut short"));
        }
        Ok(String::from_utf8_lossy(&raw).trim().to_string())
    }

    fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(conn) = self.conn.as_mut() else { return Ok(0) };
        conn.arm(self.deadline, self.read_timeout)?;
        match &mut self.framing {
            Framing::Empty => Ok(0),
            Framing::Length(left) => {
                let want = buf.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                let n = conn.io.read(&mut buf[..want])?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the answer was cut short"));
                }
                *left -= n as u64;
                if *left == 0 {
                    self.finish();
                }
                Ok(n)
            }
            Framing::ToEnd => {
                let n = conn.io.read(buf)?;
                if n == 0 {
                    self.keep_alive = false;
                    self.finish();
                }
                Ok(n)
            }
            Framing::Chunked { left, crlf_due } => {
                if *left == 0 {
                    if *crlf_due {
                        // The CRLF after the last chunk's data.
                        Body::line(conn)?;
                    }
                    let size_line = Body::line(conn)?;
                    let hex = size_line.split(';').next().unwrap_or("").trim();
                    let size = u64::from_str_radix(hex, 16)
                        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a chunk's size can't be read"))?;
                    if size == 0 {
                        // Trailers, up to the empty line.
                        while !Body::line(conn)?.is_empty() {}
                        self.finish();
                        return Ok(0);
                    }
                    *left = size;
                    *crlf_due = true;
                }
                let want = buf.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                let n = conn.io.read(&mut buf[..want])?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the answer was cut short"));
                }
                *left -= n as u64;
                Ok(n)
            }
        }
    }
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let cancelled = |b: &Body| b.cancel.as_ref().is_some_and(Cancel::is_cancelled);
        if cancelled(self) {
            self.conn = None;
            return Err(io::Error::other(NetError::Cancelled));
        }
        let read = self.read_some(buf);
        // A socket shut by a cancel may read as the end of the answer: it isn't.
        if cancelled(self) {
            self.conn = None;
            return Err(io::Error::other(NetError::Cancelled));
        }
        read.map_err(|e| self.fail(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// A server on 127.0.0.1 answering each connection's requests with `answers`, in order.
    fn serve(answers: Vec<&'static [u8]>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut seen = Vec::new();
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            for answer in answers {
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 {
                        return seen;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    head.push_str(&line);
                }
                seen.push(head);
                stream.write_all(answer).unwrap();
            }
            seen
        });
        (format!("http://127.0.0.1:{}", addr.port()), handle)
    }

    #[test]
    fn reads_lengths_and_chunks_on_one_kept_connection() {
        let (base, server) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n4;x=y\r\ndefg\r\n0\r\n\r\n",
            b"HTTP/1.1 204 No Content\r\n\r\n",
        ]);
        let t = NetTransport::new();
        let read = |path: &str| {
            let s = t.open(Request::get(format!("{base}{path}"))).unwrap();
            let status = s.status;
            let r = s.read_all(1024).unwrap();
            (status, String::from_utf8(r.body).unwrap())
        };
        assert_eq!(read("/a"), (200, "hello".to_string()));
        assert_eq!(read("/b"), (200, "abcdefg".to_string()));
        assert_eq!(read("/c"), (204, String::new()));
        let seen = server.join().unwrap();
        assert_eq!(seen.len(), 3, "one connection served all three");
        assert!(seen[0].starts_with("GET /a HTTP/1.1\r\nHost: 127.0.0.1:"));
    }

    #[test]
    fn refused_connections_say_nothing_answers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let e = NetTransport::new().open(Request::get(format!("http://127.0.0.1:{port}/"))).unwrap_err();
        assert!(matches!(e, NetError::Refused | NetError::Failed(_) | NetError::TimedOut), "{e:?}");
    }

    #[test]
    fn a_silent_server_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let keep = thread::spawn(move || {
            let (s, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(800));
            drop(s);
        });
        let req = Request::get(format!("http://127.0.0.1:{port}/")).timeout(Duration::from_millis(200));
        assert_eq!(NetTransport::new().open(req).unwrap_err(), NetError::TimedOut);
        keep.join().unwrap();
    }

    #[test]
    fn cancelling_stops_a_read_in_progress() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let keep = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n: hi\n\n").unwrap();
            thread::sleep(Duration::from_secs(3));
        });
        let cancel = Cancel::new();
        let req = Request::get(format!("http://127.0.0.1:{port}/api/events"))
            .streaming(Duration::from_secs(30))
            .cancel(Some(cancel.clone()));
        let mut answer = NetTransport::new().open(req).unwrap();
        let mut buf = [0u8; 64];
        assert!(answer.body.read(&mut buf).unwrap() > 0);
        let c = cancel.clone();
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            c.cancel();
        });
        let started = Instant::now();
        let e = answer.body.read(&mut buf).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(NetError::from_io(&e), NetError::Cancelled);
        stopper.join().unwrap();
        keep.join().unwrap();
    }
}
