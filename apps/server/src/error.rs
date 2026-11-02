use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Error type returned by every HTTP handler. Serialises to
/// `{ "code": "...", "message": "..." }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    /// A refusal the client tells apart by its code: `403` with
    /// `{ code, message }` (the web app's profile switch: `wrong_pin`,
    /// `pin_locked`, as the switch-profile grant's reasons).
    #[error("{1}")]
    ForbiddenAs(&'static str, String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    /// A conflict with what the client needs to resolve it: `409` with
    /// `{ code: "conflict", message, ...details }`.
    #[error("{0}")]
    ConflictWith(String, serde_json::Value),
    #[error("{0}")]
    Unavailable(String),
    /// Something this server can't do at all (its transcoder is too old for
    /// it): `501`, as opposed to `503`'s "not now".
    #[error("{0}")]
    NotImplemented(String),
    #[error("{0}")]
    TooManyRequests(String),
    /// The second step of a sign-in is missing or wrong (see `crate::two_step`).
    #[error("{0}")]
    TwoStep(String),
    /// A sign-in that sets up two-step sign-in first, which the server
    /// requires: `401` with `{ code: "two_step_setup", message, setup }`,
    /// `setup` being the secret to add to an authenticator app.
    #[error("{0}")]
    TwoStepSetup(String, serde_json::Value),
    /// An app's token that is expired, signed out or unknown, a refresh
    /// token sent as a bearer token, or one whose account is disabled.
    #[error("this token doesn't work any more: refresh it, or sign in again")]
    InvalidToken,
    /// An app's token anywhere but `Authorization: Bearer`.
    #[error("app tokens go in the Authorization header")]
    TokenPlacement,
    /// A compatibility token on Pley's own API, wherever it was sent: it
    /// opens the compatibility API only (see `crate::auth::compatibility`).
    #[error("tokens for other apps work on the compatibility API only")]
    CompatibilityToken,
    /// An app's token on a route its scopes don't open: the scope that
    /// would, or `None` when no scope opens it (see `crate::auth::scopes`).
    #[error("{}", scope_message(*.0))]
    InsufficientScope(Option<&'static str>),
    /// A playback session that was stopped from elsewhere (an administrator,
    /// or its owner on another device), with the message they left: `410`
    /// `{ code: "session_stopped", error: "session_stopped", message }`,
    /// where `message` is theirs or `null`.
    #[error("playback was stopped")]
    SessionStopped(Option<String>),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, AppError>;

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    /// The same error again, for each of several requests that waited on one
    /// piece of work.
    pub fn again(&self) -> Self {
        match self {
            Self::BadRequest(m) => Self::BadRequest(m.clone()),
            Self::Unauthorized => Self::Unauthorized,
            Self::Forbidden(m) => Self::Forbidden(m.clone()),
            Self::ForbiddenAs(code, m) => Self::ForbiddenAs(code, m.clone()),
            Self::NotFound(m) => Self::NotFound(m.clone()),
            Self::Conflict(m) => Self::Conflict(m.clone()),
            Self::ConflictWith(m, d) => Self::ConflictWith(m.clone(), d.clone()),
            Self::Unavailable(m) => Self::Unavailable(m.clone()),
            Self::NotImplemented(m) => Self::NotImplemented(m.clone()),
            Self::TooManyRequests(m) => Self::TooManyRequests(m.clone()),
            Self::TwoStep(m) => Self::TwoStep(m.clone()),
            Self::TwoStepSetup(m, s) => Self::TwoStepSetup(m.clone(), s.clone()),
            Self::InvalidToken => Self::InvalidToken,
            Self::TokenPlacement => Self::TokenPlacement,
            Self::CompatibilityToken => Self::CompatibilityToken,
            Self::InsufficientScope(scope) => Self::InsufficientScope(*scope),
            Self::SessionStopped(m) => Self::SessionStopped(m.clone()),
            Self::Internal(err) => Self::Internal(anyhow::anyhow!("{err:#}")),
        }
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::ForbiddenAs(code, _) => (StatusCode::FORBIDDEN, *code),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) | Self::ConflictWith(..) => (StatusCode::CONFLICT, "conflict"),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            Self::NotImplemented(_) => (StatusCode::NOT_IMPLEMENTED, "not_implemented"),
            Self::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "too_many_requests"),
            Self::TwoStep(_) => (StatusCode::UNAUTHORIZED, "two_step"),
            Self::TwoStepSetup(..) => (StatusCode::UNAUTHORIZED, "two_step_setup"),
            Self::InvalidToken | Self::CompatibilityToken => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::TokenPlacement => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::InsufficientScope(_) => (StatusCode::FORBIDDEN, "insufficient_scope"),
            Self::SessionStopped(_) => (StatusCode::GONE, "session_stopped"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }

    /// `WWW-Authenticate` of the answers to apps' tokens (RFC 6750 §3).
    fn challenge(&self) -> Option<String> {
        match self {
            Self::InvalidToken | Self::CompatibilityToken => Some(r#"Bearer error="invalid_token""#.to_owned()),
            Self::TokenPlacement => Some(r#"Bearer error="invalid_request""#.to_owned()),
            Self::InsufficientScope(None) => Some(r#"Bearer error="insufficient_scope""#.to_owned()),
            Self::InsufficientScope(Some(scope)) => {
                Some(format!(r#"Bearer error="insufficient_scope", scope="{scope}""#))
            }
            _ => None,
        }
    }
}

fn scope_message(scope: Option<&str>) -> String {
    match scope.and_then(crate::auth::scopes::Scope::from_name) {
        Some(scope) => format!("this app wasn't allowed to {}: sign in to it again to allow it", scope.words()),
        None => "apps can't do that: use Pley's web app".to_owned(),
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound("not found".into()),
            other => Self::Internal(other.into()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound("file not found".into()),
            _ => Self::Internal(err.into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if let Self::SessionStopped(message) = &self {
            // The players' contract names it `error`; `code` as every error.
            return (status, Json(json!({ "code": code, "error": code, "message": message }))).into_response();
        }
        if let Self::ConflictWith(message, details) = &self {
            let mut body = json!({ "code": code, "message": message });
            if let (Some(body), Some(details)) = (body.as_object_mut(), details.as_object()) {
                body.extend(details.clone());
            }
            return (status, Json(body)).into_response();
        }
        if let Self::TwoStepSetup(message, setup) = &self {
            return (status, Json(json!({ "code": code, "message": message, "setup": setup }))).into_response();
        }
        let challenge = self.challenge();
        let message = match &self {
            Self::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                "internal server error".to_owned()
            }
            other => other.to_string(),
        };
        let mut response = (status, Json(json!({ "code": code, "message": message }))).into_response();
        if let Some(value) = challenge.and_then(|c| HeaderValue::from_str(&c).ok()) {
            response.headers_mut().insert(header::WWW_AUTHENTICATE, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn answer(err: AppError) -> (u16, serde_json::Value) {
        let response = err.into_response();
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn errors_answer_with_a_status_code_and_message() {
        for (err, status, code) in [
            (AppError::bad_request("no"), 400, "bad_request"),
            (AppError::Unauthorized, 401, "unauthorized"),
            (AppError::forbidden("no"), 403, "forbidden"),
            (AppError::not_found("no"), 404, "not_found"),
            (AppError::conflict("no"), 409, "conflict"),
            (AppError::TooManyRequests("no".into()), 429, "too_many_requests"),
            (AppError::Unavailable("no".into()), 503, "unavailable"),
            (AppError::TwoStep("no".into()), 401, "two_step"),
            (AppError::TwoStepSetup("no".into(), json!({})), 401, "two_step_setup"),
            (AppError::InvalidToken, 401, "unauthorized"),
            (AppError::TokenPlacement, 400, "bad_request"),
            (AppError::CompatibilityToken, 401, "unauthorized"),
            (AppError::InsufficientScope(None), 403, "insufficient_scope"),
            (AppError::InsufficientScope(Some("play")), 403, "insufficient_scope"),
            (AppError::SessionStopped(None), 410, "session_stopped"),
        ] {
            let again = err.again();
            let (got, body) = answer(err).await;
            assert_eq!((got, body["code"].as_str().unwrap()), (status, code));
            assert_eq!(answer(again).await.0, status);
        }
        assert_eq!(answer(AppError::not_found("item 42 not found")).await.1["message"], "item 42 not found");
    }

    #[tokio::test]
    async fn a_sign_in_that_sets_up_two_step_sign_in_carries_the_secret() {
        let setup = json!({"secret": "JBSWY3DPEHPK3PXP", "uri": "otpauth://totp/…", "expires_at": 1});
        let (status, body) = answer(AppError::TwoStepSetup("add Pley to an app".into(), setup.clone())).await;
        assert_eq!(status, 401);
        assert_eq!(body, json!({"code": "two_step_setup", "message": "add Pley to an app", "setup": setup}));
    }

    #[tokio::test]
    async fn app_token_errors_say_why_in_their_header() {
        for (err, challenge, message) in [
            (AppError::InvalidToken, Some(r#"Bearer error="invalid_token""#), None),
            (
                AppError::TokenPlacement,
                Some(r#"Bearer error="invalid_request""#),
                Some("app tokens go in the Authorization header"),
            ),
            (
                AppError::CompatibilityToken,
                Some(r#"Bearer error="invalid_token""#),
                Some("tokens for other apps work on the compatibility API only"),
            ),
            (
                AppError::InsufficientScope(None),
                Some(r#"Bearer error="insufficient_scope""#),
                Some("apps can't do that: use Pley's web app"),
            ),
            (
                AppError::InsufficientScope(Some("playlists")),
                Some(r#"Bearer error="insufficient_scope", scope="playlists""#),
                Some(
                    "this app wasn't allowed to make, change and delete the profile's playlists: \
                     sign in to it again to allow it",
                ),
            ),
            (AppError::Unauthorized, None, None),
            (AppError::forbidden("no"), None, None),
        ] {
            let response = err.into_response();
            let header = response.headers().get(header::WWW_AUTHENTICATE).map(|v| v.to_str().unwrap().to_owned());
            assert_eq!(header.as_deref(), challenge);
            let bytes = axum::body::to_bytes(response.into_body(), 1 << 16).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            if let Some(message) = message {
                assert_eq!(body["message"], message);
            }
        }
    }

    #[tokio::test]
    async fn internal_errors_keep_their_details_out_of_the_answer() {
        let (status, body) = answer(AppError::Internal(anyhow::anyhow!("disk at /secret/path is full"))).await;
        assert_eq!(status, 500);
        assert_eq!(body, json!({"code": "internal", "message": "internal server error"}));
    }

    #[test]
    fn missing_rows_and_files_are_not_found() {
        assert!(matches!(AppError::from(sqlx::Error::RowNotFound), AppError::NotFound(_)));
        assert!(matches!(AppError::from(sqlx::Error::PoolTimedOut), AppError::Internal(_)));
        assert!(matches!(AppError::from(std::io::Error::from(std::io::ErrorKind::NotFound)), AppError::NotFound(_)));
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(matches!(AppError::from(denied), AppError::Internal(_)));
    }
}
