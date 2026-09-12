use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use application::ports::{
    AuditLogEntryRepository, BulkMonitorActionJob, BulkMonitorActionRepository, MonitorRepository,
};
use chrono::Utc;
use domain::AuditLogEntry;
use tokio::time::sleep;
use tracing::{error, info};
use uuid::Uuid;

pub struct BulkMonitorActionWorker {
    jobs: Arc<dyn BulkMonitorActionRepository>,
    monitors: Arc<dyn MonitorRepository>,
    audits: Arc<dyn AuditLogEntryRepository>,
    poll_interval: Duration,
    lease_duration: Duration,
    batch_size: u32,
}

impl BulkMonitorActionWorker {
    pub fn new(
        jobs: Arc<dyn BulkMonitorActionRepository>,
        monitors: Arc<dyn MonitorRepository>,
        audits: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            jobs,
            monitors,
            audits,
            poll_interval: Duration::from_millis(250),
            lease_duration: Duration::from_secs(600),
            batch_size: 20,
        }
    }

    pub async fn run_forever(
        self,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let worker_id = Uuid::new_v4();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = sleep(self.poll_interval) => {}
            }
            let queue_depth = self.jobs.queue_depth().await?;
            metrics::gauge!("payg_bulk_action_queue_depth").set(queue_depth as f64);
            let jobs = self
                .jobs
                .claim_jobs(
                    worker_id,
                    self.batch_size,
                    Utc::now() + chrono::Duration::from_std(self.lease_duration).unwrap(),
                )
                .await?;
            for job in jobs {
                let started = Instant::now();
                if let Err(err) = self.process(worker_id, job.clone()).await {
                    metrics::counter!("payg_bulk_action_failures_total", "action" => job.action.clone())
                        .increment(1);
                    metrics::histogram!("payg_bulk_action_duration_seconds")
                        .record(started.elapsed().as_secs_f64());
                    error!(job_id=%job.id, error=%err, "bulk monitor action failed");
                    let _ = self.jobs.fail(job.id, worker_id, &err.to_string()).await;
                } else {
                    metrics::counter!("payg_bulk_action_completed_total", "action" => job.action.clone())
                        .increment(1);
                    metrics::histogram!("payg_bulk_action_duration_seconds")
                        .record(started.elapsed().as_secs_f64());
                }
            }
        }
    }

    async fn process(
        &self,
        worker_id: Uuid,
        job: BulkMonitorActionJob,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let enabled = matches!(job.action.as_str(), "enable" | "resume");
        let ids = job.monitor_ids.as_deref().map(|ids| {
            ids.iter()
                .map(|id| domain::shared::ids::MonitorId(*id))
                .collect::<Vec<_>>()
        });
        let affected = self
            .monitors
            .set_enabled_for_tenant(domain::TenantId(job.tenant_id), ids.as_deref(), enabled)
            .await?;
        let audit_action = match job.action.as_str() {
            "enable" => domain::AuditAction::BulkMonitorEnabled,
            "disable" => domain::AuditAction::BulkMonitorDisabled,
            "pause" => domain::AuditAction::BulkMonitorPaused,
            "resume" => domain::AuditAction::BulkMonitorResumed,
            _ => return Err("invalid bulk monitor action".into()),
        };
        self.audits.create(&AuditLogEntry {
            id: domain::AuditLogEntryId::new(), actor_user_id: domain::shared::ids::UserId(job.actor_user_id),
            tenant_id: Some(domain::TenantId(job.tenant_id)), action: audit_action,
            target_id: if job.all { format!("tenant:{}", job.tenant_id) } else { format!("bulk-job:{}", job.id) },
            metadata: serde_json::json!({"bulk":true,"action":job.action,"scope":if job.all{"all"}else{"selected"},"affected":affected,"reason":job.reason,"job_id":job.id}),
            created_at: Utc::now(),
        }).await?;
        if !self.jobs.complete(job.id, worker_id, affected).await? {
            return Err("bulk action job lease was lost before completion".into());
        }
        info!(job_id=%job.id, affected, "bulk monitor action completed");
        Ok(())
    }
}
