use application::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use application::use_cases::{ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::Utc;
use infra::repositories::{
    in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository,
    in_memory_usage_event_repository::InMemoryUsageEventRepository,
    in_memory_wallet_repository::InMemoryWalletRepository,
};
use std::sync::Arc;
use uuid::Uuid;

async fn setup(
    unit_price: i64,
    _version: i64,
) -> (
    Arc<InMemoryWalletRepository>,
    Arc<dyn PAYGPricingRepository>,
    Arc<dyn UsageEventRepository>,
    Uuid,
) {
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
    let pricing_repo = Arc::new(InMemoryPAYGPricingRepository::new());

    pricing_repo
        .create_version(
            "http_check.monitoring",
            "check",
            unit_price,
            "USD",
            Utc::now() - chrono::Duration::minutes(1),
        )
        .await
        .unwrap();

    let usage_repo = Arc::new(InMemoryUsageEventRepository::default());
    let tenant_id = Uuid::new_v4();

    (wallet_repo, pricing_repo, usage_repo, tenant_id)
}

#[tokio::test]
async fn execute_successful_operation_charges_wallet_and_returns_event() {
    let (wallet_repo, pricing_repo, usage_repo, tenant_id) = setup(10, 1).await;
    wallet_repo.get_or_create(tenant_id, "USD").await.unwrap();
    wallet_repo
        .credit(
            tenant_id,
            100,
            domain::WalletTransactionType::TopUp,
            "test",
            None,
        )
        .await
        .unwrap();

    let use_case = ChargeForUsageUseCase::new(
        wallet_repo.clone() as Arc<dyn WalletRepository>,
        pricing_repo.clone(),
        usage_repo.clone(),
    );

    let input = ChargeForUsageInput {
        tenant_id,
        operation: "http_check.monitoring".to_owned(),
        units: 2,
        request_id: "req-1".to_owned(),
        monitor_id: None,
        execution_id: None,
        pricing_context: None,
    };

    let result = use_case
        .execute(input, || async { Ok::<(), std::io::Error>(()) })
        .await
        .unwrap();

    assert_eq!(result.usage_event.status, "CAPTURED");
    assert_eq!(result.usage_event.charge_units, 20);

    let wallet = wallet_repo.find(tenant_id).await.unwrap().unwrap();
    assert_eq!(wallet.balance_units, 80);
}
