use application::{
    commands::ProcessBillingWebhookCommand,
    ports::{BillingAuditLogRepository, SubscriptionRepository},
    use_cases::ProcessBillingWebhookUseCase,
};
use async_trait::async_trait;
use domain::{
    BillingAuditLogEntry, Subscription,
    enums::{BillingWebhookEventType, PlanTier},
    shared::ids::{SubscriptionId, TenantId},
};
use infra::{InMemoryBillingWebhookIdempotencyRepository, InMemorySubscriptionRepository};

#[derive(Clone, Default)]
struct FailOnceBillingAuditLogRepository {
    calls: std::sync::Arc<std::sync::Mutex<usize>>,
}

#[async_trait]
impl BillingAuditLogRepository for FailOnceBillingAuditLogRepository {
    async fn create(
        &self,
        _entry: &BillingAuditLogEntry,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            return Err(Box::new(std::io::Error::other(
                "simulated billing audit log failure",
            )));
        }
        Ok(())
    }
}

fn command(event_id: &str, tenant_id: TenantId) -> ProcessBillingWebhookCommand {
    ProcessBillingWebhookCommand {
        provider_event_id: event_id.to_owned(),
        event_type: BillingWebhookEventType::PaymentSucceeded,
        tenant_id,
        plan_tier: Some(PlanTier::Tier10),
        cancel_at_period_end: false,
        current_period_ends_at: None,
        payload: serde_json::json!({}),
    }
}

#[tokio::test]
async fn failed_webhook_processing_marks_event_pending_and_retry_succeeds() {
    let subscriptions = InMemorySubscriptionRepository::new();
    let idempotency = InMemoryBillingWebhookIdempotencyRepository::new();
    let audit = FailOnceBillingAuditLogRepository::default();

    let tenant_id = TenantId::new();
    let sub = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Free);
    subscriptions.create(&sub).await.unwrap();

    let use_case = ProcessBillingWebhookUseCase::new(
        idempotency.clone(),
        audit.clone(),
        subscriptions.clone(),
    );

    let cmd = command("evt_retry", tenant_id);

    let first = use_case.execute(cmd.clone()).await;
    assert!(first.is_err(), "first attempt must fail in audit log");

    let second = use_case.execute(cmd).await.unwrap();
    assert!(second.processed);

    let updated = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.plan_tier, PlanTier::Tier10);
}
