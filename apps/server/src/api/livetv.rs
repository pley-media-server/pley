//! Live TV (`/api/live`): channels, the guide and watching for everyone
//! Live TV is open to; sources, channels and connections for administrators.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use pley_transcoder_api::LivePlan;
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser, CurrentToken};
use crate::error::{ApiResult, AppError};
use crate::livetv::tuner::{Tune, TunerStatus, ViewerKind, WatchAs, Watching};
use crate::livetv::{
    AdminChannel, AdminGroup, AdminPage, Category, Channel, ChannelPage, ChannelPatch, Groups, Guide, LineupFilter,
    LiveSettings, Source, SourceInput, SourceTest,
};
use crate::playback::{Controls, DeviceKind};
use crate::state::AppState;
use crate::util::now;

/// What the apps need to know to show Live TV at all.
#[derive(Serialize)]
pub struct Summary {
    /// Live TV is open to this account and has channels.
    available: bool,
    /// Open to this account (it may have no channels yet).
    allowed: bool,
    channels: usize,
    /// Sources set up (administrators: whether to show where to add one).
    sources: usize,
    /// The channels watched last, newest first.
    recent: Vec<i64>,
}

pub async fn summary(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Summary>> {
    let allowed = state.live.may_watch(&user).await;
    let channels = if allowed { state.live.channel_count().await? } else { 0 };
    let sources = if user.is_admin { state.live.source_count().await? } else { 0 };
    let recent = if allowed { state.live.recent(&user, 12).await? } else { Vec::new() };
    Ok(Json(Summary { available: allowed && channels > 0, allowed, channels, sources, recent }))
}

pub async fn channels(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<Channel>>> {
    state.live.check_access(&user).await?;
    Ok(Json(state.live.channels(&user).await?))
}

pub async fn groups(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Groups>> {
    state.live.check_access(&user).await?;
    Ok(Json(state.live.groups(&user).await?))
}

#[derive(Deserialize)]
pub struct BrowseQuery {
    /// `all` (the default), `favorites`, `recent` or `group`.
    #[serde(default)]
    category: Option<String>,
    /// The group, for `category=group`.
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    /// A channel whose page is wanted (the offset is then its page's).
    #[serde(default)]
    around: Option<i64>,
}

/// A page of a category's channels, with what's on now and next.
pub async fn browse(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<BrowseQuery>,
) -> ApiResult<Json<ChannelPage>> {
    state.live.check_access(&user).await?;
    let category = match query.category.as_deref().unwrap_or("all") {
        "all" => Category::All,
        "favorites" => Category::Favorites,
        "recent" => Category::Recent,
        "group" => Category::Group(query.group.ok_or_else(|| AppError::bad_request("`group` names the group"))?),
        other => return Err(AppError::bad_request(format!("no category `{other}`"))),
    };
    let page = state
        .live
        .browse(
            &user,
            &category,
            query.q.as_deref().unwrap_or(""),
            query.offset.unwrap_or(0),
            query.limit.unwrap_or(100),
            query.around,
        )
        .await?;
    Ok(Json(page))
}

pub async fn channel(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<Channel>> {
    state.live.check_access(&user).await?;
    Ok(Json(state.live.channel(&user, id).await?))
}

pub async fn favorite(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    state.live.check_access(&user).await?;
    state.live.set_favorite(&user, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unfavorite(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    state.live.check_access(&user).await?;
    state.live.set_favorite(&user, id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct GuideQuery {
    from: Option<i64>,
    to: Option<i64>,
    /// The channels wanted, by id with commas between (all when not given).
    #[serde(default)]
    ids: Option<String>,
}

/// The guide from `from` (default: half an hour ago) to `to` (default: six
/// hours after `from`), two days at most.
pub async fn guide(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<GuideQuery>,
) -> ApiResult<Json<Guide>> {
    state.live.check_access(&user).await?;
    let from = query.from.unwrap_or_else(|| now() - 1800);
    let to = query.to.unwrap_or(from + 6 * 3600);
    let ids = query
        .ids
        .map(|ids| {
            ids.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().parse::<i64>().map_err(|_| AppError::bad_request("`ids` are channel ids")))
                .collect::<Result<HashSet<i64>, _>>()
        })
        .transpose()?;
    Ok(Json(state.live.guide(&user, from, to, ids.as_ref()).await?))
}

async fn picture(
    state: &AppState,
    path: std::path::PathBuf,
    query: &HashMap<String, String>,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let width = query.get("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0);
    let image = state.images.load(&path, width).await?;
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(image.etag.as_str()) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let mut out = HeaderMap::new();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(image.content_type));
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    if let Ok(etag) = HeaderValue::from_str(&image.etag) {
        out.insert(header::ETAG, etag);
    }
    Ok((out, image.bytes).into_response())
}

pub async fn logo(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    state.live.check_access(&user).await?;
    let path = state.live.logo(id).await?;
    picture(&state, path, &query, &headers).await
}

pub async fn program_image(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, start)): Path<(i64, i64)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    state.live.check_access(&user).await?;
    let path = state.live.program_image(id, start).await?;
    picture(&state, path, &query, &headers).await
}

#[derive(Deserialize, Default)]
pub struct WatchRequest {
    /// The device watching: what it watched before ends.
    #[serde(default)]
    client_id: Option<String>,
    /// The player can't send the Authorization header (a TV's own player):
    /// the playlist's address carries a key instead.
    #[serde(default)]
    keyed_urls: bool,
    /// Every connection being in use, the channel whose connection to take
    /// (one the `409` said may be).
    #[serde(default)]
    take_over: Option<i64>,
    /// The commands this player obeys from a remote, as for a title's play
    /// request ([`crate::playback::PlaybackRequest::controls`]).
    #[serde(default, deserialize_with = "crate::playback::known_controls")]
    controls: Controls,
    /// What the player calls its device, as for a title's play request.
    #[serde(default)]
    device_name: Option<String>,
    /// What kind of device it is, as for a title's play request.
    #[serde(default, deserialize_with = "crate::playback::known_device_kind")]
    device_kind: Option<DeviceKind>,
}

#[derive(Serialize)]
pub struct WatchInfo {
    #[serde(flatten)]
    watching: Watching,
    channel: Channel,
}

/// Starts `user` watching channel `id` as `how` asks; whether the account
/// is an administrator's and who is in the home are read here.
async fn start_watching(
    state: &AppState,
    user: &AuthUser,
    id: i64,
    mut how: WatchAs,
) -> ApiResult<(Watching, Channel)> {
    state.live.check_access(user).await?;
    // An app acts as a viewer, but whose connections it may take over goes by the account.
    how.admin = user.is_admin || state.live.account_is_admin(user.id).await;
    // Read as the take-over is asked for: who is in the home then is told who took their connection.
    how.home = if how.take_over.is_some() { state.live.home_now(user.id).await } else { None };
    let channel = state.live.channel(user, id).await?;
    let row = state.live.channel_row(id).await?;
    let (source_name, max_connections, user_agent) = state.live.source_of(&row).await?;
    let watching =
        state.live.tuners.watch(user, Tune { channel: &row, source_name, max_connections, user_agent }, how).await?;
    state.live.watched(user.id, id).await;
    Ok((watching, channel))
}

/// Starts watching a channel: its live HLS playlist's address.
pub async fn watch(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    headers: HeaderMap,
    CurrentToken(token): CurrentToken,
    body: Option<Json<WatchRequest>>,
) -> ApiResult<Json<WatchInfo>> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let how = WatchAs {
        client_id: body.client_id,
        kind: ViewerKind::Hls,
        keyed: body.keyed_urls,
        admin: false,
        take_over: body.take_over,
        home: None,
        controls: body.controls,
        device: super::remote::device_name(body.device_name.as_deref(), &headers),
        device_id: super::remote::device_id_of(&user, token.as_deref()),
        device_kind: super::remote::device_kind_of(body.device_kind, &user, super::remote::user_agent(&headers)),
    };
    let (watching, channel) = start_watching(&state, &user, id, how).await?;
    Ok(Json(WatchInfo { watching, channel }))
}

/// The channel's MPEG-TS as the provider sends it, for as long as it is
/// read: for players that read MPEG-TS themselves (VLC, Kodi, TVs).
pub async fn stream(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
    CurrentToken(token): CurrentToken,
    parts: axum::http::request::Parts,
) -> ApiResult<Response> {
    let client_id = query.get("client_id").cloned();
    let device_kind = query.get("device_kind").and_then(|kind| DeviceKind::from_name(kind));
    // Taking another channel's connection ends what others watch: not for a
    // GET the browser's cookie signs in, which over plain HTTP may be
    // another site's link or picture (pley's own page asks with POST …/watch).
    let take_over =
        query.get("take_over").and_then(|t| t.parse().ok()).filter(|_| !crate::auth::signed_in_by_cookie(&parts));
    // Players that read the stream themselves (VLC, Kodi) take no commands:
    // their device is named from the User-Agent, and is the kind the query
    // says or the app is.
    let how = WatchAs {
        client_id,
        kind: ViewerKind::Stream,
        keyed: false,
        admin: false,
        take_over,
        home: None,
        controls: Controls::new(),
        device: super::remote::device_name(None, &parts.headers),
        device_id: super::remote::device_id_of(&user, token.as_deref()),
        device_kind: super::remote::device_kind_of(device_kind, &user, super::remote::user_agent(&parts.headers)),
    };
    let (watching, _) = start_watching(&state, &user, id, how).await?;
    let body = state.live.tuners.stream(&watching.session_id, &user)?;
    Response::builder()
        .header(header::CONTENT_TYPE, "video/mp2t")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(body))
        .map_err(|e| AppError::Internal(e.into()))
}

/// A watching session's live playlist or segment.
pub async fn session_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path((session, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    session_media(&state, &user, &session, &file).await
}

/// `GET /api/live/media/{key}/{file}`: a watching session's playlist or
/// segment for a player that can't send the Authorization header, a TV's own
/// player (docs/API.md, "Keyed URLs"). The key `watch` gave for
/// `keyed_urls` stands in for the token, for that session alone; it ends
/// with the session, the account and the app's sign-in.
pub async fn keyed_file(
    State(state): State<AppState>,
    Path((key, file)): Path<(String, String)>,
) -> ApiResult<Response> {
    let gone = || AppError::SessionStopped(Some("this channel isn't being watched any more".into()));
    let Some((session, user_id, grant)) = state.live.tuners.by_media_key(&key) else {
        // Taken over or stopped: why, as the session's own address says.
        return Err(state.live.tuners.ended_by_key(&key).map_or_else(gone, AppError::SessionStopped));
    };
    let mut user = AuthUser::load(&state.db, user_id).await?.ok_or_else(gone)?;
    if let Some(grant) = grant {
        // Signed out since: the key goes with the token.
        let access = crate::auth::grants::access_of(&state.db, grant).await?.ok_or_else(gone)?;
        user.is_admin = false;
        user.app = Some(std::sync::Arc::new(access));
    }
    // Signed out since because the server requires two-step sign-in.
    for id in std::iter::once(user.id).chain(user.app.as_ref().map(|app| app.approved_by)) {
        if state.auth.lacks_two_step(id).await? {
            return Err(gone());
        }
    }
    session_media(&state, &user, &session, &file).await
}

async fn session_media(state: &AppState, user: &AuthUser, session: &str, file: &str) -> ApiResult<Response> {
    if file == "index.m3u8" {
        let settings = state.live.settings();
        let hwaccel = state.settings.get().hwaccel;
        let text = state.live.tuners.playlist(session, user, &settings, hwaccel).await?;
        return Ok((
            [(header::CONTENT_TYPE, "application/vnd.apple.mpegurl"), (header::CACHE_CONTROL, "no-cache")],
            text,
        )
            .into_response());
    }
    let upstream = state.live.tuners.segment(session, user, file).await?;
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, "video/mp2t")
        .header(header::CACHE_CONTROL, "private, max-age=60");
    if let Some(len) = upstream.content_length() {
        response = response.header(header::CONTENT_LENGTH, len);
    }
    response.body(Body::from_stream(upstream.bytes_stream())).map_err(|e| AppError::Internal(e.into()))
}

/// What the channel a session watches is made into (`null` when not known
/// within a few seconds): a player that reads MPEG-TS plays a copied channel
/// from `stream.ts` instead of waiting for its first segments.
pub async fn session_plan(
    State(state): State<AppState>,
    user: AuthUser,
    Path(session): Path<String>,
) -> ApiResult<Json<Option<LivePlan>>> {
    let settings = state.live.settings();
    let hwaccel = state.settings.get().hwaccel;
    let plan = state.live.tuners.plan(&session, &user, &settings, hwaccel, Duration::from_secs(3)).await?;
    Ok(Json(plan))
}

/// Stops watching: the connection is let go once nobody else watches.
pub async fn leave(State(state): State<AppState>, user: AuthUser, Path(session): Path<String>) -> StatusCode {
    if state.live.tuners.leave(&session, &user) { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

// ------------------------------------------------------------ administrators

pub async fn get_settings(State(state): State<AppState>, _admin: AdminUser) -> Json<LiveSettings> {
    Json((*state.live.settings()).clone())
}

pub async fn put_settings(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(settings): Json<LiveSettings>,
) -> ApiResult<Json<LiveSettings>> {
    Ok(Json((*state.live.save_settings(settings).await?).clone()))
}

pub async fn sources(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<Source>>> {
    Ok(Json(state.live.sources().await?))
}

pub async fn add_source(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(input): Json<SourceInput>,
) -> ApiResult<(StatusCode, Json<Source>)> {
    Ok((StatusCode::CREATED, Json(state.live.add_source(input).await?)))
}

pub async fn update_source(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(input): Json<SourceInput>,
) -> ApiResult<Json<Source>> {
    Ok(Json(state.live.update_source(id, input).await?))
}

pub async fn delete_source(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    state.live.delete_source(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Reads a source's channels and guide again, now.
pub async fn refresh_source(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    state.live.source(id).await?;
    state.live.refresh_soon(id);
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub struct TestRequest {
    #[serde(flatten)]
    input: SourceInput,
    /// The saved source being changed: its password, when none is given.
    #[serde(default)]
    source_id: Option<i64>,
}

/// Reads what a source's details give without saving them.
pub async fn test_source(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<TestRequest>,
) -> ApiResult<Json<SourceTest>> {
    Ok(Json(state.live.test_source(req.input, req.source_id).await?))
}

#[derive(Deserialize)]
pub struct ChannelsQuery {
    source_id: Option<i64>,
}

pub async fn admin_channels(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<ChannelsQuery>,
) -> ApiResult<Json<Vec<AdminChannel>>> {
    Ok(Json(state.live.admin_channels(query.source_id).await?))
}

#[derive(Deserialize)]
pub struct LineupPageQuery {
    #[serde(flatten)]
    filter: LineupFilter,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

/// A page of the lineup as administrators see it, filtered.
pub async fn admin_page(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<LineupPageQuery>,
) -> ApiResult<Json<AdminPage>> {
    Ok(Json(state.live.admin_page(&query.filter, query.offset.unwrap_or(0), query.limit.unwrap_or(100)).await?))
}

pub async fn admin_groups(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<AdminGroup>>> {
    Ok(Json(state.live.admin_groups().await?))
}

#[derive(Deserialize)]
pub struct ChannelsPatch {
    #[serde(default)]
    ids: Vec<i64>,
    /// Every channel of these groups, as well as `ids`.
    #[serde(default)]
    groups: Vec<String>,
    /// Every channel this filter keeps, as well.
    #[serde(default)]
    filter: Option<LineupFilter>,
    #[serde(flatten)]
    patch: ChannelPatch,
}

/// Hides, shows, renames or renumbers channels (`ids`, and those of
/// `groups`), or gives them a guide channel of their own.
pub async fn patch_channels(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<ChannelsPatch>,
) -> ApiResult<StatusCode> {
    if req.ids.len() > 20_000 || req.groups.len() > 5_000 {
        return Err(AppError::bad_request("too many channels at once"));
    }
    let mut ids = req.ids;
    if !req.groups.is_empty() {
        ids.extend(state.live.group_channels(&req.groups).await?);
    }
    if let Some(filter) = &req.filter {
        ids.extend(state.live.admin_ids(filter).await?);
    }
    state.live.patch_channels(&ids, &req.patch).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn tuners(State(state): State<AppState>, _admin: AdminUser) -> Json<Vec<TunerStatus>> {
    Json(state.live.tuners.status().await)
}

/// Stops a channel being pulled, and everyone watching it.
pub async fn stop_tuner(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<i64>) -> StatusCode {
    state.live.tuners.stop_channel(id).await;
    StatusCode::NO_CONTENT
}
