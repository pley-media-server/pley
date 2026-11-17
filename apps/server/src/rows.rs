//! The rows of Home and of a library's Recommended tab, after Continue
//! Watching and My List: collections administrators put there, in the order
//! they chose. On Home, collections of the same title in different
//! libraries are one row. Music and audiobook libraries, which have no
//! collections, keep a built-in Recently Added row.

use std::collections::{HashMap, HashSet};

use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow};
use crate::collections::{self, Builtin, CollectionRow, Names};
use crate::db::Db;
use crate::i18n;
use crate::library::{LibraryKind, LibraryRow};
use crate::listing::{self, SortField};

/// How many titles a row shows; its collection has them all.
pub const ROW_SIZE: usize = 30;

pub struct Row {
    /// Stable, and what apps know Pley's own rows by: a built-in
    /// collection's `recently_added:<library_id>` (as the rows were before
    /// they were collections), `collection:<id>`, `collections:<id>,<id>`
    /// for collections of one title shown as one row, and a music or
    /// audiobook library's `recently_added:<library_id>`.
    pub key: String,
    pub title: String,
    /// The library of a row of one library's.
    pub library_id: Option<i64>,
    /// The collections the row shows: opening the row opens them.
    pub collection_ids: Vec<i64>,
    pub items: Vec<ItemRow>,
}

/// A row's key: what the row of one collection was called when it wasn't
/// one, so that apps keep placing and opening it as before.
fn key(rows: &[CollectionRow]) -> String {
    match rows {
        [row] => match row.builtin.as_deref().and_then(Builtin::parse) {
            Some(builtin) => format!("{}:{}", builtin.key(), row.library_id),
            None => format!("collection:{}", row.id),
        },
        rows => format!("collections:{}", rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>().join(",")),
    }
}

/// The rows of a movie or TV library's Recommended tab, in order; empty ones
/// left out.
pub async fn recommended(db: &Db, user: &AuthUser, lib: &LibraryRow, language: &'static str) -> sqlx::Result<Vec<Row>> {
    let names = Names::load(db, language).await?;
    let mut rows = Vec::new();
    for row in collections::recommended_rows(db, lib.id).await? {
        let Some(mut items) = collections::visible_items(db, user, &row, Some(ROW_SIZE)).await? else { continue };
        items.truncate(ROW_SIZE);
        if items.is_empty() {
            continue;
        }
        rows.push(Row {
            key: key(std::slice::from_ref(&row)),
            title: names.title(&row),
            library_id: Some(lib.id),
            collection_ids: vec![row.id],
            items,
        });
    }
    Ok(rows)
}

/// A Home row before its titles are read.
enum Planned {
    /// Collections of one title, in the user's library order.
    Collections(Vec<CollectionRow>),
    /// A music or audiobook library's Recently Added.
    Library(LibraryRow),
}

/// Home's rows of `libraries` (in their order, which settles ties), in
/// order, before their titles are read.
async fn plan(db: &Db, libraries: &[LibraryRow], language: &'static str) -> sqlx::Result<Vec<(String, Planned)>> {
    let place: HashMap<i64, usize> = libraries.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
    let by_id: HashMap<i64, &LibraryRow> = libraries.iter().map(|l| (l.id, l)).collect();
    let names = Names::load(db, language).await?;

    // Collections on Home, by their title (case aside), the same in every
    // language: what the row is called is the first library's name for it.
    let mut groups: Vec<(i64, String, Vec<CollectionRow>)> = Vec::new();
    for row in collections::home_rows(db).await? {
        if !by_id.contains_key(&row.library_id) {
            continue;
        }
        let Some(position) = row.home_position else { continue };
        let group = names.home_group(&row);
        match groups.iter_mut().find(|(_, g, _)| *g == group) {
            Some((at, _, rows)) => {
                *at = (*at).min(position);
                rows.push(row);
            }
            None => groups.push((position, group, vec![row])),
        }
    }
    let mut planned: Vec<(i64, usize, String, Planned)> = Vec::new();
    for (position, _, mut rows) in groups {
        rows.sort_by_key(|r| place.get(&r.library_id).copied().unwrap_or(usize::MAX));
        let first = place.get(&rows[0].library_id).copied().unwrap_or(usize::MAX);
        let title = names.home_title(&rows[0], &by_id[&rows[0].library_id].name);
        planned.push((position, first, title, Planned::Collections(rows)));
    }
    for (library_id, position) in crate::library::home_positions(db).await? {
        let Some(lib) = by_id.get(&library_id) else { continue };
        let title = i18n::t_with(language, "hubs.recentlyAddedIn", &[("library", &lib.name)]);
        planned.push((position, place[&library_id], title, Planned::Library((*lib).clone())));
    }
    planned.sort_by_key(|(position, place, _, _)| (*position, *place));
    Ok(planned.into_iter().map(|(_, _, title, plan)| (title, plan)).collect())
}

impl Planned {
    fn key(&self) -> String {
        match self {
            Planned::Collections(rows) => key(rows),
            Planned::Library(lib) => format!("recently_added:{}", lib.id),
        }
    }
}

/// Home's rows after Continue Watching and My List, in order; empty ones
/// left out. Only libraries the user may open count.
pub async fn home(db: &Db, user: &AuthUser, language: &'static str) -> sqlx::Result<Vec<Row>> {
    home_rows(db, user, language, true).await
}

/// [`home`] with collections of one title as rows of their own, side by
/// side, as Plex's Home has them (its apps put them together on screen).
pub async fn home_apart(db: &Db, user: &AuthUser, language: &'static str) -> sqlx::Result<Vec<Row>> {
    home_rows(db, user, language, false).await
}

async fn home_rows(db: &Db, user: &AuthUser, language: &'static str, merge: bool) -> sqlx::Result<Vec<Row>> {
    let libraries = crate::access::visible_libraries(db, user).await?;
    let mut planned = plan(db, &libraries, language).await?;
    if !merge {
        let names = Names::load(db, language).await?;
        let name = |id: i64| libraries.iter().find(|l| l.id == id).map(|l| l.name.as_str()).unwrap_or_default();
        planned = planned
            .into_iter()
            .flat_map(|(title, plan)| match plan {
                Planned::Collections(rows) if rows.len() > 1 => rows
                    .into_iter()
                    .map(|row| (names.home_title(&row, name(row.library_id)), Planned::Collections(vec![row])))
                    .collect(),
                plan => vec![(title, plan)],
            })
            .collect();
    }
    let mut out = Vec::new();
    for (title, plan) in planned {
        let row = match plan {
            Planned::Library(lib) => Row {
                key: format!("recently_added:{}", lib.id),
                title,
                library_id: Some(lib.id),
                collection_ids: Vec::new(),
                items: catalog::recently_added(db, user.id, lib.id, lib.kind().as_str(), ROW_SIZE as i64).await?,
            },
            Planned::Collections(rows) => {
                let mut lists = Vec::new();
                for row in &rows {
                    lists.push(collections::visible_items(db, user, row, Some(ROW_SIZE)).await?.unwrap_or_default());
                }
                let libraries: Vec<i64> = rows.iter().map(|r| r.library_id).collect();
                Row {
                    key: key(&rows),
                    title,
                    library_id: (libraries.iter().all(|l| *l == libraries[0])).then_some(libraries[0]),
                    collection_ids: rows.iter().map(|r| r.id).collect(),
                    items: merged(db, &rows, lists, ROW_SIZE).await?,
                }
            }
        };
        if !row.items.is_empty() {
            out.push(row);
        }
    }
    Ok(out)
}

/// A row of Home as administrators arrange them: every library's, empty or
/// not.
#[derive(Debug, serde::Serialize)]
pub struct HomeRow {
    pub key: String,
    pub title: String,
    pub collection_ids: Vec<i64>,
    /// The libraries it shows titles of.
    pub library_ids: Vec<i64>,
}

pub async fn home_arrangement(db: &Db, language: &'static str) -> sqlx::Result<Vec<HomeRow>> {
    let libraries = crate::library::list_libraries(db).await?;
    Ok(plan(db, &libraries, language)
        .await?
        .into_iter()
        .map(|(title, plan)| HomeRow {
            key: plan.key(),
            title,
            collection_ids: match &plan {
                Planned::Collections(rows) => rows.iter().map(|r| r.id).collect(),
                Planned::Library(_) => Vec::new(),
            },
            library_ids: match &plan {
                Planned::Collections(rows) => rows.iter().map(|r| r.library_id).collect(),
                Planned::Library(lib) => vec![lib.id],
            },
        })
        .collect())
}

/// Home's order: the rows of `keys` (as [`home_arrangement`] names them)
/// first in that order, the others after as they were. Every collection of
/// a row takes the row's place.
pub async fn order_home(db: &Db, keys: &[String]) -> sqlx::Result<()> {
    let libraries = crate::library::list_libraries(db).await?;
    // Rows and their keys are the same in every language.
    let current = plan(db, &libraries, "en").await?;
    let mut order: Vec<usize> = Vec::new();
    for key in keys {
        if let Some(at) = current.iter().position(|(_, p)| p.key() == *key)
            && !order.contains(&at)
        {
            order.push(at);
        }
    }
    let rest: Vec<usize> = (0..current.len()).filter(|at| !order.contains(at)).collect();
    order.extend(rest);
    let mut tx = db.write.begin().await?;
    for (position, at) in order.into_iter().enumerate() {
        match &current[at].1 {
            Planned::Collections(rows) => {
                for row in rows {
                    sqlx::query("UPDATE collections SET home_position = ? WHERE id = ?")
                        .bind(position as i64)
                        .bind(row.id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            Planned::Library(lib) => {
                sqlx::query("UPDATE libraries SET home_position = ? WHERE id = ?")
                    .bind(position as i64)
                    .bind(lib.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await
}

/// How a collection of a library of `kind` orders its titles, when it's an
/// order titles of other libraries can be put in too; `None` for a
/// hand-made order. A film has no episodes, so when it was added is when
/// its newest episode was: a film library's newest first is a TV
/// library's (shows by their newest episode) too.
fn order_of(row: &CollectionRow, kind: Option<LibraryKind>) -> Option<Vec<(SortField, bool)>> {
    let mut order = match (&row.smart_filter, row.sort.as_str()) {
        (Some(filter), _) => listing::smart_sort(filter),
        (None, "title") => vec![(SortField::Title, false), (SortField::Year, false)],
        (None, "custom") => return None,
        (None, _) => vec![(SortField::Released, false), (SortField::Year, false)],
    };
    if kind == Some(LibraryKind::Movies) {
        for (field, _) in &mut order {
            if *field == SortField::Added {
                *field = SortField::EpisodeAdded;
            }
        }
    }
    Some(order)
}

/// The titles of collections shown as one row, up to `limit`, each once
/// (collections of one library may share some): in their common order
/// when they have one (newest first, best rated first…), else one
/// collection's after another's, in `rows`' order.
pub(crate) async fn merged(
    db: &Db,
    rows: &[CollectionRow],
    lists: Vec<Vec<ItemRow>>,
    limit: usize,
) -> sqlx::Result<Vec<ItemRow>> {
    let mut items: Vec<ItemRow> = lists.into_iter().flatten().collect();
    if rows.len() > 1 {
        let kinds: HashMap<i64, LibraryKind> =
            crate::library::list_libraries(db).await?.iter().map(|l| (l.id, l.kind())).collect();
        let orders: Vec<_> = rows.iter().map(|r| order_of(r, kinds.get(&r.library_id).copied())).collect();
        if let Some(Some(order)) = orders.first()
            && orders.iter().all(|o| o.as_ref() == Some(order))
        {
            let values = listing::sort_values(db, &items, order).await?;
            listing::sort_rows(&mut items, order, &values);
        }
        let mut seen = HashSet::new();
        items.retain(|item| seen.insert(item.id));
    }
    items.truncate(limit);
    Ok(items)
}
