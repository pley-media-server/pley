//! Metadata providers besides TMDB (which lives in [`crate::metadata`]):
//!
//! - [`tvmaze`]: TV shows, no key needed (data licensed CC BY-SA).
//! - [`tvdb`]: TheTVDB v4, TV shows, needs an API key (and for some keys a
//!   subscriber PIN).
//! - [`musicbrainz`]: artists and albums, no key needed, with album covers
//!   from the Cover Art Archive.
//! - [`opensubtitles`]: subtitles to add to a file, with an API key each
//!   server's administrator gets (see [`crate::subtitles`]).
//!
//! [`crate::metadata::MetadataAgent`] decides which source refreshes an item
//! (see [`tv_source`]) and hands TV shows and music to [`Providers`]. The TV
//! providers only fetch and translate; [`tv::write`] stores a show the same
//! way for both.

pub mod musicbrainz;
pub mod opensubtitles;
pub mod tv;
pub mod tvdb;
pub mod tvmaze;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer};
use tokio::time::Instant;

use crate::db::Db;
use crate::library::LibraryRow;
use crate::settings::{Settings, SettingsStore, TvMetadata};
use crate::store::{self, Store};
use crate::util::normalize_key;

/// Requests that are rate limited are tried this many times.
const ATTEMPTS: u32 = 4;
/// MusicBrainz asks applications to identify themselves with a contact.
const USER_AGENT: &str = concat!("pley/", env!("CARGO_PKG_VERSION"), " ( https://github.com/pley-media-server/pley )");

/// Where the providers are reached. The defaults are the public services;
/// tests point them at a local server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub tvmaze: String,
    pub tvdb: String,
    pub musicbrainz: String,
    pub cover_art: String,
    /// TMDB's API (used once a TMDB key is set).
    pub tmdb: String,
    /// OpenSubtitles.com's API (used once its key is set).
    pub opensubtitles: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            tvmaze: tvmaze::API.into(),
            tvdb: tvdb::API.into(),
            musicbrainz: musicbrainz::API.into(),
            cover_art: musicbrainz::COVER_ART.into(),
            tmdb: crate::metadata::API.into(),
            opensubtitles: opensubtitles::API.into(),
        }
    }
}

/// The source of TV show metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvSource {
    Tmdb,
    Tvdb,
    Tvmaze,
}

impl TvSource {
    pub fn name(self) -> &'static str {
        match self {
            TvSource::Tmdb => "TMDB",
            TvSource::Tvdb => "TheTVDB",
            TvSource::Tvmaze => "TVmaze",
        }
    }
}

/// The TV source the settings ask for: "Automatic" is TMDB when it has a key,
/// else TheTVDB when it has one, else TVmaze. `None` when the chosen source
/// lacks the key it needs.
pub fn tv_source(settings: &Settings) -> Option<TvSource> {
    let tmdb = !settings.tmdb_api_key.trim().is_empty();
    let tvdb = !settings.tvdb_api_key.trim().is_empty();
    match settings.tv_metadata {
        TvMetadata::Auto if tmdb => Some(TvSource::Tmdb),
        TvMetadata::Auto if tvdb => Some(TvSource::Tvdb),
        TvMetadata::Auto | TvMetadata::Tvmaze => Some(TvSource::Tvmaze),
        TvMetadata::Tmdb => tmdb.then_some(TvSource::Tmdb),
        TvMetadata::Tvdb => tvdb.then_some(TvSource::Tvdb),
    }
}

/// After a settings change: shows are fetched again from a new TV source.
/// Returns whether libraries should be scanned to fetch what is missing.
pub async fn settings_changed(db: &Db, before: &Settings, after: &Settings) -> sqlx::Result<bool> {
    let tv = tv_source(after);
    let tv_changed = tv_source(before) != tv;
    if tv_changed && tv.is_some() {
        sqlx::query("UPDATE items SET metadata_at = NULL WHERE kind = 'show' AND metadata_locked = 0")
            .execute(&db.write)
            .await?;
    }
    let tmdb_key = before.tmdb_api_key != after.tmdb_api_key && !after.tmdb_api_key.is_empty();
    let tvdb_key =
        (before.tvdb_api_key != after.tvdb_api_key || before.tvdb_pin != after.tvdb_pin) && tv == Some(TvSource::Tvdb);
    let music = after.music_metadata && !before.music_metadata;
    Ok(tmdb_key || tvdb_key || music || (tv_changed && tv.is_some()))
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("{0} rejected the API key")]
    Unauthorized(&'static str),
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<sqlx::Error> for ProviderError {
    fn from(err: sqlx::Error) -> Self {
        ProviderError::Other(err.into())
    }
}

/// A connection test from the settings.
#[derive(Debug, Clone, Copy)]
pub enum Check<'a> {
    /// Signs in with this key and PIN (not necessarily the saved ones).
    Tvdb {
        key: &'a str,
        pin: &'a str,
    },
    Tvmaze,
    MusicBrainz,
}

/// TV shows and music from the providers that aren't TMDB.
pub struct Providers {
    db: Db,
    settings: Arc<SettingsStore>,
    store: Arc<Store>,
    tvmaze: Option<tvmaze::Tvmaze>,
    tvdb: Option<tvdb::Tvdb>,
    musicbrainz: Option<musicbrainz::MusicBrainz>,
}

impl Providers {
    /// `endpoints: None` never contacts TVmaze, TheTVDB or MusicBrainz.
    pub fn new(db: Db, settings: Arc<SettingsStore>, store: Arc<Store>, endpoints: Option<Endpoints>) -> Self {
        let client = |redirects: bool| {
            let policy =
                if redirects { reqwest::redirect::Policy::default() } else { reqwest::redirect::Policy::none() };
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(20))
                .redirect(policy)
                .build()
                .expect("http client")
        };
        let (tvmaze, tvdb, musicbrainz) = match endpoints {
            Some(e) => (
                // TVmaze answers lookups with a redirect; its target is read
                // instead of fetched.
                Some(tvmaze::Tvmaze::new(client(false), &e.tvmaze)),
                Some(tvdb::Tvdb::new(client(true), &e.tvdb)),
                Some(musicbrainz::MusicBrainz::new(
                    client(true),
                    &e.musicbrainz,
                    &e.cover_art,
                    db.clone(),
                    store.clone(),
                )),
            ),
            None => (None, None, None),
        };
        Self { db, settings, store, tvmaze, tvdb, musicbrainz }
    }

    /// The TV source in use, `None` when it can't be used.
    pub fn tv_source(&self) -> Option<TvSource> {
        match tv_source(&self.settings.get())? {
            TvSource::Tmdb => Some(TvSource::Tmdb),
            TvSource::Tvdb => self.tvdb.as_ref().map(|_| TvSource::Tvdb),
            TvSource::Tvmaze => self.tvmaze.as_ref().map(|_| TvSource::Tvmaze),
        }
    }

    /// Whether a service answers: one request (TheTVDB: a sign-in).
    pub async fn check(&self, check: Check<'_>) -> Result<(), ProviderError> {
        let off = || ProviderError::Other(anyhow!("this server doesn't contact metadata services"));
        match check {
            Check::Tvdb { key, pin } => self.tvdb.as_ref().ok_or_else(off)?.check(key, pin).await,
            Check::Tvmaze => self.tvmaze.as_ref().ok_or_else(off)?.check().await,
            Check::MusicBrainz => self.musicbrainz.as_ref().ok_or_else(off)?.check().await,
        }
    }

    /// Whether artists and albums are looked up on MusicBrainz.
    pub fn music_enabled(&self) -> bool {
        self.musicbrainz.is_some() && self.settings.get().music_metadata
    }

    /// Fetch a show (with its seasons, episodes and cast) from TVmaze or
    /// TheTVDB and store it.
    pub async fn refresh_show(&self, source: TvSource, show_id: i64) -> Result<(), ProviderError> {
        let show = tv::load(&self.db, show_id).await?;
        let meta = match (source, &self.tvmaze, &self.tvdb) {
            (TvSource::Tvmaze, Some(tvmaze), _) => {
                let specials = tv::has_specials(&self.db, show_id).await?;
                tvmaze.show(&show, specials).await?
            }
            (TvSource::Tvdb, _, Some(tvdb)) => tvdb.show(&show, &self.settings.get()).await?,
            _ => return Err(anyhow!("{} is not available", source.name()).into()),
        };
        tv::write(&self.db, &self.store, &show, &meta).await
    }

    /// "Fix match" chose a TMDB entry for a show refreshed from another
    /// source: find the show there by the ids TMDB knows for it.
    pub async fn rematched(&self, show_id: i64, imdb_id: Option<String>, tvdb_id: Option<i64>) -> sqlx::Result<()> {
        sqlx::query("UPDATE items SET imdb_id = ?, tvdb_id = ?, tvmaze_id = NULL WHERE id = ? AND kind = 'show'")
            .bind(imdb_id.filter(|i| !i.is_empty()))
            .bind(tvdb_id)
            .bind(show_id)
            .execute(&self.db.write)
            .await?;
        Ok(())
    }

    /// Look an artist (and its albums) or an album up on MusicBrainz.
    pub async fn refresh_music_item(&self, item_id: i64) -> Result<(), ProviderError> {
        match &self.musicbrainz {
            Some(mb) if self.settings.get().music_metadata => mb.refresh_item(item_id).await,
            _ => Ok(()),
        }
    }

    /// Fetch MusicBrainz metadata for the artists and albums of a music
    /// library that have none yet (or all, when `force`).
    pub async fn refresh_music_library(
        &self,
        lib: &LibraryRow,
        force: bool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> anyhow::Result<()> {
        match &self.musicbrainz {
            Some(mb) if self.settings.get().music_metadata => mb.refresh_library(lib, force, progress).await,
            _ => Ok(()),
        }
    }
}

// ------------------------------------------------------------------ HTTP

/// At most `max` requests in any `window`.
pub(crate) struct Throttle {
    max: usize,
    window: Duration,
    sent: parking_lot::Mutex<VecDeque<Instant>>,
}

impl Throttle {
    pub(crate) fn new(max: usize, window: Duration) -> Self {
        Self { max: max.max(1), window, sent: parking_lot::Mutex::new(VecDeque::new()) }
    }

    /// Wait until a request may be sent.
    pub(crate) async fn wait(&self) {
        loop {
            let delay = {
                let mut sent = self.sent.lock();
                let now = Instant::now();
                while sent.front().is_some_and(|t| now.duration_since(*t) >= self.window) {
                    sent.pop_front();
                }
                if sent.len() < self.max {
                    sent.push_back(now);
                    return;
                }
                self.window.saturating_sub(now.duration_since(sent[0]))
            };
            tokio::time::sleep(delay).await;
        }
    }
}

/// A provider's API: throttled requests, rate limits waited out, answers
/// checked.
pub(crate) struct Api {
    pub(crate) http: reqwest::Client,
    name: &'static str,
    throttle: Throttle,
}

impl Api {
    pub(crate) fn new(http: reqwest::Client, name: &'static str, throttle: Throttle) -> Self {
        Self { http, name, throttle }
    }

    /// Send a request. Successful and redirect answers come back; `404` is
    /// [`ProviderError::NotFound`], `401` [`ProviderError::Unauthorized`].
    pub(crate) async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, ProviderError> {
        let mut backoff = Duration::from_secs(1);
        for _ in 0..ATTEMPTS {
            self.throttle.wait().await;
            let request = request.try_clone().ok_or_else(|| anyhow!("request can't be repeated"))?;
            let resp = request.send().await.map_err(|e| anyhow!("{}: {e}", self.name))?;
            match resp.status().as_u16() {
                200..=399 => return Ok(resp),
                401 => return Err(ProviderError::Unauthorized(self.name)),
                404 => return Err(ProviderError::NotFound),
                // Too many requests (MusicBrainz answers 503).
                429 | 503 => {
                    let wait = retry_after(&resp).unwrap_or(backoff).min(Duration::from_secs(30));
                    tracing::debug!("{} asks to slow down; waiting {wait:?}", self.name);
                    tokio::time::sleep(wait).await;
                    backoff *= 2;
                }
                status => return Err(anyhow!("{} answered HTTP {status}", self.name).into()),
            }
        }
        Err(anyhow!("{} keeps asking to slow down", self.name).into())
    }

    /// Send a request and read the JSON answer.
    pub(crate) async fn json<T: DeserializeOwned>(&self, request: reqwest::RequestBuilder) -> Result<T, ProviderError> {
        let bytes = self.send(request).await?.bytes().await.map_err(|e| anyhow!("{}: {e}", self.name))?;
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("unexpected answer from {}: {e}", self.name).into())
    }
}

pub(crate) fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let value = resp.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

// ------------------------------------------------------------------ parsing

/// `null` read as the default value (providers send `null` for empty lists).
pub(crate) fn nullable<'de, D, T>(de: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(de)?.unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Loose {
    Int(i64),
    Float(f64),
    Text(String),
    #[allow(dead_code)]
    Other(IgnoredAny),
}

/// A number sent as a number or as text (`81189`, `"81189"`); anything else
/// is no number.
pub(crate) fn loose_id<'de, D: Deserializer<'de>>(de: D) -> Result<Option<i64>, D::Error> {
    Ok(match Option::<Loose>::deserialize(de)? {
        Some(Loose::Int(n)) => Some(n),
        Some(Loose::Float(f)) if f.fract() == 0.0 => Some(f as i64),
        Some(Loose::Text(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

/// Text sent as text or as a number (`"2008"`, `2008`).
pub(crate) fn loose_text<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<Loose>::deserialize(de)? {
        Some(Loose::Text(s)) => Some(s),
        Some(Loose::Int(n)) => Some(n.to_string()),
        Some(Loose::Float(f)) => Some(f.to_string()),
        _ => None,
    })
}

/// `Some` trimmed text, `None` when there is none.
pub(crate) fn text(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned)
}

/// The year of a date ("2008-01-20", "2008").
pub(crate) fn year_of(date: Option<&str>) -> Option<i64> {
    let year: i64 = date?.trim().get(..4)?.parse().ok()?;
    (1000..=9999).contains(&year).then_some(year)
}

/// A full date ("2008-01-20"); partial dates ("2008", "2008-01") are none.
pub(crate) fn full_date(date: Option<&str>) -> Option<String> {
    let date = date?.trim();
    let b = date.as_bytes();
    let ok = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    ok.then(|| date.to_owned())
}

/// Plain text of a provider's HTML snippet (`<p><b>Breaking Bad</b> follows
/// …</p>`): tags dropped, paragraphs and line breaks kept as new lines,
/// character references decoded.
pub(crate) fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('>') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + len].trim().trim_start_matches('/').to_ascii_lowercase();
        let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or_default();
        match name {
            "br" => out.push('\n'),
            "p" | "div" | "li" | "ul" | "ol" | "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                out.push_str("\n\n")
            }
            _ => {}
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    let decoded = decode_entities(&out);
    // Spaces collapsed within lines, at most one empty line between paragraphs.
    let mut text = String::with_capacity(decoded.len());
    let mut blank = 0;
    for line in decoded.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            continue;
        }
        if !text.is_empty() {
            text.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        text.push_str(&line);
        blank = 0;
    }
    text
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let entity = rest[1..].find(';').filter(|&end| end <= 10).map(|end| &rest[1..end + 1]);
        let decoded = entity.and_then(|e| match e {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "ndash" => Some('\u{2013}'),
            "mdash" => Some('\u{2014}'),
            "hellip" => Some('\u{2026}'),
            "lsquo" => Some('\u{2018}'),
            "rsquo" => Some('\u{2019}'),
            "ldquo" => Some('\u{201C}'),
            "rdquo" => Some('\u{201D}'),
            _ => {
                let number = e.strip_prefix('#')?;
                let code = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => number.parse().ok()?,
                };
                char::from_u32(code)
            }
        });
        match (entity, decoded) {
            (Some(e), Some(c)) => {
                out.push(c);
                rest = &rest[e.len() + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// How well titles a provider has for an entry fit the one the scanner found:
/// 3 for the same title, 1.5 when one contains the other, else 0.
pub(crate) fn title_score<'a>(wanted: &str, titles: impl IntoIterator<Item = &'a str>) -> f64 {
    let wanted = normalize_key(wanted);
    if wanted.is_empty() {
        return 0.0;
    }
    titles
        .into_iter()
        .map(|t| {
            let t = normalize_key(t);
            if t.is_empty() {
                0.0
            } else if t == wanted {
                3.0
            } else if t.contains(&wanted) || wanted.contains(&t) {
                1.5
            } else {
                0.0
            }
        })
        .fold(0.0, f64::max)
}

/// The same year scores 2, a year off by one 1, another year -1.
pub(crate) fn year_score(wanted: Option<i64>, found: Option<i64>) -> f64 {
    match (wanted, found) {
        (Some(want), Some(got)) if want == got => 2.0,
        (Some(want), Some(got)) if (want - got).abs() == 1 => 1.0,
        (Some(_), Some(_)) => -1.0,
        _ => 0.0,
    }
}

/// A provider's picture as a stored reference, unless the item has local
/// (sidecar) artwork, which always wins. Pictures already stored from the
/// same URL are not downloaded again.
pub(crate) async fn artwork(store: &Store, current: Option<&str>, url: Option<&str>) -> Option<String> {
    let url = url?;
    if current.is_some_and(|cur| !store::is_blob(cur) && Path::new(cur).exists()) {
        return None;
    }
    match store.remote(url).await {
        Ok(blob) => Some(blob),
        Err(err) => {
            tracing::debug!("artwork download failed for {url}: {err:#}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(tv: TvMetadata, tmdb: &str, tvdb: &str) -> Settings {
        Settings { tv_metadata: tv, tmdb_api_key: tmdb.into(), tvdb_api_key: tvdb.into(), ..Settings::default() }
    }

    #[test]
    fn automatic_tv_source_prefers_keyed_sources() {
        assert_eq!(tv_source(&settings(TvMetadata::Auto, "", "")), Some(TvSource::Tvmaze));
        assert_eq!(tv_source(&settings(TvMetadata::Auto, "", "key")), Some(TvSource::Tvdb));
        assert_eq!(tv_source(&settings(TvMetadata::Auto, "key", "key")), Some(TvSource::Tmdb));
        assert_eq!(tv_source(&settings(TvMetadata::Tvmaze, "key", "key")), Some(TvSource::Tvmaze));
        assert_eq!(tv_source(&settings(TvMetadata::Tvdb, "key", "")), None);
        assert_eq!(tv_source(&settings(TvMetadata::Tmdb, "", "key")), None);
    }

    #[test]
    fn html_summaries_become_text() {
        assert_eq!(
            strip_html("<p><b>Breaking Bad</b> follows Walter &amp; Jesse&#39;s &quot;business&quot;.</p>"),
            "Breaking Bad follows Walter & Jesse's \"business\"."
        );
        assert_eq!(
            strip_html("<p>One.</p><p>Two<br>lines &#x2014; &hellip;</p>"),
            "One.\n\nTwo\nlines \u{2014} \u{2026}"
        );
        assert_eq!(strip_html("  Plain   text &unknown; & more "), "Plain text &unknown; & more");
        assert_eq!(strip_html("<p></p>"), "");
        assert_eq!(strip_html("a < b"), "a < b");
    }

    #[test]
    fn dates_and_years() {
        assert_eq!(year_of(Some("2008-01-20")), Some(2008));
        assert_eq!(year_of(Some("19")), None);
        assert_eq!(year_of(None), None);
        assert_eq!(full_date(Some("2001-02-26")), Some("2001-02-26".into()));
        assert_eq!(full_date(Some("2001-02")), None);
        assert_eq!(full_date(Some("2001-xx-26")), None);
    }

    #[test]
    fn titles_and_years_score() {
        assert_eq!(title_score("The Office (US)", ["The Office US"]), 3.0);
        assert_eq!(title_score("Office", ["The Office", "Bureau"]), 1.5);
        assert_eq!(title_score("Dark", ["Lost"]), 0.0);
        assert_eq!(title_score("", ["Anything"]), 0.0);
        assert_eq!(year_score(Some(2005), Some(2005)), 2.0);
        assert_eq!(year_score(Some(2005), Some(2006)), 1.0);
        assert_eq!(year_score(Some(2005), Some(2001)), -1.0);
        assert_eq!(year_score(None, Some(2001)), 0.0);
    }

    #[test]
    fn loose_numbers_and_text() {
        #[derive(Deserialize)]
        struct Probe {
            #[serde(default, deserialize_with = "loose_id")]
            id: Option<i64>,
            #[serde(default, deserialize_with = "loose_text")]
            year: Option<String>,
            #[serde(default, deserialize_with = "nullable")]
            list: Vec<i64>,
        }
        let p: Probe = serde_json::from_str(r#"{"id": "81189", "year": 2008, "list": null}"#).unwrap();
        assert_eq!((p.id, p.year.as_deref(), p.list.len()), (Some(81189), Some("2008"), 0));
        let p: Probe = serde_json::from_str(r#"{"id": {"odd": true}, "year": "2008"}"#).unwrap();
        assert_eq!((p.id, p.year.as_deref()), (None, Some("2008")));
    }

    #[tokio::test]
    async fn throttle_spaces_requests() {
        let throttle = Throttle::new(2, Duration::from_millis(150));
        let started = Instant::now();
        throttle.wait().await;
        throttle.wait().await;
        assert!(started.elapsed() < Duration::from_millis(100));
        throttle.wait().await;
        assert!(started.elapsed() >= Duration::from_millis(140));
    }
}
