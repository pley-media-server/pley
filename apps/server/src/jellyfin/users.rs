//! Signing in and the signed-in user: `/Users/AuthenticateByName` makes a
//! token for other apps through the same checks as Pley's own sign-in
//! (attempt limits, disabled accounts, two-step sign-in), named after the
//! client and its device, so the account page lists and revokes it.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Serialize;

use super::ids::Kind;
use super::{Body, Client, ClientOf, Jellyfin, JfError, JfResult, JfUser};
use crate::auth::{AuthUser, ClientIp, SourceIp, UsedFrom, compatibility};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users/authenticatebyname", post(authenticate))
        .route("/users/me", get(me))
        .route("/users", get(users))
        .route("/users/public", get(public_users))
        .route("/users/{id}", get(user))
        .route("/users/{id}/images/primary", get(avatar).head(avatar))
        .route("/userimage", get(user_image).head(user_image))
        .route("/sessions/logout", post(logout))
        .route("/sessions/capabilities", post(accepted))
        .route("/sessions/capabilities/full", post(accepted))
        .route("/displaypreferences/{id}", get(display_preferences).post(accepted))
}

/// A time as Jellyfin writes them: ISO 8601 in UTC, seven digits of
/// fractions and `Z` (Kotlin clients read a date without its zone as none).
pub fn iso(secs: i64) -> String {
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 as a date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.0000000Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    name: String,
    server_id: String,
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_image_tag: Option<String>,
    has_password: bool,
    has_configured_password: bool,
    has_configured_easy_password: bool,
    enable_auto_login: bool,
    last_login_date: String,
    last_activity_date: String,
    configuration: UserConfiguration,
    policy: UserPolicy,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UserConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_language_preference: Option<String>,
    play_default_audio_track: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle_language_preference: Option<String>,
    display_missing_episodes: bool,
    grouped_folders: [(); 0],
    subtitle_mode: &'static str,
    display_collections_view: bool,
    enable_local_password: bool,
    ordered_views: [(); 0],
    latest_items_excludes: [(); 0],
    my_media_excludes: [(); 0],
    hide_played_in_latest: bool,
    remember_audio_selections: bool,
    remember_subtitle_selections: bool,
    enable_next_episode_auto_play: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UserPolicy {
    is_administrator: bool,
    is_hidden: bool,
    enable_collection_management: bool,
    enable_subtitle_management: bool,
    enable_lyric_management: bool,
    is_disabled: bool,
    enable_user_preference_access: bool,
    enable_remote_control_of_other_users: bool,
    enable_shared_device_control: bool,
    enable_remote_access: bool,
    enable_live_tv_management: bool,
    enable_live_tv_access: bool,
    enable_media_playback: bool,
    enable_audio_playback_transcoding: bool,
    enable_video_playback_transcoding: bool,
    enable_playback_remuxing: bool,
    force_remote_source_transcoding: bool,
    enable_content_deletion: bool,
    enable_content_downloading: bool,
    enable_sync_transcoding: bool,
    enable_media_conversion: bool,
    enable_all_devices: bool,
    enable_all_channels: bool,
    enable_all_folders: bool,
    enable_public_sharing: bool,
    invalid_login_attempt_count: i64,
    login_attempts_before_lockout: i64,
    max_active_sessions: i64,
    remote_client_bitrate_limit: i64,
    authentication_provider_id: &'static str,
    password_reset_provider_id: &'static str,
    sync_play_access: &'static str,
}

/// The user as clients read them. Their rights are what the token allows:
/// a player's token is a viewer's, an administrator's account included.
pub async fn user_dto(state: &AppState, jellyfin: &Jellyfin, user: &AuthUser) -> JfResult<UserDto> {
    let account = crate::accounts::user(&state.db, user.id).await?.ok_or(JfError::Unauthorized)?;
    let seen = iso(crate::util::now());
    let subtitle_mode = match account.subtitle_mode {
        crate::streams::SubtitleMode::Off => "None",
        crate::streams::SubtitleMode::Forced => "Smart",
        crate::streams::SubtitleMode::Foreign => "Smart",
        crate::streams::SubtitleMode::Always => "Always",
    };
    Ok(UserDto {
        name: account.name.clone(),
        server_id: jellyfin.server_id.clone(),
        id: jellyfin.ids.of(Kind::User, user.id),
        primary_image_tag: account.avatar_url.as_deref().map(tag_of),
        has_password: account.has_password,
        has_configured_password: account.has_password,
        has_configured_easy_password: false,
        enable_auto_login: false,
        last_login_date: seen.clone(),
        last_activity_date: seen,
        configuration: UserConfiguration {
            audio_language_preference: account.audio_language.clone(),
            play_default_audio_track: account.audio_language.is_none(),
            subtitle_language_preference: account.subtitle_language.clone(),
            display_missing_episodes: false,
            grouped_folders: [],
            subtitle_mode,
            display_collections_view: true,
            enable_local_password: false,
            ordered_views: [],
            latest_items_excludes: [],
            my_media_excludes: [],
            hide_played_in_latest: false,
            remember_audio_selections: true,
            remember_subtitle_selections: true,
            enable_next_episode_auto_play: true,
        },
        policy: UserPolicy {
            // Nothing administers through this API, an administrator's
            // sign-in included: clients then offer no dashboard that fails.
            is_administrator: false,
            is_hidden: true,
            enable_collection_management: false,
            enable_subtitle_management: false,
            enable_lyric_management: false,
            is_disabled: account.disabled,
            enable_user_preference_access: true,
            enable_remote_control_of_other_users: false,
            enable_shared_device_control: false,
            enable_remote_access: true,
            enable_live_tv_management: false,
            enable_live_tv_access: false,
            enable_media_playback: true,
            enable_audio_playback_transcoding: true,
            enable_video_playback_transcoding: true,
            enable_playback_remuxing: true,
            force_remote_source_transcoding: false,
            enable_content_deletion: false,
            enable_content_downloading: true,
            enable_sync_transcoding: false,
            enable_media_conversion: false,
            enable_all_devices: true,
            enable_all_channels: false,
            enable_all_folders: account.all_libraries,
            enable_public_sharing: false,
            invalid_login_attempt_count: 0,
            login_attempts_before_lockout: -1,
            max_active_sessions: 0,
            remote_client_bitrate_limit: 0,
            authentication_provider_id: "Jellyfin.Server.Implementations.Users.DefaultAuthenticationProvider",
            password_reset_provider_id: "Jellyfin.Server.Implementations.Users.DefaultPasswordResetProvider",
            sync_play_access: "None",
        },
    })
}

/// An image tag: what changes when the picture does (its URL's version).
pub fn tag_of(url: &str) -> String {
    let version = url.split_once("?v=").map(|(_, v)| v).unwrap_or(url);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in version.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SessionInfo {
    id: String,
    user_id: String,
    user_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    application_version: Option<String>,
    last_activity_date: String,
    last_playback_check_in: String,
    is_active: bool,
    supports_media_control: bool,
    supports_remote_control: bool,
    has_custom_device_name: bool,
    server_id: String,
    playable_media_types: [&'static str; 2],
    supported_commands: [(); 0],
    now_playing_queue: [(); 0],
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AuthenticationResult {
    user: UserDto,
    session_info: SessionInfo,
    access_token: String,
    server_id: String,
}

/// `POST /Users/AuthenticateByName {"Username", "Pw"}`: a token for other
/// apps, `play` only. An account with two-step sign-in can't sign in this
/// way: a client has nowhere to type the code.
async fn authenticate(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    SourceIp(source): SourceIp,
    ClientIp(ip): ClientIp,
    ClientOf(client): ClientOf,
    headers: HeaderMap,
    body: Body,
) -> JfResult<Response> {
    // JSON only, as Jellyfin takes it: another site's page can send text or
    // a form here without asking the server first, and its wrong passwords
    // would lock the account out at its owner's address.
    if !is_json(&headers) {
        return Err(JfError::BadRequest("the body must be JSON".into()));
    }
    // `Pw`; older clients sent `Password`.
    let username = body.str("Username").unwrap_or_default().to_owned();
    let password = body.str("Pw").or(body.str("Password")).unwrap_or_default().to_owned();
    let asked = compatibility::Asked {
        name: client.token_name(),
        scopes: compatibility::Scopes::play(),
        origins: compatibility::Origins::default(),
    };
    let account = crate::api::auth::check_password(&state, &username, password, source).await?;
    crate::api::auth::second_step(&state, account.id, &account.username, None, None).await?;
    let (token, _) = state.auth.create_compatibility_token(account.id, &asked).await?;
    tracing::info!("'{}' signed in to the Jellyfin-compatible API from {}", account.username, asked.name);
    // Its first use, from where it signed in: the account page shows it.
    let user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let used = UsedFrom { ip, user_agent };
    let user = state.auth.validate_compatibility(&token, Some(&used)).await.ok_or(JfError::Unauthorized)?;
    Ok(Json(signed_in(&state, &jellyfin, &user, &client, token).await?).into_response())
}

/// Whether the request's body is said to be JSON (`application/json`,
/// `text/json`, `application/…+json`).
fn is_json(headers: &HeaderMap) -> bool {
    let Some(kind) = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) else { return false };
    let kind = kind.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    kind == "application/json" || kind == "text/json" || (kind.starts_with("application/") && kind.ends_with("+json"))
}

async fn signed_in(
    state: &AppState,
    jellyfin: &Jellyfin,
    user: &AuthUser,
    client: &Client,
    token: String,
) -> JfResult<AuthenticationResult> {
    let dto = user_dto(state, jellyfin, user).await?;
    let user_id = dto.id.clone();
    let session_info = SessionInfo {
        id: jellyfin.ids.of(Kind::User, user.id),
        user_id,
        user_name: dto.name.clone(),
        client: client.client.clone(),
        device_id: client.device_id.clone(),
        device_name: client.device.clone(),
        application_version: client.version.clone(),
        last_activity_date: iso(crate::util::now()),
        last_playback_check_in: iso(0),
        is_active: true,
        supports_media_control: false,
        supports_remote_control: false,
        has_custom_device_name: false,
        server_id: jellyfin.server_id.clone(),
        playable_media_types: ["Audio", "Video"],
        supported_commands: [],
        now_playing_queue: [],
    };
    Ok(AuthenticationResult { user: dto, session_info, access_token: token, server_id: jellyfin.server_id.clone() })
}

async fn me(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
) -> JfResult<Response> {
    Ok(Json(user_dto(&state, &jellyfin, &user).await?).into_response())
}

/// `/Users`: the caller alone. Who else has an account is Pley's to show.
async fn users(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
) -> JfResult<Response> {
    Ok(Json([user_dto(&state, &jellyfin, &user).await?]).into_response())
}

/// `/Users/Public`: no one. Jellyfin can list its users on the sign-in
/// screen; Pley doesn't tell who has an account to someone not signed in.
async fn public_users() -> Json<[(); 0]> {
    Json([])
}

async fn user(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    JfUser { user, .. }: JfUser,
    Path(id): Path<String>,
) -> JfResult<Response> {
    if jellyfin.ids.number(Kind::User, &id) != Some(user.id) {
        return Err(JfError::NotFound);
    }
    Ok(Json(user_dto(&state, &jellyfin, &user).await?).into_response())
}

/// A user's picture, by its id: clients load it without a token, and a
/// user id can't be guessed ([`super::ids`]).
async fn avatar(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    Path(id): Path<String>,
) -> JfResult<Response> {
    avatar_of(&state, &jellyfin, &id).await
}

/// `/UserImage?userId=`, newer clients' way to the same picture.
async fn user_image(
    State(state): State<AppState>,
    Extension(jellyfin): Extension<Arc<Jellyfin>>,
    q: super::Q,
) -> JfResult<Response> {
    avatar_of(&state, &jellyfin, q.get("userId").ok_or(JfError::NotFound)?).await
}

async fn avatar_of(state: &AppState, jellyfin: &Jellyfin, id: &str) -> JfResult<Response> {
    let id = jellyfin.ids.number(Kind::User, id).ok_or(JfError::NotFound)?;
    let file: Option<Option<String>> = sqlx::query_scalar("SELECT avatar FROM users WHERE id = ? AND NOT disabled")
        .bind(id)
        .fetch_optional(&state.db.read)
        .await?;
    let file = file.flatten().ok_or(JfError::NotFound)?;
    let bytes = tokio::fs::read(crate::accounts::avatars_dir(&state.config.data_dir).join(file))
        .await
        .map_err(|_| JfError::NotFound)?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    Ok((headers, bytes).into_response())
}

/// `POST /Sessions/Logout`: the token it was sent with ends, wherever it was
/// sent (a header or `api_key`).
async fn logout(State(state): State<AppState>, JfUser { user, .. }: JfUser) -> JfResult<StatusCode> {
    if let Some(token) = user.compatibility {
        state.auth.end_compatibility_token(user.id, token.token_id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// What clients tell the server of themselves and their layouts, which
/// Pley doesn't keep.
async fn accepted(_user: JfUser) -> StatusCode {
    StatusCode::NO_CONTENT
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DisplayPreferences {
    id: String,
    sort_by: &'static str,
    remember_indexing: bool,
    primary_image_height: i64,
    primary_image_width: i64,
    custom_prefs: std::collections::HashMap<String, String>,
    scroll_direction: &'static str,
    show_backdrop: bool,
    remember_sorting: bool,
    sort_order: &'static str,
    show_sidebar: bool,
    client: String,
}

/// Layout preferences, Jellyfin's defaults.
async fn display_preferences(
    _user: JfUser,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<DisplayPreferences> {
    let client = query.iter().find(|(k, _)| k.eq_ignore_ascii_case("client")).map(|(_, v)| v.clone());
    Json(DisplayPreferences {
        id: id.chars().take(64).collect(),
        sort_by: "SortName",
        remember_indexing: false,
        primary_image_height: 250,
        primary_image_width: 250,
        custom_prefs: Default::default(),
        scroll_direction: "Horizontal",
        show_backdrop: true,
        remember_sorting: false,
        sort_order: "Ascending",
        show_sidebar: false,
        client: client.unwrap_or_else(|| "emby".into()).chars().take(64).collect(),
    })
}
