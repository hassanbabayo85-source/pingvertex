use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use tracing::info;

use application::use_cases::ListMembersUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{
    auth_state::AuthState, error::ApiError, handlers::members_types::TenantMembershipResponse,
};

/// Lists all members of the caller's current tenant.
///
/// `GET /api/v1/members`
pub async fn list_members(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<(StatusCode, Json<Vec<TenantMembershipResponse>>), ApiError> {
    info!("API: list_members");

    let use_case = ListMembersUseCase::new(auth_state.tenant_membership_repository.clone());

    let memberships = use_case
        .execute(authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list members", err))?;

    let response: Vec<TenantMembershipResponse> = memberships
        .iter()
        .map(TenantMembershipResponse::from)
        .collect();

    Ok((StatusCode::OK, Json(response)))
}
