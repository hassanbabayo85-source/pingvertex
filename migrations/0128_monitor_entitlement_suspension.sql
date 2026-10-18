-- TASK 5 — Per-monitor entitlement suspension flag.
--
-- Distinguishes the customer's own enable/disable choice (`enabled`,
-- which this migration does NOT change) from a backend decision that
-- the current subscription does not permit this monitor
-- (`entitlement_suspended`).
--
-- A downgrade that puts the tenant over its plan limit marks the newest
-- monitors as suspended (deterministic policy: keep the oldest, suspend
-- the newest). An upgrade clears the flag on ALL monitors automatically
-- — no user action is required, and the flag is never persisted as a
-- permanent "disabled" state. If the customer had also disabled a
-- monitor via `enabled = FALSE`, that choice survives untouched.
--
-- Legacy rows: NULL is impossible; DEFAULT FALSE makes every existing
-- monitor eligible immediately, which is the correct behaviour because
-- no tenant is currently over any limit (all limits are new).

BEGIN;

ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS entitlement_suspended BOOLEAN NOT NULL DEFAULT FALSE;

-- Scheduler claim index: only enabled, non-suspended, due monitors.
-- Matches the shape of the existing idx_recurring_proxy_monitors_due
-- (TASK 3) so the two claim paths look the same on EXPLAIN.
CREATE INDEX IF NOT EXISTS idx_monitors_claimable
    ON monitors (next_due_at)
    WHERE enabled = TRUE
      AND entitlement_suspended = FALSE;

-- Per-tenant lookup by suspension state, used by the reconciliation
-- sweep to compute "how many are currently suspended".
CREATE INDEX IF NOT EXISTS idx_monitors_tenant_suspended
    ON monitors (tenant_id, created_at)
    WHERE entitlement_suspended = TRUE;

COMMIT;
