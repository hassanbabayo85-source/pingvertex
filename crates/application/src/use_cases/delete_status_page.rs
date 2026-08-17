use std::error::Error;

use crate::ports::StatusPageRepository;
use domain::{StatusPageId, UserId};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct DeleteStatusPageUseCase<R> {
    repository: R,
}

impl<R> DeleteStatusPageUseCase<R>
where
    R: StatusPageRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Returns `true` if a status page owned by `user_id` was deleted,
    /// `false` if no such status page existed (whether because it was
    /// never created or because it belongs to someone else — the
    /// repository's ownership-scoped query makes those indistinguishable
    /// by design, see the codebase's authorization rule).
    pub async fn execute(&self, id: StatusPageId, user_id: UserId) -> UseCaseResult<bool> {
        self.repository.delete_for_user(id, user_id).await
    }
}
