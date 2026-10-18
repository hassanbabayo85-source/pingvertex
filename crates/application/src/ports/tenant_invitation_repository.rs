use std::error::Error;
use std::sync::Arc;

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{InvitationId, TenantId, TenantInvitation};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `TenantInvitation`.
#[async_trait]
pub trait TenantInvitationRepository: Send + Sync {
    async fn create(&self, invitation: &TenantInvitation) -> RepoResult<()>;

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<TenantInvitation>>;

    async fn find_by_id(&self, id: InvitationId) -> RepoResult<Option<TenantInvitation>>;

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantInvitation>>;

    /// Cursor-paginated variant of `list_by_tenant`.
    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<TenantInvitation>> {
        let all = self.list_by_tenant(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |i| {
            Cursor::new(i.created_at, i.id.into_uuid())
        }))
    }

    /// Marks an invitation revoked only if it is still pending.
    async fn mark_revoked(&self, id: InvitationId, revoked_at: DateTime<Utc>) -> RepoResult<()>;

    /// Marks an invitation accepted only if it is still pending.
    async fn mark_accepted(&self, id: InvitationId, accepted_at: DateTime<Utc>) -> RepoResult<()>;

    /// TASK 4A — tenant-scoped count of PENDING invitations (not revoked,
    /// not accepted, not expired) as of `now`. Used alongside
    /// `TenantMembershipRepository::count_for_tenant` to enforce
    /// `max_team_members`.
    async fn count_pending_for_tenant(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<u64> {
        Ok(self
            .list_by_tenant(tenant_id)
            .await?
            .into_iter()
            .filter(|inv| inv.is_pending(now))
            .count() as u64)
    }
}

#[async_trait]
impl<T> TenantInvitationRepository for Arc<T>
where
    T: TenantInvitationRepository + Send + Sync + ?Sized,
{
    async fn create(&self, invitation: &TenantInvitation) -> RepoResult<()> {
        (**self).create(invitation).await
    }

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<TenantInvitation>> {
        (**self).find_by_prefix(token_prefix).await
    }

    async fn find_by_id(&self, id: InvitationId) -> RepoResult<Option<TenantInvitation>> {
        (**self).find_by_id(id).await
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantInvitation>> {
        (**self).list_by_tenant(tenant_id).await
    }

    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<TenantInvitation>> {
        (**self).list_by_tenant_paginated(tenant_id, page).await
    }

    async fn mark_revoked(&self, id: InvitationId, revoked_at: DateTime<Utc>) -> RepoResult<()> {
        (**self).mark_revoked(id, revoked_at).await
    }

    async fn mark_accepted(&self, id: InvitationId, accepted_at: DateTime<Utc>) -> RepoResult<()> {
        (**self).mark_accepted(id, accepted_at).await
    }

    async fn count_pending_for_tenant(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<u64> {
        (**self).count_pending_for_tenant(tenant_id, now).await
    }
}
