ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS next_due_at TIMESTAMPTZ NOT NULL DEFAULT now();

ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS lease_owner UUID;

ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ;

ALTER TABLE monitors
    ADD COLUMN IF NOT EXISTS lease_claimed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_monitors_next_due_at
    ON monitors (next_due_at)
    WHERE enabled = true;

CREATE INDEX IF NOT EXISTS idx_monitors_lease_expires_at
    ON monitors (lease_expires_at)
    WHERE lease_expires_at IS NOT NULL;
