//! Database backups and health. A consistent copy of the database is made
//! with `VACUUM INTO` (it reads a snapshot, so the server keeps running and
//! writing) once a day into `<data>/backups`, checked, and the last few are
//! kept. The live database is checked at start; a damaged one is reported in
//! the log and the dashboard with the backups to restore from (see
//! docs/STORAGE.md).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::{ConnectOptions, Connection};

use crate::db::Db;
use crate::util::{now, path_str};

/// Backups kept (oldest removed first).
const KEEP: usize = 3;

/// The end of the names of copies made before an upgrade migrates the
/// database (see `db.rs`). The newest is kept, apart from the others.
pub(crate) const BEFORE_UPGRADE: &str = "-before-upgrade";

#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Health {
    /// `None` until the check at start finished.
    pub ok: Option<bool>,
    /// What SQLite found, when the database is damaged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub checked_at: Option<i64>,
}

pub struct Backups {
    db: Db,
    dir: PathBuf,
    health: Mutex<Health>,
    running: tokio::sync::Mutex<()>,
}

impl Backups {
    pub fn new(db: Db, data_dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            db,
            dir: data_dir.join("backups"),
            health: Mutex::default(),
            running: tokio::sync::Mutex::new(()),
        })
    }

    pub fn health(&self) -> Health {
        self.health.lock().clone()
    }

    /// `PRAGMA quick_check` of the live database.
    pub async fn check(&self) {
        let result: sqlx::Result<Vec<String>> = sqlx::query_scalar("PRAGMA quick_check").fetch_all(&self.db.read).await;
        let problem = match result {
            Ok(rows) if rows.len() == 1 && rows[0] == "ok" => None,
            Ok(rows) => Some(rows.into_iter().take(5).collect::<Vec<_>>().join("; ")),
            Err(err) => Some(err.to_string()),
        };
        if let Some(problem) = &problem {
            let latest = self.list().await.into_iter().next().map(|b| b.path).unwrap_or_else(|| "none".into());
            tracing::error!(
                "the database is damaged ({problem}). Stop Pley and replace pley.db with the latest backup ({latest})."
            );
        }
        *self.health.lock() = Health { ok: Some(problem.is_none()), problem, checked_at: Some(now()) };
    }

    /// Backups, newest first.
    pub async fn list(&self) -> Vec<BackupInfo> {
        list(&self.dir).await
    }

    /// Write a checked copy of the database and drop old ones.
    pub async fn create(&self) -> anyhow::Result<BackupInfo> {
        let _running = self.running.lock().await;
        write_checked(&self.db.read, &self.dir, "").await
    }
}

/// The backups in `dir`, newest first.
async fn list(dir: &Path) -> Vec<BackupInfo> {
    let dir = dir.to_path_buf();
    crate::util::blocking(move || {
        let mut out: Vec<BackupInfo> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_name().to_string_lossy().ends_with(".db"))
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        let created = meta.modified().ok().map(crate::util::system_time_secs).unwrap_or(0);
                        Some(BackupInfo {
                            name: e.file_name().to_string_lossy().into_owned(),
                            path: path_str(&e.path()),
                            bytes: meta.len(),
                            created_at: created,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.name.cmp(&a.name)));
        Ok(out)
    })
    .await
    .unwrap_or_default()
}

/// Writes a checked copy of the database behind `pool` to
/// `dir/pley-<date><suffix>.db`, then drops old copies: the daily ones
/// beyond [`KEEP`], and all copies made before an upgrade but the newest.
pub(crate) async fn write_checked(pool: &SqlitePool, dir: &Path, suffix: &str) -> anyhow::Result<BackupInfo> {
    tokio::fs::create_dir_all(dir).await?;
    let name = format!("pley-{}{suffix}.db", chrono_stamp(now()));
    let target = dir.join(&name);
    let partial = dir.join(format!("{name}.part"));
    let _ = tokio::fs::remove_file(&partial).await;
    sqlx::query("VACUUM INTO ?").bind(path_str(&partial)).execute(pool).await?;
    // A copy that fails its own check is not a backup.
    let mut conn = SqliteConnectOptions::new().filename(&partial).read_only(true).connect().await?;
    let check: String = sqlx::query_scalar("PRAGMA quick_check").fetch_one(&mut conn).await?;
    conn.close().await?;
    if check != "ok" {
        let _ = tokio::fs::remove_file(&partial).await;
        anyhow::bail!("the backup failed its check: {check}");
    }
    tokio::fs::rename(&partial, &target).await?;
    let upgrade = format!("{BEFORE_UPGRADE}.db");
    let (upgrades, daily): (Vec<_>, Vec<_>) = list(dir).await.into_iter().partition(|b| b.name.ends_with(&upgrade));
    for old in daily.into_iter().skip(KEEP).chain(upgrades.into_iter().skip(1)) {
        let _ = tokio::fs::remove_file(&old.path).await;
    }
    let bytes = tokio::fs::metadata(&target).await.map(|m| m.len()).unwrap_or(0);
    tracing::info!("database backed up to {} ({} KB)", target.display(), bytes / 1024);
    Ok(BackupInfo { name, path: path_str(&target), bytes, created_at: now() })
}

/// `2026-10-22-1830` (UTC) for backup file names.
fn chrono_stamp(secs: i64) -> String {
    let [year, month, day, hour, minute, _] = crate::util::utc(secs);
    format!("{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(chrono_stamp(0), "1970-01-01-0000");
        assert_eq!(chrono_stamp(1_792_713_600 + 3_600 + 120), "2026-10-23-0102");
        assert_eq!(chrono_stamp(951_782_400), "2000-02-29-0000");
    }

    #[tokio::test]
    async fn backups_are_checked_copies() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('probe', '1')").execute(&db.write).await.unwrap();
        let backups = Backups::new(db, dir.path());
        backups.check().await;
        assert_eq!(backups.health().ok, Some(true));
        let first = backups.create().await.unwrap();
        assert!(Path::new(&first.path).is_file());
        let copy = SqliteConnectOptions::new().filename(&first.path).read_only(true);
        let mut conn = copy.connect().await.unwrap();
        let value: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'probe'").fetch_one(&mut conn).await.unwrap();
        assert_eq!(value, "1");
        assert_eq!(backups.list().await.len(), 1);
    }

    #[tokio::test]
    async fn only_the_newest_backups_are_kept() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let backups = Backups::new(db, dir.path());
        let folder = dir.path().join("backups");
        std::fs::create_dir_all(&folder).unwrap();
        // Three older backups, a day apart, and files that are not backups.
        let day = std::time::Duration::from_secs(86_400);
        for (n, name) in
            ["pley-2020-01-01-0000.db", "pley-2020-01-02-0000.db", "pley-2020-01-03-0000.db"].into_iter().enumerate()
        {
            let file = std::fs::File::create(folder.join(name)).unwrap();
            let age = day * (10 - n as u32);
            file.set_modified(std::time::SystemTime::now() - age).unwrap();
        }
        std::fs::write(folder.join("pley-2020-01-04-0000.db.part"), b"unfinished").unwrap();
        std::fs::write(folder.join("notes.txt"), b"x").unwrap();
        let listed: Vec<String> = backups.list().await.into_iter().map(|b| b.name).collect();
        assert_eq!(listed, ["pley-2020-01-03-0000.db", "pley-2020-01-02-0000.db", "pley-2020-01-01-0000.db"]);
        // Copies made before two upgrades, older than every daily one.
        for (n, name) in
            ["pley-2019-01-01-0000-before-upgrade.db", "pley-2019-06-01-0000-before-upgrade.db"].into_iter().enumerate()
        {
            let file = std::fs::File::create(folder.join(name)).unwrap();
            file.set_modified(std::time::SystemTime::now() - day * (30 - n as u32)).unwrap();
        }

        let new = backups.create().await.unwrap();
        let listed: Vec<String> = backups.list().await.into_iter().map(|b| b.name).collect();
        assert_eq!(
            listed,
            [
                new.name.as_str(),
                "pley-2020-01-03-0000.db",
                "pley-2020-01-02-0000.db",
                "pley-2019-06-01-0000-before-upgrade.db"
            ]
        );
        assert!(!folder.join("pley-2020-01-01-0000.db").exists(), "the oldest is removed");
        assert!(folder.join("notes.txt").exists(), "other files are left alone");
        assert!(folder.join("pley-2019-06-01-0000-before-upgrade.db").exists(), "the last upgrade's copy stays");
        assert!(!folder.join("pley-2019-01-01-0000-before-upgrade.db").exists(), "older upgrades' copies go");
    }

    #[tokio::test]
    async fn health_is_unknown_until_checked() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let backups = Backups::new(db, dir.path());
        assert_eq!((backups.health().ok, backups.health().checked_at), (None, None));
        assert!(backups.list().await.is_empty(), "no folder yet: no backups");
        backups.check().await;
        let health = backups.health();
        assert_eq!((health.ok, health.problem), (Some(true), None));
        assert!(health.checked_at.is_some());
    }
}
