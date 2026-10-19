use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use rand::{Rng, rng};

use application::ports::{
    EntitlementResolver, MonitorRepository, SchedulingRepository, TenantRepository,
};
use application::MeterUsageUseCase;
use async_trait::async_trait;
use chrono::Utc;
use domain::{Monitor, TenantStatus};
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
    let started = Instant::now();
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

        metrics::counter!("checks_executed_total", "kind" => domain::monitor_operation(monitor.kind, false).as_str()).increment(1);
        match executor.execute(&monitor).await {
            Ok(result) => {
                let elapsed = started.elapsed().as_secs_f64();
                metrics::histogram!("payg_scheduler_execution_latency_seconds").record(elapsed);
                metrics::histogram!("monitor_execution_latency_seconds").record(elapsed);
                return Ok(result);
            }
            Err(error) if attempt < retry_limit => {
                metrics::counter!("payg_scheduler_retry_total").increment(1);
                let exponential = retry_base.saturating_mul(1u32 << attempt.min(5));
                let jitter_ms = if jitter.is_zero() {
                    0
                } else {
                    rand::rng().random_range(0..=jitter.as_millis() as u64)
                };
                warn!(monitor_id=%monitor.id.0, attempt=attempt + 1, delay_ms=exponential.as_millis().saturating_add(u128::from(jitter_ms)), error=%error, "monitor executor failed; retrying with backoff");
                metrics::counter!("payg_scheduler_backoff_total").increment(1);
                if jitter_ms > 0 {
                    metrics::counter!("payg_scheduler_jitter_total").increment(1);
                }
                tokio::time::sleep(exponential + Duration::from_millis(jitter_ms)).await;
                attempt += 1;
            }
            Err(error) => {
                let elapsed = started.elapsed().as_secs_f64();
                metrics::histogram!("payg_scheduler_execution_latency_seconds").record(elapsed);
                metrics::histogram!("monitor_execution_latency_seconds").record(elapsed);
                return Err(error);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckExecution {
    pub attempted: bool,
    pub error: Option<String>,
    pub egress_bytes: u64,
    /// GAP 3 Kashi B: for Flow monitors, the number of steps whose HTTP
    /// request actually started. `None` for non-flow kinds. The scheduler
    /// forwards this to `MeteredAttempt::actual_units` so a flow check is
    /// billed for the steps it actually ran, not for the reserved maximum.
    pub steps_executed: Option<i64>,
}

impl CheckExecution {
    pub fn success(attempted: bool) -> Self {
        Self {
            attempted,
            error: None,
            egress_bytes: 0,
            steps_executed: None,
        }
    }
    pub fn failed(attempted: bool, error: impl Into<String>) -> Self {
        Self {
            attempted,
            error: Some(error.into()),
            egress_bytes: 0,
            steps_executed: None,
        }
    }
}

/// Executes a claimed monitor through the configured check executor.
/// The production executor dispatches to the monitor's HTTP/TCP/DNS/Flow/
/// heartbeat implementation and persists the resulting check outcome.
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
    /// TASK 5 — resolves the tenant's subscription entitlements. When
    /// absent, the scheduler fails safe (skips monitor checks).
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
    max_concurrency_per_tenant: usize,
    due_claim_limit_per_tenant: usize,
    deterministic_spread_seconds: u64,
    heartbeat_interval: Duration,
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
            entitlement_resolver: None,
            max_concurrency_per_tenant: 4,
            due_claim_limit_per_tenant: 4,
            deterministic_spread_seconds: 5,
            heartbeat_interval: (lease_duration / 3).max(Duration::from_secs(5)),
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
    ) -> Self {
        self.tenant_repository = Some(tenant_repository);
        self
    }

    /// TASK 5 — installs the monitor repository used for the
    /// over-entitlement check, and the subscription entitlement resolver
    /// that replaces the old wallet/PAYG gate.
    pub fn with_entitlement_repositories(
        mut self,
        monitor_repository: Arc<dyn MonitorRepository>,
        entitlement_resolver: Arc<dyn EntitlementResolver>,
    ) -> Self {
        self.monitor_repository = Some(monitor_repository);
        self.entitlement_resolver = Some(entitlement_resolver);
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

    pub fn with_heartbeat_interval(mut self, heartbeat_interval: Duration) -> Self {
        let max_interval = self.lease_duration / 2;
        self.heartbeat_interval = heartbeat_interval
            .max(Duration::from_secs(1))
            .min(max_interval.max(Duration::from_secs(1)));
        self
    }

    /// TASK 5 — resolves the tenant's subscription entitlements.
    ///
    /// Returns `Ok(None)` when the resolver is not wired (fail-safe: the
    /// caller skips the execution rather than running unmetered forever).
    /// A resolver error is logged and also returns `Ok(None)` for the
    /// same fail-safe reason.
    async fn resolve_entitlements(
        &self,
        tenant_id: domain::TenantId,
    ) -> Result<Option<domain::Entitlements>> {
        let Some(resolver) = &self.entitlement_resolver else {
            return Ok(None);
        };
        match resolver.resolve(tenant_id).await {
            Ok(entitlements) => Ok(Some(entitlements)),
            Err(err) => {
                warn!(
                    tenant_id = %tenant_id.0,
                    error = %err,
                    "failed to resolve entitlements; scheduler will skip due monitors for this tenant"
                );
                Ok(None)
            }
        }
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

            let expired = self
                .scheduling_repository
                .recover_expired_leases(Utc::now())
                .await?;
            if expired > 0 {
                metrics::counter!("payg_scheduler_lease_expiry_total").increment(expired as u64);
            }

            let due_depth = self
                .scheduling_repository
                .due_monitor_count(Utc::now())
                .await?;
            metrics::gauge!("scheduler_due_queue_depth").set(due_depth as f64);
            metrics::gauge!("payg_scheduler_due_monitors").set(due_depth as f64);
            metrics::gauge!("scheduler_active_tasks").set(tasks.len() as f64);
            metrics::gauge!("payg_scheduler_running_tasks").set(tasks.len() as f64);

            metrics::gauge!("payg_scheduler_per_tenant_concurrency_limit")
                .set(self.max_concurrency_per_tenant as f64);
            let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
            if free_capacity == 0 {
                metrics::counter!("scheduler_backpressure_total", "reason" => "global")
                    .increment(1);
                metrics::counter!("scheduler_rejected_backpressure_total").increment(1);
            }
            if free_capacity > 0 {
                let one_time_execs = self
                    .scheduling_repository
                    .claim_one_time_executions(
                        free_capacity as u32,
                        self.lease_owner,
                        self.lease_duration,
                        Utc::now(),
                    )
                    .await?;
                if !one_time_execs.is_empty() {
                    metrics::counter!("scheduler_claimed_total", "kind" => "one_time")
                        .increment(one_time_execs.len() as u64);
                    metrics::counter!("checks_scheduled_total", "kind" => "one_time")
                        .increment(one_time_execs.len() as u64);
                }
                for exec_claim in one_time_execs {
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
                        let execution = exec_claim.execution;
                        let execution_pk = execution.id;
                        let _execution_dbid = execution.execution_id;
                        let region_label = execution.region.clone();
                        let _request = exec_claim.request;
                        let mut monitor = exec_claim.monitor;
                        let _monitor_id = monitor.id;
                        let _monitor_tenant_id = monitor.tenant_id;

                        // Resolve the RegionCode for the billed region from the
                        // monitor's own regions (the column is authoritative).
                        let region_code = monitor
                            .regions
                            .iter()
                            .find(|r| r.as_str() == region_label)
                            .cloned()
                            .or_else(|| monitor.regions.first().cloned());

                        let _account_type = if let Some(tenant_repository) = tenant_repository {
                            match tenant_repository
                                .find_by_id(domain::TenantId(monitor.tenant_id.0))
                                .await?
                            {
                                Some(tenant) if tenant.status == TenantStatus::Active => {
                                    Some(tenant.account_type)
                                }
                                Some(_) | None => {
                                    metrics::counter!("checks_skipped_total", "reason" => "tenant_inactive")
                                        .increment(1);
                                    if let Err(err) = scheduling_repository
                                        .complete_one_time_execution(
                                            execution_pk,
                                            lease_owner,
                                            domain::OneTimeCheckExecution::FAILED,
                                            Some("tenant is not active"),
                                            false,
                                            false,
                                            0,
                                        )
                                        .await
                                    {
                                        error!(execution_id=%execution_pk, error=%err, "failed to finalize one-time execution (tenant inactive)");
                                    }
                                    return Ok::<(), Box<dyn Error + Send + Sync>>(());
                                }
                            }
                        } else {
                            None
                        };

                        // Narrow the monitor to the billed region for this
                        // execution so its billing context is region-scoped.
                        if let Some(rc) = &region_code {
                            monitor.regions = vec![rc.clone()];
                        }

                        // Task 8E — PAYG metering is retired. One-time
                        // executions now run directly with no billing. The
                        // `billable` and `charge_units` values written back
                        // to the durable execution are always 0/false.
                        let billable = false;
                        let charge_units: i64 = 0;
                        let _ = metering; // field still wired (Task 8F removes it)

                        let result: Option<String> = {
                            let executor = executor.clone();
                            let monitor = monitor.clone();
                            let execution_limiter = execution_limiter.clone();
                            match execute_with_controls(
                                executor,
                                monitor,
                                execution_limiter,
                                execution_min_interval,
                                execution_retry_limit,
                                execution_retry_base,
                                jitter,
                            )
                            .await
                            {
                                Ok(outcome) => {
                                    if !outcome.attempted {
                                        metrics::counter!("checks_skipped_total", "reason" => "not_attempted")
                                            .increment(1);
                                        Some("operation was not attempted".to_owned())
                                    } else {
                                        outcome.error
                                    }
                                }
                                Err(err) => Some(err.to_string()),
                            }
                        };

                        let status = if result.is_some() {
                            domain::OneTimeCheckExecution::FAILED
                        } else {
                            domain::OneTimeCheckExecution::SUCCEEDED
                        };
                        if let Err(err) = scheduling_repository
                            .complete_one_time_execution(
                                execution_pk,
                                lease_owner,
                                status,
                                result.as_deref(),
                                billable,
                                billable,
                                charge_units,
                            )
                            .await
                        {
                            error!(execution_id=%execution_pk, error=%err, "failed to finalize one-time execution");
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

                if !claimed.is_empty() {
                    metrics::counter!("scheduler_claimed_total", "kind" => "due_monitor")
                        .increment(claimed.len() as u64);
                    metrics::counter!("checks_scheduled_total", "kind" => "monitor")
                        .increment(claimed.len() as u64);
                }

                info!(count = claimed.len(), "claimed due monitors");

                for claim in claimed {
                    let lag = (claim.claimed_at - claim.scheduled_at)
                        .to_std()
                        .map(|duration| duration.as_secs_f64())
                        .unwrap_or(0.0);
                    metrics::histogram!("scheduler_lag_seconds").record(lag);
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
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
                        metrics::counter!("payg_scheduler_suspended_tenant_skips_total")
                            .increment(1);
                        metrics::counter!("checks_skipped_total", "reason" => "tenant_inactive")
                            .increment(1);
                        scheduling_repository
                            .complete_claim_with_spread(
                                claim.monitor.id,
                                lease_owner,
                                deterministic_spread_seconds,
                            )
                            .await?;
                        continue;
                    }

                    // TASK 5 — resolve subscription entitlements (replaces
                    // the old wallet/PAYG gate).
                    let entitlements = self
                        .resolve_entitlements(claim.monitor.tenant_id)
                        .await
                        .unwrap_or_else(|error| {
                            warn!(
                                monitor_id = %claim.monitor.id.0,
                                error = %error,
                                "failed to resolve scheduler entitlement; skipping execution"
                            );
                            None
                        });

                    // Defense in depth: if the tenant currently has more
                    // monitors than the plan allows, skip this claim now.
                    // (Reconciliation should already have suspended it.)
                    if let (Some(ent), Some(mon_repo)) =
                        (&entitlements, &self.monitor_repository)
                    {
                        let total = mon_repo
                            .count_for_tenant(claim.monitor.tenant_id)
                            .await
                            .unwrap_or(0);
                        if total > u64::from(ent.max_uptime_monitors) {
                            metrics::counter!(
                                "checks_skipped_total",
                                "reason" => "over_entitlement"
                            )
                            .increment(1);
                            metrics::counter!("scheduler_entitlement_skips_total").increment(1);
                            scheduling_repository
                                .complete_claim_with_spread(
                                    claim.monitor.id,
                                    lease_owner,
                                    deterministic_spread_seconds,
                                )
                                .await?;
                            continue;
                        }
                    }

                    let monitor = claim.monitor;

                    tasks.spawn(async move {
                        let _request_id =
                            format!("monitor-check:{}:{}", monitor.id.0, claim.execution_id);
                        // TASK 5 — wallet no longer authorizes monitor
                        // execution. The DB claim filter excludes suspended
                        // monitors, so if we're here the monitor is
                        // entitled to run. Usage metering is omitted from
                        // this path; TASK 8 decides whether to re-introduce
                        // analytics-only recording.
                        let result: Result<()> = if let Some(ent) = entitlements {
                            if monitor.interval_seconds < ent.min_uptime_interval_seconds {
                                metrics::counter!("scheduler_entitlement_skips_total")
                                    .increment(1);
                                metrics::counter!(
                                    "checks_skipped_total",
                                    "reason" => "interval_below_plan"
                                )
                                .increment(1);
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
                                        "failed to release interval-skipped claim"
                                    );
                                }
                                return Ok::<(), Box<dyn Error + Send + Sync>>(());
                            }
                            metrics::counter!(
                                "checks_executed_total",
                                "kind" => domain::monitor_operation(monitor.kind, false).as_str()
                            )
                            .increment(1);
                            match execute_with_controls(
                                executor.clone(),
                                monitor.clone(),
                                execution_limiter.clone(),
                                execution_min_interval,
                                execution_retry_limit,
                                execution_retry_base,
                                jitter,
                            )
                            .await
                            {
                                Ok(outcome) => {
                                    if outcome.error.is_some() {
                                        metrics::counter!(
                                            "checks_failed_total",
                                            "kind" => "monitor"
                                        )
                                        .increment(1);
                                    }
                                    Ok(())
                                }
                                Err(err) => Err(err),
                            }
                        } else {
                            // Resolver not wired OR resolution failed —
                            // fail safe by skipping.
                            metrics::counter!(
                                "checks_skipped_total",
                                "reason" => "no_entitlement_resolver"
                            )
                            .increment(1);
                            Ok(())
                        };

                        if let Err(err) = &result {
                            if matches!(err.downcast_ref::<application::ChargeForUsageError>(), Some(application::ChargeForUsageError::InsufficientBalance | application::ChargeForUsageError::SpendLimitExceeded)) {
                                metrics::counter!("checks_skipped_total", "reason" => "billing_block")
                                    .increment(1);
                            } else {
                                metrics::counter!("checks_failed_total", "kind" => "monitor")
                                    .increment(1);
                            }
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

/// Converts raw byte counts to whole 1 MiB (1024*1024 byte) units,
/// rounding up. Returns 0 for 0 bytes.
pub fn network_egress_units(bytes: u64) -> u64 {
    bytes.div_ceil(1024 * 1024)
}

#[cfg(test)]
mod network_egress_tests {
    use super::network_egress_units;

    #[test]
    fn network_egress_uses_binary_mb_and_rounds_up() {
        assert_eq!(network_egress_units(0), 0);
        assert_eq!(network_egress_units(1), 1);
        assert_eq!(network_egress_units(1024 * 1024), 1);
        assert_eq!(network_egress_units((1024 * 1024) + 1), 2);
    }
}
