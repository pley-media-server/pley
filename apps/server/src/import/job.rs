//! The Plex import as it runs: signing in to Plex, choosing its server,
//! the plan (each Plex library paired with Pley's, each Home user with the
//! limits they'd get), and the job that brings it all over.
//!
//! The Plex token is kept in memory only, for as long as the import needs
//! it: it's forgotten when the administrator cancels or the server stops.
//! One import runs at a time.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::accounts::{self, MAX_LABEL_RULES, NO_PASSWORD};
use crate::db::Db;
use crate::events::ServerEvent;
use crate::library::LibraryKind;
use crate::state::AppState;
use crate::util::now;

use super::matching::{Folder, Index, Matches, folder_for};
use super::plex::{HomeUser, Item, Plex, PlexError, Resource, Section, Server, UserFilters};
use super::restrictions::{self, Limits};

/// Where the profiles an import made are remembered (`import_accounts`).
const SOURCE: &str = "plex";
/// The settings key of the last import's report.
const REPORT_KEY: &str = "plex_import_report";
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
}

impl Session {
    /// Forgets the account's token and what was chosen with it.
    fn signed_out(&mut self) {
        self.token = None;
        self.servers.clear();
        self.server = None;
        self.server_name.clear();
    }
}

/// Where the import is, as `GET /api/import/plex` says it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    pub state: RunState,
    pub step: Option<Step>,
    pub done: u64,
    pub total: u64,
    /// Why it stopped, when it failed.
    pub error: Option<String>,
    pub report: Option<Report>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Scanning,
    Profiles,
    Matching,
    Labels,
    WatchState,
    History,
}

/// What an import brought over, and what it couldn't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
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
    /// Plex refused some of their lists: what came over is incomplete.
    #[serde(default)]
    pub incomplete: bool,
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
    pub pairing: Pairing,
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
    pub async fn plan(&self, db: &Db) -> ImportResult<Plan> {
        let mut session = self.session.lock().await;
        let token = session.token.clone().ok_or(ImportError::NotSignedIn)?;
        let server = session.server.clone().ok_or(ImportError::NoServer)?;
        let name = session.server_name.clone();
        let plex = self.plex(db, &mut session).await?;
        drop(session);

        let mut libraries = Vec::new();
        for section in plex.sections(&server).await? {
            let Some(kind) = pley_kind(&section.kind) else { continue };
            let locations: Vec<String> = section.location.iter().map(|l| l.path.clone()).collect();
            let pairing = pair(db, &plex, &server, &section.key, kind, &locations).await?;
            libraries.push(PlanLibrary {
                key: section.key,
                title: section.title,
                kind: kind.as_str(),
                locations,
                pairing,
            });
        }

        let home = plex.home_users(&token).await?;
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
            });
        }
        Ok(Plan { server: name, libraries, users })
    }

    // ---- The job ----

    /// Where the import is: the one running, or the last one's report.
    pub async fn progress(&self, db: &Db) -> Progress {
        let progress = self.progress.lock().clone();
        if progress.state != RunState::Idle {
            return progress;
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
            *progress = Progress { state: RunState::Running, ..Default::default() };
        }
        self.cancel.store(false, Ordering::Relaxed);
        drop(session);
        let run =
            Run { state: state.clone(), importer: self.clone(), plex, token, server, admin, report: Report::default() };
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

async fn imported_profile(db: &Db, plex_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT user_id FROM import_accounts WHERE source = ? AND external_id = ?")
        .bind(SOURCE)
        .bind(plex_id.to_string())
        .fetch_optional(&db.read)
        .await
}

/// Pairs Plex library `key` with Pley's: some of its files, from spots
/// spread over it, looked for among Pley's by name and size.
async fn pair(
    db: &Db,
    plex: &Plex,
    server: &Server,
    key: &str,
    kind: LibraryKind,
    locations: &[String],
) -> ImportResult<Pairing> {
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
        let hits: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT f.path, f.library_id, l.kind FROM media_files f JOIN libraries l ON l.id = f.library_id
             WHERE f.size = ? AND (substr(f.path, -length(?) - 1) IN ('/' || ?, '\\' || ?))",
        )
        .bind(size)
        .bind(name)
        .bind(name)
        .bind(name)
        .fetch_all(&db.read)
        .await?;
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
    Ok(Pairing { status, library_id: best.map(|(id, _)| id), folders, folders_here })
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
}

/// A library being brought: Plex's, Pley's.
struct Paired {
    key: String,
    kind: LibraryKind,
    library_id: i64,
    report: usize,
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
        let outcome = self.run(choices).await;
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
        self.importer.set(&self.state, self.admin, |p| {
            p.state = state;
            p.error = error;
            p.step = None;
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

    fn step(&self, step: Step, total: u64) {
        self.importer.set(&self.state, self.admin, |p| {
            p.step = Some(step);
            p.done = 0;
            p.total = total;
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
        self.wait_for_scans(&paired).await?;

        self.step(Step::Matching, paired.len() as u64);
        let pley_libraries: Vec<i64> = paired.iter().map(|p| p.library_id).collect();
        let index = Index::load(&self.state.db, &pley_libraries).await?;
        let mut matches = Matches::default();
        for library in &paired {
            self.find(library, &index, &folders, &mut matches).await?;
            self.advance(1);
        }

        self.step(Step::Labels, paired.len() as u64);
        for library in &paired {
            self.labels(library, &matches).await?;
            self.advance(1);
        }

        self.step(Step::WatchState, people.len() as u64);
        for person in &people {
            let done = self.watch_state(person, &paired, &matches).await;
            self.person_done(person, done)?;
            self.state.events.send(ServerEvent::OwnLibrariesChanged { user_id: person.user_id });
            self.advance(1);
        }

        self.step(Step::History, people.len() as u64);
        let devices: HashMap<i64, String> =
            self.plex.devices(&self.server).await.unwrap_or_default().into_iter().map(|d| (d.id, d.name)).collect();
        for person in &people {
            let done = self.history(person, &matches, &devices).await;
            self.person_done(person, done)?;
            self.advance(1);
        }
        Ok(())
    }

    /// Plex refusing one person's lists leaves theirs incomplete, not
    /// everyone's; a cancel or Pley's own error stops the import.
    fn person_done(&mut self, person: &Person, done: ImportResult<()>) -> ImportResult<()> {
        match done {
            Ok(()) => Ok(()),
            Err(ImportError::Plex(e)) => {
                tracing::debug!("the Plex import couldn't read all of one person's watch state: {e}");
                self.report.profiles[person.report].incomplete = true;
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
                    let id = crate::api::add_library(&self.state, &new.name, kind, &new.paths)
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

    /// Waits for the libraries' scans: what's matched is what they find.
    async fn wait_for_scans(&self, paired: &[Paired]) -> ImportResult<()> {
        let scanning = || paired.iter().filter(|p| self.state.scanner.is_scanning(p.library_id)).count();
        let waiting = scanning();
        if waiting == 0 {
            return Ok(());
        }
        self.step(Step::Scanning, waiting as u64);
        loop {
            self.cancelled()?;
            let left = scanning();
            self.importer.set(&self.state, self.admin, |p| p.done = (waiting - left.min(waiting)) as u64);
            if left == 0 {
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
                .bind(Some(user.title.trim()).filter(|t| !t.is_empty()))
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
        for (kind, list) in [("show", &show), ("hide", &hide)] {
            for label in list {
                sqlx::query("INSERT OR REPLACE INTO user_label_rules (user_id, label, kind) VALUES (?, ?, ?)")
                    .bind(id)
                    .bind(label)
                    .bind(kind)
                    .execute(&mut *tx)
                    .await?;
            }
        }
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
    /// Plex's when earlier.
    async fn find(
        &mut self,
        library: &Paired,
        index: &Index,
        folders: &[Folder],
        matches: &mut Matches,
    ) -> ImportResult<()> {
        let mut retry: Vec<Item> = Vec::new();
        let (mut matched, mut missed, mut named) = (0u64, 0u64, Vec::new());
        for (plex_type, kind) in types(library.kind) {
            let query = [("type", plex_type.to_string()), ("includeGuids", "1".to_owned())];
            let mut pager = Pager::new(&format!("/library/sections/{}/all", library.key), &query);
            while let Some(items) = pager.next(self).await? {
                let mut dates = Vec::new();
                for item in items {
                    match matches.find(index, library.library_id, folders, &item) {
                        Some(id) => {
                            if is_leaf(kind) {
                                matched += 1;
                            }
                            if let Some(added) = item.added_at.filter(|a| *a > 0) {
                                dates.push((id, added));
                            }
                        }
                        None if *kind == "episode" => retry.push(item),
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
        let report = &mut self.report.libraries[library.report];
        report.matched = matched;
        report.missed = missed;
        report.missed_titles = named;
        Ok(())
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
