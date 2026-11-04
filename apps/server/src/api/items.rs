use std::collections::HashMap;
use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use pley_transcoder_api::ThumbnailRequest;
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser};
use crate::catalog::{self, ItemDetail, ItemRow, ItemSummary};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::library::scanner::remux_candidate;
use crate::state::AppState;
use crate::streams;
use crate::util::now;

pub(crate) async fn item_or_404(state: &AppState, user_id: i64, id: i64) -> ApiResult<ItemRow> {
    catalog::item(&state.db, user_id, id).await?.ok_or_else(|| AppError::not_found(format!("item {id} not found")))
}

pub async fn get(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<ItemDetail>> {
    let row = item_or_404(&state, user.id, id).await?;
    let scope = streams::scope(&row);
    let resume = resume_ms(&row);
    let mut detail = catalog::item_detail(&state.db, row).await?;
    detail.locked_fields = crate::overrides::locked_fields(&state.db, id).await?;
    detail.in_watchlist = catalog::in_watchlist(&state.db, user.id, id).await?;
    if matches!(detail.summary.kind.as_str(), "movie" | "show") {
        detail.extras = catalog::extras(&state.db, user.id, id).await?.iter().map(ItemRow::to_summary).collect();
    }
    if !detail.files.is_empty() {
        let pref = streams::load(&state.db, user.id, scope).await?;
        let defaults = streams::defaults(&state.db, user.id).await?;
        let picked = pref.as_ref().is_some_and(|p| p.audio_language.is_some() || p.audio_title.is_some());
        for file in &mut detail.files {
            let selection = streams::resolve(pref.as_ref(), &defaults, &file.streams, None);
            file.selected_audio = selection.audio;
            file.audio_picked = picked;
            file.selected_subtitle = selection.subtitle;
        }
    }
    // Opening an item is a good moment to prepare seek previews, to index
    // keyframes so playback can remux, and to read ahead where Play starts.
    if matches!(detail.summary.kind.as_str(), "movie" | "episode") {
        for file in catalog::files_for_item(&state.db, detail.summary.id).await? {
            state.trickplay.request(&file);
            if file.video_codec.is_some() {
                state.transcoder.prefetch_soon(&file.path, resume);
            }
            let Some(dto) = detail.files.iter_mut().find(|f| f.id == file.id) else { continue };
            if dto.chapters.len() > 1 && state.trickplay.has_chapter_images(&file) {
                let version = file.fingerprint.as_deref().unwrap_or_default();
                for (n, chapter) in dto.chapters.iter_mut().enumerate() {
                    chapter.thumb_url = Some(format!("/api/files/{}/chapters/{n}.jpg?v={version}", file.id));
                }
            }
        }
    }
    if state.transcoder.is_available() {
        for file in &detail.files {
            if remux_candidate(file.container.as_deref(), &file.streams) {
                let transcoder = state.transcoder.clone();
                let path = file.path.clone();
                tokio::spawn(async move {
                    let _ = transcoder.request_keyframes(&path).await;
                });
            }
        }
    }
    // Apps and non-administrators get the file names, not where the server
    // keeps them.
    if user.app.is_some() || !user.is_admin {
        for file in &mut detail.files {
            file.path = std::path::Path::new(&file.path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
    }
    Ok(Json(detail))
}

/// Where Play starts for this user: the resume point, or the beginning once watched.
fn resume_ms(row: &ItemRow) -> i64 {
    if row.watched { 0 } else { row.position_ms.max(0) }
}

/// Someone is about to press Play (a card's Play button has the pointer or
/// the focus): read ahead where the item's video, or a show's or season's
/// next episode, starts for them. Answers at once; the reading happens in the
/// background, at most every two minutes per file.
pub async fn prefetch(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    let target = match row.kind.as_str() {
        "movie" | "episode" | "extra" => Some(row),
        "show" | "season" => match catalog::on_deck(&state.db, user.id, &row).await? {
            Some(next) => catalog::item(&state.db, user.id, next).await?,
            None => None,
        },
        _ => None,
    };
    if let Some(item) = target {
        let start = resume_ms(&item);
        for file in catalog::files_for_item(&state.db, item.id).await? {
            if file.video_codec.is_some() {
                state.transcoder.prefetch_soon(&file.path, start);
            }
        }
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub struct StreamChoice {
    /// File the indexes refer to (default: the item's first file).
    file_id: Option<i64>,
    /// Left out: the audio chosen before stays (subtitles alone were
    /// picked). `null`: no audio of its own, chosen on each play as for a
    /// title without a choice.
    #[serde(default, deserialize_with = "super::nullable")]
    audio_index: Option<Option<i64>>,
    /// `None` = subtitles off.
    subtitle_index: Option<i64>,
}

/// Remember the audio / subtitle tracks the user picked, for the whole show
/// when this is an episode.
pub async fn set_streams(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(choice): Json<StreamChoice>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    let files = catalog::files_for_item(&state.db, row.id).await?;
    let file = match choice.file_id {
        Some(file_id) => files.iter().find(|f| f.id == file_id),
        None => files.first(),
    }
    .ok_or_else(|| AppError::not_found("file not found"))?;
    let scope = streams::scope(&row);
    let mut pref = streams::from_selection(&file.streams(), choice.audio_index.flatten(), choice.subtitle_index);
    if choice.audio_index.is_none()
        && let Some(before) = streams::load(&state.db, user.id, scope).await?
    {
        (pref.audio_language, pref.audio_title) = (before.audio_language, before.audio_title);
    }
    streams::save(&state.db, user.id, scope, &pref).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct ItemList {
    items: Vec<ItemSummary>,
}

pub async fn children(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<ItemList>> {
    item_or_404(&state, user.id, id).await?;
    let rows = catalog::children(&state.db, user.id, id).await?;
    let summaries = super::wants_summaries(&query);
    Ok(Json(ItemList { items: rows.iter().map(|r| r.to_summary_with(summaries)).collect() }))
}

/// "More Like This" for movies and shows (`limit`, default 20).
pub async fn related(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<ItemList>> {
    let row = item_or_404(&state, user.id, id).await?;
    if !matches!(row.kind.as_str(), "movie" | "show") {
        return Ok(Json(ItemList { items: Vec::new() }));
    }
    let limit = query.get("limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(20).clamp(1, 60);
    let rows = crate::related::related(&state.db, user.id, id, limit).await?;
    Ok(Json(ItemList { items: rows.iter().map(ItemRow::to_summary).collect() }))
}

#[derive(Serialize)]
pub struct MaybeItem {
    item: Option<ItemSummary>,
}

pub async fn on_deck(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<MaybeItem>> {
    let row = item_or_404(&state, user.id, id).await?;
    let target = catalog::on_deck(&state.db, user.id, &row).await?;
    let item = match target {
        Some(id) => catalog::item(&state.db, user.id, id).await?.map(|r| r.to_summary()),
        None => None,
    };
    Ok(Json(MaybeItem { item }))
}

pub async fn next(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<MaybeItem>> {
    let row = item_or_404(&state, user.id, id).await?;
    let item = match catalog::next_item(&state.db, &row).await? {
        Some(id) => catalog::item(&state.db, user.id, id).await?.map(|r| r.to_summary()),
        None => None,
    };
    Ok(Json(MaybeItem { item }))
}

#[derive(Deserialize)]
pub struct Progress {
    position_ms: i64,
    #[serde(default)]
    duration_ms: Option<i64>,
    #[serde(default = "playing")]
    state: String,
    #[serde(default)]
    session_id: Option<String>,
    /// How long ago a report sent late was made (a download played while
    /// the server was out of reach): it counts unless something newer
    /// happened to the item since.
    #[serde(default)]
    age_ms: Option<i64>,
}

fn playing() -> String {
    "playing".into()
}

/// The oldest a late report counts as: older ones count as this old.
const MAX_REPORT_AGE_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// Whether the user's state of `item_id` changed after a report made
/// `age_ms` ago (played elsewhere since, marked watched, …): the newer wins.
async fn changed_since(state: &AppState, user_id: i64, item_id: i64, age_ms: i64) -> sqlx::Result<bool> {
    let at = (crate::util::now_ms() - age_ms.min(MAX_REPORT_AGE_MS)) / 1000;
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_item_state WHERE user_id = ? AND item_id = ? AND updated_at > ?)",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(at)
    .fetch_one(&state.db.read)
    .await
}

pub async fn progress(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<Progress>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    if !matches!(row.kind.as_str(), "movie" | "episode" | "track" | "extra") {
        return Err(AppError::bad_request("progress can only be recorded for playable items"));
    }
    if !matches!(body.state.as_str(), "playing" | "paused" | "stopped" | "buffering") {
        return Err(AppError::bad_request("state must be playing, paused or stopped"));
    }
    if let Some(age_ms) = body.age_ms.filter(|age| *age > 0)
        && changed_since(&state, user.id, id, age_ms).await?
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    let duration = body.duration_ms.filter(|d| *d > 0).or(row.duration_ms).unwrap_or(0);
    let device = crate::accounts::device_name(headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()));
    let recorded = catalog::record_progress(
        &state.db,
        &state.events,
        user.id,
        id,
        body.position_ms,
        duration,
        Some(&device),
        &body.state,
    )
    .await?;
    if recorded.left_watchlist {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }
    if recorded.counted {
        state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    }
    // The place is kept, but a session stopped from elsewhere is over: a
    // direct play's player hears it here (its file requests name no session).
    if let Some(stopped) = body.session_id.as_deref().and_then(|sid| state.sessions.stopped(sid))
        && stopped.user_id == user.id
    {
        return Err(AppError::SessionStopped(stopped.message));
    }
    if let Some(session) = body.session_id.as_deref().and_then(|sid| state.sessions.get(sid))
        && session.user_id == user.id
    {
        let changed = session.update(body.position_ms, &body.state);
        if session.should_touch_transcoder()
            && let Some(job) = session.job_id()
        {
            let transcoder = state.transcoder.clone();
            tokio::spawn(async move {
                let _ = transcoder.touch_job(&job).await;
            });
        }
        if changed {
            state.events.send(ServerEvent::SessionsChanged { user_id: session.user_id });
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Rating {
    rating: Option<f64>,
}

/// `PUT /api/items/{id}/rating`: the user's own rating, 0-10 (two points
/// per star), or `null` to clear it.
pub async fn set_rating(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<Rating>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    let rating = match body.rating {
        Some(value) => {
            Some(catalog::valid_rating(value).ok_or_else(|| AppError::bad_request("rating must be between 0 and 10"))?)
        }
        None => None,
    };
    catalog::set_rating(&state.db, user.id, id, rating).await?;
    state.events.send(ServerEvent::LibraryChanged { library_id: row.library_id });
    Ok(StatusCode::NO_CONTENT)
}

pub async fn watched(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    if catalog::set_watched(&state.db, &state.events, user.id, id, &row.kind, true).await? {
        state.events.send(ServerEvent::WatchlistChanged { user_id: user.id });
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unwatched(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    catalog::set_watched(&state.db, &state.events, user.id, id, &row.kind, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    if !state.metadata.can_refresh(&row.kind) {
        return Err(AppError::bad_request(match row.kind.as_str() {
            "movie" => "add a TMDB API key in Settings → Server to fetch metadata",
            "artist" | "album" | "track" => "turn on MusicBrainz in Settings → Server to fetch metadata",
            _ => "choose a TV source in Settings → Server to fetch metadata",
        }));
    }
    let metadata = state.metadata.clone();
    let events = state.events.clone();
    tokio::spawn(async move {
        match metadata.refresh_item(id).await {
            Ok(()) => events.send(ServerEvent::LibraryChanged { library_id: row.library_id }),
            Err(err) => tracing::warn!("metadata refresh for item {id} failed: {err:#}"),
        }
    });
    Ok(StatusCode::ACCEPTED)
}

/// Find intros and credits again: a movie, or the seasons of an episode,
/// season or show.
pub async fn analyze(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    if !state.markers.analyze_item(id, &row.kind).await? {
        return Err(AppError::bad_request("only movies, shows, seasons and episodes have intros and credits"));
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(sqlx::FromRow)]
struct ArtworkPaths {
    kind: String,
    poster: Option<String>,
    backdrop: Option<String>,
    thumb: Option<String>,
    parent_id: Option<i64>,
}

pub async fn image(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, kind)): Path<(i64, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if !matches!(kind.as_str(), "poster" | "backdrop" | "thumb") {
        return Err(AppError::not_found("unknown image kind"));
    }
    if !catalog::is_visible(&state.db, user.id, id).await? {
        return Err(AppError::not_found(format!("item {id} not found")));
    }
    let width = query.get("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0);
    artwork(&state, id, &kind, width, query.contains_key("v"), &headers).await
}

/// An item's `poster`, `backdrop` or `thumb` at `width` (or as it is), for a
/// caller that checked it may be seen; `versioned`: asked for at an address
/// that changes with the artwork.
pub(crate) async fn artwork(
    state: &AppState,
    id: i64,
    kind: &str,
    width: Option<u32>,
    versioned: bool,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let path = resolve_artwork(state, id, kind).await?;
    let image = state.images.load(&path, width).await?;
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(image.etag.as_str()) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let mut out = HeaderMap::new();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(image.content_type));
    // Versioned URLs (`?v=<updated_at>`) change whenever the artwork does.
    let cache = if versioned { "private, max-age=31536000, immutable" } else { "private, max-age=300" };
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(etag) = HeaderValue::from_str(&image.etag) {
        out.insert(header::ETAG, etag);
    }
    Ok((out, image.bytes).into_response())
}

/// Path of the artwork file, generating it (frame grab, embedded cover) or
/// falling back to the parent's artwork when the item has none.
async fn resolve_artwork(state: &AppState, id: i64, kind: &str) -> ApiResult<PathBuf> {
    let mut current = id;
    for _ in 0..3 {
        let row: ArtworkPaths =
            sqlx::query_as("SELECT kind, poster, backdrop, thumb, parent_id FROM items WHERE id = ?")
                .bind(current)
                .fetch_optional(&state.db.read)
                .await?
                .ok_or_else(|| AppError::not_found("item not found"))?;
        let existing = match kind {
            "poster" => row.poster.clone(),
            "backdrop" => row.backdrop.clone(),
            _ => row.thumb.clone(),
        };
        if let Some(path) = existing.and_then(|r| state.store.path(&r)).filter(|p| p.is_file()) {
            return Ok(path);
        }
        match (kind, row.kind.as_str()) {
            // A frame grab; a missing or unreadable file just has no picture.
            ("thumb", "movie" | "episode" | "extra") => {
                return generate_thumb(state, current).await.map_err(|err| {
                    tracing::debug!("frame grab of item {current}: {err}");
                    AppError::not_found("no artwork")
                });
            }
            ("poster", "album") => {
                if let Some(path) = extract_cover(state, current).await? {
                    return Ok(path);
                }
                return Err(AppError::not_found("no artwork"));
            }
            ("poster", "season" | "episode" | "track" | "extra") | ("backdrop", _) => match row.parent_id {
                Some(parent) => current = parent,
                None => break,
            },
            _ => break,
        }
    }
    Err(AppError::not_found("no artwork"))
}

/// A frame grab of the item's first file, kept in the file's media folder.
async fn generate_thumb(state: &AppState, item_id: i64) -> ApiResult<PathBuf> {
    let file = catalog::files_for_item(&state.db, item_id)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::not_found("no media file"))?;
    let fingerprint = state.store.fingerprint_of(file.id, &file.path, file.fingerprint.as_deref()).await?;
    let dir = state.store.media_dir(&fingerprint).ok_or_else(|| AppError::not_found("no media folder"))?;
    let target = dir.join("thumb.jpg");
    if target.is_file() {
        return Ok(target);
    }
    // A file that can't be grabbed isn't run through ffmpeg on every request.
    if state.images.grab_failed_lately(file.id, &fingerprint) {
        return Err(AppError::not_found("no frame grab"));
    }
    let lock = state.images.generation_lock(file.id);
    let made: ApiResult<()> = async {
        let _guard = lock.lock().await;
        if target.is_file() {
            return Ok(());
        }
        let hdr = file.media_info().and_then(|m| m.video().map(|v| v.dynamic_range().is_hdr())).unwrap_or(false);
        let bytes = state
            .transcoder
            .thumbnail(&ThumbnailRequest {
                path: file.path.clone(),
                time_ms: None,
                width: Some(640),
                duration_ms: file.duration_ms,
                hdr,
            })
            .await?;
        tokio::fs::create_dir_all(&dir).await?;
        let tmp = dir.join("thumb.jpg.tmp");
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &target).await?;
        Ok(())
    }
    .await;
    // Our handle goes first: the lock is dropped from the map once unused.
    drop(lock);
    state.images.release_generation_lock(file.id);
    match made {
        Ok(()) => Ok(target),
        Err(err) => {
            // Not when the transcoder is down: that isn't the file's fault.
            if state.transcoder.is_available() {
                state.images.grab_failed(file.id, &fingerprint);
            }
            Err(err)
        }
    }
}

/// The cover embedded in an album's tracks, stored as the album's poster.
async fn extract_cover(state: &AppState, album_id: i64) -> ApiResult<Option<PathBuf>> {
    let lock = state.images.generation_lock(album_id);
    let _guard = lock.lock().await;
    // Another request may have stored it meanwhile.
    let poster: Option<String> =
        sqlx::query_scalar("SELECT poster FROM items WHERE id = ?").bind(album_id).fetch_one(&state.db.read).await?;
    if let Some(path) = poster.and_then(|p| state.store.path(&p)).filter(|p| p.is_file()) {
        return Ok(Some(path));
    }
    // Albums without embedded art are not opened again (until the cache is cleared).
    let marker = state.config.cache_dir().join("covers").join(format!("{album_id}.none"));
    if marker.exists() {
        return Ok(None);
    }
    let tracks: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT f.path, f.media_info FROM media_files f JOIN items t ON t.id = f.item_id WHERE t.parent_id = ? ORDER BY t.disc, t.idx LIMIT 5",
    )
    .bind(album_id)
    .fetch_all(&state.db.read)
    .await?;
    for (path, info) in tracks {
        let has_cover = info
            .as_deref()
            .and_then(|j| serde_json::from_str::<pley_transcoder_api::MediaInfo>(j).ok())
            .is_some_and(|m| m.has_cover_art);
        if !has_cover {
            continue;
        }
        if let Ok((bytes, _)) = state.transcoder.cover(&path).await {
            let Some(ext) = crate::store::picture_ext(&bytes) else { continue };
            let blob = state.store.put(&bytes, ext).await?;
            sqlx::query("UPDATE items SET poster = ?, updated_at = ? WHERE id = ? AND poster IS NULL")
                .bind(&blob)
                .bind(now())
                .bind(album_id)
                .execute(&state.db.write)
                .await?;
            return Ok(state.store.path(&blob));
        }
    }
    if state.transcoder.is_available() {
        if let Some(dir) = marker.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        let _ = tokio::fs::write(&marker, b"").await;
    }
    Ok(None)
}
