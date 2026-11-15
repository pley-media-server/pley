//! Casts to the devices on the server's network. A cast is a playback
//! session like any other, started by the server for the device: its
//! stream is keyed, as the device fetches it itself, and the server reports
//! its place, as the device's player reports nothing to Pley. The remotes
//! control it through the session, as they control Pley's own players.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::player::{Ended, Event, Failure, Media, Player, State as Playing, Subtitle};
use super::{Found, Protocol};
use crate::auth::{AuthUser, CurrentToken};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::playback::{Control, Method, PlaybackRequest};
use crate::state::AppState;

/// How often the device is asked where it is, and its place kept.
const ASK_EVERY: Duration = Duration::from_secs(5);
const RECORD_EVERY: Duration = Duration::from_secs(10);
/// How long a cast waits for its subtitles to be read whole before it
/// starts (a large file on a network share takes a while).
const PREPARE_WITHIN: Duration = Duration::from_secs(20);

/// The error code of a cast to a device that wants pairing first.
pub const PAIRING_NEEDED: &str = "pairing_needed";

/// What a cast is asked to play.
#[derive(Debug, Clone)]
pub struct Cast {
    pub item_id: i64,
    pub start_ms: Option<i64>,
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
}

/// The streams a device is tried with, as its protocol picks them.
pub struct Choice {
    /// What it is said to play (a [`PlaybackRequest`]'s `caps`), first.
    pub caps: Value,
    /// The narrower caps it gets when it refuses those: none when they are
    /// the basic ones already.
    pub basic: Option<Value>,
    /// Its model, remembered when it refuses its stream and plays the
    /// basic one, so that it gets the basic one at once next time.
    pub model: Option<String>,
}

/// Plays `cast` on `device` as `user`: a session started as Pley's own
/// players start one, the device's player started and the stream loaded
/// into it. Returns once it plays, or didn't start; then the server
/// follows it until it ends. The stream is what the device's protocol
/// picks for it ([`Choice`]); one it refuses is started again with the
/// basic caps, and its model remembered. The cast playing on the device
/// goes on until this one plays, so that one that fails leaves it be. A
/// cast that a newer one to the same device replaces while it starts gives
/// way: it ends its session, leaves the device to the newer one, and
/// returns as done, since the device plays what was last asked of it.
pub async fn start(state: &AppState, user: &AuthUser, device: Found, mut cast: Cast) -> ApiResult<()> {
    // A cast that names no place starts where Play would, so it doesn't
    // start over and save the beginning over the place someone left off.
    if cast.start_ms.is_none() {
        let row = crate::api::items::item_or_404(state, user.id, cast.item_id).await?;
        cast.start_ms = Some(crate::api::items::resume_ms(&row));
    }
    let base = base_url(state, device.address).ok_or_else(|| {
        AppError::ConflictAs("cast_unreachable", "Pley doesn't listen on an address the device can reach".to_owned())
    })?;
    let choice = match device.protocol {
        Protocol::Cast => crate::cast::device::choose(state, &device).await,
        Protocol::AirPlay => {
            // A device that wants a PIN is paired first, from Play on….
            let ready = crate::airplay::device::ready(state, &device)
                .await
                .map_err(|_| AppError::ConflictAs("cast_failed", "the device didn't answer".to_owned()))?;
            if !ready {
                return Err(AppError::ConflictAs(
                    PAIRING_NEEDED,
                    "the device wants a PIN: pair with it first".to_owned(),
                ));
            }
            crate::airplay::device::choose(&device)
        }
    };
    let refused = choice.model.as_deref().is_some_and(|m| state.nearby.refused(m));
    let (first, fallback) = match (refused, choice.basic) {
        (true, Some(basic)) => (basic, None),
        (_, basic) => (choice.caps, basic),
    };
    let key = device.key();
    // The remotes may send commands while the cast loads: they're followed
    // once it plays.
    let commands = state.events.subscribe();
    let ours = CancellationToken::new();
    let cast_id = state.nearby.casts.fetch_add(1, Ordering::Relaxed);
    if let Some((_, before)) = state.nearby.pending.lock().insert(key.clone(), (cast_id, ours.clone())) {
        before.cancel();
    }
    let attempt = Attempt { state, user, device: &device, cast: &cast, base: &base, ours: &ours };
    let played = match (attempt.play(&first).await, fallback) {
        (Err(Tried::Refused), Some(basic)) => {
            tracing::info!("a device refused its stream ({first}): it is sent the basic one");
            let played = attempt.play(&basic).await;
            // Its model is remembered only when the basic stream plays:
            // then the stream, not the device, was what failed.
            if played.is_ok()
                && let Some(model) = choice.model
            {
                state.nearby.remember_refused(model);
            }
            played
        }
        (played, _) => played,
    };
    // A Cast device that answered but played neither stream is left with
    // Google's player open and idle: it is closed, unless a cast plays in
    // it or a newer one is on its way. Only now, as closing it between the
    // two tries would have the second open it again.
    if matches!(played, Err(Tried::Refused | Tried::Failed))
        && device.protocol == Protocol::Cast
        && !ours.is_cancelled()
        && !state.nearby.playing.lock().contains_key(&key)
    {
        crate::cast::player::close(device.address).await;
    }
    forget(&state.nearby.pending, &key, cast_id);
    let (player, events, session_id, duration_ms) = match played {
        Ok(started) => started,
        Err(Tried::Superseded) => {
            tracing::info!("a cast to a device on the network gave way to a newer one");
            return Ok(());
        }
        Err(tried) => {
            return Err(match tried {
                Tried::Superseded => unreachable!(),
                Tried::Refused | Tried::Failed | Tried::Lost => {
                    AppError::ConflictAs("cast_failed", "the device didn't play it".to_owned())
                }
                Tried::Error(err) => err,
            });
        }
    };
    // The cast that played there leaves the device to this one, and ends
    // its session.
    if let Some((_, before)) = state.nearby.playing.lock().insert(key.clone(), (cast_id, ours.clone())) {
        before.cancel();
    }
    tracing::info!("item {} is cast to a device on the network ({:?})", cast.item_id, device.protocol);
    let start_ms = cast.start_ms.unwrap_or(0);
    let cast = Following { state: state.clone(), user_id: user.id, device, session_id, duration_ms, start_ms, cast_id };
    tokio::spawn(cast.follow(player, events, commands, ours));
    Ok(())
}

/// Why a cast's attempt didn't play.
enum Tried {
    /// The device refused the stream: another profile may do.
    Refused,
    /// It didn't start its player, or the stream, in time.
    Failed,
    /// It couldn't be reached, or went away.
    Lost,
    /// The session didn't start.
    Error(AppError),
    /// A newer cast to the device took its place.
    Superseded,
}

/// One try at a cast, with one stream's caps.
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
    /// Starts the session and loads it into the device's player: the
    /// player, its events, the session and its length. A session that
    /// doesn't play is ended.
    async fn play(&self, caps: &Value) -> Result<(Player, tokio::sync::mpsc::Receiver<Event>, String, i64), Tried> {
        let (state, user, device, cast) = (self.state, self.user, self.device, self.cast);
        let request: PlaybackRequest = serde_json::from_value(serde_json::json!({
            "item_id": cast.item_id,
            "start_ms": cast.start_ms,
            "audio_index": cast.audio_index,
            "subtitle_index": cast.subtitle_index,
            "caps": caps,
            // One cast at a time on a device: a new one ends the last, once
            // it plays.
            "client_id": device.key(),
            "keep_previous": true,
            "keyed_urls": true,
            "controls": device.protocol.controls(),
            "device_name": device.name,
            "device_kind": device.kind(),
        }))
        .map_err(|err| Tried::Error(AppError::bad_request(err.to_string())))?;
        let mut request = PlaybackRequest { device_id: Some(device.key()), ..request };
        // The session starts, ending the device's last one, only while no
        // newer cast has: otherwise this one would end the newer one's.
        let one = state.nearby.starting.lock().entry(device.key()).or_default().clone();
        let started = {
            let _one = one.lock().await;
            if self.ours.is_cancelled() {
                return Err(Tried::Superseded);
            }
            let start = |request| {
                crate::api::playback::start(
                    State(state.clone()),
                    user.clone(),
                    HeaderMap::new(),
                    CurrentToken(None),
                    Json(request),
                )
            };
            match start(request.clone()).await {
                // The server's limit on streams counts the cast playing
                // there: it ends first, then, leaving the device to this one.
                Err(AppError::Unavailable(_)) => {
                    if let Some((_, before)) = state.nearby.playing.lock().get(&device.key()) {
                        before.cancel();
                    }
                    request.keep_previous = false;
                    start(request).await
                }
                started => started,
            }
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

    /// Loads the session `info` started into the device's player.
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
        let sideloads = device.protocol.shows_subtitles(info.method != Method::Direct);
        let subtitle = shown.filter(|_| !info.burn_subtitles && sideloads).and_then(|index| {
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
        // An AirPlay device finds its subtitles in the stream's playlist:
        // one that names them, beside the stream's own.
        let url = match (&subtitle, shown, device.protocol) {
            (Some(_), Some(index), Protocol::AirPlay) => format!("{base}/api/media/{key}/subtitled-{index}.m3u8"),
            _ => format!("{base}{}", info.url),
        };
        let media = Media {
            url,
            content_type: content_type.to_owned(),
            start_ms: cast.start_ms.unwrap_or(0),
            title,
            subtitle,
            fmp4,
        };

        // A Cast device asks for the subtitles once, as the stream loads,
        // and never again: the track is read whole first (the first read out
        // of a large file takes a while), as the device is connected to. An
        // AirPlay device asks for them a segment at a time, each waiting
        // until the track is read past it: the reading only starts here.
        let read = async {
            let within = match device.protocol {
                Protocol::Cast => PREPARE_WITHIN,
                Protocol::AirPlay => Duration::ZERO,
            };
            if media.subtitle.is_some()
                && let Some(index) = shown
                && !crate::api::playback::prepare_text_track(state, user, session.file_id, index, within).await
                && device.protocol == Protocol::Cast
            {
                tracing::warn!("a cast's subtitles weren't read whole in time: they start where they are");
            }
        };
        let connect = async {
            match device.protocol {
                Protocol::Cast => crate::cast::device::connect(device).await,
                Protocol::AirPlay => crate::airplay::device::connect(state, device).await,
            }
        };
        let ((), connected) = tokio::join!(read, connect);
        drop(session);
        let started = match connected {
            Ok(connected) => connected.start(&media).await,
            Err(failure) => Err(failure),
        };
        match started {
            Ok((player, events)) => Ok((player, events, info.session_id.clone(), info.duration_ms)),
            Err(failure) => {
                tracing::warn!("a cast to {} didn't start: {failure:?}", device.address);
                Err(match failure {
                    Failure::Refused(_) => Tried::Refused,
                    Failure::NotStarted(_) => Tried::Failed,
                    Failure::Lost => Tried::Lost,
                    Failure::NeedsPairing => Tried::Error(AppError::ConflictAs(
                        PAIRING_NEEDED,
                        "the device wants a PIN: pair with it first".to_owned(),
                    )),
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

/// Forgets the cast on `device` in `casts` unless another took its place.
fn forget(casts: &Mutex<HashMap<String, (u64, CancellationToken)>>, device: &str, cast_id: u64) {
    let mut casts = casts.lock();
    if casts.get(device).is_some_and(|(id, _)| *id == cast_id) {
        casts.remove(device);
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
    async fn follow(
        self,
        player: Player,
        mut events: tokio::sync::mpsc::Receiver<Event>,
        mut commands: tokio::sync::broadcast::Receiver<ServerEvent>,
        ours: CancellationToken,
    ) {
        let Following { state, user_id, device, session_id, duration_ms, start_ms, cast_id } = self;
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
        // Whether the device said where it is: a cast that never did keeps
        // no place.
        let mut placed = false;
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
                        // A place past the end is the device's mistake.
                        let position_ms = position_ms.clamp(0, if duration_ms > 0 { duration_ms } else { i64::MAX });
                        placed = true;
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
                                let at = position_ms.or_else(|| offset_ms.map(|offset| last.0.saturating_add(offset)));
                                if let Some(at) = at {
                                    let at = if duration_ms > 0 { at.clamp(0, duration_ms) } else { at.max(0) };
                                    player.seek(at).await;
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
                    // Stopped in Pley: the device stops at once.
                    Ok(ServerEvent::SessionStopped { session_id: sid, .. }) if sid == session_id => {
                        if !ours.is_cancelled() {
                            player.stop().await;
                        }
                        break None;
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
        // Its place is kept however it ended: on the device, stopped in Pley,
        // or given way to a newer cast.
        if ended.is_some() || placed {
            record(&state, user_id, item_id, position, duration_ms, &device.name, "stopped").await;
        }
        end_session(&state, &session_id);
        forget(&state.nearby.playing, &device.key(), cast_id);
        tracing::info!("a cast to a device on the network ended: {ended:?}");
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
