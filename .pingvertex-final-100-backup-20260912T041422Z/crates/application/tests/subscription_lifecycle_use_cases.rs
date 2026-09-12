use application::commands::{
    BeginGracePeriod, CancelSubscription, ExpireSubscription, FinalizeScheduledCancellation,
};
use application::ports::SubscriptionRepository;
use application::use_cases::{
    BeginGracePeriodUseCase, CancelSubscriptionUseCase, ExpireSubscriptionUseCase,
    FinalizeScheduledCancellationUseCase,
};
use domain::Subscription;
use domain::enums::{PlanTier, SubscriptionStatus};
use domain::shared::ids::{SubscriptionId, TenantId};
use infra::InMemorySubscriptionRepository;

#[tokio::test]
async fn begin_grace_period_moves_active_subscription_to_past_due() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    repository
        .create(&Subscription::new(
            SubscriptionId::new(),
            tenant_id,
            PlanTier::Starter,
        ))
        .await
        .unwrap();

    let use_case = BeginGracePeriodUseCase::new(repository.clone());
    use_case
        .execute(BeginGracePeriod {
            tenant_id,
            grace_period_ends_at: chrono::Utc::now() + chrono::Duration::days(3),
        })
        .await
        .unwrap();

    let subscription = repository
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(subscription.status, SubscriptionStatus::PastDue);
}

#[tokio::test]
async fn begin_grace_period_errors_when_no_subscription_exists() {
    let repository = InMemorySubscriptionRepository::new();
    let use_case = BeginGracePeriodUseCase::new(repository);

    let result = use_case
        .execute(BeginGracePeriod {
            tenant_id: TenantId::new(),
            grace_period_ends_at: chrono::Utc::now(),
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn expire_subscription_moves_past_due_to_expired() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Pro);
    subscription.status = SubscriptionStatus::PastDue;
    repository.create(&subscription).await.unwrap();

    let use_case = ExpireSubscriptionUseCase::new(repository.clone());
    use_case
        .execute(ExpireSubscription { tenant_id })
        .await
        .unwrap();

    let subscription = repository
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(subscription.status, SubscriptionStatus::Expired);
}

#[tokio::test]
async fn expire_subscription_rejects_directly_from_active() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    repository
        .create(&Subscription::new(
            SubscriptionId::new(),
            tenant_id,
            PlanTier::Pro,
        ))
        .await
        .unwrap();

    let use_case = ExpireSubscriptionUseCase::new(repository);
    let result = use_case.execute(ExpireSubscription { tenant_id }).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn cancel_subscription_immediately_sets_canceled_status() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    repository
        .create(&Subscription::new(
            SubscriptionId::new(),
            tenant_id,
            PlanTier::Starter,
        ))
        .await
        .unwrap();

    let use_case = CancelSubscriptionUseCase::new(repository.clone());
    use_case
        .execute(CancelSubscription {
            tenant_id,
            at_period_end: false,
            current_period_ends_at: None,
        })
        .await
        .unwrap();

    let subscription = repository
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(subscription.status, SubscriptionStatus::Canceled);
}

#[tokio::test]
async fn finalize_scheduled_cancellation_completes_an_end_of_period_cancellation() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    repository
        .create(&Subscription::new(
            SubscriptionId::new(),
            tenant_id,
            PlanTier::Starter,
        ))
        .await
        .unwrap();

    let cancel_use_case = CancelSubscriptionUseCase::new(repository.clone());
    cancel_use_case
        .execute(CancelSubscription {
            tenant_id,
            at_period_end: true,
            current_period_ends_at: Some(chrono::Utc::now() + chrono::Duration::days(5)),
        })
        .await
        .unwrap();

    // Still active until the period actually ends.
    let subscription = repository
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(subscription.status, SubscriptionStatus::Active);
    assert!(subscription.cancel_at_period_end);

    let finalize_use_case = FinalizeScheduledCancellationUseCase::new(repository.clone());
    finalize_use_case
        .execute(FinalizeScheduledCancellation { tenant_id })
        .await
        .unwrap();

    let subscription = repository
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(subscription.status, SubscriptionStatus::Canceled);
}

#[tokio::test]
async fn finalize_scheduled_cancellation_errors_when_none_scheduled() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    repository
        .create(&Subscription::new(
            SubscriptionId::new(),
            tenant_id,
            PlanTier::Starter,
        ))
        .await
        .unwrap();

    let use_case = FinalizeScheduledCancellationUseCase::new(repository);
    let result = use_case
        .execute(FinalizeScheduledCancellation { tenant_id })
        .await;

    assert!(result.is_err());
}
