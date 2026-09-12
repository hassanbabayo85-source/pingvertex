use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Durable audit record for financial/security-sensitive billing actions.
/// `actor` is deliberately a string so both authenticated users and trusted
/// machine actors (for example `billing:webhook`) can be represented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinancialAuditLogEntry {
    pub id: Uuid,
    pub actor: String,
    pub tenant_id: Option<Uuid>,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl FinancialAuditLogEntry {
    pub fn new(
        actor: impl Into<String>,
        tenant_id: Option<Uuid>,
        action: impl Into<String>,
        resource: impl Into<String>,
        resource_id: Option<String>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            actor: actor.into(),
            tenant_id,
            action: action.into(),
            resource: resource.into(),
            resource_id,
            created_at: Utc::now(),
            metadata: Self::sanitize_metadata(metadata),
        }
    }

    /// Financial audit metadata is defense-in-depth sanitized at the domain
    /// boundary so a future caller cannot accidentally persist credentials or
    /// secrets even if it forgets to sanitize before constructing the entry.
    fn sanitize_metadata(value: serde_json::Value) -> serde_json::Value {
        fn walk(v: serde_json::Value) -> serde_json::Value {
            match v {
                serde_json::Value::Object(map) => {
                    let mut out = serde_json::Map::new();
                    for (k, val) in map {
                        let lower = k.to_ascii_lowercase();
                        if lower.contains("password")
                            || lower.contains("credential")
                            || lower.contains("secret")
                            || lower.contains("api_key")
                            || lower.contains("apikey")
                            || lower == "token"
                            || lower.ends_with("_token")
                        {
                            continue;
                        }
                        out.insert(k, walk(val));
                    }
                    serde_json::Value::Object(out)
                }
                serde_json::Value::Array(items) => {
                    serde_json::Value::Array(items.into_iter().map(walk).collect())
                }
                other => other,
            }
        }
        walk(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn financial_audit_metadata_never_persists_secrets() {
        let entry = FinancialAuditLogEntry::new(
            "user:1",
            Some(Uuid::new_v4()),
            "payment_created",
            "payment",
            Some("payment-1".into()),
            serde_json::json!({
                "safe": "ok",
                "password": "p",
                "proxy_credentials": "c",
                "api_secret": "s",
                "api_key": "k",
                "token": "t",
                "nested": {"access_token": "at", "safe_nested": true}
            }),
        );
        let text = entry.metadata.to_string();
        assert!(text.contains("safe"));
        assert!(!text.contains("password"));
        assert!(!text.contains("credential"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("api_key"));
        assert!(!text.contains("token"));
    }
}
