use domain::pricing::PricingQuote;
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
