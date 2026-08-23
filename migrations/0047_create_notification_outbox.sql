-- B25 — Notification Outbox (Reliability Layer).
--
-- Stores notifications that must be delivered reliably. The actual
-- channel senders are intentionally NOT implemented in B25; this table
-- gives later workers (B26/B27) a durable queue to replace the
-- stub sender's no-op behavior.

CREATE TABLE IF NOT EXISTS notification_outbox (
    id UUID PRIMARY KEY,
    channel_id UUID NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    event_kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'sent', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    retry_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processing_started_at TIMESTAMPTZ,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_notification_outbox_pending_retry
    ON notification_outbox (status, retry_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_notification_outbox_processing_started
    ON notification_outbox (status, processing_started_at)
    WHERE status = 'processing';
