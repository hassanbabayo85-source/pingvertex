use application::dto::AssertionInput;
use chrono::{DateTime, Utc};
use domain::entities::NotificationChannel;
use domain::enums::NotificationChannelKind;
use domain::shared::ids::UserId;
use domain::{Check, CheckStatus, ErrorType, HttpMethod, Incident, Monitor};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JSON representation of a monitor returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub enabled: bool,
    pub is_public: bool,
    pub ssl_check_enabled: bool,
    pub ssl_expiry_warning_days: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Monitor> for MonitorResponse {
    fn from(monitor: Monitor) -> Self {
        Self {
            id: monitor.id.0.to_string(),
            user_id: monitor.user_id.0.to_string(),
            name: monitor.name.as_str().to_owned(),
            url: monitor.url,
            method: monitor.method,
            expected_status: monitor.expected_status,
            interval_seconds: monitor.interval_seconds,
            timeout_seconds: monitor.timeout_seconds,
            enabled: monitor.enabled,
            is_public: monitor.is_public,
            ssl_check_enabled: monitor.ssl_check_enabled,
            ssl_expiry_warning_days: monitor.ssl_expiry_warning_days,
            created_at: monitor.created_at,
            updated_at: monitor.updated_at,
        }
    }
}

/// Shared request body shape for create/update endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct MonitorWriteRequest {
    pub user_id: Option<String>,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub expected_status: u16,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    #[serde(default)]
    pub assertions: Vec<AssertionInput>,
    /// Whether this monitor should be exposed on the unauthenticated public
    /// status page. Defaults to `false` if the client omits it.
    #[serde(default)]
    pub is_public: bool,
    /// Whether SSL certificate expiry should be checked for this monitor.
    /// Defaults to `true` if the client omits it.
    #[serde(default = "default_ssl_check_enabled")]
    pub ssl_check_enabled: bool,
    /// Number of days before certificate expiry at which a warning should
    /// be raised. Defaults to `14` if the client omits it.
    #[serde(default = "default_ssl_expiry_warning_days")]
    pub ssl_expiry_warning_days: u32,
}

fn default_ssl_check_enabled() -> bool {
    true
}

fn default_ssl_expiry_warning_days() -> u32 {
    14
}

impl MonitorWriteRequest {
    pub fn parse_user_id(&self) -> Result<UserId, String> {
        let raw = self
            .user_id
            .as_ref()
            .ok_or_else(|| "user_id is required".to_string())?;
        let id = Uuid::parse_str(raw).map_err(|_| "invalid user_id".to_string())?;
        Ok(UserId(id))
    }
}

/// A single incident, as exposed on the public status page.
#[derive(Debug, Clone, Serialize)]
pub struct PublicIncidentResponse {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// JSON representation of a monitor returned by the *public, unauthenticated*
/// status endpoint.
///
/// IMPORTANT: this type must only ever carry safe, operational fields. It
/// must NEVER include `user_id`, `assertions`, or any other owner-only or
/// internal data — this response is served to anonymous clients.
#[derive(Debug, Clone, Serialize)]
pub struct PublicMonitorResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    pub status: Option<CheckStatus>,
    pub latency_ms: Option<u128>,
    pub uptime_percentage: f64,
    pub incidents: Vec<PublicIncidentResponse>,
}

impl PublicMonitorResponse {
    /// Builds the public response from a monitor plus its checks/incidents.
    /// `checks` is expected to be ordered most-recent-first (same ordering
    /// already relied upon by the authenticated `/monitors/{id}/checks`
    /// endpoint).
    pub fn from_parts(monitor: Monitor, checks: &[Check], incidents: &[Incident]) -> Self {
        let latest = checks.first();

        let total = checks.len();
        let up = checks
            .iter()
            .filter(|check| check.status == CheckStatus::Up)
            .count();

        let uptime_percentage = if total == 0 {
            100.0
        } else {
            (up as f64 / total as f64) * 100.0
        };

        let mut incidents: Vec<PublicIncidentResponse> = incidents
            .iter()
            .map(|incident| PublicIncidentResponse {
                id: incident.id.0.to_string(),
                started_at: incident.started_at,
                resolved_at: incident.resolved_at,
            })
            .collect();
        incidents.sort_by_key(|b| std::cmp::Reverse(b.started_at));
        incidents.truncate(10);

        Self {
            id: monitor.id.0.to_string(),
            name: monitor.name.as_str().to_owned(),
            url: monitor.url,
            status: latest.map(|check| check.status),
            latency_ms: latest.map(|check| check.latency_ms),
            uptime_percentage,
            incidents,
        }
    }
}

/// Request body for `POST /monitors/{monitor_id}/notifications`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateNotificationChannelRequest {
    pub kind: NotificationChannelKind,
    pub target: String,
}

/// JSON representation of a notification channel, as returned by the list
/// and delete endpoints.
///
/// Deliberately has NO `signing_secret` field. See
/// `NotificationChannelCreateResponse` below for the one place a signing
/// secret is ever returned, and only once, at creation time — same
/// convention GitHub/Stripe use for webhook secrets.
#[derive(Debug, Clone, Serialize)]
pub struct NotificationChannelResponse {
    pub id: String,
    pub monitor_id: String,
    pub kind: NotificationChannelKind,
    pub target: String,
    pub enabled: bool,
    /// `true` for webhook channels (which always have a signing secret),
    /// `false` for email channels. Lets a client show "signing secret
    /// configured" in a UI without ever re-exposing the value itself.
    pub has_signing_secret: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<NotificationChannel> for NotificationChannelResponse {
    fn from(channel: NotificationChannel) -> Self {
        Self {
            id: channel.id.0.to_string(),
            monitor_id: channel.monitor_id.0.to_string(),
            kind: channel.kind,
            target: channel.target,
            enabled: channel.enabled,
            has_signing_secret: channel.signing_secret.is_some(),
            created_at: channel.created_at,
            updated_at: channel.updated_at,
        }
    }
}

/// Returned ONLY by `POST /monitors/{monitor_id}/notifications`.
///
/// Carries the plaintext `signing_secret` for webhook channels exactly
/// once. No other endpoint (list, get) ever returns it again — the caller
/// must save it now, or delete and recreate the channel to get a new one.
#[derive(Debug, Clone, Serialize)]
pub struct NotificationChannelCreateResponse {
    #[serde(flatten)]
    pub channel: NotificationChannelResponse,
    pub signing_secret: Option<String>,
}

impl NotificationChannelCreateResponse {
    pub fn from_created(channel: NotificationChannel) -> Self {
        let signing_secret = channel.signing_secret.clone();

        Self {
            channel: NotificationChannelResponse::from(channel),
            signing_secret,
        }
    }
}

/// JSON representation of a single diagnostic step.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticStepResponse {
    pub name: String,
    pub status: String,
    pub duration_ms: u64,
    pub detail: String,
}

impl From<domain::DiagnosticStep> for DiagnosticStepResponse {
    fn from(step: domain::DiagnosticStep) -> Self {
        Self {
            name: step.name,
            status: format!("{:?}", step.status),
            duration_ms: step.duration_ms,
            detail: step.detail,
        }
    }
}

/// JSON representation of a full diagnostic report.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReportResponse {
    pub id: String,
    pub check_id: String,
    pub steps: Vec<DiagnosticStepResponse>,
    pub root_cause_summary: String,
    pub created_at: DateTime<Utc>,
}

impl From<domain::DiagnosticReport> for DiagnosticReportResponse {
    fn from(report: domain::DiagnosticReport) -> Self {
        Self {
            id: report.id.0.to_string(),
            check_id: report.check_result_id.0.to_string(),
            steps: report
                .steps
                .into_iter()
                .map(DiagnosticStepResponse::from)
                .collect(),
            root_cause_summary: report.root_cause_summary,
            created_at: report.created_at,
        }
    }
}

/// JSON representation of a check, now with optional diagnostic report.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResponse {
    pub id: String,
    pub monitor_id: String,
    pub status: CheckStatus,
    pub status_code: Option<u16>,
    pub latency_ms: u128,
    pub error_type: ErrorType,
    pub error_message: Option<String>,
    pub checked_at: DateTime<Utc>,
    pub cert_expires_at: Option<DateTime<Utc>>,
    pub cert_days_remaining: Option<i64>,
    pub diagnostic_report: Option<DiagnosticReportResponse>,
}

impl From<application::dto::CheckWithDiagnosticReport> for CheckResponse {
    fn from(pair: application::dto::CheckWithDiagnosticReport) -> Self {
        let check = pair.check;
        Self {
            id: check.id.0.to_string(),
            monitor_id: check.monitor_id.0.to_string(),
            status: check.status,
            status_code: check.status_code,
            latency_ms: check.latency_ms,
            error_type: check.error_type,
            error_message: check.error_message,
            checked_at: check.checked_at,
            cert_expires_at: check.cert_expires_at,
            cert_days_remaining: check.cert_days_remaining,
            diagnostic_report: pair.diagnostic_report.map(DiagnosticReportResponse::from),
        }
    }
}

/// JSON representation of a status page returned by the authenticated API.
#[derive(Debug, Clone, Serialize)]
pub struct StatusPageResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub subdomain: String,
    pub custom_domain: Option<String>,
    pub custom_domain_verified: bool,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub monitor_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<domain::entities::StatusPage> for StatusPageResponse {
    fn from(status_page: domain::entities::StatusPage) -> Self {
        let (custom_domain, custom_domain_verified) = match &status_page.custom_domain {
            Some((domain, domain::entities::CustomDomainVerification::Verified { .. })) => {
                (Some(domain.as_str().to_owned()), true)
            }
            Some((domain, domain::entities::CustomDomainVerification::Pending { .. })) => {
                (Some(domain.as_str().to_owned()), false)
            }
            None => (None, false),
        };

        Self {
            id: status_page.id.0.to_string(),
            user_id: status_page.user_id.0.to_string(),
            name: status_page.name,
            subdomain: status_page.subdomain.as_str().to_owned(),
            custom_domain,
            custom_domain_verified,
            logo_url: status_page.logo_url,
            primary_color: status_page
                .primary_color
                .as_ref()
                .map(|color| color.as_str().to_owned()),
            monitor_ids: status_page
                .monitor_ids
                .iter()
                .map(|id| id.0.to_string())
                .collect(),
            created_at: status_page.created_at,
            updated_at: status_page.updated_at,
        }
    }
}

/// Request body for `POST /status-pages`.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusPageCreateRequest {
    pub name: String,
    pub subdomain: String,
    #[serde(default)]
    pub monitor_ids: Vec<String>,
}

/// Request body for `PUT /status-pages/{id}`. Does not include the
/// subdomain (immutable after creation) or the custom domain (has its own
/// dedicated endpoint, since it triggers a separate verification flow).
#[derive(Debug, Clone, Deserialize)]
pub struct StatusPageUpdateRequest {
    pub name: String,
    #[serde(default)]
    pub monitor_ids: Vec<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
}

/// Request body for `POST /status-pages/{id}/custom-domain`.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomDomainRequestBody {
    pub domain: String,
}

/// Response body for the unauthenticated public status page endpoint.
///
/// SECURITY: must never carry `user_id` or any other owner-only field —
/// same rule as `PublicMonitorResponse` above.
#[derive(Debug, Clone, Serialize)]
pub struct PublicStatusPageResponse {
    pub name: String,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub monitors: Vec<PublicMonitorResponse>,
}
