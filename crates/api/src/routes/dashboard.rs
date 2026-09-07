use application::{
    dto::{DashboardBadges, DashboardOverview},
    use_cases::GetDashboardOverviewUseCase,
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    routing::{get, post},
};
use domain::shared::types::AuthenticatedUser;
use serde::Serialize;

use crate::{error::ApiError, state::AppState};

/// Handler for `GET /api/v1/dashboard/overview`.
///
/// Authenticated by `jwt_auth`; tenant identity is always derived from the
/// authenticated membership and never accepted from request parameters.
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

/// Lightweight counts used by the persistent sidebar/topbar. This avoids
/// downloading monitor rows and trend data on every route change.
pub async fn dashboard_badges(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<DashboardBadges>, ApiError> {
    let badges = state
        .dashboard_repository
        .get_badges(authenticated_user.tenant_context().tenant_id())
        .await
        .map_err(|err| ApiError::internal_with_source("failed to fetch dashboard badges", err))?;
    Ok(Json(badges))
}

#[derive(Debug, Clone, Serialize)]
pub struct RunChecksResponse {
    pub queued: u32,
}

/// Marks all eligible monitors in the authenticated tenant due now. Existing
/// scheduler leases are left untouched; the durable worker remains responsible
/// for claiming/executing checks. This makes the button safe under retries.
pub async fn run_checks_now(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<RunChecksResponse>, ApiError> {
    let queued = state
        .scheduling_repository
        .request_immediate_checks(authenticated_user.tenant_context().tenant_id())
        .await
        .map_err(|err| ApiError::internal_with_source("failed to queue immediate checks", err))?;

    Ok(Json(RunChecksResponse {
        queued: u32::try_from(queued).unwrap_or(u32::MAX),
    }))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard/overview", get(dashboard_overview))
        .route("/dashboard/badges", get(dashboard_badges))
}

pub fn manual_check_router() -> Router<AppState> {
    Router::new().route("/dashboard/checks/now", post(run_checks_now))
}
