// SPDX-License-Identifier: AGPL-3.0-only
//! Media that is not a file on this computer: an `http://` address. The Pley
//! server gives one for what a library's `.strm` file names, where it relays
//! the media it fetches (docs/TRANSCODER.md, "Remote inputs"). FFmpeg reads
//! such an address itself; the readers of our own (keyframe indexes,
//! subtitle tracks, fonts) read it through [`Source`], a range at a time.
//!
//! Only plain HTTP, which is all the server's own address takes: no TLS, no
//! redirects, no chunked bodies. An address that answers otherwise can't be
//! read here, and a job then does what it does for a file with no index.
//!
//! Each request asks for a bounded range, small at first and growing while
//! the reading goes straight on, and what is left of one is read out before
//! the next: whoever relays the media can then keep its own connection to
//! where the media is, rather than give one up at every seek.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

/// Whether `path` is an address rather than a file's name.
pub fn is_remote(path: &Path) -> bool {
    address(path).is_some()
}

/// Whether `path` is a file, or an address (which only reading tells of).
pub fn exists(path: &Path) -> bool {
    is_remote(path) || path.is_file()
}

/// `path` as the address it is, without its scheme.
fn address(path: &Path) -> Option<&str> {
    let text = path.to_str()?;
    let scheme = text.get(..7)?;
    scheme.eq_ignore_ascii_case("http://").then(|| &text[7..])
}

/// A file or an address, read and sought alike.
pub enum Source {
    File(File),
    Remote(Remote),
}

impl Source {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        match address(path) {
            Some(address) => Remote::open(address).map(Source::Remote),
            None => File::open(path).map(Source::File),
        }
    }

    pub fn len(&self) -> std::io::Result<u64> {
        match self {
            Source::File(file) => file.metadata().map(|m| m.len()),
            Source::Remote(remote) => Ok(remote.len),
        }
    }
}

impl Read for Source {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Source::File(file) => file.read(buf),
            Source::Remote(remote) => remote.read(buf),
        }
    }
}

impl Seek for Source {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        match self {
            Source::File(file) => file.seek(to),
            Source::Remote(remote) => remote.seek(to),
        }
    }
}

/// How long an address may take to connect, and to send or take anything.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
/// The most a response's status line and headers take.
const MAX_HEAD: u64 = 16 * 1024;
/// A seek this far ahead reads past what is between rather than asking
/// again: a request costs the server one of its own to where the media is.
const READ_PAST: u64 = 256 * 1024;
/// How much a request asks for after a seek (a block of subtitles, an
/// element's head), and the most one asks for once the reading has gone
/// straight on for a while (a file's Cues, its sample tables).
const FIRST_ASKED: u64 = 64 * 1024;
const MOST_ASKED: u64 = 8 * 1024 * 1024;

/// An address read with ranged requests: one from where the reading is, the
/// next when it is read out or a seek leaves it.
pub struct Remote {
    /// `host:port`, as connected to and as the `Host` header.
    authority: String,
    /// The path and query.
    target: String,
    len: u64,
    pos: u64,
    body: Option<Body>,
    /// How much the next request asks for.
    asked: u64,
}

/// A response being read: `left` bytes of it to come.
struct Body {
    stream: BufReader<TcpStream>,
    left: u64,
}

fn refused(why: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, why.into())
}

impl Remote {
    fn open(address: &str) -> std::io::Result<Self> {
        let (authority, target) = match address.find('/') {
            Some(at) => (&address[..at], &address[at..]),
            None => (address, "/"),
        };
        // Nothing that would end a header line, and no name and password.
        if authority.is_empty() || authority.contains('@') || address.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Err(refused("not an address this reads"));
        }
        let with_port = match authority.rfind(':') {
            Some(colon) if !authority[colon..].contains(']') => authority.to_owned(),
            _ => format!("{authority}:80"),
        };
        let mut remote =
            Self { authority: with_port, target: target.to_owned(), len: 0, pos: 0, body: None, asked: FIRST_ASKED };
        remote.len = remote.request(0, FIRST_ASKED)?;
        Ok(remote)
    }

    /// Asks for `want` bytes from `from` on, and gives the size of the whole.
    fn request(&mut self, from: u64, want: u64) -> std::io::Result<u64> {
        self.body = None;
        let until = from.saturating_add(want.max(1) - 1);
        let mut last = None;
        let mut stream = None;
        for addr in self.authority.to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(err) => last = Some(err),
            }
        }
        let mut stream = stream.ok_or_else(|| last.unwrap_or_else(|| refused("the address names no host")))?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        let _ = stream.set_nodelay(true);
        write!(
            stream,
            "GET {} HTTP/1.1\r\nHost: {}\r\nRange: bytes={from}-{until}\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
            self.target, self.authority
        )?;
        let mut stream = BufReader::with_capacity(64 * 1024, stream);
        let mut head = (&mut stream).take(MAX_HEAD);
        let mut line = String::new();
        head.read_line(&mut line)?;
        let status: u16 = line
            .split_ascii_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .ok_or_else(|| refused("no status"))?;
        let (mut length, mut range, mut chunked) = (None, None, false);
        loop {
            line.clear();
            if head.read_line(&mut line)? == 0 {
                return Err(refused("the response's headers never end"));
            }
            let header = line.trim_end();
            if header.is_empty() {
                break;
            }
            let Some((name, value)) = header.split_once(':') else { continue };
            let value = value.trim();
            match name.to_ascii_lowercase().as_str() {
                "content-length" => length = value.parse::<u64>().ok(),
                "content-range" => range = content_range(value),
                "transfer-encoding" => chunked = !value.eq_ignore_ascii_case("identity"),
                _ => {}
            }
        }
        if chunked {
            return Err(refused("the address doesn't say how much it sends"));
        }
        let (len, left) = match (status, range, length) {
            (206, Some((first, last, len)), _) if first == from && last >= first && len > last => {
                (len, last - first + 1)
            }
            // Everything, whatever was asked for: only the start can be read.
            (200, _, Some(len)) if from == 0 => (len, len),
            (200 | 206, ..) => return Err(refused("the address takes no ranges")),
            (code, ..) => return Err(std::io::Error::other(format!("the address answered {code}"))),
        };
        self.body = Some(Body { stream, left });
        Ok(len)
    }

    /// Leaves the response being read for a seek elsewhere: the little left
    /// of it is read out, so that it ends rather than is cut.
    fn leave(&mut self) {
        if let Some(mut body) = self.body.take()
            && body.left <= FIRST_ASKED
        {
            let _ = std::io::copy(&mut (&mut body.stream).take(body.left), &mut std::io::sink());
        }
        self.asked = FIRST_ASKED;
    }
}

/// `bytes first-last/len`, as numbers.
fn content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (span, len) = value.strip_prefix("bytes")?.trim_start().split_once('/')?;
    let (first, last) = span.split_once('-')?;
    Some((first.trim().parse().ok()?, last.trim().parse().ok()?, len.trim().parse().ok()?))
}

impl Read for Remote {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }
        // Once more from where it stopped, when a connection is cut short.
        for attempt in 0..2 {
            if self.body.is_none() {
                self.request(self.pos, self.asked.max(buf.len() as u64))?;
            }
            let Some(body) = self.body.as_mut() else { break };
            let want = buf.len().min(usize::try_from(body.left).unwrap_or(usize::MAX));
            match body.stream.read(&mut buf[..want]) {
                Ok(0) => self.body = None,
                Ok(n) => {
                    body.left -= n as u64;
                    self.pos += n as u64;
                    if body.left == 0 {
                        // Read out: the reading goes straight on, so more next time.
                        self.body = None;
                        self.asked = (self.asked * 2).min(MOST_ASKED);
                    }
                    return Ok(n);
                }
                // (A read that timed out is `WouldBlock` on Unix.)
                Err(err)
                    if attempt == 0
                        && !matches!(err.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) =>
                {
                    self.body = None
                }
                Err(err) => return Err(err),
            }
        }
        Err(std::io::ErrorKind::UnexpectedEof.into())
    }
}

impl Seek for Remote {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let target = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::Current(by) => self.pos.checked_add_signed(by),
            SeekFrom::End(by) => self.len.checked_add_signed(by),
        }
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a seek to before the start"))?;
        let ahead = target.checked_sub(self.pos);
        match (&mut self.body, ahead) {
            (_, Some(0)) => {}
            (Some(body), Some(ahead)) if ahead <= READ_PAST && ahead < body.left => {
                let read = std::io::copy(&mut (&mut body.stream).take(ahead), &mut std::io::sink());
                if read.is_ok_and(|read| read == ahead) {
                    body.left -= ahead;
                } else {
                    self.body = None;
                }
            }
            _ => self.leave(),
        }
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
pub mod tests {
    use std::io::{Read, Seek, SeekFrom};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};

    use super::*;

    /// A file served at an address of this computer, with ranges, as the
    /// server relays what a `.strm` file names.
    pub struct Served {
        pub url: String,
        /// Requests answered so far.
        pub requests: Arc<AtomicUsize>,
        _stop: tokio::sync::oneshot::Sender<()>,
    }

    impl Served {
        pub fn path(&self) -> PathBuf {
            PathBuf::from(&self.url)
        }
    }

    #[derive(Clone)]
    struct Serving {
        file: Arc<PathBuf>,
        requests: Arc<AtomicUsize>,
        ranges: bool,
    }

    async fn serve_range(State(serving): State<Serving>, headers: HeaderMap) -> Response {
        serving.requests.fetch_add(1, Ordering::Relaxed);
        let Ok(bytes) = tokio::fs::read(&*serving.file).await else { return StatusCode::NOT_FOUND.into_response() };
        let len = bytes.len() as u64;
        let asked = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("bytes="));
        let range = asked.filter(|_| serving.ranges).and_then(|span| {
            let (first, last) = span.split_once('-')?;
            let first: u64 = first.parse().ok()?;
            let last = last.parse::<u64>().ok().map_or(len.saturating_sub(1), |last| last.min(len.saturating_sub(1)));
            Some((first, last))
        });
        match range {
            Some((first, _)) if first >= len => {
                (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{len}"))]).into_response()
            }
            Some((first, last)) => (
                StatusCode::PARTIAL_CONTENT,
                [
                    (header::CONTENT_RANGE, format!("bytes {first}-{last}/{len}")),
                    (header::CONTENT_LENGTH, (last - first + 1).to_string()),
                ],
                Body::from(bytes[first as usize..=last as usize].to_vec()),
            )
                .into_response(),
            None => ([(header::CONTENT_LENGTH, len.to_string())], Body::from(bytes)).into_response(),
        }
    }

    /// Serves `file` until what this gives is dropped.
    pub async fn serve(file: &Path) -> Served {
        serve_with(file, true).await
    }

    async fn serve_with(file: &Path, ranges: bool) -> Served {
        let requests = Arc::new(AtomicUsize::new(0));
        let serving = Serving { file: Arc::new(file.to_owned()), requests: requests.clone(), ranges };
        let app = axum::Router::new().fallback(serve_range).with_state(serving);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/media/film.mkv?key=1", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = stopped.await;
                })
                .await;
        });
        Served { url, requests, _stop: stop }
    }

    fn numbered(len: usize) -> Vec<u8> {
        (0..len).map(|n| (n % 251) as u8).collect()
    }

    #[test]
    fn only_plain_http_addresses_are_remote() {
        assert!(is_remote(Path::new("http://127.0.0.1:7539/api/strm/1")));
        assert!(is_remote(Path::new("HTTP://host/film.mkv")));
        for file in ["https://host/film.mkv", "/media/http://film.mkv", "C:\\media\\film.mkv", "http:/film.mkv", "http"]
        {
            assert!(!is_remote(Path::new(file)), "{file}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_address_is_read_and_sought_like_a_file() {
        let dir = crate::testing::tempdir();
        let file = dir.path().join("film.bin");
        let data = numbered(900_000);
        std::fs::write(&file, &data).unwrap();
        let served = serve(&file).await;
        let (path, requests) = (served.path(), served.requests.clone());
        tokio::task::spawn_blocking(move || {
            let before = requests.load(Ordering::Relaxed);
            let mut source = Source::open(&path).unwrap();
            assert_eq!(source.len().unwrap(), 900_000);
            let mut buf = vec![0; 1000];
            source.read_exact(&mut buf).unwrap();
            assert_eq!(buf, data[..1000]);
            // A little ahead: read past, on the same request.
            assert_eq!(source.seek(SeekFrom::Current(5000)).unwrap(), 6000);
            source.read_exact(&mut buf).unwrap();
            assert_eq!(buf, data[6000..7000]);
            assert_eq!(requests.load(Ordering::Relaxed), before + 1);
            // Far ahead, back, and from the end: asked for again.
            source.seek(SeekFrom::Start(800_000)).unwrap();
            source.read_exact(&mut buf).unwrap();
            assert_eq!(buf, data[800_000..801_000]);
            source.seek(SeekFrom::Start(10)).unwrap();
            source.read_exact(&mut buf).unwrap();
            assert_eq!(buf, data[10..1010]);
            assert_eq!(source.seek(SeekFrom::End(-500)).unwrap(), 899_500);
            let mut rest = Vec::new();
            source.read_to_end(&mut rest).unwrap();
            assert_eq!(rest, data[899_500..]);
            assert_eq!(requests.load(Ordering::Relaxed), before + 4);
            // Past the end: nothing, and nothing asked.
            source.seek(SeekFrom::Start(2_000_000)).unwrap();
            assert_eq!(source.read(&mut buf).unwrap(), 0);
            assert!(source.seek(SeekFrom::Current(-3_000_000)).is_err());
            assert_eq!(requests.load(Ordering::Relaxed), before + 4);
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reading_straight_on_asks_for_more_each_time() {
        let dir = crate::testing::tempdir();
        let file = dir.path().join("film.bin");
        let data = numbered(900_000);
        std::fs::write(&file, &data).unwrap();
        let served = serve(&file).await;
        let (path, requests) = (served.path(), served.requests.clone());
        tokio::task::spawn_blocking(move || {
            let mut source = std::io::BufReader::with_capacity(64 * 1024, Source::open(&path).unwrap());
            let mut read = Vec::new();
            source.read_to_end(&mut read).unwrap();
            assert!(read == data);
            // 64 KiB, then 128, 256 and 512: not a request for each 64 KiB read.
            assert_eq!(requests.load(Ordering::Relaxed), 4);
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_address_without_ranges_is_read_from_its_start_only() {
        let dir = crate::testing::tempdir();
        let file = dir.path().join("film.bin");
        let data = numbered(400_000);
        std::fs::write(&file, &data).unwrap();
        let served = serve_with(&file, false).await;
        let path = served.path();
        tokio::task::spawn_blocking(move || {
            let mut source = Source::open(&path).unwrap();
            let mut buf = vec![0; 1000];
            source.seek(SeekFrom::Start(2000)).unwrap();
            source.read_exact(&mut buf).unwrap();
            assert_eq!(buf, data[2000..3000]);
            source.seek(SeekFrom::Start(300_000)).unwrap();
            assert!(source.read_exact(&mut buf).is_err(), "a range it would have to be asked for");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_address_that_is_not_there_does_not_open() {
        let dir = crate::testing::tempdir();
        let served = serve(&dir.path().join("none.bin")).await;
        let path = served.path();
        let opened = tokio::task::spawn_blocking(move || Source::open(&path).map(|_| ())).await.unwrap();
        assert!(opened.is_err());
        for bad in ["http://", "http://user:secret@127.0.0.1/film.mkv", "http://127.0.0.1/a b"] {
            assert!(Source::open(Path::new(bad)).is_err(), "{bad}");
        }
    }
}
