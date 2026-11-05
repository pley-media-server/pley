//! The app's state: the port of `packages/tv/src/state.ts` with the Android app's "saved first,
//! fresh after".
//!
//! It holds the server in use and its sign-in, who is watching, the libraries, Live TV's summary,
//! Home, the household, the settings and whether the server is out of reach. At start it reads
//! what was saved for the server's last profile (no network), so the first frame draws Home and
//! the rail; after the first frame it asks the server again, and asks again whatever is older
//! than 30 s whenever the event stream (re)connects. The server's events drive refreshes
//! (1.5 s after the last library change, 0.3 s after the first watch-state change), held while a
//! playback is on top.
//!
//! Everything runs on the main thread: requests go to the task pool and come back as promises
//! collected by [`AppState::update`] each frame; timers are due times on the app's clock
//! ([`pley_base::time::now`]), reported by [`AppState::next_due`].

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pley_base::{Fetch, FetchError, Waker};
use pley_core::addresses::origin;
use pley_core::auth::{AuthError, StoredTokens, Tokens};
use pley_core::model::{AppProfile, Hub, Hubs, ItemDetail, Library, LiveCategory, LiveChannel, LiveSummary, Me};
use pley_core::model::{RequestList, RequestQuery, RequestSummary, ScanPhase, ServerEvent, TitleKind};
use pley_core::requests::title_key;
use pley_core::store::{SavedServer, Snapshots, keys, snapshot_keys};
use pley_core::{Api, ApiError, CapsOverrides, CapsReport, Device, EventStream, Http, Promise, Settings, Store};
use pley_core::{StreamNews, Tasks};

use crate::cx::BackgroundSession;
use crate::players::Players;
use crate::request_state::{RequestNotice, RequestState, SUMMARY_POLL};
use crate::route::Route;
use crate::screen::AppEvent;

/// An answer older than this is asked again when something shows it (`REVALIDATE_MS`,
/// Android's `ResourceCache.kt:279-282`).
pub const REVALIDATE: f64 = 30.0;
/// How long "Pley is offline" shows (`OFFLINE_NOTICE_MS`).
pub const OFFLINE_NOTICE: f64 = 6.0;
/// While the server is out of reach it is asked this often, and given as long to answer
/// (`OFFLINE_ASK_MS`): the event stream, backing off up to 30 s, then reconnects at once.
pub const OFFLINE_ASK: f64 = 5.0;
/// Library changes come in bursts during scans: Home and the pages ask again this long after the
/// last (Android's `ServerLiveUpdates.kt`).
pub const LIBRARY_GATHER: f64 = 1.5;
/// Watch-state changes come in bursts (an episode, its season and show): gathered this long
/// from the first (`STATE_GATHER_MS`).
pub const STATE_GATHER: f64 = 0.3;
/// How long an item's details are kept for pages and playbacks (`ITEM_TTL_MS`).
pub const ITEM_TTL: f64 = 300.0;
/// How many item details are kept.
pub const ITEMS_KEPT: usize = 40;
/// How many of the live channels pages showed are kept, so the player starts with one at once.
pub const LIVE_CHANNELS_KEPT: usize = 500;

/// What the shell's notice says while the server is out of reach, then the avatar's mark.
pub const OFFLINE_TEXT: &str = "Pley is offline";

/// A library scan the server's events told of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub phase: ScanPhase,
    pub processed: u64,
    pub total: u64,
}

/// The picture fetcher of whatever server is in use: `pley-gfx`'s cache is made once with it, and
/// pictures follow the sign-in as servers change.
#[derive(Default)]
pub struct CurrentFetch(Mutex<Option<Api>>);

impl CurrentFetch {
    fn set(&self, api: Option<Api>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = api;
    }
}

impl Fetch for CurrentFetch {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let api = self.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match api {
            Some(api) => api.fetch(url),
            None => Err(FetchError::Unreachable),
        }
    }
}

/// An answer on its way, with the generation (server, sign-in and profile) it was asked for:
/// one that comes back for a server or profile since left is dropped.
struct Pending<T> {
    generation: u64,
    promise: Promise<T>,
}

impl<T> Pending<T> {
    fn take(&mut self) -> Option<T> {
        self.promise.take()
    }
}

/// When each answer the state holds came, for asking again past [`REVALIDATE`].
#[derive(Clone, Copy, Debug, Default)]
struct Fetched {
    account: Option<f64>,
    home: Option<f64>,
    live: Option<f64>,
}

/// The app's state. One for the app, owned by the main loop; screens reach it through
/// [`crate::AppCx::state`].
pub struct AppState {
    device: Arc<dyn Device>,
    store: Store,
    snapshots: Arc<Snapshots>,
    http: Http,
    tasks: Tasks,
    waker: Waker,
    fetch: Arc<CurrentFetch>,
    settings: Settings,
    install_id: String,
    user_agent: String,
    now: f64,

    server: Option<String>,
    tokens: Option<Arc<Tokens>>,
    api: Option<Api>,
    /// The server whose sign-in its tokens ended (they tell from another thread).
    ended: Arc<Mutex<Option<String>>>,
    /// The TV holds tokens for the server (read once from them, then kept up to date here).
    kept: bool,
    signed_out: bool,
    profile_gate_passed: bool,
    /// Bumped whenever the server, sign-in or profile changes: answers asked before are dropped.
    generation: u64,
    /// The profile the saved answers shown are for.
    profile: Option<i64>,

    me: Option<Me>,
    libraries: Option<Vec<Library>>,
    live: Option<LiveSummary>,
    home: Option<Vec<Hub>>,
    profiles: Option<Vec<AppProfile>>,
    home_error: Option<ApiError>,
    account_error: Option<ApiError>,
    fetched: Fetched,
    scans: BTreeMap<i64, Scan>,
    caps: Option<CapsReport>,

    visible: bool,
    offline: bool,
    notice_until: Option<f64>,
    ask_at: Option<f64>,
    holds: u32,
    library_due: Option<f64>,
    state_due: Option<f64>,
    state_ids: HashSet<i64>,
    state_parents: HashSet<i64>,
    last_sync: f64,

    items: VecDeque<(ItemDetail, f64)>,
    live_known: VecDeque<LiveChannel>,
    /// The live channel watched last in this launch and its category: Live TV opens back at it.
    pub live_last_watched: Option<(i64, LiveCategory)>,

    /// The music playing while people browse; the music screens fill it.
    pub music: Option<Box<dyn BackgroundSession>>,
    /// The music played as the app was hidden: it plays on when the app shows again.
    music_hidden_playing: bool,

    account_me: Option<Pending<Result<Me, ApiError>>>,
    account_libraries: Option<Pending<Result<Vec<Library>, ApiError>>>,
    home_task: Option<Pending<Result<Hubs, ApiError>>>,
    live_task: Option<Pending<Result<LiveSummary, ApiError>>>,
    requests: RequestState,
    requests_task: Option<Pending<Result<RequestSummary, ApiError>>>,
    catch_up_task: Option<Pending<Result<RequestList, ApiError>>>,
    profiles_task: Option<Pending<Result<Vec<AppProfile>, ApiError>>>,
    reach_task: Option<Pending<Result<(), ApiError>>>,
    caps_task: Option<Promise<CapsReport>>,
    events: Option<(EventStream, Receiver<StreamNews>)>,
    out: Vec<AppEvent>,
    players: Players,
}

impl AppState {
    /// The state as the TV kept it: settings, the server in use and its sign-in, and the saved
    /// answers of the profile last used there. Reads a few small files; asks the server nothing.
    pub fn new(device: Arc<dyn Device>, store: Store, http: Http, tasks: Tasks) -> AppState {
        let snapshots = Arc::new(Snapshots::open(device.data_dir().join("snapshots")));
        let waker = tasks.waker().clone();
        let settings = Settings::load(&store);
        let install_id = store.install_id();
        let user_agent = http.user_agent().to_string();
        let players = Players::new(&store, &settings.quality);
        let mut state = AppState {
            device,
            store,
            snapshots,
            http,
            tasks,
            waker,
            fetch: Arc::new(CurrentFetch::default()),
            settings,
            install_id,
            user_agent,
            now: pley_base::time::now(),
            server: None,
            tokens: None,
            api: None,
            ended: Arc::new(Mutex::new(None)),
            kept: false,
            signed_out: false,
            profile_gate_passed: false,
            generation: 0,
            profile: None,
            me: None,
            libraries: None,
            live: None,
            home: None,
            profiles: None,
            home_error: None,
            account_error: None,
            fetched: Fetched::default(),
            scans: BTreeMap::new(),
            caps: None,
            visible: true,
            offline: false,
            notice_until: None,
            ask_at: None,
            holds: 0,
            library_due: None,
            state_due: None,
            state_ids: HashSet::new(),
            state_parents: HashSet::new(),
            last_sync: f64::NEG_INFINITY,
            items: VecDeque::new(),
            live_known: VecDeque::new(),
            live_last_watched: None,
            music: None,
            music_hidden_playing: false,
            account_me: None,
            account_libraries: None,
            home_task: None,
            live_task: None,
            requests: RequestState::default(),
            requests_task: None,
            catch_up_task: None,
            profiles_task: None,
            reach_task: None,
            caps_task: None,
            events: None,
            out: Vec::new(),
            players,
        };
        if let Some(server) = state.store.active_server() {
            state.use_server(&server);
        }
        state
    }

    // What the app runs on.

    /// The TV.
    pub fn device(&self) -> &Arc<dyn Device> {
        &self.device
    }

    /// What the TV keeps.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Saved answers per server and profile.
    pub fn snapshots(&self) -> &Arc<Snapshots> {
        &self.snapshots
    }

    /// The task pool.
    pub fn tasks(&self) -> &Tasks {
        &self.tasks
    }

    /// The HTTP client every request goes through (its `User-Agent` names the TV).
    pub fn http(&self) -> &Http {
        &self.http
    }

    /// The picture fetcher `pley-gfx`'s cache is made with: it follows the server in use.
    pub fn picture_fetch(&self) -> Arc<dyn Fetch> {
        self.fetch.clone()
    }

    /// This TV's id for playbacks and Live TV, kept for good.
    pub fn install_id(&self) -> &str {
        &self.install_id
    }

    /// The `User-Agent` requests carry.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// The time of the last [`AppState::update`].
    pub fn now(&self) -> f64 {
        self.now
    }

    /// This TV's settings.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Changes the settings and keeps them. Changing passthrough forgets the capability probe.
    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        let passthrough = self.settings.passthrough.clone();
        f(&mut self.settings);
        self.players.quality.set(&self.settings.quality);
        if self.settings.passthrough != passthrough {
            self.caps = None;
            self.caps_task = None;
        }
        if let Err(e) = self.settings.save(&self.store) {
            log::warn!("settings not saved: {e}");
        }
    }

    /// What the players share.
    pub(crate) fn players_mut(&mut self) -> &mut Players {
        &mut self.players
    }

    /// Waits up to `timeout` for the playback reports queued so far to go out: on leaving the
    /// app, so the server hears that a playback ended. Blocks.
    pub fn flush_reports(&self, timeout: Duration) -> bool {
        self.players.flush(timeout)
    }

    // The server and the sign-in.

    /// The server in use (its origin).
    pub fn server(&self) -> Option<&str> {
        self.server.as_deref()
    }

    /// The server's API as this sign-in.
    pub fn api(&self) -> Option<&Api> {
        self.api.as_ref()
    }

    /// The server's tokens.
    pub fn tokens(&self) -> Option<&Arc<Tokens>> {
        self.tokens.as_ref()
    }

    /// Whether the TV holds a sign-in for the server in use.
    pub fn signed_in(&self) -> bool {
        !self.signed_out && self.kept
    }

    /// Bumped whenever the server, sign-in or profile changes; answers asked under an older one
    /// belong to what was left.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The profile the app shows: who is watching, else the one last used on this server.
    pub fn profile(&self) -> Option<i64> {
        self.me.as_ref().map(|m| m.id).or(self.profile)
    }

    /// Runs `f` with the API on the task pool; without a server the promise holds
    /// [`ApiError::SignedOut`].
    pub fn spawn<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Api) -> Result<T, ApiError> + Send + 'static,
    ) -> Promise<Result<T, ApiError>> {
        match &self.api {
            Some(api) => {
                let api = api.clone();
                self.tasks.spawn(move || f(&api))
            }
            None => Promise::resolved(Err(ApiError::SignedOut)),
        }
    }

    /// The first page, from what the TV kept (the Android app's `StartGate`): no server → Servers;
    /// no sign-in → Link; a household of one, or "Who's watching?" answered in this launch → Home.
    /// Else "Always start as" goes on as the profile the TV was left on, but never past its PIN:
    /// its pad comes first when it has one, and "Who's watching?" while the TV doesn't know
    /// whether it has one (or without "Always start as").
    pub fn start_route(&self) -> Route {
        let Some(server) = self.server.clone() else { return Route::Servers };
        if !self.signed_in() {
            return Route::Link { server };
        }
        if self.household_size() <= 1 || self.profile_gate_passed {
            return Route::Home;
        }
        match (self.always_start_as(), &self.me) {
            (Some(_), Some(me)) if me.has_pin => Route::Pin { profile: me.id },
            (Some(_), Some(_)) => Route::Home,
            _ => Route::Profiles,
        }
    }

    /// Uses `url`'s server from now on, with its tokens if the TV has them; what was shown from
    /// the server before goes, and what was saved from this one shows.
    pub fn use_server(&mut self, url: &str) {
        let Some(home) = origin(url) else { return };
        if self.server.as_deref() == Some(home.as_str()) && self.api.is_some() {
            return;
        }
        self.stop_events();
        if self.server.is_some() {
            self.leave_server();
        }
        if let Err(e) = self.store.set_active_server(Some(&home)) {
            log::warn!("the server in use not saved: {e}");
        }
        let tokens = Tokens::shared(&home, &self.store, &self.http);
        let ended = self.ended.clone();
        let (issuer, waker) = (home.clone(), self.waker.clone());
        tokens.listen(move |_| {
            *ended.lock().unwrap_or_else(|e| e.into_inner()) = Some(issuer.clone());
            waker.wake();
        });
        let api = Api::new(self.http.clone(), &home, tokens.clone());
        self.kept = tokens.signed_in();
        self.fetch.set(Some(api.clone()));
        self.api = Some(api);
        self.tokens = Some(tokens);
        self.server = Some(home);
        self.signed_out = false;
        self.generation += 1;
        self.read_saved();
    }

    /// Reads what was saved for the server's last profile: who, the libraries, Live TV's summary
    /// and Home, each kept only if it reads.
    fn read_saved(&mut self) {
        let Some(server) = self.server.clone() else { return };
        self.profile = self.store.last_profile(&server);
        let Some(profile) = self.profile else { return };
        let s = &self.snapshots;
        self.me = s.get::<Me>(&server, profile, snapshot_keys::ME).filter(|m| m.id == profile);
        self.libraries = s.get(&server, profile, snapshot_keys::LIBRARIES);
        self.live = s.get(&server, profile, snapshot_keys::LIVE);
        self.requests.summary = s.get(&server, profile, snapshot_keys::REQUESTS);
        self.requests.news = Some(s.get(&server, profile, snapshot_keys::REQUEST_NEWS).unwrap_or_default());
        self.home = s.get::<Hubs>(&server, profile, snapshot_keys::HOME).map(|h| h.hubs);
    }

    /// The tokens of a sign-in that just finished are kept (after the scope guard). The screen
    /// then calls [`AppState::signed_in_now`].
    pub fn install_sign_in(&self, tokens: StoredTokens, asked_scope: String) -> Promise<Result<(), AuthError>> {
        let Some(kept) = self.tokens.clone() else {
            return Promise::resolved(Err(AuthError::Failed("no server is in use".into())));
        };
        self.tasks.spawn(move || kept.install(tokens, &asked_scope))
    }

    /// The sign-in is done: "Who's watching?" isn't asked in this launch, who is watching and the
    /// libraries are asked for, and the events start.
    pub fn signed_in_now(&mut self) {
        self.kept = self.tokens.as_ref().is_some_and(|t| t.signed_in());
        self.signed_out = false;
        self.profile_gate_passed = true;
        self.generation += 1;
        self.load_account();
        self.load_home();
        self.listen(self.visible);
    }

    /// Whether "Who's watching?" was answered in this launch (a profile picked, a PIN checked, a
    /// sign-in approved): the profile's PIN isn't asked again until the app starts over.
    pub fn profile_gate_passed(&self) -> bool {
        self.profile_gate_passed
    }

    /// "Who's watching?" was answered.
    pub fn pass_profile_gate(&mut self) {
        self.profile_gate_passed = true;
    }

    /// Asks the server to act as `profile` (with its PIN when it has one). On success the screen
    /// calls [`AppState::switched`].
    pub fn switch_profile(&self, profile: i64, pin: Option<String>) -> Promise<Result<(), AuthError>> {
        let Some(tokens) = self.tokens.clone() else {
            return Promise::resolved(Err(AuthError::Failed("no server is in use".into())));
        };
        self.tasks.spawn(move || tokens.switch_profile(profile, pin.as_deref()))
    }

    /// The app acts as `profile` now: what plays ends, what was shown for the profile before goes,
    /// that profile's saved answers show, and the server is asked again.
    pub fn switched(&mut self, profile: i64) {
        self.end_playback();
        self.forget_shown();
        self.profile_gate_passed = true;
        if let Some(server) = self.server.clone() {
            let _ = self.store.set_last_profile(&server, profile);
        }
        self.generation += 1;
        self.read_saved();
        self.me = self.me.take().filter(|m| m.id == profile);
        self.load_account();
        self.load_home();
        self.stop_events();
        self.listen(self.visible);
    }

    /// Signs this TV out of the server in use: what plays ends, the tokens are revoked and
    /// forgotten, and what was saved from the server goes. The shell then shows its code screen.
    pub fn sign_out(&mut self) {
        self.stop_events();
        self.end_playback();
        if let (Some(server), Some(tokens)) = (self.server.clone(), self.tokens.clone()) {
            let _ = self.store.remove(&keys::last_profile(&server));
            let snapshots = self.snapshots.clone();
            self.tasks.detach(move || {
                tokens.sign_out();
                snapshots.forget_server(&server);
            });
        }
        self.forget_shown();
        self.profile = None;
        self.kept = false;
        self.signed_out = true;
        self.generation += 1;
    }

    /// The servers this TV chose, most recently used first.
    pub fn saved_servers(&self) -> Vec<SavedServer> {
        self.store.servers()
    }

    /// Keeps `url` first on the server list, as used now.
    pub fn remember_server(&self, url: &str, name: &str, id: Option<&str>) {
        let now_ms =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        if let Err(e) = self.store.remember_server(url, name, id, now_ms) {
            log::warn!("server list not saved: {e}");
        }
    }

    /// Whether the TV holds a sign-in for `url`'s server.
    pub fn has_tokens(&self, url: &str) -> bool {
        origin(url).is_some_and(|home| Tokens::shared(&home, &self.store, &self.http).signed_in())
    }

    /// Forgets a server of the list: its sign-in is revoked and ended, and what was saved from it
    /// goes. Forgetting the server in use leaves none in use (the caller shows Servers).
    pub fn forget_server(&mut self, url: &str) {
        let Some(home) = origin(url) else { return };
        let tokens = match (&self.tokens, self.server.as_deref() == Some(home.as_str())) {
            (Some(t), true) => t.clone(),
            _ => Tokens::shared(&home, &self.store, &self.http),
        };
        // Read before the store forgets them, so they can still be revoked.
        tokens.load();
        if self.server.as_deref() == Some(home.as_str()) {
            self.stop_events();
            self.leave_server();
            self.server = None;
            self.api = None;
            self.tokens = None;
            self.kept = false;
            self.fetch.set(None);
        }
        if let Err(e) = self.store.forget_server(&home) {
            log::warn!("server not forgotten: {e}");
        }
        let snapshots = self.snapshots.clone();
        self.tasks.detach(move || {
            tokens.sign_out();
            snapshots.forget_server(&home);
        });
    }

    /// "Delete all local data": every server's sign-in, the settings, the saved answers and the
    /// pictures go, as if the app were just installed. The caller then shows Servers.
    pub fn delete_local_data(&mut self) {
        self.stop_events();
        self.end_playback();
        let tokens = self.tokens.take();
        if let Some(t) = &tokens {
            t.load();
        }
        self.leave_server();
        self.server = None;
        self.api = None;
        self.kept = false;
        self.fetch.set(None);
        self.store.clear();
        self.settings = Settings::default();
        self.install_id = self.store.install_id();
        self.caps = None;
        self.caps_task = None;
        let snapshots = self.snapshots.clone();
        let pictures = self.device.cache_dir();
        self.tasks.detach(move || {
            if let Some(t) = tokens {
                t.sign_out();
            }
            snapshots.clear();
            if let Ok(entries) = std::fs::read_dir(&pictures) {
                for e in entries.flatten() {
                    let path = e.path();
                    let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                }
            }
        });
    }

    /// What was shown from the server in use goes: another server comes, or none.
    fn leave_server(&mut self) {
        self.end_playback();
        self.forget_shown();
        self.profiles = None;
        self.offline = false;
        self.notice_until = None;
        self.ask_at = None;
        self.profile_gate_passed = false;
        self.profile = None;
        self.generation += 1;
    }

    /// Forgets what the profile signed in showed (who, Home, libraries, Live TV, items).
    fn forget_shown(&mut self) {
        self.me = None;
        self.libraries = None;
        self.live = None;
        self.home = None;
        self.home_error = None;
        self.account_error = None;
        self.fetched = Fetched::default();
        self.scans.clear();
        self.items.clear();
        self.live_known.clear();
        self.live_last_watched = None;
        self.account_me = None;
        self.account_libraries = None;
        self.home_task = None;
        self.live_task = None;
        self.requests.reset();
        self.requests_task = None;
        self.catch_up_task = None;
        self.profiles_task = None;
    }

    /// Ends what plays in the background (the music's session) before the profile, the sign-in,
    /// the server or the app goes.
    pub fn end_playback(&mut self) {
        self.music_hidden_playing = false;
        if let Some(mut m) = self.music.take() {
            m.end();
        }
    }

    // The household.

    /// How many profiles the household had when last asked (1 while unknown).
    pub fn household_size(&self) -> usize {
        self.server.as_deref().map_or(1, |s| self.store.household(s).len().max(1))
    }

    /// The household as last fetched, for "Who's watching?" before the server answers.
    pub fn saved_household(&self) -> Vec<AppProfile> {
        self.server.as_deref().map(|s| self.store.household(s)).unwrap_or_default()
    }

    /// The household as the server listed it now; kept for the next start.
    pub fn keep_household(&mut self, profiles: Vec<AppProfile>) {
        if let Some(server) = self.server.as_deref()
            && let Err(e) = self.store.set_household(server, &profiles)
        {
            log::warn!("household not saved: {e}");
        }
        self.profiles = Some(profiles);
    }

    /// The household as the server last listed it in this launch.
    pub fn profiles(&self) -> Option<&[AppProfile]> {
        self.profiles.as_deref()
    }

    /// The profile Settings' "Always start as" names for this server; `None` asks who's watching.
    pub fn always_start_as(&self) -> Option<i64> {
        self.server.as_deref().and_then(|s| self.settings.always_start_as(s))
    }

    /// Sets "Always start as" for this server.
    pub fn set_always_start_as(&mut self, profile: Option<i64>) {
        let Some(server) = self.server.clone() else { return };
        self.update_settings(|s| s.set_always_start_as(&server, profile));
    }

    // What the server has.

    /// Who is watching, as last heard.
    pub fn me(&self) -> Option<&Me> {
        self.me.as_ref()
    }

    /// The libraries in the profile's order, as last heard.
    pub fn libraries(&self) -> Option<&[Library]> {
        self.libraries.as_deref()
    }

    /// Live TV's summary; `None` until the server says, or where it has none.
    pub fn live(&self) -> Option<&LiveSummary> {
        self.live.as_ref()
    }

    /// Whether the rail shows Live TV.
    pub fn live_available(&self) -> bool {
        self.live.as_ref().is_some_and(|l| l.available)
    }

    /// Home's rows, as last seen (saved, then the server's).
    pub fn home(&self) -> Option<&[Hub]> {
        self.home.as_deref()
    }

    /// Why Home's last load failed; the saved Home stays on screen meanwhile.
    pub fn home_error(&self) -> Option<&ApiError> {
        self.home_error.as_ref()
    }

    /// Whether Home's rows came from the server in this launch (not only from what was saved).
    pub fn home_fresh(&self) -> bool {
        self.fetched.home.is_some()
    }

    /// Why the last try at who is watching and the libraries failed (the library pages' Retry).
    pub fn account_error(&self) -> Option<&ApiError> {
        self.account_error.as_ref()
    }

    /// The library scans running, by library.
    pub fn scans(&self) -> &BTreeMap<i64, Scan> {
        &self.scans
    }

    /// Keeps the libraries as an answer gave them (a new order).
    pub fn keep_libraries(&mut self, libraries: Vec<Library>) {
        self.save_snapshot(snapshot_keys::LIBRARIES, &libraries);
        self.libraries = Some(libraries);
    }

    /// Asks for who is watching and the libraries (then the household and Live TV's summary).
    pub fn load_account(&mut self) {
        if self.account_me.is_some() || self.api.is_none() {
            return;
        }
        let generation = self.generation;
        self.account_me = Some(Pending { generation, promise: self.spawn(|api| api.me()) });
        self.account_libraries = Some(Pending { generation, promise: self.spawn(|api| api.libraries()) });
        // What requests are for it, alongside.
        self.load_requests(false);
    }

    // Requests.

    /// The requests summary (docs/API.md, "Requests"): whether Discover and the requests show,
    /// and the rail's number. `None` while unknown, and for a server without requests.
    pub fn request_summary(&self) -> Option<&RequestSummary> {
        self.requests.summary.as_ref()
    }

    /// The events aren't connected: pages that show requests ask on their own meanwhile.
    pub fn requests_events_down(&self) -> bool {
        self.requests.events_down
    }

    /// Asks for the summary: at start, after a profile switch, when the events reconnect, on
    /// request events (once, 1 s after a burst) and every 60 s while the events are down. One at
    /// a time: `changed` (something changed since the one on its way was asked for) asks once
    /// more after it. A server without requests (404) or with an older build of them (403) isn't
    /// asked until its events reconnect.
    pub fn load_requests(&mut self, changed: bool) {
        if self.requests_task.is_some() {
            self.requests.again |= changed;
            return;
        }
        if self.api.is_none() || !self.signed_in() || self.requests.missing {
            return;
        }
        let generation = self.generation;
        self.requests_task = Some(Pending { generation, promise: self.spawn(|api| api.request_summary()) });
    }

    /// Something done here changed requests for one title (`kind:tmdb_id`), or for many: the
    /// pages that show them hear [`AppEvent::RequestsChanged`] once, as after the server's events.
    pub fn requests_changed(&mut self, title: Option<String>) {
        self.requests.changed(title, self.now);
    }

    /// The ready-to-watch notice to show next.
    pub fn request_notice(&self) -> Option<&RequestNotice> {
        self.requests.notice()
    }

    /// The shell shows this notice (`None`: none).
    pub fn showing_request_notice(&mut self, id: Option<u64>) {
        self.requests.showing_notice(id);
    }

    /// The notice was opened, closed or timed out.
    pub fn drop_request_notice(&mut self, id: u64) {
        self.requests.drop_notice(id);
    }

    fn collect_requests(&mut self) {
        if let Some(task) = self.requests_task.as_mut() {
            let answer = task.take();
            let failed = answer.is_none() && task.promise.failed();
            let generation = task.generation;
            if answer.is_some() || failed {
                self.requests_task = None;
            }
            if let Some(answer) = answer
                && generation == self.generation
            {
                match answer {
                    Ok(summary) => {
                        self.save_snapshot(snapshot_keys::REQUESTS, &summary);
                        let news = self.requests.on_summary(summary);
                        if news.changed {
                            self.requests.changed(None, self.now);
                        }
                        if news.catch_up {
                            self.catch_up();
                        }
                    }
                    Err(e) if e.is(404) || e.is(403) => {
                        self.requests.missing = true;
                        self.requests.summary = None;
                        if let (Some(server), Some(profile)) = (self.server.clone(), self.profile) {
                            let snapshots = self.snapshots.clone();
                            self.tasks.detach(move || snapshots.remove(&server, profile, snapshot_keys::REQUESTS));
                        }
                    }
                    // Out of reach: what was heard stays.
                    Err(_) => {}
                }
            }
            if self.requests_task.is_none() && std::mem::take(&mut self.requests.again) {
                self.load_requests(false);
            }
        }
        if let Some(task) = self.catch_up_task.as_mut() {
            let answer = task.take();
            if answer.is_some() || task.promise.failed() {
                let generation = task.generation;
                self.catch_up_task = None;
                // Failed: asked again with the next summary that has news.
                if let Some(Ok(list)) = answer
                    && generation == self.generation
                    && self.requests.caught_up_with(&list.requests)
                    && let Some(kept) = self.requests.news.clone()
                {
                    self.save_snapshot(snapshot_keys::REQUEST_NEWS, &kept);
                }
            }
        }
        if let Some(title) = self.requests.take_change(self.now) {
            self.out.push(AppEvent::RequestsChanged { title });
        }
        if self.requests.summary_due.is_some_and(|t| self.now >= t) {
            self.requests.summary_due = None;
            self.load_requests(true);
        }
        if self.requests.poll_due.is_some_and(|t| self.now >= t) {
            self.requests.poll_due = None;
            if self.requests.events_down && self.events.is_some() {
                if self.visible {
                    self.load_requests(false);
                }
                self.requests.poll_due = Some(self.now + SUMMARY_POLL);
            }
        }
    }

    /// News that came while the app wasn't listening: the profile's 20 newest unseen requests,
    /// of which what is ready to watch pops up.
    fn catch_up(&mut self) {
        if self.catch_up_task.is_some() || self.api.is_none() {
            return;
        }
        let generation = self.generation;
        let query = RequestQuery { unseen: true, limit: Some(20), ..RequestQuery::default() };
        self.catch_up_task = Some(Pending { generation, promise: self.spawn(move |api| api.requests(&query)) });
    }

    /// A request event: the summary once after the burst, the pages told, and news popped up.
    fn request_event(&mut self, e: &ServerEvent) {
        let now = self.now;
        let key = |kind: &Option<TitleKind>, tmdb: &Option<i64>| match (kind, tmdb) {
            (Some(k), Some(t)) => Some(title_key(*k, *t)),
            _ => None,
        };
        match e {
            ServerEvent::RequestsChanged { kind, tmdb_id, .. } => {
                self.requests.summary_soon(now);
                self.requests.changed(key(kind, tmdb_id), now);
            }
            ServerEvent::RequestAvailable {
                request_id,
                title,
                kind,
                tmdb_id,
                item_id,
                poster_url,
                seasons,
                complete,
            } => {
                self.requests.news_heard += 1;
                self.requests.summary_soon(now);
                self.requests.changed(key(kind, tmdb_id), now);
                let before = self.requests.news.clone();
                self.requests.ready(*request_id, title, *item_id, poster_url.clone(), seasons.as_deref(), *complete);
                if self.requests.news != before
                    && let Some(kept) = self.requests.news.clone()
                {
                    self.save_snapshot(snapshot_keys::REQUEST_NEWS, &kept);
                }
            }
            // Declined news raises `unseen`; TVs show it as the number and in the lists, never as
            // a pop-up.
            ServerEvent::RequestDecided { .. } => {
                self.requests.news_heard += 1;
                self.requests.summary_soon(now);
                self.requests.changed(None, now);
            }
            ServerEvent::RequestCreated { .. } | ServerEvent::RequestFailed { .. } => {
                self.requests.summary_soon(now);
                self.requests.changed(None, now);
            }
            ServerEvent::RequestsSettingsChanged => {
                self.requests.missing = false;
                self.requests.summary_soon(now);
            }
            _ => {}
        }
    }

    /// Asks whether Live TV shows for this profile.
    pub fn load_live(&mut self) {
        if self.live_task.is_some() || self.api.is_none() {
            return;
        }
        self.live_task = Some(Pending { generation: self.generation, promise: self.spawn(|api| api.live_summary()) });
    }

    /// Asks for Home's rows.
    pub fn load_home(&mut self) {
        if self.home_task.is_some() || self.api.is_none() {
            return;
        }
        self.home_task = Some(Pending { generation: self.generation, promise: self.spawn(|api| api.home()) });
    }

    /// Asks again for what is older than 30 s or was never asked (a screen showing it).
    pub fn revalidate(&mut self) {
        if !self.signed_in() {
            return;
        }
        let now = self.now;
        let old = |at: Option<f64>| at.is_none_or(|t| now - t > REVALIDATE);
        if old(self.fetched.home) {
            self.load_home();
        }
        if old(self.fetched.account) || self.account_error.is_some() {
            self.load_account();
        } else if old(self.fetched.live) {
            self.load_live();
        }
    }

    /// Home and the pages that list things ask again; while a playback is on top, once it ends.
    pub fn refresh(&mut self) {
        if self.holds > 0 {
            return;
        }
        self.load_home();
        // Who is watching and the libraries, when the last try failed or none came yet.
        if self.account_error.is_some() || self.libraries.is_none() {
            self.load_account();
        } else {
            self.load_live();
        }
        self.out.push(AppEvent::Changed);
    }

    /// A playback is on top (`true`) or has ended (`false`). Meanwhile Home and the pages wait to
    /// ask again; once it ends they do, and the details kept go, since its own reports moved
    /// resume points and watched marks.
    pub fn hold_refresh(&mut self, on: bool) {
        if on {
            self.holds += 1;
            return;
        }
        self.holds = self.holds.saturating_sub(1);
        if self.holds > 0 {
            return;
        }
        self.items.clear();
        self.load_home();
        self.out.push(AppEvent::Changed);
    }

    // Item details and live channels kept for pages and playbacks.

    /// An item's details seen in the last 5 minutes, to start a page or a playback without waiting.
    pub fn cached_item(&mut self, id: i64) -> Option<&ItemDetail> {
        let now = self.now;
        let at = self.items.iter().position(|(d, _)| d.summary.id == id)?;
        if now - self.items[at].1 > ITEM_TTL {
            self.items.remove(at);
            return None;
        }
        self.items.get(at).map(|(d, _)| d)
    }

    /// Keeps a detail fetched elsewhere (an item page) for the next page or playback.
    pub fn keep_item(&mut self, detail: ItemDetail) {
        self.items.retain(|(d, _)| d.summary.id != detail.summary.id);
        self.items.push_back((detail, self.now));
        while self.items.len() > ITEMS_KEPT {
            self.items.pop_front();
        }
    }

    /// Tracks were picked for `detail`'s title (or the profile's defaults changed, `None`): the
    /// details kept name the tracks they started with, so they go; `detail` is kept.
    pub fn tracks_changed(&mut self, detail: Option<ItemDetail>) {
        self.items.clear();
        if let Some(d) = detail {
            self.keep_item(d);
        }
    }

    /// A live channel a page showed: the player starts with it at once.
    pub fn keep_live_channel(&mut self, channel: LiveChannel) {
        self.live_known.retain(|c| c.id != channel.id);
        self.live_known.push_back(channel);
        while self.live_known.len() > LIVE_CHANNELS_KEPT {
            self.live_known.pop_front();
        }
    }

    /// A live channel a page showed lately.
    pub fn known_live_channel(&self, id: i64) -> Option<&LiveChannel> {
        self.live_known.iter().rev().find(|c| c.id == id)
    }

    // The TV's capabilities.

    /// The capability report, once probed ([`AppState::probe_caps`]).
    pub fn caps(&self) -> Option<&CapsReport> {
        self.caps.as_ref()
    }

    /// Probes what the TV plays (off the main thread): once after the first screen is drawn, again
    /// when Settings change passthrough or Diagnostics asks anew (`fresh`).
    pub fn probe_caps(&mut self, fresh: bool) {
        if fresh {
            self.caps = None;
            self.caps_task = None;
        }
        if self.caps.is_some() || self.caps_task.is_some() {
            return;
        }
        let device = self.device.clone();
        let overrides = CapsOverrides::from_settings(&self.settings);
        self.caps_task = Some(self.tasks.spawn(move || device.caps(&overrides)));
    }

    // The server out of reach.

    /// Whether the server is out of reach: what shows is what was saved.
    pub fn offline(&self) -> bool {
        self.offline
    }

    /// Whether "Pley is offline" shows (its first 6 s); after it the rail's avatar keeps a mark
    /// while [`AppState::offline`].
    pub fn offline_notice(&self) -> bool {
        self.offline && self.notice_until.is_some_and(|t| self.now < t)
    }

    /// A load got no answer: the notice shows for 6 s (again each time the server goes), then the
    /// avatar's mark; meanwhile the server is asked every 5 s. Pley's client gives home-network
    /// servers 3 s to connect (`pley_core::http::HOME_CONNECT_TIMEOUT`), the TV app's watchdog.
    pub fn server_away(&mut self) {
        if self.offline {
            return;
        }
        log::info!("the server is out of reach");
        self.offline = true;
        self.notice_until = Some(self.now + OFFLINE_NOTICE);
        self.ask_at = Some(self.now + OFFLINE_ASK);
    }

    /// The server answers again: its events connected, or a load that had failed got a fresh answer.
    pub fn server_answered(&mut self) {
        if !self.offline {
            return;
        }
        log::info!("the server answers again");
        self.offline = false;
        self.notice_until = None;
        self.ask_at = None;
    }

    /// Notes a load's failure: an unreachable server raises the offline state.
    pub fn report_failure(&mut self, error: &ApiError) {
        if error.unreachable() {
            self.server_away();
        }
    }

    // The app shown or not, and the server's events.

    /// Whether the TV shows the app.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// The app was shown (`true`) or hidden: the events follow, and the server is asked again on
    /// return.
    pub fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        // Music the TV's Home paused plays on when the app is back (`App.cs`'s MusicShown).
        if let Some(m) = self.music.as_mut() {
            if !visible {
                self.music_hidden_playing = m.playing();
                m.pause();
            } else if std::mem::take(&mut self.music_hidden_playing) {
                m.play_on();
            }
        }
        self.listen(visible);
        if visible && self.offline {
            self.ask_at = Some(self.now);
        }
    }

    /// Follows the server's events while the app shows and is signed in.
    pub fn listen(&mut self, on: bool) {
        let Some(api) = self.api.clone().filter(|_| on && self.signed_in()) else {
            self.stop_events();
            return;
        };
        if self.events.is_none() {
            let (tx, rx) = mpsc::channel();
            self.events = Some((EventStream::start(api, tx, self.waker.clone()), rx));
        }
    }

    fn stop_events(&mut self) {
        if let Some((mut stream, _)) = self.events.take() {
            stream.stop();
        }
    }

    /// Whether the event stream runs.
    pub fn listening(&self) -> bool {
        self.events.is_some()
    }

    fn reconnected(&mut self) {
        // The server may have restarted, newer: one without requests is asked again, and what
        // changed while the events were down is told.
        let down = std::mem::take(&mut self.requests.events_down);
        self.requests.poll_due = None;
        self.requests.missing = false;
        self.load_requests(false);
        if down {
            self.requests.changed(None, self.now);
        }
        // Nothing is replayed: what is older than 30 s, or anything after the server was out of
        // reach, is fetched again.
        let away = self.offline;
        self.server_answered();
        if away || self.now - self.last_sync > REVALIDATE {
            self.last_sync = self.now;
            self.items.clear();
            self.refresh();
        }
    }

    fn handle_event(&mut self, e: ServerEvent) {
        if e.about_requests() {
            self.request_event(&e);
        }
        match &e {
            ServerEvent::LibraryChanged { .. } => self.library_due = Some(self.now + LIBRARY_GATHER),
            ServerEvent::LibrariesChanged => {
                self.load_account();
                self.refresh();
            }
            ServerEvent::WatchlistChanged => self.refresh(),
            ServerEvent::LiveChannelsChanged => {
                self.load_live();
                self.out.push(AppEvent::LiveChanged);
            }
            ServerEvent::UserStateChanged { item_ids, children } => {
                self.state_ids.extend(item_ids.iter().copied());
                if *children && let Some(&first) = item_ids.first() {
                    self.state_parents.insert(first);
                }
                if self.state_due.is_none() {
                    self.state_due = Some(self.now + STATE_GATHER);
                }
            }
            ServerEvent::ScanStarted { library_id } => {
                self.scans.insert(*library_id, Scan::default());
            }
            ServerEvent::ScanProgress { library_id, phase, processed, total } => {
                self.scans.insert(*library_id, Scan { phase: *phase, processed: *processed, total: *total });
            }
            ServerEvent::ScanFinished { library_id, .. } => {
                // Home asks once more when the last scan ends (Android's `HomeViewModel.kt:286-290`).
                let last = self.scans.remove(library_id).is_some() && self.scans.is_empty();
                if last {
                    self.load_home();
                }
            }
            _ => {}
        }
        if let Some(m) = self.music.as_mut() {
            m.server_event(&e);
        }
        self.out.push(AppEvent::Server(e));
    }

    /// The watch-state changes of the last 0.3 s: their details go, and Home and the pages ask
    /// again once (with a pending library change).
    fn apply_state_changes(&mut self) {
        self.state_due = None;
        let (ids, parents) = (&self.state_ids, &self.state_parents);
        let under = |p: Option<i64>| p.is_some_and(|p| parents.contains(&p));
        self.items.retain(|(d, _)| {
            !(ids.contains(&d.summary.id) || under(d.summary.parent_id) || under(d.summary.grandparent_id))
        });
        self.state_ids.clear();
        self.state_parents.clear();
        self.library_due = None;
        self.refresh();
    }

    // The frame.

    /// Collects what came back since the last frame (answers, the events' news, timers due) and
    /// returns what the shell should tell the pages.
    pub fn update(&mut self, now: f64) -> Vec<AppEvent> {
        self.now = now;
        // A quality chosen in a player's menu is the one the next title starts at.
        if let Some(quality) = self.players.quality.take_chosen() {
            self.update_settings(|s| s.quality = quality);
        }
        self.collect_ended();
        self.collect_account();
        self.collect_home();
        self.collect_live();
        self.collect_requests();
        self.collect_profiles();
        self.collect_reach();
        if let Some(p) = self.caps_task.as_mut()
            && let Some(report) = p.take()
        {
            self.caps = Some(report);
            self.caps_task = None;
        }
        self.collect_events();
        if self.notice_until.is_some_and(|t| now >= t) {
            self.notice_until = None;
        }
        if self.offline && self.visible && self.reach_task.is_none() && self.ask_at.is_some_and(|t| now >= t) {
            self.ask_at = None;
            let timeout = Duration::from_secs_f64(OFFLINE_ASK);
            self.reach_task = Some(Pending {
                generation: self.generation,
                promise: self.spawn(move |api| api.reachable(timeout).map(drop)),
            });
        }
        if self.library_due.is_some_and(|t| now >= t) {
            self.library_due = None;
            self.refresh();
        }
        if self.state_due.is_some_and(|t| now >= t) {
            self.apply_state_changes();
        }
        std::mem::take(&mut self.out)
    }

    /// When a timer of the state is next due ([`pley_base::time::now`]'s clock).
    pub fn next_due(&self) -> Option<f64> {
        let ask = self.ask_at.filter(|_| self.offline && self.visible && self.reach_task.is_none());
        [self.notice_until, ask, self.library_due, self.state_due, self.requests.next_due()]
            .into_iter()
            .flatten()
            .reduce(f64::min)
    }

    /// Whether answers are on their way (tests wait for them).
    pub fn busy(&self) -> bool {
        self.account_me.is_some()
            || self.home_task.is_some()
            || self.live_task.is_some()
            || self.requests_task.is_some()
            || self.catch_up_task.is_some()
            || self.profiles_task.is_some()
            || self.reach_task.is_some()
            || self.caps_task.is_some()
    }

    fn collect_ended(&mut self) {
        let ended = self.ended.lock().unwrap_or_else(|e| e.into_inner()).take();
        if ended.is_some() && ended.as_deref() == self.server.as_deref() && !self.signed_out {
            log::info!("the server ended the sign-in");
            self.stop_events();
            self.end_playback();
            self.kept = false;
            self.signed_out = true;
            self.generation += 1;
            self.out.push(AppEvent::SignedOut);
        }
    }

    fn collect_account(&mut self) {
        let (Some(me), Some(libraries)) = (self.account_me.as_mut(), self.account_libraries.as_mut()) else { return };
        if !(me.promise.ready() || me.promise.failed()) || !(libraries.promise.ready() || libraries.promise.failed()) {
            return;
        }
        let generation = me.generation;
        let me = me.take().unwrap_or(Err(ApiError::Unreadable("no answer".into())));
        let libraries = libraries.take().unwrap_or(Err(ApiError::Unreadable("no answer".into())));
        self.account_me = None;
        self.account_libraries = None;
        if generation != self.generation {
            return;
        }
        match (me, libraries) {
            (Ok(me), Ok(libraries)) => {
                self.account_error = None;
                self.fetched.account = Some(self.now);
                if let Some(server) = self.server.clone()
                    && self.profile != Some(me.id)
                {
                    // Another profile than the one saved answers showed for (switched elsewhere).
                    let _ = self.store.set_last_profile(&server, me.id);
                    self.profile = Some(me.id);
                }
                self.save_snapshot(snapshot_keys::ME, &me);
                self.me = Some(me);
                self.keep_libraries(libraries);
                if self.tokens.as_ref().is_some_and(|t| t.allows("profiles")) {
                    self.profiles_task = Some(Pending { generation, promise: self.spawn(|api| api.profiles()) });
                }
                self.load_live();
            }
            (Err(e), _) | (_, Err(e)) => {
                log::info!("who is watching not loaded: {e}");
                self.report_failure(&e);
                self.account_error = Some(e);
            }
        }
    }

    fn collect_home(&mut self) {
        let Some(task) = self.home_task.as_mut() else { return };
        let Some(answer) = task.take() else {
            if task.promise.failed() {
                self.home_task = None;
            }
            return;
        };
        let generation = task.generation;
        self.home_task = None;
        if generation != self.generation {
            return;
        }
        match answer {
            Ok(hubs) => {
                let failed_before = self.home_error.is_some();
                self.fetched.home = Some(self.now);
                self.save_snapshot(snapshot_keys::HOME, &hubs);
                self.home = Some(hubs.hubs);
                self.home_error = None;
                if failed_before {
                    self.server_answered();
                }
            }
            Err(e) => {
                log::info!("Home not loaded: {e}");
                self.report_failure(&e);
                self.home_error = Some(e);
            }
        }
    }

    fn collect_live(&mut self) {
        let Some(task) = self.live_task.as_mut() else { return };
        let Some(answer) = task.take() else {
            if task.promise.failed() {
                self.live_task = None;
            }
            return;
        };
        let generation = task.generation;
        self.live_task = None;
        if generation != self.generation {
            return;
        }
        match answer {
            Ok(summary) => {
                self.fetched.live = Some(self.now);
                self.save_snapshot(snapshot_keys::LIVE, &summary);
                self.live = Some(summary);
            }
            // A server without Live TV, or not for this profile: no entry in the rail.
            Err(e) if e.is(404) || e.is(403) => {
                self.fetched.live = Some(self.now);
                self.live = None;
                if let (Some(server), Some(profile)) = (self.server.clone(), self.profile) {
                    let snapshots = self.snapshots.clone();
                    self.tasks.detach(move || snapshots.remove(&server, profile, snapshot_keys::LIVE));
                }
            }
            // Out of reach: what was kept stays.
            Err(e) => self.report_failure(&e),
        }
    }

    fn collect_profiles(&mut self) {
        let Some(task) = self.profiles_task.as_mut() else { return };
        let Some(answer) = task.take() else {
            if task.promise.failed() {
                self.profiles_task = None;
            }
            return;
        };
        let generation = task.generation;
        self.profiles_task = None;
        if generation == self.generation
            && let Ok(profiles) = answer
        {
            self.keep_household(profiles);
        }
    }

    fn collect_reach(&mut self) {
        let Some(task) = self.reach_task.as_mut() else { return };
        let Some(answer) = task.take() else {
            if task.promise.failed() {
                self.reach_task = None;
                self.ask_at = Some(self.now + OFFLINE_ASK);
            }
            return;
        };
        self.reach_task = None;
        if !self.offline {
            return;
        }
        match answer {
            Ok(()) => {
                // A stream waiting out its back-off connects now; the app asks again itself, in
                // case the stream stayed connected through a short blip.
                if let Some((stream, _)) = &self.events {
                    stream.retry_now();
                }
                self.server_answered();
                self.items.clear();
                self.last_sync = self.now;
                self.refresh();
            }
            Err(_) => self.ask_at = Some(self.now + OFFLINE_ASK),
        }
    }

    fn collect_events(&mut self) {
        let mut news = Vec::new();
        let mut stopped = false;
        if let Some((_, rx)) = &self.events {
            loop {
                match rx.try_recv() {
                    Ok(n) => news.push(n),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        stopped = true;
                        break;
                    }
                }
            }
        }
        for n in news {
            match n {
                StreamNews::Connected => self.reconnected(),
                StreamNews::Event(e) => self.handle_event(e),
                StreamNews::Stopped => stopped = true,
                // Until the events connect again, the summary is asked for every 60 s.
                StreamNews::Lost => {
                    self.requests.events_down = true;
                    if self.requests.poll_due.is_none() {
                        self.requests.poll_due = Some(self.now + SUMMARY_POLL);
                    }
                }
            }
        }
        if stopped {
            // Not allowed (403) or the sign-in ended: it starts again on the next return to the
            // foreground or sign-in.
            self.stop_events();
        }
    }

    /// Saves an answer for the server and profile shown, off the main thread.
    fn save_snapshot<T: serde::Serialize>(&self, key: &'static str, value: &T) {
        let (Some(server), Some(profile)) = (self.server.clone(), self.profile()) else { return };
        let Ok(bytes) = serde_json::to_vec(value) else { return };
        let snapshots = self.snapshots.clone();
        self.tasks.detach(move || {
            if let Err(e) = snapshots.write(&server, profile, key, &bytes) {
                log::warn!("snapshot {key} not saved: {e}");
            }
        });
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.stop_events();
    }
}
