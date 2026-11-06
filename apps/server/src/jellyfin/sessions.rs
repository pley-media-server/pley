//! `/Sessions`: the user's devices as Jellyfin's sessions, which Jellyfin's
//! apps control and cast to as Pley's own remote does (docs/API.md, "Remote
//! control" and "Casting"). Each device is listed once: the devices that
//! take the user's casts, those that play (with what they play), and the
//! caller's own, as Jellyfin lists every session. `POST /Sessions/{id}/Playing`
//! casts through Pley's casting; `/Playing/{command}` and `/Command` send
//! Pley's remote commands, with their checks and limits. The household's
//! other profiles' devices aren't reached: a cast there switches the device
//! to the caller's profile, which only Pley's own apps may do
//! ([`remote::reaches_household`]).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;
use serde_json::Value;

use super::dto::{Fields, Item, TICKS_PER_MS};
use super::ids::Kind;
use super::users::iso;
use super::{Body, Client, Jellyfin, JfError, JfResult, JfUser, Q};
use crate::api::remote::{self, Cast, CastKind, Command, PlayingChannel, PlayingSession, RemoteDevice, SessionKind};
use crate::auth::AuthUser;
use crate::catalog;
use crate::oauth::clients::clean_name;
use crate::playback::{Control, MENU, Method, QualityOption, Session};
use crate::state::AppState;
use crate::util::now_ms;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sessions", get(sessions))
        .route("/sessions/{id}/playing", post(play))
        .route("/sessions/{id}/playing/{command}", post(playstate))
        .route("/sessions/{id}/command", post(command))
        .route("/sessions/{id}/command/{name}", post(named_command))
}

/// How far `Rewind` and `FastForward` go, as Pley's remotes skip.
const SKIP_BACK_MS: i64 = 10_000;
const SKIP_FORWARD_MS: i64 = 30_000;

// ------------------------------------------------------------------ the list

/// A device as Jellyfin lists a session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfoDto {
    play_state: PlayerState,
    additional_users: [(); 0],
    capabilities: Capabilities,
    playable_media_types: Vec<&'static str>,
    id: String,
    user_id: String,
    user_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client: Option<String>,
    last_activity_date: String,
    last_playback_check_in: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    now_playing_item: Option<Item>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    application_version: Option<String>,
    is_active: bool,
    supports_media_control: bool,
    supports_remote_control: bool,
    now_playing_queue: [(); 0],
    has_custom_device_name: bool,
    server_id: String,
    supported_commands: Vec<&'static str>,
    /// When the device was last heard from (unix ms), which
    /// `ActiveWithinSeconds` goes by.
    #[serde(skip)]
    active_ms: i64,
    /// It plays something, which lists it first.
    #[serde(skip)]
    playing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlayerState {
    #[serde(skip_serializing_if = "Option::is_none")]
    position_ticks: Option<i64>,
    can_seek: bool,
    is_paused: bool,
    is_muted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_stream_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle_stream_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    play_method: Option<&'static str>,
    repeat_mode: &'static str,
    playback_order: &'static str,
}

impl SessionInfoDto {
    /// Whether its place moves on as time does: a title that plays.
    pub(super) fn moves(&self) -> bool {
        self.playing && !self.play_state.is_paused && self.play_state.position_ticks.is_some()
    }
}

impl PlayerState {
    /// A device that plays nothing, or whose playback isn't shown.
    fn idle() -> Self {
        PlayerState {
            position_ticks: None,
            can_seek: false,
            is_paused: false,
            is_muted: false,
            audio_stream_index: None,
            subtitle_stream_index: None,
            media_source_id: None,
            play_method: None,
            repeat_mode: "RepeatNone",
            playback_order: "Default",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Capabilities {
    playable_media_types: Vec<&'static str>,
    supported_commands: Vec<&'static str>,
    supports_media_control: bool,
    supports_persistent_identifier: bool,
}

/// One of the user's devices: where casts go, if it takes them, and what it
/// plays, its newest session.
struct Device {
    device_id: String,
    receiver: Option<RemoteDevice>,
    session: Option<PlayingSession>,
}

/// The user's devices as Pley's remote lists them, each once (by
/// `device_id`), and the caller's own: the user's own receivers, and their
/// sessions whose players are there, a device's newest standing for it.
async fn devices(state: &AppState, user: &AuthUser) -> JfResult<Vec<Device>> {
    // The user's own alone: see the module's comment.
    let receivers = remote::receivers_of(state, user, false).await?;
    let mut out: Vec<Device> = receivers
        .into_iter()
        .map(|receiver| Device { device_id: receiver.device_id.clone(), receiver: Some(receiver), session: None })
        .collect();
    // Oldest first: the last of a device's is its newest.
    for session in remote::playing(state, user).await? {
        match out.iter_mut().find(|d| d.device_id == session.device_id) {
            Some(device) => device.session = Some(session),
            None => out.push(Device { device_id: session.device_id.clone(), receiver: None, session: Some(session) }),
        }
    }
    if let Some(own) = remote::device_id_of(user, None)
        && !out.iter().any(|d| d.device_id == own)
    {
        out.push(Device { device_id: own, receiver: None, session: None });
    }
    Ok(out)
}

/// The device whose session id is `id`, among the user's.
async fn device_of(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser, id: &str) -> JfResult<Device> {
    let wanted = id.trim().replace('-', "").to_ascii_lowercase();
    let devices = devices(state, user).await?;
    devices.into_iter().find(|d| jellyfin.ids.device(user.id, &d.device_id) == wanted).ok_or(JfError::NotFound)
}

/// The caller, whose own device is listed too: what its client says of
/// itself, and when it was last seen (unix ms: now, or when its socket
/// opened).
pub(super) struct Caller<'a> {
    pub client: &'a Client,
    pub seen_ms: i64,
}

/// The user's devices as Jellyfin's sessions: those that play first, then
/// by name.
pub(super) async fn listed(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    caller: &Caller<'_>,
) -> JfResult<Vec<SessionInfoDto>> {
    let name =
        crate::accounts::user(&state.db, user.id).await?.map(|a| a.name).unwrap_or_else(|| user.username.clone());
    let own = remote::device_id_of(user, None);
    let mut out = Vec::new();
    for device in devices(state, user).await? {
        let caller = (own.as_deref() == Some(device.device_id.as_str())).then_some(caller);
        out.push(session_info(state, jellyfin, user, &name, device, caller).await?);
    }
    out.sort_by_cached_key(|s| (!s.playing, s.device_name.as_deref().map(str::to_lowercase), s.id.clone()));
    Ok(out)
}

/// A device as a session: what Pley's remote shows of it, in Jellyfin's
/// words. `caller` for the caller's own.
async fn session_info(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    user_name: &str,
    device: Device,
    caller: Option<&Caller<'_>>,
) -> JfResult<SessionInfoDto> {
    let Device { device_id, receiver, session } = device;
    let token_id = user.compatibility.map(|c| c.token_id).unwrap_or(0);
    // A title's session, as its player started it (Live TV's have none).
    let player =
        session.as_ref().filter(|s| s.kind == SessionKind::Item).and_then(|s| state.sessions.get(&s.session_id));
    let controls = session.as_ref().map(|s| s.controls.clone()).unwrap_or_default();
    let live = session.as_ref().is_some_and(|s| s.kind == SessionKind::Live);
    let takes_items = receiver.as_ref().is_some_and(|r| r.casts.contains(&CastKind::Item));
    // A Jellyfin app is told its own device by the id its client knows
    // itself by; another Jellyfin app's by its session's; Pley's devices,
    // whose ids are Pley's, by their session id.
    let jellyfin_app = device_id.starts_with("token-");
    let client_device = match caller {
        Some(caller) => caller.client.device_id.clone(),
        None if jellyfin_app => player.as_ref().and_then(|p| p.client_id.clone()),
        None => None,
    };
    let id = jellyfin.ids.device(user.id, &device_id);
    // The caller's own, playing nothing, as it said it takes media control.
    let declared =
        caller.is_some() && client_device.as_deref().is_some_and(|d| state.jellyfin_remotes.takes_control(user.id, d));

    let supports_media_control = takes_items || !controls.is_empty() || declared;
    let mut commands = Vec::new();
    for (control, name) in [
        (Control::Audio, "SetAudioStreamIndex"),
        (Control::Subtitles, "SetSubtitleStreamIndex"),
        (Control::Quality, "SetMaxStreamingBitrate"),
    ] {
        if controls.contains(&control) {
            commands.push(name);
        }
    }
    if live && controls.contains(&Control::Next) {
        commands.push("ChannelUp");
    }
    if live && controls.contains(&Control::Previous) {
        commands.push("ChannelDown");
    }
    // Every player stops.
    if session.is_some() {
        commands.push("PlayState");
    }
    if takes_items {
        commands.push("Play");
    }
    let media_types = if takes_items { vec!["Audio", "Video"] } else { Vec::new() };

    let play_state = match &session {
        Some(s) => {
            // Where it is by now: its last report's place, moved on while it
            // plays, as Pley's remote shows it.
            let moved = if s.state == "playing" { s.age_ms } else { 0 };
            let mut position = s.position_ms.saturating_add(moved).max(0);
            if let Some(length) = s.duration_ms {
                position = position.min(length);
            }
            PlayerState {
                position_ticks: (!live).then(|| position.saturating_mul(TICKS_PER_MS)),
                can_seek: controls.contains(&Control::Seek),
                is_paused: s.state == "paused",
                audio_stream_index: s.audio_index,
                subtitle_stream_index: s.subtitle_index,
                media_source_id: player.as_ref().map(|p| jellyfin.ids.source(p.file_id, token_id)),
                play_method: player.as_ref().map(|p| match p.method {
                    Method::Direct => "DirectPlay",
                    Method::Remux => "DirectStream",
                    Method::Transcode => "Transcode",
                }),
                ..PlayerState::idle()
            }
        }
        None => PlayerState::idle(),
    };
    let now_playing_item = match (&session, &player) {
        (Some(_), Some(player)) => now_playing(state, jellyfin, user, player).await?,
        (Some(s), None) => s.channel.as_ref().map(|c| channel_item(jellyfin, c)),
        (None, _) => None,
    };

    let checked_in = session.as_ref().map(|s| s.reported_at_ms);
    let active_ms = [checked_in, receiver.as_ref().map(|r| r.connected_ms), caller.map(|c| c.seen_ms)]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
    let device_name = receiver
        .as_ref()
        .and_then(|r| r.name.clone())
        .or_else(|| session.as_ref().and_then(|s| s.device.clone()))
        .or_else(|| caller.and_then(|c| c.client.device.clone().or_else(|| c.client.client.clone())));
    let client = match caller {
        Some(caller) => caller.client.client.clone(),
        None if jellyfin_app => None,
        None => Some("Pley".to_owned()),
    };
    Ok(SessionInfoDto {
        play_state,
        additional_users: [],
        capabilities: Capabilities {
            playable_media_types: media_types.clone(),
            supported_commands: commands.clone(),
            supports_media_control,
            supports_persistent_identifier: false,
        },
        playable_media_types: media_types,
        device_id: client_device.or_else(|| Some(id.clone())),
        id,
        user_id: jellyfin.ids.of(Kind::User, user.id),
        user_name: user_name.to_owned(),
        client,
        last_activity_date: iso(active_ms / 1000),
        last_playback_check_in: iso(checked_in.unwrap_or(0) / 1000),
        device_name,
        now_playing_item,
        application_version: caller.and_then(|c| c.client.version.clone()),
        is_active: true,
        supports_media_control,
        supports_remote_control: supports_media_control || session.is_some(),
        now_playing_queue: [],
        has_custom_device_name: false,
        server_id: jellyfin.server_id.clone(),
        supported_commands: commands,
        active_ms,
        playing: session.is_some(),
    })
}

/// What `session` plays as Jellyfin's item, with the streams of the file it
/// plays: `None` for a title the user may no longer see.
async fn now_playing(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    session: &Session,
) -> JfResult<Option<Item>> {
    let Some(row) = catalog::item(&state.db, user.id, session.item_id).await? else { return Ok(None) };
    if !super::items::libraries(state, user).await?.iter().any(|l| l.id == row.library_id) {
        return Ok(None);
    }
    let fields = Fields { media_sources: true, ..Fields::default() };
    let Some(mut item) = super::items::dtos(state, jellyfin, user, std::slice::from_ref(&row), fields).await?.pop()
    else {
        return Ok(None);
    };
    let token_id = user.compatibility.map(|c| c.token_id).unwrap_or(0);
    let playing = jellyfin.ids.source(session.file_id, token_id);
    if let Some(source) = item.media_sources.as_ref().and_then(|all| all.iter().find(|s| s.id == playing)) {
        item.media_streams = Some(source.media_streams.clone());
    }
    Ok(Some(item))
}

/// A Live TV channel being watched, as Jellyfin's item.
fn channel_item(jellyfin: &Jellyfin, channel: &PlayingChannel) -> Item {
    Item {
        name: channel.name.clone(),
        server_id: jellyfin.server_id.clone(),
        id: jellyfin.ids.of(Kind::Channel, channel.id),
        kind: "TvChannel",
        media_type: "Video",
        ..Default::default()
    }
}

/// `GET /Sessions`: the user's devices ([`listed`]); with `DeviceId`, that
/// device's; with `ControllableByUserId` (the caller's own id), those its
/// remotes can do something with; with `ActiveWithinSeconds`, those heard
/// from since.
async fn sessions(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    q: Q,
) -> JfResult<Response> {
    let controllable = match q.get("controllableByUserId") {
        // Another user's devices are theirs to control.
        Some(id) if jellyfin.ids.number(Kind::User, id) != Some(user.id) => return Err(JfError::Forbidden),
        Some(_) => true,
        None => false,
    };
    let caller = Caller { client: &client, seen_ms: now_ms() };
    let mut list = listed(&state, &jellyfin, &user, &caller).await?;
    if let Some(device) = q.get("deviceId") {
        list.retain(|s| s.device_id.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(device)));
    }
    if controllable {
        list.retain(|s| s.supports_remote_control);
    }
    if let Some(secs) = q.number("activeWithinSeconds").filter(|s| *s > 0) {
        let since = now_ms().saturating_sub(secs.saturating_mul(1000));
        list.retain(|s| s.active_ms >= since);
    }
    Ok(Json(list).into_response())
}

// ------------------------------------------------------------------ commands

/// A command's parameters: a request's query, or a `GeneralCommand`'s
/// `Arguments`, read in any case.
enum Args<'a> {
    Query(&'a Q),
    Map(Option<&'a Value>),
}

impl Args<'_> {
    fn get(&self, name: &str) -> Option<String> {
        match self {
            Args::Query(q) => q.get(name).map(str::to_owned),
            Args::Map(map) => {
                let value = map.and_then(|m| super::field(m, name))?;
                let text = match value {
                    Value::String(s) => s.trim().to_owned(),
                    Value::Number(n) => n.to_string(),
                    _ => return None,
                };
                (!text.is_empty()).then_some(text)
            }
        }
    }

    fn number(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.trim().parse().ok())
    }

    /// A list: values separated by commas, as Jellyfin's apps send ids.
    fn list(&self, name: &str) -> Vec<String> {
        match self {
            Args::Query(q) => q.list(name),
            Args::Map(_) => {
                let text = self.get(name).unwrap_or_default();
                text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
            }
        }
    }
}

/// `POST /Sessions/{id}/Playing?PlayCommand=PlayNow&ItemIds=…`: casts to the
/// device ([`cast`]).
async fn play(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    Path(id): Path<String>,
    q: Q,
) -> JfResult<StatusCode> {
    let device = device_of(&state, &jellyfin, &user, &id).await?;
    cast(&state, &jellyfin, &user, &client, device, &Args::Query(&q)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Casts to `device` the item `ItemIds` names at `StartIndex` (the first
/// unless given), as Pley's remote casts one title, playlist or channel:
/// `PlayNow` plays it, `PlayShuffle` shuffled; from `StartPositionTicks`,
/// with `AudioStreamIndex` and `SubtitleStreamIndex` for a movie or an
/// episode. Pley's casting doesn't queue: `PlayNext`, `PlayLast` and
/// `PlayInstantMix` answer `400`.
async fn cast(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    client: &Client,
    device: Device,
    args: &Args<'_>,
) -> JfResult<()> {
    let receiver =
        device.receiver.ok_or_else(|| JfError::BadRequest("this device doesn't play what it is sent".into()))?;
    let shuffle = match args.get("playCommand").map(|c| c.to_ascii_lowercase()).as_deref() {
        None | Some("playnow") => false,
        Some("playshuffle") => true,
        Some(_) => return Err(JfError::BadRequest("Pley's casting plays at once (PlayNow, PlayShuffle)".into())),
    };
    let ids = args.list("itemIds");
    let at = args.number("startIndex").unwrap_or(0);
    let id = usize::try_from(at)
        .ok()
        .and_then(|at| ids.get(at))
        .ok_or_else(|| JfError::BadRequest("ItemIds and StartIndex name nothing to play".into()))?;
    let mut cast = Cast::default();
    match jellyfin.ids.read(id) {
        Some((Kind::Item, number)) => {
            cast.item_id = Some(number);
            // Tracks go with a movie or an episode; anything else plays its own.
            let row = catalog::item(&state.db, user.id, number).await?;
            if row.is_some_and(|r| matches!(r.kind.as_str(), "movie" | "episode")) {
                cast.audio_index = args.number("audioStreamIndex");
                cast.subtitle_index = args.number("subtitleStreamIndex");
            }
        }
        Some((Kind::Playlist, number)) => cast.playlist_id = Some(number),
        Some((Kind::Channel, number)) => cast.channel_id = Some(number),
        Some(_) => return Err(JfError::BadRequest("only titles, playlists and channels can be cast".into())),
        None => return Err(JfError::NotFound),
    }
    cast.start_ms = args.number("startPositionTicks").filter(|t| *t > 0).map(|t| t / TICKS_PER_MS);
    cast.shuffle = Some(shuffle);
    // The receiver says where it came from by the app's device.
    let from = client.device.as_deref().or(client.client.as_deref()).and_then(clean_name);
    remote::cast_to(state, user, &receiver.receiver_id, cast, from).await?;
    Ok(())
}

/// `POST /Sessions/{id}/Playing/{command}`: a `Playstate` command to what
/// the device plays ([`transport`]).
async fn playstate(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path((id, command)): Path<(String, String)>,
    q: Q,
) -> JfResult<StatusCode> {
    let device = device_of(&state, &jellyfin, &user, &id).await?;
    transport(&state, &user, &device, &command, &Args::Query(&q)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// What the device plays now: commands other than a stop need something.
fn playing_on(device: &Device) -> JfResult<&PlayingSession> {
    device.session.as_ref().ok_or_else(|| JfError::BadRequest("nothing plays on that device".into()))
}

/// Sends what `device` plays one of Pley's remote commands.
async fn send(state: &AppState, user: &AuthUser, session: &PlayingSession, command: Command) -> JfResult<()> {
    Ok(remote::send_command(state, user, &session.session_id, &command).await?)
}

/// A command of Pley's remote, with nothing more.
fn bare(command: Control) -> Command {
    Command { command, position_ms: None, offset_ms: None, stream_index: None, quality: None }
}

/// A `Playstate` command: `Stop` stops what the device plays, as Pley's
/// remote stops it; `Pause`, `Unpause` and `PlayPause` (by what it does
/// now), `Seek` (to `SeekPositionTicks`), `NextTrack`, `PreviousTrack`, and
/// `Rewind` and `FastForward` (a skip as far as Pley's remotes skip) go to
/// its player as Pley's remote commands.
async fn transport(state: &AppState, user: &AuthUser, device: &Device, command: &str, args: &Args<'_>) -> JfResult<()> {
    let command = command.to_ascii_lowercase();
    if command == "stop" {
        // Nothing to stop is stopped.
        return match &device.session {
            Some(session) => Ok(crate::api::playback::stop_by(state, user, &session.session_id, None)?),
            None => Ok(()),
        };
    }
    let session = playing_on(device)?;
    let order = match command.as_str() {
        "pause" => bare(Control::Pause),
        "unpause" => bare(Control::Play),
        "playpause" if session.state == "paused" => bare(Control::Play),
        "playpause" => bare(Control::Pause),
        "seek" => {
            let ticks = args
                .number("seekPositionTicks")
                .ok_or_else(|| JfError::BadRequest("Seek needs SeekPositionTicks".into()))?;
            Command { position_ms: Some(ticks / TICKS_PER_MS), ..bare(Control::Seek) }
        }
        "nexttrack" => bare(Control::Next),
        "previoustrack" => bare(Control::Previous),
        "rewind" => Command { offset_ms: Some(-SKIP_BACK_MS), ..bare(Control::Skip) },
        "fastforward" => Command { offset_ms: Some(SKIP_FORWARD_MS), ..bare(Control::Skip) },
        _ => return Err(JfError::BadRequest("no such Playstate command".into())),
    };
    send(state, user, session, order).await
}

/// `POST /Sessions/{id}/Command`: a `GeneralCommand`, `{ Name, Arguments }`
/// ([`general`]).
async fn command(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    Path(id): Path<String>,
    body: Body,
) -> JfResult<StatusCode> {
    let device = device_of(&state, &jellyfin, &user, &id).await?;
    let name = body.str("Name").ok_or_else(|| JfError::BadRequest("a command needs its Name".into()))?;
    general(&state, &jellyfin, &user, &client, device, name, &Args::Map(body.get("Arguments"))).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Sessions/{id}/Command/{name}`: a `GeneralCommand` without
/// arguments.
async fn named_command(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    Path((id, name)): Path<(String, String)>,
) -> JfResult<StatusCode> {
    let device = device_of(&state, &jellyfin, &user, &id).await?;
    general(&state, &jellyfin, &user, &client, device, &name, &Args::Map(None)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A `GeneralCommand` that Pley's remote has: `SetAudioStreamIndex` and
/// `SetSubtitleStreamIndex` (`Index`, Pley's stream index; `-1` turns
/// subtitles off), `SetMaxStreamingBitrate` (`MaxBitrate`, as the
/// quality it picks), `ChannelUp` and `ChannelDown` on Live TV, `PlayState`
/// (`Command`, as [`transport`]) and `Play` (as [`cast`]). Others answer
/// `400`.
async fn general(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    client: &Client,
    device: Device,
    name: &str,
    args: &Args<'_>,
) -> JfResult<()> {
    match name.to_ascii_lowercase().as_str() {
        "play" => cast(state, jellyfin, user, client, device, args).await,
        "playstate" => {
            let command =
                args.get("command").ok_or_else(|| JfError::BadRequest("PlayState needs its Command".into()))?;
            transport(state, user, &device, &command, args).await
        }
        audio @ ("setaudiostreamindex" | "setsubtitlestreamindex") => {
            let session = playing_on(&device)?;
            let index = args.number("index").ok_or_else(|| JfError::BadRequest("a track needs its Index".into()))?;
            let control = if audio == "setaudiostreamindex" { Control::Audio } else { Control::Subtitles };
            send(state, user, session, Command { stream_index: Some(index), ..bare(control) }).await
        }
        "setmaxstreamingbitrate" => {
            let session = playing_on(&device)?;
            let bits = args.number("maxBitrate").ok_or_else(|| JfError::BadRequest("it needs MaxBitrate".into()))?;
            let quality = quality_for(&session.qualities, bits)
                .ok_or_else(|| JfError::BadRequest("what plays there has no quality to pick".into()))?;
            send(state, user, session, Command { quality: Some(quality.to_owned()), ..bare(Control::Quality) }).await
        }
        channel @ ("channelup" | "channeldown") => {
            let session = playing_on(&device)?;
            if session.kind != SessionKind::Live {
                return Err(JfError::BadRequest("channels change on Live TV".into()));
            }
            let control = if channel == "channelup" { Control::Next } else { Control::Previous };
            send(state, user, session, bare(control)).await
        }
        _ => Err(JfError::BadRequest("Pley's remote has no such command".into())),
    }
}

/// The quality of `offered` (a player's Quality menu) that a Jellyfin
/// remote's `MaxBitrate` (bits per second) picks: the highest copy within
/// it, the lowest for less than any, and the original for more than every
/// copy. `None` for a menu without a copy or the original (music, Live TV).
fn quality_for(offered: &[QualityOption], max_bitrate: i64) -> Option<&'static str> {
    let kbps = |id: &str| MENU.iter().find(|entry| entry.0 == id).and_then(|entry| entry.2).map(|cap| i64::from(cap.0));
    let copies: Vec<(&'static str, i64)> = offered.iter().filter_map(|q| kbps(q.id).map(|k| (q.id, k))).collect();
    if copies.iter().all(|(_, k)| k.saturating_mul(1000) < max_bitrate) {
        return offered.iter().map(|q| q.id).find(|id| *id == "original");
    }
    let within = copies.iter().filter(|(_, k)| k.saturating_mul(1000) <= max_bitrate).max_by_key(|(_, k)| *k);
    within.or_else(|| copies.iter().min_by_key(|(_, k)| *k)).map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::offered_qualities;

    #[test]
    fn a_bitrate_picks_a_quality_of_the_menu() {
        // A 1080p file of 25 Mbps: its menu offers 20, 15, 12, 8, 4 and 2 Mbps.
        let menu = offered_qualities(Some(1920), Some(1080), Some(25_000_000));
        assert_eq!(quality_for(&menu, 120_000_000), Some("original"), "more than every copy");
        assert_eq!(quality_for(&menu, 20_000_000), Some("20m"));
        assert_eq!(quality_for(&menu, 10_000_000), Some("8m"), "the highest within it");
        assert_eq!(quality_for(&menu, 1_000_000), Some("2m"), "the lowest for less than any");
        assert_eq!(quality_for(&[], 8_000_000), None, "music has none");
    }

    #[test]
    fn arguments_are_read_in_any_case_and_as_numbers() {
        let map = serde_json::json!({"index": "2", "MaxBitrate": 8_000_000, "ItemIds": "a, b,", "Empty": " "});
        let args = Args::Map(Some(&map));
        assert_eq!(args.number("Index"), Some(2));
        assert_eq!(args.number("maxBitrate"), Some(8_000_000));
        assert_eq!(args.list("itemIds"), ["a", "b"]);
        assert_eq!(args.get("empty"), None);
        assert_eq!(Args::Map(None).get("index"), None);
    }
}
