use application::{
    ChangeSubscriptionPlanUseCase,
    ports::{
        CheckoutDetails, CheckoutRequest, CheckoutSessionRepository, PaymentProvider,
        PaymentVerification, SubscriptionPlanChangeRepository, SubscriptionRepository,
        SubscriptionUsagePeriodRepository, WalletRepository,
    },
};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use domain::{
    CheckoutStatus, PlanTier, Subscription, SubscriptionId, SubscriptionPlanChangeStatus,
    SubscriptionUsagePeriod, SubscriptionUsagePeriodId, shared::ids::TenantId,
};
use infra::{
    InMemoryCheckoutSessionRepository, InMemoryPricingConfigRepository,
    InMemorySubscriptionPlanChangeRepository, InMemorySubscriptionRepository,
    InMemorySubscriptionUsagePeriodRepository, InMemorySubscriptionUsageTransaction,
    InMemoryWalletRepository, UuidIdGenerator,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
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
                "plan-change-{}",
                request
                    .plan_tier
                    .as_ref()
                    .map(|t| t.as_str())
                    .unwrap_or("addon")
            ),
            checkout_url: "https://checkout.test.invalid/plan-change".to_owned(),
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

struct Fixture {
    tenant_id: TenantId,
    subscription: Subscription,
    subscriptions: InMemorySubscriptionRepository,
    pricing: InMemoryPricingConfigRepository,
    changes: InMemorySubscriptionPlanChangeRepository,
    sessions: InMemoryCheckoutSessionRepository,
    periods: InMemorySubscriptionUsagePeriodRepository,
    wallet: InMemoryWalletRepository,
    tx: InMemorySubscriptionUsageTransaction,
}

impl Fixture {
    async fn new(old_plan: PlanTier) -> Self {
        let tenant_id = TenantId::new();

        let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, old_plan);

        let pricing = infra::InMemoryPricingConfigRepository::new()
            .with_config(domain::pricing::PricingConfig::default_config());

        let end = Utc::now() + Duration::days(15);
        subscription.current_period_ends_at = Some(end);

        let subscriptions = InMemorySubscriptionRepository::new();
        subscriptions
            .create(&subscription)
            .await
            .expect("subscription creation should succeed");

        let periods = InMemorySubscriptionUsagePeriodRepository::new();

        let initial_period = SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            subscription.id,
            100,
            60,
            30,
            subscription.created_at,
        );

        periods
            .create_period(&initial_period)
            .await
            .expect("initial usage period should be created");

        let wallet = InMemoryWalletRepository::default();
        wallet
            .get_or_create(tenant_id.0, "USD")
            .await
            .expect("wallet should exist");

        let tx_subscriptions = subscriptions.clone();
        let tx_periods = periods.clone();

        Self {
            tenant_id,
            subscription,
            subscriptions,
            pricing,
            changes: InMemorySubscriptionPlanChangeRepository::new(),
            sessions: InMemoryCheckoutSessionRepository::new(),
            periods,
            wallet,
            tx: InMemorySubscriptionUsageTransaction::new(
                Arc::new(tx_subscriptions),
                Arc::new(tx_periods),
            ),
        }
    }

    fn use_case(
        &self,
        provider: TestBillingProvider,
    ) -> ChangeSubscriptionPlanUseCase<InMemorySubscriptionUsageTransaction> {
        ChangeSubscriptionPlanUseCase::new(
            Arc::new(self.subscriptions.clone()),
            Arc::new(self.pricing.clone()),
            Arc::new(self.changes.clone()),
            Arc::new(self.sessions.clone()),
            Arc::new(provider),
            Arc::new(self.wallet.clone()),
            Arc::new(self.periods.clone()),
            self.tx.clone(),
            Arc::new(UuidIdGenerator::new()),
        )
    }
}

#[tokio::test]
async fn paid_plan_change_creates_pending_checkout_and_persists_proration() {
    let f = Fixture::new(PlanTier::Starter).await;
    let use_case = f.use_case(TestBillingProvider::new(TestVerification::Paid));

    let outcome = use_case
        .execute(f.tenant_id, PlanTier::Enterprise)
        .await
        .expect("plan change should succeed");

    assert!(!outcome.applied);
    assert!(outcome.checkout_session_id.is_some());
    assert!(outcome.checkout_url.is_some());
    assert!(outcome.calculated_amount > rust_decimal::Decimal::ZERO);

    let change = f
        .changes
        .find_by_id(outcome.plan_change_id, f.tenant_id.0)
        .await
        .expect("plan change lookup should succeed")
        .expect("plan change should exist");

    assert_eq!(change.old_plan, PlanTier::Starter);
    assert_eq!(change.new_plan, PlanTier::Enterprise);
    assert_eq!(change.status, SubscriptionPlanChangeStatus::Pending);
    assert_eq!(change.pricing_version, 1);
    assert_eq!(change.currency, "USD");

    let subscription = f
        .subscriptions
        .find_by_tenant_id(f.tenant_id)
        .await
        .expect("subscription lookup should succeed")
        .expect("subscription should exist");

    assert_eq!(subscription.plan_tier, PlanTier::Starter);

    let session_id = outcome
        .checkout_session_id
        .expect("checkout session expected");
    let session = f
        .sessions
        .find_by_id(domain::CheckoutSessionId(session_id))
        .await
        .expect("checkout lookup should succeed")
        .expect("checkout should exist");

    assert_eq!(session.status, CheckoutStatus::Pending);
    assert_eq!(session.plan_change_id, Some(outcome.plan_change_id));
}

#[tokio::test]
async fn verified_paid_plan_change_updates_subscription_and_usage_period() {
    let f = Fixture::new(PlanTier::Starter).await;
    let use_case = f.use_case(TestBillingProvider::new(TestVerification::Paid));

    let created = use_case
        .execute(f.tenant_id, PlanTier::Enterprise)
        .await
        .expect("plan change should succeed");

    let session_id = created
        .checkout_session_id
        .expect("paid plan change should create checkout");

    let applied = use_case
        .apply_verified_change(f.tenant_id, session_id)
        .await
        .expect("verified payment should apply");

    assert!(applied.applied);
    assert_eq!(applied.plan_change_id, created.plan_change_id);

    let subscription = f
        .subscriptions
        .find_by_tenant_id(f.tenant_id)
        .await
        .expect("subscription lookup should succeed")
        .expect("subscription should exist");

    assert_eq!(subscription.plan_tier, PlanTier::Enterprise);
    assert_eq!(subscription.status, domain::SubscriptionStatus::Active);
    assert_eq!(subscription.pricing_version, Some(1));

    let change = f
        .changes
        .find_by_id(created.plan_change_id, f.tenant_id.0)
        .await
        .expect("plan change lookup should succeed")
        .expect("plan change should exist");

    assert_eq!(change.status, SubscriptionPlanChangeStatus::Completed);
    assert!(change.completed_at.is_some());

    let session = f
        .sessions
        .find_by_id(domain::CheckoutSessionId(session_id))
        .await
        .expect("checkout lookup should succeed")
        .expect("checkout should exist");

    assert_eq!(session.status, CheckoutStatus::Completed);

    let open_period = f
        .periods
        .find_open_period(f.subscription.id)
        .await
        .expect("usage period lookup should succeed")
        .expect("new usage period should exist");

    let enterprise = domain::Plan::for_tier(PlanTier::Enterprise);

    assert_eq!(open_period.monitors, enterprise.max_monitors as u64);
    assert_eq!(
        open_period.interval_seconds,
        enterprise.min_check_interval_seconds
    );
    assert_eq!(
        open_period.retention_days,
        enterprise.history_retention_days
    );
}

#[tokio::test]
async fn pending_payment_does_not_change_subscription() {
    let f = Fixture::new(PlanTier::Starter).await;
    let use_case = f.use_case(TestBillingProvider::new(TestVerification::Pending));

    let created = use_case
        .execute(f.tenant_id, PlanTier::Enterprise)
        .await
        .expect("plan change should succeed");

    let session_id = created
        .checkout_session_id
        .expect("checkout session should exist");

    let result = use_case
        .apply_verified_change(f.tenant_id, session_id)
        .await
        .expect("pending verification should not fail");

    assert!(!result.applied);

    let subscription = f
        .subscriptions
        .find_by_tenant_id(f.tenant_id)
        .await
        .expect("subscription lookup should succeed")
        .expect("subscription should exist");

    assert_eq!(subscription.plan_tier, PlanTier::Starter);

    let change = f
        .changes
        .find_by_id(created.plan_change_id, f.tenant_id.0)
        .await
        .expect("plan change lookup should succeed")
        .expect("plan change should exist");

    assert_eq!(change.status, SubscriptionPlanChangeStatus::Pending);
}

#[tokio::test]
async fn failed_payment_marks_plan_change_failed_without_changing_subscription() {
    let f = Fixture::new(PlanTier::Starter).await;
    let use_case = f.use_case(TestBillingProvider::new(TestVerification::Failed));

    let created = use_case
        .execute(f.tenant_id, PlanTier::Enterprise)
        .await
        .expect("plan change should succeed");

    let session_id = created
        .checkout_session_id
        .expect("checkout session should exist");

    let result = use_case
        .apply_verified_change(f.tenant_id, session_id)
        .await;

    assert!(result.is_err());

    let subscription = f
        .subscriptions
        .find_by_tenant_id(f.tenant_id)
        .await
        .expect("subscription lookup should succeed")
        .expect("subscription should exist");

    assert_eq!(subscription.plan_tier, PlanTier::Starter);

    let change = f
        .changes
        .find_by_id(created.plan_change_id, f.tenant_id.0)
        .await
        .expect("plan change lookup should succeed")
        .expect("plan change should exist");

    assert_eq!(change.status, SubscriptionPlanChangeStatus::Failed);

    let session = f
        .sessions
        .find_by_id(domain::CheckoutSessionId(session_id))
        .await
        .expect("checkout lookup should succeed")
        .expect("checkout should exist");

    assert_eq!(session.status, CheckoutStatus::Failed);
}

#[tokio::test]
async fn repeated_verified_payment_is_idempotent_after_completion() {
    let f = Fixture::new(PlanTier::Starter).await;
    let use_case = f.use_case(TestBillingProvider::new(TestVerification::Paid));

    let created = use_case
        .execute(f.tenant_id, PlanTier::Enterprise)
        .await
        .expect("plan change should succeed");

    let session_id = created
        .checkout_session_id
        .expect("checkout session should exist");

    let first = use_case
        .apply_verified_change(f.tenant_id, session_id)
        .await
        .expect("first verification should succeed");

    let second = use_case
        .apply_verified_change(f.tenant_id, session_id)
        .await
        .expect("second verification should succeed");

    assert!(first.applied);
    assert!(second.applied);
    assert_eq!(first.plan_change_id, second.plan_change_id);

    let subscription = f
        .subscriptions
        .find_by_tenant_id(f.tenant_id)
        .await
        .expect("subscription lookup should succeed")
        .expect("subscription should exist");

    assert_eq!(subscription.plan_tier, PlanTier::Enterprise);
}
