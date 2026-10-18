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
    /// Populated only for structured rejections (today:
    /// `PlanLimitReached`). `None` for all legacy variants, so existing
    /// clients see an unchanged wire shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

/// Unified API error.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Gone(String),
    Forbidden(String),
    Unauthorized(String),
    TooManyRequests(String),
    PaymentRequired { message: String, code: &'static str },
    /// TASK 2 — a hard subscription quota was exceeded. Distinct from
    /// `PaymentRequired`: this is not a "pay more to continue" error,
    /// it is a plan limit that only a plan change can lift. Serialized
    /// with `code: "plan_limit_reached"` and a `details` object.
    PlanLimitReached {
        quota: &'static str,
        current: u64,
        allowed: u64,
        action: &'static str,
    },
    Internal(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn gone(message: impl Into<String>) -> Self {
        Self::Gone(message.into())
    }

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::TooManyRequests(message.into())
    }

    pub fn payment_required(message: impl Into<String>, code: &'static str) -> Self {
        Self::PaymentRequired {
            message: message.into(),
            code,
        }
    }

    pub fn plan_limit_reached(
        quota: &'static str,
        current: u64,
        allowed: u64,
        action: &'static str,
    ) -> Self {
        Self::PlanLimitReached {
            quota,
            current,
            allowed,
            action,
        }
    }

    /// Maps a structured `application::PlanLimitError` to this crate's
    /// `PlanLimitReached` variant.
    pub fn from_plan_limit(err: application::PlanLimitError) -> Self {
        Self::PlanLimitReached {
            quota: err.quota,
            current: err.current,
            allowed: err.allowed,
            action: err.action,
        }
    }

    pub fn from_payg_error(err: application::ChargeForUsageError) -> Self {
        match err {
            application::ChargeForUsageError::SpendLimitExceeded => {
                Self::payment_required("PAYG spend limit exceeded", "PAYG_SPEND_LIMIT_EXCEEDED")
            }
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

    pub fn internal_with_source(context: &str, err: impl std::fmt::Display) -> Self {
        error!(error = %err, "{context}");
        Self::Internal(context.to_string())
    }

    pub fn forbidden_or_internal(
        context: &str,
        err: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        if let Some(denied) = err.downcast_ref::<domain::shared::types::PermissionDenied>() {
            return Self::forbidden(denied.to_string());
        }
        if let Some(pl) = err.downcast_ref::<application::PlanLimitError>() {
            return Self::PlanLimitReached {
                quota: pl.quota,
                current: pl.current,
                allowed: pl.allowed,
                action: pl.action,
            };
        }
        Self::internal_with_source(context, err)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Gone(_) => StatusCode::GONE,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::PaymentRequired { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::PlanLimitReached { .. } => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::Gone(message)
            | Self::Forbidden(message)
            | Self::Unauthorized(message)
            | Self::TooManyRequests(message)
            | Self::Internal(message) => message.clone(),
            Self::PaymentRequired { message, .. } => message.clone(),
            Self::PlanLimitReached {
                quota,
                current,
                allowed,
                action,
            } => format!(
                "plan limit reached: '{quota}' allows at most {allowed} (current: {current}); suggested action: {action}"
            ),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = match &self {
            ApiError::PlanLimitReached {
                quota,
                current,
                allowed,
                action,
            } => Json(ErrorBody {
                message: self.message(),
                code: Some("plan_limit_reached"),
                details: Some(serde_json::json!({
                    "quota": quota,
                    "current": current,
                    "allowed": allowed,
                    "action": action,
                })),
            }),
            ApiError::PaymentRequired { code, .. } => Json(ErrorBody {
                message: self.message(),
                code: Some(*code),
                details: None,
            }),
            _ => Json(ErrorBody {
                message: self.message(),
                code: None,
                details: None,
            }),
        };

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                assert!(!message.contains("unique constraint"));
                assert!(!message.contains("monitors_url_idx"));
            }
            other => panic!("expected ApiError::Internal, got {other:?}"),
        }
    }

    #[test]
    fn internal_with_source_response_body_is_generic() {
        let sensitive = FakeDbError("relation \"monitors\" does not exist at line 42");
        let err = ApiError::internal_with_source("failed to fetch monitor", sensitive);
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.message(), "failed to fetch monitor");
    }

    #[test]
    fn internal_static_message_is_used_verbatim() {
        let err = ApiError::internal("failed to set cookie");
        assert_eq!(err.message(), "failed to set cookie");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn bad_request_and_not_found_still_carry_their_own_message() {
        assert_eq!(ApiError::bad_request("invalid uuid").message(), "invalid uuid");
        assert_eq!(ApiError::not_found("monitor not found").message(), "monitor not found");
        assert_eq!(ApiError::bad_request("x").status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ApiError::not_found("x").status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn forbidden_carries_its_own_message_and_status() {
        assert_eq!(ApiError::forbidden("x").status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn payment_required_uses_402_and_application_code() {
        let err = ApiError::payment_required(
            "PAYG balance is insufficient",
            "PAYG_INSUFFICIENT_BALANCE",
        );
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(err.message(), "PAYG balance is insufficient");
    }

    #[test]
    fn too_many_requests_carries_its_own_message() {
        assert_eq!(ApiError::too_many_requests("x").status_code(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn plan_limit_reached_uses_409_conflict() {
        let err = ApiError::plan_limit_reached("max_uptime_monitors", 20, 20, "upgrade_plan");
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        assert!(err.message().contains("max_uptime_monitors"));
        assert!(err.message().contains("20"));
        assert!(err.message().contains("upgrade_plan"));
    }

    #[test]
    fn from_plan_limit_preserves_all_fields() {
        let pl = application::PlanLimitError::new(
            "max_recurring_proxy_monitors",
            5,
            5,
            "upgrade_plan",
        );
        match ApiError::from_plan_limit(pl) {
            ApiError::PlanLimitReached { quota, current, allowed, action } => {
                assert_eq!(quota, "max_recurring_proxy_monitors");
                assert_eq!(current, 5);
                assert_eq!(allowed, 5);
                assert_eq!(action, "upgrade_plan");
            }
            other => panic!("expected PlanLimitReached, got {other:?}"),
        }
    }

    #[test]
    fn forbidden_or_internal_detects_plan_limit_through_boxed_error() {
        let pl = application::PlanLimitError::new("max_uptime_monitors", 20, 20, "upgrade_plan");
        let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(pl);
        match ApiError::forbidden_or_internal("context", boxed) {
            ApiError::PlanLimitReached { quota, .. } => assert_eq!(quota, "max_uptime_monitors"),
            other => panic!("expected PlanLimitReached, got {other:?}"),
        }
    }
}
