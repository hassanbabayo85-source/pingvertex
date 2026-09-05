use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    enums::ProxyCheckJobStatus,
    shared::{
        ids::{ProxyCheckJobId, TenantId, UserId},
        policies::{Ownable, TenantOwned},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyCheckJob {
    id: ProxyCheckJobId,
    user_id: UserId,
    tenant_id: TenantId,
    status: ProxyCheckJobStatus,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
    failure_reason: Option<String>,
    total_count: u32,
    checked_count: u32,
    successful_count: u32,
    failed_count: u32,
}

impl ProxyCheckJob {
    pub fn new(
        id: ProxyCheckJobId,
        user_id: UserId,
        tenant_id: TenantId,
        total_count: u32,
    ) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            status: ProxyCheckJobStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
            failed_at: None,
            failure_reason: None,
            total_count,
            checked_count: 0,
            successful_count: 0,
            failed_count: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_persisted(
        id: ProxyCheckJobId,
        user_id: UserId,
        tenant_id: TenantId,
        status: ProxyCheckJobStatus,
        created_at: DateTime<Utc>,
        completed_at: Option<DateTime<Utc>>,
        failed_at: Option<DateTime<Utc>>,
        failure_reason: Option<String>,
        total_count: u32,
        checked_count: u32,
        successful_count: u32,
        failed_count: u32,
    ) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            status,
            created_at,
            completed_at,
            failed_at,
            failure_reason,
            total_count,
            checked_count,
            successful_count,
            failed_count,
        }
    }

    pub fn id(&self) -> ProxyCheckJobId { self.id }
    pub fn user_id(&self) -> UserId { self.user_id }
    pub fn tenant_id(&self) -> TenantId { self.tenant_id }
    pub fn status(&self) -> ProxyCheckJobStatus { self.status }
    pub fn created_at(&self) -> DateTime<Utc> { self.created_at }
    pub fn completed_at(&self) -> Option<DateTime<Utc>> { self.completed_at }
    pub fn failed_at(&self) -> Option<DateTime<Utc>> { self.failed_at }
    pub fn failure_reason(&self) -> Option<&str> { self.failure_reason.as_deref() }
    pub fn total_count(&self) -> u32 { self.total_count }
    pub fn checked_count(&self) -> u32 { self.checked_count }
    pub fn successful_count(&self) -> u32 { self.successful_count }
    pub fn failed_count(&self) -> u32 { self.failed_count }

    pub fn mark_running(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Pending {
            return false;
        }
        self.status = ProxyCheckJobStatus::Running;
        self.completed_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        true
    }

    pub fn record_progress(&mut self, checked_count: u32) {
        self.checked_count = checked_count.min(self.total_count);
    }

    pub fn record_result(&mut self, successful: bool) {
        if self.checked_count < self.total_count {
            self.checked_count += 1;
        }
        if successful {
            self.successful_count = self.successful_count.saturating_add(1);
        } else {
            self.failed_count = self.failed_count.saturating_add(1);
        }
    }

    pub fn mark_completed(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Running {
            return false;
        }
        self.status = ProxyCheckJobStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.failed_at = None;
        self.failure_reason = None;
        true
    }

    pub fn mark_failed(&mut self, reason: impl Into<String>) -> bool {
        if self.status != ProxyCheckJobStatus::Running {
            return false;
        }
        let mut reason = reason.into();
        if reason.is_empty() {
            reason = "proxy check job failed".to_owned();
        }
        // Failure details are operational metadata, not a log sink. Bound
        // storage and avoid accidentally retaining huge remote error bodies.
        reason.truncate(1024);
        self.status = ProxyCheckJobStatus::Failed;
        self.completed_at = None;
        self.failed_at = Some(Utc::now());
        self.failure_reason = Some(reason);
        true
    }
}

impl Ownable for ProxyCheckJob {
    fn owner_id(&self) -> UserId { self.user_id }
}

impl TenantOwned for ProxyCheckJob {
    fn tenant_id(&self) -> TenantId { self.tenant_id }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> ProxyCheckJob {
        ProxyCheckJob::new(
            ProxyCheckJobId::new(),
            UserId::new(),
            TenantId::new(),
            2,
        )
    }

    #[test]
    fn only_pending_can_become_running() {
        let mut job = job();
        assert!(job.mark_running());
        assert!(!job.mark_running());
    }

    #[test]
    fn running_can_complete_but_completed_cannot_restart() {
        let mut job = job();
        assert!(job.mark_running());
        assert!(job.mark_completed());
        assert!(!job.mark_running());
        assert_eq!(job.status(), ProxyCheckJobStatus::Completed);
    }

    #[test]
    fn running_can_fail_but_failed_cannot_restart() {
        let mut job = job();
        assert!(job.mark_running());
        assert!(job.mark_failed("database unavailable"));
        assert!(!job.mark_running());
        assert_eq!(job.status(), ProxyCheckJobStatus::Failed);
        assert!(job.failure_reason().is_some());
    }
}
