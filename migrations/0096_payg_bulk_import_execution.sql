BEGIN;

-- ============================================================
-- PAYG BLUEPRINT PART 3 — FIX-008
-- Durable confirm + async CSV bulk-import execution.
-- Forward-only and backwards-compatible with 0095.
-- ============================================================

ALTER TABLE monitor_bulk_import_jobs
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT,
    ADD COLUMN IF NOT EXISTS source_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS forecast_days INTEGER NOT NULL DEFAULT 30,
    ADD COLUMN IF NOT EXISTS pricing_snapshot JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS confirmed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS lease_owner UUID,
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_error TEXT;

ALTER TABLE monitor_bulk_import_jobs
    DROP CONSTRAINT IF EXISTS chk_monitor_bulk_import_forecast_days;

ALTER TABLE monitor_bulk_import_jobs
    ADD CONSTRAINT chk_monitor_bulk_import_forecast_days
    CHECK (forecast_days BETWEEN 1 AND 3650);

CREATE UNIQUE INDEX IF NOT EXISTS uq_monitor_bulk_import_jobs_tenant_idempotency
    ON monitor_bulk_import_jobs(tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- 0095 had no worker lease columns. No production worker could have owned
-- such RUNNING rows before this migration, so recover only lease-less legacy
-- rows into the READY queue.
UPDATE monitor_bulk_import_jobs
SET status = 'READY', updated_at = now()
WHERE status = 'RUNNING' AND lease_expires_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_jobs_claimable
    ON monitor_bulk_import_jobs(status, lease_expires_at, created_at);

ALTER TABLE monitor_bulk_import_rows
    ADD COLUMN IF NOT EXISTS planned_monitor_id UUID,
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS next_attempt_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_error TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_monitor_bulk_import_rows_planned_monitor
    ON monitor_bulk_import_rows(planned_monitor_id)
    WHERE planned_monitor_id IS NOT NULL;

ALTER TABLE monitor_bulk_import_rows
    DROP CONSTRAINT IF EXISTS monitor_bulk_import_rows_validation_status_check;

ALTER TABLE monitor_bulk_import_rows
    ADD CONSTRAINT monitor_bulk_import_rows_validation_status_check
    CHECK (validation_status IN ('PENDING','VALID','INVALID','PROCESSING','CREATED','FAILED'));

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_rows_claimable
    ON monitor_bulk_import_rows(job_id, validation_status, next_attempt_at, row_number);

ALTER TABLE monitor_bulk_import_jobs
    DROP CONSTRAINT IF EXISTS chk_monitor_bulk_import_attempt_count;

ALTER TABLE monitor_bulk_import_jobs
    ADD CONSTRAINT chk_monitor_bulk_import_attempt_count
    CHECK (attempt_count >= 0);

ALTER TABLE monitor_bulk_import_rows
    DROP CONSTRAINT IF EXISTS chk_monitor_bulk_import_row_attempt_count;

ALTER TABLE monitor_bulk_import_rows
    ADD CONSTRAINT chk_monitor_bulk_import_row_attempt_count
    CHECK (attempt_count >= 0);

COMMIT;
