use std::{error::Error, sync::Arc};

use application::ports::{CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind};
use tracing::{error, info};

use crate::check_error_mapping::map_check_error_kind_to_error_type;
use crate::durable_scheduler::CheckExecution;
use crate::incident_evaluator::IncidentEvaluator;
use crate::tcp_checker::check_tcp_outcome;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// B19 TCP executor for B17's `DurableScheduler`. Persists a `Check`
/// using the same error taxonomy as the HTTP executor.
#[derive(Clone)]
pub struct TcpCheckExecutor {
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
}

impl TcpCheckExecutor {
    pub fn new(
        check_repository: Arc<dyn CheckRepository>,
        id_generator: Arc<dyn IdGenerator>,
        incident_evaluator: IncidentEvaluator,
    ) -> Self {
        Self {
            check_repository,
            id_generator,
            incident_evaluator,
        }
    }
}

#[async_trait]
impl crate::durable_scheduler::CheckExecutor for TcpCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Tcp {
            return Err(format!(
                "TcpCheckExecutor only supports TCP monitors, got {:?}",
                monitor.kind
            )
            .into());
        }

        let outcome = match check_tcp_outcome(monitor).await {
            Ok(outcome) => outcome,
            Err(err) => return Ok(CheckExecution::failed(false, err.to_string())),
        };

        let status_code = outcome.result.status_code;
        let latency_ms = outcome.result.latency_ms;
        let error_message = outcome.result.error.clone();

        let check_id = self.id_generator.next_check_id().await;
        let status = if outcome.result.success {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        };

        let error_type = if outcome.result.success {
            ErrorType::None
        } else {
            map_check_error_kind_to_error_type(outcome.error_kind, status_code)
        };

        let check = Check::new(
            check_id,
            monitor.id,
            outcome.result.region.clone(),
            status,
            status_code,
            latency_ms,
            error_type,
            error_message,
        );

        if let Err(err) = self.check_repository.save(&check).await {
            return Ok(CheckExecution::failed(
                outcome.attempted,
                format!("failed to persist check: {err}"),
            ));
        }

        info!(
            monitor_id = %monitor.id.0,
            monitor_name = %monitor.name.as_str(),
            check_id = %check.id.0,
            success = outcome.result.success,
            latency_ms = latency_ms,
            "TCP check persisted"
        );

        if let Err(err) = self
            .incident_evaluator
            .evaluate(monitor, outcome.result.success)
            .await
        {
            error!(
                monitor_id = %monitor.id.0,
                error = %err,
                "failed to evaluate incident state"
            );
        }

        Ok(CheckExecution {
            attempted: outcome.attempted,
            error: None,
            egress_bytes: 0,
            steps_executed: None,
        })
    }
}
