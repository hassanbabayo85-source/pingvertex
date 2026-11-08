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

// ─── Entitlements ──────────────────────────────────────────────────

/// A tenant's effective numeric entitlements. Every field is a hard
/// limit; the backend must reject any operation that would exceed one.
/// Money is *not* part of this struct — pricing lives in `plan_prices`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entitlements {
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

    // ── History retention (Task 8C) ────────────────────────────────
    pub max_history_retention_days: u32,
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
    /// v1 fixtures for every tier, matching migration 0125 (15 keys) plus
    /// migration 0127 (3 new keys). Test-only.

    #[doc(hidden)]
    pub fn free_v1_fixture() -> Self {
        Self {
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
            max_history_retention_days: 2,
        }
    }

    #[doc(hidden)]
    pub fn starter_v1_fixture() -> Self {
        Self {
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
            max_history_retention_days: 30,
        }
    }

    #[doc(hidden)]
    pub fn pro_v1_fixture() -> Self {
        Self {
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
            max_history_retention_days: 90,
        }
    }

    #[doc(hidden)]
    pub fn business_v1_fixture() -> Self {
        Self {
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
            max_history_retention_days: 365,
        }
    }

    #[doc(hidden)]
    pub fn enterprise_v1_fixture() -> Self {
        Self {
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
            max_history_retention_days: 1095,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────
