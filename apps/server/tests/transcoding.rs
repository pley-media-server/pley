//! Playback through a transcoder: pley in external-transcoder mode talking
//! to a local stand-in that speaks the transcoder API (docs/TRANSCODER.md)
//! without running FFmpeg. It probes files by name, hands out HLS jobs with
//! fake segments, and can lose its jobs or change their layout, as a
//! restarted transcoder would.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use pley_transcoder_api::{
    API_VERSION, AudioMode, AudioPlan, BitrateChanged, BitrateRequest, Capabilities, ConversionState, ConversionStatus,
    EncoderInfo, ErrorBody, FileContainer, HoldRequest, HwAccel, JobCreated, JobRequest, JobStatus, Limits, MediaInfo,
    MediaStream, PrefetchRequest, Prefetched, ProbeRequest, SUBTITLES_COMPLETE_HEADER, StreamKind, SubtitleRequest,
    TranscoderInfo, TranscoderStatus, TrickplayInfo, TrickplayRequest, VideoCodec, VideoMode, VideoPlan,
};
use serde_json::{Value, json};

use common::{TestApp, encode};

const TOKEN: &str = "shared-secret";

// ------------------------------------------------------------------ the stand-in

#[derive(Clone, Default)]
struct Fake {
    /// Live jobs by id.
    jobs: Arc<Mutex<HashMap<String, (JobRequest, u32)>>>,
    /// Bitrates asked for as jobs play (job, kbps), in order.
    bitrates: Arc<Mutex<Vec<(String, u32)>>>,
    /// Every job request, in order.
    created: Arc<Mutex<Vec<JobRequest>>>,
    deleted: Arc<Mutex<Vec<String>>>,
    probes: Arc<Mutex<Vec<String>>>,
    subtitles: Arc<Mutex<Vec<SubtitleRequest>>>,
    thumbnails: Arc<AtomicUsize>,
    /// Paths seek previews were asked for.
    previews: Arc<Mutex<Vec<String>>>,
    /// How long the next previews take.
    preview_delay: Arc<Mutex<Duration>>,
    /// Preview requests given up by pley before they were answered.
    previews_dropped: Arc<AtomicUsize>,
    /// Files read ahead of a playback, in order.
    prefetches: Arc<Mutex<Vec<PrefetchRequest>>>,
    /// How long reading ahead takes.
    prefetch_delay: Arc<Mutex<Duration>>,
    next_id: Arc<AtomicUsize>,
    /// Segment count of new jobs instead of one per 4 s.
    layout: Arc<Mutex<Option<u32>>>,
    /// How long creating a job takes.
    job_delay: Arc<Mutex<Duration>>,
    /// Creating a job fails, as when ffmpeg can't read the file.
    fail_jobs: Arc<AtomicBool>,
    /// How long converting a subtitle file takes.
    sidecar_delay: Arc<Mutex<Duration>>,
    /// The next subtitle file's conversion fails.
    sidecar_fails: Arc<AtomicBool>,
    /// Subtitle file conversions answered (done or failed).
    sidecar_answers: Arc<AtomicUsize>,
    /// The media files can't be read (a share went to sleep): jobs and
    /// subtitles fail, and the transcoder's words name the file, as
    /// FFmpeg's do.
    unreadable: Arc<AtomicBool>,
    /// Subtitle tracks copied into Matroska files of their own, in order.
    tracks: Arc<Mutex<Vec<SubtitleRequest>>>,
    /// Conversions asked for, in order.
    conversions: Arc<Mutex<Vec<JobRequest>>>,
    /// The conversion is still being made when looked at (else done).
    conversion_running: Arc<AtomicBool>,
    /// Conversions stopped, in order.
    conversions_deleted: Arc<Mutex<Vec<String>>>,
    /// Background holds asked for (ms), in order.
    holds: Arc<Mutex<Vec<u64>>>,
    /// FFmpeg transcodes running, as `/v1/status` says.
    transcoding: Arc<AtomicU32>,
}

/// The stand-in's one conversion: running when asked for, done when looked at again.
const CONVERSION: &str = "0123456789abcdef0123456789abcdef";

impl Fake {
    /// The transcoder restarted: every job is gone.
    fn lose_jobs(&self) {
        self.jobs.lock().clear();
    }

    fn created(&self) -> usize {
        self.created.lock().len()
    }

    fn last_request(&self) -> JobRequest {
        self.created.lock().last().cloned().expect("a job request")
    }
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(ErrorBody { code: code.into(), message: format!("{code} (stand-in)") })).into_response()
}

/// The transcoder's answer for a file it can't read.
fn unreadable(path: &str) -> Response {
    let body = ErrorBody { code: "not_found".into(), message: format!("input not found: {path}") };
    (StatusCode::NOT_FOUND, Json(body)).into_response()
}

fn video(codec: &str) -> MediaStream {
    MediaStream {
        index: 0,
        kind: StreamKind::Video,
        codec: codec.into(),
        width: Some(1920),
        height: Some(1080),
        bit_depth: Some(8),
        ..Default::default()
    }
}

fn audio(index: i64, codec: &str, language: &str, default: bool) -> MediaStream {
    MediaStream {
        index,
        kind: StreamKind::Audio,
        codec: codec.into(),
        language: Some(language.into()),
        channels: Some(if codec == "aac" { 2 } else { 6 }),
        default,
        ..Default::default()
    }
}

fn subtitle(index: i64, codec: &str, language: &str, text: bool) -> MediaStream {
    MediaStream {
        index,
        kind: StreamKind::Subtitle,
        codec: codec.into(),
        language: Some(language.into()),
        text_based: Some(text),
        ..Default::default()
    }
}

/// What ffprobe would say, decided by the file name: MP4s are H.264/AAC,
/// MKVs H.264 with AC-3, a second audio track and two subtitle tracks.
fn probe_result(path: &str) -> Option<MediaInfo> {
    if path.contains("Broken") {
        return None;
    }
    let mkv = path.ends_with(".mkv");
    let titled = |index, codec, title: &str, default| MediaStream {
        title: Some(title.into()),
        ..audio(index, codec, "eng", default)
    };
    let streams = if path.contains("Lossless") {
        // A film with its English sound three ways: TrueHD (the default), Dolby Digital 5.1, stereo.
        vec![
            video("h264"),
            titled(1, "truehd", "TrueHD 7.1", true),
            titled(2, "ac3", "Dolby Digital 5.1", false),
            titled(3, "aac", "Stereo", false),
            subtitle(4, "subrip", "eng", true),
        ]
    } else if mkv {
        vec![
            video("h264"),
            audio(1, "ac3", "eng", true),
            audio(2, "aac", "fre", false),
            subtitle(3, "hdmv_pgs_subtitle", "eng", false),
            subtitle(4, "subrip", "fre", true),
        ]
    } else {
        vec![video("h264"), audio(1, "aac", "eng", true)]
    };
    Some(MediaInfo {
        container: Some(if mkv { "mkv" } else { "mp4" }.into()),
        duration_ms: Some(60_000),
        bitrate: Some(4_000_000),
        start_ms: Some(0),
        streams,
        ..Default::default()
    })
}

fn job_status(id: &str, req: &JobRequest, segments: u32) -> JobStatus {
    JobStatus {
        id: id.into(),
        label: req.label.clone(),
        owner: req.owner.clone(),
        input: req.input.clone(),
        duration_ms: 60_000,
        segment_seconds: 4.0,
        segment_count: segments,
        video: req.video.as_ref().map(|v| VideoPlan {
            mode: v.mode,
            codec_in: "h264".into(),
            codec_out: "h264".into(),
            width_in: Some(1920),
            height_in: Some(1080),
            width_out: Some(1920),
            height_out: Some(v.max_height.map(i64::from).unwrap_or(1080)),
            ..Default::default()
        }),
        audio: req.audio.as_ref().map(|a| AudioPlan {
            mode: a.mode,
            codec_in: "ac3".into(),
            codec_out: "aac".into(),
            channels_in: Some(6),
            channels_out: Some(2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Counts requests dropped before they were answered (pley gave up on them,
/// and the connection closed).
struct Answered(Arc<AtomicUsize>, bool);

impl Drop for Answered {
    fn drop(&mut self) {
        if !self.1 {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn authorized(headers: &HeaderMap) -> bool {
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(&format!("Bearer {TOKEN}"))
}

async fn start_fake() -> (Fake, String) {
    let fake = Fake::default();
    let app = Router::new()
        .route(
            "/v1/info",
            get(|headers: HeaderMap| async move {
                if !authorized(&headers) {
                    return error(StatusCode::UNAUTHORIZED, "unauthorized");
                }
                Json(TranscoderInfo {
                    name: "pley-transcoder".into(),
                    version: "9.9.9".into(),
                    api_version: API_VERSION,
                    license: "AGPL-3.0-only".into(),
                    capabilities: Capabilities {
                        ffmpeg_available: true,
                        ffprobe_available: true,
                        ffmpeg_version: Some("stand-in".into()),
                        encoders: vec![EncoderInfo {
                            name: "libx264".into(),
                            codec: VideoCodec::H264,
                            hardware: false,
                            accel: HwAccel::None,
                            main10: false,
                        }],
                        subtitle_matroska: true,
                        subtitle_matroska_partial: true,
                        conversions: true,
                        conversions_mp4: true,
                        live_bitrate: true,
                        ..Default::default()
                    },
                    limits: Limits { max_jobs: 8, max_hw_jobs: 0 },
                    ..Default::default()
                })
                .into_response()
            }),
        )
        .route(
            "/v1/status",
            get(|State(fake): State<Fake>| async move {
                let jobs = fake.jobs.lock().iter().map(|(id, (req, n))| job_status(id, req, *n)).collect();
                Json(TranscoderStatus {
                    version: "9.9.9".into(),
                    max_jobs: 8,
                    running_processes: fake.transcoding.load(Ordering::SeqCst),
                    jobs,
                    ..Default::default()
                })
            }),
        )
        .route(
            "/v1/probe",
            post(|State(fake): State<Fake>, Json(req): Json<ProbeRequest>| async move {
                fake.probes.lock().push(req.path.clone());
                match probe_result(&req.path) {
                    Some(info) => Json(info).into_response(),
                    None => error(StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed"),
                }
            }),
        )
        .route(
            "/v1/jobs",
            post(|State(fake): State<Fake>, Json(req): Json<JobRequest>| async move {
                let delay = *fake.job_delay.lock();
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                if fake.fail_jobs.load(Ordering::SeqCst) {
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed");
                }
                if fake.unreadable.load(Ordering::SeqCst) {
                    return unreadable(&req.input);
                }
                let n = fake.next_id.fetch_add(1, Ordering::SeqCst) + 1;
                let id = format!("job{n}");
                let segments = fake.layout.lock().unwrap_or(15);
                let status = job_status(&id, &req, segments);
                fake.created.lock().push(req.clone());
                fake.jobs.lock().insert(id.clone(), (req, segments));
                Json(JobCreated {
                    id,
                    duration_ms: 60_000,
                    segment_seconds: 4.0,
                    segment_count: segments,
                    video: status.video,
                    audio: status.audio,
                    warnings: vec![],
                    variants: 0,
                })
                .into_response()
            }),
        )
        .route(
            "/v1/jobs/{id}",
            get(|State(fake): State<Fake>, Path(id): Path<String>| async move {
                match fake.jobs.lock().get(&id) {
                    Some((req, n)) => {
                        let mut status = job_status(&id, req, *n);
                        if fake.unreadable.load(Ordering::SeqCst) {
                            status.last_error = Some(format!("Error opening input file file:{}.", req.input));
                        }
                        Json(status).into_response()
                    }
                    None => error(StatusCode::NOT_FOUND, "job_not_found"),
                }
            })
            .delete(|State(fake): State<Fake>, Path(id): Path<String>| async move {
                fake.deleted.lock().push(id.clone());
                match fake.jobs.lock().remove(&id) {
                    Some(_) => StatusCode::NO_CONTENT.into_response(),
                    None => error(StatusCode::NOT_FOUND, "job_not_found"),
                }
            }),
        )
        .route(
            "/v1/jobs/{id}/bitrate",
            post(|State(fake): State<Fake>, Path(id): Path<String>, Json(req): Json<BitrateRequest>| async move {
                if !fake.jobs.lock().contains_key(&id) {
                    return error(StatusCode::NOT_FOUND, "job_not_found");
                }
                fake.bitrates.lock().push((id, req.max_bitrate_kbps));
                Json(BitrateChanged { bitrate_kbps: req.max_bitrate_kbps.clamp(1_000, 8_000) }).into_response()
            }),
        )
        .route(
            "/v1/jobs/{id}/touch",
            post(|State(fake): State<Fake>, Path(id): Path<String>| async move {
                if fake.jobs.lock().contains_key(&id) {
                    StatusCode::NO_CONTENT.into_response()
                } else {
                    error(StatusCode::NOT_FOUND, "job_not_found")
                }
            }),
        )
        .route(
            "/v1/jobs/{id}/{file}",
            get(|State(fake): State<Fake>, Path((id, file)): Path<(String, String)>| async move {
                let Some(segments) = fake.jobs.lock().get(&id).map(|(_, n)| *n) else {
                    return error(StatusCode::NOT_FOUND, "job_not_found");
                };
                if file == "index.m3u8" {
                    let mut playlist = String::from("#EXTM3U\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-TARGETDURATION:4\n");
                    for n in 0..segments {
                        playlist.push_str(&format!("#EXTINF:4.000,\n{n}.ts\n"));
                    }
                    playlist.push_str("#EXT-X-ENDLIST\n");
                    return ([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")], playlist).into_response();
                }
                match file.strip_suffix(".ts").and_then(|n| n.parse::<u32>().ok()) {
                    Some(n) if n < segments => {
                        ([(header::CONTENT_TYPE, "video/mp2t")], format!("{id}:{n}")).into_response()
                    }
                    _ => error(StatusCode::NOT_FOUND, "not_found"),
                }
            }),
        )
        .route(
            "/v1/subtitles",
            // Stream 99 can't be converted; stream 5 is in a big file: the
            // first answer has only the cues read so far.
            post(|State(fake): State<Fake>, Json(req): Json<SubtitleRequest>| async move {
                let text = format!("WEBVTT\n\n00:00.000 --> 00:01.000\n{}#{:?}\n", req.path, req.stream_index);
                let fails = req.stream_index == Some(99);
                let first_of_slow =
                    req.stream_index == Some(5) && !fake.subtitles.lock().iter().any(|r| r.stream_index == Some(5));
                if fake.unreadable.load(Ordering::SeqCst) {
                    return unreadable(&req.path);
                }
                let partial = req.partial;
                let sidecar = req.stream_index.is_none();
                fake.subtitles.lock().push(req);
                if sidecar {
                    let delay = *fake.sidecar_delay.lock();
                    tokio::time::sleep(delay).await;
                    fake.sidecar_answers.fetch_add(1, Ordering::SeqCst);
                }
                if fails || (sidecar && fake.sidecar_fails.swap(false, Ordering::SeqCst)) {
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed");
                }
                if first_of_slow && partial {
                    let so_far = "WEBVTT\n\n00:00.000 --> 00:01.000\nso far\n\n";
                    let more = HeaderName::from_static(SUBTITLES_COMPLETE_HEADER);
                    return ([(header::CONTENT_TYPE, "text/vtt"), (more, "0")], so_far).into_response();
                }
                ([(header::CONTENT_TYPE, "text/vtt")], text).into_response()
            }),
        )
        .route(
            "/v1/subtitles/mks",
            // An EBML header, then what was asked for. Stream 3's copy takes
            // a while: its first look has what was copied so far.
            post(|State(fake): State<Fake>, Json(req): Json<SubtitleRequest>| async move {
                let mut mks = vec![0x1A, 0x45, 0xDF, 0xA3];
                let first =
                    !fake.tracks.lock().iter().any(|r| r.stream_index == req.stream_index && r.path == req.path);
                let slow = req.partial && req.stream_index == Some(3) && first;
                if slow {
                    mks.extend(b"so far");
                } else {
                    mks.extend(format!("{}#{:?}", req.path, req.stream_index).into_bytes());
                }
                let complete = HeaderName::from_static(SUBTITLES_COMPLETE_HEADER);
                fake.tracks.lock().push(req);
                ([(header::CONTENT_TYPE, "video/x-matroska"), (complete, if slow { "0" } else { "1" })], mks)
            }),
        )
        .route(
            "/v1/thumbnail",
            post(|State(fake): State<Fake>| async move {
                fake.thumbnails.fetch_add(1, Ordering::SeqCst);
                let mut jpeg = Vec::new();
                image::RgbImage::from_pixel(64, 36, image::Rgb([200, 100, 50]))
                    .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
                    .unwrap();
                ([(header::CONTENT_TYPE, "image/jpeg")], jpeg)
            }),
        )
        .route("/v1/keyframes", post(|| async { StatusCode::ACCEPTED }))
        .route(
            "/v1/hold",
            post(|State(fake): State<Fake>, Json(req): Json<HoldRequest>| async move {
                fake.holds.lock().push(req.ms);
                StatusCode::NO_CONTENT
            }),
        )
        .route(
            "/v1/conversions",
            post(|State(fake): State<Fake>, Json(req): Json<JobRequest>| async move {
                fake.conversions.lock().push(req);
                Json(ConversionStatus {
                    id: CONVERSION.into(),
                    state: ConversionState::Running,
                    progress: 0.25,
                    duration_ms: 60_000,
                    ..Default::default()
                })
            }),
        )
        .route(
            "/v1/conversions/{id}",
            get(|State(fake): State<Fake>, Path(id): Path<String>| async move {
                if id != CONVERSION {
                    return error(StatusCode::NOT_FOUND, "not_found");
                }
                if fake.conversion_running.load(Ordering::SeqCst) {
                    return Json(ConversionStatus {
                        id,
                        state: ConversionState::Running,
                        progress: 0.5,
                        duration_ms: 60_000,
                        ..Default::default()
                    })
                    .into_response();
                }
                Json(ConversionStatus {
                    id,
                    state: ConversionState::Done,
                    progress: 1.0,
                    duration_ms: 60_000,
                    size: Some(10),
                    ..Default::default()
                })
                .into_response()
            })
            .delete(|State(fake): State<Fake>, Path(id): Path<String>| async move {
                fake.conversions_deleted.lock().push(id);
                StatusCode::NO_CONTENT
            }),
        )
        .route(
            "/v1/conversions/{id}/file",
            get(|Path(id): Path<String>, headers: HeaderMap| async move {
                if id != CONVERSION {
                    return error(StatusCode::NOT_FOUND, "not_found");
                }
                let body = "0123456789";
                match headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("bytes=")) {
                    Some(range) => {
                        let start: usize = range.trim_end_matches('-').parse().unwrap();
                        (
                            StatusCode::PARTIAL_CONTENT,
                            [
                                (header::CONTENT_TYPE, "video/x-matroska".to_owned()),
                                (header::CONTENT_RANGE, format!("bytes {start}-9/10")),
                                (header::ACCEPT_RANGES, "bytes".to_owned()),
                            ],
                            body[start..].to_owned(),
                        )
                            .into_response()
                    }
                    None => ([(header::CONTENT_TYPE, "video/x-matroska")], body).into_response(),
                }
            }),
        )
        .route(
            "/v1/prefetch",
            post(|State(fake): State<Fake>, Json(req): Json<PrefetchRequest>| async move {
                fake.prefetches.lock().push(req);
                let delay = *fake.prefetch_delay.lock();
                tokio::time::sleep(delay).await;
                Json(Prefetched { bytes: 4 << 20, ..Default::default() })
            }),
        )
        // Previews of the MP4 work: one sheet of 5×2 tiles of 8×4. Others fail.
        .route(
            "/v1/trickplay",
            post(|State(fake): State<Fake>, Json(req): Json<TrickplayRequest>| async move {
                fake.previews.lock().push(req.path.clone());
                let mut answered = Answered(fake.previews_dropped.clone(), false);
                let delay = std::mem::take(&mut *fake.preview_delay.lock());
                tokio::time::sleep(delay).await;
                answered.1 = true;
                if !req.path.ends_with(".mp4") {
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed");
                }
                Json(TrickplayInfo {
                    id: "previews1".into(),
                    interval_ms: 10_000,
                    width: 8,
                    height: 4,
                    columns: 5,
                    rows: 2,
                    count: 6,
                    sheets: 1,
                })
                .into_response()
            }),
        )
        .route(
            "/v1/trickplay/{id}/{sheet}",
            get(|Path((id, sheet)): Path<(String, String)>| async move {
                if id != "previews1" || sheet != "0.jpg" {
                    return error(StatusCode::NOT_FOUND, "not_found");
                }
                let mut jpeg = Vec::new();
                image::RgbImage::from_fn(40, 8, |x, y| image::Rgb([(x * 6) as u8, (y * 30) as u8, 90]))
                    .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
                    .unwrap();
                ([(header::CONTENT_TYPE, "image/jpeg")], jpeg).into_response()
            }),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, format!("http://{addr}"))
}

// ------------------------------------------------------------------ pley

struct World {
    app: TestApp,
    fake: Fake,
    base: String,
    admin: String,
    /// Title → id of the movies.
    movies: HashMap<String, i64>,
}

/// pley with the stand-in as its transcoder and a scanned movie library:
/// "Feature" (MKV, with an English sidecar subtitle), "Clip" (MP4) and
/// "Broken" (ffprobe can't read it).
async fn world() -> World {
    let (fake, base) = start_fake().await;
    let url = base.clone();
    let app = TestApp::with_config(|c| {
        c.transcoder_url = Some(url);
        c.transcoder_token = Some(TOKEN.into());
    })
    .await;
    app.state.transcoder.start();
    assert!(app.state.transcoder.wait_ready(Duration::from_secs(10)).await, "the stand-in answers /v1/info");
    let admin = app.setup_admin().await;
    // No seek previews in the background: only what the tests ask for.
    let settings = app.put("/api/settings", &admin, json!({"seek_previews": false})).await;
    assert_eq!(settings.status, StatusCode::OK);
    // Different sizes: identical files would share their media folder.
    app.media_file("movies/Feature (2021)/Feature (2021).mkv", 4096);
    app.media_file("movies/Feature (2021)/Feature (2021).en.srt", 64);
    app.media_file("movies/Clip (2020).mp4", 3072);
    app.media_file("movies/Broken (2019).mkv", 2048);
    let library = app.library(&admin, "Movies", "movies", "movies").await;
    let movies = app.ids(library, &admin).await;
    World { app, fake, base, admin, movies }
}

/// A phone's caps: Matroska and H.264 played as they are, but not AC-3.
fn phone() -> Value {
    json!({"containers": ["mkv", "mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac"]})
}

#[tokio::test]
async fn a_download_takes_the_original_when_it_plays_as_it_is() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let clip = w.movies["Clip"];
    let answer = app.post("/api/conversions", admin, json!({"item_id": clip, "caps": browser()})).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let file = app.file_of(clip).await;
    assert_eq!(
        (&answer.body["state"], &answer.body["id"], &answer.body["url"], &answer.body["size"]),
        (&json!("direct"), &Value::Null, &json!(format!("/api/files/{file}/stream")), &json!(3072))
    );
    assert!(w.fake.conversions.lock().is_empty(), "nothing to convert");
}

#[tokio::test]
async fn a_download_the_player_cannot_play_is_converted_and_comes_with_ranges() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let answer = app.post("/api/conversions", admin, json!({"item_id": feature, "caps": phone()})).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(
        (&answer.body["state"], &answer.body["id"], &answer.body["progress"]),
        (&json!("running"), &json!(CONVERSION), &json!(0.25))
    );
    let url = format!("/api/conversions/{CONVERSION}/file");
    assert_eq!(answer.body["url"], json!(url));
    // What a playback would stream: the video as it is, the sound made AAC, from the start, at full speed.
    let asked = w.fake.conversions.lock().last().cloned().unwrap();
    assert_eq!(asked.video.as_ref().map(|v| v.mode), Some(VideoMode::Copy));
    assert_eq!(asked.audio.as_ref().map(|a| a.mode), Some(AudioMode::Transcode));
    assert_eq!((asked.start_ms, asked.segment_seconds), (None, None));
    assert!(asked.input.ends_with("Feature (2021).mkv"), "{}", asked.input);

    let status = app.get(&format!("/api/conversions/{CONVERSION}"), admin).await;
    assert_eq!((&status.body["state"], &status.body["size"]), (&json!("done"), &json!(10)));
    let rest = app.call(Method::GET, &url, Some(admin), None, &[("range", "bytes=4-")]).await;
    assert_eq!(rest.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(rest.headers.get("content-range").and_then(|v| v.to_str().ok()), Some("bytes 4-9/10"));
    assert_eq!(rest.text(), "456789");

    // Someone who can't see the movie can't see its conversion either.
    let (other, _) = app.user(admin, "other", json!({"all_libraries": false, "library_ids": []})).await;
    for path in [format!("/api/conversions/{CONVERSION}"), url.clone()] {
        assert_eq!(app.get(&path, &other).await.status, StatusCode::NOT_FOUND, "{path}");
    }
    assert_eq!(app.get("/api/conversions/ffffffffffffffffffffffffffffffff", admin).await.status, StatusCode::NOT_FOUND);
}

/// A Mac's QuickTime: MP4 with H.264 or HEVC and AAC, AC-3 or E-AC-3; no Matroska.
fn quicktime() -> Value {
    json!({
        "containers": ["mp4", "m4v", "mov"],
        "video_codecs": ["h264", "hevc"],
        "audio_codecs": ["aac", "ac3", "eac3", "alac", "mp3"],
    })
}

#[tokio::test]
async fn a_download_for_a_player_that_opens_no_matroska_is_made_mp4() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let answer = app.post("/api/conversions", admin, json!({"item_id": feature, "caps": quicktime()})).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let asked = w.fake.conversions.lock().last().cloned().unwrap();
    assert_eq!(asked.container, Some(FileContainer::Mp4));
    assert_eq!(asked.video.as_ref().map(|v| v.mode), Some(VideoMode::Copy), "only the container changes");

    // A phone opens Matroska, which keeps every track.
    let answer = app.post("/api/conversions", admin, json!({"item_id": feature, "caps": phone()})).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(w.fake.conversions.lock().last().unwrap().container, None);
}

#[tokio::test]
async fn a_download_waits_while_the_transcoder_isnt_running() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Feature (2021)/Feature (2021).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let feature = app.ids(movies, &admin).await["Feature"];
    // Matroska doesn't play in a browser: it would be converted.
    let answer = app.post("/api/conversions", &admin, json!({"item_id": feature, "caps": browser()})).await;
    assert_eq!((answer.status, &answer.body["code"]), (StatusCode::SERVICE_UNAVAILABLE, &json!("unavailable")));
}

#[tokio::test]
async fn a_conversion_stops_when_its_download_goes_unless_its_file_is_made() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let ask = || app.post("/api/conversions", admin, json!({"item_id": feature, "caps": phone()}));
    assert_eq!(ask().await.status, StatusCode::OK);
    let path = format!("/api/conversions/{CONVERSION}");

    // Someone who can't see the movie can't stop it.
    let (other, _) = app.user(admin, "other", json!({"all_libraries": false, "library_ids": []})).await;
    assert_eq!(app.delete(&path, &other).await.status, StatusCode::NOT_FOUND);

    // Made: the file stays for whoever asks for the same next.
    assert_eq!(app.delete(&path, admin).await.status, StatusCode::NO_CONTENT);
    assert!(w.fake.conversions_deleted.lock().is_empty());
    assert_eq!(app.get(&path, admin).await.status, StatusCode::OK);

    // Still being made: it stops, and is unknown until asked for again.
    w.fake.conversion_running.store(true, Ordering::SeqCst);
    assert_eq!(app.delete(&path, admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(*w.fake.conversions_deleted.lock(), [CONVERSION]);
    assert_eq!(app.get(&path, admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(ask().await.status, StatusCode::OK);
    assert_eq!(app.get(&path, admin).await.body["state"], json!("running"));
}

#[tokio::test]
async fn a_conversion_two_people_wait_for_stops_only_when_both_give_it_up() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let (ann, _) = app.user(admin, "ann", json!({"all_libraries": true})).await;
    let (bob, _) = app.user(admin, "bob", json!({"all_libraries": true})).await;
    for token in [&ann, &bob] {
        let asked = app.post("/api/conversions", token, json!({"item_id": feature, "caps": phone()})).await;
        assert_eq!(asked.status, StatusCode::OK, "{}", asked.body);
    }
    w.fake.conversion_running.store(true, Ordering::SeqCst);
    let path = format!("/api/conversions/{CONVERSION}");

    // Ann deleting her download doesn't stop Bob's.
    assert_eq!(app.delete(&path, &ann).await.status, StatusCode::NO_CONTENT);
    assert!(w.fake.conversions_deleted.lock().is_empty());
    assert_eq!(app.get(&path, &bob).await.status, StatusCode::OK);

    // Bob was the last to want it.
    assert_eq!(app.delete(&path, &bob).await.status, StatusCode::NO_CONTENT);
    assert_eq!(*w.fake.conversions_deleted.lock(), [CONVERSION]);
}

fn browser() -> Value {
    json!({"containers": ["mp4", "webm"], "video_codecs": ["h264"], "audio_codecs": ["aac", "mp3"]})
}

async fn play(app: &TestApp, token: &str, body: Value) -> common::Reply {
    app.post("/api/playback", token, body).await
}

/// Waits for background work of the server (job deletions) to reach the stand-in.
async fn eventually(check: impl Fn() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached");
}

#[tokio::test]
async fn scans_keep_what_the_transcoder_probed() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = app.get(&format!("/api/items/{}", w.movies["Feature"]), admin).await;
    let file = &feature.body["files"][0];
    assert_eq!(
        (&file["container"], &file["video_codec"], &file["audio_codec"], &file["duration_ms"], &file["height"]),
        (&json!("mkv"), &json!("h264"), &json!("ac3"), &json!(60_000), &json!(1080))
    );
    let streams = file["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 6, "five embedded streams and the sidecar");
    assert_eq!(streams[5]["external"], json!(true));
    assert_eq!(feature.body["duration_ms"], json!(60_000), "the item takes the file's duration");
    // ffprobe could not read this one: listed, without media details.
    let broken = app.get(&format!("/api/items/{}", w.movies["Broken"]), admin).await;
    assert_eq!(broken.body["files"][0]["video_codec"], Value::Null);

    // Unchanged files are not probed again, unreadable ones neither; a forced scan probes everything.
    let probes = || w.fake.probes.lock().len();
    assert_eq!(probes(), 3);
    let library = feature.body["library_id"].as_i64().unwrap();
    app.post(&format!("/api/libraries/{library}/scan"), admin, json!({})).await;
    app.wait_for_scans().await;
    assert_eq!(probes(), 3);
    app.post(&format!("/api/libraries/{library}/scan"), admin, json!({"force": true})).await;
    app.wait_for_scans().await;
    assert_eq!(probes(), 6);

    let system = app.get("/api/system", admin).await;
    let transcoder = &system.body["transcoder"];
    assert_eq!((&transcoder["available"], &transcoder["managed"]), (&json!(true), &json!(false)));
    assert_eq!((&transcoder["version"], &transcoder["url"]), (&json!("9.9.9"), &json!(w.base)));
    assert_eq!(transcoder["active_encoder"], json!("libx264"));
}

#[tokio::test]
async fn remux_sessions_stream_hls_from_the_transcoder() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let started = play(app, admin, json!({"item_id": feature, "caps": browser(), "client_id": "tab"})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    let info = &started.body;
    assert_eq!((&info["method"], &info["hls"], &info["duration_ms"]), (&json!("remux"), &json!(true), &json!(60_000)));
    assert_eq!(info["audio_converted"], json!(true), "AC-3 doesn't play in browsers");
    let session = info["session_id"].as_str().unwrap().to_owned();
    assert_eq!(info["url"], json!(format!("/api/playback/{session}/index.m3u8")));
    assert_eq!(info["reasons"], json!(["MKV not supported.", "AC3 audio not supported."]), "one plain sentence each");
    assert_eq!(info["reason"], json!("MKV not supported. AC3 audio not supported."));

    // The player's details: the session's transcode, live.
    let status = app.get(&format!("/api/playback/{session}"), admin).await;
    assert_eq!(status.status, StatusCode::OK, "{}", status.text());
    assert_eq!(status.body["method"], "remux");
    let live = &status.body["transcode"];
    assert_eq!((&live["video"], &live["audio"]), (&json!("copy"), &json!("transcode")));
    assert_eq!((&live["audio_in"], &live["audio_out"]), (&json!("AC3 5.1"), &json!("AAC 2.0")));
    let (other, _) = app.user(admin, "nosy", json!({})).await;
    assert_eq!(app.get(&format!("/api/playback/{session}"), &other).await.status, StatusCode::FORBIDDEN);
    let subtitles = info["subtitles"].as_array().unwrap();
    let urls: Vec<&Value> = subtitles.iter().map(|s| &s["url"]).collect();
    assert_eq!(subtitles.len(), 3);
    assert_eq!(urls[0], &Value::Null, "image subtitles have no WebVTT");
    assert!(urls[1].as_str().unwrap().ends_with("/subtitles/4.vtt"));

    // What the transcoder was asked for.
    let job = w.fake.last_request();
    assert!(job.input.ends_with("Feature (2021).mkv"));
    assert_eq!((job.label.as_deref(), job.owner.as_deref()), (Some("Feature (2021)"), Some("admin")));
    assert_eq!(job.video.as_ref().unwrap().mode, VideoMode::Copy);
    let audio = job.audio.as_ref().unwrap();
    assert_eq!((audio.mode, audio.stream_index), (AudioMode::Transcode, Some(1)));
    assert_eq!(job.media.as_ref().and_then(|m| m.duration_ms), Some(60_000), "probe data is passed on");
    assert_eq!(job.segment_seconds, Some(2.0), "short segments: the first one is ready sooner");

    let playlist = app.get(&format!("/api/playback/{session}/index.m3u8"), admin).await;
    assert_eq!(playlist.status, StatusCode::OK);
    assert_eq!(playlist.headers[header::CONTENT_TYPE], "application/vnd.apple.mpegurl");
    assert!(playlist.text().contains("14.ts\n#EXT-X-ENDLIST"));
    let segment = app.get(&format!("/api/playback/{session}/3.ts"), admin).await;
    assert_eq!((segment.status, segment.text()), (StatusCode::OK, "job1:3".to_owned()));
    assert_eq!(segment.headers[header::CONTENT_TYPE], "video/mp2t");
    // Out of range is not a lost job: nothing is recreated.
    assert_eq!(app.get(&format!("/api/playback/{session}/99.ts"), admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/playback/{session}/x.ts"), admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.fake.created(), 1);

    let activity = app.get("/api/activity", admin).await;
    let listed = &activity.body["sessions"][0];
    assert_eq!((&listed["method"], &listed["item"]["title"]), (&json!("remux"), &json!("Feature")));
    assert_eq!(listed["transcode"]["video"], json!("copy"));
    assert_eq!(listed["transcode"]["audio_out"], json!("AAC 2.0"));

    // Stopping ends the transcoder job.
    let stopped = app.delete(&format!("/api/playback/{session}"), admin).await;
    assert_eq!(stopped.status, StatusCode::NO_CONTENT);
    eventually(|| w.fake.deleted.lock().contains(&"job1".to_owned())).await;
    let gone = app.get(&format!("/api/playback/{session}/index.m3u8"), admin).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.body["message"], json!("playback session expired"));
}

#[tokio::test]
async fn a_lost_job_is_recreated_once_with_the_same_layout() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let started = play(app, admin, json!({"item_id": feature, "caps": browser()})).await;
    let session = started.body["session_id"].as_str().unwrap().to_owned();
    let progress = json!({"position_ms": 20_000, "duration_ms": 60_000, "state": "playing", "session_id": session});
    assert_eq!(
        app.post(&format!("/api/items/{feature}/progress"), admin, progress).await.status,
        StatusCode::NO_CONTENT
    );

    // The transcoder restarts; two segment requests arrive at once.
    w.fake.lose_jobs();
    let five = format!("/api/playback/{session}/5.ts");
    let six = format!("/api/playback/{session}/6.ts");
    let (a, b) = tokio::join!(app.get(&five, admin), app.get(&six, admin));
    assert_eq!((a.status, a.text()), (StatusCode::OK, "job2:5".to_owned()));
    assert_eq!((b.status, b.text()), (StatusCode::OK, "job2:6".to_owned()));
    assert_eq!(w.fake.created(), 2, "one replacement for both requests");
    let replacement = w.fake.last_request();
    assert_eq!(replacement.start_ms, Some(20_000), "pre-started at the playhead");
    assert_eq!(replacement.video.unwrap().mode, VideoMode::Copy, "the same effective mode");
    assert_eq!(replacement.segment_seconds, Some(2.0), "the same segments");
    assert_eq!(app.get(&format!("/api/playback/{session}/index.m3u8"), admin).await.status, StatusCode::OK);

    // This time it can't give the same segments: the session is over.
    *w.fake.layout.lock() = Some(10);
    w.fake.lose_jobs();
    let lost = app.get(&five, admin).await;
    assert_eq!(lost.status, StatusCode::NOT_FOUND);
    assert_eq!(lost.body["message"], json!("playback session expired"));
    eventually(|| w.fake.deleted.lock().contains(&"job3".to_owned())).await;
    assert!(w.fake.jobs.lock().is_empty(), "the mismatched job is not left running");
    assert_eq!(app.get(&six, admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.fake.created(), 3, "no further attempts");
}

#[tokio::test]
async fn a_converted_streams_bitrate_changes_as_it_plays() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let started =
        play(app, admin, json!({"item_id": feature, "caps": browser(), "mode": "transcode", "max_bitrate": 8000}))
            .await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    assert_eq!(started.body["live_bitrate"], json!(true));
    let session = started.body["session_id"].as_str().unwrap().to_owned();
    let changed = app.put(&format!("/api/playback/{session}/bitrate"), admin, json!({"max_bitrate": 3000})).await;
    assert_eq!((changed.status, changed.body.clone()), (StatusCode::OK, json!({"bitrate": 3000})));
    // What the transcoder makes of it is what the player hears.
    let low = app.put(&format!("/api/playback/{session}/bitrate"), admin, json!({"max_bitrate": 200})).await;
    assert_eq!(low.body, json!({"bitrate": 1000}));
    assert_eq!(*w.fake.bitrates.lock(), [("job1".to_owned(), 3000), ("job1".to_owned(), 200)]);

    // A job the transcoder lost comes back at the bitrate asked for since.
    w.fake.lose_jobs();
    assert_eq!(app.get(&format!("/api/playback/{session}/3.ts"), admin).await.status, StatusCode::OK);
    assert_eq!(w.fake.bitrates.lock().last(), Some(&("job2".to_owned(), 1000)));

    // A remux keeps the file's bitrate.
    let remux = play(app, admin, json!({"item_id": feature, "caps": browser(), "client_id": "tv"})).await;
    assert_eq!(remux.body["live_bitrate"], json!(false));
    let id = remux.body["session_id"].as_str().unwrap();
    let refused = app.put(&format!("/api/playback/{id}/bitrate"), admin, json!({"max_bitrate": 3000})).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_transcode_limit_counts_live_hls_sessions() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.put("/api/settings", admin, json!({"max_transcodes": 1})).await;
    let feature = w.movies["Feature"];
    let start = |client: &str| json!({"item_id": feature, "caps": browser(), "client_id": client});

    let tab = play(app, admin, start("tab")).await;
    assert_eq!(tab.status, StatusCode::OK);
    let tv = play(app, admin, start("tv")).await;
    assert_eq!(tv.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(tv.body["code"], json!("unavailable"));
    assert!(tv.body["message"].as_str().unwrap().contains("maximum of 1"));
    // Direct play never counts.
    let clip = play(app, admin, json!({"item_id": w.movies["Clip"], "caps": browser(), "client_id": "tv"})).await;
    assert_eq!((clip.status, clip.body["method"].clone()), (StatusCode::OK, json!("direct")));

    // The same client switching quality replaces its own session.
    let switched =
        play(app, admin, json!({"item_id": feature, "caps": browser(), "client_id": "tab", "max_height": 720})).await;
    assert_eq!(switched.status, StatusCode::OK, "{:?}", switched.body);
    assert_eq!(switched.body["method"], json!("transcode"));
    eventually(|| w.fake.deleted.lock().contains(&"job1".to_owned())).await;

    // A stopped player frees its slot.
    let session = switched.body["session_id"].as_str().unwrap();
    let stop = json!({"position_ms": 1_000, "state": "stopped", "session_id": session});
    app.post(&format!("/api/items/{feature}/progress"), admin, stop).await;
    assert_eq!(play(app, admin, start("tv")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn simultaneous_play_requests_stay_within_the_transcode_limit() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.put("/api/settings", admin, json!({"max_transcodes": 1})).await;
    let feature = w.movies["Feature"];
    let start = |client: &str| json!({"item_id": feature, "caps": browser(), "client_id": client});

    // Creating a job takes a while: all three requests are waiting at once.
    *w.fake.job_delay.lock() = Duration::from_millis(300);
    let (a, b, c) =
        tokio::join!(play(app, admin, start("a")), play(app, admin, start("b")), play(app, admin, start("c")));
    let started: Vec<_> = [&a, &b, &c].into_iter().filter(|r| r.status == StatusCode::OK).collect();
    assert_eq!(started.len(), 1, "{:?}", [a.status, b.status, c.status]);
    let refused = [&a, &b, &c].into_iter().filter(|r| r.status == StatusCode::SERVICE_UNAVAILABLE).count();
    assert_eq!(refused, 2);
    assert_eq!(w.fake.created(), 1, "only one job was asked for");

    // A request whose job can't be created gives its slot back.
    let session = started[0].body["session_id"].as_str().unwrap();
    assert_eq!(app.delete(&format!("/api/playback/{session}"), admin).await.status, StatusCode::NO_CONTENT);
    w.fake.fail_jobs.store(true, Ordering::SeqCst);
    assert_eq!(play(app, admin, start("d")).await.status, StatusCode::INTERNAL_SERVER_ERROR);
    w.fake.fail_jobs.store(false, Ordering::SeqCst);
    assert_eq!(play(app, admin, start("e")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn sessions_belong_to_their_user() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let (bob, _) = app.user(admin, "bob", json!({})).await;
    let (alice, _) = app.user(admin, "alice", json!({})).await;
    let started = play(app, &bob, json!({"item_id": w.movies["Feature"], "caps": browser()})).await;
    let session = started.body["session_id"].as_str().unwrap().to_owned();
    let playlist = format!("/api/playback/{session}/index.m3u8");

    assert_eq!(app.get(&playlist, &alice).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/playback/{session}"), &alice).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get(&playlist, admin).await.status, StatusCode::OK, "administrators may look in");
    assert_eq!(app.get(&playlist, &bob).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/playback/nope/index.m3u8", &bob).await.status, StatusCode::NOT_FOUND);

    let sessions = |token: &str| {
        let app = &w.app;
        let token = token.to_owned();
        async move { app.get("/api/activity", &token).await.body["sessions"].as_array().unwrap().len() }
    };
    assert_eq!((sessions(&bob).await, sessions(&alice).await, sessions(admin).await), (1, 0, 1));
}

#[tokio::test]
async fn apps_never_see_media_paths_in_the_transcoders_words() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let (token, _) = app.app_token(1, "library play").await;
    // The media folder's own name, which JSON writes as it is on every
    // system (unlike the backslashes of a whole path on Windows).
    let movies = app.media_path("movies");
    let folder = std::path::Path::new(&movies).parent().unwrap().file_name().unwrap().to_string_lossy().into_owned();
    let feature = w.movies["Feature"];
    let file = app.file_of(feature).await;
    let start = json!({"item_id": feature, "caps": browser(), "client_id": "phone"});
    let started = play(app, &token, start.clone()).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let session = started.body["session_id"].as_str().unwrap().to_owned();

    // The share holding the files goes to sleep.
    w.fake.unreadable.store(true, Ordering::SeqCst);
    let status = app.get(&format!("/api/playback/{session}"), &token).await;
    assert_eq!(status.body["transcode"]["last_error"], "the conversion failed");
    w.fake.lose_jobs();
    let replies = [
        ("the player's details", status),
        ("a lost job", app.get(&format!("/api/playback/{session}/index.m3u8"), &token).await),
        ("another session", play(app, &token, json!({"item_id": feature, "caps": browser()})).await),
        ("embedded subtitles", app.get(&format!("/api/files/{file}/subtitles/4.vtt"), &token).await),
        ("subtitle files", app.get(&format!("/api/files/{file}/subtitles/1000.vtt"), &token).await),
    ];
    for (case, reply) in &replies[1..] {
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{case}: {}", reply.text());
    }
    for (case, reply) in &replies {
        assert!(!reply.mentions(&folder), "{case}: {}", reply.text());
    }
    // pley's own sign-in gets the transcoder's own words.
    let own = play(app, admin, start).await;
    assert!(own.mentions(&folder), "{}", own.text());
}

#[tokio::test]
async fn subtitles_are_converted_once() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let embedded = format!("/api/files/{file}/subtitles/4.vtt");
    let first = app.get(&embedded, admin).await;
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.headers[header::CONTENT_TYPE], "text/vtt; charset=utf-8");
    assert!(first.text().starts_with("WEBVTT"));
    assert_eq!(app.get(&embedded, admin).await.text(), first.text(), "from the cache");
    assert_eq!(w.fake.subtitles.lock().len(), 1);
    let request = w.fake.subtitles.lock()[0].clone();
    assert!(request.path.ends_with("Feature (2021).mkv"));
    assert_eq!(request.stream_index, Some(4));

    // A sidecar is converted from its own file.
    let sidecar = app.get(&format!("/api/files/{file}/subtitles/1000.vtt"), admin).await;
    assert_eq!(sidecar.status, StatusCode::OK);
    let request = w.fake.subtitles.lock()[1].clone();
    assert!(request.path.ends_with("Feature (2021).en.srt"));
    assert_eq!((request.stream_index, request.partial), (None, false));
    for missing in ["1001.vtt", "4.srt", "x.vtt"] {
        let reply = app.get(&format!("/api/files/{file}/subtitles/{missing}"), admin).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{missing}");
    }
    assert_eq!(w.fake.subtitles.lock().len(), 2);
}

#[tokio::test]
async fn subtitles_come_moved_when_asked() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let track = format!("/api/files/{file}/subtitles/4.vtt");
    let plain = app.get(&track, admin).await.text();
    assert!(plain.contains("00:00.000 --> 00:01.000"), "{plain}");
    // Later or earlier, from the same cached track; never before the start.
    let later = app.get(&format!("{track}?offset_ms=300"), admin).await;
    assert_eq!(later.status, StatusCode::OK);
    assert!(later.text().contains("00:00:00.300 --> 00:00:01.300"), "{}", later.text());
    let earlier = app.get(&format!("{track}?offset_ms=-500"), admin).await.text();
    assert!(earlier.contains("00:00:00.000 --> 00:00:00.500"), "{earlier}");
    assert_eq!(app.get(&format!("{track}?offset_ms=0"), admin).await.text(), plain);
    assert_eq!(w.fake.subtitles.lock().len(), 1, "converted once");
    let sidecar = app.get(&format!("/api/files/{file}/subtitles/1000.vtt?offset_ms=1000"), admin).await.text();
    assert!(sidecar.contains("00:00:01.000 --> 00:00:02.000"), "{sidecar}");
    assert_eq!(app.get(&format!("{track}?offset_ms=soon"), admin).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn picture_subtitles_come_in_files_of_their_own() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let uri = format!("/api/files/{file}/subtitles/3.mks");
    // The PGS track, while the transcoder copies it: what it has copied so
    // far, at once, kept out of caches, with ranges.
    let so_far = app.get(&uri, admin).await;
    assert_eq!(so_far.status, StatusCode::OK, "{}", so_far.text());
    assert_eq!(so_far.headers[header::CONTENT_TYPE], "video/x-matroska");
    assert_eq!(so_far.headers[SUBTITLES_COMPLETE_HEADER], "0");
    assert_eq!(so_far.headers[header::CACHE_CONTROL], "no-store");
    assert!(so_far.bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) && so_far.text().ends_with("so far"));
    assert!(w.fake.tracks.lock()[0].partial, "players don't wait for the whole file");
    // Then all of it: copied once by the transcoder, then from the cache.
    let first = app.get(&uri, admin).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(first.headers[SUBTITLES_COMPLETE_HEADER], "1");
    assert!(first.bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]));
    assert!(first.text().ends_with("Feature (2021).mkv#Some(3)"), "{}", first.text());
    let cached = app.get(&uri, admin).await;
    assert_eq!(cached.bytes, first.bytes, "from the cache");
    assert_eq!(cached.headers[SUBTITLES_COMPLETE_HEADER], "1");
    let ranged = app.call(Method::GET, &uri, Some(admin), None, &[("range", "bytes=0-3")]).await;
    assert_eq!((ranged.status, ranged.bytes.len()), (StatusCode::PARTIAL_CONTENT, 4));
    assert_eq!(w.fake.tracks.lock().len(), 2);
    // Text tracks (in the file or next to it), other streams and unknown
    // indexes: none.
    for missing in ["4.mks", "1000.mks", "0.mks", "1.mks", "77.mks", "x.mks"] {
        let reply = app.get(&format!("/api/files/{file}/subtitles/{missing}"), admin).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{missing}");
    }
    assert_eq!(w.fake.tracks.lock().len(), 2);
    // Apps play it with the play scope.
    let me = app.get("/api/auth/me", admin).await.body["id"].as_i64().unwrap();
    let (browse, _) = app.app_token(me, "library").await;
    let (play, _) = app.app_token(me, "library play").await;
    assert_eq!(app.app_call(Method::GET, &uri, &browse, None).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.app_call(Method::GET, &uri, &play, None).await.bytes, first.bytes);
}

#[tokio::test]
async fn pgs_subtitles_come_as_sup_files_for_kodi() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    // A German PGS file next to the film.
    let sup = app.media_path("movies/Feature (2021)/Feature (2021).de.sup");
    std::fs::write(&sup, b"PG\x00\x00\x00\x00\x00\x00\x00\x00\x80\x00\x00").unwrap();
    let library = app.get(&format!("/api/items/{feature}"), admin).await.body["library_id"].as_i64().unwrap();
    app.post(&format!("/api/libraries/{library}/scan"), admin, json!({})).await;
    app.wait_for_scans().await;
    let file = app.file_of(feature).await;
    let item = app.get(&format!("/api/items/{feature}"), admin).await;
    let streams = item.body["files"][0]["streams"].as_array().unwrap().clone();
    let german = streams.iter().find(|s| s["codec"] == json!("hdmv_pgs_subtitle") && s["external"] == json!(true));
    let index = german.expect("the .sup is a track")["index"].as_i64().unwrap();
    // The file as it is, with ranges, the transcoder not asked.
    let whole = app.get(&format!("/api/files/{file}/subtitles/{index}.sup"), admin).await;
    assert_eq!(whole.status, StatusCode::OK);
    assert_eq!(whole.bytes, std::fs::read(&sup).unwrap());
    let ranged = app
        .call(
            Method::GET,
            &format!("/api/files/{file}/subtitles/{index}.sup"),
            Some(admin),
            None,
            &[("range", "bytes=0-1")],
        )
        .await;
    assert_eq!((ranged.status, &ranged.bytes[..]), (StatusCode::PARTIAL_CONTENT, &b"PG"[..]));
    assert!(w.fake.tracks.lock().is_empty());
    // Anything but PGS (the English SubRip file next to it too): none.
    let text = streams.iter().find(|s| s["codec"] == json!("subrip") && s["external"] == json!(true));
    let text = format!("{}.sup", text.expect("the .srt is a track")["index"]);
    for missing in ["4.sup", text.as_str(), "0.sup", "77.sup", "x.sup"] {
        let reply = app.get(&format!("/api/files/{file}/subtitles/{missing}"), admin).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{missing}");
    }
}

#[tokio::test]
async fn partial_picture_subtitles_come_with_ranges() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let uri = format!("/api/files/{file}/subtitles/3.mks");
    // What follows what the player has, of a file still being copied: its
    // length isn't known yet.
    let rest = app.call(Method::GET, &uri, Some(admin), None, &[("range", "bytes=4-")]).await;
    assert_eq!(rest.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(rest.headers[header::CONTENT_RANGE], "bytes 4-9/*");
    assert_eq!(rest.text(), "so far");
    assert_eq!(rest.headers[SUBTITLES_COMPLETE_HEADER], "0");
}

/// The web app's report: version 1, with the picture subtitles it draws.
fn web_drawing_pictures() -> Value {
    json!({
        "containers": ["mp4", "webm"], "video_codecs": ["h264"], "audio_codecs": ["aac", "mp3"],
        "subtitle_codecs": ["subrip", "ass", "webvtt", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"],
        "picture_subtitles_sideload": true
    })
}

#[tokio::test]
async fn picture_subtitles_a_player_draws_are_copied_as_its_playback_starts() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    // Without its English SubRip file, which the user's choice would pick
    // over the English PGS track (text plays everywhere).
    std::fs::remove_file(app.media_path("movies/Feature (2021)/Feature (2021).en.srt")).unwrap();
    let library = app.get(&format!("/api/items/{feature}"), admin).await.body["library_id"].as_i64().unwrap();
    app.post(&format!("/api/libraries/{library}/scan"), admin, json!({})).await;
    app.wait_for_scans().await;
    let file = app.file_of(feature).await;
    let copies = || w.fake.tracks.lock().iter().filter(|r| r.stream_index == Some(3)).count();
    // Nothing chosen: nothing copied.
    let started = play(app, admin, json!({"item_id": feature, "caps": web_drawing_pictures(), "client_id": "a"})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(copies(), 0);
    // The English PGS track chosen for the title.
    let chosen =
        app.put(&format!("/api/items/{feature}/streams"), admin, json!({"file_id": file, "subtitle_index": 3})).await;
    assert!(chosen.status.is_success(), "{:?}", chosen.body);
    // A player that draws them: repackaged (MKV, AC-3), nothing drawn on
    // the video, the track's copy started with the playback.
    let started = play(app, admin, json!({"item_id": feature, "caps": web_drawing_pictures(), "client_id": "a"})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    assert_eq!((&started.body["method"], &started.body["burn_subtitles"]), (&json!("remux"), &json!(false)));
    assert_eq!(started.body["pts_offset_ms"], json!(1400), "the segments' timestamps: the file's, 1.4 s later");
    eventually(|| copies() == 1).await;
    assert!(w.fake.tracks.lock().iter().all(|r| r.partial && r.path.ends_with("Feature (2021).mkv")));
    // A player that doesn't: nothing more.
    let started = play(app, admin, json!({"item_id": feature, "caps": browser(), "client_id": "b"})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(copies(), 1);
    // The player's first look finds the copy under way, then whole.
    let uri = format!("/api/files/{file}/subtitles/3.mks");
    assert_eq!(app.get(&uri, admin).await.headers[SUBTITLES_COMPLETE_HEADER], "1");
}

#[tokio::test]
async fn embedded_subtitles_come_as_far_as_they_are_read() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let asked = || w.fake.subtitles.lock().iter().filter(|r| r.stream_index == Some(5)).count();
    let slow = format!("/api/files/{file}/subtitles/5.vtt");

    // The cues read so far, at once, kept out of caches: more are coming.
    let first = app.get(&slow, admin).await;
    assert_eq!(first.status, StatusCode::OK);
    assert!(first.text().ends_with("so far\n\n"), "{}", first.text());
    assert_eq!(first.headers[SUBTITLES_COMPLETE_HEADER], "0");
    assert_eq!(first.headers[header::CACHE_CONTROL], "no-store");
    assert!(w.fake.subtitles.lock()[0].partial, "players don't wait for the whole file");

    // Asked again: all of them, cached from then on.
    let all = app.get(&slow, admin).await;
    assert!(all.text().contains("#Some(5)") && !all.headers.contains_key(SUBTITLES_COMPLETE_HEADER));
    assert_eq!(app.get(&slow, admin).await.text(), all.text());
    assert_eq!(asked(), 2, "served from the cache");
}

#[tokio::test]
async fn subtitles_are_cached_whole_and_follow_their_sidecar() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let mtime: i64 = sqlx::query_scalar("SELECT mtime FROM media_files WHERE id = ?")
        .bind(file)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let cache = app.state.config.cache_dir().join("subtitles");
    let converted = || w.fake.subtitles.lock().len();

    // Left cut short by a crash (earlier versions wrote in place): made again.
    let embedded = cache.join(format!("{file}_4_{mtime}.vtt"));
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(&embedded, b"").unwrap();
    let reply = app.get(&format!("/api/files/{file}/subtitles/4.vtt"), admin).await;
    assert!(reply.text().starts_with("WEBVTT"), "{}", reply.text());
    assert_eq!(converted(), 1);
    assert_eq!(std::fs::read_to_string(&embedded).unwrap(), reply.text());

    // A conversion that fails leaves nothing behind.
    let failed = app.get(&format!("/api/files/{file}/subtitles/99.vtt"), admin).await;
    assert!(failed.status.is_server_error(), "{}", failed.status);
    let names: Vec<String> =
        std::fs::read_dir(&cache).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, [format!("{file}_4_{mtime}.vtt")]);

    // A sidecar replaced by a better one (as subtitle downloaders do) is
    // converted again, though its video did not change.
    let sidecar = format!("/api/files/{file}/subtitles/1000.vtt");
    app.get(&sidecar, admin).await;
    app.get(&sidecar, admin).await;
    assert_eq!(converted(), 3, "the failed one, then the sidecar once");
    let srt = app.media.path().join("movies/Feature (2021)/Feature (2021).en.srt");
    std::fs::write(&srt, b"1\n00:00:00,000 --> 00:00:01,000\nBetter\n").unwrap();
    let later = std::time::SystemTime::now() + Duration::from_secs(10);
    std::fs::File::options().write(true).open(&srt).unwrap().set_modified(later).unwrap();
    assert_eq!(app.get(&sidecar, admin).await.status, StatusCode::OK);
    assert_eq!(converted(), 4);
}

#[tokio::test]
async fn subtitle_files_are_converted_once_and_forgotten_when_done() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let file = app.file_of(w.movies["Feature"]).await;
    let sidecar = format!("/api/files/{file}/subtitles/1000.vtt");
    let converted = || w.fake.subtitles.lock().iter().filter(|r| r.stream_index.is_none()).count();

    // Two players ask at once and share a conversion; both leave (the track
    // changed) once it's under way, and it fails.
    *w.fake.sidecar_delay.lock() = Duration::from_millis(400);
    w.fake.sidecar_fails.store(true, Ordering::SeqCst);
    tokio::select! {
        _ = async { tokio::join!(app.get(&sidecar, admin), app.get(&sidecar, admin)) } => {}
        () = eventually(|| converted() == 1) => {}
    }
    eventually(|| w.fake.sidecar_answers.load(Ordering::SeqCst) == 1).await;
    assert_eq!(converted(), 1, "one conversion for both");

    // Asked again once it has failed, it's converted again rather than
    // answered with that failure (only a request at the very moment it fails
    // may still share it), and kept from then on.
    *w.fake.sidecar_delay.lock() = Duration::ZERO;
    let mut again = app.get(&sidecar, admin).await;
    for _ in 0..100 {
        if again.status == StatusCode::OK {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        again = app.get(&sidecar, admin).await;
    }
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(app.get(&sidecar, admin).await.text(), again.text());
    assert_eq!(converted(), 2);
}

#[tokio::test]
async fn image_subtitles_and_other_tracks_need_a_transcode() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let burned = play(app, admin, json!({"item_id": feature, "caps": browser(), "subtitle_index": 3})).await;
    assert_eq!(burned.status, StatusCode::OK, "{:?}", burned.body);
    assert_eq!((&burned.body["method"], &burned.body["burn_subtitles"]), (&json!("transcode"), &json!(true)));
    assert_eq!(burned.body["subtitle_index"], json!(3));
    let job = w.fake.last_request();
    assert_eq!(job.subtitle.as_ref().and_then(|s| s.stream_index), Some(3));
    assert_eq!(job.video.unwrap().mode, VideoMode::Transcode, "burning needs new frames");

    // The French AAC track plays in a remux with the audio copied.
    let french = play(app, admin, json!({"item_id": feature, "caps": browser(), "audio_index": 2})).await;
    assert_eq!((french.body["method"].clone(), french.body["audio_index"].clone()), (json!("remux"), json!(2)));
    assert_eq!(french.body["audio_converted"], json!(false));
    let audio = w.fake.last_request().audio.unwrap();
    assert_eq!((audio.mode, audio.stream_index), (AudioMode::Copy, Some(2)));
    // And starting later starts the transcoder there.
    play(app, admin, json!({"item_id": feature, "caps": browser(), "start_ms": 42_000})).await;
    assert_eq!(w.fake.last_request().start_ms, Some(42_000));
}

/// A play request that names no audio track plays the one the user picked
/// (the item's `selected_audio`, what the player shows as chosen), not the
/// file's default: a player that plays its pick as it is would otherwise be
/// answered for the default track, and ask again (a job made for nothing).
#[tokio::test]
async fn the_audio_the_user_picked_plays_unless_another_is_named() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let file = app.file_of(feature).await;
    let choice = json!({"file_id": file, "audio_index": 2, "subtitle_index": null});
    assert_eq!(app.put(&format!("/api/items/{feature}/streams"), admin, choice).await.status, StatusCode::NO_CONTENT);
    let detail = app.get(&format!("/api/items/{feature}"), admin).await;
    assert_eq!(detail.body["files"][0]["selected_audio"], json!(2));

    let started = play(app, admin, json!({"item_id": feature, "caps": browser()})).await;
    assert_eq!((&started.body["method"], &started.body["audio_index"]), (&json!("remux"), &json!(2)));
    assert_eq!(w.fake.last_request().audio.unwrap().stream_index, Some(2));
    // A track named still wins.
    let english = play(app, admin, json!({"item_id": feature, "caps": browser(), "audio_index": 1})).await;
    assert_eq!(english.body["audio_index"], json!(1));
    assert_eq!(w.fake.last_request().audio.unwrap().stream_index, Some(1));
}

/// Without a track named, the server plays the one of the user's language
/// that the player plays best: Chrome the stereo AAC track (copied into a
/// stream, rather than the TrueHD converted), a TV with a 5.1 soundbar its
/// Dolby Digital as it is. Subtitles chosen alone leave that so; a track
/// picked is played on every player that plays it, until audio is chosen
/// as `null` (automatic) again.
#[tokio::test]
async fn the_audio_a_player_plays_best_is_chosen_for_it() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.media_file("lossless/Lossless (2022)/Lossless (2022).mkv", 5120);
    let library = app.library(admin, "Lossless", "movies", "lossless").await;
    let film = app.ids(library, admin).await["Lossless"];
    let file = app.file_of(film).await;
    let soundbar = json!({
        "containers": ["mkv", "mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac", "ac3", "eac3"],
        "caps_version": 2, "audio_decode": ["aac"], "audio_passthrough": ["ac3", "eac3"], "audio_channels": 6,
        "audio_track_switching": true
    });
    let chosen = async |caps: &Value| {
        let started = play(app, admin, json!({"item_id": film, "caps": caps})).await;
        assert_eq!(started.status, StatusCode::OK, "{}", started.body);
        (started.body["method"].as_str().unwrap_or_default().to_owned(), started.body["audio_index"].clone())
    };
    assert_eq!(chosen(&browser()).await, ("remux".to_owned(), json!(3)));
    assert_eq!(w.fake.last_request().audio.unwrap().mode, AudioMode::Copy, "copied, not converted");
    assert_eq!(chosen(&soundbar).await, ("direct".to_owned(), json!(2)));
    // The item's details don't know the player: its default, as before, and not a pick.
    let picked = async || {
        let files = app.get(&format!("/api/items/{film}"), admin).await.body["files"].clone();
        (files[0]["selected_audio"].clone(), files[0]["audio_picked"].clone())
    };
    assert_eq!(picked().await, (Value::Null, json!(false)));

    let choose = async |choice: Value| {
        let put = app.put(&format!("/api/items/{film}/streams"), admin, choice).await;
        assert_eq!(put.status, StatusCode::NO_CONTENT, "{}", put.body);
    };
    // Subtitles alone: the audio stays the player's best.
    choose(json!({"file_id": file, "subtitle_index": 4})).await;
    assert_eq!(chosen(&soundbar).await.1, json!(2));
    assert_eq!(picked().await.1, json!(false));
    // The stereo track picked: played on the soundbar too, and subtitles chosen later keep it.
    choose(json!({"file_id": file, "audio_index": 3, "subtitle_index": 4})).await;
    assert_eq!(chosen(&soundbar).await, ("direct".to_owned(), json!(3)));
    choose(json!({"file_id": file, "subtitle_index": null})).await;
    assert_eq!(chosen(&soundbar).await.1, json!(3));
    assert_eq!(picked().await, (json!(3), json!(true)));
    // Audio chosen as `null`: automatic again.
    choose(json!({"file_id": file, "audio_index": null, "subtitle_index": null})).await;
    assert_eq!(chosen(&soundbar).await.1, json!(2));
    assert_eq!(picked().await.1, json!(false));
}

#[tokio::test]
async fn frame_grabs_are_made_once_per_file() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let thumb = format!("/api/items/{}/images/thumb?w=320", w.movies["Clip"]);
    let first = app.get(&thumb, admin).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(first.headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(app.get(&thumb, admin).await.status, StatusCode::OK);
    assert_eq!(w.fake.thumbnails.load(Ordering::SeqCst), 1, "kept in the file's media folder");
}

#[tokio::test]
async fn plex_clients_stream_through_the_universal_transcoder() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let file = app.file_of(feature).await;
    let path = encode(&format!("/library/metadata/{feature}"));
    let client = [("x-plex-client-identifier", "living-room"), ("x-plex-product", "Plex for TV")];

    let start = format!(
        "/video/:/transcode/universal/start.m3u8?path={path}&offset=30&maxVideoBitrate=2000&videoResolution=1280x720&audioStreamID={}",
        file * 10_000 + 2
    );
    let master = app.plex(Method::GET, &start, admin, &client).await;
    assert_eq!(master.status, StatusCode::OK, "{}", master.text());
    let text = master.text();
    assert!(text.contains("BANDWIDTH=2000000,RESOLUTION=1280x720"), "{text}");
    let variant = text.lines().find(|l| l.starts_with("session/")).unwrap().to_owned();
    let job = w.fake.last_request();
    assert_eq!(job.start_ms, Some(30_000));
    let video = job.video.unwrap();
    assert_eq!((video.mode, video.max_height, video.max_bitrate_kbps), (VideoMode::Transcode, Some(720), Some(1_808)));
    assert_eq!(job.audio.unwrap().stream_index, Some(2), "Plex stream ids map to file streams");

    // The session URLs need no token.
    let base = format!("/video/:/transcode/universal/{}", variant.trim_end_matches("index.m3u8"));
    let playlist = app.plex(Method::GET, &format!("{base}index.m3u8"), "", &[]).await;
    assert_eq!(playlist.status, StatusCode::OK);
    let segment = app.plex(Method::GET, &format!("{base}0.ts"), "", &[]).await;
    assert_eq!((segment.status, segment.text()), (StatusCode::OK, "job1:0".to_owned()));

    // It shows up as a transcode session.
    let sessions = app.plex_json("/transcode/sessions", admin).await;
    assert_eq!(sessions["size"], json!(1));
    assert_eq!(sessions["TranscodeSession"]["videoDecision"], json!("transcode"));

    // directStream=0 forces transcoding where a remux would do.
    let plain = format!("/video/:/transcode/universal/start.m3u8?path={path}&directStream=0");
    app.plex(Method::GET, &plain, admin, &[("x-plex-client-identifier", "phone")]).await;
    assert_eq!(w.fake.last_request().video.unwrap().mode, VideoMode::Transcode);
    let plain = format!("/video/:/transcode/universal/start.m3u8?path={path}");
    app.plex(Method::GET, &plain, admin, &[("x-plex-client-identifier", "tablet")]).await;
    assert_eq!(w.fake.last_request().video.unwrap().mode, VideoMode::Copy);

    // Stopping by client ends that client's session and its job.
    let stop = app.plex(Method::GET, "/video/:/transcode/universal/stop?session=living-room", admin, &[]).await;
    assert_eq!(stop.status, StatusCode::OK);
    eventually(|| w.fake.deleted.lock().contains(&"job1".to_owned())).await;
    assert_eq!(app.plex(Method::GET, &format!("{base}index.m3u8"), "", &[]).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn seek_previews_are_made_once_per_file() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.put("/api/settings", admin, json!({"seek_previews": true})).await;
    let clip = app.file_of(w.movies["Clip"]).await;
    let previews = format!("/api/files/{clip}/trickplay");

    // Asking queues them; they are fetched from the transcoder in the background.
    assert_eq!(app.get(&previews, admin).await.status, StatusCode::NOT_FOUND, "being made");
    let mut info = Value::Null;
    for _ in 0..200 {
        let reply = app.get(&previews, admin).await;
        if reply.status == StatusCode::OK {
            info = reply.body;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!((&info["count"], &info["columns"], &info["sheets"]), (&json!(6), &json!(5), &json!(1)), "{info}");
    let sheet = app.get(&format!("{previews}/0.jpg?v={}", info["id"].as_str().unwrap()), admin).await;
    assert_eq!(sheet.status, StatusCode::OK);
    assert_eq!(sheet.headers[header::CONTENT_TYPE], "image/jpeg");
    assert!(sheet.headers[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));
    assert_eq!(app.get(&format!("{previews}/1.jpg"), admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("{previews}/x.png"), admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.fake.previews.lock().len(), 1, "made once");

    // A file the transcoder can't do is not asked for again (until a restart).
    let feature = app.file_of(w.movies["Feature"]).await;
    app.get(&format!("/api/files/{feature}/trickplay"), admin).await;
    eventually(|| w.fake.previews.lock().len() == 2).await;
    eventually(|| app.state.trickplay.pending() == 0).await;
    let again = app.get(&format!("/api/files/{feature}/trickplay"), admin).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    assert_eq!(app.state.trickplay.pending(), 0, "not queued again");
    // Files ffprobe couldn't read have no video to preview.
    let broken = app.file_of(w.movies["Broken"]).await;
    assert_eq!(app.get(&format!("/api/files/{broken}/trickplay"), admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.state.trickplay.pending(), 0);
    assert_eq!(w.fake.previews.lock().len(), 2);
}

#[tokio::test]
async fn seek_previews_wait_while_a_playback_starts() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.put("/api/settings", admin, json!({"seek_previews": true})).await;
    let clip = app.file_of(w.movies["Clip"]).await;
    let previews = format!("/api/files/{clip}/trickplay");
    *w.fake.preview_delay.lock() = Duration::from_secs(60);

    // Opening the clip starts its previews...
    assert_eq!(app.get(&format!("/api/items/{}", w.movies["Clip"]), admin).await.status, StatusCode::OK);
    eventually(|| w.fake.previews.lock().len() == 1).await;
    // ...which a playback starting stops: the transcoder's request is dropped.
    let started =
        play(app, admin, json!({"item_id": w.movies["Feature"], "caps": browser(), "client_id": "tab"})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    eventually(|| w.fake.previews_dropped.load(Ordering::SeqCst) == 1).await;
    assert_eq!(app.state.trickplay.pending(), 1, "still to be made, not failed");

    // While the playback starts, nothing is made, even when asked again.
    assert_eq!(app.get(&previews, admin).await.status, StatusCode::NOT_FOUND);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.fake.previews.lock().len(), 1);

    // Once it is over (or 30 s in), they are made after all.
    let session = started.body["session_id"].as_str().unwrap();
    assert_eq!(app.delete(&format!("/api/playback/{session}"), admin).await.status, StatusCode::NO_CONTENT);
    eventually(|| w.fake.previews.lock().len() == 2).await;
    let mut made = false;
    for _ in 0..200 {
        if app.get(&previews, admin).await.status == StatusCode::OK {
            made = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(made, "the previews were made");
    assert_eq!(w.fake.previews_dropped.load(Ordering::SeqCst), 1);
    assert_eq!(app.state.trickplay.pending(), 0);
}

#[tokio::test]
async fn seek_previews_wait_while_something_is_transcoded() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.put("/api/settings", admin, json!({"seek_previews": true})).await;
    let clip = app.file_of(w.movies["Clip"]).await;
    // A download being converted, say: no playback, but FFmpeg transcodes.
    w.fake.transcoding.store(1, Ordering::SeqCst);

    // Opening the clip queues its previews, which wait.
    assert_eq!(app.get(&format!("/api/items/{}", w.movies["Clip"]), admin).await.status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(w.fake.previews.lock().is_empty(), "nothing is made while it transcodes");
    assert_eq!(app.state.trickplay.pending(), 1);

    // Once it is done, they are made.
    w.fake.transcoding.store(0, Ordering::SeqCst);
    let asked = std::time::Instant::now();
    while app.get(&format!("/api/files/{clip}/trickplay"), admin).await.status != StatusCode::OK {
        assert!(asked.elapsed() < pley::transcoder::TRANSCODING_LOOK * 3, "the previews were made");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(w.fake.previews.lock().len(), 1);
}

#[tokio::test]
async fn seek_previews_stop_as_soon_as_play_is_pressed() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    app.put("/api/settings", admin, json!({"seek_previews": true})).await;
    *w.fake.preview_delay.lock() = Duration::from_secs(60);
    assert_eq!(app.get(&format!("/api/items/{}", w.movies["Clip"]), admin).await.status, StatusCode::OK);
    eventually(|| w.fake.previews.lock().len() == 1).await;

    // Creating the transcode reads the file too: the previews stop before it
    // begins, not once the session exists.
    *w.fake.job_delay.lock() = Duration::from_secs(1);
    let body = json!({"item_id": w.movies["Feature"], "caps": browser(), "client_id": "tab"});
    let request = app.post("/api/playback", admin, body);
    tokio::pin!(request);
    let dropped = async {
        while w.fake.previews_dropped.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    tokio::select! {
        reply = &mut request => panic!("answered before the previews stopped: {:?}", reply.body),
        () = dropped => {}
    }
    assert_eq!(w.fake.created(), 0, "the job is still being created");
    let started = request.await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    assert_eq!(app.state.trickplay.pending(), 1, "still to be made");
    assert!(w.fake.holds.lock().is_empty(), "the transcoder holds for its own jobs");
}

#[tokio::test]
async fn direct_plays_hold_the_transcoders_background_work() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let clip = play(app, admin, json!({"item_id": w.movies["Clip"], "caps": browser(), "client_id": "tv"})).await;
    assert_eq!((clip.status, clip.body["method"].clone()), (StatusCode::OK, json!("direct")));
    assert_eq!(clip.body["pts_offset_ms"], Value::Null, "the file's own times");
    // Said all the same: Kodi takes it for a server that makes .sup files.
    assert!(clip.body.as_object().is_some_and(|b| b.contains_key("pts_offset_ms")));
    eventually(|| w.fake.holds.lock().len() == 1).await;
    assert_eq!(w.fake.holds.lock()[0], 10_000);
}

#[tokio::test]
async fn titles_about_to_play_are_read_ahead_where_play_starts() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    *w.fake.prefetch_delay.lock() = Duration::from_secs(5);
    let (clip, feature, broken) = (w.movies["Clip"], w.movies["Feature"], w.movies["Broken"]);
    let progress = json!({"position_ms": 30_000, "duration_ms": 60_000, "state": "stopped"});
    assert_eq!(app.post(&format!("/api/items/{clip}/progress"), admin, progress).await.status, StatusCode::NO_CONTENT);
    let read = |n: usize| w.fake.prefetches.lock().get(n).map(|r| (r.path.clone(), r.start_ms));

    // Opening a title reads it ahead from where Play resumes, without waiting.
    let asked = std::time::Instant::now();
    assert_eq!(app.get(&format!("/api/items/{clip}"), admin).await.status, StatusCode::OK);
    assert!(asked.elapsed() < Duration::from_secs(2), "the page never waits for it");
    eventually(|| w.fake.prefetches.lock().len() == 1).await;
    let (path, start) = read(0).unwrap();
    assert!(path.ends_with("Clip (2020).mp4"), "{path}");
    assert_eq!(start, Some(30_000));
    // Once every two minutes per file.
    assert_eq!(app.get(&format!("/api/items/{clip}"), admin).await.status, StatusCode::OK);
    assert_eq!(app.post(&format!("/api/items/{clip}/prefetch"), admin, json!({})).await.status, StatusCode::ACCEPTED);

    // A card's Play button: from the start for a title never played.
    let asked = std::time::Instant::now();
    let reply = app.post(&format!("/api/items/{feature}/prefetch"), admin, json!({})).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    assert!(asked.elapsed() < Duration::from_secs(2), "answered at once");
    eventually(|| w.fake.prefetches.lock().len() == 2).await;
    let (path, start) = read(1).unwrap();
    assert!(path.ends_with("Feature (2021).mkv"), "{path}");
    assert_eq!(start, Some(0));

    // Files without video (ffprobe couldn't read this one) are left alone;
    // titles that aren't there aren't found.
    assert_eq!(app.post(&format!("/api/items/{broken}/prefetch"), admin, json!({})).await.status, StatusCode::ACCEPTED);
    assert_eq!(app.post("/api/items/999999/prefetch", admin, json!({})).await.status, StatusCode::NOT_FOUND);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.fake.prefetches.lock().len(), 2);
}

#[tokio::test]
async fn added_subtitles_play_and_convert_as_subtitle_files_do() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let feature = w.movies["Feature"];
    let file = app.file_of(feature).await;
    let boundary = "x-boundary";
    let mut body = Vec::new();
    for (name, filename, data) in
        [("file", Some("Feature.srt"), "1\n00:00:01,000 --> 00:00:02,000\nHi\n"), ("language", None, "de")]
    {
        body.extend(format!("--{boundary}\r\n").bytes());
        let disposition = match filename {
            Some(f) => format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\n\r\n"),
            None => format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n"),
        };
        body.extend(disposition.bytes());
        body.extend(data.bytes());
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").bytes());
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let added =
        app.send_body(Method::POST, &format!("/api/items/{feature}/subtitles"), admin, &content_type, body).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());

    // Listed with the file's own tracks and its sidecar, last; picked, it
    // is the player's to show.
    let started =
        play(app, admin, json!({"item_id": feature, "caps": browser(), "client_id": "tab", "subtitle_index": 9000}))
            .await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    assert_eq!(started.body["burn_subtitles"], json!(false));
    let subtitles = started.body["subtitles"].as_array().unwrap();
    let last = subtitles.last().unwrap();
    assert_eq!((&last["index"], &last["language"], &last["added"]), (&json!(9000), &json!("ger"), &json!(true)));
    assert_eq!(last["deletable"], json!(true));
    assert!(subtitles.iter().filter(|s| s["index"] != json!(9000)).all(|s| s["added"] == json!(false)));
    assert!(subtitles.iter().all(|s| s.get("deletable").is_none() || s["index"] == json!(9000)));

    // Converted to WebVTT from its file in the data folder, once.
    let vtt = app.get(&format!("/api/files/{file}/subtitles/9000.vtt"), admin).await;
    assert_eq!(vtt.status, StatusCode::OK, "{}", vtt.text());
    let request = w.fake.subtitles.lock().last().cloned().unwrap();
    let kept = app.state.config.added_subtitles_dir().join(file.to_string());
    assert!(std::path::Path::new(&request.path).starts_with(&kept), "{}", request.path);
    assert_eq!(request.stream_index, None);
    assert_eq!(app.get(&format!("/api/files/{file}/subtitles/9000.vtt"), admin).await.text(), vtt.text());
    assert_eq!(app.get(&format!("/api/files/{file}/subtitles/9001.vtt"), admin).await.status, StatusCode::NOT_FOUND);
}
