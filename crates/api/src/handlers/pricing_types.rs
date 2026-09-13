use chrono::{DateTime, Utc};
use domain::PricingAuditLogEntry;
use domain::pricing::{PricingConfig, PricingQuote};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct PricingQuoteRequest {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}

#[derive(Debug, Serialize)]
pub struct PricingQuoteResponse {
    pub is_free: bool,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub interval_label: String,
    pub retention_label: String,
    pub checks_per_period: u64,
    pub compute: CostLineResponse,
    pub network: CostLineResponse,
    pub storage: StorageCostResponse,
    pub backup: CostLineResponse,
    pub infrastructure_cost: String,
    pub markup_amount: String,
    pub monthly_price: String,
    pub currency: String,
    pub pricing_version: u32,
    pub generated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CostLineResponse {
    pub base: String,
    pub buffer: String,
    pub adjusted: String,
}

#[derive(Debug, Serialize)]
pub struct StorageCostResponse {
    pub base: String,
    pub buffer: String,
    pub adjusted: String,
    pub storage_breakdown: Vec<StorageTierCostResponse>,
}

#[derive(Debug, Serialize)]
pub struct StorageTierCostResponse {
    pub tier: String,
    pub range_days: String,
    pub volume_gb: String,
    pub base_cost: String,
}

impl From<PricingQuote> for PricingQuoteResponse {
    fn from(quote: PricingQuote) -> Self {
        let decimal_to_string = |d: rust_decimal::Decimal| d.to_string();
        Self {
            is_free: quote.is_free,
            monitors: quote.monitors,
            interval_seconds: quote.interval_seconds,
            retention_days: quote.retention_days,
            interval_label: quote.interval_label,
            retention_label: quote.retention_label,
            checks_per_period: quote.checks_per_period,
            compute: CostLineResponse {
                base: decimal_to_string(quote.compute.base),
                buffer: decimal_to_string(quote.compute.buffer),
                adjusted: decimal_to_string(quote.compute.adjusted),
            },
            network: CostLineResponse {
                base: decimal_to_string(quote.network.base),
                buffer: decimal_to_string(quote.network.buffer),
                adjusted: decimal_to_string(quote.network.adjusted),
            },
            storage: StorageCostResponse {
                base: decimal_to_string(quote.storage.base),
                buffer: decimal_to_string(quote.storage.buffer),
                adjusted: decimal_to_string(quote.storage.adjusted),
                storage_breakdown: quote
                    .storage
                    .storage_breakdown
                    .into_iter()
                    .map(|b| StorageTierCostResponse {
                        tier: b.tier,
                        range_days: b.range_days,
                        volume_gb: b.volume_gb.to_string(),
                        base_cost: b.base_cost.to_string(),
                    })
                    .collect(),
            },
            backup: CostLineResponse {
                base: decimal_to_string(quote.backup.base),
                buffer: decimal_to_string(quote.backup.buffer),
                adjusted: decimal_to_string(quote.backup.adjusted),
            },
            infrastructure_cost: decimal_to_string(quote.infrastructure_cost),
            markup_amount: decimal_to_string(quote.markup_amount),
            monthly_price: decimal_to_string(quote.monthly_price),
            currency: quote.currency,
            pricing_version: quote.pricing_version,
            generated_at: quote.generated_at.to_rfc3339(),
        }
    }
}

// ===== Admin Pricing Types =====

fn default_sms_markup_multiplier() -> Decimal {
    Decimal::new(200, 2)
}
fn default_enterprise_sla_price() -> Decimal {
    Decimal::new(9900, 2)
}

#[derive(Debug, Deserialize)]
pub struct CreatePricingConfigRequest {
    pub cost_per_check: Decimal,
    pub effective_storage_bytes_per_check: u32,
    pub cost_per_gb_hot: Decimal,
    pub cost_per_gb_warm: Decimal,
    pub cost_per_gb_cold: Decimal,
    pub backup_multiplier: Decimal,
    pub markup_multiplier: Decimal,
    pub min_interval_seconds: u32,
    pub max_monitors: u64,
    pub max_retention_days: u32,
    #[serde(default = "default_default_retention_days")]
    pub default_retention_days: u32,
    pub min_paid_monthly_price: Decimal,
    pub free_tier: FreeTierDefaultsRequest,
    pub currency: String,
    pub avg_billable_network_bytes_per_check: u64,
    pub cost_per_gb_egress: Decimal,
    pub cost_safety_buffer: Decimal,
    #[serde(default = "default_sms_markup_multiplier")]
    pub sms_markup_multiplier: Decimal,
    #[serde(default = "default_enterprise_sla_price")]
    pub enterprise_sla_monthly_price: Decimal,
    pub effective_at: Option<DateTime<Utc>>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct FreeTierDefaultsRequest {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}

#[derive(Debug, Serialize)]
pub struct PricingConfigResponse {
    pub version: u32,
    pub cost_per_check: String,
    pub effective_storage_bytes_per_check: u32,
    pub cost_per_gb_hot: String,
    pub cost_per_gb_warm: String,
    pub cost_per_gb_cold: String,
    pub backup_multiplier: String,
    pub markup_multiplier: String,
    pub min_interval_seconds: u32,
    pub max_monitors: u64,
    pub max_retention_days: u32,
    #[serde(default = "default_default_retention_days")]
    pub default_retention_days: u32,
    pub min_paid_monthly_price: String,
    pub free_tier: FreeTierDefaultsResponse,
    pub currency: String,
    pub effective_at: String,
    pub avg_billable_network_bytes_per_check: u64,
    pub cost_per_gb_egress: String,
    pub cost_safety_buffer: String,
    pub sms_markup_multiplier: String,
    pub enterprise_sla_monthly_price: String,
    pub created_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FreeTierDefaultsResponse {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}

impl From<PricingConfig> for PricingConfigResponse {
    fn from(config: PricingConfig) -> Self {
        let decimal_to_string = |d: Decimal| d.to_string();
        Self {
            version: config.version,
            cost_per_check: decimal_to_string(config.cost_per_check),
            effective_storage_bytes_per_check: config.effective_storage_bytes_per_check,
            cost_per_gb_hot: decimal_to_string(config.cost_per_gb_hot),
            cost_per_gb_warm: decimal_to_string(config.cost_per_gb_warm),
            cost_per_gb_cold: decimal_to_string(config.cost_per_gb_cold),
            backup_multiplier: decimal_to_string(config.backup_multiplier),
            markup_multiplier: decimal_to_string(config.markup_multiplier),
            min_interval_seconds: config.min_interval_seconds,
            max_monitors: config.max_monitors,
            max_retention_days: config.max_retention_days,
            default_retention_days: config.default_retention_days,
            min_paid_monthly_price: decimal_to_string(config.min_paid_monthly_price),
            free_tier: FreeTierDefaultsResponse {
                monitors: config.free_tier.monitors,
                interval_seconds: config.free_tier.interval_seconds,
                retention_days: config.free_tier.retention_days,
            },
            currency: config.currency,
            effective_at: config.effective_at.to_rfc3339(),
            avg_billable_network_bytes_per_check: config.avg_billable_network_bytes_per_check,
            cost_per_gb_egress: decimal_to_string(config.cost_per_gb_egress),
            cost_safety_buffer: decimal_to_string(config.cost_safety_buffer),
            sms_markup_multiplier: decimal_to_string(config.sms_markup_multiplier),
            enterprise_sla_monthly_price: decimal_to_string(config.enterprise_sla_monthly_price),
            created_by: config.created_by.map(|u| u.0.to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PricingAuditLogResponse {
    pub id: String,
    pub version: u32,
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub changed_by: Option<String>,
    pub changed_at: String,
    pub reason: Option<String>,
}

impl From<PricingAuditLogEntry> for PricingAuditLogResponse {
    fn from(entry: PricingAuditLogEntry) -> Self {
        Self {
            id: entry.id.0.to_string(),
            version: entry.version,
            field_name: entry.field_name,
            old_value: entry.old_value,
            new_value: entry.new_value,
            changed_by: entry.changed_by.map(|u| u.0.to_string()),
            changed_at: entry.changed_at.to_rfc3339(),
            reason: entry.reason,
        }
    }
}

/// Serde default for `default_retention_days`. Existing admin clients that
/// do not send this field get 30, matching `PricingConfig::default_config()`
/// and the `pricing_configs.default_retention_days` column default.
fn default_default_retention_days() -> u32 {
    30
}
