//! Client for the pley transcoder (a separate program; see docs/TRANSCODER.md).
//!
//! The server never links or runs FFmpeg itself. In *managed* mode it spawns
//! the `pley-transcoder` executable (found next to the server binary unless
//! configured), reads the announced address, supervises it (restarting with
//! backoff) and forwards its log output. In *external* mode it talks to a
//! transcoder running elsewhere (`transcoder_url`).

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::http::HeaderValue;
use bytes::Bytes;
use parking_lot::{Mutex, RwLock};
use pley_transcoder_api::{
    Announce, BitrateChanged, BitrateRequest, ConversionStatus, CoverRequest, ErrorBody, Fingerprint,
    FingerprintRequest, Frames, FramesRequest, HoldRequest, JobCreated, JobRequest, JobStatus, KeyframesRequest,
    LiveCreated, LiveRequest, LiveStatus, MediaInfo, PrefetchRequest, Prefetched, ProbeRequest,
    SUBTITLES_COMPLETE_HEADER, SubtitleRequest, TOKEN_ENV, ThumbnailRequest, TranscoderInfo, TranscoderStatus,
    TrickplayInfo, TrickplayRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Notify;

use crate::config::Config;
use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum TranscoderError {
    #[error("the transcoder is not available{}", .0.as_deref().map(|e| format!(": {e}")).unwrap_or_default())]
    Unavailable(Option<String>),
    #[error("{message}")]
    Api { status: u16, code: String, message: String },
    #[error("transcoder request failed: {0}")]
    Http(#[from] reqwest::Error),
}

impl TranscoderError {
    /// The transcoder no longer knows the job (it restarted or expired the
    /// job). Other 404s (segment out of range, missing input) are not this.
    pub fn is_job_gone(&self) -> bool {
        matches!(self, TranscoderError::Api { status: 404, code, .. } if code == "job_not_found")
    }
}

impl From<TranscoderError> for AppError {
    fn from(err: TranscoderError) -> Self {
        match err {
            TranscoderError::Unavailable(_) => AppError::Unavailable(err.to_string()),
            TranscoderError::Api { status: 404, message, .. } => AppError::NotFound(message),
            TranscoderError::Api { status: 400, message, .. } => AppError::BadRequest(message),
            TranscoderError::Api { status: 503, message, .. } => AppError::Unavailable(message),
            TranscoderError::Api { status: 504, message, .. } => AppError::Unavailable(message),
            // Not running, or not answering: a moment's trouble, not a bug.
            TranscoderError::Http(e) if e.is_connect() || e.is_timeout() => {
                AppError::Unavailable("the transcoder is not reachable".into())
            }
            other => AppError::Internal(anyhow::anyhow!(other.to_string())),
        }
    }
}

#[derive(Default)]
struct Node {
    base: Option<String>,
    info: Option<TranscoderInfo>,
    error: Option<String>,
}

pub struct Transcoder {
    http: reqwest::Client,
    token: String,
    managed: Option<ManagedSpec>,
    node: RwLock<Node>,
    ready: Notify,
    started: AtomicBool,
    stopping: AtomicBool,
    stop: Notify,
    supervisor: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// When each file was last read ahead (see [`Transcoder::prefetch_soon`]).
    prefetched: Mutex<HashMap<String, Instant>>,
}

/// A file is read ahead of a playback at most this often.
const PREFETCH_EVERY: Duration = Duration::from_secs(120);
/// Files remembered as read ahead before the oldest are forgotten.
const PREFETCHES_KEPT: usize = 512;
/// The transcoder's last lines on its error output kept, to say why it
/// went (an error report with its causes takes a few).
const LAST_WORDS: usize = 12;
/// How often background work waiting for transcodes asks again.
pub const TRANSCODING_LOOK: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct ManagedSpec {
    binary: PathBuf,
    args: Vec<OsString>,
}

pub type TranscoderResult<T> = Result<T, TranscoderError>;

impl Transcoder {
    pub fn new(config: &Config) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(90))
            .no_proxy()
            .build()
            .expect("http client");
        let (token, managed, base) = match &config.transcoder_url {
            Some(url) => {
                let token = config.transcoder_token.clone().unwrap_or_default();
                if token.is_empty() {
                    tracing::warn!(
                        "the transcoder at {url} is used without a token: anyone who can reach it can use it \
                         (set transcoder_token here and {TOKEN_ENV} there)"
                    );
                }
                (token, None, Some(url.trim_end_matches('/').to_owned()))
            }
            None => {
                let token = crate::auth::new_token();
                let binary = config.transcoder_path.clone().unwrap_or_else(default_transcoder_path);
                let args: Vec<OsString> = vec![
                    "--listen".into(),
                    "127.0.0.1:0".into(),
                    "--announce".into(),
                    "--exit-with-parent".into(),
                    "--work-dir".into(),
                    config.transcode_dir.clone().into_os_string(),
                    "--ffmpeg".into(),
                    config.ffmpeg.clone().into_os_string(),
                    "--ffprobe".into(),
                    config.ffprobe.clone().into_os_string(),
                    "--max-jobs".into(),
                    config.transcoder_max_jobs.to_string().into(),
                ];
                (token, Some(ManagedSpec { binary, args }), None)
            }
        };
        Arc::new(Self {
            http,
            token,
            managed,
            node: RwLock::new(Node { base, ..Default::default() }),
            ready: Notify::new(),
            started: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            stop: Notify::new(),
            supervisor: Mutex::new(None),
            prefetched: Mutex::default(),
        })
    }

    pub fn is_managed(&self) -> bool {
        self.managed.is_some()
    }

    pub fn start(self: &Arc<Self>) {
        self.started.store(true, Ordering::SeqCst);
        let this = self.clone();
        let task = if let Some(spec) = self.managed.clone() {
            tokio::spawn(async move { this.supervise(spec).await })
        } else {
            tokio::spawn(async move { this.watch_external().await })
        };
        *self.supervisor.lock() = Some(task);
    }

    /// Stop supervising; a managed transcoder is asked to exit (it stops its
    /// ffmpeg processes first) and killed only if it does not.
    pub async fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.stop.notify_waiters();
        let task = self.supervisor.lock().take();
        if let Some(task) = task
            && tokio::time::timeout(Duration::from_secs(15), task).await.is_err()
        {
            tracing::warn!("the transcoder supervisor did not stop in time");
        }
    }

    pub fn info(&self) -> Option<TranscoderInfo> {
        self.node.read().info.clone()
    }

    pub fn base_url(&self) -> Option<String> {
        self.node.read().base.clone()
    }

    pub fn last_error(&self) -> Option<String> {
        self.node.read().error.clone()
    }

    pub fn is_available(&self) -> bool {
        self.node.read().info.is_some()
    }

    /// Wait until the transcoder answered `/v1/info` (or the timeout elapses).
    /// Returns immediately when it was never started or failed permanently.
    pub async fn wait_ready(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let notified = self.ready.notified();
            if self.is_available() {
                return true;
            }
            if !self.started.load(Ordering::SeqCst) || self.last_error().is_some_and(|e| e.contains("not found")) {
                return false;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let _ = tokio::time::timeout(deadline - now, notified).await;
        }
    }

    // ---------------------------------------------------------------- API

    pub async fn probe(&self, path: &str) -> TranscoderResult<MediaInfo> {
        self.post_json("/v1/probe", &ProbeRequest { path: path.to_owned() }, Duration::from_secs(150)).await
    }

    pub async fn thumbnail(&self, req: &ThumbnailRequest) -> TranscoderResult<Bytes> {
        let resp = self.post("/v1/thumbnail", req, Duration::from_secs(90)).await?;
        Ok(resp.bytes().await?)
    }

    pub async fn cover(&self, path: &str) -> TranscoderResult<(Bytes, String)> {
        let resp = self.post("/v1/cover", &CoverRequest { path: path.to_owned() }, Duration::from_secs(60)).await?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_owned();
        Ok((resp.bytes().await?, content_type))
    }

    /// A subtitle track as WebVTT, whole: a subtitle file (`stream_index`
    /// `None`), converted in seconds, or a track of the file. A Matroska
    /// file's usually come in seconds too, read through its Cues; others may
    /// need a pass over the whole file, which the transcoder lets run as long
    /// as the file's size calls for (two hours for 70 GB on a slow share):
    /// the wait for those only ends for a transcoder that stopped answering.
    pub async fn subtitles(&self, path: &str, stream_index: Option<i64>) -> TranscoderResult<String> {
        let req = SubtitleRequest { path: path.to_owned(), stream_index, partial: false };
        // A file's conversion ends within the 30 minutes the API gives a request.
        let wait = if stream_index.is_some() { Duration::from_secs(12 * 3600) } else { Duration::from_secs(25 * 60) };
        let resp = self.post("/v1/subtitles", &req, wait).await?;
        Ok(resp.text().await?)
    }

    /// A subtitle track (`stream_index`, or the subtitle file at `path`)
    /// copied into a Matroska file of its own, for players that show
    /// picture subtitles next to an HLS stream. Copying a track out reads
    /// the whole file: it may take as long as a text track's conversion.
    pub async fn subtitle_matroska(&self, path: &str, stream_index: Option<i64>) -> TranscoderResult<Vec<u8>> {
        let req = SubtitleRequest { path: path.to_owned(), stream_index, partial: false };
        let resp = self.post("/v1/subtitles/mks", &req, Duration::from_secs(12 * 3600)).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Whether the transcoder copies subtitle tracks into files of their
    /// own ([`Transcoder::subtitle_matroska`]); older ones don't.
    pub fn copies_subtitles(&self) -> bool {
        self.info().is_some_and(|info| info.capabilities.subtitle_matroska)
    }

    /// The version of the seek previews the transcoder makes
    /// ([`TrickplayInfo::version`]): 0 from older ones, or before it answered.
    pub fn trickplay_version(&self) -> u32 {
        self.info().map_or(0, |info| info.capabilities.trickplay_version)
    }

    /// The seek-preview sets the transcoder makes at once: 1 from older
    /// ones, or before it answered.
    pub fn trickplay_runs(&self) -> u32 {
        self.info().map_or(1, |info| info.capabilities.trickplay_runs.max(1))
    }

    /// Whether the transcoder answers for a picture track's Matroska with
    /// what it has copied so far ([`Transcoder::subtitle_matroska_so_far`]);
    /// older ones answer only once it is whole.
    pub fn copies_subtitles_partially(&self) -> bool {
        self.info().is_some_and(|info| info.capabilities.subtitle_matroska_partial)
    }

    /// A picture track's Matroska ([`Transcoder::subtitle_matroska`]) as far
    /// as it has been copied, and whether that is all of it: the transcoder
    /// goes on copying after answering. It answers within seconds (half a
    /// minute while the copy waits for a playback starting, or for a
    /// sleeping disk), always with a Matroska file players read.
    pub async fn subtitle_matroska_so_far(
        &self,
        path: &str,
        stream_index: Option<i64>,
    ) -> TranscoderResult<(Bytes, bool)> {
        let req = SubtitleRequest { path: path.to_owned(), stream_index, partial: true };
        let resp = self.post("/v1/subtitles/mks", &req, Duration::from_secs(90)).await?;
        let complete = resp.headers().get(SUBTITLES_COMPLETE_HEADER).is_none_or(|v| v.as_bytes() != b"0");
        Ok((resp.bytes().await?, complete))
    }

    /// An embedded track's subtitles as far as they have been pulled out of
    /// the file, and whether that is all of them (the transcoder goes on
    /// pulling them out after answering). The transcoder answers within a
    /// few seconds: with all of them when they are out by then (as a rule
    /// for a Matroska file, read through its Cues), else with those found so
    /// far.
    pub async fn subtitles_so_far(&self, path: &str, stream_index: i64) -> TranscoderResult<(String, bool)> {
        let req = SubtitleRequest { path: path.to_owned(), stream_index: Some(stream_index), partial: true };
        let resp = self.post("/v1/subtitles", &req, Duration::from_secs(60)).await?;
        // A transcoder from before partial answers only answers when done.
        let complete = resp.headers().get(SUBTITLES_COMPLETE_HEADER).is_none_or(|v| v.as_bytes() != b"0");
        Ok((resp.text().await?, complete))
    }

    pub async fn request_keyframes(&self, path: &str) -> TranscoderResult<()> {
        self.post("/v1/keyframes", &KeyframesRequest { path: path.to_owned() }, Duration::from_secs(30)).await?;
        Ok(())
    }

    /// Read the parts of a file a playback starting at `start_ms` reads first,
    /// so they come from a cache when it does (see docs/TRANSCODER.md). The
    /// transcoder gives up after 10 s, and yields to playbacks.
    /// Keep the transcoder's background work (seek previews, fingerprints,
    /// keyframe scans, probes) waiting for `span`: a direct play is starting,
    /// which the transcoder doesn't see. Transcoders before `/v1/hold` answer
    /// `404`, which changes nothing.
    pub async fn hold_background(&self, span: Duration) -> TranscoderResult<()> {
        let req = HoldRequest { ms: span.as_millis() as u64 };
        self.post("/v1/hold", &req, Duration::from_secs(5)).await?;
        Ok(())
    }

    pub async fn prefetch(&self, path: &str, start_ms: i64) -> TranscoderResult<Prefetched> {
        let req = PrefetchRequest { path: path.to_owned(), start_ms: Some(start_ms.max(0)) };
        self.post_json("/v1/prefetch", &req, Duration::from_secs(20)).await
    }

    /// [`Transcoder::prefetch`] in the background, unless the file was read
    /// ahead in the last [`PREFETCH_EVERY`] (or the transcoder isn't there):
    /// for when someone looks like they are about to play it.
    pub fn prefetch_soon(self: &Arc<Self>, path: &str, start_ms: i64) {
        if !self.is_available() || !self.note_prefetch(path, Instant::now()) {
            return;
        }
        let (this, path) = (self.clone(), path.to_owned());
        tokio::spawn(async move {
            match this.prefetch(&path, start_ms).await {
                Ok(done) => tracing::debug!(
                    "read ahead {path} at {start_ms} ms: {} bytes in {} ms{}",
                    done.bytes,
                    done.elapsed_ms,
                    done.skipped.map(|why| format!(" (skipped: {why})")).unwrap_or_default()
                ),
                Err(err) => tracing::debug!("reading ahead {path}: {err}"),
            }
        });
    }

    /// Whether `path` is due a prefetch at `now`, noting it if so.
    fn note_prefetch(&self, path: &str, now: Instant) -> bool {
        let mut done = self.prefetched.lock();
        if done.get(path).is_some_and(|at| now.duration_since(*at) < PREFETCH_EVERY) {
            return false;
        }
        if done.len() >= PREFETCHES_KEPT {
            done.retain(|_, at| now.duration_since(*at) < PREFETCH_EVERY);
            if done.len() >= PREFETCHES_KEPT {
                done.clear();
            }
        }
        done.insert(path.to_owned(), now);
        true
    }

    /// Chromaprint fingerprint of part of a file (intro/credits detection).
    pub async fn fingerprint(&self, req: &FingerprintRequest) -> TranscoderResult<Fingerprint> {
        self.post_json("/v1/fingerprint", req, Duration::from_secs(15 * 60)).await
    }

    /// Darkness and contrast of the key frames of part of a video (credits detection).
    pub async fn frames(&self, req: &FramesRequest) -> TranscoderResult<Frames> {
        self.post_json("/v1/frames", req, Duration::from_secs(15 * 60)).await
    }

    /// Make (or look up) the seek previews of a video; can take minutes, and
    /// hours paused while something is transcoded. Transcoders that make
    /// several at once bound a set by its file's size and keep a paused
    /// one's work; older ones give a set up after an hour, and lose a paused
    /// one's folder to their clean-up after three.
    pub async fn trickplay(&self, path: &str) -> TranscoderResult<TrickplayInfo> {
        let req = TrickplayRequest { path: path.to_owned(), ..Default::default() };
        let bounded = self.info().is_some_and(|info| info.capabilities.trickplay_runs > 0);
        let wait = Duration::from_secs(if bounded { 12 * 3600 } else { 3600 });
        self.post_json("/v1/trickplay", &req, wait).await
    }

    pub async fn trickplay_sheet(&self, id: &str, n: u32) -> TranscoderResult<Bytes> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.get(format!("{base}/v1/trickplay/{id}/{n}.jpg")))
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        Ok(resp.bytes().await?)
    }

    /// A new job. The transcoder may probe the file first, and pull a text
    /// track out of it to burn in, which it gives up on after 20 minutes:
    /// the wait here is longer, so its answer comes through.
    pub async fn create_job(&self, req: &JobRequest) -> TranscoderResult<JobCreated> {
        self.post_json("/v1/jobs", req, Duration::from_secs(25 * 60)).await
    }

    /// Starts making `req` into one file (a converted download), or finds
    /// the conversion made of it: transcoders with `conversions`.
    pub async fn create_conversion(&self, req: &JobRequest) -> TranscoderResult<ConversionStatus> {
        self.post_json("/v1/conversions", req, Duration::from_secs(10 * 60)).await
    }

    pub async fn conversion(&self, id: &str) -> TranscoderResult<ConversionStatus> {
        self.get_json(&format!("/v1/conversions/{id}"), Duration::from_secs(10)).await
    }

    /// A conversion's file, from the `range` a download asks for when it
    /// goes on (the caller forwards the body). A range past its end answers
    /// as the transcoder does (416).
    pub async fn conversion_file(&self, id: &str, range: Option<&HeaderValue>) -> TranscoderResult<reqwest::Response> {
        let base = self.base()?;
        let mut req = self.authed(self.http.get(format!("{base}/v1/conversions/{id}/file")));
        if let Some(range) = range {
            req = req.header(reqwest::header::RANGE, range.as_bytes());
        }
        // No overall limit: the file comes as fast as the network takes it.
        let resp = req.send().await?;
        if resp.status().is_success() || resp.status().as_u16() == 416 { Ok(resp) } else { Err(api_error(resp).await) }
    }

    /// Stops a conversion and deletes what it made (gone already is fine).
    pub async fn delete_conversion(&self, id: &str) -> TranscoderResult<()> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.delete(format!("{base}/v1/conversions/{id}")))
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        if resp.status().is_success() || resp.status().as_u16() == 404 { Ok(()) } else { Err(api_error(resp).await) }
    }

    pub async fn delete_job(&self, id: &str) -> TranscoderResult<()> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.delete(format!("{base}/v1/jobs/{id}")))
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        if resp.status().is_success() || resp.status().as_u16() == 404 { Ok(()) } else { Err(api_error(resp).await) }
    }

    pub async fn touch_job(&self, id: &str) -> TranscoderResult<()> {
        self.post(&format!("/v1/jobs/{id}/touch"), &serde_json::json!({}), Duration::from_secs(10)).await?;
        Ok(())
    }

    /// Changes a job's video bitrate as it is sent: the bitrate it goes on at.
    pub async fn set_bitrate(&self, id: &str, kbps: u32) -> TranscoderResult<u32> {
        let changed: BitrateChanged = self
            .post_json(
                &format!("/v1/jobs/{id}/bitrate"),
                &BitrateRequest { max_bitrate_kbps: kbps },
                Duration::from_secs(30),
            )
            .await?;
        Ok(changed.bitrate_kbps)
    }

    /// Whether its jobs' bitrate can change as they are sent.
    pub fn live_bitrate(&self) -> bool {
        self.info().is_some_and(|info| info.capabilities.live_bitrate)
    }

    pub async fn job_status(&self, id: &str) -> TranscoderResult<JobStatus> {
        self.get_json(&format!("/v1/jobs/{id}"), Duration::from_secs(10)).await
    }

    pub async fn status(&self) -> TranscoderResult<TranscoderStatus> {
        self.get_json("/v1/status", Duration::from_secs(10)).await
    }

    /// Whether the transcoder is transcoding now: a playback's FFmpeg or a
    /// download being converted. Background work waits until it isn't (the
    /// transcoder pauses its own meanwhile). `false` when it can't say.
    pub async fn transcoding(&self) -> bool {
        self.is_available() && self.status().await.is_ok_and(|s| s.running_processes > 0)
    }

    /// Resolves once the transcoder isn't transcoding (see [`Transcoder::transcoding`]).
    pub async fn not_transcoding(&self) {
        while self.transcoding().await {
            tokio::time::sleep(TRANSCODING_LOOK).await;
        }
    }

    /// The job's media playlist (`index.m3u8`).
    pub async fn playlist(&self, job_id: &str) -> TranscoderResult<String> {
        self.playlist_file(job_id, "index.m3u8").await
    }

    /// The job's `index.m3u8`, or its `master.m3u8` (multivariant, naming
    /// the codecs; transcoders with `hls_master_playlist`).
    pub async fn playlist_file(&self, job_id: &str, name: &str) -> TranscoderResult<String> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.get(format!("{base}/v1/jobs/{job_id}/{name}")))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        Ok(resp.text().await?)
    }

    /// Streaming response of segment `n` (the caller forwards the body).
    pub async fn segment(&self, job_id: &str, n: u32) -> TranscoderResult<reqwest::Response> {
        self.segment_file(job_id, &format!("{n}.ts")).await
    }

    /// A segment by its name: `12.ts`, or a lower rendition's `v1-12.ts`.
    pub async fn segment_file(&self, job_id: &str, name: &str) -> TranscoderResult<reqwest::Response> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.get(format!("{base}/v1/jobs/{job_id}/{name}")))
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        Ok(resp)
    }

    /// Whether it makes live channels (`/v1/live`).
    pub fn makes_live(&self) -> bool {
        self.info().is_some_and(|info| info.capabilities.live)
    }

    pub async fn create_live(&self, req: &LiveRequest) -> TranscoderResult<LiveCreated> {
        self.post_json("/v1/live", req, Duration::from_secs(20)).await
    }

    /// Sends a live channel's MPEG-TS, `input`, as it comes: answered when
    /// it ends (or the channel is stopped, or fails).
    pub async fn feed_live<S>(&self, id: &str, input: S) -> TranscoderResult<()>
    where
        S: futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    {
        let base = self.base()?;
        // No overall limit: it lasts as long as the channel is watched.
        let resp = self
            .authed(self.http.put(format!("{base}/v1/live/{id}/input")))
            .header(reqwest::header::CONTENT_TYPE, "video/mp2t")
            .body(reqwest::Body::wrap_stream(input))
            .send()
            .await?;
        if resp.status().is_success() { Ok(()) } else { Err(api_error(resp).await) }
    }

    pub async fn live_status(&self, id: &str) -> TranscoderResult<LiveStatus> {
        self.get_json(&format!("/v1/live/{id}"), Duration::from_secs(10)).await
    }

    /// Stops a live channel (gone already is fine).
    pub async fn delete_live(&self, id: &str) -> TranscoderResult<()> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.delete(format!("{base}/v1/live/{id}")))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if resp.status().is_success() || resp.status().as_u16() == 404 { Ok(()) } else { Err(api_error(resp).await) }
    }

    /// A live channel's playlist, which waits for its first segments.
    pub async fn live_playlist(&self, id: &str) -> TranscoderResult<String> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.get(format!("{base}/v1/live/{id}/index.m3u8")))
            .timeout(Duration::from_secs(40))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        Ok(resp.text().await?)
    }

    /// Streaming response of a live channel's segment (`s12.ts`).
    pub async fn live_segment(&self, id: &str, name: &str) -> TranscoderResult<reqwest::Response> {
        let base = self.base()?;
        let resp = self
            .authed(self.http.get(format!("{base}/v1/live/{id}/{name}")))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        if resp.status().is_success() { Ok(resp) } else { Err(api_error(resp).await) }
    }

    fn base(&self) -> TranscoderResult<String> {
        let node = self.node.read();
        node.base.clone().ok_or_else(|| TranscoderError::Unavailable(node.error.clone()))
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.token.is_empty() { req } else { req.bearer_auth(&self.token) }
    }

    async fn post<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> TranscoderResult<reqwest::Response> {
        let base = self.base()?;
        let resp = self.authed(self.http.post(format!("{base}{path}"))).json(body).timeout(timeout).send().await?;
        if resp.status().is_success() { Ok(resp) } else { Err(api_error(resp).await) }
    }

    async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> TranscoderResult<T> {
        Ok(self.post(path, body, timeout).await?.json().await?)
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str, timeout: Duration) -> TranscoderResult<T> {
        let base = self.base()?;
        let resp = self.authed(self.http.get(format!("{base}{path}"))).timeout(timeout).send().await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    async fn fetch_info(&self, base: &str) -> TranscoderResult<TranscoderInfo> {
        let resp =
            self.authed(self.http.get(format!("{base}/v1/info"))).timeout(Duration::from_secs(10)).send().await?;
        if !resp.status().is_success() {
            return Err(api_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    fn set_available(&self, base: String, info: TranscoderInfo) {
        let mut node = self.node.write();
        node.base = Some(base);
        node.info = Some(info);
        node.error = None;
        drop(node);
        self.ready.notify_waiters();
    }

    fn set_unavailable(&self, error: String) {
        let mut node = self.node.write();
        if self.managed.is_some() {
            node.base = None;
        }
        node.info = None;
        node.error = Some(error);
    }

    // ---------------------------------------------------------- supervision

    async fn watch_external(self: Arc<Self>) {
        let Some(base) = self.base_url() else { return };
        loop {
            match self.fetch_info(&base).await {
                Ok(info) => {
                    if !self.is_available() {
                        tracing::info!("transcoder {} at {base} is available", info.version);
                    }
                    self.set_available(base.clone(), info);
                }
                Err(err) => {
                    if self.is_available() || self.last_error().is_none() {
                        tracing::warn!("transcoder at {base} unavailable: {err}");
                    }
                    self.set_unavailable(err.to_string());
                }
            }
            if !self.sleep_unless_stopped(Duration::from_secs(15)).await {
                return;
            }
        }
    }

    async fn supervise(self: Arc<Self>, spec: ManagedSpec) {
        let mut backoff = Duration::from_secs(1);
        while !self.stopping.load(Ordering::SeqCst) {
            if !spec.binary.is_file() {
                let message = format!("transcoder binary not found at {}", spec.binary.display());
                if self.last_error().as_deref() != Some(message.as_str()) {
                    tracing::error!("{message}; transcoding, probing and thumbnails are unavailable");
                }
                self.set_unavailable(message);
                if !self.sleep_unless_stopped(Duration::from_secs(30)).await {
                    return;
                }
                continue;
            }
            let started = Instant::now();
            match self.run_once(&spec).await {
                Ok(()) => return,
                Err(message) => {
                    tracing::warn!("transcoder stopped: {message}");
                    self.set_unavailable(message);
                }
            }
            if self.stopping.load(Ordering::SeqCst) {
                return;
            }
            if started.elapsed() > Duration::from_secs(60) {
                backoff = Duration::from_secs(1);
            }
            tracing::info!("restarting transcoder in {}s", backoff.as_secs());
            if !self.sleep_unless_stopped(backoff).await {
                return;
            }
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }

    /// Sleep for `duration`; `false` when shutdown started (before or during).
    async fn sleep_unless_stopped(&self, duration: Duration) -> bool {
        let stop = self.stop.notified();
        tokio::pin!(stop);
        stop.as_mut().enable();
        if self.stopping.load(Ordering::SeqCst) {
            return false;
        }
        tokio::select! {
            _ = tokio::time::sleep(duration) => true,
            _ = stop => false,
        }
    }

    /// Run the transcoder until it exits (`Err`) or we are shutting down (`Ok`).
    async fn run_once(&self, spec: &ManagedSpec) -> Result<(), String> {
        let mut cmd = tokio::process::Command::new(&spec.binary);
        cmd.args(&spec.args)
            .env(TOKEN_ENV, &self.token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let mut child = cmd.spawn().map_err(|e| format!("failed to start {}: {e}", spec.binary.display()))?;
        // Keep stdin open: the transcoder shuts down cleanly when it closes
        // (we stopped it, or died).
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;

        let said = Arc::new(Mutex::new(VecDeque::new()));
        let reader = tokio::spawn({
            let said = said.clone();
            async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    forward_log(&line);
                    let mut said = said.lock();
                    if said.len() == LAST_WORDS {
                        said.pop_front();
                    }
                    said.push_back(line);
                }
            }
        });
        // What it said last, once it has all been read (it went).
        let why_gone = |reader: tokio::task::JoinHandle<()>| {
            let said = said.clone();
            async move {
                let _ = tokio::time::timeout(Duration::from_secs(2), reader).await;
                last_words(&said.lock())
            }
        };

        let mut stdout_lines = BufReader::new(stdout).lines();
        // Stopping while it starts (probing ffmpeg and the GPU takes seconds)
        // ends it at once: it has nothing to finish yet.
        let stop = self.stop.notified();
        tokio::pin!(stop);
        stop.as_mut().enable();
        if self.stopping.load(Ordering::SeqCst) {
            return stop_starting(&mut child).await;
        }
        let announce = tokio::select! {
            announce = tokio::time::timeout(Duration::from_secs(120), async {
                while let Ok(Some(line)) = stdout_lines.next_line().await {
                    if let Ok(a) = serde_json::from_str::<Announce>(&line)
                        && a.event == "listening" {
                            return Some(a.addr);
                        }
                }
                None
            }) => announce,
            () = &mut stop => return stop_starting(&mut child).await,
        };
        let addr = match announce {
            Ok(Some(addr)) => addr,
            Ok(None) => {
                let status = child.wait().await.map(|s| s.to_string()).unwrap_or_default();
                let why = why_gone(reader).await;
                return Err(format!("transcoder exited before announcing its address ({status}){why}"));
            }
            Err(_) => return Err("transcoder did not start within 120s".into()),
        };
        let base = format!("http://{addr}");
        let info = tokio::select! {
            info = self.fetch_info(&base) => info,
            () = &mut stop => return stop_starting(&mut child).await,
        };
        match info {
            Ok(info) => {
                tracing::info!(
                    "transcoder {} ({}) ready at {base}: ffmpeg {}, encoders [{}]",
                    info.version,
                    info.license,
                    info.capabilities.ffmpeg_version.as_deref().unwrap_or("not found"),
                    info.capabilities.encoders.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
                );
                self.set_available(base, info);
            }
            Err(err) => return Err(format!("transcoder at {base} did not answer: {err}")),
        }
        tokio::spawn(async move { while let Ok(Some(_)) = stdout_lines.next_line().await {} });

        if self.stopping.load(Ordering::SeqCst) {
            return self.stop_child(&mut child, &mut stdin).await;
        }
        tokio::select! {
            status = child.wait() => {
                let status = status.map(|s| s.to_string()).unwrap_or_default();
                Err(format!("transcoder exited ({status}){}", why_gone(reader).await))
            }
            _ = stop => self.stop_child(&mut child, &mut stdin).await,
        }
    }

    /// Close the transcoder's stdin so it stops its jobs and exits; kill it if
    /// it is still running after a grace period.
    async fn stop_child(
        &self,
        child: &mut tokio::process::Child,
        stdin: &mut Option<tokio::process::ChildStdin>,
    ) -> Result<(), String> {
        drop(stdin.take());
        if tokio::time::timeout(Duration::from_secs(10), child.wait()).await.is_err() {
            tracing::warn!("transcoder did not exit within 10s; killing it");
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        }
        Ok(())
    }
}

/// Ends a transcoder that hasn't started serving.
async fn stop_starting(child: &mut tokio::process::Child) -> Result<(), String> {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    Ok(())
}

fn default_transcoder_path() -> PathBuf {
    let name = if cfg!(windows) { "pley-transcoder.exe" } else { "pley-transcoder" };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Re-emit a transcoder log line through our own logger at its level.
fn forward_log(line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let body =
        trimmed.split_once(" INFO ").or_else(|| trimmed.split_once(" WARN ")).or_else(|| trimmed.split_once(" ERROR "));
    let message = body.map(|(_, m)| m.trim()).unwrap_or(trimmed);
    // `Error: …`: why it couldn't start (or went), as `main` reports it.
    if trimmed.contains(" ERROR ") || trimmed.starts_with("Error:") {
        tracing::error!("[transcoder] {message}");
    } else if trimmed.contains(" WARN ") {
        tracing::warn!("[transcoder] {message}");
    } else if trimmed.contains(" DEBUG ") || trimmed.contains(" TRACE ") {
        tracing::debug!("[transcoder] {message}");
    } else {
        tracing::info!("[transcoder] {message}");
    }
}

/// Why a transcoder that went did, from its last lines on its error
/// output, for the error the admin sees: its `Error:` report (with its
/// causes) when it gave one, else its last lines; `""` when it said nothing.
fn last_words(said: &VecDeque<String>) -> String {
    let lines: Vec<&str> = said.iter().map(|line| line.trim()).filter(|line| !line.is_empty()).collect();
    let from = match lines.iter().rposition(|line| line.starts_with("Error:")) {
        Some(report) => report,
        None => lines.len().saturating_sub(3),
    };
    let words = lines[from..].join(" ");
    let words = words.strip_prefix("Error:").map_or(words.as_str(), str::trim_start);
    if words.is_empty() { String::new() } else { format!(": {words}") }
}

async fn api_error(resp: reqwest::Response) -> TranscoderError {
    let status = resp.status().as_u16();
    match resp.json::<ErrorBody>().await {
        Ok(body) => TranscoderError::Api { status, code: body.code, message: body.message },
        Err(_) => {
            TranscoderError::Api { status, code: "http".into(), message: format!("transcoder returned HTTP {status}") }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unreachable_transcoder_is_unavailable_not_an_internal_error() {
        // Nothing listens on a port just freed.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let err = reqwest::get(format!("http://127.0.0.1:{port}/v1/jobs")).await.unwrap_err();
        match AppError::from(TranscoderError::Http(err)) {
            AppError::Unavailable(message) => assert!(!message.contains("127.0.0.1"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_transcoder_that_goes_says_why() {
        let said = |lines: &[&str]| last_words(&lines.iter().map(|l| (*l).to_owned()).collect());
        assert_eq!(
            said(&[
                "2026-10-27T03:00:00Z  INFO pley-transcoder 1.0 starting",
                "Error: the token must be a secret of at least 16 characters, not an example",
                "",
            ]),
            ": the token must be a secret of at least 16 characters, not an example"
        );
        // With its causes.
        assert_eq!(
            said(&["Error: cannot listen", "", "Caused by:", "    Address in use"]),
            ": cannot listen Caused by: Address in use"
        );
        // No report (a crash): its last lines.
        assert_eq!(said(&["one", "two", "three", "four"]), ": two three four");
        assert_eq!(said(&[]), "");
    }

    #[test]
    fn files_are_read_ahead_at_most_every_two_minutes() {
        let transcoder = Transcoder::new(&Config::for_data_dir(std::path::Path::new("unused")));
        let now = Instant::now();
        assert!(transcoder.note_prefetch("a.mkv", now));
        assert!(!transcoder.note_prefetch("a.mkv", now + Duration::from_secs(60)), "read a minute ago");
        assert!(transcoder.note_prefetch("b.mkv", now + Duration::from_secs(60)), "another file");
        assert!(transcoder.note_prefetch("a.mkv", now + PREFETCH_EVERY));
        // Only so many are remembered.
        for n in 0..PREFETCHES_KEPT * 2 {
            assert!(transcoder.note_prefetch(&format!("{n}.mkv"), now));
        }
        assert!(transcoder.prefetched.lock().len() <= PREFETCHES_KEPT);
    }
}
