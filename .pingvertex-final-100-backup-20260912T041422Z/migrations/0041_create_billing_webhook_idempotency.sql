-- B15 — Billing Webhook Idempotency.
--
-- Stores provider event ids that have already been seen. The unique
-- constraint is the idempotency mechanism: duplicate delivery attempts
-- fail to insert and are safely ignored by the application layer.
CREATE TABLE billing_webhook_idempotency (
    provider_event_id TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
