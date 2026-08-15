use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::ProxyCheckJobStatus;
use crate::errors::DomainError;
use crate::shared::ids::{ProxyCheckJobId, UserId};

/// Hard cap on the number of proxy entries a single proxy-check job may
/// contain.
///
/// Like `ApiFlow::MAX_STEPS`, "bulk" does not mean "unbounded": a job with
/// an unlimited number of entries would let a client force the worker to
/// dial an unbounded number of outbound connections in a single job,
/// tying up worker capacity indefinitely — effectively a DoS vector
/// against the worker pool. 500 comfortably covers real bulk-upload use
/// cases while keeping a single job's blast radius bounded.
pub const MAX_ENTRIES_PER_JOB: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyCheckJob {
    id: ProxyCheckJobId,
    user_id: UserId,
    status: ProxyCheckJobStatus,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    total_count: u32,
    checked_count: u32,
}

impl ProxyCheckJob {
    pub fn new(
        id: ProxyCheckJobId,
        user_id: UserId,
        entry_count: usize,
    ) -> Result<Self, DomainError> {
        if entry_count == 0 {
            return Err(DomainError::EmptyProxyEntries);
        }

        if entry_count > MAX_ENTRIES_PER_JOB {
            return Err(DomainError::TooManyProxyEntries);
        }

        Ok(Self {
            id,
            user_id,
            status: ProxyCheckJobStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
            total_count: entry_count as u32,
            checked_count: 0,
        })
    }

    /// Transitions the job from `Pending` to `Running`.
    ///
    /// This is a no-op — the status is left untouched and `false` is
    /// returned — unless the job is currently `Pending`. This state
    /// machine invariant is what prevents a race condition where two
    /// workers both pick up the same job and both start "running" it
    /// concurrently: only the first caller to observe `Pending` can make
    /// the transition succeed. The caller (the worker persistence layer,
    /// Prompt 8/10) should treat the returned `bool` as an
    /// optimistic-lock signal — e.g. only commit the corresponding
    /// database update if this returns `true`. This invariant lives here,
    /// on the entity, so no other layer can bypass it.
    pub fn mark_running(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Pending {
            return false;
        }

        self.status = ProxyCheckJobStatus::Running;
        true
    }

    /// Records progress, capping at `total_count` so a caller cannot
    /// record more checked entries than the job actually has.
    pub fn record_progress(&mut self, checked: u32) {
        self.checked_count = checked.min(self.total_count);
    }

    pub fn mark_completed(&mut self) {
        self.status = ProxyCheckJobStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = ProxyCheckJobStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    pub fn id(&self) -> ProxyCheckJobId {
        self.id
    }

    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn status(&self) -> &ProxyCheckJobStatus {
        &self.status
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }

    pub fn total_count(&self) -> u32 {
        self.total_count
    }

    pub fn checked_count(&self) -> u32 {
        self.checked_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_pending_with_zero_progress() {
        let job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 5).unwrap();

        assert_eq!(job.status(), &ProxyCheckJobStatus::Pending);
        assert_eq!(job.total_count(), 5);
        assert_eq!(job.checked_count(), 0);
        assert!(job.completed_at().is_none());
    }

    #[test]
    fn rejects_zero_entries() {
        let result = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 0);

        assert_eq!(result.err(), Some(DomainError::EmptyProxyEntries));
    }

    #[test]
    fn rejects_too_many_entries() {
        let result = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 501);

        assert_eq!(result.err(), Some(DomainError::TooManyProxyEntries));
    }

    #[test]
    fn mark_running_succeeds_from_pending() {
        let mut job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 5).unwrap();

        assert!(job.mark_running());
        assert_eq!(job.status(), &ProxyCheckJobStatus::Running);
    }

    #[test]
    fn mark_running_is_a_no_op_when_not_pending() {
        let mut job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 5).unwrap();
        assert!(job.mark_running());

        // Job is now Running, not Pending — a second caller must not be
        // able to "run" it again.
        assert!(!job.mark_running());
        assert_eq!(job.status(), &ProxyCheckJobStatus::Running);
    }

    #[test]
    fn record_progress_caps_at_total_count() {
        let mut job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 5).unwrap();

        job.record_progress(999);

        assert_eq!(job.checked_count(), 5);
    }

    #[test]
    fn mark_completed_sets_status_and_completed_at() {
        let mut job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 5).unwrap();

        job.mark_completed();

        assert_eq!(job.status(), &ProxyCheckJobStatus::Completed);
        assert!(job.completed_at().is_some());
    }

    #[test]
    fn mark_failed_sets_status_and_completed_at() {
        let mut job = ProxyCheckJob::new(ProxyCheckJobId::new(), UserId::new(), 5).unwrap();

        job.mark_failed();

        assert_eq!(job.status(), &ProxyCheckJobStatus::Failed);
        assert!(job.completed_at().is_some());
    }
}
