use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use application::ports::{
    HistoryRetentionRepository, TenantRepository, TenantRetentionSettingsRepository,
    WalletRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use domain::{
    Check, CheckStatus, ErrorType, RegionCode, Tenant,
    entities::{RetentionSource, TenantRetentionSettings},
    shared::ids::{CheckId, MonitorId, TenantId, UserId},
};
use infra::{
    InMemoryHistoryRetentionRepository, InMemoryPricingConfigRepository,
    InMemoryTenantRepository, InMemoryTenantRetentionSettingsRepository,
    InMemoryWalletRepository,
};
use uuid::Uuid;
use worker::{RetentionWorker, retention_worker::DEFAULT_BATCH_SIZE};

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

#[tokio::test]
async fn retention_deletes_older_than_cutoff_per_payg_entitlement() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let free_tenant = tenant("Free Tenant");
    let paid_tenant = tenant("Paid Tenant");
    tenant_repo.create(&free_tenant).await.unwrap();
    tenant_repo.create(&paid_tenant).await.unwrap();

    // Unfunded tenants use the PAYG Free retention. A funded wallet activates
    // PAYG and uses the configured maximum retention.
    wallet_repo
        .get_or_create(paid_tenant.id.0, "USD")
        .await
        .unwrap();
    wallet_repo
        .credit(
            paid_tenant.id.0,
            1_000_000,
            domain::WalletTransactionType::TopUp,
            "retention-test-topup",
            Some("retention test"),
        )
        .await
        .unwrap();

    let pricing = Arc::new(
        InMemoryPricingConfigRepository::new()
            .with_config(domain::pricing::PricingConfig::default_config()),
    );

    // Free retains 2 days; funded PAYG retains the configured 3650 days.
    let old_free = sample_check_older_than_days(3);
    let recent_free = sample_check_older_than_days(0);
    let old_paid = sample_check_older_than_days(10);

    history_repo.seed_check(old_free.clone(), free_tenant.id);
    history_repo.seed_check(recent_free.clone(), free_tenant.id);
    history_repo.seed_check(old_paid.clone(), paid_tenant.id);

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        pricing,
        100,
        Duration::from_secs(3600),
        2,
        wallet_repo,
        Arc::new(InMemoryTenantRetentionSettingsRepository::new()),
    );

    let summary = worker.run_sweep().await.unwrap();

    assert_eq!(summary.tenants_processed, 2);
    assert_eq!(summary.checks_deleted, 1); // Only old_free is beyond its plan retention.

    let remaining_ids = history_repo.list_check_ids();
    assert!(remaining_ids.contains(&recent_free.id));
    assert!(remaining_ids.contains(&old_paid.id));
    assert!(!remaining_ids.contains(&old_free.id));
}

#[tokio::test]
async fn batching_does_not_delete_more_or_less_than_expected() {
    let tenant_id = TenantId::new();
    let repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    for _ in 0..5 {
        repo.seed_check(sample_check_older_than_days(2), tenant_id);
    }

    let cutoff = Utc::now() - ChronoDuration::days(2);
    let batch_size = 2;

    let deleted1 = repo
        .delete_older_than_batch(tenant_id, cutoff, batch_size)
        .await
        .unwrap();
    let deleted2 = repo
        .delete_older_than_batch(tenant_id, cutoff, batch_size)
        .await
        .unwrap();
    let deleted3 = repo
        .delete_older_than_batch(tenant_id, cutoff, batch_size)
        .await
        .unwrap();

    assert_eq!(deleted1, 2);
    assert_eq!(deleted2, 2);
    assert_eq!(deleted3, 1);
    assert!(repo.list_check_ids().is_empty());
}

#[tokio::test]
async fn running_twice_is_idempotent() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let tenant = tenant("Idempotent Tenant");
    tenant_repo.create(&tenant).await.unwrap();
    let _pricing = Arc::new(
        InMemoryPricingConfigRepository::new()
            .with_config(domain::pricing::PricingConfig::default_config()),
    );

    // Free retention is 2 days. Keep one fresh check and two checks beyond the cutoff.
    let old1 = sample_check_older_than_days(3);
    let old2 = sample_check_older_than_days(4);
    let recent = sample_check_older_than_days(0);

    history_repo.seed_check(old1.clone(), tenant.id);
    history_repo.seed_check(old2.clone(), tenant.id);
    history_repo.seed_check(recent.clone(), tenant.id);

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(
            InMemoryPricingConfigRepository::new()
                .with_config(domain::pricing::PricingConfig::default_config()),
        ),
        DEFAULT_BATCH_SIZE,
        Duration::from_secs(3600),
        2,
        wallet_repo,
        Arc::new(InMemoryTenantRetentionSettingsRepository::new()),
    );

    let first = worker.run_sweep().await.unwrap();
    assert_eq!(first.checks_deleted, 2);

    let second = worker.run_sweep().await.unwrap();
    assert_eq!(second.checks_deleted, 0);

    let remaining = history_repo.list_check_ids();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0], recent.id);
}

/// H16 Task A regression test: wraps `HistoryRetentionRepository` and
/// records how many `delete_older_than_batch` calls are in flight at
/// once. Proves `RetentionWorker::run_sweep`'s semaphore actually caps
/// concurrent per-tenant work at `max_concurrent_tenants` — not just
/// that the code compiles with a `Semaphore` in it — and that every
/// tenant still gets processed despite the bound (no starvation /
/// resource exhaustion).
struct ConcurrencyTrackingHistoryRepository<H> {
    inner: Arc<H>,
    in_flight: Arc<AtomicUsize>,
    max_observed: Arc<AtomicUsize>,
    delay: Duration,
}

#[async_trait]
impl<H> HistoryRetentionRepository for ConcurrencyTrackingHistoryRepository<H>
where
    H: HistoryRetentionRepository + Send + Sync,
{
    async fn delete_older_than_batch(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_observed.fetch_max(current, Ordering::SeqCst);

        tokio::time::sleep(self.delay).await;

        let result = self
            .inner
            .delete_older_than_batch(tenant_id, cutoff, batch_size)
            .await;

        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

#[tokio::test]
async fn bounded_concurrency_never_exceeds_configured_limit() {
    const TENANT_COUNT: usize = 12;
    const MAX_CONCURRENT_TENANTS: usize = 3;

    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    for i in 0..TENANT_COUNT {
        let t = tenant(&format!("Concurrency Tenant {i}"));
        tenant_repo.create(&t).await.unwrap();
        history_repo.seed_check(sample_check_older_than_days(10), t.id);
    }

    let tracking_repo = Arc::new(ConcurrencyTrackingHistoryRepository {
        inner: history_repo,
        in_flight: Arc::new(AtomicUsize::new(0)),
        max_observed: Arc::new(AtomicUsize::new(0)),
        delay: Duration::from_millis(50),
    });

    let worker = RetentionWorker::new(
        tenant_repo,
        tracking_repo.clone(),
        Arc::new(
            InMemoryPricingConfigRepository::new()
                .with_config(domain::pricing::PricingConfig::default_config()),
        ),
        100,
        Duration::from_secs(3600),
        MAX_CONCURRENT_TENANTS,
        wallet_repo,
        Arc::new(InMemoryTenantRetentionSettingsRepository::new()),
    );

    let summary = worker.run_sweep().await.unwrap();

    assert_eq!(summary.tenants_processed, TENANT_COUNT as u64);
    assert_eq!(summary.checks_deleted, TENANT_COUNT as u64);

    let max_observed = tracking_repo.max_observed.load(Ordering::SeqCst);
    assert!(
        max_observed <= MAX_CONCURRENT_TENANTS,
        "observed {max_observed} tenants being processed concurrently, expected at most \
         {MAX_CONCURRENT_TENANTS} — bounded concurrency is not being respected \
         (resource exhaustion risk)"
    );
    assert!(
        max_observed >= 2,
        "expected genuine concurrent processing (at least 2 tenants in flight at once), \
         only observed {max_observed} — sweep may have regressed to fully sequential"
    );
}

#[tokio::test]
async fn legacy_retention_is_preserved_for_non_payg_tenants() {
    // Blueprint: a pre-PAYG tenant with a `subscriptions.retention_days`
    // of 3650 must keep that value after the migration, so the retention
    // worker does not delete their history on the first sweep.
    //
    // We simulate the migration by pre-seeding `tenant_retention_settings`
    // with `source = Legacy` BEFORE running the worker.
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());
    let retention_repo = Arc::new(InMemoryTenantRetentionSettingsRepository::new());

    let legacy_tenant = tenant("Legacy Tenant");
    tenant_repo.create(&legacy_tenant).await.unwrap();

    // Wallet exists but PAYG is NOT activated.
    wallet_repo
        .get_or_create(legacy_tenant.id.0, "USD")
        .await
        .unwrap();

    retention_repo
        .upsert(&TenantRetentionSettings::new(
            legacy_tenant.id,
            3650,
            RetentionSource::Legacy,
        ))
        .await
        .unwrap();

    // A check from 200 days ago would be deleted under free-tier
    // retention (2 days) but preserved under legacy retention (3650).
    history_repo.seed_check(
        sample_check_older_than_days(200),
        legacy_tenant.id,
    );

    let worker = RetentionWorker::new(
        tenant_repo,
        history_repo.clone(),
        Arc::new(
            InMemoryPricingConfigRepository::new()
                .with_config(domain::pricing::PricingConfig::default_config()),
        ),
        DEFAULT_BATCH_SIZE,
        Duration::from_secs(3600),
        2,
        wallet_repo,
        retention_repo,
    );

    let summary = worker.run_sweep().await.unwrap();
    assert_eq!(summary.tenants_processed, 1);
    assert_eq!(
        summary.checks_deleted, 0,
        "legacy 3650-day retention must not delete a 200-day-old check"
    );
}

