-- Proxy check jobs.
--
-- `tenants` is introduced by migration 0027, so this migration cannot add
-- the tenant foreign key yet without making a fresh 0001->latest migration
-- impossible.  We create the tenant_id column here (nullable during the
-- historical bootstrap) and migration 0030 performs the backfill and
-- promotes it to NOT NULL + FK.  Migration 0076 is the idempotent repair
-- for databases that already recorded migrations 0019/0030.
CREATE TABLE IF NOT EXISTS proxy_check_jobs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id UUID,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    failure_reason TEXT,
    total_count INT NOT NULL CHECK (total_count >= 0),
    checked_count INT NOT NULL DEFAULT 0 CHECK (checked_count >= 0),
    successful_count INT NOT NULL DEFAULT 0 CHECK (successful_count >= 0),
    failed_count INT NOT NULL DEFAULT 0 CHECK (failed_count >= 0),

    CONSTRAINT chk_proxy_check_jobs_status
        CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    CONSTRAINT chk_proxy_check_jobs_counts
        CHECK (
            checked_count <= total_count
            AND successful_count + failed_count <= checked_count
        ),
    CONSTRAINT chk_proxy_check_jobs_completed_state
        CHECK ((status = 'completed') = (completed_at IS NOT NULL)),
    CONSTRAINT chk_proxy_check_jobs_failed_state
        CHECK (
            (status = 'failed') = (failed_at IS NOT NULL AND failure_reason IS NOT NULL)
        )
);

-- State-machine enforcement is deliberately in the database as well as the
-- domain/repository layer.  This prevents a stale worker, ad-hoc SQL, or a
-- compromised application process from resurrecting terminal jobs.
CREATE OR REPLACE FUNCTION enforce_proxy_check_job_status_transition()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status
       AND NOT (
            (OLD.status = 'pending' AND NEW.status = 'running')
         OR (OLD.status = 'running' AND NEW.status IN ('completed', 'failed'))
       )
    THEN
        RAISE EXCEPTION
            'invalid proxy_check_jobs status transition: % -> %',
            OLD.status, NEW.status
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_proxy_check_jobs_status_transition
    ON proxy_check_jobs;

CREATE TRIGGER trg_proxy_check_jobs_status_transition
BEFORE UPDATE OF status ON proxy_check_jobs
FOR EACH ROW
EXECUTE FUNCTION enforce_proxy_check_job_status_transition();

CREATE INDEX IF NOT EXISTS proxy_check_jobs_user_created_idx
    ON proxy_check_jobs (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS proxy_check_jobs_tenant_idx
    ON proxy_check_jobs (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS proxy_check_jobs_status_idx
    ON proxy_check_jobs (status, created_at);
