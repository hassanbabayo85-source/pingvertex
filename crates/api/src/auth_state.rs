use std::sync::Arc;

use application::ports::{
    PasswordHasher, TenantMembershipRepository, TokenService, UserRepository,
};
use infra::UuidIdGenerator;

#[derive(Clone)]
pub struct AuthState {
    pub user_repository: Arc<dyn UserRepository>,
    pub password_hasher: Arc<dyn PasswordHasher>,
    pub token_service: Arc<dyn TokenService>,
    pub id_generator: UuidIdGenerator,
    pub tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
}

impl AuthState {
    pub fn new<U, H, T, TM>(
        user_repository: U,
        password_hasher: H,
        token_service: T,
        id_generator: UuidIdGenerator,
        tenant_membership_repository: TM,
    ) -> Self
    where
        U: UserRepository + 'static,
        H: PasswordHasher + 'static,
        T: TokenService + 'static,
        TM: TenantMembershipRepository + 'static,
    {
        Self {
            user_repository: Arc::new(user_repository),
            password_hasher: Arc::new(password_hasher),
            token_service: Arc::new(token_service),
            id_generator,
            tenant_membership_repository: Arc::new(tenant_membership_repository),
        }
    }
}
