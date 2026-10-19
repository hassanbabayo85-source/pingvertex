use std::time::Duration;

use application::ports::SubmitAgentResultOutcome;
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::StatusCode,
    routing::post,
};
use chrono::Utc;
use domain::shared::ids::{CheckId, MonitorId};
use domain::{Check, CheckStatus, ErrorType, Monitor, RegionCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, middleware::agent_auth::AuthenticatedAgent, state::AppState};

fn env_positive_u32(key: &str, default: u32, max: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(max))
        .unwrap_or(default)
}

fn agent_claim_limit() -> u32 {
    env_positive_u32("SCHEDULER_AGENT_CLAIM_LIMIT", 100, 500)
}

fn max_concurrency_per_tenant() -> usize {
    env_positive_u32("SCHEDULER_MAX_CONCURRENCY_PER_TENANT", 4, 500) as usize
}

fn due_claim_limit_per_tenant() -> usize {
    env_positive_u32("SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT", 4, 500) as usize
}

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
    /// Retained because the agent wire contract still sends it. Task 8F
    /// removed the only reader (the PAYG finalize path). The field must
    /// stay on the struct so deserialization continues to accept the
    /// agent's existing payload; if/when the agent protocol is bumped,
    /// remove both sides in lockstep.
    #[allow(dead_code)]
    attempted: bool,
    country_code: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agents/{agent_id}/checks/claim", post(claim_checks))
        .route("/agents/{agent_id}/results", post(submit_result))
}

async fn claim_checks(
    State(state): State<AppState>,
    Extension(agent): Extension<AuthenticatedAgent>,
) -> Result<Json<ClaimResponse>, ApiError> {
    // `agent_id` and region come from the SERVER-SIDE identity
    // (`AuthenticatedAgent`), which the `agent_auth` middleware
    // constructed from the certificate CN + registry lookup. Neither
    // value can be influenced by anything the client sent.
    let agent_id = agent.agent_id.clone();
    let agent_region = agent.region.clone();

    let mut checks = Vec::new();

    if let Some(lease_repo) = &state.agent_lease_repository {
        let leases = lease_repo
            .claim_due_checks_with_limits(
                &agent_id,
                &agent_region,
                agent_claim_limit(),
                Duration::from_secs(420),
                Utc::now(),
                max_concurrency_per_tenant(),
                due_claim_limit_per_tenant(),
            )
            .await
            .map_err(|err| ApiError::internal_with_source("failed to claim agent checks", err))?;

        for lease in leases {
            let monitor = state
                .monitor_repository
                .find_by_id(lease.monitor_id)
                .await
                .map_err(|err| {
                    ApiError::internal_with_source("failed to load claimed monitor", err)
                })?
                .ok_or_else(|| ApiError::not_found("claimed monitor no longer exists"))?;

            // Task 8F — PAYG metering retired; agent-region checks are free.

            checks.push(ClaimedCheck {
                check_id: lease.check_id.0,
                monitor,
                region: lease.region,
            });
        }
    } else {
        // Never expose an unmetered agent fallback. Agent execution is a
        // production billing boundary and requires durable leases.
        return Err(ApiError::internal(
            "agent lease repository is not configured",
        ));
    }

    Ok(Json(ClaimResponse { checks }))
}

async fn submit_result(
    State(state): State<AppState>,
    Extension(agent): Extension<AuthenticatedAgent>,
    Json(payload): Json<SubmitResultRequest>,
) -> Result<StatusCode, ApiError> {
    let agent_id = agent.agent_id.clone();
    let region = agent.region.clone();

    // Defense-in-depth: if the body carries a region claim (agents
    // currently do, for backward compatibility), it MUST match the
    // server-side registered region. A mismatch is an identity-tampering
    // attempt and is refused rather than silently coerced.
    let body_region =
        RegionCode::new(payload.region).map_err(|err| ApiError::bad_request(err.to_string()))?;
    if body_region != region {
        return Err(ApiError::forbidden("agent region mismatch"));
    }

    let check = Check::new(
        CheckId(payload.check_id),
        MonitorId(payload.monitor_id),
        region.clone(),
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
    )
    .with_country_code(payload.country_code);

    if let Some(lease_repo) = &state.agent_lease_repository {
        let outcome = lease_repo
            .submit_result_with_check(
                &agent_id,
                CheckId(payload.check_id),
                MonitorId(payload.monitor_id),
                &region,
                Utc::now(),
                &check,
            )
            .await
            .map_err(|err| ApiError::internal_with_source("failed to validate agent lease", err))?;

        match outcome {
            SubmitAgentResultOutcome::Accepted => {
                // Task 8F — no billing; the result is already durable.
            }
            SubmitAgentResultOutcome::Duplicate => return Ok(StatusCode::ACCEPTED),
            SubmitAgentResultOutcome::Stale => {
                return Err(ApiError::Conflict(
                    "stale or reassigned agent check lease".into(),
                ));
            }
        }
    } else {
        // Never accept an agent result without a durable lease/billing
        // boundary; otherwise an agent could bypass PAYG metering.
        return Err(ApiError::internal(
            "agent lease repository is not configured",
        ));
    }

    Ok(StatusCode::ACCEPTED)
}
