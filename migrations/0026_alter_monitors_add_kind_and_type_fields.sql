ALTER TABLE monitors ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'http';
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS tcp_port SMALLINT;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS dns_record_type TEXT;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS dns_expected_value TEXT;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS heartbeat_token TEXT UNIQUE;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS heartbeat_interval_seconds INTEGER;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS heartbeat_grace_period_seconds INTEGER;
ALTER TABLE monitors ADD COLUMN IF NOT EXISTS last_heartbeat_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_monitors_heartbeat_token ON monitors (heartbeat_token) WHERE heartbeat_token IS NOT NULL;
