use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{commands::RequestCustomDomain, use_cases::RequestCustomDomainUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{CustomDomainRequestBody, StatusPageResponse},
    state::AppState,
    validation::parse_status_page_id,
};

pub async fn request_custom_domain(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(payload): Json<CustomDomainRequestBody>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: request_custom_domain");

    let id = parse_status_page_id(&id)?;

    let command = RequestCustomDomain {
        id,
        domain: payload.domain,
    };

    let use_case = RequestCustomDomainUseCase::new(
        state.status_page_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to request custom domain", err))?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
