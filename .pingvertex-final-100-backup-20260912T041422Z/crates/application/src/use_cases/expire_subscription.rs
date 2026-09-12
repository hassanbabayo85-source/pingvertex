use crate::commands::ExpireSubscription;
use crate::ports::SubscriptionRepository;
use crate::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;

pub struct ExpireSubscriptionUseCase<R> {
    repository: R,
}

impl<R> ExpireSubscriptionUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        command: ExpireSubscription,
    ) -> Result<(), SubscriptionLifecycleError> {
        let mut subscription = self
            .repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .ok_or(SubscriptionLifecycleError::SubscriptionNotFound)?;

        subscription.expire()?;

        self.repository.update(&subscription).await?;
        Ok(())
    }
}
