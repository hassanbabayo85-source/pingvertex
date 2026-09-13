-- GAP 2 — Restore proxy_check.monitoring as an active recurring operation.
--
-- Migration 0110 retired recurring proxy monitoring in favor of the
-- one-shot `proxy_check.one_time`. The GAP 2 blueprint supersedes that
-- decision: recurring proxy monitoring is a billable product surface
-- again, and the recurring proxy worker charges per proxy entry under
-- the canonical operation name `proxy_check.monitoring`.
--
-- The historical pricing rows inserted by migrations 0091/0093 for
-- `proxy_check.monitoring` remain queryable (immutable price book) and
-- are NOT deleted here. Only the retirement filter row is removed, so
-- `PostgresPAYGPricingRepository::list_active()` exposes the operation
-- again.
BEGIN;

DELETE FROM payg_retired_operations
WHERE operation = 'proxy_check.monitoring';

COMMIT;
