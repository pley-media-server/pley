//! The rows of a library and where they show: its own page, administrators'
//! Home, everyone else's Home, in an order administrators choose. A library
//! has built-in rows (Continue Watching, Recently Added, Recently Released,
//! Top Rated), and administrators can promote its collections to rows of
//! their own. They arrange them in Settings → Libraries, or a tool does
//! through the Plex-compatible API's hub management (Agregarr does).
//!
//! A library nobody arranged shows its built-in rows as Pley always has:
//! every one on its page, Recently Added on Home too.

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{ApiResult, AppError};
use crate::library::{LibraryKind, LibraryRow};

/// Rows a library holds at most: its built-in ones and promoted collections.
const MAX_HUBS: usize = 200;

/// A library's built-in rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    ContinueWatching,
    RecentlyAdded,
    RecentlyReleased,
    TopRated,
}

impl Builtin {
    pub fn key(self) -> &'static str {
        match self {
            Builtin::ContinueWatching => "continue_watching",
            Builtin::RecentlyAdded => "recently_added",
            Builtin::RecentlyReleased => "recently_released",
            Builtin::TopRated => "top_rated",
        }
    }

    /// The built-in rows of a library of this kind, in their usual order:
    /// music has nothing to continue, nor release dates and ratings to sort
    /// by.
    pub fn of(kind: LibraryKind) -> &'static [Builtin] {
        match kind {
            LibraryKind::Music => &[Builtin::RecentlyAdded],
            LibraryKind::Movies | LibraryKind::Shows => {
                &[Builtin::ContinueWatching, Builtin::RecentlyAdded, Builtin::RecentlyReleased, Builtin::TopRated]
            }
        }
    }

    /// Where it shows until an administrator says otherwise. Home has its
    /// own Continue Watching, of every library.
    fn default_place(self) -> HubPlace {
        let home = self == Builtin::RecentlyAdded;
        HubPlace { hub: self.key().to_owned(), on_library: true, on_admin_home: home, on_home: home }
    }
}

/// What a row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubKind {
    Builtin(Builtin),
    /// The collection of that id, of the same library.
    Collection(i64),
}

impl HubKind {
    pub fn parse(hub: &str) -> Option<Self> {
        if let Some(id) = hub.strip_prefix("collection:") {
            return id.parse().ok().filter(|id| *id > 0).map(HubKind::Collection);
        }
        [Builtin::ContinueWatching, Builtin::RecentlyAdded, Builtin::RecentlyReleased, Builtin::TopRated]
            .into_iter()
            .find(|b| b.key() == hub)
            .map(HubKind::Builtin)
    }

    pub fn key(self) -> String {
        match self {
            HubKind::Builtin(builtin) => builtin.key().to_owned(),
            HubKind::Collection(id) => format!("collection:{id}"),
        }
    }
}

/// A row of a library and where it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubPlace {
    /// A built-in row's key (`recently_added`) or `collection:{id}`.
    pub hub: String,
    /// On the library's own page.
    pub on_library: bool,
    /// On administrators' Home.
    pub on_admin_home: bool,
    /// On everyone else's Home.
    pub on_home: bool,
}

impl HubPlace {
    pub fn kind(&self) -> Option<HubKind> {
        HubKind::parse(&self.hub)
    }

    /// Whether it shows on Home for a viewer who is an administrator, or not.
    pub fn on_home_of(&self, admin: bool) -> bool {
        if admin { self.on_admin_home } else { self.on_home }
    }
}

/// Where rows are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The library's own page.
    Library,
    /// Home, for an administrator or not.
    Home { admin: bool },
}

/// The rows of `lib` in order, where each shows: as arranged, else as
/// usual. Built-in rows missing from an arrangement (none are, unless one
/// is added to Pley later) come last, where they usually show.
pub async fn layout(db: &Db, lib: &LibraryRow) -> sqlx::Result<Vec<HubPlace>> {
    let stored: Vec<(String, bool, bool, bool)> = sqlx::query_as(
        "SELECT hub, on_library, on_admin_home, on_home FROM library_hubs WHERE library_id = ? ORDER BY position",
    )
    .bind(lib.id)
    .fetch_all(&db.read)
    .await?;
    let builtins = Builtin::of(lib.kind());
    let mut places: Vec<HubPlace> = stored
        .into_iter()
        .map(|(hub, on_library, on_admin_home, on_home)| HubPlace { hub, on_library, on_admin_home, on_home })
        .filter(|place| match place.kind() {
            Some(HubKind::Builtin(builtin)) => builtins.contains(&builtin),
            Some(HubKind::Collection(_)) => true,
            None => false,
        })
        .collect();
    for builtin in builtins {
        if !places.iter().any(|p| p.kind() == Some(HubKind::Builtin(*builtin))) {
            places.push(builtin.default_place());
        }
    }
    Ok(places)
}

/// The rows `lib` shows on `page`, in order.
pub async fn shown(db: &Db, lib: &LibraryRow, page: Page) -> sqlx::Result<Vec<HubKind>> {
    Ok(layout(db, lib)
        .await?
        .into_iter()
        .filter(|place| match page {
            Page::Library => place.on_library,
            Page::Home { admin } => place.on_home_of(admin),
        })
        .filter_map(|place| place.kind())
        .collect())
}

/// Arrange `lib`'s rows as `places` says, in its order. Each built-in row
/// of the library is in it once at most, and a missing one goes last where
/// it usually shows; collections must be the library's. Rows that show
/// nowhere stay in place, so that a tool finds them where it left them.
pub async fn set_layout(db: &Db, lib: &LibraryRow, places: &[HubPlace]) -> ApiResult<Vec<HubPlace>> {
    if places.len() > MAX_HUBS {
        return Err(AppError::bad_request(format!("a library has {MAX_HUBS} rows at most")));
    }
    let builtins = Builtin::of(lib.kind());
    let mut arranged: Vec<HubPlace> = Vec::with_capacity(places.len() + builtins.len());
    let mut collections = Vec::new();
    for place in places {
        let kind = place.kind().ok_or_else(|| AppError::bad_request(format!("no row is called {:?}", place.hub)))?;
        match kind {
            HubKind::Builtin(builtin) if !builtins.contains(&builtin) => {
                return Err(AppError::bad_request(format!("a library of {} has no {} row", lib.kind, builtin.key())));
            }
            HubKind::Builtin(_) => {}
            HubKind::Collection(id) => collections.push(id),
        }
        if arranged.iter().any(|p| p.kind() == Some(kind)) {
            return Err(AppError::bad_request(format!("{} is in the list twice", place.hub)));
        }
        arranged.push(HubPlace { hub: kind.key(), ..place.clone() });
    }
    if !collections.is_empty() {
        let placeholders = vec!["?"; collections.len()].join(",");
        let sql = format!("SELECT COUNT(*) FROM collections WHERE library_id = ? AND id IN ({placeholders})");
        let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str())).bind(lib.id);
        for id in &collections {
            query = query.bind(id);
        }
        if usize::try_from(query.fetch_one(&db.read).await?).ok() != Some(collections.len()) {
            return Err(AppError::bad_request("only the library's own collections become its rows"));
        }
    }
    for builtin in builtins {
        if !arranged.iter().any(|p| p.kind() == Some(HubKind::Builtin(*builtin))) {
            arranged.push(builtin.default_place());
        }
    }
    let mut tx = db.write.begin().await?;
    sqlx::query("DELETE FROM library_hubs WHERE library_id = ?").bind(lib.id).execute(&mut *tx).await?;
    for (position, place) in arranged.iter().enumerate() {
        let collection = match place.kind() {
            Some(HubKind::Collection(id)) => Some(id),
            _ => None,
        };
        sqlx::query(
            "INSERT INTO library_hubs (library_id, hub, collection_id, position, on_library, on_admin_home, on_home)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(lib.id)
        .bind(&place.hub)
        .bind(collection)
        .bind(i64::try_from(position).unwrap_or(i64::MAX))
        .bind(place.on_library)
        .bind(place.on_admin_home)
        .bind(place.on_home)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(arranged)
}

/// Put `lib`'s rows back as usual: the built-in ones where they usually
/// show, and no collection promoted.
pub async fn reset(db: &Db, library_id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM library_hubs WHERE library_id = ?").bind(library_id).execute(&db.write).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hubs_are_named() {
        assert_eq!(HubKind::parse("recently_added"), Some(HubKind::Builtin(Builtin::RecentlyAdded)));
        assert_eq!(HubKind::parse("collection:12"), Some(HubKind::Collection(12)));
        assert_eq!(HubKind::Collection(12).key(), "collection:12");
        for wrong in ["collection:", "collection:0", "collection:-3", "collection:x", "Recently_added", ""] {
            assert_eq!(HubKind::parse(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn built_in_rows_show_as_usual() {
        let places: Vec<HubPlace> = Builtin::of(LibraryKind::Movies).iter().map(|b| b.default_place()).collect();
        assert!(places.iter().all(|p| p.on_library));
        let home: Vec<&str> = places.iter().filter(|p| p.on_home_of(false)).map(|p| p.hub.as_str()).collect();
        assert_eq!(home, ["recently_added"]);
        assert_eq!(Builtin::of(LibraryKind::Music), [Builtin::RecentlyAdded]);
    }
}
