-- Follow-up to 0061 (checks partitioning). 0061's own comment admits:
-- "Creating future months' partitions ahead of time on an ongoing basis
-- is an OPERATIONAL TASK NOT SOLVED BY THIS MIGRATION". Verified: no
-- scheduler/cron/systemd job existed anywhere in the repo to do this,
-- so once the 3-months-ahead window created by 0061 runs out, new rows
-- would silently fall into checks_default (unpartitioned catch-all)
-- instead of a proper monthly partition.
--
-- This migration adds an idempotent helper function; it does not itself
-- schedule anything. Pair it with scripts/ensure_checks_partitions.sh
-- and deploy/systemd/pingvertex-checks-partition.{service,timer} (added
-- alongside this migration) to run it monthly.
CREATE OR REPLACE FUNCTION ensure_checks_partition(p_month date)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    part_start date := date_trunc('month', p_month);
    part_end   date := part_start + interval '1 month';
    part_name  text := 'checks_p' || to_char(part_start, 'YYYY_MM');
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = part_name
    ) THEN
        EXECUTE format(
            'CREATE TABLE %I PARTITION OF checks FOR VALUES FROM (%L) TO (%L)',
            part_name, part_start, part_end
        );
        RAISE NOTICE 'Created partition %', part_name;
    END IF;
END;
$$;
