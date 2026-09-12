use axum::{
    Json,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
};
use tracing::info;

use application::dto::CreateProxyCheckJobRequest;
use application::parsing::proxy_list_parser;
use application::use_cases::{SubmitProxyCheckJobError, SubmitProxyCheckJobUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError, handlers::proxy_check_types::ProxyCheckJobHttpResponse, state::AppState,
};

pub async fn submit_proxy_check_job(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    body: String,
) -> Result<(StatusCode, Json<ProxyCheckJobHttpResponse>), ApiError> {
    info!("API: submit_proxy_check_job");

    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let entries = if content_type.starts_with("text/csv") {
        proxy_list_parser::parse_csv(&body)
    } else if content_type.starts_with("application/json") {
        proxy_list_parser::parse_json(&body)
    } else {
        return Err(ApiError::bad_request(
            "Content-Type must be text/csv or application/json",
        ));
    }
    .map_err(ApiError::bad_request)?;

    let request = CreateProxyCheckJobRequest { entries };

    let use_case = SubmitProxyCheckJobUseCase::new(
        state.proxy_check_job_repository.clone(),
        state.id_generator,
        state.audit_log_entry_repository.clone(),
    );

    let job = use_case
        .execute(request, authenticated_user)
        .await
        .map_err(|err| match err {
            SubmitProxyCheckJobError::QuotaExceeded { limit } => {
                ApiError::too_many_requests(format!(
                "daily proxy check job limit reached: at most {limit} jobs may be submitted per day"
            ))
            }
            SubmitProxyCheckJobError::InvalidInput(msg) => ApiError::bad_request(msg),
            SubmitProxyCheckJobError::DomainError(err) => ApiError::bad_request(err.to_string()),
            SubmitProxyCheckJobError::RepoError(err) => {
                ApiError::internal_with_source("failed to submit proxy check job", err)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ProxyCheckJobHttpResponse::from(job)),
    ))
}
