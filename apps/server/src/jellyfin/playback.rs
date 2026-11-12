//! Playback: what a device plays (`PlaybackInfo`, its device profile read
//! as Pley's own players' capabilities), the file itself with ranges, HLS
//! from Pley's transcoder, subtitles, intro and credits (media segments),
//! and the players' reports, which keep the resume point and make a Pley
//! playback session.
//!
//! Many players open the streams and subtitles they are given without a
//! token: a media source's id stands for the sign-in it was listed to
//! ([`super::ids::Ids::source`]), and works only while that sign-in does,
//! for what its user may see. HLS sessions' playlists and segments are
//! reached by the session's 128-bit id, as Pley's own are.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use pley_transcoder_api::{StreamKind, TrickplayInfo};
use serde::Serialize;

use super::dto::{self, TICKS_PER_MS};
use super::ids::Kind;
use super::items::{libraries, visible_item};
use super::profile::{caps_of, stream_caps, stream_settings};
use super::{Body, Client, Jellyfin, JfError, JfResult, JfUser, Q, QueryResult};
use crate::auth::{AuthUser, CurrentToken, SourceIp};
use crate::catalog::{self, FileRow, ItemRow};
use crate::events::ServerEvent;
use crate::oauth::clients::clean_name;
use crate::playback::{Controls, Method, PlaybackMode, PlaybackRequest, Session};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/items/{id}/playbackinfo", get(playback_info).post(playback_info))
        .route("/videos/{id}/master.m3u8", get(master))
        .route("/videos/{id}/main.m3u8", get(master))
        .route("/audio/{id}/master.m3u8", get(master))
        .route("/videos/{id}/hls1/{session}/{file}", get(session_file))
        .route("/videos/{id}/trickplay/{width}/{file}", get(trickplay))
        .route("/audio/{id}/hls1/{session}/{file}", get(session_file))
        .route("/videos/{id}/{file}", get(video_file).head(video_file))
        .route("/audio/{id}/{file}", get(audio_file).head(audio_file))
        .route("/items/{id}/download", get(download).head(download))
        .route("/videos/{id}/{source}/subtitles/{index}/{file}", get(subtitles))
        .route("/videos/{id}/{source}/subtitles/{index}/{ticks}/{file}", get(subtitles_at))
        .route("/videos/activeencodings", delete(stop_encodings))
        .route("/sessions/playing", post(playing))
        .route("/sessions/playing/progress", post(progress))
        .route("/sessions/playing/stopped", post(stopped))
        .route("/sessions/playing/ping", post(ping))
        .route("/sessions", get(sessions))
        .route("/mediasegments/{id}", get(media_segments))
        .route("/playback/bitratetest", get(bitrate_test))
}

// ------------------------------------------------------------------ playback info

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackInfoResponse {
    media_sources: Vec<dto::MediaSource>,
    play_session_id: String,
}

/// A play session's id, which clients send back in their reports.
fn new_play_session_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    hex::encode(bytes)
}

/// The file a media source id names among an item's files: a source's id
/// as this API lists them, else the first.
fn pick<'a>(jellyfin: &Jellyfin, files: &'a [FileRow], source: Option<&str>) -> Option<&'a FileRow> {
    let wanted = source.and_then(|s| jellyfin.ids.read_source(s).map(|(file, _)| file));
    wanted.and_then(|id| files.iter().find(|f| f.id == id)).or(files.first())
}

/// `POST /Items/{id}/PlaybackInfo`: each version of the item, played as it
/// is when the device's profile takes it, else with an HLS stream from
/// Pley's transcoder (`TranscodingUrl`), copied or converted as Pley's
/// own players' would be. Settings come from the body or the query.
async fn playback_info(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    Path(id): Path<String>,
    q: Q,
    body: Body,
) -> JfResult<Response> {
    let row = visible_item(&state, &jellyfin, &user, &id).await?;
    let files = catalog::files_for_item(&state.db, row.id).await?;
    if files.is_empty() {
        return Err(JfError::NotFound);
    }
    let number = |name: &str| body.number(name).or_else(|| q.number(name));
    let flag = |name: &str| body.flag(name).or_else(|| q.flag(name));
    let source = body.str("MediaSourceId").or(q.get("mediaSourceId")).map(str::to_owned);
    let profile = body.get("DeviceProfile");
    let caps = caps_of(profile);
    let max_bitrate =
        number("MaxStreamingBitrate").filter(|b| *b > 0).map(|b| (b / 1000).clamp(64, u32::MAX as i64) as u32);
    let start_ms = number("StartTimeTicks").map(|t| t / TICKS_PER_MS);
    let audio = number("AudioStreamIndex").filter(|i| *i >= 0);
    let subtitle = number("SubtitleStreamIndex").filter(|i| *i >= 0);
    let direct_allowed = flag("EnableDirectPlay").unwrap_or(true);
    let play_session = new_play_session_id();
    let token_id = user.compatibility.map(|c| c.token_id).unwrap_or(0);

    let chosen: Vec<&FileRow> = match &source {
        Some(_) => pick(&jellyfin, &files, source.as_deref()).into_iter().collect(),
        None => files.iter().collect(),
    };
    let mut sources = Vec::new();
    for file in chosen {
        // About to be played: its seek previews are made, if they aren't yet.
        if dto::is_video(&row.kind) {
            state.trickplay.request(file);
        }
        let mut out = dto::media_source(&jellyfin, &row, file, token_id);
        let streams = file.streams();
        let info = file.media_info();
        let req = PlaybackRequest {
            item_id: row.id,
            file_id: Some(file.id),
            audio_index: audio,
            subtitle_index: None,
            max_bitrate,
            max_height: None,
            start_ms,
            caps: caps.clone(),
            mode: PlaybackMode::Auto,
            client_id: client.device_id.clone(),
            keep_previous: false,
            keyed_urls: false,
            controls: Controls::new(),
            device_name: None,
            device_kind: None,
            hdr_transcodes: false,
            fmp4_segments: false,
            download: false,
            copy_video_only: !state.settings.get().transcode_video,
            stream_only: false,
        };
        let decision = crate::playback::decide(file, info.as_ref(), &streams, &req);
        // Picture subtitles chosen go onto the video, which then streams.
        let picture_subtitle = subtitle.filter(|index| {
            streams.iter().any(|s| s.index == *index && s.kind == StreamKind::Subtitle && s.text_based == Some(false))
        });
        let direct = direct_allowed && decision.method == Method::Direct && picture_subtitle.is_none();
        out.supports_direct_play = direct;
        // `/Videos/{id}/stream` sends the file as it is: what needs its
        // sound or container changed goes through the HLS address.
        out.supports_direct_stream = direct;
        if !direct {
            let settings = stream_settings(profile, info.as_ref().and_then(|i| i.video()));
            // fMP4 where it is asked for and the transcoder makes it.
            let fmp4 = settings.iter().any(|(k, v)| k == "SegmentContainer" && v == "mp4")
                && state.transcoder.info().is_some_and(|t| t.capabilities.fmp4_segments);
            let url =
                transcoding_url(&jellyfin, &row, &out.id, &play_session, &client, &req, &settings, picture_subtitle);
            out.transcoding_url = Some(url);
            out.transcoding_sub_protocol = "hls";
            out.transcoding_container = Some(if fmp4 { "mp4" } else { "ts" });
            for stream in out.media_streams.iter_mut().filter(|s| Some(s.index) == picture_subtitle) {
                stream.delivery_method = Some("Encode");
            }
        }
        if let Some(index) = audio {
            out.default_audio_stream_index = Some(index);
        }
        out.default_subtitle_stream_index = subtitle;
        sources.push(out);
    }
    Ok(Json(PlaybackInfoResponse { media_sources: sources, play_session_id: play_session }).into_response())
}

/// A stream's address: Pley's transcoder through [`master`], its settings
/// in the query, as Jellyfin's are ([`stream_settings`]), and clients add
/// their own.
#[allow(clippy::too_many_arguments)]
fn transcoding_url(
    jellyfin: &Jellyfin,
    row: &ItemRow,
    source: &str,
    play_session: &str,
    client: &Client,
    req: &PlaybackRequest,
    settings: &[(String, String)],
    burn: Option<i64>,
) -> String {
    let encode = |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
    let mut query = vec![format!("MediaSourceId={source}"), format!("PlaySessionId={play_session}")];
    query.extend(settings.iter().map(|(name, value)| format!("{}={}", encode(name), encode(value))));
    if let Some(device) = &client.device_id {
        query.push(format!("DeviceId={}", encode(device)));
    }
    if let Some(kbps) = req.max_bitrate {
        query.push(format!("MaxStreamingBitrate={}", kbps as u64 * 1000));
    }
    if let Some(index) = req.audio_index {
        query.push(format!("AudioStreamIndex={index}"));
    }
    if let Some(index) = burn {
        query.push(format!("SubtitleStreamIndex={index}&SubtitleMethod=Encode"));
    }
    if let Some(ms) = req.start_ms.filter(|ms| *ms > 0) {
        query.push(format!("StartTimeTicks={}", ms * TICKS_PER_MS));
    }
    let base = if row.kind == "track" { "audio" } else { "videos" };
    format!("/{base}/{}/master.m3u8?{}", jellyfin.ids.of(Kind::Item, row.id), query.join("&"))
}

// ------------------------------------------------------------------ who asks for a file

/// The caller of a stream, a picture track or a subtitle: its token (in a
/// header or `ApiKey`) or a media source's id that stands for one, used from
/// where the token may be, and the file asked for, which its user may see.
async fn media_caller(
    state: &AppState,
    jellyfin: &Jellyfin,
    (headers, query, from): (&HeaderMap, Option<&str>, Option<std::net::IpAddr>),
    path_id: &str,
    source: Option<&str>,
) -> JfResult<(AuthUser, FileRow)> {
    let mut user = super::token_user(state, headers, query, from).await?;
    // A source's id, as a query parameter or as the path's id (players that
    // take the source's id for the item's).
    let sourced = source.and_then(|s| jellyfin.ids.read_source(s)).or_else(|| jellyfin.ids.read_source(path_id));
    if user.is_none()
        && let Some((_, token_id)) = sourced
    {
        user = state.auth.compatibility_user(token_id, from).await.map(super::this_apis);
    }
    let user = user.ok_or(JfError::Unauthorized)?;
    let file = match sourced {
        Some((file_id, _)) => catalog::file(&state.db, file_id).await?.ok_or(JfError::NotFound)?,
        None => {
            let item = jellyfin.ids.number(Kind::Item, path_id).ok_or(JfError::NotFound)?;
            catalog::files_for_item(&state.db, item).await?.into_iter().next().ok_or(JfError::NotFound)?
        }
    };
    let row = catalog::item(&state.db, user.id, file.item_id).await?.ok_or(JfError::NotFound)?;
    if !libraries(state, &user).await?.iter().any(|l| l.id == row.library_id) {
        return Err(JfError::NotFound);
    }
    Ok((user, file))
}

/// What [`media_caller`] reads of `request`: its headers, its query and
/// where it came from.
fn asking(request: &Request) -> (&HeaderMap, Option<&str>, Option<std::net::IpAddr>) {
    (request.headers(), request.uri().query(), crate::auth::source_ip_of(request.extensions(), request.headers()))
}

/// The media file, with ranges, sent from the file that was checked.
async fn serve_file(file: &FileRow, request: Request, attachment: bool) -> JfResult<Response> {
    let mut response =
        crate::api::playback::serve_library_file(&file.path, request.method(), request.headers()).await?;
    if attachment {
        let name = std::path::Path::new(&file.path)
            .file_name()
            .map(|n| n.to_string_lossy().replace(['"', '\r', '\n'], ""))
            .unwrap_or_else(|| "file".into());
        if let Ok(value) =
            HeaderValue::from_str(&format!("attachment; filename=\"{}\"", name.replace(|c: char| !c.is_ascii(), "_")))
        {
            response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
        }
    }
    Ok(response)
}

/// `/Videos/{id}/stream[.{container}]`: the file as it is. A request for a
/// conversion (`static=false` with codecs) gets the file too: players ask
/// for streams through `PlaybackInfo`'s HLS address.
async fn video_file(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, file)): Path<(String, String)>,
    q: Q,
    request: Request,
) -> JfResult<Response> {
    if file != "stream" && !file.starts_with("stream.") {
        return Err(JfError::NotFound);
    }
    let (_, file) = media_caller(&state, &jellyfin, asking(&request), &id, q.get("mediaSourceId")).await?;
    serve_file(&file, request, false).await
}

/// `/Audio/{id}/stream[.{container}]` and `/Audio/{id}/universal`: the file
/// as it is (Pley converts music for its own players only, as HLS).
async fn audio_file(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, file)): Path<(String, String)>,
    q: Q,
    request: Request,
) -> JfResult<Response> {
    if file != "stream" && !file.starts_with("stream.") && file != "universal" {
        return Err(JfError::NotFound);
    }
    let (_, file) = media_caller(&state, &jellyfin, asking(&request), &id, q.get("mediaSourceId")).await?;
    serve_file(&file, request, false).await
}

/// `/Items/{id}/Download`: the file, to keep.
async fn download(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path(id): Path<String>,
    q: Q,
    request: Request,
) -> JfResult<Response> {
    let (_, file) = media_caller(&state, &jellyfin, asking(&request), &id, q.get("mediaSourceId")).await?;
    serve_file(&file, request, true).await
}

// ------------------------------------------------------------------ HLS

/// The settings of a stream, as `master.m3u8` takes them, that make it:
/// the same ones again are the same stream (players open the playlist once
/// to probe it, then to play).
fn stream_key(user: &AuthUser, file: &FileRow, q: &Q) -> String {
    let mut pairs: Vec<String> =
        q.0.iter()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("api_key") && !k.eq_ignore_ascii_case("apikey"))
            .map(|(k, v)| format!("{}={v}", k.to_ascii_lowercase()))
            .collect();
    pairs.sort();
    format!("{}/{}?{}", user.id, file.id, pairs.join("&"))
}

/// Streams started through `master.m3u8`, by [`stream_key`], to the Pley
/// session that plays each.
#[derive(Default)]
pub struct Streams(parking_lot::Mutex<HashMap<String, String>>);

/// Streams remembered before those that ended are forgotten.
const FORGET_STREAMS_AT: usize = 256;

/// The longest multivariant playlist read from the transcoder: a few lines
/// for each size.
const MAX_MASTER_PLAYLIST: usize = 64 * 1024;

/// `/Videos/{id}/master.m3u8`: a Pley HLS session with Jellyfin's stream
/// settings (`MaxStreamingBitrate`, `VideoBitrate`, `MaxWidth`/`MaxHeight`,
/// `AudioStreamIndex`, `SubtitleStreamIndex` with `SubtitleMethod=Encode`,
/// `StartTimeTicks`), decided as Pley's own players' streams are for what
/// the address asks ([`stream_caps`]: `VideoCodec`, `AudioCodec`,
/// `SegmentContainer`, the audio channels and each codec's limits and
/// dynamic ranges), the video copied unless `AllowVideoStreamCopy=false`.
/// It is answered with the session's own multivariant playlist, which
/// names what the stream carries (HEVC, Dolby Vision, E-AC-3, its range).
async fn master(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Extension(streams): Extension<Arc<Streams>>,
    Path(id): Path<String>,
    q: Q,
    request: Request,
) -> JfResult<Response> {
    let (user, file) = media_caller(&state, &jellyfin, asking(&request), &id, q.get("mediaSourceId")).await?;
    let row = catalog::item(&state.db, user.id, file.item_id).await?.ok_or(JfError::NotFound)?;
    let key = stream_key(&user, &file, &q);
    let running = streams.0.lock().get(&key).cloned().filter(|session| state.sessions.get(session).is_some());
    let kbps = |name: &str| q.number(name).filter(|b| *b > 0).map(|b| (b / 1000).clamp(64, u32::MAX as i64) as u32);
    let max_bitrate = match (kbps("MaxStreamingBitrate"), kbps("VideoBitrate")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let session_id = match running {
        Some(session) => session,
        None => {
            let max_height = q
                .number("maxHeight")
                .or_else(|| q.number("maxWidth").map(|w| w * 9 / 16))
                .filter(|h| *h > 0)
                .map(|h| h as u32);
            let burn = q.get("SubtitleMethod").is_some_and(|m| m.eq_ignore_ascii_case("encode"));
            let copy = q.flag("AllowVideoStreamCopy") != Some(false);
            let client = Client::of(request.headers());
            let device = q.get("DeviceId").map(str::to_owned).or(client.device_id);
            let req = PlaybackRequest {
                item_id: row.id,
                file_id: Some(file.id),
                audio_index: q.number("AudioStreamIndex").filter(|i| *i >= 0),
                subtitle_index: if burn { q.number("SubtitleStreamIndex").filter(|i| *i >= 0) } else { None },
                max_bitrate,
                max_height,
                start_ms: q.number("StartTimeTicks").map(|t| (t / TICKS_PER_MS).max(0)),
                caps: stream_caps(&q),
                mode: if copy { PlaybackMode::Remux } else { PlaybackMode::Transcode },
                client_id: device,
                keep_previous: false,
                keyed_urls: false,
                controls: Controls::new(),
                device_name: client.device.clone(),
                device_kind: None,
                hdr_transcodes: false,
                fmp4_segments: false,
                download: false,
                copy_video_only: false,
                stream_only: true,
            };
            let Json(info) = crate::api::playback::start(
                State(state.clone()),
                user.clone(),
                HeaderMap::new(),
                CurrentToken(None),
                Json(req),
            )
            .await?;
            let mut known = streams.0.lock();
            if known.len() >= FORGET_STREAMS_AT {
                known.retain(|_, session| state.sessions.get(session).is_some());
            }
            known.insert(key, info.session_id.clone());
            info.session_id
        }
    };
    let playlist = if state.transcoder.info().is_some_and(|t| t.capabilities.hls_master_playlist) {
        session_master(&state, &user, &session_id).await?
    } else {
        // A transcoder older than multivariant playlists: one of the
        // stream's own, which can't name its video's codec.
        let capped = max_bitrate.map(|kbps| i64::from(kbps) * 1000);
        let bandwidth = match (file.bitrate, capped) {
            (Some(file), Some(cap)) => file.min(cap),
            (file, cap) => file.or(cap).unwrap_or(8_000_000),
        }
        .max(64_000);
        let codecs = if row.kind == "track" { ",CODECS=\"mp4a.40.2\"" } else { "" };
        format!(
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-STREAM-INF:BANDWIDTH={bandwidth}{codecs}\nhls1/{session_id}/index.m3u8\n"
        )
    };
    Ok(([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl"), (header::CACHE_CONTROL, "no-cache")], playlist)
        .into_response())
}

/// The session's multivariant playlist, from the transcoder, its playlists'
/// addresses taken from beside it to beside `master.m3u8` (`hls1/{session}/…`).
async fn session_master(state: &AppState, user: &AuthUser, session_id: &str) -> JfResult<String> {
    let answer = crate::api::playback::hls_file(
        State(state.clone()),
        user.clone(),
        Path((session_id.to_owned(), "master.m3u8".to_owned())),
    )
    .await?;
    let bytes = axum::body::to_bytes(answer.into_body(), MAX_MASTER_PLAYLIST)
        .await
        .map_err(|e| JfError::Internal(anyhow::anyhow!("reading the stream's playlist: {e}")))?;
    Ok(beside_master(&String::from_utf8_lossy(&bytes), session_id))
}

/// A session's multivariant playlist as `master.m3u8` answers it: each
/// playlist it names (a line that isn't a tag) under `hls1/{session}/`.
fn beside_master(playlist: &str, session_id: &str) -> String {
    let mut out = String::with_capacity(playlist.len() + 64);
    for line in playlist.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            out.push_str(line);
        } else {
            out.push_str(&format!("hls1/{session_id}/{line}"));
        }
        out.push('\n');
    }
    out
}

/// A session's playlists and segments, which players fetch without a token:
/// the session's id (128 random bits) stands for the token that started it
/// through this API. Only those sessions, and only while that token works:
/// not once it is revoked or ends, its account is disabled, or the server
/// requires two-step sign-in the account lacks, nor from where the token
/// may not be used.
async fn session_file(
    State(state): State<AppState>,
    SourceIp(from): SourceIp,
    Path((_id, session_id, file)): Path<(String, String, String)>,
) -> JfResult<Response> {
    let session = state.sessions.get(&session_id).ok_or(JfError::NotFound)?;
    let token = session.compatibility_token.ok_or(JfError::NotFound)?;
    let user = state
        .auth
        .compatibility_user(token, from)
        .await
        .filter(|user| user.id == session.user_id)
        .map(super::this_apis)
        .ok_or(JfError::NotFound)?;
    Ok(crate::api::playback::hls_file(State(state), user, Path((session_id, file))).await?)
}

/// `DELETE /Videos/ActiveEncodings?DeviceId=&PlaySessionId=`: the device's
/// streams end.
async fn stop_encodings(State(state): State<AppState>, JfUser { user, client }: JfUser, q: Q) -> StatusCode {
    let device = q.get("deviceId").map(str::to_owned).or(client.device_id);
    if let Some(device) = device {
        end_sessions(&state, state.sessions.take_client_sessions(user.id, &device));
    }
    StatusCode::NO_CONTENT
}

fn end_sessions(state: &AppState, sessions: Vec<Arc<Session>>) {
    if sessions.is_empty() {
        return;
    }
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

// ------------------------------------------------------------------ seek previews

/// `/Videos/{id}/Trickplay/{width}/tiles.m3u8` and `…/{n}.jpg`: a file's
/// seek-bar previews, Pley's own sprite sheets ([`crate::trickplay`]), at
/// the one width Pley makes them. They are frames of the video, so they open
/// as its stream does: with a token, or a media source's id that stands for
/// one (`MediaSourceId`), for what its user may see. `404` while they are
/// being made, and asking queues them.
async fn trickplay(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, width, name)): Path<(String, String, String)>,
    q: Q,
    request: Request,
) -> JfResult<Response> {
    let (user, file) = media_caller(&state, &jellyfin, asking(&request), &id, q.get("mediaSourceId")).await?;
    let Some(info) = state.trickplay.index(&file) else {
        state.trickplay.request(&file);
        return Err(JfError::NotFound);
    };
    if width.parse::<u32>().ok() != Some(info.width) {
        return Err(JfError::NotFound);
    }
    if name == "tiles.m3u8" {
        let source = jellyfin.ids.source(file.id, user.compatibility.map(|c| c.token_id).unwrap_or(0));
        let playlist = tiles_playlist(&info, &source);
        return Ok(([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")], playlist).into_response());
    }
    let n: u32 = name.strip_suffix(".jpg").and_then(|n| n.parse().ok()).ok_or(JfError::NotFound)?;
    let path = state.trickplay.sheet(&file, n).ok_or(JfError::NotFound)?;
    let bytes = tokio::fs::read(path).await.map_err(|e| JfError::Internal(e.into()))?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=86400")], bytes)
        .into_response())
}

/// Seconds of `ms`, as HLS writes durations: `10`, `2.5`.
fn seconds(ms: u64) -> String {
    let text = format!("{}.{:03}", ms / 1000, ms % 1000);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The previews as an HLS playlist of images (Apple's `EXT-X-TILES`), as
/// Jellyfin writes it: a sheet a segment, the last as long as its tiles.
/// The sheets are named with the media source's id rather than a token.
fn tiles_playlist(info: &TrickplayInfo, source: &str) -> String {
    let per_sheet = (info.columns * info.rows).max(1);
    let interval = info.interval_ms.max(1) as u64;
    let sheets = info.count.div_ceil(per_sheet).min(info.sheets);
    let mut out = format!(
        "#EXTM3U\n#EXT-X-TARGETDURATION:{}\n#EXT-X-VERSION:7\n#EXT-X-MEDIA-SEQUENCE:1\n\
         #EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-IMAGES-ONLY\n",
        (interval * u64::from(per_sheet)).div_ceil(1000)
    );
    for n in 0..sheets {
        let tiles = (info.count - n * per_sheet).min(per_sheet);
        out.push_str(&format!(
            "#EXTINF:{},\n#EXT-X-TILES:RESOLUTION={}x{},LAYOUT={}x{},DURATION={}\n{n}.jpg?MediaSourceId={source}\n",
            seconds(interval * u64::from(tiles)),
            info.width,
            info.height,
            info.columns,
            info.rows,
            seconds(interval),
        ));
    }
    out.push_str("#EXT-X-ENDLIST\n");
    out
}

// ------------------------------------------------------------------ subtitles

/// The most of a subtitle file beside the video that is served as it is.
const MAX_SUBTITLE_FILE: u64 = 16 * 1024 * 1024;

/// SubRip as WebVTT: the header in, dots in its times (cue numbers stay, as
/// cue names).
fn srt_to_vtt(srt: &str) -> String {
    let mut out = String::with_capacity(srt.len() + 8);
    out.push_str("WEBVTT\n\n");
    for line in srt.trim_start_matches('\u{feff}').replace("\r\n", "\n").lines() {
        if line.contains("-->") {
            out.push_str(&line.replace(',', "."));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// A text track's own file, beside the video or added, read as it is when
/// it is SubRip or WebVTT in UTF-8: Pley's transcoder converts the rest.
async fn subtitle_file(state: &AppState, file: &FileRow, index: i64) -> Option<(String, String)> {
    let (path, _) = crate::api::playback::subtitle_source(state, file, index).await.ok()?;
    let path = std::path::PathBuf::from(path?);
    let kind = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(kind.as_str(), "srt" | "vtt") {
        return None;
    }
    // Read from the file checked: not a link swapped in since the scan, to
    // Pley's database or a device.
    let bytes = tokio::task::spawn_blocking(move || {
        let (file, meta) = crate::library::walk::open_of_its_kind(&path)?;
        if meta.len() > MAX_SUBTITLE_FILE {
            return None;
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_SUBTITLE_FILE), &mut bytes).ok()?;
        Some(bytes)
    })
    .await
    .ok()??;
    let text = String::from_utf8(bytes).ok()?;
    Some((kind, text.trim_start_matches('\u{feff}').to_owned()))
}

/// WebVTT as SubRip: the header out, a number before each cue, commas in
/// its times.
fn vtt_to_srt(vtt: &str) -> String {
    let mut out = String::with_capacity(vtt.len());
    let mut n = 0;
    for block in vtt.replace("\r\n", "\n").split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else { continue };
        n += 1;
        let timing = lines[at].split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        let fix = |t: &str| -> String {
            let t = t.replace('.', ",");
            if t.matches(':').count() == 1 { format!("00:{t}") } else { t }
        };
        let timing = timing.split(" --> ").map(fix).collect::<Vec<_>>().join(" --> ");
        out.push_str(&format!("{n}\n{timing}\n"));
        for line in &lines[at + 1..] {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

async fn subtitle(
    state: AppState,
    jellyfin: &Jellyfin,
    id: &str,
    source: &str,
    index: &str,
    name: &str,
    request: Request,
) -> JfResult<Response> {
    let (user, file) = media_caller(&state, jellyfin, asking(&request), id, Some(source)).await?;
    let index: i64 = index.parse().map_err(|_| JfError::NotFound)?;
    if !file.streams().iter().any(|s| s.index == index && s.kind == StreamKind::Subtitle) {
        return Err(JfError::NotFound);
    }
    let format = name.rsplit('.').next().unwrap_or("vtt").to_ascii_lowercase();
    if let Some((kind, text)) = subtitle_file(&state, &file, index).await {
        let text = match (kind.as_str(), format.as_str()) {
            ("srt", "vtt") => srt_to_vtt(&text),
            ("vtt", "srt") => vtt_to_srt(&text),
            _ => text,
        };
        let content_type =
            if format == "srt" { "application/x-subrip; charset=utf-8" } else { "text/vtt; charset=utf-8" };
        return Ok(([(header::CONTENT_TYPE, content_type)], text).into_response());
    }
    let response = crate::api::playback::subtitles(
        State(state),
        user,
        Path((file.id, format!("{index}.vtt"))),
        axum::extract::Query(Default::default()),
        request,
    )
    .await?;
    if format != "srt" {
        return Ok(response);
    }
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024 * 1024)
        .await
        .map_err(|e| JfError::Internal(anyhow::anyhow!("{e}")))?;
    let srt = vtt_to_srt(&String::from_utf8_lossy(&bytes));
    Ok(([(header::CONTENT_TYPE, "application/x-subrip; charset=utf-8")], srt).into_response())
}

/// `/Videos/{id}/{source}/Subtitles/{index}/Stream.{vtt|srt}`: a text track
/// (embedded, beside the file, or added), as WebVTT or SubRip.
async fn subtitles(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, source, index, name)): Path<(String, String, String, String)>,
    request: Request,
) -> JfResult<Response> {
    subtitle(state, &jellyfin, &id, &source, &index, &name, request).await
}

async fn subtitles_at(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path((id, source, index, _ticks, name)): Path<(String, String, String, String, String)>,
    request: Request,
) -> JfResult<Response> {
    subtitle(state, &jellyfin, &id, &source, &index, &name, request).await
}

// ------------------------------------------------------------------ reports

/// A player's report: where it is, paused or not, stopped. `starting` for
/// the first, `/Sessions/Playing`.
async fn report(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    client: &Client,
    body: &Body,
    play_state: &str,
    starting: bool,
) -> JfResult<()> {
    let Some(id) = body.str("ItemId") else { return Ok(()) };
    let row = visible_item(state, jellyfin, user, id).await?;
    if !dto::is_playable(&row.kind) {
        return Ok(());
    }
    let position = body.number("PositionTicks").map(|ticks| ticks.max(0) / TICKS_PER_MS);
    let position_ms = position.unwrap_or(0);
    let play_state = if play_state == "playing" && body.flag("IsPaused") == Some(true) { "paused" } else { play_state };
    let duration = row.duration_ms.unwrap_or(0);
    let device = client.device.as_deref().or(client.client.as_deref());
    // A report without a place, or a start's at 0 (before the player has
    // gone to where it resumes), leaves the resume point as it was.
    let recorded = match position.filter(|ms| *ms > 0 || !starting) {
        Some(ms) => {
            catalog::record_progress(&state.db, &state.events, user.id, row.id, ms, duration, device, play_state)
                .await?
        }
        None => catalog::Recorded::default(),
    };
    if recorded.left_watchlist {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }

    let device_id = client.device_id.clone();
    let existing = state
        .sessions
        .list()
        .into_iter()
        .find(|s| s.user_id == user.id && s.item_id == row.id && (device_id.is_none() || s.client_id == device_id));
    // A session stopped from elsewhere isn't started again by the reports
    // its player keeps sending until it hears of it.
    let stopped = existing.is_none() && state.sessions.was_stopped(user.id, device_id.as_deref(), row.id);
    // The tracks it plays, for the user's remotes (`-1`: no subtitles).
    let (audio, subtitles) = (body.number("AudioStreamIndex"), body.number("SubtitleStreamIndex"));
    match existing {
        Some(session) => {
            let moved = session.update(position_ms, play_state);
            let switched = session.report_tracks(audio, subtitles);
            if moved || switched {
                state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
            }
            if play_state == "stopped" {
                end_sessions(state, state.sessions.remove(&session.id).into_iter().collect());
            }
        }
        None if play_state != "stopped" && !stopped => {
            if let Some(file) = catalog::files_for_item(&state.db, row.id).await?.into_iter().next() {
                let mut session = Session::new(
                    user.id,
                    user.username.clone(),
                    device_id.clone(),
                    row.id,
                    file.id,
                    Method::Direct,
                    duration,
                    None,
                    None,
                    position_ms,
                );
                session.device = device.and_then(clean_name);
                session.on_device(
                    crate::api::remote::device_id_of(user, None),
                    crate::api::remote::device_kind_of(None, user, None),
                );
                let session = state.sessions.insert(session);
                session.update(position_ms, play_state);
                session.report_tracks(audio, subtitles);
                state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
            }
        }
        None if stopped && play_state == "stopped" => {
            state.sessions.forget_stopped(user.id, device_id.as_deref(), row.id)
        }
        None => {}
    }
    if recorded.counted {
        state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    }
    Ok(())
}

async fn playing(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    report(&state, &jellyfin, &user, &client, &body, "playing", true).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn progress(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    report(&state, &jellyfin, &user, &client, &body, "playing", false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stopped(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, client }: JfUser,
    body: Body,
) -> JfResult<StatusCode> {
    report(&state, &jellyfin, &user, &client, &body, "stopped", false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ping(_user: JfUser) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `/Sessions`: the caller's own devices' sessions are Pley's to show; none
/// are listed here.
async fn sessions(_user: JfUser) -> Json<[(); 0]> {
    Json([])
}

// ------------------------------------------------------------------ bitrate test

/// The most `/Playback/BitrateTest` sends, as Jellyfin allows.
const MAX_BITRATE_TEST: i64 = 100_000_000;

/// `/Playback/BitrateTest?size=`: that many bytes, for an app to time its
/// connection by and pick a quality. Random, so that nothing on the way
/// compresses them, and sent a piece at a time.
async fn bitrate_test(_user: JfUser, q: Q) -> Response {
    const PIECE: usize = 64 * 1024;
    let size = q.number("size").unwrap_or(102_400).clamp(1, MAX_BITRATE_TEST) as usize;
    let pieces = futures::stream::unfold(size, |left| async move {
        if left == 0 {
            return None;
        }
        let mut piece = vec![0u8; left.min(PIECE)];
        getrandom::fill(&mut piece).expect("the system's random numbers");
        let next = left - piece.len();
        Some((Ok::<_, std::convert::Infallible>(bytes::Bytes::from(piece)), next))
    });
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (header::CONTENT_LENGTH, size.to_string()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        axum::body::Body::from_stream(pieces),
    )
        .into_response()
}

// ------------------------------------------------------------------ media segments

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Segment {
    id: String,
    item_id: String,
    #[serde(rename = "Type")]
    kind: &'static str,
    start_ticks: i64,
    end_ticks: i64,
}

/// `/MediaSegments/{id}`: the intro and credits Pley found in the item's
/// first version, for Skip Intro and Skip Credits.
async fn media_segments(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    let row = visible_item(&state, &jellyfin, &user, &id).await?;
    let mut items = Vec::new();
    if let Some(file) = catalog::files_for_item(&state.db, row.id).await?.into_iter().next() {
        let item_id = jellyfin.ids.of(Kind::Item, row.id);
        let markers = crate::markers::for_item(&state.db, row.id).await?;
        for (n, marker) in markers.into_iter().filter(|m| m.file_id == file.id).enumerate() {
            items.push(Segment {
                id: jellyfin.ids.of(Kind::File, file.id * 16 + n as i64),
                item_id: item_id.clone(),
                kind: if marker.kind == "credits" { "Outro" } else { "Intro" },
                start_ticks: marker.start_ms * TICKS_PER_MS,
                end_ticks: marker.end_ms * TICKS_PER_MS,
            });
        }
    }
    let total = items.len();
    Ok(Json(QueryResult { items, total_record_count: total, start_index: 0 }).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sessions_playlist_is_named_beside_master() {
        let session = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-STREAM-INF:BANDWIDTH=1,CODECS=\"hvc1.2.4.L153.B0,ec-3\"\nindex.m3u8\n\
                       #EXT-X-STREAM-INF:BANDWIDTH=2\nv1.m3u8\n";
        assert_eq!(
            beside_master(session, "abc"),
            "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-STREAM-INF:BANDWIDTH=1,CODECS=\"hvc1.2.4.L153.B0,ec-3\"\n\
             hls1/abc/index.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=2\nhls1/abc/v1.m3u8\n"
        );
    }

    #[test]
    fn previews_are_a_playlist_of_tiles() {
        let info = TrickplayInfo {
            id: "t".into(),
            interval_ms: 2500,
            width: 320,
            height: 180,
            columns: 10,
            rows: 10,
            count: 150,
            sheets: 2,
            version: 1,
        };
        let playlist = tiles_playlist(&info, "abc");
        assert!(playlist.starts_with("#EXTM3U\n#EXT-X-TARGETDURATION:250\n"), "{playlist}");
        let tiles = "#EXT-X-TILES:RESOLUTION=320x180,LAYOUT=10x10,DURATION=2.5";
        assert!(playlist.contains(&format!("#EXTINF:250,\n{tiles}\n0.jpg?MediaSourceId=abc\n")), "{playlist}");
        assert!(playlist.contains(&format!("#EXTINF:125,\n{tiles}\n1.jpg?MediaSourceId=abc\n")), "the last: 50 tiles");
        assert!(playlist.ends_with("#EXT-X-ENDLIST\n") && !playlist.contains("2.jpg"));
        assert_eq!((seconds(10_000), seconds(1_250), seconds(60)), ("10".into(), "1.25".into(), "0.06".into()));
    }

    #[test]
    fn subrip_becomes_webvtt() {
        let srt = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,500\r\nHello, you\r\n\r\n2\r\n01:00:03,000 --> 01:00:04,000\r\nTwo\r\n";
        assert_eq!(
            srt_to_vtt(srt),
            "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.500\nHello, you\n\n2\n01:00:03.000 --> 01:00:04.000\nTwo\n"
        );
    }

    #[test]
    fn webvtt_becomes_subrip() {
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.500 align:start\nHello\n\n01:00:03.000 --> 01:00:04.000\nTwo\nlines\n";
        assert_eq!(
            vtt_to_srt(vtt),
            "1\n00:00:01,000 --> 00:00:02,500\nHello\n\n2\n01:00:03,000 --> 01:00:04,000\nTwo\nlines\n\n"
        );
    }
}
