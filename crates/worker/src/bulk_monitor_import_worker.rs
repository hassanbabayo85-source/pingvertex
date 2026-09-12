use std::{error::Error, sync::Arc, time::Duration};

use application::{
    dto::{BulkMonitorImportJob, BulkMonitorImportJobStatus, BulkMonitorImportRow},
    ports::{
        AuditLogEntryRepository, BulkMonitorImportRepository, MonitorIntervalPolicyRepository,
        MonitorRepository, PricingConfigRepository, TenantMembershipRepository, TenantRepository,
        WalletRepository,
    },
    use_cases::{CreateMonitorError, CreateMonitorUseCase},
};
use chrono::{Duration as ChronoDuration, Utc};
use domain::{
    TenantStatus,
    enums::PlanTier,
    shared::{
        ids::{MonitorId, TenantId, UserId},
        types::{AuthenticatedUser, Permission, TenantContext},
    },
};
use infra::UuidIdGenerator;
use rand::{Rng, rng};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct BulkMonitorImportWorker {
    jobs: Arc<dyn BulkMonitorImportRepository>,
    monitors: Arc<dyn MonitorRepository>,
    memberships: Arc<dyn TenantMembershipRepository>,
    audits: Arc<dyn AuditLogEntryRepository>,
    wallets: Arc<dyn WalletRepository>,
    interval_policies: Arc<dyn MonitorIntervalPolicyRepository>,
    tenant_repository: Arc<dyn TenantRepository>,
    pricing_config_repository: Arc<dyn PricingConfigRepository>,
    worker_id: Uuid,
    poll_interval: Duration,
    lease_duration: Duration,
    max_jobs_per_poll: u32,
    max_row_attempts: i32,
    batch_pause: Duration,
    jitter_ms: u64,
    pause_check_rows: usize,
}

impl BulkMonitorImportWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        jobs: Arc<dyn BulkMonitorImportRepository>,
        monitors: Arc<dyn MonitorRepository>,
        memberships: Arc<dyn TenantMembershipRepository>,
        audits: Arc<dyn AuditLogEntryRepository>,
        wallets: Arc<dyn WalletRepository>,
        interval_policies: Arc<dyn MonitorIntervalPolicyRepository>,
        tenant_repository: Arc<dyn TenantRepository>,
        pricing_config_repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        Self {
            jobs,
            monitors,
            memberships,
            audits,
            wallets,
            interval_policies,
            tenant_repository,
            pricing_config_repository,
            worker_id: Uuid::new_v4(),
            poll_interval: Duration::from_secs(env_u64("BULK_IMPORT_POLL_SECONDS", 2, 1, 60)),
            lease_duration: Duration::from_secs(env_u64("BULK_IMPORT_LEASE_SECONDS", 60, 15, 600)),
            max_jobs_per_poll: env_u64("BULK_IMPORT_MAX_JOBS", 2, 1, 20) as u32,
            max_row_attempts: env_u64("BULK_IMPORT_MAX_ROW_ATTEMPTS", 5, 1, 20) as i32,
            batch_pause: Duration::from_millis(env_u64(
                "BULK_IMPORT_BATCH_PAUSE_MS",
                100,
                0,
                5_000,
            )),
            jitter_ms: env_u64("BULK_IMPORT_JITTER_MS", 250, 0, 5_000),
            pause_check_rows: env_u64("BULK_IMPORT_PAUSE_CHECK_ROWS", 25, 1, 500) as usize,
        }
    }

    pub async fn run_forever(
        &self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = self.run_once() => {
                    if let Err(error) = result {
                        tracing::error!(error=%error, "bulk monitor import poll failed");
                    }
                }
            }

            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = tokio::time::sleep(self.poll_interval) => {}
            }
        }
    }

    async fn run_once(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let jobs = self
            .jobs
            .claim_jobs(
                self.worker_id,
                self.max_jobs_per_poll,
                Utc::now() + chrono_duration(self.lease_duration),
            )
            .await?;

        metrics::gauge!("payg_bulk_queue_depth").set(jobs.len() as f64);
        for job in jobs {
            metrics::counter!("payg_bulk_jobs_claimed_total").increment(1);
            if let Err(error) = self.process_job(&job).await {
                tracing::error!(
                    job_id=%job.id,
                    tenant_id=%job.tenant_id,
                    error=%error,
                    "bulk monitor import job failed transiently"
                );
                self.jobs
                    .release_job_after_error(
                        job.id,
                        self.worker_id,
                        &bounded_error(&error.to_string()),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn process_job(
        &self,
        job: &BulkMonitorImportJob,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if job.status != BulkMonitorImportJobStatus::Running {
            return Ok(());
        }

        let tenant_id = TenantId(job.tenant_id);
        let actor_user_id = UserId(job.actor_user_id);
        let batch_size = u32::try_from(job.batch_size.clamp(1, 500)).unwrap_or(500);

        loop {
            let lease_ok = self
                .jobs
                .renew_job_lease(
                    job.id,
                    self.worker_id,
                    Utc::now() + chrono_duration(self.lease_duration),
                )
                .await?;
            if !lease_ok {
                return Ok(());
            }

            let tenant = self
                .tenant_repository
                .find_by_id(tenant_id)
                .await?
                .ok_or("bulk import tenant no longer exists")?;
            if tenant.status != TenantStatus::Active {
                self.jobs.pause_job(job.id, job.tenant_id).await?;
                return Ok(());
            }

            let rows = self
                .jobs
                .claim_rows(
                    job.id,
                    batch_size,
                    Utc::now() - chrono_duration(self.lease_duration),
                )
                .await?;
            if rows.is_empty() {
                let refreshed = self.jobs.refresh_job_state(job.id, self.worker_id).await?;
                if refreshed.status == BulkMonitorImportJobStatus::Completed {
                    let action = if refreshed.failed_rows > 0 {
                        domain::AuditAction::BulkImportPartiallyFailed
                    } else {
                        domain::AuditAction::BulkImportCompleted
                    };
                    let _ = self.audits.create(&domain::AuditLogEntry {
                        id: domain::AuditLogEntryId::new(), actor_user_id, tenant_id: Some(tenant_id), action,
                        target_id: format!("bulk-import:{}", job.id),
                        metadata: serde_json::json!({"created_rows": refreshed.created_rows, "failed_rows": refreshed.failed_rows, "invalid_rows": refreshed.invalid_rows}),
                        created_at: Utc::now(),
                    }).await;
                    tracing::info!(
                        job_id=%job.id,
                        tenant_id=%job.tenant_id,
                        created_rows=refreshed.created_rows,
                        failed_rows=refreshed.failed_rows,
                        invalid_rows=refreshed.invalid_rows,
                        "bulk monitor import completed"
                    );
                    return Ok(());
                }

                // Remaining rows are waiting for retry backoff. Keep the lease
                // alive and avoid hammering Postgres.
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }

            if !self
                .jobs
                .renew_job_lease(
                    job.id,
                    self.worker_id,
                    Utc::now() + chrono_duration(self.lease_duration),
                )
                .await?
            {
                for row in rows {
                    self.jobs
                        .retry_row(
                            row.id,
                            "bulk import paused or lease ownership changed",
                            Utc::now(),
                        )
                        .await?;
                }
                return Ok(());
            }

            let membership = self
                .memberships
                .find_by_tenant_and_user(tenant_id, actor_user_id)
                .await?;
            let can_create = membership.as_ref().is_some_and(|membership| {
                membership.role.has_permission(Permission::CreateMonitor)
            });
            let entitlement = self.resolve_entitlement_policy(tenant_id).await?;
            let monitor_count = self.monitors.find_all_for_tenant(tenant_id).await?.len() as u32;
            let wallet_activated = self
                .wallets
                .find(tenant_id.0)
                .await?
                .is_some_and(|wallet| wallet.payg_activated_at.is_some());
            let free_entitled = entitlement.is_free_plan
                && !wallet_activated
                && monitor_count <= entitlement.plan.max_monitors;

            for (index, row) in rows.into_iter().enumerate() {
                if index % self.pause_check_rows == 0 {
                    let lease_alive = self
                        .jobs
                        .renew_job_lease(
                            job.id,
                            self.worker_id,
                            Utc::now() + chrono_duration(self.lease_duration),
                        )
                        .await?;
                    let tenant_active = self
                        .tenant_repository
                        .find_by_id(tenant_id)
                        .await?
                        .is_some_and(|tenant| tenant.status == domain::TenantStatus::Active);
                    if !lease_alive || !tenant_active {
                        if !tenant_active {
                            self.jobs.pause_job(job.id, job.tenant_id).await?;
                        }
                        self.jobs
                            .retry_row(
                                row.id,
                                if tenant_active {
                                    "bulk import paused by operator"
                                } else {
                                    "tenant is suspended; bulk import paused safely"
                                },
                                Utc::now(),
                            )
                            .await?;
                        return Ok(());
                    }
                }

                if !can_create {
                    metrics::counter!("payg_bulk_rows_failed_total").increment(1);
                    self.jobs
                        .fail_row(
                            row.id,
                            "actor no longer has CreateMonitor permission in this tenant",
                        )
                        .await?;
                    continue;
                }

                let planned_id = MonitorId(row.planned_monitor_id);
                if let Some(existing) = self.monitors.find_by_id(planned_id).await? {
                    if existing.tenant_id != tenant_id {
                        metrics::counter!("payg_bulk_rows_failed_total").increment(1);
                        self.jobs
                            .fail_row(row.id, "planned monitor id collided across tenant boundary")
                            .await?;
                    } else {
                        self.jobs
                            .mark_row_created(row.id, row.planned_monitor_id)
                            .await?;
                        metrics::counter!("payg_bulk_rows_processed_total").increment(1);
                    }
                    continue;
                }

                let payload: BulkMonitorImportRow =
                    match serde_json::from_value(row.payload.clone()) {
                        Ok(payload) => payload,
                        Err(error) => {
                            self.jobs
                                .fail_row(
                                    row.id,
                                    &bounded_error(&format!(
                                        "stored row payload is invalid: {error}"
                                    )),
                                )
                                .await?;
                            continue;
                        }
                    };

                if free_entitled
                    && payload.interval_seconds < entitlement.plan.min_check_interval_seconds
                {
                    self.jobs
                        .skip_row(
                            row.id,
                            &format!(
                                "free entitlement requires an interval of at least {} seconds",
                                entitlement.plan.min_check_interval_seconds
                            ),
                        )
                        .await?;
                    continue;
                }

                let membership = membership
                    .as_ref()
                    .expect("membership checked before row processing");
                let authenticated_user = AuthenticatedUser::new(
                    actor_user_id,
                    membership.role,
                    TenantContext::with_actor(tenant_id, actor_user_id),
                );

                let create = CreateMonitorUseCase::new(
                    self.monitors.clone(),
                    UuidIdGenerator::new(),
                    self.audits.clone(),
                )
                .with_wallet_repository(self.wallets.clone())
                .with_interval_policy_repository(self.interval_policies.clone())
                .with_pricing_config_repository(self.pricing_config_repository.clone())
                .with_tenant_repository(self.tenant_repository.clone());

                match create
                    .execute_with_monitor_id(
                        payload.into_create_request(),
                        authenticated_user,
                        planned_id,
                    )
                    .await
                {
                    Ok(_) => {
                        self.jobs
                            .mark_row_created(row.id, row.planned_monitor_id)
                            .await?;
                        metrics::counter!("payg_bulk_rows_processed_total").increment(1);
                    }
                    Err(error) if is_permanent(&error) => {
                        metrics::counter!("payg_bulk_rows_failed_total").increment(1);
                        self.jobs
                            .fail_row(row.id, &bounded_error(&error.to_string()))
                            .await?;
                    }
                    Err(error) if row.attempt_count >= self.max_row_attempts => {
                        metrics::counter!("payg_bulk_rows_failed_total").increment(1);
                        self.jobs
                            .fail_row(
                                row.id,
                                &bounded_error(&format!(
                                    "retry limit reached after {} attempts: {error}",
                                    row.attempt_count
                                )),
                            )
                            .await?;
                    }
                    Err(error) => {
                        let delay = retry_delay(row.attempt_count, self.jitter_ms);
                        self.jobs
                            .retry_row(
                                row.id,
                                &bounded_error(&error.to_string()),
                                Utc::now() + chrono_duration(delay),
                            )
                            .await?;
                    }
                }
            }

            let refreshed = self.jobs.refresh_job_state(job.id, self.worker_id).await?;
            if refreshed.status == BulkMonitorImportJobStatus::Completed {
                tracing::info!(
                    job_id=%job.id,
                    tenant_id=%job.tenant_id,
                    created_rows=refreshed.created_rows,
                    failed_rows=refreshed.failed_rows,
                    invalid_rows=refreshed.invalid_rows,
                    "bulk monitor import completed"
                );
                return Ok(());
            }

            let jitter = if self.jitter_ms == 0 {
                Duration::ZERO
            } else {
                Duration::from_millis(rng().random_range(0..=self.jitter_ms))
            };
            tokio::time::sleep(self.batch_pause.saturating_add(jitter)).await;
        }
    }
}

struct TenantEntitlementPolicy {
    plan: domain::Plan,
    is_free_plan: bool,
}

impl BulkMonitorImportWorker {
    async fn resolve_entitlement_policy(
        &self,
        tenant_id: TenantId,
    ) -> Result<TenantEntitlementPolicy, Box<dyn Error + Send + Sync>> {
        let config = self
            .pricing_config_repository
            .find_active_config()
            .await?
            .ok_or_else(|| "active PAYG pricing config missing".to_owned())?;
        let mut plan = domain::Plan::for_tier(PlanTier::Free);
        plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
        plan.min_check_interval_seconds = config.free_tier.interval_seconds;
        plan.history_retention_days = config.free_tier.retention_days;
        let wallet_activated = self
            .wallets
            .find(tenant_id.0)
            .await?
            .is_some_and(|wallet| wallet.payg_activated_at.is_some());
        Ok(TenantEntitlementPolicy {
            plan,
            is_free_plan: !wallet_activated,
        })
    }
}

fn is_permanent(error: &CreateMonitorError) -> bool {
    matches!(
        error,
        CreateMonitorError::Forbidden
            | CreateMonitorError::QuotaExceeded { .. }
            | CreateMonitorError::TooManyAssertions { .. }
            | CreateMonitorError::IntervalBelowPlanMinimum { .. }
            | CreateMonitorError::TenantSuspended
    )
}

fn retry_delay(attempt: i32, jitter_ms: u64) -> Duration {
    let exponent = u32::try_from(attempt.saturating_sub(1)).unwrap_or(0).min(7);
    let base_ms = 250u64.saturating_mul(1u64 << exponent).min(30_000);
    let jitter = if jitter_ms == 0 {
        0
    } else {
        rng().random_range(0..=jitter_ms)
    };
    Duration::from_millis(base_ms.saturating_add(jitter))
}

fn chrono_duration(duration: Duration) -> ChronoDuration {
    ChronoDuration::from_std(duration).unwrap_or_else(|_| ChronoDuration::seconds(60))
}

fn bounded_error(error: &str) -> String {
    const MAX: usize = 1_000;
    if error.len() <= MAX {
        error.to_owned()
    } else {
        error.chars().take(MAX).collect()
    }
}

fn env_u64(name: &str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_backoff_is_bounded() {
        let delay = retry_delay(100, 0);
        assert_eq!(delay, Duration::from_secs(30));
    }

    #[test]
    fn bounded_error_limits_persisted_text() {
        let error = "x".repeat(2_000);
        assert_eq!(bounded_error(&error).len(), 1_000);
    }
}
