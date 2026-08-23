use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{
    dto::VerifyEmailRequest,
    use_cases::{VerifyEmailError, VerifyEmailUseCase},
};

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /auth/verify-email`
///
/// Public by design: the caller proves ownership of the token, not a
/// session. Rate-limited by the shared `/auth/*` GovernorLayer in
/// production, and by the high-entropy token format.
pub async fn verify_email(
    State(state): State<AuthState>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let use_case = VerifyEmailUseCase::new(
        state.email_verification_token_repository.clone(),
        state.user_repository.clone(),
        state.password_hasher.clone(),
    );

    use_case.execute(payload.token).await.map_err(|err| {
        // SECURITY: never log the raw token. The only thing logged here
        // is a generic failure message.
        tracing::warn!("email verification failed");
        match err {
            VerifyEmailError::InvalidOrExpiredToken => {
                ApiError::bad_request("invalid or expired verification token")
            }
            VerifyEmailError::Internal(_) => ApiError::internal("failed to verify email"),
        }
    })?;

    Ok((StatusCode::OK, Json(json!({ "verified": true }))))
}
