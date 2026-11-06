//! Pley's items as Jellyfin's `BaseItemDto`s, with their user data, media
//! sources and streams. Field names, enum values and the fields clients
//! can't do without follow Jellyfin 10.10's OpenAPI description, which the
//! apps' generated models are made from: an unknown enum value or a missing
//! required field fails a whole list in them.

use std::collections::{BTreeMap, HashMap};

use pley_transcoder_api::{DynamicRange, MediaStream as Probed, StreamKind};
use serde::Serialize;

use super::Jellyfin;
use super::ids::Kind;
use super::users::{iso, tag_of};
use crate::auth::AuthUser;
use crate::catalog::{self, FileRow, ItemRow};
use crate::db::Db;
use crate::library::{LibraryKind, LibraryRow};
use crate::people::{Credit, Role};

/// Jellyfin's ticks: 100 ns.
pub const TICKS_PER_MS: i64 = 10_000;

/// What a list asked for beyond the basics (`Fields=`), and the details.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fields {
    pub overview: bool,
    pub genres: bool,
    pub people: bool,
    pub studios: bool,
    pub provider_ids: bool,
    pub media_sources: bool,
    pub chapters: bool,
    pub taglines: bool,
    pub original_title: bool,
}

impl Fields {
    /// `Fields=Overview,Genres,…` (or `Fields` repeated), in any case.
    pub fn of(q: &super::Q) -> Fields {
        let mut fields = Fields::default();
        for name in q.list("fields") {
            match name.to_ascii_lowercase().as_str() {
                "overview" => fields.overview = true,
                "genres" => fields.genres = true,
                "people" => fields.people = true,
                "studios" => fields.studios = true,
                "providerids" => fields.provider_ids = true,
                "mediasources" | "mediastreams" => fields.media_sources = true,
                "chapters" => fields.chapters = true,
                "taglines" => fields.taglines = true,
                "originaltitle" => fields.original_title = true,
                _ => {}
            }
        }
        fields
    }

    /// An item's details: everything.
    pub fn all() -> Fields {
        Fields {
            overview: true,
            genres: true,
            people: true,
            studios: true,
            provider_ids: true,
            media_sources: true,
            chapters: true,
            taglines: true,
            original_title: true,
        }
    }
}

/// Jellyfin's kind of a Pley item.
pub fn item_type(row: &ItemRow) -> &'static str {
    match row.kind.as_str() {
        "movie" => "Movie",
        "show" => "Series",
        "season" => "Season",
        "episode" => "Episode",
        "artist" => "MusicArtist",
        "album" => "MusicAlbum",
        "track" => "Audio",
        "extra" if row.extra_type.as_deref() == Some("trailer") => "Trailer",
        _ => "Video",
    }
}

/// Jellyfin's extra types of Pley's.
fn extra_type(kind: &str) -> &'static str {
    match kind {
        "trailer" => "Trailer",
        "deleted_scene" => "DeletedScene",
        "interview" => "Interview",
        "behind_the_scenes" => "BehindTheScenes",
        "scene" => "Scene",
        "featurette" => "Featurette",
        "short" => "Short",
        _ => "Clip",
    }
}

/// Pley's item kinds of Jellyfin's (`IncludeItemTypes`).
pub fn kind_of_type(name: &str) -> Option<&'static str> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "movie" => "movie",
        "series" => "show",
        "season" => "season",
        "episode" => "episode",
        "musicartist" => "artist",
        "musicalbum" => "album",
        "audio" => "track",
        _ => return None,
    })
}

/// A library's collection type.
pub fn collection_type(lib: &LibraryRow) -> &'static str {
    match lib.kind() {
        LibraryKind::Movies => "movies",
        LibraryKind::Shows => "tvshows",
        LibraryKind::Music => "music",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserData {
    pub playback_position_ticks: i64,
    pub play_count: i64,
    pub is_favorite: bool,
    pub played: bool,
    pub key: String,
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unplayed_item_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    /// A rating as Jellyfin's thumbs: up from 5 of Pley's 10 points.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likes: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NameId {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Person {
    pub name: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(rename = "Type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Chapter {
    pub start_position_ticks: i64,
    pub name: String,
    pub image_date_modified: String,
}

/// An item as Jellyfin lists it. Fields without a value are left out.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Item {
    pub name: String,
    pub server_id: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<String>,
    pub can_delete: bool,
    pub can_download: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiere_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taglines: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genres: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre_items: Option<Vec<NameId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studios: Option<Vec<NameId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub people: Option<Vec<Person>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_ids: Option<BTreeMap<&'static str, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    pub is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// The entry of a playlist the item is listed as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_item_id: Option<String>,
    #[serde(rename = "Type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_type: Option<&'static str>,
    pub media_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_access: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artists: Option<Vec<NameId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artists: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_items: Option<Vec<NameId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive_item_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_trailer_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_feature_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserData>,
    pub image_tags: BTreeMap<&'static str, String>,
    pub backdrop_image_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_backdrop_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_backdrop_image_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thumb_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thumb_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_aspect_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_subtitles: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_sources: Option<Vec<MediaSource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_streams: Option<Vec<Stream>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapters: Option<Vec<Chapter>>,
    /// An item's details: its labels, which Pley's editor keeps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// The sort title, while an edit keeps it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forced_sort_name: Option<String>,
    /// The fields edits keep, as Jellyfin names them ([`super::editing`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_fields: Option<Vec<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_data: Option<bool>,
}

impl Item {
    /// A library, as the view clients list it.
    pub fn library(jellyfin: &Jellyfin, lib: &LibraryRow) -> Item {
        Item {
            name: lib.name.clone(),
            server_id: jellyfin.server_id.clone(),
            id: jellyfin.ids.of(Kind::Library, lib.id),
            etag: Some(format!("{:x}", lib.id)),
            sort_name: Some(lib.name.to_lowercase()),
            is_folder: true,
            kind: "CollectionFolder",
            collection_type: Some(collection_type(lib)),
            media_type: "Unknown",
            location_type: Some("FileSystem"),
            play_access: Some("Full"),
            ..Default::default()
        }
    }
}

/// Columns of items that [`ItemRow`] doesn't carry.
#[derive(Debug, Default, sqlx::FromRow)]
struct ItemExtras {
    id: i64,
    original_title: Option<String>,
    tagline: Option<String>,
    genres: Option<String>,
    studio: Option<String>,
    content_rating: Option<String>,
    tmdb_id: Option<i64>,
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
    musicbrainz_id: Option<String>,
    summary: Option<String>,
}

async fn load_extras(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, ItemExtras>> {
    let mut out = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT id, original_title, tagline, genres, studio, content_rating, tmdb_id, imdb_id, tvdb_id,
                    musicbrainz_id, summary
             FROM items WHERE id IN ({})",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, ItemExtras>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for row in query.fetch_all(&db.read).await? {
            out.insert(row.id, row);
        }
    }
    Ok(out)
}

/// What a list of items needs besides the rows, loaded once for all of them.
pub struct Batch {
    extras: HashMap<i64, ItemExtras>,
    files: HashMap<i64, Vec<FileRow>>,
    people: HashMap<i64, Vec<Credit>>,
    favorites: std::collections::HashSet<i64>,
    extras_count: HashMap<i64, (i64, i64)>,
    fields: Fields,
    /// The token for other apps the list is for, which its media sources'
    /// ids stand for ([`super::ids::Ids::source`]).
    token_id: i64,
}

impl Batch {
    pub async fn load(db: &Db, user: &AuthUser, rows: &[ItemRow], fields: Fields) -> sqlx::Result<Batch> {
        let user_id = user.id;
        let token_id = user.compatibility.map(|c| c.token_id).unwrap_or(0);
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        // Ratings and genres go in every list.
        let extras = load_extras(db, &ids).await?;
        let leaves: Vec<i64> = rows.iter().filter(|r| is_playable(&r.kind)).map(|r| r.id).collect();
        // Runtimes and sizes come from the files, so lists load them too.
        let files = catalog::files_for_items(db, &leaves).await?;
        let people = if fields.people { crate::people::for_items(db, &ids).await? } else { HashMap::new() };
        let favorites = favorites(db, user_id, &ids).await?;
        let titles: Vec<i64> =
            rows.iter().filter(|r| matches!(r.kind.as_str(), "movie" | "show")).map(|r| r.id).collect();
        let extras_count = if fields.media_sources { count_extras(db, &titles).await? } else { HashMap::new() };
        Ok(Batch { extras, files, people, favorites, extras_count, fields, token_id })
    }

    /// The first file of an item, which plays unless a client picks another.
    pub fn files(&self, id: i64) -> &[FileRow] {
        self.files.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Which of `ids` are in the user's My List, Jellyfin's favorites.
pub(super) async fn favorites(db: &Db, user_id: i64, ids: &[i64]) -> sqlx::Result<std::collections::HashSet<i64>> {
    let mut out = std::collections::HashSet::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT item_id FROM watchlist WHERE user_id = ? AND item_id IN ({})",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
        for id in chunk {
            query = query.bind(id);
        }
        out.extend(query.fetch_all(&db.read).await?);
    }
    Ok(out)
}

/// Trailers and other bonus material of each title: `(trailers, others)`.
async fn count_extras(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, (i64, i64)>> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT parent_id, SUM(extra_type = 'trailer'), SUM(extra_type != 'trailer') FROM items
             WHERE kind = 'extra' AND parent_id IN ({}) GROUP BY parent_id",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, (i64, i64, i64)>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for (id, trailers, others) in query.fetch_all(&db.read).await? {
            out.insert(id, (trailers, others));
        }
    }
    Ok(out)
}

pub fn is_playable(kind: &str) -> bool {
    matches!(kind, "movie" | "episode" | "track" | "extra")
}

/// The version of an item's artwork: its `updated_at`, which changes when
/// the artwork does.
fn tag(version: i64) -> String {
    format!("{version:x}")
}

pub fn build(jellyfin: &Jellyfin, row: &ItemRow, batch: &Batch) -> Item {
    let ids = &jellyfin.ids;
    let fields = batch.fields;
    let kind = row.kind.as_str();
    let id = ids.of(Kind::Item, row.id);
    let extras = batch.extras.get(&row.id);
    let files = batch.files(row.id);
    let file = files.first();
    let duration = file.and_then(|f| f.duration_ms).or(row.duration_ms);
    let item_id = |n: i64| ids.of(Kind::Item, n);

    let mut item = Item {
        name: row.title.clone(),
        server_id: jellyfin.server_id.clone(),
        id: id.clone(),
        etag: Some(tag(row.updated_at)),
        date_created: Some(iso(row.added_at)),
        can_delete: false,
        can_download: is_playable(kind),
        sort_name: Some(row.sort_title.clone()),
        premiere_date: row.release_date.as_deref().and_then(crate::util::date_secs).map(iso),
        production_year: row.year,
        community_rating: row.rating.map(|r| (r * 10.0).round() / 10.0),
        run_time_ticks: duration.map(|d| d * TICKS_PER_MS),
        is_folder: !is_playable(kind),
        kind: item_type(row),
        media_type: match kind {
            "movie" | "episode" | "extra" => "Video",
            "track" => "Audio",
            _ => "Unknown",
        },
        location_type: Some("FileSystem"),
        video_type: matches!(kind, "movie" | "episode" | "extra").then_some("VideoFile"),
        play_access: Some("Full"),
        extra_type: row.extra_type.as_deref().map(extra_type),
        ..Default::default()
    };

    if let Some(extras) = extras {
        item.official_rating = extras.content_rating.clone();
        if fields.overview {
            item.overview = extras.summary.clone();
        }
        if fields.original_title {
            item.original_title = extras.original_title.clone().filter(|t| *t != row.title);
        }
        if fields.taglines {
            item.taglines = Some(extras.tagline.iter().cloned().collect());
        }
        let genres: Vec<String> =
            extras.genres.as_deref().and_then(|g| serde_json::from_str(g).ok()).unwrap_or_default();
        if fields.genres || !genres.is_empty() {
            item.genre_items = Some(
                genres
                    .iter()
                    .map(|g| NameId { name: g.clone(), id: ids.of(Kind::Genre, super::ids::genre_number(g)) })
                    .collect(),
            );
            item.genres = Some(genres);
        }
        if fields.studios {
            item.studios = Some(
                extras
                    .studio
                    .iter()
                    .map(|s| NameId { name: s.clone(), id: ids.of(Kind::Studio, super::ids::genre_number(s)) })
                    .collect(),
            );
        }
        let mut providers = BTreeMap::new();
        if let Some(tmdb) = extras.tmdb_id {
            providers.insert("Tmdb", tmdb.to_string());
        }
        if let Some(imdb) = &extras.imdb_id {
            providers.insert("Imdb", imdb.clone());
        }
        if let Some(tvdb) = extras.tvdb_id.filter(|_| kind == "show") {
            providers.insert("Tvdb", tvdb.to_string());
        }
        if let Some(mbid) = &extras.musicbrainz_id {
            let key = if kind == "artist" { "MusicBrainzArtist" } else { "MusicBrainzReleaseGroup" };
            providers.insert(key, mbid.clone());
        }
        item.provider_ids = Some(providers);
    }

    // Where it sits.
    match kind {
        "season" => {
            item.index_number = row.idx;
            item.parent_id = row.parent_id.map(item_id);
            item.series_id = row.parent_id.map(item_id);
            item.series_name = row.parent_title.clone();
        }
        "episode" => {
            item.index_number = row.idx;
            item.parent_index_number = row.parent_idx;
            item.parent_id = row.parent_id.map(item_id);
            item.season_id = row.parent_id.map(item_id);
            item.season_name = row.parent_title.clone();
            item.series_id = row.grandparent_id.map(item_id);
            item.series_name = row.grandparent_title.clone();
        }
        "album" => {
            item.parent_id = row.parent_id.map(item_id);
            if let (Some(artist), Some(name)) = (row.parent_id, &row.parent_title) {
                let pair = NameId { name: name.clone(), id: item_id(artist) };
                item.album_artist = Some(name.clone());
                item.album_artists = Some(vec![pair.clone()]);
                item.artist_items = Some(vec![pair]);
                item.artists = Some(vec![name.clone()]);
            }
        }
        "track" => {
            item.index_number = row.idx;
            item.parent_index_number = row.disc.or(Some(1));
            item.parent_id = row.parent_id.map(item_id);
            item.album_id = row.parent_id.map(item_id);
            item.album = row.parent_title.clone();
            if let (Some(artist), Some(name)) = (row.grandparent_id, &row.grandparent_title) {
                let pair = NameId { name: name.clone(), id: item_id(artist) };
                item.album_artist = Some(name.clone());
                item.album_artists = Some(vec![pair.clone()]);
                item.artist_items = Some(vec![pair]);
                item.artists = Some(vec![name.clone()]);
            }
        }
        "extra" => item.parent_id = row.parent_id.map(item_id),
        _ => item.parent_id = Some(ids.of(Kind::Library, row.library_id)),
    }
    if matches!(kind, "show" | "season" | "artist" | "album") {
        item.child_count = row.child_count;
        item.recursive_item_count = row.leaf_count;
    }

    // Artwork: Primary is the poster (an episode's still), Backdrop the
    // backdrop, Thumb a frame of the video.
    let own_tag = tag(row.updated_at);
    match kind {
        "episode" => {
            item.image_tags.insert("Primary", own_tag.clone());
            if row.grandparent_has_poster {
                item.series_primary_image_tag = Some(tag(row.grandparent_updated_at.unwrap_or(0)));
            }
        }
        "track" => {
            if row.parent_id.is_some() {
                item.album_primary_image_tag = Some(tag(row.parent_updated_at.unwrap_or(0)));
            }
        }
        "album" => {
            item.image_tags.insert("Primary", own_tag.clone());
        }
        "season" => {
            if row.has_poster {
                item.image_tags.insert("Primary", own_tag.clone());
            }
            if row.parent_has_poster {
                item.series_primary_image_tag = Some(tag(row.parent_updated_at.unwrap_or(0)));
            }
        }
        _ => {
            if row.has_poster {
                item.image_tags.insert("Primary", own_tag.clone());
            }
            if matches!(kind, "movie" | "extra") {
                item.image_tags.insert("Thumb", own_tag.clone());
            }
        }
    }
    if row.has_backdrop {
        item.backdrop_image_tags.push(own_tag.clone());
    } else if let Some((owner, version)) = backdrop_owner(row) {
        item.parent_backdrop_item_id = Some(item_id(owner));
        item.parent_backdrop_image_tags = Some(vec![tag(version)]);
    }
    if kind == "episode"
        && let Some(show) = row.grandparent_id.filter(|_| row.grandparent_has_backdrop)
    {
        item.parent_thumb_item_id = Some(item_id(show));
        item.parent_thumb_image_tag = Some(tag(row.grandparent_updated_at.unwrap_or(0)));
    }
    item.primary_image_aspect_ratio = Some(match kind {
        "episode" | "extra" => 16.0 / 9.0,
        "artist" | "album" | "track" => 1.0,
        _ => 2.0 / 3.0,
    });

    item.user_data = Some(user_data(jellyfin, row, batch.favorites.contains(&row.id), duration));

    if let Some((trailers, others)) = batch.extras_count.get(&row.id) {
        item.local_trailer_count = Some(*trailers);
        item.special_feature_count = Some(*others);
    }
    if let Some(credits) = batch.people.get(&row.id).filter(|_| fields.people) {
        item.people = Some(credits.iter().map(|c| person(jellyfin, c)).collect());
    }
    if let Some(file) = file {
        item.path = Some(file_name_path(file));
        item.container = file.container.as_deref().map(container_name);
        item.width = file.width;
        item.height = file.height;
        let streams = file.streams();
        item.has_subtitles = Some(streams.iter().any(|s| s.kind == StreamKind::Subtitle));
        if fields.media_sources {
            let sources: Vec<MediaSource> =
                files.iter().map(|f| media_source(jellyfin, row, f, batch.token_id)).collect();
            item.media_streams = sources.first().map(|s| s.media_streams.clone());
            item.media_sources = Some(sources);
        }
        if fields.chapters {
            item.chapters = Some(chapters(file));
        }
    }
    item
}

/// A file's place as clients are shown it: its name alone, as a path
/// (Jellyfin for Kodi splits names from paths). Where the server keeps its
/// files is its administrators' to know.
fn file_name_path(file: &FileRow) -> String {
    let name = std::path::Path::new(&file.path).file_name().map(|n| n.to_string_lossy().into_owned());
    format!("/{}", name.unwrap_or_default())
}

/// Whose backdrop an item without one shows: its parent's or grandparent's.
fn backdrop_owner(row: &ItemRow) -> Option<(i64, i64)> {
    if row.parent_has_backdrop {
        row.parent_id.map(|p| (p, row.parent_updated_at.unwrap_or(0)))
    } else if row.grandparent_has_backdrop {
        row.grandparent_id.map(|g| (g, row.grandparent_updated_at.unwrap_or(0)))
    } else {
        None
    }
}

pub fn user_data(jellyfin: &Jellyfin, row: &ItemRow, favorite: bool, duration_ms: Option<i64>) -> UserData {
    let summary_watched = match row.kind.as_str() {
        "show" | "season" => {
            let leaves = row.leaf_count.unwrap_or(0);
            leaves > 0 && row.viewed_leaf_count.unwrap_or(0) >= leaves
        }
        _ => row.watched,
    };
    let in_progress = row.position_ms > 0 && !row.watched;
    let id = jellyfin.ids.of(Kind::Item, row.id);
    UserData {
        playback_position_ticks: if in_progress { row.position_ms * TICKS_PER_MS } else { 0 },
        play_count: crate::listing::view_count(row),
        is_favorite: favorite,
        played: summary_watched,
        key: id.clone(),
        item_id: id,
        last_played_date: row.last_played_at.map(iso),
        played_percentage: duration_ms
            .filter(|d| *d > 0 && in_progress)
            .map(|d| (row.position_ms as f64 / d as f64 * 100.0).clamp(0.0, 100.0)),
        unplayed_item_count: matches!(row.kind.as_str(), "show" | "season")
            .then(|| (row.leaf_count.unwrap_or(0) - row.viewed_leaf_count.unwrap_or(0)).max(0)),
        rating: row.user_rating,
        likes: row.user_rating.map(|r| r >= 5.0),
    }
}

pub fn person(jellyfin: &Jellyfin, credit: &Credit) -> Person {
    let kind = match credit.role {
        Role::Actor => "Actor",
        Role::Director => "Director",
        Role::Writer => "Writer",
        Role::Producer => "Producer",
        Role::Creator => "Creator",
    };
    Person {
        name: credit.name.clone(),
        id: jellyfin.ids.of(Kind::Person, credit.id),
        role: credit.character.clone(),
        kind,
        primary_image_tag: credit.photo_url.as_deref().map(tag_of),
    }
}

fn chapters(file: &FileRow) -> Vec<Chapter> {
    let Some(info) = file.media_info() else { return Vec::new() };
    info.chapters
        .iter()
        .enumerate()
        .map(|(n, c)| Chapter {
            start_position_ticks: c.start_ms * TICKS_PER_MS,
            name: c.title.clone().unwrap_or_else(|| format!("Chapter {}", n + 1)),
            image_date_modified: iso(0),
        })
        .collect()
}

// ------------------------------------------------------------------ media

/// Jellyfin's container names: ffprobe's first.
pub fn container_name(container: &str) -> String {
    match container {
        c if c.starts_with("matroska") => "mkv".into(),
        c if c.starts_with("mov,mp4") => "mp4".into(),
        "mpegts" => "ts".into(),
        c => c.split(',').next().unwrap_or(c).to_owned(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSource {
    pub protocol: &'static str,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "Type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    pub size: i64,
    pub name: String,
    pub is_remote: bool,
    pub e_tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    pub read_at_native_framerate: bool,
    pub ignore_dts: bool,
    pub ignore_index: bool,
    pub gen_pts_input: bool,
    pub supports_transcoding: bool,
    pub supports_direct_stream: bool,
    pub supports_direct_play: bool,
    pub is_infinite_stream: bool,
    pub use_most_compatible_transcoding_profile: bool,
    pub requires_opening: bool,
    pub requires_closing: bool,
    pub requires_looping: bool,
    pub supports_probing: bool,
    pub has_segments: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_type: Option<&'static str>,
    pub media_streams: Vec<Stream>,
    pub media_attachments: [(); 0],
    pub formats: [(); 0],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    pub required_http_headers: BTreeMap<String, String>,
    pub transcoding_sub_protocol: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_container: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_audio_stream_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_subtitle_stream_index: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Stream {
    #[serde(rename = "Type")]
    pub kind: &'static str,
    pub index: i64,
    pub codec: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub display_title: String,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub is_original: bool,
    pub is_interlaced: bool,
    pub is_external: bool,
    pub is_text_subtitle_stream: bool,
    pub supports_external_stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    pub video_range: &'static str,
    pub video_range_type: &'static str,
    pub audio_spatial_format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_profile: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_level: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_bl_signal_compatibility_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_method: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_url: Option<String>,
}

/// The codec names Jellyfin's clients use: ffprobe's, but a few.
fn codec_name(codec: &str) -> String {
    match codec {
        "subrip" => "srt".into(),
        "hdmv_pgs_subtitle" => "pgssub".into(),
        "dvd_subtitle" => "dvdsub".into(),
        "dvb_subtitle" => "dvbsub".into(),
        "webvtt" => "webvtt".into(),
        "mov_text" => "mov_text".into(),
        c => c.to_ascii_lowercase(),
    }
}

/// The file extension a text subtitle is served with: SubRip as it is, the
/// rest as the WebVTT Pley makes of them (ASS and SSA too, their styles
/// left out), as Jellyfin's address names what it delivers.
pub fn subtitle_format(codec: &str) -> &'static str {
    match codec {
        "subrip" | "srt" => "srt",
        _ => "vtt",
    }
}

fn stream_title(s: &Probed) -> String {
    let language = s.language.as_deref().and_then(crate::languages::language).map(|l| l.2);
    let mut parts: Vec<String> = Vec::new();
    match s.kind {
        StreamKind::Video => {
            if let Some(height) = s.height {
                parts.push(match height {
                    h if h >= 2000 => "4K".into(),
                    h if h >= 1000 => "1080p".into(),
                    h if h >= 700 => "720p".into(),
                    h => format!("{h}p"),
                });
            }
            parts.push(codec_name(&s.codec).to_uppercase());
            if s.dynamic_range().is_hdr() {
                parts.push(s.dynamic_range_label());
            }
        }
        StreamKind::Audio => {
            parts.push(language.unwrap_or("Unknown").to_owned());
            parts.push(codec_name(&s.codec).to_uppercase());
            if let Some(channels) = s.channels {
                parts.push(match channels {
                    1 => "Mono".into(),
                    2 => "Stereo".into(),
                    6 => "5.1".into(),
                    8 => "7.1".into(),
                    n => format!("{n} ch"),
                });
            }
        }
        StreamKind::Subtitle => {
            parts.push(language.unwrap_or("Unknown").to_owned());
            parts.push(codec_name(&s.codec).to_uppercase());
            if s.forced {
                parts.push("Forced".into());
            }
        }
    }
    let base = parts.join(" - ");
    match &s.title {
        Some(title) if !title.trim().is_empty() && s.kind != StreamKind::Video => format!("{title} - {base}"),
        _ => base,
    }
}

pub fn stream(jellyfin: &Jellyfin, row: &ItemRow, source: &str, s: &Probed) -> Stream {
    let text = s.kind == StreamKind::Subtitle
        && s.text_based
            .unwrap_or(matches!(s.codec.as_str(), "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "text"));
    let range = if s.dynamic_range() == DynamicRange::Sdr { "SDR" } else { "HDR" };
    let range_type = super::profile::range_type(s);
    let kind = match s.kind {
        StreamKind::Video => "Video",
        StreamKind::Audio => "Audio",
        StreamKind::Subtitle => "Subtitle",
    };
    // Text subtitles are files of their own a player loads; picture ones
    // play from the file, or are burned in.
    let delivery = (s.kind == StreamKind::Subtitle).then_some(if text { "External" } else { "Embed" });
    let delivery_url = (s.kind == StreamKind::Subtitle && text).then(|| {
        format!(
            "/Videos/{}/{}/Subtitles/{}/Stream.{}",
            jellyfin.ids.of(Kind::Item, row.id),
            source,
            s.index,
            subtitle_format(&s.codec)
        )
    });
    let lang = s.language.as_deref().and_then(crate::languages::language).map(|l| l.0.to_owned());
    Stream {
        kind,
        index: s.index,
        codec: codec_name(&s.codec),
        language: lang.or_else(|| s.language.clone()),
        title: s.title.clone(),
        display_title: stream_title(s),
        is_default: s.default,
        is_forced: s.forced,
        is_hearing_impaired: s.title.as_deref().is_some_and(|t| {
            let t = t.to_lowercase();
            t.contains("sdh") || t.contains("hearing")
        }),
        is_original: false,
        is_interlaced: s.interlaced.unwrap_or(false),
        is_external: s.external,
        is_text_subtitle_stream: text,
        supports_external_stream: text,
        profile: s.profile.clone(),
        level: s.level.map(|l| l as f64),
        width: s.width,
        height: s.height,
        bit_rate: s.bitrate,
        bit_depth: s.bit_depth,
        real_frame_rate: s.frame_rate,
        average_frame_rate: s.frame_rate,
        aspect_ratio: None,
        pixel_format: s.pix_fmt.clone(),
        color_transfer: s.color_transfer.clone(),
        color_primaries: s.color_primaries.clone(),
        color_space: s.color_space.clone(),
        video_range: if s.kind == StreamKind::Video { range } else { "Unknown" },
        video_range_type: if s.kind == StreamKind::Video { range_type } else { "Unknown" },
        audio_spatial_format: "None",
        dv_profile: s.dv_profile,
        dv_level: s.dv_level,
        dv_bl_signal_compatibility_id: s.dv_bl_compat,
        channels: s.channels,
        channel_layout: s.channel_layout.clone(),
        sample_rate: s.sample_rate,
        delivery_method: delivery,
        delivery_url,
    }
}

/// A file of an item as a media source, played as it is unless a client's
/// device profile says otherwise ([`super::playback`]).
pub fn media_source(jellyfin: &Jellyfin, row: &ItemRow, file: &FileRow, token_id: i64) -> MediaSource {
    let id = jellyfin.ids.source(file.id, token_id);
    let streams = file.streams();
    let default_audio = streams
        .iter()
        .find(|s| s.kind == StreamKind::Audio && s.default)
        .or_else(|| streams.iter().find(|s| s.kind == StreamKind::Audio))
        .map(|s| s.index);
    // Versions are named by their edition, as Jellyfin names them, else by
    // their file.
    let path = std::path::Path::new(&file.path);
    let name = crate::library::parse::edition(path)
        .or_else(|| path.file_stem().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| row.title.clone());
    MediaSource {
        protocol: "File",
        path: Some(file_name_path(file)),
        kind: "Default",
        container: file.container.as_deref().map(container_name),
        size: file.size,
        name,
        is_remote: false,
        e_tag: file.fingerprint.clone().unwrap_or_else(|| format!("{:x}", file.mtime)),
        run_time_ticks: file.duration_ms.or(row.duration_ms).map(|d| d * TICKS_PER_MS),
        read_at_native_framerate: false,
        ignore_dts: false,
        ignore_index: false,
        gen_pts_input: false,
        supports_transcoding: true,
        supports_direct_stream: true,
        supports_direct_play: true,
        is_infinite_stream: false,
        use_most_compatible_transcoding_profile: false,
        requires_opening: false,
        requires_closing: false,
        requires_looping: false,
        supports_probing: true,
        has_segments: false,
        video_type: (row.kind != "track").then_some("VideoFile"),
        media_streams: streams.iter().map(|s| stream(jellyfin, row, &id, s)).collect(),
        id,
        media_attachments: [],
        formats: [],
        bitrate: file.bitrate,
        required_http_headers: BTreeMap::new(),
        transcoding_sub_protocol: "http",
        transcoding_url: None,
        transcoding_container: None,
        default_audio_stream_index: default_audio,
        default_subtitle_stream_index: None,
    }
}
