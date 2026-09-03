use chrono::{DateTime, Utc};
use domain::{CheckStatus, Incident, Monitor};
use serde::Serialize;

/// Summary counts of monitors for a tenant's dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorSummary {
    pub total: u32,
    pub up: u32,
    pub down: u32,
    pub disabled: u32,
}

/// Lightweight incident info embedded in monitor overview or recent list.
#[derive(Debug, Clone, Serialize)]
pub struct IncidentSummary {
    pub id: String,
    pub monitor_id: String,
    pub started_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

/// Per-monitor overview item shown on the dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorOverviewItem {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub status: Option<CheckStatus>,
    pub latency_ms: Option<u128>,
    pub checked_at: Option<DateTime<Utc>>,
    pub open_incident: Option<IncidentSummary>,
}

/// Summary of all incidents for the tenant, including recent list.
#[derive(Debug, Clone, Serialize)]
pub struct IncidentSummaryData {
    pub open_count: u32,
    pub recent: Vec<IncidentSummary>,
}

/// Full dashboard overview response.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardOverview {
    pub monitor_summary: MonitorSummary,
    pub monitors: Vec<MonitorOverviewItem>,
    pub incident_summary: IncidentSummaryData,
}

impl From<&Monitor> for MonitorOverviewItem {
    fn from(monitor: &Monitor) -> Self {
        Self {
            id: monitor.id.0.to_string(),
            name: monitor.name.as_str().to_owned(),
            url: monitor.url.clone(),
            enabled: monitor.enabled,
            status: None,
            latency_ms: None,
            checked_at: None,
            open_incident: None,
        }
    }
}

impl From<&Incident> for IncidentSummary {
    fn from(incident: &Incident) -> Self {
        Self {
            id: incident.id.0.to_string(),
            monitor_id: incident.monitor_id.0.to_string(),
            started_at: incident.started_at,
            acknowledged_at: incident.acknowledged_at,
        }
    }
}
