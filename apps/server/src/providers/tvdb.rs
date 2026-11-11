//! TheTVDB v4 (<https://thetvdb.github.io/v4-api/>): TV shows, with an API
//! key and, for keys tied to a subscription, the subscriber's PIN. `POST
//! /login` trades them for a token valid for a month; it is kept in memory
//! and renewed when TheTVDB stops accepting it.
//!
//! Written from TheTVDB's API documentation. Answers are read leniently
//! (missing fields, `null` lists, numbers sent as text) so a field TheTVDB
//! adds, drops or retypes costs that field, not the show.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::tv::{EpisodeMeta, SeasonMeta, ShowMeta, ShowRow};
use super::{
    Api, ProviderError, Throttle, full_date, loose_id, loose_text, nullable, text, title_score, year_of, year_score,
};
use crate::people::{self, NewCredit, PersonId, Role};
use crate::settings::Settings;

pub const API: &str = "https://api4.thetvdb.com/v4";
const ARTWORK_HOST: &str = "https://artworks.thetvdb.com";
/// Artwork types (`/artwork/types`).
const POSTER: i64 = 2;
const BACKGROUND: i64 = 3;
/// Character types, for records without a `peopleType`.
const ACTOR: i64 = 3;
const CREATOR: i64 = 6;
/// Episode pages read at most (TheTVDB sends up to 500 episodes a page).
const MAX_PAGES: usize = 20;

pub struct Tvdb {
    api: Api,
    base: String,
    token: tokio::sync::Mutex<Option<Token>>,
}

#[derive(Clone, PartialEq, Eq)]
struct Credentials {
    key: String,
    pin: String,
}

struct Token {
    credentials: Credentials,
    value: String,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    data: Option<T>,
    links: Option<Links>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Links {
    next: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Login {
    token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchResult {
    #[serde(deserialize_with = "loose_text")]
    tvdb_id: Option<String>,
    /// "series-81189"
    #[serde(deserialize_with = "loose_text")]
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    name: Option<String>,
    #[serde(deserialize_with = "loose_text")]
    year: Option<String>,
    first_air_time: Option<String>,
    #[serde(deserialize_with = "nullable")]
    aliases: Vec<serde_json::Value>,
    /// Language → title.
    translations: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RemoteIdResult {
    series: Option<SeriesId>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SeriesId {
    #[serde(deserialize_with = "loose_id")]
    id: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Series {
    #[serde(deserialize_with = "loose_id")]
    id: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    image: Option<String>,
    first_aired: Option<String>,
    #[serde(deserialize_with = "loose_text")]
    year: Option<String>,
    original_language: Option<String>,
    #[serde(deserialize_with = "loose_id")]
    default_season_type: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    genres: Vec<Named>,
    original_network: Option<Named>,
    latest_network: Option<Named>,
    #[serde(deserialize_with = "nullable")]
    remote_ids: Vec<RemoteId>,
    #[serde(deserialize_with = "nullable")]
    artworks: Vec<Artwork>,
    #[serde(deserialize_with = "nullable")]
    characters: Vec<Character>,
    #[serde(deserialize_with = "nullable")]
    seasons: Vec<Season>,
    #[serde(deserialize_with = "nullable")]
    content_ratings: Vec<ContentRating>,
    translations: Option<Translations>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Named {
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RemoteId {
    #[serde(deserialize_with = "loose_text")]
    id: Option<String>,
    #[serde(rename = "type", deserialize_with = "loose_id")]
    kind: Option<i64>,
    source_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Artwork {
    image: Option<String>,
    language: Option<String>,
    #[serde(rename = "type", deserialize_with = "loose_id")]
    kind: Option<i64>,
    score: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Character {
    /// The part played.
    name: Option<String>,
    #[serde(deserialize_with = "loose_id")]
    people_id: Option<i64>,
    person_name: Option<String>,
    #[serde(rename = "personImgURL")]
    person_img_url: Option<String>,
    image: Option<String>,
    people_type: Option<String>,
    #[serde(rename = "type", deserialize_with = "loose_id")]
    kind: Option<i64>,
    #[serde(deserialize_with = "loose_id")]
    sort: Option<i64>,
    #[serde(deserialize_with = "loose_id")]
    episode_id: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Season {
    #[serde(deserialize_with = "loose_id")]
    number: Option<i64>,
    name: Option<String>,
    image: Option<String>,
    #[serde(rename = "type")]
    kind: Option<SeasonType>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SeasonType {
    #[serde(deserialize_with = "loose_id")]
    id: Option<i64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ContentRating {
    name: Option<String>,
    country: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Translations {
    #[serde(deserialize_with = "nullable")]
    name_translations: Vec<Translation>,
    #[serde(deserialize_with = "nullable")]
    overview_translations: Vec<Translation>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Translation {
    language: Option<String>,
    name: Option<String>,
    overview: Option<String>,
    is_alias: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EpisodePage {
    #[serde(deserialize_with = "nullable")]
    episodes: Vec<Episode>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Episode {
    #[serde(deserialize_with = "loose_id")]
    id: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    aired: Option<String>,
    image: Option<String>,
    #[serde(deserialize_with = "loose_id")]
    number: Option<i64>,
    #[serde(deserialize_with = "loose_id")]
    season_number: Option<i64>,
}

impl Tvdb {
    pub fn new(http: reqwest::Client, base: &str) -> Self {
        let throttle = Throttle::new(20, Duration::from_secs(1));
        Self {
            api: Api::new(http, "TheTVDB", throttle),
            base: base.trim_end_matches('/').to_owned(),
            token: tokio::sync::Mutex::new(None),
        }
    }

    /// Everything TheTVDB has about a show: found by its TheTVDB or IMDb id,
    /// else by title and year; names and summaries in the metadata language
    /// when TheTVDB has them.
    pub async fn show(&self, show: &ShowRow, settings: &Settings) -> Result<ShowMeta, ProviderError> {
        let auth =
            Credentials { key: settings.tvdb_api_key.trim().to_owned(), pin: settings.tvdb_pin.trim().to_owned() };
        if auth.key.is_empty() {
            return Err(ProviderError::Unauthorized("TheTVDB"));
        }
        let lang = language(&settings.metadata_language);
        let id = match show.tvdb_id {
            Some(id) => id,
            None => self.find(&auth, show).await?.ok_or(ProviderError::NotFound)?,
        };
        let series: Series =
            self.get(&auth, &format!("/series/{id}/extended"), &[("meta", "translations".into())]).await?;
        let episodes = self.episodes(&auth, id, None).await?;
        let translated = if series.original_language.as_deref() == Some(lang.as_str()) {
            None
        } else {
            match self.episodes(&auth, id, Some(&lang)).await {
                Ok(list) => Some(list),
                Err(ProviderError::Unauthorized(name)) => return Err(ProviderError::Unauthorized(name)),
                // No translation: the original names stay.
                Err(err) => {
                    tracing::debug!("TheTVDB episodes of series {id} in {lang}: {err:#}");
                    None
                }
            }
        };
        Ok(to_meta(series, episodes, translated, &lang, country(&crate::ratings::region_in(settings))))
    }

    async fn find(&self, auth: &Credentials, show: &ShowRow) -> Result<Option<i64>, ProviderError> {
        if let Some(imdb) = show.imdb_id.as_deref().filter(|i| i.starts_with("tt")) {
            match self.get::<Vec<RemoteIdResult>>(auth, &format!("/search/remoteid/{imdb}"), &[]).await {
                Ok(found) => {
                    if let Some(id) = found.iter().find_map(|r| r.series.as_ref()?.id) {
                        return Ok(Some(id));
                    }
                }
                Err(ProviderError::NotFound) => {}
                Err(err) => return Err(err),
            }
        }
        let mut query = vec![("query", show.title.clone()), ("type", "series".to_owned())];
        if let Some(year) = show.year {
            query.push(("year", year.to_string()));
        }
        let mut results = self.search(auth, &query).await?;
        if results.is_empty() && show.year.is_some() {
            results = self.search(auth, &query[..2]).await?;
        }
        Ok(best_match(&results, &show.title, show.year))
    }

    async fn search(&self, auth: &Credentials, query: &[(&str, String)]) -> Result<Vec<SearchResult>, ProviderError> {
        match self.get(auth, "/search", query).await {
            Err(ProviderError::NotFound) => Ok(Vec::new()),
            other => other,
        }
    }

    /// The episodes in the series' default order, in `lang` when given.
    async fn episodes(&self, auth: &Credentials, id: i64, lang: Option<&str>) -> Result<Vec<Episode>, ProviderError> {
        let path = match lang {
            Some(lang) => format!("/series/{id}/episodes/default/{lang}"),
            None => format!("/series/{id}/episodes/default"),
        };
        let mut out = Vec::new();
        for page in 0..MAX_PAGES {
            let answer: Envelope<EpisodePage> = self.request(auth, &path, &[("page", page.to_string())]).await?;
            let episodes = answer.data.map(|d| d.episodes).unwrap_or_default();
            let next = answer.links.and_then(|l| l.next).is_some_and(|n| !n.is_null() && n != "");
            let empty = episodes.is_empty();
            out.extend(episodes);
            if empty || !next {
                break;
            }
        }
        Ok(out)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        auth: &Credentials,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ProviderError> {
        self.request::<T>(auth, path, query).await?.data.ok_or(ProviderError::NotFound)
    }

    /// An authenticated request; a token TheTVDB no longer accepts is
    /// replaced once.
    async fn request<T: DeserializeOwned>(
        &self,
        auth: &Credentials,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Envelope<T>, ProviderError> {
        let url = format!("{}{path}", self.base);
        let mut renewed = false;
        loop {
            let token = self.token(auth).await?;
            let request = self.api.http.get(&url).query(query).bearer_auth(&token);
            match self.api.json(request).await {
                Err(ProviderError::Unauthorized(_)) if !renewed => {
                    renewed = true;
                    self.forget(&token).await;
                }
                other => return other,
            }
        }
    }

    /// Whether TheTVDB signs in with this key and PIN. The token pley uses
    /// meanwhile is left alone.
    pub async fn check(&self, key: &str, pin: &str) -> Result<(), ProviderError> {
        let (key, pin) = (key.trim(), pin.trim());
        if key.is_empty() {
            return Err(ProviderError::Unauthorized("TheTVDB"));
        }
        let mut body = serde_json::json!({ "apikey": key });
        if !pin.is_empty() {
            body["pin"] = pin.into();
        }
        let login: Envelope<Login> =
            self.api.json(self.api.http.post(format!("{}/login", self.base)).json(&body)).await?;
        login
            .data
            .and_then(|d| d.token)
            .filter(|t| !t.is_empty())
            .map(|_| ())
            .ok_or(ProviderError::Unauthorized("TheTVDB"))
    }

    async fn token(&self, auth: &Credentials) -> Result<String, ProviderError> {
        let mut cached = self.token.lock().await;
        if let Some(token) = cached.as_ref().filter(|t| t.credentials == *auth) {
            return Ok(token.value.clone());
        }
        let mut body = serde_json::json!({ "apikey": auth.key });
        if !auth.pin.is_empty() {
            body["pin"] = auth.pin.clone().into();
        }
        let login: Envelope<Login> =
            self.api.json(self.api.http.post(format!("{}/login", self.base)).json(&body)).await?;
        let value =
            login.data.and_then(|d| d.token).filter(|t| !t.is_empty()).ok_or(ProviderError::Unauthorized("TheTVDB"))?;
        *cached = Some(Token { credentials: auth.clone(), value: value.clone() });
        Ok(value)
    }

    async fn forget(&self, token: &str) {
        let mut cached = self.token.lock().await;
        if cached.as_ref().is_some_and(|t| t.value == token) {
            *cached = None;
        }
    }
}

/// TheTVDB's language code for the metadata language: ISO 639-2 ("en-US" →
/// "eng"), except its own codes for Brazilian Portuguese and Traditional
/// Chinese.
pub(crate) fn language(setting: &str) -> String {
    let mut parts = setting.split(['-', '_']);
    let lang = parts.next().unwrap_or_default().to_ascii_lowercase();
    let region = parts.next().unwrap_or_default().to_ascii_uppercase();
    let code = match (lang.as_str(), region.as_str()) {
        ("pt", "BR") => "pt",
        ("zh", "TW" | "HK") => "zhtw",
        ("en", _) => "eng",
        ("fr", _) => "fra",
        ("de", _) => "deu",
        ("es", _) => "spa",
        ("it", _) => "ita",
        ("pt", _) => "por",
        ("nl", _) => "nld",
        ("sv", _) => "swe",
        ("da", _) => "dan",
        ("fi", _) => "fin",
        ("nb" | "nn" | "no", _) => "nor",
        ("pl", _) => "pol",
        ("cs", _) => "ces",
        ("ru", _) => "rus",
        ("uk", _) => "ukr",
        ("tr", _) => "tur",
        ("ar", _) => "ara",
        ("he", _) => "heb",
        ("hi", _) => "hin",
        ("ja", _) => "jpn",
        ("ko", _) => "kor",
        ("zh", _) => "zho",
        ("el", _) => "ell",
        ("hu", _) => "hun",
        ("ro", _) => "ron",
        ("hr", _) => "hrv",
        ("sk", _) => "slk",
        ("sl", _) => "slv",
        ("bg", _) => "bul",
        ("th", _) => "tha",
        ("vi", _) => "vie",
        ("id", _) => "ind",
        (other, _) if other.len() == 3 => return other.to_owned(),
        _ => "eng",
    };
    code.to_owned()
}

/// Countries by ISO 3166 alpha-2 and the alpha-3 code TheTVDB files content
/// ratings under.
const COUNTRIES: &[(&str, &str)] = &[
    ("GB", "gbr"),
    ("IE", "irl"),
    ("CA", "can"),
    ("AU", "aus"),
    ("NZ", "nzl"),
    ("FR", "fra"),
    ("BE", "bel"),
    ("CH", "che"),
    ("DE", "deu"),
    ("AT", "aut"),
    ("ES", "esp"),
    ("MX", "mex"),
    ("IT", "ita"),
    ("BR", "bra"),
    ("PT", "prt"),
    ("NL", "nld"),
    ("SE", "swe"),
    ("DK", "dnk"),
    ("FI", "fin"),
    ("NO", "nor"),
    ("PL", "pol"),
    ("CZ", "cze"),
    ("RU", "rus"),
    ("UA", "ukr"),
    ("TR", "tur"),
    ("SA", "sau"),
    ("IL", "isr"),
    ("IN", "ind"),
    ("JP", "jpn"),
    ("KR", "kor"),
    ("CN", "chn"),
    ("TW", "twn"),
];

/// The country TheTVDB files content ratings under (ISO 3166 alpha-3) for
/// the server's country (alpha-2, "GB" → "gbr"); the US's for one it
/// doesn't list.
fn country(region: &str) -> &'static str {
    COUNTRIES.iter().find(|(alpha2, _)| alpha2.eq_ignore_ascii_case(region)).map_or("usa", |(_, alpha3)| alpha3)
}

/// The alpha-2 code of one of TheTVDB's countries ("gbr" → "GB").
fn alpha2(alpha3: &str) -> Option<&'static str> {
    if alpha3 == "usa" {
        return Some("US");
    }
    COUNTRIES.iter().find(|(_, a3)| *a3 == alpha3).map(|(alpha2, _)| *alpha2)
}

/// A picture's full URL (TheTVDB sends full URLs, older records paths).
fn image_url(path: Option<&str>) -> Option<String> {
    let path = path?.trim();
    Some(if path.is_empty() {
        return None;
    } else if let Some(rest) = path.strip_prefix("http://") {
        format!("https://{rest}")
    } else if path.starts_with("https://") {
        path.to_owned()
    } else if path.starts_with('/') {
        format!("{ARTWORK_HOST}{path}")
    } else {
        format!("{ARTWORK_HOST}/banners/{path}")
    })
}

/// The search result whose titles (and aliases) and year fit best; earlier
/// results win ties.
fn best_match(results: &[SearchResult], title: &str, year: Option<i64>) -> Option<i64> {
    results
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind.as_deref().is_none_or(|k| k == "series"))
        .filter_map(|(position, r)| {
            let id = r
                .tvdb_id
                .as_deref()
                .and_then(|s| s.trim().parse().ok())
                .or_else(|| r.id.as_deref()?.rsplit('-').next()?.parse().ok())?;
            let translations = r.translations.iter().flat_map(|t| t.values()).filter_map(|v| v.as_str());
            let aliases = r.aliases.iter().filter_map(|a| a.as_str().or_else(|| a.get("name")?.as_str()));
            let fit = title_score(title, r.name.as_deref().into_iter().chain(translations).chain(aliases));
            let found = year_of(r.year.as_deref()).or_else(|| year_of(r.first_air_time.as_deref()));
            (fit > 0.0).then(|| (id, fit + year_score(year, found) - position as f64 / 1000.0))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

fn is_kind(c: &Character, name: &str, kind: i64) -> bool {
    match c.people_type.as_deref() {
        Some(t) => t.eq_ignore_ascii_case(name),
        None => c.kind == Some(kind),
    }
}

fn credit(c: &Character, role: Role) -> Option<NewCredit> {
    Some(NewCredit {
        person: PersonId::Tvdb(c.people_id?),
        name: text(c.person_name.as_deref())?,
        photo: image_url(c.person_img_url.as_deref()).or_else(|| image_url(c.image.as_deref())),
        role,
        character: if role == Role::Actor { text(c.name.as_deref()) } else { None },
    })
}

fn to_meta(
    series: Series,
    episodes: Vec<Episode>,
    translated: Option<Vec<Episode>>,
    lang: &str,
    country: &str,
) -> ShowMeta {
    let translations = series.translations.unwrap_or_default();
    let pick = |list: &[Translation], field: fn(&Translation) -> Option<&str>| {
        let find = |wanted: &str| {
            list.iter()
                .filter(|t| t.is_alias != Some(true) && t.language.as_deref() == Some(wanted))
                .find_map(|t| text(field(t)))
        };
        find(lang).or_else(|| find("eng"))
    };
    let original = text(series.name.as_deref());
    let title = pick(&translations.name_translations, |t| t.name.as_deref()).or_else(|| original.clone());
    let summary = pick(&translations.overview_translations, |t| t.overview.as_deref())
        .or_else(|| text(series.overview.as_deref()));

    let rating = |wanted: &str| {
        series
            .content_ratings
            .iter()
            .find(|r| r.country.as_deref() == Some(wanted))
            .and_then(|r| text(r.name.as_deref()))
    };
    let content_rating = rating(country).map(|r| (country, r)).or_else(|| rating("usa").map(|r| ("usa", r)));
    let mut artworks: Vec<&Artwork> =
        series.artworks.iter().filter(|a| image_url(a.image.as_deref()).is_some()).collect();
    artworks.sort_by(|a, b| b.score.unwrap_or(0.0).total_cmp(&a.score.unwrap_or(0.0)));
    let artwork = |kind: i64, language: Option<&str>| {
        artworks
            .iter()
            .find(|a| a.kind == Some(kind) && a.language.as_deref() == language)
            .and_then(|a| image_url(a.image.as_deref()))
    };
    let best = |kind: i64| artworks.iter().find(|a| a.kind == Some(kind)).and_then(|a| image_url(a.image.as_deref()));
    // A poster in the metadata language, else TheTVDB's main one; a
    // background without text, else one in the metadata language.
    let poster = artwork(POSTER, Some(lang)).or_else(|| image_url(series.image.as_deref())).or_else(|| best(POSTER));
    let backdrop = artwork(BACKGROUND, None).or_else(|| artwork(BACKGROUND, Some(lang))).or_else(|| best(BACKGROUND));

    let translated: HashMap<i64, (Option<String>, Option<String>)> = translated
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| Some((e.id?, (text(e.name.as_deref()), text(e.overview.as_deref())))))
        .collect();
    let episodes: Vec<EpisodeMeta> = episodes
        .iter()
        .filter_map(|e| {
            let (name, overview) = e.id.and_then(|id| translated.get(&id).cloned()).unwrap_or_default();
            Some(EpisodeMeta {
                season: e.season_number?,
                number: e.number?,
                title: name.or_else(|| text(e.name.as_deref())),
                summary: overview.or_else(|| text(e.overview.as_deref())),
                aired: full_date(e.aired.as_deref()),
                rating: None,
                still: image_url(e.image.as_deref()),
            })
        })
        .collect();

    // Seasons of the order the episodes come in; a season premiered when its
    // first episode aired.
    let default_type = series.default_season_type;
    let mut numbers = HashSet::new();
    let seasons: Vec<SeasonMeta> = series
        .seasons
        .iter()
        .filter(|s| {
            let kind = s.kind.as_ref();
            match (default_type, kind.and_then(|k| k.id)) {
                (Some(wanted), Some(id)) => wanted == id,
                _ => kind.and_then(|k| k.kind.as_deref()).is_none_or(|k| k == "official" || k == "default"),
            }
        })
        .filter_map(|s| {
            let number = s.number?;
            numbers.insert(number).then(|| SeasonMeta {
                number,
                name: text(s.name.as_deref()),
                summary: None,
                premiered: episodes.iter().filter(|e| e.season == number).filter_map(|e| e.aired.clone()).min(),
                poster: image_url(s.image.as_deref()),
            })
        })
        .collect();

    let mut actors: Vec<&Character> =
        series.characters.iter().filter(|c| c.episode_id.is_none() && is_kind(c, "Actor", ACTOR)).collect();
    // Billing order; unsorted parts last, in TheTVDB's order.
    actors.sort_by_key(|c| c.sort.filter(|s| *s > 0).unwrap_or(i64::MAX));
    let cast = actors.into_iter().filter_map(|c| credit(c, Role::Actor)).collect();
    let creators = series
        .characters
        .iter()
        .filter(|c| c.episode_id.is_none() && is_kind(c, "Creator", CREATOR))
        .filter_map(|c| credit(c, Role::Creator))
        .collect();

    ShowMeta {
        original_title: original.filter(|o| Some(o) != title.as_ref()),
        title: title.unwrap_or_default(),
        summary,
        year: year_of(series.year.as_deref()).or_else(|| year_of(series.first_aired.as_deref())),
        premiered: full_date(series.first_aired.as_deref()),
        rating: None,
        content_rating_country: content_rating.as_ref().and_then(|(country, _)| alpha2(country)).map(str::to_owned),
        content_rating: content_rating.map(|(_, rating)| rating),
        genres: series.genres.iter().filter_map(|g| text(g.name.as_deref())).collect(),
        network: series
            .original_network
            .as_ref()
            .and_then(|n| text(n.name.as_deref()))
            .or_else(|| series.latest_network.as_ref().and_then(|n| text(n.name.as_deref()))),
        imdb_id: series
            .remote_ids
            .iter()
            .find(|r| r.source_name.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("imdb")) || r.kind == Some(2))
            .and_then(|r| text(r.id.as_deref()))
            .filter(|i| i.starts_with("tt")),
        tvdb_id: series.id,
        tvmaze_id: None,
        poster,
        backdrop,
        credits: people::provider_show_credits(cast, creators),
        seasons,
        episodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIES: &str = include_str!("../../tests/fixtures/providers/tvdb_series.json");
    const EPISODES: &str = include_str!("../../tests/fixtures/providers/tvdb_episodes.json");
    const EPISODES_FRA: &str = include_str!("../../tests/fixtures/providers/tvdb_episodes_fra.json");
    const SEARCH: &str = include_str!("../../tests/fixtures/providers/tvdb_search.json");

    fn series() -> Series {
        serde_json::from_str::<Envelope<Series>>(SERIES).unwrap().data.unwrap()
    }

    fn episodes(json: &str) -> Vec<Episode> {
        serde_json::from_str::<Envelope<EpisodePage>>(json).unwrap().data.unwrap().episodes
    }

    #[test]
    fn series_in_english() {
        let meta = to_meta(series(), episodes(EPISODES), None, "eng", "usa");
        assert_eq!(meta.title, "Harbour Lights");
        assert_eq!(meta.original_title, None);
        assert_eq!(meta.summary.as_deref(), Some("Coastguard Nell Ward comes home to Cornwall."));
        assert_eq!((meta.year, meta.premiered.as_deref()), (Some(2015), Some("2015-09-14")));
        assert_eq!(meta.content_rating.as_deref(), Some("TV-14"));
        assert_eq!(meta.genres, ["Drama", "Mystery"]);
        assert_eq!(meta.network.as_deref(), Some("BBC One"));
        assert_eq!((meta.imdb_id.as_deref(), meta.tvdb_id), (Some("tt4000001"), Some(290001)));
        assert_eq!(
            meta.poster.as_deref(),
            Some("https://artworks.thetvdb.com/banners/v4/series/290001/posters/eng.jpg")
        );
        assert_eq!(
            meta.backdrop.as_deref(),
            Some("https://artworks.thetvdb.com/banners/v4/series/290001/backgrounds/textless.jpg")
        );

        // Aired order only: the DVD order's season 1 is left out.
        let numbers: Vec<i64> = meta.seasons.iter().map(|s| s.number).collect();
        assert_eq!(numbers, [0, 1, 2]);
        let first = meta.season(1).unwrap();
        assert_eq!(first.poster.as_deref(), Some("https://artworks.thetvdb.com/banners/v4/season/1001/posters/s1.jpg"));
        assert_eq!(first.premiered.as_deref(), Some("2015-09-14"));
        assert_eq!(meta.season(2).unwrap().poster, None);

        assert_eq!(meta.episodes.len(), 5);
        let pilot = meta.episode(1, 1).unwrap();
        assert_eq!(pilot.title.as_deref(), Some("The Wreck"));
        assert_eq!(pilot.summary.as_deref(), Some("A boat runs aground in the fog."));
        assert_eq!(pilot.aired.as_deref(), Some("2015-09-14"));
        assert_eq!(pilot.still.as_deref(), Some("https://artworks.thetvdb.com/banners/episodes/290001/3001.jpg"));
        assert_eq!(meta.episode(0, 1).unwrap().title.as_deref(), Some("A Harbour Christmas"));

        let cast: Vec<(PersonId, Role, Option<&str>)> =
            meta.credits.iter().map(|c| (c.person, c.role, c.character.as_deref())).collect();
        assert_eq!(
            cast,
            vec![
                (PersonId::Tvdb(501), Role::Actor, Some("Nell Ward")),
                (PersonId::Tvdb(502), Role::Actor, Some("Tom Ward")),
                (PersonId::Tvdb(503), Role::Actor, Some("The Lighthouse")),
                (PersonId::Tvdb(510), Role::Creator, None),
            ]
        );
        assert_eq!(
            meta.credits[0].photo.as_deref(),
            Some("https://artworks.thetvdb.com/banners/v4/person/501/photo.jpg")
        );
        // Without a photo of the person, the one in character.
        assert_eq!(meta.credits[1].photo.as_deref(), Some("https://artworks.thetvdb.com/banners/v4/character/602.jpg"));
    }

    #[test]
    fn series_in_french() {
        let meta = to_meta(series(), episodes(EPISODES), Some(episodes(EPISODES_FRA)), "fra", "fra");
        assert_eq!(meta.title, "Les Feux du port");
        assert_eq!(meta.original_title.as_deref(), Some("Harbour Lights"));
        assert_eq!(meta.summary.as_deref(), Some("La garde-côte Nell Ward rentre en Cornouailles."));
        assert_eq!(
            meta.poster.as_deref(),
            Some("https://artworks.thetvdb.com/banners/v4/series/290001/posters/fra.jpg")
        );
        // No French rating: the US one.
        assert_eq!(meta.content_rating.as_deref(), Some("TV-14"));
        assert_eq!(meta.content_rating_country.as_deref(), Some("US"));
        assert_eq!(meta.episode(1, 1).unwrap().title.as_deref(), Some("L'Épave"));
        // Not translated: the original title.
        assert_eq!(meta.episode(1, 2).unwrap().title.as_deref(), Some("Undertow"));
        let british = to_meta(series(), vec![], None, "eng", "gbr");
        assert_eq!(british.content_rating.as_deref(), Some("12"));
        assert_eq!(british.content_rating_country.as_deref(), Some("GB"));
    }

    #[test]
    fn search_prefers_title_and_year() {
        let results: Vec<SearchResult> = serde_json::from_str::<Envelope<_>>(SEARCH).unwrap().data.unwrap();
        assert_eq!(best_match(&results, "Harbour Lights", Some(2015)), Some(290001));
        assert_eq!(best_match(&results, "Harbour Lights", Some(1998)), Some(70001));
        assert_eq!(best_match(&results, "Harbor Lights", None), Some(290001), "aliases count");
        assert_eq!(best_match(&results, "Unrelated", None), None);
    }

    #[test]
    fn languages_and_countries() {
        assert_eq!(language("en-US"), "eng");
        assert_eq!(language("pt-BR"), "pt");
        assert_eq!(language("pt-PT"), "por");
        assert_eq!(language("zh-TW"), "zhtw");
        assert_eq!(language("zh-CN"), "zho");
        assert_eq!(language("nb-NO"), "nor");
        assert_eq!(language("xx"), "eng");
        assert_eq!(country("US"), "usa");
        assert_eq!(country("GB"), "gbr");
        assert_eq!(country("de"), "deu");
        assert_eq!(country("XX"), "usa");
    }

    #[test]
    fn picture_urls() {
        assert_eq!(
            image_url(Some("/banners/posters/1.jpg")).as_deref(),
            Some("https://artworks.thetvdb.com/banners/posters/1.jpg")
        );
        assert_eq!(
            image_url(Some("posters/1.jpg")).as_deref(),
            Some("https://artworks.thetvdb.com/banners/posters/1.jpg")
        );
        assert_eq!(
            image_url(Some("http://artworks.thetvdb.com/banners/1.jpg")).as_deref(),
            Some("https://artworks.thetvdb.com/banners/1.jpg")
        );
        assert_eq!(image_url(Some(" ")), None);
    }
}
