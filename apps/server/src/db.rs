//! SQLite access. Writes go through a single-connection pool so they never
//! contend for the write lock; reads use a separate read-only pool (WAL mode
//! lets them run concurrently with the writer).
//!
//! Whoever holds the writer holds up every other write (progress reports,
//! logins, settings), and those fail after the 60 s acquire timeout: work out
//! what to write first (reading through `read`, computing in
//! `spawn_blocking`), then write it in short transactions, as the scanner
//! does.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};

/// The schema, brought up to date when the database is opened.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Db {
    pub read: SqlitePool,
    pub write: SqlitePool,
    /// What search keeps in memory of the library (see [`crate::search`]).
    pub search: Arc<crate::search::Index>,
}

impl Db {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let base = SqliteConnectOptions::from_str("sqlite://")?
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(30))
            .foreign_keys(true)
            .pragma("cache_size", "-32768")
            .pragma("mmap_size", "268435456")
            .pragma("temp_store", "memory");

        let write = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(60))
            .connect_with(base.clone())
            .await?;
        backup_before_upgrade(&write, path).await?;
        MIGRATOR.run(&write).await?;

        let readers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(4, 16) as u32;
        let read = SqlitePoolOptions::new()
            .max_connections(readers)
            .acquire_timeout(Duration::from_secs(30))
            .connect_with(base.read_only(true))
            .await?;
        Ok(Self { read, write, search: Arc::default() })
    }

    /// Refresh query planner statistics (cheap; run after large scans).
    pub async fn optimize(&self) {
        let _ = sqlx::query("PRAGMA optimize").execute(&self.write).await;
    }

    pub async fn close(&self) {
        self.read.close().await;
        self.write.close().await;
    }
}

/// Copies an existing database to `backups/` before migrations change it:
/// nothing else could undo a migration that went wrong. pley doesn't upgrade
/// a database it could not copy.
async fn backup_before_upgrade(write: &SqlitePool, path: &Path) -> anyhow::Result<()> {
    let migrated: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(write)
    .await?;
    if !migrated {
        // A new database: nothing to keep.
        return Ok(());
    }
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success").fetch_all(write).await?;
    let pending = MIGRATOR.iter().any(|m| m.migration_type.is_up_migration() && !applied.contains(&m.version));
    if applied.is_empty() || !pending {
        return Ok(());
    }
    let dir = path.parent().unwrap_or(Path::new("")).join("backups");
    // Logged by write_checked; the name says why it was made.
    crate::backup::write_checked(write, &dir, crate::backup::BEFORE_UPGRADE)
        .await
        .context("could not back up the database before upgrading it")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::ConnectOptions;

    use super::*;

    async fn versions(path: &Path) -> Vec<i64> {
        let mut conn = SqliteConnectOptions::new().filename(path).read_only(true).connect().await.unwrap();
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version").fetch_all(&mut conn).await.unwrap()
    }

    #[tokio::test]
    async fn databases_are_backed_up_before_an_upgrade_and_only_then() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        let backups = dir.path().join("backups");
        // A database made by an older version: every migration but the last.
        let older = dir.path().join("older");
        std::fs::create_dir(&older).unwrap();
        let mut files: Vec<_> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        for file in &files[..files.len() - 1] {
            std::fs::copy(file, older.join(file.file_name().unwrap())).unwrap();
        }
        let options = SqliteConnectOptions::new().filename(&path).create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        Migrator::new(older.as_path()).await.unwrap().run(&pool).await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('probe', 'before')").execute(&pool).await.unwrap();
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        let latest: Vec<i64> = MIGRATOR.iter().map(|m| m.version).collect();
        assert_eq!(versions(&path).await, latest, "upgraded");
        let copies: Vec<_> = std::fs::read_dir(&backups).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(copies.len(), 1, "{copies:?}");
        assert!(copies[0].to_string_lossy().ends_with("-before-upgrade.db"), "{copies:?}");
        assert_eq!(versions(&copies[0]).await, latest[..latest.len() - 1], "the copy is the old database");
        let mut copy = SqliteConnectOptions::new().filename(&copies[0]).read_only(true).connect().await.unwrap();
        let probe: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'probe'").fetch_one(&mut copy).await.unwrap();
        assert_eq!(probe, "before");
        db.close().await;

        // Up to date: no copy. A new database: none either.
        Db::open(&path).await.unwrap().close().await;
        Db::open(&dir.path().join("new.db")).await.unwrap().close().await;
        assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 1);
    }

    /// A database at `path` as an older version left it: only the
    /// migrations numbered below `version`.
    async fn database_before(version: i64, path: &Path) -> SqlitePool {
        let older = path.with_extension("migrations");
        std::fs::create_dir(&older).unwrap();
        for file in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap().flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            let number: i64 = name.split('_').next().and_then(|n| n.parse().ok()).expect(&name);
            if number < version {
                std::fs::copy(file.path(), older.join(&name)).unwrap();
            }
        }
        let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        Migrator::new(older.as_path()).await.unwrap().run(&pool).await.unwrap();
        pool
    }

    /// Audiobook libraries come with a table of libraries made again: what
    /// every library holds stays, and so does a library's id after it is
    /// deleted (`library_ids`).
    #[tokio::test]
    async fn libraries_and_what_they_hold_stay_through_the_audiobooks_upgrade() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        let pool = database_before(55, &path).await;
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        for sql in [
            "INSERT INTO libraries (id, name, kind, paths, created_at, scanned_at, position, labels_pending)
             VALUES (3, 'Music', 'music', '[\"/m\"]', 10, 20, 1, 0), (5, 'Films', 'movies', '[]', 11, NULL, 0, 1)",
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (7, 5, 'movie', 'movie:a film:2001', 'A Film', 'film', 0, 0)",
            "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, added_at, updated_at)
             VALUES (9, 7, 5, '/films/A Film.mkv', 1, 0, 0, 0)",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&pool).await.unwrap();
        }
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        // A book's writers, narrators and series have their columns.
        sqlx::query("SELECT authors, narrators, series, series_index FROM items").fetch_all(&db.read).await.unwrap();
        type Library = (i64, String, String, String, i64, Option<i64>, i64, bool);
        let libraries: Vec<Library> = sqlx::query_as(
            "SELECT id, name, kind, paths, created_at, scanned_at, position, labels_pending FROM libraries ORDER BY id",
        )
        .fetch_all(&db.read)
        .await
        .unwrap();
        assert_eq!(
            libraries,
            [
                (3, "Music".into(), "music".into(), r#"["/m"]"#.into(), 10, Some(20), 1, false),
                (5, "Films".into(), "movies".into(), "[]".into(), 11, None, 0, true),
            ]
        );
        let held: (i64, i64) =
            sqlx::query_as("SELECT (SELECT COUNT(*) FROM items), (SELECT COUNT(*) FROM media_files)")
                .fetch_one(&db.read)
                .await
                .unwrap();
        assert_eq!(held, (1, 1), "nothing went with the old table");
        let broken: Vec<(String,)> = sqlx::query_as("PRAGMA foreign_key_check").fetch_all(&db.write).await.unwrap();
        assert!(broken.is_empty(), "{broken:?}");

        // The new kind is taken, an unknown one still isn't, and the
        // libraries' foreign keys still hold: a deleted library takes what
        // it held, and its id isn't given again.
        sqlx::query("INSERT INTO libraries (name, kind, created_at) VALUES ('Books', 'audiobooks', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        assert!(
            sqlx::query("INSERT INTO libraries (name, kind, created_at) VALUES ('Pics', 'photos', 0)")
                .execute(&db.write)
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM libraries WHERE id IN (5, 6)").execute(&db.write).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items").fetch_one(&db.read).await.unwrap();
        assert_eq!(left, 0);
        let last: i64 = sqlx::query_scalar("SELECT last FROM library_ids").fetch_one(&db.read).await.unwrap();
        assert_eq!(last, 6);
        db.close().await;
    }

    /// The audiobooks migration runs outside sqlx's transaction, which
    /// records it only after it ran: a server stopped in between runs it
    /// again as it starts, and starts.
    #[tokio::test]
    async fn the_audiobooks_upgrade_runs_again_after_a_crash() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        let pool = database_before(56, &path).await;
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 55").execute(&pool).await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.unwrap();
        for sql in [
            "INSERT INTO libraries (id, name, kind, created_at) VALUES (4, 'Books', 'audiobooks', 0)",
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (8, 4, 'audiobook', 'audiobook:a book', 'A Book', 'book', 0, 0)",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&pool).await.unwrap();
        }
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        let latest: Vec<i64> = MIGRATOR.iter().map(|m| m.version).collect();
        assert_eq!(versions(&path).await, latest);
        let held: (i64, i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM libraries), (SELECT COUNT(*) FROM items)")
            .fetch_one(&db.read)
            .await
            .unwrap();
        assert_eq!(held, (1, 1));
        sqlx::query("UPDATE items SET authors = '[\"A Writer\"]' WHERE id = 8").execute(&db.write).await.unwrap();
        db.close().await;
    }

    async fn server_settings(db: &Db) -> String {
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'server'").fetch_one(&db.read).await.unwrap()
    }

    #[tokio::test]
    async fn single_sign_on_leaves_nothing_behind_after_an_upgrade() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        // A version with single sign-on: an account it made, linked to an
        // identity at the provider, and its settings with the client secret.
        let pool = database_before(22, &path).await;
        sqlx::query("INSERT INTO users (id, username, password_hash, created_at) VALUES (1, 'ann', '!sso', 0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO user_identities (issuer, subject, user_id, email, created_at)
             VALUES ('https://idp', 'ann-at-idp', 1, 'ann@example.com', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let settings = serde_json::json!({
            "server_name": "Attic",
            "public_url": "https://p",
            "sso_enabled": true,
            "sso_name": "Idp",
            "sso_issuer": "https://idp",
            "sso_client_id": "pley",
            "sso_client_secret": "s3cret",
            "sso_new_accounts": true,
            "sso_group": "pley-users",
        });
        sqlx::query("INSERT INTO settings (key, value) VALUES ('server', ?)")
            .bind(settings.to_string())
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        let left: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE name IN ('user_identities', 'user_identities_user')",
        )
        .fetch_all(&db.read)
        .await
        .unwrap();
        assert!(left.is_empty(), "{left:?}");
        let value = server_settings(&db).await;
        let kept: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert_eq!(kept, serde_json::json!({"server_name": "Attic"}), "the other settings stay");
        assert!(!value.contains("s3cret"), "{value}");
        // The account stays, without a password until an administrator gives it one.
        let hash: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = 1").fetch_one(&db.read).await.unwrap();
        assert_eq!(hash, "!sso");
        db.close().await;

        // Settings that aren't JSON don't stop the upgrade, and stay as they were.
        let path = dir.path().join("unreadable.db");
        let pool = database_before(22, &path).await;
        sqlx::query("INSERT INTO settings (key, value) VALUES ('server', 'not json')").execute(&pool).await.unwrap();
        pool.close().await;
        let db = Db::open(&path).await.unwrap();
        assert_eq!(server_settings(&db).await, "not json");
        db.close().await;
    }

    #[tokio::test]
    async fn the_apps_sign_ins_move_with_their_client_ids() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        let pool = database_before(35, &path).await;
        sqlx::query("INSERT INTO users (id, username, password_hash, created_at) VALUES (1, 'ann', '!', 0)")
            .execute(&pool)
            .await
            .unwrap();
        for (id, client) in [
            (1, "https://hhaidar.github.io/pley/oauth/android.json"),
            (2, "https://hhaidar.github.io/pley/oauth/webos.json"),
            (3, "https://apps.example.com/pley.json"),
        ] {
            sqlx::query(
                "INSERT INTO app_grants (id, user_id, approved_by, client_id, scope, created_at, last_used_at,
                 expires_at) VALUES (?, 1, 1, ?, 'library', 0, 0, 0)",
            )
            .bind(id)
            .bind(client)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        let clients: Vec<String> =
            sqlx::query_scalar("SELECT client_id FROM app_grants ORDER BY id").fetch_all(&db.read).await.unwrap();
        assert_eq!(
            clients,
            [crate::oauth::clients::ANDROID_ID, crate::oauth::clients::WEBOS_ID, "https://apps.example.com/pley.json"]
        );
        db.close().await;
    }

    /// Probes now find Dolby Vision in the frames of 10-bit HEVC whose file
    /// doesn't name it: an upgrade has those files probed again, and only
    /// those.
    #[tokio::test]
    async fn hevc_that_may_hide_dolby_vision_is_probed_again() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        let pool = database_before(44, &path).await;
        sqlx::query("INSERT INTO libraries (id, name, kind, created_at) VALUES (1, 'Movies', 'movies', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let video = |codec: &str, range: Option<&str>, bits: Option<i64>| {
            serde_json::json!({"streams": [{"index": 0, "kind": "video", "codec": codec,
                "dynamic_range": range, "bit_depth": bits}]})
            .to_string()
        };
        let files = [
            ("hdr10", Some(video("hevc", Some("hdr10"), Some(10)))),
            ("hdr10plus", Some(video("hevc", Some("hdr10plus"), Some(10)))),
            ("hlg", Some(video("hevc", Some("hlg"), Some(10)))),
            ("sdr 10-bit", Some(video("hevc", Some("sdr"), Some(10)))),
            ("no range or depth", Some(video("hevc", None, None))),
            ("named", Some(video("hevc", Some("dolby_vision"), Some(10)))),
            ("8-bit", Some(video("hevc", Some("sdr"), Some(8)))),
            ("avc", Some(video("h264", Some("sdr"), Some(8)))),
            ("unreadable", None),
        ];
        for (id, (name, info)) in files.iter().enumerate() {
            sqlx::query(
                "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
                 VALUES (?, 1, 'movie', ?, ?, ?, 0, 0)",
            )
            .bind(id as i64)
            .bind(name)
            .bind(name)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO media_files (item_id, library_id, path, size, mtime, media_info, probed, added_at, updated_at)
                 VALUES (?, 1, ?, 1, 0, ?, 1, 0, 0)",
            )
            .bind(id as i64)
            .bind(format!("/m/{name}.mkv"))
            .bind(info)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        let again: Vec<String> = sqlx::query_scalar("SELECT path FROM media_files WHERE probed = 0 ORDER BY id")
            .fetch_all(&db.read)
            .await
            .unwrap();
        assert_eq!(
            again,
            ["/m/hdr10.mkv", "/m/hdr10plus.mkv", "/m/hlg.mkv", "/m/sdr 10-bit.mkv", "/m/no range or depth.mkv"]
        );
        db.close().await;
    }

    /// Home's and Recommended's built-in rows become collections Pley makes
    /// in each movie and TV library, in the order Home showed the libraries;
    /// TMDB's collections are Franchises.
    #[tokio::test]
    async fn the_built_in_rows_become_collections() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        let pool = database_before(57, &path).await;
        for sql in [
            "INSERT INTO libraries (id, name, kind, paths, created_at, position, labels_pending)
             VALUES (3, 'Music', 'music', '[]', 0, 0, 0), (5, 'Films', 'movies', '[]', 0, 1, 0),
                    (6, 'Shows', 'shows', '[]', 0, 1, 0)",
            "INSERT INTO collections (id, library_id, title, sort_title, tmdb_id, created_at, updated_at)
             VALUES (1, 5, 'Alien Collection', 'alien collection', 8091, 0, 0),
                    (2, 5, 'Mine', 'mine', NULL, 0, 0)",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&pool).await.unwrap();
        }
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        type Made = (i64, String, Option<String>, Option<String>, Option<i64>, Option<i64>);
        let made: Vec<Made> = sqlx::query_as(
            "SELECT c.library_id, c.title, c.builtin, k.key, c.home_position, c.recommended_position
             FROM collections c LEFT JOIN collection_categories k ON k.id = c.category_id
             ORDER BY c.library_id, c.recommended_position IS NULL, c.recommended_position, c.id",
        )
        .fetch_all(&db.read)
        .await
        .unwrap();
        let highlights = Some("highlights".to_owned());
        let row = |library: i64, title: &str, builtin: &str, home: Option<i64>, at: i64| {
            (library, title.to_owned(), Some(builtin.to_owned()), highlights.clone(), home, Some(at))
        };
        // Not a Recommended row: it would repeat Top Rated.
        let unwatched = |library: i64| {
            (library, "Top Unwatched".to_owned(), Some("top_unwatched".to_owned()), highlights.clone(), None, None)
        };
        assert_eq!(
            made,
            [
                row(5, "Recently Added", "recently_added", Some(1), 0),
                row(5, "Recently Released", "recently_released", None, 1),
                row(5, "Top Rated", "top_rated", None, 2),
                (5, "Alien Collection".into(), None, Some("franchises".into()), None, None),
                (5, "Mine".into(), None, None, None, None),
                unwatched(5),
                row(6, "Recently Added Shows", "recently_added", Some(2), 0),
                row(6, "Recently Released", "recently_released", None, 1),
                row(6, "Top Rated", "top_rated", None, 2),
                unwatched(6),
            ]
        );
        let music: Option<i64> =
            sqlx::query_scalar("SELECT home_position FROM libraries WHERE id = 3").fetch_one(&db.read).await.unwrap();
        assert_eq!(music, Some(0));
        db.close().await;
    }
}
