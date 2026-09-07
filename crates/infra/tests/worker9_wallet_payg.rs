use application::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use application::{ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::{Duration, Utc};
use domain::PAYGPricing;
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::Arc;
use uuid::Uuid;

fn setup(
    price: i64,
) -> (
    Arc<InMemoryWalletRepository>,
    Arc<InMemoryPAYGPricingRepository>,
    Arc<InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version: 1,
        operation: "proxy_check".into(),
        unit_name: "check".into(),
        unit_price: price,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::minutes(1),
        effective_until: None,
        active: true,
    }]));
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    (wallet, pricing, usage, Uuid::new_v4())
}

#[tokio::test]
async fn same_request_id_is_idempotent_under_concurrency() {
    let (wallet, pricing, usage, tenant) = setup(2);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 1,
        request_id: "same-request".into(),
    };

    let mut tasks = Vec::new();
    for _ in 0..8 {
        let uc = uc.clone();
        let input = input.clone();
        tasks.push(tokio::spawn(async move {
            uc.execute(input, || async { Ok::<_, &'static str>(()) })
                .await
        }));
    }

    let mut successes = 0;
    for task in tasks {
        if task.await.unwrap().is_ok() {
            successes += 1;
        }
    }
    assert_eq!(successes, 8);

    let wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(wallet.balance_units, 98);

    let event = usage
        .find_by_request(tenant, "same-request")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.charge_units, 2);
}

#[tokio::test]
async fn reservation_expiry_does_not_allow_overspending() {
    let (wallet, _pricing, _usage, tenant) = setup(1);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            10,
            domain::WalletTransactionType::TopUp,
            "topup",
            None,
        )
        .await
        .unwrap();

    let expired = wallet
        .reserve(
            tenant,
            7,
            "proxy_check",
            "expired",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    assert!(expired.created);

    let next = wallet
        .reserve(
            tenant,
            7,
            "proxy_check",
            "fresh",
            Utc::now() + Duration::minutes(1),
        )
        .await
        .unwrap();
    assert!(
        next.created,
        "expired reservation must not permanently lock credits"
    );
}
