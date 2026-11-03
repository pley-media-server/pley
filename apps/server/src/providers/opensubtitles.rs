//! OpenSubtitles.com's REST API (v1, <https://opensubtitles.stoplight.io>):
//! subtitles found by a file's OpenSubtitles hash ([`crate::store::fingerprint`])
//! and its title's IMDb or TMDB id, or by name, and downloaded into pley as
//! added subtitles ([`crate::subtitles`]).
//!
//! Every request carries the server's API key, which each administrator gets
//! from opensubtitles.com for their server (pley can't ship one), and a
//! `User-Agent` naming pley and its version, as OpenSubtitles asks.
//! Downloads count against a daily quota: the API key's alone, or with an
//! opensubtitles.com account set in the settings, that account's (`POST
//! /login` trades it for a token valid a day, kept in memory). Requests are
//! kept under OpenSubtitles' limit of 5 a second.
//!
//! Written from OpenSubtitles' API documentation; answers are read leniently
//! (missing fields, `null`, numbers as text).

use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{Throttle, loose_id, nullable, retry_after};
use crate::settings::SettingsStore;

pub const API: &str = "https://api.opensubtitles.com/api/v1";
/// As OpenSubtitles asks: the application and its version.
const USER_AGENT: &str = concat!("pley v", env!("CARGO_PKG_VERSION"));
/// Tokens last a day; they are renewed a little before.
const TOKEN_LIFE: Duration = Duration::from_secs(23 * 3600);
/// Rate limits waited out at most this many times.
const ATTEMPTS: u32 = 3;
/// The prefix of the results' ids.
const PROVIDER: &str = "opensubtitles";

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// No API key in the settings.
    #[error(
        "subtitle search isn't set up on this server: an administrator adds an OpenSubtitles API key in Settings → Server"
    )]
    NotSetUp,
    /// This server never contacts OpenSubtitles (tests).
    #[error("this server doesn't contact OpenSubtitles")]
    Off,
    #[error("OpenSubtitles rejected {0}")]
    Rejected(&'static str),
    /// The day's downloads are used up: OpenSubtitles' words.
    #[error("{0}")]
    Quota(String),
    #[error("OpenSubtitles has no such subtitle")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A subtitle found, as `GET /api/items/{id}/subtitles/search` lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    /// Passed back to add it: `opensubtitles:<file id>:<language>`, with
    /// `:hi`, `:forced` and `:srt` (made SubRip by OpenSubtitles) when they
    /// apply.
    pub id: String,
    pub provider: &'static str,
    /// OpenSubtitles' language code (`en`, `pt-BR`).
    pub language: String,
    /// The release it was made for, or its file's name.
    pub name: String,
    pub format: String,
    pub hearing_impaired: bool,
    pub forced: bool,
    pub downloads: i64,
    /// `hash`: made for this very file (its OpenSubtitles hash); `title`:
    /// for the film or episode.
    pub matched: &'static str,
}

/// What a result's id says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub file_id: i64,
    pub language: String,
    pub hearing_impaired: bool,
    pub forced: bool,
    /// Its own format isn't one pley takes: OpenSubtitles makes it SubRip.
    pub convert: bool,
}

impl Picked {
    pub fn parse(id: &str) -> Option<Self> {
        let mut parts = id.split(':');
        if parts.next()? != PROVIDER {
            return None;
        }
        let file_id = parts.next()?.parse().ok().filter(|id: &i64| *id > 0)?;
        let language = parts.next().unwrap_or_default().to_owned();
        let (mut hearing_impaired, mut forced, mut convert) = (false, false, false);
        for flag in parts {
            match flag {
                "hi" => hearing_impaired = true,
                "forced" => forced = true,
                "srt" => convert = true,
                _ => return None,
            }
        }
        Some(Self { file_id, language, hearing_impaired, forced, convert })
    }

    fn id(&self) -> String {
        let mut id = format!("{PROVIDER}:{}:{}", self.file_id, self.language);
        if self.hearing_impaired {
            id.push_str(":hi");
        }
        if self.forced {
            id.push_str(":forced");
        }
        if self.convert {
            id.push_str(":srt");
        }
        id
    }
}

/// What is looked for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Search {
    /// The file's OpenSubtitles hash.
    pub moviehash: Option<String>,
    /// A film's (or an episode's own) ids.
    pub imdb_id: Option<i64>,
    pub tmdb_id: Option<i64>,
    /// An episode's show's ids, with its season and number.
    pub parent_imdb_id: Option<i64>,
    pub parent_tmdb_id: Option<i64>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    /// A title, when there are no ids.
    pub query: Option<String>,
    pub year: Option<i64>,
    /// OpenSubtitles' language codes, in lower case.
    pub languages: Vec<String>,
}

impl Search {
    /// The query string: in alphabetical order and lower case, as
    /// OpenSubtitles wants it (it redirects other forms to this one).
    pub fn params(&self) -> Vec<(&'static str, String)> {
        let mut languages = self.languages.clone();
        languages.sort();
        languages.dedup();
        let mut params: Vec<(&'static str, String)> = [
            ("episode_number", self.episode_number.map(|n| n.to_string())),
            ("imdb_id", self.imdb_id.map(|n| n.to_string())),
            ("languages", (!languages.is_empty()).then(|| languages.join(","))),
            ("moviehash", self.moviehash.clone()),
            ("parent_imdb_id", self.parent_imdb_id.map(|n| n.to_string())),
            ("parent_tmdb_id", self.parent_tmdb_id.map(|n| n.to_string())),
            ("query", self.query.as_deref().map(str::to_lowercase)),
            ("season_number", self.season_number.map(|n| n.to_string())),
            ("tmdb_id", self.tmdb_id.map(|n| n.to_string())),
            ("year", self.year.map(|n| n.to_string())),
        ]
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
        .collect();
        params.sort_by_key(|(name, _)| *name);
        params
    }
}

/// OpenSubtitles' language codes for a language as pley's tracks and
/// settings name it (`eng`, `en`, `pt-BR`): ISO 639-1, with both of its
/// codes for Portuguese and Chinese, which it splits by region.
pub fn search_languages(code: &str) -> Vec<String> {
    let code = code.trim().to_lowercase().replace('_', "-");
    if matches!(code.as_str(), "pt-br" | "pt-pt" | "zh-cn" | "zh-tw") {
        return vec![code];
    }
    let primary = code.split('-').next().unwrap_or_default();
    let short = crate::languages::language(primary).map_or(primary, |(_, short, _)| short);
    match short {
        "pt" => vec!["pt-br".into(), "pt-pt".into()],
        "zh" => vec!["zh-cn".into(), "zh-tw".into()],
        "" => Vec::new(),
        short => vec![short.to_owned()],
    }
}

// ------------------------------------------------------------------ answers

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchAnswer {
    #[serde(deserialize_with = "nullable")]
    data: Vec<Subtitle>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Subtitle {
    attributes: Attributes,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Attributes {
    language: Option<String>,
    #[serde(deserialize_with = "loose_id")]
    download_count: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    hearing_impaired: bool,
    #[serde(deserialize_with = "nullable")]
    foreign_parts_only: bool,
    #[serde(deserialize_with = "nullable")]
    moviehash_match: bool,
    release: Option<String>,
    #[serde(deserialize_with = "nullable")]
    files: Vec<File>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct File {
    #[serde(deserialize_with = "loose_id")]
    file_id: Option<i64>,
    file_name: Option<String>,
}

/// The subtitles of a search's answer: those of one file (not split over
/// several CDs), the ones made for this very file first, then the most
/// downloaded.
pub fn parse_search(body: &[u8]) -> Result<Vec<Found>, SearchError> {
    let answer: SearchAnswer =
        serde_json::from_slice(body).map_err(|e| anyhow!("unexpected answer from OpenSubtitles: {e}"))?;
    let mut found: Vec<Found> = answer
        .data
        .into_iter()
        .filter_map(|subtitle| {
            let a = subtitle.attributes;
            let [file] = <[File; 1]>::try_from(a.files).ok()?;
            let file_name = file.file_name.unwrap_or_default();
            let picked = Picked {
                file_id: file.file_id?,
                language: a.language.filter(|l| !l.is_empty() && !l.contains(':'))?,
                hearing_impaired: a.hearing_impaired,
                forced: a.foreign_parts_only,
                convert: format_of(&file_name).is_none(),
            };
            let name = a.release.filter(|r| !r.trim().is_empty()).unwrap_or_else(|| file_name.clone());
            Some(Found {
                id: picked.id(),
                provider: PROVIDER,
                language: picked.language,
                name: name.trim().to_owned(),
                format: format_of(&file_name).unwrap_or("srt").to_owned(),
                hearing_impaired: picked.hearing_impaired,
                forced: picked.forced,
                downloads: a.download_count.unwrap_or(0),
                matched: if a.moviehash_match { "hash" } else { "title" },
            })
        })
        .collect();
    found.sort_by(|a, b| (b.matched == "hash").cmp(&(a.matched == "hash")).then(b.downloads.cmp(&a.downloads)));
    found.dedup_by(|a, b| a.id == b.id);
    Ok(found)
}

/// The format a subtitle comes in, by its file's name, when pley takes it
/// as it is; else OpenSubtitles makes it SubRip.
fn format_of(file_name: &str) -> Option<&'static str> {
    let ext = file_name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    crate::subtitles::Format::from_extension(&ext).map(crate::subtitles::Format::extension)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DownloadAnswer {
    link: Option<String>,
    file_name: Option<String>,
    #[serde(deserialize_with = "loose_id")]
    remaining: Option<i64>,
    message: Option<String>,
}

/// Where a download is, and its file's name; the quota's message when the
/// day's downloads are used up.
fn parse_download(body: &[u8]) -> Result<(String, String), SearchError> {
    let answer: DownloadAnswer =
        serde_json::from_slice(body).map_err(|e| anyhow!("unexpected answer from OpenSubtitles: {e}"))?;
    match answer.link.filter(|l| l.starts_with("https://") || l.starts_with("http://")) {
        Some(link) => Ok((link, answer.file_name.unwrap_or_default())),
        None if answer.remaining.is_some_and(|r| r < 0) || answer.message.is_some() => Err(quota(answer.message)),
        None => Err(anyhow!("OpenSubtitles gave no download link").into()),
    }
}

fn quota(message: Option<String>) -> SearchError {
    let message = message.map(|m| m.trim().to_owned()).filter(|m| !m.is_empty());
    SearchError::Quota(match message {
        Some(m) => format!("OpenSubtitles: {m}"),
        None => "this server's OpenSubtitles downloads for today are used up".into(),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LoginAnswer {
    token: Option<String>,
    base_url: Option<String>,
}

// ------------------------------------------------------------------ client

#[derive(Clone, PartialEq, Eq)]
struct Credentials {
    key: String,
    username: String,
    password: String,
}

struct Token {
    credentials: Credentials,
    value: String,
    /// Where a signed-in account sends its downloads (VIP accounts have a
    /// host of their own).
    base: String,
    at: Instant,
}

pub struct OpenSubtitles {
    http: reqwest::Client,
    /// `None`: never contacted (tests).
    base: Option<String>,
    settings: Arc<SettingsStore>,
    throttle: Throttle,
    token: tokio::sync::Mutex<Option<Token>>,
}

impl OpenSubtitles {
    pub fn new(settings: Arc<SettingsStore>, base: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            // Never from https to plain http, wherever a download leads.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let downgrade =
                    attempt.url().scheme() != "https" && attempt.previous().iter().any(|url| url.scheme() == "https");
                if downgrade {
                    attempt.error("a redirect from https to plain http")
                } else if attempt.previous().len() > 5 {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .expect("http client");
        Self { http, base, settings, throttle: Throttle::new(4, Duration::from_secs(1)), token: Default::default() }
    }

    /// Whether an API key is set.
    pub fn is_set_up(&self) -> bool {
        !self.settings.get().opensubtitles_api_key.trim().is_empty()
    }

    fn credentials(&self) -> Result<(String, Credentials), SearchError> {
        let settings = self.settings.get();
        let key = settings.opensubtitles_api_key.trim().to_owned();
        if key.is_empty() {
            return Err(SearchError::NotSetUp);
        }
        let base = self.base.clone().ok_or(SearchError::Off)?;
        let credentials = Credentials {
            key,
            username: settings.opensubtitles_username.clone(),
            password: settings.opensubtitles_password.clone(),
        };
        Ok((base, credentials))
    }

    /// Subtitles for `search`, best first ([`parse_search`]).
    pub async fn search(&self, search: &Search) -> Result<Vec<Found>, SearchError> {
        let (base, credentials) = self.credentials()?;
        let request = self.http.get(format!("{base}/subtitles")).query(&search.params());
        let body = self.send(request, &credentials.key).await?;
        parse_search(&body)
    }

    /// Downloads subtitle file `file_id`: its name and its bytes, at most
    /// `max_bytes` of them. `convert` asks OpenSubtitles for SubRip.
    pub async fn download(
        &self,
        file_id: i64,
        convert: bool,
        max_bytes: usize,
    ) -> Result<(String, Vec<u8>), SearchError> {
        let (base, credentials) = self.credentials()?;
        let token = self.token(&base, &credentials).await?;
        let mut body = serde_json::json!({ "file_id": file_id });
        if convert {
            body["sub_format"] = "srt".into();
        }
        let base = token.as_ref().map_or(base, |(_, base)| base.clone());
        let mut request = self.http.post(format!("{base}/download")).json(&body);
        if let Some((token, _)) = &token {
            request = request.bearer_auth(token);
        }
        let answer = match self.send(request, &credentials.key).await {
            Err(SearchError::Rejected(_)) if token.is_some() => {
                // An expired token: signed in again once.
                self.forget().await;
                let token = self.token(&base, &credentials).await?;
                let mut request = self.http.post(format!("{base}/download")).json(&body);
                if let Some((token, _)) = &token {
                    request = request.bearer_auth(token);
                }
                self.send(request, &credentials.key).await?
            }
            answer => answer?,
        };
        let (link, name) = parse_download(&answer)?;
        if !link_allowed(&link, self.base.as_deref()) {
            return Err(anyhow!("OpenSubtitles gave a download link that isn't https").into());
        }
        let mut response = self
            .http
            .get(&link)
            .send()
            .await
            .map_err(|e| anyhow!("OpenSubtitles: {e}"))?
            .error_for_status()
            .map_err(|e| anyhow!("OpenSubtitles: {e}"))?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| anyhow!("OpenSubtitles: {e}"))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > max_bytes {
                return Err(anyhow!("the subtitle file is larger than Pley takes").into());
            }
        }
        Ok((name, bytes))
    }

    /// Whether OpenSubtitles takes this key (and signs this account in),
    /// for the settings' test: what it said.
    pub async fn check(&self, key: &str, username: &str, password: &str) -> Result<String, SearchError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(SearchError::NotSetUp);
        }
        let base = self.base.clone().ok_or(SearchError::Off)?;
        if username.trim().is_empty() {
            self.send(self.http.get(format!("{base}/infos/formats")), key).await?;
            return Ok("OpenSubtitles accepted the key.".into());
        }
        let credentials =
            Credentials { key: key.to_owned(), username: username.trim().to_owned(), password: password.to_owned() };
        self.login(&base, &credentials).await?;
        Ok("OpenSubtitles accepted the key and signed the account in.".into())
    }

    /// The account's token (and where it downloads), when one is set.
    async fn token(&self, base: &str, credentials: &Credentials) -> Result<Option<(String, String)>, SearchError> {
        if credentials.username.is_empty() || credentials.password.is_empty() {
            return Ok(None);
        }
        let mut cached = self.token.lock().await;
        if let Some(token) = cached.as_ref().filter(|t| t.credentials == *credentials && t.at.elapsed() < TOKEN_LIFE) {
            return Ok(Some((token.value.clone(), token.base.clone())));
        }
        let (value, account_base) = self.login(base, credentials).await?;
        *cached = Some(Token {
            credentials: credentials.clone(),
            value: value.clone(),
            base: account_base.clone(),
            at: Instant::now(),
        });
        Ok(Some((value, account_base)))
    }

    async fn login(&self, base: &str, credentials: &Credentials) -> Result<(String, String), SearchError> {
        let body = serde_json::json!({ "username": credentials.username, "password": credentials.password });
        let answer = self.send(self.http.post(format!("{base}/login")).json(&body), &credentials.key).await;
        let answer = match answer {
            Err(SearchError::Rejected(_)) => return Err(SearchError::Rejected("the username or password")),
            answer => answer?,
        };
        let login: LoginAnswer =
            serde_json::from_slice(&answer).map_err(|e| anyhow!("unexpected answer from OpenSubtitles: {e}"))?;
        let token = login.token.filter(|t| !t.is_empty()).ok_or(SearchError::Rejected("the username or password"))?;
        // Only the public service names another host for the account.
        let account_base = match login.base_url.filter(|host| base == API && valid_host(host)) {
            Some(host) => format!("https://{host}/api/v1"),
            None => base.to_owned(),
        };
        Ok((token, account_base))
    }

    async fn forget(&self) {
        *self.token.lock().await = None;
    }

    /// Sends a request with the API key, waiting out rate limits: the
    /// answer's body.
    async fn send(&self, request: reqwest::RequestBuilder, key: &str) -> Result<Vec<u8>, SearchError> {
        let request = request.header("Api-Key", key).header(reqwest::header::ACCEPT, "application/json");
        let mut backoff = Duration::from_secs(1);
        for _ in 0..ATTEMPTS {
            self.throttle.wait().await;
            let attempt = request.try_clone().ok_or_else(|| anyhow!("request can't be repeated"))?;
            let resp = attempt.send().await.map_err(|e| anyhow!("OpenSubtitles: {e}"))?;
            let status = resp.status().as_u16();
            let wait = retry_after(&resp);
            let body = resp.bytes().await.map_err(|e| anyhow!("OpenSubtitles: {e}"))?.to_vec();
            match status {
                200..=299 => return Ok(body),
                401 | 403 => return Err(SearchError::Rejected("the API key")),
                404 => return Err(SearchError::NotFound),
                // The day's downloads used up.
                406 => {
                    let answer: DownloadAnswer = serde_json::from_slice(&body).unwrap_or_default();
                    return Err(quota(answer.message));
                }
                429 | 503 => {
                    let wait = wait.unwrap_or(backoff).min(Duration::from_secs(10));
                    tracing::debug!("OpenSubtitles asks to slow down; waiting {wait:?}");
                    tokio::time::sleep(wait).await;
                    backoff *= 2;
                }
                status => return Err(anyhow!("OpenSubtitles answered HTTP {status}").into()),
            }
        }
        Err(anyhow!("OpenSubtitles keeps asking to slow down").into())
    }
}

/// A host name as OpenSubtitles' sign-in names the account's API host.
/// Whether a download `link` may be read: over https from the public
/// service (`base`: where its API is), as pictures are; a stand-in for it
/// (tests) may name plain http.
fn link_allowed(link: &str, base: Option<&str>) -> bool {
    link.starts_with("https://") || (base != Some(API) && link.starts_with("http://"))
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && (host == "opensubtitles.com" || host.ends_with(".opensubtitles.com"))
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("../../tests/fixtures/opensubtitles/search.json");
    const DOWNLOAD: &str = include_str!("../../tests/fixtures/opensubtitles/download.json");
    const QUOTA: &str = include_str!("../../tests/fixtures/opensubtitles/download_quota.json");

    #[test]
    fn search_results_come_hash_matches_first_then_by_downloads() {
        let found = parse_search(SEARCH.as_bytes()).unwrap();
        let ids: Vec<&str> = found.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "opensubtitles:4934512:en",
                "opensubtitles:1923552:en",
                "opensubtitles:2081045:en:hi",
                "opensubtitles:7710021:fr:forced",
                "opensubtitles:6600001:es:srt",
            ],
            "the CD set and the one without files are left out"
        );
        assert_eq!(
            found[0],
            Found {
                id: "opensubtitles:4934512:en".into(),
                provider: "opensubtitles",
                language: "en".into(),
                name: "Joker.2019.1080p.BluRay.x264-SPARKS".into(),
                format: "srt".into(),
                hearing_impaired: false,
                forced: false,
                downloads: 1204,
                matched: "hash",
            }
        );
        assert_eq!((found[1].matched, found[1].downloads), ("title", 113_298));
        assert_eq!(found[2].format, "ass", "by its file's name");
        assert!(found[2].hearing_impaired);
        // Without a release name, the file's.
        assert_eq!(found[3].name, "joker.french.forced.srt");
        assert_eq!(found[3].language, "fr");
        // MicroDVD comes as SubRip.
        assert_eq!(found[4].format, "srt");
        assert!(parse_search(b"{\"data\": null}").unwrap().is_empty());
        assert!(parse_search(b"not json").is_err());
    }

    #[test]
    fn result_ids_say_what_was_picked() {
        let picked = Picked::parse("opensubtitles:2081045:pt-BR:hi:forced:srt").unwrap();
        assert_eq!(
            picked,
            Picked { file_id: 2081045, language: "pt-BR".into(), hearing_impaired: true, forced: true, convert: true }
        );
        assert_eq!(picked.id(), "opensubtitles:2081045:pt-BR:hi:forced:srt");
        assert_eq!(Picked::parse("opensubtitles:12").unwrap().language, "");
        for wrong in [
            "",
            "opensubtitles",
            "opensubtitles:x:en",
            "opensubtitles:-4:en",
            "podnapisi:1:en",
            "opensubtitles:1:en:sdh",
        ] {
            assert_eq!(Picked::parse(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn downloads_give_their_link_or_the_quota() {
        let (link, name) = parse_download(DOWNLOAD.as_bytes()).unwrap();
        assert_eq!(link, "https://www.opensubtitles.com/download/E3B1D1A2/subfile/Joker.2019.1080p.srt");
        assert_eq!(name, "Joker.2019.1080p.BluRay.x264-SPARKS.srt");
        match parse_download(QUOTA.as_bytes()) {
            Err(SearchError::Quota(message)) => assert!(message.contains("allowed 20 subtitles"), "{message}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(parse_download(b"{}"), Err(SearchError::Other(_))));
    }

    #[test]
    fn downloads_from_the_public_service_are_https_only() {
        assert!(link_allowed("https://www.opensubtitles.com/download/x.srt", Some(API)));
        assert!(!link_allowed("http://www.opensubtitles.com/download/x.srt", Some(API)));
        assert!(!link_allowed("http://169.254.169.254/latest/meta-data", Some(API)));
        // A stand-in for it, in tests.
        assert!(link_allowed("http://127.0.0.1:9/os/file/1", Some("http://127.0.0.1:9/api/v1")));
    }

    #[test]
    fn searches_are_asked_for_as_opensubtitles_wants() {
        let search = Search {
            moviehash: Some("8e245d9679d31e12".into()),
            parent_tmdb_id: Some(1399),
            season_number: Some(1),
            episode_number: Some(2),
            query: Some("Game of Thrones".into()),
            languages: vec!["fr".into(), "en".into(), "fr".into()],
            ..Default::default()
        };
        let params: Vec<String> = search.params().into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        assert_eq!(
            params,
            [
                "episode_number=2",
                "languages=en,fr",
                "moviehash=8e245d9679d31e12",
                "parent_tmdb_id=1399",
                "query=game of thrones",
                "season_number=1",
            ]
        );
    }

    #[test]
    fn languages_are_named_as_opensubtitles_names_them() {
        assert_eq!(search_languages("eng"), ["en"]);
        assert_eq!(search_languages("fr"), ["fr"]);
        assert_eq!(search_languages("ger"), ["de"]);
        assert_eq!(search_languages("pt-BR"), ["pt-br"]);
        assert_eq!(search_languages("por"), ["pt-br", "pt-pt"]);
        assert_eq!(search_languages("chi"), ["zh-cn", "zh-tw"]);
        assert!(search_languages("").is_empty());
    }

    #[test]
    fn only_opensubtitles_hosts_are_taken_for_accounts() {
        assert!(valid_host("vip-api.opensubtitles.com"));
        assert!(!valid_host("api.example.com"));
        assert!(!valid_host("evil.com/opensubtitles.com"));
        assert!(!valid_host("evilopensubtitles.com"));
    }
}
