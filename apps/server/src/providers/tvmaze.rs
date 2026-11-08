//! TVmaze (<https://www.tvmaze.com/api>): TV shows, seasons, episodes and
//! cast without an API key. At most 20 requests every 10 seconds per
//! address. The data is licensed CC BY-SA: pley credits TVmaze next to the
//! setting and in the README.
//!
//! TVmaze has no season 0: specials belong to the season they aired in and
//! have no number. They are numbered in air date order as season 0, which is
//! how specials are usually named on disk (`S00E01`).

use std::time::Duration;

use reqwest::header::LOCATION;
use serde::Deserialize;

use super::tv::{EpisodeMeta, SeasonMeta, ShowMeta, ShowRow};
use super::{
    Api, ProviderError, Throttle, full_date, loose_id, nullable, strip_html, text, title_score, year_of, year_score,
};
use crate::people::{self, NewCredit, PersonId, Role};

pub const API: &str = "https://api.tvmaze.com";

pub struct Tvmaze {
    api: Api,
    base: String,
}

#[derive(Debug, Deserialize)]
struct ShowId {
    id: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchResult {
    score: f64,
    show: Show,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Show {
    id: i64,
    name: Option<String>,
    #[serde(deserialize_with = "nullable")]
    genres: Vec<String>,
    premiered: Option<String>,
    rating: Option<Rating>,
    /// Popularity, 0-100.
    weight: Option<f64>,
    network: Option<Named>,
    #[serde(rename = "webChannel")]
    web_channel: Option<Named>,
    externals: Option<Externals>,
    image: Option<Image>,
    summary: Option<String>,
    #[serde(rename = "_embedded")]
    embedded: Option<Embedded>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Embedded {
    #[serde(deserialize_with = "nullable")]
    seasons: Vec<Season>,
    #[serde(deserialize_with = "nullable")]
    episodes: Vec<Episode>,
    #[serde(deserialize_with = "nullable")]
    cast: Vec<Cast>,
    #[serde(deserialize_with = "nullable")]
    crew: Vec<Crew>,
    #[serde(deserialize_with = "nullable")]
    images: Vec<ShowImage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Rating {
    average: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Named {
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Externals {
    #[serde(deserialize_with = "loose_id")]
    thetvdb: Option<i64>,
    imdb: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Image {
    medium: Option<String>,
    original: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Season {
    number: Option<i64>,
    name: Option<String>,
    #[serde(rename = "premiereDate")]
    premiere_date: Option<String>,
    image: Option<Image>,
    summary: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Episode {
    id: i64,
    name: Option<String>,
    season: Option<i64>,
    /// `None` for specials.
    number: Option<i64>,
    airdate: Option<String>,
    rating: Option<Rating>,
    image: Option<Image>,
    summary: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Cast {
    person: Person,
    character: Option<Named>,
    voice: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Person {
    id: i64,
    name: String,
    image: Option<Image>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Crew {
    #[serde(rename = "type")]
    kind: String,
    person: Person,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ShowImage {
    #[serde(rename = "type")]
    kind: Option<String>,
    main: bool,
    resolutions: Resolutions,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Resolutions {
    original: Option<Resolution>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Resolution {
    url: String,
    width: Option<i64>,
}

impl Tvmaze {
    pub fn new(http: reqwest::Client, base: &str) -> Self {
        let throttle = Throttle::new(20, Duration::from_secs(10));
        Self { api: Api::new(http, "TVmaze", throttle), base: base.trim_end_matches('/').to_owned() }
    }

    /// Whether TVmaze answers a search.
    pub async fn check(&self) -> Result<(), ProviderError> {
        let request = self.api.http.get(format!("{}/search/shows", self.base)).query(&[("q", "pley")]);
        self.api.json::<serde::de::IgnoredAny>(request).await.map(|_| ())
    }

    /// Everything TVmaze has about a show: found by its TVmaze, TheTVDB or
    /// IMDb id, else by title and year. `specials` also fetches the episodes
    /// that aren't part of a season.
    pub async fn show(&self, show: &ShowRow, specials: bool) -> Result<ShowMeta, ProviderError> {
        let id = match show.tvmaze_id {
            Some(id) => id,
            None => self.find(show).await?.ok_or(ProviderError::NotFound)?,
        };
        let embeds = ["seasons", "episodes", "cast", "crew", "images"].map(|e| ("embed[]", e));
        let details: Show =
            self.api.json(self.api.http.get(format!("{}/shows/{id}", self.base)).query(&embeds)).await?;
        let episodes = if specials {
            let url = format!("{}/shows/{id}/episodes", self.base);
            Some(self.api.json(self.api.http.get(url).query(&[("specials", "1")])).await?)
        } else {
            None
        };
        Ok(to_meta(details, episodes))
    }

    async fn find(&self, show: &ShowRow) -> Result<Option<i64>, ProviderError> {
        if let Some(tvdb) = show.tvdb_id
            && let Some(id) = self.lookup("thetvdb", &tvdb.to_string()).await?
        {
            return Ok(Some(id));
        }
        if let Some(imdb) = show.imdb_id.as_deref().filter(|i| i.starts_with("tt"))
            && let Some(id) = self.lookup("imdb", imdb).await?
        {
            return Ok(Some(id));
        }
        let request = self.api.http.get(format!("{}/search/shows", self.base)).query(&[("q", show.title.as_str())]);
        let results: Vec<SearchResult> = match self.api.json(request).await {
            Err(ProviderError::NotFound) => Vec::new(),
            other => other?,
        };
        Ok(best_match(&results, &show.title, show.year))
    }

    /// The show TVmaze has for another site's id. TVmaze answers with a
    /// redirect to the show, whose id is all that's needed.
    async fn lookup(&self, site: &str, id: &str) -> Result<Option<i64>, ProviderError> {
        let request = self.api.http.get(format!("{}/lookup/shows", self.base)).query(&[(site, id)]);
        let resp = match self.api.send(request).await {
            Err(ProviderError::NotFound) => return Ok(None),
            other => other?,
        };
        if resp.status().is_redirection() {
            let location = resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()).unwrap_or_default();
            return Ok(location.trim_end_matches('/').rsplit('/').next().and_then(|n| n.parse().ok()));
        }
        let bytes = resp.bytes().await.map_err(|e| anyhow::anyhow!("TVmaze: {e}"))?;
        Ok(serde_json::from_slice::<ShowId>(&bytes).ok().map(|s| s.id))
    }
}

/// The search result whose title and premiere year fit best (TVmaze's own
/// score and popularity break ties).
fn best_match(results: &[SearchResult], title: &str, year: Option<i64>) -> Option<i64> {
    results
        .iter()
        .filter_map(|r| {
            let fit = title_score(title, r.show.name.as_deref());
            (fit > 0.0).then(|| {
                let premiered = year_of(r.show.premiered.as_deref());
                let tiebreak = r.score.clamp(0.0, 2.0) / 10.0 + r.show.weight.unwrap_or(0.0).clamp(0.0, 100.0) / 2000.0;
                (r.show.id, fit + year_score(year, premiered) + tiebreak)
            })
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

fn picture(image: &Option<Image>) -> Option<String> {
    let image = image.as_ref()?;
    text(image.original.as_deref()).or_else(|| text(image.medium.as_deref()))
}

fn html(summary: Option<&str>) -> Option<String> {
    summary.map(strip_html).filter(|s| !s.is_empty())
}

fn episode(ep: &Episode, season: i64, number: i64) -> EpisodeMeta {
    EpisodeMeta {
        season,
        number,
        title: text(ep.name.as_deref()),
        summary: html(ep.summary.as_deref()),
        aired: full_date(ep.airdate.as_deref()),
        rating: ep.rating.as_ref().and_then(|r| r.average),
        still: picture(&ep.image),
    }
}

fn person(person: &Person, role: Role, character: Option<String>) -> NewCredit {
    NewCredit {
        person: PersonId::Tvmaze(person.id),
        name: person.name.trim().to_owned(),
        photo: picture(&person.image),
        role,
        character,
    }
}

fn to_meta(show: Show, all_episodes: Option<Vec<Episode>>) -> ShowMeta {
    let embedded = show.embedded.unwrap_or_default();
    let images = &embedded.images;
    let best_image = |kind: &str| {
        images
            .iter()
            .filter(|i| i.kind.as_deref() == Some(kind))
            .filter_map(|i| i.resolutions.original.as_ref().map(|r| (i.main, r.width.unwrap_or(0), r.url.as_str())))
            .filter(|(_, _, url)| !url.is_empty())
            .max_by_key(|(main, width, _)| (*main, *width))
            .map(|(_, _, url)| url.to_owned())
    };

    let list = all_episodes.unwrap_or(embedded.episodes);
    let mut episodes = Vec::with_capacity(list.len());
    let mut specials: Vec<&Episode> = Vec::new();
    for ep in &list {
        match (ep.season, ep.number) {
            (Some(season), Some(number)) => episodes.push(episode(ep, season, number)),
            _ => specials.push(ep),
        }
    }
    specials.sort_by(|a, b| {
        let date = |e: &Episode| full_date(e.airdate.as_deref()).unwrap_or_else(|| "9999".into());
        date(a).cmp(&date(b)).then(a.id.cmp(&b.id))
    });
    let first_special = episodes.iter().filter(|e| e.season == 0).map(|e| e.number).max().unwrap_or(0) + 1;
    episodes.extend(specials.into_iter().zip(first_special..).map(|(ep, number)| episode(ep, 0, number)));

    let cast: Vec<NewCredit> = embedded
        .cast
        .iter()
        .filter(|c| !c.person.name.trim().is_empty())
        .map(|c| {
            let part = c.character.as_ref().and_then(|ch| text(ch.name.as_deref()));
            let part = part.map(|p| if c.voice && !p.contains("(voice)") { format!("{p} (voice)") } else { p });
            person(&c.person, Role::Actor, part)
        })
        .collect();
    let creators: Vec<NewCredit> = embedded
        .crew
        .iter()
        .filter(|c| c.kind == "Creator" && !c.person.name.trim().is_empty())
        .map(|c| person(&c.person, Role::Creator, None))
        .collect();

    let externals = show.externals.unwrap_or_default();
    ShowMeta {
        title: text(show.name.as_deref()).unwrap_or_default(),
        original_title: None,
        summary: html(show.summary.as_deref()),
        year: year_of(show.premiered.as_deref()),
        premiered: full_date(show.premiered.as_deref()),
        rating: show.rating.and_then(|r| r.average),
        content_rating: None,
        content_rating_country: None,
        genres: show.genres.iter().filter_map(|g| text(Some(g))).collect(),
        network: show.network.or(show.web_channel).and_then(|n| text(n.name.as_deref())),
        imdb_id: text(externals.imdb.as_deref()).filter(|i| i.starts_with("tt")),
        tvdb_id: externals.thetvdb,
        tvmaze_id: Some(show.id),
        poster: picture(&show.image).or_else(|| best_image("poster")),
        backdrop: best_image("background"),
        credits: people::provider_show_credits(cast, creators),
        seasons: embedded
            .seasons
            .iter()
            .filter_map(|s| {
                Some(SeasonMeta {
                    number: s.number?,
                    name: text(s.name.as_deref()),
                    summary: html(s.summary.as_deref()),
                    premiered: full_date(s.premiere_date.as_deref()),
                    poster: picture(&s.image),
                })
            })
            .collect(),
        episodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = include_str!("../../tests/fixtures/providers/tvmaze_show.json");
    const EPISODES: &str = include_str!("../../tests/fixtures/providers/tvmaze_episodes.json");
    const SEARCH: &str = include_str!("../../tests/fixtures/providers/tvmaze_search.json");

    #[test]
    fn show_with_embedded_details() {
        let show: Show = serde_json::from_str(SHOW).unwrap();
        let meta = to_meta(show, None);
        assert_eq!(meta.title, "Harbour Lights");
        assert_eq!(
            meta.summary.as_deref(),
            Some(
                "Harbour Lights follows coastguard Nell Ward & her brother as storms bring old secrets ashore.\n\nFilmed in Cornwall."
            )
        );
        assert_eq!((meta.year, meta.premiered.as_deref()), (Some(2015), Some("2015-09-14")));
        assert_eq!(meta.rating, Some(7.9));
        assert_eq!(meta.genres, ["Drama", "Mystery"]);
        assert_eq!(meta.network.as_deref(), Some("BBC One"));
        assert_eq!(
            (meta.imdb_id.as_deref(), meta.tvdb_id, meta.tvmaze_id),
            (Some("tt4000001"), Some(290001), Some(4242))
        );
        assert_eq!(
            meta.poster.as_deref(),
            Some("https://static.tvmaze.com/uploads/images/original_untouched/1/1001.jpg")
        );
        assert_eq!(
            meta.backdrop.as_deref(),
            Some("https://static.tvmaze.com/uploads/images/original_untouched/1/1004.jpg")
        );
        assert_eq!(meta.content_rating, None);

        assert_eq!(meta.seasons.len(), 2);
        assert_eq!(meta.season(1).unwrap().name, None, "TVmaze names seasons \"\"");
        assert_eq!(meta.season(2).unwrap().name.as_deref(), Some("The Return"));
        assert_eq!(meta.season(1).unwrap().summary.as_deref(), Some("Nell comes home."));
        assert_eq!(meta.season(2).unwrap().poster, None);

        // Embedded episodes leave specials out.
        assert_eq!(meta.episodes.len(), 4);
        let pilot = meta.episode(1, 1).unwrap();
        assert_eq!(pilot.title.as_deref(), Some("The Wreck"));
        assert_eq!(pilot.summary.as_deref(), Some("A boat runs aground in the fog."));
        assert_eq!(pilot.aired.as_deref(), Some("2015-09-14"));
        assert_eq!(pilot.rating, Some(8.1));
        assert_eq!(
            pilot.still.as_deref(),
            Some("https://static.tvmaze.com/uploads/images/original_untouched/1/1101.jpg")
        );
        assert_eq!(meta.episode(2, 1).unwrap().still, None);
        assert_eq!(meta.episode(2, 1).unwrap().aired, None, "unaired episodes have an empty date");

        let cast: Vec<(PersonId, Role, Option<&str>)> =
            meta.credits.iter().map(|c| (c.person, c.role, c.character.as_deref())).collect();
        assert_eq!(
            cast,
            vec![
                (PersonId::Tvmaze(501), Role::Actor, Some("Nell Ward")),
                (PersonId::Tvmaze(502), Role::Actor, Some("Tom Ward")),
                (PersonId::Tvmaze(503), Role::Actor, Some("The Lighthouse (voice)")),
                (PersonId::Tvmaze(510), Role::Creator, None),
            ]
        );
        assert_eq!(
            meta.credits[0].photo.as_deref(),
            Some("https://static.tvmaze.com/uploads/images/original_untouched/2/2001.jpg")
        );
        assert_eq!(meta.credits[2].photo, None);
    }

    #[test]
    fn specials_are_numbered_by_air_date() {
        let show: Show = serde_json::from_str(SHOW).unwrap();
        let episodes: Vec<Episode> = serde_json::from_str(EPISODES).unwrap();
        let meta = to_meta(show, Some(episodes));
        assert_eq!(meta.episodes.len(), 6);
        assert_eq!(meta.episode(0, 1).unwrap().title.as_deref(), Some("A Harbour Christmas"));
        assert_eq!(meta.episode(0, 2).unwrap().title.as_deref(), Some("Behind the Lights"));
        assert_eq!(meta.episode(1, 2).unwrap().title.as_deref(), Some("Undertow"));
    }

    #[test]
    fn search_prefers_title_and_year() {
        let results: Vec<SearchResult> = serde_json::from_str(SEARCH).unwrap();
        assert_eq!(best_match(&results, "Harbour Lights", Some(2015)), Some(4242));
        assert_eq!(best_match(&results, "Harbour Lights", Some(1998)), Some(777));
        // Without a year, TVmaze's ranking decides.
        assert_eq!(best_match(&results, "Harbour Lights", None), Some(4242));
        assert_eq!(best_match(&results, "Something Else", None), None);
    }
}
