use std::error::Error;

use application::ports::RegistrationTransaction;
use async_trait::async_trait;
use domain::{Tenant, TenantMembership, User};

/// Test-only in-memory stand-in for `RegistrationTransaction`.
///
/// Unlike `PostgresRegistrationTransaction`, this does not persist
/// anything and does not enforce atomicity — it exists purely to satisfy
/// `AuthState::new`'s generic bound in HTTP-layer tests that assert on
/// response status codes, not on repository state. Tests that need to
/// verify actual registration persistence should exercise
/// `PostgresRegistrationTransaction` directly instead.
#[derive(Debug, Clone, Default)]
pub struct InMemoryRegistrationTransaction;

impl InMemoryRegistrationTransaction {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RegistrationTransaction for InMemoryRegistrationTransaction {
    async fn register_user_with_tenant(
        &self,
        _user: &User,
        _tenant: &Tenant,
        _membership: &TenantMembership,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}
