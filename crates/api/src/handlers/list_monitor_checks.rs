use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::info;

use application::ports::check_repository::CheckFilters;
use application::use_cases::{ListMonitorChecksError, ListMonitorChecksUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    state::AppState,
    validation::{parse_check_status, parse_datetime, parse_monitor_id, validate_pagination},
};

#[derive(Deserialize)]
pub struct ListMonitorChecksQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_offset")]
    offset: u32,
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

fn parse_filters(query: &ListMonitorChecksQuery) -> Result<CheckFilters, ApiError> {
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
        monitor_id: None,
        status,
        from,
        to,
    })
}

pub async fn list_monitor_checks(
    Path(id): Path<String>,
    Query(query): Query<ListMonitorChecksQuery>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_monitor_checks");

    let monitor_id = match parse_monitor_id(&id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    if let Err(err) = validate_pagination(query.limit, query.offset) {
        return err.into_response();
    }

    let filters = match parse_filters(&query) {
        Ok(filters) => filters,
        Err(err) => return err.into_response(),
    };

    let use_case = ListMonitorChecksUseCase::new(
        state.monitor_repository.clone(),
        state.check_repository.clone(),
    );

    match use_case
        .execute(
            monitor_id,
            authenticated_user,
            query.limit,
            query.offset,
            filters,
        )
        .await
    {
        Ok(checks) => (StatusCode::OK, Json(checks)).into_response(),
        Err(ListMonitorChecksError::NotFound) => {
            ApiError::not_found("monitor not found").into_response()
        }
        Err(ListMonitorChecksError::Other(err)) => {
            ApiError::internal_with_source("failed to list checks", err).into_response()
        }
    }
}
