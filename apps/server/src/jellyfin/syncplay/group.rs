//! A SyncPlay group: its members, its play queue and its state, as
//! Jellyfin's server keeps them, and the messages each change sends.
//!
//! A group is idle (nothing plays), waiting (for its members to be ready:
//! after a new queue, a seek, someone buffering or joining), paused or
//! playing. Members say when they buffer and when they are ready, and
//! where their player is; once none buffers (those that asked not to be
//! waited for aside), the group plays or stays paused, as it was asked.
//! Its place is `position` (ticks) as of `last_activity` (unix ms): while
//! it plays it has moved on since, and a start is set a little ahead, for
//! every member's command to arrive in time. Commands say when (`When`) to
//! do what, by the server's clock, which members learn with `/GetUtcTime`.

use serde_json::{Value, json};

use crate::auth::AuthUser;
use crate::jellyfin::dto::TICKS_PER_MS;
use crate::jellyfin::ids::{Ids, Kind as IdKind};
use crate::jellyfin::users::iso_ms;
use crate::playback::CommandWindow;

/// A member: a device of a user, as its app's `DeviceId` names it.
pub type Key = (i64, String);

/// The ping a member is taken to have until it says (ms), and the least a
/// start is set ahead by.
const DEFAULT_PING_MS: i64 = 500;
/// The most a member's ping is taken to be (ms).
const MAX_PING_MS: i64 = 10_000;
/// How far a member's clock may be from the server's before the time it
/// says it was ready at is left out (ms).
const TIME_SYNC_OFFSET_MS: i64 = 2_000;
/// How far a member may be from the group's place and be ready (ms).
const MAX_PLAYBACK_OFFSET_MS: i64 = 500;
/// The furthest a place goes in a title whose length isn't known: a day.
const MAX_UNKNOWN_TICKS: i64 = 24 * 60 * 60 * 1000 * TICKS_PER_MS;
/// An id of nothing (no group, no queue entry), as Jellyfin writes an empty
/// GUID.
pub const NO_ID: &str = "00000000000000000000000000000000";

pub struct Member {
    pub key: Key,
    /// Who it is, for what it may see.
    pub user: AuthUser,
    /// Its name as "Who's watching?" says it, which the others are shown.
    pub name: String,
    ping_ms: i64,
    buffering: bool,
    ignore_wait: bool,
    /// Its requests lately, which [`Member::take_request`] counts.
    requests: CommandWindow,
}

impl Member {
    pub fn new(key: Key, user: AuthUser, name: String) -> Member {
        Member {
            key,
            user,
            name,
            ping_ms: DEFAULT_PING_MS,
            buffering: false,
            ignore_wait: false,
            requests: CommandWindow::default(),
        }
    }

    /// Counts a request: whether it may go, ten a second at most.
    pub fn take_request(&mut self) -> bool {
        self.requests.take()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    Idle,
    Waiting,
    Paused,
    Playing,
}

impl StateKind {
    pub fn name(self) -> &'static str {
        match self {
            StateKind::Idle => "Idle",
            StateKind::Waiting => "Waiting",
            StateKind::Paused => "Paused",
            StateKind::Playing => "Playing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Until every member is ready: then it plays (`resume`) or pauses.
    Waiting {
        resume: bool,
    },
    Paused,
    /// `ignore_buffering`: started for everyone without waiting for some,
    /// it doesn't wait for a member that buffers until it next changes.
    Playing {
        ignore_buffering: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    None,
    One,
    All,
}

impl Repeat {
    pub fn of(name: &str) -> Option<Repeat> {
        match name {
            "RepeatNone" => Some(Repeat::None),
            "RepeatOne" => Some(Repeat::One),
            "RepeatAll" => Some(Repeat::All),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Repeat::None => "RepeatNone",
            Repeat::One => "RepeatOne",
            Repeat::All => "RepeatAll",
        }
    }
}

/// One title of the queue: the item, its own id in the queue (a title can
/// be queued twice) and its length in ticks (0: not known).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub item: i64,
    pub id: String,
    pub ticks: i64,
}

impl Entry {
    /// `item`, `ticks` long, with an id of its own.
    pub fn new(item: i64, ticks: i64) -> Entry {
        Entry { item, id: new_id(), ticks }
    }
}

/// An id of a group or a queue entry: 128 random bits, as a GUID's 32
/// hexadecimal digits.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    hex::encode(bytes)
}

fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        items.swap(i, (u64::from_le_bytes(bytes) % (i as u64 + 1)) as usize);
    }
}

/// The group's play queue: its titles in order, and in the order they play
/// while it shuffles; which plays; how it repeats; when it last changed.
pub struct Queue {
    sorted: Vec<Entry>,
    shuffled: Option<Vec<Entry>>,
    playing: Option<usize>,
    repeat: Repeat,
    changed_ms: i64,
}

impl Queue {
    fn new(now: i64) -> Queue {
        Queue { sorted: Vec::new(), shuffled: None, playing: None, repeat: Repeat::None, changed_ms: now }
    }

    /// Its titles in the order they play.
    pub fn list(&self) -> &[Entry] {
        self.shuffled.as_deref().unwrap_or(&self.sorted)
    }

    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    fn playing_entry(&self) -> Option<&Entry> {
        self.playing.and_then(|i| self.list().get(i))
    }

    fn playing_id(&self) -> Option<&str> {
        self.playing_entry().map(|e| e.id.as_str())
    }

    fn touch(&mut self, now: i64) {
        self.changed_ms = now;
    }

    /// `playing` plays again, wherever it is now.
    fn repoint(&mut self, playing: Option<&str>) {
        self.playing = playing.and_then(|id| self.list().iter().position(|e| e.id == id));
    }

    /// A new queue, `entries` in order, from the one at `at`: not shuffled,
    /// not repeated, as Jellyfin starts one.
    fn set(&mut self, entries: Vec<Entry>, at: usize, now: i64) {
        self.sorted = entries;
        self.shuffled = None;
        self.repeat = Repeat::None;
        self.playing = (at < self.sorted.len()).then_some(at);
        self.touch(now);
    }

    /// Adds `entries` at the end, or after the one playing (`next`).
    fn add(&mut self, entries: Vec<Entry>, next: bool, now: i64) {
        let playing = self.playing_id().map(str::to_owned);
        let after = |list: &[Entry]| match (&playing, next) {
            (Some(id), true) => list.iter().position(|e| &e.id == id).map_or(list.len(), |i| i + 1),
            _ => list.len(),
        };
        let at = after(&self.sorted);
        self.sorted.splice(at..at, entries.iter().cloned());
        if let Some(shuffled) = &mut self.shuffled {
            let at = after(shuffled);
            shuffled.splice(at..at, entries);
        }
        self.repoint(playing.as_deref());
        self.touch(now);
    }

    /// Takes out the entries `ids` names: whether the one playing was one.
    /// The one before it plays then, or the first when it was first.
    fn remove(&mut self, ids: &[String], now: i64) -> bool {
        let playing = self.playing_id().map(str::to_owned);
        let before = self
            .playing
            .and_then(|at| self.list()[..at].iter().rev().find(|e| !ids.contains(&e.id)).map(|e| e.id.clone()));
        self.sorted.retain(|e| !ids.contains(&e.id));
        if let Some(shuffled) = &mut self.shuffled {
            shuffled.retain(|e| !ids.contains(&e.id));
        }
        self.touch(now);
        match playing {
            Some(id) if ids.contains(&id) => {
                self.repoint(before.as_deref());
                if self.playing.is_none() && self.len() > 0 {
                    self.playing = Some(0);
                }
                true
            }
            playing => {
                self.repoint(playing.as_deref());
                false
            }
        }
    }

    /// Empties it, but for the one playing unless `with_playing`.
    fn clear(&mut self, with_playing: bool, now: i64) {
        let kept = if with_playing { None } else { self.playing_entry().cloned() };
        self.sorted = kept.iter().cloned().collect();
        if self.shuffled.is_some() {
            self.shuffled = Some(self.sorted.clone());
        }
        self.playing = kept.map(|_| 0);
        self.touch(now);
    }

    /// Moves entry `id` to `to` (within the queue): whether it is one.
    fn move_to(&mut self, id: &str, to: i64, now: i64) -> bool {
        let playing = self.playing_id().map(str::to_owned);
        let list = self.shuffled.as_mut().unwrap_or(&mut self.sorted);
        let Some(from) = list.iter().position(|e| e.id == id) else { return false };
        let entry = list.remove(from);
        let to = usize::try_from(to.max(0)).unwrap_or(usize::MAX).min(list.len());
        list.insert(to, entry);
        self.repoint(playing.as_deref());
        self.touch(now);
        true
    }

    /// Plays entry `id`: whether it is one.
    fn select(&mut self, id: &str) -> bool {
        let Some(at) = self.list().iter().position(|e| e.id == id) else { return false };
        self.playing = Some(at);
        true
    }

    /// Plays the next one (the first after the last when it repeats them
    /// all, the same again when it repeats one): whether there is one.
    fn next(&mut self, now: i64) -> bool {
        let Some(at) = self.playing else { return false };
        let next = match self.repeat {
            Repeat::One => at,
            _ if at + 1 < self.len() => at + 1,
            Repeat::All => 0,
            Repeat::None => return false,
        };
        self.playing = Some(next);
        self.touch(now);
        true
    }

    /// Plays the one before, the same way.
    fn previous(&mut self, now: i64) -> bool {
        let Some(at) = self.playing else { return false };
        let previous = match self.repeat {
            Repeat::One => at,
            _ if at > 0 => at - 1,
            Repeat::All => self.len().saturating_sub(1),
            Repeat::None => return false,
        };
        self.playing = Some(previous);
        self.touch(now);
        true
    }

    /// Shuffles it, the one playing first, or puts it back in order.
    fn set_shuffle(&mut self, shuffled: bool, now: i64) {
        let playing = self.playing_id().map(str::to_owned);
        self.shuffled = shuffled.then(|| {
            let mut order = self.sorted.clone();
            shuffle(&mut order);
            if let Some(at) = playing.as_ref().and_then(|id| order.iter().position(|e| &e.id == id)) {
                let entry = order.remove(at);
                order.insert(0, entry);
            }
            order
        });
        self.repoint(playing.as_deref());
        self.touch(now);
    }
}

/// What a member says of its player when it buffers or is ready.
#[derive(Debug, Clone)]
pub struct Readiness {
    /// When it was so, by the server's clock as the member knows it (ms).
    pub when_ms: i64,
    pub position_ticks: i64,
    pub playing: bool,
    /// The queue entry it plays.
    pub entry: String,
}

/// What a member asks of its group.
#[derive(Debug, Clone)]
pub enum Request {
    /// A new queue, from the entry at `at`, from `start` (ticks).
    Play {
        entries: Vec<Entry>,
        at: usize,
        start: i64,
    },
    SetPlaylistItem(String),
    Remove {
        ids: Vec<String>,
        clear: bool,
        with_playing: bool,
    },
    Move {
        id: String,
        to: i64,
    },
    Queue {
        entries: Vec<Entry>,
        next: bool,
    },
    Unpause,
    Pause,
    Stop,
    Seek(i64),
    Buffer(Readiness),
    Ready(Readiness),
    /// The next or the previous entry, from the one named: the one playing,
    /// so that two members asking at once move it on once.
    Next(String),
    Previous(String),
    Repeat(Repeat),
    Shuffle(bool),
    Ping(i64),
    IgnoreWait(bool),
}

impl Request {
    /// Its name as Jellyfin's state updates give their reason.
    fn reason(&self) -> &'static str {
        match self {
            Request::Play { .. } => "Play",
            Request::SetPlaylistItem(_) => "SetPlaylistItem",
            Request::Remove { .. } => "RemoveFromPlaylist",
            Request::Move { .. } => "MovePlaylistItem",
            Request::Queue { .. } => "Queue",
            Request::Unpause => "Unpause",
            Request::Pause => "Pause",
            Request::Stop => "Stop",
            Request::Seek(_) => "Seek",
            Request::Buffer(_) => "Buffer",
            Request::Ready(_) => "Ready",
            Request::Next(_) => "NextItem",
            Request::Previous(_) => "PreviousItem",
            Request::Repeat(_) => "SetRepeatMode",
            Request::Shuffle(_) => "SetShuffleMode",
            Request::Ping(_) => "Ping",
            Request::IgnoreWait(_) => "IgnoreWait",
        }
    }
}

/// Whom a message goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    Member(Key),
    All,
    AllBut(Key),
    /// The members that aren't buffering.
    Ready,
}

/// A message for members' sockets: its type and its data.
#[derive(Debug, Clone)]
pub struct Out {
    pub to: To,
    pub kind: &'static str,
    pub data: Value,
}

pub struct Group {
    pub id: String,
    pub name: String,
    pub members: Vec<Member>,
    pub queue: Queue,
    state: State,
    /// The place (ticks) as of `last_activity` (unix ms), which is ahead of
    /// now while a start waits for its commands to arrive.
    position: i64,
    last_activity: i64,
    /// Counted up as members join and as the queue's titles change: an
    /// access check made before is made again.
    pub members_version: u64,
    pub queue_version: u64,
}

impl Group {
    pub fn new(id: String, name: String, now: i64) -> Group {
        Group {
            id,
            name,
            members: Vec::new(),
            queue: Queue::new(now),
            state: State::Idle,
            position: 0,
            last_activity: now,
            members_version: 0,
            queue_version: 0,
        }
    }

    pub fn kind(&self) -> StateKind {
        match self.state {
            State::Idle => StateKind::Idle,
            State::Waiting { .. } => StateKind::Waiting,
            State::Paused => StateKind::Paused,
            State::Playing { .. } => StateKind::Playing,
        }
    }

    pub fn member_mut(&mut self, key: &Key) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| &m.key == key)
    }

    /// The items of its queue, for what a member may see.
    pub fn items(&self) -> Vec<i64> {
        self.queue.sorted.iter().map(|e| e.item).collect()
    }

    /// Who `to` is among its members.
    pub fn recipients(&self, to: &To) -> Vec<Key> {
        let keys =
            |keep: &dyn Fn(&Member) -> bool| self.members.iter().filter(|m| keep(m)).map(|m| m.key.clone()).collect();
        match to {
            To::Member(key) => vec![key.clone()],
            To::All => keys(&|_| true),
            To::AllBut(key) => keys(&|m| &m.key != key),
            To::Ready => keys(&|m| !m.buffering),
        }
    }

    /// Jellyfin's `GroupInfoDto`, for `key`: its members by name, each once,
    /// for one of them; none for anyone else, as this API shows no one
    /// another account's name.
    pub fn info(&self, now: i64, key: Option<&Key>) -> Value {
        let mut names: Vec<&str> = Vec::new();
        if key.is_some_and(|key| self.members.iter().any(|m| &m.key == key)) {
            for member in &self.members {
                if !names.contains(&member.name.as_str()) {
                    names.push(&member.name);
                }
            }
        }
        json!({
            "GroupId": self.id, "GroupName": self.name, "State": self.kind().name(),
            "Participants": names, "LastUpdatedAt": iso_ms(now),
        })
    }

    fn update(&self, to: To, kind: &str, data: Value) -> Out {
        Out { to, kind: "SyncPlayGroupUpdate", data: json!({"GroupId": self.id, "Type": kind, "Data": data}) }
    }

    /// A command: what to do, from `position`, at `when` (unix ms).
    fn command_at(&self, to: To, command: &str, when: i64, now: i64) -> Out {
        let data = json!({
            "GroupId": self.id, "PlaylistItemId": self.queue.playing_id().unwrap_or(NO_ID),
            "When": iso_ms(when), "PositionTicks": self.position, "Command": command, "EmittedAt": iso_ms(now),
        });
        Out { to, kind: "SyncPlayCommand", data }
    }

    fn command(&self, to: To, command: &str, now: i64) -> Out {
        self.command_at(to, command, self.last_activity, now)
    }

    fn state_update(&self, reason: &str) -> Out {
        self.update(To::All, "StateUpdate", json!({"State": self.kind().name(), "Reason": reason}))
    }

    /// Jellyfin's `PlayQueueUpdate`, from where it is now.
    fn queue_update(&self, to: To, reason: &str, now: i64, ids: &Ids) -> Out {
        let playing = self.kind() == StateKind::Playing;
        let list: Vec<Value> = self
            .queue
            .list()
            .iter()
            .map(|e| json!({"ItemId": ids.of(IdKind::Item, e.item), "PlaylistItemId": e.id}))
            .collect();
        let data = json!({
            "Reason": reason, "LastUpdate": iso_ms(self.queue.changed_ms), "Playlist": list,
            "PlayingItemIndex": self.queue.playing.map_or(-1, |i| i as i64),
            "StartPositionTicks": if playing { self.position_at(now) } else { self.position },
            "IsPlaying": playing, "ShuffleMode": if self.queue.shuffled.is_some() { "Shuffle" } else { "Sorted" },
            "RepeatMode": self.queue.repeat.name(),
        });
        self.update(to, "PlayQueue", data)
    }

    /// Where it is at `now` while it plays: moved on since its last
    /// activity, unless that is still ahead (a start that waits).
    fn position_at(&self, now: i64) -> i64 {
        self.position.saturating_add((now - self.last_activity).max(0).saturating_mul(TICKS_PER_MS))
    }

    /// Holds the place where it is now, for a pause or a wait.
    fn hold(&mut self, now: i64) {
        self.position = self.position_at(now);
        self.last_activity = now;
    }

    /// `ticks` within the title playing.
    fn bounded(&self, ticks: i64) -> i64 {
        let length = self.queue.playing_entry().map_or(0, |e| e.ticks);
        ticks.clamp(0, if length > 0 { length } else { MAX_UNKNOWN_TICKS })
    }

    /// The title playing, from its start.
    fn restart(&mut self, now: i64) {
        self.position = 0;
        self.last_activity = now;
    }

    /// Whether it waits for someone.
    fn buffering(&self) -> bool {
        self.members.iter().any(|m| m.buffering && !m.ignore_wait)
    }

    fn set_all_buffering(&mut self, buffering: bool) {
        for member in &mut self.members {
            member.buffering = buffering;
        }
    }

    fn set_buffering(&mut self, key: &Key, buffering: bool) {
        if let Some(member) = self.member_mut(key) {
            member.buffering = buffering;
        }
    }

    fn highest_ping(&self) -> i64 {
        self.members.iter().map(|m| m.ping_ms).max().unwrap_or(0)
    }

    /// Whether it waits to play once its members are ready.
    fn resumes(&self) -> bool {
        matches!(self.state, State::Waiting { resume: true })
    }

    /// To idle: everyone stops, or the one asking when it was idle already.
    fn stop(&mut self, key: &Key, now: i64, out: &mut Vec<Out>) {
        let to = if self.state == State::Idle { To::Member(key.clone()) } else { To::All };
        self.state = State::Idle;
        out.push(self.command(to, "Stop", now));
    }

    /// Plays for everyone, a little ahead for every command to arrive.
    fn start(&mut self, ignore_buffering: bool, reason: &str, now: i64, out: &mut Vec<Out>) {
        let delay = (self.highest_ping() * 2).max(DEFAULT_PING_MS);
        self.last_activity = now + delay;
        self.state = State::Playing { ignore_buffering };
        out.push(self.command(To::All, "Unpause", now));
        out.push(self.state_update(reason));
    }

    /// Waits no longer for anyone: it plays or pauses, as it was asked.
    fn waited(&mut self, reason: &str, now: i64, out: &mut Vec<Out>) {
        if self.resumes() {
            self.start(false, reason, now, out);
        } else {
            self.state = State::Paused;
            out.push(self.command(To::All, "Pause", now));
            out.push(self.state_update(reason));
        }
    }

    /// A new member: told of the group, the others told of it, then caught
    /// up with what plays.
    pub fn join(&mut self, member: Member, now: i64, ids: &Ids) -> Vec<Out> {
        let (key, name) = (member.key.clone(), member.name.clone());
        self.members.push(member);
        self.members_version += 1;
        let mut out = vec![self.update(To::Member(key.clone()), "GroupJoined", self.info(now, Some(&key)))];
        out.push(self.update(To::AllBut(key.clone()), "UserJoined", json!(name)));
        self.catch_up(&key, now, ids, &mut out);
        out
    }

    /// A member that joins again (an app that closed its player and opens
    /// it): told of the group anew, and caught up as a new member is.
    pub fn rejoin(&mut self, key: &Key, now: i64, ids: &Ids) -> Vec<Out> {
        let mut out = vec![self.update(To::Member(key.clone()), "GroupJoined", self.info(now, Some(key)))];
        self.catch_up(key, now, ids, &mut out);
        out
    }

    /// A member that joined: told to stop while nothing plays; otherwise the
    /// group waits for it to load the queue, the others paused meanwhile.
    fn catch_up(&mut self, key: &Key, now: i64, ids: &Ids, out: &mut Vec<Out>) {
        match self.state {
            State::Idle => {
                out.push(self.command(To::Member(key.clone()), "Stop", now));
                return;
            }
            State::Playing { .. } => {
                self.hold(now);
                self.state = State::Waiting { resume: true };
            }
            State::Paused => self.state = State::Waiting { resume: false },
            State::Waiting { .. } => {}
        }
        out.push(self.queue_update(To::Member(key.clone()), "NewPlaylist", now, ids));
        self.set_buffering(key, true);
        out.push(self.command(To::Ready, "Pause", now));
    }

    /// Member `key` leaves: told so, the others too. A group that waited
    /// for it alone goes on. `None` when it isn't one.
    pub fn leave(&mut self, key: &Key, now: i64) -> Option<Vec<Out>> {
        let at = self.members.iter().position(|m| &m.key == key)?;
        let member = self.members.remove(at);
        let mut out = vec![self.update(To::Member(key.clone()), "GroupLeft", json!(self.id))];
        out.push(self.update(To::All, "UserLeft", json!(member.name)));
        if matches!(self.state, State::Waiting { .. }) && !self.members.is_empty() && !self.buffering() {
            let reason = if self.resumes() { "Unpause" } else { "Pause" };
            self.waited(reason, now, &mut out);
        }
        Some(out)
    }

    /// What member `key`'s request does to the group, and the messages it
    /// sends.
    pub fn handle(&mut self, key: &Key, request: Request, now: i64, ids: &Ids) -> Vec<Out> {
        let mut out = Vec::new();
        let reason = request.reason();
        let current = self.queue.playing_id().map(str::to_owned);
        let me = || To::Member(key.clone());
        match request {
            Request::Ping(ms) => {
                if let Some(member) = self.member_mut(key) {
                    member.ping_ms = ms.clamp(0, MAX_PING_MS);
                }
            }
            Request::IgnoreWait(ignore) => {
                if let Some(member) = self.member_mut(key) {
                    member.ignore_wait = ignore;
                }
                if matches!(self.state, State::Waiting { .. }) && !self.buffering() {
                    self.waited(reason, now, &mut out);
                }
            }
            Request::Repeat(mode) => {
                self.queue.repeat = mode;
                self.queue.touch(now);
                out.push(self.queue_update(To::All, "RepeatMode", now, ids));
            }
            Request::Shuffle(shuffled) => {
                self.queue.set_shuffle(shuffled, now);
                out.push(self.queue_update(To::All, "ShuffleMode", now, ids));
            }
            Request::Move { id, to } => {
                if self.queue.move_to(&id, to, now) {
                    out.push(self.queue_update(To::All, "MoveItem", now, ids));
                }
            }
            Request::Queue { entries, next } => {
                if !entries.is_empty() {
                    self.queue.add(entries, next, now);
                    self.queue_version += 1;
                    out.push(self.queue_update(To::All, if next { "QueueNext" } else { "Queue" }, now, ids));
                }
            }
            Request::Remove { ids: removed, clear, with_playing } => {
                let playing_removed = if clear {
                    self.queue.clear(with_playing, now);
                    with_playing && current.is_some()
                } else {
                    self.queue.remove(&removed, now)
                };
                self.queue_version += 1;
                if playing_removed {
                    self.restart(now);
                }
                out.push(self.queue_update(To::All, "RemoveItems", now, ids));
                if playing_removed && self.queue.playing.is_none() {
                    self.stop(key, now, &mut out);
                }
            }
            Request::Play { entries, at, start } => {
                if at < entries.len() {
                    self.state = State::Waiting { resume: true };
                    self.queue.set(entries, at, now);
                    self.queue_version += 1;
                    self.position = self.bounded(start);
                    self.last_activity = now;
                    out.push(self.queue_update(To::All, "NewPlaylist", now, ids));
                    self.set_all_buffering(true);
                }
            }
            Request::SetPlaylistItem(id) => {
                if self.queue.select(&id) {
                    self.state = State::Waiting { resume: true };
                    self.restart(now);
                    out.push(self.queue_update(To::All, "SetCurrentItem", now, ids));
                    self.set_all_buffering(true);
                }
            }
            Request::Next(id) | Request::Previous(id) if current.as_deref() == Some(id.as_str()) => {
                let next = reason == "NextItem";
                let moved = if next { self.queue.next(now) } else { self.queue.previous(now) };
                if moved {
                    self.state = State::Waiting { resume: true };
                    self.restart(now);
                    out.push(self.queue_update(To::All, if next { "NextItem" } else { "PreviousItem" }, now, ids));
                    self.set_all_buffering(true);
                }
            }
            // From another entry than the one playing: someone moved on first.
            Request::Next(_) | Request::Previous(_) => {}
            Request::Stop => self.stop(key, now, &mut out),
            // Nothing plays: the member stops too.
            Request::Pause | Request::Seek(_) | Request::Buffer(_) | Request::Ready(_) if self.state == State::Idle => {
                out.push(self.command(me(), "Stop", now));
            }
            Request::Unpause => match self.state {
                State::Idle if self.queue.playing.is_some() => {
                    self.state = State::Waiting { resume: true };
                    self.restart(now);
                    out.push(self.queue_update(To::All, "NewPlaylist", now, ids));
                    self.set_all_buffering(true);
                }
                // Nothing to play: an app picks an entry first.
                State::Idle => {}
                // Asked while it waits to play: it starts for everyone,
                // without waiting for those not ready.
                State::Waiting { resume: true } => {
                    self.set_all_buffering(false);
                    self.start(true, reason, now, &mut out);
                }
                State::Waiting { resume: false } => {
                    self.state = State::Waiting { resume: true };
                    out.push(self.state_update(reason));
                }
                State::Paused => self.start(false, reason, now, &mut out),
                // It plays: the member is told so again.
                State::Playing { .. } => out.push(self.command(me(), "Unpause", now)),
            },
            Request::Pause => match self.state {
                State::Waiting { .. } => {
                    self.state = State::Waiting { resume: false };
                    out.push(self.state_update(reason));
                }
                State::Playing { .. } => {
                    self.hold(now);
                    self.state = State::Paused;
                    out.push(self.command(To::All, "Pause", now));
                    out.push(self.state_update(reason));
                }
                _ => out.push(self.command(me(), "Pause", now)),
            },
            Request::Seek(ticks) => {
                let resume = match self.state {
                    State::Playing { .. } => true,
                    State::Waiting { resume } => resume,
                    _ => false,
                };
                self.state = State::Waiting { resume };
                self.position = self.bounded(ticks);
                self.last_activity = now;
                out.push(self.command(To::All, "Seek", now));
                self.set_all_buffering(true);
                out.push(self.state_update(reason));
            }
            // It plays another entry than the group's: it is sent the
            // group's, and waited for while the group waits.
            Request::Buffer(said) | Request::Ready(said) if current.as_deref() != Some(said.entry.as_str()) => {
                out.push(self.queue_update(me(), "SetCurrentItem", now, ids));
                if matches!(self.state, State::Waiting { .. }) {
                    self.set_buffering(key, true);
                }
            }
            Request::Buffer(_) => match self.state {
                // Started without waiting: it doesn't wait now either.
                State::Playing { ignore_buffering: true } => {}
                State::Playing { .. } => {
                    self.hold(now);
                    self.state = State::Waiting { resume: true };
                    self.set_buffering(key, true);
                    out.push(self.command(To::Ready, "Pause", now));
                    out.push(self.state_update(reason));
                }
                State::Paused => {
                    self.state = State::Waiting { resume: false };
                    self.set_buffering(key, true);
                    out.push(self.command(me(), "Pause", now));
                    out.push(self.state_update(reason));
                }
                _ => {
                    self.set_buffering(key, true);
                    if !self.resumes() {
                        out.push(self.command(me(), "Pause", now));
                    }
                    out.push(self.state_update(reason));
                }
            },
            Request::Ready(said) => match self.state {
                State::Playing { .. } => out.push(self.command(me(), "Unpause", now)),
                State::Paused => out.push(self.command(me(), "Pause", now)),
                _ => self.ready(key, &said, reason, now, &mut out),
            },
        }
        out
    }

    /// A member ready while the group waits: where it says it is (played on
    /// since it said so) against where the group is.
    fn ready(&mut self, key: &Key, said: &Readiness, reason: &str, now: i64, out: &mut Vec<Out>) {
        let me = To::Member(key.clone());
        // From a clock too far from the server's, the time it took says
        // nothing.
        let mut elapsed = now - said.when_ms;
        if elapsed.abs() > TIME_SYNC_OFFSET_MS || !said.playing {
            elapsed = 0;
        }
        let reported = self.bounded(said.position_ticks);
        let behind = self.position - reported.saturating_add(elapsed.saturating_mul(TICKS_PER_MS));
        let tolerance = MAX_PLAYBACK_OFFSET_MS * TICKS_PER_MS;
        let lost = if self.resumes() {
            !said.playing && behind.abs() > tolerance
        } else {
            (self.position - reported).abs() > tolerance
        };
        if lost {
            // Not where the group is: sent there, and waited for.
            self.set_buffering(key, true);
            out.push(self.command(me, "Seek", now));
            out.push(self.state_update(reason));
            return;
        }
        self.set_buffering(key, false);
        if !self.resumes() {
            if !self.buffering() {
                self.waited(reason, now, out);
            }
            return;
        }
        if self.buffering() {
            // Others aren't ready: it pauses where the group is.
            out.push(self.command_at(me, "Pause", now + behind / TICKS_PER_MS, now));
            return;
        }
        if behind > self.highest_ping() * 2 * TICKS_PER_MS {
            // It is behind: the others start as it catches up.
            self.last_activity = now + behind / TICKS_PER_MS;
            self.state = State::Playing { ignore_buffering: false };
            let to = if said.playing { To::AllBut(key.clone()) } else { To::All };
            out.push(self.command(to, "Unpause", now));
            out.push(self.state_update(reason));
        } else {
            self.start(false, reason, now, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn ids() -> Ids {
        Ids::new(&[7u8; 32])
    }

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
        Member::new(key(n), user, format!("User {n}"))
    }

    fn key(n: i64) -> Key {
        (n, format!("device-{n}"))
    }

    /// A group of `n` members, idle.
    fn group_of(n: i64) -> Group {
        let mut group = Group::new("g".into(), "Movie night".into(), NOW);
        for i in 1..=n {
            group.join(member(i), NOW, &ids());
        }
        group
    }

    /// The type and the command or update type of each message, with whom
    /// it goes to.
    fn said(out: &[Out]) -> Vec<(To, String)> {
        out.iter()
            .map(|o| {
                let what = if o.kind == "SyncPlayCommand" { &o.data["Command"] } else { &o.data["Type"] };
                (o.to.clone(), what.as_str().unwrap().to_owned())
            })
            .collect()
    }

    fn entries(n: i64) -> Vec<Entry> {
        (1..=n).map(|i| Entry { item: i, id: format!("e{i}"), ticks: 100_000 * TICKS_PER_MS }).collect()
    }

    fn ready(entry: &str, position_ticks: i64, playing: bool, when_ms: i64) -> Readiness {
        Readiness { when_ms, position_ticks, playing, entry: entry.into() }
    }

    #[test]
    fn a_member_joins_an_idle_group_stopped() {
        let mut group = Group::new("g".into(), "Movie night".into(), NOW);
        let out = group.join(member(1), NOW, &ids());
        assert_eq!(
            said(&out),
            [
                (To::Member(key(1)), "GroupJoined".into()),
                (To::AllBut(key(1)), "UserJoined".into()),
                (To::Member(key(1)), "Stop".into())
            ]
        );
        assert_eq!(out[0].data["Data"]["Participants"], json!(["User 1"]), "to a member");
        assert_eq!(group.info(NOW, Some(&key(2)))["Participants"], json!([]), "to anyone else");
        assert_eq!(group.info(NOW, None)["Participants"], json!([]));
        assert_eq!(out[0].data["Data"]["State"], json!("Idle"));
        assert_eq!(out[2].data["PlaylistItemId"], json!(NO_ID), "nothing plays");
    }

    #[test]
    fn a_new_queue_plays_once_everyone_is_ready() {
        let mut group = group_of(2);
        let out = group.handle(&key(1), Request::Play { entries: entries(2), at: 1, start: 0 }, NOW, &ids());
        assert_eq!(said(&out), [(To::All, "PlayQueue".into())]);
        let queue = &out[0].data["Data"];
        assert_eq!((&queue["Reason"], &queue["PlayingItemIndex"]), (&json!("NewPlaylist"), &json!(1)));
        assert_eq!(queue["Playlist"][1], json!({"ItemId": ids().of(IdKind::Item, 2), "PlaylistItemId": "e2"}));
        assert_eq!(group.kind(), StateKind::Waiting);

        // The first ready waits for the second, paused where the group is.
        let out = group.handle(&key(1), Request::Ready(ready("e2", 0, false, NOW)), NOW, &ids());
        assert_eq!(said(&out), [(To::Member(key(1)), "Pause".into())]);
        let out = group.handle(&key(2), Request::Ready(ready("e2", 0, false, NOW)), NOW + 10, &ids());
        assert_eq!(said(&out), [(To::All, "Unpause".into()), (To::All, "StateUpdate".into())]);
        assert_eq!(out[0].data["When"], json!(iso_ms(NOW + 10 + DEFAULT_PING_MS * 2)), "ahead by twice the ping");
        assert_eq!(out[1].data["Data"], json!({"State": "Playing", "Reason": "Ready"}));
        assert_eq!(group.kind(), StateKind::Playing);
    }

    /// A group of two playing the second of two entries from NOW.
    fn playing() -> Group {
        let mut group = group_of(2);
        group.handle(&key(1), Request::Play { entries: entries(2), at: 1, start: 0 }, NOW, &ids());
        for i in 1..=2 {
            group.handle(&key(i), Request::Ready(ready("e2", 0, false, NOW)), NOW, &ids());
        }
        group.last_activity = NOW;
        group
    }

    #[test]
    fn a_member_that_buffers_holds_the_others() {
        let mut group = playing();
        let out = group.handle(&key(2), Request::Buffer(ready("e2", 0, false, NOW)), NOW + 3_000, &ids());
        assert_eq!(said(&out), [(To::Ready, "Pause".into()), (To::All, "StateUpdate".into())]);
        assert_eq!(group.recipients(&To::Ready), [key(1)], "the others");
        assert_eq!(out[0].data["PositionTicks"], json!(3_000 * TICKS_PER_MS), "where the group got to");
        // Ready again where the group is: everyone plays on.
        let out =
            group.handle(&key(2), Request::Ready(ready("e2", 3_000 * TICKS_PER_MS, false, NOW)), NOW + 4_000, &ids());
        assert_eq!(said(&out), [(To::All, "Unpause".into()), (To::All, "StateUpdate".into())]);
        // Ready somewhere else: sent where the group is, and waited for.
        let mut group = playing();
        group.handle(&key(2), Request::Buffer(ready("e2", 0, false, NOW)), NOW + 3_000, &ids());
        let out = group.handle(&key(2), Request::Ready(ready("e2", 0, false, NOW)), NOW + 4_000, &ids());
        assert_eq!(said(&out), [(To::Member(key(2)), "Seek".into()), (To::All, "StateUpdate".into())]);
        assert_eq!(group.kind(), StateKind::Waiting);
    }

    #[test]
    fn a_pause_and_a_seek_wait_paused() {
        let mut group = playing();
        let out = group.handle(&key(1), Request::Pause, NOW + 2_000, &ids());
        assert_eq!(said(&out), [(To::All, "Pause".into()), (To::All, "StateUpdate".into())]);
        assert_eq!(out[0].data["PositionTicks"], json!(2_000 * TICKS_PER_MS));
        let out = group.handle(&key(2), Request::Seek(50_000 * TICKS_PER_MS), NOW + 3_000, &ids());
        assert_eq!(said(&out), [(To::All, "Seek".into()), (To::All, "StateUpdate".into())]);
        for i in 1..=2 {
            let out = group.handle(
                &key(i),
                Request::Ready(ready("e2", 50_000 * TICKS_PER_MS, false, NOW)),
                NOW + 4_000,
                &ids(),
            );
            if i == 2 {
                assert_eq!(said(&out), [(To::All, "Pause".into()), (To::All, "StateUpdate".into())], "it stays paused");
            }
        }
        assert_eq!(group.kind(), StateKind::Paused);
        // Past the end of the title: at its end.
        group.handle(&key(1), Request::Seek(i64::MAX), NOW, &ids());
        assert_eq!(group.position, 100_000 * TICKS_PER_MS);
    }

    #[test]
    fn a_member_that_joins_is_waited_for() {
        let mut group = playing();
        let out = group.join(member(3), NOW + 5_000, &ids());
        let to_it = To::Member(key(3));
        assert_eq!(
            said(&out),
            [
                (to_it.clone(), "GroupJoined".into()),
                (To::AllBut(key(3)), "UserJoined".into()),
                (to_it, "PlayQueue".into()),
                (To::Ready, "Pause".into())
            ]
        );
        assert_eq!(out[2].data["Data"]["StartPositionTicks"], json!(5_000 * TICKS_PER_MS));
        assert_eq!(group.recipients(&To::Ready), [key(1), key(2)]);
        // It leaves before it is ready: the others go on.
        let out = group.leave(&key(3), NOW + 6_000).unwrap();
        assert_eq!(
            said(&out),
            [
                (To::Member(key(3)), "GroupLeft".into()),
                (To::All, "UserLeft".into()),
                (To::All, "Unpause".into()),
                (To::All, "StateUpdate".into())
            ]
        );
        assert_eq!(group.recipients(&To::All), [key(1), key(2)]);
        assert!(group.leave(&key(3), NOW).is_none(), "no longer a member");
    }

    #[test]
    fn a_member_not_waited_for_holds_no_one() {
        let mut group = group_of(2);
        group.handle(&key(2), Request::IgnoreWait(true), NOW, &ids());
        group.handle(&key(1), Request::Play { entries: entries(1), at: 0, start: 0 }, NOW, &ids());
        let out = group.handle(&key(1), Request::Ready(ready("e1", 0, false, NOW)), NOW, &ids());
        assert_eq!(said(&out)[0], (To::All, "Unpause".into()), "the other isn't waited for");
        // Unpause while it waits starts it for everyone, at once.
        let mut group = group_of(2);
        group.handle(&key(1), Request::Play { entries: entries(1), at: 0, start: 0 }, NOW, &ids());
        let out = group.handle(&key(1), Request::Unpause, NOW, &ids());
        assert_eq!(said(&out)[0], (To::All, "Unpause".into()));
        let out = group.handle(&key(2), Request::Buffer(ready("e1", 0, false, NOW)), NOW, &ids());
        assert!(out.is_empty(), "nor does it wait for buffering now");
    }

    #[test]
    fn a_member_on_another_entry_is_sent_the_groups() {
        let mut group = group_of(1);
        group.handle(&key(1), Request::Play { entries: entries(2), at: 0, start: 0 }, NOW, &ids());
        let out = group.handle(&key(1), Request::Ready(ready("e2", 0, false, NOW)), NOW, &ids());
        assert_eq!(said(&out), [(To::Member(key(1)), "PlayQueue".into())]);
        assert_eq!(out[0].data["Data"]["Reason"], json!("SetCurrentItem"));
        assert_eq!(group.kind(), StateKind::Waiting);
    }

    #[test]
    fn idle_groups_stop_whoever_asks() {
        let mut group = group_of(1);
        for request in [Request::Pause, Request::Seek(0), Request::Ready(ready(NO_ID, 0, false, NOW)), Request::Stop] {
            assert_eq!(said(&group.handle(&key(1), request, NOW, &ids())), [(To::Member(key(1)), "Stop".into())]);
        }
        assert!(group.handle(&key(1), Request::Unpause, NOW, &ids()).is_empty(), "nothing to play");
        // Stopped while it plays: everyone stops; Unpause plays it again.
        let mut group = playing();
        assert_eq!(said(&group.handle(&key(1), Request::Stop, NOW, &ids())), [(To::All, "Stop".into())]);
        let out = group.handle(&key(1), Request::Unpause, NOW, &ids());
        assert_eq!(said(&out), [(To::All, "PlayQueue".into())]);
        assert_eq!(out[0].data["Data"]["StartPositionTicks"], json!(0), "from its start");
    }

    #[test]
    fn next_and_previous_move_once_and_repeat() {
        let mut group = playing();
        assert!(group.handle(&key(1), Request::Next("e2".into()), NOW, &ids()).is_empty(), "the last one");
        group.handle(&key(1), Request::Repeat(Repeat::All), NOW, &ids());
        let out = group.handle(&key(1), Request::Next("e2".into()), NOW, &ids());
        assert_eq!(out[0].data["Data"]["PlayingItemIndex"], json!(0), "round to the first");
        assert!(group.handle(&key(2), Request::Next("e2".into()), NOW, &ids()).is_empty(), "moved on already");
        group.handle(&key(1), Request::Repeat(Repeat::One), NOW, &ids());
        let out = group.handle(&key(1), Request::Previous("e1".into()), NOW, &ids());
        assert_eq!(out[0].data["Data"]["PlayingItemIndex"], json!(0), "the same again");
    }

    #[test]
    fn the_queue_is_edited_by_its_entries() {
        let now = NOW;
        let mut queue = Queue::new(now);
        queue.set(entries(4), 2, now);
        queue.add(vec![Entry { item: 9, id: "n".into(), ticks: 0 }], true, now);
        let order = |q: &Queue| q.list().iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        assert_eq!(order(&queue), ["e1", "e2", "e3", "n", "e4"], "next: after the one playing");
        assert!(queue.move_to("e1", 10, now));
        assert_eq!(order(&queue), ["e2", "e3", "n", "e4", "e1"]);
        assert_eq!(queue.playing_id(), Some("e3"), "the one playing stays");
        assert!(!queue.remove(&["e4".into()], now));
        assert!(queue.remove(&["e3".into(), "e2".into()], now), "the one playing");
        assert_eq!(
            (order(&queue), queue.playing_id()),
            (vec!["n".to_owned(), "e1".to_owned()], Some("n")),
            "the first, none before it"
        );
        queue.set_shuffle(true, now);
        assert_eq!(queue.list()[0].id, "n", "the one playing first");
        queue.set_shuffle(false, now);
        assert_eq!(order(&queue), ["n", "e1"]);
        queue.clear(false, now);
        assert_eq!((order(&queue), queue.playing_id()), (vec!["n".to_owned()], Some("n")), "but for the one playing");
        queue.clear(true, now);
        assert_eq!((queue.len(), queue.playing_id()), (0, None));
    }

    #[test]
    fn removing_what_plays_stops_an_empty_group() {
        let mut group = playing();
        let out = group.handle(&key(1), Request::Remove { ids: vec![], clear: true, with_playing: true }, NOW, &ids());
        assert_eq!(said(&out), [(To::All, "PlayQueue".into()), (To::All, "Stop".into())]);
        assert_eq!(group.kind(), StateKind::Idle);
    }
}
