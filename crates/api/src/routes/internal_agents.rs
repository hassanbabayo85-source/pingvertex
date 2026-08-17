use application::ports::IdGenerator;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use domain::shared::ids::{CheckId, MonitorId};
use domain::{Check, CheckStatus, ErrorType, Monitor, RegionCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiError,
    middleware::agent_auth::{AgentId, AgentRegion},
    state::AppState,
};

#[derive(Debug, Serialize)]
struct ClaimResponse {
    checks: Vec<ClaimedCheck>,
}

#[derive(Debug, Serialize)]
struct ClaimedCheck {
    check_id: Uuid,
    monitor: Monitor,
    region: RegionCode,
}

#[derive(Debug, Deserialize)]
struct SubmitResultRequest {
    check_id: Uuid,
    monitor_id: Uuid,
    region: String,
    success: bool,
    status_code: Option<u16>,
    latency_ms: u128,
    error: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agents/{agent_id}/checks/claim", post(claim_checks))
        .route("/agents/{agent_id}/results", post(submit_result))
}

async fn claim_checks(
    State(state): State<AppState>,
    Path(_agent_id): Path<String>,
    Extension(_agent): Extension<AgentId>,
    Extension(agent_region): Extension<AgentRegion>,
) -> Result<Json<ClaimResponse>, ApiError> {
    let monitors = state.monitor_repository.find_all().await.map_err(|err| {
        ApiError::internal_with_source("failed to list monitors for agent claim", err)
    })?;

    let mut checks = Vec::new();

    for monitor in monitors {
        if monitor
            .regions
            .iter()
            .any(|region| region == &agent_region.0)
        {
            let check_id = state.id_generator.next_check_id().await.0;

            checks.push(ClaimedCheck {
                check_id,
                monitor,
                region: agent_region.0.clone(),
            });
        }
    }

    Ok(Json(ClaimResponse { checks }))
}

async fn submit_result(
    State(state): State<AppState>,
    Path(_agent_id): Path<String>,
    Extension(_agent): Extension<AgentId>,
    Extension(_agent_region): Extension<AgentRegion>,
    Json(payload): Json<SubmitResultRequest>,
) -> Result<StatusCode, ApiError> {
    let region =
        RegionCode::new(payload.region).map_err(|err| ApiError::bad_request(err.to_string()))?;

    let check = Check::new(
        CheckId(payload.check_id),
        MonitorId(payload.monitor_id),
        region,
        if payload.success {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        },
        payload.status_code,
        payload.latency_ms,
        if payload.success {
            ErrorType::None
        } else {
            ErrorType::Unknown
        },
        payload.error,
    );

    state.check_repository.save(&check).await.map_err(|err| {
        ApiError::internal_with_source("failed to persist agent check result", err)
    })?;

    Ok(StatusCode::ACCEPTED)
}
