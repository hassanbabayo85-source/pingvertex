use axum::{Json, extract::Extension, extract::Path, extract::State, http::StatusCode};
use domain::shared::ids::UserId;
use domain::shared::types::AuthenticatedUser;
use tracing::info;
use uuid::Uuid;

use application::use_cases::{ChangeMemberRoleCommand, ChangeMemberRoleUseCase};

use crate::{
    auth_state::AuthState, error::ApiError, handlers::members_types::ChangeMemberRoleRequest,
    validation::parse_role,
};

/// Changes a member's role.
///
/// `PUT /api/v1/members/{user_id}/role`
pub async fn change_member_role(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<ChangeMemberRoleRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    info!("API: change_member_role");

    let new_role = parse_role(&payload.role)?;

    let use_case = ChangeMemberRoleUseCase::new(auth_state.tenant_membership_repository.clone());

    use_case
        .execute(
            ChangeMemberRoleCommand {
                user_id: UserId(user_id),
                new_role,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to change member role", err))?;

    Ok((StatusCode::OK, Json(serde_json::json!({}))))
}
