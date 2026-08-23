use axum::extract::{Extension, Json, State};
use axum::http::StatusCode;

use application::{dto::DisableTotpRequest, use_cases::DisableTotpUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /auth/totp/disable` — behind `jwt_auth`, AND requires the
/// caller's current password in the request body (step-up
/// re-authentication) — see `DisableTotpUseCase`'s doc comment for why a
/// session JWT alone is not treated as sufficient proof of intent here.
pub async fn disable_totp(
    State(state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<DisableTotpRequest>,
) -> Result<StatusCode, ApiError> {
    let use_case = DisableTotpUseCase::new(
        state.user_repository.clone(),
        state.password_hasher.clone(),
        state.backup_code_repository.clone(),
    );

    use_case
        .execute(authenticated_user.user_id(), payload)
        .await
        .map_err(|err| ApiError::unauthorized(err.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
