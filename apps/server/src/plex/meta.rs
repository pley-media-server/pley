//! pley items as Plex metadata elements: `Video` (movies, episodes),
//! `Directory` (shows, seasons, artists, albums) and `Track`, with their
//! `Media`/`Part`/`Stream`, `Genre` and `Guid` children.

pub use crate::languages::language;
pub use crate::listing::{kind_for_number, kinds_of, type_name as plex_type, type_number, view_count};
use crate::people::{Credit, Role};
use std::collections::HashMap;
use std::path::Path;

use pley_transcoder_api::{DynamicRange, MediaInfo, MediaStream, StreamKind};

use super::xml::Node;
use crate::catalog::{FileRow, ItemRow};
use crate::db::Db;
use crate::library::LibraryRow;
use crate::library::sidecar::EXTERNAL_INDEX_BASE;
use crate::markers::Marker;

/// Plex's `extraType` number and `subtype` of pley's extra types.
fn plex_extra(extra_type: &str) -> (i64, &'static str) {
    match extra_type {
        "trailer" => (1, "trailer"),
        "deleted_scene" => (2, "deletedScene"),
        "interview" => (3, "interview"),
        "behind_the_scenes" => (5, "behindTheScenes"),
        "scene" => (6, "sceneOrSample"),
        "featurette" => (10, "featurette"),
        "short" => (11, "short"),
        _ => (12, "other"),
    }
}

/// The Plex section type of a library (`movie`, `show`, `artist`).
pub fn section_type(lib: &LibraryRow) -> &'static str {
    match lib.kind() {
        crate::library::LibraryKind::Movies => "movie",
        crate::library::LibraryKind::Shows => "show",
        crate::library::LibraryKind::Music => "artist",
    }
}

pub fn is_leaf(kind: &str) -> bool {
    matches!(kind, "movie" | "episode" | "track" | "extra")
}

pub fn item_key(row: &ItemRow) -> String {
    if is_leaf(&row.kind) {
        format!("/library/metadata/{}", row.id)
    } else {
        format!("/library/metadata/{}/children", row.id)
    }
}

pub fn guid(kind: &str, id: i64) -> String {
    crate::listing::own_guid(kind, id)
}

/// Columns not carried by [`ItemRow`].
#[derive(Debug, Clone, Default)]
pub struct Extras {
    pub original_title: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub studio: Option<String>,
    pub content_rating: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    /// Shows: the TheTVDB series (`tvdb://` guid).
    pub tvdb_id: Option<i64>,
    /// Artists and albums: the MusicBrainz artist or release group (`mbid://`).
    pub musicbrainz_id: Option<String>,
    pub path: Option<String>,
    /// Cast and crew, loaded only for full details and people filters.
    pub people: Vec<Credit>,
    /// Files of the item with seek previews (full details only): their parts
    /// get `indexes="sd"` and their chapters pictures.
    pub previews: Vec<i64>,
    /// Intro and credits of the first version (`includeMarkers=1`).
    pub markers: Vec<Marker>,
    /// Labels (see `crate::labels`).
    pub labels: Vec<String>,
    /// Collections the item is in: id and title (full details only).
    pub collections: Vec<(i64, String)>,
    /// Plex names of the fields kept as edited (full details only).
    pub locked: Vec<&'static str>,
}

#[derive(sqlx::FromRow)]
struct ExtrasRow {
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
    path: Option<String>,
}

/// Extra columns of items the caller already loaded through the catalog
/// (so access rules were applied there).
pub async fn load_extras(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Extras>> {
    let mut out = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT id, original_title, tagline, genres, studio, content_rating, tmdb_id, imdb_id, tvdb_id,
                    musicbrainz_id, path
             FROM items WHERE id IN ({})",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, ExtrasRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for row in query.fetch_all(&db.read).await? {
            out.insert(
                row.id,
                Extras {
                    original_title: row.original_title,
                    tagline: row.tagline,
                    genres: row.genres.and_then(|g| serde_json::from_str(&g).ok()).unwrap_or_default(),
                    studio: row.studio,
                    content_rating: row.content_rating,
                    tmdb_id: row.tmdb_id,
                    imdb_id: row.imdb_id,
                    tvdb_id: row.tvdb_id,
                    musicbrainz_id: row.musicbrainz_id,
                    path: row.path,
                    people: Vec::new(),
                    previews: Vec::new(),
                    markers: Vec::new(),
                    labels: Vec::new(),
                    collections: Vec::new(),
                    locked: Vec::new(),
                },
            );
        }
    }
    for (id, labels) in crate::labels::for_items(db, ids).await? {
        out.entry(id).or_default().labels = labels;
    }
    Ok(out)
}

/// Plex's name of a field pley keeps as edited (see `crate::overrides`).
fn plex_field(field: &str) -> Option<&'static str> {
    Some(match field {
        "title" => "title",
        "sort_title" => "titleSort",
        "original_title" => "originalTitle",
        "year" => "year",
        "summary" => "summary",
        "tagline" => "tagline",
        "studio" => "studio",
        "content_rating" => "contentRating",
        "release_date" => "originallyAvailableAt",
        "genres" => "genre",
        "poster" | "thumb" => "thumb",
        "backdrop" => "art",
        _ => return None,
    })
}

/// The locked fields of `ids` (Plex names), by item.
pub async fn load_locked(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<&'static str>>> {
    let mut out: HashMap<i64, Vec<&'static str>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT item_id, field FROM item_overrides WHERE item_id IN ({}) ORDER BY field",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, (i64, String)>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for (item, field) in query.fetch_all(&db.read).await? {
            if let Some(name) = plex_field(&field) {
                let locked = out.entry(item).or_default();
                if !locked.contains(&name) {
                    locked.push(name);
                }
            }
        }
    }
    Ok(out)
}

/// Collections `ids` are in, by item: id and title.
pub async fn load_collections(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<(i64, String)>>> {
    let mut out: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT ci.item_id, c.id, c.title FROM collection_items ci JOIN collections c ON c.id = ci.collection_id
             WHERE ci.item_id IN ({}) ORDER BY c.sort_title COLLATE NOCASE",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, (i64, i64, String)>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for (item, id, title) in query.fetch_all(&db.read).await? {
            out.entry(item).or_default().push((id, title));
        }
    }
    Ok(out)
}

/// Intro and credits markers of these files, by file.
pub async fn load_markers(db: &Db, file_ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<Marker>>> {
    let mut out: HashMap<i64, Vec<Marker>> = HashMap::new();
    for chunk in file_ids.chunks(500) {
        let sql = format!(
            "SELECT file_id, kind, start_ms, end_ms, source FROM markers WHERE file_id IN ({}) ORDER BY start_ms",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, Marker>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for marker in query.fetch_all(&db.read).await? {
            out.entry(marker.file_id).or_default().push(marker);
        }
    }
    Ok(out)
}

/// Plex's marker id: unique per file and kind.
fn marker_id(marker: &Marker) -> i64 {
    marker.file_id * 10 + if marker.kind == "credits" { 2 } else { 1 }
}

/// Media files of already loaded items, best quality first (like
/// `catalog::files_for_item`).
pub async fn load_files(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<FileRow>>> {
    let mut out: HashMap<i64, Vec<FileRow>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT id, item_id, library_id, path, size, mtime, container, duration_ms, bitrate, width, height,
                    video_codec, audio_codec, media_info, external_subs, fingerprint, {}
             FROM media_files WHERE item_id IN ({}) ORDER BY item_id, COALESCE(height, 0) DESC, id",
            crate::added_subs_column!(),
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

/// Library names and types, for `librarySection*` attributes.
#[derive(Default)]
pub struct Sections(pub HashMap<i64, (String, &'static str)>);

impl Sections {
    pub fn new(libraries: &[LibraryRow]) -> Self {
        Sections(libraries.iter().map(|l| (l.id, (l.name.clone(), section_type(l)))).collect())
    }
}

/// What to include in item elements.
#[derive(Clone, Copy, Default)]
pub struct Detail {
    /// `Guid` children (`includeGuids=1`; always in details).
    pub guids: bool,
    /// Full details: every genre, `Stream`s, `Location`.
    pub full: bool,
    /// `Media`/`Part` of playable items (listings and details).
    pub media: bool,
    /// `Marker`s: intro and credits (`includeMarkers=1` in details).
    pub markers: bool,
}

/// `/api/items/{id}/images/{kind}?v={v}` → `(id, version)`.
fn image_owner(url: &str) -> Option<(i64, String)> {
    let rest = url.strip_prefix("/api/items/")?;
    let (id, rest) = rest.split_once('/')?;
    let version = rest.split_once("?v=").map(|(_, v)| v.to_owned()).unwrap_or_default();
    Some((id.parse().ok()?, version))
}

fn plex_image(url: Option<&str>, kind: &str) -> Option<String> {
    let (id, version) = image_owner(url?)?;
    Some(if version.is_empty() {
        format!("/library/metadata/{id}/{kind}")
    } else {
        format!("/library/metadata/{id}/{kind}/{version}")
    })
}

/// Plex keeps case in `titleSort`; pley's sort title is lowercase.
fn title_sort(row: &ItemRow) -> Option<String> {
    let lower = row.title.to_lowercase();
    if lower == row.sort_title {
        return None;
    }
    let trimmed = row.title.trim();
    let cut = trimmed.char_indices().map(|(i, _)| i).find(|&i| trimmed[i..].to_lowercase() == row.sort_title);
    Some(cut.map(|i| trimmed[i..].to_owned()).unwrap_or_else(|| row.sort_title.clone()))
}

pub fn item_node(
    row: &ItemRow,
    extras: Option<&Extras>,
    files: &[FileRow],
    sections: &Sections,
    detail: Detail,
) -> Node {
    let kind = row.kind.as_str();
    let tag = match kind {
        "movie" | "episode" | "extra" => "Video",
        "track" => "Track",
        _ => "Directory",
    };
    let summary = row.to_summary();
    let (section_title, _) = sections.0.get(&row.library_id).cloned().unwrap_or_default();
    let mut node = Node::item(tag)
        .attr("ratingKey", row.id.to_string())
        .attr("key", item_key(row))
        .attr("guid", guid(kind, row.id))
        .attr("type", plex_type(kind))
        .attr("title", &row.title)
        .opt("titleSort", title_sort(row))
        .attr("librarySectionID", row.library_id)
        .attr("librarySectionTitle", section_title)
        .attr("librarySectionKey", format!("/library/sections/{}", row.library_id));
    if let Some(extra_type) = row.extra_type.as_deref() {
        let (number, subtype) = plex_extra(extra_type);
        node = node.attr("extraType", number).attr("subtype", subtype);
    }

    if let Some(extras) = extras {
        node = node
            .opt("originalTitle", extras.original_title.as_deref())
            .opt("studio", extras.studio.as_deref())
            .opt("contentRating", extras.content_rating.as_deref());
        if detail.full {
            node = node.opt("tagline", extras.tagline.as_deref());
        }
    }
    node = node.opt("summary", row.summary.as_deref()).opt("year", row.year);
    if let Some(rating) = row.rating {
        node = node
            .attr("rating", rating)
            .attr("audienceRating", rating)
            .attr("audienceRatingImage", "themoviedb://image.rating");
    }
    node = node.opt("userRating", row.user_rating);
    // The edition of the version played by default, as Plex names it.
    if kind == "movie" {
        let edition = files.first().and_then(|f| crate::library::parse::edition(std::path::Path::new(&f.path)));
        node = node.opt("editionTitle", edition);
    }
    node = node
        .opt("originallyAvailableAt", row.release_date.as_deref())
        .opt("duration", row.duration_ms.filter(|d| *d > 0))
        .attr("addedAt", row.added_at)
        .attr("updatedAt", row.updated_at);

    // Watch state.
    match kind {
        "show" | "season" | "artist" | "album" => {
            if let Some(leaves) = row.leaf_count {
                node = node.attr("leafCount", leaves);
                if matches!(kind, "show" | "season") {
                    node = node.attr("viewedLeafCount", row.viewed_leaf_count.unwrap_or(0));
                }
            }
            node = node.opt("childCount", row.child_count);
            if let Some(at) = row.last_played_at {
                node = node.attr("lastViewedAt", at);
            }
        }
        _ => {
            let count = view_count(row);
            if count > 0 {
                node = node.attr("viewCount", count);
            }
            if row.position_ms > 0 && !row.watched {
                node = node.attr("viewOffset", row.position_ms);
            }
            if let Some(at) = row.last_played_at {
                node = node.attr("lastViewedAt", at);
            }
        }
    }

    // Hierarchy and artwork.
    let parent_key = row.parent_id.map(|p| format!("/library/metadata/{p}"));
    let grandparent_key = row.grandparent_id.map(|g| format!("/library/metadata/{g}"));
    let own_poster = plex_image(summary.poster_url.as_deref(), "thumb");
    let art = plex_image(summary.backdrop_url.as_deref(), "art");
    let parent_thumb = row
        .parent_id
        .filter(|_| row.parent_has_poster)
        .map(|p| format!("/library/metadata/{p}/thumb/{}", row.parent_updated_at.unwrap_or(0)));
    let grandparent_thumb = row
        .grandparent_id
        .filter(|_| row.grandparent_has_poster)
        .map(|g| format!("/library/metadata/{g}/thumb/{}", row.grandparent_updated_at.unwrap_or(0)));
    let grandparent_art = row
        .grandparent_id
        .filter(|_| row.grandparent_has_backdrop)
        .map(|g| format!("/library/metadata/{g}/art/{}", row.grandparent_updated_at.unwrap_or(0)));
    match kind {
        "movie" | "show" | "artist" => {
            node = node.opt("thumb", own_poster).opt("art", art);
            if kind == "show" {
                node = node.attr("index", 1);
            }
        }
        "season" => {
            node = node
                .opt("index", row.idx)
                .opt("parentRatingKey", row.parent_id.map(|p| p.to_string()))
                .opt("parentKey", parent_key)
                .opt("parentGuid", row.parent_id.map(|p| guid("show", p)))
                .opt("parentTitle", row.parent_title.as_deref())
                .opt("parentThumb", parent_thumb)
                .opt("thumb", own_poster)
                .opt("art", art);
        }
        "episode" => {
            let still = plex_image(summary.thumb_url.as_deref(), "thumb");
            node = node
                .opt("index", row.idx)
                .opt("parentIndex", row.parent_idx)
                .opt("parentRatingKey", row.parent_id.map(|p| p.to_string()))
                .opt("parentKey", parent_key)
                .opt("parentGuid", row.parent_id.map(|p| guid("season", p)))
                .opt("parentTitle", row.parent_title.as_deref())
                .opt("parentThumb", parent_thumb.or_else(|| grandparent_thumb.clone()))
                .opt("grandparentRatingKey", row.grandparent_id.map(|g| g.to_string()))
                .opt("grandparentKey", grandparent_key)
                .opt("grandparentGuid", row.grandparent_id.map(|g| guid("show", g)))
                .opt("grandparentTitle", row.grandparent_title.as_deref())
                .opt("grandparentThumb", grandparent_thumb)
                .opt("grandparentArt", grandparent_art)
                .opt("thumb", still)
                .opt("art", art);
        }
        "album" => {
            node = node
                .opt("parentRatingKey", row.parent_id.map(|p| p.to_string()))
                .opt("parentKey", parent_key)
                .opt("parentGuid", row.parent_id.map(|p| guid("artist", p)))
                .opt("parentTitle", row.parent_title.as_deref())
                .opt("parentThumb", parent_thumb)
                .opt("thumb", own_poster)
                .opt("art", art);
        }
        _ => {
            // Track: album is the parent, artist the grandparent.
            node = node
                .opt("index", row.idx)
                .opt("parentIndex", row.disc.or(Some(1)))
                .opt("parentRatingKey", row.parent_id.map(|p| p.to_string()))
                .opt("parentKey", parent_key)
                .opt("parentGuid", row.parent_id.map(|p| guid("album", p)))
                .opt("parentTitle", row.parent_title.as_deref())
                .opt("parentThumb", own_poster.clone())
                .opt("grandparentRatingKey", row.grandparent_id.map(|g| g.to_string()))
                .opt("grandparentKey", grandparent_key)
                .opt("grandparentGuid", row.grandparent_id.map(|g| guid("artist", g)))
                .opt("grandparentTitle", row.grandparent_title.as_deref())
                .opt("grandparentThumb", grandparent_thumb)
                .opt("grandparentArt", grandparent_art)
                .opt("thumb", own_poster)
                .opt("art", art);
        }
    }

    // Children elements.
    if detail.media && is_leaf(kind) {
        for file in files {
            let previews = extras.is_some_and(|e| e.previews.contains(&file.id));
            node.push(media_node(file, kind, detail.full, previews));
        }
        // Chapters of the first version, with pictures once it has previews.
        if let Some(file) = files.first().filter(|_| detail.full) {
            let previews = extras.is_some_and(|e| e.previews.contains(&file.id));
            let chapters = file.media_info().map(|m| m.chapters).unwrap_or_default();
            for (n, chapter) in chapters.iter().enumerate().filter(|_| chapters.len() > 1) {
                let title =
                    chapter.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or(format!("Chapter {}", n + 1));
                let mut node_chapter = Node::new("Chapter")
                    .attr("id", n + 1)
                    .attr("tag", title)
                    .attr("index", n + 1)
                    .attr("startTimeOffset", chapter.start_ms)
                    .attr("endTimeOffset", chapter.end_ms);
                if previews {
                    node_chapter = node_chapter
                        .attr("filter", format!("thumb={}", n + 1))
                        .attr("thumb", format!("/library/media/{}/chapterImages/{n}", file.id));
                }
                node.push(node_chapter);
            }
        }
    }
    // Intro and credits of the first version, as Plex marks them.
    if detail.markers && is_leaf(kind) {
        for marker in extras.map(|e| e.markers.as_slice()).unwrap_or_default() {
            node.push(
                Node::new("Marker")
                    .attr("id", marker_id(marker))
                    .attr("type", &marker.kind)
                    .attr("startTimeOffset", marker.start_ms)
                    .attr("endTimeOffset", marker.end_ms)
                    // pley's credits run to the end: the last marker.
                    .opt("final", (marker.kind == "credits").then_some(1)),
            );
        }
    }
    if let Some(extras) = extras {
        let limit = if detail.full { usize::MAX } else { 3 };
        for genre in extras.genres.iter().take(limit) {
            node.push(Node::new("Genre").attr("tag", genre));
        }
        if detail.guids || detail.full {
            if let Some(imdb) = extras.imdb_id.as_deref().filter(|i| !i.is_empty()) {
                node.push(Node::new("Guid").attr("id", format!("imdb://{imdb}")));
            }
            if let Some(tmdb) = extras.tmdb_id.filter(|_| matches!(kind, "movie" | "show")) {
                node.push(Node::new("Guid").attr("id", format!("tmdb://{tmdb}")));
            }
            // What Plex reports too (Overseerr and Kometa match shows by it).
            if let Some(tvdb) = extras.tvdb_id.filter(|_| kind == "show") {
                node.push(Node::new("Guid").attr("id", format!("tvdb://{tvdb}")));
            }
            if let Some(mbid) = extras.musicbrainz_id.as_deref().filter(|_| matches!(kind, "artist" | "album")) {
                node.push(Node::new("Guid").attr("id", format!("mbid://{mbid}")));
            }
        }
        if detail.full
            && !is_leaf(kind)
            && let Some(path) = extras.path.as_deref()
        {
            node.push(Node::new("Location").attr("path", path));
        }
        if detail.full {
            for credit in &extras.people {
                node.push(credit_node(credit));
            }
            for (id, title) in &extras.collections {
                node.push(Node::new("Collection").attr("id", *id).attr("tag", title));
            }
            for label in &extras.labels {
                node.push(Node::new("Label").attr("tag", label));
            }
            for name in &extras.locked {
                node.push(Node::new("Field").attr("locked", 1).attr("name", *name));
            }
        }
    }
    node
}

/// Plex's tag element for a credit (`Role`, `Director`, …); the tag id is the
/// person id, which the `actor=`/`director=`… filters accept.
fn credit_node(credit: &Credit) -> Node {
    let (tag, filter) = match credit.role {
        Role::Actor => ("Role", "actor"),
        Role::Director => ("Director", "director"),
        Role::Writer => ("Writer", "writer"),
        Role::Producer => ("Producer", "producer"),
        // Plex has no tag for show creators.
        Role::Creator => ("Producer", "producer"),
    };
    Node::new(tag)
        .attr("id", credit.id)
        .attr("filter", format!("{filter}={}", credit.id))
        .attr("tag", &credit.name)
        .attr("tagKey", credit.id.to_string())
        .opt("role", credit.character.as_deref())
        .opt("thumb", credit.photo_source.as_deref().filter(|u| crate::store::allowed_image_url(u)))
}

// ------------------------------------------------------------------ media

/// Plex codec names (`dca` for DTS, `pcm` for every PCM variant, …).
pub fn codec_name(codec: &str) -> String {
    match codec {
        "dts" => "dca".into(),
        "subrip" => "srt".into(),
        "hdmv_pgs_subtitle" => "pgs".into(),
        "dvd_subtitle" => "vobsub".into(),
        "dvb_subtitle" => "dvb_subtitle".into(),
        "webvtt" => "vtt".into(),
        "mov_text" => "mov_text".into(),
        c if c.starts_with("pcm_") => "pcm".into(),
        c => c.to_ascii_lowercase(),
    }
}

pub fn resolution_label(width: Option<i64>, height: Option<i64>) -> Option<&'static str> {
    let (w, h) = (width.unwrap_or(0), height.unwrap_or(0));
    if w == 0 && h == 0 {
        return None;
    }
    Some(match () {
        _ if w >= 3200 || h >= 1800 => "4k",
        _ if w >= 1700 || h >= 1000 => "1080",
        _ if w >= 1200 || h >= 700 => "720",
        _ if h >= 560 => "576",
        _ if h >= 400 => "480",
        _ => "sd",
    })
}

fn frame_rate_label(fps: Option<f64>) -> Option<&'static str> {
    let fps = fps?;
    Some(match fps {
        f if f < 24.5 => "24p",
        f if f < 26.0 => "PAL",
        f if f < 31.0 => "NTSC",
        f if f < 49.0 => "48p",
        f if f < 55.0 => "50p",
        _ => "60p",
    })
}

fn extension(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_else(|| "bin".into())
}

pub fn part_key(file: &FileRow) -> String {
    format!("/library/parts/{}/{}/file.{}", file.id, file.mtime, extension(&file.path))
}

/// Plex stream ids are global: derived from the file and stream index.
pub fn stream_id(file_id: i64, index: i64) -> i64 {
    file_id * 10_000 + index
}

/// The file stream index behind a Plex stream id of `file_id`.
pub fn stream_index(file_id: i64, stream_id: i64) -> Option<i64> {
    let index = stream_id.checked_sub(file_id.checked_mul(10_000)?)?;
    (0..10_000).contains(&index).then_some(index)
}

pub fn media_node(file: &FileRow, kind: &str, full: bool, previews: bool) -> Node {
    let info = file.media_info();
    let video = info.as_ref().and_then(MediaInfo::video);
    let audio = info.as_ref().and_then(MediaInfo::default_audio);
    let (width, height) = (file.width.or(video.and_then(|v| v.width)), file.height.or(video.and_then(|v| v.height)));
    let aspect = match (width, height) {
        (Some(w), Some(h)) if h > 0 => Some(((w as f64 / h as f64) * 100.0).round() / 100.0),
        _ => None,
    };
    let container = file.container.clone().or_else(|| Some(extension(&file.path)));
    let mut media = Node::new("Media")
        .attr("id", file.id)
        .opt("duration", file.duration_ms)
        .opt("bitrate", file.bitrate.map(|b| b / 1000))
        .opt("container", container.clone());
    if kind != "track" {
        media = media
            .opt("width", width)
            .opt("height", height)
            .opt("aspectRatio", aspect)
            .opt("videoCodec", video.map(|v| codec_name(&v.codec)).or(file.video_codec.clone()))
            .opt("videoResolution", resolution_label(width, height))
            .opt("videoFrameRate", frame_rate_label(video.and_then(|v| v.frame_rate)))
            .opt("videoProfile", video.and_then(|v| v.profile.as_deref()).map(str::to_lowercase));
    }
    media = media
        .opt("audioChannels", audio.and_then(|a| a.channels))
        .opt("audioCodec", audio.map(|a| codec_name(&a.codec)).or(file.audio_codec.clone()))
        .opt("audioProfile", audio.and_then(|a| a.profile.as_deref()).map(str::to_lowercase))
        .attr("optimizedForStreaming", matches!(file.container.as_deref(), Some("mp4" | "m4v")))
        .attr("has64bitOffsets", false);

    let mut part = Node::new("Part")
        .attr("id", file.id)
        .attr("key", part_key(file))
        .opt("duration", file.duration_ms)
        .attr("file", &file.path)
        .attr("size", file.size)
        .opt("container", container)
        .opt("videoProfile", video.and_then(|v| v.profile.as_deref()).map(str::to_lowercase))
        .opt("audioProfile", audio.and_then(|a| a.profile.as_deref()).map(str::to_lowercase));
    if previews {
        part = part.attr("indexes", "sd");
    }
    if full {
        let default_audio = audio.map(|a| a.index);
        for stream in file.streams() {
            part.push(stream_node(file, &stream, default_audio));
        }
    }
    media.child(part)
}

fn stream_node(file: &FileRow, s: &MediaStream, default_audio: Option<i64>) -> Node {
    let id = stream_id(file.id, s.index);
    let lang = s.language.as_deref().and_then(language);
    let (stream_type, selected) = match s.kind {
        StreamKind::Video => (1, true),
        StreamKind::Audio => (2, Some(s.index) == default_audio),
        StreamKind::Subtitle => (3, crate::streams::is_forced(s) && s.default),
    };
    let codec = codec_name(&s.codec);
    let mut node = Node::new("Stream").attr("id", id).attr("streamType", stream_type);
    if !s.external {
        node = node.attr("index", s.index);
    }
    node = node
        .attr("codec", codec.clone())
        .attr("default", s.default)
        .attr("selected", selected)
        .opt("title", s.title.as_deref())
        .opt("language", lang.map(|l| l.2))
        .opt("languageCode", lang.map(|l| l.0))
        .opt("languageTag", lang.map(|l| l.1))
        .opt("bitrate", s.bitrate.map(|b| b / 1000));
    let lang_name = lang.map(|l| l.2).unwrap_or("Unknown");
    match s.kind {
        StreamKind::Video => {
            let res =
                resolution_label(s.width, s.height).map(|r| if r == "4k" { "4K".into() } else { format!("{r}p") });
            let range = s.dynamic_range();
            let range_label = match range {
                DynamicRange::Sdr => None,
                // What the base layer shows without Dolby Vision, as Plex
                // names it.
                DynamicRange::DolbyVision => Some(format!(
                    "DoVi/{}",
                    match s.dv_bl_compat {
                        _ if s.is_dolby_vision_only() => "None",
                        Some(4) => "HLG",
                        Some(2) => "SDR",
                        _ if s.hdr10plus == Some(true) => "HDR10+",
                        _ => "HDR10",
                    }
                )),
                other => Some(other.label().to_owned()),
            };
            let mut display = res.clone().unwrap_or_default();
            let codec_label = codec.to_uppercase().replace("H264", "H.264");
            display = format!("{display} ({codec_label}{})", range_label.map(|r| format!(" {r}")).unwrap_or_default())
                .trim()
                .to_owned();
            node = node
                .opt("width", s.width)
                .opt("height", s.height)
                .opt("bitDepth", s.bit_depth)
                .opt("frameRate", s.frame_rate.map(|f| (f * 1000.0).round() / 1000.0))
                .opt("profile", s.profile.as_deref().map(str::to_lowercase))
                .opt("level", s.level)
                .opt("colorPrimaries", s.color_primaries.as_deref())
                .opt("colorTrc", s.color_transfer.as_deref())
                .opt("colorSpace", s.color_space.as_deref())
                .opt("scanType", s.interlaced.map(|i| if i { "interlaced" } else { "progressive" }))
                .attr("displayTitle", display.clone())
                .attr("extendedDisplayTitle", display);
            if range == DynamicRange::DolbyVision {
                node = node
                    .attr("DOVIPresent", true)
                    .opt("DOVIProfile", s.dv_profile)
                    .opt("DOVILevel", s.dv_level)
                    .opt("DOVIBLCompatID", s.dv_bl_compat)
                    .attr("DOVIBLPresent", true)
                    .attr("DOVIELPresent", s.dv_profile == Some(7))
                    .attr("DOVIRPUPresent", true);
            }
        }
        StreamKind::Audio => {
            let layout = s.channels.map(channel_label);
            let display = format!(
                "{lang_name} ({}{})",
                codec.to_uppercase(),
                layout.as_deref().map(|l| format!(" {l}")).unwrap_or_default()
            );
            let extended = match s.title.as_deref() {
                Some(title) => format!(
                    "{title} ({lang_name} {}{})",
                    codec.to_uppercase(),
                    layout.as_deref().map(|l| format!(" {l}")).unwrap_or_default()
                ),
                None => display.clone(),
            };
            node = node
                .opt("channels", s.channels)
                .opt("audioChannelLayout", s.channel_layout.as_deref())
                .opt("samplingRate", s.sample_rate)
                .opt("profile", s.profile.as_deref().map(str::to_lowercase))
                .attr("displayTitle", display)
                .attr("extendedDisplayTitle", extended);
        }
        StreamKind::Subtitle => {
            // Flagged, or only named so ("English (Forced)", "Signs & Songs"), as pley's own apps count them.
            let forced = crate::streams::is_forced(s);
            let label = if forced { format!("{lang_name} Forced") } else { lang_name.to_owned() };
            let display = format!("{label} ({})", codec.to_uppercase());
            node = node.attr("forced", forced).attr("displayTitle", display.clone()).attr(
                "extendedDisplayTitle",
                s.title.as_deref().map(|t| format!("{t} ({display})")).unwrap_or(display),
            );
            if s.external || s.index >= EXTERNAL_INDEX_BASE {
                node = node.attr("key", format!("/library/streams/{id}")).attr("format", codec);
            }
        }
    }
    node
}

pub fn channel_label(channels: i64) -> String {
    match channels {
        1 => "Mono".into(),
        2 => "Stereo".into(),
        3 => "2.1".into(),
        6 => "5.1".into(),
        7 => "6.1".into(),
        8 => "7.1".into(),
        n => format!("{n}ch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolutions_and_stream_ids() {
        assert_eq!(resolution_label(Some(3840), Some(1600)), Some("4k"));
        assert_eq!(resolution_label(Some(1920), Some(800)), Some("1080"));
        assert_eq!(resolution_label(Some(1280), Some(720)), Some("720"));
        assert_eq!(resolution_label(Some(720), Some(480)), Some("480"));
        assert_eq!(resolution_label(None, None), None);
        assert_eq!(stream_index(7, stream_id(7, 3)), Some(3));
        assert_eq!(stream_index(7, stream_id(8, 3)), None);
        assert_eq!(codec_name("dts"), "dca");
        assert_eq!(codec_name("pcm_s24le"), "pcm");
    }

    #[test]
    fn image_urls() {
        assert_eq!(
            plex_image(Some("/api/items/12/images/poster?v=99"), "thumb").as_deref(),
            Some("/library/metadata/12/thumb/99")
        );
        assert_eq!(plex_image(None, "art"), None);
    }
}
