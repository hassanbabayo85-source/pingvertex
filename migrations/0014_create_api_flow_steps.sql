CREATE TABLE api_flow_steps (
    id UUID PRIMARY KEY,
    api_flow_id UUID NOT NULL,
    "order" SMALLINT NOT NULL,
    name TEXT NOT NULL,
    method TEXT NOT NULL,
    url_template TEXT NOT NULL,
    headers JSONB NOT NULL DEFAULT '[]'::jsonb,
    body TEXT,
    extract JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_api_flow_steps_api_flow
        FOREIGN KEY (api_flow_id)
        REFERENCES api_flows(id)
        ON DELETE CASCADE
);

CREATE INDEX idx_api_flow_steps_api_flow_id
    ON api_flow_steps(api_flow_id);

CREATE UNIQUE INDEX idx_api_flow_steps_api_flow_order
    ON api_flow_steps(api_flow_id, "order");

ALTER TABLE monitor_assertions ADD COLUMN api_flow_step_id UUID REFERENCES api_flow_steps(id) ON DELETE CASCADE;

CREATE INDEX idx_monitor_assertions_api_flow_step_id
    ON monitor_assertions(api_flow_step_id)
    WHERE api_flow_step_id IS NOT NULL;
