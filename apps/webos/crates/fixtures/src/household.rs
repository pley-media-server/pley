//! The household (three profiles) and what they did before the app came:
//! Alex has things in progress, watched, rated, in the history, in three
//! playlists and in My List (the Android app's fixture household).

use std::collections::{BTreeMap, HashMap};

use crate::catalog::{BASE_TIME, Catalog, DAY};
use crate::ids;
use crate::model::UserState;

/// The smart playlist's filter.
pub(crate) const SMART_PLAYLIST_FILTER: &str = "genre=Comedy&unwatched=1&sort=year:desc";

/// A profile's languages and when subtitles show.
#[derive(Clone, Debug)]
pub(crate) struct TrackDefaults {
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    /// `off`, `forced`, `foreign` or `always`.
    pub subtitle_mode: String,
}

/// The tracks picked for a movie or a whole show.
#[derive(Clone, Debug, Default)]
pub(crate) struct StreamChoice {
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
}

/// An item of My List: when it was added (unix seconds), and whether it was
/// watched then (a movie leaves the list once watched after that).
#[derive(Clone, Debug)]
pub(crate) struct Listed {
    pub item_id: i64,
    pub added_at: i64,
    pub watched_then: bool,
}

/// One play in watch history.
#[derive(Clone, Debug)]
pub(crate) struct HistoryRecord {
    pub id: i64,
    pub item_id: i64,
    pub viewed_at: i64,
    pub device: Option<String>,
}

/// A profile of the household, and everything of its own that apps change.
pub(crate) struct Profile {
    pub id: i64,
    pub username: &'static str,
    pub name: &'static str,
    pub avatar_color: &'static str,
    pub has_avatar: bool,
    pub pin: Option<&'static str>,
    pub has_password: bool,
    pub managed: bool,
    /// Live TV is open to it.
    pub live: bool,
    pub track_defaults: TrackDefaults,
    /// Playable items' states and any item's own rating.
    pub states: HashMap<i64, UserState>,
    /// Track choices, by movie or show.
    pub stream_choices: HashMap<i64, StreamChoice>,
    /// My List, in the order added.
    pub watchlist: Vec<Listed>,
    /// Movies and shows taken out of Continue Watching, and when.
    pub hidden_from_continue: HashMap<i64, i64>,
    /// Newest first.
    pub history: Vec<HistoryRecord>,
    /// Wrong PINs typed for this profile, by time (unix seconds).
    pub wrong_pins: Vec<i64>,
    /// Moves on with every change, so listings' ETags change too.
    pub version: u64,
    /// Live TV channels marked, and those watched last (newest first).
    pub live_favorites: Vec<i64>,
    pub live_recent: Vec<i64>,
    /// The profile's own order of the libraries, when it set one.
    pub library_order: Option<Vec<i64>>,
    /// The account's language (`PUT /api/me/language`); none: Automatic.
    pub language: Option<String>,
}

impl Profile {
    fn new(id: i64, username: &'static str, name: &'static str, color: &'static str) -> Profile {
        Profile {
            id,
            username,
            name,
            avatar_color: color,
            has_avatar: false,
            pin: None,
            has_password: false,
            managed: false,
            live: true,
            track_defaults: TrackDefaults {
                audio_language: Some("eng".into()),
                subtitle_language: None,
                subtitle_mode: "forced".into(),
            },
            states: HashMap::new(),
            stream_choices: HashMap::new(),
            watchlist: Vec::new(),
            hidden_from_continue: HashMap::new(),
            history: Vec::new(),
            wrong_pins: Vec::new(),
            version: 0,
            live_favorites: Vec::new(),
            live_recent: Vec::new(),
            library_order: None,
            language: None,
        }
    }

    pub fn avatar_url(&self) -> Option<String> {
        self.has_avatar.then(|| format!("/api/users/{}/avatar?v={BASE_TIME}", self.id))
    }

    /// What switching to it takes from the app: `none`, `pin` or `password`.
    pub fn needs(&self) -> &'static str {
        if self.pin.is_some() {
            "pin"
        } else if self.has_password && self.id != ids::ALEX {
            "password"
        } else {
            "none"
        }
    }

    pub fn state(&self, item: i64) -> UserState {
        self.states.get(&item).cloned().unwrap_or_default()
    }

    pub fn update(&mut self, item: i64, change: impl FnOnce(&mut UserState)) {
        change(self.states.entry(item).or_default());
        self.version += 1;
    }
}

/// A playlist; a smart one has a filter and a library and no entries of its own.
pub(crate) struct Playlist {
    pub id: i64,
    pub owner: i64,
    pub title: String,
    /// `video` or `audio`.
    pub kind: &'static str,
    pub smart_filter: Option<String>,
    pub library_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// (entry id, item id), in order.
    pub entries: Vec<(i64, i64)>,
}

/// The household and its playlists.
pub(crate) struct Household {
    pub profiles: BTreeMap<i64, Profile>,
    pub playlists: BTreeMap<i64, Playlist>,
    next_playlist: i64,
    next_entry: i64,
    next_history: i64,
}

impl Household {
    pub fn new(catalog: &Catalog) -> Household {
        let mut alex = Profile::new(ids::ALEX, "alex", "Alex", "#38bdf8");
        alex.has_avatar = true;
        alex.has_password = true;
        let mut sam = Profile::new(ids::SAM, "sam", "Sam", "#a3e635");
        sam.pin = Some(ids::PIN);
        sam.managed = true;
        sam.live = false;
        let mut robin = Profile::new(ids::ROBIN, "robin", "Robin", "#f472b6");
        robin.has_avatar = true;
        robin.has_password = true;
        let mut household = Household {
            profiles: [alex, sam, robin].into_iter().map(|p| (p.id, p)).collect(),
            playlists: BTreeMap::new(),
            next_playlist: 1,
            next_entry: 1,
            next_history: 1,
        };
        household.seed(catalog);
        household
    }

    pub fn profile(&self, id: i64) -> Option<&Profile> {
        self.profiles.get(&id)
    }

    pub fn profile_mut(&mut self, id: i64) -> Option<&mut Profile> {
        self.profiles.get_mut(&id)
    }

    pub fn new_playlist(
        &mut self,
        owner: i64,
        title: &str,
        kind: &'static str,
        filter: Option<String>,
        library: Option<i64>,
        now: i64,
    ) -> i64 {
        let id = self.next_playlist;
        self.next_playlist += 1;
        self.playlists.insert(
            id,
            Playlist {
                id,
                owner,
                title: title.to_string(),
                kind,
                smart_filter: filter,
                library_id: library,
                created_at: now,
                updated_at: now,
                entries: Vec::new(),
            },
        );
        id
    }

    pub fn new_entry(&mut self) -> i64 {
        let id = self.next_entry;
        self.next_entry += 1;
        id
    }

    /// Records a play, newest first.
    pub fn add_history(&mut self, profile: i64, item: i64, at: i64, device: Option<String>) {
        let id = self.next_history;
        self.next_history += 1;
        if let Some(p) = self.profiles.get_mut(&profile) {
            p.history.insert(0, HistoryRecord { id, item_id: item, viewed_at: at, device });
        }
    }

    fn seed(&mut self, catalog: &Catalog) {
        let alex = self.profiles.get_mut(&ids::ALEX).expect("Alex is in the household");
        // In progress: a few movies and the fixture show's second episode.
        for (n, id) in (7..=14).enumerate() {
            let Some(duration) = catalog.item(id).and_then(|i| i.base.duration_ms) else { continue };
            alex.states.insert(
                id,
                UserState {
                    position_ms: duration / (n as i64 + 2),
                    last_played_at: Some(BASE_TIME - n as i64 * 3_600),
                    ..UserState::default()
                },
            );
        }
        alex.states.insert(
            ids::SHOW_S1E1,
            UserState {
                watched: true,
                play_count: 1,
                last_played_at: Some(BASE_TIME - 2 * DAY),
                ..UserState::default()
            },
        );
        alex.states.insert(
            ids::SHOW_S1E2,
            UserState { position_ms: 9_000, last_played_at: Some(BASE_TIME - 600), ..UserState::default() },
        );
        // Watched, some rated.
        let watched: Vec<i64> = (20..=80).step_by(3).collect();
        for &id in &watched {
            alex.states.insert(
                id,
                UserState {
                    watched: true,
                    play_count: 1,
                    last_played_at: Some(BASE_TIME - id * DAY),
                    rating: (id % 2 == 0).then_some(8.0),
                    ..UserState::default()
                },
            );
        }
        let mut history = Vec::new();
        for (n, id) in watched.iter().copied().chain([ids::SHOW_S1E1]).enumerate() {
            let at = alex.state(id).last_played_at.unwrap_or(BASE_TIME - n as i64 * DAY);
            let device = if n % 2 == 0 { "LG TV" } else { "Chrome on Windows" };
            history.push(HistoryRecord {
                id: self.next_history,
                item_id: id,
                viewed_at: at,
                device: Some(device.into()),
            });
            self.next_history += 1;
        }
        history.sort_by_key(|r| std::cmp::Reverse(r.viewed_at));
        alex.history = history;

        // My List: two shows and three movies, oldest first.
        let second_show = catalog.top_ids(ids::SHOWS).get(1).copied();
        let listed = [Some(15), second_show, Some(16), Some(ids::SHOW), Some(17)];
        for (n, id) in listed.into_iter().flatten().enumerate() {
            alex.watchlist.push(Listed {
                item_id: id,
                added_at: BASE_TIME - (10 - n as i64) * 3_600,
                watched_then: false,
            });
        }

        let video = self.new_playlist(ids::ALEX, "Favourites", "video", None, None, BASE_TIME - 10 * DAY);
        for id in [1, 2, 15, 16, 17, 30, 31, 32, 101, 202, 303, ids::SHOW_S1E1] {
            let entry = self.new_entry();
            self.playlists.get_mut(&video).expect("just made").entries.push((entry, id));
        }
        let audio = self.new_playlist(ids::ALEX, "Road Trip", "audio", None, None, BASE_TIME - 5 * DAY);
        for id in catalog.leaves(ids::ALBUM).into_iter().chain(catalog.leaves(ids::ALBUM + 5)) {
            let entry = self.new_entry();
            self.playlists.get_mut(&audio).expect("just made").entries.push((entry, id));
        }
        self.new_playlist(
            ids::ALEX,
            "Unwatched Comedies",
            "video",
            Some(SMART_PLAYLIST_FILTER.into()),
            Some(ids::MOVIES),
            BASE_TIME - DAY,
        );
        // Alex's favourite channels and the ones watched last.
        let alex = self.profiles.get_mut(&ids::ALEX).expect("Alex is in the household");
        alex.live_favorites = vec![ids::LIVE_CHANNEL, 3, 9, 17];
        alex.live_recent = vec![ids::LIVE_CHANNEL, 9, 31];
    }
}
