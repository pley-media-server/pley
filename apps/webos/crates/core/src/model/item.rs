//! Items, their files and streams, markers, chapters and people.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use super::{nd, wire_enum};

wire_enum! {
    /// What an item is (`kind` in apps/server/src/catalog.rs).
    pub enum ItemKind or Unknown {
        Movie = "movie",
        Show = "show",
        Season = "season",
        Episode = "episode",
        Artist = "artist",
        Album = "album",
        Track = "track",
        /// Bonus material of a movie or a show; `extra_type` says which.
        Extra = "extra",
        #[default]
        Unknown = "unknown",
    }
}

impl ItemKind {
    /// Kinds that play themselves; the others hold items.
    pub fn playable(self) -> bool {
        matches!(self, ItemKind::Movie | ItemKind::Episode | ItemKind::Track | ItemKind::Extra)
    }

    /// Kinds My List takes: movies and shows.
    pub fn listable(self) -> bool {
        matches!(self, ItemKind::Movie | ItemKind::Show)
    }

    /// A video the player plays: a movie, an episode or an extra.
    pub fn playable_video(self) -> bool {
        matches!(self, ItemKind::Movie | ItemKind::Episode | ItemKind::Extra)
    }
}

wire_enum! {
    /// Which bonus material an extra is (apps/server/src/library/walk.rs, `ExtraKind`).
    pub enum ExtraType or Unknown {
        Trailer = "trailer",
        Featurette = "featurette",
        BehindTheScenes = "behind_the_scenes",
        DeletedScene = "deleted_scene",
        Interview = "interview",
        Scene = "scene",
        Short = "short",
        Other = "other",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// A video's dynamic range (packages/transcoder-api, `DynamicRange`).
    pub enum DynamicRange or Unknown {
        Sdr = "sdr",
        Hdr10 = "hdr10",
        Hdr10Plus = "hdr10plus",
        Hlg = "hlg",
        DolbyVision = "dolby_vision",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// The pictures an item has (`GET /api/items/{id}/images/{kind}`);
    /// collections have the first two.
    pub enum ImageKind or Poster {
        #[default]
        Poster = "poster",
        Backdrop = "backdrop",
        /// Episode stills and video frame grabs, 16:9.
        Thumb = "thumb",
    }
}

/// The profile's own state of an item.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserState {
    /// Where to resume, in ms; 0 when there's nothing to resume.
    #[serde(default, deserialize_with = "nd")]
    pub position_ms: i64,
    /// Shows and seasons: every episode is watched.
    #[serde(default, deserialize_with = "nd")]
    pub watched: bool,
    #[serde(default, deserialize_with = "nd")]
    pub play_count: i64,
    /// Unix seconds.
    pub last_played_at: Option<i64>,
    /// The profile's own rating, 0 to 10 (two points per star).
    pub rating: Option<f64>,
}

/// What a title's files are like, in library listings (a show: its
/// episodes together).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaSummary {
    /// Bits per second (a show: its episodes' average).
    pub bitrate: Option<i64>,
    /// Bytes (a show: all its episodes).
    pub size: Option<i64>,
    pub width: Option<i64>,
    /// A show: its largest episode's.
    pub height: Option<i64>,
    /// None for shows.
    pub video_codec: Option<String>,
    /// A show: its best episode's (none from older servers).
    pub dynamic_range: Option<DynamicRange>,
}

/// An item in a listing (apps/server/src/catalog.rs, `ItemSummary`).
/// Optional fields are left out of big listings when they have no value.
/// Picture addresses are the server's own (`/api/items/12/images/poster?v=…`):
/// `Api::picture` sizes them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemSummary {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub library_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub kind: ItemKind,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    #[serde(default, deserialize_with = "nd")]
    pub sort_title: String,
    pub year: Option<i64>,
    /// Season number, episode number or track number.
    pub index: Option<i64>,
    /// The season of an episode, the show of a season, the album of a track,
    /// the artist of an album, the movie or show of an extra.
    pub parent_id: Option<i64>,
    pub parent_title: Option<String>,
    /// An episode's season number; a track's disc number.
    pub parent_index: Option<i64>,
    /// The show of an episode, the artist of a track.
    pub grandparent_id: Option<i64>,
    pub grandparent_title: Option<String>,
    pub duration_ms: Option<i64>,
    /// The critics' rating, 0 to 10.
    pub rating: Option<f64>,
    #[serde(default, deserialize_with = "nd")]
    pub added_at: i64,
    pub poster_url: Option<String>,
    /// Left out of library listings.
    pub backdrop_url: Option<String>,
    /// Episode stills and frame grabs (16:9); left out of library listings.
    pub thumb_url: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub user_state: UserState,
    /// Shows and seasons: episodes; artists and albums: tracks.
    pub leaf_count: Option<i64>,
    /// Shows and seasons: watched episodes.
    pub viewed_leaf_count: Option<i64>,
    /// Shows: seasons; artists: albums.
    pub child_count: Option<i64>,
    /// In listings only for episodes (unless `summaries=1`); always in [`ItemDetail`].
    pub summary: Option<String>,
    /// `YYYY-MM-DD`.
    pub release_date: Option<String>,
    /// Extras only.
    pub extra_type: Option<ExtraType>,
    /// Only in library listings sorted by bitrate.
    pub media: Option<MediaSummary>,
}

/// `GET /api/items/{id}`: an item with everything the server knows about it
/// (the server's `ItemDetail`: an [`ItemSummary`] with more fields beside it
/// in the same object). It dereferences to its summary.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemDetail {
    #[serde(flatten)]
    pub summary: ItemSummary,
    pub original_title: Option<String>,
    pub tagline: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub genres: Vec<String>,
    pub studio: Option<String>,
    /// `PG-13`, `15`, `FSK 12`…
    pub content_rating: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    /// Empty for shows, seasons, artists and albums; the tallest first.
    #[serde(default, deserialize_with = "nd")]
    pub files: Vec<MediaFile>,
    /// Intros and credits of the files, by file then start.
    #[serde(default, deserialize_with = "nd")]
    pub markers: Vec<Marker>,
    /// Fields an administrator edited by hand.
    #[serde(default, deserialize_with = "nd")]
    pub locked_fields: Vec<String>,
    /// Movies and shows: trailers first, then by type and title.
    #[serde(default, deserialize_with = "nd")]
    pub extras: Vec<ItemSummary>,
    /// Actors in billing order; episodes: guest stars; seasons: the show's.
    #[serde(default, deserialize_with = "nd")]
    pub cast: Vec<Credit>,
    /// Creators, directors, writers and producers.
    #[serde(default, deserialize_with = "nd")]
    pub crew: Vec<Credit>,
    #[serde(default, deserialize_with = "nd")]
    pub in_watchlist: bool,
}

impl Deref for ItemDetail {
    type Target = ItemSummary;

    fn deref(&self) -> &ItemSummary {
        &self.summary
    }
}

impl ItemDetail {
    /// The file `id`, or none.
    pub fn file(&self, id: i64) -> Option<&MediaFile> {
        self.files.iter().find(|f| f.id == id)
    }

    /// The markers of file `id`.
    pub fn markers_of(&self, file_id: i64) -> impl Iterator<Item = &Marker> {
        self.markers.iter().filter(move |m| m.file_id == file_id)
    }
}

/// `{ items }`: an item's children (seasons, episodes, albums, tracks),
/// related titles, and My List (`GET /api/watchlist`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemList {
    #[serde(default, deserialize_with = "nd")]
    pub items: Vec<ItemSummary>,
}

/// `{ item }`: `GET /api/items/{id}/on-deck` and `…/next`; none when there's none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MaybeItem {
    pub item: Option<ItemSummary>,
}

/// A media file of an item (apps/server/src/catalog.rs, `MediaFileDto`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaFile {
    pub id: i64,
    /// An app gets the file name only.
    #[serde(default, deserialize_with = "nd")]
    pub path: String,
    /// Bytes.
    #[serde(default, deserialize_with = "nd")]
    pub size: i64,
    /// The transcoder's normalised name: `mkv`, `mp4`, `ts`…
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    /// Bits per second.
    pub bitrate: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// ffprobe's names: `h264`, `hevc`…
    pub video_codec: Option<String>,
    /// The default audio stream's codec.
    pub audio_codec: Option<String>,
    /// Embedded streams, then subtitle files beside the media (index 1000–8999),
    /// then subtitles added on the server (9000–9999).
    #[serde(default, deserialize_with = "nd")]
    pub streams: Vec<MediaStream>,
    /// From `{edition-…}` in the file's name.
    pub edition: Option<String>,
    /// The audio track to start with for this profile; none: the file's default.
    pub selected_audio: Option<i64>,
    /// The profile picked `selected_audio`. Absent from older servers, whose
    /// choice reads as picked (`format::audio_picked`).
    pub audio_picked: Option<bool>,
    /// The subtitle track to start with; none: off.
    pub selected_subtitle: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub chapters: Vec<Chapter>,
}

impl MediaFile {
    /// The streams of `kind`, in the file's order.
    pub fn streams_of(&self, kind: StreamKind) -> impl Iterator<Item = &MediaStream> {
        self.streams.iter().filter(move |s| s.kind == kind)
    }

    /// The stream with `index`.
    pub fn stream(&self, index: i64) -> Option<&MediaStream> {
        self.streams.iter().find(|s| s.index == index)
    }
}

wire_enum! {
    /// A stream's kind (packages/transcoder-api, `StreamKind`).
    pub enum StreamKind or Unknown {
        Video = "video",
        Audio = "audio",
        Subtitle = "subtitle",
        #[default]
        Unknown = "unknown",
    }
}

/// One stream of a file as ffprobe found it (packages/transcoder-api,
/// `MediaStream`). Fields that don't apply to its kind are left out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaStream {
    /// ffprobe's index below 1000; subtitle files beside the media from 1000; added ones from 9000.
    pub index: i64,
    #[serde(default, deserialize_with = "nd")]
    pub kind: StreamKind,
    #[serde(default, deserialize_with = "nd")]
    pub codec: String,
    /// ISO 639-2 as tagged (`eng`).
    pub language: Option<String>,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub default: bool,
    #[serde(default, deserialize_with = "nd")]
    pub forced: bool,
    pub bitrate: Option<i64>,
    /// `Main 10`, `High`, `DTS-HD MA`, `Dolby TrueHD + Dolby Atmos`…
    pub profile: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub frame_rate: Option<f64>,
    pub bit_depth: Option<i64>,
    /// `yuv420p10le`…
    pub pix_fmt: Option<String>,
    /// ffprobe's numbering: HEVC 5.1 is 153, H.264 5.1 is 51.
    pub level: Option<i64>,
    pub interlaced: Option<bool>,
    /// An older server's field order (`progressive`, `tt`…), when it says.
    pub field_order: Option<String>,
    pub has_b_frames: Option<bool>,
    /// Degrees counter-clockwise.
    pub rotation: Option<i64>,
    /// The file is always converted (timestamps in decode order).
    #[serde(default, deserialize_with = "nd")]
    pub decode_order_times: bool,
    pub hdr: Option<bool>,
    pub dynamic_range: Option<DynamicRange>,
    /// Dolby Vision profile: 5, 7, 8…
    pub dv_profile: Option<i64>,
    pub dv_level: Option<i64>,
    /// Base-layer compatibility: 0 none, 1 HDR10, 2 SDR, 4 HLG, 6 Blu-ray HDR10.
    pub dv_bl_compat: Option<i64>,
    /// Profile 7's enhancement layer: `fel` or `mel`; none when not known.
    pub dv_el: Option<String>,
    /// HDR10+ metadata in the first frames; none when not looked for.
    pub hdr10plus: Option<bool>,
    /// `smpte2084`, `arib-std-b67`, `bt709`…
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub color_space: Option<String>,
    pub channels: Option<i64>,
    /// `5.1(side)`, `stereo`…
    pub channel_layout: Option<String>,
    /// Hz.
    pub sample_rate: Option<i64>,
    /// Subtitles: text (srt, ass, WebVTT, mov_text) or pictures (PGS, VobSub, DVB).
    pub text_based: Option<bool>,
    /// A subtitle file beside the media, or one added on the server.
    #[serde(default, deserialize_with = "nd")]
    pub external: bool,
    /// Added on the server (found online, or uploaded).
    #[serde(default, deserialize_with = "nd")]
    pub added: bool,
    /// This profile may delete it.
    #[serde(default, deserialize_with = "nd")]
    pub deletable: bool,
}

/// A chapter of a file, from its container.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    #[serde(default, deserialize_with = "nd")]
    pub start_ms: i64,
    #[serde(default, deserialize_with = "nd")]
    pub end_ms: i64,
    pub title: Option<String>,
    /// `/api/files/{id}/chapters/{n}.jpg?v=…`, once the file has seek previews.
    pub thumb_url: Option<String>,
}

wire_enum! {
    /// What a marker marks.
    pub enum MarkerKind or Unknown {
        Intro = "intro",
        Credits = "credits",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// How a marker was found.
    pub enum MarkerSource or Unknown {
        /// A chapter titled like an intro or credits.
        Chapter = "chapter",
        /// Audio the season's episodes share.
        Fingerprint = "fingerprint",
        /// Text on black near the end.
        Frames = "frames",
        #[default]
        Unknown = "unknown",
    }
}

/// An intro or the credits of a file (apps/server/src/markers/mod.rs).
/// Credits that run to the end of the file end at its duration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    #[serde(default, deserialize_with = "nd")]
    pub file_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub kind: MarkerKind,
    #[serde(default, deserialize_with = "nd")]
    pub start_ms: i64,
    #[serde(default, deserialize_with = "nd")]
    pub end_ms: i64,
    #[serde(default, deserialize_with = "nd")]
    pub source: MarkerSource,
}

wire_enum! {
    /// A part someone had in a title (apps/server/src/people.rs, `Role`).
    pub enum CreditRole or Unknown {
        Actor = "actor",
        Creator = "creator",
        Director = "director",
        Writer = "writer",
        Producer = "producer",
        #[default]
        Unknown = "unknown",
    }
}

/// Someone credited in an item ([`ItemDetail::cast`] and `crew`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Credit {
    /// The person: `GET /api/people/{id}`.
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default, deserialize_with = "nd")]
    pub role: CreditRole,
    /// Actors: the part played.
    pub character: Option<String>,
    pub photo_url: Option<String>,
}

/// A person in search results.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersonSummary {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    pub photo_url: Option<String>,
}

/// `GET /api/people/{id}`: a person and what they are credited in, newest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersonDetail {
    #[serde(flatten)]
    pub person: PersonSummary,
    #[serde(default, deserialize_with = "nd")]
    pub credits: Vec<PersonCredit>,
}

impl Deref for PersonDetail {
    type Target = PersonSummary;

    fn deref(&self) -> &PersonSummary {
        &self.person
    }
}

/// A movie or show a person is credited in; episode credits count towards their show.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersonCredit {
    pub item: ItemSummary,
    #[serde(default, deserialize_with = "nd")]
    pub roles: Vec<PersonRole>,
}

/// One of a person's parts in a title.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersonRole {
    #[serde(default, deserialize_with = "nd")]
    pub role: CreditRole,
    pub character: Option<String>,
    /// Credits from single episodes: how many.
    pub episodes: Option<i64>,
}
