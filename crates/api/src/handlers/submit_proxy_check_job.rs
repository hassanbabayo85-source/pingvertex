use axum::{
    extract::{Extension, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    Json,
};
use tracing::info;

use application::dto::CreateProxyCheckJobRequest;
use application::parsing::proxy_list_parser;
use application::use_cases::{SubmitProxyCheckJobError, SubmitProxyCheckJobUseCase};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::proxy_check_types::ProxyCheckJobHttpResponse, state::AppState,
};

/// `POST /proxy-checks`
///
/// Accepts a bulk list of proxies as either CSV or JSON, distinguished by
/// the request's `Content-Type` header (there is no other precedent for
/// format-sniffing anywhere else in this codebase, and no wrapper JSON
/// field or separate-endpoint convention exists either — this was a
/// deliberate, user-confirmed choice for this endpoint specifically):
///   - `text/csv`         -> `proxy_list_parser::parse_csv`
///   - `application/json` -> `proxy_list_parser::parse_json`
/// Any other (or missing) `Content-Type` is rejected as a bad request.
///
/// The raw body is extracted as a `String` (not `Json<...>`), since the
/// CSV case is not JSON at all and the JSON case still needs to be handed
/// to `proxy_list_parser::parse_json` as a raw string (that function does
/// its own size-limit check *before* calling `serde_json::from_str`).
pub async fn submit_proxy_check_job(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
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

    let request = CreateProxyCheckJobRequest { user_id, entries };

    let use_case = SubmitProxyCheckJobUseCase::new(
        state.proxy_check_job_repository.clone(),
        state.id_generator,
    );

    let job = use_case.execute(request).await.map_err(|err| match err {
        SubmitProxyCheckJobError::QuotaExceeded { limit } => ApiError::too_many_requests(format!(
            "daily proxy check job limit reached: at most {limit} jobs may be submitted per day"
        )),
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
