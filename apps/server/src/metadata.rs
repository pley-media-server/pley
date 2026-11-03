//! Metadata agent backed by The Movie Database (TMDB). Enabled when an API key
//! (v3 key or v4 read access token) is configured in the settings. Local
//! artwork always wins over downloaded artwork.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use futures::StreamExt;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::db::Db;
use crate::library::{LibraryKind, LibraryRow};
use crate::overrides::RATING_LOCKED;

/// Items looked up at the same time. Each lookup makes a few requests (and
/// may fetch two pictures); TMDB allows about 40 a second and 429 answers
/// are retried with a pause.
const LOOKUPS: usize = 8;
use crate::people;
use crate::providers::{Endpoints, ProviderError, Providers, TvSource};
use crate::settings::SettingsStore;
use crate::store::{self, Store};
use crate::util::{normalize_key, now, sort_title};

pub(crate) const API: &str = "https://api.themoviedb.org/3";
pub(crate) const IMAGES: &str = "https://image.tmdb.org/t/p";

pub struct MetadataAgent {
    db: Db,
    settings: Arc<SettingsStore>,
    http: reqwest::Client,
    /// TMDB's API base URL ([`Endpoints::tmdb`]).
    api: String,
    store: Arc<Store>,
    /// One credits backfill at a time (startup and scans can both start one).
    backfill: tokio::sync::Mutex<()>,
    /// TV shows from TVmaze or TheTVDB, music from MusicBrainz.
    providers: Providers,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct Candidate {
    id: i64,
    kind: String,
    title: String,
    year: Option<i64>,
    tmdb_id: Option<i64>,
    imdb_id: Option<String>,
    poster: Option<String>,
    backdrop: Option<String>,
}

#[derive(Deserialize)]
struct SearchResults {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    id: i64,
    #[serde(alias = "name")]
    title: Option<String>,
    #[serde(alias = "original_name")]
    original_title: Option<String>,
    #[serde(alias = "first_air_date")]
    release_date: Option<String>,
    #[serde(default)]
    popularity: f64,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
}

/// A possible TMDB match, for "Fix match".
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchCandidate {
    pub tmdb_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
}

/// Artwork TMDB has for an item.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ArtworkCandidate {
    /// Full-size image (what gets downloaded when chosen).
    pub url: String,
    pub preview_url: String,
    pub width: i64,
    pub height: i64,
    pub language: Option<String>,
    pub votes: f64,
}

#[derive(Deserialize)]
struct Images {
    #[serde(default)]
    posters: Vec<ImageInfo>,
    #[serde(default)]
    backdrops: Vec<ImageInfo>,
    #[serde(default)]
    stills: Vec<ImageInfo>,
}

#[derive(Deserialize)]
struct ImageInfo {
    file_path: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
    iso_639_1: Option<String>,
    #[serde(default)]
    vote_average: f64,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct MovieDetails {
    title: Option<String>,
    original_title: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    vote_average: Option<f64>,
    release_date: Option<String>,
    runtime: Option<i64>,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    production_companies: Vec<Named>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    imdb_id: Option<String>,
    release_dates: Option<ReleaseDates>,
    credits: Option<people::Credits>,
    #[serde(default)]
    belongs_to_collection: Option<crate::collections::TmdbCollection>,
}

#[derive(Deserialize)]
struct ReleaseDates {
    #[serde(default)]
    results: Vec<CountryReleases>,
}

#[derive(Deserialize)]
struct CountryReleases {
    iso_3166_1: String,
    #[serde(default)]
    release_dates: Vec<Certification>,
}

#[derive(Deserialize)]
struct Certification {
    #[serde(default)]
    certification: String,
}

#[derive(Deserialize)]
struct TvDetails {
    name: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    vote_average: Option<f64>,
    first_air_date: Option<String>,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    networks: Vec<Named>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    external_ids: Option<ExternalIds>,
    content_ratings: Option<ContentRatings>,
    aggregate_credits: Option<people::AggregateCredits>,
    #[serde(default)]
    created_by: Vec<people::Creator>,
}

/// Just the credits of a show (backfill).
#[derive(Deserialize)]
struct ShowCredits {
    #[serde(default)]
    created_by: Vec<people::Creator>,
    aggregate_credits: Option<people::AggregateCredits>,
}

#[derive(Deserialize)]
struct ExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
}

#[derive(Deserialize)]
struct ContentRatings {
    #[serde(default)]
    results: Vec<ContentRating>,
}

#[derive(Deserialize)]
struct ContentRating {
    iso_3166_1: String,
    rating: String,
}

#[derive(Deserialize)]
struct SeasonDetails {
    name: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    air_date: Option<String>,
    #[serde(default)]
    episodes: Vec<EpisodeDetails>,
}

/// An episode of the season the library has, as the season's metadata sees
/// it: its number (`None` when its file name gave none), its title and the
/// length of its file.
struct SeasonItem<'a> {
    number: Option<i64>,
    title: &'a str,
    duration_ms: Option<i64>,
}

/// The season's episode an item stands for: by its number, or for an episode
/// whose file name gave none, by its title (the same words, any case and
/// punctuation) when exactly one episode of the season has it, no other file
/// of the season has its number, and its length is close to the file's. A
/// film kept in a show's folder isn't taken for the episode of the same name
/// (Firefly's pilot and the film are both "Serenity").
fn season_episode<'a>(
    episodes: &'a [EpisodeDetails],
    item: &SeasonItem,
    taken: &HashSet<i64>,
) -> Option<&'a EpisodeDetails> {
    if let Some(n) = item.number {
        return episodes.iter().find(|e| e.episode_number == n);
    }
    let wanted = crate::util::normalize_key(item.title);
    if wanted.is_empty() {
        return None;
    }
    let mut same =
        episodes.iter().filter(|e| e.name.as_deref().is_some_and(|n| crate::util::normalize_key(n) == wanted));
    let ep = match (same.next(), same.next()) {
        (Some(ep), None) => ep,
        _ => return None,
    };
    let length_fits = match (ep.runtime.filter(|r| *r > 0), item.duration_ms.filter(|d| *d > 0)) {
        (Some(minutes), Some(ms)) => {
            let ratio = ms as f64 / (minutes as f64 * 60_000.0);
            (0.7..=1.3).contains(&ratio)
        }
        _ => true,
    };
    (!taken.contains(&ep.episode_number) && length_fits).then_some(ep)
}

#[derive(Deserialize)]
struct EpisodeDetails {
    episode_number: i64,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    still_path: Option<String>,
    vote_average: Option<f64>,
    /// Minutes.
    #[serde(default)]
    runtime: Option<i64>,
    #[serde(default)]
    crew: Vec<people::CrewCredit>,
    #[serde(default)]
    guest_stars: Vec<people::CastCredit>,
}

#[derive(Deserialize)]
struct FindResults {
    #[serde(default)]
    movie_results: Vec<SearchResult>,
    #[serde(default)]
    tv_results: Vec<SearchResult>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TmdbError {
    #[error("TMDB rejected the API key")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<ProviderError> for TmdbError {
    fn from(err: ProviderError) -> Self {
        match err {
            ProviderError::Unauthorized(_) => TmdbError::Unauthorized,
            ProviderError::NotFound => TmdbError::NotFound,
            ProviderError::Other(err) => TmdbError::Other(err),
        }
    }
}

impl MetadataAgent {
    pub fn new(db: Db, settings: Arc<SettingsStore>, store: Arc<Store>) -> Arc<Self> {
        Self::with_endpoints(db, settings, store, Some(Endpoints::default()))
    }

    /// `endpoints`: where TVmaze, TheTVDB and MusicBrainz are reached; `None`
    /// never contacts them.
    pub fn with_endpoints(
        db: Db,
        settings: Arc<SettingsStore>,
        store: Arc<Store>,
        endpoints: Option<Endpoints>,
    ) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client");
        let api = endpoints.as_ref().map_or_else(|| API.to_owned(), |e| e.tmdb.clone());
        let providers = Providers::new(db.clone(), settings.clone(), store.clone(), endpoints);
        Arc::new(Self { db, settings, http, api, store, backfill: tokio::sync::Mutex::new(()), providers })
    }

    /// Whether any metadata source is in use (TMDB, a TV source or MusicBrainz).
    pub fn enabled(&self) -> bool {
        self.tmdb_enabled() || self.can_refresh("show") || self.can_refresh("album")
    }

    /// TMDB has a key: movies, collections, "Fix match" and artwork choices.
    pub fn tmdb_enabled(&self) -> bool {
        !self.settings.get().tmdb_api_key.is_empty()
    }

    /// Whether items of this kind (`movie`, `show`, `episode`, `album`, …)
    /// get metadata from somewhere.
    pub fn can_refresh(&self, kind: &str) -> bool {
        match kind {
            "movie" => self.tmdb_enabled(),
            "show" | "season" | "episode" => match self.providers.tv_source() {
                Some(TvSource::Tmdb) => self.tmdb_enabled(),
                Some(_) => true,
                None => false,
            },
            "artist" | "album" | "track" => self.providers.music_enabled(),
            _ => false,
        }
    }

    /// Whether scans of a library of this kind fetch metadata.
    pub fn enabled_for(&self, kind: LibraryKind) -> bool {
        self.can_refresh(match kind {
            LibraryKind::Movies => "movie",
            LibraryKind::Shows => "show",
            LibraryKind::Music => "album",
        })
    }

    /// Fetch metadata for items that never had any (or all, when `force`).
    pub async fn refresh_library(
        &self,
        lib: &LibraryRow,
        force: bool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> anyhow::Result<()> {
        let kind = match lib.kind() {
            LibraryKind::Movies => "movie",
            LibraryKind::Shows => "show",
            LibraryKind::Music => return self.providers.refresh_music_library(lib, force, progress).await,
        };
        let items: Vec<Candidate> = sqlx::query_as(
            "SELECT id, kind, title, year, tmdb_id, imdb_id, poster, backdrop FROM items
             WHERE library_id = ? AND kind = ? AND metadata_locked = 0 AND (? OR metadata_at IS NULL)
             ORDER BY added_at DESC",
        )
        .bind(lib.id)
        .bind(kind)
        .bind(force)
        .fetch_all(&self.db.read)
        .await?;
        let total = items.len() as u64;
        if total == 0 {
            return Ok(());
        }
        tracing::info!("fetching metadata for {total} {kind}s in library {}", lib.name);
        let mut done = 0u64;
        let mut stream = futures::stream::iter(items.into_iter().map(|item| async move {
            let id = item.id;
            let result = self.refresh_candidate(item).await;
            // Written here rather than in the loop below: while the loop
            // waits, the stream is not polled, and another refresh in it may
            // hold the (single) write connection.
            if matches!(result, Err(TmdbError::NotFound)) {
                let _ = sqlx::query("UPDATE items SET metadata_at = ? WHERE id = ?")
                    .bind(-now())
                    .bind(id)
                    .execute(&self.db.write)
                    .await;
            }
            (id, result)
        }))
        .buffer_unordered(LOOKUPS);
        while let Some((id, result)) = stream.next().await {
            done += 1;
            match result {
                Ok(()) => {}
                Err(TmdbError::Unauthorized) => {
                    let source =
                        if kind == "show" { self.providers.tv_source().map_or("TMDB", TvSource::name) } else { "TMDB" };
                    tracing::error!("{source} rejected the API key; check Settings → Server");
                    bail!("invalid {source} API key");
                }
                Err(TmdbError::NotFound) => tracing::debug!("no metadata match for item {id}"),
                Err(TmdbError::Other(err)) => tracing::warn!("metadata for item {id} failed: {err:#}"),
            }
            if done.is_multiple_of(5) || done == total {
                progress(done, total);
            }
        }
        self.backfill_credits(Some(lib.id)).await;
        Ok(())
    }

    /// Cast and crew of movies and shows matched before credits were stored:
    /// one request per movie, and one per show and season.
    pub async fn backfill_credits(&self, library_id: Option<i64>) {
        if !self.tmdb_enabled() {
            return;
        }
        let _running = self.backfill.lock().await;
        // Shows only when TMDB is their source.
        let shows = self.providers.tv_source() == Some(TvSource::Tmdb);
        let items: Vec<(i64, String, i64)> = match sqlx::query_as(
            "SELECT id, kind, tmdb_id FROM items
             WHERE (kind = 'movie' OR (kind = 'show' AND ?2)) AND tmdb_id IS NOT NULL AND credits_at IS NULL
               AND metadata_locked = 0 AND (?1 IS NULL OR library_id = ?1)",
        )
        .bind(library_id)
        .bind(shows)
        .fetch_all(&self.db.read)
        .await
        {
            Ok(items) => items,
            Err(err) => {
                tracing::warn!("credits backfill: {err}");
                return;
            }
        };
        if items.is_empty() {
            return;
        }
        tracing::info!("fetching cast and crew for {} items", items.len());
        let mut stream = futures::stream::iter(items.into_iter().map(|(id, kind, tmdb_id)| async move {
            let result = if kind == "movie" {
                self.movie_credits(id, tmdb_id).await
            } else {
                self.show_credits(id, tmdb_id).await
            };
            // Nothing to fetch: don't ask again. Written here, not in the
            // loop below (see `refresh_library`).
            if matches!(result, Err(TmdbError::NotFound)) {
                let _ = people::replace(&self.db, id, &[]).await;
            }
            (id, result)
        }))
        .buffer_unordered(LOOKUPS);
        while let Some((id, result)) = stream.next().await {
            match result {
                Ok(()) | Err(TmdbError::NotFound) => {}
                Err(TmdbError::Unauthorized) => return,
                Err(TmdbError::Other(err)) => tracing::debug!("cast and crew of item {id} failed: {err:#}"),
            }
        }
    }

    async fn movie_credits(&self, item_id: i64, tmdb_id: i64) -> Result<(), TmdbError> {
        let credits: people::Credits = self.get(&format!("/movie/{tmdb_id}/credits"), &[]).await?;
        people::replace(&self.db, item_id, &people::movie_credits(&credits))
            .await
            .map_err(|e| TmdbError::Other(e.into()))
    }

    async fn show_credits(&self, show_id: i64, tmdb_id: i64) -> Result<(), TmdbError> {
        let show: ShowCredits =
            self.get(&format!("/tv/{tmdb_id}"), &[("append_to_response", "aggregate_credits".into())]).await?;
        let seasons: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id, idx FROM items WHERE parent_id = ? AND kind = 'season' AND idx IS NOT NULL AND metadata_locked = 0",
        )
        .bind(show_id)
        .fetch_all(&self.db.read)
        .await
        .map_err(|e| TmdbError::Other(e.into()))?;
        for (season_id, number) in seasons {
            let season: SeasonDetails = match self.get(&format!("/tv/{tmdb_id}/season/{number}"), &[]).await {
                Ok(s) => s,
                Err(TmdbError::NotFound) => continue,
                Err(e) => return Err(e),
            };
            self.store_episode_credits(season_id, &season).await?;
        }
        // The show last: its `credits_at` marks the whole show as done.
        let credits = people::show_credits(&show.aggregate_credits.unwrap_or_default(), &show.created_by);
        people::replace(&self.db, show_id, &credits).await.map_err(|e| TmdbError::Other(e.into()))
    }

    async fn store_episode_credits(&self, season_id: i64, season: &SeasonDetails) -> Result<(), TmdbError> {
        let episodes: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id, idx FROM items WHERE parent_id = ? AND kind = 'episode' AND idx IS NOT NULL AND metadata_locked = 0",
        )
        .bind(season_id)
        .fetch_all(&self.db.read)
        .await
        .map_err(|e| TmdbError::Other(e.into()))?;
        for (episode_id, number) in episodes {
            let Some(ep) = season.episodes.iter().find(|e| e.episode_number == number) else { continue };
            people::replace(&self.db, episode_id, &people::episode_credits(&ep.guest_stars, &ep.crew))
                .await
                .map_err(|e| TmdbError::Other(e.into()))?;
        }
        Ok(())
    }

    /// Forget people no longer credited anywhere (after scans removed items).
    pub async fn prune_people(&self) {
        if let Err(err) = people::prune(&self.db).await {
            tracing::warn!("pruning people failed: {err}");
        }
    }

    /// A person's photo on disk, downloaded the first time.
    pub async fn person_photo(&self, url: &str) -> anyhow::Result<PathBuf> {
        let blob = self.store.remote(url).await?;
        self.store.path(&blob).context("invalid blob")
    }

    /// Refresh one item (movie, show, season or episode → its show).
    pub async fn refresh_item(&self, item_id: i64) -> anyhow::Result<()> {
        let row: Option<(String, Option<i64>, Option<i64>)> =
            sqlx::query_as("SELECT kind, parent_id, grandparent_id FROM items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&self.db.read)
                .await?;
        let Some((kind, parent, grandparent)) = row else { bail!("item not found") };
        if !self.can_refresh(&kind) {
            return Ok(());
        }
        let target = match kind.as_str() {
            "movie" | "show" | "artist" | "album" => item_id,
            "season" => parent.context("season without show")?,
            "episode" => grandparent.context("episode without show")?,
            "track" => parent.context("track without album")?,
            _ => return Ok(()),
        };
        let item: Candidate =
            sqlx::query_as("SELECT id, kind, title, year, tmdb_id, imdb_id, poster, backdrop FROM items WHERE id = ?")
                .bind(target)
                .fetch_one(&self.db.read)
                .await?;
        self.refresh_candidate(item).await.map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    async fn refresh_candidate(&self, item: Candidate) -> Result<(), TmdbError> {
        let id = item.id;
        match item.kind.as_str() {
            "movie" => self.refresh_movie(&item).await?,
            "show" => match self.providers.tv_source() {
                Some(source @ (TvSource::Tvmaze | TvSource::Tvdb)) => self.providers.refresh_show(source, id).await?,
                _ => self.refresh_show(&item).await?,
            },
            "artist" | "album" => self.providers.refresh_music_item(id).await?,
            _ => return Ok(()),
        }
        // Hand edits win over whatever was just fetched.
        crate::overrides::apply_tree(&self.db, id).await.map_err(|e| TmdbError::Other(e.into()))
    }

    /// TMDB results for a title, best first ("Fix match").
    pub async fn search_matches(
        &self,
        tv: bool,
        title: &str,
        year: Option<i64>,
    ) -> anyhow::Result<Vec<MatchCandidate>> {
        let (path, year_param) = if tv { ("/search/tv", "first_air_date_year") } else { ("/search/movie", "year") };
        let mut query = vec![("query", title.to_owned()), ("include_adult", "false".to_owned())];
        if let Some(y) = year {
            query.push((year_param, y.to_string()));
        }
        let results: SearchResults = self.get(path, &query).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let best = best_match(&results.results, title, year);
        let mut out: Vec<MatchCandidate> = results
            .results
            .into_iter()
            .take(12)
            .map(|r| MatchCandidate {
                tmdb_id: r.id,
                title: r.title.clone().or(r.original_title.clone()).unwrap_or_default(),
                original_title: r.original_title.filter(|o| Some(o) != r.title.as_ref()),
                year: r.release_date.as_deref().and_then(year_of),
                overview: r.overview.filter(|o| !o.is_empty()),
                poster_url: r.poster_path.map(|p| format!("{IMAGES}/w185{p}")),
            })
            .collect();
        if let Some(best) = best {
            out.sort_by_key(|c| c.tmdb_id != best);
        }
        Ok(out)
    }

    /// Match a movie or show to another TMDB entry and refresh it.
    pub async fn rematch(&self, item_id: i64, tmdb_id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE items SET tmdb_id = ?, imdb_id = NULL, metadata_at = NULL WHERE id = ? AND kind IN ('movie', 'show')")
            .bind(tmdb_id)
            .bind(item_id)
            .execute(&self.db.write)
            .await?;
        // A show refreshed from TVmaze or TheTVDB is found there by the ids
        // TMDB has for the chosen entry.
        if matches!(self.providers.tv_source(), Some(TvSource::Tvmaze | TvSource::Tvdb)) {
            let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&self.db.read)
                .await?;
            if kind.as_deref() == Some("show") {
                let ids: ExternalIds = self
                    .get(&format!("/tv/{tmdb_id}/external_ids"), &[])
                    .await
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                self.providers.rematched(item_id, ids.imdb_id, ids.tvdb_id).await?;
            }
        }
        self.refresh_item(item_id).await
    }

    /// Posters, backdrops or episode stills TMDB has for an item.
    pub async fn artwork_candidates(&self, item_id: i64, kind: &str) -> anyhow::Result<Vec<ArtworkCandidate>> {
        #[allow(clippy::type_complexity)]
        let row: Option<(String, Option<i64>, Option<i64>, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT i.kind, i.tmdb_id, i.idx, p.tmdb_id, gp.tmdb_id FROM items i
             LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id WHERE i.id = ?",
        )
        .bind(item_id)
        .fetch_optional(&self.db.read)
        .await?;
        let Some((item_kind, tmdb, idx, parent_tmdb, show_tmdb)) = row else { bail!("item not found") };
        let season_number = || {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT p.idx FROM items i JOIN items p ON p.id = i.parent_id WHERE i.id = ?",
            )
            .bind(item_id)
            .fetch_one(&self.db.read)
        };
        let path = match item_kind.as_str() {
            "movie" => format!("/movie/{}/images", tmdb.context("not matched to TMDB yet")?),
            "show" => format!("/tv/{}/images", tmdb.context("not matched to TMDB yet")?),
            "season" => format!(
                "/tv/{}/season/{}/images",
                parent_tmdb.context("the show is not matched yet")?,
                idx.unwrap_or(1)
            ),
            "episode" => {
                let season = season_number().await?.unwrap_or(1);
                format!(
                    "/tv/{}/season/{season}/episode/{}/images",
                    show_tmdb.context("the show is not matched yet")?,
                    idx.context("the episode has no number yet")?
                )
            }
            _ => bail!("{item_kind} items have no TMDB artwork"),
        };
        let lang = self.settings.get().metadata_language.split(['-', '_']).next().unwrap_or("en").to_owned();
        // Without a language filter TMDB only returns images in the request language.
        let images: Images = self
            .get(&path, &[("include_image_language", format!("{lang},en,null"))])
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let list = match kind {
            "poster" => images.posters,
            "backdrop" => images.backdrops,
            "thumb" => images.stills,
            _ => bail!("unknown artwork kind"),
        };
        let preview = if kind == "poster" { "w342" } else { "w780" };
        let mut out: Vec<ArtworkCandidate> = list
            .into_iter()
            .map(|i| ArtworkCandidate {
                url: format!("{IMAGES}/original{}", i.file_path),
                preview_url: format!("{IMAGES}/{preview}{}", i.file_path),
                width: i.width,
                height: i.height,
                language: i.iso_639_1,
                votes: i.vote_average,
            })
            .collect();
        // Images in the metadata language first, then the best rated.
        out.sort_by(|a, b| {
            let rank = |c: &ArtworkCandidate| match c.language.as_deref() {
                Some(l) if l == lang => 0,
                None => 1,
                _ => 2,
            };
            rank(a).cmp(&rank(b)).then(b.votes.total_cmp(&a.votes))
        });
        out.truncate(60);
        Ok(out)
    }

    /// Store a chosen picture ([`Self::artwork_candidates`]'s, or a library
    /// tool's) in the metadata folder; returns its reference. Only the image
    /// hosts of [`crate::store::allowed_image_url`] are fetched, so the
    /// server can't be pointed at other addresses.
    pub async fn download_artwork(&self, url: &str) -> anyhow::Result<String> {
        if !crate::store::allowed_image_url(url) {
            bail!(
                "pictures are taken from TMDB, TheTVDB, TVmaze, the Cover Art Archive, fanart.tv, ThePosterDB and GitHub's raw files only"
            );
        }
        self.store.remote(url).await
    }

    /// Whether TMDB accepts `key` (one not saved yet, say): one request.
    pub async fn check_tmdb(&self, key: &str) -> Result<(), ProviderError> {
        match self.get_with::<serde::de::IgnoredAny>(key, "/configuration", &[]).await {
            Ok(_) => Ok(()),
            Err(TmdbError::Unauthorized) => Err(ProviderError::Unauthorized("TMDB")),
            Err(TmdbError::NotFound) => Err(ProviderError::NotFound),
            Err(TmdbError::Other(err)) => Err(ProviderError::Other(err)),
        }
    }

    /// Whether TheTVDB (with this key and PIN), TVmaze or MusicBrainz answers.
    pub async fn check_provider(&self, check: crate::providers::Check<'_>) -> Result<(), ProviderError> {
        self.providers.check(check).await
    }

    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T, TmdbError> {
        let key = self.settings.get().tmdb_api_key.clone();
        self.get_with(&key, path, query).await
    }

    async fn get_with<T: DeserializeOwned>(
        &self,
        key: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, TmdbError> {
        let settings = self.settings.get();
        let key = key.trim();
        if key.is_empty() {
            return Err(TmdbError::Unauthorized);
        }
        let mut req = self
            .http
            .get(format!("{}{path}", self.api))
            .query(query)
            .query(&[("language", settings.metadata_language.as_str())]);
        req = if key.starts_with("eyJ") { req.bearer_auth(key) } else { req.query(&[("api_key", key)]) };
        // Errors leave the URL out: it carries the API key (logged otherwise).
        for attempt in 0..3 {
            let resp = req
                .try_clone()
                .context("request clone")?
                .send()
                .await
                .map_err(|e| TmdbError::Other(e.without_url().into()))?;
            match resp.status().as_u16() {
                200 => return resp.json::<T>().await.map_err(|e| TmdbError::Other(e.without_url().into())),
                401 => return Err(TmdbError::Unauthorized),
                404 => return Err(TmdbError::NotFound),
                429 => tokio::time::sleep(Duration::from_millis(1000 * (attempt + 1))).await,
                status => return Err(TmdbError::Other(anyhow::anyhow!("TMDB returned HTTP {status}"))),
            }
        }
        Err(TmdbError::Other(anyhow::anyhow!("TMDB rate limit")))
    }

    /// The country of `metadata_language` ("US" for "en-US"; "US" without one).
    pub(crate) fn region(&self) -> String {
        let lang = self.settings.get().metadata_language.clone();
        lang.split(['-', '_']).nth(1).unwrap_or("US").to_ascii_uppercase()
    }

    async fn find_by_imdb(&self, imdb: &str, tv: bool) -> Result<Option<i64>, TmdbError> {
        let found: FindResults = self.get(&format!("/find/{imdb}"), &[("external_source", "imdb_id".into())]).await?;
        Ok(if tv { found.tv_results.first() } else { found.movie_results.first() }.map(|r| r.id))
    }

    async fn search(&self, tv: bool, title: &str, year: Option<i64>) -> Result<Option<i64>, TmdbError> {
        let (path, year_param) = if tv { ("/search/tv", "first_air_date_year") } else { ("/search/movie", "year") };
        let mut query = vec![("query", title.to_owned()), ("include_adult", "false".to_owned())];
        if let Some(y) = year {
            query.push((year_param, y.to_string()));
        }
        let mut results: SearchResults = self.get(path, &query).await?;
        if results.results.is_empty() && year.is_some() {
            results = self.get(path, &query[..2]).await?;
        }
        Ok(best_match(&results.results, title, year))
    }

    async fn refresh_movie(&self, item: &Candidate) -> Result<(), TmdbError> {
        let tmdb_id = match item.tmdb_id {
            Some(id) => id,
            None => {
                let by_imdb = match &item.imdb_id {
                    Some(imdb) => self.find_by_imdb(imdb, false).await?,
                    None => None,
                };
                match by_imdb {
                    Some(id) => id,
                    None => self.search(false, &item.title, item.year).await?.ok_or(TmdbError::NotFound)?,
                }
            }
        };
        let details: MovieDetails =
            self.get(&format!("/movie/{tmdb_id}"), &[("append_to_response", "release_dates,credits".into())]).await?;
        let region = self.region();
        let certification = details.release_dates.as_ref().and_then(|rd| {
            rd.results
                .iter()
                .find(|c| c.iso_3166_1 == region)
                .or_else(|| rd.results.iter().find(|c| c.iso_3166_1 == "US"))
                .and_then(|c| c.release_dates.iter().map(|r| r.certification.trim()).find(|c| !c.is_empty()))
                .map(str::to_owned)
        });
        let poster = self.artwork(item.poster.as_deref(), details.poster_path.as_deref(), "w780").await;
        let backdrop = self.artwork(item.backdrop.as_deref(), details.backdrop_path.as_deref(), "w1280").await;
        let title = details.title.clone().unwrap_or_else(|| item.title.clone());
        let genres: Vec<String> = details.genres.iter().map(|g| g.name.clone()).collect();
        let age = certification.as_deref().and_then(crate::ratings::rating_age);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE items SET title = ?, sort_title = ?, original_title = ?, year = COALESCE(?, year), summary = ?, tagline = ?,
                rating = ?, content_rating = CASE WHEN {RATING_LOCKED} THEN content_rating ELSE ? END,
                rating_age = CASE WHEN {RATING_LOCKED} THEN rating_age ELSE ? END,
                genres = ?, studio = ?, release_date = ?, duration_ms = COALESCE(duration_ms, ?),
                tmdb_id = ?, imdb_id = COALESCE(?, imdb_id), poster = COALESCE(?, poster), backdrop = COALESCE(?, backdrop),
                metadata_at = ?, updated_at = ?
             WHERE id = ?"
        )))
        .bind(&title)
        .bind(sort_title(&title))
        .bind(details.original_title.filter(|o| *o != title))
        .bind(details.release_date.as_deref().and_then(year_of))
        .bind(details.overview.filter(|s| !s.is_empty()))
        .bind(details.tagline.filter(|s| !s.is_empty()))
        .bind(details.vote_average.filter(|v| *v > 0.0))
        .bind(certification)
        .bind(age)
        .bind(serde_json::to_string(&genres).ok())
        .bind(details.production_companies.first().map(|c| c.name.clone()))
        .bind(details.release_date.filter(|d| !d.is_empty()))
        .bind(details.runtime.filter(|r| *r > 0).map(|r| r * 60_000))
        .bind(tmdb_id)
        .bind(details.imdb_id)
        .bind(poster)
        .bind(backdrop)
        .bind(now())
        .bind(now())
        .bind(item.id)
        .execute(&self.db.write)
        .await
        .map_err(|e| TmdbError::Other(e.into()))?;
        if let Some(credits) = &details.credits {
            people::replace(&self.db, item.id, &people::movie_credits(credits))
                .await
                .map_err(|e| TmdbError::Other(e.into()))?;
        }
        let collection = details.belongs_to_collection.as_ref();
        if let Err(err) = crate::collections::link_tmdb(&self.db, &self.store, item.id, collection).await {
            tracing::warn!("collection of item {} not updated: {err:#}", item.id);
        }
        Ok(())
    }

    async fn refresh_show(&self, item: &Candidate) -> Result<(), TmdbError> {
        let tmdb_id = match item.tmdb_id {
            Some(id) => id,
            None => self.search(true, &item.title, item.year).await?.ok_or(TmdbError::NotFound)?,
        };
        let details: TvDetails = self
            .get(
                &format!("/tv/{tmdb_id}"),
                &[("append_to_response", "external_ids,content_ratings,aggregate_credits".into())],
            )
            .await?;
        let region = self.region();
        let rating = details.content_ratings.as_ref().and_then(|r| {
            r.results
                .iter()
                .find(|c| c.iso_3166_1 == region)
                .or_else(|| r.results.iter().find(|c| c.iso_3166_1 == "US"))
                .map(|c| c.rating.clone())
                .filter(|r| !r.is_empty())
        });
        let poster = self.artwork(item.poster.as_deref(), details.poster_path.as_deref(), "w780").await;
        let backdrop = self.artwork(item.backdrop.as_deref(), details.backdrop_path.as_deref(), "w1280").await;
        let title = details.name.clone().unwrap_or_else(|| item.title.clone());
        let genres: Vec<String> = details.genres.iter().map(|g| g.name.clone()).collect();
        let age = rating.as_deref().and_then(crate::ratings::rating_age);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE items SET title = ?, sort_title = ?, original_title = ?, year = COALESCE(?, year), summary = ?, tagline = ?,
                rating = ?, content_rating = CASE WHEN {RATING_LOCKED} THEN content_rating ELSE ? END,
                rating_age = CASE WHEN {RATING_LOCKED} THEN rating_age ELSE ? END,
                genres = ?, studio = ?, release_date = ?, tmdb_id = ?, imdb_id = COALESCE(?, imdb_id),
                poster = COALESCE(?, poster), backdrop = COALESCE(?, backdrop), metadata_at = ?, updated_at = ?
             WHERE id = ?"
        )))
        .bind(&title)
        .bind(sort_title(&title))
        .bind(details.original_name.filter(|o| *o != title))
        .bind(details.first_air_date.as_deref().and_then(year_of))
        .bind(details.overview.filter(|s| !s.is_empty()))
        .bind(details.tagline.filter(|s| !s.is_empty()))
        .bind(details.vote_average.filter(|v| *v > 0.0))
        .bind(rating)
        .bind(age)
        .bind(serde_json::to_string(&genres).ok())
        .bind(details.networks.first().map(|n| n.name.clone()))
        .bind(details.first_air_date.filter(|d| !d.is_empty()))
        .bind(tmdb_id)
        .bind(details.external_ids.and_then(|e| e.imdb_id).filter(|i| !i.is_empty()))
        .bind(poster)
        .bind(backdrop)
        .bind(now())
        .bind(now())
        .bind(item.id)
        .execute(&self.db.write)
        .await
        .map_err(|e| TmdbError::Other(e.into()))?;
        if let Some(credits) = &details.aggregate_credits {
            people::replace(&self.db, item.id, &people::show_credits(credits, &details.created_by))
                .await
                .map_err(|e| TmdbError::Other(e.into()))?;
        }

        // Seasons and their episodes.
        let seasons: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
            "SELECT id, idx, poster FROM items WHERE parent_id = ? AND kind = 'season' AND metadata_locked = 0",
        )
        .bind(item.id)
        .fetch_all(&self.db.read)
        .await
        .map_err(|e| TmdbError::Other(e.into()))?;
        for (season_id, number, season_poster) in seasons {
            let season: SeasonDetails = match self.get(&format!("/tv/{tmdb_id}/season/{number}"), &[]).await {
                Ok(s) => s,
                Err(TmdbError::NotFound) => continue,
                Err(e) => return Err(e),
            };
            let poster = self.artwork(season_poster.as_deref(), season.poster_path.as_deref(), "w780").await;
            let name = season
                .name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| if number == 0 { "Specials".into() } else { format!("Season {number}") });
            sqlx::query(
                "UPDATE items SET title = ?, sort_title = ?, summary = ?, release_date = ?, year = ?, poster = COALESCE(?, poster),
                    metadata_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&name)
            .bind(format!("{number:04}"))
            .bind(season.overview.filter(|s| !s.is_empty()))
            .bind(season.air_date.clone().filter(|d| !d.is_empty()))
            .bind(season.air_date.as_deref().and_then(year_of))
            .bind(poster)
            .bind(now())
            .bind(now())
            .bind(season_id)
            .execute(&self.db.write)
            .await
            .map_err(|e| TmdbError::Other(e.into()))?;

            #[allow(clippy::type_complexity)]
            let episodes: Vec<(i64, Option<i64>, String, Option<i64>, Option<String>)> = sqlx::query_as(
                "SELECT id, idx, title, duration_ms, thumb FROM items
                 WHERE parent_id = ? AND kind = 'episode' AND metadata_locked = 0",
            )
            .bind(season_id)
            .fetch_all(&self.db.read)
            .await
            .map_err(|e| TmdbError::Other(e.into()))?;
            // Numbers the season's files have, which no other may take by its title.
            let taken: HashSet<i64> = episodes.iter().filter_map(|e| e.1).collect();
            for (episode_id, number, file_title, duration_ms, thumb) in episodes {
                let item = SeasonItem { number, title: &file_title, duration_ms };
                let Some(ep) = season_episode(&season.episodes, &item, &taken) else { continue };
                let ep_number = ep.episode_number;
                let still = self.artwork(thumb.as_deref(), ep.still_path.as_deref(), "w780").await;
                let title = ep.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Episode {ep_number}"));
                sqlx::query(
                    "UPDATE items SET idx = ?, title = ?, sort_title = ?, summary = ?, release_date = ?, year = ?, rating = ?,
                        thumb = COALESCE(?, thumb), metadata_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(ep_number)
                .bind(&title)
                .bind(sort_title(&title))
                .bind(ep.overview.clone().filter(|s| !s.is_empty()))
                .bind(ep.air_date.clone().filter(|d| !d.is_empty()))
                .bind(ep.air_date.as_deref().and_then(year_of))
                .bind(ep.vote_average.filter(|v| *v > 0.0))
                .bind(still)
                .bind(now())
                .bind(now())
                .bind(episode_id)
                .execute(&self.db.write)
                .await
                .map_err(|e| TmdbError::Other(e.into()))?;
                people::replace(&self.db, episode_id, &people::episode_credits(&ep.guest_stars, &ep.crew))
                    .await
                    .map_err(|e| TmdbError::Other(e.into()))?;
            }
        }
        Ok(())
    }

    /// The TMDB picture `remote` as a stored reference, unless the item has
    /// local (sidecar) artwork, which always wins. Pictures already stored
    /// from the same URL are not downloaded again.
    async fn artwork(&self, current: Option<&str>, remote: Option<&str>, size: &str) -> Option<String> {
        let remote = remote?;
        if let Some(cur) = current
            && !store::is_blob(cur)
            && Path::new(cur).exists()
        {
            return None;
        }
        let url = format!("{IMAGES}/{size}{remote}");
        match self.store.remote(&url).await {
            Ok(blob) => Some(blob),
            Err(err) => {
                tracing::debug!("artwork download failed for {url}: {err:#}");
                None
            }
        }
    }
}

fn year_of(date: &str) -> Option<i64> {
    date.get(..4).and_then(|y| y.parse().ok())
}

/// Pick the result whose title and year match best (popularity breaks ties).
fn best_match(results: &[SearchResult], title: &str, year: Option<i64>) -> Option<i64> {
    let wanted = normalize_key(title);
    if wanted.is_empty() {
        return None;
    }
    results
        .iter()
        .take(10)
        .enumerate()
        .filter_map(|(rank, r)| {
            let titles = [r.title.as_deref(), r.original_title.as_deref()];
            let title_score = titles
                .iter()
                .flatten()
                .map(|t| {
                    let t = normalize_key(t);
                    if t == wanted {
                        3.0
                    } else if t.contains(&wanted) || wanted.contains(&t) {
                        1.5
                    } else {
                        0.0
                    }
                })
                .fold(0.0, f64::max);
            let year_score = match (year, r.release_date.as_deref().and_then(year_of)) {
                (Some(want), Some(got)) if want == got => 2.0,
                (Some(want), Some(got)) if (want - got).abs() == 1 => 1.0,
                (Some(_), Some(_)) => -1.0,
                _ => 0.0,
            };
            // A result needs a title that matches, or to be TMDB's first
            // answer for the right year (its title may be in another language).
            let plausible = title_score > 0.0 || (rank == 0 && year_score >= 2.0);
            plausible.then(|| (r.id, title_score + year_score + r.popularity.min(100.0) / 1000.0))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episodes_are_found_by_number_or_else_by_title() {
        let season: SeasonDetails = serde_json::from_value(serde_json::json!({
            "name": "Season 1", "overview": null, "poster_path": null, "air_date": null,
            "episodes": [
                {"episode_number": 1, "name": "Midnight on the Firing Line"},
                {"episode_number": 4, "name": "Infection"},
                {"episode_number": 6, "name": "Mind War"},
                {"episode_number": 7, "name": "The War Prayer"},
                {"episode_number": 8, "name": "The War Prayer"},
            ],
        }))
        .unwrap();
        let none = HashSet::new();
        let found = |number, title| {
            let item = SeasonItem { number, title, duration_ms: None };
            season_episode(&season.episodes, &item, &none).map(|e| e.episode_number)
        };
        assert_eq!(found(Some(4), "Whatever the file said"), Some(4));
        assert_eq!(found(Some(9), "Infection"), None, "a number wins over a title");
        // Without a number: the title, any case and punctuation.
        assert_eq!(found(None, "mind-war"), Some(6));
        assert_eq!(found(None, "INFECTION"), Some(4));
        assert_eq!(found(None, "Infection Part 2"), None);
        // Two episodes of that title: none is taken.
        assert_eq!(found(None, "The War Prayer"), None);
        assert_eq!(found(None, ""), None);
    }

    /// Firefly's folder holds the film Serenity too, and TMDB calls the
    /// pilot "Serenity": the film is no episode.
    #[test]
    fn a_title_never_takes_a_number_a_file_has_nor_a_length_far_off() {
        let season: SeasonDetails = serde_json::from_value(serde_json::json!({
            "name": "Season 1", "overview": null, "poster_path": null, "air_date": null,
            "episodes": [
                {"episode_number": 1, "name": "Serenity", "runtime": 86},
                {"episode_number": 2, "name": "The Train Job", "runtime": 43},
            ],
        }))
        .unwrap();
        let film = |taken: &HashSet<i64>, duration_ms| {
            let item = SeasonItem { number: None, title: "Serenity", duration_ms };
            season_episode(&season.episodes, &item, taken).map(|e| e.episode_number)
        };
        // The pilot's file is there, numbered.
        assert_eq!(film(&HashSet::from([1, 2]), None), None);
        // It isn't, but the film runs two hours.
        assert_eq!(film(&HashSet::from([2]), Some(119 * 60_000)), None);
        // An unnumbered pilot of about its length is the pilot.
        assert_eq!(film(&HashSet::from([2]), Some(85 * 60_000)), Some(1));
        // Without a length, the title decides.
        assert_eq!(film(&HashSet::from([2]), None), Some(1));
    }

    fn result(id: i64, title: &str, date: &str, popularity: f64) -> SearchResult {
        SearchResult {
            id,
            title: Some(title.into()),
            original_title: None,
            release_date: Some(date.into()),
            popularity,
            overview: None,
            poster_path: None,
        }
    }

    #[test]
    fn picks_title_and_year_match() {
        let results = vec![
            result(1, "The Thing", "2011-10-12", 50.0),
            result(2, "The Thing", "1982-06-25", 40.0),
            result(3, "The Thing from Another World", "1951-04-06", 10.0),
        ];
        assert_eq!(best_match(&results, "The Thing", Some(1982)), Some(2));
        assert_eq!(best_match(&results, "The Thing", None), Some(1));
        assert_eq!(best_match(&[], "x", None), None);
    }

    #[test]
    fn refuses_results_that_do_not_match() {
        let results = vec![result(7, "Something Else Entirely", "2019-03-01", 90.0)];
        // No title in common and no year to go by: no match (not the most popular).
        assert_eq!(best_match(&results, "Family Trip", None), None);
        assert_eq!(best_match(&results, "Family Trip", Some(2015)), None);
        // TMDB's first answer for the exact year is trusted (localized titles).
        assert_eq!(best_match(&results, "Family Trip", Some(2019)), Some(7));
        // A title made only of symbols matches nothing.
        assert_eq!(best_match(&[result(8, "Anything", "2001-01-01", 1.0)], "!!!", None), None);
    }
}
