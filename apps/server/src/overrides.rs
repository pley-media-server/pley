//! Hand-edited metadata, over what NFO files say. Each edited field is
//! stored as an override and written back onto the item after every
//! metadata refresh and scan, so the metadata agent never undoes an edit.
//! An item's NFO file (see [`crate::library::nfo`]) is written the same
//! way, first: it wins over the online source, and hand edits win over it.
//! Removing an override lets the next refresh fill the field again.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use sqlx::Row;

use crate::db::Db;
use crate::error::AppError;
use crate::library::nfo::{NfoFields, NfoIds};
use crate::library::scanner::{ItemIds, nfo_describes};
use crate::util::{now, sort_title};

/// Editable fields and their item columns.
pub const FIELDS: &[&str] = &[
    "title",
    "sort_title",
    "original_title",
    "year",
    "summary",
    "tagline",
    "studio",
    "content_rating",
    "release_date",
    "genres",
    "poster",
    "backdrop",
    "thumb",
];

/// Not a field: Fix Match keeps the TMDB id chosen here, so that an NFO
/// file's ids don't undo it. Dropping it (as a field is unlocked) hands the
/// match back to the NFO file.
pub const MATCH: &str = "match";

/// Item columns that are written here: the editable fields, an NFO file's
/// rating, and the country and age that go with a content rating.
const COLUMNS: &[&str] = &[
    "title",
    "sort_title",
    "original_title",
    "year",
    "summary",
    "tagline",
    "studio",
    "content_rating",
    "release_date",
    "genres",
    "poster",
    "backdrop",
    "thumb",
    "rating",
    "content_rating_country",
    "rating_age",
];

/// Items looked up at a time.
const CHUNK: usize = 500;

pub fn validate(field: &str, value: Option<&str>) -> Result<(), AppError> {
    if field == MATCH {
        return match value {
            None => Ok(()),
            Some(_) => Err(AppError::bad_request("the match is chosen with Fix Match")),
        };
    }
    if !FIELDS.contains(&field) {
        return Err(AppError::bad_request(format!("'{field}' can't be edited")));
    }
    let Some(value) = value else { return Ok(()) };
    match field {
        "title" if value.trim().is_empty() => Err(AppError::bad_request("the title can't be empty")),
        "year" if value.parse::<i64>().map_or(true, |y| !(1800..=2200).contains(&y)) => {
            Err(AppError::bad_request("the year must be a number like 1999"))
        }
        "release_date" if !looks_like_date(value) => Err(AppError::bad_request("dates look like 1999-03-31")),
        "genres" if serde_json::from_str::<Vec<String>>(value).is_err() => {
            Err(AppError::bad_request("genres must be a JSON list of names"))
        }
        _ => Ok(()),
    }
}

fn looks_like_date(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && value.split('-').all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

/// Fields of an item that are overridden (and [`MATCH`] after Fix Match).
pub async fn locked_fields(db: &Db, item_id: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT field FROM item_overrides WHERE item_id = ? ORDER BY field")
        .bind(item_id)
        .fetch_all(&db.read)
        .await
}

/// An item's NFO file as the last scan read it, with the item's ids.
#[derive(sqlx::FromRow)]
struct ItemNfo {
    item_id: i64,
    kind: String,
    fields: String,
    ids: String,
    #[sqlx(flatten)]
    matched: ItemIds,
}

const NFO_OF_ITEMS: &str = "SELECT n.item_id, i.kind, n.fields, n.ids, i.tmdb_id, i.imdb_id, i.tvdb_id, i.tvmaze_id
     FROM item_nfo n JOIN items i ON i.id = n.item_id";

impl ItemNfo {
    /// The item's kind and the fields of its NFO file, unless the file
    /// describes another title than the one the item is matched to (with
    /// Fix Match): its title or summary would be wrong there.
    fn fields(self) -> Option<(String, NfoFields)> {
        let ids = serde_json::from_str::<NfoIds>(&self.ids).ok()?;
        if !nfo_describes(&self.kind, &self.matched, &ids) {
            return None;
        }
        Some((self.kind, serde_json::from_str(&self.fields).ok()?))
    }
}

/// Fields of an item that its NFO file sets, named as [`FIELDS`] are (and
/// `rating`).
pub async fn nfo_fields(db: &Db, item_id: i64) -> sqlx::Result<Vec<String>> {
    let sql = format!("{NFO_OF_ITEMS} WHERE n.item_id = ?");
    let row: Option<ItemNfo> = sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(item_id).fetch_optional(&db.read).await?;
    let Some((kind, fields)) = row.and_then(ItemNfo::fields) else { return Ok(Vec::new()) };
    let mut names: Vec<String> = nfo_values(&kind, &fields, "US")
        .into_iter()
        // A sort title made from the NFO file's title isn't one it gives,
        // and a rating's country is part of the rating.
        .filter(|(column, _)| *column != "sort_title" || fields.sort_title.is_some())
        .filter(|(column, _)| *column != "content_rating_country")
        .map(|(column, _)| column.to_owned())
        .collect();
    names.sort();
    Ok(names)
}

/// The item columns an NFO file's fields write, and their values. A
/// season's gives its title and summary only: its sort title stays its
/// number. Of several countries' ratings, `region`'s is taken (see
/// [`NfoFields::rating_in`]).
pub fn nfo_values(kind: &str, nfo: &NfoFields, region: &str) -> Vec<(&'static str, String)> {
    let mut values = vec![("title", nfo.title.clone()), ("summary", nfo.summary.clone())];
    if kind != "season" {
        let rating = nfo.rating_in(region);
        values.extend([
            ("sort_title", nfo.sort_title.clone().or_else(|| nfo.title.as_deref().map(sort_title))),
            ("original_title", nfo.original_title.clone()),
            ("year", nfo.year.map(|y| y.to_string())),
            ("release_date", nfo.release_date.clone()),
            ("tagline", nfo.tagline.clone()),
            ("content_rating", rating.map(|(_, rating)| rating.to_owned())),
            ("content_rating_country", rating.and_then(|(country, _)| country).map(str::to_owned)),
            ("genres", (!nfo.genres.is_empty()).then(|| serde_json::to_string(&nfo.genres).unwrap_or_default())),
            ("studio", nfo.studio.clone()),
            ("rating", nfo.rating.map(|r| r.to_string())),
        ]);
    }
    values.into_iter().filter_map(|(column, value)| Some((column, value?))).collect()
}

/// After Fix Match: when the item's NFO file describes another title than
/// the one chosen, what it set gives way. Its fields still on the item are
/// emptied for the refresh to fill (TVmaze and TheTVDB keep what they have
/// no value for), but the title, which every source gives; and its lockdata
/// goes, so that the chosen title is fetched.
pub async fn nfo_set_aside(db: &Db, item_id: i64) -> sqlx::Result<()> {
    let sql = format!("{NFO_OF_ITEMS} WHERE n.item_id = ?");
    let row: Option<ItemNfo> = sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(item_id).fetch_optional(&db.read).await?;
    let Some(nfo) = row else { return Ok(()) };
    let Ok(ids) = serde_json::from_str::<NfoIds>(&nfo.ids) else { return Ok(()) };
    if nfo_describes(&nfo.kind, &nfo.matched, &ids) {
        return Ok(());
    }
    let fields = serde_json::from_str::<NfoFields>(&nfo.fields).unwrap_or_default();
    let region = crate::ratings::region(db).await?;
    let mut tx = db.write.begin().await?;
    for (column, value) in nfo_values(&nfo.kind, &fields, &region) {
        if matches!(column, "title" | "sort_title" | "content_rating_country") {
            continue;
        }
        // `column` comes from `nfo_values`, so formatting it into SQL is
        // safe. Only what still holds the NFO file's value is emptied.
        let age = if column == "content_rating" { ", content_rating_country = NULL, rating_age = NULL" } else { "" };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE items SET {column} = NULL{age} WHERE id = ? AND {column} IS ?"
        )))
        .bind(item_id)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE items SET metadata_locked = 0
         WHERE id = ? AND EXISTS (SELECT 1 FROM item_nfo n WHERE n.item_id = items.id AND n.locked)",
    )
    .bind(item_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Store (`Some`) or drop (`None`) overrides, then apply them.
pub async fn set(db: &Db, item_id: i64, changes: &[(String, Option<String>)]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    for (field, value) in changes {
        match value {
            Some(value) => {
                sqlx::query(
                    "INSERT INTO item_overrides (item_id, field, value) VALUES (?, ?, ?)
                     ON CONFLICT(item_id, field) DO UPDATE SET value = excluded.value",
                )
                .bind(item_id)
                .bind(field)
                .bind(value)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM item_overrides WHERE item_id = ? AND field = ?")
                    .bind(item_id)
                    .bind(field)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    apply(db, &[item_id]).await
}

/// Write the NFO files' fields and the overrides of `item_ids` onto the
/// items, hand edits over NFO files. What already holds them isn't
/// written: a scan applies them all again, and a rescan that finds nothing
/// new must leave listings cached (and the writer to others).
pub async fn apply(db: &Db, item_ids: &[i64]) -> sqlx::Result<()> {
    let region = crate::ratings::region(db).await?;
    let mut changes = Vec::new();
    for chunk in item_ids.chunks(CHUNK) {
        let wanted = wanted(db, chunk, &region).await?;
        if !wanted.is_empty() {
            changes.extend(differing(db, wanted, &region).await?);
        }
    }
    if changes.is_empty() {
        return Ok(());
    }
    let mut tx = db.write.begin().await?;
    let mut changed = BTreeSet::new();
    for (item, column, value) in &changes {
        // `column` is one of COLUMNS, so formatting it into SQL is safe. The
        // comparison is SQLite's: "7" is the rating 7.0.
        let written = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE items SET {column} = ? WHERE id = ? AND {column} IS NOT ?"
        )))
        .bind(value)
        .bind(item)
        .bind(value)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if written {
            changed.insert(*item);
        }
    }
    for item in changed {
        sqlx::query("UPDATE items SET updated_at = ? WHERE id = ?").bind(now()).bind(item).execute(&mut *tx).await?;
    }
    tx.commit().await
}

/// What the NFO files and overrides of `item_ids` want their items' columns
/// to hold, by item.
async fn wanted(
    db: &Db,
    item_ids: &[i64],
    region: &str,
) -> sqlx::Result<HashMap<i64, BTreeMap<&'static str, Option<String>>>> {
    let placeholders = vec!["?"; item_ids.len()].join(",");
    let sql = format!("{NFO_OF_ITEMS} WHERE n.item_id IN ({placeholders})");
    let mut query = sqlx::query_as::<_, ItemNfo>(sqlx::AssertSqlSafe(sql.as_str()));
    for id in item_ids {
        query = query.bind(id);
    }
    let nfos = query.fetch_all(&db.read).await?;
    let sql = format!("SELECT item_id, field, value FROM item_overrides WHERE item_id IN ({placeholders})");
    let mut query = sqlx::query_as::<_, (i64, String, Option<String>)>(sqlx::AssertSqlSafe(sql.as_str()));
    for id in item_ids {
        query = query.bind(id);
    }
    let edits = query.fetch_all(&db.read).await?;

    let mut wanted: HashMap<i64, BTreeMap<&'static str, Option<String>>> = HashMap::new();
    for nfo in nfos {
        let item = nfo.item_id;
        let Some((kind, fields)) = nfo.fields() else { continue };
        let values = nfo_values(&kind, &fields, region);
        if !values.is_empty() {
            wanted.entry(item).or_default().extend(values.into_iter().map(|(column, value)| (column, Some(value))));
        }
    }
    let mut by_item: HashMap<i64, Vec<(&'static str, Option<String>)>> = HashMap::new();
    for (item, field, value) in edits {
        // Fix Match's [`MATCH`] is no column.
        if let Some(column) = FIELDS.iter().find(|f| **f == field) {
            by_item.entry(item).or_default().push((column, value));
        }
    }
    for (item, edits) in by_item {
        let values = wanted.entry(item).or_default();
        let edited = |column: &str| edits.iter().any(|(c, _)| *c == column);
        // A title edited by hand sorts by itself, not by the NFO file's
        // sort title.
        if edited("title") && !edited("sort_title") {
            let title = edits.iter().find(|(c, _)| *c == "title").and_then(|(_, v)| v.as_deref());
            values.insert("sort_title", title.map(sort_title));
        }
        let rating_edited = edited("content_rating");
        values.extend(edits);
        // A rating edited is read with the server's country's ratings,
        // unless it's written with its country ("GB:15", Plex's "nz/R"):
        // then it's that country's, as an NFO file's is.
        if rating_edited {
            let rating = values.get("content_rating").cloned().flatten();
            let (country, rating) = match rating.as_deref().map(crate::ratings::with_country) {
                Some((country, rating)) => (country.map(str::to_owned), Some(rating.to_owned())),
                None => (None, None),
            };
            values.insert("content_rating", rating);
            values.insert("content_rating_country", country);
        }
    }
    Ok(wanted)
}

/// The columns of `wanted` that their items don't hold, as text: a number
/// written differently ("7" for 7.0) is left to [`apply`]'s comparison.
/// The age that goes with a wanted content rating is worked out here, where
/// the item's kind is known.
async fn differing(
    db: &Db,
    mut wanted: HashMap<i64, BTreeMap<&'static str, Option<String>>>,
    region: &str,
) -> sqlx::Result<Vec<(i64, &'static str, Option<String>)>> {
    let columns = COLUMNS.iter().map(|c| format!("CAST({c} AS TEXT) AS {c}")).collect::<Vec<_>>().join(", ");
    let placeholders = vec!["?"; wanted.len()].join(",");
    let sql = format!("SELECT id, kind, {columns} FROM items WHERE id IN ({placeholders})");
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
    for id in wanted.keys() {
        query = query.bind(id);
    }
    let mut changes = Vec::new();
    for row in query.fetch_all(&db.read).await? {
        let id: i64 = row.try_get("id")?;
        let Some(values) = wanted.get_mut(&id) else { continue };
        if let Some(rating) = values.get("content_rating").cloned() {
            let kind: String = row.try_get("kind")?;
            // An NFO file's rating with no country has none.
            let country = values.entry("content_rating_country").or_insert(None).clone();
            let country = country.as_deref().unwrap_or(region);
            let age = rating.and_then(|r| crate::ratings::age(Some(country), crate::ratings::Kind::of(&kind), &r));
            values.insert("rating_age", age.map(|a| a.to_string()));
        }
        for (column, value) in values.iter() {
            let current: Option<String> = row.try_get(*column)?;
            if current != *value {
                changes.push((id, *column, value.clone()));
            }
        }
    }
    Ok(changes)
}

/// SQL for `UPDATE items …`: whether the row's content rating was set by
/// hand or by its NFO file. Metadata refreshes leave it (and its age) alone
/// instead of overwriting it until the hand edit or the NFO file is
/// applied again: age limits depend on it. An NFO file counts only while it
/// describes the title the item is matched to, as `nfo_describes` has it: a
/// movie's TMDB or IMDb id, a show's from any source, or none at all.
pub const RATING_LOCKED: &str =
    "(EXISTS (SELECT 1 FROM item_overrides o WHERE o.item_id = items.id AND o.field = 'content_rating')
     OR EXISTS (SELECT 1 FROM item_nfo n WHERE n.item_id = items.id
                AND json_extract(n.fields, '$.content_rating') IS NOT NULL
                AND (items.kind NOT IN ('movie', 'show')
                     OR (json_extract(n.ids, '$.tmdb') IS NULL AND json_extract(n.ids, '$.imdb') IS NULL
                         AND (items.kind = 'movie'
                              OR (json_extract(n.ids, '$.tvdb') IS NULL AND json_extract(n.ids, '$.tvmaze') IS NULL)))
                     OR json_extract(n.ids, '$.tmdb') = items.tmdb_id
                     OR json_extract(n.ids, '$.imdb') = items.imdb_id
                     OR (items.kind = 'show' AND (json_extract(n.ids, '$.tvdb') = items.tvdb_id
                                                  OR json_extract(n.ids, '$.tvmaze') = items.tvmaze_id)))))";

/// Apply NFO files and overrides again to every item of a library that has
/// either (after a scan).
pub async fn apply_library(db: &Db, library_id: i64) -> sqlx::Result<()> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT o.item_id FROM item_overrides o JOIN items i ON i.id = o.item_id WHERE i.library_id = ?1
         UNION SELECT n.item_id FROM item_nfo n JOIN items i ON i.id = n.item_id WHERE i.library_id = ?1",
    )
    .bind(library_id)
    .fetch_all(&db.read)
    .await?;
    apply(db, &ids).await
}

/// Apply NFO files and overrides again to an item and everything below it
/// (after a refresh).
pub async fn apply_tree(db: &Db, item_id: i64) -> sqlx::Result<()> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT o.item_id FROM item_overrides o JOIN items i ON i.id = o.item_id
         WHERE i.id = ?1 OR i.parent_id = ?1 OR i.grandparent_id = ?1
         UNION SELECT n.item_id FROM item_nfo n JOIN items i ON i.id = n.item_id
         WHERE i.id = ?1 OR i.parent_id = ?1 OR i.grandparent_id = ?1",
    )
    .bind(item_id)
    .fetch_all(&db.read)
    .await?;
    apply(db, &ids).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::nfo::NfoRating;

    #[test]
    fn validation() {
        assert!(validate("title", Some("Alien")).is_ok());
        assert!(validate("title", Some("  ")).is_err());
        assert!(validate("year", Some("1979")).is_ok());
        assert!(validate("year", Some("79a")).is_err());
        assert!(validate("release_date", Some("1979-05-25")).is_ok());
        assert!(validate("release_date", Some("25/05/1979")).is_err());
        assert!(validate("genres", Some(r#"["Horror","Sci-Fi"]"#)).is_ok());
        assert!(validate("genres", Some("Horror")).is_err());
        assert!(validate("path", Some("/etc/passwd")).is_err());
        assert!(validate("summary", None).is_ok());
        // Fix Match's match can be dropped, not set, as a field.
        assert!(validate("match", None).is_ok());
        assert!(validate("match", Some("348")).is_err());
    }

    #[test]
    fn nfo_values_are_the_fields_an_nfo_file_has() {
        let fields = NfoFields {
            title: Some("The Matrix".into()),
            year: Some(1999),
            genres: vec!["Action".into(), "Science Fiction".into()],
            rating: Some(8.7),
            summary: Some("Neo.".into()),
            ..Default::default()
        };
        assert_eq!(
            nfo_values("movie", &fields, "US"),
            [
                ("title", "The Matrix".to_owned()),
                ("summary", "Neo.".to_owned()),
                ("sort_title", "matrix".to_owned()),
                ("year", "1999".to_owned()),
                ("genres", r#"["Action","Science Fiction"]"#.to_owned()),
                ("rating", "8.7".to_owned()),
            ]
        );
        // Of several countries' ratings, the region's, with its country.
        let rated = NfoFields {
            content_rating: Some("R".into()),
            content_ratings: vec![
                NfoRating { country: Some("GB".into()), rating: "15".into() },
                NfoRating { country: Some("US".into()), rating: "R".into() },
            ],
            ..Default::default()
        };
        let rating = |region| {
            nfo_values("movie", &rated, region)
                .into_iter()
                .filter(|(c, _)| c.starts_with("content_rating"))
                .collect::<Vec<_>>()
        };
        assert_eq!(rating("GB"), [("content_rating", "15".to_owned()), ("content_rating_country", "GB".to_owned())]);
        assert_eq!(rating("FR"), [("content_rating", "R".to_owned()), ("content_rating_country", "US".to_owned())]);
        // A season keeps its number as its sort title.
        assert_eq!(
            nfo_values("season", &fields, "US"),
            [("title", "The Matrix".to_owned()), ("summary", "Neo.".to_owned())]
        );
    }
}
