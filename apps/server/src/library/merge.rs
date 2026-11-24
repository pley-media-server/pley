//! Movies of a library matched to the same TMDB entry are one movie, with
//! each copy a version.
//!
//! The scanner tells movies apart by their files' names: their TMDB id
//! (`{tmdb-157336}`), else their title and year. When TMDB corrects a film's
//! year or title, a download manager names the copies it fetches after with
//! the new ones, and the copies would stay two movies that metadata matches
//! to the same title. [`same_movies`] makes them one: the files and bonus
//! material go to the movie added first (one keyed by its TMDB id before
//! it), with what people did with either (watched, ratings, My List,
//! playlists, collections, labels, hand edits, track choices). The keys the
//! other's files made are kept in `item_aliases`, so that a later scan of
//! them finds the movie they went into.
//!
//! [`split`] undoes it when someone fixes the movie's match: each file goes
//! back to the movie its own name makes, to be matched on its own. Those
//! movies stay apart from it (`movies_apart`) even when matched to its title
//! again: an automatic match that put two films together would do it again
//! after the split. Fixing either's match by hand forgets that.

use std::path::Path;

use sqlx::SqliteConnection;

use super::LibraryRow;
use super::scanner::{movie_for_file, movie_key};
use crate::db::Db;
use crate::util::now;

/// Merge the movies of a library that share a TMDB id, except those kept
/// apart. Returns each movie merged with the movie it went into.
pub async fn same_movies(db: &Db, library_id: i64) -> sqlx::Result<Vec<(i64, i64)>> {
    // By TMDB id, the movie keyed by it first, then the first added.
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT tmdb_id, id FROM items
         WHERE library_id = ?1 AND kind = 'movie' AND tmdb_id IN (
             SELECT tmdb_id FROM items WHERE library_id = ?1 AND kind = 'movie' AND tmdb_id IS NOT NULL
             GROUP BY tmdb_id HAVING COUNT(*) > 1)
         ORDER BY tmdb_id, key NOT LIKE 'movie:tmdb:%', added_at, id",
    )
    .bind(library_id)
    .fetch_all(&db.read)
    .await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let mut merged = Vec::new();
    let mut tx = db.write.begin().await?;
    // The movies of the TMDB id at hand that stay, which the next goes into:
    // the first it may go into, or it stays too.
    let mut tmdb = None;
    let mut kept: Vec<i64> = Vec::new();
    for (tmdb_id, id) in rows {
        if tmdb != Some(tmdb_id) {
            tmdb = Some(tmdb_id);
            kept.clear();
        }
        let mut into = None;
        for &movie in &kept {
            if mergeable(&mut tx, library_id, movie, id).await? {
                into = Some(movie);
                break;
            }
        }
        match into {
            Some(into) => {
                merge_into(&mut tx, into, id).await?;
                tracing::info!("merged movie {id} into {into}, both TMDB {tmdb_id}");
                merged.push((id, into));
            }
            None => kept.push(id),
        }
    }
    tx.commit().await?;
    // Hand edits that came with the other copies show.
    let mut into: Vec<i64> = merged.iter().map(|&(_, into)| into).collect();
    into.sort_unstable();
    into.dedup();
    crate::overrides::apply(db, &into).await?;
    Ok(merged)
}

/// Whether movie `from` goes into `into`: still matched to the same title
/// (a match may have changed since the list was read), and not kept apart
/// from it, by its own key or one merged into it.
async fn mergeable(conn: &mut SqliteConnection, library_id: i64, into: i64, from: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM items a JOIN items b ON a.tmdb_id = b.tmdb_id WHERE a.id = ?2 AND b.id = ?3)
            AND NOT EXISTS (SELECT 1 FROM movies_apart p WHERE p.library_id = ?1 AND (
                (p.item_id = ?2 AND p.key IN (SELECT key FROM items WHERE id = ?3
                                              UNION ALL SELECT key FROM item_aliases WHERE item_id = ?3))
                OR (p.item_id = ?3 AND p.key IN (SELECT key FROM items WHERE id = ?2
                                                 UNION ALL SELECT key FROM item_aliases WHERE item_id = ?2))))",
    )
    .bind(library_id)
    .bind(into)
    .bind(from)
    .fetch_one(&mut *conn)
    .await
}

/// Move everything of movie `from` to `into`, then delete `from`. What both
/// have, `into`'s wins, except what adds up: plays, watched, the latest
/// position.
async fn merge_into(conn: &mut SqliteConnection, into: i64, from: i64) -> sqlx::Result<()> {
    let statements = [
        // Its files' keys now find `into`, and so do the keys of the movies
        // that went into it before.
        "INSERT INTO item_aliases (library_id, key, item_id) SELECT library_id, key, ?1 FROM items WHERE id = ?2
         ON CONFLICT(library_id, key) DO UPDATE SET item_id = excluded.item_id",
        "UPDATE item_aliases SET item_id = ?1 WHERE item_id = ?2",
        // What was kept apart from it is from `into` too.
        "INSERT OR IGNORE INTO movies_apart (library_id, key, item_id) SELECT library_id, key, ?1 FROM movies_apart
         WHERE item_id = ?2",
        "INSERT OR IGNORE INTO item_nfo (item_id, path, size, mtime, ids, fields, locked)
         SELECT ?1, path, size, mtime, ids, fields, locked FROM item_nfo WHERE item_id = ?2",
    ];
    for sql in statements {
        sqlx::query(sql).bind(into).bind(from).execute(&mut *conn).await?;
    }
    merge_items(conn, into, from).await
}

/// Move the files and children of item `from`, and what people did with it,
/// to `into`, then delete `from`: watched, ratings, My List, playlists,
/// collections, labels, hand edits, track choices. What both have, `into`'s
/// wins, except what adds up: plays, watched, the latest position.
pub(super) async fn merge_items(conn: &mut SqliteConnection, into: i64, from: i64) -> sqlx::Result<()> {
    let statements = [
        "UPDATE media_files SET item_id = ?1 WHERE item_id = ?2",
        // Its children: a movie's bonus material.
        "UPDATE items SET parent_id = ?1 WHERE parent_id = ?2",
        "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at,
                                      rating, rated_at)
         SELECT user_id, ?1, position_ms, watched, play_count, last_played_at, updated_at, rating, rated_at
         FROM user_item_state WHERE item_id = ?2
         ON CONFLICT(user_id, item_id) DO UPDATE SET
            position_ms = CASE WHEN excluded.updated_at > user_item_state.updated_at
                               THEN excluded.position_ms ELSE user_item_state.position_ms END,
            watched = MAX(user_item_state.watched, excluded.watched),
            play_count = user_item_state.play_count + excluded.play_count,
            last_played_at = CASE WHEN user_item_state.last_played_at IS NULL THEN excluded.last_played_at
                                  WHEN excluded.last_played_at IS NULL THEN user_item_state.last_played_at
                                  ELSE MAX(user_item_state.last_played_at, excluded.last_played_at) END,
            rating = CASE WHEN excluded.rated_at > COALESCE(user_item_state.rated_at, 0)
                          THEN excluded.rating ELSE user_item_state.rating END,
            rated_at = CASE WHEN excluded.rated_at > COALESCE(user_item_state.rated_at, 0)
                            THEN excluded.rated_at ELSE user_item_state.rated_at END,
            updated_at = MAX(user_item_state.updated_at, excluded.updated_at)",
        "UPDATE play_history SET item_id = ?1 WHERE item_id = ?2",
        "INSERT INTO watchlist (user_id, item_id, added_at) SELECT user_id, ?1, added_at FROM watchlist WHERE item_id = ?2
         ON CONFLICT(user_id, item_id) DO UPDATE SET added_at = MIN(watchlist.added_at, excluded.added_at)",
        "INSERT OR IGNORE INTO continue_watching_hidden (user_id, item_id, hidden_at)
         SELECT user_id, ?1, hidden_at FROM continue_watching_hidden WHERE item_id = ?2",
        // In a playlist that has `into` already, `from` goes (with `from`).
        "UPDATE playlist_items SET item_id = ?1
         WHERE item_id = ?2 AND playlist_id NOT IN (SELECT playlist_id FROM playlist_items WHERE item_id = ?1)",
        "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position)
         SELECT collection_id, ?1, position FROM collection_items WHERE item_id = ?2",
        "INSERT OR IGNORE INTO item_labels (item_id, label) SELECT ?1, label FROM item_labels WHERE item_id = ?2",
        "INSERT OR IGNORE INTO stream_preferences (user_id, item_id, audio_language, audio_title, subtitles,
                                                   subtitle_language, subtitle_title, subtitle_forced, updated_at)
         SELECT user_id, ?1, audio_language, audio_title, subtitles, subtitle_language, subtitle_title,
                subtitle_forced, updated_at
         FROM stream_preferences WHERE item_id = ?2",
        "INSERT OR IGNORE INTO item_overrides (item_id, field, value)
         SELECT ?1, field, value FROM item_overrides WHERE item_id = ?2",
        "UPDATE requests SET item_id = ?1 WHERE item_id = ?2",
        // It was in the library since the first of them came.
        "UPDATE items SET added_at = MIN(added_at, (SELECT added_at FROM items WHERE id = ?2)), updated_at = ?3
         WHERE id = ?1",
        // Cast, genres and the rest of what is left of it go with it.
        "DELETE FROM items WHERE id = ?2",
    ];
    let now = now();
    for sql in statements {
        sqlx::query(sql).bind(into).bind(from).bind(now).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Before a movie is matched by hand: what it was kept apart from is
/// forgotten (the person says what it is), its files that its own key
/// doesn't make go back to the movies their names make, kept apart from it,
/// and the keys merged into it are let go. Returns those movies, which have
/// yet to be matched.
pub async fn split(db: &Db, lib: &LibraryRow, movie: i64) -> sqlx::Result<Vec<i64>> {
    let roots = lib.paths();
    let mut tx = db.write.begin().await?;
    let key: Option<String> = sqlx::query_scalar("SELECT key FROM items WHERE id = ? AND kind = 'movie'")
        .bind(movie)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(key) = key else { return Ok(Vec::new()) };
    sqlx::query(
        "DELETE FROM movies_apart WHERE library_id = ?1 AND (item_id = ?2 OR key = ?3
            OR key IN (SELECT key FROM item_aliases WHERE item_id = ?2))",
    )
    .bind(lib.id)
    .bind(movie)
    .bind(&key)
    .execute(&mut *tx)
    .await?;
    let aliases = sqlx::query("DELETE FROM item_aliases WHERE item_id = ?").bind(movie).execute(&mut *tx).await?;
    if aliases.rows_affected() == 0 {
        // Never merged: nothing to split.
        tx.commit().await?;
        return Ok(Vec::new());
    }
    let files: Vec<(i64, String)> = sqlx::query_as("SELECT id, path FROM media_files WHERE item_id = ? ORDER BY id")
        .bind(movie)
        .fetch_all(&mut *tx)
        .await?;
    let rooted =
        |path: &Path| roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.as_os_str().len()).cloned();
    let mut leaving: Vec<(i64, String, std::path::PathBuf)> = Vec::new();
    let mut stays = false;
    for (id, path) in files {
        let path = std::path::PathBuf::from(path);
        let Some(root) = rooted(&path) else {
            stays = true;
            continue;
        };
        if movie_key(&path, &root) == key {
            stays = true;
        } else {
            leaving.push((id, path.to_string_lossy().into_owned(), root));
        }
    }
    // A movie none of whose files make its key (its own copy went) keeps its
    // first one, and with it what people did with it. That file's key stays
    // its alias, or the next scan of the file would make it a movie again
    // and leave this one without files, to be removed.
    if !stays && !leaving.is_empty() {
        let (_, path, root) = leaving.remove(0);
        sqlx::query("INSERT OR REPLACE INTO item_aliases (library_id, key, item_id) VALUES (?, ?, ?)")
            .bind(lib.id)
            .bind(movie_key(Path::new(&path), &root))
            .bind(movie)
            .execute(&mut *tx)
            .await?;
    }
    let mut made = Vec::new();
    for (file, path, root) in leaving {
        let duration: Option<i64> = sqlx::query_scalar("SELECT duration_ms FROM media_files WHERE id = ?")
            .bind(file)
            .fetch_one(&mut *tx)
            .await?;
        let path = Path::new(&path);
        let id = movie_for_file(&mut tx, lib.id, path, &root, duration, false).await?;
        sqlx::query("UPDATE media_files SET item_id = ? WHERE id = ?").bind(id).bind(file).execute(&mut *tx).await?;
        sqlx::query("INSERT OR IGNORE INTO movies_apart (library_id, key, item_id) VALUES (?, ?, ?)")
            .bind(lib.id)
            .bind(movie_key(path, &root))
            .bind(movie)
            .execute(&mut *tx)
            .await?;
        if id != movie && !made.contains(&id) {
            made.push(id);
        }
    }
    tx.commit().await?;
    Ok(made)
}
