use std::{error::Error, sync::Arc};

use application::ports::{ApiFlowRepository, CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind, RegionCode};
use tracing::{error, info};

use crate::durable_scheduler::CheckExecution;
use crate::flow_executor::{FlowExecutionResult, execute_flow};
use crate::incident_evaluator::IncidentEvaluator;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// API Flow executor for the `DurableScheduler`. Executes a monitor
/// only when `monitor.kind == MonitorKind::Flow`, fetching the monitor's
/// `ApiFlow` and running every step via `flow_executor::execute_flow`
/// (SSRF-guarded independently per step, fail-fast, 60s total budget, 10
/// step cap — see `flow_executor.rs` and `domain::entities::ApiFlow`).
#[derive(Clone)]
pub struct FlowCheckExecutor {
    api_flow_repository: Arc<dyn ApiFlowRepository>,
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
    http_client: reqwest::Client,
}

impl FlowCheckExecutor {
    pub fn new(
        api_flow_repository: Arc<dyn ApiFlowRepository>,
        check_repository: Arc<dyn CheckRepository>,
        id_generator: Arc<dyn IdGenerator>,
        incident_evaluator: IncidentEvaluator,
    ) -> Self {
        Self {
            api_flow_repository,
            check_repository,
            id_generator,
            incident_evaluator,
            // `execute_flow`'s `http_client` parameter is unused (each step
            // pins its own SSRF-validated client — see flow_executor.rs);
            // kept only so this executor's shape matches Http/Tcp/Dns.
            http_client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("building a redirect-disabled reqwest client never fails"),
        }
    }
}

#[async_trait]
impl crate::durable_scheduler::CheckExecutor for FlowCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Flow {
            return Ok(CheckExecution::failed(
                false,
                format!(
                    "FlowCheckExecutor received unsupported monitor kind {:?}",
                    monitor.kind
                ),
            ));
        }

        let flow = match self
            .api_flow_repository
            .find_by_monitor_id(monitor.id)
            .await
        {
            Ok(Some(flow)) => flow,
            Ok(None) => {
                return Ok(CheckExecution::failed(
                    false,
                    format!(
                        "monitor {} is kind=Flow but has no api_flow row",
                        monitor.id.0
                    ),
                ));
            }
            Err(err) => {
                return Ok(CheckExecution::failed(
                    false,
                    format!("failed to load flow: {err}"),
                ));
            }
        };

        let result = execute_flow(&flow, &self.http_client).await;
        let attempted = result.outcomes.iter().any(|o| o.attempted);
        let egress_bytes = result
            .outcomes
            .iter()
            .map(|outcome| outcome.egress_bytes)
            .sum();

        let check_id = self.id_generator.next_check_id().await;
        let check = build_check(check_id, monitor, &result);

        if let Err(err) = self.check_repository.save(&check).await {
            return Ok(CheckExecution::failed(
                attempted,
                format!("failed to persist check: {err}"),
            ));
        }

        info!(
            monitor_id = %monitor.id.0,
            monitor_name = %monitor.name.as_str(),
            check_id = %check.id.0,
            success = result.passed,
            failed_at_step = ?result.failed_at_step,
            latency_ms = result.total_duration_ms,
            "flow check persisted"
        );

        if let Err(err) = self
            .incident_evaluator
            .evaluate(monitor, result.passed)
            .await
        {
            error!(monitor_id = %monitor.id.0, error = %err, "failed to evaluate incident state");
        }

        // GAP 3 Kashi B: bill only the steps whose HTTP request actually
        // started. `attempted = true` on a step means the request was sent
        // (even if it later failed or timed out); SSRF-blocked steps have
        // `attempted = false` and consume no network resources.
        let steps_executed = result.outcomes.iter().filter(|o| o.attempted).count() as i64;

        Ok(CheckExecution {
            attempted,
            error: None,
            egress_bytes,
            steps_executed: Some(steps_executed),
        })
    }
}

fn build_check(
    check_id: domain::shared::ids::CheckId,
    monitor: &Monitor,
    result: &FlowExecutionResult,
) -> Check {
    let status = if result.passed {
        CheckStatus::Up
    } else {
        CheckStatus::Down
    };

    let failing_outcome = result.outcomes.iter().find(|o| !o.passed);

    let status_code = failing_outcome
        .and_then(|o| o.status_code)
        .or_else(|| result.outcomes.last().and_then(|o| o.status_code));

    let error_type = failing_outcome
        .and_then(|o| o.error.as_deref())
        .map(|err| classify_flow_error(err, status_code))
        .unwrap_or(ErrorType::None);

    let error_message = failing_outcome.map(|o| {
        format!(
            "step {} ({}): {}",
            o.step_order,
            o.step_name,
            o.error.clone().unwrap_or_default()
        )
    });

    Check::new(
        check_id,
        monitor.id,
        RegionCode::default_region(),
        status,
        status_code,
        result.total_duration_ms,
        error_type,
        error_message,
    )
}

/// Flow step failures carry a plain `String` error (see
/// `flow_executor::FlowStepOutcome`), not `checker.rs`'s `CheckErrorKind`
/// — a flow step can fail for reasons (placeholder resolution, SSRF
/// block) the single-request taxonomy doesn't model. This is a best-effort
/// text classification onto the persisted `ErrorType` column, matching
/// `check_error_mapping::map_check_error_kind_to_error_type`'s categories
/// as closely as the available information allows.
fn classify_flow_error(error: &str, status_code: Option<u16>) -> ErrorType {
    let lower = error.to_lowercase();

    if lower.contains("ssrf blocked")
        || lower.contains("failed to resolve")
        || lower.contains("did not resolve")
    {
        ErrorType::Dns
    } else if lower.contains("timeout") || lower.contains("timed out") {
        ErrorType::Timeout
    } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        ErrorType::Ssl
    } else if let Some(status) = status_code {
        if (400..500).contains(&status) {
            ErrorType::Http4xx
        } else if status >= 500 {
            ErrorType::Http5xx
        } else {
            ErrorType::Unknown
        }
    } else {
        ErrorType::Unknown
    }
}
