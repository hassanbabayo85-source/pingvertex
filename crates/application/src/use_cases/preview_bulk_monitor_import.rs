use std::collections::{HashMap, HashSet};
use std::{error::Error, fmt, sync::Arc};

use crate::{
    dto::{
        BulkMonitorImportError, BulkMonitorImportPreview, BulkMonitorImportPricingVersion,
        BulkMonitorImportRow,
    },
    ports::{
        BulkImportExistingMonitor, BulkImportMonitorSnapshot, MonitorIntervalPolicyRepository,
        PAYGPricingRepository, PricingContext, resolve_price,
    },
};
use domain::{
    CheckInterval, ExpectedStatus, Host, MonitorKind, MonitorName, PAYGPricing, Timeout, Url,
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
    pricing_repository: Arc<dyn PAYGPricingRepository>,
    interval_policy_repository: Arc<dyn MonitorIntervalPolicyRepository>,
}

impl PreviewBulkMonitorImportUseCase {
    pub fn new(
        pricing_repository: Arc<dyn PAYGPricingRepository>,
        interval_policy_repository: Arc<dyn MonitorIntervalPolicyRepository>,
    ) -> Self {
        Self {
            pricing_repository,
            interval_policy_repository,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
        rows: Vec<BulkMonitorImportRow>,
        mut errors: Vec<BulkMonitorImportError>,
        snapshot: BulkImportMonitorSnapshot,
        payg_activated: bool,
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

        let active_pricing = self
            .pricing_repository
            .list_active()
            .await
            .map_err(BulkMonitorImportPreviewError::Repository)?;
        let mut generic_pricing_by_operation = HashMap::new();
        for pricing in active_pricing {
            if pricing.unit_price > 0 {
                generic_pricing_by_operation.insert(pricing.operation.clone(), pricing);
            }
        }
        let mut contextual_pricing_cache: HashMap<
            (String, MonitorKind, Option<String>),
            Option<PAYGPricing>,
        > = HashMap::new();

        let existing_signatures = snapshot
            .monitors
            .iter()
            .map(existing_signature)
            .collect::<HashSet<_>>();
        let mut seen_signatures = HashSet::new();
        let mut duplicate_rows = 0usize;
        let free_remaining =
            domain::FREE_MONITOR_LIMIT.saturating_sub(snapshot.total_count as u64) as usize;
        let mut accepted_free_rows = 0usize;

        let mut estimated_execution_quantity = 0u64;
        let mut estimated_payg_cost_units = 0i64;
        let mut used_pricing: HashMap<String, PAYGPricing> = HashMap::new();
        let mut valid_rows = 0usize;

        // Cache the server-owned minimum interval once per kind instead of
        // doing one repository call for every CSV row.
        let mut min_intervals: Vec<(MonitorKind, u32)> = Vec::new();

        for row in rows {
            let mut row_errors = validate_row(&row);

            let min_interval = if payg_activated {
                match min_intervals
                    .iter()
                    .find(|(kind, _)| *kind == row.kind)
                    .map(|(_, minimum)| *minimum)
                {
                    Some(minimum) => minimum,
                    None => {
                        let minimum = self
                            .interval_policy_repository
                            .minimum_interval_seconds(row.kind)
                            .await
                            .map_err(BulkMonitorImportPreviewError::Repository)?;
                        min_intervals.push((row.kind, minimum));
                        minimum
                    }
                }
            } else {
                domain::FREE_MIN_INTERVAL_SECONDS
            };

            if row.interval_seconds < min_interval {
                row_errors.push(format!(
                    "interval_seconds must be at least {min_interval} seconds for the current billing eligibility"
                ));
            }

            let signature = candidate_signature(&row);
            if existing_signatures.contains(&signature) {
                duplicate_rows = duplicate_rows.saturating_add(1);
                row_errors.push("duplicate monitor already exists in this tenant".to_owned());
            } else if !seen_signatures.insert(signature) {
                duplicate_rows = duplicate_rows.saturating_add(1);
                row_errors.push("duplicate monitor appears more than once in this CSV".to_owned());
            }

            if !payg_activated && row_errors.is_empty() && accepted_free_rows >= free_remaining {
                row_errors.push(format!(
                    "free tier monitor limit exceeded; only {free_remaining} additional monitor(s) are currently available"
                ));
            }

            let operation = domain::monitor_operation(row.kind, false)
                .as_str()
                .to_owned();
            let region_key = row.regions.first().map(ToString::to_string);
            let cache_key = (operation.clone(), row.kind, region_key.clone());
            let pricing = if let Some(cached) = contextual_pricing_cache.get(&cache_key) {
                cached.as_ref()
            } else {
                let resolved = resolve_price(
                    self.pricing_repository.as_ref(),
                    &operation,
                    &PricingContext {
                        account_type: None,
                        region: row.regions.first().cloned(),
                        monitor_kind: Some(row.kind),
                        quantity: 1,
                    },
                )
                .await
                .map_err(BulkMonitorImportPreviewError::Repository)?;
                let resolved = resolved.filter(|p| p.unit_price > 0);
                contextual_pricing_cache.insert(cache_key, resolved);
                contextual_pricing_cache
                    .get(&(operation.clone(), row.kind, region_key.clone()))
                    .and_then(|p| p.as_ref())
            };
            let pricing = pricing.or_else(|| generic_pricing_by_operation.get(&operation));
            if pricing.is_none() {
                row_errors.push(format!(
                    "PAYG pricing unavailable for operation {operation}"
                ));
            }

            if !row_errors.is_empty() {
                errors.push(BulkMonitorImportError {
                    row: row.source_row,
                    errors: row_errors,
                });
                continue;
            }

            let pricing = pricing.expect("pricing checked above");
            let execution_interval = if row.kind == MonitorKind::Heartbeat {
                row.heartbeat_interval_seconds
                    .expect("heartbeat interval validated")
            } else {
                row.interval_seconds
            };
            let executions_per_day = SECONDS_PER_DAY / u64::from(execution_interval);
            let row_executions = executions_per_day
                .checked_mul(u64::from(forecast_days))
                .ok_or(BulkMonitorImportPreviewError::Overflow)?;
            estimated_execution_quantity = estimated_execution_quantity
                .checked_add(row_executions)
                .ok_or(BulkMonitorImportPreviewError::Overflow)?;

            if payg_activated {
                let row_cost = i64::try_from(row_executions)
                    .ok()
                    .and_then(|quantity| quantity.checked_mul(pricing.unit_price))
                    .ok_or(BulkMonitorImportPreviewError::Overflow)?;
                estimated_payg_cost_units = estimated_payg_cost_units
                    .checked_add(row_cost)
                    .ok_or(BulkMonitorImportPreviewError::Overflow)?;
            } else {
                accepted_free_rows = accepted_free_rows.saturating_add(1);
            }

            used_pricing
                .entry(operation)
                .or_insert_with(|| pricing.clone());
            valid_rows = valid_rows.saturating_add(1);
        }

        errors.sort_by_key(|error| error.row);

        let mut pricing_versions = used_pricing
            .into_iter()
            .map(|(operation, pricing)| BulkMonitorImportPricingVersion {
                operation,
                unit_price: pricing.unit_price,
                currency: pricing.currency,
                version: pricing.version,
            })
            .collect::<Vec<_>>();
        pricing_versions.sort_by(|left, right| left.operation.cmp(&right.operation));

        let currency = pricing_versions
            .first()
            .map(|pricing| pricing.currency.clone())
            .unwrap_or_else(|| "USD".to_owned());
        if pricing_versions
            .iter()
            .any(|pricing| pricing.currency != currency)
        {
            return Err(BulkMonitorImportPreviewError::Invalid(
                "active PAYG pricing uses mixed currencies",
            ));
        }

        let pricing_version = pricing_versions
            .first()
            .map(|pricing| pricing.version)
            .filter(|version| {
                pricing_versions
                    .iter()
                    .all(|pricing| pricing.version == *version)
            });

        Ok(BulkMonitorImportPreview {
            total_rows,
            valid_rows,
            invalid_rows: total_rows.saturating_sub(valid_rows),
            duplicate_rows,
            forecast_days,
            estimated_execution_quantity,
            estimated_payg_cost_units,
            currency,
            pricing_version,
            pricing_versions,
            payg_eligible: payg_activated,
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

#[cfg(test)]
mod tests {
    use domain::RegionCode;

    use super::*;
    use crate::ports::PaygPricingRepoResult;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use domain::shared::{
        ids::{TenantId, UserId},
        types::{Role, TenantContext},
    };
    use uuid::Uuid;

    struct TestPricing {
        rows: Vec<PAYGPricing>,
    }

    #[async_trait]
    impl PAYGPricingRepository for TestPricing {
        async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
            Ok(self
                .rows
                .iter()
                .find(|pricing| pricing.operation == operation)
                .cloned())
        }

        async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
            Ok(self.rows.clone())
        }

        async fn create_version(
            &self,
            _operation: &str,
            _unit_name: &str,
            _unit_price: i64,
            _currency: &str,
            _effective_from: chrono::DateTime<chrono::Utc>,
        ) -> PaygPricingRepoResult<PAYGPricing> {
            Err("not used".into())
        }
    }

    struct TestIntervals;

    #[async_trait]
    impl MonitorIntervalPolicyRepository for TestIntervals {
        async fn minimum_interval_seconds(
            &self,
            _kind: MonitorKind,
        ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
            Ok(30)
        }
    }

    fn user(role: Role) -> AuthenticatedUser {
        AuthenticatedUser::new(
            UserId(Uuid::new_v4()),
            role,
            TenantContext::new(TenantId(Uuid::new_v4())),
        )
    }

    fn pricing(operation: &str, unit_price: i64, version: i64) -> PAYGPricing {
        PAYGPricing {
            id: Uuid::new_v4(),
            version,
            operation: operation.to_owned(),
            unit_name: "check".to_owned(),
            unit_price,
            currency: "USD".to_owned(),
            effective_from: Utc::now() - Duration::minutes(1),
            effective_until: None,
            active: true,
        }
    }

    fn http_row(source_row: usize, name: &str, url: &str) -> BulkMonitorImportRow {
        BulkMonitorImportRow {
            source_row,
            name: name.to_owned(),
            url: url.to_owned(),
            kind: MonitorKind::Http,
            interval_seconds: 60,
            timeout_seconds: 10,
            method: domain::HttpMethod::Get,
            expected_status: 200,
            tcp_port: None,
            dns_record_type: None,
            dns_expected_value: None,
            heartbeat_interval_seconds: None,
            heartbeat_grace_period_seconds: None,
            is_public: false,
            ssl_check_enabled: true,
            ssl_expiry_warning_days: 14,
            regions: vec![RegionCode::default_region()],
            auth_type: "none".to_owned(),
        }
    }

    #[tokio::test]
    async fn preview_is_read_only_math_and_detects_csv_duplicates() {
        let use_case = PreviewBulkMonitorImportUseCase::new(
            Arc::new(TestPricing {
                rows: vec![pricing("http_check.monitoring", 2, 7)],
            }),
            Arc::new(TestIntervals),
        );
        let preview = use_case
            .execute(
                user(Role::Admin),
                vec![
                    http_row(2, "api", "https://example.com"),
                    http_row(3, "api", "https://example.com"),
                ],
                Vec::new(),
                BulkImportMonitorSnapshot {
                    total_count: 0,
                    monitors: Vec::new(),
                },
                true,
                30,
            )
            .await
            .unwrap();

        assert_eq!(preview.total_rows, 2);
        assert_eq!(preview.valid_rows, 1);
        assert_eq!(preview.invalid_rows, 1);
        assert_eq!(preview.duplicate_rows, 1);
        assert_eq!(preview.estimated_execution_quantity, 43_200);
        assert_eq!(preview.estimated_payg_cost_units, 86_400);
        assert_eq!(preview.errors[0].row, 3);
        assert_eq!(preview.pricing_version, Some(7));
    }

    #[tokio::test]
    async fn free_tier_preview_enforces_free_interval_and_quota_without_charge() {
        let use_case = PreviewBulkMonitorImportUseCase::new(
            Arc::new(TestPricing {
                rows: vec![pricing("http_check.monitoring", 2, 7)],
            }),
            Arc::new(TestIntervals),
        );
        let mut row = http_row(2, "api", "https://example.com");
        row.interval_seconds = domain::FREE_MIN_INTERVAL_SECONDS;
        let preview = use_case
            .execute(
                user(Role::Admin),
                vec![row],
                Vec::new(),
                BulkImportMonitorSnapshot {
                    total_count: domain::FREE_MONITOR_LIMIT as usize,
                    monitors: Vec::new(),
                },
                false,
                30,
            )
            .await
            .unwrap();

        assert_eq!(preview.valid_rows, 0);
        assert_eq!(preview.estimated_payg_cost_units, 0);
        assert!(!preview.payg_eligible);
        assert!(preview.errors[0].errors[0].contains("free tier monitor limit"));
    }

    #[tokio::test]
    async fn member_without_create_permission_is_forbidden() {
        let use_case = PreviewBulkMonitorImportUseCase::new(
            Arc::new(TestPricing { rows: Vec::new() }),
            Arc::new(TestIntervals),
        );
        let result = use_case
            .execute(
                user(Role::Member),
                Vec::new(),
                Vec::new(),
                BulkImportMonitorSnapshot {
                    total_count: 0,
                    monitors: Vec::new(),
                },
                false,
                30,
            )
            .await;

        assert!(matches!(
            result,
            Err(BulkMonitorImportPreviewError::Forbidden)
        ));
    }
}
