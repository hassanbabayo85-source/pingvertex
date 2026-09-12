use std::{error::Error, sync::Arc};

use application::ports::{CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind};
use tracing::{error, info};

use crate::check_error_mapping::map_check_error_kind_to_error_type;
use crate::checker::check_http;
use crate::durable_scheduler::CheckExecution;
use crate::incident_evaluator::IncidentEvaluator;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// B18 real HTTP/HTTPS executor for B17's `DurableScheduler`.
///
/// Executes a monitor only when `monitor.kind == MonitorKind::Http`.
/// Other monitor kinds are intentionally rejected so later check workers
/// (TCP/DNS/etc.) can be wired independently without this executor
/// silently treating them as success.
#[derive(Clone)]
pub struct HttpCheckExecutor {
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
}

impl HttpCheckExecutor {
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
impl crate::durable_scheduler::CheckExecutor for HttpCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Http {
            return Err(format!(
                "B18 HttpCheckExecutor only supports HTTP monitors, got {:?}",
                monitor.kind
            )
            .into());
        }

        let outcome = check_http(monitor).await;

        let status_code = outcome.result.status_code;
        let latency_ms = outcome.result.latency_ms;
        let error_message = outcome.result.error.clone();
        let cert_expires_at = outcome.result.cert_expires_at;
        let cert_days_remaining = outcome.result.cert_days_remaining;

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
        )
        .with_cert_info(cert_expires_at, cert_days_remaining);

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
            status_code = ?status_code,
            latency_ms = latency_ms,
            "HTTP check persisted"
        );

        // Non-fatal, mirroring the diagnostic report save above and the
        // notification-failure handling inside `IncidentManager`: a
        // failure to evaluate/persist incident state must never cause
        // the check itself (already persisted above) to be reported as
        // failed.
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
            egress_bytes: outcome.egress_bytes,
        })
    }
}
