BEGIN;

-- ============================================================
-- PAYG BLUEPRINT PART 3 — Sections 20–34 hardening
-- Forward-only migration. Existing billing history is preserved.
-- ============================================================

-- 1. Durable bulk-import job state.
CREATE TABLE IF NOT EXISTS monitor_bulk_import_jobs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    actor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (
        status IN ('PENDING','VALIDATING','READY','RUNNING','COMPLETED','FAILED','CANCELLED')
    ),
    total_rows INTEGER NOT NULL DEFAULT 0 CHECK (total_rows >= 0),
    valid_rows INTEGER NOT NULL DEFAULT 0 CHECK (valid_rows >= 0),
    invalid_rows INTEGER NOT NULL DEFAULT 0 CHECK (invalid_rows >= 0),
    processed_rows INTEGER NOT NULL DEFAULT 0 CHECK (processed_rows >= 0),
    created_rows INTEGER NOT NULL DEFAULT 0 CHECK (created_rows >= 0),
    failed_rows INTEGER NOT NULL DEFAULT 0 CHECK (failed_rows >= 0),
    error_report JSONB,
    estimated_checks BIGINT NOT NULL DEFAULT 0 CHECK (estimated_checks >= 0),
    estimated_charge_units BIGINT NOT NULL DEFAULT 0 CHECK (estimated_charge_units >= 0),
    currency TEXT NOT NULL DEFAULT 'USD',
    batch_size INTEGER NOT NULL DEFAULT 500 CHECK (batch_size BETWEEN 1 AND 5000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_jobs_tenant_created
    ON monitor_bulk_import_jobs(tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_jobs_status
    ON monitor_bulk_import_jobs(status, updated_at);

CREATE TABLE IF NOT EXISTS monitor_bulk_import_rows (
    id BIGSERIAL PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES monitor_bulk_import_jobs(id) ON DELETE CASCADE,
    row_number INTEGER NOT NULL CHECK (row_number > 0),
    payload JSONB NOT NULL,
    validation_status TEXT NOT NULL CHECK (validation_status IN ('PENDING','VALID','INVALID','CREATED','FAILED')),
    validation_errors JSONB,
    monitor_id UUID REFERENCES monitors(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_monitor_bulk_import_row UNIQUE(job_id, row_number)
);

CREATE INDEX IF NOT EXISTS idx_monitor_bulk_import_rows_job_status
    ON monitor_bulk_import_rows(job_id, validation_status, row_number);

-- 2. Explicit customer spend limits. These are protection controls,
-- not subscription plans. NULL means unlimited.
ALTER TABLE wallet_settings
    ADD COLUMN IF NOT EXISTS daily_spend_limit_units BIGINT;

ALTER TABLE wallet_settings
    ADD COLUMN IF NOT EXISTS monthly_spend_limit_units BIGINT;

ALTER TABLE wallet_settings
    DROP CONSTRAINT IF EXISTS chk_wallet_daily_spend_limit;

ALTER TABLE wallet_settings
    ADD CONSTRAINT chk_wallet_daily_spend_limit
    CHECK (daily_spend_limit_units IS NULL OR daily_spend_limit_units > 0);

ALTER TABLE wallet_settings
    DROP CONSTRAINT IF EXISTS chk_wallet_monthly_spend_limit;

ALTER TABLE wallet_settings
    ADD CONSTRAINT chk_wallet_monthly_spend_limit
    CHECK (monthly_spend_limit_units IS NULL OR monthly_spend_limit_units > 0);

ALTER TABLE wallet_settings
    DROP CONSTRAINT IF EXISTS chk_wallet_monthly_spend_ge_daily;

ALTER TABLE wallet_settings
    ADD CONSTRAINT chk_wallet_monthly_spend_ge_daily
    CHECK (
        monthly_spend_limit_units IS NULL
        OR daily_spend_limit_units IS NULL
        OR monthly_spend_limit_units >= daily_spend_limit_units
    );

-- 3. Immutable audit history from the database perspective.
-- The application remains responsible for deciding what to record;
-- the database prevents UPDATE/DELETE of recorded audit entries.
CREATE OR REPLACE FUNCTION prevent_audit_log_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'audit logs are append-only';
END;
$$;

DROP TRIGGER IF EXISTS trg_audit_log_append_only ON audit_log;
CREATE TRIGGER trg_audit_log_append_only
BEFORE UPDATE OR DELETE ON audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();

DROP TRIGGER IF EXISTS trg_financial_audit_log_append_only ON financial_audit_log;
CREATE TRIGGER trg_financial_audit_log_append_only
BEFORE UPDATE OR DELETE ON financial_audit_log
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();

DROP TRIGGER IF EXISTS trg_pricing_audit_log_append_only ON pricing_audit_logs;
CREATE TRIGGER trg_pricing_audit_log_append_only
BEFORE UPDATE OR DELETE ON pricing_audit_logs
FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();

-- 4. Usage identity: retain monitor/execution references without breaking
-- historical rows. Existing events remain valid.
ALTER TABLE api_usage_events
    ADD COLUMN IF NOT EXISTS monitor_id UUID REFERENCES monitors(id) ON DELETE SET NULL;

ALTER TABLE api_usage_events
    ADD COLUMN IF NOT EXISTS execution_id UUID;

CREATE INDEX IF NOT EXISTS idx_api_usage_events_tenant_monitor_time
    ON api_usage_events(tenant_id, monitor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_api_usage_events_execution
    ON api_usage_events(execution_id)
    WHERE execution_id IS NOT NULL;

-- 5. Auto-top-up state must agree with the worker's PENDING/SUCCEEDED/FAILED
-- lifecycle. The old CHECK is replaced forward-only.
ALTER TABLE wallet_auto_top_up_attempts
    DROP CONSTRAINT IF EXISTS wallet_auto_top_up_attempts_status_check;

ALTER TABLE wallet_auto_top_up_attempts
    ADD CONSTRAINT wallet_auto_top_up_attempts_status_check
    CHECK (status IN ('PENDING','SUCCEEDED','FAILED'));

-- 6. Prevent negative/invalid bulk-job progress and impossible counts.
ALTER TABLE monitor_bulk_import_jobs
    DROP CONSTRAINT IF EXISTS chk_monitor_bulk_import_progress;

ALTER TABLE monitor_bulk_import_jobs
    ADD CONSTRAINT chk_monitor_bulk_import_progress
    CHECK (
        valid_rows <= total_rows
        AND invalid_rows <= total_rows
        AND processed_rows <= total_rows
        AND created_rows <= processed_rows
        AND failed_rows <= processed_rows
    );

COMMIT;
