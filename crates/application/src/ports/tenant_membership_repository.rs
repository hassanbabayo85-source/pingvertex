use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use crate::ports::monitor_repository::QuotaCheckOutcome;
use domain::{
    TenantMembership,
    shared::{
        ids::{TenantId, UserId},
        types::Role,
    },
};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Outcome of a "last owner" guarded mutation (`remove_member_if_not_last_owner`,
/// `update_role_if_not_last_owner`).
///
/// These operations must check-and-act atomically (locking the tenant's
/// membership rows for the duration of the check) so that two concurrent
/// requests can never both observe "owners > 1" and both proceed, which
/// would leave the tenant with zero Owners. See the 2026-09 security
/// review: SEC-P0-4 (Last Owner Race Condition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastOwnerGuardOutcome {
    /// The mutation was applied.
    Applied,
    /// No membership row existed for that tenant/user pair.
    NotFound,
    /// The mutation was refused because it would have left the tenant
    /// with zero Owners.
    WouldRemoveLastOwner,
}

/// Persistence port for `TenantMembership`.
///
/// `find_by_tenant_and_user` is deliberately first-class here because
/// it is used by the authorization middleware to resolve a user's
/// `TenantContext` and `Role` from the authenticated `UserId`.
///
/// `update_role` exists now even if this feature does not call it yet,
/// so the future `RoleChanged` audit action has a port to write through.
#[async_trait]
pub trait TenantMembershipRepository: Send + Sync {
    async fn create(&self, membership: &TenantMembership) -> RepoResult<()>;

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Option<TenantMembership>>;

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantMembership>>;

    async fn list_by_user(&self, user_id: UserId) -> RepoResult<Vec<TenantMembership>>;

    async fn update_role(&self, tenant_id: TenantId, user_id: UserId, role: Role)
    -> RepoResult<()>;

    /// Deletes a membership row. Returns an error if the row does not exist
    /// is left to the caller; this method performs a `DELETE ... WHERE ...`
    /// and does not error when 0 rows were affected.
    async fn delete(&self, tenant_id: TenantId, user_id: UserId) -> RepoResult<()>;

    /// Atomically deletes `user_id`'s membership in `tenant_id`, refusing
    /// if — and only if — the target is an `Owner` and is the tenant's
    /// last remaining `Owner`.
    ///
    /// The check (counting current Owners) and the act (deleting the row)
    /// MUST happen under the same row lock / transaction so that two
    /// concurrent calls targeting two different Owners can never both
    /// succeed and leave the tenant ownerless. Implementations backed by
    /// Postgres should lock the tenant's membership rows (e.g. `SELECT
    /// ... FOR UPDATE`) before counting.
    async fn remove_member_if_not_last_owner(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<LastOwnerGuardOutcome>;

    /// Atomically changes `user_id`'s role to `new_role` in `tenant_id`,
    /// refusing if — and only if — the target is currently the tenant's
    /// last remaining `Owner` and `new_role` is not `Owner`.
    ///
    /// Same atomicity requirement as `remove_member_if_not_last_owner`.
    async fn update_role_if_not_last_owner(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        new_role: Role,
    ) -> RepoResult<LastOwnerGuardOutcome>;

    /// TASK 4A — tenant-scoped count of ALL members (including the owner).
    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        Ok(self.list_by_tenant(tenant_id).await?.len() as u64)
    }

    /// TASK 4A — atomic count-then-create for membership rows.
    ///
    /// The count includes the owner (they are a `tenant_memberships` row
    /// like any other). `limit = max_team_members` from the tenant's
    /// entitlements. Production override in Postgres uses
    /// `pg_advisory_xact_lock` on a per-tenant key.
    async fn save_if_within_quota(
        &self,
        membership: &TenantMembership,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let count = self.count_for_tenant(tenant_id).await?;
        if count >= u64::from(limit) {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }
        self.create(membership).await?;
        Ok(QuotaCheckOutcome::Saved)
    }
}

#[async_trait]
impl<T> TenantMembershipRepository for Arc<T>
where
    T: TenantMembershipRepository + Send + Sync + ?Sized,
{
    async fn create(&self, membership: &TenantMembership) -> RepoResult<()> {
        (**self).create(membership).await
    }

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Option<TenantMembership>> {
        (**self).find_by_tenant_and_user(tenant_id, user_id).await
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantMembership>> {
        (**self).list_by_tenant(tenant_id).await
    }

    async fn list_by_user(&self, user_id: UserId) -> RepoResult<Vec<TenantMembership>> {
        (**self).list_by_user(user_id).await
    }

    async fn update_role(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        role: Role,
    ) -> RepoResult<()> {
        (**self).update_role(tenant_id, user_id, role).await
    }

    async fn delete(&self, tenant_id: TenantId, user_id: UserId) -> RepoResult<()> {
        (**self).delete(tenant_id, user_id).await
    }

    async fn remove_member_if_not_last_owner(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<LastOwnerGuardOutcome> {
        (**self)
            .remove_member_if_not_last_owner(tenant_id, user_id)
            .await
    }

    async fn update_role_if_not_last_owner(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        new_role: Role,
    ) -> RepoResult<LastOwnerGuardOutcome> {
        (**self)
            .update_role_if_not_last_owner(tenant_id, user_id, new_role)
            .await
    }

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        (**self).count_for_tenant(tenant_id).await
    }

    async fn save_if_within_quota(
        &self,
        membership: &TenantMembership,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        (**self)
            .save_if_within_quota(membership, tenant_id, limit)
            .await
    }
}
