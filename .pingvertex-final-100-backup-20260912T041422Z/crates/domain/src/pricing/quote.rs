use super::{PricingConfig, PricingInput};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingQuote {
    pub is_free: bool,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub interval_label: String,
    pub retention_label: String,
    pub checks_per_period: u64,
    pub compute: CostLine,
    pub network: CostLine,
    pub storage: StorageCost,
    pub backup: CostLine,
    pub infrastructure_cost: Decimal,
    pub markup_amount: Decimal,
    pub monthly_price: Decimal,
    pub currency: String,
    pub pricing_version: u32,
    pub generated_at: DateTime<Utc>,
}

impl PricingQuote {
    pub fn free(input: &PricingInput, config: &PricingConfig) -> Self {
        Self {
            is_free: true,
            monitors: input.monitors,
            interval_seconds: input.interval_seconds,
            retention_days: input.retention_days,
            interval_label: interval_label(input.interval_seconds),
            retention_label: retention_label(input.retention_days),
            checks_per_period: 0,
            compute: CostLine::zero(),
            network: CostLine::zero(),
            storage: StorageCost::zero(),
            backup: CostLine::zero(),
            infrastructure_cost: Decimal::ZERO,
            markup_amount: Decimal::ZERO,
            monthly_price: Decimal::ZERO,
            currency: config.currency.clone(),
            pricing_version: config.version,
            generated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostLine {
    pub base: Decimal,
    pub buffer: Decimal,
    pub adjusted: Decimal,
}

impl CostLine {
    pub fn zero() -> Self {
        Self {
            base: Decimal::ZERO,
            buffer: Decimal::ZERO,
            adjusted: Decimal::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageCost {
    pub base: Decimal,
    pub buffer: Decimal,
    pub adjusted: Decimal,
    pub storage_breakdown: Vec<StorageTierCost>,
}

impl StorageCost {
    pub fn zero() -> Self {
        Self {
            base: Decimal::ZERO,
            buffer: Decimal::ZERO,
            adjusted: Decimal::ZERO,
            storage_breakdown: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageTierCost {
    pub tier: String,
    pub range_days: String,
    pub volume_gb: Decimal,
    pub base_cost: Decimal,
}

pub fn interval_label(seconds: u32) -> String {
    match seconds {
        15 => "Every 15 seconds".to_string(),
        30 => "Every 30 seconds".to_string(),
        60 => "Every 1 minute".to_string(),
        180 => "Every 3 minutes (Free tier default)".to_string(),
        300 => "Every 5 minutes".to_string(),
        other => format!("Every {} seconds", other),
    }
}

pub fn retention_label(days: u32) -> String {
    match days {
        90 => "90 days (3 months)".to_string(),
        180 => "180 days (6 months)".to_string(),
        365 => "365 days (1 year)".to_string(),
        other => format!("{} days", other),
    }
}
