//! Recurring proxy monitoring: a long-lived, tenant-owned monitor that
//! periodically checks a single proxy target.
//!
//! This is deliberately distinct from `ProxyCheckJob` / `ProxyEntry`
//! (which represent a one-time batch). The two are not interchangeable.
//!
//! The product-wide floor is 300 seconds (5 minutes) for every tier.
//! That floor is enforced both here (domain invariant) and in the
//! database (`recurring_proxy_monitors.interval_seconds >= 300`).
//!
//! ### Why these entities do NOT derive Serialize / Deserialize / Clone
//!
//! `ProxyHost` and `ProxyCredentials` are intentionally restricted value
//! objects (see `crates/domain/src/value_objects/`): they do not derive
//! `Clone`, `Eq`, `Serialize`, or `Deserialize` so that proxy secrets
//! cannot accidentally be copied into API responses, logs, or caches.
//! These entities therefore derive only `Debug` (which the value
//! objects still safely permit). Serialization for the API belongs in a
//! dedicated DTO layer (TASK 3+) that redacts credentials before writing
//! them to the wire.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::enums::ProxyScheme;
use crate::shared::ids::{TenantId, UserId};
use crate::value_objects::{ProxyCredentials, ProxyHost, ProxyResult};

/// System-wide minimum interval for recurring proxy monitoring.
pub const MIN_RECURRING_PROXY_INTERVAL_SECONDS: u32 = 300;

// ─── Monitor ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct RecurringProxyMonitor {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub user_id: UserId,
    pub name: String,
    pub host: ProxyHost,
    pub port: u16,
    pub scheme: ProxyScheme,
    pub credentials: Option<ProxyCredentials>,
    pub interval_seconds: u32,
    /// Customer's own enable/disable choice. Never overwritten by
    /// subscription state changes.
    pub user_enabled: bool,
    /// Backend-set flag: monitor exists but the current subscription
    /// does not permit it. Scheduling must skip it; the customer's
    /// `user_enabled` choice is preserved.
    pub entitlement_suspended: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RecurringProxyMonitor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        tenant_id: TenantId,
        user_id: UserId,
        name: impl Into<String>,
        host: ProxyHost,
        port: u16,
        scheme: ProxyScheme,
        credentials: Option<ProxyCredentials>,
        interval_seconds: u32,
    ) -> Result<Self, RecurringProxyMonitorError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(RecurringProxyMonitorError::EmptyName);
        }
        Self::validate_interval(interval_seconds)?;
        let now = Utc::now();
        Ok(Self {
            id,
            tenant_id,
            user_id,
            name,
            host,
            port,
            scheme,
            credentials,
            interval_seconds,
            user_enabled: true,
            entitlement_suspended: false,
            next_run_at: Some(now + chrono::Duration::seconds(i64::from(interval_seconds))),
            last_run_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    fn validate_interval(interval_seconds: u32) -> Result<(), RecurringProxyMonitorError> {
        if interval_seconds < MIN_RECURRING_PROXY_INTERVAL_SECONDS {
            return Err(RecurringProxyMonitorError::IntervalTooShort {
                got: interval_seconds,
            });
        }
        Ok(())
    }

    pub fn set_interval(
        &mut self,
        interval_seconds: u32,
    ) -> Result<(), RecurringProxyMonitorError> {
        Self::validate_interval(interval_seconds)?;
        self.interval_seconds = interval_seconds;
        self.next_run_at = Some(Utc::now() + chrono::Duration::seconds(i64::from(interval_seconds)));
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_user_enabled(&mut self, enabled: bool) {
        self.user_enabled = enabled;
        if enabled && !self.entitlement_suspended {
            self.next_run_at =
                Some(Utc::now() + chrono::Duration::seconds(i64::from(self.interval_seconds)));
        } else {
            self.next_run_at = None;
        }
        self.updated_at = Utc::now();
    }

    /// Backend flag applied by the entitlement engine. Never touches
    /// `user_enabled`.
    pub fn set_entitlement_suspended(&mut self, suspended: bool) {
        self.entitlement_suspended = suspended;
        if suspended {
            self.next_run_at = None;
        } else if self.user_enabled {
            self.next_run_at =
                Some(Utc::now() + chrono::Duration::seconds(i64::from(self.interval_seconds)));
        }
        self.updated_at = Utc::now();
    }

    /// True iff the scheduler is permitted to enqueue a new execution
    /// for this monitor.
    pub fn is_effective_enabled(&self) -> bool {
        self.user_enabled && !self.entitlement_suspended
    }

    pub fn record_execution_at(&mut self, when: DateTime<Utc>) {
        self.last_run_at = Some(when);
        if self.is_effective_enabled() {
            self.next_run_at =
                Some(when + chrono::Duration::seconds(i64::from(self.interval_seconds)));
        }
        self.updated_at = Utc::now();
    }
}

// ─── Per-execution history ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurringProxyCheckStatus {
    Running,
    Succeeded,
    Failed,
    TimedOut,
}

impl RecurringProxyCheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
        }
    }
}

impl std::str::FromStr for RecurringProxyCheckStatus {
    type Err = RecurringProxyMonitorError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "timed_out" => Ok(Self::TimedOut),
            other => Err(RecurringProxyMonitorError::UnknownCheckStatus(
                other.to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecurringProxyCheck {
    pub id: Uuid,
    pub monitor_id: Uuid,
    pub tenant_id: TenantId,
    pub status: RecurringProxyCheckStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<ProxyResult>,
    pub error_message: Option<String>,
}

impl RecurringProxyCheck {
    pub fn new_running(id: Uuid, monitor_id: Uuid, tenant_id: TenantId) -> Self {
        Self {
            id,
            monitor_id,
            tenant_id,
            status: RecurringProxyCheckStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            result: None,
            error_message: None,
        }
    }

    pub fn finish_succeeded(&mut self, result: ProxyResult) {
        self.status = RecurringProxyCheckStatus::Succeeded;
        self.finished_at = Some(Utc::now());
        self.result = Some(result);
        self.error_message = None;
    }

    pub fn finish_failed(&mut self, message: impl Into<String>) {
        self.status = RecurringProxyCheckStatus::Failed;
        self.finished_at = Some(Utc::now());
        self.error_message = Some(message.into());
    }

    pub fn finish_timed_out(&mut self, message: impl Into<String>) {
        self.status = RecurringProxyCheckStatus::TimedOut;
        self.finished_at = Some(Utc::now());
        self.error_message = Some(message.into());
    }
}

// ─── Error ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecurringProxyMonitorError {
    #[error(
        "recurring proxy interval must be at least {} seconds, got {}",
        MIN_RECURRING_PROXY_INTERVAL_SECONDS,
        got
    )]
    IntervalTooShort { got: u32 },
    #[error("recurring proxy monitor name cannot be empty")]
    EmptyName,
    #[error("unknown recurring proxy check status: {0}")]
    UnknownCheckStatus(String),
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> ProxyHost {
        ProxyHost::new("8.8.8.8").unwrap()
    }

    fn make(interval_seconds: u32) -> Result<RecurringProxyMonitor, RecurringProxyMonitorError> {
        RecurringProxyMonitor::new(
            Uuid::new_v4(),
            TenantId::new(),
            UserId::new(),
            "test monitor",
            host(),
            8080,
            ProxyScheme::Http,
            None,
            interval_seconds,
        )
    }

    #[test]
    fn interval_below_300_is_rejected() {
        assert!(matches!(
            make(299),
            Err(RecurringProxyMonitorError::IntervalTooShort { got: 299 })
        ));
    }

    #[test]
    fn interval_equal_to_300_is_accepted() {
        assert!(make(300).is_ok());
    }

    #[test]
    fn empty_name_is_rejected() {
        // assert on the error only, not the struct: the struct does not
        // (and deliberately cannot) derive PartialEq because ProxyHost
        // and ProxyCredentials do not.
        let err = RecurringProxyMonitor::new(
            Uuid::new_v4(),
            TenantId::new(),
            UserId::new(),
            "   ",
            host(),
            8080,
            ProxyScheme::Http,
            None,
            300,
        )
        .unwrap_err();
        assert_eq!(err, RecurringProxyMonitorError::EmptyName);
    }

    #[test]
    fn set_interval_rejects_below_300() {
        let mut m = make(300).unwrap();
        assert!(matches!(
            m.set_interval(299),
            Err(RecurringProxyMonitorError::IntervalTooShort { .. })
        ));
        assert_eq!(m.interval_seconds, 300);
    }

    #[test]
    fn user_disable_then_entitlement_suspend_preserves_user_choice() {
        let mut m = make(300).unwrap();
        m.set_user_enabled(false);
        assert!(!m.user_enabled);
        m.set_entitlement_suspended(true);
        // User's own choice must not be overwritten by entitlement logic.
        assert!(!m.user_enabled);
        assert!(m.entitlement_suspended);
        assert!(!m.is_effective_enabled());
    }

    #[test]
    fn entitlement_clear_does_not_reenable_user_disabled_monitor() {
        let mut m = make(300).unwrap();
        m.set_user_enabled(false);
        m.set_entitlement_suspended(true);
        m.set_entitlement_suspended(false);
        assert!(!m.user_enabled);
        assert!(!m.is_effective_enabled());
        assert!(m.next_run_at.is_none());
    }

    #[test]
    fn status_round_trips_through_str() {
        for s in [
            RecurringProxyCheckStatus::Running,
            RecurringProxyCheckStatus::Succeeded,
            RecurringProxyCheckStatus::Failed,
            RecurringProxyCheckStatus::TimedOut,
        ] {
            assert_eq!(
                s.as_str().parse::<RecurringProxyCheckStatus>().unwrap(),
                s
            );
        }
    }
}
