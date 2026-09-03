use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::{info, warn};

use application::{
    commands::RequestCustomDomain,
    use_cases::{RequestCustomDomainError, RequestCustomDomainUseCase},
};
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
        .map_err(|err| match err {
            RequestCustomDomainError::Forbidden => {
                ApiError::forbidden("you do not have permission to manage custom domains")
            }
            RequestCustomDomainError::AlreadyClaimed { .. } => ApiError::conflict(err.to_string()),
            RequestCustomDomainError::Other(inner) => {
                ApiError::internal_with_source("failed to request custom domain", inner)
            }
        })?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    let cache_key = format!("status_page:id:{}", status_page.id.0);
    if let Err(err) = state.status_page_cache.invalidate(&cache_key).await {
        warn!(status_page_id = %status_page.id.0, error = %err, "failed to invalidate cached status page after custom-domain change");
    }

    Ok(Json(StatusPageResponse::from(status_page)))
}
