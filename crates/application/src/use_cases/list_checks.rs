use std::{error::Error, sync::Arc};

use crate::ports::CheckRepository;
use domain::Check;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all checks.
#[derive(Clone)]
pub struct ListChecksUseCase {
    repository: Arc<dyn CheckRepository>,
}

impl ListChecksUseCase {
    pub fn new(repository: Arc<dyn CheckRepository>) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, limit: u32, offset: u32) -> UseCaseResult<Vec<Check>> {
        self.repository.list_checks(limit, offset).await
    }
}
