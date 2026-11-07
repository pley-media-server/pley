//! Labels on items (see migrations/0015_labels.sql) and on collections
//! (0043_collection_labels.sql): free-form tags, matched without regard to
//! case. They are set through the Plex-compatible API and the web app's
//! edit dialog, and a profile's label rules show or hide titles by them (see
//! `catalog::VISIBLE`).

use std::collections::HashMap;

use serde::Serialize;

use crate::auth::AuthUser;
use crate::auth::compatibility::Scope;
use crate::db::Db;
use crate::error::AppError;

/// Longest label kept, in characters.
const MAX_LEN: usize = 120;

/// The kinds of item the web app labels: titles and what holds them.
pub const LABELLED: &[&str] = &["movie", "show", "season", "episode", "artist", "album"];

/// Whether `user` is told items' labels: administrators in Pley's web app,
/// and compatibility tokens that edit (Kometa's). Profiles' label rules go
/// by them, so a child shouldn't read "Override: Adult".
pub fn shown_to(user: &AuthUser) -> bool {
    (user.is_admin && user.app.is_none()) || user.compatibility_allows(Scope::Manage)
}

/// Most labels an item takes from the edit dialog.
const MAX_PER_ITEM: usize = 100;

/// `labels` as they're kept: trimmed, without empty ones or the same label
/// twice (whatever the case of its letters A to Z, as the database matches
/// them), in their order. Refused: too many, or too long.
pub fn check(labels: &[String]) -> Result<Vec<String>, AppError> {
    let mut kept: Vec<String> = Vec::new();
    for label in labels.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if label.chars().count() > MAX_LEN {
            return Err(AppError::bad_request(format!("a label is at most {MAX_LEN} characters")));
        }
        if !kept.iter().any(|k| k.eq_ignore_ascii_case(label)) {
            kept.push(label.to_owned());
        }
    }
    if kept.len() > MAX_PER_ITEM {
        return Err(AppError::bad_request(format!("at most {MAX_PER_ITEM} labels")));
    }
    Ok(kept)
}

/// A label, how many titles carry it, and how many profiles' and
/// invitations' rules name it.
#[derive(Debug, Serialize, sqlx::FromRow, PartialEq)]
pub struct LabelUse {
    pub label: String,
    pub items: i64,
    pub rules: i64,
}

/// Every label on an item or in a rule, by name: a label a rule names
/// before any title carries it is offered too.
pub async fn all(db: &Db) -> sqlx::Result<Vec<LabelUse>> {
    sqlx::query_as(
        "SELECT MIN(label) AS label, SUM(items) AS items, SUM(rules) AS rules FROM (
             SELECT label, 1 AS items, 0 AS rules FROM item_labels
             UNION ALL SELECT label, 0, 1 FROM user_label_rules
             UNION ALL SELECT j.value, 0, 1 FROM invites, json_each(invites.show_labels) j
             UNION ALL SELECT j.value, 0, 1 FROM invites, json_each(invites.hide_labels) j
         )
         GROUP BY label COLLATE NOCASE ORDER BY label COLLATE NOCASE",
    )
    .fetch_all(&db.read)
    .await
}

/// Makes `labels` (checked by [`check`]) item `item_id`'s labels. One
/// written otherwise ("kids" for "Kids") is removed and added again, as
/// it's now written.
pub async fn replace(db: &Db, item_id: i64, labels: &[String]) -> sqlx::Result<()> {
    let current = for_items(db, &[item_id]).await?.remove(&item_id).unwrap_or_default();
    let remove: Vec<String> = current.iter().filter(|c| !labels.contains(c)).cloned().collect();
    edit(db, item_id, labels, &remove).await
}

/// What labels are on: the table and its owner column.
#[derive(Clone, Copy)]
enum On {
    Items,
    Collections,
}

impl On {
    fn table(self) -> (&'static str, &'static str) {
        match self {
            On::Items => ("item_labels", "item_id"),
            On::Collections => ("collection_labels", "collection_id"),
        }
    }
}

/// The labels of `ids`, by item, sorted.
pub async fn for_items(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<String>>> {
    labels_of(db, On::Items, ids).await
}

/// The labels of collections `ids`, by collection, sorted.
pub async fn for_collections(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<String>>> {
    labels_of(db, On::Collections, ids).await
}

async fn labels_of(db: &Db, on: On, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<String>>> {
    let (table, owner) = on.table();
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT {owner}, label FROM {table} WHERE {owner} IN ({}) ORDER BY label",
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
    edit_labels(db, On::Items, item_id, add, remove).await
}

/// Adds `add` to collection `id` and removes `remove`, as [`edit`].
pub async fn edit_collection(db: &Db, id: i64, add: &[String], remove: &[String]) -> sqlx::Result<()> {
    edit_labels(db, On::Collections, id, add, remove).await
}

async fn edit_labels(db: &Db, on: On, owner_id: i64, add: &[String], remove: &[String]) -> sqlx::Result<()> {
    let (table, owner) = on.table();
    let mut tx = db.write.begin().await?;
    for label in remove {
        sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM {table} WHERE {owner} = ? AND label = ?")))
            .bind(owner_id)
            .bind(label.trim())
            .execute(&mut *tx)
            .await?;
    }
    for label in add.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let label: String = label.chars().take(MAX_LEN).collect();
        sqlx::query(sqlx::AssertSqlSafe(format!("INSERT OR IGNORE INTO {table} ({owner}, label) VALUES (?, ?)")))
            .bind(owner_id)
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
        let dir = crate::testing::tempdir();
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
        // Replaced, a label written otherwise is written so.
        replace(&db, 2, &labels(&["kids", "New"])).await.unwrap();
        assert_eq!(for_items(&db, &[2]).await.unwrap()[&2], ["kids", "New"]);
        // Letters other than A to Z are matched as written, as the database
        // matches them.
        assert_eq!(check(&labels(&["École", "école", "KIDS", "kids"])).unwrap(), ["École", "école", "KIDS"]);

        // Collections keep theirs apart, even with an item's id.
        sqlx::query(
            "INSERT INTO collections (id, library_id, title, sort_title, created_at, updated_at)
             VALUES (1, 1, 'Two', 'two', 0, 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        edit_collection(&db, 1, &labels(&["Kometa", "kometa"]), &[]).await.unwrap();
        assert_eq!(for_collections(&db, &[1]).await.unwrap()[&1], ["Kometa"]);
        assert_eq!(for_items(&db, &[1]).await.unwrap()[&1], ["4K"]);
        edit_collection(&db, 1, &[], &labels(&["KOMETA"])).await.unwrap();
        assert!(for_collections(&db, &[1]).await.unwrap().is_empty());
    }
}
