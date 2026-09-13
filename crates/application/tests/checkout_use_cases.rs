use async_trait::async_trait;

use application::{
    BillingError, InitiateCheckoutUseCase, VerifyCheckoutUseCase,
    commands::{InitiateCheckoutCommand, VerifyCheckoutCommand},
    ports::{
        CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentVerification,
        SubscriptionRepository,
    },
};
use domain::{
    CheckoutStatus, PlanTier, Subscription, SubscriptionId, SubscriptionStatus,
    shared::ids::{CheckoutSessionId, TenantId},
};
use infra::{InMemoryCheckoutSessionRepository, InMemorySubscriptionRepository};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum TestVerification {
    Paid,
    Pending,
    Failed,
}

#[derive(Debug, Clone)]
struct TestBillingProvider {
    verification: TestVerification,
}

impl TestBillingProvider {
    fn new(verification: TestVerification) -> Self {
        Self { verification }
    }
}

#[async_trait]
impl PaymentProvider for TestBillingProvider {
    fn provider_name(&self) -> &'static str {
        "test"
    }

    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> Result<CheckoutDetails, Box<dyn std::error::Error + Send + Sync>> {
        Ok(CheckoutDetails {
            provider_checkout_id: format!(
                "test-{}",
                request.plan_tier.as_ref().map(|t| t.as_str()).unwrap_or("addon")
            ),
            checkout_url: "https://checkout.test.invalid".to_owned(),
        })
    }

    async fn verify_payment(
        &self,
        _provider_checkout_id: &str,
    ) -> Result<PaymentVerification, Box<dyn std::error::Error + Send + Sync>> {
        Ok(match self.verification {
            TestVerification::Paid => PaymentVerification::Paid {
                provider_payment_id: "test-payment".into(),
                provider_reference: "test-reference".into(),
                amount_units: 1_000_000,
                currency: "USD".into(),
            },
            TestVerification::Pending => PaymentVerification::Pending {
                provider_payment_id: None,
                provider_reference: None,
                amount_units: None,
                currency: None,
            },
            TestVerification::Failed => PaymentVerification::Failed {
                provider_payment_id: None,
                provider_reference: None,
                amount_units: None,
                currency: None,
            },
        })
    }
}

async fn initiate(
    sessions: &InMemoryCheckoutSessionRepository,
    provider: &TestBillingProvider,
    tenant_id: TenantId,
    plan_tier: PlanTier,
) -> CheckoutSessionId {
    let use_case = InitiateCheckoutUseCase::new(sessions.clone(), provider.clone());
    use_case
        .execute(InitiateCheckoutCommand {
            tenant_id,
            plan_tier,
        })
        .await
        .expect("checkout initiation should succeed")
        .checkout_session_id
}

#[tokio::test]
async fn initiate_checkout_rejects_free_plan() {
    let sessions = InMemoryCheckoutSessionRepository::new();
    let provider = TestBillingProvider::new(TestVerification::Paid);
    let use_case = InitiateCheckoutUseCase::new(sessions, provider);

    let result = use_case
        .execute(InitiateCheckoutCommand {
            tenant_id: TenantId::new(),
            plan_tier: PlanTier::Free,
        })
        .await;

    assert!(matches!(result, Err(BillingError::FreePlanNotAllowed)));
}

#[tokio::test]
async fn verify_paid_creates_active_subscription_when_none_exists() {
    let sessions = InMemoryCheckoutSessionRepository::new();
    let subscriptions = InMemorySubscriptionRepository::new();
    let provider = TestBillingProvider::new(TestVerification::Paid);
    let tenant_id = TenantId::new();

    let checkout_id = initiate(&sessions, &provider, tenant_id, PlanTier::Pro).await;

    let use_case = VerifyCheckoutUseCase::new(sessions, subscriptions.clone(), provider);
    let outcome = use_case
        .execute(VerifyCheckoutCommand {
            checkout_session_id: checkout_id,
            tenant_id,
        })
        .await
        .expect("verification should succeed");

    assert_eq!(outcome.checkout_status, CheckoutStatus::Completed);

    let subscription = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .expect("lookup should succeed")
        .expect("subscription should have been created");
    assert_eq!(subscription.status, SubscriptionStatus::Active);
    assert_eq!(subscription.plan_tier, PlanTier::Pro);
}

#[tokio::test]
async fn verify_pending_does_not_create_subscription() {
    let sessions = InMemoryCheckoutSessionRepository::new();
    let subscriptions = InMemorySubscriptionRepository::new();
    let provider = TestBillingProvider::new(TestVerification::Pending);
    let tenant_id = TenantId::new();

    let checkout_id = initiate(&sessions, &provider, tenant_id, PlanTier::Starter).await;

    let use_case = VerifyCheckoutUseCase::new(sessions, subscriptions.clone(), provider);
    let outcome = use_case
        .execute(VerifyCheckoutCommand {
            checkout_session_id: checkout_id,
            tenant_id,
        })
        .await
        .expect("verification should succeed");

    assert_eq!(outcome.checkout_status, CheckoutStatus::Pending);
    assert!(
        subscriptions
            .find_by_tenant_id(tenant_id)
            .await
            .expect("lookup should succeed")
            .is_none()
    );
}

#[tokio::test]
async fn verify_paid_upgrades_existing_expired_subscription_to_active() {
    let sessions = InMemoryCheckoutSessionRepository::new();
    let subscriptions = InMemorySubscriptionRepository::new();
    let provider = TestBillingProvider::new(TestVerification::Paid);
    let tenant_id = TenantId::new();

    let mut existing = Subscription::new_trialing(
        SubscriptionId::new(),
        tenant_id,
        PlanTier::Free,
        chrono::Utc::now(),
    );
    existing.expire().expect("trial should be expirable");
    subscriptions
        .create(&existing)
        .await
        .expect("create should succeed");

    let checkout_id = initiate(&sessions, &provider, tenant_id, PlanTier::Business).await;

    let use_case = VerifyCheckoutUseCase::new(sessions, subscriptions.clone(), provider);
    let outcome = use_case
        .execute(VerifyCheckoutCommand {
            checkout_session_id: checkout_id,
            tenant_id,
        })
        .await
        .expect("verification should succeed");

    assert_eq!(outcome.checkout_status, CheckoutStatus::Completed);

    let subscription = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .expect("lookup should succeed")
        .expect("subscription should exist");
    assert_eq!(subscription.status, SubscriptionStatus::Active);
    assert_eq!(subscription.plan_tier, PlanTier::Business);
}

#[tokio::test]
async fn verify_is_idempotent_after_completion() {
    let sessions = InMemoryCheckoutSessionRepository::new();
    let subscriptions = InMemorySubscriptionRepository::new();
    let provider = TestBillingProvider::new(TestVerification::Paid);
    let tenant_id = TenantId::new();

    let checkout_id = initiate(&sessions, &provider, tenant_id, PlanTier::Enterprise).await;

    let use_case = VerifyCheckoutUseCase::new(sessions.clone(), subscriptions.clone(), provider);
    let first = use_case
        .execute(VerifyCheckoutCommand {
            checkout_session_id: checkout_id,
            tenant_id,
        })
        .await
        .expect("first verification should succeed");
    assert_eq!(first.checkout_status, CheckoutStatus::Completed);

    let second = use_case
        .execute(VerifyCheckoutCommand {
            checkout_session_id: checkout_id,
            tenant_id,
        })
        .await
        .expect("second verification should succeed");
    assert_eq!(second.checkout_status, CheckoutStatus::Completed);

    let subscription = subscriptions
        .find_by_tenant_id(tenant_id)
        .await
        .expect("lookup should succeed")
        .expect("subscription should exist");
    assert_eq!(subscription.plan_tier, PlanTier::Enterprise);
    assert_eq!(subscription.status, SubscriptionStatus::Active);
}
