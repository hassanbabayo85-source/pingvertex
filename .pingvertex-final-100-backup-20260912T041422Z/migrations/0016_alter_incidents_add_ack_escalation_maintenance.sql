ALTER TABLE incidents
    ADD COLUMN acknowledged_at TIMESTAMPTZ,
    ADD COLUMN acknowledged_by UUID,
    ADD COLUMN occurred_during_maintenance BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN escalation_step_index INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN last_escalation_at TIMESTAMPTZ;
