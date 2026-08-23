use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::{Subscription, shared::ids::TenantId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Persistence port for `Subscription`.
///
/// B11 limited this to create and lookup-by-tenant. B12 adds `update`
/// for lifecycle transitions (trial, grace period, expiry, cancellation)
/// — there is still no `delete`, since a subscription's terminal states
/// (`Expired`, `Canceled`) are represented as data, not row removal.
#[async_trait]
pub trait SubscriptionRepository: Send + Sync {
    async fn create(&self, subscription: &Subscription) -> RepoResult<()>;

    async fn find_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Option<Subscription>>;

    async fn update(&self, subscription: &Subscription) -> RepoResult<()>;
}

#[async_trait]
impl<T> SubscriptionRepository for Arc<T>
where
    T: SubscriptionRepository + Send + Sync + ?Sized,
{
    async fn create(&self, subscription: &Subscription) -> RepoResult<()> {
        (**self).create(subscription).await
    }

    async fn find_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Option<Subscription>> {
        (**self).find_by_tenant_id(tenant_id).await
    }

    async fn update(&self, subscription: &Subscription) -> RepoResult<()> {
        (**self).update(subscription).await
    }
}
