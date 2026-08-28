use std::error::Error;

use crate::ports::HelpCategoryRepository;
use domain::entities::HelpCategory;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Returns all help categories ordered by `sort_order`.
pub struct ListHelpCategoriesUseCase<R> {
    repository: R,
}

impl<R> ListHelpCategoriesUseCase<R>
where
    R: HelpCategoryRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self) -> UseCaseResult<Vec<HelpCategory>> {
        let mut categories = self.repository.list_all().await?;
        categories.sort_by_key(|c| c.sort_order);
        Ok(categories)
    }
}
