use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use domain::{Monitor, MonitorKind};

use crate::dns_check_executor::DnsCheckExecutor;
use crate::durable_scheduler::CheckExecutor;
use crate::flow_check_executor::FlowCheckExecutor;
use crate::http_check_executor::HttpCheckExecutor;
use crate::incident_evaluator::IncidentEvaluator;
use crate::tcp_check_executor::TcpCheckExecutor;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Dispatches an active check to the correct executor based on
/// `monitor.kind`. Heartbeat monitors are rejected here because they are
/// passive and are handled by the separate `HeartbeatSweeper` loop.
#[derive(Clone)]
pub struct MultiCheckExecutor {
    http: HttpCheckExecutor,
    tcp: TcpCheckExecutor,
    dns: DnsCheckExecutor,
    flow: FlowCheckExecutor,
}

impl MultiCheckExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        check_repository: Arc<dyn application::ports::CheckRepository>,
        id_generator: Arc<dyn application::ports::IdGenerator>,
        incident_evaluator: IncidentEvaluator,
        api_flow_repository: Arc<dyn application::ports::ApiFlowRepository>,
    ) -> Self {
        Self {
            http: HttpCheckExecutor::new(
                check_repository.clone(),
                id_generator.clone(),
                incident_evaluator.clone(),
            ),
            tcp: TcpCheckExecutor::new(
                check_repository.clone(),
                id_generator.clone(),
                incident_evaluator.clone(),
            ),
            dns: DnsCheckExecutor::new(
                check_repository.clone(),
                id_generator.clone(),
                incident_evaluator.clone(),
            ),
            flow: FlowCheckExecutor::new(
                api_flow_repository,
                check_repository,
                id_generator,
                incident_evaluator,
            ),
        }
    }
}

#[async_trait]
impl CheckExecutor for MultiCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<()> {
        match monitor.kind {
            MonitorKind::Http => self.http.execute(monitor).await,
            MonitorKind::Tcp => self.tcp.execute(monitor).await,
            MonitorKind::Dns => self.dns.execute(monitor).await,
            MonitorKind::Flow => self.flow.execute(monitor).await,
            MonitorKind::Heartbeat => {
                tracing::warn!(
                    monitor_id = %monitor.id.0,
                    "heartbeat monitor reached active executor; this is a scheduling bug"
                );
                Err(
                    "heartbeat monitors must be handled by HeartbeatSweeper, not active executor"
                        .into(),
                )
            }
        }
    }
}
