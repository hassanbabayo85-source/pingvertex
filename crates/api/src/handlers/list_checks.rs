use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tracing::info;

use application::use_cases::ListChecksUseCase;
use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::types::CheckResponse, state::AppState,
    validation::validate_pagination,
};

#[derive(Deserialize)]
pub struct ListChecksQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_offset")]
    offset: u32,
}

fn default_limit() -> u32 {
    10
}
fn default_offset() -> u32 {
    0
}

pub async fn list_checks(
    Query(query): Query<ListChecksQuery>,
    Extension(user_id): Extension<UserId>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_checks");

    if let Err(err) = validate_pagination(query.limit, query.offset) {
        return err.into_response();
    }

    let use_case = ListChecksUseCase::new(
        state.check_repository.clone(),
        state.diagnostic_report_repository.clone(),
    );

    match use_case.execute(query.limit, query.offset, user_id).await {
        Ok(checks) => {
            let response: Vec<CheckResponse> =
                checks.into_iter().map(CheckResponse::from).collect();
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(err) => ApiError::internal_with_source("failed to list checks", err).into_response(),
    }
}
