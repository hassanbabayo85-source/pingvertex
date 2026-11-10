BEGIN;

-- Drop billing columns from monitor_bulk_import_jobs.
--
-- These three columns were added by migrations 0095 and 0096 for the
-- PAYG billing model:
--   estimated_charge_units (0095)  — pre-computed PAYG charge
--   currency               (0095)  — ISO currency for the charge
--   pricing_snapshot       (0096)  — frozen price book at confirm time
--
-- After Phases 1-3 removed the billing surface (and migration 0134
-- dropped every billing table), nothing reads or writes these columns
-- anymore. The DTOs, handlers, repository, and Leptos page that used
-- to carry them are being updated in the same commit.
--
-- Dependency audit (verified by grep against `migrations/`):
--   - No index on monitor_bulk_import_jobs references these columns.
--     Existing indexes only cover: tenant_id, status, id, idempotency_key,
--     lease_expires_at, created_at, updated_at.
--   - No view/trigger references these columns.
--   - The FK from monitor_bulk_import_rows.job_id references `id`,
--     not any of these.
--   - The CHECK constraint on estimated_charge_units is inline on the
--     column and is dropped automatically by DROP COLUMN in PostgreSQL.
--   - The DEFAULT on pricing_snapshot is dropped with the column.

ALTER TABLE monitor_bulk_import_jobs
    DROP COLUMN IF EXISTS estimated_charge_units,
    DROP COLUMN IF EXISTS currency,
    DROP COLUMN IF EXISTS pricing_snapshot;

COMMIT;
