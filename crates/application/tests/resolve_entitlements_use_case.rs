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
    assert_eq!(plan.max_monitors, 100);
    assert_eq!(plan.min_check_interval_seconds, 120);
    assert_eq!(plan.history_retention_days, 7);
}

#[tokio::test]
async fn resolves_correct_limits_for_each_paid_tier() {
    let cases = [
        (PlanTier::Tier1, 250, 60, 30),
        (PlanTier::Tier5, 500, 60, 90),
        (PlanTier::Tier10, 1000, 30, 180),
        (PlanTier::Tier20, 2500, 30, 365),
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
        PlanTier::Tier10,
        chrono::Utc::now(),
    );
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Tier10);
}

#[tokio::test]
async fn past_due_subscription_still_resolves_full_plan_entitlements() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier5);
    subscription.status = SubscriptionStatus::PastDue;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Tier5);
}

#[tokio::test]
async fn expired_subscription_falls_back_to_free_plan() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier20);
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
    let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier20);
    subscription.status = SubscriptionStatus::Canceled;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
}
