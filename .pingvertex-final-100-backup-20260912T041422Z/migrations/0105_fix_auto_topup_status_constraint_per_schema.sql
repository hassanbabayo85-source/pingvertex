-- Migration 0100's replacement logic checked `pg_class` without a schema
-- filter, so once one schema acquired the new constraint, every other
-- schema's 0100 pass skipped the replacement and kept the old
-- (incomplete) constraint from migration 0083. Under the schema-per-binary
-- test harness, this produced a mixed state: some binaries could insert
-- REQUIRES_RECONCILIATION, others could not.
--
-- This migration is deliberately schema-local: it only touches the
-- constraint on `current_schema()`. That makes it safe to run in every
-- test schema (each gets its own correct constraint) and in production
-- (single schema, no change in behaviour).
DO $do$
DECLARE
    r_con RECORD;
    current_schema_name TEXT := current_schema();
BEGIN
    -- Drop every CHECK constraint on wallet_auto_top_up_attempts that
    -- mentions `status`, in *this* schema only.
    FOR r_con IN
        SELECT c.conname
        FROM pg_constraint c
        JOIN pg_class cl ON cl.oid = c.conrelid
        JOIN pg_namespace n ON n.oid = cl.relnamespace
        WHERE cl.relname = 'wallet_auto_top_up_attempts'
          AND n.nspname = current_schema_name
          AND c.contype = 'c'
          AND pg_get_constraintdef(c.oid) ILIKE '%status%'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.wallet_auto_top_up_attempts DROP CONSTRAINT %I',
            current_schema_name, r_con.conname
        );
    END LOOP;

    -- Add the correct constraint with all five statuses.
    EXECUTE format(
        'ALTER TABLE %I.wallet_auto_top_up_attempts
         ADD CONSTRAINT wallet_auto_top_up_attempts_status_check
         CHECK (status IN (''PENDING'', ''PROCESSING'', ''SUCCEEDED'', ''FAILED'', ''REQUIRES_RECONCILIATION''))',
        current_schema_name
    );
END
$do$;
