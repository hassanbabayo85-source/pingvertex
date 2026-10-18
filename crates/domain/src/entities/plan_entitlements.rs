//! Versioned numeric entitlement attached to a plan.
//!
//! Entitlements are (key, limit_value) pairs, e.g.
//! `("max_uptime_monitors", 20)`.
//!
//! Versioning means a subscription can keep the entitlement_version it
//! was purchased with. Plan-level changes create a new version rather
//! than silently changing existing customers' limits.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Canonical entitlement keys ─────────────────────────────────────
//
// Central constants so callers never misspell these. Adding a new
// entitlement means adding a constant here AND seeding it in the
// corresponding migration.

pub const MAX_UPTIME_MONITORS: &str = "max_uptime_monitors";
pub const MAX_HEARTBEAT_MONITORS: &str = "max_heartbeat_monitors";
pub const MIN_UPTIME_INTERVAL_SECONDS: &str = "min_uptime_interval_seconds";
pub const MAX_REGIONS_PER_MONITOR: &str = "max_regions_per_monitor";
pub const MAX_API_FLOW_MONITORS: &str = "max_api_flow_monitors";
pub const MAX_API_FLOW_STEPS: &str = "max_api_flow_steps";
pub const MAX_PROXY_ENTRIES_PER_JOB: &str = "max_proxy_entries_per_job";
pub const MAX_CONCURRENT_PROXY_JOBS: &str = "max_concurrent_proxy_jobs";
pub const MAX_RECURRING_PROXY_MONITORS: &str = "max_recurring_proxy_monitors";
pub const MIN_RECURRING_PROXY_INTERVAL_SECONDS: &str = "min_recurring_proxy_interval_seconds";
pub const MAX_RECURRING_PROXY_EXECUTIONS_PER_MONTH: &str =
    "max_recurring_proxy_executions_per_month";
pub const MAX_RUM_SITES: &str = "max_rum_sites";
pub const MAX_RUM_EVENTS_PER_MONTH: &str = "max_rum_events_per_month";
pub const MAX_STATUS_PAGES: &str = "max_status_pages";
pub const MAX_API_KEYS: &str = "max_api_keys";
pub const MAX_TEAM_MEMBERS: &str = "max_team_members";

// ─── Entity ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntitlement {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub key: String,
    pub limit_value: i64,
    pub version: i64,
    pub effective_from: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl PlanEntitlement {
    pub fn new(
        id: Uuid,
        plan_id: Uuid,
        key: impl Into<String>,
        limit_value: i64,
        version: i64,
        effective_from: DateTime<Utc>,
    ) -> Result<Self, PlanEntitlementError> {
        let key = key.into();
        if key.trim().is_empty() {
            return Err(PlanEntitlementError::EmptyKey);
        }
        if limit_value < 0 {
            return Err(PlanEntitlementError::NegativeLimit(limit_value));
        }
        if version < 1 {
            return Err(PlanEntitlementError::NonPositiveVersion(version));
        }
        Ok(Self {
            id,
            plan_id,
            key,
            limit_value,
            version,
            effective_from,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanEntitlementError {
    #[error("entitlement key cannot be empty")]
    EmptyKey,
    #[error("entitlement limit cannot be negative: {0}")]
    NegativeLimit(i64),
    #[error("entitlement version must be >= 1, got {0}")]
    NonPositiveVersion(i64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_empty_key() {
        let result = PlanEntitlement::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "   ",
            10,
            1,
            Utc::now(),
        );
        assert_eq!(result, Err(PlanEntitlementError::EmptyKey));
    }

    #[test]
    fn new_rejects_negative_limit() {
        let result = PlanEntitlement::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MAX_UPTIME_MONITORS,
            -1,
            1,
            Utc::now(),
        );
        assert_eq!(result, Err(PlanEntitlementError::NegativeLimit(-1)));
    }

    #[test]
    fn new_accepts_valid() {
        let e = PlanEntitlement::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MAX_UPTIME_MONITORS,
            20,
            1,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(e.key, "max_uptime_monitors");
        assert_eq!(e.limit_value, 20);
    }
}
