//! Discover: movies and shows found on TMDB, for people to ask for
//! ([`crate::requests`]): lists, search and a title's details, as TMDB has
//! them, with their pictures served by pley ([`pictures`]). Each answer is
//! reused for a while (per metadata language), so browsing doesn't ask TMDB
//! again for every page shown.

pub mod pictures;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::metadata::{MetadataAgent, TmdbError};
use crate::ratings::{self, Kind};
use crate::settings::SettingsStore;
use crate::util::{date_secs, now, utc};
use pictures::Picture;

/// How long lists and searches are reused, and titles' details.
const PAGE_TTL: Duration = Duration::from_secs(3600);
const DETAILS_TTL: Duration = Duration::from_secs(6 * 3600);
/// How long TMDB's list of certifications is reused.
const CERTIFICATIONS_TTL: Duration = Duration::from_secs(24 * 3600);
/// Coming to cinemas: movies released within this many days.
const UPCOMING_DAYS: i64 = 28;
/// Answers kept of each sort at most: past it, the expired ones go (all of
/// them, should none be).
const CACHE_MAX: usize = 1000;
/// The last page TMDB serves of a list.
pub const MAX_PAGE: u32 = 500;
/// TMDB's "anime" keyword, and its Animation genre.
const ANIME_KEYWORD: i64 = 210_024;
const ANIMATION: i64 = 16;
/// Cast members a title's details name.
const CAST: usize = 12;
/// Votes a title needs before its rating means something.
const MIN_VOTES: i64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleKind {
    Movie,
    Show,
}

impl TitleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TitleKind::Movie => "movie",
            TitleKind::Show => "show",
        }
    }

    pub fn parse(text: &str) -> Option<TitleKind> {
        match text {
            "movie" => Some(TitleKind::Movie),
            "show" => Some(TitleKind::Show),
            _ => None,
        }
    }

    /// TMDB's name for it in paths.
    fn tmdb(self) -> &'static str {
        match self {
            TitleKind::Movie => "movie",
            TitleKind::Show => "tv",
        }
    }
}

/// The lists Discover shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum List {
    /// This week's movies and shows.
    Trending,
    /// The most popular movies.
    Movies,
    /// The most popular shows.
    Shows,
    /// Movies coming to cinemas in the server's region.
    Upcoming,
    /// The most popular anime shows.
    Anime,
}

/// A title of a list or search.
#[derive(Debug, Clone)]
pub struct Found {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub overview: String,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub rating: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub page: u32,
    pub total_pages: u32,
    pub results: Vec<Found>,
}

/// A title as TMDB has it now.
#[derive(Debug, Clone)]
pub struct Details {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub overview: String,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub release_date: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    /// The country of `certification` (ISO 3166-1 alpha-2).
    pub certification_country: Option<String>,
    pub network: Option<String>,
    pub show_status: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub rating: Option<f64>,
    pub anime: bool,
    /// TMDB's adult titles: never in lists or search, and only managers
    /// see them otherwise.
    pub adult: bool,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub cast: Vec<CastMember>,
    /// Shows: TMDB's seasons in order (specials, 0, first); movies: none.
    pub seasons: Vec<Season>,
}

impl Details {
    /// The age `certification` stands for in its country, `None` when
    /// unrated or unknown.
    pub fn age(&self) -> Option<i64> {
        let kind = match self.kind {
            TitleKind::Movie => Kind::Film,
            TitleKind::Show => Kind::Tv,
        };
        ratings::age(self.certification_country.as_deref(), kind, self.certification.as_deref()?)
    }
}

#[derive(Debug, Clone)]
pub struct Season {
    pub number: i64,
    pub name: String,
    pub episodes: i64,
    /// Episodes of it aired by now.
    pub aired: i64,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CastMember {
    pub name: String,
    pub character: Option<String>,
    pub photo_url: Option<String>,
}

/// What a show is known by elsewhere.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExternalIds {
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
}

/// Answers kept for a while.
struct Cache<K, V> {
    entries: DashMap<K, (Instant, Arc<V>)>,
    ttl: Duration,
}

impl<K: Eq + Hash + Clone, V> Cache<K, V> {
    fn new(ttl: Duration) -> Self {
        Self { entries: DashMap::new(), ttl }
    }

    fn get(&self, key: &K) -> Option<Arc<V>> {
        let entry = self.entries.get(key)?;
        (entry.0.elapsed() < self.ttl).then(|| entry.1.clone())
    }

    fn put(&self, key: K, value: V) -> Arc<V> {
        if self.entries.len() >= CACHE_MAX {
            self.entries.retain(|_, (at, _)| at.elapsed() < self.ttl);
            if self.entries.len() >= CACHE_MAX {
                self.entries.clear();
            }
        }
        let value = Arc::new(value);
        self.entries.insert(key, (Instant::now(), value.clone()));
        value
    }
}

/// What a list or search asks TMDB: its path and parameters.
type Query = (String, Vec<(&'static str, String)>);

pub struct Discover {
    metadata: Arc<MetadataAgent>,
    settings: Arc<SettingsStore>,
    /// By language, path and parameters.
    pages: Cache<(String, String), Page>,
    details: Cache<(String, TitleKind, i64), Details>,
    external_ids: Cache<i64, ExternalIds>,
    /// TMDB's id of a title known by TheTVDB's id or IMDb's (`None`: it has
    /// none), by kind and `tvdb:<id>` or `imdb:<id>`.
    found: Cache<(TitleKind, String), Option<i64>>,
    /// Movie certifications by country, in TMDB's order.
    certifications: Cache<(), HashMap<String, Vec<String>>>,
}

impl Discover {
    pub fn new(metadata: Arc<MetadataAgent>, settings: Arc<SettingsStore>) -> Self {
        Self {
            metadata,
            settings,
            pages: Cache::new(PAGE_TTL),
            details: Cache::new(DETAILS_TTL),
            external_ids: Cache::new(DETAILS_TTL),
            found: Cache::new(DETAILS_TTL),
            certifications: Cache::new(CERTIFICATIONS_TTL),
        }
    }

    fn language(&self) -> String {
        self.settings.get().metadata_language.clone()
    }

    /// A page (from 1) of a list. `ceiling`: the highest movie
    /// certification of the server's country an account may ask for (see
    /// [`Discover::ceiling`]): popular and upcoming movies then come from
    /// TMDB's discover search, rated at most that.
    pub(crate) async fn list(&self, list: List, page: u32, ceiling: Option<&str>) -> Result<Arc<Page>, TmdbError> {
        let region = self.metadata.region();
        let rated = |ceiling: &str, mut query: Vec<(&'static str, String)>| {
            query.push(("certification_country", region.clone()));
            query.push(("certification.lte", ceiling.to_owned()));
            query.push(("include_adult", "false".into()));
            query.push(("sort_by", "popularity.desc".into()));
            query
        };
        let (path, mut query): Query = match (list, ceiling) {
            (List::Trending, _) => ("/trending/all/week".into(), vec![]),
            (List::Movies, None) => ("/movie/popular".into(), vec![]),
            (List::Movies, Some(ceiling)) => ("/discover/movie".into(), rated(ceiling, vec![])),
            (List::Shows, _) => ("/tv/popular".into(), vec![]),
            (List::Upcoming, None) => ("/movie/upcoming".into(), vec![("region", region.clone())]),
            (List::Upcoming, Some(ceiling)) => {
                // What /movie/upcoming lists: released in cinemas soon.
                let from = now();
                let upcoming = vec![
                    ("region", region.clone()),
                    ("with_release_type", "2|3".into()),
                    ("release_date.gte", day(from)),
                    ("release_date.lte", day(from + UPCOMING_DAYS * 86_400)),
                ];
                ("/discover/movie".into(), rated(ceiling, upcoming))
            }
            (List::Anime, _) => (
                "/discover/tv".into(),
                vec![("with_keywords", ANIME_KEYWORD.to_string()), ("sort_by", "popularity.desc".into())],
            ),
        };
        query.push(("page", page.to_string()));
        let kind = match list {
            List::Trending => None,
            List::Movies | List::Upcoming => Some(TitleKind::Movie),
            List::Shows | List::Anime => Some(TitleKind::Show),
        };
        self.page(path, query, kind).await
    }

    /// A page (from 1) of the movies and shows matching `text`.
    pub(crate) async fn search(&self, text: &str, page: u32) -> Result<Arc<Page>, TmdbError> {
        let query = vec![("query", text.to_owned()), ("include_adult", "false".to_owned()), ("page", page.to_string())];
        self.page("/search/multi".into(), query, None).await
    }

    /// `kind`: what the path lists; `None`: each result says (`media_type`).
    async fn page(
        &self,
        path: String,
        query: Vec<(&'static str, String)>,
        kind: Option<TitleKind>,
    ) -> Result<Arc<Page>, TmdbError> {
        let encoded = query.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
        let key = (self.language(), format!("{path}?{encoded}"));
        if let Some(page) = self.pages.get(&key) {
            return Ok(page);
        }
        let raw: RawPage = self.metadata.get(&path, &query).await?;
        Ok(self.pages.put(key, raw.into_page(kind)))
    }

    /// A movie's or show's details.
    pub(crate) async fn details(&self, kind: TitleKind, tmdb_id: i64) -> Result<Arc<Details>, TmdbError> {
        let key = (self.language(), kind, tmdb_id);
        if let Some(details) = self.details.get(&key) {
            return Ok(details);
        }
        let region = self.metadata.region();
        let path = format!("/{}/{tmdb_id}", kind.tmdb());
        let details = match kind {
            TitleKind::Movie => {
                let extra = "credits,release_dates,keywords".to_owned();
                let raw: RawMovie = self.get(&path, &[("append_to_response", extra)]).await?;
                raw.into_details(tmdb_id, &region)
            }
            TitleKind::Show => {
                let extra = "aggregate_credits,content_ratings,keywords,external_ids".to_owned();
                let raw: RawShow = self.get(&path, &[("append_to_response", extra)]).await?;
                raw.into_details(tmdb_id, &region, now())
            }
        };
        if kind == TitleKind::Show {
            self.external_ids.put(tmdb_id, ExternalIds { imdb_id: details.imdb_id.clone(), tvdb_id: details.tvdb_id });
        }
        Ok(self.details.put(key, details))
    }

    /// A show's TheTVDB and IMDb ids.
    pub(crate) async fn show_ids(&self, tmdb_id: i64) -> Result<Arc<ExternalIds>, TmdbError> {
        if let Some(ids) = self.external_ids.get(&tmdb_id) {
            return Ok(ids);
        }
        let ids: ExternalIds = self.get(&format!("/tv/{tmdb_id}/external_ids"), &[]).await?;
        let ids = ExternalIds {
            imdb_id: ids.imdb_id.filter(|i| i.starts_with("tt")),
            tvdb_id: ids.tvdb_id.filter(|id| *id > 0),
        };
        Ok(self.external_ids.put(tmdb_id, ids))
    }

    /// TMDB's id of a title known by TheTVDB's id (shows) or IMDb's, the
    /// first it finds by; `None` when it knows neither.
    pub(crate) async fn find(
        &self,
        kind: TitleKind,
        tvdb_id: Option<i64>,
        imdb_id: Option<&str>,
    ) -> Result<Option<i64>, TmdbError> {
        let tvdb = tvdb_id.filter(|_| kind == TitleKind::Show).map(|id| ("tvdb_id", id.to_string()));
        let imdb = imdb_id.filter(|id| id.starts_with("tt")).map(|id| ("imdb_id", id.to_owned()));
        for (source, id) in [tvdb, imdb].into_iter().flatten() {
            let key = (kind, format!("{source}:{id}"));
            let found = match self.found.get(&key) {
                Some(found) => *found,
                None => {
                    let answer: FindResults =
                        self.get(&format!("/find/{id}"), &[("external_source", source.into())]).await?;
                    let results = if kind == TitleKind::Movie { answer.movie_results } else { answer.tv_results };
                    *self.found.put(key, results.first().map(|r| r.id))
                }
            };
            if found.is_some() {
                return Ok(found);
            }
        }
        Ok(None)
    }

    /// The highest movie certification of the server's country that an age
    /// limit of `max_age` allows (TMDB's own order), for lists TMDB filters
    /// itself; `None` when the region has none (or TMDB doesn't say), and
    /// every title is checked as it comes.
    pub(crate) async fn ceiling(&self, max_age: i64) -> Option<String> {
        let all = match self.certifications.get(&()) {
            Some(all) => all,
            None => {
                let answer: CertificationList = match self.get("/certification/movie/list", &[]).await {
                    Ok(answer) => answer,
                    Err(err) => {
                        tracing::debug!("TMDB: its certifications: {err}");
                        return None;
                    }
                };
                let by_country = answer
                    .certifications
                    .into_iter()
                    .map(|(country, mut list)| {
                        list.sort_by_key(|c| c.order);
                        (country, list.into_iter().map(|c| c.certification).collect())
                    })
                    .collect();
                self.certifications.put((), by_country)
            }
        };
        let region = self.metadata.region();
        all.get(&region)?
            .iter()
            .rev()
            .find(|c| ratings::age(Some(&region), Kind::Film, c).is_some_and(|age| age <= max_age))
            .cloned()
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T, TmdbError> {
        self.metadata.get(path, query).await
    }
}

/// `2026-11-03`, in UTC.
fn day(secs: i64) -> String {
    let [y, m, d, ..] = utc(secs);
    format!("{y}-{m:02}-{d:02}")
}

// ------------------------------------------------------------------ TMDB's answers

#[derive(Deserialize)]
struct FindResults {
    #[serde(default)]
    movie_results: Vec<FoundId>,
    #[serde(default)]
    tv_results: Vec<FoundId>,
}

#[derive(Deserialize)]
struct FoundId {
    id: i64,
}

#[derive(Deserialize)]
struct CertificationList {
    #[serde(default)]
    certifications: HashMap<String, Vec<RawCertification>>,
}

#[derive(Deserialize)]
struct RawCertification {
    certification: String,
    #[serde(default)]
    order: i64,
}

#[derive(Deserialize)]
struct RawPage {
    #[serde(default)]
    page: u32,
    #[serde(default)]
    total_pages: u32,
    #[serde(default)]
    results: Vec<RawTitle>,
}

#[derive(Deserialize)]
struct RawTitle {
    id: i64,
    media_type: Option<String>,
    title: Option<String>,
    name: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    adult: bool,
}

impl RawPage {
    fn into_page(self, kind: Option<TitleKind>) -> Page {
        let results = self
            .results
            .into_iter()
            .filter(|t| !t.adult)
            .filter_map(|t| {
                let kind = kind.or(match t.media_type.as_deref() {
                    Some("movie") => Some(TitleKind::Movie),
                    Some("tv") => Some(TitleKind::Show),
                    _ => None,
                })?;
                let (title, date) = match kind {
                    TitleKind::Movie => (t.title, t.release_date),
                    TitleKind::Show => (t.name, t.first_air_date),
                };
                Some(Found {
                    kind,
                    tmdb_id: t.id,
                    title: title.filter(|t| !t.trim().is_empty())?,
                    year: date.as_deref().and_then(year_of),
                    overview: t.overview.unwrap_or_default(),
                    poster_path: t.poster_path,
                    backdrop_path: t.backdrop_path,
                    rating: rating(t.vote_average, t.vote_count),
                })
            })
            .collect();
        Page { page: self.page.max(1), total_pages: self.total_pages.min(MAX_PAGE), results }
    }
}

#[derive(Deserialize)]
struct Genre {
    id: i64,
    name: String,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Keyword {
    id: i64,
}

#[derive(Deserialize)]
struct MovieKeywords {
    #[serde(default)]
    keywords: Vec<Keyword>,
}

#[derive(Deserialize)]
struct ShowKeywords {
    #[serde(default)]
    results: Vec<Keyword>,
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
struct ContentRatings {
    #[serde(default)]
    results: Vec<ContentRating>,
}

#[derive(Deserialize)]
struct ContentRating {
    iso_3166_1: String,
    #[serde(default)]
    rating: String,
}

#[derive(Deserialize)]
struct Credits {
    #[serde(default)]
    cast: Vec<RawCast>,
}

#[derive(Deserialize)]
struct RawCast {
    name: String,
    character: Option<String>,
    #[serde(default)]
    roles: Vec<Role>,
    profile_path: Option<String>,
}

#[derive(Deserialize)]
struct Role {
    character: Option<String>,
}

#[derive(Deserialize)]
struct RawMovie {
    title: Option<String>,
    original_title: Option<String>,
    original_language: Option<String>,
    #[serde(default)]
    origin_country: Vec<String>,
    overview: Option<String>,
    tagline: Option<String>,
    release_date: Option<String>,
    runtime: Option<i64>,
    #[serde(default)]
    genres: Vec<Genre>,
    #[serde(default)]
    production_companies: Vec<Named>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    adult: bool,
    imdb_id: Option<String>,
    keywords: Option<MovieKeywords>,
    release_dates: Option<ReleaseDates>,
    credits: Option<Credits>,
}

impl RawMovie {
    fn into_details(self, tmdb_id: i64, region: &str) -> Details {
        let title = self.title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| format!("TMDB {tmdb_id}"));
        let keywords = self.keywords.map(|k| k.keywords).unwrap_or_default();
        let (certification, certification_country) = self
            .release_dates
            .and_then(|r| {
                let of = |country: &str| {
                    r.results
                        .iter()
                        .find(|c| c.iso_3166_1 == country)
                        .and_then(|c| c.release_dates.iter().find(|d| !d.certification.is_empty()))
                        .map(|d| (d.certification.clone(), country.to_owned()))
                };
                of(region).or_else(|| of("US"))
            })
            .unzip();
        Details {
            kind: TitleKind::Movie,
            tmdb_id,
            original_title: self.original_title.filter(|o| *o != title && !o.is_empty()),
            year: self.release_date.as_deref().and_then(year_of),
            overview: self.overview.unwrap_or_default(),
            tagline: self.tagline.filter(|t| !t.is_empty()),
            anime: anime(&keywords, &self.genres, self.original_language.as_deref(), &self.origin_country),
            genres: self.genres.into_iter().map(|g| g.name).collect(),
            release_date: self.release_date.filter(|d| !d.is_empty()),
            runtime_minutes: self.runtime.filter(|r| *r > 0),
            certification,
            certification_country,
            network: self.production_companies.into_iter().next().map(|c| c.name),
            show_status: None,
            poster_path: self.poster_path,
            backdrop_path: self.backdrop_path,
            rating: rating(self.vote_average, self.vote_count),
            adult: self.adult,
            imdb_id: self.imdb_id.filter(|i| i.starts_with("tt")),
            tvdb_id: None,
            cast: cast(self.credits),
            seasons: Vec::new(),
            title,
        }
    }
}

#[derive(Deserialize)]
struct RawShow {
    name: Option<String>,
    original_name: Option<String>,
    original_language: Option<String>,
    #[serde(default)]
    origin_country: Vec<String>,
    overview: Option<String>,
    tagline: Option<String>,
    first_air_date: Option<String>,
    #[serde(default)]
    genres: Vec<Genre>,
    #[serde(default)]
    networks: Vec<Named>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    #[serde(default)]
    adult: bool,
    status: Option<String>,
    #[serde(default)]
    seasons: Vec<RawSeason>,
    last_episode_to_air: Option<EpisodeRef>,
    keywords: Option<ShowKeywords>,
    content_ratings: Option<ContentRatings>,
    external_ids: Option<ExternalIds>,
    aggregate_credits: Option<Credits>,
}

#[derive(Deserialize)]
struct RawSeason {
    season_number: i64,
    name: Option<String>,
    episode_count: Option<i64>,
    air_date: Option<String>,
    poster_path: Option<String>,
}

#[derive(Deserialize)]
struct EpisodeRef {
    season_number: i64,
    episode_number: i64,
}

impl RawShow {
    fn into_details(self, tmdb_id: i64, region: &str, now: i64) -> Details {
        let title = self.name.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| format!("TMDB {tmdb_id}"));
        let keywords = self.keywords.map(|k| k.results).unwrap_or_default();
        let (certification, certification_country) = self
            .content_ratings
            .and_then(|r| {
                let of = |country: &str| {
                    r.results
                        .iter()
                        .find(|c| c.iso_3166_1 == country && !c.rating.is_empty())
                        .map(|c| (c.rating.clone(), country.to_owned()))
                };
                of(region).or_else(|| of("US"))
            })
            .unzip();
        let ids = self.external_ids.unwrap_or_default();
        let last = self.last_episode_to_air;
        let mut seasons: Vec<Season> = self
            .seasons
            .into_iter()
            .map(|s| {
                let episodes = s.episode_count.unwrap_or(0).max(0);
                Season {
                    aired: aired(s.season_number, episodes, s.air_date.as_deref(), last.as_ref(), now),
                    number: s.season_number,
                    name: s.name.filter(|n| !n.is_empty()).unwrap_or_else(|| match s.season_number {
                        0 => "Specials".to_owned(),
                        n => format!("Season {n}"),
                    }),
                    episodes,
                    air_date: s.air_date.filter(|d| !d.is_empty()),
                    poster_path: s.poster_path,
                }
            })
            .collect();
        seasons.sort_by_key(|s| s.number);
        seasons.dedup_by_key(|s| s.number);
        Details {
            kind: TitleKind::Show,
            tmdb_id,
            original_title: self.original_name.filter(|o| *o != title && !o.is_empty()),
            year: self.first_air_date.as_deref().and_then(year_of),
            overview: self.overview.unwrap_or_default(),
            tagline: self.tagline.filter(|t| !t.is_empty()),
            anime: anime(&keywords, &self.genres, self.original_language.as_deref(), &self.origin_country),
            genres: self.genres.into_iter().map(|g| g.name).collect(),
            release_date: self.first_air_date.filter(|d| !d.is_empty()),
            runtime_minutes: None,
            certification,
            certification_country,
            network: self.networks.into_iter().next().map(|n| n.name),
            show_status: self.status.filter(|s| !s.is_empty()),
            poster_path: self.poster_path,
            backdrop_path: self.backdrop_path,
            rating: rating(self.vote_average, self.vote_count),
            adult: self.adult,
            imdb_id: ids.imdb_id.filter(|i| i.starts_with("tt")),
            tvdb_id: ids.tvdb_id.filter(|id| *id > 0),
            cast: cast(self.aggregate_credits),
            seasons,
            title,
        }
    }
}

/// Episodes of a season aired by now: all of those before the season of
/// the last episode aired, that one's up to it, none after. Specials (and
/// every season, should the last aired be a special) count as aired once
/// their season began.
fn aired(season: i64, episodes: i64, air_date: Option<&str>, last: Option<&EpisodeRef>, now: i64) -> i64 {
    let begun = air_date.and_then(date_secs).is_some_and(|date| date <= now);
    match last {
        Some(last) if season > 0 && last.season_number > 0 => {
            if season < last.season_number {
                episodes
            } else if season == last.season_number {
                last.episode_number.max(0)
            } else {
                0
            }
        }
        Some(_) if begun => episodes,
        _ => 0,
    }
}

/// TMDB's "anime" keyword, or Japanese animation.
fn anime(keywords: &[Keyword], genres: &[Genre], language: Option<&str>, countries: &[String]) -> bool {
    keywords.iter().any(|k| k.id == ANIME_KEYWORD)
        || (genres.iter().any(|g| g.id == ANIMATION) && (language == Some("ja") || countries.iter().any(|c| c == "JP")))
}

fn cast(credits: Option<Credits>) -> Vec<CastMember> {
    credits
        .map(|c| c.cast)
        .unwrap_or_default()
        .into_iter()
        .take(CAST)
        .map(|c| CastMember {
            character: c.character.or_else(|| c.roles.into_iter().find_map(|r| r.character)).filter(|c| !c.is_empty()),
            photo_url: pictures::url(Picture::Profile, c.profile_path.as_deref()),
            name: c.name,
        })
        .collect()
}

/// TMDB's users' rating, where enough of them voted.
fn rating(average: Option<f64>, votes: Option<i64>) -> Option<f64> {
    average.filter(|a| *a > 0.0 && votes.unwrap_or(0) >= MIN_VOTES).map(|a| (a * 10.0).round() / 10.0)
}

fn year_of(date: &str) -> Option<i64> {
    date.get(..4)?.parse::<i64>().ok().filter(|y| *y > 1800)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(season_number: i64, episode_number: i64) -> EpisodeRef {
        EpisodeRef { season_number, episode_number }
    }

    #[test]
    fn episodes_aired_follow_the_last_one() {
        let now = date_secs("2024-06-01").unwrap();
        let last = episode(2, 5);
        assert_eq!(aired(1, 10, Some("2023-01-01"), Some(&last), now), 10);
        assert_eq!(aired(2, 10, Some("2024-05-01"), Some(&last), now), 5);
        assert_eq!(aired(3, 8, None, Some(&last), now), 0, "not begun");
        assert_eq!(aired(0, 3, Some("2023-06-01"), Some(&last), now), 3, "specials once begun");
        assert_eq!(aired(0, 3, Some("2025-06-01"), Some(&last), now), 0);
        assert_eq!(aired(1, 10, Some("2023-01-01"), None, now), 0, "nothing aired yet");
        let special = episode(0, 2);
        assert_eq!(aired(1, 6, Some("2023-01-01"), Some(&special), now), 6, "the last aired a special");
        assert_eq!(aired(2, 6, Some("2025-01-01"), Some(&special), now), 0);
        assert_eq!(aired(2, 4, Some("2024-05-01"), Some(&episode(2, 6)), now), 6, "TMDB's count behind");
    }

    #[test]
    fn anime_is_its_keyword_or_japanese_animation() {
        let animation = [Genre { id: ANIMATION, name: "Animation".into() }];
        let jp = ["JP".to_owned()];
        assert!(anime(&[Keyword { id: ANIME_KEYWORD }], &[], Some("en"), &[]));
        assert!(anime(&[], &animation, Some("ja"), &[]));
        assert!(anime(&[], &animation, None, &jp));
        assert!(!anime(&[], &animation, Some("en"), &["US".to_owned()]), "western animation");
        assert!(!anime(&[], &[], Some("ja"), &jp), "Japanese live action");
    }

    #[test]
    fn lists_leave_out_people_and_adult_titles() {
        let raw: RawPage = serde_json::from_value(serde_json::json!({
            "page": 2, "total_pages": 9000,
            "results": [
                {"id": 1, "media_type": "movie", "title": "Heat", "release_date": "1995-12-15",
                 "vote_average": 7.94, "vote_count": 7000, "poster_path": "/heat.jpg"},
                {"id": 2, "media_type": "tv", "name": "Cowboy Bebop", "first_air_date": "1998-04-03",
                 "vote_average": 8.0, "vote_count": 3},
                {"id": 3, "media_type": "person", "name": "Al Pacino"},
                {"id": 4, "media_type": "movie", "title": "Not for you", "adult": true},
                {"id": 5, "media_type": "movie", "title": " "}
            ]
        }))
        .unwrap();
        let page = raw.into_page(None);
        assert_eq!((page.page, page.total_pages), (2, MAX_PAGE));
        let titles: Vec<_> = page.results.iter().map(|t| (t.kind, t.tmdb_id, t.year, t.rating)).collect();
        assert_eq!(titles, [(TitleKind::Movie, 1, Some(1995), Some(7.9)), (TitleKind::Show, 2, Some(1998), None)]);
        assert_eq!(
            pictures::url(Picture::Poster, page.results[0].poster_path.as_deref()).as_deref(),
            Some("/api/discover/images/poster/heat.jpg")
        );
    }

    /// "A" is for all ages in Denmark and for adults in India.
    #[test]
    fn a_movies_age_is_read_with_its_ratings_country() {
        let raw = |country: &str| -> RawMovie {
            serde_json::from_value(serde_json::json!({
                "title": "Film",
                "release_dates": {"results": [
                    {"iso_3166_1": country, "release_dates": [{"certification": "A"}]},
                    {"iso_3166_1": "US", "release_dates": [{"certification": "R"}]}
                ]}
            }))
            .unwrap()
        };
        let danish = raw("DK").into_details(1, "DK");
        assert_eq!((danish.certification.as_deref(), danish.certification_country.as_deref()), (Some("A"), Some("DK")));
        assert_eq!(danish.age(), Some(0));
        assert_eq!(raw("IN").into_details(1, "IN").age(), Some(18));
        let elsewhere = raw("IN").into_details(1, "FR");
        assert_eq!((elsewhere.certification.as_deref(), elsewhere.age()), (Some("R"), Some(17)), "the US's");
    }

    #[test]
    fn upcoming_days_are_utc_dates() {
        assert_eq!(day(date_secs("2026-11-03").unwrap() + 23 * 3600), "2026-11-03");
    }

    #[test]
    fn a_shows_details_count_what_aired() {
        let raw: RawShow = serde_json::from_value(serde_json::json!({
            "name": "Show", "original_name": "Show", "first_air_date": "2020-01-01",
            "genres": [{"id": 16, "name": "Animation"}], "original_language": "ja",
            "seasons": [
                {"season_number": 1, "episode_count": 12, "air_date": "2020-01-01"},
                {"season_number": 0, "episode_count": 2, "air_date": "2020-06-01"},
                {"season_number": 2, "episode_count": 12, "air_date": "2024-04-01", "name": "Second"}
            ],
            "last_episode_to_air": {"season_number": 2, "episode_number": 7},
            "content_ratings": {"results": [{"iso_3166_1": "US", "rating": "TV-14"}, {"iso_3166_1": "DE", "rating": ""}]},
            "external_ids": {"imdb_id": "tt123", "tvdb_id": 42},
            "aggregate_credits": {"cast": [{"name": "A", "roles": [{"character": "B"}], "profile_path": null}]}
        }))
        .unwrap();
        let details = raw.into_details(7, "DE", date_secs("2024-05-01").unwrap());
        let seasons: Vec<_> = details.seasons.iter().map(|s| (s.number, s.name.as_str(), s.aired)).collect();
        assert_eq!(seasons, [(0, "Specials", 2), (1, "Season 1", 12), (2, "Second", 7)]);
        assert_eq!(details.certification.as_deref(), Some("TV-14"), "the region's, else the US's");
        assert_eq!(details.certification_country.as_deref(), Some("US"));
        assert_eq!(details.age(), Some(14));
        assert_eq!((details.imdb_id.as_deref(), details.tvdb_id), (Some("tt123"), Some(42)));
        assert!(details.anime && details.original_title.is_none());
        assert_eq!(details.cast[0].character.as_deref(), Some("B"));
    }
}
