-- Initial PAYG price book.
-- One wallet unit is one micro-USD (10^-6 USD), stored as BIGINT.
-- Clients submit usage units only; the server resolves these prices.
INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency, effective_from, effective_until, active)
VALUES
    (gen_random_uuid(), 1, 'proxy_check',    'check',      2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'monitor_check',  'check',      2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'SMS',            'message',    1000, 'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'rum_event',      'event',      1,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'storage',        'MB-month',   20,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'network_egress', 'MB',         100,  'USD', now(), NULL, TRUE)
ON CONFLICT (version, operation) DO NOTHING;

