//! A TV show as TVmaze or TheTVDB describe it, and how it is written onto
//! the show, its seasons and its episodes. Fields a provider has no value for
//! keep what they had (an earlier source's content rating, a tagline), and
//! local artwork always wins over downloaded pictures.

use futures::StreamExt;

use super::{ProviderError, artwork};
use crate::db::Db;
use crate::overrides::RATING_LOCKED;
use crate::people::{self, NewCredit};
use crate::ratings::Kind;
use crate::store::Store;
use crate::util::{now, sort_title};

/// Episode stills downloaded at the same time.
const STILL_DOWNLOADS: usize = 4;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShowMeta {
    pub title: String,
    pub original_title: Option<String>,
    pub summary: Option<String>,
    pub year: Option<i64>,
    /// First air date, `YYYY-MM-DD`.
    pub premiered: Option<String>,
    /// 0-10.
    pub rating: Option<f64>,
    pub content_rating: Option<String>,
    /// ISO 3166-1 alpha-2, when the source says.
    pub content_rating_country: Option<String>,
    pub genres: Vec<String>,
    pub network: Option<String>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub tvmaze_id: Option<i64>,
    /// Picture URLs.
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub credits: Vec<NewCredit>,
    pub seasons: Vec<SeasonMeta>,
    pub episodes: Vec<EpisodeMeta>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeasonMeta {
    pub number: i64,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub premiered: Option<String>,
    pub poster: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EpisodeMeta {
    /// 0: specials.
    pub season: i64,
    pub number: i64,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub aired: Option<String>,
    pub rating: Option<f64>,
    pub still: Option<String>,
}

impl ShowMeta {
    pub fn season(&self, number: i64) -> Option<&SeasonMeta> {
        self.seasons.iter().find(|s| s.number == number)
    }

    pub fn episode(&self, season: i64, number: i64) -> Option<&EpisodeMeta> {
        self.episodes.iter().find(|e| e.season == season && e.number == number)
    }
}

/// The show being refreshed, as stored.
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct ShowRow {
    pub id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub tvmaze_id: Option<i64>,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
}

pub async fn load(db: &Db, show_id: i64) -> Result<ShowRow, ProviderError> {
    sqlx::query_as(
        "SELECT id, title, year, imdb_id, tvdb_id, tvmaze_id, poster, backdrop FROM items WHERE id = ? AND kind = 'show'",
    )
    .bind(show_id)
    .fetch_optional(&db.read)
    .await?
    .ok_or(ProviderError::NotFound)
}

/// Whether the library has specials (season 0) of the show.
pub async fn has_specials(db: &Db, show_id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM items WHERE parent_id = ? AND kind = 'season' AND idx = 0)")
        .bind(show_id)
        .fetch_one(&db.read)
        .await
}

struct SeasonUpdate {
    id: i64,
    number: i64,
    poster: Option<String>,
}

struct EpisodeUpdate {
    id: i64,
    season: i64,
    number: i64,
    still: Option<String>,
}

/// Store a show fetched from a provider: the show, the seasons and episodes
/// the library has, and the cast.
pub async fn write(db: &Db, store: &Store, show: &ShowRow, meta: &ShowMeta) -> Result<(), ProviderError> {
    // Pictures first, so nothing is downloaded while the database is written.
    let poster = artwork(store, show.poster.as_deref(), meta.poster.as_deref()).await;
    let backdrop = artwork(store, show.backdrop.as_deref(), meta.backdrop.as_deref()).await;
    let seasons: Vec<(i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, idx, poster FROM items
         WHERE parent_id = ? AND kind = 'season' AND idx IS NOT NULL AND metadata_locked = 0",
    )
    .bind(show.id)
    .fetch_all(&db.read)
    .await?;
    let mut season_updates = Vec::new();
    for (id, number, current) in seasons {
        if let Some(season) = meta.season(number) {
            let poster = artwork(store, current.as_deref(), season.poster.as_deref()).await;
            season_updates.push(SeasonUpdate { id, number, poster });
        }
    }
    let episodes: Vec<(i64, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT e.id, s.idx, e.idx, e.thumb FROM items e JOIN items s ON s.id = e.parent_id
         WHERE e.grandparent_id = ? AND e.kind = 'episode' AND e.idx IS NOT NULL AND s.idx IS NOT NULL
           AND e.metadata_locked = 0",
    )
    .bind(show.id)
    .fetch_all(&db.read)
    .await?;
    let episode_updates: Vec<EpisodeUpdate> =
        futures::stream::iter(episodes.into_iter().filter_map(|(id, season, number, current)| {
            let ep = meta.episode(season, number)?;
            Some(async move {
                let still = artwork(store, current.as_deref(), ep.still.as_deref()).await;
                EpisodeUpdate { id, season, number, still }
            })
        }))
        .buffer_unordered(STILL_DOWNLOADS)
        .collect()
        .await;

    let now = now();
    let title = if meta.title.is_empty() { show.title.as_str() } else { meta.title.as_str() };
    let genres = (!meta.genres.is_empty()).then(|| serde_json::to_string(&meta.genres).unwrap_or_default());
    let region = crate::ratings::region(db).await?;
    let country = meta.content_rating_country.as_deref().unwrap_or(&region);
    let age = meta.content_rating.as_deref().and_then(|r| crate::ratings::age(Some(country), Kind::Tv, r));
    let mut tx = db.write.begin().await?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE items SET title = ?, sort_title = ?, original_title = COALESCE(?, original_title), year = COALESCE(?, year),
            summary = COALESCE(?, summary), rating = COALESCE(?, rating),
            content_rating = CASE WHEN ? AND NOT {RATING_LOCKED} THEN ? ELSE content_rating END,
            content_rating_country = CASE WHEN ? AND NOT {RATING_LOCKED} THEN ? ELSE content_rating_country END,
            rating_age = CASE WHEN ? AND NOT {RATING_LOCKED} THEN ? ELSE rating_age END,
            genres = COALESCE(?, genres), studio = COALESCE(?, studio), release_date = COALESCE(?, release_date),
            imdb_id = COALESCE(?, imdb_id), tvdb_id = COALESCE(?, tvdb_id), tvmaze_id = COALESCE(?, tvmaze_id),
            poster = COALESCE(?, poster), backdrop = COALESCE(?, backdrop), metadata_at = ?, updated_at = ?
         WHERE id = ?"
    )))
    .bind(title)
    .bind(sort_title(title))
    .bind(meta.original_title.as_deref().filter(|o| *o != title))
    .bind(meta.year)
    .bind(&meta.summary)
    .bind(meta.rating.filter(|r| *r > 0.0))
    .bind(meta.content_rating.is_some())
    .bind(&meta.content_rating)
    .bind(meta.content_rating.is_some())
    .bind(&meta.content_rating_country)
    .bind(meta.content_rating.is_some())
    .bind(age)
    .bind(genres)
    .bind(&meta.network)
    .bind(&meta.premiered)
    .bind(&meta.imdb_id)
    .bind(meta.tvdb_id)
    .bind(meta.tvmaze_id)
    .bind(poster)
    .bind(backdrop)
    .bind(now)
    .bind(now)
    .bind(show.id)
    .execute(&mut *tx)
    .await?;
    for update in season_updates {
        let Some(season) = meta.season(update.number) else { continue };
        let number = update.number;
        let name = season
            .name
            .clone()
            .unwrap_or_else(|| if number == 0 { "Specials".into() } else { format!("Season {number}") });
        sqlx::query(
            "UPDATE items SET title = ?, sort_title = ?, summary = COALESCE(?, summary),
                release_date = COALESCE(?, release_date), year = COALESCE(?, year), poster = COALESCE(?, poster),
                metadata_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&name)
        .bind(format!("{number:04}"))
        .bind(&season.summary)
        .bind(&season.premiered)
        .bind(super::year_of(season.premiered.as_deref()))
        .bind(update.poster)
        .bind(now)
        .bind(now)
        .bind(update.id)
        .execute(&mut *tx)
        .await?;
    }
    for update in episode_updates {
        let Some(ep) = meta.episode(update.season, update.number) else { continue };
        let title = ep.title.clone().unwrap_or_else(|| format!("Episode {}", update.number));
        sqlx::query(
            "UPDATE items SET title = ?, sort_title = ?, summary = COALESCE(?, summary),
                release_date = COALESCE(?, release_date), year = COALESCE(?, year), rating = COALESCE(?, rating),
                thumb = COALESCE(?, thumb), metadata_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&title)
        .bind(sort_title(&title))
        .bind(&ep.summary)
        .bind(&ep.aired)
        .bind(super::year_of(ep.aired.as_deref()))
        .bind(ep.rating.filter(|r| *r > 0.0))
        .bind(update.still)
        .bind(now)
        .bind(now)
        .bind(update.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    // A provider without cast for the show leaves the credits it had.
    if !meta.credits.is_empty() {
        people::replace(db, show.id, &meta.credits).await?;
    }
    Ok(())
}
