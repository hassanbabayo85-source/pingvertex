use async_trait::async_trait;
use std::error::Error;

use domain::entities::HelpArticle;
use domain::shared::ids::HelpCategoryId;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Help Center article storage.
#[async_trait]
pub trait HelpArticleRepository: Send + Sync {
    async fn create(&self, article: &HelpArticle) -> RepoResult<()>;
    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpArticle>>;
    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> RepoResult<Vec<HelpArticle>>;
    async fn list_published(&self) -> RepoResult<Vec<HelpArticle>>;
    async fn search(&self, query: &str) -> RepoResult<Vec<HelpArticle>>;
}

#[async_trait]
impl<T> HelpArticleRepository for std::sync::Arc<T>
where
    T: HelpArticleRepository + Send + Sync + ?Sized,
{
    async fn create(&self, article: &HelpArticle) -> RepoResult<()> {
        (**self).create(article).await
    }
    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpArticle>> {
        (**self).find_by_slug(slug).await
    }
    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> RepoResult<Vec<HelpArticle>> {
        (**self).list_by_category(category_id, only_published).await
    }
    async fn list_published(&self) -> RepoResult<Vec<HelpArticle>> {
        (**self).list_published().await
    }
    async fn search(&self, query: &str) -> RepoResult<Vec<HelpArticle>> {
        (**self).search(query).await
    }
}
