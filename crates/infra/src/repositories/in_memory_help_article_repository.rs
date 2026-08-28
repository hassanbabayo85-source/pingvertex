use application::ports::HelpArticleRepository;
use async_trait::async_trait;
use domain::entities::HelpArticle;
use domain::enums::HelpArticleStatus;
use domain::shared::ids::HelpCategoryId;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Default)]
pub struct InMemoryHelpArticleRepository {
    items: Arc<Mutex<HashMap<String, HelpArticle>>>,
}

impl InMemoryHelpArticleRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_article(article: HelpArticle) -> Self {
        let repo = Self::new();
        repo.items
            .lock()
            .unwrap()
            .insert(article.slug.clone(), article);
        repo
    }
}

#[async_trait]
impl HelpArticleRepository for InMemoryHelpArticleRepository {
    async fn create(&self, article: &HelpArticle) -> R<()> {
        self.items
            .lock()
            .unwrap()
            .insert(article.slug.clone(), article.clone());
        Ok(())
    }

    async fn find_by_slug(&self, slug: &str) -> R<Option<HelpArticle>> {
        Ok(self.items.lock().unwrap().get(slug).cloned())
    }

    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> R<Vec<HelpArticle>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| {
                a.category_id == category_id
                    && (!only_published || a.status == HelpArticleStatus::Published)
            })
            .cloned()
            .collect())
    }

    async fn search(&self, query: &str) -> R<Vec<HelpArticle>> {
        let q = query.to_lowercase();
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .filter(|a| {
                a.title.to_lowercase().contains(&q) || a.content.to_lowercase().contains(&q)
            })
            .cloned()
            .collect())
    }
}
