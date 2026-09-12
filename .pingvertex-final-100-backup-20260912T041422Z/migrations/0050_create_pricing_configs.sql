CREATE TABLE pricing_configs (
    id UUID PRIMARY KEY,
    version INTEGER NOT NULL UNIQUE,
    currency TEXT NOT NULL,
    cost_per_check NUMERIC(20,6) NOT NULL,
    effective_storage_bytes_per_check INTEGER NOT NULL,
    cost_per_gb_hot NUMERIC(20,6) NOT NULL,
    cost_per_gb_warm NUMERIC(20,6) NOT NULL,
    cost_per_gb_cold NUMERIC(20,6) NOT NULL,
    backup_multiplier NUMERIC(10,4) NOT NULL,
    markup_multiplier NUMERIC(10,4) NOT NULL,
    min_interval_seconds INTEGER NOT NULL,
    max_monitors BIGINT NOT NULL,
    max_retention_days INTEGER NOT NULL,
    min_paid_monthly_price NUMERIC(20,6) NOT NULL,
    free_monitors BIGINT NOT NULL,
    free_interval_seconds INTEGER NOT NULL,
    free_retention_days INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    effective_at TIMESTAMPTZ NOT NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL
);

-- Immutability: revoke UPDATE from PUBLIC
REVOKE UPDATE ON TABLE pricing_configs FROM PUBLIC;
GRANT INSERT, SELECT ON TABLE pricing_configs TO PUBLIC;

CREATE INDEX idx_pricing_configs_effective_at ON pricing_configs (effective_at);
