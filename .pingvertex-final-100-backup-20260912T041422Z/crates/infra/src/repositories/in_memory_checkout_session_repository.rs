use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::CheckoutSessionRepository;
use async_trait::async_trait;
use domain::CheckoutSession;
use domain::shared::ids::CheckoutSessionId;

#[derive(Debug, Clone, Default)]
pub struct InMemoryCheckoutSessionRepository {
    sessions: Arc<Mutex<HashMap<CheckoutSessionId, CheckoutSession>>>,
}

impl InMemoryCheckoutSessionRepository {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl CheckoutSessionRepository for InMemoryCheckoutSessionRepository {
    async fn create(&self, session: &CheckoutSession) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.sessions
            .lock()
            .unwrap()
            .insert(session.id, session.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        id: CheckoutSessionId,
    ) -> Result<Option<CheckoutSession>, Box<dyn Error + Send + Sync>> {
        Ok(self.sessions.lock().unwrap().get(&id).cloned())
    }

    async fn update(&self, session: &CheckoutSession) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.sessions
            .lock()
            .unwrap()
            .insert(session.id, session.clone());
        Ok(())
    }
}
