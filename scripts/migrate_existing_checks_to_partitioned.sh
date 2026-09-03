#!/usr/bin/env bash
# ============================================================
# scripts/migrate_existing_checks_to_partitioned.sh
#
# AUDIT ISSUE #3 (real fix, not documentation-only): a safe upgrade path
# for converting an EXISTING, POPULATED `checks` table into the
# partitioned structure that migration 0061 creates. 0061 itself refuses
# to run against non-empty `checks`/`diagnostic_reports` tables (by
# design — see its own header comment) because PostgreSQL cannot convert
# a plain table into a partitioned one in place; the data must be
# physically copied into a new, already-partitioned table.
#
# THIS IS DELIBERATELY NOT WIRED INTO THE NORMAL `sqlx migrate` CHAIN.
# It must be run manually, by an operator, BEFORE the application's
# normal migration chain is allowed to reach migration 0061 on a
# database that already has real `checks`/`diagnostic_reports` rows.
# Running data-moving DDL unattended as part of a routine deploy is
# exactly the kind of risk this script exists to avoid.
#
# WHAT THIS SCRIPT DOES, IN ORDER:
#   1. Preflight checks (checks isn't already partitioned; refuses to
#      run twice).
#   2. Creates a new, empty, partitioned "shadow" table
#      (checks_shadow_<batch>) with EXACTLY the structure migration
#      0061 creates (same columns, same PRIMARY KEY (id, checked_at),
#      same monitor_id FK, same three indexes) plus a monthly partition
#      for every month actually present in your existing data (computed
#      from MIN/MAX(checked_at)), padded --future-months ahead, plus a
#      DEFAULT partition as a catch-all safety net.
#   3. Copies existing rows into the shadow table in small, separately
#      committed batches (default 20,000 rows/batch) ordered by
#      (checked_at, id), tracking progress in a tiny bookkeeping table
#      so the script is safely RESUMABLE if interrupted (Ctrl-C,
#      connection drop, etc. — just re-run it).
#   4. Verifies row counts match between `checks` and the shadow table.
#   5. Only with --yes: performs the final swap in ONE short
#      ACCESS EXCLUSIVE-locked transaction — catches up any very last
#      rows written since the last batch, renames the old `checks` table
#      aside (never dropped), renames the shadow table (and its default
#      partition/indexes/constraints) into place with the exact names
#      migration 0061 would have used, and re-points diagnostic_reports'
#      FK at the new table (required: a table rename does NOT
#      automatically re-target existing foreign keys bound to the old
#      table's OID).
#
# WHAT THIS SCRIPT DELIBERATELY DOES NOT DO:
#   - It does not touch `_sqlx_migrations`. After this script succeeds,
#     `checks` is already in the exact end-state migration 0061 would
#     have produced, but sqlx does not know that — it will still try to
#     run 0061's SQL body on your next deploy, see `checks` has rows
#     (correctly — the data is real and preserved), and refuse, exactly
#     as designed. See "AFTER THIS SCRIPT SUCCEEDS" below for the one
#     safe way to reconcile that.
#   - It does not delete the old table. It is renamed to
#     `checks_pre_partition_backup_<batch>` and left in place for you to
#     verify and drop manually once you're confident, on your own
#     timeline.
#   - It does not guess at your data volume or acceptable downtime. Batch
#     size and future-month padding are flags with conservative
#     defaults; read them before running against production.
#
# USAGE:
#   scripts/migrate_existing_checks_to_partitioned.sh                # dry-run / plan only, no writes
#   scripts/migrate_existing_checks_to_partitioned.sh --copy         # runs the batched copy, does NOT swap
#   scripts/migrate_existing_checks_to_partitioned.sh --copy --yes   # copies (if needed) AND performs the final swap
#
# Re-running with --copy is always safe (resumes from the tracked
# watermark; no-ops once caught up). Re-running with --yes after a
# successful swap is also safe (step 1's preflight check no-ops).
#
# AFTER THIS SCRIPT SUCCEEDS — reconciling `_sqlx_migrations`:
#   sqlx has no built-in "mark this migration applied without running
#   it" command. The standard, safe way to tell it migration 0061 is
#   already satisfied is to let it actually "run" a no-op version of
#   0061 once, then restore the real file:
#     1. Temporarily replace the CONTENTS of
#        migrations/0061_partition_checks_by_checked_at.sql with a
#        single comment line, e.g.:
#          -- no-op: checks already converted by
#          -- scripts/migrate_existing_checks_to_partitioned.sh on <date>
#        (keep the exact same FILENAME — sqlx's version number comes
#        from the filename, not the contents).
#     2. Run your normal migration command once (`sqlx migrate run`, or
#        just start the app if it runs migrations on boot). sqlx will
#        apply the now-empty 0061 successfully and record it applied,
#        then continue on to 0062+ normally.
#     3. Restore migrations/0061_partition_checks_by_checked_at.sql to
#        its real content immediately afterward (git checkout it back).
#        Do NOT leave the neutered version committed — it must stay
#        intact for every OTHER database (fresh installs, other
#        environments) that still needs to run the real thing.
#   This is a manual, careful, one-time production step — deliberately
#   not automated here.
# ============================================================
set -euo pipefail

BATCH_SIZE=20000
FUTURE_MONTHS=3
DO_COPY=false
DO_SWAP=false

for arg in "$@"; do
  case "$arg" in
    --copy) DO_COPY=true ;;
    --yes) DO_SWAP=true ;;
    --batch-size=*) BATCH_SIZE="${arg#*=}" ;;
    --future-months=*) FUTURE_MONTHS="${arg#*=}" ;;
    *)
      echo "unknown argument: $arg" >&2
      echo "usage: $0 [--copy] [--yes] [--batch-size=N] [--future-months=N]" >&2
      exit 2
      ;;
  esac
done

POSTGRES_PASSWORD="$(grep -E '^POSTGRES_PASSWORD=' .env | cut -d '=' -f2-)"
if [ -z "${POSTGRES_PASSWORD}" ]; then
  echo "POSTGRES_PASSWORD not found in .env — aborting." >&2
  exit 1
fi

psql_exec() {
  PGPASSWORD="${POSTGRES_PASSWORD}" psql \
    --host=127.0.0.1 \
    --port=5433 \
    --username=postgres \
    --dbname=uptime_monitor \
    --set=ON_ERROR_STOP=1 \
    "$@"
}

psql_scalar() {
  psql_exec -t -A -c "$1"
}

echo "=== scripts/migrate_existing_checks_to_partitioned.sh ==="
echo "batch size: ${BATCH_SIZE}   future months padding: ${FUTURE_MONTHS}"
echo

# ---------------------------------------------------------------------
# 1) Preflight
# ---------------------------------------------------------------------
already_partitioned="$(psql_scalar "
    SELECT EXISTS (
        SELECT 1 FROM pg_partitioned_table pt
        JOIN pg_class c ON c.oid = pt.partrelid
        WHERE c.relname = 'checks'
    );
")"

if [ "${already_partitioned}" = "t" ]; then
  echo "checks is already partitioned — nothing to do. (If this is"
  echo "unexpected, migration 0061 may have already been applied normally,"
  echo "e.g. because checks/diagnostic_reports were actually empty.)"
  exit 0
fi

total_rows="$(psql_scalar "SELECT count(*) FROM checks;")"
echo "checks currently has ${total_rows} row(s) and is NOT partitioned."

if [ "${total_rows}" = "0" ]; then
  echo "checks is empty — you do not need this script. Just let the"
  echo "normal migration chain run; migration 0061 will succeed on its own."
  exit 0
fi

date_bounds="$(psql_scalar "SELECT min(checked_at)::date || ',' || max(checked_at)::date FROM checks;")"
min_date="${date_bounds%%,*}"
max_date="${date_bounds##*,}"
echo "existing checked_at range: ${min_date} .. ${max_date}"
echo

if [ "${DO_COPY}" != "true" ]; then
  echo "Dry run only (no --copy passed). Re-run with --copy to create the"
  echo "shadow table and start the batched copy, or --copy --yes to also"
  echo "perform the final swap once copying is verified complete."
  exit 0
fi

# ---------------------------------------------------------------------
# 2) Create shadow table + partitions + indexes + FK (idempotent: uses
#    IF NOT EXISTS throughout so re-running --copy is always safe).
# ---------------------------------------------------------------------
echo "=== creating shadow table (if not already created by a previous run) ==="
psql_exec -v ON_ERROR_STOP=1 << 'SQL'
CREATE TABLE IF NOT EXISTS checks_shadow (
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

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'checks_shadow_monitor_id_fkey'
    ) THEN
        ALTER TABLE checks_shadow
            ADD CONSTRAINT checks_shadow_monitor_id_fkey
            FOREIGN KEY (monitor_id) REFERENCES monitors (id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_checks_shadow_monitor_id
    ON checks_shadow (monitor_id);

CREATE INDEX IF NOT EXISTS idx_checks_shadow_checked_at
    ON checks_shadow (checked_at DESC);

CREATE INDEX IF NOT EXISTS idx_checks_shadow_monitor_checked_at
    ON checks_shadow (monitor_id, checked_at DESC);

CREATE TABLE IF NOT EXISTS checks_shadow_default PARTITION OF checks_shadow DEFAULT;

-- One-row bookkeeping table tracking copy progress, so this script is
-- resumable across separate invocations.
CREATE TABLE IF NOT EXISTS checks_shadow_migration_progress (
    id BOOLEAN PRIMARY KEY DEFAULT true,
    last_checked_at TIMESTAMPTZ,
    last_id UUID,
    CHECK (id = true)
);
INSERT INTO checks_shadow_migration_progress (id)
    VALUES (true) ON CONFLICT (id) DO NOTHING;
SQL

echo "=== creating monthly partitions for ${min_date}..${max_date} (+${FUTURE_MONTHS} months) ==="
psql_exec -v min_date="'${min_date}'" -v max_date="'${max_date}'" -v future_months="${FUTURE_MONTHS}" << 'SQL'
DO $$
DECLARE
    cursor_month date := date_trunc('month', :min_date::date);
    end_month date := date_trunc('month', :max_date::date) + (interval '1 month' * :future_months);
    part_name text;
BEGIN
    WHILE cursor_month <= end_month LOOP
        part_name := 'checks_shadow_p' || to_char(cursor_month, 'YYYY_MM');
        IF to_regclass(quote_ident(current_schema()) || '.' || quote_ident(part_name)) IS NULL THEN
            EXECUTE format(
                'CREATE TABLE %I PARTITION OF checks_shadow FOR VALUES FROM (%L) TO (%L)',
                part_name, cursor_month, cursor_month + interval '1 month'
            );
        END IF;
        cursor_month := cursor_month + interval '1 month';
    END LOOP;
END $$;
SQL

# ---------------------------------------------------------------------
# 3) Batched, resumable copy. Each batch is its own committed
#    transaction (a single psql -c statement), so an interruption never
#    loses more than one batch's worth of progress, and the watermark
#    means re-running this script picks up exactly where it left off.
# ---------------------------------------------------------------------
echo "=== copying rows in batches of ${BATCH_SIZE} ==="
copied_total=0
while :; do
  copied_this_batch="$(psql_scalar "
      WITH watermark AS (
          SELECT last_checked_at, last_id FROM checks_shadow_migration_progress
      ),
      batch AS (
          SELECT c.*
          FROM checks c, watermark w
          WHERE (w.last_checked_at IS NULL)
             OR (c.checked_at, c.id) > (w.last_checked_at, w.last_id)
          ORDER BY c.checked_at, c.id
          LIMIT ${BATCH_SIZE}
      ),
      inserted AS (
          INSERT INTO checks_shadow (
              id, monitor_id, status, http_status, response_time_ms, error_type,
              checked_at, error_message, cert_expires_at, cert_days_remaining,
              region, country_code
          )
          SELECT
              id, monitor_id, status, http_status, response_time_ms, error_type,
              checked_at, error_message, cert_expires_at, cert_days_remaining,
              region, country_code
          FROM batch
          ON CONFLICT (id, checked_at) DO NOTHING
          RETURNING 1
      ),
      new_watermark AS (
          SELECT checked_at, id FROM batch ORDER BY checked_at DESC, id DESC LIMIT 1
      )
      UPDATE checks_shadow_migration_progress p
      SET last_checked_at = COALESCE((SELECT checked_at FROM new_watermark), p.last_checked_at),
          last_id = COALESCE((SELECT id FROM new_watermark), p.last_id)
      RETURNING (SELECT count(*) FROM batch);
  ")"

  if [ -z "${copied_this_batch}" ] || [ "${copied_this_batch}" = "0" ]; then
    break
  fi
  copied_total=$((copied_total + copied_this_batch))
  echo "  ... copied ${copied_total} / ${total_rows} row(s) so far"
done

echo "=== verifying counts ==="
shadow_count="$(psql_scalar "SELECT count(*) FROM checks_shadow;")"
echo "checks: ${total_rows}   checks_shadow: ${shadow_count}"

if [ "${shadow_count}" != "${total_rows}" ]; then
  echo "COUNT MISMATCH — do not proceed to --yes. This can legitimately"
  echo "happen if rows were written to checks WHILE this script ran (the"
  echo "batched copy is not a point-in-time snapshot). Simply re-run this"
  echo "script with --copy again to catch up; the final --yes swap also"
  echo "performs one last catch-up pass under an exclusive lock before"
  echo "swapping, so a small gap here is expected and fine — a LARGE or"
  echo "persistent gap across repeated re-runs is not and should be"
  echo "investigated before proceeding." >&2
  if [ "${DO_SWAP}" = "true" ]; then
    echo "Refusing to swap with a count mismatch present. Re-run without" >&2
    echo "--yes first, confirm counts converge, then add --yes." >&2
    exit 1
  fi
fi

if [ "${DO_SWAP}" != "true" ]; then
  echo
  echo "Copy complete (or caught up). Re-run with --copy --yes to perform"
  echo "the final swap once you're satisfied the counts above match."
  exit 0
fi

# ---------------------------------------------------------------------
# 4) Final swap: short ACCESS EXCLUSIVE-locked transaction. Catches up
#    any last rows, renames tables/indexes/constraints into place with
#    the exact names migration 0061 would have used, re-points
#    diagnostic_reports' FK (required after a table rename — a rename
#    does not retarget existing FKs bound to the old table's OID).
# ---------------------------------------------------------------------
backup_suffix="$(date -u +%Y%m%d_%H%M%S)"
echo "=== performing final swap (backup table: checks_pre_partition_backup_${backup_suffix}) ==="

psql_exec -v backup_table="checks_pre_partition_backup_${backup_suffix}" -v ON_ERROR_STOP=1 << 'SQL'
BEGIN;

LOCK TABLE checks IN ACCESS EXCLUSIVE MODE;

-- Final catch-up: anything written to checks since the last batch above.
INSERT INTO checks_shadow (
    id, monitor_id, status, http_status, response_time_ms, error_type,
    checked_at, error_message, cert_expires_at, cert_days_remaining,
    region, country_code
)
SELECT
    id, monitor_id, status, http_status, response_time_ms, error_type,
    checked_at, error_message, cert_expires_at, cert_days_remaining,
    region, country_code
FROM checks c
ON CONFLICT (id, checked_at) DO NOTHING;

-- Hard stop if counts still don't match even after the final catch-up —
-- something other than ordinary concurrent writes is going on.
DO $$
DECLARE
    old_count bigint;
    new_count bigint;
BEGIN
    SELECT count(*) INTO old_count FROM checks;
    SELECT count(*) INTO new_count FROM checks_shadow;
    IF old_count <> new_count THEN
        RAISE EXCEPTION
            'Refusing to swap: checks has % row(s) but checks_shadow has % '
            'row(s) even after the final catch-up copy. Investigate before '
            're-running.', old_count, new_count;
    END IF;
END $$;

ALTER TABLE diagnostic_reports
    DROP CONSTRAINT diagnostic_reports_check_id_checked_at_fkey;

ALTER TABLE checks RENAME TO :"backup_table";
ALTER TABLE checks_shadow RENAME TO checks;
ALTER TABLE checks_shadow_default RENAME TO checks_default;
ALTER TABLE checks_shadow_migration_progress RENAME TO checks_partition_migration_progress_done;

ALTER TABLE checks RENAME CONSTRAINT checks_shadow_pkey TO checks_pkey;
ALTER TABLE checks RENAME CONSTRAINT checks_shadow_monitor_id_fkey TO checks_monitor_id_fkey;
ALTER INDEX idx_checks_shadow_monitor_id RENAME TO idx_checks_monitor_id;
ALTER INDEX idx_checks_shadow_checked_at RENAME TO idx_checks_checked_at;
ALTER INDEX idx_checks_shadow_monitor_checked_at RENAME TO idx_checks_monitor_checked_at;

DO $$
DECLARE
    part record;
BEGIN
    FOR part IN
        SELECT c.relname AS old_name
        FROM pg_inherits i
        JOIN pg_class c ON c.oid = i.inhrelid
        JOIN pg_class p ON p.oid = i.inhparent
        WHERE p.relname = 'checks' AND c.relname LIKE 'checks_shadow_p%'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I RENAME TO %I',
            part.old_name,
            replace(part.old_name, 'checks_shadow_p', 'checks_p')
        );
    END LOOP;
END $$;

ALTER TABLE diagnostic_reports
    ADD CONSTRAINT diagnostic_reports_check_id_checked_at_fkey
    FOREIGN KEY (check_id, check_checked_at)
    REFERENCES checks (id, checked_at)
    ON DELETE CASCADE;

COMMIT;
SQL

echo
echo "=== done ==="
echo "Old table preserved as: checks_pre_partition_backup_${backup_suffix}"
echo "checks is now partitioned, with the same structure migration 0061 creates."
echo
echo "NEXT STEP (required): reconcile _sqlx_migrations for migration 0061"
echo "before your normal migration chain will proceed past it. See the"
echo "\"AFTER THIS SCRIPT SUCCEEDS\" section in this script's header comment"
echo "for the exact, safe procedure."
echo
echo "Once you've verified everything (application running normally,"
echo "checks_partition_health() looks correct, etc.), you may drop"
echo "checks_pre_partition_backup_${backup_suffix} on your own timeline —"
echo "this script does not do that for you."
