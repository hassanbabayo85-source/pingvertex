use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    dto::CreateStatusPageRequest,
    use_cases::{CreateStatusPageError, CreateStatusPageUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{StatusPageCreateRequest, StatusPageResponse},
    state::AppState,
    validation::parse_monitor_id,
};

pub async fn create_status_page(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<StatusPageCreateRequest>,
) -> Result<(StatusCode, Json<StatusPageResponse>), ApiError> {
    info!("API: create_status_page");

    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|id| parse_monitor_id(id))
        .collect::<Result<Vec<_>, _>>()?;

    let request = CreateStatusPageRequest {
        name: payload.name,
        subdomain: payload.subdomain,
        monitor_ids,
    };

    let use_case = CreateStatusPageUseCase::new(
        state.status_page_repository.clone(),
        state.id_generator,
        state.monitor_repository.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let status_page = use_case
        .execute(request, authenticated_user)
        .await
        .map_err(|err| match err {
            CreateStatusPageError::QuotaExceeded { .. }
            | CreateStatusPageError::SubdomainTaken { .. } => ApiError::conflict(err.to_string()),
            CreateStatusPageError::MonitorNotFound { .. } => ApiError::not_found(err.to_string()),
            CreateStatusPageError::Other(inner) => {
                ApiError::internal_with_source("failed to create status page", inner)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(StatusPageResponse::from(status_page)),
    ))
}
