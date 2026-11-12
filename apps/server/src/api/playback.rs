use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use pley_transcoder_api::{
    AudioMode, ConversionState, ConversionStatus, FileContainer, JobRequest, JobStatus, MediaInfo, MediaStream,
    SUBTITLES_COMPLETE_HEADER, SegmentFormat, StreamKind, VideoMode,
};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::items::item_or_404;
use crate::auth::{AuthUser, CurrentToken, SourceIp};
use crate::bandwidth::{self, Answer, Report, valid_kbps};
use crate::catalog::{self, ItemRow};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::library::sidecar::EXTERNAL_INDEX_BASE;
use crate::playback::{
    Decision, Delivery, Full, HLS_PTS_OFFSET_MS, Method, NOT_CONVERTED, PlaybackInfo, PlaybackRequest, Pool, Session,
    SessionStatus, TranscodeInfo, TranscodeSpec, decide, decide_with_audio, may_draw_pictures, new_media_key,
    picture_to_prepare, sentence, subtitle_tracks, transcode_info, video_range_shown,
};
use crate::state::AppState;
use crate::streams;
use crate::subtitles::ADDED_INDEX_BASE;
use crate::webvtt;

/// Length of HLS segments. Nothing plays before FFmpeg has written a whole
/// segment, and every start, seek and quality switch starts a process that
/// writes one first: 2 s ones come sooner than the transcoder's default of
/// 4 s (by 0.35 s for 4K at 60 fps tone mapped, less for lighter video), for
/// a keyframe every 2 s.
const SEGMENT_SECONDS: f64 = 2.0;
/// How long the transcoder's background work waits for a direct play's
/// first reads (the head, the index, the start).
const DIRECT_PLAY_HOLD: Duration = Duration::from_secs(10);

fn label(row: &ItemRow) -> String {
    match row.kind.as_str() {
        "episode" => format!(
            "{} — S{:02}E{:02} · {}",
            row.grandparent_title.as_deref().unwrap_or("?"),
            row.parent_idx.unwrap_or(0),
            row.idx.unwrap_or(0),
            row.title
        ),
        "track" => format!("{} — {}", row.grandparent_title.as_deref().unwrap_or("?"), row.title),
        _ => match row.year {
            Some(y) => format!("{} ({y})", row.title),
            None => row.title.clone(),
        },
    }
}

/// [`decide`] with the audio track the player named, or with none named
/// the one the user's choices and the player pick (`decide_with_audio`).
#[allow(clippy::too_many_arguments)]
async fn decide_picked(
    state: &AppState,
    user_id: i64,
    row: &ItemRow,
    file: &catalog::FileRow,
    info: Option<&MediaInfo>,
    streams: &[MediaStream],
    req: &mut PlaybackRequest,
) -> ApiResult<Decision> {
    let mut decision = if req.audio_index.is_some() {
        decide(file, info, streams, req)
    } else {
        let pref = streams::load(&state.db, user_id, streams::scope(row)).await?;
        let defaults = streams::defaults(&state.db, user_id).await?;
        decide_with_audio(file, info, streams, req, |out| {
            streams::resolve(pref.as_ref(), &defaults, streams, Some(out)).audio
        })
    };
    // An added track is burned in from its file in the data folder.
    if let Some(burn) = decision.subtitle.as_mut()
        && burn.stream_index.is_none()
        && burn.external_path.is_none()
        && let Some(sub) = decision.burn_index.and_then(|i| file.added_subs().into_iter().find(|s| s.index == i))
    {
        let path = sub.path(&state.config.added_subtitles_dir(), file.id);
        burn.external_path = Some(path.to_string_lossy().into_owned());
    }
    Ok(decision)
}

/// What a player gets when a file needs its video converted to play there
/// and the server converts none (`transcode_video` is off): why it does,
/// as the play's reasons or the transcoder's warnings have it.
fn video_not_converted(why: impl Iterator<Item = String>) -> AppError {
    let why: Vec<String> = why.filter(|reason| reason != NOT_CONVERTED).collect();
    let needs = "video transcoding is off on this server, and this file needs it to play on this device";
    AppError::ConflictAs(
        "video_not_converted",
        if why.is_empty() { needs.to_owned() } else { format!("{needs}. {}", why.join(" ")) },
    )
}

/// Why `decision` needs its video converted, on a server that converts
/// none: its own reasons, or the transcoder's, which makes new frames of a
/// picture turned on its side rather than copy it (what it writes can't
/// say the turn). `None` when the video is copied, or there is none.
fn unwanted_conversion(req: &PlaybackRequest, decision: &Decision, info: Option<&MediaInfo>) -> Option<Vec<String>> {
    let video = decision.video.as_ref().filter(|_| req.copy_video_only)?;
    if video.mode == VideoMode::Transcode {
        return Some(decision.reasons.clone());
    }
    let turned = info.and_then(|i| i.video()).is_some_and(|v| v.rotation_degrees() != 0);
    turned.then(|| {
        let mut reasons = decision.reasons.clone();
        reasons.push("The picture is turned, which a copy can't say.".into());
        reasons
    })
}

/// What a play gets past a limit on streams (Settings → Transcoder).
fn too_many_streams(full: Full, total: u32, video: u32) -> AppError {
    let (max, made) = match full {
        Full::Total => (total, "transcodes"),
        Full::Video => (video, "video transcodes"),
    };
    AppError::Unavailable(format!("the server is already running its maximum of {max} {made}"))
}

/// The subtitles a play starts with, as the user's remotes show them until
/// its player says: those burned in, else those the user's choices pick
/// (the item's `selected_subtitle`, which players start with), `-1` for
/// none. `None` for a file without subtitles.
async fn subtitles_shown(
    state: &AppState,
    user_id: i64,
    row: &ItemRow,
    streams: &[MediaStream],
    decision: &Decision,
) -> ApiResult<Option<i64>> {
    if decision.burn_index.is_some() {
        return Ok(decision.burn_index);
    }
    if !streams.iter().any(|s| s.kind == StreamKind::Subtitle) {
        return Ok(None);
    }
    let pref = streams::load(&state.db, user_id, streams::scope(row)).await?;
    let defaults = streams::defaults(&state.db, user_id).await?;
    Ok(Some(streams::resolve(pref.as_ref(), &defaults, streams, None).subtitle.unwrap_or(-1)))
}

pub async fn start(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    CurrentToken(token): CurrentToken,
    Json(mut req): Json<PlaybackRequest>,
) -> ApiResult<Json<PlaybackInfo>> {
    let began = Instant::now();
    let row = item_or_404(&state, user.id, req.item_id).await?;
    // A transcoder older than master playlists can't name Dolby Vision
    // (or anything else) in one: the player gets what older servers gave.
    let master_ok = state.transcoder.info().is_some_and(|info| info.capabilities.hls_master_playlist);
    if !master_ok && let Some(hls) = req.caps.hls.as_mut() {
        hls.master_playlist = false;
    }
    // Transcodes keep HDR only with a transcoder that can (HEVC Main 10).
    req.hdr_transcodes = state.transcoder.info().is_some_and(|info| info.capabilities.hdr_transcode);
    // fMP4 segments only from a transcoder that makes them.
    req.fmp4_segments = state.transcoder.info().is_some_and(|info| info.capabilities.fmp4_segments);
    let settings = state.settings.get();
    // Music has no video: a track is converted as ever, also one never
    // analysed, whose streams aren't known to be sound alone.
    req.copy_video_only = !settings.transcode_video && row.kind != "track";
    if !matches!(row.kind.as_str(), "movie" | "episode" | "track" | "extra") {
        return Err(AppError::bad_request("this item cannot be played directly"));
    }
    let files = catalog::files_for_item(&state.db, row.id).await?;
    let file = match req.file_id {
        Some(id) => files.into_iter().find(|f| f.id == id).ok_or_else(|| AppError::not_found("file not found"))?,
        None => files.into_iter().next().ok_or_else(|| AppError::not_found("this item has no media file"))?,
    };
    // Background work (seek previews, intro detection) keeps off the disk
    // and the network from here on: creating a transcode reads the file too,
    // before the session exists.
    let _starting = state.sessions.begin_start();
    let info = file.media_info();
    let streams = file.streams();
    // No audio track named: the one the user's choices pick, not just the
    // file's default, and of the tracks of that language the one this
    // player plays best (as it is, then with the file as it is, then as it
    // sounds on its output), or for a file that streams anyway the one its
    // stream copies. `PlaybackInfo.audio_index` tells the player.
    let decision = decide_picked(&state, user.id, &row, &file, info.as_ref(), &streams, &mut req).await?;
    // Before what the client plays now is ended: it goes on.
    if let Some(why) = unwanted_conversion(&req, &decision, info.as_ref()) {
        return Err(video_not_converted(why.into_iter()));
    }
    // Picture subtitles the player draws from their own file beside a
    // stream: copied out of the media file from now on, rather than once
    // the player asks.
    let picture = if decision.method != Method::Direct && may_draw_pictures(&streams, &req.caps) {
        let chosen = match req.subtitle_index {
            Some(_) => None,
            None => {
                let pref = streams::load(&state.db, user.id, streams::scope(&row)).await?;
                let defaults = streams::defaults(&state.db, user.id).await?;
                streams::resolve(pref.as_ref(), &defaults, &streams, None).subtitle
            }
        };
        picture_to_prepare(&streams, &req, &decision, chosen)
    } else {
        None
    };
    let mut reasons = decision.reasons.clone();
    let video_stream = info.as_ref().and_then(|i| i.video()).cloned();
    let mut subtitles = subtitle_tracks(file.id, &streams);
    let added = file.added_subs();
    for track in subtitles.iter_mut().filter(|t| t.added) {
        track.deletable = added.iter().any(|s| s.index == track.index && s.may_delete(&user));
    }
    let start_ms = req.start_ms.unwrap_or(0).max(0);
    let client_id = req.client_id.clone().filter(|c| !c.is_empty() && c.len() <= 128);
    let device = super::remote::device_name(req.device_name.as_deref(), &headers);
    let device_id = super::remote::device_id_of(&user, token.as_deref());
    let device_kind = super::remote::device_kind_of(req.device_kind, &user, super::remote::user_agent(&headers));
    let subtitles_at_start = subtitles_shown(&state, user.id, &row, &streams, &decision).await?;

    // A client plays one thing at a time: end what it played before (a
    // quality or audio switch, the next episode) so its transcode slot is
    // free before the limit is checked. Unless it prepares this one while
    // that one plays: it ends that one itself as it moves.
    if let Some(client) = client_id.as_ref().filter(|_| !req.keep_previous) {
        let previous = state.sessions.take_client_sessions(user.id, client);
        if !previous.is_empty() {
            end_sessions(&state, previous);
        }
    }

    if decision.method == Method::Direct {
        let duration = file.duration_ms.or(row.duration_ms).unwrap_or(0);
        let mut session = Session::new(
            user.id,
            user.username.clone(),
            client_id,
            row.id,
            file.id,
            Method::Direct,
            duration,
            None,
            None,
            start_ms,
        );
        session.app_grant = user.app.as_ref().map(|app| app.grant_id);
        session.compatibility_token = user.compatibility.map(|c| c.token_id);
        session.media_key = req.keyed_urls.then(new_media_key);
        session.controls = req.controls.clone();
        session.device = device;
        session.on_device(device_id, device_kind);
        session.picked_tracks(decision.audio_index, subtitles_at_start);
        // The transcoder doesn't see direct plays: its background work waits
        // for the player's first reads too.
        if state.transcoder.is_available() {
            let transcoder = state.transcoder.clone();
            tokio::spawn(async move {
                let _ = transcoder.hold_background(DIRECT_PLAY_HOLD).await;
            });
        }
        let session = state.sessions.insert(session);
        state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
        let url = match &session.media_key {
            // Named for its container: some players go by the extension.
            Some(key) => format!("/api/media/{key}/file.{}", container_extension(file.container.as_deref())),
            None => format!("/api/files/{}/stream", file.id),
        };
        tracing::info!("playback of item {} starts directly, answered in {} ms", row.id, began.elapsed().as_millis());
        return Ok(Json(PlaybackInfo {
            session_id: session.id.clone(),
            method: Method::Direct,
            url,
            hls: false,
            file_id: file.id,
            duration_ms: duration,
            audio_index: decision.audio_index,
            subtitle_index: None,
            burn_subtitles: false,
            audio_converted: false,
            subtitles,
            reasons: Vec::new(),
            reason: None,
            auto_quality: None,
            video_range: video_stream.and_then(|v| video_range_shown(&v, &req.caps, Delivery::Direct)),
            adaptive: false,
            live_bitrate: false,
            pts_offset_ms: None,
            original_only: req.copy_video_only,
        }));
    }

    // Taken before the job is created (that takes seconds) and handed to the
    // session; given back if this request fails or the client goes away.
    // Video transcodes are some of the streams: never more than all of them.
    let (total, video) = (settings.max_transcodes, settings.max_video_transcodes.min(settings.max_transcodes));
    let pool = Pool::of(decision.video.as_ref());
    let mut slot = state
        .sessions
        .reserve_transcode(pool, total as usize, video as usize)
        .map_err(|full| too_many_streams(full, total, video))?;
    let job_request = JobRequest {
        input: file.path.clone(),
        media: info,
        label: Some(label(&row)),
        owner: Some(user.username.clone()),
        video: decision.video.clone(),
        audio: decision.audio.clone(),
        subtitle: decision.subtitle.clone(),
        segment_seconds: Some(SEGMENT_SECONDS),
        start_ms: Some(start_ms),
        options: settings.transcode_options(),
        container: None,
        // A transcoder older than adaptive bitrate makes one size.
        variants: if state.transcoder.info().is_some_and(|t| t.capabilities.hls_variants) {
            decision.variants.clone()
        } else {
            Vec::new()
        },
        segment_format: decision.segment_format,
    };
    let created = state.transcoder.create_job(&job_request).await.map_err(|err| for_viewer(&user, err))?;
    // A copy the transcoder can only make by converting (a file whose
    // keyframes it doesn't know yet): not on a server that converts no
    // video, and elsewhere as one of the video transcodes, while there is
    // room for one.
    if created.video.as_ref().is_some_and(|v| v.mode == VideoMode::Transcode) {
        let refused = if req.copy_video_only {
            Some(video_not_converted(created.warnings.iter().map(|w| sentence(w))))
        } else {
            state
                .sessions
                .transcodes_video(&mut slot, video as usize)
                .err()
                .map(|full| too_many_streams(full, total, video))
        };
        if let Some(refused) = refused {
            let _ = state.transcoder.delete_job(&created.id).await;
            return Err(refused);
        }
    }
    // Once the transcoder knows a playback starts: the copy waits for it.
    if let Some(index) = picture {
        prepare_picture_track(&state, &file, index, start_ms);
    }
    let method = match (&created.video, &created.audio) {
        (Some(v), _) if v.mode == VideoMode::Copy => Method::Remux,
        (Some(_), _) => Method::Transcode,
        (None, Some(a)) if a.mode == AudioMode::Copy => Method::Remux,
        _ => Method::Transcode,
    };
    reasons.extend(created.warnings.iter().map(|w| sentence(w)));
    let burn = decision.burn_index;
    // Pin the effective video mode and segment format (audio-only jobs
    // stay MPEG-TS): the segment layout and names depend on them.
    let mut request = job_request;
    if let (Some(video), Some(plan)) = (request.video.as_mut(), created.video.as_ref()) {
        video.mode = plan.mode;
    }
    request.segment_format = created.segment_format;
    let spec = TranscodeSpec { request, segment_count: created.segment_count, duration_ms: created.duration_ms };
    let mut session = Session::new(
        user.id,
        user.username.clone(),
        client_id,
        row.id,
        file.id,
        method,
        created.duration_ms,
        Some(spec),
        Some(created.id.clone()),
        start_ms,
    );
    session.app_grant = user.app.as_ref().map(|app| app.grant_id);
    session.compatibility_token = user.compatibility.map(|c| c.token_id);
    session.media_key = req.keyed_urls.then(new_media_key);
    session.controls = req.controls.clone();
    session.device = device;
    session.on_device(device_id, device_kind);
    session.picked_tracks(decision.audio_index, subtitles_at_start);
    let session = state.sessions.insert_transcode(session, slot);
    state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
    // Players that read multivariant playlists learn the codecs (Dolby
    // Vision, above all) before the first segment; one that moves between
    // sizes learns them there.
    let adaptive = created.variants > 0;
    let playlist = if req.caps.master_playlist() || adaptive { "master.m3u8" } else { "index.m3u8" };
    // Playlists name their segments by relative addresses, so a keyed
    // playlist's segments carry the key too.
    let url = match &session.media_key {
        Some(key) => format!("/api/media/{key}/{playlist}"),
        None => format!("/api/playback/{}/{playlist}", session.id),
    };
    tracing::info!(
        "playback of item {} starts by {method:?} (job {}), answered in {} ms",
        row.id,
        created.id,
        began.elapsed().as_millis()
    );
    Ok(Json(PlaybackInfo {
        session_id: session.id.clone(),
        method,
        url,
        hls: true,
        file_id: file.id,
        duration_ms: created.duration_ms,
        audio_index: decision.audio_index,
        subtitle_index: burn,
        burn_subtitles: burn.is_some(),
        audio_converted: created.audio.as_ref().is_some_and(|a| a.mode != AudioMode::Copy),
        subtitles,
        reason: (!reasons.is_empty()).then(|| reasons.join(" ")),
        reasons,
        // Only when the video really is converted (a remux the transcoder
        // turned into a transcode keeps its own default).
        auto_quality: decision.quality.filter(|_| method == Method::Transcode).map(str::to_owned),
        video_range: video_stream
            .zip(created.video.as_ref())
            .and_then(|(v, plan)| video_range_shown(&v, &req.caps, Delivery::of_plan(plan))),
        adaptive,
        // An fMP4 stream's init segment holds the encoder's settings: its
        // bitrate stays as it began, and players move between variants.
        live_bitrate: method == Method::Transcode && state.transcoder.live_bitrate() && created.segment_format.is_ts(),
        // The MP4 muxer keeps the file's times as they are.
        pts_offset_ms: Some(if created.segment_format.is_ts() { HLS_PTS_OFFSET_MS } else { 0 }),
        original_only: req.copy_video_only,
    }))
}

#[derive(Deserialize)]
pub struct BitrateBody {
    /// kbps.
    max_bitrate: u32,
}

#[derive(Serialize)]
pub struct BitrateAnswer {
    /// What the stream goes on at (kbps): the one asked for, kept within
    /// 1 Mbps and the session's own.
    bitrate: u32,
}

/// `PUT /api/playback/{session}/bitrate`: the session's converted video goes
/// on at another bitrate from the segments its player hasn't fetched yet,
/// the same size and the same playlist (docs/QUALITY.md, "Players that go
/// black between streams").
pub async fn set_bitrate(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<BitrateBody>,
) -> ApiResult<Json<BitrateAnswer>> {
    let session = owned_session(&state, &user, &id)?;
    let fmp4 = session.transcode.as_ref().is_some_and(|spec| !spec.request.segment_format.is_ts());
    if session.method != Method::Transcode || !state.transcoder.live_bitrate() || fmp4 {
        return Err(AppError::bad_request("this stream's bitrate can't change as it plays"));
    }
    let job = session.job_id().ok_or_else(|| AppError::not_found("playback session expired"))?;
    let bitrate = state.transcoder.set_bitrate(&job, body.max_bitrate).await.map_err(|err| for_viewer(&user, err))?;
    *session.bitrate.lock() = Some(bitrate);
    session.touch();
    Ok(Json(BitrateAnswer { bitrate }))
}

/// `GET /api/playback/{session}`: how the session plays now, with the live
/// numbers of its transcode (what it converts to, on what, how fast), for
/// the player's details.
pub async fn status(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<SessionStatus>> {
    let session = owned_session(&state, &user, &id)?;
    let transcode = match session.job_id() {
        Some(job) => state.transcoder.job_status(&job).await.ok().map(|status| transcode_seen_by(&user, &status)),
        None => None,
    };
    Ok(Json(SessionStatus { session_id: session.id.clone(), method: session.method, transcode }))
}

/// `job`'s transcode as `user` sees it, here and in the activity: FFmpeg's
/// last words name the media file, an administrator's (as in `for_viewer`).
pub(super) fn transcode_seen_by(user: &AuthUser, job: &JobStatus) -> TranscodeInfo {
    let mut info = transcode_info(job);
    if !user.is_admin && info.last_error.is_some() {
        info.last_error = Some("the conversion failed".to_owned());
    }
    info
}

#[derive(Deserialize)]
pub struct NetworkQuery {
    client_id: Option<String>,
}

/// `GET /api/playback/network?client_id=`: what Automatic knows of the
/// network this request came from, for this client (see [`bandwidth`]).
pub async fn network(
    State(state): State<AppState>,
    user: AuthUser,
    SourceIp(ip): SourceIp,
    query: Result<Query<NetworkQuery>, QueryRejection>,
) -> ApiResult<Json<Answer>> {
    let Query(query) = query.map_err(|_| AppError::bad_request("give a client_id"))?;
    let client_id = client_id_of(query.client_id.as_deref())?;
    let network = state.bandwidth.network(ip);
    Ok(Json(state.bandwidth.get(user.id, client_id, &network).await?))
}

/// `PUT /api/playback/network`'s body: a speed test's result, or
/// Automatic's new ceiling (`null`: none).
#[derive(Deserialize)]
pub struct NetworkReport {
    client_id: Option<String>,
    probe_kbps: Option<f64>,
    probe_complete: Option<bool>,
    #[serde(default, deserialize_with = "super::nullable")]
    ceiling_kbps: Option<Option<f64>>,
}

impl NetworkReport {
    fn checked(&self) -> ApiResult<(&str, Report)> {
        let client_id = client_id_of(self.client_id.as_deref())?;
        let kbps = |kbps: f64| {
            valid_kbps(kbps).ok_or_else(|| {
                AppError::bad_request(format!("speeds are whole kbps from 1 to {}", bandwidth::MAX_KBPS))
            })
        };
        let report = match (self.probe_kbps, self.ceiling_kbps) {
            (Some(probe), None) => Report::Probe { kbps: kbps(probe)?, complete: self.probe_complete.unwrap_or(false) },
            (None, Some(Some(ceiling))) => Report::Ceiling(kbps(ceiling)?),
            (None, Some(None)) => Report::Forget,
            _ => return Err(AppError::bad_request("give either probe_kbps or ceiling_kbps")),
        };
        Ok((client_id, report))
    }
}

/// `PUT /api/playback/network`: keeps what the client reports of the
/// network this request came from, and answers as `GET`.
pub async fn report_network(
    State(state): State<AppState>,
    user: AuthUser,
    SourceIp(ip): SourceIp,
    body: Result<Json<NetworkReport>, JsonRejection>,
) -> ApiResult<Json<Answer>> {
    let Json(body) = body.map_err(|err| AppError::bad_request(err.body_text()))?;
    let (client_id, report) = body.checked()?;
    let network = state.bandwidth.network(ip);
    Ok(Json(state.bandwidth.report(user.id, client_id, &network, report).await?))
}

fn client_id_of(client_id: Option<&str>) -> ApiResult<&str> {
    bandwidth::valid_client_id(client_id)
        .ok_or_else(|| AppError::bad_request("client_id must be the player's id, 1 to 128 characters"))
}

#[derive(Deserialize)]
pub struct SpeedTestQuery {
    bytes: Option<u64>,
}

/// `GET /api/playback/speedtest?bytes=`: that many bytes of data nothing
/// compresses, to time (see [`bandwidth::speed_test`]); two at a time per
/// user.
pub async fn speed_test(
    State(state): State<AppState>,
    user: AuthUser,
    query: Result<Query<SpeedTestQuery>, QueryRejection>,
) -> ApiResult<Response> {
    let Query(query) = query.map_err(|_| AppError::bad_request("bytes must be a whole number"))?;
    let size = bandwidth::speed_test_size(query.bytes);
    let turn = state.bandwidth.speed_test_turn(user.id).ok_or_else(|| {
        AppError::TooManyRequests(format!(
            "{} speed tests are running for this account: try again when one ends",
            bandwidth::SPEED_TESTS_AT_ONCE
        ))
    })?;
    // `no-transform`: no proxy compresses it either.
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")),
        (header::CONTENT_LENGTH, HeaderValue::from(size)),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-transform")),
    ];
    Ok((headers, bandwidth::speed_test(size, turn)).into_response())
}

/// What a viewer is told when the transcoder refused: its own sentence,
/// which may name the media file's path, for an administrator (on pley's
/// own sign-in, never an app's); one of pley's for anyone else, who never
/// sees paths.
fn for_viewer(user: &AuthUser, err: impl Into<AppError>) -> AppError {
    let err = err.into();
    if user.is_admin {
        return err;
    }
    match err {
        AppError::NotFound(_) => AppError::not_found("this title's file can't be read right now"),
        AppError::BadRequest(_) => AppError::bad_request("this title's file can't be played"),
        err => err,
    }
}

fn owned_session(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<Arc<Session>> {
    let Some(session) = state.sessions.get(id) else {
        // Stopped from elsewhere: a player without the event stream learns
        // it from its next request.
        return Err(match state.sessions.stopped(id) {
            Some(stopped) if stopped.user_id == user.id || user.is_admin => AppError::SessionStopped(stopped.message),
            _ => AppError::not_found("playback session expired"),
        });
    };
    if session.user_id != user.id && !user.is_admin {
        return Err(AppError::forbidden("not your session"));
    }
    Ok(session)
}

pub async fn stop(State(state): State<AppState>, user: AuthUser, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let session = match owned_session(&state, &user, &id) {
        // Over already: what the player wanted.
        Err(AppError::SessionStopped(_)) => return Ok(StatusCode::NO_CONTENT),
        other => other?,
    };
    if let Some(session) = state.sessions.remove(&session.id) {
        end_sessions(&state, vec![session]);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The most a stopped session's message says.
const STOP_MESSAGE_MAX: usize = 200;

#[derive(Deserialize, Default)]
pub struct StopSession {
    #[serde(default)]
    message: Option<String>,
}

/// `DELETE /api/sessions/{id}` with an optional `{ message }`: ends a
/// playback session at once, from elsewhere than its player (the dashboard,
/// another device). Administrators stop any session, as do pley's own apps
/// allowed `activity` on their accounts; others their own.
pub async fn stop_session(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<StatusCode> {
    let body: StopSession = if body.iter().all(u8::is_ascii_whitespace) {
        StopSession::default()
    } else {
        serde_json::from_slice(&body).map_err(|err| AppError::bad_request(format!("invalid body: {err}")))?
    };
    let message = body.message.map(|m| m.trim().to_owned()).filter(|m| !m.is_empty());
    if message.as_ref().is_some_and(|m| m.chars().count() > STOP_MESSAGE_MAX) {
        return Err(AppError::bad_request(format!("the message is longer than {STOP_MESSAGE_MAX} characters")));
    }
    stop_by(&state, &user, &id, message)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stops session `id` for `user` (who oversees playback, see
/// [`AuthUser::oversees_playback`]: any session; anyone else: their own):
/// its transcode ends, its player hears `session_stopped` and its next
/// requests answer `410`. A session stopped already is fine. A Live TV
/// viewer's session is ended the same way.
pub(crate) fn stop_by(state: &AppState, user: &AuthUser, id: &str, message: Option<String>) -> ApiResult<()> {
    let Some(session) = state.sessions.get(id) else {
        return match state.sessions.stopped(id) {
            Some(stopped) if stopped.user_id == user.id || user.oversees_playback() => Ok(()),
            Some(_) => Err(AppError::forbidden("not your session")),
            None => {
                state.live.tuners.stop(id, user, message).unwrap_or_else(|| Err(AppError::not_found("no such session")))
            }
        };
    };
    if session.user_id != user.id && !user.oversees_playback() {
        return Err(AppError::forbidden("not your session"));
    }
    let Some(session) = state.sessions.stop(id, message.clone()) else { return Ok(()) };
    state.events.send(ServerEvent::SessionStopped {
        user_id: session.user_id,
        session_id: session.id.clone(),
        message,
        by_admin: session.user_id != user.id,
    });
    end_sessions(state, vec![session]);
    Ok(())
}

/// Forget sessions that were already removed from the registry and delete
/// their transcoder jobs in the background.
fn end_sessions(state: &AppState, sessions: Vec<Arc<Session>>) {
    let jobs: Vec<String> = sessions.iter().filter_map(|s| s.job_id()).collect();
    if !jobs.is_empty() {
        let transcoder = state.transcoder.clone();
        tokio::spawn(async move {
            for job in jobs {
                let _ = transcoder.delete_job(&job).await;
            }
        });
    }
    state.events.sessions_changed(sessions.iter().map(|s| s.user_id));
}

/// The transcoder forgot the job (restart, expiry): create it again with the
/// same effective request. The client never reloads its VOD playlist, so the
/// new job must have the very same segment layout; when it does not (the
/// keyframe cache was cleared), the session is over and the client starts a
/// new one.
async fn replacement_job(state: &AppState, user: &AuthUser, session: &Session, failed: &str) -> ApiResult<String> {
    let _guard = session.recreate.lock().await;
    // Its player waits: as much a start as the first one.
    let _starting = state.sessions.begin_start();
    // Stopped while this request waited: its job went on purpose.
    if let Some(stopped) = state.sessions.stopped(&session.id) {
        return Err(AppError::SessionStopped(stopped.message));
    }
    if let Some(current) = session.job_id()
        && current != failed
    {
        return Ok(current);
    }
    let spec = session.transcode.as_ref().ok_or_else(|| AppError::not_found("not a transcode session"))?;
    // Pre-start near the playhead instead of at the beginning.
    let request = JobRequest { start_ms: Some(session.position_ms()), ..spec.request.clone() };
    let created = state.transcoder.create_job(&request).await.map_err(|err| for_viewer(user, err))?;
    let same_mode = match (&spec.request.video, &created.video) {
        (Some(wanted), Some(got)) => wanted.mode == got.mode,
        (None, None) => true,
        _ => false,
    };
    // An fMP4 player keeps the init it fetched. A remux's comes from the
    // file, the same each time; a new job's encoder may not be the first's
    // (the GPU busy, another preset), and its init then wouldn't match: the
    // player starts a new session instead, as for another layout.
    let encoded = created.video.as_ref().is_some_and(|plan| plan.mode == VideoMode::Transcode);
    if !same_mode
        || created.segment_count != spec.segment_count
        || created.duration_ms != spec.duration_ms
        || created.segment_format != spec.request.segment_format
        || (encoded && !created.segment_format.is_ts())
    {
        let _ = state.transcoder.delete_job(&created.id).await;
        *session.job_id.lock() = None;
        tracing::info!("transcode job for session {} cannot be recreated with the same layout", session.id);
        return Err(AppError::not_found("playback session expired"));
    }
    // Stopped while the job was being created: nothing else will delete it.
    if let Some(stopped) = state.sessions.stopped(&session.id) {
        let _ = state.transcoder.delete_job(&created.id).await;
        return Err(AppError::SessionStopped(stopped.message));
    }
    tracing::info!("recreated transcode job for session {}", session.id);
    // At the bitrate its player asked for since.
    let bitrate = *session.bitrate.lock();
    if let Some(kbps) = bitrate
        && let Err(err) = state.transcoder.set_bitrate(&created.id, kbps).await
    {
        tracing::warn!("the recreated job for session {} keeps its own bitrate: {err}", session.id);
    }
    *session.job_id.lock() = Some(created.id.clone());
    Ok(created.id)
}

/// Where a download stands: the original, or a conversion of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    /// The original plays on the player as it is: `url` is it.
    Direct,
    Queued,
    Running,
    /// The converted file is whole: `url` is it.
    Done,
    Failed,
}

/// `POST /api/conversions` and `GET /api/conversions/{id}`: what a
/// download of an item takes (docs/API.md, "Downloads").
#[derive(Debug, Serialize)]
pub struct DownloadInfo {
    /// The conversion's id; `None` for the original.
    pub id: Option<String>,
    pub state: DownloadState,
    /// How far a conversion has come, 0 to 1 (1 for the original).
    pub progress: f64,
    /// Bytes, once known: the original's, or the converted file's once done.
    pub size: Option<u64>,
    pub error: Option<String>,
    /// Where the file comes from (with ranges): the original, or the converted file.
    pub url: String,
}

impl DownloadInfo {
    /// `status` as `user` sees it: FFmpeg's words, which name the server's
    /// files, are an administrator's (the log keeps them); apps never get them.
    fn for_viewer(status: &ConversionStatus, user: &AuthUser) -> Self {
        let mut info = Self::of(status);
        if !user.is_admin {
            info.error = None;
        }
        info
    }

    fn of(status: &ConversionStatus) -> Self {
        Self {
            id: Some(status.id.clone()),
            state: match status.state {
                ConversionState::Queued => DownloadState::Queued,
                ConversionState::Running => DownloadState::Running,
                ConversionState::Done => DownloadState::Done,
                ConversionState::Failed => DownloadState::Failed,
            },
            progress: status.progress,
            size: status.size,
            error: status.error.clone(),
            url: format!("/api/conversions/{}/file", status.id),
        }
    }
}

/// `POST /api/conversions`: what a download of the item takes on the
/// player of `req.caps`, decided as a playback is. The original, when it
/// plays there as it is; else one file made by the transcoder of what a
/// playback would stream (the video copied or made anew, the sound made
/// AAC), once for every player that needs the same.
pub async fn convert(
    State(state): State<AppState>,
    user: AuthUser,
    Json(mut req): Json<PlaybackRequest>,
) -> ApiResult<Json<DownloadInfo>> {
    let row = item_or_404(&state, user.id, req.item_id).await?;
    if !matches!(row.kind.as_str(), "movie" | "episode" | "extra") {
        return Err(AppError::bad_request("only movies, episodes and extras are downloaded"));
    }
    let transcoder = state.transcoder.info();
    req.hdr_transcodes = transcoder.as_ref().is_some_and(|info| info.capabilities.hdr_transcode);
    req.download = true;
    req.copy_video_only = !state.settings.get().transcode_video;
    let files = catalog::files_for_item(&state.db, row.id).await?;
    let file = match req.file_id {
        Some(id) => files.into_iter().find(|f| f.id == id).ok_or_else(|| AppError::not_found("file not found"))?,
        None => files.into_iter().next().ok_or_else(|| AppError::not_found("this item has no media file"))?,
    };
    let info = file.media_info();
    let streams = file.streams();
    let decision = decide_picked(&state, user.id, &row, &file, info.as_ref(), &streams, &mut req).await?;
    if decision.method == Method::Direct {
        return Ok(Json(DownloadInfo {
            id: None,
            state: DownloadState::Direct,
            progress: 1.0,
            size: u64::try_from(file.size).ok(),
            error: None,
            url: format!("/api/files/{}/stream", file.id),
        }));
    }
    if let Some(why) = unwanted_conversion(&req, &decision, info.as_ref()) {
        return Err(video_not_converted(why.into_iter()));
    }
    // A player that opens no Matroska (QuickTime) gets MP4.
    let container = req.caps.download_container();
    // Not running (starting, restarting): a moment's trouble. Running but
    // too old to make files: for good.
    match transcoder {
        None => return Err(AppError::Unavailable("the transcoder isn't running: try again in a moment".into())),
        Some(info) if !info.capabilities.conversions => {
            return Err(AppError::NotImplemented(
                "this server's transcoder can't make files for downloading: update it, or download the original".into(),
            ));
        }
        Some(info) if container == FileContainer::Mp4 && !info.capabilities.conversions_mp4 => {
            return Err(AppError::NotImplemented(
                "this server's transcoder can't make MP4 files: update it, or download the original".into(),
            ));
        }
        Some(_) => {}
    }
    let job = JobRequest {
        input: file.path.clone(),
        media: info,
        label: Some(label(&row)),
        owner: Some(user.username.clone()),
        video: decision.video.clone(),
        audio: decision.audio.clone(),
        subtitle: decision.subtitle.clone(),
        segment_seconds: None,
        start_ms: None,
        options: state.settings.get().transcode_options(),
        container: (container == FileContainer::Mp4).then_some(container),
        variants: Vec::new(),
        segment_format: SegmentFormat::Ts,
    };
    let status = state.transcoder.create_conversion(&job).await.map_err(|err| match AppError::from(err) {
        // The file can't be read now (a share asleep, unmounted): a moment's
        // trouble for a download, which asks again later.
        AppError::NotFound(_) => AppError::Unavailable("this title's file can't be read right now".into()),
        err => for_viewer(&user, err),
    })?;
    state.sessions.note_conversion(&status.id, file.id, user.id);
    Ok(Json(DownloadInfo::for_viewer(&status, &user)))
}

/// The file conversion `id` is made of, when this user may see it.
async fn conversion_of(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<i64> {
    let file_id = state.sessions.conversion_file(id).ok_or_else(|| AppError::not_found("conversion not found"))?;
    visible_file(state, user, file_id).await?;
    Ok(file_id)
}

/// `GET /api/conversions/{id}`: how a conversion goes.
pub async fn conversion(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<DownloadInfo>> {
    conversion_of(&state, &user, &id).await?;
    let status = state.transcoder.conversion(&id).await.map_err(|err| for_viewer(&user, err))?;
    Ok(Json(DownloadInfo::for_viewer(&status, &user)))
}

/// `DELETE /api/conversions/{id}`: the download that wanted it went before
/// the file was made, so the conversion stops. One done stays, for whoever
/// asks for the same next.
pub async fn cancel_conversion(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    conversion_of(&state, &user, &id).await?;
    // Someone else waiting for the same file goes on getting it: only the
    // last to give it up (or an administrator) stops it.
    if !state.sessions.give_up_conversion(&id, user.id) && !user.is_admin {
        return Ok(StatusCode::NO_CONTENT);
    }
    let status = state.transcoder.conversion(&id).await.map_err(|err| for_viewer(&user, err))?;
    if status.state != ConversionState::Done {
        state.transcoder.delete_conversion(&id).await.map_err(|err| for_viewer(&user, err))?;
        state.sessions.forget_conversion(&id);
        tracing::info!("conversion {id} stopped: its download was deleted");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/conversions/{id}/file`: the converted file once whole, with
/// ranges (a download cut short goes on), from the transcoder as it comes.
pub async fn conversion_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    request: Request,
) -> ApiResult<Response> {
    conversion_of(&state, &user, &id).await?;
    let range = request.headers().get(header::RANGE);
    let upstream = state.transcoder.conversion_file(&id, range).await.map_err(|err| for_viewer(&user, err))?;
    let mut response =
        Response::builder().status(upstream.status().as_u16()).header(header::CACHE_CONTROL, "private, no-cache");
    for name in [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES] {
        if let Some(value) = upstream.headers().get(name.as_str()) {
            response = response.header(name, value.as_bytes());
        }
    }
    response.body(Body::from_stream(upstream.bytes_stream())).map_err(|e| AppError::Internal(e.into()))
}

pub async fn hls_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    let session = owned_session(&state, &user, &id)?;
    session_hls(&state, &user, &session, &file).await
}

/// A playlist, init segment or segment (`index.m3u8`, `master.m3u8`,
/// `init.mp4`, `{n}.ts`, `{n}.m4s`) of a transcode session, for `user`.
async fn session_hls(state: &AppState, user: &AuthUser, session: &Arc<Session>, file: &str) -> ApiResult<Response> {
    if session.touch() {
        state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
    }
    if session.transcode.is_none() {
        return Err(AppError::not_found("not a transcode session"));
    }
    // A library taken away or an age limit lowered since it started ends it
    // too, as it does direct play.
    if !catalog::is_visible(&state.db, user.id, session.item_id).await? {
        return Err(AppError::not_found("playback session expired"));
    }
    let job = session.job_id().ok_or_else(|| AppError::not_found("playback session expired"))?;

    let kind = hls_file_kind(file).ok_or_else(|| AppError::not_found("unknown playlist file"))?;
    if kind == HlsFile::Playlist {
        let playlist = match state.transcoder.playlist_file(&job, file).await {
            Err(err) if err.is_job_gone() => {
                let job = replacement_job(state, user, session, &job).await?;
                state.transcoder.playlist_file(&job, file).await?
            }
            other => other?,
        };
        return Ok((
            [(header::CONTENT_TYPE, HlsFile::Playlist.content_type()), (header::CACHE_CONTROL, "no-cache")],
            playlist,
        )
            .into_response());
    }

    let upstream = match state.transcoder.segment_file(&job, file).await {
        Err(err) if err.is_job_gone() => {
            let job = replacement_job(state, user, session, &job).await?;
            state.transcoder.segment_file(&job, file).await?
        }
        other => other?,
    };
    // A name in the other format than the job's is the transcoder's 404.
    let mut response =
        Response::builder().header(header::CONTENT_TYPE, kind.content_type()).header(header::CACHE_CONTROL, "no-cache");
    if let Some(len) = upstream.content_length() {
        response = response.header(header::CONTENT_LENGTH, len);
    }
    response.body(Body::from_stream(upstream.bytes_stream())).map_err(|e| AppError::Internal(e.into()))
}

/// What a transcode session's file is, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HlsFile {
    /// `master.m3u8`, `index.m3u8`, a lower rendition's `v1.m3u8`…
    Playlist,
    /// `12.ts`, `v1-12.ts`, or in fMP4 `12.m4s`, `v1-12.m4s`…
    Segment(SegmentFormat),
    /// An fMP4 rendition's init segment: `init.mp4`, `v1-init.mp4`…
    Init,
}

impl HlsFile {
    /// The Content-Type it is served with.
    fn content_type(self) -> &'static str {
        match self {
            HlsFile::Playlist => "application/vnd.apple.mpegurl",
            HlsFile::Segment(format) => format.content_type(),
            HlsFile::Init => SegmentFormat::Fmp4.content_type(),
        }
    }
}

/// What a transcode session's file is by its name; `None` for anything
/// else.
fn hls_file_kind(name: &str) -> Option<HlsFile> {
    let rendition = |s: &str| s.strip_prefix('v').and_then(|k| k.parse::<u32>().ok()).is_some_and(|k| k > 0);
    let number = |s: &str| !s.is_empty() && s.len() <= 9 && s.bytes().all(|b| b.is_ascii_digit());
    if let Some(stem) = name.strip_suffix(".m3u8") {
        return (stem == "index" || stem == "master" || rendition(stem)).then_some(HlsFile::Playlist);
    }
    if let Some(stem) = name.strip_suffix(".mp4") {
        let init = match stem.split_once('-') {
            Some((k, rest)) => rendition(k) && rest == "init",
            None => stem == "init",
        };
        return init.then_some(HlsFile::Init);
    }
    let (stem, format) = match name.rsplit_once('.')? {
        (stem, "ts") => (stem, SegmentFormat::Ts),
        (stem, "m4s") => (stem, SegmentFormat::Fmp4),
        _ => return None,
    };
    let segment = match stem.split_once('-') {
        Some((k, n)) => rendition(k) && number(n),
        None => number(stem),
    };
    segment.then_some(HlsFile::Segment(format))
}

/// `GET /api/media/{key}/{file}`: a session's media for a player that can't
/// send the Authorization header, a TV's own player (docs/API.md, "Keyed
/// URLs"). The key its play request asked for (`keyed_urls`) stands in for
/// the token, for this session's playlists and segments or its file, its
/// seek previews and its file's PGS pictures, and nothing else; it ends with
/// the session, the account, or the app's sign-in. A session stopped from
/// elsewhere answers `410` here too.
pub async fn keyed_media(
    State(state): State<AppState>,
    Path((key, file)): Path<(String, String)>,
    Query(query): Query<SubtitleQuery>,
    request: Request,
) -> ApiResult<Response> {
    let Some(session) = state.sessions.by_media_key(&key) else {
        return Err(match state.sessions.stopped_by_media_key(&key) {
            Some(stopped) => AppError::SessionStopped(stopped.message),
            None => AppError::not_found("playback session expired"),
        });
    };
    let gone = || AppError::not_found("playback session expired");
    let mut user = AuthUser::load(&state.db, session.user_id).await?.ok_or_else(gone)?;
    if let Some(grant) = session.app_grant {
        // Signed out since: the key goes with the token.
        let access = crate::auth::grants::access_of(&state.db, grant).await?.ok_or_else(gone)?;
        user.is_admin = false;
        user.app = Some(Arc::new(access));
    }
    // Signed out since because the server requires two-step sign-in.
    for id in std::iter::once(user.id).chain(user.app.as_ref().map(|app| app.approved_by)) {
        if state.auth.lacks_two_step(id).await? {
            return Err(gone());
        }
    }
    // The file's seek previews as a BIF file, which Roku's player fetches
    // itself, without the Authorization header.
    if file == "previews.bif" {
        let row = visible_file(&state, &user, session.file_id).await?;
        let Some(bytes) = state.trickplay.bif(&row).await.map_err(AppError::Internal)? else {
            state.trickplay.request(&row);
            return Err(AppError::not_found("seek previews are not ready yet"));
        };
        return Ok((
            [(header::CONTENT_TYPE, "application/octet-stream"), (header::CACHE_CONTROL, "private, no-store")],
            bytes,
        )
            .into_response());
    }
    // PGS subtitles as pictures, for a player that can't decode them
    // (Roku's), which fetches them itself too.
    if let Some(name) = file.strip_prefix("subtitles-") {
        let row = visible_file(&state, &user, session.file_id).await?;
        return keyed_pictures(&state, &user, row, name, query.at()).await;
    }
    if session.transcode.is_some() {
        return session_hls(&state, &user, &session, &file).await;
    }
    // Direct play: the session's own file, whatever its name says ("file.mkv").
    if file.split('.').next() != Some("file") {
        return Err(AppError::not_found("unknown file"));
    }
    if session.touch() {
        state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
    }
    let row = visible_file(&state, &user, session.file_id).await?;
    let mut response = serve_library_file(&row.path, request.method(), request.headers()).await?;
    // Never cached anywhere: the address carries a key.
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

/// What a keyed session's file's PGS track `index` shows, for a player that
/// can't decode PGS (Roku's), made of the track's `.mks` ([`picture_track`])
/// as far as it is copied, from around `at` first (docs/API.md, "Keyed
/// URLs"): `subtitles-{index}.json` its pictures as cues (when, where), with
/// `X-Pley-Complete` as for the `.mks`; `subtitles-{index}-{start_ms}.png`
/// the picture that shows from `start_ms`, with the `at` of the list that
/// named it. `404` for anything else.
async fn keyed_pictures(
    state: &AppState,
    user: &AuthUser,
    file: catalog::FileRow,
    name: &str,
    at: Option<i64>,
) -> ApiResult<Response> {
    let unknown = || AppError::not_found("unknown file");
    let (index, start_ms) = match name.strip_suffix(".json") {
        Some(index) => (index, None),
        None => {
            let (index, start) = name.strip_suffix(".png").and_then(|n| n.split_once('-')).ok_or_else(unknown)?;
            let start = start.parse::<i64>().ok().filter(|start| (0..=MAX_AT_MS).contains(start));
            (index, Some(start.ok_or_else(unknown)?))
        }
    };
    let index: i64 = index.parse().map_err(|_| unknown())?;
    let pgs = file
        .streams()
        .iter()
        .any(|s| s.index == index && s.kind == StreamKind::Subtitle && s.codec == "hdmv_pgs_subtitle");
    if !pgs {
        return Err(AppError::not_found("no PGS subtitle track has this index"));
    }
    // A picture is made of the track as its list was: while the track is
    // copied, the transcoder answers the same place with the same pictures,
    // where one asked around the picture's own time could start a read of
    // its own. Without its list's `at`, around its own time.
    let at = at.or(start_ms);
    let (mks, complete) = picture_track_so_far(state, user, &file, index, at).await?;
    // A film's track is tens of megabytes: read off the async threads.
    let made = tokio::task::spawn_blocking(move || {
        let pictures = crate::pgs::PgsPictures::read(&mks).ok_or_else(|| anyhow::anyhow!("not a PGS track"))?;
        let Some(start_ms) = start_ms else {
            let cues = if complete { pictures.cues() } else { pictures.cues_so_far() };
            let cues = cues.into_iter().map(|c| PictureCueJson::of(c, index, at)).collect();
            let body = PictureCues { complete, width: pictures.width, height: pictures.height, cues };
            return Ok(Some((serde_json::to_vec(&body)?, "application/json")));
        };
        let Some(picture) = pictures.picture(start_ms) else { return Ok(None) };
        let image = image::RgbaImage::from_raw(picture.width, picture.height, picture.rgba)
            .ok_or_else(|| anyhow::anyhow!("a picture's pixels don't fit its size"))?;
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
        anyhow::Ok(Some((png, "image/png")))
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .map_err(AppError::Internal)?;
    let (body, content_type) = made.ok_or_else(|| AppError::not_found("no picture starts then"))?;
    let mut response = (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            // The address carries a key.
            (header::CACHE_CONTROL, HeaderValue::from_static("private, no-store")),
        ],
        body,
    )
        .into_response();
    if start_ms.is_none() {
        let complete = HeaderValue::from_static(if complete { "1" } else { "0" });
        response.headers_mut().insert(SUBTITLES_COMPLETE_HEADER, complete);
    }
    Ok(response)
}

/// `subtitles-{index}.json` of a keyed session (see [`keyed_pictures`]).
#[derive(Serialize)]
struct PictureCues {
    /// Whether these are all of the track's pictures (else ask again).
    complete: bool,
    /// The plane the pictures are placed on.
    width: u32,
    height: u32,
    cues: Vec<PictureCueJson>,
}

#[derive(Serialize)]
struct PictureCueJson {
    start_ms: i64,
    end_ms: i64,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// The picture's address, beside this file's: with the `at` this list
    /// was asked for with, which it is made of the track around.
    image: String,
}

impl PictureCueJson {
    fn of(cue: crate::pgs::PictureCue, index: i64, at: Option<i64>) -> Self {
        let crate::pgs::PictureCue { start_ms, end_ms, x, y, width, height } = cue;
        let image = match at {
            Some(at) => format!("subtitles-{index}-{start_ms}.png?at={at}"),
            None => format!("subtitles-{index}-{start_ms}.png"),
        };
        Self { start_ms, end_ms, x, y, width, height, image }
    }
}

/// The whole picture tracks read last for [`picture_track_so_far`], by cache
/// file, with when each was asked for last: a player asks for a picture
/// every few seconds, each made of the whole track (tens of megabytes). A
/// cache file never changes: a new version of the media file has a name of
/// its own.
static WHOLE_PICTURE_TRACKS: LazyLock<std::sync::Mutex<WholeTracks>> = LazyLock::new(Default::default);
/// How many whole tracks are kept: one for each of a few players.
const WHOLE_PICTURE_TRACKS_KEPT: usize = 2;
/// How long a whole track no player asks for is kept.
const WHOLE_PICTURE_TRACK_IDLE: Duration = Duration::from_secs(5 * 60);

/// The whole picture tracks kept ([`WHOLE_PICTURE_TRACKS`]), by cache file,
/// with when each was asked for last.
#[derive(Default)]
struct WholeTracks {
    kept: Vec<(PathBuf, Bytes, Instant)>,
    /// Whether a task lets go of the tracks no player asked for lately.
    pruning: bool,
}

impl WholeTracks {
    /// The track cached at `cache`, when it is kept, asked for `now`.
    fn get(&mut self, cache: &std::path::Path, now: Instant) -> Option<Bytes> {
        let (_, bytes, asked) = self.kept.iter_mut().find(|(path, ..)| path == cache)?;
        *asked = now;
        Some(bytes.clone())
    }

    /// Keeps the track cached at `cache`, in place of the one asked for
    /// least lately. Two players reading the same file at once keep it once.
    fn keep(&mut self, cache: PathBuf, bytes: Bytes, now: Instant) {
        if self.kept.iter().any(|(path, ..)| *path == cache) {
            return;
        }
        if self.kept.len() >= WHOLE_PICTURE_TRACKS_KEPT
            && let Some(oldest) = (0..self.kept.len()).min_by_key(|&i| self.kept[i].2)
        {
            self.kept.remove(oldest);
        }
        self.kept.push((cache, bytes, now));
    }

    /// Lets go of the tracks no player asked for in
    /// [`WHOLE_PICTURE_TRACK_IDLE`]; whether any are left.
    fn prune(&mut self, now: Instant) -> bool {
        self.kept.retain(|(.., asked)| now.saturating_duration_since(*asked) < WHOLE_PICTURE_TRACK_IDLE);
        !self.kept.is_empty()
    }
}

/// The whole track cached at `cache`, when it is kept.
fn kept_whole_track(cache: &std::path::Path) -> Option<Bytes> {
    WHOLE_PICTURE_TRACKS.lock().unwrap_or_else(|e| e.into_inner()).get(cache, Instant::now())
}

/// Keeps the whole track cached at `cache` until no player has asked for it
/// for [`WHOLE_PICTURE_TRACK_IDLE`].
fn keep_whole_track(cache: PathBuf, bytes: Bytes) {
    let mut tracks = WHOLE_PICTURE_TRACKS.lock().unwrap_or_else(|e| e.into_inner());
    tracks.keep(cache, bytes, Instant::now());
    if std::mem::replace(&mut tracks.pruning, true) {
        return;
    }
    tokio::spawn(async {
        loop {
            tokio::time::sleep(WHOLE_PICTURE_TRACK_IDLE).await;
            let mut tracks = WHOLE_PICTURE_TRACKS.lock().unwrap_or_else(|e| e.into_inner());
            if !tracks.prune(Instant::now()) {
                tracks.pruning = false;
                return;
            }
        }
    });
}

/// Why the transcoder can't copy a picture track now: one that isn't
/// running (starting, restarting) is a moment's trouble, which players ask
/// again for; one too old to copy it is for good, and a player that can't
/// draw the track any other way has it burned in.
fn subtitle_copies_refused(running: bool) -> AppError {
    AppError::Unavailable(if running {
        "the transcoder can't copy subtitle tracks yet: update it".into()
    } else {
        "the transcoder isn't running: try again in a moment".into()
    })
}

/// Picture track `index` of `file` as far as it is copied, and whether that
/// is all of it: the cache's, else what the transcoder has copied so far
/// (from around `at` first), cached once whole.
async fn picture_track_so_far(
    state: &AppState,
    user: &AuthUser,
    file: &catalog::FileRow,
    index: i64,
    at: Option<i64>,
) -> ApiResult<(Bytes, bool)> {
    let (cache, path, stream_index) = picture_source(state, file, index).await?;
    if let Some(bytes) = kept_whole_track(&cache) {
        return Ok((bytes, true));
    }
    if let Ok(bytes) = tokio::fs::read(&cache).await
        && bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
    {
        let bytes = Bytes::from(bytes);
        keep_whole_track(cache, bytes.clone());
        return Ok((bytes, true));
    }
    if !state.transcoder.copies_subtitles_partially() {
        return Err(subtitle_copies_refused(state.transcoder.is_available()));
    }
    let media = file_media(file, stream_index);
    let (bytes, complete) = state
        .transcoder
        .subtitle_matroska_so_far(&path, stream_index, at, media, false)
        .await
        .map_err(|err| for_viewer(user, err))?;
    if complete {
        cache_subtitles(&cache, &bytes).await;
    }
    Ok((bytes, complete))
}

/// The extension a keyed file's address ends with: its container's, when
/// it is a plain word ("mkv", "mp4"), else "bin".
fn container_extension(container: Option<&str>) -> String {
    match container {
        Some(c) if !c.is_empty() && c.len() <= 8 && c.chars().all(|ch| ch.is_ascii_alphanumeric()) => {
            c.to_ascii_lowercase()
        }
        _ => "bin".to_owned(),
    }
}

/// A media file of an item the user may see.
pub(crate) async fn visible_file(state: &AppState, user: &AuthUser, id: i64) -> ApiResult<catalog::FileRow> {
    let file = catalog::file(&state.db, id).await?.ok_or_else(|| AppError::not_found("file not found"))?;
    if !catalog::is_visible(&state.db, user.id, file.item_id).await? {
        return Err(AppError::not_found("file not found"));
    }
    // A link swapped in since the scan leads nowhere else, for whatever the
    // file is used for: transcodes and downloads too, not only direct play
    // ([`crate::library::walk::leads_to_its_kind`]). A file gone (a share
    // asleep) is left to what reads it, which says so its own way.
    let path = std::path::PathBuf::from(&file.path);
    let astray = tokio::task::spawn_blocking(move || {
        std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink())
            && !crate::library::walk::leads_to_its_kind(&path)
    })
    .await
    .unwrap_or(true);
    if astray {
        return Err(AppError::not_found("file not found"));
    }
    Ok(file)
}

/// Seek-bar previews of a file; `404` while they are being made (asking
/// queues them).
pub async fn trickplay(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<pley_transcoder_api::TrickplayInfo>> {
    let file = visible_file(&state, &user, id).await?;
    match state.trickplay.index(&file) {
        Some(index) => Ok(Json(index)),
        None => {
            state.trickplay.request(&file);
            Err(AppError::not_found("seek previews are not ready yet"))
        }
    }
}

/// One sprite sheet (`{n}.jpg`); versioned by the `?v=` the client adds.
pub async fn trickplay_sheet(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, sheet)): Path<(i64, String)>,
) -> ApiResult<Response> {
    let n: u32 =
        sheet.strip_suffix(".jpg").and_then(|n| n.parse().ok()).ok_or_else(|| AppError::not_found("unknown sheet"))?;
    let file = visible_file(&state, &user, id).await?;
    let path = state.trickplay.sheet(&file, n).ok_or_else(|| AppError::not_found("unknown sheet"))?;
    let bytes = tokio::fs::read(path).await.map_err(|e| AppError::Internal(e.into()))?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=31536000, immutable")], bytes)
        .into_response())
}

/// A chapter's picture (`{n}.jpg`), cut from the seek previews.
pub async fn chapter_image(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, name)): Path<(i64, String)>,
) -> ApiResult<Response> {
    let n: usize =
        name.strip_suffix(".jpg").and_then(|n| n.parse().ok()).ok_or_else(|| AppError::not_found("unknown chapter"))?;
    let file = visible_file(&state, &user, id).await?;
    let path = state
        .trickplay
        .chapter_image(&file, n)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::not_found("no chapter image"))?;
    let bytes = tokio::fs::read(path).await.map_err(|e| AppError::Internal(e.into()))?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=31536000, immutable")], bytes)
        .into_response())
}

/// Direct play: the raw file with HTTP range support.
pub async fn stream_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    request: Request,
) -> ApiResult<Response> {
    let file = visible_file(&state, &user, id).await?;
    let mut response = serve_library_file(&file.path, request.method(), request.headers()).await?;
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    Ok(response)
}

/// A file of a library (a media file, or one beside it) served as it is,
/// with ranges, from the file opened and checked
/// ([`crate::library::walk::open_of_its_kind`]): not opened again by its
/// name, which a link swapped in meanwhile could lead elsewhere. Its tag
/// (size and time) lets a download carry on where it stopped (`If-Range`)
/// and a copy kept be asked about (`If-None-Match`).
pub(crate) async fn serve_library_file(
    path: &str,
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
) -> ApiResult<Response> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let named = PathBuf::from(path);
    let (file, meta) = tokio::task::spawn_blocking(move || crate::library::walk::open_of_its_kind(&named))
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::not_found("file not found"))?;
    let len = meta.len();
    let etag = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| format!("\"{len:x}-{:x}.{:x}\"", since.as_secs(), since.subsec_nanos()));
    let told = |name: header::HeaderName| headers.get(name).and_then(|v| v.to_str().ok());
    if let (Some(etag), Some(kept)) = (&etag, told(header::IF_NONE_MATCH))
        && kept.split(',').map(|tag| tag.trim().trim_start_matches("W/")).any(|tag| tag == etag || tag == "*")
    {
        let mut same = StatusCode::NOT_MODIFIED.into_response();
        if let Ok(value) = HeaderValue::from_str(etag) {
            same.headers_mut().insert(header::ETAG, value);
        }
        return Ok(same);
    }
    // A range of what the client has, if what it has is still this file.
    let range =
        headers.get(header::RANGE).filter(|_| told(header::IF_RANGE).is_none_or(|tag| Some(tag) == etag.as_deref()));
    let (status, first, count) = match byte_range(range, len) {
        Ok(None) => (StatusCode::OK, 0, len),
        Ok(Some((first, last))) => (StatusCode::PARTIAL_CONTENT, first, last - first + 1),
        Err(()) => {
            let mut none = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{len}")) {
                none.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            return Ok(none);
        }
    };
    let body = if method == axum::http::Method::HEAD {
        Body::empty()
    } else {
        let mut file = tokio::fs::File::from_std(file);
        file.seek(std::io::SeekFrom::Start(first)).await.map_err(|e| AppError::Internal(e.into()))?;
        // Read 64 KiB at a time, as `ServeFile` does: 4 KiB reads would each
        // be a trip to a blocking thread.
        Body::from_stream(tokio_util::io::ReaderStream::with_capacity(file.take(count), 64 * 1024))
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let content_type = mime_guess::from_path(path).first_raw().unwrap_or("application/octet-stream");
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(count));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Some(value) = etag.as_deref().and_then(|tag| HeaderValue::from_str(tag).ok()) {
        headers.insert(header::ETAG, value);
    }
    if status == StatusCode::PARTIAL_CONTENT
        && let Ok(value) = HeaderValue::from_str(&format!("bytes {first}-{}/{len}", first + count - 1))
    {
        headers.insert(header::CONTENT_RANGE, value);
    }
    Ok(response)
}

/// What a subtitle file is asked for with.
#[derive(Deserialize, Default)]
pub struct SubtitleQuery {
    /// A text track's cues moved this much (positive: later), for players
    /// that can't move them themselves (Roku's subtitle sync).
    offset_ms: Option<i64>,
    /// Where the player is (ms of the file's time), for a picture track not
    /// copied out of its file yet: the pictures around there come first.
    at: Option<i64>,
}

/// The furthest into a file a player is taken to be: two days, past any
/// film or recording.
const MAX_AT_MS: i64 = 48 * 3_600_000;

impl SubtitleQuery {
    /// Where the player is, within a file's times, to the second: a player
    /// asking again a moment later asks for the same pictures.
    fn at(&self) -> Option<i64> {
        self.at.map(|at| at.clamp(0, MAX_AT_MS) / 1000 * 1000)
    }
}

/// `/api/files/{id}/subtitles/{index}.vtt`: a text track (or subtitle file)
/// as WebVTT, moved by `offset_ms` when asked; `{index}.mks`: a picture
/// track in a Matroska file of its own; `{index}.sup`: a PGS track as a
/// `.sup` file (Kodi's).
pub async fn subtitles(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, name)): Path<(i64, String)>,
    Query(query): Query<SubtitleQuery>,
    request: Request,
) -> ApiResult<Response> {
    let at = query.at();
    if let Some(index) = name.strip_suffix(".mks") {
        let index: i64 = index.parse().map_err(|_| AppError::not_found("unknown subtitle file"))?;
        let file = visible_file(&state, &user, id).await?;
        return picture_track(&state, &user, file, index, at, request).await;
    }
    if let Some(index) = name.strip_suffix(".sup") {
        let index: i64 = index.parse().map_err(|_| AppError::not_found("unknown subtitle file"))?;
        let file = visible_file(&state, &user, id).await?;
        return pgs_track(&state, &user, file, index, request).await;
    }
    let index: i64 = name
        .strip_suffix(".vtt")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| AppError::not_found("unknown subtitle file"))?;
    let file = visible_file(&state, &user, id).await?;
    let (sidecar, version) = subtitle_source(&state, &file, index).await?;
    let offset = query.offset_ms.unwrap_or(0).clamp(-webvtt::MAX_OFFSET_MS, webvtt::MAX_OFFSET_MS);
    let moved = |text: String| if offset == 0 { text } else { webvtt::shift(&text, offset) };
    let cache =
        state.config.cache_dir().join("subtitles").join(format!("{}_{}_{}{version}.vtt", file.id, index, file.mtime));
    // Anything but WebVTT is a file cut short by an earlier version (written
    // in place) that crashed or ran out of space: it is made again.
    if let Ok(text) = tokio::fs::read_to_string(&cache).await
        && text.starts_with("WEBVTT")
    {
        return Ok(vtt(moved(text)));
    }
    let Some(path) = sidecar else {
        // An embedded track: the cues found so far, at once, while the rest
        // of the file is read (minutes for a big one on a network share).
        // The player asks again until it has them all.
        let (text, complete) =
            state.transcoder.subtitles_so_far(&file.path, index).await.map_err(|err| for_viewer(&user, err))?;
        if complete {
            cache_subtitles(&cache, text.as_bytes()).await;
            return Ok(vtt(moved(text)));
        }
        return Ok((
            [
                (header::CONTENT_TYPE, "text/vtt; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
                (header::HeaderName::from_static(SUBTITLES_COMPLETE_HEADER), "0"),
            ],
            moved(text),
        )
            .into_response());
    };
    let transcoder = state.transcoder.clone();
    let made = extract_once(cache, async move { Ok(transcoder.subtitles(&path, None).await?.into_bytes()) })
        .await
        .map_err(|err| for_viewer(&user, err))?;
    Ok(vtt(moved(String::from_utf8_lossy(&made).into_owned())))
}

/// Where subtitle `index` of `file` comes from: a sidecar's path (`None`:
/// the track is in the file), and what tells its versions apart in cache
/// names. A sidecar can be replaced without its video changing (subtitle
/// downloaders upgrade them): its own time is part of the name too.
pub(crate) async fn subtitle_source(
    state: &AppState,
    file: &catalog::FileRow,
    index: i64,
) -> ApiResult<(Option<String>, String)> {
    if index < EXTERNAL_INDEX_BASE {
        return Ok((None, String::new()));
    }
    // An added track's file never changes: its id names its version.
    if index >= ADDED_INDEX_BASE {
        let sub = file
            .added_subs()
            .into_iter()
            .find(|s| s.index == index)
            .ok_or_else(|| AppError::not_found("subtitle not found"))?;
        let path = sub.path(&state.config.added_subtitles_dir(), file.id);
        return Ok((Some(path.to_string_lossy().into_owned()), format!("_a{}", sub.id)));
    }
    let sub = file
        .external_subs()
        .into_iter()
        .find(|s| s.index == index)
        .filter(|s| crate::library::walk::leads_to_its_kind(std::path::Path::new(&s.path)))
        .ok_or_else(|| AppError::not_found("subtitle not found"))?;
    let modified = tokio::fs::metadata(&sub.path).await.ok().and_then(|m| m.modified().ok());
    Ok((Some(sub.path), format!("_{}", modified.map(crate::util::system_time_secs).unwrap_or(0))))
}

/// What `.mks` files are served as.
const MATROSKA: &str = "video/x-matroska";

/// A picture subtitle track (PGS, VobSub, DVB; in the file or next to it)
/// copied as it is into a Matroska file of its own, for players that show
/// it next to an HLS stream instead of having it drawn onto the video. Its
/// times are the original's from the start of the file, as the stream's
/// are. Made once per file version (by the transcoder) and cached, then
/// served from the cache with ranges. `404` for a text track or anything
/// but subtitles.
///
/// While the transcoder copies it (seconds through a Matroska file's Cues,
/// the pictures around where the player is, `at`, first; FFmpeg reads a file
/// whose Cues don't list it whole), the answer is what is copied so far: those
/// pictures, or FFmpeg's Matroska up to its last whole cluster, with
/// `X-Pley-Complete: 0`. Players ask again (every few seconds) for the rest,
/// as for text tracks. Every answer is a Matroska file players read, its
/// track's header at least. A whole one says `X-Pley-Complete: 1`.
async fn picture_track(
    state: &AppState,
    user: &AuthUser,
    file: catalog::FileRow,
    index: i64,
    at: Option<i64>,
    request: Request,
) -> ApiResult<Response> {
    let picture = file
        .streams()
        .iter()
        .any(|s| s.index == index && s.kind == StreamKind::Subtitle && !s.text_based.unwrap_or(true));
    if !picture {
        return Err(AppError::not_found("no picture subtitle track has this index"));
    }
    let (cache, path, stream_index) = picture_source(state, &file, index).await?;
    if !is_matroska(&cache).await {
        if !state.transcoder.copies_subtitles() {
            return Err(subtitle_copies_refused(state.transcoder.is_available()));
        }
        let made = if state.transcoder.copies_subtitles_partially() {
            let (bytes, complete) = state
                .transcoder
                .subtitle_matroska_so_far(&path, stream_index, at, file_media(&file, stream_index), false)
                .await
                .map_err(|err| for_viewer(user, err))?;
            if !complete {
                return Ok(held_bytes(request.headers(), bytes, false, MATROSKA));
            }
            cache_subtitles(&cache, &bytes).await;
            bytes
        } else {
            let transcoder = state.transcoder.clone();
            let made =
                extract_once(
                    cache.clone(),
                    async move { Ok(transcoder.subtitle_matroska(&path, stream_index).await?) },
                )
                .await
                .map_err(|err| for_viewer(user, err))?;
            Bytes::from((*made).clone())
        };
        if !is_matroska(&cache).await {
            // Not cached (a full disk): served as it is.
            return Ok(held_bytes(request.headers(), made, true, MATROSKA));
        }
    }
    let response =
        ServeFile::new(&cache).oneshot(request).await.map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;
    let mut response = response.map(Body::new);
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(MATROSKA));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    response.headers_mut().insert(SUBTITLES_COMPLETE_HEADER, HeaderValue::from_static("1"));
    Ok(response)
}

/// How long a `.sup` answer waits for its track to be whole: Kodi reads it
/// once, as the playback opens. A Matroska file's comes in seconds, read
/// through its Cues; past this, what is copied so far.
const SUP_WAIT: Duration = Duration::from_secs(8);

/// A PGS track (in the file or next to it) as a `.sup` file, for players
/// that read those next to a stream but no Matroska (Kodi): a sidecar
/// `.sup` as it is, an embedded track made of its `.mks`
/// ([`picture_track`]), with an empty display set at 0 first. Kodi reads
/// it once, as the playback opens: an answer waits up to [`SUP_WAIT`] for
/// the whole track, then has what is copied so far (`X-Pley-Complete: 0`):
/// read through the Cues, everything from around where the playback
/// started on, not only the pictures there. The player asks again later.
/// `404` for anything but PGS.
async fn pgs_track(
    state: &AppState,
    user: &AuthUser,
    file: catalog::FileRow,
    index: i64,
    request: Request,
) -> ApiResult<Response> {
    let pgs = file
        .streams()
        .iter()
        .any(|s| s.index == index && s.kind == StreamKind::Subtitle && s.codec == "hdmv_pgs_subtitle");
    if !pgs {
        return Err(AppError::not_found("no PGS subtitle track has this index"));
    }
    let (cache, path, stream_index) = picture_source(state, &file, index).await?;
    if stream_index.is_none() && path.to_ascii_lowercase().ends_with(".sup") {
        let mut response = serve_library_file(&path, request.method(), request.headers()).await?;
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(SUP));
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
        return Ok(response);
    }
    let (mks, complete) = match tokio::fs::read(&cache).await {
        Ok(bytes) if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) => (Bytes::from(bytes), true),
        _ => {
            if !state.transcoder.copies_subtitles_partially() {
                return Err(subtitle_copies_refused(state.transcoder.is_available()));
            }
            let started = std::time::Instant::now();
            loop {
                let (bytes, complete) = state
                    .transcoder
                    .subtitle_matroska_so_far(&path, stream_index, None, file_media(&file, stream_index), true)
                    .await
                    .map_err(|err| for_viewer(user, err))?;
                if complete {
                    cache_subtitles(&cache, &bytes).await;
                }
                if complete || started.elapsed() >= SUP_WAIT {
                    break (bytes, complete);
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };
    let sup = crate::pgs::from_matroska(&mks).ok_or_else(|| AppError::Internal(anyhow::anyhow!("not a PGS track")))?;
    Ok(held_bytes(request.headers(), Bytes::from(sup), complete, SUP))
}

/// What `.sup` files are served as.
const SUP: &str = "application/octet-stream";

/// Where picture track `index` of `file` is cached, and what the transcoder
/// copies it from: the file and the track's stream, or a sidecar (`None`).
async fn picture_source(
    state: &AppState,
    file: &catalog::FileRow,
    index: i64,
) -> ApiResult<(PathBuf, String, Option<i64>)> {
    let (sidecar, version) = subtitle_source(state, file, index).await?;
    let cache =
        state.config.cache_dir().join("subtitles").join(format!("{}_{}_{}{version}.mks", file.id, index, file.mtime));
    Ok(match sidecar {
        Some(path) => (cache, path, None),
        None => (cache, file.path.clone(), Some(index)),
    })
}

/// The media information of `file` for the transcoder copying a picture
/// track out of it (`stream_index`; none for a file next to it): it spares
/// a probe while a playback starts.
fn file_media(file: &catalog::FileRow, stream_index: Option<i64>) -> Option<pley_transcoder_api::MediaInfo> {
    stream_index.and_then(|_| file.media_info())
}

/// Picture tracks being prepared for a playback starting ([`prepare_picture_track`]), by cache file.
static PREPARING: LazyLock<DashMap<PathBuf, ()>> = LazyLock::new(DashMap::new);

/// Has the transcoder start copying picture track `index` of `file` out of
/// its file now (a playback starts whose player will ask for it, see
/// [`crate::playback::picture_to_prepare`]), rather than when the player
/// asks: on a network share, a file whose Cues don't list the track takes
/// minutes to read. Read through the Cues, the pictures around where the
/// playback starts (`start_ms`) come at once, the rest once the playback's
/// own start is over; FFmpeg's copy starts then. The transcoder goes on
/// alone; a copy it finishes now is cached here.
pub(crate) fn prepare_picture_track(state: &AppState, file: &catalog::FileRow, index: i64, start_ms: i64) {
    if !state.transcoder.copies_subtitles_partially() {
        return;
    }
    let (state, file) = (state.clone(), file.clone());
    tokio::spawn(async move {
        let Ok((cache, path, stream_index)) = picture_source(&state, &file, index).await else { return };
        if is_matroska(&cache).await || PREPARING.insert(cache.clone(), ()).is_some() {
            return;
        }
        let media = file_media(&file, stream_index);
        match state.transcoder.subtitle_matroska_so_far(&path, stream_index, Some(start_ms), media, false).await {
            Ok((bytes, true)) => cache_subtitles(&cache, &bytes).await,
            Ok(_) => tracing::debug!("picture subtitles {index} of file {} are being copied", file.id),
            Err(err) => tracing::debug!("picture subtitles {index} of file {} not copied: {err}", file.id),
        }
        PREPARING.remove(&cache);
    });
}

/// A picture track's file held in memory (copied so far, or whole but not
/// cached) as `content_type`, with what `X-Pley-Complete` says, and the
/// range asked for in `headers` (one, as `ServeFile` serves them). A
/// partial answer's length isn't known yet: `Content-Range` says `*` for
/// it. Of FFmpeg's copy being made, the bytes are what the file will start
/// with once whole, but for the sizes its header doesn't know yet (its
/// segment's, its duration) and the seek head FFmpeg fills in last, so a
/// player may fetch only what follows what it has; the pictures around a
/// player read through the Cues are a file of their own, which it takes in
/// place of what it has.
fn held_bytes(headers: &axum::http::HeaderMap, bytes: Bytes, complete: bool, content_type: &'static str) -> Response {
    let cache = if complete { "private, max-age=3600" } else { "no-store" };
    let mut response = match byte_range(headers.get(header::RANGE), bytes.len() as u64) {
        Ok(None) => (StatusCode::OK, bytes).into_response(),
        Ok(Some((start, end))) => {
            let total = if complete { bytes.len().to_string() } else { "*".into() };
            let range = format!("bytes {start}-{end}/{total}");
            let mut part = (StatusCode::PARTIAL_CONTENT, bytes.slice(start as usize..=end as usize)).into_response();
            if let Ok(value) = HeaderValue::from_str(&range) {
                part.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            part
        }
        Err(()) => {
            let mut none = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{}", bytes.len())) {
                none.headers_mut().insert(header::CONTENT_RANGE, value);
            }
            none
        }
    };
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(SUBTITLES_COMPLETE_HEADER, HeaderValue::from_static(if complete { "1" } else { "0" }));
    response
}

/// The byte range a `Range` header asks for of `len` bytes, first and last
/// included: `None` for all of them (no header, or one asking for several
/// ranges or in other units, which is answered whole), an error for a
/// range that starts past the end (nothing new yet, of a partial file).
fn byte_range(range: Option<&HeaderValue>, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = range.and_then(|r| r.to_str().ok()).and_then(|r| r.trim().strip_prefix("bytes=")) else {
        return Ok(None);
    };
    if spec.contains(',') {
        return Ok(None);
    }
    let Some((first, last)) = spec.split_once('-') else { return Ok(None) };
    let (first, last) = (first.trim(), last.trim());
    let range = match (first.parse::<u64>(), last.parse::<u64>()) {
        (Ok(first), Ok(last)) if first <= last => (first, last.min(len.saturating_sub(1))),
        (Ok(first), Err(_)) if last.is_empty() => (first, len.saturating_sub(1)),
        (Err(_), Ok(suffix)) if first.is_empty() && suffix > 0 => (len.saturating_sub(suffix), len.saturating_sub(1)),
        _ => return Ok(None),
    };
    if range.0 >= len { Err(()) } else { Ok(Some(range)) }
}

/// Whether `path` is a whole Matroska file (cached ones are renamed into
/// place whole, so the EBML header says enough).
async fn is_matroska(path: &std::path::Path) -> bool {
    use tokio::io::AsyncReadExt;
    let mut head = [0u8; 4];
    match tokio::fs::File::open(path).await {
        Ok(mut file) => file.read_exact(&mut head).await.is_ok() && head == [0x1A, 0x45, 0xDF, 0xA3],
        Err(_) => false,
    }
}

/// A conversion under way, and which one it is.
#[derive(Clone)]
struct Extraction {
    id: u64,
    result: Shared<BoxFuture<'static, Made>>,
}

/// What a conversion made, shared by the requests that wait for it.
type Made = Result<Arc<Vec<u8>>, Arc<AppError>>;

/// Subtitles being pulled out of their files, by cache file.
static EXTRACTIONS: LazyLock<DashMap<PathBuf, Extraction>> = LazyLock::new(DashMap::new);
static NEXT_EXTRACTION: AtomicU64 = AtomicU64::new(0);

/// What `work` makes (subtitles pulled out of their file), made once and
/// cached at `cache`: requests for the same file wait for one conversion,
/// which goes on to the cache when nobody waits any more (a player that
/// moved on, or restarted). `work` is dropped unused when one is under way.
async fn extract_once(
    cache: PathBuf,
    work: impl Future<Output = ApiResult<Vec<u8>>> + Send + 'static,
) -> ApiResult<Arc<Vec<u8>>> {
    let extraction = EXTRACTIONS
        .entry(cache.clone())
        .or_insert_with(|| {
            let id = NEXT_EXTRACTION.fetch_add(1, Ordering::Relaxed);
            let cache = cache.clone();
            let task = tokio::spawn(async move {
                let _done = Forget { cache: cache.clone(), id };
                let made = work.await?;
                cache_subtitles(&cache, &made).await;
                Ok::<_, AppError>(Arc::new(made))
            });
            let result = async move {
                match task.await {
                    Ok(result) => result.map_err(Arc::new),
                    Err(err) => Err(Arc::new(AppError::Internal(err.into()))),
                }
            };
            Extraction { id, result: result.boxed().shared() }
        })
        .clone();
    extraction.result.await.map_err(|err| err.again())
}

/// Takes a conversion out of [`EXTRACTIONS`] once it's over, however it ends
/// and whoever still waits for it: the next request finds the cache, or
/// tries again after a failure. A later conversion in its place stays.
struct Forget {
    cache: PathBuf,
    id: u64,
}

impl Drop for Forget {
    fn drop(&mut self) {
        EXTRACTIONS.remove_if(&self.cache, |_, extraction| extraction.id == self.id);
    }
}

fn vtt(text: String) -> Response {
    ([(header::CONTENT_TYPE, "text/vtt; charset=utf-8"), (header::CACHE_CONTROL, "private, max-age=3600")], text)
        .into_response()
}

/// Caches converted subtitles whole or not at all: they go to a temporary
/// file that is renamed into place, so a crash or a full disk never leaves a
/// file cut short to be served from then on.
async fn cache_subtitles(path: &std::path::Path, data: &[u8]) {
    let Some(dir) = path.parent() else { return };
    let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    let written = async {
        tokio::fs::create_dir_all(dir).await?;
        tokio::fs::write(&tmp, data).await?;
        tokio::fs::rename(&tmp, path).await
    }
    .await;
    if let Err(err) = written {
        let _ = tokio::fs::remove_file(&tmp).await;
        tracing::debug!("subtitles not cached in {}: {err}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_ranges_of_matroska_held_in_memory() {
        let range = |r: &str, len| byte_range(Some(&HeaderValue::from_str(r).unwrap()), len);
        assert_eq!(byte_range(None, 100), Ok(None));
        assert_eq!(range("bytes=0-9", 100), Ok(Some((0, 9))));
        assert_eq!(range("bytes=40-", 100), Ok(Some((40, 99))), "what follows what the player has");
        assert_eq!(range("bytes=90-200", 100), Ok(Some((90, 99))));
        assert_eq!(range("bytes=-10", 100), Ok(Some((90, 99))));
        assert_eq!(range("bytes=100-", 100), Err(()), "nothing new yet");
        assert_eq!(range("bytes=0-1,5-9", 100), Ok(None), "several: all of it");
        assert_eq!(range("items=0-1", 100), Ok(None));
        assert_eq!(range("bytes=9-0", 100), Ok(None));
    }

    #[test]
    fn players_are_where_a_file_can_be_to_the_second() {
        let at = |at: Option<i64>| SubtitleQuery { at, ..Default::default() }.at();
        assert_eq!(at(None), None);
        assert_eq!(at(Some(754_999)), Some(754_000));
        assert_eq!(at(Some(-5)), Some(0));
        assert_eq!(at(Some(i64::MAX)), Some(MAX_AT_MS));
    }

    #[test]
    fn a_transcoder_not_running_is_told_apart_from_one_too_old() {
        // Roku has picture subtitles burned in for good on the second only.
        let message = |running| match subtitle_copies_refused(running) {
            AppError::Unavailable(message) => message,
            other => panic!("{other:?}"),
        };
        assert_eq!(message(false), "the transcoder isn't running: try again in a moment");
        assert_eq!(message(true), "the transcoder can't copy subtitle tracks yet: update it");
    }

    #[test]
    fn whole_picture_tracks_are_let_go_once_no_player_asks_for_them() {
        let (a, b, c) = (PathBuf::from("a.mks"), PathBuf::from("b.mks"), PathBuf::from("c.mks"));
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        let mut tracks = WholeTracks::default();
        tracks.keep(a.clone(), Bytes::from_static(b"a"), at(0));
        // Two players reading it at once: kept once.
        tracks.keep(a.clone(), Bytes::from_static(b"a"), at(0));
        tracks.keep(b.clone(), Bytes::from_static(b"b"), at(0));
        assert_eq!(tracks.get(&a, at(60)), Some(Bytes::from_static(b"a")));
        // A third takes the place of the one asked for least lately.
        tracks.keep(c.clone(), Bytes::from_static(b"c"), at(60));
        assert_eq!(tracks.get(&b, at(60)), None);
        assert!(tracks.prune(at(300)), "asked for 4 minutes ago");
        assert!(!tracks.prune(at(360)));
        assert_eq!(tracks.get(&a, at(360)), None);
    }

    #[tokio::test]
    async fn partial_matroska_answers() {
        let bytes = Bytes::from_static(b"0123456789");
        let mut headers = axum::http::HeaderMap::new();
        let whole = held_bytes(&headers, bytes.clone(), false, MATROSKA);
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.headers()[SUBTITLES_COMPLETE_HEADER], "0");
        assert_eq!(whole.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(whole.headers()[header::CONTENT_TYPE], MATROSKA);
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=4-"));
        let rest = held_bytes(&headers, bytes.clone(), false, MATROSKA);
        assert_eq!(rest.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(rest.headers()[header::CONTENT_RANGE], "bytes 4-9/*");
        let body = axum::body::to_bytes(rest.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"456789");
        let done = held_bytes(&headers, bytes.clone(), true, MATROSKA);
        assert_eq!(done.headers()[header::CONTENT_RANGE], "bytes 4-9/10");
        assert_eq!(done.headers()[SUBTITLES_COMPLETE_HEADER], "1");
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=10-"));
        let none = held_bytes(&headers, bytes, false, MATROSKA);
        assert_eq!(none.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(none.headers()[header::CONTENT_RANGE], "bytes */10");
        assert_eq!(none.headers()[SUBTITLES_COMPLETE_HEADER], "0");
    }

    #[test]
    fn hls_files_are_known_by_their_names() {
        use HlsFile::{Init, Playlist, Segment};
        let (ts, fmp4) = (SegmentFormat::Ts, SegmentFormat::Fmp4);
        for (name, kind) in [
            ("index.m3u8", Playlist),
            ("master.m3u8", Playlist),
            ("v2.m3u8", Playlist),
            ("0.ts", Segment(ts)),
            ("12.ts", Segment(ts)),
            ("v1-12.ts", Segment(ts)),
            ("12.m4s", Segment(fmp4)),
            ("v3-0.m4s", Segment(fmp4)),
            ("init.mp4", Init),
            ("v1-init.mp4", Init),
        ] {
            assert_eq!(hls_file_kind(name), Some(kind), "{name}");
        }
        for name in [
            "v0.m3u8",
            "other.m3u8",
            "x.ts",
            ".ts",
            "1234567890.ts",
            "v0-1.ts",
            "v1-.ts",
            "v1-x.m4s",
            "-1.m4s",
            "12.m4s.p3",
            "12.mp4",
            "v0-init.mp4",
            "-init.mp4",
            "init.m4s",
            "init.ts",
            "init.mp4.p1",
            "vx-init.mp4",
            "12.mkv",
            "../init.mp4",
            "",
        ] {
            assert_eq!(hls_file_kind(name), None, "{name}");
        }
        assert_eq!(Segment(ts).content_type(), "video/mp2t");
        assert_eq!(Segment(fmp4).content_type(), "video/mp4");
        assert_eq!(Init.content_type(), "video/mp4");
        assert_eq!(Playlist.content_type(), "application/vnd.apple.mpegurl");
    }
}
