use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tracing::info;

use application::use_cases::ListChecksUseCase;

use crate::{error::ApiError, state::AppState, validation::validate_pagination};

/// Query parameters for listing checks.
#[derive(Deserialize)]
pub struct ListChecksQuery {
    /// Number of checks to retrieve per page.
    #[serde(default = "default_limit")]
    limit: u32,
    /// Number of checks to skip before starting to retrieve results.
    #[serde(default = "default_offset")]
    offset: u32,
}

/// Default limit for pagination (10).
fn default_limit() -> u32 {
    10
}
/// Default offset for pagination (0).
fn default_offset() -> u32 {
    0
}

/// Lists all checks.
pub async fn list_checks(
    Query(query): Query<ListChecksQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_checks");

    if let Err(err) = validate_pagination(query.limit, query.offset) {
        return err.into_response();
    }

    let use_case = ListChecksUseCase::new(state.check_repository.clone());

    match use_case.execute(query.limit, query.offset).await {
        Ok(checks) => (StatusCode::OK, Json(checks)).into_response(),
        Err(err) => ApiError::internal(format!("failed to list checks: {err}")).into_response(),
    }
}
