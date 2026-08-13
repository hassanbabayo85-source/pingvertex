use std::error::Error;

use crate::{commands::DeleteNotificationChannel, ports::NotificationChannelRepository};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Deletes a notification channel through the application layer.
///
/// Ownership is enforced entirely inside `NotificationChannelRepository::delete_for_user`
/// (SQL-level `WHERE id = $1 AND user_id = $2`) — this use case has nothing
/// to add on top of that, it just exists so the handler doesn't call the
/// repository directly, keeping the door open for future business rules
/// (e.g. "don't allow deleting the last channel on a monitor") without
/// touching the handler.
pub struct DeleteNotificationChannelUseCase<R> {
    repository: R,
}

impl<R> DeleteNotificationChannelUseCase<R>
where
    R: NotificationChannelRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, command: DeleteNotificationChannel) -> UseCaseResult<bool> {
        self.repository
            .delete_for_user(command.id, command.user_id)
            .await
    }
}
