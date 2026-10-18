use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};
use crate::ports::monitor_repository::QuotaCheckOutcome;
use domain::{
    entities::StatusPage,
    shared::ids::{MonitorId, StatusPageId, TenantId, UserId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait StatusPageRepository: Send + Sync {
    async fn save(&self, status_page: &StatusPage) -> RepoResult<()>;

    async fn update(&self, status_page: &StatusPage) -> RepoResult<()>;

    async fn delete_for_user(&self, id: StatusPageId, user_id: UserId) -> RepoResult<bool>;

    async fn find_by_id_for_user(
        &self,
        id: StatusPageId,
        user_id: UserId,
    ) -> RepoResult<Option<StatusPage>>;

    async fn find_all_for_user(&self, user_id: UserId) -> RepoResult<Vec<StatusPage>>;

    /// Fetches a status page by id with NO ownership filtering. Callers
    /// that need to authorize the result themselves (e.g. via
    /// `domain::shared::policies::AuthorizationPolicy::same_tenant`)
    /// should use this instead of `find_by_id_for_user`, which only
    /// recognizes the single-owner relationship and not shared tenant
    /// membership.
    async fn find_by_id(&self, _id: StatusPageId) -> RepoResult<Option<StatusPage>> {
        unimplemented!()
    }

    /// Deletes a status page by id with NO ownership filtering. Callers
    /// must authorize the deletion themselves before calling this (see
    /// `find_by_id` above).
    async fn delete(&self, _id: StatusPageId) -> RepoResult<bool> {
        unimplemented!()
    }

    /// SECURITY (P0): tenant-scoped delete — deletes only if the
    /// status page belongs to `tenant_id`. Prefer this over `delete`
    /// for anything reachable from an authenticated HTTP request.
    async fn delete_for_tenant(&self, _id: StatusPageId, _tenant_id: TenantId) -> RepoResult<bool> {
        unimplemented!()
    }

    /// Tenant-scoped equivalent of `find_all_for_user`. Returns every
    /// status page belonging to `tenant_id`, regardless of which member
    /// of the tenant created it.
    async fn find_all_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<StatusPage>> {
        unimplemented!()
    }

    /// Cursor-paginated variant of `find_all_for_tenant`.
    async fn find_all_for_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<StatusPage>> {
        let all = self.find_all_for_tenant(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |sp| {
            Cursor::new(sp.created_at, sp.id.0)
        }))
    }

    /// Looks up a status page by its `pingvertex.com` subdomain, e.g.
    /// `"acme"` for `acme.pingvertex.com`. Used by the unauthenticated
    /// public status-page endpoint.
    async fn find_by_subdomain(&self, subdomain: &str) -> RepoResult<Option<StatusPage>> {
        let _ = subdomain;
        unimplemented!()
    }

    /// Looks up a status page by a VERIFIED custom domain, e.g.
    /// `"status.acme.com"`.
    ///
    /// SECURITY: implementations MUST only match against domains whose
    /// verification state is `Verified` (see
    /// `domain::entities::CustomDomainVerification`) — a `Pending` domain
    /// must never be returned here, or host-header routing could be
    /// hijacked by anyone who merely *requested* (without proving
    /// ownership of) someone else's domain. This is exactly the strict
    /// allowlist the task's SECURITY REQUIREMENTS call for.
    async fn find_by_verified_custom_domain(
        &self,
        custom_domain: &str,
    ) -> RepoResult<Option<StatusPage>> {
        let _ = custom_domain;
        unimplemented!()
    }

    /// Returns every status page whose `custom_domain` is currently
    /// `Pending` verification. Used by the background
    /// `CustomDomainVerificationWorker` sweep — it has no per-user scope
    /// because the worker runs system-wide, not on behalf of any single
    /// caller.
    async fn find_pending_custom_domains(&self) -> RepoResult<Vec<StatusPage>> {
        unimplemented!()
    }

    /// Returns every status page (across all tenants) whose
    /// `monitor_ids` includes `monitor_id`. System-wide, not tenant-scoped
    /// — same reasoning as `find_pending_custom_domains`: this is used by
    /// `IncidentManager` (worker) to know which cached public status-page
    /// responses to invalidate whenever that monitor's incident opens or
    /// resolves, and the worker acts independently of any single caller.
    async fn find_all_referencing_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> RepoResult<Vec<StatusPage>> {
        unimplemented!()
    }

    /// TASK 4A — tenant-scoped count for quota enforcement.
    ///
    /// Default implementation is non-atomic (used only by test doubles).
    /// Production `PostgresStatusPageRepository` overrides this with a
    /// single indexed SQL `COUNT(*)`.
    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        Ok(self.find_all_for_tenant(tenant_id).await?.len() as u64)
    }

    /// TASK 4A — atomic count-then-save under `limit`.
    ///
    /// Default impl is **non-atomic** and is intended ONLY for test
    /// doubles. Production `PostgresStatusPageRepository` overrides it
    /// with a `pg_advisory_xact_lock`-guarded transaction (mirroring
    /// `PostgresMonitorRepository::save_if_within_quota`).
    ///
    /// A non-atomic default is *not* a security regression: it is no
    /// weaker than the previous behaviour, in which the use case counted
    /// and saved with no lock at all. Tests that need to prove
    /// atomicity use the Postgres impl.
    async fn save_if_within_quota(
        &self,
        status_page: &StatusPage,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let count = self.count_for_tenant(tenant_id).await?;
        if count >= u64::from(limit) {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }
        self.save(status_page).await?;
        Ok(QuotaCheckOutcome::Saved)
    }
}

#[async_trait]
impl<T> StatusPageRepository for Arc<T>
where
    T: StatusPageRepository + Send + Sync + ?Sized,
{
    async fn save(&self, status_page: &StatusPage) -> RepoResult<()> {
        (**self).save(status_page).await
    }

    async fn update(&self, status_page: &StatusPage) -> RepoResult<()> {
        (**self).update(status_page).await
    }

    async fn delete_for_user(&self, id: StatusPageId, user_id: UserId) -> RepoResult<bool> {
        (**self).delete_for_user(id, user_id).await
    }

    async fn find_by_id_for_user(
        &self,
        id: StatusPageId,
        user_id: UserId,
    ) -> RepoResult<Option<StatusPage>> {
        (**self).find_by_id_for_user(id, user_id).await
    }

    async fn find_all_for_user(&self, user_id: UserId) -> RepoResult<Vec<StatusPage>> {
        (**self).find_all_for_user(user_id).await
    }

    async fn find_by_id(&self, id: StatusPageId) -> RepoResult<Option<StatusPage>> {
        (**self).find_by_id(id).await
    }

    async fn delete(&self, id: StatusPageId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn delete_for_tenant(&self, id: StatusPageId, tenant_id: TenantId) -> RepoResult<bool> {
        (**self).delete_for_tenant(id, tenant_id).await
    }

    async fn find_all_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<StatusPage>> {
        (**self).find_all_for_tenant(tenant_id).await
    }

    async fn find_all_for_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<StatusPage>> {
        (**self)
            .find_all_for_tenant_paginated(tenant_id, page)
            .await
    }

    async fn find_by_subdomain(&self, subdomain: &str) -> RepoResult<Option<StatusPage>> {
        (**self).find_by_subdomain(subdomain).await
    }

    async fn find_by_verified_custom_domain(
        &self,
        custom_domain: &str,
    ) -> RepoResult<Option<StatusPage>> {
        (**self).find_by_verified_custom_domain(custom_domain).await
    }

    async fn find_pending_custom_domains(&self) -> RepoResult<Vec<StatusPage>> {
        (**self).find_pending_custom_domains().await
    }

    async fn find_all_referencing_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> RepoResult<Vec<StatusPage>> {
        (**self).find_all_referencing_monitor(monitor_id).await
    }

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        (**self).count_for_tenant(tenant_id).await
    }

    async fn save_if_within_quota(
        &self,
        status_page: &StatusPage,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        (**self)
            .save_if_within_quota(status_page, tenant_id, limit)
            .await
    }
}
