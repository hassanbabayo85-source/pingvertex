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
    total_count: u32,
    checked_count: u32,
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
            total_count,
            checked_count: 0,
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
        total_count: u32,
        checked_count: u32,
    ) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            status,
            created_at,
            completed_at,
            total_count,
            checked_count,
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

    pub fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }

    pub fn total_count(&self) -> u32 {
        self.total_count
    }

    pub fn checked_count(&self) -> u32 {
        self.checked_count
    }

    /// Transitions the job from `Pending` to `Running`.
    ///
    /// Returns `false` if the job is no longer `Pending` (another worker
    /// already transitioned it, or it has already finished), making the
    /// caller's optimistic claim a no-op.
    pub fn mark_running(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Pending {
            return false;
        }
        self.status = ProxyCheckJobStatus::Running;
        true
    }

    /// Updates the job's progress counter. The caller is responsible for
    /// persisting this through `ProxyCheckJobRepository::update_job`.
    pub fn record_progress(&mut self, checked_count: u32) {
        self.checked_count = checked_count;
    }

    /// Marks the job as `Completed` and stamps `completed_at` now.
    pub fn mark_completed(&mut self) {
        self.status = ProxyCheckJobStatus::Completed;
        self.completed_at = Some(Utc::now());
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
