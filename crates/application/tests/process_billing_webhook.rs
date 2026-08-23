use application::{
    commands::ProcessBillingWebhookCommand, ports::SubscriptionRepository,
    use_cases::ProcessBillingWebhookUseCase,
};
use domain::{
    Subscription,
    enums::{BillingWebhookEventType, PlanTier, SubscriptionStatus},
    shared::ids::{SubscriptionId, TenantId},
};
use infra::{
    InMemoryBillingAuditLogRepository, InMemoryBillingWebhookIdempotencyRepository,
    InMemorySubscriptionRepository,
};

fn command(
    event_id: &str,
    event_type: BillingWebhookEventType,
    tenant_id: TenantId,
    plan_tier: Option<PlanTier>,
) -> ProcessBillingWebhookCommand {
    ProcessBillingWebhookCommand {
        provider_event_id: event_id.to_owned(),
        event_type,
        tenant_id,
        plan_tier,
        cancel_at_period_end: false,
        current_period_ends_at: None,
        payload: serde_json::json!({}),
    }
}

#[tokio::test]
async fn duplicate_delivery_is_safely_ignored() {
    let subscriptions = InMemorySubscriptionRepository::new();
    let idempotency = InMemoryBillingWebhookIdempotencyRepository::new();
    let audit = InMemoryBillingAuditLogRepository::new();

    let tenant_id = TenantId::new();
    let sub = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier5);
    subscriptions.create(&sub).await.unwrap();

    let use_case = ProcessBillingWebhookUseCase::new(
        idempotency.clone(),
        audit.clone(),
        subscriptions.clone(),
    );

    let cmd = command(
        "evt_1",
        BillingWebhookEventType::PaymentSucceeded,
        tenant_id,
        Some(PlanTier::Tier10),
    );
    let first = use_case.execute(cmd.clone()).await.unwrap();
    assert!(first.processed);

    let after_first = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_first.plan_tier, PlanTier::Tier10);
    assert_eq!(after_first.status, SubscriptionStatus::Active);

    // Duplicate delivery with same provider_event_id.
    let second = use_case.execute(cmd).await.unwrap();
    assert!(!second.processed);

    let after_second = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_second.plan_tier, PlanTier::Tier10);
    assert_eq!(after_second.status, SubscriptionStatus::Active);

    // Only one audit log entry should exist.
    assert_eq!(audit.entries().len(), 1);
}

#[tokio::test]
async fn payment_succeeded_transitions_expired_to_active_and_updates_tier() {
    let subscriptions = InMemorySubscriptionRepository::new();
    let idempotency = InMemoryBillingWebhookIdempotencyRepository::new();
    let audit = InMemoryBillingAuditLogRepository::new();

    let tenant_id = TenantId::new();
    let mut sub = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Free);
    sub.status = SubscriptionStatus::Expired;
    subscriptions.create(&sub).await.unwrap();

    let use_case = ProcessBillingWebhookUseCase::new(
        idempotency.clone(),
        audit.clone(),
        subscriptions.clone(),
    );

    use_case
        .execute(command(
            "evt_paid",
            BillingWebhookEventType::PaymentSucceeded,
            tenant_id,
            Some(PlanTier::Tier20),
        ))
        .await
        .unwrap();

    let updated = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, SubscriptionStatus::Active);
    assert_eq!(updated.plan_tier, PlanTier::Tier20);
}

#[tokio::test]
async fn payment_failed_moves_active_to_past_due() {
    let subscriptions = InMemorySubscriptionRepository::new();
    let idempotency = InMemoryBillingWebhookIdempotencyRepository::new();
    let audit = InMemoryBillingAuditLogRepository::new();

    let tenant_id = TenantId::new();
    let sub = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier5);
    subscriptions.create(&sub).await.unwrap();

    let use_case = ProcessBillingWebhookUseCase::new(
        idempotency.clone(),
        audit.clone(),
        subscriptions.clone(),
    );

    use_case
        .execute(command(
            "evt_failed",
            BillingWebhookEventType::PaymentFailed,
            tenant_id,
            None,
        ))
        .await
        .unwrap();

    let updated = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, SubscriptionStatus::PastDue);
    assert!(updated.grace_period_ends_at.is_some());
}

#[tokio::test]
async fn subscription_canceled_moves_active_to_canceled_immediately() {
    let subscriptions = InMemorySubscriptionRepository::new();
    let idempotency = InMemoryBillingWebhookIdempotencyRepository::new();
    let audit = InMemoryBillingAuditLogRepository::new();

    let tenant_id = TenantId::new();
    let sub = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier1);
    subscriptions.create(&sub).await.unwrap();

    let use_case = ProcessBillingWebhookUseCase::new(
        idempotency.clone(),
        audit.clone(),
        subscriptions.clone(),
    );

    use_case
        .execute(command(
            "evt_cancel",
            BillingWebhookEventType::SubscriptionCanceled,
            tenant_id,
            None,
        ))
        .await
        .unwrap();

    let updated = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, SubscriptionStatus::Canceled);
    assert!(updated.canceled_at.is_some());
}
