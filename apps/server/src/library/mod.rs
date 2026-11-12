//! Libraries: folders of movies, shows or music that are scanned into items.

pub mod nfo;
pub mod parse;
pub mod scanner;
pub mod sidecar;
pub mod walk;
pub mod watcher;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::db::Db;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryKind {
    Movies,
    Shows,
    Music,
}

impl LibraryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryKind::Movies => "movies",
            LibraryKind::Shows => "shows",
            LibraryKind::Music => "music",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "movies" => Some(LibraryKind::Movies),
            "shows" => Some(LibraryKind::Shows),
            "music" => Some(LibraryKind::Music),
            _ => None,
        }
    }

    /// Item kind listed at the top level of a library of this kind.
    pub fn top_level_kind(self) -> &'static str {
        match self {
            LibraryKind::Movies => "movie",
            LibraryKind::Shows => "show",
            LibraryKind::Music => "artist",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LibraryRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub paths: String,
    pub created_at: i64,
    pub scanned_at: Option<i64>,
    /// A Plex import is bringing Plex's labels to its titles: a profile with
    /// labels it never sees doesn't see it until then (`crate::access`).
    pub labels_pending: bool,
}

impl LibraryRow {
    pub fn kind(&self) -> LibraryKind {
        LibraryKind::parse(&self.kind).unwrap_or(LibraryKind::Movies)
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        serde_json::from_str::<Vec<String>>(&self.paths).unwrap_or_default().into_iter().map(PathBuf::from).collect()
    }
}

pub async fn get_library(db: &Db, id: i64) -> sqlx::Result<Option<LibraryRow>> {
    sqlx::query_as("SELECT id, name, kind, paths, created_at, scanned_at, labels_pending FROM libraries WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.read)
        .await
}

/// Every library, in the order administrators chose (see [`set_order`]).
pub async fn list_libraries(db: &Db) -> sqlx::Result<Vec<LibraryRow>> {
    sqlx::query_as(
        "SELECT id, name, kind, paths, created_at, scanned_at, labels_pending FROM libraries
         ORDER BY position, name COLLATE NOCASE, id",
    )
    .fetch_all(&db.read)
    .await
}

/// Lists the libraries in the order of `ids`, the same for every user.
/// `false`, and nothing changes, unless `ids` are the libraries there are,
/// each once.
pub async fn set_order(db: &Db, ids: &[i64]) -> sqlx::Result<bool> {
    // Checked and written under the one writer: no library comes or goes
    // in between.
    let mut tx = db.write.begin().await?;
    let mut current: Vec<i64> = sqlx::query_scalar("SELECT id FROM libraries").fetch_all(&mut *tx).await?;
    let mut listed = ids.to_vec();
    current.sort_unstable();
    listed.sort_unstable();
    if listed != current {
        return Ok(false);
    }
    for (position, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE libraries SET position = ? WHERE id = ?")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use sqlx::migrate::Migrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};

    use super::*;

    fn names(libraries: Vec<LibraryRow>) -> Vec<String> {
        libraries.into_iter().map(|l| l.name).collect()
    }

    /// Libraries from before their order was stored keep the order they were
    /// listed in: by name.
    #[tokio::test]
    async fn libraries_keep_their_order_through_the_upgrade() {
        let dir = crate::testing::tempdir();
        let path = dir.path().join("pley.db");
        // A database made by a version that listed libraries by name.
        let older = dir.path().join("older");
        std::fs::create_dir(&older).unwrap();
        for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap().flatten() {
            if entry.file_name().to_string_lossy().as_ref() < "0020" {
                std::fs::copy(entry.path(), older.join(entry.file_name())).unwrap();
            }
        }
        let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(true))
            .await
            .unwrap();
        Migrator::new(older.as_path()).await.unwrap().run(&pool).await.unwrap();
        for (id, name) in [(1, "TV"), (2, "movies"), (3, "Anime"), (4, "Music")] {
            sqlx::query("INSERT INTO libraries (id, name, kind, created_at) VALUES (?, ?, 'movies', 0)")
                .bind(id)
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;

        let db = Db::open(&path).await.unwrap();
        assert_eq!(names(list_libraries(&db).await.unwrap()), ["Anime", "movies", "Music", "TV"]);
        // Kept as their places, not worked out from their names: a library
        // renamed afterwards stays where it was.
        sqlx::query("UPDATE libraries SET name = 'Zoo' WHERE id = 3").execute(&db.write).await.unwrap();
        assert_eq!(names(list_libraries(&db).await.unwrap()), ["Zoo", "movies", "Music", "TV"]);
        sqlx::query("UPDATE libraries SET name = 'Anime' WHERE id = 3").execute(&db.write).await.unwrap();
        assert!(set_order(&db, &[4, 3, 2, 1]).await.unwrap());
        assert_eq!(names(list_libraries(&db).await.unwrap()), ["Music", "Anime", "movies", "TV"]);
        assert!(!set_order(&db, &[4, 3, 2]).await.unwrap(), "every library");
        assert!(!set_order(&db, &[4, 3, 2, 1, 5]).await.unwrap(), "only libraries");
        assert_eq!(names(list_libraries(&db).await.unwrap()), ["Music", "Anime", "movies", "TV"]);
        db.close().await;
    }
}
