use application::ports::AddOnSubscriptionRepository;
use application::use_cases::add_on_subscription::CreateAddOnSubscriptionUseCase;
use domain::shared::ids::TenantId;
use domain::{AddOnStatus, AddOnType};
use infra::InMemoryAddOnSubscriptionRepository;
use rust_decimal::Decimal;

#[tokio::test]
async fn creates_and_persists_addon_subscription() {
    let repo = std::sync::Arc::new(InMemoryAddOnSubscriptionRepository::new());
    let uc = CreateAddOnSubscriptionUseCase::new(repo.clone());
    let tenant = TenantId::new();
    let addon = uc
        .execute(
            tenant,
            AddOnType::SmsCredits,
            1000,
            None,
            Decimal::new(2, 3),
            "USD".into(),
            AddOnStatus::Pending,
        )
        .await
        .unwrap();
    assert_eq!(addon.tenant_id, tenant);
    assert_eq!(addon.total_price(), Decimal::new(2000, 3));
    assert_eq!(repo.find_by_id(addon.id).await.unwrap().unwrap(), addon);
}
