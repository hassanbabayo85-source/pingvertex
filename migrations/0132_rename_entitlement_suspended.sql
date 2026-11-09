-- 0132_rename_entitlement_suspended.sql
--
-- Rename the `entitlement_suspended` boolean column on both
-- `monitors` and `recurring_proxy_monitors` to `resource_limit_suspended`.
--
-- Context: the codebase completed a terminology migration in Phase 2
-- (Entitlements -> ResourceLimits, EntitlementResolver ->
-- ResourceLimitResolver, PlanLimitError -> ResourceLimitError, etc.).
-- This migration brings the database columns in line.
--
-- This is a PURE COLUMN RENAME. PostgreSQL preserves, for each renamed
-- column:
--   * DEFAULT clause
--   * NOT NULL constraint
--   * type (BOOLEAN)
--   * COMMENT (now stale prose -- see note below)
--   * every partial index whose condition references the column, with
--     the index condition automatically rewritten to use the new name
--   * every CHECK constraint that references the column
--
-- No view, trigger, or function in this schema references either
-- column, so nothing else needs to be recreated.
--
-- Historical migrations (0125, 0126, 0128) are NOT modified. SQLx
-- verifies migration checksums against `_sqlx_migrations`; editing a
-- historical migration would fail on every existing installation.
-- Forward-only is the correct pattern.
--
-- Stale prose note: the COMMENT on the old column (set in 0125) says
-- "Set by backend when the current subscription no longer permits this
-- monitor." That comment lives on the renamed column and mentions
-- "subscription" instead of "resource limit". Functionally correct
-- (a subscription's plan determines its resource limits) but
-- cosmetically stale. A future prose-cleanup migration can update the
-- COMMENT; not part of this rename-only migration.

BEGIN;

ALTER TABLE monitors
    RENAME COLUMN entitlement_suspended TO resource_limit_suspended;

ALTER TABLE recurring_proxy_monitors
    RENAME COLUMN entitlement_suspended TO resource_limit_suspended;

COMMIT;
