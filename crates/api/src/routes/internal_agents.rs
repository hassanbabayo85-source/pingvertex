use std::time::Duration;

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

    if let Some(lease_repo) = &state.agent_lease_repository {
        let leases = lease_repo
            .claim_due_checks(
                &agent_id,
                &agent_region.0,
                100,
                Duration::from_secs(120),
                Utc::now(),
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

            // A never-funded tenant may execute the Free entitlement. Once
            // PAYG has been activated, every agent-region execution is paid.
            if wallet.payg_activated_at.is_some()
                || monitor.interval_seconds < domain::FREE_MIN_INTERVAL_SECONDS
            {
                let operation = domain::monitor_operation(monitor.kind, false);
                let pricing = state
                    .payg_pricing_repository
                    .find_active(operation.as_str())
                    .await
                    .map_err(|err| {
                        ApiError::internal_with_source("failed to load PAYG price", err)
                    })?
                    .ok_or_else(|| ApiError::internal("PAYG pricing is unavailable"))?;

                let request_id = format!("agent-check:{}", lease.check_id.0);
                let reservation = match state
                    .wallet_repository
                    .reserve(
                        monitor.tenant_id.0,
                        pricing.unit_price,
                        operation.as_str(),
                        pricing.unit_price,
                        pricing.version,
                        &pricing.currency,
                        &request_id,
                        Utc::now() + Duration::from_secs(180),
                    )
                    .await
                {
                    Ok(reservation) => reservation,
                    Err(err) if err.to_string() == "insufficient wallet balance" => {
                        // Leave the short agent lease to expire. This prevents
                        // an exhausted tenant from blocking unrelated tenants
                        // in the same claim batch.
                        continue;
                    }
                    Err(err) => {
                        return Err(ApiError::internal_with_source(
                            "failed to reserve PAYG credit for agent check",
                            err,
                        ));
                    }
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
