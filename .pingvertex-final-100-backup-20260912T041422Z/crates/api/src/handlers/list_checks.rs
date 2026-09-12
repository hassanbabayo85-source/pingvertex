use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::info;

use application::ports::check_repository::CheckFilters;
use application::use_cases::ListChecksUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::CheckResponse,
    state::AppState,
    validation::{parse_check_status, parse_datetime, parse_monitor_id, validate_pagination},
};

#[derive(Deserialize)]
pub struct ListChecksQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_offset")]
    offset: u32,
    monitor_id: Option<String>,
    status: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

fn default_limit() -> u32 {
    10
}
fn default_offset() -> u32 {
    0
}

fn parse_filters(query: &ListChecksQuery) -> Result<CheckFilters, ApiError> {
    let monitor_id = match &query.monitor_id {
        Some(raw) => Some(parse_monitor_id(raw)?),
        None => None,
    };

    let status = match &query.status {
        Some(raw) => Some(parse_check_status(raw)?),
        None => None,
    };

    let from = match &query.from {
        Some(raw) => Some(parse_datetime(raw)?),
        None => None,
    };

    let to = match &query.to {
        Some(raw) => Some(parse_datetime(raw)?),
        None => None,
    };

    Ok(CheckFilters {
        monitor_id,
        status,
        from,
        to,
    })
}

pub async fn list_checks(
    Query(query): Query<ListChecksQuery>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_checks");

    if let Err(err) = validate_pagination(query.limit, query.offset) {
        return err.into_response();
    }

    let filters = match parse_filters(&query) {
        Ok(filters) => filters,
        Err(err) => return err.into_response(),
    };

    let use_case = ListChecksUseCase::new(
        state.check_repository.clone(),
        state.diagnostic_report_repository.clone(),
    );

    match use_case
        .execute(query.limit, query.offset, authenticated_user, filters)
        .await
    {
        Ok(checks) => {
            let response: Vec<CheckResponse> =
                checks.into_iter().map(CheckResponse::from).collect();
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(err) => ApiError::internal_with_source("failed to list checks", err).into_response(),
    }
}
