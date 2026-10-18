use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use tracing::info;

use application::use_cases::{AddMemberCommand, AddMemberUseCase};
use domain::shared::ids::UserId;
use domain::shared::types::AuthenticatedUser;

use crate::{
    auth_state::AuthState,
    error::ApiError,
    handlers::members_types::{AddMemberRequest, TenantMembershipResponse},
    validation::parse_role,
};

/// Adds a user directly to the caller's tenant.
///
/// `POST /api/v1/members`
///
/// The caller must be a member of the tenant with `ManageUsers`
/// permission. See `AddMemberUseCase` for role restrictions.
pub async fn add_member(
    State(auth_state): State<AuthState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<TenantMembershipResponse>), ApiError> {
    info!("API: add_member");

    let role = parse_role(&payload.role)?;

    let use_case = AddMemberUseCase::new(
        auth_state.tenant_membership_repository.clone(),
        auth_state.user_repository.clone(),
    );

    let created = use_case
        .with_entitlement_resolver(auth_state.entitlement_resolver.clone())
        .with_audit_log_repository(auth_state.audit_log_entry_repository.clone())
        .execute(
            AddMemberCommand {
                user_id: UserId(payload.user_id),
                role,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| ApiError::internal_with_source("failed to add member", err))?;

    Ok((
        StatusCode::CREATED,
        Json(TenantMembershipResponse::from(created)),
    ))
}
