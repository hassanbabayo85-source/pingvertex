use std::error::Error;

use crate::ports::StatusPageRepository;
use domain::{UserId, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct ListStatusPagesUseCase<R> {
    repository: R,
}

impl<R> ListStatusPagesUseCase<R>
where
    R: StatusPageRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, user_id: UserId) -> UseCaseResult<Vec<StatusPage>> {
        self.repository.find_all_for_user(user_id).await
    }
}
