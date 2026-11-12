//! Read side of the library: item queries, watch state, hubs, search, and the
//! JSON shapes returned by the API (see docs/API.md).

use std::collections::{HashMap, HashSet};

use futures::TryStreamExt;
use pley_transcoder_api::{MediaInfo, MediaStream};
use serde::Serialize;
use sqlx::FromRow;

use crate::db::Db;
use crate::events::{EventBus, ServerEvent};
use crate::library::sidecar::ExternalSub;
use crate::subtitles::AddedSub;
use crate::util::now;

/// The requesting user and their restrictions. Item queries bind the user id
/// first and refer to it through `me`, so every placeholder can stay a plain
/// `?` (sqlx numbers unnamed placeholders itself, so they must not be mixed
/// with `?NNN`). Administrators have no restrictions.
macro_rules! me_cte {
    () => {
        "WITH me(uid, all_libs, max_age, shows, hides) AS (
            SELECT id, all_libraries OR is_admin, CASE WHEN is_admin THEN NULL ELSE max_age END,
                   NOT is_admin AND EXISTS (SELECT 1 FROM user_label_rules r WHERE r.user_id = users.id AND r.kind = 'show'),
                   NOT is_admin AND EXISTS (SELECT 1 FROM user_label_rules r WHERE r.user_id = users.id AND r.kind = 'hide')
            FROM users WHERE id = ?)"
    };
}
pub(crate) use me_cte;

/// Whether item `i` (with parent `p` and grandparent `gp`) is visible to the
/// user in `me`: library access, the profile's labels it never sees, and its
/// age limit unless a label it always sees is there, see [`crate::access`].
/// The labels are only looked for when the profile has rules of that kind.
pub(crate) const VISIBLE: &str = "((SELECT all_libs FROM me)
       OR i.library_id IN (SELECT library_id FROM user_libraries WHERE user_id = (SELECT uid FROM me)))
  AND (NOT (SELECT hides FROM me)
       OR NOT EXISTS (SELECT 1 FROM user_label_rules r JOIN item_labels l ON l.label = r.label
                      WHERE r.user_id = (SELECT uid FROM me) AND r.kind = 'hide'
                        AND l.item_id IN (i.id, i.parent_id, i.grandparent_id)))
  AND ((SELECT max_age FROM me) IS NULL
       OR i.kind IN ('artist', 'album', 'track')
       OR NULLIF(MAX(COALESCE(i.rating_age, -1), COALESCE(p.rating_age, -1), COALESCE(gp.rating_age, -1)), -1)
          <= (SELECT max_age FROM me)
       OR ((SELECT shows FROM me)
           AND EXISTS (SELECT 1 FROM user_label_rules r JOIN item_labels l ON l.label = r.label
                       WHERE r.user_id = (SELECT uid FROM me) AND r.kind = 'show'
                         AND l.item_id IN (i.id, i.parent_id, i.grandparent_id))))";

/// Columns of [`ItemRow`]; callers add `WHERE {VISIBLE} AND …`.
/// The `me` CTE that [`VISIBLE`] reads (binds the user id).
pub(crate) const ME_CTE: &str = me_cte!();

const SUMMARY_SELECT: &str = concat!(
    me_cte!(),
    "
SELECT i.id, i.library_id, i.kind, i.title, i.sort_title, i.year, i.idx, i.disc,
       i.parent_id, p.title AS parent_title, p.idx AS parent_idx,
       i.grandparent_id, gp.title AS grandparent_title,
       i.duration_ms, i.rating, i.added_at, i.updated_at, i.summary, i.release_date, i.extra_type,
       i.poster IS NOT NULL AS has_poster, i.backdrop IS NOT NULL AS has_backdrop, i.thumb IS NOT NULL AS has_thumb,
       COALESCE(p.poster IS NOT NULL, 0) AS parent_has_poster, COALESCE(p.backdrop IS NOT NULL, 0) AS parent_has_backdrop,
       p.updated_at AS parent_updated_at,
       COALESCE(gp.poster IS NOT NULL, 0) AS grandparent_has_poster, COALESCE(gp.backdrop IS NOT NULL, 0) AS grandparent_has_backdrop,
       gp.updated_at AS grandparent_updated_at,
       COALESCE(s.position_ms, 0) AS position_ms, COALESCE(s.watched, 0) AS watched,
       COALESCE(s.play_count, 0) AS play_count, s.last_played_at, s.rating AS user_rating,
       i.leaf_count, i.child_count,
       CASE i.kind
         WHEN 'show' THEN (SELECT COUNT(*) FROM items e JOIN user_item_state us ON us.item_id = e.id
                           WHERE e.grandparent_id = i.id AND e.kind = 'episode' AND us.user_id = (SELECT uid FROM me) AND us.watched = 1)
         WHEN 'season' THEN (SELECT COUNT(*) FROM items e JOIN user_item_state us ON us.item_id = e.id
                             WHERE e.parent_id = i.id AND us.user_id = (SELECT uid FROM me) AND us.watched = 1)
         ELSE NULL END AS viewed_leaf_count
FROM items i
LEFT JOIN items p ON p.id = i.parent_id
LEFT JOIN items gp ON gp.id = i.grandparent_id
LEFT JOIN user_item_state s ON s.item_id = i.id AND s.user_id = (SELECT uid FROM me)
"
);

#[derive(Debug, Clone, FromRow)]
pub struct ItemRow {
    pub id: i64,
    pub library_id: i64,
    pub kind: String,
    pub title: String,
    pub sort_title: String,
    pub year: Option<i64>,
    pub idx: Option<i64>,
    pub disc: Option<i64>,
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    pub parent_idx: Option<i64>,
    pub grandparent_id: Option<i64>,
    pub grandparent_title: Option<String>,
    pub duration_ms: Option<i64>,
    pub rating: Option<f64>,
    pub added_at: i64,
    pub updated_at: i64,
    pub summary: Option<String>,
    pub release_date: Option<String>,
    /// Bonus material: `trailer`, `featurette`, … (see `library::walk::ExtraKind`).
    pub extra_type: Option<String>,
    pub has_poster: bool,
    pub has_backdrop: bool,
    pub has_thumb: bool,
    pub parent_has_poster: bool,
    pub parent_has_backdrop: bool,
    pub parent_updated_at: Option<i64>,
    pub grandparent_has_poster: bool,
    pub grandparent_has_backdrop: bool,
    pub grandparent_updated_at: Option<i64>,
    pub position_ms: i64,
    pub watched: bool,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
    pub user_rating: Option<f64>,
    pub leaf_count: Option<i64>,
    pub child_count: Option<i64>,
    pub viewed_leaf_count: Option<i64>,
}

/// Null fields are left out of the JSON: lists of thousands of items are
/// much smaller that way, and clients treat a missing field like `null`.
#[derive(Debug, Clone, Serialize)]
pub struct UserState {
    pub position_ms: i64,
    pub watched: bool,
    pub play_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<i64>,
    /// The user's star rating, 0-10 (two points per star).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemSummary {
    pub id: i64,
    pub library_id: i64,
    pub kind: String,
    pub title: String,
    pub sort_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grandparent_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grandparent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    pub added_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    pub user_state: UserState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewed_leaf_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_type: Option<String>,
    /// What the title's files are like, in library listings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSummary>,
}

/// A title's best file (a show's files on average), for library listings:
/// what the posters' badges say, and the line under them sorted by bitrate.
/// Filtered by resolution or dynamic range, the version that matches: a
/// film's Dolby Vision file, not its higher-bitrate HDR10 one.
#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct MediaSummary {
    /// Bits per second.
    pub bitrate: Option<i64>,
    /// Bytes (a show's: all its episodes').
    pub size: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    /// `sdr`, `hdr10`, `hdr10plus`, `hlg` or `dolby_vision`.
    pub dynamic_range: Option<String>,
}

/// The media summaries of `items` (one listing page), by item id: of the
/// `files` that match, where a title has any.
pub async fn media_summaries(
    db: &Db,
    items: &[ItemSummary],
    files: FileFilter,
) -> sqlx::Result<HashMap<i64, MediaSummary>> {
    let ids = |shows: bool| {
        serde_json::to_string(&items.iter().filter(|i| (i.kind == "show") == shows).map(|i| i.id).collect::<Vec<_>>())
            .unwrap_or_else(|_| "[]".into())
    };
    #[derive(FromRow)]
    struct Row {
        item_id: i64,
        #[sqlx(flatten)]
        media: MediaSummary,
    }
    let matching = files.sql();
    // The file with the highest bitrate of each title comes first, of those
    // that match when some do.
    let movies: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT f.item_id, f.bitrate, f.size, f.width, f.height, f.video_codec, f.dynamic_range
         FROM media_files f WHERE f.item_id IN (SELECT value FROM json_each(?))
         ORDER BY f.item_id, {}f.bitrate IS NULL, f.bitrate DESC",
        // A constant there would name a column.
        matching.as_deref().map(|m| format!("({m}) DESC, ")).unwrap_or_default(),
    )))
    .bind(ids(false))
    .fetch_all(&db.read)
    .await?;
    // A show's best range, as its resolution is its best episode's: of the
    // episodes that match, then (for shows with none) of all of them.
    let shows = |matching: Option<&str>| {
        let query = format!(
            "SELECT e.grandparent_id AS item_id, CAST(AVG(f.bitrate) AS INTEGER) AS bitrate, SUM(f.size) AS size,
                    MAX(f.width) AS width, MAX(f.height) AS height, NULL AS video_codec,
                    {RANGE_OF_FILES} AS dynamic_range
             FROM items e JOIN media_files f ON f.item_id = e.id
             WHERE e.grandparent_id IN (SELECT value FROM json_each(?)) AND ({})
             GROUP BY e.grandparent_id",
            matching.unwrap_or("1"),
        );
        let ids = ids(true);
        async move { sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(query)).bind(ids).fetch_all(&db.read).await }
    };
    let matching_shows = if matching.is_some() { shows(matching.as_deref()).await? } else { Vec::new() };
    let mut out = HashMap::new();
    for row in movies.into_iter().chain(matching_shows).chain(shows(None).await?) {
        out.entry(row.item_id).or_insert(row.media);
    }
    Ok(out)
}

/// The files a filtered listing describes and sorts by: those of its
/// resolutions and dynamic ranges (any, where one is empty).
#[derive(Debug, Clone, Copy, Default)]
pub struct FileFilter {
    pub resolutions: &'static [&'static str],
    pub dynamic_ranges: &'static [&'static str],
}

impl FileFilter {
    /// Whether file `f` matches, as SQL; `None` when every file does. Only
    /// the catalog's own tokens are written into it.
    fn sql(&self) -> Option<String> {
        let list = |values: &[&str], known: &[&str]| {
            values.iter().filter(|v| known.contains(v)).map(|v| format!("'{v}'")).collect::<Vec<_>>().join(", ")
        };
        let mut tests = Vec::new();
        if !self.resolutions.is_empty() {
            let resolution = resolution_case("f.width", "f.height");
            tests.push(format!("COALESCE({resolution} IN ({}), 0)", list(self.resolutions, &RESOLUTIONS)));
        }
        if !self.dynamic_ranges.is_empty() {
            tests.push(format!("COALESCE(f.dynamic_range IN ({}), 0)", list(self.dynamic_ranges, &DYNAMIC_RANGES)));
        }
        (!tests.is_empty()).then(|| tests.join(" AND "))
    }
}

/// The resolutions a library can be filtered by, best first: a title's is
/// its best file's (a show's best episode's), `1080` up to 1440p, `sd`
/// below 720p.
pub const RESOLUTIONS: [&str; 4] = ["4k", "1080", "720", "sd"];

/// The dynamic ranges a title can have, best first: a title's is its best
/// file's (a show's best episode's). `hdr10` names the first three, `hdr`
/// the first four ([`dynamic_ranges_named`]).
pub const DYNAMIC_RANGES: [&str; 5] = ["dolby_vision", "hdr10plus", "hdr10", "hlg", "sdr"];

/// The dynamic ranges a library can be filtered by, as they're offered:
/// from those that keep the most kinds to those that keep one.
const RANGE_CHOICES: [&str; 6] = ["hdr", "hdr10", "dolby_vision", "hdr10plus", "hlg", "sdr"];

/// The resolutions a filter's value names: Pley's, `2160`, and the `576`
/// and `480` of Plex's filters, which are SD here. Nothing for another value.
pub fn resolutions_named(value: &str) -> &'static [&'static str] {
    let value = value.trim().to_ascii_lowercase();
    match value.strip_suffix('p').unwrap_or(&value) {
        "4k" | "2160" => &RESOLUTIONS[0..1],
        "1080" => &RESOLUTIONS[1..2],
        "720" => &RESOLUTIONS[2..3],
        "sd" | "576" | "480" => &RESOLUTIONS[3..4],
        _ => &[],
    }
}

/// The dynamic ranges a filter's value names: `hdr` every one but SDR,
/// `hdr10` (HDR) HDR10 with HDR10+ and Dolby Vision, and the others
/// themselves alone (a Dolby Vision + HDR10+ title is Dolby Vision).
/// Nothing for another value.
pub fn dynamic_ranges_named(value: &str) -> &'static [&'static str] {
    match value.trim().to_ascii_lowercase().as_str() {
        "hdr" => &DYNAMIC_RANGES[..4],
        "hdr10" => &DYNAMIC_RANGES[..3],
        value => DYNAMIC_RANGES.iter().position(|r| *r == value).map_or(&[], |i| &DYNAMIC_RANGES[i..=i]),
    }
}

/// The resolution of a `width` and `height`, with the same lines as the
/// posters' "4K": either is enough, for films cropped wider than 16:9.
fn resolution_case(width: &str, height: &str) -> String {
    format!(
        "(CASE WHEN {width} >= 3800 OR {height} >= 2100 THEN '4k'
            WHEN {width} >= 1800 OR {height} >= 1000 THEN '1080'
            WHEN {width} >= 1200 OR {height} >= 700 THEN '720'
            WHEN {width} > 0 OR {height} > 0 THEN 'sd' END)"
    )
}

/// The best dynamic range of the files `f` (as an aggregate), in the order
/// of [`DYNAMIC_RANGES`].
const RANGE_OF_FILES: &str = "CASE MAX(CASE f.dynamic_range WHEN 'dolby_vision' THEN 5 WHEN 'hdr10plus' THEN 4
        WHEN 'hdr10' THEN 3 WHEN 'hlg' THEN 2 WHEN 'sdr' THEN 1 END)
    WHEN 5 THEN 'dolby_vision' WHEN 4 THEN 'hdr10plus' WHEN 3 THEN 'hdr10' WHEN 2 THEN 'hlg' WHEN 1 THEN 'sdr' END";

/// `aggregate` of the files of title `i`: its own, a show's episodes', a
/// season's episodes'.
fn of_title_files(aggregate: &str) -> String {
    format!(
        "(CASE i.kind
            WHEN 'show' THEN (SELECT {aggregate} FROM items e JOIN media_files f ON f.item_id = e.id
                              WHERE e.grandparent_id = i.id)
            WHEN 'season' THEN (SELECT {aggregate} FROM items e JOIN media_files f ON f.item_id = e.id
                                WHERE e.parent_id = i.id)
            ELSE (SELECT {aggregate} FROM media_files f WHERE f.item_id = i.id) END)"
    )
}

/// The ids of the titles of `kind` in library `library_id` whose files,
/// taken together, pass `having` (an aggregate over the files `f`): one
/// pass over the library's files, where a subquery per title would read
/// a large library's thousands of titles one by one.
fn titles_whose_files(kind: &str, library_id: i64, having: &str) -> String {
    if kind == "show" {
        format!(
            "SELECT e.grandparent_id FROM media_files f JOIN items e ON e.id = f.item_id
             WHERE f.library_id = {library_id} AND e.grandparent_id IS NOT NULL
             GROUP BY e.grandparent_id HAVING {having}"
        )
    } else {
        format!(
            "SELECT f.item_id FROM media_files f WHERE f.library_id = {library_id}
             GROUP BY f.item_id HAVING {having}"
        )
    }
}

/// Title `i`'s resolution, one of [`RESOLUTIONS`] (NULL without a video file).
pub(crate) fn title_resolution_sql() -> String {
    // The best of the files: either line crossed by one of them.
    of_title_files(&resolution_case("MAX(f.width)", "MAX(f.height)"))
}

/// Title `i`'s dynamic range, one of [`DYNAMIC_RANGES`] (NULL without a video file).
pub(crate) fn title_range_sql() -> String {
    of_title_files(RANGE_OF_FILES)
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaFileDto {
    pub id: i64,
    pub path: String,
    pub size: i64,
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    pub bitrate: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub streams: Vec<MediaStream>,
    /// The edition the file is tagged with (`{edition-Final Cut}`).
    pub edition: Option<String>,
    /// Tracks to start with for the requesting user (remembered choices,
    /// see [`crate::streams`]); `None` = the file's default audio / no subtitles.
    pub selected_audio: Option<i64>,
    /// Whether [`Self::selected_audio`] is a track the user picked (for the
    /// show or the movie): without one, each player gets the track of that
    /// language it plays best, which a title's page can only call automatic.
    pub audio_picked: bool,
    pub selected_subtitle: Option<i64>,
    pub chapters: Vec<ChapterDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChapterDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: Option<String>,
    /// Picture of the chapter, cut from the seek previews once they exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemDetail {
    #[serde(flatten)]
    pub summary: ItemSummary,
    pub original_title: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub studio: Option<String>,
    pub content_rating: Option<String>,
    /// The country `content_rating` is from (ISO 3166-1, "GB"), when known:
    /// its age is read with that country's ratings, else the metadata
    /// region's (see `crate::ratings`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_rating_country: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub files: Vec<MediaFileDto>,
    /// Intro and credits of the files (Skip Intro / Skip Credits).
    pub markers: Vec<crate::markers::Marker>,
    /// Fields edited by hand (kept on refresh), see [`crate::overrides`].
    pub locked_fields: Vec<String>,
    /// Fields the item's NFO file sets (named as `locked_fields` are, and
    /// `rating`): they win over the online source, below hand edits.
    pub nfo_fields: Vec<String>,
    /// The item's labels, for administrators only (a child shouldn't read
    /// "Override: Adult"); left out for others.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// Trailers, featurettes and other bonus material (movies and shows).
    pub extras: Vec<ItemSummary>,
    /// Actors in billing order; for episodes, the guest stars.
    pub cast: Vec<crate::people::Credit>,
    /// Creators, directors, writers and producers.
    pub crew: Vec<crate::people::Credit>,
    /// In the user's My List (movies and shows).
    pub in_watchlist: bool,
}

pub fn image_url(id: i64, kind: &str, version: i64) -> String {
    format!("/api/items/{id}/images/{kind}?v={version}")
}

impl ItemRow {
    pub fn to_summary(&self) -> ItemSummary {
        let own = |kind: &str| image_url(self.id, kind, self.updated_at);
        let parent = |kind: &str| self.parent_id.map(|p| image_url(p, kind, self.parent_updated_at.unwrap_or(0)));
        let grandparent =
            |kind: &str| self.grandparent_id.map(|g| image_url(g, kind, self.grandparent_updated_at.unwrap_or(0)));

        let poster_url = if self.has_poster {
            Some(own("poster"))
        } else {
            match self.kind.as_str() {
                // Albums fall back to embedded cover art, extracted on first request.
                "album" => Some(own("poster")),
                "track" => parent("poster"),
                "season" if self.parent_has_poster => parent("poster"),
                "episode" if self.grandparent_has_poster => grandparent("poster"),
                _ => None,
            }
        };
        let backdrop_url = if self.has_backdrop {
            Some(own("backdrop"))
        } else if self.parent_has_backdrop {
            parent("backdrop")
        } else if self.grandparent_has_backdrop {
            grandparent("backdrop")
        } else {
            None
        };
        let thumb_url = matches!(self.kind.as_str(), "movie" | "episode" | "extra").then(|| own("thumb"));
        let watched = match self.kind.as_str() {
            "show" | "season" => {
                let leaves = self.leaf_count.unwrap_or(0);
                leaves > 0 && self.viewed_leaf_count.unwrap_or(0) >= leaves
            }
            _ => self.watched,
        };
        ItemSummary {
            id: self.id,
            library_id: self.library_id,
            kind: self.kind.clone(),
            title: self.title.clone(),
            sort_title: self.sort_title.clone(),
            year: self.year,
            index: self.idx,
            parent_id: self.parent_id,
            parent_title: self.parent_title.clone(),
            parent_index: if self.kind == "track" { self.disc } else { self.parent_idx },
            grandparent_id: self.grandparent_id,
            grandparent_title: self.grandparent_title.clone(),
            duration_ms: self.duration_ms,
            rating: self.rating,
            added_at: self.added_at,
            poster_url,
            backdrop_url,
            thumb_url,
            user_state: UserState {
                position_ms: self.position_ms,
                watched,
                play_count: self.play_count,
                last_played_at: self.last_played_at,
                rating: self.user_rating,
            },
            leaf_count: self.leaf_count,
            viewed_leaf_count: self.viewed_leaf_count,
            child_count: self.child_count,
            // Lists only show the synopsis of episodes; it would make a
            // movie library's listing several times larger.
            summary: if self.kind == "episode" { self.summary.clone() } else { None },
            release_date: self.release_date.clone(),
            extra_type: self.extra_type.clone(),
            media: None,
        }
    }

    /// [`to_summary`](Self::to_summary), with the synopsis of every kind
    /// when the client asked for them (`summaries=1`): Kodi shows what its
    /// lists carry, and nothing else, on a title's page.
    pub fn to_summary_with(&self, summaries: bool) -> ItemSummary {
        let mut summary = self.to_summary();
        if summaries {
            summary.summary = self.summary.clone();
        }
        summary
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct FileRow {
    pub id: i64,
    pub item_id: i64,
    pub library_id: i64,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    pub bitrate: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub media_info: Option<String>,
    pub external_subs: String,
    /// See [`crate::store::fingerprint`]; `None` until computed after a scan.
    pub fingerprint: Option<String>,
    /// JSON [`AddedSub`]s, from [`added_subs_column!`].
    pub added_subs: String,
}

/// The `added_subs` column of [`FileRow`] in a select of `media_files`.
#[macro_export]
macro_rules! added_subs_column {
    () => {
        "(SELECT json_group_array(json_object('id', a.id, 'index', a.idx, 'codec', a.codec,
        'language', a.language, 'title', a.title, 'forced', json(iif(a.forced, 'true', 'false')),
        'sdh', json(iif(a.sdh, 'true', 'false')), 'added_by', a.added_by) ORDER BY a.idx)
        FROM added_subtitles a WHERE a.file_id = media_files.id) AS added_subs"
    };
}

const FILE_SELECT: &str = concat!(
    "SELECT id, item_id, library_id, path, size, mtime, container, duration_ms, bitrate, width, height,
    video_codec, audio_codec, media_info, external_subs, fingerprint, ",
    added_subs_column!(),
    " FROM media_files"
);

impl FileRow {
    pub fn media_info(&self) -> Option<MediaInfo> {
        self.media_info.as_deref().and_then(|j| serde_json::from_str(j).ok())
    }

    pub fn external_subs(&self) -> Vec<ExternalSub> {
        serde_json::from_str(&self.external_subs).unwrap_or_default()
    }

    /// Subtitles people added ([`crate::subtitles`]), by index.
    pub fn added_subs(&self) -> Vec<AddedSub> {
        serde_json::from_str(&self.added_subs).unwrap_or_default()
    }

    /// Embedded streams followed by sidecar subtitles, then added ones.
    pub fn streams(&self) -> Vec<MediaStream> {
        self.streams_of(self.media_info().map(|m| m.streams).unwrap_or_default())
    }

    fn streams_of(&self, mut streams: Vec<MediaStream>) -> Vec<MediaStream> {
        streams.extend(self.external_subs().iter().map(ExternalSub::to_stream));
        streams.extend(self.added_subs().iter().map(AddedSub::to_stream));
        streams
    }

    pub fn to_dto(&self) -> MediaFileDto {
        // Parse the stored media info once.
        let (streams, chapters) = self.media_info().map(|m| (m.streams, m.chapters)).unwrap_or_default();
        MediaFileDto {
            id: self.id,
            path: self.path.clone(),
            size: self.size,
            container: self.container.clone(),
            duration_ms: self.duration_ms,
            bitrate: self.bitrate,
            width: self.width,
            height: self.height,
            video_codec: self.video_codec.clone(),
            audio_codec: self.audio_codec.clone(),
            streams: self.streams_of(streams),
            edition: crate::library::parse::edition(std::path::Path::new(&self.path)),
            selected_audio: None,
            audio_picked: false,
            selected_subtitle: None,
            chapters: chapters
                .into_iter()
                .map(|c| ChapterDto { start_ms: c.start_ms, end_ms: c.end_ms, title: c.title, thumb_url: None })
                .collect(),
        }
    }
}

/// Whether the user may see the item (library access, age limit), without
/// loading it.
pub async fn is_visible(db: &Db, user_id: i64, item_id: i64) -> sqlx::Result<bool> {
    let sql = format!(
        "{} SELECT EXISTS (SELECT 1 FROM items i LEFT JOIN items p ON p.id = i.parent_id
                           LEFT JOIN items gp ON gp.id = i.grandparent_id WHERE i.id = ? AND {VISIBLE})",
        me_cte!()
    );
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id).bind(item_id).fetch_one(&db.read).await
}

pub async fn item(db: &Db, user_id: i64, item_id: i64) -> sqlx::Result<Option<ItemRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{SUMMARY_SELECT} WHERE {VISIBLE} AND i.id = ?")))
        .bind(user_id)
        .bind(item_id)
        .fetch_optional(&db.read)
        .await
}

/// Rows for `ids`, in the order given.
/// Files of movies and episodes, newest first.
/// Movie and episode files with video, newest first: id, fingerprint and
/// path (not the probe data of every file).
pub async fn video_file_ids(db: &Db) -> sqlx::Result<Vec<(i64, Option<String>, String)>> {
    sqlx::query_as(
        "SELECT id, fingerprint, path FROM media_files WHERE video_codec IS NOT NULL
         AND item_id IN (SELECT id FROM items WHERE kind IN ('movie', 'episode')) ORDER BY added_at DESC",
    )
    .fetch_all(&db.read)
    .await
}

pub async fn items_by_ids(db: &Db, user_id: i64, ids: &[i64]) -> sqlx::Result<Vec<ItemRow>> {
    // SQLite takes at most 32766 parameters per statement.
    const CHUNK: usize = 4096;
    let mut by_id: HashMap<i64, ItemRow> = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(CHUNK) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("{SUMMARY_SELECT} WHERE {VISIBLE} AND i.id IN ({placeholders})");
        let mut query = sqlx::query_as::<_, ItemRow>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
        for id in chunk {
            query = query.bind(id);
        }
        by_id.extend(query.fetch_all(&db.read).await?.into_iter().map(|r| (r.id, r)));
    }
    // In the order asked for, repeats included (a playlist can hold an item twice).
    Ok(ids.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}

/// Seasons, episodes, albums or tracks under an item (not its extras).
pub async fn children(db: &Db, user_id: i64, parent_id: i64) -> sqlx::Result<Vec<ItemRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE} AND i.parent_id = ? AND i.kind != 'extra'
         ORDER BY CASE WHEN i.kind = 'season' AND i.idx = 0 THEN 1 ELSE 0 END, i.disc, i.idx IS NULL, i.idx, i.sort_title"
    )))
    .bind(user_id)
    .bind(parent_id)
    .fetch_all(&db.read)
    .await
}

/// Bonus material of a movie or show: trailers first, then by kind and title.
pub async fn extras(db: &Db, user_id: i64, parent_id: i64) -> sqlx::Result<Vec<ItemRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE} AND i.parent_id = ? AND i.kind = 'extra'
         ORDER BY i.extra_type != 'trailer', i.extra_type, i.sort_title"
    )))
    .bind(user_id)
    .bind(parent_id)
    .fetch_all(&db.read)
    .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Title,
    Year,
    Added,
    Rating,
    Release,
    Duration,
    LastPlayed,
    UserRating,
    /// The best file's bitrate (a show's: its episodes' on average), of
    /// the files a filter's version is made of.
    Bitrate,
}

impl SortKey {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "title" => SortKey::Title,
            "year" => SortKey::Year,
            "added" => SortKey::Added,
            "rating" => SortKey::Rating,
            "release" => SortKey::Release,
            "duration" => SortKey::Duration,
            "last_played" => SortKey::LastPlayed,
            "user_rating" => SortKey::UserRating,
            "bitrate" => SortKey::Bitrate,
            _ => return None,
        })
    }

    /// The sort expression for items of `kind`, and whether it can be NULL.
    /// Sorted by bitrate, a filtered listing reads the files that match, as
    /// [`media_summaries`] does, so the order follows the bitrates shown.
    fn column(self, kind: &str, files: FileFilter) -> (std::borrow::Cow<'static, str>, bool) {
        let show = kind == "show";
        if let (SortKey::Bitrate, Some(matching)) = (self, files.sql()) {
            let (aggregate, from) = if show {
                ("AVG", "items e JOIN media_files f ON f.item_id = e.id WHERE e.grandparent_id = i.id")
            } else {
                ("MAX", "media_files f WHERE f.item_id = i.id")
            };
            let expr = format!(
                "(SELECT CASE WHEN MAX({matching}) THEN {aggregate}(CASE WHEN {matching} THEN f.bitrate END)
                         ELSE {aggregate}(f.bitrate) END FROM {from})"
            );
            return (expr.into(), true);
        }
        let (expr, nullable) = match self {
            SortKey::Title => ("i.sort_title", false),
            SortKey::Year => ("i.year", true),
            // Shows are "added" when their newest episode was.
            SortKey::Added if show => {
                ("COALESCE((SELECT MAX(e.added_at) FROM items e WHERE e.grandparent_id = i.id), i.added_at)", false)
            }
            SortKey::Added => ("i.added_at", false),
            SortKey::Rating => ("i.rating", true),
            SortKey::Release => ("i.release_date", true),
            SortKey::Duration => ("i.duration_ms", true),
            SortKey::LastPlayed if show => (
                "COALESCE(s.last_played_at, (SELECT MAX(us.last_played_at) FROM items e JOIN user_item_state us ON us.item_id = e.id AND us.user_id = (SELECT uid FROM me) WHERE e.grandparent_id = i.id))",
                true,
            ),
            SortKey::LastPlayed => ("s.last_played_at", true),
            SortKey::UserRating => ("s.rating", true),
            SortKey::Bitrate if show => (
                "(SELECT AVG(f.bitrate) FROM items e JOIN media_files f ON f.item_id = e.id WHERE e.grandparent_id = i.id)",
                true,
            ),
            SortKey::Bitrate => ("(SELECT MAX(f.bitrate) FROM media_files f WHERE f.item_id = i.id)", true),
        };
        (expr.into(), nullable)
    }

    /// `ORDER BY` clause: NULLs last in both directions, title as tiebreaker.
    /// Columns stay bare where possible so SQLite can walk an index instead
    /// of sorting the whole library: it already puts NULLs last going down,
    /// so only ascending sorts of nullable columns need telling.
    fn order_by(self, kind: &str, descending: bool, files: FileFilter) -> String {
        let dir = if descending { "DESC" } else { "ASC" };
        let (col, nullable) = self.column(kind, files);
        match (col.as_ref(), nullable) {
            ("i.sort_title", _) => format!("i.sort_title {dir}"),
            (col, false) => format!("{col} {dir}, i.sort_title ASC"),
            (col, true) if descending => format!("{col} DESC, i.sort_title ASC"),
            (col, true) => format!("({col}) IS NULL, {col} ASC, i.sort_title ASC"),
        }
    }
}

pub struct LibraryQuery<'a> {
    pub library_id: i64,
    pub kind: &'a str,
    pub sort: SortKey,
    pub descending: bool,
    pub genre: Option<&'a str>,
    pub year: Option<i64>,
    /// Movies and episodes: watched flag; shows: every episode watched.
    pub watched: Option<bool>,
    /// The titles of these resolutions ([`resolutions_named`]).
    pub resolutions: &'static [&'static str],
    /// The titles of these dynamic ranges ([`dynamic_ranges_named`]).
    pub dynamic_ranges: &'static [&'static str],
    pub offset: i64,
    pub limit: Option<i64>,
}

impl<'a> LibraryQuery<'a> {
    /// The files its titles are described and sorted by.
    pub fn files(&self) -> FileFilter {
        FileFilter { resolutions: self.resolutions, dynamic_ranges: self.dynamic_ranges }
    }

    /// Every item of `kind` in the library, by title.
    pub fn all(library_id: i64, kind: &'a str) -> Self {
        LibraryQuery {
            library_id,
            kind,
            sort: SortKey::Title,
            descending: false,
            genre: None,
            year: None,
            watched: None,
            resolutions: &[],
            dynamic_ranges: &[],
            offset: 0,
            limit: None,
        }
    }
}

/// One page of a library (`limit` items from `offset`) and the total count.
/// Filtering, sorting and paging all happen in SQL.
pub async fn library_items(db: &Db, user_id: i64, q: &LibraryQuery<'_>) -> sqlx::Result<(Vec<ItemRow>, i64)> {
    let mut filter = String::from(" AND i.library_id = ? AND i.kind = ?");
    if q.genre.is_some() {
        filter.push_str(
            " AND EXISTS (SELECT 1 FROM item_genres g
                          WHERE g.genre = ? AND g.library_id = i.library_id AND g.kind = i.kind AND g.item_id = i.id)",
        );
    }
    if q.year.is_some() {
        filter.push_str(" AND i.year = ?");
    }
    if q.watched.is_some() {
        filter.push_str(
            " AND (CASE WHEN i.kind = 'show' THEN COALESCE(i.leaf_count, 0) > 0 AND
                     (SELECT COUNT(*) FROM items e JOIN user_item_state us ON us.item_id = e.id
                      WHERE e.grandparent_id = i.id AND e.kind = 'episode' AND us.user_id = (SELECT uid FROM me)
                        AND us.watched = 1) >= i.leaf_count
                   ELSE COALESCE(s.watched, 0) = 1 END) = ?",
        );
    }
    let json = |values: &[&str]| serde_json::to_string(values).unwrap_or_else(|_| "[]".into());
    if !q.resolutions.is_empty() {
        let best = resolution_case("MAX(f.width)", "MAX(f.height)");
        let having = format!("{best} IN (SELECT value FROM json_each(?))");
        filter.push_str(&format!(" AND i.id IN ({})", titles_whose_files(q.kind, q.library_id, &having)));
    }
    if !q.dynamic_ranges.is_empty() {
        let having = format!("{RANGE_OF_FILES} IN (SELECT value FROM json_each(?))");
        filter.push_str(&format!(" AND i.id IN ({})", titles_whose_files(q.kind, q.library_id, &having)));
    }
    let limit = q.limit.unwrap_or(i64::MAX).max(0);
    let sql = format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE}{filter} ORDER BY {} LIMIT ? OFFSET ?",
        q.sort.order_by(q.kind, q.descending, q.files())
    );
    macro_rules! bind_filter {
        ($query:expr) => {{
            let mut query = $query.bind(user_id).bind(q.library_id).bind(q.kind);
            if let Some(genre) = q.genre {
                query = query.bind(genre);
            }
            if let Some(year) = q.year {
                query = query.bind(year);
            }
            if let Some(watched) = q.watched {
                query = query.bind(watched);
            }
            if !q.resolutions.is_empty() {
                query = query.bind(json(q.resolutions));
            }
            if !q.dynamic_ranges.is_empty() {
                query = query.bind(json(q.dynamic_ranges));
            }
            query
        }};
    }
    let rows: Vec<ItemRow> = bind_filter!(sqlx::query_as::<_, ItemRow>(sqlx::AssertSqlSafe(sql.as_str())))
        .bind(limit)
        .bind(q.offset.max(0))
        .fetch_all(&db.read)
        .await?;
    // Counting is only needed when the page may not end the list: a short
    // page does, unless it's empty and past the end.
    let total = if (rows.len() as i64) < limit && (q.offset == 0 || !rows.is_empty()) {
        q.offset.max(0) + rows.len() as i64
    } else {
        let count = format!(
            "{} SELECT COUNT(*) FROM items i LEFT JOIN items p ON p.id = i.parent_id
                 LEFT JOIN items gp ON gp.id = i.grandparent_id
                 LEFT JOIN user_item_state s ON s.item_id = i.id AND s.user_id = (SELECT uid FROM me)
             WHERE {VISIBLE}{filter}",
            me_cte!()
        );
        bind_filter!(sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count.as_str()))).fetch_one(&db.read).await?
    };
    Ok((rows, total))
}

/// `FROM` clause for queries filtered by [`VISIBLE`] (it reads `p` and `gp`).
pub(crate) const VISIBLE_ITEMS: &str =
    "items i LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id";

/// Genres and years of the titles of a library that `user_id` can see (not
/// of titles their age limit hides).
pub async fn library_filters(
    db: &Db,
    user_id: i64,
    library_id: i64,
    kind: &str,
) -> sqlx::Result<(Vec<String>, Vec<i64>)> {
    let genres: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{ME_CTE} SELECT DISTINCT g.genre FROM item_genres g JOIN items i ON i.id = g.item_id
             LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE g.library_id = ? AND g.kind = ? AND {VISIBLE} ORDER BY g.genre COLLATE NOCASE"
    )))
    .bind(user_id)
    .bind(library_id)
    .bind(kind)
    .fetch_all(&db.read)
    .await?;
    let years: Vec<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{ME_CTE} SELECT DISTINCT i.year FROM {VISIBLE_ITEMS}
         WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? AND i.year IS NOT NULL ORDER BY i.year DESC"
    )))
    .bind(user_id)
    .bind(library_id)
    .bind(kind)
    .fetch_all(&db.read)
    .await?;
    Ok((genres, years))
}

/// The resolutions and dynamic ranges a library's titles may be filtered by.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MediaFilters {
    /// Of [`RESOLUTIONS`], those titles have.
    pub resolutions: Vec<&'static str>,
    /// Of [`RANGE_CHOICES`], those worth offering ([`range_choices`]).
    pub dynamic_ranges: Vec<&'static str>,
}

/// The values of a filter worth offering: those of `order` that titles
/// have, when they have two or more (one leaves nothing to choose).
fn offered(found: &HashSet<String>, order: &[&'static str]) -> Vec<&'static str> {
    let values: Vec<&'static str> = order.iter().copied().filter(|v| found.contains(*v)).collect();
    if values.len() > 1 { values } else { Vec::new() }
}

/// The dynamic ranges worth offering to titles of the ranges `found`, in
/// the order of [`RANGE_CHOICES`]: each that keeps some of the titles but
/// not all, and not the same ones as a choice of fewer kinds. Dolby Vision
/// alone takes the place of an HDR that would keep only it, and HDR that of
/// Any HDR without HLG beside it.
fn range_choices(found: &HashSet<String>) -> Vec<&'static str> {
    let kept = |choice: &str| -> Vec<&'static str> {
        dynamic_ranges_named(choice).iter().copied().filter(|r| found.contains(*r)).collect()
    };
    let all = DYNAMIC_RANGES.iter().filter(|r| found.contains(**r)).count();
    let mut narrowest_first = RANGE_CHOICES;
    narrowest_first.sort_by_key(|choice| dynamic_ranges_named(choice).len());
    let mut chosen: Vec<(&str, Vec<&str>)> = Vec::new();
    for choice in narrowest_first {
        let titles = kept(choice);
        if !titles.is_empty() && titles.len() < all && chosen.iter().all(|(_, other)| *other != titles) {
            chosen.push((choice, titles));
        }
    }
    RANGE_CHOICES.into_iter().filter(|choice| chosen.iter().any(|(c, _)| c == choice)).collect()
}

/// The resolutions and dynamic ranges of the titles of a movie or show
/// library that `user_id` can see worth filtering by.
pub async fn media_filters(db: &Db, user_id: i64, library_id: i64, kind: &str) -> sqlx::Result<MediaFilters> {
    if !matches!(kind, "movie" | "show") {
        return Ok(MediaFilters::default());
    }
    // Each title's best, in one pass over the library's files (a show's
    // through its episodes), then those the user may see.
    let (title, join) = if kind == "show" {
        ("e.grandparent_id", "JOIN items e ON e.id = f.item_id AND e.grandparent_id IS NOT NULL")
    } else {
        ("f.item_id", "")
    };
    let best = resolution_case("MAX(f.width)", "MAX(f.height)");
    let found: Vec<(Option<String>, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{ME_CTE}, titles AS (
             SELECT {title} AS id, {best} AS resolution, {RANGE_OF_FILES} AS dynamic_range
             FROM media_files f {join} WHERE f.library_id = ? GROUP BY {title})
         SELECT DISTINCT t.resolution, t.dynamic_range FROM titles t JOIN items i ON i.id = t.id
             LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE i.kind = ? AND {VISIBLE}"
    )))
    .bind(user_id)
    .bind(library_id)
    .bind(kind)
    .fetch_all(&db.read)
    .await?;
    let (resolutions, ranges): (HashSet<String>, HashSet<String>) =
        found.into_iter().map(|(r, d)| (r.unwrap_or_default(), d.unwrap_or_default())).unzip();
    Ok(MediaFilters { resolutions: offered(&resolutions, &RESOLUTIONS), dynamic_ranges: range_choices(&ranges) })
}

pub async fn files_for_item(db: &Db, item_id: i64) -> sqlx::Result<Vec<FileRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} WHERE item_id = ? ORDER BY COALESCE(height, 0) DESC, id"
    )))
    .bind(item_id)
    .fetch_all(&db.read)
    .await
}

/// The files of each of `ids`, in [`files_for_item`]'s order. The caller
/// checks access.
pub async fn files_for_items(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<FileRow>>> {
    let mut out: HashMap<i64, Vec<FileRow>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "{FILE_SELECT} WHERE item_id IN ({}) ORDER BY item_id, COALESCE(height, 0) DESC, id",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, FileRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for file in query.fetch_all(&db.read).await? {
            out.entry(file.item_id).or_default().push(file);
        }
    }
    Ok(out)
}

pub async fn file(db: &Db, file_id: i64) -> sqlx::Result<Option<FileRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{FILE_SELECT} WHERE id = ?")))
        .bind(file_id)
        .fetch_optional(&db.read)
        .await
}

#[derive(FromRow)]
struct DetailRow {
    original_title: Option<String>,
    tagline: Option<String>,
    genres: Option<String>,
    studio: Option<String>,
    content_rating: Option<String>,
    content_rating_country: Option<String>,
    tmdb_id: Option<i64>,
    imdb_id: Option<String>,
}

pub async fn item_detail(db: &Db, row: ItemRow) -> sqlx::Result<ItemDetail> {
    let detail: DetailRow = sqlx::query_as(
        "SELECT original_title, tagline, genres, studio, content_rating, content_rating_country, tmdb_id, imdb_id
         FROM items WHERE id = ?",
    )
    .bind(row.id)
    .fetch_one(&db.read)
    .await?;
    let files = files_for_item(db, row.id).await?;
    let markers = crate::markers::for_item(db, row.id).await?;
    // Seasons have the cast and crew of their show.
    let credits_of = if row.kind == "season" { row.parent_id.unwrap_or(row.id) } else { row.id };
    let (cast, crew) =
        crate::people::for_item(db, credits_of).await?.into_iter().partition(|c| c.role == crate::people::Role::Actor);
    let mut summary = row.to_summary();
    summary.summary = row.summary;
    Ok(ItemDetail {
        summary,
        original_title: detail.original_title,
        tagline: detail.tagline,
        genres: detail.genres.and_then(|g| serde_json::from_str(&g).ok()).unwrap_or_default(),
        studio: detail.studio,
        content_rating: detail.content_rating,
        content_rating_country: detail.content_rating_country,
        tmdb_id: detail.tmdb_id,
        imdb_id: detail.imdb_id,
        files: files.iter().map(FileRow::to_dto).collect(),
        markers,
        locked_fields: Vec::new(),
        nfo_fields: Vec::new(),
        labels: None,
        extras: Vec::new(),
        cast,
        crew,
        in_watchlist: false,
    })
}

// ------------------------------------------------------------------ listings with conditions built by the caller

/// A value for a `?` in the SQL of an [`ItemsQuery`].
#[derive(Debug, Clone, PartialEq)]
pub enum SqlArg {
    Int(i64),
    Real(f64),
    Text(String),
}

/// Items of one kind in a library, chosen and ordered by SQL that the caller
/// builds (a listing's filters and sorts, see `crate::listing`). [`VISIBLE`]
/// applies as everywhere else.
pub struct ItemsQuery<'a> {
    pub library_id: i64,
    pub kind: &'a str,
    /// Conditions, each starting with ` AND `, on the item `i`, its parent `p`
    /// and grandparent `gp`, the user's state `s` and the `me` CTE. Their `?`
    /// take `args` in order.
    pub filter: &'a str,
    pub args: &'a [SqlArg],
    /// Whether `filter` reads `s`; counting leaves the join out otherwise.
    pub uses_state: bool,
    /// `ORDER BY` terms, which may read `s` too.
    pub order_by: &'a str,
    pub offset: i64,
    pub limit: Option<i64>,
}

/// Binds the arguments of an [`ItemsQuery`] after those already bound.
macro_rules! bind_sql_args {
    ($query:expr, $args:expr) => {{
        let mut query = $query;
        for arg in $args {
            query = match arg {
                SqlArg::Int(value) => query.bind(*value),
                SqlArg::Real(value) => query.bind(*value),
                SqlArg::Text(value) => query.bind(value.as_str()),
            };
        }
        query
    }};
}

/// `limit` matching items from `offset`, in the query's order.
pub async fn items_matching(db: &Db, user_id: i64, q: &ItemsQuery<'_>) -> sqlx::Result<Vec<ItemRow>> {
    let sql = format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ?{} ORDER BY {} LIMIT ? OFFSET ?",
        q.filter, q.order_by
    );
    let query =
        sqlx::query_as::<_, ItemRow>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id).bind(q.library_id).bind(q.kind);
    bind_sql_args!(query, q.args)
        .bind(q.limit.unwrap_or(i64::MAX).max(0))
        .bind(q.offset.max(0))
        .fetch_all(&db.read)
        .await
}

/// How many items match, whatever the offset and limit.
pub async fn count_matching(db: &Db, user_id: i64, q: &ItemsQuery<'_>) -> sqlx::Result<i64> {
    let state = if q.uses_state {
        " LEFT JOIN user_item_state s ON s.item_id = i.id AND s.user_id = (SELECT uid FROM me)"
    } else {
        ""
    };
    let sql = format!(
        "{ME_CTE} SELECT COUNT(*) FROM {VISIBLE_ITEMS}{state} WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ?{}",
        q.filter
    );
    let query =
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id).bind(q.library_id).bind(q.kind);
    bind_sql_args!(query, q.args).fetch_one(&db.read).await
}

// ------------------------------------------------------------------ watch state

pub const WATCHED_THRESHOLD: f64 = 0.9;
const MIN_RESUME_MS: i64 = 15_000;

/// What a progress report did besides keeping the place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Recorded {
    /// The item became watched, counted as a play for the first time.
    pub counted: bool,
    /// A watched movie left the user's My List.
    pub left_watchlist: bool,
}

/// Record playback progress. Crossing 90 % marks the item watched and, the
/// first time for this play, counts it and adds it to the watch history from
/// `device`. Any report brings the title back to Continue Watching if the
/// user took it out, and a movie watched to the end leaves My List.
///
/// The user's devices hear of a report that changes what they show (see
/// [`state_changed`]), not of every one: the item watched, a resume point
/// started or cleared, a title back in Continue Watching, or the player
/// (`state`) pausing or stopping somewhere new, where Play elsewhere starts.
#[allow(clippy::too_many_arguments)]
pub async fn record_progress(
    db: &Db,
    events: &EventBus,
    user_id: i64,
    item_id: i64,
    position_ms: i64,
    duration_ms: i64,
    device: Option<&str>,
    state: &str,
) -> sqlx::Result<Recorded> {
    let now = now();
    let position_ms = position_ms.max(0);
    let back = show_again(db, user_id, "", item_id).await?;
    if duration_ms > 0 && position_ms as f64 >= duration_ms as f64 * WATCHED_THRESHOLD {
        let mut tx = db.write.begin().await?;
        // Reports keep coming after the threshold: a play counts once, until
        // the item is resumed or played again (which sets a position).
        let counted: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM user_item_state
                            WHERE user_id = ? AND item_id = ? AND watched = 1 AND position_ms = 0)",
        )
        .bind(user_id)
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at)
             VALUES (?, ?, 0, 1, 1, ?, ?)
             ON CONFLICT(user_id, item_id) DO UPDATE SET
                play_count = CASE WHEN user_item_state.position_ms > 0 OR user_item_state.watched = 0
                                  THEN user_item_state.play_count + 1 ELSE user_item_state.play_count END,
                watched = 1, position_ms = 0, last_played_at = excluded.last_played_at, updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(item_id)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if !counted {
            sqlx::query("INSERT INTO play_history (user_id, item_id, viewed_at, device) VALUES (?, ?, ?, ?)")
                .bind(user_id)
                .bind(item_id)
                .bind(now)
                .bind(device)
                .execute(&mut *tx)
                .await?;
        }
        // Of what plays, only movies are in My List (shows stay there).
        let left = sqlx::query("DELETE FROM watchlist WHERE user_id = ? AND item_id = ?")
            .bind(user_id)
            .bind(item_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        // Watched, or a rewatch's resume point cleared; the reports that
        // follow during the credits change nothing.
        if !counted || back {
            state_changed(db, events, user_id, item_id, false).await?;
        }
        return Ok(Recorded { counted: !counted, left_watchlist: left > 0 });
    }
    let stored = if position_ms >= MIN_RESUME_MS { position_ms } else { 0 };
    let was_at: i64 = sqlx::query_scalar("SELECT position_ms FROM user_item_state WHERE user_id = ? AND item_id = ?")
        .bind(user_id)
        .bind(item_id)
        .fetch_optional(&db.read)
        .await?
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at)
         VALUES (?, ?, ?, 0, 0, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
            position_ms = excluded.position_ms, last_played_at = excluded.last_played_at, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(stored)
    .bind(now)
    .bind(now)
    .execute(&db.write)
    .await?;
    // Ticks while it plays change nothing the devices show; a resume point
    // started or cleared does, and one moved as the player pauses or stops.
    let settled = matches!(state, "paused" | "stopped") && stored != was_at;
    if back || (was_at > 0) != (stored > 0) || settled {
        state_changed(db, events, user_id, item_id, false).await?;
    }
    Ok(Recorded::default())
}

/// Tells the user's devices that their watch state of `item_id` changed
/// (with `children`, of everything under it too), with its parent and
/// grandparent, whose counts follow it: one event, whatever it covers.
async fn state_changed(db: &Db, events: &EventBus, user_id: i64, item_id: i64, children: bool) -> sqlx::Result<()> {
    let parents: Option<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT parent_id, grandparent_id FROM items WHERE id = ?")
            .bind(item_id)
            .fetch_optional(&db.read)
            .await?;
    let (parent, grandparent) = parents.unwrap_or_default();
    let item_ids = std::iter::once(item_id).chain(parent).chain(grandparent).collect();
    events.send(ServerEvent::UserStateChanged { user_id, item_ids, children });
    Ok(())
}

/// When the user last played an item, as a sync tool copying it from
/// another server says (playing or marking it stamps the present time).
pub async fn set_last_played(db: &Db, user_id: i64, item_id: i64, at: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE user_item_state SET last_played_at = ? WHERE user_id = ? AND item_id = ?")
        .bind(at)
        .bind(user_id)
        .bind(item_id)
        .execute(&db.write)
        .await?;
    Ok(())
}

/// Set (0-10) or clear the user's star rating of an item.
pub async fn set_rating(db: &Db, user_id: i64, item_id: i64, rating: Option<f64>) -> sqlx::Result<()> {
    let now = now();
    sqlx::query(
        "INSERT INTO user_item_state (user_id, item_id, rating, rated_at, updated_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
            rating = excluded.rating, rated_at = excluded.rated_at, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(rating)
    .bind(rating.map(|_| now))
    .bind(now)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// A rating from 0 to 10, rounded to half stars; `None` for anything else.
pub fn valid_rating(value: f64) -> Option<f64> {
    (value.is_finite() && (0.0..=10.0).contains(&value)).then(|| value.round())
}

/// Leaf items covered by `item` (itself for movies/episodes/tracks).
fn leaves_clause(kind: &str) -> &'static str {
    match kind {
        "show" => "SELECT id FROM items WHERE grandparent_id = ?3 AND kind = 'episode'",
        "season" | "album" => "SELECT id FROM items WHERE parent_id = ?3",
        "artist" => "SELECT id FROM items WHERE grandparent_id = ?3 AND kind = 'track'",
        _ => "SELECT id FROM items WHERE id = ?3",
    }
}

/// Marks an item and what it covers (a show's episodes, …) watched or not.
/// Marked watched, they count as played for Continue Watching (a title taken
/// out of it comes back) and a movie leaves My List: returns whether one did.
/// The user's devices hear of it once, unless nothing changed (an item
/// marked watched again).
pub async fn set_watched(
    db: &Db,
    events: &EventBus,
    user_id: i64,
    item_id: i64,
    kind: &str,
    watched: bool,
) -> sqlx::Result<bool> {
    let leaves = leaves_clause(kind);
    let now = now();
    // Leaves not in that state yet (a resume point counts as not either).
    let pending = if watched {
        "COALESCE(s.watched, 0) = 0 OR COALESCE(s.position_ms, 0) > 0"
    } else {
        "s.watched = 1 OR s.position_ms > 0"
    };
    let changes: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 FROM items i LEFT JOIN user_item_state s ON s.item_id = i.id AND s.user_id = ?1
                        WHERE i.id IN ({leaves}) AND ({pending}))"
    )))
    .bind(user_id)
    .bind(0)
    .bind(item_id)
    .fetch_one(&db.read)
    .await?;
    let mut left_watchlist = false;
    let mut back = false;
    if watched {
        back = show_again(db, user_id, kind, item_id).await?;
        left_watchlist = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM watchlist WHERE user_id = ?1 AND item_id IN ({leaves})"
        )))
        .bind(user_id)
        .bind(now)
        .bind(item_id)
        .execute(&db.write)
        .await?
        .rows_affected()
            > 0;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at)
             SELECT ?1, id, 0, 1, 1, ?2, ?2 FROM items WHERE id IN ({leaves})
             ON CONFLICT(user_id, item_id) DO UPDATE SET
                play_count = CASE WHEN user_item_state.watched = 0 THEN user_item_state.play_count + 1 ELSE user_item_state.play_count END,
                watched = 1, position_ms = 0, last_played_at = excluded.last_played_at, updated_at = excluded.updated_at"
        )))
        .bind(user_id)
        .bind(now)
        .bind(item_id)
        .execute(&db.write)
        .await?;
    } else {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE user_item_state SET watched = 0, position_ms = 0, updated_at = ?2
             WHERE user_id = ?1 AND item_id IN ({leaves})"
        )))
        .bind(user_id)
        .bind(now)
        .bind(item_id)
        .execute(&db.write)
        .await?;
    }
    if changes || back {
        let children = matches!(kind, "show" | "season" | "album" | "artist");
        state_changed(db, events, user_id, item_id, children).await?;
    }
    Ok(left_watchlist)
}

// ------------------------------------------------------------------ hubs

/// Movies and episodes the user is part way through, newest first (not
/// those of titles they took out of Continue Watching).
pub async fn in_progress(db: &Db, user_id: i64, library_id: Option<i64>, limit: i64) -> sqlx::Result<Vec<ItemRow>> {
    let scope = if library_id.is_some() { "AND i.library_id = ?" } else { "" };
    let sql = format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE} AND s.position_ms > 0 AND s.watched = 0 AND i.kind IN ('movie', 'episode') {scope}
           AND NOT EXISTS (SELECT 1 FROM continue_watching_hidden h
                           WHERE h.user_id = (SELECT uid FROM me) AND h.item_id = COALESCE(i.grandparent_id, i.id))
         ORDER BY s.last_played_at DESC LIMIT ?"
    );
    let mut q = sqlx::query_as::<_, ItemRow>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
    if let Some(lib) = library_id {
        q = q.bind(lib);
    }
    q.bind(limit).fetch_all(&db.read).await
}

#[derive(FromRow)]
struct EpisodeState {
    id: i64,
    watched: bool,
    position_ms: i64,
    /// Times watched to the end: more than one is a rewatch.
    play_count: i64,
    last_played_at: Option<i64>,
}

/// Episodes of a show in viewing order (specials last) with the user's state.
async fn show_episodes(db: &Db, user_id: i64, show_id: i64) -> sqlx::Result<Vec<EpisodeState>> {
    sqlx::query_as(
        "SELECT e.id, COALESCE(s.watched, 0) AS watched, COALESCE(s.position_ms, 0) AS position_ms,
                COALESCE(s.play_count, 0) AS play_count, s.last_played_at
         FROM items e JOIN items se ON se.id = e.parent_id
         LEFT JOIN user_item_state s ON s.item_id = e.id AND s.user_id = ?
         WHERE e.grandparent_id = ? AND e.kind = 'episode'
         ORDER BY se.idx = 0, se.idx, e.idx IS NULL, e.idx, e.sort_title",
    )
    .bind(user_id)
    .bind(show_id)
    .fetch_all(&db.read)
    .await
}

/// The episode to watch after the most recently watched one, unless it is
/// part way through for the first time (Continue Watching has it already; a
/// seen one rewatched part of the way plays from its start, as apps do).
///
/// The one right after it when that one is unwatched. When it was seen
/// before, it depends on what the user is doing:
/// - going through the show again (the last one watched was a rewatch, and
///   either nothing after it is left unwatched or the one before it was
///   rewatched too, after anything later was seen): the one right after it,
///   seen or not, so a rewatch doesn't jump over episodes;
/// - otherwise (filling a gap, or one favourite seen again): the first
///   unwatched one after it, where the user had got to.
fn next_after_last_watched(episodes: &[EpisodeState]) -> Option<i64> {
    let last = episodes
        .iter()
        .enumerate()
        .filter(|(_, e)| e.watched && e.last_played_at.is_some())
        .max_by_key(|(_, e)| e.last_played_at)
        .map(|(i, _)| i)?;
    let after = &episodes[last + 1..];
    let first_unwatched = after.iter().find(|e| !e.watched);
    let next = match after.first() {
        Some(next) if !next.watched => Some(next),
        Some(next) if rewatching(episodes, last, first_unwatched.is_none()) => Some(next),
        _ => first_unwatched,
    };
    next.filter(|e| e.watched || e.position_ms == 0).map(|e| e.id)
}

/// Whether episode `last` (the most recently watched) is part of a rewatch
/// of the show, which goes on in order (see [`next_after_last_watched`]).
fn rewatching(episodes: &[EpisodeState], last: usize, all_seen_after: bool) -> bool {
    if episodes[last].play_count < 2 {
        return false;
    }
    if all_seen_after {
        return true;
    }
    // A run: the one before was rewatched too, after anything later was seen.
    let later = episodes[last + 1..].iter().filter_map(|e| e.last_played_at).max();
    last.checked_sub(1)
        .map(|i| &episodes[i])
        .is_some_and(|before| before.play_count >= 2 && before.last_played_at > later)
}

/// Episodes of several shows (as [`show_episodes`]), in one query.
async fn shows_episodes(db: &Db, user_id: i64, show_ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<EpisodeState>>> {
    let mut out: HashMap<i64, Vec<EpisodeState>> = HashMap::new();
    if show_ids.is_empty() {
        return Ok(out);
    }
    let placeholders = vec!["?"; show_ids.len()].join(",");
    let sql = format!(
        "SELECT e.grandparent_id, e.id, COALESCE(s.watched, 0), COALESCE(s.position_ms, 0), COALESCE(s.play_count, 0),
                s.last_played_at
         FROM items e JOIN items se ON se.id = e.parent_id
         LEFT JOIN user_item_state s ON s.item_id = e.id AND s.user_id = ?
         WHERE e.grandparent_id IN ({placeholders}) AND e.kind = 'episode'
         ORDER BY e.grandparent_id, se.idx = 0, se.idx, e.idx IS NULL, e.idx, e.sort_title"
    );
    let mut query =
        sqlx::query_as::<_, (i64, i64, bool, i64, i64, Option<i64>)>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
    for id in show_ids {
        query = query.bind(id);
    }
    for (show, id, watched, position_ms, play_count, last_played_at) in query.fetch_all(&db.read).await? {
        out.entry(show).or_default().push(EpisodeState { id, watched, position_ms, play_count, last_played_at });
    }
    Ok(out)
}

/// "Next up" episodes for shows with recent activity: (episode id, activity time).
pub async fn next_up(db: &Db, user_id: i64, library_id: Option<i64>, limit: i64) -> sqlx::Result<Vec<(i64, i64)>> {
    // The user's plays newest first (an index), down to enough shows: a
    // show's first sighting is its latest activity.
    let scope = if library_id.is_some() { "AND e.library_id = ?" } else { "" };
    let sql = format!(
        "SELECT e.grandparent_id, s.last_played_at FROM user_item_state s JOIN items e ON e.id = s.item_id
         WHERE s.user_id = ? AND s.last_played_at IS NOT NULL AND e.kind = 'episode'
           AND e.grandparent_id IS NOT NULL {scope}
         ORDER BY s.last_played_at DESC"
    );
    let mut q = sqlx::query_as::<_, (i64, i64)>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
    if let Some(lib) = library_id {
        q = q.bind(lib);
    }
    let hidden: Vec<i64> = sqlx::query_scalar("SELECT item_id FROM continue_watching_hidden WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(&db.read)
        .await?;
    let mut shows: Vec<(i64, i64)> = Vec::new();
    let mut plays = q.fetch(&db.read);
    while let Some((show, at)) = plays.try_next().await? {
        if !hidden.contains(&show) && !shows.iter().any(|(s, _)| *s == show) {
            shows.push((show, at));
            if shows.len() as i64 >= limit * 2 {
                break;
            }
        }
    }
    drop(plays);
    let ids: Vec<i64> = shows.iter().map(|(show, _)| *show).collect();
    let episodes = shows_episodes(db, user_id, &ids).await?;
    let mut out = Vec::new();
    for (show_id, activity) in shows {
        if let Some(next) = episodes.get(&show_id).and_then(|e| next_after_last_watched(e)) {
            out.push((next, activity));
            if out.len() as i64 >= limit {
                break;
            }
        }
    }
    Ok(out)
}

/// Continue watching = in-progress items + next-up episodes, newest activity first.
pub async fn continue_watching(
    db: &Db,
    user_id: i64,
    library_id: Option<i64>,
    limit: i64,
) -> sqlx::Result<Vec<ItemRow>> {
    let mut entries: Vec<(i64, i64)> = in_progress(db, user_id, library_id, limit)
        .await?
        .into_iter()
        .map(|r| (r.id, r.last_played_at.unwrap_or(0)))
        .collect();
    for (id, at) in next_up(db, user_id, library_id, limit).await? {
        if !entries.iter().any(|(e, _)| *e == id) {
            entries.push((id, at));
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));
    entries.truncate(limit as usize);
    let ids: Vec<i64> = entries.into_iter().map(|(id, _)| id).collect();
    items_by_ids(db, user_id, &ids).await
}

/// Episode to start when pressing Play on a show or season.
pub async fn on_deck(db: &Db, user_id: i64, item: &ItemRow) -> sqlx::Result<Option<i64>> {
    let (show_id, season_id) = match item.kind.as_str() {
        "show" => (item.id, None),
        "season" => (item.parent_id.unwrap_or_default(), Some(item.id)),
        _ => return Ok(None),
    };
    let mut episodes = show_episodes(db, user_id, show_id).await?;
    if let Some(season) = season_id {
        let in_season: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM items WHERE parent_id = ?").bind(season).fetch_all(&db.read).await?;
        episodes.retain(|e| in_season.contains(&e.id));
    }
    let in_progress =
        episodes.iter().filter(|e| e.position_ms > 0 && !e.watched).max_by_key(|e| e.last_played_at).map(|e| e.id);
    Ok(in_progress
        .or_else(|| next_after_last_watched(&episodes))
        .or_else(|| episodes.iter().find(|e| !e.watched).map(|e| e.id))
        .or_else(|| episodes.first().map(|e| e.id)))
}

// ------------------------------------------------------------------ Continue Watching and My List

/// The title a Continue Watching row stands for, which taking it out hides:
/// a movie itself, an episode's (or a season's) show. `None` for what is
/// never in Continue Watching.
pub fn continue_watching_title(row: &ItemRow) -> Option<i64> {
    match row.kind.as_str() {
        "movie" | "show" => Some(row.id),
        "season" => row.parent_id,
        "episode" => row.grandparent_id,
        _ => None,
    }
}

/// Takes a title ([`continue_watching_title`]) out of the user's Continue
/// Watching, until they play it again (see [`show_again`]). Their other
/// devices hear of it.
pub async fn hide_from_continue_watching(db: &Db, events: &EventBus, user_id: i64, title_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO continue_watching_hidden (user_id, item_id, hidden_at) VALUES (?, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET hidden_at = excluded.hidden_at",
    )
    .bind(user_id)
    .bind(title_id)
    .bind(now())
    .execute(&db.write)
    .await?;
    state_changed(db, events, user_id, title_id, false).await
}

/// The user played `item_id` (of `kind`; what it covers when it is a show
/// or a season) again: its movie or show comes back to Continue Watching
/// if they took it out; whether one did. Looked up first, since every
/// progress report asks.
async fn show_again(db: &Db, user_id: i64, kind: &str, item_id: i64) -> sqlx::Result<bool> {
    let titles = format!("SELECT COALESCE(grandparent_id, id) FROM items WHERE id IN ({})", leaves_clause(kind));
    let hidden: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 FROM continue_watching_hidden WHERE user_id = ?1 AND item_id IN ({titles}))"
    )))
    .bind(user_id)
    .bind(0)
    .bind(item_id)
    .fetch_one(&db.read)
    .await?;
    if hidden {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM continue_watching_hidden WHERE user_id = ?1 AND item_id IN ({titles})"
        )))
        .bind(user_id)
        .bind(0)
        .bind(item_id)
        .execute(&db.write)
        .await?;
    }
    Ok(hidden)
}

/// Adds a movie or a show to the user's My List; whether it wasn't there.
pub async fn add_to_watchlist(db: &Db, user_id: i64, item_id: i64) -> sqlx::Result<bool> {
    let added = sqlx::query(
        "INSERT INTO watchlist (user_id, item_id, added_at) VALUES (?, ?, ?) ON CONFLICT(user_id, item_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(now())
    .execute(&db.write)
    .await?;
    Ok(added.rows_affected() > 0)
}

/// Takes an item out of the user's My List; whether it was there.
pub async fn remove_from_watchlist(db: &Db, user_id: i64, item_id: i64) -> sqlx::Result<bool> {
    let removed = sqlx::query("DELETE FROM watchlist WHERE user_id = ? AND item_id = ?")
        .bind(user_id)
        .bind(item_id)
        .execute(&db.write)
        .await?;
    Ok(removed.rows_affected() > 0)
}

/// The user's My List, newest added first: what they may see of it (library
/// access and age limit, as everywhere).
pub async fn watchlist(db: &Db, user_id: i64, limit: Option<i64>) -> sqlx::Result<Vec<ItemRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT} JOIN watchlist w ON w.item_id = i.id AND w.user_id = (SELECT uid FROM me)
         WHERE {VISIBLE} ORDER BY w.added_at DESC, w.rowid DESC LIMIT ?"
    )))
    .bind(user_id)
    .bind(limit.unwrap_or(i64::MAX).max(0))
    .fetch_all(&db.read)
    .await
}

/// Whether the item is in the user's My List.
pub async fn in_watchlist(db: &Db, user_id: i64, item_id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM watchlist WHERE user_id = ? AND item_id = ?)")
        .bind(user_id)
        .bind(item_id)
        .fetch_one(&db.read)
        .await
}

/// The episode/track following `item` (crossing season/disc boundaries).
pub async fn next_item(db: &Db, item: &ItemRow) -> sqlx::Result<Option<i64>> {
    match item.kind.as_str() {
        "episode" => {
            let ids: Vec<i64> = sqlx::query_scalar(
                "SELECT e.id FROM items e JOIN items se ON se.id = e.parent_id
                 WHERE e.grandparent_id = ? AND e.kind = 'episode' ORDER BY se.idx = 0, se.idx, e.idx IS NULL, e.idx, e.sort_title",
            )
            .bind(item.grandparent_id)
            .fetch_all(&db.read)
            .await?;
            Ok(ids.iter().position(|id| *id == item.id).and_then(|i| ids.get(i + 1)).copied())
        }
        "track" => {
            let ids: Vec<i64> =
                sqlx::query_scalar("SELECT id FROM items WHERE parent_id = ? ORDER BY disc, idx, sort_title")
                    .bind(item.parent_id)
                    .fetch_all(&db.read)
                    .await?;
            Ok(ids.iter().position(|id| *id == item.id).and_then(|i| ids.get(i + 1)).copied())
        }
        _ => Ok(None),
    }
}

pub async fn recently_added(
    db: &Db,
    user_id: i64,
    library_id: i64,
    library_kind: &str,
    limit: i64,
) -> sqlx::Result<Vec<ItemRow>> {
    match library_kind {
        "shows" => {
            // Newest episodes first (an index), down to enough shows: a
            // show's first sighting is its newest episode.
            let mut episodes = sqlx::query_scalar::<_, i64>(
                "SELECT grandparent_id FROM items
                 WHERE library_id = ? AND kind = 'episode' AND grandparent_id IS NOT NULL ORDER BY added_at DESC",
            )
            .bind(library_id)
            .fetch(&db.read);
            let mut ids: Vec<i64> = Vec::new();
            while let Some(show) = episodes.try_next().await? {
                if !ids.contains(&show) {
                    ids.push(show);
                    if ids.len() as i64 >= limit {
                        break;
                    }
                }
            }
            drop(episodes);
            items_by_ids(db, user_id, &ids).await
        }
        kind => {
            let item_kind = if kind == "music" { "album" } else { "movie" };
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "{SUMMARY_SELECT} WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? ORDER BY i.added_at DESC, i.id DESC LIMIT ?"
            )))
            .bind(user_id)
            .bind(library_id)
            .bind(item_kind)
            .bind(limit)
            .fetch_all(&db.read)
            .await
        }
    }
}

pub async fn recently_released(
    db: &Db,
    user_id: i64,
    library_id: i64,
    kind: &str,
    limit: i64,
) -> sqlx::Result<Vec<ItemRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? AND i.release_date IS NOT NULL
         ORDER BY i.release_date DESC LIMIT ?"
    )))
    .bind(user_id)
    .bind(library_id)
    .bind(kind)
    .bind(limit)
    .fetch_all(&db.read)
    .await
}

pub async fn top_rated(db: &Db, user_id: i64, library_id: i64, kind: &str, limit: i64) -> sqlx::Result<Vec<ItemRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUMMARY_SELECT} WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? AND i.rating IS NOT NULL ORDER BY i.rating DESC, i.sort_title LIMIT ?"
    )))
    .bind(user_id)
    .bind(library_id)
    .bind(kind)
    .bind(limit)
    .fetch_all(&db.read)
    .await
}

#[cfg(test)]
impl ItemRow {
    /// A bare row for unit tests: no parents, artwork, ratings or user state.
    pub(crate) fn test_row(id: i64, kind: &str, title: &str) -> Self {
        ItemRow {
            id,
            library_id: 1,
            kind: kind.into(),
            title: title.into(),
            sort_title: title.into(),
            year: None,
            idx: None,
            disc: None,
            parent_id: None,
            parent_title: None,
            parent_idx: None,
            grandparent_id: None,
            grandparent_title: None,
            duration_ms: None,
            rating: None,
            added_at: 0,
            updated_at: 0,
            summary: None,
            release_date: None,
            extra_type: None,
            has_poster: false,
            has_backdrop: false,
            has_thumb: false,
            parent_has_poster: false,
            parent_has_backdrop: false,
            parent_updated_at: None,
            grandparent_has_poster: false,
            grandparent_has_backdrop: false,
            grandparent_updated_at: None,
            position_ms: 0,
            watched: false,
            play_count: 0,
            last_played_at: None,
            user_rating: None,
            leaf_count: None,
            child_count: None,
            viewed_leaf_count: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(id: i64, watched: bool, position_ms: i64, last: Option<i64>) -> EpisodeState {
        EpisodeState { id, watched, position_ms, play_count: i64::from(watched), last_played_at: last }
    }

    /// An episode watched `count` times, last at `at`.
    fn seen(id: i64, count: i64, at: i64) -> EpisodeState {
        EpisodeState { id, watched: true, position_ms: 0, play_count: count, last_played_at: Some(at) }
    }

    #[test]
    fn next_up_follows_last_watched() {
        let eps =
            vec![ep(1, true, 0, Some(10)), ep(2, true, 0, Some(20)), ep(3, false, 0, None), ep(4, false, 0, None)];
        assert_eq!(next_after_last_watched(&eps), Some(3));
        // An early episode watched for the first time after a later one (a
        // gap filled): on to the first unwatched one after it.
        let eps = vec![ep(1, true, 0, Some(30)), ep(2, true, 0, Some(20)), ep(3, false, 0, None)];
        assert_eq!(next_after_last_watched(&eps), Some(3));
        // Nothing watched → no next-up.
        assert_eq!(next_after_last_watched(&[ep(1, false, 0, None)]), None);
        // Next one in progress → it's in "continue watching" already.
        let eps = vec![ep(1, true, 0, Some(10)), ep(2, false, 5000, Some(11))];
        assert_eq!(next_after_last_watched(&eps), None);
        // The last episode was watched last: nothing comes next.
        let eps = vec![ep(1, false, 0, None), ep(2, true, 0, Some(10))];
        assert_eq!(next_after_last_watched(&eps), None);
    }

    #[test]
    fn a_rewatch_goes_on_in_order_without_skipping_what_was_seen() {
        // A show seen through once (plays at 1..=4), started again: after the
        // first episode, the second, though it was seen.
        let eps = vec![seen(1, 2, 100), seen(2, 1, 2), seen(3, 1, 3), seen(4, 1, 4)];
        assert_eq!(next_after_last_watched(&eps), Some(2));
        // Further on in that rewatch: the next one, still in order.
        let eps = vec![seen(1, 2, 100), seen(2, 2, 110), seen(3, 1, 3), seen(4, 1, 4)];
        assert_eq!(next_after_last_watched(&eps), Some(3));
        // A rewatch of seen episodes, with unseen ones still to come: a run of
        // rewatched episodes goes on in order (not on to episode 5).
        let eps = vec![seen(1, 2, 100), seen(2, 2, 110), seen(3, 1, 3), seen(4, 1, 4), ep(5, false, 0, None)];
        assert_eq!(next_after_last_watched(&eps), Some(3));
        // One favourite seen again while part way through the show: back to
        // where the user had got to.
        let eps = vec![seen(1, 1, 1), seen(2, 2, 100), seen(3, 1, 3), seen(4, 1, 4), ep(5, false, 0, None)];
        assert_eq!(next_after_last_watched(&eps), Some(5));
        // The one before was rewatched long ago, before the later ones were
        // seen: not a run.
        let eps = vec![seen(1, 2, 2), seen(2, 2, 100), seen(3, 1, 30), seen(4, 1, 40), ep(5, false, 0, None)];
        assert_eq!(next_after_last_watched(&eps), Some(5));
        // The rewatch reached the end of what was seen: on to the new ones.
        let eps = vec![seen(1, 2, 100), seen(2, 2, 110), ep(3, false, 0, None)];
        assert_eq!(next_after_last_watched(&eps), Some(3));
        // The episode right after was rewatched part of the way before: it
        // still comes next (apps play a seen episode from its start).
        let mut stopped = seen(2, 1, 50);
        stopped.position_ms = 600_000;
        let eps = vec![seen(1, 2, 100), stopped, seen(3, 1, 3)];
        assert_eq!(next_after_last_watched(&eps), Some(2));
    }

    #[test]
    fn ratings_are_half_stars_between_zero_and_ten() {
        assert_eq!(valid_rating(6.6), Some(7.0));
        assert_eq!(valid_rating(6.4), Some(6.0));
        assert_eq!(valid_rating(0.0), Some(0.0));
        assert_eq!(valid_rating(10.0), Some(10.0));
        assert_eq!(valid_rating(10.2), None, "above the range, even if it would round to 10");
        assert_eq!(valid_rating(-0.1), None);
        assert_eq!(valid_rating(f64::NAN), None);
        assert_eq!(valid_rating(f64::INFINITY), None);
    }

    #[test]
    fn summaries_fall_back_to_parent_artwork() {
        let mut season = ItemRow::test_row(2, "season", "Season 1");
        (season.parent_id, season.parent_updated_at) = (Some(1), Some(50));
        assert_eq!(season.to_summary().poster_url, None, "the show has no poster either");
        season.parent_has_poster = true;
        assert_eq!(season.to_summary().poster_url.as_deref(), Some("/api/items/1/images/poster?v=50"));

        // Episodes show their show's poster, never their season's.
        let mut episode = ItemRow::test_row(3, "episode", "Pilot");
        (episode.parent_id, episode.grandparent_id) = (Some(2), Some(1));
        (episode.parent_updated_at, episode.grandparent_updated_at) = (Some(60), Some(50));
        episode.parent_has_poster = true;
        assert_eq!(episode.to_summary().poster_url, None);
        episode.grandparent_has_poster = true;
        assert_eq!(episode.to_summary().poster_url.as_deref(), Some("/api/items/1/images/poster?v=50"));
        // Backdrops: own, else the parent's, else the grandparent's.
        episode.grandparent_has_backdrop = true;
        assert_eq!(episode.to_summary().backdrop_url.as_deref(), Some("/api/items/1/images/backdrop?v=50"));
        episode.parent_has_backdrop = true;
        assert_eq!(episode.to_summary().backdrop_url.as_deref(), Some("/api/items/2/images/backdrop?v=60"));
        episode.has_backdrop = true;
        episode.updated_at = 70;
        assert_eq!(episode.to_summary().backdrop_url.as_deref(), Some("/api/items/3/images/backdrop?v=70"));
        // Frame grabs exist for videos only.
        assert_eq!(episode.to_summary().thumb_url.as_deref(), Some("/api/items/3/images/thumb?v=70"));
        assert_eq!(season.to_summary().thumb_url, None);

        // Albums always offer a cover (embedded art is extracted on request);
        // tracks use their album's.
        let album = ItemRow::test_row(5, "album", "Discovery");
        assert_eq!(album.to_summary().poster_url.as_deref(), Some("/api/items/5/images/poster?v=0"));
        let mut track = ItemRow::test_row(6, "track", "One More Time");
        track.parent_id = Some(5);
        assert_eq!(track.to_summary().poster_url.as_deref(), Some("/api/items/5/images/poster?v=0"));
        assert_eq!(ItemRow::test_row(7, "artist", "Daft Punk").to_summary().poster_url, None);
    }

    #[test]
    fn summaries_of_containers_and_tracks() {
        let mut show = ItemRow::test_row(1, "show", "Lost");
        show.summary = Some("Plane crash.".into());
        (show.leaf_count, show.viewed_leaf_count) = (Some(2), Some(2));
        let summary = show.to_summary();
        assert!(summary.user_state.watched, "every episode watched");
        assert_eq!(summary.summary, None, "listings leave synopses out, except for episodes");
        assert_eq!(show.to_summary_with(true).summary.as_deref(), Some("Plane crash."), "unless asked");
        assert_eq!(show.to_summary_with(false).summary, None);
        show.viewed_leaf_count = Some(1);
        assert!(!show.to_summary().user_state.watched);
        (show.leaf_count, show.viewed_leaf_count) = (Some(0), Some(0));
        assert!(!show.to_summary().user_state.watched, "a show without episodes is not watched");

        let mut episode = ItemRow::test_row(2, "episode", "Pilot");
        episode.summary = Some("Plane crash.".into());
        episode.parent_idx = Some(1);
        let summary = episode.to_summary();
        assert_eq!(summary.summary.as_deref(), Some("Plane crash."));
        assert_eq!(summary.parent_index, Some(1), "the season number");

        let mut track = ItemRow::test_row(3, "track", "Song");
        (track.disc, track.parent_idx) = (Some(2), Some(7));
        assert_eq!(track.to_summary().parent_index, Some(2), "tracks show their disc number");
    }

    /// A database with a user (1), a movie library (1) and movies 1..=`n`.
    async fn db_with_movies(dir: &std::path::Path, n: i64) -> Db {
        let db = Db::open(&dir.join("pley.db")).await.unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash, created_at) VALUES (1, 'viewer', 'x', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'Movies', 'movies', '[]', 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        for id in 1..=n {
            sqlx::query(
                "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
                 VALUES (?, 1, 'movie', ?, ?, ?, 0, 0)",
            )
            .bind(id)
            .bind(format!("movie-{id}"))
            .bind(format!("Movie {id}"))
            .bind(format!("Movie {id}"))
            .execute(&db.write)
            .await
            .unwrap();
        }
        db
    }

    /// A report of movie `id` from a player that plays it, by user 1.
    async fn report(db: &Db, id: i64, position_ms: i64, duration_ms: i64, device: Option<&str>) -> Recorded {
        record_progress(db, &EventBus::new(), 1, id, position_ms, duration_ms, device, "playing").await.unwrap()
    }

    /// User 1 marks movie `id` watched or not.
    async fn mark(db: &Db, id: i64, watched: bool) {
        set_watched(db, &EventBus::new(), 1, id, "movie", watched).await.unwrap();
    }

    /// (position_ms, watched, play_count) of movie `id`, and its history entries.
    async fn state_of(db: &Db, id: i64) -> ((i64, bool, i64), i64) {
        let state = sqlx::query_as("SELECT position_ms, watched, play_count FROM user_item_state WHERE item_id = ?")
            .bind(id)
            .fetch_optional(&db.read)
            .await
            .unwrap()
            .unwrap_or((0, false, 0));
        let plays = sqlx::query_scalar("SELECT COUNT(*) FROM play_history WHERE item_id = ?")
            .bind(id)
            .fetch_one(&db.read)
            .await
            .unwrap();
        (state, plays)
    }

    #[tokio::test]
    async fn progress_keeps_resume_points_and_counts_each_play_once() {
        let dir = crate::testing::tempdir();
        let db = db_with_movies(dir.path(), 1).await;
        let duration = 100_000;

        // The first seconds are not worth resuming.
        assert!(!report(&db, 1, 14_999, duration, None).await.counted);
        assert_eq!(state_of(&db, 1).await, ((0, false, 0), 0));
        report(&db, 1, 15_000, duration, None).await;
        assert_eq!(state_of(&db, 1).await, ((15_000, false, 0), 0));

        // 90 % is the end: watched, no resume point, one play from that device.
        assert!(report(&db, 1, 90_000, duration, Some("TV")).await.counted);
        assert_eq!(state_of(&db, 1).await, ((0, true, 1), 1));
        let device: Option<String> =
            sqlx::query_scalar("SELECT device FROM play_history").fetch_one(&db.read).await.unwrap();
        assert_eq!(device.as_deref(), Some("TV"));

        // Reports keep coming during the credits: still one play.
        assert!(!report(&db, 1, 99_000, duration, Some("TV")).await.counted);
        assert_eq!(state_of(&db, 1).await, ((0, true, 1), 1));

        // Watching it again counts again; the watched flag stays meanwhile.
        report(&db, 1, 40_000, duration, None).await;
        assert_eq!(state_of(&db, 1).await, ((40_000, true, 1), 1));
        assert!(report(&db, 1, 95_000, duration, None).await.counted);
        assert_eq!(state_of(&db, 1).await, ((0, true, 2), 2));
    }

    #[tokio::test]
    async fn progress_without_a_duration_never_completes() {
        let dir = crate::testing::tempdir();
        let db = db_with_movies(dir.path(), 1).await;
        assert!(!report(&db, 1, 7_200_000, 0, None).await.counted);
        assert_eq!(state_of(&db, 1).await, ((7_200_000, false, 0), 0));
        // Negative positions (clock glitches) are treated as the start.
        report(&db, 1, -5_000, 100_000, None).await;
        assert_eq!(state_of(&db, 1).await, ((0, false, 0), 0));
    }

    #[tokio::test]
    async fn marking_watched_by_hand_is_not_a_play() {
        let dir = crate::testing::tempdir();
        let db = db_with_movies(dir.path(), 2).await;
        report(&db, 1, 30_000, 100_000, None).await;
        mark(&db, 1, true).await;
        assert_eq!(state_of(&db, 1).await, ((0, true, 1), 0), "counted as seen, not as a play");
        // Marking it again changes nothing.
        mark(&db, 1, true).await;
        assert_eq!(state_of(&db, 1).await, ((0, true, 1), 0));
        // A play that ends after that is recorded.
        report(&db, 1, 20_000, 100_000, None).await;
        assert!(report(&db, 1, 91_000, 100_000, None).await.counted);
        assert_eq!(state_of(&db, 1).await, ((0, true, 2), 1));

        // Unwatching clears the flag and the resume point but keeps the count.
        report(&db, 2, 50_000, 100_000, None).await;
        mark(&db, 2, false).await;
        assert_eq!(state_of(&db, 2).await, ((0, false, 0), 0));
        mark(&db, 1, false).await;
        assert_eq!(state_of(&db, 1).await, ((0, false, 2), 1));
    }

    #[tokio::test]
    async fn library_pages_filter_sort_and_count() {
        let dir = crate::testing::tempdir();
        let db = db_with_movies(dir.path(), 4).await;
        for (id, year, genres, rating) in [
            (1, Some(1999), r#"["Horror","Drama"]"#, Some(7.0)),
            (2, Some(2005), r#"["Drama"]"#, None),
            (3, None, r#"["Comedy"]"#, Some(9.0)),
            (4, Some(2005), "[]", Some(5.0)),
        ] {
            sqlx::query("UPDATE items SET year = ?, genres = ?, rating = ? WHERE id = ?")
                .bind(year)
                .bind(genres)
                .bind(rating)
                .bind(id)
                .execute(&db.write)
                .await
                .unwrap();
        }
        mark(&db, 2, true).await;
        let page = |q: LibraryQuery<'static>| {
            let db = db.clone();
            async move {
                let (rows, total) = library_items(&db, 1, &q).await.unwrap();
                (rows.iter().map(|r| r.id).collect::<Vec<_>>(), total)
            }
        };
        let base = || LibraryQuery::all(1, "movie");
        assert_eq!(page(LibraryQuery { genre: Some("Drama"), ..base() }).await, (vec![1, 2], 2));
        assert_eq!(page(LibraryQuery { year: Some(2005), ..base() }).await, (vec![2, 4], 2));
        assert_eq!(page(LibraryQuery { watched: Some(true), ..base() }).await, (vec![2], 1));
        assert_eq!(page(LibraryQuery { watched: Some(false), ..base() }).await, (vec![1, 3, 4], 3));
        // Missing values sort last whichever the direction.
        let by_year = LibraryQuery { sort: SortKey::Year, ..base() };
        assert_eq!(page(LibraryQuery { descending: false, ..by_year }).await.0, vec![1, 2, 4, 3]);
        let by_year = LibraryQuery { sort: SortKey::Year, ..base() };
        assert_eq!(page(LibraryQuery { descending: true, ..by_year }).await.0, vec![2, 4, 1, 3]);
        let by_rating = LibraryQuery { sort: SortKey::Rating, descending: true, ..base() };
        assert_eq!(page(by_rating).await.0, vec![3, 1, 4, 2]);
        // Paging: the total counts every match.
        assert_eq!(page(LibraryQuery { offset: 1, limit: Some(2), ..base() }).await, (vec![2, 3], 4));
        assert_eq!(page(LibraryQuery { limit: Some(0), ..base() }).await, (vec![], 4));
        assert_eq!(page(LibraryQuery { offset: 10, ..base() }).await, (vec![], 4));
    }

    /// HDR keeps HDR10, HDR10+ and Dolby Vision; Any HDR HLG too; the rest
    /// themselves alone.
    #[test]
    fn hdr_takes_in_hdr10plus_and_dolby_vision() {
        assert_eq!(dynamic_ranges_named("hdr10"), ["dolby_vision", "hdr10plus", "hdr10"]);
        assert_eq!(dynamic_ranges_named("HDR"), ["dolby_vision", "hdr10plus", "hdr10", "hlg"]);
        assert_eq!(dynamic_ranges_named("dolby_vision"), ["dolby_vision"]);
        assert_eq!(dynamic_ranges_named("hdr10plus"), ["hdr10plus"]);
        assert_eq!(dynamic_ranges_named("hlg"), ["hlg"]);
        assert_eq!(dynamic_ranges_named("hdr12"), [] as [&str; 0]);
    }

    /// Choices that keep the same titles as a narrower one aren't offered,
    /// nor those that keep all or none.
    #[test]
    fn range_choices_each_keep_other_titles() {
        let choices = |found: &[&str]| range_choices(&found.iter().map(|r| r.to_string()).collect());
        assert_eq!(choices(&["dolby_vision", "sdr"]), ["dolby_vision", "sdr"], "HDR would be Dolby Vision");
        assert_eq!(choices(&["hdr10", "sdr"]), ["hdr10", "sdr"]);
        assert_eq!(choices(&["dolby_vision", "hdr10", "sdr"]), ["hdr10", "dolby_vision", "sdr"], "Any HDR is HDR");
        assert_eq!(
            choices(&["dolby_vision", "hdr10plus", "hdr10", "sdr"]),
            ["hdr10", "dolby_vision", "hdr10plus", "sdr"]
        );
        assert_eq!(choices(&["dolby_vision", "hdr10plus", "sdr"]), ["hdr10", "dolby_vision", "hdr10plus", "sdr"]);
        assert_eq!(
            choices(&["dolby_vision", "hdr10", "hlg", "sdr"]),
            ["hdr", "hdr10", "dolby_vision", "hlg", "sdr"],
            "Any HDR with HLG beside the rest"
        );
        assert_eq!(choices(&["dolby_vision", "hlg", "sdr"]), ["hdr", "dolby_vision", "hlg", "sdr"]);
        assert_eq!(choices(&["hlg", "sdr"]), ["hlg", "sdr"]);
        // Without SDR, HDR keeps every title: only what narrows it.
        assert_eq!(choices(&["hdr10plus", "hdr10"]), ["hdr10plus"]);
        assert_eq!(choices(&["dolby_vision", "hdr10plus"]), ["dolby_vision", "hdr10plus"]);
        // One range, none, or titles with no probed file: nothing to choose.
        assert_eq!(choices(&["dolby_vision"]), [] as [&str; 0]);
        assert_eq!(choices(&[]), [] as [&str; 0]);
        assert_eq!(choices(&["", "sdr"]), [] as [&str; 0]);
    }
}
