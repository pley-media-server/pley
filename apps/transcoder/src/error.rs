// SPDX-License-Identifier: AGPL-3.0-only
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use pley_transcoder_api::ErrorBody;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    /// Refused whoever asks: a request to a transcoder without a token,
    /// addressed to another host than this computer.
    Forbidden(String),
    NotFound(String),
    /// The job does not exist (expired, deleted, transcoder restarted). Distinct
    /// from other 404s so clients only re-create jobs that are really gone.
    JobNotFound(String),
    /// Capacity exhausted (too many concurrent transcodes).
    Busy(String),
    Timeout(String),
    /// ffmpeg failed.
    Failed(String),
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    /// The same error again, for each of several requests that waited on one
    /// piece of work.
    pub fn again(&self) -> Self {
        match self {
            Self::BadRequest(m) => Self::BadRequest(m.clone()),
            Self::Unauthorized => Self::Unauthorized,
            Self::Forbidden(m) => Self::Forbidden(m.clone()),
            Self::NotFound(m) => Self::NotFound(m.clone()),
            Self::JobNotFound(m) => Self::JobNotFound(m.clone()),
            Self::Busy(m) => Self::Busy(m.clone()),
            Self::Timeout(m) => Self::Timeout(m.clone()),
            Self::Failed(m) => Self::Failed(m.clone()),
            Self::Internal(err) => Self::Internal(anyhow::anyhow!("{err:#}")),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            ApiError::NotFound(err.to_string())
        } else {
            ApiError::Internal(err.into())
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "missing or invalid token".into()),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, "forbidden", m),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            ApiError::JobNotFound(m) => (StatusCode::NOT_FOUND, "job_not_found", m),
            ApiError::Busy(m) => (StatusCode::SERVICE_UNAVAILABLE, "busy", m),
            ApiError::Timeout(m) => (StatusCode::GATEWAY_TIMEOUT, "timeout", m),
            ApiError::Failed(m) => (StatusCode::INTERNAL_SERVER_ERROR, "ffmpeg_failed", m),
            ApiError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                // With its causes: "probe failed: ffprobe: Access is denied".
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", format!("{e:#}"))
            }
        };
        (status, Json(ErrorBody { code: code.into(), message })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn answer(err: ApiError) -> (u16, ErrorBody) {
        let response = err.into_response();
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 16).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    /// The server tells a lost job (`job_not_found`, recreate it) from other
    /// 404s and unreadable files (`ffmpeg_failed`, don't retry) by these codes.
    #[tokio::test]
    async fn errors_keep_their_status_and_code() {
        for (err, status, code) in [
            (ApiError::BadRequest("bad".into()), 400, "bad_request"),
            (ApiError::Unauthorized, 401, "unauthorized"),
            (ApiError::Forbidden("elsewhere".into()), 403, "forbidden"),
            (ApiError::NotFound("missing".into()), 404, "not_found"),
            (ApiError::JobNotFound("gone".into()), 404, "job_not_found"),
            (ApiError::Busy("full".into()), 503, "busy"),
            (ApiError::Timeout("slow".into()), 504, "timeout"),
            (ApiError::Failed("unreadable".into()), 500, "ffmpeg_failed"),
            (ApiError::Internal(anyhow::anyhow!("boom")), 500, "internal"),
        ] {
            // Each request that waited on the work gets the same answer.
            let again = answer(err.again()).await;
            let (got_status, body) = answer(err).await;
            assert_eq!((got_status, body.code.as_str()), (status, code));
            assert!(!body.message.is_empty());
            assert_eq!((again.0, again.1.code, again.1.message), (got_status, body.code, body.message));
        }
        assert_eq!(
            answer(ApiError::NotFound("segment 9 out of range".into())).await.1.message,
            "segment 9 out of range"
        );
        // An internal error says why, not only what was being done.
        let denied = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        let message = answer(ApiError::Internal(denied.context("couldn't keep the file"))).await.1.message;
        assert!(message.starts_with("couldn't keep the file: ") && message.len() > 24, "{message}");
    }

    #[test]
    fn missing_files_are_not_found_other_io_errors_internal() {
        let missing = ApiError::from(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(matches!(missing, ApiError::NotFound(_)));
        let denied = ApiError::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(matches!(denied, ApiError::Internal(_)));
    }
}
