//! Playback: direct file streaming, watch-state reports (`/:/timeline`,
//! `/:/scrobble`, …), `/status/sessions`, and the universal transcoder
//! endpoints mapped onto pley playback sessions.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::Json;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use pley_transcoder_api::{JobStatus, VideoMode};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::meta::{self, Detail, Sections};
use super::{ClientInfo, Node, Params, PlexError, PlexResult, PlexUser};
use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow};
use crate::events::ServerEvent;
use crate::library::list_libraries;
use crate::library::sidecar::EXTERNAL_INDEX_BASE;
use crate::playback::{Caps, Method, PlaybackMode, PlaybackRequest, Session};
use crate::state::AppState;

/// Plex client details of a server's sessions (by pley session id) and the
/// small integer `sessionKey`s Plex tools expect. Each server has its own:
/// one forgetting the sessions it no longer has must not forget another's
/// (tests run several servers in one process).
#[derive(Default)]
pub struct SessionKeys {
    tracked: Mutex<HashMap<String, Tracked>>,
    last_key: AtomicI64,
}

struct Tracked {
    key: i64,
    client: ClientInfo,
}

/// Sessions tracked before those that ended are forgotten here too: only
/// tools asking for sessions forget them otherwise, and a client making up
/// a new identifier for every report would add one each time.
const FORGET_ENDED_AT: usize = 512;

impl SessionKeys {
    fn entry<'a>(&self, tracked: &'a mut HashMap<String, Tracked>, session_id: &str) -> &'a mut Tracked {
        tracked.entry(session_id.to_owned()).or_insert_with(|| Tracked {
            key: self.last_key.fetch_add(1, Ordering::Relaxed) + 1,
            client: ClientInfo::default(),
        })
    }
}

fn remember(state: &AppState, session_id: &str, client: &ClientInfo) {
    let keys = &state.plex_session_keys;
    let mut tracked = keys.tracked.lock();
    if tracked.len() >= FORGET_ENDED_AT {
        let live: std::collections::HashSet<String> = state.sessions.list().iter().map(|s| s.id.clone()).collect();
        tracked.retain(|id, _| live.contains(id));
    }
    keys.entry(&mut tracked, session_id).client = client.clone();
}

pub(super) fn session_key(state: &AppState, session_id: &str) -> (i64, ClientInfo) {
    let keys = &state.plex_session_keys;
    let mut tracked = keys.tracked.lock();
    let entry = keys.entry(&mut tracked, session_id);
    (entry.key, entry.client.clone())
}

pub(super) fn forget_ended(state: &AppState, live: &[Arc<Session>]) {
    state.plex_session_keys.tracked.lock().retain(|id, _| live.iter().any(|s| &s.id == id));
}

// ------------------------------------------------------------------ files

/// `/library/parts/{id}/{version}/file.ext`: the media file, with ranges.
/// A media file a Plex user may see, by part (or media) id.
async fn visible_file(state: &AppState, user: &AuthUser, id: i64) -> PlexResult<crate::catalog::FileRow> {
    let file = catalog::file(&state.db, id).await?.ok_or(PlexError::NotFound)?;
    catalog::item(&state.db, user.id, file.item_id).await?.ok_or(PlexError::NotFound)?;
    Ok(file)
}

fn jpeg(bytes: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=86400")], bytes).into_response()
}

/// `/library/parts/{id}/indexes/sd`: the seek previews as a BIF file.
pub async fn part_index(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, _quality)): Path<(i64, String)>,
) -> PlexResult<Response> {
    let file = visible_file(&state, &user, id).await?;
    let bytes = state.trickplay.bif(&file).await.map_err(PlexError::Internal)?.ok_or(PlexError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response())
}

/// `/library/parts/{id}/indexes/sd/{offset}`: the preview frame at `offset` ms.
pub async fn part_index_frame(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, _quality, offset)): Path<(i64, String, i64)>,
) -> PlexResult<Response> {
    let file = visible_file(&state, &user, id).await?;
    let frame =
        state.trickplay.frame_at(&file, offset).await.map_err(PlexError::Internal)?.ok_or(PlexError::NotFound)?;
    Ok(jpeg(frame))
}

/// `/library/media/{id}/chapterImages/{n}`.
pub async fn chapter_image(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, n)): Path<(i64, usize)>,
) -> PlexResult<Response> {
    let file = visible_file(&state, &user, id).await?;
    let path =
        state.trickplay.chapter_image(&file, n).await.map_err(PlexError::Internal)?.ok_or(PlexError::NotFound)?;
    Ok(jpeg(tokio::fs::read(path).await.map_err(|e| PlexError::Internal(e.into()))?))
}

pub async fn part_file(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, _version, _name)): Path<(i64, String, String)>,
    params: Params,
    request: Request,
) -> PlexResult<Response> {
    let file = catalog::file(&state.db, id).await?.ok_or(PlexError::NotFound)?;
    catalog::item(&state.db, user.id, file.item_id).await?.ok_or(PlexError::NotFound)?;
    if !crate::library::walk::leads_to_its_kind(std::path::Path::new(&file.path)) {
        return Err(PlexError::NotFound);
    }
    let response = ServeFile::new(&file.path)
        .oneshot(request)
        .await
        .map_err(|e| PlexError::Internal(anyhow::anyhow!("{e}")))?
        .map(axum::body::Body::new);
    let mut response = response;
    if params.flag("download") {
        let name = std::path::Path::new(&file.path)
            .file_name()
            .map(|n| n.to_string_lossy().replace('"', ""))
            .unwrap_or_else(|| "file".into());
        if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
            response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
        }
    }
    Ok(response)
}

/// Largest sidecar subtitle file served as it is (a film's subtitles are
/// well under 1 MB; picture subtitles are far bigger, and players don't
/// fetch those here).
const MAX_SIDECAR_BYTES: u64 = 64 * 1024 * 1024;

/// `/library/streams/{id}`: a subtitle stream (sidecar files and added
/// subtitles as they are, embedded text subtitles converted to WebVTT).
pub async fn stream_file(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
) -> PlexResult<Response> {
    let file = catalog::file(&state.db, id / 10_000).await?.ok_or(PlexError::NotFound)?;
    let index = meta::stream_index(file.id, id).ok_or(PlexError::NotFound)?;
    catalog::item(&state.db, user.id, file.item_id).await?.ok_or(PlexError::NotFound)?;
    if index >= crate::subtitles::ADDED_INDEX_BASE {
        let sub = file.added_subs().into_iter().find(|s| s.index == index).ok_or(PlexError::NotFound)?;
        let path = sub.path(&state.config.added_subtitles_dir(), file.id);
        let bytes = tokio::fs::read(&path).await.map_err(|_| PlexError::NotFound)?;
        return Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], bytes).into_response());
    }
    if index >= EXTERNAL_INDEX_BASE {
        // Only a subtitle file (not a link swapped in since the scan, to
        // pley's database or /dev/zero), and only one of subtitle size.
        let sub = file
            .external_subs()
            .into_iter()
            .find(|s| s.index == index)
            .filter(|s| crate::library::walk::leads_to_its_kind(std::path::Path::new(&s.path)))
            .ok_or(PlexError::NotFound)?;
        let meta = tokio::fs::metadata(&sub.path).await.map_err(|_| PlexError::NotFound)?;
        if !meta.is_file() || meta.len() > MAX_SIDECAR_BYTES {
            return Err(PlexError::NotFound);
        }
        let bytes = tokio::fs::read(&sub.path).await.map_err(|_| PlexError::NotFound)?;
        return Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], bytes).into_response());
    }
    let text = state
        .transcoder
        .subtitles(&file.path, Some(index))
        .await
        .map_err(|e| PlexError::from(crate::error::AppError::from(e)))?;
    Ok(([(header::CONTENT_TYPE, "text/vtt; charset=utf-8")], text).into_response())
}

// ------------------------------------------------------------------ watch state

fn rating_key(params: &Params) -> PlexResult<i64> {
    params
        .parse::<i64>("ratingKey")
        .or_else(|| {
            params.get("key").and_then(|k| k.trim_start_matches("/library/metadata/").split('/').next()?.parse().ok())
        })
        .ok_or_else(|| PlexError::BadRequest("ratingKey or key is required".into()))
}

async fn playable(state: &AppState, user: &AuthUser, id: i64) -> PlexResult<ItemRow> {
    let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    if !meta::is_leaf(&row.kind) {
        return Err(PlexError::BadRequest("progress can only be recorded for playable items".into()));
    }
    Ok(row)
}

/// Records progress like pley's own player does, and keeps a pley session
/// for the Plex client so it shows up as "now playing" everywhere.
async fn report(
    state: &AppState,
    user: &AuthUser,
    row: &ItemRow,
    time_ms: i64,
    duration_ms: Option<i64>,
    play_state: &str,
    client: &ClientInfo,
) -> PlexResult<()> {
    let duration = duration_ms.filter(|d| *d > 0).or(row.duration_ms).unwrap_or(0);
    let device = client.device_name.as_deref().or(client.product.as_deref());
    let recorded =
        catalog::record_progress(&state.db, &state.events, user.id, row.id, time_ms, duration, device, play_state)
            .await?;
    if recorded.left_watchlist {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }

    let existing = state.sessions.list().into_iter().find(|s| {
        s.user_id == user.id && s.item_id == row.id && (client.identifier.is_none() || s.client_id == client.identifier)
    });
    // A session stopped from elsewhere isn't started again by the reports
    // its player keeps sending until it hears of it.
    let stopped = existing.is_none() && state.sessions.was_stopped(user.id, client.identifier.as_deref(), row.id);
    match existing {
        Some(session) => {
            let changed = session.update(time_ms, play_state);
            if session.should_touch_transcoder()
                && let Some(job) = session.job_id()
            {
                let transcoder = state.transcoder.clone();
                tokio::spawn(async move {
                    let _ = transcoder.touch_job(&job).await;
                });
            }
            if changed {
                state.events.send(ServerEvent::SessionsChanged);
            }
        }
        None if play_state != "stopped" && !stopped => {
            if let Some(client_id) = &client.identifier {
                end_sessions(state, state.sessions.take_client_sessions(user.id, client_id));
            }
            if let Some(file) = catalog::files_for_item(&state.db, row.id).await?.into_iter().next() {
                let session = state.sessions.insert(Session::new(
                    user.id,
                    user.username.clone(),
                    client.identifier.clone(),
                    row.id,
                    file.id,
                    Method::Direct,
                    duration,
                    None,
                    None,
                    time_ms,
                ));
                session.update(time_ms, play_state);
                remember(state, &session.id, client);
                state.events.send(ServerEvent::SessionsChanged);
            }
        }
        // Its player has stopped as well: a play after this shows up again.
        None if stopped && play_state == "stopped" => {
            state.sessions.forget_stopped(user.id, client.identifier.as_deref(), row.id)
        }
        None => {}
    }
    if recorded.counted {
        state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    }
    Ok(())
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
    state.events.send(ServerEvent::SessionsChanged);
}

fn play_state(params: &Params) -> &'static str {
    match params.get("state").unwrap_or("playing") {
        "paused" => "paused",
        "stopped" => "stopped",
        "buffering" => "buffering",
        _ => "playing",
    }
}

fn millis(params: &Params, key: &str) -> Option<i64> {
    params.parse::<f64>(key).filter(|v| v.is_finite()).map(|v| v.max(0.0) as i64)
}

/// `/:/timeline?ratingKey=1&state=playing&time=12000&duration=…`
pub async fn timeline(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
    client: ClientInfo,
) -> PlexResult<Node> {
    let row = playable(&state, &user, rating_key(&params)?).await?;
    let time = millis(&params, "time").unwrap_or(0);
    report(&state, &user, &row, time, millis(&params, "duration"), play_state(&params), &client).await?;
    Ok(Node::container().attr("size", 0))
}

/// `/:/progress?key=1&time=12000&state=stopped`
pub async fn progress(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
    client: ClientInfo,
) -> PlexResult<Node> {
    let row = playable(&state, &user, rating_key(&params)?).await?;
    let time = millis(&params, "time").unwrap_or(0);
    report(&state, &user, &row, time, None, play_state(&params), &client).await?;
    Ok(Node::container().attr("size", 0))
}

/// `/:/rate?key=…&rating=0-10` (a negative rating clears it).
pub async fn rate(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let id = rating_key(&params)?;
    let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let value = params.parse::<f64>("rating").ok_or_else(|| PlexError::BadRequest("rating is required".into()))?;
    let rating = if value < 0.0 {
        None
    } else {
        Some(catalog::valid_rating(value).ok_or_else(|| PlexError::BadRequest("rating must be 0-10".into()))?)
    };
    catalog::set_rating(&state.db, user.id, row.id, rating).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Node::container())
}

async fn set_watched(state: &AppState, user: &AuthUser, params: &Params, watched: bool) -> PlexResult<Node> {
    let id = rating_key(params)?;
    let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    if catalog::set_watched(&state.db, &state.events, user.id, id, &row.kind, watched).await? {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(Node::container().attr("size", 0))
}

/// `/:/scrobble?key=1`: mark played (shows, seasons, … mark every episode).
pub async fn scrobble(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    set_watched(&state, &user, &params, true).await
}

pub async fn unscrobble(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    set_watched(&state, &user, &params, false).await
}

/// `PUT /actions/removeFromContinueWatching?ratingKey=…`: takes the movie,
/// or the episode's show, out of Continue Watching until it is played again
/// (as `DELETE /api/continue-watching/{id}`).
pub async fn remove_from_continue_watching(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
) -> PlexResult<Node> {
    let id = rating_key(&params)?;
    let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let title = catalog::continue_watching_title(&row)
        .ok_or_else(|| PlexError::BadRequest("only movies and shows are in Continue Watching".into()))?;
    catalog::hide_from_continue_watching(&state.db, &state.events, user.id, title).await?;
    Ok(Node::container().attr("size", 0))
}

// ------------------------------------------------------------------ sessions

fn decision(method: Method) -> &'static str {
    match method {
        Method::Direct => "directplay",
        Method::Remux | Method::Transcode => "transcode",
    }
}

fn transcode_node(session: &Session, job: &JobStatus) -> Node {
    let video = job.video.as_ref();
    let audio = job.audio.as_ref();
    let done_ms = job
        .process
        .as_ref()
        .map(|p| p.out_time_ms)
        .unwrap_or_else(|| (job.segments_ready as f64 * job.segment_seconds * 1000.0) as i64);
    let progress =
        if job.duration_ms > 0 { (done_ms as f64 / job.duration_ms as f64 * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    let video_decision = video.map(|v| if v.mode == VideoMode::Copy { "copy" } else { "transcode" });
    let audio_decision =
        audio.map(|a| if a.mode == pley_transcoder_api::AudioMode::Copy { "copy" } else { "transcode" });
    let hw_encoder = video.and_then(|v| v.encoder.clone()).filter(|e| !e.starts_with("lib"));
    let hw_decoder = video.and_then(|v| v.decoder.clone()).filter(|d| d != "cpu" && d != "copy");
    Node::new("TranscodeSession")
        .single()
        .attr("key", format!("/transcode/sessions/{}", session.id))
        .attr("throttled", false)
        .attr("complete", job.state == pley_transcoder_api::JobState::Finished)
        .attr("progress", (progress * 10.0).round() / 10.0)
        .opt("speed", job.process.as_ref().and_then(|p| p.speed))
        .attr("duration", job.duration_ms)
        .attr("context", "streaming")
        .opt("sourceVideoCodec", video.map(|v| meta::codec_name(&v.codec_in)))
        .opt("sourceAudioCodec", audio.map(|a| meta::codec_name(&a.codec_in)))
        .opt("videoDecision", video_decision)
        .opt("audioDecision", audio_decision)
        .attr("protocol", "hls")
        .attr("container", "mpegts")
        .opt("videoCodec", video.map(|v| meta::codec_name(&v.codec_out)))
        .opt("audioCodec", audio.map(|a| meta::codec_name(&a.codec_out)))
        .opt("audioChannels", audio.and_then(|a| a.channels_out))
        .opt("width", video.and_then(|v| v.width_out))
        .opt("height", video.and_then(|v| v.height_out))
        .attr("transcodeHwRequested", hw_encoder.is_some() || hw_decoder.is_some())
        .opt("transcodeHwDecoding", hw_decoder)
        .opt("transcodeHwEncoding", hw_encoder)
        .attr("transcodeHwFullPipeline", video.and_then(|v| v.pipeline.as_deref()) == Some("gpu"))
        .attr("minOffsetAvailable", 0.0)
        .attr("maxOffsetAvailable", (done_ms as f64 / 1000.0 * 10.0).round() / 10.0)
}

/// `/status/sessions`: what is playing now (admins see every user).
pub async fn sessions(State(state): State<AppState>, PlexUser(user): PlexUser) -> PlexResult<Node> {
    let all = state.sessions.list();
    forget_ended(&state, &all);
    let now = crate::util::now();
    let sessions: Vec<Arc<Session>> =
        all.into_iter().filter(|s| (user.is_admin || s.user_id == user.id) && s.is_active(now)).collect();
    let jobs: HashMap<String, JobStatus> = if sessions.iter().any(|s| s.job_id().is_some()) {
        match state.transcoder.status().await {
            Ok(status) => status.jobs.into_iter().map(|j| (j.id.clone(), j)).collect(),
            Err(_) => HashMap::new(),
        }
    } else {
        HashMap::new()
    };
    let libraries = list_libraries(&state.db).await?;
    let sections = Sections::new(&libraries);
    let mut mc = Node::container();
    let mut size = 0;
    for session in sessions {
        let Some(row) = catalog::item(&state.db, session.user_id, session.item_id).await? else { continue };
        let file = catalog::file(&state.db, session.file_id).await?;
        let extras = meta::load_extras(&state.db, &[row.id]).await?;
        let files: Vec<_> = file.into_iter().collect();
        let mut node = meta::item_node(
            &row,
            extras.get(&row.id),
            &files,
            &sections,
            Detail { guids: true, full: true, media: true, markers: false },
        );
        let (key, client) = session_key(&state, &session.id);
        let play = session.play.lock().clone();
        node.set("sessionKey", key.to_string());
        node.set("viewOffset", play.position_ms);
        let job = session.job_id().and_then(|id| jobs.get(&id));
        let bitrate = files.first().and_then(|f| f.bitrate).map(|b| b / 1000);
        let bandwidth = job.and_then(|j| j.video.as_ref()).and_then(|v| v.bitrate_kbps).map(i64::from).or(bitrate);
        node.push(
            Node::new("User")
                .single()
                .attr("id", session.user_id.to_string())
                .attr("title", &session.username)
                .attr("thumb", ""),
        );
        node.push(
            Node::new("Player")
                .single()
                .attr("address", client.address.clone().unwrap_or_default())
                .attr("device", client.device.clone().unwrap_or_else(|| "Web".into()))
                .attr("machineIdentifier", session.client_id.clone().unwrap_or_else(|| session.id.clone()))
                .attr("model", "")
                .attr("platform", client.platform.clone().unwrap_or_else(|| "Web".into()))
                .attr("platformVersion", client.platform_version.clone().unwrap_or_default())
                .attr("product", client.product.clone().unwrap_or_else(|| "Pley Web".into()))
                .attr("profile", "")
                .attr("state", play.state.clone())
                .attr("title", client.device_name.clone().unwrap_or_else(|| "Pley".into()))
                .attr("vendor", "")
                .attr("version", client.version.clone().unwrap_or_else(|| env!("CARGO_PKG_VERSION").into()))
                .attr("local", true)
                .attr("relayed", false)
                .attr("secure", false)
                .attr("userID", session.user_id),
        );
        node.push(
            Node::new("Session")
                .single()
                .attr("id", session.id.clone())
                .opt("bandwidth", bandwidth)
                .attr("location", "lan"),
        );
        if let Some(job) = job {
            node.push(transcode_node(&session, job));
        }
        mark_decision(&mut node, session.method);
        mc.push(node);
        size += 1;
    }
    mc.set("size", size);
    Ok(mc)
}

/// `/status/sessions/terminate?sessionId=…&reason=…` (Tautulli's "kill
/// stream"): an administrator stops a session (its `Session` `id`), as
/// `DELETE /api/sessions/{id}` does, with the reason as its message.
pub async fn terminate(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    if !user.is_admin {
        return Err(PlexError::Forbidden);
    }
    let id = params
        .get("sessionId")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| PlexError::BadRequest("sessionId is required".into()))?;
    let reason = params.get("reason").map(str::trim).filter(|r| !r.is_empty()).map(|r| r.chars().take(200).collect());
    crate::api::playback::stop_by(&state, &user, id, reason)?;
    Ok(Node::container().attr("size", 0))
}

/// Adds `decision` to the session's media part (Tautulli reads it).
fn mark_decision(node: &mut Node, method: Method) {
    for media in node.children_mut().iter_mut().filter(|c| c.tag == "Media") {
        media.set("selected", true);
        for part in media.children_mut().iter_mut().filter(|c| c.tag == "Part") {
            part.set("decision", decision(method));
            part.set("selected", true);
        }
    }
}

/// `/transcode/sessions`
pub async fn transcode_sessions(State(state): State<AppState>, PlexUser(user): PlexUser) -> PlexResult<Node> {
    let sessions: Vec<Arc<Session>> =
        state.sessions.active().into_iter().filter(|s| user.is_admin || s.user_id == user.id).collect();
    let jobs: HashMap<String, JobStatus> = match state.transcoder.status().await {
        Ok(status) => status.jobs.into_iter().map(|j| (j.id.clone(), j)).collect(),
        Err(_) => HashMap::new(),
    };
    let mut mc = Node::container();
    let mut size = 0;
    for session in sessions {
        if let Some(job) = session.job_id().and_then(|id| jobs.get(&id)) {
            mc.push(transcode_node(&session, job));
            size += 1;
        }
    }
    mc.set("size", size);
    Ok(mc)
}

// ------------------------------------------------------------------ universal transcoder

/// What an HLS client can take without conversion.
fn hls_caps() -> Caps {
    Caps {
        containers: Vec::new(),
        video_codecs: vec!["h264".into()],
        audio_codecs: vec!["aac".into(), "mp3".into()],
        hdr: false,
        dolby_vision: false,
        ..Default::default()
    }
}

/// `path=/library/metadata/1` → 1
fn path_item(params: &Params) -> PlexResult<i64> {
    let path = params.get("path").ok_or_else(|| PlexError::BadRequest("path is required".into()))?;
    let start = path.find("/library/metadata/").ok_or(PlexError::NotFound)?;
    path[start + "/library/metadata/".len()..]
        .split(['/', '?'])
        .next()
        .and_then(|id| id.parse().ok())
        .ok_or(PlexError::NotFound)
}

/// `start.m3u8`: starts a pley HLS session and answers with a master
/// playlist pointing at it.
pub async fn universal_start(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
    client: ClientInfo,
) -> PlexResult<Response> {
    let item_id = path_item(&params)?;
    let row = playable(&state, &user, item_id).await?;
    let files = catalog::files_for_item(&state.db, row.id).await?;
    let media_index = params.parse::<usize>("mediaIndex").unwrap_or(0);
    let file = files.get(media_index).or(files.first()).ok_or(PlexError::NotFound)?;
    let client_id = client.identifier.clone().or_else(|| params.get_ci("session").map(str::to_owned));
    let stream = |key: &str| params.parse::<i64>(key).and_then(|id| meta::stream_index(file.id, id));
    let burn = params.get("subtitles") == Some("burn");
    let request = PlaybackRequest {
        item_id: row.id,
        file_id: Some(file.id),
        audio_index: stream("audioStreamID"),
        subtitle_index: if burn { stream("subtitleStreamID") } else { None },
        max_bitrate: params.parse::<u32>("maxVideoBitrate").filter(|b| *b > 0),
        max_height: params
            .get("videoResolution")
            .and_then(|r| r.split_once('x'))
            .and_then(|(_, h)| h.parse::<u32>().ok())
            .filter(|h| *h > 0),
        start_ms: params.parse::<f64>("offset").map(|s| (s.max(0.0) * 1000.0) as i64),
        caps: hls_caps(),
        mode: if params.get("directStream") == Some("0") { PlaybackMode::Transcode } else { PlaybackMode::Remux },
        client_id,
        keep_previous: false,
        keyed_urls: false,
        hdr_transcodes: false,
        download: false,
    };
    let Json(info) =
        crate::api::playback::start(State(state.clone()), user, Json(request)).await.map_err(PlexError::from)?;
    remember(&state, &info.session_id, &client);

    let audio_only = row.kind == "track";
    let height = params
        .get("videoResolution")
        .and_then(|r| r.split_once('x'))
        .and_then(|(w, h)| Some((w.parse::<i64>().ok()?, h.parse::<i64>().ok()?)))
        .or(file.width.zip(file.height));
    let bandwidth = params.parse::<i64>("maxVideoBitrate").map(|k| k * 1000).or(file.bitrate).unwrap_or(8_000_000);
    let mut playlist = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
    if audio_only {
        playlist.push_str(&format!("#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},CODECS=\"mp4a.40.2\"\n"));
    } else {
        let resolution = height.map(|(w, h)| format!(",RESOLUTION={w}x{h}")).unwrap_or_default();
        playlist.push_str(&format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth}{resolution},CODECS=\"avc1.640028,mp4a.40.2\"\n"
        ));
    }
    playlist.push_str(&format!("session/{}/base/index.m3u8\n", info.session_id));
    Ok(([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl"), (header::CACHE_CONTROL, "no-cache")], playlist)
        .into_response())
}

/// Session playlists and segments. The session id (128 random bits) is the
/// credential, like the URLs Plex hands to players. Not for sessions an app
/// started (its token is what stops it, not this listener), nor for disabled
/// accounts, nor for accounts signed out because the server requires
/// two-step sign-in and they don't use it.
pub async fn universal_file(
    State(state): State<AppState>,
    Path((session_id, file)): Path<(String, String)>,
) -> PlexResult<Response> {
    let session = state.sessions.get(&session_id).ok_or(PlexError::NotFound)?;
    if session.app_grant.is_some() {
        return Err(PlexError::NotFound);
    }
    let user = AuthUser::load(&state.db, session.user_id).await?.ok_or(PlexError::NotFound)?;
    if state.auth.lacks_two_step(user.id).await? {
        return Err(PlexError::NotFound);
    }
    crate::api::playback::hls_file(State(state), user, Path((session_id, file))).await.map_err(PlexError::from)
}

pub async fn universal_file_flat(state: State<AppState>, path: Path<(String, String)>) -> PlexResult<Response> {
    universal_file(state, path).await
}

/// `decision`: what `start.m3u8` would do — always an HLS stream here.
pub async fn universal_decision(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
) -> PlexResult<Node> {
    let row = playable(&state, &user, path_item(&params)?).await?;
    let libraries = list_libraries(&state.db).await?;
    let files = catalog::files_for_item(&state.db, row.id).await?;
    let extras = meta::load_extras(&state.db, &[row.id]).await?;
    let mut node = meta::item_node(
        &row,
        extras.get(&row.id),
        &files[..files.len().min(1)],
        &Sections::new(&libraries),
        Detail { guids: false, full: true, media: true, markers: false },
    );
    mark_decision(&mut node, Method::Transcode);
    Ok(Node::container()
        .attr("size", 1)
        .attr("allowSync", false)
        .attr("directPlayDecisionCode", 3000)
        .attr("directPlayDecisionText", "App cannot direct play this item. Direct play is disabled.")
        .attr("generalDecisionCode", 1001)
        .attr("generalDecisionText", "Direct play not available; Conversion OK.")
        .attr("transcodeDecisionCode", 1001)
        .attr("transcodeDecisionText", "Direct play not available; Conversion OK.")
        .child(node))
}

/// `stop?session=<client>`: ends the client's sessions.
pub async fn universal_stop(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
    client: ClientInfo,
) -> PlexResult<Node> {
    let mut ended = Vec::new();
    if let Some(id) = params.get_ci("session") {
        if let Some(session) = state.sessions.get(id).filter(|s| s.user_id == user.id || user.is_admin) {
            ended.extend(state.sessions.remove(&session.id));
        }
        ended.extend(state.sessions.take_client_sessions(user.id, id));
    }
    if let Some(client_id) = &client.identifier {
        ended.extend(state.sessions.take_client_sessions(user.id, client_id));
    }
    end_sessions(&state, ended);
    Ok(Node::container().attr("size", 0))
}

pub async fn universal_ping(PlexUser(_user): PlexUser) -> Node {
    Node::container().attr("size", 0)
}
