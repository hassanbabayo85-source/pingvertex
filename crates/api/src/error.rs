use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
}

/// Unified API error.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    TooManyRequests(String),
    Internal(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// 429 Too Many Requests — used when a caller-specific quota (not the
    /// IP-based `GovernorLayer` rate limiter, which axum never sees as an
    /// `ApiError`) has been exceeded, e.g. the daily proxy-check job quota.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::TooManyRequests(message.into())
    }

    /// Generic internal error.
    ///
    /// ONLY use this when `message` is already known to be safe to show a
    /// client (a static, hand-written string with no interpolated
    /// database/driver/library error text). If you are wrapping an
    /// underlying `Error`, use [`ApiError::internal_with_source`] instead
    /// — that constructor logs the real cause and returns a generic
    /// message to the caller.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Preferred constructor for internal errors that wrap an underlying
    /// cause (database errors, I/O errors, etc).
    ///
    /// The full error (`err`) is logged server-side via `tracing::error!`
    /// together with `context`, so operators retain complete diagnostic
    /// information. The HTTP response sent to the client contains ONLY
    /// the generic `context` string — never the raw `{err}` — so
    /// internal details such as table/column names or database driver
    /// messages never leak to an untrusted caller. This closes an
    /// information-disclosure gap identified in the 2026-08-11 security
    /// audit, where 500 responses previously echoed raw backend errors.
    pub fn internal_with_source(context: &str, err: impl std::fmt::Display) -> Self {
        error!(error = %err, "{context}");
        Self::Internal(context.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::TooManyRequests(message)
            | Self::Internal(message) => message.clone(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ErrorBody {
            message: self.message(),
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Misalin "kuskure" mai kama da wanda sqlx/postgres ke iya bayarwa —
    /// yana ƙunshe da sunan table/column, wanda BA A SO ya bayyana ga
    /// client.
    struct FakeDbError(&'static str);

    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[test]
    fn internal_with_source_never_leaks_raw_error_text_to_client_message() {
        let sensitive =
            FakeDbError("duplicate key value violates unique constraint \"monitors_url_idx\"");

        let err = ApiError::internal_with_source("failed to create monitor", sensitive);

        match &err {
            ApiError::Internal(message) => {
                assert_eq!(message, "failed to create monitor");
                assert!(
                    !message.contains("unique constraint"),
                    "client-facing message must not contain raw DB error text, got: {message}"
                );
                assert!(
                    !message.contains("monitors_url_idx"),
                    "client-facing message must not contain table/column names, got: {message}"
                );
            }
            other => panic!("expected ApiError::Internal, got {other:?}"),
        }
    }

    #[test]
    fn internal_with_source_response_body_is_generic() {
        let sensitive = FakeDbError("relation \"monitors\" does not exist at line 42");

        let err = ApiError::internal_with_source("failed to fetch monitor", sensitive);
        let status = err.status_code();
        let message = err.message();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message, "failed to fetch monitor");
        assert!(!message.contains("relation"));
        assert!(!message.contains("line 42"));
    }

    #[test]
    fn internal_static_message_is_used_verbatim() {
        let err = ApiError::internal("failed to set cookie");
        assert_eq!(err.message(), "failed to set cookie");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn bad_request_and_not_found_still_carry_their_own_message() {
        assert_eq!(
            ApiError::bad_request("invalid uuid").message(),
            "invalid uuid"
        );
        assert_eq!(
            ApiError::not_found("monitor not found").message(),
            "monitor not found"
        );
        assert_eq!(
            ApiError::bad_request("x").status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::not_found("x").status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn too_many_requests_carries_its_own_message() {
        assert_eq!(
            ApiError::too_many_requests("daily limit reached").message(),
            "daily limit reached"
        );
        assert_eq!(
            ApiError::too_many_requests("x").status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
