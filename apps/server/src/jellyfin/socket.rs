//! `/socket`: Jellyfin's WebSocket. Clients keep it alive
//! (`ForceKeepAlive`, `KeepAlive`) and hear of changes: `LibraryChanged`
//! for the libraries the user may open (home rows and Jellyfin for Kodi's
//! library refresh), `UserDataChanged` for their watch state, `Playstate`
//! `Stop` when someone else stops their playback, and the commands of the
//! user's remotes as `Playstate` for a device that takes them ([`Remotes`]).
//! The token is checked again every 30 s, and the socket closes once it is
//! revoked.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use super::dto::TICKS_PER_MS;
use super::ids::Kind;
use super::{Jellyfin, JfUser, Q};
use crate::auth::{AuthUser, SourceIp};
use crate::events::ServerEvent;
use crate::playback::{Control, Controls, Session};
use crate::state::AppState;
use crate::util::now;

/// How often the token is checked again.
const RECHECK: Duration = Duration::from_secs(30);
/// What clients are told to keep the socket alive within, in seconds.
const KEEP_ALIVE_SECS: i64 = 60;
/// The most a message from a client may be, in bytes.
const MAX_MESSAGE: usize = 64 * 1024;

/// The most devices kept as taking media control, in memory. Once that
/// many are, those without a socket open are forgotten: a client says so
/// again each time it starts.
const MAX_CONTROLLABLE: usize = 1024;

/// What the user's remotes may send Jellyfin clients: a device that said it
/// takes media control (`SupportsMediaControl` in
/// `/Sessions/Capabilities[/Full]`) and has its socket open obeys
/// [`JELLYFIN_CONTROLS`] there. Both are by user and device id, the
/// `DeviceId` its sessions' `client_id` is.
#[derive(Default)]
pub struct Remotes {
    controllable: Mutex<HashSet<(i64, String)>>,
    /// Open sockets, how many each device has.
    sockets: Mutex<HashMap<(i64, String), usize>>,
}

/// What a Jellyfin client's `Playstate` messages do.
const JELLYFIN_CONTROLS: [Control; 6] =
    [Control::Play, Control::Pause, Control::Seek, Control::Skip, Control::Next, Control::Previous];

impl Remotes {
    /// Records whether `device` of `user_id` takes media control.
    pub(super) fn declare(&self, user_id: i64, device: &str, supports: bool) {
        let key = (user_id, device.to_owned());
        let mut controllable = self.controllable.lock();
        if !supports {
            controllable.remove(&key);
            return;
        }
        if controllable.len() >= MAX_CONTROLLABLE && !controllable.contains(&key) {
            let sockets = self.sockets.lock();
            controllable.retain(|k| sockets.contains_key(k));
            if controllable.len() >= MAX_CONTROLLABLE {
                return;
            }
        }
        controllable.insert(key);
    }

    /// The commands the player of `session` obeys through its Jellyfin
    /// socket: none unless its device took media control and has one open.
    pub(crate) fn controls(&self, session: &Session) -> Controls {
        let Some(device) = session.client_id.as_deref() else { return Controls::new() };
        let key = (session.user_id, device.to_owned());
        if self.controllable.lock().contains(&key) && self.sockets.lock().contains_key(&key) {
            JELLYFIN_CONTROLS.into_iter().collect()
        } else {
            Controls::new()
        }
    }

    /// Counts a socket of `device` open until the guard is dropped.
    fn open(self: &Arc<Self>, user_id: i64, device: Option<&str>) -> Option<OpenSocket> {
        let key = (user_id, device?.to_owned());
        *self.sockets.lock().entry(key.clone()).or_default() += 1;
        Some(OpenSocket { remotes: self.clone(), key })
    }
}

/// A device's socket, counted in [`Remotes`] while it is open.
struct OpenSocket {
    remotes: Arc<Remotes>,
    key: (i64, String),
}

impl Drop for OpenSocket {
    fn drop(&mut self) {
        let mut sockets = self.remotes.sockets.lock();
        if let Some(count) = sockets.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                sockets.remove(&self.key);
            }
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/socket", get(socket))
}

async fn socket(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    SourceIp(from): SourceIp,
    q: Q,
    upgrade: WebSocketUpgrade,
) -> Response {
    let device = q.get("deviceId").map(str::to_owned).or(client.device_id);
    let token = (user.compatibility.map(|c| c.token_id).unwrap_or(0), from);
    // Clients send keep-alives, a few bytes each.
    upgrade
        .max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| run(state, jellyfin, user, token, device, socket))
}

fn message(kind: &str, data: Value) -> Message {
    let id = {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        hex::encode(bytes)
    };
    Message::Text(json!({"MessageType": kind, "MessageId": id, "Data": data}).to_string().into())
}

/// The socket of `user`, its token (`token`: the token's id, and where the
/// socket was opened from, which its origins are checked against again).
async fn run(
    state: AppState,
    jellyfin: Arc<Jellyfin>,
    user: AuthUser,
    token: (i64, Option<std::net::IpAddr>),
    device: Option<String>,
    mut socket: WebSocket,
) {
    let mut events = state.events.subscribe();
    // Remotes control this device while its socket is open, and no longer.
    let _open = state.jellyfin_remotes.open(user.id, device.as_deref());
    if socket.send(message("ForceKeepAlive", json!(KEEP_ALIVE_SECS))).await.is_err() {
        return;
    }
    let mut recheck = tokio::time::interval(RECHECK);
    recheck.tick().await;
    loop {
        tokio::select! {
            received = socket.recv() => match received {
                Some(Ok(Message::Text(text))) => {
                    let kind = serde_json::from_str::<Value>(&text).ok()
                        .and_then(|v| super::field(&v, "MessageType").and_then(|t| t.as_str()).map(str::to_owned));
                    if kind.as_deref() == Some("KeepAlive") && socket.send(message("KeepAlive", Value::Null)).await.is_err() {
                        return;
                    }
                }
                Some(Ok(Message::Ping(bytes))) => {
                    if socket.send(Message::Pong(bytes)).await.is_err() {
                        return;
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return,
            },
            event = events.recv() => match event {
                Ok(event) => {
                    if let Some(out) = translate(&state, &jellyfin, &user, device.as_deref(), &event).await
                        && socket.send(out).await.is_err()
                    {
                        return;
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            _ = recheck.tick() => {
                if state.auth.compatibility_user(token.0, token.1).await.is_none() {
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                }
            }
        }
    }
}

/// What a server event is to this socket's user and device, if anything.
async fn translate(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    device: Option<&str>,
    event: &ServerEvent,
) -> Option<Message> {
    match event {
        ServerEvent::LibraryChanged { library_id } | ServerEvent::ScanFinished { library_id, .. } => {
            let visible = crate::access::visible_libraries(&state.db, user).await.ok()?;
            visible.iter().any(|l| l.id == *library_id).then(|| {
                message(
                    "LibraryChanged",
                    json!({
                        "FoldersAddedTo": [], "FoldersRemovedFrom": [], "ItemsAdded": [], "ItemsRemoved": [],
                        "ItemsUpdated": [], "CollectionFolders": [jellyfin.ids.of(Kind::Library, *library_id)],
                    }),
                )
            })
        }
        ServerEvent::UserStateChanged { user_id, item_ids, .. } if *user_id == user.id => {
            let rows = crate::catalog::items_by_ids(&state.db, user.id, item_ids).await.ok()?;
            let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
            let favorites = super::dto::favorites(&state.db, user.id, &ids).await.ok()?;
            let list: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let favorite = favorites.contains(&row.id);
                    serde_json::to_value(super::dto::user_data(jellyfin, row, favorite, row.duration_ms))
                        .unwrap_or_default()
                })
                .collect();
            Some(message(
                "UserDataChanged",
                json!({"UserId": jellyfin.ids.of(Kind::User, user.id), "UserDataList": list}),
            ))
        }
        ServerEvent::SessionStopped { user_id, session_id, message: text, .. } if *user_id == user.id => {
            // Only the device that plays it stops.
            let session = state.sessions.stopped(session_id)?;
            (device.is_some() && session.client_id.as_deref() == device).then(|| {
                let _ = text;
                message("Playstate", json!({"Command": "Stop"}))
            })
        }
        ServerEvent::PlayerCommand { user_id, session_id, command, position_ms, offset_ms } if *user_id == user.id => {
            // Only the device that plays it obeys, as for Stop.
            let session = state.sessions.get(session_id)?;
            (device.is_some() && session.client_id.as_deref() == device)
                .then(|| playstate(&session, *command, *position_ms, *offset_ms))
        }
        _ => None,
    }
}

/// A remote's command as the `Playstate` message Jellyfin clients obey.
/// Jellyfin has no skip: it is a seek from where the player is by now.
fn playstate(session: &Session, command: Control, position_ms: Option<i64>, offset_ms: Option<i64>) -> Message {
    let seek = |ms: i64| json!({"Command": "Seek", "SeekPositionTicks": ms.max(0).saturating_mul(TICKS_PER_MS)});
    let data = match command {
        Control::Play => json!({"Command": "Unpause"}),
        Control::Pause => json!({"Command": "Pause"}),
        Control::Seek => seek(position_ms.unwrap_or(0)),
        Control::Skip => {
            let mut to = position_now(session).saturating_add(offset_ms.unwrap_or(0));
            if session.duration_ms > 0 {
                to = to.min(session.duration_ms);
            }
            seek(to)
        }
        Control::Next => json!({"Command": "NextTrack"}),
        Control::Previous => json!({"Command": "PreviousTrack"}),
    };
    message("Playstate", data)
}

/// Where the session's player is by now: its last report's place, moved on
/// since while it plays (players report every few seconds).
fn position_now(session: &Session) -> i64 {
    session.play.lock().position_now(now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Method;

    fn data(message: Message) -> Value {
        let Message::Text(text) = message else { panic!("a text message") };
        serde_json::from_str::<Value>(&text).unwrap()["Data"].clone()
    }

    #[test]
    fn a_skip_is_a_seek_from_where_the_player_is_by_now() {
        let session = Session::new(1, "u".into(), Some("d".into()), 1, 1, Method::Direct, 100_000, None, None, 0);
        *session.play.lock() = crate::playback::PlayState {
            position_ms: 50_000,
            state: "playing".into(),
            updated_at: now(),
            reported_at: Some(now() - 10),
        };
        let skip = |offset| data(playstate(&session, Control::Skip, None, Some(offset)))["SeekPositionTicks"].clone();
        // Ten seconds on since its report, a second more should the clock tick.
        let ticks = skip(30_000).as_i64().unwrap();
        assert!((90_000 * TICKS_PER_MS..=91_000 * TICKS_PER_MS).contains(&ticks), "{ticks}");
        assert_eq!(skip(-600_000), json!(0), "not before the start");
        assert_eq!(skip(600_000), json!(100_000 * TICKS_PER_MS), "not past the end");
        session.play.lock().state = "paused".into();
        assert_eq!(skip(-20_000), json!(30_000 * TICKS_PER_MS), "paused, where it was");
        assert_eq!(data(playstate(&session, Control::Play, None, None)), json!({"Command": "Unpause"}));
        assert_eq!(data(playstate(&session, Control::Next, None, None)), json!({"Command": "NextTrack"}));
    }
}
