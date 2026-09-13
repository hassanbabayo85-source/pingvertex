use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::TenantId;

/// How the tenant's current retention value was set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionSource {
    /// Came from the active `pricing_configs.default_retention_days`.
    Default,
    /// Migrated from a pre-PAYG `subscriptions.retention_days`. Preserved
    /// so the retention worker does not delete a legacy tenant's history
    /// on the first sweep after the migration.
    Legacy,
    /// Purchased explicitly via the extended-retention add-on. Always wins
    /// until the tenant cancels the add-on.
    Addon,
}

impl RetentionSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Legacy => "legacy",
            Self::Addon => "addon",
        }
    }

    /// Priority when merging multiple sources: `Addon` beats `Legacy`, and
    /// `Legacy` beats `Default`. Used by `TenantRetentionSettingsRepository`
    /// implementations to decide whether an existing row should be
    /// overwritten by an incoming one.
    pub const fn priority(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::Legacy => 1,
            Self::Addon => 2,
        }
    }
}

impl std::str::FromStr for RetentionSource {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "default" => Ok(Self::Default),
            "legacy" => Ok(Self::Legacy),
            "addon" => Ok(Self::Addon),
            other => Err(format!("unknown retention source: {other}")),
        }
    }
}

/// A tenant's effective history-retention setting.
///
/// At most one row per tenant. When no row exists, the pricing
/// configuration's `default_retention_days` applies — the repository's
/// `effective_retention` method handles that fallback so callers never
/// have to encode the default themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantRetentionSettings {
    pub tenant_id: TenantId,
    pub retention_days: u32,
    pub source: RetentionSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TenantRetentionSettings {
    pub fn new(tenant_id: TenantId, retention_days: u32, source: RetentionSource) -> Self {
        let now = Utc::now();
        Self {
            tenant_id,
            retention_days,
            source,
            created_at: now,
            updated_at: now,
        }
    }
}
