-- PingVertex bootstrap business model.
-- This migration only changes entitlement identifiers and safe Free defaults.
-- Monetary pricing remains versioned in pricing_configs and is NOT finalized here.

BEGIN;

UPDATE subscriptions SET plan_tier = 'starter' WHERE plan_tier = 'tier_1';
UPDATE subscriptions SET plan_tier = 'pro' WHERE plan_tier = 'tier_5';
UPDATE subscriptions SET plan_tier = 'business' WHERE plan_tier = 'tier_10';
UPDATE subscriptions SET plan_tier = 'enterprise' WHERE plan_tier = 'tier_20';

UPDATE checkout_sessions SET plan_tier = 'starter' WHERE plan_tier = 'tier_1';
UPDATE checkout_sessions SET plan_tier = 'pro' WHERE plan_tier = 'tier_5';
UPDATE checkout_sessions SET plan_tier = 'business' WHERE plan_tier = 'tier_10';
UPDATE checkout_sessions SET plan_tier = 'enterprise' WHERE plan_tier = 'tier_20';

ALTER TABLE subscriptions DROP CONSTRAINT IF EXISTS chk_subscriptions_plan_tier;
ALTER TABLE subscriptions
    ADD CONSTRAINT chk_subscriptions_plan_tier
    CHECK (plan_tier IN ('free', 'starter', 'pro', 'business', 'enterprise'));

ALTER TABLE checkout_sessions DROP CONSTRAINT IF EXISTS chk_checkout_sessions_plan_tier;
ALTER TABLE checkout_sessions
    ADD CONSTRAINT chk_checkout_sessions_plan_tier
    CHECK (plan_tier IN ('free', 'starter', 'pro', 'business', 'enterprise'));

-- Bootstrap-safe Free tier: 5 monitors, 10-minute checks, 24-hour retention.
UPDATE pricing_configs
SET free_monitors = 5,
    free_interval_seconds = 600,
    free_retention_days = 1
WHERE version = (SELECT MAX(version) FROM pricing_configs);

COMMIT;
