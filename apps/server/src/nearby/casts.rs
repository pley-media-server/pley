//! Casts to the devices on the server's network. A cast is a playback
//! session like any other, started by the server for the device: its
//! stream is keyed, as the device fetches it itself, and the server reports
//! its place, as the device's player reports nothing to Pley. The remotes
//! control it through the session, as they control Pley's own players.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use pley_transcoder_api::{MediaStream, StreamKind};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::player::{Ended, Event, Failure, Media, Player, State as Playing, Subtitle};
use super::{Found, Protocol};
use crate::auth::{AuthUser, CurrentToken};
use crate::catalog::ItemRow;
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::livetv::tuner::{ViewerKind, WatchAs};
use crate::network::{Kind, Listener};
use crate::playback::{Control, Method, PlaybackMode, PlaybackRequest, Pool};
use crate::state::AppState;

/// How often the device is asked where it is, and its place kept.
const ASK_EVERY: Duration = Duration::from_secs(5);
const RECORD_EVERY: Duration = Duration::from_secs(10);
/// How long a cast waits for its subtitles to be read whole before it
/// starts (a large file on a network share takes a while).
const PREPARE_WITHIN: Duration = Duration::from_secs(20);

/// How long a cast may take to start, all its tries and waits together:
/// past it, the cast stops as one that failed does, rather than play
/// minutes after the remote was told it didn't. Clients wait longer than
/// this for the answer (docs/API.md).
pub const CAST_WITHIN: Duration = Duration::from_secs(180);

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
/// returns as done, since the device plays what was last asked of it. A
/// cast that doesn't play within [`CAST_WITHIN`] (the config's
/// `cast_within`) ends as one the device didn't play.
pub async fn start(state: &AppState, user: &AuthUser, device: Found, mut cast: Cast) -> ApiResult<()> {
    let deadline = tokio::time::Instant::now() + state.config.cast_within;
    let row = crate::api::items::item_or_404(state, user.id, cast.item_id).await?;
    // A cast that names no place starts where Play would, so it doesn't
    // start over and save the beginning over the place someone left off.
    if cast.start_ms.is_none() {
        cast.start_ms = Some(crate::api::items::resume_ms(&row));
    }
    // A device that shows no text subtitles beside a file played as it is
    // (AirPlay's) gets a stream when the cast shows some: its playlist
    // names them.
    let stream = !device.protocol.shows_subtitles(false) && shows_text_subtitles(state, user.id, &row, &cast).await?;
    let base = base_url(state, device.address).ok_or_else(unreachable)?;
    let choosing = async {
        match device.protocol {
            Protocol::Cast => Ok::<_, AppError>(crate::cast::device::choose(state, &device).await),
            Protocol::AirPlay => {
                paired(state, &device).await?;
                Ok(crate::airplay::device::choose(&device))
            }
        }
    };
    let choice = tokio::time::timeout_at(deadline, choosing).await.map_err(|_| too_late(&device))??;
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
    // The session of Google's player that a try loaded into, on a Cast
    // device: the one to close should the cast not play.
    let launched = Mutex::new(None);
    let attempt = Attempt {
        state,
        user,
        device: &device,
        cast: &cast,
        stream,
        base: &base,
        ours: &ours,
        deadline,
        launched: &launched,
    };
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
    // A Cast device that answered but played neither stream, or not in
    // time, is left with Google's player open: the session a try loaded
    // into is closed, unless a cast plays in it or a newer one is on its
    // way, then or as the stop goes. Only now, as closing it between the
    // two tries would have the second open it again. A try that loaded
    // into no player closes none: what runs there isn't this cast's.
    let free = || !ours.is_cancelled() && !state.nearby.playing.lock().contains_key(&key);
    let loaded = launched.lock().clone();
    if matches!(played, Err(Tried::Refused | Tried::Failed | Tried::TimedOut))
        && let Some(app_session) = loaded
        && free()
    {
        crate::cast::player::close(device.address, &app_session, free).await;
    }
    forget(&state.nearby.pending, &key, cast_id);
    let (player, events, session_id, duration_ms) = match played {
        Ok(started) => started,
        Err(Tried::Superseded) => {
            tracing::info!("a cast to a device on the network gave way to a newer one");
            return Ok(());
        }
        Err(tried) => return Err(failed(tried, &device)),
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

/// Plays Live TV channel `channel_id` on `device` as `user`: a viewer of
/// the channel started as Pley's own players start one, with keyed
/// addresses, and its live playlist loaded into the device's player.
/// Returns once it plays, or didn't start; then the server follows it until
/// it ends. A channel plays from its live edge, with no pause or place, as
/// it does in Pley's own players: the remotes change channel and set a Cast
/// device's volume. A cast playing on the device goes on until this one
/// plays, and a newer one takes this one's place, as with [`start`]. The
/// viewer is the device's: the channel it watched for `user` before ends
/// as this one starts, which frees its provider connection and ends its
/// playlist, so the device plays only what it loaded of it meanwhile.
pub async fn start_channel(state: &AppState, user: &AuthUser, device: Found, channel_id: i64) -> ApiResult<()> {
    let deadline = tokio::time::Instant::now() + state.config.cast_within;
    // What the caller may watch is checked first: a channel refused asks
    // for no pairing, and stops nothing loading on the device.
    state.live.check_access(user).await?;
    state.live.channel(user, channel_id).await?;
    let base = base_url(state, device.address).ok_or_else(unreachable)?;
    if device.protocol == Protocol::AirPlay {
        tokio::time::timeout_at(deadline, paired(state, &device)).await.map_err(|_| too_late(&device))??;
    }
    // The viewer starts before the cast takes the device: a source with
    // every connection in use leaves what loads there alone, and a cast
    // that gives way, or doesn't play, always knows which viewer to end.
    let (watching, channel) =
        crate::api::livetv::start_watching(state, user, channel_id, watch_as(&device)).await.map_err(full)?;
    let session_id = watching.session_id.clone();
    let key = device.key();
    let commands = state.events.subscribe();
    let ours = CancellationToken::new();
    let cast_id = state.nearby.casts.fetch_add(1, Ordering::Relaxed);
    if let Some((_, before)) = state.nearby.pending.lock().insert(key.clone(), (cast_id, ours.clone())) {
        before.cancel();
    }
    let launched = Mutex::new(None);
    let tuned = tokio::select! {
        biased;
        () = ours.cancelled() => Err(Tried::Superseded),
        () = tokio::time::sleep_until(deadline) => Err(Tried::TimedOut),
        tuned = tune(state, &device, &base, &watching.url, channel.name, &launched) => tuned,
    };
    if tuned.is_err() {
        state.live.tuners.leave(&session_id, user);
    }
    // Google's player left open by a try that didn't play is closed, as
    // in [`start`].
    let free = || !ours.is_cancelled() && !state.nearby.playing.lock().contains_key(&key);
    let loaded = launched.lock().clone();
    if matches!(tuned, Err(Tried::Refused | Tried::Failed | Tried::TimedOut))
        && let Some(app_session) = loaded
        && free()
    {
        crate::cast::player::close(device.address, &app_session, free).await;
    }
    forget(&state.nearby.pending, &key, cast_id);
    let (player, events) = match tuned {
        Ok(started) => started,
        Err(Tried::Superseded) => {
            tracing::info!("a channel's cast to a device on the network gave way to a newer cast");
            return Ok(());
        }
        Err(tried) => return Err(failed(tried, &device)),
    };
    if let Some((_, before)) = state.nearby.playing.lock().insert(key, (cast_id, ours.clone())) {
        before.cancel();
    }
    tracing::info!("channel {channel_id} is cast to a device on the network ({:?})", device.protocol);
    let watching = Watching { state: state.clone(), user: user.clone(), device, session_id, channel_id, cast_id };
    tokio::spawn(watching.follow(player, events, commands, ours));
    Ok(())
}

/// [`start_channel`], boxed: a cast's channel change starts one from its
/// follower, which that start spawns.
fn change_channel(
    state: AppState,
    user: AuthUser,
    device: Found,
    channel_id: i64,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ApiResult<()>> + Send>> {
    Box::pin(async move { start_channel(&state, &user, device, channel_id).await })
}

/// The viewer of a channel's cast: the device's own, with its name, kind
/// and controls, and keyed addresses it opens without signing in.
fn watch_as(device: &Found) -> WatchAs {
    WatchAs {
        client_id: Some(device.key()),
        kind: ViewerKind::Hls,
        keyed: true,
        // Read by the start, from the account.
        admin: false,
        take_over: None,
        home: None,
        controls: device.protocol.live_controls(),
        device: Some(device.name.clone()),
        device_id: Some(device.key()),
        device_kind: device.kind(),
    }
}

/// Loads a channel's live playlist (`url`, the viewer's) into the device's
/// player: the player and its events.
async fn tune(
    state: &AppState,
    device: &Found,
    base: &str,
    url: &str,
    title: String,
    launched: &Mutex<Option<String>>,
) -> Result<(Player, tokio::sync::mpsc::Receiver<Event>), Tried> {
    let media = Media {
        url: format!("{base}{url}"),
        content_type: "application/x-mpegurl".to_owned(),
        start_ms: 0,
        title,
        subtitle: None,
        fmp4: false,
        live: true,
    };
    let started = match connect(state, device).await {
        Ok(connected) => connected.start(&media, launched).await,
        Err(failure) => Err(failure),
    };
    started.map_err(|failure| {
        tracing::warn!("a channel's cast to {} didn't start: {failure:?}", device.address);
        tried_of(failure)
    })
}

/// Whether an AirPlay device takes a cast: one that wants a PIN is paired
/// first, from Play on….
async fn paired(state: &AppState, device: &Found) -> ApiResult<()> {
    let ready = crate::airplay::device::ready(state, device)
        .await
        .map_err(|_| AppError::ConflictAs("cast_failed", "the device didn't answer".to_owned()))?;
    if !ready {
        return Err(AppError::ConflictAs(PAIRING_NEEDED, "the device wants a PIN: pair with it first".to_owned()));
    }
    Ok(())
}

/// Connects to the device, its player not started yet.
async fn connect(state: &AppState, device: &Found) -> Result<super::player::Connected, Failure> {
    match device.protocol {
        Protocol::Cast => crate::cast::device::connect(device).await,
        Protocol::AirPlay => crate::airplay::device::connect(state, device).await,
    }
}

/// Why a try at a cast didn't play, from why the device's player didn't
/// start.
fn tried_of(failure: Failure) -> Tried {
    match failure {
        Failure::Refused(_) => Tried::Refused,
        Failure::NotStarted(_) => Tried::Failed,
        Failure::Lost => Tried::Lost,
        Failure::NeedsPairing => {
            Tried::Error(AppError::ConflictAs(PAIRING_NEEDED, "the device wants a PIN: pair with it first".to_owned()))
        }
    }
}

/// The answer to a cast that didn't play.
/// Every connection of the channel's source in use, as Play on… says it: a
/// cast takes none over, so the connections watching are no use to it, and
/// its own code tells the apps from the other conflicts a cast may meet.
fn full(err: AppError) -> AppError {
    match err {
        AppError::ConflictWith(message, _) => AppError::ConflictAs("connections_full", message),
        err => err,
    }
}

fn failed(tried: Tried, device: &Found) -> AppError {
    match tried {
        Tried::Refused | Tried::Failed | Tried::Lost | Tried::Superseded => {
            AppError::ConflictAs("cast_failed", "the device didn't play it".to_owned())
        }
        Tried::TimedOut => too_late(device),
        Tried::Error(err) => err,
    }
}

/// Why a cast's attempt didn't play.
enum Tried {
    /// The device refused the stream: another profile may do.
    Refused,
    /// It didn't start its player, or the stream, in time.
    Failed,
    /// It couldn't be reached, or went away.
    Lost,
    /// The cast's time to start ran out.
    TimedOut,
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
    /// It asks for a stream, whose playlist names the subtitles shown: the
    /// file plays as it is, without them, should none start
    /// ([`start_session`]).
    stream: bool,
    base: &'a str,
    /// Canceled when a newer cast to the device takes this one's place.
    ours: &'a CancellationToken,
    /// When the cast's time to start runs out ([`CAST_WITHIN`]).
    deadline: tokio::time::Instant,
    /// The session of Google's player the try loaded into, on a Cast
    /// device.
    launched: &'a Mutex<Option<String>>,
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
            "mode": if self.stream { "remux" } else { "auto" },
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
        let request = PlaybackRequest { device_id: Some(device.key()), ..request };
        // The session starts, ending the device's last one, only while no
        // newer cast has: otherwise this one would end the newer one's.
        let one = state.nearby.starting.lock().entry(device.key()).or_default().clone();
        let started = {
            let Ok(_one) = tokio::time::timeout_at(self.deadline, one.lock()).await else {
                return Err(Tried::TimedOut);
            };
            if self.ours.is_cancelled() {
                return Err(Tried::Superseded);
            }
            // Started apart, so that the deadline can leave a start that
            // takes too long (a transcoder slow to make the job): what it
            // makes once it is done is then ended, and the device is free
            // for the next cast meanwhile.
            let abandoned = CancellationToken::new();
            let mut starting =
                tokio::spawn(start_session(state.clone(), user.clone(), request, device.key(), abandoned.clone()));
            tokio::select! {
                started = &mut starting => started.unwrap_or_else(|err| {
                    Err(AppError::Internal(anyhow::anyhow!("a cast's session didn't start: {err}")))
                }),
                () = tokio::time::sleep_until(self.deadline) => {
                    abandoned.cancel();
                    let state = state.clone();
                    tokio::spawn(async move {
                        if let Ok(Ok(Json(info))) = starting.await {
                            end_session(&state, &info.session_id);
                        }
                    });
                    return Err(Tried::TimedOut);
                }
            }
        };
        let Json(info) = started.map_err(Tried::Error)?;
        let end = |tried: Tried| {
            end_session(state, &info.session_id);
            tried
        };
        // From here on, a newer cast ends this one where it is: the
        // connection, dropped, closes, and the device is left to the newer
        // cast. So does the end of the cast's time to start, its session
        // ended so that the device can fetch no more of it.
        tokio::select! {
            biased;
            () = self.ours.cancelled() => Err(end(Tried::Superseded)),
            () = tokio::time::sleep_until(self.deadline) => Err(end(Tried::TimedOut)),
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
            live: false,
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
        let ((), connected) = tokio::join!(read, connect(state, device));
        drop(session);
        let started = match connected {
            Ok(connected) => connected.start(&media, self.launched).await,
            Err(failure) => Err(failure),
        };
        match started {
            Ok((player, events)) => Ok((player, events, info.session_id.clone(), info.duration_ms)),
            Err(failure) => {
                tracing::warn!("a cast to {} didn't start: {failure:?}", device.address);
                Err(tried_of(failure))
            }
        }
    }
}

/// Whether `cast` shows text subtitles: those the remote picked, else those
/// the user's choices pick for the title `row`, as its session starts with.
async fn shows_text_subtitles(state: &AppState, user_id: i64, row: &ItemRow, cast: &Cast) -> ApiResult<bool> {
    let files = crate::catalog::files_for_item(&state.db, row.id).await?;
    // The file a session plays when none is named.
    let Some(file) = files.first() else {
        return Ok(false);
    };
    let streams = file.streams();
    let index = match cast.subtitle_index {
        Some(index) => Some(index),
        None => crate::api::playback::subtitles_picked(state, user_id, row, &streams).await?,
    };
    // Text unless the file says otherwise, as the session's tracks are.
    let text = |s: &MediaStream| s.kind == StreamKind::Subtitle && s.text_based != Some(false);
    Ok(index.is_some_and(|index| streams.iter().any(|s| s.index == index && text(s))))
}

/// The devices on the server's network that a cast can send to Pley's web
/// port ([`base_url`]), as [`super::NearbyDevices::listed`] finds them.
/// None, without a look, while casting to them is off ([`turned_on`]) or no
/// device can reach Pley ([`reaches_pley`]). The log says once that devices
/// were left out.
pub async fn reachable(state: &AppState) -> Vec<Found> {
    if !turned_on(state) {
        // Pley keeps none it may not cast to, the device it was told of too.
        state.nearby.forget_while_off(None, false);
        return Vec::new();
    }
    let fixed = state.config.cast_fixed.found();
    let discovery = state.network.wanted().discovery;
    if !reaches_pley(state) {
        // Turned off, discovery forgets what it found all the same; and
        // the log says nothing of devices Pley wouldn't look for anyway.
        state.nearby.forget_while_off(fixed.as_ref(), discovery);
        if discovery || fixed.is_some() {
            say_unreachable(state);
        }
        return Vec::new();
    }
    let found = state.nearby.listed(fixed, discovery).await;
    let all = found.len();
    let reachable: Vec<Found> = found.into_iter().filter(|d| base_url(state, d.address).is_some()).collect();
    if reachable.len() < all {
        say_unreachable(state);
    }
    reachable
}

fn say_unreachable(state: &AppState) {
    if !state.nearby.said_unreachable.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "Google Cast and AirPlay devices that can't reach Pley's web port aren't offered in Play on…: Pley \
             listens on no address of their network (Settings → Network)"
        );
    }
}

/// Whether the administrator turned casting to Google Cast and AirPlay
/// devices on ([`crate::settings::Settings::cast_devices`]): off, Pley
/// neither looks for them nor lists, casts to or pairs with one.
pub fn turned_on(state: &AppState) -> bool {
    state.settings.get().cast_devices
}

/// Whether a device on the network may reach Pley's web port at all: Pley
/// listens beyond this computer, or was told where the device reaches it.
/// Not while it listens on this computer alone (behind a reverse proxy
/// there), where a cast to any device would only fail: Pley doesn't look
/// for them then, to list them, cast or pair.
pub fn reaches_pley(state: &AppState) -> bool {
    state.config.cast_fixed.server.is_some() || listens_on_network(&state.network.listeners())
}

/// The answer to a cast to a device that can't reach Pley's web port.
pub fn unreachable() -> AppError {
    AppError::ConflictAs("cast_unreachable", "Pley doesn't listen on an address the device can reach".to_owned())
}

/// Whether Pley's web port listens beyond this computer: on every address,
/// or on one of a network.
fn listens_on_network(listeners: &[Listener]) -> bool {
    listeners.iter().any(|l| l.kind == Kind::Web && l.error.is_none() && !l.address.ip().is_loopback())
}

/// The web port's listener a device reaches when this computer's address on
/// the way to it is `ip`.
fn web_listener(listeners: &[Listener], ip: IpAddr) -> Option<&Listener> {
    listeners
        .iter()
        .find(|l| l.kind == Kind::Web && l.error.is_none() && (l.address.ip().is_unspecified() || l.address.ip() == ip))
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
    let web = web_listener(&listeners, ip)?;
    Some(format!("http://{}", SocketAddr::new(ip, web.address.port())))
}

/// Starts the session of a cast to the device of `key`, as Pley's own
/// players start one. A stream refused past the server's limit on streams
/// is tried again once ending what the device plays makes room for it
/// ([`end_cast_in_the_way`]); a refusal for any other reason leaves what
/// plays there be. A stream asked for only for the subtitles (`Remux`)
/// that doesn't start, for whatever reason, gives way to what Play would
/// start, the file played as it is where the device plays it. Once the
/// cast's attempt has `abandoned` it, nothing is ended to make room, nor
/// started again.
async fn start_session(
    state: AppState,
    user: AuthUser,
    mut request: PlaybackRequest,
    key: String,
    abandoned: CancellationToken,
) -> ApiResult<Json<crate::playback::PlaybackInfo>> {
    let start = |request| {
        crate::api::playback::start(
            State(state.clone()),
            user.clone(),
            HeaderMap::new(),
            CurrentToken(None),
            Json(request),
        )
    };
    let started = match start(request.clone()).await {
        // Past the server's limit on streams, what plays there leaves the
        // device to this cast when that makes room for it.
        Err(AppError::StreamsFull(pool, message)) if !abandoned.is_cancelled() => {
            match end_cast_in_the_way(&state, &key, pool) {
                Room::Lacking => Err(AppError::StreamsFull(pool, message)),
                Room::Free => start(request.clone()).await,
                Room::Made => {
                    request.keep_previous = false;
                    start(request.clone()).await
                }
            }
        }
        started => started,
    };
    match started {
        // A stream asked for only so that the device shows the subtitles
        // (AirPlay's), which the server didn't make, whatever went wrong:
        // the file plays as it is, without them, as it did before such a
        // cast asked for a stream, rather than nothing. What would keep the
        // file from playing too (an item gone) fails it again.
        Err(err) if request.mode == PlaybackMode::Remux && !abandoned.is_cancelled() => {
            tracing::info!("a cast's subtitles need a stream the server couldn't make ({err}): it plays without them");
            request.mode = PlaybackMode::Auto;
            start(request).await
        }
        started => started,
    }
}

/// The room [`end_cast_in_the_way`] found for a cast's stream.
enum Room {
    /// Not enough, even with what the device plays ended: it plays on.
    Lacking,
    /// A slot freed meanwhile: nothing needed to end.
    Free,
    /// What the device played ended to make it.
    Made,
}

/// Makes room for a cast to the device of `key` whose stream, needing a
/// slot of `pool`, was refused past the server's limit on streams. What the
/// device plays ends only when the stream fits once it has, and not while
/// it plays, counted as the limit counts (the streams whose player was
/// heard from lately, and the play requests still creating their job): at
/// once, so that the next try finds the slot free, and its follower leaves
/// the device to the cast that replaces it. It plays on when ending it
/// would free nothing the stream can take: a file played as it is, a copy
/// while a video transcode is refused, a stream gone quiet, a limit lowered
/// under the streams playing.
fn end_cast_in_the_way(state: &AppState, key: &str, pool: Pool) -> Room {
    let settings = state.settings.get();
    let total = settings.max_transcodes as usize;
    let video = settings.max_video_transcodes.min(settings.max_transcodes) as usize;
    // The device's own sessions: the server names the device of a cast.
    let theirs: Vec<_> = state.sessions.list().into_iter().filter(|s| s.device_id == key).collect();
    let ids: Vec<&str> = theirs.iter().map(|s| s.id.as_str()).collect();
    if state.sessions.full_without(pool, total, video, &ids).is_some() {
        return Room::Lacking;
    }
    if state.sessions.full(pool, total, video).is_none() {
        return Room::Free;
    }
    if let Some((_, before)) = state.nearby.playing.lock().get(key) {
        before.cancel();
    }
    for id in ids {
        end_session(state, id);
    }
    Room::Made
}

/// The answer to a cast that didn't play within its time to start.
fn too_late(device: &Found) -> AppError {
    tracing::warn!("a cast to {} didn't play in time: it is stopped", device.address);
    AppError::ConflictAs("cast_failed", "the device didn't play it in time".to_owned())
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

/// Whether a cast newer than `cast_id` to the device of `key` is starting
/// or plays.
fn newer_on(state: &AppState, key: &str, cast_id: u64) -> bool {
    let newer = |casts: &Mutex<HashMap<String, (u64, CancellationToken)>>| {
        casts.lock().get(key).is_some_and(|(id, _)| *id != cast_id)
    };
    newer(&state.nearby.pending) || newer(&state.nearby.playing)
}

/// Follows a channel's cast until it ends: the remotes' channel changes
/// and volume passed on, and the device stopped when the viewer is stopped
/// in Pley or its connection taken over.
struct Watching {
    state: AppState,
    user: AuthUser,
    device: Found,
    /// The viewer's session.
    session_id: String,
    channel_id: i64,
    cast_id: u64,
}

impl Watching {
    async fn follow(
        self,
        player: Player,
        mut events: tokio::sync::mpsc::Receiver<Event>,
        mut commands: tokio::sync::broadcast::Receiver<ServerEvent>,
        ours: CancellationToken,
    ) {
        let Watching { state, user, device, session_id, channel_id, cast_id } = self;
        let key = device.key();
        let mut ask = tokio::time::interval(ASK_EVERY);
        player.ask_volume().await;
        // Where the last channel change went: the next goes on from there,
        // though it may still be starting.
        let mut tuned = channel_id;
        let ended = loop {
            tokio::select! {
                biased;
                () = ours.cancelled() => {
                    // Another cast took the device: a channel change too.
                    player.leave();
                    break None;
                }
                event = events.recv() => match event {
                    // Live goes on at its edge: there is no place to keep.
                    Some(Event::Status { .. }) => {}
                    Some(Event::Volume(percent)) => {
                        if state.live.tuners.report_volume(&session_id, percent) {
                            state.events.send(ServerEvent::SessionsChanged { user_id: user.id });
                        }
                    }
                    Some(Event::Ended(ended)) => break Some(ended),
                    None => break Some(Ended::Lost),
                },
                command = commands.recv() => match command {
                    Ok(ServerEvent::PlayerCommand { session_id: sid, command, volume, .. }) if sid == session_id => {
                        match command {
                            // A new cast of the channel beside, which takes
                            // this one's place once it plays.
                            Control::Next | Control::Previous => {
                                match state.live.beside(tuned, command == Control::Next).await {
                                    Ok(Some(next)) => {
                                        tuned = next;
                                        let change = change_channel(state.clone(), user.clone(), device.clone(), next);
                                        tokio::spawn(async move {
                                            if let Err(err) = change.await {
                                                tracing::warn!("a cast's channel change didn't play: {err}");
                                            }
                                        });
                                    }
                                    Ok(None) => {}
                                    Err(err) => tracing::warn!("a cast's channel change found no channel: {err}"),
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
                    // Stopped in Pley, or its connection taken over: the
                    // device stops at once.
                    Ok(
                        ServerEvent::SessionStopped { session_id: sid, .. }
                        | ServerEvent::LiveTakenOver { session_id: sid, .. },
                    ) if sid == session_id => {
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
                    if state.live.tuners.present(&session_id).is_none() {
                        // A channel change ends the viewer, and its
                        // playlist, as it starts: the device plays what it
                        // loaded of this one until the new one plays, and
                        // stops should it not. The new viewer starts a
                        // moment before its cast is pending.
                        if newer_on(&state, &key, cast_id) || state.live.tuners.watching_on(user.id, &key) {
                            continue;
                        }
                        if !ours.is_cancelled() {
                            player.stop().await;
                        }
                        break None;
                    }
                    player.status().await;
                }
            }
        };
        state.live.tuners.leave(&session_id, &user);
        forget(&state.nearby.playing, &key, cast_id);
        tracing::info!("a channel's cast to a device on the network ended: {ended:?}");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(kind: Kind, address: &str, error: Option<&str>) -> Listener {
        Listener { kind, address: address.parse().unwrap(), error: error.map(str::to_owned) }
    }

    #[test]
    fn a_device_reaches_the_web_port_where_it_listens_on_the_way_to_it() {
        let ip: IpAddr = "192.168.1.10".parse().unwrap();
        let on = |listeners: &[Listener]| web_listener(listeners, ip).map(|l| l.address.to_string());
        // On every address, or on this one.
        assert_eq!(on(&[listener(Kind::Web, "0.0.0.0:7539", None)]), Some("0.0.0.0:7539".into()));
        assert_eq!(on(&[listener(Kind::Web, "192.168.1.10:7539", None)]), Some("192.168.1.10:7539".into()));
        // Not on another address, nor this computer alone, nor where it
        // couldn't listen, nor another API's port.
        assert_eq!(on(&[listener(Kind::Web, "10.0.0.5:7539", None)]), None);
        assert_eq!(on(&[listener(Kind::Web, "127.0.0.1:7539", None)]), None);
        assert_eq!(on(&[listener(Kind::Web, "0.0.0.0:7539", Some("in use"))]), None);
        assert_eq!(on(&[listener(Kind::Jellyfin, "0.0.0.0:8096", None)]), None);
    }

    #[test]
    fn behind_a_proxy_on_this_computer_pley_listens_on_no_network() {
        assert!(!listens_on_network(&[]));
        assert!(!listens_on_network(&[listener(Kind::Web, "127.0.0.1:7539", None)]));
        assert!(!listens_on_network(&[listener(Kind::Web, "[::1]:7539", None)]));
        assert!(!listens_on_network(&[listener(Kind::Plex, "0.0.0.0:32400", None)]));
        assert!(!listens_on_network(&[listener(Kind::Web, "0.0.0.0:7539", Some("in use"))]));
        assert!(listens_on_network(&[listener(Kind::Web, "0.0.0.0:7539", None)]));
        assert!(listens_on_network(&[listener(Kind::Web, "192.168.1.10:7539", None)]));
    }

    /// A server without a transcoder, as `adjust` configures it.
    async fn server(adjust: impl FnOnce(&mut crate::config::Config)) -> AppState {
        let dir = crate::testing::tempdir();
        let mut config = crate::config::Config::for_data_dir(dir.path());
        config.transcoder_path = Some(dir.path().join("no-transcoder-in-tests"));
        adjust(&mut config);
        AppState::init(config).await.unwrap()
    }

    #[tokio::test]
    async fn behind_a_proxy_pley_still_forgets_the_devices_found_once_discovery_is_off() {
        let tv = Found {
            id: "tv".into(),
            name: "Living room TV".into(),
            model: None,
            address: "192.0.2.20:8009".parse().unwrap(),
            video: true,
            protocol: Protocol::Cast,
        };
        // Pley listens on no network: a test listens nowhere at all.
        for discovery in [true, false] {
            let state = server(|config| config.discovery = discovery).await;
            cast_devices(&state, true).await;
            assert!(!reaches_pley(&state));
            state.nearby.found.lock().insert(tv.key(), (tv.clone(), Instant::now()));
            assert!(reachable(&state).await.is_empty(), "none listed");
            // Turned off, discovery forgets them, and the log says devices
            // were left out only where Pley would look for them.
            assert_eq!(state.nearby.found.lock().is_empty(), !discovery, "discovery {discovery}");
            assert_eq!(state.nearby.said_unreachable.load(Ordering::Relaxed), discovery, "discovery {discovery}");
        }
    }

    /// Turns casting to the devices on the network on or off, as the
    /// administrator does.
    async fn cast_devices(state: &AppState, on: bool) {
        let patch = crate::settings::SettingsPatch { cast_devices: Some(on), ..Default::default() };
        state.settings.update(&state.db, patch).await.unwrap();
    }

    #[tokio::test]
    async fn turned_off_pley_forgets_the_devices_found_and_lists_none() {
        let tv = Found {
            id: "tv".into(),
            name: "Living room TV".into(),
            model: None,
            address: "192.0.2.20:8009".parse().unwrap(),
            video: true,
            protocol: Protocol::Cast,
        };
        // Told where the device reaches Pley, so that only the setting
        // keeps it out.
        let state = server(|config| config.cast_fixed.server = Some("http://pley.test:7539".into())).await;
        assert!(!turned_on(&state), "off on a new server");
        state.nearby.found.lock().insert(tv.key(), (tv.clone(), Instant::now()));
        assert!(reachable(&state).await.is_empty(), "none listed");
        assert!(state.nearby.found.lock().is_empty(), "forgotten");
        assert!(state.nearby.looked.lock().is_none(), "nor looked for");
        cast_devices(&state, true).await;
        assert!(turned_on(&state));
    }

    #[tokio::test]
    async fn a_stream_asked_for_the_subtitles_falls_back_to_the_file_whatever_failed() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicU16;

        use axum::routing::{get, post};
        // A transcoder that answers every job with the status it is given.
        let failing = Arc::new(AtomicU16::new(500));
        let status = failing.clone();
        let answer = move || {
            let status = status.load(Ordering::SeqCst);
            async move { axum::http::StatusCode::from_u16(status).unwrap() }
        };
        let info = pley_transcoder_api::TranscoderInfo {
            name: "pley-transcoder".into(),
            version: "9.9.9".into(),
            api_version: pley_transcoder_api::API_VERSION,
            ..Default::default()
        };
        let transcoder = axum::Router::new()
            .route("/v1/info", get(move || async move { Json(info) }))
            .route("/v1/jobs", post(answer));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, transcoder).await.unwrap() });
        let state = server(|config| {
            config.transcoder_url = Some(format!("http://{address}"));
            config.transcoder_token = Some("secret".into());
        })
        .await;
        state.transcoder.start();
        assert!(state.transcoder.wait_ready(Duration::from_secs(10)).await);
        // A film in H.264 and AAC in MP4, which an AirPlay device plays as
        // it is, with English text subtitles.
        let media = serde_json::json!({
            "container": "mov,mp4,m4a,3gp,3g2,mj2",
            "duration_ms": 7_200_000,
            "streams": [
                {"index": 0, "kind": "video", "codec": "h264", "width": 1920, "height": 1080, "pix_fmt": "yuv420p"},
                {"index": 1, "kind": "audio", "codec": "aac", "channels": 2, "language": "eng", "default": true},
                {"index": 2, "kind": "subtitle", "codec": "mov_text", "language": "eng", "text_based": true},
            ],
        });
        for sql in [
            "INSERT INTO users (id, username, password_hash, is_admin, created_at) VALUES (1, 'sam', '', 1, 0)",
            // Scanned just now: the library watcher's periodic rescan, which
            // starts with the server, leaves it be. A scan of a library
            // without folders would take the film away under the test.
            "INSERT INTO libraries (id, name, kind, paths, created_at, scanned_at)
             VALUES (1, 'Movies', 'movies', '[]', 0, CAST(strftime('%s', 'now') AS INTEGER))",
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'Film', 'film', 0, 0)",
        ] {
            sqlx::query(sql).execute(&state.db.write).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, container, video_codec,
             audio_codec, duration_ms, media_info, added_at, updated_at)
             VALUES (1, 1, 1, '/films/film.mp4', 1, 0, 'mp4', 'h264', 'aac', 7200000, ?, 0, 0)",
        )
        .bind(media.to_string())
        .execute(&state.db.write)
        .await
        .unwrap();
        let user = AuthUser {
            id: 1,
            username: "sam".into(),
            is_admin: true,
            account_admin: true,
            all_libraries: true,
            max_age: None,
            app: None,
            compatibility: None,
        };
        let tv = Found {
            id: "a1b2c3d4e5f6".into(),
            name: "TV".into(),
            model: None,
            address: "192.0.2.20:7000".parse().unwrap(),
            video: true,
            protocol: Protocol::AirPlay,
        };
        // The stream the cast asks for, so that the device shows them.
        let request: PlaybackRequest = serde_json::from_value(serde_json::json!({
            "item_id": 1,
            "subtitle_index": 2,
            "caps": crate::airplay::device::choose(&tv).caps,
            "mode": "remux",
            "client_id": tv.key(),
            "keep_previous": true,
            "keyed_urls": true,
        }))
        .unwrap();
        let request = PlaybackRequest { device_id: Some(tv.key()), ..request };
        // The transcoder fails, refuses the job, or can't take it now: the
        // file plays as it is, as it did before such casts asked for a
        // stream.
        for status in [500, 400, 503] {
            failing.store(status, Ordering::SeqCst);
            let started =
                start_session(state.clone(), user.clone(), request.clone(), tv.key(), CancellationToken::new()).await;
            let Json(info) = started.unwrap_or_else(|err| panic!("{status}: {err:?}"));
            assert_eq!(info.method, Method::Direct, "{status}");
        }
    }
}
