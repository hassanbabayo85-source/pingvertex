use std::{error::Error, sync::Arc};

use domain::{
    entities::ApiKey,
    shared::types::{AuthenticatedUser, Permission},
};

use crate::ports::ApiKeyRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists every API key belonging to the caller's tenant.
///
/// Tenant-wide (Model A): every member of the tenant can see every key —
/// including ones created by other members — matching
/// `AuthorizationPolicy::same_tenant` used throughout this codebase.
/// `hashed_key` is present on the returned entities but the API layer's
/// response DTO (`ApiKeySummaryResponse`) never serializes it, so the
/// hash itself never leaves the server.
#[derive(Clone)]
pub struct ListApiKeysUseCase {
    api_key_repository: Arc<dyn ApiKeyRepository>,
}

impl ListApiKeysUseCase {
    pub fn new(api_key_repository: Arc<dyn ApiKeyRepository>) -> Self {
        Self { api_key_repository }
    }

    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Vec<ApiKey>> {
        // Security: API keys are privileged tenant credentials.
        // Only roles carrying ManageApiKeys may enumerate them.
        authenticated_user.require(Permission::ManageApiKeys)?;

        let tenant_id = authenticated_user.tenant_context().tenant_id();
        self.api_key_repository.list_by_tenant(tenant_id).await
    }
}
