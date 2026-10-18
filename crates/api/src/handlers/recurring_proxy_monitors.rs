//! HTTP handlers for recurring proxy monitors.

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use application::dto::{
    CreateRecurringProxyMonitorRequest, RecurringProxyCheckResponse,
    RecurringProxyMonitorResponse, UpdateRecurringProxyMonitorRequest,
};
use application::use_cases::{
    CreateRecurringProxyMonitorError, CreateRecurringProxyMonitorUseCase,
    DeleteRecurringProxyMonitorError, DeleteRecurringProxyMonitorUseCase,
    GetLatestRecurringProxyCheckError, GetLatestRecurringProxyCheckUseCase,
    GetRecurringProxyMonitorUseCase, ListRecurringProxyChecksError,
    ListRecurringProxyChecksUseCase, ListRecurringProxyMonitorsUseCase, SetEnabledError,
    SetEnabledUseCase, UpdateRecurringProxyMonitorError, UpdateRecurringProxyMonitorUseCase,
};
use domain::shared::types::AuthenticatedUser;

use crate::error::ApiError;
use crate::state::AppState;

// ── Helpers ────────────────────────────────────────────────────────

fn parse_monitor_uuid(raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ApiError::bad_request("invalid recurring proxy monitor id"))
}

#[derive(Debug, Deserialize)]
pub struct ChecksListQuery {
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ChecksListResponse {
    pub checks: Vec<RecurringProxyCheckResponse>,
    pub total: u64,
}

// ── Handlers ───────────────────────────────────────────────────────

pub async fn create_recurring_proxy_monitor(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(body): Json<CreateRecurringProxyMonitorRequest>,
) -> Result<(StatusCode, Json<RecurringProxyMonitorResponse>), ApiError> {
    let uc = CreateRecurringProxyMonitorUseCase::new(
        state.recurring_proxy_monitor_repository.clone(),
        state.entitlement_resolver.clone(),
    );

    let response = uc.execute(body, user).await.map_err(|err| match err {
        CreateRecurringProxyMonitorError::Forbidden => {
            ApiError::forbidden("you cannot create recurring proxy monitors")
        }
        CreateRecurringProxyMonitorError::PlanLimitExceeded(pl) => ApiError::from_plan_limit(pl),
        CreateRecurringProxyMonitorError::InvalidHost(m) => ApiError::bad_request(m),
        CreateRecurringProxyMonitorError::InvalidCredentials(m) => ApiError::bad_request(m),
        CreateRecurringProxyMonitorError::Other(e) => {
            ApiError::internal_with_source("failed to create recurring proxy monitor", e)
        }
    })?;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn list_recurring_proxy_monitors(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<RecurringProxyMonitorResponse>>, ApiError> {
    let uc = ListRecurringProxyMonitorsUseCase::new(
        state.recurring_proxy_monitor_repository.clone(),
        state.entitlement_resolver.clone(),
    );

    let monitors = uc.execute(user).await.map_err(|e| {
        ApiError::forbidden_or_internal("failed to list recurring proxy monitors", e)
    })?;

    Ok(Json(monitors))
}

pub async fn get_recurring_proxy_monitor(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<RecurringProxyMonitorResponse>, ApiError> {
    let monitor_id = parse_monitor_uuid(&id)?;
    let uc = GetRecurringProxyMonitorUseCase::new(
        state.recurring_proxy_monitor_repository.clone(),
        state.entitlement_resolver.clone(),
    );

    let monitor = uc
        .execute(monitor_id, user)
        .await
        .map_err(|e| ApiError::forbidden_or_internal("failed to get recurring proxy monitor", e))?;

    monitor
        .map(Json)
        .ok_or_else(|| ApiError::not_found("recurring proxy monitor not found"))
}

pub async fn update_recurring_proxy_monitor(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Json(body): Json<UpdateRecurringProxyMonitorRequest>,
) -> Result<Json<RecurringProxyMonitorResponse>, ApiError> {
    let monitor_id = parse_monitor_uuid(&id)?;
    let uc = UpdateRecurringProxyMonitorUseCase::new(
        state.recurring_proxy_monitor_repository.clone(),
        state.entitlement_resolver.clone(),
    );

    let response = uc
        .execute(monitor_id, body, user)
        .await
        .map_err(|err| match err {
            UpdateRecurringProxyMonitorError::Forbidden => ApiError::forbidden("forbidden"),
            UpdateRecurringProxyMonitorError::NotFound => {
                ApiError::not_found("recurring proxy monitor not found")
            }
            UpdateRecurringProxyMonitorError::PlanLimitExceeded(pl) => {
                ApiError::from_plan_limit(pl)
            }
            UpdateRecurringProxyMonitorError::Other(e) => {
                ApiError::internal_with_source("failed to update recurring proxy monitor", e)
            }
        })?;

    Ok(Json(response))
}

pub async fn enable_recurring_proxy_monitor(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<RecurringProxyMonitorResponse>, ApiError> {
    set_enabled_impl(state, user, &id, true).await
}

pub async fn disable_recurring_proxy_monitor(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<RecurringProxyMonitorResponse>, ApiError> {
    set_enabled_impl(state, user, &id, false).await
}

async fn set_enabled_impl(
    state: AppState,
    user: AuthenticatedUser,
    id: &str,
    enabled: bool,
) -> Result<Json<RecurringProxyMonitorResponse>, ApiError> {
    let monitor_id = parse_monitor_uuid(id)?;
    let uc = SetEnabledUseCase::new(state.recurring_proxy_monitor_repository.clone());

    let response = uc
        .execute(monitor_id, enabled, user)
        .await
        .map_err(|err| match err {
            SetEnabledError::Forbidden => ApiError::forbidden("forbidden"),
            SetEnabledError::NotFound => {
                ApiError::not_found("recurring proxy monitor not found")
            }
            SetEnabledError::Other(e) => {
                ApiError::internal_with_source("failed to update recurring proxy monitor state", e)
            }
        })?;

    Ok(Json(response))
}

pub async fn delete_recurring_proxy_monitor(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let monitor_id = parse_monitor_uuid(&id)?;
    let uc =
        DeleteRecurringProxyMonitorUseCase::new(state.recurring_proxy_monitor_repository.clone());

    uc.execute(monitor_id, user).await.map_err(|err| match err {
        DeleteRecurringProxyMonitorError::Forbidden => ApiError::forbidden("forbidden"),
        DeleteRecurringProxyMonitorError::NotFound => {
            ApiError::not_found("recurring proxy monitor not found")
        }
        DeleteRecurringProxyMonitorError::Other(e) => {
            ApiError::internal_with_source("failed to delete recurring proxy monitor", e)
        }
    })?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_recurring_proxy_checks(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
    Query(query): Query<ChecksListQuery>,
) -> Result<Json<ChecksListResponse>, ApiError> {
    let monitor_id = parse_monitor_uuid(&id)?;
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let uc =
        ListRecurringProxyChecksUseCase::new(state.recurring_proxy_monitor_repository.clone());

    let result = uc
        .execute(monitor_id, offset, limit, user)
        .await
        .map_err(|err| match err {
            ListRecurringProxyChecksError::Forbidden => ApiError::forbidden("forbidden"),
            ListRecurringProxyChecksError::NotFound => {
                ApiError::not_found("recurring proxy monitor not found")
            }
            ListRecurringProxyChecksError::Other(e) => {
                ApiError::internal_with_source("failed to list recurring proxy checks", e)
            }
        })?;

    Ok(Json(ChecksListResponse {
        checks: result.checks,
        total: result.total,
    }))
}

pub async fn get_latest_recurring_proxy_check(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<Option<RecurringProxyCheckResponse>>, ApiError> {
    let monitor_id = parse_monitor_uuid(&id)?;
    let uc =
        GetLatestRecurringProxyCheckUseCase::new(state.recurring_proxy_monitor_repository.clone());

    let latest = uc.execute(monitor_id, user).await.map_err(|err| match err {
        GetLatestRecurringProxyCheckError::Forbidden => ApiError::forbidden("forbidden"),
        GetLatestRecurringProxyCheckError::NotFound => {
            ApiError::not_found("recurring proxy monitor not found")
        }
        GetLatestRecurringProxyCheckError::Other(e) => {
            ApiError::internal_with_source("failed to get latest recurring proxy check", e)
        }
    })?;

    Ok(Json(latest))
}
