// SPDX-License-Identifier: AGPL-3.0-only
//! Live channels (IPTV): an MPEG-TS stream sent in as the channel sends it
//! (`PUT /v1/live/{id}/input`) made into a live HLS stream, a sliding window
//! of short segments (1 s, or a copied channel's keyframe interval) that
//! every player of the channel reads.
//!
//! The first megabyte or so of the input is probed to choose what is made:
//! the video is copied when it is H.264 already (and progressive, unless
//! asked to copy interlaced video too), else transcoded to H.264 with the
//! best encoder that takes frames from system memory; the audio is copied
//! when it is AAC, else made AAC stereo. FFmpeg reads the input from its
//! standard input, so it never opens a connection of its own: the caller
//! holds the one to the channel, which providers count.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use pley_transcoder_api::{
    EncoderInfo, HwAccel, LiveCreated, LivePlan, LiveRequest, LiveState, LiveStatus, VideoCodec, default_video_kbps,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::error::{ApiError, ApiResult};
use crate::ffmpeg::Ffmpeg;
use crate::job::Slots;
use crate::util::now_ms;

/// Segment length, seconds: short, so a channel starts and changes quickly.
/// A transcode makes a keyframe this often; copied video is cut at its own
/// keyframes, so its segments are as long as their interval (often 2 s).
const SEGMENT_SECONDS: u32 = 1;
/// What is kept behind the live edge by default, and at least (seconds).
const DEFAULT_WINDOW: u32 = 300;
const MIN_WINDOW: u32 = 30;
/// Input probed before FFmpeg starts: what came in by each of these moments
/// after its first byte, until one says what the channel carries (its
/// picture's size known: a keyframe came); at the last, or with this much
/// in, whatever it says. A 2 s keyframe interval, the usual, falls within
/// it. What a channel takes to start, so looked at early and often.
const PROBE_STEPS: [Duration; 5] = [
    Duration::from_millis(300),
    Duration::from_millis(600),
    Duration::from_millis(1000),
    Duration::from_millis(1500),
    Duration::from_millis(2500),
];
const PROBE_BYTES: usize = 1_500_000;
/// How long the input may say nothing before the channel is taken for gone.
const INPUT_STALL: Duration = Duration::from_secs(20);
/// Media a player waits for before its first playlist: enough to start
/// without stalling at once, ahead of what is still being made.
const START_MS: u64 = 2000;
const PLAYLIST_WAIT: Duration = Duration::from_secs(30);
/// A channel nobody read anything of for this long is stopped (its caller
/// stops it itself when its last viewer leaves; this is for a caller gone).
const IDLE: Duration = Duration::from_secs(90);
/// Lines of FFmpeg's output kept for an error.
const STDERR_LINES: usize = 12;

pub struct LiveManager {
    ff: Arc<Ffmpeg>,
    slots: Arc<Slots>,
    dir: PathBuf,
    channels: Mutex<HashMap<String, Arc<Live>>>,
    closed: AtomicBool,
}

pub struct Live {
    pub id: String,
    request: LiveRequest,
    dir: PathBuf,
    created_at_ms: i64,
    inner: Mutex<Inner>,
    bytes_in: AtomicU64,
    segments: AtomicU64,
    /// Media made so far, in milliseconds.
    made_ms: AtomicU64,
    lag_ms: AtomicI64,
    last_access: Mutex<Instant>,
    /// Set while an input is being read: one at a time.
    feeding: AtomicBool,
    /// Woken when the playlist changes or the channel ends.
    changed: Notify,
    stop: CancellationToken,
}

#[derive(Default)]
struct Inner {
    state: LiveState,
    plan: Option<LivePlan>,
    error: Option<String>,
}

/// A live channel's file a player asks for.
pub enum LiveFile {
    Playlist(String),
    Segment(PathBuf),
}

impl LiveManager {
    /// Channels in `dir`, the work folder's `live`, made empty and kept to
    /// this account at start-up (main.rs `prepare_work_dir`): not emptied
    /// and made again here, which would give it the default permissions.
    pub fn new(ff: Arc<Ffmpeg>, slots: Arc<Slots>, dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { ff, slots, dir, channels: Mutex::new(HashMap::new()), closed: AtomicBool::new(false) })
    }

    pub fn create(&self, request: LiveRequest) -> ApiResult<LiveCreated> {
        if self.closed.load(Relaxed) {
            return Err(ApiError::Busy("the transcoder is shutting down".into()));
        }
        if !self.ff.caps.ffmpeg_available {
            return Err(ApiError::Failed("FFmpeg isn't available".into()));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.dir.join(&id);
        // `live` as it was made at start-up, even if something took it away
        // since (a clean-up of the folder it is in), then a new folder in
        // it: none another account made, nor a link.
        crate::own_folder(&self.dir)?;
        std::fs::create_dir(&dir)?;
        let live = Arc::new(Live {
            id: id.clone(),
            request,
            dir,
            created_at_ms: now_ms(),
            inner: Mutex::new(Inner::default()),
            bytes_in: AtomicU64::new(0),
            segments: AtomicU64::new(0),
            made_ms: AtomicU64::new(0),
            lag_ms: AtomicI64::new(0),
            last_access: Mutex::new(Instant::now()),
            feeding: AtomicBool::new(false),
            changed: Notify::new(),
            stop: CancellationToken::new(),
        });
        tracing::info!(id = %id, label = ?live.request.label, "live channel created");
        self.channels.lock().insert(id.clone(), live);
        Ok(LiveCreated { id })
    }

    pub fn get(&self, id: &str) -> ApiResult<Arc<Live>> {
        self.channels.lock().get(id).cloned().ok_or_else(|| ApiError::JobNotFound(format!("no live channel {id}")))
    }

    pub fn list(&self) -> Vec<LiveStatus> {
        let mut list: Vec<LiveStatus> = self.channels.lock().values().map(|l| l.status()).collect();
        list.sort_by_key(|s| s.created_at_ms);
        list
    }

    pub fn remove(&self, id: &str) -> bool {
        let Some(live) = self.channels.lock().remove(id) else { return false };
        live.close();
        true
    }

    /// Reads `input` into the channel until it ends, the channel is
    /// stopped, or FFmpeg fails.
    pub async fn feed<S, E>(&self, live: &Arc<Live>, input: S) -> ApiResult<()>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
        E: std::fmt::Display,
    {
        if live.feeding.swap(true, Relaxed) {
            return Err(ApiError::BadRequest("this live channel is already reading an input".into()));
        }
        let result = self.run(live, input).await;
        live.feeding.store(false, Relaxed);
        let mut inner = live.inner.lock();
        match &result {
            Ok(()) => inner.state = LiveState::Ended,
            Err(e) => {
                inner.state = LiveState::Failed;
                inner.error = Some(error_text(e));
            }
        }
        drop(inner);
        live.changed.notify_waiters();
        result
    }

    async fn run<S, E>(&self, live: &Arc<Live>, mut input: S) -> ApiResult<()>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
        E: std::fmt::Display,
    {
        // What comes first is probed, then given to FFmpeg ahead of the rest:
        // the first moment's, and more when that didn't say enough.
        let mut head: Vec<Bytes> = Vec::new();
        let mut head_len = 0usize;
        let mut first: Option<Instant> = None;
        let mut ended = false;
        let mut step = 0;
        let probed = loop {
            let until = PROBE_STEPS[step];
            let full = head_len >= PROBE_BYTES || first.is_some_and(|t| t.elapsed() >= until);
            if full || ended {
                if head_len == 0 {
                    return Err(ApiError::Failed("the channel sent nothing".into()));
                }
                let found = probe(&self.ff, &head).await;
                let last = ended || head_len >= PROBE_BYTES || step == PROBE_STEPS.len() - 1;
                match found {
                    Ok(p) if last || p.complete() => break p,
                    Err(e) if last => return Err(e),
                    // Not enough yet: the next moment, past any this look took.
                    _ => {
                        let elapsed = first.map_or(Duration::ZERO, |t| t.elapsed());
                        step = (step + 1..PROBE_STEPS.len())
                            .find(|&s| PROBE_STEPS[s] > elapsed)
                            .unwrap_or(PROBE_STEPS.len() - 1);
                    }
                }
            }
            let wait = first.map_or(INPUT_STALL, |t| until.saturating_sub(t.elapsed()));
            let next = tokio::select! {
                next = tokio::time::timeout(wait, input.next()) => next,
                () = live.stop.cancelled() => return Ok(()),
            };
            match next {
                Ok(Some(Ok(chunk))) => {
                    first.get_or_insert_with(Instant::now);
                    live.bytes_in.fetch_add(chunk.len() as u64, Relaxed);
                    head_len += chunk.len();
                    head.push(chunk);
                }
                Ok(Some(Err(e))) => return Err(ApiError::Failed(format!("the channel's stream broke off: {e}"))),
                Ok(None) => ended = true,
                Err(_) if first.is_none() => {
                    return Err(ApiError::Timeout("the channel sent nothing".into()));
                }
                Err(_) => {}
            }
        };
        let plan = plan(&self.ff, &live.request, &probed)?;
        let slot = match plan.encoder {
            Some(enc) => Some(self.slots.take(enc.hardware).ok_or_else(|| {
                ApiError::Busy("every transcode slot is taken: no transcode is free for this channel".into())
            })?),
            None => None,
        };
        tracing::info!(
            id = %live.id,
            video = %plan.status.video,
            audio = %plan.status.audio,
            input = ?(&plan.status.video_codec, plan.status.height, plan.status.interlaced, &plan.status.audio_codec),
            "live channel starting"
        );
        {
            let mut inner = live.inner.lock();
            inner.plan = Some(plan.status.clone());
            inner.state = LiveState::Running;
        }
        live.changed.notify_waiters();

        let window = live.request.window_seconds.unwrap_or(DEFAULT_WINDOW).max(MIN_WINDOW);
        let mut cmd = self.ff.command();
        cmd.args(ffmpeg_args(&plan, window)).current_dir(&live.dir);
        cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| ApiError::Failed(format!("FFmpeg didn't start: {e}")))?;
        let mut stdin = child.stdin.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let tail = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let tail_task = {
            let tail = tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut tail = tail.lock();
                    if tail.len() == STDERR_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            })
        };
        let watcher = tokio::spawn(watch_playlist(live.clone()));

        let started = Instant::now();
        let mut broke = None;
        'feed: {
            for chunk in head {
                if stdin.write_all(&chunk).await.is_err() {
                    break 'feed;
                }
            }
            if ended {
                break 'feed;
            }
            loop {
                let next = tokio::select! {
                    next = tokio::time::timeout(INPUT_STALL, input.next()) => next,
                    () = live.stop.cancelled() => break,
                };
                match next {
                    Ok(Some(Ok(chunk))) => {
                        live.bytes_in.fetch_add(chunk.len() as u64, Relaxed);
                        if stdin.write_all(&chunk).await.is_err() {
                            break;
                        }
                    }
                    Ok(Some(Err(e))) => {
                        broke = Some(format!("the channel's stream broke off: {e}"));
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        broke = Some(format!("the channel sent nothing for {} s", INPUT_STALL.as_secs()));
                        break;
                    }
                }
            }
        }
        drop(stdin);
        // FFmpeg finishes the segment it was making once its input ends.
        let status = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
            Ok(status) => status.ok(),
            Err(_) => {
                let _ = child.kill().await;
                None
            }
        };
        let _ = tail_task.await;
        watcher.abort();
        drop(slot);
        let stopped = live.stop.is_cancelled();
        tracing::info!(id = %live.id, secs = started.elapsed().as_secs(), ?status, stopped, "live channel ended");
        if stopped {
            return Ok(());
        }
        if let Some(why) = broke {
            return Err(ApiError::Failed(why));
        }
        match status {
            Some(s) if s.success() => Ok(()),
            _ if live.segments.load(Relaxed) > 0 && started.elapsed() > Duration::from_secs(10) => Ok(()),
            _ => {
                let tail = tail.lock().iter().cloned().collect::<Vec<_>>().join("\n");
                Err(ApiError::Failed(if tail.is_empty() {
                    "FFmpeg stopped".into()
                } else {
                    format!("FFmpeg stopped: {tail}")
                }))
            }
        }
    }

    /// Stops channels whose caller went away without stopping them.
    pub fn spawn_reaper(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            loop {
                tick.tick().await;
                let Some(mgr) = weak.upgrade() else { break };
                let idle: Vec<String> = mgr
                    .channels
                    .lock()
                    .values()
                    .filter(|l| l.last_access.lock().elapsed() > IDLE)
                    .map(|l| l.id.clone())
                    .collect();
                for id in idle {
                    tracing::info!(id = %id, "live channel idle: stopped");
                    mgr.remove(&id);
                }
            }
        });
    }

    pub fn shutdown(&self) {
        self.closed.store(true, Relaxed);
        let all: Vec<Arc<Live>> = self.channels.lock().drain().map(|(_, l)| l).collect();
        for live in all {
            live.close();
        }
    }
}

impl Live {
    pub fn touch(&self) {
        *self.last_access.lock() = Instant::now();
    }

    pub fn status(&self) -> LiveStatus {
        let inner = self.inner.lock();
        LiveStatus {
            id: self.id.clone(),
            label: self.request.label.clone(),
            owner: self.request.owner.clone(),
            state: inner.state,
            plan: inner.plan.clone(),
            error: inner.error.clone(),
            created_at_ms: self.created_at_ms,
            bytes_in: self.bytes_in.load(Relaxed),
            segments: self.segments.load(Relaxed),
            lag_ms: self.lag_ms.load(Relaxed),
        }
    }

    fn close(&self) {
        self.stop.cancel();
        self.changed.notify_waiters();
        let dir = self.dir.clone();
        // FFmpeg may still hold a segment open for a moment (Windows).
        tokio::spawn(async move {
            for _ in 0..10 {
                if tokio::fs::remove_dir_all(&dir).await.is_ok() || !dir.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }

    /// The playlist, or a segment's path, once there is one.
    pub async fn file(&self, name: &str) -> ApiResult<LiveFile> {
        self.touch();
        if name == "index.m3u8" {
            let deadline = Instant::now() + PLAYLIST_WAIT;
            loop {
                let notified = self.changed.notified();
                let state = self.inner.lock().state;
                match state {
                    LiveState::Failed => {
                        let why = self.inner.lock().error.clone().unwrap_or_else(|| "the channel failed".into());
                        return Err(ApiError::Failed(why));
                    }
                    LiveState::Ended if self.segments.load(Relaxed) == 0 => {
                        return Err(ApiError::JobNotFound("the channel ended".into()));
                    }
                    _ => {}
                }
                if (self.made_ms.load(Relaxed) >= START_MS || state == LiveState::Ended)
                    && let Ok(text) = tokio::fs::read_to_string(self.dir.join("index.m3u8")).await
                {
                    return Ok(LiveFile::Playlist(text));
                }
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(ApiError::Timeout("the channel's first segments didn't come in time".into()));
                }
                let _ = tokio::time::timeout(left.min(Duration::from_millis(500)), notified).await;
            }
        }
        if !is_segment_name(name) {
            return Err(ApiError::NotFound(format!("no file {name}")));
        }
        let path = self.dir.join(name);
        if tokio::fs::metadata(&path).await.is_ok() {
            Ok(LiveFile::Segment(path))
        } else {
            Err(ApiError::NotFound(format!("segment {name} is gone or not made yet")))
        }
    }
}

/// `s{n}.ts`.
fn is_segment_name(name: &str) -> bool {
    name.strip_prefix('s')
        .and_then(|n| n.strip_suffix(".ts"))
        .is_some_and(|n| !n.is_empty() && n.len() <= 12 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Follows the playlist FFmpeg writes: segments made, how far behind real
/// time they come out.
async fn watch_playlist(live: Arc<Live>) {
    let path = live.dir.join("index.m3u8");
    let mut last = String::new();
    let mut first_at: Option<Instant> = None;
    let mut made_ms = 0f64;
    let mut seen_upto: Option<u64> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        tick.tick().await;
        let Ok(text) = tokio::fs::read_to_string(&path).await else { continue };
        if text == last {
            continue;
        }
        let (sequence, durations) = playlist_segments(&text);
        let upto = sequence + durations.len() as u64;
        for (i, d) in durations.iter().enumerate() {
            if seen_upto.is_none_or(|s| sequence + i as u64 >= s) {
                made_ms += d * 1000.0;
            }
        }
        let first = *first_at.get_or_insert_with(Instant::now);
        seen_upto = Some(upto);
        live.segments.store(upto, Relaxed);
        live.made_ms.store(made_ms as u64, Relaxed);
        // The first segments come in a burst from the probed head; after,
        // a channel that keeps up makes media as fast as time passes.
        let first_ms = durations.first().copied().unwrap_or(0.0) * 1000.0;
        let lag = first.elapsed().as_millis() as f64 - (made_ms - first_ms);
        live.lag_ms.store(lag.max(0.0) as i64, Relaxed);
        last = text;
        live.changed.notify_waiters();
    }
}

/// A live playlist's media sequence and its segments' durations (seconds).
fn playlist_segments(text: &str) -> (u64, Vec<f64>) {
    let mut sequence = 0;
    let mut durations = Vec::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            sequence = n.trim().parse().unwrap_or(0);
        } else if let Some(d) = line.strip_prefix("#EXTINF:") {
            durations.push(d.split(',').next().and_then(|d| d.trim().parse().ok()).unwrap_or(0.0));
        }
    }
    (sequence, durations)
}

/// What the input carries, as far as its head says.
#[derive(Debug, Default, Clone, PartialEq)]
struct Probed {
    video: Option<ProbedStream>,
    audio: Option<ProbedStream>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct ProbedStream {
    index: i64,
    codec: String,
    width: Option<i64>,
    height: Option<i64>,
    frame_rate: Option<f64>,
    interlaced: bool,
    /// 10 bits or more.
    deep: bool,
    channels: Option<i64>,
}

async fn probe(ff: &Ffmpeg, head: &[Bytes]) -> ApiResult<Probed> {
    let mut cmd = ff.probe_command();
    cmd.args(["-v", "error", "-probesize", "5000000", "-analyzeduration", "5000000"]);
    cmd.args(["-show_streams", "-of", "json", "-f", "mpegts", "-i", "pipe:0"]);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| ApiError::Failed(format!("ffprobe didn't start: {e}")))?;
    let mut stdin = child.stdin.take().expect("piped");
    let head: Vec<Bytes> = head.to_vec();
    let writer = tokio::spawn(async move {
        for chunk in head {
            if stdin.write_all(&chunk).await.is_err() {
                break;
            }
        }
    });
    let mut stdout = child.stdout.take().expect("piped");
    let mut out = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(15), stdout.read_to_end(&mut out)).await;
    writer.abort();
    let _ = child.kill().await;
    if read.is_err() {
        return Err(ApiError::Timeout("probing the channel's stream took too long".into()));
    }
    let probed = parse_probe(&out);
    if probed.video.is_none() && probed.audio.is_none() {
        return Err(ApiError::Failed(
            "the channel's stream carries no video or audio FFmpeg reads (is it MPEG-TS?)".into(),
        ));
    }
    Ok(probed)
}

impl Probed {
    /// It says all a plan needs: the picture's size (a keyframe came), or
    /// sound alone with no picture.
    fn complete(&self) -> bool {
        match &self.video {
            Some(v) => v.width.is_some() && v.height.is_some(),
            None => self.audio.is_some(),
        }
    }
}

fn parse_probe(json: &[u8]) -> Probed {
    let value: serde_json::Value = serde_json::from_slice(json).unwrap_or_default();
    let mut probed = Probed::default();
    for s in value["streams"].as_array().into_iter().flatten() {
        let codec = s["codec_name"].as_str().unwrap_or_default().to_owned();
        if codec.is_empty() {
            continue;
        }
        let stream = ProbedStream {
            index: s["index"].as_i64().unwrap_or_default(),
            codec,
            width: s["width"].as_i64().filter(|w| *w > 0),
            height: s["height"].as_i64().filter(|h| *h > 0),
            frame_rate: s["avg_frame_rate"]
                .as_str()
                .and_then(rate)
                .or_else(|| s["r_frame_rate"].as_str().and_then(rate)),
            interlaced: matches!(s["field_order"].as_str(), Some("tt" | "bb" | "tb" | "bt")),
            deep: s["pix_fmt"].as_str().is_some_and(|p| p.contains("10") || p.contains("12")),
            channels: s["channels"].as_i64(),
        };
        match s["codec_type"].as_str() {
            Some("video") if probed.video.is_none() && s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                probed.video = Some(stream)
            }
            Some("audio") if probed.audio.is_none() => probed.audio = Some(stream),
            _ => {}
        }
    }
    probed
}

/// `30000/1001` as frames a second, when it is one.
fn rate(text: &str) -> Option<f64> {
    let (n, d) = text.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d).filter(|r| (1.0..=240.0).contains(r))
}

struct Plan<'a> {
    video: Option<ProbedStream>,
    audio: Option<ProbedStream>,
    /// Transcoding the video with it; `None` copies it (or there's none).
    encoder: Option<&'a EncoderInfo>,
    deinterlace: bool,
    height: Option<i64>,
    kbps: u32,
    status: LivePlan,
}

fn plan<'a>(ff: &'a Ffmpeg, request: &LiveRequest, probed: &Probed) -> ApiResult<Plan<'a>> {
    let video = probed.video.clone();
    let audio = probed.audio.clone();
    let deinterlace = request.deinterlace && video.as_ref().is_some_and(|v| v.interlaced);
    let too_tall = |v: &ProbedStream| request.max_height.zip(v.height).is_some_and(|(max, h)| h > i64::from(max));
    let copy = video
        .as_ref()
        .is_some_and(|v| v.codec == "h264" && !v.deep && !deinterlace && !too_tall(v) && !request.force_transcode);
    let encoder = match &video {
        Some(_) if !copy => Some(live_encoder(ff, request.hwaccel).ok_or_else(|| {
            ApiError::Failed("this channel's video needs a transcode, and no H.264 encoder works".into())
        })?),
        _ => None,
    };
    let height = video.as_ref().and_then(|v| match (v.height, request.max_height) {
        (Some(h), Some(max)) if h > i64::from(max) => Some(i64::from(max)),
        (h, _) => h,
    });
    let width = video.as_ref().and_then(|v| v.width);
    let kbps = request
        .max_bitrate_kbps
        .unwrap_or_else(|| default_video_kbps(width.unwrap_or(1920), height.unwrap_or(1080)))
        .max(500);
    let audio_copy = audio.as_ref().is_some_and(|a| a.codec == "aac");
    let status = LivePlan {
        video_codec: video.as_ref().map(|v| v.codec.clone()),
        width,
        height: video.as_ref().and_then(|v| v.height),
        frame_rate: video.as_ref().and_then(|v| v.frame_rate),
        interlaced: video.as_ref().is_some_and(|v| v.interlaced),
        audio_codec: audio.as_ref().map(|a| a.codec.clone()),
        audio_channels: audio.as_ref().and_then(|a| a.channels),
        video: match (&video, encoder) {
            (None, _) => "none".into(),
            (Some(_), None) => "copy".into(),
            (Some(_), Some(enc)) => enc.name.clone(),
        },
        audio: match &audio {
            None => "none".into(),
            Some(_) if audio_copy => "copy".into(),
            Some(_) => "aac".into(),
        },
    };
    Ok(Plan { video, audio, encoder, deinterlace, height, kbps, status })
}

/// The H.264 encoder a live transcode uses: a hardware one that takes
/// frames from system memory, else x264. (VAAPI and Vulkan want frames
/// uploaded to their device, which file jobs' pipelines do.)
fn live_encoder(ff: &Ffmpeg, accel: HwAccel) -> Option<&EncoderInfo> {
    let takes_system_frames = |e: &&EncoderInfo| {
        !e.hardware || matches!(e.accel, HwAccel::Nvenc | HwAccel::Qsv | HwAccel::Amf | HwAccel::Videotoolbox)
    };
    let (picked, _) = ff.pick_encoder(VideoCodec::H264, accel);
    picked.filter(takes_system_frames).or_else(|| ff.software_encoder_for(VideoCodec::H264, false))
}

fn ffmpeg_args(plan: &Plan, window: u32) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    push(&["-loglevel", "warning", "-fflags", "+genpts+discardcorrupt", "-err_detect", "ignore_err"]);
    // The input was probed already: FFmpeg needn't look far into it again.
    push(&["-probesize", "1000000", "-analyzeduration", "1000000", "-f", "mpegts", "-i", "pipe:0"]);
    if let Some(v) = &plan.video {
        a.extend(["-map".into(), format!("0:{}", v.index)]);
    }
    if let Some(au) = &plan.audio {
        a.extend(["-map".into(), format!("0:{}", au.index)]);
    }
    if plan.video.is_some() {
        match plan.encoder {
            None => a.extend(["-c:v", "copy"].map(String::from)),
            Some(enc) => a.extend(video_args(plan, enc)),
        }
    }
    if let Some(au) = &plan.audio {
        if au.codec == "aac" {
            a.extend(["-c:a", "copy"].map(String::from));
        } else {
            a.extend(["-c:a", "aac", "-ac", "2", "-b:a", "192k"].map(String::from));
        }
    }
    let list_size = (window / SEGMENT_SECONDS).max(3).to_string();
    a.extend(
        [
            "-max_muxing_queue_size",
            "2048",
            "-f",
            "hls",
            "-hls_time",
            &SEGMENT_SECONDS.to_string(),
            "-hls_list_size",
            &list_size,
            "-hls_delete_threshold",
            "3",
            "-hls_flags",
            "delete_segments+independent_segments+omit_endlist+temp_file",
            "-hls_segment_type",
            "mpegts",
            "-start_number",
            "0",
            "-hls_segment_filename",
            "s%d.ts",
            "index.m3u8",
        ]
        .map(String::from),
    );
    a
}

fn video_args(plan: &Plan, enc: &EncoderInfo) -> Vec<String> {
    let v = plan.video.as_ref().expect("video");
    let fps_in = v.frame_rate.unwrap_or(25.0);
    // Both fields, each a frame, where a hardware encoder makes them: motion
    // (sport) stays smooth. x264 makes one frame of the two, to keep up.
    let double = plan.deinterlace && enc.hardware;
    let fps = if double { fps_in * 2.0 } else { fps_in };
    let mut filters: Vec<String> = Vec::new();
    if plan.deinterlace {
        let mode = if double { "send_field" } else { "send_frame" };
        filters.push(format!("yadif=mode={mode}:deint=interlaced"));
    }
    if let (Some(h), Some(src)) = (plan.height, v.height)
        && h < src
    {
        filters.push(format!("scale=-2:{h}"));
    }
    filters.push(if enc.hardware { "format=nv12".into() } else { "format=yuv420p".into() });
    let br = plan.kbps;
    let gop = ((fps * f64::from(SEGMENT_SECONDS)).round() as u32).max(1).to_string();
    let mut s: Vec<String> = vec!["-vf".into(), filters.join(","), "-c:v".into(), enc.name.clone()];
    let mut push = |items: &[&str]| s.extend(items.iter().map(|x| x.to_string()));
    let (b, max, buf) = (format!("{br}k"), format!("{br}k"), format!("{}k", br * 2));
    match enc.accel {
        HwAccel::Nvenc => {
            push(&["-preset", "p4", "-tune", "ll", "-rc", "vbr", "-cq", "23", "-b:v", &b, "-maxrate", &max]);
            push(&["-bufsize", &buf, "-forced-idr", "1", "-g", &gop, "-profile:v", "high"]);
        }
        HwAccel::Qsv => {
            let max = (u64::from(br) * 1000 + 1).to_string();
            push(&["-preset", "veryfast", "-look_ahead", "0", "-forced_idr", "1", "-g", &gop, "-mbbrc", "1"]);
            push(&["-b:v", &b, "-maxrate", &max, "-bufsize", &buf, "-profile:v", "high"]);
        }
        HwAccel::Amf => {
            push(&["-usage", "lowlatency", "-quality", "balanced", "-rc", "vbr_peak", "-forced_idr", "1"]);
            push(&["-g", &gop, "-b:v", &b, "-maxrate", &max, "-bufsize", &buf, "-profile:v", "high"]);
        }
        HwAccel::Videotoolbox => {
            push(&["-b:v", &b, "-maxrate", &max, "-bufsize", &buf, "-allow_sw", "1", "-realtime", "1"]);
            push(&["-g", &gop, "-profile:v", "high"]);
        }
        _ => {
            push(&["-preset", "veryfast", "-tune", "zerolatency", "-crf", "21", "-maxrate", &max, "-bufsize", &buf]);
            push(&["-profile:v", "high", "-sc_threshold", "0", "-g", &gop]);
        }
    }
    // A keyframe each segment, whatever the encoder decides.
    s.extend(["-force_key_frames".into(), format!("expr:gte(t,n_forced*{SEGMENT_SECONDS})")]);
    s
}

fn error_text(e: &ApiError) -> String {
    match e {
        ApiError::BadRequest(m)
        | ApiError::Forbidden(m)
        | ApiError::NotFound(m)
        | ApiError::JobNotFound(m)
        | ApiError::Busy(m)
        | ApiError::Timeout(m)
        | ApiError::Failed(m) => m.clone(),
        ApiError::Unauthorized => "unauthorized".into(),
        ApiError::Internal(e) => format!("{e:#}"),
    }
}

/// A segment's bytes for a player.
pub async fn open_segment(path: &Path) -> ApiResult<tokio::fs::File> {
    Ok(tokio::fs::File::open(path).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(codec: &str, height: i64, interlaced: bool) -> ProbedStream {
        ProbedStream {
            index: 0,
            codec: codec.into(),
            width: Some(height * 16 / 9),
            height: Some(height),
            frame_rate: Some(25.0),
            interlaced,
            ..Default::default()
        }
    }

    #[test]
    fn segment_names_are_the_ones_ffmpeg_makes() {
        assert!(is_segment_name("s0.ts"));
        assert!(is_segment_name("s123.ts"));
        assert!(!is_segment_name("s.ts"));
        assert!(!is_segment_name("../index.m3u8"));
        assert!(!is_segment_name("s1.ts/../../x"));
        assert!(!is_segment_name("x1.ts"));
    }

    #[test]
    fn playlists_say_their_segments() {
        let text = "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:41\n\
                    #EXTINF:2.000000,\ns41.ts\n#EXTINF:1.960000,\ns42.ts\n";
        assert_eq!(playlist_segments(text), (41, vec![2.0, 1.96]));
    }

    #[test]
    fn probes_find_the_first_video_and_audio() {
        let json = br#"{"streams":[
            {"index":0,"codec_type":"video","codec_name":"mpeg2video","width":1920,"height":1080,
             "avg_frame_rate":"25/1","field_order":"tt","pix_fmt":"yuv420p"},
            {"index":1,"codec_type":"audio","codec_name":"mp2","channels":2},
            {"index":2,"codec_type":"audio","codec_name":"ac3","channels":6},
            {"index":3,"codec_type":"subtitle","codec_name":"dvb_teletext"}]}"#;
        let p = parse_probe(json);
        let v = p.video.unwrap();
        assert_eq!(
            (v.codec.as_str(), v.height, v.interlaced, v.frame_rate),
            ("mpeg2video", Some(1080), true, Some(25.0))
        );
        assert_eq!(p.audio.unwrap().codec, "mp2");
        assert_eq!(rate("30000/1001").map(|r| (r * 100.0).round()), Some(2997.0));
        assert_eq!(rate("0/0"), None);
    }

    #[test]
    fn hls_output_is_a_sliding_window_of_short_segments() {
        let plan = Plan {
            video: Some(stream("h264", 720, false)),
            audio: Some(ProbedStream { index: 1, codec: "mp2".into(), ..Default::default() }),
            encoder: None,
            deinterlace: false,
            height: Some(720),
            kbps: 4000,
            status: LivePlan::default(),
        };
        let line = ffmpeg_args(&plan, 300).join(" ");
        assert!(line.contains("-i pipe:0"), "{line}");
        assert!(line.contains("-map 0:0 -map 0:1 -c:v copy -c:a aac -ac 2"), "{line}");
        assert!(line.contains("-hls_time 1 -hls_list_size 300"), "{line}");
        assert!(line.contains("delete_segments") && line.contains("omit_endlist"), "{line}");
        assert!(line.ends_with("-hls_segment_filename s%d.ts index.m3u8"), "{line}");
    }

    #[test]
    fn interlaced_video_is_deinterlaced_at_double_rate_on_hardware() {
        let nvenc = EncoderInfo {
            name: "h264_nvenc".into(),
            codec: VideoCodec::H264,
            hardware: true,
            accel: HwAccel::Nvenc,
            main10: false,
        };
        let x264 = EncoderInfo {
            name: "libx264".into(),
            codec: VideoCodec::H264,
            hardware: false,
            accel: HwAccel::None,
            main10: false,
        };
        let mut plan = Plan {
            video: Some(stream("mpeg2video", 1080, true)),
            audio: None,
            encoder: Some(&nvenc),
            deinterlace: true,
            height: Some(720),
            kbps: 4000,
            status: LivePlan::default(),
        };
        let line = video_args(&plan, &nvenc).join(" ");
        assert!(line.contains("yadif=mode=send_field:deint=interlaced,scale=-2:720,format=nv12"), "{line}");
        assert!(line.contains("-g 50"), "{line}");
        assert!(line.contains("-force_key_frames expr:gte(t,n_forced*1)"), "{line}");
        plan.encoder = Some(&x264);
        let line = video_args(&plan, &x264).join(" ");
        assert!(line.contains("yadif=mode=send_frame") && line.contains("-g 25") && line.contains("libx264"), "{line}");
    }
}
