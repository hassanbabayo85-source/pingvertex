-- Worker 2: proxy-check lease, cancellation, and tenant-safe persistence.
BEGIN;

ALTER TABLE proxy_check_jobs
    ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS heartbeat_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS worker_id TEXT,
    ADD COLUMN IF NOT EXISTS canceled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS retry_count INT NOT NULL DEFAULT 0;

-- Normalize legacy rows before constraints are added.
-- Normalize the legacy result shape (which used `error` and omitted the
-- proxy identity) so repository deserialization remains backward compatible.
UPDATE proxy_entries pe
SET result_json = (pe.result_json - 'error')
    || jsonb_build_object(
        'host', pe.host,
        'port', pe.port,
        'scheme', pe.scheme,
        'error_code', CASE
            WHEN pe.result_json->>'error' IS NULL THEN NULL
            ELSE 'INTERNAL_ERROR'
        END,
        'error_message', pe.result_json->'error'
    )
WHERE pe.result_json IS NOT NULL
  AND NOT (pe.result_json ? 'host');

UPDATE proxy_check_jobs
SET started_at = COALESCE(started_at, created_at),
    heartbeat_at = COALESCE(heartbeat_at, started_at, created_at)
WHERE status = 'running';

UPDATE proxy_check_jobs
SET canceled_at = COALESCE(canceled_at, now())
WHERE status = 'canceled';

UPDATE proxy_check_jobs
SET completed_at = NULL, failed_at = NULL, failure_reason = NULL,
    heartbeat_at = NULL, worker_id = NULL
WHERE status = 'canceled';

ALTER TABLE proxy_check_jobs
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_status,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_completed_state,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_failed_state,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_counts,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_lease_state,
    DROP CONSTRAINT IF EXISTS chk_proxy_check_jobs_canceled_state;

ALTER TABLE proxy_check_jobs
    ADD CONSTRAINT chk_proxy_check_jobs_status
        CHECK (status IN ('pending','running','completed','failed','canceled')),
    ADD CONSTRAINT chk_proxy_check_jobs_counts
        CHECK (
            total_count >= 0 AND checked_count >= 0
            AND successful_count >= 0 AND failed_count >= 0
            AND retry_count >= 0
            AND checked_count <= total_count
            AND successful_count + failed_count <= checked_count
        ),
    ADD CONSTRAINT chk_proxy_check_jobs_completed_state
        CHECK ((status = 'completed') = (completed_at IS NOT NULL)),
    ADD CONSTRAINT chk_proxy_check_jobs_failed_state
        CHECK ((status = 'failed') = (failed_at IS NOT NULL AND failure_reason IS NOT NULL)),
    ADD CONSTRAINT chk_proxy_check_jobs_canceled_state
        CHECK ((status = 'canceled') = (canceled_at IS NOT NULL)),
    ADD CONSTRAINT chk_proxy_check_jobs_lease_state
        CHECK (
            status <> 'running'
            OR (started_at IS NOT NULL AND heartbeat_at IS NOT NULL AND worker_id IS NOT NULL)
        );

CREATE INDEX IF NOT EXISTS proxy_check_jobs_claim_idx
    ON proxy_check_jobs (status, created_at, id);

CREATE INDEX IF NOT EXISTS proxy_check_jobs_heartbeat_idx
    ON proxy_check_jobs (status, heartbeat_at)
    WHERE status = 'running';

CREATE OR REPLACE FUNCTION enforce_proxy_check_job_status_transition()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status
       AND NOT (
            (OLD.status = 'pending' AND NEW.status IN ('running','canceled'))
         OR (OLD.status = 'running' AND NEW.status IN ('pending','completed','failed','canceled'))
       )
    THEN
        RAISE EXCEPTION
            'invalid proxy_check_jobs status transition: % -> %',
            OLD.status, NEW.status
            USING ERRCODE = 'check_violation';
    END IF;

    -- Terminal jobs are immutable with respect to state.
    IF OLD.status IN ('completed','failed','canceled') AND NEW.status <> OLD.status THEN
        RAISE EXCEPTION 'terminal proxy_check_jobs status cannot change'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_proxy_check_jobs_status_transition ON proxy_check_jobs;
CREATE TRIGGER trg_proxy_check_jobs_status_transition
BEFORE UPDATE OF status ON proxy_check_jobs
FOR EACH ROW EXECUTE FUNCTION enforce_proxy_check_job_status_transition();

COMMIT;
