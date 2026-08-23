-- B23: Incident State Machine — consecutive failure/success tracking.
--
-- These counters live on `monitors` (not `incidents`) because they must
-- persist across the open/resolved boundary: e.g. a monitor recovers
-- (incident resolves, consecutive_successes resets) then starts failing
-- again — the new failure streak has nothing to do with the old,
-- already-closed incident row.
--
-- `record_check_outcome` (see PostgresMonitorRepository) updates these
-- atomically in a single UPDATE ... RETURNING, so concurrent check
-- results for the same monitor never read a stale count.
ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS consecutive_failures INTEGER NOT NULL DEFAULT 0;

ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS consecutive_successes INTEGER NOT NULL DEFAULT 0;
