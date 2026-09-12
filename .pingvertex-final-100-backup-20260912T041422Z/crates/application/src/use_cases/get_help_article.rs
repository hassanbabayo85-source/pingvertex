use std::error::Error;

use crate::ports::HelpArticleRepository;
use domain::entities::HelpArticle;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Retrieves a single help article by slug.
pub struct GetHelpArticleUseCase<R> {
    repository: R,
}

impl<R> GetHelpArticleUseCase<R>
where
    R: HelpArticleRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, slug: &str) -> UseCaseResult<Option<HelpArticle>> {
        self.repository.find_by_slug(slug).await
    }
}
