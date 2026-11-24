//! What people watched of movies and episodes that left the library comes
//! back with them.
//!
//! A movie or an episode goes when a scan finds none of its files. A file
//! renamed or moved keeps its title's state on the way (see the scanner's
//! `relink`), but a copy replaced by one named differently is another file,
//! and a film deleted and added again later is another title. So before
//! such a title goes, [`keep`] keeps each person's state of it (resume
//! point, watched, plays, rating) and its plays in history, by the ids that
//! tell it apart: a movie's TMDB and IMDb ids, an episode's show's TMDB or
//! TheTVDB id with its season and number. After each scan, once new titles
//! are matched, [`give_back`] gives it to the movie or episode of the same
//! library with those ids, as merging two copies of a movie would. What
//! nothing claims within [`KEPT_FOR`] is forgotten.

use sqlx::SqliteConnection;

use super::merge::state_joined;
use crate::db::Db;
use crate::util::now;

/// How long the state of a title that went is kept for one that comes.
pub const KEPT_FOR: i64 = 180 * 24 * 3600;

/// Keeps people's state of those of `items` (movies and episodes about to
/// go, as a JSON array) that have some and ids to find them by.
pub(super) async fn keep(conn: &mut SqliteConnection, items: &str, at: i64) -> sqlx::Result<()> {
    let kept = sqlx::query(
        "INSERT INTO gone_items (item_id, library_id, kind, tmdb_id, imdb_id, show_tmdb_id, show_tvdb_id, season,
                                 episode, gone_at)
         SELECT i.id, i.library_id, i.kind, i.tmdb_id, NULLIF(i.imdb_id, ''), sh.tmdb_id, sh.tvdb_id, s.idx, i.idx, ?2
         FROM items i
         LEFT JOIN items s ON s.id = i.parent_id AND i.kind = 'episode'
         LEFT JOIN items sh ON sh.id = i.grandparent_id AND i.kind = 'episode'
         WHERE i.id IN (SELECT value FROM json_each(?1)) AND i.kind IN ('movie', 'episode')
           AND (EXISTS (SELECT 1 FROM user_item_state u WHERE u.item_id = i.id
                          AND (u.watched = 1 OR u.position_ms > 0 OR u.play_count > 0 OR u.rating IS NOT NULL))
                OR EXISTS (SELECT 1 FROM play_history h WHERE h.item_id = i.id))
           AND CASE i.kind
                 WHEN 'movie' THEN i.tmdb_id IS NOT NULL OR NULLIF(i.imdb_id, '') IS NOT NULL
                 ELSE (sh.tmdb_id IS NOT NULL OR sh.tvdb_id IS NOT NULL) AND s.idx IS NOT NULL AND i.idx IS NOT NULL
               END",
    )
    .bind(items)
    .bind(at)
    .execute(&mut *conn)
    .await?;
    if kept.rows_affected() == 0 {
        return Ok(());
    }
    // The items are still there: those kept are the ones in `items`.
    for sql in [
        "INSERT INTO gone_item_state (item_id, user_id, position_ms, watched, play_count, last_played_at, rating,
                                      rated_at, updated_at)
         SELECT u.item_id, u.user_id, u.position_ms, u.watched, u.play_count, u.last_played_at, u.rating, u.rated_at,
                u.updated_at
         FROM user_item_state u JOIN gone_items g ON g.item_id = u.item_id
         WHERE u.item_id IN (SELECT value FROM json_each(?1))
           AND (u.watched = 1 OR u.position_ms > 0 OR u.play_count > 0 OR u.rating IS NOT NULL)",
        "INSERT INTO gone_play_history (item_id, user_id, viewed_at, device)
         SELECT h.item_id, h.user_id, h.viewed_at, h.device
         FROM play_history h JOIN gone_items g ON g.item_id = h.item_id
         WHERE h.item_id IN (SELECT value FROM json_each(?1))",
    ] {
        sqlx::query(sql).bind(items).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Gives what was kept of titles that went to the movies and episodes of
/// `library_id` that have their ids (a movie's TMDB id, or its IMDb id when
/// it had none; the latest added, when several do), and forgets what was
/// kept longer than [`KEPT_FOR`]. Returns how many titles got some.
pub async fn give_back(db: &Db, library_id: i64) -> sqlx::Result<u64> {
    sqlx::query("DELETE FROM gone_items WHERE library_id = ? AND gone_at < ?")
        .bind(library_id)
        .bind(now() - KEPT_FOR)
        .execute(&db.write)
        .await?;
    // Found by their ids alone (`+` keeps SQLite from going through every
    // movie or episode of the library instead), then by their seasons'
    // and numbers.
    let found: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT gone, item FROM (
            SELECT g.item_id AS gone, CASE g.kind
                WHEN 'movie' THEN (
                    SELECT MAX(i.id) FROM items i
                    WHERE i.id IN (SELECT id FROM items WHERE tmdb_id = g.tmdb_id
                                   UNION ALL SELECT id FROM items WHERE imdb_id = g.imdb_id AND g.tmdb_id IS NULL)
                      AND +i.library_id = g.library_id AND +i.kind = 'movie')
                ELSE (
                    SELECT MAX(e.id) FROM items sh
                    JOIN items s ON s.parent_id = sh.id AND s.idx = g.season AND +s.kind = 'season'
                    JOIN items e ON e.parent_id = s.id AND e.idx = g.episode AND +e.kind = 'episode'
                    WHERE sh.id IN (SELECT id FROM items WHERE tmdb_id = g.show_tmdb_id
                                    UNION ALL SELECT id FROM items WHERE tvdb_id = g.show_tvdb_id)
                      AND +sh.library_id = g.library_id AND +sh.kind = 'show')
                END AS item
            FROM gone_items g WHERE g.library_id = ?1)
         WHERE item IS NOT NULL",
    )
    .bind(library_id)
    .fetch_all(&db.read)
    .await?;
    for chunk in found.chunks(500) {
        let mut tx = db.write.begin().await?;
        for &(gone, item) in chunk {
            give(&mut tx, gone, item).await?;
        }
        tx.commit().await?;
    }
    if !found.is_empty() {
        tracing::info!("library {library_id}: {} titles got back what was watched of them before", found.len());
    }
    Ok(found.len() as u64)
}

/// Gives what was kept of title `gone` to item `into`, and forgets it.
async fn give(conn: &mut SqliteConnection, gone: i64, into: i64) -> sqlx::Result<()> {
    let statements = [
        concat!(
            "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at,
                                          updated_at, rating, rated_at)
             SELECT user_id, ?1, position_ms, watched, play_count, last_played_at, updated_at, rating, rated_at
             FROM gone_item_state WHERE item_id = ?2 ",
            state_joined!()
        ),
        // Unless that play is there already (imported since, say).
        "INSERT INTO play_history (user_id, item_id, viewed_at, device)
         SELECT g.user_id, ?1, g.viewed_at, g.device FROM gone_play_history g
         WHERE g.item_id = ?2 AND NOT EXISTS (SELECT 1 FROM play_history h
                                              WHERE h.item_id = ?1 AND h.user_id = g.user_id
                                                AND h.viewed_at = g.viewed_at)",
        "DELETE FROM gone_items WHERE item_id = ?2",
    ];
    for sql in statements {
        sqlx::query(sql).bind(into).bind(gone).execute(&mut *conn).await?;
    }
    Ok(())
}
