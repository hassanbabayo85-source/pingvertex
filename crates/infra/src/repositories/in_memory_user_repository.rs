use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::UserRepository;
use async_trait::async_trait;
use domain::{Email, User, UserId, Username};

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Simple in-memory repository for development and tests.
///
/// Mirrors `InMemoryMonitorRepository`'s shape (`Arc<RwLock<HashMap<..>>>`
/// behind a `Clone` struct, so cloned handles share state).
///
/// Added for the HTTP-layer integration test suite
/// (`crates/api/tests/auth.rs`): the existing `InMemoryRegistrationTransaction`
/// (used by other HTTP tests such as `crates/api/tests/monitors.rs`) is a
/// pure no-op that does not persist anything, so a register -> login ->
/// session-validation round trip needs a `UserRepository` double that
/// actually stores what it's given. See `crates/api/tests/auth.rs`'s
/// `PersistingRegistrationTransaction` for how the two are wired together.
#[derive(Debug, Clone, Default)]
pub struct InMemoryUserRepository {
    users: Arc<RwLock<HashMap<UserId, User>>>,
}

impl InMemoryUserRepository {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl UserRepository for InMemoryUserRepository {
    async fn create(&self, user: &User) -> RepoResult<()> {
        self.users.write().unwrap().insert(user.id(), user.clone());
        Ok(())
    }

    async fn update(&self, user: &User) -> RepoResult<()> {
        self.users.write().unwrap().insert(user.id(), user.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> RepoResult<Option<User>> {
        Ok(self.users.read().unwrap().get(id).cloned())
    }

    async fn find_by_email(&self, email: &Email) -> RepoResult<Option<User>> {
        Ok(self
            .users
            .read()
            .unwrap()
            .values()
            .find(|u| u.email().as_str() == email.as_str())
            .cloned())
    }

    async fn find_by_username(&self, username: &Username) -> RepoResult<Option<User>> {
        Ok(self
            .users
            .read()
            .unwrap()
            .values()
            .find(|u| u.username().as_str() == username.as_str())
            .cloned())
    }

    async fn delete(&self, id: &UserId) -> RepoResult<()> {
        self.users.write().unwrap().remove(id);
        Ok(())
    }

    async fn email_exists(&self, email: &Email) -> RepoResult<bool> {
        Ok(self
            .users
            .read()
            .unwrap()
            .values()
            .any(|u| u.email().as_str() == email.as_str()))
    }

    async fn list_all(&self) -> RepoResult<Vec<User>> {
        Ok(self.users.read().unwrap().values().cloned().collect())
    }

    async fn username_exists(&self, username: &Username) -> RepoResult<bool> {
        Ok(self
            .users
            .read()
            .unwrap()
            .values()
            .any(|u| u.username().as_str() == username.as_str()))
    }
}
