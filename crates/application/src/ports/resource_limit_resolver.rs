//! The single authoritative resolver for a tenant's resource limits.
//!
//! # Authority
//!
//! `ResourceLimitResolver::resolve(tenant)` is the *only* sanctioned way
//! to answer "what does this tenant's plan allow right now?". Every
//! hard-quota enforcement path in the application MUST go through this
//! trait.
//!
//! Two implementations exist in `infra`:
//!
//!   * `EnvResourceLimitResolver` — for self-hostable deployments.
//!     Parses `PV_*` environment variables once at construction and
//!     returns the same snapshot for every tenant. Wired by both
//!     `api::main` and `worker::main`.
//!   * `InMemoryResourceLimitResolver` — for tests. Defaults every
//!     tenant to a `free_default` snapshot and supports per-tenant
//!     overrides.
//!
//! # Boundaries
//!
//! The resolver returns a single immutable `ResourceLimits` snapshot. It
//! does not cache — caching would risk handing a stale snapshot to a
//! race-sensitive quota check. Callers that need a snapshot across
//! several operations should call `resolve` once and reuse the
//! returned value.

use async_trait::async_trait;
use domain::{ResourceLimits, shared::ids::TenantId};

use super::RepoResult;

#[async_trait]
pub trait ResourceLimitResolver: Send + Sync {
    /// Resolves `tenant_id`'s effective resource limits for the current
    /// moment.
    ///
    /// The returned `ResourceLimits` must be treated as a hard ceiling:
    /// no operation may be performed that would exceed one of its
    /// numeric limits without returning
    /// `crate::errors::ResourceLimitError`.
    async fn resolve(&self, tenant_id: TenantId) -> RepoResult<ResourceLimits>;
}

#[async_trait]
impl<T> ResourceLimitResolver for std::sync::Arc<T>
where
    T: ResourceLimitResolver + ?Sized,
{
    async fn resolve(&self, tenant_id: TenantId) -> RepoResult<ResourceLimits> {
        (**self).resolve(tenant_id).await
    }
}
