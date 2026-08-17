use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::use_cases::GetProxyCheckJobUseCase;
use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::proxy_check_types::ProxyCheckJobDetailHttpResponse, state::AppState,
    validation::parse_proxy_check_job_id,
};

pub async fn get_proxy_check_job(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<Json<ProxyCheckJobDetailHttpResponse>, ApiError> {
    info!("API: get_proxy_check_job");

    let id = parse_proxy_check_job_id(&id)?;

    let use_case = GetProxyCheckJobUseCase::new(state.proxy_check_job_repository.clone());

    let (job, entries) = use_case
        .execute(id, user_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch proxy check job", err))?
        .ok_or_else(|| ApiError::not_found("proxy check job not found"))?;

    Ok(Json(ProxyCheckJobDetailHttpResponse::from((job, entries))))
}
