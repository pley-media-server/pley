//! `/socket`: Jellyfin's WebSocket. Clients keep it alive
//! (`ForceKeepAlive`, `KeepAlive`) and hear of changes: `LibraryChanged`
//! for the libraries the user may open (home rows and Jellyfin for Kodi's
//! library refresh), `UserDataChanged` for their watch state, `Playstate`
//! `Stop` when someone else stops their playback. The token is checked
//! again every 30 s, and the socket closes once it is revoked.

use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use super::ids::Kind;
use super::{Jellyfin, JfUser, Q};
use crate::auth::{AuthUser, SourceIp};
use crate::events::ServerEvent;
use crate::state::AppState;

/// How often the token is checked again.
const RECHECK: Duration = Duration::from_secs(30);
/// What clients are told to keep the socket alive within, in seconds.
const KEEP_ALIVE_SECS: i64 = 60;
/// The most a message from a client may be, in bytes.
const MAX_MESSAGE: usize = 64 * 1024;

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
        _ => None,
    }
}
