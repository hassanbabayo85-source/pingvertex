use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    enums::ApiKeyScope,
    shared::ids::{ApiKeyId, TenantId, UserId},
    shared::policies::{Ownable, TenantOwned},
};

/// Data required to create a new `ApiKey`.
///
/// Deliberately does NOT take the raw or hashed key — those are generated
/// by the caller (the application layer, via `IdGenerator::next_api_key_secret`
/// and `PasswordHasher::hash`), because randomness/hashing are I/O-adjacent
/// concerns the domain entity itself never performs, matching the
/// `CreateWebhookChannel` pattern used for notification channels.
#[derive(Clone)]
pub struct CreateApiKey {
    pub id: ApiKeyId,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub name: String,
    /// The non-secret lookup portion of the raw key (e.g. `pv_3f9a2c1e`),
    /// stored in plaintext so `ApiKeyAuthMiddleware` can find the row with
    /// a single indexed lookup instead of scanning + Argon2-verifying
    /// every key on every request.
    pub key_prefix: String,
    /// Argon2 hash of the FULL raw key. The raw key itself is never
    /// persisted anywhere and is shown to the user exactly once, at
    /// creation time (see `CreateApiKeyUseCase`).
    pub hashed_key: String,
    pub scopes: Vec<ApiKeyScope>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// A scoped credential that lets a third-party integration authenticate to
/// the API as `X-API-Key: <raw key>` instead of a user's session/JWT.
///
/// Tenant-wide (Model A): any member of the tenant can view/revoke any key
/// belonging to that tenant, not just the key's creator — enforced via
/// `AuthorizationPolicy::same_tenant` in the `CreateApiKey`/`ListApiKeys`/
/// `DeleteApiKey` use cases, exactly like every other tenant-scoped
/// resource in this codebase. `user_id` is retained purely as an audit
/// trail of who originally created the key, not as an ownership gate.
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub name: String,
    pub key_prefix: String,
    pub hashed_key: String,
    pub scopes: Vec<ApiKeyScope>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Updated (best-effort, fire-and-forget) by `ApiKeyAuthMiddleware`
    /// every time this key successfully authenticates a request. `None`
    /// until the key is used for the first time.
    pub last_used_at: Option<DateTime<Utc>>,
}

impl ApiKey {
    pub fn create(input: CreateApiKey) -> Self {
        Self {
            id: input.id,
            tenant_id: input.tenant_id,
            user_id: input.user_id,
            name: input.name,
            key_prefix: input.key_prefix,
            hashed_key: input.hashed_key,
            scopes: input.scopes,
            created_at: Utc::now(),
            expires_at: input.expires_at,
            last_used_at: None,
        }
    }

    /// Whether this key has expired as of `now`. Keys with no `expires_at`
    /// never expire.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }

    /// Whether this key was granted `scope`.
    pub fn has_scope(&self, scope: ApiKeyScope) -> bool {
        self.scopes.contains(&scope)
    }
}

impl TenantOwned for ApiKey {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}

impl Ownable for ApiKey {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

// The `Debug` impls in this file sit after `mod tests` because they are
// the credential-redaction boundary added and were kept at the
// bottom to keep that PART's diff focused on the impls themselves. The
// ordering is a style preference; the lint is suppressed explicitly here
// rather than reorganising the file, so the diff stays minimal and the
// redaction review remains easy to re-audit.
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_key(scopes: Vec<ApiKeyScope>, expires_at: Option<DateTime<Utc>>) -> ApiKey {
        ApiKey::create(CreateApiKey {
            id: ApiKeyId::new(),
            tenant_id: TenantId::new(),
            user_id: UserId::new(),
            name: "CI integration".to_string(),
            key_prefix: "pv_abc123".to_string(),
            hashed_key: "argon2-hash".to_string(),
            scopes,
            expires_at,
        })
    }

    #[test]
    fn key_with_no_expiry_never_expires() {
        let key = sample_key(vec![ApiKeyScope::MonitorRead], None);
        assert!(!key.is_expired(Utc::now() + chrono::Duration::days(3650)));
    }

    #[test]
    fn key_is_expired_once_expires_at_has_passed() {
        let key = sample_key(
            vec![ApiKeyScope::MonitorRead],
            Some(Utc::now() - chrono::Duration::seconds(1)),
        );
        assert!(key.is_expired(Utc::now()));
    }

    #[test]
    fn key_is_not_expired_before_expires_at() {
        let key = sample_key(
            vec![ApiKeyScope::MonitorRead],
            Some(Utc::now() + chrono::Duration::days(1)),
        );
        assert!(!key.is_expired(Utc::now()));
    }

    #[test]
    fn has_scope_checks_membership() {
        let key = sample_key(vec![ApiKeyScope::MonitorRead], None);
        assert!(key.has_scope(ApiKeyScope::MonitorRead));
        assert!(!key.has_scope(ApiKeyScope::MonitorWrite));
    }
}

/// Manual Debug that redacts `hashed_key` on `ApiKey`.
impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKey")
            .field("id", &self.id)
            .field("tenant_id", &self.tenant_id)
            .field("name", &self.name)
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at)
            .field("hashed_key", &"[REDACTED]")
            .finish()
    }
}

/// Manual Debug that redacts `hashed_key` on the create-DTO.
impl std::fmt::Debug for CreateApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateApiKey")
            .field("id", &self.id)
            .field("tenant_id", &self.tenant_id)
            .field("user_id", &self.user_id)
            .field("name", &self.name)
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at)
            .field("hashed_key", &"[REDACTED]")
            .finish()
    }
}
