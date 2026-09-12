use application::ports::HelpCategoryRepository;
use async_trait::async_trait;
use domain::entities::HelpCategory;
use domain::shared::ids::HelpCategoryId;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Default)]
pub struct InMemoryHelpCategoryRepository {
    items: Arc<Mutex<HashMap<HelpCategoryId, HelpCategory>>>,
}

impl InMemoryHelpCategoryRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl HelpCategoryRepository for InMemoryHelpCategoryRepository {
    async fn create(&self, category: &HelpCategory) -> R<()> {
        self.items
            .lock()
            .unwrap()
            .insert(category.id, category.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: HelpCategoryId) -> R<Option<HelpCategory>> {
        Ok(self.items.lock().unwrap().get(&id).cloned())
    }

    async fn find_by_slug(&self, slug: &str) -> R<Option<HelpCategory>> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .values()
            .find(|c| c.slug == slug)
            .cloned())
    }

    async fn list_all(&self) -> R<Vec<HelpCategory>> {
        Ok(self.items.lock().unwrap().values().cloned().collect())
    }
}
