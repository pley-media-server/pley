//! Where pley keeps the files it downloads or makes (docs/STORAGE.md).
//!
//! - **Blobs** (`<data>/metadata/blobs/ab/abcd….jpg`): pictures that are
//!   metadata — downloaded artwork, uploads, embedded album covers, people's
//!   photos. Stored once, named by the SHA-256 of their content, and
//!   referenced from the database as `blob:<name>`: the same picture is never
//!   stored twice and moving the data folder breaks nothing. Downloads
//!   remember their URL ([`Store::remote`]), so refreshes don't fetch them
//!   again.
//! - **Media folders** (`<data>/media/ab/<fingerprint>/`): files generated
//!   from a media file — frame grabs, seek-bar previews, chapter images. Keyed
//!   by the file's fingerprint (its OpenSubtitles hash), so they follow the
//!   file when it is moved or renamed.
//! - Local artwork next to the media (`poster.jpg`, …) stays where it is and
//!   is referenced by its path.
//!
//! [`Store::clean`] removes whatever nothing references any more, and the
//! files of `<data>/cache` once they are old.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, bail};
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::util::{now, path_str};

const BLOB_PREFIX: &str = "blob:";
/// Largest picture accepted from the internet.
const MAX_REMOTE_BYTES: usize = 25 * 1024 * 1024;
/// Files younger than this, and blobs and media folders handed out within it,
/// are never cleaned up: their database row may not be committed yet.
const GRACE: Duration = Duration::from_secs(3600);
/// Bytes hashed at each end of a media file for its fingerprint.
const FINGERPRINT_CHUNK: u64 = 64 * 1024;
/// Fingerprints that could not be saved in a row before a run gives up.
const MAX_FINGERPRINT_FAILURES: u32 = 3;
const DAY: u64 = 24 * 3600;
/// Caches under `<data>/cache`, and how long their files are kept after they
/// were made. Their names change with what they were made from (a path, a
/// size, a modification time), so the files of media that changed or went
/// are never asked for again. Resized pictures are quick to make again;
/// subtitles extracted from a large file and audio fingerprints are not.
const CACHES: &[(&str, Duration)] = &[
    ("images", Duration::from_secs(30 * DAY)),       // images.rs: resized pictures
    ("covers", Duration::from_secs(30 * DAY)),       // api/items.rs: albums without embedded covers
    ("subtitles", Duration::from_secs(90 * DAY)),    // api/playback.rs: WebVTT
    ("fingerprints", Duration::from_secs(90 * DAY)), // markers: audio fingerprints
    ("bif", Duration::from_secs(30 * DAY)),          // trickplay.rs: Plex's BIF indexes of seek previews
    ("discover", Duration::from_secs(30 * DAY)),     // discover/pictures.rs: TMDB's pictures
];

/// Hosts pictures may be downloaded from: metadata providers' image servers,
/// and those library tools take artwork from (fanart.tv, ThePosterDB, and
/// GitHub's raw files for Kometa's own). Public hosts only, so that no
/// address can turn a download towards this network.
const IMAGE_HOSTS: &[&str] = &[
    "image.tmdb.org",
    "static.tvmaze.com",
    "artworks.thetvdb.com",
    "coverartarchive.org",
    "assets.fanart.tv",
    "theposterdb.com",
    "raw.githubusercontent.com",
];

/// Whether a stored reference is a blob (as opposed to a local file path).
pub fn is_blob(reference: &str) -> bool {
    reference.starts_with(BLOB_PREFIX)
}

/// `<64 hex>.<ext>`: nothing else is ever turned into a path.
fn valid_blob_name(name: &str) -> bool {
    let Some((hash, ext)) = name.split_once('.') else { return false };
    hash.len() == 64
        && hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && matches!(ext, "jpg" | "png" | "webp" | "gif")
}

/// Extension for picture bytes, `None` when they are not a picture pley serves.
pub fn picture_ext(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Jpeg => Some("jpg"),
        image::ImageFormat::Png => Some("png"),
        image::ImageFormat::WebP => Some("webp"),
        image::ImageFormat::Gif => Some("gif"),
        _ => None,
    }
}

/// Whether pley may download a picture from `url`: https on a known image host.
pub fn allowed_image_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    IMAGE_HOSTS.contains(&host)
}

/// The OpenSubtitles hash of a file: its size plus the sum of the 64-bit
/// little-endian words of its first and last 64 KiB, as 16 hex digits.
pub fn fingerprint(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut hash = size;
    let mut add = |buf: &[u8]| {
        for word in buf.chunks(8) {
            let mut bytes = [0u8; 8];
            bytes[..word.len()].copy_from_slice(word);
            hash = hash.wrapping_add(u64::from_le_bytes(bytes));
        }
    };
    let chunk = FINGERPRINT_CHUNK.min(size) as usize;
    let mut buf = vec![0u8; chunk];
    file.read_exact(&mut buf)?;
    add(&buf);
    file.seek(SeekFrom::Start(size - chunk as u64))?;
    file.read_exact(&mut buf)?;
    add(&buf);
    Ok(format!("{hash:016x}"))
}

fn valid_fingerprint(fp: &str) -> bool {
    fp.len() == 16 && fp.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Usage {
    pub blobs: u64,
    pub blob_bytes: u64,
    pub media_folders: u64,
    pub media_bytes: u64,
    pub cache_bytes: u64,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct CleanReport {
    pub blobs_removed: u64,
    pub media_folders_removed: u64,
    /// By the blobs and media folders.
    pub bytes_freed: u64,
    /// Old files of the caches ([`CACHES`]).
    pub cache_files_removed: u64,
    pub cache_bytes_freed: u64,
    /// Added subtitles whose media file went (counted in `bytes_freed`).
    pub subtitles_removed: u64,
}

pub struct Store {
    db: Db,
    blobs: PathBuf,
    media: PathBuf,
    cache: PathBuf,
    /// Added subtitles ([`crate::subtitles`]), beside `media`.
    subtitles: PathBuf,
    http: reqwest::Client,
    cleaning: tokio::sync::Mutex<()>,
    fingerprinting: tokio::sync::Mutex<()>,
    handed_out: Arc<Mutex<HandedOut>>,
}

/// Blobs and media folders handed out lately: a picture stored, stored again
/// or found by its URL, a media folder asked for, a fingerprint computed.
/// Whoever got them may not have saved their reference yet, so the clean-up
/// spares them for the grace period, however old their files are.
#[derive(Debug, Default)]
struct HandedOut {
    /// Blob file name or fingerprint → when it was last handed out.
    at: HashMap<String, Instant>,
    /// Size at which entries older than the grace period are dropped.
    prune_at: usize,
}

impl HandedOut {
    fn add(&mut self, name: &str) {
        let now = Instant::now();
        self.at.insert(name.to_owned(), now);
        if self.at.len() >= self.prune_at {
            self.at.retain(|_, at| now.duration_since(*at) < GRACE);
            self.prune_at = (self.at.len() * 2).max(1024);
        }
    }

    /// Whether `name` was handed out at or after `since` (`None`: ever).
    fn since(&self, name: &str, since: Option<Instant>) -> bool {
        self.at.get(name).is_some_and(|at| since.is_none_or(|since| *at >= since))
    }
}

impl Store {
    pub fn new(db: Db, metadata_dir: &Path, media_dir: &Path, cache_dir: &Path) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            // A redirect stays on the picture hosts, over HTTPS, as the first
            // address must ([`allowed_image_url`]): never to this network.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 3 && allowed_image_url(attempt.url().as_str()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            // A compressed answer's size says nothing of what it unpacks to.
            .no_gzip()
            .build()
            .expect("http client");
        Self {
            db,
            blobs: metadata_dir.join("blobs"),
            media: media_dir.to_path_buf(),
            cache: cache_dir.to_path_buf(),
            subtitles: media_dir.with_file_name("subtitles"),
            http,
            cleaning: tokio::sync::Mutex::new(()),
            fingerprinting: tokio::sync::Mutex::new(()),
            handed_out: Arc::default(),
        }
    }

    /// Remember that a blob (by file name) or a media folder (by
    /// fingerprint) was handed out. Callers do this before they look at the
    /// file: a clean-up running meanwhile then either removed it already
    /// (and it is made again) or sees it handed out and spares it.
    fn hand_out(&self, name: &str) {
        self.handed_out.lock().add(name);
    }

    /// The file behind a reference: a blob in the store, or a local file. A
    /// local file that became a link to anything but a picture since the
    /// scan found it (`poster.jpg` to pley's database) is no file.
    pub fn path(&self, reference: &str) -> Option<PathBuf> {
        match reference.strip_prefix(BLOB_PREFIX) {
            Some(name) => valid_blob_name(name).then(|| self.blobs.join(&name[..2]).join(name)),
            None => Some(PathBuf::from(reference)).filter(|path| crate::library::walk::leads_to_its_kind(path)),
        }
    }

    /// Whether the file behind a reference is there.
    pub fn exists(&self, reference: &str) -> bool {
        self.path(reference).is_some_and(|p| p.is_file())
    }

    /// Store a picture; returns its reference. Storing the same bytes twice
    /// keeps one copy.
    pub async fn put(&self, bytes: &[u8], ext: &str) -> std::io::Result<String> {
        let hash = hex::encode(Sha256::digest(bytes));
        let name = format!("{hash}.{ext}");
        let dir = self.blobs.join(&hash[..2]);
        let path = dir.join(&name);
        // A picture stored long ago and unused since may be in a clean-up's
        // way right now: it must stay until the caller saved its reference.
        self.hand_out(&name);
        if !path.is_file() {
            tokio::fs::create_dir_all(&dir).await?;
            let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
            tokio::fs::write(&tmp, bytes).await?;
            if let Err(err) = tokio::fs::rename(&tmp, &path).await {
                let _ = tokio::fs::remove_file(&tmp).await;
                // Someone else stored the same picture meanwhile.
                if !path.is_file() {
                    return Err(err);
                }
            }
        }
        Ok(format!("{BLOB_PREFIX}{name}"))
    }

    /// A picture from the internet, downloaded the first time and reused
    /// after that (image URLs of metadata providers change with the picture).
    pub async fn remote(&self, url: &str) -> anyhow::Result<String> {
        if let Some(blob) = self.known_remote(url).await? {
            return Ok(blob);
        }
        let bytes = self.fetch(url).await?;
        let ext = picture_ext(&bytes).with_context(|| format!("{url} is not a picture"))?;
        let blob = self.put(&bytes, ext).await?;
        sqlx::query("INSERT OR REPLACE INTO remote_images (url, blob, fetched_at) VALUES (?, ?, ?)")
            .bind(url)
            .bind(&blob)
            .bind(now())
            .execute(&self.db.write)
            .await?;
        Ok(blob)
    }

    /// The bytes of a picture from the internet, as [`Self::remote`] gets
    /// them but not stored: a picture someone chose by its address. Some of
    /// the hosts serve anyone's files, so the caller decodes and re-encodes
    /// it as an upload before storing it.
    pub async fn download(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        if let Some(path) = self.known_remote(url).await?.and_then(|blob| self.path(&blob)) {
            return Ok(tokio::fs::read(path).await?);
        }
        self.fetch(url).await
    }

    /// The blob a picture from `url` was stored as, if it still is.
    async fn known_remote(&self, url: &str) -> anyhow::Result<Option<String>> {
        let known: Option<String> = sqlx::query_scalar("SELECT blob FROM remote_images WHERE url = ?")
            .bind(url)
            .fetch_optional(&self.db.read)
            .await?;
        let Some(blob) = known else { return Ok(None) };
        if let Some(name) = blob.strip_prefix(BLOB_PREFIX) {
            self.hand_out(name);
        }
        Ok(self.exists(&blob).then_some(blob))
    }

    /// Downloads `url` from one of the image hosts, up to [`MAX_REMOTE_BYTES`].
    async fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        if !allowed_image_url(url) {
            bail!("pictures are not downloaded from {url}");
        }
        let mut resp = self.http.get(url).send().await?.error_for_status()?;
        if resp.content_length().is_some_and(|n| n as usize > MAX_REMOTE_BYTES) {
            bail!("{url} is too large");
        }
        // Read up to the limit, whatever the answer said its size was.
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            anyhow::ensure!(bytes.len() + chunk.len() <= MAX_REMOTE_BYTES, "{url} is too large");
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    /// Folder of files generated from the media file with this fingerprint.
    pub fn media_dir(&self, fingerprint: &str) -> Option<PathBuf> {
        if !valid_fingerprint(fingerprint) {
            return None;
        }
        self.hand_out(fingerprint);
        Some(self.media.join(&fingerprint[..2]).join(fingerprint))
    }

    /// The fingerprint of a media file, computed (and saved) when missing.
    pub async fn fingerprint_of(&self, file_id: i64, path: &str, known: Option<&str>) -> anyhow::Result<String> {
        if let Some(fp) = known.filter(|f| valid_fingerprint(f)) {
            return Ok(fp.to_owned());
        }
        let owned = PathBuf::from(path);
        let fp = crate::util::blocking(move || Ok(fingerprint(&owned)?)).await?;
        // A moved file finds its old media folder again.
        self.hand_out(&fp);
        sqlx::query("UPDATE media_files SET fingerprint = ? WHERE id = ?")
            .bind(&fp)
            .bind(file_id)
            .execute(&self.db.write)
            .await?;
        Ok(fp)
    }

    /// Fingerprint media files scanned since the last run, one at a time.
    ///
    /// Each pending file is read once per run, whether or not its fingerprint
    /// could be saved: when saving fails (a full disk, a read-only or damaged
    /// database) the run stops after a few failures in a row instead of
    /// reading the same files again and again, and the next run (after the
    /// next scan, or the next start) tries again.
    pub async fn fingerprint_pending(&self) {
        // One run at a time: start-up and every scan ask for one.
        let _running = self.fingerprinting.lock().await;
        let (mut after, mut done, mut failed, mut in_a_row) = (0i64, 0u64, 0u64, 0u32);
        let mut last_error = None;
        'run: loop {
            let batch: Vec<(i64, String, i64, i64)> = match sqlx::query_as(
                "SELECT id, path, size, mtime FROM media_files
                 WHERE fingerprint IS NULL AND id > ? ORDER BY id LIMIT 200",
            )
            .bind(after)
            .fetch_all(&self.db.read)
            .await
            {
                Ok(batch) => batch,
                Err(err) => {
                    tracing::warn!("fingerprinting media files: {err}");
                    return;
                }
            };
            if batch.is_empty() {
                break;
            }
            for (id, path, size, mtime) in batch {
                after = id;
                let owned = PathBuf::from(&path);
                // Unreadable files (offline drives) get an empty marker so the
                // loop moves on; the next scan that sees them changed resets it.
                let fp = crate::util::blocking(move || Ok(fingerprint(&owned)?)).await.unwrap_or_default();
                if valid_fingerprint(&fp) {
                    self.hand_out(&fp);
                }
                let saved =
                    sqlx::query("UPDATE media_files SET fingerprint = ? WHERE id = ? AND size = ? AND mtime = ?")
                        .bind(&fp)
                        .bind(id)
                        .bind(size)
                        .bind(mtime)
                        .execute(&self.db.write)
                        .await;
                match saved {
                    Ok(_) => {
                        done += 1;
                        in_a_row = 0;
                    }
                    Err(err) => {
                        failed += 1;
                        in_a_row += 1;
                        last_error = Some(err);
                        if in_a_row >= MAX_FINGERPRINT_FAILURES {
                            break 'run;
                        }
                    }
                }
            }
        }
        if let Some(err) = last_error {
            tracing::warn!(
                "could not save the fingerprints of {failed} media files ({err}); they are tried again after the next scan"
            );
        }
        if done > 0 {
            tracing::debug!("fingerprinted {done} media files");
        }
    }

    /// Every blob the database still uses.
    async fn referenced_blobs(&self) -> sqlx::Result<HashSet<String>> {
        let queries = [
            "SELECT poster FROM items WHERE poster LIKE 'blob:%'
             UNION SELECT backdrop FROM items WHERE backdrop LIKE 'blob:%'
             UNION SELECT thumb FROM items WHERE thumb LIKE 'blob:%'",
            "SELECT value FROM item_overrides WHERE value LIKE 'blob:%'",
            "SELECT poster FROM collections WHERE poster LIKE 'blob:%'
             UNION SELECT backdrop FROM collections WHERE backdrop LIKE 'blob:%'",
            // People's photos are only known by URL.
            "SELECT r.blob FROM remote_images r JOIN people p ON r.url = p.photo_url",
        ];
        let mut out = HashSet::new();
        for sql in queries {
            let rows: Vec<String> = sqlx::query_scalar(sql).fetch_all(&self.db.read).await?;
            out.extend(rows);
        }
        Ok(out)
    }

    /// Remove blobs and media folders nothing references any more, and old
    /// cache files ([`CACHES`]).
    pub async fn clean(&self) -> anyhow::Result<CleanReport> {
        self.clean_older_than(GRACE).await
    }

    /// [`Self::clean`], sparing files younger than `grace` and what was
    /// handed out within `grace` of the start (or since).
    pub async fn clean_older_than(&self, grace: Duration) -> anyhow::Result<CleanReport> {
        let _running = self.cleaning.lock().await;
        // What is handed out from here on may be referenced after the
        // snapshot of references below: the sweeps spare it.
        let since = Instant::now().checked_sub(grace);
        let mut report = CleanReport::default();
        let referenced = self.referenced_blobs().await?;
        // Downloads nothing uses any more are forgotten with their pictures,
        // once they are as old as the grace period too.
        let fetched_before = now() - i64::try_from(grace.as_secs()).unwrap_or(i64::MAX);
        let remote: Vec<(String, String, i64)> =
            sqlx::query_as("SELECT url, blob, fetched_at FROM remote_images").fetch_all(&self.db.read).await?;
        let forget: Vec<String> = {
            let handed_out = self.handed_out.lock();
            remote
                .into_iter()
                .filter(|(_, blob, fetched_at)| {
                    !referenced.contains(blob)
                        && *fetched_at <= fetched_before
                        && !handed_out.since(blob.strip_prefix(BLOB_PREFIX).unwrap_or(blob), since)
                })
                .map(|(url, ..)| url)
                .collect()
        };
        for chunk in forget.chunks(500) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("DELETE FROM remote_images WHERE url IN ({placeholders})");
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            for url in chunk {
                query = query.bind(url);
            }
            query.execute(&self.db.write).await?;
        }
        let names: HashSet<String> =
            referenced.iter().filter_map(|r| r.strip_prefix(BLOB_PREFIX)).map(str::to_owned).collect();
        let (blobs, handed_out) = (self.blobs.clone(), self.handed_out.clone());
        let (count, bytes) =
            crate::util::blocking(move || Ok(sweep(&blobs, grace, |name| names.contains(name), &handed_out, since)))
                .await?;
        report.blobs_removed = count;
        report.bytes_freed += bytes;

        let fingerprints: HashSet<String> =
            sqlx::query_scalar("SELECT DISTINCT fingerprint FROM media_files WHERE fingerprint IS NOT NULL")
                .fetch_all(&self.db.read)
                .await?
                .into_iter()
                .collect();
        let (media, handed_out) = (self.media.clone(), self.handed_out.clone());
        let (count, bytes) = crate::util::blocking(move || {
            Ok(sweep(&media, grace, |name| fingerprints.contains(name), &handed_out, since))
        })
        .await?;
        report.media_folders_removed = count;
        report.bytes_freed += bytes;

        // Added subtitles go with their media file's row; their files here.
        let listed: HashSet<i64> =
            sqlx::query_scalar("SELECT id FROM added_subtitles").fetch_all(&self.db.read).await?.into_iter().collect();
        let subtitles = self.subtitles.clone();
        let (count, bytes) =
            crate::util::blocking(move || Ok(crate::subtitles::sweep(&subtitles, &listed, grace))).await?;
        report.subtitles_removed = count;
        report.bytes_freed += bytes;

        let cache = self.cache.clone();
        let (count, bytes) = crate::util::blocking(move || {
            Ok(CACHES.iter().fold((0, 0), |(count, bytes), (name, max_age)| {
                let (n, b) = prune(&cache.join(name), *max_age);
                (count + n, bytes + b)
            }))
        })
        .await?;
        report.cache_files_removed = count;
        report.cache_bytes_freed = bytes;
        if report.blobs_removed + report.media_folders_removed + report.cache_files_removed > 0 {
            tracing::info!(
                "storage cleaned: {} pictures, {} media folders and {} old cache files removed, {} MB freed",
                report.blobs_removed,
                report.media_folders_removed,
                report.cache_files_removed,
                (report.bytes_freed + report.cache_bytes_freed) / 1_000_000
            );
        }
        Ok(report)
    }

    /// Space used by blobs, media folders and the cache.
    pub async fn usage(&self) -> anyhow::Result<Usage> {
        let (blobs, media, cache) = (self.blobs.clone(), self.media.clone(), self.cache.clone());
        crate::util::blocking(move || {
            let (blob_count, blob_bytes) = tree_size(&blobs);
            let media_folders = std::fs::read_dir(&media)
                .map(|shards| {
                    shards.flatten().map(|s| std::fs::read_dir(s.path()).map(|d| d.count() as u64).unwrap_or(0)).sum()
                })
                .unwrap_or(0);
            Ok(Usage {
                blobs: blob_count,
                blob_bytes,
                media_folders,
                media_bytes: tree_size(&media).1,
                cache_bytes: tree_size(&cache).1,
            })
        })
        .await
    }

    /// Move pictures kept by earlier versions (absolute paths under
    /// `metadata/items`, `metadata/collections` and `metadata/people`) into
    /// the blob store. Runs at every start; only does work once.
    pub async fn migrate_legacy(&self, metadata_dir: &Path) -> anyhow::Result<()> {
        let prefix = path_str(metadata_dir);
        let like = format!("{}%", like_escape(&prefix));
        let mut moved = 0u64;
        // An override and the item it applies to name the same file.
        let mut done: HashMap<String, Option<String>> = HashMap::new();
        for (table, key, columns) in
            [("items", "id", &["poster", "backdrop", "thumb"][..]), ("collections", "id", &["poster", "backdrop"][..])]
        {
            for column in columns {
                let rows: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
                    "SELECT {key}, {column} FROM {table} WHERE {column} LIKE ? ESCAPE '^'"
                )))
                .bind(&like)
                .fetch_all(&self.db.read)
                .await?;
                for (id, old) in rows {
                    let new = self.import_once(&old, metadata_dir, &mut done, &mut moved).await;
                    sqlx::query(sqlx::AssertSqlSafe(format!(
                        "UPDATE {table} SET {column} = ? WHERE {key} = ? AND {column} = ?"
                    )))
                    .bind(new)
                    .bind(id)
                    .bind(&old)
                    .execute(&self.db.write)
                    .await?;
                }
            }
        }
        let overrides: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT item_id, field, value FROM item_overrides WHERE value LIKE ? ESCAPE '^'")
                .bind(&like)
                .fetch_all(&self.db.read)
                .await?;
        for (item, field, old) in overrides {
            let new = self.import_once(&old, metadata_dir, &mut done, &mut moved).await;
            sqlx::query("UPDATE item_overrides SET value = ? WHERE item_id = ? AND field = ?")
                .bind(new)
                .bind(item)
                .bind(field)
                .execute(&self.db.write)
                .await?;
        }
        // People's photos were files named after their TMDB path.
        let people_dir = metadata_dir.join("people");
        if people_dir.is_dir() {
            let people: Vec<(String,)> = sqlx::query_as("SELECT photo_url FROM people WHERE photo_url IS NOT NULL")
                .fetch_all(&self.db.read)
                .await?;
            for (url,) in people {
                let Some(file) = url.rsplit('/').next().map(|name| people_dir.join(name)) else { continue };
                if !file.is_file() {
                    continue;
                }
                if let Some(blob) = self.import(&file, metadata_dir).await {
                    moved += 1;
                    sqlx::query("INSERT OR IGNORE INTO remote_images (url, blob, fetched_at) VALUES (?, ?, ?)")
                        .bind(&url)
                        .bind(&blob)
                        .bind(now())
                        .execute(&self.db.write)
                        .await?;
                }
            }
        }
        for dir in ["items", "collections", "people"] {
            let dir = metadata_dir.join(dir);
            if dir.is_dir() {
                let _ = crate::util::blocking(move || {
                    remove_old_layout(&dir);
                    Ok(())
                })
                .await;
            }
        }
        if moved > 0 {
            tracing::info!("moved {moved} pictures into the blob store");
        }
        Ok(())
    }

    async fn import_once(
        &self,
        old: &str,
        metadata_dir: &Path,
        done: &mut HashMap<String, Option<String>>,
        moved: &mut u64,
    ) -> Option<String> {
        if let Some(new) = done.get(old) {
            return new.clone();
        }
        let new = self.import(Path::new(old), metadata_dir).await;
        *moved += u64::from(new.is_some());
        done.insert(old.to_owned(), new.clone());
        new
    }

    /// Move a file kept under `metadata_dir` into the blob store.
    async fn import(&self, path: &Path, metadata_dir: &Path) -> Option<String> {
        if !path.starts_with(metadata_dir) {
            return None;
        }
        let bytes = tokio::fs::read(path).await.ok()?;
        let ext = picture_ext(&bytes)?;
        let blob = self.put(&bytes, ext).await.ok()?;
        let _ = tokio::fs::remove_file(path).await;
        Some(blob)
    }
}

/// Escapes `LIKE` wildcards with `^` (backslashes are common in Windows
/// paths): use with `ESCAPE '^'`.
pub(crate) fn like_escape(value: &str) -> String {
    value.replace('^', "^^").replace('%', "^%").replace('_', "^_")
}

fn old_enough(path: &Path, grace: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_none_or(|age| age >= grace)
}

/// Remove entries of a two-level store (`ab/<name>`) that `keep` rejects,
/// older than `grace` and not handed out since `since`.
///
/// Whether an entry was handed out is checked under the lock that handing
/// out takes, right before it goes: a picture or folder is either handed out
/// first (and stays), or removed first (and made again by whoever asks for it
/// next). Shard folders stay even when empty, for `put` may be about to write
/// into one.
fn sweep(
    root: &Path,
    grace: Duration,
    keep: impl Fn(&str) -> bool,
    handed_out: &Mutex<HandedOut>,
    since: Option<Instant>,
) -> (u64, u64) {
    let (mut count, mut bytes) = (0, 0);
    let Ok(shards) = std::fs::read_dir(root) else { return (0, 0) };
    for shard in shards.flatten() {
        let Ok(entries) = std::fs::read_dir(shard.path()) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            // A picture being stored is a temporary file until it is complete.
            let grace = if name.ends_with(".tmp") { grace.max(GRACE) } else { grace };
            if keep(&name) || !old_enough(&path, grace) {
                continue;
            }
            let removed = if path.is_dir() {
                let size = tree_size(&path).1;
                // Moved out of the way under the lock; removing it takes longer.
                let trash = shard.path().join(format!(".{}.trash", uuid::Uuid::new_v4().simple()));
                let moved = {
                    let handed_out = handed_out.lock();
                    !handed_out.since(&name, since) && std::fs::rename(&path, &trash).is_ok()
                };
                if moved {
                    let _ = std::fs::remove_dir_all(&trash);
                }
                moved.then_some(size)
            } else {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let handed_out = handed_out.lock();
                (!handed_out.since(&name, since) && std::fs::remove_file(&path).is_ok()).then_some(size)
            };
            if let Some(size) = removed {
                count += 1;
                bytes += size;
            }
        }
    }
    (count, bytes)
}

/// Remove the files of a cache folder (and of the folders in it) made more
/// than `max_age` ago; returns how many, and their bytes.
fn prune(dir: &Path, max_age: Duration) -> (u64, u64) {
    let (mut count, mut bytes) = (0, 0);
    for entry in walkdir::WalkDir::new(dir).min_depth(1).max_depth(2).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let age = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok());
        if age.is_some_and(|age| age >= max_age) && std::fs::remove_file(entry.path()).is_ok() {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

/// Files and bytes under a folder.
fn tree_size(root: &Path) -> (u64, u64) {
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .fold((0, 0), |(n, b), e| (n + 1, b + e.metadata().map(|m| m.len()).unwrap_or(0)))
}

/// Remove the old per-item folders. Nothing refers to their files any more:
/// pictures were moved into the store (references to files that could not
/// be moved were cleared), and frame grabs are made again on demand.
fn remove_old_layout(dir: &Path) {
    if let Err(err) = std::fs::remove_dir_all(dir) {
        tracing::debug!("removing {}: {err}", dir.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opensubtitles_hash_of_small_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.bin");
        // 8 bytes: size 8 + the word, counted at both ends.
        std::fs::write(&file, 1u64.to_le_bytes()).unwrap();
        assert_eq!(fingerprint(&file).unwrap(), format!("{:016x}", 8u64 + 1 + 1));
    }

    #[test]
    fn opensubtitles_hash_reference_value() {
        // Reference: http://www.opensubtitles.org/addons/avi/breakdance.avi
        // isn't shipped; check the algorithm on known synthetic data instead.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("b.bin");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file, &data).unwrap();
        let mut expected = data.len() as u64;
        for chunk in [&data[..65536], &data[data.len() - 65536..]] {
            for word in chunk.chunks(8) {
                expected = expected.wrapping_add(u64::from_le_bytes(word.try_into().unwrap()));
            }
        }
        assert_eq!(fingerprint(&file).unwrap(), format!("{expected:016x}"));
    }

    #[test]
    fn only_plain_blob_names_become_paths() {
        let hash = "a".repeat(64);
        assert!(valid_blob_name(&format!("{hash}.jpg")));
        assert!(!valid_blob_name(&format!("{hash}.exe")));
        assert!(!valid_blob_name("../../etc/passwd.jpg"));
        assert!(!valid_blob_name(&format!("{}.jpg", "A".repeat(64))));
        assert!(allowed_image_url("https://image.tmdb.org/t/p/w780/x.jpg"));
        assert!(!allowed_image_url("http://image.tmdb.org/t/p/w780/x.jpg"));
        assert!(!allowed_image_url("https://image.tmdb.org.evil.com/x.jpg"));
        assert!(!allowed_image_url("https://localhost/x.jpg"));
        assert!(allowed_image_url("https://raw.githubusercontent.com/team/images/master/poster.jpg"));
        assert!(allowed_image_url("https://theposterdb.com/api/assets/1"));
        assert!(!allowed_image_url("https://raw.githubusercontent.com@169.254.169.254/x.jpg"));
        assert!(!allowed_image_url("https://raw.githubusercontent.com:8443/x.jpg"));
        assert!(!allowed_image_url("https://theposterdb.com.example/x.jpg"));
    }

    #[tokio::test]
    async fn same_picture_is_stored_once() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let store = Store::new(db, &dir.path().join("metadata"), &dir.path().join("media"), &dir.path().join("cache"));
        let a = store.put(b"\xff\xd8\xff\xe0 picture", "jpg").await.unwrap();
        let b = store.put(b"\xff\xd8\xff\xe0 picture", "jpg").await.unwrap();
        assert_eq!(a, b);
        assert!(is_blob(&a));
        assert!(store.exists(&a));
        let path = store.path(&a).unwrap();
        assert!(path.starts_with(dir.path().join("metadata").join("blobs")));
        let poster = dir.path().join("poster.jpg");
        std::fs::write(&poster, b"\xff\xd8\xff\xe0 picture").unwrap();
        assert_eq!(store.path(&poster.to_string_lossy()), Some(poster));
        assert_eq!(store.path(&dir.path().join("gone.jpg").to_string_lossy()), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_picture_swapped_for_a_link_to_another_file_is_no_picture() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let secret = dir.path().join("secret.key");
        std::fs::write(&secret, b"secret").unwrap();
        let poster = dir.path().join("poster.jpg");
        std::os::unix::fs::symlink(&secret, &poster).unwrap();
        assert_eq!(store.path(&poster.to_string_lossy()), None);
        assert!(!store.exists(&poster.to_string_lossy()));
    }

    async fn store_in(dir: &Path) -> Store {
        let db = Db::open(&dir.join("pley.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'Movies', 'movies', '[]', 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        Store::new(db, &dir.join("metadata"), &dir.join("media"), &dir.join("cache"))
    }

    #[tokio::test]
    async fn clean_up_spares_what_is_used_and_what_is_new() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let used = store.put(b"\xff\xd8\xff\xe0 used", "jpg").await.unwrap();
        let unused = store.put(b"\xff\xd8\xff\xe0 unused", "jpg").await.unwrap();
        let photo = store.put(b"\xff\xd8\xff\xe0 photo", "jpg").await.unwrap();
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, poster, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'M', 'm', ?, 0, 0)",
        )
        .bind(&used)
        .execute(&store.db.write)
        .await
        .unwrap();
        // A download nothing uses any more is forgotten with its picture.
        sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES ('https://image.tmdb.org/x.jpg', ?, 0)")
            .bind(&photo)
            .execute(&store.db.write)
            .await
            .unwrap();
        // Media folders: one of a known file, one of a file that is gone.
        sqlx::query(
            "INSERT INTO media_files (item_id, library_id, path, size, mtime, fingerprint, added_at, updated_at)
             VALUES (1, 1, '/m/a.mkv', 1, 0, '0123456789abcdef', 0, 0)",
        )
        .execute(&store.db.write)
        .await
        .unwrap();
        for fp in ["0123456789abcdef", "fedcba9876543210"] {
            let folder = store.media_dir(fp).unwrap();
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("thumb.jpg"), b"jpeg").unwrap();
        }
        let before = store.usage().await.unwrap();
        assert_eq!((before.blobs, before.media_folders), (3, 2));

        // Everything is younger than the grace period: nothing goes.
        let report = store.clean().await.unwrap();
        assert_eq!((report.blobs_removed, report.media_folders_removed), (0, 0));

        let report = store.clean_older_than(Duration::ZERO).await.unwrap();
        assert_eq!((report.blobs_removed, report.media_folders_removed), (2, 1));
        assert!(report.bytes_freed > 0);
        assert!(store.exists(&used) && !store.exists(&unused) && !store.exists(&photo));
        assert!(store.media_dir("0123456789abcdef").unwrap().is_dir());
        assert!(!store.media_dir("fedcba9876543210").unwrap().exists());
        let remote: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_images").fetch_one(&store.db.read).await.unwrap();
        assert_eq!(remote, 0);
        let after = store.usage().await.unwrap();
        assert_eq!((after.blobs, after.media_folders), (1, 1));
    }

    /// Makes a file look `secs` seconds old.
    fn age(path: &Path, secs: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(secs)).unwrap();
    }

    /// The same store as after a restart: nothing handed out yet.
    fn restarted(store: &Store, dir: &Path) -> Store {
        Store::new(store.db.clone(), &dir.join("metadata"), &dir.join("media"), &dir.join("cache"))
    }

    #[tokio::test]
    async fn pictures_used_again_stay_until_their_reference_is_saved() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let (poster, photo, unused) =
            (b"\xff\xd8\xff\xe0 poster", b"\xff\xd8\xff\xe0 photo", b"\xff\xd8\xff\xe0 unused");
        let poster_ref = store.put(poster, "jpg").await.unwrap();
        let photo_ref = store.put(photo, "jpg").await.unwrap();
        let unused_ref = store.put(unused, "jpg").await.unwrap();
        let url = "https://image.tmdb.org/t/p/original/photo.jpg";
        sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES (?, ?, ?)")
            .bind(url)
            .bind(&photo_ref)
            .bind(now() - 7200)
            .execute(&store.db.write)
            .await
            .unwrap();
        // Stored hours ago, unused since, and the server restarted meanwhile.
        for blob in [&poster_ref, &photo_ref, &unused_ref] {
            age(&store.path(blob).unwrap(), 7200);
        }
        let store = restarted(&store, dir.path());

        // A refresh stores the poster again and finds the photo by its URL;
        // it saves the references after the clean-up took its snapshot.
        assert_eq!(store.put(poster, "jpg").await.unwrap(), poster_ref);
        assert_eq!(store.remote(url).await.unwrap(), photo_ref);
        let report = store.clean().await.unwrap();
        assert_eq!(report.blobs_removed, 1);
        assert!(store.exists(&poster_ref) && store.exists(&photo_ref) && !store.exists(&unused_ref));
        let urls: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_images").fetch_one(&store.db.read).await.unwrap();
        assert_eq!(urls, 1, "the photo's URL is remembered too");
    }

    #[tokio::test]
    async fn what_is_handed_out_during_a_clean_up_stays() {
        // The steps of clean_older_than(0), with a refresh and a file moved
        // back between its snapshot of references and its sweeps.
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let bytes = b"\xff\xd8\xff\xe0 poster";
        let blob = store.put(bytes, "jpg").await.unwrap();
        let (moved, gone) = ("0123456789abcdef", "fedcba9876543210");
        for fp in [moved, gone] {
            std::fs::create_dir_all(store.media_dir(fp).unwrap().join("trickplay")).unwrap();
        }

        let since = Instant::now().checked_sub(Duration::ZERO);
        let referenced = store.referenced_blobs().await.unwrap();
        assert!(referenced.is_empty());
        assert_eq!(store.put(bytes, "jpg").await.unwrap(), blob);
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, poster, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'M', 'm', ?, 0, 0)",
        )
        .bind(&blob)
        .execute(&store.db.write)
        .await
        .unwrap();
        let folder = store.media_dir(moved).unwrap();

        let (blobs, _) = sweep(&store.blobs, Duration::ZERO, |_| false, &store.handed_out, since);
        let (folders, _) = sweep(&store.media, Duration::ZERO, |_| false, &store.handed_out, since);
        assert_eq!((blobs, folders), (0, 1));
        assert!(store.exists(&blob), "the poster refers to it");
        assert!(folder.join("trickplay").is_dir());
        assert!(!store.media_dir(gone).unwrap().exists());
    }

    #[tokio::test]
    async fn downloads_are_forgotten_after_the_grace_period_only() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let fresh = store.put(b"\xff\xd8\xff\xe0 fresh", "jpg").await.unwrap();
        let old = store.put(b"\xff\xd8\xff\xe0 old", "jpg").await.unwrap();
        age(&store.path(&old).unwrap(), 7200);
        for (url, blob, fetched_at) in [
            ("https://image.tmdb.org/fresh.jpg", &fresh, now() - 60),
            ("https://image.tmdb.org/old.jpg", &old, now() - 7200),
        ] {
            sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES (?, ?, ?)")
                .bind(url)
                .bind(blob)
                .bind(fetched_at)
                .execute(&store.db.write)
                .await
                .unwrap();
        }
        let store = restarted(&store, dir.path());
        store.clean().await.unwrap();
        // Downloaded a minute ago and not used yet: remembered, with its picture.
        let urls: Vec<String> =
            sqlx::query_scalar("SELECT url FROM remote_images").fetch_all(&store.db.read).await.unwrap();
        assert_eq!(urls, ["https://image.tmdb.org/fresh.jpg"]);
        assert!(store.exists(&fresh) && !store.exists(&old));
    }

    #[tokio::test]
    async fn old_cache_files_go() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let cache = dir.path().join("cache");
        let files = [
            ("images/ab/old.jpg", 31),
            ("images/ab/recent.jpg", 29),
            ("covers/12.none", 31),
            ("subtitles/7_4_100.vtt", 91),
            ("subtitles/8_4_100.vtt", 60),
            ("fingerprints/7_100_5_0_1000.fp", 91),
            ("bif/0123456789abcdef-10000-320x180-720.bif", 31),
            ("bif/fedcba9876543210-10000-320x180-720.bif", 29),
            ("discover/w342/abc.jpg", 31),
            ("discover/w342/def.jpg", 2),
            ("trickplay/7/index.bif", 400), // not one of the caches the clean-up prunes
        ];
        for (name, days) in files {
            let path = cache.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"cached").unwrap();
            age(&path, days * DAY);
        }
        let report = store.clean().await.unwrap();
        assert_eq!((report.cache_files_removed, report.cache_bytes_freed), (6, 36));
        let mut left: Vec<String> = walkdir::WalkDir::new(&cache)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().strip_prefix(&cache).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "bif/fedcba9876543210-10000-320x180-720.bif",
                "discover/w342/def.jpg",
                "images/ab/recent.jpg",
                "subtitles/8_4_100.vtt",
                "trickplay/7/index.bif"
            ]
        );
    }

    #[tokio::test]
    async fn downloads_are_reused_and_only_from_image_hosts() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let blob = store.put(b"\x89PNG\r\n\x1a\n known", "png").await.unwrap();
        let url = "https://image.tmdb.org/t/p/original/known.png";
        sqlx::query("INSERT INTO remote_images (url, blob, fetched_at) VALUES (?, ?, 0)")
            .bind(url)
            .bind(&blob)
            .execute(&store.db.write)
            .await
            .unwrap();
        assert_eq!(store.remote(url).await.unwrap(), blob, "no download for a known URL");
        for url in [
            "http://image.tmdb.org/x.jpg",
            "https://127.0.0.1/x.jpg",
            "file:///etc/passwd",
            "https://evil.example/x.jpg",
        ] {
            let err = store.remote(url).await.unwrap_err();
            assert!(err.to_string().contains("not downloaded"), "{url}: {err}");
        }
    }

    #[test]
    fn only_fingerprints_name_media_folders() {
        let store_dir = Path::new("/data/media");
        let path = |fp: &str| valid_fingerprint(fp).then(|| store_dir.join(&fp[..2]).join(fp));
        assert!(path("0123456789abcdef").is_some());
        assert!(path("../../etc/passwd").is_none());
        assert!(path("0123456789abcde").is_none());
        assert!(path("0123456789abcdeg").is_none());
        assert_eq!(like_escape("C:\\100%_done^"), "C:\\100^%^_done^^");
        assert_eq!(picture_ext(b"\xff\xd8\xff\xe0 jpeg"), Some("jpg"));
        assert_eq!(picture_ext(b"GIF89a......"), Some("gif"));
        assert_eq!(picture_ext(b"%PDF-1.7"), None);
    }

    #[tokio::test]
    async fn unreadable_files_get_an_empty_fingerprint_and_are_retried_when_they_change() {
        let dir = crate::testing::tempdir();
        let store = store_in(dir.path()).await;
        let readable = dir.path().join("a.mkv");
        std::fs::write(&readable, vec![1u8; 100]).unwrap();
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'M', 'm', 0, 0)",
        )
        .execute(&store.db.write)
        .await
        .unwrap();
        for (id, path) in [(1, path_str(&readable)), (2, path_str(&dir.path().join("offline.mkv")))] {
            sqlx::query(
                "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, added_at, updated_at)
                 VALUES (?, 1, 1, ?, 100, 0, 0, 0)",
            )
            .bind(id)
            .bind(path)
            .execute(&store.db.write)
            .await
            .unwrap();
        }
        store.fingerprint_pending().await;
        let fps: Vec<Option<String>> = sqlx::query_scalar("SELECT fingerprint FROM media_files ORDER BY id")
            .fetch_all(&store.db.read)
            .await
            .unwrap();
        assert_eq!(fps[0].as_deref(), Some(fingerprint(&readable).unwrap().as_str()));
        assert_eq!(fps[1].as_deref(), Some(""), "marked, so the loop ends");
        assert!(store.media_dir("").is_none(), "the marker names no folder");
        let fp = store.fingerprint_of(2, &path_str(&readable), Some("")).await.unwrap();
        assert_eq!(fp.len(), 16, "an invalid stored value is computed again");
    }

    /// A store with `files` media files waiting for their fingerprint
    /// (ids 1…, real files of 100 bytes).
    async fn pending_files(dir: &Path, files: i64) -> Store {
        let store = store_in(dir).await;
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'M', 'm', 0, 0)",
        )
        .execute(&store.db.write)
        .await
        .unwrap();
        for id in 1..=files {
            let path = dir.join(format!("{id}.mkv"));
            std::fs::write(&path, vec![id as u8; 100]).unwrap();
            sqlx::query(
                "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, added_at, updated_at)
                 VALUES (?, 1, 1, ?, 100, 0, 0, 0)",
            )
            .bind(id)
            .bind(path_str(&path))
            .execute(&store.db.write)
            .await
            .unwrap();
        }
        store
    }

    async fn count(store: &Store, sql: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).fetch_one(&store.db.read).await.unwrap()
    }

    #[tokio::test]
    async fn fingerprinting_gives_up_when_saving_fails() {
        let dir = crate::testing::tempdir();
        let store = pending_files(dir.path(), 300).await;
        // Saving fails, as on a full disk; the trigger counts the attempts.
        for sql in [
            "CREATE TABLE attempts (id INTEGER)",
            "CREATE TRIGGER disk_full BEFORE UPDATE OF fingerprint ON media_files
             BEGIN INSERT INTO attempts VALUES (NEW.id); SELECT RAISE(FAIL, 'database or disk is full'); END",
        ] {
            sqlx::query(sql).execute(&store.db.write).await.unwrap();
        }
        // It used to read the same 200 files again and again.
        tokio::time::timeout(Duration::from_secs(20), store.fingerprint_pending()).await.expect("the run ends");
        assert_eq!(count(&store, "SELECT COUNT(*) FROM attempts").await, i64::from(MAX_FINGERPRINT_FAILURES));
        assert_eq!(count(&store, "SELECT COUNT(*) FROM media_files WHERE fingerprint IS NULL").await, 300);

        // Space again: the next run saves them all.
        sqlx::query("DROP TRIGGER disk_full").execute(&store.db.write).await.unwrap();
        store.fingerprint_pending().await;
        assert_eq!(count(&store, "SELECT COUNT(*) FROM media_files WHERE fingerprint IS NULL").await, 0);
    }

    #[tokio::test]
    async fn fingerprinting_moves_past_files_it_cannot_save() {
        let dir = crate::testing::tempdir();
        let store = pending_files(dir.path(), 4).await;
        // One update fails, one changes nothing: neither stops the others.
        for sql in [
            "CREATE TRIGGER fails BEFORE UPDATE OF fingerprint ON media_files WHEN NEW.id = 2
             BEGIN SELECT RAISE(FAIL, 'database disk image is malformed'); END",
            "CREATE TRIGGER ignored BEFORE UPDATE OF fingerprint ON media_files WHEN NEW.id = 3
             BEGIN SELECT RAISE(IGNORE); END",
        ] {
            sqlx::query(sql).execute(&store.db.write).await.unwrap();
        }
        tokio::time::timeout(Duration::from_secs(20), store.fingerprint_pending()).await.expect("the run ends");
        let saved: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM media_files WHERE fingerprint IS NOT NULL ORDER BY id")
                .fetch_all(&store.db.read)
                .await
                .unwrap();
        assert_eq!(saved, [1, 4]);
    }
}
