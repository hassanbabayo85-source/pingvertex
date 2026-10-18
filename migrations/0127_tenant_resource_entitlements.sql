-- TASK 4A — Tenant-scoped resource entitlements.
--
-- Adds three new subscription entitlements that were previously either
-- hardcoded or unbounded:
--
--   max_status_pages  — replaces the hardcoded
--                       MAX_STATUS_PAGES_PER_USER = 3 in
--                       create_status_page.rs. Now PER TENANT.
--   max_api_keys      — API key quota.
--   max_team_members  — team membership quota (owner counts as one).
--
-- All three keys are inserted at `version = 1`, matching every existing
-- row in `plan_entitlements` (migration 0125). This preserves the
-- invariant enforced by `domain::Entitlements::from_rows` that all rows
-- for a plan must share the same version. Bumping to version 2 would
-- make `MAX(version)` return only the new keys and break every existing
-- subscription's entitlement resolution.
--
-- HISTORICAL RECORDS: no existing row is modified or deleted. This
-- migration only INSERTs new rows. The `ON CONFLICT (plan_id, key,
-- version) DO NOTHING` clause makes the migration idempotent and safe
-- to re-run.
--
-- ENTERPRISE CUSTOM PLANS: the values below are defaults. There is no
-- per-tenant override table yet, so a contracted Enterprise customer
-- cannot receive a value other than the default until a follow-up
-- migration (planned for TASK 8, when PAYG is retired) adds
-- `tenant_entitlement_overrides`. Until then, the comment is
-- documentation only — do not assume an override path exists.

BEGIN;

INSERT INTO plan_entitlements (id, plan_id, key, limit_value, version)
SELECT gen_random_uuid(), p.id, s.key, s.limit_value, 1
FROM (
    VALUES
    -- max_status_pages — PER TENANT (was MAX_STATUS_PAGES_PER_USER = 3)
    ('free',       'max_status_pages',  1),
    ('starter',    'max_status_pages',  2),
    ('pro',        'max_status_pages',  5),
    ('business',   'max_status_pages', 15),
    ('enterprise', 'max_status_pages', 50),

    -- max_api_keys
    ('free',       'max_api_keys',  1),
    ('starter',    'max_api_keys',  3),
    ('pro',        'max_api_keys', 10),
    ('business',   'max_api_keys', 30),
    ('enterprise', 'max_api_keys', 100),

    -- max_team_members — includes the owner account
    ('free',       'max_team_members',  1),
    ('starter',    'max_team_members',  3),
    ('pro',        'max_team_members', 10),
    ('business',   'max_team_members', 25),
    ('enterprise', 'max_team_members', 100)
) AS s(plan_tier, key, limit_value)
JOIN plans p ON p.tier = s.plan_tier
ON CONFLICT (plan_id, key, version) DO NOTHING;

-- Sanity assertion: every self-service plan must have all three keys.
DO $$
DECLARE
    missing INT;
BEGIN
    SELECT COUNT(*) INTO missing
    FROM (VALUES ('free'), ('starter'), ('pro'), ('business'), ('enterprise')) AS t(tier)
    JOIN plans p ON p.tier = t.tier
    CROSS JOIN (VALUES
        ('max_status_pages'), ('max_api_keys'), ('max_team_members')
    ) AS k(key)
    WHERE NOT EXISTS (
        SELECT 1 FROM plan_entitlements pe
        WHERE pe.plan_id = p.id AND pe.key = k.key AND pe.version = 1
    );
    IF missing > 0 THEN
        RAISE EXCEPTION 'migration 0127: % entitlement row(s) missing after seed', missing;
    END IF;
END $$;

COMMIT;
