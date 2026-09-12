//! Acceptance tests for the worker-side `bill_network_egress` helper.
//!
//! Proves the live scheduler call-site: after a monitor execution, the
//! scheduler passes the measured egress byte count to `bill_network_egress`,
//! which converts to whole 1 MiB units using the existing
//! `network_egress_units()` and charges via the existing PAYG billing
//! machinery (`MeterUsageUseCase::execute_recoverable`).
//!
//! Covers: 0 bytes, 1 byte, 1 MiB, 1 MiB + 1 byte, and retry/idempotency.

use std::sync::Arc;

use application::MeterUsageUseCase;
use application::ports::{PAYGPricingRepository, WalletRepository};
use chrono::{Duration, Utc};
use domain::{MonitorKind, WalletTransactionType};
use infra::repositories::in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;
use infra::{InMemoryUsageEventRepository, InMemoryWalletRepository};
use uuid::Uuid;
use worker::bill_network_egress;

async fn setup() -> (Arc<InMemoryWalletRepository>, MeterUsageUseCase, Uuid) {
    let tenant = Uuid::new_v4();
    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            10_000,
            WalletTransactionType::TopUp,
            "egress-topup",
            None,
        )
        .await
        .unwrap();

    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let now = Utc::now() - Duration::minutes(1);
    PAYGPricingRepository::create_version(&*pricing, "network_egress", "MB", 100, "USD", now)
        .await
        .unwrap();

    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let meter = MeterUsageUseCase::new(wallet.clone(), pricing, usage);
    (wallet, meter, tenant)
}

async fn balance(wallet: &Arc<InMemoryWalletRepository>, tenant: Uuid) -> i64 {
    wallet.find(tenant).await.unwrap().unwrap().balance_units
}

#[tokio::test]
async fn zero_bytes_is_a_no_op() {
    let (wallet, meter, tenant) = setup().await;
    bill_network_egress(
        &meter,
        tenant,
        Uuid::new_v4(),
        Uuid::new_v4(),
        MonitorKind::Http,
        None,
        None,
        0,
        "egress-zero".to_owned(),
    )
    .await;
    assert_eq!(balance(&wallet, tenant).await, 10_000);
}

#[tokio::test]
async fn one_byte_charges_one_unit() {
    let (wallet, meter, tenant) = setup().await;
    bill_network_egress(
        &meter,
        tenant,
        Uuid::new_v4(),
        Uuid::new_v4(),
        MonitorKind::Http,
        None,
        None,
        1,
        "egress-1b".to_owned(),
    )
    .await;
    assert_eq!(balance(&wallet, tenant).await, 10_000 - 100);
}

#[tokio::test]
async fn one_mib_charges_one_unit() {
    let (wallet, meter, tenant) = setup().await;
    bill_network_egress(
        &meter,
        tenant,
        Uuid::new_v4(),
        Uuid::new_v4(),
        MonitorKind::Http,
        None,
        None,
        1024 * 1024,
        "egress-1mib".to_owned(),
    )
    .await;
    assert_eq!(balance(&wallet, tenant).await, 10_000 - 100);
}

#[tokio::test]
async fn one_mib_plus_one_byte_charges_two_units() {
    let (wallet, meter, tenant) = setup().await;
    bill_network_egress(
        &meter,
        tenant,
        Uuid::new_v4(),
        Uuid::new_v4(),
        MonitorKind::Http,
        None,
        None,
        (1024 * 1024) + 1,
        "egress-1mib-plus-1".to_owned(),
    )
    .await;
    assert_eq!(balance(&wallet, tenant).await, 10_000 - 200);
}

#[tokio::test]
async fn retry_with_same_request_id_does_not_double_charge() {
    let (wallet, meter, tenant) = setup().await;
    let monitor_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    let request_id = format!("monitor-egress:{monitor_id}:{execution_id}");
    for _ in 0..3 {
        bill_network_egress(
            &meter,
            tenant,
            monitor_id,
            execution_id,
            MonitorKind::Http,
            None,
            None,
            1024 * 1024 + 500,
            request_id.clone(),
        )
        .await;
    }
    assert_eq!(
        balance(&wallet, tenant).await,
        10_000 - 200,
        "three retries with the same request id must charge exactly once"
    );
}
