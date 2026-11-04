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
use crate::oauth::clients::{self, clean_name};
use crate::playback::{COMMANDS_PER_SECOND, Control, Controls, DeviceKind, Session};
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
/// pley's own sign-in is a browser. The Android app plays on TVs and
/// phones alike and says which when it knows; until it does, and for apps
/// by other publishers and Jellyfin and Plex apps, the kind isn't known.
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
    /// The programme on now.
    now: Option<String>,
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
                now: channel.now.map(|p| p.title),
            }),
            // Live goes on at the edge: there is no place to show.
            state: "playing".to_owned(),
            position_ms: 0,
            duration_ms: None,
            updated_at: viewer.seen_at_ms / 1000,
            age_ms: 0,
            controls: viewer.controls,
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
            started_at: session.started_at,
            reported_at_ms,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::auth::scopes::Scopes;
    use crate::auth::{AppAccess, CompatibilityAccess, compatibility};

    /// A request's user: pley's own sign-in, an app's (by client id), or a
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
}
