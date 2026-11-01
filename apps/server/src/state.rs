use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::auth::AuthService;
use crate::backup::Backups;
use crate::config::Config;
use crate::db::Db;
use crate::events::EventBus;
use crate::images::Images;
use crate::library::scanner::Scanner;
use crate::library::watcher::LibraryWatcher;
use crate::markers::MarkerService;
use crate::metadata::MetadataAgent;
use crate::playback::Sessions;
use crate::schedule::Scheduler;
use crate::settings::SettingsStore;
use crate::store::Store;
use crate::transcoder::Transcoder;
use crate::trickplay::TrickplayService;

/// Shared application state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

pub struct Inner {
    pub config: Config,
    pub db: Db,
    pub auth: AuthService,
    pub settings: Arc<SettingsStore>,
    pub events: EventBus,
    pub transcoder: Arc<Transcoder>,
    pub metadata: Arc<MetadataAgent>,
    /// Subtitle search and downloads (see [`crate::subtitles`]).
    pub opensubtitles: Arc<crate::providers::opensubtitles::OpenSubtitles>,
    /// Movies and shows people ask for (`crate::requests`).
    pub requests: Arc<crate::requests::Requests>,
    pub store: Arc<Store>,
    pub backups: Arc<Backups>,
    pub scanner: Arc<Scanner>,
    pub watcher: Arc<LibraryWatcher>,
    pub images: Images,
    pub sessions: Arc<Sessions>,
    /// The `sessionKey`s Plex tools know the sessions by.
    pub(crate) plex_session_keys: crate::plex::SessionKeys,
    /// When library work runs (see [`crate::schedule`]).
    pub schedule: Arc<Scheduler>,
    pub markers: Arc<MarkerService>,
    pub trickplay: Arc<TrickplayService>,
    /// Live TV: its sources, channels, guide and connections (see [`crate::livetv`]).
    pub live: Arc<crate::livetv::LiveTv>,
    pub two_step: crate::two_step::TwoStep,
    /// What Automatic quality knows of each client's networks.
    pub bandwidth: crate::bandwidth::Bandwidth,
    /// Sign-ins of apps in progress (see [`crate::oauth`]).
    pub oauth: crate::oauth::AuthorizationServer,
    /// Where the server listens, and wants to (see [`crate::network`]).
    pub network: crate::network::Network,
    /// The processors' and memory's use, for the dashboard.
    pub resources: Arc<crate::resources::Monitor>,
    pub started_at: Instant,
    /// Cancelled when the server starts shutting down (the tray's Quit
    /// cancels it to ask for that). Responses that never end on their own,
    /// such as the event stream, end with it.
    pub stopping: CancellationToken,
}

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.inner
    }
}

impl AppState {
    /// This server's stable id (Plex's `machineIdentifier`).
    pub fn plex_machine_id(&self) -> String {
        crate::plex::PlexInfo::load(&self.config.data_dir).machine_id
    }

    pub async fn init(mut config: Config) -> anyhow::Result<Self> {
        config.check_transcode_dir();
        for dir in [
            config.data_dir.clone(),
            config.cache_dir(),
            config.metadata_dir(),
            config.media_dir(),
            config.transcode_dir.clone(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("cannot create {}: {e}", dir.display()))?;
        }
        crate::util::keep_private(&config.data_dir);
        crate::util::check_program_folder();
        // A transcode folder of its own choosing (`/tmp`, say) stays as it
        // is: the transcoder keeps what it makes there private.
        if config.transcode_dir.starts_with(&config.data_dir) {
            crate::util::keep_private(&config.transcode_dir);
        }
        let db = Db::open(&config.db_path()).await?;
        let backups = Backups::new(db.clone(), &config.data_dir);
        crate::ratings::backfill(&db).await?;
        let settings = Arc::new(SettingsStore::load(&db).await?);
        let events = EventBus::new();
        let transcoder = Transcoder::new(&config);
        let store = Arc::new(Store::new(db.clone(), &config.metadata_dir(), &config.media_dir(), &config.cache_dir()));
        if let Err(err) = store.migrate_legacy(&config.metadata_dir()).await {
            tracing::warn!("moving pictures into the blob store failed: {err:#}");
        }
        let metadata = MetadataAgent::with_endpoints(
            db.clone(),
            settings.clone(),
            store.clone(),
            config.metadata_endpoints.clone(),
        );
        let opensubtitles = Arc::new(crate::providers::opensubtitles::OpenSubtitles::new(
            settings.clone(),
            config.metadata_endpoints.as_ref().map(|e| e.opensubtitles.clone()),
        ));
        let scanner =
            Scanner::new(db.clone(), transcoder.clone(), events.clone(), metadata.clone(), store.clone(), &config);
        // Discover's pictures, from TMDB's picture server (none in tests
        // without endpoints).
        let pictures = crate::discover::pictures::Pictures::new(
            config.cache_dir().join("discover"),
            config.metadata_endpoints.as_ref().map(|e| e.tmdb_images.clone()),
        );
        let requests =
            crate::requests::Requests::new(db.clone(), settings.clone(), events.clone(), metadata.clone(), pictures);
        let watcher = LibraryWatcher::new(db.clone(), scanner.clone(), settings.clone());
        let images = Images::new(config.cache_dir().join("images"));
        let sessions = Arc::new(Sessions::default());
        let stopping = CancellationToken::new();
        let resources = crate::resources::Monitor::new();
        let schedule = Scheduler::new(db.clone(), settings.clone(), sessions.clone(), resources.clone());
        let markers = MarkerService::new(
            db.clone(),
            transcoder.clone(),
            settings.clone(),
            sessions.clone(),
            schedule.clone(),
            events.clone(),
            config.cache_dir().join("fingerprints"),
            stopping.clone(),
        );
        // A transcoder elsewhere reads the files as its own mounts present
        // them: they count as one storage.
        let disks = Arc::new(crate::disks::Disks::new(config.trickplay_disks.clone(), !transcoder.is_managed()));
        let trickplay = TrickplayService::new(
            db.clone(),
            transcoder.clone(),
            settings.clone(),
            sessions.clone(),
            schedule.clone(),
            store.clone(),
            disks,
            config.trickplay_pace,
            config.cache_dir(),
            stopping.clone(),
        );
        let live =
            crate::livetv::LiveTv::new(db.clone(), events.clone(), transcoder.clone(), config.cache_dir()).await?;
        let auth = AuthService::new(db.clone());
        let oauth = crate::oauth::AuthorizationServer::new(&config);
        let two_step = crate::two_step::TwoStep::new(db.clone(), &config.data_dir);
        two_step.check_key().await;
        let require_two_step = settings.get().require_two_step;
        if require_two_step && !two_step.can_set_up() {
            tracing::error!(
                "two-step sign-in is required, but the server can't use its {}: until it can, accounts that haven't \
                 set it up can't sign in, and accounts that have sign in with a recovery code",
                crate::two_step::KEY_FILE
            );
        }
        auth.require_two_step(require_two_step);
        // After two-step sign-in, which makes the key it derives its own from.
        let bandwidth = crate::bandwidth::Bandwidth::new(db.clone(), &config.data_dir);
        let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db.read).await?;
        if users == 0 {
            auth.setup.issue();
        }
        Ok(Self {
            inner: Arc::new(Inner {
                network: crate::network::Network::new(&config),
                resources,
                auth,
                sessions,
                plex_session_keys: Default::default(),
                schedule,
                markers,
                trickplay,
                live,
                two_step,
                bandwidth,
                oauth,
                started_at: Instant::now(),
                stopping,
                config,
                db,
                settings,
                events,
                transcoder,
                metadata,
                opensubtitles,
                requests,
                store,
                backups,
                scanner,
                watcher,
                images,
            }),
        })
    }

    pub fn start_background_tasks(&self) {
        self.transcoder.start();
        self.sessions.spawn_reaper(self.transcoder.clone(), self.events.clone());
        self.markers.start();
        self.resources.start(self.stopping.clone());
        self.schedule.start(self.stopping.clone());
        self.live.start(self.stopping.clone());
        self.requests.start(&self.events, self.stopping.clone());
        let metadata = self.metadata.clone();
        tokio::spawn(async move {
            metadata.prune_people().await;
            metadata.backfill_credits(None).await;
        });
        // Check the database, then back it up once a day (right away when the
        // last backup is older than that).
        let backups = self.backups.clone();
        tokio::spawn(async move {
            backups.check().await;
            if backups.health().ok != Some(true) {
                return;
            }
            let last = backups.list().await.first().map(|b| b.created_at).unwrap_or(0);
            let wait = (last + 24 * 3600 - crate::util::now()).max(60) as u64;
            tokio::time::sleep(Duration::from_secs(wait.min(24 * 3600))).await;
            let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
            loop {
                tick.tick().await;
                if let Err(err) = backups.create().await {
                    tracing::error!("database backup failed: {err:#}");
                }
            }
        });
        // Seek previews of whole libraries after each scan.
        let (trickplay, mut events) = (self.trickplay.clone(), self.events.subscribe());
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(crate::events::ServerEvent::ScanFinished { .. }) => trickplay.request_all().await,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    _ => {}
                }
            }
        });
        // Fingerprints of files scanned by older versions (then previews of
        // what lacks them), and a daily clean-up.
        let (store, trickplay) = (self.store.clone(), self.trickplay.clone());
        tokio::spawn(async move {
            store.fingerprint_pending().await;
            trickplay.request_all().await;
            let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
            loop {
                tick.tick().await;
                if let Err(err) = store.clean().await {
                    tracing::warn!("storage clean-up failed: {err:#}");
                }
            }
        });
        let watcher = self.watcher.clone();
        tokio::spawn(async move { watcher.reload().await });
        let state = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(3600));
            loop {
                tick.tick().await;
                state.auth.purge_expired().await;
                state.oauth.sweep();
            }
        });
    }

    pub async fn shutdown(&self) {
        self.stopping.cancel();
        // External transcoders keep running: delete our jobs there. A managed
        // one stops all of its jobs itself when it shuts down.
        if !self.transcoder.is_managed() {
            let jobs: Vec<String> = self.sessions.list().iter().filter_map(|s| s.job_id()).collect();
            let deletes = futures::future::join_all(jobs.iter().map(|job| self.transcoder.delete_job(job)));
            let _ = tokio::time::timeout(Duration::from_secs(5), deletes).await;
        }
        self.transcoder.shutdown().await;
        self.db.close().await;
    }
}
