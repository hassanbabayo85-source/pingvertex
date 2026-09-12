use std::collections::{HashMap, HashSet};
use std::{error::Error, fmt, sync::Arc};

use crate::{
    dto::{
        BulkMonitorImportError, BulkMonitorImportPreview, BulkMonitorImportPricingVersion,
        BulkMonitorImportRow,
    },
    ports::{
        BulkImportExistingMonitor, BulkImportMonitorSnapshot, MonitorIntervalPolicyRepository,
        PAYGPricingRepository, PricingConfigRepository, PricingContext, resolve_price,
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
    tenant_repository: Option<Arc<dyn crate::ports::TenantRepository>>,
    pricing_repository: Arc<dyn PAYGPricingRepository>,
    interval_policy_repository: Arc<dyn MonitorIntervalPolicyRepository>,
    pricing_config_repository: Option<Arc<dyn PricingConfigRepository>>,
}

impl PreviewBulkMonitorImportUseCase {
    pub fn new(
        pricing_repository: Arc<dyn PAYGPricingRepository>,
        interval_policy_repository: Arc<dyn MonitorIntervalPolicyRepository>,
    ) -> Self {
        Self {
            pricing_repository,
            interval_policy_repository,
            pricing_config_repository: None,
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

    pub fn with_pricing_config_repository(
        mut self,
        repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_config_repository = Some(repository);
        self
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

        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut account_type = None;
        if let Some(repo) = &self.tenant_repository
            && let Ok(Some(tenant)) = repo.find_by_id(tenant_id).await
        {
            account_type = Some(tenant.account_type);
        }

        // Free entitlement is sourced only from the active PAYG pricing
        // configuration. Subscription plans are not part of runtime PAYG.
        let effective_plan = if let Some(pricing_repo) = &self.pricing_config_repository {
            if let Some(config) = pricing_repo
                .find_active_config()
                .await
                .map_err(BulkMonitorImportPreviewError::Repository)?
            {
                let mut plan = domain::Plan::for_tier(domain::enums::PlanTier::Free);
                plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                plan.history_retention_days = config.free_tier.retention_days;
                plan
            } else {
                domain::Plan::for_tier(domain::enums::PlanTier::Free)
            }
        } else {
            domain::Plan::for_tier(domain::enums::PlanTier::Free)
        };

        let free_monitor_limit = effective_plan.max_monitors as u64;
        let free_min_interval_seconds = effective_plan.min_check_interval_seconds;

        let mut contextual_pricing_cache: HashMap<
            (String, MonitorKind, String, u64),
            Option<PAYGPricing>,
        > = HashMap::new();

        let existing_signatures = snapshot
            .monitors
            .iter()
            .map(existing_signature)
            .collect::<HashSet<_>>();
        let existing_names = snapshot
            .monitors
            .iter()
            .map(|monitor| monitor.name.as_str().trim().to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let existing_targets = snapshot
            .monitors
            .iter()
            .map(|monitor| normalized_target(monitor.kind, &monitor.url))
            .collect::<HashSet<_>>();
        let mut seen_signatures = HashSet::new();
        let mut seen_names = HashSet::new();
        let mut seen_targets = HashSet::new();
        let mut duplicate_rows = 0usize;
        let free_remaining =
            free_monitor_limit.saturating_sub(snapshot.total_count as u64) as usize;
        let mut accepted_free_rows = 0usize;

        let mut estimated_execution_quantity = 0u64;
        let mut estimated_payg_cost_units = 0i64;
        let mut pricing_versions = Vec::new();
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
                free_min_interval_seconds
            };

            if row.interval_seconds < min_interval {
                row_errors.push(format!(
                    "interval_seconds must be at least {min_interval} seconds for the current billing eligibility"
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

            if !payg_activated && row_errors.is_empty() && accepted_free_rows >= free_remaining {
                row_errors.push(format!(
                    "free tier monitor limit exceeded; only {free_remaining} additional monitor(s) are currently available"
                ));
            }

            let operation = domain::monitor_operation(row.kind, false)
                .as_str()
                .to_owned();

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

            let mut row_pricing = Vec::with_capacity(row.regions.len());
            for region in &row.regions {
                let region_key = region.as_str().to_owned();
                let cache_key = (operation.clone(), row.kind, region_key.clone(), 1);
                let pricing = if let Some(cached) = contextual_pricing_cache.get(&cache_key) {
                    cached.as_ref()
                } else {
                    let resolved = resolve_price(
                        self.pricing_repository.as_ref(),
                        &operation,
                        &PricingContext {
                            account_type,
                            region: Some(region.clone()),
                            monitor_kind: Some(row.kind),
                            // Actual runtime billing resolves one price per execution.
                            quantity: 1,
                        },
                    )
                    .await
                    .map_err(BulkMonitorImportPreviewError::Repository)?;
                    let resolved = resolved.filter(|p| p.unit_price > 0);
                    contextual_pricing_cache.insert(cache_key.clone(), resolved);
                    contextual_pricing_cache
                        .get(&cache_key)
                        .and_then(|pricing| pricing.as_ref())
                };

                let Some(pricing) = pricing else {
                    row_errors.push(format!(
                        "PAYG pricing unavailable for operation {operation}, region {region_key}"
                    ));
                    continue;
                };

                let region_cost = if payg_activated {
                    i64::try_from(executions_per_region)
                        .ok()
                        .and_then(|quantity| quantity.checked_mul(pricing.unit_price))
                        .ok_or(BulkMonitorImportPreviewError::Overflow)?
                } else {
                    0
                };

                row_pricing.push((
                    region.clone(),
                    executions_per_region,
                    pricing.clone(),
                    region_cost,
                ));
            }

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

            if payg_activated {
                let row_cost =
                    row_pricing
                        .iter()
                        .try_fold(0i64, |total, (_, _, _, region_cost)| {
                            total
                                .checked_add(*region_cost)
                                .ok_or(BulkMonitorImportPreviewError::Overflow)
                        })?;
                estimated_payg_cost_units = estimated_payg_cost_units
                    .checked_add(row_cost)
                    .ok_or(BulkMonitorImportPreviewError::Overflow)?;
            } else {
                accepted_free_rows = accepted_free_rows.saturating_add(1);
            }

            for (region, quantity, pricing, estimated_charge_units) in row_pricing {
                pricing_versions.push(BulkMonitorImportPricingVersion {
                    operation: operation.clone(),
                    account_type,
                    monitor_kind: row.kind,
                    region,
                    quantity,
                    unit_price: pricing.unit_price,
                    estimated_charge_units,
                    currency: pricing.currency,
                    version: pricing.version,
                });
            }
            valid_rows = valid_rows.saturating_add(1);
        }

        errors.sort_by_key(|error| error.row);

        pricing_versions.sort_by(|left, right| {
            left.operation
                .cmp(&right.operation)
                .then_with(|| left.region.as_str().cmp(right.region.as_str()))
                .then_with(|| left.version.cmp(&right.version))
        });

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
            pricing_snapshot_hash: String::new(),
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

#[cfg(test)]
mod tests {
    use domain::RegionCode;

    use super::*;
    use crate::ports::PaygPricingRepoResult;
    use crate::ports::PricingRuleContext;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use domain::shared::{
        ids::{TenantId, UserId},
        types::{Role, TenantContext},
    };
    use uuid::Uuid;

    struct TestPricing {
        rows: Vec<PAYGPricing>,
        region_prices: HashMap<String, (i64, i64)>,
    }

    #[async_trait]
    impl PAYGPricingRepository for TestPricing {
        async fn find_active_for_context(
            &self,
            operation: &str,
            context: &PricingContext,
        ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
            if let Some(region) = &context.region
                && let Some((unit_price, version)) = self.region_prices.get(region.as_str())
            {
                let mut pricing = self
                    .find_active(operation)
                    .await?
                    .ok_or_else(|| "missing test pricing".to_owned())?;
                pricing.unit_price = *unit_price;
                pricing.version = *version;
                return Ok(Some(pricing));
            }
            self.find_active(operation).await
        }

        async fn create_version_with_context(
            &self,
            operation: &str,
            unit_name: &str,
            unit_price: i64,
            currency: &str,
            effective_from: chrono::DateTime<chrono::Utc>,
            _context: &PricingRuleContext,
        ) -> PaygPricingRepoResult<PAYGPricing> {
            self.create_version(operation, unit_name, unit_price, currency, effective_from)
                .await
        }
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
                region_prices: HashMap::new(),
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
    async fn multi_region_pricing_sums_each_region_price() {
        let mut row = http_row(2, "api", "https://example.com");
        row.regions = vec![
            RegionCode::new("us-east").unwrap(),
            RegionCode::new("eu-west").unwrap(),
        ];
        let use_case = PreviewBulkMonitorImportUseCase::new(
            Arc::new(TestPricing {
                rows: vec![pricing("http_check.monitoring", 2, 7)],
                region_prices: HashMap::from([
                    ("us-east".to_owned(), (1, 11)),
                    ("eu-west".to_owned(), (3, 12)),
                ]),
            }),
            Arc::new(TestIntervals),
        );

        let preview = use_case
            .execute(
                user(Role::Admin),
                vec![row],
                Vec::new(),
                BulkImportMonitorSnapshot {
                    total_count: 0,
                    monitors: Vec::new(),
                },
                true,
                1,
            )
            .await
            .unwrap();

        assert_eq!(preview.estimated_execution_quantity, 2 * 1_440);
        assert_eq!(preview.estimated_payg_cost_units, 1_440 + 1_440 * 3);
        assert_eq!(preview.pricing_versions.len(), 2);
        let us_east = preview
            .pricing_versions
            .iter()
            .find(|pricing| pricing.region.as_str() == "us-east")
            .unwrap();
        let eu_west = preview
            .pricing_versions
            .iter()
            .find(|pricing| pricing.region.as_str() == "eu-west")
            .unwrap();
        assert_eq!(us_east.quantity, 1_440);
        assert_eq!(us_east.unit_price, 1);
        assert_eq!(eu_west.quantity, 1_440);
        assert_eq!(eu_west.unit_price, 3);
    }

    #[tokio::test]
    async fn free_tier_preview_enforces_free_interval_and_quota_without_charge() {
        let use_case = PreviewBulkMonitorImportUseCase::new(
            Arc::new(TestPricing {
                rows: vec![pricing("http_check.monitoring", 2, 7)],
                region_prices: HashMap::new(),
            }),
            Arc::new(TestIntervals),
        );
        let mut row = http_row(2, "api", "https://example.com");
        // Pull the free-tier minimum interval from the same source of truth
        // production uses (`Plan::for_tier(Free).min_check_interval_seconds`)
        // rather than a hardcoded domain constant, so this test cannot
        // silently drift from the pricing contract.
        row.interval_seconds =
            domain::Plan::for_tier(domain::enums::PlanTier::Free).min_check_interval_seconds;
        let preview = use_case
            .execute(
                user(Role::Admin),
                vec![row],
                Vec::new(),
                BulkImportMonitorSnapshot {
                    // Same reasoning as above: use the free-tier monitor limit
                    // that the production preview code resolves, not a domain constant.
                    total_count: domain::Plan::for_tier(domain::enums::PlanTier::Free).max_monitors
                        as usize,
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
    async fn viewer_without_create_permission_is_forbidden() {
        let use_case = PreviewBulkMonitorImportUseCase::new(
            Arc::new(TestPricing {
                rows: Vec::new(),
                region_prices: HashMap::new(),
            }),
            Arc::new(TestIntervals),
        );
        let result = use_case
            .execute(
                user(Role::Viewer),
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
