use chrono::{DateTime, Utc};
use domain::{entities::ApiKey, enums::ApiKeyScope};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

/// Request body for `POST /api-keys`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Scope strings, e.g. `["monitor:read", "monitor:write"]`. Parsed
    /// via `ApiKeyScope::from_str` — an unknown value is a 400, not a
    /// silent no-op.
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl CreateApiKeyRequest {
    /// Parses `scopes` into `ApiKeyScope`s, returning a 400 `ApiError` on
    /// the first unrecognized scope string.
    pub fn parse_scopes(&self) -> Result<Vec<ApiKeyScope>, ApiError> {
        self.scopes
            .iter()
            .map(|raw| {
                raw.parse::<ApiKeyScope>()
                    .map_err(|_| ApiError::bad_request(format!("unknown scope '{raw}'")))
            })
            .collect()
    }
}

/// Response returned ONCE, immediately after creation — the only time the
/// raw (unhashed) key is ever available. Callers must store it now; it
/// cannot be recovered later (`GET`/`list` only ever return
/// `ApiKeySummaryResponse`, which has no `key` field).
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyCreateResponse {
    pub id: String,
    pub name: String,
    /// The raw, usable API key. Shown once. Send as `X-API-Key: <key>`.
    pub key: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ApiKeyCreateResponse {
    pub fn new(api_key: ApiKey, raw_key: String) -> Self {
        Self {
            id: api_key.id.0.to_string(),
            name: api_key.name,
            key: raw_key,
            key_prefix: api_key.key_prefix,
            scopes: api_key.scopes.iter().map(|s| s.to_string()).collect(),
            created_at: api_key.created_at,
            expires_at: api_key.expires_at,
        }
    }
}

/// Summary view returned by `GET /api-keys` and used for any other
/// non-creation response. Deliberately has NO `key` or `hashed_key`
/// field — the raw key is unrecoverable after creation, and the hash
/// must never leave the server.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeySummaryResponse {
    pub id: String,
    pub name: String,
    /// Safe to display (e.g. "pv_3f9a2c1e…") so a user can tell which key
    /// is which without ever seeing the secret again.
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl From<ApiKey> for ApiKeySummaryResponse {
    fn from(api_key: ApiKey) -> Self {
        Self {
            id: api_key.id.0.to_string(),
            name: api_key.name,
            key_prefix: api_key.key_prefix,
            scopes: api_key.scopes.iter().map(|s| s.to_string()).collect(),
            created_at: api_key.created_at,
            expires_at: api_key.expires_at,
            last_used_at: api_key.last_used_at,
        }
    }
}
