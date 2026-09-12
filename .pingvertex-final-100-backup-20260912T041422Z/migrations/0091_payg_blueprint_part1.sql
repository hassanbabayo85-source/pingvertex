BEGIN;

-- Preserve historical pricing configs. PAYG migration creates a new effective
-- version instead of mutating an existing pricing snapshot.
INSERT INTO pricing_configs (
    id, version, currency, cost_per_check, effective_storage_bytes_per_check,
    cost_per_gb_hot, cost_per_gb_warm, cost_per_gb_cold, backup_multiplier,
    markup_multiplier, min_interval_seconds, max_monitors, max_retention_days,
    min_paid_monthly_price, free_monitors, free_interval_seconds,
    free_retention_days, created_at, effective_at, created_by
)
SELECT
    gen_random_uuid(), version + 1, currency, cost_per_check,
    effective_storage_bytes_per_check, cost_per_gb_hot, cost_per_gb_warm,
    cost_per_gb_cold, backup_multiplier, markup_multiplier,
    min_interval_seconds, max_monitors, max_retention_days,
    min_paid_monthly_price, 10, 300, free_retention_days,
    now(), now(), created_by
FROM pricing_configs
ORDER BY version DESC
LIMIT 1;

-- Versioned, server-controlled PAYG prices. Values are micro-USD per unit.
INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency,
     effective_from, effective_until, active)
VALUES
    (gen_random_uuid(), 3, 'http_check.one_time',       'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'tcp_check.one_time',        'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'dns_check.one_time',        'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'api_flow_check.one_time',   'request/check',4,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'proxy_check.one_time',      'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'http_check.monitoring',     'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'tcp_check.monitoring',      'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'dns_check.monitoring',      'check',       3,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'heartbeat_processing',      'check',       1,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'api_flow_check.monitoring', 'request/check',4,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'proxy_check.monitoring',    'check',       2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'rum_event',                 'event',       1,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'sms',                       'message',     1000, 'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'storage',                   'MB/month',    20,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 3, 'network_egress',            'MB',          100,  'USD', now(), NULL, TRUE)
ON CONFLICT (version, operation) DO NOTHING;

COMMIT;
