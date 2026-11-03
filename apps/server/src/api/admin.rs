use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Query, State};
use pley_transcoder_api::Counters;
use serde::{Deserialize, Serialize};

use crate::auth::AdminUser;
use crate::error::{ApiResult, AppError};
use crate::network::{self, Adapter, Fixed, Listen, Listener};
use crate::playback::{TranscodeInfo, transcode_info};
use crate::providers::{Check, ProviderError};
use crate::requests::arr;
use crate::settings::{AdminSettings, SettingsPatch};
use crate::state::AppState;

// ------------------------------------------------------------------ settings

pub async fn get_settings(State(state): State<AppState>, _admin: AdminUser) -> Json<AdminSettings> {
    Json(state.settings.get().for_admin())
}

pub async fn put_settings(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(mut patch): Json<SettingsPatch>,
) -> ApiResult<Json<AdminSettings>> {
    let before = state.settings.get();
    if patch.require_two_step == Some(true) && !before.require_two_step {
        // Whoever turns it on uses it, so nobody locks themselves out, and
        // everyone else can set it up.
        if !state.two_step.can_set_up() {
            return Err(AppError::bad_request(format!(
                "two-step sign-in can't be set up on this server: it can't use its {} (the log says why)",
                crate::two_step::KEY_FILE
            )));
        }
        if !state.two_step.is_on(admin.id).await? {
            return Err(AppError::bad_request(
                "turn on two-step sign-in for your own account first (Account → Two-step sign-in)",
            ));
        }
    }
    // A choice that can't work here is refused; one saved before (another
    // GPU, another FFmpeg) stays, and the transcoder does without it.
    let info = state.transcoder.info();
    let caps = info.as_ref().map(|info| info.capabilities.clone());
    let os = transcoder_os(&state);
    if let Some(hwaccel) = patch.hwaccel.filter(|v| *v != before.hwaccel)
        && let Some(why) = crate::settings::hwaccel_problem(hwaccel, os, caps.as_ref())
    {
        return Err(AppError::bad_request(why));
    }
    if let Some(mode) = patch.tonemapping.filter(|v| *v != before.tonemapping)
        && let Some(why) = crate::settings::tonemap_problem(mode, caps.as_ref())
    {
        return Err(AppError::bad_request(why));
    }
    if patch.transcode_in_memory == Some(true)
        && !before.transcode_in_memory
        && let Some(why) = crate::settings::memory_problem(info.as_ref())
    {
        return Err(AppError::bad_request(format!("transcoding in memory isn't available here: {why}")));
    }
    // What goes with the hardware acceleration chosen, whatever the hardware.
    crate::settings::fit_to_accel(&before, &mut patch).map_err(AppError::bad_request)?;
    let after = state.settings.update(&state.db, patch).await?;
    if before.watch_libraries != after.watch_libraries {
        state.watcher.reload().await;
    }
    // Apps by other publishers turned off: they are signed out at once.
    if before.third_party_apps && !after.third_party_apps {
        let ended = state.auth.end_grants_except_clients(crate::oauth::clients::BUILTIN_IDS).await?;
        tracing::info!("apps by other publishers were turned off: {} signed out", ended.len());
    }
    if before.require_two_step != after.require_two_step {
        require_two_step(&state, after.require_two_step).await?;
    }
    // A new key or source: fetch metadata for everything that has none yet
    // (shows are fetched again from a new TV source).
    if crate::providers::settings_changed(&state.db, &before, &after).await? {
        state.scanner.enqueue_all(false).await?;
    }
    // Requests turned on or off, other limits, another TMDB key: every app
    // fetches its summary again.
    if before.requests != after.requests
        || before.request_limit_movies != after.request_limit_movies
        || before.request_limit_seasons != after.request_limit_seasons
        || before.tmdb_api_key != after.tmdb_api_key
    {
        state.events.send(crate::events::ServerEvent::RequestsSettingsChanged { user_id: None });
    }
    Ok(Json(after.for_admin()))
}

/// Requires two-step sign-in of every account with a password, or stops
/// requiring it. Turned on, whoever signs in with a password alone is
/// signed out everywhere, browsers and apps, and sets it up at their next
/// sign-in; sessions and apps that slip past this (one signing in at that
/// moment) stop working anyway ([`crate::auth::AuthService::require_two_step`]).
async fn require_two_step(state: &AppState, on: bool) -> ApiResult<()> {
    state.auth.require_two_step(on);
    if !on {
        tracing::info!("two-step sign-in is no longer required");
        return Ok(());
    }
    let without = state.two_step.accounts_without().await?;
    for &id in &without {
        state.oauth.forget_user(id);
        state.auth.sign_out_everywhere(id, None).await?;
    }
    tracing::info!(
        "two-step sign-in is required: {} account(s) without it were signed out, and set it up at their next sign-in",
        without.len()
    );
    Ok(())
}

/// The transcoder's operating system, when known: a managed one runs on
/// pley's computer, an external one anywhere.
fn transcoder_os(state: &AppState) -> Option<&'static str> {
    state.transcoder.is_managed().then_some(std::env::consts::OS)
}

/// A service the settings can test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Tmdb,
    Tvdb,
    Tvmaze,
    Musicbrainz,
    Transcoder,
    Opensubtitles,
}

impl Service {
    fn name(self) -> &'static str {
        match self {
            Service::Tmdb => "TMDB",
            Service::Tvdb => "TheTVDB",
            Service::Tvmaze => "TVmaze",
            Service::Musicbrainz => "MusicBrainz",
            Service::Transcoder => "The transcoder",
            Service::Opensubtitles => "OpenSubtitles",
        }
    }
}

#[derive(Deserialize)]
pub struct TestRequest {
    service: Service,
    /// A key typed in the form, maybe not saved yet; the saved one otherwise.
    #[serde(default)]
    api_key: Option<String>,
    /// TheTVDB's subscriber PIN, likewise.
    #[serde(default)]
    pin: Option<String>,
    /// An OpenSubtitles account, likewise (the password: the saved one when
    /// left out and the username is the saved one).
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    ok: bool,
    message: String,
    /// How long the answer took.
    ms: u64,
}

/// How long a test waits for an answer.
const TEST_TIMEOUT: Duration = Duration::from_secs(15);

/// `POST /api/settings/test`: whether a metadata service accepts a key (the
/// one typed in, before it is saved) and answers, or whether the transcoder
/// does. Each test sends one request to the service's usual address.
pub async fn test_service(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<TestRequest>,
) -> Json<TestResult> {
    let settings = state.settings.get();
    if req.service == Service::Opensubtitles {
        return Json(test_opensubtitles(&state, &req).await);
    }
    let saved = if req.service == Service::Tvdb { &settings.tvdb_api_key } else { &settings.tmdb_api_key };
    let key = req.api_key.as_deref().unwrap_or(saved).trim().to_owned();
    let pin = req.pin.as_deref().unwrap_or(&settings.tvdb_pin).trim().to_owned();
    let name = req.service.name();
    let fail = |message: String, ms: u64| Json(TestResult { ok: false, message, ms });
    if matches!(req.service, Service::Tmdb | Service::Tvdb) && key.is_empty() {
        return fail(format!("Enter a {name} API key first."), 0);
    }

    let started = Instant::now();
    let outcome = tokio::time::timeout(TEST_TIMEOUT, async {
        match req.service {
            Service::Tmdb => state.metadata.check_tmdb(&key).await.map(|()| None),
            Service::Tvdb => state.metadata.check_provider(Check::Tvdb { key: &key, pin: &pin }).await.map(|()| None),
            Service::Tvmaze => state.metadata.check_provider(Check::Tvmaze).await.map(|()| None),
            Service::Musicbrainz => state.metadata.check_provider(Check::MusicBrainz).await.map(|()| None),
            Service::Opensubtitles => unreachable!("tested above"),
            Service::Transcoder => match state.transcoder.status().await {
                Ok(status) => Ok(Some(match status.ffmpeg_version {
                    Some(ffmpeg) => format!("pley-transcoder {} with ffmpeg {ffmpeg}", status.version),
                    None => format!("pley-transcoder {}", status.version),
                })),
                Err(crate::transcoder::TranscoderError::Unavailable(why)) => Err(ProviderError::Other(
                    anyhow::anyhow!("it isn't running{}", why.map(|w| format!(" ({w})")).unwrap_or_default()),
                )),
                Err(err) => Err(ProviderError::Other(anyhow::anyhow!("{err}"))),
            },
        }
    })
    .await;
    let ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(Ok(detail)) => Json(TestResult {
            ok: true,
            message: match (req.service, detail) {
                (Service::Tmdb, _) => "TMDB accepted the key.".into(),
                (Service::Tvdb, _) if !pin.is_empty() => "TheTVDB accepted the key and PIN.".into(),
                (Service::Tvdb, _) => "TheTVDB accepted the key.".into(),
                (_, Some(detail)) => format!("{name} answered: {detail}."),
                (_, None) => format!("{name} answered."),
            },
            ms,
        }),
        Ok(Err(ProviderError::Unauthorized(_))) => fail(
            match req.service {
                Service::Tvdb if pin.is_empty() => {
                    "TheTVDB rejected this key. A user-supported key also needs your subscriber PIN.".into()
                }
                Service::Tvdb => "TheTVDB rejected this key and PIN.".into(),
                _ => format!(
                    "{name} rejected this key. Use the API key or the API read access token from themoviedb.org/settings/api."
                ),
            },
            ms,
        ),
        Ok(Err(ProviderError::NotFound)) => fail(format!("{name} answered, but not as expected (HTTP 404)."), ms),
        Ok(Err(ProviderError::Other(err))) => fail(format!("Couldn't reach {}: {err:#}", object(name)), ms),
        Err(_) => fail(format!("{name} didn't answer within {} seconds.", TEST_TIMEOUT.as_secs()), ms),
    }
}

/// The OpenSubtitles key and account typed in (or saved): whether
/// OpenSubtitles takes them.
async fn test_opensubtitles(state: &AppState, req: &TestRequest) -> TestResult {
    use crate::providers::opensubtitles::SearchError;
    let settings = state.settings.get();
    let key = req.api_key.as_deref().unwrap_or(&settings.opensubtitles_api_key).trim().to_owned();
    let username = req.username.as_deref().unwrap_or(&settings.opensubtitles_username).trim().to_owned();
    let password = match req.password.as_deref().filter(|p| !p.is_empty()) {
        Some(password) => password.to_owned(),
        None if username == settings.opensubtitles_username => settings.opensubtitles_password.clone(),
        None => String::new(),
    };
    if key.is_empty() {
        return TestResult { ok: false, message: "Enter an OpenSubtitles API key first.".into(), ms: 0 };
    }
    if !username.is_empty() && password.is_empty() {
        return TestResult { ok: false, message: "Enter the account's password too.".into(), ms: 0 };
    }
    let started = Instant::now();
    let outcome = tokio::time::timeout(TEST_TIMEOUT, state.opensubtitles.check(&key, &username, &password)).await;
    let ms = started.elapsed().as_millis() as u64;
    let (ok, message) = match outcome {
        Ok(Ok(message)) => (true, message),
        Ok(Err(SearchError::Rejected(what))) => (false, format!("OpenSubtitles rejected {what}.")),
        Ok(Err(SearchError::Other(err))) => (false, format!("Couldn't reach OpenSubtitles: {err:#}")),
        Ok(Err(err)) => (false, format!("{err}.")),
        Err(_) => (false, format!("OpenSubtitles didn't answer within {} seconds.", TEST_TIMEOUT.as_secs())),
    };
    TestResult { ok, message, ms }
}

/// "The transcoder" in the middle of a sentence.
fn object(name: &str) -> String {
    match name.strip_prefix("The ") {
        Some(rest) => format!("the {rest}"),
        None => name.to_owned(),
    }
}

#[derive(Deserialize)]
pub struct ArrRequest {
    service: arr::Service,
    /// An address typed in the form, maybe not saved yet; the saved one otherwise.
    #[serde(default)]
    url: Option<String>,
    /// Likewise its key: the saved one goes to the saved address only.
    #[serde(default)]
    api_key: Option<String>,
}

/// `POST /api/settings/arr`: what Radarr or Sonarr offers requests (its
/// quality profiles, root folders and language profiles), asked at the
/// address and with the key typed in, before they are saved.
pub async fn arr_options(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<ArrRequest>,
) -> ApiResult<Json<arr::Options>> {
    let settings = state.settings.get();
    let name = req.service.name();
    let (saved_url, saved_key) = match req.service {
        arr::Service::Radarr => (&settings.radarr_url, &settings.radarr_api_key),
        arr::Service::Sonarr => (&settings.sonarr_url, &settings.sonarr_api_key),
    };
    let url = match req.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => crate::settings::arr_url(url)?,
        None => saved_url.clone(),
    };
    if url.is_empty() {
        return Err(AppError::bad_request(format!("give {name}'s address")));
    }
    let key = match req.api_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty()) {
        Some(key) => key,
        None if url == *saved_url && !saved_key.is_empty() => saved_key.clone(),
        None if url == *saved_url => return Err(AppError::bad_request(format!("give {name}'s API key"))),
        None => {
            let message =
                format!("type {name}'s API key for this address: the saved one goes to the saved address only");
            return Err(AppError::bad_request(message));
        }
    };
    let to = arr::Connection { url, key };
    let options = state.requests.arr().options(req.service, &to).await;
    options.map(Json).map_err(|err| AppError::bad_request(err.to_string()))
}

// ------------------------------------------------------------------ network

/// Where pley listens, and could: Settings → Network.
#[derive(Debug, Serialize)]
pub struct NetworkInfo {
    #[serde(flatten)]
    listen: Listen,
    fixed: Fixed,
    listeners: Vec<Listener>,
    adapters: Vec<Adapter>,
    /// Where a change is saved.
    config_file: String,
    /// Windows Firewall decides which kinds of network reach pley.
    windows: bool,
}

async fn network_info(state: &AppState) -> ApiResult<NetworkInfo> {
    let adapters = tokio::task::spawn_blocking(network::adapters).await.map_err(anyhow::Error::from)?;
    Ok(NetworkInfo {
        listen: state.network.wanted(),
        fixed: state.network.fixed,
        listeners: state.network.listeners(),
        adapters,
        config_file: state.network.config_file.display().to_string(),
        windows: cfg!(windows),
    })
}

pub async fn get_network(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<NetworkInfo>> {
    Ok(Json(network_info(&state).await?))
}

/// Listens as `listen` says from now on, and from the next start on.
pub async fn put_network(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Json(mut listen): Json<Listen>,
) -> ApiResult<Json<NetworkInfo>> {
    // This computer's address first: pley must listen on the first as it
    // starts, and an adapter's may be gone by then (DHCP, a VPN off).
    listen.web.sort_by_key(|addr| !addr.ip().is_loopback());
    // An adapter chosen twice is chosen once, by its name as the system
    // gives it.
    for name in &mut listen.bonjour_adapters {
        *name = name.trim().to_owned();
    }
    listen.bonjour_adapters.sort();
    listen.bonjour_adapters.dedup();
    listen.check().map_err(AppError::bad_request)?;
    let (now, fixed) = (state.network.wanted(), state.network.fixed);
    if (fixed.web && listen.web != now.web)
        || (fixed.jellyfin && (listen.jellyfin != now.jellyfin || listen.jellyfin_on_web != now.jellyfin_on_web))
        || (fixed.discovery && listen.discovery != now.discovery)
        || (fixed.bonjour && listen.bonjour != now.bonjour)
    {
        return Err(AppError::bad_request("the command line or the environment sets that: change it there"));
    }
    if listen != now {
        let addresses =
            |list: &[std::net::SocketAddr]| list.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        tracing::info!(
            "{} changed where Pley listens: {} (the Jellyfin-compatible API: {}; discovery {}; Bonjour {})",
            user.username,
            addresses(&listen.web),
            if listen.jellyfin_on_web {
                "on Pley's port".to_owned()
            } else if listen.jellyfin.is_empty() {
                "off".to_owned()
            } else {
                addresses(&listen.jellyfin)
            },
            if listen.discovery { "on" } else { "off" },
            if listen.bonjour { "on" } else { "off" },
        );
        state.network.change(listen).await.map_err(|err| AppError::Unavailable(format!("{err:#}")))?;
    }
    Ok(Json(network_info(&state).await?))
}

// ------------------------------------------------------------------ system

/// The managed transcoder's work folder (Settings → Transcoder).
#[derive(Serialize)]
pub struct TranscodeDir {
    /// The folder it works in now.
    in_use: String,
    /// The one pley's next start uses, when that is another.
    next: Option<String>,
    /// `transcode` in the data folder: used when none is chosen, or the one
    /// chosen can't be.
    default: String,
    /// The folder chosen, when it couldn't be used at start, and why.
    problem: Option<TranscodeDirProblem>,
    /// The command line or the environment chooses it: Settings can't.
    fixed: bool,
}

#[derive(Serialize)]
pub struct TranscodeDirProblem {
    folder: String,
    reason: String,
}

fn transcode_dir(state: &AppState) -> TranscodeDir {
    let config = &state.config;
    let default = config.default_transcode_dir();
    let saved = || crate::config::saved_transcode_dir(&config.config_file).unwrap_or_else(|| default.clone());
    let failed = config.transcode_dir_problem.as_ref().map(|(dir, _)| dir);
    let next =
        (!config.transcode_dir_fixed).then(saved).filter(|next| *next != config.transcode_dir && Some(next) != failed);
    TranscodeDir {
        in_use: config.transcode_dir.to_string_lossy().into_owned(),
        next: next.map(|dir| dir.to_string_lossy().into_owned()),
        default: default.to_string_lossy().into_owned(),
        problem: config.transcode_dir_problem.as_ref().map(|(dir, reason)| TranscodeDirProblem {
            folder: dir.to_string_lossy().into_owned(),
            reason: reason.clone(),
        }),
        fixed: config.transcode_dir_fixed,
    }
}

#[derive(Deserialize)]
pub struct TranscodeDirChange {
    /// `null` (or blank): the default one.
    path: Option<String>,
}

/// Chooses the transcoder's work folder from pley's next start: the
/// transcoder can't move what it is making while it runs.
pub async fn put_transcode_dir(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Json(change): Json<TranscodeDirChange>,
) -> ApiResult<Json<TranscodeDir>> {
    let config = &state.config;
    if config.transcode_dir_fixed {
        return Err(AppError::bad_request("the command line or the environment sets that: change it there"));
    }
    let default = config.default_transcode_dir();
    let dir = change.path.as_deref().map(str::trim).filter(|path| !path.is_empty()).map(PathBuf::from);
    let dir = dir.filter(|dir| *dir != default);
    if let Some(dir) = &dir {
        if !dir.is_absolute() {
            return Err(AppError::bad_request("give the folder's whole path"));
        }
        if dir.parent().is_none() {
            return Err(AppError::bad_request(
                "choose a folder, not a whole drive: the transcoder keeps several folders of its own in it",
            ));
        }
        if *dir == config.data_dir {
            return Err(AppError::bad_request("choose a folder of its own, not Pley's data folder"));
        }
        let check = dir.clone();
        tokio::task::spawn_blocking(move || crate::config::usable_folder(&check))
            .await
            .map_err(|err| AppError::Internal(err.into()))?
            .map_err(|err| AppError::bad_request(format!("Pley can't write in {}: {err}", dir.display())))?;
    }
    crate::config::save_transcode_dir(&config.config_file, dir.as_deref())?;
    tracing::info!(
        "{} chose {} as the transcode folder, from Pley's next start",
        user.username,
        dir.as_deref().unwrap_or(&default).display()
    );
    Ok(Json(transcode_dir(&state)))
}

#[derive(Serialize)]
pub struct TranscoderSummary {
    available: bool,
    managed: bool,
    url: Option<String>,
    version: Option<String>,
    commit: Option<String>,
    license: Option<String>,
    ffmpeg_version: Option<String>,
    encoders: Vec<String>,
    active_encoder: Option<String>,
    hwaccels: Vec<String>,
    /// Hardware accelerations with a working H.264 encoder, best first:
    /// the first is the one Automatic uses.
    accels: Vec<pley_transcoder_api::HwAccel>,
    /// Some video is decoded on a GPU: `hardware_decoding` changes something.
    hardware_decoding: bool,
    /// What each hardware acceleration choice does here (empty while the
    /// transcoder is unavailable).
    with_accel: HashMap<pley_transcoder_api::HwAccel, crate::settings::AccelUse>,
    tonemapping: Vec<String>,
    dolby_vision: bool,
    /// Audio fingerprints, which find intros and credits that chapters don't name.
    chromaprint: bool,
    /// FFmpeg can hold transcodes back (6.1 and newer).
    throttling: bool,
    /// Whether segments can be kept in memory, and how much (`null` while
    /// the transcoder is unavailable, or from one too old to say).
    memory: Option<pley_transcoder_api::MemoryInfo>,
    max_jobs: u32,
    active_jobs: u32,
    error: Option<String>,
}

pub async fn system(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<serde_json::Value>> {
    let info = state.transcoder.info();
    let settings = state.settings.get();
    let active_jobs = match &info {
        Some(_) => state.transcoder.status().await.map(|s| s.running_processes).unwrap_or(0),
        None => 0,
    };
    let transcoder = TranscoderSummary {
        available: info.is_some(),
        managed: state.transcoder.is_managed(),
        url: state.transcoder.base_url(),
        version: info.as_ref().map(|i| i.version.clone()),
        commit: info.as_ref().and_then(|i| i.commit.clone()),
        license: info.as_ref().map(|i| i.license.clone()),
        ffmpeg_version: info.as_ref().and_then(|i| i.capabilities.ffmpeg_version.clone()),
        encoders: info
            .as_ref()
            .map(|i| i.capabilities.encoders.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default(),
        active_encoder: info.as_ref().and_then(|i| {
            let caps = &i.capabilities;
            let wanted = settings.hwaccel;
            let hardware = caps.encoders.iter().find(|e| {
                e.codec == pley_transcoder_api::VideoCodec::H264
                    && e.hardware
                    && (wanted == pley_transcoder_api::HwAccel::Auto || e.accel == wanted)
            });
            let software =
                caps.encoders.iter().find(|e| e.codec == pley_transcoder_api::VideoCodec::H264 && !e.hardware);
            match wanted {
                pley_transcoder_api::HwAccel::None => software,
                _ => hardware.or(software),
            }
            .map(|e| e.name.clone())
        }),
        hwaccels: info.as_ref().map(|i| i.capabilities.hwaccels.clone()).unwrap_or_default(),
        accels: info
            .as_ref()
            .map(|i| {
                let mut accels = Vec::new();
                for e in &i.capabilities.encoders {
                    if e.hardware && e.codec == pley_transcoder_api::VideoCodec::H264 && !accels.contains(&e.accel) {
                        accels.push(e.accel);
                    }
                }
                accels
            })
            .unwrap_or_default(),
        hardware_decoding: info.as_ref().is_some_and(|i| crate::settings::decodes_on_gpu(&i.capabilities)),
        with_accel: info
            .as_ref()
            .map(|i| {
                crate::settings::ACCELS
                    .into_iter()
                    .map(|accel| (accel, crate::settings::accel_use(&i.capabilities, accel)))
                    .collect()
            })
            .unwrap_or_default(),
        tonemapping: info
            .as_ref()
            .map(|i| i.capabilities.tonemappers.iter().map(|t| t.as_str().to_owned()).collect())
            .unwrap_or_default(),
        dolby_vision: info.as_ref().is_some_and(|i| i.capabilities.dolby_vision),
        chromaprint: info.as_ref().is_some_and(|i| i.capabilities.chromaprint),
        throttling: info.as_ref().is_some_and(|i| i.capabilities.readrate),
        memory: info.as_ref().map(|i| {
            i.memory.clone().unwrap_or_else(|| pley_transcoder_api::MemoryInfo {
                reason: crate::settings::memory_problem(Some(i)),
                ..Default::default()
            })
        }),
        max_jobs: info.as_ref().map(|i| i.limits.max_jobs).unwrap_or(0),
        active_jobs,
        error: state.transcoder.last_error(),
    };

    let count = |sql: &'static str| {
        let db = state.db.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&db.read).await }
    };
    let counts = serde_json::json!({
        "libraries": count("SELECT COUNT(*) FROM libraries").await?,
        "movies": count("SELECT COUNT(*) FROM items WHERE kind = 'movie'").await?,
        "shows": count("SELECT COUNT(*) FROM items WHERE kind = 'show'").await?,
        "episodes": count("SELECT COUNT(*) FROM items WHERE kind = 'episode'").await?,
        "artists": count("SELECT COUNT(*) FROM items WHERE kind = 'artist'").await?,
        "albums": count("SELECT COUNT(*) FROM items WHERE kind = 'album'").await?,
        "tracks": count("SELECT COUNT(*) FROM items WHERE kind = 'track'").await?,
        "files": count("SELECT COUNT(*) FROM media_files").await?,
        "users": count("SELECT COUNT(*) FROM users").await?,
    });
    Ok(Json(serde_json::json!({
        "name": settings.server_name,
        "version": crate::VERSION,
        "commit": crate::COMMIT,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "uptime_secs": state.started_at.elapsed().as_secs(),
        "data_dir": state.config.data_dir.to_string_lossy(),
        "transcode_dir": transcode_dir(&state),
        "database": state.backups.health(),
        // Without it, authenticator apps can't be set up or checked.
        "secret_key_ok": state.two_step.can_set_up(),
        "transcoder": transcoder,
        "counts": counts,
    })))
}

#[derive(Serialize)]
pub struct TranscoderJob {
    id: String,
    label: Option<String>,
    owner: Option<String>,
    state: pley_transcoder_api::JobState,
    created_at_ms: i64,
    last_access_ms: i64,
    info: TranscodeInfo,
    restarts: u32,
    served_bytes: u64,
    warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct TranscoderStatusDto {
    available: bool,
    version: Option<String>,
    uptime_secs: u64,
    max_jobs: u32,
    running_processes: u32,
    hw_processes: u32,
    max_hw_jobs: u32,
    work_dir_bytes: u64,
    counters: Counters,
    jobs: Vec<TranscoderJob>,
}

#[derive(Serialize)]
pub struct BackupsDto {
    health: crate::backup::Health,
    backups: Vec<crate::backup::BackupInfo>,
}

/// The database's health and its backups, newest first.
pub async fn backups(State(state): State<AppState>, _admin: AdminUser) -> Json<BackupsDto> {
    Json(BackupsDto { health: state.backups.health(), backups: state.backups.list().await })
}

/// Back the database up now.
pub async fn create_backup(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<crate::backup::BackupInfo>> {
    Ok(Json(state.backups.create().await.map_err(AppError::Internal)?))
}

#[derive(Deserialize)]
pub struct ResourcesQuery {
    /// Only the samples taken after this (Unix milliseconds).
    since: Option<i64>,
}

/// How busy the machine's processors and memory are, and how much of them
/// pley takes: the last five minutes, or what came after `since`.
pub async fn resources(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<ResourcesQuery>,
) -> Json<crate::resources::Resources> {
    Json(state.resources.resources(query.since))
}

/// When library work runs: the hours of the week people watch, as learned,
/// and those it may begin in.
pub async fn library_work(State(state): State<AppState>, _admin: AdminUser) -> Json<crate::schedule::WatchingHours> {
    Json(state.schedule.watching_hours())
}

/// Space used by stored pictures, generated media files and the cache.
pub async fn storage(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<crate::store::Usage>> {
    Ok(Json(state.store.usage().await.map_err(AppError::Internal)?))
}

/// Remove pictures and generated files nothing uses any more.
pub async fn clean_storage(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<crate::store::CleanReport>> {
    Ok(Json(state.store.clean().await.map_err(AppError::Internal)?))
}

pub async fn transcoder_status(State(state): State<AppState>, _admin: AdminUser) -> Json<TranscoderStatusDto> {
    match state.transcoder.status().await {
        Ok(status) => Json(TranscoderStatusDto {
            available: true,
            version: Some(status.version.clone()),
            uptime_secs: status.uptime_secs,
            max_jobs: status.max_jobs,
            running_processes: status.running_processes,
            hw_processes: status.hw_processes,
            max_hw_jobs: status.max_hw_jobs,
            work_dir_bytes: status.work_dir_bytes,
            counters: status.counters.clone(),
            jobs: status
                .jobs
                .iter()
                .map(|j| TranscoderJob {
                    id: j.id.clone(),
                    label: j.label.clone(),
                    owner: j.owner.clone(),
                    state: j.state,
                    created_at_ms: j.created_at_ms,
                    last_access_ms: j.last_access_ms,
                    info: transcode_info(j),
                    restarts: j.restarts,
                    served_bytes: j.served_bytes,
                    warnings: j.warnings.clone(),
                })
                .collect(),
        }),
        Err(_) => Json(TranscoderStatusDto {
            available: false,
            version: None,
            uptime_secs: 0,
            max_jobs: 0,
            running_processes: 0,
            hw_processes: 0,
            max_hw_jobs: 0,
            work_dir_bytes: 0,
            counters: Counters::default(),
            jobs: Vec::new(),
        }),
    }
}

// ------------------------------------------------------------------ folder browser

#[derive(Serialize)]
pub struct DirEntry {
    name: String,
    path: String,
}

#[derive(Serialize)]
pub struct Listing {
    path: Option<String>,
    parent: Option<String>,
    entries: Vec<DirEntry>,
}

fn roots() -> Vec<DirEntry> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if Path::new(&root).exists() {
                out.push(DirEntry { name: format!("{}:", letter as char), path: root });
            }
        }
    } else {
        out.push(DirEntry { name: "/".into(), path: "/".into() });
        for candidate in ["/mnt", "/media", "/srv", "/Volumes"] {
            if Path::new(candidate).is_dir() {
                out.push(DirEntry { name: candidate.into(), path: candidate.into() });
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        out.push(DirEntry { name: "Home".into(), path: home.to_string_lossy().into_owned() });
    }
    out
}

#[derive(Deserialize)]
pub struct FsQuery {
    #[serde(default)]
    path: Option<String>,
}

/// `POST /api/fs`: the folders in `path`, or the roots and drives without
/// it, to pick a library's or the transcoder's from. A POST, though it
/// changes nothing: over plain HTTP browsers don't say which page asked for
/// a GET, so another site's link or picture could have the server open a
/// folder of its choosing (on Windows, a share of its own, signing in to
/// it). A POST's `Origin` says.
pub async fn browse_fs(_admin: AdminUser, Json(query): Json<FsQuery>) -> ApiResult<Json<Listing>> {
    let Some(raw) = query.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(Json(Listing { path: None, parent: None, entries: roots() }));
    };
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(AppError::bad_request("path must be absolute"));
    }
    let listing = crate::util::blocking(move || {
        let mut entries = Vec::new();
        let read = std::fs::read_dir(&path).map_err(|e| anyhow::anyhow!("cannot open {}: {e}", path.display()))?;
        for entry in read.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let is_dir = kind.is_dir() || (kind.is_symlink() && entry.path().is_dir());
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_dir || name.starts_with('.') || name.starts_with('$') || name == "System Volume Information" {
                continue;
            }
            entries.push(DirEntry { path: entry.path().to_string_lossy().into_owned(), name });
        }
        entries.sort_by_key(|e| e.name.to_lowercase());
        Ok(Listing {
            parent: path.parent().map(|p| p.to_string_lossy().into_owned()),
            path: Some(path.to_string_lossy().into_owned()),
            entries,
        })
    })
    .await
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(listing))
}
