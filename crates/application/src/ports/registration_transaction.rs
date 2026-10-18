use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::{Tenant, TenantMembership, User};

pub type RegistrationTransactionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// This port represents a single, indivisible (atomic) operation: creating a
/// new User together with their first Tenant and the owning TenantMembership
/// (Owner) — all three at once, or all three fail together (all-or-nothing).
///
/// The application layer does NOT know how this is implemented on Postgres or
/// any other database — it only knows THAT the operation is atomic. Everything
/// to do with `sqlx`/`Transaction` lives solely in the `infra` crate.
#[async_trait]
pub trait RegistrationTransaction: Send + Sync {
    /// Persists the User, Tenant, and TenantMembership as one operation.
    /// If any part fails, nothing is persisted (rollback).
    async fn register_user_with_tenant(
        &self,
        user: &User,
        tenant: &Tenant,
        membership: &TenantMembership,
    ) -> RegistrationTransactionResult<()>;
}

#[async_trait]
impl<T> RegistrationTransaction for Arc<T>
where
    T: RegistrationTransaction + ?Sized,
{
    async fn register_user_with_tenant(
        &self,
        user: &User,
        tenant: &Tenant,
        membership: &TenantMembership,
    ) -> RegistrationTransactionResult<()> {
        (**self)
            .register_user_with_tenant(user, tenant, membership)
            .await
    }
}
