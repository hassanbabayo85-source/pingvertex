-- Set the product-authoritative Free tier for existing installations.
-- Free: 20 monitors, 5-minute minimum interval, 2-day history retention.
-- This migration is intentionally separate from older seed/bootstrap migrations
-- so databases that already applied them converge to the current contract.

BEGIN;

UPDATE pricing_configs
SET free_monitors = 20,
    free_interval_seconds = 300,
    free_retention_days = 2
WHERE version = (
    SELECT version
    FROM pricing_configs
    WHERE effective_at <= now()
    ORDER BY version DESC
    LIMIT 1
);

COMMIT;
