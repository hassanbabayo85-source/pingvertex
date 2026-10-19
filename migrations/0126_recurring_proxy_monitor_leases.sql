-- TASK 3 — Worker-claim fields for recurring proxy monitors.
--
-- Migration 0125 created `recurring_proxy_monitors` with only the
-- configuration and scheduling columns. The worker needs a lease so
-- that exactly one process runs a due monitor at a time, and can
-- recover if that process dies mid-run. Two columns are added:
--
--   worker_id         — opaque worker identity that currently holds
--                       the claim, or NULL when nobody holds it.
--   lease_expires_at  — the instant after which the claim is void
--                       and another worker may re-claim the monitor.
--
-- Both are NULL when the monitor is idle. The pair is updated
-- atomically by the repository's `claim_due_monitors` and
-- `reschedule` implementations. Nothing here touches existing rows:
-- a monitor that was already present (there should be none in
-- production yet, since 0125 predates Task 3) simply has NULL leases
-- and is immediately claimable.

BEGIN;

ALTER TABLE recurring_proxy_monitors
    ADD COLUMN IF NOT EXISTS worker_id TEXT,
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ;

-- Worker claim index: only idle or lease-expired monitors whose next
-- run is due. Matches the same shape as 0125's `idx_recurring_proxy_monitors_due`
-- but also makes the lease columns queryable without a full table scan.
-- NOTE (Task 8D): the original predicate included
--   AND (lease_expires_at IS NULL OR lease_expires_at < now())
-- PostgreSQL rejects that with 42P17 (functions in index predicate must be
-- marked IMMUTABLE) on any fresh database. `claim_due_monitors` already
-- evaluates the lease-expiry comparison at runtime against $now, so the
-- index only needs to skip permanently unclaimable rows.
CREATE INDEX IF NOT EXISTS idx_recurring_proxy_monitors_claimable
    ON recurring_proxy_monitors (next_run_at)
    WHERE user_enabled = TRUE
      AND entitlement_suspended = FALSE;

-- Worker-side "who owns what" index, for operational queries and for
-- the recovery sweep that lists expired leases.
CREATE INDEX IF NOT EXISTS idx_recurring_proxy_monitors_lease_owner
    ON recurring_proxy_monitors (worker_id)
    WHERE worker_id IS NOT NULL;

COMMIT;
