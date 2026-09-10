//! PAYG Part-3 release-gate load/stress tests.
//!
//! These tests deliberately exercise the same application ports used by the
//! production PostgreSQL adapters, using lock-safe in-memory adapters to make
//! the suite deterministic and runnable without an external service.

use application::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use application::{ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::Utc;
use domain::{WalletSettings, WalletTransactionType};
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::Arc;
use tokio::task::JoinSet;
use uuid::Uuid;

async fn setup() -> (
    Arc<InMemoryWalletRepository>,
    Arc<InMemoryPAYGPricingRepository>,
    Arc<InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    pricing
        .create_version("http_check.monitoring", "check", 1, "USD", Utc::now())
        .await
        .unwrap();
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let tenant = Uuid::new_v4();
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            20_000,
            WalletTransactionType::TopUp,
            "load-seed",
            None,
        )
        .await
        .unwrap();
    (wallet, pricing, usage, tenant)
}

#[tokio::test]
async fn ten_thousand_unique_payg_executions_are_not_lost_or_duplicated() {
    let (wallet, pricing, usage, tenant) = setup().await;
    let meter = Arc::new(ChargeForUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let mut tasks = JoinSet::new();

    for i in 0..10_000_u32 {
        let meter = meter.clone();
        tasks.spawn(async move {
            meter
                .execute(
                    ChargeForUsageInput {
                        tenant_id: tenant,
                        operation: "http_check.monitoring".to_owned(),
                        units: 1,
                        request_id: format!("load-{i}"),
                        monitor_id: None,
                        execution_id: Some(Uuid::new_v4()),
                        pricing_context: None,
                    },
                    || async { Ok::<(), &'static str>(()) },
                )
                .await
        });
    }

    let mut completed = 0usize;
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
        completed += 1;
    }
    assert_eq!(completed, 10_000);
    let report = usage
        .report(
            tenant,
            Utc::now() - chrono::Duration::hours(1),
            Utc::now() + chrono::Duration::minutes(1),
            None,
            None,
            20_000,
        )
        .await
        .unwrap();
    // `rows` is intentionally capped at 500 by the repository contract
    // (mirrors the Postgres adapter's `LIMIT clamp(1,500)`); it is a
    // detail-list preview, not the full dataset.
    assert_eq!(report.rows.len(), 500);
    // Aggregates must reflect the FULL 10,000-event dataset, not the
    // capped detail rows — this is what actually proves no events were
    // lost or duplicated under concurrent load.
    assert_eq!(report.month_usage_units, 10_000);
    assert_eq!(
        wallet.find(tenant).await.unwrap().unwrap().balance_units,
        10_000
    );
}

#[tokio::test]
async fn one_hundred_twenty_eight_concurrent_replays_create_one_debit() {
    let (wallet, pricing, usage, tenant) = setup().await;
    let meter = Arc::new(ChargeForUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "http_check.monitoring".to_owned(),
        units: 1,
        request_id: "concurrent-same-request".to_owned(),
        monitor_id: None,
        execution_id: Some(Uuid::new_v4()),
        pricing_context: None,
    };
    let mut tasks = JoinSet::new();
    for _ in 0..128 {
        let meter = meter.clone();
        let input = input.clone();
        tasks.spawn(async move {
            meter
                .execute(input, || async { Ok::<(), &'static str>(()) })
                .await
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }
    assert_eq!(
        wallet.find(tenant).await.unwrap().unwrap().balance_units,
        19_999
    );
    assert_eq!(
        usage
            .find_by_request(tenant, "concurrent-same-request")
            .await
            .unwrap()
            .unwrap()
            .charge_units,
        1
    );
}

#[tokio::test]
async fn concurrent_wallet_credits_are_idempotent() {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let tenant = Uuid::new_v4();
    wallet.get_or_create(tenant, "USD").await.unwrap();
    let mut tasks = JoinSet::new();
    for _ in 0..128 {
        let wallet = wallet.clone();
        tasks.spawn(async move {
            wallet
                .credit(
                    tenant,
                    1_000,
                    WalletTransactionType::TopUp,
                    "provider-payment-once",
                    None,
                )
                .await
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }
    assert_eq!(
        wallet.find(tenant).await.unwrap().unwrap().balance_units,
        1_000
    );
}

#[tokio::test]
async fn pricing_concurrency_keeps_resolution_deterministic() {
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let mut tasks = JoinSet::new();
    for _ in 0..128 {
        let pricing = pricing.clone();
        tasks.spawn(async move {
            pricing
                .create_version("http_check.monitoring", "check", 1, "USD", Utc::now())
                .await
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }

    let resolved = pricing
        .find_active_for_context(
            "http_check.monitoring",
            &application::ports::PricingContext::default(),
        )
        .await;
    // In-memory rules with identical runtime precedence must be rejected by the
    // release contract rather than silently selecting an arbitrary price.
    assert!(
        resolved.is_err(),
        "ambiguous equal-specificity pricing must fail closed"
    );
}

#[tokio::test]
async fn concurrent_ten_thousand_row_imports_respect_the_boundary() {
    let header = "name,url,kind,interval_seconds,timeout_seconds,method,expected_status,tcp_port,dns_record_type,dns_expected_value,heartbeat_interval_seconds,heartbeat_grace_period_seconds,is_public,ssl_check_enabled,ssl_expiry_warning_days,regions,auth_type";
    let row = "monitor,http://example.com,http,300,10,GET,200,,, ,,,false,true,14,us-east,none";
    let csv = Arc::new(format!(
        "{}\n{}",
        header,
        (0..10_000)
            .map(|i| format!("{}-{row}", i))
            .collect::<Vec<_>>()
            .join("\n")
    ));
    let mut tasks = JoinSet::new();
    for _tenant in 0..2 {
        let csv = csv.clone();
        tasks.spawn_blocking(move || application::parsing::bulk_monitor_import::parse_csv(&csv));
    }
    while let Some(result) = tasks.join_next().await {
        let (rows, errors) = result.unwrap().unwrap();
        assert_eq!(rows.len(), 10_000);
        assert!(errors.is_empty());
    }
}

#[tokio::test]
async fn spend_limits_remain_tenant_scoped_under_parallel_requests() {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    for tenant in [tenant_a, tenant_b] {
        wallet.get_or_create(tenant, "USD").await.unwrap();
        wallet
            .credit(
                tenant,
                1_000,
                WalletTransactionType::TopUp,
                &format!("seed-{tenant}"),
                None,
            )
            .await
            .unwrap();
        let mut settings = wallet.get_settings(tenant).await.unwrap();
        settings.daily_spend_limit_units = Some(100);
        settings.monthly_spend_limit_units = Some(100);
        wallet
            .update_settings(&WalletSettings {
                tenant_id: tenant,
                ..settings
            })
            .await
            .unwrap();
    }
    let mut tasks = JoinSet::new();
    for (tenant, prefix) in [(tenant_a, "a"), (tenant_b, "b")] {
        for i in 0..128 {
            let wallet = wallet.clone();
            tasks.spawn(async move {
                wallet
                    .reserve(
                        tenant,
                        1,
                        "http_check.monitoring",
                        1,
                        1,
                        "USD",
                        &format!("{prefix}-{i}"),
                        Utc::now() + chrono::Duration::minutes(1),
                    )
                    .await
            });
        }
    }
    while let Some(result) = tasks.join_next().await {
        let _ = result.unwrap();
    }
    for tenant in [tenant_a, tenant_b] {
        let summary = wallet.get_balance_summary(tenant, "USD").await.unwrap();
        assert!(summary.reserved_units <= 100);
    }
}
