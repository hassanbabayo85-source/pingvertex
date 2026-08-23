use crate::commands::{InitiateCheckoutCommand, InitiateCheckoutOutcome};
use crate::ports::{CheckoutRequest, CheckoutSessionRepository, PaymentProvider};
use crate::use_cases::billing_support::BillingError;
use domain::CheckoutSession;
use domain::enums::PlanTier;
use domain::shared::ids::CheckoutSessionId;

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
        Self {
            checkout_session_repository,
            payment_provider,
        }
    }

    pub async fn execute(
        &self,
        command: InitiateCheckoutCommand,
    ) -> Result<InitiateCheckoutOutcome, BillingError> {
        if command.plan_tier == PlanTier::Free {
            return Err(BillingError::FreePlanNotAllowed);
        }

        let checkout = self
            .payment_provider
            .create_checkout(&CheckoutRequest {
                tenant_id: command.tenant_id,
                plan_tier: command.plan_tier,
            })
            .await?;

        let mut session = CheckoutSession::new_pending(
            CheckoutSessionId::new(),
            command.tenant_id,
            command.plan_tier,
            self.payment_provider.provider_name().to_owned(),
        );
        session.attach_provider_checkout_id(checkout.provider_checkout_id.clone());

        self.checkout_session_repository.create(&session).await?;

        Ok(InitiateCheckoutOutcome {
            checkout_session_id: session.id,
            checkout_url: checkout.checkout_url,
        })
    }
}
