//! Libraries and their listings, hubs, collections, playlists, history and search.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use super::{ItemSummary, LiveChannel, PersonSummary, nd, wire_enum};

wire_enum! {
    /// What a library holds (apps/server/src/library/mod.rs, `LibraryKind`).
    pub enum LibraryKind or Unknown {
        Movies = "movies",
        Shows = "shows",
        Music = "music",
        #[default]
        Unknown = "unknown",
    }
}

/// A library: `GET /api/libraries` (in the profile's order) and `…/{id}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default, deserialize_with = "nd")]
    pub kind: LibraryKind,
    /// Always empty for an app's token.
    #[serde(default, deserialize_with = "nd")]
    pub paths: Vec<String>,
    /// Top-level items this profile sees: movies, shows or artists.
    #[serde(default, deserialize_with = "nd")]
    pub item_count: i64,
    #[serde(default, deserialize_with = "nd")]
    pub created_at: i64,
    pub scanned_at: Option<i64>,
    /// A scan is queued or running.
    #[serde(default, deserialize_with = "nd")]
    pub scanning: bool,
}

/// `{ ids }`: the body of `PUT /api/me/libraries/order`, every library the profile may open once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryIds {
    #[serde(default, deserialize_with = "nd")]
    pub ids: Vec<i64>,
}

/// A page of `GET /api/libraries/{id}/items`: `total` counts every match.
/// Items leave out backdrops and stills, and carry `media` (older servers only sorted by bitrate).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemsPage {
    #[serde(default, deserialize_with = "nd")]
    pub total: i64,
    #[serde(default, deserialize_with = "nd")]
    pub items: Vec<ItemSummary>,
}

/// `GET /api/libraries/{id}/filters`: the genres (by name) and years (newest first) to filter by,
/// and the resolutions and dynamic ranges, best first, when the titles have two or more (none
/// from an older server, nor for music).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryFilters {
    #[serde(default, deserialize_with = "nd")]
    pub genres: Vec<String>,
    #[serde(default, deserialize_with = "nd")]
    pub years: Vec<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub resolutions: Vec<ResolutionFilter>,
    #[serde(default, deserialize_with = "nd")]
    pub dynamic_ranges: Vec<RangeFilter>,
}

wire_enum! {
    /// A listing's `resolution` filter: the best file's (a show's best episode's), `1080` up to
    /// 1440p, `sd` below 720p.
    pub enum ResolutionFilter or Unknown {
        Uhd = "4k",
        Fhd = "1080",
        Hd = "720",
        Sd = "sd",
        /// A value this app doesn't know: never offered nor sent.
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// A listing's `dynamic_range` filter: the best file's (a show's best episode's); `hdr` is any
    /// but SDR.
    pub enum RangeFilter or Unknown {
        Hdr = "hdr",
        DolbyVision = "dolby_vision",
        Hdr10Plus = "hdr10plus",
        Hdr10 = "hdr10",
        Hlg = "hlg",
        Sdr = "sdr",
        /// A value this app doesn't know: never offered nor sent.
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// The `sort` of a library listing (apps/server/src/catalog.rs, `SortKey`).
    pub enum LibrarySort or Title {
        /// The server's default, ascending.
        #[default]
        Title = "title",
        Year = "year",
        /// Newest first by default; shows by their newest episode.
        Added = "added",
        Rating = "rating",
        UserRating = "user_rating",
        Release = "release",
        Duration = "duration",
        LastPlayed = "last_played",
        /// The best file's bitrate.
        Bitrate = "bitrate",
    }
}

impl LibrarySort {
    /// The order the server sorts by when none is asked for: titles, years
    /// and durations ascending, the rest descending.
    pub fn default_order(self) -> SortOrder {
        match self {
            LibrarySort::Title | LibrarySort::Year | LibrarySort::Duration => SortOrder::Asc,
            _ => SortOrder::Desc,
        }
    }
}

wire_enum! {
    /// The `order` of a listing.
    pub enum SortOrder or Asc {
        #[default]
        Asc = "asc",
        Desc = "desc",
    }
}

/// How a library's listing is asked for: sort, order and filters (kept per
/// library as the view it was left in), plus the page.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryQuery {
    #[serde(default, deserialize_with = "nd")]
    pub sort: LibrarySort,
    /// None: the sort's own order.
    pub order: Option<SortOrder>,
    /// `Some(true)`: watched only; `Some(false)`: unwatched only.
    pub watched: Option<bool>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    /// Views kept before these two filters have neither; a value this app doesn't know reads as
    /// `Unknown`, which the library drops.
    #[serde(default)]
    pub resolution: Option<ResolutionFilter>,
    #[serde(default)]
    pub dynamic_range: Option<RangeFilter>,
}

impl LibraryQuery {
    /// The query string's parameters in the order the TV app sends them
    /// (the server's listing ETag hashes the query as written), the page's included.
    pub fn params(&self, offset: i64, limit: Option<i64>) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("sort", Some(self.sort.as_str().to_string())),
            ("order", self.order.map(|o| o.as_str().to_string())),
            ("watched", self.watched.map(|w| w.to_string())),
            ("genre", self.genre.clone()),
            ("year", self.year.map(|y| y.to_string())),
            ("resolution", self.resolution.map(|r| r.as_str().to_string())),
            ("dynamic_range", self.dynamic_range.map(|r| r.as_str().to_string())),
            ("offset", Some(offset.to_string())),
            ("limit", limit.map(|l| l.to_string())),
        ]
    }

    /// The query in a snapshot key: sort, order, watched, genre, year, resolution and dynamic
    /// range.
    pub fn key(&self) -> String {
        let mut key = format!("{}.{}", self.sort, self.order.unwrap_or(self.sort.default_order()));
        match self.watched {
            Some(true) => key.push_str(".watched"),
            Some(false) => key.push_str(".unwatched"),
            None => {}
        }
        if let Some(g) = &self.genre {
            key.push_str(".g=");
            key.push_str(g);
        }
        if let Some(y) = self.year {
            key.push_str(&format!(".y={y}"));
        }
        if let Some(r) = self.resolution {
            key.push_str(&format!(".r={r}"));
        }
        if let Some(d) = self.dynamic_range {
            key.push_str(&format!(".d={d}"));
        }
        key
    }
}

/// A row of Home or of a library's front page (apps/server/src/api/libraries.rs, `Hub`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hub {
    /// `continue_watching`, `my_list`, `recently_added:<library>` on Home; on a
    /// library's page `continue_watching:<id>`, `recently_added:<id>`,
    /// `recently_released:<id>`, `top_rated:<id>`.
    #[serde(default, deserialize_with = "nd")]
    pub key: String,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    /// None for Home's Continue Watching and My List.
    pub library_id: Option<i64>,
    /// The collections the row shows: opening the row opens them (one collection's page, or the
    /// merged page of several same-titled ones). Empty for Continue Watching, My List and rows
    /// from servers before collections were rows.
    #[serde(default, deserialize_with = "nd")]
    pub collection_ids: Vec<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub items: Vec<ItemSummary>,
}

/// The keys of the hubs the app treats as their own kind.
pub mod hub_keys {
    /// Home's Continue Watching; a library's is `continue_watching:<id>`.
    pub const CONTINUE_WATCHING: &str = "continue_watching";
    /// Home's My List.
    pub const MY_LIST: &str = "my_list";

    /// Whether `key` is Home's or a library's Continue Watching.
    pub fn is_continue_watching(key: &str) -> bool {
        key == CONTINUE_WATCHING || key.strip_prefix(CONTINUE_WATCHING).is_some_and(|rest| rest.starts_with(':'))
    }
}

impl Hub {
    /// Home's or a library's Continue Watching.
    pub fn continue_watching(&self) -> bool {
        hub_keys::is_continue_watching(&self.key)
    }
}

/// `{ hubs }`: `GET /api/hubs/home` and `GET /api/libraries/{id}/hubs`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hubs {
    #[serde(default, deserialize_with = "nd")]
    pub hubs: Vec<Hub>,
}

wire_enum! {
    /// The order of a collection's items.
    pub enum CollectionSort or Unknown {
        Release = "release",
        Title = "title",
        /// The order an administrator gave.
        Custom = "custom",
        #[default]
        Unknown = "unknown",
    }
}

/// A collection of one library's movies or shows (apps/server/src/collections.rs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub library_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    pub summary: Option<String>,
    /// Mirrors a TMDB movie collection.
    #[serde(default, deserialize_with = "nd")]
    pub automatic: bool,
    #[serde(default, deserialize_with = "nd")]
    pub sort: CollectionSort,
    /// Smart collections: the library filter that fills them.
    pub smart_filter: Option<String>,
    /// Its category; `None`: Other (and every collection of a server without categories).
    #[serde(default)]
    pub category: Option<CategoryRef>,
    /// Items this profile can see.
    #[serde(default, deserialize_with = "nd")]
    pub item_count: i64,
    /// Its own artwork, else (by redirect) its first item's.
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub created_at: i64,
    #[serde(default, deserialize_with = "nd")]
    pub updated_at: i64,
}

/// A collection's category as the collection names it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CategoryRef {
    pub id: i64,
    /// `highlights` or `franchises` for Pley's own; `None` for an administrator's.
    #[serde(default)]
    pub key: Option<String>,
    /// In the profile's language for Pley's own.
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
}

/// A category of collections, shared by every library (`GET /api/collection-categories`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionCategory {
    pub id: i64,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
}

/// `{ categories }`: `GET /api/collection-categories`, in the order administrators chose.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionCategories {
    #[serde(default, deserialize_with = "nd")]
    pub categories: Vec<CollectionCategory>,
}

/// A library's collections under one category.
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionGroup<'a> {
    /// The category's id; `None` for collections a server without categories sends.
    pub id: Option<i64>,
    pub title: &'a str,
    pub collections: Vec<&'a CollectionSummary>,
}

/// `collections` by category, in the categories' order (Other among them, where administrators
/// put it); empty groups left out (the web's `groupByCategory`). One of a category not listed yet
/// (made since) goes after the rest under its own category's title; those of a server without
/// categories make one group without a title.
pub fn group_by_category<'a>(
    collections: impl IntoIterator<Item = &'a CollectionSummary>,
    categories: &'a [CollectionCategory],
) -> Vec<CollectionGroup<'a>> {
    let mut groups: Vec<CollectionGroup<'a>> = categories
        .iter()
        .map(|c| CollectionGroup { id: Some(c.id), title: &c.title, collections: Vec::new() })
        .collect();
    for c in collections {
        let id = c.category.as_ref().map(|cat| cat.id);
        match groups.iter_mut().find(|g| g.id == id) {
            Some(g) => g.collections.push(c),
            None => {
                let title = c.category.as_ref().map_or("", |cat| cat.title.as_str());
                groups.push(CollectionGroup { id, title, collections: vec![c] });
            }
        }
    }
    // A server without categories: its uncategorized ones after any that have one.
    if let Some(at) = groups.iter().position(|g| g.id.is_none()) {
        let none = groups.remove(at);
        groups.push(none);
    }
    groups.retain(|g| !g.collections.is_empty());
    groups
}

/// `GET /api/collections/merged?ids=`: a Home row of same-titled collections from several
/// libraries, opened together.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MergedCollection {
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    /// Those the profile can see, each with its library.
    #[serde(default, deserialize_with = "nd")]
    pub collections: Vec<MergedPart>,
    /// Every title of the row, in its order.
    #[serde(default, deserialize_with = "nd")]
    pub items: Vec<ItemSummary>,
}

/// One library's collection of a merged one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MergedPart {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub library_id: i64,
}

/// `GET /api/collections/{id}`: a collection and its items, in its order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionDetail {
    #[serde(flatten)]
    pub collection: CollectionSummary,
    #[serde(default, deserialize_with = "nd")]
    pub items: Vec<ItemSummary>,
}

impl Deref for CollectionDetail {
    type Target = CollectionSummary;

    fn deref(&self) -> &CollectionSummary {
        &self.collection
    }
}

/// `{ collections }`: `GET /api/libraries/{id}/collections`, by title.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionList {
    #[serde(default, deserialize_with = "nd")]
    pub collections: Vec<CollectionSummary>,
}

/// A collection named only: what an item is "Part of".
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionRef {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
}

/// `{ collections }`: `GET /api/items/{id}/collections`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemCollections {
    #[serde(default, deserialize_with = "nd")]
    pub collections: Vec<CollectionRef>,
}

wire_enum! {
    /// What a playlist plays.
    pub enum PlaylistKind or Unknown {
        /// Movies and episodes.
        Video = "video",
        /// Tracks.
        Audio = "audio",
        #[default]
        Unknown = "unknown",
    }
}

/// A playlist of the profile's own (apps/server/src/playlists.rs).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistSummary {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    #[serde(default, deserialize_with = "nd")]
    pub kind: PlaylistKind,
    /// Entries this profile can see.
    #[serde(default, deserialize_with = "nd")]
    pub item_count: i64,
    #[serde(default, deserialize_with = "nd")]
    pub duration_ms: i64,
    /// Picture addresses of the first entries (at most 4), for a mosaic.
    #[serde(default, deserialize_with = "nd")]
    pub artwork: Vec<String>,
    /// Smart playlists: the listing of `library_id` they play.
    pub smart_filter: Option<String>,
    pub library_id: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub created_at: i64,
    #[serde(default, deserialize_with = "nd")]
    pub updated_at: i64,
}

impl PlaylistSummary {
    /// A smart playlist, whose entries can't be added, removed or moved.
    pub fn smart(&self) -> bool {
        self.smart_filter.is_some()
    }
}

/// An entry of a playlist: the same item can be in one more than once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistEntry {
    /// This entry; a smart playlist's are its items' ids.
    pub entry_id: i64,
    pub item: ItemSummary,
}

/// A playlist and its entries: `GET`/`PATCH /api/playlists/{id}`, `POST /api/playlists`, `PUT …/order`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistDetail {
    #[serde(flatten)]
    pub playlist: PlaylistSummary,
    #[serde(default, deserialize_with = "nd")]
    pub entries: Vec<PlaylistEntry>,
}

impl Deref for PlaylistDetail {
    type Target = PlaylistSummary;

    fn deref(&self) -> &PlaylistSummary {
        &self.playlist
    }
}

/// `{ playlists }`: `GET /api/playlists`, by title.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistList {
    #[serde(default, deserialize_with = "nd")]
    pub playlists: Vec<PlaylistSummary>,
}

/// The body of `POST /api/playlists`: a title with items (and a kind, else
/// the first item decides), or a smart playlist: title, library and filter.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewPlaylist {
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<PlaylistKind>,
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "Vec::is_empty")]
    pub item_ids: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smart_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_id: Option<i64>,
}

/// The body of `PATCH /api/playlists/{id}`: what changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Smart playlists only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smart_filter: Option<String>,
}

/// `{ item_ids }`: the body of `POST /api/playlists/{id}/items`; containers add their episodes or tracks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemIds {
    #[serde(default, deserialize_with = "nd")]
    pub item_ids: Vec<i64>,
}

/// `{ entry_ids }`: the body of `PUT /api/playlists/{id}/order`: these first, the rest after.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryIds {
    #[serde(default, deserialize_with = "nd")]
    pub entry_ids: Vec<i64>,
}

/// `{ added }`: how many entries `POST /api/playlists/{id}/items` added.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Added {
    #[serde(default, deserialize_with = "nd")]
    pub added: i64,
}

/// Who played an entry of the history.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryUser {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
}

/// A play, recorded once it reached 90 %, with the device it played on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    /// Unix seconds.
    #[serde(default, deserialize_with = "nd")]
    pub viewed_at: i64,
    pub device: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub user: HistoryUser,
    pub item: ItemSummary,
}

/// A page of `GET /api/history`, newest first; `total` counts every entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryPage {
    #[serde(default, deserialize_with = "nd")]
    pub total: i64,
    #[serde(default, deserialize_with = "nd")]
    pub entries: Vec<HistoryEntry>,
}

/// `GET /api/search`: items by relevance, people, and Live TV channels for
/// those Live TV is open to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    #[serde(default, deserialize_with = "nd")]
    pub items: Vec<ItemSummary>,
    #[serde(default, deserialize_with = "nd")]
    pub people: Vec<PersonSummary>,
    #[serde(default, deserialize_with = "nd")]
    pub channels: Vec<LiveChannel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(id: i64, title: &str) -> CollectionCategory {
        CollectionCategory { id, key: None, title: title.into() }
    }

    fn collection(id: i64, category: Option<(i64, &str)>) -> CollectionSummary {
        let category = category.map(|(id, title)| CategoryRef { id, key: None, title: title.into() });
        CollectionSummary { id, category, ..CollectionSummary::default() }
    }

    #[test]
    fn collections_group_in_the_categories_order_other_where_it_is_listed() {
        let categories =
            [category(1, "Highlights"), category(4, "Other"), category(2, "Franchises"), category(3, "Empty")];
        // 9: a category made since the categories came, so after the rest under its own title.
        let list = [
            collection(10, Some((2, "Franchises"))),
            collection(11, Some((4, "Other"))),
            collection(12, Some((1, "Highlights"))),
            collection(13, Some((9, "New"))),
            collection(14, Some((4, "Other"))),
        ];
        let groups = group_by_category(&list, &categories);
        let shape: Vec<(Option<i64>, &str, Vec<i64>)> =
            groups.iter().map(|g| (g.id, g.title, g.collections.iter().map(|c| c.id).collect())).collect();
        assert_eq!(
            shape,
            [
                (Some(1), "Highlights", vec![12]),
                (Some(4), "Other", vec![11, 14]),
                (Some(2), "Franchises", vec![10]),
                (Some(9), "New", vec![13])
            ]
        );
        // A server without categories: one group without a title.
        let old = [collection(20, None), collection(21, None)];
        let groups = group_by_category(&old, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!((groups[0].id, groups[0].title, groups[0].collections.len()), (None, "", 2));
    }

    #[test]
    fn a_hub_and_a_collection_read_their_new_fields() {
        let hub: Hub = serde_json::from_str(
            r#"{"key":"collections:4,9","title":"Holiday","library_id":null,"collection_ids":[4,9],"items":[]}"#,
        )
        .unwrap();
        assert_eq!(hub.collection_ids, [4, 9]);
        let old: Hub =
            serde_json::from_str(r#"{"key":"recently_added:1","title":"Recently Added","library_id":1,"items":[]}"#)
                .unwrap();
        assert!(old.collection_ids.is_empty());
        let c: CollectionSummary = serde_json::from_str(
            r#"{"id":4,"category":{"id":2,"key":"franchises","title":"Franchises"},"home_position":0}"#,
        )
        .unwrap();
        assert_eq!(c.category.map(|c| c.title), Some("Franchises".to_string()));
    }
}
