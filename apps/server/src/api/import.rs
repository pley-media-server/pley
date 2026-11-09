//! `/api/import/plex`: moving from Plex (see `crate::import`), for
//! administrators only. Apps never get these routes.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AdminUser;
use crate::error::{ApiResult, AppError};
use crate::import::job::{Choices, ImportError, Plan, Progress, ServerChoice};
use crate::import::plex::PlexError;
use crate::state::AppState;

impl From<ImportError> for AppError {
    fn from(e: ImportError) -> Self {
        match e {
            ImportError::NotSignedIn => AppError::ConflictAs("plex_signed_out", e.to_string()),
            ImportError::NoServer => AppError::ConflictAs("plex_no_server", e.to_string()),
            ImportError::PinExpired => AppError::ConflictAs("plex_pin_expired", e.to_string()),
            ImportError::Running => AppError::ConflictAs("import_running", e.to_string()),
            ImportError::NotOwned => AppError::ConflictAs("plex_not_owned", e.to_string()),
            ImportError::Cancelled | ImportError::NoAccess => AppError::ConflictAs("import_stopped", e.to_string()),
            ImportError::Invalid(message) => AppError::BadRequest(message),
            ImportError::Plex(PlexError::Refused(_)) => AppError::ConflictAs("plex_refused", e.to_string()),
            ImportError::Plex(PlexError::NeedsPin) => AppError::ConflictAs("plex_pin_needed", e.to_string()),
            ImportError::Plex(PlexError::Unreachable(message)) => AppError::Unavailable(message),
            ImportError::Db(e) => AppError::Internal(e.into()),
        }
    }
}

#[derive(Deserialize)]
pub struct StartSignIn {
    /// Where Plex's page goes once the person has allowed Pley: a page of
    /// the web app.
    #[serde(default)]
    forward_url: Option<String>,
}

/// `POST /api/import/plex/pin`: starts Plex's sign-in.
pub async fn start_sign_in(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<StartSignIn>,
) -> ApiResult<Json<Value>> {
    let forward = body.forward_url.as_deref().map(str::trim).filter(|u| !u.is_empty());
    if let Some(forward) = forward {
        let ok =
            url::Url::parse(forward).is_ok_and(|u| matches!(u.scheme(), "http" | "https")) && forward.len() <= 2048;
        if !ok {
            return Err(AppError::bad_request("forward_url must be an http or https address"));
        }
    }
    let (code, auth_url) = state.plex_import.start_sign_in(&state.db, forward).await?;
    Ok(Json(json!({ "code": code, "auth_url": auth_url })))
}

/// `GET /api/import/plex/pin`: whether the person has allowed Pley yet.
pub async fn sign_in_status(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Value>> {
    let signed_in = state.plex_import.signed_in(&state.db).await?;
    Ok(Json(json!({ "signed_in": signed_in })))
}

#[derive(Deserialize)]
pub struct PastedToken {
    token: String,
}

/// `POST /api/import/plex/token`: signs in with a Plex token instead.
pub async fn use_token(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<PastedToken>,
) -> ApiResult<StatusCode> {
    state.plex_import.use_token(&state.db, &body.token).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/import/plex/servers`: the account's Plex servers.
pub async fn servers(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<ServerChoice>>> {
    Ok(Json(state.plex_import.servers(&state.db).await?))
}

#[derive(Deserialize)]
pub struct ChosenServer {
    #[serde(default)]
    id: Option<String>,
    /// An address, for a server plex.tv doesn't list or can't reach.
    #[serde(default)]
    address: Option<String>,
}

/// `POST /api/import/plex/server`: connects to the server to import from.
pub async fn choose_server(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<ChosenServer>,
) -> ApiResult<Json<Value>> {
    let address = body.address.as_deref().map(str::trim).filter(|a| !a.is_empty());
    if address.is_some_and(|a| a.len() > 2048) {
        return Err(AppError::bad_request("that address is too long"));
    }
    let name = state.plex_import.choose_server(&state.db, body.id.as_deref(), address).await?;
    Ok(Json(json!({ "name": name })))
}

/// `GET /api/import/plex/plan`: Plex's libraries paired with Pley's, and
/// its Home users with the limits they'd get.
pub async fn plan(State(state): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Plan>> {
    Ok(Json(state.plex_import.plan(&state.db, &state.config.data_dir).await?))
}

/// `GET /api/import/plex/users/{id}/picture`: a Home user's picture, as
/// the last plan found it.
pub async fn user_picture(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let jpeg = state.plex_import.user_picture(id).await.ok_or_else(|| AppError::not_found("no picture"))?;
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")),
        (header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache")),
    ];
    Ok((headers, jpeg).into_response())
}

/// `POST /api/import/plex/start`: starts the import.
pub async fn start(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(choices): Json<Choices>,
) -> ApiResult<StatusCode> {
    check(&choices)?;
    state.plex_import.start(&state, admin.id, choices).await?;
    Ok(StatusCode::ACCEPTED)
}

/// Most libraries, folders and people one import takes.
const MAX_CHOICES: usize = 200;

fn check(choices: &Choices) -> ApiResult<()> {
    if choices.libraries.len() > MAX_CHOICES || choices.folders.len() > MAX_CHOICES || choices.users.len() > MAX_CHOICES
    {
        return Err(AppError::bad_request("too many choices"));
    }
    for folder in &choices.folders {
        if folder.plex.trim().is_empty() || folder.plex.len() > 4096 || folder.pley.len() > 4096 {
            return Err(AppError::bad_request("a folder must be 1-4096 characters"));
        }
    }
    for user in &choices.users {
        if user.pin.as_ref().is_some_and(|p| p.len() > 16 || !p.chars().all(|c| c.is_ascii_digit())) {
            return Err(AppError::bad_request("a Plex PIN is digits"));
        }
    }
    Ok(())
}

/// `GET /api/import/plex`: where the import is, or the last one's report.
pub async fn progress(State(state): State<AppState>, _admin: AdminUser) -> Json<Progress> {
    Json(state.plex_import.progress(&state.db).await)
}

/// `DELETE /api/import/plex`: stops the import and forgets the Plex
/// sign-in.
pub async fn forget(State(state): State<AppState>, _admin: AdminUser) -> StatusCode {
    state.plex_import.forget().await;
    StatusCode::NO_CONTENT
}
