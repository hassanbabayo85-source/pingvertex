//! Task 6 — subscription checkout use case tests.
//!
//! Covers: server-side price lookup, USD-monthly rejection (no native
//! recurring), authenticated-email propagation, calendar period math,
//! amount/currency mismatch rejection, idempotency, tenant isolation.

use std::sync::{Arc, Mutex};

use application::ports::{
    CheckoutDetails, CheckoutRequest, CheckoutSessionRepository, PaymentProvider,
    PaymentProviderResult, PaymentVerification, PlanPriceRepository,
    SubscriptionBillingRecordRepository, SubscriptionCheckoutRequest,
    SubscriptionPaymentVerification, SubscriptionRepository,
};
use application::use_cases::initiate_subscription_checkout::{
    InitiateSubscriptionCheckoutCommand, InitiateSubscriptionCheckoutError,
    InitiateSubscriptionCheckoutUseCase,
};
use application::use_cases::verify_subscription_checkout::{
    VerifySubscriptionCheckoutCommand, VerifySubscriptionCheckoutError,
    VerifySubscriptionCheckoutUseCase,
};
use async_trait::async_trait;
use chrono::Utc;
use domain::enums::{CheckoutStatus, PlanTier, SubscriptionStatus};
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::TenantId;
use infra::{
    InMemoryCheckoutSessionRepository, InMemoryPlanPriceRepository,
    InMemorySubscriptionBillingRecordRepository, InMemorySubscriptionRepository,
};

// ── Fake payment provider ────────────────────────────────────────────

#[derive(Clone, Default)]
struct FakeProvider {
    captured: Arc<Mutex<Vec<SubscriptionCheckoutRequest>>>,
    verify_response: Arc<Mutex<Option<SubscriptionPaymentVerification>>>,
}

impl FakeProvider {
    fn set_verify(&self, response: SubscriptionPaymentVerification) {
        *self.verify_response.lock().unwrap() = Some(response);
    }

    fn captured(&self) -> Vec<SubscriptionCheckoutRequest> {
        self.captured.lock().unwrap().clone()
    }
}

#[async_trait]
impl PaymentProvider for FakeProvider {
    fn provider_name(&self) -> &'static str {
        "fake"
    }

    async fn create_checkout(
        &self,
        _request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("wallet checkout not used in this test").into())
    }

    async fn verify_payment(
        &self,
        _reference: &str,
    ) -> PaymentProviderResult<PaymentVerification> {
        Err(std::io::Error::other("wallet verify not used in this test").into())
    }

    async fn create_subscription_checkout(
        &self,
        request: &SubscriptionCheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        self.captured.lock().unwrap().push(request.clone());
        Ok(CheckoutDetails {
            provider_checkout_id: request.reference.clone(),
            checkout_url: format!("https://fake.checkout/{}", request.reference),
        })
    }

    async fn verify_payment_minor_units(
        &self,
        _reference: &str,
    ) -> PaymentProviderResult<SubscriptionPaymentVerification> {
        self.verify_response
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| std::io::Error::other("no verify response configured").into())
    }
}

// ── Harness ──────────────────────────────────────────────────────────

struct Harness {
    checkouts: Arc<InMemoryCheckoutSessionRepository>,
    subs: Arc<InMemorySubscriptionRepository>,
    provider: Arc<FakeProvider>,
    initiate: InitiateSubscriptionCheckoutUseCase,
    verify: VerifySubscriptionCheckoutUseCase,
}

impl Harness {
    fn new() -> Self {
        let plan_prices = Arc::new(InMemoryPlanPriceRepository::default());
        let checkouts = Arc::new(InMemoryCheckoutSessionRepository::new());
        let subs = Arc::new(InMemorySubscriptionRepository::new());
        let records = Arc::new(InMemorySubscriptionBillingRecordRepository::new());
        let provider = Arc::new(FakeProvider::default());

        let initiate = InitiateSubscriptionCheckoutUseCase::new(
            plan_prices.clone() as Arc<dyn PlanPriceRepository>,
            checkouts.clone() as Arc<dyn CheckoutSessionRepository>,
            provider.clone() as Arc<dyn PaymentProvider>,
        );
        let verify = VerifySubscriptionCheckoutUseCase::new(
            checkouts.clone() as Arc<dyn CheckoutSessionRepository>,
            subs.clone() as Arc<dyn SubscriptionRepository>,
            records.clone() as Arc<dyn SubscriptionBillingRecordRepository>,
            provider.clone() as Arc<dyn PaymentProvider>,
        );

        Self {
            checkouts,
            subs,
            provider,
            initiate,
            verify,
        }
    }

    async fn initiate_ngn_monthly(
        &self,
        tenant_id: TenantId,
        tier: PlanTier,
    ) -> Result<
        application::use_cases::initiate_subscription_checkout::InitiateSubscriptionCheckoutOutcome,
        InitiateSubscriptionCheckoutError,
    > {
        self.initiate
            .execute(InitiateSubscriptionCheckoutCommand {
                tenant_id,
                plan_tier: tier,
                currency: Currency::Ngn,
                billing_interval: BillingInterval::Monthly,
                customer_email: "auth@example.com".to_owned(),
            })
            .await
    }
}

fn tenant() -> TenantId {
    TenantId(uuid::Uuid::new_v4())
}

fn paid(amount_minor: i64, currency: Currency) -> SubscriptionPaymentVerification {
    SubscriptionPaymentVerification::Paid {
        provider_payment_id: "pay-1".to_owned(),
        provider_reference: "ref-1".to_owned(),
        amount_minor,
        currency,
    }
}

// ── Initiate tests ───────────────────────────────────────────────────

#[tokio::test]
async fn initiate_rejects_free_plan() {
    let h = Harness::new();
    let err = h
        .initiate_ngn_monthly(tenant(), PlanTier::Free)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        InitiateSubscriptionCheckoutError::FreePlanNotAllowed
    ));
    assert!(h.provider.captured().is_empty());
}

#[tokio::test]
async fn initiate_rejects_enterprise() {
    let h = Harness::new();
    let err = h
        .initiate_ngn_monthly(tenant(), PlanTier::Enterprise)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        InitiateSubscriptionCheckoutError::EnterpriseIsContactSalesOnly
    ));
    assert!(h.provider.captured().is_empty());
}

#[tokio::test]
async fn initiate_rejects_usd_monthly_as_native_recurring_unavailable() {
    let h = Harness::new();
    let err = h
        .initiate
        .execute(InitiateSubscriptionCheckoutCommand {
            tenant_id: tenant(),
            plan_tier: PlanTier::Starter,
            currency: Currency::Usd,
            billing_interval: BillingInterval::Monthly,
            customer_email: "auth@example.com".to_owned(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        InitiateSubscriptionCheckoutError::NativeRecurringUnavailable(_)
    ));
    assert!(h.provider.captured().is_empty());
}

#[tokio::test]
async fn initiate_ngn_monthly_loads_price_from_repository() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    // Starter NGN monthly = ₦8,000 = 800,000 kobo (from migration 0125 seed).
    assert_eq!(outcome.amount_minor, 800_000);
    assert_eq!(outcome.currency, Currency::Ngn);
    assert_eq!(outcome.billing_interval, BillingInterval::Monthly);
    assert!(outcome.checkout_url.starts_with("https://fake.checkout/"));
}

#[tokio::test]
async fn initiate_usd_annual_loads_price_from_repository() {
    let h = Harness::new();
    let outcome = h
        .initiate
        .execute(InitiateSubscriptionCheckoutCommand {
            tenant_id: tenant(),
            plan_tier: PlanTier::Pro,
            currency: Currency::Usd,
            billing_interval: BillingInterval::Annual,
            customer_email: "auth@example.com".to_owned(),
        })
        .await
        .unwrap();
    // Pro USD annual = $190.00 = 19,000 cents (from migration 0125 seed).
    assert_eq!(outcome.amount_minor, 19_000);
    assert_eq!(outcome.currency, Currency::Usd);
    assert_eq!(outcome.billing_interval, BillingInterval::Annual);
}

#[tokio::test]
async fn initiate_captures_authenticated_email_in_provider_request() {
    let h = Harness::new();
    let _ = h.initiate_ngn_monthly(tenant(), PlanTier::Starter).await.unwrap();
    let reqs = h.provider.captured();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].customer_email, "auth@example.com");
    assert!(reqs[0].reference.starts_with("pv-sub-"));
}

// ── Verify tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn verify_paid_activates_new_subscription() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let v = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();
    assert_eq!(v.checkout_status, CheckoutStatus::Completed);

    let sub = h.subs.find_by_tenant_id(t).await.unwrap().unwrap();
    assert_eq!(sub.plan_tier, PlanTier::Starter);
    assert_eq!(sub.status, SubscriptionStatus::Active);
    assert!(sub.current_period_ends_at.is_some());
}

#[tokio::test]
async fn verify_paid_monthly_uses_calendar_month_period() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));
    h.verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();

    let sub = h.subs.find_by_tenant_id(t).await.unwrap().unwrap();
    let end = sub.current_period_ends_at.unwrap();
    let days = (end - Utc::now()).num_days();
    assert!(
        (27..=32).contains(&days),
        "monthly period must be ~1 calendar month, got {days} days"
    );
}

#[tokio::test]
async fn verify_paid_annual_uses_twelve_calendar_months() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h
        .initiate
        .execute(InitiateSubscriptionCheckoutCommand {
            tenant_id: t,
            plan_tier: PlanTier::Pro,
            currency: Currency::Usd,
            billing_interval: BillingInterval::Annual,
            customer_email: "auth@example.com".to_owned(),
        })
        .await
        .unwrap();
    h.provider.set_verify(paid(19_000, Currency::Usd));
    h.verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();

    let sub = h.subs.find_by_tenant_id(t).await.unwrap().unwrap();
    let end = sub.current_period_ends_at.unwrap();
    let days = (end - Utc::now()).num_days();
    assert!(
        (360..=370).contains(&days),
        "annual period must be ~12 months, got {days} days"
    );
}

#[tokio::test]
async fn verify_pending_does_not_activate() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider
        .set_verify(SubscriptionPaymentVerification::Pending {
            provider_payment_id: Some("pay-1".to_owned()),
            provider_reference: Some("ref-1".to_owned()),
            amount_minor: Some(800_000),
            currency: Some(Currency::Ngn),
        });

    let v = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();
    assert_eq!(v.checkout_status, CheckoutStatus::Pending);
    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());
}

#[tokio::test]
async fn verify_failed_marks_session_failed_and_does_not_activate() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider
        .set_verify(SubscriptionPaymentVerification::Failed {
            provider_payment_id: Some("pay-1".to_owned()),
            provider_reference: Some("ref-1".to_owned()),
            amount_minor: Some(800_000),
            currency: Some(Currency::Ngn),
        });

    let v = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();
    assert_eq!(v.checkout_status, CheckoutStatus::Failed);
    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());

    let session = h
        .checkouts
        .find_by_id(outcome.checkout_session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.status, CheckoutStatus::Failed);
}

#[tokio::test]
async fn verify_amount_mismatch_returns_error_and_does_not_activate() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    // Attacker tampers with the price: expected 800_000, provider says 1 kobo.
    h.provider.set_verify(paid(1, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        VerifySubscriptionCheckoutError::AmountMismatch { .. }
    ));
    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());
}

#[tokio::test]
async fn verify_currency_mismatch_returns_error() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Usd));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        VerifySubscriptionCheckoutError::CurrencyMismatch { .. }
    ));
    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());
}

#[tokio::test]
async fn verify_duplicate_is_idempotent() {
    let h = Harness::new();
    let t = tenant();
    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    // First verify activates.
    let v1 = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();
    assert_eq!(v1.checkout_status, CheckoutStatus::Completed);
    let sub_after_first = h.subs.find_by_tenant_id(t).await.unwrap().unwrap();

    // Second verify — same session, same result, no state change.
    let v2 = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap();
    assert_eq!(v2.checkout_status, CheckoutStatus::Completed);
    let sub_after_second = h.subs.find_by_tenant_id(t).await.unwrap().unwrap();

    assert_eq!(sub_after_first.id, sub_after_second.id);
    assert_eq!(
        sub_after_first.current_period_ends_at,
        sub_after_second.current_period_ends_at,
        "second verify must not extend the period"
    );
}

#[tokio::test]
async fn verify_rejects_cross_tenant_session() {
    let h = Harness::new();
    let owner = tenant();
    let attacker = tenant();
    let outcome = h.initiate_ngn_monthly(owner, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: attacker,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        VerifySubscriptionCheckoutError::CheckoutSessionNotFound
    ));

    // No subscription is created for the attacker.
    assert!(h.subs.find_by_tenant_id(attacker).await.unwrap().is_none());
    // Owner's subscription is untouched (attacker call must not verify).
    assert!(h.subs.find_by_tenant_id(owner).await.unwrap().is_none());
}
