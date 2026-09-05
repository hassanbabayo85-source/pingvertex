use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{
    dto::VerifyEmailOtpRequest,
    use_cases::{VerifyEmailOtpError, VerifyEmailOtpUseCase},
};

use crate::{auth_state::AuthState, error::ApiError};

/// Public email-verification endpoint. No JWT is accepted or required.
/// A valid email + 6-digit OTP activates the account.
pub async fn verify_email_otp(
    State(state): State<AuthState>,
    Json(payload): Json<VerifyEmailOtpRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let email_key = payload.email.trim().to_ascii_lowercase();

    let allowed = state
        .rate_limiter
        .check_and_record("verify_email_otp", &email_key, 5, 600)
        .await
        .map_err(|_| ApiError::internal("verification service unavailable"))?;

    if !allowed {
        return Err(ApiError::too_many_requests(
            "too many verification attempts; please try again later",
        ));
    }

    let use_case = VerifyEmailOtpUseCase::new(
        state.email_verification_token_repository.clone(),
        state.user_repository.clone(),
        state.password_hasher.clone(),
    );

    use_case
        .execute(payload.email, payload.code)
        .await
        .map_err(|err| {
            tracing::warn!("email OTP verification failed");
            match err {
                VerifyEmailOtpError::InvalidCode => {
                    ApiError::bad_request("invalid or expired verification code")
                }
                VerifyEmailOtpError::Internal(_) => ApiError::internal("failed to verify email"),
            }
        })?;

    Ok((StatusCode::OK, Json(json!({ "verified": true }))))
}
