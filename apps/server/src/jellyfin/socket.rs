//! `/socket`: Jellyfin's WebSocket. Clients keep it alive
//! (`ForceKeepAlive`, `KeepAlive`) and hear of changes: `LibraryChanged`
//! for the libraries the user may open (home rows and Jellyfin for Kodi's
//! library refresh), `UserDataChanged` for their watch state, `Playstate`
//! `Stop` when someone else stops their playback, and the commands of the
//! user's remotes as `Playstate`, or `GeneralCommand` for a change of
//! track, for a device that takes them ([`Remotes`]). After `SessionsStart`
//! it is sent `Sessions`, the user's devices as `/Sessions` lists them, as
//! they change, until `SessionsStop`.
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
use tokio::time::Instant;

use super::dto::TICKS_PER_MS;
use super::ids::Kind;
use super::{Client, Jellyfin, JfUser, Q};
use crate::auth::{AuthUser, SourceIp};
use crate::events::ServerEvent;
use crate::playback::{Control, Controls, Session};
use crate::state::AppState;
use crate::util::now_ms;

/// How often the token is checked again.
const RECHECK: Duration = Duration::from_secs(30);
/// What clients are told to keep the socket alive within, in seconds.
const KEEP_ALIVE_SECS: i64 = 60;
/// The most a message from a client may be, in bytes.
const MAX_MESSAGE: usize = 64 * 1024;

/// The shortest and longest a socket waits between two `Sessions`, whatever
/// its client asked: each is a list worked out anew. Jellyfin's web app asks
/// for one every 800 ms.
const SESSIONS_MIN: Duration = Duration::from_millis(1000);
const SESSIONS_MAX: Duration = Duration::from_secs(60);
/// The longest it waits before the first.
const SESSIONS_FIRST_MAX: Duration = Duration::from_secs(10);

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

/// What a Jellyfin client's `Playstate` messages do, and its
/// `GeneralCommand`s that change track.
const JELLYFIN_CONTROLS: [Control; 8] = [
    Control::Play,
    Control::Pause,
    Control::Seek,
    Control::Skip,
    Control::Next,
    Control::Previous,
    Control::Audio,
    Control::Subtitles,
];

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
        if self.takes_control(session.user_id, device) {
            JELLYFIN_CONTROLS.into_iter().collect()
        } else {
            Controls::new()
        }
    }

    /// Whether `device` of `user_id` took media control and has its socket
    /// open.
    pub(super) fn takes_control(&self, user_id: i64, device: &str) -> bool {
        let key = (user_id, device.to_owned());
        self.controllable.lock().contains(&key) && self.sockets.lock().contains_key(&key)
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
    let device = q.get("deviceId").map(str::to_owned).or(client.device_id.clone());
    let token = (user.compatibility.map(|c| c.token_id).unwrap_or(0), from);
    let client = Client { device_id: device, ..client };
    // Clients send keep-alives, a few bytes each.
    upgrade
        .max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| run(state, jellyfin, user, token, client, socket))
}

fn message(kind: &str, data: Value) -> Message {
    let id = {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        hex::encode(bytes)
    };
    Message::Text(json!({"MessageType": kind, "MessageId": id, "Data": data}).to_string().into())
}

/// What a `SessionsStart` asked for: `Sessions` when the list changes, and
/// while something plays (its place moves), at most every `every`.
struct Feed {
    every: Duration,
    /// When the list is worked out next.
    next: Instant,
    /// What was sent last, which an unchanged list isn't sent again.
    last: Option<Value>,
}

impl Feed {
    /// `SessionsStart`'s `Data`: `"first,every"` in milliseconds (the wait
    /// before the first, and between two), bounded. Never sooner than
    /// [`SESSIONS_MIN`] after the last one `sent`.
    fn asked(data: Option<&Value>, sent: Option<Instant>) -> Feed {
        let text = data.and_then(Value::as_str).unwrap_or_default();
        let mut parts = text.split(',').map(|part| part.trim().parse::<u64>().ok().map(Duration::from_millis));
        let first = parts.next().flatten().unwrap_or_default().min(SESSIONS_FIRST_MAX);
        let every = parts.next().flatten().unwrap_or(SESSIONS_MIN).clamp(SESSIONS_MIN, SESSIONS_MAX);
        let mut next = Instant::now() + first;
        if let Some(sent) = sent {
            next = next.max(sent + SESSIONS_MIN);
        }
        Feed { every, next, last: None }
    }

    /// The user's devices changed: the list is worked out as soon as it may.
    fn soon(&mut self, sent: Option<Instant>) {
        let soonest = sent.map_or_else(Instant::now, |sent| (sent + SESSIONS_MIN).max(Instant::now()));
        self.next = self.next.min(soonest);
    }
}

/// Waits until `deadline`, or for ever without one.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// The socket of `user`, its token (`token`: the token's id, and where the
/// socket was opened from, which its origins are checked against again),
/// from `client`'s device.
async fn run(
    state: AppState,
    jellyfin: Arc<Jellyfin>,
    user: AuthUser,
    token: (i64, Option<std::net::IpAddr>),
    client: Client,
    mut socket: WebSocket,
) {
    let device = client.device_id.clone();
    let opened_ms = now_ms();
    let mut events = state.events.subscribe();
    // Remotes control this device while its socket is open, and no longer.
    let _open = state.jellyfin_remotes.open(user.id, device.as_deref());
    if socket.send(message("ForceKeepAlive", json!(KEEP_ALIVE_SECS))).await.is_err() {
        return;
    }
    let mut recheck = tokio::time::interval(RECHECK);
    recheck.tick().await;
    let (mut feed, mut sent): (Option<Feed>, Option<Instant>) = (None, None);
    loop {
        tokio::select! {
            received = socket.recv() => match received {
                Some(Ok(Message::Text(text))) => {
                    let parsed = serde_json::from_str::<Value>(&text).ok();
                    let kind = parsed.as_ref().and_then(|v| super::field(v, "MessageType")).and_then(Value::as_str);
                    match kind {
                        Some("KeepAlive") => {
                            if socket.send(message("KeepAlive", Value::Null)).await.is_err() {
                                return;
                            }
                        }
                        Some("SessionsStart") => {
                            feed = Some(Feed::asked(parsed.as_ref().and_then(|v| super::field(v, "Data")), sent));
                        }
                        Some("SessionsStop") => feed = None,
                        _ => {}
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
                    if let (ServerEvent::SessionsChanged { user_id }, Some(feed)) = (&event, feed.as_mut())
                        && *user_id == user.id
                    {
                        feed.soon(sent);
                    }
                    if let Some(out) = translate(&state, &jellyfin, &user, device.as_deref(), &event).await
                        && socket.send(out).await.is_err()
                    {
                        return;
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            _ = until(feed.as_ref().map(|f| f.next)) => {
                let Some(feed) = feed.as_mut() else { continue };
                feed.next = Instant::now() + feed.every;
                let caller = super::sessions::Caller { client: &client, seen_ms: opened_ms };
                // A list that can't be worked out now is tried again next time.
                let Ok(list) = super::sessions::listed(&state, &jellyfin, &user, &caller).await else { continue };
                // Only a place moves without a change the user's devices are
                // told of (a pause, a jump, a device that comes or goes).
                if !list.iter().any(super::sessions::SessionInfoDto::moves) {
                    feed.next = Instant::now() + SESSIONS_MAX;
                }
                let Ok(list) = serde_json::to_value(list) else { continue };
                if feed.last.as_ref() != Some(&list) {
                    if socket.send(message("Sessions", list.clone())).await.is_err() {
                        return;
                    }
                    feed.last = Some(list);
                    sent = Some(Instant::now());
                }
            }
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
        ServerEvent::PlayerCommand { user_id, session_id, command, position_ms, offset_ms, stream_index, .. }
            if *user_id == user.id =>
        {
            // Only the device that plays it obeys, as for Stop.
            let session = state.sessions.get(session_id)?;
            let values = Values { position_ms: *position_ms, offset_ms: *offset_ms, stream_index: *stream_index };
            (device.is_some() && session.client_id.as_deref() == device)
                .then(|| obey(&session, &jellyfin.ids.of(Kind::User, user.id), *command, values))
                .flatten()
        }
        _ => None,
    }
}

/// What a remote's command carries ([`ServerEvent::PlayerCommand`]).
#[derive(Debug, Clone, Copy, Default)]
struct Values {
    position_ms: Option<i64>,
    offset_ms: Option<i64>,
    stream_index: Option<i64>,
}

/// A remote's command as the message Jellyfin clients obey: `Playstate`
/// for the transport, a `GeneralCommand` from `user` (the user's Jellyfin
/// id) to change track, whose index is Pley's, as the client's media
/// streams name them; `-1` turns subtitles off. Jellyfin has no skip: it is
/// a seek from where the player is by now. A change of quality has no
/// message: Jellyfin clients don't take one (the server never sends them
/// one, as they don't list it).
fn obey(session: &Session, user: &str, command: Control, values: Values) -> Option<Message> {
    let seek = |ms: i64| json!({"Command": "Seek", "SeekPositionTicks": ms.max(0).saturating_mul(TICKS_PER_MS)});
    let track = |name: &str| {
        let index = values.stream_index.unwrap_or(-1).to_string();
        message("GeneralCommand", json!({"Name": name, "ControllingUserId": user, "Arguments": {"Index": index}}))
    };
    let data = match command {
        Control::Play => json!({"Command": "Unpause"}),
        Control::Pause => json!({"Command": "Pause"}),
        Control::Seek => seek(values.position_ms.unwrap_or(0)),
        Control::Skip => {
            let mut to = position_now(session).saturating_add(values.offset_ms.unwrap_or(0));
            if session.duration_ms > 0 {
                to = to.min(session.duration_ms);
            }
            seek(to)
        }
        Control::Next => json!({"Command": "NextTrack"}),
        Control::Previous => json!({"Command": "PreviousTrack"}),
        Control::Audio => return Some(track("SetAudioStreamIndex")),
        Control::Subtitles => return Some(track("SetSubtitleStreamIndex")),
        Control::Quality => return None,
    };
    Some(message("Playstate", data))
}

/// Where the session's player is by now: its last report's place, moved on
/// since while it plays (players report every few seconds).
fn position_now(session: &Session) -> i64 {
    session.play.lock().position_now(now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Method;

    fn data(message: Message) -> Value {
        let Message::Text(text) = message else { panic!("a text message") };
        serde_json::from_str::<Value>(&text).unwrap()["Data"].clone()
    }

    fn kind(message: &Message) -> Value {
        let Message::Text(text) = message else { panic!("a text message") };
        serde_json::from_str::<Value>(text).unwrap()["MessageType"].clone()
    }

    fn playstate(session: &Session, command: Control, position_ms: Option<i64>, offset_ms: Option<i64>) -> Message {
        obey(session, "u1", command, Values { position_ms, offset_ms, stream_index: None }).expect("a message")
    }

    #[test]
    fn a_skip_is_a_seek_from_where_the_player_is_by_now() {
        let session = Session::new(1, "u".into(), Some("d".into()), 1, 1, Method::Direct, 100_000, None, None, 0);
        *session.play.lock() = crate::playback::PlayState {
            position_ms: 50_000,
            state: "playing".into(),
            updated_at: now_ms() / 1000,
            reported_at_ms: Some(now_ms() - 10_000),
            audio_index: None,
            subtitle_index: None,
            quality: None,
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

    #[test]
    fn a_track_is_a_general_command() {
        let session = Session::new(1, "u".into(), Some("d".into()), 1, 1, Method::Direct, 100_000, None, None, 0);
        let track = |command, index| {
            obey(&session, "u1", command, Values { stream_index: index, ..Values::default() }).expect("a message")
        };
        let audio = track(Control::Audio, Some(2));
        assert_eq!(kind(&audio), json!("GeneralCommand"));
        let expected = json!({"Name": "SetAudioStreamIndex", "ControllingUserId": "u1", "Arguments": {"Index": "2"}});
        assert_eq!(data(audio), expected);
        let off = data(track(Control::Subtitles, Some(-1)));
        assert_eq!(
            (off["Name"].clone(), off["Arguments"].clone()),
            (json!("SetSubtitleStreamIndex"), json!({"Index": "-1"}))
        );
        assert_eq!(kind(&playstate(&session, Control::Pause, None, None)), json!("Playstate"));
        let quality = Values::default();
        assert!(obey(&session, "u1", Control::Quality, quality).is_none(), "Jellyfin clients take no quality");
    }
}
