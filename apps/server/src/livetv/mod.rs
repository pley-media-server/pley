//! Live TV (IPTV): channels from M3U playlists and Xtream Codes accounts,
//! their guide from XMLTV, watched through this server.
//!
//! The server holds the connections to the provider ([`tuner`]): a channel
//! is pulled once however many watch it, and a source plays no more channels
//! at once than its provider allows (`max_connections`). Players get the
//! channel from the transcoder as live HLS (H.264 and AAC), or as the
//! provider's MPEG-TS untouched (`stream.ts`, for players that read it).
//! Logos and programme pictures come through the server too: the web app
//! loads nothing from elsewhere.

pub mod m3u;
pub mod tuner;
pub mod xmltv;
pub mod xtream;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::auth::AuthUser;
use crate::db::Db;
use crate::error::AppError;
use crate::events::{EventBus, ServerEvent};
use crate::transcoder::Transcoder;
use crate::util::now;

use tuner::Played;
pub use tuner::Tuners;

/// The settings table's key for [`LiveSettings`].
const SETTINGS_KEY: &str = "live_tv";
/// A playlist larger than this isn't read (they run to tens of MB with films).
const MAX_PLAYLIST_BYTES: usize = 200 << 20;
/// A guide larger than this isn't read (gzipped ones are counted as they come).
const MAX_GUIDE_BYTES: u64 = 3 << 30;
const MAX_PICTURE_BYTES: usize = 4 << 20;
/// A logo larger than this isn't taken (most are a few KB; a few are photos).
const MAX_LOGO_BYTES: usize = 1 << 20;
/// Logos are kept no larger than this either way: the apps show them small.
const LOGO_SIDE: u32 = 400;
/// A picture is downloaded again once it's this old, when next asked for (a
/// provider changes its logos); one nobody asked for in [`PICTURE_DROP`] is
/// deleted, and the oldest go first once all of them take [`PICTURES_MAX_BYTES`].
const PICTURE_FRESH: Duration = Duration::from_secs(7 * 86_400);
const PICTURE_DROP: Duration = Duration::from_secs(30 * 86_400);
const PICTURES_MAX_BYTES: u64 = 256 << 20;
/// How often the pictures kept are looked over.
const PICTURES_PRUNED_EVERY: Duration = Duration::from_secs(3600);
/// A channel its source stopped listing is forgotten after this long.
const MISSING_KEPT: i64 = 14 * 86_400;
/// Programmes kept behind now (the guide shows the past few hours).
const GUIDE_BEHIND: i64 = 6 * 3600;
/// After a failed refresh, the next try waits this long.
const RETRY_AFTER: Duration = Duration::from_secs(30 * 60);
/// A picture that failed to download isn't asked for again for this long.
const PICTURE_RETRY: Duration = Duration::from_secs(3600);
/// Rows inserted per write transaction (the database has one writer).
const BATCH: usize = 2000;
/// The most channels a page of them has.
const MAX_PAGE: usize = 500;
/// How many channels "Recently watched" keeps.
const RECENT: i64 = 24;
/// An offline channel is tried again this long after it last was.
const RECHECK_AFTER: i64 = 6 * 3600;
/// Offline channels tried per source each minute, while it has nothing to pull.
const RECHECKS: i64 = 10;
/// Bytes a channel tried again must send to count as playing.
const PLAYS_BYTES: usize = 188 * 64;
/// Logos fetched from providers each minute ahead of anyone looking, and
/// how many at once: a lineup's logos are on this server within the hour,
/// and a page of channels shows its logos at once.
const LOGOS_AHEAD: usize = 600;
const LOGOS_AT_ONCE: usize = 8;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Everyone with an account (see [`LiveSettings::age_limited`]).
    #[default]
    Everyone,
    /// The household: accounts in Home (not friends given access).
    Home,
    /// Administrators only.
    Admins,
}

/// Live TV's settings (administrators', in Settings → Live TV).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LiveSettings {
    pub access: Access,
    /// Profiles with an age limit see Live TV too (channels have no ratings
    /// to hold them to it).
    pub age_limited: bool,
    /// Interlaced channels are made progressive (transcoded) rather than
    /// sent as they are, which players show combed.
    pub deinterlace: bool,
    /// The most lines a transcoded channel has; `None`: as many as it has.
    pub max_height: Option<u32>,
    /// How far back a channel can be paused and rewound, in minutes.
    pub timeshift_minutes: u32,
    /// Channels and guides are read again this often, in hours.
    pub refresh_hours: u32,
    /// Days of guide kept ahead.
    pub guide_days: u32,
    /// Channels that didn't play, and a provider's divider rows ("##### UK
    /// #####"), are left out of the lists.
    pub hide_offline: bool,
    /// People who watch Live TV whatever `access` says, and people who
    /// don't (administrators always do).
    pub allowed_users: Vec<i64>,
    pub denied_users: Vec<i64>,
}

impl Default for LiveSettings {
    fn default() -> Self {
        Self {
            access: Access::Everyone,
            age_limited: false,
            deinterlace: true,
            max_height: None,
            timeshift_minutes: 5,
            refresh_hours: 24,
            guide_days: 7,
            hide_offline: true,
            allowed_users: Vec::new(),
            denied_users: Vec::new(),
        }
    }
}

impl LiveSettings {
    fn check(&self) -> Result<(), AppError> {
        if !(1..=60).contains(&self.timeshift_minutes) {
            return Err(AppError::bad_request("timeshift_minutes must be between 1 and 60"));
        }
        if !(1..=168).contains(&self.refresh_hours) {
            return Err(AppError::bad_request("refresh_hours must be between 1 and 168"));
        }
        if !(1..=14).contains(&self.guide_days) {
            return Err(AppError::bad_request("guide_days must be between 1 and 14"));
        }
        if self.max_height.is_some_and(|h| !(240..=2160).contains(&h)) {
            return Err(AppError::bad_request("max_height must be between 240 and 2160"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    M3u,
    Xtream,
}

impl SourceKind {
    fn as_str(self) -> &'static str {
        match self {
            SourceKind::M3u => "m3u",
            SourceKind::Xtream => "xtream",
        }
    }
}

/// A source as administrators see it (never its password).
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub id: i64,
    /// Its short id, which channel addresses name it by.
    pub uid: String,
    pub name: String,
    pub kind: SourceKind,
    pub url: String,
    pub username: String,
    pub password_set: bool,
    pub guide_url: String,
    pub user_agent: String,
    pub max_connections: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub channels_at: Option<i64>,
    pub channels_error: Option<String>,
    pub guide_at: Option<i64>,
    pub guide_error: Option<String>,
    pub account: Option<xtream::Account>,
    /// Channels it lists now, and of those hidden.
    pub channel_count: i64,
    pub hidden_count: i64,
    /// Channels with a guide.
    pub guided_count: i64,
    /// Connections in use now.
    pub connections_in_use: usize,
    /// Being read now.
    pub refreshing: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceInput {
    pub name: String,
    pub kind: SourceKind,
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// `None` keeps the one saved.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub guide_url: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default = "one")]
    pub max_connections: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn one() -> i64 {
    1
}

fn yes() -> bool {
    true
}

/// What a test of a source's details found.
#[derive(Debug, Clone, Serialize)]
pub struct SourceTest {
    pub channels: usize,
    pub groups: usize,
    pub guide_url: Option<String>,
    pub account: Option<xtream::Account>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SourceRow {
    id: i64,
    uid: String,
    name: String,
    kind: String,
    url: String,
    username: String,
    password: String,
    guide_url: String,
    user_agent: String,
    max_connections: i64,
    enabled: bool,
    created_at: i64,
    channels_at: Option<i64>,
    channels_error: Option<String>,
    guide_at: Option<i64>,
    guide_error: Option<String>,
    account: Option<String>,
}

impl SourceRow {
    fn kind(&self) -> SourceKind {
        if self.kind == "xtream" { SourceKind::Xtream } else { SourceKind::M3u }
    }

    /// The guide read for it: its own, or an Xtream account's.
    fn effective_guide_url(&self) -> Option<String> {
        if !self.guide_url.trim().is_empty() {
            return Some(self.guide_url.trim().to_owned());
        }
        (self.kind() == SourceKind::Xtream)
            .then(|| xtream::guide_url(&xtream::base(&self.url), &self.username, &self.password))
    }
}

/// A programme, as apps get it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Program {
    pub channel_id: i64,
    pub start: i64,
    pub stop: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub categories: Vec<String>,
    /// `/api/live/channels/{id}/programs/{start}/image`, when it has one.
    pub image: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub is_new: bool,
    pub is_live: bool,
    pub rating: Option<String>,
}

/// A channel, as apps get it.
#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    pub id: i64,
    pub source_id: i64,
    /// Its source's short id: `/live/{source_uid}/{id}`.
    pub source_uid: String,
    pub name: String,
    pub number: Option<String>,
    pub group: String,
    /// `/api/live/channels/{id}/logo`, when it has one.
    pub logo: Option<String>,
    pub favorite: bool,
    /// Whether the guide has programmes for it.
    pub has_guide: bool,
    pub now: Option<Program>,
    pub next: Option<Program>,
}

/// A channel as administrators see it: as the source has it and as set.
#[derive(Debug, Clone, Serialize)]
pub struct AdminChannel {
    pub id: i64,
    pub source_id: i64,
    pub name: String,
    pub number: Option<String>,
    pub group: String,
    pub logo: Option<String>,
    pub guide_id: String,
    pub hidden: bool,
    pub source_name: String,
    pub source_number: Option<String>,
    pub source_guide_id: String,
    /// The guide's channel of its name, found when it names none.
    pub matched_guide_id: Option<String>,
    pub custom_name: Option<String>,
    pub custom_number: Option<String>,
    pub custom_guide_id: Option<String>,
    pub has_guide: bool,
    /// It didn't play when last tried (since then), or it is a divider row.
    pub offline: bool,
    pub offline_since: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ChannelPatch {
    #[serde(default)]
    pub hidden: Option<bool>,
    /// `""` clears it (back to the source's).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub guide_id: Option<String>,
}

/// The guide for a span of time.
#[derive(Debug, Clone, Serialize)]
pub struct Guide {
    pub from: i64,
    pub to: i64,
    /// Programmes by channel id.
    pub programs: HashMap<i64, Vec<Program>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChannelRow {
    pub id: i64,
    pub source_id: i64,
    name: String,
    number: Option<String>,
    group_name: String,
    logo_url: String,
    guide_id: String,
    matched_guide_id: Option<String>,
    pub stream_url: String,
    user_agent: Option<String>,
    position: i64,
    hidden: bool,
    custom_name: Option<String>,
    custom_number: Option<String>,
    custom_guide_id: Option<String>,
    offline_since: Option<i64>,
}

impl ChannelRow {
    pub fn display_name(&self) -> &str {
        self.custom_name.as_deref().filter(|n| !n.is_empty()).unwrap_or(&self.name)
    }

    /// It didn't play when last tried, or it is a provider's divider row.
    fn offline(&self) -> bool {
        self.offline_since.is_some() || is_divider(self.display_name())
    }

    fn display_number(&self) -> Option<String> {
        self.custom_number.clone().filter(|n| !n.is_empty()).or_else(|| self.number.clone())
    }

    /// The guide's channel it takes its programmes from: the one set for it,
    /// else the match found when the guide lacks the source's id, else the
    /// source's id.
    fn guide_key(&self) -> Option<&str> {
        [self.custom_guide_id.as_deref(), self.matched_guide_id.as_deref(), Some(self.guide_id.as_str())]
            .into_iter()
            .flatten()
            .find(|id| !id.is_empty())
    }
}

const CHANNEL_COLUMNS: &str = "c.id, c.source_id, c.name, c.number, c.group_name, c.logo_url, c.guide_id, \
     c.matched_guide_id, c.stream_url, c.user_agent, c.position, c.hidden, c.custom_name, c.custom_number, \
     c.custom_guide_id, c.offline_since";

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProgramRow {
    source_id: i64,
    guide_id: String,
    start: i64,
    stop: i64,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    categories: Option<String>,
    image: Option<String>,
    season: Option<i64>,
    episode: Option<i64>,
    is_new: bool,
    is_live: bool,
    rating: Option<String>,
}

impl ProgramRow {
    fn for_channel(&self, channel_id: i64) -> Program {
        Program {
            channel_id,
            start: self.start,
            stop: self.stop,
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            description: self.description.clone(),
            categories: self.categories.as_deref().and_then(|c| serde_json::from_str(c).ok()).unwrap_or_default(),
            image: self
                .image
                .as_ref()
                .map(|_| format!("/api/live/channels/{channel_id}/programs/{}/image", self.start)),
            season: self.season,
            episode: self.episode,
            is_new: self.is_new,
            is_live: self.is_live,
            rating: self.rating.clone(),
        }
    }
}

/// Who asks for a source to be read, which says what happens while it is
/// being read already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asker {
    /// Reads that fall due: looked at again in a minute.
    Schedule,
    /// An administrator (a source added or changed, Refresh): read once more
    /// after the read under way, channels and guide.
    Admin,
    /// Channels given a guide channel or a name: the guide read once more,
    /// unless the read under way hadn't chosen its channels yet. Not counted
    /// as a try, which would put off reads that fall due.
    ChannelsSet,
}

pub struct LiveTv {
    db: Db,
    events: EventBus,
    settings: RwLock<Arc<LiveSettings>>,
    http: reqwest::Client,
    pub tuners: Arc<Tuners>,
    /// Logos and programme pictures, kept a while ([`PICTURE_FRESH`]).
    pictures: PathBuf,
    /// Pictures whose download failed lately (by URL).
    failed_pictures: Mutex<HashMap<String, Instant>>,
    /// When the pictures kept were last looked over.
    pictures_pruned: Mutex<Option<Instant>>,
    /// Guides are downloaded here before they are read.
    downloads: PathBuf,
    /// Sources being read now, and when each was last tried.
    refreshing: Mutex<HashSet<i64>>,
    /// Sources read once more when their read ends, their channels too when
    /// `true`: asked for meanwhile ([`Asker`]). Taken with `refreshing`
    /// locked first.
    again: Mutex<HashMap<i64, bool>>,
    tried: Mutex<HashMap<i64, Instant>>,
    refresh_lock: tokio::sync::Mutex<()>,
    /// Every channel, in order, read once and kept until something changes:
    /// a lineup runs to tens of thousands, and every page of the apps asks.
    lineup: Mutex<Option<Arc<Lineup>>>,
    /// One reading of the lineup at a time: those asking meanwhile wait for it.
    reading_lineup: tokio::sync::Mutex<()>,
    /// Counts changes, so a lineup read while one happened isn't kept.
    generation: AtomicU64,
    /// Logo addresses fetched ahead (or that failed): not asked for again.
    logos_ahead: Mutex<HashSet<String>>,
    /// What's on, as search finds it, read again a minute later or after a
    /// change (see [`LiveTv::airing_titled`]).
    airing: Mutex<Option<Arc<Airing>>>,
}

/// The programmes on (and starting within the minute) when read, their
/// titles folded for search.
struct Airing {
    read_at: i64,
    generation: u64,
    programs: Vec<AiringProgram>,
}

struct AiringProgram {
    source_id: i64,
    guide_id: String,
    start: i64,
    stop: i64,
    /// The title folded (see [`crate::search::text::fold`]).
    title: Box<str>,
}

/// The channels of the enabled sources as they are now, hidden ones too.
pub(crate) struct Lineup {
    rows: Vec<ChannelRow>,
    /// Whether each row is shown: not hidden, and not offline while offline
    /// channels are left out.
    shown: Vec<bool>,
    /// Each channel's name and number in lower case, for finding it.
    names: Vec<String>,
    /// The groups of the channels shown, in the lineup's order.
    groups: Vec<Group>,
    /// The guide's channels (by source) with programmes to come, as the
    /// lineup was read: the guide is read again only with a change.
    guided: HashSet<(i64, String)>,
    /// Each source's short id, by source.
    source_uids: HashMap<i64, String>,
    /// The names as search finds them, made when first searched.
    search: std::sync::OnceLock<crate::search::Names>,
}

impl Lineup {
    fn has_guide(&self, c: &ChannelRow) -> bool {
        c.guide_key().is_some_and(|k| self.guided.contains(&(c.source_id, k.to_owned())))
    }

    /// The channels `search` finds by name (see [`crate::search::Names`]), by id.
    fn find(&self, search: &str) -> HashMap<i64, crate::search::Found> {
        let names = self.search.get_or_init(|| crate::search::Names::new(self.rows.iter().map(|c| c.display_name())));
        names.find(search).into_iter().map(|(i, how)| (self.rows[i].id, how)).collect()
    }
}

impl Lineup {
    fn shown(&self) -> impl Iterator<Item = &ChannelRow> {
        self.rows.iter().zip(&self.shown).filter(|(_, shown)| **shown).map(|(c, _)| c)
    }
}

/// Which of the lineup's channels administrators list.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LineupState {
    #[default]
    All,
    Shown,
    Hidden,
    NoGuide,
    Offline,
}

/// The lineup's channels to list or change: of a source, of a group, in a
/// state, matching a name or a number's beginning.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LineupFilter {
    #[serde(default)]
    pub source_id: Option<i64>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub state: LineupState,
    #[serde(default)]
    pub q: Option<String>,
}

/// A page of the lineup as administrators see it.
#[derive(Debug, Serialize)]
pub struct AdminPage {
    pub total: usize,
    /// Of `total`, how many are hidden.
    pub hidden: usize,
    pub offset: usize,
    pub channels: Vec<AdminChannel>,
}

/// A group as administrators see it: hidden channels counted too.
#[derive(Debug, Clone, Serialize)]
pub struct AdminGroup {
    #[serde(flatten)]
    pub group: Group,
    pub hidden: usize,
    /// Its channels that don't play (or are divider rows).
    pub offline: usize,
}

/// A group of channels, as the provider names it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Group {
    pub name: String,
    /// The region its name begins with ("UK" of "UK| Sport"), which
    /// providers with hundreds of groups name them by.
    pub region: Option<String>,
    /// The name without its region ("Sport").
    pub label: String,
    pub count: usize,
}

impl Group {
    fn new(name: &str) -> Self {
        let (region, label) = match split_region(name) {
            Some((region, label)) => (Some(region.to_owned()), label.to_owned()),
            None => (None, name.trim().to_owned()),
        };
        Self { name: name.to_owned(), region, label, count: 0 }
    }
}

/// Which channels to list.
#[derive(Debug, Clone, PartialEq)]
pub enum Category {
    All,
    Favorites,
    /// The ones watched last, newest first.
    Recent,
    Group(String),
}

/// A page of a list of channels.
#[derive(Debug, Serialize)]
pub struct ChannelPage {
    /// How many channels the whole list has.
    pub total: usize,
    pub offset: usize,
    pub channels: Vec<Channel>,
}

/// The groups `user` sees, with how many channels in all and marked.
#[derive(Debug, Serialize)]
pub struct Groups {
    pub total: usize,
    pub favorites: usize,
    pub recent: usize,
    pub groups: Vec<Group>,
}

impl LiveTv {
    pub async fn new(
        db: Db,
        events: EventBus,
        transcoder: Arc<Transcoder>,
        cache_dir: PathBuf,
    ) -> anyhow::Result<Arc<Self>> {
        let stored: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(SETTINGS_KEY)
            .fetch_optional(&db.read)
            .await?;
        let settings: LiveSettings = stored.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let http = reqwest::Client::builder()
            .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .build()?;
        let settings = RwLock::new(Arc::new(settings));
        let (played_tx, played) = tokio::sync::mpsc::unbounded_channel();
        let tuners = Tuners::new(transcoder, events.clone(), http.clone(), played_tx);
        let pictures = cache_dir.join("live").join("pictures");
        let downloads = cache_dir.join("live").join("downloads");
        let _ = std::fs::remove_dir_all(&downloads);
        let live = Arc::new(Self {
            db,
            events,
            settings,
            http,
            tuners,
            pictures,
            failed_pictures: Mutex::default(),
            pictures_pruned: Mutex::default(),
            downloads,
            refreshing: Mutex::default(),
            again: Mutex::default(),
            tried: Mutex::default(),
            refresh_lock: tokio::sync::Mutex::new(()),
            lineup: Mutex::default(),
            reading_lineup: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
            logos_ahead: Mutex::default(),
            airing: Mutex::default(),
        });
        // Whether channels played, as the tuners find out (until Live TV goes).
        let weak = Arc::downgrade(&live);
        let mut played = played;
        tokio::spawn(async move {
            while let Some(p) = played.recv().await {
                let Some(live) = weak.upgrade() else { break };
                live.mark_played(p).await;
            }
        });
        Ok(live)
    }

    /// Something about the channels changed: they are read again when next
    /// asked for, and the apps are told.
    fn changed(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        *self.lineup.lock() = None;
        self.events.send(ServerEvent::LiveChannelsChanged);
    }

    pub fn settings(&self) -> Arc<LiveSettings> {
        self.settings.read().clone()
    }

    pub async fn save_settings(&self, settings: LiveSettings) -> Result<Arc<LiveSettings>, AppError> {
        settings.check()?;
        let json = serde_json::to_string(&settings).map_err(|e| AppError::Internal(e.into()))?;
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(SETTINGS_KEY)
        .bind(json)
        .execute(&self.db.write)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        let settings = Arc::new(settings);
        *self.settings.write() = settings.clone();
        self.changed();
        Ok(settings)
    }

    /// Whether `user` may watch Live TV.
    /// Whether `user` may watch Live TV: administrators always (whatever
    /// signed them in), the people let in or kept out by name as they were,
    /// and the rest as `access` says.
    pub async fn may_watch(&self, user: &AuthUser) -> bool {
        let settings = self.settings();
        if user.is_admin || self.account_is_admin(user.id).await {
            return true;
        }
        if settings.denied_users.contains(&user.id) {
            return false;
        }
        if settings.allowed_users.contains(&user.id) {
            return true;
        }
        let age = user.max_age.is_none() || settings.age_limited;
        match settings.access {
            Access::Admins => false,
            Access::Home => age && self.in_home(user.id).await,
            Access::Everyone => age,
        }
    }

    pub async fn check_access(&self, user: &AuthUser) -> Result<(), AppError> {
        if self.may_watch(user).await { Ok(()) } else { Err(AppError::forbidden("Live TV isn't open to this account")) }
    }

    /// The account's name (its display name, else its username) when it is
    /// one of the household's (in Home).
    pub async fn home_name(&self, user_id: i64) -> Option<String> {
        sqlx::query_as::<_, (String, Option<String>)>("SELECT username, display_name FROM users WHERE id = ? AND home")
            .bind(user_id)
            .fetch_optional(&self.db.read)
            .await
            .ok()
            .flatten()
            .map(|(username, display)| display.filter(|d| !d.trim().is_empty()).unwrap_or(username))
    }

    /// The home as it is now, when the account is in it: its name, which
    /// the viewers whose connection it takes over are told when they are in
    /// the home too, and the accounts in it.
    pub async fn home_now(&self, user_id: i64) -> Option<tuner::InHome> {
        let name = self.home_name(user_id).await?;
        let accounts = sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE home")
            .fetch_all(&self.db.read)
            .await
            .ok()?
            .into_iter()
            .collect();
        Some(tuner::InHome { name, accounts })
    }

    /// Whether the account is one of the household's (in Home).
    async fn in_home(&self, user_id: i64) -> bool {
        sqlx::query_scalar::<_, bool>("SELECT home FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&self.db.read)
            .await
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    /// Reads sources that are due, every minute, until `stopping`.
    pub fn start(self: &Arc<Self>, stopping: CancellationToken) {
        self.tuners.start(stopping.clone());
        let this = self.clone();
        tokio::spawn(async move {
            // Leave start-up to the libraries first.
            tokio::select! {
                () = stopping.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(20)) => {}
            }
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = stopping.cancelled() => break,
                    _ = tick.tick() => {}
                }
                this.refresh_due().await;
            }
        });
    }

    async fn refresh_due(&self) {
        // A lineup changed meanwhile is read again now, not by the next app asking.
        let _ = self.lineup().await;
        let every = i64::from(self.settings().refresh_hours) * 3600;
        let Ok(sources) = self.source_rows().await else { return };
        for source in sources.into_iter().filter(|s| s.enabled) {
            let tried_lately = self.tried.lock().get(&source.id).is_some_and(|t| t.elapsed() < RETRY_AFTER);
            let channels_due = source.channels_at.is_none_or(|t| now() - t >= every);
            let guide_due =
                source.effective_guide_url().is_some() && source.guide_at.is_none_or(|t| now() - t >= every);
            if (channels_due || guide_due) && !tried_lately {
                self.refresh(source.id, channels_due, true).await;
            }
        }
        let _ = sqlx::query("DELETE FROM live_programs WHERE stop < ?")
            .bind(now() - GUIDE_BEHIND)
            .execute(&self.db.write)
            .await;
        self.recheck_offline().await;
        self.prune_pictures().await;
        self.fetch_logos_ahead().await;
    }

    /// Deletes the pictures nobody asked for lately, then the oldest while
    /// they take more than they may; hourly.
    async fn prune_pictures(&self) {
        {
            let mut last = self.pictures_pruned.lock();
            if last.is_some_and(|t| t.elapsed() < PICTURES_PRUNED_EVERY) {
                return;
            }
            *last = Some(Instant::now());
        }
        let dir = self.pictures.clone();
        let pruned = crate::util::blocking(move || crate::util::prune(&dir, PICTURE_DROP, PICTURES_MAX_BYTES)).await;
        match pruned {
            Ok(0) => {}
            Ok(n) => tracing::info!("{n} live TV pictures let go"),
            Err(err) => tracing::warn!("looking over the live TV pictures failed: {err:#}"),
        }
    }

    /// Fetches the logos of channels shown that nobody has looked at yet,
    /// in the lineup's order, each address once.
    async fn fetch_logos_ahead(&self) {
        let Ok(lineup) = self.lineup().await else { return };
        let urls: Vec<String> = {
            let done = self.logos_ahead.lock();
            let mut seen = HashSet::new();
            lineup
                .shown()
                .map(|c| c.logo_url.as_str())
                .filter(|u| !u.is_empty() && !done.contains(*u) && seen.insert(*u))
                .take(LOGOS_AHEAD)
                .map(str::to_owned)
                .collect()
        };
        futures::StreamExt::for_each_concurrent(futures::stream::iter(urls), LOGOS_AT_ONCE, |url| async move {
            // Kept, or failed and not tried again for a while: either way done.
            let _ = self.picture(&url, true).await;
            self.logos_ahead.lock().insert(url);
        })
        .await;
    }

    /// A channel played, or gave nothing: offline until it plays again.
    async fn mark_played(&self, p: Played) {
        let at = now();
        let query = if p.played {
            sqlx::query(
                "UPDATE live_channels SET offline_since = NULL, offline_checked = NULL \
                 WHERE id = ?1 AND offline_since IS NOT NULL",
            )
        } else {
            sqlx::query(
                "UPDATE live_channels SET offline_since = ?2, offline_checked = ?2 \
                 WHERE id = ?1 AND offline_since IS NULL",
            )
        };
        match query.bind(p.channel_id).bind(at).execute(&self.db.write).await {
            Ok(done) if done.rows_affected() > 0 => {
                tracing::info!(
                    channel = p.channel_id,
                    "live channel {}",
                    if p.played { "plays again" } else { "offline" }
                );
                self.changed();
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(channel = p.channel_id, "a live channel's state wasn't saved: {err}"),
        }
    }

    /// Tries channels that didn't play again, one at a time, on sources
    /// nobody is watching (a try takes one of their connections).
    async fn recheck_offline(&self) {
        let Ok(sources) = self.source_rows().await else { return };
        for source in sources.into_iter().filter(|s| s.enabled) {
            let sql = format!(
                "SELECT {CHANNEL_COLUMNS} FROM live_channels c WHERE c.source_id = ? AND c.offline_since IS NOT NULL \
                 AND c.missing_since IS NULL AND c.hidden = 0 AND (c.offline_checked IS NULL OR c.offline_checked < ?) \
                 ORDER BY c.offline_checked LIMIT ?"
            );
            let Ok(due) = sqlx::query_as::<_, ChannelRow>(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(source.id)
                .bind(now() - RECHECK_AFTER)
                .bind(RECHECKS)
                .fetch_all(&self.db.read)
                .await
            else {
                continue;
            };
            for channel in due {
                if self.tuners.source_busy(source.id) {
                    break;
                }
                let agent =
                    channel.user_agent.clone().filter(|a| !a.is_empty()).unwrap_or_else(|| source.user_agent.clone());
                if self.plays(&channel.stream_url, &agent).await {
                    self.mark_played(Played { channel_id: channel.id, played: true }).await;
                } else {
                    let _ = sqlx::query("UPDATE live_channels SET offline_checked = ? WHERE id = ?")
                        .bind(now())
                        .bind(channel.id)
                        .execute(&self.db.write)
                        .await;
                }
            }
        }
    }

    /// Whether a channel's stream sends something within a few seconds.
    async fn plays(&self, url: &str, agent: &str) -> bool {
        let tried = async {
            let mut req = self.http.get(url);
            if !agent.is_empty() {
                req = req.header(reqwest::header::USER_AGENT, agent);
            }
            let mut resp = req.send().await.ok()?;
            if !resp.status().is_success() {
                return Some(false);
            }
            let mut got = 0;
            while let Some(chunk) = resp.chunk().await.ok()? {
                got += chunk.len();
                if got >= PLAYS_BYTES {
                    return Some(true);
                }
            }
            // A provider's HLS playlist: short, and it answered.
            Some(got > 0)
        };
        matches!(tokio::time::timeout(Duration::from_secs(15), tried).await, Ok(Some(true)))
    }

    // ------------------------------------------------------------ sources

    /// Whether the account is an administrator's, whatever signed it in.
    pub async fn account_is_admin(&self, user_id: i64) -> bool {
        sqlx::query_scalar::<_, bool>("SELECT is_admin FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&self.db.read)
            .await
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    /// How many sources are set up.
    pub async fn source_count(&self) -> Result<usize, AppError> {
        Ok(self.source_rows().await.map_err(|e| AppError::Internal(e.into()))?.len())
    }

    /// Each source's short id, by source.
    async fn source_uids(&self) -> Result<HashMap<i64, String>, AppError> {
        Ok(self.lineup().await?.source_uids.clone())
    }

    async fn source_rows(&self) -> sqlx::Result<Vec<SourceRow>> {
        sqlx::query_as("SELECT * FROM live_sources ORDER BY id").fetch_all(&self.db.read).await
    }

    async fn source_row(&self, id: i64) -> Result<SourceRow, AppError> {
        sqlx::query_as("SELECT * FROM live_sources WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db.read)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::not_found(format!("live TV source {id} not found")))
    }

    pub async fn sources(&self) -> Result<Vec<Source>, AppError> {
        let rows = self.source_rows().await.map_err(|e| AppError::Internal(e.into()))?;
        let counts: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT source_id, COUNT(*), SUM(hidden) FROM live_channels WHERE missing_since IS NULL GROUP BY source_id",
        )
        .fetch_all(&self.db.read)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        let guided = self.guided_counts().await?;
        let in_use = self.tuners.in_use();
        let refreshing = self.refreshing.lock().clone();
        Ok(rows
            .into_iter()
            .map(|r| {
                let (count, hidden) =
                    counts.iter().find(|(id, ..)| *id == r.id).map(|(_, c, h)| (*c, *h)).unwrap_or((0, 0));
                Source {
                    kind: r.kind(),
                    account: r.account.as_deref().and_then(|a| serde_json::from_str(a).ok()),
                    password_set: !r.password.is_empty(),
                    channel_count: count,
                    hidden_count: hidden,
                    guided_count: guided.get(&r.id).copied().unwrap_or(0),
                    connections_in_use: in_use.get(&r.id).copied().unwrap_or(0),
                    refreshing: refreshing.contains(&r.id),
                    id: r.id,
                    uid: r.uid,
                    name: r.name,
                    url: r.url,
                    username: r.username,
                    guide_url: r.guide_url,
                    user_agent: r.user_agent,
                    max_connections: r.max_connections,
                    enabled: r.enabled,
                    created_at: r.created_at,
                    channels_at: r.channels_at,
                    channels_error: r.channels_error,
                    guide_at: r.guide_at,
                    guide_error: r.guide_error,
                }
            })
            .collect())
    }

    /// Channels with programmes now or ahead, by source.
    async fn guided_counts(&self) -> Result<HashMap<i64, i64>, AppError> {
        let lineup = self.lineup().await?;
        let mut out = HashMap::new();
        for c in lineup.rows.iter().filter(|c| lineup.has_guide(c)) {
            *out.entry(c.source_id).or_insert(0) += 1;
        }
        Ok(out)
    }

    fn check_input(input: &SourceInput) -> Result<(), AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::bad_request("a source needs a name"));
        }
        let url = url::Url::parse(input.url.trim()).map_err(|_| AppError::bad_request("the address isn't a URL"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(AppError::bad_request("the address must start with http:// or https://"));
        }
        if input.kind == SourceKind::Xtream && input.username.trim().is_empty() {
            return Err(AppError::bad_request("an Xtream Codes account needs its user name"));
        }
        if !input.guide_url.trim().is_empty()
            && url::Url::parse(input.guide_url.trim()).is_err_and(|_| !input.guide_url.trim().starts_with('/'))
        {
            return Err(AppError::bad_request("the guide's address isn't a URL"));
        }
        // A file is checked as its refresh will read it, so that the source
        // isn't saved with a guide it never gets.
        if let Some(Err(err)) = guide_file(input.guide_url.trim()) {
            return Err(AppError::bad_request(err.to_string()));
        }
        if !(0..=100).contains(&input.max_connections) {
            return Err(AppError::bad_request("connections must be between 0 (no limit) and 100"));
        }
        Ok(())
    }

    pub async fn add_source(self: &Arc<Self>, input: SourceInput) -> Result<Source, AppError> {
        Self::check_input(&input)?;
        let url = match input.kind {
            SourceKind::Xtream => xtream::base(&input.url),
            SourceKind::M3u => input.url.trim().to_owned(),
        };
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO live_sources (uid, name, kind, url, username, password, guide_url, user_agent, \
             max_connections, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(new_source_uid())
        .bind(input.name.trim())
        .bind(input.kind.as_str())
        .bind(url)
        .bind(input.username.trim())
        .bind(input.password.unwrap_or_default())
        .bind(input.guide_url.trim())
        .bind(input.user_agent.trim())
        .bind(input.max_connections)
        .bind(input.enabled)
        .bind(now())
        .fetch_one(&self.db.write)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        self.refresh_soon(id);
        self.source(id).await
    }

    pub async fn update_source(self: &Arc<Self>, id: i64, input: SourceInput) -> Result<Source, AppError> {
        Self::check_input(&input)?;
        let before = self.source_row(id).await?;
        let url = match input.kind {
            SourceKind::Xtream => xtream::base(&input.url),
            SourceKind::M3u => input.url.trim().to_owned(),
        };
        let password = input.password.clone().unwrap_or(before.password.clone());
        let reread = url != before.url
            || input.kind != before.kind()
            || input.username.trim() != before.username
            || password != before.password
            || input.guide_url.trim() != before.guide_url
            || (input.enabled && !before.enabled);
        sqlx::query(
            "UPDATE live_sources SET name = ?, kind = ?, url = ?, username = ?, password = ?, guide_url = ?, \
             user_agent = ?, max_connections = ?, enabled = ? WHERE id = ?",
        )
        .bind(input.name.trim())
        .bind(input.kind.as_str())
        .bind(url)
        .bind(input.username.trim())
        .bind(password)
        .bind(input.guide_url.trim())
        .bind(input.user_agent.trim())
        .bind(input.max_connections)
        .bind(input.enabled)
        .bind(id)
        .execute(&self.db.write)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if !input.enabled {
            self.tuners.stop_source(id).await;
        }
        if reread {
            self.refresh_soon(id);
        }
        self.changed();
        self.source(id).await
    }

    pub async fn delete_source(&self, id: i64) -> Result<(), AppError> {
        self.tuners.stop_source(id).await;
        let done = sqlx::query("DELETE FROM live_sources WHERE id = ?")
            .bind(id)
            .execute(&self.db.write)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        if done.rows_affected() == 0 {
            return Err(AppError::not_found(format!("live TV source {id} not found")));
        }
        self.changed();
        Ok(())
    }

    pub async fn source(&self, id: i64) -> Result<Source, AppError> {
        self.sources()
            .await?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::not_found(format!("live TV source {id} not found")))
    }

    /// Reads a source again in the background, after a read under way.
    pub fn refresh_soon(self: &Arc<Self>, id: i64) {
        let this = self.clone();
        tokio::spawn(async move { this.read_source(id, true, true, Asker::Admin).await });
    }

    /// Reads a source's details without saving them: what an administrator
    /// is about to add.
    pub async fn test_source(&self, input: SourceInput, saved: Option<i64>) -> Result<SourceTest, AppError> {
        Self::check_input(&input)?;
        let password = match (input.password.clone(), saved) {
            (Some(p), _) => p,
            (None, Some(id)) => self.source_row(id).await?.password,
            (None, None) => String::new(),
        };
        let row = SourceRow {
            id: 0,
            uid: String::new(),
            name: input.name.clone(),
            kind: input.kind.as_str().into(),
            url: if input.kind == SourceKind::Xtream { xtream::base(&input.url) } else { input.url.trim().into() },
            username: input.username.trim().into(),
            password,
            guide_url: input.guide_url.trim().into(),
            user_agent: input.user_agent.trim().into(),
            max_connections: input.max_connections,
            enabled: true,
            created_at: now(),
            channels_at: None,
            channels_error: None,
            guide_at: None,
            guide_error: None,
            account: None,
        };
        let listed = self.list_channels(&row).await.map_err(|e| AppError::BadRequest(format!("{e:#}")))?;
        let groups: HashSet<&str> = listed.entries.iter().map(|e| e.group.as_str()).collect();
        Ok(SourceTest {
            channels: listed.entries.len(),
            groups: groups.len(),
            guide_url: listed.guide_url.or_else(|| row.effective_guide_url()).map(|u| redact(&u, &row.password)),
            account: listed.account,
        })
    }

    /// Reads a source's channels (when `channels`) and its guide (when
    /// `guide`) as they fall due, saying how it went in the source.
    pub async fn refresh(&self, id: i64, channels: bool, guide: bool) {
        self.read_source(id, channels, guide, Asker::Schedule).await;
    }

    /// [`Self::refresh`] for `asker`.
    async fn read_source(&self, id: i64, mut channels: bool, mut guide: bool, asker: Asker) {
        {
            let mut refreshing = self.refreshing.lock();
            if !refreshing.insert(id) {
                match asker {
                    Asker::Schedule => {}
                    Asker::Admin => {
                        self.again.lock().insert(id, true);
                    }
                    Asker::ChannelsSet => {
                        self.again.lock().entry(id).or_insert(false);
                    }
                }
                return;
            }
        }
        if asker != Asker::ChannelsSet {
            self.tried.lock().insert(id, Instant::now());
        }
        self.changed();
        // One source at a time: guides are large.
        let _one = self.refresh_lock.lock().await;
        loop {
            self.read_source_once(id, channels, guide).await;
            let next = {
                let mut refreshing = self.refreshing.lock();
                let next = self.again.lock().remove(&id);
                if next.is_none() {
                    refreshing.remove(&id);
                }
                next
            };
            let Some(next_channels) = next else { break };
            (channels, guide) = (next_channels, true);
        }
        self.changed();
    }

    async fn read_source_once(&self, id: i64, channels: bool, guide: bool) {
        if channels {
            let result = self.refresh_channels(id).await;
            if let Err(err) = &result {
                tracing::warn!(source = id, "reading the live TV channels failed: {err:#}");
            }
            let _ = sqlx::query("UPDATE live_sources SET channels_error = ? WHERE id = ?")
                .bind(result.err().map(|e| format!("{e:#}")))
                .bind(id)
                .execute(&self.db.write)
                .await;
            self.changed();
        }
        if guide {
            self.read_guide(id).await;
        }
    }

    /// Reads a source's guide, saying in the source how it went.
    async fn read_guide(&self, id: i64) {
        let result = self.refresh_guide(id).await;
        match &result {
            Ok(Some(n)) => tracing::info!(source = id, programs = n, "live TV guide read"),
            Ok(None) => {}
            Err(err) => tracing::warn!(source = id, "reading the live TV guide failed: {err:#}"),
        }
        if let Err(err) = result {
            let _ = sqlx::query("UPDATE live_sources SET guide_error = ? WHERE id = ?")
                .bind(format!("{err:#}"))
                .bind(id)
                .execute(&self.db.write)
                .await;
        }
    }

    /// Reads again the guides of the sources of channels `ids`, given a
    /// guide channel with no programmes (when `guide_set`) or a name that
    /// decides their guide channel (when `renamed`). A guide read keeps the
    /// guide channels some channel took then, by guide channel or by name,
    /// so one set by hand since has nothing until the next read. A source
    /// being read now is read once more after it, unless that read hadn't
    /// chosen its channels yet.
    async fn read_guides_wanted(&self, ids: &[i64], guide_set: bool, renamed: bool) -> Result<(), AppError> {
        let ids: HashSet<i64> = ids.iter().copied().collect();
        let lineup = self.lineup().await?;
        // The name decides only for channels given no guide channel by hand,
        // whose playlist's guide channel the guide lacks: none, or one with
        // no programmes (see `ChannelRow::guide_key`).
        let by_name = |c: &ChannelRow| {
            c.custom_guide_id.as_deref().is_none_or(str::is_empty)
                && (c.guide_id.is_empty() || !lineup.guided.contains(&(c.source_id, c.guide_id.clone())))
        };
        let unguided: HashSet<i64> = lineup
            .rows
            .iter()
            .filter(|c| ids.contains(&c.id) && ((guide_set && !lineup.has_guide(c)) || (renamed && by_name(c))))
            .map(|c| c.source_id)
            .collect();
        if unguided.is_empty() {
            return Ok(());
        }
        let sources = self.source_rows().await.map_err(|e| AppError::Internal(e.into()))?;
        for source in sources {
            if !source.enabled || source.effective_guide_url().is_none() || !unguided.contains(&source.id) {
                continue;
            }
            self.read_source(source.id, false, true, Asker::ChannelsSet).await;
        }
        Ok(())
    }

    async fn list_channels(&self, source: &SourceRow) -> anyhow::Result<Listed> {
        match source.kind() {
            SourceKind::M3u => {
                let bytes = self.download(&source.url, &source.user_agent, MAX_PLAYLIST_BYTES).await?;
                let text = String::from_utf8_lossy(&bytes);
                if !text.trim_start_matches('\u{feff}').trim_start().starts_with("#EXTM3U") && !text.contains("#EXTINF")
                {
                    bail!("the address doesn't give an M3U playlist");
                }
                let playlist = m3u::parse(&text);
                if playlist.entries.is_empty() {
                    bail!("the playlist lists no channels");
                }
                Ok(Listed { entries: playlist.entries, guide_url: playlist.guide_url, account: None })
            }
            SourceKind::Xtream => {
                let base = xtream::base(&source.url);
                let get = |action: Option<&'static str>| {
                    let url = xtream::api_url(&base, &source.username, &source.password, action);
                    async move {
                        let bytes = self
                            .download(&url, &source.user_agent, MAX_PLAYLIST_BYTES)
                            .await
                            .map_err(|e| anyhow::anyhow!(redact(&format!("{e:#}"), &source.password)))?;
                        serde_json::from_slice::<serde_json::Value>(&bytes)
                            .context("the server didn't answer as an Xtream Codes server does")
                    }
                };
                let account = xtream::account(&get(None).await?).map_err(|e| anyhow::anyhow!(e))?;
                if !account.status.eq_ignore_ascii_case("active") {
                    bail!("the provider says the account is {}", account.status.to_lowercase());
                }
                let categories = xtream::categories(&get(Some("get_live_categories")).await.unwrap_or_default());
                let streams = xtream::streams(&get(Some("get_live_streams")).await?, &categories);
                if streams.is_empty() {
                    bail!("the account has no live channels");
                }
                let entries = streams
                    .into_iter()
                    .map(|s| m3u::Entry {
                        key: s.id.to_string(),
                        url: xtream::stream_url(&base, &source.username, &source.password, s.id),
                        name: s.name,
                        number: s.number,
                        group: s.category,
                        logo: s.logo,
                        guide_id: s.guide_id,
                        user_agent: None,
                    })
                    .collect();
                Ok(Listed { entries, guide_url: None, account: Some(account) })
            }
        }
    }

    async fn refresh_channels(&self, id: i64) -> anyhow::Result<()> {
        let source = self.source_row(id).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let listed = self.list_channels(&source).await?;
        let at = now();
        let mut tx = self.db.write.begin().await?;
        sqlx::query("UPDATE live_channels SET missing_since = COALESCE(missing_since, ?) WHERE source_id = ?")
            .bind(at)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for (position, e) in listed.entries.iter().enumerate() {
            sqlx::query(
                "INSERT INTO live_channels (source_id, key, name, number, group_name, logo_url, guide_id, stream_url, \
                 user_agent, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(source_id, key) DO UPDATE SET name = excluded.name, number = excluded.number, \
                 group_name = excluded.group_name, logo_url = excluded.logo_url, \
                 matched_guide_id = CASE WHEN guide_id = excluded.guide_id THEN matched_guide_id END, \
                 guide_id = excluded.guide_id, \
                 offline_since = CASE WHEN stream_url = excluded.stream_url THEN offline_since END, \
                 stream_url = excluded.stream_url, user_agent = excluded.user_agent, position = excluded.position, \
                 missing_since = NULL",
            )
            .bind(id)
            .bind(&e.key)
            .bind(&e.name)
            .bind(&e.number)
            .bind(&e.group)
            .bind(&e.logo)
            .bind(&e.guide_id)
            .bind(&e.url)
            .bind(&e.user_agent)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM live_channels WHERE source_id = ? AND missing_since < ?")
            .bind(id)
            .bind(at - MISSING_KEPT)
            .execute(&mut *tx)
            .await?;
        let mut set = String::from("UPDATE live_sources SET channels_at = ?, channels_error = NULL");
        if source.guide_url.is_empty() && listed.guide_url.is_some() {
            set.push_str(", guide_url = ?");
        }
        if listed.account.is_some() {
            set.push_str(", account = ?");
        }
        set.push_str(" WHERE id = ?");
        let mut query = sqlx::query(sqlx::AssertSqlSafe(set.as_str())).bind(at);
        if source.guide_url.is_empty()
            && let Some(guide) = &listed.guide_url
        {
            query = query.bind(guide);
        }
        if let Some(account) = &listed.account {
            query = query.bind(serde_json::to_string(account)?);
        }
        query.bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        tracing::info!(source = id, channels = listed.entries.len(), "live TV channels read");
        Ok(())
    }

    /// Reads the source's guide: the number of programmes kept, `None` when
    /// it has no guide.
    async fn refresh_guide(&self, id: i64) -> anyhow::Result<Option<usize>> {
        let source = self.source_row(id).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let Some(url) = source.effective_guide_url() else { return Ok(None) };
        let path = self.download_guide(&url, &source).await?;
        // Channels set by hand until now are seen by this read: no guide read
        // after it for them (one asked for by an administrator stays).
        {
            let mut again = self.again.lock();
            if again.get(&id) == Some(&false) {
                again.remove(&id);
            }
        }
        let channels = self.channel_rows(Some(id), true).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let days = i64::from(self.settings().guide_days);
        let (from, to) = (now() - GUIDE_BEHIND, now() + days * 86_400);
        let read_path = path.clone();
        let channels_for_match = channels.clone();
        let (guide, matches) = crate::util::blocking(move || {
            let file = std::fs::File::open(&read_path)?;
            let mut reader = std::io::BufReader::with_capacity(1 << 20, file);
            let gzipped = std::io::BufRead::fill_buf(&mut reader)?.starts_with(&[0x1f, 0x8b]);
            let input: Box<dyn std::io::BufRead> = if gzipped {
                Box::new(std::io::BufReader::new(flate2::bufread::MultiGzDecoder::new(reader)))
            } else {
                Box::new(reader)
            };
            let mut matches: HashMap<i64, String> = HashMap::new();
            let guide = xmltv::read(input, from, to, |guide_channels| {
                let index = GuideIndex::new(guide_channels);
                let mut wanted = HashSet::new();
                for c in &channels_for_match {
                    for id in [c.custom_guide_id.as_deref(), Some(c.guide_id.as_str())].into_iter().flatten() {
                        if index.ids.contains(id) {
                            wanted.insert(id.to_owned());
                        }
                    }
                    // Made whether or not a guide channel is set, so that
                    // clearing it falls back to this at once.
                    if let Some(id) = index.matching(&c.guide_id, c.display_name()) {
                        matches.insert(c.id, id.to_owned());
                        wanted.insert(id.to_owned());
                    }
                }
                wanted
            })
            .context("the guide isn't XMLTV")?;
            Ok((guide, matches))
        })
        .await?;
        let _ = tokio::fs::remove_file(&path).await;
        if guide.channels.is_empty() {
            bail!("the guide lists no channels");
        }
        let count = guide.programs.len();
        // The old programmes go, then the new ones come in batches: the
        // database has one writer, which others wait for meanwhile.
        let mut tx = self.db.write.begin().await?;
        sqlx::query("DELETE FROM live_programs WHERE source_id = ?").bind(id).execute(&mut *tx).await?;
        for c in &channels {
            sqlx::query("UPDATE live_channels SET matched_guide_id = ? WHERE id = ?")
                .bind(matches.get(&c.id))
                .bind(c.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        for batch in guide.programs.chunks(BATCH) {
            let mut tx = self.db.write.begin().await?;
            for p in batch {
                sqlx::query(
                    "INSERT OR REPLACE INTO live_programs (source_id, guide_id, start, stop, title, subtitle, description, \
                     categories, image, season, episode, is_new, is_live, rating) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(&p.channel)
                .bind(p.start)
                .bind(p.stop)
                .bind(&p.title)
                .bind(&p.subtitle)
                .bind(&p.description)
                .bind((!p.categories.is_empty()).then(|| serde_json::to_string(&p.categories).unwrap_or_default()))
                .bind(&p.image)
                .bind(p.season)
                .bind(p.episode)
                .bind(p.is_new)
                .bind(p.is_live)
                .bind(&p.rating)
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
        }
        sqlx::query("UPDATE live_sources SET guide_at = ?, guide_error = NULL WHERE id = ?")
            .bind(now())
            .bind(id)
            .execute(&self.db.write)
            .await?;
        Ok(Some(count))
    }

    async fn download(&self, url: &str, user_agent: &str, max: usize) -> anyhow::Result<Vec<u8>> {
        let mut req = self.http.get(url).timeout(Duration::from_secs(120));
        if !user_agent.is_empty() {
            req = req.header(reqwest::header::USER_AGENT, user_agent);
        }
        let mut resp = req.send().await.map_err(|e| anyhow::anyhow!("{}", describe(&e)))?;
        if !resp.status().is_success() {
            bail!("the provider answered {}", resp.status());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            anyhow::ensure!(bytes.len() + chunk.len() <= max, "the answer is larger than {} MB", max >> 20);
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    /// The guide at `url` (or a file on this computer) in a file to read.
    async fn download_guide(&self, url: &str, source: &SourceRow) -> anyhow::Result<PathBuf> {
        use tokio::io::AsyncWriteExt;
        tokio::fs::create_dir_all(&self.downloads).await?;
        let path = self.downloads.join(format!("{}.xml", uuid::Uuid::new_v4().simple()));
        if let Some(local) = guide_file(url) {
            // Also one a playlist named before playlists' guides had to be
            // web addresses: a file of a guide's size on this computer, not
            // a device or a pipe, which would never end.
            let local = local?;
            let meta =
                tokio::fs::metadata(&local).await.with_context(|| format!("{} can't be read", local.display()))?;
            anyhow::ensure!(meta.is_file(), "{} isn't a file", local.display());
            anyhow::ensure!(meta.len() <= MAX_GUIDE_BYTES, "the guide is larger than {} GB", MAX_GUIDE_BYTES >> 30);
            if let Err(err) = tokio::fs::copy(&local, &path).await {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(err).with_context(|| format!("{} can't be read", local.display()));
            }
            return Ok(path);
        }
        let mut req = self.http.get(url).timeout(Duration::from_secs(15 * 60));
        if !source.user_agent.is_empty() {
            req = req.header(reqwest::header::USER_AGENT, &source.user_agent);
        }
        let mut resp = req.send().await.map_err(|e| anyhow::anyhow!("{}", redact(&describe(&e), &source.password)))?;
        if !resp.status().is_success() {
            bail!("the guide's server answered {}", resp.status());
        }
        let mut file = tokio::fs::File::create(&path).await?;
        let mut written = 0u64;
        while let Some(chunk) = resp.chunk().await? {
            written += chunk.len() as u64;
            if written > MAX_GUIDE_BYTES {
                drop(file);
                let _ = tokio::fs::remove_file(&path).await;
                bail!("the guide is larger than {} GB", MAX_GUIDE_BYTES >> 30);
            }
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(path)
    }

    // ----------------------------------------------------------- channels

    /// Channels of `source` (every source's when `None`) its list has now;
    /// hidden ones too when `hidden`. Enabled sources' only. From the
    /// database: [`Self::lineup`] keeps them.
    pub(crate) async fn channel_rows(&self, source: Option<i64>, hidden: bool) -> Result<Vec<ChannelRow>, AppError> {
        let sql = format!(
            "SELECT {CHANNEL_COLUMNS} FROM live_channels c JOIN live_sources s ON s.id = c.source_id \
             WHERE c.missing_since IS NULL AND s.enabled = 1 AND (?1 IS NULL OR c.source_id = ?1) \
             AND (?2 = 1 OR c.hidden = 0) ORDER BY c.source_id, c.position"
        );
        let mut rows: Vec<ChannelRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(source)
            .bind(hidden)
            .fetch_all(&self.db.read)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        sort_channels(&mut rows);
        Ok(rows)
    }

    pub(crate) async fn channel_row(&self, id: i64) -> Result<ChannelRow, AppError> {
        let sql = format!(
            "SELECT {CHANNEL_COLUMNS} FROM live_channels c JOIN live_sources s ON s.id = c.source_id \
             WHERE c.id = ? AND c.missing_since IS NULL AND s.enabled = 1"
        );
        sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(&self.db.read)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::not_found(format!("channel {id} not found")))
    }

    /// The channel's source: its connection limit and user agent.
    pub(crate) async fn source_of(&self, channel: &ChannelRow) -> Result<(String, i64, String), AppError> {
        let s = self.source_row(channel.source_id).await?;
        let agent = channel.user_agent.clone().filter(|a| !a.is_empty()).unwrap_or(s.user_agent);
        Ok((s.name, s.max_connections, agent))
    }

    async fn favorites(&self, user_id: i64) -> Result<HashSet<i64>, AppError> {
        Ok(sqlx::query_scalar::<_, i64>("SELECT channel_id FROM live_favorites WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(&self.db.read)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .into_iter()
            .collect())
    }

    /// The lineup, read again when something changed since it was last.
    pub(crate) async fn lineup(&self) -> Result<Arc<Lineup>, AppError> {
        if let Some(lineup) = self.lineup.lock().clone() {
            return Ok(lineup);
        }
        let _one = self.reading_lineup.lock().await;
        // Read by whoever went first.
        if let Some(lineup) = self.lineup.lock().clone() {
            return Ok(lineup);
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let rows = self.channel_rows(None, true).await?;
        let names = rows.iter().map(|c| c.display_name().to_lowercase()).collect();
        let hide_offline = self.settings().hide_offline;
        let shown: Vec<bool> = rows.iter().map(|c| !c.hidden && !(hide_offline && c.offline())).collect();
        let mut groups: Vec<Group> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        for c in rows.iter().zip(&shown).filter(|(c, shown)| **shown && !c.group_name.is_empty()).map(|(c, _)| c) {
            let at = *index.entry(&c.group_name).or_insert_with(|| {
                groups.push(Group::new(&c.group_name));
                groups.len() - 1
            });
            groups[at].count += 1;
        }
        let guided =
            sqlx::query_as::<_, (i64, String)>("SELECT DISTINCT source_id, guide_id FROM live_programs WHERE stop > ?")
                .bind(now())
                .fetch_all(&self.db.read)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
                .into_iter()
                .collect();
        let source_uids = self
            .source_rows()
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .into_iter()
            .map(|s| (s.id, s.uid))
            .collect();
        let lineup = Arc::new(Lineup { rows, shown, names, groups, guided, source_uids, search: Default::default() });
        let mut kept = self.lineup.lock();
        // A change while it was read: this one is already out of date.
        if self.generation.load(Ordering::SeqCst) == generation {
            *kept = Some(lineup.clone());
        }
        Ok(lineup)
    }

    /// Programmes between `from` and `to` of `channels`, by channel.
    async fn programs_of(
        &self,
        channels: &[&ChannelRow],
        from: i64,
        to: i64,
    ) -> Result<HashMap<i64, Vec<Program>>, AppError> {
        let mut wanted: HashMap<i64, HashSet<&str>> = HashMap::new();
        for c in channels {
            if let Some(key) = c.guide_key() {
                wanted.entry(c.source_id).or_default().insert(key);
            }
        }
        let rows: Vec<ProgramRow> = if wanted.values().map(HashSet::len).sum::<usize>() > 2000 {
            // Most of a lineup: one pass over the time asked for.
            sqlx::query_as("SELECT * FROM live_programs WHERE stop > ? AND start < ? ORDER BY start")
                .bind(from)
                .bind(to)
                .fetch_all(&self.db.read)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
        } else {
            // A page of channels: theirs only, by the table's key.
            let mut rows = Vec::new();
            for (source, keys) in &wanted {
                let keys: Vec<&str> = keys.iter().copied().collect();
                for batch in keys.chunks(500) {
                    let sql = format!(
                        "SELECT * FROM live_programs WHERE source_id = ? AND guide_id IN ({}) AND stop > ? AND start < ? \
                         ORDER BY start",
                        vec!["?"; batch.len()].join(", ")
                    );
                    let mut query = sqlx::query_as::<_, ProgramRow>(sqlx::AssertSqlSafe(sql.as_str())).bind(source);
                    for key in batch {
                        query = query.bind(*key);
                    }
                    rows.extend(
                        query
                            .bind(from)
                            .bind(to)
                            .fetch_all(&self.db.read)
                            .await
                            .map_err(|e| AppError::Internal(e.into()))?,
                    );
                }
            }
            rows
        };
        let mut by_key: HashMap<(i64, &str), Vec<&ProgramRow>> = HashMap::new();
        for r in &rows {
            by_key.entry((r.source_id, r.guide_id.as_str())).or_default().push(r);
        }
        let mut out = HashMap::new();
        for c in channels {
            if let Some(list) = c.guide_key().and_then(|k| by_key.get(&(c.source_id, k))) {
                out.insert(c.id, list.iter().map(|r| r.for_channel(c.id)).collect());
            }
        }
        Ok(out)
    }

    fn channel_out(
        c: &ChannelRow,
        source_uid: &str,
        favorites: &HashSet<i64>,
        programs: Option<&Vec<Program>>,
    ) -> Channel {
        let at = now();
        let mut upcoming = programs.into_iter().flatten().filter(|p| p.stop > at);
        let first = upcoming.next().cloned();
        let (now_on, next) = match first {
            Some(p) if p.start <= at => (Some(p), upcoming.next().cloned()),
            Some(p) => (None, Some(p)),
            None => (None, None),
        };
        Channel {
            id: c.id,
            source_id: c.source_id,
            source_uid: source_uid.to_owned(),
            name: c.display_name().to_owned(),
            number: c.display_number(),
            group: c.group_name.clone(),
            logo: logo_path(c),
            favorite: favorites.contains(&c.id),
            has_guide: programs.is_some_and(|p| !p.is_empty()),
            now: now_on,
            next,
        }
    }

    /// Channels as apps get them, with what's on now and next.
    async fn channels_out(&self, user: &AuthUser, rows: &[&ChannelRow]) -> Result<Vec<Channel>, AppError> {
        let favorites = self.favorites(user.id).await?;
        let programs = self.programs_of(rows, now(), now() + 12 * 3600).await?;
        let uids = self.source_uids().await?;
        Ok(rows
            .iter()
            .map(|c| {
                let uid = uids.get(&c.source_id).map_or("", String::as_str);
                Self::channel_out(c, uid, &favorites, programs.get(&c.id))
            })
            .collect())
    }

    /// How many channels are shown.
    pub async fn channel_count(&self) -> Result<usize, AppError> {
        Ok(self.lineup().await?.shown().count())
    }

    /// Every channel `user` sees, with what's on now and next.
    pub async fn channels(&self, user: &AuthUser) -> Result<Vec<Channel>, AppError> {
        let lineup = self.lineup().await?;
        let rows: Vec<&ChannelRow> = lineup.shown().collect();
        self.channels_out(user, &rows).await
    }

    pub async fn channel(&self, user: &AuthUser, id: i64) -> Result<Channel, AppError> {
        let row = self.channel_row(id).await?;
        if row.hidden {
            return Err(AppError::not_found(format!("channel {id} not found")));
        }
        self.channels_out(user, &[&row])
            .await?
            .pop()
            .ok_or_else(|| AppError::not_found(format!("channel {id} not found")))
    }

    /// The groups `user` sees, and how many channels in all, marked and watched lately.
    pub async fn groups(&self, user: &AuthUser) -> Result<Groups, AppError> {
        let lineup = self.lineup().await?;
        let favorites = self.favorites(user.id).await?;
        let shown: HashSet<i64> = lineup.shown().map(|c| c.id).collect();
        Ok(Groups {
            total: shown.len(),
            favorites: favorites.iter().filter(|id| shown.contains(id)).count(),
            recent: self.recent(user, RECENT).await?.len(),
            groups: lineup.groups.clone(),
        })
    }

    /// A page of the channels of `category` that match `search` (a name, a
    /// number's beginning, or the title of what's on): `limit` from
    /// `offset`, or the page with channel `around` in it.
    pub async fn browse(
        &self,
        user: &AuthUser,
        category: &Category,
        search: &str,
        offset: usize,
        limit: usize,
        around: Option<i64>,
    ) -> Result<ChannelPage, AppError> {
        let lineup = self.lineup().await?;
        let search = search.trim().to_lowercase();
        let airing = if search.is_empty() { HashSet::new() } else { self.airing_titled(&search).await? };
        // By name: as typed, its words in any order, words read as others.
        // On a thread of its own: the first search of a lineup reads its names.
        let found = if search.is_empty() {
            HashMap::new()
        } else {
            let (lineup, search) = (lineup.clone(), search.clone());
            tokio::task::spawn_blocking(move || lineup.find(&search))
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("searching the lineup: {e}")))?
        };
        let by_number_or_airing = |c: &ChannelRow| {
            c.display_number().is_some_and(|n| n.starts_with(&search))
                || c.guide_key().is_some_and(|k| airing.contains(&(c.source_id, k.to_owned())))
        };
        let only_alike = |c: &ChannelRow| {
            matches!(found.get(&c.id), Some(crate::search::Found::Alike(_))) && !by_number_or_airing(c)
        };
        let matches = |i: usize, c: &ChannelRow| {
            search.is_empty()
                || lineup.names[i].contains(&search)
                || found.contains_key(&c.id)
                || by_number_or_airing(c)
        };
        let mut listed: Vec<&ChannelRow> = match category {
            Category::Recent => {
                let by_id: HashMap<i64, usize> = lineup.rows.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
                self.recent(user, RECENT)
                    .await?
                    .into_iter()
                    .filter_map(|id| by_id.get(&id).copied())
                    .filter(|&i| lineup.shown[i] && matches(i, &lineup.rows[i]))
                    .map(|i| &lineup.rows[i])
                    .collect()
            }
            _ => {
                let favorites =
                    if *category == Category::Favorites { self.favorites(user.id).await? } else { HashSet::new() };
                lineup
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| lineup.shown[i])
                    .filter(|(_, c)| match category {
                        Category::Favorites => favorites.contains(&c.id),
                        Category::Group(name) => c.group_name == *name,
                        _ => true,
                    })
                    .filter(|&(i, c)| matches(i, c))
                    .map(|(_, c)| c)
                    .collect()
            }
        };
        // Names only alike to the search (nothing was found by its words)
        // give way to channels found by number or by what's on.
        if listed.iter().any(|c| !only_alike(c)) {
            listed.retain(|c| !only_alike(c));
        }
        // Found ones come closest first, the lineup's order among equals:
        // by name as typed, then by number or what's on, then by words read
        // as others, then alike; recent ones stay in the order they were watched.
        if !search.is_empty() && *category != Category::Recent {
            use crate::search::Found;
            listed.sort_by_cached_key(|c| {
                let how = match found.get(&c.id) {
                    Some(Found::AsTyped) => (0, std::cmp::Reverse(0)),
                    None => (1, std::cmp::Reverse(0)),
                    Some(Found::Corrected(close)) => (2, *close),
                    Some(Found::Alike(close)) => (3, *close),
                };
                (channel_rank(&search, c.display_name(), c.display_number().as_deref()), how)
            });
        }
        let limit = limit.clamp(1, MAX_PAGE);
        let offset = match around.and_then(|id| listed.iter().position(|c| c.id == id)) {
            Some(at) => at / limit * limit,
            None => offset.min(listed.len()),
        };
        let page = &listed[offset..(offset + limit).min(listed.len())];
        Ok(ChannelPage { total: listed.len(), offset, channels: self.channels_out(user, page).await? })
    }

    /// The guide channels (by source) airing a programme whose title has `search` in it.
    /// Titles are compared as search compares names ([`crate::search::text::finds`]):
    /// "lakeside games" finds "Winter Games - Lakeside 2020".
    /// The titles are read and folded once a minute at most, not with every
    /// search: a guide has thousands of channels, and search asks as people type.
    async fn airing_titled(&self, search: &str) -> Result<HashSet<(i64, String)>, AppError> {
        let at = now();
        let generation = self.generation.load(Ordering::SeqCst);
        let kept = self.airing.lock().clone().filter(|a| a.generation == generation && at - a.read_at < 60);
        let airing = match kept {
            Some(airing) => airing,
            None => {
                let rows = sqlx::query_as::<_, (i64, String, i64, i64, String)>(
                    "SELECT source_id, guide_id, start, stop, title FROM live_programs WHERE start <= ? AND stop > ?",
                )
                .bind(at + 60)
                .bind(at)
                .fetch_all(&self.db.read)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
                let programs = tokio::task::spawn_blocking(move || {
                    rows.into_iter()
                        .map(|(source_id, guide_id, start, stop, title)| AiringProgram {
                            source_id,
                            guide_id,
                            start,
                            stop,
                            title: crate::search::text::fold(&title).into_boxed_str(),
                        })
                        .collect()
                })
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("reading what's on: {e}")))?;
                let airing = Arc::new(Airing { read_at: at, generation, programs });
                *self.airing.lock() = Some(airing.clone());
                airing
            }
        };
        let search = crate::search::text::Finder::new(search);
        Ok(airing
            .programs
            .iter()
            .filter(|p| p.start <= at && p.stop > at && search.finds_folded(&p.title))
            .map(|p| (p.source_id, p.guide_id.clone()))
            .collect())
    }

    /// The channels `user` watched last, newest first.
    pub async fn recent(&self, user: &AuthUser, limit: i64) -> Result<Vec<i64>, AppError> {
        sqlx::query_scalar(
            "SELECT w.channel_id FROM live_watched w JOIN live_channels c ON c.id = w.channel_id \
             WHERE w.user_id = ? AND c.hidden = 0 AND c.missing_since IS NULL ORDER BY w.watched_at DESC LIMIT ?",
        )
        .bind(user.id)
        .bind(limit)
        .fetch_all(&self.db.read)
        .await
        .map_err(|e| AppError::Internal(e.into()))
    }

    pub(crate) async fn watched(&self, user_id: i64, channel_id: i64) {
        let _ = sqlx::query(
            "INSERT INTO live_watched (user_id, channel_id, watched_at) VALUES (?, ?, ?) \
             ON CONFLICT(user_id, channel_id) DO UPDATE SET watched_at = excluded.watched_at",
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(now())
        .execute(&self.db.write)
        .await;
    }

    pub async fn set_favorite(&self, user: &AuthUser, id: i64, favorite: bool) -> Result<(), AppError> {
        self.channel_row(id).await?;
        let query = if favorite {
            sqlx::query("INSERT OR IGNORE INTO live_favorites (user_id, channel_id, added_at) VALUES (?, ?, ?)")
                .bind(user.id)
                .bind(id)
                .bind(now())
        } else {
            sqlx::query("DELETE FROM live_favorites WHERE user_id = ? AND channel_id = ?").bind(user.id).bind(id)
        };
        query.execute(&self.db.write).await.map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }

    /// The guide from `from` to `to` (at most two days) for channels `ids`
    /// (every channel `user` sees when `None`).
    pub async fn guide(
        &self,
        _user: &AuthUser,
        from: i64,
        to: i64,
        ids: Option<&HashSet<i64>>,
    ) -> Result<Guide, AppError> {
        if to <= from || to - from > 2 * 86_400 {
            return Err(AppError::bad_request("the guide is asked for two days at most, `to` after `from`"));
        }
        let lineup = self.lineup().await?;
        let rows: Vec<&ChannelRow> = lineup.shown().filter(|c| ids.is_none_or(|ids| ids.contains(&c.id))).collect();
        let programs = self.programs_of(&rows, from, to).await?;
        Ok(Guide { from, to, programs })
    }

    pub async fn admin_channels(&self, source: Option<i64>) -> Result<Vec<AdminChannel>, AppError> {
        let lineup = self.lineup().await?;
        let rows = lineup.rows.iter().filter(|c| source.is_none_or(|s| c.source_id == s));
        Ok(rows.map(|c| Self::admin_channel(&lineup, c)).collect())
    }

    /// The lineup's channels `filter` keeps (hidden ones too), by index.
    fn admin_matching<'a>(lineup: &'a Lineup, filter: &LineupFilter) -> impl Iterator<Item = &'a ChannelRow> {
        let search = filter.q.as_deref().unwrap_or("").trim().to_lowercase();
        lineup.rows.iter().enumerate().filter_map(move |(i, c)| {
            let keep = filter.source_id.is_none_or(|s| c.source_id == s)
                && filter.group.as_ref().is_none_or(|g| c.group_name == *g)
                && match filter.state {
                    LineupState::All => true,
                    LineupState::Shown => !c.hidden,
                    LineupState::Hidden => c.hidden,
                    LineupState::NoGuide => !lineup.has_guide(c),
                    LineupState::Offline => c.offline(),
                }
                && (search.is_empty()
                    || lineup.names[i].contains(&search)
                    || c.name.to_lowercase().contains(&search)
                    || c.display_number().is_some_and(|n| n.starts_with(&search)));
            keep.then_some(c)
        })
    }

    /// A page of the lineup as administrators see it, `filter` applied.
    pub async fn admin_page(&self, filter: &LineupFilter, offset: usize, limit: usize) -> Result<AdminPage, AppError> {
        let lineup = self.lineup().await?;
        let matching: Vec<&ChannelRow> = Self::admin_matching(&lineup, filter).collect();
        let offset = offset.min(matching.len());
        let end = (offset + limit.clamp(1, MAX_PAGE)).min(matching.len());
        Ok(AdminPage {
            total: matching.len(),
            hidden: matching.iter().filter(|c| c.hidden).count(),
            offset,
            channels: matching[offset..end].iter().map(|c| Self::admin_channel(&lineup, c)).collect(),
        })
    }

    /// The ids of every channel `filter` keeps.
    pub async fn admin_ids(&self, filter: &LineupFilter) -> Result<Vec<i64>, AppError> {
        Ok(Self::admin_matching(&*self.lineup().await?, filter).map(|c| c.id).collect())
    }

    fn admin_channel(lineup: &Lineup, c: &ChannelRow) -> AdminChannel {
        AdminChannel {
            id: c.id,
            source_id: c.source_id,
            name: c.display_name().to_owned(),
            number: c.display_number(),
            group: c.group_name.clone(),
            logo: logo_path(c),
            guide_id: c.guide_key().unwrap_or_default().to_owned(),
            has_guide: lineup.has_guide(c),
            hidden: c.hidden,
            source_name: c.name.clone(),
            source_number: c.number.clone(),
            source_guide_id: c.guide_id.clone(),
            matched_guide_id: c.matched_guide_id.clone(),
            custom_name: c.custom_name.clone(),
            custom_number: c.custom_number.clone(),
            custom_guide_id: c.custom_guide_id.clone(),
            offline: c.offline(),
            offline_since: c.offline_since,
        }
    }

    /// Every group, hidden channels' too, with how many of each are hidden.
    pub async fn admin_groups(&self) -> Result<Vec<AdminGroup>, AppError> {
        let lineup = self.lineup().await?;
        let mut groups: Vec<AdminGroup> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        for c in &lineup.rows {
            let at = *index.entry(&c.group_name).or_insert_with(|| {
                groups.push(AdminGroup { group: Group::new(&c.group_name), hidden: 0, offline: 0 });
                groups.len() - 1
            });
            groups[at].group.count += 1;
            groups[at].hidden += usize::from(c.hidden);
            groups[at].offline += usize::from(c.offline());
        }
        Ok(groups)
    }

    /// The channels in groups `names`, hidden ones too.
    pub async fn group_channels(&self, names: &[String]) -> Result<Vec<i64>, AppError> {
        let names: HashSet<&str> = names.iter().map(String::as_str).collect();
        Ok(self.lineup().await?.rows.iter().filter(|c| names.contains(c.group_name.as_str())).map(|c| c.id).collect())
    }

    /// Changes channels `ids` as `patch` says.
    pub async fn patch_channels(self: &Arc<Self>, ids: &[i64], patch: &ChannelPatch) -> Result<(), AppError> {
        if ids.is_empty() {
            return Ok(());
        }
        let clean = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_owned()).map(|s| (!s.is_empty()).then_some(s));
        let (name, number, guide) = (clean(&patch.name), clean(&patch.number), clean(&patch.guide_id));
        let mut sets = Vec::new();
        if patch.hidden.is_some() {
            sets.push("hidden = ?");
        }
        if name.is_some() {
            sets.push("custom_name = ?");
        }
        if number.is_some() {
            sets.push("custom_number = ?");
        }
        if guide.is_some() {
            sets.push("custom_guide_id = ?");
        }
        if sets.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.write.begin().await.map_err(|e| AppError::Internal(e.into()))?;
        // Whole groups run to thousands of channels: hundreds per statement.
        for batch in ids.chunks(500) {
            let sql = format!(
                "UPDATE live_channels SET {} WHERE id IN ({})",
                sets.join(", "),
                vec!["?"; batch.len()].join(", ")
            );
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            if let Some(hidden) = patch.hidden {
                q = q.bind(hidden);
            }
            for value in [&name, &number, &guide].into_iter().flatten() {
                q = q.bind(value.clone());
            }
            for id in batch {
                q = q.bind(id);
            }
            q.execute(&mut *tx).await.map_err(|e| AppError::Internal(e.into()))?;
        }
        tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;
        if patch.hidden == Some(true) {
            for id in ids {
                self.tuners.stop_channel(*id).await;
            }
        }
        self.changed();
        // A guide channel, or a name the guide may know, that the last read
        // left out: in the background, as the lineup is read again for it.
        if guide.is_some() || name.is_some() {
            let (this, ids, guide_set, renamed) = (self.clone(), ids.to_vec(), guide.is_some(), name.is_some());
            tokio::spawn(async move {
                if let Err(err) = this.read_guides_wanted(&ids, guide_set, renamed).await {
                    tracing::warn!("reading the guides of channels set again failed: {err}");
                }
            });
        }
        Ok(())
    }

    // ----------------------------------------------------------- pictures

    /// A channel's logo, downloaded the first time (and again once a week).
    pub async fn logo(&self, id: i64) -> Result<PathBuf, AppError> {
        let row = self.channel_row(id).await?;
        if row.logo_url.is_empty() {
            return Err(AppError::not_found("the channel has no logo"));
        }
        self.picture(&row.logo_url, true).await
    }

    /// A programme's picture, downloaded the first time (and again once a week).
    pub async fn program_image(&self, channel: i64, start: i64) -> Result<PathBuf, AppError> {
        let row = self.channel_row(channel).await?;
        let key = row.guide_key().ok_or_else(|| AppError::not_found("the channel has no guide"))?;
        let url: Option<String> =
            sqlx::query_scalar("SELECT image FROM live_programs WHERE source_id = ? AND guide_id = ? AND start = ?")
                .bind(row.source_id)
                .bind(key)
                .bind(start)
                .fetch_optional(&self.db.read)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
                .flatten();
        self.picture(&url.ok_or_else(|| AppError::not_found("the programme has no picture"))?, false).await
    }

    /// The picture at `url`, kept on this server: downloaded the first time
    /// and once [`PICTURE_FRESH`] old (the one kept still served when that
    /// fails). A `logo` is made small enough ([`LOGO_SIDE`]), and light
    /// where it was drawn dark on nothing ([`lightened`]).
    async fn picture(&self, url: &str, logo: bool) -> Result<PathBuf, AppError> {
        // Logos are kept apart from those kept before they were made fit.
        let key = if logo { format!("logo\n{url}") } else { url.to_owned() };
        let hash = hex::encode(&Sha256::digest(key.as_bytes())[..16]);
        let dir = self.pictures.join(&hash[..2]);
        let kept = PICTURE_EXTS.iter().map(|ext| dir.join(format!("{hash}.{ext}"))).find(|p| p.is_file());
        let fresh = |p: &PathBuf| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t.elapsed().is_ok_and(|age| age < PICTURE_FRESH))
        };
        if let Some(path) = &kept
            && fresh(path)
        {
            return Ok(path.clone());
        }
        if self.failed_pictures.lock().get(url).is_some_and(|t| t.elapsed() < PICTURE_RETRY) {
            return kept.ok_or_else(|| AppError::not_found("the picture can't be downloaded"));
        }
        let fetched = async {
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                bail!("not a web address");
            }
            let bytes = self.download(url, "", if logo { MAX_LOGO_BYTES } else { MAX_PICTURE_BYTES }).await?;
            // SVG isn't served: a picture opened from this server's address
            // must not run scripts there.
            let mut ext = crate::store::picture_ext(&bytes).context("not a picture")?;
            let mut bytes = bytes;
            if logo {
                // One that can't be read here is kept as it came: the apps may.
                let given = bytes.clone();
                if let Ok(Some(png)) = crate::util::blocking(move || made_fit(&given)).await {
                    (bytes, ext) = (png, "png");
                }
            }
            tokio::fs::create_dir_all(&dir).await?;
            let path = dir.join(format!("{hash}.{ext}"));
            let tmp = dir.join(format!(".{hash}.tmp"));
            tokio::fs::write(&tmp, &bytes).await?;
            tokio::fs::rename(&tmp, &path).await?;
            // One kept before in another format goes.
            if let Some(old) = kept.as_ref().filter(|old| **old != path) {
                let _ = tokio::fs::remove_file(old).await;
            }
            Ok::<_, anyhow::Error>(path)
        };
        let failed = match tokio::time::timeout(Duration::from_secs(20), fetched).await {
            Ok(Ok(path)) => return Ok(path),
            Ok(Err(err)) => {
                tracing::debug!(url, "live TV picture not downloaded: {err:#}");
                "the picture can't be downloaded"
            }
            Err(_) => "the picture took too long to download",
        };
        self.failed_pictures.lock().insert(url.to_owned(), Instant::now());
        kept.ok_or_else(|| AppError::not_found(failed))
    }
}

/// What a channel's logo is made into: changed, the apps' copies are
/// asked for again (their addresses change).
const LOGO_MAKING: &str = "2";

/// The address of a channel's logo, if it has one: `?v=` changes with its
/// provider's address and [`LOGO_MAKING`], so apps never keep an old one.
fn logo_path(c: &ChannelRow) -> Option<String> {
    if c.logo_url.is_empty() {
        return None;
    }
    let version = Sha256::digest(format!("{LOGO_MAKING}\n{}", c.logo_url).as_bytes());
    Some(format!("/api/live/channels/{}/logo?v={}", c.id, hex::encode(&version[..4])))
}

/// The formats a picture is kept in.
const PICTURE_EXTS: [&str; 4] = ["png", "jpg", "webp", "gif"];

/// A logo as it's kept, when it isn't as it came: no larger than
/// [`LOGO_SIDE`] either way, and [`lightened`] where it was drawn dark.
fn made_fit(bytes: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
    let img = crate::images::decode(bytes)?;
    let big = img.width() > LOGO_SIDE || img.height() > LOGO_SIDE;
    let img = if big { img.resize(LOGO_SIDE, LOGO_SIDE, image::imageops::FilterType::Triangle) } else { img };
    let img = match lightened(&img) {
        Some(light) => image::DynamicImage::ImageRgba8(light),
        None if big => img,
        None => return Ok(None),
    };
    let mut png = Vec::new();
    if img.color().has_alpha() {
        img.to_rgba8().write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    } else {
        img.to_rgb8().write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    }
    Ok(Some(png))
}

/// A logo drawn dark on nothing (black lettering, a navy badge) is lost on
/// the apps' dark screens: its lightness is turned around, black becoming
/// white and navy light blue, colours keeping their hue. `None` for any
/// other logo: one on a background of its own already shows.
fn lightened(img: &image::DynamicImage) -> Option<image::RgbaImage> {
    if !img.color().has_alpha() {
        return None;
    }
    let mut rgba = img.to_rgba8();
    // Judged on a small copy: as cheap whatever the logo's size.
    let sample = image::imageops::thumbnail(&rgba, 64, 64);
    let (mut clear, mut solid, mut dark) = (0usize, 0usize, 0usize);
    for p in sample.pixels() {
        match p[3] {
            0..32 => clear += 1,
            32..160 => {}
            _ => {
                solid += 1;
                // Dim and with no bright channel: a strong red isn't dark on black.
                let luma = 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]);
                if luma < 0.3 * 255.0 && p[0].max(p[1]).max(p[2]) < 140 {
                    dark += 1;
                }
            }
        }
    }
    let total = sample.pixels().len();
    // A tenth of it see-through, and most of what shows dark.
    if clear * 10 < total || solid == 0 || dark * 10 < solid * 7 {
        return None;
    }
    for p in rgba.pixels_mut() {
        // Lightness turned around with hue and saturation kept: each channel
        // moves by 255 − max − min (HSL keeps its chroma).
        let (max, min) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
        let shift = 255 - i16::from(max) - i16::from(min);
        for c in &mut p.0[..3] {
            *c = (i16::from(*c) + shift).clamp(0, 255) as u8;
        }
    }
    Some(rgba)
}

struct Listed {
    entries: Vec<m3u::Entry>,
    guide_url: Option<String>,
    account: Option<xtream::Account>,
}

/// Channels in their order: by number where they have one (numerically),
/// then as their sources list them.
fn sort_channels(rows: &mut [ChannelRow]) {
    let key = |c: &ChannelRow| {
        let number = c.display_number().and_then(|n| n.trim().parse::<f64>().ok());
        (number.is_none(), number.unwrap_or(0.0), c.source_id, c.position)
    };
    rows.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
}

/// How closely a channel named `name` answers a search for `search`, best
/// first: its number in full, then by name as titles are
/// ([`crate::search::match_rank`], a region before the name not counting),
/// then those found by their number's beginning or by what's on.
fn channel_rank(search: &str, name: &str, number: Option<&str>) -> u8 {
    if number.is_some_and(|n| n.trim().eq_ignore_ascii_case(search)) {
        return 0;
    }
    if !name.to_lowercase().contains(search) {
        return 4;
    }
    let rank = crate::search::match_rank(search, name);
    split_region(name).map_or(rank, |(_, rest)| rank.min(crate::search::match_rank(search, rest)))
}

/// A group name's region and the rest, when it begins with one the way
/// providers write them: "UK| Sport", "|UK| Sport", "UK: Sport", "[UK] Sport".
fn split_region(name: &str) -> Option<(&str, &str)> {
    let name = name.trim().trim_start_matches('|').trim_start();
    let (region, rest) = if let Some(inner) = name.strip_prefix('[') {
        inner.split_once(']')?
    } else if let Some(split) = name.split_once('|') {
        split
    } else {
        // A colon only after a short code ("UK:", not "Live: Sport").
        name.split_once(':').filter(|(r, _)| r.len() <= 4 && r.chars().all(|c| c.is_ascii_uppercase()))?
    };
    let (region, rest) = (region.trim(), rest.trim().trim_start_matches(['|', ':', '-']).trim());
    (!region.is_empty() && region.chars().count() <= 16 && !rest.is_empty()).then_some((region, rest))
}

/// A short id for a new source: six hex digits, like those given to the
/// sources there were before ids.
fn new_source_uid() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..6].to_owned()
}

/// A provider's divider row rather than a channel: a name between runs of
/// marks ("##### UK SPORTS #####", "---- NEWS ----", "★★★ VIP ★★★").
fn is_divider(name: &str) -> bool {
    let name = name.trim();
    name.chars().take_while(|c| "#=*-~_+•●★☆✦━─|".contains(*c)).count() >= 3 || !name.chars().any(char::is_alphanumeric)
}

/// A guide's channels, as a source's channels are looked up in them.
struct GuideIndex<'a> {
    ids: HashSet<&'a str>,
    /// By [`loose_id`].
    loose: HashMap<String, &'a str>,
    /// By [`normalize`]d name.
    by_name: HashMap<String, &'a str>,
}

impl<'a> GuideIndex<'a> {
    fn new(channels: &'a [xmltv::GuideChannel]) -> Self {
        let mut index = GuideIndex { ids: HashSet::new(), loose: HashMap::new(), by_name: HashMap::new() };
        let mut ambiguous = HashSet::new();
        for c in channels {
            index.ids.insert(&c.id);
            let loose = loose_id(&c.id);
            if !loose.is_empty() {
                match index.loose.get(&loose) {
                    Some(id) if *id != c.id => {
                        ambiguous.insert(loose);
                    }
                    Some(_) => {}
                    None => {
                        index.loose.insert(loose, &c.id);
                    }
                }
            }
            for name in &c.names {
                index.by_name.entry(normalize(name)).or_insert(&c.id);
            }
        }
        // Two guide channels written alike would be a guess between them.
        for loose in ambiguous {
            index.loose.remove(&loose);
        }
        index
    }

    /// The guide's channel for one whose source gives it `id` and `name`,
    /// when the guide doesn't have `id` as it is: `id` written otherwise
    /// (`Kika.de` for `KIKA.de`, `ARD-alpha.de` for `ARD.alpha.de`), else the
    /// channel of its name.
    fn matching(&self, id: &str, name: &str) -> Option<&'a str> {
        if self.ids.contains(id) {
            return None;
        }
        let loose = loose_id(id);
        let by_id = if loose.is_empty() { None } else { self.loose.get(&loose) };
        by_id.or_else(|| self.by_name.get(&normalize(name))).copied()
    }
}

/// A guide id as compared when it isn't found as it is: lower case, letters
/// and digits only. Sources and guides often write the same id in another
/// case or with other dots and dashes. Between two digits a separator stays,
/// as one dot: `21.1` and `2.11` are different channels.
fn loose_id(id: &str) -> String {
    let mut loose = String::with_capacity(id.len());
    let mut chars = id.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() {
            loose.extend(c.to_lowercase());
        } else if loose.ends_with(|p: char| p.is_ascii_digit()) && chars.peek().is_some_and(|n| n.is_ascii_digit()) {
            loose.push('.');
        }
    }
    loose
}

/// A channel name as compared with the guide's: lower case, letters and
/// digits only, and a trailing quality said or not (`HD`, `FHD`, `4K`).
fn normalize(name: &str) -> String {
    let mut s: String = name.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
    for suffix in ["uhd", "fhd", "hd", "sd", "4k"] {
        if s.len() > suffix.len() + 1 && s.ends_with(suffix) {
            s.truncate(s.len() - suffix.len());
            break;
        }
    }
    s
}

/// The file a guide's address names, when it names one (`/…`, `file://…`)
/// rather than a web address: checked to be on this computer
/// ([`local_guide`]).
fn guide_file(url: &str) -> Option<anyhow::Result<PathBuf>> {
    url.strip_prefix("file://").or_else(|| url.starts_with('/').then_some(url)).map(local_guide)
}

/// The file on this computer `local` names: never one on another computer
/// (`//host/share/…`, `\\host\share\…`), which Windows signs in to, as the
/// account Pley runs as, wherever it is, nor a device (`\\.\…`).
fn local_guide(local: &str) -> anyhow::Result<PathBuf> {
    use std::path::{Component, Prefix};
    let path = PathBuf::from(local);
    // On Windows, `/…` is on the drive Pley runs from.
    anyhow::ensure!(path.has_root(), "the guide's file is named from the top of its drive (/…)");
    // Windows takes any two separators at the start as a share's, `/\host`
    // too, though Rust sees no prefix there.
    let separator = |c: char| c == '/' || c == '\\';
    let mut start = local.chars();
    let share = start.next().is_some_and(separator) && start.next().is_some_and(separator);
    let mut components = path.components();
    let here = !share
        && match components.next() {
            Some(Component::Prefix(prefix)) => matches!(prefix.kind(), Prefix::Disk(_)),
            // `\??\…` is a path Windows passes on as it is, a share's
            // (`\??\UNC\host\share\…`) too.
            _ => components.next() != Some(Component::Normal("??".as_ref())),
        };
    anyhow::ensure!(here, "a guide on another computer is read from its web address (http:// or https://)");
    Ok(path)
}

/// `text` with `secret` (a password in a URL) blanked out.
fn redact(text: &str, secret: &str) -> String {
    if secret.len() < 2 { text.to_owned() } else { text.replace(secret, "••••") }
}

/// What went wrong with a request, in words.
fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "the provider didn't answer in time".into()
    } else if e.is_connect() {
        format!("the provider can't be reached ({})", e.url().and_then(|u| u.host_str()).unwrap_or("?"))
    } else {
        // Without the URL, which may carry a password.
        let text = e.to_string();
        match e.url() {
            Some(url) => text.replace(&format!(" ({url})"), "").replace(url.as_str(), "…"),
            None => text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_missing_from_the_guide_is_matched_by_its_id_written_otherwise_then_its_name() {
        let channel =
            |id: &str, name: &str| xmltv::GuideChannel { id: id.into(), names: vec![name.into()], icon: None };
        let channels =
            [channel("KIKA.de", "KiKA"), channel("ARD.alpha.de", "ARD alpha"), channel("WDR.de", "WDR Fernsehen")];
        let index = GuideIndex::new(&channels);
        // The guide has the id: nothing to match.
        assert_eq!(index.matching("KIKA.de", "Anything"), None);
        assert_eq!(index.matching("Kika.de", "Anything"), Some("KIKA.de"));
        assert_eq!(index.matching("ARD-alpha.de", "Anything"), Some("ARD.alpha.de"));
        // An id the guide has in no form: the name decides.
        assert_eq!(index.matching("WDRFernsehen.de", "WDR Fernsehen HD"), Some("WDR.de"));
        assert_eq!(index.matching("", "KiKA HD"), Some("KIKA.de"));
        assert_eq!(index.matching("-", "Nothing like it"), None);

        // Digits keep their separator, and ids two channels share in their
        // loose form match neither.
        let channels =
            [channel("2.11", "Two"), channel("SPORT.de", "S1"), channel("Sport.de", "S2"), channel("5-1", "Five")];
        let index = GuideIndex::new(&channels);
        assert_eq!(index.matching("21.1", "Anything"), None);
        assert_eq!(index.matching("2-11", "Anything"), Some("2.11"));
        assert_eq!(index.matching("5.1", "Anything"), Some("5-1"));
        assert_eq!(index.matching("sport.de", "Anything"), None);
    }

    #[test]
    fn a_match_comes_before_an_id_the_guide_lacks() {
        let row = |custom: Option<&str>, matched: Option<&str>| ChannelRow {
            id: 1,
            source_id: 1,
            name: "KiKA".into(),
            number: None,
            group_name: String::new(),
            logo_url: String::new(),
            guide_id: "Kika.de".into(),
            matched_guide_id: matched.map(Into::into),
            stream_url: String::new(),
            user_agent: None,
            position: 0,
            hidden: false,
            custom_name: None,
            custom_number: None,
            custom_guide_id: custom.map(Into::into),
            offline_since: None,
        };
        assert_eq!(row(None, None).guide_key(), Some("Kika.de"));
        assert_eq!(row(None, Some("KIKA.de")).guide_key(), Some("KIKA.de"));
        assert_eq!(row(Some("kika.example"), Some("KIKA.de")).guide_key(), Some("kika.example"));
        assert_eq!(row(Some(""), Some("KIKA.de")).guide_key(), Some("KIKA.de"));
    }

    #[test]
    fn channels_found_come_closest_first() {
        assert_eq!(channel_rank("101", "Anything", Some("101")), 0);
        assert_eq!(channel_rank("news", "News", Some("7")), 0);
        // A region before the name doesn't count.
        assert_eq!(channel_rank("news", "UK| News", None), 0);
        assert_eq!(channel_rank("news", "UK| News HD", None), 1);
        assert_eq!(channel_rank("news", "World News", None), 2);
        assert_eq!(channel_rank("news", "Newsroom", None), 1);
        assert_eq!(channel_rank("ews", "News", None), 3);
        // Found by its number's beginning or by what's on.
        assert_eq!(channel_rank("10", "Sport", Some("101")), 4);
        assert_eq!(channel_rank("quiz", "Sport", None), 4);
    }

    #[test]
    fn a_local_guide_is_a_file_on_this_computer() {
        for elsewhere in [
            "//host/share/guide.xml",
            r"\\host\share\guide.xml",
            r"\\?\UNC\host\share\guide.xml",
            r"\??\UNC\host\share\guide.xml",
            r"/\host\share\guide.xml",
            r"/??/UNC/host/share/guide.xml",
        ] {
            assert!(local_guide(elsewhere).is_err(), "{elsewhere}");
        }
        assert!(local_guide("guide.xml").is_err(), "nowhere in particular");
        let here = if cfg!(windows) { "C:/guides/guide.xml" } else { "/srv/guides/guide.xml" };
        assert_eq!(local_guide(here).unwrap(), PathBuf::from(here));
        // On Windows, on Pley's own drive.
        assert_eq!(local_guide("/guides/guide.xml").unwrap(), PathBuf::from("/guides/guide.xml"));
        assert!(guide_file("https://p.example/guide.xml").is_none(), "a web address");
        assert_eq!(guide_file("file:///guides/guide.xml").unwrap().unwrap(), PathBuf::from("/guides/guide.xml"));
    }

    /// A source isn't saved with a guide file its refresh would refuse.
    #[test]
    fn a_source_is_saved_only_with_a_guide_it_can_read() {
        let input = |guide: &str| -> SourceInput {
            let source =
                serde_json::json!({"name": "P", "kind": "m3u", "url": "http://p.example/list.m3u", "guide_url": guide});
            serde_json::from_value(source).unwrap()
        };
        for refused in
            ["//host/share/guide.xml", r"\\host\share\guide.xml", "file://host/share/guide.xml", "file://guide.xml"]
        {
            assert!(LiveTv::check_input(&input(refused)).is_err(), "{refused}");
        }
        for taken in ["", "https://p.example/guide.xml.gz", "/guides/guide.xml", "file:///guides/guide.xml"] {
            assert!(LiveTv::check_input(&input(taken)).is_ok(), "{taken}");
        }
    }

    /// A 100×100 logo: `ink` drawn in its middle, on nothing or on `ground`.
    fn logo(ink: [u8; 3], ground: Option<[u8; 3]>) -> image::DynamicImage {
        let mut img = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 0]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            if (25..75).contains(&x) && (25..75).contains(&y) {
                *p = image::Rgba([ink[0], ink[1], ink[2], 255]);
            } else if let Some(g) = ground {
                *p = image::Rgba([g[0], g[1], g[2], 255]);
            }
        }
        image::DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn dark_logos_on_nothing_are_made_light() {
        let light = lightened(&logo([0, 0, 0], None)).unwrap();
        assert_eq!(light[(50, 50)].0, [255, 255, 255, 255], "black is white");
        assert_eq!(light[(5, 5)][3], 0, "still see-through around it");
        let navy = lightened(&logo([0, 0, 128], None)).unwrap();
        assert_eq!(navy[(50, 50)].0, [127, 127, 255, 255], "navy is light blue: its hue kept");

        assert!(lightened(&logo([230, 230, 230], None)).is_none(), "light already");
        assert!(lightened(&logo([200, 30, 30], None)).is_none(), "colourful");
        assert!(lightened(&logo([0, 0, 0], Some([255, 255, 255]))).is_none(), "on its own background");
        let opaque = image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10));
        assert!(lightened(&opaque).is_none(), "nothing see-through: a background of its own");
    }

    #[test]
    fn logos_are_kept_small_and_as_they_came_when_they_show() {
        let encode = |img: &image::DynamicImage| {
            let mut out = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
            out
        };
        assert!(made_fit(&encode(&logo([200, 30, 30], None))).unwrap().is_none(), "kept as it came");
        let dark = image::load_from_memory(&made_fit(&encode(&logo([0, 0, 0], None))).unwrap().unwrap()).unwrap();
        assert_eq!(dark.to_rgba8()[(50, 50)].0, [255, 255, 255, 255]);
        let huge =
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(2000, 1000, image::Rgba([200, 30, 30, 255])));
        let small = image::load_from_memory(&made_fit(&encode(&huge)).unwrap().unwrap()).unwrap();
        assert_eq!((small.width(), small.height()), (LOGO_SIDE, LOGO_SIDE / 2));
        assert!(made_fit(b"<svg/>").is_err(), "not a picture");
    }

    #[test]
    fn names_match_the_guide_whatever_their_quality() {
        assert_eq!(normalize("BBC One HD"), normalize("bbc one"));
        assert_eq!(normalize("Sky Sports F1 FHD"), "skysportsf1");
        assert_eq!(normalize("HD"), "hd");
        assert_eq!(redact("http://x/live/me/secret/1.ts", "secret"), "http://x/live/me/••••/1.ts");
    }

    #[test]
    fn groups_are_told_by_their_region() {
        assert_eq!(split_region("UK| SPORTS"), Some(("UK", "SPORTS")));
        assert_eq!(split_region("|AR| MBC"), Some(("AR", "MBC")));
        assert_eq!(split_region("GR| ΓΕΝΙΚΆ/GENERAL"), Some(("GR", "ΓΕΝΙΚΆ/GENERAL")));
        assert_eq!(split_region("[DE] Sport"), Some(("DE", "Sport")));
        assert_eq!(split_region("FR: Cinéma"), Some(("FR", "Cinéma")));
        assert_eq!(split_region("Live: Sport"), None);
        assert_eq!(split_region("24/7 SHOWS VIP"), None);
        assert_eq!(split_region("UK|"), None);
        assert_eq!(Group::new("US| ESPN+ PPV").label, "ESPN+ PPV");
        assert_eq!(Group::new("News").region, None);
    }

    #[test]
    fn settings_are_checked() {
        assert!(LiveSettings::default().check().is_ok());
        assert!(LiveSettings { timeshift_minutes: 0, ..Default::default() }.check().is_err());
        assert!(LiveSettings { max_height: Some(100), ..Default::default() }.check().is_err());
    }
}
