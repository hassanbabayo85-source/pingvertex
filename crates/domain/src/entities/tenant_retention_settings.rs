use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
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
    /// PART 9 — minimum allowed retention window, in days.
    pub const MIN_RETENTION_DAYS: u32 = 1;
    /// PART 9 — maximum allowed retention window, in days (10 years).
    pub const MAX_RETENTION_DAYS: u32 = 3650;

    /// Constructs retention settings from a trusted source.
    ///
    /// This constructor intentionally does NOT validate `retention_days`,
    /// because it is also used to reconstruct rows from the database
    /// (including historical or hand-edited rows whose values may fall
    /// outside the current allowed range). `effective_retention` on the
    /// repository clamps such values before use.
    ///
    /// Callers that pass a *user-supplied* `retention_days` MUST use
    /// [`Self::try_new`] instead so the value is validated at the domain
    /// boundary.
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

    /// PART 9 — fallible constructor that validates `retention_days` is
    /// within `[MIN_RETENTION_DAYS, MAX_RETENTION_DAYS]`.
    pub fn try_new(
        tenant_id: TenantId,
        retention_days: u32,
        source: RetentionSource,
    ) -> Result<Self, DomainError> {
        if retention_days < Self::MIN_RETENTION_DAYS {
            return Err(DomainError::RetentionDaysTooSmall);
        }
        if retention_days > Self::MAX_RETENTION_DAYS {
            return Err(DomainError::RetentionDaysTooLarge);
        }
        let now = Utc::now();
        Ok(Self {
            tenant_id,
            retention_days,
            source,
            created_at: now,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod part9_tests {
    use super::*;

    #[test]
    fn accepts_retention_days_at_min() {
        let result = TenantRetentionSettings::try_new(
            TenantId::new(),
            TenantRetentionSettings::MIN_RETENTION_DAYS,
            RetentionSource::Default,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn accepts_retention_days_at_max() {
        let result = TenantRetentionSettings::try_new(
            TenantId::new(),
            TenantRetentionSettings::MAX_RETENTION_DAYS,
            RetentionSource::Default,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_retention_days_below_min() {
        let result = TenantRetentionSettings::try_new(TenantId::new(), 0, RetentionSource::Default);
        assert_eq!(result.unwrap_err(), DomainError::RetentionDaysTooSmall);
    }

    #[test]
    fn rejects_retention_days_above_max() {
        let result = TenantRetentionSettings::try_new(
            TenantId::new(),
            TenantRetentionSettings::MAX_RETENTION_DAYS + 1,
            RetentionSource::Default,
        );
        assert_eq!(result.unwrap_err(), DomainError::RetentionDaysTooLarge);
    }
}
