//! Who may see what.
//!
//! Administrators see everything. Other users see all libraries or a chosen
//! set and, with an age limit, only movies and shows rated for that age:
//! music is exempt, seasons and episodes go by their show's rating (or their
//! own, when that is stricter) and unrated titles are hidden. A profile's
//! label rules make exceptions: titles with a label it always sees show
//! above its age limit, and titles with a label it never sees don't show at
//! all, whatever their rating ("never" wins). A profile with labels it
//! never sees doesn't see a library a Plex import is bringing Plex's labels
//! to (`labels_pending`) until it's done: its rules would hide nothing there
//! yet. Item queries apply the same rules in SQL (see `catalog::VISIBLE`);
//! this module answers them for whole libraries, and lists them in the
//! user's own order when they chose one.
//!
//! Apps that can't show audiobooks don't get audiobook libraries, nor
//! their books where several libraries' titles are listed (Home, search,
//! history): see [`library_ids`]. That is what they show, not who may see
//! what: Pley's own API answers for a book asked for by its id. The Plex-
//! and Jellyfin-compatible APIs, whose clients have no books, answer `404`
//! for one, and change none ([`withholds`]).

use std::collections::HashMap;

use crate::auth::AuthUser;
use crate::db::Db;
use crate::library::{LibraryRow, list_libraries};

/// The account is an administrator's, whatever the token: every library,
/// with nothing withheld, as `catalog::VISIBLE` goes by the account too.
fn sees_all(user: &AuthUser) -> bool {
    user.is_admin || user.account_admin
}

/// The libraries a non-administrator `user_id` may open (`all`: every
/// library they may), binding `all` then the user id twice.
const OPEN: &str = "SELECT id FROM libraries
     WHERE (? OR id IN (SELECT library_id FROM user_libraries WHERE user_id = ?))
       AND NOT (labels_pending
                AND EXISTS (SELECT 1 FROM user_label_rules WHERE user_id = ? AND kind = 'hide'))";

/// Whether some titles of the libraries the user opens may be hidden from
/// them: an age limit, or labels it never sees. Their counts are then made
/// with `catalog::VISIBLE`.
pub async fn limited(db: &Db, user: &AuthUser) -> sqlx::Result<bool> {
    if user.is_admin || user.max_age.is_some() {
        return Ok(!user.is_admin);
    }
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_label_rules WHERE user_id = ? AND kind = 'hide')")
        .bind(user.id)
        .fetch_one(&db.read)
        .await
}

/// Library ids the user may open; `None` means all of them. A request that
/// doesn't get audiobooks ([`AuthUser::audiobooks`]) gets no audiobook
/// library, an administrator's app too.
pub async fn library_ids(db: &Db, user: &AuthUser) -> sqlx::Result<Option<Vec<i64>>> {
    let open = account_library_ids(db, user).await?;
    let withheld = withheld(&db.read, user).await?;
    if withheld.is_empty() {
        return Ok(open);
    }
    let open = match open {
        Some(ids) => ids,
        None => sqlx::query_scalar("SELECT id FROM libraries").fetch_all(&db.read).await?,
    };
    Ok(Some(open.into_iter().filter(|id| !withheld.contains(id)).collect()))
}

/// Whether library `id` is one the request doesn't get, whatever the
/// account may open: an audiobook library, unless it gets audiobooks.
pub async fn withholds(db: &Db, user: &AuthUser, id: i64) -> sqlx::Result<bool> {
    if user.audiobooks() {
        return Ok(false);
    }
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM libraries WHERE id = ? AND kind = 'audiobooks')")
        .bind(id)
        .fetch_one(&db.read)
        .await
}

/// Libraries the request doesn't get, whatever the account may open:
/// audiobook libraries, unless it gets audiobooks.
async fn withheld<'e, E: sqlx::SqliteExecutor<'e>>(db: E, user: &AuthUser) -> sqlx::Result<Vec<i64>> {
    if user.audiobooks() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar("SELECT id FROM libraries WHERE kind = 'audiobooks'").fetch_all(db).await
}

/// Library ids the account may open, whatever the request; `None` means
/// all of them. An event stream hears those, less what [`withholds`] says,
/// so that it hears of a library added after it last looked at once.
pub async fn account_library_ids(db: &Db, user: &AuthUser) -> sqlx::Result<Option<Vec<i64>>> {
    if sees_all(user) {
        return Ok(None);
    }
    if user.all_libraries {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM libraries WHERE labels_pending)
                AND EXISTS (SELECT 1 FROM user_label_rules WHERE user_id = ? AND kind = 'hide')",
        )
        .bind(user.id)
        .fetch_one(&db.read)
        .await?;
        if !waiting {
            return Ok(None);
        }
    }
    let ids = sqlx::query_scalar(OPEN).bind(user.all_libraries).bind(user.id).bind(user.id).fetch_all(&db.read).await?;
    Ok(Some(ids))
}

/// The libraries the user may open, in their own order ([`set_own_order`]):
/// the ones it places first, the others after in the order everyone sees.
pub async fn visible_libraries(db: &Db, user: &AuthUser) -> sqlx::Result<Vec<LibraryRow>> {
    let mut libraries = shared_order(db, user).await?;
    let own = own_order(&db.read, user.id).await?;
    if !own.is_empty() {
        // A stable sort: the libraries it doesn't place keep the shared order.
        libraries.sort_by_key(|l| own.get(&l.id).copied().unwrap_or(i64::MAX));
    }
    Ok(libraries)
}

/// Where the user's own order places libraries, by id.
async fn own_order<'e, E: sqlx::SqliteExecutor<'e>>(db: E, user_id: i64) -> sqlx::Result<HashMap<i64, i64>> {
    let places: Vec<(i64, i64)> = sqlx::query_as("SELECT library_id, position FROM library_order WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(places.into_iter().collect())
}

/// `order` with the libraries not in `kept` put in the order of `ids`
/// (which lists each of them once), and those in it where they are.
fn keep_places(order: &[i64], kept: &[i64], ids: &[i64]) -> Vec<i64> {
    let mut ids = ids.iter().copied();
    order.iter().filter_map(|&id| if kept.contains(&id) { Some(id) } else { ids.next() }).collect()
}

/// The shared order `ids` makes, from a request that may not get every
/// library: `ids` lists those it gets, each once, and those it doesn't (an
/// app's audiobook libraries) keep their places. `None` when `ids` aren't
/// the libraries it gets.
pub async fn whole_shared_order(db: &Db, user: &AuthUser, ids: &[i64]) -> sqlx::Result<Option<Vec<i64>>> {
    let shared: Vec<i64> = list_libraries(db).await?.into_iter().map(|l| l.id).collect();
    let withheld = withheld(&db.read, user).await?;
    let mut given: Vec<i64> = shared.iter().copied().filter(|id| !withheld.contains(id)).collect();
    let mut listed = ids.to_vec();
    given.sort_unstable();
    listed.sort_unstable();
    Ok((listed == given).then(|| keep_places(&shared, &withheld, ids)))
}

/// The libraries the user may open, in the order everyone sees them in.
pub async fn shared_order(db: &Db, user: &AuthUser) -> sqlx::Result<Vec<LibraryRow>> {
    let all = list_libraries(db).await?;
    Ok(match library_ids(db, user).await? {
        None => all,
        Some(ids) => all.into_iter().filter(|l| ids.contains(&l.id)).collect(),
    })
}

/// Lists the libraries `user` may open in the order of `ids`, for them
/// alone. `false`, and nothing changes, unless `ids` are the libraries the
/// request gets, each once; those it doesn't get (an app's audiobook
/// libraries) keep their places.
pub async fn set_own_order(db: &Db, user: &AuthUser, ids: &[i64]) -> sqlx::Result<bool> {
    // Checked and written under the one writer: no library comes or goes,
    // and no access changes, in between.
    let mut tx = db.write.begin().await?;
    let open: Vec<i64> = if sees_all(user) {
        sqlx::query_scalar("SELECT id FROM libraries").fetch_all(&mut *tx).await?
    } else {
        sqlx::query_scalar(OPEN).bind(user.all_libraries).bind(user.id).bind(user.id).fetch_all(&mut *tx).await?
    };
    let withheld: Vec<i64> = withheld(&mut *tx, user).await?.into_iter().filter(|id| open.contains(id)).collect();
    let mut current: Vec<i64> = open.iter().copied().filter(|id| !withheld.contains(id)).collect();
    let mut listed = ids.to_vec();
    current.sort_unstable();
    listed.sort_unstable();
    if listed != current {
        return Ok(false);
    }
    let order = if withheld.is_empty() {
        ids.to_vec()
    } else {
        // The order they are all seen in now, as `visible_libraries` lists
        // them, with the ones listed in their new order.
        let own = own_order(&mut *tx, user.id).await?;
        let mut seen: Vec<i64> =
            list_libraries(db).await?.into_iter().map(|l| l.id).filter(|id| open.contains(id)).collect();
        seen.sort_by_key(|id| own.get(id).copied().unwrap_or(i64::MAX));
        keep_places(&seen, &withheld, ids)
    };
    sqlx::query("DELETE FROM library_order WHERE user_id = ?").bind(user.id).execute(&mut *tx).await?;
    for (position, id) in order.iter().enumerate() {
        sqlx::query("INSERT INTO library_order (user_id, library_id, position) VALUES (?, ?, ?)")
            .bind(user.id)
            .bind(id)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// Forgets the user's own order: they see the libraries in the shared one.
pub async fn forget_own_order(db: &Db, user: &AuthUser) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM library_order WHERE user_id = ?").bind(user.id).execute(&db.write).await?;
    Ok(())
}

pub async fn can_open_library(db: &Db, user: &AuthUser, library_id: i64) -> sqlx::Result<bool> {
    Ok(library_ids(db, user).await?.is_none_or(|ids| ids.contains(&library_id)))
}
