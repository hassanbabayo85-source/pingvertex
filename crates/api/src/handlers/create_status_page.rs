use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    dto::CreateStatusPageRequest,
    use_cases::{CreateStatusPageError, CreateStatusPageUseCase},
};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{StatusPageCreateRequest, StatusPageResponse},
    state::AppState,
    validation::parse_monitor_id,
};

/// Creates a status page through the API.
pub async fn create_status_page(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Json(payload): Json<StatusPageCreateRequest>,
) -> Result<(StatusCode, Json<StatusPageResponse>), ApiError> {
    info!("API: create_status_page");

    let monitor_ids = payload
        .monitor_ids
        .iter()
        .map(|id| parse_monitor_id(id))
        .collect::<Result<Vec<_>, _>>()?;

    let request = CreateStatusPageRequest {
        user_id,
        name: payload.name,
        subdomain: payload.subdomain,
        monitor_ids,
    };

    let use_case =
        CreateStatusPageUseCase::new(state.status_page_repository.clone(), state.id_generator);

    let status_page = use_case.execute(request).await.map_err(|err| match err {
        CreateStatusPageError::QuotaExceeded { .. }
        | CreateStatusPageError::SubdomainTaken { .. } => ApiError::conflict(err.to_string()),
        CreateStatusPageError::Other(inner) => {
            ApiError::internal_with_source("failed to create status page", inner)
        }
    })?;

    Ok((
        StatusCode::CREATED,
        Json(StatusPageResponse::from(status_page)),
    ))
}
