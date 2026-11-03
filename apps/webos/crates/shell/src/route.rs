//! Every page of the app, with what it is opened on: the Android app's `PleyKey`s and the TV
//! app's `PageKey`s (`packages/tv/src/ui/shell/navigator.ts`), plus the pages the TV app shows
//! inside others (the address pad, the PIN pad, a settings category, the licences).

use std::path::PathBuf;

use pley_core::model::{DiscoverList, LibrarySort, LiveCategory, TitleKind};

use crate::screen::Chrome;

/// A library page's tabs, as on the web and Android.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibraryTab {
    Recommended,
    Library,
    Collections,
}

impl LibraryTab {
    /// How the tab is named in stored views and on the simulator's command line.
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryTab::Recommended => "recommended",
            LibraryTab::Library => "library",
            LibraryTab::Collections => "collections",
        }
    }

    /// The tab named `s`.
    pub fn parse(s: &str) -> Option<LibraryTab> {
        Some(match s {
            "recommended" => LibraryTab::Recommended,
            "library" => LibraryTab::Library,
            "collections" => LibraryTab::Collections,
            _ => return None,
        })
    }
}

/// The categories of Settings, top to bottom (Android's `TvSettingsScreen.kt:83-92`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    Profile,
    Playback,
    Audio,
    SubtitlesAndDisplay,
    Server,
    Diagnostics,
    About,
    SignOut,
}

impl SettingsSection {
    /// Every category in the order Settings lists them.
    pub const ALL: [SettingsSection; 8] = [
        SettingsSection::Profile,
        SettingsSection::Playback,
        SettingsSection::Audio,
        SettingsSection::SubtitlesAndDisplay,
        SettingsSection::Server,
        SettingsSection::Diagnostics,
        SettingsSection::About,
        SettingsSection::SignOut,
    ];

    /// The category's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::Profile => "Profile",
            SettingsSection::Playback => "Playback",
            SettingsSection::Audio => "Audio",
            SettingsSection::SubtitlesAndDisplay => "Subtitles and display",
            SettingsSection::Server => "Server",
            SettingsSection::Diagnostics => "Diagnostics",
            SettingsSection::About => "About",
            SettingsSection::SignOut => "Sign out",
        }
    }

    /// Its name on the simulator's command line (`settings:about`).
    pub fn as_str(self) -> &'static str {
        match self {
            SettingsSection::Profile => "profile",
            SettingsSection::Playback => "playback",
            SettingsSection::Audio => "audio",
            SettingsSection::SubtitlesAndDisplay => "subtitles",
            SettingsSection::Server => "server",
            SettingsSection::Diagnostics => "diagnostics",
            SettingsSection::About => "about",
            SettingsSection::SignOut => "sign-out",
        }
    }

    /// The category named `s`.
    pub fn parse(s: &str) -> Option<SettingsSection> {
        SettingsSection::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Where a playback's Next and Previous go: a playlist's or a collection's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayQueue {
    Playlist(i64),
    Collection(i64),
}

/// A video to play, as the player opens with it (the TV app's `PlayRequest`,
/// `packages/tv/src/player/plan.ts`). What isn't given the player chooses: the file that plays
/// best here, the resume point, the profile's tracks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayRequest {
    pub item_id: i64,
    pub file_id: Option<i64>,
    /// Where to start, in ms; `None`: the resume point (or the start when watched).
    pub start_ms: Option<u64>,
    /// Start over, whatever the resume point says.
    pub from_start: bool,
    pub audio_index: Option<i64>,
    /// A subtitle stream to start with; `Some(-1)` starts with them off.
    pub subtitle_index: Option<i64>,
    pub queue: Option<PlayQueue>,
    /// A video on the USB drive, played from the file with no server behind it (`item_id` is
    /// then pley-local's `ITEM_ID`).
    pub file: Option<PathBuf>,
    /// When OK was pressed ([`pley_base::time::now`]), for the time to the first picture.
    pub pressed_at: f64,
}

impl PlayRequest {
    /// Plays `item_id` as the player chooses.
    pub fn item(item_id: i64) -> PlayRequest {
        PlayRequest { item_id, ..PlayRequest::default() }
    }
}

/// A smart playlist being saved from a library's listing: the library and its query string.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SmartPlaylist {
    pub library_id: i64,
    pub filter: String,
}

/// A page of the app and what it is opened on. The app maps each to the screen that draws it
/// ([`crate::Registry`]); screens open each other only through these.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    /// "Choose your server": the saved servers, the ones found on the network, an address to type.
    Servers,
    /// The address pad: typing a server's address.
    AddressPad,
    /// Signing in to `server` with a code shown on screen.
    Link {
        server: String,
    },
    /// "Who's watching?".
    Profiles,
    /// The PIN pad of a profile picked on "Who's watching?".
    Pin {
        profile: i64,
    },
    Home,
    /// Every library; `sort`: arranging their order (from Settings), and back when that ends.
    Libraries {
        sort: bool,
    },
    /// A library, on `tab` (`None`: the tab it was left on); `sort`: a row's See all, listed so
    /// until the view is changed.
    Library {
        id: i64,
        tab: Option<LibraryTab>,
        sort: Option<LibrarySort>,
    },
    /// The collections of the movie and show libraries (the rail's entry names the first one).
    Collections {
        library_id: Option<i64>,
    },
    Collection {
        id: i64,
    },
    Playlists,
    Playlist {
        id: i64,
    },
    /// Adding titles to a playlist; `title` names them in the heading.
    AddToPlaylist {
        item_ids: Vec<i64>,
        title: String,
    },
    /// A new playlist with `item_ids` in it; with `smart`, Save these filters.
    NewPlaylist {
        item_ids: Vec<i64>,
        smart: Option<SmartPlaylist>,
    },
    /// A movie, show, season, episode or extra.
    Item {
        id: i64,
    },
    /// A title's actions (long OK or the menu key on a tile); `continue_watching`: opened from
    /// that row, so it offers to take the title out of it.
    ItemActions {
        id: i64,
        continue_watching: bool,
    },
    Person {
        id: i64,
    },
    Search {
        query: String,
    },
    Artist {
        id: i64,
    },
    /// An album; `track_id` is the track it was opened from, highlighted in its list.
    Album {
        id: i64,
        track_id: Option<i64>,
    },
    /// Music playing now, with its queue.
    NowPlaying,
    /// Live TV: the guide, or the channels as cards.
    LiveTv,
    /// Discover: TMDB's lists and titles, for a profile that may ask for them.
    Discover,
    /// One of Discover's rows, whole.
    DiscoverList {
        list: DiscoverList,
    },
    /// Everything TMDB has for words, from the See all of search's Discover row.
    DiscoverSearch {
        query: String,
    },
    /// A movie or show from TMDB; `pick`: its season picker opens (Request more seasons).
    Title {
        kind: TitleKind,
        tmdb_id: i64,
        pick: bool,
    },
    /// Your requests (for managers Requests, opening on Needs attention).
    Requests,
    /// A live channel playing; channel up and down stay in `category`.
    LivePlayer {
        channel_id: i64,
        category: LiveCategory,
        pressed_at: f64,
    },
    /// The video player.
    Player(PlayRequest),
    /// A folder of the USB drive plugged into the TV; `None`: the drive's top.
    Storage {
        path: Option<PathBuf>,
    },
    /// Settings, on `section` (`None`: the first, or the one it was left on).
    Settings {
        section: Option<SettingsSection>,
    },
    /// What this profile watched, newest first.
    History,
    /// What this TV decodes and shows, and why.
    Diagnostics,
    /// The open-source licences of what Pley for LG TV is built from, with their texts.
    Licences,
}

impl Route {
    /// A short name for logs, the simulator and the placeholder screen.
    pub fn name(&self) -> &'static str {
        match self {
            Route::Servers => "servers",
            Route::AddressPad => "address",
            Route::Link { .. } => "link",
            Route::Profiles => "profiles",
            Route::Pin { .. } => "pin",
            Route::Home => "home",
            Route::Libraries { .. } => "libraries",
            Route::Library { .. } => "library",
            Route::Collections { .. } => "collections",
            Route::Collection { .. } => "collection",
            Route::Playlists => "playlists",
            Route::Playlist { .. } => "playlist",
            Route::AddToPlaylist { .. } => "add-to-playlist",
            Route::NewPlaylist { .. } => "new-playlist",
            Route::Item { .. } => "item",
            Route::ItemActions { .. } => "actions",
            Route::Person { .. } => "person",
            Route::Search { .. } => "search",
            Route::Artist { .. } => "artist",
            Route::Album { .. } => "album",
            Route::NowPlaying => "now-playing",
            Route::LiveTv => "live",
            Route::Discover => "discover",
            Route::DiscoverList { .. } => "discover-list",
            Route::DiscoverSearch { .. } => "discover-search",
            Route::Title { .. } => "title",
            Route::Requests => "requests",
            Route::LivePlayer { .. } => "live-player",
            Route::Player(_) => "player",
            Route::Storage { .. } => "storage",
            Route::Settings { .. } => "settings",
            Route::History => "history",
            Route::Diagnostics => "diagnostics",
            Route::Licences => "licences",
        }
    }

    /// What the page is called on screen, for the placeholder of a page not built yet.
    pub fn title(&self) -> String {
        match self {
            Route::Servers => "Choose your server".into(),
            Route::AddressPad => "Enter your server's address".into(),
            Route::Link { .. } => "Sign in to Pley".into(),
            Route::Profiles => "Who's watching?".into(),
            Route::Pin { .. } => "Enter the PIN".into(),
            Route::Home => "Home".into(),
            Route::Libraries { .. } => "Libraries".into(),
            Route::Library { id, .. } => format!("Library {id}"),
            Route::Collections { .. } => "Collections".into(),
            Route::Collection { id } => format!("Collection {id}"),
            Route::Playlists => "Playlists".into(),
            Route::Playlist { id } => format!("Playlist {id}"),
            Route::AddToPlaylist { title, .. } => format!("Add “{title}” to a playlist"),
            Route::NewPlaylist { .. } => "New playlist".into(),
            Route::Item { id } => format!("Item {id}"),
            Route::ItemActions { id, .. } => format!("Actions of item {id}"),
            Route::Person { id } => format!("Person {id}"),
            Route::Search { .. } => "Search".into(),
            Route::Artist { id } => format!("Artist {id}"),
            Route::Album { id, .. } => format!("Album {id}"),
            Route::NowPlaying => "Now Playing".into(),
            Route::LiveTv => "Live TV".into(),
            Route::Discover => "Discover".into(),
            Route::DiscoverList { list } => format!("Discover: {}", list.as_str()),
            Route::DiscoverSearch { query } => format!("On TMDB: {query}"),
            Route::Title { kind, tmdb_id, .. } => format!("{} {tmdb_id} on TMDB", kind.as_str()),
            Route::Requests => "Requests".into(),
            Route::LivePlayer { channel_id, .. } => format!("Channel {channel_id}"),
            Route::Player(r) => format!("Playing item {}", r.item_id),
            Route::Storage { .. } => "USB drive".into(),
            Route::Settings { section: Some(s) } => format!("Settings: {}", s.label()),
            Route::Settings { section: None } => "Settings".into(),
            Route::History => "Watch history".into(),
            Route::Diagnostics => "Diagnostics".into(),
            Route::Licences => "Open-source licences".into(),
        }
    }

    /// How the page shows by default: getting in is full screen (Android hides the rail when the
    /// stack starts at Servers, Link or Profiles, `TvApp.kt:157-158`), the players play video, the
    /// rest sit beside the rail.
    pub fn chrome(&self) -> Chrome {
        match self {
            Route::Servers | Route::AddressPad | Route::Link { .. } | Route::Profiles | Route::Pin { .. } => {
                Chrome::Bare
            }
            Route::Player(_) | Route::LivePlayer { .. } => Chrome::Video,
            _ => Chrome::Rail,
        }
    }

    /// Whether the rail opens this page itself (it then starts a back stack of its own).
    pub fn is_rail_page(&self) -> bool {
        matches!(
            self,
            Route::Home
                | Route::Search { .. }
                | Route::LiveTv
                | Route::Discover
                | Route::Requests
                | Route::Library { .. }
                | Route::Libraries { .. }
                | Route::Playlists
                | Route::Collections { .. }
                | Route::NowPlaying
                | Route::Storage { .. }
                | Route::Settings { .. }
                | Route::Profiles
        )
    }

    /// The route the simulator's `--route` names: `home`, `item:12`, `library:3:collections`,
    /// `search:alien`, `settings:about`, `player:12`, `live-player:4`, `storage:<folder>`…
    pub fn parse(s: &str) -> Option<Route> {
        // A folder's path may hold colons of its own (`C:\Films`).
        if let Some(rest) = s.trim().strip_prefix("storage") {
            return match rest.strip_prefix(':') {
                Some(path) if !path.is_empty() => Some(Route::Storage { path: Some(PathBuf::from(path)) }),
                _ if rest.is_empty() || rest == ":" => Some(Route::Storage { path: None }),
                _ => None,
            };
        }
        let mut parts = s.trim().splitn(3, ':');
        let name = parts.next()?;
        let a = parts.next();
        let b = parts.next();
        let id = || a.and_then(|v| v.parse::<i64>().ok());
        Some(match name {
            "servers" => Route::Servers,
            "address" => Route::AddressPad,
            "link" => Route::Link { server: [Some(a?), b].into_iter().flatten().collect::<Vec<_>>().join(":") },
            "profiles" => Route::Profiles,
            "pin" => Route::Pin { profile: id()? },
            "home" => Route::Home,
            "libraries" => Route::Libraries { sort: a == Some("sort") },
            "library" => Route::Library { id: id()?, tab: b.and_then(LibraryTab::parse), sort: None },
            "collections" => Route::Collections { library_id: id() },
            "collection" => Route::Collection { id: id()? },
            "playlists" => Route::Playlists,
            "playlist" => Route::Playlist { id: id()? },
            "add-to-playlist" => Route::AddToPlaylist { item_ids: vec![id()?], title: b.unwrap_or("").to_string() },
            "new-playlist" => Route::NewPlaylist { item_ids: id().into_iter().collect(), smart: None },
            "item" => Route::Item { id: id()? },
            "actions" => Route::ItemActions { id: id()?, continue_watching: b == Some("continue") },
            "person" => Route::Person { id: id()? },
            "search" => Route::Search { query: [a, b].into_iter().flatten().collect::<Vec<_>>().join(":") },
            "artist" => Route::Artist { id: id()? },
            "album" => Route::Album { id: id()?, track_id: b.and_then(|v| v.parse().ok()) },
            "now-playing" => Route::NowPlaying,
            "live" => Route::LiveTv,
            "discover" => Route::Discover,
            "discover-list" => Route::DiscoverList { list: DiscoverList::from_wire(a?) },
            "discover-search" => {
                Route::DiscoverSearch { query: [a, b].into_iter().flatten().collect::<Vec<_>>().join(":") }
            }
            // `title:movie:603`, or `title:603` for a movie.
            "title" => match b {
                Some(id) => Route::Title { kind: TitleKind::from_wire(a?), tmdb_id: id.parse().ok()?, pick: false },
                None => Route::Title { kind: TitleKind::Movie, tmdb_id: id()?, pick: false },
            },
            "requests" => Route::Requests,
            "live-player" => Route::LivePlayer {
                channel_id: id()?,
                category: b.map(LiveCategory::parse).unwrap_or_default(),
                pressed_at: 0.0,
            },
            "player" => Route::Player(PlayRequest::item(id()?)),
            "settings" => Route::Settings { section: a.and_then(SettingsSection::parse) },
            "history" => Route::History,
            "diagnostics" => Route::Diagnostics,
            "licences" | "licenses" => Route::Licences,
            _ => return None,
        })
    }

    /// One route of each kind: tests check that every page has a screen.
    pub fn samples() -> Vec<Route> {
        vec![
            Route::Servers,
            Route::AddressPad,
            Route::Link { server: "http://192.168.1.20:7539".into() },
            Route::Profiles,
            Route::Pin { profile: 2 },
            Route::Home,
            Route::Libraries { sort: false },
            Route::Library { id: 1, tab: None, sort: None },
            Route::Collections { library_id: Some(1) },
            Route::Collection { id: 5 },
            Route::Playlists,
            Route::Playlist { id: 3 },
            Route::AddToPlaylist { item_ids: vec![12], title: "Arrival".into() },
            Route::NewPlaylist { item_ids: Vec::new(), smart: None },
            Route::Item { id: 12 },
            Route::ItemActions { id: 12, continue_watching: true },
            Route::Person { id: 7 },
            Route::Search { query: String::new() },
            Route::Artist { id: 40 },
            Route::Album { id: 41, track_id: None },
            Route::NowPlaying,
            Route::LiveTv,
            Route::Discover,
            Route::DiscoverList { list: DiscoverList::Trending },
            Route::DiscoverSearch { query: "dune".into() },
            Route::Title { kind: TitleKind::Movie, tmdb_id: 438631, pick: false },
            Route::Requests,
            Route::LivePlayer { channel_id: 4, category: LiveCategory::All, pressed_at: 0.0 },
            Route::Player(PlayRequest::item(12)),
            Route::Storage { path: None },
            Route::Settings { section: None },
            Route::History,
            Route::Diagnostics,
            Route::Licences,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_page_has_a_sample_and_its_own_name() {
        let names: Vec<&str> = Route::samples().iter().map(Route::name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        assert_eq!(names.len(), 34);
    }

    #[test]
    fn the_command_line_names_routes() {
        assert_eq!(Route::parse("home"), Some(Route::Home));
        assert_eq!(Route::parse("item:12"), Some(Route::Item { id: 12 }));
        assert_eq!(
            Route::parse("library:3:collections"),
            Some(Route::Library { id: 3, tab: Some(LibraryTab::Collections), sort: None })
        );
        assert_eq!(Route::parse("settings:about"), Some(Route::Settings { section: Some(SettingsSection::About) }));
        assert_eq!(Route::parse("search:alien"), Some(Route::Search { query: "alien".into() }));
        assert_eq!(
            Route::parse("link:http://10.0.0.2:7539"),
            Some(Route::Link { server: "http://10.0.0.2:7539".into() })
        );
        assert_eq!(Route::parse("player:12"), Some(Route::Player(PlayRequest::item(12))));
        assert_eq!(
            Route::parse("title:show:95396"),
            Some(Route::Title { kind: TitleKind::Show, tmdb_id: 95396, pick: false })
        );
        assert_eq!(Route::parse("storage"), Some(Route::Storage { path: None }));
        assert_eq!(
            Route::parse("storage:/tmp/usb/sda/sda1/Films"),
            Some(Route::Storage { path: Some(PathBuf::from("/tmp/usb/sda/sda1/Films")) })
        );
        assert_eq!(Route::parse("storages"), None);
        assert_eq!(Route::parse("item"), None);
        assert_eq!(Route::parse("nowhere"), None);
        // Every page can be opened from the command line, with an id where it needs one.
        for r in Route::samples() {
            let name = r.name();
            let parsed = Route::parse(name).or_else(|| Route::parse(&format!("{name}:1")));
            assert_eq!(parsed.as_ref().map(Route::name), Some(name), "{name}");
        }
    }

    #[test]
    fn getting_in_is_bare_and_the_players_show_video() {
        assert_eq!(Route::Servers.chrome(), Chrome::Bare);
        assert_eq!(Route::Profiles.chrome(), Chrome::Bare);
        assert_eq!(Route::Player(PlayRequest::item(1)).chrome(), Chrome::Video);
        assert_eq!(Route::Home.chrome(), Chrome::Rail);
        assert_eq!(Route::ItemActions { id: 1, continue_watching: false }.chrome(), Chrome::Rail);
    }
}
