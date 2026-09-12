-- PAYG blueprint: retire recurring proxy_check.monitoring.
--
-- The Rust-side canonical operation set (crates/domain/src/payg.rs)
-- already removed `proxy_check.monitoring` from `PaygOperation::ALL`
-- (14 canonical ops) and lists it in
-- `RETIRED_PAYG_OPERATIONS = &["proxy_check.monitoring"]`.
--
-- `PostgresPAYGPricingRepository::list_active()` filters active prices
-- through a `NOT EXISTS (SELECT 1 FROM payg_retired_operations ...)`
-- subquery, so the table must exist for that repository to work at all.
-- This migration creates the table and seeds the currently retired
-- operations from the Rust constant.
--
-- Forward-only: never delete the historical pricing rows for retired
-- operations — they remain queryable for audit/history. The retirement
-- table only *filters* what `list_active()` exposes to customers.
BEGIN;

CREATE TABLE IF NOT EXISTS payg_retired_operations (
    operation   TEXT PRIMARY KEY,
    retired_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    reason      TEXT,
    CONSTRAINT chk_payg_retired_operation_nonempty
        CHECK (length(btrim(operation)) > 0)
);

CREATE INDEX IF NOT EXISTS idx_payg_retired_operations_retired_at
    ON payg_retired_operations (retired_at DESC);

-- Seed the currently retired operation(s). `ON CONFLICT DO NOTHING`
-- keeps this idempotent across environments that may have been patched
-- manually before this migration was applied.
INSERT INTO payg_retired_operations (operation, reason)
VALUES (
    'proxy_check.monitoring',
    'Recurring proxy monitoring retired in favor of one-shot proxy_check.one_time'
)
ON CONFLICT (operation) DO NOTHING;

COMMIT;
