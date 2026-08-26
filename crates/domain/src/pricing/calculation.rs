use super::{CostLine, PricingConfig, PricingInput, PricingQuote, StorageCost, StorageTierCost};
use crate::pricing::{interval_label, retention_label};
use chrono::Utc;
use rust_decimal::Decimal;

const PERIOD_SECONDS: u64 = 2_592_000; // 30 days
const AVG_BILLABLE_NETWORK_BYTES_PER_CHECK: u64 = 512;
const BYTES_PER_GB: u64 = 1_073_741_824;
const COST_PER_GB_EGRESS: Decimal = Decimal::from_parts(12, 0, 0, false, 2); // $0.12
const COST_SAFETY_BUFFER: Decimal = Decimal::from_parts(120, 0, 0, false, 2); // 1.20

pub fn calculate_price(
    input: PricingInput,
    config: &PricingConfig,
) -> Result<PricingQuote, crate::pricing::PricingValidationError> {
    input.validate(config)?;

    if input.matches_free_tier(config) {
        return Ok(PricingQuote::free(&input, config));
    }

    let checks_per_period =
        (input.monitors as u128 * PERIOD_SECONDS as u128) / input.interval_seconds as u128;

    let base_compute = Decimal::from(checks_per_period) * config.cost_per_check;

    let base_network = Decimal::from(checks_per_period)
        * Decimal::from(AVG_BILLABLE_NETWORK_BYTES_PER_CHECK)
        / Decimal::from(BYTES_PER_GB)
        * COST_PER_GB_EGRESS;

    let storage = compute_storage(input, config);

    let base_backup = (base_compute + base_network + storage.base) * config.backup_multiplier;

    let adj_compute = base_compute * COST_SAFETY_BUFFER;
    let adj_network = base_network * COST_SAFETY_BUFFER;
    let adj_storage = storage.base * COST_SAFETY_BUFFER;
    let adj_backup = base_backup * COST_SAFETY_BUFFER;

    let infrastructure_cost = adj_compute + adj_network + adj_storage + adj_backup;
    let markup_amount = infrastructure_cost * (config.markup_multiplier - Decimal::ONE);
    let monthly_price_raw = infrastructure_cost + markup_amount;
    let monthly_price = if monthly_price_raw < config.min_paid_monthly_price {
        config.min_paid_monthly_price
    } else {
        monthly_price_raw
    };

    let round2 = |d: Decimal| d.round_dp(2);

    Ok(PricingQuote {
        is_free: false,
        monitors: input.monitors,
        interval_seconds: input.interval_seconds,
        retention_days: input.retention_days,
        interval_label: interval_label(input.interval_seconds),
        retention_label: retention_label(input.retention_days),
        checks_per_period: checks_per_period as u64,
        compute: CostLine {
            base: round2(base_compute),
            buffer: round2(adj_compute - base_compute),
            adjusted: round2(adj_compute),
        },
        network: CostLine {
            base: round2(base_network),
            buffer: round2(adj_network - base_network),
            adjusted: round2(adj_network),
        },
        storage: StorageCost {
            base: round2(storage.base),
            buffer: round2(adj_storage - storage.base),
            adjusted: round2(adj_storage),
            storage_breakdown: storage
                .storage_breakdown
                .into_iter()
                .map(|b| StorageTierCost {
                    tier: b.tier,
                    range_days: b.range_days,
                    volume_gb: b.volume_gb.round_dp(6),
                    base_cost: round2(b.base_cost),
                })
                .collect(),
        },
        backup: CostLine {
            base: round2(base_backup),
            buffer: round2(adj_backup - base_backup),
            adjusted: round2(adj_backup),
        },
        infrastructure_cost: round2(infrastructure_cost),
        markup_amount: round2(markup_amount),
        monthly_price: round2(monthly_price),
        currency: config.currency.clone(),
        pricing_version: config.version,
        generated_at: Utc::now(),
    })
}

fn compute_storage(input: PricingInput, config: &PricingConfig) -> StorageCost {
    let hot_days = input.retention_days.min(30) as u64;
    let warm_days = input.retention_days.saturating_sub(30).min(60) as u64;
    let cold_days = input.retention_days.saturating_sub(90) as u64;

    let hot_checks =
        (input.monitors as u128 * hot_days as u128 * 86400) / input.interval_seconds as u128;
    let warm_checks = input.monitors as u128 * warm_days as u128 * 24;
    let cold_checks = input.monitors as u128 * cold_days as u128;

    let bytes_to_gb = |checks: u128| -> Decimal {
        Decimal::from(checks) * Decimal::from(config.effective_storage_bytes_per_check)
            / Decimal::from(BYTES_PER_GB)
    };

    let hot_volume_gb = bytes_to_gb(hot_checks);
    let warm_volume_gb = bytes_to_gb(warm_checks);
    let cold_volume_gb = bytes_to_gb(cold_checks);

    let hot_cost = hot_volume_gb * config.cost_per_gb_hot;
    let warm_cost = warm_volume_gb * config.cost_per_gb_warm;
    let cold_cost = cold_volume_gb * config.cost_per_gb_cold;

    let base = hot_cost + warm_cost + cold_cost;

    let breakdown = vec![
        StorageTierCost {
            tier: "hot".to_string(),
            range_days: "0-30".to_string(),
            volume_gb: hot_volume_gb,
            base_cost: hot_cost,
        },
        StorageTierCost {
            tier: "warm".to_string(),
            range_days: "31-90".to_string(),
            volume_gb: warm_volume_gb,
            base_cost: warm_cost,
        },
        StorageTierCost {
            tier: "cold".to_string(),
            range_days: "91+".to_string(),
            volume_gb: cold_volume_gb,
            base_cost: cold_cost,
        },
    ];

    StorageCost {
        base,
        buffer: Decimal::ZERO,
        adjusted: base,
        storage_breakdown: breakdown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::PricingInput;
    use rust_decimal::Decimal;

    fn test_config() -> PricingConfig {
        PricingConfig::default_config()
    }

    #[test]
    fn free_tier_returns_zero() {
        let config = test_config();
        let input = PricingInput {
            monitors: 30,
            interval_seconds: 180,
            retention_days: 3,
        };
        let quote = calculate_price(input, &config).unwrap();
        assert!(quote.is_free);
        assert_eq!(quote.monthly_price, Decimal::ZERO);
    }

    #[test]
    fn above_free_tier_applies_minimum_price() {
        let config = test_config();
        let input = PricingInput {
            monitors: 31,
            interval_seconds: 180,
            retention_days: 3,
        };
        let quote = calculate_price(input, &config).unwrap();
        assert!(!quote.is_free);
        assert_eq!(quote.monthly_price, config.min_paid_monthly_price);
    }

    #[test]
    fn halving_interval_doubles_compute() {
        let config = test_config();
        let base = PricingInput {
            monitors: 100,
            interval_seconds: 60,
            retention_days: 30,
        };
        let faster = PricingInput {
            interval_seconds: 30,
            ..base
        };
        let base_quote = calculate_price(base, &config).unwrap();
        let faster_quote = calculate_price(faster, &config).unwrap();
        let ratio = faster_quote.compute.base / base_quote.compute.base;
        assert!(
            ratio > Decimal::from(180) / Decimal::from(100)
                && ratio < Decimal::from(220) / Decimal::from(100)
        );
    }

    #[test]
    fn increasing_retention_increases_storage() {
        let config = test_config();
        let short = PricingInput {
            monitors: 50,
            interval_seconds: 60,
            retention_days: 30,
        };
        let long = PricingInput {
            retention_days: 90,
            ..short
        };
        let short_quote = calculate_price(short, &config).unwrap();
        let long_quote = calculate_price(long, &config).unwrap();
        assert!(long_quote.storage.base > short_quote.storage.base);
    }

    #[test]
    fn validation_rejects_interval_below_min() {
        let config = test_config();
        let input = PricingInput {
            monitors: 10,
            interval_seconds: 4,
            retention_days: 30,
        };
        assert_eq!(
            calculate_price(input, &config),
            Err(crate::pricing::PricingValidationError::IntervalTooLow { min: 5 })
        );
    }

    #[test]
    fn validation_rejects_zero_monitors() {
        let config = test_config();
        let input = PricingInput {
            monitors: 0,
            interval_seconds: 60,
            retention_days: 30,
        };
        assert_eq!(
            calculate_price(input, &config),
            Err(crate::pricing::PricingValidationError::MonitorsZero)
        );
    }

    #[test]
    fn validation_rejects_exceeding_max_monitors() {
        let config = test_config();
        let input = PricingInput {
            monitors: config.max_monitors + 1,
            interval_seconds: 60,
            retention_days: 30,
        };
        assert_eq!(
            calculate_price(input, &config),
            Err(crate::pricing::PricingValidationError::MonitorsExceedMax {
                max: config.max_monitors
            })
        );
    }

    #[test]
    fn no_overflow_for_max_values() {
        let config = test_config();
        let input = PricingInput {
            monitors: config.max_monitors,
            interval_seconds: config.min_interval_seconds,
            retention_days: config.max_retention_days,
        };
        let quote = calculate_price(input, &config).unwrap();
        assert!(quote.monthly_price > Decimal::ZERO);
    }

    #[test]
    fn deterministic_output() {
        let config = test_config();
        let input = PricingInput {
            monitors: 500,
            interval_seconds: 30,
            retention_days: 90,
        };
        let q1 = calculate_price(input, &config).unwrap();
        let mut q2 = calculate_price(input, &config).unwrap();
        // `generated_at` is a real-time timestamp and will naturally differ
        // between the two calls; normalize it before comparing everything
        // else for determinism.
        q2.generated_at = q1.generated_at;
        assert_eq!(q1, q2);
    }
}
