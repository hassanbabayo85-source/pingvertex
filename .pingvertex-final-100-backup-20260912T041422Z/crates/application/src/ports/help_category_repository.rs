use async_trait::async_trait;
use std::error::Error;

use domain::entities::HelpCategory;
use domain::shared::ids::HelpCategoryId;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Help Center category storage.
#[async_trait]
pub trait HelpCategoryRepository: Send + Sync {
    async fn create(&self, category: &HelpCategory) -> RepoResult<()>;
    async fn find_by_id(&self, id: HelpCategoryId) -> RepoResult<Option<HelpCategory>>;
    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpCategory>>;
    async fn list_all(&self) -> RepoResult<Vec<HelpCategory>>;
}

#[async_trait]
impl<T> HelpCategoryRepository for std::sync::Arc<T>
where
    T: HelpCategoryRepository + Send + Sync + ?Sized,
{
    async fn create(&self, category: &HelpCategory) -> RepoResult<()> {
        (**self).create(category).await
    }
    async fn find_by_id(&self, id: HelpCategoryId) -> RepoResult<Option<HelpCategory>> {
        (**self).find_by_id(id).await
    }
    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpCategory>> {
        (**self).find_by_slug(slug).await
    }
    async fn list_all(&self) -> RepoResult<Vec<HelpCategory>> {
        (**self).list_all().await
    }
}
