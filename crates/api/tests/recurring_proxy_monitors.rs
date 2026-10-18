//! TASK 3 — Block 6a: Critical handler tests for recurring proxy monitors.
//!
//! These tests call the handler functions directly (bypassing the JWT
//! middleware) so they can focus on the handler contract:
//!
//!   * the JSON response never contains proxy credentials;
//!   * a plan-limit failure maps to `ApiError::PlanLimitReached` (409);
//!   * cross-tenant reads return 404, not 403 (no existence leak);
//!   * the list endpoint is tenant-scoped;
//!   * the update endpoint enforces the 300s interval floor;
//!   * a role without `DeleteMonitor` cannot delete.
//!
//! The route wiring itself (JWT middleware, route matching) is exercised
//! by the compilation of `routes/recurring_proxy_monitors.rs` and
//! `router.rs`, and by the existing JWT middleware tests. These tests
//! verify the layer directly above the repository.

mod common;

use std::sync::Arc;

use api::error::ApiError;
use api::handlers::recurring_proxy_monitors::{
    create_recurring_proxy_monitor, delete_recurring_proxy_monitor,
    get_recurring_proxy_monitor, list_recurring_proxy_monitors,
    update_recurring_proxy_monitor,
};
use api::state::AppState;
use application::dto::{
    CreateRecurringProxyMonitorRequest, UpdateRecurringProxyMonitorRequest,
};
use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use common::*;
use domain::Entitlements;
use domain::PlanTier;
use domain::enums::ProxyScheme;
use domain::pricing::PricingConfig;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::repositories::in_memory_pricing_config_repository::InMemoryPricingConfigRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository,
    InMemoryEntitlementResolver, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    InMemoryRecurringProxyMonitorRepository, UuidIdGenerator,
};

// ── Helpers ─────────────────────────────────────────────────────────

/// Pro-tier entitlements matching migration 0125's seed.
/// `max_recurring_proxy_monitors = 5`, `min_interval = 300s`.
fn pro_entitlements() -> Entitlements {
    Entitlements {
        plan_tier: PlanTier::Pro,
        entitlement_version: 1,
        max_uptime_monitors: 80,
        min_uptime_interval_seconds: 60,
        max_regions_per_monitor: 3,
        max_heartbeat_monitors: 30,
        max_api_flow_monitors: 10,
        max_api_flow_steps: 10,
        max_proxy_entries_per_job: 2_000,
        max_concurrent_proxy_jobs: 2,
        max_recurring_proxy_monitors: 5,
        min_recurring_proxy_interval_seconds: 300,
        max_recurring_proxy_executions_per_month: 43_200,
        max_rum_sites: 3,
        max_rum_events_per_month: 100_000,
    }
}

/// Builds an `AppState` that uses the caller-supplied recurring-proxy
/// repository and entitlement resolver. All other sub-states are the
/// no-op defaults used by `common::build()`.
fn build_state_with(
    repo: Arc<InMemoryRecurringProxyMonitorRepository>,
    resolver: InMemoryEntitlementResolver,
) -> AppState {
    let monitor_repository = InMemoryMonitorRepository::new();
    let pricing_repo = Arc::new(
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config()),
    );

    AppState::new(
        monitor_repository.clone(),
        NoopCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repository),
        infra::NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_pricing_config_repository(pricing_repo)
    .with_recurring_proxy_monitor_repository(repo)
    .with_entitlement_resolver(resolver)
}

fn owner_of(tenant: TenantId) -> AuthenticatedUser {
    AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant))
}

fn viewer_of(tenant: TenantId) -> AuthenticatedUser {
    AuthenticatedUser::new(UserId::new(), Role::Viewer, TenantContext::new(tenant))
}

fn request(name: &str) -> CreateRecurringProxyMonitorRequest {
    CreateRecurringProxyMonitorRequest {
        name: name.into(),
        host: "8.8.8.8".into(),
        port: 8080,
        scheme: ProxyScheme::Http,
        username: None,
        password: None,
        interval_seconds: 300,
    }
}

fn update_cmd(interval_seconds: u32) -> UpdateRecurringProxyMonitorRequest {
    UpdateRecurringProxyMonitorRequest {
        name: "renamed".into(),
        interval_seconds,
    }
}

// ── Test 1: Security — credentials never leak in the JSON response ──

#[tokio::test]
async fn create_returns_201_with_no_credentials_in_response() {
    let tenant = TenantId::new();
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let resolver =
        InMemoryEntitlementResolver::default().with_override(tenant, pro_entitlements());
    let state = build_state_with(repo.clone(), resolver);
    let user = owner_of(tenant);

    let mut req = request("with-credentials");
    req.username = Some("proxy-user".into());
    req.password = Some("very-secret-password-xyz".into());

    let (status, Json(response)) =
        create_recurring_proxy_monitor(State(state), Extension(user), Json(req))
            .await
            .expect("expected 201 CREATED");

    assert_eq!(status, StatusCode::CREATED);
    assert!(
        response.has_credentials,
        "monitor should report it has credentials"
    );

    // CRITICAL SECURITY ASSERTION:
    let json = serde_json::to_string(&response).unwrap();
    assert!(
        !json.contains("very-secret-password-xyz"),
        "SECURITY VIOLATION — password leaked into JSON response: {json}"
    );
    assert!(
        !json.contains("proxy-user"),
        "SECURITY VIOLATION — username leaked into JSON response: {json}"
    );
    assert!(
        json.contains("\"has_credentials\":true"),
        "response should advertise has_credentials=true"
    );
}

// ── Test 2: Quota — Free tier returns 409 plan_limit_reached ────────

#[tokio::test]
async fn create_on_free_tier_returns_409_plan_limit_reached() {
    let tenant = TenantId::new();
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    // No override → InMemoryEntitlementResolver::default() returns the
    // Free v1 fixture, whose `max_recurring_proxy_monitors` is 0.
    let state = build_state_with(repo, InMemoryEntitlementResolver::default());
    let user = owner_of(tenant);

    let err = create_recurring_proxy_monitor(
        State(state),
        Extension(user),
        Json(request("free-tier")),
    )
    .await
    .unwrap_err();

    match err {
        ApiError::PlanLimitReached { quota, allowed, .. } => {
            assert_eq!(quota, "max_recurring_proxy_monitors");
            assert_eq!(allowed, 0, "Free tier allows 0 recurring monitors");
        }
        other => panic!("expected ApiError::PlanLimitReached, got {other:?}"),
    }
}

// ── Test 3: Isolation — cross-tenant GET returns 404 ───────────────

#[tokio::test]
async fn cross_tenant_get_returns_404_not_403() {
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();

    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let resolver = InMemoryEntitlementResolver::default()
        .with_override(tenant_a, pro_entitlements())
        .with_override(tenant_b, pro_entitlements());
    let state = build_state_with(repo.clone(), resolver);

    // Create a monitor owned by tenant A.
    let (_, Json(created)) = create_recurring_proxy_monitor(
        State(state.clone()),
        Extension(owner_of(tenant_a)),
        Json(request("a-owned")),
    )
    .await
    .expect("tenant A should be able to create");

    // Tenant B tries to GET it by id.
    let err = get_recurring_proxy_monitor(
        State(state),
        Extension(owner_of(tenant_b)),
        Path(created.id.to_string()),
    )
    .await
    .unwrap_err();

    // Must be NotFound, not Forbidden: leaking existence would let an
    // attacker enumerate other tenants' monitor ids.
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected NotFound (no existence leak), got {err:?}"
    );
}

// ── Test 4: Isolation — list is tenant-scoped ─────────────────────

#[tokio::test]
async fn list_returns_only_own_monitors() {
    let tenant_a = TenantId::new();
    let tenant_b = TenantId::new();

    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let resolver = InMemoryEntitlementResolver::default()
        .with_override(tenant_a, pro_entitlements())
        .with_override(tenant_b, pro_entitlements());
    let state = build_state_with(repo.clone(), resolver);

    // Tenant A creates 2 monitors.
    for name in ["a-1", "a-2"] {
        let _ = create_recurring_proxy_monitor(
            State(state.clone()),
            Extension(owner_of(tenant_a)),
            Json(request(name)),
        )
        .await
        .expect("tenant A create should succeed");
    }

    // Tenant B creates 1 monitor.
    let _ = create_recurring_proxy_monitor(
        State(state.clone()),
        Extension(owner_of(tenant_b)),
        Json(request("b-1")),
    )
    .await
    .expect("tenant B create should succeed");

    // List for A → exactly 2, names start with "a-".
    let Json(list_a) = list_recurring_proxy_monitors(
        State(state.clone()),
        Extension(owner_of(tenant_a)),
    )
    .await
    .unwrap();
    assert_eq!(list_a.len(), 2);
    assert!(list_a.iter().all(|m| m.name.starts_with("a-")));

    // List for B → exactly 1.
    let Json(list_b) = list_recurring_proxy_monitors(
        State(state),
        Extension(owner_of(tenant_b)),
    )
    .await
    .unwrap();
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].name, "b-1");
}

// ── Test 5: Plan floor — update below 300s is rejected ─────────────

#[tokio::test]
async fn update_below_300s_returns_409_plan_limit_reached() {
    let tenant = TenantId::new();
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let resolver =
        InMemoryEntitlementResolver::default().with_override(tenant, pro_entitlements());
    let state = build_state_with(repo, resolver);
    let user = owner_of(tenant);

    // Create at the legal 300s floor.
    let (_, Json(created)) = create_recurring_proxy_monitor(
        State(state.clone()),
        Extension(user.clone()),
        Json(request("interval-test")),
    )
    .await
    .expect("create at 300s should succeed");

    // Try to update to 299s.
    let err = update_recurring_proxy_monitor(
        State(state),
        Extension(user),
        Path(created.id.to_string()),
        Json(update_cmd(299)),
    )
    .await
    .unwrap_err();

    match err {
        ApiError::PlanLimitReached { quota, current, allowed, .. } => {
            assert_eq!(quota, "min_recurring_proxy_interval_seconds");
            assert_eq!(current, 299);
            assert_eq!(allowed, 300);
        }
        other => panic!("expected PlanLimitReached for interval, got {other:?}"),
    }
}

// ── Test 6: AuthZ — a role without DeleteMonitor cannot delete ─────

#[tokio::test]
async fn delete_requires_delete_permission() {
    let tenant = TenantId::new();
    let repo = Arc::new(InMemoryRecurringProxyMonitorRepository::new());
    let resolver =
        InMemoryEntitlementResolver::default().with_override(tenant, pro_entitlements());
    let state = build_state_with(repo, resolver);

    // Owner creates a monitor.
    let (_, Json(created)) = create_recurring_proxy_monitor(
        State(state.clone()),
        Extension(owner_of(tenant)),
        Json(request("to-delete")),
    )
    .await
    .expect("owner create should succeed");

    // Viewer (has ViewMonitor only, NOT DeleteMonitor) tries to delete.
    let err = delete_recurring_proxy_monitor(
        State(state.clone()),
        Extension(viewer_of(tenant)),
        Path(created.id.to_string()),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(err, ApiError::Forbidden(_)),
        "expected Forbidden for a role without DeleteMonitor, got {err:?}"
    );

    // Owner can still delete.
    let status = delete_recurring_proxy_monitor(
        State(state),
        Extension(owner_of(tenant)),
        Path(created.id.to_string()),
    )
    .await
    .expect("owner delete should succeed");
    assert_eq!(status, StatusCode::NO_CONTENT);
}
