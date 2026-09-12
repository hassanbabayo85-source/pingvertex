use crate::commands::BeginGracePeriod;
use crate::ports::SubscriptionRepository;
use crate::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;

pub struct BeginGracePeriodUseCase<R> {
    repository: R,
}

impl<R> BeginGracePeriodUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        command: BeginGracePeriod,
    ) -> Result<(), SubscriptionLifecycleError> {
        let mut subscription = self
            .repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .ok_or(SubscriptionLifecycleError::SubscriptionNotFound)?;

        subscription.begin_grace_period(command.grace_period_ends_at)?;

        self.repository.update(&subscription).await?;
        Ok(())
    }
}
