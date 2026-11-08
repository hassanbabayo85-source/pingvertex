use std::collections::HashSet;
use std::{error::Error, fmt, sync::Arc};

use crate::{
    dto::{BulkMonitorImportError, BulkMonitorImportPreview, BulkMonitorImportRow},
    ports::{BulkImportExistingMonitor, BulkImportMonitorSnapshot},
};
use domain::{
    CheckInterval, ExpectedStatus, Host, MonitorKind, MonitorName, Timeout, Url,
    shared::types::{AuthenticatedUser, Permission},
};

const SECONDS_PER_DAY: u64 = 86_400;
const MAX_FORECAST_DAYS: u32 = 3_650;

#[derive(Debug)]
pub enum BulkMonitorImportPreviewError {
    Forbidden,
    Invalid(&'static str),
    Overflow,
    Repository(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for BulkMonitorImportPreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create monitors"
            ),
            Self::Invalid(message) => write!(f, "{message}"),
            Self::Overflow => write!(f, "bulk import estimate exceeds supported numeric range"),
            Self::Repository(_) => write!(f, "failed to load bulk import preview data"),
        }
    }
}

impl Error for BulkMonitorImportPreviewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

pub struct PreviewBulkMonitorImportUseCase {
    tenant_repository: Option<Arc<dyn crate::ports::TenantRepository>>,
}

impl PreviewBulkMonitorImportUseCase {
    pub fn new() -> Self {
        Self {
            tenant_repository: None,
        }
    }

    pub fn with_tenant_repository(
        mut self,
        repository: Arc<dyn crate::ports::TenantRepository>,
    ) -> Self {
        self.tenant_repository = Some(repository);
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
        rows: Vec<BulkMonitorImportRow>,
        mut errors: Vec<BulkMonitorImportError>,
        snapshot: BulkImportMonitorSnapshot,
        forecast_days: u32,
    ) -> Result<BulkMonitorImportPreview, BulkMonitorImportPreviewError> {
        authenticated_user
            .require(Permission::CreateMonitor)
            .map_err(|_| BulkMonitorImportPreviewError::Forbidden)?;

        if forecast_days == 0 || forecast_days > MAX_FORECAST_DAYS {
            return Err(BulkMonitorImportPreviewError::Invalid(
                "forecast_days must be between 1 and 3650",
            ));
        }

        let total_rows = rows.len().saturating_add(errors.len());
        if total_rows == 0 {
            return Err(BulkMonitorImportPreviewError::Invalid(
                "CSV must contain at least one data row",
            ));
        }

        const FREE_MONITOR_LIMIT: u64 = 20;
        const FREE_MIN_INTERVAL_SECONDS: u32 = 300;

        let existing_signatures = snapshot
            .monitors
            .iter()
            .map(existing_signature)
            .collect::<HashSet<_>>();
        let existing_names = snapshot
            .monitors
            .iter()
            .map(|m| m.name.as_str().trim().to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let existing_targets = snapshot
            .monitors
            .iter()
            .map(|m| normalized_target(m.kind, &m.url))
            .collect::<HashSet<_>>();
        let mut seen_signatures = HashSet::new();
        let mut seen_names = HashSet::new();
        let mut seen_targets = HashSet::new();
        let mut duplicate_rows = 0usize;
        let free_remaining =
            FREE_MONITOR_LIMIT.saturating_sub(snapshot.total_count as u64) as usize;
        let mut accepted_free_rows = 0usize;
        let mut estimated_execution_quantity = 0u64;
        let mut valid_rows = 0usize;

        for row in rows {
            let mut row_errors = validate_row(&row);
            if row.interval_seconds < FREE_MIN_INTERVAL_SECONDS {
                row_errors.push(format!(
                    "interval_seconds must be at least {FREE_MIN_INTERVAL_SECONDS} seconds"
                ));
            }
            let signature = candidate_signature(&row);
            let normalized_name = row.name.trim().to_ascii_lowercase();
            let normalized_target_value = normalized_target(row.kind, &row.url);

            let mut duplicate = false;
            if existing_signatures.contains(&signature) {
                duplicate = true;
                row_errors
                    .push("duplicate monitor identity already exists in this tenant".to_owned());
            } else if !seen_signatures.insert(signature) {
                duplicate = true;
                row_errors.push(
                    "duplicate monitor identity appears more than once in this CSV".to_owned(),
                );
            }
            if existing_names.contains(&normalized_name) || !seen_names.insert(normalized_name) {
                duplicate = true;
                row_errors
                    .push("duplicate monitor name already exists in this tenant or CSV".to_owned());
            }
            if existing_targets.contains(&normalized_target_value)
                || !seen_targets.insert(normalized_target_value)
            {
                duplicate = true;
                row_errors.push(
                    "duplicate monitor target URL/host already exists in this tenant or CSV"
                        .to_owned(),
                );
            }
            if duplicate {
                duplicate_rows = duplicate_rows.saturating_add(1);
            }

            if row_errors.is_empty() && accepted_free_rows >= free_remaining {
                row_errors.push(format!(
                    "free tier monitor limit exceeded; only {free_remaining} additional monitor(s) are currently available"
                ));
            }

            let execution_interval = if row.kind == MonitorKind::Heartbeat {
                row.heartbeat_interval_seconds
                    .expect("heartbeat interval validated")
            } else {
                row.interval_seconds
            };
            let executions_per_region = SECONDS_PER_DAY
                .checked_div(u64::from(execution_interval))
                .and_then(|per_day| per_day.checked_mul(u64::from(forecast_days)))
                .ok_or(BulkMonitorImportPreviewError::Overflow)?;

            if !row_errors.is_empty() {
                errors.push(BulkMonitorImportError {
                    row: row.source_row,
                    errors: row_errors,
                });
                continue;
            }

            let row_executions = executions_per_region
                .checked_mul(row.regions.len() as u64)
                .ok_or(BulkMonitorImportPreviewError::Overflow)?;
            estimated_execution_quantity = estimated_execution_quantity
                .checked_add(row_executions)
                .ok_or(BulkMonitorImportPreviewError::Overflow)?;
            accepted_free_rows = accepted_free_rows.saturating_add(1);
            valid_rows = valid_rows.saturating_add(1);
        }

        errors.sort_by_key(|e| e.row);

        Ok(BulkMonitorImportPreview {
            total_rows,
            valid_rows,
            invalid_rows: total_rows.saturating_sub(valid_rows),
            duplicate_rows,
            forecast_days,
            estimated_execution_quantity,
            errors,
        })
    }
}

fn validate_row(row: &BulkMonitorImportRow) -> Vec<String> {
    let mut errors = Vec::new();

    if let Err(error) = MonitorName::new(row.name.clone()) {
        errors.push(error.to_string());
    }

    match row.kind {
        MonitorKind::Http | MonitorKind::Heartbeat => {
            if let Err(error) = Url::new(row.url.clone()) {
                errors.push(error.to_string());
            }
        }
        MonitorKind::Tcp | MonitorKind::Dns => {
            if let Err(error) = Host::new(row.url.clone()) {
                errors.push(error.to_string());
            }
        }
        MonitorKind::Flow => {
            if row.url.trim().is_empty() {
                errors.push("flow target is required".to_owned());
            }
        }
    }

    if let Err(error) = CheckInterval::new(row.interval_seconds) {
        errors.push(error.to_string());
    }
    if let Err(error) = Timeout::new(row.timeout_seconds) {
        errors.push(error.to_string());
    }
    if let Err(error) = ExpectedStatus::new(row.expected_status) {
        errors.push(error.to_string());
    }

    if row.kind == MonitorKind::Tcp && row.tcp_port == Some(0) {
        errors.push("tcp_port must be greater than zero".to_owned());
    }

    if row.kind == MonitorKind::Dns
        && let Some(record_type) = row.dns_record_type.as_deref()
    {
        const SUPPORTED: [&str; 6] = ["A", "AAAA", "CNAME", "TXT", "MX", "NS"];
        if !SUPPORTED.contains(&record_type.trim().to_ascii_uppercase().as_str()) {
            errors.push(format!(
                "unsupported DNS record type: {}",
                record_type.trim()
            ));
        }
    }

    if row.kind == MonitorKind::Heartbeat {
        match row.heartbeat_interval_seconds {
            Some(0) | None => {
                errors.push(
                    "heartbeat_interval_seconds must be greater than zero for Heartbeat monitors"
                        .to_owned(),
                );
            }
            Some(_) => {}
        }
        match row.heartbeat_grace_period_seconds {
            Some(0) | None => {
                errors.push(
                    "heartbeat_grace_period_seconds must be greater than zero for Heartbeat monitors"
                        .to_owned(),
                );
            }
            Some(_) => {}
        }
    }

    match row.kind {
        MonitorKind::Http | MonitorKind::Flow => {
            if row.tcp_port.is_some() {
                errors.push("tcp_port is only valid for Tcp monitors".to_owned());
            }
            if row.dns_record_type.is_some() || row.dns_expected_value.is_some() {
                errors.push("DNS fields are only valid for Dns monitors".to_owned());
            }
            if row.heartbeat_interval_seconds.is_some()
                || row.heartbeat_grace_period_seconds.is_some()
            {
                errors.push("heartbeat fields are only valid for Heartbeat monitors".to_owned());
            }
        }
        MonitorKind::Tcp => {
            if row.dns_record_type.is_some() || row.dns_expected_value.is_some() {
                errors.push("DNS fields are not valid for Tcp monitors".to_owned());
            }
            if row.heartbeat_interval_seconds.is_some()
                || row.heartbeat_grace_period_seconds.is_some()
            {
                errors.push("heartbeat fields are not valid for Tcp monitors".to_owned());
            }
        }
        MonitorKind::Dns => {
            if row.tcp_port.is_some() {
                errors.push("tcp_port is not valid for Dns monitors".to_owned());
            }
            if row.heartbeat_interval_seconds.is_some()
                || row.heartbeat_grace_period_seconds.is_some()
            {
                errors.push("heartbeat fields are not valid for Dns monitors".to_owned());
            }
        }
        MonitorKind::Heartbeat => {
            if row.tcp_port.is_some() {
                errors.push("tcp_port is not valid for Heartbeat monitors".to_owned());
            }
            if row.dns_record_type.is_some() || row.dns_expected_value.is_some() {
                errors.push("DNS fields are not valid for Heartbeat monitors".to_owned());
            }
        }
    }

    errors
}

fn normalized_target(kind: MonitorKind, target: &str) -> String {
    match kind {
        MonitorKind::Http | MonitorKind::Heartbeat => Url::new(target.to_owned())
            .map(|url| url.as_str().to_owned())
            .unwrap_or_else(|_| target.trim().to_owned()),
        MonitorKind::Tcp | MonitorKind::Dns => Host::new(target.to_owned())
            .map(|host| host.as_str().to_ascii_lowercase())
            .unwrap_or_else(|_| target.trim().to_ascii_lowercase()),
        MonitorKind::Flow => target.trim().to_owned(),
    }
}

fn candidate_signature(row: &BulkMonitorImportRow) -> String {
    format!(
        "{}|{:?}|{}|{:?}|{}|{}|{}|{:?}|{}|{}|{:?}|{:?}|{}|{}",
        row.name.trim().to_ascii_lowercase(),
        row.kind,
        normalized_target(row.kind, &row.url),
        row.method,
        row.expected_status,
        row.interval_seconds,
        row.timeout_seconds,
        row.tcp_port,
        row.dns_record_type
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase(),
        row.dns_expected_value
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
        row.heartbeat_interval_seconds,
        row.heartbeat_grace_period_seconds,
        row.regions
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(";"),
        row.auth_type.trim().to_ascii_lowercase(),
    )
}

fn existing_signature(monitor: &BulkImportExistingMonitor) -> String {
    format!(
        "{}|{:?}|{}|{:?}|{}|{}|{}|{:?}|{}|{}|{:?}|{:?}|{}|{}",
        monitor.name.trim().to_ascii_lowercase(),
        monitor.kind,
        normalized_target(monitor.kind, &monitor.url),
        monitor.method,
        monitor.expected_status,
        monitor.interval_seconds,
        monitor.timeout_seconds,
        monitor.tcp_port,
        monitor
            .dns_record_type
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase(),
        monitor
            .dns_expected_value
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
        monitor.heartbeat_interval_seconds,
        monitor.heartbeat_grace_period_seconds,
        monitor
            .regions
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(";"),
        "none",
    )
}
