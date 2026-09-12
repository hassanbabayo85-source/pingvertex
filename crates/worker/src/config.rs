use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub database_url: String,
    pub lease_seconds: u64,
    /// How often an in-flight check renews its scheduler lease and its
    /// PAYG reservation. Must be strictly less than `lease_seconds` so
    /// the lease is renewed before it expires. Defaults to
    /// `lease_seconds / 3`, floored at 5 seconds and clamped to
    /// `lease_seconds / 2` (or `lease_seconds` for very small values).
    pub heartbeat_interval_seconds: u64,
    pub max_concurrent_checks: usize,
    pub scheduler_max_concurrency_per_tenant: usize,
    pub scheduler_due_claim_limit_per_tenant: usize,
    pub scheduler_deterministic_spread_seconds: u64,
    pub retention_max_concurrent_tenants: usize,
    pub incident_failure_threshold: u32,
    pub incident_recovery_threshold: u32,
    pub redis_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerConfigError {
    MissingDatabaseUrl,
    InvalidLeaseSeconds(String),
    InvalidHeartbeatIntervalSeconds(String),
    InvalidMaxConcurrentChecks(String),
    InvalidSchedulerMaxConcurrencyPerTenant(String),
    InvalidSchedulerDueClaimLimitPerTenant(String),
    InvalidSchedulerDeterministicSpreadSeconds(String),
    InvalidRetentionMaxConcurrentTenants(String),
    InvalidIncidentFailureThreshold(String),
    InvalidIncidentRecoveryThreshold(String),
}

impl std::fmt::Display for WorkerConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDatabaseUrl => {
                write!(f, "DATABASE_URL environment variable is not set")
            }
            Self::InvalidLeaseSeconds(v) => write!(
                f,
                "WORKER_LEASE_SECONDS must be a positive integer, got: {v}"
            ),
            Self::InvalidHeartbeatIntervalSeconds(v) => write!(
                f,
                "WORKER_HEARTBEAT_INTERVAL_SECONDS must be a positive integer, got: {v}"
            ),
            Self::InvalidMaxConcurrentChecks(v) => write!(
                f,
                "WORKER_MAX_CONCURRENT_CHECKS must be a positive integer, got: {v}"
            ),
            Self::InvalidSchedulerMaxConcurrencyPerTenant(v) => write!(
                f,
                "SCHEDULER_MAX_CONCURRENCY_PER_TENANT must be a positive integer, got: {v}"
            ),
            Self::InvalidSchedulerDueClaimLimitPerTenant(v) => write!(
                f,
                "SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT must be a positive integer, got: {v}"
            ),
            Self::InvalidSchedulerDeterministicSpreadSeconds(v) => write!(
                f,
                "SCHEDULER_DETERMINISTIC_SPREAD_SECONDS must be a non-negative integer, got: {v}"
            ),
            Self::InvalidRetentionMaxConcurrentTenants(v) => write!(
                f,
                "RETENTION_MAX_CONCURRENT_TENANTS must be a positive integer, got: {v}"
            ),
            Self::InvalidIncidentFailureThreshold(v) => write!(
                f,
                "INCIDENT_FAILURE_THRESHOLD must be a positive integer, got: {v}"
            ),
            Self::InvalidIncidentRecoveryThreshold(v) => write!(
                f,
                "INCIDENT_RECOVERY_THRESHOLD must be a positive integer, got: {v}"
            ),
        }
    }
}

impl std::error::Error for WorkerConfigError {}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn parse_env_u64(key: &str, default: u64) -> Result<u64, WorkerConfigError> {
    match non_empty_env(key) {
        Some(v) => v
            .parse::<u64>()
            .map_err(|_| WorkerConfigError::InvalidLeaseSeconds(v.clone()))
            .and_then(|n| {
                if n == 0 {
                    Err(WorkerConfigError::InvalidLeaseSeconds(v))
                } else {
                    Ok(n)
                }
            }),
        None => Ok(default),
    }
}

fn parse_env_u64_with(
    key: &str,
    default: u64,
    err: impl Fn(String) -> WorkerConfigError,
) -> Result<u64, WorkerConfigError> {
    match non_empty_env(key) {
        Some(v) => v
            .parse::<u64>()
            .map_err(|_| err(v.clone()))
            .and_then(|n| if n == 0 { Err(err(v)) } else { Ok(n) }),
        None => Ok(default),
    }
}

fn parse_env_u64_nonnegative(
    key: &str,
    default: u64,
    err: impl Fn(String) -> WorkerConfigError,
) -> Result<u64, WorkerConfigError> {
    match non_empty_env(key) {
        Some(v) => v.parse::<u64>().map_err(|_| err(v.clone())),
        None => Ok(default),
    }
}

fn parse_env_usize(
    key: &str,
    default: usize,
    err: impl Fn(String) -> WorkerConfigError,
) -> Result<usize, WorkerConfigError> {
    match non_empty_env(key) {
        Some(v) => v
            .parse::<usize>()
            .map_err(|_| err(v.clone()))
            .and_then(|n| if n == 0 { Err(err(v)) } else { Ok(n) }),
        None => Ok(default),
    }
}

fn parse_env_u32(
    key: &str,
    default: u32,
    err: impl Fn(String) -> WorkerConfigError,
) -> Result<u32, WorkerConfigError> {
    match non_empty_env(key) {
        Some(v) => v
            .parse::<u32>()
            .map_err(|_| err(v.clone()))
            .and_then(|n| if n == 0 { Err(err(v)) } else { Ok(n) }),
        None => Ok(default),
    }
}

impl WorkerConfig {
    pub fn from_env() -> Self {
        Self::try_from_env().unwrap_or_else(|err| panic!("invalid worker configuration: {err}"))
    }

    pub fn try_from_env() -> Result<Self, WorkerConfigError> {
        let database_url =
            non_empty_env("DATABASE_URL").ok_or(WorkerConfigError::MissingDatabaseUrl)?;
        let lease_seconds = parse_env_u64("WORKER_LEASE_SECONDS", 60)?;
        // Default: one third of the lease, floored at 5s. Clamped so it can
        // never be >= the lease duration (which would allow the lease to
        // expire between heartbeats).
        let heartbeat_default = (lease_seconds / 3).max(5);
        let heartbeat_upper_bound = (lease_seconds / 2).max(1);
        let heartbeat_interval_seconds = parse_env_u64_with(
            "WORKER_HEARTBEAT_INTERVAL_SECONDS",
            heartbeat_default.min(heartbeat_upper_bound),
            WorkerConfigError::InvalidHeartbeatIntervalSeconds,
        )?
        .min(heartbeat_upper_bound);
        let max_concurrent_checks = parse_env_usize(
            "WORKER_MAX_CONCURRENT_CHECKS",
            10,
            WorkerConfigError::InvalidMaxConcurrentChecks,
        )?;
        let scheduler_max_concurrency_per_tenant = parse_env_usize(
            "SCHEDULER_MAX_CONCURRENCY_PER_TENANT",
            4,
            WorkerConfigError::InvalidSchedulerMaxConcurrencyPerTenant,
        )?;
        let scheduler_due_claim_limit_per_tenant = parse_env_usize(
            "SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT",
            4,
            WorkerConfigError::InvalidSchedulerDueClaimLimitPerTenant,
        )?;
        let scheduler_deterministic_spread_seconds = parse_env_u64_nonnegative(
            "SCHEDULER_DETERMINISTIC_SPREAD_SECONDS",
            5,
            WorkerConfigError::InvalidSchedulerDeterministicSpreadSeconds,
        )?;
        let retention_max_concurrent_tenants = parse_env_usize(
            "RETENTION_MAX_CONCURRENT_TENANTS",
            4,
            WorkerConfigError::InvalidRetentionMaxConcurrentTenants,
        )?;
        let incident_failure_threshold = parse_env_u32(
            "INCIDENT_FAILURE_THRESHOLD",
            3,
            WorkerConfigError::InvalidIncidentFailureThreshold,
        )?;
        let incident_recovery_threshold = parse_env_u32(
            "INCIDENT_RECOVERY_THRESHOLD",
            2,
            WorkerConfigError::InvalidIncidentRecoveryThreshold,
        )?;
        let redis_url = non_empty_env("REDIS_URL");

        Ok(Self {
            database_url,
            lease_seconds,
            heartbeat_interval_seconds,
            max_concurrent_checks,
            scheduler_max_concurrency_per_tenant,
            scheduler_due_claim_limit_per_tenant,
            scheduler_deterministic_spread_seconds,
            retention_max_concurrent_tenants,
            incident_failure_threshold,
            incident_recovery_threshold,
            redis_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial(worker_database_url_env)]
    fn fails_closed_when_database_url_missing() {
        unsafe {
            env::remove_var("DATABASE_URL");
        }
        assert_eq!(
            WorkerConfig::try_from_env(),
            Err(WorkerConfigError::MissingDatabaseUrl)
        );
    }

    #[test]
    #[serial(worker_database_url_env)]
    fn accepts_a_configured_database_url_with_defaults() {
        unsafe {
            env::set_var("DATABASE_URL", "postgres://user:pass@localhost:5432/db");
            env::remove_var("WORKER_LEASE_SECONDS");
            env::remove_var("WORKER_HEARTBEAT_INTERVAL_SECONDS");
            env::remove_var("WORKER_MAX_CONCURRENT_CHECKS");
            env::remove_var("SCHEDULER_MAX_CONCURRENCY_PER_TENANT");
            env::remove_var("SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT");
            env::remove_var("SCHEDULER_DETERMINISTIC_SPREAD_SECONDS");
            env::remove_var("RETENTION_MAX_CONCURRENT_TENANTS");
            env::remove_var("INCIDENT_FAILURE_THRESHOLD");
            env::remove_var("INCIDENT_RECOVERY_THRESHOLD");
            env::remove_var("REDIS_URL");
        }
        let config = WorkerConfig::try_from_env().expect("expected valid config");
        assert_eq!(
            config.database_url,
            "postgres://user:pass@localhost:5432/db"
        );
        assert_eq!(config.lease_seconds, 60);
        // 60 / 3 = 20, clamped to 60 / 2 = 30 -> 20.
        assert_eq!(config.heartbeat_interval_seconds, 20);
        assert_eq!(config.max_concurrent_checks, 10);
        assert_eq!(config.scheduler_max_concurrency_per_tenant, 4);
        assert_eq!(config.scheduler_due_claim_limit_per_tenant, 4);
        assert_eq!(config.scheduler_deterministic_spread_seconds, 5);
        assert_eq!(config.retention_max_concurrent_tenants, 4);
        assert_eq!(config.incident_failure_threshold, 3);
        assert_eq!(config.incident_recovery_threshold, 2);
        assert_eq!(config.redis_url, None);
        unsafe {
            env::remove_var("DATABASE_URL");
        }
    }

    #[test]
    #[serial(worker_database_url_env)]
    fn heartbeat_interval_cannot_reach_lease_duration() {
        unsafe {
            env::set_var("DATABASE_URL", "postgres://user:pass@localhost:5432/db");
            env::set_var("WORKER_LEASE_SECONDS", "60");
            env::set_var("WORKER_HEARTBEAT_INTERVAL_SECONDS", "600");
        }
        let config = WorkerConfig::try_from_env().expect("expected valid config");
        assert!(
            config.heartbeat_interval_seconds < config.lease_seconds,
            "heartbeat interval must be strictly less than lease duration"
        );
        unsafe {
            env::remove_var("DATABASE_URL");
            env::remove_var("WORKER_LEASE_SECONDS");
            env::remove_var("WORKER_HEARTBEAT_INTERVAL_SECONDS");
        }
    }

    #[test]
    #[serial(worker_database_url_env)]
    fn accepts_an_optional_redis_url() {
        unsafe {
            env::set_var("DATABASE_URL", "postgres://user:pass@localhost:5432/db");
            env::set_var("REDIS_URL", "redis://localhost:6379");
        }
        let config = WorkerConfig::try_from_env().expect("expected valid config");
        assert_eq!(config.redis_url, Some("redis://localhost:6379".to_string()));
        unsafe {
            env::remove_var("DATABASE_URL");
            env::remove_var("REDIS_URL");
        }
    }

    #[test]
    #[serial(worker_database_url_env)]
    fn rejects_a_zero_incident_failure_threshold() {
        unsafe {
            env::set_var("DATABASE_URL", "postgres://user:pass@localhost:5432/db");
            env::set_var("INCIDENT_FAILURE_THRESHOLD", "0");
        }
        assert_eq!(
            WorkerConfig::try_from_env(),
            Err(WorkerConfigError::InvalidIncidentFailureThreshold(
                "0".to_string()
            ))
        );
        unsafe {
            env::remove_var("DATABASE_URL");
            env::remove_var("INCIDENT_FAILURE_THRESHOLD");
        }
    }

    #[test]
    #[serial(worker_database_url_env)]
    fn accepts_scheduler_backpressure_overrides() {
        unsafe {
            env::set_var("DATABASE_URL", "postgres://user:pass@localhost:5432/db");
            env::set_var("SCHEDULER_MAX_CONCURRENCY_PER_TENANT", "7");
            env::set_var("SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT", "11");
            env::set_var("SCHEDULER_DETERMINISTIC_SPREAD_SECONDS", "19");
        }
        let config = WorkerConfig::try_from_env().expect("expected valid config");
        assert_eq!(config.scheduler_max_concurrency_per_tenant, 7);
        assert_eq!(config.scheduler_due_claim_limit_per_tenant, 11);
        assert_eq!(config.scheduler_deterministic_spread_seconds, 19);
        unsafe {
            env::remove_var("SCHEDULER_MAX_CONCURRENCY_PER_TENANT");
            env::remove_var("SCHEDULER_DUE_CLAIM_LIMIT_PER_TENANT");
            env::remove_var("SCHEDULER_DETERMINISTIC_SPREAD_SECONDS");
        }
    }
}
