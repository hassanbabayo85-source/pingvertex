mod common;
use common::*;

use std::sync::Arc;

use application::dto::CreateStatusPageRequest;
use application::ports::MonitorRepository;
use application::use_cases::{CreateStatusPageError, CreateStatusPageUseCase};
use domain::shared::ids::UserId;
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, Monitor, MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::{
    InMemoryEntitlementResolver, InMemoryMonitorRepository, InMemoryStatusPageRepository,
    UuidIdGenerator,
};
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────

fn request(_user_id: UserId, subdomain: &str) -> CreateStatusPageRequest {
    CreateStatusPageRequest {
        name: "Acme Status".into(),
        subdomain: subdomain.into(),
        monitor_ids: vec![],
    }
}

/// Builder for the default resolver (Free tier) — the common case for
/// tests that only check the wiring.
fn free_resolver() -> Arc<InMemoryEntitlementResolver> {
    Arc::new(InMemoryEntitlementResolver::default())
}

fn use_case(
    status_pages: InMemoryStatusPageRepository,
    monitors: InMemoryMonitorRepository,
) -> CreateStatusPageUseCase<InMemoryStatusPageRepository, UuidIdGenerator, InMemoryMonitorRepository>
{
    use_case_with_resolver(status_pages, monitors, free_resolver())
}

fn use_case_with_resolver(
    status_pages: InMemoryStatusPageRepository,
    monitors: InMemoryMonitorRepository,
    resolver: Arc<InMemoryEntitlementResolver>,
) -> CreateStatusPageUseCase<InMemoryStatusPageRepository, UuidIdGenerator, InMemoryMonitorRepository>
{
    CreateStatusPageUseCase::new(
        status_pages,
        UuidIdGenerator::new(),
        monitors,
        new_audit_repo(),
    )
    .with_entitlement_resolver(resolver)
}

fn sample_monitor(user_id: UserId, tenant_id: domain::TenantId) -> Monitor {
    MonitorBuilder::new(domain::MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("Acme API").unwrap())
        .url(Url::new("https://api.acme.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build()
}

// ── Existing tests (updated only for the resolver wiring) ───────────

#[tokio::test]
async fn create_status_page_persists_status_page() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());

    let status_page = use_case
        .execute(request(user_id, "acme"), auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    assert_eq!(status_page.name, "Acme Status");
    assert_eq!(status_page.subdomain.as_str(), "acme");
}

#[tokio::test]
async fn rejects_duplicate_subdomain() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());

    use_case
        .execute(request(user_id, "acme"), auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    let result = use_case
        .execute(
            request(other_user_id, "acme"),
            auth_user(other_user_id, tenant_id()),
        )
        .await;
    assert!(matches!(
        result,
        Err(CreateStatusPageError::SubdomainTaken { .. })
    ));
}

// ── Quota test — updated: Free tier v1 gives max_status_pages = 1 ───

#[tokio::test]
async fn rejects_creation_past_free_tier_quota() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    // Read the limit from the same source the resolver uses — do NOT
    // hardcode it, so this test stays valid if the v1 seed changes.
    let limit = domain::Entitlements::free_v1_fixture().max_status_pages;
    assert!(
        limit >= 1,
        "Free tier must allow at least 1 status page for this test to be meaningful"
    );

    for i in 0..limit {
        use_case
            .execute(
                request(user_id, &format!("acme-{i}")),
                auth_user(user_id, tid),
            )
            .await
            .unwrap_or_else(|e| panic!("create #{} should succeed: {e:?}", i));
    }

    let result = use_case
        .execute(request(user_id, "one-too-many"), auth_user(user_id, tid))
        .await;

    match result {
        Err(CreateStatusPageError::PlanLimitExceeded(pl)) => {
            assert_eq!(pl.quota, "max_status_pages");
            assert_eq!(pl.allowed, u64::from(limit));
            assert_eq!(pl.code(), "plan_limit_reached");
        }
        other => panic!("expected PlanLimitExceeded, got {other:?}"),
    }
}

// ── NEW: proves per-tenant quota (not per-user) ─────────────────────

#[tokio::test]
async fn same_tenant_different_users_share_the_quota() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let user_a = UserId(Uuid::new_v4());
    let user_b = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let limit = domain::Entitlements::free_v1_fixture().max_status_pages;
    let use_case = use_case(repository.clone(), monitors);

    // User A creates up to (but not past) the tenant's limit.
    for i in 0..limit {
        use_case
            .execute(request(user_a, &format!("a-{i}")), auth_user(user_a, tid))
            .await
            .unwrap_or_else(|e| panic!("A create #{} should succeed: {e:?}", i));
    }

    // User B in the SAME tenant is now blocked — proving the count is
    // tenant-scoped, not per-user.
    let result = use_case
        .execute(request(user_b, "b-0"), auth_user(user_b, tid))
        .await;
    match result {
        Err(CreateStatusPageError::PlanLimitExceeded(pl)) => {
            assert_eq!(pl.quota, "max_status_pages");
        }
        other => panic!("expected PlanLimitExceeded for user B, got {other:?}"),
    }
}

// ── NEW: proves per-tenant isolation (two tenants independent) ──────

#[tokio::test]
async fn different_tenants_have_independent_quotas() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let user_a = UserId(Uuid::new_v4());
    let user_b = UserId(Uuid::new_v4());
    let tenant_a = tenant_id();
    let tenant_b = tenant_id();

    let limit = domain::Entitlements::free_v1_fixture().max_status_pages;
    let use_case = use_case(repository.clone(), monitors);

    // Fill tenant A to the limit.
    for i in 0..limit {
        use_case
            .execute(
                request(user_a, &format!("a-{i}")),
                auth_user(user_a, tenant_a),
            )
            .await
            .unwrap();
    }

    // Tenant B still has its own independent quota — first create succeeds.
    use_case
        .execute(request(user_b, "b-0"), auth_user(user_b, tenant_b))
        .await
        .expect("tenant B should not be limited by tenant A's quota");
}

// ── NEW: proves plan-dependent quota (Starter allows more than Free) ─

#[tokio::test]
async fn starter_plan_allows_its_own_higher_quota() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let user_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let starter = domain::Entitlements::starter_v1_fixture();
    let limit = starter.max_status_pages;
    let free_limit = domain::Entitlements::free_v1_fixture().max_status_pages;
    assert!(
        limit > free_limit,
        "test only meaningful if Starter is higher than Free"
    );

    let resolver =
        Arc::new(InMemoryEntitlementResolver::default().with_override(tid, starter));
    let use_case = use_case_with_resolver(repository.clone(), monitors, resolver);

    // Create Free+1 pages; only a Starter tenant can go past the Free limit.
    for i in 0..=free_limit {
        use_case
            .execute(
                request(user_id, &format!("s-{i}")),
                auth_user(user_id, tid),
            )
            .await
            .unwrap_or_else(|e| panic!("Starter create #{} should succeed: {e:?}", i));
    }

    // Fill the rest of the Starter limit, then verify the next one fails.
    for i in (free_limit + 1)..limit {
        use_case
            .execute(
                request(user_id, &format!("s-{i}")),
                auth_user(user_id, tid),
            )
            .await
            .unwrap_or_else(|e| panic!("Starter create #{} should succeed: {e:?}", i));
    }

    let result = use_case
        .execute(request(user_id, "s-too-many"), auth_user(user_id, tid))
        .await;
    assert!(matches!(
        result,
        Err(CreateStatusPageError::PlanLimitExceeded(_))
    ));
}

// ── NEW: missing resolver fails closed ──────────────────────────────

#[tokio::test]
async fn missing_entitlement_resolver_fails_closed() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    // No `.with_entitlement_resolver(...)` call.
    let use_case =
        CreateStatusPageUseCase::new(repository, UuidIdGenerator::new(), monitors, new_audit_repo());
    let user_id = UserId(Uuid::new_v4());

    let result = use_case
        .execute(request(user_id, "acme"), auth_user(user_id, tenant_id()))
        .await;
    assert!(matches!(
        result,
        Err(CreateStatusPageError::EntitlementResolverMissing)
    ));
}

// ── Existing monitor-ownership tests (unchanged) ────────────────────

#[tokio::test]
async fn allows_monitor_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let owner_id = UserId(Uuid::new_v4());
    let tid = tenant_id();
    let own_monitor = sample_monitor(owner_id, tid);
    monitors.save(&own_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);
    let status_page = use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Status".into(),
                subdomain: "acme".into(),
                monitor_ids: vec![own_monitor.id],
            },
            auth_user(owner_id, tid),
        )
        .await
        .unwrap();

    assert_eq!(status_page.monitor_ids, vec![own_monitor.id]);
}

#[tokio::test]
async fn allows_monitor_owned_by_another_tenant_member() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let owner_id = UserId(Uuid::new_v4());
    let tenant_mate_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let victim_monitor = sample_monitor(owner_id, tid);
    monitors.save(&victim_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);
    let result = use_case
        .execute(
            CreateStatusPageRequest {
                name: "Tenant Mate Status".into(),
                subdomain: "tenant-mate".into(),
                monitor_ids: vec![victim_monitor.id],
            },
            auth_user(tenant_mate_id, tid),
        )
        .await;

    assert!(result.is_ok());
    let status_page = result.unwrap();
    assert_eq!(status_page.monitor_ids, vec![victim_monitor.id]);
}

#[tokio::test]
async fn rejects_monitor_from_another_tenant() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let owner_id = UserId(Uuid::new_v4());
    let other_tenant_id = tenant_id();
    let attacker_id = UserId(Uuid::new_v4());
    let attacker_tenant_id = tenant_id();

    let victim_monitor = sample_monitor(owner_id, other_tenant_id);
    monitors.save(&victim_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);
    let result = use_case
        .execute(
            CreateStatusPageRequest {
                name: "Cross Tenant Status".into(),
                subdomain: "cross-tenant".into(),
                monitor_ids: vec![victim_monitor.id],
            },
            auth_user(attacker_id, attacker_tenant_id),
        )
        .await;

    assert!(matches!(
        result,
        Err(CreateStatusPageError::MonitorNotFound { .. })
    ));
}
