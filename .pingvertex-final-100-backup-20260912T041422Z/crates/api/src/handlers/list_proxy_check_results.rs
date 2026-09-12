use crate::{
    error::ApiError,
    handlers::proxy_check_types::{ProxyCheckResultHttpResponse, ProxyCheckResultsHttpResponse},
    state::AppState,
    validation::parse_proxy_check_job_id,
};
use application::ports::ProxyResultStatusFilter;
use application::use_cases::ListProxyCheckResultsUseCase;
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use domain::ProxyScheme;
use domain::shared::types::AuthenticatedUser;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProxyResultsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub status: Option<String>,
    pub scheme: Option<String>,
    pub working_only: Option<bool>,
}

fn parse_status(raw: Option<&str>) -> Result<Option<ProxyResultStatusFilter>, ApiError> {
    match raw.map(|s| s.to_ascii_lowercase()) {
        None => Ok(None),
        Some(s) if s == "working" || s == "successful" => {
            Ok(Some(ProxyResultStatusFilter::Working))
        }
        Some(s) if s == "failed" || s == "unreachable" => Ok(Some(ProxyResultStatusFilter::Failed)),
        Some(_) => Err(ApiError::bad_request("status must be working or failed")),
    }
}
fn parse_scheme(raw: Option<&str>) -> Result<Option<ProxyScheme>, ApiError> {
    match raw.map(|s| s.to_ascii_lowercase()) {
        None => Ok(None),
        Some(s) if s == "http" => Ok(Some(ProxyScheme::Http)),
        Some(s) if s == "https" => Ok(Some(ProxyScheme::Https)),
        Some(s) if s == "socks5" || s == "socks" => Ok(Some(ProxyScheme::Socks5)),
        Some(_) => Err(ApiError::bad_request(
            "scheme must be http, https, or socks5",
        )),
    }
}

pub async fn list_proxy_check_results(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Query(query): Query<ProxyResultsQuery>,
) -> Result<Json<ProxyCheckResultsHttpResponse>, ApiError> {
    let id = parse_proxy_check_job_id(&id)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0);
    let status = parse_status(query.status.as_deref())?;
    let scheme = parse_scheme(query.scheme.as_deref())?;
    let working_only = query.working_only.unwrap_or(false);
    let use_case = ListProxyCheckResultsUseCase::new(state.proxy_check_job_repository.clone());
    let (items, total) = use_case
        .execute(id, user, offset, limit, status, scheme, working_only)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to list proxy check results", e))?
        .ok_or_else(|| ApiError::not_found("proxy check job not found"))?;
    Ok(Json(ProxyCheckResultsHttpResponse {
        items: items
            .into_iter()
            .map(ProxyCheckResultHttpResponse::from)
            .collect(),
        total,
        limit,
        offset,
    }))
}
