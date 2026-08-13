use std::{error::Error, sync::Arc};

use crate::ports::CheckRepository;
use domain::Check;
use domain::shared::ids::UserId;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all checks belonging to the authenticated user.
#[derive(Clone)]
pub struct ListChecksUseCase {
    repository: Arc<dyn CheckRepository>,
}

impl ListChecksUseCase {
    pub fn new(repository: Arc<dyn CheckRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        limit: u32,
        offset: u32,
        user_id: UserId,
    ) -> UseCaseResult<Vec<Check>> {
        self.repository.list_checks(limit, offset, user_id).await
    }
}
