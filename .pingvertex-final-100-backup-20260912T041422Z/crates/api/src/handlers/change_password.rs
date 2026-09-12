use axum::extract::{Extension, Json, State};
use axum::http::StatusCode;

use application::{
    dto::ChangePasswordRequest,
    use_cases::{ChangePasswordError, ChangePasswordUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

pub async fn change_password(
    State(state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<StatusCode, ApiError> {
    let use_case = ChangePasswordUseCase::new(
        state.user_repository.clone(),
        state.password_hasher.clone(),
        state.pending_auth_cache.clone(),
    );

    use_case
        .execute(
            authenticated_user.user_id(),
            payload.current_password,
            payload.new_password,
        )
        .await
        .map_err(|err| {
            tracing::warn!("password change failed");
            match err {
                ChangePasswordError::InvalidCurrentPassword => {
                    ApiError::unauthorized("invalid current password")
                }
                ChangePasswordError::WeakPassword(msg) => ApiError::bad_request(msg),
                ChangePasswordError::Internal(_) => ApiError::internal("failed to change password"),
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}
