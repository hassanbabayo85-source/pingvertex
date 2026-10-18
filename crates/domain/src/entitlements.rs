//! The immutable snapshot of a tenant's effective subscription
//! entitlements for the current billing state.
//!
//! `Entitlements` is the **authority** for numeric limits (quotas and
//! minimum intervals). It is produced by `EntitlementResolver` from
//! `plan_entitlements` rows for the tenant's current plan.
//!
//! The struct is not `Serialize` / `Deserialize` on purpose: it is a
//! runtime decision object, not a wire format. Serialization belongs in
//! a DTO layer, so the resolver contract cannot accidentally leak
//! internal plan details into an API response.

// NOTE: `plan_entitlements` is a private module inside `entities/mod.rs`,
// but `PlanEntitlement` and every `MAX_*` / `MIN_*` constant are
// `pub use`-re-exported from `crate::entities`. Import them at that
// level rather than reaching into the private module.
use crate::entities::{
    PlanEntitlement, MAX_API_FLOW_MONITORS, MAX_API_FLOW_STEPS, MAX_API_KEYS,
    MAX_CONCURRENT_PROXY_JOBS, MAX_HEARTBEAT_MONITORS, MAX_PROXY_ENTRIES_PER_JOB,
    MAX_RECURRING_PROXY_EXECUTIONS_PER_MONTH, MAX_RECURRING_PROXY_MONITORS,
    MAX_REGIONS_PER_MONITOR, MAX_RUM_EVENTS_PER_MONTH, MAX_RUM_SITES, MAX_STATUS_PAGES,
    MAX_TEAM_MEMBERS, MAX_UPTIME_MONITORS, MIN_RECURRING_PROXY_INTERVAL_SECONDS,
    MIN_UPTIME_INTERVAL_SECONDS,
};
use crate::enums::PlanTier;

// ─── Errors ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EntitlementResolutionError {
    #[error("plan entitlements for tier {tier:?} are missing required key '{key}'")]
    MissingKey { tier: PlanTier, key: &'static str },
    #[error("plan entitlement '{key}' has an unexpected negative limit {value}")]
    NegativeLimit { key: &'static str, value: i64 },
    #[error("plan entitlement '{key}' value {value} does not fit in u32")]
    ValueOutOfRange { key: &'static str, value: i64 },
    #[error("plan entitlements contain no rows; cannot resolve")]
    Empty,
    #[error("plan entitlement rows have inconsistent versions: {versions:?}")]
    VersionMismatch { versions: Vec<i64> },
}

// ─── Entitlements ──────────────────────────────────────────────────

/// A tenant's effective numeric entitlements. Every field is a hard
/// limit; the backend must reject any operation that would exceed one.
/// Money is *not* part of this struct — pricing lives in `plan_prices`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entitlements {
    pub plan_tier: PlanTier,
    pub entitlement_version: i64,

    // ── HTTP / TCP / DNS monitors (shared quota) ───────────────────
    pub max_uptime_monitors: u32,
    pub min_uptime_interval_seconds: u32,
    pub max_regions_per_monitor: u32,

    // ── Heartbeat monitors (separate quota) ────────────────────────
    pub max_heartbeat_monitors: u32,

    // ── API Flow (separate quota) ──────────────────────────────────
    pub max_api_flow_monitors: u32,
    pub max_api_flow_steps: u32,

    // ── One-time proxy jobs ────────────────────────────────────────
    pub max_proxy_entries_per_job: u32,
    pub max_concurrent_proxy_jobs: u32,

    // ── Recurring proxy monitors (Task 1 entities) ─────────────────
    pub max_recurring_proxy_monitors: u32,
    pub min_recurring_proxy_interval_seconds: u32,
    pub max_recurring_proxy_executions_per_month: u64,

    // ── Real User Monitoring ───────────────────────────────────────
    pub max_rum_sites: u32,
    pub max_rum_events_per_month: u64,

    // ── Tenant-scoped resource entitlements (TASK 4A) ──────────────
    pub max_status_pages: u32,
    pub max_api_keys: u32,
    pub max_team_members: u32,
}

impl Entitlements {
    /// Constructs the authoritative `Entitlements` snapshot from the
    /// `plan_entitlements` rows for the tenant's plan.
    ///
    /// Strict on purpose:
    ///   * every required key must be present, or
    ///     `MissingKey` is returned (no silent defaults — a missing
    ///     row is a migration bug, not a customer bug);
    ///   * every limit must be non-negative and fit in the target
    ///     integer type;
    ///   * every row must carry the same `version`, or
    ///     `VersionMismatch` is returned.
    ///
    /// The domain floor for recurring proxy intervals is enforced again
    /// here (`max(300)`) as a belt-and-braces check on top of the DB
    /// CHECK constraint from migration 0125.
    pub fn from_rows(
        plan_tier: PlanTier,
        rows: &[PlanEntitlement],
    ) -> Result<Self, EntitlementResolutionError> {
        if rows.is_empty() {
            return Err(EntitlementResolutionError::Empty);
        }

        // All rows for a plan should share a single version. Reject if
        // not — mixing versions would mean some entitlements are newer
        // than others, which is not a supported state.
        let first_version = rows[0].version;
        if rows.iter().any(|r| r.version != first_version) {
            let versions: Vec<i64> = rows.iter().map(|r| r.version).collect();
            return Err(EntitlementResolutionError::VersionMismatch { versions });
        }

        macro_rules! get {
            ($key:expr, $ty:ty, $field:ident) => {{
                let raw = rows
                    .iter()
                    .find(|r| r.key == $key)
                    .ok_or(EntitlementResolutionError::MissingKey {
                        tier: plan_tier,
                        key: $key,
                    })?
                    .limit_value;
                if raw < 0 {
                    return Err(EntitlementResolutionError::NegativeLimit {
                        key: $key,
                        value: raw,
                    });
                }
                let value: $ty = raw.try_into().map_err(|_| {
                    EntitlementResolutionError::ValueOutOfRange {
                        key: $key,
                        value: raw,
                    }
                })?;
                value
            }};
        }

        Ok(Self {
            plan_tier,
            entitlement_version: first_version,
            max_uptime_monitors: get!(MAX_UPTIME_MONITORS, u32, max_uptime_monitors),
            min_uptime_interval_seconds: get!(
                MIN_UPTIME_INTERVAL_SECONDS,
                u32,
                min_uptime_interval_seconds
            ),
            max_regions_per_monitor: get!(
                MAX_REGIONS_PER_MONITOR,
                u32,
                max_regions_per_monitor
            ),
            max_heartbeat_monitors: get!(
                MAX_HEARTBEAT_MONITORS,
                u32,
                max_heartbeat_monitors
            ),
            max_api_flow_monitors: get!(MAX_API_FLOW_MONITORS, u32, max_api_flow_monitors),
            max_api_flow_steps: get!(MAX_API_FLOW_STEPS, u32, max_api_flow_steps),
            max_proxy_entries_per_job: get!(
                MAX_PROXY_ENTRIES_PER_JOB,
                u32,
                max_proxy_entries_per_job
            ),
            max_concurrent_proxy_jobs: get!(
                MAX_CONCURRENT_PROXY_JOBS,
                u32,
                max_concurrent_proxy_jobs
            ),
            max_recurring_proxy_monitors: get!(
                MAX_RECURRING_PROXY_MONITORS,
                u32,
                max_recurring_proxy_monitors
            ),
            min_recurring_proxy_interval_seconds: std::cmp::max(
                300,
                get!(
                    MIN_RECURRING_PROXY_INTERVAL_SECONDS,
                    u32,
                    min_recurring_proxy_interval_seconds
                ),
            ),
            max_recurring_proxy_executions_per_month: get!(
                MAX_RECURRING_PROXY_EXECUTIONS_PER_MONTH,
                u64,
                max_recurring_proxy_executions_per_month
            ),
            max_rum_sites: get!(MAX_RUM_SITES, u32, max_rum_sites),
            max_rum_events_per_month: get!(
                MAX_RUM_EVENTS_PER_MONTH,
                u64,
                max_rum_events_per_month
            ),
            max_status_pages: get!(MAX_STATUS_PAGES, u32, max_status_pages),
            max_api_keys: get!(MAX_API_KEYS, u32, max_api_keys),
            max_team_members: get!(MAX_TEAM_MEMBERS, u32, max_team_members),
        })
    }

    pub fn is_free(&self) -> bool {
        matches!(self.plan_tier, PlanTier::Free)
    }

    /// v1 fixtures for every tier, matching migration 0125 (15 keys) plus
    /// migration 0127 (3 new keys). Test-only.

    #[doc(hidden)]
    pub fn free_v1_fixture() -> Self {
        Self {
            plan_tier: PlanTier::Free,
            entitlement_version: 1,
            max_uptime_monitors: 20,
            min_uptime_interval_seconds: 300,
            max_regions_per_monitor: 1,
            max_heartbeat_monitors: 5,
            max_api_flow_monitors: 0,
            max_api_flow_steps: 0,
            max_proxy_entries_per_job: 0,
            max_concurrent_proxy_jobs: 0,
            max_recurring_proxy_monitors: 0,
            min_recurring_proxy_interval_seconds: 300,
            max_recurring_proxy_executions_per_month: 0,
            max_rum_sites: 0,
            max_rum_events_per_month: 0,
            max_status_pages: 1,
            max_api_keys: 1,
            max_team_members: 1,
        }
    }

    #[doc(hidden)]
    pub fn starter_v1_fixture() -> Self {
        Self {
            plan_tier: PlanTier::Starter,
            entitlement_version: 1,
            max_uptime_monitors: 30,
            min_uptime_interval_seconds: 60,
            max_regions_per_monitor: 2,
            max_heartbeat_monitors: 10,
            max_api_flow_monitors: 2,
            max_api_flow_steps: 5,
            max_proxy_entries_per_job: 500,
            max_concurrent_proxy_jobs: 1,
            max_recurring_proxy_monitors: 0,
            min_recurring_proxy_interval_seconds: 300,
            max_recurring_proxy_executions_per_month: 0,
            max_rum_sites: 1,
            max_rum_events_per_month: 25_000,
            max_status_pages: 2,
            max_api_keys: 3,
            max_team_members: 3,
        }
    }

    #[doc(hidden)]
    pub fn pro_v1_fixture() -> Self {
        Self {
            plan_tier: PlanTier::Pro,
            entitlement_version: 1,
            max_uptime_monitors: 80,
            min_uptime_interval_seconds: 60,
            max_regions_per_monitor: 3,
            max_heartbeat_monitors: 30,
            max_api_flow_monitors: 10,
            max_api_flow_steps: 10,
            max_proxy_entries_per_job: 2_000,
            max_concurrent_proxy_jobs: 2,
            max_recurring_proxy_monitors: 5,
            min_recurring_proxy_interval_seconds: 300,
            max_recurring_proxy_executions_per_month: 43_200,
            max_rum_sites: 3,
            max_rum_events_per_month: 100_000,
            max_status_pages: 5,
            max_api_keys: 10,
            max_team_members: 10,
        }
    }

    #[doc(hidden)]
    pub fn business_v1_fixture() -> Self {
        Self {
            plan_tier: PlanTier::Business,
            entitlement_version: 1,
            max_uptime_monitors: 100,
            min_uptime_interval_seconds: 30,
            max_regions_per_monitor: 3,
            max_heartbeat_monitors: 50,
            max_api_flow_monitors: 30,
            max_api_flow_steps: 15,
            max_proxy_entries_per_job: 5_000,
            max_concurrent_proxy_jobs: 3,
            max_recurring_proxy_monitors: 10,
            min_recurring_proxy_interval_seconds: 300,
            max_recurring_proxy_executions_per_month: 86_400,
            max_rum_sites: 10,
            max_rum_events_per_month: 500_000,
            max_status_pages: 15,
            max_api_keys: 30,
            max_team_members: 25,
        }
    }

    #[doc(hidden)]
    pub fn enterprise_v1_fixture() -> Self {
        Self {
            plan_tier: PlanTier::Enterprise,
            entitlement_version: 1,
            max_uptime_monitors: 1_000,
            min_uptime_interval_seconds: 30,
            max_regions_per_monitor: 10,
            max_heartbeat_monitors: 500,
            max_api_flow_monitors: 100,
            max_api_flow_steps: 50,
            max_proxy_entries_per_job: 20_000,
            max_concurrent_proxy_jobs: 10,
            max_recurring_proxy_monitors: 100,
            min_recurring_proxy_interval_seconds: 300,
            max_recurring_proxy_executions_per_month: 500_000,
            max_rum_sites: 50,
            max_rum_events_per_month: 5_000_000,
            max_status_pages: 50,
            max_api_keys: 100,
            max_team_members: 100,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn row(key: &str, value: i64, version: i64) -> PlanEntitlement {
        PlanEntitlement::new(Uuid::new_v4(), Uuid::new_v4(), key, value, version, Utc::now())
            .unwrap()
    }

    /// Builds a complete set of v1 rows for `Free`, matching migration
    /// 0125's seed exactly.
    fn free_v1_rows() -> Vec<PlanEntitlement> {
        vec![
            row(MAX_UPTIME_MONITORS, 20, 1),
            row(MAX_HEARTBEAT_MONITORS, 5, 1),
            row(MIN_UPTIME_INTERVAL_SECONDS, 300, 1),
            row(MAX_REGIONS_PER_MONITOR, 1, 1),
            row(MAX_API_FLOW_MONITORS, 0, 1),
            row(MAX_API_FLOW_STEPS, 0, 1),
            row(MAX_PROXY_ENTRIES_PER_JOB, 0, 1),
            row(MAX_CONCURRENT_PROXY_JOBS, 0, 1),
            row(MAX_RECURRING_PROXY_MONITORS, 0, 1),
            row(MIN_RECURRING_PROXY_INTERVAL_SECONDS, 300, 1),
            row(MAX_RECURRING_PROXY_EXECUTIONS_PER_MONTH, 0, 1),
            row(MAX_RUM_SITES, 0, 1),
            row(MAX_RUM_EVENTS_PER_MONTH, 0, 1),
            row(MAX_STATUS_PAGES, 1, 1),
            row(MAX_API_KEYS, 1, 1),
            row(MAX_TEAM_MEMBERS, 1, 1),
        ]
    }

    #[test]
    fn parses_the_full_v1_free_seed_successfully() {
        let e = Entitlements::from_rows(PlanTier::Free, &free_v1_rows()).unwrap();
        assert_eq!(e.plan_tier, PlanTier::Free);
        assert_eq!(e.entitlement_version, 1);
        assert_eq!(e.max_uptime_monitors, 20);
        assert_eq!(e.min_uptime_interval_seconds, 300);
        assert_eq!(e.max_recurring_proxy_monitors, 0);
        assert_eq!(e.max_status_pages, 1);
        assert_eq!(e.max_api_keys, 1);
        assert_eq!(e.max_team_members, 1);
        assert!(e.is_free());
    }

    #[test]
    fn rejects_empty_rows() {
        assert_eq!(
            Entitlements::from_rows(PlanTier::Free, &[]),
            Err(EntitlementResolutionError::Empty)
        );
    }

    #[test]
    fn rejects_missing_key() {
        let mut rows = free_v1_rows();
        rows.retain(|r| r.key != MAX_UPTIME_MONITORS);
        let err = Entitlements::from_rows(PlanTier::Free, &rows).unwrap_err();
        assert!(matches!(
            err,
            EntitlementResolutionError::MissingKey {
                key: MAX_UPTIME_MONITORS,
                ..
            }
        ));
    }

    #[test]
    fn rejects_negative_limit() {
        // `PlanEntitlement::new` already rejects negatives, so a
        // negative-limit row can only reach `from_rows` by bypassing
        // the constructor (e.g. loaded directly from the database).
        // Build the struct literal directly to exercise `from_rows`'s
        // defensive check.
        let mut rows = free_v1_rows();
        rows.retain(|r| r.key != MAX_HEARTBEAT_MONITORS);
        rows.push(PlanEntitlement {
            id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            key: MAX_HEARTBEAT_MONITORS.to_owned(),
            limit_value: -1,
            version: 1,
            effective_from: Utc::now(),
            created_at: Utc::now(),
        });
        assert!(matches!(
            Entitlements::from_rows(PlanTier::Free, &rows),
            Err(EntitlementResolutionError::NegativeLimit { .. })
        ));
    }

    #[test]
    fn rejects_mixed_versions() {
        let mut rows = free_v1_rows();
        rows[0].version = 2;
        assert!(matches!(
            Entitlements::from_rows(PlanTier::Free, &rows),
            Err(EntitlementResolutionError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn recurring_proxy_interval_floor_is_enforced_at_300() {
        // Even if the DB somehow has 100, the resolver must return 300.
        let mut rows = free_v1_rows();
        rows.retain(|r| r.key != MIN_RECURRING_PROXY_INTERVAL_SECONDS);
        rows.push(row(MIN_RECURRING_PROXY_INTERVAL_SECONDS, 100, 1));
        let e = Entitlements::from_rows(PlanTier::Free, &rows).unwrap();
        assert_eq!(e.min_recurring_proxy_interval_seconds, 300);
    }
}
