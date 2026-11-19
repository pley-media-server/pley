//! Collections: groups of movies or shows within one library, shared by all
//! users. Automatic ones mirror TMDB's collections and are filled by the
//! metadata agent; administrators create and edit their own. Items a user
//! can't see are left out, and collections left without visible items are
//! hidden from that user.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow, VISIBLE, me_cte};
use crate::db::Db;
use crate::i18n;
use crate::library::LibraryKind;
use crate::store::Store;
use crate::util::{now, sort_title};

/// Automatic collections with fewer items than this are not listed (a
/// collection holding a single film is clutter).
const MIN_AUTOMATIC: i64 = 2;
const IMAGES: &str = "https://image.tmdb.org/t/p";

/// The collections Pley makes in each movie and TV library: what were its
/// built-in rows. Smart collections like any other, which administrators
/// can change, delete and restore ([`restore_defaults`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    RecentlyAdded,
    RecentlyReleased,
    TopRated,
    TopUnwatched,
}

impl Builtin {
    pub const ALL: [Builtin; 4] =
        [Builtin::RecentlyAdded, Builtin::RecentlyReleased, Builtin::TopRated, Builtin::TopUnwatched];

    pub fn key(self) -> &'static str {
        match self {
            Builtin::RecentlyAdded => "recently_added",
            Builtin::RecentlyReleased => "recently_released",
            Builtin::TopRated => "top_rated",
            Builtin::TopUnwatched => "top_unwatched",
        }
    }

    pub fn parse(key: &str) -> Option<Builtin> {
        Builtin::ALL.into_iter().find(|b| b.key() == key)
    }

    /// Plex's name for its row in a library of `kind`, or one in Plex's
    /// style where Plex has none (`movie.toprated`, `tv.topunwatched`).
    pub fn plex_identifier(self, kind: LibraryKind) -> String {
        let prefix = if kind == LibraryKind::Shows { "tv" } else { "movie" };
        let name = match (kind, self) {
            (_, Builtin::RecentlyAdded) => "recentlyadded",
            (LibraryKind::Shows, Builtin::RecentlyReleased) => "recentlyaired",
            (_, Builtin::RecentlyReleased) => "recentlyreleased",
            (_, Builtin::TopRated) => "toprated",
            (_, Builtin::TopUnwatched) => "topunwatched",
        };
        format!("{prefix}.{name}")
    }

    /// The icon its poster starts with, and its color ([`crate::collection_art`]).
    pub fn icon(self) -> (&'static str, &'static str) {
        match self {
            Builtin::RecentlyAdded => ("clock", "blue"),
            Builtin::RecentlyReleased => ("calendar", "teal"),
            Builtin::TopRated => ("star", "amber"),
            Builtin::TopUnwatched => ("eye-off", "pink"),
        }
    }

    /// Its name (the server's words), by the library's kind.
    pub fn title_key(self, kind: LibraryKind) -> &'static str {
        match self {
            Builtin::RecentlyAdded if kind == LibraryKind::Shows => "hubs.recentlyAddedShows",
            Builtin::RecentlyAdded => "hubs.recentlyAdded",
            Builtin::RecentlyReleased => "hubs.recentlyReleased",
            Builtin::TopRated => "hubs.topRated",
            Builtin::TopUnwatched => "hubs.topUnwatched",
        }
    }

    /// Its filter: what the built-in row showed, its 100 first. Shows came
    /// to Recently Added by their newest episode.
    pub fn filter(self, kind: LibraryKind) -> &'static str {
        match self {
            Builtin::RecentlyAdded if kind == LibraryKind::Shows => "sort=episode.addedAt:desc,id:desc&limit=100",
            Builtin::RecentlyAdded => "sort=addedAt:desc,id:desc&limit=100",
            Builtin::RecentlyReleased => "originallyAvailableAt!=-1&sort=originallyAvailableAt:desc&limit=100",
            Builtin::TopRated => "rating!=-1&sort=rating:desc,titleSort&limit=100",
            Builtin::TopUnwatched => "unwatched=1&rating!=-1&sort=rating:desc,titleSort&limit=100",
        }
    }

    /// Its place on the library's Recommended tab, as the rows were, or
    /// none for Top Unwatched: for anyone who hasn't watched the best rated
    /// yet, as on a new server, it would be Top Rated again. Administrators
    /// can add it.
    fn position(self) -> Option<i64> {
        (self != Builtin::TopUnwatched).then(|| Builtin::ALL.iter().position(|b| *b == self).unwrap_or_default() as i64)
    }
}

/// What collections are called for one user: built-in ones named in their
/// language until an administrator renames them, and the categories.
pub struct Names {
    language: &'static str,
    kinds: HashMap<i64, LibraryKind>,
    categories: HashMap<i64, crate::categories::CategoryRef>,
    /// Where collections without a category are listed.
    other: crate::categories::CategoryRef,
}

impl Names {
    pub async fn load(db: &Db, language: &'static str) -> sqlx::Result<Names> {
        let kinds: Vec<(i64, String)> = sqlx::query_as("SELECT id, kind FROM libraries").fetch_all(&db.read).await?;
        let categories = crate::categories::list(db, language).await?;
        let other = categories
            .iter()
            .find(|c| c.key.as_deref() == Some(crate::categories::OTHER))
            .cloned()
            .ok_or(sqlx::Error::RowNotFound)?
            .into_ref();
        Ok(Names {
            other,
            language,
            kinds: kinds
                .into_iter()
                .map(|(id, kind)| (id, LibraryKind::parse(&kind).unwrap_or(LibraryKind::Movies)))
                .collect(),
            categories: categories.into_iter().map(|c| (c.id, c.into_ref())).collect(),
        })
    }

    fn kind(&self, library_id: i64) -> LibraryKind {
        self.kinds.get(&library_id).copied().unwrap_or(LibraryKind::Movies)
    }

    /// A built-in collection still called what Pley called it.
    fn untouched(&self, row: &CollectionRow) -> Option<Builtin> {
        let builtin = row.builtin.as_deref().and_then(Builtin::parse)?;
        (row.title == i18n::t("en", builtin.title_key(self.kind(row.library_id)))).then_some(builtin)
    }

    pub(crate) fn title(&self, row: &CollectionRow) -> String {
        match self.untouched(row) {
            Some(builtin) => i18n::t(self.language, builtin.title_key(self.kind(row.library_id))),
            None => row.title.clone(),
        }
    }

    /// Its title as a row on Home, where Recently Added says which library
    /// (and so doesn't join other libraries' until it's renamed alike).
    pub(crate) fn home_title(&self, row: &CollectionRow, library: &str) -> String {
        match self.untouched(row) {
            Some(Builtin::RecentlyAdded) => {
                i18n::t_with(self.language, "hubs.recentlyAddedIn", &[("library", library)])
            }
            _ => self.title(row),
        }
    }

    /// Which row on Home it is part of, the same in every language:
    /// collections of the same title, an untouched Recently Added on its
    /// own.
    pub(crate) fn home_group(&self, row: &CollectionRow) -> String {
        match self.untouched(row) {
            // Titles never hold a NUL, so this is no collection's title.
            Some(Builtin::RecentlyAdded) => format!("\0recently_added:{}", row.library_id),
            // Untouched or not, the stored title, which is English for an
            // untouched one, as its title is in English.
            _ => row.title.to_lowercase(),
        }
    }
}

/// `belongs_to_collection` of a TMDB movie.
#[derive(Debug, Clone, Deserialize)]
pub struct TmdbCollection {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub backdrop_path: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct CollectionRow {
    pub(crate) id: i64,
    pub(crate) library_id: i64,
    pub(crate) title: String,
    sort_title: String,
    summary: Option<String>,
    tmdb_id: Option<i64>,
    poster: Option<String>,
    backdrop: Option<String>,
    /// A poster an administrator uploaded: it wins over `poster`.
    custom_poster: Option<String>,
    /// An icon and its color ([`crate::collection_art`]) drawn over its
    /// films' posters: it wins over `poster` too.
    icon: Option<String>,
    icon_color: Option<String>,
    /// `color`: the icon on its color alone, not over the posters.
    poster_background: Option<String>,
    pub(crate) sort: String,
    pub(crate) smart_filter: Option<String>,
    category_id: Option<i64>,
    pub(crate) home_position: Option<i64>,
    pub recommended_position: Option<i64>,
    pub(crate) builtin: Option<String>,
    created_at: i64,
    updated_at: i64,
}

const COLUMNS: &str =
    "id, library_id, title, sort_title, summary, tmdb_id, poster, backdrop, custom_poster, icon, icon_color, poster_background, sort,
    smart_filter,
    category_id, home_position, recommended_position, builtin, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct CollectionDto {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    /// What it is ordered by; set by hand (through the Plex API) when it isn't
    /// the title's own.
    #[serde(skip)]
    pub sort_title: String,
    pub summary: Option<String>,
    /// Filled automatically from TMDB.
    pub automatic: bool,
    /// `release`, `title` or `custom`.
    pub sort: String,
    /// A smart collection's filter: the section listing query (Plex's syntax)
    /// whose items it shows. `None` for collections that hold items.
    pub smart_filter: Option<String>,
    /// Its category: Other when it has none.
    pub category: crate::categories::CategoryRef,
    /// Where it is a row, in what order (lowest first): Home, and its
    /// library's Recommended tab. `None`: not a row there.
    pub home_position: Option<i64>,
    pub recommended_position: Option<i64>,
    /// One of the collections Pley makes in each library
    /// ([`Builtin`]); `None` for the rest.
    pub builtin: Option<String>,
    /// Items this user can see.
    pub item_count: i64,
    /// Where its poster comes from: `custom`, a picture an administrator
    /// uploaded; `icon`, `icon` in `icon_color` over its first items'
    /// posters; `color`, `icon` on `icon_color` alone; or `automatic`,
    /// TMDB's, else its first items' posters.
    pub artwork: &'static str,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    /// The collection's own artwork, else its first visible item's.
    pub poster_url: String,
    pub backdrop_url: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl CollectionRow {
    /// Its icon is drawn on its color alone.
    fn on_color(&self) -> bool {
        self.poster_background.as_deref() == Some("color")
    }

    /// The poster URL's version: it changes with what the poster is drawn
    /// from, even within the second `updated_at` counts in.
    fn poster_version(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(self.updated_at.to_le_bytes());
        for part in [&self.custom_poster, &self.icon, &self.icon_color, &self.poster_background] {
            hash.update(part.as_deref().unwrap_or_default());
            hash.update([0]);
        }
        hash.finalize().iter().take(6).map(|b| format!("{b:02x}")).collect()
    }

    fn into_dto(self, item_count: i64, names: &Names) -> CollectionDto {
        let untouched = names.untouched(&self).is_some();
        let poster_version = self.poster_version();
        let on_color = self.on_color();
        let title = names.title(&self);
        CollectionDto {
            // Pley's own, named in the user's language, sort by that name.
            sort_title: if untouched { crate::util::sort_title(&title) } else { self.sort_title },
            title,
            category: self.category_id.and_then(|id| names.categories.get(&id)).unwrap_or(&names.other).clone(),
            home_position: self.home_position,
            recommended_position: self.recommended_position,
            builtin: self.builtin,
            artwork: match (&self.custom_poster, &self.icon) {
                (Some(_), _) => "custom",
                (None, Some(_)) if on_color => "color",
                (None, Some(_)) => "icon",
                (None, None) => "automatic",
            },
            icon_color: self
                .icon
                .as_ref()
                .map(|_| self.icon_color.clone().unwrap_or_else(|| crate::collection_art::DEFAULT_COLOR.to_owned())),
            icon: self.icon,
            poster_url: format!("/api/collections/{}/images/poster?v={}", self.id, poster_version),
            backdrop_url: format!("/api/collections/{}/images/backdrop?v={}", self.id, self.updated_at),
            id: self.id,
            library_id: self.library_id,
            summary: self.summary,
            automatic: self.tmdb_id.is_some(),
            sort: self.sort,
            smart_filter: self.smart_filter,
            item_count,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Visible item counts of collections, for one user.
async fn visible_counts(db: &Db, user_id: i64, ids: &[i64]) -> sqlx::Result<HashMap<i64, i64>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "{} SELECT ci.collection_id, COUNT(*) FROM collection_items ci
           JOIN items i ON i.id = ci.item_id
           LEFT JOIN items p ON p.id = i.parent_id
           LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE {VISIBLE} AND ci.collection_id IN ({placeholders})
         GROUP BY ci.collection_id",
        me_cte!()
    );
    let mut query = sqlx::query_as::<_, (i64, i64)>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
    for id in ids {
        query = query.bind(id);
    }
    Ok(query.fetch_all(&db.read).await?.into_iter().collect())
}

/// Whether a collection with `count` visible items is listed for the user.
/// Administrators also see a TMDB collection that is down to one title
/// while it is a row on Home or Recommended, so they can take it off.
fn listed(row: &CollectionRow, count: i64, user: &AuthUser) -> bool {
    if row.tmdb_id.is_some() {
        count >= MIN_AUTOMATIC || (user.is_admin && (row.home_position.is_some() || row.recommended_position.is_some()))
    } else {
        count > 0 || user.is_admin
    }
}

/// [`list`] as the Plex- and Jellyfin-compatible APIs show it: without
/// Pley's own four, which stand for those servers' built-in rows there.
pub async fn list_shared(
    db: &Db,
    user: &AuthUser,
    library_id: i64,
    language: &'static str,
) -> sqlx::Result<Vec<CollectionDto>> {
    let mut all = list(db, user, library_id, language).await?;
    all.retain(|c| c.builtin.is_none());
    Ok(all)
}

/// Collections of a library (the caller checked library access), named in
/// `language`.
pub async fn list(
    db: &Db,
    user: &AuthUser,
    library_id: i64,
    language: &'static str,
) -> sqlx::Result<Vec<CollectionDto>> {
    let names = Names::load(db, language).await?;
    let rows: Vec<CollectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE library_id = ? ORDER BY sort_title COLLATE NOCASE, id"
    )))
    .bind(library_id)
    .fetch_all(&db.read)
    .await?;
    let ids: Vec<i64> = rows.iter().filter(|r| r.smart_filter.is_none()).map(|r| r.id).collect();
    let mut counts = visible_counts(db, user.id, &ids).await?;
    if rows.iter().any(|r| r.smart_filter.is_some())
        && let Some(lib) = crate::library::get_library(db, library_id).await?
    {
        for row in &rows {
            if let Some(filter) = &row.smart_filter {
                let count = crate::listing::smart_count(db, user.id, &lib, filter).await?;
                counts.insert(row.id, i64::try_from(count).unwrap_or(i64::MAX));
            }
        }
    }
    let mut listed: Vec<CollectionDto> = rows
        .into_iter()
        .filter_map(|r| {
            let count = counts.get(&r.id).copied().unwrap_or(0);
            listed(&r, count, user).then(|| r.into_dto(count, &names))
        })
        .collect();
    // By title as the user reads it: Pley's own are named in their language.
    listed.sort_by_cached_key(|c| (c.sort_title.to_lowercase(), c.id));
    Ok(listed)
}

/// Every collection of a library, as stored.
pub(crate) async fn library_rows(db: &Db, library_id: i64) -> sqlx::Result<Vec<CollectionRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE library_id = ? ORDER BY sort_title COLLATE NOCASE, id"
    )))
    .bind(library_id)
    .fetch_all(&db.read)
    .await
}

/// A library's collections that are rows on its Recommended tab, in order.
pub(crate) async fn recommended_rows(db: &Db, library_id: i64) -> sqlx::Result<Vec<CollectionRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE library_id = ? AND recommended_position IS NOT NULL
         ORDER BY recommended_position, sort_title COLLATE NOCASE, id"
    )))
    .bind(library_id)
    .fetch_all(&db.read)
    .await
}

/// Every collection that is a row on Home, in order.
pub(crate) async fn home_rows(db: &Db) -> sqlx::Result<Vec<CollectionRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE home_position IS NOT NULL
         ORDER BY home_position, sort_title COLLATE NOCASE, id"
    )))
    .fetch_all(&db.read)
    .await
}

/// Collections by id, those that exist, in the order of `ids`.
pub(crate) async fn rows(db: &Db, ids: &[i64]) -> sqlx::Result<Vec<CollectionRow>> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(row) = row(db, *id).await?
            && !out.iter().any(|r: &CollectionRow| r.id == row.id)
        {
            out.push(row);
        }
    }
    Ok(out)
}

/// Collection `id`'s icon, its color, and whether it's drawn on the color
/// alone; `None`: no such collection.
pub async fn icon_of(db: &Db, id: i64) -> sqlx::Result<Option<(Option<String>, Option<String>, bool)>> {
    Ok(row(db, id).await?.map(|r| {
        let on_color = r.on_color();
        (r.icon, r.icon_color, on_color)
    }))
}

async fn row(db: &Db, id: i64) -> sqlx::Result<Option<CollectionRow>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM collections WHERE id = ?")))
        .bind(id)
        .fetch_optional(&db.read)
        .await
}

/// Item ids of a collection in its display order.
async fn ordered_ids(db: &Db, id: i64, sort: &str) -> sqlx::Result<Vec<i64>> {
    let order = match sort {
        "title" => "i.sort_title, i.year",
        "custom" => "ci.position, i.sort_title",
        _ => {
            "COALESCE(i.release_date, CAST(i.year AS TEXT)) IS NULL, COALESCE(i.release_date, CAST(i.year AS TEXT)), i.sort_title"
        }
    };
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT ci.item_id FROM collection_items ci JOIN items i ON i.id = ci.item_id
         WHERE ci.collection_id = ? ORDER BY {order}"
    )))
    .bind(id)
    .fetch_all(&db.read)
    .await
}

/// The collection and its visible items, if the user may see it, named in
/// `language`. Library access is checked here too.
pub async fn get(
    db: &Db,
    user: &AuthUser,
    id: i64,
    language: &'static str,
) -> sqlx::Result<Option<(CollectionDto, Vec<ItemRow>)>> {
    let Some(row) = row(db, id).await? else { return Ok(None) };
    let Some(items) = visible_items(db, user, &row, None).await? else { return Ok(None) };
    let count = items.len() as i64;
    Ok(Some((row.into_dto(count, &Names::load(db, language).await?), items)))
}

/// What a collection shows the user, in its order: `None` when they may not
/// see it (no access to its library, or nothing in it for them). A smart
/// collection's first `most` alone when it's given, as a row shows them:
/// its filter may find a whole library.
pub(crate) async fn visible_items(
    db: &Db,
    user: &AuthUser,
    row: &CollectionRow,
    most: Option<usize>,
) -> sqlx::Result<Option<Vec<ItemRow>>> {
    if !crate::access::can_open_library(db, user, row.library_id).await? {
        return Ok(None);
    }
    let items = match &row.smart_filter {
        // What the filter finds, in the filter's order.
        Some(filter) => match crate::library::get_library(db, row.library_id).await? {
            Some(lib) => crate::listing::smart_items_up_to(db, user.id, &lib, filter, most).await?,
            None => Vec::new(),
        },
        None => catalog::items_by_ids(db, user.id, &ordered_ids(db, row.id, &row.sort).await?).await?,
    };
    if items.is_empty() && !(user.is_admin && row.tmdb_id.is_none()) {
        return Ok(None);
    }
    Ok(Some(items))
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CollectionRef {
    pub id: i64,
    pub title: String,
}

/// Collections an item belongs to (the caller checked the item is visible).
pub async fn for_item(db: &Db, user: &AuthUser, item_id: i64) -> sqlx::Result<Vec<CollectionRef>> {
    let rows: Vec<CollectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM collections WHERE id IN (SELECT collection_id FROM collection_items WHERE item_id = ?)
         ORDER BY sort_title COLLATE NOCASE"
    )))
    .bind(item_id)
    .fetch_all(&db.read)
    .await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let counts = visible_counts(db, user.id, &ids).await?;
    Ok(rows
        .into_iter()
        .filter(|r| listed(r, counts.get(&r.id).copied().unwrap_or(0), user))
        .map(|r| CollectionRef { id: r.id, title: r.title })
        .collect())
}

pub async fn library_of(db: &Db, id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT library_id FROM collections WHERE id = ?").bind(id).fetch_optional(&db.read).await
}

/// A smart collection of `library_id`, filled by `filter` (see [`CollectionDto::smart_filter`]).
pub async fn create_smart(db: &Db, library_id: i64, title: &str, filter: &str) -> sqlx::Result<i64> {
    let id = create(db, library_id, title, None).await?;
    set_smart_filter(db, id, filter).await?;
    Ok(id)
}

pub async fn set_smart_filter(db: &Db, id: i64, filter: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE collections SET smart_filter = ?, updated_at = ? WHERE id = ?")
        .bind(filter)
        .bind(now())
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(())
}

/// Whether the collection is a smart one (it holds no items of its own).
pub async fn is_smart(db: &Db, id: i64) -> sqlx::Result<bool> {
    Ok(sqlx::query_scalar::<_, Option<String>>("SELECT smart_filter FROM collections WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.read)
        .await?
        .flatten()
        .is_some())
}

pub async fn create(db: &Db, library_id: i64, title: &str, summary: Option<&str>) -> sqlx::Result<i64> {
    let now = now();
    sqlx::query_scalar(
        "INSERT INTO collections (library_id, title, sort_title, summary, sort, edited, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'release', 1, ?, ?) RETURNING id",
    )
    .bind(library_id)
    .bind(title)
    .bind(sort_title(title))
    .bind(summary)
    .bind(now)
    .bind(now)
    .fetch_one(&db.write)
    .await
}

pub async fn update(
    db: &Db,
    id: i64,
    title: Option<&str>,
    summary: Option<Option<&str>>,
    sort: Option<&str>,
) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    // `edited` (SET reads the old row) only when a value really changes.
    if let Some(title) = title {
        sqlx::query("UPDATE collections SET title = ?, sort_title = ?, edited = edited OR title IS NOT ? WHERE id = ?")
            .bind(title)
            .bind(sort_title(title))
            .bind(title)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(summary) = summary {
        sqlx::query("UPDATE collections SET summary = ?, edited = edited OR summary IS NOT ? WHERE id = ?")
            .bind(summary)
            .bind(summary)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(sort) = sort {
        sqlx::query("UPDATE collections SET sort = ? WHERE id = ?").bind(sort).bind(id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE collections SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&mut *tx).await?;
    tx.commit().await
}

pub async fn delete(db: &Db, id: i64) -> sqlx::Result<bool> {
    let deleted =
        sqlx::query("DELETE FROM collections WHERE id = ?").bind(id).execute(&db.write).await?.rows_affected();
    Ok(deleted > 0)
}

/// Removes a library's collections from TMDB, once it no longer makes them:
/// all of them, edited or not, since another tool (such as Kometa) makes
/// them instead and one kept would sit beside its own. Returns how many went.
pub async fn drop_tmdb(db: &Db, library_id: i64) -> sqlx::Result<u64> {
    let dropped = sqlx::query("DELETE FROM collections WHERE library_id = ? AND tmdb_id IS NOT NULL")
        .bind(library_id)
        .execute(&db.write)
        .await?
        .rows_affected();
    Ok(dropped)
}

/// How many of `item_ids` are movies or shows of the library (what a
/// collection of it can hold).
pub async fn eligible(db: &Db, library_id: i64, item_ids: &[i64]) -> sqlx::Result<i64> {
    if item_ids.is_empty() {
        return Ok(0);
    }
    let placeholders = vec!["?"; item_ids.len()].join(",");
    let sql = format!(
        "SELECT COUNT(*) FROM items WHERE library_id = ? AND kind IN ('movie', 'show') AND id IN ({placeholders})"
    );
    let mut query = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(library_id);
    for id in item_ids {
        query = query.bind(id);
    }
    query.fetch_one(&db.read).await
}

/// Add movies or shows of the collection's library; others are ignored.
/// Returns how many were added.
pub async fn add_items(db: &Db, id: i64, library_id: i64, item_ids: &[i64]) -> sqlx::Result<u64> {
    let mut tx = db.write.begin().await?;
    let mut added = 0;
    for item in item_ids {
        added += sqlx::query(
            "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position)
             SELECT ?, i.id, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_items WHERE collection_id = ?)
             FROM items i WHERE i.id = ? AND i.library_id = ? AND i.kind IN ('movie', 'show')",
        )
        .bind(id)
        .bind(id)
        .bind(item)
        .bind(library_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    sqlx::query("UPDATE collections SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(added)
}

pub async fn remove_item(db: &Db, id: i64, item_id: i64) -> sqlx::Result<bool> {
    let removed = sqlx::query("DELETE FROM collection_items WHERE collection_id = ? AND item_id = ?")
        .bind(id)
        .bind(item_id)
        .execute(&db.write)
        .await?
        .rows_affected();
    if removed > 0 {
        sqlx::query("UPDATE collections SET updated_at = ? WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&db.write)
            .await?;
    }
    Ok(removed > 0)
}

/// Custom order: `item_ids` first in that order, anything not listed after.
pub async fn reorder(db: &Db, id: i64, item_ids: &[i64]) -> sqlx::Result<()> {
    let current = ordered_ids(db, id, "custom").await?;
    let order = crate::playlists::merged_order(&current, item_ids);
    let mut tx = db.write.begin().await?;
    for (position, item) in order.iter().enumerate() {
        sqlx::query("UPDATE collection_items SET position = ? WHERE collection_id = ? AND item_id = ?")
            .bind(position as i64)
            .bind(id)
            .bind(item)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE collections SET sort = 'custom', updated_at = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Move an item right after another one, or first (`after: None`), in the
/// order the collection shows now, which it keeps from then on (`custom`).
/// False when either isn't in the collection.
pub async fn move_item(db: &Db, id: i64, item_id: i64, after: Option<i64>) -> sqlx::Result<bool> {
    let Some(row) = row(db, id).await? else { return Ok(false) };
    let mut order = ordered_ids(db, id, &row.sort).await?;
    let Some(from) = order.iter().position(|i| *i == item_id) else { return Ok(false) };
    order.remove(from);
    let to = match after {
        Some(after) => match order.iter().position(|i| *i == after) {
            Some(at) => at + 1,
            None => return Ok(false),
        },
        None => 0,
    };
    order.insert(to, item_id);
    reorder(db, id, &order).await?;
    Ok(true)
}

/// What a collection's poster or backdrop is ([`artwork`]).
pub enum Artwork {
    /// A picture of its own.
    File(PathBuf),
    /// One drawn ([`crate::collection_art`]) from the posters of the first
    /// [`DRAWN_POSTERS`] of these items whose posters can be read, and the
    /// icon (its name and its color's), if any. Without an icon, the first
    /// item's poster when fewer than [`DRAWN_POSTERS_ALONE`] can be read.
    Drawn { items: Vec<i64>, icon: Option<(String, String)> },
    /// This item's.
    Item(i64),
}

/// The items whose posters a drawn poster shows.
pub const DRAWN_POSTERS: usize = 4;

/// Without an icon, the posters a drawn poster needs: from one, it would
/// only be that item's own poster.
pub const DRAWN_POSTERS_ALONE: usize = 2;

/// Items a drawn poster may take its posters from: a few more than it
/// shows, for posters that can't be read.
const DRAWN_CANDIDATES: usize = 8;

/// A collection's artwork of `kind` (`poster` or `backdrop`), for `user`.
/// The poster is the uploaded one, else its icon over its first visible
/// items' posters (or on its color alone), else TMDB's, else those posters in a grid when there are
/// two or more; and a backdrop its own. Else the first visible item's that has
/// such artwork; a poster with none, its library's icon on slate, and a
/// backdrop the first item's.
pub async fn artwork(db: &Db, store: &Store, user: &AuthUser, id: i64, kind: &str) -> sqlx::Result<Option<Artwork>> {
    let Some(row) = row(db, id).await? else { return Ok(None) };
    let Some(items) = visible_items(db, user, &row, None).await? else { return Ok(None) };
    let file = |r: Option<String>| r.and_then(|r| store.path(&r)).filter(|p| p.is_file());
    let poster = kind == "poster";
    let with_posters: Vec<i64> = items.iter().filter(|i| i.has_poster).take(DRAWN_CANDIDATES).map(|i| i.id).collect();
    let on_color = row.on_color();
    if poster {
        if let Some(path) = file(row.custom_poster) {
            return Ok(Some(Artwork::File(path)));
        }
        if let Some(icon) = row.icon {
            let color = row.icon_color.unwrap_or_else(|| crate::collection_art::DEFAULT_COLOR.to_owned());
            let items = if on_color { Vec::new() } else { with_posters };
            return Ok(Some(Artwork::Drawn { items, icon: Some((icon, color)) }));
        }
    }
    if let Some(path) = file(if poster { row.poster } else { row.backdrop }) {
        return Ok(Some(Artwork::File(path)));
    }
    if poster && with_posters.len() >= DRAWN_POSTERS_ALONE {
        return Ok(Some(Artwork::Drawn { items: with_posters, icon: None }));
    }
    let with_art = items.iter().find(|i| if poster { i.has_poster } else { i.has_backdrop });
    if poster && with_art.is_none() {
        // No poster to show: its library's icon on a quiet color, the same
        // on every app, rather than each app's own placeholder.
        let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM libraries WHERE id = ?")
            .bind(row.library_id)
            .fetch_optional(&db.read)
            .await?;
        let kind = kind.as_deref().and_then(LibraryKind::parse).unwrap_or(LibraryKind::Movies);
        let icon = crate::collection_art::default_icon(kind).to_owned();
        return Ok(Some(Artwork::Drawn {
            items: Vec::new(),
            icon: Some((icon, crate::collection_art::EMPTY_COLOR.to_owned())),
        }));
    }
    Ok(with_art.or(items.first()).map(|i| Artwork::Item(i.id)))
}

/// Sets or clears (`None`) the poster an administrator uploaded for
/// collection `id`; either way, it has no icon after.
pub async fn set_custom_poster(db: &Db, id: i64, blob: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE collections SET custom_poster = ?, icon = NULL, icon_color = NULL, poster_background = NULL,
         updated_at = ? WHERE id = ?",
    )
    .bind(blob)
    .bind(now())
    .bind(id)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// Gives collection `id` an icon in a color (names checked by the caller),
/// over its films' posters or (`on_color`) on the color alone; or none
/// (`None`). An uploaded poster goes either way.
pub async fn set_icon(db: &Db, id: i64, icon: Option<(&str, &str)>, on_color: bool) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE collections SET icon = ?, icon_color = ?, poster_background = ?, custom_poster = NULL, updated_at = ?
         WHERE id = ?",
    )
    .bind(icon.map(|(name, _)| name))
    .bind(icon.map(|(_, color)| color))
    .bind(icon.filter(|_| on_color).map(|_| "color"))
    .bind(now())
    .bind(id)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// Makes the collections Pley makes in a movie or TV library ([`Builtin`])
/// that it doesn't have: all of them for a new library, those deleted for
/// "Restore". Each goes under Highlights, last on the library's Recommended
/// tab (all but Top Unwatched), and Recently Added last on Home, as a new
/// library's row was.
/// Returns how many were made.
pub async fn restore_defaults(db: &Db, library_id: i64) -> sqlx::Result<u64> {
    let Some(lib) = crate::library::get_library(db, library_id).await? else { return Ok(0) };
    let kind = lib.kind();
    if !matches!(kind, LibraryKind::Movies | LibraryKind::Shows) {
        return Ok(0);
    }
    let now = now();
    let mut tx = db.write.begin().await?;
    let mut made = 0;
    for builtin in Builtin::ALL {
        let title = i18n::t("en", builtin.title_key(kind));
        let home = if builtin == Builtin::RecentlyAdded { Some(next_home_position(&mut tx).await?) } else { None };
        made += sqlx::query(
            "INSERT INTO collections (library_id, title, sort_title, sort, smart_filter, builtin, icon, icon_color,
                category_id, category_from, home_position, recommended_position, created_at, updated_at)
             SELECT ?, ?, ?, 'release', ?, ?, ?, ?, (SELECT id FROM collection_categories WHERE key = 'highlights'),
                'pley',
                ?, CASE WHEN ? IS NOT NULL THEN (SELECT COALESCE(MAX(recommended_position) + 1, ?)
                    FROM collections WHERE library_id = ?) END, ?, ?
             WHERE NOT EXISTS (SELECT 1 FROM collections WHERE library_id = ? AND builtin = ?)",
        )
        .bind(library_id)
        .bind(&title)
        .bind(sort_title(&title))
        .bind(builtin.filter(kind))
        .bind(builtin.key())
        .bind(builtin.icon().0)
        .bind(builtin.icon().1)
        .bind(home)
        .bind(builtin.position())
        .bind(builtin.position())
        .bind(library_id)
        .bind(now)
        .bind(now)
        .bind(library_id)
        .bind(builtin.key())
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    tx.commit().await?;
    Ok(made)
}

/// After every row on Home: collections' and music and audiobook
/// libraries' Recently Added.
pub(crate) async fn next_home_position(tx: &mut sqlx::SqliteConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(p) + 1, 0) FROM (SELECT home_position AS p FROM collections
         UNION ALL SELECT home_position FROM libraries)",
    )
    .fetch_one(&mut *tx)
    .await
}

/// Puts a collection under a category (`None`, or Other's id: none), as an
/// administrator's choice, which labels don't change afterwards.
pub async fn set_category(db: &Db, id: i64, category_id: Option<i64>) -> sqlx::Result<()> {
    let category_id = crate::categories::stored(db, category_id).await?;
    sqlx::query("UPDATE collections SET category_id = ?, category_from = 'admin', updated_at = ? WHERE id = ?")
        .bind(category_id)
        .bind(now())
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(())
}

/// Shows a collection as a row on Home or takes it off. On Home it joins
/// the row of collections of the same title, else comes last.
pub async fn set_on_home(db: &Db, id: i64, on: bool) -> sqlx::Result<()> {
    let position = match (on, row(db, id).await?) {
        (false, _) | (_, None) => None,
        (true, Some(row)) if row.home_position.is_some() => row.home_position,
        (true, Some(row)) => {
            // The row it joins is found as Home groups rows (any language
            // does: the grouping is the same in all of them).
            let names = Names::load(db, "en").await?;
            let group = names.home_group(&row);
            let shared = home_rows(db)
                .await?
                .iter()
                .filter(|other| other.id != id && names.home_group(other) == group)
                .filter_map(|other| other.home_position)
                .min();
            match shared {
                Some(position) => Some(position),
                None => next_home_position(&mut *db.write.acquire().await?).await.map(Some)?,
            }
        }
    };
    sqlx::query("UPDATE collections SET home_position = ?, updated_at = ? WHERE id = ?")
        .bind(position)
        .bind(now())
        .bind(id)
        .execute(&db.write)
        .await?;
    Ok(())
}

/// Shows a collection as a row on its library's Recommended tab (last) or
/// takes it off.
pub async fn set_on_recommended(db: &Db, id: i64, on: bool) -> sqlx::Result<()> {
    let sql = if on {
        "UPDATE collections SET recommended_position = COALESCE(recommended_position,
            (SELECT COALESCE(MAX(c.recommended_position) + 1, 0) FROM collections c
             WHERE c.library_id = collections.library_id)), updated_at = ? WHERE id = ?"
    } else {
        "UPDATE collections SET recommended_position = NULL, updated_at = ? WHERE id = ?"
    };
    sqlx::query(sql).bind(now()).bind(id).execute(&db.write).await?;
    Ok(())
}

/// The order of a library's Recommended tab: `ids` (its collections that
/// are rows there) first in this order, the others after as they were.
pub async fn order_recommended(db: &Db, library_id: i64, ids: &[i64]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    let current: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM collections WHERE library_id = ? AND recommended_position IS NOT NULL
         ORDER BY recommended_position, sort_title, id",
    )
    .bind(library_id)
    .fetch_all(&mut *tx)
    .await?;
    let listed: Vec<i64> = ids.iter().copied().filter(|id| current.contains(id)).collect();
    for (position, id) in crate::playlists::merged_order(&current, &listed).iter().enumerate() {
        sqlx::query("UPDATE collections SET recommended_position = ? WHERE id = ?")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

async fn download(store: &Store, remote: &str, size: &str) -> Option<String> {
    store.remote(&format!("{IMAGES}/{size}{remote}")).await.ok()
}

/// Put a movie into its TMDB collection (creating it), take it out of any
/// other automatic one, and drop automatic collections left empty; nothing
/// in a library that doesn't make them.
pub async fn link_tmdb(db: &Db, store: &Store, item_id: i64, found: Option<&TmdbCollection>) -> anyhow::Result<()> {
    let library_id: i64 =
        sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?").bind(item_id).fetch_one(&db.read).await?;
    let found = found.filter(|c| !c.name.trim().is_empty());
    let now = now();
    // Membership changes in one transaction: a concurrent refresh must not
    // drop a collection between its creation and its first item.
    let mut tx = db.write.begin().await?;
    // Asked under the writer, so a library turned off meanwhile (see
    // `drop_tmdb`) gets none back from a refresh already under way.
    let makes: bool = sqlx::query_scalar("SELECT tmdb_collections FROM libraries WHERE id = ?")
        .bind(library_id)
        .fetch_one(&mut *tx)
        .await?;
    if !makes {
        return Ok(());
    }
    sqlx::query(
        "DELETE FROM collection_items WHERE item_id = ?
         AND collection_id IN (SELECT id FROM collections WHERE tmdb_id IS NOT NULL AND tmdb_id != ?)",
    )
    .bind(item_id)
    .bind(found.map(|c| c.id).unwrap_or(-1))
    .execute(&mut *tx)
    .await?;
    let mut linked = None;
    if let Some(c) = found {
        let (id, poster, backdrop): (i64, Option<String>, Option<String>) = sqlx::query_as(
            "INSERT INTO collections (library_id, title, sort_title, tmdb_id, category_id, category_from,
                created_at, updated_at)
             VALUES (?, ?, ?, ?, (SELECT id FROM collection_categories WHERE key = 'box_sets'), 'pley', ?, ?)
             ON CONFLICT(library_id, tmdb_id) WHERE tmdb_id IS NOT NULL DO UPDATE SET
                title = CASE WHEN collections.edited THEN collections.title ELSE excluded.title END,
                sort_title = CASE WHEN collections.edited THEN collections.sort_title ELSE excluded.sort_title END
             RETURNING id, poster, backdrop",
        )
        .bind(library_id)
        .bind(c.name.trim())
        .bind(sort_title(c.name.trim()))
        .bind(c.id)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position)
             VALUES (?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_items WHERE collection_id = ?))",
        )
        .bind(id)
        .bind(item_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        linked = Some((c, id, poster, backdrop));
    }
    let emptied: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM collections WHERE tmdb_id IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM collection_items WHERE collection_id = collections.id)
         RETURNING id",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    // Artwork is fetched once, outside the transaction.
    if let Some((c, id, poster, backdrop)) = linked {
        let missing = |p: &Option<String>| p.as_deref().is_none_or(|p| !store.exists(p));
        let new_poster = match (&c.poster_path, missing(&poster)) {
            (Some(remote), true) => download(store, remote, "w780").await,
            _ => None,
        };
        let new_backdrop = match (&c.backdrop_path, missing(&backdrop)) {
            (Some(remote), true) => download(store, remote, "w1280").await,
            _ => None,
        };
        if new_poster.is_some() || new_backdrop.is_some() {
            sqlx::query(
                "UPDATE collections SET poster = COALESCE(?, poster), backdrop = COALESCE(?, backdrop), updated_at = ?
                 WHERE id = ?",
            )
            .bind(new_poster)
            .bind(new_backdrop)
            .bind(now)
            .bind(id)
            .execute(&db.write)
            .await?;
        }
    }
    if !emptied.is_empty() {
        tracing::debug!("removed {} empty automatic collections", emptied.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_collection_parses() {
        let json = r#"{"id": 1241, "name": "Harry Potter Collection", "poster_path": "/p.jpg", "backdrop_path": null}"#;
        let c: TmdbCollection = serde_json::from_str(json).unwrap();
        assert_eq!(c.id, 1241);
        assert_eq!(c.poster_path.as_deref(), Some("/p.jpg"));
        assert!(c.backdrop_path.is_none());
    }
}
