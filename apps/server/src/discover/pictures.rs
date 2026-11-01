//! Discover's pictures from pley's own address: TMDB's posters, backdrops,
//! cast photos and stills, fetched at the size a client draws them and kept
//! as files in `<data>/cache/discover/<size>/<file>` (docs/STORAGE.md), with
//! no database row. No client contacts TMDB for Discover, so no token goes
//! there, and TVs and phones never pull TMDB's originals.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use parking_lot::Mutex;
use tokio::sync::Semaphore;

/// From this width a client gets TMDB's original (none asks for that: their
/// widths stop at 1920).
const ORIGINAL_FROM: u32 = 2000;
/// How long TMDB's "no such picture" is believed.
const MISSING_FOR: Duration = Duration::from_secs(24 * 3600);
/// Pictures remembered missing at most: past it, those believed long
/// enough go (all of them, should none be).
const MISSING_MAX: usize = 10_000;
/// A picture is this large at most.
const MAX_BYTES: usize = 10 * 1024 * 1024;
/// How long TMDB's picture server has to answer.
const TIMEOUT: Duration = Duration::from_secs(20);
/// Pictures fetched at once (each held whole until kept), and fetched or
/// waiting to be at most: past that, more fail until some are done.
const FETCHES_AT_ONCE: usize = 8;
const FETCHES_MAX: usize = 256;
/// What the pictures kept may take: past it, the oldest go, looked over
/// once an hour at most, after a fetch.
const CACHE_MAX_BYTES: u64 = 256 << 20;
const PRUNED_EVERY: Duration = Duration::from_secs(3600);

/// What a picture shows, each with the sizes TMDB makes of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picture {
    Poster,
    Backdrop,
    /// A cast member's photo.
    Profile,
    /// An episode's.
    Still,
}

impl Picture {
    pub fn parse(text: &str) -> Option<Picture> {
        match text {
            "poster" => Some(Picture::Poster),
            "backdrop" => Some(Picture::Backdrop),
            "profile" => Some(Picture::Profile),
            "still" => Some(Picture::Still),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Picture::Poster => "poster",
            Picture::Backdrop => "backdrop",
            Picture::Profile => "profile",
            Picture::Still => "still",
        }
    }

    /// TMDB's sizes, smallest first, with the width each stands for (a
    /// profile's `h632` is 421 pixels wide).
    fn sizes(self) -> &'static [(&'static str, u32)] {
        match self {
            Picture::Poster => {
                &[("w92", 92), ("w154", 154), ("w185", 185), ("w342", 342), ("w500", 500), ("w780", 780)]
            }
            Picture::Backdrop => &[("w300", 300), ("w780", 780), ("w1280", 1280)],
            Picture::Profile => &[("w45", 45), ("w185", 185), ("h632", 421)],
            Picture::Still => &[("w92", 92), ("w185", 185), ("w300", 300)],
        }
    }

    /// The size fetched for a client drawing it `width` pixels wide: the
    /// smallest at least that wide, the largest when none is, the original
    /// only from [`ORIGINAL_FROM`]; without a width, the one most screens
    /// draw.
    pub fn size(self, width: Option<u32>) -> &'static str {
        let Some(width) = width else {
            return match self {
                Picture::Poster => "w342",
                Picture::Backdrop => "w1280",
                Picture::Profile => "w185",
                Picture::Still => "w300",
            };
        };
        if width >= ORIGINAL_FROM {
            return "original";
        }
        let sizes = self.sizes();
        sizes.iter().find(|(_, wide)| *wide >= width).or(sizes.last()).map_or("original", |(size, _)| size)
    }
}

/// Where pley serves TMDB's picture `path` (`/abc.jpg`): `None` without one,
/// or for a name pley wouldn't serve.
pub fn url(kind: Picture, path: Option<&str>) -> Option<String> {
    let file = path?.strip_prefix('/')?;
    valid_file(file).then(|| format!("/api/discover/images/{}/{file}", kind.name()))
}

/// TMDB's file names: up to 64 letters and digits, then `.jpg` or `.png`.
/// Nothing else is fetched or turned into a path (TMDB's pictures are never
/// SVG, and pley serves no SVG from its own address).
pub fn valid_file(file: &str) -> bool {
    let Some((name, ext)) = file.rsplit_once('.') else { return false };
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric()) && matches!(ext, "jpg" | "png")
}

/// Why a picture can't be had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// TMDB has no such picture (believed for [`MISSING_FOR`]).
    Missing,
    /// TMDB didn't answer, or not with a picture: nothing was kept.
    Failed,
}

/// A picture being fetched, shared by everyone who asks for it meanwhile.
type Fetch = Shared<BoxFuture<'static, Result<Bytes, Unavailable>>>;

pub struct Pictures {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    /// TMDB's picture server; `None` fetches nothing (tests without one).
    base: Option<String>,
    /// `<data>/cache/discover`.
    dir: PathBuf,
    /// `<size>/<file>` TMDB said it doesn't have, and when.
    missing: DashMap<String, Instant>,
    /// Fetches under way or waiting for [`Self::fetches`], by
    /// `<size>/<file>`, with their numbers.
    fetching: DashMap<String, (u64, Fetch)>,
    next: AtomicU64,
    /// [`FETCHES_AT_ONCE`].
    fetches: Semaphore,
    /// When the pictures kept were last looked over.
    pruned: Mutex<Option<Instant>>,
}

impl Pictures {
    pub fn new(dir: PathBuf, base: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .build()
            .expect("http client");
        let base = base.map(|base| base.trim_end_matches('/').to_owned());
        Self {
            inner: Arc::new(Inner {
                http,
                base,
                dir,
                missing: DashMap::new(),
                fetching: DashMap::new(),
                next: AtomicU64::new(0),
                fetches: Semaphore::new(FETCHES_AT_ONCE),
                pruned: Mutex::new(None),
            }),
        }
    }

    /// `file` (a [`valid_file`]) at `size` (one of [`Picture::size`]'s):
    /// from the cache, or from TMDB once however many ask for it at once,
    /// [`FETCHES_AT_ONCE`] pictures at a time.
    pub async fn get(&self, size: &'static str, file: &str) -> Result<Bytes, Unavailable> {
        let inner = &self.inner;
        let path = inner.dir.join(size).join(file);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(Bytes::from(bytes));
        }
        let key = format!("{size}/{file}");
        if inner.missing.get(&key).is_some_and(|at| at.elapsed() < MISSING_FOR) {
            return Err(Unavailable::Missing);
        }
        let Some(base) = &inner.base else { return Err(Unavailable::Failed) };
        if inner.fetching.len() >= FETCHES_MAX && !inner.fetching.contains_key(&key) {
            tracing::debug!("a TMDB picture: {FETCHES_MAX} are being fetched or waiting to be already");
            return Err(Unavailable::Failed);
        }
        let fetch = inner
            .fetching
            .entry(key.clone())
            .or_insert_with(|| {
                let id = inner.next.fetch_add(1, Ordering::Relaxed);
                let (shared, url, key) = (inner.clone(), format!("{base}/{key}"), key.clone());
                // On its own: it goes on to the cache when nobody waits any
                // more (a client that scrolled on).
                let task = tokio::spawn(async move {
                    let fetched = {
                        let _turn = shared.fetches.acquire().await;
                        download(&shared.http, &url, &path).await
                    };
                    if fetched == Err(Unavailable::Missing) {
                        shared.remember_missing(&key);
                    }
                    shared.fetching.remove_if(&key, |_, (other, _)| *other == id);
                    if fetched.is_ok() {
                        shared.prune();
                    }
                    fetched
                });
                let fetch = async move { task.await.unwrap_or(Err(Unavailable::Failed)) };
                (id, fetch.boxed().shared())
            })
            .1
            .clone();
        fetch.await
    }
}

impl Inner {
    /// Lets the oldest pictures kept go while they take more than
    /// [`CACHE_MAX_BYTES`], in the background; once an hour at most.
    fn prune(&self) {
        {
            let mut last = self.pruned.lock();
            if last.is_some_and(|at| at.elapsed() < PRUNED_EVERY) {
                return;
            }
            *last = Some(Instant::now());
        }
        let dir = self.dir.clone();
        tokio::spawn(async move {
            // However old: Store::clean removes those older than 30 days.
            match crate::util::blocking(move || crate::util::prune(&dir, Duration::MAX, CACHE_MAX_BYTES)).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("{n} of Discover's pictures let go"),
                Err(err) => tracing::warn!("looking over Discover's pictures failed: {err:#}"),
            }
        });
    }

    fn remember_missing(&self, key: &str) {
        if self.missing.len() >= MISSING_MAX {
            self.missing.retain(|_, at| at.elapsed() < MISSING_FOR);
            if self.missing.len() >= MISSING_MAX {
                self.missing.clear();
            }
        }
        self.missing.insert(key.to_owned(), Instant::now());
    }
}

/// Fetches a picture from `url` and keeps it at `path`, whole or not at all
/// (a temporary file renamed into place).
async fn download(http: &reqwest::Client, url: &str, path: &Path) -> Result<Bytes, Unavailable> {
    let failed = |why: String| {
        tracing::debug!("a TMDB picture: {why}");
        Unavailable::Failed
    };
    let mut resp = http.get(url).send().await.map_err(|e| failed(e.without_url().to_string()))?;
    match resp.status().as_u16() {
        200 => {}
        404 => return Err(Unavailable::Missing),
        status => return Err(failed(format!("HTTP {status}"))),
    }
    if resp.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
        return Err(failed("too large".into()));
    }
    // Up to the limit, whatever the answer said its size was.
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| failed(e.without_url().to_string()))? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(failed("too large".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    if !matches!(crate::store::picture_ext(&bytes), Some("jpg" | "png")) {
        return Err(failed("not a JPEG or PNG picture".into()));
    }
    if let Some(dir) = path.parent() {
        let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
        let written = async {
            tokio::fs::create_dir_all(dir).await?;
            tokio::fs::write(&tmp, &bytes).await?;
            tokio::fs::rename(&tmp, path).await
        }
        .await;
        if let Err(err) = written {
            let _ = tokio::fs::remove_file(&tmp).await;
            tracing::debug!("a TMDB picture not cached in {}: {err}", path.display());
        }
    }
    Ok(Bytes::from(bytes))
}

/// The Content-Type of a picture [`Pictures::get`] gave.
pub fn content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_tmdbs_at_or_above_the_width_drawn() {
        assert_eq!(Picture::Poster.size(None), "w342");
        assert_eq!(Picture::Poster.size(Some(1)), "w92");
        assert_eq!(Picture::Poster.size(Some(185)), "w185");
        assert_eq!(Picture::Poster.size(Some(186)), "w342");
        assert_eq!(Picture::Poster.size(Some(1920)), "w780", "the largest named size");
        assert_eq!(Picture::Poster.size(Some(2000)), "original");
        assert_eq!(Picture::Backdrop.size(None), "w1280");
        assert_eq!(Picture::Backdrop.size(Some(781)), "w1280");
        assert_eq!(Picture::Profile.size(None), "w185");
        assert_eq!(Picture::Profile.size(Some(300)), "h632", "421 pixels wide");
        assert_eq!(Picture::Profile.size(Some(422)), "h632");
        assert_eq!(Picture::Still.size(Some(200)), "w300");
        for kind in ["poster", "backdrop", "profile", "still"] {
            assert_eq!(Picture::parse(kind).map(Picture::name), Some(kind));
        }
        assert_eq!(Picture::parse("logo"), None);
    }

    #[test]
    fn only_tmdbs_file_names_are_served() {
        assert_eq!(
            url(Picture::Poster, Some("/abc123.jpg")).as_deref(),
            Some("/api/discover/images/poster/abc123.jpg")
        );
        assert_eq!(url(Picture::Profile, Some("/X.png")).as_deref(), Some("/api/discover/images/profile/X.png"));
        assert_eq!(url(Picture::Poster, None), None);
        for bad in ["abc.jpg", "/abc.svg", "/../x.jpg", "/a b.jpg", "/.jpg", "/abc.jpg.svg", "/abc.JPG", "/é.jpg"] {
            assert_eq!(url(Picture::Poster, Some(bad)), None, "{bad}");
        }
        assert!(valid_file(&format!("{}.jpg", "a".repeat(64))));
        assert!(!valid_file(&format!("{}.jpg", "a".repeat(65))));
        assert_eq!(content_type(&[0x89, b'P', b'N', b'G', 13, 10]), "image/png");
        assert_eq!(content_type(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
    }

    #[tokio::test]
    async fn fetches_wait_their_turn_up_to_a_limit() {
        use std::sync::atomic::AtomicUsize;
        // A picture server that holds every answer until let go.
        let (go, gate) = tokio::sync::watch::channel(false);
        let asked = Arc::new(AtomicUsize::new(0));
        let answer = {
            let asked = asked.clone();
            move || {
                let (asked, mut gate) = (asked.clone(), gate.clone());
                async move {
                    asked.fetch_add(1, Ordering::SeqCst);
                    let _ = gate.wait_for(|go| *go).await;
                    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];
                    ([(axum::http::header::CONTENT_TYPE, "image/jpeg")], jpeg)
                }
            }
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, axum::Router::new().fallback(answer)).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let pictures = Arc::new(Pictures::new(dir.path().to_owned(), Some(base)));

        let fetches: Vec<_> = (0..FETCHES_MAX)
            .map(|n| {
                let pictures = pictures.clone();
                tokio::spawn(async move { pictures.get("w92", &format!("p{n}.jpg")).await })
            })
            .collect();
        let started = async {
            while pictures.inner.fetching.len() < FETCHES_MAX || asked.load(Ordering::SeqCst) < FETCHES_AT_ONCE {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), started).await.expect("every fetch under way");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(asked.load(Ordering::SeqCst), FETCHES_AT_ONCE, "the others wait their turn");
        assert_eq!(pictures.get("w92", "more.jpg").await, Err(Unavailable::Failed), "too many waiting");
        go.send(true).unwrap();
        for fetch in fetches {
            assert!(fetch.await.unwrap().is_ok());
        }
        assert_eq!(asked.load(Ordering::SeqCst), FETCHES_MAX);
    }
}
