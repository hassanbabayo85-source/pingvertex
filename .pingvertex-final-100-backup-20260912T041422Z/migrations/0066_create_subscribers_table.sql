-- Persists the `subscribers` entity (crates/domain/src/entities/subscriber.rs)
-- and backs application::ports::SubscriberRepository. The domain model,
-- trait, and in-memory repository already existed; this table was the
-- missing piece — until now only InMemorySubscriberRepository existed,
-- so subscriber data was never actually persisted.
--
-- Per SEC-P0-3 (see migration 0065), every new monitor-child table must
-- carry its own tenant_id and be constrained with a composite
-- (monitor_id, tenant_id) foreign key against monitors(id, tenant_id) —
-- not just a plain monitor_id FK — so a subscriber row can never silently
-- point at a monitor belonging to a different tenant.
CREATE TABLE IF NOT EXISTS subscribers (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    email TEXT NOT NULL,
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_subscribers_monitor_email UNIQUE (monitor_id, email),

    CONSTRAINT fk_subscribers_monitor_tenant
        FOREIGN KEY (monitor_id, tenant_id)
        REFERENCES monitors (id, tenant_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_subscribers_monitor_id ON subscribers (monitor_id);
CREATE INDEX idx_subscribers_tenant_id ON subscribers (tenant_id);
