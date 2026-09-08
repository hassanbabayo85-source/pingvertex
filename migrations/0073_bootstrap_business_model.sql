-- PingVertex bootstrap business model.
-- Monetary pricing remains versioned in pricing_configs.
BEGIN;

-- The old CHECK constraint rejects the new identifiers, so remove it
-- BEFORE translating persisted rows.
ALTER TABLE subscriptions
    DROP CONSTRAINT IF EXISTS chk_subscriptions_plan_tier;

ALTER TABLE checkout_sessions
    DROP CONSTRAINT IF EXISTS chk_checkout_sessions_plan_tier;

UPDATE subscriptions
SET plan_tier = CASE
    WHEN plan_tier = 'tier_1'  THEN 'starter'
    WHEN plan_tier = 'tier_5'  THEN 'pro'
    WHEN plan_tier = 'tier_10' THEN 'business'
    WHEN plan_tier = 'tier_20' THEN 'enterprise'
    ELSE plan_tier
END;

UPDATE checkout_sessions
SET plan_tier = CASE
    WHEN plan_tier = 'tier_1'  THEN 'starter'
    WHEN plan_tier = 'tier_5'  THEN 'pro'
    WHEN plan_tier = 'tier_10' THEN 'business'
    WHEN plan_tier = 'tier_20' THEN 'enterprise'
    ELSE plan_tier
END;

ALTER TABLE subscriptions
    ADD CONSTRAINT chk_subscriptions_plan_tier
    CHECK (plan_tier IN ('free', 'starter', 'pro', 'business', 'enterprise'));

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
