use axum::{
    extract::{Extension, Path, State},
    Json,
};
use tracing::info;

use application::{commands::RequestCustomDomain, use_cases::RequestCustomDomainUseCase};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{CustomDomainRequestBody, StatusPageResponse},
    state::AppState,
    validation::parse_status_page_id,
};

/// Begins the custom-domain verification flow for a status page.
///
/// Always leaves the domain in a `Pending` state — see
/// `RequestCustomDomainUseCase` and `StatusPage::request_custom_domain`
/// for why this handler must never mark a domain verified itself.
pub async fn request_custom_domain(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
    Json(payload): Json<CustomDomainRequestBody>,
) -> Result<Json<StatusPageResponse>, ApiError> {
    info!("API: request_custom_domain");

    let id = parse_status_page_id(&id)?;

    let command = RequestCustomDomain {
        id,
        user_id,
        domain: payload.domain,
    };

    let use_case = RequestCustomDomainUseCase::new(state.status_page_repository.clone());

    let status_page = use_case
        .execute(command)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to request custom domain", err))?
        .ok_or_else(|| ApiError::not_found("status page not found"))?;

    Ok(Json(StatusPageResponse::from(status_page)))
}
