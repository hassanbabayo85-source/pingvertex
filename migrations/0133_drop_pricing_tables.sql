-- 0133_drop_pricing_tables.sql
--
-- Drops the two tables that backed the legacy PricingConfig /
-- PricingAuditLogEntry domain types, both of which were removed from
-- the codebase in phase-3-2. After that commit no Rust code reads or
-- writes either table, so dropping them is safe.
--
-- Tables dropped:
--   * pricing_audit_logs  (created in 0051)
--   * pricing_configs     (created in 0050, extended in 0054, seeded
--                          in 0055 and 0089, touched by the overlap
--                          trigger in 0104)
--
-- FK note: neither table is referenced by any other table in this
-- schema, so no ON DELETE CASCADE is required at the FK level. The
-- CASCADE on DROP TABLE handles any lingering trigger/function
-- dependencies created by migration 0104.
--
-- Tables deliberately NOT dropped in this migration:
--   * plans             -- referenced by plan_entitlements and
--                          plan_prices; dropped in phase-3-3
--   * plan_prices       -- billing surface; dropped in phase-3-3
--   * plan_entitlements -- billing surface; dropped in phase-3-3
--   * subscriptions     -- billing surface; dropped in phase-3-3
--   * all wallet_* tables -- billing surface; dropped in phase-3-3
--
-- Historical migrations (0050, 0051, 0054, 0055, 0089, 0104, etc.) are
-- NOT modified. SQLx verifies migration checksums; editing a historical
-- migration would fail on every existing installation. Forward-only.

BEGIN;

DROP TABLE IF EXISTS pricing_audit_logs CASCADE;
DROP TABLE IF EXISTS pricing_configs CASCADE;

COMMIT;
