use axum::{
    Json,
    extract::{Extension, State},
};
use serde::Serialize;

use application::ports::UserRepository;
use domain::shared::types::AuthenticatedUser;

use crate::{auth_state::AuthState, error::ApiError};

#[derive(Debug, Serialize)]
pub struct CurrentSessionResponse {
    pub authenticated: bool,
    pub user: CurrentSessionUser,
}

#[derive(Debug, Serialize)]
pub struct CurrentSessionUser {
    pub id: String,
    pub email: String,
    pub username: String,
    pub email_verified: bool,
    pub tenant_id: String,
    pub role: domain::shared::types::Role,
    pub totp_enabled: bool,
}

/// `GET /api/v1/auth/me`
///
/// The server is the source of truth for browser authentication. The JWT is
/// intentionally kept in an HttpOnly cookie and is never exposed to
/// JavaScript. This endpoint validates the session through `jwt_auth` and
/// returns only the non-sensitive identity information required by the UI.
pub async fn current_session(
    State(state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<CurrentSessionResponse>, ApiError> {
    let user_id = authenticated_user.user_id();
    let user = state
        .user_repository
        .find_by_id(&user_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to load current user", err))?
        .ok_or_else(|| ApiError::unauthorized("invalid session"))?;

    Ok(Json(CurrentSessionResponse {
        authenticated: true,
        user: CurrentSessionUser {
            id: user.id().to_string(),
            email: user.email().to_string(),
            username: user.username().to_string(),
            email_verified: user.is_email_verified(),
            tenant_id: authenticated_user
                .tenant_context()
                .tenant_id()
                .0
                .to_string(),
            role: authenticated_user.role(),
            totp_enabled: user.totp_enabled(),
        },
    }))
}
