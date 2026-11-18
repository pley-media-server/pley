//! `.strm` files (src/strm.rs): pley with a stand-in transcoder, a library of
//! `.strm` files and a stand-in for the addresses they name. The stand-in
//! transcoder reads what it is given to probe, as FFprobe would, so a probe
//! goes all the way round: transcoder → pley's own address → the address
//! the file names.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use pley_transcoder_api::{
    API_VERSION, Capabilities, ConversionStatus, ErrorBody, JobCreated, JobRequest, Limits, MediaInfo, MediaStream,
    ProbeRequest, SegmentFormat, StreamKind, SubtitleRequest, TranscoderInfo, TranscoderStatus, VideoPlan,
};
use serde_json::{Value, json};

use common::TestApp;

const TOKEN: &str = "shared-secret";
/// What the addresses carry that must never leave the server.
const SECRET: &str = "s3cr3t-t0ken";

/// The media the addresses give: Matroska's first bytes, then numbered ones.
fn media() -> Vec<u8> {
    let mut bytes = vec![0x1A, 0x45, 0xDF, 0xA3];
    bytes.extend((4..150_000u32).map(|n| (n % 251) as u8));
    bytes
}

// ------------------------------------------------------------------ the addresses

#[derive(Clone, Default)]
struct Remote {
    /// Requests for the media, and for the address that redirects to it.
    asked: Arc<AtomicUsize>,
    redirected: Arc<AtomicUsize>,
    /// The headers of the last request for the media.
    headers: Arc<Mutex<HeaderMap>>,
}

fn ranged(bytes: &[u8], headers: &HeaderMap) -> Response {
    let len = bytes.len() as u64;
    let asked = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("bytes="));
    let Some((first, last)) = asked.and_then(|span| span.split_once('-')) else {
        return (
            [(header::ACCEPT_RANGES, "bytes".to_owned()), (header::CONTENT_TYPE, "text/html".to_owned())],
            bytes.to_vec(),
        )
            .into_response();
    };
    let first: u64 = first.parse().unwrap();
    if first >= len {
        return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{len}"))])
            .into_response();
    }
    let last = last.parse::<u64>().map_or(len - 1, |last| last.min(len - 1));
    (
        StatusCode::PARTIAL_CONTENT,
        // What the address says its media is must not be what players are told.
        [
            (header::CONTENT_RANGE, format!("bytes {first}-{last}/{len}")),
            (header::CONTENT_TYPE, "text/html".to_owned()),
        ],
        bytes[first as usize..=last as usize].to_vec(),
    )
        .into_response()
}

async fn start_remote() -> (Remote, String) {
    let remote = Remote::default();
    let film = |State(remote): State<Remote>, headers: HeaderMap| async move {
        remote.asked.fetch_add(1, Ordering::SeqCst);
        *remote.headers.lock() = headers.clone();
        ranged(&media(), &headers)
    };
    let app = Router::new()
        .route("/dl/{token}/film.mkv", get(film))
        .route(
            "/resolve/{token}",
            get(|State(remote): State<Remote>| async move {
                remote.redirected.fetch_add(1, Ordering::SeqCst);
                (StatusCode::FOUND, [(header::LOCATION, format!("/dl/{SECRET}/film.mkv"))])
            }),
        )
        .route("/page", get(|headers: HeaderMap| async move { ranged(b"<html>not a film</html>", &headers) }))
        .with_state(remote.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (remote, base)
}

// ------------------------------------------------------------------ the stand-in transcoder

#[derive(Clone, Default)]
struct Fake {
    http: reqwest::Client,
    /// It is too old to read an address (`remote_inputs`).
    old: Arc<AtomicBool>,
    probes: Arc<Mutex<Vec<String>>>,
    jobs: Arc<Mutex<Vec<JobRequest>>>,
    conversions: Arc<Mutex<Vec<JobRequest>>>,
    subtitles: Arc<Mutex<Vec<SubtitleRequest>>>,
    /// Anything else asked of it: frame grabs, seek previews, read-ahead…
    others: Arc<Mutex<Vec<String>>>,
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(ErrorBody { code: code.into(), message: format!("{code} (stand-in)") })).into_response()
}

fn probed() -> MediaInfo {
    let stream = |index, kind, codec: &str| MediaStream { index, kind, codec: codec.into(), ..Default::default() };
    MediaInfo {
        container: Some("mkv".into()),
        format_name: Some("matroska,webm".into()),
        duration_ms: Some(60_000),
        start_ms: Some(0),
        streams: vec![
            MediaStream {
                width: Some(1920),
                height: Some(1080),
                bit_depth: Some(8),
                ..stream(0, StreamKind::Video, "h264")
            },
            MediaStream { channels: Some(2), default: true, ..stream(1, StreamKind::Audio, "aac") },
            MediaStream { text_based: Some(true), ..stream(2, StreamKind::Subtitle, "subrip") },
        ],
        ..Default::default()
    }
}

async fn start_fake() -> (Fake, String) {
    let fake = Fake::default();
    let app = Router::new()
        .route(
            "/v1/info",
            get(|State(fake): State<Fake>| async move {
                Json(TranscoderInfo {
                    name: "pley-transcoder".into(),
                    version: "9.9.9".into(),
                    api_version: API_VERSION,
                    capabilities: Capabilities {
                        ffmpeg_available: true,
                        ffprobe_available: true,
                        conversions: true,
                        conversions_mp4: true,
                        remote_inputs: !fake.old.load(Ordering::SeqCst),
                        ..Default::default()
                    },
                    limits: Limits { max_jobs: 8, max_hw_jobs: 0 },
                    ..Default::default()
                })
            }),
        )
        .route("/v1/status", get(|| async { Json(TranscoderStatus { max_jobs: 8, ..Default::default() }) }))
        .route(
            "/v1/probe",
            // As FFprobe: it reads the start of what it is given, and finds
            // media there or not.
            post(|State(fake): State<Fake>, Json(req): Json<ProbeRequest>| async move {
                fake.probes.lock().push(req.path.clone());
                if !req.path.starts_with("http://") {
                    return error(StatusCode::NOT_FOUND, "not_found");
                }
                let read = fake.http.get(&req.path).header(header::RANGE, "bytes=0-3").send().await;
                match read {
                    Ok(answer) if answer.status().is_success() => match answer.bytes().await {
                        Ok(bytes) if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) => Json(probed()).into_response(),
                        _ => error(StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed"),
                    },
                    _ => error(StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed"),
                }
            }),
        )
        .route(
            "/v1/jobs",
            post(|State(fake): State<Fake>, Json(req): Json<JobRequest>| async move {
                let video = req.video.as_ref().map(|v| VideoPlan { mode: v.mode, ..Default::default() });
                fake.jobs.lock().push(req);
                Json(JobCreated {
                    id: "job1".into(),
                    duration_ms: 60_000,
                    segment_seconds: 4.0,
                    segment_count: 15,
                    video,
                    audio: None,
                    warnings: vec![],
                    variants: 0,
                    segment_format: SegmentFormat::Ts,
                })
            }),
        )
        .route("/v1/jobs/{id}", axum::routing::delete(|| async { StatusCode::NO_CONTENT }))
        .route(
            "/v1/conversions",
            post(|State(fake): State<Fake>, Json(req): Json<JobRequest>| async move {
                fake.conversions.lock().push(req);
                Json(ConversionStatus { id: "0123456789abcdef0123456789abcdef".into(), ..Default::default() })
            }),
        )
        .route(
            "/v1/subtitles",
            post(|State(fake): State<Fake>, Json(req): Json<SubtitleRequest>| async move {
                fake.subtitles.lock().push(req);
                (
                    [(header::CONTENT_TYPE, "text/vtt"), (header::HeaderName::from_static("x-pley-complete"), "1")],
                    "WEBVTT\n\n00:01.000 --> 00:02.000\nHello\n",
                )
            }),
        )
        .route("/v1/hold", post(|| async { StatusCode::NO_CONTENT }))
        .fallback(|State(fake): State<Fake>, request: Request| async move {
            fake.others.lock().push(format!("{} {}", request.method(), request.uri().path()));
            error(StatusCode::NOT_FOUND, "not_found")
        })
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, base)
}

// ------------------------------------------------------------------ pley

struct World {
    app: TestApp,
    fake: Fake,
    remote: Remote,
    /// Where the addresses are, and where pley answers the transcoder.
    remote_base: String,
    own: String,
    admin: String,
    library: i64,
    movies: HashMap<String, i64>,
}

impl World {
    fn strm(&self, rel: &str, text: &str) {
        let path = self.app.media.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    async fn scan(&mut self, force: bool) {
        let scan = format!("/api/libraries/{}/scan", self.library);
        self.app.post(&scan, &self.admin, json!({"force": force})).await;
        self.app.wait_for_scans().await;
        self.movies = self.app.ids(self.library, &self.admin).await;
    }

    /// The item's file as its page shows it. Opening the page looks at a
    /// `.strm` file's media when it isn't known yet.
    async fn file(&self, title: &str) -> Value {
        let item = self.app.get(&format!("/api/items/{}", self.movies[title]), &self.admin).await;
        assert_eq!(item.status, StatusCode::OK, "{}", item.body);
        item.body["files"][0].clone()
    }

    /// What is kept of the item's file: its container, its media's size and length.
    async fn kept(&self, title: &str) -> (Option<String>, i64, Option<i64>) {
        sqlx::query_as("SELECT container, size, duration_ms FROM media_files WHERE item_id = ?")
            .bind(self.movies[title])
            .fetch_one(&self.app.state.db.read)
            .await
            .unwrap()
    }

    async fn play(&self, title: &str, caps: Value) -> common::Reply {
        self.app.post("/api/playback", &self.admin, json!({"item_id": self.movies[title], "caps": caps})).await
    }
}

/// pley, listening where the transcoder can reach it, with a movie library
/// holding "Film" (a `.strm` file naming the media, with Kodi's headers and
/// a sidecar subtitle) and whatever `more` writes.
async fn world_with(more: impl FnOnce(&std::path::Path, &str)) -> World {
    let (fake, transcoder) = start_fake().await;
    let (remote, remote_base) = start_remote().await;
    let app = TestApp::with_config(|c| {
        c.transcoder_url = Some(transcoder);
        c.transcoder_token = Some(TOKEN.into());
    })
    .await;
    app.state.transcoder.start();
    assert!(app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let own = format!("http://{}", listener.local_addr().unwrap());
    let router = app.api.clone();
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap()
    });
    app.state.strm.listen_at(&own);
    let admin = app.setup_admin().await;
    let movies = app.media.path().join("movies");
    std::fs::create_dir_all(movies.join("Film (2020)")).unwrap();
    std::fs::write(
        movies.join("Film (2020)/Film (2020).strm"),
        format!("#EXTM3U\r\n{remote_base}/dl/{SECRET}/film.mkv|User-Agent=Kodi%2F21&Referer={remote_base}/\r\n"),
    )
    .unwrap();
    app.media_file("movies/Film (2020)/Film (2020).en.srt", 64);
    more(&movies, &remote_base);
    let library = app.library(&admin, "Movies", "movies", "movies").await;
    let movies = app.ids(library, &admin).await;
    World { app, fake, remote, remote_base, own, admin, library, movies }
}

async fn world() -> World {
    world_with(|_, _| {}).await
}

/// A player that plays the media as it is, and one that plays no Matroska.
fn plays_mkv() -> Value {
    json!({"containers": ["mkv", "mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac"]})
}

fn plays_mp4() -> Value {
    json!({"containers": ["mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac"]})
}

#[tokio::test]
async fn a_scan_lists_a_strm_file_without_asking_its_address() {
    let w = world_with(|movies, remote| {
        // Only movie and show libraries read them.
        std::fs::create_dir_all(movies.join("../music/Artist/Album")).unwrap();
        std::fs::write(movies.join("../music/Artist/Album/01 Song.strm"), format!("{remote}/dl/x/film.mkv")).unwrap();
    })
    .await;
    assert_eq!(w.movies.keys().collect::<Vec<_>>(), ["Film"]);
    assert_eq!(w.kept("Film").await, (Some("strm".into()), 0, None), "what its media is isn't known yet");
    let music = w.app.library(&w.admin, "Music", "music", "music").await;
    assert!(w.app.ids(music, &w.admin).await.is_empty());
    // Listing it, and asking for its pictures, asks nothing either.
    let listed = w.app.get(&format!("/api/libraries/{}/items", w.library), &w.admin).await;
    assert_eq!(listed.body["items"][0]["title"], json!("Film"));
    let thumb = w.app.get(&format!("/api/items/{}/images/thumb", w.movies["Film"]), &w.admin).await;
    assert_eq!(thumb.status, StatusCode::NOT_FOUND);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.remote.asked.load(Ordering::SeqCst), 0, "nothing asked of the address");
    assert!(w.fake.probes.lock().is_empty(), "nor of the transcoder");
    assert!(w.fake.others.lock().is_empty(), "{:?}", w.fake.others.lock());

    // Opening it finds what its media is, once: its page shows the tracks
    // and the size, with the subtitle file beside it.
    let file = w.file("Film").await;
    assert_eq!(
        (&file["container"], &file["video_codec"], &file["size"]),
        (&json!("mkv"), &json!("h264"), &json!(media().len()))
    );
    assert!(file["path"].as_str().unwrap().ends_with("Film (2020).strm"));
    let streams = file["streams"].as_array().unwrap();
    assert_eq!((streams.len(), &streams[3]["external"]), (4, &json!(true)));
    w.file("Film").await;
    assert_eq!(w.fake.probes.lock().len(), 1);
    assert!(w.fake.others.lock().is_empty(), "{:?}", w.fake.others.lock());
}

#[tokio::test]
async fn playing_it_finds_what_its_media_is_once_and_plays_it_through_pley() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let started = w.play("Film", plays_mkv()).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.body);
    let file_id = app.file_of(w.movies["Film"]).await;
    assert_eq!(
        (&started.body["method"], &started.body["url"], &started.body["duration_ms"]),
        (&json!("direct"), &json!(format!("/api/files/{file_id}/stream")), &json!(60_000))
    );
    // The transcoder probed pley's own address for it, which says nothing
    // of the one the file names.
    let probes = w.fake.probes.lock().clone();
    assert_eq!(probes.len(), 1);
    assert!(probes[0].starts_with(&format!("{}/api/strm/", w.own)) && probes[0].ends_with("/media.mkv"), "{probes:?}");
    assert!(!probes[0].contains(SECRET) && !probes[0].contains("film") && !probes[0].contains(&w.remote_base));
    // The address was asked as the file says to.
    let asked_with = w.remote.headers.lock().clone();
    assert_eq!(asked_with[header::USER_AGENT], "Kodi/21");
    assert_eq!(asked_with[header::REFERER], format!("{}/", w.remote_base));

    // What was found is kept, for the item too.
    let file = w.file("Film").await;
    assert_eq!(
        (&file["container"], &file["video_codec"], &file["audio_codec"], &file["height"], &file["duration_ms"]),
        (&json!("mkv"), &json!("h264"), &json!("aac"), &json!(1080), &json!(60_000))
    );
    assert_eq!(file["size"], json!(media().len()), "the media's size, not the file's");
    assert_eq!(file["streams"].as_array().unwrap().len(), 4, "three of the media and the subtitle file");
    let item = app.get(&format!("/api/items/{}", w.movies["Film"]), admin).await;
    assert_eq!(item.body["duration_ms"], json!(60_000));
    // Played again, it isn't looked at again.
    assert_eq!(w.play("Film", plays_mkv()).await.status, StatusCode::OK);
    assert_eq!(w.fake.probes.lock().len(), 1);

    // The player reads it through pley, a range at a time, as the video it
    // was found to be whatever the address calls it.
    let stream = format!("/api/files/{file_id}/stream");
    let part = app.call(Method::GET, &stream, Some(admin), None, &[("range", "bytes=1000-1999")]).await;
    assert_eq!(part.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(part.bytes, media()[1000..2000]);
    assert_eq!(part.headers[header::CONTENT_RANGE], format!("bytes 1000-1999/{}", media().len()));
    assert_eq!(part.headers[header::CONTENT_TYPE], "video/x-matroska");
    assert_eq!(part.headers[header::ACCEPT_RANGES], "bytes");
    let whole = app.get(&stream, admin).await;
    assert_eq!((whole.status, whole.bytes.len()), (StatusCode::OK, media().len()));
    assert_eq!(whole.headers[header::CONTENT_TYPE], "video/x-matroska");
    let head = app.call(Method::HEAD, &stream, Some(admin), None, &[]).await;
    assert_eq!((head.status, head.bytes.len()), (StatusCode::OK, 0));
    assert_eq!(head.headers[header::CONTENT_LENGTH], media().len().to_string());
    let past = app.call(Method::GET, &stream, Some(admin), None, &[("range", "bytes=999999999-")]).await;
    assert_eq!(past.status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(past.headers[header::CONTENT_RANGE], format!("bytes */{}", media().len()));
    // Nobody but someone who may see the item.
    assert_eq!(app.call(Method::GET, &stream, None, None, &[]).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_stream_and_its_subtitles_are_made_from_pleys_own_address() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let started = w.play("Film", plays_mp4()).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.body);
    assert_eq!((&started.body["method"], &started.body["hls"]), (&json!("remux"), &json!(true)));
    let job = w.fake.jobs.lock()[0].clone();
    let address = w.fake.probes.lock()[0].clone();
    assert_eq!(job.input, address, "the same address as it was probed at: what the transcoder keeps of it stays");
    assert_eq!(job.media.as_ref().and_then(|m| m.container.as_deref()), Some("mkv"));

    // The transcoder reads the media there, with ranges...
    let http = reqwest::Client::new();
    let part = http.get(&address).header(header::RANGE, "bytes=100000-").send().await.unwrap();
    assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(part.headers()[header::CONTENT_TYPE], "application/octet-stream");
    assert_eq!(part.bytes().await.unwrap()[..], media()[100_000..]);
    // ...and nobody else does: not through a proxy, not from elsewhere, and
    // not at another key.
    let through_proxy = http.get(&address).header("x-forwarded-for", "203.0.113.9").send().await.unwrap();
    assert_eq!(through_proxy.status(), StatusCode::NOT_FOUND);
    let path = address.strip_prefix(&w.own).unwrap();
    assert_eq!(app.get_from("203.0.113.9:4000", path, &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get_from("127.0.0.1:4000", path, &[]).await.status, StatusCode::OK);
    let other = format!("{}/api/strm/{}/media.mkv", w.own, "0".repeat(32));
    assert_eq!(http.get(&other).send().await.unwrap().status(), StatusCode::NOT_FOUND);

    // A track of the media is read at that address too; the file beside it
    // as any subtitle file.
    let file_id = app.file_of(w.movies["Film"]).await;
    let track = app.get(&format!("/api/files/{file_id}/subtitles/2.vtt"), admin).await;
    assert_eq!(track.status, StatusCode::OK, "{}", track.text());
    assert!(track.text().contains("Hello"));
    let asked = w.fake.subtitles.lock().clone();
    assert_eq!((asked.len(), asked[0].path.as_str(), asked[0].stream_index), (1, address.as_str(), Some(2)));

    // A download that has to be converted is made from it as well.
    let download = app.post("/api/conversions", admin, json!({"item_id": w.movies["Film"], "caps": plays_mp4()})).await;
    assert_eq!(download.status, StatusCode::OK, "{}", download.body);
    assert_eq!(w.fake.conversions.lock()[0].input, address);
    let original = app.post("/api/conversions", admin, json!({"item_id": w.movies["Film"], "caps": plays_mkv()})).await;
    assert_eq!(
        (&original.body["state"], &original.body["size"], &original.body["url"]),
        (&json!("direct"), &json!(media().len()), &json!(format!("/api/files/{file_id}/stream")))
    );
    // Nothing else was asked of the transcoder: no previews, no frames, no read-ahead.
    app.get(&format!("/api/items/{}", w.movies["Film"]), admin).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(w.fake.others.lock().is_empty(), "{:?}", w.fake.others.lock());
}

#[tokio::test]
async fn the_address_reaches_nobody() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    assert_eq!(w.play("Film", plays_mkv()).await.status, StatusCode::OK);
    let (viewer, _) = app.user(admin, "viewer", json!({})).await;
    let film = w.movies["Film"];
    let file_id = app.file_of(film).await;
    let host = w.remote_base.strip_prefix("http://").unwrap();
    let leaks = |what: &str, reply: &common::Reply| {
        assert!(reply.status.is_success(), "{what}: {}", reply.text());
        assert!(!reply.mentions(SECRET) && !reply.mentions(host), "{what}: {}", reply.text());
    };
    for token in [admin, viewer.as_str()] {
        leaks("the item", &app.get(&format!("/api/items/{film}"), token).await);
        leaks("its library", &app.get(&format!("/api/libraries/{}/items", w.library), token).await);
        leaks("its playback", &app.post("/api/playback", token, json!({"item_id": film, "caps": plays_mp4()})).await);
        // The Jellyfin-compatible API, which gives Jellyfin's own apps the address.
        let jf = app.jf_item(film);
        let info = app.jf(Method::POST, &format!("/Items/{jf}/PlaybackInfo"), token, Some(json!({}))).await;
        leaks("Jellyfin's playback info", &info);
        let source = &info.body["MediaSources"][0];
        assert_eq!((&source["Path"], &source["Container"]), (&json!("/Film (2020).strm"), &json!("mkv")));
        assert_eq!((&source["Protocol"], &source["Size"]), (&json!("File"), &json!(media().len())));
        leaks("Jellyfin's item", &app.jf(Method::GET, &format!("/Items/{jf}"), token, None).await);
    }
    // Jellyfin's apps play and download it through pley too, named as what it is.
    let jf = app.jf_item(film);
    let source = app.jf_source(file_id, admin).await;
    let stream = format!("/Videos/{jf}/stream.mkv?static=true&mediaSourceId={source}");
    let played = app.jf(Method::GET, &stream, admin, None).await;
    assert_eq!((played.status, played.bytes.len()), (StatusCode::OK, media().len()));
    let saved = app.jf(Method::GET, &format!("/Items/{jf}/Download"), admin, None).await;
    assert_eq!(saved.headers[header::CONTENT_DISPOSITION], "attachment; filename=\"Film (2020).mkv\"");
    // And the Plex-compatible one.
    let plex = app.plex_json(&format!("/library/metadata/{film}"), admin).await;
    let text = plex.to_string();
    assert!(!text.contains(SECRET) && !text.contains(host), "{text}");
    let part = plex["Metadata"][0]["Media"][0]["Part"][0]["key"].as_str().unwrap().to_owned();
    let played = app.plex(Method::GET, &part, admin, &[("range", "bytes=0-9")]).await;
    assert_eq!((played.status, &played.bytes[..]), (StatusCode::PARTIAL_CONTENT, &media()[..10]));
}

#[tokio::test]
async fn a_file_that_names_nothing_to_play_says_why() {
    let w = world_with(|movies, remote| {
        let write = |name: &str, text: String| std::fs::write(movies.join(name), text).unwrap();
        write("Local (2001).strm", "/etc/passwd\n".into());
        write("Windows (2002).strm", "C:\\Users\\someone\\film.mkv".into());
        write("Empty (2003).strm", "# nothing here\n".into());
        write("Gone (2004).strm", format!("{remote}/nothing/here.mkv"));
        write("Page (2005).strm", format!("{remote}/page"));
        write("Dead (2006).strm", "http://127.0.0.1:9/film.mkv".into());
    })
    .await;
    let why = |reply: common::Reply| (reply.status, reply.body["message"].as_str().unwrap_or_default().to_owned());
    let not_http = "this item's .strm file names something other than an http or https address";
    for title in ["Local", "Windows"] {
        assert_eq!(why(w.play(title, plays_mkv()).await), (StatusCode::NOT_FOUND, not_http.into()), "{title}");
    }
    assert_eq!(
        why(w.play("Empty", plays_mkv()).await),
        (StatusCode::NOT_FOUND, "this item's .strm file names no address".into())
    );
    assert!(w.fake.probes.lock().is_empty(), "none of them was asked");
    assert_eq!(
        why(w.play("Gone", plays_mkv()).await),
        (StatusCode::NOT_FOUND, "the address this item's .strm file names has nothing there any more".into())
    );
    assert_eq!(
        why(w.play("Page", plays_mkv()).await),
        (StatusCode::NOT_FOUND, "the address this item's .strm file names gave no video or audio".into())
    );
    assert_eq!(
        why(w.play("Dead", plays_mkv()).await),
        (StatusCode::SERVICE_UNAVAILABLE, "the address this item's .strm file names didn't answer".into())
    );
    // Not media: never handed out as it is, to a player or a download.
    let page = w.app.file_of(w.movies["Page"]).await;
    let served = w.app.get(&format!("/api/files/{page}/stream"), &w.admin).await;
    assert_eq!(served.status, StatusCode::NOT_FOUND);
    assert!(!served.mentions("not a film"));
    // Asked again at once, the address isn't: what it said is remembered a moment.
    let probes = w.fake.probes.lock().len();
    assert_eq!(why(w.play("Page", plays_mkv()).await).0, StatusCode::NOT_FOUND);
    assert_eq!(w.fake.probes.lock().len(), probes);
    assert_eq!(w.kept("Page").await, (Some("strm".into()), 0, None), "nothing kept of it");
    // Nor when its item is opened, for longer: it opens as it is.
    assert_eq!(w.file("Page").await["container"], json!("strm"));
    assert_eq!(w.fake.probes.lock().len(), probes);
}

#[tokio::test]
async fn what_was_found_stays_until_the_file_names_another_address() {
    let mut w = world().await;
    assert_eq!(w.play("Film", plays_mkv()).await.status, StatusCode::OK);
    let rel = "movies/Film (2020)/Film (2020).strm";
    let named = std::fs::read_to_string(w.app.media.path().join(rel)).unwrap();

    // Written again as it was (a tool that makes its links again): kept.
    w.strm(rel, &format!("# made again\n{named}"));
    w.scan(false).await;
    assert_eq!(w.kept("Film").await, (Some("mkv".into()), media().len() as i64, Some(60_000)));
    assert_eq!(w.play("Film", plays_mkv()).await.status, StatusCode::OK);
    assert_eq!(w.fake.probes.lock().len(), 1);

    // Another address: what its media is isn't known until it is played.
    w.strm(rel, &format!("{}/resolve/{SECRET}", w.remote_base));
    w.scan(false).await;
    assert_eq!(w.kept("Film").await, (Some("strm".into()), 0, None));
    assert_eq!(w.play("Film", plays_mkv()).await.status, StatusCode::OK);
    let probes = w.fake.probes.lock().clone();
    assert_eq!(probes.len(), 2);
    assert_ne!(probes[0], probes[1], "pley's own address for it changes with it");
    assert_eq!(w.kept("Film").await.0.as_deref(), Some("mkv"));

    // Where it redirects to is asked directly for a while: not through the
    // address once more for each range a player reads.
    let through = w.remote.redirected.load(Ordering::SeqCst);
    assert!(through >= 1);
    let stream = format!("/api/files/{}/stream", w.app.file_of(w.movies["Film"]).await);
    for first in [0, 50_000, 100_000] {
        let range = format!("bytes={first}-{}", first + 9);
        let part = w.app.call(Method::GET, &stream, Some(&w.admin), None, &[("range", &range)]).await;
        assert_eq!((part.status, &part.bytes[..]), (StatusCode::PARTIAL_CONTENT, &media()[first..first + 10]));
    }
    assert_eq!(w.remote.redirected.load(Ordering::SeqCst), through);

    // Every file scanned again: looked at again when it next plays.
    w.scan(true).await;
    assert_eq!(w.kept("Film").await, (Some("strm".into()), 0, None));
    assert_eq!(w.play("Film", plays_mkv()).await.status, StatusCode::OK);
    assert_eq!(w.fake.probes.lock().len(), 3);
}

#[tokio::test]
async fn a_transcoder_too_old_to_read_an_address_says_so() {
    let (fake, transcoder) = start_fake().await;
    fake.old.store(true, Ordering::SeqCst);
    let (_, remote) = start_remote().await;
    let app = TestApp::with_config(|c| {
        c.transcoder_url = Some(transcoder);
        c.transcoder_token = Some(TOKEN.into());
    })
    .await;
    app.state.transcoder.start();
    assert!(app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    let admin = app.setup_admin().await;
    std::fs::create_dir_all(app.media.path().join("movies")).unwrap();
    std::fs::write(app.media.path().join("movies/Film (2020).strm"), format!("{remote}/dl/x/film.mkv")).unwrap();
    let library = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(library, &admin).await["Film"];
    let started = app.post("/api/playback", &admin, json!({"item_id": film, "caps": plays_mkv()})).await;
    assert_eq!(started.status, StatusCode::NOT_IMPLEMENTED, "{}", started.body);
    assert!(fake.probes.lock().is_empty());
}
