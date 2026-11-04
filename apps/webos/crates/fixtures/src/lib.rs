//! A fake Pley server on 127.0.0.1 with a demo household, for the simulator
//! and the tests of Pley for LG TV: a port of the Android app's fixture server
//! (`apps/android/testing/fixtures`), answering the server's routes with its
//! JSON as an app's token sees them (docs/API.md):
//!
//! - signing in a TV with a code (approved at once, or after a delay), token
//!   refresh with rotation and reuse detection, revocation, the server's
//!   proofs of who it is (docs/API.md, "Telling servers apart"), the
//!   household's profiles and switching between them (Sam's PIN is
//!   [`ids::PIN`]);
//! - the catalogue of [`ids`]: libraries with sorting, filters and paging,
//!   Home's and libraries' hubs, items with files, streams, chapters, markers
//!   and people, collections, playlists, My List, history and search;
//! - playback: sessions answered direct or as HLS, at keyed addresses when
//!   the player can't send a token, serving the Android fixtures' sample clips
//!   with Range; WebVTT subtitles, PGS as `.mks`, seek previews and chapter
//!   pictures, subtitle search, bitrate changes, stopping;
//! - requests and Discover: a small pretend TMDB, the summary, asking,
//!   following, managers' decisions and the listing's filters;
//! - Live TV with a guide, keyed live HLS, sessions, plans and a connection
//!   limit that answers `409`;
//! - server events (SSE), the user's remote (commands, and casts to the
//!   devices whose event streams name them) and pictures it draws itself
//!   (JPEG, PNG with transparency for channel logos);
//! - and, apart from the server, a pretend USB drive of videos that are heads
//!   only, for the simulator's `--drive fixtures` ([`write_sample_drive`]).
//!
//! ```no_run
//! let server = pley_fixtures::FixtureServer::start();
//! let tokens = server.sign_in(pley_fixtures::ids::ALEX);
//! // GET {server.url()}/api/libraries with "Authorization: Bearer {tokens.access_token}"
//! server.stop();
//! ```

mod browse;
mod cast;
mod catalog;
mod drive;
mod events;
mod extra;
mod household;
mod http;
pub mod ids;
mod jpeg;
mod lists;
mod live;
mod media;
mod model;
mod oauth;
mod pgs;
mod pictures;
mod play;
mod playlists;
mod png;
mod random;
mod requests;
mod routes;
mod server;
mod views;
mod words;

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use cast::CastBody;
pub use drive::write_sample_drive;
pub use events::ServerEvent;
pub use requests::titles as request_titles;

use crate::catalog::Catalog;
use crate::events::{EventHub, Scans};
use crate::household::Household;
use crate::http::{Listener, lock};
use crate::media::{Clip, Media};
use crate::server::{ALL_SCOPES, Shared, State};

/// The server's clock, in unix milliseconds: token lifetimes, poll
/// intervals, dates of plays, the Live TV guide.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> i64 + Send + Sync>);

impl Clock {
    /// The system's clock.
    pub fn system() -> Clock {
        Clock::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        })
    }

    /// A clock that reads `now` (unix ms).
    pub fn new(now: impl Fn() -> i64 + Send + Sync + 'static) -> Clock {
        Clock(Arc::new(now))
    }

    pub fn now_ms(&self) -> i64 {
        (self.0)()
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Clock({})", self.now_ms())
    }
}

/// A clock tests move by hand.
#[derive(Clone, Debug)]
pub struct ManualClock(Arc<AtomicI64>);

impl ManualClock {
    /// A clock at `now_ms` (unix ms).
    pub fn new(now_ms: i64) -> ManualClock {
        ManualClock(Arc::new(AtomicI64::new(now_ms)))
    }

    /// The [`Clock`] to give the server in [`Options::clock`].
    pub fn clock(&self) -> Clock {
        let now = self.0.clone();
        Clock::new(move || now.load(Ordering::SeqCst))
    }

    pub fn advance(&self, by: Duration) {
        self.0.fetch_add(by.as_millis() as i64, Ordering::SeqCst);
    }

    pub fn set(&self, now_ms: i64) {
        self.0.store(now_ms, Ordering::SeqCst);
    }
}

/// How the fixture server behaves. Every field has the real server's value or
/// a sensible default.
#[derive(Clone, Debug)]
pub struct Options {
    /// The port to listen on; 0 for any free one.
    pub port: u16,
    /// Routes need a token. Off, requests without one act as the sign-in
    /// profile with every scope.
    pub require_auth: bool,
    /// Between the event stream's keep-alive comments (the server's 15 s).
    pub keep_alive: Duration,
    /// Between two pretend library scans' events; `None` for none.
    pub scan_every: Option<Duration>,
    /// What the device grant asks TVs to wait between polls, in seconds.
    pub device_interval_secs: u32,
    /// How long the person takes to approve a sign-in code; zero approves it
    /// at the first poll.
    pub device_delay: Duration,
    /// [`Mode::Slow`]: added before each answer.
    pub slow_latency: Duration,
    /// [`Mode::Slow`]: the bodies' pace.
    pub slow_bytes_per_second: u64,
    pub clock: Clock,
    /// Where the sample media are; `None` for `PLEY_FIXTURES_MEDIA` when set,
    /// else the Android fixtures' folder in this repository
    /// (`apps/android/testing/fixtures/src/main/resources/app/pley/android/fixtures/media`).
    pub media_dir: Option<PathBuf>,
    /// Tokens this server never issued sign in as the sign-in profile with
    /// every scope: a simulator restarted with saved tokens stays signed in.
    pub adopt_unknown_tokens: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            port: 0,
            require_auth: true,
            keep_alive: Duration::from_secs(15),
            scan_every: Some(Duration::from_secs(30)),
            device_interval_secs: 5,
            device_delay: Duration::ZERO,
            slow_latency: Duration::from_millis(800),
            slow_bytes_per_second: 256 * 1_024,
            clock: Clock::system(),
            media_dir: None,
            adopt_unknown_tokens: false,
        }
    }
}

/// How the server answers, changed at any time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Every answer late and its body slow ([`Options::slow_latency`],
    /// [`Options::slow_bytes_per_second`]).
    Slow,
    /// Not listening: connections are refused, on the same port, until the
    /// mode changes; open event streams end.
    Down,
}

/// How the person answers a TV's sign-in code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceApproval {
    /// Allowed, as [`FixtureServer::set_sign_in_as`]'s profile, once
    /// [`Options::device_delay`] has passed.
    Allow,
    /// Refused once the delay has passed.
    Deny,
    /// Never answered: the code expires.
    Never,
}

/// A token pair, as the token endpoint gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds.
    pub expires_in: i64,
    /// Space separated.
    pub scope: String,
    /// The sign-in's id and key, with which the server proves itself to the
    /// app later (docs/API.md, "Telling servers apart"): the same at every
    /// refresh and profile switch.
    pub proof_id: String,
    pub proof_key: String,
}

/// One write an app made: every POST, PUT, PATCH and DELETE under `/api` but
/// the sign-in's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub method: String,
    /// As sent, still percent-encoded.
    pub path: String,
    pub query: Option<String>,
    /// The profile the token acted as.
    pub profile_id: Option<i64>,
    /// The body as it came (JSON, or empty).
    pub body: String,
    /// When it came, on the server's clock.
    pub at_ms: i64,
}

struct Listening {
    listener: Option<Listener>,
    port: u16,
    mode: Mode,
    scans: Option<Scans>,
    stopped: bool,
}

/// The fake Pley server, listening on 127.0.0.1 on its own threads until
/// [`stop`](FixtureServer::stop)ped or dropped.
pub struct FixtureServer {
    shared: Arc<Shared>,
    listening: Mutex<Listening>,
    url: String,
}

impl FixtureServer {
    /// Starts a server with the default options on a free port.
    ///
    /// # Panics
    ///
    /// When it can't listen on 127.0.0.1.
    pub fn start() -> FixtureServer {
        FixtureServer::start_with(Options::default()).expect("the fixture server listens on 127.0.0.1")
    }

    /// Starts a server with `options`.
    pub fn start_with(options: Options) -> io::Result<FixtureServer> {
        let media = Media::new(options.media_dir.clone().unwrap_or_else(media::default_dir));
        let catalog = Catalog::new(media.size(Clip::Sdr1080), media.size(Clip::Hdr2160));
        let household = Household::new(&catalog);
        let now_ms = options.clock.now_ms();
        let mut oauth = oauth::OAuth::new(&options);
        oauth.adopt_unknown_tokens = options.adopt_unknown_tokens;
        let state = State {
            household,
            oauth,
            playback: play::Playback::default(),
            live: live::Live::new(now_ms),
            network: extra::NetworkMemory::default(),
            requests: requests::Requests::new(now_ms.div_euclid(1_000)),
            subtitle_search: true,
            two_step_required: false,
            subtitle_asks: Default::default(),
            added_subtitles: Default::default(),
            casts_sent: Default::default(),
        };
        let libraries: Vec<(i64, i64)> = catalog.libraries.iter().map(|l| (l.id, l.item_count)).collect();
        let shared = Arc::new(Shared {
            options: options.clone(),
            catalog,
            media,
            pictures: pictures::Pictures::default(),
            state: Mutex::new(state),
            events: Arc::new(EventHub::default()),
            journal: Mutex::new(Vec::new()),
            slow: AtomicBool::new(false),
            routes: routes::all(),
        });
        let listener = Listener::bind(options.port, shared.clone())?;
        let port = listener.port();
        let scans = options.scan_every.map(|every| Scans::start(shared.events.clone(), libraries, every));
        Ok(FixtureServer {
            shared,
            listening: Mutex::new(Listening {
                listener: Some(listener),
                port,
                mode: Mode::Normal,
                scans,
                stopped: false,
            }),
            url: format!("http://127.0.0.1:{port}"),
        })
    }

    /// The server's address: `http://127.0.0.1:<port>`.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn port(&self) -> u16 {
        lock(&self.listening).port
    }

    /// Stops listening, ends every connection and event stream; the port is
    /// free when this returns. Stopping again does nothing.
    pub fn stop(&self) {
        let mut listening = lock(&self.listening);
        if listening.stopped {
            return;
        }
        listening.stopped = true;
        if let Some(mut scans) = listening.scans.take() {
            scans.stop();
        }
        self.shared.events.end_all();
        if let Some(mut listener) = listening.listener.take() {
            listener.stop();
        }
    }

    pub fn mode(&self) -> Mode {
        lock(&self.listening).mode
    }

    /// Changes how the server answers (see [`Mode`]).
    pub fn set_mode(&self, mode: Mode) -> io::Result<()> {
        let mut listening = lock(&self.listening);
        if listening.stopped {
            return Ok(());
        }
        self.shared.slow.store(mode == Mode::Slow, Ordering::SeqCst);
        if mode == Mode::Down && listening.listener.is_some() {
            self.shared.events.end_all();
            if let Some(mut listener) = listening.listener.take() {
                listener.stop();
            }
        } else if mode != Mode::Down && listening.listener.is_none() {
            listening.listener = Some(Listener::bind(listening.port, self.shared.clone())?);
        }
        listening.mode = mode;
        Ok(())
    }

    /// A token pair for `profile` with every scope, as if the app had signed
    /// in: for tests that don't test signing in, and the simulator's
    /// `--fixtures`.
    pub fn sign_in(&self, profile: i64) -> Tokens {
        assert!(lock(&self.shared.state).household.profile(profile).is_some(), "no profile {profile}");
        let now = self.shared.now_ms();
        lock(&self.shared.state).oauth.grant_for(profile, ALL_SCOPES, now)
    }

    /// An access token for `profile` with every scope.
    pub fn access_token(&self, profile: i64) -> String {
        self.sign_in(profile).access_token
    }

    /// The profile sign-in codes sign in as ([`ids::ALEX`] at first).
    pub fn set_sign_in_as(&self, profile: i64) {
        assert!(lock(&self.shared.state).household.profile(profile).is_some(), "no profile {profile}");
        lock(&self.shared.state).oauth.sign_in_as = profile;
    }

    pub fn set_device_approval(&self, approval: DeviceApproval) {
        lock(&self.shared.state).oauth.approval = approval;
    }

    /// How long the person takes to approve the next codes.
    pub fn set_device_delay(&self, delay: Duration) {
        lock(&self.shared.state).oauth.delay_ms = delay.as_millis() as i64;
    }

    /// See [`Options::adopt_unknown_tokens`].
    pub fn set_adopt_unknown_tokens(&self, on: bool) {
        lock(&self.shared.state).oauth.adopt_unknown_tokens = on;
    }

    /// Every access token refused with `invalid_token` while refreshes still
    /// answer: a server whose token check no longer takes this sign-in.
    pub fn set_refuse_access_tokens(&self, on: bool) {
        lock(&self.shared.state).oauth.refuse_access_tokens = on;
    }

    /// Ends every access token now, as an hour passing would; refresh tokens
    /// still work.
    pub fn expire_access_tokens(&self) {
        let now = self.shared.now_ms();
        lock(&self.shared.state).oauth.expire_access(now);
    }

    /// Off, subtitle search answers `503` "not set up".
    pub fn set_subtitle_search(&self, on: bool) {
        lock(&self.shared.state).subtitle_search = on;
    }

    /// The server requires two-step sign-in (`two_step_required` in `GET /api/server`).
    pub fn set_two_step_required(&self, on: bool) {
        lock(&self.shared.state).two_step_required = on;
    }

    /// The apps play from another network than the server's: a speed test is
    /// due when nothing is known of theirs.
    pub fn set_network_remote(&self, on: bool) {
        lock(&self.shared.state).network.remote = on;
    }

    /// Off, GET and PUT /api/playback/network answer as an older server's
    /// do (404, 405).
    pub fn set_network_routes(&self, on: bool) {
        lock(&self.shared.state).network.available = on;
    }

    /// Off, requests are off on the server: the summary says so and every other request route
    /// answers `503 unavailable`.
    pub fn set_requests(&self, on: bool) {
        lock(&self.shared.state).requests.enabled = on;
    }

    /// What asking does for `profile`: `auto`, `ask` or `off`.
    pub fn set_request_access(&self, profile: i64, access: &'static str) {
        lock(&self.shared.state).requests.set_access(profile, access);
    }

    /// The download of request `id`: how far it is (0 to 1), or none (searching).
    pub fn set_request_download(&self, id: i64, progress: Option<f64>) {
        let now = self.shared.now();
        lock(&self.shared.state).requests.set_download(id, progress, now);
    }

    /// Sends `event` to every open event stream.
    pub fn emit(&self, event: ServerEvent) {
        self.shared.events.emit(&event);
    }

    /// Sends a raw `data:` line (JSON) to every open event stream: an event
    /// type the app doesn't know, say.
    pub fn emit_raw(&self, json: &str) {
        self.shared.events.emit_text(json.to_string(), None);
    }

    /// `profile`'s own sessions, as `GET /api/sessions` lists them: what its players obey, the
    /// tracks they play, and their ids for [`ServerEvent::PlayerCommand`].
    pub fn sessions_of(&self, profile: i64) -> Vec<serde_json::Value> {
        play::listed(&self.shared.catalog, &lock(&self.shared.state), profile, false)
    }

    /// Event streams open now: tests wait for one before sending.
    pub fn event_listeners(&self) -> usize {
        self.shared.events.listeners()
    }

    /// `profile`'s devices that take casts, as `GET /api/remote/devices` lists them: their
    /// `receiver_id` is where [`FixtureServer::cast`] sends.
    pub fn receivers_of(&self, profile: i64) -> Vec<serde_json::Value> {
        self.shared.events.receivers(profile).iter().map(|r| r.to_json()).collect()
    }

    /// What `profile`'s remote on the device of User-Agent `agent` casting `body` to `receiver`
    /// does (`POST /api/remote/devices/{receiver}/cast`, docs/API.md "Casting"): its status, `202`
    /// when the profile's streams were sent `cast`.
    pub fn cast(&self, profile: i64, receiver: &str, body: &CastBody, agent: Option<&str>) -> u16 {
        let mut state = lock(&self.shared.state);
        match cast::send(&self.shared, &mut state, profile, receiver, &body.to_json(), agent) {
            Ok(answer) | Err(answer) => answer.status,
        }
    }

    /// Every write so far.
    pub fn journal(&self) -> Vec<JournalEntry> {
        lock(&self.shared.journal).clone()
    }

    /// The writes of `method` to `path` ("/api/items/1/progress").
    pub fn writes(&self, method: &str, path: &str) -> Vec<JournalEntry> {
        self.journal().into_iter().filter(|e| e.method == method && e.path == path).collect()
    }

    pub fn clear_journal(&self) {
        lock(&self.shared.journal).clear();
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop();
    }
}

impl fmt::Debug for FixtureServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixtureServer").field("url", &self.url).finish()
    }
}
