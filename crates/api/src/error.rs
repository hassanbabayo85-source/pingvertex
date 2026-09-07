use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

/// Unified API error.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Forbidden(String),
    Unauthorized(String),
    TooManyRequests(String),
    PaymentRequired { message: String, code: &'static str },
    Internal(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// 403 Forbidden — the caller is authenticated but their role lacks
    /// the permission required for this action (e.g. `Role::Auditor`
    /// calling an endpoint gated on `Permission::ManageApiKeys`).
    /// Distinct from `UNAUTHORIZED`, which means "not authenticated at
    /// all" and is handled upstream by the auth middleware.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// 401 Unauthorized — used for auth-flow failures that are NOT about
    /// missing/invalid session tokens (that case is handled upstream by
    /// `jwt_auth` returning a bare `StatusCode`, before an `ApiError` is
    /// ever constructed). This variant covers e.g. an invalid/expired
    /// pending 2FA token, an invalid TOTP/backup code, or a failed
    /// password step-up re-authentication check.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
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
    pub fn payment_required(message: impl Into<String>, code: &'static str) -> Self {
        Self::PaymentRequired {
            message: message.into(),
            code,
        }
    }

    pub fn from_payg_error(err: application::ChargeForUsageError) -> Self {
        match err {
            application::ChargeForUsageError::InsufficientBalance => {
                Self::payment_required("PAYG balance is insufficient", "PAYG_INSUFFICIENT_BALANCE")
            }
            application::ChargeForUsageError::InvalidInput(message) => Self::bad_request(message),
            application::ChargeForUsageError::ReservationInProgress => {
                Self::conflict("PAYG request is already in progress")
            }
            application::ChargeForUsageError::OperationFailed(message) => {
                Self::internal_with_source("metered operation failed", message)
            }
            application::ChargeForUsageError::OperationNotAttempted(message) => {
                Self::bad_request(message)
            }
            application::ChargeForUsageError::NoPricing
            | application::ChargeForUsageError::InvalidPricing(_) => {
                Self::internal("PAYG pricing is unavailable")
            }
            application::ChargeForUsageError::Repository(err)
            | application::ChargeForUsageError::CaptureFailed(err) => {
                Self::internal_with_source("PAYG operation failed", err)
            }
        }
    }

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

    /// Maps a use-case error to `403 Forbidden` when it is (or wraps) a
    /// [`domain::shared::types::PermissionDenied`], otherwise falls back to
    /// [`ApiError::internal_with_source`].
    ///
    /// Use cases enforce authorization by calling `AuthenticatedUser::require`,
    /// which returns `PermissionDenied` on a boxed `dyn Error`. Handlers must
    /// route that through here (or an equivalent explicit check) rather than
    /// letting it fall into a generic 500 — a permission failure is a client
    /// error, and treating it as an opaque internal error also makes such
    /// bypasses harder to notice in logs/monitoring.
    pub fn forbidden_or_internal(
        context: &str,
        err: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        if let Some(denied) = err.downcast_ref::<domain::shared::types::PermissionDenied>() {
            return Self::forbidden(denied.to_string());
        }
        Self::internal_with_source(context, err)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::PaymentRequired { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::Forbidden(message)
            | Self::Unauthorized(message)
            | Self::TooManyRequests(message)
            | Self::Internal(message) => message.clone(),
            Self::PaymentRequired { message, .. } => message.clone(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = match &self {
            ApiError::PaymentRequired { code, .. } => Some(*code),
            _ => None,
        };
        let body = Json(ErrorBody {
            message: self.message(),
            code,
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
    fn forbidden_carries_its_own_message_and_status() {
        assert_eq!(
            ApiError::forbidden("you do not have permission to do this").message(),
            "you do not have permission to do this"
        );
        assert_eq!(
            ApiError::forbidden("x").status_code(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn payment_required_uses_402_and_application_code() {
        let err =
            ApiError::payment_required("PAYG balance is insufficient", "PAYG_INSUFFICIENT_BALANCE");
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(err.message(), "PAYG balance is insufficient");
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
