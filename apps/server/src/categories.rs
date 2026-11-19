//! Categories of collections, by which a library's collections are grouped:
//! Pley's own (Highlights, the collections Pley makes; Box sets, TMDB's;
//! Other, for collections without one), named in each language, and
//! administrators' own. Shared by every library, in an order administrators
//! choose. A collection has one category or none, and none is sent as Other,
//! so apps group by what the server sends.

use serde::Serialize;
use sqlx::FromRow;

use crate::db::Db;
use crate::i18n;
use crate::util::now;

/// A category as a collection names it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CategoryRef {
    pub id: i64,
    /// `highlights`, `box_sets` or `other` for Pley's own; `None` for one
    /// an administrator made.
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

/// The key of Other, where collections without a category are listed. It
/// stands for none: a collection put in it is left without one.
pub const OTHER: &str = "other";

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

/// What a collection keeps for category `id`: none for Other.
pub async fn stored(db: &Db, id: Option<i64>) -> sqlx::Result<Option<i64>> {
    let Some(id) = id else { return Ok(None) };
    Ok(match row(db, id).await? {
        Some(r) if r.key.as_deref() == Some(OTHER) => None,
        _ => Some(id),
    })
}

/// The longest name a category has.
pub const MAX_TITLE: usize = 100;

/// Labels tools put on every collection they manage, which name no group:
/// Kometa's (`PMM` in its older versions).
const TOOL_LABELS: [&str; 2] = ["Kometa", "PMM"];

/// Puts a collection under the category its group label names, as Kometa
/// labels them ("Seasonal", "Decade"): its one label besides a tool's. None,
/// or several, leave it without one. Only a category that is unset or came
/// from a label changes: an administrator's choice and Pley's own stay.
pub async fn place_by_labels(db: &Db, collection_id: i64) -> sqlx::Result<()> {
    let from: Option<Option<String>> = sqlx::query_scalar("SELECT category_from FROM collections WHERE id = ?")
        .bind(collection_id)
        .fetch_optional(&db.read)
        .await?;
    let Some(from) = from else { return Ok(()) };
    if from.as_deref().is_some_and(|f| f != "label") {
        return Ok(());
    }
    let labels = crate::labels::for_collections(db, &[collection_id]).await?.remove(&collection_id).unwrap_or_default();
    let groups: Vec<&String> =
        labels.iter().filter(|l| !TOOL_LABELS.iter().any(|t| t.eq_ignore_ascii_case(l))).collect();
    let category = match groups.as_slice() {
        // A label named Other finds Pley's Other, which is none.
        [label] => stored(db, Some(for_label(db, label).await?)).await?,
        _ => None,
    };
    // Unless an administrator chose one meanwhile.
    sqlx::query(
        "UPDATE collections SET category_id = ?, category_from = ?
         WHERE id = ? AND (category_from IS NULL OR category_from = 'label')",
    )
    .bind(category)
    .bind(category.map(|_| "label"))
    .bind(collection_id)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// The category of a group label: the one made from it (renamed or not),
/// else one of that name (Pley's own in any language included), which then
/// keeps the label, else a new one, last. Another request making the same
/// one meanwhile is found the second time round.
async fn for_label(db: &Db, label: &str) -> sqlx::Result<i64> {
    let wanted = label.to_lowercase();
    let title: String = label.chars().take(MAX_TITLE).collect();
    for _ in 0..2 {
        let categories = list(db, "en").await?;
        if let Some(made) =
            categories.iter().find(|c| c.from_label.as_ref().is_some_and(|f| f.to_lowercase() == wanted))
        {
            return Ok(made.id);
        }
        for language in i18n::LANGUAGES {
            let named = list(db, language).await?.into_iter().find(|c| c.title.to_lowercase() == title.to_lowercase());
            if let Some(found) = named {
                if found.key.is_none() && found.from_label.is_none() {
                    sqlx::query("UPDATE collection_categories SET from_label = ? WHERE id = ? AND from_label IS NULL")
                        .bind(label)
                        .bind(found.id)
                        .execute(&db.write)
                        .await?;
                }
                return Ok(found.id);
            }
        }
        sqlx::query(
            "INSERT OR IGNORE INTO collection_categories (title, from_label, position, created_at)
             VALUES (?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_categories), ?)",
        )
        .bind(&title)
        .bind(label)
        .bind(now())
        .execute(&db.write)
        .await?;
    }
    Err(sqlx::Error::RowNotFound)
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
            [
                (Some("highlights".into()), "Highlights".into()),
                (Some("other".into()), "Other".into()),
                (Some("box_sets".into()), "Box sets".into())
            ]
        );
        let fr: Vec<_> = list(&db, "fr").await.unwrap().into_iter().map(|c| c.title).collect();
        assert_eq!(fr, ["À la une", "Autres", "Sagas"]);
        for builtin in list(&db, "en").await.unwrap() {
            assert_eq!(rename(&db, builtin.id, "Mine").await.unwrap(), Err(Refused::Builtin));
            assert_eq!(delete(&db, builtin.id).await.unwrap(), Err(Refused::Builtin));
        }
        // Other stands for none.
        let other = list(&db, "en").await.unwrap()[1].id;
        assert_eq!(stored(&db, Some(other)).await.unwrap(), None);
        assert_eq!(stored(&db, Some(other + 1)).await.unwrap(), Some(other + 1));
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
        assert_eq!(titles, ["By decade", "Highlights", "Other", "Box sets", "Holidays"]);
        delete(&db, holidays).await.unwrap().unwrap();
        assert!(!exists(&db, holidays).await.unwrap());
    }

    async fn category_of(db: &Db, collection: i64) -> (Option<String>, Option<String>) {
        sqlx::query_as(
            "SELECT COALESCE(k.title, k.key), c.category_from FROM collections c
             LEFT JOIN collection_categories k ON k.id = c.category_id WHERE c.id = ?",
        )
        .bind(collection)
        .fetch_one(&db.read)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn group_labels_put_collections_under_categories() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query("INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'M', 'movies', '[]', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO collections (id, library_id, title, sort_title, created_at, updated_at)
             VALUES (10, 1, 'Christmas', 'christmas', 0, 0), (11, 1, 'Halloween', 'halloween', 0, 0),
                    (12, 1, 'Mine', 'mine', 0, 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        let labels = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let set = |id: i64, add: Vec<String>, remove: Vec<String>| {
            let db = &db;
            async move { crate::labels::edit_collection(db, id, &add, &remove).await.unwrap() }
        };

        set(10, labels(&["Kometa", "Seasonal"]), vec![]).await;
        assert_eq!(category_of(&db, 10).await, (Some("Seasonal".into()), Some("label".into())));
        // Renamed, it still takes what Kometa labels "Seasonal".
        let seasonal = list(&db, "en").await.unwrap().into_iter().find(|c| c.title == "Seasonal").unwrap().id;
        rename(&db, seasonal, "Holidays").await.unwrap().unwrap();
        set(11, labels(&["PMM", "seasonal"]), vec![]).await;
        assert_eq!(category_of(&db, 11).await.0.as_deref(), Some("Holidays"));
        // A label naming Pley's own category puts it there, in any language.
        set(12, labels(&["Box Sets"]), vec![]).await;
        assert_eq!(category_of(&db, 12).await.0.as_deref(), Some("box_sets"));
        set(12, labels(&["sagas"]), labels(&["Box Sets"])).await;
        assert_eq!(category_of(&db, 12).await.0.as_deref(), Some("box_sets"));
        // But a label naming Other leaves it without one, as Other stands for none.
        set(12, labels(&["other"]), labels(&["sagas"])).await;
        assert_eq!(category_of(&db, 12).await, (None, None));
        // Two group labels, or none, name no category.
        set(10, labels(&["Decade"]), vec![]).await;
        assert_eq!(category_of(&db, 10).await, (None, None));
        set(10, vec![], labels(&["Decade"])).await;
        assert_eq!(category_of(&db, 10).await.0.as_deref(), Some("Holidays"));
        // An administrator's choice stays.
        sqlx::query("UPDATE collections SET category_id = NULL, category_from = 'admin' WHERE id = 11")
            .execute(&db.write)
            .await
            .unwrap();
        set(11, labels(&["Decade"]), labels(&["seasonal"])).await;
        assert_eq!(category_of(&db, 11).await, (None, Some("admin".into())));
    }

    #[tokio::test]
    async fn a_label_finds_its_category_whatever_its_case_and_long_ones_are_cut() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let summer = for_label(&db, "Été").await.unwrap();
        rename(&db, summer, "Summer").await.unwrap().unwrap();
        assert_eq!(for_label(&db, "été").await.unwrap(), summer);
        let long = "L".repeat(115);
        let made = for_label(&db, &long).await.unwrap();
        let title = get(&db, made, "en").await.unwrap().unwrap().title;
        assert_eq!(title.chars().count(), MAX_TITLE);
        assert_eq!(for_label(&db, &long).await.unwrap(), made);
    }
}
