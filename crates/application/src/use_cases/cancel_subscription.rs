use crate::commands::CancelSubscription;
use crate::ports::SubscriptionRepository;
use crate::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;

pub struct CancelSubscriptionUseCase<R> {
    repository: R,
}

impl<R> CancelSubscriptionUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        command: CancelSubscription,
    ) -> Result<(), SubscriptionLifecycleError> {
        let mut subscription = self
            .repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .ok_or(SubscriptionLifecycleError::SubscriptionNotFound)?;

        // The current period end is authoritative billing state. Never trust a
        // client-provided timestamp for the subscription lifecycle.
        let period_ends_at = if command.at_period_end {
            subscription.current_period_ends_at
        } else {
            None
        };
        subscription.cancel(command.at_period_end, period_ends_at)?;

        self.repository.update(&subscription).await?;
        Ok(())
    }
}
