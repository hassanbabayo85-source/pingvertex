//! Environment-driven `EntitlementResolver` for self-hostable
//! deployments. Parses `PV_*` env vars once at construction time and
//! returns that snapshot from every `resolve` call. Phase 1L-a-2.

use application::ports::{EntitlementResolver, RepoResult};
use async_trait::async_trait;
use domain::Entitlements;
use domain::entitlements_env::EnvConfigError;
use domain::shared::ids::TenantId;

/// A resolver that holds a single `Entitlements` snapshot parsed from
/// environment variables at construction time.
pub struct EnvEntitlementResolver {
    entitlements: Entitlements,
}

impl EnvEntitlementResolver {
    /// Parses `PV_*` env vars once. Returns `EnvConfigError` if any
    /// variable is invalid, so `main.rs` can fail fast on startup.
    pub fn from_env() -> Result<Self, EnvConfigError> {
        Ok(Self {
            entitlements: Entitlements::from_env()?,
        })
    }
}

#[async_trait]
impl EntitlementResolver for EnvEntitlementResolver {
    async fn resolve(&self, _tenant_id: TenantId) -> RepoResult<Entitlements> {
        Ok(self.entitlements)
    }
}
