use serde::{Deserialize, Serialize};

/// Fine-grained permission grant carried by an `ApiKey` (see
/// `crates/domain/src/entities/api_key.rs`).
///
/// Unlike `shared::types::Permission` (which is derived from a user's
/// coarse-grained `Role` at request time), scopes are chosen explicitly by
/// the tenant member who creates the key and stored on the key itself —
/// an API key never inherits its creator's full role. This is what makes
/// API keys safe to hand to third-party integrations: a key minted with
/// only `MonitorRead` can never be used to delete a monitor, even though
/// the user who created it could.
///
/// Keep this list stable and additive. Avoid removing or reusing variant
/// names once they are shipped, since existing keys in the database
/// reference these names via `ApiKey::scopes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiKeyScope {
    #[serde(rename = "monitor:read")]
    MonitorRead,
    #[serde(rename = "monitor:write")]
    MonitorWrite,
    #[serde(rename = "status_page:read")]
    StatusPageRead,
    #[serde(rename = "status_page:write")]
    StatusPageWrite,
    #[serde(rename = "incident:read")]
    IncidentRead,
    #[serde(rename = "incident:write")]
    IncidentWrite,
    #[serde(rename = "notification:read")]
    NotificationRead,
    #[serde(rename = "notification:write")]
    NotificationWrite,
    #[serde(rename = "api_flow:read")]
    ApiFlowRead,
    #[serde(rename = "api_flow:write")]
    ApiFlowWrite,
}

impl ApiKeyScope {
    /// All scopes that currently exist. Used by request validation to
    /// reject unknown scope strings with a helpful error instead of a
    /// silent no-op.
    pub const ALL: &'static [ApiKeyScope] = &[
        ApiKeyScope::MonitorRead,
        ApiKeyScope::MonitorWrite,
        ApiKeyScope::StatusPageRead,
        ApiKeyScope::StatusPageWrite,
        ApiKeyScope::IncidentRead,
        ApiKeyScope::IncidentWrite,
        ApiKeyScope::NotificationRead,
        ApiKeyScope::NotificationWrite,
        ApiKeyScope::ApiFlowRead,
        ApiKeyScope::ApiFlowWrite,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MonitorRead => "monitor:read",
            Self::MonitorWrite => "monitor:write",
            Self::StatusPageRead => "status_page:read",
            Self::StatusPageWrite => "status_page:write",
            Self::IncidentRead => "incident:read",
            Self::IncidentWrite => "incident:write",
            Self::NotificationRead => "notification:read",
            Self::NotificationWrite => "notification:write",
            Self::ApiFlowRead => "api_flow:read",
            Self::ApiFlowWrite => "api_flow:write",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.as_str() == value)
    }

    /// The `Permission` a tenant member must hold on their own `Role` in
    /// order to be allowed to mint a key carrying this scope.
    ///
    /// This is what makes `CreateApiKeyUseCase` enforce "an API key must
    /// never have more power than its creator": every scope maps back to
    /// the same coarse-grained permission that gates the equivalent
    /// first-party action, so a role that can't perform an action itself
    /// can't hand a key that grants it either. See P0/P1 "API Key
    /// Authorization Parity" / "API Key Scope Escalation" in the 2026-09
    /// security review.
    ///
    /// Deliberately exhaustive (no wildcard fallback) so that adding a
    /// new `ApiKeyScope` variant without adding an arm here is a compile
    /// error, not a silently-unguarded scope.
    pub fn required_permission(&self) -> crate::shared::types::Permission {
        use crate::shared::types::Permission;
        match self {
            Self::MonitorRead => Permission::ViewMonitor,
            Self::MonitorWrite => Permission::UpdateMonitor,
            Self::StatusPageRead => Permission::ViewMonitor,
            Self::StatusPageWrite => Permission::UpdateMonitor,
            Self::IncidentRead => Permission::ViewIncident,
            Self::IncidentWrite => Permission::AcknowledgeIncident,
            Self::NotificationRead => Permission::ViewMonitor,
            Self::NotificationWrite => Permission::UpdateMonitor,
            Self::ApiFlowRead => Permission::ViewMonitor,
            Self::ApiFlowWrite => Permission::UpdateMonitor,
        }
    }
}

impl std::fmt::Display for ApiKeyScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ApiKeyScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_opt(s).ok_or_else(|| format!("unknown api key scope '{s}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn round_trips_every_scope_through_its_string_form() {
        for scope in ApiKeyScope::ALL {
            let s = scope.to_string();
            let parsed = ApiKeyScope::from_str(&s).expect("known scope must parse back");
            assert_eq!(parsed, *scope);
        }
    }

    #[test]
    fn rejects_unknown_scope_strings() {
        assert!(ApiKeyScope::from_str("monitor:delete").is_err());
        assert!(ApiKeyScope::from_str_opt("bogus").is_none());
    }
}
