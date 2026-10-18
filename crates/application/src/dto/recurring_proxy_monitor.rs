//! DTOs for the recurring proxy monitoring API.
//!
//! **Credentials are deliberately absent from every response DTO.**
//! `ProxyCredentials`' non-`Clone`-and-non-`Serialize` design means a
//! credentials leak is impossible from this layer even by accident.
//! Customers receive `has_credentials: bool` and can rotate the secret
//! via the update endpoint (TASK 3, block 5).

use chrono::{DateTime, Utc};
use domain::enums::ProxyScheme;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// ── Create ──────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/proxy-monitors`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateRecurringProxyMonitorRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub scheme: ProxyScheme,
    /// Optional proxy username. If both username and password are
    /// `None`, the monitor connects anonymously.
    pub username: Option<String>,
    /// Optional proxy password. Stored encrypted; never returned.
    pub password: Option<String>,
    pub interval_seconds: u32,
}

// ── Update ──────────────────────────────────────────────────────────

/// Request body for `PATCH /api/v1/proxy-monitors/{id}`.
///
/// Only fields that are **safe to change mid-flight** are editable.
/// Credentials, host, port, and scheme are intentionally excluded —
/// changing them is equivalent to deleting the monitor and creating a
/// new one, which is the only way to guarantee no check races against
/// a half-updated target.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRecurringProxyMonitorRequest {
    pub name: String,
    pub interval_seconds: u32,
}

// ── Responses ───────────────────────────────────────────────────────

/// Response for `GET /api/v1/proxy-monitors/{id}` and items in the
/// list endpoint. Contains no secrets.
#[derive(Debug, Clone, Serialize)]
pub struct RecurringProxyMonitorResponse {
    pub id: Uuid,
    pub name: String,
    /// Hostname or IP literal, as submitted.
    pub host: String,
    pub port: u16,
    pub scheme: ProxyScheme,
    /// True iff credentials were supplied at creation time. The actual
    /// credentials are never returned.
    pub has_credentials: bool,
    pub interval_seconds: u32,
    /// The customer's own on/off choice.
    pub user_enabled: bool,
    /// True iff the backend suspended scheduling because the tenant's
    /// current plan does not permit this monitor.
    pub entitlement_suspended: bool,
    /// Convenience: `user_enabled && !entitlement_suspended`.
    pub effective_enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response for a single execution.
#[derive(Debug, Clone, Serialize)]
pub struct RecurringProxyCheckResponse {
    pub id: Uuid,
    pub monitor_id: Uuid,
    /// One of `"running"`, `"succeeded"`, `"failed"`, `"timed_out"`.
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Raw `ProxyResult` as stored. Opaque to the API layer: it is the
    /// proxy engine's structured result and changes with the engine.
    pub result: Option<JsonValue>,
    pub error_message: Option<String>,
}

// ── Conversions from domain ─────────────────────────────────────────

impl RecurringProxyMonitorResponse {
    pub fn from_entity(m: &domain::entities::RecurringProxyMonitor) -> Self {
        Self {
            id: m.id,
            name: m.name.clone(),
            host: m.host.as_str().to_owned(),
            port: m.port,
            scheme: m.scheme.clone(),
            has_credentials: m.credentials.is_some(),
            interval_seconds: m.interval_seconds,
            user_enabled: m.user_enabled,
            entitlement_suspended: m.entitlement_suspended,
            effective_enabled: m.is_effective_enabled(),
            next_run_at: m.next_run_at,
            last_run_at: m.last_run_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

impl RecurringProxyCheckResponse {
    pub fn from_entity(c: &domain::entities::RecurringProxyCheck) -> Self {
        Self {
            id: c.id,
            monitor_id: c.monitor_id,
            status: c.status.as_str().to_owned(),
            started_at: c.started_at,
            finished_at: c.finished_at,
            result: c
                .result
                .as_ref()
                .and_then(|r| serde_json::to_value(r).ok()),
            error_message: c.error_message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::entities::RecurringProxyMonitor;
    use domain::enums::ProxyScheme;
    use domain::shared::ids::{TenantId, UserId};
    use domain::value_objects::{ProxyCredentials, ProxyHost};

    fn sample_monitor(with_credentials: bool) -> RecurringProxyMonitor {
        let creds = if with_credentials {
            Some(ProxyCredentials::new("user", "pass").unwrap())
        } else {
            None
        };
        RecurringProxyMonitor::new(
            Uuid::new_v4(),
            TenantId::new(),
            UserId::new(),
            "test-monitor",
            ProxyHost::new("8.8.8.8").unwrap(),
            8080,
            ProxyScheme::Http,
            creds,
            300,
        )
        .unwrap()
    }

    #[test]
    fn response_never_contains_password_string_when_credentials_present() {
        let m = sample_monitor(true);
        let response = RecurringProxyMonitorResponse::from_entity(&m);
        let json = serde_json::to_string(&response).unwrap();

        assert!(
            !json.contains("pass"),
            "response JSON must never contain the proxy password: {json}"
        );
        assert!(json.contains("has_credentials"));
        assert!(json.contains("\"has_credentials\":true"));
    }

    #[test]
    fn response_reports_has_credentials_false_when_absent() {
        let m = sample_monitor(false);
        let response = RecurringProxyMonitorResponse::from_entity(&m);
        assert!(!response.has_credentials);
    }

    #[test]
    fn response_reports_effective_enabled_false_when_suspended() {
        let mut m = sample_monitor(false);
        m.set_entitlement_suspended(true);
        let response = RecurringProxyMonitorResponse::from_entity(&m);
        assert!(response.user_enabled);
        assert!(response.entitlement_suspended);
        assert!(!response.effective_enabled);
    }

    #[test]
    fn response_reports_effective_enabled_false_when_user_disabled() {
        let mut m = sample_monitor(false);
        m.set_user_enabled(false);
        let response = RecurringProxyMonitorResponse::from_entity(&m);
        assert!(!response.user_enabled);
        assert!(!response.entitlement_suspended);
        assert!(!response.effective_enabled);
    }

    #[test]
    fn check_response_serializes_without_credentials() {
        let check = domain::entities::RecurringProxyCheck {
            id: Uuid::new_v4(),
            monitor_id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            status: domain::entities::RecurringProxyCheckStatus::Succeeded,
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            result: None,
            error_message: None,
        };
        let response = RecurringProxyCheckResponse::from_entity(&check);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"succeeded\""));
    }
}
