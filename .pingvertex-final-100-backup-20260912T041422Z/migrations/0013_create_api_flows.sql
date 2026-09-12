CREATE TABLE api_flows (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_api_flows_monitor
        FOREIGN KEY (monitor_id)
        REFERENCES monitors(id)
        ON DELETE CASCADE
);

CREATE INDEX idx_api_flows_monitor_id
    ON api_flows(monitor_id);
