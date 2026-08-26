CREATE TABLE pricing_audit_logs (
    id UUID PRIMARY KEY,
    version INTEGER NOT NULL,
    field_name TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    changed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reason TEXT
);

CREATE INDEX idx_pricing_audit_logs_version ON pricing_audit_logs (version);
