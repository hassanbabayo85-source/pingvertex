-- 0130_retention_entitlement.sql
-- Task 8C — add `max_history_retention_days` to plan_entitlements.
--
-- This is the single authoritative source for the retention-worker's
-- cutoff per tenant. It replaces the PAYG-era chain of
-- (WalletRepository + PricingConfig.free_tier + PricingConfig.default_retention_days).
--
-- Values match `domain::entities::plan::Plan::for_tier().history_retention_days`
-- for Free/Starter/Pro/Business. Enterprise is set to 1095 (3 years) as a
-- conservative floor; Enterprise contracts are negotiated individually.
--
-- Added at version 1 alongside the existing keys so that
-- Entitlements::from_rows (which requires all keys at the same version)
-- continues to resolve for every tier. No existing row is modified or
-- deleted.

DO $$
DECLARE
    tiers TEXT[] := ARRAY['free', 'starter', 'pro', 'business', 'enterprise'];
    days  INT[] := ARRAY[2, 30, 90, 365, 1095];
    i     INT;
    tier_id UUID;
BEGIN
    FOR i IN 1..array_length(tiers, 1) LOOP
        SELECT id INTO tier_id FROM plans WHERE tier = tiers[i];
        IF tier_id IS NULL THEN
            RAISE EXCEPTION 'plan tier % not found', tiers[i];
        END IF;
        INSERT INTO plan_entitlements (id, plan_id, key, limit_value, version)
        VALUES (
            gen_random_uuid(),
            tier_id,
            'max_history_retention_days',
            days[i]::bigint,
            1
        )
        ON CONFLICT (plan_id, key, version) DO NOTHING;
    END LOOP;
END $$;

-- Sanity: verify every tier now has the new key at version 1.
DO $$
DECLARE
    cnt INT;
BEGIN
    SELECT COUNT(*) INTO cnt
    FROM plan_entitlements
    WHERE key = 'max_history_retention_days' AND version = 1;
    IF cnt <> 5 THEN
        RAISE EXCEPTION
            'expected 5 rows for max_history_retention_days, found %', cnt;
    END IF;
END $$;
