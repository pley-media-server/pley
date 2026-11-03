//! Labels on items (see migrations/0015_labels.sql): free-form tags, matched
//! without regard to case. They are set through the Plex-compatible API.

use std::collections::HashMap;

use crate::db::Db;

/// Longest label kept, in characters.
const MAX_LEN: usize = 120;

/// The labels of `ids`, by item, sorted.
pub async fn for_items(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<String>>> {
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT item_id, label FROM item_labels WHERE item_id IN ({}) ORDER BY label",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, (i64, String)>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for (item, label) in query.fetch_all(&db.read).await? {
            out.entry(item).or_default().push(label);
        }
    }
    Ok(out)
}

/// Adds `add` and removes `remove` (names, whatever their case).
pub async fn edit(db: &Db, item_id: i64, add: &[String], remove: &[String]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    for label in remove {
        sqlx::query("DELETE FROM item_labels WHERE item_id = ? AND label = ?")
            .bind(item_id)
            .bind(label.trim())
            .execute(&mut *tx)
            .await?;
    }
    for label in add.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let label: String = label.chars().take(MAX_LEN).collect();
        sqlx::query("INSERT OR IGNORE INTO item_labels (item_id, label) VALUES (?, ?)")
            .bind(item_id)
            .bind(label)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn labels_are_added_once_and_removed_whatever_their_case() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query("INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'M', 'movies', '[]', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (1, 1, 'movie', 'a', 'A', 'a', 0, 0), (2, 1, 'movie', 'b', 'B', 'b', 0, 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        let labels = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        edit(&db, 1, &labels(&["Overlay", "4K", " overlay ", ""]), &[]).await.unwrap();
        edit(&db, 2, &labels(&["Kids"]), &[]).await.unwrap();
        let found = for_items(&db, &[1, 2]).await.unwrap();
        assert_eq!(found[&1], ["4K", "Overlay"]);
        assert_eq!(found[&2], ["Kids"]);
        edit(&db, 1, &[], &labels(&["OVERLAY"])).await.unwrap();
        assert_eq!(for_items(&db, &[1]).await.unwrap()[&1], ["4K"]);
    }
}
