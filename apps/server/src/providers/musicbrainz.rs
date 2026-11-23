//! MusicBrainz (<https://musicbrainz.org/doc/MusicBrainz_API>): artists and
//! albums (release groups) without an API key, with album covers from the
//! Cover Art Archive. MusicBrainz allows about one request a second and asks
//! applications to name themselves and a contact in their User-Agent. Its
//! data is public domain (CC0).
//!
//! The files' tags win: MusicBrainz fills in what they lack (genres, the
//! release date, the year, a cover) and links artists and albums to their
//! MusicBrainz ids (MBIDs). Files tagged with MusicBrainz Picard carry those
//! ids, which saves the search. Artists get no picture: MusicBrainz has none.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use reqwest::header::ACCEPT;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{Api, ProviderError, Throttle, artwork, full_date, loose_id, nullable, title_score, year_of, year_score};
use crate::db::Db;
use crate::library::LibraryRow;
use crate::store::Store;
use crate::util::{normalize_key, now};

pub const API: &str = "https://musicbrainz.org/ws/2";
pub const COVER_ART: &str = "https://coverartarchive.org";
/// Genres kept for an artist or album.
const MAX_GENRES: usize = 5;
/// A library pass stops after this many failures in a row (MusicBrainz is
/// down or unreachable); what's left is tried after the next scan.
const MAX_FAILURES: u32 = 5;
/// What the scanner calls artists and albums it knows nothing about.
const UNKNOWN: [&str; 2] = ["Unknown Artist", "Unknown Album"];

pub struct MusicBrainz {
    api: Api,
    base: String,
    cover_art: String,
    db: Db,
    store: Arc<Store>,
    /// MusicBrainz's genres: search results carry free-form tags, and the
    /// tags that are genres are the album's or artist's genres.
    genres: tokio::sync::OnceCell<HashSet<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ArtistSearch {
    #[serde(deserialize_with = "nullable")]
    artists: Vec<Artist>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Artist {
    id: String,
    name: String,
    sort_name: Option<String>,
    #[serde(deserialize_with = "loose_id")]
    score: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    aliases: Vec<Alias>,
    #[serde(deserialize_with = "nullable")]
    tags: Vec<Tag>,
    #[serde(deserialize_with = "nullable")]
    genres: Vec<Tag>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Alias {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Tag {
    name: String,
    #[serde(deserialize_with = "loose_id")]
    count: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GroupSearch {
    #[serde(rename = "release-groups", deserialize_with = "nullable")]
    release_groups: Vec<ReleaseGroup>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct ReleaseGroup {
    id: String,
    title: String,
    #[serde(deserialize_with = "loose_id")]
    score: Option<i64>,
    primary_type: Option<String>,
    #[serde(deserialize_with = "nullable")]
    secondary_types: Vec<String>,
    first_release_date: Option<String>,
    #[serde(deserialize_with = "nullable")]
    artist_credit: Vec<Credit>,
    #[serde(deserialize_with = "nullable")]
    tags: Vec<Tag>,
    #[serde(deserialize_with = "nullable")]
    genres: Vec<Tag>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Credit {
    name: Option<String>,
    artist: Option<CreditedArtist>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CreditedArtist {
    id: String,
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Release {
    release_group: Option<ReleaseGroup>,
    #[serde(deserialize_with = "nullable")]
    artist_credit: Vec<Credit>,
}

/// What a track's probe says about the album (`media_files.media_info`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TrackInfo {
    tags: HashMap<String, String>,
    has_cover_art: bool,
}

/// MusicBrainz ids and cover art found in the files.
#[derive(Debug, Default)]
struct FileIds {
    release_group: Option<String>,
    release: Option<String>,
    album_artist: Option<String>,
    cover: bool,
}

/// The album MusicBrainz has for one of ours.
#[derive(Debug, PartialEq)]
struct AlbumMatch {
    id: String,
    first_release: Option<String>,
    genres: Vec<String>,
    /// The first credited artist: id and the names it goes by.
    artist: Option<(String, Vec<String>)>,
}

#[derive(sqlx::FromRow)]
struct AlbumRow {
    title: String,
    year: Option<i64>,
    poster: Option<String>,
    musicbrainz_id: Option<String>,
    artist_id: Option<i64>,
    artist: Option<String>,
    artist_mbid: Option<String>,
}

impl MusicBrainz {
    pub fn new(http: reqwest::Client, base: &str, cover_art: &str, db: Db, store: Arc<Store>) -> Self {
        let base = base.trim_end_matches('/').to_owned();
        // The public server allows a request a second; mirrors don't limit.
        let throttle = if base == API {
            Throttle::new(1, Duration::from_secs(1))
        } else {
            Throttle::new(50, Duration::from_secs(1))
        };
        Self {
            api: Api::new(http, "MusicBrainz", throttle),
            base,
            cover_art: cover_art.trim_end_matches('/').to_owned(),
            db,
            store,
            genres: tokio::sync::OnceCell::new(),
        }
    }

    /// Look up the albums, then the artists, of a library that have no
    /// MusicBrainz metadata yet (all of them when `force`). Albums go first:
    /// the artists they credit save searching for the artists.
    pub async fn refresh_library(
        &self,
        lib: &LibraryRow,
        force: bool,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> anyhow::Result<()> {
        let pending = |kind: &'static str| {
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM items WHERE library_id = ? AND kind = ? AND metadata_locked = 0
                   AND (? OR metadata_at IS NULL) ORDER BY added_at DESC",
            )
            .bind(lib.id)
            .bind(kind)
            .bind(force)
            .fetch_all(&self.db.read)
        };
        let albums = pending("album").await?;
        let artists = pending("artist").await?;
        let total = (albums.len() + artists.len()) as u64;
        if total == 0 {
            return Ok(());
        }
        tracing::info!(
            "fetching MusicBrainz metadata for {} albums and {} artists in library {}",
            albums.len(),
            artists.len(),
            lib.name
        );
        let items = albums.into_iter().map(|id| (id, true)).chain(artists.into_iter().map(|id| (id, false)));
        let (mut done, mut failures) = (0u64, 0u32);
        for (id, album) in items {
            let result = if album { self.refresh_album(id).await } else { self.refresh_artist(id).await };
            done += 1;
            match result {
                Ok(()) => failures = 0,
                Err(ProviderError::NotFound) => {
                    failures = 0;
                    tracing::debug!("no MusicBrainz match for item {id}");
                    sqlx::query("UPDATE items SET metadata_at = ? WHERE id = ?")
                        .bind(-now())
                        .bind(id)
                        .execute(&self.db.write)
                        .await?;
                }
                Err(err) => {
                    tracing::warn!("MusicBrainz metadata for item {id} failed: {err:#}");
                    failures += 1;
                    if failures >= MAX_FAILURES {
                        bail!("MusicBrainz is not answering; the rest waits for the next scan");
                    }
                }
            }
            // Hand edits win.
            crate::overrides::apply(&self.db, &[id]).await?;
            if done % 5 == 0 || done == total {
                progress(done, total);
            }
        }
        Ok(())
    }

    /// Refresh an album, or an artist and its albums.
    pub async fn refresh_item(&self, id: i64) -> Result<(), ProviderError> {
        let kind: Option<String> =
            sqlx::query_scalar("SELECT kind FROM items WHERE id = ?").bind(id).fetch_optional(&self.db.read).await?;
        match kind.as_deref() {
            Some("album") => self.refresh_album(id).await,
            Some("artist") => {
                let albums: Vec<i64> = sqlx::query_scalar(
                    "SELECT id FROM items WHERE parent_id = ? AND kind = 'album' AND metadata_locked = 0",
                )
                .bind(id)
                .fetch_all(&self.db.read)
                .await?;
                for album in albums {
                    match self.refresh_album(album).await {
                        Ok(()) | Err(ProviderError::NotFound) => {}
                        Err(err) => return Err(err),
                    }
                }
                self.refresh_artist(id).await
            }
            _ => Ok(()),
        }
    }

    async fn refresh_album(&self, id: i64) -> Result<(), ProviderError> {
        let album: AlbumRow = sqlx::query_as(
            "SELECT a.title, a.year, a.poster, a.musicbrainz_id, ar.id AS artist_id, ar.title AS artist,
                    ar.musicbrainz_id AS artist_mbid
             FROM items a LEFT JOIN items ar ON ar.id = a.parent_id WHERE a.id = ? AND a.kind = 'album'",
        )
        .bind(id)
        .fetch_optional(&self.db.read)
        .await?
        .ok_or(ProviderError::NotFound)?;
        if UNKNOWN.contains(&album.title.as_str()) {
            return Err(ProviderError::NotFound);
        }
        let files = self.file_ids("parent_id", id).await?;
        let known = album.musicbrainz_id.as_deref().filter(|m| is_mbid(m)).map(str::to_owned);
        let found = if let Some(group) = known.or(files.release_group) {
            self.release_group(&group).await?
        } else if let Some(release) = files.release {
            self.release(&release).await?
        } else {
            let artist = album.artist.as_deref().filter(|a| !UNKNOWN.contains(a));
            let artist_id = album.artist_mbid.as_deref().filter(|m| is_mbid(m)).or(files.album_artist.as_deref());
            self.search_album(&album.title, artist, artist_id, album.year).await?.ok_or(ProviderError::NotFound)?
        };

        // Covers in the files or next to them win.
        let cover = format!("{}/release-group/{}/front-500", self.cover_art, found.id);
        let poster = if files.cover { None } else { artwork(&self.store, album.poster.as_deref(), Some(&cover)).await };
        let genres = (!found.genres.is_empty()).then(|| serde_json::to_string(&found.genres).unwrap_or_default());
        let now = now();
        sqlx::query(
            "UPDATE items SET musicbrainz_id = ?, year = COALESCE(year, ?), release_date = COALESCE(?, release_date),
                genres = COALESCE(genres, ?), poster = COALESCE(?, poster), metadata_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&found.id)
        .bind(year_of(found.first_release.as_deref()))
        .bind(full_date(found.first_release.as_deref()))
        .bind(genres)
        .bind(poster)
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&self.db.write)
        .await?;
        // The album's artist, when MusicBrainz credits it under our name.
        if let (Some(artist_id), Some(name), Some((mbid, names))) = (album.artist_id, &album.artist, &found.artist)
            && names.iter().any(|n| normalize_key(n) == normalize_key(name))
        {
            sqlx::query("UPDATE items SET musicbrainz_id = ? WHERE id = ? AND musicbrainz_id IS NULL")
                .bind(mbid)
                .bind(artist_id)
                .execute(&self.db.write)
                .await?;
        }
        Ok(())
    }

    async fn refresh_artist(&self, id: i64) -> Result<(), ProviderError> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT title, musicbrainz_id FROM items WHERE id = ? AND kind = 'artist'")
                .bind(id)
                .fetch_optional(&self.db.read)
                .await?;
        let (name, known) = row.ok_or(ProviderError::NotFound)?;
        if UNKNOWN.contains(&name.as_str()) {
            return Err(ProviderError::NotFound);
        }
        let mbid = match known.filter(|m| is_mbid(m)) {
            Some(mbid) => Some(mbid),
            None => self.file_ids("grandparent_id", id).await?.album_artist,
        };
        let (mbid, genres) = match mbid {
            Some(mbid) => {
                let artist: Artist = self.get(&format!("/artist/{mbid}?inc=genres&fmt=json")).await?;
                (artist.id, top_genres(&artist.genres, None))
            }
            None => {
                let query = format!("artist:{}", quote(&name));
                let found: ArtistSearch = self.search("artist", &query).await?;
                let artist = best_artist(&found.artists, &name).ok_or(ProviderError::NotFound)?;
                let known = self.known_genres().await;
                (artist.id.clone(), known.map(|k| top_genres(&artist.tags, Some(k))).unwrap_or_default())
            }
        };
        let genres = (!genres.is_empty()).then(|| serde_json::to_string(&genres).unwrap_or_default());
        let now = now();
        sqlx::query(
            "UPDATE items SET musicbrainz_id = ?, genres = COALESCE(genres, ?), metadata_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&mbid)
        .bind(genres)
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    async fn release_group(&self, mbid: &str) -> Result<AlbumMatch, ProviderError> {
        let group: ReleaseGroup =
            self.get(&format!("/release-group/{mbid}?inc=genres+artist-credits&fmt=json")).await?;
        let genres = top_genres(&group.genres, None);
        Ok(AlbumMatch::new(group, genres))
    }

    async fn release(&self, mbid: &str) -> Result<AlbumMatch, ProviderError> {
        let release: Release =
            self.get(&format!("/release/{mbid}?inc=release-groups+genres+artist-credits&fmt=json")).await?;
        let mut group = release.release_group.ok_or(ProviderError::NotFound)?;
        if group.artist_credit.is_empty() {
            group.artist_credit = release.artist_credit;
        }
        let genres = top_genres(&group.genres, None);
        Ok(AlbumMatch::new(group, genres))
    }

    async fn search_album(
        &self,
        title: &str,
        artist: Option<&str>,
        artist_id: Option<&str>,
        year: Option<i64>,
    ) -> Result<Option<AlbumMatch>, ProviderError> {
        let mut query = format!("releasegroup:{}", quote(title));
        match (artist_id, artist) {
            (Some(id), _) => query.push_str(&format!(" AND arid:{id}")),
            (None, Some(name)) => query.push_str(&format!(" AND artist:{}", quote(name))),
            (None, None) => {}
        }
        let found: GroupSearch = self.search("release-group", &query).await?;
        let Some(group) = best_release_group(found.release_groups, title, year) else { return Ok(None) };
        let known = self.known_genres().await;
        let genres = known.map(|k| top_genres(&group.tags, Some(k))).unwrap_or_default();
        Ok(Some(AlbumMatch::new(group, genres)))
    }

    /// Whether MusicBrainz answers a search.
    pub async fn check(&self) -> Result<(), ProviderError> {
        self.search::<serde::de::IgnoredAny>("release-group", "pley").await.map(|_| ())
    }

    async fn get<T: DeserializeOwned>(&self, path_and_query: &str) -> Result<T, ProviderError> {
        let request = self.api.http.get(format!("{}{path_and_query}", self.base)).header(ACCEPT, "application/json");
        self.api.json(request).await
    }

    async fn search<T: DeserializeOwned>(&self, entity: &str, query: &str) -> Result<T, ProviderError> {
        let request = self
            .api
            .http
            .get(format!("{}/{entity}", self.base))
            .query(&[("query", query), ("fmt", "json"), ("limit", "10")])
            .header(ACCEPT, "application/json");
        self.api.json(request).await
    }

    /// MusicBrainz's genre list, fetched once; `None` while it can't be had.
    async fn known_genres(&self) -> Option<&HashSet<String>> {
        let fetched = self
            .genres
            .get_or_try_init(|| async {
                let request = self.api.http.get(format!("{}/genre/all?fmt=txt", self.base));
                let body = String::from_utf8_lossy(&self.api.body(request).await?).into_owned();
                let list: HashSet<String> =
                    body.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect();
                Ok::<_, ProviderError>(list)
            })
            .await;
        match fetched {
            Ok(list) => Some(list),
            Err(err) => {
                tracing::debug!("MusicBrainz genre list: {err:#}");
                None
            }
        }
    }

    /// MusicBrainz ids in the tags of an album's (`parent_id`) or an
    /// artist's (`grandparent_id`) tracks, and whether they embed a cover.
    async fn file_ids(&self, parent: &'static str, id: i64) -> sqlx::Result<FileIds> {
        // `parent` is one of two column names.
        let infos: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT f.media_info FROM media_files f JOIN items t ON t.id = f.item_id
             WHERE t.{parent} = ? AND f.media_info IS NOT NULL ORDER BY t.disc, t.idx LIMIT 5"
        )))
        .bind(id)
        .fetch_all(&self.db.read)
        .await?;
        let mut out = FileIds::default();
        for info in infos {
            let Ok(info) = serde_json::from_str::<TrackInfo>(&info) else { continue };
            let tag = |keys: &[&str]| keys.iter().find_map(|k| info.tags.get(*k)).and_then(|v| mbid(v));
            out.cover |= info.has_cover_art;
            out.release_group =
                out.release_group.or_else(|| tag(&["musicbrainz_releasegroupid", "musicbrainz release group id"]));
            out.release = out.release.or_else(|| tag(&["musicbrainz_albumid", "musicbrainz album id"]));
            out.album_artist =
                out.album_artist.or_else(|| tag(&["musicbrainz_albumartistid", "musicbrainz album artist id"]));
        }
        Ok(out)
    }
}

impl AlbumMatch {
    fn new(group: ReleaseGroup, genres: Vec<String>) -> Self {
        let artist = group.artist_credit.into_iter().next().and_then(|c| {
            let artist = c.artist?;
            let names = c.name.into_iter().chain(std::iter::once(artist.name)).collect();
            is_mbid(&artist.id).then_some((artist.id, names))
        });
        Self { id: group.id, first_release: group.first_release_date, genres, artist }
    }
}

/// A Lucene phrase.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `8-4-4-4-12` lowercase or uppercase hex.
fn is_mbid(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 36
        && b.iter()
            .enumerate()
            .all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { *c == b'-' } else { c.is_ascii_hexdigit() })
}

/// The first MusicBrainz id in a tag (tags can hold several: "id1/id2").
fn mbid(value: &str) -> Option<String> {
    value.split(|c: char| !(c.is_ascii_hexdigit() || c == '-')).find(|v| is_mbid(v)).map(str::to_ascii_lowercase)
}

/// The best rated genres (tags that are genres, for search results) as
/// display names.
fn top_genres(tags: &[Tag], known: Option<&HashSet<String>>) -> Vec<String> {
    let mut list: Vec<&Tag> = tags
        .iter()
        .filter(|t| t.count.unwrap_or(0) > 0 && known.is_none_or(|k| k.contains(&t.name.trim().to_lowercase())))
        .collect();
    list.sort_by_key(|t| std::cmp::Reverse(t.count.unwrap_or(0)));
    list.into_iter().take(MAX_GENRES).map(|t| genre_name(t.name.trim())).collect()
}

/// "french house" → "French House", "r&b" → "R&B", "drum and bass" → "Drum and Bass".
fn genre_name(name: &str) -> String {
    const SMALL: [&str; 7] = ["and", "of", "the", "n", "de", "la", "y"];
    const UPPER: [&str; 7] = ["uk", "us", "edm", "idm", "ebm", "dj", "mpb"];
    name.split(' ')
        .enumerate()
        .map(|(i, word)| {
            if i > 0 && SMALL.contains(&word) {
                return word.to_owned();
            }
            if UPPER.contains(&word) {
                return word.to_uppercase();
            }
            let mut out = String::with_capacity(word.len());
            let mut upper = true;
            for c in word.chars() {
                if upper && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    upper = false;
                } else {
                    out.push(c);
                }
                if matches!(c, '-' | '/' | '&') {
                    upper = true;
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The artist called exactly `name` (or with it as an alias) that
/// MusicBrainz ranks best.
fn best_artist<'a>(artists: &'a [Artist], name: &str) -> Option<&'a Artist> {
    let wanted = normalize_key(name);
    artists
        .iter()
        .filter(|a| {
            let names = std::iter::once(a.name.as_str())
                .chain(a.sort_name.as_deref())
                .chain(a.aliases.iter().map(|al| al.name.as_str()));
            !a.id.is_empty() && names.into_iter().any(|n| normalize_key(n) == wanted)
        })
        .reduce(|best, a| if a.score.unwrap_or(0) > best.score.unwrap_or(0) { a } else { best })
}

/// The release group whose title and year fit best: plain albums before
/// remixes, live albums and compilations, MusicBrainz's score breaking ties.
fn best_release_group(groups: Vec<ReleaseGroup>, title: &str, year: Option<i64>) -> Option<ReleaseGroup> {
    groups
        .into_iter()
        .filter(|g| is_mbid(&g.id))
        .filter_map(|g| {
            let fit = title_score(title, [g.title.as_str()]);
            let kind = match (g.primary_type.as_deref(), g.secondary_types.is_empty()) {
                (Some("Album"), true) => 0.3,
                (_, true) => 0.1,
                _ => -0.5,
            };
            let score = fit
                + year_score(year, year_of(g.first_release_date.as_deref()))
                + kind
                + g.score.unwrap_or(0) as f64 / 200.0;
            (fit > 0.0).then_some((g, score))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(g, _)| g)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTIST_SEARCH: &str = include_str!("../../tests/fixtures/providers/musicbrainz_artist_search.json");
    const ARTIST: &str = include_str!("../../tests/fixtures/providers/musicbrainz_artist.json");
    const GROUP_SEARCH: &str = include_str!("../../tests/fixtures/providers/musicbrainz_release_group_search.json");
    const GROUP: &str = include_str!("../../tests/fixtures/providers/musicbrainz_release_group.json");
    const RELEASE: &str = include_str!("../../tests/fixtures/providers/musicbrainz_release.json");
    const GENRES: &str = include_str!("../../tests/fixtures/providers/musicbrainz_genres.txt");

    const DISCOVERY: &str = "48117b90-a16e-34ca-a514-19c702df1158";
    const DAFT_PUNK: &str = "056e4f3e-d505-4dad-8ec1-d04f521cbb56";

    fn known() -> HashSet<String> {
        GENRES.lines().map(str::to_owned).collect()
    }

    #[test]
    fn album_search_picks_the_album_and_its_genres() {
        let found: GroupSearch = serde_json::from_str(GROUP_SEARCH).unwrap();
        let group = best_release_group(found.release_groups, "Discovery", Some(2001)).unwrap();
        assert_eq!(group.id, DISCOVERY);
        let genres = top_genres(&group.tags, Some(&known()));
        // "club/dance" is a tag, not a genre.
        assert_eq!(genres, ["House", "Electronic", "French House", "Progressive House", "Dance"]);
        let album = AlbumMatch::new(group, genres);
        assert_eq!(album.first_release.as_deref(), Some("2001-02-26"));
        assert_eq!(album.artist, Some((DAFT_PUNK.to_owned(), vec!["Daft Punk".to_owned(), "Daft Punk".to_owned()])));

        let found: GroupSearch = serde_json::from_str(GROUP_SEARCH).unwrap();
        assert_eq!(best_release_group(found.release_groups, "Re-Discovery", None).unwrap().title, "Re-Discovery");
        let found: GroupSearch = serde_json::from_str(GROUP_SEARCH).unwrap();
        assert!(best_release_group(found.release_groups, "Homework", None).is_none());
    }

    #[test]
    fn lookups_use_curated_genres() {
        let group: ReleaseGroup = serde_json::from_str(GROUP).unwrap();
        let genres = top_genres(&group.genres, None);
        assert_eq!(genres, ["House", "Electronic", "French House", "Progressive House", "Dance"]);
        let album = AlbumMatch::new(group, genres);
        assert_eq!((album.id.as_str(), album.first_release.as_deref()), (DISCOVERY, Some("2001-02-26")));

        let artist: Artist = serde_json::from_str(ARTIST).unwrap();
        assert_eq!(top_genres(&artist.genres, None), ["Electronic", "House", "French House", "Dance", "Disco"]);

        let release: Release = serde_json::from_str(RELEASE).unwrap();
        let group = release.release_group.unwrap();
        assert_eq!(group.id, DISCOVERY);
        assert_eq!(group.first_release_date.as_deref(), Some("2001-02-26"));
    }

    #[test]
    fn artist_search_wants_the_exact_name() {
        let found: ArtistSearch = serde_json::from_str(ARTIST_SEARCH).unwrap();
        let artist = best_artist(&found.artists, "Daft Punk").unwrap();
        assert_eq!(artist.id, DAFT_PUNK);
        assert_eq!(best_artist(&found.artists, "daft-punk").unwrap().id, DAFT_PUNK);
        // Aliases count.
        assert_eq!(best_artist(&found.artists, "DaftPunk").unwrap().id, DAFT_PUNK);
        assert!(best_artist(&found.artists, "Daft").is_none());
        let genres = top_genres(&artist.tags, Some(&known()));
        assert_eq!(genres.first().map(String::as_str), Some("Electronic"));
    }

    #[test]
    fn ids_in_tags() {
        assert_eq!(mbid(DAFT_PUNK).as_deref(), Some(DAFT_PUNK));
        assert_eq!(mbid(&format!("{}; {DAFT_PUNK}", DISCOVERY.to_uppercase())).as_deref(), Some(DISCOVERY));
        assert_eq!(mbid("not an id"), None);
        assert!(!is_mbid("48117b90-a16e-34ca-a514-19c702df115"));
        assert_eq!(quote(r#"Say "Hi" \o/"#), r#""Say \"Hi\" \\o/""#);
    }

    #[test]
    fn genre_names() {
        assert_eq!(genre_name("french house"), "French House");
        assert_eq!(genre_name("r&b"), "R&B");
        assert_eq!(genre_name("drum and bass"), "Drum and Bass");
        assert_eq!(genre_name("synth-pop"), "Synth-Pop");
        assert_eq!(genre_name("uk garage"), "UK Garage");
        assert_eq!(genre_name("2 tone"), "2 Tone");
    }
}
