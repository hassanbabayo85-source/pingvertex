//! TASK 4A — regression coverage for the Feature 10 privilege-escalation
//! fix PLUS the new tenant-scoped `max_api_keys` quota.
//!
//! * `Forbidden` must still take priority over validation and quota.
//! * `admin_can_create` / `owner_can_create` now prove the whole path
//!   works end-to-end (permission → validation → resolver → quota → save).
//! * The new quota tests prove the limit is tenant-scoped and enforced.

mod common;
use common::*;

use std::sync::Arc;

use application::{
    commands::CreateApiKey,
    use_cases::{CreateApiKeyError, CreateApiKeyUseCase},
};
use domain::Entitlements;
use domain::enums::ApiKeyScope;
use domain::shared::ids::UserId;
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::{
    Argon2PasswordHasher, InMemoryApiKeyRepository, InMemoryAuditLogEntryRepository,
    InMemoryEntitlementResolver, UuidIdGenerator,
};
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────

fn auth_with_role(role: Role) -> AuthenticatedUser {
    let user_id = UserId(Uuid::new_v4());
    let tenant = tenant_id();
    AuthenticatedUser::new(user_id, role, TenantContext::with_actor(tenant, user_id))
}

fn auth_with_role_in_tenant(role: Role, tenant: domain::TenantId) -> AuthenticatedUser {
    let user_id = UserId(Uuid::new_v4());
    AuthenticatedUser::new(user_id, role, TenantContext::with_actor(tenant, user_id))
}

fn make_use_case() -> CreateApiKeyUseCase {
    CreateApiKeyUseCase::new(
        Arc::new(InMemoryApiKeyRepository::new()),
        Arc::new(UuidIdGenerator::new()),
        Arc::new(Argon2PasswordHasher),
        Arc::new(InMemoryAuditLogEntryRepository::new()),
    )
    .with_entitlement_resolver(Arc::new(InMemoryEntitlementResolver::default()))
}

fn make_use_case_with_resolver(
    resolver: InMemoryEntitlementResolver,
) -> CreateApiKeyUseCase {
    CreateApiKeyUseCase::new(
        Arc::new(InMemoryApiKeyRepository::new()),
        Arc::new(UuidIdGenerator::new()),
        Arc::new(Argon2PasswordHasher),
        Arc::new(InMemoryAuditLogEntryRepository::new()),
    )
    .with_entitlement_resolver(Arc::new(resolver))
}

fn valid_command() -> CreateApiKey {
    CreateApiKey {
        name: "CI integration".to_string(),
        scopes: vec![ApiKeyScope::MonitorWrite, ApiKeyScope::StatusPageWrite],
        expires_at: None,
    }
}

// ── Existing privilege-escalation tests (unchanged assertions) ─────

#[tokio::test]
async fn auditor_cannot_create_an_api_key() {
    let use_case = make_use_case();
    let auditor = auth_with_role(Role::Auditor);

    let result = use_case.execute(valid_command(), auditor).await;

    match result {
        Err(CreateApiKeyError::Forbidden) => {}
        other => panic!("expected Forbidden for an Auditor, got {other:?}"),
    }
}

#[tokio::test]
async fn operator_cannot_create_an_api_key() {
    let use_case = make_use_case();
    let member = auth_with_role(Role::Operator);

    let result = use_case.execute(valid_command(), member).await;

    match result {
        Err(CreateApiKeyError::Forbidden) => {}
        other => panic!("expected Forbidden for an Operator, got {other:?}"),
    }
}

#[tokio::test]
async fn admin_can_create_an_api_key() {
    let use_case = make_use_case();
    let admin = auth_with_role(Role::Admin);

    let result = use_case.execute(valid_command(), admin).await;

    assert!(
        result.is_ok(),
        "expected an Admin to be able to create an api key, got {result:?}"
    );
}

#[tokio::test]
async fn owner_can_create_an_api_key() {
    let use_case = make_use_case();
    let owner = auth_with_role(Role::Owner);

    let result = use_case.execute(valid_command(), owner).await;

    assert!(
        result.is_ok(),
        "expected an Owner to be able to create an api key, got {result:?}"
    );
}

/// Authorization must run before validation, so a forbidden caller gets
/// `Forbidden` (not `EmptyName` / `EmptyScopes`) even when the request
/// body is also invalid.
#[tokio::test]
async fn forbidden_takes_priority_over_validation_errors() {
    let use_case = make_use_case();
    let auditor = auth_with_role(Role::Auditor);

    let invalid_command = CreateApiKey {
        name: String::new(),
        scopes: vec![],
        expires_at: None,
    };

    let result = use_case.execute(invalid_command, auditor).await;

    match result {
        Err(CreateApiKeyError::Forbidden) => {}
        other => panic!("expected Forbidden to take priority, got {other:?}"),
    }
}

// ── TASK 4A: tenant-scoped quota tests ──────────────────────────────

/// Free tier v1 allows exactly one API key per tenant.
#[tokio::test]
async fn free_tier_rejects_the_second_api_key() {
    let use_case = make_use_case();
    let tenant = tenant_id();
    let admin = auth_with_role_in_tenant(Role::Admin, tenant);

    let free_limit = Entitlements::free_v1_fixture().max_api_keys;
    assert!(
        free_limit >= 1,
        "Free tier must allow at least one api key for this test"
    );

    // Create up to (but not past) the Free limit.
    for i in 0..free_limit {
        use_case
            .execute(valid_command(), admin.clone())
            .await
            .unwrap_or_else(|e| panic!("create #{} should succeed: {e:?}", i));
    }

    // Next one must be rejected.
    let result = use_case.execute(valid_command(), admin).await;

    match result {
        Err(CreateApiKeyError::PlanLimitExceeded(pl)) => {
            assert_eq!(pl.quota, "max_api_keys");
            assert_eq!(pl.allowed, u64::from(free_limit));
            assert_eq!(pl.code(), "plan_limit_reached");
        }
        other => panic!("expected PlanLimitExceeded, got {other:?}"),
    }
}

/// Two tenants have independent quotas.
#[tokio::test]
async fn different_tenants_have_independent_api_key_quotas() {
    let use_case = make_use_case();
    let tenant_a = tenant_id();
    let tenant_b = tenant_id();
    let admin_a = auth_with_role_in_tenant(Role::Admin, tenant_a);
    let admin_b = auth_with_role_in_tenant(Role::Admin, tenant_b);

    let free_limit = Entitlements::free_v1_fixture().max_api_keys;

    // Fill tenant A to its limit.
    for _ in 0..free_limit {
        use_case
            .execute(valid_command(), admin_a.clone())
            .await
            .unwrap();
    }

    // Tenant B is unaffected.
    use_case
        .execute(valid_command(), admin_b)
        .await
        .expect("tenant B should not be limited by tenant A's quota");
}

/// Starter tier allows more keys than Free.
#[tokio::test]
async fn starter_plan_allows_more_api_keys_than_free() {
    let starter_entitlements = Entitlements::starter_v1_fixture();
    let free_entitlements = Entitlements::free_v1_fixture();
    assert!(
        starter_entitlements.max_api_keys > free_entitlements.max_api_keys,
        "Starter must allow more api keys than Free for this test"
    );

    let tenant = tenant_id();
    let resolver =
        InMemoryEntitlementResolver::default().with_override(tenant, starter_entitlements);
    let use_case = make_use_case_with_resolver(resolver);
    let admin = auth_with_role_in_tenant(Role::Admin, tenant);

    // Create one MORE than the Free tier would allow.
    for i in 0..=free_entitlements.max_api_keys {
        use_case
            .execute(valid_command(), admin.clone())
            .await
            .unwrap_or_else(|e| panic!("Starter create #{} should succeed: {e:?}", i));
    }
}

/// Missing resolver must fail closed — no key is ever saved.
#[tokio::test]
async fn missing_entitlement_resolver_fails_closed() {
    // Note: no `.with_entitlement_resolver(...)` call.
    let use_case = CreateApiKeyUseCase::new(
        Arc::new(InMemoryApiKeyRepository::new()),
        Arc::new(UuidIdGenerator::new()),
        Arc::new(Argon2PasswordHasher),
        Arc::new(InMemoryAuditLogEntryRepository::new()),
    );
    let admin = auth_with_role(Role::Admin);

    let result = use_case.execute(valid_command(), admin).await;
    match result {
        Err(CreateApiKeyError::EntitlementResolverMissing) => {}
        other => panic!("expected EntitlementResolverMissing, got {other:?}"),
    }
}
