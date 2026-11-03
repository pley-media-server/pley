//! The catalogue's JSON as apps get it (docs/API.md, "Types"): the field
//! names, which fields are always there (`null` when empty) and which are left
//! out when empty, as the server's `skip_serializing_if` has them.

use serde::Serialize;

pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Movie,
    Show,
    Season,
    Episode,
    Artist,
    Album,
    Track,
    Extra,
}

impl Kind {
    /// Items with files of their own.
    pub fn playable(self) -> bool {
        matches!(self, Kind::Movie | Kind::Episode | Kind::Track | Kind::Extra)
    }
}

/// A profile's state of an item.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct UserState {
    pub position_ms: i64,
    pub watched: bool,
    pub play_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<i64>,
    /// The profile's own rating, 0 to 10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
}

/// The best file's numbers, in listings sorted by bitrate.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct MediaSummary {
    pub bitrate: Option<i64>,
    pub size: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub dynamic_range: Option<&'static str>,
}

/// An item as listings give it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ItemSummary {
    pub id: i64,
    pub library_id: i64,
    pub kind: Kind,
    pub title: String,
    pub sort_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grandparent_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grandparent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    pub added_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    pub user_state: UserState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewed_leaf_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSummary>,
}

/// An item with everything about it: GET /api/items/{id}.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ItemDetail {
    #[serde(flatten)]
    pub base: ItemSummary,
    pub original_title: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub studio: Option<String>,
    pub content_rating: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub locked_fields: Vec<String>,
    pub files: Vec<MediaFile>,
    pub markers: Vec<Marker>,
    pub extras: Vec<ItemSummary>,
    pub cast: Vec<Credit>,
    pub crew: Vec<Credit>,
    pub in_watchlist: bool,
}

impl ItemDetail {
    /// An item with nothing but its identity filled in.
    pub fn new(id: i64, library_id: i64, kind: Kind, title: &str, sort_title: &str, added_at: i64) -> ItemDetail {
        ItemDetail {
            base: ItemSummary {
                id,
                library_id,
                kind,
                title: title.to_string(),
                sort_title: sort_title.to_string(),
                year: None,
                index: None,
                parent_id: None,
                parent_title: None,
                parent_index: None,
                grandparent_id: None,
                grandparent_title: None,
                duration_ms: None,
                rating: None,
                added_at,
                poster_url: None,
                backdrop_url: None,
                thumb_url: None,
                user_state: UserState::default(),
                leaf_count: None,
                viewed_leaf_count: None,
                child_count: None,
                summary: None,
                release_date: None,
                extra_type: None,
                media: None,
            },
            original_title: None,
            tagline: None,
            genres: Vec::new(),
            studio: None,
            content_rating: None,
            tmdb_id: None,
            imdb_id: None,
            locked_fields: Vec::new(),
            files: Vec::new(),
            markers: Vec::new(),
            extras: Vec::new(),
            cast: Vec::new(),
            crew: Vec::new(),
            in_watchlist: false,
        }
    }

    pub fn id(&self) -> i64 {
        self.base.id
    }

    pub fn kind(&self) -> Kind {
        self.base.kind
    }
}

/// A file of an item.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct MediaFile {
    pub id: i64,
    /// The file's name only, as apps see it.
    pub path: String,
    pub size: i64,
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    /// Bits per second.
    pub bitrate: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub streams: Vec<MediaStream>,
    pub edition: Option<String>,
    pub selected_audio: Option<i64>,
    pub audio_picked: bool,
    pub selected_subtitle: Option<i64>,
    pub chapters: Vec<Chapter>,
}

/// A stream of a file, as the transcoder's `MediaStream` serialises.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct MediaStream {
    pub index: i64,
    pub kind: &'static str,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pix_fmt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interlaced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hdr: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_range: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_profile: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_level: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dv_bl_compat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hdr10plus: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_based: Option<bool>,
    #[serde(skip_serializing_if = "is_false")]
    pub external: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub added: bool,
}

impl MediaStream {
    pub fn is(&self, kind: &str) -> bool {
        self.kind == kind
    }
}

/// A chapter of a file.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Chapter {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
}

/// Where an intro or credits are in a file.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Marker {
    pub file_id: i64,
    pub kind: &'static str,
    pub start_ms: i64,
    pub end_ms: i64,
    pub source: &'static str,
}

/// A person credited in a title.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Credit {
    pub id: i64,
    pub name: String,
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
}

/// A library.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Library {
    pub id: i64,
    pub name: String,
    pub kind: &'static str,
    /// Always empty for apps.
    pub paths: Vec<String>,
    pub item_count: i64,
    pub created_at: i64,
    pub scanned_at: Option<i64>,
    pub scanning: bool,
}

/// A row of Home or of a library's page.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Hub {
    pub key: String,
    pub title: String,
    pub library_id: Option<i64>,
    pub items: Vec<ItemSummary>,
}

/// A collection, without its items.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CollectionSummary {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub summary: Option<String>,
    pub automatic: bool,
    pub sort: &'static str,
    pub smart_filter: Option<String>,
    pub item_count: i64,
    pub poster_url: String,
    pub backdrop_url: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A collection with its items.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CollectionDetail {
    #[serde(flatten)]
    pub base: CollectionSummary,
    pub items: Vec<ItemSummary>,
}

/// A playlist, without its entries.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PlaylistSummary {
    pub id: i64,
    pub title: String,
    pub kind: &'static str,
    pub item_count: i64,
    pub duration_ms: i64,
    pub artwork: Vec<String>,
    pub smart_filter: Option<String>,
    pub library_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One entry of a playlist.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PlaylistEntry {
    pub entry_id: i64,
    pub item: ItemSummary,
}

/// A playlist with its entries.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PlaylistDetail {
    #[serde(flatten)]
    pub base: PlaylistSummary,
    pub entries: Vec<PlaylistEntry>,
}

/// Who played something, in watch history.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct HistoryUser {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

/// One play.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct HistoryEntry {
    pub id: i64,
    pub viewed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    pub user: HistoryUser,
    pub item: ItemSummary,
}

/// A person in search results.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PersonSummary {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
}

/// What a person did in one title.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct PersonRole {
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episodes: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PersonCredit {
    pub item: ItemSummary,
    pub roles: Vec<PersonRole>,
}

/// GET /api/people/{id}.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PersonDetail {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
    pub credits: Vec<PersonCredit>,
}

/// GET /api/auth/me: the profile the app acts as.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Me {
    pub id: i64,
    pub username: String,
    pub name: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
    pub managed: bool,
    pub home: bool,
    pub has_pin: bool,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
    pub all_libraries: bool,
    pub library_ids: Vec<i64>,
    pub max_age: Option<i64>,
    pub disabled: bool,
    pub created_at: i64,
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: String,
    pub has_password: bool,
    pub two_step: bool,
    pub scope: String,
}

/// A profile of the household, for apps.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct AppProfile {
    pub id: i64,
    pub name: String,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
    pub current: bool,
    pub needs: &'static str,
}
