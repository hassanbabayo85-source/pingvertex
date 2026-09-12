BEGIN;

ALTER TABLE monitor_bulk_import_jobs
    ADD COLUMN IF NOT EXISTS skipped_rows INTEGER NOT NULL DEFAULT 0;

ALTER TABLE monitor_bulk_import_jobs
    DROP CONSTRAINT IF EXISTS monitor_bulk_import_jobs_status_check;

ALTER TABLE monitor_bulk_import_jobs
    ADD CONSTRAINT monitor_bulk_import_jobs_status_check
    CHECK (status IN ('PENDING','VALIDATING','READY','RUNNING','PAUSED','COMPLETED','FAILED','CANCELLED'));

ALTER TABLE monitor_bulk_import_jobs
    DROP CONSTRAINT IF EXISTS chk_monitor_bulk_import_skipped_rows;

ALTER TABLE monitor_bulk_import_jobs
    ADD CONSTRAINT chk_monitor_bulk_import_skipped_rows
    CHECK (skipped_rows >= 0);

ALTER TABLE monitor_bulk_import_rows
    DROP CONSTRAINT IF EXISTS monitor_bulk_import_rows_validation_status_check;

ALTER TABLE monitor_bulk_import_rows
    ADD CONSTRAINT monitor_bulk_import_rows_validation_status_check
    CHECK (validation_status IN ('PENDING','VALID','INVALID','PROCESSING','CREATED','FAILED','SKIPPED'));

COMMIT;
