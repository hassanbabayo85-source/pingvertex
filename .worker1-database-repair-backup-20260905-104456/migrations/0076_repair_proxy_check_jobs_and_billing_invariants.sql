-- Repair migration for databases that already recorded 0019/0030.
-- Safe to run after 0075. All DDL and data normalization is transactional.

BEGIN;

ALTER TABLE proxy_check_jobs
    ADD COLUMN IF NOT EXISTS failed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT,
    ADD COLUMN IF NOT EXISTS successful_count INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS failed_count INT NOT NULL DEFAULT 0;

-- Existing jobs may predate the failure metadata. Normalize only the
-- metadata fields; the original proxy/job data remains untouched.
UPDATE proxy_check_jobs
SET completed_at = COALESCE(completed_at, now())
WHERE status = 'completed'
  AND completed_at IS NULL;

UPDATE proxy_check_jobs
SET failed_at = COALESCE(failed_at, now()),
    failure_reason = COALESCE(NULLIF(failure_reason, ''), 'proxy check job failed')
WHERE status = 'failed';

UPDATE proxy_check_jobs
SET completed_at = NULL
WHERE status IN ('pending', 'running');

UPDATE proxy_check_jobs
SET failed_at = NULL,
    failure_reason = NULL
WHERE status <> 'failed';

-- Recompute result counters from persisted proxy entry results. This does
-- not alter proxy results themselves and makes the new counters truthful.
UPDATE proxy_check_jobs j
SET checked_count = stats.checked_count,
    successful_count = stats.successful_count,
    failed_count = stats.failed_count
FROM (
    SELECT
        j2.id,
        COUNT(pe.id) FILTER (WHERE pe.result_json IS NOT NULL)::INT AS checked_count,
        COUNT(pe.id) FILTER (
            WHERE pe.result_json->>'reachable' = 'true'
        )::INT AS successful_count,
        COUNT(pe.id) FILTER (
            WHERE pe.result_json->>'reachable' = 'false'
        )::INT AS failed_count
    FROM proxy_check_jobs j2
    LEFT JOIN proxy_entries pe ON pe.job_id = j2.id
    GROUP BY j2.id
) stats
WHERE j.id = stats.id;

ALTER TABLE proxy_check_jobs
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_status,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_counts,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_completed_state,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_failed_state;

ALTER TABLE proxy_check_jobs
    ADD CONSTRAINT chk_proxy_check_jobs_status
        CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    ADD CONSTRAINT chk_proxy_check_jobs_counts
        CHECK (
            total_count >= 0
            AND checked_count >= 0
            AND successful_count >= 0
            AND failed_count >= 0
            AND checked_count <= total_count
            AND successful_count + failed_count <= checked_count
        ),
    ADD CONSTRAINT chk_proxy_check_jobs_completed_state
        CHECK ((status = 'completed') = (completed_at IS NOT NULL)),
    ADD CONSTRAINT chk_proxy_check_jobs_failed_state
        CHECK (
            (status = 'failed') = (failed_at IS NOT NULL AND failure_reason IS NOT NULL)
        );

ALTER TABLE proxy_check_jobs
    ALTER COLUMN tenant_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_proxy_check_jobs_tenant'
          AND conrelid = 'proxy_check_jobs'::regclass
    ) THEN
        ALTER TABLE proxy_check_jobs
            ADD CONSTRAINT fk_proxy_check_jobs_tenant
            FOREIGN KEY (tenant_id) REFERENCES tenants(id);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_proxy_check_jobs_tenant_user'
          AND conrelid = 'proxy_check_jobs'::regclass
    ) THEN
        ALTER TABLE proxy_check_jobs
            ADD CONSTRAINT fk_proxy_check_jobs_tenant_user
            FOREIGN KEY (tenant_id, user_id)
            REFERENCES tenant_memberships (tenant_id, user_id);
    END IF;
END;
$$;

CREATE INDEX IF NOT EXISTS proxy_check_jobs_tenant_idx
    ON proxy_check_jobs (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS proxy_check_jobs_status_idx
    ON proxy_check_jobs (status, created_at);

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

COMMIT;
