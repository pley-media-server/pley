//! The connections to IPTV providers: a *tuner* pulls one channel, once,
//! however many watch it, and hands what comes to each of them.
//!
//! - A source plays no more channels at once than its provider allows
//!   (`max_connections`). A channel nobody watches any more is let go after
//!   [`LINGER`], or at once when another channel needs its connection.
//! - Players watch through a *viewer*: an HLS one gets the channel from the
//!   transcoder (one live job per tuner, which every HLS viewer of it reads),
//!   a stream one the provider's MPEG-TS as it comes, from its last video
//!   keyframe. Starting to watch on a device (`client_id`, and the
//!   `<client_id>-stream` its stream reader goes by) ends what that device
//!   watched before, so changing channels on a one-connection account just
//!   works.
//! - Channels served as HLS by the provider are read segment by segment and
//!   handed on as one MPEG-TS stream.
//! - A connection that breaks is made again, a few times, while the channel
//!   is watched.
//! - Whether a channel played is told ([`Played`]): one the provider gives
//!   nothing for is offline until it plays again.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use pley_transcoder_api::{HwAccel, LivePlan, LiveRequest};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use super::{ChannelRow, LiveSettings};
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::events::{EventBus, ServerEvent};
use crate::transcoder::Transcoder;
use crate::util::now;

/// A channel nobody watches is let go this long after the last viewer left
/// (unless its connection is wanted sooner): coming back to it is instant.
const LINGER: Duration = Duration::from_secs(20);
/// An HLS viewer that asked for nothing this long left (players ask for the
/// playlist every few seconds while they play; a paused one too).
const VIEWER_GONE: Duration = Duration::from_secs(45);
/// What the provider last sent, given to whoever joins (from its last video
/// keyframe): a quick start.
const BACKLOG_BYTES: usize = 2 << 20;
/// An MPEG-TS packet's size, and the byte it starts with.
const TS_PACKET: usize = 188;
const TS_SYNC: u8 = 0x47;
/// Chunks a slow reader may fall behind before it skips some.
const CHANNEL_CHUNKS: usize = 2048;
/// The provider sending nothing this long breaks the connection.
const STALL: Duration = Duration::from_secs(15);
/// Connection attempts in a row before a channel is given up on.
const ATTEMPTS: u32 = 5;
/// Bytes after which a connection counts as having worked (its failures
/// start again from 0).
const WORKED_BYTES: u64 = 4 << 20;

/// Whether a channel played: it sent something, or the provider gave
/// nothing for it however often it was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub channel_id: i64,
    pub played: bool,
}

/// How a provider refusing the account (not the channel) begins.
const REFUSED: &str = "the provider refused it";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewerKind {
    /// Live HLS from the transcoder.
    Hls,
    /// The provider's MPEG-TS as it is.
    Stream,
}

#[derive(Debug, Clone)]
struct Viewer {
    id: String,
    user_id: i64,
    username: String,
    client_id: Option<String>,
    channel_id: i64,
    started_at: i64,
    last_seen: Instant,
    kind: ViewerKind,
    /// Its keyed addresses' key, for a player that can't send the
    /// Authorization header (a TV's own player).
    media_key: Option<String>,
    /// The app sign-in it was started with: the key ends with it.
    app_grant: Option<i64>,
    /// The account is an administrator's (whatever signed it in): whose
    /// connections it may take over.
    admin: bool,
    /// The account is in the home: who took its connection over is named
    /// to it when they are too.
    home: bool,
}

/// How a channel is watched.
pub struct WatchAs {
    /// The device watching: what it watched before ends.
    pub client_id: Option<String>,
    pub kind: ViewerKind,
    /// Its playlist's address carries a key (a player that can't send the
    /// Authorization header).
    pub keyed: bool,
    /// The account is an administrator's (whatever signed it in).
    pub admin: bool,
    /// Every connection being in use, the channel whose connection to take.
    pub take_over: Option<i64>,
    /// The account's name when it is in the home (`None` when it isn't):
    /// the viewers it takes a connection from who are in the home too are
    /// told who did.
    pub home_name: Option<String>,
}

/// A connection of a source whose connections are all in use, as someone
/// refused one is told of it.
#[derive(Debug, Clone, Serialize)]
pub struct BusyConnection {
    pub channel_id: i64,
    pub channel_name: String,
    /// Who watches it: named to administrators only.
    pub watchers: Vec<String>,
    /// How many people watch it.
    pub people: usize,
    /// Only the one asking watches it (on another device).
    pub yours: bool,
    /// The one asking may take it over (`take_over`): it's theirs, or they
    /// are an administrator and nobody watching it is.
    pub can_take_over: bool,
}

/// What a watching session ended by someone taking its connection over says.
const ENDED_KEPT: usize = 500;

/// Someone watching, as administrators see it.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerStatus {
    pub session_id: String,
    pub user_id: i64,
    pub username: String,
    pub kind: ViewerKind,
    pub started_at: i64,
}

/// A connection in use, as administrators see it.
#[derive(Debug, Clone, Serialize)]
pub struct TunerStatus {
    pub channel_id: i64,
    pub channel_name: String,
    pub source_id: i64,
    pub source_name: String,
    pub started_at: i64,
    /// The provider is sending.
    pub connected: bool,
    pub error: Option<String>,
    pub bytes_in: u64,
    /// What the provider sends, kbit/s (over the last seconds).
    pub kbps: u64,
    pub viewers: Vec<ViewerStatus>,
    /// Nobody watches: it goes soon.
    pub idle: bool,
    /// What the transcoder makes of it, for HLS viewers.
    pub plan: Option<LivePlan>,
}

/// What a player watches with.
#[derive(Debug, Clone, Serialize)]
pub struct Watching {
    pub session_id: String,
    /// The live HLS playlist (HLS viewers).
    pub url: String,
    pub kind: ViewerKind,
}

struct Feed {
    backlog: VecDeque<Bytes>,
    backlog_bytes: usize,
    /// Chunks dropped from the backlog's front: the number of its first.
    dropped: u64,
    /// Where the last video keyframe starts: its chunk's number and the
    /// offset in it.
    keyframe: Option<(u64, usize)>,
    ts: TsScan,
    tx: Option<broadcast::Sender<Bytes>>,
}

impl Feed {
    fn new() -> Self {
        Self {
            backlog: VecDeque::new(),
            backlog_bytes: 0,
            dropped: 0,
            keyframe: None,
            ts: TsScan::default(),
            tx: Some(broadcast::channel(CHANNEL_CHUNKS).0),
        }
    }

    fn push(&mut self, chunk: &Bytes) {
        if let Some(offset) = self.ts.scan(chunk) {
            self.keyframe = Some((self.dropped + self.backlog.len() as u64, offset));
        }
        self.backlog_bytes += chunk.len();
        self.backlog.push_back(chunk.clone());
        while self.backlog_bytes > BACKLOG_BYTES {
            let Some(old) = self.backlog.pop_front() else { break };
            self.backlog_bytes -= old.len();
            self.dropped += 1;
        }
        if self.keyframe.is_some_and(|(n, _)| n < self.dropped) {
            self.keyframe = None;
        }
    }

    /// What a new reader is given first: the tables and what came from the
    /// last video keyframe, so the picture starts whole and at once instead
    /// of seconds behind (players skip ahead to catch up: the picture jumps).
    /// A stream without marked keyframes is given all of the backlog.
    fn start(&self) -> Vec<Bytes> {
        let (Some((n, offset)), Some(pat), Some(pmt)) = (self.keyframe, &self.ts.pat, &self.ts.pmt) else {
            return self.backlog.iter().cloned().collect();
        };
        let from = (n - self.dropped) as usize;
        let mut out = vec![pat.clone(), pmt.clone()];
        for (i, chunk) in self.backlog.iter().skip(from).enumerate() {
            out.push(if i == 0 { chunk.slice(offset..) } else { chunk.clone() });
        }
        out
    }
}

/// Reads the MPEG-TS packets in what a provider sends, chunk by chunk
/// (packets run across chunks): the tables a demuxer needs before anything
/// else (PAT and PMT) and where video keyframes start.
#[derive(Default)]
struct TsScan {
    /// Where the next packet starts in the next chunk; `None` until the
    /// packets are found.
    next: Option<usize>,
    pat: Option<Bytes>,
    pmt_pid: Option<u16>,
    pmt: Option<Bytes>,
}

impl TsScan {
    /// Where the last video keyframe's first packet starts in `chunk`.
    fn scan(&mut self, chunk: &Bytes) -> Option<usize> {
        let data = &chunk[..];
        let mut at = match self.next {
            Some(n) if n >= data.len() => {
                self.next = Some(n - data.len());
                return None;
            }
            Some(n) if data[n] == TS_SYNC => n,
            _ => ts_sync(data)?,
        };
        let mut keyframe = None;
        while at + TS_PACKET <= data.len() {
            if data[at] != TS_SYNC {
                // Lost: found again further on, or in the next chunk.
                match ts_sync(&data[at..]) {
                    Some(skip) => at += skip,
                    None => {
                        self.next = None;
                        return keyframe;
                    }
                }
                continue;
            }
            let packet = &data[at..at + TS_PACKET];
            let pid = u16::from(packet[1] & 0x1f) << 8 | u16::from(packet[2]);
            if packet[1] & 0x40 != 0 {
                if pid == 0 {
                    self.pat = Some(Bytes::copy_from_slice(packet));
                    self.pmt_pid = pmt_pid(packet).or(self.pmt_pid);
                } else if Some(pid) == self.pmt_pid {
                    self.pmt = Some(Bytes::copy_from_slice(packet));
                } else if is_video_keyframe(packet) {
                    keyframe = Some(at);
                }
            }
            at += TS_PACKET;
        }
        self.next = Some(if at < data.len() { TS_PACKET - (data.len() - at) } else { 0 });
        keyframe
    }
}

/// Where packets start in `data`: a sync byte with another a packet on, when
/// there's that much.
fn ts_sync(data: &[u8]) -> Option<usize> {
    (0..data.len()).find(|&i| data[i] == TS_SYNC && data.get(i + TS_PACKET).is_none_or(|&b| b == TS_SYNC))
}

/// A packet's payload, after its adaptation field.
fn ts_payload(packet: &[u8]) -> Option<&[u8]> {
    let control = (packet[3] >> 4) & 3;
    if control & 1 == 0 {
        return None;
    }
    let start = if control & 2 != 0 { 5 + usize::from(packet[4]) } else { 4 };
    packet.get(start..)
}

/// The PMT's PID, from a PAT packet: its first program's.
fn pmt_pid(packet: &[u8]) -> Option<u16> {
    let payload = ts_payload(packet)?;
    let table = payload.get(1 + usize::from(*payload.first()?)..)?;
    // table_id 0, then the section's length and five bytes of header.
    if table.first() != Some(&0) {
        return None;
    }
    let length = usize::from(u16::from(*table.get(1)? & 0x0f) << 8 | u16::from(*table.get(2)?));
    let programs = table.get(8..(3 + length).saturating_sub(4))?;
    programs
        .as_chunks::<4>()
        .0
        .iter()
        .find(|p| p[0] != 0 || p[1] != 0)
        .map(|p| u16::from(p[2] & 0x1f) << 8 | u16::from(p[3]))
}

/// The device a client id is: players read a channel's stream as
/// `<client_id>-stream`, beside the session asked for as `<client_id>`.
fn device_of(client: &str) -> &str {
    client.strip_suffix("-stream").unwrap_or(client)
}

/// What a viewer whose connection `taker` took over to watch `channel` is
/// told. The taker is named, with the channel, only to their own devices
/// and to someone else in the home when they are in it too (`home_name`);
/// anyone else learns only that the connection was needed.
fn taken_over_why(viewer: &Viewer, taker: i64, home_name: Option<&str>, channel: &str) -> String {
    if viewer.user_id == taker {
        return format!("Watching moved to {channel} on another device.");
    }
    match home_name {
        Some(name) if viewer.home => format!("{name} is watching {channel} now, on the connection this channel used."),
        _ => "Someone else needed this channel's connection to watch another channel.".to_owned(),
    }
}

/// A packet that starts a video PES at a random access point: a keyframe.
fn is_video_keyframe(packet: &[u8]) -> bool {
    let control = (packet[3] >> 4) & 3;
    let random_access = control & 2 != 0 && packet[4] > 0 && packet[5] & 0x40 != 0;
    random_access
        && ts_payload(packet).is_some_and(|p| p.len() >= 4 && p[..3] == [0, 0, 1] && (0xe0..=0xef).contains(&p[3]))
}

struct TunerState {
    connected: bool,
    error: Option<String>,
    bytes_in: u64,
    /// Bytes counted at the start of the current rate window.
    rate: (Instant, u64, u64),
    idle_since: Option<Instant>,
    /// The provider was given up on.
    ended: bool,
    /// The transcoder's job failed (and why): not made again for this tuner.
    hls_failed: Option<String>,
    plan: Option<LivePlan>,
}

impl Default for TunerState {
    fn default() -> Self {
        Self {
            connected: false,
            error: None,
            bytes_in: 0,
            rate: (Instant::now(), 0, 0),
            idle_since: None,
            ended: false,
            hls_failed: None,
            plan: None,
        }
    }
}

struct Tuner {
    channel_id: i64,
    channel_name: String,
    source_id: i64,
    source_name: String,
    url: String,
    user_agent: String,
    started_at: i64,
    feed: Mutex<Feed>,
    state: Mutex<TunerState>,
    stop: CancellationToken,
    /// The transcoder's live job, once an HLS viewer came.
    hls: tokio::sync::Mutex<Option<String>>,
    played: mpsc::UnboundedSender<Played>,
}

impl Tuner {
    fn is_live(&self) -> bool {
        !self.stop.is_cancelled() && !self.state.lock().ended
    }

    fn push(&self, chunk: Bytes) {
        {
            let mut state = self.state.lock();
            if state.bytes_in == 0 {
                let _ = self.played.send(Played { channel_id: self.channel_id, played: true });
            }
            state.bytes_in += chunk.len() as u64;
            state.connected = true;
            state.error = None;
        }
        let mut feed = self.feed.lock();
        feed.push(&chunk);
        if let Some(tx) = &feed.tx {
            let _ = tx.send(chunk);
        }
    }

    /// What came lately and what comes from now on, as one stream; `None`
    /// once the tuner stopped.
    fn subscribe(&self) -> Option<impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static + use<>> {
        let feed = self.feed.lock();
        let rx = feed.tx.as_ref()?.subscribe();
        let backlog = feed.start();
        drop(feed);
        let live = futures::stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(chunk) => return Some((Ok(chunk), rx)),
                    // A reader that fell behind skips what it missed: the
                    // MPEG-TS demuxer finds its way again.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        });
        Some(futures::stream::iter(backlog.into_iter().map(Ok)).chain(live))
    }

    fn close(&self) {
        self.stop.cancel();
        self.feed.lock().tx = None;
    }

    fn kbps(&self) -> u64 {
        let mut state = self.state.lock();
        let (since, at_start, last) = state.rate;
        let elapsed = since.elapsed().as_secs_f64();
        if elapsed >= 10.0 {
            let rate = ((state.bytes_in - at_start) as f64 * 8.0 / 1000.0 / elapsed) as u64;
            state.rate = (Instant::now(), state.bytes_in, rate);
            rate
        } else if last == 0 && elapsed > 1.0 {
            ((state.bytes_in - at_start) as f64 * 8.0 / 1000.0 / elapsed) as u64
        } else {
            last
        }
    }
}

pub struct Tuners {
    transcoder: Arc<Transcoder>,
    events: EventBus,
    http: reqwest::Client,
    tuners: Mutex<HashMap<i64, Arc<Tuner>>>,
    viewers: Mutex<HashMap<String, Viewer>>,
    /// Decisions to start a tuner, one at a time (they count connections).
    starting: tokio::sync::Mutex<()>,
    /// Why sessions ended that were taken over: their players are told.
    ended: Mutex<HashMap<String, String>>,
    played: mpsc::UnboundedSender<Played>,
}

/// What a tuner needs to know of its channel.
pub struct Tune<'a> {
    pub channel: &'a ChannelRow,
    pub source_name: String,
    pub max_connections: i64,
    pub user_agent: String,
}

impl Tuners {
    pub fn new(
        transcoder: Arc<Transcoder>,
        events: EventBus,
        http: reqwest::Client,
        played: mpsc::UnboundedSender<Played>,
    ) -> Arc<Self> {
        Arc::new(Self {
            transcoder,
            events,
            http,
            tuners: Mutex::default(),
            viewers: Mutex::default(),
            starting: tokio::sync::Mutex::new(()),
            ended: Mutex::default(),
            played,
        })
    }

    /// Whether any channel of `source_id` is being pulled.
    pub fn source_busy(&self, source_id: i64) -> bool {
        self.tuners.lock().values().any(|t| t.source_id == source_id && t.is_live())
    }

    /// Lets go of channels nobody watches, every few seconds.
    pub fn start(self: &Arc<Self>, stopping: CancellationToken) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    () = stopping.cancelled() => break,
                    _ = tick.tick() => {}
                }
                this.reap();
            }
            let all: Vec<Arc<Tuner>> = this.tuners.lock().drain().map(|(_, t)| t).collect();
            for tuner in all {
                this.stop_tuner(&tuner);
            }
        });
    }

    fn reap(&self) {
        let mut changed = false;
        {
            let mut viewers = self.viewers.lock();
            let before = viewers.len();
            viewers.retain(|_, v| v.kind == ViewerKind::Stream || v.last_seen.elapsed() < VIEWER_GONE);
            changed |= viewers.len() != before;
        }
        let watched: HashMap<i64, usize> = self.viewers.lock().values().fold(HashMap::new(), |mut m, v| {
            *m.entry(v.channel_id).or_insert(0) += 1;
            m
        });
        let tuners: Vec<Arc<Tuner>> = self.tuners.lock().values().cloned().collect();
        for tuner in tuners {
            let watching = watched.get(&tuner.channel_id).copied().unwrap_or(0);
            let gone = {
                let mut state = tuner.state.lock();
                if watching > 0 {
                    state.idle_since = None;
                    false
                } else {
                    let since = *state.idle_since.get_or_insert_with(Instant::now);
                    state.ended || since.elapsed() >= LINGER
                }
            };
            if gone || tuner.stop.is_cancelled() {
                self.remove_tuner(&tuner);
                changed = true;
            }
        }
        if changed {
            self.events.send(ServerEvent::LiveTunersChanged);
        }
    }

    fn remove_tuner(&self, tuner: &Arc<Tuner>) {
        {
            let mut tuners = self.tuners.lock();
            if tuners.get(&tuner.channel_id).is_some_and(|t| Arc::ptr_eq(t, tuner)) {
                tuners.remove(&tuner.channel_id);
            }
        }
        self.stop_tuner(tuner);
    }

    fn stop_tuner(&self, tuner: &Arc<Tuner>) {
        if !tuner.stop.is_cancelled() {
            tracing::info!(channel = tuner.channel_id, name = %tuner.channel_name, "live channel let go");
        }
        tuner.close();
        let (transcoder, tuner) = (self.transcoder.clone(), tuner.clone());
        tokio::spawn(async move {
            if let Some(job) = tuner.hls.lock().await.take() {
                let _ = transcoder.delete_live(&job).await;
            }
        });
    }

    /// Channels pulled now, by source.
    pub fn in_use(&self) -> HashMap<i64, usize> {
        let mut out = HashMap::new();
        for t in self.tuners.lock().values().filter(|t| t.is_live()) {
            *out.entry(t.source_id).or_insert(0) += 1;
        }
        out
    }

    pub async fn stop_source(&self, source_id: i64) {
        let tuners: Vec<Arc<Tuner>> =
            self.tuners.lock().values().filter(|t| t.source_id == source_id).cloned().collect();
        for t in &tuners {
            self.end_viewers_of(t.channel_id);
            self.remove_tuner(t);
        }
        if !tuners.is_empty() {
            self.events.send(ServerEvent::LiveTunersChanged);
        }
    }

    pub async fn stop_channel(&self, channel_id: i64) {
        let tuner = self.tuners.lock().get(&channel_id).cloned();
        if let Some(t) = tuner {
            self.end_viewers_of(channel_id);
            self.remove_tuner(&t);
            self.events.send(ServerEvent::LiveTunersChanged);
        }
    }

    fn end_viewers_of(&self, channel_id: i64) {
        self.viewers.lock().retain(|_, v| v.channel_id != channel_id);
    }

    /// Starts `user` watching `tune.channel`: through the tuner already
    /// pulling it, or a new one when its source has a connection free.
    /// `keyed`: the HLS playlist's address carries a key of its own instead
    /// of needing the token (docs/API.md, "Keyed URLs").
    pub async fn watch(self: &Arc<Self>, user: &AuthUser, tune: Tune<'_>, how: WatchAs) -> Result<Watching, AppError> {
        let WatchAs { client_id, kind, keyed, admin, take_over, home_name } = how;
        let channel_id = tune.channel.id;
        // What this device watched before ends: its connection is free. Its
        // stream reader of another channel too, though that response may not
        // have closed yet (channels changed quickly would find them all in use).
        if let Some(client) = client_id.as_deref().filter(|c| !c.is_empty()) {
            self.viewers.lock().retain(|_, v| {
                let Some(theirs) = v.client_id.as_deref() else { return true };
                let same = theirs == client || (device_of(theirs) == device_of(client) && v.channel_id != channel_id);
                !(v.user_id == user.id && same)
            });
        }
        let tuner = {
            let _one = self.starting.lock().await;
            // Not in the match: its guard would live through start_tuner's own lock.
            let pulling = self.tuners.lock().get(&channel_id).filter(|t| t.is_live()).cloned();
            match pulling {
                Some(t) => t,
                None => self.start_tuner(user, admin, home_name.as_deref(), &tune, take_over)?,
            }
        };
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let media_key = (keyed && kind == ViewerKind::Hls).then(crate::playback::new_media_key);
        let url = match &media_key {
            Some(key) => format!("/api/live/media/{key}/index.m3u8"),
            None => format!("/api/live/sessions/{session_id}/index.m3u8"),
        };
        self.viewers.lock().insert(
            session_id.clone(),
            Viewer {
                id: session_id.clone(),
                user_id: user.id,
                username: user.username.clone(),
                client_id,
                channel_id,
                started_at: now(),
                last_seen: Instant::now(),
                kind,
                media_key,
                app_grant: user.app.as_ref().map(|app| app.grant_id),
                admin,
                home: home_name.is_some(),
            },
        );
        tuner.state.lock().idle_since = None;
        self.events.send(ServerEvent::LiveTunersChanged);
        Ok(Watching { url, session_id, kind })
    }

    /// The session a keyed address's `key` opens, who watches in it, and
    /// the app sign-in it was started with.
    pub fn by_media_key(&self, key: &str) -> Option<(String, i64, Option<i64>)> {
        self.viewers
            .lock()
            .values()
            .find(|v| v.media_key.as_deref().is_some_and(|k| crate::playback::same_key(k, key)))
            .map(|v| (v.id.clone(), v.user_id, v.app_grant))
    }

    fn start_tuner(
        self: &Arc<Self>,
        user: &AuthUser,
        admin: bool,
        home_name: Option<&str>,
        tune: &Tune<'_>,
        take_over: Option<i64>,
    ) -> Result<Arc<Tuner>, AppError> {
        let watched: HashMap<i64, usize> = self.viewers.lock().values().fold(HashMap::new(), |mut m, v| {
            *m.entry(v.channel_id).or_insert(0) += 1;
            m
        });
        let on_source: Vec<Arc<Tuner>> = self
            .tuners
            .lock()
            .values()
            .filter(|t| t.source_id == tune.channel.source_id && t.is_live())
            .cloned()
            .collect();
        if tune.max_connections > 0 && on_source.len() as i64 >= tune.max_connections {
            // A channel nobody watches gives its connection up.
            match on_source.iter().find(|t| !watched.contains_key(&t.channel_id)) {
                Some(idle) => self.remove_tuner(idle),
                None => match take_over.and_then(|id| on_source.iter().find(|t| t.channel_id == id)) {
                    Some(held) if self.may_take_over(user.id, admin, held.channel_id) => {
                        self.take_over(user, home_name, held, tune.channel.display_name());
                    }
                    Some(_) => {
                        return Err(AppError::forbidden(
                            "someone else watches that channel: only an administrator takes it over, from those who aren't",
                        ));
                    }
                    None => return Err(self.all_in_use(user, admin, tune, &on_source)),
                },
            }
        }
        let tuner = Arc::new(Tuner {
            channel_id: tune.channel.id,
            channel_name: tune.channel.display_name().to_owned(),
            source_id: tune.channel.source_id,
            source_name: tune.source_name.clone(),
            url: tune.channel.stream_url.clone(),
            user_agent: tune.user_agent.clone(),
            started_at: now(),
            feed: Mutex::new(Feed::new()),
            state: Mutex::new(TunerState::default()),
            stop: CancellationToken::new(),
            hls: tokio::sync::Mutex::new(None),
            played: self.played.clone(),
        });
        tracing::info!(channel = tuner.channel_id, name = %tuner.channel_name, source = %tuner.source_name, "live channel tuned");
        self.tuners.lock().insert(tuner.channel_id, tuner.clone());
        let (http, t) = (self.http.clone(), tuner.clone());
        tokio::spawn(async move { pull(t, http).await });
        Ok(tuner)
    }

    /// Whether a user may take channel `channel_id`'s connection over: all
    /// who watch it are them, or they are an administrator and none of
    /// those watching it is.
    fn may_take_over(&self, user_id: i64, admin: bool, channel_id: i64) -> bool {
        self.viewers
            .lock()
            .values()
            .filter(|v| v.channel_id == channel_id)
            .all(|v| v.user_id == user_id || (admin && !v.admin))
    }

    /// Ends everyone watching `held`, telling them why (their players at
    /// once, through their event streams), and lets its connection go for
    /// `user` to watch `channel_name` with. `home_name`: `user`'s name, when
    /// they are in the home.
    fn take_over(&self, user: &AuthUser, home_name: Option<&str>, held: &Arc<Tuner>, channel_name: &str) {
        let told: Vec<(Viewer, String)> = {
            let mut viewers = self.viewers.lock();
            let mut ended = self.ended.lock();
            if ended.len() > ENDED_KEPT {
                ended.clear();
            }
            let told: Vec<(Viewer, String)> = viewers
                .values()
                .filter(|v| v.channel_id == held.channel_id)
                .map(|v| (v.clone(), taken_over_why(v, user.id, home_name, channel_name)))
                .collect();
            for (v, why) in &told {
                ended.insert(v.id.clone(), why.clone());
            }
            viewers.retain(|_, v| v.channel_id != held.channel_id);
            told
        };
        tracing::info!(channel = held.channel_id, by = %user.username, "live channel's connection taken over");
        // Told before the stream stops: a player whose stream breaks off
        // knows why instead of tuning again.
        for (v, message) in told {
            self.events.send(ServerEvent::LiveTakenOver {
                user_id: v.user_id,
                session_id: v.id,
                channel_id: v.channel_id,
                message,
            });
        }
        self.remove_tuner(held);
        self.events.send(ServerEvent::LiveTunersChanged);
    }

    fn all_in_use(&self, user: &AuthUser, admin: bool, tune: &Tune<'_>, on_source: &[Arc<Tuner>]) -> AppError {
        let connections: Vec<BusyConnection> = {
            let viewers = self.viewers.lock();
            on_source
                .iter()
                .map(|t| {
                    let watching: Vec<&Viewer> = viewers.values().filter(|v| v.channel_id == t.channel_id).collect();
                    let mut who: Vec<String> = watching.iter().map(|v| v.username.clone()).collect();
                    // One person on two devices is named once.
                    who.sort_unstable();
                    who.dedup();
                    BusyConnection {
                        channel_id: t.channel_id,
                        channel_name: t.channel_name.clone(),
                        people: who.len(),
                        watchers: if admin { who } else { Vec::new() },
                        yours: !watching.is_empty() && watching.iter().all(|v| v.user_id == user.id),
                        can_take_over: watching.iter().all(|v| v.user_id == user.id || (admin && !v.admin)),
                    }
                })
                .collect()
        };
        let n = tune.max_connections;
        let message = format!(
            "{} allows {} at once, and {} in use: watching {}.",
            tune.source_name,
            if n == 1 { "one connection".to_owned() } else { format!("{n} connections") },
            if n == 1 { "it is" } else { "all are" },
            connections.iter().map(|c| c.channel_name.as_str()).collect::<Vec<_>>().join(", ")
        );
        AppError::ConflictWith(message, serde_json::json!({ "connections": connections }))
    }

    fn viewer(&self, session_id: &str, user: &AuthUser) -> Result<Viewer, AppError> {
        let mut viewers = self.viewers.lock();
        let Some(v) = viewers.get_mut(session_id).filter(|v| v.user_id == user.id) else {
            let why = self.ended.lock().get(session_id).cloned();
            return Err(AppError::SessionStopped(Some(
                why.unwrap_or_else(|| "this channel isn't being watched any more".into()),
            )));
        };
        v.last_seen = Instant::now();
        Ok(v.clone())
    }

    fn tuner_of(&self, viewer: &Viewer) -> Result<Arc<Tuner>, AppError> {
        self.tuners
            .lock()
            .get(&viewer.channel_id)
            .cloned()
            .ok_or_else(|| AppError::SessionStopped(Some("the channel was stopped".into())))
    }

    pub fn leave(&self, session_id: &str, user: &AuthUser) -> bool {
        let removed = {
            let mut viewers = self.viewers.lock();
            match viewers.get(session_id) {
                Some(v) if v.user_id == user.id || user.is_admin => viewers.remove(session_id).is_some(),
                _ => false,
            }
        };
        if removed {
            self.events.send(ServerEvent::LiveTunersChanged);
        }
        removed
    }

    /// The live playlist of an HLS viewer's channel, the transcoder's job
    /// made when it is the first.
    /// What the transcoder makes of the channel a session watches, once it
    /// has looked at its first moment (starting its live job when there is
    /// none yet); `None` when that takes longer than `wait`. Players that
    /// read MPEG-TS themselves play a channel sent as it is (H.264 and AAC)
    /// straight from `stream.ts`, sooner than its first segments.
    pub async fn plan(
        &self,
        session_id: &str,
        user: &AuthUser,
        settings: &LiveSettings,
        hwaccel: HwAccel,
        wait: Duration,
    ) -> Result<Option<LivePlan>, AppError> {
        let viewer = self.viewer(session_id, user)?;
        let tuner = self.tuner_of(&viewer)?;
        let job = self.hls_job(&tuner, settings, hwaccel).await?;
        let deadline = Instant::now() + wait;
        loop {
            if let Some(plan) = tuner.state.lock().plan.clone() {
                return Ok(Some(plan));
            }
            if let Ok(status) = self.transcoder.live_status(&job).await
                && let Some(plan) = status.plan
            {
                tuner.state.lock().plan = Some(plan.clone());
                return Ok(Some(plan));
            }
            if Instant::now() >= deadline || !tuner.is_live() {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    pub async fn playlist(
        &self,
        session_id: &str,
        user: &AuthUser,
        settings: &LiveSettings,
        hwaccel: HwAccel,
    ) -> Result<String, AppError> {
        let viewer = self.viewer(session_id, user)?;
        let tuner = self.tuner_of(&viewer)?;
        for attempt in 0..2 {
            let job = self.hls_job(&tuner, settings, hwaccel).await?;
            match self.transcoder.live_playlist(&job).await {
                Ok(text) => return Ok(text),
                Err(err) if err.is_job_gone() && attempt == 0 => {
                    // The transcoder restarted, or gave up on it: once more.
                    let mut hls = tuner.hls.lock().await;
                    if hls.as_deref() == Some(job.as_str()) {
                        *hls = None;
                    }
                }
                Err(err) => return Err(self.why_not(&tuner, err.into())),
            }
        }
        Err(self.why_not(&tuner, AppError::Unavailable("the channel didn't start".into())))
    }

    /// The error a player gets: what the provider did, when it is to blame.
    fn why_not(&self, tuner: &Tuner, fallback: AppError) -> AppError {
        let state = tuner.state.lock();
        match (&state.error, state.connected, &state.hls_failed) {
            (Some(why), false, _) => AppError::Unavailable(format!("{} isn't playing: {why}", tuner.channel_name)),
            (_, _, Some(why)) => AppError::Unavailable(format!("{} can't be shown: {why}", tuner.channel_name)),
            _ => fallback,
        }
    }

    async fn hls_job(&self, tuner: &Arc<Tuner>, settings: &LiveSettings, hwaccel: HwAccel) -> Result<String, AppError> {
        let mut hls = tuner.hls.lock().await;
        if let Some(job) = hls.clone() {
            return Ok(job);
        }
        if let Some(why) = tuner.state.lock().hls_failed.clone() {
            return Err(AppError::Unavailable(format!("{} can't be shown: {why}", tuner.channel_name)));
        }
        if !tuner.is_live() {
            return Err(self.why_not(tuner, AppError::Unavailable("the channel stopped".into())));
        }
        if !self.transcoder.is_available() {
            return Err(AppError::Unavailable("the transcoder isn't running".into()));
        }
        if !self.transcoder.makes_live() {
            return Err(AppError::Unavailable("this transcoder doesn't make live channels: update it".into()));
        }
        let created = self
            .transcoder
            .create_live(&LiveRequest {
                label: Some(tuner.channel_name.clone()),
                owner: Some("Live TV".into()),
                deinterlace: settings.deinterlace,
                force_transcode: false,
                max_height: settings.max_height,
                max_bitrate_kbps: None,
                hwaccel,
                window_seconds: Some(settings.timeshift_minutes * 60),
            })
            .await?;
        let input = tuner.subscribe().ok_or_else(|| AppError::Unavailable("the channel stopped".into()))?;
        let (transcoder, t, job) = (self.transcoder.clone(), tuner.clone(), created.id.clone());
        tokio::spawn(async move {
            let result = transcoder.feed_live(&job, input).await;
            if t.stop.is_cancelled() {
                return;
            }
            // The transcoder stopped making it: say why, unless the provider is to blame.
            if let Ok(status) = transcoder.live_status(&job).await {
                t.state.lock().plan = status.plan;
                let connected = t.state.lock().connected;
                if let Some(why) = status.error
                    && connected
                {
                    tracing::warn!(channel = t.channel_id, "the transcoder gave up on a live channel: {why}");
                    t.state.lock().hls_failed = Some(why);
                }
            } else if let Err(err) = result {
                tracing::warn!(channel = t.channel_id, "sending a live channel to the transcoder failed: {err}");
            }
            let mut hls = t.hls.lock().await;
            if hls.as_deref() == Some(job.as_str()) {
                *hls = None;
            }
        });
        // What it is made into, for the administrators' view, once it is known.
        let (transcoder, t, job) = (self.transcoder.clone(), tuner.clone(), created.id.clone());
        tokio::spawn(async move {
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                match transcoder.live_status(&job).await {
                    Ok(status) if status.plan.is_some() => {
                        t.state.lock().plan = status.plan;
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
        *hls = Some(created.id.clone());
        Ok(created.id)
    }

    /// A segment of an HLS viewer's channel.
    pub async fn segment(&self, session_id: &str, user: &AuthUser, name: &str) -> Result<reqwest::Response, AppError> {
        if !is_segment_name(name) {
            return Err(AppError::not_found(format!("no file {name}")));
        }
        let viewer = self.viewer(session_id, user)?;
        let tuner = self.tuner_of(&viewer)?;
        let job = tuner.hls.lock().await.clone().ok_or_else(|| AppError::not_found("the channel is starting again"))?;
        Ok(self.transcoder.live_segment(&job, name).await?)
    }

    /// The provider's MPEG-TS of a stream viewer's channel, as it comes,
    /// for as long as the response is read.
    pub fn stream(
        self: &Arc<Self>,
        session_id: &str,
        user: &AuthUser,
    ) -> Result<impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static + use<>, AppError> {
        let viewer = self.viewer(session_id, user)?;
        let tuner = self.tuner_of(&viewer)?;
        let stream = tuner.subscribe().ok_or_else(|| AppError::Unavailable("the channel stopped".into()))?;
        let guard = Leaves { tuners: self.clone(), session_id: session_id.to_owned() };
        Ok(stream.map(move |chunk| {
            let _watching = &guard;
            chunk
        }))
    }

    /// The connections in use, for administrators.
    pub async fn status(&self) -> Vec<TunerStatus> {
        let tuners: Vec<Arc<Tuner>> = self.tuners.lock().values().filter(|t| !t.stop.is_cancelled()).cloned().collect();
        let viewers: Vec<Viewer> = self.viewers.lock().values().cloned().collect();
        let mut out: Vec<TunerStatus> = tuners
            .iter()
            .map(|t| {
                let watching: Vec<ViewerStatus> = viewers
                    .iter()
                    .filter(|v| v.channel_id == t.channel_id)
                    .map(|v| ViewerStatus {
                        session_id: v.id.clone(),
                        user_id: v.user_id,
                        username: v.username.clone(),
                        kind: v.kind,
                        started_at: v.started_at,
                    })
                    .collect();
                let kbps = t.kbps();
                let state = t.state.lock();
                TunerStatus {
                    channel_id: t.channel_id,
                    channel_name: t.channel_name.clone(),
                    source_id: t.source_id,
                    source_name: t.source_name.clone(),
                    started_at: t.started_at,
                    connected: state.connected,
                    error: state.error.clone().or_else(|| state.hls_failed.clone()),
                    bytes_in: state.bytes_in,
                    kbps,
                    idle: watching.is_empty(),
                    viewers: watching,
                    plan: state.plan.clone(),
                }
            })
            .collect();
        out.sort_by_key(|t| (t.source_id, t.started_at));
        out
    }
}

/// Ends a stream viewer when its response is dropped.
struct Leaves {
    tuners: Arc<Tuners>,
    session_id: String,
}

impl Drop for Leaves {
    fn drop(&mut self) {
        if self.tuners.viewers.lock().remove(&self.session_id).is_some() {
            self.tuners.events.send(ServerEvent::LiveTunersChanged);
        }
    }
}

/// `s{n}.ts`, the transcoder's live segments.
fn is_segment_name(name: &str) -> bool {
    name.strip_prefix('s')
        .and_then(|n| n.strip_suffix(".ts"))
        .is_some_and(|n| !n.is_empty() && n.len() <= 12 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Pulls the channel while the tuner lives, connecting again when it breaks.
async fn pull(tuner: Arc<Tuner>, http: reqwest::Client) {
    let mut failures = 0u32;
    let mut refused = false;
    while !tuner.stop.is_cancelled() {
        let before = tuner.state.lock().bytes_in;
        let result = tokio::select! {
            result = pull_once(&tuner, &http) => result,
            () = tuner.stop.cancelled() => break,
        };
        let got = tuner.state.lock().bytes_in - before;
        if got >= WORKED_BYTES {
            failures = 0;
        }
        failures += 1;
        let why = match result {
            Ok(()) => "the provider ended the stream".to_owned(),
            Err(why) => why,
        };
        refused = why.starts_with(REFUSED);
        tracing::info!(channel = tuner.channel_id, attempt = failures, "live channel's connection broke: {why}");
        {
            let mut state = tuner.state.lock();
            state.error = Some(why);
            state.connected = false;
        }
        if failures >= ATTEMPTS {
            break;
        }
        let wait = Duration::from_millis(500 * (1 << failures.min(4)));
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = tuner.stop.cancelled() => break,
        }
    }
    let gave_nothing = tuner.state.lock().bytes_in == 0;
    // Nothing ever came for it, and not for want of a connection: offline.
    if gave_nothing && !refused && !tuner.stop.is_cancelled() {
        let _ = tuner.played.send(Played { channel_id: tuner.channel_id, played: false });
    }
    tuner.state.lock().ended = true;
    tuner.feed.lock().tx = None;
}

/// One connection to the provider, until it ends: `Err` says how it failed.
async fn pull_once(tuner: &Tuner, http: &reqwest::Client) -> Result<(), String> {
    let mut req = http.get(&tuner.url);
    if !tuner.user_agent.is_empty() {
        req = req.header(reqwest::header::USER_AGENT, &tuner.user_agent);
    }
    let mut resp = tokio::time::timeout(Duration::from_secs(15), req.send())
        .await
        .map_err(|_| "the provider didn't answer".to_owned())?
        .map_err(|e| super::describe(&e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 | 458 | 509 => {
                format!("{REFUSED} ({status}): the account's connections may be in use elsewhere, or it expired")
            }
            404 => "the provider doesn't have this channel now (404)".into(),
            _ => format!("the provider answered {status}"),
        });
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let url = resp.url().clone();
    if content_type.contains("mpegurl") || url.path().to_ascii_lowercase().ends_with(".m3u8") {
        let text = read_text(resp).await?;
        return pull_hls(tuner, http, url, text).await;
    }
    loop {
        match tokio::time::timeout(STALL, resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                // An HLS playlist served as something else.
                if tuner.state.lock().bytes_in == 0 && chunk.starts_with(b"#EXTM3U") {
                    let mut text = String::from_utf8_lossy(&chunk).into_owned();
                    text.push_str(&read_text(resp).await?);
                    return pull_hls(tuner, http, url, text).await;
                }
                tuner.push(chunk);
            }
            Ok(Ok(None)) => return Ok(()),
            Ok(Err(e)) => return Err(super::describe(&e)),
            Err(_) => return Err(format!("the provider sent nothing for {} s", STALL.as_secs())),
        }
    }
}

async fn read_text(resp: reqwest::Response) -> Result<String, String> {
    let bytes = tokio::time::timeout(Duration::from_secs(10), resp.bytes())
        .await
        .map_err(|_| "the provider's playlist didn't come".to_owned())?
        .map_err(|e| super::describe(&e))?;
    if bytes.len() > 2 << 20 {
        return Err("the provider's playlist is too large".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn get_text(tuner: &Tuner, http: &reqwest::Client, url: &url::Url) -> Result<String, String> {
    let mut req = http.get(url.clone()).timeout(Duration::from_secs(10));
    if !tuner.user_agent.is_empty() {
        req = req.header(reqwest::header::USER_AGENT, &tuner.user_agent);
    }
    let resp = req.send().await.map_err(|e| super::describe(&e))?;
    if !resp.status().is_success() {
        return Err(format!("the provider answered {} for its playlist", resp.status()));
    }
    read_text(resp).await
}

/// A channel the provider serves as HLS: its media playlist followed,
/// each new segment handed on as it comes.
async fn pull_hls(tuner: &Tuner, http: &reqwest::Client, mut url: url::Url, mut text: String) -> Result<(), String> {
    if let Some(variant) = best_variant(&text) {
        url = url.join(&variant).map_err(|_| "the provider's playlist names a bad address".to_owned())?;
        text = get_text(tuner, http, &url).await?;
    }
    let mut next: Option<u64> = None;
    loop {
        let playlist = media_playlist(&text)?;
        // Join a few segments behind the live edge, as players do.
        let first = next.unwrap_or_else(|| playlist.last_sequence().saturating_sub(2));
        for (sequence, uri) in playlist.segments.iter().filter(|(s, _)| *s >= first) {
            let segment = url.join(uri).map_err(|_| "the provider's playlist names a bad address".to_owned())?;
            let mut req = http.get(segment).timeout(Duration::from_secs(20));
            if !tuner.user_agent.is_empty() {
                req = req.header(reqwest::header::USER_AGENT, &tuner.user_agent);
            }
            let mut resp = req.send().await.map_err(|e| super::describe(&e))?;
            if !resp.status().is_success() {
                return Err(format!("the provider answered {} for a segment", resp.status()));
            }
            while let Some(chunk) = resp.chunk().await.map_err(|e| super::describe(&e))? {
                tuner.push(chunk);
            }
            next = Some(sequence + 1);
        }
        if playlist.ended {
            return Ok(());
        }
        let wait = Duration::from_secs_f64((playlist.target_duration / 2.0).clamp(1.0, 5.0));
        tokio::time::sleep(wait).await;
        text = get_text(tuner, http, &url).await?;
    }
}

struct MediaPlaylist {
    target_duration: f64,
    segments: Vec<(u64, String)>,
    ended: bool,
}

impl MediaPlaylist {
    fn last_sequence(&self) -> u64 {
        self.segments.last().map_or(0, |(s, _)| *s)
    }
}

/// The variant with the most bits of a multivariant playlist.
fn best_variant(text: &str) -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    let mut lines = text.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let bandwidth = attrs
                .split(',')
                .find_map(|a| a.trim().strip_prefix("BANDWIDTH="))
                .and_then(|b| b.parse().ok())
                .unwrap_or(0);
            if let Some(uri) = lines.by_ref().find(|l| !l.is_empty() && !l.starts_with('#'))
                && best.as_ref().is_none_or(|(b, _)| bandwidth > *b)
            {
                best = Some((bandwidth, uri.to_owned()));
            }
        }
    }
    best.map(|(_, uri)| uri)
}

fn media_playlist(text: &str) -> Result<MediaPlaylist, String> {
    let mut sequence = 0u64;
    let mut target_duration = 6.0;
    let mut segments = Vec::new();
    let mut ended = false;
    let mut in_segment = false;
    for line in text.lines().map(str::trim) {
        if let Some(n) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            sequence = n.parse().unwrap_or(0);
        } else if let Some(d) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = d.parse().unwrap_or(6.0);
        } else if line.starts_with("#EXT-X-MAP") {
            return Err("the provider sends fragmented MP4, which isn't read yet".into());
        } else if let Some(key) = line.strip_prefix("#EXT-X-KEY:") {
            if !key.contains("METHOD=NONE") {
                return Err("the provider's stream is encrypted".into());
            }
        } else if line.starts_with("#EXT-X-ENDLIST") {
            ended = true;
        } else if line.starts_with("#EXTINF") {
            in_segment = true;
        } else if !line.is_empty() && !line.starts_with('#') && in_segment {
            segments.push((sequence + segments.len() as u64, line.to_owned()));
            in_segment = false;
        }
    }
    if segments.is_empty() && !ended {
        return Err("the provider's playlist has no segments".into());
    }
    Ok(MediaPlaylist { target_duration, segments, ended })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_variant_is_the_one_with_most_bits() {
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360\nlow.m3u8\n\
                    #EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080\nhigh/index.m3u8\n";
        assert_eq!(best_variant(text).as_deref(), Some("high/index.m3u8"));
        assert_eq!(best_variant("#EXTM3U\n#EXTINF:2,\na.ts\n"), None);
    }

    #[test]
    fn media_playlists_number_their_segments() {
        let text =
            "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-MEDIA-SEQUENCE:100\n#EXTINF:4.0,\na.ts\n#EXTINF:4.0,\nb.ts\n";
        let p = media_playlist(text).unwrap();
        assert_eq!(p.segments, [(100, "a.ts".to_owned()), (101, "b.ts".to_owned())]);
        assert_eq!((p.target_duration, p.ended, p.last_sequence()), (4.0, false, 101));
        assert!(media_playlist("#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n#EXTINF:4,\na.ts\n").is_err());
        assert!(media_playlist("#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n").is_err());
    }

    fn packet(pid: u16, start: bool, flags: Option<u8>, payload: &[u8]) -> Vec<u8> {
        let mut p = vec![TS_SYNC, if start { 0x40 } else { 0 } | (pid >> 8) as u8, pid as u8, 0x10];
        if let Some(flags) = flags {
            p[3] = 0x30;
            p.extend([1, flags]);
        }
        p.extend_from_slice(payload);
        p.resize(TS_PACKET, 0xff);
        p
    }

    fn pat(pmt: u16) -> Vec<u8> {
        packet(0, true, None, &[0, 0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xe0 | (pmt >> 8) as u8, pmt as u8, 0, 0, 0, 0])
    }

    fn feed(stream: &[u8], cuts: &[usize]) -> Feed {
        let mut feed = Feed::new();
        let mut from = 0;
        for &to in cuts.iter().chain([stream.len()].iter()) {
            feed.push(&Bytes::copy_from_slice(&stream[from..to]));
            from = to;
        }
        feed
    }

    #[test]
    fn new_readers_start_at_the_last_keyframe_with_the_tables() {
        let (pat, pmt) = (pat(0x100), packet(0x100, true, None, &[0, 2]));
        let key = |n: u8| packet(0x101, true, Some(0x40), &[0, 0, 1, 0xe0, n]);
        let frame = packet(0x101, true, Some(0), &[0, 0, 1, 0xe0]);
        // Audio is marked at every frame: not where the picture starts.
        let audio = packet(0x102, true, Some(0x40), &[0, 0, 1, 0xc0]);
        let stream = [pat.clone(), pmt.clone(), key(1), frame.clone(), audio, key(2), frame.clone()].concat();
        // Cut across packets: the scan carries on in the next chunk.
        let start = feed(&stream, &[400, 900]).start().concat();
        assert_eq!(start, [pat, pmt, key(2), frame].concat());
    }

    #[test]
    fn a_stream_without_marked_keyframes_is_given_whole() {
        let audio = packet(0x102, true, Some(0x40), &[0, 0, 1, 0xc0]);
        let stream = [pat(0x100), packet(0x100, true, None, &[0, 2]), audio.clone(), audio].concat();
        assert_eq!(feed(&stream, &[300]).start().concat(), stream);
        // Not MPEG-TS at all: as it came.
        assert_eq!(feed(b"not a transport stream", &[5]).start().concat(), b"not a transport stream");
    }

    #[test]
    fn a_keyframe_dropped_from_the_backlog_is_forgotten() {
        let mut f = feed(
            &[pat(0x100), packet(0x100, true, None, &[0, 2]), packet(0x101, true, Some(0x40), &[0, 0, 1, 0xe0])]
                .concat(),
            &[],
        );
        assert!(f.keyframe.is_some());
        let filler = Bytes::from(packet(0x101, false, None, &[]).repeat(BACKLOG_BYTES / TS_PACKET));
        f.push(&filler);
        assert!(f.keyframe.is_none());
        assert_eq!(f.start(), vec![filler]);
    }

    #[test]
    fn a_pat_cut_short_is_no_pat() {
        // A pointer field leaving one byte of table.
        let mut p = packet(0, true, None, &[]);
        p[4] = (TS_PACKET - 6) as u8;
        p[TS_PACKET - 1] = 0;
        assert_eq!(pmt_pid(&p), None);
    }

    fn viewer(user_id: i64, home: bool) -> Viewer {
        Viewer {
            id: "s".into(),
            user_id,
            username: format!("user{user_id}"),
            client_id: None,
            channel_id: 1,
            started_at: 0,
            last_seen: Instant::now(),
            kind: ViewerKind::Hls,
            media_key: None,
            app_grant: None,
            admin: false,
            home,
        }
    }

    #[test]
    fn who_took_a_connection_over_is_named_only_in_the_home() {
        // Their own other device.
        assert_eq!(taken_over_why(&viewer(1, false), 1, None, "News"), "Watching moved to News on another device.");
        // Both in the home.
        assert_eq!(
            taken_over_why(&viewer(2, true), 1, Some("Sam"), "News"),
            "Sam is watching News now, on the connection this channel used."
        );
        // Either outside it: neither who nor what.
        let anonymous = "Someone else needed this channel's connection to watch another channel.";
        assert_eq!(taken_over_why(&viewer(2, false), 1, Some("Sam"), "News"), anonymous);
        assert_eq!(taken_over_why(&viewer(2, true), 1, None, "News"), anonymous);
    }

    #[test]
    fn a_stream_reader_is_its_device() {
        assert_eq!(device_of("android-1-stream"), "android-1");
        assert_eq!(device_of("android-1"), "android-1");
    }

    #[test]
    fn segment_names_are_checked() {
        assert!(is_segment_name("s5.ts"));
        assert!(!is_segment_name("../s5.ts"));
        assert!(!is_segment_name("index.m3u8"));
    }
}
