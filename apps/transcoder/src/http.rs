// SPDX-License-Identifier: AGPL-3.0-only
//! HTTP API of the transcoder (see docs/TRANSCODER.md).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use pley_transcoder_api::{
    BitrateChanged, BitrateRequest, ConversionStatus, CoverRequest, FileContainer, Fingerprint, FingerprintRequest,
    Frames, FramesRequest, HoldRequest, JobCreated, JobRequest, JobStatus, KeyframesRequest, MediaInfo,
    PrefetchRequest, Prefetched, ProbeRequest, SUBTITLES_COMPLETE_HEADER, SubtitleRequest, TOKEN_ENV, ThumbnailRequest,
    TranscoderInfo, TranscoderStatus, TrickplayInfo, TrickplayRequest,
};
use tokio_util::io::ReaderStream;

use crate::convert::Conversions;
use crate::error::{ApiError, ApiResult};
use crate::extract::Extractor;
use crate::fingerprint::Fingerprinter;
use crate::job::{JobFile, JobManager};
use crate::keyframes::KeyframeStore;
use crate::live::{LiveFile, LiveManager};
use crate::metrics::{Metrics, render_prometheus};
use crate::prefetch::Prefetcher;
use crate::probe::{Prober, Rejected};
use crate::trickplay::Trickplay;
use crate::util::dir_size;

pub struct AppState {
    pub jobs: Arc<JobManager>,
    /// Live channels, sharing the jobs' transcode slots.
    pub live: Arc<LiveManager>,
    pub conversions: Arc<Conversions>,
    pub prober: Arc<Prober>,
    pub extractor: Arc<Extractor>,
    pub keyframes: Arc<KeyframeStore>,
    pub fingerprinter: Arc<Fingerprinter>,
    pub trickplay: Arc<Trickplay>,
    pub prefetcher: Prefetcher,
    pub metrics: Arc<Metrics>,
    pub info: TranscoderInfo,
    pub token: Option<String>,
    pub started: Instant,
    pub work_dir: PathBuf,
    /// Size of the work directory, refreshed in the background.
    work_dir_bytes: AtomicU64,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        jobs: Arc<JobManager>,
        conversions: Arc<Conversions>,
        prober: Arc<Prober>,
        extractor: Arc<Extractor>,
        keyframes: Arc<KeyframeStore>,
        fingerprinter: Arc<Fingerprinter>,
        trickplay: Arc<Trickplay>,
        metrics: Arc<Metrics>,
        info: TranscoderInfo,
        token: Option<String>,
        work_dir: PathBuf,
    ) -> Self {
        let live = LiveManager::new(jobs.ffmpeg(), jobs.slots.clone(), work_dir.join("live"));
        live.spawn_reaper();
        Self {
            prefetcher: Prefetcher::new(jobs.clone(), keyframes.clone()),
            live,
            jobs,
            conversions,
            prober,
            extractor,
            keyframes,
            fingerprinter,
            trickplay,
            metrics,
            info,
            token,
            started: Instant::now(),
            work_dir,
            work_dir_bytes: AtomicU64::new(0),
        }
    }

    /// Walk the work directory every few seconds on the blocking pool.
    pub fn spawn_disk_usage(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tick.tick().await;
                let Some(state) = weak.upgrade() else { break };
                let dir = state.work_dir.clone();
                if let Ok(bytes) = tokio::task::spawn_blocking(move || dir_size(&dir)).await {
                    state.work_dir_bytes.store(bytes, Ordering::Relaxed);
                }
            }
        });
    }

    fn status(&self) -> TranscoderStatus {
        let (running, hw) = self.jobs.slots.counts();
        let work_dir_bytes = self.work_dir_bytes.load(Ordering::Relaxed);
        TranscoderStatus {
            version: self.info.version.clone(),
            uptime_secs: self.started.elapsed().as_secs(),
            ffmpeg_version: self.info.capabilities.ffmpeg_version.clone(),
            max_jobs: self.jobs.limits.max_jobs,
            running_processes: running,
            hw_processes: hw,
            max_hw_jobs: self.jobs.limits.max_hw_jobs,
            work_dir_bytes,
            memory_bytes: self.jobs.memory_bytes(),
            background_paused: self.jobs.background.paused() as u32,
            counters: self.metrics.snapshot(),
            jobs: self.jobs.list().iter().map(|j| j.status()).collect(),
            live: self.live.list(),
        }
    }
}

type Shared = State<Arc<AppState>>;

pub fn router(state: Arc<AppState>) -> Router {
    let v1 = Router::new()
        .route("/info", get(info))
        .route("/status", get(status))
        .route("/probe", post(probe))
        .route("/thumbnail", post(thumbnail))
        .route("/cover", post(cover))
        .route("/subtitles", post(subtitles))
        .route("/subtitles/mks", post(subtitles_matroska))
        .route("/keyframes", post(keyframes))
        .route("/prefetch", post(prefetch))
        .route("/hold", post(hold))
        .route("/fingerprint", post(fingerprint))
        .route("/frames", post(frames))
        .route("/trickplay", post(trickplay))
        .route("/trickplay/{id}/{sheet}", get(trickplay_sheet))
        .route("/jobs", get(list_jobs).post(create_job))
        .route("/jobs/{id}", get(get_job).delete(delete_job))
        .route("/jobs/{id}/touch", post(touch_job))
        .route("/jobs/{id}/bitrate", post(job_bitrate))
        .route("/jobs/{id}/{file}", get(job_file))
        .route("/conversions", post(create_conversion))
        .route("/conversions/{id}", get(get_conversion).delete(delete_conversion))
        .route("/conversions/{id}/file", get(conversion_file))
        .route("/live", get(list_live).post(create_live))
        .route("/live/{id}", get(get_live).delete(delete_live))
        .route("/live/{id}/input", axum::routing::put(live_input))
        .route("/live/{id}/{file}", get(live_file));
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/metrics", get(metrics))
        .nest("/v1", v1)
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

async fn auth(State(state): Shared, request: Request, next: Next) -> Response {
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }
    match &state.token {
        Some(expected) => {
            let presented = request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .unwrap_or_default();
            if !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
                return ApiError::Unauthorized.into_response();
            }
        }
        // Without a token (on this computer only, see `main.rs`), only what
        // is addressed to this computer is answered: a web page whose own
        // name a DNS server turned into 127.0.0.1 (DNS rebinding) still
        // sends that name, and must not read files through the transcoder.
        None => {
            if let Some(host) = foreign_host(&request) {
                return ApiError::Forbidden(format!(
                    "without a token this transcoder only answers requests addressed to localhost, 127.0.0.1 \
                     or [::1], not {host}: give it a token ({TOKEN_ENV}) to reach it by another name"
                ))
                .into_response();
            }
        }
    }
    next.run(request).await
}

/// The host `request` is addressed to (its `Host`, and the authority of
/// its target when it has one), said for an error, when that isn't this
/// computer by its loopback name or address.
fn foreign_host(request: &Request) -> Option<String> {
    let host = request.headers().get(header::HOST).map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned());
    let authority = request.uri().authority().map(|a| a.as_str().to_owned());
    if host.is_none() && authority.is_none() {
        return Some("one that names no host".into());
    }
    host.into_iter().chain(authority).find(|name| !is_loopback_host(name)).map(|name| format!("{name:?}"))
}

/// Whether `host` (`name`, `name:port`, `[v6]` or `[v6]:port`) is
/// `localhost` or a loopback address.
fn is_loopback_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(bracketed) => match bracketed.split_once(']') {
            Some((address, port)) if port.is_empty() || port.starts_with(':') => address,
            _ => return false,
        },
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    name.eq_ignore_ascii_case("localhost") || name.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn info(State(state): Shared) -> Json<TranscoderInfo> {
    Json(state.info.clone())
}

async fn status(State(state): Shared) -> Json<TranscoderStatus> {
    Json(state.status())
}

async fn metrics(State(state): Shared) -> impl IntoResponse {
    let body = render_prometheus(&state.status());
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], body)
}

async fn probe(State(state): Shared, Json(req): Json<ProbeRequest>) -> ApiResult<Json<MediaInfo>> {
    let path = std::path::Path::new(&req.path);
    if !path.is_file() {
        return Err(ApiError::NotFound(format!("file not found: {}", req.path)));
    }
    // Library scans ask; jobs probe without asking. A playback starting goes first.
    state.jobs.background.wait_for_starts().await;
    let info = state.prober.probe(path).await.map_err(|e| {
        // Only a file ffprobe could not read is a definitive failure; timeouts
        // (sleeping NAS) and start-up problems are worth retrying later.
        if e.downcast_ref::<Rejected>().is_some() {
            ApiError::Failed(format!("{e:#}"))
        } else if e.downcast_ref::<tokio::time::error::Elapsed>().is_some() {
            ApiError::Timeout(format!("{e:#}"))
        } else {
            ApiError::Internal(e)
        }
    })?;
    Ok(Json((*info).clone()))
}

async fn thumbnail(State(state): Shared, Json(req): Json<ThumbnailRequest>) -> ApiResult<Response> {
    let image = state.extractor.thumbnail(&req).await?;
    Ok(([(header::CONTENT_TYPE, image.content_type)], image.bytes).into_response())
}

async fn cover(State(state): Shared, Json(req): Json<CoverRequest>) -> ApiResult<Response> {
    let image = state.extractor.cover(&req.path).await?;
    Ok(([(header::CONTENT_TYPE, image.content_type)], image.bytes).into_response())
}

async fn subtitles(State(state): Shared, Json(req): Json<SubtitleRequest>) -> ApiResult<Response> {
    if let (true, Some(index)) = (req.partial, req.stream_index) {
        let (text, complete) = state.extractor.subtitle_so_far(&req.path, index, Duration::from_secs(2)).await?;
        let complete = if complete { "1" } else { "0" };
        return Ok((
            [
                (header::CONTENT_TYPE, "text/vtt; charset=utf-8"),
                (HeaderName::from_static(SUBTITLES_COMPLETE_HEADER), complete),
            ],
            text,
        )
            .into_response());
    }
    let text = state.extractor.subtitle(&req.path, req.stream_index).await?;
    Ok(([(header::CONTENT_TYPE, "text/vtt; charset=utf-8")], text).into_response())
}

async fn subtitles_matroska(State(state): Shared, Json(req): Json<SubtitleRequest>) -> ApiResult<Response> {
    if req.partial {
        let (bytes, complete) =
            state.extractor.subtitle_matroska_so_far(&req.path, req.stream_index, Duration::from_secs(2)).await?;
        let complete = if complete { "1" } else { "0" };
        return Ok((
            [
                (header::CONTENT_TYPE, "video/x-matroska"),
                (HeaderName::from_static(SUBTITLES_COMPLETE_HEADER), complete),
            ],
            bytes,
        )
            .into_response());
    }
    let bytes = state.extractor.subtitle_matroska(&req.path, req.stream_index).await?;
    Ok(([(header::CONTENT_TYPE, "video/x-matroska")], bytes).into_response())
}

async fn keyframes(State(state): Shared, Json(req): Json<KeyframesRequest>) -> ApiResult<StatusCode> {
    let path = std::path::Path::new(&req.path);
    if !path.is_file() {
        return Err(ApiError::NotFound(format!("file not found: {}", req.path)));
    }
    if state.keyframes.get(path).is_some() {
        return Ok(StatusCode::OK);
    }
    state.keyframes.request(path);
    Ok(StatusCode::ACCEPTED)
}

async fn prefetch(State(state): Shared, Json(req): Json<PrefetchRequest>) -> ApiResult<Json<Prefetched>> {
    Ok(Json(state.prefetcher.prefetch(&req).await?))
}

async fn hold(State(state): Shared, Json(req): Json<HoldRequest>) -> StatusCode {
    state.jobs.background.hold(Duration::from_millis(req.ms));
    StatusCode::NO_CONTENT
}

async fn trickplay(State(state): Shared, Json(req): Json<TrickplayRequest>) -> ApiResult<Json<TrickplayInfo>> {
    Ok(Json(state.trickplay.generate(&req).await?))
}

async fn trickplay_sheet(State(state): Shared, Path((id, sheet)): Path<(String, String)>) -> ApiResult<Response> {
    let n: u32 = sheet
        .strip_suffix(".jpg")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| ApiError::NotFound(format!("unknown file {sheet}")))?;
    let path = state.trickplay.sheet_path(&id, n).ok_or_else(|| ApiError::NotFound("no such sheet".into()))?;
    let bytes = tokio::fs::read(path).await?;
    // Read whole: the set may go now that this sheet has been fetched.
    let trickplay = state.trickplay.clone();
    let _ = tokio::task::spawn_blocking(move || trickplay.sheet_served(&id, n)).await;
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}

async fn fingerprint(State(state): Shared, Json(req): Json<FingerprintRequest>) -> ApiResult<Json<Fingerprint>> {
    Ok(Json(state.fingerprinter.fingerprint(&req).await?))
}

async fn frames(State(state): Shared, Json(req): Json<FramesRequest>) -> ApiResult<Json<Frames>> {
    Ok(Json(state.fingerprinter.frames(&req).await?))
}

async fn list_jobs(State(state): Shared) -> Json<Vec<JobStatus>> {
    Json(state.jobs.list().iter().map(|j| j.status()).collect())
}

async fn create_job(State(state): Shared, Json(req): Json<JobRequest>) -> ApiResult<(StatusCode, Json<JobCreated>)> {
    let created = state.jobs.create(req).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn get_job(State(state): Shared, Path(id): Path<String>) -> ApiResult<Json<JobStatus>> {
    let job = state.jobs.get(&id).ok_or_else(|| ApiError::JobNotFound(format!("job {id} not found")))?;
    Ok(Json(job.status()))
}

async fn delete_job(State(state): Shared, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if state.jobs.remove(&id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::JobNotFound(format!("job {id} not found")))
    }
}

async fn create_conversion(State(state): Shared, Json(req): Json<JobRequest>) -> ApiResult<Json<ConversionStatus>> {
    Ok(Json(state.conversions.create(req).await?))
}

async fn get_conversion(State(state): Shared, Path(id): Path<String>) -> ApiResult<Json<ConversionStatus>> {
    state.conversions.status(&id).map(Json).ok_or_else(|| ApiError::NotFound(format!("conversion {id} not found")))
}

async fn delete_conversion(State(state): Shared, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if state.conversions.remove(&id).await {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("conversion {id} not found")))
    }
}

/// A conversion's file once whole, with ranges: a download cut short goes on.
async fn conversion_file(State(state): Shared, Path(id): Path<String>, headers: HeaderMap) -> ApiResult<Response> {
    let path = state.conversions.file(&id).ok_or_else(|| ApiError::NotFound(format!("conversion {id} has no file")))?;
    let container =
        if path.extension().is_some_and(|e| e == "mp4") { FileContainer::Mp4 } else { FileContainer::Matroska };
    serve_range(&path, &headers, container.content_type()).await
}

/// `path` whole (200), or the one range asked for (206, `bytes=a-` or
/// `bytes=a-b`); 416 for a range past its end.
async fn serve_range(path: &std::path::Path, headers: &HeaderMap, content_type: &'static str) -> ApiResult<Response> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = tokio::fs::File::open(path).await?;
    let total = file.metadata().await?.len();
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| parse_range(v, total));
    let builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-cache");
    let response = match range {
        None => builder
            .header(header::CONTENT_LENGTH, total)
            .body(Body::from_stream(ReaderStream::with_capacity(file, 256 * 1024))),
        Some(None) => builder
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Body::empty()),
        Some(Some((start, end))) => {
            file.seek(std::io::SeekFrom::Start(start)).await?;
            let len = end - start + 1;
            builder
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_LENGTH, len)
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"))
                .body(Body::from_stream(ReaderStream::with_capacity(file.take(len), 256 * 1024)))
        }
    };
    response.map_err(|e| ApiError::Internal(e.into()))
}

/// `Range: bytes=a-` or `bytes=a-b` of a file of `total` bytes: `None` for
/// none (or one this doesn't serve, several ranges: the whole file then),
/// `Some(None)` past the end, else the inclusive byte range.
fn parse_range(value: &str, total: u64) -> Option<Option<(u64, u64)>> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (first, last) = spec.split_once('-')?;
    let start: u64 = first.trim().parse().ok()?;
    if start >= total {
        return Some(None);
    }
    let end = match last.trim() {
        "" => total - 1,
        last => last.parse::<u64>().ok()?.min(total - 1),
    };
    if end < start {
        return Some(None);
    }
    Some(Some((start, end)))
}

async fn touch_job(State(state): Shared, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let job = state.jobs.get(&id).ok_or_else(|| ApiError::JobNotFound(format!("job {id} not found")))?;
    job.touch();
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/jobs/{id}/bitrate`: see [`JobManager::set_bitrate`].
async fn job_bitrate(
    State(state): Shared,
    Path(id): Path<String>,
    Json(req): Json<BitrateRequest>,
) -> ApiResult<Json<BitrateChanged>> {
    let job = state.jobs.get(&id).ok_or_else(|| ApiError::JobNotFound(format!("job {id} not found")))?;
    let bitrate_kbps = state.jobs.set_bitrate(&job, req.max_bitrate_kbps).await?;
    Ok(Json(BitrateChanged { bitrate_kbps }))
}

async fn job_file(State(state): Shared, Path((id, file)): Path<(String, String)>) -> ApiResult<Response> {
    let job = state.jobs.get(&id).ok_or_else(|| ApiError::JobNotFound(format!("job {id} not found")))?;
    let unknown = || ApiError::NotFound(format!("unknown file {file}"));
    let (k, n) = match JobFile::parse(&file).ok_or_else(unknown)? {
        JobFile::Segment(k, n) => (k, n),
        playlist => {
            job.touch();
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/vnd.apple.mpegurl"));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            let playlist = match playlist {
                JobFile::Playlist(k) => job.renditions().get(k).ok_or_else(unknown)?.playlist(),
                _ => job.master_playlist(),
            };
            return Ok((headers, playlist).into_response());
        }
    };
    let (rendition, path) = state.jobs.rendition_segment(&job, k, n).await?;
    let handle = tokio::fs::File::open(&path).await?;
    let len = handle.metadata().await?.len();
    rendition.record_served(len);
    Metrics::inc(&state.metrics.segments_served);
    state.metrics.bytes_served.fetch_add(len, Ordering::Relaxed);
    let body = Body::from_stream(ReaderStream::with_capacity(handle, 256 * 1024));
    Response::builder()
        .header(header::CONTENT_TYPE, "video/mp2t")
        .header(header::CONTENT_LENGTH, len)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .map_err(|e| ApiError::Internal(e.into()))
}

async fn list_live(State(state): Shared) -> Json<Vec<pley_transcoder_api::LiveStatus>> {
    Json(state.live.list())
}

async fn create_live(
    State(state): Shared,
    Json(req): Json<pley_transcoder_api::LiveRequest>,
) -> ApiResult<(StatusCode, Json<pley_transcoder_api::LiveCreated>)> {
    Ok((StatusCode::CREATED, Json(state.live.create(req)?)))
}

async fn get_live(State(state): Shared, Path(id): Path<String>) -> ApiResult<Json<pley_transcoder_api::LiveStatus>> {
    Ok(Json(state.live.get(&id)?.status()))
}

async fn delete_live(State(state): Shared, Path(id): Path<String>) -> StatusCode {
    if state.live.remove(&id) { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

/// The channel's MPEG-TS, as long as it lasts: answered when it ends.
async fn live_input(State(state): Shared, Path(id): Path<String>, body: Body) -> ApiResult<StatusCode> {
    let live = state.live.get(&id)?;
    state.live.feed(&live, body.into_data_stream()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn live_file(State(state): Shared, Path((id, file)): Path<(String, String)>) -> ApiResult<Response> {
    let live = state.live.get(&id)?;
    match live.file(&file).await? {
        LiveFile::Playlist(text) => {
            Ok(([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl"), (header::CACHE_CONTROL, "no-cache")], text)
                .into_response())
        }
        LiveFile::Segment(path) => {
            let handle = crate::live::open_segment(&path).await?;
            let len = handle.metadata().await?.len();
            Metrics::inc(&state.metrics.segments_served);
            state.metrics.bytes_served.fetch_add(len, Ordering::Relaxed);
            Response::builder()
                .header(header::CONTENT_TYPE, "video/mp2t")
                .header(header::CONTENT_LENGTH, len)
                .header(header::CACHE_CONTROL, "max-age=60")
                .body(Body::from_stream(ReaderStream::with_capacity(handle, 256 * 1024)))
                .map_err(|e| ApiError::Internal(e.into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use pley_transcoder_api::{Capabilities, ErrorBody, Limits};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::ffmpeg::Ffmpeg;

    /// A transcoder whose FFmpeg is missing, on a free local port.
    async fn serve(dir: &std::path::Path, token: Option<&str>) -> SocketAddr {
        let ff = Arc::new(Ffmpeg {
            ffmpeg: dir.join("no-ffmpeg"),
            ffprobe: dir.join("no-ffprobe"),
            caps: Capabilities::default(),
            segment_times_relative: true,
        });
        let metrics = Arc::new(Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics.clone(), 1));
        let background = crate::background::Background::new();
        let keyframes = KeyframeStore::new(ff.clone(), metrics.clone(), background.clone(), dir.join("keyframes"));
        let extractor = Arc::new(Extractor::new(ff.clone(), prober.clone(), metrics.clone(), dir.join("subtitles"), 1));
        let fingerprinter = Arc::new(Fingerprinter::new(ff.clone(), background.clone(), 1));
        let trickplay = Arc::new(Trickplay::new(ff.clone(), prober.clone(), background.clone(), dir.join("trickplay")));
        let limits = Limits { max_jobs: 3, max_hw_jobs: 1 };
        let jobs = JobManager::new(
            ff.clone(),
            prober.clone(),
            keyframes.clone(),
            extractor.clone(),
            metrics.clone(),
            dir.join("jobs"),
            None,
            None,
            limits.clone(),
            background,
        );
        let info =
            TranscoderInfo { version: "1.2.3".into(), capabilities: ff.caps.clone(), limits, ..Default::default() };
        let conversions =
            Conversions::new(ff.clone(), prober.clone(), jobs.slots.clone(), None, dir.join("conversions"));
        let state = Arc::new(AppState::new(
            jobs,
            conversions,
            prober,
            extractor,
            keyframes,
            fingerprinter,
            trickplay,
            metrics,
            info,
            token.map(str::to_owned),
            dir.to_path_buf(),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(state)).await.unwrap() });
        addr
    }

    struct Answer {
        status: u16,
        /// The status line and headers, as sent (names lowercase).
        head: String,
        body: String,
    }

    impl Answer {
        fn code(&self) -> String {
            serde_json::from_str::<ErrorBody>(&self.body).map(|e| e.code).unwrap_or_default()
        }
    }

    /// One HTTP/1.1 request, read until the server closes the connection.
    async fn request(addr: SocketAddr, method: &str, path: &str, auth: Option<&str>, json: Option<String>) -> Answer {
        let headers: Vec<(&str, &str)> = auth.map(|auth| ("Authorization", auth)).into_iter().collect();
        request_with(addr, method, path, &headers, json).await
    }

    /// [`request`] with headers of its own.
    async fn request_with(
        addr: SocketAddr,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        json: Option<String>,
    ) -> Answer {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let body = json.unwrap_or_default();
        let mut message = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
        if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("host")) {
            message.push_str("Host: localhost\r\n");
        }
        for (name, value) in headers {
            message.push_str(&format!("{name}: {value}\r\n"));
        }
        message.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        stream.write_all(message.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP response");
        let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).expect("a status line");
        Answer { status, head: head.to_ascii_lowercase(), body: body.to_owned() }
    }

    #[test]
    fn ranges_are_read_as_downloaders_send_them() {
        assert_eq!(parse_range("bytes=0-", 10), Some(Some((0, 9))));
        assert_eq!(parse_range("bytes=4-6", 10), Some(Some((4, 6))));
        assert_eq!(parse_range("bytes=4-60", 10), Some(Some((4, 9))));
        assert_eq!(parse_range("bytes=10-", 10), Some(None), "past the end");
        assert_eq!(parse_range("bytes=6-4", 10), Some(None));
        assert_eq!(parse_range("bytes=0-1,4-5", 10), None, "several: the whole file");
        assert_eq!(parse_range("items=0-1", 10), None);
    }

    #[tokio::test]
    async fn a_conversions_file_comes_whole_or_from_where_a_download_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let id = "0123456789abcdef0123456789abcdef";
        std::fs::create_dir_all(dir.path().join("conversions")).unwrap();
        std::fs::write(dir.path().join("conversions").join(format!("{id}.mkv")), b"0123456789").unwrap();

        // Made before a restart: known by its file.
        let status = request(addr, "GET", &format!("/v1/conversions/{id}"), None, None).await;
        let status: ConversionStatus = serde_json::from_str(&status.body).unwrap();
        assert_eq!((status.state, status.size), (pley_transcoder_api::ConversionState::Done, Some(10)));

        let file = format!("/v1/conversions/{id}/file");
        let whole = request_with(addr, "GET", &file, &[], None).await;
        assert_eq!((whole.status, whole.body.as_str()), (200, "0123456789"));
        assert!(whole.head.contains("accept-ranges: bytes"), "{}", whole.head);
        let rest = request_with(addr, "GET", &file, &[("Range", "bytes=4-")], None).await;
        assert_eq!((rest.status, rest.body.as_str()), (206, "456789"));
        assert!(rest.head.contains("content-range: bytes 4-9/10"), "{}", rest.head);
        let past = request_with(addr, "GET", &file, &[("Range", "bytes=10-")], None).await;
        assert_eq!(past.status, 416);
        assert!(past.head.contains("content-range: bytes */10"), "{}", past.head);

        let removed = request(addr, "DELETE", &format!("/v1/conversions/{id}"), None, None).await;
        assert_eq!(removed.status, 204);
        for path in [format!("/v1/conversions/{id}"), file] {
            assert_eq!(request(addr, "GET", &path, None, None).await.status, 404, "{path}");
        }
        // What isn't an id names no file.
        for id in ["..%2F..%2Fsecret", "0123456789abcdef", "0123456789abcdef0123456789abcdeg"] {
            let answer = request(addr, "GET", &format!("/v1/conversions/{id}/file"), None, None).await;
            assert_eq!(answer.status, 404, "{id}");
        }
    }

    #[tokio::test]
    async fn conversions_need_ffmpeg_and_their_input() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let input = dir.path().join("movie.mkv");
        std::fs::write(&input, b"x").unwrap();
        let body = serde_json::json!({"input": input}).to_string();
        let answer = request(addr, "POST", "/v1/conversions", None, Some(body)).await;
        assert_eq!((answer.status, answer.code()), (500, "ffmpeg_failed".to_owned()), "{}", answer.body);
    }

    #[tokio::test]
    async fn the_token_guards_everything_but_health() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), Some("s3cret")).await;
        let health = request(addr, "GET", "/health", None, None).await;
        assert_eq!((health.status, health.body.as_str()), (200, "ok"));
        for auth in [
            None,
            Some("Bearer wrong"),
            Some("Bearer s3cret2"),
            Some("Bearer s3cre"),
            Some("bearer s3cret"),
            Some("s3cret"),
        ] {
            for path in ["/v1/info", "/v1/status", "/v1/jobs", "/metrics"] {
                let answer = request(addr, "GET", path, auth, None).await;
                assert_eq!((answer.status, answer.code()), (401, "unauthorized".to_owned()), "{path} with {auth:?}");
            }
        }
        let info = request(addr, "GET", "/v1/info", Some("Bearer s3cret"), None).await;
        assert_eq!(info.status, 200);
        assert_eq!(serde_json::from_str::<TranscoderInfo>(&info.body).unwrap().version, "1.2.3");
    }

    #[tokio::test]
    async fn without_a_token_only_requests_to_this_computer_are_answered() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let at_addr = addr.to_string();
        for host in ["localhost", "LocalHost:7540", "127.0.0.1", at_addr.as_str(), "[::1]", "[::1]:7540"] {
            let answer = request_with(addr, "GET", "/v1/info", &[("Host", host)], None).await;
            assert_eq!(answer.status, 200, "{host}");
        }
        // A web page whose own name was made 127.0.0.1 (DNS rebinding)
        // still sends that name: it reads nothing, converts nothing.
        let input = dir.path().join("movie.mkv");
        std::fs::write(&input, b"x").unwrap();
        let conversion = serde_json::json!({"input": input}).to_string();
        let subtitles = serde_json::json!({"path": dir.path().join("movie.vtt")}).to_string();
        for host in ["attacker.example", "attacker.example:7540", "localhost.attacker.example", "127.0.0.1.nip.io"] {
            for (method, path, body) in [
                ("GET", "/v1/status", None),
                ("GET", "/metrics", None),
                ("POST", "/v1/conversions", Some(conversion.clone())),
                ("POST", "/v1/subtitles", Some(subtitles.clone())),
            ] {
                let answer = request_with(addr, method, path, &[("Host", host)], body).await;
                assert_eq!((answer.status, answer.code()), (403, "forbidden".to_owned()), "{method} {path} to {host}");
                assert!(answer.body.contains("PLEY_TRANSCODER_TOKEN"), "{}", answer.body);
            }
        }
        let health = request_with(addr, "GET", "/health", &[("Host", "attacker.example")], None).await;
        assert_eq!((health.status, health.body.as_str()), (200, "ok"), "nothing to read there");

        // With a token, the token decides: other machines use its name.
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), Some("s3cret")).await;
        let headers = [("Host", "nas.lan:7540"), ("Authorization", "Bearer s3cret")];
        assert_eq!(request_with(addr, "GET", "/v1/info", &headers, None).await.status, 200);
    }

    #[test]
    fn loopback_hosts() {
        for host in ["localhost", "localhost:80", "127.0.0.1", "127.3.2.1:7540", "[::1]", "[::1]:7540"] {
            assert!(is_loopback_host(host), "{host}");
        }
        for host in ["", "nas.lan", "0.0.0.0:7540", "192.168.1.2", "localhost.example", "[::1]x", "[::2]:80", "::1"] {
            assert!(!is_loopback_host(host), "{host}");
        }
    }

    #[tokio::test]
    async fn status_and_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let status = request(addr, "GET", "/v1/status", None, None).await;
        assert_eq!(status.status, 200, "no token configured: open");
        let status: TranscoderStatus = serde_json::from_str(&status.body).unwrap();
        assert_eq!((status.max_jobs, status.max_hw_jobs, status.running_processes), (3, 1, 0));
        assert!(status.jobs.is_empty());
        let metrics = request(addr, "GET", "/metrics", None, None).await;
        assert_eq!(metrics.status, 200);
        assert!(metrics.body.contains("# TYPE pley_transcoder_jobs_created_total counter\n"), "{}", metrics.body);
        assert!(metrics.body.contains("pley_transcoder_max_jobs 3\n"), "{}", metrics.body);
    }

    #[tokio::test]
    async fn unknown_jobs_are_reported_as_gone() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        for (method, path) in [
            ("GET", "/v1/jobs/nope"),
            ("DELETE", "/v1/jobs/nope"),
            ("POST", "/v1/jobs/nope/touch"),
            ("GET", "/v1/jobs/nope/index.m3u8"),
            ("GET", "/v1/jobs/nope/master.m3u8"),
            ("GET", "/v1/jobs/nope/3.ts"),
        ] {
            let answer = request(addr, method, path, None, None).await;
            assert_eq!((answer.status, answer.code()), (404, "job_not_found".to_owned()), "{method} {path}");
        }
    }

    #[tokio::test]
    async fn missing_files_and_bad_names_are_plain_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let missing = serde_json::json!({"path": dir.path().join("missing.mkv")}).to_string();
        for path in ["/v1/probe", "/v1/keyframes", "/v1/trickplay", "/v1/prefetch"] {
            let answer = request(addr, "POST", path, None, Some(missing.clone())).await;
            assert_eq!((answer.status, answer.code()), (404, "not_found".to_owned()), "{path}");
        }
        // Sheet ids can't leave the previews folder.
        std::fs::create_dir_all(dir.path().join("secret")).unwrap();
        std::fs::write(dir.path().join("secret").join("0.jpg"), b"jpeg").unwrap();
        for path in ["/v1/trickplay/..%2Fsecret/0.jpg", "/v1/trickplay/a.b/0.jpg", "/v1/trickplay/abc/first.jpg"] {
            let answer = request(addr, "GET", path, None, None).await;
            assert_eq!((answer.status, answer.code()), (404, "not_found".to_owned()), "{path}");
        }
    }

    #[tokio::test]
    async fn preview_sheets_go_once_all_have_been_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let id = "0123abcd-2000-320";
        let set = dir.path().join("trickplay").join(id);
        std::fs::create_dir_all(&set).unwrap();
        let info = TrickplayInfo {
            id: id.into(),
            interval_ms: 2_000,
            width: 320,
            height: 180,
            columns: 10,
            rows: 10,
            count: 150,
            sheets: 2,
        };
        std::fs::write(set.join("index.json"), serde_json::to_vec(&info).unwrap()).unwrap();
        std::fs::write(set.join("0.jpg"), b"sheet 0").unwrap();
        std::fs::write(set.join("1.jpg"), b"sheet 1").unwrap();
        let sheet = |n: u32| {
            let path = format!("/v1/trickplay/{id}/{n}.jpg");
            async move { request(addr, "GET", &path, None, None).await }
        };
        // In any order, more than once…
        assert_eq!(sheet(1).await.status, 200);
        assert_eq!(sheet(1).await.status, 200);
        assert!(set.exists(), "sheet 0 has not been fetched yet");
        let last = sheet(0).await;
        assert_eq!((last.status, last.body.as_str()), (200, "sheet 0"));
        // …and once every sheet has been fetched, the server has its copy.
        assert!(!set.exists());
        assert_eq!(sheet(0).await.status, 404);
    }

    #[tokio::test]
    async fn prefetches_say_what_they_read() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let input = dir.path().join("clip.ts");
        std::fs::write(&input, vec![0x47u8; 3 << 20]).unwrap();
        let req = serde_json::json!({"path": input, "start_ms": 60_000}).to_string();
        let answer = request(addr, "POST", "/v1/prefetch", None, Some(req)).await;
        assert_eq!(answer.status, 200, "{}", answer.body);
        let done: Prefetched = serde_json::from_str(&answer.body).unwrap();
        assert_eq!(done.bytes, 3 << 20);
        let ranges: Vec<_> = done.ranges.iter().map(|r| (r.part.as_str(), r.start, r.end)).collect();
        assert_eq!(ranges, vec![("head", 0, 3 << 20)], "no index: the head only");
    }

    #[tokio::test]
    async fn a_hold_keeps_background_work_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let input = dir.path().join("clip.ts");
        std::fs::write(&input, vec![0x47u8; 1 << 20]).unwrap();
        let hold = serde_json::json!({"ms": 30_000}).to_string();
        assert_eq!(request(addr, "POST", "/v1/hold", None, Some(hold)).await.status, 204);
        let req = serde_json::json!({"path": input}).to_string();
        let answer = request(addr, "POST", "/v1/prefetch", None, Some(req)).await;
        let done: Prefetched = serde_json::from_str(&answer.body).unwrap();
        assert_eq!((done.bytes, done.skipped.as_deref()), (0, Some("a playback is starting")));
    }

    #[tokio::test]
    async fn jobs_need_a_working_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let addr = serve(dir.path(), None).await;
        let input = dir.path().join("movie.mkv");
        std::fs::write(&input, b"not really a movie").unwrap();
        let job = serde_json::json!({"input": input, "video": {}, "audio": {}}).to_string();
        let answer = request(addr, "POST", "/v1/jobs", None, Some(job)).await;
        assert_eq!((answer.status, answer.code()), (500, "ffmpeg_failed".to_owned()));
        let jobs = request(addr, "GET", "/v1/jobs", None, None).await;
        assert_eq!(jobs.body, "[]", "nothing was left behind");
    }
}
