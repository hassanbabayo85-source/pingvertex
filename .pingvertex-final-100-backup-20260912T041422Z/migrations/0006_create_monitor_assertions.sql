CREATE TABLE monitor_assertions (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL,
    position INTEGER NOT NULL,
    response_part TEXT NOT NULL,
    operator TEXT NOT NULL,
    target TEXT,
    expected_value TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT fk_monitor_assertions_monitor
        FOREIGN KEY (monitor_id)
        REFERENCES monitors(id)
        ON DELETE CASCADE
);

CREATE INDEX idx_monitor_assertions_monitor_id
    ON monitor_assertions(monitor_id);

CREATE INDEX idx_monitor_assertions_monitor_position
    ON monitor_assertions(monitor_id, position);
