use std::sync::Arc;

use application::ports::{PasswordHasher, TokenService, UserRepository};
use infra::UuidIdGenerator;

#[derive(Clone)]
pub struct AuthState {
    pub user_repository: Arc<dyn UserRepository>,
    pub password_hasher: Arc<dyn PasswordHasher>,
    pub token_service: Arc<dyn TokenService>,
    pub id_generator: UuidIdGenerator,
}

impl AuthState {
    pub fn new<U, H, T>(
        user_repository: U,
        password_hasher: H,
        token_service: T,
        id_generator: UuidIdGenerator,
    ) -> Self
    where
        U: UserRepository + 'static,
        H: PasswordHasher + 'static,
        T: TokenService + 'static,
    {
        Self {
            user_repository: Arc::new(user_repository),
            password_hasher: Arc::new(password_hasher),
            token_service: Arc::new(token_service),
            id_generator,
        }
    }
}
