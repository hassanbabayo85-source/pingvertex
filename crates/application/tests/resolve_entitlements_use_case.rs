use application::ResolveEntitlementsUseCase;
use application::ports::SubscriptionRepository;
use application::queries::ResolveEntitlements;
use domain::Subscription;
use domain::enums::{PlanTier, SubscriptionStatus};
use domain::shared::ids::{SubscriptionId, TenantId};
use infra::InMemorySubscriptionRepository;

#[tokio::test]
async fn defaults_to_free_plan_when_no_subscription_exists() {
    let use_case = ResolveEntitlementsUseCase::new(InMemorySubscriptionRepository::new());
    let tenant_id = TenantId::new();

    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
    assert_eq!(plan.max_monitors, 20);
    assert_eq!(plan.min_check_interval_seconds, 300);
    assert_eq!(plan.history_retention_days, 2);
}

#[tokio::test]
async fn resolves_correct_limits_for_each_paid_tier() {
    let cases = [
        (PlanTier::Starter, 25, 60, 30),
        (PlanTier::Pro, 100, 30, 90),
        (PlanTier::Business, 500, 15, 365),
        (PlanTier::Enterprise, 10_000, 15, 3650),
    ];

    for (tier, max_monitors, min_interval, retention_days) in cases {
        let repository = InMemorySubscriptionRepository::new();
        let tenant_id = TenantId::new();
        let subscription = Subscription::new(SubscriptionId::new(), tenant_id, tier);
        repository.create(&subscription).await.unwrap();

        let use_case = ResolveEntitlementsUseCase::new(repository);

        let plan = use_case
            .execute(ResolveEntitlements { tenant_id })
            .await
            .unwrap();

        assert_eq!(plan.tier, tier);
        assert_eq!(plan.max_monitors, max_monitors);
        assert_eq!(plan.min_check_interval_seconds, min_interval);
        assert_eq!(plan.history_retention_days, retention_days);
    }
}

#[tokio::test]
async fn trialing_subscription_resolves_full_plan_entitlements() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let subscription = Subscription::new_trialing(
        SubscriptionId::new(),
        tenant_id,
        PlanTier::Business,
        chrono::Utc::now(),
    );
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Business);
}

#[tokio::test]
async fn past_due_subscription_still_resolves_full_plan_entitlements() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Pro);
    subscription.status = SubscriptionStatus::PastDue;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Pro);
}

#[tokio::test]
async fn expired_subscription_falls_back_to_free_plan() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription =
        Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Enterprise);
    subscription.status = SubscriptionStatus::Expired;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
}

#[tokio::test]
async fn canceled_subscription_falls_back_to_free_plan() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription =
        Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Enterprise);
    subscription.status = SubscriptionStatus::Canceled;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
}

#[tokio::test]
async fn free_entitlements_follow_active_database_pricing_config() {
    let subscription_repository = InMemorySubscriptionRepository::new();
    let pricing_repository = infra::InMemoryPricingConfigRepository::default().with_config({
        let mut config = domain::pricing::PricingConfig::default_config();
        config.free_tier.monitors = 7;
        config.free_tier.interval_seconds = 420;
        config.free_tier.retention_days = 4;
        config
    });
    let tenant_id = TenantId::new();

    let plan = ResolveEntitlementsUseCase::new(subscription_repository)
        .with_pricing_config_repository(std::sync::Arc::new(pricing_repository))
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.max_monitors, 7);
    assert_eq!(plan.min_check_interval_seconds, 420);
    assert_eq!(plan.history_retention_days, 4);
}
