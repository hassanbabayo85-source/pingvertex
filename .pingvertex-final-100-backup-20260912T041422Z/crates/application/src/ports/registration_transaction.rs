use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::{Tenant, TenantMembership, User};

pub type RegistrationTransactionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Wannan port yana wakiltar wani aiki guda ɗaya, mai kama (atomic) da ba za a
/// iya raba shi ba: ƙirƙirar sabon User tare da Tenant ɗinsa na farko da
/// TenantMembership ɗin mallaka (Owner) — duk uku a lokaci guda, ko duk su
/// gaza tare (all-or-nothing).
///
/// Application layer BAI SANI ba yadda ake aiwatar da wannan a Postgres ko
/// wani database — kawai ya san CEWA aikin atomic ne. Duk wani abu na
/// `sqlx`/`Transaction` yana zaune a cikin `infra` crate kaɗai.
#[async_trait]
pub trait RegistrationTransaction: Send + Sync {
    /// Ajiye User, Tenant, da TenantMembership a matsayin aiki guda ɗaya.
    /// Idan wani sashi ya gaza, kada a ajiye komai (rollback).
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
