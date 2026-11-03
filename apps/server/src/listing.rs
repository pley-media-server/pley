//! Listings of a library, filtered and sorted by the query string smart
//! collections and smart playlists keep ([API.md](../../../docs/API.md#collections)):
//! `type=1&genre=Comedy&year>>=2000&sort=year:desc`. Fields, operators and
//! sorts are those of the query strings media servers have long used, so
//! filters written for one read the same here.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::catalog::{self, ItemRow, ItemsQuery, LibraryQuery, SqlArg};
use crate::db::Db;
use crate::library::{LibraryKind, LibraryRow};
use crate::people::{self, Credit};
use crate::util::{date_secs, now};

/// The name the query string gives items of `kind` (`show.title=…`); the
/// same as Pley's, but for bonus material.
pub fn type_name(kind: &str) -> &'static str {
    match kind {
        "movie" => "movie",
        "show" => "show",
        "season" => "season",
        "episode" => "episode",
        "artist" => "artist",
        "album" => "album",
        "extra" => "clip",
        _ => "track",
    }
}

/// The number `type=` gives items of `kind`.
pub fn type_number(kind: &str) -> i64 {
    match kind {
        "movie" => 1,
        "show" => 2,
        "season" => 3,
        "episode" => 4,
        "artist" => 8,
        "album" => 9,
        "track" => 10,
        _ => 0,
    }
}

/// The kind `type=` names.
pub fn kind_for_number(number: &str) -> Option<&'static str> {
    Some(match number.trim() {
        "1" => "movie",
        "2" => "show",
        "3" => "season",
        "4" => "episode",
        "8" => "artist",
        "9" => "album",
        "10" => "track",
        _ => return None,
    })
}

/// Item kinds that live in a library of this kind.
pub fn kinds_of(lib: &LibraryRow) -> &'static [&'static str] {
    match lib.kind() {
        LibraryKind::Movies => &["movie"],
        LibraryKind::Shows => &["show", "season", "episode"],
        LibraryKind::Music => &["artist", "album", "track"],
    }
}

/// The guid the Plex-compatible API gives an item, which the `guid` filter
/// takes too.
pub fn own_guid(kind: &str, id: i64) -> String {
    format!("plex://{}/{id:024x}", type_name(kind))
}

/// Plays of a watched item, 0 once it is marked unplayed: Pley keeps the
/// lifetime count, and `viewCount` counts what is watched now.
pub fn view_count(row: &ItemRow) -> i64 {
    if row.watched { row.play_count.max(1) } else { 0 }
}

/// What filters look at that [`ItemRow`] doesn't carry.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    pub genres: Vec<String>,
    pub studio: Option<String>,
    pub content_rating: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    /// Shows: the TheTVDB series (`tvdb://` guid).
    pub tvdb_id: Option<i64>,
    /// Artists and albums: the MusicBrainz artist or release group (`mbid://`).
    pub musicbrainz_id: Option<String>,
    /// Cast and crew, loaded only for people filters.
    pub people: Vec<Credit>,
    /// Labels (see `crate::labels`).
    pub labels: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct ExtrasRow {
    id: i64,
    genres: Option<String>,
    studio: Option<String>,
    content_rating: Option<String>,
    tmdb_id: Option<i64>,
    imdb_id: Option<String>,
    tvdb_id: Option<i64>,
    musicbrainz_id: Option<String>,
}

/// The extras of items the caller loaded through the catalog (so access
/// rules were applied there).
async fn load_extras(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Extras>> {
    let mut out = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT id, genres, studio, content_rating, tmdb_id, imdb_id, tvdb_id, musicbrainz_id
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
                    genres: row.genres.and_then(|g| serde_json::from_str(&g).ok()).unwrap_or_default(),
                    studio: row.studio,
                    content_rating: row.content_rating,
                    tmdb_id: row.tmdb_id,
                    imdb_id: row.imdb_id,
                    tvdb_id: row.tvdb_id,
                    musicbrainz_id: row.musicbrainz_id,
                    people: Vec::new(),
                    labels: Vec::new(),
                },
            );
        }
    }
    for (id, labels) in crate::labels::for_items(db, ids).await? {
        out.entry(id).or_default().labels = labels;
    }
    Ok(out)
}

/// Cast and crew of `ids` into their extras (seasons get their show's).
async fn add_people(db: &Db, ids: &[i64], extras: &mut HashMap<i64, Extras>) -> sqlx::Result<()> {
    for (id, credits) in people::for_items(db, ids).await? {
        extras.entry(id).or_default().people = credits;
    }
    Ok(())
}

/// Query parameters in order, repeated keys kept.
#[derive(Debug, Clone, Default)]
pub struct Params(pub Vec<(String, String)>);

impl Params {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn parse<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }

    pub fn flag(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| matches!(v, "1" | "true" | "yes"))
    }
}

/// A page of a listing: `size` items from `start` (all of them without one).
#[derive(Debug, Clone, Copy, Default)]
pub struct Page {
    pub start: usize,
    pub size: Option<usize>,
}

impl Page {
    /// The requested slice of `items` and the total count.
    pub fn apply<T>(&self, items: Vec<T>) -> (Vec<T>, usize) {
        let total = items.len();
        let items = items.into_iter().skip(self.start).take(self.size.unwrap_or(usize::MAX)).collect();
        (items, total)
    }
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
    /// `field<=v`: begins with (strings), at most (numbers, dates).
    Begins,
    /// `field>=v`: ends with (strings), at least (numbers, dates).
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
    let own = type_name(kind);
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
pub(crate) fn people_role(field: &str) -> Option<people::Role> {
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

/// Fields of the query string Pley doesn't keep the data for or doesn't read
/// yet. A filter on one is refused rather than ignored: a tool changing what
/// the filter finds would otherwise change the whole library.
const UNREAD_FIELDS: &[&str] = &[
    "collection",
    "country",
    "network",
    "resolution",
    "videoResolution",
    "hdr",
    "dovi",
    "hdr10plus",
    "atmos",
    "audioLanguage",
    "subtitleLanguage",
    "audioTrackTitle",
    "subtitleTrackTitle",
    "audioCodec",
    "videoCodec",
    "audioChannels",
    "videoFrameRate",
    "mediaSize",
    "mediaBitrate",
    "editionTitle",
    "originalTitle",
    "unmatched",
    "duplicate",
    "trash",
    "location",
    "contentRatingAge",
    "mood",
    "style",
    "format",
    "subformat",
    "source",
    "skipCount",
    "lastSkippedAt",
    "lastRatedAt",
    "place",
    "tag",
];

/// What a field holds, which decides the operators it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Text,
    Number,
    Tag,
    Boolean,
    Date,
    Guid,
}

impl FieldKind {
    /// The fields [`Filter::matches`] reads, at their level.
    fn of(level: Level, field: &str) -> Option<FieldKind> {
        Some(match (level, field) {
            (Level::Parent, "id" | "index") | (Level::Grandparent, "id") => FieldKind::Number,
            (Level::Parent | Level::Grandparent, "title") => FieldKind::Text,
            (Level::Parent | Level::Grandparent, _) => return None,
            (Level::Own, "title" | "studio") => FieldKind::Text,
            (
                Level::Own,
                "year" | "decade" | "index" | "id" | "duration" | "viewCount" | "viewOffset" | "rating"
                | "audienceRating" | "userRating" | "leafCount",
            ) => FieldKind::Number,
            (Level::Own, "genre" | "label" | "contentRating") => FieldKind::Tag,
            (Level::Own, field) if people_role(field).is_some() => FieldKind::Tag,
            (Level::Own, "unwatched" | "unwatchedLeaves" | "watched" | "inProgress") => FieldKind::Boolean,
            (Level::Own, "addedAt" | "updatedAt" | "lastViewedAt" | "originallyAvailableAt") => FieldKind::Date,
            (Level::Own, "guid") => FieldKind::Guid,
            (Level::Own, _) => return None,
        })
    }

    fn takes(self, op: Op) -> bool {
        match self {
            FieldKind::Text => !matches!(op, Op::Greater | Op::Less),
            FieldKind::Number | FieldKind::Date => true,
            FieldKind::Tag | FieldKind::Boolean | FieldKind::Guid => {
                matches!(op, Op::Is | Op::Not | Op::Exactly | Op::NotExactly)
            }
        }
    }
}

/// A filter Pley can't apply: the listing is refused, as a tool acting on
/// what the filter finds would otherwise act on everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FilterError(pub String);

/// How deep `push=1` groups nest at most.
const MAX_GROUPS: usize = 8;
/// Filters a listing takes, and values a filter names, at most: more than
/// any tool sends, and a bound on the work one request makes.
const MAX_FILTERS: usize = 64;
const MAX_VALUES: usize = 100;

/// Filters joined as the query string joins them: `and=1` (or nothing) and
/// `or=1` between terms, `push=1` … `pop=1` around a group. `and` binds
/// tighter than `or`.
#[derive(Debug, Clone)]
enum Expr {
    Filter(Filter),
    All(Vec<Expr>),
    Any(Vec<Expr>),
}

impl Expr {
    fn parse(params: &Params, kind: &str) -> Result<Expr, FilterError> {
        // The groups open, innermost last: each a list of runs joined by
        // `or`, each run a list of terms joined by `and`. A group always
        // has a run.
        let mut groups: Vec<Vec<Vec<Expr>>> = vec![vec![Vec::new()]];
        let mut filters = 0;
        let add = |groups: &mut Vec<Vec<Vec<Expr>>>, term: Expr| {
            if let Some(run) = groups.last_mut().and_then(|runs| runs.last_mut()) {
                run.push(term);
            }
        };
        for (key, value) in &params.0 {
            match key.as_str() {
                "push" => {
                    if groups.len() > MAX_GROUPS {
                        return Err(FilterError("filters nest too deep".into()));
                    }
                    groups.push(vec![Vec::new()]);
                }
                "pop" => {
                    if groups.len() > 1
                        && let Some(group) = groups.pop()
                    {
                        add(&mut groups, Expr::of_runs(group));
                    }
                }
                "or" => {
                    if let Some(runs) = groups.last_mut()
                        && runs.last().is_some_and(|run| !run.is_empty())
                    {
                        runs.push(Vec::new());
                    }
                }
                "and" => {}
                _ => {
                    if let Some(filter) = Filter::parse(key, value, kind)? {
                        filters += 1;
                        if filters > MAX_FILTERS {
                            return Err(FilterError(format!("{MAX_FILTERS} filters at most")));
                        }
                        add(&mut groups, Expr::Filter(filter));
                    }
                }
            }
        }
        // Groups left open end with the query.
        while groups.len() > 1 {
            if let Some(group) = groups.pop() {
                add(&mut groups, Expr::of_runs(group));
            }
        }
        Ok(Expr::of_runs(groups.pop().unwrap_or_default()))
    }

    /// A group: its one term, all of its run, or any of its runs.
    fn of_runs(runs: Vec<Vec<Expr>>) -> Expr {
        let mut any: Vec<Expr> = runs
            .into_iter()
            .filter(|run| !run.is_empty())
            .map(|mut run| if run.len() == 1 { run.remove(0) } else { Expr::All(run) })
            .collect();
        match any.len() {
            0 => Expr::All(Vec::new()),
            1 => any.remove(0),
            _ => Expr::Any(any),
        }
    }

    fn is_empty(&self) -> bool {
        matches!(self, Expr::All(terms) if terms.is_empty())
    }

    fn any_filter(&self, test: &dyn Fn(&Filter) -> bool) -> bool {
        match self {
            Expr::Filter(filter) => test(filter),
            Expr::All(terms) | Expr::Any(terms) => terms.iter().any(|t| t.any_filter(test)),
        }
    }

    fn matches(&self, row: &ItemRow, extras: Option<&Extras>, now: i64) -> bool {
        match self {
            Expr::Filter(filter) => filter.matches(row, extras, now),
            Expr::All(terms) => terms.iter().all(|t| t.matches(row, extras, now)),
            Expr::Any(terms) => terms.iter().any(|t| t.matches(row, extras, now)),
        }
    }

    /// [`Expr::matches`] as SQL, when every filter in it can be.
    fn condition(&self, kind: &str, now: i64) -> Condition {
        let (terms, any) = match self {
            Expr::Filter(filter) => return filter.condition(kind, now),
            Expr::All(terms) => (terms, false),
            Expr::Any(terms) => (terms, true),
        };
        let mut tests = Vec::new();
        let mut args = Vec::new();
        let mut state = false;
        for term in terms {
            match term.condition(kind, now) {
                Condition::InMemory => return Condition::InMemory,
                // Any of a group that holds everything holds everything.
                Condition::All if any => return Condition::All,
                Condition::All => {}
                Condition::Sql { sql, args: more, state: reads } => {
                    tests.push(format!("({sql})"));
                    args.extend(more);
                    state |= reads;
                }
            }
        }
        if tests.is_empty() {
            return Condition::All;
        }
        Condition::Sql { sql: tests.join(if any { " OR " } else { " AND " }), args, state }
    }
}

impl Filter {
    /// The filter of one query parameter, `None` for a parameter that isn't
    /// one (the client's own, or a field this isn't a filter of).
    fn parse(raw_key: &str, raw_value: &str, kind: &str) -> Result<Option<Filter>, FilterError> {
        if raw_key.len() > 7 && raw_key.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("x-plex-")) {
            return Ok(None);
        }
        if NOT_FILTERS.contains(&raw_key) {
            return Ok(None);
        }
        let refused = || FilterError(format!("Pley can't filter by {raw_key}"));
        // The operator's `=` may come in the key, encoded: `title%3D=x` is
        // `title==x`, as Kometa writes it.
        let (mut key, equals_in_key) = match raw_key.strip_suffix('=') {
            Some(key) => (key, true),
            None => (raw_key, false),
        };
        let mut level = Level::Own;
        let mut prefixed = false;
        if let Some((prefix, rest)) = key.split_once('.') {
            // Descendants (`episode.title` on shows) aren't read.
            level = level_of(prefix, kind).ok_or_else(refused)?;
            key = rest;
            prefixed = true;
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
        let (op, value) = match (op, equals_in_key, raw_value.strip_prefix('=')) {
            (Op::Is, true, _) => (Op::Exactly, raw_value),
            (Op::Not, true, _) => (Op::NotExactly, raw_value),
            (Op::Is, false, Some(rest)) => (Op::Exactly, rest),
            (Op::Not, false, Some(rest)) => (Op::NotExactly, rest),
            (op, _, _) => (op, raw_value),
        };
        match FieldKind::of(level, field) {
            Some(field_kind) if field_kind.takes(op) => {}
            Some(_) => return Err(refused()),
            // With an operator or a level it is a filter all the same.
            None if prefixed || op != Op::Is || UNREAD_FIELDS.contains(&field) => return Err(refused()),
            None => return Ok(None),
        }
        let values: Vec<String> = value.split(',').map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).collect();
        if values.len() > MAX_VALUES {
            return Err(FilterError(format!("{raw_key} names {MAX_VALUES} values at most")));
        }
        Ok(Some(Filter { level, field: field.to_owned(), op, values }))
    }

    fn needs_extras(&self) -> bool {
        matches!(self.field.as_str(), "genre" | "label" | "contentRating" | "studio" | "guid") || self.needs_people()
    }

    fn needs_people(&self) -> bool {
        people_role(&self.field).is_some()
    }

    /// Fields it doesn't read match nothing ([`Filter::parse`] refuses them).
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
            return result.unwrap_or(false);
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
            "viewCount" => Some(self.number(Some(view_count(row) as f64))),
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
                let own = own_guid(&row.kind, row.id);
                let imdb = extras.and_then(|e| e.imdb_id.clone()).map(|i| format!("imdb://{i}"));
                let tmdb = extras.and_then(|e| e.tmdb_id).map(|t| format!("tmdb://{t}"));
                // Only where the details list them.
                let tvdb = extras.and_then(|e| e.tvdb_id).filter(|_| row.kind == "show").map(|t| format!("tvdb://{t}"));
                let mbid = extras
                    .and_then(|e| e.musicbrainz_id.clone())
                    .filter(|_| matches!(row.kind.as_str(), "artist" | "album"))
                    .map(|m| format!("mbid://{m}"));
                let known = [imdb, tmdb, tvdb, mbid];
                let any = self.values.iter().any(|v| *v == own || known.iter().any(|g| g.as_ref() == Some(v)));
                Some(if matches!(self.op, Op::Not | Op::NotExactly) { !any } else { any })
            }
            _ => None,
        };
        result.unwrap_or(false)
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
            Op::Greater | Op::Less => false,
        }
    }

    /// A missing value is -1 to `=` and `!=` (`rating!=-1` is "rated"), and
    /// passes no comparison.
    fn number(&self, value: Option<f64>) -> bool {
        let targets: Vec<f64> = self.values.iter().filter_map(|v| v.parse().ok()).collect();
        let equal = |value: f64| targets.iter().any(|t| (value - t).abs() < 1e-9);
        let first = targets.first().copied();
        match (self.op, value) {
            (Op::Is | Op::Exactly, value) => equal(value.unwrap_or(-1.0)),
            (Op::Not | Op::NotExactly, value) => !equal(value.unwrap_or(-1.0)),
            (_, None) => false,
            (Op::Greater, Some(value)) => first.is_some_and(|t| value > t),
            (Op::Less, Some(value)) => first.is_some_and(|t| value < t),
            (Op::Ends, Some(value)) => first.is_some_and(|t| value >= t),
            (Op::Begins, Some(value)) => first.is_some_and(|t| value <= t),
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
            Op::Ends => value >= target,
            Op::Begins => value <= target,
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
pub(crate) enum SortField {
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
pub(crate) async fn bitrates_for(
    db: &Db,
    rows: &[ItemRow],
    spec: &[(SortField, bool)],
) -> sqlx::Result<HashMap<i64, f64>> {
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
pub(crate) fn parse_sort(value: Option<&str>) -> Vec<(SortField, bool)> {
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
pub(crate) fn sort_rows(rows: &mut [ItemRow], spec: &[(SortField, bool)], bitrates: &HashMap<i64, f64>) {
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
                SortField::ViewCount => cmp_opt(Some(view_count(a)), Some(view_count(b)), desc),
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

/// Plex's `viewCount` of an item (see [`view_count`]).
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
    /// A condition and its arguments; `state`: it reads the user's state `s`.
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
        Self::sql("0".into(), Vec::new())
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
            (Level::Parent | Level::Grandparent, _) => Condition::none(),
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
            (Level::Own, _) => Condition::none(),
        }
    }

    /// Whether any of `tests` holds, or none of them for `!=` and `!==`.
    fn any_sql(&self, tests: Vec<String>, args: Vec<SqlArg>) -> Condition {
        let negated = matches!(self.op, Op::Not | Op::NotExactly);
        match (tests.is_empty(), negated) {
            (true, false) => Condition::none(),
            (true, true) => Condition::All,
            (false, false) => Condition::sql(tests.join(" OR "), args),
            (false, true) => Condition::sql(format!("NOT ({})", tests.join(" OR ")), args),
        }
    }

    /// [`Filter::text`]: in SQL when every value is ASCII once lowercased.
    fn text_sql(&self, column: &str) -> Condition {
        if matches!(self.op, Op::Greater | Op::Less) {
            return Condition::none();
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
        let op = match self.op {
            Op::Is | Op::Exactly | Op::Not | Op::NotExactly => {
                let mut tests = Vec::new();
                let mut args = Vec::new();
                // NaN equals nothing (and SQLite would take it for NULL).
                for target in targets.into_iter().filter(|t| !t.is_nan()) {
                    tests.push(format!("abs(COALESCE({value}, -1.0) - ?) < ?"));
                    args.extend([SqlArg::Real(target), SqlArg::Real(1e-9)]);
                }
                let any = if tests.is_empty() { "0".to_owned() } else { tests.join(" OR ") };
                let negated = matches!(self.op, Op::Not | Op::NotExactly);
                return Condition::sql(if negated { format!("NOT ({any})") } else { any }, args);
            }
            Op::Greater => ">",
            Op::Less => "<",
            Op::Ends => ">=",
            Op::Begins => "<=",
        };
        // A missing value (NULL) passes no comparison.
        match targets.first() {
            Some(target) if !target.is_nan() => Condition::sql(format!("{value} {op} ?"), vec![SqlArg::Real(*target)]),
            _ => Condition::none(),
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
        Condition::sql(if wanted != negated { expr.to_owned() } else { format!("NOT {expr}") }, Vec::new())
    }

    /// [`Filter::date`] of a unix time.
    fn date_sql(&self, expr: &str, now: i64) -> Condition {
        let Some(target) = self.values.first().and_then(|v| parse_date(v, now)) else {
            // An unreadable date keeps whatever has a date.
            return Condition::sql(format!("{expr} IS NOT NULL"), Vec::new());
        };
        // The same day: less than a day apart.
        let day = [SqlArg::Int(target.saturating_sub(DAY - 1)), SqlArg::Int(target.saturating_add(DAY - 1))];
        let compared = |op: &str| Condition::sql(format!("{expr} {op} ?"), vec![SqlArg::Int(target)]);
        match self.op {
            Op::Greater => compared(">"),
            Op::Less => compared("<"),
            Op::Ends => compared(">="),
            Op::Begins => compared("<="),
            Op::Not | Op::NotExactly => Condition::sql(format!("{expr} NOT BETWEEN ? AND ?"), day.into()),
            _ => Condition::sql(format!("{expr} BETWEEN ? AND ?"), day.into()),
        }
    }

    /// `plex://`, `imdb://`, `tmdb://`, `tvdb://` (shows) and `mbid://`
    /// (artists, albums) guids, like [`Filter::matches`].
    /// Compared with `IS`, so that a missing id is a different one, not an
    /// unknown that `!=` would drop.
    fn guid_sql(&self, kind: &str) -> Condition {
        let mut tests = Vec::new();
        let mut args = Vec::new();
        for value in &self.values {
            let own = value
                .rsplit_once('/')
                .and_then(|(_, hex)| u64::from_str_radix(hex, 16).ok())
                .map(|id| id as i64)
                .filter(|id| own_guid(kind, *id) == *value);
            if let Some(id) = own {
                tests.push("i.id = ?".to_owned());
                args.push(SqlArg::Int(id));
            }
            if let Some(imdb) = value.strip_prefix("imdb://") {
                tests.push("i.imdb_id IS ?".to_owned());
                args.push(SqlArg::Text(imdb.to_owned()));
            }
            let tmdb = value.strip_prefix("tmdb://").and_then(|t| t.parse::<i64>().ok());
            if let Some(tmdb) = tmdb.filter(|t| format!("tmdb://{t}") == *value) {
                tests.push("i.tmdb_id IS ?".to_owned());
                args.push(SqlArg::Int(tmdb));
            }
            let tvdb = value.strip_prefix("tvdb://").and_then(|t| t.parse::<i64>().ok());
            if let Some(tvdb) = tvdb.filter(|t| kind == "show" && format!("tvdb://{t}") == *value) {
                tests.push("i.tvdb_id IS ?".to_owned());
                args.push(SqlArg::Int(tvdb));
            }
            if let Some(mbid) = value.strip_prefix("mbid://").filter(|_| matches!(kind, "artist" | "album")) {
                tests.push("i.musicbrainz_id IS ?".to_owned());
                args.push(SqlArg::Text(mbid.to_owned()));
            }
        }
        self.any_sql(tests, args)
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
pub(crate) struct SectionQuery<'a> {
    library_id: i64,
    kind: &'a str,
    filter: Expr,
    sort: Vec<(SortField, bool)>,
    /// Plex's `limit`: how many items there are at most.
    cap: Option<usize>,
    now: i64,
}

impl<'a> SectionQuery<'a> {
    pub(crate) fn new(library_id: i64, kind: &'a str, params: &Params) -> Result<Self, FilterError> {
        Ok(SectionQuery {
            library_id,
            kind,
            filter: Expr::parse(params, kind)?,
            sort: parse_sort(params.get("sort")),
            cap: params.parse("limit"),
            now: now(),
        })
    }

    /// The `page` of the items and how many there are: filtered, sorted and
    /// paged in SQL unless a filter can only be checked in memory.
    pub(crate) async fn page(&self, db: &Db, user_id: i64, page: &Page) -> sqlx::Result<(Vec<ItemRow>, usize)> {
        match self.sql_filters() {
            Some(filters) => self.page_in_sql(db, user_id, &filters, page).await,
            None => self.page_in_memory(db, user_id, page).await,
        }
    }

    fn sql_filters(&self) -> Option<SqlFilters> {
        match self.filter.condition(self.kind, self.now) {
            Condition::All => Some(SqlFilters { filter: String::new(), args: Vec::new(), uses_state: false }),
            Condition::Sql { sql, args, state } => {
                Some(SqlFilters { filter: format!(" AND ({sql})"), args, uses_state: state })
            }
            Condition::InMemory => None,
        }
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
        if self.filter.any_filter(&Filter::needs_extras) {
            let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
            extras = load_extras(db, &ids).await?;
            if self.filter.any_filter(&Filter::needs_people) {
                add_people(db, &ids, &mut extras).await?;
            }
        }
        if !self.filter.is_empty() {
            rows.retain(|r| self.filter.matches(r, extras.get(&r.id), self.now));
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

/// Whether `filter` is a section listing query a smart collection can use:
/// of a kind `lib` has, with filters Pley can apply.
pub(crate) fn valid_smart_filter(lib: &LibraryRow, filter: &str) -> bool {
    let params = filter_params(filter);
    !params.0.is_empty() && smart_kind(lib, &params).is_some_and(|kind| Expr::parse(&params, kind).is_ok())
}

/// What a smart collection's filter finds in `lib`, as `user_id` may see it,
/// in its order. A filter Pley can't apply finds nothing.
pub(crate) async fn smart_items(db: &Db, user_id: i64, lib: &LibraryRow, filter: &str) -> sqlx::Result<Vec<ItemRow>> {
    let params = filter_params(filter);
    let Some(kind) = smart_kind(lib, &params) else { return Ok(Vec::new()) };
    let Ok(query) = SectionQuery::new(lib.id, kind, &params) else { return Ok(Vec::new()) };
    Ok(query.page(db, user_id, &Page::default()).await?.0)
}

/// How many items a smart collection's filter finds.
pub(crate) async fn smart_count(db: &Db, user_id: i64, lib: &LibraryRow, filter: &str) -> sqlx::Result<usize> {
    let params = filter_params(filter);
    let Some(kind) = smart_kind(lib, &params) else { return Ok(0) };
    let Ok(query) = SectionQuery::new(lib.id, kind, &params) else { return Ok(0) };
    Ok(query.page(db, user_id, &Page { start: 0, size: Some(0) }).await?.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Params {
        Params(pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect())
    }

    fn parsed(pairs: &[(&str, &str)], kind: &str) -> Result<Expr, FilterError> {
        Expr::parse(&params(pairs), kind)
    }

    /// The filters of a query joined by `and` alone.
    fn filters(pairs: &[(&str, &str)], kind: &str) -> Vec<Filter> {
        match parsed(pairs, kind).unwrap() {
            Expr::Filter(filter) => vec![filter],
            Expr::All(terms) => terms
                .into_iter()
                .map(|t| match t {
                    Expr::Filter(filter) => filter,
                    other => panic!("{other:?}"),
                })
                .collect(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_plex_filters() {
        let found = filters(
            &[
                ("type", "1"),
                ("title", "=Exact"),
                ("year>>", "2000"),
                ("genre!", "Horror,Comedy"),
                ("X-Plex-Token", "secret"),
                ("includeGuids", "1"),
            ],
            "movie",
        );
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!((found[0].field.as_str(), found[0].op), ("title", Op::Exactly));
        assert_eq!(found[0].values, vec!["Exact"]);
        assert_eq!((found[1].field.as_str(), found[1].op), ("year", Op::Greater));
        assert_eq!((found[2].op, found[2].values.len()), (Op::Not, 2));

        let found = filters(&[("show.unwatched", "1")], "show");
        assert_eq!((found[0].field.as_str(), found[0].op), ("unwatched", Op::Is));

        // Ancestor filters (python-plexapi's `artist.albums()`).
        let found = filters(&[("artist.id", "16")], "album");
        assert_eq!((found[0].level, found[0].field.as_str()), (Level::Parent, "id"));
        assert_eq!(filters(&[("show.id", "3")], "episode")[0].level, Level::Grandparent);

        // Kometa writes the operator's `=` into the key, encoded.
        let found = filters(&[("title=", "Exact"), ("title!=", "Not"), ("year>", "2000"), ("year<", "2010")], "movie");
        let ops: Vec<Op> = found.iter().map(|f| f.op).collect();
        assert_eq!(ops, [Op::Exactly, Op::NotExactly, Op::Ends, Op::Begins]);
        assert_eq!(found[0].values, vec!["Exact"]);
    }

    #[test]
    fn groups_join_filters_with_or_and_and() {
        let ids = |pairs: &[(&str, &str)]| match parsed(pairs, "movie").unwrap() {
            Expr::Any(runs) => runs.len(),
            Expr::All(terms) => 100 + terms.len(),
            Expr::Filter(_) => 1000,
        };
        // Kometa's `any:` of two genres, and a year with it.
        let query = [
            ("push", "1"),
            ("genre", "Horror"),
            ("or", "1"),
            ("genre", "Comedy"),
            ("pop", "1"),
            ("and", "1"),
            ("year>>", "1990"),
        ];
        assert_eq!(ids(&query), 102, "all of the group and the year");
        assert_eq!(ids(&[("genre", "a"), ("or", "1"), ("genre", "b"), ("year", "1")]), 2, "`and` binds tighter");
        assert_eq!(ids(&[("or", "1"), ("genre", "a")]), 1000, "a leading `or` joins nothing");
        assert_eq!(ids(&[("push", "1"), ("pop", "1"), ("pop", "1")]), 100, "empty, and a `pop` too many");
        assert_eq!(ids(&[("push", "1"), ("genre", "a"), ("or", "1"), ("genre", "b")]), 2, "open groups close");
        let deep: Vec<(&str, &str)> = std::iter::repeat_n(("push", "1"), MAX_GROUPS + 1).collect();
        assert!(parsed(&deep, "movie").is_err());
        let deep: Vec<(&str, &str)> = std::iter::repeat_n(("push", "1"), MAX_GROUPS).collect();
        assert!(parsed(&deep, "movie").is_ok());

        // Bounded, however long the query string.
        let many: Vec<(&str, &str)> = std::iter::repeat_n(("genre", "a"), MAX_FILTERS).collect();
        assert!(parsed(&many, "movie").is_ok());
        let many: Vec<(&str, &str)> = std::iter::repeat_n(("genre", "a"), MAX_FILTERS + 1).collect();
        assert!(parsed(&many, "movie").is_err());
        let values = vec!["a"; MAX_VALUES + 1].join(",");
        assert!(parsed(&[("title", &values)], "movie").is_err());
    }

    #[test]
    fn refuses_filters_pley_cant_apply() {
        for (pairs, kind) in [
            (&[("resolution", "4k")][..], "movie"),
            (&[("hdr", "1")], "movie"),
            (&[("editionTitle", "Director's Cut")], "movie"),
            (&[("collection", "5")], "movie"),
            (&[("episode.title", "x")], "show"),
            (&[("show.year", "2004")], "episode"),
            (&[("show.title", "x")], "movie"),
            (&[("title>>", "a")], "movie"),
            (&[("genre>>", "x")], "movie"),
            (&[("genre<", "x")], "movie"),
            (&[("unwatched>>", "1")], "movie"),
            (&[("audioLanguage", "en")], "movie"),
            (&[("someField!", "1")], "movie"),
            (&[("someField=", "1")], "movie"),
            (&[("genre", "a"), ("or", "1"), ("hdr", "1")], "movie"),
        ] {
            assert!(parsed(pairs, kind).is_err(), "{pairs:?} on {kind}s");
        }
        assert_eq!(parsed(&[("hdr", "1")], "movie").unwrap_err().0, "Pley can't filter by hdr");
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
    }

    fn row(id: i64, kind: &str, title: &str) -> ItemRow {
        ItemRow::test_row(id, kind, title)
    }

    /// Titles of `rows` that pass the filters in `pairs` (query keys as
    /// Plex clients send them: `year>>=2000` arrives as `("year>>", "2000")`).
    fn matching(
        rows: &[ItemRow],
        kind: &str,
        pairs: &[(&str, &str)],
        extras: &HashMap<i64, Extras>,
        now: i64,
    ) -> Vec<String> {
        let filter = parsed(pairs, kind).unwrap();
        rows.iter().filter(|r| filter.matches(r, extras.get(&r.id), now)).map(|r| r.title.clone()).collect()
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
        // Kometa's `.is` and `.isnot`.
        assert_eq!(titles(&[("title=", "alpha")]), ["Alpha"]);
        assert_eq!(titles(&[("title!=", "alpha")]), ["Alphabet City", "Bravo", "The Charlie Movie"]);
        // Kometa's `any:`.
        let either = [("push", "1"), ("title", "bravo"), ("or", "1"), ("title>", "movie"), ("pop", "1")];
        assert_eq!(titles(&either), ["Bravo", "The Charlie Movie"]);
        let both = [
            ("title", "alpha"),
            ("or", "1"),
            ("push", "1"),
            ("title", "c"),
            ("and", "1"),
            ("title<", "the"),
            ("pop", "1"),
        ];
        assert_eq!(titles(&both), ["Alpha", "Alphabet City", "The Charlie Movie"]);
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
        assert_eq!(titles(&[("year>", "2001")]), ["Bravo", "Charlie"], "`>=` is at least, Kometa's `.gte`");
        assert_eq!(titles(&[("year<", "2001")]), ["Alpha", "Bravo"], "`<=` is at most, Kometa's `.lte`");
        assert_eq!(titles(&[("year!", "-1")]), ["Alpha", "Bravo", "Charlie"], "a missing value is -1");
        assert_eq!(titles(&[("year", "-1")]), ["Delta"]);
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
        let own = own_guid("movie", 2);
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
        let old_added = (now - 40 * 86_400).to_string();
        assert_eq!(titles(&[("addedAt>", old_added.as_str())]), ["Recent", "Old"], "on or after");
        assert_eq!(titles(&[("addedAt<", old_added.as_str())]), ["Old"], "on or before");
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

        let mut album = row(3, "album", "Discovery");
        album.parent_id = Some(5);
        album.parent_title = Some("Daft Punk".into());
        let albums = vec![album, row(4, "album", "Loose")];
        assert_eq!(matching(&albums, "album", &[("artist.id", "5")], &none, 0), ["Discovery"]);
        assert_eq!(matching(&albums, "album", &[("artist.title!", "daft")], &none, 0), ["Loose"]);
    }

    #[test]
    fn other_and_non_ascii_parameters_filter_nothing() {
        let rows = vec![row(1, "movie", "Alpha"), row(2, "movie", "Beta")];
        let none = HashMap::new();
        let pairs = [
            ("X-Plex-Product", "Plex Web"),
            ("x-plex-container-start", "0"),
            ("includeGuids", "1"),
            ("sort", "titleSort"),
            ("token", "abc"),
            ("checkFiles", "1"),
            // Keys whose 7th byte falls inside a character.
            ("abcdefé", "1"),
            ("ééééé", "x"),
        ];
        assert_eq!(matching(&rows, "movie", &pairs, &none, 0), ["Alpha", "Beta"]);
        assert!(parsed(&pairs, "movie").unwrap().is_empty());
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
        SectionQuery { now: NOW, ..SectionQuery::new(library, kind, &params(pairs)).unwrap() }
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
        let guid = own_guid("movie", 3);
        let show_guid = own_guid("show", 3);
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
            vec![("year<", "2001")],
            vec![("year>", "2001")],
            vec![("year>", "nan")],
            vec![("year", "-1")],
            vec![("year!", "-1")],
            vec![("rating!", "-1")],
            vec![("rating", "-1")],
            vec![("title=", "alpha")],
            vec![("title!=", "alpha")],
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
            vec![("addedAt>", "1000")],
            vec![("addedAt<", "1000")],
            vec![("addedAt<", "soon")],
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
            vec![("genre", "horror"), ("unwatched", "1")],
            vec![
                ("push", "1"),
                ("genre", "horror"),
                ("or", "1"),
                ("genre", "comedy"),
                ("pop", "1"),
                ("year>>", "1990"),
            ],
            vec![("genre", "horror"), ("or", "1"), ("unwatched", "1")],
            vec![("title", "alpha"), ("or", "1"), ("push", "1"), ("year", "2001"), ("and", "1"), ("genre!", "drama")],
            vec![("or", "1"), ("rating>>", "7"), ("or", "1")],
            vec![("push", "1"), ("pop", "1"), ("label", "overlay")],
            vec![("title", "zzz"), ("or", "1"), ("title", "")],
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
            vec![("title", "alpha"), ("or", "1"), ("title", "é")],
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
            vec![("unwatched", "1")],
            vec![("show.title", "lost"), ("or", "1"), ("season.index", "2")],
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
