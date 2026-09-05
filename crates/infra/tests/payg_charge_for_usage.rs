use application::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use application::{ChargeForUsageError, ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::{Duration, Utc};
use domain::PAYGPricing;
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

fn setup(
    unit_price: i64,
    version: i64,
) -> (
    Arc<dyn WalletRepository>,
    Arc<dyn PAYGPricingRepository>,
    Arc<dyn UsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());

    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version,
        operation: "proxy_check".into(),
        unit_name: "check".into(),
        unit_price,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::seconds(1),
        effective_until: None,
        active: true,
    }]));

    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let tenant = Uuid::new_v4();

    (wallet, pricing, usage, tenant)
}

#[tokio::test]
async fn duplicate_request_returns_previous_result_without_running_operation_twice() {
    let (wallet, pricing, usage, tenant) = setup(10, 7);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-1",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let calls = Arc::new(AtomicUsize::new(0));

    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 2,
        request_id: "req-1".into(),
    };

    let c = calls.clone();
    let first = uc
        .execute(input.clone(), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(())
            }
        })
        .await
        .unwrap();

    let c = calls.clone();
    let second = uc
        .execute(input, move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(())
            }
        })
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.usage_event, second.usage_event);
    assert_eq!(second.usage_event.pricing_version, 7);
    assert_eq!(second.usage_event.charge_units, 20);
}

#[tokio::test]
async fn concurrent_different_requests_never_overspend() {
    let (wallet, pricing, usage, tenant) = setup(60, 3);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-2",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(wallet.clone(), pricing, usage));
    let mut tasks = Vec::new();

    for i in 0..10 {
        let uc = uc.clone();

        tasks.push(tokio::spawn(async move {
            let input = ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: format!("concurrent-{i}"),
            };

            uc.execute(input, || async { Ok::<_, &'static str>(()) })
                .await
        }));
    }

    let mut success = 0;

    for task in tasks {
        if task.await.unwrap().is_ok() {
            success += 1;
        }
    }

    assert_eq!(success, 1);

    let final_wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(final_wallet.balance_units, 40);
}

#[tokio::test]
async fn insufficient_balance_is_rejected_and_wallet_never_goes_negative() {
    let (wallet, pricing, usage, tenant) = setup(60, 2);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            50,
            domain::WalletTransactionType::TopUp,
            "topup-3",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);

    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "insufficient".into(),
            },
            || async { Ok::<_, &'static str>(()) },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::InsufficientBalance)
    ));

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        50
    );
}

#[tokio::test]
async fn failed_operation_releases_reservation_and_does_not_debit() {
    let (wallet, pricing, usage, tenant) = setup(25, 9);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-4",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 2,
                request_id: "failed-op".into(),
            },
            || async { Err::<(), _>("downstream failed") },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationFailed(_))
    ));

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        100
    );

    let event = usage
        .find_by_request(tenant, "failed-op")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(event.status, "FAILED");
    assert_eq!(event.units_billed, 0);
}
