use axum::{
    Json,
    extract::{Extension, Path, State},
};
use tracing::info;

use application::use_cases::GetProxyCheckJobUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::proxy_check_types::ProxyCheckJobDetailHttpResponse, state::AppState,
    validation::parse_proxy_check_job_id,
};

pub async fn get_proxy_check_job(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<ProxyCheckJobDetailHttpResponse>, ApiError> {
    info!("API: get_proxy_check_job");

    let id = parse_proxy_check_job_id(&id)?;

    let use_case = GetProxyCheckJobUseCase::new(state.proxy_check_job_repository.clone());

    let (job, entries) = use_case
        .execute(id, authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch proxy check job", err))?
        .ok_or_else(|| ApiError::not_found("proxy check job not found"))?;

    Ok(Json(ProxyCheckJobDetailHttpResponse::from((job, entries))))
}
