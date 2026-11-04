//! Titles and people: found through the full-text indexes of the database
//! (`items_fts`, `people_fts`), with the words of those indexes and the
//! pieces of every title and name kept in memory (an [`Index`]).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::fuzzy::{self, Reading, Words};
use super::text::{Query, closeness, match_rank};
use super::trigram::Trigrams;
use crate::catalog::{self, ItemRow, ME_CTE, VISIBLE};
use crate::db::Db;
use crate::people::{PersonSummary, photo_url};

/// What a search looks at before choosing what to show, at most.
const CANDIDATES: i64 = 300;

/// How alike a title or name must be to the search to be found when its
/// words find nothing (see [`super::text::similarity`]).
const NEAR: f32 = 0.8;

/// How often the index is checked against the library (a cheap query).
const CHECKED: Duration = Duration::from_secs(5);

/// How often it is read again at most: a scan changes the library many
/// times a second, and words half a minute old still read slips well.
const READ: Duration = Duration::from_secs(30);

/// The words of the library's titles and people's names, and their pieces,
/// read from the database when first searched and again after the library
/// changes. Only in memory: nothing of it is written anywhere.
#[derive(Default)]
pub struct Index {
    held: Mutex<Option<Holding>>,
    /// One reading at a time; searches meanwhile use what's held.
    reading: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
struct Holding {
    held: Arc<Held>,
    read_at: Instant,
    checked_at: Instant,
}

#[derive(Default)]
struct Held {
    /// What the library was when read (see [`changes`]).
    changes: Changes,
    titles: Words,
    names: Words,
    title_pieces: Trigrams,
    name_pieces: Trigrams,
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("search::Index")
    }
}

impl Index {
    /// What the index holds, read again when the library changed. When it
    /// can't be read, search goes on with what it held (or nothing: words are
    /// then found as typed only) and reads it again later.
    async fn get(&self, db: &Db) -> Arc<Held> {
        let holding = self.held.lock().unwrap().clone();
        if let Some(h) = &holding
            && h.checked_at.elapsed() < CHECKED
        {
            return h.held.clone();
        }
        let _reading = match (self.reading.try_lock(), &holding) {
            (Ok(guard), _) => guard,
            (Err(_), Some(h)) => return h.held.clone(),
            (Err(_), None) => self.reading.lock().await,
        };
        // Checked by another search while this one waited.
        let holding = self.held.lock().unwrap().clone();
        if let Some(h) = &holding
            && h.checked_at.elapsed() < CHECKED
        {
            return h.held.clone();
        }
        let checked = Instant::now();
        let fresh = match (changes(db).await, holding) {
            (Ok(now), Some(h)) if h.held.changes == now || h.read_at.elapsed() < READ => {
                Holding { checked_at: checked, ..h }
            }
            (Ok(now), holding) => match read(db, now).await {
                Ok(held) => Holding { held: Arc::new(held), read_at: checked, checked_at: checked },
                Err(e) => {
                    tracing::warn!("search: the library's words weren't read: {e}");
                    keep(holding, checked)
                }
            },
            (Err(e), holding) => {
                tracing::warn!("search: the library's changes weren't read: {e}");
                keep(holding, checked)
            }
        };
        let held = fresh.held.clone();
        *self.held.lock().unwrap() = Some(fresh);
        held
    }
}

/// What was held, or nothing, until the next try.
fn keep(holding: Option<Holding>, checked: Instant) -> Holding {
    holding.map(|h| Holding { checked_at: checked, ..h }).unwrap_or_else(|| Holding {
        held: Arc::default(),
        read_at: checked,
        checked_at: checked,
    })
}

/// What changes with the titles and names: the libraries' listing versions
/// (bumped by every change to an item), the people (renamed ones by their
/// `updated_at`) and their credits.
type Changes = (i64, i64, i64, i64);

async fn changes(db: &Db) -> sqlx::Result<Changes> {
    sqlx::query_as(
        "SELECT (SELECT COALESCE(SUM(version), 0) FROM listing_versions WHERE scope = 'library'),
                (SELECT COUNT(*) FROM people), (SELECT COALESCE(MAX(updated_at), 0) FROM people),
                (SELECT COUNT(*) FROM item_people)",
    )
    .fetch_one(&db.read)
    .await
}

async fn read(db: &Db, changes: Changes) -> sqlx::Result<Held> {
    let mut conn = db.read.acquire().await?;
    // The indexes' words, through tables of the connection's own (temp)
    // schema: nothing is added to the database.
    for (table, index) in [("search_title_words", "items_fts"), ("search_name_words", "people_fts")] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS temp.{table} USING fts5vocab(main, {index}, row)"
        )))
        .execute(&mut *conn)
        .await?;
    }
    let title_words: Vec<(String, i64)> =
        sqlx::query_as("SELECT term, doc FROM temp.search_title_words").fetch_all(&mut *conn).await?;
    let name_words: Vec<(String, i64)> =
        sqlx::query_as("SELECT term, doc FROM temp.search_name_words").fetch_all(&mut *conn).await?;
    let titles: Vec<(i64, String, Option<String>)> =
        sqlx::query_as("SELECT id, title, original_title FROM items WHERE kind != 'extra'")
            .fetch_all(&mut *conn)
            .await?;
    let names: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM people WHERE EXISTS (SELECT 1 FROM item_people c WHERE c.person_id = people.id)",
    )
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);
    let held = tokio::task::spawn_blocking(move || {
        let count = |words: Vec<(String, i64)>| Words::new(words.into_iter().map(|(w, n)| (w, n as u32)));
        let title_names = titles.into_iter().flat_map(|(id, title, original)| {
            let original = original.filter(|o| *o != title);
            std::iter::once((id, title)).chain(original.map(|o| (id, o)))
        });
        Held {
            changes,
            titles: count(title_words),
            names: count(name_words),
            title_pieces: Trigrams::new(title_names),
            name_pieces: Trigrams::new(names),
        }
    })
    .await
    .map_err(|e| sqlx::Error::Protocol(format!("reading the search index: {e}")))?;
    tracing::debug!(
        titles = held.title_pieces.len(),
        names = held.name_pieces.len(),
        words = held.titles.len() + held.names.len(),
        "search index read"
    );
    Ok(held)
}

/// What `pieces` of the index finds alike to `query`: on a thread of its
/// own, as the first search to need them works the pieces out.
async fn alike(index: &Arc<Held>, pieces: fn(&Held) -> &Trigrams, query: String) -> sqlx::Result<Vec<i64>> {
    let index = index.clone();
    tokio::task::spawn_blocking(move || pieces(&index).near(&query, NEAR))
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("searching: {e}")))
}

/// A query FTS5 can't read finds nothing (the words are quoted, so this is
/// for what quoting misses).
fn unless_unreadable<T: Default>(result: sqlx::Result<T>) -> sqlx::Result<T> {
    match result {
        Err(sqlx::Error::Database(e)) if e.message().contains("fts5") => Ok(T::default()),
        other => other,
    }
}

/// Titles `user_id` may see that `input` finds, at most `limit`: by kind
/// (movies, shows, seasons, episodes, then music), the closest matches
/// first in each.
///
/// Every word of `input` begins a word of the title (or of its original
/// title); a year at the end narrows them to that year's; a word the
/// library doesn't have is read as the closest it has (see
/// [`fuzzy::read`]). When that finds nothing, titles alike enough are found.
pub async fn items(db: &Db, user_id: i64, input: &str, limit: i64) -> sqlx::Result<Vec<ItemRow>> {
    let query = Query::parse(input);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let index = db.search.get(db).await;
    let mut reading = fuzzy::read(&query.words, &index.titles);
    let mut ids = matching_items(db, user_id, &reading, query.year).await?;
    if ids.is_empty() && query.year.is_some() {
        // No title of that year: the year is part of the title ("2001: …", "Blade Runner 2049").
        reading = fuzzy::read(&query.all_words(), &index.titles);
        ids = matching_items(db, user_id, &reading, None).await?;
    }
    let mut rows = catalog::items_by_ids(db, user_id, &ids).await?;
    let mut found_alike = false;
    if rows.is_empty() {
        let near = alike(&index, |held| &held.title_pieces, query.all_text()).await?;
        rows = catalog::items_by_ids(db, user_id, &near[..near.len().min(CANDIDATES as usize)]).await?;
        found_alike = true;
    }
    rank_items(&mut rows, &query, reading.corrected || found_alike, limit.max(0) as usize);
    Ok(rows)
}

async fn matching_items(db: &Db, user_id: i64, reading: &Reading, year: Option<i64>) -> sqlx::Result<Vec<i64>> {
    let Some(fts) = reading.fts() else { return Ok(Vec::new()) };
    let years = if year.is_some() { "AND i.year BETWEEN ? AND ?" } else { "" };
    let sql = format!(
        "{ME_CTE} SELECT i.id FROM items_fts f JOIN items i ON i.id = f.rowid
             LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE items_fts MATCH ? AND i.kind != 'extra' AND {VISIBLE} {years}
         ORDER BY f.rank LIMIT ?"
    );
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(user_id).bind(fts);
    if let Some(year) = year {
        q = q.bind(year - 1).bind(year + 1);
    }
    unless_unreadable(q.bind(CANDIDATES).fetch_all(&db.read).await)
}

/// Where a kind of item comes in search results: movies, shows, seasons
/// and episodes, then music.
fn search_order(kind: &str) -> u8 {
    match kind {
        "movie" => 0,
        "show" => 1,
        "season" => 2,
        "episode" => 3,
        "artist" => 4,
        "album" => 5,
        "track" => 6,
        _ => 7,
    }
}

/// Keeps the `limit` best of `rows` (in the index's order of relevance) and
/// puts them in the order search shows them. The best: whole titles, then
/// their beginnings, then a word onwards (a sort title without its "The"
/// counts too), top-level items before seasons, albums, episodes and tracks
/// that match as well, so a large library's episodes don't crowd out the
/// movie whose name was typed. When words were read as others (`near`), the
/// titles closest to what was typed come first among equals.
fn rank_items(rows: &mut Vec<ItemRow>, query: &Query, near: bool, limit: usize) {
    let text = query.text();
    let depth = |kind: &str| match kind {
        "movie" | "show" | "artist" => 0,
        "album" | "season" => 1,
        _ => 2,
    };
    rows.sort_by_cached_key(|r| {
        let rank = match_rank(text, &r.title).min(match_rank(text, &r.sort_title));
        let close = if near {
            closeness(&super::text::fold(&r.title), text).min(closeness(&super::text::fold(&r.sort_title), text))
        } else {
            std::cmp::Reverse(0)
        };
        (rank, close, depth(&r.kind))
    });
    rows.truncate(limit);
    rows.sort_by_key(|r| search_order(&r.kind));
}

/// People whose name `input` finds (as titles are found, see [`items`]),
/// among those credited in items `user_id` may see: the closest names
/// first (the whole name, then its beginning, …), the most credited first
/// among them.
pub async fn people(db: &Db, user_id: i64, input: &str, limit: i64) -> sqlx::Result<Vec<PersonSummary>> {
    let query = Query::parse(input);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let index = db.search.get(db).await;
    // Names have no years: a year is a word.
    let reading = fuzzy::read(&query.all_words(), &index.names);
    let mut rows = match reading.fts() {
        Some(fts) => unless_unreadable(credited(db, user_id, Candidates::Matching(&fts), limit).await)?,
        None => Vec::new(),
    };
    let mut near = reading.corrected;
    if rows.is_empty() {
        let found = alike(&index, |held| &held.name_pieces, query.all_text()).await?;
        let found = serde_json::to_string(&found[..found.len().min(CANDIDATES as usize)]).unwrap_or_default();
        rows = credited(db, user_id, Candidates::Ids(&found), limit).await?;
        near = true;
    }
    let text = query.all_text();
    rows.sort_by_cached_key(|(_, name, _)| {
        let close = if near { closeness(&super::text::fold(name), &text) } else { std::cmp::Reverse(0) };
        (match_rank(&text, name), close)
    });
    rows.truncate(limit.max(0) as usize);
    Ok(rows
        .into_iter()
        .map(|(id, name, photo)| PersonSummary { id, photo_url: photo_url(id, photo.as_deref()), name })
        .collect())
}

/// The people a search looks at.
enum Candidates<'a> {
    /// Those whose name matches an FTS5 query.
    Matching(&'a str),
    /// These, a JSON array of their ids.
    Ids(&'a str),
}

/// Of `candidates`, those credited in items `user_id` may see, most
/// credited first: more than `limit`, for a name typed in full whose person
/// has few credits. Someone with access limits gets the 200 best ranked by
/// the credits they can see.
async fn credited(
    db: &Db,
    user_id: i64,
    candidates: Candidates<'_>,
    limit: i64,
) -> sqlx::Result<Vec<(i64, String, Option<String>)>> {
    let shown = limit.max(100);
    let (from, arg) = match candidates {
        Candidates::Matching(fts) => ("people_fts f JOIN people pe ON pe.id = f.rowid WHERE people_fts MATCH ?", fts),
        Candidates::Ids(ids) => ("people pe WHERE pe.id IN (SELECT value FROM json_each(?))", ids),
    };
    let unrestricted: bool = sqlx::query_scalar(
        "SELECT (is_admin OR all_libraries) AND (is_admin OR max_age IS NULL) FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(&db.read)
    .await?
    .unwrap_or(false);
    if unrestricted {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT pe.id, pe.name, pe.photo_url FROM {from}
               AND EXISTS (SELECT 1 FROM item_people c WHERE c.person_id = pe.id)
             ORDER BY (SELECT COUNT(*) FROM item_people c WHERE c.person_id = pe.id) DESC, pe.name LIMIT ?"
        )))
        .bind(arg)
        .bind(shown)
        .fetch_all(&db.read)
        .await
    } else {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{ME_CTE}, candidates AS (
                 SELECT pe.id, pe.name, pe.photo_url FROM {from}
                 ORDER BY (SELECT COUNT(*) FROM item_people c WHERE c.person_id = pe.id) DESC, pe.name LIMIT 200),
             counted AS (
                 SELECT c.id, c.name, c.photo_url,
                        (SELECT COUNT(*) FROM item_people ip JOIN items i ON i.id = ip.item_id
                             LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
                         WHERE ip.person_id = c.id AND {VISIBLE}) AS visible
                 FROM candidates c)
             SELECT id, name, photo_url FROM counted WHERE visible > 0 ORDER BY visible DESC, name LIMIT ?"
        )))
        .bind(user_id)
        .bind(arg)
        .bind(shown)
        .fetch_all(&db.read)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_by_kind_closest_first() {
        let row = |id, kind, title: &str| ItemRow::test_row(id, kind, title);
        let heat = Query::parse("heat");
        // In the index's order: an episode, a longer movie title, an album,
        // the movie of that name, a show beginning with it.
        let mut rows = vec![
            row(1, "episode", "Heat"),
            row(2, "movie", "Heat Wave"),
            row(3, "album", "Heat"),
            row(4, "movie", "Heat"),
            row(5, "show", "Heatwave"),
        ];
        rank_items(&mut rows, &heat, false, 10);
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, [4, 2, 5, 1, 3]);

        // Fewer than found: whole titles stay, the movie and then the album
        // before the episode of the same name.
        let mut rows = vec![
            row(1, "episode", "Heat"),
            row(2, "movie", "Heat Wave"),
            row(3, "album", "Heat"),
            row(4, "movie", "Heat"),
        ];
        rank_items(&mut rows, &heat, false, 2);
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, [4, 3]);

        // A sort title counts: "Office" is the whole of "The Office".
        let office_query = Query::parse("office");
        let mut office = row(1, "show", "The Office");
        office.sort_title = "Office".into();
        let mut rows = vec![row(2, "movie", "Office Space"), office];
        rank_items(&mut rows, &office_query, false, 10);
        assert_eq!(rows[1].id, 1);
        rank_items(&mut rows, &office_query, false, 1);
        assert_eq!(rows[0].id, 1);
    }

    #[test]
    fn titles_read_from_slips_come_closest_first() {
        let row = |id, kind, title: &str| ItemRow::test_row(id, kind, title);
        let mut rows =
            vec![row(1, "movie", "The Mother"), row(2, "movie", "The Matrix Reloaded"), row(3, "movie", "The Matrix")];
        rank_items(&mut rows, &Query::parse("the matirx"), true, 1);
        assert_eq!(rows[0].id, 3);
    }
}
