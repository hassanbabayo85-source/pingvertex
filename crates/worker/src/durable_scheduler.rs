use std::{error::Error, sync::Arc, time::Duration};

use application::ports::SchedulingRepository;
use application::{MeterUsageInput, MeterUsageUseCase};
use async_trait::async_trait;
use chrono::Utc;
use domain::Monitor;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Executes the actual network check for a claimed monitor. B17 ships a
/// stub implementation; B18/B19 will replace it with the real HTTP/TCP/
/// DNS/heartbeat check logic and persistence.
#[async_trait]
pub trait CheckExecutor: Send + Sync + Clone + 'static {
    async fn execute(&self, monitor: &Monitor) -> Result<()>;
}

/// Blanket implementation so `Arc<T>` can be passed to the scheduler
/// without a special wrapper type.
#[async_trait]
impl<T> CheckExecutor for Arc<T>
where
    T: CheckExecutor + Send + Sync + 'static,
{
    async fn execute(&self, monitor: &Monitor) -> Result<()> {
        (**self).execute(monitor).await
    }
}

/// B17 placeholder executor. Logs and succeeds without doing any check
/// work, satisfying the durable scheduler contract without stepping on
/// B18/B19's scope.
#[derive(Clone)]
pub struct StubCheckExecutor;

#[async_trait]
impl CheckExecutor for StubCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<()> {
        info!(
            monitor_id = %monitor.id.0,
            "B17 placeholder check executor: not performing real check yet"
        );
        Ok(())
    }
}

pub struct DurableScheduler<R, E> {
    scheduling_repository: R,
    executor: E,
    lease_duration: Duration,
    max_concurrency: usize,
    lease_owner: Uuid,
    metering: Option<Arc<MeterUsageUseCase>>,
}

impl<R, E> DurableScheduler<R, E>
where
    R: SchedulingRepository + Clone + Send + Sync + 'static,
    E: CheckExecutor,
{
    pub fn new(
        scheduling_repository: R,
        executor: E,
        lease_duration: Duration,
        max_concurrency: usize,
    ) -> Self {
        Self {
            scheduling_repository,
            executor,
            lease_duration,
            max_concurrency,
            lease_owner: Uuid::new_v4(),
            metering: None,
        }
    }

    /// Enables PAYG billing for durable monitor executions. The scheduler
    /// supplies the stable execution identity; metering finalizes the charge
    /// after the actual attempt, including failed target/network attempts.
    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub async fn run(self) -> Result<()> {
        info!("Durable scheduler started");
        let mut tasks: JoinSet<Result<()>> = JoinSet::new();

        loop {
            self.scheduling_repository
                .recover_expired_leases(Utc::now())
                .await?;

            let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
            if free_capacity > 0 {
                let claimed = self
                    .scheduling_repository
                    .claim_due_monitors(
                        free_capacity as u32,
                        self.lease_owner,
                        self.lease_duration,
                        Utc::now(),
                    )
                    .await?;

                info!(count = claimed.len(), "claimed due monitors");

                for claim in claimed {
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
                    let metering = self.metering.clone();
                    let lease_owner = self.lease_owner;

                    tasks.spawn(async move {
                        let monitor = claim.monitor;
                        let request_id = format!("monitor-check:{}:{}", monitor.id.0, claim.execution_id);
                        let result = if let Some(metering) = metering {
                            let network_egress_request_id = format!("network-egress:{}:{}", monitor.id.0, claim.execution_id);
                            metering
                                .execute_attempt(
                                    MeterUsageInput {
                                        tenant_id: monitor.tenant_id.0,
                                        operation: "monitor_check".to_owned(),
                                        units: 1,
                                        request_id,
                                    },
                                    || {
                                        let metering = metering.clone();
                                        let monitor = monitor.clone();
                                        async move {
                                            metering
                                                .execute_attempt(
                                                    MeterUsageInput {
                                                        tenant_id: monitor.tenant_id.0,
                                                        operation: "network_egress".to_owned(),
                                                        // The current checker contract does not expose wire-byte
                                                        // counters. One billable MB is therefore reserved as the
                                                        // minimum egress allowance for an active check. This keeps
                                                        // billing fail-closed and avoids estimating usage from
                                                        // attacker-controlled response metadata.
                                                        units: 1,
                                                        request_id: network_egress_request_id,
                                                    },
                                                    || executor.execute(&monitor),
                                                )
                                                .await
                                                .map(|_| ())
                                        }
                                    },
                                )
                                .await
                                .map(|_| ())
                                .map_err(|err| -> Box<dyn Error + Send + Sync> { Box::new(err) })
                        } else {
                            executor.execute(&monitor).await
                        };
                        if let Err(err) = &result {
                            warn!(
                                monitor_id = %monitor.id.0,
                                error = %err,
                                "check execution failed"
                            );
                        }

                        // Always release the claim after an execution attempt.
                        // If the process crashes before this point, the lease
                        // will expire and `recover_expired_leases` will make
                        // the monitor claimable again.
                        if let Err(err) = scheduling_repository
                            .complete_claim(monitor.id, lease_owner)
                            .await
                        {
                            error!(
                                monitor_id = %monitor.id.0,
                                error = %err,
                                "failed to release claim"
                            );
                        }

                        result
                    });
                }
            }

            if tasks.is_empty() {
                // No due monitors and no running tasks; avoid a hot loop.
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                // Wait for at least one task to finish before the next
                // claim/recovery iteration. This bounds concurrency.
                match tasks.join_next().await {
                    Some(Ok(Ok(_))) => {}
                    Some(Ok(Err(err))) => {
                        warn!(error = %err, "monitor check task returned an error");
                    }
                    Some(Err(join_err)) => {
                        error!(error = %join_err, "monitor check task panicked");
                    }
                    None => {}
                }
            }
        }
    }
}

/// Convenience entry point used by the worker binary.
pub async fn run_durable<R, E>(
    scheduling_repository: R,
    executor: E,
    lease_duration: Duration,
    max_concurrency: usize,
) -> Result<()>
where
    R: SchedulingRepository + Clone + Send + Sync + 'static,
    E: CheckExecutor,
{
    DurableScheduler::new(
        scheduling_repository,
        executor,
        lease_duration,
        max_concurrency,
    )
    .run()
    .await
}
