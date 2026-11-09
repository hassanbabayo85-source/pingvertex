//! In-memory `ResourceLimitResolver` for tests.
//!
//! Mirrors `PostgresEntitlementResolver`'s fallback behavior: a tenant
//! with no override resolves to `free_default`. Tests seed specific
//! tenants via `set_override` / `with_override`, and rely on the
//! default only when they want to simulate "no subscription row".

use std::collections::HashMap;
use std::sync::RwLock;

use application::ports::{RepoResult, ResourceLimitResolver};
use async_trait::async_trait;
use domain::{ResourceLimits, shared::ids::TenantId};

#[derive(Debug)]
pub struct InMemoryResourceLimitResolver {
    free_default: ResourceLimits,
    overrides: RwLock<HashMap<TenantId, ResourceLimits>>,
}

impl InMemoryResourceLimitResolver {
    /// Builds a resolver whose default (for any tenant without an
    /// override) is `free_default`. Tests should pass
    /// `ResourceLimits::free_v1_fixture()`; production never constructs
    /// this type.
    /// Task 8A — test-only constructor that returns Enterprise for every
    /// tenant. Existing quota tests keep using `default()` (Free), so this
    /// method is purely additive.
    pub fn default_enterprise() -> Self {
        Self::new(ResourceLimits::enterprise_v1_fixture())
    }

    pub fn new(free_default: ResourceLimits) -> Self {
        Self {
            free_default,
            overrides: RwLock::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn with_override(self, tenant_id: TenantId, ent: ResourceLimits) -> Self {
        self.set_override(tenant_id, ent);
        self
    }

    pub fn set_override(&self, tenant_id: TenantId, ent: ResourceLimits) {
        self.overrides.write().unwrap().insert(tenant_id, ent);
    }

    pub fn remove_override(&self, tenant_id: TenantId) -> Option<ResourceLimits> {
        self.overrides.write().unwrap().remove(&tenant_id)
    }
}

impl Default for InMemoryResourceLimitResolver {
    fn default() -> Self {
        Self::new(ResourceLimits::free_v1_fixture())
    }
}

#[async_trait]
impl ResourceLimitResolver for InMemoryResourceLimitResolver {
    async fn resolve(&self, tenant_id: TenantId) -> RepoResult<ResourceLimits> {
        if let Some(ent) = self.overrides.read().unwrap().get(&tenant_id) {
            return Ok(*ent);
        }
        Ok(self.free_default)
    }
}
