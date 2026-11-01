//! Radarr and Sonarr, through their API (v3): approved requests are added
//! there to be found and downloaded, the settings list what they offer
//! ([`Options`]), and their download queues say how far requests are.
//!
//! The API key goes in a header, only to the address saved with it, and
//! never along a redirect or to a link-local address.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Method;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::settings::{MinimumAvailability, Settings};
use crate::util::utc_secs;

/// How long an answer may take: adding a title has them look it up.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long a download queue is given before it is fetched again.
const QUEUE_TTL: Duration = Duration::from_secs(10);
/// How long fetching a queue may take.
const QUEUE_WAIT: Duration = Duration::from_secs(5);
/// How long a queue that couldn't be read isn't asked for again.
const QUEUE_RETRY: Duration = Duration::from_secs(60);
/// Downloads a queue lists at most (one per episode for Sonarr).
const QUEUE_SIZE: u32 = 1000;
/// What an error quotes of an answer at most.
const QUOTE: usize = 300;
/// What is read of an answer at most, whatever it says its length is: a
/// queue of 1,000 downloads with their episodes takes a few MB.
const ANSWER_MAX: usize = 32 << 20;
/// What is read of an error answer at most: only its message is quoted.
const ERROR_MAX: usize = 64 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Radarr,
    Sonarr,
}

impl Service {
    pub fn name(self) -> &'static str {
        match self {
            Service::Radarr => "Radarr",
            Service::Sonarr => "Sonarr",
        }
    }

    /// As requests store where they went.
    pub fn as_str(self) -> &'static str {
        match self {
            Service::Radarr => "radarr",
            Service::Sonarr => "sonarr",
        }
    }

    /// Its saved address and key, where both are.
    pub fn saved(self, settings: &Settings) -> Option<Connection> {
        let (url, key) = match self {
            Service::Radarr => (&settings.radarr_url, &settings.radarr_api_key),
            Service::Sonarr => (&settings.sonarr_url, &settings.sonarr_api_key),
        };
        (!url.is_empty() && !key.is_empty()).then(|| Connection { url: url.clone(), key: key.clone() })
    }

    /// The same, where it is connected: its quality profile and root folder
    /// are chosen too, so approved requests are sent there.
    pub fn connected(self, settings: &Settings) -> Option<Connection> {
        let (profile, folder) = self.chosen(settings).0;
        self.saved(settings).filter(|_| profile > 0 && !folder.is_empty())
    }

    /// The quality profile and root folder chosen in the settings, then
    /// anime's (0 and "" where none is).
    pub fn chosen(self, settings: &Settings) -> ((i64, &str), (i64, &str)) {
        match self {
            Service::Radarr => (
                (settings.radarr_quality_profile, settings.radarr_root_folder.as_str()),
                (settings.radarr_anime_quality_profile, settings.radarr_anime_root_folder.as_str()),
            ),
            Service::Sonarr => (
                (settings.sonarr_quality_profile, settings.sonarr_root_folder.as_str()),
                (settings.sonarr_anime_quality_profile, settings.sonarr_anime_root_folder.as_str()),
            ),
        }
    }
}

/// Where Radarr or Sonarr is, and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub url: String,
    pub key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ArrError {
    #[error("{0} rejected the API key")]
    Unauthorized(&'static str),
    #[error("{0} couldn't be reached: {1}")]
    Unreachable(&'static str, String),
    #[error("{0} refused: {1}")]
    Refused(&'static str, String),
    #[error("{0} answered something unexpected: {1}")]
    Unexpected(&'static str, String),
    /// Something to choose in the settings first, or to do by hand.
    #[error("{0}")]
    Needs(String),
}

/// What Radarr or Sonarr offers requests.
#[derive(Debug, Clone, Serialize)]
pub struct Options {
    pub version: String,
    pub quality_profiles: Vec<Profile>,
    pub root_folders: Vec<RootFolder>,
    /// Sonarr 3's; none otherwise.
    pub language_profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RootFolder {
    pub path: String,
    pub free_bytes: Option<i64>,
}

/// What an approval may choose from: the quality profiles and root folders.
#[derive(Debug, Clone, Default)]
pub struct Lists {
    pub quality_profiles: Vec<Profile>,
    pub root_folders: Vec<RootFolder>,
}

/// A title added (or there already): its id there, and the quality profile
/// and root folder it has, where known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub id: i64,
    pub quality_profile: Option<i64>,
    pub root_folder: Option<String>,
}

impl Added {
    /// One there already, as Radarr or Sonarr lists it.
    fn listed(service: Service, title: &Value) -> Result<Added, ArrError> {
        Ok(Added {
            id: id_of(service, title)?,
            quality_profile: title["qualityProfileId"].as_i64(),
            root_folder: title["rootFolderPath"].as_str().filter(|f| !f.is_empty()).map(str::to_owned),
        })
    }
}

/// A title to add: what the request asks for, and what to add it with.
#[derive(Debug, Clone)]
pub struct Title<'a> {
    pub tmdb_id: i64,
    pub tvdb_id: Option<i64>,
    pub anime: bool,
    /// Shows: the seasons asked for.
    pub seasons: &'a [i64],
    /// Chosen when approving, instead of the settings'.
    pub quality_profile: Option<i64>,
    pub root_folder: Option<&'a str>,
}

/// How far a request's download is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Download {
    /// 0-1, of everything of it queued.
    pub progress: f64,
    pub state: DownloadState,
    pub eta: Option<i64>,
    /// Shows: episodes queued (movies: 1).
    pub episodes: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    Queued,
    Paused,
    Importing,
    Downloading,
    Warning,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuePage {
    #[serde(default)]
    records: Vec<QueueRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRecord {
    movie_id: Option<i64>,
    series_id: Option<i64>,
    season_number: Option<i64>,
    /// Sonarr 3 names the season in the episode.
    episode: Option<QueueEpisode>,
    #[serde(default)]
    size: f64,
    #[serde(default)]
    sizeleft: f64,
    status: Option<String>,
    tracked_download_state: Option<String>,
    tracked_download_status: Option<String>,
    estimated_completion_time: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueEpisode {
    season_number: Option<i64>,
}

impl QueueRecord {
    fn state(&self) -> DownloadState {
        let tracked = self.tracked_download_state.as_deref().unwrap_or_default();
        if matches!(self.tracked_download_status.as_deref(), Some("warning" | "error"))
            || matches!(tracked, "importBlocked" | "failedPending" | "failed")
        {
            return DownloadState::Warning;
        }
        if matches!(tracked, "importPending" | "importing" | "imported") {
            return DownloadState::Importing;
        }
        match self.status.as_deref().unwrap_or_default() {
            "downloading" => DownloadState::Downloading,
            "paused" => DownloadState::Paused,
            "completed" => DownloadState::Importing,
            "failed" | "warning" => DownloadState::Warning,
            _ => DownloadState::Queued,
        }
    }

    fn season(&self) -> Option<i64> {
        self.season_number.or_else(|| self.episode.as_ref().and_then(|e| e.season_number))
    }
}

/// The download of the records of one title: `None` for none.
pub fn download(records: &[&QueueRecord]) -> Option<Download> {
    if records.is_empty() {
        return None;
    }
    let size: f64 = records.iter().map(|r| r.size.max(0.0)).sum();
    let left: f64 = records.iter().map(|r| r.sizeleft.max(0.0)).sum();
    let progress = if size > 0.0 { ((size - left) / size).clamp(0.0, 1.0) } else { 0.0 };
    Some(Download {
        progress: (progress * 1000.0).round() / 1000.0,
        state: records.iter().map(|r| r.state()).max().unwrap_or(DownloadState::Queued),
        eta: records.iter().filter_map(|r| r.estimated_completion_time.as_deref().and_then(utc_secs)).max(),
        episodes: records.len() as i64,
    })
}

/// A queue, as fetched: its records, or why there are none.
type Queue = Result<Arc<Vec<QueueRecord>>, String>;

/// Radarr's or Sonarr's queue: the last fetched, with when that fetch
/// started and from where, and whether one runs in the background now.
#[derive(Default)]
struct Queued {
    last: Option<(Instant, Connection, Queue)>,
    fetching: bool,
}

#[derive(Clone)]
pub struct Arr {
    http: reqwest::Client,
    /// Radarr's queue, then Sonarr's.
    queues: Arc<[parking_lot::Mutex<Queued>; 2]>,
}

impl Default for Arr {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `ip` is link-local, where clouds answer for the machine itself
/// (169.254.169.254): never where Radarr or Sonarr is.
pub fn link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_link_local(),
        IpAddr::V6(ip) => {
            (ip.segments()[0] & 0xffc0) == 0xfe80 || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_link_local())
        }
    }
}

/// Resolves Radarr's and Sonarr's names without the link-local addresses
/// ([`link_local`]), failing when none is left: an address typed in is
/// checked in the settings, and this checks the address a name leads to,
/// the one connected to.
struct NotLinkLocal;

impl Resolve for NotLinkLocal {
    fn resolve(&self, name: Name) -> Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.filter(|addr| !link_local(addr.ip())).collect();
            if addrs.is_empty() {
                return Err(format!("{name} has only link-local addresses, where Radarr and Sonarr aren't").into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

impl Arr {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .dns_resolver(Arc::new(NotLinkLocal))
            .build()
            .expect("http client");
        Self { http, queues: Default::default() }
    }

    async fn call<T: DeserializeOwned>(
        &self,
        service: Service,
        to: &Connection,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<T, ArrError> {
        let name = service.name();
        let mut req = self.http.request(method, format!("{}/api/v3{path}", to.url)).header("X-Api-Key", &to.key);
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await.map_err(|e| ArrError::Unreachable(name, reason(&e.without_url())))?;
        let status = resp.status();
        if matches!(status.as_u16(), 401 | 403) {
            return Err(ArrError::Unauthorized(name));
        }
        if status.is_redirection() {
            let message = "it answered with a redirect: give its address with its base URL, as its own settings show";
            return Err(ArrError::Unexpected(name, message.to_owned()));
        }
        if !status.is_success() {
            let text = read_answer(resp, ERROR_MAX).await.unwrap_or_default();
            let message =
                message_of(&String::from_utf8_lossy(&text)).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Err(ArrError::Refused(name, message));
        }
        let bytes = read_answer(resp, ANSWER_MAX).await.map_err(|why| ArrError::Unexpected(name, why))?;
        serde_json::from_slice(&bytes).map_err(|e| ArrError::Unexpected(name, quote(&e.to_string())))
    }

    async fn get<T: DeserializeOwned>(
        &self,
        service: Service,
        to: &Connection,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ArrError> {
        self.call(service, to, Method::GET, path, query, None).await
    }

    /// Its version, quality profiles, root folders and (Sonarr 3's)
    /// language profiles.
    pub async fn options(&self, service: Service, to: &Connection) -> Result<Options, ArrError> {
        #[derive(Deserialize)]
        struct Status {
            version: String,
        }
        let status: Status = self.get(service, to, "/system/status", &[]).await?;
        let lists = self.lists(service, to).await?;
        let language_profiles = if service == Service::Sonarr && status.version.starts_with("3.") {
            self.get(service, to, "/languageprofile", &[]).await?
        } else {
            Vec::new()
        };
        Ok(Options {
            version: status.version,
            quality_profiles: lists.quality_profiles,
            root_folders: lists.root_folders,
            language_profiles,
        })
    }

    /// Its quality profiles and root folders, asked for at once.
    pub async fn lists(&self, service: Service, to: &Connection) -> Result<Lists, ArrError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Folder {
            path: String,
            free_space: Option<i64>,
        }
        let (quality_profiles, folders) = tokio::try_join!(
            self.get::<Vec<Profile>>(service, to, "/qualityprofile", &[]),
            self.get::<Vec<Folder>>(service, to, "/rootfolder", &[]),
        )?;
        Ok(Lists {
            quality_profiles,
            root_folders: folders.into_iter().map(|f| RootFolder { path: f.path, free_bytes: f.free_space }).collect(),
        })
    }

    /// Adds a movie to Radarr, monitored, and searches for it; one there
    /// already is monitored again and searched for unless it has a file.
    pub async fn add_movie(&self, to: &Connection, settings: &Settings, title: &Title<'_>) -> Result<Added, ArrError> {
        let radarr = Service::Radarr;
        let tmdb = [("tmdbId", title.tmdb_id.to_string())];
        let found: Vec<Value> = self.get(radarr, to, "/movie", &tmdb).await?;
        if let Some(mut movie) = found.into_iter().next() {
            let added = Added::listed(radarr, &movie)?;
            let id = added.id;
            if movie["monitored"] != json!(true) {
                movie["monitored"] = json!(true);
                let _: Value = self.call(radarr, to, Method::PUT, &format!("/movie/{id}"), &[], Some(&movie)).await?;
            }
            if settings.radarr_search && movie["hasFile"] != json!(true) {
                let command = json!({"name": "MoviesSearch", "movieIds": [id]});
                let _: Value = self.call(radarr, to, Method::POST, "/command", &[], Some(&command)).await?;
            }
            return Ok(added);
        }
        let (profile, folder) = choice(
            radarr,
            title,
            (settings.radarr_quality_profile, &settings.radarr_root_folder),
            (settings.radarr_anime_quality_profile, &settings.radarr_anime_root_folder),
        )?;
        let mut movie: Value = self.get(radarr, to, "/movie/lookup/tmdb", &tmdb).await?;
        if !movie.is_object() {
            return Err(ArrError::Unexpected(radarr.name(), "its lookup found no movie".to_owned()));
        }
        movie["qualityProfileId"] = json!(profile);
        movie["rootFolderPath"] = json!(folder);
        movie["monitored"] = json!(true);
        movie["minimumAvailability"] = json!(match settings.radarr_minimum_availability {
            MinimumAvailability::Announced => "announced",
            MinimumAvailability::InCinemas => "inCinemas",
            MinimumAvailability::Released => "released",
        });
        movie["addOptions"] = json!({"searchForMovie": settings.radarr_search});
        if !movie["tags"].is_array() {
            movie["tags"] = json!([]);
        }
        let added: Value = self.call(radarr, to, Method::POST, "/movie", &[], Some(&movie)).await?;
        Ok(Added { id: id_of(radarr, &added)?, quality_profile: Some(profile), root_folder: Some(folder.to_owned()) })
    }

    /// Adds a show to Sonarr with the seasons asked for monitored, and
    /// searches for them; one there already gets them monitored too.
    pub async fn add_series(&self, to: &Connection, settings: &Settings, title: &Title<'_>) -> Result<Added, ArrError> {
        let sonarr = Service::Sonarr;
        let tvdb = title.tvdb_id.ok_or_else(|| {
            ArrError::Needs(
                "TMDB knows no TheTVDB id for this show, which Sonarr needs: add it to Sonarr by hand".into(),
            )
        })?;
        let wanted = |season: &Value| season["seasonNumber"].as_i64().is_some_and(|n| title.seasons.contains(&n));
        let found: Vec<Value> = self.get(sonarr, to, "/series", &[("tvdbId", tvdb.to_string())]).await?;
        if let Some(mut series) = found.into_iter().next() {
            let added = Added::listed(sonarr, &series)?;
            let id = added.id;
            series["monitored"] = json!(true);
            for season in series["seasons"].as_array_mut().into_iter().flatten() {
                if wanted(&*season) {
                    season["monitored"] = json!(true);
                }
            }
            let _: Value = self.call(sonarr, to, Method::PUT, &format!("/series/{id}"), &[], Some(&series)).await?;
            if settings.sonarr_search {
                for season in title.seasons {
                    let command = json!({"name": "SeasonSearch", "seriesId": id, "seasonNumber": season});
                    let _: Value = self.call(sonarr, to, Method::POST, "/command", &[], Some(&command)).await?;
                }
            }
            return Ok(added);
        }
        let (profile, folder) = choice(
            sonarr,
            title,
            (settings.sonarr_quality_profile, &settings.sonarr_root_folder),
            (settings.sonarr_anime_quality_profile, &settings.sonarr_anime_root_folder),
        )?;
        let found: Vec<Value> = self.get(sonarr, to, "/series/lookup", &[("term", format!("tvdb:{tvdb}"))]).await?;
        let Some(mut series) = found.into_iter().next().filter(Value::is_object) else {
            return Err(ArrError::Refused(sonarr.name(), format!("it found no show with TheTVDB id {tvdb}")));
        };
        series["qualityProfileId"] = json!(profile);
        series["rootFolderPath"] = json!(folder);
        series["seasonFolder"] = json!(settings.sonarr_season_folders);
        series["monitored"] = json!(true);
        series["seriesType"] = json!(if title.anime { "anime" } else { "standard" });
        // Sonarr 3 needs a language profile; Sonarr 4 has none, and no
        // longer lists them.
        let language = match settings.sonarr_language_profile {
            0 => {
                let profiles = self.get::<Vec<Profile>>(sonarr, to, "/languageprofile", &[]).await.ok();
                profiles.and_then(|p| p.first().map(|p| p.id))
            }
            id => Some(id),
        };
        if let Some(language) = language {
            series["languageProfileId"] = json!(language);
        }
        for season in series["seasons"].as_array_mut().into_iter().flatten() {
            let monitored = wanted(&*season);
            season["monitored"] = json!(monitored);
        }
        series["addOptions"] = json!({
            "ignoreEpisodesWithFiles": true,
            "ignoreEpisodesWithoutFiles": false,
            "searchForMissingEpisodes": settings.sonarr_search,
        });
        if !series["tags"].is_array() {
            series["tags"] = json!([]);
        }
        let added: Value = self.call(sonarr, to, Method::POST, "/series", &[], Some(&series)).await?;
        Ok(Added { id: id_of(sonarr, &added)?, quality_profile: Some(profile), root_folder: Some(folder.to_owned()) })
    }

    /// Whether Sonarr has a file for every episode of a season of a show
    /// (counting files of several episodes as each of them).
    pub async fn season_complete(&self, to: &Connection, series_id: i64, season: i64) -> Result<bool, ArrError> {
        let series: Value = self.get(Service::Sonarr, to, &format!("/series/{series_id}"), &[]).await?;
        let complete = series["seasons"].as_array().into_iter().flatten().any(|s| {
            let stats = &s["statistics"];
            let (files, episodes) = (stats["episodeFileCount"].as_i64(), stats["episodeCount"].as_i64());
            s["seasonNumber"].as_i64() == Some(season)
                && matches!((files, episodes), (Some(files), Some(episodes)) if episodes > 0 && files >= episodes)
        });
        Ok(complete)
    }

    /// What Radarr's or Sonarr's queue at `to` listed when last fetched, by
    /// movie or show id with the season of each, without waiting for it:
    /// once that is 10 seconds old (a minute, when it couldn't be read), it
    /// is fetched again in the background, one fetch at a time. `None`
    /// until it first is, and while it can't be read.
    pub fn queue(&self, service: Service, to: &Connection) -> Option<Arc<Vec<QueueRecord>>> {
        let mut queued = self.queues[service as usize].lock();
        let last = queued.last.as_ref().filter(|(_, from, _)| from == to);
        let fresh =
            last.is_some_and(|(at, _, queue)| at.elapsed() < if queue.is_ok() { QUEUE_TTL } else { QUEUE_RETRY });
        let records = last.and_then(|(_, _, queue)| queue.as_ref().ok().cloned());
        if !fresh && !queued.fetching {
            queued.fetching = true;
            let (arr, to) = (self.clone(), to.clone());
            tokio::spawn(async move {
                arr.fetch_queue(service, &to).await;
                arr.queues[service as usize].lock().fetching = false;
            });
        }
        records
    }

    /// Fetches Radarr's or Sonarr's queue at `to` now, for [`Self::queue`]
    /// to give. Its answer is kept unless a fetch started after it has kept
    /// one already.
    pub async fn fetch_queue(&self, service: Service, to: &Connection) {
        let started = Instant::now();
        let mut query = vec![("page", "1".to_owned()), ("pageSize", QUEUE_SIZE.to_string())];
        match service {
            Service::Radarr => query.push(("includeUnknownMovieItems", "false".into())),
            Service::Sonarr => {
                query.push(("includeUnknownSeriesItems", "false".into()));
                query.push(("includeEpisode", "true".into()));
            }
        }
        let fetched = tokio::time::timeout(QUEUE_WAIT, self.get::<QueuePage>(service, to, "/queue", &query)).await;
        let queue = match fetched {
            Ok(Ok(page)) => Ok(Arc::new(page.records)),
            Ok(Err(err)) => {
                tracing::debug!("{err}");
                Err(err.to_string())
            }
            Err(_) => Err(format!("{} didn't answer in time", service.name())),
        };
        let mut queued = self.queues[service as usize].lock();
        if queued.last.as_ref().is_none_or(|(at, ..)| *at <= started) {
            queued.last = Some((started, to.clone(), queue));
        }
    }
}

/// The records of a queue for a movie (`seasons` none) or for some seasons
/// of a show, by Radarr's or Sonarr's id.
pub fn records_of<'a>(queue: &'a [QueueRecord], arr_id: i64, seasons: Option<&[i64]>) -> Vec<&'a QueueRecord> {
    queue
        .iter()
        .filter(|r| match seasons {
            None => r.movie_id == Some(arr_id),
            Some(seasons) => r.series_id == Some(arr_id) && r.season().is_none_or(|s| seasons.contains(&s)),
        })
        .collect()
}

/// The quality profile and root folder a title goes with: those chosen
/// when approving, else anime's own where set, else the settings'.
fn choice<'a>(
    service: Service,
    title: &Title<'a>,
    usual: (i64, &'a str),
    anime: (i64, &'a str),
) -> Result<(i64, &'a str), ArrError> {
    let profile =
        title.quality_profile.filter(|p| *p > 0).unwrap_or(if title.anime && anime.0 > 0 { anime.0 } else { usual.0 });
    let folder = title.root_folder.filter(|f| !f.is_empty()).unwrap_or(if title.anime && !anime.1.is_empty() {
        anime.1
    } else {
        usual.1
    });
    let name = service.name();
    if profile <= 0 {
        return Err(ArrError::Needs(format!("choose {name}'s quality profile in Settings → Requests")));
    }
    if folder.is_empty() {
        return Err(ArrError::Needs(format!("choose {name}'s root folder in Settings → Requests")));
    }
    Ok((profile, folder))
}

fn id_of(service: Service, value: &Value) -> Result<i64, ArrError> {
    value["id"].as_i64().ok_or_else(|| ArrError::Unexpected(service.name(), "no id in its answer".to_owned()))
}

/// An answer's body, `max` bytes at most whatever its length says; why
/// not, when it is larger or breaks off.
async fn read_answer(mut resp: reqwest::Response, max: usize) -> Result<Vec<u8>, String> {
    let too_large = || format!("its answer is larger than {} MB", max.div_ceil(1 << 20));
    if resp.content_length().is_some_and(|n| n > max as u64) {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| reason(&e.without_url()))? {
        if body.len() + chunk.len() > max {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// What went wrong with a request, without the address (reqwest's own
/// words, then their causes).
fn reason(err: &reqwest::Error) -> String {
    let mut text = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    quote(&text)
}

/// The message of an error answer: validation errors' (`[{ errorMessage }]`)
/// or `{ message }`; `None` for anything else, whose text isn't quoted (a
/// proxy's page, or whatever answers at an address that isn't Radarr's).
fn message_of(text: &str) -> Option<String> {
    let message = match serde_json::from_str::<Value>(text).ok()? {
        Value::Array(errors) => errors.iter().filter_map(|e| e["errorMessage"].as_str()).collect::<Vec<_>>().join("; "),
        value => value["message"].as_str().unwrap_or_default().to_owned(),
    };
    (!message.is_empty()).then(|| quote(&message))
}

fn quote(text: &str) -> String {
    match text.char_indices().nth(QUOTE) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(value: Value) -> QueueRecord {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_download_adds_its_parts_up() {
        let queue = vec![
            record(json!({"seriesId": 7, "seasonNumber": 1, "size": 1000.0, "sizeleft": 250.0,
                          "status": "downloading", "trackedDownloadState": "downloading",
                          "trackedDownloadStatus": "ok", "estimatedCompletionTime": "2024-05-01T12:00:00Z"})),
            record(json!({"seriesId": 7, "episode": {"seasonNumber": 2}, "size": 1000.0, "sizeleft": 1000.0,
                          "status": "queued", "estimatedCompletionTime": "2024-05-01T13:00:00.5Z"})),
            record(json!({"seriesId": 7, "seasonNumber": 3, "size": 5.0, "sizeleft": 0.0,
                          "trackedDownloadState": "importBlocked"})),
            record(json!({"movieId": 7, "size": 0.0, "status": "paused"})),
        ];
        let show = download(&records_of(&queue, 7, Some(&[1, 2]))).unwrap();
        assert_eq!(show.state, DownloadState::Downloading);
        assert_eq!((show.progress, show.episodes), (0.375, 2));
        assert_eq!(show.eta, crate::util::utc_secs("2024-05-01T13:00:00Z"), "when the last one ends");
        let blocked = download(&records_of(&queue, 7, Some(&[3]))).unwrap();
        assert_eq!((blocked.state, blocked.progress), (DownloadState::Warning, 1.0));
        let movie = download(&records_of(&queue, 7, None)).unwrap();
        assert_eq!((movie.state, movie.progress, movie.episodes), (DownloadState::Paused, 0.0, 1));
        assert!(download(&records_of(&queue, 8, None)).is_none());
    }

    #[test]
    fn errors_say_what_they_said() {
        let validation =
            r#"[{"propertyName":"Path","errorMessage":"Path is already configured"},{"errorMessage":"Invalid"}]"#;
        assert_eq!(message_of(validation).as_deref(), Some("Path is already configured; Invalid"));
        assert_eq!(message_of(r#"{"message":"NotFound"}"#).as_deref(), Some("NotFound"));
        assert_eq!(message_of(" <html>Bad gateway</html> ").as_deref(), None, "only HTTP 502 is said");
        assert_eq!(message_of("").as_deref(), None);
        assert_eq!(message_of(r#"{"title":"oops"}"#).as_deref(), None);
        let long = "é".repeat(QUOTE + 5);
        assert_eq!(quote(&long).chars().count(), QUOTE + 1);
    }

    #[tokio::test]
    async fn names_never_lead_to_link_local_addresses() {
        for ip in ["169.254.169.254", "fe80::1", "::ffff:169.254.1.1"] {
            assert!(link_local(ip.parse().unwrap()), "{ip}");
            let resolved = NotLinkLocal.resolve(ip.parse().unwrap()).await;
            assert!(resolved.is_err(), "{ip}");
        }
        for ip in ["127.0.0.1", "192.168.1.5", "::1", "fd00::5"] {
            assert!(!link_local(ip.parse().unwrap()), "{ip}");
        }
        let resolved: Vec<SocketAddr> = NotLinkLocal.resolve("localhost".parse().unwrap()).await.unwrap().collect();
        assert!(!resolved.is_empty() && resolved.iter().all(|a| a.ip().is_loopback()), "{resolved:?}");
    }

    #[tokio::test]
    async fn answers_are_read_up_to_a_limit() {
        let answer = |len: usize| reqwest::Response::from(axum::http::Response::new(vec![b'x'; len]));
        assert_eq!(read_answer(answer(10), 10).await.unwrap().len(), 10);
        assert!(read_answer(answer(11), 10).await.is_err());
        assert_eq!(
            read_answer(answer(ANSWER_MAX + 1), ANSWER_MAX).await.unwrap_err(),
            "its answer is larger than 32 MB"
        );
    }

    #[test]
    fn titles_go_with_what_was_chosen() {
        let title = |anime: bool, quality_profile: Option<i64>, root_folder: Option<&'static str>| Title {
            tmdb_id: 1,
            tvdb_id: None,
            anime,
            seasons: &[],
            quality_profile,
            root_folder,
        };
        let usual = (4, "/movies");
        let anime = (9, "/anime");
        let radarr = Service::Radarr;
        assert_eq!(choice(radarr, &title(false, None, None), usual, anime).unwrap(), (4, "/movies"));
        assert_eq!(choice(radarr, &title(true, None, None), usual, anime).unwrap(), (9, "/anime"));
        assert_eq!(choice(radarr, &title(true, None, None), usual, (0, "")).unwrap(), (4, "/movies"));
        assert_eq!(choice(radarr, &title(true, Some(2), Some("/x")), usual, anime).unwrap(), (2, "/x"));
        assert!(matches!(choice(radarr, &title(false, None, None), (0, "/m"), anime), Err(ArrError::Needs(_))));
        assert!(matches!(choice(radarr, &title(false, None, None), (3, ""), anime), Err(ArrError::Needs(_))));
    }
}
