use crate::commands::{VerifyCheckoutCommand, VerifyCheckoutOutcome};
use crate::ports::{
    CheckoutSessionRepository, PaymentProvider, PaymentVerification, SubscriptionRepository,
};
use crate::use_cases::billing_support::BillingError;
use crate::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;
use domain::enums::{CheckoutStatus, PlanTier};
use domain::shared::ids::TenantId;
use domain::{Subscription, SubscriptionId};

pub struct VerifyCheckoutUseCase<R, S, P> {
    checkout_session_repository: R,
    subscription_repository: S,
    payment_provider: P,
}

impl<R, S, P> VerifyCheckoutUseCase<R, S, P>
where
    R: CheckoutSessionRepository,
    S: SubscriptionRepository,
    P: PaymentProvider,
{
    pub fn new(
        checkout_session_repository: R,
        subscription_repository: S,
        payment_provider: P,
    ) -> Self {
        Self {
            checkout_session_repository,
            subscription_repository,
            payment_provider,
        }
    }

    pub async fn execute(
        &self,
        command: VerifyCheckoutCommand,
    ) -> Result<VerifyCheckoutOutcome, BillingError> {
        let session = self
            .checkout_session_repository
            .find_by_id(command.checkout_session_id)
            .await?
            .ok_or(BillingError::CheckoutSessionNotFound)?;

        // Tenant isolation: the checkout session must belong to the same
        // tenant that owns the authenticated request. We return NotFound
        // (not Forbidden) to avoid leaking existence of another tenant's
        // session id.
        if session.tenant_id != command.tenant_id {
            return Err(BillingError::CheckoutSessionNotFound);
        }

        // Idempotency: a completed session has already activated the
        // subscription. Never re-run activation for the same session.
        if session.status == CheckoutStatus::Completed {
            return Ok(VerifyCheckoutOutcome {
                checkout_status: CheckoutStatus::Completed,
            });
        }

        let provider_checkout_id = session
            .provider_checkout_id
            .clone()
            .ok_or(BillingError::CheckoutNotReady)?;

        let verification = self
            .payment_provider
            .verify_payment(&provider_checkout_id)
            .await?;

        match verification {
            PaymentVerification::Paid { .. } => {
                self.activate_subscription(session.tenant_id, session.plan_tier)
                    .await?;

                let mut updated = session.clone();
                updated.mark_completed();
                self.checkout_session_repository.update(&updated).await?;

                Ok(VerifyCheckoutOutcome {
                    checkout_status: CheckoutStatus::Completed,
                })
            }
            PaymentVerification::Pending { .. } => Ok(VerifyCheckoutOutcome {
                checkout_status: CheckoutStatus::Pending,
            }),
            PaymentVerification::Failed { .. } => {
                let mut updated = session.clone();
                updated.mark_failed();
                self.checkout_session_repository.update(&updated).await?;

                Ok(VerifyCheckoutOutcome {
                    checkout_status: CheckoutStatus::Failed,
                })
            }
        }
    }

    async fn activate_subscription(
        &self,
        tenant_id: TenantId,
        plan_tier: PlanTier,
    ) -> Result<(), BillingError> {
        match self
            .subscription_repository
            .find_by_tenant_id(tenant_id)
            .await?
        {
            Some(mut subscription) => {
                subscription
                    .apply_verified_payment(plan_tier)
                    .map_err(SubscriptionLifecycleError::from)?;
                self.subscription_repository.update(&subscription).await?;
            }
            None => {
                let subscription = Subscription::new(SubscriptionId::new(), tenant_id, plan_tier);
                self.subscription_repository.create(&subscription).await?;
            }
        }
        Ok(())
    }
}
