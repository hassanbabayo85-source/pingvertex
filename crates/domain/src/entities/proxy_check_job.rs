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
    started_at: Option<DateTime<Utc>>,
    heartbeat_at: Option<DateTime<Utc>>,
    worker_id: Option<String>,
    completed_at: Option<DateTime<Utc>>,
    canceled_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
    failure_reason: Option<String>,
    retry_count: u32,
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
            started_at: None,
            heartbeat_at: None,
            worker_id: None,
            completed_at: None,
            canceled_at: None,
            failed_at: None,
            failure_reason: None,
            retry_count: 0,
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
        started_at: Option<DateTime<Utc>>,
        heartbeat_at: Option<DateTime<Utc>>,
        worker_id: Option<String>,
        completed_at: Option<DateTime<Utc>>,
        canceled_at: Option<DateTime<Utc>>,
        failed_at: Option<DateTime<Utc>>,
        failure_reason: Option<String>,
        retry_count: u32,
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
            started_at,
            heartbeat_at,
            worker_id,
            completed_at,
            canceled_at,
            failed_at,
            failure_reason,
            retry_count,
            total_count,
            checked_count,
            successful_count,
            failed_count,
        }
    }

    pub fn id(&self) -> ProxyCheckJobId {
        self.id
    }
    pub fn user_id(&self) -> UserId {
        self.user_id
    }
    pub fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
    pub fn status(&self) -> ProxyCheckJobStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }
    pub fn heartbeat_at(&self) -> Option<DateTime<Utc>> {
        self.heartbeat_at
    }
    pub fn worker_id(&self) -> Option<&str> {
        self.worker_id.as_deref()
    }
    pub fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
    pub fn canceled_at(&self) -> Option<DateTime<Utc>> {
        self.canceled_at
    }
    pub fn failed_at(&self) -> Option<DateTime<Utc>> {
        self.failed_at
    }
    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }
    pub fn total_count(&self) -> u32 {
        self.total_count
    }
    pub fn checked_count(&self) -> u32 {
        self.checked_count
    }
    pub fn successful_count(&self) -> u32 {
        self.successful_count
    }
    pub fn failed_count(&self) -> u32 {
        self.failed_count
    }

    /// Used by domain-only tests and non-persistent callers. Production
    /// workers claim atomically through the repository before executing.
    pub fn mark_running(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Pending {
            return false;
        }
        self.status = ProxyCheckJobStatus::Running;
        self.started_at = Some(Utc::now());
        self.heartbeat_at = self.started_at;
        self.completed_at = None;
        self.canceled_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        true
    }

    pub fn set_worker_lease(&mut self, worker_id: impl Into<String>, heartbeat_at: DateTime<Utc>) {
        self.status = ProxyCheckJobStatus::Running;
        if self.started_at.is_none() {
            self.started_at = Some(heartbeat_at);
        }
        self.heartbeat_at = Some(heartbeat_at);
        self.worker_id = Some(worker_id.into());
        self.completed_at = None;
        self.canceled_at = None;
        self.failed_at = None;
        self.failure_reason = None;
    }

    pub fn record_result(&mut self, successful: bool) {
        if self.checked_count >= self.total_count {
            return;
        }
        self.checked_count += 1;
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
        self.canceled_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        self.heartbeat_at = None;
        self.worker_id = None;
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
        reason.truncate(1024);
        self.status = ProxyCheckJobStatus::Failed;
        self.completed_at = None;
        self.canceled_at = None;
        self.failed_at = Some(Utc::now());
        self.failure_reason = Some(reason);
        self.heartbeat_at = None;
        self.worker_id = None;
        true
    }

    pub fn mark_canceled(&mut self) -> bool {
        if !matches!(
            self.status,
            ProxyCheckJobStatus::Pending | ProxyCheckJobStatus::Running
        ) {
            return false;
        }
        self.status = ProxyCheckJobStatus::Canceled;
        self.completed_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        self.canceled_at = Some(Utc::now());
        self.heartbeat_at = None;
        self.worker_id = None;
        true
    }
}

impl Ownable for ProxyCheckJob {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}
impl TenantOwned for ProxyCheckJob {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}
