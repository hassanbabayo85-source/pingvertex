CREATE TABLE IF NOT EXISTS proxy_check_jobs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    total_count INT NOT NULL,
    checked_count INT NOT NULL DEFAULT 0
);

-- Speeds up the per-user daily quota query (COUNT/SUM of jobs for a given
-- user within a time window), which filters on user_id and orders/filters
-- by created_at.
CREATE INDEX IF NOT EXISTS idx_proxy_check_jobs_user_id_created_at
    ON proxy_check_jobs (user_id, created_at);
