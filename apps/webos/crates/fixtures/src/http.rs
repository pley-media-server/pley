//! A small HTTP/1.1 server on std's sockets, enough for Pley's clients: one
//! thread accepting, one per connection, keep-alive, request bodies sized or
//! chunked, and answers held in memory, shared (the sample media), repeated
//! (the speed test) or streamed in chunks (server events).

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The most a request's line and headers may hold.
const MAX_HEAD: usize = 64 * 1024;
/// The most a request's body may hold (the server's own JSON limit is 2 MB).
const MAX_BODY: usize = 16 * 1024 * 1024;
/// A connection that sends nothing for this long is closed.
const IDLE: Duration = Duration::from_secs(60);

/// Locks `m`, carrying on after a panic elsewhere: the fake server's state stays
/// usable for the next request, as a real server's database would.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One request, read whole.
pub(crate) struct Request {
    pub method: String,
    /// The path as sent (still percent-encoded), without the query.
    pub path: String,
    pub raw_query: Option<String>,
    /// The path's parts, decoded, without the empty ones: `["api", "items", "12"]`.
    pub segments: Vec<String>,
    pub query: Vec<(String, String)>,
    /// Names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The first value of a query parameter.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body as JSON, when it is.
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }

    /// An `application/x-www-form-urlencoded` body's pairs, in order.
    pub fn form(&self) -> Vec<(String, String)> {
        parse_pairs(&self.text())
    }
}

/// An answer.
pub(crate) struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

/// What an answer carries.
pub(crate) enum Body {
    Empty,
    Bytes(Vec<u8>),
    /// A part of bytes held once for every request (a sample file).
    Shared(Arc<[u8]>, Range<usize>),
    /// `len` bytes made of `block` over and over (the speed test).
    Repeat(Arc<[u8]>, u64),
    /// Written in chunks as it goes, until the function returns; the
    /// connection closes after it.
    Stream(Streamer),
}

/// What writes a streamed body.
pub(crate) type Streamer = Box<dyn FnOnce(&mut ChunkWriter<'_>) -> io::Result<()> + Send>;

impl Body {
    fn len(&self) -> u64 {
        match self {
            Body::Empty | Body::Stream(_) => 0,
            Body::Bytes(b) => b.len() as u64,
            Body::Shared(_, range) => range.len() as u64,
            Body::Repeat(_, len) => *len,
        }
    }
}

impl Response {
    pub fn new(status: u16) -> Response {
        Response { status, headers: Vec::new(), body: Body::Empty }
    }

    pub fn bytes(status: u16, content_type: &str, bytes: Vec<u8>) -> Response {
        Response::new(status).with_header("Content-Type", content_type).with_body(Body::Bytes(bytes))
    }

    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.set_header(name, value);
        self
    }

    pub fn with_body(mut self, body: Body) -> Response {
        self.body = body;
        self
    }

    /// Sets a header, replacing one of the same name.
    pub fn set_header(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.headers.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
            Some(slot) => slot.1 = value,
            None => self.headers.push((name.to_string(), value)),
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Writes a streamed body's chunks.
pub(crate) struct ChunkWriter<'a> {
    out: &'a mut TcpStream,
}

impl ChunkWriter<'_> {
    /// Sends `data` as one chunk, at once.
    pub fn send(&mut self, data: &[u8]) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        write!(self.out, "{:x}\r\n", data.len())?;
        self.out.write_all(data)?;
        self.out.write_all(b"\r\n")?;
        self.out.flush()
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.write_all(b"0\r\n\r\n")?;
        self.out.flush()
    }
}

/// How slowly answers go out (the fixture server's slow mode).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pace {
    pub latency: Duration,
    pub bytes_per_second: u64,
}

/// What answers the requests.
pub(crate) trait App: Send + Sync + 'static {
    fn answer(&self, request: &Request) -> Response;
    /// `Some` while answers are to be slow.
    fn pace(&self) -> Option<Pace>;
}

#[derive(Default)]
struct Connections {
    next: AtomicU64,
    open: Mutex<HashMap<u64, TcpStream>>,
}

/// A listening socket and its threads.
pub(crate) struct Listener {
    addr: SocketAddr,
    stopping: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    connections: Arc<Connections>,
}

impl Listener {
    /// Listens on 127.0.0.1:`port` (0 for any free port) and answers with `app`.
    pub fn bind(port: u16, app: Arc<dyn App>) -> io::Result<Listener> {
        let socket = TcpListener::bind(("127.0.0.1", port))?;
        let addr = socket.local_addr()?;
        let stopping = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(Connections::default());
        let accept = {
            let stopping = stopping.clone();
            let connections = connections.clone();
            thread::Builder::new()
                .name("pley-fixtures accept".into())
                .spawn(move || accept_loop(socket, app, stopping, connections))?
        };
        Ok(Listener { addr, stopping, accept: Some(accept), connections })
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Stops listening and ends every open connection; the port is free when
    /// this returns.
    pub fn stop(&mut self) {
        let Some(accept) = self.accept.take() else { return };
        self.stopping.store(true, Ordering::SeqCst);
        // Wakes the accepting thread, which sees the flag and drops the socket.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        let _ = accept.join();
        for (_, stream) in lock(&self.connections.open).drain() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(socket: TcpListener, app: Arc<dyn App>, stopping: Arc<AtomicBool>, connections: Arc<Connections>) {
    for stream in socket.incoming() {
        if stopping.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else {
            // Out of descriptors and the like: let it pass rather than spin.
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        let id = connections.next.fetch_add(1, Ordering::Relaxed);
        if let Ok(clone) = stream.try_clone() {
            lock(&connections.open).insert(id, clone);
        }
        let app = app.clone();
        let connections = connections.clone();
        let spawned = thread::Builder::new().name("pley-fixtures connection".into()).spawn(move || {
            serve(stream, &*app);
            lock(&connections.open).remove(&id);
        });
        if spawned.is_err() {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn serve(stream: TcpStream, app: &dyn App) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(IDLE));
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let mut out = stream;
    loop {
        let request = match read_request(&mut reader, &mut out) {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(e) => {
                if e.kind() == io::ErrorKind::InvalidData {
                    let answer = Response::bytes(400, "text/plain; charset=utf-8", e.to_string().into_bytes());
                    let _ = write_response(&mut out, answer, false, false, None);
                }
                break;
            }
        };
        let head = request.method == "HEAD";
        let keep = keeps_alive(&request);
        let answer = catch_unwind(AssertUnwindSafe(|| app.answer(&request))).unwrap_or_else(|_| {
            Response::bytes(
                500,
                "application/json",
                br#"{"code":"internal","message":"internal server error"}"#.to_vec(),
            )
        });
        match write_response(&mut out, answer, head, keep, app.pace()) {
            Ok(true) => continue,
            _ => break,
        }
    }
    let _ = out.shutdown(Shutdown::Both);
}

fn keeps_alive(request: &Request) -> bool {
    !request.header("connection").is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case("close")))
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

/// Reads up to a newline, at most `limit` bytes.
fn read_line(reader: &mut BufReader<TcpStream>, line: &mut Vec<u8>, limit: usize) -> io::Result<usize> {
    line.clear();
    let n = reader.by_ref().take(limit as u64 + 1).read_until(b'\n', line)?;
    if n > limit {
        return Err(invalid("the request's head is too large"));
    }
    while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        line.pop();
    }
    Ok(n)
}

fn read_request(reader: &mut BufReader<TcpStream>, out: &mut TcpStream) -> io::Result<Option<Request>> {
    let mut line = Vec::new();
    let mut used = 0;
    // An empty line before a request is allowed (RFC 9112 §2.2).
    loop {
        let n = read_line(reader, &mut line, MAX_HEAD)?;
        if n == 0 {
            return Ok(None);
        }
        used += n;
        if !line.is_empty() {
            break;
        }
        if used > MAX_HEAD {
            return Err(invalid("the request's head is too large"));
        }
    }
    let request_line = String::from_utf8_lossy(&line).into_owned();
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid("not an HTTP request"));
    };
    if !version.starts_with("HTTP/1.") || method.is_empty() || !target.starts_with('/') {
        return Err(invalid("not an HTTP/1.1 request"));
    }
    let mut headers = Vec::new();
    loop {
        let n = read_line(reader, &mut line, MAX_HEAD)?;
        used += n;
        if used > MAX_HEAD || headers.len() > 100 {
            return Err(invalid("the request's head is too large"));
        }
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the connection closed mid-request"));
        }
        if line.is_empty() {
            break;
        }
        let text = String::from_utf8_lossy(&line);
        let Some((name, value)) = text.split_once(':') else {
            return Err(invalid("a header without a colon"));
        };
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let chunked = header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
    let length = match header("content-length") {
        Some(v) => v.parse::<usize>().map_err(|_| invalid("a Content-Length that isn't a number"))?,
        None => 0,
    };
    if length > MAX_BODY {
        return Err(invalid("the body is too large"));
    }
    if header("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) && (chunked || length > 0) {
        out.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
        out.flush()?;
    }
    let body = if chunked { read_chunked(reader)? } else { read_exact(reader, length)? };

    let (path, raw_query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };
    let segments = path.split('/').filter(|s| !s.is_empty()).map(|s| percent_decode(s, false)).collect();
    let query = raw_query.as_deref().map(parse_pairs).unwrap_or_default();
    Ok(Some(Request { method: method.to_string(), path, raw_query, segments, query, headers, body }))
}

fn read_exact(reader: &mut BufReader<TcpStream>, length: usize) -> io::Result<Vec<u8>> {
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(body)
}

fn read_chunked(reader: &mut BufReader<TcpStream>) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    let mut line = Vec::new();
    loop {
        read_line(reader, &mut line, 1024)?;
        let text = String::from_utf8_lossy(&line);
        let size = text.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size, 16).map_err(|_| invalid("a chunk size that isn't hexadecimal"))?;
        if size == 0 {
            // Trailers, up to the empty line.
            loop {
                if read_line(reader, &mut line, MAX_HEAD)? == 0 || line.is_empty() {
                    return Ok(body);
                }
            }
        }
        if body.len() + size > MAX_BODY {
            return Err(invalid("the body is too large"));
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        read_line(reader, &mut line, 16)?;
    }
}

/// Writes `answer`; says whether the connection may carry another request.
fn write_response(
    out: &mut TcpStream,
    answer: Response,
    head_only: bool,
    keep: bool,
    pace: Option<Pace>,
) -> io::Result<bool> {
    if let Some(pace) = pace {
        thread::sleep(pace.latency);
    }
    let status = answer.status;
    let bodiless = status == 204 || status == 304 || (100..200).contains(&status);
    let streamed = matches!(answer.body, Body::Stream(_));
    let keep = keep && !streamed;
    let mut text = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (name, value) in &answer.headers {
        text.push_str(name);
        text.push_str(": ");
        text.push_str(value);
        text.push_str("\r\n");
    }
    if streamed {
        text.push_str("Transfer-Encoding: chunked\r\n");
    } else if !bodiless {
        text.push_str(&format!("Content-Length: {}\r\n", answer.body.len()));
    }
    text.push_str(if keep { "Connection: keep-alive\r\n\r\n" } else { "Connection: close\r\n\r\n" });
    out.write_all(text.as_bytes())?;
    if head_only || bodiless {
        out.flush()?;
        return Ok(keep);
    }
    match answer.body {
        Body::Empty => {}
        Body::Bytes(bytes) => send(out, &bytes, pace)?,
        Body::Shared(data, range) => send(out, &data[range], pace)?,
        Body::Repeat(block, len) => {
            let mut left = len;
            while left > 0 {
                let n = left.min(block.len() as u64) as usize;
                send(out, &block[..n], pace)?;
                left -= n as u64;
            }
        }
        Body::Stream(run) => {
            out.flush()?;
            let mut writer = ChunkWriter { out };
            run(&mut writer)?;
            writer.finish()?;
        }
    }
    out.flush()?;
    Ok(keep)
}

/// Sends `bytes`, at the slow mode's pace when there is one.
fn send(out: &mut TcpStream, bytes: &[u8], pace: Option<Pace>) -> io::Result<()> {
    let Some(pace) = pace else { return out.write_all(bytes) };
    let step = (pace.bytes_per_second / 10).max(1) as usize;
    for piece in bytes.chunks(step) {
        out.write_all(piece)?;
        out.flush()?;
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        302 => "Found",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        416 => "Range Not Satisfiable",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "",
    }
}

/// `a=1&b=x%20y` as pairs, decoded (`+` is a space).
pub(crate) fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(name, true), percent_decode(value, true))
        })
        .collect()
}

/// Decodes `%XX` (and `+` as a space in queries); malformed escapes stay as
/// they are.
pub(crate) fn percent_decode(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = |b: u8| (b as char).to_digit(16);
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                        continue;
                    }
                    _ => out.push(b'%'),
                }
            }
            b'+' if plus_is_space => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_escapes_and_pluses() {
        assert_eq!(percent_decode("a%20b+c", true), "a b c");
        assert_eq!(percent_decode("a%20b+c", false), "a b+c");
        assert_eq!(percent_decode("100%", true), "100%");
        assert_eq!(percent_decode("%e2%82%ac", false), "€");
        assert_eq!(
            parse_pairs("q=the+harbor&limit=5&x"),
            vec![
                ("q".to_string(), "the harbor".to_string()),
                ("limit".to_string(), "5".to_string()),
                ("x".to_string(), String::new()),
            ]
        );
    }
}
