use crate::commands::{InitiateCheckoutCommand, InitiateCheckoutOutcome};
use crate::ports::{CheckoutRequest, CheckoutSessionRepository, PaymentProvider};
use crate::use_cases::billing_support::BillingError;
use domain::CheckoutSession;
use domain::enums::PlanTier;
use domain::shared::ids::CheckoutSessionId;
use rust_decimal::Decimal;

pub struct InitiateCheckoutUseCase<R, P> {
    checkout_session_repository: R,
    payment_provider: P,
}

impl<R, P> InitiateCheckoutUseCase<R, P>
where
    R: CheckoutSessionRepository,
    P: PaymentProvider,
{
    pub fn new(checkout_session_repository: R, payment_provider: P) -> Self {
        Self { checkout_session_repository, payment_provider }
    }

    pub async fn execute(
        &self,
        command: InitiateCheckoutCommand,
    ) -> Result<InitiateCheckoutOutcome, BillingError> {
        if command.plan_tier == PlanTier::Free {
            return Err(BillingError::FreePlanNotAllowed);
        }
        let checkout = self.payment_provider.create_checkout(&CheckoutRequest {
            tenant_id: command.tenant_id, plan_tier: command.plan_tier, amount_minor: 0, currency: "USD".to_owned(),
        }).await?;
        let mut session = CheckoutSession::new_pending(CheckoutSessionId::new(), command.tenant_id, command.plan_tier, self.payment_provider.provider_name().to_owned());
        session.attach_provider_checkout_id(checkout.provider_checkout_id.clone());
        self.checkout_session_repository.create(&session).await?;
        Ok(InitiateCheckoutOutcome { checkout_session_id: session.id, checkout_url: checkout.checkout_url })
    }

    /// Production API path: amount is calculated from the server-side pricing
    /// configuration before the provider is called. The client never supplies
    /// an amount.
    pub async fn execute_with_amount(
        &self,
        command: InitiateCheckoutCommand,
        amount_minor: i64,
        currency: String,
        amount: Decimal,
    ) -> Result<InitiateCheckoutOutcome, BillingError> {
        if command.plan_tier == PlanTier::Free {
            return Err(BillingError::FreePlanNotAllowed);
        }
        if amount_minor <= 0 || amount <= Decimal::ZERO || currency.trim().is_empty() {
            return Err(BillingError::Provider(
                std::io::Error::other("invalid server-calculated checkout amount").into(),
            ));
        }
        let checkout = self.payment_provider.create_checkout(&CheckoutRequest {
            tenant_id: command.tenant_id,
            plan_tier: command.plan_tier,
            amount_minor,
            currency: currency.clone(),
        }).await?;
        let mut session = CheckoutSession::new_pending(
            CheckoutSessionId::new(), command.tenant_id, command.plan_tier,
            self.payment_provider.provider_name().to_owned(),
        );
        session.attach_provider_checkout_id(checkout.provider_checkout_id.clone());
        session.amount = Some(amount);
        session.currency = Some(currency);
        self.checkout_session_repository.create(&session).await?;
        Ok(InitiateCheckoutOutcome { checkout_session_id: session.id, checkout_url: checkout.checkout_url })
    }
}

