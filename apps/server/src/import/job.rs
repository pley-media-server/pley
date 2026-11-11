//! The Plex import as it runs: signing in to Plex, choosing its server,
//! the plan (each Plex library paired with Pley's, each Home user with the
//! limits they'd get), and the job that brings it all over.
//!
//! The Plex token is kept in memory only, for as long as the import needs
//! it: it's forgotten when the administrator cancels or the server stops.
//! One import runs at a time.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::accounts::{self, MAX_LABEL_RULES, NO_PASSWORD};
use crate::db::Db;
use crate::events::{ScanPhase, ServerEvent};
use crate::library::scanner::ScanStatus;
use crate::library::{LibraryKind, get_library};
use crate::state::AppState;
use crate::util::{Pace, now, sort_title};

use super::matching::{Folder, Index, Matches, folder_for, map_path, named};
use futures::StreamExt;

use super::plex::{HomeUser, Item, Plex, PlexError, Resource, Section, Server, UserFilters};
use super::restrictions::{self, Limits};

/// Where the profiles an import made are remembered (`import_accounts`).
const SOURCE: &str = "plex";
/// Titles' own pages read at a time, for their locked fields.
const PAGES_AT_ONCE: usize = 4;
/// The label Kometa puts on a title once it has drawn its overlay on the
/// title's poster.
const KOMETA_OVERLAY: &str = "Overlay";
/// Longest title, sort title, tagline, studio or genre brought, in characters.
const MAX_TITLE: usize = 500;
/// A person's name from Plex, as a profile's display name.
const MAX_NAME: usize = 100;
/// Longest summary brought, in characters.
const MAX_SUMMARY: usize = 20_000;
/// Most genres a title brings.
const MAX_GENRES: usize = 30;
/// The settings key of the last import's report.
const REPORT_KEY: &str = "plex_import_report";
/// When the import running started, kept until its report is: found with
/// none running, the server restarted while it ran.
const RUNNING_KEY: &str = "plex_import_running";
/// The settings key of the id plex.tv knows this server by.
const CLIENT_ID_KEY: &str = "plex_client_id";
/// Files of a Plex library looked for in Pley's, to pair the libraries.
const SAMPLE: usize = 12;
/// The places in a library the plan's sample is taken from, spread over
/// it: the first titles alone may all be in one of its folders.
const SAMPLE_SPOTS: usize = 4;
/// Titles named in the report for each library, of those not found.
const MISSED_NAMED: usize = 50;

/// The import's state, for the server's life.
pub struct Importer {
    tv: String,
    session: tokio::sync::Mutex<Session>,
    progress: parking_lot::Mutex<Progress>,
    cancel: AtomicBool,
}

/// What the administrator has chosen so far.
#[derive(Default)]
struct Session {
    plex: Option<Plex>,
    pin: Option<i64>,
    token: Option<String>,
    servers: Vec<Resource>,
    server: Option<Server>,
    /// The server's name, as Plex shows it.
    server_name: String,
    /// The plan's Home users' pictures, as Pley keeps a profile's, by
    /// their Plex id.
    pictures: HashMap<i64, Vec<u8>>,
    /// The files the plan sampled from each library, by its key, with their
    /// sizes: a folder given for it is checked against them.
    samples: HashMap<String, Vec<(String, Option<i64>)>>,
}

impl Session {
    /// Forgets the account's token and what was chosen with it.
    fn signed_out(&mut self) {
        self.token = None;
        self.servers.clear();
        self.server = None;
        self.server_name.clear();
        self.pictures.clear();
        self.samples.clear();
    }
}

/// Where the import is, as `GET /api/import/plex` says it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    pub state: RunState,
    /// When the import last started (Unix seconds), telling one import from another.
    pub started_at: i64,
    pub step: Option<Step>,
    pub done: u64,
    pub total: u64,
    /// About how many seconds the step has left, where that can be told
    /// from how fast it goes: fetching the libraries' details.
    pub left_secs: Option<u64>,
    /// Why it stopped, when it failed.
    pub error: Option<String>,
    pub report: Option<Report>,
    /// What was found by file has come over: watched titles, resume points,
    /// labels and the rest. Metadata and the titles found by id follow.
    pub first_pass_done: bool,
    /// The library being scanned, while the import waits for the scans.
    pub scan: Option<ScanNow>,
    /// Each library the import brings, while it waits for their scans and
    /// finds Plex's titles in them; kept once that's done.
    pub libraries: Vec<LibraryRow>,
    /// The steps it goes through, in order, as they stand.
    pub steps: Vec<StepRow>,
}

/// One of the steps an import goes through, as it stands.
#[derive(Debug, Clone, Serialize)]
pub struct StepRow {
    pub step: Step,
    pub state: StepState,
    /// What it brought, once done, where there's a count to give.
    pub count: Option<u64>,
    /// When it began, and how long it took once done: for the report.
    #[serde(skip)]
    began: Option<Instant>,
    #[serde(skip)]
    took: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    ToCome,
    Running,
    Done,
}

/// The steps an import of what `bring` says goes through, in order:
/// finding Plex's titles is part of the scans (each library is matched as
/// its scan ends), and the second round, for the titles found by their ids
/// after the fetch, is part of finding the rest.
fn steps(bring: Bring) -> Vec<StepRow> {
    let mut steps = vec![Step::Profiles, Step::Scanning, Step::Labels];
    if bring.edits {
        steps.push(Step::Edits);
    }
    if bring.collections {
        steps.push(Step::Collections);
    }
    if bring.watch_history {
        steps.extend([Step::WatchState, Step::History]);
    }
    if bring.collections {
        steps.push(Step::Playlists);
    }
    steps.extend([Step::Fetching, Step::MatchingAgain]);
    steps
        .into_iter()
        .map(|step| StepRow { step, state: StepState::ToCome, count: None, began: None, took: None })
        .collect()
}

/// What step `step` brought, from the report so far.
/// How long each of `steps` that ran took, the one running until now.
fn step_times(steps: &[StepRow]) -> Vec<StepTime> {
    steps
        .iter()
        .filter_map(|row| {
            let took = match row.state {
                StepState::Done => row.took?,
                StepState::Running => row.began?.elapsed(),
                StepState::ToCome => return None,
            };
            Some(StepTime { step: row.step, secs: took.as_secs_f64().round() as u64 })
        })
        .collect()
}

fn step_count(report: &Report, step: Step) -> Option<u64> {
    let profiles = || report.profiles.iter().filter(|p| p.user_id.is_some() && p.skipped.is_none());
    match step {
        Step::Profiles => Some(profiles().count() as u64),
        Step::Scanning | Step::Matching => Some(report.libraries.iter().map(|l| l.matched).sum()),
        Step::Labels => Some(report.labels_added),
        Step::Edits => Some(report.edits.values().sum()),
        Step::Collections => Some(report.collections_made),
        Step::WatchState => Some(profiles().map(|p| p.watched).sum()),
        Step::History => Some(profiles().map(|p| p.plays).sum()),
        Step::Playlists => Some(profiles().map(|p| p.playlists).sum()),
        Step::Fetching | Step::MatchingAgain => None,
    }
}

/// A library's scan the import waits for: its files found, or read of
/// those found (`done` of `total`).
#[derive(Debug, Clone, Serialize)]
pub struct ScanNow {
    pub library: String,
    /// `queued`, `walking` or `probing`.
    pub phase: ScanPhase,
    pub done: u64,
    pub total: u64,
}

/// A library the import brings, during the scanning step.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryRow {
    pub library: String,
    pub phase: LibraryPhase,
    /// Its files read of those found, while they're read.
    pub done: u64,
    pub total: u64,
    /// About how many seconds its files have left to be read, once that
    /// can be told from how fast they've been read.
    pub left_secs: Option<u64>,
    /// Plex's titles found in it, once looked for.
    pub found: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryPhase {
    /// Its scan waits for another's.
    Queued,
    /// Its files are found.
    Walking,
    /// Its files are read.
    Probing,
    /// Plex's titles are looked for in it.
    Matching,
    Done,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Idle,
    Running,
    Done,
    Failed,
    Cancelled,
    /// The server restarted while it ran: it stopped, with no report.
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Scanning,
    Profiles,
    Matching,
    Labels,
    Edits,
    Collections,
    WatchState,
    History,
    Playlists,
    /// The libraries' metadata, fetched once Plex's matches are in.
    Fetching,
    /// Plex's titles not found by their files, looked for by their ids.
    MatchingAgain,
}

/// What an import brought over, and what it couldn't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    /// When it started (Unix seconds), with `finished_at` how long it took;
    /// 0 in reports from before it was kept.
    #[serde(default)]
    pub started_at: i64,
    pub finished_at: i64,
    /// How it ended: done, failed or cancelled.
    #[serde(default)]
    pub outcome: RunState,
    /// Why it failed.
    #[serde(default)]
    pub error: Option<String>,
    pub libraries: Vec<LibraryReport>,
    pub profiles: Vec<ProfileReport>,
    /// Labels put on titles.
    pub labels_added: u64,
    /// Titles whose date added moved to Plex's.
    pub dates_moved: u64,
    /// Titles matched again to the TMDB entry Plex has, as Fix Match does.
    #[serde(default)]
    pub matches_fixed: u64,
    /// Plex's locked fields brought as edits, by Pley's name for the field
    /// (`title`, `content_rating`, …): tools such as Kometa lock what they
    /// set, so a field locked on most titles is likely theirs.
    #[serde(default)]
    pub edits: BTreeMap<String, u64>,
    /// Collections made for Plex's.
    #[serde(default)]
    pub collections_made: u64,
    /// Titles put in collections, new or found again.
    #[serde(default)]
    pub collection_items: u64,
    /// Smart collections and playlists, which aren't brought: Plex's
    /// filters don't map to Pley's.
    #[serde(default)]
    pub smart_left: u64,
    /// What the administrator chose to bring: what wasn't brought isn't
    /// counted as nothing found.
    #[serde(default)]
    pub bring: Bring,
    /// How long each step took, in the order they ran; the one it stopped
    /// in, until it stopped.
    #[serde(default)]
    pub steps: Vec<StepTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTime {
    pub step: Step,
    pub secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryReport {
    pub title: String,
    /// The Pley library it went to; `None`: left out.
    pub library_id: Option<i64>,
    pub created: bool,
    pub matched: u64,
    pub missed: u64,
    /// Some of the titles not found, at most [`MISSED_NAMED`].
    pub missed_titles: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileReport {
    pub title: String,
    pub user_id: Option<i64>,
    pub created: bool,
    /// Why nothing came over for them: `pin_needed`, `refused` or
    /// `no_access` (Plex lists no access to the server for them).
    pub skipped: Option<String>,
    pub max_age: Option<i64>,
    pub show_labels: Vec<String>,
    pub hide_labels: Vec<String>,
    /// Labels past the most a profile keeps.
    pub left_over: Vec<String>,
    /// Parts of Plex's rules Pley has no way to keep.
    pub not_kept: Vec<String>,
    pub watched: u64,
    pub resumed: u64,
    pub rated: u64,
    pub plays: u64,
    /// Their playlists brought, made or added to.
    #[serde(default)]
    pub playlists: u64,
    /// Plex refused some of their lists: what came over is incomplete.
    #[serde(default)]
    pub incomplete: bool,
    /// Plex refused some of their playlists.
    #[serde(default)]
    pub playlists_incomplete: bool,
}

// ---- The plan ----

/// A Plex server the account has.
#[derive(Debug, Serialize)]
pub struct ServerChoice {
    pub id: String,
    pub name: String,
    pub owned: bool,
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub server: String,
    pub libraries: Vec<PlanLibrary>,
    pub users: Vec<PlanUser>,
}

#[derive(Debug, Serialize)]
pub struct PlanLibrary {
    pub key: String,
    pub title: String,
    /// Pley's kind: `movies`, `shows` or `music`.
    pub kind: &'static str,
    /// Plex's folders.
    pub locations: Vec<String>,
    /// How much it holds: movies, episodes or tracks.
    pub items: u64,
    /// Its shows or artists; none for movies.
    pub groups: Option<u64>,
    pub pairing: Pairing,
}

/// Whether one of Plex's folders is where it's said to be for Pley.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderCheck {
    /// One of the files the plan sampled from it is there, the same size.
    Found,
    /// None of them is.
    NoneFound,
    /// Pley can't open it.
    Missing,
    /// No file was sampled from it to tell.
    Untold,
}

/// How a Plex library pairs with Pley's.
#[derive(Debug, Serialize)]
pub struct Pairing {
    /// `matched`: its files are a Pley library's; `unsure`: some are, or in
    /// more than one; `not_in_pley`: none are.
    pub status: &'static str,
    pub library_id: Option<i64>,
    /// Where its folders are for Pley, as read off the files found.
    pub folders: Vec<Folder>,
    /// Plex's folders this server can open, for a library made from them.
    pub folders_here: Vec<String>,
    /// Where Plex's other folders seem to be for Pley: found by their path's
    /// end (`/media/movies` for `/data/media/movies`) and one of the files
    /// sampled from them (`crate::folders::find`).
    pub folders_found: Vec<Folder>,
}

#[derive(Debug, Serialize)]
pub struct PlanUser {
    pub id: i64,
    pub title: String,
    /// The Plex account signed in, brought as the administrator.
    pub admin: bool,
    /// Plex limits what they see: their sharing filters have a rule (a
    /// managed user without one sees everything, as on Plex).
    pub restricted: bool,
    pub protected: bool,
    /// The Pley profile an earlier import made for them.
    pub profile_id: Option<i64>,
    pub limits: PlanLimits,
    /// Their picture can be had, from `GET /api/import/plex/users/{id}/picture`.
    pub picture: bool,
    /// The colour of Plex's picture for them, which their profile takes.
    pub color: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct PlanLimits {
    pub max_age: Option<i64>,
    pub max_age_system: Option<String>,
    pub show_labels: Vec<String>,
    pub hide_labels: Vec<String>,
    pub left_over: Vec<String>,
    pub not_kept: Vec<String>,
}

impl From<Limits> for PlanLimits {
    fn from(l: Limits) -> Self {
        PlanLimits {
            max_age: l.max_age,
            max_age_system: l.system,
            show_labels: l.show,
            hide_labels: l.hide,
            left_over: l.left_over,
            not_kept: l.not_kept,
        }
    }
}

/// What the administrator chose to bring.
#[derive(Debug, Deserialize)]
pub struct Choices {
    /// The Plex libraries to bring; those not listed are left out.
    pub libraries: Vec<LibraryChoice>,
    /// Where Plex's folders are for Pley.
    #[serde(default)]
    pub folders: Vec<FolderChoice>,
    /// The Home users to bring (the Plex account always comes).
    #[serde(default)]
    pub users: Vec<UserChoice>,
    /// What comes over besides the libraries and people; all of it when
    /// left out.
    #[serde(default)]
    pub bring: Bring,
}

/// The import's "What to bring" boxes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Bring {
    /// Watched, resume points, ratings and plays.
    pub watch_history: bool,
    /// Collections that aren't smart, and everyone's playlists.
    pub collections: bool,
    /// Plex's matches as Fix Match, and its locked fields as edits.
    pub edits: bool,
}

impl Default for Bring {
    fn default() -> Self {
        Self { watch_history: true, collections: true, edits: true }
    }
}

#[derive(Debug, Deserialize)]
pub struct LibraryChoice {
    pub key: String,
    /// The Pley library it goes to, or `new` for one made for it.
    pub library_id: Option<i64>,
    pub new: Option<NewLibrary>,
}

#[derive(Debug, Deserialize)]
pub struct NewLibrary {
    pub name: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FolderChoice {
    pub plex: String,
    pub pley: String,
}

#[derive(Debug, Deserialize)]
pub struct UserChoice {
    pub id: i64,
    /// Their Plex PIN, when Plex asked for it.
    pub pin: Option<String>,
}

/// What went wrong, for the routes to answer with.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("sign in to Plex first")]
    NotSignedIn,
    #[error("choose a Plex server first")]
    NoServer,
    #[error("the sign-in expired: start again")]
    PinExpired,
    #[error("an import is running")]
    Running,
    #[error("only a server the Plex account owns can be imported from")]
    NotOwned,
    #[error("cancelled")]
    Cancelled,
    /// Plex lists no access to the server for a Home user.
    #[error("no access to the server")]
    NoAccess,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Plex(#[from] PlexError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub type ImportResult<T> = Result<T, ImportError>;

impl Importer {
    pub fn new(tv: &str) -> Self {
        Self {
            tv: tv.to_owned(),
            session: Default::default(),
            progress: Default::default(),
            cancel: AtomicBool::new(false),
        }
    }

    /// The client, made with the id plex.tv knows this server by (made
    /// once, then kept in the settings).
    async fn plex(&self, db: &Db, session: &mut Session) -> ImportResult<Plex> {
        if let Some(plex) = &session.plex {
            return Ok(plex.clone());
        }
        let id: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(CLIENT_ID_KEY)
            .fetch_optional(&db.read)
            .await?;
        let id = match id {
            Some(id) => id,
            None => {
                let id = format!("pley-{}", uuid::Uuid::new_v4());
                sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
                    .bind(CLIENT_ID_KEY)
                    .bind(&id)
                    .execute(&db.write)
                    .await?;
                id
            }
        };
        let plex = Plex::new(&self.tv, &id);
        session.plex = Some(plex.clone());
        Ok(plex)
    }

    // ---- Signing in ----

    /// Starts Plex's sign-in: the code, and Plex's page to allow Pley on.
    pub async fn start_sign_in(&self, db: &Db, forward: Option<&str>) -> ImportResult<(String, String)> {
        let mut session = self.session.lock().await;
        let plex = self.plex(db, &mut session).await?;
        let pin = plex.create_pin().await?;
        session.pin = Some(pin.id);
        session.signed_out();
        Ok((pin.code.clone(), plex.auth_url(&pin.code, forward)))
    }

    /// Whether the person has allowed Pley on Plex's page yet.
    pub async fn signed_in(&self, db: &Db) -> ImportResult<bool> {
        let mut session = self.session.lock().await;
        if session.token.is_some() {
            return Ok(true);
        }
        let Some(pin) = session.pin else { return Err(ImportError::NotSignedIn) };
        let plex = self.plex(db, &mut session).await?;
        match plex.pin_token(pin).await {
            Ok(Some(token)) => {
                session.signed_out();
                session.token = Some(token);
                session.pin = None;
                Ok(true)
            }
            Ok(None) => Ok(false),
            // Said once: plex.tv isn't asked about it again.
            Err(PlexError::Refused(404)) => {
                session.pin = None;
                Err(ImportError::PinExpired)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Signs in with a token the administrator pasted.
    pub async fn use_token(&self, db: &Db, token: &str) -> ImportResult<()> {
        let token = token.trim();
        if token.is_empty() || token.len() > 256 || !token.chars().all(|c| c.is_ascii_graphic()) {
            return Err(ImportError::Invalid("that isn't a Plex token".into()));
        }
        let mut session = self.session.lock().await;
        let plex = self.plex(db, &mut session).await?;
        // A token Plex takes lists the account's servers.
        let servers = plex.servers(token).await?;
        session.signed_out();
        session.servers = servers;
        session.token = Some(token.to_owned());
        session.pin = None;
        Ok(())
    }

    /// Forgets the token and what was chosen, and stops a running import.
    pub async fn forget(&self) {
        let mut session = self.session.lock().await;
        // Under the lock, so a start in progress either sees it or starts
        // before it and is stopped by it.
        self.cancel.store(true, Ordering::Relaxed);
        let plex = session.plex.take();
        *session = Session { plex, ..Default::default() };
    }

    // ---- The server ----

    pub async fn servers(&self, db: &Db) -> ImportResult<Vec<ServerChoice>> {
        let mut session = self.session.lock().await;
        let token = session.token.clone().ok_or(ImportError::NotSignedIn)?;
        let plex = self.plex(db, &mut session).await?;
        session.servers = plex.servers(&token).await?;
        // Those shared with it too, after, for the web app to say whose
        // account to sign in with instead.
        let mut servers: Vec<&Resource> = session.servers.iter().collect();
        servers.sort_by_key(|s| !s.owned);
        Ok(servers
            .into_iter()
            .map(|s| ServerChoice {
                id: s.client_identifier.clone(),
                name: s.name.clone(),
                owned: s.owned,
                version: s.product_version.clone(),
            })
            .collect())
    }

    /// Connects to the account's server `id`, or to the server at `address`.
    pub async fn choose_server(&self, db: &Db, id: Option<&str>, address: Option<&str>) -> ImportResult<String> {
        let mut session = self.session.lock().await;
        let token = session.token.clone().ok_or(ImportError::NotSignedIn)?;
        let plex = self.plex(db, &mut session).await?;
        let server = match (id, address) {
            (_, Some(address)) => {
                let server = plex.server_at(address, &token).await?;
                // The account's token opens a server shared with it too:
                // only one plex.tv lists as the account's own is imported.
                if session.servers.is_empty() {
                    session.servers = plex.servers(&token).await?;
                }
                if !session.servers.iter().any(|s| s.client_identifier == server.machine_id && s.owned) {
                    return Err(ImportError::NotOwned);
                }
                server
            }
            (Some(id), None) => {
                if session.servers.is_empty() {
                    session.servers = plex.servers(&token).await?;
                }
                let resource = session
                    .servers
                    .iter()
                    .find(|s| s.client_identifier == id)
                    .ok_or_else(|| ImportError::Invalid("the account has no such server".into()))?;
                // Only the owner reads everyone's history and the Home's
                // restrictions on it.
                if !resource.owned {
                    return Err(ImportError::NotOwned);
                }
                plex.connect(resource).await?
            }
            (None, None) => return Err(ImportError::Invalid("choose a server".into())),
        };
        let name = session
            .servers
            .iter()
            .find(|s| s.client_identifier == server.machine_id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| server.base.host_str().unwrap_or_default().to_owned());
        session.server = Some(server);
        session.server_name = name.clone();
        Ok(name)
    }

    // ---- The plan ----

    /// Each Plex library with the Pley library it pairs with, and each Home
    /// user with the limits they'd get.
    pub async fn plan(&self, db: &Db, data_dir: &std::path::Path) -> ImportResult<Plan> {
        let mut session = self.session.lock().await;
        let token = session.token.clone().ok_or(ImportError::NotSignedIn)?;
        let server = session.server.clone().ok_or(ImportError::NoServer)?;
        let name = session.server_name.clone();
        let plex = self.plex(db, &mut session).await?;
        drop(session);

        let data_dir = data_dir.to_path_buf();
        let bases = tokio::task::spawn_blocking(move || crate::folders::bases(&data_dir)).await.unwrap_or_default();
        let home = plex.home_users(&token).await?;
        // The pictures come from plex.tv while the libraries are paired.
        let pairing = async {
            let (mut libraries, mut samples) = (Vec::new(), HashMap::new());
            for section in plex.sections(&server).await? {
                let Some(kind) = pley_kind(&section.kind) else { continue };
                let locations: Vec<String> = section.location.iter().map(|l| l.path.clone()).collect();
                let (pairing, sample, items) = pair(db, &plex, &server, &section.key, kind, &locations, &bases).await?;
                let groups = match group_type(kind) {
                    Some(kind) => {
                        let query = [("type", kind.to_string())];
                        let path = format!("/library/sections/{}/all", section.key);
                        Some(plex.page_of(&server, &path, &query, 0, 0).await?.total as u64)
                    }
                    None => None,
                };
                samples.insert(section.key.clone(), sample);
                libraries.push(PlanLibrary {
                    key: section.key,
                    title: section.title,
                    kind: kind.as_str(),
                    locations,
                    items: items as u64,
                    groups,
                    pairing,
                });
            }
            Ok::<_, ImportError>((libraries, samples))
        };
        let pictures = futures::future::join_all(home.iter().map(|user| home_picture(&plex, user)));
        let (libraries, pictures) = tokio::join!(pairing, pictures);
        let (libraries, samples) = libraries?;
        let pictures: HashMap<i64, (Option<Vec<u8>>, Option<String>)> = pictures.into_iter().collect();

        // A managed user's rules (a preset's included) and a Home member's
        // with their own account are both sharing filters. Plex calls every
        // managed user restricted, with rules or none, so the filters say.
        let filters = if home.iter().any(|u| !u.admin) { plex.user_filters(&token).await? } else { Vec::new() };
        let region = crate::ratings::region(db).await?;
        let mut users = Vec::new();
        for user in &home {
            let limits = if user.admin { Limits::default() } else { limits_of(&filters, user.id, &region) };
            let restricted = limits != Limits::default();
            users.push(PlanUser {
                id: user.id,
                title: user.title.clone(),
                admin: user.admin,
                restricted,
                protected: user.protected,
                profile_id: if user.admin { None } else { imported_profile(db, user.id).await? },
                limits: limits.into(),
                picture: pictures.get(&user.id).is_some_and(|(jpeg, _)| jpeg.is_some()),
                color: pictures.get(&user.id).and_then(|(_, color)| color.clone()),
            });
        }
        let pictures = pictures.into_iter().filter_map(|(id, (jpeg, _))| Some((id, jpeg?))).collect();
        let mut session = self.session.lock().await;
        session.pictures = pictures;
        session.samples = samples;
        Ok(Plan { server: name, libraries, users })
    }

    /// Whether Plex library `key`'s folder `plex` is `pley` for Pley, told
    /// by the files the last plan sampled from it.
    pub async fn check_folder(&self, key: &str, plex: &str, pley: &str) -> FolderCheck {
        let files = self.session.lock().await.samples.get(key).cloned().unwrap_or_default();
        let folder = Folder { plex: plex.to_owned(), pley: pley.to_owned() };
        tokio::task::spawn_blocking(move || {
            if !std::path::Path::new(&folder.pley).is_dir() {
                return FolderCheck::Missing;
            }
            let there: Vec<(String, Option<i64>)> = files
                .iter()
                .filter_map(|(file, size)| Some((map_path(std::slice::from_ref(&folder), file)?, *size)))
                .collect();
            if there.is_empty() {
                FolderCheck::Untold
            } else if there
                .iter()
                .any(|(path, size)| std::fs::metadata(path).is_ok_and(|m| m.is_file() && Some(m.len() as i64) == *size))
            {
                FolderCheck::Found
            } else {
                FolderCheck::NoneFound
            }
        })
        .await
        .unwrap_or(FolderCheck::Untold)
    }

    /// Home user `id`'s picture, as the last plan found it: a JPEG.
    pub async fn user_picture(&self, id: i64) -> Option<Vec<u8>> {
        self.session.lock().await.pictures.get(&id).cloned()
    }

    // ---- The job ----

    /// Where the import is: the one running, the one the server restarted
    /// during, or the last one's report.
    pub async fn progress(&self, db: &Db) -> Progress {
        let progress = self.progress.lock().clone();
        if progress.state != RunState::Idle {
            return progress;
        }
        let running: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(RUNNING_KEY)
            .fetch_optional(&db.read)
            .await
            .ok()
            .flatten();
        if let Some(started_at) = running.and_then(|at| at.parse().ok()) {
            return Progress { state: RunState::Interrupted, started_at, ..Default::default() };
        }
        let report: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(REPORT_KEY)
            .fetch_optional(&db.read)
            .await
            .ok()
            .flatten();
        match report.and_then(|r| serde_json::from_str::<Report>(&r).ok()) {
            Some(report) => Progress {
                state: report.outcome,
                error: report.error.clone(),
                report: Some(report),
                ..Default::default()
            },
            None => progress,
        }
    }

    /// Starts the import of what `choices` says, for administrator `admin`.
    pub async fn start(self: &Arc<Self>, state: &AppState, admin: i64, choices: Choices) -> ImportResult<()> {
        let mut session = self.session.lock().await;
        let token = session.token.clone().ok_or(ImportError::NotSignedIn)?;
        let server = session.server.clone().ok_or(ImportError::NoServer)?;
        let plex = self.plex(&state.db, &mut session).await?;
        {
            let mut progress = self.progress.lock();
            if progress.state == RunState::Running {
                return Err(ImportError::Running);
            }
            *progress = Progress {
                state: RunState::Running,
                started_at: now(),
                steps: steps(choices.bring),
                ..Default::default()
            };
        }
        let started_at = self.progress.lock().started_at;
        let marked = sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(RUNNING_KEY)
        .bind(started_at.to_string())
        .execute(&state.db.write)
        .await;
        if let Err(err) = marked {
            *self.progress.lock() = Progress::default();
            return Err(err.into());
        }
        self.cancel.store(false, Ordering::Relaxed);
        drop(session);
        let run = Run {
            state: state.clone(),
            importer: self.clone(),
            plex,
            token,
            server,
            admin,
            report: Report::default(),
            again: false,
        };
        tokio::spawn(run.go(choices));
        Ok(())
    }

    fn set(&self, state: &AppState, admin: i64, change: impl FnOnce(&mut Progress)) {
        change(&mut self.progress.lock());
        state.events.send(ServerEvent::ImportChanged { user_id: admin });
    }
}

/// The Pley kind of a Plex library's `type`; photos aren't brought.
fn pley_kind(kind: &str) -> Option<LibraryKind> {
    match kind {
        "movie" => Some(LibraryKind::Movies),
        "show" => Some(LibraryKind::Shows),
        "artist" => Some(LibraryKind::Music),
        _ => None,
    }
}

/// Plex's item types in a library of `kind`, in the order they're matched:
/// a show before its episodes (found by its numbers), tracks before their
/// albums and artists (found through them).
fn types(kind: LibraryKind) -> &'static [(u8, &'static str)] {
    match kind {
        LibraryKind::Movies => &[(1, "movie")],
        LibraryKind::Shows => &[(2, "show"), (4, "episode"), (3, "season")],
        LibraryKind::Music => &[(10, "track"), (9, "album"), (8, "artist")],
    }
}

/// The type whose files a library of `kind` is sampled by.
/// Plex's type for what a library of `kind` groups its items in: shows,
/// artists. Movies aren't grouped.
fn group_type(kind: LibraryKind) -> Option<u8> {
    match kind {
        LibraryKind::Movies => None,
        LibraryKind::Shows => Some(2),
        LibraryKind::Music => Some(8),
    }
}

fn leaf_type(kind: LibraryKind) -> u8 {
    match kind {
        LibraryKind::Movies => 1,
        LibraryKind::Shows => 4,
        LibraryKind::Music => 10,
    }
}

fn limits_of(filters: &[UserFilters], id: i64, region: &str) -> Limits {
    match filters.iter().find(|f| f.id == id) {
        Some(f) => restrictions::limits(&f.movies, &f.television, &f.music, region),
        None => Limits::default(),
    }
}

/// The scans of `paired` that are still finding their files.
fn finding(state: &AppState, paired: &[Paired]) -> Vec<ScanStatus> {
    paired
        .iter()
        .filter_map(|p| state.scanner.status_of(p.library_id).filter(|s| s.phase != ScanPhase::Metadata))
        .collect()
}

async fn imported_profile(db: &Db, plex_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT user_id FROM import_accounts WHERE source = ? AND external_id = ?")
        .bind(SOURCE)
        .bind(plex_id.to_string())
        .fetch_optional(&db.read)
        .await
}

/// Home user `user`'s picture, made as a profile's is (a small JPEG, read
/// and written afresh), and the colour of Plex's picture for them.
async fn home_picture(plex: &Plex, user: &HomeUser) -> (i64, (Option<Vec<u8>>, Option<String>)) {
    let Some(thumb) = user.thumb.as_deref() else { return (user.id, (None, None)) };
    let picture = plex.picture(thumb).await;
    let color = picture.color.filter(|c| accounts::validate_color(c).is_ok());
    let jpeg = match (picture.bytes, crate::images::UPLOAD_DECODES.acquire().await) {
        (Some(bytes), Ok(_decoding)) => {
            tokio::task::spawn_blocking(move || accounts::process_avatar(&bytes)).await.ok().and_then(Result::ok)
        }
        _ => None,
    };
    (user.id, (jpeg, color))
}

/// Pairs Plex library `key` with Pley's: some of its files, from spots
/// spread over it, looked for among Pley's by name and size. Its folders
/// Pley knows neither way are looked for under `bases`. With the files
/// sampled, and how many items (movies, episodes, tracks) it holds.
async fn pair(
    db: &Db,
    plex: &Plex,
    server: &Server,
    key: &str,
    kind: LibraryKind,
    locations: &[String],
    bases: &[std::path::PathBuf],
) -> ImportResult<(Pairing, Vec<(String, Option<i64>)>, usize)> {
    let query = [("type", leaf_type(kind).to_string())];
    let path = format!("/library/sections/{key}/all");
    let each = SAMPLE / SAMPLE_SPOTS;
    let first = plex.page_of(server, &path, &query, 0, each).await?;
    let mut items = first.items;
    for spot in 1..SAMPLE_SPOTS {
        let start = first.total * spot / SAMPLE_SPOTS;
        // A library too small to spread over is all in the first.
        if start < each {
            continue;
        }
        items.extend(plex.page_of(server, &path, &query, start, each).await?.items);
    }
    let files: Vec<(String, Option<i64>)> =
        items.iter().filter_map(|i| i.files().next().map(|(f, s)| (f.to_owned(), s))).collect();
    let mut votes: HashMap<i64, usize> = HashMap::new();
    let mut folders: Vec<Folder> = Vec::new();
    let mut found = 0;
    for (file, size) in &files {
        let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
        let Some(size) = size else { continue };
        // The name's compared here, not in SQL, for its accents may be
        // written another way (see `matching::nfc`).
        let hits: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT f.path, f.library_id, l.kind FROM media_files f JOIN libraries l ON l.id = f.library_id
             WHERE f.size = ?",
        )
        .bind(size)
        .fetch_all(&db.read)
        .await?;
        let hits: Vec<_> = hits.into_iter().filter(|(path, ..)| named(path, name)).collect();
        // A file found more than once (a copy) says nothing for sure.
        let [(path, library, library_kind)] = hits.as_slice() else { continue };
        if library_kind != kind.as_str() {
            continue;
        }
        found += 1;
        *votes.entry(*library).or_default() += 1;
        if let Some(folder) = folder_for(locations, file, path)
            && !folders.contains(&folder)
        {
            folders.push(folder);
        }
    }
    let best = votes.iter().max_by_key(|(_, n)| **n).map(|(id, n)| (*id, *n));
    let status = match best {
        None => "not_in_pley",
        Some((_, n)) if n == files.len() && votes.len() == 1 && found > 0 => "matched",
        Some(_) => "unsure",
    };
    let folders_here = locations.iter().filter(|l| std::path::Path::new(l).is_dir()).cloned().collect::<Vec<String>>();
    let unknown: Vec<String> = locations
        .iter()
        .filter(|l| {
            !folders_here.contains(l) && !folders.iter().any(|f| map_path(std::slice::from_ref(f), l).is_some())
        })
        .cloned()
        .collect();
    let bases = bases.to_vec();
    let sample = files.clone();
    let folders_found = tokio::task::spawn_blocking(move || {
        unknown
            .iter()
            .filter_map(|location| {
                let found = crate::folders::find(location, &bases, |dir| {
                    let folder = Folder { plex: location.clone(), pley: dir.to_string_lossy().into_owned() };
                    // One of its sampled files there, the same size, says it's the one.
                    files.iter().any(|(file, size)| {
                        map_path(std::slice::from_ref(&folder), file)
                            .and_then(|path| std::fs::metadata(path).ok())
                            .is_some_and(|m| Some(m.len() as i64) == *size)
                    })
                })?;
                Some(Folder { plex: location.clone(), pley: found.to_string_lossy().into_owned() })
            })
            .collect()
    })
    .await
    .unwrap_or_default();
    let pairing = Pairing { status, library_id: best.map(|(id, _)| id), folders, folders_here, folders_found };
    Ok((pairing, sample, first.total))
}

/// One import as it runs.
struct Run {
    state: AppState,
    importer: Arc<Importer>,
    plex: Plex,
    /// The Plex account's.
    token: String,
    /// The server, with the account's token for it.
    server: Server,
    admin: i64,
    report: Report,
    /// In the second round, for Plex's titles found by their ids after the
    /// fetch: its steps show as finding them.
    again: bool,
}

/// A library being brought: Plex's, Pley's.
struct Paired {
    key: String,
    kind: LibraryKind,
    library_id: i64,
    report: usize,
}

/// What "Matches and edits" needs of a library's movies, shows and
/// seasons, by Plex's `ratingKey`.
#[derive(Default)]
struct Titles {
    /// Movies and shows with the ids Plex matched them to.
    ids: Vec<PlexIds>,
    /// Those whose own page may have locked fields.
    pages: Vec<String>,
}

/// A movie's or show's ids, as Plex matched it.
struct PlexIds {
    key: String,
    show: bool,
    tmdb: Option<i64>,
    tvdb: Option<i64>,
    imdb: Option<String>,
    title: String,
}

impl PlexIds {
    fn of(item: &Item) -> Option<PlexIds> {
        let number = |source| item.guid(source).and_then(|id| id.parse::<i64>().ok()).filter(|id| *id > 0);
        let imdb = item
            .guid("imdb")
            .filter(|id| {
                id.len() <= 12
                    && id.strip_prefix("tt").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .map(str::to_owned);
        let ids = PlexIds {
            key: item.rating_key.clone(),
            show: item.kind == "show",
            tmdb: number("tmdb"),
            tvdb: number("tvdb"),
            imdb,
            title: item.title.clone(),
        };
        (ids.tmdb.is_some() || ids.tvdb.is_some() || ids.imdb.is_some()).then_some(ids)
    }
}

/// A person being brought: their Plex account, profile and server token.
struct Person {
    /// Plex's history names the account's own plays `1`.
    account_id: i64,
    user_id: i64,
    server: Server,
    /// The Plex libraries they can open.
    sections: HashSet<String>,
    report: usize,
}

impl Run {
    async fn go(mut self, choices: Choices) {
        // Seek previews, intros and fingerprints wait until it's over: they
        // would read the same disks and write the same database as its
        // scans and fetch.
        let hold = self.state.schedule.hold();
        let outcome = self.run(choices).await;
        drop(hold);
        let cancelled = matches!(outcome, Err(ImportError::Cancelled)) || self.importer.cancel.load(Ordering::Relaxed);
        let state = match (&outcome, cancelled) {
            (_, true) => RunState::Cancelled,
            (Ok(()), _) => RunState::Done,
            (Err(_), _) => RunState::Failed,
        };
        let error = match &outcome {
            Err(e) if !cancelled => Some(e.to_string()),
            _ => None,
        };
        if let Some(e) = &error {
            tracing::warn!("the Plex import stopped: {e}");
        }
        self.report.finished_at = now();
        (self.report.started_at, self.report.steps) = {
            let progress = self.importer.progress.lock();
            (progress.started_at, step_times(&progress.steps))
        };
        self.report.outcome = state;
        self.report.error = error.clone();
        let report = self.report.clone();
        // Kept however it ended, so the administrator learns what came over.
        if let Ok(json) = serde_json::to_string(&report) {
            let _ = sqlx::query(
                "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(REPORT_KEY)
            .bind(json)
            .execute(&self.state.db.write)
            .await;
        }
        let _ = sqlx::query("DELETE FROM settings WHERE key = ?").bind(RUNNING_KEY).execute(&self.state.db.write).await;
        self.importer.set(&self.state, self.admin, |p| {
            p.state = state;
            p.error = error;
            p.step = None;
            p.scan = None;
            p.report = Some(report);
        });
        self.state.events.send(ServerEvent::LibrariesChanged);
    }

    fn cancelled(&self) -> ImportResult<()> {
        if self.importer.cancel.load(Ordering::Relaxed) {
            return Err(ImportError::Cancelled);
        }
        Ok(())
    }

    /// Starts step `step`, of `total` things to do: its row runs, and the
    /// rows before it are done, with what they brought.
    fn step(&self, step: Step, total: u64) {
        let row = match step {
            _ if self.again => Step::MatchingAgain,
            Step::Matching => Step::Scanning,
            step => step,
        };
        self.importer.set(&self.state, self.admin, |p| {
            p.step = Some(step);
            p.done = 0;
            p.total = total;
            p.left_secs = None;
            p.scan = None;
            if let Some(at) = p.steps.iter().position(|r| r.step == row) {
                for before in p.steps[..at].iter_mut().filter(|r| r.state != StepState::Done) {
                    before.state = StepState::Done;
                    before.count = step_count(&self.report, before.step);
                    before.took = before.began.map(|began| began.elapsed());
                }
                p.steps[at].state = StepState::Running;
                p.steps[at].began.get_or_insert_with(Instant::now);
            }
        });
    }

    fn advance(&self, by: u64) {
        self.importer.set(&self.state, self.admin, |p| p.done += by);
    }

    async fn run(&mut self, choices: Choices) -> ImportResult<()> {
        let folders: Vec<Folder> =
            choices.folders.iter().map(|f| Folder { plex: f.plex.clone(), pley: f.pley.clone() }).collect();
        let sections = self.plex.sections(&self.server).await?;
        self.check(&choices, &sections).await?;
        let paired = self.libraries(&choices, &sections).await?;
        // Profiles come with their limits before anything else, so a
        // child's is never open while the rest runs, or if it stops.
        let people = self.people(&choices, &sections, &paired).await?;
        let bring = choices.bring;
        self.report.bring = bring;

        // First what's found by file, before any metadata: the libraries the
        // import made are scanned for their files only, and Plex's matches
        // go on them, so the fetch looks titles up by Plex's ids rather than
        // searching by their names.
        let (matches, titles) = self.match_as_scanned(&paired, &folders).await?;
        let pley_libraries: Vec<i64> = paired.iter().map(|p| p.library_id).collect();
        let index = Index::load(&self.state.db, &pley_libraries).await?;
        self.bring(bring, &paired, &people, &titles, &index, &matches).await?;
        self.importer.set(&self.state, self.admin, |p| p.first_pass_done = true);
        for person in &people {
            self.state.events.send(ServerEvent::OwnLibrariesChanged { user_id: person.user_id });
        }

        // Then the metadata, and Plex's titles found only by their ids,
        // which the fetch gives Pley's.
        let all_found = self.report.libraries.iter().all(|l| l.missed == 0);
        if all_found {
            self.importer.set(&self.state, self.admin, |p| p.steps.retain(|r| r.step != Step::MatchingAgain));
        }
        self.fetch(&paired).await?;
        if all_found {
            return Ok(());
        }
        self.again = true;
        self.step(Step::MatchingAgain, paired.len() as u64);
        let index = Index::load(&self.state.db, &pley_libraries).await?;
        let mut again = matches.clone();
        let mut titles = Vec::new();
        for library in &paired {
            titles.push(self.find(library, &index, &folders, &mut again).await?);
            self.advance(1);
        }
        let found = again.since(&matches);
        if !found.is_empty() {
            self.bring(bring, &paired, &people, &titles, &index, &found).await?;
            for person in &people {
                self.state.events.send(ServerEvent::OwnLibrariesChanged { user_id: person.user_id });
            }
        }
        Ok(())
    }

    /// What the import brings for the titles in `matches`, as `bring` says.
    async fn bring(
        &mut self,
        bring: Bring,
        paired: &[Paired],
        people: &[Person],
        titles: &[Titles],
        index: &Index,
        matches: &Matches,
    ) -> ImportResult<()> {
        self.step(Step::Labels, paired.len() as u64);
        for library in paired {
            self.labels(library, matches).await?;
            self.advance(1);
        }

        if bring.edits {
            let pages = titles.iter().flat_map(|t| &t.pages).filter(|k| matches.get(k).is_some()).count();
            self.step(Step::Edits, pages as u64);
            for t in titles {
                self.edits(t, index, matches).await?;
            }
        }

        if bring.collections {
            self.step(Step::Collections, paired.len() as u64);
            for library in paired {
                self.collections(library, matches).await?;
                self.advance(1);
            }
        }

        if bring.watch_history {
            self.step(Step::WatchState, people.len() as u64);
            for person in people {
                let done = self.watch_state(person, paired, matches).await;
                self.person_done(person, done, false)?;
                self.state.events.send(ServerEvent::OwnLibrariesChanged { user_id: person.user_id });
                self.advance(1);
            }

            self.step(Step::History, people.len() as u64);
            let devices: HashMap<i64, String> =
                self.plex.devices(&self.server).await.unwrap_or_default().into_iter().map(|d| (d.id, d.name)).collect();
            for person in people {
                let done = self.history(person, matches, &devices).await;
                self.person_done(person, done, false)?;
                self.advance(1);
            }
        }

        if bring.collections {
            self.step(Step::Playlists, people.len() as u64);
            for person in people {
                let done = self.playlists(person, matches).await;
                self.person_done(person, done, true)?;
                self.advance(1);
            }
        }
        Ok(())
    }

    /// Plex refusing one person's lists leaves theirs incomplete, not
    /// everyone's; a cancel or Pley's own error stops the import. Their
    /// playlists are said apart from what they watched.
    fn person_done(&mut self, person: &Person, done: ImportResult<()>, playlists: bool) -> ImportResult<()> {
        match done {
            Ok(()) => Ok(()),
            Err(ImportError::Plex(e)) => {
                tracing::debug!("the Plex import couldn't read all of one person's lists: {e}");
                let profile = &mut self.report.profiles[person.report];
                if playlists {
                    profile.playlists_incomplete = true;
                } else {
                    profile.incomplete = true;
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Checks every choice before anything is made: the libraries paired
    /// are of the right kind, and those to make have a name and folders.
    async fn check(&self, choices: &Choices, sections: &[Section]) -> ImportResult<()> {
        for choice in &choices.libraries {
            let Some(section) = sections.iter().find(|s| s.key == choice.key) else { continue };
            let Some(kind) = pley_kind(&section.kind) else { continue };
            if let Some(new) = &choice.new {
                let name = new.name.trim();
                if name.is_empty() || name.chars().count() > 100 {
                    return Err(ImportError::Invalid("library name must be 1-100 characters".into()));
                }
                crate::api::validate_library_paths(&new.paths).map_err(|e| ImportError::Invalid(e.to_string()))?;
            } else if let Some(id) = choice.library_id {
                let pley_kind: Option<String> = sqlx::query_scalar("SELECT kind FROM libraries WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&self.state.db.read)
                    .await?;
                if pley_kind.as_deref() != Some(kind.as_str()) {
                    return Err(ImportError::Invalid(format!("library {id} isn't one of {}", kind.as_str())));
                }
            }
        }
        Ok(())
    }

    /// Pairs each chosen Plex library with a Pley library, making those
    /// asked for.
    async fn libraries(&mut self, choices: &Choices, sections: &[Section]) -> ImportResult<Vec<Paired>> {
        let mut paired = Vec::new();
        for section in sections {
            let Some(kind) = pley_kind(&section.kind) else { continue };
            let choice = choices.libraries.iter().find(|c| c.key == section.key);
            let mut report = LibraryReport { title: section.title.clone(), ..Default::default() };
            let library_id = match choice {
                Some(LibraryChoice { new: Some(new), .. }) => {
                    let id = crate::api::add_library_files_first(&self.state, &new.name, kind, &new.paths)
                        .await
                        .map_err(|e| ImportError::Invalid(e.to_string()))?;
                    report.created = true;
                    Some(id)
                }
                Some(LibraryChoice { library_id: Some(id), .. }) => Some(*id),
                _ => None,
            };
            report.library_id = library_id;
            self.report.libraries.push(report);
            if let Some(library_id) = library_id {
                let report = self.report.libraries.len() - 1;
                paired.push(Paired { key: section.key.clone(), kind, library_id, report });
            }
        }
        Ok(paired)
    }

    /// Matches each library with Plex's once its files are in: what's
    /// matched is what scans find. A scan of a library the import made finds
    /// its files only; one already running is waited for until its files are
    /// written, as it starts on its metadata. A library is matched while the
    /// others are still scanned, so that its titles show Plex's names and
    /// dates added as soon as they can; those left once nothing is scanned
    /// are matched as a step of their own.
    async fn match_as_scanned(
        &mut self,
        paired: &[Paired],
        folders: &[Folder],
    ) -> ImportResult<(Matches, Vec<Titles>)> {
        let waiting = finding(&self.state, paired).len();
        let mut names = HashMap::new();
        for library in paired {
            if let Some(lib) = get_library(&self.state.db, library.library_id).await? {
                names.insert(lib.id, lib.name);
            }
        }
        if waiting > 0 {
            self.step(Step::Scanning, waiting as u64);
        }
        let (mut matches, mut titles) = (Matches::default(), Vec::new());
        let mut left: Vec<&Paired> = paired.iter().collect();
        let mut rows: Vec<LibraryRow> = paired
            .iter()
            .map(|p| LibraryRow {
                library: names.get(&p.library_id).cloned().unwrap_or_default(),
                phase: LibraryPhase::Queued,
                done: 0,
                total: 0,
                left_secs: None,
                found: None,
            })
            .collect();
        loop {
            self.cancelled()?;
            let scans = finding(&self.state, paired);
            for (row, library) in rows.iter_mut().zip(paired) {
                let Some(scan) = scans.iter().find(|s| s.library_id == library.library_id) else { continue };
                row.phase = match scan.phase {
                    ScanPhase::Walking => LibraryPhase::Walking,
                    ScanPhase::Probing => LibraryPhase::Probing,
                    _ => LibraryPhase::Queued,
                };
                (row.done, row.total) = (scan.processed, scan.total);
                row.left_secs = if scan.phase == ScanPhase::Probing { scan.left_secs } else { None };
            }
            if scans.is_empty() {
                break;
            }
            // One library is scanned at a time: the one under way, else the
            // next in line.
            let now = scans.iter().find(|s| s.phase != ScanPhase::Queued).or(scans.first()).map(|s| ScanNow {
                library: names.get(&s.library_id).cloned().unwrap_or_else(|| s.library_name.clone()),
                phase: s.phase,
                done: s.processed,
                total: s.total,
            });
            let done = (waiting - scans.len().min(waiting)) as u64;
            self.importer.set(&self.state, self.admin, |p| {
                p.done = done;
                p.scan = now;
                p.libraries = rows.clone();
            });
            let (ready, scanning): (Vec<&Paired>, Vec<&Paired>) =
                left.into_iter().partition(|p| !scans.iter().any(|s| s.library_id == p.library_id));
            left = scanning;
            for library in ready {
                self.matching(&mut rows, paired, library);
                let index = Index::load(&self.state.db, &[library.library_id]).await?;
                let found = self.find(library, &index, folders, &mut matches).await?;
                self.found(&mut rows, paired, library);
                self.state.events.send(ServerEvent::LibraryChanged { library_id: library.library_id });
                titles.push(found);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        if !left.is_empty() {
            self.step(Step::Matching, left.len() as u64);
            let ids: Vec<i64> = left.iter().map(|p| p.library_id).collect();
            let index = Index::load(&self.state.db, &ids).await?;
            for library in &left {
                self.matching(&mut rows, paired, library);
                titles.push(self.find(library, &index, folders, &mut matches).await?);
                self.found(&mut rows, paired, library);
                self.advance(1);
            }
            for library in &left {
                self.state.events.send(ServerEvent::LibraryChanged { library_id: library.library_id });
            }
        }
        Ok((matches, titles))
    }

    /// Says Plex's titles are looked for in `library`, of the libraries `rows` are for.
    fn matching(&self, rows: &mut [LibraryRow], paired: &[Paired], library: &Paired) {
        if let Some(row) = paired.iter().position(|p| p.library_id == library.library_id).map(|i| &mut rows[i]) {
            *row = LibraryRow { phase: LibraryPhase::Matching, left_secs: None, ..row.clone() };
        }
        let rows = rows.to_vec();
        self.importer.set(&self.state, self.admin, |p| p.libraries = rows);
    }

    /// Says `library` is done, with the titles found in it.
    fn found(&self, rows: &mut [LibraryRow], paired: &[Paired], library: &Paired) {
        if let Some(row) = paired.iter().position(|p| p.library_id == library.library_id).map(|i| &mut rows[i]) {
            let found = self.report.libraries[library.report].matched;
            *row = LibraryRow { phase: LibraryPhase::Done, found: Some(found), ..row.clone() };
        }
        let rows = rows.to_vec();
        self.importer.set(&self.state, self.admin, |p| p.libraries = rows);
    }

    /// Fetches the libraries' metadata, now that Plex's ids are on the
    /// titles it found, and waits for it: titles found only by their ids
    /// need it. Counts the titles fetched of those to fetch.
    async fn fetch(&self, paired: &[Paired]) -> ImportResult<()> {
        let scanner = &self.state.scanner;
        for library in paired {
            scanner.enqueue_metadata(library.library_id);
        }
        self.step(Step::Fetching, 0);
        // Each library's last count, kept once its fetch is done and its
        // status gone.
        let mut counts: HashMap<i64, (u64, u64)> = HashMap::new();
        let pace = Pace::new(0);
        loop {
            self.cancelled()?;
            let (mut busy, mut uncounted) = (false, false);
            for library in paired {
                match scanner.status_of(library.library_id) {
                    Some(status) => {
                        busy = true;
                        if status.phase == ScanPhase::Metadata {
                            counts.insert(library.library_id, (status.processed, status.total));
                        } else {
                            uncounted = true;
                        }
                    }
                    None => {
                        if let Some((done, total)) = counts.get_mut(&library.library_id) {
                            *done = *total;
                        }
                    }
                }
            }
            let (done, total) = counts.values().fold((0, 0), |(d, t), (done, total)| (d + done, t + total));
            // A library still waiting for its fetch would add to what's left.
            let left = if uncounted { None } else { pace.left(done, total) };
            self.importer.set(&self.state, self.admin, |p| {
                p.done = done;
                p.total = total;
                p.left_secs = left;
            });
            if !busy {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// The people brought: the Plex account as the administrator, and each
    /// Home user chosen as a profile of the home, made or found again, with
    /// Plex's restrictions.
    async fn people(
        &mut self,
        choices: &Choices,
        sections: &[Section],
        paired: &[Paired],
    ) -> ImportResult<Vec<Person>> {
        self.step(Step::Profiles, choices.users.len() as u64 + 1);
        let home = self.plex.home_users(&self.token).await?;
        let owner = home.iter().find(|u| u.admin);
        let mut people = vec![Person {
            account_id: 1,
            user_id: self.admin,
            server: self.server.clone(),
            sections: paired.iter().map(|p| p.key.clone()).collect(),
            report: self.report.profiles.len(),
        }];
        self.report.profiles.push(ProfileReport {
            title: owner.map(|u| u.title.clone()).unwrap_or_default(),
            user_id: Some(self.admin),
            ..Default::default()
        });
        if let Some(owner) = owner {
            self.name(self.admin, owner).await?;
            self.picture(self.admin, owner).await?;
        }
        self.advance(1);
        let chosen: Vec<&HomeUser> =
            home.iter().filter(|u| !u.admin && choices.users.iter().any(|c| c.id == u.id)).collect();
        let filters = if chosen.is_empty() { Vec::new() } else { self.plex.user_filters(&self.token).await? };
        let region = crate::ratings::region(&self.state.db).await?;
        let all_sections: HashSet<&str> = sections.iter().map(|s| s.key.as_str()).collect();
        for user in chosen {
            self.cancelled()?;
            let pin = choices.users.iter().find(|c| c.id == user.id).and_then(|c| c.pin.as_deref());
            let mut report = ProfileReport { title: user.title.clone(), ..Default::default() };
            let theirs = async {
                let server = self.server_for(user, pin).await?;
                let sections: HashSet<String> = self.plex.sections(&server).await?.into_iter().map(|s| s.key).collect();
                Ok::<_, ImportError>((server, sections))
            }
            .await;
            match theirs {
                Ok((server, sections)) => {
                    let limits = limits_of(&filters, user.id, &region);
                    // Every Plex library, and every Pley library one of
                    // them: only then may the profile open all of Pley's.
                    let everything =
                        all_sections.iter().all(|k| sections.contains(*k)) && self.all_paired(paired).await?;
                    let open: Vec<i64> =
                        paired.iter().filter(|p| sections.contains(&p.key)).map(|p| p.library_id).collect();
                    let (user_id, created) = self.profile(user, &limits, everything, &open, &mut report).await?;
                    self.picture(user_id, user).await?;
                    report.user_id = Some(user_id);
                    report.created = created;
                    people.push(Person {
                        account_id: user.id,
                        user_id,
                        server,
                        sections,
                        report: self.report.profiles.len(),
                    });
                }
                Err(ImportError::Plex(PlexError::NeedsPin)) => report.skipped = Some("pin_needed".into()),
                Err(ImportError::NoAccess) => report.skipped = Some("no_access".into()),
                Err(ImportError::Plex(_)) => report.skipped = Some("refused".into()),
                Err(e) => return Err(e),
            }
            self.report.profiles.push(report);
            self.advance(1);
        }
        Ok(people)
    }

    /// Home user `user`'s Plex name as the display name of `user_id`, the
    /// administrator, where it has none: their username stands for them
    /// until then. A name that is the username already changes nothing.
    async fn name(&self, user_id: i64, user: &HomeUser) -> ImportResult<()> {
        let name = cut(&user.title, MAX_NAME);
        if name.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "UPDATE users SET display_name = ?
             WHERE id = ? AND NULLIF(TRIM(display_name), '') IS NULL AND username <> ? COLLATE NOCASE",
        )
        .bind(&name)
        .bind(user_id)
        .bind(&name)
        .execute(&self.state.db.write)
        .await?;
        Ok(())
    }

    /// Home user `user`'s Plex picture and colour for profile `user_id`,
    /// each only where the profile has none. A picture that can't be had,
    /// or isn't a picture, is left out without a word.
    async fn picture(&self, user_id: i64, user: &HomeUser) -> ImportResult<()> {
        let Some(thumb) = user.thumb.as_deref() else { return Ok(()) };
        let db = &self.state.db;
        let (avatar, color): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT avatar, avatar_color FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_one(&db.read)
                .await?;
        if avatar.is_some() && color.is_some() {
            return Ok(());
        }
        let picture = self.plex.picture(thumb).await;
        if color.is_none()
            && let Some(color) = picture.color.filter(|c| accounts::validate_color(c).is_ok())
        {
            sqlx::query("UPDATE users SET avatar_color = ? WHERE id = ? AND avatar_color IS NULL")
                .bind(color)
                .bind(user_id)
                .execute(&db.write)
                .await?;
        }
        let Some(bytes) = picture.bytes.filter(|_| avatar.is_none()) else { return Ok(()) };
        let Ok(_decoding) = crate::images::UPLOAD_DECODES.acquire().await else { return Ok(()) };
        match tokio::task::spawn_blocking(move || accounts::process_avatar(&bytes)).await {
            Ok(Ok(jpeg)) => {
                if let Err(e) = accounts::set_avatar(db, &self.state.config.data_dir, user_id, jpeg).await {
                    tracing::debug!("the Plex import couldn't keep a profile's picture: {e}");
                }
            }
            _ => tracing::debug!("the Plex import found no picture it could read for a profile"),
        }
        Ok(())
    }

    /// Whether every Pley library is one of those paired.
    async fn all_paired(&self, paired: &[Paired]) -> ImportResult<bool> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM libraries").fetch_all(&self.state.db.read).await?;
        Ok(ids.iter().all(|id| paired.iter().any(|p| p.library_id == *id)))
    }

    /// The server with Home user `user`'s own token, which lists their
    /// watch state.
    async fn server_for(&self, user: &HomeUser, pin: Option<&str>) -> ImportResult<Server> {
        let token = self.plex.switch(&self.token, &user.uuid, pin).await?;
        let theirs = self.plex.servers(&token).await?;
        let resource =
            theirs.into_iter().find(|r| r.client_identifier == self.server.machine_id).ok_or(ImportError::NoAccess)?;
        let token = resource.access_token.ok_or(ImportError::NoAccess)?;
        Ok(Server { base: self.server.base.clone(), token, machine_id: self.server.machine_id.clone() })
    }

    /// Home user `user`'s profile, with Plex's `limits`. A new one is a
    /// managed profile of the home, made in one go with its limits and the
    /// libraries it may open (all of them only when `everything`, else
    /// `open`). One an earlier import made gets the labels added to its own
    /// rules, and the age limit only when it has none. Whether it's new.
    async fn profile(
        &self,
        user: &HomeUser,
        limits: &Limits,
        everything: bool,
        open: &[i64],
        report: &mut ProfileReport,
    ) -> ImportResult<(i64, bool)> {
        let db = &self.state.db;
        let found = imported_profile(db, user.id).await?;
        let (mut show, mut hide) = match found {
            Some(id) => (accounts::label_rules(db, id, "show").await?, accounts::label_rules(db, id, "hide").await?),
            None => (Vec::new(), Vec::new()),
        };
        let mut left_over = limits.left_over.clone();
        for (list, from) in [(&mut hide, &limits.hide), (&mut show, &limits.show)] {
            for label in from {
                if label.chars().count() > crate::labels::MAX_LEN {
                    left_over.push(label.clone());
                } else if !list.iter().any(|l| l.eq_ignore_ascii_case(label)) {
                    list.push(label.clone());
                }
            }
        }
        // "Never" in either is "never".
        show.retain(|s| !hide.iter().any(|h| h.eq_ignore_ascii_case(s)));
        left_over.extend(hide.split_off(hide.len().min(MAX_LABEL_RULES)));
        left_over.extend(show.split_off(show.len().min(MAX_LABEL_RULES)));

        let mut tx = db.write.begin().await?;
        let (id, created, max_age) = match found {
            Some(id) => {
                let (age, born): (Option<i64>, Option<String>) =
                    sqlx::query_as("SELECT max_age, birth_month FROM users WHERE id = ?")
                        .bind(id)
                        .fetch_one(&mut *tx)
                        .await?;
                let age = if age.is_none() && born.is_none() && limits.max_age.is_some() {
                    sqlx::query("UPDATE users SET max_age = ?, max_age_system = ? WHERE id = ?")
                        .bind(limits.max_age)
                        .bind(&limits.system)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    limits.max_age
                } else {
                    age
                };
                sqlx::query("DELETE FROM user_label_rules WHERE user_id = ?").bind(id).execute(&mut *tx).await?;
                (id, false, age)
            }
            None => {
                let username = free_username(db, &user.title).await?;
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO users (id, username, password_hash, display_name, is_admin, managed, home, all_libraries,
                                        max_age, max_age_system, request_access, created_at)
                     VALUES ((SELECT last + 1 FROM user_ids), ?, ?, ?, 0, 1, 1, ?, ?, ?, 'off', ?) RETURNING id",
                )
                .bind(&username)
                .bind(NO_PASSWORD)
                .bind(Some(cut(&user.title, MAX_NAME)).filter(|t| !t.is_empty()))
                .bind(everything)
                .bind(limits.max_age)
                .bind(&limits.system)
                .bind(now())
                .fetch_one(&mut *tx)
                .await?;
                sqlx::query("INSERT OR REPLACE INTO import_accounts (source, external_id, user_id) VALUES (?, ?, ?)")
                    .bind(SOURCE)
                    .bind(user.id.to_string())
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                if !everything {
                    for library in open {
                        sqlx::query("INSERT OR IGNORE INTO user_libraries (user_id, library_id) VALUES (?, ?)")
                            .bind(id)
                            .bind(library)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
                (id, true, limits.max_age)
            }
        };
        accounts::add_label_rules(&mut tx, id, "show", &show).await?;
        accounts::add_label_rules(&mut tx, id, "hide", &hide).await?;
        tx.commit().await?;
        if !created {
            self.state.auth.forget_user(id);
            self.state.events.send(ServerEvent::OwnLibrariesChanged { user_id: id });
            self.state.events.send(ServerEvent::RequestsSettingsChanged { user_id: Some(id) });
        }
        report.max_age = max_age;
        report.show_labels = limits.show.clone();
        report.hide_labels = limits.hide.clone();
        report.left_over = left_over;
        report.not_kept = limits.not_kept.clone();
        Ok((id, created))
    }

    /// Matches library `library`'s items, moving their dates added to
    /// Plex's when earlier. What "Matches and edits" needs of them.
    async fn find(
        &mut self,
        library: &Paired,
        index: &Index,
        folders: &[Folder],
        matches: &mut Matches,
    ) -> ImportResult<Titles> {
        let mut retry: Vec<Item> = Vec::new();
        let mut shows: Vec<Item> = Vec::new();
        let (mut matched, mut missed, mut named) = (0u64, 0u64, Vec::new());
        let mut titles = Titles::default();
        for (plex_type, kind) in types(library.kind) {
            let query = [("type", plex_type.to_string()), ("includeGuids", "1".to_owned())];
            let mut pager = Pager::new(&format!("/library/sections/{}/all", library.key), &query);
            while let Some(items) = pager.next(self).await? {
                let (mut dates, mut details) = (Vec::new(), Vec::new());
                for item in items {
                    if matches!(*kind, "movie" | "show" | "season") {
                        titles.pages.push(item.rating_key.clone());
                    }
                    if matches!(*kind, "movie" | "show")
                        && let Some(ids) = PlexIds::of(&item)
                    {
                        titles.ids.push(ids);
                    }
                    match matches.find(index, library.library_id, folders, &item) {
                        Some(id) => {
                            if is_leaf(kind) {
                                matched += 1;
                            }
                            if let Some(added) = item.added_at.filter(|a| *a > 0) {
                                dates.push((id, added));
                            }
                            if let Some(found) = PlexDetails::of(&item) {
                                details.push((id, found));
                            }
                        }
                        None if *kind == "episode" => retry.push(item),
                        None if *kind == "show" => shows.push(item),
                        None if is_leaf(kind) => {
                            missed += 1;
                            if named.len() < MISSED_NAMED {
                                named.push(describe(&item));
                            }
                        }
                        None => {}
                    }
                }
                self.dates(&dates).await?;
                write_details(&self.state.db, &details).await?;
            }
            // An episode whose show was found only through a later
            // episode's file.
            for item in retry.drain(..) {
                match matches.find(index, library.library_id, folders, &item) {
                    Some(id) => {
                        matched += 1;
                        if let Some(added) = item.added_at.filter(|a| *a > 0) {
                            self.dates(&[(id, added)]).await?;
                        }
                        if let Some(found) = PlexDetails::of(&item) {
                            write_details(&self.state.db, &[(id, found)]).await?;
                        }
                    }
                    None => {
                        missed += 1;
                        if named.len() < MISSED_NAMED {
                            named.push(describe(&item));
                        }
                    }
                }
            }
        }
        // A show without files of its own is found through its episodes',
        // which Plex lists after it.
        let (mut dates, mut details) = (Vec::new(), Vec::new());
        for item in shows {
            let Some(id) = matches.get(&item.rating_key) else { continue };
            if let Some(added) = item.added_at.filter(|a| *a > 0) {
                dates.push((id, added));
            }
            if let Some(found) = PlexDetails::of(&item) {
                details.push((id, found));
            }
        }
        self.dates(&dates).await?;
        write_details(&self.state.db, &details).await?;
        let report = &mut self.report.libraries[library.report];
        report.matched = matched;
        report.missed = missed;
        report.missed_titles = named;
        Ok(titles)
    }

    /// Moves items' dates added to Plex's, only ever earlier.
    async fn dates(&mut self, dates: &[(i64, i64)]) -> ImportResult<()> {
        if dates.is_empty() {
            return Ok(());
        }
        let mut tx = self.state.db.write.begin().await?;
        let mut moved = 0;
        for (id, added) in dates {
            moved += sqlx::query("UPDATE items SET added_at = ? WHERE id = ? AND added_at > ?")
                .bind(added)
                .bind(id)
                .bind(added)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        }
        tx.commit().await?;
        self.report.dates_moved += moved;
        Ok(())
    }

    /// Puts Plex's labels on the titles of `library` found in Pley.
    async fn labels(&mut self, library: &Paired, matches: &Matches) -> ImportResult<()> {
        let labels = self.plex.labels(&self.server, &library.key).await?;
        let mut on: HashMap<i64, Vec<String>> = HashMap::new();
        for label in labels {
            // Longer than Pley keeps: no rule could name it either.
            if label.title.trim().is_empty() || label.title.chars().count() > crate::labels::MAX_LEN {
                continue;
            }
            // Kometa's mark of an overlay it drew in Plex. On Pley it would
            // send Kometa to the original it kept for that `ratingKey` in
            // Plex, which in Pley is another title's; without it, Kometa
            // draws its overlays afresh on Pley's poster.
            if label.title.trim().eq_ignore_ascii_case(KOMETA_OVERLAY) {
                continue;
            }
            for (plex_type, _) in types(library.kind) {
                let query = [("type", plex_type.to_string()), ("label", label.key.clone())];
                let mut pager = Pager::new(&format!("/library/sections/{}/all", library.key), &query);
                while let Some(items) = pager.next(self).await? {
                    for item in items {
                        if let Some(id) = matches.get(&item.rating_key) {
                            on.entry(id).or_default().push(label.title.clone());
                        }
                    }
                }
            }
        }
        let ids: Vec<i64> = on.keys().copied().collect();
        let count = |labels: HashMap<i64, Vec<String>>| labels.values().map(Vec::len).sum::<usize>();
        let before = count(crate::labels::for_items(&self.state.db, &ids).await?);
        for (id, mut labels) in on {
            labels.truncate(crate::labels::MAX_PER_ITEM);
            let Ok(labels) = crate::labels::check(&labels) else { continue };
            crate::labels::edit(&self.state.db, id, &labels, &[]).await?;
        }
        let after = count(crate::labels::for_items(&self.state.db, &ids).await?);
        self.report.labels_added += after.saturating_sub(before) as u64;
        Ok(())
    }

    /// Plex's matches and locked fields of a library's movies, shows and
    /// seasons. A title Plex matched to another TMDB entry than Pley did is
    /// matched again to Plex's, as Fix Match does, unless it was fixed in
    /// Pley; one Pley hasn't matched yet starts from Plex's. A locked field
    /// becomes an edit, unless Pley has one for it already.
    async fn edits(&mut self, titles: &Titles, index: &Index, matches: &Matches) -> ImportResult<()> {
        let db = self.state.db.clone();
        for ids in &titles.ids {
            self.cancelled()?;
            let Some(id) = matches.get(&ids.key) else { continue };
            let title = &ids.title;
            let fetched: Option<bool> = sqlx::query_scalar(
                "SELECT metadata_at IS NOT NULL FROM items WHERE id = ? AND kind IN ('movie', 'show')",
            )
            .bind(id)
            .fetch_optional(&db.read)
            .await?;
            let Some(fetched) = fetched else { continue };
            if fixed_in_pley(&db, id).await? {
                continue;
            }
            if !fetched {
                seed(&db, id, ids).await?;
                continue;
            }
            let Some(tmdb) = &ids.tmdb else { continue };
            let ours = index.tmdb(id);
            if ours == Some(*tmdb) {
                continue;
            }
            if ours.is_none() {
                // A show found on TheTVDB or TVmaze keeps that match, and
                // only learns its TMDB id. A title with no match at all
                // takes Plex's: matched to it with TMDB's key, or else
                // refreshed with it next time, even after a failed search.
                let unmatched: bool = sqlx::query_scalar(
                    "SELECT tvdb_id IS NULL AND tvmaze_id IS NULL FROM items WHERE id = ? AND kind IN ('movie', 'show')",
                )
                .bind(id)
                .fetch_optional(&db.read)
                .await?
                .unwrap_or(false);
                if unmatched && self.state.metadata.tmdb_enabled() {
                    match self.state.metadata.rematch(id, *tmdb).await {
                        Ok(()) => self.report.matches_fixed += 1,
                        Err(e) => tracing::debug!("the Plex import couldn't match {title}: {e:#}"),
                    }
                } else {
                    sqlx::query(
                        "UPDATE items SET tmdb_id = ?, metadata_at = CASE WHEN ? THEN NULL ELSE metadata_at END
                         WHERE id = ? AND tmdb_id IS NULL AND kind IN ('movie', 'show')",
                    )
                    .bind(tmdb)
                    .bind(unmatched)
                    .bind(id)
                    .execute(&db.write)
                    .await?;
                }
            } else if self.state.metadata.tmdb_enabled() {
                match self.state.metadata.rematch(id, *tmdb).await {
                    Ok(()) => self.report.matches_fixed += 1,
                    Err(e) => tracing::debug!("the Plex import couldn't match {title} again: {e:#}"),
                }
            }
        }

        let pages: Vec<(i64, String)> =
            titles.pages.iter().filter_map(|key| Some((matches.get(key)?, key.clone()))).collect();
        let (plex, server) = (self.plex.clone(), self.server.clone());
        let mut read = futures::stream::iter(pages.into_iter().map(|(id, key)| {
            let (plex, server) = (plex.clone(), server.clone());
            async move { (id, plex.metadata(&server, &key).await) }
        }))
        .buffer_unordered(PAGES_AT_ONCE);
        while let Some((id, page)) = read.next().await {
            self.cancelled()?;
            self.advance(1);
            let item = match page {
                Ok(Some(item)) => item,
                Ok(None) => continue,
                Err(e) => {
                    tracing::debug!("the Plex import couldn't read a title's page: {e}");
                    continue;
                }
            };
            let locked = locked_edits(&item);
            if locked.is_empty() {
                continue;
            }
            let ours = crate::overrides::locked_fields(&db, id).await?;
            let changes: Vec<(String, Option<String>)> = locked
                .into_iter()
                .filter(|(field, value)| {
                    !ours.iter().any(|f| f == field) && crate::overrides::validate(field, Some(value)).is_ok()
                })
                .map(|(field, value)| (field.to_owned(), Some(value)))
                .collect();
            if changes.is_empty() {
                continue;
            }
            crate::overrides::set(&db, id, &changes).await?;
            for (field, _) in &changes {
                *self.report.edits.entry(field.clone()).or_default() += 1;
            }
        }
        Ok(())
    }

    /// Library `library`'s collections that aren't smart, with the titles
    /// found in Pley, in Plex's order: one Pley has by the same title gets
    /// those it lacks, else one is made, with Plex's order and labels. Pley's
    /// own TMDB collections aren't found again: their refresh keeps only
    /// what TMDB puts in them.
    async fn collections(&mut self, library: &Paired, matches: &Matches) -> ImportResult<()> {
        if !matches!(library.kind, LibraryKind::Movies | LibraryKind::Shows) {
            return Ok(());
        }
        let db = self.state.db.clone();
        let mut found = Vec::new();
        let mut pager = Pager::new(&format!("/library/sections/{}/collections", library.key), &[]);
        while let Some(items) = pager.next(self).await? {
            found.extend(items);
        }
        for collection in found {
            if collection.smart {
                self.report.smart_left += 1;
                continue;
            }
            let title = cut(&collection.title, MAX_TITLE);
            if title.is_empty() || !is_key(&collection.rating_key) {
                continue;
            }
            let mut ids = Vec::new();
            let mut pager = Pager::new(&format!("/library/collections/{}/children", collection.rating_key), &[]);
            while let Some(items) = pager.next(self).await? {
                ids.extend(items.iter().filter_map(|i| matches.get(&i.rating_key)));
            }
            if ids.is_empty() {
                continue;
            }
            let existing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM collections WHERE library_id = ? AND title = ? COLLATE NOCASE AND smart_filter IS NULL
                 AND tmdb_id IS NULL ORDER BY id LIMIT 1",
            )
            .bind(library.library_id)
            .bind(&title)
            .fetch_optional(&db.read)
            .await?;
            let id = match existing {
                Some(id) => id,
                None => {
                    let summary = collection.summary.as_deref().map(|s| cut(s, MAX_SUMMARY)).filter(|s| !s.is_empty());
                    let id = crate::collections::create(&db, library.library_id, &title, summary.as_deref()).await?;
                    let sort = match collection.collection_sort {
                        Some(1) => "title",
                        Some(2) => "custom",
                        _ => "release",
                    };
                    if sort != "release" {
                        crate::collections::update(&db, id, None, None, Some(sort)).await?;
                    }
                    self.report.collections_made += 1;
                    id
                }
            };
            self.report.collection_items += crate::collections::add_items(&db, id, library.library_id, &ids).await?;
            // Listings leave labels out: the collection's own page has them.
            if let Ok(Some(page)) = self.plex.metadata(&self.server, &collection.rating_key).await {
                let labels = kept_labels(&page);
                if !labels.is_empty() {
                    crate::labels::edit_collection(&db, id, &labels, &[]).await?;
                }
            }
        }
        Ok(())
    }

    /// `person`'s playlists that aren't smart, with the titles found in
    /// Pley: one of theirs by the same title and kind gets those it lacks,
    /// else one is made, in Plex's order.
    async fn playlists(&mut self, person: &Person, matches: &Matches) -> ImportResult<()> {
        let db = self.state.db.clone();
        let mut lists = Vec::new();
        let mut pager = Pager::on(person.server.clone(), "/playlists", &[]);
        while let Some(items) = pager.next(self).await? {
            lists.extend(items);
        }
        for list in lists {
            let kind = match list.playlist_type.as_deref() {
                Some("video") => "video",
                Some("audio") => "audio",
                _ => continue,
            };
            if list.smart {
                self.report.smart_left += 1;
                continue;
            }
            let title = cut(&list.title, MAX_TITLE);
            if title.is_empty() || !is_key(&list.rating_key) {
                continue;
            }
            let mut ids = Vec::new();
            let mut pager = Pager::on(person.server.clone(), &format!("/playlists/{}/items", list.rating_key), &[]);
            while let Some(items) = pager.next(self).await? {
                ids.extend(items.iter().filter_map(|i| matches.get(&i.rating_key)));
            }
            if ids.is_empty() {
                continue;
            }
            let existing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM playlists WHERE user_id = ? AND title = ? COLLATE NOCASE AND kind = ?
                 AND smart_filter IS NULL ORDER BY id LIMIT 1",
            )
            .bind(person.user_id)
            .bind(&title)
            .bind(kind)
            .fetch_optional(&db.read)
            .await?;
            let made = existing.is_none();
            let id = match existing {
                Some(id) => {
                    // Found again: only what it lacks, once.
                    let mut have: HashSet<i64> =
                        sqlx::query_scalar("SELECT item_id FROM playlist_items WHERE playlist_id = ?")
                            .bind(id)
                            .fetch_all(&db.read)
                            .await?
                            .into_iter()
                            .collect();
                    ids.retain(|item| have.insert(*item));
                    id
                }
                None => {
                    let stamp = now();
                    sqlx::query_scalar(
                        "INSERT INTO playlists (user_id, title, kind, created_at, updated_at) VALUES (?, ?, ?, ?, ?)
                         RETURNING id",
                    )
                    .bind(person.user_id)
                    .bind(&title)
                    .bind(kind)
                    .bind(stamp)
                    .bind(stamp)
                    .fetch_one(&db.write)
                    .await?
                }
            };
            let added = crate::playlists::append_leaves(&db, id, &ids).await?;
            if made || added > 0 {
                self.report.profiles[person.report].playlists += 1;
            }
        }
        Ok(())
    }

    /// `person`'s watched titles, resume points and ratings, merged with
    /// what Pley has.
    async fn watch_state(&mut self, person: &Person, paired: &[Paired], matches: &Matches) -> ImportResult<()> {
        let (mut watched, mut resumed, mut rated) = (0, 0, 0);
        for library in paired.iter().filter(|p| person.sections.contains(&p.key)) {
            let path = format!("/library/sections/{}/all", library.key);
            for (plex_type, kind) in types(library.kind) {
                let plays = matches!(*kind, "movie" | "episode");
                let mut lists: Vec<(&str, [(&str, String); 2])> = Vec::new();
                if plays {
                    lists.push(("watched", [("type", plex_type.to_string()), ("unwatched", "0".into())]));
                    lists.push(("resume", [("type", plex_type.to_string()), ("inProgress", "1".into())]));
                }
                lists.push(("rated", [("type", plex_type.to_string()), ("userRating>>", "0".into())]));
                for (list, query) in lists {
                    let mut pager = Pager::on(person.server.clone(), &path, &query);
                    while let Some(items) = pager.next(self).await? {
                        let found: Vec<(i64, Item)> =
                            items.into_iter().filter_map(|i| Some((matches.get(&i.rating_key)?, i))).collect();
                        let n = merge(&self.state.db, person.user_id, list, &found).await?;
                        match list {
                            "watched" => watched += n,
                            "resume" => resumed += n,
                            _ => rated += n,
                        }
                    }
                }
            }
        }
        let report = &mut self.report.profiles[person.report];
        report.watched += watched;
        report.resumed += resumed;
        report.rated += rated;
        Ok(())
    }

    /// `person`'s plays from Plex's history, each once.
    async fn history(
        &mut self,
        person: &Person,
        matches: &Matches,
        devices: &HashMap<i64, String>,
    ) -> ImportResult<()> {
        let query = [("accountID", person.account_id.to_string()), ("sort", "viewedAt:desc".to_owned())];
        let mut pager = Pager::new("/status/sessions/history/all", &query);
        while let Some(items) = pager.next(self).await? {
            let mut tx = self.state.db.write.begin().await?;
            let mut plays = 0;
            for item in items {
                let (Some(id), Some(viewed)) = (matches.get(&item.rating_key), item.viewed_at) else { continue };
                if item.account_id.is_some_and(|a| a != person.account_id) {
                    continue;
                }
                let device = item.device_id.and_then(|d| devices.get(&d)).filter(|d| !d.is_empty());
                plays += sqlx::query(
                    "INSERT INTO play_history (user_id, item_id, viewed_at, device)
                     SELECT ?, ?, ?, ? WHERE NOT EXISTS
                        (SELECT 1 FROM play_history WHERE user_id = ? AND item_id = ? AND viewed_at = ?)",
                )
                .bind(person.user_id)
                .bind(id)
                .bind(viewed)
                .bind(device)
                .bind(person.user_id)
                .bind(id)
                .bind(viewed)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }
            tx.commit().await?;
            self.report.profiles[person.report].plays += plays;
        }
        Ok(())
    }
}

/// Merges one list of a person's watch state into Pley's: `watched` is
/// never taken away and keeps the larger count and the later date; a
/// resume point comes when Plex's is newer than Pley's row; a rating when
/// Pley has none. How many rows changed.
async fn merge(db: &Db, user_id: i64, list: &str, found: &[(i64, Item)]) -> sqlx::Result<u64> {
    let mut tx = db.write.begin().await?;
    let mut changed = 0;
    let stamp = now();
    for (id, item) in found {
        let viewed = item.last_viewed_at;
        let result = match list {
            "watched" => {
                let count = item.view_count.unwrap_or(1).max(1);
                sqlx::query(
                    "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at)
                     VALUES (?, ?, 0, 1, ?, ?, ?)
                     ON CONFLICT(user_id, item_id) DO UPDATE SET
                        watched = 1,
                        play_count = MAX(user_item_state.play_count, excluded.play_count),
                        last_played_at = NULLIF(MAX(COALESCE(user_item_state.last_played_at, 0),
                                                    COALESCE(excluded.last_played_at, 0)), 0),
                        -- A resume point Pley has from after Plex's watch is
                        -- a rewatch, and stays.
                        position_ms = CASE WHEN user_item_state.watched = 0
                                            AND COALESCE(user_item_state.last_played_at, 0) <= COALESCE(excluded.last_played_at, 0)
                                           THEN 0 ELSE user_item_state.position_ms END
                     WHERE user_item_state.watched = 0 OR user_item_state.play_count < excluded.play_count
                        OR COALESCE(user_item_state.last_played_at, 0) < COALESCE(excluded.last_played_at, 0)",
                )
                .bind(user_id)
                .bind(id)
                .bind(count)
                .bind(viewed)
                .bind(viewed.unwrap_or(stamp))
                .execute(&mut *tx)
                .await?
            }
            "resume" => {
                let (Some(offset), Some(viewed)) = (item.view_offset.filter(|o| *o > 0), viewed) else { continue };
                sqlx::query(
                    "INSERT INTO user_item_state (user_id, item_id, position_ms, watched, play_count, last_played_at, updated_at)
                     VALUES (?, ?, ?, 0, 0, ?, ?)
                     ON CONFLICT(user_id, item_id) DO UPDATE SET
                        position_ms = excluded.position_ms,
                        last_played_at = MAX(COALESCE(user_item_state.last_played_at, 0), excluded.last_played_at),
                        updated_at = excluded.updated_at
                     WHERE user_item_state.updated_at <= excluded.updated_at
                        AND COALESCE(user_item_state.last_played_at, 0) <= excluded.last_played_at
                        AND user_item_state.position_ms != excluded.position_ms",
                )
                .bind(user_id)
                .bind(id)
                .bind(offset)
                .bind(viewed)
                .bind(viewed)
                .execute(&mut *tx)
                .await?
            }
            _ => {
                let Some(rating) = item.user_rating.and_then(crate::catalog::valid_rating) else { continue };
                let rated_at = item.last_rated_at.unwrap_or(stamp);
                sqlx::query(
                    "INSERT INTO user_item_state (user_id, item_id, updated_at, rating, rated_at)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT(user_id, item_id) DO UPDATE SET rating = excluded.rating, rated_at = excluded.rated_at
                     WHERE user_item_state.rating IS NULL",
                )
                .bind(user_id)
                .bind(id)
                .bind(stamp)
                .bind(rating)
                .bind(rated_at)
                .execute(&mut *tx)
                .await?
            }
        };
        changed += result.rows_affected();
    }
    tx.commit().await?;
    Ok(changed)
}

/// A movie's, show's or episode's title, year and summary as Plex has
/// them, cut to the lengths Pley keeps.
struct PlexDetails {
    title: String,
    sort_title: String,
    year: Option<i64>,
    summary: Option<String>,
}

impl PlexDetails {
    fn of(item: &Item) -> Option<Self> {
        if !matches!(item.kind.as_str(), "movie" | "show" | "episode") {
            return None;
        }
        let title = cut(&item.title, MAX_TITLE);
        if title.is_empty() {
            return None;
        }
        let sort = item.title_sort.as_deref().map(|s| cut(s, MAX_TITLE)).filter(|s| !s.is_empty());
        Some(Self {
            sort_title: sort.map_or_else(|| sort_title(&title), |s| s.to_lowercase()),
            title,
            year: item.year.filter(|y| (1800..=2200).contains(y)),
            summary: item.summary.as_deref().map(|s| cut(s, MAX_SUMMARY)).filter(|s| !s.is_empty()),
        })
    }
}

/// Plex's details on titles the fetch hasn't reached, or found nothing
/// for (a negative `metadata_at`), so that they show with their names
/// rather than their files'. What Pley has better is left: a fetch's, a
/// title locked or edited in Pley, an NFO file's.
async fn write_details(db: &Db, details: &[(i64, PlexDetails)]) -> sqlx::Result<()> {
    if details.is_empty() {
        return Ok(());
    }
    let mut tx = db.write.begin().await?;
    for (id, d) in details {
        sqlx::query(
            "UPDATE items SET title = ?, sort_title = ?, year = COALESCE(?, year), summary = COALESCE(?, summary),
                 updated_at = ?
             WHERE id = ? AND (metadata_at IS NULL OR metadata_at < 0) AND metadata_locked = 0
               AND NOT EXISTS (SELECT 1 FROM item_nfo WHERE item_id = items.id)
               AND NOT EXISTS (SELECT 1 FROM item_overrides WHERE item_id = items.id)",
        )
        .bind(&d.title)
        .bind(&d.sort_title)
        .bind(d.year)
        .bind(&d.summary)
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Plex's ids on a movie or show Pley hasn't fetched yet, where it has none,
/// for the fetch to use instead of searching by title. A show takes its
/// TheTVDB and IMDb ids too: TheTVDB's and TVmaze's refreshes look a show
/// up by those, never by its TMDB id.
async fn seed(db: &Db, id: i64, ids: &PlexIds) -> sqlx::Result<()> {
    let (tvdb, imdb) = if ids.show { (ids.tvdb, ids.imdb.as_deref()) } else { (None, None) };
    sqlx::query(
        "UPDATE items SET tmdb_id = COALESCE(tmdb_id, ?), tvdb_id = COALESCE(tvdb_id, ?), imdb_id = COALESCE(imdb_id, ?)
         WHERE id = ? AND metadata_at IS NULL AND kind IN ('movie', 'show')",
    )
    .bind(ids.tmdb)
    .bind(tvdb)
    .bind(imdb)
    .bind(id)
    .execute(&db.write)
    .await?;
    Ok(())
}

/// Whether title `id`'s match was settled in Pley, which the import leaves:
/// chosen with Fix Match, locked, or named by its NFO file.
async fn fixed_in_pley(db: &Db, id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM item_overrides WHERE item_id = ?1 AND field = ?2)
             OR EXISTS (SELECT 1 FROM items WHERE id = ?1 AND metadata_locked)
             OR EXISTS (SELECT 1 FROM item_nfo WHERE item_id = ?1 AND (locked OR json_valid(ids)
                        AND COALESCE(json_extract(ids, '$.tmdb'), json_extract(ids, '$.imdb'),
                                     json_extract(ids, '$.tvdb'), json_extract(ids, '$.tvmaze')) IS NOT NULL))",
    )
    .bind(id)
    .bind(crate::overrides::MATCH)
    .fetch_one(&db.read)
    .await
}

/// The edits Plex's locked fields of `item` make, by Pley's name for the
/// field: cut to the lengths Pley keeps, empty ones left out. A season has
/// a title and a summary only.
fn locked_edits(item: &Item) -> Vec<(&'static str, String)> {
    let text = |value: Option<&str>, max: usize| value.map(|v| cut(v, max)).filter(|v| !v.is_empty());
    let mut fields = vec![
        ("title", "title", text(Some(&item.title), MAX_TITLE)),
        ("summary", "summary", text(item.summary.as_deref(), MAX_SUMMARY)),
    ];
    if item.kind != "season" {
        let genres: Vec<String> =
            item.genres.iter().map(|g| cut(&g.tag, MAX_TITLE)).filter(|g| !g.is_empty()).take(MAX_GENRES).collect();
        fields.extend([
            ("titleSort", "sort_title", text(item.title_sort.as_deref(), MAX_TITLE)),
            ("originalTitle", "original_title", text(item.original_title.as_deref(), MAX_TITLE)),
            ("tagline", "tagline", text(item.tagline.as_deref(), MAX_TITLE)),
            ("year", "year", item.year.map(|y| y.to_string())),
            ("originallyAvailableAt", "release_date", text(item.originally_available_at.as_deref(), 10)),
            // As Plex writes it, with its country when it has one ("gb/12A"),
            // which the edit keeps for the rating's age.
            ("contentRating", "content_rating", text(item.content_rating.as_deref(), 40)),
            ("studio", "studio", text(item.studio.as_deref(), MAX_TITLE)),
            ("genre", "genres", (!genres.is_empty()).then(|| serde_json::to_string(&genres).unwrap_or_default())),
        ]);
    }
    fields
        .into_iter()
        .filter(|(plex, _, _)| item.locked(plex))
        .filter_map(|(_, field, value)| Some((field, value?)))
        .collect()
}

/// A collection's labels from its page, as Pley keeps them.
fn kept_labels(page: &Item) -> Vec<String> {
    let labels: Vec<String> = page
        .labels
        .iter()
        .map(|l| l.tag.trim().to_owned())
        .filter(|l| !l.is_empty() && l.chars().count() <= crate::labels::MAX_LEN)
        .take(crate::labels::MAX_PER_ITEM)
        .collect();
    crate::labels::check(&labels).unwrap_or_default()
}

/// `text` trimmed and cut to `max` characters.
fn cut(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect::<String>().trim_end().to_owned()
}

/// Whether `key` is a `ratingKey` as Plex writes them, a number, before it
/// goes in a path.
fn is_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 20 && key.chars().all(|c| c.is_ascii_digit())
}

/// Whether items of Plex's `kind` are counted in the report (those that
/// play), rather than found through them.
fn is_leaf(kind: &str) -> bool {
    matches!(kind, "movie" | "episode" | "track")
}

/// An item as the report names it.
fn describe(item: &Item) -> String {
    match (item.kind.as_str(), &item.grandparent_title, item.parent_index, item.index) {
        ("episode", Some(show), Some(season), Some(episode)) => format!("{show} S{season:02}E{episode:02}"),
        ("track", Some(artist), ..) => format!("{artist} – {}", item.title),
        _ => item.title.clone(),
    }
}

/// A username for a profile named `title`, no one's yet.
async fn free_username(db: &Db, title: &str) -> sqlx::Result<String> {
    let base: String = title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '@') { c } else { '.' })
        .collect::<String>()
        .split('.')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(".");
    let base: String = if base.is_empty() { "profile".into() } else { base.chars().take(56).collect() };
    for n in 1.. {
        let name = if n == 1 { base.clone() } else { format!("{base}-{n}") };
        let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE username = ? COLLATE NOCASE)")
            .bind(&name)
            .fetch_one(&db.read)
            .await?;
        if !taken {
            return Ok(name);
        }
    }
    unreachable!()
}

/// A listing read a page at a time.
struct Pager {
    server: Option<Server>,
    path: String,
    query: Vec<(&'static str, String)>,
    start: usize,
    done: bool,
}

impl Pager {
    /// A listing of the server, as the Plex account.
    fn new(path: &str, query: &[(&'static str, String)]) -> Self {
        Self { server: None, path: path.to_owned(), query: query.to_vec(), start: 0, done: false }
    }

    /// A listing of the server, as whoever `server`'s token is.
    fn on(server: Server, path: &str, query: &[(&'static str, String)]) -> Self {
        Self { server: Some(server), ..Self::new(path, query) }
    }

    async fn next(&mut self, run: &Run) -> ImportResult<Option<Vec<Item>>> {
        run.cancelled()?;
        if self.done {
            return Ok(None);
        }
        let server = self.server.as_ref().unwrap_or(&run.server);
        let page = run.plex.page(server, &self.path, &self.query, self.start).await?;
        let got = page.items.len();
        self.start += got;
        // Plex leaves out the total on some listings: a short page is the last.
        self.done = got == 0 || (self.start >= page.total && got < super::plex::PAGE);
        Ok((got > 0).then_some(page.items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_times_the_steps_that_ran_and_the_one_it_stopped_in() {
        let ago = |secs| Some(Instant::now() - Duration::from_secs(secs));
        let row = |step, state, began, took| StepRow { step, state, count: None, began, took };
        let rows = [
            row(Step::Profiles, StepState::Done, ago(100), Some(Duration::from_millis(2600))),
            row(Step::Scanning, StepState::Done, ago(97), Some(Duration::from_secs(60))),
            row(Step::Labels, StepState::Running, ago(37), None),
            row(Step::Fetching, StepState::ToCome, None, None),
        ];
        let times: Vec<(Step, u64)> = step_times(&rows).into_iter().map(|t| (t.step, t.secs)).collect();
        assert_eq!(times, [(Step::Profiles, 3), (Step::Scanning, 60), (Step::Labels, 37)]);
    }

    #[test]
    fn the_steps_listed_are_those_chosen_in_the_order_they_run() {
        let listed = |bring| steps(bring).into_iter().map(|r| r.step).collect::<Vec<_>>();
        let nothing = Bring { watch_history: false, collections: false, edits: false };
        assert_eq!(
            listed(nothing),
            [Step::Profiles, Step::Scanning, Step::Labels, Step::Fetching, Step::MatchingAgain]
        );
        assert_eq!(
            listed(Bring { watch_history: true, ..nothing }),
            [
                Step::Profiles,
                Step::Scanning,
                Step::Labels,
                Step::WatchState,
                Step::History,
                Step::Fetching,
                Step::MatchingAgain
            ]
        );
        assert_eq!(listed(Bring::default()).len(), 10);
        assert!(steps(Bring::default()).iter().all(|r| r.state == StepState::ToCome && r.count.is_none()));
    }
}
