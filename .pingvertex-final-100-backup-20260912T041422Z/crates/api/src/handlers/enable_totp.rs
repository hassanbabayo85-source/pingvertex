use axum::extract::{Extension, Json, State};

use application::{dto::EnableTotpResponse, use_cases::EnableTotpUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /auth/totp/enable` — behind `jwt_auth` (see
/// `crates/api/src/routes/auth.rs`). Starts TOTP 2FA enrollment for the
/// caller's own account and returns an `otpauth://` URI to render as a
/// QR code. Does NOT turn 2FA on by itself — see
/// `POST /auth/totp/confirm`.
pub async fn enable_totp(
    State(state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<EnableTotpResponse>, ApiError> {
    let use_case = EnableTotpUseCase::new(
        state.user_repository.clone(),
        state.secret_cipher.clone(),
        state.pending_auth_cache.clone(),
    );

    let response = use_case
        .execute(authenticated_user.user_id())
        .await
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok(Json(response))
}
