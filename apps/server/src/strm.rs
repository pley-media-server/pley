//! `.strm` files: a text file in a movie or show library that names the
//! address (`http://` or `https://`) its media is at, as Kodi, Jellyfin and
//! Emby read them. Its name places it like any video's (title, year,
//! episode), and the artwork, subtitles and NFO file beside it are its own.
//!
//! * **The file.** The first line that isn't empty and doesn't start with
//!   `#` is the address ([`parse`]). Kodi's `|User-Agent=…&Referer=…` after
//!   it gives headers to ask with. Anything but an `http` or `https`
//!   address is refused: never a path, so a `.strm` file can't name a file
//!   of this computer.
//! * **Nobody but the server asks it.** The address often carries a
//!   password or a token, so it never leaves the server: not to apps, not
//!   to the transcoder or FFmpeg's command line, not to the log. A player
//!   that plays the media as it is gets it through Pley ([`serve`]), with
//!   ranges; the transcoder reads it at an address of the server's own
//!   ([`input`], `/api/strm/{key}/…`), which answers only to it.
//! * **Asked only when someone wants it.** A scan never asks the address
//!   (a library of links to a hosting service would ask it thousands of
//!   times): what the media is (its streams, its length, its size) is found
//!   the first time its item is opened ([`glance`]) or played ([`ready`]),
//!   and kept for as long as the file names that address. Seek previews,
//!   intro detection and frame grabs, which read a whole file, aren't made
//!   for it.
//! * **Addresses on the internet stay there.** An address on the internet
//!   can't lead (by a redirect, or a name that changes what it resolves to)
//!   to one of this network or computer, whose answer would be handed out
//!   as the video. An address the file itself names on this network is
//!   asked, that being where a NAS or a download service is, and its name
//!   is held to this network from then on: it can't become a host on the
//!   internet that redirects back in.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use parking_lot::Mutex;
use pley_transcoder_api::MediaInfo;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use ring::hmac;
use sha2::{Digest, Sha256};
use url::Url;

use crate::catalog::FileRow;
use crate::error::{ApiResult, AppError};
use crate::library::walk;
use crate::oauth::fetch::{PublicOnly, is_public};
use crate::state::AppState;
use crate::transcoder::TranscoderError;

/// The most of a `.strm` file that is read: an address and a few comments.
const MAX_FILE: u64 = 64 * 1024;
/// The longest address, with its headers.
const MAX_ADDRESS: usize = 8 * 1024;
/// The headers a `.strm` file may ask with (Kodi's `|name=value&…`).
const HEADERS: [HeaderName; 7] = [
    header::USER_AGENT,
    header::REFERER,
    header::ORIGIN,
    header::COOKIE,
    header::AUTHORIZATION,
    header::ACCEPT,
    header::ACCEPT_LANGUAGE,
];
/// Those of them that go to the address's own host only: not to where it
/// redirects (as reqwest has it for the redirects it follows).
const OWN_HOST_ONLY: [HeaderName; 2] = [header::COOKIE, header::AUTHORIZATION];

/// Why a `.strm` file isn't played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Unreadable,
    TooBig,
    Empty,
    NotAnAddress,
    NotHttp,
}

impl Refused {
    /// What is wrong with the file, to follow "its .strm file".
    pub fn what(self) -> &'static str {
        match self {
            Refused::Unreadable => "can't be read",
            Refused::TooBig => "is too big to be one",
            Refused::Empty => "names no address",
            Refused::NotAnAddress => "names an address that can't be read",
            Refused::NotHttp => "names something other than an http or https address",
        }
    }
}

/// What a `.strm` file names: the address, and the headers to ask it with.
#[derive(Clone, PartialEq, Eq)]
pub struct Target {
    url: Url,
    headers: Vec<(HeaderName, HeaderValue)>,
}

/// The host only: the rest may be a password or a token.
impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Target({}://{}/…)", self.url.scheme(), self.url.host_str().unwrap_or_default())
    }
}

impl Target {
    /// Tells this address (and its headers) from any other, without saying
    /// what it is: kept with the file's details ([`ready`]).
    pub fn key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.url.as_str().as_bytes());
        for (name, value) in &self.headers {
            hasher.update(b"\n");
            hasher.update(name.as_str().as_bytes());
            hasher.update(b":");
            hasher.update(value.as_bytes());
        }
        hex::encode(&hasher.finalize()[..16])
    }

    /// The extension the address's path ends with, when it is a plain word
    /// (`mkv`, `webm`): what tells WebM from other Matroska files.
    fn extension(&self) -> Option<String> {
        let name = self.url.path_segments()?.next_back()?;
        let (_, ext) = name.rsplit_once('.')?;
        (1..=5)
            .contains(&ext.len())
            .then_some(ext)
            .filter(|e| e.bytes().all(|b| b.is_ascii_alphanumeric()))
            .map(str::to_ascii_lowercase)
    }
}

/// What a `.strm` file's bytes name (see the module's documentation).
pub fn parse(bytes: &[u8]) -> Result<Target, Refused> {
    let text = decode(bytes).ok_or(Refused::NotAnAddress)?;
    let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).ok_or(Refused::Empty)?;
    if line.len() > MAX_ADDRESS {
        return Err(Refused::NotAnAddress);
    }
    let (address, headers) = match line.split_once('|') {
        Some((address, options)) => match kodi_headers(options) {
            Some(headers) => (address.trim_end(), headers),
            // Not Kodi's options: a `|` of the address's own.
            None => (line, Vec::new()),
        },
        None => (line, Vec::new()),
    };
    let lower = address.get(..8).unwrap_or(address).to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(Refused::NotHttp);
    }
    let url = Url::parse(address).map_err(|_| Refused::NotAnAddress)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(Refused::NotAnAddress);
    }
    Ok(Target { url, headers })
}

/// A `.strm` file's text: UTF-8, or UTF-16 with a byte order mark (what
/// Windows' Notepad calls Unicode), without the mark.
fn decode(bytes: &[u8]) -> Option<String> {
    let utf16 = |unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|pair| unit(*pair)).collect();
        String::from_utf16(&units).ok()
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(u16::from_le_bytes),
        [0xFE, 0xFF, ..] => utf16(u16::from_be_bytes),
        _ => {
            let text = std::str::from_utf8(bytes).ok()?;
            Some(text.strip_prefix('\u{feff}').unwrap_or(text).to_owned())
        }
    }
}

/// Kodi's options after an address's `|` (`name=value&name=value`, values
/// URL-encoded): the headers among them that a file may set. `None` when
/// what follows the `|` isn't options at all.
fn kodi_headers(options: &str) -> Option<Vec<(HeaderName, HeaderValue)>> {
    let mut headers = Vec::new();
    let mut any = false;
    for part in options.split('&').filter(|part| !part.is_empty()) {
        let (name, _) = part.split_once('=')?;
        let Ok(name) = HeaderName::from_bytes(name.trim().as_bytes()) else { return None };
        any = true;
        let Some((_, value)) = url::form_urlencoded::parse(part.as_bytes()).next() else { continue };
        if HEADERS.contains(&name)
            && !headers.iter().any(|(set, _)| *set == name)
            && let Ok(value) = HeaderValue::from_str(value.trim())
        {
            headers.push((name, value));
        }
    }
    any.then_some(headers)
}

/// What the `.strm` file at `path` names, with the file's size and time.
/// Opened as a library's files are ([`walk::open_of_its_kind`]): a link
/// leads only to another `.strm` file. Blocking.
fn read(path: &Path) -> Result<(Target, u64, Option<SystemTime>), Refused> {
    if !walk::is_strm(path) {
        return Err(Refused::Unreadable);
    }
    let (file, meta) = walk::open_of_its_kind(path).ok_or(Refused::Unreadable)?;
    if meta.len() > MAX_FILE {
        return Err(Refused::TooBig);
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).map_err(|_| Refused::Unreadable)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(Refused::TooBig);
    }
    Ok((parse(&bytes)?, meta.len(), meta.modified().ok()))
}

/// [`Target::key`] of what the `.strm` file at `path` names, for a scan;
/// `None` for a file that names nothing to play. Blocking.
pub fn key_of(path: &Path) -> Option<String> {
    match read(path) {
        Ok((target, ..)) => Some(target.key()),
        Err(why) => {
            tracing::warn!("{}: the .strm file {}", path.display(), why.what());
            None
        }
    }
}

/// Why a `.strm` file's media didn't come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    Refused(Refused),
    /// The address didn't answer, or its answer stopped.
    Unreachable,
    /// The address answered with this, not with media.
    Answered(StatusCode),
    /// What the address gave isn't video or audio.
    NotMedia,
    /// The server has no address the transcoder can read it at.
    NoWayIn,
}

impl From<Refused> for Failed {
    fn from(why: Refused) -> Self {
        Failed::Refused(why)
    }
}

impl From<Failed> for AppError {
    fn from(failed: Failed) -> Self {
        match failed {
            Failed::Refused(why) => AppError::NotFound(format!("this item's .strm file {}", why.what())),
            Failed::Answered(StatusCode::NOT_FOUND | StatusCode::GONE) => {
                AppError::NotFound("the address this item's .strm file names has nothing there any more".into())
            }
            Failed::Answered(status) => {
                AppError::Unavailable(format!("the address this item's .strm file names answered {}", status.as_u16()))
            }
            Failed::Unreachable => {
                AppError::Unavailable("the address this item's .strm file names didn't answer".into())
            }
            Failed::NotMedia => {
                AppError::NotFound("the address this item's .strm file names gave no video or audio".into())
            }
            Failed::NoWayIn => AppError::Unavailable(
                "the transcoder can't reach this server to read a .strm file's media: it needs the server's address"
                    .into(),
            ),
        }
    }
}

/// A `.strm` file as it was last read: what it names, and what is known of
/// the media there.
pub struct Link {
    path: String,
    size: u64,
    mtime: Option<SystemTime>,
    target: Target,
    /// What the server's own address for it ends with ([`Relay::address`]).
    key: String,
    /// Whether the address is on the internet: found once, when it is first
    /// asked, and held to from then on.
    public: tokio::sync::OnceCell<bool>,
    /// The media's size (0: not known yet).
    len: AtomicU64,
    /// Where the address last redirected to, and when: asked there for a
    /// while, rather than through the address once more for each range.
    resolved: Mutex<Option<(Url, Instant)>>,
    /// When it was last used ([`Relay::links`] forgets the oldest).
    used: AtomicI64,
}

impl Link {
    /// The media's size, once an answer told.
    pub fn media_len(&self) -> Option<u64> {
        Some(self.len.load(Ordering::Relaxed)).filter(|len| *len > 0)
    }

    /// [`Target::key`] of what it names.
    pub fn target_key(&self) -> String {
        self.target.key()
    }
}

/// How long a redirect's end is asked directly.
const RESOLVED_FOR: Duration = Duration::from_secs(10 * 60);
/// The `.strm` files remembered as read; the least recently used go first.
const LINKS_KEPT: usize = 4096;
/// How long a file whose media wasn't found isn't asked again when it is
/// played, and when its item is only opened.
const FAILED_FOR: Duration = Duration::from_secs(20);
const FAILED_TO_OPEN_FOR: Duration = Duration::from_secs(10 * 60);
/// How long opening an item waits for what its `.strm` files' media is.
const GLANCE: Duration = Duration::from_millis(2500);

#[derive(Default)]
struct Links {
    by_path: HashMap<String, Arc<Link>>,
    by_key: HashMap<String, Arc<Link>>,
}

/// Reads what `.strm` files name, for players and for the transcoder.
pub struct Relay {
    key: hmac::Key,
    /// Reaches addresses on the internet only, wherever they redirect to.
    public: reqwest::Client,
    /// Reaches any address: for one the file names on this network.
    any: reqwest::Client,
    /// The names `.strm` files gave for addresses of this network: `any`
    /// resolves them to this network only ([`Inside`]).
    inside: Arc<Mutex<HashSet<String>>>,
    links: Mutex<Links>,
    /// One look at a file's media at a time ([`ready`]).
    probing: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    failed: Mutex<HashMap<i64, (Instant, Failed)>>,
    /// Where the transcoder reaches this server, when it isn't where the
    /// server listens ([`Relay::listen_at`]).
    address: Mutex<Option<String>>,
}

/// Whether `url` names, in numbers, an address of this network or
/// computer. One it names by a name is the resolver's to tell
/// ([`PublicOnly`]), as it connects.
fn written_as_private(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => !is_public(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => !is_public(IpAddr::V6(ip)),
        Some(url::Host::Domain(_)) => false,
        None => true,
    }
}

/// The resolver of the client that reaches any address. A name a `.strm`
/// file gave for an address of this network resolves to this network only:
/// were it to name a host on the internet later (a name that changes what
/// it resolves to), that host could redirect to wherever it liked here,
/// its answer handed out as the video. Other names (where an address of
/// this network redirects to) resolve as they do.
struct Inside(Arc<Mutex<HashSet<String>>>);

impl Resolve for Inside {
    fn resolve(&self, name: Name) -> Resolving {
        let name = name.as_str().to_ascii_lowercase();
        let held = self.0.lock().contains(&name);
        Box::pin(async move {
            let found: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|addr| !held || !is_public(addr.ip()))
                .collect();
            if found.is_empty() {
                return Err(format!("{name} is no longer on this network").into());
            }
            Ok(Box::new(found.into_iter()) as Addrs)
        })
    }
}

/// The client for addresses on the internet (`inside`: none), which
/// reaches nothing else, or the one that reaches any address.
fn http_client(inside: Option<Arc<Mutex<HashSet<String>>>>) -> reqwest::Client {
    let public_only = inside.is_none();
    let mut headers = HeaderMap::new();
    // The media as it is: nothing to decompress.
    headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    let redirects = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        if public_only && written_as_private(attempt.url()) {
            return attempt.error("a redirect to this network");
        }
        attempt.follow()
    });
    let mut builder = reqwest::Client::builder()
        .redirect(redirects)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .no_gzip()
        .default_headers(headers)
        .user_agent(concat!("Pley/", env!("CARGO_PKG_VERSION")));
    builder = match inside {
        Some(names) => builder.dns_resolver(Arc::new(Inside(names))),
        None => builder.dns_resolver(Arc::new(PublicOnly)),
    };
    builder.build().expect("http client")
}

impl Relay {
    /// With a key derived from the server's `secret.key`, so that a file's
    /// address for the transcoder stays the same from one start to the next
    /// (the transcoder keeps what it read of it by that address); a key
    /// that can't be read gives one for this run only.
    pub fn new(data_dir: &Path) -> Self {
        let secret = crate::two_step::load_key(&data_dir.join(crate::two_step::KEY_FILE)).unwrap_or_else(|_| {
            let mut secret = [0u8; 32];
            getrandom::fill(&mut secret).expect("the system's random numbers");
            secret
        });
        let inside = Arc::new(Mutex::new(HashSet::new()));
        Self {
            key: crate::two_step::derived_key(&secret, b"pley strm relay"),
            public: http_client(None),
            any: http_client(Some(inside.clone())),
            inside,
            links: Mutex::default(),
            probing: Mutex::default(),
            failed: Mutex::default(),
            address: Mutex::new(None),
        }
    }

    /// Tells the transcoder to read `.strm` files' media at `base`
    /// (`http://host:port`) rather than where the server listens.
    pub fn listen_at(&self, base: &str) {
        *self.address.lock() = Some(base.trim_end_matches('/').to_owned());
    }

    /// The `.strm` file at `path` as it is now: read again only when the
    /// file changed.
    pub async fn link(&self, path: &str) -> Result<Arc<Link>, Refused> {
        let (named, known) = (std::path::PathBuf::from(path), self.links.lock().by_path.get(path).cloned());
        let read = tokio::task::spawn_blocking(move || {
            let meta = std::fs::metadata(&named).map_err(|_| Refused::Unreadable)?;
            if let Some(known) = known.filter(|k| k.size == meta.len() && k.mtime == meta.modified().ok()) {
                return Ok(Err(known));
            }
            read(&named).map(Ok)
        })
        .await
        .map_err(|_| Refused::Unreadable)??;
        let link = match read {
            Err(known) => known,
            Ok((target, size, mtime)) => {
                let tag = hmac::sign(&self.key, format!("{path}\n{}", target.key()).as_bytes());
                let link = Arc::new(Link {
                    path: path.to_owned(),
                    size,
                    mtime,
                    key: hex::encode(&tag.as_ref()[..16]),
                    target,
                    public: tokio::sync::OnceCell::new(),
                    len: AtomicU64::new(0),
                    resolved: Mutex::new(None),
                    used: AtomicI64::new(0),
                });
                let mut links = self.links.lock();
                if links.by_key.len() >= LINKS_KEPT {
                    let mut oldest: Vec<_> = links.by_key.values().map(|l| l.used.load(Ordering::Relaxed)).collect();
                    oldest.sort_unstable();
                    let cut = oldest[oldest.len() / 4];
                    links.by_path.retain(|_, l| l.used.load(Ordering::Relaxed) > cut);
                    links.by_key.retain(|_, l| l.used.load(Ordering::Relaxed) > cut);
                }
                // The address a job still reads the file's last version at
                // stays, until it is the oldest.
                links.by_path.insert(path.to_owned(), link.clone());
                links.by_key.insert(link.key.clone(), link.clone());
                link
            }
        };
        link.used.store(crate::util::now(), Ordering::Relaxed);
        Ok(link)
    }

    /// The file the server's own address ending in `key` is for.
    fn by_key(&self, key: &str) -> Option<Arc<Link>> {
        let link = self.links.lock().by_key.get(key).cloned()?;
        link.used.store(crate::util::now(), Ordering::Relaxed);
        Some(link)
    }

    /// Where the transcoder reads `link`'s media: an address of the
    /// server's own, which says nothing of the one the file names. It ends
    /// like that one, for what tells formats apart by their name.
    async fn address(&self, state: &AppState, link: &Link) -> Result<String, Failed> {
        let told = self.address.lock().clone();
        let base = match told {
            Some(base) => base,
            None => own_address(state).await.ok_or(Failed::NoWayIn)?,
        };
        let ext = link.target.extension().map(|ext| format!(".{ext}")).unwrap_or_default();
        Ok(format!("{base}/api/strm/{}/media{ext}", link.key))
    }

    /// The client `link`'s address is asked with: once it is found to be on
    /// the internet (a name with any public address is), the one that
    /// reaches nothing else. A name found to be of this network is held to
    /// it ([`Inside`]).
    async fn client(&self, link: &Link) -> Result<&reqwest::Client, Failed> {
        let public = link
            .public
            .get_or_try_init(|| async {
                let url = &link.target.url;
                match url.host() {
                    Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)) => Ok(!written_as_private(url)),
                    Some(url::Host::Domain(name)) => {
                        let port = url.port_or_known_default().unwrap_or(80);
                        let found: Vec<SocketAddr> =
                            tokio::net::lookup_host((name, port)).await.map_err(|_| Failed::Unreachable)?.collect();
                        if found.is_empty() {
                            return Err(Failed::Unreachable);
                        }
                        let public = found.iter().any(|addr| is_public(addr.ip()));
                        if !public {
                            self.inside.lock().insert(name.to_ascii_lowercase());
                        }
                        Ok(public)
                    }
                    None => Err(Failed::Refused(Refused::NotAnAddress)),
                }
            })
            .await?;
        Ok(if *public { &self.public } else { &self.any })
    }

    async fn get(
        client: &reqwest::Client,
        link: &Link,
        url: &Url,
        range: Option<&HeaderValue>,
    ) -> Result<reqwest::Response, Failed> {
        let own_host = url.origin() == link.target.url.origin();
        let mut request = client.get(url.clone());
        for (name, value) in &link.target.headers {
            if own_host || !OWN_HOST_ONLY.contains(name) {
                request = request.header(name, value);
            }
        }
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        request.send().await.map_err(|err| {
            // Without the address: it may hold a password or a token.
            tracing::debug!("{}: its address didn't answer: {}", link.path, err.without_url());
            Failed::Unreachable
        })
    }

    /// Asks `link`'s address for `range` of its media (all of it without
    /// one): where it last redirected to first, while that still answers.
    async fn fetch(&self, link: &Link, range: Option<&HeaderValue>) -> Result<reqwest::Response, Failed> {
        let answers = |status: StatusCode| {
            matches!(status, StatusCode::OK | StatusCode::PARTIAL_CONTENT | StatusCode::RANGE_NOT_SATISFIABLE)
        };
        let client = self.client(link).await?;
        let resolved = link.resolved.lock().clone().filter(|(_, at)| at.elapsed() < RESOLVED_FOR);
        if let Some((url, _)) = resolved {
            match Self::get(client, link, &url, range).await {
                Ok(response) if answers(response.status()) => return Ok(response),
                _ => *link.resolved.lock() = None,
            }
        }
        let response = Self::get(client, link, &link.target.url, range).await?;
        if !answers(response.status()) {
            tracing::debug!("{}: its address answered {}", link.path, response.status());
            return Err(Failed::Answered(response.status()));
        }
        if response.url() != &link.target.url {
            *link.resolved.lock() = Some((response.url().clone(), Instant::now()));
        }
        Ok(response)
    }

    /// `link`'s media as the answer to a request with `method` and
    /// `headers` (its `Range`), said to be `content_type`: the bytes as they
    /// come, never the address's own headers.
    pub async fn respond(
        &self,
        link: &Link,
        method: &Method,
        headers: &HeaderMap,
        content_type: &'static str,
    ) -> Result<Response, Failed> {
        let head = method == Method::HEAD;
        let asked = headers.get(header::RANGE).filter(|range| is_byte_range(range));
        // A HEAD is asked as one byte: not every address answers a HEAD.
        let first_byte = HeaderValue::from_static("bytes=0-0");
        let upstream = self.fetch(link, if head { Some(&first_byte) } else { asked }).await?;
        let status = upstream.status();
        let told = |name: HeaderName| upstream.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
        let range = told(header::CONTENT_RANGE).and_then(|value| content_range(&value));
        let length = told(header::CONTENT_LENGTH).and_then(|value| value.parse::<u64>().ok());
        let total = match status {
            StatusCode::OK => length,
            _ => range.and_then(|(_, total)| total),
        };
        if let Some(total) = total.filter(|total| *total > 0) {
            link.len.store(total, Ordering::Relaxed);
        }
        let ranges = status == StatusCode::PARTIAL_CONTENT
            || told(header::ACCEPT_RANGES).is_some_and(|unit| unit.eq_ignore_ascii_case("bytes"));

        let mut response = Response::new(Body::empty());
        let out = response.headers_mut();
        out.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        out.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
        if ranges {
            out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        }
        if head {
            if let Some(total) = total {
                out.insert(header::CONTENT_LENGTH, HeaderValue::from(total));
            }
            return Ok(response);
        }
        match (status, range) {
            (StatusCode::PARTIAL_CONTENT, Some((Some((first, last)), total))) => {
                let total = total.map_or_else(|| "*".to_owned(), |total| total.to_string());
                let value = format!("bytes {first}-{last}/{total}");
                out.insert(header::CONTENT_RANGE, HeaderValue::from_str(&value).map_err(|_| Failed::Unreachable)?);
                out.insert(header::CONTENT_LENGTH, HeaderValue::from(last - first + 1));
                *response.status_mut() = StatusCode::PARTIAL_CONTENT;
            }
            (StatusCode::RANGE_NOT_SATISFIABLE, range) => {
                let total = range.and_then(|(_, total)| total).or(link.media_len());
                if let Some(value) = total.and_then(|total| HeaderValue::from_str(&format!("bytes */{total}")).ok()) {
                    out.insert(header::CONTENT_RANGE, value);
                }
                *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                return Ok(response);
            }
            (StatusCode::OK, _) => {
                if let Some(length) = length {
                    out.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
                }
            }
            // Part of it, with nothing to say which part.
            _ => return Err(Failed::Unreachable),
        }
        *response.body_mut() = Body::from_stream(upstream.bytes_stream());
        Ok(response)
    }
}

/// Whether `range` asks for one range of bytes: `bytes=a-b`, `bytes=a-` or
/// `bytes=-n`. Anything else isn't passed on.
fn is_byte_range(range: &HeaderValue) -> bool {
    let Some(span) = range.to_str().ok().and_then(|value| value.trim().strip_prefix("bytes=")) else { return false };
    let Some((first, last)) = span.split_once('-') else { return false };
    let number = |text: &str| text.len() <= 19 && text.bytes().all(|b| b.is_ascii_digit());
    number(first) && number(last) && !(first.is_empty() && last.is_empty())
}

/// What a `Content-Range` says: the first and last byte it holds (none for
/// `*`), and the size of the whole, when it says.
type ContentRange = (Option<(u64, u64)>, Option<u64>);

/// `bytes first-last/total` as numbers; `bytes */total` has no span, and a
/// total of `*` isn't known.
fn content_range(value: &str) -> Option<ContentRange> {
    let (span, total) = value.trim().strip_prefix("bytes")?.trim_start().split_once('/')?;
    let total = match total.trim() {
        "*" => None,
        total => Some(total.parse().ok()?),
    };
    let span = match span.trim() {
        "*" => None,
        span => {
            let (first, last) = span.split_once('-')?;
            let (first, last): (u64, u64) = (first.trim().parse().ok()?, last.trim().parse().ok()?);
            if last < first {
                return None;
            }
            Some((first, last))
        }
    };
    Some((span, total))
}

/// The transcoder's host, when it runs elsewhere (`transcoder_url`).
fn transcoder_host(state: &AppState) -> Option<(String, u16)> {
    let url = Url::parse(state.config.transcoder_url.as_deref()?).ok()?;
    Some((url.host_str()?.trim_matches(['[', ']']).to_owned(), url.port_or_known_default()?))
}

/// Where the transcoder reaches this server: on this computer for the one
/// the server runs, else at the address this computer has towards it.
async fn own_address(state: &AppState) -> Option<String> {
    use crate::network::Kind;
    let web: Vec<SocketAddr> = state
        .network
        .listeners()
        .into_iter()
        .filter(|l| l.kind == Kind::Web && l.error.is_none() && l.address.port() != 0)
        .map(|l| l.address)
        .collect();
    let towards = match transcoder_host(state) {
        None => None,
        // No packet is sent: connecting a UDP socket only picks the route.
        Some((host, port)) => {
            let there = tokio::net::lookup_host((host.as_str(), port)).await.ok()?.next()?;
            let socket = std::net::UdpSocket::bind(if there.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).ok()?;
            socket.connect(there).ok()?;
            Some(socket.local_addr().ok()?.ip().to_canonical())
        }
    };
    let reached = |listener: &SocketAddr| match towards {
        Some(ip) => ((listener.ip().is_unspecified() && listener.is_ipv4() == ip.is_ipv4()) || listener.ip() == ip)
            .then(|| SocketAddr::new(ip, listener.port())),
        None if listener.ip().is_loopback() => Some(*listener),
        None if listener.ip().is_unspecified() => {
            let loopback: IpAddr =
                if listener.is_ipv4() { [127, 0, 0, 1].into() } else { std::net::Ipv6Addr::LOCALHOST.into() };
            Some(SocketAddr::new(loopback, listener.port()))
        }
        None => None,
    };
    let address = web.iter().find_map(reached).or_else(|| web.first().copied().filter(|_| towards.is_none()))?;
    Some(format!("http://{address}"))
}

/// Whether a request for a file's media at the server's own address comes
/// from the transcoder: from this computer for the one the server runs,
/// from its host for one elsewhere, and through no proxy (a proxy on this
/// computer would make every request look local). The address's key is
/// what the transcoder was given; this keeps one read in a log from being
/// of use anywhere else.
async fn from_transcoder(state: &AppState, peer: Option<IpAddr>, headers: &HeaderMap) -> bool {
    const FORWARDED: [&str; 3] = ["forwarded", "x-forwarded-for", "x-real-ip"];
    if FORWARDED.iter().any(|name| headers.contains_key(*name)) {
        return false;
    }
    // Without a connection (tests), the key alone.
    let Some(peer) = peer.map(|ip| ip.to_canonical()) else { return true };
    if peer.is_loopback() {
        return true;
    }
    match transcoder_host(state) {
        Some((host, port)) => match tokio::net::lookup_host((host.as_str(), port)).await {
            Ok(mut found) => found.any(|addr| addr.ip().to_canonical() == peer),
            Err(_) => false,
        },
        None => false,
    }
}

/// `GET /api/strm/{key}/{name}`: a `.strm` file's media, for the
/// transcoder ([`input`]). No account asks: the key in the address is what
/// the transcoder was given, and only it is answered.
pub async fn relay(
    axum::extract::State(state): axum::extract::State<AppState>,
    crate::auth::PeerIp(peer): crate::auth::PeerIp,
    axum::extract::Path((key, _name)): axum::extract::Path<(String, String)>,
    request: axum::extract::Request,
) -> ApiResult<Response> {
    if !from_transcoder(&state, peer, request.headers()).await {
        return Err(AppError::not_found("not found"));
    }
    let link = state.strm.by_key(&key).ok_or_else(|| AppError::not_found("not found"))?;
    Ok(state.strm.respond(&link, request.method(), request.headers(), "application/octet-stream").await?)
}

/// What the transcoder is given to read `file`: its path, or for a `.strm`
/// file the server's own address for its media.
pub async fn input(state: &AppState, file: &FileRow) -> ApiResult<String> {
    if !file.is_strm() {
        return Ok(file.path.clone());
    }
    if !state.transcoder.info().is_some_and(|info| info.capabilities.remote_inputs) {
        return Err(if state.transcoder.is_available() {
            AppError::NotImplemented("the transcoder is too old to read a .strm file's media".into())
        } else {
            AppError::Unavailable("the transcoder isn't running: try again in a moment".into())
        });
    }
    let link = state.strm.link(&file.path).await.map_err(Failed::from)?;
    Ok(state.strm.address(state, &link).await?)
}

/// A `.strm` file's media as a player's direct play or download gets it:
/// the bytes, with ranges, said to be what the file was found to hold.
/// Only once it was found to hold video or audio ([`ready`]).
pub async fn serve(state: &AppState, file: &FileRow, method: &Method, headers: &HeaderMap) -> ApiResult<Response> {
    let file = ready(state, file.clone()).await?;
    let link = state.strm.link(&file.path).await.map_err(Failed::from)?;
    let name = format!("media.{}", file.container.as_deref().unwrap_or_default());
    let content_type = match mime_guess::from_path(&name).first_raw() {
        Some(kind) if kind.starts_with("video/") || kind.starts_with("audio/") => kind,
        _ => "application/octet-stream",
    };
    Ok(state.strm.respond(&link, method, headers, content_type).await?)
}

/// The name `file` is saved under when it is downloaded: its own, or for a
/// `.strm` file its name ending as what its media is (`Film.mkv`).
pub fn download_name(file: &FileRow) -> String {
    let path = Path::new(&file.path);
    let text = |name: Option<&std::ffi::OsStr>| name.map(|n| n.to_string_lossy().into_owned());
    match file.container.as_deref().filter(|_| file.is_strm()) {
        Some(container) if !container.is_empty() && container.bytes().all(|b| b.is_ascii_alphanumeric()) => {
            format!("{}.{container}", text(path.file_stem()).unwrap_or_else(|| "file".into()))
        }
        _ => text(path.file_name()).unwrap_or_else(|| "file".into()),
    }
}

/// `file` with what its media is: for a `.strm` file that hasn't been
/// played since it named its address, found now (the transcoder probes it
/// through the server) and kept. Any other file as it is.
pub async fn ready(state: &AppState, file: FileRow) -> ApiResult<FileRow> {
    if !file.is_strm() || file.media_info.is_some() {
        return Ok(file);
    }
    let relay = &state.strm;
    let turn = relay.probing.lock().entry(file.id).or_default().clone();
    let _one_at_a_time = turn.lock().await;
    // Found by the request that had the turn before this one?
    let file = crate::catalog::file(&state.db, file.id).await?.ok_or_else(|| AppError::not_found("file not found"))?;
    if file.media_info.is_some() {
        return Ok(file);
    }
    if let Some((_, failed)) = relay.failed.lock().get(&file.id).filter(|(at, _)| at.elapsed() < FAILED_FOR) {
        return Err((*failed).into());
    }
    let found = look(state, &file).await;
    // The turns nobody waits for any more.
    relay.probing.lock().retain(|_, turn| Arc::strong_count(turn) > 1);
    match found {
        Ok(found) => Ok(found),
        Err(Ok(failed)) => {
            let mut failures = relay.failed.lock();
            failures.retain(|_, (at, _)| at.elapsed() < FAILED_TO_OPEN_FOR);
            failures.insert(file.id, (Instant::now(), failed));
            Err(failed.into())
        }
        Err(Err(other)) => Err(other),
    }
}

/// Finds what the media of item `item_id`'s `.strm` files is, when it isn't
/// known yet, as someone opens the item: its page then shows the tracks and
/// the size, and a download knows what it takes. Waited for a moment only
/// (the look goes on by itself), and not tried again for a while where it
/// failed: the item opens as it is then. Whether anything was found.
pub async fn glance(state: &AppState, item_id: i64) -> bool {
    let Ok(files) = crate::catalog::files_for_item(&state.db, item_id).await else { return false };
    let unknown: Vec<FileRow> = {
        let failed = state.strm.failed.lock();
        let tried = |id: &i64| failed.get(id).is_some_and(|(at, _)| at.elapsed() < FAILED_TO_OPEN_FOR);
        files.into_iter().filter(|f| f.is_strm() && f.media_info.is_none() && !tried(&f.id)).collect()
    };
    if unknown.is_empty() || !state.transcoder.info().is_some_and(|info| info.capabilities.remote_inputs) {
        return false;
    }
    let state = state.clone();
    let looking = tokio::spawn(async move {
        let mut found = false;
        for file in unknown {
            found |= ready(&state, file).await.is_ok();
        }
        found
    });
    matches!(tokio::time::timeout(GLANCE, looking).await, Ok(Ok(true)))
}

/// [`ready`]'s look at `file`'s media. A failure of the address or its
/// media (remembered for a moment), or any other error.
async fn look(state: &AppState, file: &FileRow) -> Result<FileRow, Result<Failed, AppError>> {
    let link = state.strm.link(&file.path).await.map_err(|why| Ok(Failed::Refused(why)))?;
    let address = input(state, file).await.map_err(Err)?;
    let info = match state.transcoder.probe_now(&address).await {
        Ok(info) => info,
        // FFprobe read it and found no media: what the address answered
        // says why, when it wasn't media at all.
        Err(TranscoderError::Api { code, .. }) if code == "ffmpeg_failed" => {
            let asked = state.strm.fetch(&link, Some(&HeaderValue::from_static("bytes=0-0"))).await;
            return Err(Ok(asked.err().unwrap_or(Failed::NotMedia)));
        }
        Err(other) => return Err(Err(other.into())),
    };
    if !info
        .streams
        .iter()
        .any(|s| matches!(s.kind, pley_transcoder_api::StreamKind::Video | pley_transcoder_api::StreamKind::Audio))
    {
        return Err(Ok(Failed::NotMedia));
    }
    let size = link.media_len().and_then(|len| i64::try_from(len).ok()).unwrap_or(0);
    save(state, file, &link, &info, size).await.map_err(|err| Err(err.into()))
}

/// Keeps what `file`'s media was found to be, for as long as the file
/// names the address it was found at.
async fn save(state: &AppState, file: &FileRow, link: &Link, info: &MediaInfo, size: i64) -> sqlx::Result<FileRow> {
    let (video, audio) = (info.video(), info.default_audio());
    // Without a bitrate of the whole (an address that didn't say its size
    // when it was probed), the one its size and length make.
    let bitrate = info
        .bitrate
        .or_else(|| info.duration_ms.filter(|ms| *ms > 0 && size > 0).map(|ms| size.saturating_mul(8000) / ms));
    let json = serde_json::to_string(info).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    let now = crate::util::now();
    let mut tx = state.db.write.begin().await?;
    let kept = sqlx::query(
        "UPDATE media_files SET container = ?, duration_ms = ?, bitrate = ?, width = ?, height = ?, video_codec = ?,
            dynamic_range = ?, audio_codec = ?, media_info = ?, size = ?, updated_at = ?
         WHERE id = ? AND link_key = ?",
    )
    .bind(&info.container)
    .bind(info.duration_ms)
    .bind(bitrate)
    .bind(video.and_then(|v| v.width))
    .bind(video.and_then(|v| v.height))
    .bind(video.map(|v| v.codec.clone()))
    .bind(video.map(|v| v.dynamic_range().as_str()))
    .bind(audio.map(|a| a.codec.clone()))
    .bind(&json)
    .bind(size)
    .bind(now)
    .bind(file.id)
    .bind(link.target_key())
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    if kept {
        sqlx::query(
            "UPDATE items SET duration_ms = ?, updated_at = ? WHERE id = ? AND duration_ms IS NULL AND ? IS NOT NULL",
        )
        .bind(info.duration_ms)
        .bind(now)
        .bind(file.item_id)
        .bind(info.duration_ms)
        .execute(&mut *tx)
        .await?;
        // Listings show an item's files (sizes, bitrates, 4K and HDR badges).
        sqlx::query(
            "INSERT INTO listing_versions (scope, id, version) VALUES ('library', ?, 1)
             ON CONFLICT (scope, id) DO UPDATE SET version = version + 1",
        )
        .bind(file.library_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    if kept {
        tracing::info!("{}: its media is {} now", file.path, info.container.as_deref().unwrap_or("known"));
        state.events.send(crate::events::ServerEvent::LibraryChanged { library_id: file.library_id });
    }
    // As it is kept, or (a file changed since its scan) for this playback.
    let mut found = file.clone();
    found.container = info.container.clone();
    found.duration_ms = info.duration_ms;
    found.bitrate = bitrate;
    found.width = video.and_then(|v| v.width);
    found.height = video.and_then(|v| v.height);
    found.video_codec = video.map(|v| v.codec.clone());
    found.audio_codec = audio.map(|a| a.codec.clone());
    found.media_info = Some(json);
    found.size = size;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(text: &str) -> Result<String, Refused> {
        parse(text.as_bytes()).map(|target| target.url.to_string())
    }

    #[test]
    fn the_first_line_that_is_not_a_comment_is_the_address() {
        assert_eq!(address("https://media.example/film.mkv").unwrap(), "https://media.example/film.mkv");
        let commented = "\u{feff}#EXTM3U\r\n\r\n#KODIPROP:mimetype=video/mp4\r\n  http://nas.lan:8080/a%20b.mp4?t=1 \r\nhttp://other/\r\n";
        assert_eq!(address(commented).unwrap(), "http://nas.lan:8080/a%20b.mp4?t=1");
        assert_eq!(address("HTTPS://Media.Example/Film.mkv").unwrap(), "https://media.example/Film.mkv");
        assert_eq!(address("http://user:secret@host/dav/film.mkv").unwrap(), "http://user:secret@host/dav/film.mkv");
        for nothing in ["", "\n\n", "# only a comment\n"] {
            assert_eq!(address(nothing), Err(Refused::Empty), "{nothing:?}");
        }
    }

    #[test]
    fn only_http_and_https_addresses_are_read() {
        for other in [
            "/mnt/media/film.mkv",
            "C:\\Media\\film.mkv",
            "\\\\nas\\media\\film.mkv",
            "file:///etc/passwd",
            "../film.mkv",
            "film.mkv",
            "rtsp://camera/stream",
            "rtmp://host/live",
            "smb://nas/media/film.mkv",
            "ftp://host/film.mkv",
            "concat:http://a|http://b",
            "plugin://plugin.video.example/?id=1",
        ] {
            assert_eq!(address(other), Err(Refused::NotHttp), "{other}");
        }
        for broken in ["http://", "https://", "http://[::1/film.mkv", "http://host:port/"] {
            assert_eq!(address(broken), Err(Refused::NotAnAddress), "{broken}");
        }
        assert_eq!(parse(&[0xC3, 0x28, b'h']), Err(Refused::NotAnAddress), "not text");
        assert_eq!(address(&format!("http://host/{}", "a".repeat(MAX_ADDRESS))), Err(Refused::NotAnAddress));
    }

    #[test]
    fn notepads_unicode_is_read() {
        let text = "http://host/film.mkv\r\n";
        let mut le = vec![0xFF, 0xFE];
        le.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        let mut be = vec![0xFE, 0xFF];
        be.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        for bytes in [le, be] {
            assert_eq!(parse(&bytes).unwrap().url.as_str(), "http://host/film.mkv");
        }
    }

    #[test]
    fn kodis_headers_after_the_address_are_asked_with() {
        let target = parse(
            b"http://host/film.mkv|User-Agent=VLC%2F3.0&Referer=http://site/&Host=evil&Range=bytes%3D0-&seekable=0",
        )
        .unwrap();
        assert_eq!(target.url.as_str(), "http://host/film.mkv");
        let headers: Vec<(&str, &str)> =
            target.headers.iter().map(|(n, v)| (n.as_str(), v.to_str().unwrap())).collect();
        assert_eq!(headers, [("user-agent", "VLC/3.0"), ("referer", "http://site/")]);
        // A `|` that isn't Kodi's is the address's own.
        let plain = parse(b"http://host/a|b.mkv").unwrap();
        assert!(plain.headers.is_empty());
        assert_eq!(plain.url.path(), "/a|b.mkv");
        // Another address, or other headers: another key.
        assert_ne!(target.key(), parse(b"http://host/film.mkv").unwrap().key());
        assert_ne!(parse(b"http://host/film.mkv").unwrap().key(), parse(b"http://host/film2.mkv").unwrap().key());
        assert_eq!(target.key().len(), 32);
    }

    #[test]
    fn a_target_shows_its_host_only() {
        let target =
            parse(b"https://user:secret@media.example/dl/TOKEN/film.mkv?key=SECRET|Cookie=session%3DSECRET").unwrap();
        let shown = format!("{target:?}");
        assert_eq!(shown, "Target(https://media.example/…)");
        assert_eq!(target.extension().as_deref(), Some("mkv"));
        assert_eq!(parse(b"http://host/stream").unwrap().extension(), None);
        assert_eq!(parse(b"http://host/a.b/c.verylongext").unwrap().extension(), None);
    }

    #[test]
    fn an_address_written_in_numbers_is_told_apart_as_it_is() {
        for private in [
            "http://127.0.0.1:7539/api/settings",
            "http://192.168.1.10/film.mkv",
            "http://10.0.0.2/",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.64.0.1/",
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://[::ffff:192.168.1.10]/",
            "http://0.0.0.0/",
        ] {
            assert!(written_as_private(&Url::parse(private).unwrap()), "{private}");
        }
        for public in ["http://93.184.216.34/film.mkv", "https://[2606:4700::1111]/", "https://media.example/film.mkv"]
        {
            assert!(!written_as_private(&Url::parse(public).unwrap()), "{public}");
        }
    }

    /// The client for addresses on the internet follows a redirect to
    /// neither an address of this computer written in numbers nor one named
    /// by a name; the client for addresses of this network follows both.
    #[tokio::test]
    async fn the_internets_client_follows_no_redirect_inside() {
        use axum::routing::get;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let to = |host: &str| {
            let location = format!("http://{host}:{}/inside", addr.port());
            get(move || async move { (StatusCode::FOUND, [(header::LOCATION, location)]) })
        };
        let app = axum::Router::new()
            .route("/numbers", to("127.0.0.1"))
            .route("/name", to("localhost"))
            .route("/inside", get(|| async { "of this network" }));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let inside = Arc::new(Mutex::new(HashSet::new()));
        let (public, any) = (http_client(None), http_client(Some(inside.clone())));
        for through in ["numbers", "name"] {
            let url = format!("http://{addr}/{through}");
            let refused = public.get(&url).send().await;
            assert!(refused.is_err(), "{through}: {:?}", refused.map(|r| r.status()));
            let followed = any.get(&url).send().await.unwrap();
            assert_eq!(followed.text().await.unwrap(), "of this network", "{through}");
        }
        // A name held to this network still resolves, to this network.
        inside.lock().insert("localhost".into());
        let held = any.get(format!("http://localhost:{}/inside", addr.port())).send().await.unwrap();
        assert_eq!(held.text().await.unwrap(), "of this network");
    }

    #[test]
    fn one_range_of_bytes_is_passed_on() {
        for range in ["bytes=0-", "bytes=10-20", "bytes=-500", " bytes=5-5"] {
            assert!(is_byte_range(&HeaderValue::from_static(range)), "{range}");
        }
        for other in ["bytes=-", "bytes=0-1,5-6", "bytes=a-b", "items=0-1", "bytes=0", "bytes=99999999999999999999-"] {
            assert!(!is_byte_range(&HeaderValue::from_static(other)), "{other}");
        }
        assert_eq!(content_range("bytes 0-9/100"), Some((Some((0, 9)), Some(100))));
        assert_eq!(content_range("bytes 5-9/*"), Some((Some((5, 9)), None)));
        assert_eq!(content_range("bytes */100"), Some((None, Some(100))));
        for bad in ["bytes 9-0/100", "0-9/100", "bytes 0-9", "bytes a-b/c"] {
            assert_eq!(content_range(bad), None, "{bad}");
        }
    }
}
