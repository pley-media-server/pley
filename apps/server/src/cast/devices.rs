//! The Cast devices the household's remotes cast to, and the casts playing
//! on them. A cast is a playback session like any other, started by the
//! server for the device: its stream is keyed, as the device fetches it
//! itself, and the server reports its place, as the device's player reports
//! nothing to Pley. The remotes control it through the session, as they
//! control Pley's own players.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use super::connection::Connection;
use super::discovery::{self, Found};
use super::player::{Ended, Event, Failure, Media, Player, State as Playing, Subtitle};
use super::profile::{self, Hardware, Profile};
use crate::auth::{AuthUser, CurrentToken};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::playback::{Control, Method, PlaybackRequest};
use crate::state::AppState;

/// How long a look for devices waits for answers, how long one is good
/// for, and how long a device that stops answering stays listed.
const LOOK_FOR: Duration = Duration::from_millis(1500);
const LOOK_AGAIN: Duration = Duration::from_secs(30);
const KEEP_FOR: Duration = Duration::from_secs(5 * 60);
/// How often the device is asked where it is, and its place kept.
const ASK_EVERY: Duration = Duration::from_secs(5);
const RECORD_EVERY: Duration = Duration::from_secs(10);
/// How long a cast waits for its subtitles to be read whole before it
/// starts (a large file on a network share takes a while).
const PREPARE_WITHIN: Duration = Duration::from_secs(20);

/// Devices whose setup API's answer is kept, and models remembered as
/// refusing their profile, at most.
const MAX_KNOWN: usize = 256;

/// The prefix of a Cast device's id among the remotes' devices.
pub const ID_PREFIX: &str = "cast:";

/// What a cast is asked to play.
#[derive(Debug, Clone)]
pub struct Cast {
    pub item_id: i64,
    pub start_ms: Option<i64>,
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
}

#[derive(Default)]
pub struct CastDevices {
    found: Mutex<HashMap<String, (Found, Instant)>>,
    looked: Mutex<Option<Instant>>,
    looking: tokio::sync::Mutex<()>,
    /// The cast playing on each device, by its id: a new one ends it.
    playing: Mutex<HashMap<String, (u64, CancellationToken)>>,
    /// Held, by device, while a cast starts its session, which ends the
    /// device's last one: of two casts at once, the newer one's session is
    /// the one that stays.
    starting: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    casts: AtomicU64,
    /// What each device's setup API said, by its id, asked once (`None`:
    /// it didn't answer).
    hardware: Mutex<HashMap<String, Option<Hardware>>>,
    /// The models ([`profile::identity`]) that refused their profile: they
    /// get the basic one, until the server restarts.
    refused: Mutex<HashSet<String>>,
}

impl CastDevices {
    /// The devices that show video, looking again when the last look is
    /// old. Pley looks on the local network only while `discovery` is on
    /// (Settings → Network); turned off, the devices found are forgotten.
    /// `fixed` is the device the server was told of, which is always listed.
    pub async fn listed(&self, fixed: Option<Found>, discovery: bool) -> Vec<Found> {
        if !discovery {
            let fixed = fixed.as_ref().map(|f| f.id.as_str());
            self.found.lock().retain(|id, _| Some(id.as_str()) == fixed);
            *self.looked.lock() = None;
        }
        if (discovery || fixed.is_some()) && self.looked.lock().is_none_or(|at| at.elapsed() >= LOOK_AGAIN) {
            let _one = self.looking.lock().await;
            // Another request looked while this one waited.
            if self.looked.lock().is_none_or(|at| at.elapsed() >= LOOK_AGAIN) {
                self.look(fixed).await;
            }
        }
        let mut found = self.found.lock();
        found.retain(|_, (_, at)| at.elapsed() < KEEP_FOR);
        found.values().map(|(device, _)| device.clone()).filter(|d| d.video).collect()
    }

    async fn look(&self, fixed: Option<Found>) {
        let from: Vec<Ipv4Addr> =
            crate::network::adapters().into_iter().filter(|a| a.up).flat_map(|a| a.addresses).collect();
        let browse = async {
            if from.is_empty() || fixed.is_some() {
                return Ok(Vec::new());
            }
            discovery::browse(&from, LOOK_FOR).await
        };
        let mut seen = browse.await.unwrap_or_else(|err| {
            tracing::warn!("couldn't look for Cast devices: {err}");
            Vec::new()
        });
        seen.extend(fixed);
        let now = Instant::now();
        let mut found = self.found.lock();
        for device in seen {
            found.insert(device.id.clone(), (device, now));
        }
        *self.looked.lock() = Some(now);
    }

    /// The device of `id` (without [`ID_PREFIX`]) seen lately, while
    /// `discovery` is on, as [`Self::listed`] lists it.
    pub fn device(&self, id: &str, fixed: Option<Found>, discovery: bool) -> Option<Found> {
        if !discovery && fixed.is_none_or(|f| f.id != id) {
            return None;
        }
        let found = self.found.lock();
        found.get(id).filter(|(d, at)| d.video && at.elapsed() < KEEP_FOR).map(|(d, _)| d.clone())
    }
}

/// Plays `cast` on `device` as `user`: a session started as Pley's own
/// players start one, Google's player started on the device and the
/// stream loaded into it. Returns once it plays, or didn't start; then the
/// server follows it until it ends. The stream is what the device's
/// [profile] takes; one it refuses is started again with the basic
/// profile, and its model remembered. A cast that a newer one to the same
/// device replaces while it starts gives way: it ends its session, leaves
/// the device to the newer one, and returns as done, since the device
/// plays what was last asked of it.
pub async fn start(state: &AppState, user: &AuthUser, device: Found, cast: Cast) -> ApiResult<()> {
    let base = base_url(state, device.address).ok_or_else(|| {
        AppError::ConflictAs("cast_unreachable", "Pley doesn't listen on an address the device can reach".to_owned())
    })?;
    let hardware = hardware(state, &device).await;
    let model = profile::identity(device.model.as_deref(), hardware.as_ref());
    let refused = model.as_ref().is_some_and(|m| state.cast_devices.refused.lock().contains(m));
    let picked = if refused { Profile::BASIC } else { profile::pick(device.model.as_deref(), hardware.as_ref()) };
    // A cast playing there already leaves the device to this one, before
    // its session ends (this one's start ends it), so that it doesn't stop
    // the device on its way out.
    let ours = CancellationToken::new();
    let cast_id = state.cast_devices.casts.fetch_add(1, Ordering::Relaxed);
    if let Some((_, before)) = state.cast_devices.playing.lock().insert(device.id.clone(), (cast_id, ours.clone())) {
        before.cancel();
    }
    let attempt = Attempt { state, user, device: &device, cast: &cast, base: &base, ours: &ours };
    let played = match attempt.play(picked).await {
        Err(Tried::Refused) if picked != Profile::BASIC => {
            tracing::info!("a Cast device refused its profile ({picked:?}): it is sent the basic one");
            let basic = attempt.play(Profile::BASIC).await;
            // Its model is remembered only when the basic profile plays:
            // then the stream, not the device, was what failed.
            if basic.is_ok()
                && let Some(model) = model
            {
                let mut refused = state.cast_devices.refused.lock();
                if refused.len() < MAX_KNOWN {
                    refused.insert(model);
                }
            }
            basic
        }
        played => played,
    };
    let (player, events, session_id, duration_ms) = match played {
        Ok(started) => started,
        Err(Tried::Superseded) => {
            tracing::info!("a cast to a Cast device gave way to a newer one");
            return Ok(());
        }
        Err(tried) => {
            forget(state, &device.id, cast_id);
            return Err(match tried {
                Tried::Superseded => unreachable!(),
                Tried::Refused | Tried::Failed => {
                    AppError::ConflictAs("cast_failed", "the device didn't play it".to_owned())
                }
                Tried::Error(err) => err,
            });
        }
    };
    tracing::info!("item {} is cast to a Cast device", cast.item_id);
    let start_ms = cast.start_ms.unwrap_or(0);
    let cast = Following { state: state.clone(), user_id: user.id, device, session_id, duration_ms, start_ms, cast_id };
    tokio::spawn(cast.follow(player, events, ours));
    Ok(())
}

/// What `device`'s setup API said, asked the first time it is cast to.
async fn hardware(state: &AppState, device: &Found) -> Option<Hardware> {
    if let Some(known) = state.cast_devices.hardware.lock().get(&device.id) {
        return known.clone();
    }
    let fixed = &state.config.cast_fixed;
    let setup = match fixed.device {
        Some(address) if address == device.address => fixed.setup,
        _ => Some(SocketAddr::new(device.address.ip(), profile::SETUP_PORT)),
    };
    let answer = match setup {
        Some(setup) => profile::ask(setup).await,
        None => None,
    };
    let mut known = state.cast_devices.hardware.lock();
    if known.len() < MAX_KNOWN {
        known.insert(device.id.clone(), answer.clone());
    }
    answer
}

/// Why a cast's attempt didn't play.
enum Tried {
    /// The device refused the stream: another profile may do.
    Refused,
    /// It didn't answer, or went away.
    Failed,
    /// The session didn't start.
    Error(AppError),
    /// A newer cast to the device took its place.
    Superseded,
}

/// One try at a cast, with one profile.
struct Attempt<'a> {
    state: &'a AppState,
    user: &'a AuthUser,
    device: &'a Found,
    cast: &'a Cast,
    base: &'a str,
    /// Cancelled when a newer cast to the device takes this one's place.
    ours: &'a CancellationToken,
}

impl Attempt<'_> {
    /// Starts the session and loads it into Google's player: the player,
    /// its events, the session and its length. A session that doesn't play
    /// is ended.
    async fn play(&self, profile: Profile) -> Result<(Player, tokio::sync::mpsc::Receiver<Event>, String, i64), Tried> {
        let (state, user, device, cast) = (self.state, self.user, self.device, self.cast);
        let request: PlaybackRequest = serde_json::from_value(serde_json::json!({
            "item_id": cast.item_id,
            "start_ms": cast.start_ms,
            "audio_index": cast.audio_index,
            "subtitle_index": cast.subtitle_index,
            "caps": profile.caps(),
            // One cast at a time on a device: a new one ends the last.
            "client_id": format!("{ID_PREFIX}{}", device.id),
            "keyed_urls": true,
            "controls": ["play", "pause", "seek", "skip", "volume"],
            "device_name": device.name,
            "device_kind": "tv",
        }))
        .map_err(|err| Tried::Error(AppError::bad_request(err.to_string())))?;
        let request = PlaybackRequest { device_id: Some(format!("{ID_PREFIX}{}", device.id)), ..request };
        // The session starts, ending the device's last one, only while no
        // newer cast has: otherwise this one would end the newer one's.
        let one = state.cast_devices.starting.lock().entry(device.id.clone()).or_default().clone();
        let started = {
            let _one = one.lock().await;
            if self.ours.is_cancelled() {
                return Err(Tried::Superseded);
            }
            crate::api::playback::start(
                State(state.clone()),
                user.clone(),
                HeaderMap::new(),
                CurrentToken(None),
                Json(request),
            )
            .await
        };
        let Json(info) = started.map_err(Tried::Error)?;
        let end = |tried: Tried| {
            end_session(state, &info.session_id);
            tried
        };
        // From here on, a newer cast ends this one where it is: the
        // connection, dropped, closes, and the device is left to the newer
        // cast.
        tokio::select! {
            biased;
            () = self.ours.cancelled() => Err(end(Tried::Superseded)),
            played = self.load(&info) => played.map_err(end),
        }
    }

    /// Loads the session `info` started into Google's player.
    async fn load(
        &self,
        info: &crate::playback::PlaybackInfo,
    ) -> Result<(Player, tokio::sync::mpsc::Receiver<Event>, String, i64), Tried> {
        let (state, user, device, cast, base) = (self.state, self.user, self.device, self.cast, self.base);
        let Some(session) = state.sessions.get(&info.session_id) else {
            return Err(Tried::Error(AppError::not_found("the session ended")));
        };
        let Some(key) = session.media_key.clone() else {
            return Err(Tried::Error(AppError::Conflict("the session has no keyed address".to_owned())));
        };
        // The subtitles it shows: picked by the remote, else by the user's
        // choices. Text ones go beside the stream, as WebVTT; picture ones
        // the stream has burned in.
        let shown = cast.subtitle_index.or(session.play.lock().subtitle_index).filter(|i| *i >= 0);
        let subtitle = shown.filter(|_| !info.burn_subtitles).and_then(|index| {
            let track = info.subtitles.iter().find(|t| t.index == index && t.text_based)?;
            let name = track.title.clone().or_else(|| track.language.clone()).unwrap_or_else(|| format!("{index}"));
            let url = format!("{base}/api/media/{key}/subtitles-{index}.vtt");
            Some(Subtitle { url, language: track.language.clone(), name })
        });
        // What it shows, for the remotes: none when the track isn't one it
        // can.
        if session.report_tracks(None, Some(subtitle.as_ref().and(shown).unwrap_or(-1))) {
            state.events.send(ServerEvent::SessionsChanged { user_id: user.id });
        }
        let content_type = match info.method {
            Method::Direct => "video/mp4",
            _ => "application/x-mpegurl",
        };
        let fmp4 = session.transcode.as_ref().is_some_and(|spec| !spec.request.segment_format.is_ts());
        let title =
            crate::api::items::item_or_404(state, user.id, cast.item_id).await.map(|row| row.title).unwrap_or_default();
        let media = Media {
            url: format!("{base}{}", info.url),
            content_type: content_type.to_owned(),
            start_ms: cast.start_ms.unwrap_or(0),
            title,
            subtitle,
            fmp4,
        };

        // The device asks for the subtitles once, as the stream loads, and
        // never again: the track is read whole first (the first read out of
        // a large file takes a while), as the device is connected to.
        let read = async {
            if media.subtitle.is_some()
                && let Some(index) = shown
                && !crate::api::playback::prepare_text_track(state, user, session.file_id, index, PREPARE_WITHIN).await
            {
                tracing::warn!("a cast's subtitles weren't read whole in time: they start where they are");
            }
        };
        let connect = async {
            Connection::connect(device.address).await.map_err(|err| {
                tracing::warn!("couldn't connect to the Cast device {}: {err}", device.address);
                Failure::Lost
            })
        };
        let ((), connected) = tokio::join!(read, connect);
        drop(session);
        let started = match connected {
            Ok((connection, incoming)) => Player::start(connection, incoming, &media).await,
            Err(failure) => Err(failure),
        };
        match started {
            Ok((player, events)) => Ok((player, events, info.session_id.clone(), info.duration_ms)),
            Err(failure) => {
                tracing::warn!("a cast to {} didn't start: {failure:?}", device.address);
                Err(match failure {
                    Failure::Load(_) => Tried::Refused,
                    Failure::Launch(_) | Failure::NotStarted(_) | Failure::Lost => Tried::Failed,
                })
            }
        }
    }
}

/// Where the device reaches Pley's web port: the address of this computer
/// on the way to it.
fn base_url(state: &AppState, device: SocketAddr) -> Option<String> {
    if let Some(server) = &state.config.cast_fixed.server {
        return Some(server.clone());
    }
    let local = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    local.connect(device).ok()?;
    let ip = local.local_addr().ok()?.ip();
    let listeners = state.network.listeners();
    let web = listeners.iter().find(|l| {
        l.kind == crate::network::Kind::Web
            && l.error.is_none()
            && (l.address.ip().is_unspecified() || l.address.ip() == ip)
    })?;
    Some(format!("http://{}", SocketAddr::new(ip, web.address.port())))
}

fn end_session(state: &AppState, id: &str) {
    if let Some(session) = state.sessions.remove(id) {
        if let Some(job) = session.job_id() {
            let transcoder = state.transcoder.clone();
            tokio::spawn(async move {
                let _ = transcoder.delete_job(&job).await;
            });
        }
        state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
    }
}

/// Forgets the cast on `device` unless another took its place.
fn forget(state: &AppState, device: &str, cast_id: u64) {
    let mut playing = state.cast_devices.playing.lock();
    if playing.get(device).is_some_and(|(id, _)| *id == cast_id) {
        playing.remove(device);
    }
}

/// Follows a cast until it ends: its place kept, the remotes' commands
/// passed on, and the device stopped when the session is stopped in Pley.
struct Following {
    state: AppState,
    user_id: i64,
    device: Found,
    session_id: String,
    duration_ms: i64,
    /// Where it started: its place until the device says another.
    start_ms: i64,
    cast_id: u64,
}

impl Following {
    async fn follow(self, player: Player, mut events: tokio::sync::mpsc::Receiver<Event>, ours: CancellationToken) {
        let Following { state, user_id, device, session_id, duration_ms, start_ms, cast_id } = self;
        let mut commands = state.events.subscribe();
        let mut ask = tokio::time::interval(ASK_EVERY);
        let item_id = state.sessions.get(&session_id).map(|s| s.item_id);
        let Some(item_id) = item_id else {
            // Ended already: by a newer cast, which has the device, or in
            // Pley.
            if !ours.is_cancelled() {
                player.stop().await;
            }
            return;
        };
        // Its volume, for the remotes' slider: the device says it again
        // whenever it changes.
        player.ask_volume().await;
        // A cast that ends before the device says where it is keeps the
        // place it started at, not the title's start.
        let mut last = (start_ms, "buffering");
        let mut recorded = Instant::now();
        let ended = loop {
            tokio::select! {
                biased;
                () = ours.cancelled() => {
                    // Another cast took the device: it plays, this session ends.
                    player.leave();
                    break None;
                }
                event = events.recv() => match event {
                    Some(Event::Status { state: playing, position_ms }) => {
                        let word = match playing {
                            Playing::Playing => "playing",
                            Playing::Paused => "paused",
                            Playing::Buffering => "buffering",
                        };
                        let Some(session) = state.sessions.get(&session_id) else {
                            if !ours.is_cancelled() {
                                player.stop().await;
                            }
                            break None;
                        };
                        if session.update(position_ms, word) {
                            state.events.send(ServerEvent::SessionsChanged { user_id });
                        }
                        if session.should_touch_transcoder() && let Some(job) = session.job_id() {
                            let transcoder = state.transcoder.clone();
                            tokio::spawn(async move { let _ = transcoder.touch_job(&job).await; });
                        }
                        if word != last.1 || recorded.elapsed() >= RECORD_EVERY {
                            record(&state, user_id, item_id, position_ms, duration_ms, &device.name, word).await;
                            recorded = Instant::now();
                        }
                        last = (position_ms, word);
                    }
                    Some(Event::Volume(percent)) => {
                        if state.sessions.get(&session_id).is_some_and(|s| s.report_volume(percent)) {
                            state.events.send(ServerEvent::SessionsChanged { user_id });
                        }
                    }
                    Some(Event::Ended(ended)) => break Some(ended),
                    None => break Some(Ended::Lost),
                },
                command = commands.recv() => match command {
                    Ok(ServerEvent::PlayerCommand { session_id: sid, command, position_ms, offset_ms, volume, .. }) if sid == session_id => {
                        match command {
                            Control::Play => { player.play().await; }
                            Control::Pause => { player.pause().await; }
                            Control::Seek | Control::Skip => {
                                let at = position_ms.or_else(|| offset_ms.map(|offset| last.0 + offset));
                                if let Some(at) = at {
                                    player.seek(at.clamp(0, duration_ms.max(0))).await;
                                }
                            }
                            Control::Volume => {
                                if let Some(volume) = volume {
                                    player.set_volume(volume).await;
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        player.stop().await;
                        break None;
                    }
                    _ => {}
                },
                _ = ask.tick() => {
                    if state.sessions.get(&session_id).is_none() {
                        // Stopped in Pley: from the dashboard, a remote, a new cast.
                        if !ours.is_cancelled() {
                            player.stop().await;
                        }
                        break None;
                    }
                    player.status().await;
                }
            }
        };
        let position = match ended {
            Some(Ended::Finished) => duration_ms,
            _ => last.0,
        };
        if ended.is_some() {
            record(&state, user_id, item_id, position, duration_ms, &device.name, "stopped").await;
        }
        end_session(&state, &session_id);
        forget(&state, &device.id, cast_id);
        tracing::info!("a cast to a Cast device ended: {ended:?}");
    }
}

async fn record(
    state: &AppState,
    user_id: i64,
    item_id: i64,
    position_ms: i64,
    duration_ms: i64,
    device: &str,
    word: &str,
) {
    let recorded = crate::catalog::record_progress(
        &state.db,
        &state.events,
        user_id,
        item_id,
        position_ms,
        duration_ms,
        Some(device),
        word,
    )
    .await;
    match recorded {
        Ok(recorded) => {
            if recorded.left_watchlist {
                state.events.send(ServerEvent::WatchlistChanged { user_id });
            }
            if recorded.counted
                && let Ok(row) = crate::api::items::item_or_404(state, user_id, item_id).await
            {
                state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
            }
        }
        Err(err) => tracing::warn!("couldn't keep a cast's place: {err}"),
    }
}

/// A Cast device no look finds, to develop and test with: Android's Google
/// TV emulator, its port 8009 forwarded (`adb forward tcp:8009 tcp:8009`),
/// or a test's fake device. Debug builds read it from `PLEY_CAST_DEVICE`
/// (its address) and `PLEY_CAST_SERVER` (Pley's address as the device
/// reaches it: `http://10.0.2.2:7539` from the emulator); release builds
/// have none.
#[derive(Debug, Clone, Default)]
pub struct Fixed {
    pub device: Option<SocketAddr>,
    pub server: Option<String>,
    /// Where its setup API answers (`PLEY_CAST_SETUP`, else the device's
    /// own on [`profile::SETUP_PORT`]); none, not asked.
    pub setup: Option<SocketAddr>,
}

impl Fixed {
    pub fn from_env() -> Fixed {
        if !cfg!(debug_assertions) {
            return Fixed::default();
        }
        let device: Option<SocketAddr> = std::env::var("PLEY_CAST_DEVICE").ok().and_then(|a| a.parse().ok());
        let setup = std::env::var("PLEY_CAST_SETUP").ok().and_then(|a| a.parse().ok());
        Fixed {
            device,
            server: std::env::var("PLEY_CAST_SERVER").ok().filter(|s| s.starts_with("http://")),
            setup: setup.or_else(|| device.map(|d| SocketAddr::new(d.ip(), profile::SETUP_PORT))),
        }
    }

    pub(crate) fn found(&self) -> Option<Found> {
        let address = self.device?;
        Some(Found { id: "dev".into(), name: "Cast emulator".into(), model: None, address, video: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn turning_discovery_off_forgets_the_devices_found() {
        let devices = CastDevices::default();
        let tv = Found {
            id: "tv".into(),
            name: "Living room TV".into(),
            model: None,
            address: "192.168.1.20:8009".parse().unwrap(),
            video: true,
        };
        devices.found.lock().insert(tv.id.clone(), (tv.clone(), Instant::now()));
        *devices.looked.lock() = Some(Instant::now());
        assert_eq!(devices.device("tv", None, true), Some(tv.clone()));
        assert_eq!(devices.device("tv", None, false), None, "nor cast to");
        assert!(devices.listed(None, false).await.is_empty());
        assert_eq!(devices.device("tv", None, true), None, "forgotten");
    }
}
