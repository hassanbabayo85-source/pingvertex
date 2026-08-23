use chrono::{DateTime, Utc};
use domain::enums::ApiKeyScope;

/// Command to create a new scoped API key for the caller's tenant.
#[derive(Debug, Clone)]
pub struct CreateApiKey {
    pub name: String,
    pub scopes: Vec<ApiKeyScope>,
    /// Optional expiry. `None` means the key never expires.
    pub expires_at: Option<DateTime<Utc>>,
}
