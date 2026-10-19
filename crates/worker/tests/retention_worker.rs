//! Task 8C — retention sweep tests.
//!
//! Retention is now derived from `EntitlementResolver.max_history_retention_days`.
//! WalletRepository and PricingConfigRepository are no longer involved.

use std::sync::Arc;
use std::time::Duration;

use application::ports::{TenantRepository, TenantRetentionSettingsRepository};
use chrono::{Duration as ChronoDuration, Utc};
use domain::{
    Check, CheckStatus, Entitlements, ErrorType, RegionCode, Tenant,
    shared::ids::{CheckId, MonitorId, TenantId, UserId},
};
use infra::{
    InMemoryEntitlementResolver, InMemoryHistoryRetentionRepository,
    InMemoryTenantRepository, InMemoryTenantRetentionSettingsRepository,
};
use uuid::Uuid;
use worker::RetentionWorker;

fn sample_check_older_than_days(days: i64) -> Check {
    let mut check = Check::new(
        CheckId(Uuid::new_v4()),
        MonitorId(Uuid::new_v4()),
        RegionCode::default_region(),
        CheckStatus::Up,
        Some(200),
        123,
        ErrorType::None,
        None,
    );
    check.checked_at = Utc::now() - ChronoDuration::days(days);
    check
}

fn tenant(name: &str) -> Tenant {
    Tenant::new(TenantId::new(), name, UserId::new()).unwrap()
}

fn settings_repo() -> Arc<dyn TenantRetentionSettingsRepository> {
    Arc::new(InMemoryTenantRetentionSettingsRepository::new())
}

#[tokio::test]
async fn retention_uses_plan_max_for_free_tier() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let free_tenant = tenant("Free Tenant");
    tenant_repo.create(&free_tenant).await.unwrap();

    // Free: max_history_retention_days = 2.
    history_repo.seed_check(sample_check_older_than_days(3), free_tenant.id);
    history_repo.seed_check(sample_check_older_than_days(0), free_tenant.id);

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(InMemoryEntitlementResolver::default()),
        100,
        Duration::from_secs(3600),
        2,
        settings_repo(),
    );

    let summary = worker.run_sweep().await.unwrap();
    assert_eq!(summary.tenants_processed, 1);
    assert_eq!(summary.checks_deleted, 1);
}

#[tokio::test]
async fn retention_uses_plan_max_for_business_tier() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let business_tenant = tenant("Business Tenant");
    tenant_repo.create(&business_tenant).await.unwrap();

    // Business: max_history_retention_days = 365.
    history_repo.seed_check(sample_check_older_than_days(400), business_tenant.id);
    history_repo.seed_check(sample_check_older_than_days(10), business_tenant.id);

    let resolver = InMemoryEntitlementResolver::default()
        .with_override(business_tenant.id, Entitlements::business_v1_fixture());

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(resolver),
        100,
        Duration::from_secs(3600),
        2,
        settings_repo(),
    );

    let summary = worker.run_sweep().await.unwrap();
    assert_eq!(summary.tenants_processed, 1);
    assert_eq!(summary.checks_deleted, 1);
}

#[tokio::test]
async fn retention_sweep_processes_all_tenants() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let free_tenant = tenant("Free");
    let business_tenant = tenant("Business");
    tenant_repo.create(&free_tenant).await.unwrap();
    tenant_repo.create(&business_tenant).await.unwrap();

    // Free: 3-day-old check deleted (max=2).
    history_repo.seed_check(sample_check_older_than_days(3), free_tenant.id);
    // Business: 10-day-old check kept (max=365).
    history_repo.seed_check(sample_check_older_than_days(10), business_tenant.id);

    let resolver = InMemoryEntitlementResolver::default()
        .with_override(business_tenant.id, Entitlements::business_v1_fixture());

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(resolver),
        100,
        Duration::from_secs(3600),
        2,
        settings_repo(),
    );

    let summary = worker.run_sweep().await.unwrap();
    assert_eq!(summary.tenants_processed, 2);
    assert_eq!(summary.checks_deleted, 1);
}

#[tokio::test]
async fn retention_uses_starter_plan_max() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let starter_tenant = tenant("Starter");
    tenant_repo.create(&starter_tenant).await.unwrap();

    // Starter: max_history_retention_days = 30.
    history_repo.seed_check(sample_check_older_than_days(31), starter_tenant.id);
    history_repo.seed_check(sample_check_older_than_days(29), starter_tenant.id);

    let resolver = InMemoryEntitlementResolver::default()
        .with_override(starter_tenant.id, Entitlements::starter_v1_fixture());

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(resolver),
        100,
        Duration::from_secs(3600),
        2,
        settings_repo(),
    );

    let summary = worker.run_sweep().await.unwrap();
    assert_eq!(summary.checks_deleted, 1);
}

#[tokio::test]
async fn retention_defaults_to_free_tier_for_unknown_tenant() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let unknown_tenant = tenant("Unknown");
    tenant_repo.create(&unknown_tenant).await.unwrap();

    // No override → InMemoryEntitlementResolver defaults to Free (max=2).
    history_repo.seed_check(sample_check_older_than_days(3), unknown_tenant.id);
    history_repo.seed_check(sample_check_older_than_days(0), unknown_tenant.id);

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(InMemoryEntitlementResolver::default()),
        100,
        Duration::from_secs(3600),
        2,
        settings_repo(),
    );

    let summary = worker.run_sweep().await.unwrap();
    assert_eq!(summary.checks_deleted, 1);
}
