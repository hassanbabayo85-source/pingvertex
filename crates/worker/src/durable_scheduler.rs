use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use rand::{Rng, rng};

use application::ports::{
    MonitorRepository, PricingConfigRepository, SchedulingRepository, SubscriptionRepository,
    TenantRepository, WalletRepository,
};
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use async_trait::async_trait;
use chrono::Utc;
use domain::{
    Monitor, TenantStatus,
    enums::{PlanTier, SubscriptionStatus},
};
use tokio::{sync::Mutex, task::JoinSet};
use tracing::{error, info, warn};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

async fn execute_with_controls<E: CheckExecutor>(
    executor: E,
    monitor: Monitor,
    limiter: Arc<Mutex<Instant>>,
    min_interval: Duration,
    retry_limit: u32,
    retry_base: Duration,
    jitter: Duration,
) -> Result<CheckExecution> {
    let mut attempt = 0u32;
    loop {
        let wait_for = {
            let mut next_allowed = limiter.lock().await;
            let now = Instant::now();
            let scheduled = std::cmp::max(*next_allowed, now);
            *next_allowed = scheduled + min_interval;
            scheduled.saturating_duration_since(now)
        };
        if !wait_for.is_zero() {
            tokio::time::sleep(wait_for).await;
        }

        match executor.execute(&monitor).await {
            Ok(result) => return Ok(result),
            Err(error) if attempt < retry_limit => {
                let exponential = retry_base.saturating_mul(1u32 << attempt.min(5));
                let jitter_ms = if jitter.is_zero() {
                    0
                } else {
                    rand::rng().random_range(0..=jitter.as_millis() as u64)
                };
                warn!(monitor_id=%monitor.id.0, attempt=attempt + 1, delay_ms=exponential.as_millis().saturating_add(u128::from(jitter_ms)), error=%error, "monitor executor failed; retrying with backoff");
                tokio::time::sleep(exponential + Duration::from_millis(jitter_ms)).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

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
    batch_pause: Duration,
    jitter: Duration,
    execution_min_interval: Duration,
    execution_limiter: Arc<Mutex<Instant>>,
    execution_retry_limit: u32,
    execution_retry_base: Duration,
    tenant_repository: Option<Arc<dyn TenantRepository>>,
    monitor_repository: Option<Arc<dyn MonitorRepository>>,
    wallet_repository: Option<Arc<dyn WalletRepository>>,
    subscription_repository: Option<Arc<dyn SubscriptionRepository>>,
    pricing_config_repository: Option<Arc<dyn PricingConfigRepository>>,
    max_concurrency_per_tenant: usize,
    due_claim_limit_per_tenant: usize,
    deterministic_spread_seconds: u64,
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
            max_concurrency: max_concurrency.max(1),
            lease_owner: Uuid::new_v4(),
            metering: None,
            batch_pause: Duration::from_millis(
                std::env::var("SCHEDULER_BATCH_PAUSE_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(100),
            ),
            jitter: Duration::from_millis(
                std::env::var("SCHEDULER_JITTER_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(250),
            ),
            execution_min_interval: Duration::from_millis(
                std::env::var("SCHEDULER_EXECUTION_MIN_INTERVAL_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(20),
            ),
            execution_limiter: Arc::new(Mutex::new(Instant::now())),
            execution_retry_limit: std::env::var("SCHEDULER_EXECUTION_RETRY_LIMIT")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(2)
                .min(5),
            execution_retry_base: Duration::from_millis(
                std::env::var("SCHEDULER_EXECUTION_RETRY_BASE_MS")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(250)
                    .min(10_000),
            ),
            tenant_repository: None,
            monitor_repository: None,
            wallet_repository: None,
            subscription_repository: None,
            pricing_config_repository: None,
            max_concurrency_per_tenant: 4,
            due_claim_limit_per_tenant: 4,
            deterministic_spread_seconds: 5,
        }
    }

    /// Enables PAYG billing for durable monitor executions. The scheduler
    /// supplies the stable execution identity; metering finalizes the charge
    /// after the actual attempt, including failed target/network attempts.
    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub fn with_tenant_repository(
        mut self,
        tenant_repository: Arc<dyn TenantRepository>,
        subscription_repository: Arc<dyn SubscriptionRepository>,
        pricing_config_repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.tenant_repository = Some(tenant_repository);
        self.subscription_repository = Some(subscription_repository);
        self.pricing_config_repository = Some(pricing_config_repository);
        self
    }

    pub fn with_entitlement_repositories(
        mut self,
        monitor_repository: Arc<dyn MonitorRepository>,
        wallet_repository: Arc<dyn WalletRepository>,
    ) -> Self {
        self.monitor_repository = Some(monitor_repository);
        self.wallet_repository = Some(wallet_repository);
        self
    }

    pub fn with_backpressure(
        mut self,
        max_concurrency_per_tenant: usize,
        due_claim_limit_per_tenant: usize,
    ) -> Self {
        self.max_concurrency_per_tenant = max_concurrency_per_tenant.max(1);
        self.due_claim_limit_per_tenant = due_claim_limit_per_tenant.max(1);
        self
    }

    pub fn with_deterministic_spread(mut self, spread_seconds: u64) -> Self {
        self.deterministic_spread_seconds = spread_seconds.min(300);
        self
    }

    async fn resolve_free_entitlement(
        &self,
        tenant_id: domain::TenantId,
    ) -> Result<Option<domain::Plan>> {
        let (Some(monitor_repository), Some(wallet_repository), Some(subscription_repository), Some(pricing_config_repository)) = (
            &self.monitor_repository,
            &self.wallet_repository,
            &self.subscription_repository,
            &self.pricing_config_repository,
        ) else {
            return Ok(None);
        };

        let plan = application::use_cases::ResolveEntitlementsUseCase::new(
            subscription_repository.clone(),
        )
        .with_pricing_config_repository(pricing_config_repository.clone())
        .execute(application::queries::ResolveEntitlements { tenant_id })
        .await?;

        let tier = if let Some(override_tier) = subscription_repository
            .find_test_plan_override(tenant_id)
            .await?
        {
            override_tier
        } else {
            match subscription_repository.find_by_tenant_id(tenant_id).await? {
                Some(subscription) => match subscription.status {
                    SubscriptionStatus::Expired | SubscriptionStatus::Canceled => PlanTier::Free,
                    SubscriptionStatus::Trialing
                    | SubscriptionStatus::Active
                    | SubscriptionStatus::PastDue => subscription.plan_tier,
                },
                None => PlanTier::Free,
            }
        };

        if tier != PlanTier::Free {
            return Ok(None);
        }

        let wallet_activated = wallet_repository
            .find(tenant_id.0)
            .await?
            .is_some_and(|wallet| wallet.payg_activated_at.is_some());
        if wallet_activated {
            return Ok(None);
        }

        let monitor_count = monitor_repository
            .find_all_for_tenant(tenant_id)
            .await?
            .len() as u32;

        Ok((monitor_count <= plan.max_monitors).then_some(plan))
    }

    pub async fn run(self) -> Result<()> {
        info!("Durable scheduler started");
        let mut tasks: JoinSet<Result<()>> = JoinSet::new();

        loop {
            // Anti-thundering-herd guard: every claim cycle is separated by a
            // bounded pause plus randomized jitter. Concurrency remains hard
            // bounded by max_concurrency and DB claiming remains lease based.
            let jitter_ms = if self.jitter.is_zero() {
                0
            } else {
                rng().random_range(0..=self.jitter.as_millis() as u64)
            };
            tokio::time::sleep(self.batch_pause + Duration::from_millis(jitter_ms)).await;

            self.scheduling_repository
                .recover_expired_leases(Utc::now())
                .await?;

            let due_depth = self
                .scheduling_repository
                .due_monitor_count(Utc::now())
                .await?;
            metrics::gauge!("scheduler_due_queue_depth").set(due_depth as f64);
            metrics::gauge!("scheduler_active_tasks").set(tasks.len() as f64);

            let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
            if free_capacity == 0 {
                metrics::counter!("scheduler_backpressure_total", "reason" => "global")
                    .increment(1);
            }
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
                    let execution_limiter = self.execution_limiter.clone();
                    let execution_min_interval = self.execution_min_interval;
                    let execution_retry_limit = self.execution_retry_limit;
                    let execution_retry_base = self.execution_retry_base;
                    let jitter = self.jitter;
                    let lease_owner = self.lease_owner;
                    let tenant_repository = self.tenant_repository.clone();
                    tasks.spawn(async move {
                        let monitor = claim.monitor;
                        let monitor_id = monitor.id;
                        let account_type = if let Some(tenant_repository) = tenant_repository {
                            match tenant_repository
                                .find_by_id(domain::TenantId(monitor.tenant_id.0))
                                .await?
                            {
                                Some(tenant) if tenant.status == TenantStatus::Active => {
                                    Some(tenant.account_type)
                                }
                                Some(_) | None => {
                                    scheduling_repository
                                        .complete_one_time_check(
                                            claim.request.id,
                                            lease_owner,
                                            domain::OneTimeCheckRequest::FAILED,
                                            Some("tenant is not active"),
                                        )
                                        .await?;
                                    return Ok::<(), Box<dyn Error + Send + Sync>>(());
                                }
                            }
                        } else {
                            None
                        };
                        let monitor_tenant_id = monitor.tenant_id;
                        let operation = domain::monitor_operation(monitor.kind, true).as_str().to_owned();
                        let billing_request_id = format!("one-time:{}:{}", claim.request.id, claim.request.execution_id);
                        let result = if let Some(metering) = metering {
                            match metering.execute_classified_attempt(
                                MeterUsageInput { tenant_id: monitor.tenant_id.0, operation, units: 1, request_id: billing_request_id,
                                    monitor_id: Some(monitor.id.0),
                                    execution_id: Some(claim.request.execution_id),
                                    pricing_context: Some(application::ports::PricingContext {
                                        monitor_kind: Some(monitor.kind),
                                        region: monitor.regions.first().cloned(),
                                        quantity: 1,
                                        account_type,
                                    }),
        },
                                move || {
                                    let executor = executor.clone();
                                    let monitor = monitor.clone();
                                    async move { execute_with_controls(executor.clone(), monitor.clone(), execution_limiter.clone(), execution_min_interval, execution_retry_limit, execution_retry_base, jitter).await.map(|o| MeteredAttempt { attempted: o.attempted, error: o.error }) }
                                },
                            ).await {
                                Ok((_, attempt)) => if attempt.attempted { attempt.error } else { Some("operation was not attempted".to_owned()) },
                                Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => Some(reason),
                                Err(application::ChargeForUsageError::InsufficientBalance) => {
                                    if let Err(block_err) = scheduling_repository
                                        .set_billing_block(
                                            monitor_id,
                                            monitor_tenant_id,
                                            "Insufficient wallet balance",
                                        )
                                        .await
                                    {
                                        error!(monitor_id=%monitor_id.0, error=%block_err, "failed to persist PAYG billing block");
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
                    .claim_due_monitors_with_limits(
                        free_capacity as u32,
                        self.lease_owner,
                        self.lease_duration,
                        Utc::now(),
                        self.max_concurrency_per_tenant,
                        self.due_claim_limit_per_tenant,
                    )
                    .await?;

                info!(count = claimed.len(), "claimed due monitors");

                for claim in claimed {
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
                    let metering = self.metering.clone();
                    let execution_limiter = self.execution_limiter.clone();
                    let execution_min_interval = self.execution_min_interval;
                    let execution_retry_limit = self.execution_retry_limit;
                    let execution_retry_base = self.execution_retry_base;
                    let jitter = self.jitter;
                    let lease_owner = self.lease_owner;
                    let deterministic_spread_seconds = self.deterministic_spread_seconds;

                    let tenant_status = if let Some(tenant_repository) = &self.tenant_repository {
                        tenant_repository
                            .find_by_id(domain::TenantId(claim.monitor.tenant_id.0))
                            .await?
                            .map(|tenant| (tenant.status, tenant.account_type))
                    } else {
                        None
                    };

                    if tenant_status
                        .as_ref()
                        .is_some_and(|(status, _)| *status != TenantStatus::Active)
                    {
                        metrics::counter!("scheduler_suspended_tenant_skips_total").increment(1);
                        scheduling_repository
                            .complete_claim_with_spread(
                                claim.monitor.id,
                                lease_owner,
                                deterministic_spread_seconds,
                            )
                            .await?;
                        continue;
                    }

                    let free_plan = self
                        .resolve_free_entitlement(claim.monitor.tenant_id)
                        .await
                        .unwrap_or_else(|error| {
                            warn!(
                                monitor_id = %claim.monitor.id.0,
                                error = %error,
                                "failed to resolve scheduler entitlement; using PAYG path"
                            );
                            None
                        });
                    let account_type = tenant_status
                        .as_ref()
                        .map(|(_, account_type)| *account_type);
                    let monitor = claim.monitor;

                    tasks.spawn(async move {
                        let request_id =
                            format!("monitor-check:{}:{}", monitor.id.0, claim.execution_id);
                        let result = if let Some(plan) = free_plan {
                            if monitor.interval_seconds < plan.min_check_interval_seconds {
                                metrics::counter!("scheduler_entitlement_skips_total").increment(1);
                                if let Err(err) = scheduling_repository
                                    .complete_claim_with_spread(
                                        monitor.id,
                                        lease_owner,
                                        deterministic_spread_seconds,
                                    )
                                    .await
                                {
                                    error!(
                                        monitor_id = %monitor.id.0,
                                        error = %err,
                                        "failed to release entitlement-skipped claim"
                                    );
                                }
                                return Ok::<(), Box<dyn Error + Send + Sync>>(());
                            }
                            execute_with_controls(
                                executor.clone(),
                                monitor.clone(),
                                execution_limiter.clone(),
                                execution_min_interval,
                                execution_retry_limit,
                                execution_retry_base,
                                jitter,
                            )
                            .await
                            .map(|_| ())
                        } else if let Some(metering) = metering {
                            match metering
                                .execute_classified_attempt(
                                    MeterUsageInput {
                                        tenant_id: monitor.tenant_id.0,
                                        operation: domain::monitor_operation(
                                            monitor.kind,
                                            false,
                                        )
                                        .as_str()
                                        .to_owned(),
                                        units: 1,
                                        request_id,
                                        monitor_id: Some(monitor.id.0),
                                        execution_id: Some(claim.execution_id),
                                        pricing_context: Some(
                                            application::ports::PricingContext {
                                                monitor_kind: Some(monitor.kind),
                                                region: monitor.regions.first().cloned(),
                                                quantity: 1,
                                                account_type,
                                            },
                                        ),
                                    },
                                    || {
                                        let executor = executor.clone();
                                        let monitor = monitor.clone();
                                        async move {
                                            execute_with_controls(
                                                executor.clone(),
                                                monitor.clone(),
                                                execution_limiter.clone(),
                                                execution_min_interval,
                                                execution_retry_limit,
                                                execution_retry_base,
                                                jitter,
                                            )
                                            .await
                                            .map(|outcome| MeteredAttempt {
                                                attempted: outcome.attempted,
                                                error: outcome.error,
                                            })
                                        }
                                    },
                                )
                                .await
                            {
                                Ok(_) => Ok(()),
                                Err(application::ChargeForUsageError::OperationNotAttempted(
                                    reason,
                                )) => {
                                    warn!(
                                        monitor_id = %monitor.id.0,
                                        reason = %reason,
                                        "monitor execution did not reach a billable attempt"
                                    );
                                    Ok(())
                                }
                                Err(err) => {
                                    Err(Box::new(err) as Box<dyn Error + Send + Sync>)
                                }
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

                        if let Err(err) = scheduling_repository
                            .complete_claim_with_spread(
                                monitor.id,
                                lease_owner,
                                deterministic_spread_seconds,
                            )
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
