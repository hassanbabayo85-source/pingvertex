-- AUDIT ISSUES #4 / #10 / #12 (2026-09 remediation):
--   #4  "checks_default partition recovery gap" — no way to move rows
--       that landed in the DEFAULT partition back into their proper
--       monthly partition once it's created.
--   #10 "partition existence check is not schema-aware" —
--       `ensure_checks_partition` (migration 0068) checks
--       `pg_class.relname = part_name` without checking the owning
--       schema, so an identically-named relation in another schema
--       could make it believe the partition already exists.
--   #12 "no explicit partition health / future-partition alert" — no
--       single query ops can point a monitor/alert at to know whether
--       partition maintenance is keeping up.
--
-- Migrations here are treated as append-only (see 0063's own comment),
-- so this does not edit 0068's function body directly; it instead
-- CREATE OR REPLACEs the same function (same name/signature, so every
-- existing caller — scripts/ensure_checks_partitions.sh — keeps working
-- unchanged) and adds new functions alongside it.

-- ---------------------------------------------------------------------
-- ISSUE #10 fix: redefine ensure_checks_partition to use a
-- schema-qualified `to_regclass` lookup instead of an unqualified
-- `pg_class.relname` scan, so a same-named relation in a different
-- schema can never cause this to skip creating a partition that
-- doesn't actually exist in this schema.
-- ---------------------------------------------------------------------
CREATE OR REPLACE FUNCTION ensure_checks_partition(p_month date)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    part_start date := date_trunc('month', p_month);
    part_end   date := part_start + interval '1 month';
    part_name  text := 'checks_p' || to_char(part_start, 'YYYY_MM');
BEGIN
    IF to_regclass(quote_ident(current_schema()) || '.' || quote_ident(part_name)) IS NULL THEN
        EXECUTE format(
            'CREATE TABLE %I PARTITION OF checks FOR VALUES FROM (%L) TO (%L)',
            part_name, part_start, part_end
        );
        RAISE NOTICE 'Created partition %', part_name;
    END IF;
END;
$$;

-- ---------------------------------------------------------------------
-- ISSUE #4 fix: reconciles rows that landed in checks_default (because
-- their monthly partition didn't exist yet at insert time) back into
-- their proper monthly partition, once that partition exists.
--
-- Always call ensure_checks_partition(p_month) before this (or call
-- checks_partition_maintenance(p_month), below, which does both in the
-- right order) — this function is a no-op if the target partition does
-- not exist yet, since there is nowhere safe to move the rows to.
--
-- Mechanism: checks_default is itself a partition of `checks`, so a
-- plain DELETE ... RETURNING * paired with an INSERT back into the
-- parent `checks` table lets PostgreSQL's partition routing put each
-- row into whichever partition now actually matches its checked_at —
-- no manual per-partition INSERT target needed, and this stays correct
-- even if partition boundaries change in the future. Both statements
-- run inside the single implicit transaction of this function, so a
-- failure partway through does not leave rows duplicated or dropped.
-- ---------------------------------------------------------------------
CREATE OR REPLACE FUNCTION reconcile_checks_default_partition(p_month date)
RETURNS bigint
LANGUAGE plpgsql
AS $$
DECLARE
    part_start date := date_trunc('month', p_month);
    part_end   date := part_start + interval '1 month';
    part_name  text := 'checks_p' || to_char(part_start, 'YYYY_MM');
    moved_count bigint := 0;
BEGIN
    IF to_regclass(quote_ident(current_schema()) || '.' || quote_ident(part_name)) IS NULL THEN
        RAISE NOTICE
            'Partition % does not exist yet; skipping reconciliation for %. '
            'Call ensure_checks_partition(%) first.',
            part_name, p_month, p_month;
        RETURN 0;
    END IF;

    WITH moved AS (
        DELETE FROM checks_default
        WHERE checked_at >= part_start::timestamptz
          AND checked_at < part_end::timestamptz
        RETURNING *
    )
    INSERT INTO checks (
        id, monitor_id, status, http_status, response_time_ms, error_type,
        checked_at, error_message, cert_expires_at, cert_days_remaining,
        region, country_code
    )
    SELECT
        id, monitor_id, status, http_status, response_time_ms, error_type,
        checked_at, error_message, cert_expires_at, cert_days_remaining,
        region, country_code
    FROM moved;

    GET DIAGNOSTICS moved_count = ROW_COUNT;

    IF moved_count > 0 THEN
        RAISE NOTICE 'Reconciled % row(s) from checks_default into %', moved_count, part_name;
    END IF;

    RETURN moved_count;
END;
$$;

-- ---------------------------------------------------------------------
-- Convenience wrapper pairing ensure + reconcile in the correct order.
-- scripts/ensure_checks_partitions.sh is updated (see that file) to
-- call this instead of ensure_checks_partition alone, so routine
-- partition maintenance also sweeps checks_default on every run.
-- ---------------------------------------------------------------------
CREATE OR REPLACE FUNCTION checks_partition_maintenance(p_month date)
RETURNS bigint
LANGUAGE plpgsql
AS $$
DECLARE
    reconciled bigint;
BEGIN
    PERFORM ensure_checks_partition(p_month);
    SELECT reconcile_checks_default_partition(p_month) INTO reconciled;
    RETURN reconciled;
END;
$$;

-- ---------------------------------------------------------------------
-- ISSUE #12 fix: single-query partition health check, meant to back an
-- operational health check / metrics export / alert (see also the
-- deployment-automation fix to scripts/ensure_checks_partitions.sh and
-- the new scripts/install_checks_partition_timer.sh for issue #11).
--
-- Reports, for the current month and each of the next `months_ahead`
-- months:
--   - whether that month's partition exists
-- plus, once for the whole call:
--   - how many rows are currently sitting in checks_default (any value
--     > 0 means partition maintenance has fallen behind at some point
--     and reconcile_checks_default_partition should be run).
-- ---------------------------------------------------------------------
CREATE OR REPLACE FUNCTION checks_partition_health(months_ahead integer DEFAULT 3)
RETURNS TABLE (
    partition_month date,
    partition_name text,
    partition_exists boolean,
    checks_default_row_count bigint
)
LANGUAGE plpgsql
AS $$
DECLARE
    default_rows bigint;
    i integer;
    target_month date;
    target_name text;
BEGIN
    SELECT count(*) INTO default_rows FROM checks_default;

    FOR i IN 0..months_ahead LOOP
        target_month := date_trunc('month', CURRENT_DATE + (interval '1 month' * i));
        target_name := 'checks_p' || to_char(target_month, 'YYYY_MM');

        partition_month := target_month;
        partition_name := target_name;
        partition_exists :=
            to_regclass(quote_ident(current_schema()) || '.' || quote_ident(target_name)) IS NOT NULL;
        checks_default_row_count := default_rows;

        RETURN NEXT;
    END LOOP;
END;
$$;
