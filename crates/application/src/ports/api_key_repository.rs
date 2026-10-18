use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{error::Error, sync::Arc};

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};
use crate::ports::monitor_repository::QuotaCheckOutcome;
use domain::{
    entities::ApiKey,
    shared::ids::{ApiKeyId, TenantId},
};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `ApiKey`.
///
/// Tenant-wide (Model A): there is deliberately no `find_by_id_for_user` /
/// `delete_for_user` pair here (unlike `NotificationChannelRepository`,
/// which predates the tenant-wide model). Every method that needs an
/// authorization check is called by a use case that has already verified
/// `AuthorizationPolicy::same_tenant` against the caller's
/// `AuthenticatedUser`, matching `AuditLogEntryRepository` and
/// `TenantMembershipRepository`.
///
/// SECURITY: the application-layer `same_tenant` check is not the only
/// line of defense for deletion — `delete_for_tenant` below re-asserts
/// tenant ownership at the SQL layer, so a future caller that forgets
/// the application-layer check still cannot delete another tenant's key.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn create(&self, api_key: &ApiKey) -> RepoResult<()>;

    /// Fetches a single key by id, with no tenant filtering — callers
    /// (e.g. `DeleteApiKeyUseCase`) authorize the result themselves via
    /// `AuthorizationPolicy::same_tenant`.
    async fn find_by_id(&self, id: ApiKeyId) -> RepoResult<Option<ApiKey>>;

    /// Fetches a single key by its plaintext lookup prefix (see
    /// `ApiKey::key_prefix`). This is the ONLY way `ApiKeyAuthMiddleware`
    /// finds a key from a raw `X-API-Key` header value — Argon2 hashes are
    /// salted and therefore cannot be looked up by equality, so the
    /// middleware fetches by prefix first and then verifies the full raw
    /// key against `hashed_key`.
    async fn find_by_prefix(&self, key_prefix: &str) -> RepoResult<Option<ApiKey>>;

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<ApiKey>>;

    /// Cursor-paginated variant of `list_by_tenant`.
    ///
    /// Default impl paginates in memory (correct for `InMemoryApiKeyRepository`).
    /// `PostgresApiKeyRepository` overrides it with SQL-level `LIMIT` so
    /// the DB never materializes more than `limit + 1` rows.
    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<ApiKey>> {
        let all = self.list_by_tenant(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |k| {
            Cursor::new(k.created_at, k.id.0)
        }))
    }

    /// Deletes a key by id, with no tenant filtering — same authorization
    /// contract as `find_by_id` above. Returns `Ok(false)` if no key with
    /// that id exists.
    ///
    /// Prefer `delete_for_tenant` for anything reachable from an
    /// authenticated HTTP request; this method is kept only for internal/
    /// administrative callers that have no tenant context to scope by.
    async fn delete(&self, id: ApiKeyId) -> RepoResult<bool>;

    /// SECURITY (defense-in-depth): tenant-scoped delete. Deletes the key
    /// only if it belongs to `tenant_id`; returns `Ok(false)` (not an
    /// error) if the id doesn't exist or belongs to a different tenant.
    /// Mirrors `MonitorRepository::delete_for_tenant` — enforces tenant
    /// isolation at the SQL layer itself, rather than depending solely on
    /// the application-layer `AuthorizationPolicy::same_tenant` check
    /// already performed before this is called (see
    /// `DeleteApiKeyUseCase`), so a future caller that forgets that check
    /// still cannot delete another tenant's key.
    async fn delete_for_tenant(&self, id: ApiKeyId, tenant_id: TenantId) -> RepoResult<bool>;

    /// Best-effort bookkeeping update, called by `ApiKeyAuthMiddleware`
    /// after a successful authentication. Given a default no-op
    /// implementation so it can be added without breaking existing
    /// `ApiKeyRepository` implementors; `PostgresApiKeyRepository`
    /// overrides it with a real `UPDATE`.
    #[allow(unused_variables)]
    async fn touch_last_used(&self, id: ApiKeyId, at: DateTime<Utc>) -> RepoResult<()> {
        Ok(())
    }

    /// TASK 4A — tenant-scoped count for quota enforcement.
    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        Ok(self.list_by_tenant(tenant_id).await?.len() as u64)
    }

    /// TASK 4A — atomic count-then-create under `limit`.
    ///
    /// Same non-atomic default caveat as `StatusPageRepository::save_if_within_quota`.
    async fn save_if_within_quota(
        &self,
        api_key: &ApiKey,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let count = self.count_for_tenant(tenant_id).await?;
        if count >= u64::from(limit) {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }
        self.create(api_key).await?;
        Ok(QuotaCheckOutcome::Saved)
    }
}

#[async_trait]
impl<T> ApiKeyRepository for Arc<T>
where
    T: ApiKeyRepository + Send + Sync + ?Sized,
{
    async fn create(&self, api_key: &ApiKey) -> RepoResult<()> {
        (**self).create(api_key).await
    }

    async fn find_by_id(&self, id: ApiKeyId) -> RepoResult<Option<ApiKey>> {
        (**self).find_by_id(id).await
    }

    async fn find_by_prefix(&self, key_prefix: &str) -> RepoResult<Option<ApiKey>> {
        (**self).find_by_prefix(key_prefix).await
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<ApiKey>> {
        (**self).list_by_tenant(tenant_id).await
    }

    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<ApiKey>> {
        (**self).list_by_tenant_paginated(tenant_id, page).await
    }

    async fn delete(&self, id: ApiKeyId) -> RepoResult<bool> {
        (**self).delete(id).await
    }

    async fn delete_for_tenant(&self, id: ApiKeyId, tenant_id: TenantId) -> RepoResult<bool> {
        (**self).delete_for_tenant(id, tenant_id).await
    }

    async fn touch_last_used(&self, id: ApiKeyId, at: DateTime<Utc>) -> RepoResult<()> {
        (**self).touch_last_used(id, at).await
    }

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        (**self).count_for_tenant(tenant_id).await
    }

    async fn save_if_within_quota(
        &self,
        api_key: &ApiKey,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        (**self)
            .save_if_within_quota(api_key, tenant_id, limit)
            .await
    }
}
