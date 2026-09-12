-- H9: Seed initial pricing config version 1.
-- Values must mirror domain::pricing::PricingConfig::default_config()
-- exactly so that Rust-level default and DB seed are identical.
 
INSERT INTO pricing_configs (
    id,
    version,
    currency,
    cost_per_check,
    effective_storage_bytes_per_check,
    cost_per_gb_hot,
    cost_per_gb_warm,
    cost_per_gb_cold,
    backup_multiplier,
    markup_multiplier,
    min_interval_seconds,
    max_monitors,
    max_retention_days,
    min_paid_monthly_price,
    free_monitors,
    free_interval_seconds,
    free_retention_days,
    avg_billable_network_bytes_per_check,
    cost_per_gb_egress,
    cost_safety_buffer,
    created_at,
    effective_at,
    created_by
) VALUES (
    gen_random_uuid(),        -- id
    1,                        -- version
    'USD',                    -- currency
    0.000001,                 -- cost_per_check ($0.000001)
    1024,                     -- effective_storage_bytes_per_check
    0.21,                     -- cost_per_gb_hot
    0.11,                     -- cost_per_gb_warm
    0.04,                     -- cost_per_gb_cold
    0.25,                     -- backup_multiplier
    2.00,                     -- markup_multiplier
    5,                        -- min_interval_seconds
    10000,                    -- max_monitors
    3650,                     -- max_retention_days
    3.00,                     -- min_paid_monthly_price
    30,                       -- free_monitors
    180,                      -- free_interval_seconds
    3,                        -- free_retention_days
    512,                      -- avg_billable_network_bytes_per_check
    0.12,                     -- cost_per_gb_egress
    1.20,                     -- cost_safety_buffer
    now(),                    -- created_at
    now(),                    -- effective_at (active immediately)
    NULL                      -- created_by (no user yet)
) ON CONFLICT (version) DO NOTHING;
