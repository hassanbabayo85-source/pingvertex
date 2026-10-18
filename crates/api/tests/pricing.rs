use axum::response::IntoResponse;
use axum::{
    Extension,
    extract::{Json, State},
};
use domain::{
    MonitorKind, TenantId, UserId,
    shared::types::Role,
    shared::types::{AuthenticatedUser, TenantContext},
};
use infra::{
    InMemoryMonitorIntervalPolicyRepository, InMemoryPAYGPricingRepository,
    InMemoryTenantRepository,
};
use std::sync::Arc;

use api::{
    handlers::payg_preview::{PaygPreviewRequest, PaygPreviewResponse, payg_preview},
    state::AppState,
};

mod common;

async fn fixture() -> (AppState, AuthenticatedUser) {
    let payg_repo = Arc::new(InMemoryPAYGPricingRepository::new());

    application::ports::PAYGPricingRepository::create_version(
        &*payg_repo,
        "http_check.monitoring",
        "check",
        2,
        "USD",
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await
    .expect("test PAYG pricing must be created");

    let interval_policies = InMemoryMonitorIntervalPolicyRepository::default()
        .with_policy(MonitorKind::Http, 30)
        .with_policy(MonitorKind::Tcp, 30)
        .with_policy(MonitorKind::Dns, 30)
        .with_policy(MonitorKind::Heartbeat, 30)
        .with_policy(MonitorKind::Flow, 30);

    let monitor_repository = infra::InMemoryMonitorRepository::new();

    let state = AppState::new(
        monitor_repository.clone(),
        common::NoopCheckRepository,
        common::NoopDiagnosticReportRepository,
        common::NoopIncidentRepository,
        infra::InMemoryNotificationChannelRepository::new(monitor_repository),
        infra::NoopNotificationService::new(),
        infra::InMemoryApiFlowRepository::new(),
        infra::InMemoryProxyCheckJobRepository::new(),
        infra::UuidIdGenerator::new(),
        Arc::new(common::NoopSecretCipher),
        infra::InMemoryAuditLogEntryRepository::new(),
    )
    .with_payg_pricing_repository(payg_repo)
    .with_monitor_interval_policy_repository(interval_policies);

    let tenant_id = TenantId::new();
    let user_id = UserId::new();

    let tenant_repository = InMemoryTenantRepository::new();
    tenant_repository.seed_with_id(tenant_id, user_id);

    let state =
        state.with_platform_admin_repositories(common::NoopUserRepository, tenant_repository);

    let user = AuthenticatedUser::new(user_id, Role::Owner, TenantContext::new(tenant_id));

    (state, user)
}

#[tokio::test]
async fn pricing_quote_returns_full_breakdown() {
    let (state, user) = fixture().await;

    let Json(response): Json<PaygPreviewResponse> = payg_preview(
        State(state),
        Extension(user),
        Json(PaygPreviewRequest {
            monitors: 500,
            interval_seconds: 30,
            regions: 1,
            forecast_days: 90,
            kind: MonitorKind::Http,
        }),
    )
    .await
    .expect("PAYG preview must succeed");

    assert_eq!(response.monitors, 500);
    assert_eq!(response.interval_seconds, 30);
    assert_eq!(response.regions, 1);
    assert_eq!(response.forecast_days, 90);
    assert_eq!(response.checks_per_day, 2_880);
    assert_eq!(response.checks_total, 129_600_000);
    assert_eq!(response.price_per_check_units, 2);
    assert_eq!(response.currency, "USD");
    assert_eq!(response.pricing_version, 1);
    assert_eq!(response.estimated_cost_units, 259_200_000);
    assert_eq!(response.estimate_label, "Estimated cost");
}

#[tokio::test]
async fn pricing_quote_validation_error() {
    let (state, user) = fixture().await;

    let result = payg_preview(
        State(state),
        Extension(user),
        Json(PaygPreviewRequest {
            monitors: 0,
            interval_seconds: 30,
            regions: 1,
            forecast_days: 90,
            kind: MonitorKind::Http,
        }),
    )
    .await;

    assert!(result.is_err(), "zero monitors must be rejected");

    let error = result.unwrap_err();

    let response = error.into_response();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}
