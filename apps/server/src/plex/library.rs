//! Library browsing: sections, listings with Plex filters, sorting and
//! paging, metadata, hierarchy, hubs, search and artwork.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;

use super::meta::{self, Detail, Extras, Sections, kind_for_number, kinds_of, plex_type, section_type, type_number};
use super::{Node, Page, Params, PlexAdmin, PlexError, PlexInfo, PlexResult, PlexUser};
use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow, ItemsQuery, LibraryQuery, ME_CTE, SqlArg, VISIBLE, VISIBLE_ITEMS};
use crate::db::Db;
use crate::events::ServerEvent;
use crate::library::LibraryRow;
use crate::people;
use crate::state::AppState;
use crate::util::{date_secs, now};

/// Libraries the user can see (per-user library access).
pub(super) async fn visible_libraries(state: &AppState, user: &AuthUser) -> PlexResult<Vec<LibraryRow>> {
    Ok(crate::access::visible_libraries(&state.db, user).await?)
}

pub(super) async fn library_for(
    state: &AppState,
    user: &AuthUser,
    id: &str,
) -> PlexResult<(LibraryRow, Vec<LibraryRow>)> {
    let id: i64 = id.parse().map_err(|_| PlexError::NotFound)?;
    let libraries = visible_libraries(state, user).await?;
    let lib = libraries.iter().find(|l| l.id == id).cloned().ok_or(PlexError::NotFound)?;
    Ok((lib, libraries))
}

pub(super) fn base_container() -> Node {
    Node::container()
        .attr("allowSync", false)
        .attr("identifier", "com.plexapp.plugins.library")
        .attr("mediaTagPrefix", "/system/bundle/media/flags/")
        .attr("mediaTagVersion", 1)
}

/// A paged list of items.
pub(super) fn list_container(
    lib: Option<&LibraryRow>,
    info: &PlexInfo,
    nodes: Vec<Node>,
    total: usize,
    page: &Page,
) -> Node {
    let mut mc = base_container().attr("size", nodes.len()).attr("totalSize", total).attr("offset", page.start);
    if let Some(lib) = lib {
        mc = mc
            .attr("librarySectionID", lib.id)
            .attr("librarySectionTitle", &lib.name)
            .attr("librarySectionUUID", info.section_uuid(lib.id))
            .attr("title1", &lib.name);
    }
    mc.extend(nodes);
    mc
}

/// Item elements for rows loaded through the catalog.
pub(super) async fn item_nodes(
    state: &AppState,
    rows: &[ItemRow],
    sections: &Sections,
    detail: Detail,
) -> PlexResult<Vec<Node>> {
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut extras = meta::load_extras(&state.db, &ids).await?;
    if detail.full {
        add_people(&state.db, &ids, &mut extras).await?;
        for (id, collections) in meta::load_collections(&state.db, &ids).await? {
            extras.entry(id).or_default().collections = collections;
        }
        for (id, locked) in meta::load_locked(&state.db, &ids).await? {
            extras.entry(id).or_default().locked = locked;
        }
    }
    let leaves: Vec<i64> = rows.iter().filter(|r| meta::is_leaf(&r.kind)).map(|r| r.id).collect();
    let files =
        if detail.media && !leaves.is_empty() { meta::load_files(&state.db, &leaves).await? } else { HashMap::new() };
    if detail.full {
        for (item, item_files) in &files {
            let with_previews: Vec<i64> =
                item_files.iter().filter(|f| state.trickplay.index(f).is_some()).map(|f| f.id).collect();
            if !with_previews.is_empty() {
                extras.entry(*item).or_default().previews = with_previews;
            }
        }
    }
    if detail.markers {
        // The first version's: the one played unless the client picks another.
        let first: Vec<(i64, i64)> = files.iter().filter_map(|(item, f)| Some((*item, f.first()?.id))).collect();
        let file_ids: Vec<i64> = first.iter().map(|(_, file)| *file).collect();
        let mut markers = meta::load_markers(&state.db, &file_ids).await?;
        for (item, file) in first {
            if let Some(found) = markers.remove(&file) {
                extras.entry(item).or_default().markers = found;
            }
        }
    }
    Ok(rows
        .iter()
        .map(|r| {
            let files = files.get(&r.id).map(Vec::as_slice).unwrap_or(&[]);
            meta::item_node(r, extras.get(&r.id), files, sections, detail)
        })
        .collect())
}

/// Cast and crew of `ids` into their extras (seasons get their show's).
async fn add_people(db: &Db, ids: &[i64], extras: &mut HashMap<i64, Extras>) -> sqlx::Result<()> {
    for (id, credits) in people::for_items(db, ids).await? {
        extras.entry(id).or_default().people = credits;
    }
    Ok(())
}

pub(super) fn listing_detail(params: &Params) -> Detail {
    Detail { guids: params.flag("includeGuids"), full: false, media: true, markers: false }
}

// ------------------------------------------------------------------ sections

pub async fn root(_user: PlexUser) -> Node {
    base_container()
        .attr("size", 3)
        .attr("title1", "Plex Library")
        .attr("title2", "")
        .child(Node::new("Directory").attr("key", "sections").attr("title", "Library Sections"))
        .child(Node::new("Directory").attr("key", "recentlyAdded").attr("title", "Recently Added Content"))
        .child(Node::new("Directory").attr("key", "onDeck").attr("title", "On Deck Content"))
}

fn section_node(state: &AppState, info: &PlexInfo, lib: &LibraryRow) -> Node {
    let (agent, scanner) = match section_type(lib) {
        "movie" => ("tv.plex.agents.movie", "Plex Movie"),
        "show" => ("tv.plex.agents.series", "Plex TV Series"),
        _ => ("tv.plex.agents.music", "Plex Music"),
    };
    let changed = lib.scanned_at.unwrap_or(lib.created_at);
    let mut node = Node::new("Directory")
        .attr("allowSync", false)
        .attr("filters", true)
        .attr("refreshing", state.scanner.is_scanning(lib.id))
        .attr("key", lib.id.to_string())
        .attr("type", section_type(lib))
        .attr("title", &lib.name)
        .attr("agent", agent)
        .attr("scanner", scanner)
        .attr("language", "en-US")
        .attr("uuid", info.section_uuid(lib.id))
        .attr("updatedAt", changed)
        .attr("createdAt", lib.created_at)
        .opt("scannedAt", lib.scanned_at)
        .attr("content", true)
        .attr("directory", true)
        .attr("contentChangedAt", changed)
        .attr("hidden", 0);
    for (i, path) in lib.paths().iter().enumerate() {
        node.push(
            Node::new("Location")
                .attr("id", lib.id * 1000 + i as i64)
                .attr("path", path.to_string_lossy().into_owned()),
        );
    }
    node
}

pub async fn sections(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let mut mc = base_container().attr("size", libraries.len()).attr("title1", "Plex Library");
    for lib in &libraries {
        mc.push(section_node(&state, &info, lib));
    }
    Ok(mc)
}

pub async fn section(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let top = lib.kind().top_level_kind();
    let plural = match top {
        "movie" => "Movies",
        "show" => "Shows",
        _ => "Artists",
    };
    let directory = |key: String, title: &str| Node::new("Directory").attr("key", key).attr("title", title);
    let mut mc = base_container()
        .attr("content", "secondary")
        .attr("librarySectionID", lib.id)
        .attr("librarySectionUUID", info.section_uuid(lib.id))
        .attr("title1", &lib.name)
        .attr("viewGroup", "secondary");
    mc.push(directory("all".into(), &format!("All {plural}")));
    mc.push(directory("recentlyAdded".into(), "Recently Added"));
    if top != "artist" {
        mc.push(directory("onDeck".into(), "On Deck"));
    }
    for (filter, title) in [("genre", "By Genre"), ("year", "By Year"), ("decade", "By Decade")] {
        mc.push(directory(filter.into(), title).attr("secondary", true));
    }
    mc.push(
        directory(format!("search?type={}", type_number(top)), &format!("Search {plural}..."))
            .attr("prompt", format!("Search {plural}"))
            .attr("search", true),
    );
    let size = mc.children().len();
    mc.set("size", size);
    Ok(mc)
}

pub async fn refresh_all(
    State(state): State<AppState>,
    PlexAdmin(_admin): PlexAdmin,
    params: Params,
) -> PlexResult<Node> {
    state.scanner.enqueue_all(params.flag("force")).await?;
    Ok(Node::container().attr("size", 0))
}

pub async fn refresh(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    state.scanner.enqueue(lib.id, params.flag("force"));
    Ok(Node::container().attr("size", 0))
}

pub async fn refresh_item(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<i64>,
) -> PlexResult<Node> {
    let row = catalog::item(&state.db, admin.id, id).await?.ok_or(PlexError::NotFound)?;
    if state.metadata.enabled() {
        let metadata = state.metadata.clone();
        let events = state.events.clone();
        tokio::spawn(async move {
            match metadata.refresh_item(id).await {
                Ok(()) => events.send(ServerEvent::LibraryChanged { library_id: row.library_id }),
                Err(err) => tracing::warn!("metadata refresh for item {id} failed: {err:#}"),
            }
        });
    }
    Ok(Node::container().attr("size", 0))
}

// ------------------------------------------------------------------ filtering and sorting

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `field=v`: equals (numbers, tags) or contains (strings).
    Is,
    /// `field!=v`
    Not,
    /// `field>>=v`
    Greater,
    /// `field<<=v`
    Less,
    /// `field<=v` (strings)
    Begins,
    /// `field>=v` (strings)
    Ends,
    /// `field==v` (strings)
    Exactly,
    /// `field!==v` (strings)
    NotExactly,
}

/// Which item a filter looks at: the listed one or an ancestor
/// (`artist.id=5` on albums, `show.title=…` on episodes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Own,
    Parent,
    Grandparent,
}

fn level_of(prefix: &str, kind: &str) -> Option<Level> {
    let own = plex_type(kind);
    if prefix == own {
        return Some(Level::Own);
    }
    let (parent, grandparent) = match own {
        "season" => ("show", ""),
        "episode" => ("season", "show"),
        "album" => ("artist", ""),
        "track" => ("album", "artist"),
        _ => ("", ""),
    };
    if prefix == parent {
        Some(Level::Parent)
    } else if !grandparent.is_empty() && prefix == grandparent {
        Some(Level::Grandparent)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct Filter {
    level: Level,
    field: String,
    op: Op,
    values: Vec<String>,
}

/// The credit role a Plex tag filter (`actor=`, `director=`, …) selects.
fn people_role(field: &str) -> Option<people::Role> {
    Some(match field {
        "actor" => people::Role::Actor,
        "director" => people::Role::Director,
        "writer" => people::Role::Writer,
        "producer" => people::Role::Producer,
        _ => return None,
    })
}

/// Query parameters that are not filters.
const NOT_FILTERS: &[&str] = &[
    "type",
    "sort",
    "limit",
    "query",
    "includeGuids",
    "includeMeta",
    "includeAdvanced",
    "includeCollections",
    "includeExternalMedia",
    "includeDetails",
    "includeFields",
    "excludeFields",
    "excludeElements",
    "includeMarkers",
    "includeChapters",
    "checkFiles",
    "push",
    "pop",
    "and",
    "or",
    "group",
    "having",
    "sectionId",
    "contentDirectoryID",
    "count",
    "excludeAllLeaves",
    "includeOnDeck",
    "skipRefresh",
    "asyncAugmentMetadata",
];

impl Filter {
    fn parse(params: &Params, kind: &str) -> Vec<Filter> {
        params
            .0
            .iter()
            .filter_map(|(raw_key, raw_value)| {
                if raw_key.len() > 7 && raw_key.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("x-plex-")) {
                    return None;
                }
                if NOT_FILTERS.contains(&raw_key.as_str()) {
                    return None;
                }
                let mut key = raw_key.as_str();
                let mut level = Level::Own;
                if let Some((prefix, rest)) = key.split_once('.') {
                    // Filters on descendants (`episode.title` on shows) are not supported.
                    level = level_of(prefix, kind)?;
                    key = rest;
                }
                let (field, op) = if let Some(f) = key.strip_suffix(">>") {
                    (f, Op::Greater)
                } else if let Some(f) = key.strip_suffix("<<") {
                    (f, Op::Less)
                } else if let Some(f) = key.strip_suffix('!') {
                    (f, Op::Not)
                } else if let Some(f) = key.strip_suffix('<') {
                    (f, Op::Begins)
                } else if let Some(f) = key.strip_suffix('>') {
                    (f, Op::Ends)
                } else {
                    (key, Op::Is)
                };
                let (op, value) = match (op, raw_value.strip_prefix('=')) {
                    (Op::Is, Some(rest)) => (Op::Exactly, rest),
                    (Op::Not, Some(rest)) => (Op::NotExactly, rest),
                    (op, _) => (op, raw_value.as_str()),
                };
                let values = value.split(',').map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).collect();
                Some(Filter { level, field: field.to_owned(), op, values })
            })
            .collect()
    }

    fn needs_extras(&self) -> bool {
        matches!(self.field.as_str(), "genre" | "label" | "contentRating" | "studio" | "guid") || self.needs_people()
    }

    fn needs_people(&self) -> bool {
        people_role(&self.field).is_some()
    }

    /// Unknown fields don't filter anything out.
    fn matches(&self, row: &ItemRow, extras: Option<&Extras>, now: i64) -> bool {
        let ancestor = match self.level {
            Level::Own => None,
            Level::Parent => Some(match self.field.as_str() {
                "id" => Some(self.number(row.parent_id.map(|i| i as f64))),
                "title" => Some(self.text(row.parent_title.as_deref())),
                "index" => Some(self.number(row.parent_idx.map(|i| i as f64))),
                _ => None,
            }),
            Level::Grandparent => Some(match self.field.as_str() {
                "id" => Some(self.number(row.grandparent_id.map(|i| i as f64))),
                "title" => Some(self.text(row.grandparent_title.as_deref())),
                _ => None,
            }),
        };
        if let Some(result) = ancestor {
            return result.unwrap_or(true);
        }
        let watched = match row.kind.as_str() {
            "show" | "season" => {
                let leaves = row.leaf_count.unwrap_or(0);
                leaves > 0 && row.viewed_leaf_count.unwrap_or(0) >= leaves
            }
            _ => row.watched,
        };
        let result = match self.field.as_str() {
            "title" => Some(self.text(Some(&row.title))),
            "studio" => Some(self.text(extras.and_then(|e| e.studio.as_deref()))),
            "year" => Some(self.number(row.year.map(|y| y as f64))),
            "decade" => Some(self.number(row.year.map(|y| (y / 10 * 10) as f64))),
            "index" => Some(self.number(row.idx.map(|i| i as f64))),
            "id" => Some(self.number(Some(row.id as f64))),
            "duration" => Some(self.number(row.duration_ms.map(|d| d as f64))),
            "viewCount" => Some(self.number(Some(meta::view_count(row) as f64))),
            "viewOffset" => Some(self.number(Some(row.position_ms as f64))),
            "rating" | "audienceRating" => Some(self.number(row.rating)),
            "userRating" => Some(self.number(row.user_rating)),
            "leafCount" => Some(self.number(row.leaf_count.map(|l| l as f64))),
            "genre" => Some(self.tags(extras.map(|e| e.genres.as_slice()).unwrap_or(&[]))),
            "label" => Some(self.tags(extras.map(|e| e.labels.as_slice()).unwrap_or(&[]))),
            "contentRating" => {
                let rating: Vec<String> = extras.and_then(|e| e.content_rating.clone()).into_iter().collect();
                Some(self.tags(&rating))
            }
            field if people_role(field).is_some() => {
                let role = people_role(field);
                let credits = extras.map(|e| e.people.as_slice()).unwrap_or(&[]);
                // Values are person ids (the tag ids of `/library/sections/{id}/actor`) or names.
                let credited = |v: &str| {
                    credits.iter().any(|c| {
                        Some(c.role) == role && (v.parse::<i64>().ok() == Some(c.id) || c.name.eq_ignore_ascii_case(v))
                    })
                };
                let any = self.values.iter().any(|v| credited(v));
                Some(if matches!(self.op, Op::Not | Op::NotExactly) { !any } else { any })
            }
            "unwatched" | "unwatchedLeaves" => Some(self.boolean(!watched)),
            "watched" => Some(self.boolean(watched)),
            "inProgress" => Some(self.boolean(row.position_ms > 0 && !row.watched)),
            "addedAt" => Some(self.date(Some(row.added_at), now)),
            "updatedAt" => Some(self.date(Some(row.updated_at), now)),
            "lastViewedAt" => Some(self.date(row.last_played_at, now)),
            "originallyAvailableAt" => Some(self.date(row.release_date.as_deref().and_then(date_secs), now)),
            "guid" => {
                let own = meta::guid(&row.kind, row.id);
                let imdb = extras.and_then(|e| e.imdb_id.clone()).map(|i| format!("imdb://{i}"));
                let tmdb = extras.and_then(|e| e.tmdb_id).map(|t| format!("tmdb://{t}"));
                // Only where the details list them.
                let tvdb = extras.and_then(|e| e.tvdb_id).filter(|_| row.kind == "show").map(|t| format!("tvdb://{t}"));
                let mbid = extras
                    .and_then(|e| e.musicbrainz_id.clone())
                    .filter(|_| matches!(row.kind.as_str(), "artist" | "album"))
                    .map(|m| format!("mbid://{m}"));
                let known = [imdb, tmdb, tvdb, mbid];
                Some(self.values.iter().any(|v| *v == own || known.iter().any(|g| g.as_ref() == Some(v))))
            }
            _ => None,
        };
        result.unwrap_or(true)
    }

    fn text(&self, value: Option<&str>) -> bool {
        let value = value.unwrap_or_default().to_lowercase();
        let any = |f: &dyn Fn(&str) -> bool| self.values.iter().any(|v| f(&v.to_lowercase()));
        match self.op {
            Op::Is => any(&|v| value.contains(v)),
            Op::Not => !any(&|v| value.contains(v)),
            Op::Exactly => any(&|v| value == v),
            Op::NotExactly => !any(&|v| value == v),
            Op::Begins => any(&|v| value.starts_with(v)),
            Op::Ends => any(&|v| value.ends_with(v)),
            Op::Greater | Op::Less => true,
        }
    }

    fn number(&self, value: Option<f64>) -> bool {
        let targets: Vec<f64> = self.values.iter().filter_map(|v| v.parse().ok()).collect();
        let Some(value) = value else { return matches!(self.op, Op::Not | Op::NotExactly) };
        match self.op {
            Op::Is | Op::Exactly => targets.iter().any(|t| (value - t).abs() < 1e-9),
            Op::Not | Op::NotExactly => !targets.iter().any(|t| (value - t).abs() < 1e-9),
            Op::Greater => targets.first().is_some_and(|t| value > *t),
            Op::Less => targets.first().is_some_and(|t| value < *t),
            Op::Begins | Op::Ends => true,
        }
    }

    fn tags(&self, tags: &[String]) -> bool {
        let has = |v: &str| tags.iter().any(|t| t.eq_ignore_ascii_case(v));
        match self.op {
            Op::Not | Op::NotExactly => !self.values.iter().any(|v| has(v)),
            _ => self.values.iter().any(|v| has(v)),
        }
    }

    fn boolean(&self, value: bool) -> bool {
        let wanted = !self.values.first().is_some_and(|v| matches!(v.as_str(), "0" | "false"));
        match self.op {
            Op::Not | Op::NotExactly => value != wanted,
            _ => value == wanted,
        }
    }

    /// Dates are unix seconds or relative (`-30d`: thirty days ago).
    fn date(&self, value: Option<i64>, now: i64) -> bool {
        let Some(value) = value else { return false };
        let Some(target) = self.values.first().and_then(|v| parse_date(v, now)) else { return true };
        match self.op {
            Op::Greater => value > target,
            Op::Less => value < target,
            Op::Not | Op::NotExactly => value.abs_diff(target) >= DAY.unsigned_abs(),
            _ => value.abs_diff(target) < DAY.unsigned_abs(),
        }
    }
}

/// A day in seconds: `=` and `!=` on dates mean the same day or not.
const DAY: i64 = 86_400;

fn parse_date(value: &str, now: i64) -> Option<i64> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<i64>() {
        return Some(secs);
    }
    if let Some(secs) = date_secs(value) {
        return Some(secs);
    }
    let relative = value.trim_start_matches('-');
    let (number, unit) = relative.split_at(relative.find(|c: char| !c.is_ascii_digit())?);
    if number.is_empty() {
        return None;
    }
    let unit_secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3_600,
        "d" => DAY,
        "w" => 7 * DAY,
        "mon" => 30 * DAY,
        "y" => 365 * DAY,
        _ => return None,
    };
    // Further back than an i64 reaches is the earliest time there is.
    let back = number.parse::<i64>().ok().and_then(|n| n.checked_mul(unit_secs));
    Some(back.map_or(i64::MIN, |back| now.saturating_sub(back)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortField {
    Title,
    Added,
    Released,
    Year,
    Rating,
    UserRating,
    LastViewed,
    ViewCount,
    Duration,
    Index,
    Id,
    Updated,
    /// The best file's bitrate (a show's: its episodes' on average).
    Bitrate,
}

/// A title's bitrate as [`SortField::Bitrate`] sorts it.
const BITRATE_SQL: &str = "(CASE WHEN i.kind = 'show'
    THEN (SELECT AVG(f.bitrate) FROM items e JOIN media_files f ON f.item_id = e.id WHERE e.grandparent_id = i.id)
    ELSE (SELECT MAX(f.bitrate) FROM media_files f WHERE f.item_id = i.id) END)";

/// The bitrates of `rows` when `spec` sorts by them (rows don't carry them).
async fn bitrates_for(db: &Db, rows: &[ItemRow], spec: &[(SortField, bool)]) -> sqlx::Result<HashMap<i64, f64>> {
    if !spec.iter().any(|(field, _)| *field == SortField::Bitrate) {
        return Ok(HashMap::new());
    }
    let ids = serde_json::to_string(&rows.iter().map(|r| r.id).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into());
    let found: Vec<(i64, Option<f64>)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT i.id, CAST({BITRATE_SQL} AS REAL) FROM items i WHERE i.id IN (SELECT value FROM json_each(?))"
    )))
    .bind(ids)
    .fetch_all(&db.read)
    .await?;
    Ok(found.into_iter().filter_map(|(id, bitrate)| bitrate.map(|b| (id, b))).collect())
}

/// `sort=movie.addedAt:desc,titleSort` → fields with direction (descending?).
fn parse_sort(value: Option<&str>) -> Vec<(SortField, bool)> {
    let mut out = Vec::new();
    for spec in value.unwrap_or_default().split(',') {
        let spec = spec.trim();
        if spec.is_empty() {
            continue;
        }
        let (field, dir) = spec.split_once(':').unwrap_or((spec, ""));
        let field = field.rsplit('.').next().unwrap_or(field);
        let field = match field {
            "titleSort" | "title" => SortField::Title,
            "addedAt" => SortField::Added,
            "originallyAvailableAt" => SortField::Released,
            "year" => SortField::Year,
            "rating" | "audienceRating" => SortField::Rating,
            "userRating" => SortField::UserRating,
            "lastViewedAt" => SortField::LastViewed,
            "viewCount" => SortField::ViewCount,
            "duration" => SortField::Duration,
            "index" => SortField::Index,
            "id" => SortField::Id,
            "updatedAt" => SortField::Updated,
            "mediaBitrate" => SortField::Bitrate,
            _ => continue,
        };
        out.push((field, dir == "desc"));
    }
    if out.is_empty() {
        out.push((SortField::Title, false));
    }
    out
}

/// Sorts `rows` as `spec` says, with `bitrates` from [`bitrates_for`].
fn sort_rows(rows: &mut [ItemRow], spec: &[(SortField, bool)], bitrates: &HashMap<i64, f64>) {
    fn cmp_opt<T: PartialOrd>(a: Option<T>, b: Option<T>, desc: bool) -> Ordering {
        match (a, b) {
            (Some(a), Some(b)) => {
                let o = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
                if desc { o.reverse() } else { o }
            }
            // Missing values last in both directions.
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }
    rows.sort_by(|a, b| {
        for &(field, desc) in spec {
            let o = match field {
                SortField::Title => cmp_opt(Some(&a.sort_title), Some(&b.sort_title), desc),
                SortField::Added => cmp_opt(Some(a.added_at), Some(b.added_at), desc),
                SortField::Released => cmp_opt(a.release_date.as_ref(), b.release_date.as_ref(), desc),
                SortField::Year => cmp_opt(a.year, b.year, desc),
                SortField::Rating => cmp_opt(a.rating, b.rating, desc),
                SortField::UserRating => cmp_opt(a.user_rating, b.user_rating, desc),
                SortField::LastViewed => cmp_opt(a.last_played_at, b.last_played_at, desc),
                SortField::ViewCount => cmp_opt(Some(meta::view_count(a)), Some(meta::view_count(b)), desc),
                SortField::Duration => cmp_opt(a.duration_ms, b.duration_ms, desc),
                SortField::Index => cmp_opt(a.idx, b.idx, desc),
                SortField::Id => cmp_opt(Some(a.id), Some(b.id), desc),
                SortField::Updated => cmp_opt(Some(a.updated_at), Some(b.updated_at), desc),
                SortField::Bitrate => cmp_opt(bitrates.get(&a.id), bitrates.get(&b.id), desc),
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        a.sort_title.cmp(&b.sort_title)
    });
}

// ------------------------------------------------------------------ the same filters and sorts in SQL

/// Plex's `viewCount` of an item (see [`meta::view_count`]).
const VIEW_COUNT_SQL: &str = "(CASE WHEN COALESCE(s.watched, 0) THEN MAX(COALESCE(s.play_count, 0), 1) ELSE 0 END)";

/// Whether the user watched an item of `kind` as [`Filter::matches`] sees
/// it (shows and seasons: every episode), and whether that reads `s`.
fn watched_sql(kind: &str) -> (&'static str, bool) {
    match kind {
        "show" => (
            "(COALESCE(i.leaf_count, 0) > 0 AND COALESCE(i.leaf_count, 0) <= (SELECT COUNT(*) FROM items e
               JOIN user_item_state us ON us.item_id = e.id WHERE e.grandparent_id = i.id AND e.kind = 'episode'
               AND us.user_id = (SELECT uid FROM me) AND us.watched = 1))",
            false,
        ),
        "season" => (
            "(COALESCE(i.leaf_count, 0) > 0 AND COALESCE(i.leaf_count, 0) <= (SELECT COUNT(*) FROM items e
               JOIN user_item_state us ON us.item_id = e.id WHERE e.parent_id = i.id
               AND us.user_id = (SELECT uid FROM me) AND us.watched = 1))",
            false,
        ),
        _ => ("COALESCE(s.watched, 0)", true),
    }
}

/// A filter as SQL, for listings filtered, sorted and paged by the database:
/// it keeps exactly the items [`Filter::matches`] keeps.
#[derive(Debug)]
enum Condition {
    /// Every item.
    All,
    /// ` AND …` and its arguments; `state`: it reads the user's state `s`.
    Sql { sql: String, args: Vec<SqlArg>, state: bool },
    /// Only [`Filter::matches`] can tell (Unicode case folding, calendar dates).
    InMemory,
}

impl Condition {
    fn sql(sql: String, args: Vec<SqlArg>) -> Self {
        Condition::Sql { sql, args, state: false }
    }

    /// No item.
    fn none() -> Self {
        Self::sql(" AND 0".into(), Vec::new())
    }

    fn reading_state(self, state: bool) -> Self {
        match self {
            Condition::Sql { sql, args, state: before } => Condition::Sql { sql, args, state: before || state },
            other => other,
        }
    }
}

impl Filter {
    /// This filter as a condition on the item `i` (of `kind`), its parent
    /// `p`, grandparent `gp` and the user's state `s`.
    fn condition(&self, kind: &str, now: i64) -> Condition {
        match (self.level, self.field.as_str()) {
            (Level::Parent, "id") => self.number_sql("i.parent_id"),
            (Level::Parent, "title") => self.text_sql("p.title"),
            (Level::Parent, "index") => self.number_sql("p.idx"),
            (Level::Grandparent, "id") => self.number_sql("i.grandparent_id"),
            (Level::Grandparent, "title") => self.text_sql("gp.title"),
            (Level::Parent | Level::Grandparent, _) => Condition::All,
            (Level::Own, "title") => self.text_sql("i.title"),
            (Level::Own, "studio") => self.text_sql("i.studio"),
            (Level::Own, "year") => self.number_sql("i.year"),
            (Level::Own, "decade") => self.number_sql("(i.year / 10 * 10)"),
            (Level::Own, "index") => self.number_sql("i.idx"),
            (Level::Own, "id") => self.number_sql("i.id"),
            (Level::Own, "duration") => self.number_sql("i.duration_ms"),
            (Level::Own, "viewCount") => self.number_sql(VIEW_COUNT_SQL).reading_state(true),
            (Level::Own, "viewOffset") => self.number_sql("COALESCE(s.position_ms, 0)").reading_state(true),
            (Level::Own, "rating" | "audienceRating") => self.number_sql("i.rating"),
            (Level::Own, "userRating") => self.number_sql("s.rating").reading_state(true),
            (Level::Own, "leafCount") => self.number_sql("i.leaf_count"),
            (Level::Own, "genre") => self.tags_sql(|list| {
                format!(
                    "EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(i.genres) THEN i.genres END) g
                     WHERE g.value COLLATE NOCASE IN ({list}))"
                )
            }),
            (Level::Own, "label") => self.tags_sql(|list| {
                format!("EXISTS (SELECT 1 FROM item_labels l WHERE l.item_id = i.id AND l.label IN ({list}))")
            }),
            (Level::Own, "contentRating") => self.tags_sql(|list| {
                format!("(i.content_rating IS NOT NULL AND i.content_rating COLLATE NOCASE IN ({list}))")
            }),
            (Level::Own, field) if people_role(field).is_some() => {
                self.people_sql(people_role(field).map(people::Role::as_str).unwrap_or_default())
            }
            (Level::Own, "unwatched" | "unwatchedLeaves") => {
                let (watched, state) = watched_sql(kind);
                self.boolean_sql(&format!("(NOT {watched})")).reading_state(state)
            }
            (Level::Own, "watched") => {
                let (watched, state) = watched_sql(kind);
                self.boolean_sql(watched).reading_state(state)
            }
            (Level::Own, "inProgress") => {
                self.boolean_sql("(COALESCE(s.position_ms, 0) > 0 AND NOT COALESCE(s.watched, 0))").reading_state(true)
            }
            (Level::Own, "addedAt") => self.date_sql("i.added_at", now),
            (Level::Own, "updatedAt") => self.date_sql("i.updated_at", now),
            (Level::Own, "lastViewedAt") => self.date_sql("s.last_played_at", now).reading_state(true),
            (Level::Own, "originallyAvailableAt") => Condition::InMemory,
            (Level::Own, "guid") => self.guid_sql(kind),
            (Level::Own, _) => Condition::All,
        }
    }

    /// Whether any of `tests` holds, or none of them for `!=` and `!==`.
    fn any_sql(&self, tests: Vec<String>, args: Vec<SqlArg>) -> Condition {
        let negated = matches!(self.op, Op::Not | Op::NotExactly);
        match (tests.is_empty(), negated) {
            (true, false) => Condition::none(),
            (true, true) => Condition::All,
            (false, false) => Condition::sql(format!(" AND ({})", tests.join(" OR ")), args),
            (false, true) => Condition::sql(format!(" AND NOT ({})", tests.join(" OR ")), args),
        }
    }

    /// [`Filter::text`]: in SQL when every value is ASCII once lowercased.
    fn text_sql(&self, column: &str) -> Condition {
        if matches!(self.op, Op::Greater | Op::Less) {
            return Condition::All;
        }
        let needles: Vec<String> = self.values.iter().map(|v| v.to_lowercase()).collect();
        if needles.iter().any(|n| !n.is_ascii()) {
            return Condition::InMemory;
        }
        // SQLite's lower() only folds ASCII. Two other characters lowercase to
        // ASCII ('İ' to "i\u{307}", the Kelvin sign to 'k'); with those mapped
        // first, both lowercasings have the same runs of ASCII, so they agree
        // on every comparison with an ASCII needle.
        let value =
            format!("lower(replace(replace(COALESCE({column}, ''), char(304), 'i' || char(775)), char(8490), 'k'))");
        let mut tests = Vec::new();
        let mut args = Vec::new();
        for needle in needles {
            match self.op {
                Op::Begins => tests.push(format!("instr({value}, ?) = 1")),
                Op::Ends => {
                    tests.push(format!("substr({value}, ?) = ?"));
                    args.push(SqlArg::Int(-i64::try_from(needle.len()).unwrap_or(i64::MAX)));
                }
                Op::Exactly | Op::NotExactly => tests.push(format!("{value} = ?")),
                _ => tests.push(format!("instr({value}, ?) > 0")),
            }
            args.push(SqlArg::Text(needle));
        }
        self.any_sql(tests, args)
    }

    /// [`Filter::number`]: compared as `f64`.
    fn number_sql(&self, expr: &str) -> Condition {
        let targets: Vec<f64> = self.values.iter().filter_map(|v| v.parse().ok()).collect();
        // SQLite would compare an integer with a real exactly; `f64` doesn't.
        let value = format!("({expr} + 0.0)");
        match self.op {
            Op::Is | Op::Exactly | Op::Not | Op::NotExactly => {
                let mut tests = Vec::new();
                let mut args = Vec::new();
                // NaN equals nothing (and SQLite would take it for NULL).
                for target in targets.into_iter().filter(|t| !t.is_nan()) {
                    tests.push(format!("abs({value} - ?) < ?"));
                    args.extend([SqlArg::Real(target), SqlArg::Real(1e-9)]);
                }
                let any = if tests.is_empty() { "0".to_owned() } else { tests.join(" OR ") };
                // A missing value equals nothing.
                let sql = if matches!(self.op, Op::Not | Op::NotExactly) {
                    format!(" AND ({expr} IS NULL OR NOT ({any}))")
                } else {
                    format!(" AND {expr} IS NOT NULL AND ({any})")
                };
                Condition::sql(sql, args)
            }
            Op::Greater | Op::Less => match targets.first() {
                Some(target) if !target.is_nan() => {
                    let op = if self.op == Op::Greater { ">" } else { "<" };
                    Condition::sql(format!(" AND {value} {op} ?"), vec![SqlArg::Real(*target)])
                }
                _ => Condition::none(),
            },
            Op::Begins | Op::Ends => Condition::sql(format!(" AND {expr} IS NOT NULL"), Vec::new()),
        }
    }

    /// [`Filter::tags`]; `test` gets the placeholders of the values.
    fn tags_sql(&self, test: impl Fn(&str) -> String) -> Condition {
        if self.values.is_empty() {
            return self.any_sql(Vec::new(), Vec::new());
        }
        let list = vec!["?"; self.values.len()].join(", ");
        self.any_sql(vec![test(&list)], self.values.iter().map(|v| SqlArg::Text(v.clone())).collect())
    }

    /// People credited in `role`, by person id or name.
    fn people_sql(&self, role: &str) -> Condition {
        if self.values.is_empty() {
            return self.any_sql(Vec::new(), Vec::new());
        }
        let mut args = vec![SqlArg::Text(role.to_owned())];
        let mut who = Vec::new();
        let ids: Vec<i64> = self.values.iter().filter_map(|v| v.parse().ok()).collect();
        if !ids.is_empty() {
            who.push(format!("pe.id IN ({})", vec!["?"; ids.len()].join(", ")));
            args.extend(ids.into_iter().map(SqlArg::Int));
        }
        who.push(format!("pe.name COLLATE NOCASE IN ({})", vec!["?"; self.values.len()].join(", ")));
        args.extend(self.values.iter().map(|v| SqlArg::Text(v.clone())));
        let test = format!(
            "EXISTS (SELECT 1 FROM item_people ip WHERE ip.item_id = i.id AND ip.role = ?
               AND ip.person_id IN (SELECT pe.id FROM people pe WHERE {}))",
            who.join(" OR ")
        );
        self.any_sql(vec![test], args)
    }

    /// [`Filter::boolean`] of `expr`, a parenthesised condition.
    fn boolean_sql(&self, expr: &str) -> Condition {
        let wanted = !self.values.first().is_some_and(|v| matches!(v.as_str(), "0" | "false"));
        let negated = matches!(self.op, Op::Not | Op::NotExactly);
        Condition::sql(if wanted != negated { format!(" AND {expr}") } else { format!(" AND NOT {expr}") }, Vec::new())
    }

    /// [`Filter::date`] of a unix time.
    fn date_sql(&self, expr: &str, now: i64) -> Condition {
        let Some(target) = self.values.first().and_then(|v| parse_date(v, now)) else {
            // An unreadable date keeps whatever has a date.
            return Condition::sql(format!(" AND {expr} IS NOT NULL"), Vec::new());
        };
        // The same day: less than a day apart.
        let day = [SqlArg::Int(target.saturating_sub(DAY - 1)), SqlArg::Int(target.saturating_add(DAY - 1))];
        match self.op {
            Op::Greater => Condition::sql(format!(" AND {expr} > ?"), vec![SqlArg::Int(target)]),
            Op::Less => Condition::sql(format!(" AND {expr} < ?"), vec![SqlArg::Int(target)]),
            Op::Not | Op::NotExactly => Condition::sql(format!(" AND {expr} NOT BETWEEN ? AND ?"), day.into()),
            _ => Condition::sql(format!(" AND {expr} BETWEEN ? AND ?"), day.into()),
        }
    }

    /// `plex://`, `imdb://`, `tmdb://`, `tvdb://` (shows) and `mbid://`
    /// (artists, albums) guids, whatever the operator, like [`Filter::matches`].
    fn guid_sql(&self, kind: &str) -> Condition {
        let mut tests = Vec::new();
        let mut args = Vec::new();
        for value in &self.values {
            let own = value
                .rsplit_once('/')
                .and_then(|(_, hex)| u64::from_str_radix(hex, 16).ok())
                .map(|id| id as i64)
                .filter(|id| meta::guid(kind, *id) == *value);
            if let Some(id) = own {
                tests.push("i.id = ?".to_owned());
                args.push(SqlArg::Int(id));
            }
            if let Some(imdb) = value.strip_prefix("imdb://") {
                tests.push("i.imdb_id = ?".to_owned());
                args.push(SqlArg::Text(imdb.to_owned()));
            }
            let tmdb = value.strip_prefix("tmdb://").and_then(|t| t.parse::<i64>().ok());
            if let Some(tmdb) = tmdb.filter(|t| format!("tmdb://{t}") == *value) {
                tests.push("i.tmdb_id = ?".to_owned());
                args.push(SqlArg::Int(tmdb));
            }
            let tvdb = value.strip_prefix("tvdb://").and_then(|t| t.parse::<i64>().ok());
            if let Some(tvdb) = tvdb.filter(|t| kind == "show" && format!("tvdb://{t}") == *value) {
                tests.push("i.tvdb_id = ?".to_owned());
                args.push(SqlArg::Int(tvdb));
            }
            if let Some(mbid) = value.strip_prefix("mbid://").filter(|_| matches!(kind, "artist" | "album")) {
                tests.push("i.musicbrainz_id = ?".to_owned());
                args.push(SqlArg::Text(mbid.to_owned()));
            }
        }
        if tests.is_empty() {
            Condition::none()
        } else {
            Condition::sql(format!(" AND ({})", tests.join(" OR ")), args)
        }
    }
}

impl SortField {
    /// The expression sorted on, and whether it can be NULL.
    fn sql(self) -> (&'static str, bool) {
        match self {
            SortField::Title => ("i.sort_title", false),
            SortField::Added => ("i.added_at", false),
            SortField::Released => ("i.release_date", true),
            SortField::Year => ("i.year", true),
            SortField::Rating => ("i.rating", true),
            SortField::UserRating => ("s.rating", true),
            SortField::LastViewed => ("s.last_played_at", true),
            SortField::ViewCount => (VIEW_COUNT_SQL, false),
            SortField::Duration => ("i.duration_ms", true),
            SortField::Index => ("i.idx", true),
            SortField::Id => ("i.id", false),
            SortField::Updated => ("i.updated_at", false),
            SortField::Bitrate => (BITRATE_SQL, true),
        }
    }
}

/// `ORDER BY` terms that sort like [`sort_rows`] does the rows of one
/// library (which come by sort title, then id): missing values last in both
/// directions, then by sort title, then by id.
fn order_by_sql(sort: &[(SortField, bool)]) -> String {
    let mut terms = Vec::new();
    for &(field, descending) in sort {
        let (expr, nullable) = field.sql();
        if descending {
            // NULL sorts first, so it comes last when descending.
            terms.push(format!("{expr} DESC"));
        } else {
            if nullable {
                terms.push(format!("{expr} IS NULL"));
            }
            terms.push(format!("{expr} ASC"));
        }
    }
    terms.push("i.sort_title ASC".into());
    terms.push("i.id ASC".into());
    terms.join(", ")
}

/// The filters of a listing as SQL, when all of them can be.
struct SqlFilters {
    filter: String,
    args: Vec<SqlArg>,
    uses_state: bool,
}

/// A section listing: the items of one kind in a library that pass Plex
/// filters, in a Plex sort order, at most `limit` of them.
struct SectionQuery<'a> {
    library_id: i64,
    kind: &'a str,
    filters: Vec<Filter>,
    sort: Vec<(SortField, bool)>,
    /// Plex's `limit`: how many items there are at most.
    cap: Option<usize>,
    now: i64,
}

impl<'a> SectionQuery<'a> {
    fn new(library_id: i64, kind: &'a str, params: &Params) -> Self {
        SectionQuery {
            library_id,
            kind,
            filters: Filter::parse(params, kind),
            sort: parse_sort(params.get("sort")),
            cap: params.parse("limit"),
            now: now(),
        }
    }

    /// The `page` of the items and how many there are: filtered, sorted and
    /// paged in SQL unless a filter can only be checked in memory.
    async fn page(&self, db: &Db, user_id: i64, page: &Page) -> sqlx::Result<(Vec<ItemRow>, usize)> {
        match self.sql_filters() {
            Some(filters) => self.page_in_sql(db, user_id, &filters, page).await,
            None => self.page_in_memory(db, user_id, page).await,
        }
    }

    fn sql_filters(&self) -> Option<SqlFilters> {
        let mut out = SqlFilters { filter: String::new(), args: Vec::new(), uses_state: false };
        for filter in &self.filters {
            match filter.condition(self.kind, self.now) {
                Condition::All => {}
                Condition::Sql { sql, args, state } => {
                    out.filter.push_str(&sql);
                    out.args.extend(args);
                    out.uses_state |= state;
                }
                Condition::InMemory => return None,
            }
        }
        Some(out)
    }

    async fn page_in_sql(
        &self,
        db: &Db,
        user_id: i64,
        filters: &SqlFilters,
        page: &Page,
    ) -> sqlx::Result<(Vec<ItemRow>, usize)> {
        let end = self.cap.unwrap_or(usize::MAX);
        let start = page.start.min(end);
        let size = page.size.unwrap_or(usize::MAX).min(end - start);
        let order_by = order_by_sql(&self.sort);
        let query = ItemsQuery {
            library_id: self.library_id,
            kind: self.kind,
            filter: &filters.filter,
            args: &filters.args,
            uses_state: filters.uses_state,
            order_by: &order_by,
            offset: i64::try_from(start).unwrap_or(i64::MAX),
            limit: (size < usize::MAX).then(|| i64::try_from(size).unwrap_or(i64::MAX)),
        };
        let rows = catalog::items_matching(db, user_id, &query).await?;
        let total = if rows.len() < size && (start == 0 || !rows.is_empty()) {
            // The page ends the list.
            start + rows.len()
        } else if size > 0 && rows.len() == size && start + size == end {
            // The page ends at `limit`.
            end
        } else {
            usize::try_from(catalog::count_matching(db, user_id, &query).await?).unwrap_or(0).min(end)
        };
        Ok((rows, total))
    }

    /// Everything loaded, then filtered and sorted here.
    async fn page_in_memory(&self, db: &Db, user_id: i64, page: &Page) -> sqlx::Result<(Vec<ItemRow>, usize)> {
        let (mut rows, _) = catalog::library_items(db, user_id, &LibraryQuery::all(self.library_id, self.kind)).await?;
        // Titles tie by id, in SQL as here.
        rows.sort_by(|a, b| a.sort_title.cmp(&b.sort_title).then(a.id.cmp(&b.id)));
        let mut extras = HashMap::new();
        if self.filters.iter().any(Filter::needs_extras) {
            let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
            extras = meta::load_extras(db, &ids).await?;
            if self.filters.iter().any(Filter::needs_people) {
                add_people(db, &ids, &mut extras).await?;
            }
        }
        if !self.filters.is_empty() {
            rows.retain(|r| self.filters.iter().all(|f| f.matches(r, extras.get(&r.id), self.now)));
        }
        let bitrates = bitrates_for(db, &rows, &self.sort).await?;
        sort_rows(&mut rows, &self.sort, &bitrates);
        if let Some(cap) = self.cap {
            rows.truncate(cap);
        }
        Ok(page.apply(rows))
    }
}

// ------------------------------------------------------------------ smart collections

/// A section listing query (`type=1&genre=Comedy&sort=year:desc`) as
/// parameters.
fn filter_params(filter: &str) -> Params {
    Params(
        filter
            .trim_start_matches('?')
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                (crate::util::percent_decode(key, true), crate::util::percent_decode(value, true))
            })
            .collect(),
    )
}

/// The kind a smart filter lists in `lib`, when the library has it.
fn smart_kind(lib: &LibraryRow, params: &Params) -> Option<&'static str> {
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    kinds_of(lib).contains(&kind).then_some(kind)
}

/// Whether `filter` is a section listing query a smart collection can use.
pub(crate) fn valid_smart_filter(lib: &LibraryRow, filter: &str) -> bool {
    let params = filter_params(filter);
    !params.0.is_empty() && smart_kind(lib, &params).is_some()
}

/// What a smart collection's filter finds in `lib`, as `user_id` may see it,
/// in its order.
pub(crate) async fn smart_items(db: &Db, user_id: i64, lib: &LibraryRow, filter: &str) -> sqlx::Result<Vec<ItemRow>> {
    let params = filter_params(filter);
    let Some(kind) = smart_kind(lib, &params) else { return Ok(Vec::new()) };
    Ok(SectionQuery::new(lib.id, kind, &params).page(db, user_id, &Page::default()).await?.0)
}

/// How many items a smart collection's filter finds.
pub(crate) async fn smart_count(db: &Db, user_id: i64, lib: &LibraryRow, filter: &str) -> sqlx::Result<usize> {
    let params = filter_params(filter);
    let Some(kind) = smart_kind(lib, &params) else { return Ok(0) };
    let count = Page { start: 0, size: Some(0) };
    Ok(SectionQuery::new(lib.id, kind, &params).page(db, user_id, &count).await?.1)
}

/// `/library/sections/{id}/all`
pub async fn all(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    if params.get("type") == Some(super::lists::COLLECTION_TYPE) {
        return super::lists::section_collections(State(state), PlexUser(user), Extension(info), Path(id), page).await;
    }
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    let (rows, total) = if kinds_of(&lib).contains(&kind) {
        SectionQuery::new(lib.id, kind, &params).page(&state.db, user.id, &page).await?
    } else {
        (Vec::new(), 0)
    };
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    let mut mc = list_container(Some(&lib), &info, nodes, total, &page)
        .attr("title2", format!("All {}", plural(kind)))
        .attr("viewGroup", plex_type(kind));
    if params.flag("includeMeta") {
        mc.push(filter_meta(&lib));
    }
    Ok(mc)
}

/// `/library/sections/{id}/albums` (music sections).
pub async fn section_albums(
    state: State<AppState>,
    user: PlexUser,
    info: Extension<Arc<PlexInfo>>,
    id: Path<String>,
    Params(mut pairs): Params,
    page: Page,
) -> PlexResult<Node> {
    pairs.retain(|(k, _)| k != "type");
    pairs.push(("type".into(), type_number("album").to_string()));
    all(state, user, info, id, Params(pairs), page).await
}

/// `/library/all`: a search across every library.
pub async fn library_all(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let wanted = params.get("type").and_then(kind_for_number);
    // Each library's share of the page is among its first `start + size` items.
    let window = Page { start: 0, size: page.size.map(|size| size.saturating_add(page.start)) };
    let mut rows = Vec::new();
    let mut total = 0;
    for lib in &libraries {
        let kinds: Vec<&str> = match wanted {
            Some(kind) if kinds_of(lib).contains(&kind) => vec![kind],
            Some(_) => continue,
            None => vec![lib.kind().top_level_kind()],
        };
        for kind in kinds {
            let (found, count) = SectionQuery::new(lib.id, kind, &params).page(&state.db, user.id, &window).await?;
            rows.extend(found);
            total += count;
        }
    }
    let sort = parse_sort(params.get("sort"));
    let bitrates = bitrates_for(&state.db, &rows, &sort).await?;
    sort_rows(&mut rows, &sort, &bitrates);
    let rows: Vec<ItemRow> = rows.into_iter().skip(page.start).take(page.size.unwrap_or(usize::MAX)).collect();
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page))
}

fn plural(kind: &str) -> &'static str {
    match kind {
        "movie" => "Movies",
        "show" => "Shows",
        "season" => "Seasons",
        "episode" => "Episodes",
        "artist" => "Artists",
        "album" => "Albums",
        _ => "Tracks",
    }
}

/// `Meta` describing the filters, sorts and fields this API understands
/// (python-plexapi validates searches against it).
fn filter_meta(lib: &LibraryRow) -> Node {
    let mut meta = Node::new("Meta");
    for (i, kind) in kinds_of(lib).iter().enumerate() {
        let n = type_number(kind);
        let mut t = Node::new("Type")
            .attr("key", format!("/library/sections/{}/all?type={n}", lib.id))
            .attr("type", plex_type(kind))
            .attr("title", plural(kind))
            .attr("active", i == 0);
        let mut filters = vec![("genre", "Genre"), ("year", "Year"), ("decade", "Decade"), ("label", "Labels")];
        if matches!(*kind, "movie" | "show") {
            filters.push(("contentRating", "Content Rating"));
            filters.push(("studio", "Studio"));
            filters.push(("actor", "Actor"));
        }
        if *kind == "movie" {
            filters.extend([("director", "Director"), ("writer", "Writer"), ("producer", "Producer")]);
        }
        for (filter, title) in filters {
            t.push(
                Node::new("Filter")
                    .attr("filter", filter)
                    .attr("filterType", if filter == "year" || filter == "decade" { "integer" } else { "string" })
                    .attr("key", format!("/library/sections/{}/{filter}?type={n}", lib.id))
                    .attr("title", title)
                    .attr("type", "filter"),
            );
        }
        if matches!(*kind, "movie" | "episode" | "track" | "show") {
            t.push(
                Node::new("Filter")
                    .attr("filter", "unwatched")
                    .attr("filterType", "boolean")
                    .attr("key", "unwatched")
                    .attr("title", "Unplayed")
                    .attr("type", "filter"),
            );
        }
        for (key, title, dir) in [
            ("titleSort", "Title", "asc"),
            ("addedAt", "Date Added", "desc"),
            ("originallyAvailableAt", "Release Date", "desc"),
            ("year", "Year", "desc"),
            ("rating", "Critic Rating", "desc"),
            ("audienceRating", "Audience Rating", "desc"),
            ("lastViewedAt", "Last Played", "desc"),
            ("viewCount", "Plays", "desc"),
            ("duration", "Duration", "desc"),
        ] {
            t.push(
                Node::new("Sort")
                    .attr("defaultDirection", dir)
                    .attr("descKey", format!("{key}:desc"))
                    .attr("firstCharacterKey", format!("/library/sections/{}/firstCharacter", lib.id))
                    .attr("key", key)
                    .attr("title", title),
            );
        }
        let prefix = if *kind == "movie" { String::new() } else { format!("{}.", plex_type(kind)) };
        for (field, ty, title) in [
            ("title", "string", "Title"),
            ("studio", "string", "Studio"),
            ("year", "integer", "Year"),
            ("decade", "integer", "Decade"),
            ("genre", "tag", "Genre"),
            ("label", "tag", "Labels"),
            ("contentRating", "tag", "Content Rating"),
            ("unwatched", "boolean", "Unplayed"),
            ("inProgress", "boolean", "In Progress"),
            ("viewCount", "integer", "Plays"),
            ("addedAt", "date", "Date Added"),
            ("lastViewedAt", "date", "Last Played"),
            ("originallyAvailableAt", "date", "Release Date"),
        ] {
            t.push(Node::new("Field").attr("key", format!("{prefix}{field}")).attr("title", title).attr("type", ty));
        }
        meta.push(t);
    }
    let operators: &[(&str, &[(&str, &str)])] = &[
        ("tag", &[("=", "is"), ("!=", "is not")]),
        ("integer", &[("=", "is"), ("!=", "is not"), (">>=", "is greater than"), ("<<=", "is less than")]),
        (
            "string",
            &[
                ("=", "contains"),
                ("!=", "does not contain"),
                ("==", "is"),
                ("!==", "is not"),
                ("<=", "begins with"),
                (">=", "ends with"),
            ],
        ),
        ("boolean", &[("=", "is true"), ("!=", "is false")]),
        ("date", &[("<<=", "is before"), (">>=", "is after")]),
        ("resolution", &[("=", "is")]),
    ];
    for (ty, ops) in operators {
        let mut field_type = Node::new("FieldType").attr("type", *ty);
        for (key, title) in *ops {
            field_type.push(Node::new("Operator").attr("key", *key).attr("title", *title));
        }
        meta.push(field_type);
    }
    meta
}

/// `/library/sections/{id}/{genre|year|decade|contentRating|studio}`
pub async fn filter_choices(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path((id, filter)): Path<(String, String)>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    let (genres, years) = catalog::library_filters(&state.db, user.id, lib.id, kind).await?;
    let values: Vec<String> = match filter.as_str() {
        "genre" => genres,
        "year" => years.iter().map(i64::to_string).collect(),
        "decade" => {
            let mut decades: Vec<i64> = years.iter().map(|y| y / 10 * 10).collect();
            decades.dedup();
            decades.iter().map(i64::to_string).collect()
        }
        field if people_role(field).is_some() => {
            // Keyed by person id, so the filter is exact. People only appear
            // through titles the user may see.
            let role = people_role(field).map(people::Role::as_str).unwrap_or_default();
            let credited: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "{} SELECT DISTINCT pe.id, pe.name FROM item_people ip
                 JOIN people pe ON pe.id = ip.person_id
                 JOIN items i ON i.id = ip.item_id
                 LEFT JOIN items p ON p.id = i.parent_id
                 LEFT JOIN items gp ON gp.id = i.grandparent_id
                 WHERE {} AND i.library_id = ? AND i.kind = ? AND ip.role = ?
                 ORDER BY pe.name COLLATE NOCASE",
                catalog::ME_CTE,
                catalog::VISIBLE,
            )))
            .bind(user.id)
            .bind(lib.id)
            .bind(kind)
            .bind(role)
            .fetch_all(&state.db.read)
            .await?;
            let mut mc = base_container()
                .attr("size", credited.len())
                .attr("librarySectionID", lib.id)
                .attr("librarySectionUUID", info.section_uuid(lib.id))
                .attr("title1", &lib.name)
                .attr("viewGroup", "secondary");
            let n = type_number(kind);
            for (id, name) in credited {
                mc.push(
                    Node::new("Directory")
                        .attr("fastKey", format!("/library/sections/{}/all?type={n}&{filter}={id}", lib.id))
                        .attr("key", id)
                        .attr("title", name)
                        .attr("type", filter.clone()),
                );
            }
            return Ok(mc);
        }
        "label" => {
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "{ME_CTE} SELECT DISTINCT l.label FROM {VISIBLE_ITEMS} JOIN item_labels l ON l.item_id = i.id
                 WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? ORDER BY l.label"
            )))
            .bind(user.id)
            .bind(lib.id)
            .bind(kind)
            .fetch_all(&state.db.read)
            .await?
        }
        "contentRating" | "studio" => {
            let column = if filter == "studio" { "studio" } else { "content_rating" };
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "{ME_CTE} SELECT DISTINCT i.{column} FROM {VISIBLE_ITEMS}
                 WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? AND i.{column} IS NOT NULL
                 ORDER BY i.{column} COLLATE NOCASE"
            )))
            .bind(user.id)
            .bind(lib.id)
            .bind(kind)
            .fetch_all(&state.db.read)
            .await?
        }
        _ => return Err(PlexError::NotFound),
    };
    let mut mc = base_container()
        .attr("size", values.len())
        .attr("librarySectionID", lib.id)
        .attr("librarySectionUUID", info.section_uuid(lib.id))
        .attr("title1", &lib.name)
        .attr("viewGroup", "secondary");
    let n = type_number(kind);
    for value in values {
        mc.push(
            Node::new("Directory")
                .attr("fastKey", format!("/library/sections/{}/all?type={n}&{filter}={}", lib.id, encode(&value)))
                .attr("key", value.clone())
                .attr("title", value)
                .attr("type", filter.clone()),
        );
    }
    Ok(mc)
}

/// Percent-encodes a query value.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub async fn first_character(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    // Counted by exact first character in SQL, then merged here into what
    // Plex lists (SQLite's UPPER() only knows ASCII).
    let firsts: Vec<(String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{ME_CTE} SELECT SUBSTR(i.sort_title, 1, 1), COUNT(*) FROM {VISIBLE_ITEMS}
         WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? GROUP BY 1"
    )))
    .bind(user.id)
    .bind(lib.id)
    .bind(kind)
    .fetch_all(&state.db.read)
    .await?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (first, count) in firsts {
        let first = first.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        let key = if first.chars().all(char::is_alphabetic) && !first.is_empty() { first } else { "#".into() };
        *counts.entry(key).or_default() += usize::try_from(count).unwrap_or(0);
    }
    let mut mc = base_container().attr("size", counts.len()).attr("title1", &lib.name);
    for (letter, size) in counts {
        mc.push(Node::new("Directory").attr("size", size).attr("key", letter.clone()).attr("title", letter));
    }
    Ok(mc)
}

// ------------------------------------------------------------------ metadata

pub async fn metadata(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(ids): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let ids: Vec<i64> = ids.split(',').filter_map(|i| i.trim().parse().ok()).take(500).collect();
    let rows = catalog::items_by_ids(&state.db, user.id, &ids).await?;
    let libraries = visible_libraries(&state, &user).await?;
    let rows: Vec<ItemRow> = rows.into_iter().filter(|r| libraries.iter().any(|l| l.id == r.library_id)).collect();
    let Some(first) = rows.first() else { return Err(PlexError::NotFound) };
    let lib = libraries.iter().find(|l| l.id == first.library_id);
    let sections = Sections::new(&libraries);
    let detail = Detail { guids: true, full: true, media: true, markers: params.flag("includeMarkers") };
    let mut nodes = item_nodes(&state, &rows, &sections, detail).await?;
    if params.flag("includeOnDeck") {
        for (node, row) in nodes.iter_mut().zip(&rows) {
            if let Some(next) = catalog::on_deck(&state.db, user.id, row).await?
                && let Some(next_row) = catalog::item(&state.db, user.id, next).await?
            {
                let detail = Detail { guids: true, full: false, media: true, markers: false };
                let next_node = item_nodes(&state, &[next_row], &sections, detail).await?;
                let mut on_deck = Node::new("OnDeck");
                on_deck.extend(next_node);
                node.push(on_deck);
            }
        }
    }
    let total = nodes.len();
    Ok(list_container(lib, &info, nodes, total, &Page::default()))
}

pub async fn children(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let parent = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(&state, &user).await?;
    let lib = libraries.iter().find(|l| l.id == parent.library_id).ok_or(PlexError::NotFound)?;
    let mut rows = catalog::children(&state.db, user.id, id).await?;
    if let Some(index) = params.parse::<i64>("index") {
        rows.retain(|r| r.idx == Some(index));
    }
    let (rows, total) = page.apply(rows);
    let view_group = rows.first().map(|r| plex_type(&r.kind)).unwrap_or("secondary");
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(hierarchy_container(lib, &info, &parent, nodes, total, &page).attr("viewGroup", view_group))
}

/// `/library/metadata/{id}/extras`: trailers and other bonus material.
pub async fn extras(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let parent = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(&state, &user).await?;
    let rows = catalog::extras(&state.db, user.id, parent.id).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page))
}

fn hierarchy_container(
    lib: &LibraryRow,
    info: &PlexInfo,
    parent: &ItemRow,
    nodes: Vec<Node>,
    total: usize,
    page: &Page,
) -> Node {
    list_container(Some(lib), info, nodes, total, page)
        .attr("key", parent.id.to_string())
        .attr("parentRatingKey", parent.id.to_string())
        .attr("parentTitle", &parent.title)
        .opt("parentIndex", parent.idx)
        .opt("parentYear", parent.year)
        .opt("summary", parent.summary.as_deref())
        .attr("title2", &parent.title)
        .attr("nocache", true)
}

/// Every playable item below a show, season, artist or album.
pub async fn all_leaves(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let parent = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(&state, &user).await?;
    let lib = libraries.iter().find(|l| l.id == parent.library_id).ok_or(PlexError::NotFound)?;
    let mut rows = Vec::new();
    if meta::is_leaf(&parent.kind) {
        rows.push(parent.clone());
    } else {
        for child in catalog::children(&state.db, user.id, id).await? {
            if meta::is_leaf(&child.kind) {
                rows.push(child);
            } else {
                rows.extend(catalog::children(&state.db, user.id, child.id).await?);
            }
        }
    }
    let (rows, total) = page.apply(rows);
    let view_group = if matches!(parent.kind.as_str(), "artist" | "album") { "track" } else { "episode" };
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(hierarchy_container(lib, &info, &parent, nodes, total, &page).attr("viewGroup", view_group))
}

// ------------------------------------------------------------------ on deck, recently added, hubs

async fn continue_rows(
    state: &AppState,
    user: &AuthUser,
    libraries: &[LibraryRow],
    library_id: Option<i64>,
    limit: i64,
) -> PlexResult<Vec<ItemRow>> {
    let rows = catalog::continue_watching(&state.db, user.id, library_id, limit).await?;
    Ok(rows.into_iter().filter(|r| libraries.iter().any(|l| l.id == r.library_id)).collect())
}

pub async fn on_deck(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let rows = continue_rows(&state, &user, &libraries, None, 50).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page).attr("title1", "On Deck"))
}

pub async fn section_on_deck(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let rows = continue_rows(&state, &user, &libraries, Some(lib.id), 50).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(Some(&lib), &info, nodes, total, &page))
}

pub async fn continue_watching(
    state: State<AppState>,
    user: PlexUser,
    info: Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    on_deck(state, user, info, params, page).await
}

pub async fn section_continue_watching(
    state: State<AppState>,
    user: PlexUser,
    info: Extension<Arc<PlexInfo>>,
    id: Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    section_on_deck(state, user, info, id, params, page).await
}

async fn recent_rows(state: &AppState, user: &AuthUser, lib: &LibraryRow, limit: i64) -> PlexResult<Vec<ItemRow>> {
    Ok(catalog::recently_added(&state.db, user.id, lib.id, lib.kind().as_str(), limit).await?)
}

pub async fn recently_added(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let mut rows = Vec::new();
    for lib in &libraries {
        rows.extend(recent_rows(&state, &user, lib, 50).await?);
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.added_at));
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page).attr("title1", "Recently Added"))
}

/// `/hubs/home/recentlyAdded?type=4&sectionID=2`: newest items of one type.
pub async fn home_recently_added(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let section = params.parse::<i64>("sectionID");
    let wanted = params.get("type").and_then(kind_for_number);
    let newest = Params(vec![("sort".into(), "addedAt:desc".into()), ("limit".into(), "50".into())]);
    let mut rows = Vec::new();
    for lib in libraries.iter().filter(|l| section.is_none_or(|s| s == l.id)) {
        match wanted {
            Some(kind) if kinds_of(lib).contains(&kind) => {
                let query = SectionQuery::new(lib.id, kind, &newest);
                rows.extend(query.page(&state.db, user.id, &Page::default()).await?.0);
            }
            Some(_) => {}
            None => rows.extend(recent_rows(&state, &user, lib, 50).await?),
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.added_at));
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page).attr("title1", "Recently Added"))
}

pub async fn section_recently_added(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let rows = recent_rows(&state, &user, &lib, 50).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(Some(&lib), &info, nodes, total, &page))
}

fn hub(identifier: String, title: String, hub_type: &str, key: Option<String>, items: Vec<Node>) -> Node {
    let ids: Vec<String> = items
        .iter()
        .filter_map(|n| match n.get("ratingKey") {
            Some(super::xml::Value::Str(id)) => Some(id.clone()),
            _ => None,
        })
        .collect();
    let mut node = Node::new("Hub")
        .attr("hubKey", format!("/library/metadata/{}", ids.join(",")))
        .opt("key", key)
        .attr("title", title)
        .attr("type", hub_type)
        .attr("hubIdentifier", identifier.clone())
        .attr("context", format!("hub.{identifier}"))
        .attr("size", items.len())
        .attr("more", false)
        .attr("style", "shelf")
        .attr("promoted", true);
    node.extend(items);
    node
}

/// The items like `id` that pley's own item page shows ("More Like This"),
/// for an item the user may see.
async fn like_this(
    state: &AppState,
    user: &AuthUser,
    id: &str,
    params: &Params,
) -> PlexResult<(Vec<ItemRow>, Vec<LibraryRow>)> {
    let id: i64 = id.parse().map_err(|_| PlexError::NotFound)?;
    let item = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(state, user).await?;
    if !libraries.iter().any(|l| l.id == item.library_id) {
        return Err(PlexError::NotFound);
    }
    let count = params.parse::<usize>("count").unwrap_or(20).clamp(1, 50);
    Ok((crate::related::related(&state.db, user.id, id, count).await?, libraries))
}

/// `/library/metadata/{id}/related`: "More Like This" as a hub, which is
/// what Plex clients show under an item.
pub async fn related(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (rows, libraries) = like_this(&state, &user, &id, &params).await?;
    let mut mc = base_container();
    if !rows.is_empty() {
        let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
        let kind = hub_type(&rows);
        let identifier = format!("{}.similar", if kind == "show" { "tv" } else { kind });
        mc.push(hub(identifier, "More Like This".into(), kind, None, nodes));
    }
    let size = usize::from(!rows.is_empty());
    Ok(mc.attr("size", size))
}

/// `/library/metadata/{id}/similar`: the same items, as a plain list.
pub async fn similar(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (rows, libraries) = like_this(&state, &user, &id, &params).await?;
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    let total = nodes.len();
    Ok(list_container(None, &info, nodes, total, &Page::default()))
}

fn hub_type(rows: &[ItemRow]) -> &'static str {
    match rows.first() {
        Some(first) if rows.iter().all(|r| r.kind == first.kind) => plex_type(&first.kind),
        _ => "mixed",
    }
}

pub async fn hubs(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let count = params.parse::<i64>("count").unwrap_or(12).clamp(1, 100);
    let mut libraries = visible_libraries(&state, &user).await?;
    if let Some(ids) = params.get("contentDirectoryID") {
        let wanted: Vec<i64> = ids.split(',').filter_map(|i| i.trim().parse().ok()).collect();
        libraries.retain(|l| wanted.contains(&l.id));
    }
    let sections = Sections::new(&libraries);
    let detail = listing_detail(&params);
    let mut hubs = Vec::new();
    let rows = continue_rows(&state, &user, &libraries, None, count).await?;
    if !rows.is_empty() {
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        hubs.push(hub(
            "home.continue".into(),
            "Continue Watching".into(),
            t,
            Some("/hubs/continueWatching/items".into()),
            items,
        ));
    }
    for lib in &libraries {
        let rows = recent_rows(&state, &user, lib, count).await?;
        if rows.is_empty() {
            continue;
        }
        let area = match section_type(lib) {
            "movie" => "movies",
            "show" => "television",
            _ => "music",
        };
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        hubs.push(hub(
            format!("home.{area}.recent"),
            format!("Recently Added in {}", lib.name),
            t,
            Some(format!("/library/sections/{}/recentlyAdded", lib.id)),
            items,
        ));
    }
    if let Some(identifiers) = params.get("identifier") {
        let wanted: Vec<&str> = identifiers.split(',').collect();
        hubs.retain(
            |h| matches!(h.get("hubIdentifier"), Some(super::xml::Value::Str(id)) if wanted.contains(&id.as_str())),
        );
    }
    let mut mc = Node::container()
        .attr("size", hubs.len())
        .attr("allowSync", false)
        .attr("identifier", "com.plexapp.plugins.library");
    mc.extend(hubs);
    Ok(mc)
}

pub async fn section_hubs(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let count = params.parse::<i64>("count").unwrap_or(12).clamp(1, 100);
    let sections = Sections::new(&libraries);
    let detail = listing_detail(&params);
    let prefix = match section_type(&lib) {
        "movie" => "movie",
        "show" => "tv",
        _ => "music",
    };
    let top = lib.kind().top_level_kind();
    let mut candidates: Vec<(String, String, Option<String>, Vec<ItemRow>)> = Vec::new();
    if top != "artist" {
        let rows = continue_rows(&state, &user, &libraries, Some(lib.id), count).await?;
        candidates.push((format!("{prefix}.inprogress"), "Continue Watching".into(), None, rows));
    }
    candidates.push((
        format!("{prefix}.recentlyadded"),
        "Recently Added".into(),
        Some(format!("/library/sections/{}/recentlyAdded", lib.id)),
        recent_rows(&state, &user, &lib, count).await?,
    ));
    if top != "artist" {
        let rows = catalog::recently_released(&state.db, user.id, lib.id, top, count).await?;
        candidates.push((format!("{prefix}.recentlyreleased"), "Recently Released".into(), None, rows));
        let rows = catalog::top_rated(&state.db, user.id, lib.id, top, count).await?;
        candidates.push((format!("{prefix}.toprated"), "Top Rated".into(), None, rows));
    }
    let mut mc = Node::container()
        .attr("allowSync", false)
        .attr("librarySectionID", lib.id)
        .attr("librarySectionTitle", &lib.name);
    let mut size = 0;
    for (identifier, title, key, rows) in candidates {
        if rows.is_empty() {
            continue;
        }
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        mc.push(hub(identifier, title, t, key, items));
        size += 1;
    }
    mc.set("size", size);
    Ok(mc)
}

// ------------------------------------------------------------------ search

async fn search_rows(
    state: &AppState,
    user: &AuthUser,
    libraries: &[LibraryRow],
    query: &str,
    section: Option<i64>,
) -> PlexResult<Vec<ItemRow>> {
    let rows = catalog::search(&state.db, user.id, query, 200).await?;
    Ok(rows
        .into_iter()
        .filter(|r| libraries.iter().any(|l| l.id == r.library_id) && section.is_none_or(|s| s == r.library_id))
        .collect())
}

pub async fn hub_search(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let query = params.get("query").unwrap_or_default().trim().to_owned();
    let limit = params.parse::<usize>("limit").unwrap_or(10).clamp(1, 100);
    let libraries = visible_libraries(&state, &user).await?;
    let section = params.parse::<i64>("sectionId");
    let rows = search_rows(&state, &user, &libraries, &query, section).await?;
    let sections = Sections::new(&libraries);
    let mut mc = Node::container().attr("allowSync", false).attr("identifier", "com.plexapp.plugins.library");
    let mut size = 0;
    for kind in ["movie", "show", "season", "episode", "artist", "album", "track"] {
        let group: Vec<ItemRow> = rows.iter().filter(|r| r.kind == kind).take(limit).cloned().collect();
        if group.is_empty() {
            continue;
        }
        let items =
            item_nodes(&state, &group, &sections, Detail { guids: true, full: false, media: true, markers: false })
                .await?;
        mc.push(hub(kind.into(), plural(kind).into(), kind, None, items));
        size += 1;
    }
    mc.set("size", size);
    Ok(mc)
}

pub async fn search(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let query = params.get("query").unwrap_or_default().trim().to_owned();
    let mut rows = search_rows(&state, &user, &libraries, &query, params.parse("sectionId")).await?;
    if let Some(kind) = params.get("type").and_then(kind_for_number) {
        rows.retain(|r| r.kind == kind);
    }
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page))
}

pub async fn section_search(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let query = params.get("query").unwrap_or_default().trim().to_owned();
    let mut rows = search_rows(&state, &user, &libraries, &query, Some(lib.id)).await?;
    if let Some(kind) = params.get("type").and_then(kind_for_number) {
        rows.retain(|r| r.kind == kind);
    }
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(Some(&lib), &info, nodes, total, &page))
}

// ------------------------------------------------------------------ artwork

/// Serves Plex image URLs (`/library/metadata/{id}/thumb/{version}`) with
/// pley's artwork handler: posters, backgrounds and episode stills.
pub(super) async fn serve_image(
    state: AppState,
    user: AuthUser,
    id: i64,
    image: &str,
    version: Option<String>,
    width: Option<u32>,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let kind = match image {
        "thumb" if row.kind == "episode" => "thumb",
        "thumb" => "poster",
        "art" => "backdrop",
        _ => return Err(PlexError::NotFound),
    };
    let mut query = HashMap::new();
    if let Some(version) = version {
        query.insert("v".to_owned(), version);
    }
    if let Some(width) = width {
        query.insert("w".to_owned(), width.to_string());
    }
    crate::api::items::image(State(state), user, Path((id, kind.to_owned())), Query(query), headers)
        .await
        .map_err(PlexError::from)
}

pub async fn image(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, image)): Path<(i64, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    serve_image(state, user, id, &image, None, params.parse("width"), headers).await
}

pub async fn image_version(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, image, version)): Path<(i64, String, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    serve_image(state, user, id, &image, Some(version), params.parse("width"), headers).await
}

/// `/photo/:/transcode?url=/library/metadata/1/thumb/123&width=300&height=450`
pub async fn photo_transcode(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let url = params.get("url").ok_or(PlexError::BadRequest("url is required".into()))?;
    if let Some(response) = super::lists::photo_transcode(&state, &user, url, &params, &headers).await? {
        return Ok(response);
    }
    // Absolute URLs pointing back at this server are fine too.
    let path = match url.find("/library/metadata/") {
        Some(start) => &url[start..],
        None => return Err(PlexError::NotFound),
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path.trim_start_matches("/library/metadata/").split('/');
    let id: i64 = segments.next().and_then(|s| s.parse().ok()).ok_or(PlexError::NotFound)?;
    let image = segments.next().unwrap_or("thumb").to_owned();
    let version = segments.next().map(str::to_owned);
    let width = params.parse::<u32>("width").filter(|w| *w > 0);
    serve_image(state, user, id, &image, version, width, headers).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Params {
        Params(pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect())
    }

    #[test]
    fn parses_plex_filters() {
        let filters = Filter::parse(
            &params(&[
                ("type", "1"),
                ("title", "=Exact"),
                ("year>>", "2000"),
                ("genre!", "Horror,Comedy"),
                ("show.title<", "The"),
                ("X-Plex-Token", "secret"),
                ("includeGuids", "1"),
            ]),
            "movie",
        );
        assert_eq!(filters.len(), 3, "{filters:?}");
        assert_eq!((filters[0].field.as_str(), filters[0].op), ("title", Op::Exactly));
        assert_eq!(filters[0].values, vec!["Exact"]);
        assert_eq!((filters[1].field.as_str(), filters[1].op), ("year", Op::Greater));
        assert_eq!((filters[2].op, filters[2].values.len()), (Op::Not, 2));

        let filters = Filter::parse(&params(&[("show.unwatched", "1")]), "show");
        assert_eq!((filters[0].field.as_str(), filters[0].op), ("unwatched", Op::Is));

        // Ancestor filters (python-plexapi's `artist.albums()`), descendants ignored.
        let filters = Filter::parse(&params(&[("artist.id", "16"), ("track.title", "x")]), "album");
        assert_eq!(filters.len(), 1);
        assert_eq!((filters[0].level, filters[0].field.as_str()), (Level::Parent, "id"));
        let filters = Filter::parse(&params(&[("show.id", "3")]), "episode");
        assert_eq!(filters[0].level, Level::Grandparent);
    }

    #[test]
    fn parses_sorts_and_dates() {
        assert_eq!(
            parse_sort(Some("movie.addedAt:desc,titleSort")),
            vec![(SortField::Added, true), (SortField::Title, false)]
        );
        assert_eq!(parse_sort(None), vec![(SortField::Title, false)]);
        assert_eq!(date_secs("1970-01-02"), Some(86_400));
        assert_eq!(date_secs("2000-03-01"), Some(951_868_800));
        assert_eq!(parse_date("-2d", 1_000_000), Some(1_000_000 - 2 * 86_400));
        assert_eq!(parse_date("1700000000", 0), Some(1_700_000_000));
        assert_eq!(encode("Sci-Fi & Fantasy"), "Sci-Fi%20%26%20Fantasy");
    }

    fn row(id: i64, kind: &str, title: &str) -> ItemRow {
        ItemRow::test_row(id, kind, title)
    }

    /// Titles of `rows` that pass every filter in `pairs` (query keys as
    /// Plex clients send them: `year>>=2000` arrives as `("year>>", "2000")`).
    fn matching(
        rows: &[ItemRow],
        kind: &str,
        pairs: &[(&str, &str)],
        extras: &HashMap<i64, Extras>,
        now: i64,
    ) -> Vec<String> {
        let filters = Filter::parse(&params(pairs), kind);
        rows.iter()
            .filter(|r| filters.iter().all(|f| f.matches(r, extras.get(&r.id), now)))
            .map(|r| r.title.clone())
            .collect()
    }

    #[test]
    fn string_filters_follow_plex_operators() {
        let rows: Vec<ItemRow> = ["Alpha", "Alphabet City", "Bravo", "The Charlie Movie"]
            .iter()
            .enumerate()
            .map(|(i, t)| row(i as i64 + 1, "movie", t))
            .collect();
        let none = HashMap::new();
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &none, 0);
        assert_eq!(titles(&[("title", "ALPHA")]), ["Alpha", "Alphabet City"], "contains, any case");
        assert_eq!(titles(&[("title!", "alpha")]), ["Bravo", "The Charlie Movie"], "does not contain");
        assert_eq!(titles(&[("title", "=alpha")]), ["Alpha"], "`==` is");
        assert_eq!(titles(&[("title!", "=Alpha")]), ["Alphabet City", "Bravo", "The Charlie Movie"], "`!==` is not");
        assert_eq!(titles(&[("title<", "the")]), ["The Charlie Movie"], "`<=` begins with");
        assert_eq!(titles(&[("title>", "MOVIE")]), ["The Charlie Movie"], "`>=` ends with");
        assert_eq!(titles(&[("title", "bravo,charlie")]), ["Bravo", "The Charlie Movie"], "any of several values");
        // Every filter must match.
        assert_eq!(titles(&[("title", "alpha"), ("title>", "city")]), ["Alphabet City"]);
    }

    #[test]
    fn number_filters_compare_and_treat_missing_values_as_different() {
        let mut rows = Vec::new();
        for (id, title, year) in [(1, "Alpha", Some(1999)), (2, "Bravo", Some(2001)), (3, "Charlie", Some(2010))] {
            let mut r = row(id, "movie", title);
            r.year = year;
            rows.push(r);
        }
        rows.push(row(4, "movie", "Delta"));
        let none = HashMap::new();
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &none, 0);
        assert_eq!(titles(&[("year", "2001")]), ["Bravo"]);
        assert_eq!(titles(&[("year", "1999,2010")]), ["Alpha", "Charlie"]);
        assert_eq!(titles(&[("year!", "2001")]), ["Alpha", "Charlie", "Delta"], "no year is not 2001");
        assert_eq!(titles(&[("year>>", "2001")]), ["Charlie"], "strictly after");
        assert_eq!(titles(&[("year<<", "2001")]), ["Alpha"], "strictly before; no year is never before");
        assert_eq!(titles(&[("decade", "2000")]), ["Bravo"]);
        assert_eq!(titles(&[("decade", "1990")]), ["Alpha"]);
        assert_eq!(titles(&[("id", "3")]), ["Charlie"]);
    }

    #[test]
    fn watch_state_filters() {
        let mut watched = row(1, "movie", "Watched");
        watched.watched = true;
        watched.play_count = 2;
        let mut started = row(2, "movie", "Started");
        started.position_ms = 5_000;
        let fresh = row(3, "movie", "Fresh");
        let rows = vec![watched, started, fresh];
        let none = HashMap::new();
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &none, 0);
        assert_eq!(titles(&[("unwatched", "1")]), ["Started", "Fresh"]);
        assert_eq!(titles(&[("unwatched", "0")]), ["Watched"]);
        assert_eq!(titles(&[("unwatched!", "1")]), ["Watched"], "`!=` negates");
        assert_eq!(titles(&[("inProgress", "1")]), ["Started"]);
        assert_eq!(titles(&[("viewCount>>", "1")]), ["Watched"]);

        // A show counts as watched once every episode is.
        let mut done = row(4, "show", "Done");
        (done.leaf_count, done.viewed_leaf_count) = (Some(3), Some(3));
        let mut halfway = row(5, "show", "Halfway");
        (halfway.leaf_count, halfway.viewed_leaf_count) = (Some(3), Some(1));
        let mut empty = row(6, "show", "Empty");
        (empty.leaf_count, empty.viewed_leaf_count) = (Some(0), Some(0));
        let shows = vec![done, halfway, empty];
        assert_eq!(matching(&shows, "show", &[("unwatched", "1")], &none, 0), ["Halfway", "Empty"]);
        assert_eq!(matching(&shows, "show", &[("show.unwatched", "0")], &none, 0), ["Done"]);
    }

    #[test]
    fn tag_studio_and_guid_filters_use_the_extras() {
        let rows = vec![row(1, "movie", "Alien"), row(2, "movie", "Airplane")];
        let mut extras = HashMap::new();
        extras.insert(
            1,
            Extras {
                genres: vec!["Horror".into(), "Science Fiction".into()],
                content_rating: Some("R".into()),
                studio: Some("Brandywine Productions".into()),
                imdb_id: Some("tt0078748".into()),
                tmdb_id: Some(348),
                ..Default::default()
            },
        );
        extras.insert(
            2,
            Extras { genres: vec!["Comedy".into()], content_rating: Some("PG".into()), ..Default::default() },
        );
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &extras, 0);
        assert_eq!(titles(&[("genre", "horror")]), ["Alien"], "tags ignore case");
        assert_eq!(titles(&[("genre!", "horror")]), ["Airplane"]);
        assert_eq!(titles(&[("genre", "comedy,horror")]), ["Alien", "Airplane"]);
        assert_eq!(titles(&[("contentRating", "PG")]), ["Airplane"], "a tag is matched whole, not as a prefix");
        assert_eq!(titles(&[("studio", "brandywine")]), ["Alien"]);
        assert_eq!(titles(&[("studio!", "brandywine")]), ["Airplane"], "no studio does not contain it");
        assert_eq!(titles(&[("guid", "imdb://tt0078748")]), ["Alien"]);
        assert_eq!(titles(&[("guid", "tmdb://348")]), ["Alien"]);
        let own = meta::guid("movie", 2);
        assert_eq!(titles(&[("guid", own.as_str())]), ["Airplane"]);

        // TheTVDB ids are shows', MusicBrainz ids artists' and albums'.
        let others = vec![row(3, "show", "Lost"), row(4, "album", "Kid A"), row(5, "movie", "Heat")];
        extras.insert(3, Extras { tvdb_id: Some(73739), ..Default::default() });
        extras.insert(4, Extras { musicbrainz_id: Some("b8048f24".into()), ..Default::default() });
        extras.insert(5, Extras { tvdb_id: Some(1), musicbrainz_id: Some("x".into()), ..Default::default() });
        let found = |kind: &str, guid: &str| matching(&others, kind, &[("guid", guid)], &extras, 0);
        assert_eq!(found("show", "tvdb://73739"), ["Lost"]);
        assert_eq!(found("album", "mbid://b8048f24"), ["Kid A"]);
        assert!(found("movie", "tvdb://1").is_empty() && found("movie", "mbid://x").is_empty());
    }

    #[test]
    fn people_filters_match_ids_or_names_in_their_role() {
        let credit = |id: i64, name: &str, role: people::Role| people::Credit {
            id,
            name: name.into(),
            role,
            character: None,
            photo_url: None,
            photo_source: None,
        };
        let rows = vec![row(1, "movie", "Alpha"), row(2, "movie", "Beta")];
        let mut extras = HashMap::new();
        extras.insert(
            1,
            Extras {
                people: vec![
                    credit(7, "Ann Actor", people::Role::Actor),
                    credit(8, "Dan Director", people::Role::Director),
                ],
                ..Default::default()
            },
        );
        extras.insert(
            2,
            Extras { people: vec![credit(8, "Dan Director", people::Role::Director)], ..Default::default() },
        );
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &extras, 0);
        assert_eq!(titles(&[("actor", "7")]), ["Alpha"]);
        assert_eq!(titles(&[("actor", "ann actor")]), ["Alpha"]);
        assert_eq!(titles(&[("actor!", "7")]), ["Beta"]);
        assert_eq!(titles(&[("director", "8")]), ["Alpha", "Beta"]);
        assert_eq!(titles(&[("actor", "8")]), Vec::<String>::new(), "directing is not acting");
        assert_eq!(titles(&[("writer", "Dan Director")]), Vec::<String>::new());
    }

    #[test]
    fn date_filters_take_unix_times_dates_and_relative_times() {
        let now = 1_000_000_000;
        let mut recent = row(1, "movie", "Recent");
        recent.added_at = now - 2 * 86_400;
        recent.last_played_at = Some(now - 3_600);
        recent.release_date = Some("2000-01-02".into());
        let mut old = row(2, "movie", "Old");
        old.added_at = now - 40 * 86_400;
        old.release_date = Some("1999-12-31".into());
        let rows = vec![recent, old];
        let none = HashMap::new();
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &none, now);
        assert_eq!(titles(&[("addedAt>>", "-30d")]), ["Recent"]);
        assert_eq!(titles(&[("addedAt<<", "-30d")]), ["Old"]);
        let exact = (now - 40 * 86_400 + 600).to_string();
        assert_eq!(titles(&[("addedAt", exact.as_str())]), ["Old"], "`=` is the same day");
        assert_eq!(titles(&[("lastViewedAt>>", "-1d")]), ["Recent"], "never played never matches");
        assert_eq!(titles(&[("originallyAvailableAt>>", "2000-01-01")]), ["Recent"]);
        assert_eq!(titles(&[("addedAt>>", "-3x")]), ["Recent", "Old"], "an unreadable date filters nothing");
    }

    #[test]
    fn ancestor_filters_look_at_the_parent_or_grandparent() {
        let episode = |id: i64, title: &str, season: i64, show_id: i64, show: &str| {
            let mut r = row(id, "episode", title);
            r.parent_id = Some(100 + id);
            r.parent_title = Some(format!("Season {season}"));
            r.parent_idx = Some(season);
            r.grandparent_id = Some(show_id);
            r.grandparent_title = Some(show.into());
            r
        };
        let rows = vec![episode(1, "Pilot", 1, 10, "Lost"), episode(2, "Return", 2, 11, "Fringe")];
        let none = HashMap::new();
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "episode", pairs, &none, 0);
        assert_eq!(titles(&[("show.title", "lost")]), ["Pilot"]);
        assert_eq!(titles(&[("show.id", "11")]), ["Return"]);
        assert_eq!(titles(&[("season.index", "2")]), ["Return"]);
        assert_eq!(titles(&[("season.id", "101")]), ["Pilot"]);
        assert_eq!(titles(&[("show.year", "2004")]), ["Pilot", "Return"], "unsupported ancestor fields are ignored");

        let mut album = row(3, "album", "Discovery");
        album.parent_id = Some(5);
        album.parent_title = Some("Daft Punk".into());
        let albums = vec![album, row(4, "album", "Loose")];
        assert_eq!(matching(&albums, "album", &[("artist.id", "5")], &none, 0), ["Discovery"]);
        assert_eq!(matching(&albums, "album", &[("artist.title!", "daft")], &none, 0), ["Loose"]);
    }

    #[test]
    fn unknown_and_non_ascii_parameters_filter_nothing() {
        let rows = vec![row(1, "movie", "Alpha"), row(2, "movie", "Beta")];
        let none = HashMap::new();
        let pairs = [
            ("resolution", "4k"),
            ("X-Plex-Product", "Plex Web"),
            ("x-plex-container-start", "0"),
            ("includeGuids", "1"),
            ("sort", "titleSort"),
            // Keys whose 7th byte falls inside a character.
            ("abcdefé", "1"),
            ("ééééé", "x"),
        ];
        assert_eq!(matching(&rows, "movie", &pairs, &none, 0), ["Alpha", "Beta"]);
        let filters = Filter::parse(&params(&pairs), "movie");
        let fields: Vec<&str> = filters.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["resolution", "abcdefé", "ééééé"]);
    }

    #[test]
    fn sorting_keeps_missing_values_last_in_both_directions() {
        let mut rows = Vec::new();
        for (id, title, year, rating) in [
            (1, "Alpha", Some(2001), Some(7.5)),
            (2, "Bravo", None, Some(9.0)),
            (3, "Charlie", Some(1999), None),
            (4, "Delta", Some(2001), Some(6.0)),
        ] {
            let mut r = row(id, "movie", title);
            r.year = year;
            r.rating = rating;
            rows.push(r);
        }
        let sorted = |spec: &str| {
            let mut rows = rows.clone();
            sort_rows(&mut rows, &parse_sort(Some(spec)), &HashMap::new());
            rows.iter().map(|r| r.title.clone()).collect::<Vec<_>>()
        };
        assert_eq!(sorted("year"), ["Charlie", "Alpha", "Delta", "Bravo"], "ties by title, missing last");
        assert_eq!(sorted("year:desc"), ["Alpha", "Delta", "Charlie", "Bravo"]);
        assert_eq!(sorted("rating:desc,titleSort"), ["Bravo", "Alpha", "Delta", "Charlie"]);
        assert_eq!(sorted("audienceRating"), ["Delta", "Alpha", "Bravo", "Charlie"]);
        assert_eq!(sorted("titleSort:desc"), ["Delta", "Charlie", "Bravo", "Alpha"]);
        assert_eq!(sorted("random"), ["Alpha", "Bravo", "Charlie", "Delta"], "unknown sorts fall back to title");
        assert_eq!(parse_sort(Some("random,id:desc")), vec![(SortField::Id, true)]);
    }

    #[test]
    fn relative_dates_and_calendar_dates() {
        let now = 10_000_000;
        assert_eq!(parse_date("-30s", now), Some(now - 30));
        assert_eq!(parse_date("-5m", now), Some(now - 300));
        assert_eq!(parse_date("-2h", now), Some(now - 7_200));
        assert_eq!(parse_date("-1w", now), Some(now - 7 * 86_400));
        assert_eq!(parse_date("-1mon", now), Some(now - 30 * 86_400));
        assert_eq!(parse_date("-1y", now), Some(now - 365 * 86_400));
        assert_eq!(parse_date("-3x", now), None);
        assert_eq!(parse_date("soon", now), None);
        assert_eq!(parse_date("2024-02-29", now), Some(1_709_164_800));
        assert_eq!(date_secs("2024-05-01T12:30:00Z"), date_secs("2024-05-01"));
        assert_eq!(date_secs("2024-13-01"), None);
        assert_eq!(date_secs("2024-00-10"), None);
        assert_eq!(date_secs("2024-5-1"), None);
        assert_eq!(date_secs(""), None);
    }

    #[test]
    fn dates_out_of_range_neither_overflow_nor_filter_wrongly() {
        let now = 1_000_000_000;
        // Further back than an i64 reaches is the earliest time there is.
        assert_eq!(parse_date("-9999999999999y", now), Some(i64::MIN));
        assert_eq!(parse_date("-99999999999999999999d", now), Some(i64::MIN));
        assert_eq!(parse_date("-0d", now), Some(now));
        assert_eq!(parse_date("-d", now), None);
        assert_eq!(parse_date("-99999999999999999999x", now), None, "the unit is checked first");

        let mut old = row(1, "movie", "Old");
        old.added_at = -5_000_000_000;
        let mut new = row(2, "movie", "New");
        new.added_at = now;
        let rows = vec![old, new];
        let none = HashMap::new();
        let titles = |pairs: &[(&str, &str)]| matching(&rows, "movie", pairs, &none, now);
        let nothing = Vec::<String>::new();
        assert_eq!(titles(&[("addedAt>>", "-9999999999999y")]), ["Old", "New"], "everything is newer than that");
        assert_eq!(titles(&[("addedAt<<", "-9999999999999y")]), nothing);
        // Same day or not, however far apart the times are.
        assert_eq!(titles(&[("addedAt", "-9999999999999y")]), nothing);
        assert_eq!(titles(&[("addedAt!", "-9999999999999y")]), ["Old", "New"]);
        let (max, min) = (i64::MAX.to_string(), i64::MIN.to_string());
        assert_eq!(titles(&[("addedAt", max.as_str())]), nothing);
        assert_eq!(titles(&[("addedAt", min.as_str())]), nothing);
        assert_eq!(titles(&[("addedAt!", min.as_str())]), ["Old", "New"]);
        assert_eq!(titles(&[("addedAt>>", min.as_str())]), ["Old", "New"]);
    }

    /// What the text filters in SQL rely on.
    #[test]
    fn only_two_characters_lowercase_to_ascii() {
        let odd: Vec<char> =
            (0x80..=0x10_FFFF).filter_map(char::from_u32).filter(|c| c.to_lowercase().any(|l| l.is_ascii())).collect();
        assert_eq!(odd, ['\u{130}', '\u{212A}']);
    }

    // ------------------------------------------------------------------ listings in SQL

    #[derive(Default)]
    struct Item {
        id: i64,
        library: i64,
        kind: &'static str,
        title: &'static str,
        sort: &'static str,
        parent: Option<i64>,
        grandparent: Option<i64>,
        idx: Option<i64>,
        year: Option<i64>,
        rating: Option<f64>,
        genres: Option<&'static str>,
        content_rating: Option<&'static str>,
        age: Option<i64>,
        studio: Option<&'static str>,
        release: Option<&'static str>,
        duration: Option<i64>,
        added: i64,
        updated: i64,
        imdb: Option<&'static str>,
        tmdb: Option<i64>,
        tvdb: Option<i64>,
        leaves: Option<i64>,
    }

    async fn exec(db: &Db, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&db.write).await.unwrap();
    }

    /// Movies (library 1) and shows (library 2) with what filters and sorts
    /// look at, ties included, for an administrator (user 1) and a user who
    /// may open both libraries and watch titles up to 12 (user 2).
    async fn listing_fixture(dir: &std::path::Path) -> Db {
        let db = Db::open(&dir.join("pley.db")).await.unwrap();
        exec(
            &db,
            "INSERT INTO users (id, username, password_hash, is_admin, created_at, all_libraries, max_age)
             VALUES (1, 'admin', 'x', 1, 0, 1, NULL), (2, 'kid', 'x', 0, 0, 0, 12)",
        )
        .await;
        exec(
            &db,
            "INSERT INTO libraries (id, name, kind, paths, created_at)
             VALUES (1, 'Movies', 'movies', '[]', 0), (2, 'Shows', 'shows', '[]', 0)",
        )
        .await;
        exec(&db, "INSERT INTO user_libraries (user_id, library_id) VALUES (2, 1), (2, 2)").await;
        let movie = |id, title, sort| Item { id, library: 1, kind: "movie", title, sort, ..Default::default() };
        let items = [
            Item {
                year: Some(2001),
                rating: Some(7.5),
                genres: Some(r#"["Horror","Drama"]"#),
                content_rating: Some("R"),
                age: Some(17),
                studio: Some("Brandywine"),
                release: Some("2001-05-01"),
                duration: Some(5_400_000),
                added: 1_000,
                updated: 1_000,
                imdb: Some("tt0001"),
                tmdb: Some(348),
                ..movie(1, "Alpha", "alpha")
            },
            // Ties with Alpha on everything but the id.
            Item {
                year: Some(2001),
                rating: Some(7.5),
                genres: Some(r#"["drama"]"#),
                content_rating: Some("PG"),
                age: Some(10),
                release: Some("2001-05-01"),
                duration: Some(5_400_000),
                added: 1_000,
                updated: 1_000,
                ..movie(2, "alpha", "alpha")
            },
            Item {
                rating: Some(9.0),
                genres: Some("[]"),
                content_rating: Some("pg-13"),
                age: Some(13),
                studio: Some("ÉTUDE Films"),
                added: 2_000,
                updated: 3_000,
                imdb: Some("tt0003"),
                tmdb: Some(3),
                ..movie(3, "Beta", "beta")
            },
            // Unrated: hidden from user 2.
            Item {
                year: Some(1999),
                studio: Some("Day Studio"),
                release: Some("1999-12-31"),
                duration: Some(6_000_000),
                added: 500,
                updated: 500,
                ..movie(4, "The Gamma", "gamma")
            },
            Item {
                year: Some(2010),
                rating: Some(6.0),
                genres: Some(r#"["Comedy"]"#),
                content_rating: Some("G"),
                age: Some(0),
                release: Some("2010"),
                duration: Some(4_000_000),
                added: 999_000,
                updated: 4_000,
                ..movie(5, "École", "école")
            },
            // Genres that aren't JSON; a date that doesn't exist.
            Item {
                year: Some(2005),
                rating: Some(8.1),
                genres: Some("nope"),
                content_rating: Some("PG"),
                age: Some(10),
                studio: Some("Night Studio"),
                release: Some("2005-02-31"),
                duration: Some(7_200_000),
                added: 998_000,
                updated: 998_000,
                ..movie(6, "İstanbul", "istanbul")
            },
            // With a Kelvin sign: lowercase "kiss".
            Item {
                year: Some(2005),
                rating: Some(8.1),
                genres: Some(r#"["Horror"]"#),
                content_rating: Some("PG"),
                age: Some(10),
                added: 86_400,
                updated: 86_400,
                ..movie(7, "\u{212A}iss", "kiss")
            },
            Item {
                year: Some(1985),
                rating: Some(5.0),
                genres: Some(r#"["Horror","Comedy"]"#),
                content_rating: Some("PG"),
                age: Some(10),
                release: Some("1985-07-03"),
                duration: Some(3_000_000),
                added: 2_000,
                updated: 2_000,
                ..movie(8, "123 Go", "123 go")
            },
            Item {
                year: Some(2020),
                rating: Some(7.0),
                genres: Some(r#"["Drama"]"#),
                content_rating: Some("R"),
                age: Some(17),
                studio: Some("Night Studio"),
                release: Some("2020-01-01"),
                duration: Some(5_000_000),
                added: 3_000,
                updated: 3_000,
                ..movie(9, "Zeta", "zeta")
            },
            Item {
                year: Some(2020),
                rating: Some(7.0),
                genres: Some(r#"["Drama"]"#),
                content_rating: Some("PG"),
                age: Some(10),
                release: Some("2020-01-01"),
                duration: Some(5_000_000),
                added: 3_000,
                updated: 3_000,
                ..movie(10, "zeta", "zeta")
            },
            Item {
                year: Some(1969),
                rating: Some(3.3),
                genres: Some(r#"["Science Fiction"]"#),
                content_rating: Some("G"),
                age: Some(0),
                studio: Some("MGM"),
                release: Some("1969-10-01"),
                added: 100,
                updated: 100,
                ..movie(11, "Omega", "omega")
            },
            Item {
                content_rating: Some("PG"),
                age: Some(10),
                added: 4_000,
                updated: 4_000,
                ..movie(12, "Delta Force", "delta force")
            },
            // A show user 2 may not watch, one they may, and two without episodes.
            Item {
                library: 2,
                kind: "show",
                year: Some(2004),
                rating: Some(8.0),
                genres: Some(r#"["Drama"]"#),
                age: Some(14),
                added: 5_000,
                updated: 5_000,
                tvdb: Some(73739),
                leaves: Some(4),
                ..movie(100, "Lost", "lost")
            },
            Item {
                library: 2,
                kind: "show",
                year: Some(2008),
                rating: Some(7.0),
                genres: Some(r#"["Sci-Fi"]"#),
                age: Some(10),
                added: 6_000,
                updated: 6_000,
                leaves: Some(2),
                ..movie(110, "Fringe", "fringe")
            },
            Item {
                library: 2,
                kind: "show",
                age: Some(0),
                added: 7_000,
                updated: 7_000,
                leaves: Some(0),
                ..movie(120, "Empty", "empty")
            },
            Item { library: 2, kind: "show", age: Some(0), added: 7_000, updated: 7_000, ..movie(121, "Bare", "bare") },
        ];
        let season = |id, show, idx, title, leaves| Item {
            id,
            library: 2,
            kind: "season",
            title,
            sort: title,
            parent: Some(show),
            idx: Some(idx),
            added: 5_000 + id,
            updated: 5_000 + id,
            leaves: Some(leaves),
            ..Default::default()
        };
        let episode = |id, season, show, idx, title, sort| Item {
            id,
            library: 2,
            kind: "episode",
            title,
            sort,
            parent: Some(season),
            grandparent: Some(show),
            idx: Some(idx),
            duration: Some(2_000_000 + id),
            release: Some("2004-09-22"),
            added: 5_000 + id % 3,
            updated: 5_000,
            ..Default::default()
        };
        let children = [
            season(101, 100, 1, "Season 1", 3),
            season(102, 100, 2, "Season 2", 1),
            season(111, 110, 1, "Season 1", 2),
            episode(103, 101, 100, 1, "Pilot", "pilot"),
            episode(104, 101, 100, 2, "Tabula Rasa", "tabula rasa"),
            episode(105, 101, 100, 3, "Walkabout", "walkabout"),
            episode(106, 102, 100, 1, "Man of Science", "man of science"),
            episode(112, 111, 110, 1, "Pilot", "pilot"),
            episode(113, 111, 110, 2, "The Same Old Story", "same old story"),
        ];
        for item in items.iter().chain(&children) {
            sqlx::query(
                "INSERT INTO items (id, library_id, kind, key, title, sort_title, parent_id, grandparent_id, idx,
                    year, rating, genres, content_rating, rating_age, studio, release_date, duration_ms,
                    added_at, updated_at, imdb_id, tmdb_id, tvdb_id, leaf_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.id)
            .bind(item.library)
            .bind(item.kind)
            .bind(format!("key-{}", item.id))
            .bind(item.title)
            .bind(item.sort)
            .bind(item.parent)
            .bind(item.grandparent)
            .bind(item.idx)
            .bind(item.year)
            .bind(item.rating)
            .bind(item.genres)
            .bind(item.content_rating)
            .bind(item.age)
            .bind(item.studio)
            .bind(item.release)
            .bind(item.duration)
            .bind(item.added)
            .bind(item.updated)
            .bind(item.imdb)
            .bind(item.tmdb)
            .bind(item.tvdb)
            .bind(item.leaves)
            .execute(&db.write)
            .await
            .unwrap();
        }
        exec(
            &db,
            "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at, rating)
             VALUES (1, 1, 0, 1, 2, 5000, 0, 8), (1, 2, 0, 1, 0, 6000, 0, NULL), (1, 3, 5000, 0, 0, 7000, 0, NULL),
                    (1, 5, 0, 0, 3, NULL, 0, 4), (1, 9, 0, 1, 1, NULL, 0, NULL), (1, 7, 0, 0, 0, 999500, 0, 10),
                    (1, 103, 0, 1, 1, 5000, 0, NULL), (1, 104, 0, 1, 1, 5001, 0, NULL), (1, 105, 0, 1, 1, 5002, 0, NULL),
                    (1, 106, 0, 1, 1, 5003, 0, NULL), (1, 112, 0, 1, 1, 5004, 0, NULL), (1, 113, 1000, 0, 0, 5005, 0, NULL),
                    (1, 110, 0, 0, 0, 7777, 0, NULL),
                    (2, 2, 0, 1, 1, 100, 0, NULL), (2, 7, 1000, 0, 0, 200, 0, 6), (2, 112, 0, 1, 1, 300, 0, NULL),
                    (2, 113, 0, 1, 1, 400, 0, NULL)",
        )
        .await;
        exec(
            &db,
            "INSERT INTO item_labels (item_id, label) VALUES (1, 'Overlay'), (3, 'overlay'), (5, '4K'), (100, 'Kids')",
        )
        .await;
        exec(
            &db,
            "INSERT INTO people (id, tmdb_id, name, updated_at)
             VALUES (1, 1, 'Ann Actor', 0), (2, 2, 'Dan Director', 0), (3, 3, 'ANN ACTOR', 0), (4, 4, '7', 0)",
        )
        .await;
        exec(
            &db,
            "INSERT INTO item_people (item_id, person_id, role, character, position)
             VALUES (1, 1, 'actor', NULL, 0), (1, 2, 'director', NULL, 1), (2, 2, 'director', NULL, 0),
                    (3, 3, 'actor', NULL, 0), (5, 4, 'actor', NULL, 0), (7, 1, 'writer', NULL, 0),
                    (100, 1, 'actor', NULL, 0)",
        )
        .await;
        db
    }

    /// Ids and total of a listing, through SQL (`None` when it can't be).
    async fn in_sql(db: &Db, user: i64, query: &SectionQuery<'_>, page: &Page) -> Option<(Vec<i64>, usize)> {
        let filters = query.sql_filters()?;
        let (rows, total) = query.page_in_sql(db, user, &filters, page).await.unwrap();
        Some((rows.iter().map(|r| r.id).collect(), total))
    }

    async fn in_memory(db: &Db, user: i64, query: &SectionQuery<'_>, page: &Page) -> (Vec<i64>, usize) {
        let (rows, total) = query.page_in_memory(db, user, page).await.unwrap();
        (rows.iter().map(|r| r.id).collect(), total)
    }

    const NOW: i64 = 1_000_000;

    fn section_query<'a>(library: i64, kind: &'a str, pairs: &[(&str, &str)]) -> SectionQuery<'a> {
        SectionQuery { now: NOW, ..SectionQuery::new(library, kind, &params(pairs)) }
    }

    const SORTS: &[&str] = &[
        "",
        "titleSort",
        "titleSort:desc",
        "addedAt",
        "addedAt:desc",
        "originallyAvailableAt",
        "originallyAvailableAt:desc",
        "year",
        "year:desc",
        "rating",
        "audienceRating:desc",
        "userRating",
        "userRating:desc",
        "lastViewedAt",
        "lastViewedAt:desc",
        "viewCount",
        "viewCount:desc",
        "duration",
        "duration:desc",
        "index",
        "index:desc",
        "id:desc",
        "updatedAt:desc",
        "year:desc,rating,titleSort:desc",
        "random",
    ];

    #[tokio::test]
    async fn filters_and_sorts_in_sql_list_what_the_in_memory_path_lists() {
        let dir = tempfile::tempdir().unwrap();
        let db = listing_fixture(dir.path()).await;
        let guid = meta::guid("movie", 3);
        let show_guid = meta::guid("show", 3);
        let mut movie_filters: Vec<Vec<(&str, &str)>> = vec![
            vec![],
            vec![("title", "alpha")],
            vec![("title", "ALPHA")],
            vec![("title!", "alpha")],
            vec![("title", "=alpha")],
            vec![("title!", "=alpha")],
            vec![("title<", "al")],
            vec![("title>", "TA")],
            vec![("title", "i")],
            vec![("title<", "i")],
            vec![("title>", "L")],
            vec![("title", "cole")],
            vec![("title>", "le")],
            vec![("title", "=kiss")],
            vec![("title<", "k")],
            vec![("title", "x,ze")],
            vec![("title>>", "a")],
            vec![("title", "")],
            vec![("title!", "")],
            vec![("title", "123")],
            vec![("studio", "brandy")],
            vec![("studio!", "studio")],
            vec![("studio", "tude")],
            vec![("studio", "=")],
            vec![("year", "2001")],
            vec![("year", "2001,1999")],
            vec![("year!", "2001")],
            vec![("year>>", "2000")],
            vec![("year<<", "2000")],
            vec![("year", "abc")],
            vec![("year!", "abc")],
            vec![("year", "nan")],
            vec![("year!", "NaN")],
            vec![("year>>", "nan")],
            vec![("year<<", "inf")],
            vec![("year>>", "-inf")],
            vec![("year", "2001.0000000001")],
            vec![("year", "2001.1")],
            vec![("year<", "20")],
            vec![("decade", "2000")],
            vec![("decade", "1960")],
            vec![("decade!", "2000")],
            vec![("rating>>", "7.5")],
            vec![("rating", "8.1")],
            vec![("audienceRating<<", "7")],
            vec![("duration<<", "5000000")],
            vec![("duration", "5400000")],
            vec![("index", "3")],
            vec![("index!", "3")],
            vec![("id", "3")],
            vec![("id>>", "5")],
            vec![("viewCount>>", "0")],
            vec![("viewCount", "0")],
            vec![("viewCount", "1")],
            vec![("viewCount>>", "1")],
            vec![("viewOffset>>", "0")],
            vec![("userRating>>", "5")],
            vec![("userRating", "4")],
            vec![("userRating!", "4")],
            vec![("leafCount>>", "1")],
            vec![("leafCount!", "1")],
            vec![("genre", "horror")],
            vec![("genre", "Drama,Comedy")],
            vec![("genre!", "horror")],
            vec![("genre", "")],
            vec![("genre!", "")],
            vec![("genre>>", "horror")],
            vec![("contentRating", "pg")],
            vec![("contentRating!", "R")],
            vec![("contentRating", "PG-13,g")],
            vec![("contentRating!", "")],
            vec![("actor", "1")],
            vec![("actor", "ann actor")],
            vec![("actor!", "1")],
            vec![("actor", "7")],
            vec![("actor", "1,dan director")],
            vec![("director", "2")],
            vec![("director!", "2")],
            vec![("writer", "Ann Actor")],
            vec![("producer", "2")],
            vec![("actor", "")],
            vec![("unwatched", "1")],
            vec![("unwatched", "0")],
            vec![("unwatched!", "1")],
            vec![("unwatched", "")],
            vec![("watched", "1")],
            vec![("watched", "false")],
            vec![("unwatchedLeaves", "1")],
            vec![("inProgress", "1")],
            vec![("inProgress", "0")],
            vec![("inProgress!", "1")],
            vec![("addedAt>>", "1500")],
            vec![("addedAt<<", "1500")],
            vec![("addedAt", "1000")],
            vec![("addedAt!", "1000")],
            vec![("addedAt>>", "-9999999999999y")],
            vec![("addedAt<<", "-9999999999999y")],
            vec![("addedAt", "-9999999999999y")],
            vec![("addedAt>>", "soon")],
            vec![("addedAt", "1970-01-02")],
            vec![("addedAt>>", "-2d")],
            vec![("addedAt<<", "-2d")],
            vec![("updatedAt>>", "2000")],
            vec![("lastViewedAt>>", "5500")],
            vec![("lastViewedAt", "soon")],
            vec![("lastViewedAt<<", "-1d")],
            vec![("lastViewedAt!", "6000")],
            vec![("guid", "imdb://tt0001")],
            vec![("guid", "tmdb://348")],
            vec![("guid", "tmdb://0348")],
            vec![("guid", guid.as_str())],
            vec![("guid", show_guid.as_str())],
            vec![("guid!", "imdb://tt0003,tmdb://3")],
            vec![("guid", "imdb://")],
            vec![("guid", "tvdb://73739")],
            vec![("label", "overlay")],
            vec![("label", "OVERLAY,4k")],
            vec![("label!", "overlay")],
            vec![("label", "")],
            vec![("resolution", "4k")],
            vec![("show.title", "x")],
            vec![("genre", "horror"), ("unwatched", "1")],
            vec![("year>>", "2000"), ("rating>>", "7"), ("title!", "zeta")],
            vec![("actor!", "1"), ("contentRating", "PG"), ("lastViewedAt>>", "0")],
        ];
        // Only the in-memory path can check these.
        let memory_only: Vec<Vec<(&str, &str)>> = vec![
            vec![("title", "é")],
            vec![("title", "İ")],
            vec![("studio", "Étude")],
            vec![("title", "alpha,ÉCOLE")],
            vec![("originallyAvailableAt>>", "2000-01-01")],
            vec![("originallyAvailableAt", "2005-03-03")],
        ];
        movie_filters.extend(memory_only.iter().cloned());
        let show_filters: Vec<Vec<(&str, &str)>> = vec![
            vec![],
            vec![("unwatched", "1")],
            vec![("unwatched", "0")],
            vec![("watched", "1")],
            vec![("show.unwatched", "0")],
            vec![("unwatched!", "1")],
            vec![("inProgress", "1")],
            vec![("leafCount>>", "1")],
            vec![("leafCount", "0")],
            vec![("title", "e")],
            vec![("genre", "drama")],
            vec![("actor", "Ann Actor")],
            vec![("lastViewedAt>>", "1")],
            vec![("guid", "tvdb://73739")],
            vec![("guid", "tvdb://073739")],
            vec![("guid", "tvdb://73739,tvdb://1")],
            vec![("label", "kids")],
        ];
        let season_filters: Vec<Vec<(&str, &str)>> = vec![
            vec![],
            vec![("unwatched", "1")],
            vec![("watched", "1")],
            vec![("show.id", "100")],
            vec![("show.title", "fringe")],
            vec![("show.index", "1")],
            vec![("show.year", "2004")],
            vec![("index", "1")],
            vec![("season.title", "2")],
        ];
        let episode_filters: Vec<Vec<(&str, &str)>> = vec![
            vec![],
            vec![("show.id", "100")],
            vec![("show.id!", "100")],
            vec![("season.id", "101")],
            vec![("season.index", "2")],
            vec![("show.title", "LOST")],
            vec![("show.title<", "fr")],
            vec![("season.title", "season")],
            vec![("show.year", "2004")],
            vec![("unwatched", "1")],
            vec![("inProgress", "1")],
            vec![("title", "pilot")],
            vec![("title", "=pilot")],
            vec![("index", "1")],
            vec![("viewCount>>", "0")],
            vec![("addedAt", "5001")],
        ];
        let mut compared = 0;
        for user in [1, 2] {
            for (library, kind, filters) in [
                (1, "movie", &movie_filters),
                (2, "show", &show_filters),
                (2, "season", &season_filters),
                (2, "episode", &episode_filters),
            ] {
                for (n, filter) in filters.iter().enumerate() {
                    // Every filter with the default order, a few with every order.
                    let sorts = if n < 3 || filter.len() > 1 { SORTS } else { &SORTS[..1] };
                    for &sort in sorts {
                        let mut pairs = filter.clone();
                        pairs.push(("sort", sort));
                        let query = section_query(library, kind, &pairs);
                        let expected = in_memory(&db, user, &query, &Page::default()).await;
                        match in_sql(&db, user, &query, &Page::default()).await {
                            Some(found) => assert_eq!(found, expected, "user {user}, {kind}s, {pairs:?}"),
                            None => assert!(memory_only.contains(filter), "{pairs:?} should be checked in SQL"),
                        }
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 500, "{compared}");
    }

    #[tokio::test]
    async fn pages_and_totals_in_sql_match_the_in_memory_path() {
        let dir = tempfile::tempdir().unwrap();
        let db = listing_fixture(dir.path()).await;
        let pages = [(0, None), (0, Some(0)), (0, Some(3)), (2, Some(3)), (3, Some(100)), (11, Some(5)), (50, Some(5))];
        for user in [1, 2] {
            for pairs in [
                vec![],
                vec![("sort", "year:desc")],
                vec![("unwatched", "1"), ("sort", "rating")],
                vec![("limit", "4")],
                vec![("limit", "4"), ("sort", "addedAt:desc")],
                vec![("limit", "0")],
                vec![("limit", "100"), ("genre", "drama")],
                vec![("genre", "nothing")],
            ] {
                for (start, size) in pages {
                    let page = Page { start, size };
                    let query = section_query(1, "movie", &pairs);
                    let found = in_sql(&db, user, &query, &page).await.unwrap();
                    let expected = in_memory(&db, user, &query, &page).await;
                    assert_eq!(found, expected, "user {user}, {pairs:?}, {page:?}");
                }
            }
        }
    }
}
