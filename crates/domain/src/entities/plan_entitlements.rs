//! Versioned numeric entitlement attached to a plan.
//!
//! ResourceLimits are (key, limit_value) pairs, e.g.
//! `("max_uptime_monitors", 20)`.
//!
//! Versioning means a subscription can keep the entitlement_version it
//! was purchased with. Plan-level changes create a new version rather
//! than silently changing existing customers' limits.

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
