use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{
    dto::ResetPasswordRequest,
    use_cases::{ResetPasswordError, ResetPasswordUseCase},
};

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /auth/reset-password`
///
/// Public by design: the caller proves ownership of the reset token, not
/// a session (the whole point is that they've forgotten their password
/// and can't log in). Rate-limited by the shared `/auth/*` GovernorLayer
/// in production, plus the token's own high entropy — mirrors
/// `verify_email.rs`'s exact protection posture.
pub async fn reset_password(
    State(state): State<AuthState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let use_case = ResetPasswordUseCase::new(
        state.reset_password_token_repository.clone(),
        state.user_repository.clone(),
        state.password_hasher.clone(),
        state.pending_auth_cache.clone(),
    );

    use_case
        .execute(payload.token, payload.new_password)
        .await
        .map_err(|err| {
            // SECURITY: never log the raw token.
            tracing::warn!("password reset failed");
            match err {
                ResetPasswordError::InvalidOrExpiredToken => {
                    ApiError::bad_request("invalid or expired reset token")
                }
                ResetPasswordError::WeakPassword(msg) => ApiError::bad_request(msg),
                ResetPasswordError::Internal(_) => ApiError::internal("failed to reset password"),
            }
        })?;

    Ok((StatusCode::OK, Json(json!({ "reset": true }))))
}
