use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub database_url: String,
    pub lease_seconds: u64,
    pub max_concurrent_checks: usize,
    /// Maximum number of tenants processed concurrently during a retention
    /// sweep.
    pub retention_max_concurrent_tenants: usize,
    /// B23: consecutive failing checks required before an incident opens.
    pub incident_failure_threshold: u32,
    /// B23: consecutive successful checks required before an open
    /// incident resolves.
    pub incident_recovery_threshold: u32,
    /// B34: connection URL for the status-page response cache
    /// (`RedisStatusPageCache`). Optional, same reasoning as the API
    /// crate's `Config::redis_url`: a deployment that doesn't care about
    /// cache-invalidation-on-incident-change shouldn't be forced to stand
    /// up Redis just to run the worker. When absent, `main.rs` wires a
    /// no-op cache instead, and `IncidentManager` falls back to
    /// TTL-only staleness for public status pages (see
    /// `IncidentManager::with_status_page_cache_invalidation`).
    pub redis_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerConfigError {
    MissingDatabaseUrl,
    InvalidLeaseSeconds(String),
    InvalidMaxConcurrentChecks(String),
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
            Self::InvalidMaxConcurrentChecks(v) => write!(
                f,
                "WORKER_MAX_CONCURRENT_CHECKS must be a positive integer, got: {v}"
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
        let max_concurrent_checks = parse_env_usize(
            "WORKER_MAX_CONCURRENT_CHECKS",
            10,
            WorkerConfigError::InvalidMaxConcurrentChecks,
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
            max_concurrent_checks,
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
            env::remove_var("WORKER_MAX_CONCURRENT_CHECKS");
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
        assert_eq!(config.max_concurrent_checks, 10);
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
}
