use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::shared::ids::UserId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingConfig {
    pub version: u32,
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
    pub min_paid_monthly_price: Decimal,
    pub free_tier: FreeTierDefaults,
    pub currency: String,
    pub effective_at: DateTime<Utc>,
    pub avg_billable_network_bytes_per_check: u64,
    pub cost_per_gb_egress: Decimal,
    pub cost_safety_buffer: Decimal,
    pub sms_markup_multiplier: Decimal,
    pub enterprise_sla_monthly_price: Decimal,
    pub created_by: Option<UserId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeTierDefaults {
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
}

impl PricingConfig {
    pub fn default_config() -> Self {
        Self {
            version: 1,
            cost_per_check: Decimal::new(1, 6), // $0.000001
            effective_storage_bytes_per_check: 1024,
            cost_per_gb_hot: Decimal::new(21, 2),    // $0.21
            cost_per_gb_warm: Decimal::new(11, 2),   // $0.11
            cost_per_gb_cold: Decimal::new(4, 2),    // $0.04
            backup_multiplier: Decimal::new(25, 2),  // 0.25
            markup_multiplier: Decimal::new(200, 2), // 2.00
            min_interval_seconds: 5,
            max_monitors: 10000,
            max_retention_days: 3650,
            min_paid_monthly_price: Decimal::new(300, 2), // $3.00
            free_tier: FreeTierDefaults {
                monitors: 10,
                interval_seconds: 300,
                retention_days: 2,
            },
            currency: "USD".to_string(),
            effective_at: Utc::now(),
            avg_billable_network_bytes_per_check: 512,
            cost_per_gb_egress: Decimal::new(12, 2),  // $0.12
            cost_safety_buffer: Decimal::new(120, 2), // 1.20
            sms_markup_multiplier: Decimal::new(200, 2), // 2.00
            enterprise_sla_monthly_price: Decimal::new(9900, 2), // $99.00 initial default
            created_by: None,
        }
    }
}
