//! Collections: groups of movies or shows within one library, shared by all
//! users. Automatic ones mirror TMDB's collections and are filled by the
//! metadata agent; administrators create and edit their own. Items a user
//! can't see are left out, and collections left without visible items are
//! hidden from that user.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow, VISIBLE, me_cte};
use crate::db::Db;
use crate::store::Store;
use crate::util::{now, sort_title};

/// Automatic collections with fewer items than this are not listed (a
/// collection holding a single film is clutter).
const MIN_AUTOMATIC: i64 = 2;
const IMAGES: &str = "https://image.tmdb.org/t/p";

/// `belongs_to_collection` of a TMDB movie.
#[derive(Debug, Clone, Deserialize)]
pub struct TmdbCollection {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct CollectionRow {
    id: i64,
    library_id: i64,
    title: String,
    sort_title: String,
    summary: Option<String>,
    tmdb_id: Option<i64>,
    poster: Option<String>,
    backdrop: Option<String>,
    sort: String,
    smart_filter: Option<String>,
    created_at: i64,
    updated_at: i64,
}

const COLUMNS: &str =
    "id, library_id, title, sort_title, summary, tmdb_id, poster, backdrop, sort, smart_filter, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct CollectionDto {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    /// What it is ordered by; set by hand (through the Plex API) when it isn't
    /// the title's own.
    #[serde(skip)]
    pub sort_title: String,
    pub summary: Option<String>,
    /// Filled automatically from TMDB.
    pub automatic: bool,
    /// `release`, `title` or `custom`.
    pub sort: String,
    /// A smart collection's filter: the section listing query (Plex's syntax)
    /// whose items it shows. `None` for collections that hold items.
    pub smart_filter: Option<String>,
    /// Items this user can see.
    pub item_count: i64,
    /// The collection's own artwork, else its first visible item's.
    pub poster_url: String,
    pub backdrop_url: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl CollectionRow {
    fn into_dto(self, item_count: i64) -> CollectionDto {
        CollectionDto {
            poster_url: format!("/api/collections/{}/images/poster?v={}", self.id, self.updated_at),
            backdrop_url: format!("/api/collections/{}/images/backdrop?v={}", self.id, self.updated_at),
            id: self.id,
            library_id: self.library_id,
            title: self.title,
            sort_title: self.sort_title,
            summary: self.summary,
            automatic: self.tmdb_id.is_some(),
            sort: self.sort,
            smart_filter: self.smart_filter,
            item_count,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Visible item counts of collections, for one user.
async fn visible_counts(db: &Db, user_id: i64, ids: &[i64]) -> sqlx::Result<HashMap<i64, i64>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "{} SELECT ci.collection_id, COUNT(*) FROM collection_items ci
           JOIN items i ON i.id = ci.item_id
           LEFT JOIN items p ON p.id = i.parent_id
           LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE {VISIBLE} AND ci.collection_id IN ({placeholders})
         GROUP BY ci.collection_id",
        me_cte!()
    );
    let mut query = sqlx::query_as::<_, (i64, i64)>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
    for id in ids {
        query = query.bind(id);
    }
    Ok(query.fetch_all(&db.read).await?.into_iter().collect())
}

/// Whether a collection with `count` visible items is listed for the user.
fn listed(row: &CollectionRow, count: i64, user: &AuthUser) -> bool {
    if row.tmdb_id.is_some() { count >= MIN_AUTOMATIC } else { count > 0 || user.is_admin }
}

/// Collections of a library (the caller checked library access).
pub async fn list(db: &Db, user: &AuthUser, library_id: i64) -> sqlx::Result<Vec<CollectionDto>> {
    let rows: Vec<CollectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE library_id = ? ORDER BY sort_title COLLATE NOCASE, id"
    )))
    .bind(library_id)
    .fetch_all(&db.read)
    .await?;
    let ids: Vec<i64> = rows.iter().filter(|r| r.smart_filter.is_none()).map(|r| r.id).collect();
    let mut counts = visible_counts(db, user.id, &ids).await?;
    if rows.iter().any(|r| r.smart_filter.is_some())
        && let Some(lib) = crate::library::get_library(db, library_id).await?
    {
        for row in &rows {
            if let Some(filter) = &row.smart_filter {
                let count = crate::listing::smart_count(db, user.id, &lib, filter).await?;
                counts.insert(row.id, i64::try_from(count).unwrap_or(i64::MAX));
            }
        }
    }
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let count = counts.get(&r.id).copied().unwrap_or(0);
            listed(&r, count, user).then(|| r.into_dto(count))
        })
        .collect())
}

async fn row(db: &Db, id: i64) -> sqlx::Result<Option<CollectionRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM collections WHERE id = ?")))
        .bind(id)
        .fetch_optional(&db.read)
        .await
}

/// Item ids of a collection in its display order.
async fn ordered_ids(db: &Db, id: i64, sort: &str) -> sqlx::Result<Vec<i64>> {
    let order = match sort {
        "title" => "i.sort_title, i.year",
        "custom" => "ci.position, i.sort_title",
        _ => {
            "COALESCE(i.release_date, CAST(i.year AS TEXT)) IS NULL, COALESCE(i.release_date, CAST(i.year AS TEXT)), i.sort_title"
        }
    };
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT ci.item_id FROM collection_items ci JOIN items i ON i.id = ci.item_id
         WHERE ci.collection_id = ? ORDER BY {order}"
    )))
    .bind(id)
    .fetch_all(&db.read)
    .await
}

/// The collection and its visible items, if the user may see it. Library
/// access is checked here too.
pub async fn get(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<Option<(CollectionDto, Vec<ItemRow>)>> {
    let Some(row) = row(db, id).await? else { return Ok(None) };
    if !crate::access::can_open_library(db, user, row.library_id).await? {
        return Ok(None);
    }
    let items = match &row.smart_filter {
        // What the filter finds, in the filter's order.
        Some(filter) => match crate::library::get_library(db, row.library_id).await? {
            Some(lib) => crate::listing::smart_items(db, user.id, &lib, filter).await?,
            None => Vec::new(),
        },
        None => catalog::items_by_ids(db, user.id, &ordered_ids(db, id, &row.sort).await?).await?,
    };
    if items.is_empty() && !(user.is_admin && row.tmdb_id.is_none()) {
        return Ok(None);
    }
    let count = items.len() as i64;
    Ok(Some((row.into_dto(count), items)))
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CollectionRef {
    pub id: i64,
    pub title: String,
}

/// Collections an item belongs to (the caller checked the item is visible).
pub async fn for_item(db: &Db, user: &AuthUser, item_id: i64) -> sqlx::Result<Vec<CollectionRef>> {
    let rows: Vec<CollectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE id IN (SELECT collection_id FROM collection_items WHERE item_id = ?)
         ORDER BY sort_title COLLATE NOCASE"
    )))
    .bind(item_id)
    .fetch_all(&db.read)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let counts = visible_counts(db, user.id, &ids).await?;
    Ok(rows
        .into_iter()
        .filter(|r| listed(r, counts.get(&r.id).copied().unwrap_or(0), user))
        .map(|r| CollectionRef { id: r.id, title: r.title })
        .collect())
}

pub async fn library_of(db: &Db, id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT library_id FROM collections WHERE id = ?").bind(id).fetch_optional(&db.read).await
}

/// A smart collection of `library_id`, filled by `filter` (see [`CollectionDto::smart_filter`]).
pub async fn create_smart(db: &Db, library_id: i64, title: &str, filter: &str) -> sqlx::Result<i64> {
    let id = create(db, library_id, title, None).await?;
    set_smart_filter(db, id, filter).await?;
    Ok(id)
}

pub async fn set_smart_filter(db: &Db, id: i64, filter: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE collections SET smart_filter = ?, updated_at = ? WHERE id = ?")
        .bind(filter)
        .bind(now())
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(())
}

/// Whether the collection is a smart one (it holds no items of its own).
pub async fn is_smart(db: &Db, id: i64) -> sqlx::Result<bool> {
    Ok(sqlx::query_scalar::<_, Option<String>>("SELECT smart_filter FROM collections WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.read)
        .await?
        .flatten()
        .is_some())
}

pub async fn create(db: &Db, library_id: i64, title: &str, summary: Option<&str>) -> sqlx::Result<i64> {
    let now = now();
    sqlx::query_scalar(
        "INSERT INTO collections (library_id, title, sort_title, summary, sort, edited, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'release', 1, ?, ?) RETURNING id",
    )
    .bind(library_id)
    .bind(title)
    .bind(sort_title(title))
    .bind(summary)
    .bind(now)
    .bind(now)
    .fetch_one(&db.write)
    .await
}

pub async fn update(
    db: &Db,
    id: i64,
    title: Option<&str>,
    summary: Option<Option<&str>>,
    sort: Option<&str>,
) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    // `edited` (SET reads the old row) only when a value really changes.
    if let Some(title) = title {
        sqlx::query("UPDATE collections SET title = ?, sort_title = ?, edited = edited OR title IS NOT ? WHERE id = ?")
            .bind(title)
            .bind(sort_title(title))
            .bind(title)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(summary) = summary {
        sqlx::query("UPDATE collections SET summary = ?, edited = edited OR summary IS NOT ? WHERE id = ?")
            .bind(summary)
            .bind(summary)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(sort) = sort {
        sqlx::query("UPDATE collections SET sort = ? WHERE id = ?").bind(sort).bind(id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE collections SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&mut *tx).await?;
    tx.commit().await
}

pub async fn delete(db: &Db, id: i64) -> sqlx::Result<bool> {
    let deleted =
        sqlx::query("DELETE FROM collections WHERE id = ?").bind(id).execute(&db.write).await?.rows_affected();
    Ok(deleted > 0)
}

/// How many of `item_ids` are movies or shows of the library (what a
/// collection of it can hold).
pub async fn eligible(db: &Db, library_id: i64, item_ids: &[i64]) -> sqlx::Result<i64> {
    if item_ids.is_empty() {
        return Ok(0);
    }
    let placeholders = vec!["?"; item_ids.len()].join(",");
    let sql = format!(
        "SELECT COUNT(*) FROM items WHERE library_id = ? AND kind IN ('movie', 'show') AND id IN ({placeholders})"
    );
    let mut query = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(library_id);
    for id in item_ids {
        query = query.bind(id);
    }
    query.fetch_one(&db.read).await
}

/// Add movies or shows of the collection's library; others are ignored.
/// Returns how many were added.
pub async fn add_items(db: &Db, id: i64, library_id: i64, item_ids: &[i64]) -> sqlx::Result<u64> {
    let mut tx = db.write.begin().await?;
    let mut added = 0;
    for item in item_ids {
        added += sqlx::query(
            "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position)
             SELECT ?, i.id, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_items WHERE collection_id = ?)
             FROM items i WHERE i.id = ? AND i.library_id = ? AND i.kind IN ('movie', 'show')",
        )
        .bind(id)
        .bind(id)
        .bind(item)
        .bind(library_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    sqlx::query("UPDATE collections SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(added)
}

pub async fn remove_item(db: &Db, id: i64, item_id: i64) -> sqlx::Result<bool> {
    let removed = sqlx::query("DELETE FROM collection_items WHERE collection_id = ? AND item_id = ?")
        .bind(id)
        .bind(item_id)
        .execute(&db.write)
        .await?
        .rows_affected();
    if removed > 0 {
        sqlx::query("UPDATE collections SET updated_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&db.write)
            .await?;
    }
    Ok(removed > 0)
}

/// Custom order: `item_ids` first in that order, anything not listed after.
pub async fn reorder(db: &Db, id: i64, item_ids: &[i64]) -> sqlx::Result<()> {
    let current = ordered_ids(db, id, "custom").await?;
    let order = crate::playlists::merged_order(&current, item_ids);
    let mut tx = db.write.begin().await?;
    for (position, item) in order.iter().enumerate() {
        sqlx::query("UPDATE collection_items SET position = ? WHERE collection_id = ? AND item_id = ?")
            .bind(position as i64)
            .bind(id)
            .bind(item)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE collections SET sort = 'custom', updated_at = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Move an item right after another one, or first (`after: None`), in the
/// order the collection shows now, which it keeps from then on (`custom`).
/// False when either isn't in the collection.
pub async fn move_item(db: &Db, id: i64, item_id: i64, after: Option<i64>) -> sqlx::Result<bool> {
    let Some(row) = row(db, id).await? else { return Ok(false) };
    let mut order = ordered_ids(db, id, &row.sort).await?;
    let Some(from) = order.iter().position(|i| *i == item_id) else { return Ok(false) };
    order.remove(from);
    let to = match after {
        Some(after) => match order.iter().position(|i| *i == after) {
            Some(at) => at + 1,
            None => return Ok(false),
        },
        None => 0,
    };
    order.insert(to, item_id);
    reorder(db, id, &order).await?;
    Ok(true)
}

/// Own artwork of the collection, or `Err(item)` to fall back to: the first
/// visible item that has such artwork (else the first one).
pub async fn artwork(
    db: &Db,
    store: &Store,
    user: &AuthUser,
    id: i64,
    kind: &str,
) -> sqlx::Result<Option<Result<PathBuf, i64>>> {
    let Some(row) = row(db, id).await? else { return Ok(None) };
    let Some((_, items)) = get(db, user, id).await? else { return Ok(None) };
    let poster = kind == "poster";
    let own = if poster { row.poster } else { row.backdrop };
    if let Some(path) = own.and_then(|r| store.path(&r)).filter(|p| p.is_file()) {
        return Ok(Some(Ok(path)));
    }
    let with_art = items.iter().find(|i| if poster { i.has_poster } else { i.has_backdrop });
    Ok(with_art.or(items.first()).map(|i| Err(i.id)))
}

async fn download(store: &Store, remote: &str, size: &str) -> Option<String> {
    store.remote(&format!("{IMAGES}/{size}{remote}")).await.ok()
}

/// Put a movie into its TMDB collection (creating it), take it out of any
/// other automatic one, and drop automatic collections left empty.
pub async fn link_tmdb(db: &Db, store: &Store, item_id: i64, found: Option<&TmdbCollection>) -> anyhow::Result<()> {
    let library_id: i64 =
        sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(item_id).fetch_one(&db.read).await?;
    let found = found.filter(|c| !c.name.trim().is_empty());
    let now = now();
    // Membership changes in one transaction: a concurrent refresh must not
    // drop a collection between its creation and its first item.
    let mut tx = db.write.begin().await?;
    sqlx::query(
        "DELETE FROM collection_items WHERE item_id = ?
         AND collection_id IN (SELECT id FROM collections WHERE tmdb_id IS NOT NULL AND tmdb_id != ?)",
    )
    .bind(item_id)
    .bind(found.map(|c| c.id).unwrap_or(-1))
    .execute(&mut *tx)
    .await?;
    let mut linked = None;
    if let Some(c) = found {
        let (id, poster, backdrop): (i64, Option<String>, Option<String>) = sqlx::query_as(
            "INSERT INTO collections (library_id, title, sort_title, tmdb_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(library_id, tmdb_id) WHERE tmdb_id IS NOT NULL DO UPDATE SET
                title = CASE WHEN collections.edited THEN collections.title ELSE excluded.title END,
                sort_title = CASE WHEN collections.edited THEN collections.sort_title ELSE excluded.sort_title END
             RETURNING id, poster, backdrop",
        )
        .bind(library_id)
        .bind(c.name.trim())
        .bind(sort_title(c.name.trim()))
        .bind(c.id)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position)
             VALUES (?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_items WHERE collection_id = ?))",
        )
        .bind(id)
        .bind(item_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        linked = Some((c, id, poster, backdrop));
    }
    let emptied: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM collections WHERE tmdb_id IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM collection_items WHERE collection_id = collections.id)
         RETURNING id",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    // Artwork is fetched once, outside the transaction.
    if let Some((c, id, poster, backdrop)) = linked {
        let missing = |p: &Option<String>| p.as_deref().is_none_or(|p| !store.exists(p));
        let new_poster = match (&c.poster_path, missing(&poster)) {
            (Some(remote), true) => download(store, remote, "w780").await,
            _ => None,
        };
        let new_backdrop = match (&c.backdrop_path, missing(&backdrop)) {
            (Some(remote), true) => download(store, remote, "w1280").await,
            _ => None,
        };
        if new_poster.is_some() || new_backdrop.is_some() {
            sqlx::query(
                "UPDATE collections SET poster = COALESCE(?, poster), backdrop = COALESCE(?, backdrop), updated_at = ?
                 WHERE id = ?",
            )
            .bind(new_poster)
            .bind(new_backdrop)
            .bind(now)
            .bind(id)
            .execute(&db.write)
            .await?;
        }
    }
    if !emptied.is_empty() {
        tracing::debug!("removed {} empty automatic collections", emptied.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_collection_parses() {
        let json = r#"{"id": 1241, "name": "Harry Potter Collection", "poster_path": "/p.jpg", "backdrop_path": null}"#;
        let c: TmdbCollection = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, 1241);
        assert_eq!(c.poster_path.as_deref(), Some("/p.jpg"));
        assert!(c.backdrop_path.is_none());
    }
}
