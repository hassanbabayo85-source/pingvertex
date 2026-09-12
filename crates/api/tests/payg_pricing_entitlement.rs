//! Task 2 acceptance test: `GET /payg/pricing` exposes the tenant's
//! current free-tier entitlement so the frontend never hardcodes it.
//!
//! Seeds a custom `PricingConfig` (with free-tier values different from
//! any hardcoded default) into an in-memory pricing config repository,
//! calls the handler directly, and asserts that the response's
//! `free_tier` block reflects the seeded DB values verbatim.
//!
//! Before Task 2, the frontend pages `pricing.rs` and `billing.rs`
//! hardcoded "10 monitors at a 5-minute minimum interval", so a change to
//! the DB free-tier config could not change what the UI showed.

#![allow(dead_code)]

mod common;

use api::handlers::payg_pricing::{FreeTierResponse, PaygPricingPage, payg_pricing};
use api::state::AppState;
use axum::Json;
use axum::extract::{Extension, State};
use domain::pricing::PricingConfig;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::InMemoryApiFlowRepository;
use infra::InMemoryMonitorIntervalPolicyRepository;
use infra::InMemoryMonitorRepository;
use infra::InMemoryNotificationChannelRepository;
use infra::InMemoryProxyCheckJobRepository;
use infra::NoopNotificationService;
use infra::UuidIdGenerator;
use infra::repositories::in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;
use infra::repositories::in_memory_pricing_config_repository::InMemoryPricingConfigRepository;
use std::sync::Arc;

#[tokio::test]
async fn payg_pricing_reflects_db_configured_free_tier() {
    let user_id = UserId::new();
    let tenant_id = TenantId::new();

    // Seed a free tier that is deliberately different from the old
    // hardcoded values (10 monitors, 300 seconds, 2 days). If the
    // handler still had any hardcoded fallback, this test would catch it.
    let mut config = PricingConfig::default_config();
    config.free_tier.monitors = 42;
    config.free_tier.interval_seconds = 1234;
    config.free_tier.retention_days = 7;
    let pricing_repo: Arc<InMemoryPricingConfigRepository> =
        Arc::new(InMemoryPricingConfigRepository::new().with_config(config));

    let payg_repo = Arc::new(InMemoryPAYGPricingRepository::new());
    let interval_policies = InMemoryMonitorIntervalPolicyRepository::default()
        .with_policy(domain::MonitorKind::Http, 47)
        .with_policy(domain::MonitorKind::Tcp, 53)
        .with_policy(domain::MonitorKind::Dns, 59)
        .with_policy(domain::MonitorKind::Heartbeat, 67)
        .with_policy(domain::MonitorKind::Flow, 89);

    let state = AppState::new(
        InMemoryMonitorRepository::new(),
        common::NoopCheckRepository,
        common::NoopDiagnosticReportRepository,
        common::NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(InMemoryMonitorRepository::new()),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(common::NoopSecretCipher),
        infra::InMemoryAuditLogEntryRepository::new(),
    )
    .with_pricing_config_repository(pricing_repo)
    .with_payg_pricing_repository(payg_repo)
    .with_monitor_interval_policy_repository(interval_policies);

    let auth = AuthenticatedUser::new(user_id, Role::Owner, TenantContext::new(tenant_id));

    let Json(PaygPricingPage {
        free_tier,
        preview_limits,
        ..
    }) = payg_pricing(State(state), Extension(auth))
        .await
        .expect("payg_pricing handler must succeed");

    let FreeTierResponse {
        max_monitors,
        min_check_interval_seconds,
        retention_days,
    } = free_tier;

    assert_eq!(
        max_monitors, 42,
        "free_tier.max_monitors must come from the DB config, not a hardcoded default"
    );
    assert_eq!(
        min_check_interval_seconds, 1234,
        "free_tier.min_check_interval_seconds must come from the DB config"
    );
    assert_eq!(
        retention_days, 7,
        "free_tier.retention_days must come from the DB config"
    );

    assert_eq!(preview_limits.min_interval_seconds_http, 47);
    assert_eq!(preview_limits.min_interval_seconds_tcp, 53);
    assert_eq!(preview_limits.min_interval_seconds_dns, 59);
    assert_eq!(preview_limits.min_interval_seconds_heartbeat, 67);
    assert_eq!(preview_limits.min_interval_seconds_flow, 89);
}
