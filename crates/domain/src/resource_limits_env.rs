//! Environment-driven resource limits for self-hostable deployments.
//!
//! `ResourceLimits::from_env()` parses the `PV_*` environment variables
//! once and returns an immutable snapshot. The API and worker wire this
//! into an environment-driven resolver at startup; tests that need
//! per-tenant overrides wire the in-memory equivalent instead.

use crate::resource_limits::ResourceLimits;

#[derive(Debug, thiserror::Error)]
pub enum EnvConfigError {
    #[error("environment variable {0} is not a valid integer: {1}")]
    InvalidInteger(&'static str, std::num::ParseIntError),
    #[error("environment variable {0} is out of range for {1}")]
    OutOfRange(&'static str, &'static str),
}

fn env_u32(name: &'static str, default: u32) -> Result<u32, EnvConfigError> {
    match std::env::var(name) {
        Ok(v) => {
            let parsed: u64 = v
                .trim()
                .parse()
                .map_err(|e| EnvConfigError::InvalidInteger(name, e))?;
            u32::try_from(parsed).map_err(|_| EnvConfigError::OutOfRange(name, "u32"))
        }
        Err(_) => Ok(default),
    }
}

fn env_u64(name: &'static str, default: u64) -> Result<u64, EnvConfigError> {
    match std::env::var(name) {
        Ok(v) => v
            .trim()
            .parse()
            .map_err(|e| EnvConfigError::InvalidInteger(name, e)),
        Err(_) => Ok(default),
    }
}

impl ResourceLimits {
    pub fn from_env() -> Result<Self, EnvConfigError> {
        Ok(Self {
            max_uptime_monitors: env_u32("PV_MAX_UPTIME_MONITORS", 100)?,
            min_uptime_interval_seconds: env_u32("PV_MIN_UPTIME_INTERVAL_SECONDS", 60)?,
            max_regions_per_monitor: env_u32("PV_MAX_REGIONS_PER_MONITOR", 3)?,
            max_heartbeat_monitors: env_u32("PV_MAX_HEARTBEAT_MONITORS", 50)?,
            max_api_flow_monitors: env_u32("PV_MAX_API_FLOW_MONITORS", 30)?,
            max_api_flow_steps: env_u32("PV_MAX_API_FLOW_STEPS", 15)?,
            max_proxy_entries_per_job: env_u32("PV_MAX_PROXY_ENTRIES_PER_JOB", 5000)?,
            max_concurrent_proxy_jobs: env_u32("PV_MAX_CONCURRENT_PROXY_JOBS", 3)?,
            max_recurring_proxy_monitors: env_u32("PV_MAX_RECURRING_PROXY_MONITORS", 10)?,
            min_recurring_proxy_interval_seconds: std::cmp::max(
                300,
                env_u32("PV_MIN_RECURRING_PROXY_INTERVAL_SECONDS", 300)?,
            ),
            max_recurring_proxy_executions_per_month: env_u64(
                "PV_MAX_RECURRING_PROXY_EXECUTIONS_PER_MONTH",
                86400,
            )?,
            max_rum_sites: env_u32("PV_MAX_RUM_SITES", 10)?,
            max_rum_events_per_month: env_u64("PV_MAX_RUM_EVENTS_PER_MONTH", 500000)?,
            max_status_pages: env_u32("PV_MAX_STATUS_PAGES", 15)?,
            max_api_keys: env_u32("PV_MAX_API_KEYS", 30)?,
            max_team_members: env_u32("PV_MAX_TEAM_MEMBERS", 25)?,
            max_history_retention_days: env_u32("PV_MAX_HISTORY_RETENTION_DAYS", 90)?,
        })
    }
}
