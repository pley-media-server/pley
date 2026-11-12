//! Playlists: a user's own ordered lists of movies and episodes (video) or
//! tracks (audio). Adding a show, season, album or artist adds its playable
//! items in order. Entries the user can no longer see (library access, age
//! limit) are skipped, not deleted.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sqlx::FromRow;

use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow, ItemSummary, VISIBLE, me_cte};
use crate::db::Db;
use crate::error::AppError;
use crate::util::now;

/// Most entries a playlist can hold.
const MAX_ENTRIES: i64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Audio,
}

impl Kind {
    pub fn parse(value: &str) -> Option<Kind> {
        match value {
            "video" => Some(Kind::Video),
            "audio" => Some(Kind::Audio),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Kind::Video => "video",
            Kind::Audio => "audio",
        }
    }

    /// The playlist kind an item (or its leaves) belongs in.
    pub fn of_item(kind: &str) -> Option<Kind> {
        match kind {
            "movie" | "show" | "season" | "episode" => Some(Kind::Video),
            "artist" | "album" | "track" => Some(Kind::Audio),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct PlaylistRow {
    id: i64,
    title: String,
    kind: String,
    library_id: Option<i64>,
    smart_filter: Option<String>,
    created_at: i64,
    updated_at: i64,
}

const COLUMNS: &str = "id, title, kind, library_id, smart_filter, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistDto {
    pub id: i64,
    pub title: String,
    /// `video` or `audio`.
    pub kind: String,
    /// Entries this user can see.
    pub item_count: i64,
    pub duration_ms: i64,
    /// Artwork of the first entries, for a mosaic.
    pub artwork: Vec<String>,
    /// A smart playlist's filter: a listing of `library_id` as a query string
    /// (see `crate::listing::smart_items`). `None` for playlists of entries.
    pub smart_filter: Option<String>,
    pub library_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryDto {
    pub entry_id: i64,
    pub item: ItemSummary,
}

/// Leaf items of `item` in playing order, as ids (not yet filtered).
async fn leaf_ids(db: &Db, item: &ItemRow) -> sqlx::Result<Vec<i64>> {
    let sql = match item.kind.as_str() {
        "movie" | "episode" | "track" => return Ok(vec![item.id]),
        "show" => {
            "SELECT e.id FROM items e JOIN items se ON se.id = e.parent_id
             WHERE e.grandparent_id = ? AND e.kind = 'episode' ORDER BY se.idx = 0, se.idx, e.idx IS NULL, e.idx, e.sort_title"
        }
        "season" => "SELECT id FROM items WHERE parent_id = ? AND kind = 'episode' ORDER BY idx IS NULL, idx, sort_title",
        "album" => "SELECT id FROM items WHERE parent_id = ? AND kind = 'track' ORDER BY disc, idx, sort_title",
        "artist" => {
            "SELECT t.id FROM items t JOIN items a ON a.id = t.parent_id
             WHERE t.grandparent_id = ? AND t.kind = 'track' ORDER BY a.year, a.sort_title, t.disc, t.idx"
        }
        _ => return Ok(Vec::new()),
    };
    sqlx::query_scalar(sql).bind(item.id).fetch_all(&db.read).await
}

/// The playlist kind of the first of `item_ids` the user can see.
pub async fn kind_of_first(db: &Db, user: &AuthUser, item_ids: &[i64]) -> sqlx::Result<Option<Kind>> {
    let first = catalog::items_by_ids(db, user.id, item_ids).await?.into_iter().next();
    Ok(first.and_then(|item| Kind::of_item(&item.kind)))
}

/// Visible leaves of the given items, in order.
pub async fn expand(db: &Db, user: &AuthUser, item_ids: &[i64]) -> sqlx::Result<Vec<ItemRow>> {
    let mut out = Vec::new();
    for item in catalog::items_by_ids(db, user.id, item_ids).await? {
        let ids = leaf_ids(db, &item).await?;
        out.extend(catalog::items_by_ids(db, user.id, &ids).await?);
    }
    Ok(out)
}

async fn owned(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<Option<PlaylistRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM playlists WHERE id = ? AND user_id = ?")))
        .bind(id)
        .bind(user.id)
        .fetch_optional(&db.read)
        .await
}

/// What a smart playlist plays: its filter's finds, shows and albums as
/// their episodes and tracks, at most [`MAX_ENTRIES`]. Entries are keyed by
/// item.
async fn smart_entries(db: &Db, user: &AuthUser, row: &PlaylistRow) -> sqlx::Result<Vec<(i64, ItemRow)>> {
    let (Some(library_id), Some(filter)) = (row.library_id, row.smart_filter.as_deref()) else { return Ok(Vec::new()) };
    let Some(lib) = crate::library::get_library(db, library_id).await? else { return Ok(Vec::new()) };
    let found = crate::listing::smart_items(db, user.id, &lib, filter).await?;
    let mut leaves = Vec::new();
    for item in found {
        if leaves.len() as i64 >= MAX_ENTRIES {
            break;
        }
        if matches!(item.kind.as_str(), "movie" | "episode" | "track") {
            leaves.push(item);
        } else {
            leaves.extend(expand(db, user, &[item.id]).await?);
        }
    }
    leaves.truncate(usize::try_from(MAX_ENTRIES).unwrap_or(usize::MAX));
    Ok(leaves.into_iter().map(|item| (item.id, item)).collect())
}

/// Whether playlist `id` is the user's: playlists are private, so it is
/// the only one who sees it.
pub async fn is_theirs(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<bool> {
    Ok(owned(db, user, id).await?.is_some())
}

/// Whether the user's playlist is a smart one (it holds no entries).
pub async fn is_smart(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<bool> {
    Ok(owned(db, user, id).await?.is_some_and(|row| row.smart_filter.is_some()))
}

fn artwork_of(item: &ItemSummary) -> Option<String> {
    match item.kind.as_str() {
        "episode" => item.thumb_url.clone().or_else(|| item.poster_url.clone()),
        _ => item.poster_url.clone(),
    }
}

/// The user's playlists with visible counts and durations.
pub async fn list(db: &Db, user: &AuthUser) -> sqlx::Result<Vec<PlaylistDto>> {
    let rows: Vec<PlaylistRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM playlists WHERE user_id = ? ORDER BY title COLLATE NOCASE, id"
    )))
    .bind(user.id)
    .fetch_all(&db.read)
    .await?;
    let sql = format!(
        "{} SELECT pi.playlist_id, COUNT(*), COALESCE(SUM(i.duration_ms), 0) FROM playlist_items pi
           JOIN playlists pl ON pl.id = pi.playlist_id
           JOIN items i ON i.id = pi.item_id
           LEFT JOIN items p ON p.id = i.parent_id
           LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE pl.user_id = (SELECT uid FROM me) AND {VISIBLE}
         GROUP BY pi.playlist_id",
        me_cte!()
    );
    let totals: HashMap<i64, (i64, i64)> = sqlx::query_as::<_, (i64, i64, i64)>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(user.id)
        .fetch_all(&db.read)
        .await?
        .into_iter()
        .map(|(id, count, duration)| (id, (count, duration)))
        .collect();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if row.smart_filter.is_some() {
            let entries = smart_entries(db, user, &row).await?;
            out.push(dto_of(row, entries).0);
            continue;
        }
        let first: Vec<i64> =
            sqlx::query_scalar("SELECT item_id FROM playlist_items WHERE playlist_id = ? ORDER BY position LIMIT 12")
                .bind(row.id)
                .fetch_all(&db.read)
                .await?;
        let artwork = catalog::items_by_ids(db, user.id, &first)
            .await?
            .iter()
            .filter_map(|r| artwork_of(&r.to_summary()))
            .fold(Vec::new(), |mut acc, url| {
                if !acc.contains(&url) && acc.len() < 4 {
                    acc.push(url);
                }
                acc
            });
        let (item_count, duration_ms) = totals.get(&row.id).copied().unwrap_or((0, 0));
        out.push(PlaylistDto {
            id: row.id,
            title: row.title,
            kind: row.kind,
            item_count,
            duration_ms,
            artwork,
            smart_filter: None,
            library_id: row.library_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        });
    }
    Ok(out)
}

/// A playlist as the API shows it, with its visible `entries`.
fn dto_of(row: PlaylistRow, entries: Vec<(i64, ItemRow)>) -> (PlaylistDto, Vec<(i64, ItemRow)>) {
    let mut artwork = Vec::new();
    for (_, item) in &entries {
        if let Some(url) = artwork_of(&item.to_summary()).filter(|u| !artwork.contains(u)) {
            artwork.push(url);
        }
        if artwork.len() == 4 {
            break;
        }
    }
    let dto = PlaylistDto {
        id: row.id,
        title: row.title,
        kind: row.kind,
        item_count: entries.len() as i64,
        duration_ms: entries.iter().map(|(_, item)| item.duration_ms.unwrap_or(0)).sum(),
        artwork,
        smart_filter: row.smart_filter,
        library_id: row.library_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    (dto, entries)
}

/// A playlist and its visible entries as `(entry id, item)`, if it belongs
/// to the user.
pub async fn get_rows(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<Option<(PlaylistDto, Vec<(i64, ItemRow)>)>> {
    let Some(row) = owned(db, user, id).await? else { return Ok(None) };
    if row.smart_filter.is_some() {
        let entries = smart_entries(db, user, &row).await?;
        return Ok(Some(dto_of(row, entries)));
    }
    let entries: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, item_id FROM playlist_items WHERE playlist_id = ? ORDER BY position, id")
            .bind(id)
            .fetch_all(&db.read)
            .await?;
    let mut ids: Vec<i64> = entries.iter().map(|(_, item)| *item).collect();
    ids.sort_unstable();
    ids.dedup();
    let rows: HashMap<i64, ItemRow> =
        catalog::items_by_ids(db, user.id, &ids).await?.into_iter().map(|r| (r.id, r)).collect();
    let entries: Vec<(i64, ItemRow)> =
        entries.into_iter().filter_map(|(entry_id, item)| rows.get(&item).map(|r| (entry_id, r.clone()))).collect();
    Ok(Some(dto_of(row, entries)))
}

/// A playlist and its visible entries, if it belongs to the user; with every
/// entry's synopsis when `summaries` (see `ItemRow::to_summary_with`).
pub async fn get(
    db: &Db,
    user: &AuthUser,
    id: i64,
    summaries: bool,
) -> sqlx::Result<Option<(PlaylistDto, Vec<EntryDto>)>> {
    Ok(get_rows(db, user, id).await?.map(|(dto, rows)| {
        let entries =
            rows.iter().map(|(entry_id, row)| EntryDto { entry_id: *entry_id, item: row.to_summary_with(summaries) });
        (dto, entries.collect())
    }))
}

pub async fn create(db: &Db, user: &AuthUser, title: &str, kind: Kind) -> sqlx::Result<i64> {
    let now = now();
    sqlx::query_scalar(
        "INSERT INTO playlists (user_id, title, kind, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(user.id)
    .bind(title)
    .bind(kind.as_str())
    .bind(now)
    .bind(now)
    .fetch_one(&db.write)
    .await
}

/// A smart playlist of `library_id`, filled by `filter`.
pub async fn create_smart(
    db: &Db,
    user: &AuthUser,
    title: &str,
    kind: Kind,
    library_id: i64,
    filter: &str,
) -> sqlx::Result<i64> {
    let id = create(db, user, title, kind).await?;
    sqlx::query("UPDATE playlists SET library_id = ?, smart_filter = ? WHERE id = ?")
        .bind(library_id)
        .bind(filter)
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(id)
}

/// A new filter for the user's smart playlist; whether there was one to change.
pub async fn set_smart_filter(db: &Db, user: &AuthUser, id: i64, filter: &str) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE playlists SET smart_filter = ?, updated_at = ? WHERE id = ? AND user_id = ? AND smart_filter IS NOT NULL",
    )
    .bind(filter)
    .bind(now())
    .bind(id)
    .bind(user.id)
    .execute(&db.write)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn rename(db: &Db, user: &AuthUser, id: i64, title: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE playlists SET title = ?, updated_at = ? WHERE id = ? AND user_id = ?")
        .bind(title)
        .bind(now())
        .bind(id)
        .bind(user.id)
        .execute(&db.write)
        .await?
        .rows_affected();
    Ok(done > 0)
}

pub async fn delete(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM playlists WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&db.write)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Append items (containers expand to their leaves). Returns how many entries
/// were added, or an error when they don't fit the playlist's kind.
pub async fn add(db: &Db, user: &AuthUser, id: i64, item_ids: &[i64]) -> Result<u64, AppError> {
    let row = owned(db, user, id).await?.ok_or_else(|| AppError::not_found("playlist not found"))?;
    if row.smart_filter.is_some() {
        return Err(AppError::bad_request("a smart playlist is filled by its filter"));
    }
    let kind = Kind::parse(&row.kind).unwrap_or(Kind::Video);
    let leaves = expand(db, user, item_ids).await?;
    if leaves.iter().any(|l| Kind::of_item(&l.kind) != Some(kind)) {
        return Err(AppError::bad_request(match kind {
            Kind::Video => "only movies and episodes can go in a video playlist",
            Kind::Audio => "only music can go in a music playlist",
        }));
    }
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM playlist_items WHERE playlist_id = ?")
        .bind(id)
        .fetch_one(&db.read)
        .await?;
    if existing + leaves.len() as i64 > MAX_ENTRIES {
        return Err(AppError::bad_request(format!("a playlist holds at most {MAX_ENTRIES} items")));
    }
    let now = now();
    let mut tx = db.write.begin().await?;
    let next: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(position) + 1, 0) FROM playlist_items WHERE playlist_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    for (offset, leaf) in leaves.iter().enumerate() {
        sqlx::query("INSERT INTO playlist_items (playlist_id, item_id, position, added_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(leaf.id)
            .bind(next + offset as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?").bind(now).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(leaves.len() as u64)
}

/// Appends `item_ids` to playlist `id` in their order, as the Plex import
/// brings a playlist: those of its kind that play (movies and episodes, or
/// tracks), others left out, up to the most a playlist holds. How many were
/// added.
pub async fn append_leaves(db: &Db, id: i64, item_ids: &[i64]) -> sqlx::Result<u64> {
    let mut tx = db.write.begin().await?;
    let Some(kind) = sqlx::query_scalar::<_, String>("SELECT kind FROM playlists WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(0);
    };
    let kinds = if kind == "audio" { "('track')" } else { "('movie', 'episode')" };
    let (mut count, mut next): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COALESCE(MAX(position) + 1, 0) FROM playlist_items WHERE playlist_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let now = now();
    let mut added = 0;
    // `kinds` is one of the two lists above.
    let sql = format!(
        "INSERT INTO playlist_items (playlist_id, item_id, position, added_at)
         SELECT ?, id, ?, ? FROM items WHERE id = ? AND kind IN {kinds}"
    );
    for item in item_ids {
        if count >= MAX_ENTRIES {
            break;
        }
        let done = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .bind(next)
            .bind(now)
            .bind(item)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        count += done as i64;
        next += done as i64;
        added += done;
    }
    if added > 0 {
        sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?").bind(now).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(added)
}

/// Puts the leaves of `item_ids` in place of the entries the user sees, in
/// that order, as apps that edit a playlist as a whole send it: entries the
/// user can't see stay, after them. Refused, changing nothing, as [`add`]
/// refuses.
pub async fn replace(db: &Db, user: &AuthUser, id: i64, item_ids: &[i64]) -> Result<u64, AppError> {
    let row = owned(db, user, id).await?.ok_or_else(|| AppError::not_found("playlist not found"))?;
    if row.smart_filter.is_some() {
        return Err(AppError::bad_request("a smart playlist is filled by its filter"));
    }
    let kind = Kind::parse(&row.kind).unwrap_or(Kind::Video);
    let leaves = expand(db, user, item_ids).await?;
    if leaves.iter().any(|l| Kind::of_item(&l.kind) != Some(kind)) {
        return Err(AppError::bad_request(match kind {
            Kind::Video => "only movies and episodes can go in a video playlist",
            Kind::Audio => "only music can go in a music playlist",
        }));
    }
    let seen: HashSet<i64> =
        get_rows(db, user, id).await?.map(|(_, entries)| entries).unwrap_or_default().iter().map(|(e, _)| *e).collect();
    let hidden: Vec<i64> = entry_order(db, id).await?.into_iter().filter(|e| !seen.contains(e)).collect();
    if (hidden.len() + leaves.len()) as i64 > MAX_ENTRIES {
        return Err(AppError::bad_request(format!("a playlist holds at most {MAX_ENTRIES} items")));
    }
    let now = now();
    let mut tx = db.write.begin().await?;
    for entry in &seen {
        sqlx::query("DELETE FROM playlist_items WHERE id = ? AND playlist_id = ?")
            .bind(entry)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    for (position, leaf) in leaves.iter().enumerate() {
        sqlx::query("INSERT INTO playlist_items (playlist_id, item_id, position, added_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(leaf.id)
            .bind(position as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    for (offset, entry) in hidden.iter().enumerate() {
        sqlx::query("UPDATE playlist_items SET position = ? WHERE id = ? AND playlist_id = ?")
            .bind((leaves.len() + offset) as i64)
            .bind(entry)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?").bind(now).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(leaves.len() as u64)
}

pub async fn remove_entry(db: &Db, user: &AuthUser, id: i64, entry_id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "DELETE FROM playlist_items WHERE id = ? AND playlist_id = (SELECT id FROM playlists WHERE id = ? AND user_id = ?)",
    )
    .bind(entry_id)
    .bind(id)
    .bind(user.id)
    .execute(&db.write)
    .await?;
    if done.rows_affected() > 0 {
        sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&db.write).await?;
    }
    Ok(done.rows_affected() > 0)
}

/// `wanted` (those present in `current`, once each) followed by the rest of
/// `current` in its existing order.
pub(crate) fn merged_order(current: &[i64], wanted: &[i64]) -> Vec<i64> {
    let present: HashSet<i64> = current.iter().copied().collect();
    let mut seen = HashSet::new();
    let mut order: Vec<i64> = wanted.iter().copied().filter(|id| present.contains(id) && seen.insert(*id)).collect();
    order.extend(current.iter().copied().filter(|id| !seen.contains(id)));
    order
}

/// Entry ids of a playlist in order, hidden ones included.
async fn entry_order(db: &Db, id: i64) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar("SELECT id FROM playlist_items WHERE playlist_id = ? ORDER BY position, id")
        .bind(id)
        .fetch_all(&db.read)
        .await
}

/// New order: the listed entries first, in that order, then any others.
pub async fn reorder(db: &Db, user: &AuthUser, id: i64, entry_ids: &[i64]) -> sqlx::Result<bool> {
    if owned(db, user, id).await?.is_none() {
        return Ok(false);
    }
    let order = merged_order(&entry_order(db, id).await?, entry_ids);
    write_order(db, id, &order).await?;
    Ok(true)
}

/// Move an entry right after another one, or to the top (`after: None`).
pub async fn move_entry(db: &Db, user: &AuthUser, id: i64, entry_id: i64, after: Option<i64>) -> sqlx::Result<bool> {
    if owned(db, user, id).await?.is_none() {
        return Ok(false);
    }
    let mut order = entry_order(db, id).await?;
    let Some(from) = order.iter().position(|e| *e == entry_id) else { return Ok(false) };
    order.remove(from);
    let to = match after {
        Some(after) => match order.iter().position(|e| *e == after) {
            Some(at) => at + 1,
            None => return Ok(false),
        },
        None => 0,
    };
    order.insert(to, entry_id);
    write_order(db, id, &order).await?;
    Ok(true)
}

async fn write_order(db: &Db, id: i64, order: &[i64]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    for (position, entry) in order.iter().enumerate() {
        sqlx::query("UPDATE playlist_items SET position = ? WHERE id = ? AND playlist_id = ?")
            .bind(position as i64)
            .bind(entry)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&mut *tx).await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_of_items() {
        assert_eq!(Kind::of_item("show"), Some(Kind::Video));
        assert_eq!(Kind::of_item("episode"), Some(Kind::Video));
        assert_eq!(Kind::of_item("album"), Some(Kind::Audio));
        assert_eq!(Kind::of_item("unknown"), None);
        assert_eq!(Kind::parse("audio"), Some(Kind::Audio));
        assert_eq!(Kind::parse("photo"), None);
    }

    #[test]
    fn merged_order_keeps_unlisted_and_ignores_strangers() {
        assert_eq!(merged_order(&[1, 2, 3, 4], &[3, 1]), vec![3, 1, 2, 4]);
        assert_eq!(merged_order(&[1, 2, 3], &[2, 9, 2, 3, 2]), vec![2, 3, 1]);
        assert_eq!(merged_order(&[1, 2], &[]), vec![1, 2]);
    }
}
