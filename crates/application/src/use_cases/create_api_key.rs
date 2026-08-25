use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    entities::{ApiKey, CreateApiKey as CreateApiKeyEntity},
    shared::types::{AuthenticatedUser, Permission},
};
use serde_json::json;

use crate::{
    commands::CreateApiKey,
    ports::{ApiKeyRepository, AuditLogEntryRepository, IdGenerator, PasswordHasher},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Maximum length for an API key's display name. Mirrors the pattern
/// used by other named resources in this codebase (e.g. monitor names,
/// status page names).
const MAX_API_KEY_NAME_LENGTH: usize = 100;

#[derive(Debug)]
pub enum CreateApiKeyError {
    /// The caller's role does not carry `Permission::ManageApiKeys`.
    ///
    /// Minting a key is a *grant* of capability — unlike listing or
    /// revoking an existing key, it lets the caller hand out arbitrary
    /// scopes (including write scopes) to whoever ends up holding the
    /// raw key. Without this check a read-only role such as
    /// `Role::Auditor` could mint a key with `MonitorWrite` /
    /// `StatusPageWrite` / etc. and use it to perform writes the role
    /// itself is not permitted to perform, which defeats the read-only
    /// guarantee entirely. See the 2026-08-20 security review of
    /// Feature 10.
    Forbidden,
    EmptyName,
    NameTooLong,
    EmptyScopes,
    ExpiryInPast,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateApiKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "you do not have permission to create api keys for this tenant"
            ),
            Self::EmptyName => write!(f, "api key name cannot be empty"),
            Self::NameTooLong => write!(
                f,
                "api key name cannot exceed {MAX_API_KEY_NAME_LENGTH} characters"
            ),
            Self::EmptyScopes => write!(f, "an api key must have at least one scope"),
            Self::ExpiryInPast => write!(f, "api key expiry must be in the future"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateApiKeyError {}

/// The result of successfully creating an API key: the persisted entity
/// (safe to store/return — `hashed_key` is never the raw secret) plus the
/// RAW key value.
///
/// The raw key is intentionally returned as a separate field rather than
/// stored anywhere on `ApiKey` itself: this is the ONLY moment in the
/// key's entire lifecycle where the raw value exists outside the
/// caller's `X-API-Key` header. The handler must show it to the user
/// once and then discard it.
#[derive(Debug, Clone)]
pub struct CreatedApiKey {
    pub api_key: ApiKey,
    pub raw_key: String,
}

/// Creates a scoped API key for the caller's tenant.
///
/// Tenant-wide (Model A): the key is created for
/// `authenticated_user.tenant_context().tenant_id()`, matching every
/// other tenant-scoped write in this codebase. There is no ownership
/// check to perform here (unlike delete/list, which fetch an existing
/// resource first) — instead the FIRST thing `execute` does is check
/// `Permission::ManageApiKeys` (see `CreateApiKeyError::Forbidden`).
///
/// This check matters more here than on `list`/`delete`: creating a key
/// is a *grant* of scopes, up to and including write scopes, to whoever
/// ends up holding the raw key. `list_api_keys`/`delete_api_key`
/// deliberately stay tenant-wide (`AuthorizationPolicy::same_tenant`) —
/// viewing metadata or revoking a key cannot expand anyone's access —
/// but minting a new key can, so only roles with `ManageApiKeys`
/// (`Admin`/`Owner`) may do it. Without this, a read-only role like
/// `Role::Auditor` could mint a key with write scopes and use it to
/// perform the very writes its role is supposed to be barred from.
pub struct CreateApiKeyUseCase {
    api_key_repository: Arc<dyn ApiKeyRepository>,
    id_generator: Arc<dyn IdGenerator>,
    password_hasher: Arc<dyn PasswordHasher>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
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
        }
    }

    pub async fn execute(
        &self,
        command: CreateApiKey,
        authenticated_user: AuthenticatedUser,
    ) -> Result<CreatedApiKey, CreateApiKeyError> {
        // Permission check FIRST, before any validation or persistence
        // work below — a caller without `ManageApiKeys` (e.g.
        // `Role::Auditor`) must be rejected regardless of how well-formed
        // their request otherwise is.
        if !authenticated_user.can(Permission::ManageApiKeys) {
            return Err(CreateApiKeyError::Forbidden);
        }

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

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let id = self.id_generator.next_api_key_id().await;

        // `pv_<prefix>_<secret>` — see `IdGenerator::next_api_key_secret`
        // for the exact format. Splitting on '_' twice isolates the
        // plaintext lookup prefix from the secret half.
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

        self.api_key_repository
            .create(&api_key)
            .await
            .map_err(CreateApiKeyError::Other)?;

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
