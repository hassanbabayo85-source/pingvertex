-- H15 Task B: partition `checks` by month on `checked_at`.
--
-- Preconditions verified before writing this migration (dev-DB check on
-- 2026-08-29): both `checks` and `diagnostic_reports` were confirmed
-- EMPTY (0 rows) in this environment, and there is no production
-- deployment yet (confirmed with the project owner) — so this migration
-- performs a direct drop-and-recreate of `checks` rather than a
-- dual-write/backfill migration. The safety check below makes that
-- assumption explicit and fails loudly instead of silently discarding
-- data if this is ever run against a database that is no longer empty.
--
-- Composite primary key: PostgreSQL requires a partitioned table's
-- primary key (and anything referencing it) to include the partition
-- key column, hence PRIMARY KEY (id, checked_at) instead of just (id).
-- Migration 0060 already prepared `diagnostic_reports` for this by
-- adding `check_checked_at` and pointing its FK at the composite key.
--
-- Partitioning scheme: RANGE partitioning by month on `checked_at`,
-- with a DEFAULT partition to catch any row outside the explicitly
-- created monthly ranges (so an insert can never fail for lack of a
-- partition). Monthly partitions are created here for a rolling window
-- (2 months back through 3 months ahead of the date this migration was
-- written). Creating future months' partitions ahead of time on an
-- ongoing basis is an OPERATIONAL TASK NOT SOLVED BY THIS MIGRATION —
-- flagged in HANDOVER.md for the next worker (e.g. a scheduled job that
-- creates next month's partition a few weeks in advance).

DO $$
DECLARE
    checks_rows bigint;
    reports_rows bigint;
BEGIN
    SELECT count(*) INTO checks_rows FROM checks;
    SELECT count(*) INTO reports_rows FROM diagnostic_reports;

    IF checks_rows <> 0 OR reports_rows <> 0 THEN
        RAISE EXCEPTION
            'Refusing to run: checks has % row(s) and diagnostic_reports has % row(s). '
            'This migration drops and recreates checks assuming both tables are empty '
            '(verified true when this migration was written). If real data now exists, '
            'this migration must be replaced with a dual-write + backfill migration '
            'instead of being run as-is.',
            checks_rows, reports_rows;
    END IF;
END $$;

-- Detach the FK from diagnostic_reports so `checks` can be freely
-- restructured underneath it.
ALTER TABLE diagnostic_reports
    DROP CONSTRAINT diagnostic_reports_check_id_checked_at_fkey;

-- Drop the old, unpartitioned checks table. Safe: verified empty above,
-- and the FK that referenced it was just dropped. No views depend on
-- checks (verified via pg_views before writing this migration).
DROP TABLE checks;

-- Recreate `checks` as a table partitioned by RANGE on `checked_at`.
-- Column list, types, nullability, and defaults match the table this
-- replaces exactly (verified via \d checks before writing this).
CREATE TABLE checks (
    id UUID NOT NULL,
    monitor_id UUID NOT NULL,
    status SMALLINT NOT NULL,
    http_status INTEGER,
    response_time_ms BIGINT,
    error_type TEXT,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    error_message TEXT,
    cert_expires_at TIMESTAMPTZ,
    cert_days_remaining BIGINT,
    region TEXT NOT NULL DEFAULT 'us-east',
    country_code VARCHAR(2),
    PRIMARY KEY (id, checked_at)
) PARTITION BY RANGE (checked_at);

ALTER TABLE checks
    ADD CONSTRAINT checks_monitor_id_fkey
    FOREIGN KEY (monitor_id) REFERENCES monitors (id) ON DELETE CASCADE;

-- Same index names and shapes as before partitioning. On a partitioned
-- table these are created automatically on every partition ("global
-- looking", per-partition in reality) — no per-partition index
-- maintenance is needed here.
CREATE INDEX idx_checks_monitor_id
    ON checks (monitor_id);

CREATE INDEX idx_checks_checked_at
    ON checks (checked_at DESC);

CREATE INDEX idx_checks_monitor_checked_at
    ON checks (monitor_id, checked_at DESC);

-- Monthly partitions: 2 months back through 3 months ahead of the date
-- this migration was written (2026-08-29), plus a DEFAULT partition for
-- anything outside that window so writes never fail for lack of a
-- partition.
CREATE TABLE checks_p2026_06 PARTITION OF checks
    FOR VALUES FROM ('2026-06-01T00:00:00+00') TO ('2026-07-01T00:00:00+00');

CREATE TABLE checks_p2026_07 PARTITION OF checks
    FOR VALUES FROM ('2026-07-01T00:00:00+00') TO ('2026-08-01T00:00:00+00');

CREATE TABLE checks_p2026_08 PARTITION OF checks
    FOR VALUES FROM ('2026-08-01T00:00:00+00') TO ('2026-09-01T00:00:00+00');

CREATE TABLE checks_p2026_09 PARTITION OF checks
    FOR VALUES FROM ('2026-09-01T00:00:00+00') TO ('2026-10-01T00:00:00+00');

CREATE TABLE checks_p2026_10 PARTITION OF checks
    FOR VALUES FROM ('2026-10-01T00:00:00+00') TO ('2026-11-01T00:00:00+00');

CREATE TABLE checks_p2026_11 PARTITION OF checks
    FOR VALUES FROM ('2026-11-01T00:00:00+00') TO ('2026-12-01T00:00:00+00');

CREATE TABLE checks_default PARTITION OF checks DEFAULT;

-- Re-point diagnostic_reports' FK at the new, partitioned checks table.
-- checks' new PRIMARY KEY (id, checked_at) already provides the unique
-- index this composite FK needs as a target.
ALTER TABLE diagnostic_reports
    ADD CONSTRAINT diagnostic_reports_check_id_checked_at_fkey
    FOREIGN KEY (check_id, check_checked_at)
    REFERENCES checks (id, checked_at)
    ON DELETE CASCADE;
