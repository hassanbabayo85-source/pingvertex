use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    entities::{ApiKey, CreateApiKey as CreateApiKeyEntity, MAX_API_KEYS},
    enums::ApiKeyScope,
    shared::types::{AuthenticatedUser, Permission},
};
use serde_json::json;

use crate::{
    commands::CreateApiKey,
    errors::PlanLimitError,
    ports::{
        ApiKeyRepository, AuditLogEntryRepository, EntitlementResolver, IdGenerator,
        PasswordHasher,
    },
};

use crate::ports::monitor_repository::QuotaCheckOutcome;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Maximum length for an API key's display name.
const MAX_API_KEY_NAME_LENGTH: usize = 100;

#[derive(Debug)]
pub enum CreateApiKeyError {
    /// The caller's role does not carry `Permission::ManageApiKeys`.
    Forbidden,
    /// One or more requested scopes grant more than the caller's own
    /// role permits.
    ScopeExceedsPermissions(ApiKeyScope),
    EmptyName,
    NameTooLong,
    EmptyScopes,
    ExpiryInPast,
    /// TASK 4A — the tenant has reached `entitlements.max_api_keys`.
    PlanLimitExceeded(PlanLimitError),
    /// TASK 4A — the resolver was not wired. Deployment bug.
    EntitlementResolverMissing,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateApiKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "you do not have permission to create api keys for this tenant"
            ),
            Self::ScopeExceedsPermissions(scope) => write!(
                f,
                "requested scope '{scope}' exceeds your own role's permissions"
            ),
            Self::EmptyName => write!(f, "api key name cannot be empty"),
            Self::NameTooLong => write!(
                f,
                "api key name cannot exceed {MAX_API_KEY_NAME_LENGTH} characters"
            ),
            Self::EmptyScopes => write!(f, "an api key must have at least one scope"),
            Self::ExpiryInPast => write!(f, "api key expiry must be in the future"),
            Self::PlanLimitExceeded(pl) => write!(f, "{pl}"),
            Self::EntitlementResolverMissing => write!(
                f,
                "entitlement resolver is not configured; cannot enforce plan limits"
            ),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateApiKeyError {}

/// The result of successfully creating an API key: the persisted entity
/// plus the RAW key value.
#[derive(Debug, Clone)]
pub struct CreatedApiKey {
    pub api_key: ApiKey,
    pub raw_key: String,
}

pub struct CreateApiKeyUseCase {
    api_key_repository: Arc<dyn ApiKeyRepository>,
    id_generator: Arc<dyn IdGenerator>,
    password_hasher: Arc<dyn PasswordHasher>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
}

impl CreateApiKeyUseCase {
    pub fn new(
        api_key_repository: Arc<dyn ApiKeyRepository>,
        id_generator: Arc<dyn IdGenerator>,
        password_hasher: Arc<dyn PasswordHasher>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            api_key_repository,
            id_generator,
            password_hasher,
            audit_log_entry_repository,
            entitlement_resolver: None,
        }
    }

    /// TASK 4A — installs the authoritative `EntitlementResolver`.
    /// Production handlers MUST call this; `execute` fails closed with
    /// `EntitlementResolverMissing` if it is absent.
    pub fn with_entitlement_resolver(
        mut self,
        resolver: Arc<dyn EntitlementResolver>,
    ) -> Self {
        self.entitlement_resolver = Some(resolver);
        self
    }

    pub async fn execute(
        &self,
        command: CreateApiKey,
        authenticated_user: AuthenticatedUser,
    ) -> Result<CreatedApiKey, CreateApiKeyError> {
        // 1. Permission check FIRST (see the privilege-escalation note
        //    on this type's doc comment).
        if !authenticated_user.can(Permission::ManageApiKeys) {
            return Err(CreateApiKeyError::Forbidden);
        }

        // 2. Scope permission check ("an API key must never have more
        //    power than its creator").
        for scope in &command.scopes {
            if !authenticated_user.can(scope.required_permission()) {
                return Err(CreateApiKeyError::ScopeExceedsPermissions(*scope));
            }
        }

        // 3. Field validation.
        let name = command.name.trim();
        if name.is_empty() {
            return Err(CreateApiKeyError::EmptyName);
        }
        if name.chars().count() > MAX_API_KEY_NAME_LENGTH {
            return Err(CreateApiKeyError::NameTooLong);
        }
        if command.scopes.is_empty() {
            return Err(CreateApiKeyError::EmptyScopes);
        }
        if let Some(expires_at) = command.expires_at
            && expires_at <= Utc::now()
        {
            return Err(CreateApiKeyError::ExpiryInPast);
        }

        // 4. Resolve entitlements (fail closed if the resolver is not
        //    wired — deployment bug).
        let resolver = self
            .entitlement_resolver
            .as_ref()
            .ok_or(CreateApiKeyError::EntitlementResolverMissing)?;
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let entitlements = resolver
            .resolve(tenant_id)
            .await
            .map_err(CreateApiKeyError::Other)?;

        // 5. Generate the id + secret + hash. This is the most expensive
        //    step (Argon2). It runs only after every cheap check above
        //    has passed, so a missing resolver does not waste CPU.
        let id = self.id_generator.next_api_key_id().await;

        let raw_key = self.id_generator.next_api_key_secret().await;
        let key_prefix = raw_key
            .rsplit_once('_')
            .map(|(prefix, _secret)| prefix.to_string())
            .unwrap_or_else(|| raw_key.clone());

        let hashed_key = self
            .password_hasher
            .hash(&raw_key)
            .await
            .map_err(CreateApiKeyError::Other)?;

        let api_key = ApiKey::create(CreateApiKeyEntity {
            id,
            tenant_id,
            user_id,
            name: name.to_string(),
            key_prefix,
            hashed_key,
            scopes: command.scopes,
            expires_at: command.expires_at,
        });

        // 6. Atomic tenant-scoped quota check + insert.
        //
        // TASK 4A — quota limit is `entitlements.max_api_keys`.
        //
        // `save_if_within_quota` counts ALL rows currently in
        // `api_keys` for the tenant, INCLUDING expired ones. Expired
        // keys still consume their quota slot until they are explicitly
        // deleted. This matches the pre-existing behaviour (keys are
        // never auto-deleted) and a retention policy can be added
        // later without changing this contract.
        match self
            .api_key_repository
            .save_if_within_quota(&api_key, tenant_id, entitlements.max_api_keys)
            .await
            .map_err(CreateApiKeyError::Other)?
        {
            QuotaCheckOutcome::Saved => {}
            QuotaCheckOutcome::QuotaExceeded => {
                return Err(CreateApiKeyError::PlanLimitExceeded(PlanLimitError::new(
                    MAX_API_KEYS,
                    u64::from(entitlements.max_api_keys),
                    u64::from(entitlements.max_api_keys),
                    "upgrade_plan",
                )));
            }
        }

        // 7. Audit (only after the save succeeded).
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::ApiKeyCreated,
            target_id: api_key.id.0.to_string(),
            metadata: json!({
                "name": api_key.name,
                "scopes": api_key.scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(CreateApiKeyError::Other)?;

        Ok(CreatedApiKey { api_key, raw_key })
    }
}
