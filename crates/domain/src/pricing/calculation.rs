use super::{CostLine, PricingConfig, PricingInput, PricingQuote, StorageCost, StorageTierCost};
use crate::entities::SubscriptionUsagePeriod;
use crate::pricing::{interval_label, retention_label};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

const PERIOD_SECONDS: u64 = 2_592_000; // 30 days
const BYTES_PER_GB: u64 = 1_073_741_824;

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
        * Decimal::from(config.avg_billable_network_bytes_per_check)
        / Decimal::from(BYTES_PER_GB)
        * config.cost_per_gb_egress;

    let storage = compute_storage(input, config);

    let base_backup = (base_compute + base_network + storage.base) * config.backup_multiplier;

    let adj_compute = base_compute * config.cost_safety_buffer;
    let adj_network = base_network * config.cost_safety_buffer;
    let adj_storage = storage.base * config.cost_safety_buffer;
    let adj_backup = base_backup * config.cost_safety_buffer;

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

/// H12 — Proration Service.
///
/// Computes the (prorated) cost contributed by ONE `SubscriptionUsagePeriod`
/// to a given billing cycle `[cycle_start, cycle_end)`. Deliberately does
/// NOT re-implement the pricing formula: it calls `calculate_price` (H8's
/// full-cycle monthly quote for the period's usage) and then scales that
/// monthly price by the fraction of the cycle the period actually
/// overlaps. Full-cycle billing (the "monthly quote") is just the special
/// case of this function where a single period covers the entire cycle
/// — the overlap fraction is 1 and the result equals `calculate_price`'s
/// `monthly_price`.
///
/// Returns `Decimal::ZERO` if the period does not actually overlap the
/// cycle, or if the period's usage falls in the free tier.
pub fn calculate_period_cost(
    period: &SubscriptionUsagePeriod,
    config: &PricingConfig,
    cycle_start: DateTime<Utc>,
    cycle_end: DateTime<Utc>,
) -> Result<Decimal, crate::pricing::PricingValidationError> {
    let input = PricingInput {
        monitors: period.monitors,
        interval_seconds: period.interval_seconds,
        retention_days: period.retention_days,
    };

    let quote = calculate_price(input, config)?;

    if quote.is_free {
        return Ok(Decimal::ZERO);
    }

    let overlap_start = period.start_at.max(cycle_start);
    let overlap_end = period.end_at.unwrap_or(cycle_end).min(cycle_end);

    if overlap_end <= overlap_start {
        return Ok(Decimal::ZERO);
    }

    let overlap_seconds = (overlap_end - overlap_start).num_seconds().max(0);
    let cycle_seconds = (cycle_end - cycle_start).num_seconds().max(1);

    let fraction = Decimal::from(overlap_seconds) / Decimal::from(cycle_seconds);
    Ok((quote.monthly_price * fraction).round_dp(2))
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
    use crate::shared::ids::{SubscriptionId, SubscriptionUsagePeriodId};
    use rust_decimal::Decimal;

    fn test_config() -> PricingConfig {
        PricingConfig::default_config()
    }

    #[test]
    fn free_tier_returns_zero() {
        let config = test_config();
        let input = PricingInput {
            monitors: 20,
            interval_seconds: 300,
            retention_days: 2,
        };
        let quote = calculate_price(input, &config).unwrap();
        assert!(quote.is_free);
        assert_eq!(quote.monthly_price, Decimal::ZERO);
    }

    #[test]
    fn above_free_tier_applies_minimum_price() {
        let config = test_config();
        let input = PricingInput {
            monitors: 21,
            interval_seconds: 300,
            retention_days: 2,
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
    fn deterministic_output_excluding_generated_at() {
        let config = test_config();
        let input = PricingInput {
            monitors: 500,
            interval_seconds: 30,
            retention_days: 90,
        };
        let q1 = calculate_price(input, &config).unwrap();
        let q2 = calculate_price(input, &config).unwrap();

        // generated_at is a real-time timestamp and may differ between calls.
        // We assert on every other field to demonstrate true determinism of the
        // calculation logic without hiding the issue by overwriting the timestamp.
        assert_eq!(q1.is_free, q2.is_free);
        assert_eq!(q1.monitors, q2.monitors);
        assert_eq!(q1.interval_seconds, q2.interval_seconds);
        assert_eq!(q1.retention_days, q2.retention_days);
        assert_eq!(q1.interval_label, q2.interval_label);
        assert_eq!(q1.retention_label, q2.retention_label);
        assert_eq!(q1.checks_per_period, q2.checks_per_period);
        assert_eq!(q1.compute, q2.compute);
        assert_eq!(q1.network, q2.network);
        assert_eq!(q1.storage, q2.storage);
        assert_eq!(q1.backup, q2.backup);
        assert_eq!(q1.infrastructure_cost, q2.infrastructure_cost);
        assert_eq!(q1.markup_amount, q2.markup_amount);
        assert_eq!(q1.monthly_price, q2.monthly_price);
        assert_eq!(q1.currency, q2.currency);
        assert_eq!(q1.pricing_version, q2.pricing_version);
    }

    #[test]
    fn changing_config_fields_changes_cost() {
        let mut config = test_config();
        let input = PricingInput {
            monitors: 100,
            interval_seconds: 60,
            retention_days: 30,
        };
        let base_quote = calculate_price(input, &config).unwrap();

        // Increase avg_billable_network_bytes_per_check -> network cost increases
        config.avg_billable_network_bytes_per_check = 1024;
        let higher_network_quote = calculate_price(input, &config).unwrap();
        assert!(
            higher_network_quote.network.base > base_quote.network.base,
            "network base should increase when avg_billable_network_bytes_per_check increases"
        );

        // Reset config, then increase cost_safety_buffer -> adjusted costs increase
        let mut config2 = test_config();
        config2.cost_safety_buffer = Decimal::new(150, 2); // 1.50
        let higher_buffer_quote = calculate_price(input, &config2).unwrap();
        assert!(
            higher_buffer_quote.infrastructure_cost > base_quote.infrastructure_cost,
            "infrastructure cost should increase when safety buffer increases"
        );
    }

    // H12 — Proration Service: calculate_period_cost

    #[test]
    fn calculate_period_cost_for_a_full_cycle_period_equals_monthly_price() {
        let config = test_config();
        let cycle_start = Utc::now();
        let cycle_end = cycle_start + chrono::Duration::days(30);

        let period = SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            SubscriptionId::new(),
            100,
            60,
            30,
            cycle_start,
        );

        let input = PricingInput {
            monitors: 100,
            interval_seconds: 60,
            retention_days: 30,
        };
        let expected_monthly = calculate_price(input, &config).unwrap().monthly_price;

        let cost = calculate_period_cost(&period, &config, cycle_start, cycle_end).unwrap();

        assert_eq!(cost, expected_monthly);
    }

    #[test]
    fn calculate_period_cost_prorates_a_half_cycle_period() {
        let config = test_config();
        let cycle_start = Utc::now();
        let cycle_end = cycle_start + chrono::Duration::days(30);
        let midpoint = cycle_start + chrono::Duration::days(15);

        let mut period = SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            SubscriptionId::new(),
            100,
            60,
            30,
            cycle_start,
        );
        period.close(midpoint).unwrap();

        let input = PricingInput {
            monitors: 100,
            interval_seconds: 60,
            retention_days: 30,
        };
        let full_monthly = calculate_price(input, &config).unwrap().monthly_price;

        let cost = calculate_period_cost(&period, &config, cycle_start, cycle_end).unwrap();

        // Roughly half the full-cycle price (allow small rounding slack).
        let expected_half = (full_monthly / Decimal::from(2)).round_dp(2);
        let diff = (cost - expected_half).abs();
        assert!(
            diff <= Decimal::new(1, 2),
            "expected ~{expected_half}, got {cost}"
        );
    }

    #[test]
    fn calculate_period_cost_sums_correctly_across_two_periods_in_one_cycle() {
        let config = test_config();
        let cycle_start = Utc::now();
        let cycle_end = cycle_start + chrono::Duration::days(30);
        let switch_at = cycle_start + chrono::Duration::days(10);

        // Period 1: first 10 days, old usage (100 monitors).
        let mut period1 = SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            SubscriptionId::new(),
            100,
            60,
            30,
            cycle_start,
        );
        period1.close(switch_at).unwrap();

        // Period 2: remaining 20 days, new usage (200 monitors) — still open
        // at cycle end.
        let period2 = SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            period1.subscription_id,
            200,
            60,
            30,
            switch_at,
        );

        let cost1 = calculate_period_cost(&period1, &config, cycle_start, cycle_end).unwrap();
        let cost2 = calculate_period_cost(&period2, &config, cycle_start, cycle_end).unwrap();
        let total = cost1 + cost2;

        // Sanity: total proration must NOT equal full-month billing of
        // either the old usage alone or the new usage alone.
        let old_full_month = calculate_price(
            PricingInput {
                monitors: 100,
                interval_seconds: 60,
                retention_days: 30,
            },
            &config,
        )
        .unwrap()
        .monthly_price;
        let new_full_month = calculate_price(
            PricingInput {
                monitors: 200,
                interval_seconds: 60,
                retention_days: 30,
            },
            &config,
        )
        .unwrap()
        .monthly_price;

        assert!(total > Decimal::ZERO);
        assert_ne!(total, old_full_month);
        assert_ne!(total, new_full_month);
        assert!(total > old_full_month);
        assert!(total < new_full_month);
    }

    #[test]
    fn calculate_period_cost_returns_zero_for_a_period_outside_the_cycle() {
        let config = test_config();
        let cycle_start = Utc::now();
        let cycle_end = cycle_start + chrono::Duration::days(30);

        let mut period = SubscriptionUsagePeriod::open(
            SubscriptionUsagePeriodId::new(),
            SubscriptionId::new(),
            100,
            60,
            30,
            cycle_start - chrono::Duration::days(60),
        );
        period
            .close(cycle_start - chrono::Duration::days(30))
            .unwrap();

        let cost = calculate_period_cost(&period, &config, cycle_start, cycle_end).unwrap();
        assert_eq!(cost, Decimal::ZERO);
    }
}
