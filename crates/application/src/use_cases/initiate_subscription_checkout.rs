//! Task 6 — initiate a subscription checkout.
//!
//! The amount is loaded from the server-side `plan_prices` table; the
//! client never supplies it. Task 6 scope restricts USD to annual: USD
//! monthly is rejected until native recurring is verified for the
//! merchant account.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use domain::CheckoutSession;
use domain::enums::PlanTier;
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::{CheckoutSessionId, TenantId};

use crate::ports::{
    CheckoutSessionRepository, PaymentProvider, PlanPriceRepository, SubscriptionCheckoutRequest,
};

#[derive(Debug, Clone)]
pub struct InitiateSubscriptionCheckoutCommand {
    pub tenant_id: TenantId,
    pub plan_tier: PlanTier,
    pub currency: Currency,
    pub billing_interval: BillingInterval,
    /// From the authenticated user's account record, never the
    /// frontend request body.
    pub customer_email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiateSubscriptionCheckoutOutcome {
    pub checkout_session_id: CheckoutSessionId,
    pub checkout_url: String,
    pub amount_minor: i64,
    pub currency: Currency,
    pub billing_interval: BillingInterval,
}

#[derive(Debug)]
pub enum InitiateSubscriptionCheckoutError {
    FreePlanNotAllowed,
    EnterpriseIsContactSalesOnly,
    /// Requested combination requires native recurring billing that
    /// has not been verified for the merchant account.
    NativeRecurringUnavailable(String),
    PriceNotFound,
    Invalid(String),
    Provider(Box<dyn Error + Send + Sync>),
    Repository(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for InitiateSubscriptionCheckoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FreePlanNotAllowed => write!(f, "the free plan does not require checkout"),
            Self::EnterpriseIsContactSalesOnly => {
                write!(f, "Enterprise is by custom quote only")
            }
            Self::NativeRecurringUnavailable(msg) => {
                write!(f, "native recurring billing is not available: {msg}")
            }
            Self::PriceNotFound => write!(
                f,
                "no effective price configured for this tier / currency / interval"
            ),
            Self::Invalid(msg) => write!(f, "{msg}"),
            Self::Provider(e) => write!(f, "provider error: {e}"),
            Self::Repository(e) => write!(f, "repository error: {e}"),
        }
    }
}

impl Error for InitiateSubscriptionCheckoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(e) | Self::Repository(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct InitiateSubscriptionCheckoutUseCase {
    plan_price_repository: Arc<dyn PlanPriceRepository>,
    checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
    payment_provider: Arc<dyn PaymentProvider>,
}

impl InitiateSubscriptionCheckoutUseCase {
    pub fn new(
        plan_price_repository: Arc<dyn PlanPriceRepository>,
        checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
        payment_provider: Arc<dyn PaymentProvider>,
    ) -> Self {
        Self {
            plan_price_repository,
            checkout_session_repository,
            payment_provider,
        }
    }

    pub async fn execute(
        &self,
        command: InitiateSubscriptionCheckoutCommand,
    ) -> Result<InitiateSubscriptionCheckoutOutcome, InitiateSubscriptionCheckoutError> {
        if command.plan_tier == PlanTier::Free {
            return Err(InitiateSubscriptionCheckoutError::FreePlanNotAllowed);
        }
        if command.plan_tier == PlanTier::Enterprise {
            return Err(InitiateSubscriptionCheckoutError::EnterpriseIsContactSalesOnly);
        }
        if command.currency == Currency::Usd
            && command.billing_interval == BillingInterval::Monthly
        {
            return Err(InitiateSubscriptionCheckoutError::NativeRecurringUnavailable(
                "USD monthly recurring is not available on this merchant account; \
                 use USD annual or NGN"
                    .to_owned(),
            ));
        }

        let email = command.customer_email.trim();
        if email.is_empty() {
            return Err(InitiateSubscriptionCheckoutError::Invalid(
                "authenticated customer email is required".to_owned(),
            ));
        }

        let price = self
            .plan_price_repository
            .find_effective(
                command.plan_tier,
                command.currency,
                command.billing_interval,
            )
            .await
            .map_err(InitiateSubscriptionCheckoutError::Repository)?
            .ok_or(InitiateSubscriptionCheckoutError::PriceNotFound)?;

        if price.amount.amount_minor <= 0 {
            return Err(InitiateSubscriptionCheckoutError::PriceNotFound);
        }

        let reference = format!("pv-sub-{}", uuid::Uuid::new_v4().simple());
        let details = self
            .payment_provider
            .create_subscription_checkout(&SubscriptionCheckoutRequest {
                tenant_id: command.tenant_id,
                plan_tier: command.plan_tier,
                billing_interval: command.billing_interval,
                amount_minor: price.amount.amount_minor,
                currency: price.amount.currency,
                customer_email: email.to_owned(),
                reference,
            })
            .await
            .map_err(InitiateSubscriptionCheckoutError::Provider)?;

        let mut session = CheckoutSession::new_pending(
            CheckoutSessionId::new(),
            command.tenant_id,
            Some(command.plan_tier),
            self.payment_provider.provider_name().to_owned(),
        );
        session.attach_subscription_billing(
            command.billing_interval,
            price.amount.amount_minor,
            price.amount.currency,
        );
        session.attach_provider_checkout_id(details.provider_checkout_id);
        self.checkout_session_repository
            .create(&session)
            .await
            .map_err(InitiateSubscriptionCheckoutError::Repository)?;

        Ok(InitiateSubscriptionCheckoutOutcome {
            checkout_session_id: session.id,
            checkout_url: details.checkout_url,
            amount_minor: price.amount.amount_minor,
            currency: price.amount.currency,
            billing_interval: command.billing_interval,
        })
    }
}
