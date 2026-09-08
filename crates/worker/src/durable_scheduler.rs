use std::{error::Error, sync::Arc, time::Duration};

use application::ports::SchedulingRepository;
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use async_trait::async_trait;
use chrono::Utc;
use domain::Monitor;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct CheckExecution {
    pub attempted: bool,
    pub error: Option<String>,
}

impl CheckExecution {
    pub fn success(attempted: bool) -> Self {
        Self {
            attempted,
            error: None,
        }
    }
    pub fn failed(attempted: bool, error: impl Into<String>) -> Self {
        Self {
            attempted,
            error: Some(error.into()),
        }
    }
}

/// Executes the actual network check for a claimed monitor. B17 ships a
/// stub implementation; B18/B19 will replace it with the real HTTP/TCP/
/// DNS/heartbeat check logic and persistence.
#[async_trait]
pub trait CheckExecutor: Send + Sync + Clone + 'static {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution>;
}

/// Blanket implementation so `Arc<T>` can be passed to the scheduler
/// without a special wrapper type.
#[async_trait]
impl<T> CheckExecutor for Arc<T>
where
    T: CheckExecutor + Send + Sync + 'static,
{
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
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
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        info!(
            monitor_id = %monitor.id.0,
            "B17 placeholder check executor: not performing real check yet"
        );
        Ok(CheckExecution::success(false))
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
                let one_time = self
                    .scheduling_repository
                    .claim_one_time_checks(
                        free_capacity as u32,
                        self.lease_owner,
                        self.lease_duration,
                        Utc::now(),
                    )
                    .await?;
                for claim in one_time {
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
                    let metering = self.metering.clone();
                    let lease_owner = self.lease_owner;
                    tasks.spawn(async move {
                        let monitor = claim.monitor;
                        let operation = domain::monitor_operation(monitor.kind, true).as_str().to_owned();
                        let billing_request_id = format!("one-time:{}:{}", claim.request.id, claim.request.execution_id);
                        let result = if let Some(metering) = metering {
                            match metering.execute_classified_attempt(
                                MeterUsageInput { tenant_id: monitor.tenant_id.0, operation, units: 1, request_id: billing_request_id },
                                || {
                                    let executor = executor.clone();
                                    let monitor = monitor.clone();
                                    async move { executor.execute(&monitor).await.map(|o| MeteredAttempt { attempted: o.attempted, error: o.error }) }
                                },
                            ).await {
                                Ok((_, attempt)) => if attempt.attempted { attempt.error } else { Some("operation was not attempted".to_owned()) },
                                Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => Some(reason),
                                Err(application::ChargeForUsageError::InsufficientBalance) => {
                                    if let Err(block_err) = scheduling_repository
                                        .set_billing_block(
                                            monitor.id,
                                            monitor.tenant_id,
                                            "Insufficient wallet balance",
                                        )
                                        .await
                                    {
                                        error!(monitor_id=%monitor.id.0, error=%block_err, "failed to persist PAYG billing block");
                                    }
                                    Some("PAYG balance is insufficient".to_owned())
                                }
                                Err(err) => Some(err.to_string()),
                            }
                        } else {
                            Some("PAYG metering is not configured".to_owned())
                        };
                        let status = if result.is_some() { domain::OneTimeCheckRequest::FAILED } else { domain::OneTimeCheckRequest::SUCCEEDED };
                        if let Err(err) = scheduling_repository.complete_one_time_check(claim.request.id, lease_owner, status, result.as_deref()).await {
                            error!(request_id=%claim.request.id, error=%err, "failed to finalize one-time check");
                        }
                        Ok::<(), Box<dyn Error + Send + Sync>>(())
                    });
                }
                let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
                if free_capacity == 0 {
                    continue;
                }
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
                        let request_id =
                            format!("monitor-check:{}:{}", monitor.id.0, claim.execution_id);
                        let result = if claim.free_entitled {
                            executor.execute(&monitor).await.map(|_| ())
                        } else if let Some(metering) = metering {
                            match metering.execute_classified_attempt(
                                MeterUsageInput {
                                    tenant_id: monitor.tenant_id.0,
                                    operation: domain::monitor_operation(monitor.kind, false).as_str().to_owned(),
                                    units: 1,
                                    request_id,
                                },
                                || {
                                    let executor = executor.clone();
                                    let monitor = monitor.clone();
                                    async move {
                                        executor.execute(&monitor).await.map(|outcome| {
                                            MeteredAttempt {
                                                attempted: outcome.attempted,
                                                error: outcome.error,
                                            }
                                        })
                                    }
                                },
                            ).await {
                                Ok(_) => Ok(()),
                                Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => {
                                    warn!(monitor_id=%monitor.id.0, reason=%reason, "monitor execution did not reach a billable attempt");
                                    Ok(())
                                }
                                Err(err) => Err(Box::new(err) as Box<dyn Error + Send + Sync>),
                            }
                        } else {
                            Err("PAYG metering is not configured".into())
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

