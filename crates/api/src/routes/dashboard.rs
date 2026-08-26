use application::{dto::DashboardOverview, use_cases::GetDashboardOverviewUseCase};
use axum::{
    Json, Router,
    extract::{Extension, State},
    routing::get,
};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState};

/// Handler for `GET /api/v1/dashboard/overview`.
///
/// Authenticated by `jwt_auth` middleware; the caller's `TenantContext`
/// is passed to the use case, which delegates to the `DashboardRepository`
/// for efficient, tenant-scoped aggregate queries. No N+1 here:
/// the Postgres implementation uses LATERAL joins to fetch each monitor's
/// latest check and open incident in a single round-trip, plus one
/// additional query for recent incidents.
pub async fn dashboard_overview(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<DashboardOverview>, ApiError> {
    let use_case = GetDashboardOverviewUseCase::new(state.dashboard_repository.clone());

    let overview = use_case
        .execute(authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch dashboard overview", err))?;

    Ok(Json(overview))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard/overview", get(dashboard_overview))
}
