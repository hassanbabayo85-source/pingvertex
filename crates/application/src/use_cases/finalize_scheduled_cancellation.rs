use crate::commands::FinalizeScheduledCancellation;
use crate::ports::SubscriptionRepository;
use crate::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;

pub struct FinalizeScheduledCancellationUseCase<R> {
    repository: R,
}

impl<R> FinalizeScheduledCancellationUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        command: FinalizeScheduledCancellation,
    ) -> Result<(), SubscriptionLifecycleError> {
        let mut subscription = self
            .repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .ok_or(SubscriptionLifecycleError::SubscriptionNotFound)?;

        subscription.finalize_scheduled_cancellation()?;

        self.repository.update(&subscription).await?;
        Ok(())
    }
}
