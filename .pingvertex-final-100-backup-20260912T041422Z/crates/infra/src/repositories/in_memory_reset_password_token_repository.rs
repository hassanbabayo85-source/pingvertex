use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use application::ports::ResetPasswordTokenRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{ResetPasswordToken, ResetPasswordTokenId, UserId};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryResetPasswordTokenRepository {
    tokens: Arc<Mutex<Vec<ResetPasswordToken>>>,
}

impl InMemoryResetPasswordTokenRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ResetPasswordTokenRepository for InMemoryResetPasswordTokenRepository {
    async fn create(&self, token: &ResetPasswordToken) -> RepoResult<()> {
        self.tokens.lock().unwrap().push(token.clone());
        Ok(())
    }

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<ResetPasswordToken>> {
        Ok(self
            .tokens
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.token_prefix == token_prefix)
            .cloned())
    }

    async fn mark_used(&self, id: ResetPasswordTokenId, used_at: DateTime<Utc>) -> RepoResult<()> {
        if let Some(token) = self.tokens.lock().unwrap().iter_mut().find(|t| t.id == id) {
            token.used_at = Some(used_at);
        }
        Ok(())
    }

    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()> {
        let mut tokens = self.tokens.lock().unwrap();
        tokens.retain(|t| t.user_id != user_id || t.used_at.is_some());
        Ok(())
    }

    async fn try_consume(
        &self,
        id: ResetPasswordTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let mut tokens = self.tokens.lock().unwrap();
        if let Some(token) = tokens
            .iter_mut()
            .find(|t| t.id == id && t.used_at.is_none())
        {
            token.used_at = Some(used_at);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
