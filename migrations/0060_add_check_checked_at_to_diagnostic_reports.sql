-- H15 follow-up (FK prep): denormalize checks.checked_at onto
-- diagnostic_reports so that (check_id, check_checked_at) can later serve
-- as a composite foreign key into checks(id, checked_at).
--
-- Why: if/when `checks` is partitioned by `checked_at` (see HANDOVER.md
-- H15 Task B assessment), PostgreSQL requires that a partitioned table's
-- primary key include the partition key column. That forces the primary
-- key on `checks` to become the composite `(id, checked_at)`, which in
-- turn means any foreign key referencing it must reference the whole
-- composite key, not just `id`. This migration adds the extra column and
-- swaps the FK now, ahead of time, so that a future partitioning
-- migration does not also need to touch every existing
-- `diagnostic_reports` row or this table's write path.
--
-- This migration does NOT partition `checks`. `checks` is untouched other
-- than gaining one new UNIQUE constraint, which has no behavioral effect
-- today since `id` was already a PRIMARY KEY (and therefore already
-- unique) on its own.

-- 1) Add the new column, nullable at first so the backfill can run.
ALTER TABLE diagnostic_reports
    ADD COLUMN check_checked_at TIMESTAMPTZ;

-- 2) Backfill every existing row from its referenced check.
UPDATE diagnostic_reports dr
    SET check_checked_at = c.checked_at
    FROM checks c
    WHERE dr.check_id = c.id;

-- 3) Now that every row has a value, enforce NOT NULL going forward.
ALTER TABLE diagnostic_reports
    ALTER COLUMN check_checked_at SET NOT NULL;

-- 4) checks.id is already unique (PRIMARY KEY). Add the explicit
--    composite UNIQUE constraint needed as an FK target, and which
--    already matches the shape of the composite primary key a future
--    partitioning migration would need to introduce.
ALTER TABLE checks
    ADD CONSTRAINT uq_checks_id_checked_at UNIQUE (id, checked_at);

-- 5) Drop the existing single-column FK on diagnostic_reports.check_id.
--    Looked up dynamically instead of hardcoding the constraint name, in
--    case it differs from PostgreSQL's default auto-generated name.
DO $$
DECLARE
    fk_name text;
BEGIN
    SELECT tc.constraint_name INTO fk_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = current_schema()
      AND tc.table_name = 'diagnostic_reports'
      AND tc.constraint_type = 'FOREIGN KEY'
      AND kcu.column_name = 'check_id'
    LIMIT 1;

    IF fk_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE diagnostic_reports DROP CONSTRAINT %I', fk_name);
    END IF;
END $$;

-- 6) Replace it with the composite FK.
ALTER TABLE diagnostic_reports
    ADD CONSTRAINT diagnostic_reports_check_id_checked_at_fkey
    FOREIGN KEY (check_id, check_checked_at)
    REFERENCES checks (id, checked_at)
    ON DELETE CASCADE;
