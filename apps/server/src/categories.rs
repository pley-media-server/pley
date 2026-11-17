//! Categories of collections, by which a library's collections are grouped:
//! Pley's own (Highlights, the collections Pley makes; Franchises, TMDB's),
//! named in each language, and administrators' own. Shared by every library,
//! in an order administrators choose. A collection has one category or none
//! (listed under Other).

use serde::Serialize;
use sqlx::FromRow;

use crate::db::Db;
use crate::i18n;
use crate::util::now;

/// A category as a collection names it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CategoryRef {
    pub id: i64,
    /// `highlights` or `franchises` for Pley's own; `None` for one an
    /// administrator made.
    pub key: Option<String>,
    /// In the user's language for Pley's own.
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub key: Option<String>,
    pub title: String,
    /// The group label it was made from, which still puts collections in it
    /// after it is renamed.
    pub from_label: Option<String>,
    pub position: i64,
}

impl Category {
    pub fn into_ref(self) -> CategoryRef {
        CategoryRef { id: self.id, key: self.key, title: self.title }
    }
}

#[derive(FromRow)]
struct Row {
    id: i64,
    key: Option<String>,
    title: Option<String>,
    from_label: Option<String>,
    position: i64,
}

/// The words for Pley's own categories.
fn builtin_title(language: &str, key: &str) -> String {
    i18n::t(language, &format!("collections.{key}"))
}

/// Every category, in order, named in `language`.
pub async fn list(db: &Db, language: &str) -> sqlx::Result<Vec<Category>> {
    let rows: Vec<Row> =
        sqlx::query_as("SELECT id, key, title, from_label, position FROM collection_categories ORDER BY position, id")
            .fetch_all(&db.read)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| Category {
            title: match (&r.key, r.title) {
                (Some(key), _) => builtin_title(language, key),
                (None, title) => title.unwrap_or_default(),
            },
            id: r.id,
            key: r.key,
            from_label: r.from_label,
            position: r.position,
        })
        .collect())
}

pub async fn get(db: &Db, id: i64, language: &str) -> sqlx::Result<Option<Category>> {
    Ok(list(db, language).await?.into_iter().find(|c| c.id == id))
}

/// Why a category's name wasn't taken.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// Another category is called that (case aside).
    Taken,
    /// Pley's own categories keep their names, and stay.
    Builtin,
    NotFound,
}

/// Whether `title` is already a category's name, Pley's own included (in
/// any of the server's languages) so the list never shows two alike.
async fn taken(db: &Db, title: &str, except: Option<i64>) -> sqlx::Result<bool> {
    let wanted = title.to_lowercase();
    for language in i18n::LANGUAGES {
        if list(db, language).await?.iter().any(|c| Some(c.id) != except && c.title.to_lowercase() == wanted) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A new category of an administrator's, last.
pub async fn create(db: &Db, title: &str) -> sqlx::Result<Result<i64, Refused>> {
    if taken(db, title, None).await? {
        return Ok(Err(Refused::Taken));
    }
    let id = sqlx::query_scalar(
        "INSERT INTO collection_categories (title, position, created_at)
         VALUES (?, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_categories), ?) RETURNING id",
    )
    .bind(title)
    .bind(now())
    .fetch_one(&db.write)
    .await?;
    Ok(Ok(id))
}

async fn row(db: &Db, id: i64) -> sqlx::Result<Option<Row>> {
    sqlx::query_as("SELECT id, key, title, from_label, position FROM collection_categories WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.read)
        .await
}

pub async fn rename(db: &Db, id: i64, title: &str) -> sqlx::Result<Result<(), Refused>> {
    match row(db, id).await? {
        None => return Ok(Err(Refused::NotFound)),
        Some(r) if r.key.is_some() => return Ok(Err(Refused::Builtin)),
        Some(_) => {}
    }
    if taken(db, title, Some(id)).await? {
        return Ok(Err(Refused::Taken));
    }
    sqlx::query("UPDATE collection_categories SET title = ? WHERE id = ?")
        .bind(title)
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(Ok(()))
}

/// Deletes an administrator's category; its collections are left without
/// one, as nobody's choice, so their labels may place them again.
pub async fn delete(db: &Db, id: i64) -> sqlx::Result<Result<(), Refused>> {
    match row(db, id).await? {
        None => return Ok(Err(Refused::NotFound)),
        Some(r) if r.key.is_some() => return Ok(Err(Refused::Builtin)),
        Some(_) => {}
    }
    let mut tx = db.write.begin().await?;
    sqlx::query("UPDATE collections SET category_from = NULL WHERE category_id = ?").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM collection_categories WHERE id = ?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Ok(()))
}

/// `ids` first in this order, the rest after as they were.
pub async fn reorder(db: &Db, ids: &[i64]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    let current: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM collection_categories ORDER BY position, id").fetch_all(&mut *tx).await?;
    for (position, id) in crate::playlists::merged_order(&current, ids).iter().enumerate() {
        sqlx::query("UPDATE collection_categories SET position = ? WHERE id = ?")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

pub async fn exists(db: &Db, id: i64) -> sqlx::Result<bool> {
    Ok(row(db, id).await?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pleys_categories_are_named_in_each_language_and_stay() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let en: Vec<_> = list(&db, "en").await.unwrap().into_iter().map(|c| (c.key, c.title)).collect();
        assert_eq!(
            en,
            [(Some("highlights".into()), "Highlights".into()), (Some("franchises".into()), "Franchises".into())]
        );
        let fr: Vec<_> = list(&db, "fr").await.unwrap().into_iter().map(|c| c.title).collect();
        assert_eq!(fr, ["À la une", "Sagas"]);
        let highlights = list(&db, "en").await.unwrap()[0].id;
        assert_eq!(rename(&db, highlights, "Mine").await.unwrap(), Err(Refused::Builtin));
        assert_eq!(delete(&db, highlights).await.unwrap(), Err(Refused::Builtin));
    }

    #[tokio::test]
    async fn an_administrators_categories_come_last_with_names_of_their_own() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let holidays = create(&db, "Holidays").await.unwrap().unwrap();
        assert_eq!(create(&db, "holidays").await.unwrap(), Err(Refused::Taken));
        // Pley's own names, in any of the server's languages, are taken too.
        assert_eq!(create(&db, "Sagas").await.unwrap(), Err(Refused::Taken));
        let decades = create(&db, "Decades").await.unwrap().unwrap();
        assert_eq!(rename(&db, decades, "HOLIDAYS").await.unwrap(), Err(Refused::Taken));
        rename(&db, decades, "By decade").await.unwrap().unwrap();
        reorder(&db, &[decades]).await.unwrap();
        let titles: Vec<_> = list(&db, "en").await.unwrap().into_iter().map(|c| c.title).collect();
        assert_eq!(titles, ["By decade", "Highlights", "Franchises", "Holidays"]);
        delete(&db, holidays).await.unwrap().unwrap();
        assert!(!exists(&db, holidays).await.unwrap());
    }
}
