//! In-process event bus feeding the `/api/events` Server-Sent Events stream.

use serde::Serialize;
use tokio::sync::broadcast;

use crate::discover::TitleKind;
use crate::playback::Control;
use crate::requests::{RequestStatus, RequestUser};

/// Whom a request's event is for: the accounts it names (its maker, its
/// followers) and whether managers get it too
/// ([`crate::auth::AuthUser::manages_requests`]). Apps' streams get request
/// events only with `requests`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Audience {
    pub users: Vec<i64>,
    pub managers: bool,
    /// Left out even as a manager: the request's maker, of its own
    /// `request_created`.
    pub except_user: Option<i64>,
    /// Left out: the streams of this token (its hash), the session that
    /// caused the event.
    pub except_token: Option<String>,
}

impl Audience {
    /// The accounts named, and managers.
    pub fn of(users: Vec<i64>) -> Audience {
        Audience { users, managers: true, ..Audience::default() }
    }

    /// Only these accounts.
    pub fn only(users: Vec<i64>) -> Audience {
        Audience { users, ..Audience::default() }
    }

    /// Whether a stream of account `user_id`, opened with the token hashed
    /// `token`, gets the event: `manager` while it manages requests.
    pub fn reaches(&self, user_id: i64, manager: bool, token: &str) -> bool {
        if self.except_token.as_deref() == Some(token) || self.except_user == Some(user_id) {
            return false;
        }
        self.users.contains(&user_id) || (self.managers && manager)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Queued,
    Walking,
    Probing,
    Metadata,
    Done,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    ScanStarted {
        library_id: i64,
    },
    ScanProgress {
        library_id: i64,
        phase: ScanPhase,
        processed: u64,
        total: u64,
    },
    ScanFinished {
        library_id: i64,
        added: u64,
        updated: u64,
        removed: u64,
    },
    LibraryChanged {
        library_id: i64,
    },
    LibrariesChanged,
    /// The libraries one user sees changed: they chose their own order or
    /// went back to the shared one, or an administrator changed what they
    /// may see (their libraries, their age limit, administrator or not).
    /// To their streams, as `libraries_changed`: their devices fetch the
    /// libraries, Home and what shows again.
    #[serde(rename = "libraries_changed")]
    OwnLibrariesChanged {
        #[serde(skip)]
        user_id: i64,
    },
    /// One of the user's playback or Live TV sessions started, stopped,
    /// went quiet or changed (paused, played, moved): to their streams, whose
    /// remotes list their sessions again, and to administrators', whose
    /// dashboard lists everyone's ([`ServerEvent::to_administrators`]).
    SessionsChanged {
        #[serde(skip)]
        user_id: i64,
    },
    /// The user's My List changed (added, removed, or a movie in it was
    /// watched): their other devices fetch Home again.
    WatchlistChanged {
        #[serde(skip)]
        user_id: i64,
    },
    /// The user's watch state of an item changed (watched or not, a resume
    /// point started, moved or cleared, in or out of Continue Watching):
    /// their devices fetch Home and that item's pages again. `item_ids` is
    /// the item, then its parent and grandparent (a season and its show),
    /// whose counts follow it; `children`: everything under the first one
    /// changed too (a show or season marked as a whole).
    UserStateChanged {
        #[serde(skip)]
        user_id: i64,
        item_ids: Vec<i64>,
        children: bool,
    },
    /// A playback session was stopped by someone else than its player (who
    /// oversees playback: an administrator, or one of Pley's own apps
    /// allowed `activity` on an administrator's account; or its owner on
    /// another device): the player stops. `by_admin` is true when one who
    /// oversees playback stopped another user's session.
    SessionStopped {
        #[serde(skip)]
        user_id: i64,
        session_id: String,
        message: Option<String>,
        by_admin: bool,
    },
    /// The user's remote (another of their devices) sent the player of
    /// `session_id` a command it said it obeys: to that user's streams,
    /// whose player of that session acts on it and reports at once.
    /// `position_ms` is where `seek` goes, `offset_ms` how far `skip` goes
    /// (back when negative), `stream_index` the track `audio` and
    /// `subtitles` change to (`-1`: subtitles off), `quality` the id of the
    /// Quality menu's entry `quality` picks; each is `null` for the other
    /// commands.
    PlayerCommand {
        #[serde(skip)]
        user_id: i64,
        session_id: String,
        command: Control,
        position_ms: Option<i64>,
        offset_ms: Option<i64>,
        stream_index: Option<i64>,
        quality: Option<&'static str>,
    },
    /// The user's remote cast a title, a playlist or a Live TV channel to
    /// one of their devices: to that user's streams, whose app holding
    /// `receiver_id` plays it as its own Play would. One of `item_id`,
    /// `playlist_id` and `channel_id` is set; `audio_index` and
    /// `subtitle_index` (`-1`: none), when set, are the tracks the remote
    /// had picked for the title, which it plays with; `from` is the
    /// remote's device. Cast from another profile of the household, it goes
    /// to the streams of the profile the receiver is on, with the remote's
    /// profile (`profile_id`) and a ticket (`switch_ticket`) the receiver
    /// switches to it with first; both are left out otherwise.
    Cast {
        #[serde(skip)]
        user_id: i64,
        receiver_id: String,
        item_id: Option<i64>,
        playlist_id: Option<i64>,
        channel_id: Option<i64>,
        start_ms: Option<i64>,
        shuffle: bool,
        audio_index: Option<i64>,
        subtitle_index: Option<i64>,
        from: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        profile_id: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        switch_ticket: Option<String>,
    },
    /// Live TV's sources, channels or guide changed: apps fetch the
    /// channels and the guide again.
    LiveChannelsChanged,
    /// A live channel started or stopped being pulled, or someone started
    /// or stopped watching one: the connections in use changed.
    LiveTunersChanged,
    /// Someone took the connection a Live TV session's channel used, to
    /// watch another channel: to its viewer's streams, whose player of
    /// `session_id` stops at once and shows `message` (why, and who only
    /// when both are in the home).
    LiveTakenOver {
        #[serde(skip)]
        user_id: i64,
        session_id: String,
        channel_id: i64,
        message: String,
    },
    /// A request changed (made, followed, decided, sent, failed, ready,
    /// withdrawn, deleted, or its stage), to its maker, its followers and
    /// managers; or many did (`request_id`, `kind` and `tmdb_id` all
    /// `None`): an account saw its news (to its other streams), or one was
    /// deleted with its requests (to managers and those who inherited
    /// some). Clients fetch the summary and what shows that request again.
    RequestsChanged {
        #[serde(skip)]
        to: Audience,
        request_id: Option<i64>,
        kind: Option<TitleKind>,
        tmdb_id: Option<i64>,
    },
    /// A request became available, or some more of a show's seasons did
    /// (`seasons`: those newly ready; `complete`: all it asks for): to its
    /// maker and to each follower who can open the title, one event each,
    /// with the library's title where that account may open it.
    RequestAvailable {
        #[serde(skip)]
        to: Audience,
        request_id: i64,
        title: String,
        kind: TitleKind,
        tmdb_id: i64,
        item_id: Option<i64>,
        poster_url: Option<String>,
        seasons: Option<Vec<i64>>,
        complete: bool,
    },
    /// A request was approved (to its maker, unless they approved it
    /// themselves) or declined (to its maker with the reason, and to its
    /// followers without).
    RequestDecided {
        #[serde(skip)]
        to: Audience,
        request_id: i64,
        title: String,
        kind: TitleKind,
        tmdb_id: i64,
        status: RequestStatus,
        reason: Option<String>,
    },
    /// Someone asked for a title: to managers but its maker.
    RequestCreated {
        #[serde(skip)]
        to: Audience,
        request_id: i64,
        title: String,
        kind: TitleKind,
        tmdb_id: i64,
        seasons: Option<Vec<i64>>,
        requested_by: RequestUser,
        status: RequestStatus,
    },
    /// A request couldn't be sent to Radarr or Sonarr and is left for a
    /// manager: to managers. `retry_at` is always `None`: nothing is tried
    /// again by itself.
    RequestFailed {
        #[serde(skip)]
        to: Audience,
        request_id: i64,
        title: String,
        kind: TitleKind,
        reason: String,
        retry_at: Option<i64>,
    },
    /// What the request summary says may have changed: requests were turned
    /// on or off, their limits or TMDB's key changed (`user_id: None`: to
    /// every stream, apps' without `requests` included), or one account's
    /// access to them (its request access, administrator role, age limit or
    /// libraries). Clients fetch the summary again.
    RequestsSettingsChanged {
        #[serde(skip)]
        user_id: Option<i64>,
    },
}

impl ServerEvent {
    /// The library the event is about, if it is about one.
    pub fn library_id(&self) -> Option<i64> {
        match self {
            ServerEvent::ScanStarted { library_id }
            | ServerEvent::ScanProgress { library_id, .. }
            | ServerEvent::ScanFinished { library_id, .. }
            | ServerEvent::LibraryChanged { library_id } => Some(*library_id),
            ServerEvent::LibrariesChanged
            | ServerEvent::OwnLibrariesChanged { .. }
            | ServerEvent::SessionsChanged { .. }
            | ServerEvent::WatchlistChanged { .. }
            | ServerEvent::UserStateChanged { .. }
            | ServerEvent::SessionStopped { .. }
            | ServerEvent::PlayerCommand { .. }
            | ServerEvent::Cast { .. }
            | ServerEvent::LiveChannelsChanged
            | ServerEvent::LiveTunersChanged
            | ServerEvent::LiveTakenOver { .. }
            | ServerEvent::RequestsChanged { .. }
            | ServerEvent::RequestAvailable { .. }
            | ServerEvent::RequestDecided { .. }
            | ServerEvent::RequestCreated { .. }
            | ServerEvent::RequestFailed { .. }
            | ServerEvent::RequestsSettingsChanged { .. } => None,
        }
    }

    /// The only user the event is for, if it is for one (request events
    /// say whom with [`ServerEvent::audience`]).
    pub fn user_id(&self) -> Option<i64> {
        match self {
            ServerEvent::WatchlistChanged { user_id }
            | ServerEvent::OwnLibrariesChanged { user_id }
            | ServerEvent::SessionsChanged { user_id }
            | ServerEvent::UserStateChanged { user_id, .. }
            | ServerEvent::SessionStopped { user_id, .. }
            | ServerEvent::PlayerCommand { user_id, .. }
            | ServerEvent::Cast { user_id, .. }
            | ServerEvent::LiveTakenOver { user_id, .. } => Some(*user_id),
            ServerEvent::RequestsSettingsChanged { user_id } => *user_id,
            _ => None,
        }
    }

    /// Whether administrators' streams get the event too, whoever's it is:
    /// their dashboard lists everyone's sessions.
    pub fn to_administrators(&self) -> bool {
        matches!(self, ServerEvent::SessionsChanged { .. })
    }

    /// Whom a request's event is for.
    pub fn audience(&self) -> Option<&Audience> {
        match self {
            ServerEvent::RequestsChanged { to, .. }
            | ServerEvent::RequestAvailable { to, .. }
            | ServerEvent::RequestDecided { to, .. }
            | ServerEvent::RequestCreated { to, .. }
            | ServerEvent::RequestFailed { to, .. } => Some(to),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<ServerEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(512);
        Self { tx }
    }

    pub fn send(&self, event: ServerEvent) {
        let _ = self.tx.send(event);
    }

    /// `sessions_changed` to each of these users once, however many of
    /// their sessions changed together (a sweep, a take-over).
    pub fn sessions_changed(&self, users: impl IntoIterator<Item = i64>) {
        for user_id in users.into_iter().collect::<std::collections::BTreeSet<_>>() {
            self.send(ServerEvent::SessionsChanged { user_id });
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_type_tag() {
        let json = serde_json::to_string(&ServerEvent::LibrariesChanged).unwrap();
        assert_eq!(json, r#"{"type":"libraries_changed"}"#);
        let json = serde_json::to_string(&ServerEvent::ScanProgress {
            library_id: 1,
            phase: ScanPhase::Probing,
            processed: 2,
            total: 3,
        })
        .unwrap();
        assert_eq!(json, r#"{"type":"scan_progress","library_id":1,"phase":"probing","processed":2,"total":3}"#);
        // Whom an event is for stays on the server.
        let json = serde_json::to_string(&ServerEvent::WatchlistChanged { user_id: 4 }).unwrap();
        assert_eq!(json, r#"{"type":"watchlist_changed"}"#);
        let event = ServerEvent::UserStateChanged { user_id: 4, item_ids: vec![12, 11, 10], children: false };
        assert_eq!(event.user_id(), Some(4));
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"type":"user_state_changed","item_ids":[12,11,10],"children":false}"#);
        let event = ServerEvent::LiveTakenOver {
            user_id: 4,
            session_id: "abc".into(),
            channel_id: 7,
            message: "Someone else needed the connection.".into(),
        };
        assert_eq!(event.user_id(), Some(4));
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"type":"live_taken_over","session_id":"abc","channel_id":7,"message":"Someone else needed the connection."}"#
        );
        let json = serde_json::to_string(&ServerEvent::SessionStopped {
            user_id: 4,
            session_id: "s1".into(),
            message: None,
            by_admin: true,
        })
        .unwrap();
        assert_eq!(json, r#"{"type":"session_stopped","session_id":"s1","message":null,"by_admin":true}"#);
        // Sessions' changes go to their user, and administrators.
        let event = ServerEvent::SessionsChanged { user_id: 4 };
        assert_eq!((event.user_id(), event.to_administrators()), (Some(4), true));
        assert_eq!(serde_json::to_string(&event).unwrap(), r#"{"type":"sessions_changed"}"#);
        assert!(!ServerEvent::WatchlistChanged { user_id: 4 }.to_administrators());
        let event = ServerEvent::PlayerCommand {
            user_id: 4,
            session_id: "s1".into(),
            command: Control::Seek,
            position_ms: Some(90_000),
            offset_ms: None,
            stream_index: None,
            quality: None,
        };
        assert_eq!(event.user_id(), Some(4), "to the session's user only");
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"player_command","session_id":"s1","command":"seek","position_ms":90000,"offset_ms":null,"stream_index":null,"quality":null}"#
        );
        let event = ServerEvent::Cast {
            user_id: 4,
            receiver_id: "tv-1".into(),
            item_id: Some(12),
            playlist_id: None,
            channel_id: None,
            start_ms: None,
            shuffle: false,
            audio_index: Some(2),
            subtitle_index: Some(-1),
            from: Some("Chrome on Windows".into()),
            profile_id: None,
            switch_ticket: None,
        };
        assert_eq!((event.user_id(), event.to_administrators()), (Some(4), false), "to the receiver's user only");
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"cast","receiver_id":"tv-1","item_id":12,"playlist_id":null,"channel_id":null,"start_ms":null,"shuffle":false,"audio_index":2,"subtitle_index":-1,"from":"Chrome on Windows"}"#
        );
        let event = ServerEvent::Cast {
            user_id: 5,
            receiver_id: "tv-1".into(),
            item_id: Some(12),
            playlist_id: None,
            channel_id: None,
            start_ms: None,
            shuffle: false,
            audio_index: None,
            subtitle_index: None,
            from: None,
            profile_id: Some(4),
            switch_ticket: Some("t1".into()),
        };
        assert_eq!(event.user_id(), Some(5), "to the profile the receiver is on");
        assert!(
            serde_json::to_string(&event).unwrap().ends_with(r#""from":null,"profile_id":4,"switch_ticket":"t1"}"#)
        );
        // Request events say whom they are for, and stay off the
        // per-user path.
        let event = ServerEvent::RequestsChanged {
            to: Audience::of(vec![4, 5]),
            request_id: Some(2),
            kind: Some(TitleKind::Movie),
            tmdb_id: Some(949),
        };
        assert_eq!((event.user_id(), event.audience().map(|a| a.managers)), (None, Some(true)));
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"type":"requests_changed","request_id":2,"kind":"movie","tmdb_id":949}"#);
        let many =
            ServerEvent::RequestsChanged { to: Audience::only(vec![4]), request_id: None, kind: None, tmdb_id: None };
        let json = serde_json::to_string(&many).unwrap();
        assert_eq!(json, r#"{"type":"requests_changed","request_id":null,"kind":null,"tmdb_id":null}"#);
        let event = ServerEvent::RequestAvailable {
            to: Audience::only(vec![4]),
            request_id: 2,
            title: "Heat".into(),
            kind: TitleKind::Show,
            tmdb_id: 7,
            item_id: Some(9),
            poster_url: None,
            seasons: Some(vec![1, 2]),
            complete: false,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"request_available","request_id":2,"title":"Heat","kind":"show","tmdb_id":7,"item_id":9,"poster_url":null,"seasons":[1,2],"complete":false}"#
        );
        let event = ServerEvent::RequestDecided {
            to: Audience::only(vec![4]),
            request_id: 2,
            title: "Heat".into(),
            kind: TitleKind::Movie,
            tmdb_id: 949,
            status: RequestStatus::Declined,
            reason: None,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"request_decided","request_id":2,"title":"Heat","kind":"movie","tmdb_id":949,"status":"declined","reason":null}"#
        );
        let everyone = ServerEvent::RequestsSettingsChanged { user_id: None };
        assert_eq!((everyone.user_id(), everyone.audience()), (None, None));
        assert_eq!(serde_json::to_string(&everyone).unwrap(), r#"{"type":"requests_settings_changed"}"#);
        let one = ServerEvent::RequestsSettingsChanged { user_id: Some(4) };
        assert_eq!((one.user_id(), one.audience()), (Some(4), None));
    }

    #[test]
    fn sessions_changed_goes_once_to_each_user() {
        let bus = EventBus::new();
        let mut events = bus.subscribe();
        bus.sessions_changed([7, 4, 7]);
        let mut users = Vec::new();
        while let Ok(event) = events.try_recv() {
            users.push(event.user_id());
        }
        assert_eq!(users, vec![Some(4), Some(7)]);
    }

    #[test]
    fn an_audience_names_accounts_and_managers() {
        let to = Audience::of(vec![4]);
        assert!(to.reaches(4, false, "a"), "named");
        assert!(to.reaches(7, true, "b"), "a manager");
        assert!(!to.reaches(7, false, "b"), "neither");
        let only = Audience::only(vec![4]);
        assert!(!only.reaches(7, true, "b"), "not for managers");
        let created = Audience { except_user: Some(4), ..Audience::of(Vec::new()) };
        assert!(!created.reaches(4, true, "a") && created.reaches(7, true, "b"), "its maker is left out");
        let seen = Audience { except_token: Some("a".into()), ..Audience::only(vec![4]) };
        assert!(!seen.reaches(4, false, "a") && seen.reaches(4, false, "c"), "the account's other streams");
    }
}
