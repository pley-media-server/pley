//! User management (admins), profile self-service (avatars, PIN, devices) and
//! invitations.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::auth::{Me, can_set_up_two_step, confirm_password, login_response, me, second_step};
use super::nullable;
use crate::accounts::{self, NO_PASSWORD, UserDto};
use crate::auth::{
    AdminUser, AuthUser, ClientIp, CurrentToken, hash_password, hash_token, validate_password, validate_username,
};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::i18n;
use crate::ratings;
use crate::requests::{self, RequestAccess};
use crate::state::AppState;
use crate::streams::{self, SubtitleMode};
use crate::util::now;

fn check_age(max_age: Option<i64>) -> ApiResult<()> {
    match max_age {
        Some(age) if !(0..=21).contains(&age) => Err(AppError::bad_request("the age limit must be between 0 and 21")),
        _ => Ok(()),
    }
}

/// The rating system an age limit is shown in, as stored: a country with
/// ratings ("GB"), or "age".
fn check_system(system: Option<&str>) -> ApiResult<Option<String>> {
    match system.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) if s.eq_ignore_ascii_case(AGE_SYSTEM) => Ok(Some(AGE_SYSTEM.to_owned())),
        Some(s) => match ratings::system(s) {
            Some(system) => Ok(Some(system.country.to_owned())),
            None => Err(AppError::bad_request("unknown rating system")),
        },
    }
}

/// The rating system of plain ages.
const AGE_SYSTEM: &str = "age";

#[derive(Serialize)]
pub struct RatingSystems {
    /// The metadata region (ISO 3166-1 alpha-2): its system, when it has
    /// one, is where an age limit is picked first.
    region: String,
    systems: Vec<RatingSystem>,
}

#[derive(Serialize)]
struct RatingSystem {
    country: &'static str,
    name: &'static str,
    ratings: Vec<SystemRating>,
}

#[derive(Serialize)]
struct SystemRating {
    name: &'static str,
    age: i64,
    /// "film" or "tv"; `None`: both.
    kind: Option<&'static str>,
}

/// The rating systems an age limit is picked and shown in, each country's
/// ratings youngest first. Names are as short as the boards' own allow:
/// Denmark's "Tilladt for alle" is "A".
pub async fn rating_systems(State(state): State<AppState>, _user: AuthUser) -> ApiResult<Json<RatingSystems>> {
    let systems = ratings::SYSTEMS
        .iter()
        .map(|s| RatingSystem {
            country: s.country,
            name: s.name,
            ratings: s
                .ratings
                .iter()
                .map(|r| SystemRating {
                    name: r.short_name(),
                    age: r.age,
                    kind: r.kind.map(|k| match k {
                        ratings::Kind::Film => "film",
                        ratings::Kind::Tv => "tv",
                    }),
                })
                .collect(),
        })
        .collect();
    Ok(Json(RatingSystems { region: ratings::region(&state.db).await?, systems }))
}

async fn admins_left_after(state: &AppState, without: i64) -> ApiResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1 AND disabled = 0 AND id != ?")
        .bind(without)
        .fetch_one(&state.db.read)
        .await?)
}

// ------------------------------------------------------------------ admin: users

pub async fn list_users(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<UserDto>>> {
    Ok(Json(accounts::for_administrators(&state.db, accounts::users(&state.db).await?).await?))
}

/// A user as an administrator sees it: with their birthday.
async fn admin_view(state: &AppState, id: i64) -> ApiResult<UserDto> {
    let user = accounts::user(&state.db, id).await?.ok_or_else(|| AppError::not_found("user not found"))?;
    Ok(accounts::for_administrators(&state.db, vec![user]).await?.remove(0))
}

/// An age limit is fixed or follows a birthday, not both.
fn one_limit(max_age: bool, birth_month: bool) -> ApiResult<()> {
    if max_age && birth_month {
        return Err(AppError::bad_request("an age limit is fixed or follows a birthday, not both"));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct NewUser {
    username: String,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    is_admin: bool,
    /// A profile without a password (children, guests), reached by switching.
    #[serde(default)]
    managed: bool,
    #[serde(default = "yes")]
    home: bool,
    #[serde(default = "yes")]
    all_libraries: bool,
    #[serde(default)]
    library_ids: Vec<i64>,
    #[serde(default)]
    max_age: Option<i64>,
    #[serde(default)]
    max_age_system: Option<String>,
    /// "2015-03": the age limit is the profile's age (see
    /// `accounts::birthday_limit`), instead of `max_age`.
    #[serde(default)]
    birth_month: Option<String>,
    /// Labels whose titles it sees even above its age limit, and labels
    /// whose titles it never sees (see `catalog::VISIBLE`).
    #[serde(default)]
    show_labels: Vec<String>,
    #[serde(default)]
    hide_labels: Vec<String>,
    #[serde(default)]
    pin: Option<String>,
    /// Left out: profiles without a password and accounts with an age limit
    /// may not ask for titles, others ask an administrator.
    #[serde(default)]
    request_access: Option<RequestAccess>,
}

fn yes() -> bool {
    true
}

pub async fn create_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<NewUser>,
) -> ApiResult<(StatusCode, Json<UserDto>)> {
    let username = body.username.trim().to_owned();
    validate_username(&username)?;
    check_age(body.max_age)?;
    let max_age_system = check_system(body.max_age_system.as_deref())?;
    one_limit(body.max_age.is_some(), body.birth_month.is_some())?;
    let show_labels = accounts::check_label_rules(&body.show_labels)?;
    let hide_labels = accounts::check_label_rules(&body.hide_labels)?;
    accounts::check_rules_apart(&show_labels, &hide_labels)?;
    let birth_month = match body.birth_month.as_deref() {
        Some(born) => Some(accounts::check_birth_month(born, &accounts::local_month(&state.db).await?)?),
        None => None,
    };
    let max_age = match &birth_month {
        Some(born) => accounts::birthday_limit(born, &accounts::local_month(&state.db).await?),
        None => body.max_age,
    };
    if body.managed && body.is_admin {
        return Err(AppError::bad_request("a managed profile can't be an administrator"));
    }
    let password_hash = match (&body.password, body.managed) {
        (_, true) => NO_PASSWORD.to_owned(),
        (Some(password), false) => {
            validate_password(password, &username)?;
            hash_password(password.clone()).await?
        }
        (None, false) => return Err(AppError::bad_request("a password is required (or create a managed profile)")),
    };
    let pin_hash = match body.pin.as_deref().filter(|p| !p.is_empty()) {
        Some(pin) => {
            accounts::validate_pin(pin)?;
            Some(hash_password(pin.to_owned()).await?)
        }
        None => None,
    };
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(&state.db.read)
        .await?;
    if exists.is_some() {
        return Err(AppError::conflict(format!("user '{username}' already exists")));
    }
    let request_access = body.request_access.unwrap_or(if body.managed || max_age.is_some() {
        RequestAccess::Off
    } else {
        RequestAccess::Ask
    });
    // The rules are made with the profile: it never exists without them.
    let mut tx = state.db.write.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (id, username, password_hash, display_name, is_admin, managed, home, all_libraries, max_age,
                            max_age_system, birth_month, pin_hash, request_access, created_at)
         VALUES ((SELECT last + 1 FROM user_ids), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&username)
    .bind(password_hash)
    .bind(body.display_name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()))
    .bind(body.is_admin)
    .bind(body.managed)
    .bind(body.home)
    .bind(body.all_libraries)
    .bind(max_age)
    .bind(max_age_system)
    .bind(birth_month)
    .bind(pin_hash)
    .bind(request_access.as_str())
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    accounts::add_label_rules(&mut tx, id, "show", &show_labels).await?;
    accounts::add_label_rules(&mut tx, id, "hide", &hide_labels).await?;
    tx.commit().await?;
    if !body.all_libraries {
        accounts::set_libraries(&state.db, id, &body.library_ids).await?;
    }
    Ok((StatusCode::CREATED, Json(admin_view(&state, id).await?)))
}

#[derive(Deserialize)]
pub struct UserPatch {
    #[serde(default)]
    password: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    display_name: Option<Option<String>>,
    #[serde(default)]
    is_admin: Option<bool>,
    #[serde(default)]
    home: Option<bool>,
    #[serde(default)]
    disabled: Option<bool>,
    #[serde(default)]
    all_libraries: Option<bool>,
    #[serde(default)]
    library_ids: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "nullable")]
    max_age: Option<Option<i64>>,
    /// `null`: the metadata region's.
    #[serde(default, deserialize_with = "nullable")]
    max_age_system: Option<Option<String>>,
    /// "2015-03": the age limit follows the profile's age from now on;
    /// `null` stops it following (the limit stays as it is). A `max_age`
    /// stops it too.
    #[serde(default, deserialize_with = "nullable")]
    birth_month: Option<Option<String>>,
    /// Replace the profile's label rules (see [`NewUser`]).
    #[serde(default)]
    show_labels: Option<Vec<String>>,
    #[serde(default)]
    hide_labels: Option<Vec<String>>,
    /// `null` removes the PIN.
    #[serde(default, deserialize_with = "nullable")]
    pin: Option<Option<String>>,
    #[serde(default)]
    request_access: Option<RequestAccess>,
}

pub async fn update_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<UserPatch>,
) -> ApiResult<Json<UserDto>> {
    let current = accounts::user(&state.db, id).await?.ok_or_else(|| AppError::not_found("user not found"))?;
    // Check everything first: a refused update changes nothing.
    if let Some(password) = &body.password {
        // One's own, from Account, which asks for the current one first and
        // signs one's other devices out: a session alone isn't enough.
        if id == admin.id {
            return Err(AppError::bad_request("change your own password in Account"));
        }
        validate_password(password, &current.username)?;
    }
    if let Some(is_admin) = body.is_admin {
        if !is_admin && current.is_admin && admins_left_after(&state, id).await? == 0 {
            return Err(AppError::bad_request("Pley needs at least one administrator"));
        }
        if is_admin && current.managed {
            return Err(AppError::bad_request("a managed profile can't be an administrator"));
        }
    }
    if body.disabled == Some(true) {
        if id == admin.id {
            return Err(AppError::bad_request("you can't disable your own account"));
        }
        if current.is_admin && admins_left_after(&state, id).await? == 0 {
            return Err(AppError::bad_request("Pley needs at least one administrator"));
        }
    }
    if let Some(max_age) = body.max_age {
        check_age(max_age)?;
    }
    let max_age_system = match &body.max_age_system {
        Some(system) => Some(check_system(system.as_deref())?),
        None => None,
    };
    one_limit(body.max_age.is_some(), matches!(body.birth_month, Some(Some(_))))?;
    let month = accounts::local_month(&state.db).await?;
    let birth_month = match &body.birth_month {
        Some(Some(born)) => Some(Some(accounts::check_birth_month(born, &month)?)),
        Some(None) => Some(None),
        // A fixed limit (or none) replaces one that follows a birthday.
        None => body.max_age.is_some().then_some(None),
    };
    let max_age = match &birth_month {
        Some(Some(born)) => Some(accounts::birthday_limit(born, &month)),
        _ => body.max_age,
    };
    if body.pin.is_some() && id == admin.id {
        // As the password: from Account, which asks for the password. A PIN
        // switches a profile to this one without its two-step code.
        return Err(AppError::bad_request("change your own PIN in Account"));
    }
    if let Some(Some(pin)) = body.pin.as_ref().map(|p| p.as_deref().filter(|p| !p.is_empty())) {
        accounts::validate_pin(pin)?;
    }
    let show_labels = body.show_labels.as_deref().map(accounts::check_label_rules).transpose()?;
    let hide_labels = body.hide_labels.as_deref().map(accounts::check_label_rules).transpose()?;
    if show_labels.is_some() || hide_labels.is_some() {
        // A list left out stays as it is, and is checked against the other.
        let show = match &show_labels {
            Some(show) => show.clone(),
            None => accounts::label_rules(&state.db, id, "show").await?,
        };
        let hide = match &hide_labels {
            Some(hide) => hide.clone(),
            None => accounts::label_rules(&state.db, id, "hide").await?,
        };
        accounts::check_rules_apart(&show, &hide)?;
    }

    // What the user may see changes: their devices are told to fetch it again.
    let sees_otherwise = body.is_admin.is_some()
        || body.all_libraries.is_some()
        || body.library_ids.is_some()
        || max_age.is_some()
        || show_labels.is_some()
        || hide_labels.is_some();
    if let Some(password) = body.password {
        let hash = hash_password(password).await?;
        sqlx::query("UPDATE users SET password_hash = ?, managed = 0 WHERE id = ?")
            .bind(hash)
            .bind(id)
            .execute(&state.db.write)
            .await?;
        // Sign the user out everywhere else, and every app using the account
        // or allowed by it (an administrator's own included), with the
        // sign-ins of apps in progress: codes not exchanged yet, and TVs
        // allowed that haven't asked for their answer yet. Their tokens for
        // other apps end too.
        if id != admin.id {
            sqlx::query("DELETE FROM sessions WHERE user_id = ?").bind(id).execute(&state.db.write).await?;
        }
        state.oauth.forget_user(id);
        state.auth.end_apps(id).await?;
        state.auth.end_compatibility_tokens(id).await?;
    }
    if let Some(name) = body.display_name {
        let name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(is_admin) = body.is_admin {
        sqlx::query("UPDATE users SET is_admin = ? WHERE id = ?")
            .bind(is_admin)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(home) = body.home {
        sqlx::query("UPDATE users SET home = ? WHERE id = ?").bind(home).bind(id).execute(&state.db.write).await?;
    }
    if let Some(disabled) = body.disabled {
        sqlx::query("UPDATE users SET disabled = ? WHERE id = ?")
            .bind(disabled)
            .bind(id)
            .execute(&state.db.write)
            .await?;
        // Apps it was signing in (a code not exchanged yet, a TV allowed
        // that hasn't asked for its answer yet) get nothing, rather than
        // signed in should the account be enabled again.
        if disabled {
            state.oauth.forget_user(id);
        }
    }
    if let Some(all) = body.all_libraries {
        sqlx::query("UPDATE users SET all_libraries = ? WHERE id = ?")
            .bind(all)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(ids) = &body.library_ids {
        accounts::set_libraries(&state.db, id, ids).await?;
    }
    if let Some(labels) = &show_labels {
        accounts::set_label_rules(&state.db, id, "show", labels).await?;
    }
    if let Some(labels) = &hide_labels {
        accounts::set_label_rules(&state.db, id, "hide", labels).await?;
    }
    if let Some(born) = &birth_month {
        sqlx::query("UPDATE users SET birth_month = ? WHERE id = ?")
            .bind(born)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(max_age) = max_age {
        sqlx::query("UPDATE users SET max_age = ? WHERE id = ?")
            .bind(max_age)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(system) = max_age_system {
        sqlx::query("UPDATE users SET max_age_system = ? WHERE id = ?")
            .bind(system)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(pin) = body.pin {
        let hash = match pin.filter(|p| !p.is_empty()) {
            Some(pin) => Some(hash_password(pin).await?),
            None => None,
        };
        sqlx::query("UPDATE users SET pin_hash = ?, pin_failures = 0 WHERE id = ?")
            .bind(hash)
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(access) = body.request_access {
        sqlx::query("UPDATE users SET request_access = ? WHERE id = ?")
            .bind(access.as_str())
            .bind(id)
            .execute(&state.db.write)
            .await?;
    } else if current.max_age.is_none() && matches!(max_age, Some(Some(_))) {
        // An age limit added: the account no longer asks until an
        // administrator lets it, as those made with one.
        sqlx::query("UPDATE users SET request_access = 'off' WHERE id = ? AND request_access = 'ask'")
            .bind(id)
            .execute(&state.db.write)
            .await?;
    }
    state.auth.forget_user(id);
    if sees_otherwise {
        state.events.send(ServerEvent::OwnLibrariesChanged { user_id: id });
    }
    // What they may ask for, and what Discover shows them, changed: their
    // summary is fetched again.
    if body.request_access.is_some() || sees_otherwise {
        state.events.send(ServerEvent::RequestsSettingsChanged { user_id: Some(id) });
    }
    Ok(Json(admin_view(&state, id).await?))
}

pub async fn delete_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if id == admin.id {
        return Err(AppError::bad_request("you cannot delete your own account"));
    }
    let avatar: Option<Option<String>> =
        sqlx::query_scalar("SELECT avatar FROM users WHERE id = ?").bind(id).fetch_optional(&state.db.read).await?;
    let mut tx = state.db.write.begin().await?;
    // Requests others follow stay, with a new maker.
    let told = requests::hand_down(&mut tx, id).await?;
    let result = sqlx::query("DELETE FROM users WHERE id = ?").bind(id).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("user not found"));
    }
    tx.commit().await?;
    if let Some(to) = told {
        state.requests.account_deleted(to);
    }
    if let Some(Some(file)) = avatar {
        let _ = tokio::fs::remove_file(accounts::avatars_dir(&state.config.data_dir).join(file)).await;
    }
    state.auth.forget_user(id);
    // An app it was signing in gets nothing.
    state.oauth.forget_user(id);
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ devices

#[derive(Serialize, sqlx::FromRow)]
struct SessionRow {
    token_hash: String,
    created_at: i64,
    last_seen_at: i64,
    user_agent: Option<String>,
    last_ip: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceDto {
    /// Stable handle of the session (not the token).
    id: String,
    name: String,
    user_agent: Option<String>,
    ip: Option<String>,
    created_at: i64,
    last_seen_at: i64,
    current: bool,
}

async fn devices_of(state: &AppState, user_id: i64, current: Option<&str>) -> ApiResult<Vec<DeviceDto>> {
    let rows: Vec<SessionRow> = sqlx::query_as(
        "SELECT token_hash, created_at, last_seen_at, user_agent, last_ip FROM sessions
         WHERE user_id = ? AND expires_at > ? ORDER BY last_seen_at DESC",
    )
    .bind(user_id)
    .bind(now())
    .fetch_all(&state.db.read)
    .await?;
    let current = current.map(hash_token);
    Ok(rows
        .into_iter()
        .map(|r| DeviceDto {
            id: r.token_hash[..16].to_owned(),
            name: accounts::device_name(r.user_agent.as_deref()),
            current: current.as_deref() == Some(r.token_hash.as_str()),
            user_agent: r.user_agent,
            ip: r.last_ip,
            created_at: r.created_at,
            last_seen_at: r.last_seen_at,
        })
        .collect())
}

pub async fn my_devices(
    State(state): State<AppState>,
    user: AuthUser,
    CurrentToken(token): CurrentToken,
) -> ApiResult<Json<Vec<DeviceDto>>> {
    Ok(Json(devices_of(&state, user.id, token.as_deref()).await?))
}

pub async fn sign_out_device(
    State(state): State<AppState>,
    user: AuthUser,
    Path(device): Path<String>,
) -> ApiResult<StatusCode> {
    if device.len() < 16 || !device.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::not_found("device not found"));
    }
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND substr(token_hash, 1, 16) = ?")
        .bind(user.id)
        .bind(&device[..16])
        .execute(&state.db.write)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("device not found"));
    }
    state.auth.forget_user(user.id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn user_devices(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<DeviceDto>>> {
    Ok(Json(devices_of(&state, id, None).await?))
}

/// Sign a user out on every device and in every app, apps they were signing
/// in included (codes not exchanged yet, TVs allowed that haven't asked for
/// their answer yet).
pub async fn sign_out_everywhere(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if id == admin.id {
        return Err(AppError::bad_request("sign yourself out from your account page instead"));
    }
    state.oauth.forget_user(id);
    state.auth.sign_out_everywhere(id, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ own profile

#[derive(Deserialize)]
pub struct ProfilePatch {
    #[serde(default, deserialize_with = "nullable")]
    display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    avatar_color: Option<Option<String>>,
    /// ISO 639 code; `null`: the file's default track.
    #[serde(default, deserialize_with = "nullable")]
    audio_language: Option<Option<String>>,
    /// ISO 639 code; `null`: the audio language.
    #[serde(default, deserialize_with = "nullable")]
    subtitle_language: Option<Option<String>>,
    #[serde(default)]
    subtitle_mode: Option<SubtitleMode>,
    /// One of `crate::i18n::LANGUAGES`; `null`: the server's.
    #[serde(default, deserialize_with = "nullable")]
    language: Option<Option<String>>,
}

pub async fn update_me(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ProfilePatch>,
) -> ApiResult<Json<UserDto>> {
    if let Some(name) = body.display_name {
        let name = name.map(|n| n.trim().chars().take(64).collect::<String>()).filter(|n| !n.is_empty());
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(name)
            .bind(user.id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(color) = body.avatar_color {
        if let Some(c) = &color {
            accounts::validate_color(c)?;
        }
        sqlx::query("UPDATE users SET avatar_color = ? WHERE id = ?")
            .bind(color)
            .bind(user.id)
            .execute(&state.db.write)
            .await?;
    }
    for (column, value) in [("audio_language", body.audio_language), ("subtitle_language", body.subtitle_language)] {
        let Some(value) = value else { continue };
        check_language(column, value.as_deref())?;
        sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE users SET {column} = ? WHERE id = ?")))
            .bind(value)
            .bind(user.id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(mode) = body.subtitle_mode {
        sqlx::query("UPDATE users SET subtitle_mode = ? WHERE id = ?")
            .bind(mode.as_str())
            .bind(user.id)
            .execute(&state.db.write)
            .await?;
    }
    if let Some(language) = body.language {
        save_language(&state, user.id, language).await?;
    }
    Ok(Json(accounts::user(&state.db, user.id).await?.ok_or(AppError::Unauthorized)?))
}

/// A track default's language: an ISO 639 code, or `None`.
fn check_language(field: &str, code: Option<&str>) -> ApiResult<()> {
    match code {
        Some(code) if !streams::valid_language(code) => {
            Err(AppError::bad_request(format!("{field} must be an ISO 639 code such as \"eng\"")))
        }
        _ => Ok(()),
    }
}

/// The tracks titles start with, all three at once: a field left out takes
/// its default, as `null` does for the languages.
#[derive(Deserialize)]
pub struct TrackDefaults {
    /// ISO 639 code; `null`: the file's default track.
    #[serde(default)]
    audio_language: Option<String>,
    /// ISO 639 code; `null`: the audio language.
    #[serde(default)]
    subtitle_language: Option<String>,
    /// `off`, `forced` (the default), `foreign` or `always`.
    #[serde(default)]
    subtitle_mode: Option<String>,
}

/// The language the profile sees Pley in: one of `i18n::LANGUAGES`, or
/// `None` for the server's.
async fn save_language(state: &AppState, user_id: i64, language: Option<String>) -> ApiResult<()> {
    if language.as_deref().is_some_and(|code| !i18n::LANGUAGES.contains(&code)) {
        return Err(AppError::bad_request(format!("language is one of {}, or null", i18n::LANGUAGES.join(", "))));
    }
    sqlx::query("UPDATE users SET language = ? WHERE id = ?")
        .bind(language)
        .bind(user_id)
        .execute(&state.db.write)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct LanguageChoice {
    /// `null` or left out: the server's.
    #[serde(default)]
    language: Option<String>,
}

/// `PUT /api/me/language`: the language the profile sees Pley in, for apps
/// allowed `play` as for the web (which also has `PATCH /api/me`). The
/// answer is the profile as `GET /api/auth/me` shows it.
pub async fn set_language(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<LanguageChoice>,
) -> ApiResult<Json<Me>> {
    save_language(&state, user.id, body.language).await?;
    me(State(state), user).await
}

/// `PUT /api/me/track-defaults`: the profile's audio and subtitle defaults,
/// for apps allowed `play` as for the web (which also has `PATCH /api/me`).
/// The answer is the profile as `GET /api/auth/me` shows it.
pub async fn set_track_defaults(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<TrackDefaults>,
) -> ApiResult<Json<Me>> {
    check_language("audio_language", body.audio_language.as_deref())?;
    check_language("subtitle_language", body.subtitle_language.as_deref())?;
    let mode = match body.subtitle_mode.as_deref() {
        None => SubtitleMode::default(),
        Some(name) => [SubtitleMode::Off, SubtitleMode::Forced, SubtitleMode::Foreign, SubtitleMode::Always]
            .into_iter()
            .find(|mode| mode.as_str() == name)
            .ok_or_else(|| AppError::bad_request("subtitle_mode is off, forced, foreign or always"))?,
    };
    sqlx::query("UPDATE users SET audio_language = ?, subtitle_language = ?, subtitle_mode = ? WHERE id = ?")
        .bind(body.audio_language)
        .bind(body.subtitle_language)
        .bind(mode.as_str())
        .bind(user.id)
        .execute(&state.db.write)
        .await?;
    me(State(state), user).await
}

#[derive(Deserialize)]
pub struct PinChange {
    /// `null` removes the PIN.
    pin: Option<String>,
    /// Your password (not needed for managed profiles, which have none).
    #[serde(default)]
    password: Option<String>,
}

pub async fn set_my_pin(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<PinChange>,
) -> ApiResult<StatusCode> {
    let managed: bool =
        sqlx::query_scalar("SELECT managed FROM users WHERE id = ?").bind(user.id).fetch_one(&state.db.read).await?;
    if managed {
        return Err(AppError::forbidden("ask an administrator to change this profile's PIN"));
    }
    confirm_password(&state, &user, body.password).await?;
    let hash = match body.pin.filter(|p| !p.is_empty()) {
        Some(pin) => {
            accounts::validate_pin(&pin)?;
            Some(hash_password(pin).await?)
        }
        None => None,
    };
    sqlx::query("UPDATE users SET pin_hash = ?, pin_failures = 0 WHERE id = ?")
        .bind(hash)
        .bind(user.id)
        .execute(&state.db.write)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

const AVATAR_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Upload a profile picture (any common image format, raw request body).
pub async fn upload_avatar(State(state): State<AppState>, user: AuthUser, body: Bytes) -> ApiResult<Json<UserDto>> {
    if body.is_empty() || body.len() > AVATAR_MAX_BYTES {
        return Err(AppError::bad_request("pictures must be smaller than 10 MB"));
    }
    let _decoding = crate::images::UPLOAD_DECODES.acquire().await.map_err(|e| AppError::Internal(e.into()))?;
    let jpeg = tokio::task::spawn_blocking(move || accounts::process_avatar(&body))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
    accounts::set_avatar(&state.db, &state.config.data_dir, user.id, jpeg).await?;
    Ok(Json(accounts::user(&state.db, user.id).await?.ok_or(AppError::Unauthorized)?))
}

pub async fn delete_avatar(State(state): State<AppState>, user: AuthUser) -> ApiResult<StatusCode> {
    let old: Option<String> =
        sqlx::query_scalar("SELECT avatar FROM users WHERE id = ?").bind(user.id).fetch_one(&state.db.read).await?;
    sqlx::query("UPDATE users SET avatar = NULL WHERE id = ?").bind(user.id).execute(&state.db.write).await?;
    if let Some(old) = old {
        let _ = tokio::fs::remove_file(accounts::avatars_dir(&state.config.data_dir).join(old)).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn avatar(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> ApiResult<Response> {
    // Apps see their own profile's picture, those of its household, and
    // those of the people named in the requests they may read.
    if user.app.is_some() && id != user.id {
        let household: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users me, users them
                            WHERE me.id = ? AND them.id = ? AND me.home AND them.home AND NOT them.disabled)",
        )
        .bind(user.id)
        .bind(id)
        .fetch_one(&state.db.read)
        .await?;
        if !household && !state.requests.names(&user, id).await? {
            return Err(AppError::not_found("no picture"));
        }
    }
    let file: Option<Option<String>> =
        sqlx::query_scalar("SELECT avatar FROM users WHERE id = ?").bind(id).fetch_optional(&state.db.read).await?;
    let file = file.flatten().ok_or_else(|| AppError::not_found("no picture"))?;
    let bytes = tokio::fs::read(accounts::avatars_dir(&state.config.data_dir).join(file))
        .await
        .map_err(|_| AppError::not_found("no picture"))?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    Ok((headers, bytes).into_response())
}

// ------------------------------------------------------------------ invites

#[derive(sqlx::FromRow)]
struct InviteRow {
    code: String,
    created_at: i64,
    expires_at: Option<i64>,
    uses_left: i64,
    all_libraries: bool,
    library_ids: String,
    max_age: Option<i64>,
    max_age_system: Option<String>,
    show_labels: String,
    hide_labels: String,
    note: Option<String>,
}

#[derive(Serialize)]
pub struct InviteDto {
    code: String,
    created_at: i64,
    expires_at: Option<i64>,
    uses_left: i64,
    all_libraries: bool,
    library_ids: Vec<i64>,
    max_age: Option<i64>,
    max_age_system: Option<String>,
    show_labels: Vec<String>,
    hide_labels: Vec<String>,
    note: Option<String>,
}

impl From<InviteRow> for InviteDto {
    fn from(r: InviteRow) -> Self {
        InviteDto {
            library_ids: serde_json::from_str(&r.library_ids).unwrap_or_default(),
            show_labels: serde_json::from_str(&r.show_labels).unwrap_or_default(),
            hide_labels: serde_json::from_str(&r.hide_labels).unwrap_or_default(),
            code: r.code,
            created_at: r.created_at,
            expires_at: r.expires_at,
            uses_left: r.uses_left,
            all_libraries: r.all_libraries,
            max_age: r.max_age,
            max_age_system: r.max_age_system,
            note: r.note,
        }
    }
}

const INVITE_COLUMNS: &str =
    "code, created_at, expires_at, uses_left, all_libraries, library_ids, max_age, max_age_system, show_labels,
     hide_labels, note";

pub async fn list_invites(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<InviteDto>>> {
    let rows: Vec<InviteRow> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {INVITE_COLUMNS} FROM invites ORDER BY created_at DESC")))
            .fetch_all(&state.db.read)
            .await?;
    Ok(Json(rows.into_iter().map(InviteDto::from).collect()))
}

#[derive(Deserialize)]
pub struct NewInvite {
    /// Days until the link stops working (default 7, 0 = never).
    #[serde(default)]
    expires_in_days: Option<i64>,
    #[serde(default)]
    uses: Option<i64>,
    #[serde(default = "yes")]
    all_libraries: bool,
    #[serde(default)]
    library_ids: Vec<i64>,
    #[serde(default)]
    max_age: Option<i64>,
    #[serde(default)]
    max_age_system: Option<String>,
    #[serde(default)]
    show_labels: Vec<String>,
    #[serde(default)]
    hide_labels: Vec<String>,
    #[serde(default)]
    note: Option<String>,
}

pub async fn create_invite(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(body): Json<NewInvite>,
) -> ApiResult<(StatusCode, Json<InviteDto>)> {
    check_age(body.max_age)?;
    let max_age_system = check_system(body.max_age_system.as_deref())?;
    let show_labels = accounts::check_label_rules(&body.show_labels)?;
    let hide_labels = accounts::check_label_rules(&body.hide_labels)?;
    accounts::check_rules_apart(&show_labels, &hide_labels)?;
    let days = body.expires_in_days.unwrap_or(7);
    let uses = body.uses.unwrap_or(1).clamp(1, 1000);
    let code = accounts::invite_code();
    let created_at = now();
    let expires_at = (days > 0).then(|| created_at + days * 86_400);
    let note = body.note.map(|n| n.trim().chars().take(200).collect::<String>()).filter(|n| !n.is_empty());
    sqlx::query(
        "INSERT INTO invites (code, created_by, created_at, expires_at, uses_left, all_libraries, library_ids, max_age,
                              max_age_system, show_labels, hide_labels, note)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&code)
    .bind(admin.id)
    .bind(created_at)
    .bind(expires_at)
    .bind(uses)
    .bind(body.all_libraries)
    .bind(serde_json::to_string(&body.library_ids).unwrap_or_else(|_| "[]".into()))
    .bind(body.max_age)
    .bind(&max_age_system)
    .bind(serde_json::to_string(&show_labels).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&hide_labels).unwrap_or_else(|_| "[]".into()))
    .bind(&note)
    .execute(&state.db.write)
    .await?;
    let invite = InviteDto {
        code,
        created_at,
        expires_at,
        uses_left: uses,
        all_libraries: body.all_libraries,
        library_ids: body.library_ids,
        max_age: body.max_age,
        max_age_system,
        show_labels,
        hide_labels,
        note,
    };
    Ok((StatusCode::CREATED, Json(invite)))
}

pub async fn delete_invite(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(code): Path<String>,
) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM invites WHERE code = ?").bind(code).execute(&state.db.write).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn valid_invite(state: &AppState, code: &str) -> ApiResult<InviteDto> {
    let invite: Option<InviteRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {INVITE_COLUMNS} FROM invites WHERE code = ? AND uses_left > 0 AND (expires_at IS NULL OR expires_at > ?)"
    )))
    .bind(code)
    .bind(now())
    .fetch_optional(&state.db.read)
    .await?;
    invite.map(InviteDto::from).ok_or_else(|| AppError::not_found("this invitation is invalid or has expired"))
}

/// Public: is this invitation still valid? (Shows the server name on the sign-up page.)
pub async fn check_invite(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invite = valid_invite(&state, &code).await?;
    Ok(Json(serde_json::json!({
        "server_name": state.settings.get().server_name,
        "expires_at": invite.expires_at,
    })))
}

#[derive(Deserialize)]
pub struct AcceptInvite {
    username: String,
    password: String,
    #[serde(default)]
    display_name: Option<String>,
}

/// Public: create an account with an invitation and sign in.
pub async fn accept_invite(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: HeaderMap,
    ClientIp(ip): ClientIp,
    Json(body): Json<AcceptInvite>,
) -> ApiResult<Response> {
    let username = body.username.trim().to_owned();
    validate_username(&username)?;
    validate_password(&body.password, &username)?;
    let invite = valid_invite(&state, &code).await?;
    if state.auth.two_step_required() {
        // No account that couldn't sign in: the invitation stays for later.
        can_set_up_two_step(&state)?;
    }
    let hash = hash_password(body.password).await?;
    let mut tx = state.db.write.begin().await?;
    // Claim a use first so concurrent sign-ups can't overdraw the invitation.
    let claimed = sqlx::query(
        "UPDATE invites SET uses_left = uses_left - 1
         WHERE code = ? AND uses_left > 0 AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(&code)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    if claimed.rows_affected() == 0 {
        return Err(AppError::not_found("this invitation is invalid or has expired"));
    }
    let taken: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = ?").bind(&username).fetch_optional(&mut *tx).await?;
    if taken.is_some() {
        return Err(AppError::conflict(format!("the username '{username}' is taken")));
    }
    // Invited people are friends of the server, not members of the home.
    // With an age limit, they don't ask for titles until an administrator
    // lets them (as accounts made with one).
    let request_access = if invite.max_age.is_some() { RequestAccess::Off } else { RequestAccess::Ask };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (id, username, password_hash, display_name, home, all_libraries, max_age, max_age_system,
                            request_access, created_at)
         VALUES ((SELECT last + 1 FROM user_ids), ?, ?, ?, 0, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&username)
    .bind(hash)
    .bind(body.display_name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()))
    .bind(invite.all_libraries)
    .bind(invite.max_age)
    .bind(&invite.max_age_system)
    .bind(request_access.as_str())
    .bind(now())
    .fetch_one(&mut *tx)
    .await?;
    // Made with the account: it never signs in without them.
    accounts::add_label_rules(&mut tx, id, "show", &invite.show_labels).await?;
    accounts::add_label_rules(&mut tx, id, "hide", &invite.hide_labels).await?;
    tx.commit().await?;
    if !invite.all_libraries {
        accounts::set_libraries(&state.db, id, &invite.library_ids).await?;
    }
    tracing::info!("'{username}' joined with an invitation");
    // A server that requires two-step sign-in answers as a sign-in that sets
    // it up does: the account is made, and the person goes on signing in to
    // it with its first code.
    second_step(&state, id, &username, None, None).await?;
    login_response(&state, id, &headers, ip.as_deref(), None).await
}
