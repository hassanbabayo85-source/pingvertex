-- Worker 9/10: runtime PAYG operations that were present in the price-book
-- schema but had no live billing path.
--
-- Pricing rows are immutable, so this migration adds a new version instead
-- of mutating the original SMS seed row (which used uppercase `SMS`).
-- Runtime operation names are lowercase and stable.
INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency, effective_from, effective_until, active)
VALUES
    (gen_random_uuid(), 2, 'sms',            'message', 1000, 'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'heartbeat_ping', 'ping',       2, 'USD', now(), NULL, TRUE)
ON CONFLICT (version, operation) DO NOTHING;
