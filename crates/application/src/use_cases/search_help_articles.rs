use std::error::Error;

use crate::ports::HelpArticleRepository;
use domain::entities::HelpArticle;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Searches help articles by title, content, or slug.
pub struct SearchHelpArticlesUseCase<R> {
    repository: R,
}

impl<R> SearchHelpArticlesUseCase<R>
where
    R: HelpArticleRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, query: &str) -> UseCaseResult<Vec<HelpArticle>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.repository.search(query).await
    }
}
