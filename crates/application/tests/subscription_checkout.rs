//! Task 6 — subscription checkout use case tests.
//!
//! Covers: server-side price lookup, USD-monthly rejection (no native
//! recurring), authenticated-email propagation, calendar period math,
//! amount/currency mismatch rejection, idempotency, tenant isolation.

use std::sync::{Arc, Mutex};

use application::ports::{
    ActivationOutcome, SubscriptionAction, SubscriptionActivationError,
    SubscriptionActivationRepository, SubscriptionActivationResult,
};
use application::ports::{
    BillingRecordStatus, CheckoutDetails, CheckoutRequest, CheckoutSessionRepository,
    InsertBillingRecordOutcome, PaymentProvider, PaymentProviderResult, PaymentVerification,
    PlanPriceRepository, SubscriptionBillingRecord, SubscriptionBillingRecordRepository,
    SubscriptionCheckoutRequest, SubscriptionPaymentVerification, SubscriptionRepository,
};
use application::use_cases::initiate_subscription_checkout::{
    InitiateSubscriptionCheckoutCommand, InitiateSubscriptionCheckoutError,
    InitiateSubscriptionCheckoutUseCase,
};
use application::use_cases::verify_subscription_checkout::{
    VerifySubscriptionCheckoutCommand, VerifySubscriptionCheckoutError,
    VerifySubscriptionCheckoutOutcome, VerifySubscriptionCheckoutUseCase,
};
use async_trait::async_trait;
use chrono::Utc;
use domain::Subscription;
use domain::enums::{CheckoutStatus, PlanTier, SubscriptionStatus};
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::SubscriptionId;
use domain::shared::ids::TenantId;
use infra::{
    InMemoryCheckoutSessionRepository, InMemoryPlanPriceRepository,
    InMemorySubscriptionActivationRepository, InMemorySubscriptionBillingRecordRepository,
    InMemorySubscriptionRepository,
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

    async fn verify_payment(&self, _reference: &str) -> PaymentProviderResult<PaymentVerification> {
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
    records: Arc<InMemorySubscriptionBillingRecordRepository>,
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

        let activation = Arc::new(InMemorySubscriptionActivationRepository::new(
            subs.clone(),
            records.clone(),
            checkouts.clone(),
        ));

        let initiate = InitiateSubscriptionCheckoutUseCase::new(
            plan_prices.clone() as Arc<dyn PlanPriceRepository>,
            checkouts.clone() as Arc<dyn CheckoutSessionRepository>,
            provider.clone() as Arc<dyn PaymentProvider>,
        );
        let verify = VerifySubscriptionCheckoutUseCase::new(
            checkouts.clone() as Arc<dyn CheckoutSessionRepository>,
            subs.clone() as Arc<dyn SubscriptionRepository>,
            provider.clone() as Arc<dyn PaymentProvider>,
            activation.clone() as Arc<dyn SubscriptionActivationRepository>,
        );

        Self {
            checkouts,
            subs,
            records,
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
    let _ = h
        .initiate_ngn_monthly(tenant(), PlanTier::Starter)
        .await
        .unwrap();
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
        sub_after_first.current_period_ends_at, sub_after_second.current_period_ends_at,
        "second verify must not extend the period"
    );
}

#[tokio::test]
async fn verify_rejects_cross_tenant_session() {
    let h = Harness::new();
    let owner = tenant();
    let attacker = tenant();
    let outcome = h
        .initiate_ngn_monthly(owner, PlanTier::Starter)
        .await
        .unwrap();
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

// ─── BE-1 F3 — idempotent checkout recovery ───────────────────────

/// Seed a billing record with a fresh random `subscription_id`. The port
/// will correct the link (Step 4b) if the request succeeds.
async fn seed_billing_record<R>(
    repo: &R,
    tenant: TenantId,
    status: BillingRecordStatus,
    reference: &str,
) where
    R: SubscriptionBillingRecordRepository + ?Sized,
{
    seed_billing_record_with_sub_id(repo, tenant, status, reference, uuid::Uuid::new_v4()).await;
}

/// Seed a billing record with an explicit `subscription_id` (used when
/// the test wants the port's `Succeeded` path to find a specific
/// existing subscription).
async fn seed_billing_record_with_sub_id<R>(
    repo: &R,
    tenant: TenantId,
    status: BillingRecordStatus,
    reference: &str,
    subscription_id: uuid::Uuid,
) where
    R: SubscriptionBillingRecordRepository + ?Sized,
{
    let record = SubscriptionBillingRecord {
        id: uuid::Uuid::new_v4(),
        subscription_id,
        tenant_id: tenant.0,
        plan_tier: PlanTier::Starter,
        billing_interval: BillingInterval::Monthly,
        currency: Currency::Ngn,
        amount_minor: 800_000,
        status,
        provider: "fake".to_owned(),
        provider_reference: reference.to_owned(),
        provider_payment_id: Some("pay-existing".to_owned()),
        provider_subscription_code: None,
        period_start: Utc::now(),
        period_end: Utc::now() + chrono::Duration::days(30),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let outcome = repo.try_insert(&record).await.unwrap();
    assert_eq!(outcome, InsertBillingRecordOutcome::Inserted);
}

#[tokio::test]
async fn pending_billing_record_retries_activation() {
    let h = Harness::new();
    let t = tenant();

    seed_billing_record(&*h.records, t, BillingRecordStatus::Pending, "ref-1").await;

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
    assert_eq!(sub.status, SubscriptionStatus::Active);
    assert_eq!(sub.plan_tier, PlanTier::Starter);

    let rec = h
        .records
        .find_by_provider_reference("fake", "ref-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.status, BillingRecordStatus::Succeeded);
    // BE-1 F5 — Step 4b corrects the dangling subscription_id from the seed
    // to point at the actual persisted subscription.
    assert_eq!(
        rec.subscription_id, sub.id.0,
        "billing record's subscription_id must be relinked to the persisted subscription"
    );
}

#[tokio::test]
async fn succeeded_billing_record_is_idempotent() {
    let h = Harness::new();
    let t = tenant();

    let existing_sub = Subscription::new_with_period(
        SubscriptionId::new(),
        t,
        PlanTier::Starter,
        Utc::now() + chrono::Duration::days(30),
    );
    h.subs.create(&existing_sub).await.unwrap();
    // BE-1 F5 — the record must reference the real subscription so the
    // port's strengthened Succeeded check finds it.
    seed_billing_record_with_sub_id(
        &*h.records,
        t,
        BillingRecordStatus::Succeeded,
        "ref-1",
        existing_sub.id.0,
    )
    .await;
    let original_period_end = existing_sub.current_period_ends_at;

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

    let sub_after = h.subs.find_by_tenant_id(t).await.unwrap().unwrap();
    assert_eq!(sub_after.id, existing_sub.id);
    assert_eq!(sub_after.current_period_ends_at, original_period_end);
}

#[tokio::test]
async fn succeeded_record_referencing_missing_subscription_is_rejected() {
    let h = Harness::new();
    let t = tenant();

    // Record says Succeeded but points at a subscription_id that does not
    // exist. Port must reject rather than complete the session.
    seed_billing_record(&*h.records, t, BillingRecordStatus::Succeeded, "ref-1").await;

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Invalid(m)
            if m.contains("references a missing subscription")),
        "expected Invalid(missing-subscription), got {err:?}"
    );

    // Session stays Pending, no subscription created.
    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());
    let session = h
        .checkouts
        .find_by_id(outcome.checkout_session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.status, CheckoutStatus::Pending);
}

#[tokio::test]
async fn succeeded_record_referencing_canceled_subscription_is_rejected() {
    let h = Harness::new();
    let t = tenant();

    // Canceled subscription referenced by a Succeeded record.
    let mut canceled_sub = Subscription::new_with_period(
        SubscriptionId::new(),
        t,
        PlanTier::Starter,
        Utc::now() + chrono::Duration::days(30),
    );
    canceled_sub.cancel(false, None).unwrap();
    h.subs.create(&canceled_sub).await.unwrap();
    seed_billing_record_with_sub_id(
        &*h.records,
        t,
        BillingRecordStatus::Succeeded,
        "ref-1",
        canceled_sub.id.0,
    )
    .await;

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    // BE-1 F5 — the use case's domain transition
    // (`apply_verified_payment_with_period`) rejects Canceled subscriptions
    // BEFORE the port is reached, so the error is
    // `Subscription(InvalidSubscriptionTransition(...))` — a 409-class
    // rejection — rather than the port's `SucceededSubscriptionTerminal`.
    // Both are correct: the customer cannot reactivate a canceled
    // subscription. This test asserts the domain-level rejection that fires
    // first in production.
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Subscription(_)),
        "expected Subscription(domain-level rejection of canceled subscription), got {err:?}"
    );

    let session = h
        .checkouts
        .find_by_id(outcome.checkout_session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.status, CheckoutStatus::Pending);
}

#[tokio::test]
async fn succeeded_record_referencing_expired_subscription_is_rejected() {
    let h = Harness::new();
    let t = tenant();

    // Expired subscription referenced by a Succeeded record.
    let mut expired_sub = Subscription::new_trialing(
        SubscriptionId::new(),
        t,
        PlanTier::Starter,
        Utc::now() + chrono::Duration::days(14),
    );
    expired_sub.expire().unwrap();
    h.subs.create(&expired_sub).await.unwrap();
    seed_billing_record_with_sub_id(
        &*h.records,
        t,
        BillingRecordStatus::Succeeded,
        "ref-1",
        expired_sub.id.0,
    )
    .await;

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Invalid(m)
            if m.contains("terminal state")),
        "expected Invalid(terminal), got {err:?}"
    );

    let session = h
        .checkouts
        .find_by_id(outcome.checkout_session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.status, CheckoutStatus::Pending);
}

#[tokio::test]
async fn failed_billing_record_is_not_reactivated() {
    let h = Harness::new();
    let t = tenant();
    seed_billing_record(&*h.records, t, BillingRecordStatus::Failed, "ref-1").await;

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Invalid(m) if m.contains("failed")),
        "expected Invalid(failed-state), got {err:?}"
    );

    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());
    let session = h
        .checkouts
        .find_by_id(outcome.checkout_session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.status, CheckoutStatus::Pending);
}

#[tokio::test]
async fn mismatched_tenant_billing_record_is_rejected() {
    let h = Harness::new();
    let t = tenant();
    let other_tenant = tenant();

    seed_billing_record(
        &*h.records,
        other_tenant,
        BillingRecordStatus::Pending,
        "ref-1",
    )
    .await;

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Invalid(m)
            if m.contains("different tenant")),
        "expected Invalid(tenant-mismatch), got {err:?}"
    );

    assert!(h.subs.find_by_tenant_id(t).await.unwrap().is_none());
}

#[tokio::test]
async fn mismatched_amount_billing_record_is_rejected() {
    let h = Harness::new();
    let t = tenant();

    let bad_record = SubscriptionBillingRecord {
        id: uuid::Uuid::new_v4(),
        subscription_id: uuid::Uuid::new_v4(),
        tenant_id: t.0,
        plan_tier: PlanTier::Starter,
        billing_interval: BillingInterval::Monthly,
        currency: Currency::Ngn,
        amount_minor: 999,
        status: BillingRecordStatus::Pending,
        provider: "fake".to_owned(),
        provider_reference: "ref-1".to_owned(),
        provider_payment_id: Some("pay-other".to_owned()),
        provider_subscription_code: None,
        period_start: Utc::now(),
        period_end: Utc::now() + chrono::Duration::days(30),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    h.records.try_insert(&bad_record).await.unwrap();

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    let err = h
        .verify
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Invalid(m)
            if m.contains("does not match") && m.contains("amount_minor")),
        "expected Invalid(amount-mismatch), got {err:?}"
    );
}

// ─── BE-1 F5 — port-level error classification ───────────────────

#[derive(Clone, Copy)]
enum FakeErrorKind {
    BillingRecordMissingAfterConflict,
    BillingRecordLinkFailed,
}

#[derive(Clone)]
struct FakeActivationRepository {
    kind: FakeErrorKind,
}

#[async_trait]
impl SubscriptionActivationRepository for FakeActivationRepository {
    async fn activate_atomically(
        &self,
        _record: SubscriptionBillingRecord,
        _action: SubscriptionAction,
        _session_id: domain::shared::ids::CheckoutSessionId,
    ) -> SubscriptionActivationResult<ActivationOutcome> {
        Err(match self.kind {
            FakeErrorKind::BillingRecordMissingAfterConflict => {
                SubscriptionActivationError::BillingRecordMissingAfterConflict
            }
            FakeErrorKind::BillingRecordLinkFailed => {
                SubscriptionActivationError::BillingRecordLinkFailed
            }
        })
    }
}

async fn execute_with_fake_port(
    h: &Harness,
    t: TenantId,
    kind: FakeErrorKind,
) -> Result<VerifySubscriptionCheckoutOutcome, VerifySubscriptionCheckoutError> {
    let fake = Arc::new(FakeActivationRepository { kind });
    let use_case = VerifySubscriptionCheckoutUseCase::new(
        h.checkouts.clone() as Arc<dyn CheckoutSessionRepository>,
        h.subs.clone() as Arc<dyn SubscriptionRepository>,
        h.provider.clone() as Arc<dyn PaymentProvider>,
        fake as Arc<dyn SubscriptionActivationRepository>,
    );

    let outcome = h.initiate_ngn_monthly(t, PlanTier::Starter).await.unwrap();
    h.provider.set_verify(paid(800_000, Currency::Ngn));

    use_case
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id: t,
            checkout_session_id: outcome.checkout_session_id,
        })
        .await
}

#[tokio::test]
async fn client_error_from_port_maps_to_invalid() {
    let h = Harness::new();
    let t = tenant();
    let err = execute_with_fake_port(&h, t, FakeErrorKind::BillingRecordMissingAfterConflict)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Invalid(m)
            if m.contains("already existing but not found")),
        "client-class error must map to Invalid (400-class), got {err:?}"
    );
}

#[tokio::test]
async fn server_error_from_port_maps_to_repository() {
    let h = Harness::new();
    let t = tenant();
    let err = execute_with_fake_port(&h, t, FakeErrorKind::BillingRecordLinkFailed)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, VerifySubscriptionCheckoutError::Repository(_)),
        "server-class error must map to Repository (500-class), got {err:?}"
    );
}
