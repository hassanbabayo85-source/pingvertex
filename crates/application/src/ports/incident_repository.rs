use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Abstraction for incident persistence.
#[async_trait]
pub trait IncidentRepository: Send + Sync {
    async fn save(&self, incident: &Incident) -> RepoResult<()>;

    async fn update(&self, incident: &Incident) -> RepoResult<()>;

    async fn find_by_id(&self, id: IncidentId) -> RepoResult<Option<Incident>>;

    async fn find_open_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Option<Incident>>;

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Incident>>;

    async fn list_open_unacknowledged(&self) -> RepoResult<Vec<Incident>> {
        Ok(vec![])
    }

    async fn touch_last_seen(&self, monitor_id: MonitorId, now: DateTime<Utc>) -> RepoResult<()> {
        let _ = (monitor_id, now);
        Ok(())
    }

    /// Attempts to save a new incident, returning `true` if the row was
    /// actually inserted, or `false` if a conflict prevented insertion
    /// (e.g. an open incident already exists for the same monitor).
    /// Used by `IncidentManager` to decide whether to emit a
    /// `MonitorDown` notification without racing concurrent opens.
    ///
    /// Defaults to `save` + `true` for simple mocks. The Postgres
    /// implementation overrides this with an atomic
    /// `INSERT ... ON CONFLICT ... DO NOTHING` that reports the actual
    /// rows affected.
    async fn save_and_report(&self, incident: &Incident) -> RepoResult<bool> {
        self.save(incident).await?;
        Ok(true)
    }

    /// Attempts to resolve an open incident, returning `true` if the
    /// update actually changed the row (i.e. the incident was open and
    /// is now resolved), or `false` if it was already resolved or did
    /// not exist. Used by `IncidentManager` to avoid duplicate
    /// `MonitorRecovered` notifications under concurrency.
    ///
    /// Defaults to `update` + `true` for simple mocks. The Postgres
    /// implementation overrides this with a conditional `UPDATE ...
    /// WHERE resolved_at IS NULL` that reports actual rows affected.
    async fn resolve_if_open(&self, incident: &Incident) -> RepoResult<bool> {
        self.update(incident).await?;
        Ok(true)
    }

    /// Atomically claims the right to send the escalation notification
    /// for `incident`'s CURRENT `escalation_step_index`. If the claim
    /// succeeds, `incident.escalation_step_index` and
    /// `incident.last_escalation_at` are advanced in place to reflect
    /// the newly-persisted state and `Ok(true)` is returned — the caller
    /// may now go ahead and send the notification.
    ///
    /// If another process already claimed this step (or the incident
    /// was acknowledged/resolved in the meantime), returns `Ok(false)`
    /// and leaves `incident` untouched — the caller MUST NOT send a
    /// notification in that case.
    ///
    /// This is the `EscalationWorker` equivalent of `save_and_report` /
    /// `resolve_if_open`: claim-before-act, so concurrent worker sweeps
    /// (multiple replicas, or overlapping sweeps) can never send the
    /// same escalation step twice.
    ///
    /// Default (for simple mocks): always succeeds via `update`, with
    /// no compare-and-swap protection — fine for single-threaded unit
    /// tests, mirroring `resolve_if_open`'s default.
    async fn try_claim_escalation_step(
        &self,
        incident: &mut Incident,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        incident.escalation_step_index += 1;
        incident.last_escalation_at = Some(now);
        incident.updated_at = now;
        self.update(incident).await?;
        Ok(true)
    }
}

#[async_trait]
impl<T> IncidentRepository for Arc<T>
where
    T: IncidentRepository + ?Sized + Send + Sync,
{
    async fn save(&self, incident: &Incident) -> RepoResult<()> {
        (**self).save(incident).await
    }

    async fn update(&self, incident: &Incident) -> RepoResult<()> {
        (**self).update(incident).await
    }

    async fn find_by_id(&self, id: IncidentId) -> RepoResult<Option<Incident>> {
        (**self).find_by_id(id).await
    }

    async fn find_open_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Option<Incident>> {
        (**self).find_open_by_monitor(monitor_id).await
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Incident>> {
        (**self).list_by_monitor(monitor_id).await
    }

    async fn list_open_unacknowledged(&self) -> RepoResult<Vec<Incident>> {
        (**self).list_open_unacknowledged().await
    }

    async fn touch_last_seen(&self, monitor_id: MonitorId, now: DateTime<Utc>) -> RepoResult<()> {
        (**self).touch_last_seen(monitor_id, now).await
    }

    async fn save_and_report(&self, incident: &Incident) -> RepoResult<bool> {
        (**self).save_and_report(incident).await
    }

    async fn resolve_if_open(&self, incident: &Incident) -> RepoResult<bool> {
        (**self).resolve_if_open(incident).await
    }

    async fn try_claim_escalation_step(
        &self,
        incident: &mut Incident,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        (**self).try_claim_escalation_step(incident, now).await
    }
}
