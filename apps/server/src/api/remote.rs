//! Remote control (docs/API.md, "Remote control"): a user's devices list what
//! their other devices play (`GET /api/sessions`) and send those players
//! commands (`POST /api/sessions/{id}/command`), which reach them on the
//! user's event streams as `player_command`. Only the session's own user
//! controls it: administrators stop other people's sessions
//! (`DELETE /api/sessions/{id}`), nothing more. A session is a title's
//! (`/api/playback`) or a Live TV viewer's (`/api/live/channels/{id}/watch`).

use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::catalog::{self, ItemSummary};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::livetv::tuner::RemoteViewer;
use crate::oauth::clients::clean_name;
use crate::playback::{COMMANDS_PER_SECOND, Control, Controls, Session};
use crate::state::AppState;
use crate::util::now;

/// What the user's remotes call a player's device: the player's own name
/// for it (`device_name`), or one from its User-Agent ("Firefox on macOS"),
/// which for other clients is the agent itself, cleaned the same way.
pub(crate) fn device_name(named: Option<&str>, headers: &HeaderMap) -> Option<String> {
    named.and_then(clean_name).or_else(|| {
        let agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).filter(|a| !a.is_empty())?;
        clean_name(&crate::accounts::device_name(Some(agent)))
    })
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
    /// The programme on now.
    now: Option<String>,
}

/// One of the user's sessions as their remotes show it.
#[derive(Debug, Clone, Serialize)]
pub struct PlayingSession {
    session_id: String,
    kind: SessionKind,
    device: Option<String>,
    /// What it plays, for `item`.
    item: Option<ItemSummary>,
    /// What it watches, for `live`.
    channel: Option<PlayingChannel>,
    state: String,
    position_ms: i64,
    duration_ms: Option<i64>,
    /// When its player last reported: remotes move the place on from here
    /// while it plays.
    updated_at: i64,
    controls: Controls,
    /// For the order they are listed in.
    #[serde(skip)]
    started_at: i64,
}

/// `GET /api/sessions`: what the caller's own players play now, oldest
/// first. Administrators get only theirs too: everyone's is `/api/activity`.
pub async fn list(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<PlayingSession>>> {
    let mut playing = playing_items(&state, &user).await?;
    playing.extend(playing_live(&state, &user).await?);
    playing.sort_by_key(|s| s.started_at);
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
            item: None,
            channel: Some(PlayingChannel {
                id: channel.id,
                name: channel.name,
                number: channel.number,
                logo: channel.logo,
                now: channel.now.map(|p| p.title),
            }),
            // Live goes on at the edge: there is no place to show.
            state: "playing".to_owned(),
            position_ms: 0,
            duration_ms: None,
            updated_at: viewer.seen_at,
            controls: viewer.controls,
            started_at: viewer.started_at,
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
        let play = session.play.lock().clone();
        out.push(PlayingSession {
            session_id: session.id.clone(),
            kind: SessionKind::Item,
            device: session.device.clone(),
            item: Some(row.to_summary()),
            channel: None,
            state: play.state,
            position_ms: play.position_ms,
            duration_ms: (session.duration_ms > 0).then_some(session.duration_ms),
            // Its place is as of its last report, which a remote moves on
            // from; a request for media since doesn't move it.
            updated_at: play.reported_at.unwrap_or(play.updated_at),
            controls: controls_of(state, &session),
            started_at: session.started_at,
        });
    }
    Ok(out)
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
    let (position_ms, offset_ms) = command_values(&body, target.duration_ms())?;
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
        position_ms,
        offset_ms,
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

/// The command's place or offset, checked: `seek` goes somewhere in the
/// title (of `duration_ms`, 0 when not known: then at most
/// [`MAX_UNKNOWN_SEEK_MS`]), `skip` at most
/// [`MAX_SKIP_MS`] either way. The others take none.
fn command_values(body: &Command, duration_ms: i64) -> ApiResult<(Option<i64>, Option<i64>)> {
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
            Ok((Some(position), None))
        }
        Control::Skip => {
            let offset = body.offset_ms.ok_or_else(|| AppError::bad_request("skip needs offset_ms"))?;
            if !(-MAX_SKIP_MS..=MAX_SKIP_MS).contains(&offset) {
                return Err(AppError::bad_request(format!(
                    "offset_ms must be between -{MAX_SKIP_MS} and {MAX_SKIP_MS}"
                )));
            }
            Ok((None, Some(offset)))
        }
        Control::Play | Control::Pause | Control::Next | Control::Previous => Ok((None, None)),
    }
}

fn words(command: Control) -> &'static str {
    match command {
        Control::Play => "play",
        Control::Pause => "pause",
        Control::Seek => "seek",
        Control::Skip => "skip",
        Control::Next => "next",
        Control::Previous => "previous",
    }
}
