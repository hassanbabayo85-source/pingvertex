use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::{info, warn};

use application::{commands::RemoveCustomDomain, use_cases::RemoveCustomDomainUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::types::StatusPageResponse, state::AppState,
    validation::parse_status_page_id,
};

pub async fn remove_custom_domain(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: remove_custom_domain");

    let id = parse_status_page_id(&id)?;

    let command = RemoveCustomDomain { id };

    let use_case = RemoveCustomDomainUseCase::new(
        state.status_page_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute(command, authenticated_user)
        .await
        .map_err(|err| ApiError::forbidden_or_internal("failed to remove custom domain", err))?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    let cache_key = format!("status_page:id:{}", status_page.id.0);
    if let Err(err) = state.status_page_cache.invalidate(&cache_key).await {
        warn!(status_page_id = %status_page.id.0, error = %err, "failed to invalidate cached status page after custom-domain change");
    }

    Ok(Json(StatusPageResponse::from(status_page)))
}
