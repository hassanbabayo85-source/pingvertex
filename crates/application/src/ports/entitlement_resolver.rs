//! The single authoritative resolver for a tenant's subscription
//! entitlements.
//!
//! # Authority
//!
//! `EntitlementResolver::resolve(tenant)` is the *only* sanctioned way
//! to answer "what does this tenant's plan allow right now?". Every
//! hard-quota enforcement path in the application MUST go through this
//! trait. Reading `plan_entitlements` directly from a use case is a
//! bug; so is calling `ResolveEntitlementsUseCase` (which is retired).
//!
//! The resolver is implemented by `PostgresEntitlementResolver` in
//! `infra`, with an `InMemoryEntitlementResolver` for tests.
//!
//! # Boundaries
//!
//! The resolver returns a single immutable `Entitlements` snapshot. It
//! does not cache — caching would risk handing a stale snapshot to a
//! race-sensitive quota check. Callers that need a snapshot across
//! several operations should call `resolve` once and reuse the
//! returned value.

use async_trait::async_trait;
use domain::{Entitlements, shared::ids::TenantId};

use super::RepoResult;

#[async_trait]
pub trait EntitlementResolver: Send + Sync {
    /// Resolves `tenant_id`'s effective entitlements for the *current*
    /// moment:
    ///
    ///   * a tenant with no `subscriptions` row is treated as `Free`;
    ///   * a tenant whose subscription is `Expired` or `Canceled` is
    ///     treated as `Free` (their previous entitlements are no
    ///     longer honored);
    ///   * a tenant whose subscription is `Active`, `Trialing`, or
    ///     `PastDue` (in grace) uses the plan's entitlements;
    ///   * the "current" version of `plan_entitlements` for that plan
    ///     is the latest `version` row.
    ///
    /// The returned `Entitlements` must be treated as a hard ceiling:
    /// no operation may be performed that would exceed one of its
    /// numeric limits without returning
    /// `crate::errors::PlanLimitError`.
    async fn resolve(&self, tenant_id: TenantId) -> RepoResult<Entitlements>;
}

#[async_trait]
impl<T> EntitlementResolver for std::sync::Arc<T>
where
    T: EntitlementResolver + ?Sized,
{
    async fn resolve(&self, tenant_id: TenantId) -> RepoResult<Entitlements> {
        (**self).resolve(tenant_id).await
    }
}
