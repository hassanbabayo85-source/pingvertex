//! Smoke tests for `InMemoryEntitlementResolver`.
//!
//! These verify the contract the use-case tests will rely on:
//!   * a tenant with no override resolves to the Free default;
//!   * an explicit override is returned for that tenant only;
//!   * other tenants are unaffected.

use application::ports::EntitlementResolver;
use domain::{Entitlements, PlanTier, shared::ids::TenantId};
use infra::InMemoryEntitlementResolver;

#[tokio::test]
async fn tenant_with_no_override_resolves_to_free_default() {
    let resolver = InMemoryEntitlementResolver::default();
    let ent = resolver.resolve(TenantId::new()).await.unwrap();
    assert_eq!(ent.plan_tier, PlanTier::Free);
    assert_eq!(ent.max_uptime_monitors, 20);
    assert_eq!(ent.min_uptime_interval_seconds, 300);
}

#[tokio::test]
async fn explicit_override_is_returned_for_that_tenant_only() {
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();

    let starter = Entitlements::starter_v1_fixture();

    let resolver = InMemoryEntitlementResolver::default().with_override(tenant_a, starter);

    let a = resolver.resolve(tenant_a).await.unwrap();
    assert_eq!(a.plan_tier, PlanTier::Starter);
    assert_eq!(a.max_uptime_monitors, 30);

    let b = resolver.resolve(tenant_b).await.unwrap();
    assert_eq!(b.plan_tier, PlanTier::Free);
    assert_eq!(b.max_uptime_monitors, 20);
}

#[tokio::test]
async fn remove_override_falls_back_to_free() {
    let tenant = TenantId::new();
    let resolver = InMemoryEntitlementResolver::default();
    let starter = Entitlements::starter_v1_fixture();

    resolver.set_override(tenant, starter);
    assert_eq!(
        resolver.resolve(tenant).await.unwrap().plan_tier,
        PlanTier::Starter
    );

    resolver.remove_override(tenant);
    assert_eq!(
        resolver.resolve(tenant).await.unwrap().plan_tier,
        PlanTier::Free
    );
}
