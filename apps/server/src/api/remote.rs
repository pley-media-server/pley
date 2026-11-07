//! Remote control (docs/API.md, "Remote control"): a user's devices list what
//! their other devices play (`GET /api/sessions`) and send those players
//! commands (`POST /api/sessions/{id}/command`), which reach them on the
//! user's event streams as `player_command`. They also list the devices that
//! take casts, apps with an event stream open as a receiver
//! (`GET /api/remote/devices`), and cast a title, a playlist or a channel to
//! one (`POST /api/remote/devices/{id}/cast`, `cast` on the streams). Only
//! the session's own user controls it: administrators stop other people's
//! sessions (`DELETE /api/sessions/{id}`), nothing more. A home member
//! also casts to the household's other profiles' receivers, which switch to
//! the caster's profile first, with a ticket the cast carries (after the
//! caster's PIN, when their profile has one): a receiver plays as the
//! profile of the remote that sent it something. A session is a title's
//! (`/api/playback`) or a Live TV viewer's (`/api/live/channels/{id}/watch`).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use parking_lot::Mutex;
use pley_transcoder_api::{MediaStream, StreamKind};
use serde::{Deserialize, Serialize};

use super::auth::{CAST_ADMIN_TO_BROWSER, CAST_NEEDS_PIN, PIN_NEEDED, SET_UP_FIRST, check_own_pin, switch_target};
use super::items::item_or_404;
use crate::auth::scopes::Scope;
use crate::auth::{AuthUser, CurrentToken, hash_token, new_token};
use crate::catalog::{self, ItemSummary};
use crate::error::{ApiResult, AppError};
use crate::events::{EventBus, ServerEvent, SwitchTicket};
use crate::livetv::tuner::RemoteViewer;
use crate::markers::Marker;
use crate::oauth::clients::{self, clean_name};
use crate::playback::{
    COMMANDS_PER_SECOND, CommandWindow, Control, Controls, DeviceKind, QualityOption, Session, TrackOptions,
    offered_qualities, track_options,
};
use crate::state::AppState;
use crate::util::{now, now_ms};

/// What the user's remotes call a player's device: the player's own name
/// for it (`device_name`), or one from its User-Agent ("Firefox on macOS"),
/// which for other clients is the agent itself, cleaned the same way.
pub(crate) fn device_name(named: Option<&str>, headers: &HeaderMap) -> Option<String> {
    named.and_then(clean_name).or_else(|| clean_name(&crate::accounts::device_name(user_agent(headers))))
}

/// The request's User-Agent, when it says one.
pub(crate) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).filter(|a| !a.is_empty())
}

/// Which device a session plays on, as remotes tell devices apart
/// (`device_id`): the same for everything one sign-in plays, however many
/// tabs, titles or channels. An app's grant (`app-`), a compatibility token
/// (`token-`: a Jellyfin app's sign-in, or a token made in Account for a
/// Plex app) or a browser's session token (`web-`), hashed under a prefix
/// of its own so the hash the token is stored by never shows. `None` with
/// none of these: the session stands for itself.
pub(crate) fn device_id_of(user: &AuthUser, token: Option<&str>) -> Option<String> {
    if let Some(app) = &user.app {
        return Some(format!("app-{}", app.grant_id));
    }
    if let Some(token) = user.compatibility {
        return Some(format!("token-{}", token.token_id));
    }
    let token = token.filter(|t| !t.is_empty())?;
    Some(format!("web-{}", &crate::auth::hash_token(&format!("device:{token}"))[..16]))
}

/// What kind of device a session plays on (`device_kind`): what the player
/// said, else what its app says. Pley's TV apps (Apple TV, Roku, Samsung,
/// LG) and Kodi play on TVs; the iPhone and iPad app on one or the other,
/// as its User-Agent's system says; the Mac and desktop apps on computers;
/// Pley's own sign-in is a browser. The Android app plays on TVs and
/// phones alike and says which when it knows; until it does, and for Pley
/// for Home Assistant (which never plays), apps by other publishers and
/// Jellyfin and Plex apps, the kind isn't known.
pub(crate) fn device_kind_of(declared: Option<DeviceKind>, user: &AuthUser, user_agent: Option<&str>) -> DeviceKind {
    if let Some(kind) = declared {
        return kind;
    }
    match &user.app {
        Some(app) => match &*app.client_id {
            clients::TVOS_ID | clients::ROKU_ID | clients::TIZEN_ID | clients::WEBOS_ID | clients::KODI_ID => {
                DeviceKind::Tv
            }
            clients::IOS_ID => {
                let system =
                    user_agent.and_then(crate::accounts::bracketed).and_then(|inside| inside.split([';', ' ']).next());
                if system == Some("iPadOS") { DeviceKind::Tablet } else { DeviceKind::Phone }
            }
            clients::MACOS_ID | clients::DESKTOP_ID => DeviceKind::Computer,
            _ => DeviceKind::Other,
        },
        None if user.compatibility.is_some() => DeviceKind::Other,
        None => DeviceKind::Browser,
    }
}

/// What a session plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    /// A movie, an episode, a track or an extra.
    Item,
    /// A Live TV channel.
    Live,
}

/// The channel a Live TV session watches.
#[derive(Debug, Clone, Serialize)]
pub struct PlayingChannel {
    id: i64,
    name: String,
    number: Option<String>,
    logo: Option<String>,
    /// The programme on now…
    now: Option<String>,
    /// …when it started and when it ends (unix seconds), for how far in it
    /// is…
    now_start: Option<i64>,
    now_stop: Option<i64>,
    /// …and the one after it.
    next: Option<String>,
}

/// One of the user's sessions as their remotes show it.
#[derive(Debug, Clone, Serialize)]
pub struct PlayingSession {
    session_id: String,
    kind: SessionKind,
    device: Option<String>,
    /// Which device that is ([`device_id_of`]) and what kind
    /// ([`device_kind_of`]).
    device_id: String,
    device_kind: DeviceKind,
    /// What it plays, for `item`.
    item: Option<ItemSummary>,
    /// What it watches, for `live`.
    channel: Option<PlayingChannel>,
    state: String,
    position_ms: i64,
    duration_ms: Option<i64>,
    /// When its player last reported, or a live one asked for the channel
    /// (unix seconds).
    updated_at: i64,
    /// How long ago that was as this answer is made, by the server's clock:
    /// remotes move the place on by this and by the time since the answer
    /// came, while it plays, so a device whose clock is off still shows
    /// where the player is.
    age_ms: i64,
    controls: Controls,
    /// The audio track and the subtitles it plays, by stream index (`-1`:
    /// no subtitles): what its player last said, else what its play
    /// request picked; `None` when not known, and on Live TV.
    audio_index: Option<i64>,
    subtitle_index: Option<i64>,
    /// Its file's tracks, which `audio` and `subtitles` pick from; none on
    /// Live TV.
    tracks: TrackOptions,
    /// The quality its player's menu has picked, by id, as it last said;
    /// `None` until it does, and on Live TV.
    quality: Option<&'static str>,
    /// The qualities its player's menu offers for the file, which `quality`
    /// picks from; none for music and on Live TV.
    qualities: Vec<QualityOption>,
    /// The intro and credits of the file it plays, for a remote's Skip
    /// intro and Skip credits; none on Live TV.
    markers: Vec<Marker>,
    /// For the order they are listed in.
    #[serde(skip)]
    started_at: i64,
    /// When its player last reported, or a live one asked for the channel
    /// (unix ms): what `age_ms` counts from.
    #[serde(skip)]
    reported_at_ms: i64,
}

/// `GET /api/sessions`: what the caller's own players play now, oldest
/// first. Administrators get only theirs too: everyone's is `/api/activity`.
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<PlayingSession>>> {
    let mut playing = playing_items(&state, &user).await?;
    playing.extend(playing_live(&state, &user).await?);
    playing.sort_by_key(|s| s.started_at);
    let now = now_ms();
    for session in &mut playing {
        session.age_ms = (now - session.reported_at_ms).max(0);
    }
    Ok(Json(playing))
}

/// The user's Live TV viewers whose players are there. A channel hidden
/// since isn't listed: its viewer ends with its next request.
async fn playing_live(state: &AppState, user: &AuthUser) -> ApiResult<Vec<PlayingSession>> {
    let mut out = Vec::new();
    for viewer in state.live.tuners.playing(user.id) {
        let channel = match state.live.channel(user, viewer.channel_id).await {
            Ok(channel) => channel,
            Err(AppError::NotFound(_)) => continue,
            Err(err) => return Err(err),
        };
        out.push(PlayingSession {
            session_id: viewer.session_id,
            kind: SessionKind::Live,
            device: viewer.device,
            device_id: viewer.device_id,
            device_kind: viewer.device_kind,
            item: None,
            channel: Some(PlayingChannel {
                id: channel.id,
                name: channel.name,
                number: channel.number,
                logo: channel.logo,
                now_start: channel.now.as_ref().map(|p| p.start),
                now_stop: channel.now.as_ref().map(|p| p.stop),
                now: channel.now.map(|p| p.title),
                next: channel.next.map(|p| p.title),
            }),
            // Live goes on at the edge: there is no place to show.
            state: "playing".to_owned(),
            position_ms: 0,
            duration_ms: None,
            updated_at: viewer.seen_at_ms / 1000,
            age_ms: 0,
            controls: viewer.controls,
            audio_index: None,
            subtitle_index: None,
            tracks: TrackOptions::default(),
            quality: None,
            qualities: Vec::new(),
            markers: Vec::new(),
            started_at: viewer.started_at,
            reported_at_ms: viewer.seen_at_ms,
        });
    }
    Ok(out)
}

/// The user's sessions of the library's titles whose players are there
/// ([`Session::is_active`]).
async fn playing_items(state: &AppState, user: &AuthUser) -> ApiResult<Vec<PlayingSession>> {
    let mut out = Vec::new();
    for session in state.sessions.active().into_iter().filter(|s| s.user_id == user.id) {
        let Some(row) = catalog::item(&state.db, user.id, session.item_id).await? else { continue };
        let file = catalog::file(&state.db, session.file_id).await?;
        let tracks = track_options(&file.as_ref().map(|f| f.streams()).unwrap_or_default());
        let qualities = file_qualities(file.as_ref());
        // Markers belong to a file: another version of the title can be another cut.
        let mut markers = crate::markers::for_item(&state.db, row.id).await?;
        markers.retain(|m| m.file_id == session.file_id);
        let play = session.play.lock().clone();
        let reported_at_ms = play.reported_at_ms.unwrap_or(play.updated_at.saturating_mul(1000));
        out.push(PlayingSession {
            session_id: session.id.clone(),
            kind: SessionKind::Item,
            device: session.device.clone(),
            device_id: session.device_id.clone(),
            device_kind: session.device_kind,
            item: Some(row.to_summary()),
            channel: None,
            state: play.state,
            position_ms: play.position_ms,
            duration_ms: (session.duration_ms > 0).then_some(session.duration_ms),
            // Its place is as of its last report, which a remote moves on
            // from; a request for media since doesn't move it.
            updated_at: reported_at_ms / 1000,
            age_ms: 0,
            controls: controls_of(state, &session),
            audio_index: play.audio_index,
            subtitle_index: play.subtitle_index,
            tracks,
            quality: play.quality,
            qualities,
            markers,
            started_at: session.started_at,
            reported_at_ms,
        });
    }
    Ok(out)
}

/// The qualities a player's menu offers for a file: none for one without
/// video (music), or once the file is gone.
fn file_qualities(file: Option<&catalog::FileRow>) -> Vec<QualityOption> {
    match file {
        Some(f) if f.video_codec.is_some() => offered_qualities(f.width, f.height, f.bitrate),
        _ => Vec::new(),
    }
}

/// What the player of `session` obeys: what it said when it started, or,
/// for a Jellyfin client, what its socket takes while it is open. Plex
/// clients obey only a stop.
fn controls_of(state: &AppState, session: &Session) -> Controls {
    let mut controls = session.controls.clone();
    controls.extend(state.jellyfin_remotes.controls(session));
    controls
}

#[derive(Debug, Deserialize)]
pub struct Command {
    command: Control,
    #[serde(default)]
    position_ms: Option<i64>,
    #[serde(default)]
    offset_ms: Option<i64>,
    #[serde(default)]
    stream_index: Option<i64>,
    #[serde(default)]
    quality: Option<String>,
}

/// What a command carries to its player, each `None` for the commands that
/// don't take it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Values {
    position_ms: Option<i64>,
    offset_ms: Option<i64>,
    stream_index: Option<i64>,
    quality: Option<&'static str>,
}

/// What a title's session picks from: its file's streams (for `audio` and
/// `subtitles`) and the qualities its player offers for it (for `quality`).
/// `None` on Live TV, which takes none of these.
struct Choices {
    streams: Vec<MediaStream>,
    qualities: Vec<QualityOption>,
}

/// The furthest one `skip` goes, either way.
const MAX_SKIP_MS: i64 = 10 * 60 * 1000;
/// The furthest a `seek` goes when the length isn't known (Live TV, a
/// title not yet measured): a day, so players never get a place they
/// can't hold (JavaScript's numbers are exact only to 2^53).
const MAX_UNKNOWN_SEEK_MS: i64 = 24 * 60 * 60 * 1000;

/// `POST /api/sessions/{id}/command`: sends one of the caller's own players
/// a command it said it obeys, through their event streams. `202`: it is
/// on its way; the player's next report says what it did.
pub async fn command(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Result<Json<Command>, JsonRejection>,
) -> ApiResult<StatusCode> {
    let Json(body) = body.map_err(|err| AppError::bad_request(err.body_text()))?;
    let target = controlled(&state, &user, &id)?;
    // A track or a quality is one of the session file's; Live TV has none to pick.
    let choices = match &target {
        Controlled::Item(session) if matches!(body.command, Control::Audio | Control::Subtitles | Control::Quality) => {
            let file = catalog::file(&state.db, session.file_id).await?;
            let streams = file.as_ref().map(|f| f.streams()).unwrap_or_default();
            Some(Choices { streams, qualities: file_qualities(file.as_ref()) })
        }
        Controlled::Item(_) => Some(Choices { streams: Vec::new(), qualities: Vec::new() }),
        Controlled::Live(_) => None,
    };
    let values = command_values(&body, target.duration_ms(), choices.as_ref())?;
    if !target.controls(&state).contains(&body.command) {
        return Err(AppError::ConflictAs(
            "not_controllable",
            format!("this player doesn't take {} from a remote", words(body.command)),
        ));
    }
    let allowed = match &target {
        Controlled::Item(session) => session.take_command(),
        Controlled::Live(viewer) => state
            .live
            .tuners
            .take_command(&viewer.session_id)
            .ok_or_else(|| AppError::not_found("no such session playing"))?,
    };
    if !allowed {
        return Err(AppError::TooManyRequests(format!(
            "{COMMANDS_PER_SECOND} commands a second at most: wait a moment"
        )));
    }
    // On Live TV, next and previous change channel: the player knows the
    // order it tunes them in, so the server only passes them on.
    state.events.send(ServerEvent::PlayerCommand {
        user_id: user.id,
        session_id: id,
        command: body.command,
        position_ms: values.position_ms,
        offset_ms: values.offset_ms,
        stream_index: values.stream_index,
        quality: values.quality,
    });
    Ok(StatusCode::ACCEPTED)
}

/// A session a remote sends a command to.
enum Controlled {
    Item(Arc<Session>),
    Live(RemoteViewer),
}

impl Controlled {
    fn controls(&self, state: &AppState) -> Controls {
        match self {
            Self::Item(session) => controls_of(state, session),
            Self::Live(viewer) => viewer.controls.clone(),
        }
    }

    /// The title's length, or 0 when it isn't known (and on Live TV).
    fn duration_ms(&self) -> i64 {
        match self {
            Self::Item(session) => session.duration_ms,
            Self::Live(_) => 0,
        }
    }
}

/// Session `id`, a title's or a Live TV viewer's, when its player is there
/// to obey and it is the caller's. Someone else's is refused even to an
/// administrator: stopping it is all they may do.
fn controlled(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<Controlled> {
    let (target, user_id) = if let Some(session) = state.sessions.get(id).filter(|s| s.is_active(now())) {
        let user_id = session.user_id;
        (Controlled::Item(session), user_id)
    } else if let Some(viewer) = state.live.tuners.present(id) {
        let user_id = viewer.user_id;
        (Controlled::Live(viewer), user_id)
    } else {
        return Err(AppError::not_found("no such session playing"));
    };
    if user_id != user.id {
        return Err(AppError::forbidden("not your session: only its user controls it"));
    }
    Ok(target)
}

/// The command's place, offset or track, checked: `seek` goes somewhere in
/// the title (of `duration_ms`, 0 when not known: then at most
/// [`MAX_UNKNOWN_SEEK_MS`]), `skip` at most [`MAX_SKIP_MS`] either way;
/// `audio` to one of the file's audio streams, `subtitles` to one of its
/// subtitle streams or `-1` for none, `quality` to one its player offers
/// for the file (`choices` is `None` on Live TV, which takes none of
/// these). The others take nothing.
fn command_values(body: &Command, duration_ms: i64, choices: Option<&Choices>) -> ApiResult<Values> {
    match body.command {
        Control::Seek => {
            let position = body.position_ms.ok_or_else(|| AppError::bad_request("seek needs position_ms"))?;
            if position < 0 {
                return Err(AppError::bad_request("position_ms can't be negative"));
            }
            // A title whose length isn't known goes anywhere in its first
            // day; a live one too, as far as its player's timeshift goes.
            if duration_ms > 0 && position > duration_ms {
                return Err(AppError::bad_request(format!(
                    "position_ms must be at most the title's length, {duration_ms}"
                )));
            }
            if duration_ms <= 0 && position > MAX_UNKNOWN_SEEK_MS {
                return Err(AppError::bad_request(format!(
                    "position_ms must be at most {MAX_UNKNOWN_SEEK_MS} when the length isn't known"
                )));
            }
            Ok(Values { position_ms: Some(position), ..Values::default() })
        }
        Control::Skip => {
            let offset = body.offset_ms.ok_or_else(|| AppError::bad_request("skip needs offset_ms"))?;
            if !(-MAX_SKIP_MS..=MAX_SKIP_MS).contains(&offset) {
                return Err(AppError::bad_request(format!(
                    "offset_ms must be between -{MAX_SKIP_MS} and {MAX_SKIP_MS}"
                )));
            }
            Ok(Values { offset_ms: Some(offset), ..Values::default() })
        }
        Control::Audio | Control::Subtitles => {
            let choices = choices.ok_or_else(|| AppError::bad_request("Live TV has no tracks to pick"))?;
            let index = body
                .stream_index
                .ok_or_else(|| AppError::bad_request(format!("{} needs stream_index", words(body.command))))?;
            let kind = if body.command == Control::Audio { StreamKind::Audio } else { StreamKind::Subtitle };
            check_track(&choices.streams, kind, index, "stream_index")?;
            Ok(Values { stream_index: Some(index), ..Values::default() })
        }
        Control::Quality => {
            let choices = choices.ok_or_else(|| AppError::bad_request("Live TV has no quality to pick"))?;
            let asked = body.quality.as_deref().ok_or_else(|| AppError::bad_request("quality needs quality"))?;
            let id = choices
                .qualities
                .iter()
                .map(|q| q.id)
                .find(|id| *id == asked)
                .ok_or_else(|| AppError::bad_request("quality must be one of the session's qualities"))?;
            Ok(Values { quality: Some(id), ..Values::default() })
        }
        Control::Play | Control::Pause | Control::Next | Control::Previous => Ok(Values::default()),
    }
}

/// That `index`, the request's `field`, is one of `streams`' tracks of
/// `kind`, or `-1` (none) for subtitles.
fn check_track(streams: &[MediaStream], kind: StreamKind, index: i64, field: &str) -> ApiResult<()> {
    let off = kind == StreamKind::Subtitle && index == -1;
    if off || streams.iter().any(|s| s.kind == kind && s.index == index) {
        return Ok(());
    }
    Err(AppError::bad_request(if kind == StreamKind::Audio {
        format!("{field} must be one of the file's audio tracks")
    } else {
        format!("{field} must be one of the file's subtitle tracks, or -1 for none")
    }))
}

fn words(command: Control) -> &'static str {
    match command {
        Control::Play => "play",
        Control::Pause => "pause",
        Control::Seek => "seek",
        Control::Skip => "skip",
        Control::Next => "next",
        Control::Previous => "previous",
        Control::Audio => "audio",
        Control::Subtitles => "subtitles",
        Control::Quality => "quality",
    }
}

// ------------------------------------------------------------------ casting

/// The most receivers one user has at once: every device of a household,
/// not a list without end. A stream past them still gets its events.
const MAX_RECEIVERS_EACH: usize = 32;

/// What a remote casts to a receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CastKind {
    /// A title or a playlist of the library.
    Item,
    /// A Live TV channel.
    Live,
}

impl CastKind {
    fn from_name(name: &str) -> Option<CastKind> {
        match name {
            "item" => Some(CastKind::Item),
            "live" => Some(CastKind::Live),
            _ => None,
        }
    }
}

/// An event stream's `casts`: the kinds named, comma-separated, that this
/// server knows, each once. Others (from a newer app) are left out.
pub(crate) fn cast_kinds(text: &str) -> BTreeSet<CastKind> {
    text.split(',').filter_map(|name| CastKind::from_name(name.trim())).collect()
}

/// Whether an event stream's `casts` names `switch`: its app switches to
/// the profile of a remote of another profile that casts to it, before it
/// plays. Only such receivers are offered across profiles: an app that
/// doesn't would play the cast as the profile it is on.
pub(crate) fn casts_switch(text: &str) -> bool {
    text.split(',').any(|name| name.trim() == "switch")
}

/// An event stream's `receiver`, the app's own id for its device, when it
/// is one: 1 to 128 letters, digits, `.`, `_`, `:` and `-`.
pub(crate) fn receiver_id(text: &str) -> Option<&str> {
    let valid = (1..=128).contains(&text.len())
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'));
    valid.then_some(text)
}

/// A device that takes casts, as its newest event stream says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Receiver {
    /// Which device it is, as sessions say ([`device_id_of`]).
    pub device_id: String,
    pub name: Option<String>,
    /// The name the app gave its device (`device_name`), cleaned, which the
    /// casts it sends as a remote say they come from.
    pub given_name: Option<String>,
    pub kind: DeviceKind,
    pub casts: BTreeSet<CastKind>,
    /// It switches to a casting remote's profile ([`casts_switch`]).
    pub switches: bool,
}

/// A receiver while one of its streams is open.
struct Open {
    receiver: Receiver,
    /// When its newest stream connected: a device's newest receiver is the
    /// one listed.
    connected_at: Instant,
    /// Its streams open: several with the same id count once.
    streams: usize,
    /// Casts sent to it lately, which [`Receivers::take_cast`] counts.
    casts_sent: CommandWindow,
}

/// How long a cast's switch ticket lasts: the receiver redeems it as the
/// cast arrives.
const TICKET_TTL: Duration = Duration::from_secs(60);

/// The most tickets waiting at once; past them, the oldest go.
const MAX_TICKETS: usize = 4096;

/// The most tickets waiting for one device; past them, its oldest go.
const TICKETS_EACH: usize = 4;

/// What a cast's switch ticket lets its receiver do: device `device_id`,
/// on profile `from`, switches to profile `to`, once.
struct Ticket {
    device_id: String,
    from: i64,
    to: i64,
    issued: Instant,
}

/// A receiver as the server keeps it: its device (`device_id`) and the
/// app's own id for it.
type Key = (String, String);

/// The devices the user's remotes cast to: apps with an event stream open
/// as a receiver (`GET /api/events?receiver=`), by user, device and the
/// app's own id for its device, for as long as one is. Also the switch
/// tickets of casts to another profile's receiver, by their hash.
#[derive(Default)]
pub struct Receivers {
    by_user: Mutex<HashMap<i64, HashMap<Key, Open>>>,
    tickets: Mutex<HashMap<String, Ticket>>,
}

impl Receivers {
    /// Counts one of `user_id`'s event streams as receiver `id` of its
    /// device until the guard is dropped: `None` when the user has
    /// [`MAX_RECEIVERS_EACH`] others already. Apps name their own ids, so
    /// another sign-in of the profile's (a child's phone) may name the TV's:
    /// it is a receiver of its own, listed beside the TV, never in its
    /// place, whether the TV is there or comes back later. A cast across
    /// profiles names the device too, so its title and switch ticket reach
    /// the TV alone. A receiver new, or described anew, sends
    /// `sessions_changed`, for the user's remotes to list devices again.
    pub(crate) fn open(
        self: &Arc<Self>,
        events: &EventBus,
        user_id: i64,
        id: &str,
        receiver: Receiver,
    ) -> Option<ReceiverSlot> {
        let key = (receiver.device_id.clone(), id.to_owned());
        let changed = {
            let mut by_user = self.by_user.lock();
            let theirs = by_user.entry(user_id).or_default();
            let full = theirs.len() >= MAX_RECEIVERS_EACH;
            match theirs.get_mut(&key) {
                Some(open) => {
                    open.streams += 1;
                    open.connected_at = Instant::now();
                    let changed = open.receiver != receiver;
                    open.receiver = receiver;
                    changed
                }
                None if full => return None,
                None => {
                    let open = Open {
                        receiver,
                        connected_at: Instant::now(),
                        streams: 1,
                        casts_sent: CommandWindow::default(),
                    };
                    theirs.insert(key.clone(), open);
                    true
                }
            }
        };
        if changed {
            events.send(ServerEvent::SessionsChanged { user_id });
        }
        Some(ReceiverSlot { receivers: self.clone(), events: events.clone(), user_id, key })
    }

    /// `user_id`'s receivers, one per device (the one whose stream connected
    /// last), by name.
    fn listed(&self, user_id: i64) -> Vec<RemoteDevice> {
        let by_user = self.by_user.lock();
        let Some(theirs) = by_user.get(&user_id) else { return Vec::new() };
        let mut newest: HashMap<&str, (&String, &Open)> = HashMap::new();
        for ((device, id), open) in theirs {
            let kept = newest.entry(device).or_insert((id, open));
            if (open.connected_at, id) > (kept.1.connected_at, kept.0) {
                *kept = (id, open);
            }
        }
        let mut out: Vec<RemoteDevice> = newest
            .into_values()
            .map(|(id, open)| RemoteDevice {
                receiver_id: id.clone(),
                device_id: open.receiver.device_id.clone(),
                name: open.receiver.name.clone(),
                kind: open.receiver.kind,
                casts: open.receiver.casts.clone(),
                profile: None,
                switches: open.receiver.switches,
            })
            .collect();
        out.sort_by_cached_key(listing_order);
        out
    }

    /// The name `user_id`'s device `device_id` gave itself as a receiver (its
    /// newest stream's), while it is one.
    fn given_name(&self, user_id: i64, device_id: &str) -> Option<String> {
        let by_user = self.by_user.lock();
        let theirs = by_user.get(&user_id)?;
        let newest = theirs.values().filter(|open| open.receiver.device_id == device_id).max_by_key(|o| o.connected_at);
        newest.and_then(|open| open.receiver.given_name.clone())
    }

    /// Receiver `id` of `user_id` on device `device`, or, without one, the
    /// one of that id whose stream connected last, while it is there.
    fn receiver(&self, user_id: i64, id: &str, device: Option<&str>) -> Option<Receiver> {
        let by_user = self.by_user.lock();
        let of_id =
            by_user.get(&user_id)?.iter().filter(|((d, i), _)| i == id && device.is_none_or(|device| d == device));
        of_id.max_by_key(|(_, open)| open.connected_at).map(|(_, open)| open.receiver.clone())
    }

    /// Counts a cast to receiver `id` of `user_id` on device `device`, the
    /// one the caller checked: whether it may go, at most
    /// [`COMMANDS_PER_SECOND`] each second, or `None` when it has gone
    /// since. Another device of the same id is another receiver, which
    /// neither gets the cast nor counts it.
    fn take_cast(&self, user_id: i64, id: &str, device: &str) -> Option<bool> {
        let mut by_user = self.by_user.lock();
        let open = by_user.get_mut(&user_id)?.get_mut(&(device.to_owned(), id.to_owned()))?;
        Some(open.casts_sent.take())
    }

    /// A ticket for device `device_id`, on profile `from`, to switch to
    /// profile `to` once within [`TICKET_TTL`]. Kept by its hash only. A
    /// device keeps its [`TICKETS_EACH`] newest, so one member's casts never
    /// push out another's.
    fn issue_ticket(&self, device_id: &str, from: i64, to: i64) -> String {
        let ticket = new_token();
        let mut tickets = self.tickets.lock();
        tickets.retain(|_, t| t.issued.elapsed() < TICKET_TTL);
        // The oldest ticket, of `device` or of any.
        let oldest = |tickets: &HashMap<String, Ticket>, device: Option<&str>| {
            let of = tickets.iter().filter(|(_, t)| device.is_none_or(|d| t.device_id == d));
            of.min_by_key(|(_, t)| t.issued).map(|(key, _)| key.clone())
        };
        if tickets.values().filter(|t| t.device_id == device_id).count() >= TICKETS_EACH
            && let Some(key) = oldest(&tickets, Some(device_id))
        {
            tickets.remove(&key);
        }
        if tickets.len() >= MAX_TICKETS
            && let Some(key) = oldest(&tickets, None)
        {
            tickets.remove(&key);
        }
        let entry = Ticket { device_id: device_id.to_owned(), from, to, issued: Instant::now() };
        tickets.insert(hash_token(&ticket), entry);
        ticket
    }

    /// Spends `ticket`: whether it lets device `device_id`, on profile
    /// `from`, switch to profile `to` now. A ticket shown for anything else
    /// is spent too: it was copied.
    pub(crate) fn redeem(&self, ticket: &str, device_id: &str, from: i64, to: i64) -> bool {
        let Some(t) = self.tickets.lock().remove(&hash_token(ticket)) else { return false };
        t.issued.elapsed() < TICKET_TTL && t.device_id == device_id && t.from == from && t.to == to
    }
}

/// One of a receiver's event streams, counted while it lasts.
pub(crate) struct ReceiverSlot {
    receivers: Arc<Receivers>,
    events: EventBus,
    user_id: i64,
    key: Key,
}

impl ReceiverSlot {
    /// Whether this stream is receiver `id` of device `device_id`, as a
    /// cast from another profile names the one it goes to.
    pub(crate) fn is(&self, (id, device_id): (&str, &str)) -> bool {
        self.key.0 == device_id && self.key.1 == id
    }
}

impl Drop for ReceiverSlot {
    fn drop(&mut self) {
        let mut by_user = self.receivers.by_user.lock();
        let Some(theirs) = by_user.get_mut(&self.user_id) else { return };
        let Some(open) = theirs.get_mut(&self.key) else { return };
        open.streams = open.streams.saturating_sub(1);
        if open.streams > 0 {
            return;
        }
        theirs.remove(&self.key);
        if theirs.is_empty() {
            by_user.remove(&self.user_id);
        }
        drop(by_user);
        self.events.send(ServerEvent::SessionsChanged { user_id: self.user_id });
    }
}

/// One of the user's devices that takes casts, as their remotes list it.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteDevice {
    receiver_id: String,
    /// Which device it is: remotes merge it with its sessions by this.
    device_id: String,
    name: Option<String>,
    kind: DeviceKind,
    casts: BTreeSet<CastKind>,
    /// The household's other profile the device is on; left out for the
    /// caller's own. A cast there switches it to the caller's profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<DeviceProfile>,
    #[serde(skip)]
    switches: bool,
}

/// The order remotes list devices in: by name, whatever its case, unnamed
/// last. Two of one name and receiver id (a sign-in that names itself after
/// the TV) go by device, so each keeps its place, and the number the apps
/// tell them apart by.
fn listing_order(d: &RemoteDevice) -> (bool, Option<String>, String, String) {
    (d.name.is_none(), d.name.as_deref().map(str::to_lowercase), d.receiver_id.clone(), d.device_id.clone())
}

/// The profile another of the household's devices is on.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceProfile {
    id: i64,
    name: String,
}

/// `GET /api/remote/devices`'s query.
#[derive(Debug, Default, Deserialize)]
pub struct DevicesQuery {
    /// `1`: the household's other profiles' receivers too. Remotes that
    /// don't know them (older apps) leave it out, and get their own.
    household: Option<String>,
}

/// Whether the request may list and cast to the household's other profiles'
/// devices, which switches them to its profile: a person's own sign-in, or
/// one of Pley's own apps allowed to switch profiles, as a switch itself
/// takes. Another app would learn the household's names and take over its
/// TVs as its own profile.
pub(crate) fn reaches_household(user: &AuthUser) -> bool {
    user.first_party() && user.allowed(Scope::Profiles)
}

/// `GET /api/remote/devices`: the devices that take casts, one each: the
/// caller's own, and with `household=1`, for a home member, those of the
/// household's other profiles that switch to a casting remote's profile,
/// each with the profile it is on (what they play stays theirs).
/// Administrators get only the household's too, and an app only when it
/// [reaches the household](reaches_household).
pub async fn devices(
    State(state): State<AppState>,
    user: AuthUser,
    query: Result<Query<DevicesQuery>, QueryRejection>,
) -> ApiResult<Json<Vec<RemoteDevice>>> {
    let mut out = state.receivers.listed(user.id);
    // A query that doesn't parse asks for nothing more.
    let asked = query.ok().and_then(|Query(q)| q.household).is_some_and(|v| v == "1" || v == "true");
    if !asked || !reaches_household(&user) {
        return Ok(Json(out));
    }
    let everyone = crate::accounts::users(&state.db).await?;
    let home = everyone.iter().any(|u| u.id == user.id && u.home);
    for other in everyone.into_iter().filter(|u| home && u.home && !u.disabled && u.id != user.id) {
        // Its name as "Who's watching?" says it: apps never see usernames.
        let profile = DeviceProfile { id: other.id, name: other.name };
        let theirs = state.receivers.listed(other.id).into_iter().filter(|d| d.switches);
        out.extend(theirs.map(|d| RemoteDevice { profile: Some(profile.clone()), ..d }));
    }
    // A device on two profiles at once is the same one twice only while it
    // switches: the caller's own wins, as its sessions say what it plays.
    let mut seen = HashSet::new();
    out.retain(|d| seen.insert(d.device_id.clone()));
    out.sort_by_cached_key(listing_order);
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct Cast {
    #[serde(default)]
    item_id: Option<i64>,
    /// With `profile_id`: the device the remote listed under the receiver
    /// id (`device_id`). A cast goes to that device or to none, so another
    /// device that names the same id never gets it, nor its ticket.
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    playlist_id: Option<i64>,
    #[serde(default)]
    channel_id: Option<i64>,
    #[serde(default)]
    start_ms: Option<i64>,
    #[serde(default)]
    shuffle: Option<bool>,
    /// The audio track and the subtitles (`-1`: none) the remote had
    /// picked for a movie or an episode, by stream index of one of its
    /// files.
    #[serde(default)]
    audio_index: Option<i64>,
    #[serde(default)]
    subtitle_index: Option<i64>,
    /// The household's profile whose receiver it is, as the device list
    /// says (`profile.id`); the caller's own when left out.
    #[serde(default)]
    profile_id: Option<i64>,
    /// The caller's PIN, when their profile has one and the receiver is on
    /// another profile: it is about to act as theirs.
    #[serde(default)]
    pin: Option<CastPin>,
}

/// A PIN a cast carries: never in Debug output, which logs may carry.
#[derive(Deserialize)]
#[serde(transparent)]
struct CastPin(String);

impl std::fmt::Debug for CastPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CastPin(..)")
    }
}

/// What has tracks to pick: a title of one file's worth.
const WITH_TRACKS: [&str; 2] = ["movie", "episode"];

/// What may be cast from the library: what plays itself, a show or a season
/// (its next episode), an album or an artist (its tracks).
const CASTABLE: [&str; 7] = ["movie", "episode", "show", "season", "track", "album", "artist"];

/// `POST /api/remote/devices/{id}/cast`: sends a receiver a title, a
/// playlist or a Live TV channel to play, through the event streams of the
/// profile it is on. `202`: it is on its way; the device's sessions say what
/// it did. A receiver of another of the household's profiles
/// (`profile_id`) gets a switch ticket with it, which it switches to the
/// caller's profile with before it plays; the caller's PIN comes first when
/// their profile has one (`pin`: `403 pin_needed`, `wrong_pin`,
/// `pin_locked`), throttled and counted as a switch's.
pub async fn cast(
    State(state): State<AppState>,
    user: AuthUser,
    CurrentToken(token): CurrentToken,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Result<Json<Cast>, JsonRejection>,
) -> ApiResult<StatusCode> {
    let Json(body) = body.map_err(|err| AppError::bad_request(err.body_text()))?;
    let owner = body.profile_id.unwrap_or(user.id);
    if owner != user.id && !reaches_household(&user) {
        return Err(AppError::ForbiddenAs(
            "not_allowed",
            format!("only Pley's own apps allowed to {} cast to another profile's device", Scope::Profiles.words()),
        ));
    }
    if owner != user.id {
        // A receiver outside the household is as unknown as one that isn't
        // there.
        let household = switch_target(&state, user.id, owner).await?;
        if !household.is_some_and(|p| p.both_home && !p.disabled) {
            return Err(AppError::not_found("no such device"));
        }
    }
    // Another profile's receiver is the device the remote listed, or none:
    // another device that names the same id (a sign-in that copied the
    // TV's) never gets the caller's profile. A cast to the caller's own is
    // checked against the one of that id that connected last, and reaches
    // all their streams, as ever.
    let listed = if owner == user.id { None } else { body.device_id.as_deref() };
    let receiver = state.receivers.receiver(owner, &id, listed).ok_or_else(|| AppError::not_found("no such device"))?;
    // Another profile's receiver that doesn't switch would play it as that
    // profile: it isn't offered, and is as unknown.
    if owner != user.id && !receiver.switches {
        return Err(AppError::not_found("no such device"));
    }
    if owner != user.id && listed.is_none() {
        return Err(AppError::bad_request("device_id, as the device list says, goes with profile_id"));
    }
    // The device another profile's receiver is, which switches.
    let device = (owner != user.id).then(|| receiver.device_id.clone());
    let picked = body.audio_index.is_some() || body.subtitle_index.is_some();
    let kind = match (body.item_id, body.playlist_id, body.channel_id) {
        (Some(item_id), None, None) => {
            let row = item_or_404(&state, user.id, item_id).await?;
            if !CASTABLE.contains(&row.kind.as_str()) {
                return Err(AppError::bad_request(
                    "only movies, episodes, shows, seasons, tracks, albums and artists can be cast",
                ));
            }
            // The tracks are one of its files': the remote's page picked
            // from the version it shows. The quality stays the receiver's
            // own: what suits a phone's network doesn't suit a TV's.
            if picked {
                if !WITH_TRACKS.contains(&row.kind.as_str()) {
                    return Err(AppError::bad_request("audio_index and subtitle_index go with a movie or an episode"));
                }
                let files = catalog::files_for_item(&state.db, row.id).await?;
                let streams: Vec<MediaStream> = files.iter().flat_map(|f| f.streams()).collect();
                if let Some(index) = body.audio_index {
                    check_track(&streams, StreamKind::Audio, index, "audio_index")?;
                }
                if let Some(index) = body.subtitle_index {
                    check_track(&streams, StreamKind::Subtitle, index, "subtitle_index")?;
                }
            }
            CastKind::Item
        }
        _ if picked && body.item_id.is_none() => {
            return Err(AppError::bad_request("audio_index and subtitle_index go with item_id"));
        }
        (None, Some(playlist_id), None) => {
            if !crate::playlists::is_theirs(&state.db, &user, playlist_id).await? {
                return Err(AppError::not_found(format!("playlist {playlist_id} not found")));
            }
            CastKind::Item
        }
        (None, None, Some(channel_id)) => {
            state.live.check_access(&user).await?;
            state.live.channel(&user, channel_id).await?;
            CastKind::Live
        }
        _ => return Err(AppError::bad_request("name one of item_id, playlist_id and channel_id")),
    };
    if !receiver.casts.contains(&kind) {
        return Err(AppError::ConflictAs(
            "not_castable",
            match kind {
                CastKind::Item => "this device doesn't play titles from a remote".to_owned(),
                CastKind::Live => "this device doesn't play Live TV from a remote".to_owned(),
            },
        ));
    }
    if body.start_ms.is_some_and(|start| !(0..=MAX_UNKNOWN_SEEK_MS).contains(&start)) {
        return Err(AppError::bad_request(format!("start_ms must be between 0 and {MAX_UNKNOWN_SEEK_MS}")));
    }
    // The receiver will act as the caller's profile, by a switch's rules
    // from the profile it is on: a freer profile takes its PIN (there is no
    // password on a TV), one that has to set up two-step sign-in can't be
    // switched to; then the PIN, when it has one.
    if let Some(device) = &device {
        check_cast_switch(&state, owner, user.id, device).await?;
        match &body.pin {
            Some(CastPin(pin)) => {
                check_own_pin(&state, &user, pin).await?;
            }
            None if has_pin(&state, user.id).await? => {
                return Err(AppError::ForbiddenAs("pin_needed", PIN_NEEDED.to_owned()));
            }
            None => {}
        }
    }
    match state.receivers.take_cast(owner, &id, &receiver.device_id) {
        None => return Err(AppError::not_found("no such device")),
        Some(false) => {
            return Err(AppError::TooManyRequests(format!(
                "{COMMANDS_PER_SECOND} casts a second at most: wait a moment"
            )));
        }
        Some(true) => {}
    }
    // The receiver says where it came from by the name the remote gave its
    // device ("Phone"), where the remote is a receiver too: its User-Agent
    // says only which app it is ("Pley for Android").
    let from = device_id_of(&user, token.as_deref())
        .and_then(|device| state.receivers.given_name(user.id, &device))
        .or_else(|| device_name(None, &headers));
    // The device it was counted for, just now.
    let switch_ticket = device.map(|device| SwitchTicket(state.receivers.issue_ticket(&device, owner, user.id)));
    state.events.send(ServerEvent::Cast {
        user_id: owner,
        receiver_id: id,
        device_id: receiver.device_id,
        item_id: body.item_id,
        playlist_id: body.playlist_id,
        channel_id: body.channel_id,
        start_ms: body.start_ms,
        shuffle: body.shuffle.unwrap_or(false),
        audio_index: body.audio_index,
        subtitle_index: body.subtitle_index,
        from,
        profile_id: switch_ticket.is_some().then_some(user.id),
        switch_ticket,
    });
    Ok(StatusCode::ACCEPTED)
}

/// Whether profile `id` has a PIN.
async fn has_pin(state: &AppState, id: i64) -> ApiResult<bool> {
    Ok(switch_target(state, id, id).await?.is_some_and(|p| p.pin_hash.is_some()))
}

/// Whether a receiver on profile `owner` may switch to the caster's
/// profile with a ticket, by a switch's rules as seen from `owner`, without
/// the administrators' exemption (the receiver may be an app, which never
/// has it): a caster freer than `owner` takes a PIN, and one that has to
/// set up two-step sign-in, which the server requires, can't be switched
/// to. An administrator's profile goes only to an app's device
/// (`device_id`), whose tokens never have administrator rights: a browser's
/// or the desktop app's sign-in would stay one with them. The PIN itself is
/// checked next.
async fn check_cast_switch(state: &AppState, owner: i64, caster: i64, device_id: &str) -> ApiResult<()> {
    let (Some(device), Some(to)) =
        (switch_target(state, owner, owner).await?, switch_target(state, owner, caster).await?)
    else {
        return Err(AppError::not_found("no such device"));
    };
    if to.sets_up_two_step(state.auth.two_step_required()) {
        return Err(AppError::ForbiddenAs("not_allowed", SET_UP_FIRST.to_owned()));
    }
    if to.is_admin && !device_id.starts_with("app-") {
        return Err(AppError::ForbiddenAs("not_allowed", CAST_ADMIN_TO_BROWSER.to_owned()));
    }
    let max_age = if device.is_admin { None } else { device.max_age };
    let all_libraries = device.all_libraries || device.is_admin;
    let freer = to.is_admin
        || max_age.is_some_and(|mine| to.max_age.is_none_or(|theirs| theirs > mine))
        || (!all_libraries && (to.all_libraries || to.more_libraries))
        || (!device.is_admin && (to.fewer_hidden_labels || (max_age.is_some() && to.more_shown_labels)));
    if freer && to.pin_hash.is_none() {
        return Err(AppError::ForbiddenAs("not_allowed", CAST_NEEDS_PIN.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::auth::scopes::Scopes;
    use crate::auth::{AppAccess, CompatibilityAccess, compatibility};

    /// A request's user: Pley's own sign-in, an app's (by client id), or a
    /// compatibility token's.
    fn user(app: Option<&str>, token: bool) -> AuthUser {
        AuthUser {
            id: 1,
            username: "sam".into(),
            is_admin: false,
            account_admin: false,
            all_libraries: true,
            max_age: None,
            app: app.map(|client_id| {
                Arc::new(AppAccess { grant_id: 7, client_id: client_id.into(), scopes: Scopes::all(), approved_by: 1 })
            }),
            compatibility: token.then_some(CompatibilityAccess { token_id: 3, scopes: compatibility::Scopes::play() }),
        }
    }

    #[test]
    fn a_device_is_the_sign_in_whatever_it_plays() {
        assert_eq!(device_id_of(&user(Some(clients::ANDROID_ID), false), Some("pley_at_x")), Some("app-7".into()));
        assert_eq!(device_id_of(&user(None, true), None), Some("token-3".into()));
        let web = device_id_of(&user(None, false), Some("s3ss10n")).unwrap();
        assert_eq!(web.len(), "web-".len() + 16, "{web}");
        assert!(web.strip_prefix("web-").unwrap().chars().all(|c| c.is_ascii_hexdigit()), "{web}");
        assert_eq!(device_id_of(&user(None, false), Some("s3ss10n")).unwrap(), web, "the same sign-in");
        assert_ne!(device_id_of(&user(None, false), Some("0ther")).unwrap(), web, "another sign-in");
        assert!(!crate::auth::hash_token("s3ss10n").contains(&web["web-".len()..]), "never the stored hash");
        assert_eq!(device_id_of(&user(None, false), Some("")), None);
        assert_eq!(device_id_of(&user(None, false), None), None);
    }

    #[test]
    fn a_device_kind_is_said_or_told_from_the_app() {
        let kind = |client: &str, agent: Option<&str>| device_kind_of(None, &user(Some(client), false), agent);
        for tv in [clients::TVOS_ID, clients::ROKU_ID, clients::TIZEN_ID, clients::WEBOS_ID, clients::KODI_ID] {
            assert_eq!(kind(tv, None), DeviceKind::Tv, "{tv}");
        }
        assert_eq!(kind(clients::IOS_ID, Some("pleyApple/0.1.0 (iPadOS 16.7; iPad)")), DeviceKind::Tablet);
        assert_eq!(kind(clients::IOS_ID, Some("pleyApple/0.1.0 (iOS 18.1; iPhone)")), DeviceKind::Phone);
        assert_eq!(kind(clients::IOS_ID, None), DeviceKind::Phone);
        assert_eq!(kind(clients::MACOS_ID, None), DeviceKind::Computer);
        assert_eq!(kind(clients::DESKTOP_ID, None), DeviceKind::Computer);
        assert_eq!(kind(clients::ANDROID_ID, Some("pleyAndroid/0.1.0 (Android 14; Den TV)")), DeviceKind::Other);
        assert_eq!(kind("https://apps.example.com/client.json", None), DeviceKind::Other);
        assert_eq!(device_kind_of(None, &user(None, true), Some("Findroid/1.2.0")), DeviceKind::Other);
        assert_eq!(device_kind_of(None, &user(None, false), Some("Mozilla/5.0 Chrome/140.0")), DeviceKind::Browser);
        let said = device_kind_of(Some(DeviceKind::Tv), &user(Some(clients::ANDROID_ID), false), None);
        assert_eq!(said, DeviceKind::Tv, "what the player says wins");
        assert_eq!(device_kind_of(Some(DeviceKind::Phone), &user(None, false), None), DeviceKind::Phone);
    }

    #[test]
    fn a_receiver_id_is_short_and_plain() {
        for id in ["tv-1", "den.TV_2", "app:4f2c", "0"] {
            assert_eq!(receiver_id(id), Some(id));
        }
        assert_eq!(receiver_id(&"a".repeat(128)).map(str::len), Some(128));
        for id in ["", &"a".repeat(129), "tv 1", "tv/1", "tv%201", "t\u{e9}l\u{e9}", "tv\n"] {
            assert_eq!(receiver_id(id), None, "{id:?}");
        }
    }

    #[test]
    fn casts_are_the_kinds_this_server_knows_once() {
        assert_eq!(cast_kinds("live,item,live"), BTreeSet::from([CastKind::Item, CastKind::Live]));
        assert_eq!(cast_kinds(" item ,fax,,"), BTreeSet::from([CastKind::Item]));
        assert!(cast_kinds("").is_empty() && cast_kinds("ITEM").is_empty());
        assert!(casts_switch("item, switch") && !casts_switch("item,live") && !casts_switch("switching"));
        assert_eq!(serde_json::to_string(&cast_kinds("live,item")).unwrap(), r#"["item","live"]"#);
    }

    /// A receiver of device `device_id`, named `name`, that takes `casts`.
    fn receiver(device_id: &str, name: Option<&str>, casts: &str) -> Receiver {
        Receiver {
            device_id: device_id.into(),
            name: name.map(str::to_owned),
            given_name: name.map(str::to_owned),
            kind: DeviceKind::Tv,
            casts: cast_kinds(casts),
            switches: casts_switch(casts),
        }
    }

    /// How many `sessions_changed` were sent since `events` subscribed.
    fn changes(events: &mut tokio::sync::broadcast::Receiver<ServerEvent>) -> usize {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter(|e| matches!(e, ServerEvent::SessionsChanged { user_id: 1 }))
            .count()
    }

    #[test]
    fn a_receivers_streams_count_once_and_it_goes_with_the_last() {
        let (receivers, bus) = (Arc::new(Receivers::default()), EventBus::new());
        let mut events = bus.subscribe();
        let first = receivers.open(&bus, 1, "tv-1", receiver("app-7", Some("Den"), "item")).unwrap();
        assert_eq!(changes(&mut events), 1, "a receiver appeared");
        let second = receivers.open(&bus, 1, "tv-1", receiver("app-7", Some("Den"), "item")).unwrap();
        assert_eq!(changes(&mut events), 0, "the same, described the same");
        let third = receivers.open(&bus, 1, "tv-1", receiver("app-7", Some("Den"), "item,live")).unwrap();
        assert_eq!(changes(&mut events), 1, "the same, described anew");
        let listed = receivers.listed(1);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].casts, cast_kinds("item,live"), "as its newest stream says");
        assert!(receivers.listed(2).is_empty(), "another user's");
        assert_eq!(receivers.receiver(2, "tv-1", None), None);
        drop((first, third));
        assert_eq!((receivers.listed(1).len(), changes(&mut events)), (1, 0), "a stream is still open");
        drop(second);
        assert!(receivers.listed(1).is_empty());
        assert_eq!(changes(&mut events), 1, "it went");
        assert!(receivers.by_user.lock().is_empty(), "nothing is kept of it");
    }

    #[test]
    fn another_device_of_a_receivers_id_is_listed_beside_it_never_in_its_place() {
        let (receivers, bus) = (Arc::new(Receivers::default()), EventBus::new());
        let devices = |receivers: &Receivers| receivers.listed(1).into_iter().map(|d| d.device_id).collect::<Vec<_>>();
        let tv = receivers.open(&bus, 1, "tv-1", receiver("app-7", Some("Den"), "item,switch")).unwrap();
        // Another sign-in of the profile's, with the TV's id and name: a
        // receiver of its own.
        let copy = receivers.open(&bus, 1, "tv-1", receiver("app-9", Some("Den"), "item")).unwrap();
        assert_eq!(devices(&receivers), ["app-7", "app-9"], "both, by device");
        assert!(receivers.receiver(1, "tv-1", Some("app-7")).unwrap().switches, "each as it said");
        assert!(!receivers.receiver(1, "tv-1", Some("app-9")).unwrap().switches);
        assert_eq!(receivers.receiver(1, "tv-1", None).unwrap().device_id, "app-9", "without one, the newest");
        // The TV gone, what goes to it goes nowhere; back, it is listed again.
        drop(tv);
        assert_eq!(receivers.receiver(1, "tv-1", Some("app-7")), None);
        assert_eq!(receivers.take_cast(1, "tv-1", "app-7"), None);
        let _tv = receivers.open(&bus, 1, "tv-1", receiver("app-7", Some("Den"), "item,switch")).unwrap();
        assert_eq!(devices(&receivers), ["app-7", "app-9"]);
        drop(copy);
        assert_eq!(devices(&receivers), ["app-7"]);
    }

    #[test]
    fn a_device_is_listed_once_by_its_newest_receiver() {
        let (receivers, bus) = (Arc::new(Receivers::default()), EventBus::new());
        let _old = receivers.open(&bus, 1, "tab-1", receiver("web-1", Some("Laptop"), "item")).unwrap();
        let new = receivers.open(&bus, 1, "tab-2", receiver("web-1", Some("Laptop"), "item")).unwrap();
        let _tv = receivers.open(&bus, 1, "tv", receiver("app-7", Some("den TV"), "live")).unwrap();
        let _unnamed = receivers.open(&bus, 1, "box", receiver("app-8", None, "item")).unwrap();
        let ids = |listed: Vec<RemoteDevice>| listed.into_iter().map(|d| d.receiver_id).collect::<Vec<_>>();
        assert_eq!(ids(receivers.listed(1)), ["tv", "tab-2", "box"], "by name, whatever its case, unnamed last");
        drop(new);
        assert_eq!(ids(receivers.listed(1)), ["tv", "tab-1", "box"], "the device's other receiver");
    }

    #[test]
    fn a_user_has_a_bounded_number_of_receivers() {
        let (receivers, bus) = (Arc::new(Receivers::default()), EventBus::new());
        let open: Vec<ReceiverSlot> = (0..MAX_RECEIVERS_EACH)
            .map(|n| receivers.open(&bus, 1, &format!("r{n}"), receiver(&format!("app-{n}"), None, "item")).unwrap())
            .collect();
        assert!(receivers.open(&bus, 1, "one-more", receiver("app-x", None, "item")).is_none());
        assert!(receivers.open(&bus, 1, "r0", receiver("app-x", None, "item")).is_none(), "another device's r0");
        assert!(receivers.open(&bus, 1, "r0", receiver("app-0", None, "item")).is_some(), "one already there");
        assert!(receivers.open(&bus, 2, "one-more", receiver("app-x", None, "item")).is_some(), "another user's");
        drop(open);
        assert!(receivers.open(&bus, 1, "one-more", receiver("app-x", None, "item")).is_some());
    }

    #[test]
    fn casts_to_a_receiver_are_limited_each_second() {
        let (receivers, bus) = (Arc::new(Receivers::default()), EventBus::new());
        assert_eq!(receivers.take_cast(1, "tv", "app-7"), None);
        let _tv = receivers.open(&bus, 1, "tv", receiver("app-7", None, "item")).unwrap();
        assert_eq!(receivers.take_cast(1, "tv", "app-8"), None, "another device than the one listed");
        assert!((0..COMMANDS_PER_SECOND).all(|_| receivers.take_cast(1, "tv", "app-7") == Some(true)));
        assert_eq!(receivers.take_cast(1, "tv", "app-7"), Some(false));
        assert_eq!(receivers.take_cast(2, "tv", "app-7"), None, "another user's");
        // Another device of the same id counts its own.
        let _copy = receivers.open(&bus, 1, "tv", receiver("app-8", None, "item")).unwrap();
        assert_eq!(receivers.take_cast(1, "tv", "app-8"), Some(true));
    }

    #[test]
    fn a_device_keeps_its_newest_tickets_and_others_keep_theirs() {
        let receivers = Receivers::default();
        let others: Vec<String> = (0..2).map(|_| receivers.issue_ticket("app-1", 1, 2)).collect();
        let mine: Vec<String> = (0..TICKETS_EACH + 2).map(|_| receivers.issue_ticket("app-2", 1, 2)).collect();
        assert!(!receivers.redeem(&mine[0], "app-2", 1, 2), "its oldest went");
        assert!(receivers.redeem(mine.last().unwrap(), "app-2", 1, 2), "its newest stays");
        assert!(others.iter().all(|t| receivers.redeem(t, "app-1", 1, 2)), "another device's are kept");
    }
}
