use std::{collections::HashMap, time::Duration};

use application::ports::SubmitAgentResultOutcome;
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::post,
};
use chrono::Utc;
use domain::shared::ids::{CheckId, MonitorId};
use domain::{Check, CheckStatus, ErrorType, Monitor, RegionCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiError,
    middleware::agent_auth::{AgentId, AgentRegion},
    state::AppState,
};

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
    Path(agent_id): Path<String>,
    Extension(_agent): Extension<AgentId>,
    Extension(agent_region): Extension<AgentRegion>,
) -> Result<Json<ClaimResponse>, ApiError> {
    let mut checks = Vec::new();
    let mut plan_cache: HashMap<Uuid, domain::Plan> = HashMap::new();

    if let Some(lease_repo) = &state.agent_lease_repository {
        let leases = lease_repo
            .claim_due_checks_with_limits(
                &agent_id,
                &agent_region.0,
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

            let wallet = state
                .wallet_repository
                .get_or_create(monitor.tenant_id.0, "USD")
                .await
                .map_err(|err| {
                    ApiError::internal_with_source("failed to load monitor wallet", err)
                })?;

            let plan = if let Some(plan) = plan_cache.get(&monitor.tenant_id.0) {
                *plan
            } else {
                let config = state
                    .pricing_config_repository
                    .find_active_config()
                    .await
                    .map_err(|err| {
                        ApiError::internal_with_source(
                            "failed to load PAYG pricing config for agent scheduler",
                            err,
                        )
                    })?
                    .ok_or_else(|| ApiError::internal("active PAYG pricing config is missing"))?;
                let mut plan = domain::Plan::for_tier(domain::enums::PlanTier::Free);
                plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                plan.history_retention_days = config.free_tier.retention_days;
                plan_cache.insert(monitor.tenant_id.0, plan);
                plan
            };

            // A never-funded tenant may execute the resolved Free entitlement.
            // Once PAYG has been activated, every agent-region execution is paid.
            if wallet.payg_activated_at.is_some()
                || monitor.interval_seconds < plan.min_check_interval_seconds
            {
                let metering = state
                    .payg_metering
                    .as_ref()
                    .ok_or_else(|| ApiError::internal("PAYG metering is not configured"))?
                    .clone();
                let operation = domain::monitor_operation(monitor.kind, false);
                let request_id = format!("agent-check:{}", lease.check_id.0);
                let reservation = match metering
                    .reserve_for_execution(application::MeterUsageInput {
                        tenant_id: monitor.tenant_id.0,
                        operation: operation.as_str().to_owned(),
                        units: 1,
                        request_id,
                        monitor_id: Some(monitor.id.0),
                        execution_id: Some(lease.check_id.0),
                        pricing_context: Some(application::ports::PricingContext {
                            account_type: None,
                            region: Some(agent_region.0.clone()),
                            monitor_kind: Some(monitor.kind),
                            quantity: 1,
                        }),
                    })
                    .await
                {
                    Ok(reservation) => reservation,
                    Err(application::ChargeForUsageError::InsufficientBalance) => {
                        if let Err(error) = state
                            .monitor_repository
                            .set_billing_block(
                                monitor.id,
                                monitor.tenant_id,
                                "Insufficient wallet balance",
                            )
                            .await
                        {
                            tracing::warn!(error=%error, monitor_id=%monitor.id.0, "failed to persist agent billing block");
                        }
                        continue;
                    }
                    Err(application::ChargeForUsageError::SpendLimitExceeded) => {
                        if let Err(error) = state
                            .monitor_repository
                            .set_billing_block(
                                monitor.id,
                                monitor.tenant_id,
                                "PAYG spend limit exceeded",
                            )
                            .await
                        {
                            tracing::warn!(error=%error, monitor_id=%monitor.id.0, "failed to persist agent spend-limit block");
                        }
                        continue;
                    }
                    Err(err) => return Err(ApiError::from_payg_error(err)),
                };

                if let Err(err) = state
                    .agent_lease_repository
                    .as_ref()
                    .expect("agent lease repository is configured")
                    .mark_billing_reserved(lease.check_id)
                    .await
                {
                    let _ = state
                        .wallet_repository
                        .release(reservation.reservation.id)
                        .await;
                    return Err(ApiError::internal_with_source(
                        "failed to mark PAYG reservation on agent lease",
                        err,
                    ));
                }
            }

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
    Path(agent_id): Path<String>,
    Extension(_agent): Extension<AgentId>,
    Extension(_agent_region): Extension<AgentRegion>,
    Json(payload): Json<SubmitResultRequest>,
) -> Result<StatusCode, ApiError> {
    let region =
        RegionCode::new(payload.region).map_err(|err| ApiError::bad_request(err.to_string()))?;

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
                let monitor = state
                    .monitor_repository
                    .find_by_id(MonitorId(payload.monitor_id))
                    .await
                    .map_err(|err| {
                        ApiError::internal_with_source(
                            "failed to load agent monitor for billing",
                            err,
                        )
                    })?
                    .ok_or_else(|| ApiError::not_found("monitor not found"))?;

                let billable = state
                    .agent_lease_repository
                    .as_ref()
                    .expect("agent lease repository is configured")
                    .billing_reserved(CheckId(payload.check_id))
                    .await
                    .map_err(|err| {
                        ApiError::internal_with_source("failed to load agent billing state", err)
                    })?;

                if billable {
                    let metering = state
                        .payg_metering
                        .as_ref()
                        .ok_or_else(|| ApiError::internal("PAYG metering is not configured"))?
                        .clone();
                    let operation = domain::monitor_operation(monitor.kind, false);
                    let request_id = format!("agent-check:{}", payload.check_id);
                    match metering
                        .finalize_reserved_attempt(
                            application::MeterUsageInput {
                                tenant_id: monitor.tenant_id.0,
                                operation: operation.as_str().to_owned(),
                                units: 1,
                                request_id,
                                monitor_id: Some(monitor.id.0),
                                // The durable agent check ID is the execution identity
                                // for this metered agent-region attempt.
                                execution_id: Some(payload.check_id),
                                pricing_context: None,
                            },
                            payload.attempted,
                        )
                        .await
                    {
                        Ok(_) | Err(application::ChargeForUsageError::OperationNotAttempted(_)) => {
                        }
                        Err(err) => return Err(ApiError::from_payg_error(err)),
                    }
                }
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
