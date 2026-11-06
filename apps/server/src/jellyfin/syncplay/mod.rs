//! SyncPlay: Jellyfin's apps watch together (`/SyncPlay/…`). An account
//! makes a group, others join it from their devices, and the group plays
//! one queue for everyone: a pause, a seek or someone buffering pauses all,
//! and each member's player starts at the time the server says, by its
//! clock (`/GetUtcTime`). Messages reach members through their socket
//! (`SyncPlayGroupUpdate`, `SyncPlayCommand`).
//!
//! Every account may make and join groups. A group is shown to those who
//! may see everything it plays, who is in it to its members alone (this
//! API shows no one another account's name), and a member who may not see
//! a title is refused it (`LibraryAccessDenied`): a title some member may
//! not see is never queued, and someone who may not see what a group plays
//! doesn't join it. Members are devices, by their app's `DeviceId`. Groups
//! are kept in memory, as Jellyfin keeps them, and bounded: [`MAX_GROUPS`]
//! groups, [`MAX_MEMBERS`] members each, [`MAX_QUEUE`] titles in a queue. A
//! device whose socket stays closed for [`LEAVE_AFTER`] leaves its group.

mod group;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use self::group::{Entry, Group, Key, Member, NO_ID, Readiness, Repeat, Request};
use super::dto::TICKS_PER_MS;
use super::ids::{Ids, Kind};
use super::users::{iso_ms, parse_iso};
use super::{Body, Client, Jellyfin, JfError, JfResult, JfUser};
use crate::auth::AuthUser;
use crate::state::AppState;
use crate::util::now_ms;

/// The most groups at once.
pub const MAX_GROUPS: usize = 64;
/// The most members a group has.
pub const MAX_MEMBERS: usize = 32;
/// The most titles a group's queue holds.
pub const MAX_QUEUE: usize = 500;
/// The longest a group's name is, in characters.
const MAX_NAME_CHARS: usize = 100;
/// How long a member's socket may stay closed before it leaves its group:
/// an app that lost its connection comes back within it, and one that is
/// gone no longer holds the others at every seek.
pub const LEAVE_AFTER: Duration = Duration::from_secs(30);
/// The messages a socket may have waiting; past them, the newest are left
/// out (a member that far behind reports itself again).
const OUTBOX: usize = 64;

/// A message for a member's socket: its type and its data.
pub type Outgoing = (&'static str, Value);

/// A device's open sockets, each by a number of its own.
type Sockets = Vec<(u64, mpsc::Sender<Outgoing>)>;

/// The groups, and the sockets their members hear them through.
#[derive(Default)]
pub struct SyncPlay {
    groups: Mutex<Vec<Group>>,
    sockets: Mutex<HashMap<Key, Sockets>>,
    next_socket: AtomicU64,
}

/// A device's socket, counted while it is open.
pub struct SocketSlot {
    syncplay: Arc<SyncPlay>,
    key: Key,
    id: u64,
}

impl Drop for SocketSlot {
    fn drop(&mut self) {
        let mut sockets = self.syncplay.sockets.lock();
        let gone = match sockets.get_mut(&self.key) {
            Some(open) => {
                open.retain(|(id, _)| *id != self.id);
                open.is_empty()
            }
            None => false,
        };
        if gone {
            sockets.remove(&self.key);
            drop(sockets);
            self.syncplay.leave_unless_back(self.key.clone(), LEAVE_AFTER);
        }
    }
}

/// What a join came to.
enum Joined {
    Yes,
    /// The group is gone.
    Missing,
    /// Its queue changed since it was checked.
    Changed,
    Full,
}

/// Who is in a member's group: its id, its members (to check a queue
/// against) and their version, and how long its queue is.
struct Audience {
    group: String,
    members: Vec<AuthUser>,
    version: u64,
    queued: usize,
}

/// What a request came to.
enum Handled {
    Yes,
    NotInGroup,
    /// Its members changed since the queue was checked against them.
    Changed,
    TooMany,
}

impl SyncPlay {
    /// The messages for `key`'s device, while the slot is held.
    pub fn connect(self: &Arc<Self>, key: Key) -> (mpsc::Receiver<Outgoing>, SocketSlot) {
        let (sender, receiver) = mpsc::channel(OUTBOX);
        let id = self.next_socket.fetch_add(1, Ordering::Relaxed);
        self.sockets.lock().entry(key.clone()).or_default().push((id, sender));
        (receiver, SocketSlot { syncplay: self.clone(), key, id })
    }

    /// Sends `key`'s device a message on each of its sockets.
    fn send_to(&self, key: &Key, kind: &'static str, data: &Value) {
        if let Some(open) = self.sockets.lock().get(key) {
            for (_, sender) in open {
                let _ = sender.try_send((kind, data.clone()));
            }
        }
    }

    /// A group update for a device that isn't in the group it names
    /// (`NotInGroup`, `GroupDoesNotExist`, `LibraryAccessDenied`).
    fn tell(&self, key: &Key, group_id: &str, kind: &str) {
        self.send_to(key, "SyncPlayGroupUpdate", &json!({"GroupId": group_id, "Type": kind, "Data": ""}));
    }

    fn deliver(&self, group: &Group, out: Vec<group::Out>) {
        for message in out {
            for key in group.recipients(&message.to) {
                self.send_to(&key, message.kind, &message.data);
            }
        }
    }

    fn has_socket(&self, key: &Key) -> bool {
        self.sockets.lock().contains_key(key)
    }

    /// Takes `key` out of its group once `after` has passed, unless its
    /// device has a socket open again by then.
    fn leave_unless_back(self: &Arc<Self>, key: Key, after: Duration) {
        let member = self.groups.lock().iter().any(|g| g.members.iter().any(|m| m.key == key));
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        if member {
            let syncplay = self.clone();
            runtime.spawn(async move {
                tokio::time::sleep(after).await;
                if !syncplay.has_socket(&key) {
                    syncplay.leave(&key, now_ms());
                }
            });
        }
    }

    /// Takes `key` out of the group it is in, if it is in one, which goes
    /// once it is empty: whether it was.
    fn leave(&self, key: &Key, now: i64) -> bool {
        let mut groups = self.groups.lock();
        let Some(at) = groups.iter().position(|g| g.members.iter().any(|m| &m.key == key)) else { return false };
        let group = &mut groups[at];
        if let Some(out) = group.leave(key, now) {
            self.deliver(group, out);
        }
        if group.members.is_empty() {
            groups.remove(at);
        }
        true
    }

    /// A new group, named `name`, with `member` in it (out of its group, if
    /// it was in one): its `GroupInfoDto`. `None` with [`MAX_GROUPS`] already.
    fn create(&self, member: Member, name: String, ids: &Ids) -> Option<Value> {
        let now = now_ms();
        let key = member.key.clone();
        {
            // A member alone in its group frees one by leaving it.
            let groups = self.groups.lock();
            let frees = groups.iter().any(|g| g.members.len() == 1 && g.members[0].key == key);
            if groups.len() >= MAX_GROUPS && !frees {
                return None;
            }
        }
        self.leave(&key, now);
        let mut groups = self.groups.lock();
        if groups.len() >= MAX_GROUPS {
            return None;
        }
        let mut group = Group::new(group::new_id(), name, now);
        let out = group.join(member, now, ids);
        self.deliver(&group, out);
        let info = group.info(now, Some(&key));
        groups.push(group);
        Some(info)
    }

    /// The items of group `id`'s queue and its version, while it is there.
    fn queue_of(&self, id: &str) -> Option<(Vec<i64>, u64)> {
        self.groups.lock().iter().find(|g| g.id == id).map(|g| (g.items(), g.queue_version))
    }

    /// Puts `member` in group `id`, its queue as checked (`version`). A
    /// member of it already joins again; one of another group leaves it.
    fn join(&self, member: Member, id: &str, version: u64, ids: &Ids) -> Joined {
        let now = now_ms();
        let key = member.key.clone();
        let mut groups = self.groups.lock();
        let Some(at) = groups.iter().position(|g| g.id == id) else { return Joined::Missing };
        let group = &mut groups[at];
        if group.queue_version != version {
            return Joined::Changed;
        }
        if let Some(already) = group.member_mut(&key) {
            already.user = member.user;
            let out = group.rejoin(&key, now, ids);
            self.deliver(group, out);
            return Joined::Yes;
        }
        if group.members.len() >= MAX_MEMBERS {
            return Joined::Full;
        }
        drop(groups);
        self.leave(&key, now);
        let mut groups = self.groups.lock();
        let Some(group) = groups.iter_mut().find(|g| g.id == id) else { return Joined::Missing };
        if group.queue_version != version {
            return Joined::Changed;
        }
        if group.members.len() >= MAX_MEMBERS {
            return Joined::Full;
        }
        let out = group.join(member, now, ids);
        self.deliver(group, out);
        Joined::Yes
    }

    /// Who is in `key`'s group: `None` when it is in none.
    fn audience(&self, key: &Key) -> Option<Audience> {
        let groups = self.groups.lock();
        let group = groups.iter().find(|g| g.members.iter().any(|m| &m.key == key))?;
        Some(Audience {
            group: group.id.clone(),
            members: group.members.iter().map(|m| m.user.clone()).collect(),
            version: group.members_version,
            queued: group.queue.len(),
        })
    }

    /// `key`'s request to its group, by `user`; with `version`, only while
    /// its members are those checked.
    fn handle(&self, key: &Key, user: &AuthUser, request: Request, version: Option<u64>, ids: &Ids) -> Handled {
        let mut groups = self.groups.lock();
        let Some(group) = groups.iter_mut().find(|g| g.members.iter().any(|m| &m.key == key)) else {
            return Handled::NotInGroup;
        };
        if version.is_some_and(|v| v != group.members_version) {
            return Handled::Changed;
        }
        let Some(member) = group.member_mut(key) else { return Handled::NotInGroup };
        if !member.take_request() {
            return Handled::TooMany;
        }
        // What it may see is as its newest request says.
        member.user = user.clone();
        // Checked before, the queue may have grown since: what is past its
        // bounds isn't queued.
        if let Request::Queue { entries, .. } = &request
            && group.queue.len() + entries.len() > MAX_QUEUE
        {
            return Handled::Yes;
        }
        let out = group.handle(key, request, now_ms(), ids);
        self.deliver(group, out);
        Handled::Yes
    }

    /// Every group's queue and `GroupInfoDto` for device `key`, to show those
    /// who may see it.
    fn snapshot(&self, key: Option<&Key>) -> Vec<(String, Vec<i64>, Value)> {
        let now = now_ms();
        self.groups.lock().iter().map(|g| (g.id.clone(), g.items(), g.info(now, key))).collect()
    }

    #[cfg(test)]
    fn state_of(&self, id: &str) -> Option<group::StateKind> {
        self.groups.lock().iter().find(|g| g.id == id).map(Group::kind)
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/getutctime", get(utc_time))
        .route("/syncplay/list", get(list))
        .route("/syncplay/{id}", get(one))
        .route("/syncplay/new", post(new_group))
        .route("/syncplay/join", post(join))
        .route("/syncplay/leave", post(leave))
        .route("/syncplay/setnewqueue", post(set_new_queue))
        .route("/syncplay/queue", post(queue))
        .route("/syncplay/setplaylistitem", post(set_playlist_item))
        .route("/syncplay/removefromplaylist", post(remove_from_playlist))
        .route("/syncplay/moveplaylistitem", post(move_playlist_item))
        .route("/syncplay/unpause", post(unpause))
        .route("/syncplay/pause", post(pause))
        .route("/syncplay/stop", post(stop))
        .route("/syncplay/seek", post(seek))
        .route("/syncplay/buffering", post(buffering))
        .route("/syncplay/ready", post(ready))
        .route("/syncplay/setignorewait", post(set_ignore_wait))
        .route("/syncplay/nextitem", post(next_item))
        .route("/syncplay/previousitem", post(previous_item))
        .route("/syncplay/setrepeatmode", post(set_repeat_mode))
        .route("/syncplay/setshufflemode", post(set_shuffle_mode))
        .route("/syncplay/ping", post(ping))
}

/// `GET /GetUtcTime`: the server's clock, which members time commands by.
/// Without a token, as Jellyfin answers it: it tells only the time.
async fn utc_time() -> Json<Value> {
    let received = now_ms();
    Json(json!({"RequestReceptionTime": iso_ms(received), "ResponseTransmissionTime": iso_ms(now_ms())}))
}

/// The member a request is from: its user's device, by `DeviceId`.
fn key_of(user: &AuthUser, client: &Client) -> JfResult<Key> {
    match client.device_id.as_deref().filter(|d| d.len() <= 128) {
        Some(device) => Ok((user.id, device.to_owned())),
        None => Err(JfError::BadRequest("SyncPlay needs the app's DeviceId".into())),
    }
}

/// The user's name as "Who's watching?" says it, which a group's other
/// members are shown.
async fn name_of(state: &AppState, user: &AuthUser) -> JfResult<String> {
    let account = crate::accounts::user(&state.db, user.id).await?;
    Ok(account.map(|a| a.name).unwrap_or_else(|| user.username.clone()))
}

/// Whether `user` may see every one of `items`: each in a library they may
/// open, within their age limit.
async fn sees(state: &AppState, user: &AuthUser, items: &[i64]) -> JfResult<bool> {
    if items.is_empty() {
        return Ok(true);
    }
    let unique: Vec<i64> = items.iter().copied().collect::<HashSet<_>>().into_iter().collect();
    let libraries: HashSet<i64> =
        crate::access::visible_libraries(&state.db, user).await?.into_iter().map(|l| l.id).collect();
    let rows = crate::catalog::items_by_ids(&state.db, user.id, &unique).await?;
    Ok(rows.len() == unique.len() && rows.iter().all(|r| libraries.contains(&r.library_id)))
}

/// A GUID as this API writes them: 32 lowercase hexadecimal digits.
fn guid(text: &str) -> Option<String> {
    let hex: String = text.trim().chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    (hex.len() == 32 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

/// A GUID the body names in `field`.
fn guid_of(body: &Body, field: &str) -> JfResult<String> {
    body.str(field).and_then(guid).ok_or_else(|| JfError::BadRequest(format!("{field} must be an id")))
}

/// The strings of a list the body names in `field`, at most `max`.
fn strings(body: &Body, field: &str, max: usize) -> JfResult<Vec<String>> {
    let Some(list) = body.get(field) else { return Ok(Vec::new()) };
    let list = list.as_array().ok_or_else(|| JfError::BadRequest(format!("{field} must be a list")))?;
    if list.len() > max {
        return Err(JfError::BadRequest(format!("{field} holds at most {max}")));
    }
    list.iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(|| JfError::BadRequest(format!("{field} holds ids"))))
        .collect()
}

/// `GET /SyncPlay/List`: the groups the caller may see everything of, with
/// their members only for a member.
async fn list(State(state): State<AppState>, JfUser { user, client }: JfUser) -> JfResult<Response> {
    let key = key_of(&user, &client).ok();
    let mut out = Vec::new();
    for (_, items, info) in state.jellyfin_syncplay.snapshot(key.as_ref()) {
        if sees(&state, &user, &items).await? {
            out.push(info);
        }
    }
    Ok(Json(out).into_response())
}

/// `GET /SyncPlay/{id}`: one of them.
async fn one(
    State(state): State<AppState>,
    JfUser { user, client }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    let id = guid(&id).ok_or(JfError::NotFound)?;
    let key = key_of(&user, &client).ok();
    let found = state.jellyfin_syncplay.snapshot(key.as_ref()).into_iter().find(|(group, ..)| *group == id);
    let (_, items, info) = found.ok_or(JfError::NotFound)?;
    if !sees(&state, &user, &items).await? {
        return Err(JfError::NotFound);
    }
    Ok(Json(info).into_response())
}

/// `POST /SyncPlay/New {GroupName}`: a new group with the caller's device
/// in it (out of the group it was in), its `GroupInfoDto`. Named after the
/// caller without a name.
async fn new_group(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<Response> {
    let key = key_of(&user, &client)?;
    let me = name_of(&state, &user).await?;
    let asked: String =
        body.str("GroupName").unwrap_or_default().chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    let name = if asked.trim().is_empty() { me.clone() } else { asked.trim().to_owned() };
    let member = Member::new(key.clone(), user, me);
    let info = state
        .jellyfin_syncplay
        .create(member, name, &jellyfin.ids)
        .ok_or_else(|| JfError::Unavailable(format!("{MAX_GROUPS} groups at most: try one of those")))?;
    watch(&state, key);
    Ok(Json(info).into_response())
}

/// A member whose device has no socket open is taken out unless one opens
/// in time.
fn watch(state: &AppState, key: Key) {
    if !state.jellyfin_syncplay.has_socket(&key) {
        state.jellyfin_syncplay.leave_unless_back(key, LEAVE_AFTER);
    }
}

/// `POST /SyncPlay/Join {GroupId}`: puts the caller's device in the group,
/// if they may see everything it plays (`LibraryAccessDenied` otherwise),
/// and `GroupDoesNotExist` for one that isn't there.
async fn join(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    let key = key_of(&user, &client)?;
    let id = guid_of(&body, "GroupId")?;
    let name = name_of(&state, &user).await?;
    let syncplay = &state.jellyfin_syncplay;
    // The queue is checked, then the member joins if it is the same still.
    for _ in 0..3 {
        let Some((items, version)) = syncplay.queue_of(&id) else {
            syncplay.tell(&key, NO_ID, "GroupDoesNotExist");
            return Ok(StatusCode::NO_CONTENT);
        };
        if !sees(&state, &user, &items).await? {
            syncplay.tell(&key, &id, "LibraryAccessDenied");
            return Ok(StatusCode::NO_CONTENT);
        }
        match syncplay.join(Member::new(key.clone(), user.clone(), name.clone()), &id, version, &jellyfin.ids) {
            Joined::Yes => {
                watch(&state, key);
                return Ok(StatusCode::NO_CONTENT);
            }
            Joined::Missing => {
                syncplay.tell(&key, NO_ID, "GroupDoesNotExist");
                return Ok(StatusCode::NO_CONTENT);
            }
            Joined::Full => return Err(JfError::Unavailable(format!("a group has {MAX_MEMBERS} members at most"))),
            Joined::Changed => continue,
        }
    }
    Err(JfError::Unavailable("the group's queue keeps changing: try again".into()))
}

/// `POST /SyncPlay/Leave`: takes the caller's device out of its group
/// (`NotInGroup` when it is in none).
async fn leave(State(state): State<AppState>, JfUser { user, client }: JfUser) -> JfResult<StatusCode> {
    let key = key_of(&user, &client)?;
    if !state.jellyfin_syncplay.leave(&key, now_ms()) {
        state.jellyfin_syncplay.tell(&key, NO_ID, "NotInGroup");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// A request to the caller's group: `403` and `NotInGroup` outside one.
async fn send(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    client: &Client,
    request: Request,
) -> JfResult<StatusCode> {
    let key = key_of(user, client)?;
    match state.jellyfin_syncplay.handle(&key, user, request, None, &jellyfin.ids) {
        Handled::Yes | Handled::Changed => Ok(StatusCode::NO_CONTENT),
        Handled::NotInGroup => {
            state.jellyfin_syncplay.tell(&key, NO_ID, "NotInGroup");
            Err(JfError::Forbidden)
        }
        Handled::TooMany => Err(JfError::Unavailable("too many SyncPlay requests: wait a moment".into())),
    }
}

/// Titles to queue, as the body names them in `field` (to add to the queue,
/// `adds`, or to be it): the caller's to see, titles that play, and every
/// member's to see (`LibraryAccessDenied` to the caller otherwise, and
/// nothing queued). `make` makes the request of them, which goes while the
/// group's members are those checked.
#[allow(clippy::too_many_arguments)]
async fn queue_titles(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    client: &Client,
    body: &Body,
    field: &str,
    adds: bool,
    make: impl Fn(Vec<Entry>) -> Request,
) -> JfResult<StatusCode> {
    let key = key_of(user, client)?;
    let syncplay = &state.jellyfin_syncplay;
    let mut items = Vec::new();
    for id in strings(body, field, MAX_QUEUE)? {
        let item = jellyfin.ids.number(Kind::Item, &id);
        items.push(item.ok_or_else(|| JfError::BadRequest(format!("{field} holds ids of titles")))?);
    }
    if items.is_empty() {
        return Err(JfError::BadRequest(format!("{field} names nothing")));
    }
    let rows = crate::catalog::items_by_ids(&state.db, user.id, &items).await?;
    let libraries: HashSet<i64> =
        crate::access::visible_libraries(&state.db, user).await?.into_iter().map(|l| l.id).collect();
    let seen = rows.len() == items.len() && rows.iter().all(|r| libraries.contains(&r.library_id));
    for _ in 0..3 {
        let Some(audience) = syncplay.audience(&key) else {
            syncplay.tell(&key, NO_ID, "NotInGroup");
            return Err(JfError::Forbidden);
        };
        if adds && audience.queued + items.len() > MAX_QUEUE {
            return Err(JfError::BadRequest(format!("a queue holds {MAX_QUEUE} titles at most")));
        }
        if !seen {
            syncplay.tell(&key, &audience.group, "LibraryAccessDenied");
            return Ok(StatusCode::NO_CONTENT);
        }
        if !rows.iter().all(|r| super::dto::is_playable(&r.kind)) {
            return Err(JfError::BadRequest("only titles that play are queued: movies, episodes, tracks".into()));
        }
        for member in audience.members.iter().filter(|m| m.id != user.id) {
            if !sees(state, member, &items).await? {
                syncplay.tell(&key, &audience.group, "LibraryAccessDenied");
                return Ok(StatusCode::NO_CONTENT);
            }
        }
        let entries =
            rows.iter().map(|r| Entry::new(r.id, r.duration_ms.unwrap_or(0).saturating_mul(TICKS_PER_MS))).collect();
        match syncplay.handle(&key, user, make(entries), Some(audience.version), &jellyfin.ids) {
            Handled::Yes => return Ok(StatusCode::NO_CONTENT),
            Handled::Changed => continue,
            Handled::NotInGroup => {
                syncplay.tell(&key, NO_ID, "NotInGroup");
                return Err(JfError::Forbidden);
            }
            Handled::TooMany => return Err(JfError::Unavailable("too many SyncPlay requests: wait a moment".into())),
        }
    }
    Err(JfError::Unavailable("the group keeps changing: try again".into()))
}

/// `POST /SyncPlay/SetNewQueue {PlayingQueue, PlayingItemPosition,
/// StartPositionTicks}`: the group plays these titles, from the one at the
/// position, once everyone is ready.
async fn set_new_queue(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    let at = body.number("PlayingItemPosition").unwrap_or(0);
    let start = body.number("StartPositionTicks").unwrap_or(0);
    let length = body.get("PlayingQueue").and_then(Value::as_array).map_or(0, Vec::len);
    let at = usize::try_from(at)
        .ok()
        .filter(|at| *at < length)
        .ok_or_else(|| JfError::BadRequest("PlayingItemPosition must be one of PlayingQueue's".into()))?;
    queue_titles(&state, &jellyfin, &user, &client, &body, "PlayingQueue", false, |entries| Request::Play {
        entries,
        at,
        start,
    })
    .await
}

/// `POST /SyncPlay/Queue {ItemIds, Mode}`: adds titles at the end
/// (`Queue`) or after the one playing (`QueueNext`).
async fn queue(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    let next = match body.str("Mode").unwrap_or("Queue") {
        "Queue" => false,
        "QueueNext" => true,
        _ => return Err(JfError::BadRequest("Mode is Queue or QueueNext".into())),
    };
    queue_titles(&state, &jellyfin, &user, &client, &body, "ItemIds", true, |entries| Request::Queue { entries, next })
        .await
}

/// What a member says of its player (`Buffering`, `Ready`).
fn readiness(body: &Body) -> JfResult<Readiness> {
    let when = body.str("When").and_then(parse_iso).ok_or_else(|| JfError::BadRequest("When must be a time".into()))?;
    Ok(Readiness {
        when_ms: when,
        position_ticks: body.number("PositionTicks").unwrap_or(0),
        playing: body.flag("IsPlaying").unwrap_or(false),
        entry: guid_of(body, "PlaylistItemId")?,
    })
}

/// Handlers of requests to the caller's group, each made of its body.
macro_rules! requests {
    ($($(#[$doc:meta])* $name:ident => |$body:ident| $request:expr;)*) => {$(
        $(#[$doc])*
        async fn $name(
            State(state): State<AppState>,
            Extension(jellyfin): Extension<Arc<Jellyfin>>,
            JfUser { user, client }: JfUser,
            $body: Body,
        ) -> JfResult<StatusCode> {
            let request: Request = $request;
            send(&state, &jellyfin, &user, &client, request).await
        }
    )*};
}

requests! {
    /// `POST /SyncPlay/SetPlaylistItem {PlaylistItemId}`: plays that entry.
    set_playlist_item => |body| Request::SetPlaylistItem(guid_of(&body, "PlaylistItemId")?);
    /// `POST /SyncPlay/RemoveFromPlaylist {PlaylistItemIds, ClearPlaylist,
    /// ClearPlayingItem}`.
    remove_from_playlist => |body| Request::Remove {
        ids: strings(&body, "PlaylistItemIds", MAX_QUEUE)?.iter().filter_map(|id| guid(id)).collect(),
        clear: body.flag("ClearPlaylist").unwrap_or(false),
        with_playing: body.flag("ClearPlayingItem").unwrap_or(false),
    };
    /// `POST /SyncPlay/MovePlaylistItem {PlaylistItemId, NewIndex}`.
    move_playlist_item => |body| Request::Move {
        id: guid_of(&body, "PlaylistItemId")?,
        to: body.number("NewIndex").ok_or_else(|| JfError::BadRequest("NewIndex must be a number".into()))?,
    };
    /// `POST /SyncPlay/Unpause`.
    unpause => |_body| Request::Unpause;
    /// `POST /SyncPlay/Pause`.
    pause => |_body| Request::Pause;
    /// `POST /SyncPlay/Stop`.
    stop => |_body| Request::Stop;
    /// `POST /SyncPlay/Seek {PositionTicks}`.
    seek => |body| Request::Seek(
        body.number("PositionTicks").ok_or_else(|| JfError::BadRequest("PositionTicks must be a number".into()))?,
    );
    /// `POST /SyncPlay/Buffering {When, PositionTicks, IsPlaying, PlaylistItemId}`.
    buffering => |body| Request::Buffer(readiness(&body)?);
    /// `POST /SyncPlay/Ready {When, PositionTicks, IsPlaying, PlaylistItemId}`.
    ready => |body| Request::Ready(readiness(&body)?);
    /// `POST /SyncPlay/SetIgnoreWait {IgnoreWait}`: whether the group waits
    /// for this device.
    set_ignore_wait => |body| Request::IgnoreWait(body.flag("IgnoreWait").unwrap_or(false));
    /// `POST /SyncPlay/NextItem {PlaylistItemId}`: from the entry playing.
    next_item => |body| Request::Next(guid_of(&body, "PlaylistItemId")?);
    /// `POST /SyncPlay/PreviousItem {PlaylistItemId}`.
    previous_item => |body| Request::Previous(guid_of(&body, "PlaylistItemId")?);
    /// `POST /SyncPlay/SetRepeatMode {Mode}`.
    set_repeat_mode => |body| Request::Repeat(
        body.str("Mode").and_then(Repeat::of).ok_or_else(|| JfError::BadRequest("Mode is RepeatNone, RepeatOne or RepeatAll".into()))?,
    );
    /// `POST /SyncPlay/SetShuffleMode {Mode}`.
    set_shuffle_mode => |body| Request::Shuffle(match body.str("Mode") {
        Some("Shuffle") => true,
        Some("Sorted") => false,
        _ => return Err(JfError::BadRequest("Mode is Sorted or Shuffle".into())),
    });
}

/// `POST /SyncPlay/Ping {Ping}`: the device's ping (ms), which starts are
/// set ahead by. Nothing outside a group.
async fn ping(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    let key = key_of(&user, &client)?;
    let ping = body.number("Ping").ok_or_else(|| JfError::BadRequest("Ping must be a number".into()))?;
    match state.jellyfin_syncplay.handle(&key, &user, Request::Ping(ping), None, &jellyfin.ids) {
        Handled::TooMany => Err(JfError::Unavailable("too many SyncPlay requests: wait a moment".into())),
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(n: i64) -> Member {
        let user = AuthUser {
            id: n,
            username: format!("user{n}"),
            is_admin: false,
            account_admin: false,
            all_libraries: true,
            max_age: None,
            app: None,
            compatibility: None,
        };
        Member::new((n, format!("device-{n}")), user, format!("User {n}"))
    }

    /// The types of the messages `receiver` has.
    fn kinds(receiver: &mut mpsc::Receiver<Outgoing>) -> Vec<String> {
        std::iter::from_fn(|| receiver.try_recv().ok())
            .map(|(kind, data)| match kind {
                "SyncPlayCommand" => data["Command"].as_str().unwrap().to_owned(),
                _ => data["Type"].as_str().unwrap().to_owned(),
            })
            .collect()
    }

    #[tokio::test]
    async fn members_hear_their_group_through_their_sockets() {
        let syncplay = Arc::new(SyncPlay::default());
        let ids = Ids::new(&[7u8; 32]);
        let (mut first, _a) = syncplay.connect((1, "device-1".into()));
        let (mut second, _b) = syncplay.connect((2, "device-2".into()));
        let info = syncplay.create(member(1), "Movie night".into(), &ids).unwrap();
        assert_eq!(kinds(&mut first), ["GroupJoined", "Stop"]);
        let id = info["GroupId"].as_str().unwrap().to_owned();
        assert!(matches!(syncplay.join(member(2), &id, 0, &ids), Joined::Yes));
        assert_eq!(kinds(&mut first), ["UserJoined"]);
        assert_eq!(kinds(&mut second), ["GroupJoined", "Stop"]);
        assert!(matches!(syncplay.join(member(3), &id, 1, &ids), Joined::Changed), "another queue");
        assert!(matches!(syncplay.join(member(3), "x", 0, &ids), Joined::Missing));
        // Leaving: the last one's group goes.
        assert!(syncplay.leave(&(2, "device-2".into()), now_ms()));
        assert_eq!(
            (kinds(&mut first), kinds(&mut second)),
            (vec!["UserLeft".to_owned()], vec!["GroupLeft".to_owned()])
        );
        assert!(!syncplay.leave(&(2, "device-2".into()), now_ms()), "in none");
        assert!(syncplay.leave(&(1, "device-1".into()), now_ms()));
        assert_eq!(syncplay.state_of(&id), None, "an empty group goes");
    }

    #[tokio::test]
    async fn groups_and_members_are_bounded() {
        let syncplay = Arc::new(SyncPlay::default());
        let ids = Ids::new(&[7u8; 32]);
        for n in 0..MAX_GROUPS as i64 {
            assert!(syncplay.create(member(n), format!("g{n}"), &ids).is_some());
        }
        assert!(syncplay.create(member(1000), "one more".into(), &ids).is_none());
        let id = syncplay.snapshot(None)[0].0.clone();
        for n in 1..MAX_MEMBERS as i64 {
            assert!(matches!(syncplay.join(member(2000 + n), &id, 0, &ids), Joined::Yes));
        }
        assert!(matches!(syncplay.join(member(3000), &id, 0, &ids), Joined::Full));
    }

    #[tokio::test]
    async fn a_device_whose_socket_stays_closed_leaves() {
        let syncplay = Arc::new(SyncPlay::default());
        let ids = Ids::new(&[7u8; 32]);
        let key: Key = (1, "device-1".into());
        let info = syncplay.create(member(1), "g".into(), &ids).unwrap();
        let id = info["GroupId"].as_str().unwrap().to_owned();
        let soon = Duration::from_millis(50);
        // Back in time: still a member.
        syncplay.leave_unless_back(key.clone(), soon);
        let (_socket, slot) = syncplay.connect(key.clone());
        tokio::time::sleep(soon * 3).await;
        assert_eq!(syncplay.state_of(&id), Some(group::StateKind::Idle));
        // Gone for good: out, and the group with it.
        drop(slot);
        syncplay.leave_unless_back(key, soon);
        tokio::time::sleep(soon * 3).await;
        assert_eq!(syncplay.state_of(&id), None);
    }
}
